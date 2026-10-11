//! Drawing the main window, and the maths of its three sliders.
//!
//! Buttons and toggles come from the widget table (`layout.rs`). Everything
//! whose appearance depends on playback — time, analyser, marquee, bitrate,
//! sliders — is drawn here from a [`MainView`], a plain snapshot the app
//! builds each frame. Keeping this a pure function of `(skin, view, widgets)`
//! is what lets tests render it into a buffer without a window.

use lantern_audio::spectrum::{self, Bar};
use lantern_skin::{Skin, SpriteKey};

use super::blit::{self, Surface};
use super::layout::{self, CLUTTER_BAR, KBPS, KHZ, MARQUEE, MONO, PLAY_STATE, STEREO};
use super::{Widget, WidgetKind};

/// Shown when no skin loads, so the window is never invisible.
pub const NO_SKIN_BACKGROUND: u32 = 0x1C_1C_1C;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Playback {
    #[default]
    Stopped,
    Playing,
    Paused,
}

/// Everything the main window shows that is not a button state.
#[derive(Debug, Clone, Default)]
pub struct MainView {
    pub focused: bool,
    pub playback: Playback,
    pub elapsed_secs: f64,
    pub duration_secs: Option<f64>,
    /// Counting down rather than up; toggled by clicking the time.
    pub show_remaining: bool,
    /// False during the "off" half of the paused blink.
    pub time_visible: bool,
    pub kbps: Option<u32>,
    pub sample_rate: Option<u32>,
    pub channels: Option<u16>,
    /// The string to draw and how far it has scrolled, from `Marquee::layout`.
    pub marquee: (String, u32),
    pub volume: f32,
    pub balance: f32,
    /// 0.0..=1.0 through the track: the scrub position while dragging,
    /// otherwise playback. `None` hides the thumb.
    pub position: Option<f32>,
    /// `None` when the analyser is switched off.
    pub bars: Option<[Bar; spectrum::BAR_COUNT]>,
}

// -- Slider maths -----------------------------------------------------------
//
// Each slider's thumb travels the track width minus its own width. Dragging
// keeps the cursor on the thumb's centre, as a browser range input does, so
// a click jumps the thumb to meet the pointer.

const VOLUME_TRAVEL: f32 = 51.0; // 65px input - 14px thumb
const BALANCE_TRAVEL: f32 = 24.0; // 38px - 14px thumb
const POSITION_TRAVEL: f32 = 219.0; // 248px - 29px thumb
const SLIDER_THUMB_HALF: f32 = 7.0;
const POSITION_THUMB_HALF: f32 = 14.5;

/// Balance within this of centre snaps to it; landing on exactly zero by
/// hand is otherwise nearly impossible. A pixel of travel is 1/12, so this
/// is a pixel either side.
const BALANCE_SNAP: f32 = 0.1;

/// Which of volume.bmp's 28 track frames to show. Webamp's formula.
pub fn volume_frame(volume: f32) -> u8 {
    ((volume * 28.0).round() as i32 - 1).clamp(0, 27) as u8
}

pub fn volume_thumb_x(volume: f32) -> i32 {
    (volume.clamp(0.0, 1.0) * VOLUME_TRAVEL).round() as i32
}

/// Volume for a pointer at `local_x` pixels from the slider's left edge.
pub fn volume_from_x(local_x: i32) -> f32 {
    ((local_x as f32 - SLIDER_THUMB_HALF) / VOLUME_TRAVEL).clamp(0.0, 1.0)
}

/// Which of balance.bmp's 28 frames to show: the same image for left and
/// right, indexed by distance from centre.
pub fn balance_frame(balance: f32) -> u8 {
    (balance.abs().min(1.0) * 27.0).floor() as u8
}

pub fn balance_thumb_x(balance: f32) -> i32 {
    ((balance.clamp(-1.0, 1.0) + 1.0) / 2.0 * BALANCE_TRAVEL).round() as i32
}

pub fn balance_from_x(local_x: i32) -> f32 {
    let b = ((local_x as f32 - SLIDER_THUMB_HALF) / BALANCE_TRAVEL * 2.0 - 1.0).clamp(-1.0, 1.0);
    if b.abs() < BALANCE_SNAP { 0.0 } else { b }
}

pub fn position_thumb_x(fraction: f32) -> i32 {
    (fraction.clamp(0.0, 1.0) * POSITION_TRAVEL).round() as i32
}

/// The shade bar's 17px position slider: a 3px thumb over 14px of travel.
pub fn shade_position_from_x(local_x: i32) -> f32 {
    ((local_x as f32 - 1.5) / 14.0).clamp(0.0, 1.0)
}

