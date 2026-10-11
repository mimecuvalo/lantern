//! Where the main window's controls sit.
//!
//! Fixed by the skin format. Every classic skin is cut for exactly these
//! positions, which is what "pixel-exact" buys: no per-skin adaptation, ever.
//! Coordinates are from Webamp's `main-window.css` (MIT, Copyright (c) 2015
//! Jordan Eldredge).

use lantern_skin::{ClutterButton, SpriteKey};

use super::geometry::{Rect, WINDOW_WIDTH};
use super::{Widget, WidgetId, WidgetKind};

pub const TITLE_DRAG: WidgetId = WidgetId(1);
pub const OPTIONS: WidgetId = WidgetId(2);
pub const MINIMIZE: WidgetId = WidgetId(3);
pub const SHADE: WidgetId = WidgetId(4);
pub const CLOSE: WidgetId = WidgetId(5);

pub const CLUTTER_O: WidgetId = WidgetId(10);
pub const CLUTTER_A: WidgetId = WidgetId(11);
pub const CLUTTER_I: WidgetId = WidgetId(12);
pub const CLUTTER_D: WidgetId = WidgetId(13);
pub const CLUTTER_V: WidgetId = WidgetId(14);

pub const TIME: WidgetId = WidgetId(20);
pub const VISUALIZER: WidgetId = WidgetId(21);

pub const VOLUME: WidgetId = WidgetId(30);
pub const BALANCE: WidgetId = WidgetId(31);
pub const EQ_TOGGLE: WidgetId = WidgetId(32);
pub const PL_TOGGLE: WidgetId = WidgetId(33);
pub const POSITION: WidgetId = WidgetId(34);

pub const PREVIOUS: WidgetId = WidgetId(40);
pub const PLAY: WidgetId = WidgetId(41);
pub const PAUSE: WidgetId = WidgetId(42);
pub const STOP: WidgetId = WidgetId(43);
pub const NEXT: WidgetId = WidgetId(44);
pub const EJECT: WidgetId = WidgetId(45);
pub const SHUFFLE: WidgetId = WidgetId(46);
pub const REPEAT: WidgetId = WidgetId(47);
pub const ABOUT: WidgetId = WidgetId(48);

/// Displays that are drawn but take no input.
pub const MARQUEE: Rect = Rect::new(111, 27, 154, 6);
pub const KBPS: Rect = Rect::new(111, 43, 15, 6);
pub const KHZ: Rect = Rect::new(156, 43, 10, 6);
pub const PLAY_STATE: Rect = Rect::new(26, 28, 9, 9);
pub const WORK_INDICATOR: Rect = Rect::new(24, 28, 3, 9);
pub const CLUTTER_BAR: Rect = Rect::new(10, 22, 8, 43);
pub const MONO: Rect = Rect::new(212, 41, 27, 12);
pub const STEREO: Rect = Rect::new(239, 41, 29, 12);

/// The main window's widgets, in draw order. Hit testing scans in reverse, so
/// the title-bar drag zone comes first and its buttons win over it.
pub fn main_window() -> Vec<Widget> {
    let w = |id, x, y, width, height, kind| Widget::new(id, Rect::new(x, y, width, height), kind);
    use WidgetKind::{Button, HitZone, Slider, Toggle};
    vec![
        w(TITLE_DRAG, 0, 0, WINDOW_WIDTH, 14, HitZone),
        w(OPTIONS, 6, 3, 9, 9, Button),
        w(MINIMIZE, 244, 3, 9, 9, Button),
        w(SHADE, 254, 3, 9, 9, Button),
        w(CLOSE, 264, 3, 9, 9, Button),
        // The clutter bar's letters are uneven: O and D are 8px tall, the
        // rest 7.
        w(CLUTTER_O, 10, 25, 8, 8, Button),
        w(CLUTTER_A, 10, 33, 8, 7, Toggle),
        w(CLUTTER_I, 10, 40, 8, 7, Button),
        w(CLUTTER_D, 10, 47, 8, 8, Toggle),
        w(CLUTTER_V, 10, 55, 8, 7, Button),
        w(TIME, 39, 26, 59, 13, HitZone),
        w(VISUALIZER, 24, 43, 76, 16, HitZone),
        w(VOLUME, 107, 57, 68, 13, Slider),
        w(BALANCE, 177, 57, 38, 13, Slider),
        w(EQ_TOGGLE, 219, 58, 23, 12, Toggle),
        w(PL_TOGGLE, 242, 58, 23, 12, Toggle),
        w(POSITION, 16, 72, 248, 10, Slider),
        // The transport row. Eject is 16px tall and one pixel lower than the
        // other five — a genuine quirk of the format, not a rounding error.
        w(PREVIOUS, 16, 88, 23, 18, Button),
        w(PLAY, 39, 88, 23, 18, Button),
        w(PAUSE, 62, 88, 23, 18, Button),
        w(STOP, 85, 88, 23, 18, Button),
        w(NEXT, 108, 88, 22, 18, Button),
        w(EJECT, 136, 89, 22, 16, Button),
        // Shuffle's sprite is 47 wide and repeat's overlaps its last column;
        // the hit area stops at 46 so a click there belongs to repeat, which
        // is drawn on top.
        w(SHUFFLE, 164, 89, 46, 15, Toggle),
        w(REPEAT, 210, 89, 28, 15, Toggle),
        w(ABOUT, 253, 91, 13, 15, HitZone),
    ]
}

