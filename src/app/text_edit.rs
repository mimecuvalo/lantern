//! A one-line text field's editing: the search box, a playlist's name
//! while it is renamed. Characters go in at the caret; the usual keys move
//! it and delete; ⌘V pastes, ⌘⌫ deletes back to the start.

use winit::keyboard::{Key, ModifiersState, NamedKey};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct TextEdit {
    pub text: String,
    /// In chars, 0 to the text's length.
    pub caret: usize,
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
    pub fn new(text: &str) -> Self {
        Self {
            text: text.to_string(),
            caret: text.chars().count(),
        }
    }

    fn chars(&self) -> Vec<char> {
        self.text.chars().collect()
    }

    /// Put `s` in at the caret, leaving out control characters.
    pub fn insert(&mut self, s: &str) -> Edit {
        let typed: Vec<char> = s.chars().filter(|c| !c.is_control()).collect();
        if typed.is_empty() {
            return Edit::Ignored;
        }
        let mut chars = self.chars();
        let at = self.caret.min(chars.len());
        chars.splice(at..at, typed.iter().copied());
        self.text = chars.into_iter().collect();
        self.caret = at + typed.len();
        Edit::Changed
    }

    /// A key, with the text it typed (if any) and the modifiers held.
    pub fn key(&mut self, key: &Key, typed: Option<&str>, m: ModifiersState) -> Edit {
        let mut chars = self.chars();
        let caret = self.caret.min(chars.len());
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
                Key::Named(NamedKey::Backspace) if caret > 0 => {
                    chars.drain(..caret);
                    self.text = chars.into_iter().collect();
                    self.caret = 0;
                    Edit::Changed
                }
                _ => Edit::Ignored,
            };
        }
        match key {
            Key::Named(NamedKey::Backspace) if caret > 0 => {
                chars.remove(caret - 1);
                self.text = chars.into_iter().collect();
                self.caret = caret - 1;
                Edit::Changed
            }
            Key::Named(NamedKey::Delete) if caret < chars.len() => {
                chars.remove(caret);
                self.text = chars.into_iter().collect();
                Edit::Changed
            }
            Key::Named(NamedKey::Backspace | NamedKey::Delete) => Edit::Moved,
            Key::Named(NamedKey::ArrowLeft) => {
                self.caret = caret.saturating_sub(1);
                Edit::Moved
            }
            Key::Named(NamedKey::ArrowRight) => {
                self.caret = (caret + 1).min(chars.len());
                Edit::Moved
            }
            Key::Named(NamedKey::Home) => {
                self.caret = 0;
                Edit::Moved
            }
            Key::Named(NamedKey::End) => {
                self.caret = chars.len();
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
        e.caret = 3;
        let edit = e.key(&named(NamedKey::Backspace), None, ModifiersState::SUPER);
        assert_eq!(edit, Edit::Changed);
        assert_eq!((e.text.as_str(), e.caret), ("rk", 0));
    }

    #[test]
    fn control_characters_stay_out() {
        let mut e = TextEdit::default();
        assert_eq!(e.insert("a\tb\n"), Edit::Changed);
        assert_eq!(e.text, "ab");
        assert_eq!(e.insert("\r"), Edit::Ignored);
    }
}
