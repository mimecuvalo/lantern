//! Sprite blitting into a softbuffer surface.
//!
//! The surface is `&mut [u32]` in `0x00RRGGBB`. At 275x116 that is 31,900
//! pixels — 128 KB — so a full repaint every frame costs roughly 7.6 MB/s of
//! memcpy at 60fps. **There is deliberately no dirty-rectangle tracking.**
//! Repainting everything is cheap enough to be free, and it deletes an entire
//! class of invalidation bugs that the previous implementation spent commits
//! chasing.
//!
//! Scaling is integer nearest-neighbour and nothing else. Double-size is
//! `scale = 2`. Because there is no filtering path at all, the resampling
//! artifacts the Swift version had to correct with explicit pixel-snapping
//! cannot occur here by construction.

use lantern_skin::{Rect, Sheet};

/// Destination surface: `0x00RRGGBB`, row-major, `width * height` long.
pub struct Surface<'a> {
    pub pixels: &'a mut [u32],
    pub width: u32,
    pub height: u32,
    /// Physical-pixel rectangle writes are confined to, `[x0, x1) x [y0, y1)`.
    clip: (i32, i32, i32, i32),
}

impl<'a> Surface<'a> {
    pub fn new(pixels: &'a mut [u32], width: u32, height: u32) -> Self {
        Self {
            pixels,
            width,
            height,
            clip: (0, 0, width as i32, height as i32),
        }
    }

    /// Confine drawing to a logical rectangle at `scale` — the marquee's
    /// window onto its scrolling text — or lift the confinement with `None`.
    pub fn set_clip(&mut self, clip: Option<Rect>, scale: u32) {
        let s = scale.max(1) as i32;
        self.clip = match clip {
            Some(r) => (
                r.x as i32 * s,
                r.y as i32 * s,
                (r.x + r.w) as i32 * s,
                (r.y + r.h) as i32 * s,
            ),
            None => (0, 0, self.width as i32, self.height as i32),
        };
        self.clip.2 = self.clip.2.min(self.width as i32);
        self.clip.3 = self.clip.3.min(self.height as i32);
    }

    /// Mix `rgb` over the pixel at `(x, y)` with `alpha` (0..=255), within
    /// the clip. For antialiased text.
    #[inline]
    pub fn blend(&mut self, x: i32, y: i32, rgb: u32, alpha: u8) {
        let (x0, y0, x1, y1) = self.clip;
        if x < x0.max(0) || y < y0.max(0) || x >= x1 || y >= y1 {
            return;
        }
        let i = y as usize * self.width as usize + x as usize;
        let (a, dst) = (u32::from(alpha), self.pixels[i]);
        let mix = |shift: u32| {
            let s = (rgb >> shift) & 0xFF;
            let d = (dst >> shift) & 0xFF;
            ((s * a + d * (255 - a)) / 255) << shift
        };
        self.pixels[i] = mix(16) | mix(8) | mix(0);
    }

    #[inline]
    fn put(&mut self, x: i32, y: i32, colour: u32) {
        let (x0, y0, x1, y1) = self.clip;
        if x < x0.max(0) || y < y0.max(0) || x >= x1 || y >= y1 {
            return;
        }
        self.pixels[y as usize * self.width as usize + x as usize] = colour;
    }
}

/// Blit `src` from `sheet` to `(dx, dy)`, magnified `scale`x.
///
/// Fully transparent source pixels are skipped, so sprites composite over
/// whatever is already there. Destination coordinates are clipped, which
/// matters because some skins ship sheets smaller than the format implies.
pub fn blit(dst: &mut Surface<'_>, sheet: &Sheet, src: Rect, dx: i32, dy: i32, scale: u32) {
    let s = scale.max(1) as i32;
    let (x0, y0, x1, y1) = dst.clip;
    let (x0, y0) = (x0.max(0), y0.max(0));
    let stride = dst.width as usize;
    // Rows and columns past the sheet's edge read as transparent, so stop
    // there rather than asking for each pixel.
    let rows = src.h.min(sheet.height.saturating_sub(src.y));
    let cols = src.w.min(sheet.width.saturating_sub(src.x));
    for sy in 0..rows {
        let top = dy + sy as i32 * s;
        let (ya, yb) = (top.max(y0), (top + s).min(y1));
        if ya >= yb {
            continue;
        }
        let row = ((src.y + sy) * sheet.width + src.x) as usize * 4;
        for sx in 0..cols {
            let left = dx + sx as i32 * s;
            let (xa, xb) = (left.max(x0), (left + s).min(x1));
            if xa >= xb {
                continue;
            }
            let p = &sheet.pixels[row + sx as usize * 4..][..4];
            if p[3] == 0 {
                continue;
            }
            let colour = u32::from(p[0]) << 16 | u32::from(p[1]) << 8 | u32::from(p[2]);
            for y in ya..yb {
                let line = y as usize * stride;
                dst.pixels[line + xa as usize..line + xb as usize].fill(colour);
            }
        }
    }
}

