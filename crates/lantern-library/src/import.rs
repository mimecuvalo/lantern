//! Bringing in an Apple Music (or iTunes) library: its tracks, their play
//! history and ratings, and its playlists.
//!
//! Two ways in arrive at the same [`Imported`]: the library XML file that
//! Music writes when "Share Library XML with other applications" is on
//! (parsed here), and Apple's iTunesLibrary framework, which the app reads
//! on macOS. Only tracks whose files are on this computer come in; streamed
//! and cloud-only tracks cannot be played from here.

use std::path::{Path, PathBuf};

use plist::{Dictionary, Value};

use crate::{Library, LibraryError, Result, TrackInfo, tags};

/// Playlists brought in are filed under this source, and replaced as a set
/// on the next import.
pub const SOURCE: &str = "apple-music";

/// A track as another library knows it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ImportedTrack {
    pub info: TrackInfo,
    pub added: Option<i64>,
    pub play_count: u32,
    pub last_played: Option<i64>,
    /// 0 to 5 stars, from Music's 0 to 100. A rating Music guessed from the
    /// album's is not taken.
    pub rating: u8,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Imported {
    pub tracks: Vec<ImportedTrack>,
    /// Each a name and its tracks' files, in order.
    pub playlists: Vec<(String, Vec<PathBuf>)>,
    /// Tracks passed over: streamed, cloud-only, video, or with no file
    /// here.
    pub skipped: usize,
}

/// What an import changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Summary {
    pub tracks: usize,
    pub playlists: usize,
    pub skipped: usize,
}

/// Music's 0-100 rating as stars.
pub fn stars(rating: i64) -> u8 {
    ((rating.clamp(0, 100) + 10) / 20) as u8
}

