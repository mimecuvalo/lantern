# Lantern

A native, Winamp 2.x-compatible music player for macOS, written in Rust.

> **Status: pre-release.** It plays music, reads classic skins pixel-exactly,
> and remembers its state, but there is no packaged app yet — you build and run
> it from source.

## Why

macOS lacks a good local music player. IINA and VLC are video players wearing a
hat, Music.app is heavy, and none of them are Winamp. The goals, in order:

1. **Fast** — instant launch, no beachballs
2. **Small** — single-digit megabytes
3. **Real Winamp 2.x skin support** — the ~25,000 skins on
   [skins.webamp.org](https://skins.webamp.org/) should just work

Rust was chosen over the working Swift implementation after using it: the
cross-platform door stays open (Windows, Linux, and eventually Android/iOS), and
codec coverage stops being capped by what AVFoundation happens to decode.

## Running it

You need Rust (latest stable, via [rustup](https://rustup.rs)) and the Xcode
Command Line Tools (`xcode-select --install`). Full Xcode is not required.

```bash
git clone <this repo> && cd music
cargo run --release -- ~/Music/SomeAlbum          # files, folders, .m3u/.m3u8/.pls/.cue
cargo run --release -- --skin path/to/skin.wsz    # choose a skin
```

**Skins.** Lantern comes with its own skins, original artwork drawn in code
(see [`docs/legal.md`](docs/legal.md)): Night, built in, and more in
`assets/skins/` — Aurora, Dusk, Fireflies, Cave, Temple, Wireframe. It loads any
classic Winamp skin too: Get More Skins… in the menu opens
[skins.webamp.org](https://skins.webamp.org/). Pass a skin with `--skin`, or load one at runtime from the main menu.

**Using it.** It behaves like Winamp 2:

- Right-click (or the `O` in the clutter bar) for the main menu; the macOS menu
  bar has File and Window menus too.
- Drag windows by their title bars. They snap to each other and to the screen
  edges; the main window carries every window attached to it. Double-click a
  title bar for windowshade mode.
- Keys: `Z X C V B` previous / play / pause / stop / next, `L` open files,
  `J` jump to a file in the playlist, `I` (or `⌥3`) file info,
  arrows seek and change volume, `Ctrl-A` always on top, `⌘Q` quit. In the
  playlist, `Enter` plays, `Delete` removes, `⌘A` selects all.
- The playlist's ADD / REM / SEL / MISC / LIST buttons open their menus; click
  an item, or press, slide and release as in Winamp.
- The EQ's PRESETS button has Winamp's stock presets and loads and saves
  `.eqf` files.
- Media keys and Control Centre's Now Playing work.

**Settings** (volume, EQ, toggles, skin, window layout, playlist) are saved in
`~/Library/Application Support/Lantern/`: `settings.ini` and `playlist.m3u8`,
both plain text. Set `LANTERN_CONFIG_DIR` to use another folder — handy for
development runs that shouldn't disturb your real setup:

```bash
LANTERN_CONFIG_DIR=/tmp/lantern-dev cargo run -- sample.mp3
```

**Media library.** Option-L opens it. The Library button (also the Library
menu, and right-click in its window) adds folders to watch and imports your
Apple Music library — its local songs, play counts, ratings and playlists.
Pick a view or playlist in the tree, type to search, click a column header
to sort, and double-click (or Play) to play; Enqueue adds to the playlist.
Drag tracks onto the playlist to put them exactly there. Right-click a track
to rate it or send it to a playlist, a column header to choose columns, the
tree to make, rename or delete playlists. Pick an artist or album in the
panes above the Audio view to narrow it. Changes in watched folders show up
on their own. Importing straight from
Apple Music works only with Lantern running as an app, not from `cargo run`;
build and launch one with:

```bash
scripts/bundle-app.sh --open             # --release for an optimised build
```

Or turn on Music's *Settings → Files → Share Library XML with other
applications* and use Import Library XML File… instead.

## Developing

Read [`CLAUDE.md`](CLAUDE.md) and
[`docs/design/2026-09-21-rust-architecture.md`](docs/design/2026-09-21-rust-architecture.md)
first: they fix the threading model, crate layout and the decisions not to
revisit quietly.

```bash
cargo build && cargo test --workspace
cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings
cargo run -p lantern-audio --example play -- track.mp3 5     # headless audio check
LANTERN_RENDER_DUMP=/tmp cargo test --bin lantern render        # windows as PNGs
cargo run -p lantern-skin --example make_fixture            # regenerate the test skin
```

CI runs formatting, clippy with warnings as errors, the tests and a release
build; run all four before calling something done. The toolchain is
deliberately unpinned — latest stable only.

Tests against real skins in `skins/` run locally and skip in CI with a loud
`SKIPPED` line; the committed, original-artwork fixture
`tests/fixtures/skins/lantern-fixture.wsz` is what guards CI.

## Prior art and provenance

This is a clean-room implementation. Skin format knowledge derives from
[Webamp](https://github.com/captbaritone/webamp) (MIT) and public format
documentation — **never** from the Winamp source that was briefly published under
the Winamp Collaborative License in 2024. [`NOTICE`](NOTICE) lists what came
from where.

Not affiliated with, endorsed by, or connected to Winamp or Llama Group.
"Winamp" is their trademark; it appears here only to describe the skin format
this player reads.

See [`docs/legal.md`](docs/legal.md) for the full provenance rules.

## Repository

| Path | What |
|---|---|
| `crates/lantern-skin` | `.wsz` bytes → skin: sprites, fonts, colours, regions. Pure, no I/O |
| `crates/lantern-audio` | Decode, resample, EQ, output, spectrum analyser, tags |
| `crates/lantern-playlist` | Playlist model; M3U/PLS/cue parsing with encoding detection |
| `src/` | The app: windows, drawing, input, menus, macOS integration |
| [`docs/legal.md`](docs/legal.md) | Provenance, trademark, codec patents |
| [`docs/non-goals.md`](docs/non-goals.md) | What this deliberately won't do |
| [`docs/reference/skin-format-design.md`](docs/reference/skin-format-design.md) | `.wsz` format analysis (inherited, partly obsolete — see its header) |
| `tests/fixtures/` | Test skin, cue sheets covering encoding edge cases, sample audio |
| `scripts/` | DMG packaging, release notes |

## License

MIT — see [`LICENSE`](LICENSE). Forked from
[wishval/wamp](https://github.com/wishval/wamp), whose Swift implementation and
design documentation this project builds on.
