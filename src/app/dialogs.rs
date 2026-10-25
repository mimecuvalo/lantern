//! Native file dialogs, and what they feed: files, folders, playlists,
//! skins.
//!
//! `rfd` runs the platform's own panel modally on the UI thread — on macOS
//! an `NSOpenPanel`. The event loop simply waits while it is open.

use std::path::PathBuf;

use rfd::FileDialog;

use super::{App, PaneKind};
use crate::ui::font::TextFont;

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
    /// Eject: choose files and play them, replacing the playlist — what
    /// Winamp's eject button does.
    pub(crate) fn eject(&mut self) {
        let Some(files) = open_media().set_title("Open files").pick_files() else {
            return;
        };
        self.playlist.clear();
        self.pl_scroll = 0;
        self.add_paths(files);
        self.play_entry(0);
    }

    pub(crate) fn add_files_dialog(&mut self) {
        if let Some(files) = open_media().set_title("Add files").pick_files() {
            self.add_paths(files);
        }
    }

    pub(crate) fn add_folder_dialog(&mut self) {
        if let Some(dir) = FileDialog::new().set_title("Add folder").pick_folder() {
            self.add_paths([dir]);
        }
    }

    /// Replace the playlist with a saved one.
    pub(crate) fn load_playlist_dialog(&mut self) {
        let Some(file) = FileDialog::new()
            .set_title("Load playlist")
            .add_filter("Playlists", PLAYLISTS)
            .pick_file()
        else {
            return;
        };
        self.playlist.clear();
        self.pl_scroll = 0;
        self.add_paths([file]);
    }

    pub(crate) fn save_playlist_dialog(&mut self) {
        let Some(mut file) = FileDialog::new()
            .set_title("Save playlist")
            .add_filter("M3U8 playlist", &["m3u8"])
            .set_file_name("playlist.m3u8")
            .save_file()
        else {
            return;
        };
        if file.extension().is_none() {
            file.set_extension("m3u8");
        }
        let text = lantern_playlist::m3u::write_m3u8(self.playlist.entries());
        if let Err(e) = std::fs::write(&file, text) {
            eprintln!("could not save {}: {e}", file.display());
        }
    }

    pub(crate) fn load_skin_dialog(&mut self) {
        let Some(file) = FileDialog::new()
            .set_title("Load skin")
            .add_filter("Winamp classic skins", &["wsz", "zip"])
            .pick_file()
        else {
            return;
        };
        self.load_skin_file(file);
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
