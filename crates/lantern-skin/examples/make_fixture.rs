//! Generates `tests/fixtures/skins/lantern-fixture.wsz`, the committed test skin.
//!
//! All artwork is drawn here, programmatically — plain bevelled panels on
//! every sprite rectangle, tinted per sheet — so the fixture is original work
//! that can live in the repository and run in CI, unlike the third-party
//! skins in `skins/`.
//!
//! It is deliberately awkward in the ways real skins are, so that CI covers
//! the parser's corner cases:
//!
//! - sheets inside a subdirectory, some with uppercase names;
//! - `cbuttons.bmp` is really a PNG;
//! - `nums_ex.bmp` instead of `numbers.bmp`;
//! - text files with comments, CRLF, a missing `#` and a real `region.txt`;
//! - a junk file the parser must ignore;
//! - `gen.bmp`'s title letters in widths of 3 to 6, so the scan for their
//!   edges is tested, and `genex.bmp`'s colour swatches, each distinct.
//!
//! Each sheet's top-left pixel encodes its [`SheetId`] (`red = index * 16`),
//! so tests can check that every file landed in the right slot.
//!
//! Output is deterministic: rerunning produces a byte-identical file.
//!
//! ```bash
//! cargo run -p lantern-skin --example make_fixture
//! ```

use std::io::{Cursor, Write};
use std::path::PathBuf;

use image::{ImageFormat, Rgb, RgbImage};
use lantern_skin::SheetId;
use lantern_skin::sprite::{self, SpriteKey};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipWriter};

const LIGHT: [u8; 3] = [120, 132, 170];
const DARK: [u8; 3] = [24, 28, 40];
const BACKDROP: [u8; 3] = [16, 18, 26];

fn main() -> anyhow::Result<()> {
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/skins/lantern-fixture.wsz");
    std::fs::create_dir_all(out.parent().unwrap())?;

    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    let opts = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .last_modified_time(DateTime::default());

    for id in SheetId::ALL {
        let img = draw_sheet(id);
        let (name, format) = match id {
            SheetId::Main => ("Fixture/MAIN.BMP".to_string(), ImageFormat::Bmp),
            SheetId::CButtons => ("Fixture/cbuttons.bmp".to_string(), ImageFormat::Png),
            SheetId::Numbers => ("Fixture/nums_ex.bmp".to_string(), ImageFormat::Bmp),
            _ => (format!("Fixture/{}.bmp", id.base_name()), ImageFormat::Bmp),
        };
        let mut bytes = Vec::new();
        img.write_to(&mut Cursor::new(&mut bytes), format)?;
        zip.start_file(name, opts)?;
        zip.write_all(&bytes)?;
    }

    for (name, text) in [
        ("Fixture/VISCOLOR.TXT", VISCOLOR),
        ("Fixture/pledit.txt", PLEDIT),
        ("Fixture/region.txt", REGION),
        ("Fixture/Thumbs.db", "not a skin file"),
    ] {
        zip.start_file(name, opts)?;
        zip.write_all(text.replace('\n', "\r\n").as_bytes())?;
    }

    let bytes = zip.finish()?.into_inner();
    std::fs::write(&out, &bytes)?;
    println!("wrote {} ({} bytes)", out.display(), bytes.len());
    Ok(())
}

fn draw_sheet(id: SheetId) -> RgbImage {
    let (w, h) = sheet_size(id);
    let mut img = RgbImage::from_pixel(w, h, Rgb(BACKDROP));
    // Each sheet gets its own panel tint, so sprites from one sheet stand
    // out when composited over another's.
    let i = id as u8;
    let panel = [52 + i * 7, 66 + (i % 3) * 12, 110 - i * 4];

    for key in sprite::all_keys() {
        let (sheet, r) = sprite::resolve(key);
        if sheet != id {
            continue;
        }
        let (tl, br) = if is_pressed(key) {
            (DARK, LIGHT)
        } else {
            (LIGHT, DARK)
        };
        for y in r.y..r.y + r.h {
            for x in r.x..r.x + r.w {
                let c = if x == r.x || y == r.y {
                    tl
                } else if x == r.x + r.w - 1 || y == r.y + r.h - 1 {
                    br
                } else {
                    panel
                };
                img.put_pixel(x, y, Rgb(c));
            }
        }
    }

    match id {
        SheetId::Gen => {
            for y in [88, 96] {
                gen_letters(&mut img, y);
            }
        }
        SheetId::GenEx => {
            for slot in 0..22u8 {
                img.put_pixel(48 + 2 * u32::from(slot), 0, Rgb(genex_swatch(slot)));
            }
        }
        _ => {}
    }

    img.put_pixel(0, 0, Rgb([(id as u8) * 16, 0, 0]));
    img
}

/// Letter `i` of the alphabet is `3 + i % 4` wide.
fn gen_letters(img: &mut RgbImage, y: u32) {
    const KEY: [u8; 3] = [0, 200, 255];
    let mut x = 0;
    for i in 0..=26 {
        for yy in y..y + 7 {
            img.put_pixel(x, yy, Rgb(KEY));
        }
        if i == 26 {
            break;
        }
        let w = 3 + i % 4;
        for yy in y..y + 7 {
            for xx in x + 1..x + 1 + w {
                img.put_pixel(xx, yy, Rgb(LIGHT));
            }
        }
        x += w + 1;
    }
}

/// `genex.bmp` colour slot `slot`.
fn genex_swatch(slot: u8) -> [u8; 3] {
    [slot * 10, 100, 200]
}

/// Standard sizes, except the Numbers slot, which ships as `nums_ex.bmp`.
fn sheet_size(id: SheetId) -> (u32, u32) {
    match id {
        SheetId::Numbers => (108, 13),
        _ => sprite::standard_size(id),
    }
}

fn is_pressed(key: SpriteKey) -> bool {
    // Every key with a `pressed` field formats it the same way; matching the
    // debug string avoids a 30-arm match that must track the enum.
    format!("{key:?}").contains("pressed: true")
}

const VISCOLOR: &str = "\
0,0,0 // background
40,40,56 // dots
255,64,64
240,80,64
224,96,64
208,112,64
192,128,64
176,144,64
160,160,64
144,176,64
128,192,64
112,208,64
96,224,64
80,240,64
64,255,64
64,240,80
64,224,96
64,208,112
255,255,255
224,224,224
192,192,192
160,160,160
128,128,128
96,96,96
";

const PLEDIT: &str = "\
; lantern test fixture
[Text]
Normal=#A0C0FF
Current=FFFFFF
NormalBG=#101218
SelectedBG=#30406080
Font=Verdana
";

/// Main window with its four corners clipped by 2px, as an octagon; the
/// shade window as a plain rectangle.
const REGION: &str = "\
; lantern test fixture
[Normal]
NumPoints=8
PointList=2,0 273,0 275,2 275,114 273,116 2,116 0,114 0,2
[WindowShade]
NumPoints=4
PointList=0,0, 275,0, 275,14, 0,14
";
