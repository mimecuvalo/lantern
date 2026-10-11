//! Cue sheets: one audio file (or several), split into tracks by timecode.
//!
//! Ported from the Swift implementation's `CueSheetParser`, with the same
//! strictness: a TRACK without `INDEX 01`, a TRACK before any FILE, or a bad
//! timecode is an error with a line number, because a cue sheet that parses
//! "leniently" into wrong track boundaries is worse than one that refuses.

use std::path::{Path, PathBuf};

use crate::{Entry, Segment, text};

#[derive(Debug, Clone, PartialEq)]
pub struct CueSheet {
    pub title: Option<String>,
    pub performer: Option<String>,
    pub genre: Option<String>,
    pub date: Option<String>,
    pub files: Vec<CueFile>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CueFile {
    /// As written in the sheet; see [`CueSheet::entries`] for resolution.
    pub path: String,
    pub tracks: Vec<CueTrack>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CueTrack {
    pub number: u32,
    pub title: Option<String>,
    pub performer: Option<String>,
    /// `INDEX 01`, in CD frames (1/75 s).
    pub start_frames: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CueError {
    #[error("line {line}: {reason}")]
    Malformed { line: usize, reason: String },
    #[error("the cue sheet lists no tracks")]
    NoTracks,
}

/// CD frames per second, the unit of cue timecodes.
const FRAMES_PER_SEC: f64 = 75.0;

pub fn parse(bytes: &[u8]) -> Result<CueSheet, CueError> {
    let (text, _) = text::decode(bytes);
    parse_text(&text)
}

pub fn parse_text(text: &str) -> Result<CueSheet, CueError> {
    let mut sheet = CueSheet {
        title: None,
        performer: None,
        genre: None,
        date: None,
        files: Vec::new(),
    };
    let malformed = |line: usize, reason: String| CueError::Malformed { line, reason };

    // (track, the line it began on) — so a missing INDEX is reported where
    // the track is, not where the next one starts.
    let mut open_track: Option<(CueTrack, usize, bool)> = None;

    let close_track = |sheet: &mut CueSheet,
                       open: &mut Option<(CueTrack, usize, bool)>|
     -> Result<(), CueError> {
        if let Some((track, line, has_index)) = open.take() {
            if !has_index {
                return Err(malformed(
                    line,
                    format!("TRACK {} missing INDEX 01", track.number),
                ));
            }
            if let Some(file) = sheet.files.last_mut() {
                file.tracks.push(track);
            }
        }
        Ok(())
    };

    // Lines are numbered as an editor would: CRLF is one break.
    for (idx, raw) in text.replace("\r\n", "\n").split(['\n', '\r']).enumerate() {
        let line_no = idx + 1;
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        let (keyword, rest) = split_keyword(line);
        match keyword.to_ascii_uppercase().as_str() {
            "TITLE" => match &mut open_track {
                Some((t, ..)) => t.title = Some(unquote(rest)),
                None => sheet.title = Some(unquote(rest)),
            },
            "PERFORMER" => match &mut open_track {
                Some((t, ..)) => t.performer = Some(unquote(rest)),
                None => sheet.performer = Some(unquote(rest)),
            },
            "REM" => {
                let (key, value) = split_keyword(rest);
                match key.to_ascii_uppercase().as_str() {
                    "GENRE" => sheet.genre = Some(unquote(value)),
                    "DATE" => sheet.date = Some(unquote(value)),
                    _ => {}
                }
            }
            "FILE" => {
                close_track(&mut sheet, &mut open_track)?;
                sheet.files.push(CueFile {
                    path: file_path(rest),
                    tracks: Vec::new(),
                });
            }
            "TRACK" => {
                if sheet.files.is_empty() {
                    return Err(malformed(line_no, "TRACK before FILE".into()));
                }
                close_track(&mut sheet, &mut open_track)?;
                let (number, _) = split_keyword(rest);
                let number = number
                    .parse()
                    .map_err(|_| malformed(line_no, format!("invalid TRACK number '{number}'")))?;
                let track = CueTrack {
                    number,
                    title: None,
                    performer: None,
                    start_frames: 0,
                };
                open_track = Some((track, line_no, false));
            }
            "INDEX" => {
                let Some((track, _, has_index)) = &mut open_track else {
                    return Err(malformed(line_no, "INDEX outside TRACK".into()));
                };
                let (index, timecode) = split_keyword(rest);
                let index: u32 = index
                    .parse()
                    .map_err(|_| malformed(line_no, format!("invalid INDEX number '{index}'")))?;
                // INDEX 00 (pregap) and higher indices are deliberately
                // ignored: a track starts where its audio starts.
                if index == 1 {
                    track.start_frames = parse_timecode(timecode).ok_or_else(|| {
                        malformed(line_no, format!("invalid timecode '{timecode}'"))
                    })?;
                    *has_index = true;
                }
            }
            _ => {}
        }
    }
    close_track(&mut sheet, &mut open_track)?;

    if sheet.files.iter().all(|f| f.tracks.is_empty()) {
        return Err(CueError::NoTracks);
    }
    Ok(sheet)
}

impl CueSheet {
    /// One playlist entry per track. Each ends where the next in the same
    /// file starts; a file's last track plays to its end.
    ///
    /// FILE paths resolve against `dir`. Cues ripped elsewhere often carry
    /// absolute or Windows paths while the audio sits beside the sheet, so
    /// when the written path does not exist its bare filename is tried —
    /// Winamp and foobar2000 do the same. `exists` is injected for testing.
    pub fn entries(&self, dir: &Path, exists: impl Fn(&Path) -> bool) -> Vec<Entry> {
        let mut entries = Vec::new();
        for file in &self.files {
            let path = resolve(&file.path, dir, &exists);
            for (i, track) in file.tracks.iter().enumerate() {
                let start = track.start_frames as f64 / FRAMES_PER_SEC;
                let end = file
                    .tracks
                    .get(i + 1)
                    .map(|next| next.start_frames as f64 / FRAMES_PER_SEC);
                let performer = track.performer.as_ref().or(self.performer.as_ref());
                let title = track
                    .title
                    .clone()
                    .unwrap_or_else(|| format!("Track {:02}", track.number));
                let mut entry = Entry::new(path.clone());
                entry.title = Some(match performer {
                    Some(p) => format!("{p} - {title}"),
                    None => title,
                });
                entry.duration = end.map(|e| (e - start).max(0.0));
                entry.segment = Some(Segment { start, end });
                entries.push(entry);
            }
        }
        entries
    }
}

fn resolve(written: &str, dir: &Path, exists: &impl Fn(&Path) -> bool) -> PathBuf {
    let direct = dir.join(written.replace('\\', std::path::MAIN_SEPARATOR_STR));
    if exists(&direct) {
        return direct;
    }
    let basename = written.rsplit(['/', '\\']).next().unwrap_or(written);
    let beside = dir.join(basename);
    if exists(&beside) { beside } else { direct }
}

/// `"name with spaces.flac" WAVE` or `name.wav WAVE` → the name.
fn file_path(rest: &str) -> String {
    let rest = rest.trim();
    if let Some(quoted) = rest.strip_prefix('"') {
        return quoted.split('"').next().unwrap_or(quoted).to_string();
    }
    split_keyword(rest).0.to_string()
}

/// `MM:SS:FF` to CD frames. Minutes are bounded far past any real disc so
/// hostile input cannot overflow.
fn parse_timecode(s: &str) -> Option<u64> {
    let mut parts = s.trim().split(':');
    let (m, sec, f) = (parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some() {
        return None;
    }
    let (m, sec, f): (u64, u64, u64) = (m.parse().ok()?, sec.parse().ok()?, f.parse().ok()?);
    (m < 1_000_000 && sec < 60 && f < 75).then_some((m * 60 + sec) * 75 + f)
}

fn split_keyword(s: &str) -> (&str, &str) {
    match s.find([' ', '\t']) {
        Some(i) => (&s[..i], s[i + 1..].trim()),
        None => (s, ""),
    }
}

fn unquote(s: &str) -> String {
    let t = s.trim();
    t.strip_prefix('"')
        .and_then(|t| t.strip_suffix('"'))
        .unwrap_or(t)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> Vec<u8> {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/cue")
            .join(name);
        std::fs::read(path).unwrap()
    }

    #[test]
    fn basic_sheet() {
        let sheet = parse(&fixture("basic.cue")).unwrap();
        assert_eq!(sheet.title.as_deref(), Some("DJ Mix vol. 3"));
        assert_eq!(sheet.genre.as_deref(), Some("Electronic"));
        assert_eq!(sheet.date.as_deref(), Some("2003"));
        let tracks = &sheet.files[0].tracks;
        assert_eq!(tracks.len(), 2);
        // INDEX 00 is the pregap; the track starts at INDEX 01.
        assert_eq!(tracks[1].start_frames, (5 * 60 + 32) * 75 + 40);
        assert_eq!(tracks[1].performer.as_deref(), Some("DJ Y"));
    }

    #[test]
    fn legacy_encodings() {
        let sjis = parse(&fixture("shift-jis.cue")).unwrap();
        assert_eq!(sjis.performer.as_deref(), Some("山田太郎"));
        assert_eq!(sjis.files[0].tracks[0].title.as_deref(), Some("序章"));

        let cp1251 = parse(&fixture("cp1251.cue")).unwrap();
        assert_eq!(cp1251.title.as_deref(), Some("Микс №3"));

        let cp1252 = parse(&fixture("cp1252.cue")).unwrap();
        assert_eq!(cp1252.performer.as_deref(), Some("Café Crème"));
    }

    #[test]
    fn malformed_and_empty_sheets_are_errors() {
        assert_eq!(
            parse(&fixture("malformed.cue")),
            Err(CueError::Malformed {
                line: 1,
                reason: "TRACK before FILE".into()
            })
        );
        assert_eq!(parse(&fixture("zero-tracks.cue")), Err(CueError::NoTracks));
    }

    #[test]
    fn missing_index_is_reported_at_its_track() {
        let err = parse_text(
            "FILE a.wav WAVE\nTRACK 01 AUDIO\nTITLE x\nTRACK 02 AUDIO\nINDEX 01 00:00:00\n",
        );
        assert_eq!(
            err,
            Err(CueError::Malformed {
                line: 2,
                reason: "TRACK 1 missing INDEX 01".into()
            })
        );
    }

    #[test]
    fn bad_timecodes_are_rejected() {
        for tc in ["00:60:00", "00:00:75", "1:2", "a:b:c", "1:2:3:4"] {
            assert_eq!(parse_timecode(tc), None, "{tc}");
        }
        assert_eq!(parse_timecode("01:02:03"), Some((62) * 75 + 3));
    }

    #[test]
    fn entries_split_each_file_into_segments() {
        let sheet = parse(&fixture("multi-file.cue")).unwrap();
        let entries = sheet.entries(Path::new("/rips"), |_| true);
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].path, Path::new("/rips/side-a.flac"));
        assert_eq!(
            entries[0].segment,
            Some(Segment {
                start: 0.0,
                end: Some(195.0)
            })
        );
        assert_eq!(entries[0].duration, Some(195.0));
        assert_eq!(
            entries[1].segment.unwrap().end,
            None,
            "last in file plays out"
        );
        assert_eq!(entries[2].path, Path::new("/rips/side-b.flac"));
        assert_eq!(entries[2].title.as_deref(), Some("B1"));
    }

    #[test]
    fn track_title_carries_performer() {
        let sheet = parse(&fixture("basic.cue")).unwrap();
        let entries = sheet.entries(Path::new("/x"), |_| true);
        assert_eq!(entries[0].title.as_deref(), Some("DJ X - Opening"));
    }

    #[test]
    fn foreign_paths_fall_back_to_the_file_beside_the_sheet() {
        let sheet =
            parse_text("FILE \"C:\\Rips\\album.flac\" WAVE\nTRACK 01 AUDIO\nINDEX 01 00:00:00\n")
                .unwrap();
        let beside = Path::new("/music/album.flac");
        let entries = sheet.entries(Path::new("/music"), |p| p == beside);
        assert_eq!(entries[0].path, beside);
    }
}
