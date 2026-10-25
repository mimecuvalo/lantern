//! The main window's behaviour: transport, sliders, displays, keys.

use std::time::Instant;

use lantern_audio::engine::{Command, Engine, Event, Transport};
use lantern_audio::spectrum::Analyzer;
use winit::keyboard::{Key, NamedKey};
use winit::window::WindowLevel;

use super::{App, PaneKind};
use crate::ui::main_window::{self, MainView, Playback};
use crate::ui::marquee;
use crate::ui::{WidgetId, layout};

/// What the arrow keys move.
const SEEK_STEP_SECS: f64 = 5.0;
const VOLUME_STEP: f32 = 0.02;

impl App {
    pub(super) fn playback(&self) -> Playback {
        match self.engine.as_ref().map(Engine::transport) {
            Some(Transport::Playing) => Playback::Playing,
            Some(Transport::Paused) => Playback::Paused,
            _ => Playback::Stopped,
        }
    }

    fn duration(&self) -> Option<f64> {
        self.info
            .as_ref()
            .and_then(|i| i.duration_secs)
            .filter(|&d| d > 0.0)
    }

    /// The text the marquee should show right now.
    pub(super) fn marquee_text(&self) -> String {
        let main = PaneKind::Main;
        if self.dragging(main, layout::VOLUME) {
            return marquee::volume_text(self.volume);
        }
        if self.dragging(main, layout::BALANCE) {
            return marquee::balance_text(self.balance);
        }
        if let (Some(scrub), Some(duration)) = (self.scrub, self.duration()) {
            return marquee::seek_text(scrub, duration);
        }
        if let Some(text) = self.eq_marquee_text() {
            return text;
        }
        self.current_track_text(self.info.as_ref())
            .unwrap_or_else(|| "LANTERN".to_string())
    }

    pub(super) fn main_view(&self, now: Instant) -> MainView {
        let playback = self.playback();
        let elapsed = self.engine.as_ref().map_or(0.0, Engine::position_secs);
        let duration = self.duration();
        let position = match (playback, duration) {
            (Playback::Stopped, _) | (_, None) => None,
            (_, Some(d)) => Some(self.scrub.unwrap_or((elapsed / d) as f32)),
        };
        // Paused time blinks: a second shown, a second hidden.
        let time_visible = self
            .paused_since
            .is_none_or(|t| now.duration_since(t).as_secs().is_multiple_of(2));
        MainView {
            focused: self.pane(PaneKind::Main).is_some_and(|p| p.focused),
            playback,
            elapsed_secs: elapsed,
            duration_secs: duration,
            show_remaining: self.show_remaining,
            time_visible,
            kbps: self.info.as_ref().and_then(|i| i.kbps),
            sample_rate: self.info.as_ref().map(|i| i.sample_rate),
            channels: self.info.as_ref().map(|i| i.channels),
            marquee: self.marquee.layout(&self.marquee_text()),
            volume: self.volume,
            balance: self.balance,
            position,
            bars: self
                .analyzer
                .as_ref()
                .filter(|_| self.analyser_on)
                .map(Analyzer::bars),
        }
    }

    /// Advance everything time-driven: engine events, analyser, marquee.
    pub(super) fn tick(&mut self, now: Instant) {
        while let Some(event) = self.engine.as_mut().and_then(Engine::poll_event) {
            match event {
                Event::Opened(info) | Event::Advanced(info) => {
                    self.track_started(&info);
                    // A cue sheet's later tracks are the same file: count
                    // the file once, from its start.
                    if info.start_secs < 1e-3 {
                        self.library_played(&info.path);
                    }
                    self.info = Some(info);
                    self.marquee.reset(now);
                    // Queue what follows now, so the decoder has it in hand
                    // when this track runs dry.
                    self.requeue();
                }
                Event::OpenFailed { path, error } => {
                    eprintln!("cannot open {}: {error}", path.display());
                }
                // Played through: stopped on the last track, as in Winamp.
                Event::Ended => self.request_redraw(PaneKind::Playlist),
            }
        }
        self.apply_tags();

        let playback = self.playback();
        match (playback, self.paused_since) {
            (Playback::Paused, None) => self.paused_since = Some(now),
            (Playback::Paused, Some(_)) => {}
            _ => self.paused_since = None,
        }
        // A paused analyser freezes, as Winamp's does, rather than decaying.
        if playback != Playback::Paused
            && let Some(analyzer) = &mut self.analyzer
        {
            analyzer.update(now);
        }
        self.marquee.tick(now);
    }

    /// Resume if paused; otherwise start the current entry from the top.
    pub(super) fn play(&mut self) {
        match self.engine.as_ref().map(Engine::transport) {
            Some(Transport::Paused) => self.send(Command::Play),
            Some(_) => {
                if let Some(i) = self
                    .playlist
                    .current()
                    .or((!self.playlist.is_empty()).then_some(0))
                {
                    self.play_entry(i);
                }
            }
            None => {}
        }
    }

    pub(super) fn toggle_pause(&self) {
        match self.playback() {
            Playback::Paused => self.send(Command::Play),
            Playback::Playing => self.send(Command::Pause),
            Playback::Stopped => {}
        }
    }

    pub(super) fn set_volume(&mut self, volume: f32) {
        self.volume = volume.clamp(0.0, 1.0);
        self.send(Command::SetVolume(self.volume));
        // The shaded EQ shows volume too.
        self.request_redraw(PaneKind::Equalizer);
    }

