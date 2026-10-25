# Rust Architecture — Design

**Date:** 2026-09-21
**Status:** proposed, not yet implemented
**Supersedes:** the earlier Swift/AppKit implementation

## 1. Decisions locked

| Decision | Choice | Consequence |
|---|---|---|
| **Fidelity** | Pixel-exact Winamp 2.9 geometry | 275x116 main and EQ, 275x232 playlist, 275x14 shade. Skins supply pixels only; we never move a control. This is the only way "25,000 museum skins just work" is true. |
| **Platform** | Portable core, macOS shell only | winit + softbuffer + cpal + Symphonia everywhere. Build/test/ship macOS alone for now. Mac-only APIs behind `#[cfg(target_os = "macos")]`. |
| **First milestone** | Vertical slice | Prove every risky seam at once before any of it is deep. |

The fidelity choice is a deliberate reversal of the Swift version, which kept
its own layout (`mainPlayerHeight = 126`, `scale = 1.3`) and adapted skins to
it. That is why skins looked subtly wrong there. See inherited spec §8.3.

## 2. The two insights this design rests on

**A Winamp skin is a sprite-blitting problem, not a GUI-toolkit problem.**
Geometry is fixed and hardcoded in the format. There is no layout to compute,
no reflow, no constraint solving. A widget is a rectangle, a sprite key, and a
state enum. This is why we need no GUI framework at all.

**The surface is tiny.** 275x116 is 31,900 pixels — 128 KB. Repainting the
entire window every frame at 60fps costs ~7.6 MB/s of memcpy, which is noise.
**We will not build dirty-rectangle tracking.** Full repaint, always. This
deletes an entire category of invalidation bugs that the Swift version spent
commits fighting.

## 3. Workspace layout

```
lantern/
├── Cargo.toml                 — workspace
├── crates/
│   ├── lantern-skin/             — pure: &[u8] (.wsz bytes) -> Skin
│   └── lantern-audio/            — Symphonia + cpal + DSP, headless, CLI-testable
└── src/
    ├── main.rs                — winit event loop, wiring
    ├── ui/                    — widget table, blitter, hit testing
    └── platform/macos.rs      — media keys, Now Playing, .app integration
```

Three units, not five. `lantern-skin` and `lantern-audio` earn separation because
both are pure enough to test headlessly and both carry heavy compile times we
don't want in the UI iteration loop. **`lantern-ui` is deliberately not split** —
the widget table is tightly coupled to the application and splitting it buys
indirection, not testability.

**Amended at M5: four units.** `crates/lantern-playlist` joined for the same
reasons the other two exist. It is pure (bytes in, entries out) and so tests
without a window. It carries dependencies the UI loop should not recompile:
`encoding_rs` and `chardetng` for cue sheets in Shift-JIS and CP-1251. It
holds the playlist model and the M3U/PLS/cue parsers. Tag reading stays in
`lantern-audio` (`tags.rs`), which already has lofty.

**Amended after M6: five units.** `crates/lantern-library` holds the media
library: the SQLite store, watched-folder scans, the tags its columns need,
the standing views and search, and Apple Music import (from the library XML;
the iTunesLibrary framework is read in `platform/apple_music.rs`). Pure of UI
and audio, so it tests headlessly; it keeps `rusqlite`'s bundled SQLite out
of the UI loop's rebuilds. Scans run on a worker thread the app owns.

The binary's `src/` also grew an `app/` directory at M5, once there were three
windows. It holds one `Pane` per window (winit window, softbuffer surface,
widget table) and a behaviour file per window: `player.rs`, `equalizer.rs`,
`playlist.rs`. `ui/` stays drawing and layout only.

## 4. Dependencies

Versions were resolved at M0 and are recorded in `Cargo.lock`; the notable
ones are `symphonia` 0.6, `winit` 0.30, `cpal` 0.18, `softbuffer` 0.4.
Not pinned in this table, which would only rot.

