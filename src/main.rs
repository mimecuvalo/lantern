//! Lantern — a Winamp 2.x-compatible music player.
//!
//! See `docs/design/2026-09-21-rust-architecture.md` before changing the shape
//! of any of this.
//!
//! Entry point: arguments, the skin, the event loop. The application itself
//! is in `app/`; drawing and layout in `ui/`.

// Scaffolding for later milestones: window sizes and the tiling blitter are
// defined ahead of the code that uses them, because they are format facts
// rather than guesses. Narrowed as each milestone lands.
#![allow(dead_code)]

mod app;
mod platform;
mod ui;

use std::path::PathBuf;

use anyhow::{Context as _, Result};
use lantern_skin::{SheetId, Skin};
use winit::event_loop::{ControlFlow, EventLoop};

use app::App;

fn load_skin(path: &std::path::Path) -> Result<Skin> {
    let bytes =
        std::fs::read(path).with_context(|| format!("could not read skin {}", path.display()))?;
    let skin = lantern_skin::parse(&bytes)
        .with_context(|| format!("could not parse skin {}", path.display()))?;
    Ok(skin)
}

fn main() -> Result<()> {
    let mut tracks: Vec<PathBuf> = Vec::new();
    let mut skin_arg: Option<PathBuf> = None;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--skin" => {
                skin_arg = Some(
                    args.next()
                        .map(PathBuf::from)
                        .context("--skin needs a path")?,
                );
            }
            "-h" | "--help" => {
                println!("usage: lantern [audio-file ...] [--skin <file.wsz>]");
                return Ok(());
            }
            other => tracks.push(PathBuf::from(other)),
        }
    }

    let (settings, saved_playlist) = app::settings::load();
    // An explicit --skin wins, then the last skin used, then the built-in.
    let (skin, skin_path) = match skin_arg.or_else(|| settings.skin.clone()) {
        Some(path) => match load_skin(&path) {
            Ok(skin) => {
                println!(
                    "loaded skin {} ({} of {} sheets)",
                    path.display(),
                    SheetId::ALL
                        .iter()
                        .filter(|id| skin.sheet(**id).is_some() && !skin.supplied(**id))
                        .count(),
                    SheetId::COUNT
                );
                (Some(skin), Some(std::path::absolute(&path).unwrap_or(path)))
            }
            Err(e) => {
                eprintln!("{e:#}\nusing the built-in skin");
                (app::default_skin(), None)
            }
        },
        None => (app::default_skin(), None),
    };

    println!("keys: x play, c pause, v stop, arrows seek/volume, cmd-q quits");

    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut app = App::new(skin, skin_path, tracks, settings, saved_playlist);
    app.set_proxy(event_loop.create_proxy());
    event_loop.run_app(&mut app)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lantern_audio::spectrum::{BAR_COUNT, Bar};
    use std::time::Instant;
    use ui::Widget;
    use ui::blit::Surface;
    use ui::geometry::{MAIN_HEIGHT, WINDOW_WIDTH};
    use ui::layout;
    use ui::main_window::{self, MainView, Playback};
    use ui::marquee::{self, Marquee};

    /// A mid-song view: every display populated, so a render exercises all
    /// of them.
    fn playing_view() -> MainView {
        let bars = std::array::from_fn::<Bar, BAR_COUNT, _>(|i| Bar {
            height: 16 - (i as u8 % 12),
            peak: Some(16),
        });
        MainView {
            focused: true,
            playback: Playback::Playing,
            elapsed_secs: 83.0,
            duration_secs: Some(225.0),
            show_remaining: false,
            time_visible: true,
            kbps: Some(320),
            sample_rate: Some(44_100),
            channels: Some(2),
            marquee: Marquee::new(Instant::now()).layout(&marquee::track_text(
                1,
                "Artist - Title",
                Some(225.0),
            )),
            volume: 0.7,
            balance: -0.3,
            position: Some(83.0 / 225.0),
            bars: Some(bars),
        }
    }

    /// Render the main window to a buffer, no window and no display involved.
    ///
    /// This is the only automated check that the blitter, the sprite
    /// coordinates and the skin parser agree with each other. It writes PNGs
    /// per skin when `LANTERN_RENDER_DUMP` is set, so the result can be inspected
    /// by eye. The committed fixture always runs; the gitignored reference
    /// skin runs when present (see `docs/legal.md`).
    #[test]
    fn main_window_renders_to_a_buffer() {
        assert!(app::default_skin().is_some(), "the built-in skin parses");
        for (path, name) in skin_paths() {
            render_check(&path, &name);
        }
    }

    fn render(skin: &Skin, view: &MainView, widgets: &[Widget]) -> Vec<u32> {
        let (w, h) = (WINDOW_WIDTH, MAIN_HEIGHT);
        let mut pixels = vec![0u32; (w * h) as usize];
        let mut dst = Surface::new(&mut pixels, w, h);
        main_window::draw(&mut dst, Some(skin), view, widgets, 1);
        pixels
    }

    /// Skins to render: the fixture, the built-in and the other Lantern
    /// themes always; the gitignored reference skin when present, so it can
    /// be compared by eye.
    pub(crate) fn skin_paths() -> Vec<(String, String)> {
        let mut v = vec![
            (
                "tests/fixtures/skins/lantern-fixture.wsz".to_string(),
                "fixture".to_string(),
            ),
            ("assets/default-skin.wsz".to_string(), "default".to_string()),
        ];
        let mut themes: Vec<_> = std::fs::read_dir("assets/skins")
            .expect("assets/skins holds Lantern's other themes")
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "wsz"))
            .collect();
        themes.sort();
        let reference = std::path::PathBuf::from("skins/base-2.91.wsz");
        if reference.exists() {
            themes.push(reference);
        } else {
            eprintln!("SKIPPED: {} not present", reference.display());
        }
        for p in themes {
            let name = p
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            v.push((p.to_string_lossy().into_owned(), name));
        }
        v
    }

    pub(crate) fn skins() -> Vec<(String, Skin)> {
        skin_paths()
            .into_iter()
            .map(|(path, name)| (name, load_skin(std::path::Path::new(&path)).unwrap()))
            .collect()
    }

    fn render_check(path: &str, name: &str) {
        let skin = load_skin(std::path::Path::new(path)).expect("skin must load");
        let widgets = layout::main_window();
        let (w, h) = (WINDOW_WIDTH, MAIN_HEIGHT);

        let stopped = render(&skin, &MainView::default(), &widgets);
        // The background alone should fill every pixel of a classic skin, so
        // nothing may be left at the no-skin colour.
        let untouched = stopped
            .iter()
            .filter(|p| **p == main_window::NO_SKIN_BACKGROUND)
            .count();
        assert!(
            untouched < (w * h) as usize / 100,
            "{name}: {untouched} pixels never painted — background did not cover the window"
        );

        // Playing must light up the displays: time, analyser, info, thumb.
        let playing = render(&skin, &playing_view(), &widgets);
        let differing = stopped.iter().zip(&playing).filter(|(a, b)| a != b).count();
        assert!(
            differing > 500,
            "{name}: only {differing} pixels changed between stopped and playing"
        );

        if let Ok(dir) = std::env::var("LANTERN_RENDER_DUMP") {
            for (state, pixels) in [("stopped", &stopped), ("playing", &playing)] {
                let mut rgba = Vec::with_capacity(pixels.len() * 4);
                for p in pixels.iter() {
                    rgba.extend_from_slice(&[(p >> 16) as u8, (p >> 8) as u8, *p as u8, 255]);
                }
                let out = std::path::Path::new(&dir).join(format!("main-{name}-{state}.png"));
                match image::save_buffer(&out, &rgba, w, h, image::ColorType::Rgba8) {
                    Ok(()) => eprintln!("wrote {}", out.display()),
                    Err(e) => eprintln!("could not write {}: {e}", out.display()),
                }
            }
        }
    }
}

