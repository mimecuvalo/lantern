//! Winamp `.eqf` equaliser presets: read and write.
//!
//! Layout from Webamp's `winamp-eqf` package (ISC, Copyright (c) Jordan
//! Eldredge): the header `Winamp EQ library file v1.1`, the four bytes
//! `\x1a!--`, then per preset a 257-byte NUL-padded name and eleven level
//! bytes — ten bands, bass first, then the preamp. A level byte `b` means
//! slider value `64 - b` on a 1..=64 scale, so 0 is +12 dB and 63 is -12 dB.
//!
//! Single-preset `.eqf` files and multi-preset libraries share the format.

use crate::eq::EqSettings;

const HEADER: &[u8] = b"Winamp EQ library file v1.1";
const MARKER: &[u8] = b"\x1a!--";
const NAME_LEN: usize = 257;
const PRESET_LEN: usize = NAME_LEN + 11;

#[derive(Debug, Clone, PartialEq)]
pub struct Preset {
    pub name: String,
    pub preamp_db: f32,
    pub band_gains_db: [f32; 10],
}

impl Preset {
    /// These levels on `settings`, leaving whether the EQ is on alone.
    pub fn apply_to(&self, settings: &mut EqSettings) {
        settings.preamp_db = self.preamp_db;
        settings.band_gains_db = self.band_gains_db;
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EqfError {
    #[error("not a Winamp EQ preset file")]
    NotEqf,
}

fn db_from_byte(b: u8) -> f32 {
    let value = 64.0 - f32::from(b.min(63));
    (value - 1.0) * 24.0 / 63.0 - 12.0
}

fn byte_from_db(db: f32) -> u8 {
    let value = ((db.clamp(-12.0, 12.0) + 12.0) * 63.0 / 24.0).round() + 1.0;
    (64.0 - value) as u8
}

pub fn parse(bytes: &[u8]) -> Result<Vec<Preset>, EqfError> {
    let body = bytes
        .strip_prefix(HEADER)
        .and_then(|b| b.strip_prefix(MARKER))
        .ok_or(EqfError::NotEqf)?;
    Ok(body
        .as_chunks::<PRESET_LEN>()
        .0
        .iter()
        .map(|p| {
            let name = &p[..NAME_LEN];
            let end = name.iter().position(|&b| b == 0).unwrap_or(NAME_LEN);
            let levels = &p[NAME_LEN..];
            Preset {
                // Names are Windows-1252 in practice; Latin-1 is close enough
                // for a label.
                name: name[..end].iter().map(|&b| char::from(b)).collect(),
                band_gains_db: std::array::from_fn(|i| db_from_byte(levels[i])),
                preamp_db: db_from_byte(levels[10]),
            }
        })
        .collect())
}

pub fn write(presets: &[Preset]) -> Vec<u8> {
    let mut out = [HEADER, MARKER].concat();
    for p in presets {
        let mut name: Vec<u8> = p
            .name
            .chars()
            .map(|c| u8::try_from(c).unwrap_or(b'?'))
            .take(NAME_LEN - 1)
            .collect();
        name.resize(NAME_LEN, 0);
        out.extend(name);
        out.extend(p.band_gains_db.iter().map(|&db| byte_from_db(db)));
        out.push(byte_from_db(p.preamp_db));
    }
    out
}

/// Winamp's stock presets, as Webamp ships them in
/// `packages/webamp/presets/builtin.json` (MIT, Copyright (c) 2015 Jordan
/// Eldredge): slider values on the 1..=64 scale, ten bands bass first, then
/// the preamp.
const BUILTIN: [(&str, [u8; 11]); 17] = [
    ("Classical", [33, 33, 33, 33, 33, 33, 20, 20, 20, 16, 33]),
    ("Club", [33, 33, 38, 42, 42, 42, 38, 33, 33, 33, 33]),
    ("Dance", [48, 44, 36, 32, 32, 22, 20, 20, 32, 32, 33]),
    (
        "Laptop speakers/headphones",
        [40, 50, 41, 26, 28, 35, 40, 48, 53, 56, 33],
    ),
    ("Large hall", [49, 49, 42, 42, 33, 24, 24, 24, 33, 33, 33]),
    ("Party", [44, 44, 33, 33, 33, 33, 33, 33, 44, 44, 33]),
    ("Pop", [29, 40, 44, 45, 41, 30, 28, 28, 29, 29, 33]),
    ("Reggae", [33, 33, 31, 22, 33, 43, 43, 33, 33, 33, 33]),
    ("Rock", [45, 40, 23, 19, 26, 39, 47, 50, 50, 50, 33]),
    ("Soft", [40, 35, 30, 28, 30, 39, 46, 48, 50, 52, 33]),
    ("Ska", [28, 24, 25, 31, 39, 42, 47, 48, 50, 48, 33]),
    ("Full Bass", [48, 48, 48, 42, 35, 25, 18, 15, 14, 14, 33]),
    ("Soft Rock", [39, 39, 36, 31, 25, 23, 26, 31, 37, 47, 33]),
    ("Full Treble", [16, 16, 16, 25, 37, 50, 58, 58, 58, 60, 33]),
    (
        "Full Bass & Treble",
        [44, 42, 33, 20, 24, 35, 46, 50, 52, 52, 33],
    ),
    ("Live", [24, 33, 39, 41, 42, 42, 39, 37, 37, 36, 33]),
    ("Techno", [45, 42, 33, 23, 24, 33, 45, 48, 48, 47, 33]),
];

fn db_from_value(v: u8) -> f32 {
    (f32::from(v.clamp(1, 64)) - 1.0) * 24.0 / 63.0 - 12.0
}

/// The presets Winamp comes with, in its order.
pub fn builtin() -> Vec<Preset> {
    BUILTIN
        .iter()
        .map(|(name, v)| Preset {
            name: (*name).to_string(),
            preamp_db: db_from_value(v[10]),
            band_gains_db: std::array::from_fn(|i| db_from_value(v[i])),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scale_ends_are_exact() {
        assert_eq!(db_from_byte(0), 12.0);
        assert_eq!(db_from_byte(63), -12.0);
        assert_eq!(byte_from_db(12.0), 0);
        assert_eq!(byte_from_db(-12.0), 63);
    }

    #[test]
    fn round_trips_within_a_step() {
        let preset = Preset {
            name: "Rock-ish".into(),
            preamp_db: -3.0,
            band_gains_db: [6.0, 4.5, 0.0, -2.0, -4.0, 0.0, 3.0, 6.0, 8.0, 12.0],
        };
        let read = parse(&write(std::slice::from_ref(&preset))).unwrap();
        assert_eq!(read.len(), 1);
        assert_eq!(read[0].name, "Rock-ish");
        // The file stores 64 steps across 24 dB: within half a step.
        let step = 24.0 / 63.0;
        for (a, b) in read[0].band_gains_db.iter().zip(preset.band_gains_db) {
            assert!((a - b).abs() <= step / 2.0 + 1e-4, "{a} vs {b}");
        }
        assert!((read[0].preamp_db - preset.preamp_db).abs() <= step / 2.0 + 1e-4);
    }

    #[test]
    fn libraries_hold_several_presets() {
        let flat = |name: &str| Preset {
            name: name.into(),
            preamp_db: 0.0,
            band_gains_db: [0.0; 10],
        };
        let bytes = write(&[flat("A"), flat("B")]);
        assert_eq!(bytes.len(), 27 + 4 + 2 * 268);
        let names: Vec<_> = parse(&bytes).unwrap().into_iter().map(|p| p.name).collect();
        assert_eq!(names, ["A", "B"]);
    }

    #[test]
    fn other_files_are_rejected() {
        assert_eq!(parse(b"RIFF...."), Err(EqfError::NotEqf));
    }

    #[test]
    fn builtin_presets_span_the_range() {
        let presets = builtin();
        assert_eq!(presets.len(), 17);
        let bass = presets.iter().find(|p| p.name == "Full Bass").unwrap();
        assert!(bass.band_gains_db[0] > 5.0, "{:?}", bass.band_gains_db);
        assert!(bass.band_gains_db[9] < -6.0, "{:?}", bass.band_gains_db);
        let classical = &presets[0];
        assert!(classical.preamp_db.abs() < 0.5, "33 is about 0 dB");
    }
}
