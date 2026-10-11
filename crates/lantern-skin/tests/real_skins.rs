//! Parses the real acceptance skins.
//!
//! These `.wsz` files are third-party copyrighted artwork and deliberately not
//! committed (see `docs/legal.md`), so they live in a gitignored `skins/`
//! directory. When absent — as in CI — each test prints a loud SKIPPED line
//! and passes. CI coverage of the parser comes from `fixture_skin.rs`
//! instead; these tests add real-world breadth on a developer machine.

use std::path::PathBuf;

fn skin_bytes(name: &str) -> Option<Vec<u8>> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../skins")
        .join(name);
    match std::fs::read(&path) {
        Ok(bytes) => Some(bytes),
        Err(_) => {
            eprintln!("SKIPPED: {name} not present at {}", path.display());
            None
        }
    }
}

#[test]
fn base_291_parses_with_all_sheets() {
    let Some(bytes) = skin_bytes("base-2.91.wsz") else {
        return;
    };
    let skin = lantern_skin::parse(&bytes).expect("base-2.91 must parse");

    // The canonical full skin: every sheet should be present.
    for id in lantern_skin::SheetId::ALL {
        assert!(
            skin.sheet(id).is_some(),
            "base-2.91 should supply {}",
            id.base_name()
        );
    }

    // main.bmp is RLE8-compressed 8bpp; this asserts the decoder handled it.
    let main = skin.sheet(lantern_skin::SheetId::Main).unwrap();
    assert_eq!((main.width, main.height), (275, 116));
    assert_eq!(main.pixels.len(), 275 * 116 * 4);

    // A decode that silently produced a blank image would still satisfy the
    // dimensions, so require actual variation.
    let distinct: std::collections::HashSet<_> = main.pixels.chunks(4).collect();
    assert!(
        distinct.len() > 8,
        "main.bmp decoded to {} distinct colours — looks blank",
        distinct.len()
    );
}

#[test]
fn cbuttons_has_expected_geometry() {
    let Some(bytes) = skin_bytes("base-2.91.wsz") else {
        return;
    };
    let skin = lantern_skin::parse(&bytes).unwrap();
    let cb = skin.sheet(lantern_skin::SheetId::CButtons).unwrap();
    // Six transport buttons, normal over pressed.
    assert_eq!((cb.width, cb.height), (136, 36));
}

#[test]
fn minimal_skin_parses_without_optional_sheets() {
    let Some(bytes) = skin_bytes("OS8 AMP - Aquamarine.wsz") else {
        return;
    };
    let skin = lantern_skin::parse(&bytes).expect("minimal skin must parse");
    assert!(skin.sheet(lantern_skin::SheetId::Main).is_some());
    // Ships nums_ex.bmp rather than numbers.bmp; unification should mean the
    // Numbers slot is filled either way.
    assert!(
        skin.sheet(lantern_skin::SheetId::Numbers).is_some(),
        "nums_ex.bmp should populate the Numbers slot"
    );
}

#[test]
fn mixed_case_and_junk_entries_parse() {
    let Some(bytes) = skin_bytes("Blue Plasma.wsz") else {
        return;
    };
    // Ships Cbuttons.bmp / Numbers.bmp / Eqmain.bmp plus .psd junk.
    let skin = lantern_skin::parse(&bytes).expect("mixed-case skin must parse");
    assert!(skin.sheet(lantern_skin::SheetId::CButtons).is_some());
    assert!(skin.sheet(lantern_skin::SheetId::Numbers).is_some());
}

#[test]
fn uppercase_filenames_parse() {
    let Some(bytes) = skin_bytes("Radar_Amp.wsz") else {
        return;
    };
    let skin = lantern_skin::parse(&bytes).expect("uppercase skin must parse");
    assert!(skin.sheet(lantern_skin::SheetId::Main).is_some());
}

#[test]
fn every_sprite_fits_the_reference_skin() {
    let Some(bytes) = skin_bytes("base-2.91.wsz") else {
        return;
    };
    let skin = lantern_skin::parse(&bytes).unwrap();
    let mut misfits = Vec::new();
    for key in lantern_skin::sprite::all_keys() {
        let (id, r) = skin.resolve(key);
        let sheet = skin.sheet(id).unwrap();
        if r.x + r.w > sheet.width || r.y + r.h > sheet.height {
            misfits.push(format!(
                "{key:?} -> {r:?} in {}.bmp ({}x{})",
                id.base_name(),
                sheet.width,
                sheet.height
            ));
        }
    }
    assert!(misfits.is_empty(), "{}", misfits.join("\n"));
}

#[test]
fn nums_ex_minus_resolves_to_its_own_cells() {
    let Some(bytes) = skin_bytes("OS8 AMP - Aquamarine.wsz") else {
        return;
    };
    let skin = lantern_skin::parse(&bytes).unwrap();
    let (_, minus) = skin.resolve(lantern_skin::SpriteKey::MinusSign);
    assert_eq!((minus.x, minus.w, minus.h), (99, 9, 13));
    let nums = skin.sheet(lantern_skin::SheetId::Numbers).unwrap();
    assert!(nums.width >= 108, "nums_ex.bmp is {}px wide", nums.width);
}

#[test]
fn text_files_parse_from_real_skins() {
    if let Some(bytes) = skin_bytes("base-2.91.wsz") {
        let skin = lantern_skin::parse(&bytes).unwrap();
        assert_eq!(skin.playlist_style.selected_bg, [0x00, 0x00, 0xC6]);
        assert_eq!(skin.viscolors[2], [239, 49, 16]);
        // Its region.txt is documentation only — every line a comment.
        assert_eq!(skin.region, lantern_skin::Region::default());
    }
    if let Some(bytes) = skin_bytes("Blue Plasma.wsz") {
        let skin = lantern_skin::parse(&bytes).unwrap();
        assert_eq!(skin.playlist_style.font, "Tahoma");
        assert_eq!(skin.playlist_style.normal, [0x56, 0xC6, 0xFF]);
        // Zero-padded, uncommented triples.
        assert_eq!(skin.viscolors[0], [0, 39, 63]);
        assert_eq!(skin.viscolors[2], [34, 162, 218]);
    }
    if let Some(bytes) = skin_bytes("OS8 AMP - Aquamarine.wsz") {
        let skin = lantern_skin::parse(&bytes).unwrap();
        // Comment header before [Text]; tab-separated viscolor annotations.
        assert_eq!(skin.playlist_style.normal, [0x00, 0xCF, 0x90]);
        assert_eq!(skin.viscolors[2], [0, 207, 144]);
    }
}
