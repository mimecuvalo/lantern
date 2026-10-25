//! Jump to File: Winamp's J. A field to type in, the playlist's entries
//! that match below it, and buttons to jump to the one picked or close.
//! A generic window, framed from `gen.bmp` like the library.

use lantern_skin::Skin;

use super::blit::{self, Surface};
use super::font::TextFont;
use super::gen_controls::{self as gc, ARROW, Field, ScrollState, clip, rgb, text_in};
use super::gen_window::{self, content_area};
use super::geometry::Rect;
use super::main_window::NO_SKIN_BACKGROUND;
use super::{Widget, WidgetId, WidgetKind};

pub const TITLE_DRAG: WidgetId = WidgetId(400);
pub const CLOSE: WidgetId = WidgetId(401);
pub const FIELD: WidgetId = WidgetId(402);
pub const LIST: WidgetId = WidgetId(403);
pub const SCROLL: WidgetId = WidgetId(404);
pub const JUMP: WidgetId = WidgetId(405);
pub const DISMISS: WidgetId = WidgetId(406);

pub const WIDTH: u32 = 325;
pub const HEIGHT: u32 = 261;
pub const TITLE: &str = "Jump to File";

pub const ROW_HEIGHT: u32 = 13;
const BUTTON_HEIGHT: u32 = 15;

pub struct Layout {
    pub field: Rect,
    pub list: Rect,
    pub scroll: Rect,
    pub jump: Rect,
    pub dismiss: Rect,
}

pub fn layout() -> Layout {
    let c = content_area(WIDTH, HEIGHT);
    let bottom = c.y + c.h as i32 - BUTTON_HEIGHT as i32;
    let field = Rect::new(c.x + 1, c.y + 1, c.w - 2, 13);
    let list_y = field.y + field.h as i32 + 6;
    let list_h = ((bottom - 5 - list_y) as u32 / ROW_HEIGHT) * ROW_HEIGHT;
    let list = Rect::new(c.x + 1, list_y, c.w - 2 - ARROW, list_h);
    Layout {
        field,
        list,
        scroll: Rect::new(list.x + list.w as i32, list_y, ARROW, list_h),
        jump: Rect::new(c.x, bottom, 80, BUTTON_HEIGHT),
        dismiss: Rect::new(c.x + c.w as i32 - 60, bottom, 60, BUTTON_HEIGHT),
    }
}

/// Whole rows in the list.
pub fn visible_rows() -> usize {
    (layout().list.h / ROW_HEIGHT) as usize
}

pub fn widgets() -> Vec<Widget> {
    let l = layout();
    let ids = [TITLE_DRAG, CLOSE, WidgetId(0)];
    let mut v = gen_window::frame_widgets(WIDTH, HEIGHT, ids, false);
    use WidgetKind::{Button, HitZone, Slider};
    v.extend([
        Widget::new(FIELD, l.field, Slider),
        Widget::new(LIST, l.list, HitZone),
        Widget::new(SCROLL, l.scroll, Slider),
        Widget::new(JUMP, l.jump, Button),
        Widget::new(DISMISS, l.dismiss, Button),
    ]);
    v
}

pub struct JumpView<'a> {
    pub focused: bool,
    pub field: Field<'a>,
    /// The matches in view, top first: each line, and whether it is picked.
    pub rows: Vec<(String, bool)>,
    pub scroll: ScrollState,
}

pub fn draw(
    dst: &mut Surface<'_>,
    skin: Option<&Skin>,
    font: Option<&TextFont>,
    view: &JumpView<'_>,
    widgets: &[Widget],
    scale: u32,
) {
    blit::clear(dst, NO_SKIN_BACKGROUND);
    let Some(skin) = skin else { return };
    let colours = *skin.genex_colors();
    let c = content_area(WIDTH, HEIGHT);
    let bg = rgb(colours.window_background);
    blit::fill_rect(dst, c.x, c.y, c.w, c.h, bg, scale);
    let l = layout();
    let pressed = |id| widgets.iter().any(|w| w.id == id && w.pressed);

    if let Some(font) = font {
        gc::field(dst, font, &colours, l.field, &view.field, scale);
    }
    let r = l.list;
    blit::fill_rect(dst, r.x, r.y, r.w, r.h, rgb(colours.item_background), scale);
    gc::sunken(dst, &colours, Rect::new(r.x, r.y, r.w + ARROW, r.h), scale);
    clip(dst, r, scale);
    for (i, (line, picked)) in view.rows.iter().enumerate() {
        let y = r.y + (i as u32 * ROW_HEIGHT) as i32;
        let fg = if *picked {
            blit::fill_rect(
                dst,
                r.x,
                y,
                r.w,
                ROW_HEIGHT,
                rgb(colours.list_background_selected),
                scale,
            );
            rgb(colours.list_text_selected)
        } else {
            rgb(colours.item_foreground)
        };
        if let Some(font) = font {
            let at = Rect::new(r.x, y, r.w, ROW_HEIGHT);
            text_in(dst, font, line, at, fg, (false, false), scale);
        }
    }
    dst.set_clip(None, scale);
    gc::scrollbar(dst, skin, l.scroll, true, &view.scroll, scale);
    gc::labelled_button(
        dst,
        skin,
        font,
        l.jump,
        "Jump to File",
        pressed(JUMP),
        scale,
    );
    gc::labelled_button(dst, skin, font, l.dismiss, "Close", pressed(DISMISS), scale);

    let frame = (WIDTH, HEIGHT, view.focused);
    gen_window::draw_frame(dst, skin, frame, TITLE, pressed(CLOSE), scale);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn everything_fits_inside_the_frame() {
        let c = content_area(WIDTH, HEIGHT);
        let l = layout();
        for r in [l.field, l.list, l.scroll, l.jump, l.dismiss] {
            assert!(
                r.x >= c.x
                    && r.y >= c.y
                    && r.x + r.w as i32 <= c.x + c.w as i32
                    && r.y + r.h as i32 <= c.y + c.h as i32,
                "{r:?} outside {c:?}"
            );
        }
        assert!(visible_rows() >= 10);
        assert!(l.jump.x + (l.jump.w as i32) < l.dismiss.x);
    }
}
