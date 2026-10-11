//! Classic Winamp geometry, in logical pixels.
//!
//! These are fixed by the skin format. Every sprite sheet in every skin ever
//! made is cut for exactly these dimensions, which is why matching them is
//! what makes the ~25,000 museum skins render correctly with no per-skin work.

/// Every classic window is this wide.
pub const WINDOW_WIDTH: u32 = 275;

/// Main window height.
pub const MAIN_HEIGHT: u32 = 116;

/// Equaliser window height.
pub const EQ_HEIGHT: u32 = 116;

/// Default playlist height. Resizable, in 29px row increments.
pub const PLAYLIST_HEIGHT: u32 = 232;

/// Windowshade ("rolled up") height.
pub const SHADE_HEIGHT: u32 = 14;

/// The title strip is painted into `main.bmp` rows 0..14; the close and
/// minimise affordances are invisible hit zones over it.
pub const TITLE_BAR_HEIGHT: u32 = 14;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

impl Rect {
    pub const fn new(x: i32, y: i32, w: u32, h: u32) -> Self {
        Self { x, y, w, h }
    }

    pub fn contains(&self, px: i32, py: i32) -> bool {
        px >= self.x && py >= self.y && px < self.x + self.w as i32 && py < self.y + self.h as i32
    }
}

#[cfg(test)]
mod tests {
    use super::Rect;

    #[test]
    fn contains_is_half_open() {
        let r = Rect::new(10, 10, 5, 5);
        assert!(r.contains(10, 10));
        assert!(r.contains(14, 14));
        assert!(!r.contains(15, 14), "right edge is exclusive");
        assert!(!r.contains(14, 15), "bottom edge is exclusive");
        assert!(!r.contains(9, 10));
    }
}