| Need | Crate | Why, and what was rejected |
|---|---|---|
| Window + input | `winit` | Only real cross-platform option. **Resolved at M0: 0.30.13, so the `ApplicationHandler` trait is the API.** |
| Pixel presentation | `softbuffer` | Gives a plain `&mut [u32]` tied to the window. **Rejected `wgpu`:** megabytes of binary and minutes of compile time to blit 128 KB. |
| Audio output | `cpal` | CoreAudio/WASAPI/ALSA/AAudio. **Rejected `rodio`:** its mixer abstraction fights gapless, sample-accurate position, and an EQ tap — all three of which we need. |
| Decode | `symphonia` | Pure Rust. Excellent FLAC/PCM/Vorbis/MP3, Great AAC-LC/ALAC/MP1/MP2. Opus via `symphonia-adapter-libopus` if wanted later. |
| PCM handoff | `rtrb` | Lock-free SPSC ring. Realtime-safe. |
| Resampling | `rubato` | Device rate rarely matches file rate. Runs on the decoder thread, never in the callback. Band-limited FFT resampler since M4, replacing M1's linear stopgap. Trims the filter's startup delay and flushes its tail at end of track, so output length is exactly `input * ratio`, which gapless playback depends on. |
| EQ | `biquad`, or hand-rolled DF-II-transposed | 10 bands is ~80 lines hand-rolled. Decide at implementation. |
| Spectrum | `realfft` | Real-input wrapper over `rustfft`; half the work of a complex FFT. |
| Tags | `lofty` | Better tag coverage than Symphonia's metadata layer. |
| ZIP | `zip` (deflate only) | For `.wsz`. Default features off. |
| BMP/PNG | `image`, features = `["bmp", "png"]` only | Default features pull in a dozen codecs we'll never see. |
| State | `serde`, `serde_json`, `directories` | JSON to the platform config dir. |
| macOS shell | `objc2`, `objc2-media-player`, `block2` | Media keys + Now Playing, region masks, the device-RGB backing store; behind a cfg gate. |
| Menus | `muda` | Native context menus and the macOS menu bar (added M6). Winamp's menus were native too. Pulls GTK only on Linux. |
| File dialogs | `rfd` | Native open/save panels (added M6). Shares the objc2 generation on macOS. |
| Playlist text | `fontdue` | Rasterises the `pledit.txt` system font (added M5); see §6. |
| Cue encodings | `encoding_rs`, `chardetng` | Shift-JIS/CP-1251/CP-1252 cue sheets (added M5), in `lantern-playlist`. |
| Library store | `rusqlite`, feature `bundled` | The media library's tracks, history and playlists (added with the library). SQLite: crash-safe, incremental writes as a scan goes, and grows by migration. Bundled so every platform gets the same SQLite. **Rejected** a JSON file: rewriting tens of thousands of tracks for each play count. |
| Library XML | `plist`, default features off | Reads Apple Music's and iTunes' library XML for import. |

## 5. Threading model

This is the part that must be right from the first commit, because it is
expensive to retrofit.

```
  ┌─────────────┐   commands    ┌──────────────┐   PCM ring    ┌───────────────┐
  │  UI thread  │ ────────────► │   decoder    │ ────────────► │ audio callback│
  │   (winit)   │  (crossbeam)  │    thread    │    (rtrb)     │  (cpal, RT)   │
  └─────────────┘               └──────────────┘               └───────────────┘
         ▲                                                             │
         │              atomics: frame position, transport state       │
         └─────────────────────────────────────────────────────────────┘
         ▲                          tap ring (rtrb)                    │
         └─────────────────────────────────────────────────────────────┘
```

**UI thread** — owns the window, the softbuffer surface, the `Skin`, the widget
table. Renders on `RedrawRequested`. Also runs the FFT (see below). Never blocks.

**Decoder thread** — owns Symphonia and `rubato`. Drains a command channel
(open/seek/stop), decodes packets, resamples to the device rate, writes
interleaved `f32` into the PCM ring. Blocks when the ring is full. All
allocation happens here.

**Audio callback thread** — owned by cpal. Pops frames, applies EQ biquads then
preamp then volume/balance, writes the device buffer, and pushes a copy into a
small tap ring. **No allocation, no locks, no syscalls, no logging.**

**Coefficient updates** cross into the callback via a second small `rtrb`
command ring that the callback drains at the top of each invocation. Not a
mutex, not an `ArcSwap` — a ring, so the realtime thread never touches the
allocator or blocks.

**The FFT runs on the UI thread, not in the audio callback.** The callback only
copies samples into the tap ring. The UI pulls the most recent 1024-sample
window when it wants to draw, and runs `realfft` there. The analyzer needs
~60fps, not 44,100fps; doing it in the callback is pure waste and a realtime
hazard.

