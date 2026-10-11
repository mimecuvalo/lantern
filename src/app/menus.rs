//! Native menus: the main (options) menu, EQ presets, playlist sort and
//! misc options, a playlist context menu, and the macOS menu bar.
//!
//! Winamp used native Windows menus here, so native is the faithful choice.
//! Menus are built fresh each time they open, so check marks always match
//! the current state. On macOS the popup is modal: the chosen item is read
//! from `muda`'s event queue as soon as it closes. Menu-bar choices arrive
//! on the same queue and are drained every loop turn.

use lantern_audio::engine::Command;
use lantern_audio::eqf;
use muda::accelerator::Accelerator;
use muda::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};

use super::playlist::SortBy;
use super::{App, PaneKind};
use crate::ui::layout;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MenuKind {
    Main,
    Presets,
    Sort,
    Misc,
    Playlist,
}

/// The Winamp Skin Museum: tens of thousands of classic skins, each a
/// `.wsz` download that Load Skin… opens.
const SKIN_MUSEUM: &str = "https://skins.webamp.org/";

/// Open `url` in the user's browser.
fn open_url(url: &str) {
    let mut command = if cfg!(target_os = "macos") {
        std::process::Command::new("open")
    } else if cfg!(windows) {
        let mut c = std::process::Command::new("cmd");
        c.args(["/C", "start", ""]);
        c
    } else {
        std::process::Command::new("xdg-open")
    };
    if let Err(e) = command.arg(url).spawn() {
        eprintln!("could not open {url}: {e}");
    }
}

/// Menu ids for the stock EQ presets: this, then the preset's index.
const BUILTIN_PRESET: &str = "eq_builtin:";

fn item(id: &str, text: &str) -> MenuItem {
    MenuItem::with_id(id, text, true, None)
}

fn keyed(id: &str, text: &str, accelerator: &str) -> MenuItem {
    MenuItem::with_id(id, text, true, accelerator.parse::<Accelerator>().ok())
}

fn check(id: &str, text: &str, on: bool) -> CheckMenuItem {
    CheckMenuItem::with_id(id, text, true, on, None)
}

fn sep() -> PredefinedMenuItem {
    PredefinedMenuItem::separator()
}

impl App {
    fn widget_on(&self, id: crate::ui::WidgetId) -> bool {
        self.pane(PaneKind::Main)
            .and_then(|p| p.widgets.iter().find(|w| w.id == id))
            .is_some_and(|w| w.active)
    }

    fn build_menu(&self, kind: MenuKind) -> muda::Result<Menu> {
        let menu = Menu::new();
        match kind {
            MenuKind::Main => {
                menu.append_items(&[
                    &item("play_files", "Play File…"),
                    &item("play_folder", "Play Folder…"),
                    &sep(),
                    &item("load_skin", "Load Skin…"),
                    &item("default_skin", "Default Skin"),
                    &item("get_skins", "Get More Skins…"),
                    &sep(),
                ])?;
                let playback = Submenu::with_items(
                    "Playback",
                    true,
                    &[
                        &item("prev", "Previous"),
                        &item("play", "Play"),
                        &item("pause", "Pause"),
                        &item("stop", "Stop"),
                        &item("next", "Next"),
                        &sep(),
                        &check("shuffle", "Shuffle", self.widget_on(layout::SHUFFLE)),
                        &check("repeat", "Repeat", self.widget_on(layout::REPEAT)),
                    ],
                )?;
                let options = Submenu::with_items(
                    "Options",
                    true,
                    &[
                        &check("aot", "Always On Top", self.widget_on(layout::CLUTTER_A)),
                        &check("double", "Double Size", self.doubled),
                        &check("remaining", "Time Remaining", self.show_remaining),
                        &check("vis", "Spectrum Analyser", self.analyser_on),
                    ],
                )?;
                let windows = Submenu::with_items(
                    "Windows",
                    true,
                    &[
                        &check("show_eq", "Equalizer", self.visible(PaneKind::Equalizer)),
                        &check("show_pl", "Playlist", self.visible(PaneKind::Playlist)),
                    ],
                )?;
                menu.append_items(&[&playback, &options, &windows])?;
                menu.append_items(&[
                    &sep(),
                    &item("about", "About Lantern…"),
                    &item("exit", "Exit"),
                ])?;
            }
            MenuKind::Presets => {
                let load = Submenu::new("Load", true);
                for (i, preset) in eqf::builtin().iter().enumerate() {
                    load.append(&item(&format!("{BUILTIN_PRESET}{i}"), &preset.name))?;
                }
                load.append_items(&[&sep(), &item("eq_load", "From File…")])?;
                menu.append_items(&[
                    &load,
                    &item("eq_save", "Save to File…"),
                    &sep(),
                    &item("eq_reset", "Reset"),
                ])?;
            }
            MenuKind::Sort => {
                menu.append_items(&[
                    &item("sort_title", "Sort by Title"),
                    &item("sort_file", "Sort by File Name"),
                    &item("sort_path", "Sort by Path and File Name"),
                    &sep(),
                    &item("reverse", "Reverse List"),
                    &item("randomize", "Randomize List"),
                ])?;
            }
            MenuKind::Misc => {
                menu.append_items(&[
                    &item("remove_dead", "Remove Dead Files"),
                    &item("save_list", "Save Playlist…"),
                ])?;
            }
            MenuKind::Playlist => {
                menu.append_items(&[
                    &item("play_selected", "Play Item"),
                    &item("remove_selected", "Remove Item(s)"),
                    &item("file_info", "File Info…"),
                    &sep(),
                    &item("add_files", "Add Files…"),
                    &item("add_folder", "Add Folder…"),
                    &sep(),
                    &item("load_list", "Load Playlist…"),
                    &item("save_list", "Save Playlist…"),
                ])?;
            }
        }
        Ok(menu)
    }

