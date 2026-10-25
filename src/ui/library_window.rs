//! The media library window: a generic skinned window, framed from
//! `gen.bmp`.
//!
//! The frame follows Webamp's `GenWindow` and `gen-window.css` (MIT,
//! Copyright (c) 2015 Jordan Eldredge): a 20px title bar built from corner,
//! fill and end-cap pieces around the title, which is spelled in the
//! skin's own alphabet; 11px and 8px sides; a 14px bottom. It resizes in the
//! playlist's steps, 25px across and 29px down. Checked pixel for pixel
//! against Winamp's own library window in the base skin.
//!
//! Inside, Winamp's library layout: the tree of views and playlists on the
//! left over the Library button; on the right a search box over the track
//! list, its column headers sorting it, with Play and Enqueue beneath. The
//! controls come from `genex.bmp` (see `gen_controls`), the text is the
//! playlist's font in `genex.bmp`'s colours.

use lantern_library::view::{Column, Sort};
use lantern_skin::Skin;

use super::blit::{self, Surface};
use super::font::TextFont;
use super::gen_controls::{self as gc, ARROW, ScrollState, clip, rgb, style, text_in};
pub use super::gen_window::content_area;
use super::gen_window::{self, LEFT, RIGHT, TOP};
use super::geometry::Rect;
use super::main_window::NO_SKIN_BACKGROUND;
use super::{Widget, WidgetId, WidgetKind};

pub const TITLE_DRAG: WidgetId = WidgetId(300);
pub const CLOSE: WidgetId = WidgetId(301);
pub const RESIZE: WidgetId = WidgetId(302);
pub const TREE: WidgetId = WidgetId(303);
pub const TREE_SCROLL: WidgetId = WidgetId(304);
pub const LIBRARY_BUTTON: WidgetId = WidgetId(305);
pub const SEARCH: WidgetId = WidgetId(306);
pub const CLEAR: WidgetId = WidgetId(307);
pub const HEADER: WidgetId = WidgetId(308);
pub const LIST: WidgetId = WidgetId(309);
pub const LIST_SCROLL: WidgetId = WidgetId(310);
pub const LIST_HSCROLL: WidgetId = WidgetId(311);
pub const PLAY: WidgetId = WidgetId(312);
pub const ENQUEUE: WidgetId = WidgetId(313);
pub const DIVIDER: WidgetId = WidgetId(314);
pub const ARTISTS: WidgetId = WidgetId(315);
pub const ARTISTS_SCROLL: WidgetId = WidgetId(316);
pub const ALBUMS: WidgetId = WidgetId(317);
pub const ALBUMS_SCROLL: WidgetId = WidgetId(318);

/// The filter panes' lists, Artist then Album, and their scrollbars.
pub const FILTER_LISTS: [WidgetId; 2] = [ARTISTS, ALBUMS];
pub const FILTER_SCROLLS: [WidgetId; 2] = [ARTISTS_SCROLL, ALBUMS_SCROLL];

/// Smaller than this, the tree and the list have no room.
pub const MIN_WIDTH: u32 = 375;
pub const MIN_HEIGHT: u32 = 203;

/// Tree and list rows, as the playlist's.
pub const ROW_HEIGHT: u32 = 13;
const HEADER_HEIGHT: u32 = 14;
const BUTTON_HEIGHT: u32 = 15;
/// The tree's width, its scrollbar included, until the divider is dragged.
pub const DEFAULT_TREE_WIDTH: u32 = 125;
/// The narrowest the tree, and the list, are let go.
const MIN_TREE_WIDTH: u32 = 70;
const MIN_LIST_WIDTH: u32 = 120;
/// The gap between the tree and the list, which drags.
const GAP: i32 = 6;
/// The narrowest a column is let go.
pub const MIN_COLUMN_WIDTH: u32 = 16;
/// How far each level of the tree is indented.
const INDENT: i32 = 11;

