//! The skin's plain-text files: `viscolor.txt`, `pledit.txt`, `region.txt`.
//!
//! All three are hand-edited, decades old, and broken in every way a text
//! file can be. Each parser follows Webamp's (MIT, Copyright (c) 2015 Jordan
//! Eldredge) tolerance: take what parses, default the rest, never fail.

use std::collections::HashMap;

use crate::{PlaylistStyle, Polygon, Region, VisColors};

/// Bytes to text. UTF-8 when it is (BOM stripped), otherwise Latin-1 — these
/// files come from Windows code pages, where Latin-1 is right for ASCII and
/// close enough for the rest.
pub(crate) fn decode(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(s) => s.strip_prefix('\u{feff}').unwrap_or(s).to_string(),
        Err(_) => bytes.iter().map(|&b| char::from(b)).collect(),
    }
}

/// `section -> key -> value`, sections and keys lowercased.
type Ini = HashMap<String, HashMap<String, String>>;

/// Winamp's forgiving INI dialect.
///
/// `;` starts a comment line. A value is cut at any second `=` and loses one
/// layer of surrounding quotes. Properties before the first section are
/// dropped, and a repeated section header starts that section afresh.
pub(crate) fn parse_ini(text: &str) -> Ini {
    let mut ini = Ini::new();
    let mut section: Option<String> = None;
    for line in text.split(['\r', '\n']) {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with(';') {
            continue;
        }
        if let Some(name) = trimmed.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            let name = name.trim().to_lowercase();
            ini.insert(name.clone(), HashMap::new());
            section = Some(name);
            continue;
        }
        let (Some(section), Some((key, value))) = (&section, trimmed.split_once('=')) else {
            continue;
        };
        let value = value.split('=').next().unwrap_or("").trim();
        let value = strip_quotes(value);
        if let Some(props) = ini.get_mut(section) {
            props.insert(key.trim().to_lowercase(), value.to_string());
        }
    }
    ini
}

fn strip_quotes(s: &str) -> &str {
    let s = s.strip_prefix(['"', '\'']).unwrap_or(s);
    s.strip_suffix(['"', '\'']).unwrap_or(s)
}

/// `viscolor.txt`: up to 24 lines, each starting `r,g,b`.
///
/// Anything after the third number is ignored — skins annotate freely with
/// `//` comments, tabs or nothing at all. Lines that do not start with three
/// numbers are skipped *without* consuming a slot; missing slots keep the
/// default.
pub(crate) fn parse_viscolors(text: &str, defaults: &VisColors) -> VisColors {
    let mut colors = *defaults;
    let parsed = text.split('\n').filter_map(leading_rgb);
    for (slot, rgb) in colors.iter_mut().zip(parsed) {
        *slot = rgb;
    }
    colors
}

/// Three leading decimal numbers separated by whitespace and at most one
/// comma each. Values over 255 clamp.
fn leading_rgb(line: &str) -> Option<[u8; 3]> {
    let mut rest = line;
    let mut rgb = [0u8; 3];
    for (i, channel) in rgb.iter_mut().enumerate() {
        rest = rest.trim_start();
        if i > 0 {
            rest = rest.strip_prefix(',').unwrap_or(rest).trim_start();
        }
        let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
        if digits == 0 {
            return None;
        }
        let value: u32 = rest[..digits].parse().unwrap_or(u32::MAX);
        *channel = value.min(255) as u8;
        rest = &rest[digits..];
    }
    Some(rgb)
}

/// `pledit.txt`'s `[Text]` section. Unparseable colours keep their default.
pub(crate) fn parse_pledit(text: &str) -> PlaylistStyle {
    let mut style = PlaylistStyle::default();
    let ini = parse_ini(text);
    let Some(props) = ini.get("text") else {
        return style;
    };
    for (key, slot) in [
        ("normal", &mut style.normal),
        ("current", &mut style.current),
        ("normalbg", &mut style.normal_bg),
        ("selectedbg", &mut style.selected_bg),
    ] {
        if let Some(rgb) = props.get(key).and_then(|v| parse_hex_color(v)) {
            *slot = rgb;
        }
    }
    if let Some(font) = props.get("font").filter(|f| !f.is_empty()) {
        style.font = font.clone();
    }
    style
}

/// `#RRGGBB`, with the `#` optional and anything past six digits ignored —
/// Winamp accepts overlong colours and skins depend on it.
fn parse_hex_color(value: &str) -> Option<[u8; 3]> {
    let hex = value.strip_prefix('#').unwrap_or(value);
    let hex = hex.get(..6)?;
    let n = u32::from_str_radix(hex, 16).ok()?;
    Some([(n >> 16) as u8, (n >> 8) as u8, n as u8])
}

/// `region.txt`: per window, a list of polygons given as `NumPoints` (a
/// comma list of per-polygon counts) and `PointList` (coordinates separated
/// by commas and/or spaces).
///
/// Polygons with fewer than three points are skipped but still consume their
/// points, as in Webamp. Where Webamp keeps a truncated final polygon when
/// the point list runs short, this drops it: a truncated outline is a
/// different shape, not a partial one.
pub(crate) fn parse_region(text: &str) -> Region {
    let ini = parse_ini(text);
    let section = |name: &str| ini.get(name).map(polygons).unwrap_or_default();
    Region {
        normal: section("normal"),
        windowshade: section("windowshade"),
        equalizer: section("equalizer"),
        equalizer_shade: section("equalizerws"),
    }
}

