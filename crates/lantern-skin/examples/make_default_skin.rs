//! Generates Lantern's own skins: `assets/default-skin.wsz`, the one built
//! into the app — Night: stars, a moon, glass controls — and the other
//! themes in `assets/skins/`, which Load Skin… opens like any other.
//!
//! Every pixel is drawn here, in code, so the art is original and can ship
//! (Winamp's own `base-2.91` cannot; see `docs/legal.md`). Themes share the
//! sprite drawing and differ in a style (how surfaces are built), a mood
//! (the night sky's variations), a palette of colour roles, and what lives
//! in them. The 4x5 pixel font is ours, and the same in every theme.
//!
//! Output is deterministic: rerunning produces a byte-identical file.
//!
//! ```bash
//! cargo run -p lantern-skin --example make_default_skin
//! ```

use std::io::{Cursor, Write};
use std::path::PathBuf;

use image::{ImageFormat, Rgb, RgbImage};
use lantern_skin::SheetId;
use lantern_skin::sprite::{
    self, ClutterButton, PlaylistButton, PlaylistMenu, ShadeThumb, SpriteKey,
};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipWriter};

type C = [u8; 3];

// -- Themes -------------------------------------------------------------------

/// Colours by role, so every sprite works in every theme: `void` is the
/// display background, `s0`-`s4` stone from shadow to highlight, `rim` the
/// light stone catches, `moss*` the flora, `cyan*` whatever is lit (bright to
/// faint), `pink*` and `amber*` the accents, `spark` the sparkles, `white` the
/// strongest highlight.
#[derive(Clone, Copy)]
struct Palette {
    void: C,
    s0: C,
    s1: C,
    s2: C,
    s3: C,
    s4: C,
    rim: C,
    rim_hi: C,
    moss0: C,
    moss1: C,
    moss2: C,
    cyan: C,
    cyan1: C,
    cyan2: C,
    cyan3: C,
    pink: C,
    pink1: C,
    amber: C,
    amber1: C,
    spark: C,
    white: C,
}

/// How surfaces are built: the backdrop, recesses and raised controls.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Style {
    /// Rim-lit brick.
    Bricks,
    /// Large dressed stone panels, carved edges.
    Temple,
    /// No stone: a dithered night sky, glass controls.
    Night,
    /// Flat navy, a grid, glowing outlines.
    Wireframe,
    /// Pure red and blue on black, for chromostereopsis: red floats in
    /// front of blue, so what is raised is red and what is pressed or quiet
    /// sinks into blue.
    Chroma,
}

/// Variations on the night sky: what is in the air, on the ground, and who
/// peeks over the display. `Plain` for every other style.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mood {
    Plain,
    Aurora,
    Dusk,
    Fireflies,
}

/// A direction for the skin: how it is built, its colours, and what lives
/// in it. The fonts are the same in all of them.
struct Theme {
    /// File name, under `skins/` for all but the default.
    name: &'static str,
    style: Style,
    mood: Mood,
    palette: Palette,
    /// Ferns, vines, moss.
    flora: bool,
    /// Fireflies and twinkles.
    sparkles: bool,
    /// The peeking critter, the eyes in the wall, the ghost koi.
    creatures: bool,
    /// Soft halos around lit things.
    glow: bool,
    /// Every other row darkened, as on a CRT.
    scanlines: bool,
}

const THEMES: [Theme; 8] = [
    // Open night sky: no stone at all; stars, a moon, glass controls. The
    // default, built in.
    Theme {
        name: "night",
        style: Style::Night,
        mood: Mood::Plain,
        palette: Palette {
            void: [2, 2, 10],
            s0: [4, 5, 18],
            s1: [8, 10, 30],
            s2: [14, 18, 48],
            s3: [26, 34, 86],
            s4: [52, 66, 150],
            rim: [112, 132, 255],
            rim_hi: [204, 214, 255],
            moss0: [8, 52, 62],
            moss1: [18, 110, 112],
            moss2: [56, 226, 206],
            cyan: [64, 244, 232],
            cyan1: [34, 160, 176],
            cyan2: [16, 78, 100],
            cyan3: [8, 36, 56],
            pink: [255, 120, 200],
            pink1: [150, 52, 120],
            amber: [255, 214, 96],
            amber1: [160, 110, 34],
            spark: [240, 240, 200],
            white: [230, 246, 255],
        },
        flora: false,
        sparkles: true,
        creatures: true,
        glow: true,
        scanlines: false,
    },
    // A cave: rim-lit brick, ferns and vines, things peeking.
    Theme {
        name: "cave",
        style: Style::Bricks,
        mood: Mood::Plain,
        palette: Palette {
            void: [3, 4, 14],
            s0: [8, 10, 30],
            s1: [12, 16, 46],
            s2: [18, 24, 66],
            s3: [30, 42, 108],
            s4: [64, 84, 200],
            rim: [96, 118, 255],
            rim_hi: [158, 176, 255],
            moss0: [8, 52, 62],
            moss1: [18, 110, 112],
            moss2: [56, 226, 206],
            cyan: [64, 244, 232],
            cyan1: [34, 160, 176],
            cyan2: [16, 78, 100],
            cyan3: [8, 36, 56],
            pink: [255, 120, 200],
            pink1: [150, 52, 120],
            amber: [255, 214, 96],
            amber1: [160, 110, 34],
            spark: [255, 236, 120],
            white: [230, 246, 255],
        },
        flora: true,
        sparkles: true,
        creatures: true,
        glow: true,
        scanlines: false,
    },
    // A sunken temple: dressed stone panels, fluted pillars, a carved frieze.
    Theme {
        name: "temple",
        style: Style::Temple,
        mood: Mood::Plain,
        palette: Palette {
            void: [3, 4, 14],
            s0: [14, 16, 34],
            s1: [24, 28, 56],
            s2: [36, 42, 80],
            s3: [58, 68, 122],
            s4: [100, 116, 200],
            rim: [150, 170, 255],
            rim_hi: [214, 224, 255],
            moss0: [8, 52, 62],
            moss1: [18, 110, 112],
            moss2: [56, 226, 206],
            cyan: [64, 244, 232],
            cyan1: [34, 160, 176],
            cyan2: [16, 78, 100],
            cyan3: [8, 36, 56],
            pink: [255, 120, 200],
            pink1: [150, 52, 120],
            amber: [255, 214, 96],
            amber1: [160, 110, 34],
            spark: [255, 236, 120],
            white: [230, 246, 255],
        },
        flora: false,
        sparkles: true,
        creatures: true,
        glow: true,
        scanlines: false,
    },
    // A hologram schematic: grid, glowing outlines, corner brackets, scanlines.
    Theme {
        name: "wireframe",
        style: Style::Wireframe,
        mood: Mood::Plain,
        palette: Palette {
            void: [2, 6, 16],
            s0: [4, 10, 26],
            s1: [8, 18, 42],
            s2: [12, 30, 64],
            s3: [20, 50, 100],
            s4: [40, 100, 170],
            rim: [60, 200, 255],
            rim_hi: [160, 240, 255],
            moss0: [8, 52, 62],
            moss1: [18, 110, 112],
            moss2: [56, 226, 206],
            cyan: [96, 232, 255],
            cyan1: [40, 150, 204],
            cyan2: [20, 80, 124],
            cyan3: [10, 36, 62],
            pink: [255, 120, 200],
            pink1: [150, 52, 120],
            amber: [255, 214, 96],
            amber1: [160, 110, 34],
            spark: [255, 236, 120],
            white: [230, 246, 255],
        },
        flora: false,
        sparkles: false,
        creatures: false,
        glow: true,
        scanlines: true,
    },
    // Night, with aurora curtains over snow-capped peaks; an owl; teal glass.
    Theme {
        name: "aurora",
        style: Style::Night,
        mood: Mood::Aurora,
        palette: Palette {
            void: [1, 4, 10],
            s0: [3, 8, 18],
            s1: [6, 14, 30],
            s2: [10, 24, 44],
            s3: [20, 46, 78],
            s4: [40, 90, 130],
            rim: [80, 220, 190],
            rim_hi: [170, 255, 220],
            moss0: [8, 52, 62],
            moss1: [30, 140, 100],
            moss2: [80, 255, 160],
            cyan: [90, 250, 200],
            cyan1: [40, 170, 140],
            cyan2: [18, 80, 74],
            cyan3: [8, 36, 36],
            pink: [190, 120, 255],
            pink1: [90, 50, 140],
            amber: [255, 214, 96],
            amber1: [160, 110, 34],
            spark: [220, 255, 240],
            white: [230, 246, 255],
        },
        flora: false,
        sparkles: true,
        creatures: true,
        glow: true,
        scanlines: false,
    },
    // Night falling: indigo over a magenta horizon, hills, a setting sun; a fox.
    Theme {
        name: "dusk",
        style: Style::Night,
        mood: Mood::Dusk,
        palette: Palette {
            void: [6, 4, 22],
            s0: [16, 10, 44],
            s1: [40, 20, 72],
            s2: [104, 44, 96],
            s3: [70, 40, 110],
            s4: [130, 90, 190],
            rim: [255, 150, 170],
            rim_hi: [255, 212, 192],
            moss0: [8, 52, 62],
            moss1: [18, 110, 112],
            moss2: [56, 226, 206],
            cyan: [255, 196, 150],
            cyan1: [200, 122, 112],
            cyan2: [100, 50, 72],
            cyan3: [40, 20, 42],
            pink: [255, 120, 160],
            pink1: [140, 50, 90],
            amber: [255, 190, 90],
            amber1: [160, 110, 34],
            spark: [255, 230, 170],
            white: [255, 240, 230],
        },
        flora: false,
        sparkles: true,
        creatures: true,
        glow: true,
        scanlines: false,
    },
    // A summer night: tall grass, a swarm of fireflies, a firefly jar; a field mouse.
    Theme {
        name: "fireflies",
        style: Style::Night,
        mood: Mood::Fireflies,
        palette: Palette {
            void: [2, 4, 10],
            s0: [4, 8, 18],
            s1: [7, 14, 28],
            s2: [12, 22, 40],
            s3: [24, 40, 70],
            s4: [50, 80, 130],
            rim: [110, 150, 230],
            rim_hi: [200, 220, 255],
            moss0: [6, 16, 10],
            moss1: [14, 40, 24],
            moss2: [40, 96, 52],
            cyan: [200, 255, 120],
            cyan1: [120, 180, 70],
            cyan2: [50, 80, 34],
            cyan3: [20, 34, 16],
            pink: [255, 140, 190],
            pink1: [150, 52, 120],
            amber: [255, 214, 96],
            amber1: [160, 110, 34],
            spark: [230, 255, 140],
            white: [230, 246, 255],
        },
        flora: false,
        sparkles: true,
        creatures: true,
        glow: true,
        scanlines: false,
    },
    // Chromostereopsis: red line-work standing off black, and blue rays
    // far behind it, bursting from the lantern; pressed buttons, cut EQ
    // bands and quiet bars fall back into the blue. The EQ docked below
    // carries on the burst, so the two read as one piece.
    Theme {
        name: "chroma",
        style: Style::Chroma,
        mood: Mood::Plain,
        palette: Palette {
            void: [0, 0, 0],
            s0: [0, 0, 0],
            s1: [0, 0, 150],
            s2: [0, 0, 255],
            s3: [56, 56, 255],
            s4: [120, 120, 255],
            rim: [255, 0, 0],
            rim_hi: [255, 110, 110],
            moss0: [0, 0, 84],
            moss1: [0, 0, 170],
            moss2: [0, 0, 255],
            cyan: [255, 0, 0],
            cyan1: [214, 0, 0],
            cyan2: [0, 0, 170],
            cyan3: [0, 0, 84],
            pink: [0, 0, 255],
            pink1: [0, 0, 150],
            amber: [255, 255, 255],
            amber1: [255, 0, 0],
            spark: [255, 255, 255],
            white: [255, 255, 255],
        },
        flora: false,
        sparkles: false,
        creatures: false,
        glow: false,
        scanlines: false,
    },
];

thread_local! {
    static THEME: std::cell::Cell<&'static Theme> = const { std::cell::Cell::new(&THEMES[0]) };
}

fn theme() -> &'static Theme {
    THEME.with(std::cell::Cell::get)
}

fn pal() -> Palette {
    theme().palette
}
fn mix(a: C, b: C, t: f32) -> C {
    let t = t.clamp(0.0, 1.0);
    std::array::from_fn(|i| (f32::from(a[i]) * (1.0 - t) + f32::from(b[i]) * t).round() as u8)
}

/// A stable pseudo-random number per pixel, so textures are deterministic.
fn hash(x: i32, y: i32, seed: u32) -> u32 {
    let mut h = (x as u32).wrapping_mul(0x9E37_79B1)
        ^ (y as u32).wrapping_mul(0x85EB_CA77)
        ^ seed.wrapping_mul(0xC2B2_AE3D);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    h = h.wrapping_mul(0x297A_2D39);
    h ^ (h >> 15)
}

// -- Font ---------------------------------------------------------------------

