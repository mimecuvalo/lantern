//! The equaliser window: layout, drawing, and slider maths.
//!
//! Coordinates from Webamp's `equalizer-window.css`; slider sprite indexing
//! and the response-curve drawing from its `Band.tsx` and `EqGraph.tsx`
//! (MIT, Copyright (c) 2015 Jordan Eldredge).

use lantern_skin::{SheetId, Skin, SpriteKey};

use super::blit::{self, Surface};
use super::geometry::{Rect, WINDOW_WIDTH};
use super::main_window::{NO_SKIN_BACKGROUND, draw_sprite};
use super::{Widget, WidgetId, WidgetKind};

pub const TITLE_DRAG: WidgetId = WidgetId(100);
pub const SHADE: WidgetId = WidgetId(101);
pub const CLOSE: WidgetId = WidgetId(102);
pub const ON: WidgetId = WidgetId(103);
pub const AUTO: WidgetId = WidgetId(104);
pub const PRESETS: WidgetId = WidgetId(105);
pub const PREAMP: WidgetId = WidgetId(110);
/// Bands are `BAND_0 + i`, bass first.
pub const BAND_0: WidgetId = WidgetId(120);
/// Windowshade mode's mini volume and balance sliders.
pub const SHADE_VOLUME: WidgetId = WidgetId(130);
pub const SHADE_BALANCE: WidgetId = WidgetId(131);

const GRAPH: Rect = Rect::new(86, 17, 113, 19);
const SLIDER_TOP: i32 = 38;
const SLIDER_HEIGHT: u32 = 63;

/// The thumb travels the 62px track less its own 11px.
const THUMB_TRAVEL: f32 = 51.0;
const THUMB_HALF: f32 = 5.5;

pub fn band_id(i: usize) -> WidgetId {
    WidgetId(BAND_0.0 + i as u16)
}

pub fn band_index(id: WidgetId) -> Option<usize> {
    (BAND_0.0..BAND_0.0 + 10)
        .contains(&id.0)
        .then(|| (id.0 - BAND_0.0) as usize)
}

pub fn widgets() -> Vec<Widget> {
    let w = |id, x, y, width, height, kind| Widget::new(id, Rect::new(x, y, width, height), kind);
    use WidgetKind::{Button, HitZone, Slider, Toggle};
    let mut v = vec![
        w(TITLE_DRAG, 0, 0, WINDOW_WIDTH, 14, HitZone),
        // Shade's pixels are part of the title bar; only close has a
        // pressed sprite in eqmain.bmp.
        w(SHADE, 254, 3, 9, 9, HitZone),
        w(CLOSE, 264, 3, 9, 9, Button),
        w(ON, 14, 18, 26, 12, Toggle),
        w(AUTO, 40, 18, 32, 12, Toggle),
        w(PRESETS, 217, 18, 44, 12, Button),
        w(PREAMP, 21, SLIDER_TOP, 14, SLIDER_HEIGHT, Slider),
    ];
    for i in 0..10 {
        v.push(w(
            band_id(i),
            78 + 18 * i as i32,
            SLIDER_TOP,
            14,
            SLIDER_HEIGHT,
            Slider,
        ));
    }
    v
}

/// The EQ in windowshade mode: title-bar buttons and two mini sliders,
/// at Webamp's `equalizer-window.css` positions.
pub fn shade_widgets() -> Vec<Widget> {
    let w = |id, x, y, width, height, kind| Widget::new(id, Rect::new(x, y, width, height), kind);
    use WidgetKind::{Button, HitZone, Slider};
    vec![
        w(TITLE_DRAG, 0, 0, WINDOW_WIDTH, 14, HitZone),
        w(SHADE_VOLUME, 61, 4, 97, 6, Slider),
        w(SHADE_BALANCE, 164, 4, 43, 6, Slider),
        w(SHADE, 254, 3, 9, 9, Button),
        w(CLOSE, 264, 3, 9, 9, Button),
    ]
}

/// A shade mini-slider position from a pointer `local_x` into it: a 3px
/// thumb over `width - 3` of travel.
pub fn shade_fraction_from_x(local_x: i32, width: u32) -> f32 {
    ((local_x as f32 - 1.5) / (width - 3) as f32).clamp(0.0, 1.0)
}

/// What the shaded EQ shows: the main window's volume and balance.
#[derive(Debug, Clone, Default)]
pub struct EqShadeView {
    pub focused: bool,
    pub volume: f32,
    pub balance: f32,
}

