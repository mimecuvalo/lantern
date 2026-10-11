//! The playlist window's behaviour, and playback driven by the playlist.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use lantern_audio::engine::{Command, Engine, Source, TrackInfo};
use lantern_playlist::{Entry, Select};
use winit::event::MouseScrollDelta;
use winit::keyboard::{Key, NamedKey};

use super::{App, PaneKind};
use crate::ui::main_window::Playback;
use crate::ui::playlist_window::{self, OpenMenu, PlaylistView, ROW_HEIGHT, Row, clock};
use crate::ui::{WidgetId, layout};
use lantern_skin::{PlaylistButton, PlaylistMenu};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SortBy {
    Title,
    FileName,
    Path,
    Reverse,
    Randomize,
}

/// Two clicks on one row within this are a double-click.
const DOUBLE_CLICK: Duration = Duration::from_millis(400);

/// What symphonia is built to decode, for adding folders.
const AUDIO_EXTENSIONS: &[&str] = &[
    "mp3", "mp2", "mp1", "flac", "ogg", "oga", "m4a", "m4b", "mp4", "aac", "wav", "aif", "aiff",
    "caf",
];

/// `path` made absolute against the working directory, without resolving
/// symlinks (which would show surprising names in the playlist).
pub(crate) fn absolute(path: PathBuf) -> PathBuf {
    std::path::absolute(&path).unwrap_or(path)
}

fn is_audio(path: &Path) -> bool {
    path.extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .is_some_and(|e| AUDIO_EXTENSIONS.contains(&e.as_str()))
}

/// Every audio file under `dir`, sorted by path so albums play in order.
fn audio_files_in(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(read) = std::fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<PathBuf> = read.filter_map(|e| e.ok().map(|e| e.path())).collect();
    paths.sort();
    for p in paths {
        if p.is_dir() {
            audio_files_in(&p, out);
        } else if is_audio(&p) {
            out.push(p);
        }
    }
}

impl App {
    // -- Filling the playlist ----------------------------------------------

    /// Add entries as they are, asking for tags where none are loaded.
    pub(crate) fn add_entries(&mut self, entries: Vec<Entry>) {
        let mut asked = std::collections::HashSet::new();
        for e in &entries {
            if !e.tags_loaded && asked.insert(e.path.clone()) {
                self.tags.request(e.path.clone());
            }
        }
        self.playlist.add(entries);
        self.requeue();
        self.request_redraw(PaneKind::Playlist);
    }

    /// Add files, folders, playlists and cue sheets, in order.
    pub(crate) fn add_paths(&mut self, paths: impl IntoIterator<Item = PathBuf>) {
        let mut entries: Vec<Entry> = Vec::new();
        // Absolute from here on: the playlist is saved and reloaded from
        // another directory, where a relative path would mean nothing.
        for path in paths.into_iter().map(absolute) {
            if path.is_dir() {
                let mut files = Vec::new();
                audio_files_in(&path, &mut files);
                entries.extend(files.into_iter().map(Entry::new));
                continue;
            }
            match lantern_playlist::load(&path) {
                Ok(Ok(loaded)) => entries.extend(loaded),
                Ok(Err(e)) => eprintln!("{}: {e}", path.display()),
                Err(e) => eprintln!("cannot read {}: {e}", path.display()),
            }
        }
        self.add_entries(entries);
    }

    /// Fold in tags read in the background.
    pub(super) fn apply_tags(&mut self) {
        let results = self.tags.poll();
        if results.is_empty() {
            return;
        }
        for (path, tags) in results {
            for i in 0..self.playlist.len() {
                let Some(e) = self.playlist.get_mut(i) else {
                    continue;
                };
                if e.path != path {
                    continue;
                }
                e.tags_loaded = true;
                match e.segment {
                    // A cue track keeps the sheet's title; only a last track,
                    // which plays to the end, needs the file's length.
                    Some(seg) => {
                        if seg.end.is_none() {
                            e.duration = tags.duration_secs.map(|d| (d - seg.start).max(0.0));
                        }
                    }
                    None => {
                        if tags.title.is_some() {
                            e.title.clone_from(&tags.title);
                        }
                        if tags.duration_secs.is_some() {
                            e.duration = tags.duration_secs;
                        }
                    }
                }
            }
        }
        self.request_redraw(PaneKind::Playlist);
        self.request_redraw(PaneKind::Main);
    }

