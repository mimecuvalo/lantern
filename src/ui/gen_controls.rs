//! The controls inside generic windows, drawn from `genex.bmp`: buttons,
//! scrollbars, and the sunken boxes lists and edits sit in.
//!
//! A sprite is drawn at any length by keeping its ends, repeating a plain
//! line from just inside them, and setting its middle (a grip, say) back in
//! the centre.

use lantern_skin::{GenExColors, Rect as SheetRect, Skin, SpriteKey};

use super::blit::{self, Surface};
use super::font::{TextFont, TextStyle};
use super::geometry::Rect;

/// Text in generic windows: the playlist's font at its size.
const FONT_PX: f32 = 9.0;
const LETTER_SPACING: f32 = 0.5;

pub fn style(scale: u32) -> TextStyle {
    let s = scale as f32;
    TextStyle {
        px: FONT_PX * s,
        spacing: LETTER_SPACING * s,
    }
}

/// Text in `font`, cut to fit `r` and set on its middle; right-aligned
/// within it if `right`, centred if `centre`.
#[allow(clippy::too_many_arguments)]
pub fn text_in(
    dst: &mut Surface<'_>,
    font: &TextFont,
    text: &str,
    r: Rect,
    colour: u32,
    (right, centre): (bool, bool),
    scale: u32,
) {
    let s = scale as f32;
    let st = style(scale);
    let pad = 2.0 * s;
    let room = r.w as f32 * s - 2.0 * pad;
    if room <= 0.0 {
        return;
    }
    let fitted = font.fit(text, st, room);
    let w = font.width(&fitted, st);
    let left = r.x as f32 * s;
    let x = if centre {
        left + (r.w as f32 * s - w) / 2.0
    } else if right {
        left + r.w as f32 * s - pad - w
    } else {
        left + pad
    };
    let baseline = (r.y + (r.h as i32 + 7) / 2) * scale as i32;
    font.draw(dst, &fitted, (x, baseline), st, colour);
}

/// Confine drawing to `r`, in window pixels.
pub fn clip(dst: &mut Surface<'_>, r: Rect, scale: u32) {
    let c = lantern_skin::Rect::new(r.x.max(0) as u32, r.y.max(0) as u32, r.w, r.h);
    dst.set_clip(Some(c), scale);
}

/// A one-line text field: sunken, its text, a selection behind it, the
/// stretch an input method is composing underlined, and the caret if it
/// has the keyboard. Positions are in chars.
pub struct Field<'a> {
    pub text: &'a str,
    pub caret: Option<usize>,
    pub selection: Option<(usize, usize)>,
    pub marked: Option<(usize, usize)>,
}

pub fn field(
    dst: &mut Surface<'_>,
    font: &TextFont,
    colours: &GenExColors,
    r: Rect,
    f: &Field<'_>,
    scale: u32,
) {
    let (bg, fg) = (rgb(colours.item_background), rgb(colours.item_foreground));
    let (sel_bg, sel_fg) = (
        rgb(colours.list_background_selected),
        rgb(colours.list_text_selected),
    );
    blit::fill_rect(dst, r.x, r.y, r.w, r.h, bg, scale);
    sunken(dst, colours, r, scale);
    clip(dst, r, scale);
    // Where the n-th character starts, in window pixels.
    let s = scale as f32;
    let x_at = |n: usize| {
        let before: String = f.text.chars().take(n).collect();
        (((r.x as f32 + 2.0) * s + font.width(&before, style(scale))) / s).round() as i32
    };
    if let Some((a, b)) = f.selection {
        let (xa, xb) = (x_at(a), x_at(b));
        blit::fill_rect(
            dst,
            xa,
            r.y + 1,
            (xb - xa).max(0) as u32,
            r.h - 2,
            sel_bg,
            scale,
        );
    }
    text_in(dst, font, f.text, r, fg, (false, false), scale);
    // The selected stretch again, in the selection's own text colour.
    if let Some((a, b)) = f.selection {
        let (xa, xb) = (x_at(a).max(r.x), x_at(b).min(r.x + r.w as i32));
        if xb > xa {
            clip(dst, Rect::new(xa, r.y, (xb - xa) as u32, r.h), scale);
            text_in(dst, font, f.text, r, sel_fg, (false, false), scale);
            clip(dst, r, scale);
        }
    }
    if let Some((a, b)) = f.marked {
        let (xa, xb) = (x_at(a), x_at(b));
        blit::fill_rect(
            dst,
            xa,
            r.y + r.h as i32 - 3,
            (xb - xa).max(1) as u32,
            1,
            fg,
            scale,
        );
    }
    if let Some(caret) = f.caret {
        blit::fill_rect(dst, x_at(caret), r.y + 2, 1, r.h - 4, fg, scale);
    }
    dst.set_clip(None, scale);
}

