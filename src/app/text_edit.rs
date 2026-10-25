//! A one-line text field's editing: the search box, a playlist's name
//! while it is renamed. Characters go in at the caret; the usual keys move
//! it and delete; with Shift they select. Typing or deleting replaces the
//! selection; ⌘C, ⌘X and ⌘V copy, cut and paste; ⌘⌫ deletes back to the
//! start.

use winit::keyboard::{Key, ModifiersState, NamedKey};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct TextEdit {
    pub text: String,
    /// In chars, 0 to the text's length.
    pub caret: usize,
    /// Where a selection began; it runs from here to the caret.
    pub anchor: Option<usize>,
}

/// What a key did to the field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Edit {
    /// The text changed.
    Changed,
    /// Only the caret moved.
    Moved,
    /// Enter.
    Submit,
    /// Escape.
    Cancel,
    /// Not a key for the field.
    Ignored,
}

impl TextEdit {
    /// `text`, all of it selected, as a field is when it opens for a rename.
    pub fn new(text: &str) -> Self {
        let n = text.chars().count();
        Self {
            text: text.to_string(),
            caret: n,
            anchor: (n > 0).then_some(0),
        }
    }

    fn chars(&self) -> Vec<char> {
        self.text.chars().collect()
    }

    /// The selection, start to end, in chars; `None` if nothing is.
    pub fn selection(&self) -> Option<(usize, usize)> {
        let a = self.anchor?;
        let n = self.text.chars().count();
        let (a, c) = (a.min(n), self.caret.min(n));
        (a != c).then(|| (a.min(c), a.max(c)))
    }

    pub fn selected_text(&self) -> Option<String> {
        let (a, b) = self.selection()?;
        Some(self.text.chars().skip(a).take(b - a).collect())
    }

    pub fn select_all(&mut self) {
        self.anchor = Some(0);
        self.caret = self.text.chars().count();
    }

    /// Move the caret to `to`, selecting from where it was if `extend`.
    pub fn move_to(&mut self, to: usize, extend: bool) {
        if extend {
            self.anchor.get_or_insert(self.caret);
        } else {
            self.anchor = None;
        }
        self.caret = to.min(self.text.chars().count());
    }

    /// Delete the selection, if any. True if there was one.
    fn delete_selection(&mut self) -> bool {
        let Some((a, b)) = self.selection() else {
            self.anchor = None;
            return false;
        };
        let mut chars = self.chars();
        chars.drain(a..b);
        self.text = chars.into_iter().collect();
        self.caret = a;
        self.anchor = None;
        true
    }

    /// Put `s` in at the caret, in place of any selection, leaving out
    /// control characters.
    pub fn insert(&mut self, s: &str) -> Edit {
        let typed: Vec<char> = s.chars().filter(|c| !c.is_control()).collect();
        if typed.is_empty() {
            return Edit::Ignored;
        }
        self.delete_selection();
        let mut chars = self.chars();
        let at = self.caret.min(chars.len());
        chars.splice(at..at, typed.iter().copied());
        self.text = chars.into_iter().collect();
        self.caret = at + typed.len();
        Edit::Changed
    }

