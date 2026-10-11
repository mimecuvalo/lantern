//! The playlist: what is queued, what is selected, what plays next — and the
//! file formats that fill it (M3U/M3U8, PLS, cue sheets).
//!
//! Pure data and parsing, like `lantern-skin`: no audio, no windows, no tag
//! reading. The app fills in titles and durations from tags as they load.

pub mod cue;
pub mod m3u;
pub mod text;

use std::path::{Path, PathBuf};

/// Part of a file, for cue-sheet tracks. Seconds from the file's start.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Segment {
    pub start: f64,
    /// `None` plays to the end of the file.
    pub end: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub path: PathBuf,
    /// From tags, a playlist hint or a cue sheet; `None` until known.
    pub title: Option<String>,
    pub duration: Option<f64>,
    pub segment: Option<Segment>,
    pub selected: bool,
    /// Tags have been read (or tried), so the app need not read them again.
    pub tags_loaded: bool,
}

impl Entry {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            title: None,
            duration: None,
            segment: None,
            selected: false,
            tags_loaded: false,
        }
    }

    /// What the playlist shows: the title, or failing that the file name.
    pub fn display_title(&self) -> String {
        self.title.clone().unwrap_or_else(|| {
            self.path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default()
        })
    }
}

/// Load a file into entries: a playlist or cue sheet is expanded, anything
/// else is taken to be audio. `Err` only for a cue sheet that is broken.
pub fn load(path: &Path) -> std::io::Result<Result<Vec<Entry>, cue::CueError>> {
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    let dir = path.parent().unwrap_or(Path::new("."));
    Ok(Ok(match ext.as_str() {
        "m3u" | "m3u8" => m3u::parse_m3u(&std::fs::read(path)?, dir),
        "pls" => m3u::parse_pls(&std::fs::read(path)?, dir),
        "cue" => match cue::parse(&std::fs::read(path)?) {
            Ok(sheet) => sheet.entries(dir, |p| p.exists()),
            Err(e) => return Ok(Err(e)),
        },
        _ => vec![Entry::new(path.to_path_buf())],
    }))
}

/// Does the extension name something this crate expands?
pub fn is_playlist(path: &Path) -> bool {
    path.extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .is_some_and(|e| matches!(e.as_str(), "m3u" | "m3u8" | "pls" | "cue"))
}

/// How a click changes the selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Select {
    /// Plain click: only this one.
    Only,
    /// Cmd/Ctrl-click: flip this one.
    Toggle,
    /// Shift-click: everything from the last plain click to here.
    Extend,
}

#[derive(Debug, Clone, Default)]
pub struct Playlist {
    entries: Vec<Entry>,
    current: Option<usize>,
    /// Where a Shift-click range starts.
    anchor: Option<usize>,
}

impl Playlist {
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn get(&self, i: usize) -> Option<&Entry> {
        self.entries.get(i)
    }

    pub fn get_mut(&mut self, i: usize) -> Option<&mut Entry> {
        self.entries.get_mut(i)
    }

    /// The entry playing, or that would play on Play.
    pub fn current(&self) -> Option<usize> {
        self.current
    }

    pub fn set_current(&mut self, i: Option<usize>) {
        self.current = i.filter(|&i| i < self.entries.len());
    }

    pub fn add(&mut self, entries: impl IntoIterator<Item = Entry>) {
        self.entries.extend(entries);
        if self.current.is_none() && !self.entries.is_empty() {
            self.current = Some(0);
        }
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.current = None;
        self.anchor = None;
    }

    /// Drop every entry for which `keep` is false, keeping `current` on the
    /// same entry if it survives, and on whatever slid into its place if not.
    fn retain(&mut self, keep: impl Fn(&Entry) -> bool) {
        let mut new_current = None;
        let mut kept = 0;
        for (i, e) in self.entries.iter().enumerate() {
            if keep(e) {
                if Some(i) == self.current {
                    new_current = Some(kept);
                }
                kept += 1;
            } else if Some(i) == self.current {
                new_current = Some(kept);
            }
        }
        self.entries.retain(|e| keep(e));
        self.current = new_current.filter(|&i| i < self.entries.len()).or((!self
            .entries
            .is_empty()
            && self.current.is_some())
        .then(|| self.entries.len() - 1));
        self.anchor = None;
    }

    pub fn remove_selected(&mut self) {
        self.retain(|e| !e.selected);
    }

    /// Winamp's "crop": keep only the selection.
    pub fn crop(&mut self) {
        self.retain(|e| e.selected);
    }