/// Every column the list can show: what it shows, its name in settings,
/// its header, its width to start with, whether it is a number (set
/// right), and whether it shows to start with.
pub const ALL_COLUMNS: [(Column, &str, &str, u32, bool, bool); 13] = [
    (Column::Artist, "artist", "Artist", 100, false, true),
    (Column::Title, "title", "Title", 130, false, true),
    (Column::Album, "album", "Album", 100, false, true),
    (Column::Length, "length", "Length", 40, true, true),
    (Column::TrackNo, "track", "Track #", 42, true, true),
    (Column::Genre, "genre", "Genre", 60, false, true),
    (Column::Year, "year", "Year", 34, true, true),
    (Column::Rating, "rating", "Rating", 42, false, true),
    (Column::PlayCount, "plays", "Play Count", 58, true, true),
    (
        Column::LastPlayed,
        "last_played",
        "Last Played",
        70,
        false,
        true,
    ),
    (Column::Added, "added", "Date Added", 70, false, false),
    (Column::Bitrate, "bitrate", "Bitrate", 44, true, false),
    (Column::Path, "path", "File", 200, false, false),
];

/// The list's columns while it lists the playlists themselves, under the
/// Playlists heading: header, width, set right.
pub const PLAYLIST_COLUMNS: [(&str, u32, bool); 4] = [
    ("Playlist", 180, false),
    ("Items", 50, true),
    ("Length", 64, true),
    ("From", 90, false),
];

/// The columns shown: indices into [`ALL_COLUMNS`], each with its width.
pub type Columns = Vec<(usize, u32)>;

pub fn default_columns() -> Columns {
    ALL_COLUMNS
        .iter()
        .enumerate()
        .filter(|(_, c)| c.5)
        .map(|(i, c)| (i, c.3))
        .collect()
}

/// `columns` side by side.
pub fn columns_width(columns: &[(usize, u32)]) -> u32 {
    columns.iter().map(|c| c.1).sum()
}

/// A tree `tree_width` wide, kept so that both it and the list have room
/// in a window `width` wide.
pub fn clamp_tree_width(width: u32, tree_width: u32) -> u32 {
    let content = width.saturating_sub(LEFT + RIGHT);
    let most = content.saturating_sub(MIN_LIST_WIDTH + GAP as u32);
    tree_width.min(most).max(MIN_TREE_WIDTH)
}

/// Where everything inside the frame goes, for a window this size.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Layout {
    pub tree: Rect,
    pub tree_scroll: Rect,
    pub library_button: Rect,
    pub search_label: (i32, i32),
    pub search: Rect,
    pub clear: Rect,
    pub header: Rect,
    pub list: Rect,
    pub list_scroll: Rect,
    pub list_hscroll: Rect,
    pub play: Rect,
    pub enqueue: Rect,
    pub status: (i32, i32),
    /// Between the tree and the list, the full height: it drags.
    pub divider: Rect,
    /// The Artist and Album panes above the list, where the view has them
    /// and the window has room.
    pub filters: Option<FilterPanes>,
}

/// The two filter panes, Artist then Album: each a header, a list and a
/// scrollbar beside both.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FilterPanes {
    pub heads: [Rect; 2],
    pub lists: [Rect; 2],
    pub scrolls: [Rect; 2],
}

/// The filter panes take this share of the height, in whole rows, but no
/// fewer than this many rows.
const FILTER_SHARE: f32 = 0.35;
const MIN_FILTER_ROWS: u32 = 4;
/// The fewest track rows the panes leave below them.
const MIN_LIST_ROWS: u32 = 5;