/// Which character a click `local_x` pixels into a field lands before.
pub fn caret_at(font: &TextFont, text: &str, local_x: i32) -> usize {
    let st = style(1);
    // Before the first character whose middle is past the pointer.
    let chars: Vec<char> = text.chars().collect();
    let width = |n: usize| font.width(&chars[..n].iter().collect::<String>(), st);
    (0..chars.len())
        .find(|&n| (local_x as f32) < (width(n) + width(n + 1)) / 2.0)
        .unwrap_or(chars.len())
}

/// A push button with `label` on it, nudged down a pixel while pressed.
#[allow(clippy::too_many_arguments)]
pub fn labelled_button(
    dst: &mut Surface<'_>,
    skin: &Skin,
    font: Option<&TextFont>,
    r: Rect,
    label: &str,
    pressed: bool,
    scale: u32,
) {
    button(dst, skin, r, pressed, scale);
    if let Some(font) = font {
        let nudge = i32::from(pressed);
        let at = Rect::new(r.x + nudge, r.y + nudge, r.w, r.h);
        let colour = rgb(skin.genex_colors().button_text);
        text_in(dst, font, label, at, colour, (false, true), scale);
    }
}

/// Kept whole at each end of a stretched sprite.
const END: u32 = 4;
/// Scroll arrows are square, this big.
pub const ARROW: u32 = 14;
/// The shortest a thumb gets.
const MIN_THUMB: u32 = 10;

pub fn rgb([r, g, b]: [u8; 3]) -> u32 {
    u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b)
}

/// Copy `src` from the genex sheet repeatedly over `to`, clipped to it.
fn tile(dst: &mut Surface<'_>, skin: &Skin, src: SheetRect, to: Rect, scale: u32) {
    let Some(sheet) = skin.sheet(lantern_skin::SheetId::GenEx) else {
        return;
    };
    if src.w == 0 || src.h == 0 || to.w == 0 || to.h == 0 {
        return;
    }
    let s = scale as i32;
    let clip = SheetRect::new(to.x.max(0) as u32, to.y.max(0) as u32, to.w, to.h);
    dst.set_clip(Some(clip), scale);
    let mut y = to.y;
    while y < to.y + to.h as i32 {
        let mut x = to.x;
        while x < to.x + to.w as i32 {
            blit::blit(dst, sheet, src, x * s, y * s, scale);
            x += src.w as i32;
        }
        y += src.h as i32;
    }
    dst.set_clip(None, scale);
}