    fn source(&self, i: usize) -> Option<Source> {
        let e = self.playlist.get(i)?;
        Some(Source {
            path: e.path.clone(),
            start_secs: e.segment.map_or(0.0, |s| s.start),
            end_secs: e.segment.and_then(|s| s.end),
        })
    }

    // -- Playback through the playlist ---------------------------------------

    fn shuffle_on(&self) -> bool {
        self.pane(PaneKind::Main)
            .and_then(|p| p.widgets.iter().find(|w| w.id == layout::SHUFFLE))
            .is_some_and(|w| w.active)
    }

    fn repeat_on(&self) -> bool {
        self.pane(PaneKind::Main)
            .and_then(|p| p.widgets.iter().find(|w| w.id == layout::REPEAT))
            .is_some_and(|w| w.active)
    }

    /// A small xorshift: shuffle needs variety, not cryptography.
    fn random_below(&mut self, n: usize) -> usize {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.rng = x;
        (x % n.max(1) as u64) as usize
    }

    /// What should play after the current entry.
    fn following(&mut self) -> Option<usize> {
        let repeat = self.repeat_on();
        if self.shuffle_on() {
            let n = self.playlist.len();
            let pick = self.random_below(n.saturating_sub(1).max(1));
            let mut rng = move |_| pick;
            self.playlist.next(repeat, Some(&mut rng))
        } else {
            self.playlist.next(repeat, None)
        }
    }

    /// Tell the engine what follows the current entry, so it can join it
    /// without a gap. Called whenever that may have changed.
    pub(crate) fn requeue(&mut self) {
        if self.playback() == Playback::Stopped {
            return;
        }
        let next = self.following();
        self.queued = next;
        match next.and_then(|i| self.source(i)) {
            Some(source) => self.send(Command::Enqueue(source)),
            None => self.send(Command::ClearQueue),
        }
    }

    /// Start entry `i` from its beginning.
    pub(crate) fn play_entry(&mut self, i: usize) {
        let Some(source) = self.source(i) else { return };
        self.playlist.set_current(Some(i));
        self.send(Command::Open(source));
        self.send(Command::Play);
        self.request_redraw(PaneKind::Playlist);
    }

    /// Previous (-1) or next (+1). Playing carries on into the new entry;
    /// stopped just moves the cursor, as in Winamp.
    pub(crate) fn skip(&mut self, forward: bool) {
        let target = if forward {
            self.following()
        } else {
            self.playlist.previous(self.repeat_on())
        };
        let Some(i) = target else { return };
        if self.playback() == Playback::Stopped {
            self.playlist.set_current(Some(i));
            self.request_redraw(PaneKind::Playlist);
        } else {
            self.play_entry(i);
        }
    }

    /// The engine opened or crossed into `info`: find its entry.
    pub(super) fn track_started(&mut self, info: &TrackInfo) {
        let matches = |i: usize| {
            self.playlist.get(i).is_some_and(|e| {
                e.path == info.path
                    && (e.segment.map_or(0.0, |s| s.start) - info.start_secs).abs() < 1e-3
            })
        };
        // Prefer the entry we queued or opened, when a file appears twice.
        let found = [self.queued, self.playlist.current()]
            .into_iter()
            .flatten()
            .find(|&i| matches(i))
            .or_else(|| (0..self.playlist.len()).find(|&i| matches(i)));
        if let Some(i) = found {
            self.playlist.set_current(Some(i));
            self.scroll_into_view(i);
        }
        self.request_redraw(PaneKind::Playlist);
    }

