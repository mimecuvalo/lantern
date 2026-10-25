//! What a file says about itself, for the library: the full set of tags
//! the views and columns use, where the playlist needs only a title.
//!
//! Reading never fails outright. A file lofty cannot read, or one without
//! tags, still joins the library under its file name. [`write`] puts edited
//! tags back, for File Info; [`details`] is what File Info shows of the
//! file itself.

use std::path::Path;

use lofty::prelude::*;
use lofty::tag::ItemKey;

use crate::TrackInfo;

/// Size and modification time, which tell a rescan whether to look again.
pub fn fingerprint(meta: &std::fs::Metadata) -> (u64, i64) {
    let modified = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs() as i64);
    (meta.len(), modified)
}

fn clean(s: Option<impl AsRef<str>>) -> String {
    s.map(|s| s.as_ref().trim().to_string()).unwrap_or_default()
}

pub fn read(path: &Path) -> TrackInfo {
    let (size, modified) = std::fs::metadata(path)
        .map(|m| fingerprint(&m))
        .unwrap_or_default();
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut info = TrackInfo {
        path: path.to_path_buf(),
        title: stem.clone(),
        size,
        modified,
        ..TrackInfo::default()
    };
    let Ok(file) = lofty::read_from_path(path) else {
        return info;
    };
    let props = file.properties();
    info.duration_secs = Some(props.duration().as_secs_f64()).filter(|&d| d > 0.0);
    info.kbps = props.audio_bitrate().filter(|&k| k > 0);
    if let Some(tag) = file.primary_tag().or_else(|| file.first_tag()) {
        let title = clean(tag.title());
        if !title.is_empty() {
            info.title = title;
        }
        info.artist = clean(tag.artist());
        info.album = clean(tag.album());
        info.album_artist = clean(tag.get_string(ItemKey::AlbumArtist));
        info.genre = clean(tag.genre());
        info.year = tag.date().map(|d| u32::from(d.year)).filter(|&y| y > 0);
        info.track_no = tag.track().filter(|&n| n > 0);
        info.disc_no = tag.disk().filter(|&n| n > 0);
    }
    info
}

/// What File Info shows of a file beyond its tags.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Details {
    /// "MP3", "FLAC"…
    pub format: String,
    pub duration_secs: Option<f64>,
    pub kbps: Option<u32>,
    pub sample_rate: Option<u32>,
    pub channels: Option<u8>,
    pub size: u64,
}

pub fn details(path: &Path) -> Details {
    let size = std::fs::metadata(path).map_or(0, |m| m.len());
    let Ok(file) = lofty::read_from_path(path) else {
        return Details {
            size,
            ..Details::default()
        };
    };
    let props = file.properties();
    use lofty::file::FileType as F;
    let format = match file.file_type() {
        F::Aac => "AAC",
        F::Aiff => "AIFF",
        F::Ape => "Monkey's Audio",
        F::Flac => "FLAC",
        F::Mpeg => "MP3",
        F::Mp4 => "MPEG-4 audio",
        F::Mpc => "Musepack",
        F::Opus => "Opus",
        F::Vorbis => "Ogg Vorbis",
        F::Speex => "Speex",
        F::Wav => "WAV",
        F::WavPack => "WavPack",
        _ => "Audio",
    };
    Details {
        format: format.to_string(),
        duration_secs: Some(props.duration().as_secs_f64()).filter(|&d| d > 0.0),
        kbps: props.audio_bitrate().filter(|&k| k > 0),
        sample_rate: props.sample_rate(),
        channels: props.channels(),
        size,
    }
}

/// Write `info`'s tags into the file at `path`: title, artist, album,
/// album artist, genre, year, track and disc. An empty one is removed. The
/// file keeps any other tags it has.
pub fn write(path: &Path, info: &TrackInfo) -> Result<(), String> {
    use lofty::config::WriteOptions;
    use lofty::tag::{Accessor, Tag};
    let mut file = lofty::read_from_path(path).map_err(|e| e.to_string())?;
    if file.primary_tag().is_none() {
        file.insert_tag(Tag::new(file.primary_tag_type()));
    }
    let tag = file
        .primary_tag_mut()
        .ok_or_else(|| "this kind of file cannot hold tags".to_string())?;
    let text = |tag: &mut Tag, key: ItemKey, value: &str| {
        if value.trim().is_empty() {
            tag.remove_key(key);
        } else {
            tag.insert_text(key, value.trim().to_string());
        }
    };
    text(tag, ItemKey::TrackTitle, &info.title);
    text(tag, ItemKey::TrackArtist, &info.artist);
    text(tag, ItemKey::AlbumTitle, &info.album);
    text(tag, ItemKey::AlbumArtist, &info.album_artist);
    text(tag, ItemKey::Genre, &info.genre);
    match info.year {
        Some(year) => tag.set_date(lofty::tag::items::Timestamp {
            year: u16::try_from(year).map_err(|_| "the year is too large".to_string())?,
            ..Default::default()
        }),
        None => tag.remove_date(),
    }
    match info.track_no {
        Some(n) => tag.set_track(n),
        None => tag.remove_track(),
    }
    match info.disc_no {
        Some(n) => tag.set_disk(n),
        None => tag.remove_disk(),
    }
    tag.save_to_path(path, WriteOptions::default())
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unreadable_file_joins_under_its_name() {
        let dir = std::env::temp_dir().join(format!("lantern-tags-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("03 Not Really Audio.mp3");
        std::fs::write(&file, b"nothing to see").unwrap();
        let info = read(&file);
        assert_eq!(info.title, "03 Not Really Audio");
        assert_eq!(info.size, 14);
        assert!(info.modified > 0);
        assert_eq!(info.artist, "");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn written_tags_read_back() {
        let sample = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../sample-music/04 - The Hot Rock/sp1106-04_burn_dont_freeze.mp3");
        if !sample.exists() {
            eprintln!("SKIPPED: {} not present", sample.display());
            return;
        }
        let dir = std::env::temp_dir().join(format!("lantern-write-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let copy = dir.join("burn.mp3");
        std::fs::copy(&sample, &copy).unwrap();

        let mut info = read(&copy);
        info.title = "Burn, Don't Freeze!".into();
        info.album_artist = String::new();
        info.genre = "Riot Grrrl".into();
        info.year = Some(1999);
        info.track_no = Some(4);
        info.disc_no = Some(1);
        write(&copy, &info).unwrap();

        let back = read(&copy);
        assert_eq!(back.title, "Burn, Don't Freeze!");
        assert_eq!(back.artist, info.artist, "untouched fields stay");
        assert_eq!(back.album_artist, "", "an emptied field goes");
        assert_eq!(back.genre, "Riot Grrrl");
        assert_eq!(
            (back.year, back.track_no, back.disc_no),
            (Some(1999), Some(4), Some(1))
        );
        let d = details(&copy);
        assert_eq!(d.format, "MP3");
        assert!(d.sample_rate.is_some() && d.kbps.is_some());
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Against the local sample album, which is not in the repository:
    /// skipped where it is absent, as in CI.
    #[test]
    fn reads_real_tags() {
        let file = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../sample-music/04 - The Hot Rock/sp1106-01_start_together.mp3");
        if !file.exists() {
            eprintln!("SKIPPED: {} not present", file.display());
            return;
        }
        let info = read(&file);
        assert!(
            !info.artist.is_empty() && !info.album.is_empty(),
            "{info:?}"
        );
        assert_ne!(info.title, "sp1106-01_start_together");
        assert!(info.duration_secs.is_some_and(|d| d > 60.0));
        assert_eq!(info.track_no, Some(1));
    }
}
