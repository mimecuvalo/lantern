//! Jump to File: J opens it; typing narrows the playlist to the entries
//! whose title or file name holds every word typed; Up and Down pick one;
//! Enter, a double-click or Jump to File plays it; Escape closes.

use std::time::Instant;

use winit::event::{KeyEvent, MouseScrollDelta};
use winit::keyboard::{Key, NamedKey};

use super::text_edit::{Edit, TextEdit};
use super::{App, DOUBLE_CLICK, PaneKind};
use crate::ui::WidgetId;
use crate::ui::gen_controls::{Field, ScrollState, caret_at};
use crate::ui::jump_window::{self as jw, JumpView, ROW_HEIGHT, visible_rows};

#[derive(Default)]
pub(crate) struct JumpUi {
    query: TextEdit,
    /// Which match is picked, counting from the first.
    pick: usize,
    /// The first match in view.
    scroll: usize,
    preedit: Option<String>,
    last_click: Option<(Instant, usize)>,
    /// When the field was last pressed, for a double-click.
    field_click: Option<Instant>,
}

/// What the window shows, owned, so it can be built before drawing borrows
/// the window.
pub(super) struct JumpState {
    text: String,
    caret: usize,
    selection: Option<(usize, usize)>,
    marked: Option<(usize, usize)>,
    rows: Vec<(String, bool)>,
    scroll: ScrollState,
}

impl JumpState {
    pub(super) fn view(&self, focused: bool) -> JumpView<'_> {
        JumpView {
            focused,
            field: Field {
                text: &self.text,
                caret: focused.then_some(self.caret),
                selection: self.selection,
                marked: self.marked,
            },
            rows: self.rows.clone(),
            scroll: self.scroll,
        }
    }
}

impl App {
    /// The playlist entries that match what is typed, in playlist order.
    fn jump_matches(&self) -> Vec<usize> {
        let terms = lantern_library::view::terms(&self.jump.query.text);
        self.playlist
            .entries()
            .iter()
            .enumerate()
            .filter(|(_, e)| {
                let file = e
                    .path
                    .file_name()
                    .map(|f| f.to_string_lossy())
                    .unwrap_or_default();
                let hay = format!("{}\n{file}", e.display_title()).to_lowercase();
                terms.iter().all(|t| hay.contains(t.as_str()))
            })
            .map(|(i, _)| i)
            .collect()
    }

    /// Open Jump to File, its last words selected so typing replaces them,
    /// the playing entry picked.
    pub(crate) fn open_jump(&mut self) {
        self.jump.query.select_all();
        let matches = self.jump_matches();
        self.jump.pick = self
            .playlist
            .current()
            .and_then(|c| matches.iter().position(|&i| i == c))
            .unwrap_or(0);
        self.jump.scroll = self.jump.pick.saturating_sub(visible_rows() / 2);
        self.set_visible(PaneKind::Jump, true);
        if let Some(p) = self.pane(PaneKind::Jump) {
            p.window.focus_window();
        }
        self.request_redraw(PaneKind::Jump);
    }

    fn close_jump(&mut self) {
        self.set_visible(PaneKind::Jump, false);
        if let Some(p) = self.pane(PaneKind::Main) {
            p.window.focus_window();
        }
    }

    /// Play the picked match, and close.
    fn jump(&mut self) {
        if let Some(&i) = self.jump_matches().get(self.jump.pick) {
            self.play_entry(i);
            self.scroll_into_view(i);
            self.close_jump();
        }
    }

    pub(super) fn jump_state(&self) -> JumpState {
        let matches = self.jump_matches();
        let shown = visible_rows();
        let j = &self.jump;
        let rows = matches
            .iter()
            .enumerate()
            .skip(j.scroll)
            .take(shown)
            .map(|(n, &i)| {
                let title = self
                    .playlist
                    .get(i)
                    .map(|e| e.display_title())
                    .unwrap_or_default();
                (format!("{}. {title}", i + 1), n == j.pick)
            })
            .collect();
        let max = matches.len().saturating_sub(shown);
        let scroll = ScrollState {
            position: if max == 0 {
                0.0
            } else {
                j.scroll as f32 / max as f32
            },
            visible: if matches.is_empty() {
                1.0
            } else {
                shown as f32 / matches.len() as f32
            },
            pressed: None,
        };
        // Text an input method is composing shows at the caret, underlined.
        let (text, caret, marked) = match &j.preedit {
            Some(m) => {
                let mut chars: Vec<char> = j.query.text.chars().collect();
                let at = j.query.caret.min(chars.len());
                let n = m.chars().count();
                chars.splice(at..at, m.chars());
                (chars.into_iter().collect(), at + n, Some((at, at + n)))
            }
            None => (j.query.text.clone(), j.query.caret, None),
        };
        JumpState {
            text,
            caret,
            selection: j.query.selection(),
            marked,
            rows,
            scroll,
        }
    }