### Position tracking and gapless

The audio callback maintains an `AtomicU64` of frames consumed. The decoder
thread, on hitting EOF for track N, immediately opens track N+1 and keeps
filling the same ring — the callback never learns a boundary occurred, which is
exactly what gapless means.

The decoder publishes `(cumulative_frame_offset, track_id)` boundary markers to
a small queue. The UI maps the atomic frame counter against those markers to
know which track is playing and how far in. Lock-free, and correct even when
the ring holds several seconds of a future track.

**As built at M4.** The markers are `Report::Queued { at_frame, info }`, and
`Timeline` on the UI side holds `Event::Advanced` back until `frames_played`
reaches `at_frame`. The frame counter runs on across the seam; the timeline
remembers where the current track began.

- **Placing the seam exactly.** `at_frame` is frames played, plus frames
  sitting in the ring, plus what the resampler still owes for input it has
  already been given. That last term is `round(frames_in * ratio) -
  frames_out`, which is exact because output is aligned to input. An earlier
  estimate from buffered input plus filter delay was 300 frames off. A test
  plays two WAV halves of one sine and checks the marker is within 2 frames.
- **No filter restart at the seam.** When both tracks share a rate and channel
  count, the resampler moves over to the next track. Otherwise the old one's
  tail is flushed first, which is as seamless as a format change allows.

## 6. Rendering

`softbuffer` hands us `&mut [u32]` in `0x00RRGGBB`. The blitter is ours:

```rust
fn blit(dst: &mut [u32], dst_w: usize, src: &Sprite, at: Point, scale: u32)
```

- **Scaling is integer nearest-neighbour**, always. Double-size is `scale = 2`.
  There is no filtering path, so the scaling artifacts the Swift version had to
  fix with explicit pixel-snapping cannot occur here by construction.
- **Tiling** for the playlist frame's 9 tiles and the EQ background.
- **Transparency:** classic skins are largely opaque; non-rectangularity comes
  from `region.txt`, not per-pixel alpha. ~~*Open item:* verify against Webamp
  whether any sheet relies on a colorkey.~~ **Checked at M2:** Webamp's parser
  slices sheets with a plain canvas `drawImage` and applies no colorkey
  anywhere, so neither do we. The blitter skips alpha-0 pixels, which only a
  PNG sheet can produce.

### The widget model

The entire UI framework:

```rust
struct Widget {
    id:      WidgetId,
    rect:    Rect,        // logical 275x116 space, from the format
    kind:    WidgetKind,  // Button | Toggle | Slider | Display | HitZone
    pressed: bool,        // mouse is down on it
    active:  bool,        // toggle is on
}
```

Render: iterate the table, ask the skin for `sprite_for(widget)`, blit at
`rect.origin`. Hit test: linear reverse-z scan. There are ~25 widgets; a loop
is faster than any spatial index and infinitely easier to debug.

**Amended at M3.** `state: Normal | Pressed | Active` became two booleans,
because toggles (shuffle, EQ) are pressed *and* active at once and have a
sprite for each combination. And the table covers only buttons, toggles and
hit zones: anything whose look depends on playback — time digits, analyser,
marquee, kbps/kHz, slider tracks and thumbs — is drawn by
`ui::main_window::draw` from a `MainView` snapshot the app builds each frame.
Forcing a scrolling marquee or a 19-bar analyser through "one sprite per
widget" would have bent the table out of shape. The draw function stays pure,
`(skin, view, widgets) -> pixels`, so tests render it without a window.

**Frame pacing.** 60fps while playing (the analyser), 20fps while paused or
while a long title scrolls, and no redraws at all otherwise. A redraw is
requested only when that deadline passes. Requesting one on every
`about_to_wait` spins the loop, since each redraw wakes it again; the first M3
build did that and pinned a core.

Input: winit gives physical pixels → divide by window scale factor and the
double-size factor → logical coords → hit test. On mouse-down, latch
`active: Option<WidgetId>` and route all subsequent motion there until
mouse-up, which is what makes slider drags work off-widget.

### The playlist font (M5)

