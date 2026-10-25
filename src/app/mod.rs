//! The application: four skinned windows, one audio engine.
//!
//! Each window is a [`Pane`] — a winit window, its softbuffer surface and its
//! widget table. Behaviour lives beside it: `player.rs` for the main window,
//! `equalizer.rs` for the EQ. Events arrive tagged with a `WindowId` and are
//! routed to the pane they belong to.
//!
//! **Docking.** Windows are dragged by hand rather than by the system, so
//! that they can snap: to each other's edges and to the screen's, as in
//! Winamp (see `snap.rs`). Windows whose edges touch are attached; the
//! main window carries everything attached to it, and a window that grows
//! or shrinks pushes along those attached below it or to its right.

mod dialogs;
mod equalizer;
mod library;
mod library_ui;
mod media;
mod menus;
mod player;
mod playlist;
pub mod settings;
mod snap;
mod tags;

use std::num::NonZeroU32;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use lantern_audio::engine::{Command, Engine, TrackInfo};
use lantern_audio::eq::EqSettings;
use lantern_audio::spectrum::Analyzer;
use lantern_playlist::Playlist;
use lantern_skin::Skin;
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalPosition, LogicalSize, Position};
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow};
use winit::keyboard::{KeyCode, ModifiersState, PhysicalKey};
use winit::window::{Window, WindowId};

use crate::ui::blit::Surface;
use crate::ui::font::TextFont;
use crate::ui::geometry::{EQ_HEIGHT, MAIN_HEIGHT, SHADE_HEIGHT, WINDOW_WIDTH};
use crate::ui::marquee::Marquee;
use crate::ui::{Input, Widget, WidgetId, WidgetKind, eq_window, hit_test, layout, main_window};
use crate::ui::{library_window, playlist_window};

/// Redraw interval while audio plays: the analyser wants 60fps.
const PLAYING_FRAME: Duration = Duration::from_micros(16_667);
/// Redraw interval for slower animation — the paused blink, a scrolling
/// title while stopped. Fine enough for a 220ms marquee step.
const IDLE_FRAME: Duration = Duration::from_millis(50);
const AUTOSAVE_EVERY: Duration = Duration::from_secs(10);
/// Two presses within this are a double-click.
const DOUBLE_CLICK: Duration = Duration::from_millis(400);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaneKind {
    Main,
    Equalizer,
    Playlist,
    Library,
}

impl PaneKind {
    /// Double size magnifies these; the resizable windows stay put.
    fn doubles(self) -> bool {
        matches!(self, PaneKind::Main | PaneKind::Equalizer)
    }
}

struct Pane {
    kind: PaneKind,
    window: Arc<Window>,
    surface: softbuffer::Surface<Arc<Window>, Arc<Window>>,
    widgets: Vec<Widget>,
    input: Input,
    focused: bool,
    visible: bool,
    /// Rolled up to a 14px strip.
    shaded: bool,
}

pub struct App {
    skin: Option<Skin>,
    engine: Option<Engine>,
    analyzer: Option<Analyzer>,
    playlist: Playlist,
    /// The entry the engine has queued to follow the current one.
    queued: Option<usize>,
    /// First row in view.
    pl_scroll: usize,
    /// For double-click detection: when, and on which row.
    pl_last_click: Option<(Instant, usize)>,
    /// While resizing: the grip's offset from the window's corner.
    pl_resize: Option<(i32, i32)>,
    /// The bottom-bar menu that is open, if any.
    pl_menu: Option<playlist_window::OpenMenu>,
    /// The open menu was opened by the press still held, and the pointer
    /// has not left the item under its button.
    pl_menu_fresh: bool,
    /// The skin's playlist font, or a fallback; `None` if no font at all.
    font: Option<TextFont>,
    tags: tags::TagLoader,
    rng: u64,
    /// The playing second the playlist last drew, so its mini time repaints
    /// once a second rather than every frame.
    pl_drawn_second: Option<u64>,
    info: Option<TrackInfo>,
    modifiers: ModifiersState,

    volume: f32,
    balance: f32,
    /// Where the position slider is being dragged to, before release seeks.
    scrub: Option<f32>,
    show_remaining: bool,
    analyser_on: bool,
    /// Double-size mode: main and EQ windows at 2x logical size.
    doubled: bool,
    marquee: Marquee,
    /// When pause began, to phase the time display's blink.
    paused_since: Option<Instant>,

    eq: EqSettings,
    eq_auto: bool,

    /// Logical size of the playlist window, which the user can resize.
    playlist_size: (u32, u32),
    /// Logical size of the library window, likewise.
    library_size: (u32, u32),
    /// While resizing the library: the grip's offset from its corner.
    lib_resize: Option<(i32, i32)>,
    /// The media library: its database, its tracks, its worker.
    library: library::LibraryState,
    /// What the library window has selected, typed and scrolled to.
    lib_ui: library_ui::LibraryUi,

    /// When the next animation frame is due.
    next_frame_at: Option<Instant>,
    context: Option<softbuffer::Context<Arc<Window>>>,
    panes: Vec<Pane>,

