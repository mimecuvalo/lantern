# Non-goals

Things Lantern deliberately does **not** do, and why. These aren't "not yet" — they're "no".

## Spotify playback

Not possible. `libspotify` — the only API that ever allowed third-party apps to decode and play Spotify audio — was deprecated in 2015 and the servers were shut down. What remains:

- **Spotify Web API** only controls the official Spotify desktop client via Spotify Connect. Audio flows through the Spotify app, not through Lantern. Our EQ, spectrum analyzer, and (future) skins never see the bits, so the features that define Lantern wouldn't apply.
- **Reverse-engineered alternatives** like librespot violate Spotify's Terms of Service and get accounts banned. Not a foundation we'll build on.

If you want to play Spotify, use the Spotify app. Lantern is a local player.

## Apple Music streaming

MusicKit on macOS exposes `ApplicationMusicPlayer`, which will play Apple Music catalog tracks for subscribers. We're not integrating it either, for the same reason as Spotify: `ApplicationMusicPlayer` routes audio through a system-managed graph that bypasses our DSP pipeline. You get Apple Music playback, but without EQ and without visualization — which is most of what Lantern is for. The trade-off isn't worth it.

We *do* read your Music.app library to import **local** files — tracks on disk, including ones you downloaded from Apple Music for offline playback. That's done by reading the library index, not via `ApplicationMusicPlayer`. Streaming-only tracks are skipped with a count, never played.

## iTunes Match / iCloud Music Library sync

Lantern reads your local Music.app library. It does not sync with iCloud, does not download cloud-only tracks, does not re-evaluate smart playlists. If the file isn't on disk, Lantern can't play it.

## What Lantern **does** support

Local audio files. Specifically:

- **Formats:** MP3, AAC-LC, M4A/ALAC, FLAC, WAV, AIFF, Ogg Vorbis, MP1/MP2
  (whatever [Symphonia](https://github.com/pdeljanov/symphonia) decodes; Opus via
  `symphonia-adapter-libopus`). Not supported, and not planned: WavPack, HE-AAC,
  AC-3, WMA, APE, Musepack, tracker formats.
- **Sources:** any file on disk, including tracks Apple Music stores locally
  (downloaded from the service for offline playback, or ripped from CD into your library)
- **Playlist formats:** M3U, M3U8, CUE sheets
- **Import:** one-way import from the Music.app local library; no write-back, no sync.
  Read through the `iTunesLibrary` framework (`ITLibrary`, `src/platform/apple_music.rs`),
  which only answers Lantern running as an app, or from the library XML file Music
  writes when sharing it is turned on.

In short: if the bytes live on your disk, Lantern will play them with full DSP. If they live on someone else's server, they're out of scope.