/// The window's layout. `filters`: the view has Artist and Album panes,
/// which show if there is room for them.
pub fn layout(width: u32, height: u32, tree_width: u32, filters: bool) -> Layout {
    let tree_width = clamp_tree_width(width, tree_width);
    let c = content_area(width, height);
    let bottom = c.y + c.h as i32 - BUTTON_HEIGHT as i32;
    // The tree sits in a sunken box, its scrollbar inside it.
    let tree_h = c.h - BUTTON_HEIGHT - 5;
    let tree = Rect::new(c.x + 1, c.y + 1, tree_width - ARROW, tree_h);
    let tree_scroll = Rect::new(tree.x + tree.w as i32, tree.y, ARROW, tree_h);
    let rx = c.x + tree_width as i32 + GAP;
    let rw = (c.x + c.w as i32 - rx) as u32;
    // Room for the label and the button in the widest playlist fonts.
    let clear = Rect::new(rx + rw as i32 - 72, c.y, 72, BUTTON_HEIGHT);
    let search_x = rx + 46;
    let search = Rect::new(search_x, c.y + 1, (clear.x - 5 - search_x) as u32, 13);
    // The filter panes, if any, then the list: it and its header share a
    // sunken box with both scrollbars.
    let top = c.y + 21;
    let room = bottom - 5 - ARROW as i32 - top;
    let fixed = |rows: u32| (HEADER_HEIGHT + rows * ROW_HEIGHT) as i32;
    let pane_rows =
        ((room as f32 * FILTER_SHARE) as i32 - HEADER_HEIGHT as i32).max(0) as u32 / ROW_HEIGHT;
    let pane_rows = pane_rows.max(MIN_FILTER_ROWS);
    let fits = room >= fixed(pane_rows) + 7 + fixed(MIN_LIST_ROWS);
    let filters = (filters && fits).then(|| {
        let inner = rw - 2;
        let pane_w = (inner - GAP as u32 - 2) / 2;
        let at = |i: i32| rx + 1 + i * (pane_w as i32 + GAP + 2);
        let rows_h = pane_rows * ROW_HEIGHT;
        let head = |i| Rect::new(at(i), top, pane_w - ARROW, HEADER_HEIGHT);
        let list = |i| Rect::new(at(i), top + HEADER_HEIGHT as i32, pane_w - ARROW, rows_h);
        let scroll = |i| {
            Rect::new(
                at(i) + (pane_w - ARROW) as i32,
                top,
                ARROW,
                HEADER_HEIGHT + rows_h,
            )
        };
        FilterPanes {
            heads: [head(0), head(1)],
            lists: [list(0), list(1)],
            scrolls: [scroll(0), scroll(1)],
        }
    });
    let header_y = match filters {
        Some(f) => f.lists[0].y + f.lists[0].h as i32 + 7,
        None => top,
    };
    let header = Rect::new(rx + 1, header_y, rw - 2 - ARROW, HEADER_HEIGHT);
    let list_y = header.y + HEADER_HEIGHT as i32;
    let list_h = (bottom - 5 - ARROW as i32 - list_y).max(0) as u32;
    let list = Rect::new(header.x, list_y, header.w, list_h);
    Layout {
        tree,
        tree_scroll,
        library_button: Rect::new(c.x, bottom, tree_width + 1, BUTTON_HEIGHT),
        search_label: (rx, c.y + 11),
        search,
        clear,
        header,
        list,
        list_scroll: Rect::new(
            header.x + header.w as i32,
            header.y,
            ARROW,
            HEADER_HEIGHT + list_h,
        ),
        list_hscroll: Rect::new(list.x, list_y + list_h as i32, list.w, ARROW),
        play: Rect::new(rx, bottom, 47, BUTTON_HEIGHT),
        enqueue: Rect::new(rx + 51, bottom, 55, BUTTON_HEIGHT),
        status: (rx + 113, bottom + 11),
        divider: Rect::new(c.x + tree_width as i32 + 1, c.y, (GAP - 1) as u32, c.h),
        filters,
    }
}

/// Whole rows that fit in the tree, and in the list.
pub fn visible_rows(width: u32, height: u32, tree_width: u32, filters: bool) -> (usize, usize) {
    let l = layout(width, height, tree_width, filters);
    (
        (l.tree.h / ROW_HEIGHT) as usize,
        (l.list.h / ROW_HEIGHT) as usize,
    )
}

pub const DEFAULT_WIDTH: u32 = 500;
pub const DEFAULT_HEIGHT: u32 = 348;

pub const TITLE: &str = "Lantern Library";