/// Draw `key` stretched along its long axis to fill `to`, which should be
/// the sprite's own size across.
pub fn stretch(
    dst: &mut Surface<'_>,
    skin: &Skin,
    key: SpriteKey,
    to: Rect,
    vertical: bool,
    scale: u32,
) {
    let (_, src) = skin.resolve(key);
    let (len, have) = if vertical {
        (to.h, src.h)
    } else {
        (to.w, src.w)
    };
    if len < 2 * END || have < 2 * END + 1 {
        tile(dst, skin, src, to, scale);
        return;
    }
    // In sheet and window terms along the axis: (offset, length).
    let part = |from: u32, n: u32| {
        if vertical {
            SheetRect::new(src.x, src.y + from, src.w, n)
        } else {
            SheetRect::new(src.x + from, src.y, n, src.h)
        }
    };
    let place = |at: u32, n: u32| {
        if vertical {
            Rect::new(to.x, to.y + at as i32, to.w, n)
        } else {
            Rect::new(to.x + at as i32, to.y, n, to.h)
        }
    };
    // The plain line just inside the head fills the body; then the middle
    // goes back in the centre, if it fits.
    tile(dst, skin, part(END, 1), place(END, len - 2 * END), scale);
    let middle = have - 2 * END;
    if len >= have {
        let at = (len - middle) / 2;
        tile(dst, skin, part(END, middle), place(at, middle), scale);
    }
    tile(dst, skin, part(0, END), place(0, END), scale);
    tile(
        dst,
        skin,
        part(have - END, END),
        place(len - END, END),
        scale,
    );
}

/// A push button with `label` centred on it; the caller draws the label.
pub fn button(dst: &mut Surface<'_>, skin: &Skin, to: Rect, pressed: bool, scale: u32) {
    stretch(
        dst,
        skin,
        SpriteKey::GenExButton { pressed },
        to,
        false,
        scale,
    );
}

/// A 1px sunken border just outside `inner`, in the dividers' colour.
pub fn sunken(dst: &mut Surface<'_>, colours: &GenExColors, inner: Rect, scale: u32) {
    let c = rgb(colours.divider);
    let (x, y, w, h) = (inner.x - 1, inner.y - 1, inner.w + 2, inner.h + 2);
    blit::fill_rect(dst, x, y, w, 1, c, scale);
    blit::fill_rect(dst, x, y + h as i32 - 1, w, 1, c, scale);
    blit::fill_rect(dst, x, y, 1, h, c, scale);
    blit::fill_rect(dst, x + w as i32 - 1, y, 1, h, c, scale);
}

/// Which part of a scrollbar the pointer is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollPart {
    /// The up or left arrow.
    Back,
    /// The down or right arrow.
    Forward,
    /// The track before the thumb.
    PageBack,
    /// The track after it.
    PageForward,
    Thumb,
}

/// A scrollbar's state: how far along (0..=1) and how much is in view.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScrollState {
    pub position: f32,
    /// The fraction of the whole in view; 1 or more needs no thumb.
    pub visible: f32,
    /// The part held down, drawn pressed.
    pub pressed: Option<ScrollPart>,
}

/// Where the track and thumb lie along a scrollbar `len` long:
/// (track start, track length, thumb start, thumb length), all from the
/// scrollbar's own start. No thumb when everything is in view.
pub fn scroll_geometry(len: u32, state: &ScrollState) -> (u32, u32, Option<(u32, u32)>) {
    let track = len.saturating_sub(2 * ARROW);
    if state.visible >= 1.0 || track < MIN_THUMB {
        return (ARROW, track, None);
    }
    let thumb = ((track as f32 * state.visible) as u32).clamp(MIN_THUMB, track);
    let at = ARROW + ((track - thumb) as f32 * state.position.clamp(0.0, 1.0)).round() as u32;
    (ARROW, track, Some((at, thumb)))
}

/// What is at `along` pixels down (or across) a scrollbar `len` long.
pub fn scroll_hit(len: u32, state: &ScrollState, along: i32) -> ScrollPart {
    let (_, _, thumb) = scroll_geometry(len, state);
    let along = along.max(0) as u32;
    if along < ARROW {
        ScrollPart::Back
    } else if along >= len.saturating_sub(ARROW) {
        ScrollPart::Forward
    } else {
        match thumb {
            Some((at, _)) if along < at => ScrollPart::PageBack,
            Some((at, n)) if along < at + n => ScrollPart::Thumb,
            _ => ScrollPart::PageForward,
        }
    }
}

