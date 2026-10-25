//! The library's standing views, its search, and sorting by column.
//!
//! Views are Winamp's Local Media ones: all audio, then smart views over
//! play history and ratings. All of it runs over the tracks in memory —
//! tens of thousands filter and sort in milliseconds, which keeps the
//! search live as you type.

use std::cmp::Ordering;

use crate::Track;

/// "Recent" reaches back this far.
const RECENT_SECS: i64 = 30 * 24 * 60 * 60;
/// The fewest stars that make Top Rated.
const TOP_RATING: u8 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum View {
    Audio,
    MostPlayed,
    RecentlyAdded,
    RecentlyPlayed,
    NeverPlayed,
    TopRated,
}

impl View {
    pub const ALL: [View; 6] = [
        View::Audio,
        View::MostPlayed,
        View::RecentlyAdded,
        View::RecentlyPlayed,
        View::NeverPlayed,
        View::TopRated,
    ];

    pub fn name(self) -> &'static str {
        match self {
            View::Audio => "Audio",
            View::MostPlayed => "Most Played",
            View::RecentlyAdded => "Recently Added",
            View::RecentlyPlayed => "Recently Played",
            View::NeverPlayed => "Never Played",
            View::TopRated => "Top Rated",
        }
    }

    pub fn includes(self, t: &Track, now: i64) -> bool {
        match self {
            View::Audio => true,
            View::MostPlayed => t.play_count > 0,
            View::RecentlyAdded => now - t.added <= RECENT_SECS,
            View::RecentlyPlayed => t.last_played.is_some_and(|at| now - at <= RECENT_SECS),
            View::NeverPlayed => t.play_count == 0,
            View::TopRated => t.rating >= TOP_RATING,
        }
    }

    /// How the view orders itself until a column header says otherwise.
    pub fn default_sort(self) -> Sort {
        let (column, descending) = match self {
            View::Audio | View::NeverPlayed => (Column::Artist, false),
            View::MostPlayed => (Column::PlayCount, true),
            View::RecentlyAdded => (Column::Added, true),
            View::RecentlyPlayed => (Column::LastPlayed, true),
            View::TopRated => (Column::Rating, true),
        };
        Sort { column, descending }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Column {
    Artist,
    Title,
    Album,
    Length,
    TrackNo,
    Genre,
    Year,
    Rating,
    PlayCount,
    LastPlayed,
    Added,
    Bitrate,
    Path,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sort {
    pub column: Column,
    pub descending: bool,
}

/// Text compared without regard to case. Empty sorts last either way, so
/// untagged tracks gather at the bottom rather than the top.
fn text(a: &str, b: &str) -> Ordering {
    match (a.is_empty(), b.is_empty()) {
        (true, false) => Ordering::Greater,
        (false, true) => Ordering::Less,
        _ => a.to_lowercase().cmp(&b.to_lowercase()),
    }
}

/// Album order within an artist: album, disc, track, then title.
fn album_order(a: &Track, b: &Track) -> Ordering {
    text(&a.info.album, &b.info.album)
        .then(a.info.disc_no.cmp(&b.info.disc_no))
        .then(a.info.track_no.cmp(&b.info.track_no))
        .then(text(&a.info.title, &b.info.title))
}

fn compare(column: Column, a: &Track, b: &Track) -> Ordering {
    let (x, y) = (&a.info, &b.info);
    match column {
        Column::Artist => text(&x.artist, &y.artist).then_with(|| album_order(a, b)),
        Column::Title => text(&x.title, &y.title),
        Column::Album => album_order(a, b),
        Column::Length => x
            .duration_secs
            .partial_cmp(&y.duration_secs)
            .unwrap_or(Ordering::Equal),
        Column::TrackNo => x.track_no.cmp(&y.track_no),
        Column::Genre => text(&x.genre, &y.genre),
        Column::Year => x.year.cmp(&y.year),
        Column::Rating => a.rating.cmp(&b.rating),
        Column::PlayCount => a.play_count.cmp(&b.play_count),
        Column::LastPlayed => a.last_played.cmp(&b.last_played),
        Column::Added => a.added.cmp(&b.added),
        Column::Bitrate => x.kbps.cmp(&y.kbps),
        Column::Path => x.path.cmp(&y.path),
    }
}

/// The search box's text as terms: words, or "quoted phrases", lowercased.
pub fn terms(search: &str) -> Vec<String> {
    let mut out = Vec::new();
    // Splitting on quotes alternates outside, inside, outside...
    for (i, part) in search.split('"').enumerate() {
        if i % 2 == 1 {
            let phrase = part.trim();
            if !phrase.is_empty() {
                out.push(phrase.to_lowercase());
            }
        } else {
            out.extend(part.split_whitespace().map(str::to_lowercase));
        }
    }
    out
}

/// Whether every term turns up somewhere in the track: its title, artist,
/// album, album artist, genre, year or file name.
pub fn matches(t: &Track, terms: &[String]) -> bool {
    if terms.is_empty() {
        return true;
    }
    let i = &t.info;
    let file = i
        .path
        .file_name()
        .map(|f| f.to_string_lossy())
        .unwrap_or_default();
    let year = i.year.map(|y| y.to_string()).unwrap_or_default();
    let hay = [
        i.title.as_str(),
        &i.artist,
        &i.album,
        &i.album_artist,
        &i.genre,
        &year,
        &file,
    ]
    .join("\n")
    .to_lowercase();
    terms.iter().all(|term| hay.contains(term.as_str()))
}

/// The tracks a view shows for `search`, in `sort` order: their indices
/// into `tracks`. Ties keep the order they had.
pub fn select(tracks: &[Track], view: View, search: &str, sort: Sort, now: i64) -> Vec<usize> {
    let terms = terms(search);
    let mut rows: Vec<usize> = (0..tracks.len())
        .filter(|&i| view.includes(&tracks[i], now) && matches(&tracks[i], &terms))
        .collect();
    rows.sort_by(|&a, &b| {
        let o = compare(sort.column, &tracks[a], &tracks[b]);
        if sort.descending { o.reverse() } else { o }
    });
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TrackInfo;

    fn track(artist: &str, album: &str, n: u32, title: &str) -> Track {
        Track {
            info: TrackInfo {
                path: format!("/m/{artist}/{title}.mp3").into(),
                title: title.into(),
                artist: artist.into(),
                album: album.into(),
                track_no: Some(n),
                ..TrackInfo::default()
            },
            ..Track::default()
        }
    }

    fn titles(tracks: &[Track], rows: &[usize]) -> Vec<String> {
        rows.iter().map(|&i| tracks[i].info.title.clone()).collect()
    }

    #[test]
    fn audio_sorts_by_artist_then_album_order() {
        let tracks = vec![
            track("Sleater-Kinney", "The Hot Rock", 2, "Hot Rock"),
            track("", "", 0, "Untagged"),
            track("sleater-kinney", "The Hot Rock", 1, "Start Together"),
            track("Bikini Kill", "Pussy Whipped", 1, "Blood One"),
        ];
        let rows = select(&tracks, View::Audio, "", View::Audio.default_sort(), 0);
        assert_eq!(
            titles(&tracks, &rows),
            ["Blood One", "Start Together", "Hot Rock", "Untagged"]
        );
    }

    #[test]
    fn smart_views_filter_on_history() {
        let now = 100 * 24 * 3600;
        let mut a = track("A", "", 1, "played lots");
        a.play_count = 9;
        a.last_played = Some(now - 3600);
        a.rating = 5;
        a.added = 0;
        let mut b = track("B", "", 1, "played once");
        b.play_count = 1;
        b.last_played = Some(now - 90 * 24 * 3600);
        b.added = now - 60;
        let c = track("C", "", 1, "never");
        let tracks = vec![a, b, c];
        let show = |v: View| titles(&tracks, &select(&tracks, v, "", v.default_sort(), now));
        assert_eq!(show(View::MostPlayed), ["played lots", "played once"]);
        assert_eq!(show(View::RecentlyPlayed), ["played lots"]);
        assert_eq!(show(View::NeverPlayed), ["never"]);
        assert_eq!(show(View::TopRated), ["played lots"]);
        assert_eq!(show(View::RecentlyAdded), ["played once"]);
    }

    #[test]
    fn search_needs_every_term_anywhere() {
        let tracks = vec![
            track("Sleater-Kinney", "The Hot Rock", 2, "Hot Rock"),
            track("Sleater-Kinney", "Dig Me Out", 1, "Dig Me Out"),
            track("The Rock", "Hot", 1, "Other"),
        ];
        let sort = View::Audio.default_sort();
        let find = |s: &str| titles(&tracks, &select(&tracks, View::Audio, s, sort, 0));
        assert_eq!(find("sleater hot"), ["Hot Rock"]);
        assert_eq!(find("\"hot rock\""), ["Hot Rock"]);
        assert_eq!(find("ROCK"), ["Hot Rock", "Other"]);
        assert_eq!(find("  "), ["Dig Me Out", "Hot Rock", "Other"]);
    }

    #[test]
    fn terms_keep_quoted_phrases_whole() {
        assert_eq!(terms(r#"a "b c" d"#), ["a", "b c", "d"]);
        assert_eq!(terms(r#""unclosed phrase"#), ["unclosed phrase"]);
    }

    #[test]
    fn descending_reverses() {
        let tracks = vec![track("A", "", 1, "x"), track("B", "", 1, "y")];
        let sort = Sort {
            column: Column::Artist,
            descending: true,
        };
        assert_eq!(
            titles(&tracks, &select(&tracks, View::Audio, "", sort, 0)),
            ["y", "x"]
        );
    }
}
