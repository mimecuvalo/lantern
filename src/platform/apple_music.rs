//! Reading the Apple Music library through Apple's iTunesLibrary framework.
//!
//! The supported way in: it needs no XML export, and brings play counts,
//! ratings and playlists. The first use asks the user's permission to reach
//! their media library; refused, it fails with the system's reason. The
//! framework is loaded on demand, so a Mac without it still runs.
//!
//! Only an app can use it: the framework's helper refuses a bare binary
//! started from a terminal, which macOS holds the terminal responsible for.
//! `scripts/bundle-app.sh --open` runs Lantern as an app during development.
//!
//! Slow on a big library, so call it off the UI thread.

// Message sends to the framework's documented classes; see the SAFETY notes.
#![allow(unsafe_code)]

use std::path::PathBuf;

use lantern_library::TrackInfo;
use lantern_library::import::{Imported, ImportedTrack, stars};
use objc2::msg_send;
use objc2::rc::{Retained, autoreleasepool};
use objc2::runtime::{AnyClass, AnyObject};
use objc2_foundation::{NSArray, NSString};

const FRAMEWORK: &str = "/System/Library/Frameworks/iTunesLibrary.framework";

/// `ITLibMediaItemLocationTypeFile`.
const LOCATION_FILE: usize = 1;
/// `ITLibMediaItemMediaKindSong`.
const KIND_SONG: usize = 2;
/// `ITLibDistinguishedPlaylistKindNone`.
const DISTINGUISHED_NONE: usize = 0;
/// `ITLibPlaylistKindFolder`.
const PLAYLIST_FOLDER: usize = 3;

type Obj = Retained<AnyObject>;

fn text(s: Option<Retained<NSString>>) -> String {
    s.map(|s| s.to_string().trim().to_string())
        .unwrap_or_default()
}

fn positive(n: usize) -> Option<u32> {
    u32::try_from(n).ok().filter(|&n| n > 0)
}

/// SAFETY for every helper below: each sends a property getter the
/// iTunesLibrary headers declare on that object's class, with the return
/// type they declare.
fn string(obj: &AnyObject, get: &str) -> String {
    let sel = objc2::runtime::Sel::register(&std::ffi::CString::new(get).unwrap());
    // SAFETY: see above; every getter named here returns `NSString *`.
    let s: Option<Retained<NSString>> = unsafe { msg_send![obj, performSelector: sel] };
    text(s)
}

fn date_secs(obj: &AnyObject, get: &str) -> Option<i64> {
    let sel = objc2::runtime::Sel::register(&std::ffi::CString::new(get).unwrap());
    // SAFETY: see above; the getters named here return `NSDate *`.
    let date: Option<Obj> = unsafe { msg_send![obj, performSelector: sel] };
    // SAFETY: `timeIntervalSince1970` is NSDate's, returning a double.
    let secs: f64 = unsafe { msg_send![&*date?, timeIntervalSince1970] };
    Some(secs as i64)
}

fn track(item: &AnyObject) -> Option<ImportedTrack> {
    // SAFETY: documented ITLibMediaItem getters with their declared types.
    let (location_type, kind, video, cloud): (usize, usize, bool, bool) = unsafe {
        (
            msg_send![item, locationType],
            msg_send![item, mediaKind],
            msg_send![item, isVideo],
            msg_send![item, isCloud],
        )
    };
    if location_type != LOCATION_FILE || kind != KIND_SONG || video || cloud {
        return None;
    }
    // SAFETY: `location` is an NSURL, whose `path` is an NSString.
    let location: Option<Obj> = unsafe { msg_send![item, location] };
    let path: Option<Retained<NSString>> = unsafe { msg_send![&*location?, path] };
    let path = PathBuf::from(path?.to_string());
    let meta = std::fs::metadata(&path).ok()?;
    let (size, modified) = lantern_library::tags::fingerprint(&meta);

    // SAFETY: documented getters: `artist` an ITLibArtist, `album` an
    // ITLibAlbum, the rest NSUInteger, NSInteger or BOOL as declared.
    let (artist, album, year, track_no, total_ms, kbps, plays, rating, computed) = unsafe {
        let artist: Option<Obj> = msg_send![item, artist];
        let album: Option<Obj> = msg_send![item, album];
        let year: usize = msg_send![item, year];
        let track_no: usize = msg_send![item, trackNumber];
        let total_ms: usize = msg_send![item, totalTime];
        let kbps: usize = msg_send![item, bitrate];
        let plays: usize = msg_send![item, playCount];
        let rating: isize = msg_send![item, rating];
        let computed: bool = msg_send![item, isRatingComputed];
        (
            artist, album, year, track_no, total_ms, kbps, plays, rating, computed,
        )
    };
    let disc_no = album.as_deref().map_or(0, |a| {
        // SAFETY: ITLibAlbum's `discNumber` is an NSUInteger.
        let n: usize = unsafe { msg_send![a, discNumber] };
        n
    });
    let mut title = string(item, "title");
    if title.is_empty() {
        title = path.file_stem()?.to_string_lossy().into_owned();
    }
    Some(ImportedTrack {
        info: TrackInfo {
            title,
            artist: artist
                .as_deref()
                .map(|a| string(a, "name"))
                .unwrap_or_default(),
            album: album
                .as_deref()
                .map(|a| string(a, "title"))
                .unwrap_or_default(),
            album_artist: album
                .as_deref()
                .map(|a| string(a, "albumArtist"))
                .unwrap_or_default(),
            genre: string(item, "genre"),
            year: positive(year),
            track_no: positive(track_no),
            disc_no: positive(disc_no),
            duration_secs: Some(total_ms as f64 / 1000.0).filter(|&s| s > 0.0),
            kbps: positive(kbps),
            size,
            modified,
            path,
        },
        added: date_secs(item, "addedDate"),
        play_count: u32::try_from(plays).unwrap_or(u32::MAX),
        last_played: date_secs(item, "lastPlayedDate"),
        rating: if computed { 0 } else { stars(rating as i64) },
    })
}