    pub(super) fn set_balance(&mut self, balance: f32) {
        self.balance = balance.clamp(-1.0, 1.0);
        self.send(Command::SetBalance(self.balance));
        self.request_redraw(PaneKind::Equalizer);
    }

    fn seek_by(&self, delta: f64) {
        if self.playback() == Playback::Stopped {
            return;
        }
        let now = self.engine.as_ref().map_or(0.0, Engine::position_secs);
        let target = (now + delta).max(0.0);
        let target = self.duration().map_or(target, |d| target.min(d));
        self.send(Command::Seek { seconds: target });
    }

    /// A main-window slider moved to logical x `x`, while dragged.
    pub(super) fn drag_main_slider(&mut self, id: WidgetId, x: i32) {
        let Some(rect) = self
            .pane(PaneKind::Main)
            .and_then(|p| p.widgets.iter().find(|w| w.id == id))
            .map(|w| w.rect)
        else {
            return;
        };
        let local = x - rect.x;
        match id {
            layout::VOLUME => self.set_volume(main_window::volume_from_x(local)),
            layout::BALANCE => self.set_balance(main_window::balance_from_x(local)),
            layout::POSITION
                if self.playback() != Playback::Stopped && self.duration().is_some() =>
            {
                let shaded = self.pane(PaneKind::Main).is_some_and(|p| p.shaded);
                self.scrub = Some(if shaded {
                    main_window::shade_position_from_x(local)
                } else {
                    main_window::position_from_x(local)
                });
            }
            _ => {}
        }
    }

    /// The position slider seeks only on release, so a drag does not stutter
    /// through every intermediate position.
    pub(super) fn release_main_slider(&mut self, id: WidgetId) {
        if id == layout::POSITION
            && let (Some(fraction), Some(duration)) = (self.scrub.take(), self.duration())
        {
            self.send(Command::Seek {
                seconds: f64::from(fraction) * duration,
            });
        }
    }

    /// Act on a completed click in the main window.
    pub(super) fn activate_main(&mut self, id: WidgetId) {
        let main = PaneKind::Main;
        match id {
            layout::PLAY => self.play(),
            layout::PAUSE => self.toggle_pause(),
            layout::STOP => self.send(Command::Stop),
            layout::CLOSE => self.quit_requested = true,
            layout::MINIMIZE => {
                for p in &self.panes {
                    p.window.set_minimized(true);
                }
            }
            layout::TIME => self.show_remaining = !self.show_remaining,
            layout::VISUALIZER => self.analyser_on = !self.analyser_on,
            layout::CLUTTER_A => {
                let on = self.toggle(main, id);
                if let Some(item) = &self.aot_item {
                    item.set_checked(on);
                }
                let level = if on {
                    WindowLevel::AlwaysOnTop
                } else {
                    WindowLevel::Normal
                };
                for p in &self.panes {
                    p.window.set_window_level(level);
                }
            }
            layout::CLUTTER_D => {
                self.doubled = self.toggle(main, id);
                // Winamp 2 doubles the main and EQ windows; the playlist
                // keeps its size.
                self.apply_size(main);
                self.apply_size(PaneKind::Equalizer);
            }
            layout::EQ_TOGGLE => {
                let show = !self.pane(PaneKind::Equalizer).is_some_and(|p| p.visible);
                self.set_visible(PaneKind::Equalizer, show);
            }
            layout::PL_TOGGLE => {
                let show = !self.pane(PaneKind::Playlist).is_some_and(|p| p.visible);
                self.set_visible(PaneKind::Playlist, show);
            }
            layout::SHUFFLE | layout::REPEAT => {
                self.toggle(main, id);
                // What follows the current track may have changed.
                self.requeue();
            }
            layout::PREVIOUS => self.skip(false),
            layout::NEXT => self.skip(true),
            layout::EJECT => self.eject(),
            layout::SHADE => self.toggle_shade(main),
            layout::OPTIONS | layout::CLUTTER_O => {
                self.show_menu(super::menus::MenuKind::Main, main);
            }
            layout::CLUTTER_I => self.show_file_info(),
            layout::CLUTTER_V => self.analyser_on = !self.analyser_on,
            _ => {}
        }
    }

    pub(super) fn key(&mut self, key: &Key) {
        // Winamp's keys: Z X C V B along the bottom row drive the transport.
        match key {
            Key::Character(c) if c.as_str() == "q" && self.modifiers.super_key() => {
                self.quit_requested = true;
            }
            Key::Character(c) => match c.to_lowercase().as_str() {
                "z" => self.skip(false),
                "b" => self.skip(true),
                "x" => self.play(),
                "c" => self.toggle_pause(),
                "v" => self.send(Command::Stop),
                // Winamp's L opens files, as eject does.
                "l" => self.eject(),
                // And J jumps to a file in the playlist. I, for File Info,
                // is Lantern's own: Winamp's is Alt+3, which works too.
                "j" => self.open_jump(),
                "i" => self.show_file_info(),
                _ => {}
            },
            Key::Named(NamedKey::ArrowLeft) => self.seek_by(-SEEK_STEP_SECS),
            Key::Named(NamedKey::ArrowRight) => self.seek_by(SEEK_STEP_SECS),
            Key::Named(NamedKey::ArrowUp) => self.set_volume(self.volume + VOLUME_STEP),
            Key::Named(NamedKey::ArrowDown) => self.set_volume(self.volume - VOLUME_STEP),
            _ => return,
        }
        self.request_redraw(PaneKind::Main);
    }
}
