//! The playlist window: its resizable nine-tile frame, the track list, and
//! the bottom bar's displays and mini transport.
//!
//! Geometry follows Webamp's `playlist-window.css` (MIT, Copyright (c) 2015
//! Jordan Eldredge). The window resizes in whole tiles: 25px across, 29px
//! down, from a 275x116 minimum.

use lantern_skin::{PlaylistButton, PlaylistMenu, PlaylistStyle, Skin, SpriteKey};

use super::blit::{self, Surface};
use super::font::{TextFont, TextStyle};
use super::geometry::Rect;
use super::main_window::{NO_SKIN_BACKGROUND, draw_sprite, draw_text};
use super::{Widget, WidgetId, WidgetKind};

pub const TITLE_DRAG: WidgetId = WidgetId(200);
pub const SHADE: WidgetId = WidgetId(201);
pub const CLOSE: WidgetId = WidgetId(202);
pub const RESIZE: WidgetId = WidgetId(203);
pub const LIST: WidgetId = WidgetId(204);
pub const SCROLLBAR: WidgetId = WidgetId(205);
pub const SCROLL_UP: WidgetId = WidgetId(206);
pub const SCROLL_DOWN: WidgetId = WidgetId(207);
pub const MENU_ADD: WidgetId = WidgetId(210);
pub const MENU_REMOVE: WidgetId = WidgetId(211);
pub const MENU_SELECT: WidgetId = WidgetId(212);
pub const MENU_MISC: WidgetId = WidgetId(213);
pub const MENU_LIST: WidgetId = WidgetId(214);
pub const MINI_PREVIOUS: WidgetId = WidgetId(220);
pub const MINI_PLAY: WidgetId = WidgetId(221);
pub const MINI_PAUSE: WidgetId = WidgetId(222);
pub const MINI_STOP: WidgetId = WidgetId(223);
pub const MINI_NEXT: WidgetId = WidgetId(224);
pub const MINI_EJECT: WidgetId = WidgetId(225);

pub const MIN_WIDTH: u32 = 275;
pub const MIN_HEIGHT: u32 = 116;
pub const WIDTH_STEP: u32 = 25;
pub const HEIGHT_STEP: u32 = 29;
/// One track per 13px row.
pub const ROW_HEIGHT: u32 = 13;

const TOP: u32 = 20;
const BOTTOM: u32 = 38;
const LEFT: u32 = 12;
const RIGHT: u32 = 20;
const SCROLL_HANDLE_HEIGHT: u32 = 18;

/// Text in 9px type with half a pixel of letter spacing, as Webamp sets it.
const FONT_PX: f32 = 9.0;
const LETTER_SPACING: f32 = 0.5;

/// Snap a requested size to the frame's tile grid.
pub fn snap_size(width: i32, height: i32) -> (u32, u32) {
    let snap = |v: i32, min: u32, step: u32| {
        let extra = (v - min as i32).max(0) as u32;
        min + (extra + step / 2) / step * step
    };
    (
        snap(width, MIN_WIDTH, WIDTH_STEP),
        snap(height, MIN_HEIGHT, HEIGHT_STEP),
    )
}

pub fn widgets(width: u32, height: u32) -> Vec<Widget> {
    let w = |id, x, y, width, height, kind| Widget::new(id, Rect::new(x, y, width, height), kind);
    let (wi, hi) = (width as i32, height as i32);
    let list = list_area(width, height);
    let bottom = hi - BOTTOM as i32;
    // The bottom-right panel is 150px wide at the window's right edge.
    let panel = wi - 150;
    use WidgetKind::{Button, HitZone, Slider};
    let mut v = vec![
        w(TITLE_DRAG, 0, 0, width, TOP, HitZone),
        w(SHADE, wi - 21, 3, 9, 9, Button),
        w(CLOSE, wi - 11, 3, 9, 9, Button),
        w(LIST, list.x, list.y, list.w, list.h, HitZone),
        w(
            SCROLLBAR,
            wi - 15,
            TOP as i32,
            8,
            height - TOP - BOTTOM,
            Slider,
        ),
        w(MENU_ADD, 14, bottom + 8, 22, 18, HitZone),
        w(MENU_REMOVE, 43, bottom + 8, 22, 18, HitZone),
        w(MENU_SELECT, 72, bottom + 8, 22, 18, HitZone),
        w(MENU_MISC, 101, bottom + 8, 22, 18, HitZone),
        w(MENU_LIST, wi - 44, bottom + 8, 22, 18, HitZone),
        w(SCROLL_UP, wi - 15, bottom + 2, 8, 5, HitZone),
        w(SCROLL_DOWN, wi - 15, bottom + 8, 8, 5, HitZone),
    ];
    let mini = [
        MINI_PREVIOUS,
        MINI_PLAY,
        MINI_PAUSE,
        MINI_STOP,
        MINI_NEXT,
        MINI_EJECT,
    ];
    for (i, id) in mini.into_iter().enumerate() {
        v.push(w(
            id,
            panel + 3 + 10 * i as i32,
            bottom + 22,
            10,
            10,
            HitZone,
        ));
    }
    // Last, so it wins over the scroll buttons it overlaps.
    v.push(w(RESIZE, wi - 20, hi - 20, 20, 20, Slider));
    v
}