/// An item's persistent ID: the same item from any list has the same one.
fn persistent_id(item: &AnyObject) -> Option<u64> {
    // SAFETY: ITLibMediaItem's `persistentID` is an NSNumber.
    let n: Option<Obj> = unsafe { msg_send![item, persistentID] };
    // SAFETY: NSNumber's `unsignedLongLongValue`.
    Some(unsafe { msg_send![&*n?, unsignedLongLongValue] })
}

fn items(list: &AnyObject) -> Vec<Obj> {
    // SAFETY: `allMediaItems` and `items` both return NSArray of
    // ITLibMediaItem.
    let array: Option<Retained<NSArray<AnyObject>>> = unsafe { msg_send![list, items] };
    array.map(|a| a.to_vec()).unwrap_or_default()
}

/// The user's Apple Music library: its local songs and its playlists.
/// Said when Lantern is not running as an app, which the framework needs.
const NOT_AN_APP: &str = "Apple Music only answers Lantern running as an app \
     (scripts/bundle-app.sh --open); or import its library XML file";

/// `NSXPCConnectionInvalid`: the framework's helper would not talk.
const HELPER_REFUSED: isize = 4097;

pub fn read() -> Result<Imported, String> {
    autoreleasepool(|_| {
        let bundle_class = AnyClass::get(c"NSBundle").ok_or("no NSBundle")?;
        // SAFETY: NSBundle's documented `mainBundle` and its
        // `bundleIdentifier`, an NSString or nil.
        let identified = unsafe {
            let main: Obj = msg_send![bundle_class, mainBundle];
            let id: Option<Retained<NSString>> = msg_send![&*main, bundleIdentifier];
            id.is_some()
        };
        if !identified {
            return Err(NOT_AN_APP.into());
        }
        let path = NSString::from_str(FRAMEWORK);
        // SAFETY: NSBundle's documented `bundleWithPath:` and `load`.
        let loaded = unsafe {
            let bundle: Option<Obj> = msg_send![bundle_class, bundleWithPath: &*path];
            bundle.is_some_and(|b| {
                let ok: bool = msg_send![&*b, load];
                ok
            })
        };
        let class = AnyClass::get(c"ITLibrary")
            .filter(|_| loaded)
            .ok_or("this Mac has no iTunesLibrary framework")?;
        let version = NSString::from_str("1.1");
        let mut error: *mut AnyObject = std::ptr::null_mut();
        // SAFETY: `+libraryWithAPIVersion:error:` as declared, with an
        // out-pointer for the error.
        let library: Option<Obj> =
            unsafe { msg_send![class, libraryWithAPIVersion: &*version, error: &mut error] };
        let Some(library) = library else {
            // SAFETY: on failure the framework sets an NSError, whose
            // `localizedDescription` is an NSString.
            let reason = (!error.is_null()).then(|| unsafe {
                let code: isize = msg_send![&*error, code];
                if code == HELPER_REFUSED {
                    return NOT_AN_APP.to_string();
                }
                let s: Option<Retained<NSString>> = msg_send![&*error, localizedDescription];
                text(s)
            });
            return Err(reason.unwrap_or_else(|| "Apple Music did not open".into()));
        };

        let mut out = Imported::default();
        // SAFETY: ITLibrary's `allMediaItems`, an NSArray of ITLibMediaItem.
        let all: Retained<NSArray<AnyObject>> = unsafe { msg_send![&*library, allMediaItems] };
        let mut by_item = std::collections::HashMap::new();
        for item in all.iter() {
            match track(&item) {
                Some(t) => {
                    if let Some(id) = persistent_id(&item) {
                        by_item.insert(id, t.info.path.clone());
                    }
                    out.tracks.push(t);
                }
                None => out.skipped += 1,
            }
        }

        // SAFETY: ITLibrary's `allPlaylists`, an NSArray of ITLibPlaylist,
        // and that class's documented getters.
        let lists: Retained<NSArray<AnyObject>> = unsafe { msg_send![&*library, allPlaylists] };
        for list in lists.iter() {
            let (master, distinguished, kind, visible): (bool, usize, usize, bool) = unsafe {
                (
                    msg_send![&*list, isMaster],
                    msg_send![&*list, distinguishedKind],
                    msg_send![&*list, kind],
                    msg_send![&*list, isVisible],
                )
            };
            if master || distinguished != DISTINGUISHED_NONE || kind == PLAYLIST_FOLDER || !visible
            {
                continue;
            }
            let paths = items(&list)
                .iter()
                .filter_map(|i| by_item.get(&persistent_id(i)?).cloned())
                .collect();
            out.playlists.push((string(&list, "name"), paths));
        }
        Ok(out)
    })
}