/// Which frame a shade-mode thumb shows: it leans left in the first third
/// of its travel and right in the last, as Webamp's `segment` does.
pub fn shade_thumb(fraction: f32) -> lantern_skin::ShadeThumb {
    match fraction {
        f if f < 1.0 / 3.0 => lantern_skin::ShadeThumb::Left,
        f if f < 2.0 / 3.0 => lantern_skin::ShadeThumb::Center,
        _ => lantern_skin::ShadeThumb::Right,
    }
}

pub fn position_from_x(local_x: i32) -> f32 {
    ((local_x as f32 - POSITION_THUMB_HALF) / POSITION_TRAVEL).clamp(0.0, 1.0)
}

// -- Drawing ----------------------------------------------------------------

/// Paint the whole main window.
///
/// No dirty-rect tracking: the surface is 128 KB, so a full repaint is free.
pub fn draw(
    dst: &mut Surface<'_>,
    skin: Option<&Skin>,
    view: &MainView,
    widgets: &[Widget],
    scale: u32,
) {
    blit::clear(dst, NO_SKIN_BACKGROUND);
    let Some(skin) = skin else { return };
    let sprite = |dst: &mut Surface<'_>, key, x, y| draw_sprite(dst, skin, key, x, y, scale);

    sprite(dst, SpriteKey::MainBackground, 0, 0);
    sprite(
        dst,
        SpriteKey::TitleBar {
            active: view.focused,
        },
        0,
        0,
    );
    sprite(
        dst,
        SpriteKey::ClutterBar { enabled: true },
        CLUTTER_BAR.x,
        CLUTTER_BAR.y,
    );

    let state = match view.playback {
        Playback::Playing => SpriteKey::PlayingIndicator,
        Playback::Paused => SpriteKey::PausedIndicator,
        Playback::Stopped => SpriteKey::StoppedIndicator,
    };
    sprite(dst, state, PLAY_STATE.x, PLAY_STATE.y);
    // Beside the play arrow: the green lamp lit while playing normally.
    // Winamp lights the red one (the "working" frame) only while busy,
    // such as buffering a stream, which local files never are.
    if view.playback == Playback::Playing {
        let w = layout::WORK_INDICATOR;
        sprite(dst, SpriteKey::NotWorkingIndicator, w.x, w.y);
    }

    if view.playback != Playback::Stopped {
        if view.time_visible {
            draw_time(dst, skin, view, scale);
        }
        draw_info(dst, skin, view, scale);
    }
    if view.playback != Playback::Stopped
        && let Some(bars) = &view.bars
    {
        draw_analyser(dst, skin, bars, scale);
    }
    draw_marquee(dst, skin, &view.marquee, scale);

    let active = |on: bool| (view.playback != Playback::Stopped && view.channels.is_some()) && on;
    let channels = view.channels.unwrap_or(0);
    sprite(
        dst,
        SpriteKey::Mono {
            active: active(channels == 1),
        },
        MONO.x,
        MONO.y,
    );
    sprite(
        dst,
        SpriteKey::Stereo {
            active: active(channels >= 2),
        },
        STEREO.x,
        STEREO.y,
    );

    draw_sliders(dst, skin, view, widgets, scale);

    for widget in widgets {
        if widget.kind == WidgetKind::HitZone {
            continue;
        }
        if let Some(key) = layout::sprite_for(widget) {
            sprite(dst, key, widget.rect.x, widget.rect.y);
        }
    }
}

pub(crate) fn draw_sprite(
    dst: &mut Surface<'_>,
    skin: &Skin,
    key: SpriteKey,
    x: i32,
    y: i32,
    scale: u32,
) {
    let (sheet_id, src) = skin.resolve(key);
    // A skin that omits a sheet simply draws nothing there, rather than
    // failing to load at all.
    if let Some(sheet) = skin.sheet(sheet_id) {
        blit::blit(dst, sheet, src, x * scale as i32, y * scale as i32, scale);
    }
}

/// `text` in the skin's bitmap font, one 5px cell per character.
pub(crate) fn draw_text(
    dst: &mut Surface<'_>,
    skin: &Skin,
    text: &str,
    x: i32,
    y: i32,
    scale: u32,
) {
    let step = lantern_skin::font::GLYPH_WIDTH as i32;
    for (i, c) in text.chars().enumerate() {
        draw_sprite(dst, skin, SpriteKey::Char(c), x + i as i32 * step, y, scale);
    }
}