    /// A key, with the text it typed (if any) and the modifiers held.
    pub fn key(&mut self, key: &Key, typed: Option<&str>, m: ModifiersState) -> Edit {
        let n = self.text.chars().count();
        let caret = self.caret.min(n);
        let shift = m.shift_key();
        if m.control_key() || m.alt_key() {
            return Edit::Ignored;
        }
        if m.super_key() {
            return match key {
                Key::Character(c) if c.eq_ignore_ascii_case("v") => {
                    #[cfg(target_os = "macos")]
                    if let Some(paste) = crate::platform::macos::clipboard_text() {
                        return self.insert(&paste);
                    }
                    Edit::Ignored
                }
                Key::Character(c) if c.eq_ignore_ascii_case("c") || c.eq_ignore_ascii_case("x") => {
                    let Some(text) = self.selected_text() else {
                        return Edit::Ignored;
                    };
                    #[cfg(target_os = "macos")]
                    crate::platform::macos::set_clipboard_text(&text);
                    let _ = text;
                    if c.eq_ignore_ascii_case("x") {
                        self.delete_selection();
                        Edit::Changed
                    } else {
                        Edit::Moved
                    }
                }
                Key::Named(NamedKey::Backspace) => {
                    if self.delete_selection() {
                        return Edit::Changed;
                    }
                    if caret == 0 {
                        return Edit::Moved;
                    }
                    let mut chars = self.chars();
                    chars.drain(..caret);
                    self.text = chars.into_iter().collect();
                    self.caret = 0;
                    Edit::Changed
                }
                // ⌘← and ⌘→ go to the ends, as Home and End do.
                Key::Named(NamedKey::ArrowLeft) => {
                    self.move_to(0, shift);
                    Edit::Moved
                }
                Key::Named(NamedKey::ArrowRight) => {
                    self.move_to(n, shift);
                    Edit::Moved
                }
                _ => Edit::Ignored,
            };
        }
        match key {
            Key::Named(NamedKey::Backspace | NamedKey::Delete) if self.delete_selection() => {
                Edit::Changed
            }
            Key::Named(NamedKey::Backspace) if caret > 0 => {
                let mut chars = self.chars();
                chars.remove(caret - 1);
                self.text = chars.into_iter().collect();
                self.caret = caret - 1;
                Edit::Changed
            }
            Key::Named(NamedKey::Delete) if caret < n => {
                let mut chars = self.chars();
                chars.remove(caret);
                self.text = chars.into_iter().collect();
                Edit::Changed
            }
            Key::Named(NamedKey::Backspace | NamedKey::Delete) => Edit::Moved,
            // Without Shift, an arrow first collapses a selection to its
            // end on that side.
            Key::Named(NamedKey::ArrowLeft) => {
                let to = match self.selection() {
                    Some((a, _)) if !shift => a,
                    _ => caret.saturating_sub(1),
                };
                self.move_to(to, shift);
                Edit::Moved
            }
            Key::Named(NamedKey::ArrowRight) => {
                let to = match self.selection() {
                    Some((_, b)) if !shift => b,
                    _ => (caret + 1).min(n),
                };
                self.move_to(to, shift);
                Edit::Moved
            }
            Key::Named(NamedKey::Home) => {
                self.move_to(0, shift);
                Edit::Moved
            }
            Key::Named(NamedKey::End) => {
                self.move_to(n, shift);
                Edit::Moved
            }
            Key::Named(NamedKey::Enter) => Edit::Submit,
            Key::Named(NamedKey::Escape) => Edit::Cancel,
            _ => typed.map_or(Edit::Ignored, |t| self.insert(t)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named(k: NamedKey) -> Key {
        Key::Named(k)
    }

    fn typed(e: &mut TextEdit, s: &str) {
        for c in s.chars() {
            let k = Key::Character(c.to_string().into());
            e.key(&k, Some(&c.to_string()), ModifiersState::empty());
        }
    }

    #[test]
    fn types_moves_and_deletes_at_the_caret() {
        let none = ModifiersState::empty();
        let mut e = TextEdit::default();
        typed(&mut e, "hot rck");
        e.key(&named(NamedKey::ArrowLeft), None, none);
        e.key(&named(NamedKey::ArrowLeft), None, none);
        typed(&mut e, "o");
        assert_eq!((e.text.as_str(), e.caret), ("hot rock", 6));
        assert_eq!(
            e.key(&named(NamedKey::Backspace), None, none),
            Edit::Changed
        );
        assert_eq!(e.key(&named(NamedKey::Delete), None, none), Edit::Changed);
        assert_eq!(e.text, "hot rk");
        e.key(&named(NamedKey::Home), None, none);
        assert_eq!(e.key(&named(NamedKey::Backspace), None, none), Edit::Moved);
        assert_eq!(e.key(&named(NamedKey::Enter), None, none), Edit::Submit);
        assert_eq!(e.key(&named(NamedKey::Escape), None, none), Edit::Cancel);
    }

    #[test]
    fn command_backspace_clears_to_the_start() {
        let mut e = TextEdit::new("Björk");
        e.move_to(3, false);
        let edit = e.key(&named(NamedKey::Backspace), None, ModifiersState::SUPER);
        assert_eq!(edit, Edit::Changed);
        assert_eq!((e.text.as_str(), e.caret), ("rk", 0));
    }

    #[test]
    fn shift_selects_and_typing_replaces_it() {
        let shift = ModifiersState::SHIFT;
        let none = ModifiersState::empty();
        let mut e = TextEdit::default();
        typed(&mut e, "hot rock");
        e.key(&named(NamedKey::ArrowLeft), None, shift);
        e.key(&named(NamedKey::ArrowLeft), None, shift);
        assert_eq!(e.selected_text().as_deref(), Some("ck"));
        typed(&mut e, "x");
        assert_eq!(
            (e.text.as_str(), e.caret, e.selection()),
            ("hot rox", 7, None)
        );
        e.key(&named(NamedKey::Home), None, shift);
        assert_eq!(e.selection(), Some((0, 7)));
        // Right without Shift lands at the selection's end.
        e.key(&named(NamedKey::ArrowLeft), None, none);
        assert_eq!((e.caret, e.selection()), (0, None));
        e.move_to(4, true);
        assert_eq!(e.key(&named(NamedKey::Delete), None, none), Edit::Changed);
        assert_eq!(e.text, "rox");
    }

    #[test]
    fn a_new_field_has_its_text_selected() {
        let mut e = TextEdit::new("New Playlist");
        assert_eq!(e.selection(), Some((0, 12)));
        typed(&mut e, "Riot");
        assert_eq!(e.text, "Riot");
    }

    #[test]
    fn control_characters_stay_out() {
        let mut e = TextEdit::default();
        assert_eq!(e.insert("a\tb\n"), Edit::Changed);
        assert_eq!(e.text, "ab");
        assert_eq!(e.insert("\r"), Edit::Ignored);
    }
}