    pub fn select(&mut self, i: usize, how: Select) {
        if i >= self.entries.len() {
            return;
        }
        match how {
            Select::Only => {
                for e in &mut self.entries {
                    e.selected = false;
                }
                self.entries[i].selected = true;
                self.anchor = Some(i);
            }
            Select::Toggle => {
                self.entries[i].selected = !self.entries[i].selected;
                self.anchor = Some(i);
            }
            Select::Extend => {
                let a = self.anchor.unwrap_or(i);
                let (lo, hi) = (a.min(i), a.max(i));
                for (j, e) in self.entries.iter_mut().enumerate() {
                    e.selected = (lo..=hi).contains(&j);
                }
            }
        }
    }

    /// Winamp's "remove dead files": entries whose file is gone. `exists`
    /// is injected for testing.
    pub fn remove_missing(&mut self, exists: impl Fn(&Path) -> bool) {
        self.retain(|e| exists(&e.path));
    }

    /// Put the entries in `order` (a permutation of indices), keeping
    /// `current` on the same entry.
    fn reorder(&mut self, order: Vec<usize>) {
        let current = self
            .current
            .and_then(|c| order.iter().position(|&i| i == c));
        let mut old: Vec<Option<Entry>> = self.entries.drain(..).map(Some).collect();
        self.entries = order.into_iter().filter_map(|i| old[i].take()).collect();
        self.current = current;
        self.anchor = None;
    }

    /// Stable sort by `key`.
    pub fn sort_by_key<K: Ord>(&mut self, key: impl Fn(&Entry) -> K) {
        let mut order: Vec<usize> = (0..self.entries.len()).collect();
        order.sort_by_key(|&i| key(&self.entries[i]));
        self.reorder(order);
    }

    pub fn reverse(&mut self) {
        self.reorder((0..self.entries.len()).rev().collect());
    }

    /// Shuffle the list itself — Winamp's "randomize list" — using `random`
    /// to pick below a bound (Fisher-Yates).
    pub fn randomize(&mut self, mut random: impl FnMut(usize) -> usize) {
        let mut order: Vec<usize> = (0..self.entries.len()).collect();
        for i in (1..order.len()).rev() {
            order.swap(i, random(i + 1));
        }
        self.reorder(order);
    }

    pub fn select_all(&mut self, on: bool) {
        for e in &mut self.entries {
            e.selected = on;
        }
    }

    pub fn invert_selection(&mut self) {
        for e in &mut self.entries {
            e.selected = !e.selected;
        }
    }

    /// The entry after `current`: in order, or at random with `shuffle`
    /// (which picks any other entry). `None` at the end unless `repeat`.
    pub fn next(
        &self,
        repeat: bool,
        shuffle: Option<&mut dyn FnMut(usize) -> usize>,
    ) -> Option<usize> {
        let n = self.entries.len();
        if n == 0 {
            return None;
        }
        let cur = self.current?;
        if let Some(random) = shuffle {
            if n == 1 {
                return repeat.then_some(0);
            }
            // Any entry but this one.
            let pick = random(n - 1);
            return Some(if pick >= cur { pick + 1 } else { pick });
        }
        match cur + 1 {
            i if i < n => Some(i),
            _ if repeat => Some(0),
            _ => None,
        }
    }

    pub fn previous(&self, repeat: bool) -> Option<usize> {
        let n = self.entries.len();
        match self.current? {
            0 if repeat && n > 0 => Some(n - 1),
            0 => None,
            i => Some(i - 1),
        }
    }

    /// Sum of known durations, and whether every duration is known — Winamp
    /// shows a `+` when some are not.
    pub fn total_duration(&self) -> (f64, bool) {
        let known = self.entries.iter().filter_map(whole_seconds).sum();
        (known, self.entries.iter().all(|e| e.duration.is_some()))
    }

    /// Sum of durations over the selection.
    pub fn selected_duration(&self) -> f64 {
        self.entries
            .iter()
            .filter(|e| e.selected)
            .filter_map(whole_seconds)
            .sum()
    }
}

