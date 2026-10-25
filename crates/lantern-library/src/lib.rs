//! The media library: every track Lantern knows about, beyond the playlist.
//!
//! Tracks come from watched folders, scanned for audio files and their tags
//! ([`scan`], [`tags`]), and from an Apple Music library ([`import`]). They
//! are kept in SQLite ([`Library`]), with what Winamp's library kept beside
//! them: when each was added, how often and when it was last played, and its
//! rating. [`view`] holds the library's standing views — Most Played,
//! Recently Added and the rest — and its search.
//!
//! No UI and no audio here: the app drives a scan on a thread of its own and
//! hands the results to the store.

pub mod import;
pub mod scan;
mod store;
pub mod tags;
pub mod view;

use std::path::PathBuf;

pub use store::{Library, Playlist};

#[derive(Debug, thiserror::Error)]
pub enum LibraryError {
    #[error("library database: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("not an iTunes or Apple Music library file: {0}")]
    NotALibrary(String),
}

pub type Result<T> = std::result::Result<T, LibraryError>;

/// What a file says about itself: its tags, and enough of its metadata to
/// tell later whether it has changed.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TrackInfo {
    pub path: PathBuf,
    /// The tagged title, or failing that the file's name.
    pub title: String,
    pub artist: String,
    pub album: String,
    pub album_artist: String,
    pub genre: String,
    pub year: Option<u32>,
    pub track_no: Option<u32>,
    pub disc_no: Option<u32>,
    pub duration_secs: Option<f64>,
    pub kbps: Option<u32>,
    pub size: u64,
    /// Seconds since the Unix epoch.
    pub modified: i64,
}

/// A track in the library: its info, and its history here.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Track {
    pub id: i64,
    pub info: TrackInfo,
    /// When it joined the library, in seconds since the Unix epoch.
    pub added: i64,
    pub play_count: u32,
    pub last_played: Option<i64>,
    /// 0 (unrated) to 5 stars.
    pub rating: u8,
}

/// Seconds since the Unix epoch, now.
pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}
