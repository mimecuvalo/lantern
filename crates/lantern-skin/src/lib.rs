//! Parser for Winamp 2.x classic skins (`.wsz`).
//!
//! Pure by construction: bytes in, [`Skin`] out. No file I/O, no platform
//! types, no windowing. This is what makes the whole format testable against
//! the acceptance skins without a display.
//!
//! Sprite coordinates derive from [Webamp](https://github.com/captbaritone/webamp)
//! (MIT). See `docs/legal.md` — never from the Winamp source.

pub mod archive;
pub mod error;
pub mod font;
pub mod sprite;
mod text;

use std::collections::HashMap;

pub use error::SkinError;
pub use sprite::{
    ClutterButton, PlaylistButton, PlaylistMenu, Rect, ShadeThumb, SheetId, SpriteKey,
};

/// 24 colours from `viscolor.txt`, used by the spectrum analyser.
pub type VisColors = [[u8; 3]; 24];

/// Used for any `viscolor.txt` slot a skin leaves out, or the whole table
/// when there is no file.
///
/// Webamp's default-skin palette (`baseSkin.json`, MIT): a black background,
/// a grey dot colour, a red-to-green spectrum ramp and a grey oscilloscope/
/// peak ramp.
pub const DEFAULT_VISCOLORS: VisColors = [
    [0, 0, 0],
    [24, 33, 41],
    [239, 49, 16],
    [206, 41, 16],
    [214, 90, 0],
    [214, 102, 0],
    [214, 115, 0],
    [198, 123, 8],
    [222, 165, 24],
    [214, 181, 33],
    [189, 222, 41],
    [148, 222, 33],
    [41, 206, 16],
    [50, 190, 16],
    [57, 181, 16],
    [49, 156, 8],
    [41, 148, 0],
    [24, 132, 8],
    [255, 255, 255],
    [214, 214, 222],
    [181, 189, 189],
    [160, 170, 175],
    [148, 156, 165],
    [150, 150, 150],
];

/// Playlist colours and font from `pledit.txt`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaylistStyle {
    pub normal: [u8; 3],
    pub current: [u8; 3],
    pub normal_bg: [u8; 3],
    pub selected_bg: [u8; 3],
    pub font: String,
}

impl Default for PlaylistStyle {
    /// Used for anything a skin's `pledit.txt` omits. Webamp's defaults.
    fn default() -> Self {
        Self {
            normal: [0x00, 0xFF, 0x00],
            current: [0xFF, 0xFF, 0xFF],
            normal_bg: [0x00, 0x00, 0x00],
            selected_bg: [0x00, 0x00, 0xFF],
            font: "Arial".to_string(),
        }
    }
}

/// One closed outline from `region.txt`, in window coordinates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Polygon {
    pub points: Vec<(i32, i32)>,
}

/// `region.txt`: the visible area of each window, as a union of polygons.
///
/// An empty list means "rectangular" — which is most skins, including
/// base-2.91, whose `region.txt` is entirely comments. Whether shaped windows
/// can be rendered portably is a separate question; see the architecture
/// doc §6.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Region {
    pub normal: Vec<Polygon>,
    pub windowshade: Vec<Polygon>,
    pub equalizer: Vec<Polygon>,
    pub equalizer_shade: Vec<Polygon>,
}

/// The colours generic windows draw their controls in, read from a row of
/// swatches along the top of `genex.bmp`, one every 2px from x=48.
///
/// The slots and their meanings are Webamp's `getGenExColors` (MIT). The
/// last four came later than the rest, and older sheets — the stock skin's
/// among them — stop short of them: where a slot holds only the sheet's
/// background, as the separator beside it does, selections fall back to
/// the window's text on the playlist's selection colour, as Winamp draws
/// them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GenExColors {
    /// Behind edits and list views.
    pub item_background: [u8; 3],
    /// Text in edits and list views.
    pub item_foreground: [u8; 3],
    /// The window's own background.
    pub window_background: [u8; 3],
    pub button_text: [u8; 3],
    pub window_text: [u8; 3],
    /// Dividers and sunken borders.
    pub divider: [u8; 3],
    pub playlist_selection: [u8; 3],
    pub list_header_background: [u8; 3],
    pub list_header_text: [u8; 3],
    pub list_header_frame_top_left: [u8; 3],
    pub list_header_frame_bottom_right: [u8; 3],
    pub list_header_frame_pressed: [u8; 3],
    /// Header space to the right of the last column.
    pub list_header_dead_area: [u8; 3],
    pub scrollbar_one: [u8; 3],
    pub scrollbar_two: [u8; 3],
    pub pressed_scrollbar_one: [u8; 3],
    pub pressed_scrollbar_two: [u8; 3],
    pub scrollbar_dead_area: [u8; 3],
    pub list_text_highlighted: [u8; 3],
    pub list_background_highlighted: [u8; 3],
    pub list_text_selected: [u8; 3],
    pub list_background_selected: [u8; 3],
}

