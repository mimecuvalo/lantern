//! The widget layer.
//!
//! There is no GUI framework here and there will not be one. Classic Winamp
//! geometry is fixed by the skin format — a skin repaints a control, it never
//! moves one — so there is no layout to compute, no reflow, no constraints.
//! A widget is a rectangle, a sprite key and a state. Hit testing is a linear
//! scan over roughly twenty-five of them, which beats any spatial index and
//! is far easier to debug.

pub mod blit;
pub mod eq_window;
pub mod font;
pub mod geometry;
pub mod layout;
pub mod main_window;
pub mod marquee;
pub mod playlist_window;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WidgetId(pub u16);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WidgetKind {
    Button,
    Toggle,
    /// Horizontal slider. Drags are routed here until mouse-up, so the
    /// pointer may leave the rect mid-gesture.
    Slider,
    /// Draws but does not accept input.
    Display,
    /// Accepts input but draws nothing itself — the title drag strip, or a
    /// display that responds to clicks, like the time.
    HitZone,
}

#[derive(Debug, Clone, Copy)]
pub struct Widget {
    pub id: WidgetId,
    pub rect: geometry::Rect,
    pub kind: WidgetKind,
    /// The mouse went down on it and has not been released.
    pub pressed: bool,
    /// Switched on, for toggles.
    pub active: bool,
}

impl Widget {
    pub const fn new(id: WidgetId, rect: geometry::Rect, kind: WidgetKind) -> Self {
        Self {
            id,
            rect,
            kind,
            pressed: false,
            active: false,
        }
    }
}

/// Input routing.
///
/// On mouse-down the hit widget is latched into `active` and receives every
/// subsequent motion until mouse-up, which is what lets a slider drag continue
/// off-widget the way it does in Winamp.
#[derive(Debug, Default)]
pub struct Input {
    pub cursor: Option<(i32, i32)>,
    pub active: Option<WidgetId>,
}

/// Topmost widget containing the point, or none.
pub fn hit_test(widgets: &[Widget], x: i32, y: i32) -> Option<WidgetId> {
    widgets
        .iter()
        .rev()
        .find(|w| w.kind != WidgetKind::Display && w.rect.contains(x, y))
        .map(|w| w.id)
}