/// Our 4x5 pixel font. Every glyph sits in the top-left of a 5x6 cell.
fn glyph(c: char) -> [&'static str; 5] {
    match c.to_ascii_uppercase() {
        'A' => [".##.", "#..#", "####", "#..#", "#..#"],
        'B' => ["###.", "#..#", "###.", "#..#", "###."],
        'C' => [".###", "#...", "#...", "#...", ".###"],
        'D' => ["###.", "#..#", "#..#", "#..#", "###."],
        'E' => ["####", "#...", "###.", "#...", "####"],
        'F' => ["####", "#...", "###.", "#...", "#..."],
        'G' => [".###", "#...", "#.##", "#..#", ".###"],
        'H' => ["#..#", "#..#", "####", "#..#", "#..#"],
        'I' => ["###.", ".#..", ".#..", ".#..", "###."],
        'J' => ["..##", "...#", "...#", "#..#", ".##."],
        'K' => ["#..#", "#.#.", "##..", "#.#.", "#..#"],
        'L' => ["#...", "#...", "#...", "#...", "####"],
        'M' => ["#..#", "####", "####", "#..#", "#..#"],
        'N' => ["#..#", "##.#", "#.##", "#..#", "#..#"],
        'O' => [".##.", "#..#", "#..#", "#..#", ".##."],
        'P' => ["###.", "#..#", "###.", "#...", "#..."],
        'Q' => [".##.", "#..#", "#..#", "#.#.", ".#.#"],
        'R' => ["###.", "#..#", "###.", "#.#.", "#..#"],
        'S' => [".###", "#...", ".##.", "...#", "###."],
        'T' => ["###.", ".#..", ".#..", ".#..", ".#.."],
        'U' => ["#..#", "#..#", "#..#", "#..#", ".##."],
        'V' => ["#.#.", "#.#.", "#.#.", "#.#.", ".#.."],
        'W' => ["#..#", "#..#", "####", "####", "#..#"],
        'X' => ["#..#", "#..#", ".##.", "#..#", "#..#"],
        'Y' => ["#.#.", "#.#.", ".#..", ".#..", ".#.."],
        'Z' => ["####", "..#.", ".#..", "#...", "####"],
        '0' => [".##.", "#.##", "##.#", "#..#", ".##."],
        '1' => [".#..", "##..", ".#..", ".#..", "###."],
        '2' => ["###.", "...#", ".##.", "#...", "####"],
        '3' => ["###.", "...#", ".##.", "...#", "###."],
        '4' => ["#..#", "#..#", "####", "...#", "...#"],
        '5' => ["####", "#...", "###.", "...#", "###."],
        '6' => [".##.", "#...", "###.", "#..#", ".##."],
        '7' => ["####", "...#", "..#.", ".#..", ".#.."],
        '8' => [".##.", "#..#", ".##.", "#..#", ".##."],
        '9' => [".##.", "#..#", ".###", "...#", ".##."],
        '"' => ["#.#.", "#.#.", "....", "....", "...."],
        '@' => [".##.", "#.##", "#.##", "#...", ".##."],
        '\u{2026}' => ["....", "....", "....", "....", "#.#."],
        '.' => ["....", "....", "....", "....", ".#.."],
        ':' => ["....", ".#..", "....", ".#..", "...."],
        '(' => ["..#.", ".#..", ".#..", ".#..", "..#."],
        ')' => [".#..", "..#.", "..#.", "..#.", ".#.."],
        '-' => ["....", "....", "###.", "....", "...."],
        '\'' => [".#..", ".#..", "....", "....", "...."],
        '!' => [".#..", ".#..", ".#..", "....", ".#.."],
        '_' => ["....", "....", "....", "....", "####"],
        '+' => ["....", ".#..", "###.", ".#..", "...."],
        '\\' => ["#...", "#...", ".#..", "..#.", "..#."],
        '/' => ["..#.", "..#.", ".#..", "#...", "#..."],
        '[' => [".##.", ".#..", ".#..", ".#..", ".##."],
        ']' => [".##.", "..#.", "..#.", "..#.", ".##."],
        '^' => [".#..", "#.#.", "....", "....", "...."],
        '&' => [".#..", "#.#.", ".#..", "#.#.", ".#.#"],
        '%' => ["#..#", "..#.", ".#..", "#...", "#..#"],
        ',' => ["....", "....", "....", ".#..", "#..."],
        '=' => ["....", "###.", "....", "###.", "...."],
        '$' => [".###", "##..", ".##.", "..##", "###."],
        '#' => ["#.#.", "####", "#.#.", "####", "#.#."],
        'Å' => [".##.", ".##.", "#..#", "####", "#..#"],
        'Ö' => ["#..#", ".##.", "#..#", "#..#", ".##."],
        'Ä' => ["#..#", ".##.", "#..#", "####", "#..#"],
        '?' => ["###.", "...#", ".##.", "....", ".#.."],
        '*' => ["....", "#.#.", ".#..", "#.#.", "...."],
        _ => ["....", "....", "....", "....", "...."],
    }
}

/// Width of `s` in the 4x5 font, with a 1px gap between letters.
fn text_width(s: &str) -> i32 {
    (s.chars().count() as i32 * 5 - 1).max(0)
}

// -- Canvas ---------------------------------------------------------------------

struct Canvas(RgbImage);

impl Canvas {
    fn new(w: u32, h: u32) -> Self {
        Self(RgbImage::from_pixel(w, h, Rgb(pal().void)))
    }

    fn px(&mut self, x: i32, y: i32, c: C) {
        if x >= 0 && y >= 0 && (x as u32) < self.0.width() && (y as u32) < self.0.height() {
            self.0.put_pixel(x as u32, y as u32, Rgb(c));
        }
    }

    fn get(&self, x: i32, y: i32) -> C {
        if x >= 0 && y >= 0 && (x as u32) < self.0.width() && (y as u32) < self.0.height() {
            self.0.get_pixel(x as u32, y as u32).0
        } else {
            pal().void
        }
    }

    /// Blend `c` over what is there.
    fn tint(&mut self, x: i32, y: i32, c: C, t: f32) {
        let under = self.get(x, y);
        self.px(x, y, mix(under, c, t));
    }

    fn fill(&mut self, x: i32, y: i32, w: i32, h: i32, c: C) {
        for yy in y..y + h {
            for xx in x..x + w {
                self.px(xx, yy, c);
            }
        }
    }

    /// A 1px frame: `tl` along the top and left, `br` along the bottom and
    /// right.
    fn frame(&mut self, x: i32, y: i32, w: i32, h: i32, tl: C, br: C) {
        for xx in x..x + w {
            self.px(xx, y, tl);
            self.px(xx, y + h - 1, br);
        }
        for yy in y..y + h {
            self.px(x, yy, tl);
            self.px(x + w - 1, yy, br);
        }
    }

    /// The surface everything sits on, as the theme's style builds it.
    fn backdrop(&mut self, x: i32, y: i32, w: i32, h: i32, seed: u32) {
        match theme().style {
            Style::Bricks => self.bricks(x, y, w, h, seed),
            Style::Temple => self.ashlar(x, y, w, h, seed),
            Style::Night => self.sky(x, y, w, h, seed),
            Style::Wireframe => self.grid(x, y, w, h),
            Style::Chroma => self.fill(x, y, w, h, pal().void),
        }
    }

    /// Chroma's burst: blue rays and rings from the main window's lantern,
    /// into a window piece at `(x, y, w, h)` whose top is `below` pixels
    /// under the main window's top, so the EQ docked beneath carries it on.
    fn burst(&mut self, x: i32, y: i32, w: i32, h: i32, below: i32) {
        if theme().style != Style::Chroma {
            return;
        }
        let blue = pal().s2;
        let (cx, cy) = (x + 259, y + 96 - below);
        let clip = (x, y, w, h);
        rays(self, cx, cy, (12.0, 400.0), 36, blue, clip);
        for r in [24.0f32, 42.0, 66.0, 96.0, 130.0] {
            let steps = (r * 8.0) as i32;
            for k in 0..steps {
                let a = k as f32 / steps as f32 * std::f32::consts::TAU;
                let (px, py) = (
                    cx + (r * a.cos()).round() as i32,
                    cy + (r * a.sin()).round() as i32,
                );
                if (x..x + w).contains(&px) && (y..y + h).contains(&py) {
                    self.px(px, py, blue);
                }
            }
        }
    }

    /// Big dressed stone panels, each with an engraved border.
    fn ashlar(&mut self, x: i32, y: i32, w: i32, h: i32, seed: u32) {
        const BW: i32 = 34;
        const BH: i32 = 17;
        let p = pal();
        for yy in y..y + h {
            for xx in x..x + w {
                let row = yy.div_euclid(BH);
                let shift = if row % 2 == 0 { 0 } else { BW / 2 };
                let col = (xx + shift).div_euclid(BW);
                let (bx, by) = ((xx + shift).rem_euclid(BW), yy.rem_euclid(BH));
                let base = if hash(col, row, seed).is_multiple_of(2) {
                    p.s2
                } else {
                    mix(p.s2, p.s1, 0.4)
                };
                let lit = hash(col, row, seed ^ 0x3C).is_multiple_of(3);
                let c = if bx == 0 || by == 0 {
                    p.s0
                } else if by == 1 {
                    if lit { p.rim } else { mix(base, p.s4, 0.35) }
                } else if bx == 1 {
                    mix(base, p.s4, 0.25)
                } else if by == BH - 1 || bx == BW - 1 {
                    mix(base, p.s0, 0.5)
                } else if (by == 4 || by == BH - 4) && (4..BW - 4).contains(&bx)
                    || (bx == 4 || bx == BW - 4) && (4..BH - 3).contains(&by)
                {
                    // The engraving: a cut line, lit along its lower edge.
                    p.s1
                } else if (by == 5 && (5..BW - 4).contains(&bx))
                    || (bx == 5 && (5..BH - 3).contains(&by))
                {
                    mix(base, p.s4, 0.2)
                } else if bx % 3 == 0 {
                    // Faint tooling marks.
                    mix(base, p.s3, 0.12)
                } else {
                    base
                };
                self.px(xx, yy, c);
            }
        }
    }

    /// A night sky: a dithered gradient, darker overhead, scattered stars.
    fn sky(&mut self, x: i32, y: i32, w: i32, h: i32, seed: u32) {
        const BAYER: [[f32; 4]; 4] = [
            [0.0, 8.0, 2.0, 10.0],
            [12.0, 4.0, 14.0, 6.0],
            [3.0, 11.0, 1.0, 9.0],
            [15.0, 7.0, 13.0, 5.0],
        ];
        let p = pal();
        for yy in y..y + h {
            for xx in x..x + w {
                let t = (yy - y) as f32 / h.max(1) as f32;
                let level = t * 3.0;
                let threshold = BAYER[yy.rem_euclid(4) as usize][xx.rem_euclid(4) as usize] / 16.0;
                let band = level.floor() as usize + usize::from(level.fract() > threshold);
                let mut c = [p.void, p.s0, p.s1, p.s2][band.min(3)];
                let star = hash(xx, yy, seed);
                let mood = theme().mood;
                // How thick the stars are, and how they look.
                let (rare, faint) = match mood {
                    Mood::Plain => (150, 53),
                    Mood::Aurora => (190, 70),
                    Mood::Dusk => (if t < 0.45 { 170 } else { 100_000 }, 90),
                    Mood::Fireflies => (400, 120),
                };
                if mood == Mood::Aurora {
                    // Curtains: a wavering band, streaked vertically, green
                    // at the foot fading to violet at the top.
                    let centre = 34.0
                        + 9.0 * (xx as f32 * 0.045).sin()
                        + 4.0 * (xx as f32 * 0.11 + 1.0).sin();
                    let d = yy as f32 - centre;
                    if (-16.0..5.0).contains(&d) {
                        let streak = 0.35 + 0.65 * vnoise(xx, 0, 3, seed ^ 0xA0);
                        let fade = if d < 0.0 {
                            1.0 + d / 16.0
                        } else {
                            1.0 - d / 5.0
                        };
                        let hue = mix(p.moss2, p.pink, (-d / 16.0).clamp(0.0, 1.0));
                        c = mix(c, hue, 0.5 * streak * fade);
                    }
                }
                if mood == Mood::Dusk && t > 0.6 {
                    c = mix(c, p.rim, (t - 0.6) * 0.45);
                }
                if star.is_multiple_of(rare) {
                    c = mix(p.rim_hi, p.white, (star % 7) as f32 / 7.0);
                } else if star.is_multiple_of(faint) {
                    c = mix(c, p.rim, 0.45);
                }
                self.px(xx, yy, c);
            }
        }
    }

    /// A schematic: flat navy with a faint grid, brighter every fourth line.
    fn grid(&mut self, x: i32, y: i32, w: i32, h: i32) {
        let p = pal();
        for yy in y..y + h {
            for xx in x..x + w {
                let major = xx.rem_euclid(32) == 0 || yy.rem_euclid(32) == 0;
                let minor = xx.rem_euclid(8) == 0 || yy.rem_euclid(8) == 0;
                let c = if major {
                    p.s2
                } else if minor && (xx + yy).rem_euclid(2) == 0 {
                    p.s1
                } else {
                    p.s0
                };
                self.px(xx, yy, c);
            }
        }
    }

    /// Ink-blue stone blocks with mortar, specks, and rim light along the
    /// top of some — where they catch the glow.
    fn bricks(&mut self, x: i32, y: i32, w: i32, h: i32, seed: u32) {
        const BW: i32 = 14;
        const BH: i32 = 7;
        for yy in y..y + h {
            for xx in x..x + w {
                let row = yy.div_euclid(BH);
                let shift = if row % 2 == 0 { 0 } else { BW / 2 };
                let col = (xx + shift).div_euclid(BW);
                let (bx, by) = ((xx + shift).rem_euclid(BW), yy.rem_euclid(BH));
                let shade = hash(col, row, seed) % 3;
                let base = [pal().s1, mix(pal().s1, pal().s2, 0.5), pal().s2][shade as usize];
                let lit = hash(col, row, seed ^ 0xA1) % 5 < 2;
                let c = if bx == 0 || by == 0 {
                    pal().s0
                } else if by == 1 && lit {
                    if hash(xx, yy, seed).is_multiple_of(7) {
                        pal().rim_hi
                    } else {
                        pal().rim
                    }
                } else if by == 1 || bx == 1 {
                    mix(base, pal().s3, 0.55)
                } else if by == BH - 1 || bx == BW - 1 {
                    mix(base, pal().s0, 0.4)
                } else {
                    match hash(xx, yy, seed ^ 0x55) % 41 {
                        0 => mix(base, pal().s3, 0.7),
                        1 => mix(base, pal().s0, 0.7),
                        _ => base,
                    }
                };
                self.px(xx, yy, c);
            }
        }
    }

    /// A recess for a display or a control: cut into stone, or outlined in
    /// light, as the style has it.
    fn recess(&mut self, x: i32, y: i32, w: i32, h: i32, inside: C) {
        let p = pal();
        self.fill(x, y, w, h, inside);
        match theme().style {
            Style::Night => {
                let edge = mix(p.rim, inside, 0.5);
                for xx in x..x + w {
                    self.px(xx, y - 1, edge);
                    self.px(xx, y + h, edge);
                }
                for yy in y..y + h {
                    self.px(x - 1, yy, edge);
                    self.px(x + w, yy, edge);
                }
                return;
            }
            Style::Wireframe => {
                self.frame(x - 1, y - 1, w + 2, h + 2, p.cyan2, p.cyan2);
                self.brackets(x - 1, y - 1, w + 2, h + 2, p.cyan1);
                return;
            }
            Style::Chroma => {
                self.frame(x - 1, y - 1, w + 2, h + 2, p.rim, p.rim);
                return;
            }
            _ => {}
        }
        self.frame(x - 1, y - 1, w + 2, h + 2, pal().s0, pal().s4);
        for xx in x..x + w {
            self.px(xx, y + h, mix(pal().rim, pal().s2, 0.45));
        }
    }

    /// L-shaped marks on a rectangle's corners.
    fn brackets(&mut self, x: i32, y: i32, w: i32, h: i32, c: C) {
        let arm = 3.min(w / 2).min(h / 2);
        for k in 0..arm {
            for (cx, cy, dx, dy) in [
                (x, y, 1, 1),
                (x + w - 1, y, -1, 1),
                (x, y + h - 1, 1, -1),
                (x + w - 1, y + h - 1, -1, -1),
            ] {
                self.px(cx + k * dx, cy, c);
                self.px(cx, cy + k * dy, c);
            }
        }
    }