/// Fill a logical rectangle with one colour, magnified `scale`x. Used for
/// the things skins describe by colour rather than bitmap: the analyser.
pub fn fill_rect(dst: &mut Surface<'_>, x: i32, y: i32, w: u32, h: u32, rgb: u32, scale: u32) {
    let s = scale.max(1) as i32;
    for py in y * s..(y + h as i32) * s {
        for px in x * s..(x + w as i32) * s {
            dst.put(px, py, rgb);
        }
    }
}

/// Fill the whole surface with one colour.
pub fn clear(dst: &mut Surface<'_>, rgb: u32) {
    dst.pixels.fill(rgb);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2x2 sheet: opaque red, opaque green, transparent, opaque blue.
    fn sheet() -> Sheet {
        Sheet {
            width: 2,
            height: 2,
            pixels: vec![
                255, 0, 0, 255, // (0,0) red
                0, 255, 0, 255, // (1,0) green
                0, 0, 0, 0, // (0,1) transparent
                0, 0, 255, 255, // (1,1) blue
            ],
        }
    }

    fn surface(w: u32, h: u32) -> Vec<u32> {
        vec![0; (w * h) as usize]
    }

    #[test]
    fn blits_at_scale_one() {
        let mut px = surface(2, 2);
        let mut s = Surface::new(&mut px, 2, 2);
        blit(&mut s, &sheet(), Rect::new(0, 0, 2, 2), 0, 0, 1);
        assert_eq!(px[0], 0xFF0000);
        assert_eq!(px[1], 0x00FF00);
        assert_eq!(px[2], 0, "transparent source must not be written");
        assert_eq!(px[3], 0x0000FF);
    }

    #[test]
    fn scale_two_expands_each_pixel_to_a_block() {
        let mut px = surface(4, 4);
        let mut s = Surface::new(&mut px, 4, 4);
        blit(&mut s, &sheet(), Rect::new(0, 0, 2, 2), 0, 0, 2);
        // The red pixel becomes a 2x2 block at the origin.
        for (x, y) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
            assert_eq!(px[y * 4 + x], 0xFF0000, "block pixel ({x},{y})");
        }
        // The transparent pixel leaves its whole block untouched.
        for (x, y) in [(0, 2), (1, 2), (0, 3), (1, 3)] {
            assert_eq!(px[y * 4 + x], 0, "transparent block ({x},{y})");
        }
    }

    #[test]
    fn clips_negative_and_overflowing_destinations() {
        let mut px = surface(2, 2);
        let mut s = Surface::new(&mut px, 2, 2);
        // Mostly off the top-left; only the bottom-right source pixel lands.
        blit(&mut s, &sheet(), Rect::new(0, 0, 2, 2), -1, -1, 1);
        // Entirely off the far edge: must not panic, must not write.
        blit(&mut s, &sheet(), Rect::new(0, 0, 2, 2), 50, 50, 1);
        assert_eq!(px[0], 0x0000FF);
        assert_eq!(px[1..], [0, 0, 0]);
    }

    #[test]
    fn clip_confines_writes_at_scale() {
        let mut px = surface(8, 8);
        let mut s = Surface::new(&mut px, 8, 8);
        // Logical (1,1) 1x1 at scale 2 is physical (2..4, 2..4).
        s.set_clip(Some(Rect::new(1, 1, 1, 1)), 2);
        fill_rect(&mut s, 0, 0, 4, 4, 0xFFFFFF, 2);
        let lit: Vec<usize> = (0..64).filter(|&i| px[i] != 0).collect();
        assert_eq!(lit, vec![18, 19, 26, 27]);
    }

    #[test]
    fn out_of_bounds_source_reads_are_transparent_not_panics() {
        let mut px = surface(4, 4);
        let mut s = Surface::new(&mut px, 4, 4);
        // Ask for a 4x4 region from a 2x2 sheet.
        blit(&mut s, &sheet(), Rect::new(0, 0, 4, 4), 0, 0, 1);
        assert_eq!(px[15], 0, "reads past the sheet must be transparent");
    }
}