    /// The shaded playlist's title and time: the current track, as
    /// "N. Title" and its length.
    pub(super) fn playlist_shade_text(&self) -> (String, String) {
        let Some(i) = self.playlist.current() else {
            return ("[No file]".into(), String::new());
        };
        let Some(e) = self.playlist.get(i) else {
            return ("[No file]".into(), String::new());
        };
        (
            format!("{}. {}", i + 1, e.display_title()),
            e.duration.map(clock).unwrap_or_default(),
        )
    }

    /// Marquee text for the current track: "N. Title (m:ss)".
    pub(super) fn current_track_text(&self, info: Option<&TrackInfo>) -> Option<String> {
        let i = self.playlist.current()?;
        let e = self.playlist.get(i)?;
        let title = e
            .title
            .clone()
            .or_else(|| info.map(|i| i.title.clone()))
            .unwrap_or_else(|| e.display_title());
        let duration = e.duration.or(info.and_then(|i| i.duration_secs));
        Some(crate::ui::marquee::track_text(i + 1, &title, duration))
    }

    // -- The window -------------------------------------------------------------

    fn rows_in_view(&self) -> usize {
        playlist_window::visible_rows(self.playlist_size.0, self.playlist_size.1)
    }

    fn max_scroll(&self) -> usize {
        self.playlist.len().saturating_sub(self.rows_in_view())
    }

    fn scroll_to(&mut self, first: usize) {
        self.pl_scroll = first.min(self.max_scroll());
        self.request_redraw(PaneKind::Playlist);
    }

    fn scroll_into_view(&mut self, i: usize) {
        let rows = self.rows_in_view().max(1);
        if i < self.pl_scroll {
            self.scroll_to(i);
        } else if i >= self.pl_scroll + rows {
            self.scroll_to(i + 1 - rows);
        }
    }

    pub(super) fn playlist_view(&self) -> PlaylistView {
        let (width, height) = self.playlist_size;
        let rows = self
            .playlist
            .entries()
            .iter()
            .enumerate()
            .skip(self.pl_scroll)
            .take(self.rows_in_view() + 1)
            .map(|(i, e)| Row {
                number: i + 1,
                title: e.display_title(),
                duration: e.duration.map(clock).unwrap_or_default(),
                selected: e.selected,
                current: self.playlist.current() == Some(i),
            })
            .collect();
        let max = self.max_scroll();
        let (total, complete) = self.playlist.total_duration();
        let running_time = format!(
            "{}/{}{}",
            clock(self.playlist.selected_duration()),
            clock(total),
            if complete { "" } else { "+" }
        );
        let mini_time = match self.playback() {
            Playback::Stopped => String::new(),
            _ => {
                let elapsed = self.engine.as_ref().map_or(0.0, Engine::position_secs);
                clock(elapsed)
            }
        };
        PlaylistView {
            focused: self.pane(PaneKind::Playlist).is_some_and(|p| p.focused),
            width,
            height,
            rows,
            scroll: if max == 0 {
                0.0
            } else {
                self.pl_scroll as f32 / max as f32
            },
            running_time,
            mini_time,
            menu: self.pl_menu,
        }
    }

    /// Which item of the open menu is at `(x, y)`.
    fn menu_item_at(&self, x: i32, y: i32) -> Option<usize> {
        let open = self.pl_menu?;
        let (w, h) = self.playlist_size;
        playlist_window::menu_item_rects(open.menu, w, h)
            .iter()
            .position(|r| r.contains(x, y))
    }

    pub(super) fn playlist_menu_hover(&mut self, x: i32, y: i32) {
        let Some(open) = self.pl_menu else { return };
        let hover = self.menu_item_at(x, y);
        if hover != open.hover {
            self.pl_menu_fresh = false;
            self.pl_menu = Some(OpenMenu { hover, ..open });
            self.request_redraw(PaneKind::Playlist);
        }
    }