    /// Pick match `to`, keeping it in view.
    fn pick_jump(&mut self, to: usize) {
        let n = self.jump_matches().len();
        let shown = visible_rows().max(1);
        let j = &mut self.jump;
        j.pick = to.min(n.saturating_sub(1));
        if j.pick < j.scroll {
            j.scroll = j.pick;
        } else if j.pick >= j.scroll + shown {
            j.scroll = j.pick + 1 - shown;
        }
        self.request_redraw(PaneKind::Jump);
    }

    /// What was typed changed: start from the first match.
    fn set_jump_query(&mut self, query: TextEdit) {
        let changed = query.text != self.jump.query.text;
        self.jump.query = query;
        if changed {
            self.jump.pick = 0;
            self.jump.scroll = 0;
        }
        self.request_redraw(PaneKind::Jump);
    }

    pub(super) fn jump_key(&mut self, event: &KeyEvent) -> bool {
        let shown = visible_rows();
        let pick = self.jump.pick;
        match &event.logical_key {
            Key::Named(NamedKey::ArrowUp) => self.pick_jump(pick.saturating_sub(1)),
            Key::Named(NamedKey::ArrowDown) => self.pick_jump(pick + 1),
            Key::Named(NamedKey::PageUp) => self.pick_jump(pick.saturating_sub(shown)),
            Key::Named(NamedKey::PageDown) => self.pick_jump(pick + shown),
            Key::Named(NamedKey::Enter) => self.jump(),
            Key::Named(NamedKey::Escape) => self.close_jump(),
            key => {
                let mut query = self.jump.query.clone();
                match query.key(key, event.text.as_deref(), self.modifiers) {
                    Edit::Ignored => return false,
                    _ => self.set_jump_query(query),
                }
            }
        }
        true
    }

    pub(super) fn jump_ime(&mut self, ime: winit::event::Ime) {
        use winit::event::Ime;
        match ime {
            Ime::Preedit(text, _) => self.jump.preedit = (!text.is_empty()).then_some(text),
            Ime::Commit(text) => {
                self.jump.preedit = None;
                let mut query = self.jump.query.clone();
                query.insert(&text);
                self.set_jump_query(query);
            }
            Ime::Enabled | Ime::Disabled => self.jump.preedit = None,
        }
        self.request_redraw(PaneKind::Jump);
    }

    pub(super) fn press_jump(&mut self, id: WidgetId, x: i32, y: i32) {
        let l = jw::layout();
        match id {
            jw::FIELD => {
                let at = self
                    .font
                    .as_ref()
                    .map_or(0, |f| caret_at(f, &self.jump.query.text, x - l.field.x - 2));
                let now = Instant::now();
                let double = self
                    .jump
                    .field_click
                    .is_some_and(|t| now.duration_since(t) < DOUBLE_CLICK);
                self.jump.field_click = Some(now);
                if double {
                    self.jump.query.select_all();
                } else {
                    self.jump.query.move_to(at, self.modifiers.shift_key());
                }
            }
            jw::LIST => {
                let n = ((y - l.list.y) / ROW_HEIGHT as i32).max(0) as usize + self.jump.scroll;
                if n >= self.jump_matches().len() {
                    return;
                }
                let now = Instant::now();
                let double = self
                    .jump
                    .last_click
                    .is_some_and(|(t, m)| m == n && now.duration_since(t) < DOUBLE_CLICK);
                self.jump.last_click = Some((now, n));
                self.pick_jump(n);
                if double {
                    self.jump();
                }
            }
            _ => {}
        }
        self.request_redraw(PaneKind::Jump);
    }

    /// The field dragged across, selecting, or the scrollbar dragged.
    pub(super) fn drag_jump(&mut self, id: WidgetId, x: i32, y: i32) {
        let l = jw::layout();
        match id {
            jw::FIELD => {
                let at = self
                    .font
                    .as_ref()
                    .map_or(0, |f| caret_at(f, &self.jump.query.text, x - l.field.x - 2));
                if at != self.jump.query.caret {
                    self.jump.query.move_to(at, true);
                }
            }
            jw::SCROLL => {
                // The bar maps straight onto the matches.
                let n = self.jump_matches().len();
                let max = n.saturating_sub(visible_rows());
                let at = (y - l.scroll.y) as f32 / l.scroll.h.max(1) as f32;
                self.jump.scroll = (at.clamp(0.0, 1.0) * max as f32).round() as usize;
            }
            _ => {}
        }
        self.request_redraw(PaneKind::Jump);
    }

    pub(super) fn wheel_jump(&mut self, delta: MouseScrollDelta) {
        let lines = match delta {
            MouseScrollDelta::LineDelta(_, y) => -y.round() as i64 * 3,
            MouseScrollDelta::PixelDelta(p) => -(p.y / f64::from(ROW_HEIGHT)).round() as i64,
        };
        let max = self.jump_matches().len().saturating_sub(visible_rows());
        self.jump.scroll = ((self.jump.scroll as i64 + lines).max(0) as usize).min(max);
        self.request_redraw(PaneKind::Jump);
    }

    pub(super) fn activate_jump(&mut self, id: WidgetId) {
        match id {
            jw::JUMP => self.jump(),
            jw::CLOSE | jw::DISMISS => self.close_jump(),
            _ => {}
        }
    }
}