/// The playlist in windowshade mode: a strip as wide as the window, which
/// resizes sideways only.
pub fn shade_widgets(width: u32) -> Vec<Widget> {
    let w = |id, x, y, width, height, kind| Widget::new(id, Rect::new(x, y, width, height), kind);
    let wi = width as i32;
    use WidgetKind::{Button, HitZone, Slider};
    vec![
        w(TITLE_DRAG, 0, 0, width, 14, HitZone),
        w(RESIZE, wi - 29, 3, 9, 9, Slider),
        w(SHADE, wi - 21, 3, 9, 9, Button),
        w(CLOSE, wi - 11, 3, 9, 9, Button),
    ]
}

/// The shaded playlist: the current track's title and length in the
/// skin's bitmap font, cut to fit as Webamp's `PlaylistShade` does.
pub fn draw_shade(
    dst: &mut Surface<'_>,
    skin: Option<&Skin>,
    width: u32,
    focused: bool,
    (title, time): (&str, &str),
    widgets: &[Widget],
    scale: u32,
) {
    blit::clear(dst, NO_SKIN_BACKGROUND);
    let Some(skin) = skin else { return };
    let w = width as i32;
    tile(
        dst,
        skin,
        SpriteKey::PlaylistShadeBackground,
        (25, 0),
        true,
        w - 50,
        scale,
    );
    draw_sprite(
        dst,
        skin,
        SpriteKey::PlaylistShadeBackgroundLeft,
        0,
        0,
        scale,
    );
    let right = SpriteKey::PlaylistShadeBackgroundRight { active: focused };
    draw_sprite(dst, skin, right, w - 50, 0, scale);

    let max_chars = ((205 + width.saturating_sub(MIN_WIDTH)) / 5) as usize;
    let title: String = if title.chars().count() > max_chars {
        let mut t: String = title.chars().take(max_chars.saturating_sub(1)).collect();
        t.push('\u{2026}');
        t
    } else {
        title.to_string()
    };
    draw_text(dst, skin, &title, 5, 4, scale);
    let time_x = w - 30 - (time.chars().count() as i32 * 5);
    draw_text(dst, skin, time, time_x, 4, scale);

    for x in widgets {
        let key = match x.id {
            CLOSE if x.pressed => SpriteKey::PlaylistCloseButton,
            SHADE if x.pressed => SpriteKey::PlaylistExpandButton,
            _ => continue,
        };
        draw_sprite(dst, skin, key, x.rect.x, x.rect.y, scale);
    }
}

/// Where the track list sits: inside the frame, with 3px of padding above
/// and below.
pub fn list_area(width: u32, height: u32) -> Rect {
    Rect::new(
        LEFT as i32,
        TOP as i32 + 3,
        width - LEFT - RIGHT,
        height - TOP - BOTTOM - 6,
    )
}

/// Whole rows that fit in the list.
pub fn visible_rows(width: u32, height: u32) -> usize {
    (list_area(width, height).h / ROW_HEIGHT) as usize
}

/// Scroll fraction for a pointer `local_y` pixels below the scrollbar's top.
pub fn scroll_from_y(local_y: i32, height: u32) -> f32 {
    let travel = (height - TOP - BOTTOM - SCROLL_HANDLE_HEIGHT) as f32;
    ((local_y as f32 - SCROLL_HANDLE_HEIGHT as f32 / 2.0) / travel).clamp(0.0, 1.0)
}