    /// The skin file in use, remembered for next time.
    skin_path: Option<PathBuf>,
    /// Settings as loaded: window places and toggles apply once the windows
    /// exist.
    restored: settings::Settings,
    /// Files were given on the command line, so play them straight away.
    autoplay: bool,
    /// Kept alive for as long as it is the app's menu bar.
    menu_bar: Option<muda::Menu>,
    /// The menu bar's Always on Top item, whose check follows the A toggle.
    aot_item: Option<muda::CheckMenuItem>,
    /// The system's media controls.
    #[cfg(target_os = "macos")]
    media: Option<crate::platform::macos::MediaRemote>,
    /// What Now Playing last showed.
    published: Option<media::Published>,
    /// Bumped whenever playback jumps (open, seek), so Now Playing
    /// republishes its position.
    position_epoch: std::cell::Cell<u64>,
    /// Wakes the event loop from other threads.
    proxy: Option<winit::event_loop::EventLoopProxy<()>>,
    /// The last title-bar press, for spotting a double-click.
    title_click: Option<(PaneKind, Instant)>,
    /// The window being dragged by its title bar, if any.
    drag: Option<Drag>,
    /// Windows are being put back as saved: a size change moves nothing
    /// else.
    restoring: bool,
    /// Quit at the next turn of the loop — for actions that run where the
    /// event loop is not to hand, such as menu choices.
    quit_requested: bool,
    /// What was last written to disk, so autosave only writes on change.
    last_saved: Option<(settings::Settings, String)>,
    last_save_check: Instant,
}

/// The built-in skin, also called Lantern: original artwork, drawn in code by
/// `crates/lantern-skin/examples/make_default_skin.rs` and built in.
const DEFAULT_SKIN: &[u8] = include_bytes!("../../assets/default-skin.wsz");

pub fn default_skin() -> Option<Skin> {
    lantern_skin::parse(DEFAULT_SKIN)
        .inspect_err(|e| eprintln!("the built-in skin is damaged: {e}"))
        .ok()
}

