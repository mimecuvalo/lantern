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

use std::collections::{HashMap, HashSet};
use std::time::Instant;

use lantern_library::Track;
use lantern_library::view::{self, Column, Sort, View};
use lantern_playlist::Entry;
use winit::event::{KeyEvent, MouseScrollDelta};
use winit::keyboard::{Key, NamedKey};

use super::{App, DOUBLE_CLICK, PaneKind};
use crate::ui::WidgetId;
use crate::ui::gen_controls::{ARROW, ScrollPart, ScrollState, scroll_geometry, scroll_hit};
use crate::ui::library_window::{
    self as lw, ALL_COLUMNS, Columns, Layout, LibraryView, ListRow, MIN_COLUMN_WIDTH, ROW_HEIGHT,
    TreeRow, clamp_tree_width, columns_width, default_columns,
};

/// A press moves this far before it becomes a drag.
const DRAG_START: i32 = 4;
/// A column's edge can be taken this far either side of it.
const EDGE_GRAB: i32 = 3;
use crate::ui::playlist_window::clock;

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
}

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
    search: String,
    /// In chars.
    caret: usize,
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
            search: String::new(),
            caret: 0,
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
        }
    }
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
    fn lib_layout(&self) -> Layout {
        let (w, h) = self.library_size;
        lw::layout(w, h, self.lib_ui.tree_width)
    }

    /// Whole rows in view: in the tree, and in the list.
    fn lib_visible(&self) -> (usize, usize) {
        let (w, h) = self.library_size;
        lw::visible_rows(w, h, self.lib_ui.tree_width)
    }

    /// The tree's lines, top to bottom, with the node each selects.
    fn tree_items(&self) -> Vec<(Node, TreeRow)> {
        let ui = &self.lib_ui;
        let row = |node: Node, label: &str, depth: u8, expander: Option<bool>| {
            (
                node,
                TreeRow {
                    label: label.to_string(),
                    depth,
                    expander,
                    selected: ui.node == node,
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
                view::select(tracks, v, &ui.search, sort, now)
            }
            Node::Playlists => Vec::new(),
            Node::Playlist(id) => {
                let index: HashMap<i64, usize> =
                    tracks.iter().enumerate().map(|(i, t)| (t.id, i)).collect();
                let terms = view::terms(&ui.search);
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
        let (position, visible) = bar(ui.list_scroll, ui.rows.len(), list_shown);
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
            None => {
                let n = ui.rows.len();
                format!(
                    "{n} {} [{}]",
                    if n == 1 { "item" } else { "items" },
                    clock(total)
                )
            }
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
            search: ui.search.clone(),
            caret: focused.then_some(ui.caret),
            sort,
            rows,
            list_scroll,
            list_hscroll,
            columns_offset: ui.columns_offset,
            columns: ui.columns.clone(),
            tree_width: ui.tree_width,
            column_drop: self.column_drop_x(),
            status,
        }
    }

    // -- Pointer ------------------------------------------------------------

    /// The pointer went down on `id` at logical `(x, y)`.
    pub(super) fn press_library(&mut self, id: WidgetId, x: i32, y: i32) {
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
                    self.requery();
                }
            }
            lw::SEARCH => {
                let local = x - l.search.x - 2;
                self.lib_ui.caret = self.caret_at(local);
            }
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
            lw::LIST => {
                if !self.click_library_row(y - l.list.y) {
                    self.lib_ui.list_drag = Some(((x, y), false));
                }
            }
            lw::DIVIDER => self.lib_ui.divider_grab = Some(x - l.divider.x),
            _ => {}
        }
        self.request_redraw(PaneKind::Library);
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
        let Some(font) = &self.font else { return 0 };
        let style = crate::ui::font::TextStyle {
            px: 9.0,
            spacing: 0.5,
        };
        // Before the first character whose middle is past the pointer.
        let chars: Vec<char> = self.lib_ui.search.chars().collect();
        let width = |n: usize| font.width(&chars[..n].iter().collect::<String>(), style);
        (0..chars.len())
            .find(|&n| (local_x as f32) < (width(n) + width(n + 1)) / 2.0)
            .unwrap_or(chars.len())
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
        let (w, h) = self.library_size;
        let tree_width = self.lib_ui.tree_width;
        if let Some(p) = self.pane_mut(PaneKind::Library) {
            let held = p.input.active;
            p.widgets = lw::widgets(w, h, tree_width);
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
    fn drop_library_tracks(&mut self) {
        let Some((gx, gy)) = self.global_cursor(PaneKind::Library) else {
            return;
        };
        let (kinds, rects, shown) = self.frames();
        let target = (0..kinds.len()).find(|&i| {
            let r = rects[i];
            let inside = gx >= r.x && gx < r.x + r.w && gy >= r.y && gy < r.y + r.h;
            shown[i] && kinds[i] != PaneKind::Library && inside
        });
        let Some(i) = target else { return };
        let entries = self.library_selection_entries();
        if entries.is_empty() {
            return;
        }
        if kinds[i] == PaneKind::Playlist
            && !self.pane(PaneKind::Playlist).is_some_and(|p| p.shaded)
        {
            let local_y = (gy - rects[i].y) as i32;
            let at = self.playlist_row_at(local_y);
            self.insert_entries(at, entries);
        } else {
            self.add_entries(entries);
        }
    }

    pub(super) fn drag_library_scroll(&mut self, id: WidgetId, x: i32, y: i32) {
        let l = self.lib_layout();
        let (tree_shown, list_shown) = self.lib_visible();
        let (rect, vertical, total, shown) = match id {
            lw::TREE_SCROLL => (l.tree_scroll, true, self.tree_items().len(), tree_shown),
            lw::LIST_SCROLL => (l.list_scroll, true, self.lib_ui.rows.len(), list_shown),
            _ => (
                l.list_hscroll,
                false,
                columns_width(&self.lib_ui.columns) as usize,
                l.list.w as usize,
            ),
        };
        let offset = match id {
            lw::TREE_SCROLL => self.lib_ui.tree_scroll,
            lw::LIST_SCROLL => self.lib_ui.list_scroll,
            _ => self.lib_ui.columns_offset as usize,
        };
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
        let max = total.saturating_sub(shown);
        // Rows step by one; the columns, by a little more.
        let step = if vertical { 1 } else { 20 };
        let new = match self.lib_ui.hold {
            None => {
                let part = scroll_hit(len, &state, along);
                let grab = match scroll_geometry(len, &state).2 {
                    Some((at, _)) => along - at as i32,
                    None => 0,
                };
                self.lib_ui.hold = Some(ScrollHold { id, part, grab });
                match part {
                    ScrollPart::Back => offset.saturating_sub(step),
                    ScrollPart::Forward => offset + step,
                    ScrollPart::PageBack => offset.saturating_sub(shown),
                    ScrollPart::PageForward => offset + shown,
                    ScrollPart::Thumb => offset,
                }
            }
            Some(hold) if hold.part == ScrollPart::Thumb => {
                let (_, track, thumb) = scroll_geometry(len, &state);
                let thumb = thumb.map_or(0, |t| t.1);
                let travel = track.saturating_sub(thumb).max(1) as f32;
                let at = (along - hold.grab - ARROW as i32) as f32 / travel;
                (at.clamp(0.0, 1.0) * max as f32).round() as usize
            }
            Some(_) => offset,
        };
        let new = new.min(max);
        match id {
            lw::TREE_SCROLL => self.lib_ui.tree_scroll = new,
            lw::LIST_SCROLL => self.lib_ui.list_scroll = new,
            _ => self.lib_ui.columns_offset = new as u32,
        }
        self.request_redraw(PaneKind::Library);
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
        let over_tree = self
            .pane(PaneKind::Library)
            .and_then(|p| p.input.cursor)
            .is_some_and(|(x, _)| x < l.tree_scroll.x + ARROW as i32);
        let ui = &mut self.lib_ui;
        let by =
            |at: usize, d: f32, max: usize| ((at as f32 + d).round().max(0.0) as usize).min(max);
        if over_tree {
            let n = self.tree_items().len();
            self.lib_ui.tree_scroll = by(self.lib_ui.tree_scroll, dy, n.saturating_sub(tree_shown));
        } else {
            ui.list_scroll = by(ui.list_scroll, dy, ui.rows.len().saturating_sub(list_shown));
            let max = columns_w.saturating_sub(l.list.w as usize);
            ui.columns_offset = by(ui.columns_offset as usize, dx, max) as u32;
        }
        self.request_redraw(PaneKind::Library);
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
        if l.list.contains(x, y) {
            self.select_library_row_at(y - l.list.y);
            MenuKind::LibraryTracks
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
            lw::CLEAR => self.set_search(String::new(), 0),
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

    fn set_search(&mut self, text: String, caret: usize) {
        self.lib_ui.caret = caret.min(text.chars().count());
        if text != self.lib_ui.search {
            self.lib_ui.search = text;
            self.requery();
        }
        self.request_redraw(PaneKind::Library);
    }

    /// Keys while the library has focus: typing goes to the search box, the
    /// arrows move through the list. True if the key was used.
    pub(super) fn library_key(&mut self, event: &KeyEvent) -> bool {
        let m = self.modifiers;
        if m.control_key() || m.alt_key() {
            return false;
        }
        let search = self.lib_ui.search.clone();
        let caret = self.lib_ui.caret;
        let chars: Vec<char> = search.chars().collect();
        let joined = |c: &[char]| c.iter().collect::<String>();
        if m.super_key() {
            match &event.logical_key {
                Key::Character(c) if c.eq_ignore_ascii_case("a") => {
                    self.lib_ui.selected = self
                        .lib_ui
                        .rows
                        .iter()
                        .map(|&i| self.library.tracks[i].id)
                        .collect();
                }
                Key::Character(c) if c.eq_ignore_ascii_case("v") => {
                    #[cfg(target_os = "macos")]
                    if let Some(paste) = crate::platform::macos::clipboard_text() {
                        let paste: String = paste.chars().filter(|c| !c.is_control()).collect();
                        let n = paste.chars().count();
                        let mut text = joined(&chars[..caret]);
                        text.push_str(&paste);
                        text.push_str(&joined(&chars[caret..]));
                        self.set_search(text, caret + n);
                    }
                }
                Key::Named(NamedKey::Backspace) => self.set_search(joined(&chars[caret..]), 0),
                _ => return false,
            }
            self.request_redraw(PaneKind::Library);
            return true;
        }
        match &event.logical_key {
            Key::Named(NamedKey::Backspace) if caret > 0 => {
                let text = joined(&chars[..caret - 1]) + &joined(&chars[caret..]);
                self.set_search(text, caret - 1);
            }
            Key::Named(NamedKey::Delete) if caret < chars.len() => {
                let text = joined(&chars[..caret]) + &joined(&chars[caret + 1..]);
                self.set_search(text, caret);
            }
            Key::Named(NamedKey::Backspace | NamedKey::Delete) => {}
            Key::Named(NamedKey::ArrowLeft) => self.lib_ui.caret = caret.saturating_sub(1),
            Key::Named(NamedKey::ArrowRight) => self.lib_ui.caret = (caret + 1).min(chars.len()),
            Key::Named(NamedKey::Home) => self.lib_ui.caret = 0,
            Key::Named(NamedKey::End) => self.lib_ui.caret = chars.len(),
            Key::Named(NamedKey::Escape) => self.set_search(String::new(), 0),
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
            _ => {
                let Some(text) = event.text.as_ref() else {
                    return false;
                };
                let typed: Vec<char> = text.chars().filter(|c| !c.is_control()).collect();
                if typed.is_empty() {
                    return false;
                }
                let new = joined(&chars[..caret]) + &joined(&typed) + &joined(&chars[caret..]);
                self.set_search(new, caret + typed.len());
            }
        }
        self.request_redraw(PaneKind::Library);
        true
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