    fn visible(&self, kind: PaneKind) -> bool {
        self.pane(kind).is_some_and(|p| p.visible)
    }

    /// Pop `kind` up over `over`'s window at the pointer, then act on the
    /// choice.
    pub(crate) fn show_menu(&mut self, kind: MenuKind, over: PaneKind) {
        let menu = match self.build_menu(kind) {
            Ok(m) => m,
            Err(e) => {
                eprintln!("could not build menu: {e}");
                return;
            }
        };
        #[cfg(target_os = "macos")]
        if let Some(pane) = self.pane(over) {
            crate::platform::macos::popup_menu(&pane.window, &menu);
        }
        #[cfg(not(target_os = "macos"))]
        let _ = over;
        self.drain_menu_events();
    }

    /// Act on every menu choice waiting — from a popup or the menu bar.
    pub(crate) fn drain_menu_events(&mut self) {
        while let Ok(event) = MenuEvent::receiver().try_recv() {
            let id = event.id().0.clone();
            self.menu_action(&id);
        }
    }

    fn menu_action(&mut self, id: &str) {
        if let Some(i) = id.strip_prefix(BUILTIN_PRESET) {
            let preset = i
                .parse()
                .ok()
                .and_then(|i: usize| eqf::builtin().get(i).cloned());
            if let Some(preset) = preset {
                preset.apply_to(&mut self.eq);
                self.send(Command::SetEq(self.eq));
                self.request_redraw(PaneKind::Equalizer);
            }
            return;
        }
        match id {
            "play_files" => self.eject(),
            "play_folder" => {
                if let Some(dir) = rfd::FileDialog::new()
                    .set_title("Play folder")
                    .pick_folder()
                {
                    self.playlist.clear();
                    self.pl_scroll = 0;
                    self.add_paths([dir]);
                    self.play_entry(0);
                }
            }
            "load_skin" => self.load_skin_dialog(),
            "default_skin" => self.use_default_skin(),
            "get_skins" => open_url(SKIN_MUSEUM),
            "prev" => self.skip(false),
            "next" => self.skip(true),
            "play" => self.play(),
            "pause" => self.toggle_pause(),
            "stop" => self.send(Command::Stop),
            "shuffle" => self.activate_main(layout::SHUFFLE),
            "repeat" => self.activate_main(layout::REPEAT),
            "aot" => self.activate_main(layout::CLUTTER_A),
            "double" => self.activate_main(layout::CLUTTER_D),
            "remaining" => self.show_remaining = !self.show_remaining,
            "vis" => self.analyser_on = !self.analyser_on,
            "show_eq" => self.activate_main(layout::EQ_TOGGLE),
            "show_pl" => self.activate_main(layout::PL_TOGGLE),
            "about" => {
                rfd::MessageDialog::new()
                    .set_title("About Lantern")
                    .set_description(format!(
                        "Lantern {}\n\nA player for Winamp 2 classic skins.\n\n\
                         Skin geometry and behaviour from Webamp by Jordan \
                         Eldredge (MIT). Not affiliated with Winamp or Llama Group.",
                        env!("CARGO_PKG_VERSION")
                    ))
                    .show();
            }
            "exit" => self.quit_requested = true,
            "eq_load" => self.load_eq_preset(),
            "eq_save" => self.save_eq_preset(),
            "eq_reset" => {
                self.eq.preamp_db = 0.0;
                self.eq.band_gains_db = [0.0; 10];
                self.send(Command::SetEq(self.eq));
            }
            "sort_title" => self.sort_playlist(SortBy::Title),
            "sort_file" => self.sort_playlist(SortBy::FileName),
            "sort_path" => self.sort_playlist(SortBy::Path),
            "reverse" => self.sort_playlist(SortBy::Reverse),
            "randomize" => self.sort_playlist(SortBy::Randomize),
            "remove_dead" => {
                self.playlist.remove_missing(|p| p.exists());
                self.requeue();
            }
            "play_selected" => {
                let first = (0..self.playlist.len())
                    .find(|&i| self.playlist.get(i).is_some_and(|e| e.selected));
                if let Some(i) = first {
                    self.play_entry(i);
                }
            }
            "remove_selected" => {
                self.playlist.remove_selected();
                self.requeue();
            }
            "file_info" => self.show_file_info(),
            "add_files" => self.add_files_dialog(),
            "add_folder" => self.add_folder_dialog(),
            "load_list" => self.load_playlist_dialog(),
            "save_list" => self.save_playlist_dialog(),
            _ => {}
        }
        for kind in [PaneKind::Main, PaneKind::Equalizer, PaneKind::Playlist] {
            self.request_redraw(kind);
        }
    }