/// The main window in windowshade mode: a 275x14 strip. Its transport
/// buttons are painted into the shade background, so they are invisible
/// hit zones; the widget ids are the full window's, so clicks act the same.
/// Positions from Webamp's `main-window.css` `.shade` rules.
pub fn main_shade() -> Vec<Widget> {
    let w = |id, x, y, width, height, kind| Widget::new(id, Rect::new(x, y, width, height), kind);
    use WidgetKind::{Button, HitZone, Slider};
    vec![
        w(TITLE_DRAG, 0, 0, WINDOW_WIDTH, 14, HitZone),
        w(OPTIONS, 6, 3, 9, 9, Button),
        w(VISUALIZER, 79, 5, 38, 5, HitZone),
        w(TIME, 127, 4, 25, 6, HitZone),
        w(PREVIOUS, 169, 2, 7, 10, HitZone),
        w(PLAY, 176, 2, 10, 10, HitZone),
        w(PAUSE, 186, 2, 9, 10, HitZone),
        w(STOP, 195, 2, 9, 10, HitZone),
        w(NEXT, 204, 2, 10, 10, HitZone),
        w(EJECT, 215, 2, 10, 10, HitZone),
        w(POSITION, 226, 4, 17, 7, Slider),
        w(MINIMIZE, 244, 3, 9, 9, Button),
        w(SHADE, 254, 3, 9, 9, Button),
        w(CLOSE, 264, 3, 9, 9, Button),
    ]
}

/// The sprite a button or toggle draws in its current state, at its rect's
/// origin. Sliders and displays draw themselves; see `main_window.rs`.
pub fn sprite_for(widget: &Widget) -> Option<SpriteKey> {
    let (pressed, active) = (widget.pressed, widget.active);
    let clutter = |b| (pressed || active).then_some(SpriteKey::ClutterBarButton(b));
    match widget.id {
        OPTIONS => Some(SpriteKey::OptionsButton { pressed }),
        MINIMIZE => Some(SpriteKey::MinimizeButton { pressed }),
        SHADE => Some(SpriteKey::ShadeButton { pressed }),
        CLOSE => Some(SpriteKey::CloseButton { pressed }),
        // The clutter bar's background already shows every letter; a letter
        // only gets its own sprite while held or switched on.
        CLUTTER_O => clutter(ClutterButton::O),
        CLUTTER_A => clutter(ClutterButton::A),
        CLUTTER_I => clutter(ClutterButton::I),
        CLUTTER_D => clutter(ClutterButton::D),
        CLUTTER_V => clutter(ClutterButton::V),
        EQ_TOGGLE => Some(SpriteKey::EqToggle { active, pressed }),
        PL_TOGGLE => Some(SpriteKey::PlaylistToggle { active, pressed }),
        PREVIOUS => Some(SpriteKey::Previous { pressed }),
        PLAY => Some(SpriteKey::Play { pressed }),
        PAUSE => Some(SpriteKey::Pause { pressed }),
        STOP => Some(SpriteKey::Stop { pressed }),
        NEXT => Some(SpriteKey::Next { pressed }),
        EJECT => Some(SpriteKey::Eject { pressed }),
        SHUFFLE => Some(SpriteKey::Shuffle { active, pressed }),
        REPEAT => Some(SpriteKey::Repeat { active, pressed }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::geometry::MAIN_HEIGHT;

    fn overlaps(a: &Rect, b: &Rect) -> bool {
        !(a.x + a.w as i32 <= b.x
            || b.x + b.w as i32 <= a.x
            || a.y + a.h as i32 <= b.y
            || b.y + b.h as i32 <= a.y)
    }

    #[test]
    fn interactive_widgets_do_not_overlap() {
        // The title drag zone deliberately sits under its buttons.
        let widgets: Vec<_> = main_window()
            .into_iter()
            .filter(|w| w.id != TITLE_DRAG)
            .collect();
        for (i, a) in widgets.iter().enumerate() {
            for b in &widgets[i + 1..] {
                assert!(
                    !overlaps(&a.rect, &b.rect),
                    "{:?} overlaps {:?}",
                    a.id,
                    b.id
                );
            }
        }
    }

    #[test]
    fn buttons_and_toggles_have_sprites() {
        for mut w in main_window() {
            if matches!(w.kind, WidgetKind::Button | WidgetKind::Toggle) {
                w.pressed = true;
                assert!(sprite_for(&w).is_some(), "{:?} has no sprite", w.id);
            }
        }
    }

    #[test]
    fn widgets_lie_within_the_window() {
        for w in main_window() {
            assert!(w.rect.x >= 0 && w.rect.y >= 0);
            assert!(
                w.rect.x + w.rect.w as i32 <= WINDOW_WIDTH as i32,
                "{:?} overflows the window width",
                w.id
            );
            assert!(
                w.rect.y + w.rect.h as i32 <= MAIN_HEIGHT as i32,
                "{:?} overflows the window height",
                w.id
            );
        }
    }

    #[test]
    fn title_buttons_win_over_the_drag_zone() {
        let widgets = main_window();
        assert_eq!(crate::ui::hit_test(&widgets, 268, 7), Some(CLOSE));
        assert_eq!(crate::ui::hit_test(&widgets, 100, 7), Some(TITLE_DRAG));
    }
}