/// The menu a bottom-bar button opens.
pub fn menu_for(id: WidgetId) -> Option<PlaylistMenu> {
    Some(match id {
        MENU_ADD => PlaylistMenu::Add,
        MENU_REMOVE => PlaylistMenu::Remove,
        MENU_SELECT => PlaylistMenu::Select,
        MENU_MISC => PlaylistMenu::Misc,
        MENU_LIST => PlaylistMenu::List,
        _ => return None,
    })
}

/// A menu's items, top to bottom, in Webamp's order.
pub fn menu_items(menu: PlaylistMenu) -> &'static [PlaylistButton] {
    use PlaylistButton as B;
    match menu {
        PlaylistMenu::Add => &[B::AddUrl, B::AddDir, B::AddFile],
        PlaylistMenu::Remove => &[B::RemoveMisc, B::RemoveAll, B::Crop, B::RemoveSelected],
        PlaylistMenu::Select => &[B::InvertSelection, B::SelectZero, B::SelectAll],
        PlaylistMenu::Misc => &[B::SortList, B::FileInfo, B::MiscOptions],
        PlaylistMenu::List => &[B::NewList, B::SaveList, B::LoadList],
    }
}

/// Where each item of an open menu sits: a column of 22x18 buttons
/// standing on the menu's own button, so the last item covers it.
pub fn menu_item_rects(menu: PlaylistMenu, width: u32, height: u32) -> Vec<Rect> {
    let id = match menu {
        PlaylistMenu::Add => MENU_ADD,
        PlaylistMenu::Remove => MENU_REMOVE,
        PlaylistMenu::Select => MENU_SELECT,
        PlaylistMenu::Misc => MENU_MISC,
        PlaylistMenu::List => MENU_LIST,
    };
    let Some(anchor) = widgets(width, height).into_iter().find(|w| w.id == id) else {
        return Vec::new();
    };
    let n = menu_items(menu).len() as i32;
    (0..n)
        .map(|i| Rect::new(anchor.rect.x, anchor.rect.y - (n - 1 - i) * 18, 22, 18))
        .collect()
}

/// An open bottom-bar menu, and the item under the pointer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OpenMenu {
    pub menu: PlaylistMenu,
    pub hover: Option<usize>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Row {
    /// 1-based position in the playlist.
    pub number: usize,
    pub title: String,
    pub duration: String,
    pub selected: bool,
    pub current: bool,
}

#[derive(Debug, Clone, Default)]
pub struct PlaylistView {
    pub focused: bool,
    pub width: u32,
    pub height: u32,
    /// Only the rows in view, top first.
    pub rows: Vec<Row>,
    /// Where the scroll handle sits, 0.0..=1.0.
    pub scroll: f32,
    /// "selected/total", e.g. "3:45/1:02:13".
    pub running_time: String,
    /// The playing track's time, blank when stopped.
    pub mini_time: String,
    pub menu: Option<OpenMenu>,
}

pub fn draw(
    dst: &mut Surface<'_>,
    skin: Option<&Skin>,
    font: Option<&TextFont>,
    view: &PlaylistView,
    widgets: &[Widget],
    scale: u32,
) {
    blit::clear(dst, NO_SKIN_BACKGROUND);
    let Some(skin) = skin else { return };
    draw_list(dst, &skin.playlist_style, font, view, scale);
    draw_frame(dst, skin, view, scale);

    let (w, h) = (view.width as i32, view.height as i32);
    let track = h - (TOP + BOTTOM + SCROLL_HANDLE_HEIGHT) as i32;
    let handle_y = TOP as i32 + (view.scroll * track as f32).round() as i32;
    let pressed = widgets.iter().any(|x| x.id == SCROLLBAR && x.pressed);
    draw_sprite(
        dst,
        skin,
        SpriteKey::PlaylistScrollHandle { pressed },
        w - 15,
        handle_y,
        scale,
    );

    let bottom = h - BOTTOM as i32;
    draw_text(dst, skin, &view.running_time, w - 143, bottom + 10, scale);
    draw_text(dst, skin, &view.mini_time, w - 84, bottom + 23, scale);

    for x in widgets {
        let key = match x.id {
            CLOSE if x.pressed => SpriteKey::PlaylistCloseButton,
            SHADE if x.pressed => SpriteKey::PlaylistCollapseButton,
            _ => continue,
        };
        draw_sprite(dst, skin, key, x.rect.x, x.rect.y, scale);
    }

    if let Some(open) = view.menu {
        let rects = menu_item_rects(open.menu, view.width, view.height);
        if let (Some(first), Some(last)) = (rects.first(), rects.last()) {
            let bar_top = last.y + last.h as i32
                - skin.resolve(SpriteKey::PlaylistMenuBar(open.menu)).1.h as i32;
            draw_sprite(
                dst,
                skin,
                SpriteKey::PlaylistMenuBar(open.menu),
                first.x - 3,
                bar_top,
                scale,
            );
        }
        for (i, (r, &button)) in rects.iter().zip(menu_items(open.menu)).enumerate() {
            let pressed = open.hover == Some(i);
            draw_sprite(
                dst,
                skin,
                SpriteKey::PlaylistButton { button, pressed },
                r.x,
                r.y,
                scale,
            );
        }
    }
}