impl App {
    /// `paths` from the command line replace the saved playlist and start
    /// playing, as in Winamp; without them the saved playlist comes back,
    /// stopped.
    pub fn new(
        skin: Option<Skin>,
        skin_path: Option<PathBuf>,
        paths: Vec<PathBuf>,
        restored: settings::Settings,
        saved_playlist: Vec<lantern_playlist::Entry>,
    ) -> Self {
        let font_name = skin
            .as_ref()
            .map_or("Arial".to_string(), |s| s.playlist_style.font.clone());
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0x9E37_79B9_7F4A_7C15, |d| d.as_nanos() as u64)
            | 1;
        let mut app = Self {
            skin,
            engine: None,
            analyzer: None,
            playlist: Playlist::default(),
            queued: None,
            pl_scroll: 0,
            pl_last_click: None,
            pl_resize: None,
            pl_menu: None,
            pl_menu_fresh: false,
            font: TextFont::load(&font_name),
            tags: tags::TagLoader::new(),
            rng: seed,
            pl_drawn_second: None,
            info: None,
            modifiers: ModifiersState::default(),
            volume: restored.volume,
            balance: restored.balance,
            scrub: None,
            show_remaining: restored.show_remaining,
            analyser_on: restored.analyser_on,
            doubled: restored.doubled,
            marquee: Marquee::new(Instant::now()),
            paused_since: None,
            eq: restored.eq,
            eq_auto: restored.eq_auto,
            playlist_size: playlist_window::snap_size(
                restored.playlist_size.0 as i32,
                restored.playlist_size.1 as i32,
            ),
            library_size: library::min_library_size(playlist_window::snap_size(
                restored.library_size.0 as i32,
                restored.library_size.1 as i32,
            )),
            lib_resize: None,
            library: library::LibraryState::open(settings::config_dir()),
            lib_ui: library_ui::LibraryUi::default(),
            next_frame_at: None,
            context: None,
            panes: Vec::new(),
            skin_path,
            autoplay: !paths.is_empty(),
            restored,
            menu_bar: None,
            aot_item: None,
            #[cfg(target_os = "macos")]
            media: None,
            published: None,
            position_epoch: std::cell::Cell::new(0),
            proxy: None,
            title_click: None,
            drag: None,
            restoring: false,
            quit_requested: false,
            last_saved: None,
            last_save_check: Instant::now(),
        };
        app.lib_ui.tree_width =
            library_window::clamp_tree_width(app.library_size.0, app.restored.library_tree_width);
        app.lib_ui.columns = library_ui::columns_from_settings(&app.restored.library_columns);
        if app.font.is_none() {
            eprintln!("no usable font for the playlist; titles will not show");
        }
        if paths.is_empty() {
            app.add_entries(saved_playlist);
            app.playlist.set_current(app.restored.current);
        } else {
            app.add_paths(paths);
        }
        app
    }

    /// The state worth keeping, as it stands now.
    fn snapshot(&self) -> settings::Settings {
        let widget_on = |kind, id| {
            self.pane(kind)
                .and_then(|p| p.widgets.iter().find(|w| w.id == id))
                .is_some_and(|w| w.active)
        };
        let place = |kind| {
            let fallback = match kind {
                PaneKind::Main => self.restored.main,
                PaneKind::Equalizer => self.restored.equalizer,
                PaneKind::Playlist => self.restored.playlist,
                PaneKind::Library => self.restored.library,
            };
            self.pane(kind).map_or(fallback, |p| settings::WindowPlace {
                position: p.window.outer_position().ok().map(|pos| {
                    let l: LogicalPosition<f64> = pos.to_logical(p.window.scale_factor());
                    (l.x, l.y)
                }),
                visible: p.visible,
                shaded: p.shaded,
            })
        };
        settings::Settings {
            volume: self.volume,
            balance: self.balance,
            eq: self.eq,
            eq_auto: self.eq_auto,
            shuffle: widget_on(PaneKind::Main, layout::SHUFFLE),
            repeat: widget_on(PaneKind::Main, layout::REPEAT),
            show_remaining: self.show_remaining,
            analyser_on: self.analyser_on,
            doubled: self.doubled,
            always_on_top: widget_on(PaneKind::Main, layout::CLUTTER_A),
            skin: self.skin_path.clone(),
            main: place(PaneKind::Main),
            equalizer: place(PaneKind::Equalizer),
            playlist: place(PaneKind::Playlist),
            playlist_size: self.playlist_size,
            library: place(PaneKind::Library),
            library_size: self.library_size,
            library_tree_width: self.lib_ui.tree_width,
            library_columns: library_ui::columns_to_settings(&self.lib_ui.columns),
            current: self.playlist.current(),
        }
    }

    pub(crate) fn save_settings(&mut self) {
        let state = (
            self.snapshot(),
            lantern_playlist::m3u::write_m3u8(self.playlist.entries()),
        );
        if self.last_saved.as_ref() != Some(&state) {
            settings::save(&state.0, &state.1);
            self.last_saved = Some(state);
        }
    }

    /// Save every few seconds if anything changed, so a crash or a forced
    /// quit loses little.
    fn autosave(&mut self, now: Instant) {
        if now.duration_since(self.last_save_check) >= AUTOSAVE_EVERY {
            self.last_save_check = now;
            self.save_settings();
        }
    }

    fn pane(&self, kind: PaneKind) -> Option<&Pane> {
        self.panes.iter().find(|p| p.kind == kind)
    }

    fn pane_mut(&mut self, kind: PaneKind) -> Option<&mut Pane> {
        self.panes.iter_mut().find(|p| p.kind == kind)
    }

    fn pane_of(&self, id: WindowId) -> Option<PaneKind> {
        self.panes
            .iter()
            .find(|p| p.window.id() == id)
            .map(|p| p.kind)
    }

    /// The pane's size in skin pixels, before any magnification.
    fn logical_size(&self, kind: PaneKind) -> (u32, u32) {
        let shaded = self.pane(kind).is_some_and(|p| p.shaded);
        let (w, h) = match kind {
            PaneKind::Main => (WINDOW_WIDTH, MAIN_HEIGHT),
            PaneKind::Equalizer => (WINDOW_WIDTH, EQ_HEIGHT),
            PaneKind::Playlist => self.playlist_size,
            PaneKind::Library => self.library_size,
        };
        (w, if shaded { SHADE_HEIGHT } else { h })
    }

    /// Roll a window up to its 14px strip, or back down.
    pub(crate) fn toggle_shade(&mut self, kind: PaneKind) {
        // The library has no windowshade mode, as in Winamp.
        if kind == PaneKind::Library {
            return;
        }
        let (width, height) = self.playlist_size;
        // Toggle states live on the widgets about to be replaced: read them
        // first.
        let before = self.snapshot();
        let Some(pane) = self.pane_mut(kind) else {
            return;
        };
        pane.shaded = !pane.shaded;
        pane.widgets = match (kind, pane.shaded) {
            (PaneKind::Main, false) => layout::main_window(),
            (PaneKind::Main, true) => layout::main_shade(),
            (PaneKind::Equalizer, false) => eq_window::widgets(),
            (PaneKind::Equalizer, true) => eq_window::shade_widgets(),
            (PaneKind::Playlist, false) => playlist_window::widgets(width, height),
            (PaneKind::Playlist, true) => playlist_window::shade_widgets(width),
            (PaneKind::Library, _) => return,
        };
        pane.input.active = None;
        if kind == PaneKind::Main {
            // The main window's toggles live on its widgets; carry them over.
            self.restore_toggles(&before);
        } else if kind == PaneKind::Equalizer {
            let (on, auto) = (self.eq.enabled, self.eq_auto);
            for (id, active) in [(eq_window::ON, on), (eq_window::AUTO, auto)] {
                if let Some(w) = self.widget_mut(kind, id) {
                    w.active = active;
                }
            }
        }
        self.apply_size(kind);
        if kind == PaneKind::Main {
            self.apply_region();
        }
        self.request_redraw(kind);
    }

    /// Size the window to its logical size, doubled where double size
    /// applies. Windows attached below it or to its right move with its
    /// bottom and right edges.
    fn apply_size(&self, kind: PaneKind) {
        let (w, h) = self.logical_size(kind);
        let factor = if self.doubled && kind.doubles() {
            2.0
        } else {
            1.0
        };
        let (w, h) = (f64::from(w) * factor, f64::from(h) * factor);
        let (kinds, rects, _) = self.frames();
        if let Some(p) = self.pane(kind) {
            let _ = p.window.request_inner_size(LogicalSize::new(w, h));
        }
        if self.restoring {
            return;
        }
        let Some(i) = kinds.iter().position(|&k| k == kind) else {
            return;
        };
        let new = snap::Rect::new(rects[i].x, rects[i].y, w, h);
        for (j, dx, dy) in snap::follow_resize(i, new, &rects) {
            self.place(kinds[j], rects[j].x + dx, rects[j].y + dy);
        }
    }

    /// A window's frame in logical screen points.
    fn rect(&self, kind: PaneKind) -> Option<snap::Rect> {
        let p = self.pane(kind)?;
        let scale = p.window.scale_factor();
        let at: LogicalPosition<f64> = p.window.outer_position().ok()?.to_logical(scale);
        let size: LogicalSize<f64> = p.window.outer_size().to_logical(scale);
        Some(snap::Rect::new(at.x, at.y, size.width, size.height))
    }

    /// Every window's frame, and whether it shows. Hidden windows keep
    /// their places — attached ones travel with the group, to reappear
    /// where they belong — but only showing ones attract.
    fn frames(&self) -> (Vec<PaneKind>, Vec<snap::Rect>, Vec<bool>) {
        let mut out = (Vec::new(), Vec::new(), Vec::new());
        for p in &self.panes {
            if let Some(r) = self.rect(p.kind) {
                out.0.push(p.kind);
                out.1.push(r);
                out.2.push(p.visible);
            }
        }
        out
    }

    fn place(&self, kind: PaneKind, x: f64, y: f64) {
        if let Some(p) = self.pane(kind) {
            p.window.set_outer_position(LogicalPosition::new(x, y));
        }
    }

    /// The pointer on screen, in logical points.
    fn global_cursor(&self, kind: PaneKind) -> Option<(f64, f64)> {
        #[cfg(target_os = "macos")]
        {
            let _ = kind;
            crate::platform::macos::cursor_position()
        }
        #[cfg(not(target_os = "macos"))]
        {
            // Skin pixels, to physical, to logical points.
            let (x, y) = self.pane(kind)?.input.cursor?;
            let r = self.rect(kind)?;
            let px = f64::from(self.scale(kind)) / self.pane(kind)?.window.scale_factor();
            Some((r.x + f64::from(x) * px, r.y + f64::from(y) * px))
        }
    }

    /// The areas a window may snap to the edges of.
    fn screens(&self, kind: PaneKind) -> Vec<snap::Rect> {
        #[cfg(target_os = "macos")]
        {
            let _ = kind;
            crate::platform::macos::visible_screen_rects()
                .into_iter()
                .map(|(x, y, w, h)| snap::Rect::new(x, y, w, h))
                .collect()
        }
        #[cfg(not(target_os = "macos"))]
        {
            self.pane(kind)
                .into_iter()
                .flat_map(|p| p.window.available_monitors())
                .map(|m| {
                    let at: LogicalPosition<f64> = m.position().to_logical(m.scale_factor());
                    let size: LogicalSize<f64> = m.size().to_logical(m.scale_factor());
                    snap::Rect::new(at.x, at.y, size.width, size.height)
                })
                .collect()
        }
    }

    /// Take hold of a window by its title bar. The main window brings along
    /// every window attached to it; any other comes alone.
    fn start_drag(&mut self, kind: PaneKind) {
        let Some(cursor) = self.global_cursor(kind) else {
            return;
        };
        let (kinds, rects, shown) = self.frames();
        let Some(i) = kinds.iter().position(|&k| k == kind) else {
            return;
        };
        let group = if kind == PaneKind::Main {
            snap::attached(i, &rects)
        } else {
            vec![i]
        };
        self.drag = Some(Drag {
            cursor,
            group: group.iter().map(|&j| (kinds[j], rects[j])).collect(),
            snapping: group
                .iter()
                .filter(|&&j| shown[j])
                .map(|&j| rects[j])
                .collect(),
            fixed: (0..rects.len())
                .filter(|&j| shown[j] && !group.contains(&j))
                .map(|j| rects[j])
                .collect(),
            screens: self.screens(kind),
        });
    }

    /// The pointer moved while dragging: move the group with it, snapped.
    fn continue_drag(&self, kind: PaneKind) {
        let (Some(drag), Some((x, y))) = (&self.drag, self.global_cursor(kind)) else {
            return;
        };
        let offset = (x - drag.cursor.0, y - drag.cursor.1);
        let (dx, dy) = snap::snap(&drag.snapping, offset, &drag.fixed, &drag.screens);
        for &(k, r) in &drag.group {
            self.place(k, r.x + dx, r.y + dy);
        }
    }

    /// Integer magnification from skin pixels to physical pixels: Retina's
    /// 2x, times 2 again in double-size mode. Fractional factors floor, and
    /// the remainder is left as background rather than smeared.
    fn scale(&self, kind: PaneKind) -> u32 {
        let Some(pane) = self.pane(kind) else {
            return 1;
        };
        let size = pane.window.inner_size();
        let (w, h) = self.logical_size(kind);
        (size.width / w).min(size.height / h).max(1)
    }

    fn widget_mut(&mut self, kind: PaneKind, id: WidgetId) -> Option<&mut Widget> {
        self.pane_mut(kind)?.widgets.iter_mut().find(|w| w.id == id)
    }

    /// Hand over the loop's proxy, before `run_app`.
    pub fn set_proxy(&mut self, proxy: winit::event_loop::EventLoopProxy<()>) {
        self.proxy = Some(proxy);
    }

    fn send(&self, command: Command) {
        if matches!(command, Command::Open(_) | Command::Seek { .. }) {
            self.position_epoch.set(self.position_epoch.get() + 1);
        }
        if let Some(engine) = &self.engine
            && let Err(e) = engine.send(command)
        {
            eprintln!("engine command failed: {e}");
        }
    }

    fn request_redraw(&self, kind: PaneKind) {
        if let Some(p) = self.pane(kind) {
            p.window.request_redraw();
        }
    }

    fn render(&mut self, kind: PaneKind) {
        let now = Instant::now();
        let scale = self.scale(kind);
        let lib_view = (kind == PaneKind::Library).then(|| self.library_view());
        let skin = self.skin.as_ref();
        let shaded = self.pane(kind).is_some_and(|p| p.shaded);
        let (main_view, eq_view, pl_view) = match kind {
            PaneKind::Main => (Some(self.main_view(now)), None, None),
            PaneKind::Equalizer => (None, Some(self.eq_view()), None),
            PaneKind::Playlist => (None, None, Some(self.playlist_view())),
            PaneKind::Library => (None, None, None),
        };
        let eq_shade = eq_window::EqShadeView {
            focused: eq_view.as_ref().is_some_and(|v| v.focused),
            volume: self.volume,
            balance: self.balance,
        };
        let pl_shade = self.playlist_shade_text();
        let Some(pane) = self.panes.iter_mut().find(|p| p.kind == kind) else {
            return;
        };
        let size = pane.window.inner_size();
        let (Some(w), Some(h)) = (NonZeroU32::new(size.width), NonZeroU32::new(size.height)) else {
            return;
        };
        if pane.surface.resize(w, h).is_err() {
            return;
        }
        let Ok(mut buffer) = pane.surface.buffer_mut() else {
            return;
        };
        let mut dst = Surface::new(&mut buffer, size.width, size.height);
        let widgets = &pane.widgets;
        match (main_view, eq_view, pl_view) {
            (Some(v), ..) if shaded => main_window::draw_shade(&mut dst, skin, &v, widgets, scale),
            (Some(v), ..) => main_window::draw(&mut dst, skin, &v, widgets, scale),
            (_, Some(_), _) if shaded => {
                eq_window::draw_shade(&mut dst, skin, &eq_shade, widgets, scale);
            }
            (_, Some(v), _) => eq_window::draw(&mut dst, skin, &v, widgets, scale),
            (.., Some(v)) if shaded => {
                let text = (pl_shade.0.as_str(), pl_shade.1.as_str());
                let (w, focused) = (v.width, v.focused);
                playlist_window::draw_shade(&mut dst, skin, w, focused, text, widgets, scale);
            }
            (.., Some(v)) => {
                let font = self.font.as_ref();
                playlist_window::draw(&mut dst, skin, font, &v, widgets, scale);
            }
            _ => {
                if let Some(v) = &lib_view {
                    let font = self.font.as_ref();
                    library_window::draw(&mut dst, skin, font, v, widgets, scale);
                }
            }
        }
        let _ = buffer.present();
        #[cfg(target_os = "macos")]
        crate::platform::macos::tag_frame_srgb(&pane.window);
    }

    /// How soon the windows next need repainting on their own, if at all.
    fn next_frame(&self) -> Option<Duration> {
        use main_window::Playback;
        match self.playback() {
            Playback::Playing => Some(PLAYING_FRAME),
            Playback::Paused => Some(IDLE_FRAME),
            Playback::Stopped if Marquee::scrolls(&self.marquee_text()) => Some(IDLE_FRAME),
            Playback::Stopped => None,
        }
    }

    // -- Windows ------------------------------------------------------------

    fn create_pane(
        &mut self,
        event_loop: &ActiveEventLoop,
        kind: PaneKind,
        position: Option<Position>,
    ) -> Option<()> {
        let (w, h) = self.logical_size(kind);
        let factor = if self.doubled && kind.doubles() {
            2.0
        } else {
            1.0
        };
        // Borderless: every title bar is the skin's. Transparent only for a
        // shaped main window, so the desktop shows through the region mask.
        let shaped = kind == PaneKind::Main
            && self
                .skin
                .as_ref()
                .is_some_and(|s| !s.region.normal.is_empty());
        let title = match kind {
            PaneKind::Main => "Lantern",
            PaneKind::Equalizer => "Lantern Equalizer",
            PaneKind::Playlist => "Lantern Playlist",
            PaneKind::Library => library_window::TITLE,
        };
        let mut attrs = Window::default_attributes()
            .with_title(title)
            .with_inner_size(LogicalSize::new(
                f64::from(w) * factor,
                f64::from(h) * factor,
            ))
            .with_resizable(false)
            .with_decorations(false)
            .with_transparent(shaped)
            // Shown once placed, so a restored window never flashes at the
            // system's default spot.
            .with_visible(false);
        if let Some(p) = position {
            attrs = attrs.with_position(p);
        }
        let window = match event_loop.create_window(attrs) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                eprintln!("could not create window: {e}");
                return None;
            }
        };
        if self.context.is_none() {
            match softbuffer::Context::new(Arc::clone(&window)) {
                Ok(c) => self.context = Some(c),
                Err(e) => {
                    eprintln!("could not create graphics context: {e}");
                    return None;
                }
            }
        }
        let surface = match softbuffer::Surface::new(self.context.as_ref()?, Arc::clone(&window)) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("could not create drawing surface: {e}");
                return None;
            }
        };
        let widgets = match kind {
            PaneKind::Main => layout::main_window(),
            PaneKind::Equalizer => eq_window::widgets(),
            PaneKind::Playlist => playlist_window::widgets(w, h),
            PaneKind::Library => library_window::widgets(w, h, self.lib_ui.tree_width),
        };
        // Colour-managed without a per-frame conversion; see the notes.
        #[cfg(target_os = "macos")]
        crate::platform::macos::use_srgb(&window);
        self.panes.push(Pane {
            kind,
            window,
            surface,
            widgets,
            input: Input::default(),
            focused: kind == PaneKind::Main,
            visible: false,
            shaded: false,
        });
        Some(())
    }

    /// Where a window goes with no saved place: stacked under the main
    /// window — the playlist under the EQ, too — as Winamp starts. The
    /// library goes beside the main window, on its right.
    fn default_place(&self, kind: PaneKind) -> Option<LogicalPosition<f64>> {
        let main = self.rect(PaneKind::Main)?;
        if kind == PaneKind::Library {
            return Some(LogicalPosition::new(main.x + main.w, main.y));
        }
        let mut y = main.y + main.h;
        if kind == PaneKind::Playlist
            && let Some(eq) = self.rect(PaneKind::Equalizer)
        {
            y = eq.y + eq.h;
        }
        Some(LogicalPosition::new(main.x, y))
    }

    pub(crate) fn set_visible(&mut self, kind: PaneKind, visible: bool) {
        if let Some(p) = self.pane_mut(kind) {
            p.visible = visible;
            p.window.set_visible(visible);
        }
        self.sync_toggles();
    }

    /// The main window's EQ and PL buttons light when their windows show.
    fn sync_toggles(&mut self) {
        let eq = self.pane(PaneKind::Equalizer).is_some_and(|p| p.visible);
        let pl = self.pane(PaneKind::Playlist).is_some_and(|p| p.visible);
        if let Some(w) = self.widget_mut(PaneKind::Main, layout::EQ_TOGGLE) {
            w.active = eq;
        }
        if let Some(w) = self.widget_mut(PaneKind::Main, layout::PL_TOGGLE) {
            w.active = pl;
        }
        self.request_redraw(PaneKind::Main);
    }

    /// Re-clip the main window to the skin's region at the current size.
    /// An empty region clears the mask, for a switch to a rectangular skin.
    fn apply_region(&self) {
        let Some(pane) = self.pane(PaneKind::Main) else {
            return;
        };
        // Shade mode has its own region section.
        let region = self
            .skin
            .as_ref()
            .map(|s| {
                if pane.shaded {
                    s.region.windowshade.as_slice()
                } else {
                    s.region.normal.as_slice()
                }
            })
            .unwrap_or_default();
        // A clear window background is what lets the desktop show through
        // where the mask cuts; opaque is cheaper when nothing is cut.
        pane.window.set_transparent(!region.is_empty());
        #[cfg(target_os = "macos")]
        {
            let factor = if self.doubled { 2.0 } else { 1.0 };
            if !crate::platform::macos::set_window_region(&pane.window, region, factor) {
                eprintln!("could not shape window; drawing it rectangular");
            }
        }
    }

    // -- Input --------------------------------------------------------------

    fn to_logical(&self, kind: PaneKind, x: f64, y: f64) -> (i32, i32) {
        let scale = f64::from(self.scale(kind));
        ((x / scale) as i32, (y / scale) as i32)
    }

    fn press(&mut self, kind: PaneKind) {
        let Some(pane) = self.pane(kind) else { return };
        let Some((x, y)) = pane.input.cursor else {
            return;
        };
        if kind == PaneKind::Playlist {
            if self.playlist_menu_press(x, y) {
                return;
            }
        } else {
            self.close_playlist_menu();
        }
        let Some(pane) = self.pane(kind) else { return };
        let Some(id) = hit_test(&pane.widgets, x, y) else {
            return;
        };
        let is_title = matches!(
            id,
            layout::TITLE_DRAG
                | eq_window::TITLE_DRAG
                | playlist_window::TITLE_DRAG
                | library_window::TITLE_DRAG
        );
        if is_title {
            // A double-click on a title bar rolls the window up or down —
            // any but the library's, which has no windowshade mode.
            let now = Instant::now();
            let double = kind != PaneKind::Library
                && self
                    .title_click
                    .is_some_and(|(k, t)| k == kind && now.duration_since(t) < DOUBLE_CLICK);
            if double {
                self.title_click = None;
                self.toggle_shade(kind);
            } else {
                self.title_click = Some((kind, now));
                self.start_drag(kind);
            }
            return;
        }
        if let Some(p) = self.pane_mut(kind) {
            // Latch the widget so the drag belongs to it until release, even
            // if the pointer wanders off.
            p.input.active = Some(id);
        }
        let slider = self.widget_mut(kind, id).is_some_and(|w| {
            w.pressed = true;
            w.kind == WidgetKind::Slider
        });
        // A click on a slider's track jumps the thumb there.
        if slider {
            self.drag_slider(kind, id, x, y);
        }
        match kind {
            PaneKind::Playlist => self.press_playlist(id, y),
            PaneKind::Library => self.press_library(id, x, y),
            _ => {}
        }
        self.request_redraw(kind);
    }

    fn release(&mut self, kind: PaneKind) {
        if self.drag.take().is_some() {
            return;
        }
        let Some(pane) = self.pane(kind) else { return };
        if kind == PaneKind::Playlist
            && let Some((x, y)) = pane.input.cursor
            && self.playlist_menu_release(x, y)
        {
            return;
        }
        let Some(pane) = self.pane(kind) else { return };
        let Some(id) = pane.input.active else { return };
        let over = pane
            .input
            .cursor
            .and_then(|(x, y)| hit_test(&pane.widgets, x, y));
        let slider = self.widget_mut(kind, id).is_some_and(|w| {
            w.pressed = false;
            w.kind == WidgetKind::Slider
        });
        if let Some(p) = self.pane_mut(kind) {
            p.input.active = None;
        }
        if slider {
            self.release_slider(kind, id);
        } else if over == Some(id) {
            // Only fire if released over the widget pressed, which is how a
            // click is meant to be cancellable.
            self.activate(kind, id);
        }
        self.request_redraw(kind);
    }

    fn drag_slider(&mut self, kind: PaneKind, id: WidgetId, x: i32, y: i32) {
        match kind {
            PaneKind::Main => self.drag_main_slider(id, x),
            PaneKind::Equalizer => self.drag_eq_slider(id, x, y),
            PaneKind::Playlist => self.drag_playlist(id, x, y),
            PaneKind::Library => self.drag_library(id, x, y),
        }
    }

    fn release_slider(&mut self, kind: PaneKind, id: WidgetId) {
        match kind {
            PaneKind::Main => self.release_main_slider(id),
            PaneKind::Playlist => self.release_playlist_slider(id),
            PaneKind::Library => self.release_library_slider(id),
            PaneKind::Equalizer => {}
        }
    }

    fn activate(&mut self, kind: PaneKind, id: WidgetId) {
        match kind {
            PaneKind::Main => self.activate_main(id),
            PaneKind::Equalizer => self.activate_eq(id),
            PaneKind::Playlist => self.activate_playlist(id),
            PaneKind::Library => self.activate_library(id),
        }
    }

    /// Flip a toggle, returning its new state.
    fn toggle(&mut self, kind: PaneKind, id: WidgetId) -> bool {
        self.widget_mut(kind, id).is_some_and(|w| {
            w.active = !w.active;
            w.active
        })
    }

    fn dragging(&self, kind: PaneKind, id: WidgetId) -> bool {
        self.pane(kind).is_some_and(|p| p.input.active == Some(id))
    }
}

