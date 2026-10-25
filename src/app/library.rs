//! The media library window's behaviour, and the library behind it.
//!
//! The library lives in `library.db` beside the settings. Scans of the
//! watched folders and imports from Apple Music run on a worker thread, one
//! job at a time; their results come back here, to the UI thread, which
//! writes them to the database in batches and keeps the tracks in memory
//! for the views. While Lantern runs, the watched folders are watched too:
//! once changes there settle, they are rescanned.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

use lantern_library::import::Imported;
use lantern_library::scan::{self, Event};
use lantern_library::{Library, Playlist, Track, TrackInfo};

use super::{App, PaneKind};
use crate::ui::WidgetId;
use crate::ui::library_window;
use crate::ui::playlist_window::snap_size;

/// No smaller than the window's minimum, which leaves room for the tree
/// and the list.
pub(super) fn min_library_size((w, h): (u32, u32)) -> (u32, u32) {
    (
        w.max(library_window::MIN_WIDTH),
        h.max(library_window::MIN_HEIGHT),
    )
}

/// Changes in the watched folders must be quiet this long before a rescan,
/// so a copy of a whole album is one rescan, not dozens.
const SETTLE: Duration = Duration::from_secs(2);

/// While a scan runs, the views refresh at most this often.
const RELOAD_EVERY: Duration = Duration::from_secs(1);

enum Job {
    Scan {
        folders: Vec<PathBuf>,
        known: std::collections::HashMap<PathBuf, (u64, i64)>,
    },
    AppleMusic,
    Xml(PathBuf),
    /// Make a library playlist of these files, first reading the tags of
    /// any the library does not have.
    Save {
        name: String,
        paths: Vec<PathBuf>,
        known: std::collections::HashSet<PathBuf>,
    },
}

enum Done {
    Scan(Event),
    ScanFinished,
    Imported(Result<Imported, String>),
    Saved {
        name: String,
        paths: Vec<PathBuf>,
        new: Vec<TrackInfo>,
    },
}

/// What the library is busy with, for the status line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Busy {
    /// Walking the folders, before the count is known.
    Looking,
    /// Reading tags: done of total.
    Reading(usize, usize),
    Importing,
}

pub(crate) struct LibraryState {
    db: Option<Library>,
    /// Every track, as the views see it.
    pub(crate) tracks: Vec<Track>,
    pub(crate) playlists: Vec<Playlist>,
    jobs: Option<Sender<Job>>,
    results: Option<Receiver<Done>>,
    cancel: Arc<AtomicBool>,
    /// Jobs sent and not yet finished.
    queued: usize,
    busy: Option<Busy>,
    /// Scan results not yet written.
    pending: Vec<TrackInfo>,
    /// The database has changed since `tracks` was read.
    stale: bool,
    reloaded: Instant,
    /// The last thing worth telling the user, such as an import's outcome.
    message: Option<String>,
    /// Bumped whenever `tracks` or `playlists` change, so views know to
    /// rebuild.
    pub(crate) generation: u64,
    /// Nudges the event loop from other threads.
    wake: Option<Arc<dyn Fn() + Send + Sync>>,
    /// Watches the watched folders; replaced when they change.
    watch: Option<lantern_library::watch::FolderWatch>,
    /// The watch saw changes settle: rescan when free.
    changed: Arc<AtomicBool>,
    /// A playlist just saved from the playlist window, for the window to
    /// show and name.
    pub(crate) saved: Option<i64>,
}

impl LibraryState {
    /// Open `library.db` in `dir`, or carry on without a library if it
    /// cannot be opened.
    pub(crate) fn open(dir: Option<PathBuf>) -> Self {
        let db = dir.and_then(|d| {
            Library::open(&d.join("library.db"))
                .inspect_err(|e| eprintln!("the media library is unavailable: {e}"))
                .ok()
        });
        let mut state = Self {
            db,
            tracks: Vec::new(),
            playlists: Vec::new(),
            jobs: None,
            results: None,
            cancel: Arc::new(AtomicBool::new(false)),
            queued: 0,
            busy: None,
            pending: Vec::new(),
            stale: true,
            reloaded: Instant::now(),
            message: None,
            generation: 0,
            wake: None,
            watch: None,
            changed: Arc::new(AtomicBool::new(false)),
            saved: None,
        };
        state.reload();
        state
    }

