//! Generates Lantern's app icon, `macos/AppIcon.icns`, and a PNG of it for
//! the README, `macos/AppIcon.png`: a pixel-art hurricane lantern, lit, in
//! the night sky of the built-in skin, its glass rim-lit like Night's
//! controls and its glow the only light but the stars.
//!
//! Drawn on a 32x32 grid and scaled up in whole steps, so it stays crisp at
//! every size; 16px has a drawing of its own, as a scaled-down lantern would
//! blur. The sky fills a plain square, cut to the macOS icon grid's rounded
//! square, 824 of the 1024 points, centred, with smooth corners: macOS 26
//! sets an icon that doesn't fill that shape, as stepped pixel corners
//! don't, small on a grey plate.
//!
//! Output is deterministic. With `--preview <dir>`, it also writes the
//! sizes there as PNGs.
//!
//! ```bash
//! cargo run -p lantern-skin --example make_icon
//! ```

use std::path::{Path, PathBuf};

use image::{ImageFormat, Rgba, RgbaImage, imageops};

type C = [u8; 3];

// Night's own colours, from the built-in skin.
const S0: C = [4, 5, 18];
const S1: C = [8, 10, 30];
const S2: C = [14, 18, 48];
const S3: C = [26, 34, 86];
const RIM: C = [112, 132, 255];
const RIM_HI: C = [204, 214, 255];
const STAR: C = [230, 236, 255];
/// The lantern's haze on the sky: rosy, which reads as light on the blue
/// where amber turns brown.
const HAZE: C = [236, 170, 210];

/// A 4x4 ordered dither, as Night's own art uses.
const BAYER: [[f32; 4]; 4] = [
    [0.0, 8.0, 2.0, 10.0],
    [12.0, 4.0, 14.0, 6.0],
    [3.0, 11.0, 1.0, 9.0],
    [15.0, 7.0, 13.0, 5.0],
];

fn mix(a: C, b: C, t: f32) -> C {
    let t = t.clamp(0.0, 1.0);
    std::array::from_fn(|i| (f32::from(a[i]) * (1.0 - t) + f32::from(b[i]) * t).round() as u8)
}

/// The lantern's inks: E rim light and L its bright lip, G the dark glass
/// faces of Night's controls; A and a amber glass, Y flame, W its white
/// heart, o its orange foot, h a highlight on the glass.
fn ink(c: char) -> Option<C> {
    Some(match c {
        'E' => RIM,
        'L' => RIM_HI,
        'G' => S1,
        'A' => [255, 190, 80],
        'a' => [200, 120, 40],
        'Y' => [255, 230, 140],
        'W' => [255, 252, 225],
        'o' => [255, 140, 40],
        'h' => [255, 226, 180],
        _ => return None,
    })
}

/// The hurricane lantern: wire handle, cap and chimney, globe, fuel tank.
const HURRICANE: [&str; 23] = [
    ".....EEEE.....",
    "....E....E....",
    "...E......E...",
    "..E........E..",
    "..E...EE...E..",
    "..E..ELLE..E..",
    "..E..EGGE..E..",
    "..E.EELLEE.E..",
    "..EEEEEEEEEE..",
    "...EhAAAAAE...",
    "...EhAYYAAE...",
    "..EhAYWWYAAE..",
    "..EAYWWWWYAE..",
    "..EAYWWWWYAE..",
    "..EAYYWWYYAE..",
    "..EAAYYYYAAE..",
    "...EAAooAAE...",
    "...EaAAAAaE...",
    "..EEEEEEEEEE..",
    "..ELLLLLLLLE..",
    "..EGGGGGGGGE..",
    "..EGGGGGGGGE..",
    "...EEEEEEEE...",
];

/// The hurricane for 16px.
const SMALL_HURRICANE: [&str; 11] = [
    "..EEEEEE..",
    ".E......E.",
    ".E..LL..E.",
    ".EEEEEEEE.",
    "...EAYE...",
    "..EAYWYE..",
    "..EAYWYE..",
    "...EAaE...",
    ".EEEEEEEE.",
    ".ELLLLLLE.",
    "..EEEEEE..",
];