pub fn scrollbar(
    dst: &mut Surface<'_>,
    skin: &Skin,
    to: Rect,
    vertical: bool,
    state: &ScrollState,
    scale: u32,
) {
    let colours = skin.genex_colors();
    let len = if vertical { to.h } else { to.w };
    let (track_at, track, thumb) = scroll_geometry(len, state);
    let along = |at: u32, n: u32| {
        if vertical {
            Rect::new(to.x, to.y + at as i32, to.w, n)
        } else {
            Rect::new(to.x + at as i32, to.y, n, to.h)
        }
    };
    // The track: the two scrollbar colours in a fine check, as Windows drew
    // them; darker while paged.
    let paging = matches!(
        state.pressed,
        Some(ScrollPart::PageBack | ScrollPart::PageForward)
    );
    let (one, two) = if paging {
        (colours.pressed_scrollbar_one, colours.pressed_scrollbar_two)
    } else {
        (colours.scrollbar_one, colours.scrollbar_two)
    };
    let r = along(track_at, track);
    for py in 0..r.h as i32 {
        for px in 0..r.w as i32 {
            let c = if (px + py) % 2 == 0 { one } else { two };
            blit::fill_rect(dst, r.x + px, r.y + py, 1, 1, rgb(c), scale);
        }
    }
    let pressed = |p| state.pressed == Some(p);
    let (back, forward) = if vertical {
        (
            SpriteKey::GenExScrollUp {
                pressed: pressed(ScrollPart::Back),
            },
            SpriteKey::GenExScrollDown {
                pressed: pressed(ScrollPart::Forward),
            },
        )
    } else {
        (
            SpriteKey::GenExScrollLeft {
                pressed: pressed(ScrollPart::Back),
            },
            SpriteKey::GenExScrollRight {
                pressed: pressed(ScrollPart::Forward),
            },
        )
    };
    tile(
        dst,
        skin,
        skin.resolve(back).1,
        along(0, ARROW.min(len)),
        scale,
    );
    if len > ARROW {
        tile(
            dst,
            skin,
            skin.resolve(forward).1,
            along(len - ARROW, ARROW),
            scale,
        );
    }
    if let Some((at, n)) = thumb {
        let key = if vertical {
            SpriteKey::GenExVerticalThumb {
                pressed: pressed(ScrollPart::Thumb),
            }
        } else {
            SpriteKey::GenExHorizontalThumb {
                pressed: pressed(ScrollPart::Thumb),
            }
        };
        stretch(dst, skin, key, along(at, n), vertical, scale);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(position: f32, visible: f32) -> ScrollState {
        ScrollState {
            position,
            visible,
            pressed: None,
        }
    }

    #[test]
    fn the_thumb_shows_how_much_is_in_view() {
        // 128 long: 100 of track between the arrows.
        assert_eq!(
            scroll_geometry(128, &state(0.0, 0.5)),
            (14, 100, Some((14, 50)))
        );
        assert_eq!(
            scroll_geometry(128, &state(1.0, 0.5)),
            (14, 100, Some((64, 50)))
        );
        assert_eq!(
            scroll_geometry(128, &state(0.0, 0.01)).2,
            Some((14, MIN_THUMB))
        );
        assert_eq!(scroll_geometry(128, &state(0.0, 1.0)).2, None);
    }

    #[test]
    fn parts_are_found_along_the_bar() {
        let s = state(0.5, 0.2);
        // Track 100, thumb 20 at 14 + 40.
        assert_eq!(scroll_hit(128, &s, 3), ScrollPart::Back);
        assert_eq!(scroll_hit(128, &s, 20), ScrollPart::PageBack);
        assert_eq!(scroll_hit(128, &s, 60), ScrollPart::Thumb);
        assert_eq!(scroll_hit(128, &s, 100), ScrollPart::PageForward);
        assert_eq!(scroll_hit(128, &s, 120), ScrollPart::Forward);
    }
}