impl App {
    /// Toggles whose state lives on widgets, and what they switch.
    fn restore_toggles(&mut self, s: &settings::Settings) {
        for (kind, id, on) in [
            (PaneKind::Main, layout::SHUFFLE, s.shuffle),
            (PaneKind::Main, layout::REPEAT, s.repeat),
            (PaneKind::Main, layout::CLUTTER_A, s.always_on_top),
            (PaneKind::Main, layout::CLUTTER_D, s.doubled),
            (PaneKind::Equalizer, eq_window::ON, s.eq.enabled),
            (PaneKind::Equalizer, eq_window::AUTO, s.eq_auto),
        ] {
            if let Some(w) = self.widget_mut(kind, id) {
                w.active = on;
            }
        }
        if s.always_on_top {
            for p in &self.panes {
                p.window
                    .set_window_level(winit::window::WindowLevel::AlwaysOnTop);
            }
        }
        self.sync_toggles();
    }
}

/// A title-bar drag in progress. Everything is measured from where it
/// began, so snapping never accumulates error.
struct Drag {
    /// The pointer on screen when the drag began.
    cursor: (f64, f64),
    /// The windows moving, and where each began.
    group: Vec<(PaneKind, snap::Rect)>,
    /// Where the showing ones among them began: those whose edges snap.
    snapping: Vec<snap::Rect>,
    /// Showing windows that stay put, whose edges attract.
    fixed: Vec<snap::Rect>,
    screens: Vec<snap::Rect>,
}

