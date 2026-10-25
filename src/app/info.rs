//! File Info: the clutter bar's I, the I key and Alt+3, View File Info…,
//! and the playlist's and the library's File Info…
//! It shows a file's tags in fields to change, and what the file is. Tab
//! moves between fields; Enter or Save writes the tags into the file, and
//! the playlist and the library take the new ones; Escape closes.

use std::path::{Path, PathBuf};
use std::time::Instant;

use lantern_library::TrackInfo;
use lantern_library::tags;
use winit::event::KeyEvent;
use winit::keyboard::{Key, NamedKey};

use super::text_edit::{Edit, TextEdit};
use super::{App, DOUBLE_CLICK, PaneKind};
use crate::ui::WidgetId;
use crate::ui::gen_controls::{Field, caret_at};
use crate::ui::info_window::{self as iw, FIELDS, InfoView};
use crate::ui::playlist_window::clock;

/// Which field holds what, in the window's order.
const TITLE: usize = 0;
const ARTIST: usize = 1;
const ALBUM: usize = 2;
const ALBUM_ARTIST: usize = 3;
const GENRE: usize = 4;
const YEAR: usize = 5;
const TRACK: usize = 6;
const DISC: usize = 7;

#[derive(Default)]
pub(crate) struct InfoUi {
    path: Option<PathBuf>,
    fields: [TextEdit; 8],
    /// The field with the keyboard.
    focus: usize,
    details: Vec<String>,
    message: String,
    preedit: Option<String>,
    field_click: Option<(Instant, usize)>,
}

/// A field as drawn: its text, the caret, the selection and what an input
/// method is composing, positions in chars.
type FieldState = (
    String,
    usize,
    Option<(usize, usize)>,
    Option<(usize, usize)>,
);

/// What the window shows, owned, so it can be built before drawing borrows
/// the window.
pub(super) struct InfoState {
    fields: [FieldState; 8],
    focus: usize,
    details: Vec<String>,
    message: String,
}

impl InfoState {
    pub(super) fn view(&self, focused: bool) -> InfoView<'_> {
        InfoView {
            focused,
            fields: std::array::from_fn(|i| {
                let (text, caret, selection, marked) = &self.fields[i];
                Field {
                    text,
                    caret: (focused && i == self.focus).then_some(*caret),
                    selection: *selection,
                    marked: *marked,
                }
            }),
            details: self.details.clone(),
            message: &self.message,
        }
    }
}

fn number(n: Option<u32>) -> String {
    n.map(|n| n.to_string()).unwrap_or_default()
}

/// A number field: empty for none; otherwise it must be a whole number.
fn parse(field: &TextEdit, what: &str) -> Result<Option<u32>, String> {
    let t = field.text.trim();
    if t.is_empty() {
        return Ok(None);
    }
    t.parse()
        .map(Some)
        .map_err(|_| format!("The {what} must be a whole number"))
}

/// The lines about the file under its tags.
fn detail_lines(path: &Path) -> Vec<String> {
    let d = tags::details(path);
    let mut lines = vec![format!("Format: {}", d.format)];
    if let Some(secs) = d.duration_secs {
        lines.push(format!("Length: {}", clock(secs)));
    }
    if let Some(k) = d.kbps {
        lines.push(format!("Bitrate: {k} kbps"));
    }
    if let Some(hz) = d.sample_rate {
        lines.push(format!("Sample rate: {hz} Hz"));
    }
    if let Some(ch) = d.channels {
        let what = match ch {
            1 => "mono".to_string(),
            2 => "stereo".to_string(),
            n => format!("{n} channels"),
        };
        lines.push(format!("Channels: {what}"));
    }
    lines.push(format!("Size: {:.1} MB", d.size as f64 / 1_048_576.0));
    lines.push(path.display().to_string());
    lines
}