impl GenExColors {
    fn read(sheet: &Sheet) -> Self {
        let at = |slot: u32| {
            let [r, g, b, _] = sheet.pixel(48 + slot * 2, 0);
            [r, g, b]
        };
        // A later slot left as background reads the same as its separator.
        let later = |slot: u32, fallback: [u8; 3]| {
            let x = 48 + slot * 2;
            if sheet.pixel(x, 0) == sheet.pixel(x - 1, 0) {
                fallback
            } else {
                at(slot)
            }
        };
        Self {
            item_background: at(0),
            item_foreground: at(1),
            window_background: at(2),
            button_text: at(3),
            window_text: at(4),
            divider: at(5),
            playlist_selection: at(6),
            list_header_background: at(7),
            list_header_text: at(8),
            list_header_frame_top_left: at(9),
            list_header_frame_bottom_right: at(10),
            list_header_frame_pressed: at(11),
            list_header_dead_area: at(12),
            scrollbar_one: at(13),
            scrollbar_two: at(14),
            pressed_scrollbar_one: at(15),
            pressed_scrollbar_two: at(16),
            scrollbar_dead_area: at(17),
            list_text_highlighted: later(18, at(4)),
            list_background_highlighted: later(19, at(6)),
            list_text_selected: later(20, at(4)),
            list_background_selected: later(21, at(6)),
        }
    }
}

/// The title alphabet of `gen.bmp`: A to Z, 7px tall, focused on row 88
/// and unfocused on row 96. Letters vary in width; each ends where the
/// row's background colour (its pixel at x=0) next appears, which is how
/// Webamp finds them (MIT).
///
/// Unlike Webamp, a letter also sheds trailing columns of the sheet's
/// filler (the colour just below the row's start): the stock skin puts no
/// separator after Z, which would otherwise run on into the padding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenFont {
    active: [Rect; 26],
    inactive: [Rect; 26],
}

impl GenFont {
    pub const HEIGHT: u32 = 7;
    /// A space, or anything that is not a letter, advances this far.
    pub const SPACE: u32 = 5;

    fn scan(sheet: &Sheet) -> Self {
        let row = |y: u32| {
            let background = sheet.pixel(0, y);
            let filler = sheet.pixel(0, y + Self::HEIGHT);
            let blank = |x: u32| (y..y + Self::HEIGHT).all(|yy| sheet.pixel(x, yy) == filler);
            let mut x = 1;
            std::array::from_fn(|_| {
                let mut end = x;
                while end < sheet.width && sheet.pixel(end, y) != background {
                    end += 1;
                }
                let next = end + 1;
                while end > x && blank(end - 1) {
                    end -= 1;
                }
                let r = Rect::new(x, y, end - x, Self::HEIGHT);
                x = next;
                r
            })
        };
        Self {
            active: row(88),
            inactive: row(96),
        }
    }

    /// Where `c` is drawn from, case-insensitively; `None` for anything
    /// that is not a letter of the alphabet.
    pub fn letter(&self, c: char, active: bool) -> Option<Rect> {
        let c = c.to_ascii_uppercase();
        if !c.is_ascii_uppercase() {
            return None;
        }
        let rects = if active { &self.active } else { &self.inactive };
        Some(rects[(c as u8 - b'A') as usize])
    }

    /// How wide `text` is drawn, in pixels.
    pub fn width(&self, text: &str, active: bool) -> u32 {
        text.chars()
            .map(|c| self.letter(c, active).map_or(Self::SPACE, |r| r.w))
            .sum()
    }
}

