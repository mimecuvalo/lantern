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
    pub fn sheet(&self, id: SheetId) -> Option<&Sheet> {
        self.sheets.get(id as usize).and_then(Option::as_ref)
    }
}

/// Decode one sheet by base name, trying `.bmp` then `.png`.
///
/// The format is sniffed from the bytes rather than trusted from the
/// extension, because skins genuinely ship PNGs named `.bmp`.
fn load_sheet(base: &str, entries: &HashMap<String, Vec<u8>>) -> Option<Sheet> {
    for ext in ["bmp", "png"] {
        let Some(data) = entries.get(&format!("{base}.{ext}")) else {
            continue;
        };
        let Ok(img) = image::load_from_memory(data) else {
            continue;
        };
        let rgba = img.to_rgba8();
        let (width, height) = rgba.dimensions();
        return Some(Sheet {
            width,
            height,
            pixels: rgba.into_raw(),
        });
    }
    None
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
    })
}