    fn menu_button_at(&self, x: i32, y: i32) -> Option<PlaylistMenu> {
        self.pane(PaneKind::Playlist)
            .and_then(|p| crate::ui::hit_test(&p.widgets, x, y))
            .and_then(playlist_window::menu_for)
    }

    /// Menus take the press first. A press on a menu button opens its menu
    /// (or closes it, if open); a press on an item waits for the release;
    /// any other press closes an open menu before doing its own thing.
    /// Returns true if the press was used up.
    pub(super) fn playlist_menu_press(&mut self, x: i32, y: i32) -> bool {
        if self.pl_menu.is_some() && self.menu_item_at(x, y).is_some() {
            return true;
        }
        let was_open = self.pl_menu.take().map(|m| m.menu);
        let Some(menu) = self.menu_button_at(x, y) else {
            if was_open.is_some() {
                self.request_redraw(PaneKind::Playlist);
            }
            return false;
        };
        if was_open != Some(menu) {
            self.pl_menu = Some(OpenMenu { menu, hover: None });
            // The menu's bottom item covers its button, so the pointer is
            // already over it; the release that ends this click must not
            // choose it.
            let hover = self.menu_item_at(x, y);
            self.pl_menu = Some(OpenMenu { menu, hover });
            self.pl_menu_fresh = true;
        }
        self.request_redraw(PaneKind::Playlist);
        true
    }

    /// Items run on release, as in Winamp: press a menu button, slide to an
    /// item, let go — or click the button, then click the item. Releasing
    /// on the button leaves the menu open; anywhere else closes it. Running
    /// on release also matters on macOS, where a modal file panel opened
    /// during a mouse-down cancels itself at once. Returns true if the
    /// release was used up.
    pub(super) fn playlist_menu_release(&mut self, x: i32, y: i32) -> bool {
        let Some(open) = self.pl_menu else {
            return false;
        };
        if std::mem::take(&mut self.pl_menu_fresh) {
            // Released without leaving the item under the button: a click
            // that opened the menu. Keep it open for the next click.
            return true;
        }
        if let Some(i) = self.menu_item_at(x, y) {
            self.pl_menu = None;
            // Paint the menu away now: the item may open something modal,
            // which would hold off a requested redraw until it closes.
            self.render(PaneKind::Playlist);
            self.run_menu_item(playlist_window::menu_items(open.menu)[i]);
        } else if self.menu_button_at(x, y) != Some(open.menu) {
            self.close_playlist_menu();
        }
        true
    }

    pub(super) fn close_playlist_menu(&mut self) {
        if self.pl_menu.take().is_some() {
            self.request_redraw(PaneKind::Playlist);
        }
    }

    fn run_menu_item(&mut self, item: PlaylistButton) {
        use PlaylistButton as B;
        match item {
            B::AddUrl => eprintln!("streams are not supported"),
            B::AddDir => self.add_folder_dialog(),
            B::AddFile => self.add_files_dialog(),
            B::RemoveMisc => self.playlist.remove_missing(|p| p.exists()),
            B::RemoveAll | B::NewList => self.playlist.clear(),
            B::Crop => self.playlist.crop(),
            B::RemoveSelected => self.playlist.remove_selected(),
            B::InvertSelection => self.playlist.invert_selection(),
            B::SelectZero => self.playlist.select_all(false),
            B::SelectAll => self.playlist.select_all(true),
            B::SortList => self.show_sort_menu(),
            B::FileInfo => self.show_file_info(),
            B::MiscOptions => self.show_misc_menu(),
            B::SaveList => self.save_playlist_dialog(),
            B::LoadList => self.load_playlist_dialog(),
        }
        self.scroll_to(self.pl_scroll);
        self.requeue();
        self.request_redraw(PaneKind::Playlist);
    }

