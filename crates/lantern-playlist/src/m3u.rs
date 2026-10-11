//! M3U / M3U8 and PLS playlists.
//!
//! Both are read leniently: unknown directives are ignored, streams (any
//! `scheme://` other than `file://`) are skipped since there is no stream
//! support, and relative paths resolve against the playlist's directory.

use std::path::{Path, PathBuf};

use crate::{Entry, Segment, text};

/// Our own extended-M3U directive, carrying a cue track's stretch of its file
/// as `#LANTERNSEGMENT:<start>,<end>` (end empty for "to the end"). Other
/// players ignore unknown `#` lines, so the file stays a valid playlist
/// everywhere.
const SEGMENT_DIRECTIVE: &str = "#LANTERNSEGMENT:";
/// The same directive under the project's earlier name, still read.
const OLD_SEGMENT_DIRECTIVE: &str = "#WAMPSEGMENT:";

/// Parse an M3U or extended M3U playlist.
///
/// `#EXTINF:<seconds>,<title>` supplies a display title and duration for the
/// path that follows; they are hints until the file's own tags are read.
pub fn parse_m3u(bytes: &[u8], base: &Path) -> Vec<Entry> {
    let (text, _) = text::decode(bytes);
    let mut entries = Vec::new();
    let (mut duration, mut title, mut segment) = (None, None, None);
    for line in text::lines(&text) {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(seg) = line
            .strip_prefix(SEGMENT_DIRECTIVE)
            .or_else(|| line.strip_prefix(OLD_SEGMENT_DIRECTIVE))
        {
            let (start, end) = seg.split_once(',').unwrap_or((seg, ""));
            segment = start.trim().parse::<f64>().ok().map(|start| Segment {
                start,
                end: end.trim().parse::<f64>().ok(),
            });
            continue;
        }
        if let Some(info) = line.strip_prefix("#EXTINF:") {
            let (secs, name) = info.split_once(',').unwrap_or((info, ""));
            duration = secs.trim().parse::<f64>().ok().filter(|d| *d >= 0.0);
            title = Some(name.trim().to_string()).filter(|t| !t.is_empty());
            continue;
        }
        if line.starts_with('#') {
            continue;
        }
        if let Some(path) = resolve(line, base) {
            let mut entry = Entry::new(path);
            entry.title = title.take();
            entry.duration = duration.take();
            entry.segment = segment.take();
            entries.push(entry);
        }
        (duration, title, segment) = (None, None, None);
    }
    entries
}

/// Write `entries` as extended M3U, UTF-8, with absolute paths.
///
/// Titles and durations go in `#EXTINF` lines so the list shows at once when
/// reloaded; cue tracks carry their segment in a `#LANTERNSEGMENT` line.
pub fn write_m3u8(entries: &[Entry]) -> String {
    let mut out = String::from("#EXTM3U\n");
    for e in entries {
        let secs = e.duration.map_or(-1, |d| d.round() as i64);
        out.push_str(&format!("#EXTINF:{secs},{}\n", e.display_title()));
        if let Some(seg) = e.segment {
            let end = seg.end.map(|e| e.to_string()).unwrap_or_default();
            out.push_str(&format!("{SEGMENT_DIRECTIVE}{},{end}\n", seg.start));
        }
        out.push_str(&e.path.to_string_lossy());
        out.push('\n');
    }
    out
}

/// Parse a PLS playlist: `FileN=`, with optional `TitleN=` and `LengthN=`.
/// Entries are ordered by N, not by line order.
pub fn parse_pls(bytes: &[u8], base: &Path) -> Vec<Entry> {
    let (text, _) = text::decode(bytes);
    let mut slots: std::collections::BTreeMap<u32, Entry> = Default::default();
    let mut titles = std::collections::HashMap::new();
    let mut lengths = std::collections::HashMap::new();
    for line in text::lines(&text) {
        let Some((key, value)) = line.trim().split_once('=') else {
            continue;
        };
        let key = key.trim().to_ascii_lowercase();
        let value = value.trim();
        let numbered = |prefix: &str| key.strip_prefix(prefix)?.parse::<u32>().ok();
        if let Some(n) = numbered("file") {
            if let Some(path) = resolve(value, base) {
                slots.insert(n, Entry::new(path));
            }
        } else if let Some(n) = numbered("title") {
            titles.insert(n, value.to_string());
        } else if let Some(n) = numbered("length") {
            lengths.insert(n, value.parse::<f64>().ok().filter(|d| *d >= 0.0));
        }
    }
    slots
        .into_iter()
        .map(|(n, mut entry)| {
            entry.title = titles.remove(&n).filter(|t| !t.is_empty());
            entry.duration = lengths.remove(&n).flatten();
            entry
        })
        .collect()
}

