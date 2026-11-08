//! Native file dialogs, and what they feed: files, folders, playlists,
//! skins, EQ presets.
//!
//! `rfd` runs the platform's own panel modally on the UI thread — on macOS
//! an `NSOpenPanel`. It must not run inside winit's event handler: AppKit
//! keeps delivering events during the panel's nested run loop, winit queues
//! them as run-loop blocks, and when one of those blocks runs while the
//! handler is still busy, winit panics ("tried to handle event while
//! another event is currently being handled") and the app aborts. So a
//! panel is shown from a block on the main dispatch queue, after the
//! handler has returned; what it chose is applied on the next turn of the
//! loop, which the proxy wakes.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use rfd::FileDialog;

use super::{App, PaneKind};
use crate::ui::font::TextFont;

/// What a panel's choice does to the app.
type Then = Box<dyn FnOnce(&mut App) + Send>;

/// Panels waiting to be shown or answered. Shared with the block that
/// shows them.
#[derive(Default)]
pub(crate) struct Dialogs {
    chosen: Arc<Mutex<Vec<Then>>>,
    /// A panel is open or about to be: a second request is dropped.
    open: Arc<AtomicBool>,
}

const AUDIO: &[&str] = &[
    "mp3", "mp2", "mp1", "flac", "ogg", "oga", "m4a", "m4b", "mp4", "aac", "wav", "aif", "aiff",
    "caf",
];
const PLAYLISTS: &[&str] = &["m3u", "m3u8", "pls", "cue"];

fn open_media() -> FileDialog {
    let everything: Vec<&str> = AUDIO.iter().chain(PLAYLISTS).copied().collect();
    FileDialog::new()
        .add_filter("Audio and playlists", &everything)
        .add_filter("Audio", AUDIO)
        .add_filter("Playlists", PLAYLISTS)
}
impl App {
    /// Show a panel with `show`, then hand what it chose to `then` — later,
    /// on a turn of the event loop. Cancelling does nothing.
    pub(crate) fn ask<T: Send + 'static>(
        &self,
        show: impl FnOnce() -> Option<T> + Send + 'static,
        then: impl FnOnce(&mut App, T) + Send + 'static,
    ) {
        if self.dialogs.open.swap(true, Ordering::AcqRel) {
            return;
        }
        let chosen = Arc::clone(&self.dialogs.chosen);
        let open = Arc::clone(&self.dialogs.open);
        let proxy = self.proxy.clone();
        let run = move || {
            if let Some(value) = show() {
                let then: Then = Box::new(move |app| then(app, value));
                chosen.lock().unwrap_or_else(|e| e.into_inner()).push(then);
            }
            open.store(false, Ordering::Release);
            if let Some(proxy) = proxy {
                let _ = proxy.send_event(());
            }
        };
        #[cfg(target_os = "macos")]
        dispatch2::DispatchQueue::main().exec_async(run);
        #[cfg(not(target_os = "macos"))]
        run();
    }

    /// Apply whatever the panels chose since the last turn.
    pub(crate) fn drain_dialogs(&mut self) {
        let chosen = std::mem::take(
            &mut *self
                .dialogs
                .chosen
                .lock()
                .unwrap_or_else(|e| e.into_inner()),
        );
        for then in chosen {
            then(self);
        }
    }

    /// Eject: choose files and play them, replacing the playlist — what
    /// Winamp's eject button does.
    pub(crate) fn eject(&mut self) {
        self.ask(
            || open_media().set_title("Open files").pick_files(),
            |app, files| app.play_paths(files),
        );
    }

    /// Replace the playlist with `paths` and play from the top.
    pub(crate) fn play_paths(&mut self, paths: impl IntoIterator<Item = PathBuf>) {
        self.playlist.clear();
        self.pl_scroll = 0;
        self.add_paths(paths);
        self.play_entry(0);
    }

    pub(crate) fn add_files_dialog(&mut self) {
        self.ask(
            || open_media().set_title("Add files").pick_files(),
            |app, files| app.add_paths(files),
        );
    }

    pub(crate) fn add_folder_dialog(&mut self) {
        self.ask(
            || FileDialog::new().set_title("Add folder").pick_folder(),
            |app, dir| app.add_paths([dir]),
        );
    }

    /// Replace the playlist with a saved one.
    pub(crate) fn load_playlist_dialog(&mut self) {
        self.ask(
            || {
                FileDialog::new()
                    .set_title("Load playlist")
                    .add_filter("Playlists", PLAYLISTS)
                    .pick_file()
            },
            |app, file| {
                app.playlist.clear();
                app.pl_scroll = 0;
                app.add_paths([file]);
            },
        );
    }

    pub(crate) fn save_playlist_dialog(&mut self) {
        self.ask(
            || {
                FileDialog::new()
                    .set_title("Save playlist")
                    .add_filter("M3U8 playlist", &["m3u8"])
                    .set_file_name("playlist.m3u8")
                    .save_file()
            },
            |app, mut file| {
                if file.extension().is_none() {
                    file.set_extension("m3u8");
                }
                let text = lantern_playlist::m3u::write_m3u8(app.playlist.entries());
                if let Err(e) = std::fs::write(&file, text) {
                    eprintln!("could not save {}: {e}", file.display());
                }
            },
        );
    }

    pub(crate) fn load_skin_dialog(&mut self) {
        self.ask(
            || {
                FileDialog::new()
                    .set_title("Load skin")
                    .add_filter("Winamp classic skins", &["wsz", "zip"])
                    .pick_file()
            },
            |app, file| app.load_skin_file(file),
        );
    }

    /// Back to the built-in skin.
    pub(crate) fn use_default_skin(&mut self) {
        if let Some(skin) = super::default_skin() {
            self.apply_skin(skin, None);
        }
    }

    /// Switch skins live: sheets, colours, the playlist font and the window
    /// shape all change together. A skin that fails to parse changes
    /// nothing.
    pub(crate) fn load_skin_file(&mut self, path: PathBuf) {
        let parsed = std::fs::read(&path)
            .map_err(|e| e.to_string())
            .and_then(|b| lantern_skin::parse(&b).map_err(|e| e.to_string()));
        match parsed {
            Ok(skin) => self.apply_skin(skin, Some(super::playlist::absolute(path))),
            Err(e) => eprintln!("cannot load skin {}: {e}", path.display()),
        }
    }

    /// `path` is where it came from, remembered for next time; `None` for
    /// the built-in skin.
    fn apply_skin(&mut self, skin: lantern_skin::Skin, path: Option<PathBuf>) {
        if let Some(font) = TextFont::load(&skin.playlist_style.font) {
            self.font = Some(font);
        }
        self.skin = Some(skin);
        self.skin_path = path;
        self.apply_region();
        for kind in [
            PaneKind::Main,
            PaneKind::Equalizer,
            PaneKind::Playlist,
            PaneKind::Library,
        ] {
            self.request_redraw(kind);
        }
    }
}