The playlist is the one place Winamp draws with a real font: `pledit.txt`
names it, Arial by default. We rasterise that system font with `fontdue`, a
small pure-Rust rasteriser, at *physical* resolution, so text is sharp on
Retina even though everything else is integer-scaled pixel art. Font files
are found by name in the usual system directories, falling back through
Arial, Helvetica, Geneva, DejaVu Sans and Liberation Sans. Skins from Windows
name Tahoma and Verdana freely, which macOS may lack. Text is antialiased,
where Winamp's GDI text was not. That is a deliberate concession: aliased
9px text on a Retina display reads worse, not more authentically.

### Persistence (M6)

`settings.ini` holds plain `key = value` lines, and `playlist.m3u8` sits
beside it, in the platform's config directory (on macOS,
`~/Library/Application Support/Lantern`). Both are written on quit and
autosaved every ten seconds when something changed. Hand-rolled rather than
serde: a few dozen scalars, and a human can repair the file. Reading is
forgiving, so a bad key keeps its default. Cue tracks survive the M3U8
round-trip through a `#LANTERNSEGMENT:start,end` line, which other players
ignore. Command-line files replace the saved playlist and autoplay;
otherwise it is restored, stopped.

The directory is shared by name with the Swift version, whose
`equalizer.json`, `playlist.json` and `state.json` are left untouched.

### Menus (M6)

Winamp mixed two kinds of menu, and so do we. The playlist's ADD, REM, SEL,
MISC and LIST pop-ups are skinned: columns of `pledit.bmp` buttons, drawn and
hit-tested like any widget. Everything else was a native menu, so here it is
native too, via `muda`: the main menu, EQ presets, playlist sort and misc
options, the playlist context menu, and a macOS menu bar. Menus are rebuilt
on each open so check marks are current. A popup is modal on macOS, so its
choice is read the moment it closes.

### Windows and docking (M5)

Three winit windows, each borderless with its own softbuffer surface. The EQ
and playlist open stacked under the main window and follow its `Moved`
events. A window dragged away undocks; dropped within 10 logical pixels of
its slot, it snaps back and re-docks. This is simpler than Winamp's
snap-to-any-edge magnetism, which remains for M6 if wanted. Double size
doubles the main and EQ windows but not the playlist, as Winamp 2 does.

### Window shaping — the known risk

`region.txt` defines non-rectangular windows as polygons. winit exposes no
window-shape API, and transparent-window support across platforms is uneven.
This is the one place the "portable core" claim may crack.

**Investigated at M2.** Findings:

- **Parsing is portable and done.** `lantern_skin::Region` holds the polygons for
  all four sections (`[Normal]`, `[WindowShade]`, `[Equalizer]`,
  `[EqualizerWS]`), with Webamp's tolerance rules.
- **The data is rare.** None of the four acceptance skins has a real region:
  base-2.91's `region.txt` is documentation only, every line a comment. The
  committed fixture skin carries one so the path is exercised.
- **Per-pixel alpha through softbuffer is not an option on macOS.** Its
  CoreGraphics backend builds the image with `kCGImageAlphaNoneSkipFirst`: the
  alpha byte is ignored and the surface is always opaque.
- **A layer mask is.** softbuffer draws into a *sublayer* of the content
  view's root layer, so a mask on the root layer clips it without touching
  softbuffer. `platform::macos::set_window_region` builds that mask: a
  container layer with one filled `CAShapeLayer` per polygon. Their composite
  alpha is the union of the polygons, which is what `region.txt` means (Webamp
  renders it as an SVG `clipPath`, also a union). A single path with even-odd
  or non-zero fill would punch holes where polygons overlap with opposite
  winding. The window is created with `with_transparent(true)`, only for shaped
  skins, so the masked-out area shows the desktop rather than the window
  background, and the shadow is invalidated after masking.
- **Cost.** It uses the objc2 0.6 crates softbuffer and cpal already build, so
  it adds no new crates. `unsafe` is confined to `platform/macos.rs`.
- **Not yet verified on screen.** The M2 session could not capture the screen,
  so what the mask does visually, and whether clicks in the cut-out area pass
  through to the window below, has not been seen. Check this first when region
  work resumes.

**Conclusion:** shaped windows are a platform-shell feature, not a core one.
macOS is achievable as above. Windows has `SetWindowRgn` natively, which is
what Winamp itself relied on. X11 has the Shape extension; Wayland would need
an input region plus real alpha. None of this touches `lantern-skin` or the
renderer. Double-size mode scales the polygons by the same integer factor as
the pixels (`set_window_region` takes the scale); `[WindowShade]` and the EQ
sections wait for those windows (M5/M6).