impl App {
    /// Open File Info on the file at `path`.
    pub(crate) fn open_info(&mut self, path: PathBuf) {
        let t = tags::read(&path);
        // A file without a title tag reads as its file name; show it empty.
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let title = if t.title == stem { "" } else { &t.title };
        let values = [
            title.to_string(),
            t.artist.clone(),
            t.album.clone(),
            t.album_artist.clone(),
            t.genre.clone(),
            number(t.year),
            number(t.track_no),
            number(t.disc_no),
        ];
        self.file_info = InfoUi {
            details: detail_lines(&path),
            fields: values.map(|v| {
                let mut e = TextEdit::new(&v);
                e.anchor = None;
                e
            }),
            path: Some(path),
            ..InfoUi::default()
        };
        self.file_info.fields[TITLE].select_all();
        self.set_visible(PaneKind::Info, true);
        if let Some(p) = self.pane(PaneKind::Info) {
            p.window.focus_window();
        }
        self.request_redraw(PaneKind::Info);
    }

    fn close_info(&mut self) {
        self.set_visible(PaneKind::Info, false);
        if let Some(p) = self.pane(PaneKind::Main) {
            p.window.focus_window();
        }
    }

    /// Write the fields into the file, and let the playlist and the
    /// library know.
    fn save_info(&mut self) {
        let Some(path) = self.file_info.path.clone() else {
            return;
        };
        let f = &self.file_info.fields;
        let numbers = (|| {
            Ok::<_, String>((
                parse(&f[YEAR], "year")?,
                parse(&f[TRACK], "track")?,
                parse(&f[DISC], "disc")?,
            ))
        })();
        let (year, track_no, disc_no) = match numbers {
            Ok(n) => n,
            Err(e) => {
                self.file_info.message = e;
                self.request_redraw(PaneKind::Info);
                return;
            }
        };
        let info = TrackInfo {
            path: path.clone(),
            title: f[TITLE].text.clone(),
            artist: f[ARTIST].text.clone(),
            album: f[ALBUM].text.clone(),
            album_artist: f[ALBUM_ARTIST].text.clone(),
            genre: f[GENRE].text.clone(),
            year,
            track_no,
            disc_no,
            ..TrackInfo::default()
        };
        match tags::write(&path, &info) {
            Ok(()) => {
                self.file_info.message = "Saved".into();
                self.retagged(&path);
            }
            Err(e) => self.file_info.message = format!("Could not save: {e}"),
        }
        self.request_redraw(PaneKind::Info);
    }

    /// The file at `path` has new tags: the playlist rereads its entries
    /// for it, and the library its track.
    fn retagged(&mut self, path: &Path) {
        let mut stale = false;
        for i in 0..self.playlist.len() {
            if let Some(e) = self.playlist.get_mut(i)
                && e.path == path
                && e.segment.is_none()
            {
                e.title = None;
                e.tags_loaded = false;
                stale = true;
            }
        }
        if stale {
            self.tags.request(path.to_path_buf());
        }
        self.library.refresh_file(path);
        self.request_redraw(PaneKind::Playlist);
        self.request_redraw(PaneKind::Library);
    }

    pub(super) fn info_state(&self) -> InfoState {
        let i = &self.file_info;
        InfoState {
            fields: std::array::from_fn(|n| {
                let f = &i.fields[n];
                match &i.preedit {
                    // Text an input method is composing shows at the caret.
                    Some(m) if n == i.focus => {
                        let mut chars: Vec<char> = f.text.chars().collect();
                        let at = f.caret.min(chars.len());
                        let k = m.chars().count();
                        chars.splice(at..at, m.chars());
                        (
                            chars.into_iter().collect(),
                            at + k,
                            None,
                            Some((at, at + k)),
                        )
                    }
                    _ => (f.text.clone(), f.caret, f.selection(), None),
                }
            }),
            focus: i.focus,
            details: i.details.clone(),
            message: i.message.clone(),
        }
    }