/// A `file://` URL as a path, or `None` for any other kind of URL.
pub fn file_url_to_path(url: &str) -> Option<PathBuf> {
    let rest = url
        .strip_prefix("file://localhost")
        .or_else(|| url.strip_prefix("file://"))?;
    let bytes = rest.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = |b: u8| (b as char).to_digit(16);
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let (Some(h), Some(l)) = (hex(bytes[i + 1]), hex(bytes[i + 2]))
        {
            out.push((h * 16 + l) as u8);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    Some(PathBuf::from(String::from_utf8(out).ok()?))
}

fn date_secs(v: Option<&Value>) -> Option<i64> {
    let t: std::time::SystemTime = v?.as_date()?.into();
    Some(t.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs() as i64)
}

fn int(d: &Dictionary, key: &str) -> Option<i64> {
    d.get(key)?.as_signed_integer()
}

fn string(d: &Dictionary, key: &str) -> String {
    d.get(key)
        .and_then(Value::as_string)
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

fn flag(d: &Dictionary, key: &str) -> bool {
    d.get(key).and_then(Value::as_boolean).unwrap_or(false)
}

fn positive(v: Option<i64>) -> Option<u32> {
    v.filter(|&n| n > 0).and_then(|n| u32::try_from(n).ok())
}

/// One track from the XML, if it is a local audio file that exists.
fn xml_track(d: &Dictionary) -> Option<ImportedTrack> {
    let video = ["Has Video", "Movie", "TV Show", "Music Video", "Podcast"]
        .iter()
        .any(|k| flag(d, k));
    if video || d.get("Track Type").and_then(Value::as_string) != Some("File") {
        return None;
    }
    let path = file_url_to_path(d.get("Location")?.as_string()?)?;
    let meta = std::fs::metadata(&path).ok()?;
    let (size, modified) = tags::fingerprint(&meta);
    let mut title = string(d, "Name");
    if title.is_empty() {
        title = path.file_stem()?.to_string_lossy().into_owned();
    }
    let rating = if flag(d, "Rating Computed") {
        0
    } else {
        int(d, "Rating").map_or(0, stars)
    };
    Some(ImportedTrack {
        info: TrackInfo {
            title,
            artist: string(d, "Artist"),
            album: string(d, "Album"),
            album_artist: string(d, "Album Artist"),
            genre: string(d, "Genre"),
            year: positive(int(d, "Year")),
            track_no: positive(int(d, "Track Number")),
            disc_no: positive(int(d, "Disc Number")),
            duration_secs: int(d, "Total Time")
                .filter(|&ms| ms > 0)
                .map(|ms| ms as f64 / 1000.0),
            kbps: positive(int(d, "Bit Rate")),
            size,
            modified,
            path,
        },
        added: date_secs(d.get("Date Added")),
        play_count: positive(int(d, "Play Count")).unwrap_or(0),
        last_played: date_secs(d.get("Play Date UTC")),
        rating,
    })
}

/// Read Music's library XML file.
pub fn read_xml(file: &Path) -> Result<Imported> {
    let root = Value::from_file(file)
        .map_err(|e| LibraryError::NotALibrary(format!("{}: {e}", file.display())))?;
    let root = root
        .as_dictionary()
        .ok_or_else(|| LibraryError::NotALibrary(file.display().to_string()))?;
    let tracks = root
        .get("Tracks")
        .and_then(Value::as_dictionary)
        .ok_or_else(|| LibraryError::NotALibrary(format!("{}: no tracks", file.display())))?;

    let mut out = Imported::default();
    let mut by_id = std::collections::HashMap::new();
    for value in tracks.values() {
        let Some(d) = value.as_dictionary() else {
            continue;
        };
        match xml_track(d) {
            Some(t) => {
                if let Some(id) = int(d, "Track ID") {
                    by_id.insert(id, t.info.path.clone());
                }
                out.tracks.push(t);
            }
            None => out.skipped += 1,
        }
    }

    let lists = root.get("Playlists").and_then(Value::as_array);
    for list in lists.into_iter().flatten().filter_map(Value::as_dictionary) {
        // The whole library, Music's own kinds (Music, Podcasts,
        // Purchased...), folders and hidden lists are not playlists anyone
        // made.
        let built_in = flag(list, "Master")
            || list.contains_key("Distinguished Kind")
            || flag(list, "Folder")
            || list.get("Visible").and_then(Value::as_boolean) == Some(false);
        if built_in {
            continue;
        }
        let items = list.get("Playlist Items").and_then(Value::as_array);
        let paths: Vec<PathBuf> = items
            .into_iter()
            .flatten()
            .filter_map(|item| by_id.get(&int(item.as_dictionary()?, "Track ID")?).cloned())
            .collect();
        out.playlists.push((string(list, "Name"), paths));
    }
    Ok(out)
}

impl Library {
    /// Bring `imported` in: its tracks join or update the library with
    /// their history merged, and its playlists replace any from the last
    /// import.
    pub fn import(&mut self, imported: &Imported, now: i64) -> Result<Summary> {
        let tx = self.transaction()?;
        for t in &imported.tracks {
            Self::upsert_in(&tx, &t.info, t.added.unwrap_or(now))?;
            Self::merge_history(
                &tx,
                &t.info.path,
                t.play_count,
                t.last_played,
                t.rating,
                t.added,
            )?;
        }
        Self::replace_playlists(&tx, SOURCE, &imported.playlists)?;
        tx.commit()?;
        Ok(Summary {
            tracks: imported.tracks.len(),
            playlists: imported.playlists.len(),
            skipped: imported.skipped,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stars_round_from_hundredths() {
        assert_eq!(
            [0, 20, 40, 60, 80, 100, 50].map(stars),
            [0, 1, 2, 3, 4, 5, 3]
        );
    }

    #[test]
    fn file_urls_decode() {
        assert_eq!(
            file_url_to_path("file:///Users/me/Music/Sleater-Kinney/01%20Start%20Together.mp3"),
            Some(PathBuf::from(
                "/Users/me/Music/Sleater-Kinney/01 Start Together.mp3"
            ))
        );
        assert_eq!(
            file_url_to_path("file://localhost/C:/Music/Bj%C3%B6rk.mp3"),
            Some(PathBuf::from("/C:/Music/Björk.mp3"))
        );
        assert_eq!(file_url_to_path("https://example.com/x.mp3"), None);
        assert_eq!(
            file_url_to_path("file:///odd%2"),
            Some(PathBuf::from("/odd%2"))
        );
    }

    /// A library XML in Music's shape, around files made for the test.
    fn write_library(dir: &Path) -> PathBuf {
        let song = dir.join("Start Together.mp3");
        let other = dir.join("Hot Rock.mp3");
        std::fs::write(&song, "x").unwrap();
        std::fs::write(&other, "xy").unwrap();
        let url = |p: &Path| format!("file://{}", p.display()).replace(' ', "%20");
        let xml = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>Tracks</key><dict>
  <key>1</key><dict>
    <key>Track ID</key><integer>1</integer>
    <key>Name</key><string>Start Together</string>
    <key>Artist</key><string>Sleater-Kinney</string>
    <key>Album</key><string>The Hot Rock</string>
    <key>Track Number</key><integer>1</integer>
    <key>Total Time</key><integer>159000</integer>
    <key>Date Added</key><date>2004-05-06T07:08:09Z</date>
    <key>Play Count</key><integer>12</integer>
    <key>Play Date UTC</key><date>2026-01-02T03:04:05Z</date>
    <key>Rating</key><integer>80</integer>
    <key>Track Type</key><string>File</string>
    <key>Location</key><string>{song}</string>
  </dict>
  <key>2</key><dict>
    <key>Track ID</key><integer>2</integer>
    <key>Name</key><string>Hot Rock</string>
    <key>Rating</key><integer>60</integer>
    <key>Rating Computed</key><true/>
    <key>Track Type</key><string>File</string>
    <key>Location</key><string>{other}</string>
  </dict>
  <key>3</key><dict>
    <key>Track ID</key><integer>3</integer>
    <key>Name</key><string>In the cloud</string>
    <key>Track Type</key><string>Remote</string>
  </dict>
  <key>4</key><dict>
    <key>Track ID</key><integer>4</integer>
    <key>Name</key><string>Deleted</string>
    <key>Track Type</key><string>File</string>
    <key>Location</key><string>file:///nowhere/gone.mp3</string>
  </dict>
</dict>
<key>Playlists</key><array>
  <dict><key>Name</key><string>Library</string><key>Master</key><true/>
    <key>Playlist Items</key><array><dict><key>Track ID</key><integer>1</integer></dict></array></dict>
  <dict><key>Name</key><string>Music</string><key>Distinguished Kind</key><integer>4</integer></dict>
  <dict><key>Name</key><string>Riot</string>
    <key>Playlist Items</key><array>
      <dict><key>Track ID</key><integer>2</integer></dict>
      <dict><key>Track ID</key><integer>3</integer></dict>
      <dict><key>Track ID</key><integer>1</integer></dict>
    </array></dict>
</array>
</dict></plist>"#,
            song = url(&song),
            other = url(&other),
        );
        let file = dir.join("Library.xml");
        std::fs::write(&file, xml).unwrap();
        file
    }

    #[test]
    fn reads_tracks_history_and_playlists_from_xml() {
        let dir = std::env::temp_dir().join(format!("lantern-import-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let imported = read_xml(&write_library(&dir)).unwrap();

        assert_eq!(imported.tracks.len(), 2);
        assert_eq!(imported.skipped, 2, "the cloud track and the missing file");
        let start = imported
            .tracks
            .iter()
            .find(|t| t.info.title == "Start Together")
            .unwrap();
        assert_eq!(start.info.artist, "Sleater-Kinney");
        assert_eq!(start.info.duration_secs, Some(159.0));
        assert_eq!(start.info.size, 1);
        assert_eq!((start.play_count, start.rating), (12, 4));
        assert_eq!(start.added, Some(1_083_827_289));
        let hot = imported
            .tracks
            .iter()
            .find(|t| t.info.title == "Hot Rock")
            .unwrap();
        assert_eq!(hot.rating, 0, "a computed rating is not taken");

        assert_eq!(imported.playlists.len(), 1);
        assert_eq!(imported.playlists[0].0, "Riot");
        assert_eq!(
            imported.playlists[0].1,
            [dir.join("Hot Rock.mp3"), dir.join("Start Together.mp3")]
        );

        // Into a library that already played one of them once.
        let mut lib = Library::open_in_memory().unwrap();
        lib.upsert(std::slice::from_ref(&start.info), 2_000_000_000)
            .unwrap();
        lib.record_play(&start.info.path, 1_000).unwrap();
        let summary = lib.import(&imported, 2_000_000_000).unwrap();
        assert_eq!(summary.tracks, 2);
        let tracks = lib.tracks().unwrap();
        let t = tracks
            .iter()
            .find(|t| t.info.title == "Start Together")
            .unwrap();
        assert_eq!(t.play_count, 12, "the larger count");
        assert_eq!(t.last_played, Some(1_767_323_045), "the later play");
        assert_eq!(t.added, 1_083_827_289, "the earlier date");
        assert_eq!(t.rating, 4);
        let lists = lib.playlists().unwrap();
        assert_eq!(lists.len(), 1);
        assert_eq!(lists[0].tracks.len(), 2);

        // Again: playlists are replaced, not doubled.
        lib.import(&imported, 2_000_000_000).unwrap();
        assert_eq!(lib.playlists().unwrap().len(), 1);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_non_library_file_says_so() {
        let dir = std::env::temp_dir().join(format!("lantern-import-bad-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("not.xml");
        std::fs::write(&file, "hello").unwrap();
        assert!(matches!(read_xml(&file), Err(LibraryError::NotALibrary(_))));
        let _ = std::fs::remove_dir_all(dir);
    }
}