/// mm:ss in the big digits. Minutes cap at 99, as the display has two cells.
fn draw_time(dst: &mut Surface<'_>, skin: &Skin, view: &MainView, scale: u32) {
    let secs = if view.show_remaining {
        view.duration_secs.map_or(0.0, |d| d - view.elapsed_secs)
    } else {
        view.elapsed_secs
    };
    let total = secs.max(0.0) as u64;
    let minutes = (total / 60).min(99) as u8;
    let seconds = (total % 60) as u8;
    let (y, digits) = (26, [minutes / 10, minutes % 10, seconds / 10, seconds % 10]);
    for (x, d) in [48, 60, 78, 90].into_iter().zip(digits) {
        draw_sprite(dst, skin, SpriteKey::Digit(d), x, y, scale);
    }
    if view.show_remaining {
        // numbers.bmp's minus is a 5x1 dash cut from mid-cell; nums_ex.bmp
        // has a full 9x13 cell for it, occupying the gap before the minutes.
        let (x, y) = if skin.has_nums_ex() {
            (39, 26)
        } else {
            (38, 32)
        };
        draw_sprite(dst, skin, SpriteKey::MinusSign, x, y, scale);
    }
}

/// kbps and kHz, right-aligned in their 3- and 2-character boxes.
fn draw_info(dst: &mut Surface<'_>, skin: &Skin, view: &MainView, scale: u32) {
    let fit = |value: Option<u32>, width: usize| {
        value.map_or_else(String::new, |v| {
            let s = v.to_string();
            // Too many digits: keep the leading ones, as Winamp does.
            format!("{:>width$}", &s[..s.len().min(width)])
        })
    };
    draw_text(dst, skin, &fit(view.kbps, 3), KBPS.x, KBPS.y, scale);
    draw_text(
        dst,
        skin,
        &fit(view.sample_rate.map(|r| r / 1000), 2),
        KHZ.x,
        KHZ.y,
        scale,
    );
}

fn draw_marquee(dst: &mut Surface<'_>, skin: &Skin, marquee: &(String, u32), scale: u32) {
    let (text, offset) = marquee;
    let clip = lantern_skin::Rect::new(MARQUEE.x as u32, MARQUEE.y as u32, MARQUEE.w, MARQUEE.h);
    dst.set_clip(Some(clip), scale);
    draw_text(
        dst,
        skin,
        text,
        MARQUEE.x - *offset as i32,
        MARQUEE.y,
        scale,
    );
    dst.set_clip(None, scale);
}

/// The analyser: `viscolor.txt` colour 0 as background, colour 1 as a dot
/// grid, colours 2..=17 top to bottom as the bars' fixed row colours, and
/// colour 23 for peaks.
/// Windowshade mode: the shade background (with its baked-in buttons), a
/// mini time, a mini analyser and the mini position slider.
pub fn draw_shade(
    dst: &mut Surface<'_>,
    skin: Option<&Skin>,
    view: &MainView,
    widgets: &[Widget],
    scale: u32,
) {
    blit::clear(dst, NO_SKIN_BACKGROUND);
    let Some(skin) = skin else { return };
    let sprite = |dst: &mut Surface<'_>, key, x, y| draw_sprite(dst, skin, key, x, y, scale);
    sprite(
        dst,
        SpriteKey::ShadeBackground {
            active: view.focused,
        },
        0,
        0,
    );

    if view.playback != Playback::Stopped {
        if view.time_visible {
            draw_text(dst, skin, &mini_time(view), 127, 4, scale);
        }
        if let Some(bars) = &view.bars {
            draw_mini_analyser(dst, skin, bars, scale);
        }
    }
    if let Some(p) = view.position {
        let thumb = SpriteKey::ShadePositionThumb(shade_thumb(p));
        sprite(
            dst,
            thumb,
            226 + (p.clamp(0.0, 1.0) * 14.0).round() as i32,
            4,
        );
    }
    for w in widgets {
        let pressed = w.pressed;
        let key = match w.id {
            layout::OPTIONS => SpriteKey::OptionsButton { pressed },
            layout::MINIMIZE => SpriteKey::MinimizeButton { pressed },
            layout::SHADE => SpriteKey::UnshadeButton { pressed },
            layout::CLOSE => SpriteKey::CloseButton { pressed },
            _ => continue,
        };
        sprite(dst, key, w.rect.x, w.rect.y);
    }
}

/// `" 1:23"`, or `"-1:23"` counting down: five 5px characters.
fn mini_time(view: &MainView) -> String {
    let secs = if view.show_remaining {
        view.duration_secs.map_or(0.0, |d| d - view.elapsed_secs)
    } else {
        view.elapsed_secs
    };
    let s = secs.max(0.0) as u64;
    let sign = if view.show_remaining { '-' } else { ' ' };
    format!("{sign}{}:{:02}", (s / 60).min(99), s % 60)
}