#[cfg(test)]
mod window_render_tests {
    use super::*;
    use ui::blit::Surface;
    use ui::eq_window::{self, EqView};
    use ui::playlist_window::{self, PlaylistView};

    fn dump(name: &str, pixels: &[u32], w: u32, h: u32) {
        let Ok(dir) = std::env::var("LANTERN_RENDER_DUMP") else {
            return;
        };
        let rgba: Vec<u8> = pixels
            .iter()
            .flat_map(|p| [(p >> 16) as u8, (p >> 8) as u8, *p as u8, 255])
            .collect();
        let out = std::path::Path::new(&dir).join(format!("{name}.png"));
        if image::save_buffer(&out, &rgba, w, h, image::ColorType::Rgba8).is_ok() {
            eprintln!("wrote {}", out.display());
        }
    }

    #[test]
    fn equalizer_renders() {
        for (name, skin) in crate::tests::skins() {
            let (w, h) = (275, 116);
            let mut px = vec![0u32; (w * h) as usize];
            let mut widgets = eq_window::widgets();
            widgets
                .iter_mut()
                .for_each(|w| w.active = w.id == eq_window::ON);
            let mut bands = [0.5; 10];
            bands[0] = 0.9;
            bands[4] = 0.2;
            bands[9] = 0.75;
            let view = EqView {
                focused: true,
                enabled: true,
                auto: false,
                preamp: 0.6,
                bands,
            };
            eq_window::draw(
                &mut Surface::new(&mut px, w, h),
                Some(&skin),
                &view,
                &widgets,
                1,
            );
            let untouched = px
                .iter()
                .filter(|p| **p == ui::main_window::NO_SKIN_BACKGROUND)
                .count();
            assert!(untouched < 300, "{name}: {untouched} unpainted");
            dump(&format!("eq-{name}"), &px, w, h);
        }
    }

