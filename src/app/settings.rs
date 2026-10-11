//! What survives a restart: volume, EQ, toggles, skin, window layout, and
//! the playlist.
//!
//! Stored as plain `key = value` lines in the platform's config directory,
//! beside the playlist as `playlist.m3u8`. Hand-rolled rather than serde:
//! a dozen scalars do not need a framework, and a human can fix the file.
//! Reading is forgiving — a missing or malformed key keeps its default —
//! so an old or hand-damaged file never stops the player starting.

use std::path::PathBuf;

use lantern_audio::eq::EqSettings;
use lantern_playlist::Entry;

/// Where a window sits, in logical pixels, and whether it shows.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct WindowPlace {
    /// `None` until a position has been saved: let the system place it.
    pub position: Option<(f64, f64)>,
    pub visible: bool,
    /// Rolled up in windowshade mode.
    pub shaded: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub volume: f32,
    pub balance: f32,
    pub eq: EqSettings,
    pub eq_auto: bool,
    pub shuffle: bool,
    pub repeat: bool,
    pub show_remaining: bool,
    pub analyser_on: bool,
    pub doubled: bool,
    pub always_on_top: bool,
    pub skin: Option<PathBuf>,
    pub main: WindowPlace,
    pub equalizer: WindowPlace,
    pub playlist: WindowPlace,
    pub playlist_size: (u32, u32),
    /// Index of the current playlist entry.
    pub current: Option<usize>,
}

impl Default for Settings {
    fn default() -> Self {
        let shown = WindowPlace {
            position: None,
            visible: true,
            shaded: false,
        };
        Self {
            volume: 0.7,
            balance: 0.0,
            eq: EqSettings::default(),
            eq_auto: false,
            shuffle: false,
            repeat: false,
            show_remaining: false,
            analyser_on: true,
            doubled: false,
            always_on_top: false,
            skin: None,
            main: shown,
            equalizer: shown,
            playlist: shown,
            playlist_size: (275, 232),
            current: None,
        }
    }
}

impl Settings {
    pub fn to_text(&self) -> String {
        let mut out = String::from("# Lantern settings. Safe to edit while Lantern is closed.\n");
        let mut kv = |k: &str, v: String| out.push_str(&format!("{k} = {v}\n"));
        kv("volume", self.volume.to_string());
        kv("balance", self.balance.to_string());
        kv("eq.enabled", self.eq.enabled.to_string());
        kv("eq.auto", self.eq_auto.to_string());
        kv("eq.preamp", self.eq.preamp_db.to_string());
        let bands: Vec<String> = self.eq.band_gains_db.iter().map(f32::to_string).collect();
        kv("eq.bands", bands.join(","));
        kv("shuffle", self.shuffle.to_string());
        kv("repeat", self.repeat.to_string());
        kv("time.remaining", self.show_remaining.to_string());
        kv("analyser", self.analyser_on.to_string());
        kv("double_size", self.doubled.to_string());
        kv("always_on_top", self.always_on_top.to_string());
        if let Some(skin) = &self.skin {
            kv("skin", skin.to_string_lossy().into_owned());
        }
        for (name, place) in [
            ("main", &self.main),
            ("equalizer", &self.equalizer),
            ("playlist", &self.playlist),
        ] {
            if let Some((x, y)) = place.position {
                kv(&format!("{name}.position"), format!("{x},{y}"));
            }
            kv(&format!("{name}.visible"), place.visible.to_string());
            kv(&format!("{name}.shaded"), place.shaded.to_string());
        }
        kv(
            "playlist.size",
            format!("{},{}", self.playlist_size.0, self.playlist_size.1),
        );
        if let Some(i) = self.current {
            kv("playlist.current", i.to_string());
        }
        out
    }