    pub(super) fn info_key(&mut self, event: &KeyEvent) -> bool {
        match &event.logical_key {
            Key::Named(NamedKey::Tab) => {
                let n = FIELDS.len();
                self.file_info.focus = if self.modifiers.shift_key() {
                    (self.file_info.focus + n - 1) % n
                } else {
                    (self.file_info.focus + 1) % n
                };
                self.file_info.fields[self.file_info.focus].select_all();
            }
            Key::Named(NamedKey::Enter) => self.save_info(),
            Key::Named(NamedKey::Escape) => self.close_info(),
            key => {
                let focus = self.file_info.focus;
                let typed = event.text.as_deref();
                if self.file_info.fields[focus].key(key, typed, self.modifiers) == Edit::Ignored {
                    return false;
                }
            }
        }
        self.request_redraw(PaneKind::Info);
        true
    }

    pub(super) fn info_ime(&mut self, ime: winit::event::Ime) {
        use winit::event::Ime;
        match ime {
            Ime::Preedit(text, _) => self.file_info.preedit = (!text.is_empty()).then_some(text),
            Ime::Commit(text) => {
                self.file_info.preedit = None;
                let focus = self.file_info.focus;
                self.file_info.fields[focus].insert(&text);
            }
            Ime::Enabled | Ime::Disabled => self.file_info.preedit = None,
        }
        self.request_redraw(PaneKind::Info);
    }

    fn info_field_at(&self, id: WidgetId) -> Option<usize> {
        FIELDS.iter().position(|&f| f == id)
    }

    /// Where in field `n` a pointer `x` across the window lands.
    fn info_caret(&self, n: usize, x: i32) -> usize {
        let r = iw::layout().fields[n];
        self.font.as_ref().map_or(0, |f| {
            caret_at(f, &self.file_info.fields[n].text, x - r.x - 2)
        })
    }

    pub(super) fn press_info(&mut self, id: WidgetId, x: i32) {
        let Some(n) = self.info_field_at(id) else {
            return;
        };
        self.file_info.focus = n;
        let now = Instant::now();
        let double = self
            .file_info
            .field_click
            .is_some_and(|(t, f)| f == n && now.duration_since(t) < DOUBLE_CLICK);
        self.file_info.field_click = Some((now, n));
        if double {
            self.file_info.fields[n].select_all();
        } else {
            let at = self.info_caret(n, x);
            let shift = self.modifiers.shift_key();
            self.file_info.fields[n].move_to(at, shift);
        }
        self.request_redraw(PaneKind::Info);
    }

    /// A field dragged across, selecting.
    pub(super) fn drag_info(&mut self, id: WidgetId, x: i32) {
        let Some(n) = self
            .info_field_at(id)
            .filter(|&n| n == self.file_info.focus)
        else {
            return;
        };
        let at = self.info_caret(n, x);
        if at != self.file_info.fields[n].caret {
            self.file_info.fields[n].move_to(at, true);
            self.request_redraw(PaneKind::Info);
        }
    }

    pub(super) fn activate_info(&mut self, id: WidgetId) {
        match id {
            iw::SAVE => self.save_info(),
            iw::CLOSE | iw::DISMISS => self.close_info(),
            _ => {}
        }
    }

    /// File Info for the playlist's selected entry, or the one playing.
    pub(crate) fn show_file_info(&mut self) {
        let path = self
            .playlist
            .entries()
            .iter()
            .find(|e| e.selected)
            .or_else(|| self.playlist.current().and_then(|i| self.playlist.get(i)))
            .map(|e| e.path.clone());
        if let Some(path) = path {
            self.open_info(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn number_fields_take_whole_numbers_or_nothing() {
        assert_eq!(parse(&TextEdit::new(" 1999 "), "year"), Ok(Some(1999)));
        assert_eq!(parse(&TextEdit::new(""), "year"), Ok(None));
        assert!(parse(&TextEdit::new("nineteen"), "year").is_err());
    }
}