    #[test]
    fn playlist_frame_renders_at_any_size() {
        for (name, skin) in crate::tests::skins() {
            for (w, h) in [(275, 232), (350, 290), (275, 116)] {
                let mut px = vec![0u32; (w * h) as usize];
                let rows = (1..=20)
                    .map(|n| playlist_window::Row {
                        number: n,
                        title: format!("Artist {n} - A Song Whose Title Runs Rather Long"),
                        duration: "3:45".into(),
                        selected: n == 3,
                        current: n == 2,
                    })
                    .collect();
                let view = PlaylistView {
                    focused: true,
                    width: w,
                    height: h,
                    rows,
                    scroll: 0.25,
                    running_time: "3:45/1:15:00".into(),
                    mini_time: "1:23".into(),
                    menu: Some(playlist_window::OpenMenu {
                        menu: lantern_skin::PlaylistMenu::Remove,
                        hover: Some(2),
                    }),
                };
                let widgets = playlist_window::widgets(w, h);
                let font = ui::font::TextFont::load(&skin.playlist_style.font);
                playlist_window::draw(
                    &mut Surface::new(&mut px, w, h),
                    Some(&skin),
                    font.as_ref(),
                    &view,
                    &widgets,
                    1,
                );
                // The frame's four edges must be painted all the way round.
                let at = |x: u32, y: u32| px[(y * w + x) as usize];
                let bg = ui::main_window::NO_SKIN_BACKGROUND;
                for x in 0..w {
                    assert_ne!(at(x, 0), bg, "{name} {w}x{h}: top edge gap at x={x}");
                    assert_ne!(at(x, h - 1), bg, "{name} {w}x{h}: bottom edge gap at x={x}");
                }
                for y in 0..h {
                    assert_ne!(at(0, y), bg, "{name} {w}x{h}: left edge gap at y={y}");
                    assert_ne!(at(w - 1, y), bg, "{name} {w}x{h}: right edge gap at y={y}");
                }
                dump(&format!("playlist-{name}-{w}x{h}"), &px, w, h);
            }
        }
    }

