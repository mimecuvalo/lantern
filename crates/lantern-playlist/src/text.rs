//! Bytes to text for playlist and cue files, whose encoding is never stated.
//!
//! Order: a BOM wins; then strict UTF-8, which rejects the ill-formed byte
//! sequences legacy encodings produce; then a statistical guess among legacy
//! encodings. Cue sheets from Japanese and Russian rippers are routinely
//! Shift-JIS or CP-1251, so guessing beats assuming CP-1252.

use chardetng::{EncodingDetector, Iso2022JpDetection, Utf8Detection};
use encoding_rs::{Encoding, UTF_8, UTF_16BE, UTF_16LE};

/// Decode `bytes`, reporting which encoding was used.
pub fn decode(bytes: &[u8]) -> (String, &'static Encoding) {
    if let Some((encoding, bom_len)) = Encoding::for_bom(bytes) {
        let (text, _) = encoding.decode_without_bom_handling(&bytes[bom_len..]);
        return (text.into_owned(), encoding);
    }
    if let Ok(text) = std::str::from_utf8(bytes) {
        return (text.to_string(), UTF_8);
    }
    let mut detector = EncodingDetector::new(Iso2022JpDetection::Deny);
    detector.feed(bytes, true);
    let encoding = detector.guess(None, Utf8Detection::Deny);
    // UTF-16 without a BOM is not something the detector proposes, and not
    // something these files are; guard anyway so a guess never yields
    // mojibake of every character.
    let encoding = if encoding == UTF_16LE || encoding == UTF_16BE {
        encoding_rs::WINDOWS_1252
    } else {
        encoding
    };
    let (text, _) = encoding.decode_without_bom_handling(bytes);
    (text.into_owned(), encoding)
}

/// Lines split on CRLF, LF or CR, which all occur in the wild — sometimes
/// mixed in one file.
pub fn lines(text: &str) -> impl Iterator<Item = &str> {
    text.split('\n').flat_map(|l| l.split('\r'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boms_win() {
        assert_eq!(decode(b"\xEF\xBB\xBFabc").0, "abc");
        assert_eq!(decode(b"\xFF\xFEa\0b\0").0, "ab");
        assert_eq!(decode(b"\xFE\xFF\0a\0b").0, "ab");
    }

    #[test]
    fn utf8_is_taken_as_is() {
        let (text, enc) = decode("TITLE \"Café\"".as_bytes());
        assert_eq!(text, "TITLE \"Café\"");
        assert_eq!(enc, UTF_8);
    }

    #[test]
    fn legacy_encodings_are_detected() {
        let shift_jis = [
            0x54, 0x49, 0x54, 0x4C, 0x45, 0x20, 0x22, 0x93, 0xFA, 0x96, 0x7B, 0x22, 0x0A,
        ];
        assert!(
            decode(&shift_jis).0.contains("日本"),
            "{:?}",
            decode(&shift_jis)
        );

        let cp1251 = [
            0x54, 0x49, 0x54, 0x4C, 0x45, 0x20, 0x22, 0xD2, 0xE5, 0xF1, 0xF2, 0x22, 0x0D, 0x0A,
        ];
        assert!(decode(&cp1251).0.contains("Тест"), "{:?}", decode(&cp1251));

        let cp1252 = [
            0x54, 0x49, 0x54, 0x4C, 0x45, 0x20, 0x22, 0x43, 0x61, 0x66, 0xE9, 0x22, 0x0A,
        ];
        assert!(decode(&cp1252).0.contains("Café"), "{:?}", decode(&cp1252));
    }

    #[test]
    fn mixed_line_endings_split() {
        let got: Vec<_> = lines("a\r\nb\nc\rd").filter(|l| !l.is_empty()).collect();
        assert_eq!(got, ["a", "b", "c", "d"]);
    }
}