    /// A raised control, as the style builds it.
    fn raised(&mut self, x: i32, y: i32, w: i32, h: i32, pressed: bool) {
        let p = pal();
        match theme().style {
            Style::Night => {
                // Glass: dark, edged in full rim light with a lit top lip,
                // corners rounded — bright enough to make out in the dark.
                let face = if pressed { p.void } else { p.s1 };
                self.fill(x, y, w, h, face);
                self.frame(x, y, w, h, p.rim, p.rim);
                for (cx, cy) in [
                    (x, y),
                    (x + w - 1, y),
                    (x, y + h - 1),
                    (x + w - 1, y + h - 1),
                ] {
                    self.px(cx, cy, p.void);
                }
                self.fill(x + 1, y + 1, w - 2, 1, mix(face, p.rim_hi, 0.4));
                return;
            }
            Style::Wireframe => {
                let (face, edge) = if pressed {
                    (p.cyan3, p.cyan)
                } else {
                    (p.s0, p.cyan2)
                };
                self.fill(x, y, w, h, face);
                self.frame(x, y, w, h, edge, edge);
                self.brackets(x, y, w, h, if pressed { p.white } else { p.cyan1 });
                return;
            }
            Style::Temple => {
                // Carved: a bevel, then an engraved line just inside it.
                let (face, tl, br) = if pressed {
                    (p.s1, p.s0, p.s3)
                } else {
                    (p.s2, p.s4, p.s0)
                };
                self.fill(x, y, w, h, face);
                self.frame(x, y, w, h, tl, br);
                if w > 6 && h > 6 {
                    self.frame(x + 2, y + 2, w - 4, h - 4, p.s1, mix(face, p.s4, 0.3));
                }
                return;
            }
            Style::Chroma => {
                // Red line-work on black, standing forward; pressed, the
                // face drops back to blue behind its red edge and glyph.
                self.fill(x, y, w, h, if pressed { p.s2 } else { p.void });
                self.frame(x, y, w, h, p.rim, p.rim);
                return;
            }
            Style::Bricks => {}
        }
        let (face, tl, br) = if pressed {
            (pal().s1, pal().s0, pal().s3)
        } else {
            (pal().s2, pal().s4, pal().s0)
        };
        self.fill(x, y, w, h, face);
        self.frame(x, y, w, h, tl, br);
        // Weathering: a few specks on the face.
        for yy in y + 2..y + h - 2 {
            for xx in x + 2..x + w - 2 {
                if hash(xx, yy, 7).is_multiple_of(29) {
                    self.px(xx, yy, mix(face, pal().s3, 0.6));
                }
            }
        }
    }

    /// Pixel art from rows of characters: `#` takes `c`, `+` a dimmer `c`,
    /// anything else is left alone.
    fn art(&mut self, x: i32, y: i32, rows: &[&str], c: C) {
        let dim = mix(c, pal().s0, 0.55);
        for (dy, row) in rows.iter().enumerate() {
            for (dx, ch) in row.chars().enumerate() {
                let at = (x + dx as i32, y + dy as i32);
                match ch {
                    '#' => self.px(at.0, at.1, c),
                    '+' => self.px(at.0, at.1, dim),
                    _ => {}
                }
            }
        }
    }

    fn text(&mut self, x: i32, y: i32, s: &str, c: C) {
        for (i, ch) in s.chars().enumerate() {
            self.art(x + i as i32 * 5, y, &glyph(ch), c);
        }
    }

    fn text_centred(&mut self, cx: i32, y: i32, s: &str, c: C) {
        self.text(cx - text_width(s) / 2, y, s, c);
    }

    /// A soft halo of `c` around a point, fading over `r` pixels.
    fn glow(&mut self, x: i32, y: i32, c: C, r: i32, strength: f32) {
        if !theme().glow {
            return;
        }
        for dy in -r..=r {
            for dx in -r..=r {
                let d = ((dx * dx + dy * dy) as f32).sqrt();
                if d <= r as f32 {
                    let t = strength * (1.0 - d / (r as f32 + 0.5)).powi(2);
                    self.tint(x + dx, y + dy, c, t);
                }
            }
        }
    }

    /// A firefly: a bright pixel in a faint halo.
    fn spore(&mut self, x: i32, y: i32, c: C) {
        if !theme().sparkles {
            return;
        }
        self.glow(x, y, c, 2, 0.35);
        self.px(x, y, mix(c, pal().white, 0.3));
    }

    /// Moss creeping along a horizontal edge, `up` for growth upwards.
    fn moss(&mut self, x: i32, y: i32, w: i32, up: bool, seed: u32) {
        if !theme().flora {
            return;
        }
        for xx in x..x + w {
            let height = (hash(xx / 2, 0, seed) % 4) as i32;
            for k in 0..=height {
                let yy = if up { y - k } else { y + k };
                let c = match k {
                    0 => pal().moss0,
                    1 => pal().moss1,
                    _ => {
                        if hash(xx, yy, seed).is_multiple_of(3) {
                            pal().moss2
                        } else {
                            pal().moss1
                        }
                    }
                };
                self.px(xx, yy, c);
            }
        }
    }

    /// Pixel art with a palette: each character maps to a colour; others
    /// are left alone.
    fn art_map(&mut self, x: i32, y: i32, rows: &[&str], colours: &[(char, C)]) {
        for (dy, row) in rows.iter().enumerate() {
            for (dx, ch) in row.chars().enumerate() {
                if let Some(&(_, c)) = colours.iter().find(|(k, _)| *k == ch) {
                    self.px(x + dx as i32, y + dy as i32, c);
                }
            }
        }
    }

    /// A glowing fern growing up from `(x, y)`: a stem and paired fronds,
    /// bright at the tips.
    fn fern(&mut self, x: i32, y: i32, height: i32, seed: u32) {
        if !theme().flora {
            return;
        }
        self.glow(x, y - height / 2, pal().moss2, height / 2 + 2, 0.12);
        for k in 0..height {
            self.px(x, y - k, if k < 2 { pal().moss1 } else { pal().moss2 });
            if k >= 2 && k % 2 == 0 {
                let reach = 1 + ((height - k) / 3).min(2) + (hash(k, 0, seed) % 2) as i32;
                for d in 1..=reach {
                    let tip = d == reach;
                    let c = if tip { pal().cyan } else { pal().moss2 };
                    self.px(x - d, y - k - d / 2, c);
                    self.px(x + d, y - k - d / 2, c);
                }
            }
        }
        self.px(x, y - height, pal().cyan);
    }

    /// A vine hanging from `(x, y)`, ending in a pink bud.
    fn vine(&mut self, x: i32, y: i32, len: i32, seed: u32) {
        if !theme().flora {
            return;
        }
        let mut xx = x;
        for k in 0..len {
            if hash(k, 0, seed).is_multiple_of(4) {
                xx += if hash(k, 1, seed).is_multiple_of(2) {
                    1
                } else {
                    -1
                };
            }
            self.px(
                xx,
                y + k,
                if k % 3 == 2 { pal().moss2 } else { pal().moss1 },
            );
        }
        self.glow(xx, y + len, pal().pink, 2, 0.4);
        self.px(xx, y + len, pal().pink);
    }

    /// A twinkle: a small yellow cross.
    fn sparkle(&mut self, x: i32, y: i32) {
        if !theme().sparkles {
            return;
        }
        self.glow(x, y, pal().spark, 3, 0.18);
        for (dx, dy) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
            self.px(x + dx, y + dy, mix(pal().spark, pal().amber1, 0.3));
        }
        self.px(x, y, mix(pal().spark, pal().white, 0.6));
    }

    /// A thin chain from `y0` down to `y1`.
    fn chain(&mut self, x: i32, y0: i32, y1: i32) {
        for y in y0..y1 {
            self.px(x, y, if y % 2 == 0 { pal().s4 } else { pal().s3 });
        }
    }

    fn put(&mut self, key: SpriteKey, f: impl FnOnce(&mut Self, i32, i32, i32, i32)) {
        let (_, r) = sprite::resolve(key);
        f(self, r.x as i32, r.y as i32, r.w as i32, r.h as i32);
    }
}

// -- Shared pieces ----------------------------------------------------------------

/// The lantern: Lantern's emblem. 7x9.
const LANTERN: [&str; 9] = [
    "..###..", ".#...#.", "#######", "#+###+#", "#+#+#+#", "#+###+#", "#######", ".#...#.",
    "..###..",
];

fn lantern(c: &mut Canvas, x: i32, y: i32, lit: bool) {
    if lit {
        c.glow(x + 3, y + 4, pal().amber, 6, 0.35);
    }
    c.art(x, y, &LANTERN, if lit { pal().amber1 } else { pal().s3 });
    let flame = if lit { pal().amber } else { pal().s4 };
    c.px(x + 3, y + 4, flame);
    c.px(
        x + 3,
        y + 3,
        if lit {
            mix(pal().amber, pal().white, 0.5)
        } else {
            pal().s4
        },
    );
}

/// A translucent koi, 34x12, drawn by tinting what is beneath it.
fn ghost_koi(c: &mut Canvas, x: i32, y: i32) {
    if !theme().creatures {
        return;
    }
    const KOI: [&str; 12] = [
        "...........oo.....................",
        ".........oo##o....................",
        "......ooo####oooooo........oo.....",
        "....oo##############oo....o##o....",
        "..oo####w+############oo.o###o....",
        ".o######++##############o####o....",
        ".o######################o####o....",
        "..oo####################oo###o....",
        "....oo###############oo...o##o....",
        "......oooo####oooooo.......oo.....",
        "..........o##o....................",
        "...........oo.....................",
    ];
    c.glow(x + 14, y + 6, pal().rim, 14, 0.12);
    for (dy, row) in KOI.iter().enumerate() {
        for (dx, ch) in row.chars().enumerate() {
            let (px, py) = (x + dx as i32, y + dy as i32);
            match ch {
                'o' => c.tint(px, py, pal().rim_hi, 0.75),
                '#' => {
                    // Faint scales: every other pixel a little brighter.
                    let t = if (dx + dy) % 2 == 0 { 0.38 } else { 0.28 };
                    c.tint(px, py, pal().rim, t);
                }
                'w' => c.px(px, py, pal().white),
                '+' => c.tint(px, py, pal().pink, 0.5),
                _ => {}
            }
        }
    }
}

/// A small 9x9 title-bar button with `icon` (rows of 5x5) in it.
fn small_button(c: &mut Canvas, x: i32, y: i32, icon: &[&str], pressed: bool) {
    c.raised(x, y, 9, 9, pressed);
    c.art(
        x + 2,
        y + 2,
        icon,
        if pressed { pal().cyan } else { pal().cyan1 },
    );
}

const ICON_OPTIONS: [&str; 5] = ["..#..", ".###.", "#####", ".###.", "..#.."];
const ICON_MINIMIZE: [&str; 5] = [".....", ".....", ".....", ".....", "#####"];
const ICON_SHADE: [&str; 5] = ["#####", ".....", "#####", ".....", "....."];
const ICON_UNSHADE: [&str; 5] = ["#####", "#...#", "#...#", "#...#", "#####"];
const ICON_CLOSE: [&str; 5] = ["#...#", ".#.#.", "..#..", ".#.#.", "#...#"];

/// A title strip at `(x, y)`, `w` wide: stone with glowing grooves either
/// side of `label`. `below` is how far under the main window's top the
/// strip's window sits, for Chroma's burst.
fn title_strip(
    c: &mut Canvas,
    (x, y, w): (i32, i32, i32),
    label: &str,
    active: bool,
    seed: u32,
    below: i32,
) {
    c.backdrop(x, y, w, 14, seed);
    c.burst(x, y, w, 14, below);
    if theme().style == Style::Chroma {
        // The frame's outer rail runs on down the sides, unbroken from
        // the main window into the EQ; only the topmost strip gets a top.
        let p = pal();
        for (inset, col) in [(0, p.void), (1, p.rim), (2, p.void)] {
            c.fill(x + inset, y, 1, 14, col);
            c.fill(x + w - 1 - inset, y, 1, 14, col);
            if below == 0 {
                c.fill(x + inset, y + inset, w - 2 * inset, 1, col);
            }
        }
    } else {
        c.frame(x, y, w, 14, pal().s3, pal().s0);
    }
    let line = if active { pal().cyan1 } else { pal().s3 };
    let lw = text_width(label);
    let cx = x + w / 2;
    let (left_end, right_start) = (cx - lw / 2 - 6, cx + lw / 2 + 6);
    for xx in x + 20..left_end {
        c.px(xx, y + 6, line);
        c.px(xx, y + 8, mix(line, pal().s0, 0.5));
    }
    for xx in right_start..x + w - 34 {
        c.px(xx, y + 6, line);
        c.px(xx, y + 8, mix(line, pal().s0, 0.5));
    }
    c.fill(cx - lw / 2 - 3, y + 3, lw + 6, 8, pal().void);
    if active {
        c.text_centred(cx, y + 4, label, pal().cyan);
        c.glow(cx, y + 6, pal().cyan, 4, 0.0);
    } else {
        c.text_centred(cx, y + 4, label, pal().s4);
    }
}

/// A two-line label button for the playlist's pop-up menus.
fn menu_button(c: &mut Canvas, x: i32, y: i32, top: &str, bottom: &str, pressed: bool) {
    c.raised(x, y, 22, 18, pressed);
    let (dim, lit) = if pressed {
        (pal().cyan1, pal().cyan)
    } else {
        (pal().s4, pal().cyan1)
    };
    if bottom.is_empty() {
        c.text_centred(x + 11, y + 6, top, lit);
    } else {
        c.text_centred(x + 11, y + 3, top, dim);
        c.text_centred(x + 11, y + 10, bottom, lit);
    }
}

// -- Sheets -----------------------------------------------------------------------

fn main_bmp() -> Canvas {
    let mut c = Canvas::new(275, 116);
    c.backdrop(0, 0, 275, 116, 1);
    c.burst(0, 0, 275, 116, 0);
    c.frame(0, 0, 275, 116, pal().s3, pal().s0);

    // The display: clutter bar column and the screen, cut into the wall.
    c.recess(9, 21, 95, 42, pal().void);
    c.frame(19, 22, 1, 40, pal().s1, pal().s1);
    // A faint cave-glow at the screen's foot.
    for xx in 20..103 {
        for k in 0..3 {
            c.tint(xx, 61 - k, pal().cyan3, 0.5 - k as f32 * 0.15);
        }
    }

    // Track info: marquee, rates, channel lamps.
    c.recess(109, 24, 158, 12, pal().void);
    c.recess(109, 41, 19, 10, pal().void);
    c.text(130, 43, "KBPS", pal().cyan1);
    c.recess(154, 41, 14, 10, pal().void);
    c.text(170, 43, "KHZ", pal().cyan1);
    c.recess(211, 40, 57, 14, pal().void);

    // Grooves the sliders and position bar run in.
    c.recess(106, 56, 110, 15, pal().s0);
    c.recess(218, 57, 48, 14, pal().s0);
    c.recess(15, 71, 250, 12, pal().s0);
    // Beds for the transport row.
    c.recess(15, 87, 116, 20, pal().s0);
    c.recess(135, 88, 24, 18, pal().s0);
    c.recess(163, 88, 76, 17, pal().s0);

    style_decor(&mut c, 275, 116, true);

    // Moss along the floor and under the ledges; fireflies about.
    c.moss(1, 114, 273, true, 2);
    c.moss(242, 107, 30, true, 3);
    for (x, y, col) in [
        (6, 70, pal().cyan),
        (247, 84, pal().pink),
        (163, 18, pal().cyan),
        (190, 108, pal().amber),
        (4, 104, pal().pink),
        (229, 18, pal().cyan),
    ] {
        c.spore(x, y, col);
    }

    // Ferns along the floor, vines from the ledges, sparkles in the air.
    for (x, h, seed) in [
        (5, 9, 1),
        (100, 6, 2),
        (150, 7, 3),
        (233, 8, 4),
        (270, 6, 5),
    ] {
        c.fern(x, 113, h, seed);
    }
    c.vine(104, 14, 9, 6);
    c.vine(270, 14, 16, 7);
    c.vine(2, 14, 12, 8);
    for (x, y) in [(195, 18), (60, 67), (246, 77), (122, 110)] {
        c.sparkle(x, y);
    }

    if theme().creatures {
        creatures(&mut c);
    }

    // The about hotspot: a lantern hanging in the corner.
    c.chain(259, 84, 92);
    lantern(&mut c, 256, 92, true);
    vignette(&mut c, 275, 116);
    c
}