impl ApplicationHandler for App {
    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        self.library.stop();
        self.save_settings();
    }

    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if !self.panes.is_empty() {
            return;
        }
        let restored = self.restored.clone();
        let logical = |p: Option<(f64, f64)>| p.map(|(x, y)| LogicalPosition::new(x, y).into());
        if self
            .create_pane(event_loop, PaneKind::Main, logical(restored.main.position))
            .is_none()
        {
            event_loop.exit();
            return;
        }
        self.apply_region();
        self.set_visible(PaneKind::Main, true);
        // Saved places already account for shading.
        self.restoring = true;
        for (kind, place) in [
            (PaneKind::Equalizer, restored.equalizer),
            (PaneKind::Playlist, restored.playlist),
            (PaneKind::Library, restored.library),
        ] {
            let at =
                logical(place.position).or_else(|| self.default_place(kind).map(Position::from));
            self.create_pane(event_loop, kind, at);
            if place.shaded {
                self.toggle_shade(kind);
            }
            self.set_visible(kind, place.visible);
        }
        if restored.main.shaded {
            self.toggle_shade(PaneKind::Main);
        }
        self.restoring = false;
        self.restore_toggles(&restored);
        if let Some(proxy) = self.proxy.clone() {
            self.library.start(move || {
                let _ = proxy.send_event(());
            });
        }
        // Catch up with whatever changed in the watched folders while
        // Lantern was closed.
        self.library.rescan();
        self.install_menu_bar();
        if let Some(main) = self.pane(PaneKind::Main) {
            main.window.focus_window();
        }

        match Engine::new() {
            Ok(mut engine) => {
                self.analyzer = engine.take_tap().map(Analyzer::new);
                self.engine = Some(engine);
                self.set_volume(self.volume);
                self.send(Command::SetBalance(self.balance));
                self.send(Command::SetEq(self.eq));
                // Winamp plays files it is launched with.
                if self.autoplay {
                    self.play();
                }
                #[cfg(target_os = "macos")]
                if let Some(proxy) = self.proxy.clone() {
                    self.install_media(move || {
                        let _ = proxy.send_event(());
                    });
                }
            }
            // A missing output device should not stop the UI from rendering.
            Err(e) => eprintln!("audio unavailable: {e}"),
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.drain_menu_events();
        if self.quit_requested {
            event_loop.exit();
            return;
        }
        let now = Instant::now();
        self.tick(now);
        self.poll_library();
        self.service_media();
        self.autosave(now);
        let Some(interval) = self.next_frame() else {
            self.next_frame_at = None;
            event_loop.set_control_flow(ControlFlow::Wait);
            return;
        };
        // Only a due deadline paints. Requesting a redraw unconditionally
        // here would spin: the redraw wakes the loop, which lands back here.
        if self.next_frame_at.is_none_or(|at| now >= at) {
            self.request_redraw(PaneKind::Main);
            // The playlist's only moving part is its mini time.
            let second = self.engine.as_ref().map(|e| e.position_secs() as u64);
            if second != self.pl_drawn_second {
                self.pl_drawn_second = second;
                self.request_redraw(PaneKind::Playlist);
            }
            self.next_frame_at = Some(now + interval);
        }
        if let Some(at) = self.next_frame_at {
            event_loop.set_control_flow(ControlFlow::WaitUntil(at));
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let Some(kind) = self.pane_of(id) else { return };
        match event {
            WindowEvent::CloseRequested => match kind {
                PaneKind::Main => event_loop.exit(),
                other => self.set_visible(other, false),
            },

            WindowEvent::RedrawRequested => self.render(kind),

            WindowEvent::Resized(_) => {
                if kind == PaneKind::Main {
                    // Double size changes the window, and the region with it.
                    self.apply_region();
                }
                self.request_redraw(kind);
            }

            WindowEvent::Focused(focused) => {
                if !focused && kind == PaneKind::Playlist {
                    self.close_playlist_menu();
                }
                if let Some(p) = self.pane_mut(kind) {
                    p.focused = focused;
                }
                self.request_redraw(kind);
            }

            WindowEvent::ModifiersChanged(m) => self.modifiers = m.state(),

            WindowEvent::KeyboardInput { event, .. } => {
                // Winamp's Alt+L: by its physical key, as Option turns L
                // into another character, and AppKit will not reliably
                // match an Option-only menu shortcut.
                let alt_l = self.modifiers.alt_key()
                    && event.physical_key == PhysicalKey::Code(KeyCode::KeyL);
                if event.state == ElementState::Pressed && alt_l {
                    if !event.repeat {
                        self.toggle_library();
                    }
                } else if event.state == ElementState::Pressed
                    && kind == PaneKind::Library
                    && self.library_key(&event)
                {
                } else if event.state == ElementState::Pressed {
                    let handled =
                        kind == PaneKind::Playlist && self.playlist_key(&event.logical_key);
                    if !handled {
                        self.key(&event.logical_key);
                    }
                }
            }

            WindowEvent::MouseWheel { delta, .. } => match kind {
                PaneKind::Playlist => self.wheel_playlist(delta),
                PaneKind::Library => self.wheel_library(delta),
                _ => {}
            },

            // Dropping files on any window adds them, as in Winamp.
            WindowEvent::DroppedFile(path) => self.add_paths([path]),

            WindowEvent::CursorMoved { position, .. } => {
                if self.drag.is_some() {
                    self.continue_drag(kind);
                    return;
                }
                let (x, y) = self.to_logical(kind, position.x, position.y);
                if kind == PaneKind::Playlist {
                    self.playlist_menu_hover(x, y);
                }
                let Some(pane) = self.pane_mut(kind) else {
                    return;
                };
                pane.input.cursor = Some((x, y));
                if let Some(id) = pane.input.active
                    && pane
                        .widgets
                        .iter()
                        .any(|w| w.id == id && w.kind == WidgetKind::Slider)
                {
                    self.drag_slider(kind, id, x, y);
                    self.request_redraw(kind);
                }
            }

            WindowEvent::CursorLeft { .. } => {
                if let Some(p) = self.pane_mut(kind) {
                    p.input.cursor = None;
                }
            }

            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => match state {
                ElementState::Pressed => self.press(kind),
                ElementState::Released => self.release(kind),
            },

            // Right-click: the playlist's and the library's own menus over
            // them, the main menu anywhere else — as in Winamp.
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Right,
                ..
            } => {
                let menu = match kind {
                    PaneKind::Playlist => menus::MenuKind::Playlist,
                    PaneKind::Library => self.library_menu_here(),
                    _ => menus::MenuKind::Main,
                };
                self.show_menu(menu, kind);
            }

            _ => {}
        }
    }
}
