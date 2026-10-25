//! The frame of a generic skinned window, from `gen.bmp`: the library,
//! Jump to File, File Info.
//!
//! The frame follows Webamp's `GenWindow` and `gen-window.css` (MIT,
//! Copyright (c) 2015 Jordan Eldredge): a 20px title bar built from corner,
//! fill and end-cap pieces around the title, which is spelled in the
//! skin's own alphabet; 11px and 8px sides; a 14px bottom. Checked pixel for
//! pixel against Winamp's own library window in the base skin.

use lantern_skin::{GenFont, Skin, SpriteKey};

use super::blit::{self, Surface};
use super::geometry::Rect;
use super::main_window::draw_sprite;
use super::{Widget, WidgetId, WidgetKind};

pub const TOP: u32 = 20;
pub const BOTTOM: u32 = 14;
pub const LEFT: u32 = 11;
pub const RIGHT: u32 = 8;
/// Every title-bar piece, and the bottom corners' overlap allowance.
const PIECE: i32 = 25;
/// Space either side of the title, between it and the end caps. Measured
/// against Winamp; Webamp's (4, 3) sets the caps 4px too close.
const TITLE_PAD: (i32, i32) = (8, 7);
/// The title's letters sit this far down the bar.
const TITLE_Y: i32 = 4;

/// The frame's own widgets: the title bar to drag by, the close button,
/// and if `resizable`, the grip in the bottom-right corner. Ids are the
/// window's own, in that order.
pub fn frame_widgets(width: u32, height: u32, ids: [WidgetId; 3], resizable: bool) -> Vec<Widget> {
    let w = |id, x, y, width, height, kind| Widget::new(id, Rect::new(x, y, width, height), kind);
    let (wi, hi) = (width as i32, height as i32);
    let mut v = vec![
        w(ids[0], 0, 0, width, TOP, WidgetKind::HitZone),
        w(ids[1], wi - 11, 3, 9, 9, WidgetKind::Button),
    ];
    if resizable {
        v.push(w(ids[2], wi - 20, hi - 20, 20, 20, WidgetKind::Slider));
    }
    v
}

/// Where the window's contents go: inside the frame.
pub fn content_area(width: u32, height: u32) -> Rect {
    Rect::new(
        LEFT as i32,
        TOP as i32,
        width - LEFT - RIGHT,
        height - TOP - BOTTOM,
    )
}

/// Repeat `key` along `y` to fill `from..to`, clipping at both ends. Tiles
/// start at `from`, or with `from_right`, end flush at `to` — Webamp's
/// `background-position: right`.
#[allow(clippy::too_many_arguments)]
fn tile_x(
    dst: &mut Surface<'_>,
    skin: &Skin,
    key: SpriteKey,
    (from, to): (i32, i32),
    y: i32,
    from_right: bool,
    scale: u32,
) {
    let step = skin.resolve(key).1.w as i32;
    if step == 0 || to <= from {
        return;
    }
    let h = skin.resolve(key).1.h;
    let clip = lantern_skin::Rect::new(from.max(0) as u32, y.max(0) as u32, (to - from) as u32, h);
    dst.set_clip(Some(clip), scale);
    let span = to - from;
    let mut x = if from_right {
        to - (span + step - 1) / step * step
    } else {
        from
    };
    while x < to {
        draw_sprite(dst, skin, key, x, y, scale);
        x += step;
    }
    dst.set_clip(None, scale);
}

/// Repeat `key` down `x` to fill `from..to`, clipping the last copy.
fn tile_y(
    dst: &mut Surface<'_>,
    skin: &Skin,
    key: SpriteKey,
    x: i32,
    (from, to): (i32, i32),
    scale: u32,
) {
    let (_, src) = skin.resolve(key);
    if src.h == 0 || to <= from {
        return;
    }
    let clip = lantern_skin::Rect::new(
        x.max(0) as u32,
        from.max(0) as u32,
        src.w,
        (to - from) as u32,
    );
    dst.set_clip(Some(clip), scale);
    let mut y = from;
    while y < to {
        draw_sprite(dst, skin, key, x, y, scale);
        y += src.h as i32;
    }
    dst.set_clip(None, scale);
}

/// Where the title bar's pieces go for a title `title_w` wide: the two
/// fills share what is left over, the right one taking any odd pixel.
/// Returns the left fill's width and the title's left edge.
fn title_layout(width: i32, title_w: i32) -> (i32, i32) {
    let label = title_w + TITLE_PAD.0 + TITLE_PAD.1;
    let fills = (width - 4 * PIECE - label).max(0);
    let left_fill = fills / 2;
    (left_fill, 2 * PIECE + left_fill + TITLE_PAD.0)
}

