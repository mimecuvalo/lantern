//! The library's database: tracks, the folders watched for them, and
//! playlists brought in from elsewhere.
//!
//! SQLite, one file, opened by the UI thread. Writes come in batches —
//! a scan's worth of results at a time — each in one transaction. The
//! schema is versioned with `PRAGMA user_version` so it can grow.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension, params};

use crate::{Result, Track, TrackInfo};

/// Bumped with each change to the schema; see [`Library::migrate`].
const SCHEMA_VERSION: i32 = 1;

pub struct Library {
    db: Connection,
}

/// A playlist kept in the library, such as one imported from Apple Music.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Playlist {
    pub id: i64,
    pub name: String,
    /// Where it came from, e.g. `"apple-music"`.
    pub source: String,
    /// Track ids, in order. Entries whose track has left the library drop out.
    pub tracks: Vec<i64>,
}

fn path_text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn row_to_track(row: &rusqlite::Row<'_>) -> rusqlite::Result<Track> {
    Ok(Track {
        id: row.get("id")?,
        info: TrackInfo {
            path: PathBuf::from(row.get::<_, String>("path")?),
            title: row.get("title")?,
            artist: row.get("artist")?,
            album: row.get("album")?,
            album_artist: row.get("album_artist")?,
            genre: row.get("genre")?,
            year: row.get("year")?,
            track_no: row.get("track_no")?,
            disc_no: row.get("disc_no")?,
            duration_secs: row.get("duration")?,
            kbps: row.get("kbps")?,
            size: row.get::<_, i64>("size")? as u64,
            modified: row.get("modified")?,
        },
        added: row.get("added")?,
        play_count: row.get("play_count")?,
        last_played: row.get("last_played")?,
        rating: row.get("rating")?,
    })
}