    #[test]
    fn library_window_renders_at_any_size() {
        use lantern_library::view::{Column, Sort};
        use ui::gen_controls::ScrollState;
        use ui::library_window::{self, LibraryView, ListRow, TreeRow};
        let tree_row = |label: &str, depth, expander, selected| TreeRow {
            label: label.into(),
            depth,
            expander,
            selected,
        };
        let tree = vec![
            tree_row("Local Media", 0, Some(true), false),
            tree_row("Audio", 1, None, true),
            tree_row("Most Played", 1, None, false),
            tree_row("Recently Added", 1, None, false),
            tree_row("Playlists", 0, Some(false), false),
        ];
        let rows: Vec<ListRow> = (1..=30)
            .map(|n| ListRow {
                cells: vec![
                    "Sleater-Kinney".into(),
                    format!("Song Number {n}"),
                    "The Hot Rock".into(),
                    "3:45".into(),
                    n.to_string(),
                    "Indie".into(),
                    "1999".into(),
                    "****".into(),
                    "12".into(),
                    "2026-01-02".into(),
                ],
                selected: n == 3 || n == 4,
            })
            .collect();
        let scroll = |position, visible| ScrollState {
            position,
            visible,
            pressed: None,
        };
        for (name, skin) in crate::tests::skins() {
            let (min_w, min_h) = (library_window::MIN_WIDTH, library_window::MIN_HEIGHT);
            for (w, h, focused) in [(500, 348, true), (min_w, min_h, false), (650, 464, false)] {
                let mut px = vec![0u32; (w * h) as usize];
                let tree_width = library_window::DEFAULT_TREE_WIDTH;
                let shown = library_window::visible_rows(w, h, tree_width).1;
                let view = LibraryView {
                    focused,
                    width: w,
                    height: h,
                    tree: tree.clone(),
                    tree_scroll: scroll(0.0, 1.0),
                    search: "hot rock".into(),
                    caret: focused.then_some(3),
                    sort: Some(Sort {
                        column: Column::Artist,
                        descending: false,
                    }),
                    rows: rows.iter().take(shown).cloned().collect(),
                    list_scroll: scroll(0.3, 0.4),
                    list_hscroll: scroll(0.0, 0.5),
                    columns_offset: 0,
                    columns: library_window::default_columns(),
                    tree_width,
                    column_drop: None,
                    status: "1,234 items [3:24:11]".into(),
                };
                let font = ui::font::TextFont::load(&skin.playlist_style.font);
                library_window::draw(
                    &mut Surface::new(&mut px, w, h),
                    Some(&skin),
                    font.as_ref(),
                    &view,
                    &library_window::widgets(w, h, tree_width),
                    1,
                );
                // Nothing is left unpainted: frame and contents cover it all.
                let bg = ui::main_window::NO_SKIN_BACKGROUND;
                let gaps = px.iter().filter(|p| **p == bg).count();
                assert_eq!(gaps, 0, "{name} {w}x{h}: {gaps} pixels unpainted");
                dump(&format!("library-{name}-{w}x{h}"), &px, w, h);
            }
        }
    }
}

#[cfg(test)]
mod shade_render_tests {
    use super::*;
    use ui::blit::Surface;
    use ui::main_window::{self, MainView, Playback};

    #[test]
    fn shade_strips_render() {
        let skins = crate::tests::skins();
        for (name, skin) in skins {
            let (w, h) = (275, 14);
            let paint = |draw: &dyn Fn(&mut Surface<'_>)| {
                let mut px = vec![0u32; (w * h) as usize];
                draw(&mut Surface::new(&mut px, w, h));
                let bg = main_window::NO_SKIN_BACKGROUND;
                assert!(
                    px.iter().filter(|p| **p == bg).count() < 50,
                    "{name}: unpainted"
                );
                px
            };
            let view = MainView {
                focused: true,
                playback: Playback::Playing,
                elapsed_secs: 83.0,
                duration_secs: Some(225.0),
                time_visible: true,
                position: Some(0.8),
                bars: Some(std::array::from_fn(|i| lantern_audio::spectrum::Bar {
                    height: (i * 3 % 17) as u8,
                    peak: None,
                })),
                ..MainView::default()
            };
            let main = paint(&|dst| {
                main_window::draw_shade(dst, Some(&skin), &view, &ui::layout::main_shade(), 1)
            });
            let eq_view = ui::eq_window::EqShadeView {
                focused: true,
                volume: 0.7,
                balance: -0.4,
            };
            let eq = paint(&|dst| {
                ui::eq_window::draw_shade(
                    dst,
                    Some(&skin),
                    &eq_view,
                    &ui::eq_window::shade_widgets(),
                    1,
                )
            });
            let pl = paint(&|dst| {
                ui::playlist_window::draw_shade(
                    dst,
                    Some(&skin),
                    275,
                    true,
                    ("3. Sleater-Kinney - Start Together", "2:39"),
                    &ui::playlist_window::shade_widgets(275),
                    1,
                )
            });
            if let Ok(dir) = std::env::var("LANTERN_RENDER_DUMP") {
                // Stack the three strips into one image.
                let all: Vec<u8> = [main, eq, pl]
                    .concat()
                    .iter()
                    .flat_map(|p| [(p >> 16) as u8, (p >> 8) as u8, *p as u8, 255])
                    .collect();
                let out = std::path::Path::new(&dir).join(format!("shade-{name}.png"));
                if image::save_buffer(&out, &all, w, h * 3, image::ColorType::Rgba8).is_ok() {
                    eprintln!("wrote {}", out.display());
                }
            }
        }
    }
}