/// Lantern's own generic window, for skins that bring none — original
/// artwork in the manner of the classic base skin, drawn by
/// `examples/make_default_skin.rs`. Winamp similarly falls back to its base
/// skin's.
const FALLBACK_GEN: &[u8] = include_bytes!("../../../assets/fallback/gen.bmp");
const FALLBACK_GENEX: &[u8] = include_bytes!("../../../assets/fallback/genex.bmp");

/// One decoded sprite sheet: RGBA8, row-major, top-left origin.
#[derive(Debug, Clone)]
pub struct Sheet {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl Sheet {
    /// RGBA at `(x, y)`, or fully transparent when out of bounds.
    ///
    /// Out-of-bounds reads happen for real: some skins ship sheets smaller
    /// than the coordinates the format says they should contain.
    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        if x >= self.width || y >= self.height {
            return [0, 0, 0, 0];
        }
        let i = ((y * self.width + x) * 4) as usize;
        [
            self.pixels[i],
            self.pixels[i + 1],
            self.pixels[i + 2],
            self.pixels[i + 3],
        ]
    }
}

/// A fully parsed skin.
#[derive(Debug)]
pub struct Skin {
    sheets: Vec<Option<Sheet>>,
    pub viscolors: VisColors,
    pub playlist_style: PlaylistStyle,
    pub region: Region,
    /// Whether the Numbers slot holds `nums_ex.bmp` rather than `numbers.bmp`.
    nums_ex: bool,
    /// Sheets the skin lacked, filled in with Lantern's own.
    supplied: Vec<SheetId>,
    gen_font: GenFont,
    genex_colors: GenExColors,
}

impl Skin {
    /// Whether the Numbers slot holds `nums_ex.bmp`, whose minus sign is a
    /// full digit cell rather than a dash and so sits elsewhere.
    pub fn has_nums_ex(&self) -> bool {
        self.nums_ex
    }

    /// Where `key` lives in *this* skin.
    ///
    /// Identical to [`sprite::resolve`] except for the minus signs, which
    /// `nums_ex.bmp` places differently from `numbers.bmp`.
    pub fn resolve(&self, key: SpriteKey) -> (SheetId, Rect) {
        if self.nums_ex
            && let Some(rect) = sprite::resolve_nums_ex(key)
        {
            return (SheetId::Numbers, rect);
        }
        sprite::resolve(key)
    }

    /// The decoded sheet for `id`, if the skin supplied one.
    ///
    /// Skins routinely omit sheets; callers fall back rather than fail.
    /// `gen.bmp` and `genex.bmp` are always present: see [`Skin::supplied`].
    pub fn sheet(&self, id: SheetId) -> Option<&Sheet> {
        self.sheets.get(id as usize).and_then(Option::as_ref)
    }

    /// Whether `id` is Lantern's stand-in rather than the skin's own.
    pub fn supplied(&self, id: SheetId) -> bool {
        self.supplied.contains(&id)
    }

    /// The generic windows' title alphabet.
    pub fn gen_font(&self) -> &GenFont {
        &self.gen_font
    }

    /// The generic windows' control colours.
    pub fn genex_colors(&self) -> &GenExColors {
        &self.genex_colors
    }
}

/// Decode one sheet by base name, trying `.bmp` then `.png`.
///
/// The format is sniffed from the bytes rather than trusted from the
/// extension, because skins genuinely ship PNGs named `.bmp`.
fn load_sheet(base: &str, entries: &HashMap<String, Vec<u8>>) -> Option<Sheet> {
    ["bmp", "png"]
        .iter()
        .find_map(|ext| decode_sheet(entries.get(&format!("{base}.{ext}"))?))
}

fn decode_sheet(data: &[u8]) -> Option<Sheet> {
    let rgba = image::load_from_memory(data).ok()?.to_rgba8();
    let (width, height) = rgba.dimensions();
    Some(Sheet {
        width,
        height,
        pixels: rgba.into_raw(),
    })
}