    /// Start the worker. `wake` nudges the event loop when results arrive.
    pub(crate) fn start(&mut self, wake: impl Fn() + Send + Sync + 'static) {
        let wake: Arc<dyn Fn() + Send + Sync> = Arc::new(wake);
        self.wake = Some(Arc::clone(&wake));
        self.watch_folders();
        let (jobs, inbox) = channel::<Job>();
        let (outbox, results) = channel();
        let cancel = Arc::clone(&self.cancel);
        let spawned = std::thread::Builder::new()
            .name("lantern-library".into())
            .spawn(move || {
                let send = |d: Done| {
                    let ok = outbox.send(d).is_ok();
                    wake();
                    ok
                };
                for job in inbox {
                    let ok = match job {
                        Job::Scan { folders, known } => {
                            scan::scan(&folders, &known, &cancel, |e| {
                                send(Done::Scan(e));
                            });
                            send(Done::ScanFinished)
                        }
                        Job::AppleMusic => send(Done::Imported(read_apple_music())),
                        Job::Xml(file) => send(Done::Imported(
                            lantern_library::import::read_xml(&file).map_err(|e| e.to_string()),
                        )),
                        Job::Save { name, paths, known } => {
                            let new = paths
                                .iter()
                                .filter(|p| !known.contains(*p) && scan::is_audio(p) && p.is_file())
                                .map(|p| lantern_library::tags::read(p))
                                .collect();
                            send(Done::Saved { name, paths, new })
                        }
                    };
                    if !ok || cancel.load(Ordering::Relaxed) {
                        break;
                    }
                }
            });
        match spawned {
            Ok(_) => {
                self.jobs = Some(jobs);
                self.results = Some(results);
            }
            Err(e) => eprintln!("could not start the library worker: {e}"),
        }
    }

    /// Watch the watched folders as they now are.
    pub(crate) fn watch_folders(&mut self) {
        let Some(wake) = self.wake.clone() else {
            return;
        };
        let folders = self.folders();
        self.watch = None;
        if folders.is_empty() {
            return;
        }
        let changed = Arc::clone(&self.changed);
        match lantern_library::watch::FolderWatch::start(&folders, SETTLE, move || {
            changed.store(true, Ordering::Relaxed);
            wake();
        }) {
            Ok(w) => self.watch = Some(w),
            Err(e) => eprintln!("cannot watch the library's folders: {e}"),
        }
    }

