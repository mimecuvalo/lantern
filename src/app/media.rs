//! Media keys and Now Playing: the system's side of the transport.
//!
//! Commands arrive from the platform layer and drive the same actions as
//! the buttons. Now Playing is republished only when something a listener
//! would notice changes — the track, playing versus paused, or a jump in
//! position — since the system extrapolates the clock by itself.

use lantern_audio::engine::{Command, Engine};

use super::App;
use crate::ui::main_window::Playback;

/// The state last published, to spot changes.
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct Published {
    title: String,
    playback: Option<Playback>,
    /// Bumped on every open and seek.
    position_epoch: u64,
}

impl App {
    /// Start listening to the system's media controls. `wake` nudges the
    /// event loop when a command arrives while it sleeps.
    #[cfg(target_os = "macos")]
    pub(crate) fn install_media(&mut self, wake: impl Fn() + Send + Sync + 'static) {
        self.media = Some(crate::platform::macos::MediaRemote::install(wake));
    }

    /// Act on media commands, and republish Now Playing if it changed.
    #[cfg(target_os = "macos")]
    pub(super) fn service_media(&mut self) {
        use crate::platform::macos::{MediaCommand, NowPlaying};

        let commands = self.media.as_ref().map(|m| m.poll()).unwrap_or_default();
        for command in commands {
            match command {
                MediaCommand::Play => self.play(),
                MediaCommand::Pause => {
                    if self.playback() == Playback::Playing {
                        self.toggle_pause();
                    }
                }
                MediaCommand::TogglePlayPause => match self.playback() {
                    Playback::Stopped => self.play(),
                    _ => self.toggle_pause(),
                },
                MediaCommand::Stop => self.send(Command::Stop),
                MediaCommand::Next => self.skip(true),
                MediaCommand::Previous => self.skip(false),
                MediaCommand::Seek(secs) => self.send(Command::Seek { seconds: secs }),
            }
        }

        let playback = self.playback();
        let title = self
            .playlist
            .current()
            .and_then(|i| self.playlist.get(i))
            .map(|e| e.display_title())
            .unwrap_or_default();
        let state = Published {
            title: title.clone(),
            playback: Some(playback),
            position_epoch: self.position_epoch.get(),
        };
        if self.published.as_ref() == Some(&state) {
            return;
        }
        self.published = Some(state);
        let Some(media) = &self.media else { return };
        if playback == Playback::Stopped {
            media.publish(None);
            return;
        }
        // Tags give "Artist - Title"; the system shows them apart.
        let (artist, title) = match title.split_once(" - ") {
            Some((a, t)) => (Some(a.to_string()), t.to_string()),
            None => (None, title),
        };
        media.publish(Some(&NowPlaying {
            title,
            artist,
            duration_secs: self.info.as_ref().and_then(|i| i.duration_secs),
            elapsed_secs: self.engine.as_ref().map_or(0.0, Engine::position_secs),
            playing: playback == Playback::Playing,
        }));
    }

    #[cfg(not(target_os = "macos"))]
    pub(super) fn service_media(&mut self) {}
}