/// Parse a `.wsz` archive.
///
/// Malformed individual entries are tolerated — these files are decades old
/// and frequently slightly broken — but a skin without `main.bmp` cannot be
/// rendered at all and is an error.
pub fn parse(wsz: &[u8]) -> Result<Skin, SkinError> {
    let entries = archive::extract(wsz)?;

    let mut sheets: Vec<Option<Sheet>> = vec![None; SheetId::COUNT];
    for id in SheetId::ALL {
        sheets[id as usize] = load_sheet(id.base_name(), &entries);
    }

    // `nums_ex.bmp` supersedes `numbers.bmp` where both exist, matching
    // Webamp's CSS cascade order.
    let nums_ex = match load_sheet("nums_ex", &entries) {
        Some(ex) => {
            sheets[SheetId::Numbers as usize] = Some(ex);
            true
        }
        None => false,
    };

    if sheets[SheetId::Main as usize].is_none() {
        return Err(SkinError::MissingSheet("main"));
    }

    // Each falls back on its own: a skin may bring one and not the other.
    let mut supplied = Vec::new();
    for (id, fallback) in [
        (SheetId::Gen, FALLBACK_GEN),
        (SheetId::GenEx, FALLBACK_GENEX),
    ] {
        let slot = &mut sheets[id as usize];
        if slot.is_none() {
            *slot = decode_sheet(fallback);
            supplied.push(id);
        }
    }
    let (gen_font, genex_colors) = match (
        &sheets[SheetId::Gen as usize],
        &sheets[SheetId::GenEx as usize],
    ) {
        (Some(gen_sheet), Some(genex)) => (GenFont::scan(gen_sheet), GenExColors::read(genex)),
        _ => unreachable!("the fallbacks are built in and decode"),
    };

    let text_file = |name: &str| entries.get(name).map(|bytes| text::decode(bytes));

    Ok(Skin {
        sheets,
        viscolors: text_file("viscolor.txt")
            .map(|t| text::parse_viscolors(&t, &DEFAULT_VISCOLORS))
            .unwrap_or(DEFAULT_VISCOLORS),
        playlist_style: text_file("pledit.txt")
            .map(|t| text::parse_pledit(&t))
            .unwrap_or_default(),
        region: text_file("region.txt")
            .map(|t| text::parse_region(&t))
            .unwrap_or_default(),
        nums_ex,
        supplied,
        gen_font,
        genex_colors,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Write};

    /// A skin of nothing but a blank `main.bmp`.
    fn bare_skin() -> Vec<u8> {
        let mut bmp = Vec::new();
        image::RgbImage::new(275, 116)
            .write_to(&mut Cursor::new(&mut bmp), image::ImageFormat::Bmp)
            .unwrap();
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        zip.start_file("main.bmp", zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(&bmp).unwrap();
        zip.finish().unwrap().into_inner()
    }

    #[test]
    fn an_older_genex_without_selection_colours_gets_them() {
        // 18 swatches on a cyan sheet, as the stock skin's: slot 4 white,
        // slot 6 blue, the rest grey.
        let (w, h) = (130u32, 75u32);
        let mut pixels = [0u8, 198, 255, 255].repeat((w * h) as usize);
        for slot in 0..18 {
            let c = match slot {
                4 => [255, 255, 255],
                6 => [0, 0, 198],
                _ => [60, 60, 60],
            };
            let i = ((48 + slot * 2) * 4) as usize;
            pixels[i..i + 3].copy_from_slice(&c);
        }
        let c = GenExColors::read(&Sheet {
            width: w,
            height: h,
            pixels,
        });
        assert_eq!(c.list_text_selected, [255, 255, 255]);
        assert_eq!(c.list_background_selected, [0, 0, 198]);
        assert_eq!(c.item_background, [60, 60, 60]);
    }

    #[test]
    fn generic_window_falls_back_to_lantern_s_own() {
        let skin = parse(&bare_skin()).unwrap();
        for id in [SheetId::Gen, SheetId::GenEx] {
            assert!(skin.supplied(id), "{} should be supplied", id.base_name());
            let (w, h) = sprite::standard_size(id);
            let sheet = skin.sheet(id).unwrap();
            assert_eq!((sheet.width, sheet.height), (w, h));
        }
        assert!(!skin.supplied(SheetId::Main));
        // Every letter of the fallback's alphabet was found, with ink in it.
        for active in [true, false] {
            for c in 'A'..='Z' {
                let r = skin.gen_font().letter(c, active).unwrap();
                assert!((3..=8).contains(&r.w), "{c}: {r:?}");
            }
        }
        let colours = skin.genex_colors();
        assert_ne!(colours.item_background, colours.item_foreground);
    }
}
