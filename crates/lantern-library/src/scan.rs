//! Scanning watched folders for music.
//!
//! A scan walks each folder, skips files whose size and modification time
//! are unchanged since last time, reads the tags of the rest, and notices
//! files that have gone. It reports as it goes, so the app can run it on a
//! thread of its own and show the library filling in.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use crate::{TrackInfo, tags};

/// File extensions the player can play.
pub const AUDIO: &[&str] = &[
    "mp3", "mp2", "mp1", "flac", "ogg", "oga", "m4a", "m4b", "mp4", "aac", "wav", "aif", "aiff",
    "caf",
];

pub fn is_audio(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| AUDIO.iter().any(|a| a.eq_ignore_ascii_case(e)))
}

/// What a scan reports, as it goes.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// The walk is done: this many files are new or changed and will be
    /// read.
    Counted(usize),
    /// A new or changed file, read.
    Found(TrackInfo),
    /// Files the library knew, under a watched folder, that are no longer
    /// there.
    Gone(Vec<PathBuf>),
}

/// Every audio file under `folder`, depth first, sorted within each
/// folder. Hidden files and folders are skipped, and symbolic links to
/// folders are not followed, so a link back up the tree cannot loop.
fn walk(folder: &Path, out: &mut Vec<(PathBuf, (u64, i64))>, cancel: &AtomicBool) {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        if cancel.load(Ordering::Relaxed) {
            return;
        }
        if entry.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        let path = entry.path();
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() {
            walk(&path, out, cancel);
        } else if is_audio(&path)
            && let Ok(meta) = std::fs::metadata(&path)
            && meta.is_file()
        {
            out.push((path, tags::fingerprint(&meta)));
        }
    }
}

/// Scan `folders` against what the library already `known`s, reporting
/// to `emit`. Stops early, between files, once `cancel` is set.
///
/// A folder that is missing altogether — an unplugged drive — is skipped
/// rather than treated as emptied, so its tracks are not let go.
pub fn scan(
    folders: &[PathBuf],
    known: &HashMap<PathBuf, (u64, i64)>,
    cancel: &AtomicBool,
    mut emit: impl FnMut(Event),
) {
    let mut seen = Vec::new();
    let mut walked = Vec::new();
    for folder in folders.iter().filter(|f| f.is_dir()) {
        walk(folder, &mut seen, cancel);
        walked.push(folder);
    }
    if cancel.load(Ordering::Relaxed) {
        return;
    }
    let present: HashSet<&Path> = seen.iter().map(|(p, _)| p.as_path()).collect();
    let gone: Vec<PathBuf> = known
        .keys()
        .filter(|p| walked.iter().any(|f| p.starts_with(f)) && !present.contains(p.as_path()))
        .cloned()
        .collect();
    if !gone.is_empty() {
        emit(Event::Gone(gone));
    }
    let changed: Vec<PathBuf> = seen
        .into_iter()
        .filter(|(p, print)| known.get(p) != Some(print))
        .map(|(p, _)| p)
        .collect();
    emit(Event::Counted(changed.len()));
    for path in changed {
        if cancel.load(Ordering::Relaxed) {
            return;
        }
        emit(Event::Found(tags::read(&path)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_tree(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("lantern-scan-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for (file, body) in [
            ("a/one.mp3", "1"),
            ("a/two.FLAC", "22"),
            ("a/cover.jpg", "x"),
            ("a/.hidden.mp3", "x"),
            (".trash/three.mp3", "x"),
            ("b/c/four.m4a", "4444"),
        ] {
            let p = root.join(file);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, body).unwrap();
        }
        root
    }

    fn run(folders: &[PathBuf], known: &HashMap<PathBuf, (u64, i64)>) -> Vec<Event> {
        let mut events = Vec::new();
        scan(folders, known, &AtomicBool::new(false), |e| events.push(e));
        events
    }

    fn found(events: &[Event]) -> Vec<String> {
        events
            .iter()
            .filter_map(|e| match e {
                Event::Found(t) => Some(t.title.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn finds_audio_skipping_hidden_and_other_files() {
        let root = temp_tree("find");
        let events = run(std::slice::from_ref(&root), &HashMap::new());
        assert_eq!(events[0], Event::Counted(3));
        assert_eq!(found(&events), ["one", "two", "four"]);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn rescans_only_what_changed_and_notices_what_went() {
        let root = temp_tree("rescan");
        let mut known = HashMap::new();
        for e in run(std::slice::from_ref(&root), &HashMap::new()) {
            if let Event::Found(t) = e {
                known.insert(t.path, (t.size, t.modified));
            }
        }
        known.insert(root.join("a/deleted.mp3"), (1, 1));
        // Outside every watched folder: an import, say. Never "gone".
        known.insert(PathBuf::from("/elsewhere/x.mp3"), (1, 1));
        std::fs::write(root.join("a/one.mp3"), "longer now").unwrap();

        let events = run(std::slice::from_ref(&root), &known);
        assert_eq!(events[0], Event::Gone(vec![root.join("a/deleted.mp3")]));
        assert_eq!(events[1], Event::Counted(1));
        assert_eq!(found(&events), ["one"]);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_missing_folder_loses_nothing() {
        let gone_drive = PathBuf::from("/Volumes/not-plugged-in-lantern");
        let known = HashMap::from([(gone_drive.join("song.mp3"), (1, 1))]);
        let events = run(&[gone_drive], &known);
        assert_eq!(events, [Event::Counted(0)]);
    }
}