/// Ten 3px bars, 5 rows high, at (79, 5): every pair of the full
/// analyser's bars folded into one. Rows take viscolor 4, 8, 11, 14 and
/// 17 from the top, as Webamp's small visualiser does.
fn draw_mini_analyser(dst: &mut Surface<'_>, skin: &Skin, bars: &[Bar], scale: u32) {
    const ROW_COLOURS: [usize; 5] = [4, 8, 11, 14, 17];
    for (i, pair) in bars.chunks(2).enumerate() {
        let full = pair.iter().map(|b| b.height).max().unwrap_or(0);
        let height = (u32::from(full) * 5)
            .div_ceil(u32::from(spectrum::HEIGHT))
            .min(5) as i32;
        for row in (5 - height)..5 {
            let [r, g, b] = skin.viscolors[ROW_COLOURS[row as usize]];
            let rgb = u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b);
            blit::fill_rect(dst, 79 + i as i32 * 4, 5 + row, 3, 1, rgb, scale);
        }
    }
}

fn draw_analyser(dst: &mut Surface<'_>, skin: &Skin, bars: &[Bar], scale: u32) {
    let rgb = |i: usize| {
        let [r, g, b] = skin.viscolors[i];
        u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b)
    };
    let (x0, y0) = (24, 43);
    // The display is 16px; bars reach 15, leaving the top row for peaks.
    let h = 16;

    blit::fill_rect(dst, x0, y0, 76, h as u32, rgb(0), scale);
    for x in (0..76).step_by(2) {
        for y in (1..h).step_by(2) {
            blit::fill_rect(dst, x0 + x, y0 + y, 1, 1, rgb(1), scale);
        }
    }

    for (i, bar) in bars.iter().enumerate() {
        let x = x0 + i as i32 * 4;
        for row in (h - i32::from(bar.height))..h {
            blit::fill_rect(dst, x, y0 + row, 3, 1, rgb(2 + row as usize), scale);
        }
        if let Some(peak) = bar.peak {
            // One row above the bar's top, as Winamp draws it.
            let y = y0 + h - 1 - i32::from(peak);
            blit::fill_rect(dst, x, y, 3, 1, rgb(23), scale);
        }
    }
}

fn draw_sliders(
    dst: &mut Surface<'_>,
    skin: &Skin,
    view: &MainView,
    widgets: &[Widget],
    scale: u32,
) {
    let pressed = |id| widgets.iter().any(|w| w.id == id && w.pressed);

    let v = view.volume;
    draw_sprite(
        dst,
        skin,
        SpriteKey::VolumeBackground(volume_frame(v)),
        107,
        57,
        scale,
    );
    let thumb = SpriteKey::VolumeThumb {
        pressed: pressed(layout::VOLUME),
    };
    draw_sprite(dst, skin, thumb, 107 + volume_thumb_x(v), 58, scale);

    let b = view.balance;
    draw_sprite(
        dst,
        skin,
        SpriteKey::BalanceBackground(balance_frame(b)),
        177,
        57,
        scale,
    );
    let thumb = SpriteKey::BalanceThumb {
        pressed: pressed(layout::BALANCE),
    };
    draw_sprite(dst, skin, thumb, 177 + balance_thumb_x(b), 58, scale);

    draw_sprite(dst, skin, SpriteKey::PosBarBackground, 16, 72, scale);
    if let Some(p) = view.position {
        let thumb = SpriteKey::PosBarThumb {
            pressed: pressed(layout::POSITION),
        };
        draw_sprite(dst, skin, thumb, 16 + position_thumb_x(p), 72, scale);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volume_frames_span_the_sheet() {
        assert_eq!(volume_frame(0.0), 0);
        assert_eq!(volume_frame(1.0), 27);
        assert_eq!(volume_frame(0.5), 13);
    }

    #[test]
    fn slider_round_trips() {
        for v in [0.0, 0.25, 0.7, 1.0] {
            let x = volume_thumb_x(v) + SLIDER_THUMB_HALF as i32;
            assert!((volume_from_x(x) - v).abs() < 0.02, "volume {v}");
        }
        for b in [-1.0, -0.5, 0.5, 1.0] {
            let x = balance_thumb_x(b) + SLIDER_THUMB_HALF as i32;
            assert!((balance_from_x(x) - b).abs() < 0.05, "balance {b}");
        }
        for p in [0.0, 0.33, 1.0] {
            let x = position_thumb_x(p) + POSITION_THUMB_HALF as i32;
            assert!((position_from_x(x) - p).abs() < 0.01, "position {p}");
        }
    }

    #[test]
    fn sliders_clamp_past_their_ends() {
        assert_eq!(volume_from_x(-50), 0.0);
        assert_eq!(volume_from_x(500), 1.0);
        assert_eq!(balance_from_x(-50), -1.0);
        assert_eq!(position_from_x(1000), 1.0);
    }

    #[test]
    fn balance_snaps_to_centre() {
        let centre = balance_thumb_x(0.0) + SLIDER_THUMB_HALF as i32;
        assert_eq!(balance_from_x(centre + 1), 0.0);
        assert_eq!(balance_frame(0.0), 0);
        assert_eq!(balance_frame(-1.0), 27);
    }
}