impl Library {
    /// Open the library at `path`, creating it if need be.
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        Self::setup(Connection::open(path)?)
    }

    /// A library that lives only as long as it does: for tests.
    pub fn open_in_memory() -> Result<Self> {
        Self::setup(Connection::open_in_memory()?)
    }

    fn setup(db: Connection) -> Result<Self> {
        // WAL keeps a crash from costing more than the last batch, and lets
        // a reader in while a batch is written.
        db.pragma_update(None, "journal_mode", "WAL")?;
        db.pragma_update(None, "foreign_keys", true)?;
        let library = Self { db };
        library.migrate()?;
        Ok(library)
    }

    fn migrate(&self) -> Result<()> {
        let version: i32 = self
            .db
            .pragma_query_value(None, "user_version", |r| r.get(0))?;
        if version < 1 {
            self.db.execute_batch(
                "CREATE TABLE folders (path TEXT PRIMARY KEY);
                 CREATE TABLE tracks (
                     id INTEGER PRIMARY KEY,
                     path TEXT NOT NULL UNIQUE,
                     title TEXT NOT NULL DEFAULT '',
                     artist TEXT NOT NULL DEFAULT '',
                     album TEXT NOT NULL DEFAULT '',
                     album_artist TEXT NOT NULL DEFAULT '',
                     genre TEXT NOT NULL DEFAULT '',
                     year INTEGER,
                     track_no INTEGER,
                     disc_no INTEGER,
                     duration REAL,
                     kbps INTEGER,
                     size INTEGER NOT NULL DEFAULT 0,
                     modified INTEGER NOT NULL DEFAULT 0,
                     added INTEGER NOT NULL,
                     play_count INTEGER NOT NULL DEFAULT 0,
                     last_played INTEGER,
                     rating INTEGER NOT NULL DEFAULT 0
                 );
                 CREATE TABLE playlists (
                     id INTEGER PRIMARY KEY,
                     name TEXT NOT NULL,
                     source TEXT NOT NULL
                 );
                 CREATE TABLE playlist_items (
                     playlist INTEGER NOT NULL REFERENCES playlists(id) ON DELETE CASCADE,
                     position INTEGER NOT NULL,
                     track INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
                     PRIMARY KEY (playlist, position)
                 );",
            )?;
        }
        self.db
            .pragma_update(None, "user_version", SCHEMA_VERSION)?;
        Ok(())
    }

    // -- Folders ------------------------------------------------------------

    /// The folders scanned for music.
    pub fn folders(&self) -> Result<Vec<PathBuf>> {
        let mut q = self.db.prepare("SELECT path FROM folders ORDER BY path")?;
        let rows = q.query_map([], |r| r.get::<_, String>(0))?;
        Ok(rows
            .collect::<rusqlite::Result<Vec<_>>>()?
            .into_iter()
            .map(PathBuf::from)
            .collect())
    }

    pub fn add_folder(&self, path: &Path) -> Result<()> {
        self.db.execute(
            "INSERT OR IGNORE INTO folders (path) VALUES (?1)",
            [path_text(path)],
        )?;
        Ok(())
    }

    /// Stop watching `path`, and let go of the tracks found in it.
    pub fn remove_folder(&mut self, path: &Path) -> Result<()> {
        let tx = self.db.transaction()?;
        tx.execute("DELETE FROM folders WHERE path = ?1", [path_text(path)])?;
        let gone: Vec<String> = {
            let mut q = tx.prepare("SELECT path FROM tracks")?;
            q.query_map([], |r| r.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?
                .into_iter()
                .filter(|p| Path::new(p).starts_with(path))
                .collect()
        };
        for p in gone {
            tx.execute("DELETE FROM tracks WHERE path = ?1", [p])?;
        }
        tx.commit()?;
        Ok(())
    }

    // -- Tracks -------------------------------------------------------------

    /// Every track, in the order they were added.
    pub fn tracks(&self) -> Result<Vec<Track>> {
        let mut q = self.db.prepare("SELECT * FROM tracks ORDER BY id")?;
        let rows = q.query_map([], row_to_track)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn track_count(&self) -> Result<usize> {
        let n: i64 = self
            .db
            .query_row("SELECT COUNT(*) FROM tracks", [], |r| r.get(0))?;
        Ok(n as usize)
    }

    /// Each known file's size and modification time, so a scan can skip
    /// those that have not changed.
    pub fn fingerprints(&self) -> Result<HashMap<PathBuf, (u64, i64)>> {
        let mut q = self.db.prepare("SELECT path, size, modified FROM tracks")?;
        let rows = q.query_map([], |r| {
            Ok((
                PathBuf::from(r.get::<_, String>(0)?),
                (r.get::<_, i64>(1)? as u64, r.get::<_, i64>(2)?),
            ))
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Add these tracks, or refresh what is known of them. A track already
    /// here keeps its history — when it was added, its plays, its rating.
    pub fn upsert(&mut self, infos: &[TrackInfo], added: i64) -> Result<()> {
        let tx = self.db.transaction()?;
        for t in infos {
            Self::upsert_in(&tx, t, added)?;
        }
        tx.commit()?;
        Ok(())
    }

    pub(crate) fn upsert_in(
        tx: &rusqlite::Transaction<'_>,
        t: &TrackInfo,
        added: i64,
    ) -> Result<()> {
        let mut q = tx.prepare_cached(
            "INSERT INTO tracks (path, title, artist, album, album_artist, genre,
                 year, track_no, disc_no, duration, kbps, size, modified, added)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
             ON CONFLICT(path) DO UPDATE SET
                 title = excluded.title, artist = excluded.artist,
                 album = excluded.album, album_artist = excluded.album_artist,
                 genre = excluded.genre, year = excluded.year,
                 track_no = excluded.track_no, disc_no = excluded.disc_no,
                 duration = excluded.duration, kbps = excluded.kbps,
                 size = excluded.size, modified = excluded.modified",
        )?;
        q.execute(params![
            path_text(&t.path),
            t.title,
            t.artist,
            t.album,
            t.album_artist,
            t.genre,
            t.year,
            t.track_no,
            t.disc_no,
            t.duration_secs,
            t.kbps,
            t.size as i64,
            t.modified,
            added,
        ])?;
        Ok(())
    }

    pub fn remove(&mut self, paths: &[PathBuf]) -> Result<()> {
        let tx = self.db.transaction()?;
        {
            let mut q = tx.prepare("DELETE FROM tracks WHERE path = ?1")?;
            for p in paths {
                q.execute([path_text(p)])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Count a play of the file at `path`, if it is in the library.
    pub fn record_play(&self, path: &Path, at: i64) -> Result<bool> {
        let n = self.db.execute(
            "UPDATE tracks SET play_count = play_count + 1, last_played = ?2 WHERE path = ?1",
            params![path_text(path), at],
        )?;
        Ok(n > 0)
    }

    /// Rate a track, 0 (unrated) to 5 stars.
    pub fn set_rating(&self, id: i64, rating: u8) -> Result<()> {
        self.db.execute(
            "UPDATE tracks SET rating = ?2 WHERE id = ?1",
            params![id, rating.min(5)],
        )?;
        Ok(())
    }

    /// Fold history from elsewhere into a track's: the larger play count,
    /// the later last play, the earlier date added, and a rating where this
    /// library has none.
    pub(crate) fn merge_history(
        tx: &rusqlite::Transaction<'_>,
        path: &Path,
        play_count: u32,
        last_played: Option<i64>,
        rating: u8,
        added: Option<i64>,
    ) -> Result<()> {
        tx.execute(
            "UPDATE tracks SET
                 play_count = MAX(play_count, ?2),
                 last_played = CASE
                     WHEN ?3 IS NULL THEN last_played
                     WHEN last_played IS NULL OR ?3 > last_played THEN ?3
                     ELSE last_played END,
                 rating = CASE WHEN rating = 0 THEN ?4 ELSE rating END,
                 added = CASE WHEN ?5 IS NOT NULL AND ?5 < added THEN ?5 ELSE added END
             WHERE path = ?1",
            params![
                path_text(path),
                play_count,
                last_played,
                rating.min(5),
                added
            ],
        )?;
        Ok(())
    }

    pub(crate) fn transaction(&mut self) -> Result<rusqlite::Transaction<'_>> {
        Ok(self.db.transaction()?)
    }

    // -- Playlists ----------------------------------------------------------

    pub fn playlists(&self) -> Result<Vec<Playlist>> {
        let mut q = self
            .db
            .prepare("SELECT id, name, source FROM playlists ORDER BY id")?;
        let mut lists: Vec<Playlist> = q
            .query_map([], |r| {
                Ok(Playlist {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    source: r.get(2)?,
                    tracks: Vec::new(),
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        let mut items = self
            .db
            .prepare("SELECT track FROM playlist_items WHERE playlist = ?1 ORDER BY position")?;
        for list in &mut lists {
            list.tracks = items
                .query_map([list.id], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?;
        }
        Ok(lists)
    }

    /// Replace every playlist from `source` with `lists`: each a name and
    /// the paths of its tracks, in order. Paths not in the library are left
    /// out.
    pub(crate) fn replace_playlists(
        tx: &rusqlite::Transaction<'_>,
        source: &str,
        lists: &[(String, Vec<PathBuf>)],
    ) -> Result<()> {
        tx.execute("DELETE FROM playlists WHERE source = ?1", [source])?;
        let mut id_of = tx.prepare("SELECT id FROM tracks WHERE path = ?1")?;
        let mut item = tx.prepare(
            "INSERT INTO playlist_items (playlist, position, track) VALUES (?1, ?2, ?3)",
        )?;
        for (name, paths) in lists {
            tx.execute(
                "INSERT INTO playlists (name, source) VALUES (?1, ?2)",
                [name, source],
            )?;
            let list = tx.last_insert_rowid();
            let mut position = 0;
            for p in paths {
                let id: Option<i64> = id_of.query_row([path_text(p)], |r| r.get(0)).optional()?;
                if let Some(id) = id {
                    item.execute(params![list, position, id])?;
                    position += 1;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(path: &str, title: &str) -> TrackInfo {
        TrackInfo {
            path: PathBuf::from(path),
            title: title.into(),
            size: 100,
            modified: 5,
            ..TrackInfo::default()
        }
    }

    #[test]
    fn upsert_adds_then_refreshes_keeping_history() {
        let mut lib = Library::open_in_memory().unwrap();
        lib.upsert(&[info("/m/a.mp3", "A")], 1000).unwrap();
        assert!(lib.record_play(Path::new("/m/a.mp3"), 2000).unwrap());
        lib.set_rating(lib.tracks().unwrap()[0].id, 4).unwrap();

        // Retagged: the new title lands, the history stays.
        lib.upsert(&[info("/m/a.mp3", "A (Remastered)")], 3000)
            .unwrap();
        let t = &lib.tracks().unwrap()[0];
        assert_eq!(t.info.title, "A (Remastered)");
        assert_eq!(
            (t.added, t.play_count, t.last_played, t.rating),
            (1000, 1, Some(2000), 4)
        );
        assert_eq!(lib.track_count().unwrap(), 1);
    }

    #[test]
    fn removing_a_folder_lets_go_of_its_tracks() {
        let mut lib = Library::open_in_memory().unwrap();
        lib.add_folder(Path::new("/m/rock")).unwrap();
        lib.add_folder(Path::new("/m/jazz")).unwrap();
        lib.upsert(
            &[
                info("/m/rock/a.mp3", "A"),
                info("/m/rockabilly/b.mp3", "B"),
                info("/m/jazz/c.mp3", "C"),
            ],
            1,
        )
        .unwrap();
        lib.remove_folder(Path::new("/m/rock")).unwrap();
        assert_eq!(lib.folders().unwrap(), vec![PathBuf::from("/m/jazz")]);
        let left: Vec<String> = lib
            .tracks()
            .unwrap()
            .into_iter()
            .map(|t| t.info.title)
            .collect();
        assert_eq!(left, ["B", "C"], "a sibling sharing the prefix stays");
    }

    #[test]
    fn fingerprints_report_size_and_time() {
        let mut lib = Library::open_in_memory().unwrap();
        lib.upsert(&[info("/m/a.mp3", "A")], 1).unwrap();
        assert_eq!(lib.fingerprints().unwrap()[Path::new("/m/a.mp3")], (100, 5));
        lib.remove(&[PathBuf::from("/m/a.mp3")]).unwrap();
        assert!(lib.fingerprints().unwrap().is_empty());
    }

    #[test]
    fn opens_again_from_disk() {
        let dir = std::env::temp_dir().join(format!("lantern-library-{}", std::process::id()));
        let file = dir.join("library.db");
        {
            let mut lib = Library::open(&file).unwrap();
            lib.upsert(&[info("/m/a.mp3", "A")], 1).unwrap();
        }
        let lib = Library::open(&file).unwrap();
        assert_eq!(lib.track_count().unwrap(), 1);
        drop(lib);
        let _ = std::fs::remove_dir_all(dir);
    }
}