/// Something peeks over the top of the display; something lives in the
/// wall.
fn creatures(c: &mut Canvas) {
    let p = pal();
    let critter: [&str; 6] = match theme().mood {
        // The cat-eared critter.
        Mood::Plain => [
            "..o.....o..",
            ".obo...obo.",
            ".obbooobbo.",
            "obwwbbbwwbo",
            "obwpbbbwpbo",
            "obbbbbbbbbo",
        ],
        // An owl: ear tufts, round eyes, a beak.
        Mood::Aurora => [
            "o.........o",
            "oo.ooooo.oo",
            ".obbbbbbbo.",
            "obwpwbwpwbo",
            "obwwwkwwwbo",
            "obbbbkbbbbo",
        ],
        // A fox: tall ears, a long face, a dark nose.
        Mood::Dusk => [
            "o.........o",
            "bo.......ob",
            "bbooooooobb",
            "bbbebbbebbb",
            ".bbbbbbbbb.",
            "..bbbpbbb..",
        ],
        // A field mouse: round ears, dark eyes with a glint, a pink nose.
        Mood::Fireflies => [
            ".oo.....oo.",
            "obbo...obbo",
            "obbbooobbbo",
            "obpwbbbpwbo",
            "obppbbbppbo",
            ".obbbnbbbo.",
        ],
    };
    let (body, outline) = match theme().mood {
        Mood::Dusk => (mix(p.amber1, p.pink1, 0.3), p.rim),
        Mood::Fireflies => (mix(p.s4, p.amber1, 0.55), mix(p.rim_hi, p.amber, 0.3)),
        _ => (p.s3, p.rim),
    };
    c.glow(61, 17, outline, 6, 0.15);
    c.art_map(
        56,
        14,
        &critter,
        &[
            ('o', outline),
            ('b', body),
            ('w', p.white),
            ('p', p.void),
            ('k', p.amber),
            ('e', p.amber),
            ('n', p.pink),
        ],
    );

    // Something lives in the wall — where there is a wall.
    if matches!(theme().style, Style::Night | Style::Wireframe) {
        return;
    }
    c.art(
        241,
        91,
        &[".####.", "######", "######", ".####."],
        pal().void,
    );
    c.px(242, 92, pal().pink);
    c.px(245, 92, pal().pink);
}

/// What each style adds to a 275-wide window background of height `h`.
/// `main` adds the pieces that only fit the main window.
fn style_decor(c: &mut Canvas, w: i32, h: i32, main: bool) {
    let p = pal();
    match theme().style {
        Style::Bricks => {}
        Style::Temple => {
            // Fluted pillars at either edge.
            for x0 in [1, w - 8] {
                c.fill(x0, 14, 7, h - 15, p.s2);
                for x in x0..x0 + 7 {
                    let col = match x - x0 {
                        0 | 6 => p.s0,
                        1 => mix(p.s3, p.rim, 0.5),
                        3 => mix(p.s3, p.rim, 0.25),
                        _ => p.s2,
                    };
                    c.fill(x, 18, 1, h - 26, col);
                }
                // Capital and base.
                c.fill(x0 - 1, 14, 9, 4, p.s3);
                c.fill(x0 - 1, 14, 9, 1, p.rim_hi);
                c.fill(x0 - 1, 17, 9, 1, p.s0);
                c.fill(x0 - 1, h - 8, 9, 4, p.s3);
                c.fill(x0 - 1, h - 8, 9, 1, p.rim);
            }
            // A carved key-pattern frieze along the floor.
            const KEY: [&str; 5] = ["######..", "#....#..", "#.##.#..", "#.#..#..", "#.####.."];
            c.fill(9, h - 7, w - 18, 6, p.s1);
            let mut x = 10;
            while x + 8 < w - 9 {
                c.art(x, h - 7, &KEY, mix(p.rim, p.s2, 0.35));
                x += 8;
            }
        }
        Style::Night if theme().mood != Mood::Plain => night_scenery(c, w, h, main),
        Style::Night => {
            if main {
                // A crescent moon, low in the corner.
                c.glow(244, 89, p.rim_hi, 7, 0.25);
                c.art(
                    240,
                    85,
                    &[".###.", "##...", "#....", "#....", "##...", ".###."],
                    p.rim_hi,
                );
                // A constellation over the display.
                let stars = [(118, 17), (131, 19), (146, 16), (158, 18), (171, 16)];
                for pair in stars.windows(2) {
                    let ((x0, y0), (x1, y1)) = (pair[0], pair[1]);
                    for k in 0..=(x1 - x0) {
                        if k % 2 == 0 {
                            let y = y0 + (y1 - y0) * k / (x1 - x0);
                            c.px(x0 + k, y, mix(p.rim, p.s1, 0.4));
                        }
                    }
                }
                for (x, y) in stars {
                    c.sparkle(x, y);
                }
                // A shooting star.
                for k in 0..14 {
                    c.tint(150 + k, 66 - k / 5, p.rim_hi, 0.15 + k as f32 * 0.05);
                }
            }
        }
        Style::Chroma => chroma_frame(c, w, h, main),
        Style::Wireframe => {
            c.brackets(0, 0, w, h, p.cyan);
            // A ruler along the floor.
            for x in (2..w - 2).step_by(4) {
                let tall = if x % 16 == 2 { 3 } else { 1 };
                c.fill(x, h - 2 - tall, 1, tall, p.cyan2);
            }
        }
    }
}

/// The ground and the sky's furniture for each night variation.
fn night_scenery(c: &mut Canvas, w: i32, h: i32, main: bool) {
    let p = pal();
    match theme().mood {
        Mood::Plain => {}
        Mood::Aurora => {
            // Snow-capped peaks along the floor.
            for x in 1..w - 1 {
                let top =
                    h - 3 - (vnoise(x, 0, 14, 5) * 9.0) as i32 - (vnoise(x, 0, 5, 6) * 3.0) as i32;
                for y in top..h - 1 {
                    let col = if y < top + 2 {
                        mix(p.rim_hi, p.s1, 0.3)
                    } else {
                        p.void
                    };
                    c.px(x, y, col);
                }
            }
        }
        Mood::Dusk => {
            if main {
                // The sun, half-set, then the hills in front of it.
                c.glow(244, h - 8, p.amber, 12, 0.45);
                for dy in -7i32..=0 {
                    for dx in -7i32..=7 {
                        if dx * dx + dy * dy <= 49 {
                            let band = (dy + 7) % 3 == 0;
                            c.px(244 + dx, h - 8 + dy, if band { p.pink } else { p.amber });
                        }
                    }
                }
            }
            for x in 1..w - 1 {
                let top = h - 4 - (vnoise(x, 0, 30, 9) * 6.0) as i32;
                for y in top..h - 1 {
                    c.px(x, y, if y == top { p.pink1 } else { p.void });
                }
            }
        }
        Mood::Fireflies => {
            // Tall grass along the floor, lit at the tips.
            for x in (1..w - 1).step_by(2) {
                let height = 3 + (hash(x, 0, 61) % 9) as i32;
                let lean = if hash(x, 1, 61).is_multiple_of(2) {
                    1
                } else {
                    -1
                };
                for k in 0..height {
                    let xx = x + if k > height / 2 { lean } else { 0 };
                    let col = if k == height - 1 { p.moss2 } else { p.moss1 };
                    c.px(xx, h - 2 - k, col);
                }
            }
            // The swarm.
            for k in 0..14 {
                let x = 4 + (hash(k, 2, 62) % (w as u32 - 8)) as i32;
                let y = 15 + (hash(k, 3, 62) % (h as u32 - 22)) as i32;
                c.glow(x, y, p.cyan, 3, 0.35);
                c.px(x, y, mix(p.cyan, p.white, 0.5));
            }
            if main {
                c.glow(244, 89, p.rim_hi, 7, 0.25);
                c.art(
                    240,
                    85,
                    &[".###.", "##...", "#....", "#....", "##...", ".###."],
                    p.rim_hi,
                );
            }
        }
    }
}

/// Chroma's ornate frame: a double red border on black, red pegs off it;
/// on the main window, a teeth row along the floor and a black niche of
/// blue rays for the lantern to hang in front of; on the EQ, dithered
/// corner blocks at the foot.
fn chroma_frame(c: &mut Canvas, w: i32, h: i32, main: bool) {
    let p = pal();
    for (inset, col) in [(0, p.void), (1, p.rim), (2, p.void), (3, p.rim)] {
        c.fill(inset, 14, 1, h - 14 - inset, col);
        c.fill(w - 1 - inset, 14, 1, h - 14 - inset, col);
        c.fill(inset, h - 1 - inset, w - 2 * inset, 1, col);
    }
    for y in (20..h - 14).step_by(8) {
        c.fill(4, y, 3, 1, p.rim);
        // Clear of the lantern's niche.
        if !main || y < 80 {
            c.fill(w - 7, y, 3, 1, p.rim);
        }
    }
    if main {
        for x in (14..w - 14).step_by(8) {
            c.fill(x, h - 7, 1, 3, p.rim);
        }
        let (cx, cy) = (259, 96);
        disc(c, cx, cy, 11, p.void);
        rays(c, cx, cy, (6.0, 10.5), 12, p.s2, (cx - 11, cy - 11, 23, 23));
        circle(c, cx, cy, 8.0, p.s2);
        circle(c, cx, cy, 11.0, p.rim);
    }
    const BLOCK: [&str; 9] = [
        "#########",
        "#.......#",
        "#.#.#.#.#",
        "#..#.#..#",
        "#.#.#.#.#",
        "#..#.#..#",
        "#.#.#.#.#",
        "#.......#",
        "#########",
    ];
    // Where the main window and the EQ stacked under it end.
    if !main {
        for x in [1, w - 10] {
            c.fill(x, h - 10, 9, 9, p.void);
            c.art(x, h - 10, &BLOCK, p.rim);
        }
    }
}

fn disc(c: &mut Canvas, cx: i32, cy: i32, r: i32, col: C) {
    for dy in -r..=r {
        for dx in -r..=r {
            if dx * dx + dy * dy <= r * r {
                c.px(cx + dx, cy + dy, col);
            }
        }
    }
}

fn circle(c: &mut Canvas, cx: i32, cy: i32, r: f32, col: C) {
    let steps = (r * 8.0) as i32;
    for k in 0..steps {
        let a = k as f32 / steps as f32 * std::f32::consts::TAU;
        c.px(
            cx + (r * a.cos()).round() as i32,
            cy + (r * a.sin()).round() as i32,
            col,
        );
    }
}

/// `n` rays out from `(cx, cy)`, between the radii `(from, to)`, kept
/// inside `clip` (x, y, w, h).
fn rays(
    c: &mut Canvas,
    cx: i32,
    cy: i32,
    (from, to): (f32, f32),
    n: usize,
    col: C,
    clip: (i32, i32, i32, i32),
) {
    let (x0, y0, cw, ch) = clip;
    for k in 0..n {
        let a = k as f32 / n as f32 * std::f32::consts::TAU;
        let mut r = from;
        while r <= to {
            let (x, y) = (
                cx + (r * a.cos()).round() as i32,
                cy + (r * a.sin()).round() as i32,
            );
            if (x0..x0 + cw).contains(&x) && (y0..y0 + ch).contains(&y) {
                c.px(x, y, col);
            }
            r += 0.5;
        }
    }
}

/// Chroma's playlist panel: a blue sunburst over a red sun setting into
/// red dithered ground.
fn chroma_panel(c: &mut Canvas, x: i32, y: i32, w: i32, h: i32) {
    let p = pal();
    let (cx, cy) = (x + w / 2, y + h - 4);
    let clip = (x, y, w, h);
    rays(c, cx, cy, (7.0, w as f32), 16, p.s2, clip);
    for r in [10.0, 16.0] {
        circle(c, cx, cy, r, p.s2);
    }
    disc(c, cx, cy, 5, p.rim);
    for k in 0..4 {
        let yy = y + h - 4 + k;
        for xx in x..x + w {
            let on = match k {
                0 => (xx + yy).rem_euclid(4) == 0 && yy % 2 == 0,
                1 => (xx + yy).rem_euclid(2) == 0,
                2 => !hash(xx, yy, 71).is_multiple_of(4),
                _ => true,
            };
            c.px(xx, yy, if on { p.rim } else { p.void });
        }
    }
}

/// The scene in the playlist's open panel, a `w`x`h` dark window at `x, y`.
fn panel_scene(c: &mut Canvas, x: i32, y: i32, w: i32, h: i32) {
    let p = pal();
    match theme().mood {
        Mood::Plain => {
            ghost_koi(c, x + 11, y + 4);
            c.sparkle(x + 57, y + 4);
        }
        Mood::Aurora => {
            for xx in x..x + w {
                let centre = y + 8 + (2.5 * ((xx as f32) * 0.2).sin()) as i32;
                for yy in centre - 5..centre + 2 {
                    let t = 1.0 - ((yy - centre).abs() as f32 / 6.0);
                    c.tint(xx, yy, mix(p.moss2, p.pink, 0.3), 0.5 * t);
                }
                let peak = y + h - 2 - (vnoise(xx, 0, 9, 3) * 7.0) as i32;
                for yy in peak..y + h {
                    c.px(xx, yy, if yy == peak { p.rim_hi } else { p.s0 });
                }
            }
        }
        Mood::Dusk => {
            for yy in y..y + h {
                let t = (yy - y) as f32 / h as f32;
                for xx in x..x + w {
                    c.tint(xx, yy, p.pink1, t * 0.5);
                }
            }
            c.glow(x + w / 2, y + h - 5, p.amber, 9, 0.5);
            for dy in -5i32..=0 {
                for dx in -5i32..=5 {
                    if dx * dx + dy * dy <= 25 {
                        c.px(x + w / 2 + dx, y + h - 5 + dy, p.amber);
                    }
                }
            }
            for xx in x..x + w {
                let top = y + h - 4 - (vnoise(xx, 0, 12, 4) * 4.0) as i32;
                for yy in top..y + h {
                    c.px(xx, yy, p.void);
                }
            }
        }
        Mood::Fireflies => {
            // A jar, lit by what is caught in it.
            c.glow(x + w / 2, y + h / 2, p.cyan, 7, 0.3);
            lantern(c, x + w / 2 - 3, y + h / 2 - 4, false);
            c.px(x + w / 2, y + h / 2, p.cyan);
            c.px(x + w / 2 - 1, y + h / 2 - 1, mix(p.cyan, p.white, 0.4));
            for k in 0..9 {
                let fx = x + w / 2 - 12 + (hash(k, 4, 91) % 24) as i32;
                let fy = y + 2 + (hash(k, 5, 91) % (h as u32 - 4)) as i32;
                c.glow(fx, fy, p.cyan, 2, 0.5);
                c.px(fx, fy, mix(p.cyan, p.white, 0.4));
            }
        }
    }
}