/// An entry's length in whole seconds: totals add these up, as Winamp's
/// do, so they agree with the lengths the list shows.
fn whole_seconds(e: &Entry) -> Option<f64> {
    e.duration.map(f64::trunc)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn playlist(n: usize) -> Playlist {
        let mut p = Playlist::default();
        p.add((0..n).map(|i| Entry::new(PathBuf::from(format!("{i}.mp3")))));
        p
    }

    fn selected(p: &Playlist) -> Vec<usize> {
        (0..p.len()).filter(|&i| p.entries[i].selected).collect()
    }

    #[test]
    fn adding_to_an_empty_playlist_makes_the_first_current() {
        assert_eq!(playlist(3).current(), Some(0));
        assert_eq!(Playlist::default().current(), None);
    }

    #[test]
    fn selection_modes() {
        let mut p = playlist(6);
        p.select(1, Select::Only);
        p.select(4, Select::Extend);
        assert_eq!(selected(&p), [1, 2, 3, 4]);
        p.select(2, Select::Toggle);
        assert_eq!(selected(&p), [1, 3, 4]);
        p.select(0, Select::Only);
        assert_eq!(selected(&p), [0]);
        p.invert_selection();
        assert_eq!(selected(&p), [1, 2, 3, 4, 5]);
    }

    #[test]
    fn removing_keeps_current_on_its_entry() {
        let mut p = playlist(5);
        p.set_current(Some(3));
        p.select(0, Select::Only);
        p.select(1, Select::Toggle);
        p.remove_selected();
        assert_eq!(p.len(), 3);
        assert_eq!(p.current(), Some(1));
        assert_eq!(p.get(1).unwrap().path, Path::new("3.mp3"));
    }

    #[test]
    fn removing_the_current_entry_moves_to_its_successor() {
        let mut p = playlist(3);
        p.set_current(Some(1));
        p.select(1, Select::Only);
        p.remove_selected();
        assert_eq!(
            p.get(p.current().unwrap()).unwrap().path,
            Path::new("2.mp3")
        );

        let mut p = playlist(3);
        p.set_current(Some(2));
        p.select(2, Select::Only);
        p.remove_selected();
        assert_eq!(p.current(), Some(1), "removing the last falls back");
    }

    #[test]
    fn crop_keeps_the_selection() {
        let mut p = playlist(4);
        p.select(1, Select::Only);
        p.select(2, Select::Toggle);
        p.crop();
        let paths: Vec<_> = p.entries().iter().map(|e| e.path.clone()).collect();
        assert_eq!(paths, [PathBuf::from("1.mp3"), PathBuf::from("2.mp3")]);
    }

    #[test]
    fn next_and_previous_respect_repeat() {
        let mut p = playlist(3);
        p.set_current(Some(2));
        assert_eq!(p.next(false, None), None);
        assert_eq!(p.next(true, None), Some(0));
        p.set_current(Some(0));
        assert_eq!(p.previous(false), None);
        assert_eq!(p.previous(true), Some(2));
        assert_eq!(p.next(false, None), Some(1));
    }

    #[test]
    fn shuffle_never_repeats_the_current_entry() {
        let mut p = playlist(4);
        p.set_current(Some(2));
        for pick in 0..3 {
            let mut rng = |_n: usize| pick;
            let next = p.next(false, Some(&mut rng)).unwrap();
            assert_ne!(next, 2);
            assert!(next < 4);
        }
    }

    #[test]
    fn sorting_and_reordering_keep_current_on_its_entry() {
        let mut p = playlist(4);
        p.set_current(Some(1));
        p.reverse();
        assert_eq!(
            p.get(p.current().unwrap()).unwrap().path,
            Path::new("1.mp3")
        );
        assert_eq!(p.get(0).unwrap().path, Path::new("3.mp3"));

        p.sort_by_key(|e| e.path.clone());
        assert_eq!(p.current(), Some(1));

        let mut n = 0;
        p.randomize(|bound| {
            n += 1;
            n % bound
        });
        assert_eq!(p.len(), 4);
        assert_eq!(
            p.get(p.current().unwrap()).unwrap().path,
            Path::new("1.mp3")
        );
    }

    #[test]
    fn remove_missing_drops_dead_files() {
        let mut p = playlist(3);
        p.remove_missing(|path| path != Path::new("1.mp3"));
        assert_eq!(p.len(), 2);
    }

    #[test]
    fn total_duration_flags_unknowns() {
        let mut p = playlist(2);
        p.get_mut(0).unwrap().duration = Some(60.0);
        assert_eq!(p.total_duration(), (60.0, false));
        p.get_mut(1).unwrap().duration = Some(30.9);
        assert_eq!(p.total_duration(), (90.0, true), "whole seconds add up");
    }

    #[test]
    fn load_expands_playlists_and_passes_audio_through() {
        let dir = std::env::temp_dir().join(format!("lantern-pl-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let m3u = dir.join("list.m3u");
        std::fs::write(&m3u, "a.mp3\nb.mp3\n").unwrap();
        let loaded = load(&m3u).unwrap().unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[1].path, dir.join("b.mp3"));
        let audio = load(&dir.join("song.flac")).unwrap().unwrap();
        assert_eq!(audio[0].path, dir.join("song.flac"));
        std::fs::remove_dir_all(&dir).ok();
    }
}