/// The 32-cell drawing, from 32px up.
fn big() -> Grid {
    let mut g = night_sky(32, 3, 28);
    for (x, y) in [(5, 15), (27, 13), (7, 26), (24, 26)] {
        g.set(x, y, STAR);
    }
    for (x, y) in [(6, 7), (25, 6), (26, 21)] {
        twinkle(&mut g, x, y);
    }
    let (x, y) = (9, 4);
    glow(&mut g, (x as f32 + 6.5, y as f32 + 13.5), 1.8);
    g.art(x, y, &HURRICANE);
    g
}

/// The 16-cell drawing, for 16px.
fn small() -> Grid {
    let mut g = night_sky(16, 1, 14);
    g.set(2, 4, STAR);
    g.set(13, 3, STAR);
    g.art(3, 2, &SMALL_HURRICANE);
    g
}

/// Night's sky in the square from `lo` to `hi`, a little past the icon
/// grid's so the mask cuts it: bands from near-black at the top to deep
/// blue below, blended with the dither as the skin's own is, and a faint
/// rim of light along the top.
fn night_sky(n: i32, lo: i32, hi: i32) -> Grid {
    let mut g = Grid::new(n);
    g.square(lo, hi);
    for y in lo..=hi {
        for x in lo..=hi {
            if g.get(x, y).is_none() {
                continue;
            }
            let level = (y - lo) as f32 / (hi - lo) as f32 * 3.0;
            let threshold = BAYER[(y % 4) as usize][(x % 4) as usize] / 16.0;
            let band = level.floor() as usize + usize::from(level.fract() > threshold);
            g.set(x, y, [S0, S1, S2, S3][band.min(3)]);
        }
    }
    for x in lo + 1..hi {
        g.tint(x, lo, RIM, 0.35);
    }
    g
}

/// A star with four short arms.
fn twinkle(g: &mut Grid, x: i32, y: i32) {
    g.set(x, y, STAR);
    for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
        g.tint(x + dx, y + dy, RIM, 0.7);
    }
}

/// The lantern's haze around the flame at `(fx, fy)`, in stepped rings,
/// `strength` times the usual size and brightness.
fn glow(g: &mut Grid, (fx, fy): (f32, f32), strength: f32) {
    let radii = [6.0, 8.5, 10.5].map(|r| r * strength.sqrt());
    for y in 0..g.n {
        for x in 0..g.n {
            let r = ((x as f32 - fx).powi(2) + (y as f32 - fy).powi(2)).sqrt();
            let t = if r < radii[0] {
                0.16
            } else if r < radii[1] {
                0.08
            } else if r < radii[2] {
                0.04
            } else {
                continue;
            };
            g.tint(x, y, HAZE, t * strength);
        }
    }
}

/// A grid of cells, `None` where the icon is transparent.
struct Grid {
    n: i32,
    cells: Vec<Option<C>>,
}

impl Grid {
    fn new(n: i32) -> Self {
        Self {
            n,
            cells: vec![None; (n * n) as usize],
        }
    }

    fn get(&self, x: i32, y: i32) -> Option<C> {
        (x >= 0 && y >= 0 && x < self.n && y < self.n)
            .then(|| self.cells[(y * self.n + x) as usize])
            .flatten()
    }

    fn set(&mut self, x: i32, y: i32, c: C) {
        if x >= 0 && y >= 0 && x < self.n && y < self.n {
            self.cells[(y * self.n + x) as usize] = Some(c);
        }
    }

    /// Tint a cell that is already there.
    fn tint(&mut self, x: i32, y: i32, c: C, t: f32) {
        if let Some(under) = self.get(x, y) {
            self.set(x, y, mix(under, c, t));
        }
    }