pub fn widgets(width: u32, height: u32, tree_width: u32, filters: bool) -> Vec<Widget> {
    let w = |id, x, y, width, height, kind| Widget::new(id, Rect::new(x, y, width, height), kind);
    let at = |id, r: Rect, kind| Widget::new(id, r, kind);
    let (wi, hi) = (width as i32, height as i32);
    let l = layout(width, height, tree_width, filters);
    use WidgetKind::{Button, HitZone, Slider};
    let mut v = vec![
        w(TITLE_DRAG, 0, 0, width, TOP, HitZone),
        w(CLOSE, wi - 11, 3, 9, 9, Button),
        at(TREE, l.tree, HitZone),
        at(TREE_SCROLL, l.tree_scroll, Slider),
        at(LIBRARY_BUTTON, l.library_button, Button),
        // A drag across the search box selects.
        at(SEARCH, l.search, Slider),
        at(CLEAR, l.clear, Button),
        // The header and the list take drags — to resize and move columns,
        // and to carry tracks to the playlist — so they hold the pointer.
        at(HEADER, l.header, Slider),
        at(LIST, l.list, Slider),
        at(DIVIDER, l.divider, Slider),
        at(LIST_SCROLL, l.list_scroll, Slider),
        at(LIST_HSCROLL, l.list_hscroll, Slider),
        at(PLAY, l.play, Button),
        at(ENQUEUE, l.enqueue, Button),
    ];
    if let Some(f) = l.filters {
        for i in 0..2 {
            v.push(at(FILTER_LISTS[i], f.lists[i], HitZone));
            v.push(at(FILTER_SCROLLS[i], f.scrolls[i], Slider));
        }
    }
    // Last, so it wins where it overlaps the frame's corner.
    v.push(w(RESIZE, wi - 20, hi - 20, 20, 20, Slider));
    v
}

/// One line of the tree.
#[derive(Debug, Clone, PartialEq)]
pub struct TreeRow {
    pub label: String,
    pub depth: u8,
    /// `Some(open)` for a row with children.
    pub expander: Option<bool>,
    pub selected: bool,
    /// Being renamed: the caret's place in `label`, in chars.
    pub editing: Option<usize>,
}

/// One line of the list: its cells, in the order the columns show.
#[derive(Debug, Clone, PartialEq)]
pub struct ListRow {
    pub cells: Vec<String>,
    pub selected: bool,
}

/// One line of a filter pane: an artist or album, or the "All" line.
#[derive(Debug, Clone, PartialEq)]
pub struct FilterRow {
    pub label: String,
    /// Its tracks.
    pub count: usize,
    pub selected: bool,
}

/// What the filter panes show, Artist then Album.
#[derive(Debug, Clone, PartialEq)]
pub struct FilterView {
    /// The rows in view of each pane, top first.
    pub rows: [Vec<FilterRow>; 2],
    pub scrolls: [ScrollState; 2],
}

#[derive(Debug, Clone)]
pub struct LibraryView {
    pub focused: bool,
    pub width: u32,
    pub height: u32,
    /// The tree's rows in view, top first.
    pub tree: Vec<TreeRow>,
    pub tree_scroll: ScrollState,
    pub search: String,
    /// Where the caret is in `search`, in chars, while it has the keyboard.
    pub caret: Option<usize>,
    /// The selected stretch of `search`, in chars.
    pub search_selection: Option<(usize, usize)>,
    /// The stretch of `search` an input method is still composing.
    pub search_marked: Option<(usize, usize)>,
    /// The order shown on the headers; `None` for a playlist's own order.
    pub sort: Option<Sort>,
    /// The list's rows in view, top first.
    pub rows: Vec<ListRow>,
    pub list_scroll: ScrollState,
    pub list_hscroll: ScrollState,
    /// How far the columns are scrolled left, in pixels.
    pub columns_offset: u32,
    /// The columns shown, in order: indices into [`ALL_COLUMNS`] and widths.
    pub columns: Columns,
    pub tree_width: u32,
    /// While a header is dragged: where it would land, as an x in the
    /// window.
    pub column_drop: Option<i32>,
    /// The Artist and Album panes, for a view that has them.
    pub filters: Option<FilterView>,
    /// The list lists playlists, in [`PLAYLIST_COLUMNS`], not tracks.
    pub listing_playlists: bool,
    /// What the list holds, or what the library is busy with.
    pub status: String,
}

/// Text as the playlist sets it.
/// Each rating star's slot in the Rating column, from the cell's left.
pub const STAR_STEP: i32 = 7;
const STAR_LEFT: i32 = 3;
const STAR: [&str; 5] = ["..#..", ".###.", "#####", ".###.", ".#.#."];

