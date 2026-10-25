//! Parses the committed original-artwork fixture skin.
//!
//! Unlike `real_skins.rs`, this always runs — including in CI. Regenerate the
//! fixture with `cargo run -p lantern-skin --example make_fixture`; the example's
//! header lists the corner cases it deliberately contains.

use lantern_skin::{SheetId, Skin, SpriteKey};

fn fixture() -> Skin {
    let bytes = include_bytes!("../../../tests/fixtures/skins/lantern-fixture.wsz");
    lantern_skin::parse(bytes).expect("fixture skin must parse")
}

#[test]
fn every_sheet_lands_in_its_own_slot() {
    let skin = fixture();
    for id in SheetId::ALL {
        let sheet = skin
            .sheet(id)
            .unwrap_or_else(|| panic!("{} missing", id.base_name()));
        // The generator stamps each sheet's index into its top-left pixel.
        assert_eq!(
            sheet.pixel(0, 0),
            [(id as u8) * 16, 0, 0, 255],
            "{} holds the wrong file",
            id.base_name()
        );
    }
}

#[test]
fn every_sprite_fits_the_fixture() {
    let skin = fixture();
    for key in lantern_skin::sprite::all_keys() {
        let (id, r) = skin.resolve(key);
        let sheet = skin.sheet(id).unwrap();
        assert!(
            r.x + r.w <= sheet.width && r.y + r.h <= sheet.height,
            "{key:?} -> {r:?} overflows {}.bmp",
            id.base_name()
        );
    }
}

#[test]
fn png_named_bmp_decodes() {
    let skin = fixture();
    let cb = skin.sheet(SheetId::CButtons).unwrap();
    assert_eq!((cb.width, cb.height), (136, 36));
}

#[test]
fn nums_ex_supplies_numbers_and_its_own_minus() {
    let skin = fixture();
    assert_eq!(skin.sheet(SheetId::Numbers).unwrap().width, 108);
    let (_, minus) = skin.resolve(SpriteKey::MinusSign);
    assert_eq!((minus.x, minus.y, minus.w, minus.h), (99, 0, 9, 13));
    // Digits are laid out identically in both sheets.
    assert_eq!(
        skin.resolve(SpriteKey::Digit(4)),
        lantern_skin::sprite::resolve(SpriteKey::Digit(4))
    );
}

#[test]
fn text_files_parse() {
    let skin = fixture();
    assert_eq!(skin.viscolors[1], [40, 40, 56]);
    assert_eq!(skin.viscolors[23], [96, 96, 96]);

    let style = &skin.playlist_style;
    assert_eq!(style.normal, [0xA0, 0xC0, 0xFF]);
    assert_eq!(style.current, [0xFF, 0xFF, 0xFF], "missing '#'");
    assert_eq!(style.selected_bg, [0x30, 0x40, 0x60], "overlong colour");
    assert_eq!(style.font, "Verdana");

    assert_eq!(skin.region.normal.len(), 1);
    assert_eq!(skin.region.normal[0].points.len(), 8);
    assert_eq!(skin.region.windowshade.len(), 1);
    assert!(skin.region.equalizer.is_empty());
}

#[test]
fn sprites_carry_the_generated_bevel() {
    let skin = fixture();
    let (id, r) = skin.resolve(SpriteKey::Play { pressed: false });
    let sheet = skin.sheet(id).unwrap();
    let top_left = sheet.pixel(r.x, r.y);
    let (_, r) = skin.resolve(SpriteKey::Play { pressed: true });
    let pressed_top_left = sheet.pixel(r.x, r.y);
    assert_ne!(
        top_left, pressed_top_left,
        "pressed frames are drawn with an inverted bevel"
    );
}

#[test]
fn gen_letters_are_found_by_their_edges() {
    let skin = fixture();
    assert!(!skin.supplied(SheetId::Gen), "the fixture brings its own");
    let font = skin.gen_font();
    // The fixture's letters run 3, 4, 5, 6 wide, then repeat, with a 1px
    // separator before each.
    let a = font.letter('A', true).unwrap();
    assert_eq!((a.x, a.y, a.w, a.h), (1, 88, 3, 7));
    let b = font.letter('b', true).unwrap();
    assert_eq!((b.x, b.w), (5, 4), "case-insensitive");
    let e = font.letter('E', false).unwrap();
    assert_eq!((e.x, e.y, e.w), (1 + 4 + 5 + 6 + 7, 96, 3));
    assert_eq!(font.letter('7', true), None);
    assert_eq!(font.width("AB C", true), 3 + 4 + 5 + 5);
}

#[test]
fn genex_colours_come_from_their_swatches() {
    let skin = fixture();
    let c = skin.genex_colors();
    assert_eq!(c.item_background, [0, 100, 200]);
    assert_eq!(c.window_background, [20, 100, 200]);
    assert_eq!(c.list_background_selected, [210, 100, 200]);
}