    /// The selected entry's facts, in a native message box — Winamp's file
    /// info dialog was a tag editor, which is out of scope.
    pub(super) fn show_file_info(&self) {
        let Some(e) = self
            .playlist
            .entries()
            .iter()
            .find(|e| e.selected)
            .or_else(|| self.playlist.current().and_then(|i| self.playlist.get(i)))
        else {
            return;
        };
        let mut text = format!("{}\n\n{}", e.display_title(), e.path.display());
        if let Some(d) = e.duration {
            text.push_str(&format!("\n\nLength: {}", clock(d)));
        }
        if let Some(seg) = e.segment {
            text.push_str(&format!("\nCue track from {}", clock(seg.start)));
        }
        rfd::MessageDialog::new()
            .set_title("File info")
            .set_description(text)
            .show();
    }

    fn show_sort_menu(&mut self) {
        self.show_menu(super::menus::MenuKind::Sort, PaneKind::Playlist);
    }

    fn show_misc_menu(&mut self) {
        self.show_menu(super::menus::MenuKind::Misc, PaneKind::Playlist);
    }

    pub(super) fn sort_playlist(&mut self, how: SortBy) {
        match how {
            SortBy::Title => self
                .playlist
                .sort_by_key(|e| e.display_title().to_lowercase()),
            SortBy::FileName => self.playlist.sort_by_key(|e| {
                e.path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_lowercase())
            }),
            SortBy::Path => self.playlist.sort_by_key(|e| e.path.clone()),
            SortBy::Reverse => self.playlist.reverse(),
            SortBy::Randomize => {
                let mut picks = Vec::with_capacity(self.playlist.len());
                for i in (1..self.playlist.len()).rev() {
                    picks.push(self.random_below(i + 1));
                }
                let mut picks = picks.into_iter();
                self.playlist.randomize(|_| picks.next().unwrap_or(0));
            }
        }
        self.requeue();
        self.request_redraw(PaneKind::Playlist);
    }

    /// Mouse down in the playlist: rows select on press, not release.
    pub(super) fn press_playlist(&mut self, id: WidgetId, y: i32) {
        if id != playlist_window::LIST {
            return;
        }
        let list = playlist_window::list_area(self.playlist_size.0, self.playlist_size.1);
        let row = self.pl_scroll + ((y - list.y).max(0) as u32 / ROW_HEIGHT) as usize;
        if row >= self.playlist.len() {
            self.playlist.select_all(false);
            self.request_redraw(PaneKind::Playlist);
            return;
        }
        let now = Instant::now();
        let double = self
            .pl_last_click
            .is_some_and(|(t, r)| r == row && now.duration_since(t) < DOUBLE_CLICK);
        self.pl_last_click = Some((now, row));
        if double {
            self.play_entry(row);
            return;
        }
        let how = if self.modifiers.shift_key() {
            Select::Extend
        } else if self.modifiers.super_key() || self.modifiers.control_key() {
            Select::Toggle
        } else {
            Select::Only
        };
        self.playlist.select(row, how);
        self.request_redraw(PaneKind::Playlist);
    }

    /// The scrollbar or the resize grip, dragged to logical `(x, y)`.
    pub(super) fn drag_playlist(&mut self, id: WidgetId, x: i32, y: i32) {
        match id {
            playlist_window::SCROLLBAR => {
                let f = playlist_window::scroll_from_y(y - 20, self.playlist_size.1);
                self.scroll_to((f * self.max_scroll() as f32).round() as usize);
            }
            playlist_window::RESIZE => {
                let (w, h) = self.playlist_size;
                let shaded = self.pane(PaneKind::Playlist).is_some_and(|p| p.shaded);
                let shown_h = if shaded { 14 } else { h as i32 };
                // Keep the grip where the pointer took hold of it.
                let (dx, dy) = *self.pl_resize.get_or_insert((w as i32 - x, shown_h - y));
                let mut size = playlist_window::snap_size(x + dx, y + dy);
                // Shaded, it resizes sideways only.
                if shaded {
                    size.1 = h;
                }
                if size != self.playlist_size {
                    self.resize_playlist(size);
                }
            }
            _ => {}
        }
    }

    pub(super) fn release_playlist_slider(&mut self, id: WidgetId) {
        if id == playlist_window::RESIZE {
            self.pl_resize = None;
        }
    }

    fn resize_playlist(&mut self, (w, h): (u32, u32)) {
        self.playlist_size = (w, h);
        self.apply_size(PaneKind::Playlist);
        if let Some(p) = self.pane_mut(PaneKind::Playlist) {
            // New geometry, same interaction state: the grip stays held.
            let held = p.input.active;
            p.widgets = if p.shaded {
                playlist_window::shade_widgets(w)
            } else {
                playlist_window::widgets(w, h)
            };
            if let Some(held) = held
                && let Some(widget) = p.widgets.iter_mut().find(|x| x.id == held)
            {
                widget.pressed = true;
            }
        }
        self.scroll_to(self.pl_scroll);
    }

    pub(super) fn activate_playlist(&mut self, id: WidgetId) {
        use playlist_window as pw;
        match id {
            pw::CLOSE => self.set_visible(PaneKind::Playlist, false),
            pw::SHADE => self.toggle_shade(PaneKind::Playlist),
            pw::MINI_PREVIOUS => self.skip(false),
            pw::MINI_PLAY => self.play(),
            pw::MINI_PAUSE => self.toggle_pause(),
            pw::MINI_STOP => self.send(Command::Stop),
            pw::MINI_NEXT => self.skip(true),
            pw::SCROLL_UP => self.scroll_to(self.pl_scroll.saturating_sub(self.rows_in_view())),
            pw::SCROLL_DOWN => self.scroll_to(self.pl_scroll + self.rows_in_view()),
            pw::MINI_EJECT => self.eject(),
            _ => {}
        }
    }

    pub(super) fn wheel_playlist(&mut self, delta: MouseScrollDelta) {
        let lines = match delta {
            MouseScrollDelta::LineDelta(_, y) => -y.round() as i64 * 3,
            MouseScrollDelta::PixelDelta(p) => -(p.y
                / f64::from(ROW_HEIGHT)
                / self
                    .pane(PaneKind::Playlist)
                    .map_or(1.0, |p| p.window.scale_factor()))
            .round() as i64,
        };
        let first = (self.pl_scroll as i64 + lines).max(0) as usize;
        self.scroll_to(first);
    }

    /// Keys while the playlist has focus. Returns false for keys it does not
    /// handle, which then fall through to the global ones.
    pub(super) fn playlist_key(&mut self, key: &Key) -> bool {
        let cursor = (0..self.playlist.len())
            .rev()
            .find(|&i| self.playlist.get(i).is_some_and(|e| e.selected));
        match key {
            Key::Named(NamedKey::ArrowUp | NamedKey::ArrowDown) => {
                let down = matches!(key, Key::Named(NamedKey::ArrowDown));
                let last = self.playlist.len().saturating_sub(1);
                let to = match (cursor, down) {
                    (None, _) => 0,
                    (Some(i), true) => (i + 1).min(last),
                    (Some(i), false) => i.saturating_sub(1),
                };
                let how = if self.modifiers.shift_key() {
                    Select::Extend
                } else {
                    Select::Only
                };
                self.playlist.select(to, how);
                self.scroll_into_view(to);
            }
            Key::Named(NamedKey::Enter) => {
                if let Some(i) = cursor {
                    self.play_entry(i);
                }
            }
            Key::Named(NamedKey::Delete | NamedKey::Backspace) => {
                self.playlist.remove_selected();
                self.scroll_to(self.pl_scroll);
                self.requeue();
            }
            Key::Character(c) if c.as_str() == "a" && self.modifiers.super_key() => {
                self.playlist.select_all(true);
            }
            _ => return false,
        }
        self.request_redraw(PaneKind::Playlist);
        true
    }
}