    /// Stop any scan between files: the app is quitting.
    pub(crate) fn stop(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    fn send(&mut self, job: Job) -> bool {
        let sent = self.jobs.as_ref().is_some_and(|j| j.send(job).is_ok());
        if sent {
            self.queued += 1;
        }
        sent
    }

    fn reload(&mut self) {
        let Some(db) = &self.db else { return };
        match (db.tracks(), db.playlists()) {
            (Ok(tracks), Ok(playlists)) => {
                self.tracks = tracks;
                self.playlists = playlists;
            }
            (Err(e), _) | (_, Err(e)) => eprintln!("could not read the library: {e}"),
        }
        self.stale = false;
        self.reloaded = Instant::now();
        self.generation += 1;
    }

    fn flush(&mut self) {
        if self.pending.is_empty() {
            return;
        }
        if let Some(db) = &mut self.db
            && let Err(e) = db.upsert(&self.pending, lantern_library::now())
        {
            eprintln!("could not add to the library: {e}");
        }
        self.pending.clear();
        self.stale = true;
    }

    /// Take in whatever the worker has finished. True if anything changed
    /// that the window shows.
    fn poll(&mut self) -> bool {
        // The folders changed: rescan, once nothing else is under way.
        let mut moved = false;
        if self.busy.is_none() && self.changed.swap(false, Ordering::Relaxed) {
            self.rescan();
            moved = true;
        }
        let Some(results) = &self.results else {
            return moved;
        };
        let done: Vec<Done> = results.try_iter().collect();
        if done.is_empty() {
            return moved;
        }
        for d in done {
            match d {
                Done::Scan(Event::Gone(paths)) => {
                    if let Some(db) = &mut self.db
                        && let Err(e) = db.remove(&paths)
                    {
                        eprintln!("could not update the library: {e}");
                    }
                    self.stale = true;
                }
                Done::Scan(Event::Counted(total)) => self.busy = Some(Busy::Reading(0, total)),
                Done::Scan(Event::Found(info)) => {
                    if let Some(Busy::Reading(n, total)) = self.busy {
                        self.busy = Some(Busy::Reading(n + 1, total));
                    }
                    self.pending.push(info);
                }
                Done::ScanFinished => self.finish_job(),
                Done::Saved { name, paths, new } => {
                    if let Some(db) = &mut self.db {
                        let made = db
                            .upsert(&new, lantern_library::now())
                            .and_then(|()| db.ids_of(&paths))
                            .and_then(|ids| {
                                let id = db.create_playlist(&name)?;
                                db.set_playlist_tracks(id, &ids)?;
                                Ok(id)
                            });
                        match made {
                            Ok(id) => self.saved = Some(id),
                            Err(e) => eprintln!("could not save the playlist: {e}"),
                        }
                    }
                    self.stale = true;
                    self.finish_job();
                }
                Done::Imported(result) => {
                    self.message = Some(match result {
                        Ok(imported) => self.import(&imported),
                        Err(e) => format!("Import failed: {e}"),
                    });
                    self.finish_job();
                }
            }
        }
        self.flush();
        let idle = self.busy.is_none();
        if self.stale && (idle || self.reloaded.elapsed() >= RELOAD_EVERY) {
            self.reload();
        }
        true
    }

    fn finish_job(&mut self) {
        self.flush();
        self.queued = self.queued.saturating_sub(1);
        self.busy = (self.queued > 0).then_some(Busy::Looking);
    }

    fn import(&mut self, imported: &Imported) -> String {
        let Some(db) = &mut self.db else {
            return "No library to import into".into();
        };
        self.stale = true;
        match db.import(imported, lantern_library::now()) {
            Ok(s) => format!(
                "Imported {} tracks and {} playlists; {} passed over",
                s.tracks, s.playlists, s.skipped
            ),
            Err(e) => format!("Import failed: {e}"),
        }
    }

    pub(crate) fn folders(&self) -> Vec<PathBuf> {
        self.db
            .as_ref()
            .and_then(|db| db.folders().ok())
            .unwrap_or_default()
    }

    /// Look through the watched folders again for what changed.
    pub(crate) fn rescan(&mut self) {
        let Some(db) = &self.db else { return };
        let (Ok(folders), Ok(known)) = (db.folders(), db.fingerprints()) else {
            return;
        };
        if folders.is_empty() {
            return;
        }
        if self.send(Job::Scan { folders, known }) {
            self.busy = Some(Busy::Looking);
        }
    }

    /// What the library is busy with, or has to report; `None` when
    /// there is nothing to say.
    pub(crate) fn activity(&self) -> Option<String> {
        match self.busy {
            Some(Busy::Looking) => Some("Looking for music…".into()),
            Some(Busy::Reading(n, total)) => Some(format!("Reading tags: {n} of {total}")),
            Some(Busy::Importing) => Some("Importing…".into()),
            None if self.db.is_none() => Some("The library could not be opened".into()),
            None => self.message.clone(),
        }
    }

    pub(crate) fn clear_message(&mut self) {
        self.message = None;
    }

    pub(crate) fn playlist(&self, id: i64) -> Option<&Playlist> {
        self.playlists.iter().find(|p| p.id == id)
    }

    /// Whether the playlist was made here, and so can be changed; imported
    /// ones are replaced on the next import.
    pub(crate) fn is_own(&self, id: i64) -> bool {
        self.playlist(id)
            .is_some_and(|p| p.source == lantern_library::OWN)
    }

    /// A new playlist of these tracks; its id.
    pub(crate) fn create_playlist(&mut self, name: &str, tracks: &[i64]) -> Option<i64> {
        let db = self.db.as_mut()?;
        let made = db.create_playlist(name).and_then(|id| {
            db.set_playlist_tracks(id, tracks)?;
            Ok(id)
        });
        let id = made
            .inspect_err(|e| eprintln!("could not make a playlist: {e}"))
            .ok();
        self.reload();
        id
    }

    pub(crate) fn rename_playlist(&mut self, id: i64, name: &str) {
        if let Some(db) = &self.db
            && let Err(e) = db.rename_playlist(id, name)
        {
            eprintln!("could not rename the playlist: {e}");
        }
        self.reload();
    }

    pub(crate) fn delete_playlist(&mut self, id: i64) {
        if let Some(db) = &self.db
            && let Err(e) = db.delete_playlist(id)
        {
            eprintln!("could not delete the playlist: {e}");
        }
        self.reload();
    }

    pub(crate) fn set_playlist_tracks(&mut self, id: i64, tracks: &[i64]) {
        if let Some(db) = &mut self.db
            && let Err(e) = db.set_playlist_tracks(id, tracks)
        {
            eprintln!("could not change the playlist: {e}");
        }
        self.reload();
    }

    /// Save these files as a library playlist called `name`, adding any
    /// the library lacks. Done on the worker; `saved` says when.
    pub(crate) fn save_playlist(&mut self, name: String, paths: Vec<PathBuf>) {
        let known = self.tracks.iter().map(|t| t.info.path.clone()).collect();
        if self.send(Job::Save { name, paths, known }) {
            self.busy = Some(Busy::Looking);
        }
    }

    /// The file at `path` has new tags: read them again, if it is in the
    /// library.
    pub(crate) fn refresh_file(&mut self, path: &std::path::Path) {
        if !self.tracks.iter().any(|t| t.info.path == path) {
            return;
        }
        let info = lantern_library::tags::read(path);
        if let Some(db) = &mut self.db
            && let Err(e) = db.upsert(&[info], lantern_library::now())
        {
            eprintln!("could not update the library: {e}");
        }
        self.reload();
    }

    /// Rate these tracks, 0 (none) to 5 stars.
    pub(crate) fn rate(&mut self, ids: &[i64], stars: u8) {
        let Some(db) = &self.db else { return };
        for &id in ids {
            if let Err(e) = db.set_rating(id, stars) {
                eprintln!("could not rate a track: {e}");
                return;
            }
        }
        for t in self.tracks.iter_mut().filter(|t| ids.contains(&t.id)) {
            t.rating = stars.min(5);
        }
        self.generation += 1;
    }

    /// Take these files out of the library, leaving them on disk.
    pub(crate) fn forget(&mut self, paths: &[PathBuf]) {
        if let Some(db) = &mut self.db
            && let Err(e) = db.remove(paths)
        {
            eprintln!("could not remove from the library: {e}");
        }
        self.reload();
    }
}

#[cfg(target_os = "macos")]
fn read_apple_music() -> Result<Imported, String> {
    crate::platform::apple_music::read()
}

#[cfg(not(target_os = "macos"))]
fn read_apple_music() -> Result<Imported, String> {
    Err("reading Apple Music needs macOS; import its library XML file instead".into())
}

impl App {
    /// Show the library if hidden, hide it if showing — Winamp's Alt+L.
    pub(crate) fn toggle_library(&mut self) {
        let shown = self.pane(PaneKind::Library).is_some_and(|p| p.visible);
        self.set_visible(PaneKind::Library, !shown);
        if !shown && let Some(p) = self.pane(PaneKind::Library) {
            p.window.focus_window();
        }
    }

