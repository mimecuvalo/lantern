//! The equaliser window's behaviour.

use lantern_audio::engine::Command;
use lantern_audio::eq::BAND_FREQUENCIES;

use super::{App, PaneKind};
use crate::ui::WidgetId;
use crate::ui::eq_window::{self, EqView};

impl App {
    pub(super) fn eq_view(&self) -> EqView {
        EqView {
            focused: self.pane(PaneKind::Equalizer).is_some_and(|p| p.focused),
            enabled: self.eq.enabled,
            auto: self.eq_auto,
            preamp: eq_window::level_from_db(self.eq.preamp_db),
            bands: self.eq.band_gains_db.map(eq_window::level_from_db),
        }
    }

    fn push_eq(&self) {
        self.send(Command::SetEq(self.eq));
        self.request_redraw(PaneKind::Equalizer);
    }

    /// An EQ slider dragged to logical y `y`.
    pub(super) fn drag_eq_slider(&mut self, id: WidgetId, x: i32, y: i32) {
        let Some(rect) = self
            .pane(PaneKind::Equalizer)
            .and_then(|p| p.widgets.iter().find(|w| w.id == id))
            .map(|w| w.rect)
        else {
            return;
        };
        match id {
            eq_window::SHADE_VOLUME => {
                self.set_volume(eq_window::shade_fraction_from_x(x - rect.x, rect.w));
                self.request_redraw(PaneKind::Main);
                return;
            }
            eq_window::SHADE_BALANCE => {
                let f = eq_window::shade_fraction_from_x(x - rect.x, rect.w);
                let b = f * 2.0 - 1.0;
                // Snap to centre, as the main balance slider does.
                self.set_balance(if b.abs() < 0.1 { 0.0 } else { b });
                self.request_redraw(PaneKind::Main);
                return;
            }
            _ => {}
        }
        let db = eq_window::db_from_level(eq_window::level_from_y(y - rect.y));
        if id == eq_window::PREAMP {
            self.eq.preamp_db = db;
        } else if let Some(i) = eq_window::band_index(id) {
            self.eq.band_gains_db[i] = db;
        } else {
            return;
        }
        self.push_eq();
        // The marquee narrates the drag.
        self.request_redraw(PaneKind::Main);
    }

    /// While an EQ slider is held, the main window's marquee reads it out,
    /// as Winamp's does.
    pub(super) fn eq_marquee_text(&self) -> Option<String> {
        let id = self.pane(PaneKind::Equalizer)?.input.active?;
        let fmt = |db: f32| format!("{}{db:.1} DB", if db >= 0.0 { "+" } else { "" });
        if id == eq_window::PREAMP {
            return Some(format!("PREAMP: {}", fmt(self.eq.preamp_db)));
        }
        let i = eq_window::band_index(id)?;
        let hz = BAND_FREQUENCIES[i];
        let label = if hz >= 1000.0 {
            format!("{}KHZ", hz / 1000.0)
        } else {
            format!("{hz}HZ")
        };
        Some(format!("EQ: {label}: {}", fmt(self.eq.band_gains_db[i])))
    }

    pub(super) fn activate_eq(&mut self, id: WidgetId) {
        let eq = PaneKind::Equalizer;
        match id {
            eq_window::CLOSE => self.set_visible(eq, false),
            eq_window::ON => {
                self.eq.enabled = self.toggle(eq, id);
                self.push_eq();
            }
            // Per-file preset auto-loading needs a preset library; the toggle
            // is remembered, but does nothing yet.
            eq_window::AUTO => self.eq_auto = self.toggle(eq, id),
            eq_window::PRESETS => self.show_menu(super::menus::MenuKind::Presets, eq),
            eq_window::SHADE => self.toggle_shade(eq),
            _ => {}
        }
    }
}
