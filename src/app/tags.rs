//! Reads tags off the UI thread, so a thousand-track playlist appears at
//! once and fills in its titles and durations as they load.

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};

use lantern_audio::tags::{self, Tags};

pub struct TagLoader {
    requests: Sender<PathBuf>,
    results: Receiver<(PathBuf, Tags)>,
}

impl TagLoader {
    pub fn new() -> Self {
        let (requests, inbox) = channel::<PathBuf>();
        let (outbox, results) = channel();
        std::thread::Builder::new()
            .name("lantern-tags".into())
            .spawn(move || {
                // Ends when the loader, and so the sender, is dropped.
                for path in inbox {
                    let tags = tags::read(&path);
                    if outbox.send((path, tags)).is_err() {
                        break;
                    }
                }
            })
            .expect("spawning the tag reader thread");
        Self { requests, results }
    }

    pub fn request(&self, path: PathBuf) {
        let _ = self.requests.send(path);
    }

    /// Everything read since the last call. Never blocks.
    pub fn poll(&self) -> Vec<(PathBuf, Tags)> {
        self.results.try_iter().collect()
    }
}