    pub fn from_text(text: &str) -> Self {
        let mut s = Self::default();
        for line in text.lines() {
            let line = line.trim();
            if line.starts_with('#') {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            s.apply(key.trim(), value.trim());
        }
        s
    }

    /// Set one key, ignoring it if the value does not parse.
    fn apply(&mut self, key: &str, v: &str) {
        fn set<T: std::str::FromStr>(slot: &mut T, v: &str) {
            if let Ok(parsed) = v.parse() {
                *slot = parsed;
            }
        }
        fn pair<T: std::str::FromStr>(v: &str) -> Option<(T, T)> {
            let (a, b) = v.split_once(',')?;
            Some((a.trim().parse().ok()?, b.trim().parse().ok()?))
        }
        match key {
            "volume" => set(&mut self.volume, v),
            "balance" => set(&mut self.balance, v),
            "eq.enabled" => set(&mut self.eq.enabled, v),
            "eq.auto" => set(&mut self.eq_auto, v),
            "eq.preamp" => set(&mut self.eq.preamp_db, v),
            "eq.bands" => {
                let parsed: Vec<f32> = v.split(',').filter_map(|b| b.trim().parse().ok()).collect();
                if let Ok(bands) = <[f32; 10]>::try_from(parsed) {
                    self.eq.band_gains_db = bands;
                }
            }
            "shuffle" => set(&mut self.shuffle, v),
            "repeat" => set(&mut self.repeat, v),
            "time.remaining" => set(&mut self.show_remaining, v),
            "analyser" => set(&mut self.analyser_on, v),
            "double_size" => set(&mut self.doubled, v),
            "always_on_top" => set(&mut self.always_on_top, v),
            "skin" => self.skin = Some(PathBuf::from(v)),
            "playlist.size" => {
                if let Some(size) = pair(v) {
                    self.playlist_size = size;
                }
            }
            "playlist.current" => self.current = v.parse().ok(),
            _ => {
                let Some((name, field)) = key.split_once('.') else {
                    return;
                };
                let place = match name {
                    "main" => &mut self.main,
                    "equalizer" => &mut self.equalizer,
                    "playlist" => &mut self.playlist,
                    _ => return,
                };
                match field {
                    "position" => place.position = pair(v).or(place.position),
                    "visible" => set(&mut place.visible, v),
                    "shaded" => set(&mut place.shaded, v),
                    _ => {}
                }
            }
        }
        // Values from a hand-edited file stay within their ranges.
        self.volume = self.volume.clamp(0.0, 1.0);
        self.balance = self.balance.clamp(-1.0, 1.0);
    }
}

/// Where settings live: the platform's per-user config directory, or
/// `LANTERN_CONFIG_DIR` if set — so a development run can leave the real
/// settings alone.
pub fn config_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("LANTERN_CONFIG_DIR") {
        return Some(PathBuf::from(dir));
    }
    app_dir("Lantern")
}

/// The platform's per-user folder for an app called `name`.
fn app_dir(name: &str) -> Option<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    if cfg!(target_os = "macos") {
        return home.map(|h| h.join("Library/Application Support").join(name));
    }
    if cfg!(windows) {
        return std::env::var_os("APPDATA").map(|d| PathBuf::from(d).join(name));
    }
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| home.map(|h| h.join(".config")))
        .map(|d| d.join(name.to_lowercase()))
}

/// The saved settings and playlist, or defaults where there are none.
///
/// Until Lantern has saved anything, it reads what the project saved under
/// its earlier name, Wamp, so settings carry over; saving then writes to
/// Lantern's own folder.
pub fn load() -> (Settings, Vec<Entry>) {
    let Some(mut dir) = config_dir() else {
        return (Settings::default(), Vec::new());
    };
    if std::env::var_os("LANTERN_CONFIG_DIR").is_none()
        && !dir.join("settings.ini").exists()
        && let Some(old) = app_dir("Wamp").filter(|d| d.join("settings.ini").exists())
    {
        dir = old;
    }
    let settings = std::fs::read_to_string(dir.join("settings.ini"))
        .map(|t| Settings::from_text(&t))
        .unwrap_or_default();
    let playlist = std::fs::read(dir.join("playlist.m3u8"))
        .map(|b| lantern_playlist::m3u::parse_m3u(&b, &dir))
        .unwrap_or_default();
    (settings, playlist)
}

/// Write the settings, and the playlist already rendered as M3U8.
pub fn save(settings: &Settings, playlist_m3u8: &str) {
    let Some(dir) = config_dir() else { return };
    let result = std::fs::create_dir_all(&dir)
        .and_then(|()| std::fs::write(dir.join("settings.ini"), settings.to_text()))
        .and_then(|()| std::fs::write(dir.join("playlist.m3u8"), playlist_m3u8));
    if let Err(e) = result {
        eprintln!("could not save settings to {}: {e}", dir.display());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let mut s = Settings {
            volume: 0.42,
            balance: -0.25,
            shuffle: true,
            doubled: true,
            skin: Some(PathBuf::from("/skins/My Skin.wsz")),
            playlist_size: (325, 290),
            current: Some(7),
            ..Settings::default()
        };
        s.eq.enabled = true;
        s.eq.preamp_db = -3.5;
        s.eq.band_gains_db[0] = 6.0;
        s.main.position = Some((100.0, 200.5));
        s.playlist.visible = false;
        s.equalizer.visible = false;
        s.main.shaded = true;
        assert_eq!(Settings::from_text(&s.to_text()), s);
    }

    #[test]
    fn damage_falls_back_per_key() {
        let s = Settings::from_text(
            "volume = loud\nbalance = 5\neq.bands = 1,2,3\nmain.position = 1\nshuffle = true\n\
             nonsense line\nunknown.key = 1\n",
        );
        assert_eq!(s.volume, 0.7, "unparseable keeps the default");
        assert_eq!(s.balance, 1.0, "out of range is clamped");
        assert_eq!(s.eq.band_gains_db, [0.0; 10], "wrong band count is ignored");
        assert_eq!(s.main.position, None);
        assert!(s.shuffle, "good keys still apply");
    }

    #[test]
    fn empty_file_is_defaults() {
        assert_eq!(Settings::from_text(""), Settings::default());
    }
}