pub fn draw_shade(
    dst: &mut Surface<'_>,
    skin: Option<&Skin>,
    view: &EqShadeView,
    widgets: &[Widget],
    scale: u32,
) {
    use super::main_window::shade_thumb;
    blit::clear(dst, NO_SKIN_BACKGROUND);
    let Some(skin) = skin else { return };
    let sprite = |dst: &mut Surface<'_>, key, x, y| draw_sprite(dst, skin, key, x, y, scale);
    sprite(
        dst,
        SpriteKey::EqShadeBackground {
            active: view.focused,
        },
        0,
        0,
    );
    let volume = view.volume.clamp(0.0, 1.0);
    let balance = (view.balance.clamp(-1.0, 1.0) + 1.0) / 2.0;
    let x = |f: f32, width: u32| (f * (width - 3) as f32).round() as i32;
    sprite(
        dst,
        SpriteKey::EqShadeVolumeThumb(shade_thumb(volume)),
        61 + x(volume, 97),
        4,
    );
    sprite(
        dst,
        SpriteKey::EqShadeBalanceThumb(shade_thumb(balance)),
        164 + x(balance, 43),
        4,
    );
    for w in widgets {
        if w.id == CLOSE {
            let key = SpriteKey::EqShadeCloseButton { pressed: w.pressed };
            sprite(dst, key, w.rect.x, w.rect.y);
        }
    }
}

/// Slider position 0.0 (bottom, -12 dB) ..= 1.0 (top, +12 dB).
pub fn level_from_db(db: f32) -> f32 {
    (db / 24.0 + 0.5).clamp(0.0, 1.0)
}

pub fn db_from_level(level: f32) -> f32 {
    (level.clamp(0.0, 1.0) - 0.5) * 24.0
}

/// Level for a pointer `local_y` pixels below the slider's top, keeping the
/// pointer on the thumb's centre.
pub fn level_from_y(local_y: i32) -> f32 {
    (1.0 - (local_y as f32 - THUMB_HALF) / THUMB_TRAVEL).clamp(0.0, 1.0)
}

fn thumb_y(level: f32) -> i32 {
    ((1.0 - level.clamp(0.0, 1.0)) * THUMB_TRAVEL).round() as i32
}

/// Which of the 28 prerendered slider backgrounds matches `level`.
fn slider_frame(level: f32) -> u8 {
    (level.clamp(0.0, 1.0) * 27.0).round() as u8
}

#[derive(Debug, Clone, Default)]
pub struct EqView {
    pub focused: bool,
    pub enabled: bool,
    pub auto: bool,
    /// Slider levels, 0.0..=1.0: the preamp, then ten bands.
    pub preamp: f32,
    pub bands: [f32; 10],
}

pub fn draw(
    dst: &mut Surface<'_>,
    skin: Option<&Skin>,
    view: &EqView,
    widgets: &[Widget],
    scale: u32,
) {
    blit::clear(dst, NO_SKIN_BACKGROUND);
    let Some(skin) = skin else { return };
    let sprite = |dst: &mut Surface<'_>, key, x, y| draw_sprite(dst, skin, key, x, y, scale);

    sprite(dst, SpriteKey::EqBackground, 0, 0);
    sprite(
        dst,
        SpriteKey::EqTitleBar {
            active: view.focused,
        },
        0,
        0,
    );
    draw_graph(dst, skin, view, scale);

    for w in widgets {
        let (pressed, active) = (w.pressed, w.active);
        let (x, y) = (w.rect.x, w.rect.y);
        let level = match w.id {
            PREAMP => Some(view.preamp),
            id => band_index(id).map(|i| view.bands[i]),
        };
        if let Some(level) = level {
            sprite(
                dst,
                SpriteKey::EqSliderBackground(slider_frame(level)),
                x,
                y,
            );
            sprite(
                dst,
                SpriteKey::EqSliderThumb { pressed },
                x + 1,
                y + thumb_y(level),
            );
            continue;
        }
        let key = match w.id {
            CLOSE if pressed => SpriteKey::EqCloseButton { pressed },
            ON => SpriteKey::EqOnButton { active, pressed },
            AUTO => SpriteKey::EqAutoButton { active, pressed },
            PRESETS => SpriteKey::EqPresetsButton { pressed },
            _ => continue,
        };
        sprite(dst, key, x, y);
    }
}