## 7. Audio detail

**EQ: 10 bands at Winamp's authentic frequencies** — 70, 180, 320, 600, 1000,
3000, 6000, 12000, 14000, 16000 Hz. Not the ISO octave series; one of the forks
"fixed" these to 32/64/125/... and that change was explicitly rejected.

Two corrections from the Swift work carry forward, both already validated there:

1. **Outer bands are shelves, not peaking.** Low-shelf at 70, high-shelf at
   16000, peaking for the eight in between. A parametric filter at the edge of
   the spectrum leaves the actual extremes unaffected.
2. **Preamp is a gain stage, not a volume multiplier.** ±12 dB applied to the
   EQ block, not folded into output volume — otherwise any volume or mute
   change silently wipes it.

**Resampling** happens on the decoder thread when file rate ≠ device rate. cpal
reports the device rate at stream build; treat a rate change as a stream rebuild.

**Seek** = flush the ring, `symphonia::seek`, reset biquad state (otherwise the
filters ring audibly across the discontinuity), reset the frame counter.

**Declick (M4).** A flush does not cut a waveform mid-cycle. The callback fades
the audio already playing out over ~5 ms (256 frames), drops the rest of the
ring, then fades the first new audio in over the same span. The same happens on
stop and open. Gapless seams never flush, so they are untouched. The callback
body is a plain `Output` struct, so this is tested without a device.

## 8. Skin subsystem (`lantern-skin`)

Pure function: `parse(zip_bytes: &[u8]) -> Result<Skin>`. No file I/O, no
platform deps, no windowing types. Fully testable against the four acceptance
skins in `skins/`.

Carry forward the Swift version's hardening, which was genuinely good:

- **In-memory only.** Entries land in a map keyed by *lowercased basename* with
  all path components stripped, including backslash-separated Windows paths.
  This makes zip-slip structurally impossible rather than defended-against.
- **Two-sided bomb caps.** Check the declared `uncompressed_size` *and* count
  bytes while streaming, so a lying header buys nothing. 32 MB/entry, 96 MB total.
- **Damaged entries are skipped, not fatal.** These archives are 25 years old
  and frequently slightly broken. Required-file validation catches the cases
  where the missing entry actually mattered.

Format details confirmed in the inherited spec, all directly portable:

- `nums_ex.bmp` overrides `numbers.bmp` when both exist (Webamp cascade order)
- Lookup is case-insensitive; skins ship `Cbuttons.bmp`, `PLEDIT.TXT`, etc.
- Skins may nest everything one directory deep
- `viscolor.txt` → 24 RGB triples; defaults when absent
- `pledit.txt` → INI, playlist colors and font; defaults when absent
- `region.txt` → `[Normal]` polygon only
- EQ graph colors are sampled from `eqmain.bmp` pixels at y=313 (19 px) and y=314 (1 px)
- 14 EQ slider thumb variants; 28 prerendered volume/balance backgrounds; 9 playlist frame tiles

Sprite coordinates port from Webamp's `skinSprites.ts` (MIT).

## 9. Milestones