/// Which star a click `x` pixels into a Rating cell lands on, 1 to 5.
pub fn star_at(x: i32) -> u8 {
    ((x - STAR_LEFT).max(0) / STAR_STEP + 1).min(5) as u8
}

/// Five star slots: `filled` drawn whole, the rest as a dot, to click.
fn draw_stars(dst: &mut Surface<'_>, cell: Rect, filled: usize, colour: u32, scale: u32) {
    let y = cell.y + (cell.h as i32 - 5) / 2;
    for n in 0..5 {
        let x = cell.x + STAR_LEFT + n as i32 * STAR_STEP;
        if x + 5 > cell.x + cell.w as i32 {
            break;
        }
        if n < filled {
            for (dy, row) in STAR.iter().enumerate() {
                for (dx, c) in row.bytes().enumerate() {
                    if c == b'#' {
                        blit::fill_rect(dst, x + dx as i32, y + dy as i32, 1, 1, colour, scale);
                    }
                }
            }
        } else {
            blit::fill_rect(dst, x + 2, y + 2, 1, 1, colour, scale);
        }
    }
}

/// A small solid triangle, 5 wide, pointing down if `down`, else right.
fn triangle(dst: &mut Surface<'_>, x: i32, y: i32, down: bool, colour: u32, scale: u32) {
    for k in 0..3 {
        if down {
            blit::fill_rect(dst, x + k, y + k, (5 - 2 * k) as u32, 1, colour, scale);
        } else {
            blit::fill_rect(dst, x + k, y + k - 2, 1, (5 - 2 * k) as u32, colour, scale);
        }
    }
}

/// A list header's cell: its background, lit top and left, shaded bottom
/// and right.
fn header_cell(dst: &mut Surface<'_>, colours: &lantern_skin::GenExColors, cell: Rect, scale: u32) {
    let bg = rgb(colours.list_header_background);
    blit::fill_rect(dst, cell.x, cell.y, cell.w, cell.h, bg, scale);
    let (tl, br) = (
        rgb(colours.list_header_frame_top_left),
        rgb(colours.list_header_frame_bottom_right),
    );
    blit::fill_rect(dst, cell.x, cell.y, cell.w, 1, tl, scale);
    blit::fill_rect(dst, cell.x, cell.y, 1, cell.h, tl, scale);
    blit::fill_rect(
        dst,
        cell.x,
        cell.y + cell.h as i32 - 1,
        cell.w,
        1,
        br,
        scale,
    );
    blit::fill_rect(
        dst,
        cell.x + cell.w as i32 - 1,
        cell.y,
        1,
        cell.h,
        br,
        scale,
    );
}