fn polygons(props: &HashMap<String, String>) -> Vec<Polygon> {
    let (Some(counts), Some(list)) = (props.get("numpoints"), props.get("pointlist")) else {
        return Vec::new();
    };
    let coords: Option<Vec<i32>> = list
        .split([',', ' ', '\t'])
        .filter(|s| !s.is_empty())
        .map(|s| s.trim().parse().ok())
        .collect();
    let Some(coords) = coords else {
        return Vec::new();
    };
    let points: Vec<(i32, i32)> = coords
        .as_chunks::<2>()
        .0
        .iter()
        .map(|&[x, y]| (x, y))
        .collect();

    let mut out = Vec::new();
    let mut next = 0;
    for count in counts
        .split(',')
        .filter_map(|s| s.trim().parse::<usize>().ok())
    {
        let Some(polygon) = points.get(next..next + count) else {
            break;
        };
        next += count;
        if count >= 3 {
            out.push(Polygon {
                points: polygon.to_vec(),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DEFAULT_VISCOLORS;

    #[test]
    fn ini_is_case_insensitive_and_tolerant() {
        let ini =
            parse_ini("orphan=1\r\n; comment\r\n[Text]\r\nNormal = \"#00FF00\"\r\nFont=A=B\r\n");
        let text = &ini["text"];
        assert_eq!(text["normal"], "#00FF00");
        assert_eq!(text["font"], "A", "value is cut at a second '='");
        assert_eq!(
            ini.len(),
            1,
            "orphan property before any section is dropped"
        );
    }

    #[test]
    fn repeated_section_starts_afresh() {
        let ini = parse_ini("[A]\nx=1\n[a]\ny=2\n");
        assert!(!ini["a"].contains_key("x"));
        assert_eq!(ini["a"]["y"], "2");
    }

    #[test]
    fn viscolors_accept_every_annotation_style() {
        let text = "0,0,0, // color 0 = black\r\n\
                    000,039,063\r\n\
                    0,207,144\t// 2\r\n\
                    garbage line\r\n\
                    1 2 3\r\n\
                    999,0,0\r\n";
        let c = parse_viscolors(text, &DEFAULT_VISCOLORS);
        assert_eq!(c[0], [0, 0, 0]);
        assert_eq!(c[1], [0, 39, 63], "leading zeros are decimal, not octal");
        assert_eq!(c[2], [0, 207, 144]);
        assert_eq!(c[3], [1, 2, 3], "garbage does not consume a slot");
        assert_eq!(c[4], [255, 0, 0], "out-of-range clamps");
        assert_eq!(c[5], DEFAULT_VISCOLORS[5], "missing lines keep defaults");
    }

    #[test]
    fn viscolors_ignore_lines_beyond_24() {
        let text = "1,1,1\n".repeat(30);
        let c = parse_viscolors(&text, &DEFAULT_VISCOLORS);
        assert!(c.iter().all(|rgb| *rgb == [1, 1, 1]));
    }

    #[test]
    fn pledit_normalises_colours() {
        let style = parse_pledit(
            "[Text]\r\nNormal=00CF90\r\nCurrent=#BAF1FFAA\r\nNormalBG=#zzzzzz\r\nFont=Tahoma\r\n",
        );
        assert_eq!(style.normal, [0x00, 0xCF, 0x90], "missing '#' is tolerated");
        assert_eq!(style.current, [0xBA, 0xF1, 0xFF], "overlong colour is cut");
        assert_eq!(style.normal_bg, PlaylistStyle::default().normal_bg);
        assert_eq!(style.font, "Tahoma");
    }

    #[test]
    fn pledit_without_text_section_is_default() {
        assert_eq!(parse_pledit("Normal=#FFFFFF"), PlaylistStyle::default());
    }

    #[test]
    fn region_parses_multiple_polygons_and_sections() {
        let region = parse_region(
            "[Normal]\r\n\
             NumPoints=4,4\r\n\
             PointList=0,0, 275,0, 275,116, 0,116  10 10 20 10 20 20 10 20\r\n\
             [WindowShade]\r\n\
             NumPoints=3\r\n\
             PointList=0,0,275,0,0,14\r\n",
        );
        assert_eq!(region.normal.len(), 2);
        assert_eq!(region.normal[0].points[2], (275, 116));
        assert_eq!(region.normal[1].points[0], (10, 10));
        assert_eq!(region.windowshade.len(), 1);
        assert!(region.equalizer.is_empty());
    }

    #[test]
    fn region_skips_degenerate_and_truncated_polygons() {
        let region =
            parse_region("[Normal]\nNumPoints=2,3,5\nPointList=0,0,1,1, 0,0,9,0,0,9, 1,1,2,2\n");
        // The 2-gon is skipped but consumes its points; the triangle after it
        // survives; the 5-gon has only two points left and is dropped.
        assert_eq!(region.normal.len(), 1);
        assert_eq!(region.normal[0].points, vec![(0, 0), (9, 0), (0, 9)]);
    }

    #[test]
    fn region_with_only_comments_is_empty() {
        let region = parse_region("; all comments\n; like base-2.91's\n");
        assert_eq!(region, Region::default());
    }

    #[test]
    fn non_utf8_text_decodes_as_latin1() {
        assert_eq!(decode(b"Font=Caf\xe9"), "Font=Café");
        assert_eq!(decode("\u{feff}[Text]".as_bytes()), "[Text]");
    }
}
