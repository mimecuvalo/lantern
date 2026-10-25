//! File Info: a file's tags, to read and change, and what it is — format,
//! length, bitrate and the rest — what the clutter bar's I opens. A generic
//! window, framed from
//! `gen.bmp` like the library.

use lantern_skin::Skin;

use super::blit::{self, Surface};
use super::font::TextFont;
use super::gen_controls::{self as gc, Field, rgb, text_in};
use super::gen_window::{self, content_area};
use super::geometry::Rect;
use super::main_window::NO_SKIN_BACKGROUND;
use super::{Widget, WidgetId, WidgetKind};

pub const TITLE_DRAG: WidgetId = WidgetId(500);
pub const CLOSE: WidgetId = WidgetId(501);
pub const SAVE: WidgetId = WidgetId(502);
pub const DISMISS: WidgetId = WidgetId(503);
/// The fields, in [`LABELS`] order.
pub const FIELDS: [WidgetId; 8] = [
    WidgetId(510),
    WidgetId(511),
    WidgetId(512),
    WidgetId(513),
    WidgetId(514),
    WidgetId(515),
    WidgetId(516),
    WidgetId(517),
];

/// What each field holds.
pub const LABELS: [&str; 8] = [
    "Title",
    "Artist",
    "Album",
    "Album Artist",
    "Genre",
    "Year",
    "Track #",
    "Disc #",
];

pub const WIDTH: u32 = 375;
pub const HEIGHT: u32 = 319;
pub const TITLE: &str = "File Info";

const LABEL_WIDTH: u32 = 70;
const FIELD_STEP: i32 = 18;
const LINE: u32 = 13;
const BUTTON_HEIGHT: u32 = 15;

pub struct Layout {
    pub fields: [Rect; 8],
    /// Where the file's details begin, a line each.
    pub details: Rect,
    pub message: Rect,
    pub save: Rect,
    pub dismiss: Rect,
}

pub fn layout() -> Layout {
    let c = content_area(WIDTH, HEIGHT);
    let x = c.x + LABEL_WIDTH as i32;
    let w = c.w - LABEL_WIDTH - 1;
    let fields = std::array::from_fn(|i| Rect::new(x, c.y + 3 + i as i32 * FIELD_STEP, w, LINE));
    let details_y = c.y + 3 + 8 * FIELD_STEP + 6;
    let bottom = c.y + c.h as i32 - BUTTON_HEIGHT as i32;
    Layout {
        fields,
        details: Rect::new(c.x + 2, details_y, c.w - 4, (bottom - 4 - details_y) as u32),
        message: Rect::new(c.x + 130, bottom, c.w - 130, BUTTON_HEIGHT),
        save: Rect::new(c.x, bottom, 60, BUTTON_HEIGHT),
        dismiss: Rect::new(c.x + 64, bottom, 60, BUTTON_HEIGHT),
    }
}

pub fn widgets() -> Vec<Widget> {
    let l = layout();
    let ids = [TITLE_DRAG, CLOSE, WidgetId(0)];
    let mut v = gen_window::frame_widgets(WIDTH, HEIGHT, ids, false);
    for (id, r) in FIELDS.iter().zip(l.fields) {
        // A drag across a field selects.
        v.push(Widget::new(*id, r, WidgetKind::Slider));
    }
    v.push(Widget::new(SAVE, l.save, WidgetKind::Button));
    v.push(Widget::new(DISMISS, l.dismiss, WidgetKind::Button));
    v
}

pub struct InfoView<'a> {
    pub focused: bool,
    pub fields: [Field<'a>; 8],
    /// Lines about the file: format, length and so on, then its path.
    pub details: Vec<String>,
    /// What Save did, or why it could not.
    pub message: &'a str,
}

pub fn draw(
    dst: &mut Surface<'_>,
    skin: Option<&Skin>,
    font: Option<&TextFont>,
    view: &InfoView<'_>,
    widgets: &[Widget],
    scale: u32,
) {
    blit::clear(dst, NO_SKIN_BACKGROUND);
    let Some(skin) = skin else { return };
    let colours = *skin.genex_colors();
    let c = content_area(WIDTH, HEIGHT);
    blit::fill_rect(
        dst,
        c.x,
        c.y,
        c.w,
        c.h,
        rgb(colours.window_background),
        scale,
    );
    let l = layout();
    let pressed = |id| widgets.iter().any(|w| w.id == id && w.pressed);
    let text = rgb(colours.window_text);

    if let Some(font) = font {
        for ((label, r), f) in LABELS.iter().zip(l.fields).zip(&view.fields) {
            let at = Rect::new(c.x, r.y, LABEL_WIDTH - 4, r.h);
            text_in(
                dst,
                font,
                &format!("{label}:"),
                at,
                text,
                (false, false),
                scale,
            );
            gc::field(dst, font, &colours, r, f, scale);
        }
        gc::sunken(dst, &colours, l.details, scale);
        for (i, line) in view.details.iter().enumerate() {
            let at = Rect::new(
                l.details.x,
                l.details.y + 2 + (i as u32 * LINE) as i32,
                l.details.w,
                LINE,
            );
            if at.y + LINE as i32 > l.details.y + l.details.h as i32 {
                break;
            }
            text_in(dst, font, line, at, text, (false, false), scale);
        }
        text_in(
            dst,
            font,
            view.message,
            l.message,
            text,
            (false, false),
            scale,
        );
    }
    gc::labelled_button(dst, skin, font, l.save, "Save", pressed(SAVE), scale);
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
        let mut all = l.fields.to_vec();
        all.extend([l.details, l.message, l.save, l.dismiss]);
        for r in all {
            assert!(
                r.x >= c.x
                    && r.y >= c.y
                    && r.x + r.w as i32 <= c.x + c.w as i32
                    && r.y + r.h as i32 <= c.y + c.h as i32,
                "{r:?} outside {c:?}"
            );
        }
        // Room for seven lines about the file.
        assert!(l.details.h >= 7 * LINE + 2, "{:?}", l.details);
    }
}