    /// Draw `rows` in the inks, only within the square.
    fn art(&mut self, x: i32, y: i32, rows: &[&str]) {
        for (dy, row) in rows.iter().enumerate() {
            for (dx, ch) in row.chars().enumerate() {
                let (px, py) = (x + dx as i32, y + dy as i32);
                if let Some(c) = ink(ch)
                    && self.get(px, py).is_some()
                {
                    self.set(px, py, c);
                }
            }
        }
    }

    /// The square from `lo` to `hi` inclusive.
    fn square(&mut self, lo: i32, hi: i32) {
        for y in lo..=hi {
            for x in lo..=hi {
                self.set(x, y, S0);
            }
        }
    }

    fn scaled(&self, size: u32) -> RgbaImage {
        let mut img = RgbaImage::new(self.n as u32, self.n as u32);
        for y in 0..self.n {
            for x in 0..self.n {
                if let Some([r, g, b]) = self.get(x, y) {
                    img.put_pixel(x as u32, y as u32, Rgba([r, g, b, 255]));
                }
            }
        }
        imageops::resize(&img, size, size, imageops::FilterType::Nearest)
    }
}

/// The image for a `size` in pixels: the small drawing at 16, the big one
/// from 32 up.
fn render(size: u32) -> RgbaImage {
    let mut img = if size < 32 {
        small().scaled(size)
    } else {
        big().scaled(size)
    };
    mask(&mut img);
    img
}

/// Cut `img` to the macOS icon grid's rounded square: 824 of 1024 wide,
/// centred, corners of radius 185, edges antialiased.
fn mask(img: &mut RgbaImage) {
    let size = img.width() as f32;
    let half = size * 824.0 / 1024.0 / 2.0;
    let radius = size * 185.0 / 1024.0;
    let centre = size / 2.0;
    for (x, y, px) in img.enumerate_pixels_mut() {
        // Distance from the pixel's centre to the shape's edge, outside
        // positive.
        let qx = (x as f32 + 0.5 - centre).abs() - (half - radius);
        let qy = (y as f32 + 0.5 - centre).abs() - (half - radius);
        let d = qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - radius;
        let cover = (0.5 - d).clamp(0.0, 1.0);
        px[3] = (f32::from(px[3]) * cover).round() as u8;
    }
}

fn png(img: &RgbaImage) -> anyhow::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut bytes), ImageFormat::Png)?;
    Ok(bytes)
}

/// An `.icns`: PNGs under their type codes, the sizes macOS asks for.
fn icns() -> anyhow::Result<Vec<u8>> {
    let mut body = Vec::new();
    for (code, size) in [
        (b"icp4", 16),
        (b"ic11", 32),
        (b"icp5", 32),
        (b"ic12", 64),
        (b"ic07", 128),
        (b"ic13", 256),
        (b"ic08", 256),
        (b"ic14", 512),
        (b"ic09", 512),
        (b"ic10", 1024),
    ] {
        let data = png(&render(size))?;
        body.extend_from_slice(code);
        body.extend_from_slice(&(data.len() as u32 + 8).to_be_bytes());
        body.extend_from_slice(&data);
    }
    let mut out = b"icns".to_vec();
    out.extend_from_slice(&(body.len() as u32 + 8).to_be_bytes());
    out.extend_from_slice(&body);
    Ok(out)
}

fn preview(dir: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(dir)?;
    for size in [16, 32, 128, 512] {
        render(size).save(dir.join(format!("icon-{size}.png")))?;
    }
    println!("wrote previews to {}", dir.display());
    Ok(())
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == "--preview") {
        let dir = args
            .get(i + 1)
            .map_or_else(|| PathBuf::from("."), PathBuf::from);
        preview(&dir)?;
    }
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let out = root.join("macos/AppIcon.icns");
    let bytes = icns()?;
    std::fs::write(&out, &bytes)?;
    println!("wrote {} ({} bytes)", out.display(), bytes.len());
    // For the README.
    let out = root.join("macos/AppIcon.png");
    std::fs::write(&out, png(&render(256))?)?;
    println!("wrote {}", out.display());
    Ok(())
}
