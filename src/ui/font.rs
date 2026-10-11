//! TrueType text for the playlist.
//!
//! Winamp draws the playlist in a real font — `pledit.txt` names it, Arial
//! by default — not the skin's bitmap font. We rasterise the named system
//! font with `fontdue` at physical resolution, so text is sharp on Retina,
//! and fall back through common fonts when the named one is not installed
//! (skins from Windows name Tahoma and Verdana freely).

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;

use fontdue::{Font, FontSettings, Metrics};

use super::blit::Surface;

/// Fonts tried after the skin's own choice, in order.
const FALLBACKS: &[&str] = &[
    "Arial",
    "Helvetica",
    "Geneva",
    "DejaVuSans",
    "LiberationSans-Regular",
    "Verdana",
];

/// Rasterised glyphs by (character, size in 1/64 px).
type GlyphCache = HashMap<(char, u32), (Metrics, Vec<u8>)>;

/// Size and letter spacing, in physical pixels.
#[derive(Debug, Clone, Copy)]
pub struct TextStyle {
    pub px: f32,
    pub spacing: f32,
}

pub struct TextFont {
    font: Font,
    cache: RefCell<GlyphCache>,
}

impl TextFont {
    /// The font `name`, or the first fallback found.
    pub fn load(name: &str) -> Option<Self> {
        std::iter::once(name)
            .chain(FALLBACKS.iter().copied())
            .find_map(|n| font_files(n).into_iter().find_map(|p| Self::from_file(&p)))
    }

    fn from_file(path: &std::path::Path) -> Option<Self> {
        let bytes = std::fs::read(path).ok()?;
        let font = Font::from_bytes(bytes, FontSettings::default()).ok()?;
        Some(Self {
            font,
            cache: RefCell::new(HashMap::new()),
        })
    }

    /// Advance width of `text`.
    pub fn width(&self, text: &str, style: TextStyle) -> f32 {
        text.chars()
            .map(|c| self.font.metrics(c, style.px).advance_width + style.spacing)
            .sum()
    }

    /// `text` shortened to fit `max` pixels, ending in an ellipsis if cut.
    pub fn fit(&self, text: &str, style: TextStyle, max: f32) -> String {
        if self.width(text, style) <= max {
            return text.to_string();
        }
        let ellipsis = self.width("…", style);
        let mut out = String::new();
        let mut used = 0.0;
        for c in text.chars() {
            let w = self.font.metrics(c, style.px).advance_width + style.spacing;
            if used + w + ellipsis > max {
                break;
            }
            used += w;
            out.push(c);
        }
        out.push('…');
        out
    }

    /// Draw `text` with its baseline at physical `(x, baseline)`, blending
    /// coverage over what is already there. Honours the surface's clip.
    pub fn draw(
        &self,
        dst: &mut Surface<'_>,
        text: &str,
        (x, baseline): (f32, i32),
        style: TextStyle,
        rgb: u32,
    ) {
        let mut pen = x;
        let mut cache = self.cache.borrow_mut();
        for c in text.chars() {
            let key = (c, (style.px * 64.0) as u32);
            let (m, bitmap) = cache
                .entry(key)
                .or_insert_with(|| self.font.rasterize(c, style.px));
            let left = pen.round() as i32 + m.xmin;
            let top = baseline - m.ymin - m.height as i32;
            for row in 0..m.height {
                for col in 0..m.width {
                    let coverage = bitmap[row * m.width + col];
                    if coverage > 0 {
                        dst.blend(left + col as i32, top + row as i32, rgb, coverage);
                    }
                }
            }
            pen += m.advance_width + style.spacing;
        }
    }
}

/// Where a font called `name` might live on this system.
fn font_files(name: &str) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if cfg!(target_os = "macos") {
        dirs.extend(
            [
                "/System/Library/Fonts/Supplemental",
                "/System/Library/Fonts",
                "/Library/Fonts",
            ]
            .map(PathBuf::from),
        );
        if let Some(home) = std::env::var_os("HOME") {
            dirs.push(PathBuf::from(home).join("Library/Fonts"));
        }
    } else if cfg!(windows) {
        dirs.push(PathBuf::from(r"C:\Windows\Fonts"));
    } else {
        dirs.extend(
            [
                "/usr/share/fonts/truetype/msttcorefonts",
                "/usr/share/fonts/truetype/dejavu",
                "/usr/share/fonts/truetype/liberation",
                "/usr/share/fonts/TTF",
            ]
            .map(PathBuf::from),
        );
    }
    let names = [name.to_string(), name.to_lowercase()];
    dirs.iter()
        .flat_map(|d| {
            names
                .iter()
                .flat_map(move |n| ["ttf", "ttc", "otf"].map(|ext| d.join(format!("{n}.{ext}"))))
        })
        .filter(|p| p.exists())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn some_font_loads_and_measures() {
        // Every supported platform ships at least one fallback.
        let Some(font) = TextFont::load("No Such Font Anywhere") else {
            eprintln!("SKIPPED: no system font found");
            return;
        };
        let style = TextStyle {
            px: 9.0,
            spacing: 0.0,
        };
        let w = font.width("Winamp", style);
        assert!(w > 10.0 && w < 60.0, "width {w}");
        let fitted = font.fit("A rather long track title indeed", style, 40.0);
        assert!(fitted.ends_with('…'));
        assert!(font.width(&fitted, style) <= 40.0);
    }

    #[test]
    fn drawing_marks_pixels() {
        let Some(font) = TextFont::load("Arial") else {
            return;
        };
        let mut px = vec![0u32; 40 * 16];
        let style = TextStyle {
            px: 12.0,
            spacing: 0.0,
        };
        font.draw(
            &mut Surface::new(&mut px, 40, 16),
            "Hi",
            (1.0, 12),
            style,
            0xFFFFFF,
        );
        assert!(px.iter().any(|&p| p != 0));
    }
}
