//! Display metadata: titles, durations, bitrates.
//!
//! Symphonia decodes; lofty is better at tags and knows each format's
//! bitrate. Reading tags never fails outright — a file without them just
//! shows fewer facts — and is lenient, so one malformed frame does not
//! cost a file its other tags.

use std::path::Path;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Tags {
    /// "Artist - Title", or just the title.
    pub title: Option<String>,
    pub kbps: Option<u32>,
    pub duration_secs: Option<f64>,
}

/// A file's name without its extension, composed to NFC so a decomposed
/// "š" (as macOS file names often have it) draws as one letter.
pub fn stem_title(path: &Path) -> String {
    use unicode_normalization::UnicodeNormalization;
    path.file_stem()
        .map(|s| s.to_string_lossy().nfc().collect())
        .unwrap_or_default()
}

pub fn read(path: &Path) -> Tags {
    use lofty::config::{ParseOptions, ParsingMode};
    use lofty::prelude::*;

    let Ok(file) = lofty::probe::Probe::open(path).and_then(|p| {
        p.options(
            ParseOptions::new()
                .parsing_mode(ParsingMode::Relaxed)
                .read_cover_art(false),
        )
            .read()
    }) else {
        return Tags::default();
    };
    let props = file.properties();
    let title = file
        .primary_tag()
        .or_else(|| file.first_tag())
        .and_then(|tag| {
            let title = tag
                .title()
                .map(|t| t.trim().to_string())
                .filter(|t| !t.is_empty())?;
            Some(match tag.artist().map(|a| a.trim().to_string()) {
                Some(artist) if !artist.is_empty() => format!("{artist} - {title}"),
                _ => title,
            })
        });
    Tags {
        title,
        kbps: props.audio_bitrate().filter(|&k| k > 0),
        duration_secs: Some(props.duration().as_secs_f64()).filter(|&d| d > 0.0),
    }
}