/// Smooth value noise in 0..1, on a lattice `cell` pixels apart.
fn vnoise(x: i32, y: i32, cell: i32, seed: u32) -> f32 {
    let (gx, gy) = (x.div_euclid(cell), y.div_euclid(cell));
    let (fx, fy) = (
        x.rem_euclid(cell) as f32 / cell as f32,
        y.rem_euclid(cell) as f32 / cell as f32,
    );
    let at = |i: i32, j: i32| (hash(gx + i, gy + j, seed) % 1000) as f32 / 999.0;
    let smooth = |t: f32| t * t * (3.0 - 2.0 * t);
    let (sx, sy) = (smooth(fx), smooth(fy));
    let top = at(0, 0) + (at(1, 0) - at(0, 0)) * sx;
    let bottom = at(0, 1) + (at(1, 1) - at(0, 1)) * sx;
    top + (bottom - top) * sy
}

/// Darken towards the edges, for depth.
fn vignette(c: &mut Canvas, w: i32, h: i32) {
    if matches!(
        theme().style,
        Style::Wireframe | Style::Temple | Style::Chroma
    ) {
        return;
    }
    for y in 0..h {
        for x in 0..w {
            let dx = (x as f32 / (w - 1) as f32 - 0.5).abs() * 2.0;
            let dy = (y as f32 / (h - 1) as f32 - 0.5).abs() * 2.0;
            let d = dx.max(dy);
            if d > 0.7 {
                c.tint(x, y, pal().void, (d - 0.7) / 0.3 * 0.35);
            }
        }
    }
}

fn titlebar_bmp() -> Canvas {
    let mut c = Canvas::new(344, 87);
    c.backdrop(0, 0, 344, 87, 11);
    for active in [true, false] {
        for key in [
            SpriteKey::TitleBar { active },
            SpriteKey::EasterEggTitleBar { active },
        ] {
            c.put(key, |c, x, y, w, _| {
                title_strip(c, (x, y, w), "LANTERN", active, 12, 0)
            });
        }
        // Windowshade: the strip carries the mini display and transport.
        c.put(SpriteKey::ShadeBackground { active }, |c, x, y, w, h| {
            c.backdrop(x, y, w, h, 13);
            c.frame(x, y, w, h, pal().s3, pal().s0);
            lantern(&mut *c, x + 20, y + 2, active);
            c.recess(x + 79, y + 4, 39, 7, pal().void);
            c.recess(x + 126, y + 3, 27, 8, pal().void);
            // Mini transport: previous, play, pause, stop, next, eject.
            let tint = if active { pal().cyan1 } else { pal().s4 };
            c.art(
                x + 169,
                y + 3,
                &["#..#", "#.##", "####", "#.##", "#..#"],
                tint,
            );
            c.art(
                x + 178,
                y + 3,
                &["#...", "##..", "###.", "##..", "#..."],
                tint,
            );
            c.art(
                x + 187,
                y + 3,
                &["##.#", "##.#", "##.#", "##.#", "##.#"],
                tint,
            );
            c.art(
                x + 196,
                y + 3,
                &["####", "####", "####", "####", "####"],
                tint,
            );
            c.art(
                x + 205,
                y + 3,
                &["#..#", "##.#", "####", "##.#", "#..#"],
                tint,
            );
            c.art(
                x + 216,
                y + 3,
                &["..#..", ".###.", "#####", ".....", "#####"],
                tint,
            );
            c.recess(x + 226, y + 4, 17, 7, pal().s0);
        });
    }
    // The 27px column of small buttons at the left.
    for pressed in [false, true] {
        c.put(SpriteKey::OptionsButton { pressed }, |c, x, y, _, _| {
            small_button(c, x, y, &ICON_OPTIONS, pressed)
        });
        c.put(SpriteKey::MinimizeButton { pressed }, |c, x, y, _, _| {
            small_button(c, x, y, &ICON_MINIMIZE, pressed)
        });
        c.put(SpriteKey::ShadeButton { pressed }, |c, x, y, _, _| {
            small_button(c, x, y, &ICON_SHADE, pressed)
        });
        c.put(SpriteKey::CloseButton { pressed }, |c, x, y, _, _| {
            small_button(c, x, y, &ICON_CLOSE, pressed)
        });
        c.put(SpriteKey::UnshadeButton { pressed }, |c, x, y, _, _| {
            small_button(c, x, y, &ICON_UNSHADE, pressed)
        });
    }
    c.put(SpriteKey::ShadePositionBackground, |c, x, y, w, h| {
        c.fill(x, y, w, h, pal().s0);
        for xx in x..x + w {
            c.px(xx, y + 3, pal().cyan3);
        }
    });
    for t in [ShadeThumb::Left, ShadeThumb::Center, ShadeThumb::Right] {
        c.put(SpriteKey::ShadePositionThumb(t), |c, x, y, w, h| {
            c.fill(x, y, w, h, pal().s0);
            c.fill(x, y + 1, w, h - 2, pal().cyan1);
            c.fill(x + 1, y + 2, 1, h - 4, pal().cyan);
        });
    }

    // The clutter bar: O A I D V, lit when pressed.
    let letters = [
        (ClutterButton::O, 'O', 3),
        (ClutterButton::A, 'A', 11),
        (ClutterButton::I, 'I', 18),
        (ClutterButton::D, 'D', 25),
        (ClutterButton::V, 'V', 33),
    ];
    for enabled in [true, false] {
        c.put(SpriteKey::ClutterBar { enabled }, |c, x, y, w, h| {
            c.fill(x, y, w, h, pal().void);
            c.frame(x, y, w, h, pal().s1, pal().s1);
            for (_, ch, dy) in letters {
                let col = if enabled { pal().cyan2 } else { pal().s2 };
                c.art(x + 2, y + dy + 1, &glyph(ch), col);
            }
        });
    }
    for (b, ch, _) in letters {
        c.put(SpriteKey::ClutterBarButton(b), |c, x, y, w, h| {
            c.fill(x, y, w, h, pal().cyan3);
            c.art(x + 2, y + 1, &glyph(ch), pal().cyan);
        });
    }
    c
}

