//! The `text.bmp` bitmap font: a 31x3 grid of 5x6 glyphs.
//!
//! The character-to-cell mapping derives from Webamp's `FONT_LOOKUP` in
//! `skinSprites.ts` (MIT, Copyright (c) 2015 Jordan Eldredge). The font has no
//! lowercase: letters render the same regardless of case.

pub const GLYPH_WIDTH: u32 = 5;
pub const GLYPH_HEIGHT: u32 = 6;

/// The cell a space occupies, and where anything unknown falls back to.
const SPACE: (u8, u8) = (0, 30);

/// Every character with a glyph of its own (or a deliberate substitute),
/// uppercase and lowercase letters both included.
pub const CHARSET: &str = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ\
                           \"@ 0123456789\u{2026}.:()-'!_+\\/[]^&%,=$#\
                           ÅÖÄåöä?*<>{}";

/// `(row, column)` of `c`'s glyph. Characters the font lacks render as a
/// space rather than failing — track titles contain everything.
pub fn cell(c: char) -> (u8, u8) {
    if c.is_ascii_alphabetic() {
        return (0, c.to_ascii_lowercase() as u8 - b'a');
    }
    match c {
        '"' => (0, 26),
        '@' => (0, 27),
        ' ' => SPACE,
        '0'..='9' => (1, c as u8 - b'0'),
        '\u{2026}' => (1, 10),
        '.' => (1, 11),
        ':' => (1, 12),
        '(' => (1, 13),
        ')' => (1, 14),
        '-' => (1, 15),
        '\'' => (1, 16),
        '!' => (1, 17),
        '_' => (1, 18),
        '+' => (1, 19),
        '\\' => (1, 20),
        '/' => (1, 21),
        // Angle and curly brackets borrow the square ones.
        '[' | '<' | '{' => (1, 22),
        ']' | '>' | '}' => (1, 23),
        '^' => (1, 24),
        '&' => (1, 25),
        '%' => (1, 26),
        ',' => (1, 27),
        '=' => (1, 28),
        '$' => (1, 29),
        '#' => (1, 30),
        'Å' | 'å' => (2, 0),
        'Ö' | 'ö' => (2, 1),
        'Ä' | 'ä' => (2, 2),
        '?' => (2, 3),
        '*' => (2, 4),
        _ => SPACE,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letters_ignore_case() {
        assert_eq!(cell('a'), (0, 0));
        assert_eq!(cell('Z'), cell('z'));
        assert_eq!(cell('Ä'), cell('ä'));
    }

    #[test]
    fn unknown_characters_fall_back_to_space() {
        assert_eq!(cell('€'), cell(' '));
        assert_eq!(cell('漢'), SPACE);
        // Accented letters outside the font's three do not borrow a base
        // letter.
        assert_eq!(cell('é'), SPACE);
    }

    #[test]
    fn every_charset_member_has_a_real_cell() {
        for c in CHARSET.chars() {
            let (row, col) = cell(c);
            assert!(row < 3 && col < 31, "{c:?} -> ({row}, {col})");
            if c != ' ' {
                assert_ne!((row, col), SPACE, "{c:?} fell back to space");
            }
        }
    }
}