/// A playlist line to a local path, or `None` for a stream.
fn resolve(line: &str, base: &Path) -> Option<PathBuf> {
    if let Some(rest) = line.strip_prefix("file://") {
        // Legacy tools write unencoded URLs; decode what is encoded.
        return Some(PathBuf::from(percent_decode(rest)));
    }
    if let Some((scheme, _)) = line.split_once("://")
        && !scheme.is_empty()
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "+.-".contains(c))
    {
        return None;
    }
    // Windows playlists use backslashes; on a Unix-like system a backslash
    // is never a separator a user meant.
    let native = if cfg!(windows) {
        line.to_string()
    } else {
        line.replace('\\', "/")
    };
    let path = PathBuf::from(native);
    Some(if path.is_absolute() {
        path
    } else {
        base.join(path)
    })
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && let Some(hex) = s.get(i + 1..i + 3)
            && let Ok(b) = u8::from_str_radix(hex, 16)
        {
            out.push(b);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extended_m3u_carries_titles_and_durations() {
        let m3u = b"#EXTM3U\r\n#EXTINF:225,Artist - Song\r\nsong.mp3\r\nother.flac\r\n";
        let entries = parse_m3u(m3u, Path::new("/music"));
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].path, Path::new("/music/song.mp3"));
        assert_eq!(entries[0].title.as_deref(), Some("Artist - Song"));
        assert_eq!(entries[0].duration, Some(225.0));
        assert_eq!(entries[1].title, None, "hints apply to one entry only");
    }

    #[test]
    fn streams_are_skipped_and_file_urls_decoded() {
        let m3u = b"http://radio.example/stream\nfile:///Users/me/My%20Song.mp3\n/abs/x.mp3\n";
        let entries = parse_m3u(m3u, Path::new("/music"));
        let paths: Vec<_> = entries.iter().map(|e| e.path.clone()).collect();
        assert_eq!(
            paths,
            [
                PathBuf::from("/Users/me/My Song.mp3"),
                PathBuf::from("/abs/x.mp3")
            ]
        );
    }

    #[test]
    fn negative_extinf_duration_means_unknown() {
        let entries = parse_m3u(b"#EXTINF:-1,Live\nlive.mp3\n", Path::new("/"));
        assert_eq!(entries[0].duration, None);
        assert_eq!(entries[0].title.as_deref(), Some("Live"));
    }

    #[cfg(not(windows))]
    #[test]
    fn windows_separators_are_converted() {
        let entries = parse_m3u(b"Album\\01.mp3\n", Path::new("/music"));
        assert_eq!(entries[0].path, Path::new("/music/Album/01.mp3"));
    }

    #[test]
    fn legacy_encoded_m3u_decodes() {
        // "Café.mp3" in CP-1252.
        let entries = parse_m3u(b"Caf\xe9.mp3\n", Path::new("/m"));
        assert_eq!(entries[0].path, Path::new("/m/Café.mp3"));
    }

    #[test]
    fn written_m3u8_reads_back_including_segments() {
        let mut a = Entry::new(PathBuf::from("/music/a.mp3"));
        a.title = Some("Artist - Song".into());
        a.duration = Some(225.0);
        let mut b = Entry::new(PathBuf::from("/music/album.flac"));
        b.title = Some("Track 2".into());
        b.segment = Some(Segment {
            start: 195.5,
            end: None,
        });
        let mut c = b.clone();
        c.segment = Some(Segment {
            start: 0.0,
            end: Some(195.5),
        });
        let written = write_m3u8(&[a.clone(), b.clone(), c.clone()]);
        let read = parse_m3u(written.as_bytes(), Path::new("/elsewhere"));
        assert_eq!(read.len(), 3);
        assert_eq!(read[0].path, a.path);
        assert_eq!(read[0].title, a.title);
        assert_eq!(read[0].duration, Some(225.0));
        assert_eq!(read[1].segment, b.segment);
        assert_eq!(read[2].segment, c.segment);
        assert_eq!(read[1].duration, None, "unknown stays unknown");
    }

    #[test]
    fn segments_saved_under_the_old_name_still_load() {
        let old = b"#EXTM3U\n#WAMPSEGMENT:12.5,30\n/music/album.flac\n";
        let read = parse_m3u(old, Path::new("/"));
        assert_eq!(
            read[0].segment,
            Some(Segment {
                start: 12.5,
                end: Some(30.0)
            })
        );
    }

    #[test]
    fn pls_orders_by_number() {
        let pls =
            b"[playlist]\nFile2=b.mp3\nTitle2=Bee\nFile1=a.mp3\nLength1=61\nNumberOfEntries=2\n";
        let entries = parse_pls(pls, Path::new("/m"));
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].path, Path::new("/m/a.mp3"));
        assert_eq!(entries[0].duration, Some(61.0));
        assert_eq!(entries[1].title.as_deref(), Some("Bee"));
    }
}