pub fn draw(
    dst: &mut Surface<'_>,
    skin: Option<&Skin>,
    font: Option<&TextFont>,
    view: &LibraryView,
    widgets: &[Widget],
    scale: u32,
) {
    blit::clear(dst, NO_SKIN_BACKGROUND);
    let Some(skin) = skin else { return };
    let inside = content_area(view.width, view.height);
    let colours = *skin.genex_colors();
    blit::fill_rect(
        dst,
        inside.x,
        inside.y,
        inside.w,
        inside.h,
        rgb(colours.window_background),
        scale,
    );
    let l = layout(
        view.width,
        view.height,
        view.tree_width,
        view.filters.is_some(),
    );
    let pressed = |id| widgets.iter().any(|x| x.id == id && x.pressed);
    let item_bg = rgb(colours.item_background);
    let item_fg = rgb(colours.item_foreground);
    let sel_bg = rgb(colours.list_background_selected);
    let sel_fg = rgb(colours.list_text_selected);

    // The tree.
    let r = l.tree;
    blit::fill_rect(dst, r.x, r.y, r.w, r.h, item_bg, scale);
    gc::sunken(dst, &colours, Rect::new(r.x, r.y, r.w + ARROW, r.h), scale);
    clip(dst, r, scale);
    for (i, row) in view.tree.iter().enumerate() {
        let y = r.y + (i as u32 * ROW_HEIGHT) as i32;
        let x = r.x + 2 + i32::from(row.depth) * INDENT;
        let fg = if row.selected { sel_fg } else { item_fg };
        if row.selected {
            blit::fill_rect(dst, r.x, y, r.w, ROW_HEIGHT, sel_bg, scale);
        }
        if let Some(open) = row.expander {
            triangle(dst, x + 1, y + 4 + i32::from(!open), open, fg, scale);
        }
        if let Some(font) = font {
            let text = Rect::new(
                x + 8,
                y,
                (r.x + r.w as i32 - x - 8).max(0) as u32,
                ROW_HEIGHT,
            );
            match row.editing {
                // A field over the row while it is renamed.
                Some(caret) => {
                    blit::fill_rect(dst, text.x, y, text.w, ROW_HEIGHT, item_bg, scale);
                    let field = Rect::new(text.x, y + 1, text.w, ROW_HEIGHT - 2);
                    gc::sunken(dst, &colours, field, scale);
                    text_in(dst, font, &row.label, text, item_fg, (false, false), scale);
                    let before: String = row.label.chars().take(caret).collect();
                    let s = scale as f32;
                    let cx = ((text.x as f32 + 2.0) * s + font.width(&before, style(scale))) / s;
                    let caret_x = cx.round() as i32;
                    blit::fill_rect(dst, caret_x, y + 2, 1, ROW_HEIGHT - 4, item_fg, scale);
                }
                None => text_in(dst, font, &row.label, text, fg, (false, false), scale),
            }
        }
    }
    dst.set_clip(None, scale);
    gc::scrollbar(dst, skin, l.tree_scroll, true, &view.tree_scroll, scale);

    // Buttons.
    let button_fg = rgb(colours.button_text);
    for (id, r, label) in [
        (LIBRARY_BUTTON, l.library_button, "Library"),
        (CLEAR, l.clear, "Clear Search"),
        (PLAY, l.play, "Play"),
        (ENQUEUE, l.enqueue, "Enqueue"),
    ] {
        let down = pressed(id);
        gc::button(dst, skin, r, down, scale);
        if let Some(font) = font {
            let nudge = i32::from(down);
            let at = Rect::new(r.x + nudge, r.y + nudge, r.w, r.h);
            text_in(dst, font, label, at, button_fg, (false, true), scale);
        }
    }

    // Search.
    let text_fg = rgb(colours.window_text);
    if let Some(font) = font {
        let (lx, ly) = l.search_label;
        let label = Rect::new(lx - 2, ly - 10, 48, 13);
        text_in(dst, font, "Search:", label, text_fg, (false, false), scale);
        let f = gc::Field {
            text: &view.search,
            caret: view.caret,
            selection: view.search_selection,
            marked: view.search_marked,
        };
        gc::field(dst, font, &colours, l.search, &f, scale);
    }

    if let (Some(panes), Some(fv)) = (l.filters, &view.filters) {
        for i in 0..2 {
            let (head, list) = (panes.heads[i], panes.lists[i]);
            gc::sunken(
                dst,
                &colours,
                Rect::new(head.x, head.y, head.w + ARROW, head.h + list.h),
                scale,
            );
            header_cell(dst, &colours, head, scale);
            blit::fill_rect(dst, list.x, list.y, list.w, list.h, item_bg, scale);
            let head_fg = rgb(colours.list_header_text);
            if let Some(font) = font {
                let label = ["Artist", "Album"][i];
                text_in(dst, font, label, head, head_fg, (false, false), scale);
                text_in(dst, font, "Tracks", head, head_fg, (true, false), scale);
            }
            clip(dst, list, scale);
            for (n, row) in fv.rows[i].iter().enumerate() {
                let y = list.y + (n as u32 * ROW_HEIGHT) as i32;
                let fg = if row.selected { sel_fg } else { item_fg };
                if row.selected {
                    blit::fill_rect(dst, list.x, y, list.w, ROW_HEIGHT, sel_bg, scale);
                }
                if let Some(font) = font {
                    let at = Rect::new(list.x, y, list.w, ROW_HEIGHT);
                    let count = row.count.to_string();
                    let count_w = (font.width(&count, style(scale)) / scale as f32) as u32 + 6;
                    let name = Rect::new(list.x, y, list.w.saturating_sub(count_w), ROW_HEIGHT);
                    text_in(dst, font, &row.label, name, fg, (false, false), scale);
                    text_in(dst, font, &count, at, fg, (true, false), scale);
                }
            }
            dst.set_clip(None, scale);
            gc::scrollbar(dst, skin, panes.scrolls[i], true, &fv.scrolls[i], scale);
        }
    }

    // The list: header, then rows, both scrolled sideways together.
    let h = l.header;
    blit::fill_rect(
        dst,
        h.x,
        h.y,
        h.w,
        h.h,
        rgb(colours.list_header_dead_area),
        scale,
    );
    let r = l.list;
    blit::fill_rect(dst, r.x, r.y, r.w, r.h, item_bg, scale);
    gc::sunken(
        dst,
        &colours,
        Rect::new(h.x, h.y, h.w + ARROW, h.h + r.h + ARROW),
        scale,
    );
    let mut x = h.x - view.columns_offset as i32;
    clip(dst, h, scale);
    // What heads the list: the track columns shown, or the playlists'.
    let heads: Vec<(Option<Column>, &str, u32, bool)> = if view.listing_playlists {
        PLAYLIST_COLUMNS
            .iter()
            .map(|&(label, width, right)| (None, label, width, right))
            .collect()
    } else {
        view.columns
            .iter()
            .map(|&(i, width)| {
                let c = ALL_COLUMNS[i];
                (Some(c.0), c.2, width, c.4)
            })
            .collect()
    };
    for &(column, label, width, _) in &heads {
        let cell = Rect::new(x, h.y, width, h.h);
        header_cell(dst, &colours, cell, scale);
        let head_fg = rgb(colours.list_header_text);
        let mut text = cell;
        if let Some(sort) = view.sort.filter(|s| Some(s.column) == column) {
            // The sorted column shows its direction at its right.
            let ax = cell.x + cell.w as i32 - 9;
            let ay = cell.y + 5;
            for k in 0..3 {
                let (row, n) = if sort.descending {
                    (ay + k, 5 - 2 * k)
                } else {
                    (ay + 2 - k, 5 - 2 * k)
                };
                blit::fill_rect(dst, ax + k, row, n as u32, 1, head_fg, scale);
            }
            text.w = text.w.saturating_sub(9);
        }
        if let Some(font) = font {
            text_in(dst, font, label, text, head_fg, (false, false), scale);
        }
        x += width as i32;
    }
    if let Some(x) = view.column_drop {
        let marker = rgb(colours.list_header_text);
        blit::fill_rect(dst, x - 1, h.y, 2, h.h, marker, scale);
    }
    dst.set_clip(None, scale);
    clip(dst, r, scale);
    for (i, row) in view.rows.iter().enumerate() {
        let y = r.y + (i as u32 * ROW_HEIGHT) as i32;
        let fg = if row.selected { sel_fg } else { item_fg };
        if row.selected {
            blit::fill_rect(dst, r.x, y, r.w, ROW_HEIGHT, sel_bg, scale);
        }
        let Some(font) = font else { continue };
        let mut x = r.x - view.columns_offset as i32;
        for (cell, &(column, _, width, right)) in row.cells.iter().zip(&heads) {
            let at = Rect::new(x, y, width, ROW_HEIGHT);
            if x + (width as i32) > r.x && x < r.x + r.w as i32 {
                if column == Some(Column::Rating) {
                    let stars = cell.chars().filter(|&c| c == '*').count();
                    draw_stars(dst, at, stars, fg, scale);
                } else {
                    text_in(dst, font, cell, at, fg, (right, false), scale);
                }
            }
            x += width as i32;
        }
    }
    dst.set_clip(None, scale);
    gc::scrollbar(dst, skin, l.list_scroll, true, &view.list_scroll, scale);
    gc::scrollbar(dst, skin, l.list_hscroll, false, &view.list_hscroll, scale);
    // The square where the two scrollbars meet.
    let corner = Rect::new(l.list_scroll.x, l.list_hscroll.y, ARROW, ARROW);
    blit::fill_rect(
        dst,
        corner.x,
        corner.y,
        corner.w,
        corner.h,
        rgb(colours.scrollbar_dead_area),
        scale,
    );

    if let Some(font) = font {
        let (sx, sy) = l.status;
        let room = (inside.x + inside.w as i32 - sx).max(0) as u32;
        let r = Rect::new(sx, sy - 11, room, BUTTON_HEIGHT);
        text_in(dst, font, &view.status, r, text_fg, (false, false), scale);
    }

    let frame = (view.width, view.height, view.focused);
    gen_window::draw_frame(dst, skin, frame, TITLE, pressed(CLOSE), scale);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn widgets_stay_inside_the_window() {
        for (w, h) in [(MIN_WIDTH, MIN_HEIGHT), (500, 348)] {
            for x in widgets(w, h, DEFAULT_TREE_WIDTH, true) {
                assert!(
                    x.rect.x >= 0
                        && x.rect.y >= 0
                        && x.rect.x + x.rect.w as i32 <= w as i32
                        && x.rect.y + x.rect.h as i32 <= h as i32,
                    "{:?} at {w}x{h}",
                    x.id
                );
            }
        }
    }

    #[test]
    fn layout_fits_inside_the_frame_at_every_size() {
        for (w, h) in [(MIN_WIDTH, MIN_HEIGHT), (500, 348), (900, 667)] {
            let c = content_area(w, h);
            let l = layout(w, h, DEFAULT_TREE_WIDTH, true);
            for r in [
                l.tree,
                l.tree_scroll,
                l.library_button,
                l.search,
                l.clear,
                l.header,
                l.list,
                l.list_scroll,
                l.list_hscroll,
                l.play,
                l.enqueue,
                l.divider,
            ] {
                assert!(
                    r.x >= c.x
                        && r.y >= c.y
                        && r.x + r.w as i32 <= c.x + c.w as i32
                        && r.y + r.h as i32 <= c.y + c.h as i32,
                    "{r:?} outside {c:?} at {w}x{h}"
                );
            }
            assert!(l.search.w > 40, "room to type at {w}x{h}");
            assert!(
                visible_rows(w, h, DEFAULT_TREE_WIDTH, true).1 >= 5,
                "a list at {w}x{h}"
            );
        }
    }

    #[test]
    fn filter_panes_show_only_with_room() {
        assert!(
            layout(MIN_WIDTH, MIN_HEIGHT, DEFAULT_TREE_WIDTH, true)
                .filters
                .is_none()
        );
        let l = layout(500, 348, DEFAULT_TREE_WIDTH, true);
        let f = l.filters.expect("room at the default size");
        assert!(f.lists[0].h >= MIN_FILTER_ROWS * ROW_HEIGHT);
        assert!(f.lists[0].x + (f.lists[0].w + ARROW) as i32 <= f.lists[1].x);
        assert!(
            f.lists[0].y + (f.lists[0].h as i32) < l.header.y,
            "the list below"
        );
        assert!(
            layout(500, 348, DEFAULT_TREE_WIDTH, false)
                .filters
                .is_none()
        );
    }

    #[test]
    fn clicks_land_on_stars() {
        assert_eq!(star_at(0), 1);
        assert_eq!(star_at(STAR_LEFT + STAR_STEP), 2);
        assert_eq!(star_at(STAR_LEFT + 4 * STAR_STEP + 2), 5);
        assert_eq!(star_at(200), 5);
    }

    #[test]
    fn the_tree_keeps_within_bounds() {
        assert_eq!(clamp_tree_width(500, 10), MIN_TREE_WIDTH);
        // 500 wide: 481 inside, less the list's 120 and the gap.
        assert_eq!(clamp_tree_width(500, 900), 481 - 120 - 6);
        assert_eq!(clamp_tree_width(500, 200), 200);
    }

    #[test]
    fn the_default_columns_are_the_shown_ones() {
        let cols = default_columns();
        assert_eq!(cols.len(), 10);
        assert_eq!(cols[0], (0, 100));
        assert_eq!(columns_width(&cols), 676);
    }
}