/// The frame around a generic window `(width, height, focused)`, titled
/// `title`, its close button held down if `close_pressed`.
pub fn draw_frame(
    dst: &mut Surface<'_>,
    skin: &Skin,
    (width, height, active): (u32, u32, bool),
    title: &str,
    close_pressed: bool,
    scale: u32,
) {
    let (w, h) = (width as i32, height as i32);
    let font: &GenFont = skin.gen_font();
    let sprite = |dst: &mut Surface<'_>, key, x, y| draw_sprite(dst, skin, key, x, y, scale);

    // Top: the centre fill runs behind everything, showing only around the
    // title; corners, fills and caps go over it.
    let title_w = font.width(title, active) as i32;
    let (left_fill, title_x) = title_layout(w, title_w);
    let right_end = title_x + title_w + TITLE_PAD.1;
    tile_x(
        dst,
        skin,
        SpriteKey::GenTopCenterFill { active },
        (0, w),
        0,
        false,
        scale,
    );
    let fill = SpriteKey::GenTopLeftRightFill { active };
    tile_x(dst, skin, fill, (PIECE, PIECE + left_fill), 0, false, scale);
    tile_x(
        dst,
        skin,
        fill,
        (right_end + PIECE, w - PIECE),
        0,
        true,
        scale,
    );
    sprite(dst, SpriteKey::GenTopLeft { active }, 0, 0);
    sprite(
        dst,
        SpriteKey::GenTopLeftEnd { active },
        PIECE + left_fill,
        0,
    );
    sprite(dst, SpriteKey::GenTopRightEnd { active }, right_end, 0);
    sprite(dst, SpriteKey::GenTopRight { active }, w - PIECE, 0);
    draw_title(dst, skin, title, title_x, active, scale);

    // Sides: tiled down from the top, with their bottom pieces flush with
    // the bottom bar.
    let (top, bottom) = (TOP as i32, h - BOTTOM as i32);
    let right = w - RIGHT as i32;
    tile_y(dst, skin, SpriteKey::GenMiddleLeft, 0, (top, bottom), scale);
    tile_y(
        dst,
        skin,
        SpriteKey::GenMiddleRight,
        right,
        (top, bottom),
        scale,
    );
    let foot = bottom - skin.resolve(SpriteKey::GenMiddleLeftBottom).1.h as i32;
    if foot >= top {
        sprite(dst, SpriteKey::GenMiddleLeftBottom, 0, foot);
        sprite(dst, SpriteKey::GenMiddleRightBottom, right, foot);
    }

    // Bottom: filled, then the corners at either end.
    tile_x(
        dst,
        skin,
        SpriteKey::GenBottomFill,
        (0, w),
        bottom,
        false,
        scale,
    );
    sprite(dst, SpriteKey::GenBottomLeft, 0, bottom);
    let corner = skin.resolve(SpriteKey::GenBottomRight).1.w as i32;
    sprite(dst, SpriteKey::GenBottomRight, w - corner, bottom);
    if close_pressed {
        sprite(dst, SpriteKey::GenClosePressed, w - 11, 3);
    }
}

fn draw_title(dst: &mut Surface<'_>, skin: &Skin, title: &str, x: i32, active: bool, scale: u32) {
    let Some(sheet) = skin.sheet(lantern_skin::SheetId::Gen) else {
        return;
    };
    let mut x = x;
    for c in title.chars() {
        match skin.gen_font().letter(c, active) {
            Some(r) => {
                let s = scale as i32;
                blit::blit(dst, sheet, r, x * s, TITLE_Y * s, scale);
                x += r.w as i32;
            }
            None => x += GenFont::SPACE as i32,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_is_centred_between_the_caps() {
        // 500 wide, a 100px title: 285px of fill, 142 left and 143 right.
        let (left_fill, x) = title_layout(500, 100);
        assert_eq!(left_fill, 142);
        assert_eq!(x, 50 + 142 + 8);
        let right_end = x + 100 + 7;
        assert_eq!(500 - 25 - (right_end + 25), 143);
    }

    #[test]
    fn an_overlong_title_squeezes_the_fills_to_nothing() {
        assert_eq!(title_layout(275, 400), (0, 58));
    }

    #[test]
    fn content_fills_the_frame() {
        let r = content_area(500, 348);
        assert_eq!((r.x, r.y, r.w, r.h), (11, 20, 481, 314));
    }
}