fn rgb([r, g, b]: [u8; 3]) -> u32 {
    u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b)
}

/// The track list: `normalbg` behind everything, `selectedbg` behind
/// selected rows, `current` for the playing track's text and `normal` for
/// the rest. Titles are cut with an ellipsis; durations right-align.
fn draw_list(
    dst: &mut Surface<'_>,
    style: &PlaylistStyle,
    font: Option<&TextFont>,
    view: &PlaylistView,
    scale: u32,
) {
    // The background runs under the frame's inner edges too, so no seam
    // shows between frame and list.
    let inner = Rect::new(
        LEFT as i32,
        TOP as i32,
        view.width - LEFT - RIGHT,
        view.height - TOP - BOTTOM,
    );
    blit::fill_rect(
        dst,
        inner.x,
        inner.y,
        inner.w,
        inner.h,
        rgb(style.normal_bg),
        scale,
    );
    let Some(font) = font else { return };

    let list = list_area(view.width, view.height);
    let s = scale as f32;
    let text_style = TextStyle {
        px: FONT_PX * s,
        spacing: LETTER_SPACING * s,
    };
    let clip = lantern_skin::Rect::new(list.x as u32, list.y as u32, list.w, list.h);
    dst.set_clip(Some(clip), scale);
    for (i, row) in view.rows.iter().enumerate() {
        let y = list.y + (i as u32 * ROW_HEIGHT) as i32;
        if row.selected {
            blit::fill_rect(
                dst,
                list.x,
                y,
                list.w,
                ROW_HEIGHT,
                rgb(style.selected_bg),
                scale,
            );
        }
        let colour = rgb(if row.current {
            style.current
        } else {
            style.normal
        });
        let baseline = (y + 10) * scale as i32;

        let duration_w = font.width(&row.duration, text_style);
        let right = (list.x + list.w as i32 - 3) as f32 * s;
        font.draw(
            dst,
            &row.duration,
            (right - duration_w, baseline),
            text_style,
            colour,
        );

        let left = (list.x + 2) as f32 * s;
        let room = right - duration_w - left - 4.0 * s;
        let text = font.fit(&format!("{}. {}", row.number, row.title), text_style, room);
        font.draw(dst, &text, (left, baseline), text_style, colour);
    }
    dst.set_clip(None, scale);
}

/// Repeat `key` from `from` to `to` along one axis, clipping the last copy.
fn tile(
    dst: &mut Surface<'_>,
    skin: &Skin,
    key: SpriteKey,
    (x, y): (i32, i32),
    along_x: bool,
    to: i32,
    scale: u32,
) {
    let (_, src) = skin.resolve(key);
    let step = if along_x { src.w } else { src.h } as i32;
    if step == 0 {
        return;
    }
    let (cw, ch) = if along_x {
        ((to - x).max(0) as u32, src.h)
    } else {
        (src.w, (to - y).max(0) as u32)
    };
    let clip = lantern_skin::Rect::new(x.max(0) as u32, y.max(0) as u32, cw, ch);
    dst.set_clip(Some(clip), scale);
    let mut p = if along_x { x } else { y };
    while p < to {
        let (px, py) = if along_x { (p, y) } else { (x, p) };
        draw_sprite(dst, skin, key, px, py, scale);
        p += step;
    }
    dst.set_clip(None, scale);
}

