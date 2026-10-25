//! The media library window's insides: the tree, the search box, the track
//! list and its buttons — what each click and key does, and the view drawn
//! from it.
//!
//! As in Winamp: the tree picks what the list shows; typing goes to the
//! search box, narrowing the list as you type; a column header sorts by it,
//! again to reverse; double-click or Enter plays the selection (or the whole
//! list, if nothing is selected) in place of the playlist, and Enqueue adds
//! it to the end. Tracks drag out of the list onto the playlist, landing
//! where they are dropped, or onto the main window, joining the end. The
//! divider between tree and list drags; so do the headers' edges, to resize
//! a column, and the headers themselves, to move one.
//!
//! The Audio view has Winamp's filter panes above the list: its artists,
//! and the albums of the artist picked, each with an "All" line on top.
//! Picking one narrows what is below it; double-clicking plays it.

use std::collections::{HashMap, HashSet};
use std::time::Instant;

use lantern_library::Track;
use lantern_library::view::{self, Column, Sort, View};
use lantern_playlist::Entry;
use winit::event::{KeyEvent, MouseScrollDelta};
use winit::keyboard::{Key, NamedKey};

use super::text_edit::{Edit, TextEdit};
use super::{App, DOUBLE_CLICK, PaneKind};
use crate::ui::WidgetId;
use crate::ui::gen_controls::{ARROW, ScrollPart, ScrollState, scroll_geometry, scroll_hit};
use crate::ui::library_window::{
    self as lw, ALL_COLUMNS, Columns, FilterRow, FilterView, Layout, LibraryView, ListRow,
    MIN_COLUMN_WIDTH, ROW_HEIGHT, TreeRow, clamp_tree_width, columns_width, default_columns,
};

/// A press moves this far before it becomes a drag.
const DRAG_START: i32 = 4;
/// A column's edge can be taken this far either side of it.
const EDGE_GRAB: i32 = 3;
use crate::ui::playlist_window::clock;

/// Where dragged tracks land.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Drop {
    /// One of Lantern's playlists in the tree: at its end.
    LibraryPlaylist(i64),
    /// The playlist window, before this row.
    Playlist(usize),
    /// The main window or the equaliser: the playlist's end.
    End,
}

/// What the tree has selected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Node {
    /// The Local Media heading: shows all audio, as Audio does.
    LocalMedia,
    View(View),
    /// The Playlists heading: shows nothing until a playlist is picked.
    Playlists,
    Playlist(i64),
}

/// A scrollbar held down: which, and on what part. For the thumb, how far
/// into it the pointer took hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ScrollHold {
    id: WidgetId,
    part: ScrollPart,
    grab: i32,
    /// When an arrow or the track next steps again; `None` for the thumb.
    next: Option<Instant>,
}

/// Held down, an arrow or the track waits this long, then repeats this
/// often — about Windows' own pace.
const REPEAT_DELAY: std::time::Duration = std::time::Duration::from_millis(400);
const REPEAT_EVERY: std::time::Duration = std::time::Duration::from_millis(50);

/// A header held down: on a column's edge, resizing it; or on the
/// column, to sort by it on release or, once it moves, to move it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HeaderHold {
    Resize {
        /// Position among the shown columns.
        column: usize,
        start_x: i32,
        start_width: u32,
    },
    Press {
        column: usize,
        start_x: i32,
        moved: bool,
    },
}

pub(crate) struct LibraryUi {
    node: Node,
    /// The tree's width, as the divider was last left.
    pub(super) tree_width: u32,
    pub(super) columns: Columns,
    header: Option<HeaderHold>,
    /// Where the pointer went down in the list, and whether it has since
    /// moved far enough to be dragging tracks.
    list_drag: Option<((i32, i32), bool)>,
    /// While the divider is held: the pointer's offset from it.
    divider_grab: Option<i32>,
    local_open: bool,
    playlists_open: bool,
    tree_scroll: usize,
    search: TextEdit,
    /// A playlist of Lantern's own being renamed in the tree: its id, and
    /// the name so far.
    renaming: Option<(i64, TextEdit)>,
    /// `None`: the view's own order, or a playlist's.
    sort: Option<Sort>,
    /// Indices into the library's tracks, in list order.
    rows: Vec<usize>,
    /// The library generation `rows` was built from.
    built: Option<u64>,
    list_scroll: usize,
    columns_offset: u32,
    /// Track ids.
    selected: HashSet<i64>,
    /// The row a Shift-click extends from.
    anchor: Option<usize>,
    last_click: Option<(Instant, usize)>,
    hold: Option<ScrollHold>,
    /// Where a held header is, while it is dragged.
    drag_x: i32,
    /// What each filter pane, Artist then Album, has picked: `None` for
    /// All.
    filter: [Option<String>; 2],
    /// Each pane's lines below All: a name ("" for none tagged) and its
    /// tracks.
    filter_rows: [Vec<(String, usize)>; 2],
    /// Each pane's "All" count: the tracks it covers.
    filter_totals: [usize; 2],
    filter_scroll: [usize; 2],
    /// For a double-click in a pane: when, which pane, which line.
    filter_click: Option<(Instant, usize, usize)>,
    /// The last press in the search box, for a double-click.
    search_click: Option<Instant>,
    /// What an input method is composing, not yet typed.
    preedit: Option<String>,
    /// While tracks are dragged over a playlist in the tree: that one.
    drop_node: Option<Node>,
    /// Under the Playlists heading: the playlists listed, as their ids,
    /// and the one picked.
    listed: Vec<i64>,
    listed_pick: Option<i64>,
}

impl Default for LibraryUi {
    fn default() -> Self {
        Self {
            node: Node::View(View::Audio),
            tree_width: lw::DEFAULT_TREE_WIDTH,
            columns: default_columns(),
            header: None,
            list_drag: None,
            divider_grab: None,
            local_open: true,
            playlists_open: true,
            tree_scroll: 0,
            search: TextEdit::default(),
            renaming: None,
            sort: None,
            rows: Vec::new(),
            built: None,
            list_scroll: 0,
            columns_offset: 0,
            selected: HashSet::new(),
            anchor: None,
            last_click: None,
            hold: None,
            drag_x: 0,
            filter: [None, None],
            filter_rows: [Vec::new(), Vec::new()],
            filter_totals: [0, 0],
            filter_scroll: [0, 0],
            filter_click: None,
            search_click: None,
            preedit: None,
            drop_node: None,
            listed: Vec::new(),
            listed_pick: None,
        }
    }
}

impl LibraryUi {
    /// Lines in the list: tracks, or under the Playlists heading, playlists.
    fn list_len(&self) -> usize {
        if self.node == Node::Playlists {
            self.listed.len()
        } else {
            self.rows.len()
        }
    }

    /// Whether the view has the Artist and Album panes: all audio does.
    fn has_filters(&self) -> bool {
        matches!(self.node, Node::LocalMedia | Node::View(View::Audio))
    }
}

/// What a filter pane says for a name left untagged.
const UNKNOWN: [&str; 2] = ["(Unknown Artist)", "(Unknown Album)"];

/// The distinct values of `key` among `rows`, with how many rows have each,
/// sorted without regard to case, the untagged last.
fn distinct(
    tracks: &[Track],
    rows: &[usize],
    key: impl Fn(&Track) -> &str,
) -> Vec<(String, usize)> {
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for &i in rows {
        *counts.entry(key(&tracks[i])).or_default() += 1;
    }
    let mut out: Vec<(String, usize)> = counts
        .into_iter()
        .map(|(k, n)| (k.to_string(), n))
        .collect();
    out.sort_by(|a, b| {
        (a.0.is_empty(), a.0.to_lowercase()).cmp(&(b.0.is_empty(), b.0.to_lowercase()))
    });
    out
}

fn artist(t: &Track) -> &str {
    &t.info.artist
}

fn album(t: &Track) -> &str {
    &t.info.album
}