    /// Take in finished library work, and repaint if it shows.
    pub(super) fn poll_library(&mut self) {
        if self.library.poll() {
            self.show_saved_playlist();
            self.request_redraw(PaneKind::Library);
        }
    }

    /// A track began: if it is in the library, count the play.
    pub(super) fn library_played(&mut self, path: &std::path::Path) {
        let now = lantern_library::now();
        let Some(db) = &self.library.db else { return };
        match db.record_play(path, now) {
            Ok(true) => {
                if let Some(t) = self.library.tracks.iter_mut().find(|t| t.info.path == path) {
                    t.play_count += 1;
                    t.last_played = Some(now);
                }
                self.request_redraw(PaneKind::Library);
            }
            Ok(false) => {}
            Err(e) => eprintln!("could not count the play: {e}"),
        }
    }

    pub(crate) fn library_add_folder(&mut self) {
        let Some(dir) = rfd::FileDialog::new()
            .set_title("Add a folder to the library")
            .pick_folder()
        else {
            return;
        };
        if let Some(db) = &self.library.db
            && let Err(e) = db.add_folder(&dir)
        {
            eprintln!("could not watch {}: {e}", dir.display());
            return;
        }
        self.library.watch_folders();
        self.library.rescan();
        self.show_library();
    }

    pub(crate) fn library_remove_folder(&mut self, dir: &std::path::Path) {
        if let Some(db) = &mut self.library.db
            && let Err(e) = db.remove_folder(dir)
        {
            eprintln!("could not stop watching {}: {e}", dir.display());
        }
        self.library.watch_folders();
        self.library.reload();
        self.request_redraw(PaneKind::Library);
    }