| # | Goal | Done when |
|---|---|---|
| **M0** | Toolchain and skeleton | ✅ **Done 2026-09-22.** Latest stable Rust (no pinned toolchain), workspace of 3 units builds, fmt/clippy(-D warnings)/test/release all green, CI workflow added |
| **M1** | **Vertical slice** | ✅ **Done 2026-09-22.** All four skins parse; main.bmp and the six transport buttons blit at classic coordinates; MP3 decodes through cpal with 44.1k→48k conversion; play/pause/stop hit-test and work. |
| **M2** | Skin parser complete | ✅ **Done 2026-09-26.** Every sprite Webamp models (14 sheets incl. `eq_ex`, `nums_ex` minus signs), `text.bmp` font, viscolor/pledit/region parsers; original-artwork fixture skin committed so CI covers parsing; region investigation done (§6) with a macOS mask prototype |
| **M3** | Main window complete | ✅ **Done 2026-09-27.** Every control renders and works: seek (engine flush handshake), volume, balance, marquee via the bitmap font, time digits, mono/stereo, kbps/kHz, 19-bar spectrum analyser, title-bar close/minimise, always-on-top, double size |
| **M4** | Audio engine complete | ✅ **Done 2026-09-27.** 10-band EQ + preamp (RBJ biquads, shelves at the ends, coefficients over an rtrb ring), gapless (sample-continuous seam, exact seam marker), rubato resampling (exact length, no leading silence), declicked seek/stop/open, FFT tap. EQ is not yet reachable from the UI; that is the M5 EQ window |
| **M5** | Playlist + EQ windows | ✅ **Done 2026-09-27.** EQ window (sliders, ON/AUTO, spline response curve, live to the engine); playlist window (9-tile frame resizable on its 25x29 grid, `pledit.txt` font and colours, selection, scrolling, mini transport, running time); docking; M3U/PLS/cue; folders and drag-and-drop; playlist-driven gapless playback with shuffle/repeat; cue tracks gapless within a file. Menus (ADD/REM/SEL/MISC/LIST, presets, options) and file dialogs are M6 |
| **M6** | Shell polish | ✅ **Done 2026-09-28.** Persistence (settings.ini + playlist.m3u8, autosaved), windowshade for all three windows, native menus (main, presets, sort, misc, playlist) and macOS menu bar, file dialogs, the playlist's skinned ADD/REM/SEL/MISC/LIST menus, `.eqf` presets, live skin switching, media keys and Now Playing (unverified outside an app bundle), and the per-frame colour conversion removed (~28% to ~16% CPU while playing) |
| **M7** | Packaging | `.app` bundle, DMG via the existing script, release CI |

`region.txt` rendering slots into M5 or M6 depending on what §6 turns up.

## 10. Risks

1. ~~**Shaped windows** (§6).~~ **Investigated at M2.** Solvable per platform
   in the shell (macOS: a root-layer mask, prototyped), and absent from every
   acceptance skin. The remaining risk is only that the macOS mask has not
   been seen on screen yet.
2. ~~**BMP variants.**~~ **Largely retired at M1.** `base-2.91.wsz`'s `main.bmp`
   is indeed 8bpp BI_RLE8, and `image` 0.25 decodes it correctly, as it does
   every sheet in all four acceptance skins. A fallback reader is not needed
   for now; revisit only if a museum skin in the wild fails.
3. **winit API churn.** The 0.30 `ApplicationHandler` migration was disruptive.
   ~~Verify on add.~~ Resolved at M0 (0.30.13); `Cargo.lock` is committed, so
   read the changelog before any deliberate bump.
4. **Device changes mid-playback.** Unplugging headphones changes the device and
   possibly the rate. Needs explicit stream-rebuild handling; easy to defer and
   then painful.
5. **Codec coverage vs. the actual library.** Worth auditing the real music
   folder early — if it holds Opus or WavPack, that changes M4's scope. The
   same audit would show whether multichannel files exist: surround sources
   are currently folded to stereo by dropping channels, not downmixed.
6. **Metadata readers are not optional.** Enabling a codec is not enough:
   symphonia's probe cannot find the MP3 sync behind an ID3v2 tag unless the
   `id3v2` *metadata* reader is also enabled. Discovered at M1, when every real
   MP3 failed to open while the codec was present. `id3v1`, `id3v2` and `ape`
   are now enabled alongside the codecs.
7. ~~**Presenting frames costs more than drawing them.**~~ **Largely fixed at
   M6.** Measured at M3 on a Retina Mac, release build, playing: most of the
   main thread's work was Core Animation converting softbuffer's `DeviceRGB`
   `CGImage` to the display's colour space on every frame. Setting each
   window's `colorSpace` to device RGB (`platform::macos::use_device_rgb`)
   removes the conversion. Measured by CPU time over 10s while playing, with
   three windows: 28.3% of a core before, 15.8% after. (`ps %cpu`, used for
   the M3 numbers, is a decaying average and understated both.)
   The trade-off is colour management. Skin pixels reach the display as raw
   values, as Winamp's did on Windows, and may look slightly more saturated on
   a wide-gamut display. If idle cost still matters, 30fps is one constant
   (`PLAYING_FRAME`).

## 11. Open questions

- **Name.** Decided: Lantern (formerly Wamp). Clearance still to do; see `docs/legal.md` §5.
- **Music.app import.** Resolved with the library: `ITLibrary` through objc2, and the XML path. `ITLibrary`'s helper refuses a binary run from a terminal, so it needs the app bundle (`scripts/bundle-app.sh`), the first piece of M7.