fn cbuttons_bmp() -> Canvas {
    let mut c = Canvas::new(136, 36);
    type Icon = (fn(bool) -> SpriteKey, [&'static str; 7]);
    let icons: [Icon; 6] = [
        (
            |p| SpriteKey::Previous { pressed: p },
            [
                "#...#..", "#..##..", "#.###..", "#####..", "#.###..", "#..##..", "#...#..",
            ],
        ),
        (
            |p| SpriteKey::Play { pressed: p },
            [
                "#......", "###....", "#####..", "#######", "#####..", "###....", "#......",
            ],
        ),
        (
            |p| SpriteKey::Pause { pressed: p },
            [
                "##..##.", "##..##.", "##..##.", "##..##.", "##..##.", "##..##.", "##..##.",
            ],
        ),
        (
            |p| SpriteKey::Stop { pressed: p },
            [
                "######.", "######.", "######.", "######.", "######.", "######.", ".......",
            ],
        ),
        (
            |p| SpriteKey::Next { pressed: p },
            [
                "#...#..", "##..#..", "###.#..", "#####..", "###.#..", "##..#..", "#...#..",
            ],
        ),
        (
            |p| SpriteKey::Eject { pressed: p },
            [
                "...#...", "..###..", ".#####.", "#######", ".......", "#######", ".......",
            ],
        ),
    ];
    for (key, icon) in icons {
        for pressed in [false, true] {
            c.put(key(pressed), |c, x, y, w, h| {
                c.raised(x, y, w, h, pressed);
                let (ix, iy) = (x + (w - 7) / 2, y + (h - 7) / 2);
                if pressed {
                    c.glow(ix + 3, iy + 3, pal().cyan, 6, 0.3);
                }
                c.art(
                    ix,
                    iy,
                    &icon,
                    if pressed { pal().cyan } else { pal().cyan1 },
                );
            });
        }
    }
    c
}

/// Seven-segment digits, with the unlit segments faintly visible, as on an
/// old LCD. `nums_ex.bmp` layout: 0-9, then a blank and a minus.
fn numbers_bmp() -> Canvas {
    let mut c = Canvas::new(108, 13);
    // Segments a-g as (x, y, w, h) in a 9x13 cell.
    const SEG: [(i32, i32, i32, i32); 7] = [
        (2, 0, 5, 2),  // a: top
        (6, 1, 2, 5),  // b: upper right
        (6, 7, 2, 5),  // c: lower right
        (2, 11, 5, 2), // d: bottom
        (1, 7, 2, 5),  // e: lower left
        (1, 1, 2, 5),  // f: upper left
        (2, 6, 5, 1),  // g: middle
    ];
    const ON: [u8; 10] = [
        0b011_1111, 0b000_0110, 0b101_1011, 0b100_1111, 0b110_0110, 0b110_1101, 0b111_1101,
        0b000_0111, 0b111_1111, 0b110_1111,
    ];
    let digit = |c: &mut Canvas, x: i32, mask: u8| {
        for (i, &(sx, sy, w, h)) in SEG.iter().enumerate() {
            let lit = mask & (1 << i) != 0;
            c.fill(x + sx, sy, w, h, if lit { pal().cyan } else { pal().cyan3 });
        }
    };
    for (n, &mask) in ON.iter().enumerate() {
        digit(&mut c, n as i32 * 9, mask);
    }
    digit(&mut c, 90, 0);
    digit(&mut c, 99, 0b100_0000);
    c
}

fn playpaus_bmp() -> Canvas {
    let mut c = Canvas::new(42, 9);
    c.art(
        1,
        1,
        &["#....", "###..", "#####", "###..", "#...."],
        pal().cyan,
    );
    c.art(
        10,
        1,
        &["##.##", "##.##", "##.##", "##.##", "##.##"],
        pal().amber,
    );
    c.art(
        19,
        1,
        &["#####", "#####", "#####", "#####", "#####"],
        pal().pink1,
    );
    // The lamps beside the state: green-ish cyan for fine, pink for busy.
    c.fill(36, 1, 2, 3, pal().cyan);
    c.fill(36, 5, 2, 3, pal().pink1);
    c.fill(39, 1, 2, 3, pal().cyan3);
    c.fill(39, 5, 2, 3, pal().pink);
    c
}

fn monoster_bmp() -> Canvas {
    let mut c = Canvas::new(56, 24);
    for active in [true, false] {
        let col = if active { pal().cyan } else { pal().cyan2 };
        c.put(SpriteKey::Stereo { active }, |c, x, y, w, h| {
            c.fill(x, y, w, h, pal().void);
            c.text(x, y + 3, "STEREO", col);
        });
        c.put(SpriteKey::Mono { active }, |c, x, y, w, h| {
            c.fill(x, y, w, h, pal().void);
            c.text_centred(x + w / 2, y + 3, "MONO", col);
        });
    }
    c
}

fn posbar_bmp() -> Canvas {
    let mut c = Canvas::new(307, 10);
    c.put(SpriteKey::PosBarBackground, |c, x, y, w, h| {
        c.fill(x, y, w, h, pal().s0);
        // A root running along the groove.
        for xx in x..x + w {
            let wobble = (hash(xx / 6, 0, 21) % 3) as i32;
            c.px(xx, y + 4 + wobble / 2, pal().cyan3);
        }
    });
    for pressed in [false, true] {
        c.put(SpriteKey::PosBarThumb { pressed }, |c, x, y, w, h| {
            c.fill(x, y, w, h, pal().s0);
            let core = if pressed { pal().pink } else { pal().cyan };
            c.fill(x + 2, y + 2, w - 4, h - 4, mix(core, pal().s0, 0.5));
            c.fill(x + 1, y + 3, w - 2, h - 6, mix(core, pal().s0, 0.5));
            c.fill(x + 4, y + 4, w - 8, 2, core);
            c.fill(x + w / 2 - 2, y + 4, 4, 2, pal().white);
        });
    }
    c
}

/// A horizontal slider sheet: 28 track frames, then thumbs.
fn slider_bmp(width: u32, track_x: i32, track_w: i32, balance: bool) -> Canvas {
    let mut c = Canvas::new(width, 433);
    for p in 0..28u8 {
        let key = if balance {
            SpriteKey::BalanceBackground(p)
        } else {
            SpriteKey::VolumeBackground(p)
        };
        c.put(key, |c, x, y, w, h| {
            c.fill(x, y, w, h, pal().s0);
            c.frame(x, y + 4, w, 5, pal().s0, pal().s2);
            let t = f32::from(p) / 27.0;
            if balance {
                // Balance glows amber as it leans off centre.
                let col = mix(pal().cyan2, pal().amber1, t);
                c.fill(x + 1, y + 5, w - 2, 3, col);
                c.fill(x + w / 2, y + 5, 1, 3, pal().cyan1);
            } else {
                // Volume fills with light from the left.
                let fill = ((w - 2) as f32 * t).round() as i32;
                for xx in 0..fill {
                    let u = xx as f32 / (w - 2) as f32;
                    c.fill(x + 1 + xx, y + 5, 1, 3, mix(pal().cyan2, pal().cyan, u));
                }
            }
        });
    }
    let _ = (track_x, track_w);
    for pressed in [false, true] {
        let key = if balance {
            SpriteKey::BalanceThumb { pressed }
        } else {
            SpriteKey::VolumeThumb { pressed }
        };
        c.put(key, |c, x, y, w, h| {
            c.raised(x, y, w, h, pressed);
            let rune = if pressed { pal().pink } else { pal().cyan1 };
            c.fill(x + w / 2 - 1, y + 3, 2, h - 6, rune);
        });
    }
    c
}

fn shufrep_bmp() -> Canvas {
    let mut c = Canvas::new(92, 85);
    for active in [false, true] {
        for pressed in [false, true] {
            let lamp = if active { pal().cyan } else { pal().s3 };
            c.put(SpriteKey::Shuffle { active, pressed }, |c, x, y, w, h| {
                c.raised(x, y, w, h, pressed);
                c.fill(x + 3, y + 6, 2, 3, lamp);
                c.text(
                    x + 8,
                    y + 5,
                    "SHUFFLE",
                    if active { pal().cyan } else { pal().cyan1 },
                );
            });
            c.put(SpriteKey::Repeat { active, pressed }, |c, x, y, w, h| {
                c.raised(x, y, w, h, pressed);
                c.fill(x + 3, y + 6, 2, 3, lamp);
                c.art(
                    x + 10,
                    y + 3,
                    &[
                        "..#....", ".######", "..#...#", "#...#..", "######.", "....#..",
                    ],
                    if active { pal().cyan } else { pal().cyan1 },
                );
            });
            c.put(SpriteKey::EqToggle { active, pressed }, |c, x, y, w, h| {
                c.raised(x, y, w, h, pressed);
                c.fill(x + 3, y + 5, 2, 2, lamp);
                c.text(
                    x + 8,
                    y + 4,
                    "EQ",
                    if active { pal().cyan } else { pal().cyan1 },
                );
            });
            c.put(
                SpriteKey::PlaylistToggle { active, pressed },
                |c, x, y, w, h| {
                    c.raised(x, y, w, h, pressed);
                    c.fill(x + 3, y + 5, 2, 2, lamp);
                    c.text(
                        x + 8,
                        y + 4,
                        "PL",
                        if active { pal().cyan } else { pal().cyan1 },
                    );
                },
            );
        }
    }
    c
}

const BANDS: [&str; 10] = [
    "70", "180", "320", "600", "1K", "3K", "6K", "12K", "14K", "16K",
];

fn eqmain_bmp() -> Canvas {
    let mut c = Canvas::new(275, 315);
    c.backdrop(0, 0, 275, 315, 31);
    c.put(SpriteKey::EqBackground, |c, x, y, w, h| {
        c.backdrop(x, y, w, h, 32);
        c.burst(x, y, w, h, 116);
        c.frame(x, y, w, h, pal().s3, pal().s0);
        style_decor(&mut *c, w, h, false);
        c.recess(86, 17, 113, 19, pal().void);
        // Slider beds and their labels.
        // Dark plaques, so the labels read against the rim-lit stone.
        c.recess(40, 37, 34, 65, pal().s0);
        c.recess(13, 104, 253, 7, pal().s0);
        c.recess(20, 37, 16, 65, pal().s0);
        c.text_centred(28, 105, "PREAMP", pal().cyan1);
        for (i, label) in BANDS.iter().enumerate() {
            let bx = 78 + 18 * i as i32;
            c.recess(bx - 1, 37, 16, 65, pal().s0);
            c.text_centred(bx + 7, 105, label, pal().cyan1);
        }
        c.text(43, 39, "+12", pal().cyan1);
        c.text(48, 67, "0", pal().cyan1);
        c.text(43, 96, "-12", pal().cyan1);
        for k in [41, 69, 98] {
            c.fill(60, k, 12, 1, pal().cyan3);
        }
        c.moss(1, 114, 273, true, 33);
        for (fx, fh, seed) in [(6, 10, 34), (64, 7, 35), (268, 9, 36)] {
            c.fern(fx, 113, fh, seed);
        }
        c.vine(77, 14, 12, 37);
        c.vine(206, 14, 7, 38);
        c.vine(3, 14, 18, 39);
        c.vine(271, 14, 22, 40);
        for (sx, sy) in [(9, 32), (210, 30), (266, 62)] {
            c.sparkle(sx, sy);
        }
        vignette(&mut *c, w, h);
    });
    for active in [true, false] {
        c.put(SpriteKey::EqTitleBar { active }, |c, x, y, w, _| {
            title_strip(c, (x, y, w), "EQUALIZER", active, 34, 116);
            // Shade and close, painted in; close also has its own sprite.
            small_button(c, x + 254, y + 3, &ICON_SHADE, false);
            small_button(c, x + 264, y + 3, &ICON_CLOSE, false);
        });
    }
    for pressed in [false, true] {
        c.put(SpriteKey::EqCloseButton { pressed }, |c, x, y, _, _| {
            small_button(c, x, y, &ICON_CLOSE, pressed)
        });
    }
    c.put(SpriteKey::EqMaximizeButtonFallback, |c, x, y, _, _| {
        small_button(c, x, y, &ICON_UNSHADE, false)
    });
    for active in [false, true] {
        for pressed in [false, true] {
            let lamp = if active { pal().cyan } else { pal().s3 };
            c.put(
                SpriteKey::EqOnButton { active, pressed },
                |c, x, y, w, h| {
                    c.raised(x, y, w, h, pressed);
                    c.fill(x + 3, y + 5, 2, 2, lamp);
                    c.text(
                        x + 9,
                        y + 4,
                        "ON",
                        if active { pal().cyan } else { pal().cyan1 },
                    );
                },
            );
            c.put(
                SpriteKey::EqAutoButton { active, pressed },
                |c, x, y, w, h| {
                    c.raised(x, y, w, h, pressed);
                    c.fill(x + 3, y + 5, 2, 2, lamp);
                    c.text(
                        x + 8,
                        y + 4,
                        "AUTO",
                        if active { pal().cyan } else { pal().cyan1 },
                    );
                },
            );
        }
    }
    for pressed in [false, true] {
        c.put(SpriteKey::EqPresetsButton { pressed }, |c, x, y, w, h| {
            c.raised(x, y, w, h, pressed);
            c.text_centred(
                x + w / 2,
                y + 4,
                "PRESETS",
                if pressed { pal().cyan } else { pal().cyan1 },
            );
        });
        c.put(SpriteKey::EqSliderThumb { pressed }, |c, x, y, w, h| {
            c.raised(x, y, w, h, pressed);
            c.fill(
                x + 2,
                y + 5,
                w - 4,
                1,
                if pressed { pal().pink } else { pal().cyan1 },
            );
        });
    }
    // Band tracks: a channel of light, cool when cut, bright when boosted.
    for p in 0..28u8 {
        c.put(SpriteKey::EqSliderBackground(p), |c, x, y, w, h| {
            c.fill(x, y, w, h, pal().s0);
            let t = f32::from(p) / 27.0;
            let col = mix(pal().pink1, pal().cyan, t);
            c.frame(x + 5, y, 4, h, pal().s0, pal().s2);
            c.fill(x + 6, y + 1, 2, h - 2, mix(col, pal().s0, 0.35));
            c.fill(x + 6, y + h / 2, 2, 1, pal().white);
        });
    }
    c.put(SpriteKey::EqGraphBackground, |c, x, y, w, h| {
        c.fill(x, y, w, h, pal().void);
        for xx in (x..x + w).step_by(4) {
            c.px(xx, y + h / 2, pal().cyan3);
        }
        for yy in (y..y + h).step_by(3) {
            for xx in (x..x + w).step_by(12) {
                c.px(xx, yy, pal().s1);
            }
        }
    });
    c.put(SpriteKey::EqGraphLineColors, |c, x, y, _, h| {
        for k in 0..h {
            let t = k as f32 / (h - 1) as f32;
            let col = if t < 0.5 {
                mix(pal().pink, pal().cyan, t * 2.0)
            } else {
                mix(pal().cyan, pal().cyan1, (t - 0.5) * 2.0)
            };
            c.px(x, y + k, col);
        }
    });
    c.put(SpriteKey::EqPreampLine, |c, x, y, w, _| {
        for xx in x..x + w {
            c.px(xx, y, if xx % 2 == 0 { pal().amber1 } else { pal().s0 });
        }
    });
    c
}

fn eq_ex_bmp() -> Canvas {
    let mut c = Canvas::new(275, 56);
    for active in [true, false] {
        c.put(SpriteKey::EqShadeBackground { active }, |c, x, y, w, h| {
            c.backdrop(x, y, w, h, 41);
            c.frame(x, y, w, h, pal().s3, pal().s0);
            c.text(
                x + 8,
                y + 4,
                "EQ",
                if active { pal().cyan1 } else { pal().s4 },
            );
            c.recess(x + 61, y + 4, 97, 6, pal().s0);
            c.recess(x + 164, y + 4, 43, 6, pal().s0);
            for xx in x + 62..x + 157 {
                c.px(xx, y + 6, pal().cyan3);
            }
            for xx in x + 165..x + 206 {
                c.px(xx, y + 6, pal().cyan3);
            }
        });
    }
    for t in [ShadeThumb::Left, ShadeThumb::Center, ShadeThumb::Right] {
        for key in [
            SpriteKey::EqShadeVolumeThumb(t),
            SpriteKey::EqShadeBalanceThumb(t),
        ] {
            c.put(key, |c, x, y, w, h| {
                c.fill(x, y, w, h, pal().cyan1);
                c.fill(x + 1, y + 1, 1, h - 2, pal().cyan);
            });
        }
    }
    c.put(SpriteKey::EqMaximizeButton, |c, x, y, _, _| {
        small_button(c, x, y, &ICON_UNSHADE, false)
    });
    c.put(SpriteKey::EqMinimizeButton, |c, x, y, _, _| {
        small_button(c, x, y, &ICON_SHADE, true)
    });
    for pressed in [false, true] {
        c.put(
            SpriteKey::EqShadeCloseButton { pressed },
            |c, x, y, _, _| small_button(c, x, y, &ICON_CLOSE, pressed),
        );
    }
    c
}

fn pledit_bmp() -> Canvas {
    let mut c = Canvas::new(280, 186);
    c.backdrop(0, 0, 280, 186, 51);
    for active in [true, false] {
        let line = if active { pal().cyan1 } else { pal().s3 };
        c.put(
            SpriteKey::PlaylistTopLeftCorner { active },
            |c, x, y, w, h| {
                c.backdrop(x, y, w, h, 52);
                c.frame(x, y, w, 1, pal().s3, pal().s3);
                c.frame(x, y, 1, h, pal().s3, pal().s3);
                for xx in x + 6..x + w {
                    c.px(xx, y + 9, line);
                }
            },
        );
        c.put(SpriteKey::PlaylistTitleBar { active }, |c, x, y, w, h| {
            c.backdrop(x, y, w, h, 53);
            c.frame(x, y, w, 1, pal().s3, pal().s3);
            c.fill(x + 12, y + 5, w - 24, 9, pal().s0);
            c.text_centred(
                x + w / 2,
                y + 7,
                "PLAYLIST",
                if active { pal().cyan } else { pal().s4 },
            );
            for xx in (x..x + 10).chain(x + w - 10..x + w) {
                c.px(xx, y + 9, line);
            }
        });
        c.put(SpriteKey::PlaylistTopTile { active }, |c, x, y, w, h| {
            c.backdrop(x, y, w, h, 54);
            c.frame(x, y, w, 1, pal().s3, pal().s3);
            for xx in x..x + w {
                c.px(xx, y + 9, line);
            }
        });
        c.put(
            SpriteKey::PlaylistTopRightCorner { active },
            |c, x, y, w, h| {
                c.backdrop(x, y, w, h, 55);
                c.frame(x, y, w, 1, pal().s3, pal().s3);
                c.fill(x + w - 1, y, 1, h, pal().s0);
                for xx in x..x + 3 {
                    c.px(xx, y + 9, line);
                }
                small_button(c, x + 4, y + 3, &ICON_SHADE, false);
                small_button(c, x + 14, y + 3, &ICON_CLOSE, false);
            },
        );
    }
    c.put(SpriteKey::PlaylistLeftTile, |c, x, y, w, h| {
        c.backdrop(x, y, w, h, 56);
        c.fill(x, y, 1, h, pal().s3);
        c.fill(x + w - 1, y, 1, h, pal().s0);
    });
    c.put(SpriteKey::PlaylistRightTile, |c, x, y, w, h| {
        c.backdrop(x, y, w, h, 57);
        c.fill(x, y, 1, h, pal().s3);
        c.fill(x + w - 1, y, 1, h, pal().s0);
        // The scrollbar's groove.
        c.fill(x + 5, y, 8, h, pal().s0);
        c.fill(x + 8, y, 2, h, pal().cyan3);
    });
    c.put(SpriteKey::PlaylistBottomTile, |c, x, y, w, h| {
        c.backdrop(x, y, w, h, 58);
        c.fill(x, y + h - 1, w, 1, pal().s0);
        c.moss(x, y + h - 2, w, true, 59);
    });
    c.put(SpriteKey::PlaylistBottomLeftCorner, |c, x, y, w, h| {
        c.backdrop(x, y, w, h, 60);
        c.fill(x, y, 1, h, pal().s3);
        c.fill(x, y + h - 1, w, 1, pal().s0);
        c.moss(x + 1, y + h - 2, w - 1, true, 61);
        let labels = [("ADD", ""), ("REM", ""), ("SEL", ""), ("MISC", "")];
        for (i, (top, bottom)) in labels.iter().enumerate() {
            menu_button(c, x + 14 + 29 * i as i32, y + 8, top, bottom, false);
        }
    });
    c.put(SpriteKey::PlaylistBottomRightCorner, |c, x, y, w, h| {
        c.backdrop(x, y, w, h, 62);
        c.fill(x + w - 1, y, 1, h, pal().s0);
        c.fill(x, y + h - 1, w, 1, pal().s0);
        c.moss(x, y + h - 2, w - 22, true, 63);
        // Running time, then mini transport and mini time.
        c.recess(x + 6, y + 9, 60, 8, pal().void);
        c.recess(x + 65, y + 22, 31, 8, pal().void);
        let icons: [[&str; 5]; 6] = [
            ["#..#", "#.##", "####", "#.##", "#..#"],
            ["#...", "##..", "###.", "##..", "#..."],
            ["##.#", "##.#", "##.#", "##.#", "##.#"],
            ["####", "####", "####", "####", "####"],
            ["#..#", "##.#", "####", "##.#", "#..#"],
            ["..#..", ".###.", "#####", ".....", "#####"],
        ];
        for (i, icon) in icons.iter().enumerate() {
            c.art(x + 5 + 10 * i as i32, y + 24, icon, pal().cyan1);
        }
        menu_button(c, x + 106, y + 8, "LIST", "", false);
        // Scroll arrows and the resize grip.
        c.art(x + 136, y + 2, &["..#...", ".###..", "#####."], pal().cyan1);
        c.art(x + 136, y + 9, &["#####.", ".###..", "..#..."], pal().cyan1);
        for k in 0..4 {
            for j in 0..=k {
                c.px(x + w - 4 - 3 * j, y + h - 4 - 3 * (k - j), pal().s4);
            }
        }
    });
    c.put(SpriteKey::PlaylistVisualizerBackground, |c, x, y, w, h| {
        c.backdrop(x, y, w, h, 64);
        c.fill(x, y + h - 1, w, 1, pal().s0);
        c.recess(x + 3, y + 8, w - 6, 21, pal().void);
        // Something in the dark: a ghost koi, or the theme's own scene.
        if theme().style == Style::Chroma {
            chroma_panel(&mut *c, x + 3, y + 8, w - 6, 21);
        } else if theme().creatures || theme().style == Style::Night {
            panel_scene(&mut *c, x + 3, y + 8, w - 6, 21);
        }
        c.moss(x, y + h - 2, w, true, 65);
        c.fern(x + 6, y + h - 2, 6, 66);
    });
    c.put(SpriteKey::PlaylistShadeBackground, |c, x, y, w, h| {
        c.backdrop(x, y, w, h, 66);
        c.fill(x, y, w, 1, pal().s3);
        c.fill(x, y + h - 1, w, 1, pal().s0);
    });
    c.put(SpriteKey::PlaylistShadeBackgroundLeft, |c, x, y, w, h| {
        c.backdrop(x, y, w, h, 67);
        c.frame(x, y, w, h, pal().s3, pal().s0);
        c.fill(x + w - 1, y, 1, h, pal().s2);
        lantern(&mut *c, x + 6, y + 2, false);
    });
    for active in [true, false] {
        c.put(
            SpriteKey::PlaylistShadeBackgroundRight { active },
            |c, x, y, w, h| {
                c.backdrop(x, y, w, h, 68);
                c.frame(x, y, w, h, pal().s3, pal().s0);
                c.fill(x, y, 1, h, pal().s2);
                // Resize grip, shade (expand) and close.
                c.art(
                    x + 21,
                    y + 4,
                    &["..#", ".##", "###"],
                    if active { pal().cyan1 } else { pal().s4 },
                );
                small_button(c, x + 29, y + 3, &ICON_UNSHADE, false);
                small_button(c, x + 39, y + 3, &ICON_CLOSE, false);
            },
        );
    }
    c.put(SpriteKey::PlaylistCloseButton, |c, x, y, _, _| {
        small_button(c, x, y, &ICON_CLOSE, true)
    });
    c.put(SpriteKey::PlaylistCollapseButton, |c, x, y, _, _| {
        small_button(c, x, y, &ICON_SHADE, true)
    });
    c.put(SpriteKey::PlaylistExpandButton, |c, x, y, _, _| {
        small_button(c, x, y, &ICON_UNSHADE, true)
    });
    for pressed in [false, true] {
        c.put(
            SpriteKey::PlaylistScrollHandle { pressed },
            |c, x, y, w, h| {
                c.raised(x, y, w, h, pressed);
                c.fill(
                    x + 2,
                    y + h / 2 - 1,
                    w - 4,
                    2,
                    if pressed { pal().cyan } else { pal().cyan1 },
                );
            },
        );
    }
    use PlaylistButton as B;
    let labels = [
        (B::AddUrl, "ADD", "URL"),
        (B::AddDir, "ADD", "DIR"),
        (B::AddFile, "ADD", "FILE"),
        (B::RemoveAll, "REM", "ALL"),
        (B::Crop, "CROP", ""),
        (B::RemoveSelected, "REM", "SEL"),
        (B::RemoveMisc, "REM", "MISC"),
        (B::InvertSelection, "INV", "SEL"),
        (B::SelectZero, "SEL", "ZERO"),
        (B::SelectAll, "SEL", "ALL"),
        (B::SortList, "SORT", "LIST"),
        (B::FileInfo, "FILE", "INFO"),
        (B::MiscOptions, "MISC", "OPTS"),
        (B::NewList, "NEW", "LIST"),
        (B::SaveList, "SAVE", "LIST"),
        (B::LoadList, "LOAD", "LIST"),
    ];
    for (button, top, bottom) in labels {
        for pressed in [false, true] {
            c.put(
                SpriteKey::PlaylistButton { button, pressed },
                |c, x, y, _, _| menu_button(c, x, y, top, bottom, pressed),
            );
        }
    }
    for menu in [
        PlaylistMenu::Add,
        PlaylistMenu::Remove,
        PlaylistMenu::Select,
        PlaylistMenu::Misc,
        PlaylistMenu::List,
    ] {
        c.put(SpriteKey::PlaylistMenuBar(menu), |c, x, y, w, h| {
            c.fill(x, y, w, h, pal().s0);
            c.fill(x + 1, y + 1, 1, h - 2, pal().cyan1);
        });
    }
    c
}

fn text_bmp() -> Canvas {
    let mut c = Canvas::new(155, 18);
    for ch in lantern_skin::font::CHARSET.chars() {
        let (_, r) = sprite::resolve(SpriteKey::Char(ch));
        c.art(r.x as i32, r.y as i32, &glyph(ch), pal().cyan);
    }
    c
}

// -- Generic windows ----------------------------------------------------------

/// Separates the title letters in `gen.bmp`: readers find each letter's end
/// by the next pixel of this colour, so no letter may contain it.
const LETTER_KEY: C = [255, 0, 255];

/// A glyph's inked width in the 4x5 font.
fn glyph_width(ch: char) -> i32 {
    glyph(ch)
        .iter()
        .filter_map(|row| row.rfind('#'))
        .max()
        .map_or(0, |i| i as i32 + 1)
}

/// The title alphabet: a row of 7px cells, A to Z, each the glyph plus a
/// 1px gap either side, on `plate` so the title's backing shows unbroken,
/// with `LETTER_KEY` between cells and at the row's start.
fn gen_letters(c: &mut Canvas, y: i32, ink: C, plate: C) {
    c.fill(0, y, 1, 7, LETTER_KEY);
    let mut x = 1;
    for ch in 'A'..='Z' {
        let w = glyph_width(ch) + 2;
        c.fill(x, y, w, 7, plate);
        c.art(x + 1, y + 1, &glyph(ch), ink);
        c.fill(x + w, y, 1, 7, LETTER_KEY);
        x += w + 1;
    }
}

/// `gen.bmp` in the theme's manner, built like the playlist's frame: a lit
/// groove along the title bar level with the title's middle, the title on a
/// dark plate, and a fine line edging the contents.
fn gen_bmp() -> Canvas {
    let p = pal();
    let mut c = Canvas::new(194, 109);
    c.backdrop(0, 0, 194, 109, 81);
    for active in [true, false] {
        let line = if active { p.cyan1 } else { p.s3 };
        let groove = |c: &mut Canvas, x0: i32, x1: i32, y: i32| {
            for xx in x0..x1 {
                c.px(xx, y + 7, line);
            }
        };
        let top = |c: &mut Canvas, x: i32, y: i32, w: i32, h: i32, seed: u32| {
            c.backdrop(x, y, w, h, seed);
            c.fill(x, y, w, 1, p.s3);
            // The frame's inner edge, where the contents begin.
            c.fill(x, y + h - 1, w, 1, p.s3);
        };
        c.put(SpriteKey::GenTopLeft { active }, |c, x, y, w, h| {
            top(c, x, y, w, h, 82);
            c.fill(x, y, 1, h, p.s3);
            lantern(&mut *c, x + 3, y + 3, active);
            groove(c, x + 13, x + w, y);
        });
        c.put(
            SpriteKey::GenTopLeftRightFill { active },
            |c, x, y, w, h| {
                top(c, x, y, w, h, 83);
                groove(c, x, x + w, y);
            },
        );
        c.put(SpriteKey::GenTopLeftEnd { active }, |c, x, y, w, h| {
            top(c, x, y, w, h, 84);
            groove(c, x, x + w - 3, y);
        });
        c.put(SpriteKey::GenTopCenterFill { active }, |c, x, y, w, h| {
            top(c, x, y, w, h, 85);
            c.fill(x, y + 3, w, 9, p.s0);
        });
        c.put(SpriteKey::GenTopRightEnd { active }, |c, x, y, w, h| {
            top(c, x, y, w, h, 86);
            groove(c, x + 3, x + w, y);
        });
        c.put(SpriteKey::GenTopRight { active }, |c, x, y, w, h| {
            top(c, x, y, w, h, 87);
            c.fill(x + w - 1, y, 1, h, p.s0);
            groove(c, x, x + 11, y);
            small_button(c, x + w - 11, y + 3, &ICON_CLOSE, false);
        });
        let (ink, row) = if active { (p.cyan, 88) } else { (p.s4, 96) };
        gen_letters(&mut c, row, ink, p.s0);
    }
    let side = |c: &mut Canvas, x: i32, y: i32, w: i32, h: i32, seed: u32, left: bool| {
        c.backdrop(x, y, w, h, seed);
        // The outer edge, then the fine line along the contents.
        if left {
            c.fill(x, y, 1, h, p.s3);
            c.fill(x + w - 1, y, 1, h, p.s3);
        } else {
            c.fill(x, y, 1, h, p.s3);
            c.fill(x + w - 1, y, 1, h, p.s0);
        }
    };
    c.put(SpriteKey::GenMiddleLeft, |c, x, y, w, h| {
        side(c, x, y, w, h, 88, true)
    });
    c.put(SpriteKey::GenMiddleLeftBottom, |c, x, y, w, h| {
        side(c, x, y, w, h, 89, true);
        c.fern(x + 4, y + h, 7, 90);
    });
    c.put(SpriteKey::GenMiddleRight, |c, x, y, w, h| {
        side(c, x, y, w, h, 91, false)
    });
    c.put(SpriteKey::GenMiddleRightBottom, |c, x, y, w, h| {
        side(c, x, y, w, h, 92, false)
    });
    let bottom = |c: &mut Canvas, x: i32, y: i32, w: i32, h: i32, seed: u32| {
        c.backdrop(x, y, w, h, seed);
        c.fill(x, y, w, 1, p.s3);
        c.fill(x, y + h - 1, w, 1, p.s0);
        c.moss(x, y + h - 2, w, true, seed + 1);
    };
    c.put(SpriteKey::GenBottomFill, |c, x, y, w, h| {
        bottom(c, x, y, w, h, 93)
    });
    c.put(SpriteKey::GenBottomLeft, |c, x, y, w, h| {
        bottom(c, x, y, w, h, 95);
        c.fill(x, y, 1, h, p.s3);
    });
    c.put(SpriteKey::GenBottomRight, |c, x, y, w, h| {
        bottom(c, x, y, w, h, 97);
        c.fill(x + w - 1, y, 1, h, p.s0);
        // The resize grip.
        for k in 0..3 {
            for j in 0..=k {
                c.px(x + w - 4 - 3 * j, y + h - 4 - 3 * (k - j), p.s4);
            }
        }
    });
    c.put(SpriteKey::GenClosePressed, |c, x, y, _, _| {
        small_button(c, x, y, &ICON_CLOSE, true)
    });
    c
}

/// Scroll arrows, 7x4 pointing up; the others are this turned.
const ARROW_UP: [&str; 4] = ["...#...", "..###..", ".#####.", "#######"];

/// `ARROW_UP` turned to face `dir` (0 up, 1 down, 2 left, 3 right), as
/// rows of a 7x7 cell.
fn arrow(dir: u8) -> Vec<String> {
    let at = |x: usize, y: usize| -> bool {
        // Map the 7x7 cell back onto the 7x4 up arrow, centred.
        let (ux, uy) = match dir {
            0 => (x, y as i32 - 1),
            1 => (x, 5 - y as i32),
            2 => (y, x as i32 - 1),
            _ => (y, 5 - x as i32),
        };
        (0..4).contains(&uy) && ARROW_UP[uy as usize].as_bytes()[ux] == b'#'
    };
    (0..7)
        .map(|y| (0..7).map(|x| if at(x, y) { '#' } else { '.' }).collect())
        .collect()
}

/// `genex.bmp`'s controls — buttons, scroll arrows, thumbs — each drawn by
/// `face(canvas, x, y, w, h, pressed)` with `ink` for arrows and grips.
fn genex_controls(
    c: &mut Canvas,
    face: impl Fn(&mut Canvas, i32, i32, i32, i32, bool),
    ink: impl Fn(bool) -> C,
) {
    for pressed in [false, true] {
        c.put(SpriteKey::GenExButton { pressed }, |c, x, y, w, h| {
            face(c, x, y, w, h, pressed)
        });
        for (dir, key) in [
            (0, SpriteKey::GenExScrollUp { pressed }),
            (1, SpriteKey::GenExScrollDown { pressed }),
            (2, SpriteKey::GenExScrollLeft { pressed }),
            (3, SpriteKey::GenExScrollRight { pressed }),
        ] {
            c.put(key, |c, x, y, w, h| {
                face(c, x, y, w, h, pressed);
                let rows = arrow(dir);
                let rows: Vec<&str> = rows.iter().map(String::as_str).collect();
                let nudge = i32::from(pressed);
                c.art(x + 3 + nudge, y + 3 + nudge, &rows, ink(pressed));
            });
        }
        c.put(
            SpriteKey::GenExVerticalThumb { pressed },
            |c, x, y, w, h| {
                face(c, x, y, w, h, pressed);
                for k in 0..3 {
                    c.fill(x + 4, y + h / 2 - 3 + 3 * k, w - 8, 1, ink(pressed));
                }
            },
        );
        c.put(
            SpriteKey::GenExHorizontalThumb { pressed },
            |c, x, y, w, h| {
                face(c, x, y, w, h, pressed);
                for k in 0..3 {
                    c.fill(x + w / 2 - 3 + 3 * k, y + 4, 1, h - 8, ink(pressed));
                }
            },
        );
    }
}

/// `genex.bmp`'s colour swatches, one every 2px along the top from x=48,
/// in the order `GenExColors` reads them.
fn genex_swatches(c: &mut Canvas, colours: [C; 22]) {
    for (i, colour) in colours.into_iter().enumerate() {
        c.px(48 + 2 * i as i32, 0, colour);
    }
}

fn genex_bmp() -> Canvas {
    let p = pal();
    let mut c = Canvas::new(130, 75);
    c.fill(0, 0, 130, 75, p.s1);
    genex_controls(
        &mut c,
        |c, x, y, w, h, pressed| c.raised(x, y, w, h, pressed),
        |pressed| if pressed { p.cyan } else { p.cyan1 },
    );
    let text = mix(p.cyan, p.cyan1, 0.5);
    let selected = mix(p.s2, p.s3, 0.5);
    genex_swatches(
        &mut c,
        [
            p.void,   // item background
            text,     // item foreground
            p.s1,     // window background
            p.cyan,   // button text
            p.rim_hi, // window text
            p.s3,     // divider
            selected, // playlist selection
            p.s2,     // list header background
            p.cyan1,  // list header text
            p.s4,     // list header frame, top and left
            p.s0,     // list header frame, bottom and right
            p.s3,     // list header frame, pressed
            p.s1,     // list header dead area
            p.s3,     // scrollbar 1
            p.s2,     // scrollbar 2
            p.s4,     // pressed scrollbar 1
            p.s3,     // pressed scrollbar 2
            p.s0,     // scrollbar dead area
            p.white,  // highlighted text
            p.s3,     // highlighted background
            p.amber,  // selected text
            selected, // selected background
        ],
    );
    c
}

// -- The fallback generic window -------------------------------------------------

/// Lantern's `gen.bmp` and `genex.bmp` for skins that bring none, in the
/// manner of the classic base skin: slate-violet bevels, twin gilt grips
/// either side of the title, green on black inside. Drawn here from scratch.
mod classic {
    use super::{
        C, Canvas, LETTER_KEY, SpriteKey, genex_controls, genex_swatches, glyph, glyph_width, mix,
    };

    const FACE: C = [46, 44, 70];
    const FACE_LOW: C = [36, 35, 56];
    const EDGE: C = [18, 18, 28];
    const BEVEL_HI: C = [104, 102, 134];
    const BEVEL_LO: C = [24, 24, 38];
    const INNER: C = [128, 126, 158];
    const GILT: C = [214, 192, 120];
    const GILT_LOW: C = [126, 110, 64];
    const DULL: C = [118, 114, 104];
    const DULL_LOW: C = [74, 72, 70];
    const INK: C = [226, 226, 234];
    const INK_DIM: C = [150, 150, 168];

    /// Face shading: lighter towards the top, as if lit from above.
    fn face(c: &mut Canvas, x: i32, y: i32, w: i32, h: i32, top: i32, span: i32) {
        for yy in y..y + h {
            let t = ((yy - top) as f32 / span as f32).clamp(0.0, 1.0);
            c.fill(x, yy, w, 1, mix(FACE, FACE_LOW, t));
        }
    }

    /// Twin raised grips across `x0..x1`, capped at whichever ends are
    /// their true ends rather than where one tile meets the next.
    fn grips(c: &mut Canvas, (x0, x1): (i32, i32), y: i32, active: bool, caps: (bool, bool)) {
        let (hi, lo) = if active {
            (GILT, GILT_LOW)
        } else {
            (DULL, DULL_LOW)
        };
        for (row, colour) in [(5, lo), (6, hi), (7, lo), (9, lo), (10, hi), (11, lo)] {
            c.fill(x0, y + row, x1 - x0, 1, colour);
        }
        for row in [5, 9] {
            if caps.0 {
                c.fill(x0, y + row, 1, 3, BEVEL_LO);
            }
            if caps.1 {
                c.fill(x1 - 1, y + row, 1, 3, BEVEL_LO);
            }
        }
    }

    /// The top bar's shared construction: dark outer edge, a bright bevel
    /// inside it, and the inner border the content hangs from.
    fn top(c: &mut Canvas, x: i32, y: i32, w: i32, h: i32) {
        face(c, x, y, w, h, y, h);
        c.fill(x, y, w, 1, EDGE);
        c.fill(x, y + 1, w, 1, BEVEL_HI);
        c.fill(x, y + h - 3, w, 1, BEVEL_LO);
        c.fill(x, y + h - 2, w, 1, INNER);
        c.fill(x, y + h - 1, w, 1, EDGE);
    }

    fn close(c: &mut Canvas, x: i32, y: i32, pressed: bool) {
        let (face, tl, br) = if pressed {
            (FACE_LOW, BEVEL_LO, BEVEL_HI)
        } else {
            (FACE, BEVEL_HI, BEVEL_LO)
        };
        c.fill(x, y, 9, 9, face);
        c.frame(x, y, 9, 9, tl, br);
        let ink = if pressed { GILT } else { INK_DIM };
        for k in 0..5 {
            c.px(x + 2 + k, y + 2 + k, ink);
            c.px(x + 6 - k, y + 2 + k, ink);
        }
    }

    fn letters(c: &mut Canvas, y: i32, ink: C) {
        c.fill(0, y, 1, 7, LETTER_KEY);
        let mut x = 1;
        for ch in 'A'..='Z' {
            let w = glyph_width(ch) + 2;
            face(c, x, y, w, 7, y - 4, 20);
            c.art(x + 1, y + 1, &glyph(ch), ink);
            c.fill(x + w, y, 1, 7, LETTER_KEY);
            x += w + 1;
        }
    }

    pub fn gen_bmp() -> Canvas {
        let mut c = Canvas::new(194, 109);
        c.fill(0, 0, 194, 109, EDGE);
        for active in [true, false] {
            c.put(SpriteKey::GenTopLeft { active }, |c, x, y, w, h| {
                top(c, x, y, w, h);
                c.fill(x, y, 1, h, EDGE);
                c.fill(x + 1, y + 1, 1, h - 2, BEVEL_HI);
                c.fill(x + 9, y + h - 2, 1, 2, INNER);
                grips(c, (x + 4, x + w), y, active, (true, false));
            });
            for (key, from, to, caps) in [
                (
                    SpriteKey::GenTopLeftRightFill { active },
                    0,
                    25,
                    (false, false),
                ),
                (SpriteKey::GenTopLeftEnd { active }, 0, 22, (false, true)),
                (SpriteKey::GenTopRightEnd { active }, 3, 25, (true, false)),
            ] {
                c.put(key, |c, x, y, w, h| {
                    top(c, x, y, w, h);
                    grips(c, (x + from, x + to.min(w)), y, active, caps);
                });
            }
            c.put(SpriteKey::GenTopCenterFill { active }, |c, x, y, w, h| {
                top(c, x, y, w, h)
            });
            c.put(SpriteKey::GenTopRight { active }, |c, x, y, w, h| {
                top(c, x, y, w, h);
                c.fill(x + w - 1, y, 1, h, EDGE);
                c.fill(x + w - 2, y + 1, 1, h - 2, BEVEL_LO);
                grips(c, (x, x + 12), y, active, (false, true));
                close(c, x + w - 11, y + 3, false);
            });
            letters(
                &mut c,
                if active { 88 } else { 96 },
                if active { INK } else { INK_DIM },
            );
        }
        let side = |c: &mut Canvas, x: i32, y: i32, w: i32, h: i32, left: bool| {
            face(c, x, y, w, h, y - 40, 120);
            if left {
                c.fill(x, y, 1, h, EDGE);
                c.fill(x + 1, y, 1, h, BEVEL_HI);
                c.fill(x + w - 3, y, 1, h, BEVEL_LO);
                c.fill(x + w - 2, y, 1, h, INNER);
                c.fill(x + w - 1, y, 1, h, EDGE);
            } else {
                c.fill(x, y, 1, h, EDGE);
                c.fill(x + 1, y, 1, h, INNER);
                c.fill(x + 2, y, 1, h, BEVEL_LO);
                c.fill(x + w - 2, y, 1, h, BEVEL_LO);
                c.fill(x + w - 1, y, 1, h, EDGE);
            }
        };
        c.put(SpriteKey::GenMiddleLeft, |c, x, y, w, h| {
            side(c, x, y, w, h, true)
        });
        c.put(SpriteKey::GenMiddleLeftBottom, |c, x, y, w, h| {
            side(c, x, y, w, h, true)
        });
        c.put(SpriteKey::GenMiddleRight, |c, x, y, w, h| {
            side(c, x, y, w, h, false)
        });
        c.put(SpriteKey::GenMiddleRightBottom, |c, x, y, w, h| {
            side(c, x, y, w, h, false)
        });
        let bottom = |c: &mut Canvas, x: i32, y: i32, w: i32, h: i32| {
            face(c, x, y, w, h, y - 10, 30);
            c.fill(x, y, w, 1, EDGE);
            c.fill(x, y + 1, w, 1, INNER);
            c.fill(x, y + 2, w, 1, BEVEL_LO);
            c.fill(x, y + h - 2, w, 1, BEVEL_LO);
            c.fill(x, y + h - 1, w, 1, EDGE);
        };
        c.put(SpriteKey::GenBottomFill, |c, x, y, w, h| {
            bottom(c, x, y, w, h)
        });
        c.put(SpriteKey::GenBottomLeft, |c, x, y, w, h| {
            bottom(c, x, y, w, h);
            c.fill(x, y, 1, h, EDGE);
            c.fill(x + 1, y + 1, 1, h - 2, BEVEL_HI);
            c.fill(x + 9, y, 1, 2, INNER);
        });
        c.put(SpriteKey::GenBottomRight, |c, x, y, w, h| {
            bottom(c, x, y, w, h);
            c.fill(x + w - 1, y, 1, h, EDGE);
            c.fill(x + w - 2, y + 1, 1, h - 2, BEVEL_LO);
            c.fill(x + w - 8, y, 1, 2, INNER);
            // The resize grip: three ridges along the diagonal.
            for k in 0..3 {
                let d = 4 + 3 * k;
                for j in 0..d {
                    c.px(x + w - 3 - d + j, y + h - 3 - j, BEVEL_HI);
                    c.px(x + w - 2 - d + j, y + h - 3 - j, BEVEL_LO);
                }
            }
        });
        c.put(SpriteKey::GenClosePressed, |c, x, y, _, _| {
            close(c, x, y, true)
        });
        c
    }

    /// A raised control in light slate, sunk when pressed.
    fn control(c: &mut Canvas, x: i32, y: i32, w: i32, h: i32, pressed: bool) {
        const FACE: C = [184, 192, 210];
        const FACE_DOWN: C = [150, 158, 178];
        const HI: C = [238, 242, 250];
        const LO: C = [110, 118, 138];
        if pressed {
            c.fill(x, y, w, h, FACE_DOWN);
            c.frame(x, y, w, h, EDGE, HI);
            c.frame(x + 1, y + 1, w - 2, h - 2, LO, FACE_DOWN);
        } else {
            c.fill(x, y, w, h, FACE);
            c.frame(x, y, w, h, HI, EDGE);
            c.frame(x + 1, y + 1, w - 2, h - 2, FACE, LO);
        }
    }

    pub fn genex_bmp() -> Canvas {
        let mut c = Canvas::new(130, 75);
        c.fill(0, 0, 130, 75, FACE);
        genex_controls(&mut c, control, |_| [36, 40, 54]);
        genex_swatches(
            &mut c,
            [
                [0, 0, 0],       // item background
                [0, 226, 0],     // item foreground
                FACE,            // window background
                [20, 20, 28],    // button text
                INK,             // window text
                [118, 118, 146], // divider
                [0, 0, 190],     // playlist selection
                [68, 66, 104],   // list header background
                INK,             // list header text
                [120, 118, 156], // list header frame, top and left
                [28, 28, 44],    // list header frame, bottom and right
                [86, 84, 120],   // list header frame, pressed
                [56, 54, 86],    // list header dead area
                [172, 176, 196], // scrollbar 1
                [128, 132, 154], // scrollbar 2
                [150, 154, 176], // pressed scrollbar 1
                [104, 108, 130], // pressed scrollbar 2
                [36, 35, 56],    // scrollbar dead area
                [255, 255, 255], // highlighted text
                [0, 0, 190],     // highlighted background
                [255, 255, 255], // selected text
                [0, 0, 150],     // selected background
            ],
        );
        c
    }
}

// -- Text files ---------------------------------------------------------------

/// Background, grid dots, 16 bar rows top to bottom, oscilloscope, peaks.
fn viscolor() -> String {
    let p = pal();
    if theme().style == Style::Chroma {
        // Loud is red and stands forward; quiet is blue and sinks.
        let mut lines = vec![p.void, p.cyan3];
        lines.extend((0..16).map(|k| if k < 9 { p.cyan } else { p.s2 }));
        lines.extend([p.cyan; 5]);
        lines.push(p.cyan);
        return lines
            .iter()
            .map(|[r, g, b]| format!("{r},{g},{b}\n"))
            .collect();
    }
    let mut lines = vec![pal().void, [24, 22, 44]];
    for k in 0..16 {
        let t = k as f32 / 15.0;
        lines.push(if t < 0.3 {
            mix(pal().white, pal().pink, t / 0.3)
        } else if t < 0.6 {
            mix(pal().pink, pal().cyan, (t - 0.3) / 0.3)
        } else {
            mix(pal().cyan, pal().cyan2, (t - 0.6) / 0.4)
        });
    }
    for k in 0..5 {
        lines.push(mix(pal().cyan, pal().cyan2, k as f32 / 4.0));
    }
    lines.push(pal().white);
    lines
        .iter()
        .map(|[r, g, b]| format!("{r},{g},{b}\n"))
        .collect()
}

fn pledit() -> String {
    let hex = |[r, g, b]: C| format!("#{r:02X}{g:02X}{b:02X}");
    let p = pal();
    format!(
        "[Text]\nNormal={}\nCurrent={}\nNormalBG={}\nSelectedBG={}\nFont={}\n",
        hex(mix(p.cyan, p.cyan1, 0.5)),
        hex(p.amber),
        hex(p.void),
        hex(mix(p.s2, p.s3, 0.5)),
        "Verdana",
    )
}

/// Darken every other row, as on a CRT.
fn scanlines(c: &mut Canvas) {
    let (w, h) = (c.0.width() as i32, c.0.height() as i32);
    for y in (1..h).step_by(2) {
        for x in 0..w {
            c.tint(x, y, [0, 0, 0], 0.3);
        }
    }
}

/// Writes every theme: the first, the default, to `assets/default-skin.wsz`,
/// built into the app; the rest to `assets/skins/`. Then the generic window
/// the parser falls back on, to `assets/fallback/`.
fn main() -> anyhow::Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    for (i, t) in THEMES.iter().enumerate() {
        THEME.with(|cell| cell.set(t));
        let out = if i == 0 {
            root.join("assets/default-skin.wsz")
        } else {
            root.join(format!("assets/skins/lantern-{}.wsz", t.name))
        };
        write_skin(&out)?;
    }
    for (name, canvas) in [
        ("gen.bmp", classic::gen_bmp()),
        ("genex.bmp", classic::genex_bmp()),
    ] {
        let out = root.join("assets/fallback").join(name);
        canvas.0.save_with_format(&out, ImageFormat::Bmp)?;
        println!("wrote {}", out.display());
    }
    Ok(())
}

fn write_skin(out: &std::path::Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(out.parent().unwrap())?;

    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    let opts = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .last_modified_time(DateTime::default());

    for id in SheetId::ALL {
        let canvas = match id {
            SheetId::Main => main_bmp(),
            SheetId::TitleBar => titlebar_bmp(),
            SheetId::CButtons => cbuttons_bmp(),
            SheetId::Numbers => numbers_bmp(),
            SheetId::PlayPaus => playpaus_bmp(),
            SheetId::MonoSter => monoster_bmp(),
            SheetId::PosBar => posbar_bmp(),
            SheetId::Volume => slider_bmp(68, 0, 68, false),
            SheetId::Balance => slider_bmp(47, 9, 38, true),
            SheetId::ShufRep => shufrep_bmp(),
            SheetId::EqMain => eqmain_bmp(),
            SheetId::EqEx => eq_ex_bmp(),
            SheetId::PlEdit => pledit_bmp(),
            SheetId::Text => text_bmp(),
            SheetId::Gen => gen_bmp(),
            SheetId::GenEx => genex_bmp(),
        };
        let mut canvas = canvas;
        if theme().scanlines {
            scanlines(&mut canvas);
        }
        let name = match id {
            SheetId::Numbers => "nums_ex.bmp".to_string(),
            _ => format!("{}.bmp", id.base_name()),
        };
        let mut bytes = Vec::new();
        canvas
            .0
            .write_to(&mut Cursor::new(&mut bytes), ImageFormat::Bmp)?;
        zip.start_file(name, opts)?;
        zip.write_all(&bytes)?;
    }
    for (name, text) in [
        ("viscolor.txt", viscolor()),
        ("pledit.txt", pledit()),
        (
            "readme.txt",
            format!(
                "Lantern ({}) - a skin for the Lantern player. Original artwork, drawn in code \
                 by crates/lantern-skin/examples/make_default_skin.rs. MIT licensed.\n",
                theme().name
            ),
        ),
    ] {
        zip.start_file(name, opts)?;
        zip.write_all(text.as_bytes())?;
    }
    let bytes = zip.finish()?.into_inner();
    std::fs::write(out, &bytes)?;
    println!("wrote {} ({} bytes)", out.display(), bytes.len());
    Ok(())
}