    fn load_eq_preset(&mut self) {
        let Some(file) = rfd::FileDialog::new()
            .set_title("Load EQ preset")
            .add_filter("Winamp EQ presets", &["eqf", "q1"])
            .pick_file()
        else {
            return;
        };
        let presets = match std::fs::read(&file)
            .map_err(|e| e.to_string())
            .and_then(|b| eqf::parse(&b).map_err(|e| e.to_string()))
        {
            Ok(p) => p,
            Err(e) => {
                eprintln!("cannot load {}: {e}", file.display());
                return;
            }
        };
        // A library holds many; a single-preset file, one. Without a picker
        // yet, a library loads its first.
        if let Some(preset) = presets.first() {
            preset.apply_to(&mut self.eq);
            self.send(Command::SetEq(self.eq));
        }
    }

    fn save_eq_preset(&mut self) {
        let Some(mut file) = rfd::FileDialog::new()
            .set_title("Save EQ preset")
            .add_filter("Winamp EQ preset", &["eqf"])
            .set_file_name("preset.eqf")
            .save_file()
        else {
            return;
        };
        if file.extension().is_none() {
            file.set_extension("eqf");
        }
        let name = file
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let preset = eqf::Preset {
            name,
            preamp_db: self.eq.preamp_db,
            band_gains_db: self.eq.band_gains_db,
        };
        if let Err(e) = std::fs::write(&file, eqf::write(&[preset])) {
            eprintln!("could not save {}: {e}", file.display());
        }
    }

    /// The macOS menu bar: an app menu with About and Quit, File, and Window
    /// with Always on Top — findable there, and on Winamp's Ctrl+A. Also
    /// replaces winit's default, whose Quit would skip saving settings.
    pub(crate) fn install_menu_bar(&mut self) {
        let build = || -> muda::Result<(Menu, CheckMenuItem)> {
            let bar = Menu::new();
            let app = Submenu::with_items(
                "Lantern",
                true,
                &[
                    &item("about", "About Lantern"),
                    &sep(),
                    &keyed("exit", "Quit Lantern", "CmdOrCtrl+Q"),
                ],
            )?;
            let file = Submenu::with_items(
                "File",
                true,
                &[
                    &keyed("play_files", "Play File…", "CmdOrCtrl+O"),
                    &item("play_folder", "Play Folder…"),
                    &item("add_files", "Add Files…"),
                    &item("add_folder", "Add Folder…"),
                    &sep(),
                    &item("load_list", "Load Playlist…"),
                    &item("save_list", "Save Playlist…"),
                    &sep(),
                    &item("load_skin", "Load Skin…"),
                    &item("default_skin", "Default Skin"),
                    &item("get_skins", "Get More Skins…"),
                ],
            )?;
            let aot = CheckMenuItem::with_id(
                "aot",
                "Always on Top",
                true,
                self.widget_on(layout::CLUTTER_A),
                "Ctrl+A".parse::<Accelerator>().ok(),
            );
            let window = Submenu::with_items("Window", true, &[&aot])?;
            bar.append_items(&[&app, &file, &window])?;
            Ok((bar, aot))
        };
        match build() {
            Ok((bar, aot)) => {
                self.aot_item = Some(aot);
                #[cfg(target_os = "macos")]
                bar.init_for_nsapp();
                self.menu_bar = Some(bar);
            }
            Err(e) => eprintln!("could not build the menu bar: {e}"),
        }
    }
}
