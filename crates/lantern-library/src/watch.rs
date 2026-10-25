//! Watching the watched folders while the app runs, so music added,
//! changed or removed there shows up without a rescan by hand.
//!
//! Changes arrive in bursts — a copy of an album is dozens of events — so
//! nothing happens until they have been quiet for a moment; then `changed`
//! is called once, and the app rescans. The rescan itself only reads what
//! is new or different (see [`crate::scan`]).

use std::path::{Path, PathBuf};
use std::sync::mpsc::{RecvTimeoutError, channel};
use std::time::Duration;

use notify::{RecursiveMode, Watcher};

use crate::scan::is_audio;

/// Watches folders until dropped.
pub struct FolderWatch {
    _watcher: notify::RecommendedWatcher,
}

/// Whether an event about `path` could change the library: an audio file,
/// or something without an extension, which may be a folder of them.
fn matters(path: &Path) -> bool {
    is_audio(path) || path.extension().is_none()
}

impl FolderWatch {
    /// Watch `folders`, each with everything under it. `changed` runs, on a
    /// thread of the watcher's, once changes have settled for `settle`.
    pub fn start(
        folders: &[PathBuf],
        settle: Duration,
        changed: impl Fn() + Send + 'static,
    ) -> notify::Result<Self> {
        let (tx, rx) = channel::<()>();
        let mut watcher =
            notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                if let Ok(e) = event
                    && !e.kind.is_access()
                    && e.paths.iter().any(|p| matters(p))
                {
                    let _ = tx.send(());
                }
            })?;
        for folder in folders.iter().filter(|f| f.is_dir()) {
            watcher.watch(folder, RecursiveMode::Recursive)?;
        }
        std::thread::Builder::new()
            .name("lantern-watch".into())
            .spawn(move || {
                // Ends when the watcher, and so the sender, is dropped.
                while rx.recv().is_ok() {
                    loop {
                        match rx.recv_timeout(settle) {
                            Ok(()) => {}
                            Err(RecvTimeoutError::Timeout) => break,
                            Err(RecvTimeoutError::Disconnected) => return,
                        }
                    }
                    changed();
                }
            })
            .map_err(|e| notify::Error::generic(&e.to_string()))?;
        Ok(Self { _watcher: watcher })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Instant;

    #[test]
    fn a_burst_of_new_music_is_one_change() {
        let dir = std::env::temp_dir().join(format!("lantern-watch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        // FSEvents reports through the real path, not /var's /private link.
        let dir = dir.canonicalize().unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let seen = Arc::clone(&calls);
        let watch = FolderWatch::start(
            std::slice::from_ref(&dir),
            Duration::from_secs(1),
            move || {
                seen.fetch_add(1, Ordering::SeqCst);
            },
        )
        .unwrap();
        // Let the watcher settle in before changing anything.
        std::thread::sleep(Duration::from_millis(500));
        for n in 0..5 {
            std::fs::write(dir.join(format!("{n}.mp3")), "x").unwrap();
        }
        let deadline = Instant::now() + Duration::from_secs(10);
        while calls.load(Ordering::SeqCst) == 0 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(50));
        }
        // Long enough for a second call, were the burst split.
        std::thread::sleep(Duration::from_millis(1500));
        assert_eq!(calls.load(Ordering::SeqCst), 1, "one call for the burst");
        drop(watch);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn only_music_and_folders_matter() {
        assert!(matters(Path::new("/m/a.flac")));
        assert!(matters(Path::new("/m/New Album")));
        assert!(!matters(Path::new("/m/.DS_Store.tmp")));
        assert!(!matters(Path::new("/m/cover.jpg")));
    }
}