fn draw_frame(dst: &mut Surface<'_>, skin: &Skin, view: &PlaylistView, scale: u32) {
    let (w, h) = (view.width as i32, view.height as i32);
    let active = view.focused;
    let sprite = |dst: &mut Surface<'_>, key, x, y| draw_sprite(dst, skin, key, x, y, scale);

    // Top: corners, a 100px title centred in the fill between them.
    let title_x = 37 + (w - 175) / 2;
    let top_tile = SpriteKey::PlaylistTopTile { active };
    tile(dst, skin, top_tile, (25, 0), true, w - 25, scale);
    sprite(dst, SpriteKey::PlaylistTopLeftCorner { active }, 0, 0);
    sprite(dst, SpriteKey::PlaylistTitleBar { active }, title_x, 0);
    sprite(dst, SpriteKey::PlaylistTopRightCorner { active }, w - 25, 0);

    // Sides.
    let (top, bottom) = (TOP as i32, h - BOTTOM as i32);
    let right = w - RIGHT as i32;
    tile(
        dst,
        skin,
        SpriteKey::PlaylistLeftTile,
        (0, top),
        false,
        bottom,
        scale,
    );
    tile(
        dst,
        skin,
        SpriteKey::PlaylistRightTile,
        (right, top),
        false,
        bottom,
        scale,
    );

    // Bottom: fixed corners, filled between; the visualiser panel appears
    // once the window is wide enough to hold it.
    let bottom_tile = SpriteKey::PlaylistBottomTile;
    tile(dst, skin, bottom_tile, (125, bottom), true, w - 150, scale);
    sprite(dst, SpriteKey::PlaylistBottomLeftCorner, 0, bottom);
    if w >= 350 {
        sprite(
            dst,
            SpriteKey::PlaylistVisualizerBackground,
            w - 225,
            bottom,
        );
    }
    sprite(dst, SpriteKey::PlaylistBottomRightCorner, w - 150, bottom);
}

/// `m:ss`, or `h:mm:ss` from an hour up, as the list and running time show.
/// Seconds are cut, not rounded, as Winamp does: 3:45.9 is 3:45.
pub fn clock(secs: f64) -> String {
    let s = secs.max(0.0) as u64;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_snap_to_whole_tiles() {
        assert_eq!(snap_size(100, 50), (275, 116), "never below the minimum");
        assert_eq!(snap_size(290, 232), (300, 232), "232 = 116 + 4 x 29");
        assert_eq!(snap_size(287, 130), (275, 116), "rounds to nearest");
    }

    #[test]
    fn default_title_sits_where_webamp_puts_it() {
        // 25 corner + 12 spacer + 50 of fill = 87 at the default width.
        assert_eq!(37 + (275 - 175) / 2, 87);
    }

    #[test]
    fn default_window_shows_a_dozen_rows() {
        assert_eq!(visible_rows(275, 232), 12);
        assert_eq!(visible_rows(275, 116), 4, "52px of list, exactly 4 rows");
    }

    #[test]
    fn clock_switches_to_hours() {
        assert_eq!(clock(225.4), "3:45");
        assert_eq!(clock(3725.0), "1:02:05");
    }

    #[test]
    fn menus_stand_on_their_buttons() {
        for menu in [
            PlaylistMenu::Add,
            PlaylistMenu::Remove,
            PlaylistMenu::Select,
            PlaylistMenu::Misc,
            PlaylistMenu::List,
        ] {
            let rects = menu_item_rects(menu, 275, 232);
            assert_eq!(rects.len(), menu_items(menu).len());
            let last = rects.last().unwrap();
            assert_eq!(
                last.y + 18,
                232 - 38 + 8 + 18,
                "{menu:?} ends on its button"
            );
            assert!(rects[0].y >= 0, "{menu:?} fits above the bar");
        }
    }

    #[test]
    fn widgets_stay_inside_the_window_at_every_size() {
        for (w, h) in [(275, 116), (275, 232), (400, 348)] {
            for x in widgets(w, h) {
                assert!(
                    x.rect.x >= 0
                        && x.rect.y >= 0
                        && x.rect.x + x.rect.w as i32 <= w as i32
                        && x.rect.y + x.rect.h as i32 <= h as i32,
                    "{:?} at {w}x{h}: {:?}",
                    x.id,
                    x.rect
                );
            }
        }
    }
}