    pub(crate) fn library_rescan(&mut self) {
        self.library.rescan();
        self.request_redraw(PaneKind::Library);
    }

    pub(crate) fn library_import_apple_music(&mut self) {
        if self.library.send(Job::AppleMusic) {
            self.library.busy = Some(Busy::Importing);
        }
        self.show_library();
    }

    pub(crate) fn library_import_xml(&mut self) {
        let Some(file) = rfd::FileDialog::new()
            .set_title("Import an Apple Music or iTunes library file")
            .add_filter("Library XML", &["xml"])
            .pick_file()
        else {
            return;
        };
        if self.library.send(Job::Xml(file)) {
            self.library.busy = Some(Busy::Importing);
        }
        self.show_library();
    }

    pub(super) fn show_library(&mut self) {
        if !self.pane(PaneKind::Library).is_some_and(|p| p.visible) {
            self.toggle_library();
        }
        self.request_redraw(PaneKind::Library);
    }

    /// The resize grip, dragged to logical `(x, y)`.
    pub(super) fn drag_library(&mut self, id: WidgetId, x: i32, y: i32) {
        if id != library_window::RESIZE {
            self.drag_library_part(id, x, y);
            return;
        }
        let (w, h) = self.library_size;
        // Keep the grip where the pointer took hold of it.
        let (dx, dy) = *self.lib_resize.get_or_insert((w as i32 - x, h as i32 - y));
        // The same grid as the playlist's, from a larger minimum.
        let size = min_library_size(snap_size(x + dx, y + dy));
        if size != self.library_size {
            self.resize_library(size);
        }
    }

    pub(super) fn release_library_slider(&mut self, id: WidgetId) {
        if id == library_window::RESIZE {
            self.lib_resize = None;
        } else {
            self.release_library_part(id);
        }
    }

    fn resize_library(&mut self, (w, h): (u32, u32)) {
        self.library_size = (w, h);
        self.apply_size(PaneKind::Library);
        // New geometry, same interaction state: the grip stays held. The
        // tree keeps its width if there is room.
        self.lib_ui.tree_width = library_window::clamp_tree_width(w, self.lib_ui.tree_width);
        self.relayout_library();
    }

    pub(super) fn activate_library(&mut self, id: WidgetId) {
        if id == library_window::CLOSE {
            self.set_visible(PaneKind::Library, false);
        } else {
            self.activate_library_button(id);
        }
    }
}