/// `secs` since the epoch as a date, yyyy-mm-dd (UTC).
fn date(secs: i64) -> String {
    // Howard Hinnant's days-to-civil.
    let z = secs.div_euclid(86_400) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

fn cell(t: &Track, column: Column) -> String {
    let i = &t.info;
    let num = |n: Option<u32>| n.map(|n| n.to_string()).unwrap_or_default();
    match column {
        Column::Artist => i.artist.clone(),
        Column::Title => i.title.clone(),
        Column::Album => i.album.clone(),
        Column::Length => i.duration_secs.map(clock).unwrap_or_default(),
        Column::TrackNo => num(i.track_no),
        Column::Genre => i.genre.clone(),
        Column::Year => num(i.year),
        Column::Rating => "*".repeat(usize::from(t.rating)),
        Column::PlayCount => t.play_count.to_string(),
        Column::LastPlayed => t.last_played.map(date).unwrap_or_default(),
        Column::Added => date(t.added),
        Column::Bitrate => num(i.kbps),
        Column::Path => i.path.display().to_string(),
    }
}

/// The playlist line for a library track: "Artist - Title", as Winamp
/// shows it.
fn entry(t: &Track) -> Entry {
    let mut e = Entry::new(t.info.path.clone());
    e.title = Some(if t.info.artist.is_empty() {
        t.info.title.clone()
    } else {
        format!("{} - {}", t.info.artist, t.info.title)
    });
    e.duration = t.info.duration_secs;
    e.tags_loaded = true;
    e
}

/// How a scroll offset reads on its bar: (position, visible).
fn bar(offset: usize, total: usize, shown: usize) -> (f32, f32) {
    let max = total.saturating_sub(shown);
    let position = if max == 0 {
        0.0
    } else {
        offset as f32 / max as f32
    };
    let visible = if total == 0 {
        1.0
    } else {
        shown as f32 / total as f32
    };
    (position, visible)
}

/// The columns as settings keep them: each one's name and width.
pub(super) fn columns_to_settings(columns: &[(usize, u32)]) -> Vec<(String, u32)> {
    columns
        .iter()
        .map(|&(i, w)| (ALL_COLUMNS[i].1.to_string(), w))
        .collect()
}

/// Columns from settings, skipping names this version does not know; the
/// defaults if none are left.
pub(super) fn columns_from_settings(saved: &[(String, u32)]) -> Columns {
    let mut out: Columns = Vec::new();
    for (key, w) in saved {
        if let Some(i) = ALL_COLUMNS.iter().position(|c| c.1 == key)
            && !out.iter().any(|c| c.0 == i)
        {
            out.push((i, (*w).max(MIN_COLUMN_WIDTH)));
        }
    }
    if out.is_empty() {
        default_columns()
    } else {
        out
    }
}

impl App {
    /// The library window's layout, as it stands.
    pub(super) fn lib_layout(&self) -> Layout {
        let (w, h) = self.library_size;
        lw::layout(w, h, self.lib_ui.tree_width, self.lib_ui.has_filters())
    }

    /// Whole rows in view: in the tree, and in the list.
    fn lib_visible(&self) -> (usize, usize) {
        let (w, h) = self.library_size;
        lw::visible_rows(w, h, self.lib_ui.tree_width, self.lib_ui.has_filters())
    }

    /// Whole rows in view in the filter panes; none without them.
    fn filter_visible(&self) -> usize {
        self.lib_layout()
            .filters
            .map_or(0, |f| (f.lists[0].h / ROW_HEIGHT) as usize)
    }

    /// The library window's widgets, as its layout stands.
    pub(super) fn library_widgets(&self) -> Vec<crate::ui::Widget> {
        let (w, h) = self.library_size;
        lw::widgets(w, h, self.lib_ui.tree_width, self.lib_ui.has_filters())
    }

    /// The tree's lines, top to bottom, with the node each selects.
    fn tree_items(&self) -> Vec<(Node, TreeRow)> {
        let ui = &self.lib_ui;
        let row = |node: Node, label: &str, depth: u8, expander: Option<bool>| {
            // A playlist being renamed shows its name so far, and the caret.
            let renaming = match (node, &ui.renaming) {
                (Node::Playlist(id), Some((r, name))) if id == *r => Some(name),
                _ => None,
            };
            (
                node,
                TreeRow {
                    label: renaming.map_or_else(|| label.to_string(), |n| n.text.clone()),
                    depth,
                    expander,
                    selected: ui.node == node || ui.drop_node == Some(node),
                    editing: renaming.map(|n| n.caret),
                },
            )
        };
        let mut items = vec![row(Node::LocalMedia, "Local Media", 0, Some(ui.local_open))];
        if ui.local_open {
            for v in View::ALL {
                items.push(row(Node::View(v), v.name(), 1, None));
            }
        }
        let lists = &self.library.playlists;
        let expander = (!lists.is_empty()).then_some(ui.playlists_open);
        items.push(row(Node::Playlists, "Playlists", 0, expander));
        if ui.playlists_open {
            for p in lists {
                items.push(row(Node::Playlist(p.id), &p.name, 1, None));
            }
        }
        items
    }

    /// Rebuild the list if the library or the query changed.
    fn refresh_library_rows(&mut self) {
        let generation = self.library.generation;
        if self.lib_ui.built == Some(generation) {
            return;
        }
        let tracks = &self.library.tracks;
        let ui = &self.lib_ui;
        let now = lantern_library::now();
        let rows = match ui.node {
            Node::LocalMedia | Node::View(_) => {
                let v = match ui.node {
                    Node::View(v) => v,
                    _ => View::Audio,
                };
                let sort = ui.sort.unwrap_or_else(|| v.default_sort());
                let rows = view::select(tracks, v, &ui.search.text, sort, now);
                if ui.has_filters() {
                    self.apply_filters(rows)
                } else {
                    rows
                }
            }
            Node::Playlists => {
                // The list lists the playlists: those whose names match.
                let terms = view::terms(&ui.search.text);
                let listed: Vec<i64> = self
                    .library
                    .playlists
                    .iter()
                    .filter(|p| {
                        let name = p.name.to_lowercase();
                        terms.iter().all(|t| name.contains(t.as_str()))
                    })
                    .map(|p| p.id)
                    .collect();
                self.lib_ui.listed = listed;
                Vec::new()
            }
            Node::Playlist(id) => {
                let index: HashMap<i64, usize> =
                    tracks.iter().enumerate().map(|(i, t)| (t.id, i)).collect();
                let terms = view::terms(&ui.search.text);
                let mut rows: Vec<usize> = self
                    .library
                    .playlists
                    .iter()
                    .find(|p| p.id == id)
                    .map(|p| {
                        p.tracks
                            .iter()
                            .filter_map(|t| index.get(t).copied())
                            .collect()
                    })
                    .unwrap_or_default();
                rows.retain(|&i| view::matches(&tracks[i], &terms));
                if let Some(sort) = ui.sort {
                    let order = view::select(tracks, View::Audio, "", sort, now);
                    let rank: HashMap<usize, usize> =
                        order.into_iter().enumerate().map(|(r, i)| (i, r)).collect();
                    rows.sort_by_key(|i| rank.get(i).copied().unwrap_or(usize::MAX));
                }
                rows
            }
        };
        let (_, shown) = self.lib_visible();
        let ui = &mut self.lib_ui;
        ui.rows = rows;
        ui.built = Some(generation);
        ui.list_scroll = ui.list_scroll.min(ui.rows.len().saturating_sub(shown));
    }

    /// Fill the filter panes from `rows`, and narrow `rows` to what they
    /// pick. A pick no longer among them — after a search, say — goes back
    /// to All.
    fn apply_filters(&mut self, rows: Vec<usize>) -> Vec<usize> {
        let tracks = &self.library.tracks;
        let ui = &mut self.lib_ui;
        let artists = distinct(tracks, &rows, artist);
        if ui.filter[0]
            .as_ref()
            .is_some_and(|a| !artists.iter().any(|(n, _)| n == a))
        {
            ui.filter[0] = None;
        }
        let by_artist: Vec<usize> = match &ui.filter[0] {
            Some(a) => rows
                .iter()
                .copied()
                .filter(|&i| artist(&tracks[i]) == a)
                .collect(),
            None => rows.clone(),
        };
        let albums = distinct(tracks, &by_artist, album);
        if ui.filter[1]
            .as_ref()
            .is_some_and(|a| !albums.iter().any(|(n, _)| n == a))
        {
            ui.filter[1] = None;
        }
        let out: Vec<usize> = match &ui.filter[1] {
            Some(a) => by_artist
                .iter()
                .copied()
                .filter(|&i| album(&tracks[i]) == a)
                .collect(),
            None => by_artist.clone(),
        };
        ui.filter_totals = [rows.len(), by_artist.len()];
        ui.filter_rows = [artists, albums];
        for p in 0..2 {
            let n = ui.filter_rows[p].len() + 1;
            ui.filter_scroll[p] = ui.filter_scroll[p].min(n.saturating_sub(1));
        }
        out
    }

    /// The query changed: rebuild from the top, nothing selected.
    fn requery(&mut self) {
        let ui = &mut self.lib_ui;
        ui.built = None;
        ui.list_scroll = 0;
        ui.selected.clear();
        ui.anchor = None;
        self.library.clear_message();
        self.request_redraw(PaneKind::Library);
    }

    pub(super) fn library_view(&mut self) -> LibraryView {
        self.refresh_library_rows();
        let (w, h) = self.library_size;
        let (tree_shown, list_shown) = self.lib_visible();
        let focused = self.pane(PaneKind::Library).is_some_and(|p| p.focused);
        let items = self.tree_items();
        let ui = &self.lib_ui;
        let tracks = &self.library.tracks;
        let hold = |id| ui.hold.filter(|h| h.id == id).map(|h| h.part);

        let (position, visible) = bar(ui.tree_scroll, items.len(), tree_shown);
        let tree_scroll = ScrollState {
            position,
            visible,
            pressed: hold(lw::TREE_SCROLL),
        };
        let (position, visible) = bar(ui.list_scroll, ui.list_len(), list_shown);
        let list_scroll = ScrollState {
            position,
            visible,
            pressed: hold(lw::LIST_SCROLL),
        };
        let list_w = self.lib_layout().list.w as usize;
        let total_w = columns_width(&ui.columns) as usize;
        let (position, visible) = bar(ui.columns_offset as usize, total_w, list_w);
        let list_hscroll = ScrollState {
            position,
            visible,
            pressed: hold(lw::LIST_HSCROLL),
        };

        let rows = ui
            .rows
            .iter()
            .skip(ui.list_scroll)
            .take(list_shown)
            .map(|&i| {
                let t = &tracks[i];
                ListRow {
                    cells: ui
                        .columns
                        .iter()
                        .map(|&(c, _)| cell(t, ALL_COLUMNS[c].0))
                        .collect(),
                    selected: ui.selected.contains(&t.id),
                }
            })
            .collect();
        let listing = ui.node == Node::Playlists;
        let rows = if listing {
            self.playlist_listing_rows(list_shown)
        } else {
            rows
        };
        let total: f64 = ui
            .rows
            .iter()
            .filter_map(|&i| tracks[i].info.duration_secs)
            .sum();
        let status = match self.library.activity() {
            Some(busy) => busy,
            None if self.library.tracks.is_empty() => {
                "Add a folder, or import Apple Music, from the Library button".into()
            }
            None if listing => {
                let n = ui.listed.len();
                format!("{n} {}", if n == 1 { "playlist" } else { "playlists" })
            }
            None => {
                let n = ui.rows.len();
                format!(
                    "{n} {} [{}]",
                    if n == 1 { "item" } else { "items" },
                    clock(total)
                )
            }
        };
        // Text an input method is composing shows at the caret, underlined.
        let (search_shown, caret_shown, search_marked) = match &ui.preedit {
            Some(marked) if ui.renaming.is_none() => {
                let mut chars: Vec<char> = ui.search.text.chars().collect();
                let at = ui.search.caret.min(chars.len());
                let n = marked.chars().count();
                chars.splice(at..at, marked.chars());
                (chars.into_iter().collect(), at + n, Some((at, at + n)))
            }
            _ => (ui.search.text.clone(), ui.search.caret, None),
        };
        let sort = match ui.node {
            Node::View(v) => Some(ui.sort.unwrap_or_else(|| v.default_sort())),
            Node::LocalMedia => Some(ui.sort.unwrap_or_else(|| View::Audio.default_sort())),
            _ => ui.sort,
        };
        LibraryView {
            focused,
            width: w,
            height: h,
            tree: items
                .into_iter()
                .skip(ui.tree_scroll)
                .take(tree_shown)
                .map(|(_, r)| r)
                .collect(),
            tree_scroll,
            search: search_shown,
            caret: (focused && ui.renaming.is_none()).then_some(caret_shown),
            search_selection: ui.search.selection(),
            search_marked,
            sort,
            rows,
            list_scroll,
            list_hscroll,
            columns_offset: ui.columns_offset,
            columns: ui.columns.clone(),
            tree_width: ui.tree_width,
            column_drop: self.column_drop_x(),
            filters: self.filter_view(),
            listing_playlists: listing,
            status,
        }
    }

    /// The filter panes' lines in view, if the view has them and they fit.
    fn filter_view(&self) -> Option<FilterView> {
        let ui = &self.lib_ui;
        if !ui.has_filters() || self.lib_layout().filters.is_none() {
            return None;
        }
        let shown = self.filter_visible();
        let hold = |id| ui.hold.filter(|h| h.id == id).map(|h| h.part);
        let pane = |p: usize| {
            let what = ["artists", "albums"][p];
            let names = &ui.filter_rows[p];
            let all = FilterRow {
                label: format!("All ({} {what})", names.len()),
                count: ui.filter_totals[p],
                selected: ui.filter[p].is_none(),
            };
            let lines = std::iter::once(all).chain(names.iter().map(|(n, count)| FilterRow {
                label: if n.is_empty() {
                    UNKNOWN[p].to_string()
                } else {
                    n.clone()
                },
                count: *count,
                selected: ui.filter[p].as_ref() == Some(n),
            }));
            let rows: Vec<FilterRow> = lines.skip(ui.filter_scroll[p]).take(shown).collect();
            let (position, visible) = bar(ui.filter_scroll[p], names.len() + 1, shown);
            let scroll = ScrollState {
                position,
                visible,
                pressed: hold(lw::FILTER_SCROLLS[p]),
            };
            (rows, scroll)
        };
        let (a, b) = (pane(0), pane(1));
        Some(FilterView {
            rows: [a.0, b.0],
            scrolls: [a.1, b.1],
        })
    }

    /// The playlists listed under the Playlists heading, the rows in view:
    /// name, how many tracks, how long, and where from.
    fn playlist_listing_rows(&self, shown: usize) -> Vec<ListRow> {
        let ui = &self.lib_ui;
        let length: HashMap<i64, f64> = self
            .library
            .tracks
            .iter()
            .map(|t| (t.id, t.info.duration_secs.unwrap_or(0.0)))
            .collect();
        ui.listed
            .iter()
            .skip(ui.list_scroll)
            .take(shown)
            .filter_map(|&id| self.library.playlist(id))
            .map(|p| {
                let secs: f64 = p.tracks.iter().filter_map(|t| length.get(t)).sum();
                let from = if p.source == lantern_library::OWN {
                    "Lantern"
                } else {
                    "Apple Music"
                };
                ListRow {
                    cells: vec![
                        p.name.clone(),
                        p.tracks.len().to_string(),
                        clock(secs),
                        from.to_string(),
                    ],
                    selected: ui.listed_pick == Some(p.id),
                }
            })
            .collect()
    }

    /// A click on a listed playlist: pick it; a double-click opens it.
    fn click_listed_playlist(&mut self, local_y: i32) {
        let row = (local_y / ROW_HEIGHT as i32).max(0) as usize + self.lib_ui.list_scroll;
        let Some(&id) = self.lib_ui.listed.get(row) else {
            self.lib_ui.listed_pick = None;
            return;
        };
        let now = Instant::now();
        let double = self
            .lib_ui
            .last_click
            .is_some_and(|(t, r)| r == row && now.duration_since(t) < DOUBLE_CLICK);
        self.lib_ui.last_click = Some((now, row));
        self.lib_ui.listed_pick = Some(id);
        if double {
            self.show_library_playlist(id);
        }
    }

    // -- Pointer ------------------------------------------------------------

    /// The pointer went down on `id` at logical `(x, y)`.
    pub(super) fn press_library(&mut self, id: WidgetId, x: i32, y: i32) {
        // A click anywhere settles a rename under way.
        if let Some((list, name)) = self.lib_ui.renaming.take() {
            self.finish_rename(list, &name.text);
        }
        let l = self.lib_layout();
        match id {
            lw::TREE => {
                let i =
                    ((y - l.tree.y) / ROW_HEIGHT as i32).max(0) as usize + self.lib_ui.tree_scroll;
                let items = self.tree_items();
                let Some((node, row)) = items.get(i) else {
                    return;
                };
                let toggle_x = l.tree.x + 2 + i32::from(row.depth) * 11 + 8;
                if row.expander.is_some() && x < toggle_x {
                    match node {
                        Node::LocalMedia => self.lib_ui.local_open ^= true,
                        _ => self.lib_ui.playlists_open ^= true,
                    }
                    let n = self.tree_items().len();
                    let shown = self.lib_visible().0;
                    self.lib_ui.tree_scroll = self.lib_ui.tree_scroll.min(n.saturating_sub(shown));
                } else if self.lib_ui.node != *node {
                    self.lib_ui.node = *node;
                    self.lib_ui.sort = None;
                    self.lib_ui.filter = [None, None];
                    self.lib_ui.filter_scroll = [0, 0];
                    self.requery();
                    // The panes come and go with the view.
                    self.relayout_library();
                }
            }
            lw::SEARCH => {
                // Click: the caret; Shift-click: select to there;
                // double-click: select it all.
                let at = self.caret_at(x - l.search.x - 2);
                let now = Instant::now();
                let double = self
                    .lib_ui
                    .search_click
                    .is_some_and(|t| now.duration_since(t) < DOUBLE_CLICK);
                self.lib_ui.search_click = Some(now);
                if double {
                    self.lib_ui.search.select_all();
                } else {
                    self.lib_ui.search.move_to(at, self.modifiers.shift_key());
                }
            }
            // The playlists' own columns stay as they are.
            lw::HEADER if self.lib_ui.node == Node::Playlists => {}
            lw::HEADER => {
                // On an edge it resizes the column to its left; elsewhere it
                // takes hold of the column.
                let mut right = l.header.x - self.lib_ui.columns_offset as i32;
                let mut hold = None;
                for (n, &(_, width)) in self.lib_ui.columns.iter().enumerate() {
                    let left = right;
                    right += width as i32;
                    if (x - right).abs() <= EDGE_GRAB {
                        hold = Some(HeaderHold::Resize {
                            column: n,
                            start_x: x,
                            start_width: width,
                        });
                        break;
                    }
                    if x >= left && x < right {
                        hold = Some(HeaderHold::Press {
                            column: n,
                            start_x: x,
                            moved: false,
                        });
                        break;
                    }
                }
                self.lib_ui.header = hold;
            }
            lw::LIST if self.lib_ui.node == Node::Playlists => {
                self.click_listed_playlist(y - l.list.y);
            }
            lw::LIST => {
                if self.click_library_stars(x, y - l.list.y) {
                    return;
                }
                if !self.click_library_row(y - l.list.y) {
                    self.lib_ui.list_drag = Some(((x, y), false));
                }
            }
            lw::DIVIDER => self.lib_ui.divider_grab = Some(x - l.divider.x),
            lw::ARTISTS | lw::ALBUMS => {
                let pane = usize::from(id == lw::ALBUMS);
                let Some(f) = l.filters else { return };
                let line = ((y - f.lists[pane].y) / ROW_HEIGHT as i32).max(0) as usize
                    + self.lib_ui.filter_scroll[pane];
                self.click_filter(pane, line);
            }
            _ => {}
        }
        self.request_redraw(PaneKind::Library);
    }

    /// A click on line `line` of filter pane `pane`: 0 is All. Picking an
    /// artist starts its albums from All. A double-click plays what the
    /// list then holds.
    fn click_filter(&mut self, pane: usize, line: usize) {
        let ui = &mut self.lib_ui;
        if line > ui.filter_rows[pane].len() {
            return;
        }
        let pick = line
            .checked_sub(1)
            .map(|n| ui.filter_rows[pane][n].0.clone());
        if ui.filter[pane] != pick {
            ui.filter[pane] = pick;
            if pane == 0 {
                ui.filter[1] = None;
                ui.filter_scroll[1] = 0;
            }
            self.requery();
        }
        let now = Instant::now();
        let double = self.lib_ui.filter_click.is_some_and(|(t, p, l)| {
            p == pane && l == line && now.duration_since(t) < DOUBLE_CLICK
        });
        self.lib_ui.filter_click = Some((now, pane, line));
        if double {
            self.lib_ui.selected.clear();
            self.send_library_selection(true);
        }
    }

    fn sort_library_by(&mut self, column: Column) {
        let current = match self.lib_ui.node {
            Node::View(v) => Some(self.lib_ui.sort.unwrap_or_else(|| v.default_sort())),
            Node::LocalMedia => Some(
                self.lib_ui
                    .sort
                    .unwrap_or_else(|| View::Audio.default_sort()),
            ),
            _ => self.lib_ui.sort,
        };
        let descending = current.is_some_and(|s| s.column == column && !s.descending);
        self.lib_ui.sort = Some(Sort { column, descending });
        self.lib_ui.built = None;
    }

    fn caret_at(&self, local_x: i32) -> usize {
        self.font.as_ref().map_or(0, |font| {
            crate::ui::gen_controls::caret_at(font, &self.lib_ui.search.text, local_x)
        })
    }

    /// A click on the Rating column's stars: rates that track — or, if it
    /// already has just that many, clears its rating. True if it was one.
    fn click_library_stars(&mut self, x: i32, local_y: i32) -> bool {
        let l = self.lib_layout();
        let mut left = l.list.x - self.lib_ui.columns_offset as i32;
        let mut cell = None;
        for &(index, width) in &self.lib_ui.columns {
            if x >= left && x < left + width as i32 {
                cell = (ALL_COLUMNS[index].0 == Column::Rating).then_some(left);
                break;
            }
            left += width as i32;
        }
        let Some(left) = cell else { return false };
        let row = (local_y / ROW_HEIGHT as i32).max(0) as usize + self.lib_ui.list_scroll;
        let Some(&i) = self.lib_ui.rows.get(row) else {
            return false;
        };
        let track = &self.library.tracks[i];
        let stars = lw::star_at(x - left);
        let stars = if track.rating == stars { 0 } else { stars };
        let id = track.id;
        self.library.rate(&[id], stars);
        self.request_redraw(PaneKind::Library);
        true
    }

    /// A click in the list, `local_y` below its top. True if it was a
    /// double-click, which plays.
    fn click_library_row(&mut self, local_y: i32) -> bool {
        let row = (local_y / ROW_HEIGHT as i32).max(0) as usize + self.lib_ui.list_scroll;
        let ui = &mut self.lib_ui;
        let Some(&track) = ui.rows.get(row) else {
            ui.selected.clear();
            return false;
        };
        let id = self.library.tracks[track].id;
        let now = Instant::now();
        let double = ui
            .last_click
            .is_some_and(|(t, r)| r == row && now.duration_since(t) < DOUBLE_CLICK);
        ui.last_click = Some((now, row));
        if double {
            ui.selected = HashSet::from([id]);
            self.send_library_selection(true);
            return true;
        }
        // Pressing on a row already among several selected keeps them all,
        // so they can be dragged together.
        let plain = !self.modifiers.shift_key()
            && !self.modifiers.super_key()
            && !self.modifiers.control_key();
        if plain && ui.selected.contains(&id) {
            ui.anchor = Some(row);
            return false;
        }
        let ids: Vec<i64> = ui.rows.iter().map(|&i| self.library.tracks[i].id).collect();
        if self.modifiers.shift_key()
            && let Some(anchor) = ui.anchor
        {
            let (a, b) = (anchor.min(row), anchor.max(row));
            ui.selected = ids[a..=b].iter().copied().collect();
        } else if self.modifiers.super_key() || self.modifiers.control_key() {
            if !ui.selected.remove(&id) {
                ui.selected.insert(id);
            }
            ui.anchor = Some(row);
        } else {
            ui.selected = HashSet::from([id]);
            ui.anchor = Some(row);
        }
        false
    }

    /// A scrollbar, pressed or dragged at logical `(x, y)`.
    /// Something in the window held and dragged to logical `(x, y)`: a
    /// scrollbar, a header, the list, the divider.
    pub(super) fn drag_library_part(&mut self, id: WidgetId, x: i32, y: i32) {
        match id {
            lw::HEADER => self.drag_library_header(x),
            lw::LIST => self.drag_library_tracks(x, y),
            lw::SEARCH => {
                let x0 = self.lib_layout().search.x;
                let at = self.caret_at(x - x0 - 2);
                if at != self.lib_ui.search.caret {
                    self.lib_ui.search.move_to(at, true);
                    self.request_redraw(PaneKind::Library);
                }
            }
            lw::DIVIDER => {
                if let Some(grab) = self.lib_ui.divider_grab {
                    let c = lw::content_area(self.library_size.0, self.library_size.1);
                    let wanted = (x - grab - c.x - 1).max(0) as u32;
                    let width = clamp_tree_width(self.library_size.0, wanted);
                    if width != self.lib_ui.tree_width {
                        self.lib_ui.tree_width = width;
                        self.relayout_library();
                    }
                }
            }
            _ => self.drag_library_scroll(id, x, y),
        }
    }

    /// The window's widgets, rebuilt for a new layout, keeping hold of
    /// whatever is held.
    pub(super) fn relayout_library(&mut self) {
        let widgets = self.library_widgets();
        if let Some(p) = self.pane_mut(PaneKind::Library) {
            let held = p.input.active;
            p.widgets = widgets;
            if let Some(held) = held
                && let Some(widget) = p.widgets.iter_mut().find(|x| x.id == held)
            {
                widget.pressed = true;
            }
        }
        let max = self.max_columns_offset();
        self.lib_ui.columns_offset = self.lib_ui.columns_offset.min(max);
        self.request_redraw(PaneKind::Library);
    }

    fn max_columns_offset(&self) -> u32 {
        columns_width(&self.lib_ui.columns).saturating_sub(self.lib_layout().list.w)
    }

    fn drag_library_header(&mut self, x: i32) {
        match self.lib_ui.header {
            Some(HeaderHold::Resize {
                column,
                start_x,
                start_width,
            }) => {
                let width = (start_width as i32 + x - start_x).max(MIN_COLUMN_WIDTH as i32);
                if let Some(c) = self.lib_ui.columns.get_mut(column) {
                    c.1 = width as u32;
                }
            }
            Some(HeaderHold::Press {
                column,
                start_x,
                moved,
            }) if !moved && (x - start_x).abs() >= DRAG_START => {
                self.lib_ui.header = Some(HeaderHold::Press {
                    column,
                    start_x,
                    moved: true,
                });
            }
            _ => {}
        }
        self.lib_ui.drag_x = x;
        self.request_redraw(PaneKind::Library);
    }

    /// Where a header being moved would go: the position among the shown
    /// columns it would take, and the x of that boundary in the window.
    fn column_drop(&self) -> Option<(usize, i32)> {
        let Some(HeaderHold::Press { moved: true, .. }) = self.lib_ui.header else {
            return None;
        };
        let x = self.lib_ui.drag_x;
        let mut edge = self.lib_layout().header.x - self.lib_ui.columns_offset as i32;
        for (n, &(_, width)) in self.lib_ui.columns.iter().enumerate() {
            if x < edge + width as i32 / 2 {
                return Some((n, edge));
            }
            edge += width as i32;
        }
        Some((self.lib_ui.columns.len(), edge))
    }

    fn column_drop_x(&self) -> Option<i32> {
        self.column_drop().map(|(_, x)| x)
    }

    fn drag_library_tracks(&mut self, x: i32, y: i32) {
        let Some(((sx, sy), dragging)) = self.lib_ui.list_drag else {
            return;
        };
        if !dragging && ((x - sx).abs() >= DRAG_START || (y - sy).abs() >= DRAG_START) {
            self.lib_ui.list_drag = Some(((sx, sy), true));
            if let Some(p) = self.pane(PaneKind::Library) {
                p.window.set_cursor(winit::window::CursorIcon::Copy);
            }
        }
        if self.lib_ui.list_drag.is_some_and(|d| d.1) {
            self.mark_drop();
        }
    }

    /// Something in the window let go of.
    pub(super) fn release_library_part(&mut self, id: WidgetId) {
        match id {
            lw::HEADER => {
                match self.lib_ui.header {
                    Some(HeaderHold::Press {
                        column,
                        moved: false,
                        ..
                    }) => {
                        if let Some(&(index, _)) = self.lib_ui.columns.get(column) {
                            self.sort_library_by(ALL_COLUMNS[index].0);
                        }
                    }
                    Some(HeaderHold::Press { column, .. }) => {
                        if let Some((to, _)) = self.column_drop() {
                            let moving = self.lib_ui.columns.remove(column);
                            let to = if to > column { to - 1 } else { to };
                            self.lib_ui.columns.insert(to, moving);
                        }
                    }
                    _ => {}
                }
                self.lib_ui.header = None;
                let max = self.max_columns_offset();
                self.lib_ui.columns_offset = self.lib_ui.columns_offset.min(max);
            }
            lw::LIST => {
                if let Some((_, true)) = self.lib_ui.list_drag {
                    if let Some(p) = self.pane(PaneKind::Library) {
                        p.window.set_cursor(winit::window::CursorIcon::Default);
                    }
                    self.drop_library_tracks();
                }
                self.lib_ui.list_drag = None;
            }
            lw::DIVIDER => self.lib_ui.divider_grab = None,
            _ => self.release_library_scroll(),
        }
        self.request_redraw(PaneKind::Library);
    }

    /// Tracks dragged out of the list were let go: onto the playlist, they
    /// go in where they landed; onto the main window or the equaliser, at
    /// the end.
    /// Where dragged tracks would land if let go now.
    fn drop_target(&self) -> Option<Drop> {
        // Onto one of Lantern's playlists in the tree.
        if let Some((x, y)) = self.pane(PaneKind::Library).and_then(|p| p.input.cursor)
            && self.lib_layout().tree.contains(x, y)
        {
            return match self.tree_node_at(y) {
                Some(Node::Playlist(id)) if self.library.is_own(id) => {
                    Some(Drop::LibraryPlaylist(id))
                }
                _ => None,
            };
        }
        let (gx, gy) = self.global_cursor(PaneKind::Library)?;
        let (kinds, rects, shown) = self.frames();
        let i = (0..kinds.len()).find(|&i| {
            let r = rects[i];
            let inside = gx >= r.x && gx < r.x + r.w && gy >= r.y && gy < r.y + r.h;
            shown[i] && kinds[i] != PaneKind::Library && inside
        })?;
        let shaded = self.pane(PaneKind::Playlist).is_some_and(|p| p.shaded);
        Some(if kinds[i] == PaneKind::Playlist && !shaded {
            Drop::Playlist(self.playlist_row_at((gy - rects[i].y) as i32))
        } else {
            Drop::End
        })
    }

    /// Show where dragged tracks would land: a line in the playlist, or the
    /// playlist in the tree picked out.
    fn mark_drop(&mut self) {
        let target = self.drop_target();
        let row = match target {
            Some(Drop::Playlist(row)) => Some(row),
            _ => None,
        };
        let node = match target {
            Some(Drop::LibraryPlaylist(id)) => Some(Node::Playlist(id)),
            _ => None,
        };
        if row != self.pl_drop {
            self.pl_drop = row;
            self.request_redraw(PaneKind::Playlist);
        }
        if node != self.lib_ui.drop_node {
            self.lib_ui.drop_node = node;
            self.request_redraw(PaneKind::Library);
        }
    }

    /// Tracks dragged out of the list were let go.
    fn drop_library_tracks(&mut self) {
        let target = self.drop_target();
        self.pl_drop = None;
        self.lib_ui.drop_node = None;
        self.request_redraw(PaneKind::Playlist);
        match target {
            Some(Drop::LibraryPlaylist(id)) => self.send_selection_to_playlist(id),
            Some(Drop::Playlist(at)) => {
                let entries = self.library_selection_entries();
                self.insert_entries(at, entries);
            }
            Some(Drop::End) => {
                let entries = self.library_selection_entries();
                self.add_entries(entries);
            }
            None => {}
        }
    }

    /// A scrollbar's geometry and what it scrolls: its rect, whether it is
    /// vertical, how many lines (or pixels) there are and are shown, and
    /// the offset now.
    fn scroll_info(
        &self,
        id: WidgetId,
    ) -> Option<(crate::ui::geometry::Rect, bool, usize, usize, usize)> {
        let l = self.lib_layout();
        let (tree_shown, list_shown) = self.lib_visible();
        let ui = &self.lib_ui;
        let pane = usize::from(id == lw::ALBUMS_SCROLL);
        Some(match id {
            lw::TREE_SCROLL => (
                l.tree_scroll,
                true,
                self.tree_items().len(),
                tree_shown,
                ui.tree_scroll,
            ),
            lw::LIST_SCROLL => (
                l.list_scroll,
                true,
                ui.list_len(),
                list_shown,
                ui.list_scroll,
            ),
            lw::ARTISTS_SCROLL | lw::ALBUMS_SCROLL => {
                let f = l.filters?;
                let lines = ui.filter_rows[pane].len() + 1;
                (
                    f.scrolls[pane],
                    true,
                    lines,
                    self.filter_visible(),
                    ui.filter_scroll[pane],
                )
            }
            _ => (
                l.list_hscroll,
                false,
                columns_width(&ui.columns) as usize,
                l.list.w as usize,
                ui.columns_offset as usize,
            ),
        })
    }

    fn set_scroll(&mut self, id: WidgetId, to: usize) {
        let ui = &mut self.lib_ui;
        match id {
            lw::TREE_SCROLL => ui.tree_scroll = to,
            lw::LIST_SCROLL => ui.list_scroll = to,
            lw::ARTISTS_SCROLL => ui.filter_scroll[0] = to,
            lw::ALBUMS_SCROLL => ui.filter_scroll[1] = to,
            _ => ui.columns_offset = to as u32,
        }
        self.request_redraw(PaneKind::Library);
    }

    /// What part of scrollbar `id` the pointer at `(x, y)` is on, and the
    /// state it is drawn in.
    fn scroll_part_at(
        &self,
        id: WidgetId,
        x: i32,
        y: i32,
    ) -> Option<(ScrollPart, i32, ScrollState, u32)> {
        let (rect, vertical, total, shown, offset) = self.scroll_info(id)?;
        let (len, along) = if vertical {
            (rect.h, y - rect.y)
        } else {
            (rect.w, x - rect.x)
        };
        let (position, visible) = bar(offset, total, shown);
        let state = ScrollState {
            position,
            visible,
            pressed: None,
        };
        Some((scroll_hit(len, &state, along), along, state, len))
    }

    /// One step of an arrow or the track, as a press does and holding
    /// repeats.
    fn scroll_step(&mut self, id: WidgetId, part: ScrollPart) {
        let Some((_, vertical, total, shown, offset)) = self.scroll_info(id) else {
            return;
        };
        // Rows step by one; the columns, by a little more.
        let step = if vertical { 1 } else { 20 };
        let new = match part {
            ScrollPart::Back => offset.saturating_sub(step),
            ScrollPart::Forward => offset + step,
            ScrollPart::PageBack => offset.saturating_sub(shown),
            ScrollPart::PageForward => offset + shown,
            ScrollPart::Thumb => offset,
        };
        self.set_scroll(id, new.min(total.saturating_sub(shown)));
    }

    pub(super) fn drag_library_scroll(&mut self, id: WidgetId, x: i32, y: i32) {
        let Some((part, along, state, len)) = self.scroll_part_at(id, x, y) else {
            return;
        };
        match self.lib_ui.hold {
            None => {
                let grab = match scroll_geometry(len, &state).2 {
                    Some((at, _)) => along - at as i32,
                    None => 0,
                };
                let next = (part != ScrollPart::Thumb).then(|| Instant::now() + REPEAT_DELAY);
                self.lib_ui.hold = Some(ScrollHold {
                    id,
                    part,
                    grab,
                    next,
                });
                self.scroll_step(id, part);
            }
            Some(hold) if hold.part == ScrollPart::Thumb => {
                let Some((_, _, total, shown, _)) = self.scroll_info(id) else {
                    return;
                };
                let max = total.saturating_sub(shown);
                let (_, track, thumb) = scroll_geometry(len, &state);
                let thumb = thumb.map_or(0, |t| t.1);
                let travel = track.saturating_sub(thumb).max(1) as f32;
                let at = (along - hold.grab - ARROW as i32) as f32 / travel;
                self.set_scroll(id, (at.clamp(0.0, 1.0) * max as f32).round() as usize);
            }
            Some(_) => {}
        }
    }

    /// While an arrow or the track is held: step again when due, as long as
    /// the pointer is still on it — the track stops once the thumb reaches
    /// the pointer.
    pub(super) fn repeat_library_scroll(&mut self, now: Instant) {
        let Some(hold) = self.lib_ui.hold else { return };
        if hold.next.is_none_or(|t| now < t) {
            return;
        }
        let cursor = self.pane(PaneKind::Library).and_then(|p| p.input.cursor);
        let here = cursor.and_then(|(x, y)| self.scroll_part_at(hold.id, x, y));
        if here.is_some_and(|(part, ..)| part == hold.part) {
            self.scroll_step(hold.id, hold.part);
        }
        if let Some(h) = &mut self.lib_ui.hold {
            h.next = Some(now + REPEAT_EVERY);
        }
    }

    /// How soon a held scrollbar next repeats, if one is held.
    pub(super) fn library_repeat_due(&self) -> Option<Instant> {
        self.lib_ui.hold.and_then(|h| h.next)
    }

    pub(super) fn release_library_scroll(&mut self) {
        self.lib_ui.hold = None;
        self.request_redraw(PaneKind::Library);
    }

    pub(super) fn wheel_library(&mut self, delta: MouseScrollDelta) {
        let l = self.lib_layout();
        let (tree_shown, list_shown) = self.lib_visible();
        let columns_w = columns_width(&self.lib_ui.columns) as usize;
        let scale = self
            .pane(PaneKind::Library)
            .map_or(1.0, |p| p.window.scale_factor());
        let (dx, dy) = match delta {
            MouseScrollDelta::LineDelta(x, y) => (-x * 3.0 * ROW_HEIGHT as f32, -y * 3.0),
            MouseScrollDelta::PixelDelta(p) => (
                -(p.x / scale) as f32,
                -(p.y / scale / f64::from(ROW_HEIGHT)) as f32,
            ),
        };
        let cursor = self.pane(PaneKind::Library).and_then(|p| p.input.cursor);
        let over_tree = cursor.is_some_and(|(x, _)| x < l.tree_scroll.x + ARROW as i32);
        // Over a filter pane, or its scrollbar: that pane.
        let over_pane = l.filters.zip(cursor).and_then(|(f, (x, y))| {
            (0..2).find(|&p| {
                let r = f.lists[p];
                let s = f.scrolls[p];
                x >= r.x && x < s.x + s.w as i32 && y >= s.y && y < s.y + s.h as i32
            })
        });
        let filter_shown = self.filter_visible();
        let ui = &mut self.lib_ui;
        let by =
            |at: usize, d: f32, max: usize| ((at as f32 + d).round().max(0.0) as usize).min(max);
        if let Some(p) = over_pane {
            let lines = ui.filter_rows[p].len() + 1;
            ui.filter_scroll[p] = by(ui.filter_scroll[p], dy, lines.saturating_sub(filter_shown));
        } else if over_tree {
            let n = self.tree_items().len();
            self.lib_ui.tree_scroll = by(self.lib_ui.tree_scroll, dy, n.saturating_sub(tree_shown));
        } else {
            ui.list_scroll = by(ui.list_scroll, dy, ui.list_len().saturating_sub(list_shown));
            let max = columns_w.saturating_sub(l.list.w as usize);
            ui.columns_offset = by(ui.columns_offset as usize, dx, max) as u32;
        }
        self.request_redraw(PaneKind::Library);
    }

    // -- Playlists ------------------------------------------------------------

    /// `base`, or `base 2`, `base 3`… — whichever no playlist has yet.
    fn unused_playlist_name(&self, base: &str) -> String {
        let taken = |n: &str| self.library.playlists.iter().any(|p| p.name == n);
        (1..)
            .map(|n| {
                if n == 1 {
                    base.to_string()
                } else {
                    format!("{base} {n}")
                }
            })
            .find(|n| !taken(n))
            .unwrap_or_else(|| base.to_string())
    }

    /// Show a playlist: select it in the tree, its heading open.
    fn show_library_playlist(&mut self, id: i64) {
        self.lib_ui.playlists_open = true;
        self.lib_ui.node = Node::Playlist(id);
        self.lib_ui.sort = None;
        self.lib_ui.filter = [None, None];
        self.requery();
        self.relayout_library();
    }

    /// Make a playlist of `tracks` (ids, perhaps none), show it, and start
    /// naming it.
    pub(super) fn new_library_playlist(&mut self, tracks: &[i64]) {
        let name = self.unused_playlist_name("New Playlist");
        if let Some(id) = self.library.create_playlist(&name, tracks) {
            self.show_library_playlist(id);
            self.start_rename(id);
        }
    }

    /// The current playlist, saved into the library: it shows when saved.
    pub(super) fn save_playlist_to_library(&mut self) {
        let paths: Vec<std::path::PathBuf> = self
            .playlist
            .entries()
            .iter()
            .map(|e| e.path.clone())
            .collect();
        if paths.is_empty() {
            return;
        }
        let name = self.unused_playlist_name("Saved Playlist");
        self.library.save_playlist(name, paths);
        self.show_library();
    }

    /// A playlist saved from the playlist window has arrived: show it and
    /// start naming it.
    pub(super) fn show_saved_playlist(&mut self) {
        if let Some(id) = self.library.saved.take() {
            self.show_library_playlist(id);
            self.start_rename(id);
        }
    }

    /// Start renaming one of Lantern's own playlists in the tree.
    pub(super) fn start_rename(&mut self, id: i64) {
        let Some(p) = self
            .library
            .playlist(id)
            .filter(|_| self.library.is_own(id))
        else {
            return;
        };
        self.lib_ui.renaming = Some((id, TextEdit::new(&p.name)));
        // Keep it in view.
        let items = self.tree_items();
        if let Some(i) = items.iter().position(|(n, _)| *n == Node::Playlist(id)) {
            let shown = self.lib_visible().0.max(1);
            let ui = &mut self.lib_ui;
            if i < ui.tree_scroll {
                ui.tree_scroll = i;
            } else if i >= ui.tree_scroll + shown {
                ui.tree_scroll = i + 1 - shown;
            }
        }
        self.request_redraw(PaneKind::Library);
    }

    /// Settle a rename: a name left blank keeps the old one.
    fn finish_rename(&mut self, id: i64, name: &str) {
        let name = name.trim();
        if !name.is_empty() && self.library.playlist(id).is_some_and(|p| p.name != name) {
            self.library.rename_playlist(id, name);
        }
        self.request_redraw(PaneKind::Library);
    }

    /// Delete a playlist, once the user agrees. Its tracks stay.
    pub(super) fn delete_library_playlist(&mut self, id: i64) {
        let Some(name) = self.library.playlist(id).map(|p| p.name.clone()) else {
            return;
        };
        let answer = rfd::MessageDialog::new()
            .set_title("Delete Playlist")
            .set_description(format!(
                "Delete the playlist \u{201c}{name}\u{201d}? Its tracks stay in the library."
            ))
            .set_buttons(rfd::MessageButtons::OkCancelCustom(
                "Delete".into(),
                "Cancel".into(),
            ))
            .show();
        if answer != rfd::MessageDialogResult::Custom("Delete".into()) {
            return;
        }
        self.library.delete_playlist(id);
        if self.lib_ui.listed_pick == Some(id) {
            self.lib_ui.listed_pick = None;
        }
        self.lib_ui.built = None;
        if self.lib_ui.node == Node::Playlist(id) {
            self.lib_ui.node = Node::Playlists;
            self.requery();
        }
        self.request_redraw(PaneKind::Library);
    }

    /// The playlist the tree has selected, if it is one of Lantern's own.
    pub(super) fn own_playlist_shown(&self) -> Option<i64> {
        match self.lib_ui.node {
            Node::Playlist(id) if self.library.is_own(id) => Some(id),
            _ => None,
        }
    }

    /// The playlist the tree has selected — or, under the Playlists
    /// heading, the one picked in the list.
    pub(super) fn playlist_selected_in_tree(&self) -> Option<i64> {
        match self.lib_ui.node {
            Node::Playlist(id) => Some(id),
            Node::Playlists => self.lib_ui.listed_pick,
            _ => None,
        }
    }

    /// Add the selected tracks (or every row) to the end of playlist `id`.
    pub(super) fn send_selection_to_playlist(&mut self, id: i64) {
        self.refresh_library_rows();
        let ids: Vec<i64> = self
            .library_selection()
            .into_iter()
            .map(|i| self.library.tracks[i].id)
            .collect();
        self.append_to_playlist(id, &ids);
    }

    fn append_to_playlist(&mut self, id: i64, ids: &[i64]) {
        if ids.is_empty() || !self.library.is_own(id) {
            return;
        }
        let mut tracks = self
            .library
            .playlist(id)
            .map(|p| p.tracks.clone())
            .unwrap_or_default();
        tracks.extend_from_slice(ids);
        self.library.set_playlist_tracks(id, &tracks);
        self.request_redraw(PaneKind::Library);
    }

    /// A new playlist of the selected tracks (or every row).
    pub(super) fn send_selection_to_new_playlist(&mut self) {
        self.refresh_library_rows();
        let ids: Vec<i64> = self
            .library_selection()
            .into_iter()
            .map(|i| self.library.tracks[i].id)
            .collect();
        self.new_library_playlist(&ids);
    }

    /// Take the selected tracks out of the playlist shown. They stay in the
    /// library.
    pub(super) fn remove_selection_from_playlist(&mut self) {
        let Some(id) = self.own_playlist_shown() else {
            return;
        };
        let selected = self.lib_ui.selected.clone();
        let tracks: Vec<i64> = self
            .library
            .playlist(id)
            .map(|p| {
                p.tracks
                    .iter()
                    .copied()
                    .filter(|t| !selected.contains(t))
                    .collect()
            })
            .unwrap_or_default();
        self.library.set_playlist_tracks(id, &tracks);
        self.lib_ui.selected.clear();
        self.request_redraw(PaneKind::Library);
    }

    /// Play, or enqueue, a whole library playlist.
    pub(super) fn send_library_playlist(&mut self, id: i64, replace: bool) {
        let index: HashMap<i64, usize> = self
            .library
            .tracks
            .iter()
            .enumerate()
            .map(|(i, t)| (t.id, i))
            .collect();
        let entries: Vec<Entry> = self
            .library
            .playlist(id)
            .map(|p| {
                p.tracks
                    .iter()
                    .filter_map(|t| index.get(t))
                    .map(|&i| entry(&self.library.tracks[i]))
                    .collect()
            })
            .unwrap_or_default();
        if entries.is_empty() {
            return;
        }
        if replace {
            self.playlist.clear();
            self.pl_scroll = 0;
            self.add_entries(entries);
            self.play_entry(0);
        } else {
            self.add_entries(entries);
        }
    }

    /// The tree's node under logical `y`, if any.
    fn tree_node_at(&self, y: i32) -> Option<Node> {
        let l = self.lib_layout();
        if y < l.tree.y || y >= l.tree.y + l.tree.h as i32 {
            return None;
        }
        let i = ((y - l.tree.y) / ROW_HEIGHT as i32) as usize + self.lib_ui.tree_scroll;
        self.tree_items().get(i).map(|(n, _)| *n)
    }

    // -- Buttons --------------------------------------------------------------

    /// The selected tracks, or every row if none is selected, in list
    /// order, as playlist entries.
    fn library_selection_entries(&mut self) -> Vec<Entry> {
        self.refresh_library_rows();
        self.library_selection()
            .into_iter()
            .map(|i| entry(&self.library.tracks[i]))
            .collect()
    }

    /// Indices into the tracks: the selection, or every row if none.
    fn library_selection(&self) -> Vec<usize> {
        let ui = &self.lib_ui;
        ui.rows
            .iter()
            .copied()
            .filter(|&i| ui.selected.is_empty() || ui.selected.contains(&self.library.tracks[i].id))
            .collect()
    }

    /// The menu a right-click in the library window opens: the tracks' over
    /// the list (selecting the row under the pointer), the columns' over
    /// the header, the library's anywhere else.
    pub(super) fn library_menu_here(&mut self) -> super::menus::MenuKind {
        use super::menus::MenuKind;
        let Some((x, y)) = self.pane(PaneKind::Library).and_then(|p| p.input.cursor) else {
            return MenuKind::Library;
        };
        let l = self.lib_layout();
        if l.list.contains(x, y) && self.lib_ui.node == Node::Playlists {
            // A listed playlist: pick it, and offer what the tree would.
            let row =
                ((y - l.list.y) / ROW_HEIGHT as i32).max(0) as usize + self.lib_ui.list_scroll;
            self.lib_ui.listed_pick = self.lib_ui.listed.get(row).copied();
            self.request_redraw(PaneKind::Library);
            MenuKind::LibraryTree
        } else if l.list.contains(x, y) {
            self.select_library_row_at(y - l.list.y);
            MenuKind::LibraryTracks
        } else if l.tree.contains(x, y) {
            // On a playlist, its menu; the tree's own anywhere else.
            if let Some(node @ Node::Playlist(_)) = self.tree_node_at(y)
                && self.lib_ui.node != node
            {
                self.lib_ui.node = node;
                self.lib_ui.sort = None;
                self.requery();
                self.relayout_library();
            }
            MenuKind::LibraryTree
        } else if l.header.contains(x, y) {
            MenuKind::LibraryColumns
        } else {
            MenuKind::Library
        }
    }

    /// Right-click in the list: on a row not yet selected, select it alone.
    pub(super) fn select_library_row_at(&mut self, local_y: i32) {
        let row = (local_y / ROW_HEIGHT as i32).max(0) as usize + self.lib_ui.list_scroll;
        if let Some(&i) = self.lib_ui.rows.get(row) {
            let id = self.library.tracks[i].id;
            if !self.lib_ui.selected.contains(&id) {
                self.lib_ui.selected = HashSet::from([id]);
                self.lib_ui.anchor = Some(row);
            }
        }
        self.request_redraw(PaneKind::Library);
    }

    /// Rate the selected tracks, 0 (none) to 5 stars.
    pub(super) fn rate_library_selection(&mut self, stars: u8) {
        let ids: Vec<i64> = self.lib_ui.selected.iter().copied().collect();
        self.library.rate(&ids, stars);
    }

    /// Take the selected tracks out of the library. The files stay.
    pub(super) fn remove_library_selection(&mut self) {
        let paths: Vec<std::path::PathBuf> = self
            .library
            .tracks
            .iter()
            .filter(|t| self.lib_ui.selected.contains(&t.id))
            .map(|t| t.info.path.clone())
            .collect();
        self.library.forget(&paths);
        self.lib_ui.selected.clear();
    }

    /// File Info for the first selected track.
    pub(super) fn library_selection_info(&mut self) {
        let first = self.library_selection().into_iter().next();
        if let Some(i) = first {
            let path = self.library.tracks[i].info.path.clone();
            self.open_info(path);
        }
    }

    /// Show the first selected track's file in Finder.
    pub(super) fn reveal_library_selection(&self) {
        let first = self.library_selection().into_iter().next();
        if let Some(i) = first {
            let path = &self.library.tracks[i].info.path;
            if let Err(e) = std::process::Command::new("open")
                .arg("-R")
                .arg(path)
                .spawn()
            {
                eprintln!("could not show {}: {e}", path.display());
            }
        }
    }

    /// Show or hide a column (an index into [`ALL_COLUMNS`]); the last one
    /// shown stays.
    pub(super) fn toggle_library_column(&mut self, index: usize) {
        let cols = &mut self.lib_ui.columns;
        if let Some(n) = cols.iter().position(|c| c.0 == index) {
            if cols.len() > 1 {
                cols.remove(n);
            }
        } else if let Some(c) = ALL_COLUMNS.get(index) {
            cols.push((index, c.3));
        }
        self.relayout_library();
    }

    pub(super) fn reset_library_columns(&mut self) {
        self.lib_ui.columns = default_columns();
        self.lib_ui.columns_offset = 0;
        self.request_redraw(PaneKind::Library);
    }

    /// Whether the column (an index into [`ALL_COLUMNS`]) shows.
    pub(super) fn library_column_shown(&self, index: usize) -> bool {
        self.lib_ui.columns.iter().any(|c| c.0 == index)
    }

    /// The library window's buttons, released over.
    pub(super) fn activate_library_button(&mut self, id: WidgetId) {
        match id {
            lw::CLEAR => self.set_search(TextEdit::default()),
            lw::PLAY => self.send_library_selection(true),
            lw::ENQUEUE => self.send_library_selection(false),
            lw::LIBRARY_BUTTON => {
                self.show_menu(super::menus::MenuKind::Library, PaneKind::Library)
            }
            _ => {}
        }
    }

    /// Play (replacing the playlist) or enqueue the selection, or every row
    /// if none is selected.
    pub(super) fn send_library_selection(&mut self, replace: bool) {
        // Under the Playlists heading, the playlist picked.
        if self.lib_ui.node == Node::Playlists {
            if let Some(id) = self.lib_ui.listed_pick {
                self.send_library_playlist(id, replace);
            }
            return;
        }
        let chosen = self.library_selection_entries();
        if chosen.is_empty() {
            return;
        }
        if replace {
            self.playlist.clear();
            self.pl_scroll = 0;
            self.add_entries(chosen);
            self.play_entry(0);
        } else {
            self.add_entries(chosen);
        }
    }

    // -- Keys -----------------------------------------------------------------

    fn set_search(&mut self, search: TextEdit) {
        let changed = search.text != self.lib_ui.search.text;
        self.lib_ui.search = search;
        if changed {
            self.requery();
        }
        self.request_redraw(PaneKind::Library);
    }

    /// Keys while the library has focus: a playlist's new name while it is
    /// renamed; otherwise the arrows move through the list, Enter plays,
    /// and typing goes to the search box. True if the key was used.
    pub(super) fn library_key(&mut self, event: &KeyEvent) -> bool {
        let m = self.modifiers;
        let typed = event.text.as_deref();
        if let Some((id, mut name)) = self.lib_ui.renaming.take() {
            match name.key(&event.logical_key, typed, m) {
                Edit::Submit => self.finish_rename(id, &name.text),
                Edit::Cancel => {}
                Edit::Ignored if m.control_key() || m.alt_key() => {
                    self.lib_ui.renaming = Some((id, name));
                    return false;
                }
                _ => self.lib_ui.renaming = Some((id, name)),
            }
            self.request_redraw(PaneKind::Library);
            return true;
        }
        if m.control_key() || m.alt_key() {
            return false;
        }
        match &event.logical_key {
            Key::Character(c) if m.super_key() && c.eq_ignore_ascii_case("a") => {
                self.lib_ui.selected = self
                    .lib_ui
                    .rows
                    .iter()
                    .map(|&i| self.library.tracks[i].id)
                    .collect();
            }
            Key::Named(NamedKey::Enter) => self.send_library_selection(true),
            Key::Named(NamedKey::ArrowUp | NamedKey::ArrowDown) => {
                let down = matches!(event.logical_key, Key::Named(NamedKey::ArrowDown));
                self.move_library_cursor(down);
            }
            Key::Named(NamedKey::PageUp | NamedKey::PageDown) => {
                let down = matches!(event.logical_key, Key::Named(NamedKey::PageDown));
                let shown = self.lib_visible().1;
                let max = self.lib_ui.rows.len().saturating_sub(shown);
                let at = self.lib_ui.list_scroll;
                self.lib_ui.list_scroll = if down {
                    (at + shown).min(max)
                } else {
                    at.saturating_sub(shown)
                };
            }
            key => {
                let mut search = self.lib_ui.search.clone();
                match search.key(key, typed, m) {
                    Edit::Ignored => return false,
                    // Escape empties the search, as Clear Search does.
                    Edit::Cancel => self.set_search(TextEdit::default()),
                    _ => self.set_search(search),
                }
            }
        }
        self.request_redraw(PaneKind::Library);
        true
    }

    /// An input method's doing, while the library has the keyboard: text
    /// being composed shows in the search box; text settled on is typed —
    /// into a playlist's name if one is being renamed.
    pub(super) fn library_ime(&mut self, ime: winit::event::Ime) {
        use winit::event::Ime;
        match ime {
            Ime::Preedit(text, _) => {
                self.lib_ui.preedit = (!text.is_empty()).then_some(text);
            }
            Ime::Commit(text) => {
                self.lib_ui.preedit = None;
                if let Some((_, name)) = &mut self.lib_ui.renaming {
                    name.insert(&text);
                } else {
                    let mut search = self.lib_ui.search.clone();
                    search.insert(&text);
                    self.set_search(search);
                }
            }
            Ime::Enabled | Ime::Disabled => self.lib_ui.preedit = None,
        }
        self.place_ime_window();
        self.request_redraw(PaneKind::Library);
    }

    /// Tell the input method where the caret is, so its candidate window
    /// opens beside it.
    fn place_ime_window(&self) {
        let l = self.lib_layout();
        let x = l.search.x + 2 + (self.caret_at_x() as i32);
        let at = winit::dpi::LogicalPosition::new(f64::from(x), f64::from(l.search.y));
        let size = winit::dpi::LogicalSize::new(1.0, f64::from(l.search.h));
        if let Some(p) = self.pane(PaneKind::Library) {
            p.window.set_ime_cursor_area(at, size);
        }
    }

    /// How far into the search box the caret is, in pixels.
    fn caret_at_x(&self) -> f32 {
        let Some(font) = &self.font else { return 0.0 };
        let style = crate::ui::font::TextStyle {
            px: 9.0,
            spacing: 0.5,
        };
        let before: String = self
            .lib_ui
            .search
            .text
            .chars()
            .take(self.lib_ui.search.caret)
            .collect();
        font.width(&before, style)
    }

    /// Up or down a row from the last one clicked, selecting it alone and
    /// keeping it in view.
    fn move_library_cursor(&mut self, down: bool) {
        let shown = self.lib_visible().1.max(1);
        let ui = &mut self.lib_ui;
        if ui.rows.is_empty() {
            return;
        }
        let last = ui.rows.len() - 1;
        let row = match (ui.anchor, down) {
            (None, _) => 0,
            (Some(r), true) => (r + 1).min(last),
            (Some(r), false) => r.saturating_sub(1),
        };
        ui.anchor = Some(row);
        ui.selected = HashSet::from([self.library.tracks[ui.rows[row]].id]);
        if row < ui.list_scroll {
            ui.list_scroll = row;
        } else if row >= ui.list_scroll + shown {
            ui.list_scroll = row + 1 - shown;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_lines_count_and_sort_with_untagged_last() {
        let track = |artist: &str| {
            let mut t = Track::default();
            t.info.artist = artist.into();
            t
        };
        let tracks = vec![track("b"), track(""), track("A"), track("b")];
        let all: Vec<usize> = (0..tracks.len()).collect();
        assert_eq!(
            distinct(&tracks, &all, artist),
            [
                ("A".to_string(), 1),
                ("b".to_string(), 2),
                (String::new(), 1)
            ]
        );
        assert_eq!(distinct(&tracks, &[0, 3], artist), [("b".to_string(), 2)]);
    }

    #[test]
    fn columns_round_trip_through_settings() {
        let cols = vec![(2, 90), (0, 120)];
        let saved = columns_to_settings(&cols);
        assert_eq!(
            saved,
            [("album".to_string(), 90), ("artist".to_string(), 120)]
        );
        assert_eq!(columns_from_settings(&saved), cols);
        // Unknown names drop out; nothing left means the defaults.
        let odd = [("moods".to_string(), 50), ("artist".to_string(), 1)];
        assert_eq!(columns_from_settings(&odd), [(0, MIN_COLUMN_WIDTH)]);
        assert_eq!(columns_from_settings(&[]), default_columns());
    }

    #[test]
    fn dates_read_as_the_calendar_does() {
        assert_eq!(date(0), "1970-01-01");
        assert_eq!(date(951_782_400), "2000-02-29");
        assert_eq!(date(1_767_323_045), "2026-01-02");
    }

    #[test]
    fn bars_read_offset_and_proportion() {
        assert_eq!(bar(0, 100, 10), (0.0, 0.1));
        assert_eq!(bar(90, 100, 10), (1.0, 0.1));
        assert_eq!(bar(0, 5, 10), (0.0, 2.0));
        assert_eq!(bar(0, 0, 10), (0.0, 1.0));
    }

    #[test]
    fn library_tracks_go_to_the_playlist_titled() {
        let mut t = Track::default();
        t.info.path = "/m/a.mp3".into();
        t.info.title = "Start Together".into();
        t.info.artist = "Sleater-Kinney".into();
        t.info.duration_secs = Some(159.0);
        let e = entry(&t);
        assert_eq!(e.title.as_deref(), Some("Sleater-Kinney - Start Together"));
        assert_eq!(e.duration, Some(159.0));
        assert!(e.tags_loaded);
    }
}
