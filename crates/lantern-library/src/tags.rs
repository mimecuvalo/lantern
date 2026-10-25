//! What a file says about itself, for the library: the full set of tags
//! the views and columns use, where the playlist needs only a title.
//!
//! Reading never fails outright. A file lofty cannot read, or one without
//! tags, still joins the library under its file name.

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