/// The response preview: the preamp line, and a spline through the ten
/// band levels coloured row by row from `eqmain.bmp`'s 1x19 colour strip.
fn draw_graph(dst: &mut Surface<'_>, skin: &Skin, view: &EqView, scale: u32) {
    draw_sprite(
        dst,
        skin,
        SpriteKey::EqGraphBackground,
        GRAPH.x,
        GRAPH.y,
        scale,
    );
    let max_y = (GRAPH.h - 1) as f32;
    let preamp_y = ((1.0 - view.preamp) * max_y).round() as i32;
    draw_sprite(
        dst,
        skin,
        SpriteKey::EqPreampLine,
        GRAPH.x,
        GRAPH.y + preamp_y,
        scale,
    );

    let Some(sheet) = skin.sheet(SheetId::EqMain) else {
        return;
    };
    let (_, strip) = skin.resolve(SpriteKey::EqGraphLineColors);
    let colour = |row: i32| {
        let [r, g, b, _] = sheet.pixel(strip.x, strip.y + row.clamp(0, 18) as u32);
        u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b)
    };

    let ys: Vec<f64> = view
        .bands
        .iter()
        .map(|&v| f64::from((1.0 - v) * max_y))
        .collect();
    let curve = natural_spline(&ys, 12);
    // Webamp starts the curve 2px in; each band is 12px apart.
    let mut last = curve[0].round() as i32;
    for (x, &y) in curve.iter().enumerate() {
        let y = (y.round() as i32).clamp(0, max_y as i32);
        let (top, bottom) = (y.min(last), y.max(last));
        for row in top..=bottom {
            blit::fill_rect(
                dst,
                GRAPH.x + 2 + x as i32,
                GRAPH.y + row,
                1,
                1,
                colour(row),
                scale,
            );
        }
        last = y;
    }
}

/// Natural cubic spline through `ys`, spaced `step` apart, sampled at every
/// integer x from the first point to the last.
fn natural_spline(ys: &[f64], step: usize) -> Vec<f64> {
    let n = ys.len();
    if n < 2 {
        return ys.to_vec();
    }
    let h = step as f64;
    // Second derivatives by the tridiagonal (Thomas) solve, with M0 = Mn = 0.
    let mut m = vec![0.0; n];
    let mut c = vec![0.0; n];
    let mut d = vec![0.0; n];
    for i in 1..n - 1 {
        let rhs = 6.0 * (ys[i + 1] - 2.0 * ys[i] + ys[i - 1]) / (h * h);
        let denom = 4.0 - c[i - 1];
        c[i] = 1.0 / denom;
        d[i] = (rhs - d[i - 1]) / denom;
    }
    for i in (1..n - 1).rev() {
        m[i] = d[i] - c[i] * m[i + 1];
    }
    (0..=(n - 1) * step)
        .map(|x| {
            let i = (x / step).min(n - 2);
            let t = (x - i * step) as f64;
            let a = ys[i];
            let b = (ys[i + 1] - ys[i]) / h - h * (2.0 * m[i] + m[i + 1]) / 6.0;
            let cc = m[i] / 2.0;
            let dd = (m[i + 1] - m[i]) / (6.0 * h);
            a + b * t + cc * t * t + dd * t * t * t
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn db_and_level_round_trip() {
        assert_eq!(level_from_db(0.0), 0.5);
        assert_eq!(level_from_db(12.0), 1.0);
        assert_eq!(level_from_db(-12.0), 0.0);
        assert_eq!(db_from_level(0.75), 6.0);
    }

    #[test]
    fn slider_pointer_maps_to_thumb_centre() {
        for level in [0.0, 0.3, 0.5, 1.0] {
            let y = thumb_y(level) + THUMB_HALF as i32;
            assert!((level_from_y(y) - level).abs() < 0.03, "level {level}");
        }
        assert_eq!(level_from_y(-40), 1.0);
        assert_eq!(level_from_y(400), 0.0);
    }

    #[test]
    fn spline_passes_through_its_points() {
        let ys = [0.0, 18.0, 9.0, 3.0];
        let curve = natural_spline(&ys, 12);
        assert_eq!(curve.len(), 37);
        for (i, &y) in ys.iter().enumerate() {
            assert!((curve[i * 12] - y).abs() < 1e-9, "point {i}");
        }
    }

    #[test]
    fn flat_bands_draw_a_flat_line() {
        let curve = natural_spline(&[9.0; 10], 12);
        assert!(curve.iter().all(|&y| (y - 9.0).abs() < 1e-9));
    }

    #[test]
    fn layout_fits_the_window() {
        for w in widgets() {
            assert!(w.rect.x + w.rect.w as i32 <= 275 && w.rect.y + w.rect.h as i32 <= 116);
        }
        assert_eq!(band_index(band_id(9)), Some(9));
        assert_eq!(band_index(PREAMP), None);
    }
}
