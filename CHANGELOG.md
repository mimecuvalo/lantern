# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

Nothing has been released yet. After `[Unreleased]` comes the
**[commit history](#commit-history)**: every commit message from the start of
the project, kept here because the git history itself was then cleared.

## [Unreleased]

### Added

- **The media library**, so far its window and what is behind it. Alt-L (or
  Media Library in the Window menu and the main menu's Windows submenu) shows
  and hides the window, which drags, snaps, docks and resizes like the
  playlist. It is framed by the skin's `gen.bmp`, as in Winamp; skins without
  one get Lantern's own frame in the style of the classic base skin. The new
  Library menu (also on right-click in the window) adds folders to watch,
  rescans them, and imports an Apple Music library — directly, with your
  permission, or from its library XML file — with play counts, ratings and
  playlists. Watched folders are rescanned at launch for what changed, and
  playing a track counts the play. Inside, Winamp's layout: a tree of
  Local Media (Audio, Most Played, Recently Added, Recently Played, Never
  Played, Top Rated) and your playlists; a search box that narrows the list
  as you type; a track list whose column headers sort it; Play and Enqueue
  beneath, and the Library button for the menu. Double-click or Enter plays
  the selection, or the whole list, in place of the playlist. Buttons and
  scrollbars come from the skin's `genex.bmp`. Tracks drag from the list
  onto the playlist, landing where they are dropped, or onto the main
  window to join the end. The divider between tree and list drags; column
  headers resize from their edges and move by dragging; right-click a
  header to show or hide columns (Date Added, Bitrate and File too), or a
  track to play, enqueue, rate, show in Finder or remove it from the
  library. The layout is remembered.
- **`scripts/bundle-app.sh`** builds Lantern into `Lantern.app` and can
  launch it. Importing straight from Apple Music needs Lantern running as an
  app; run from a terminal, the import now says so.
- **Get More Skins…** in the main menu and the File menu opens the Winamp
  Skin Museum (skins.webamp.org), where any skin downloads as a `.wsz` that
  Load Skin… opens.
- **Lantern's own skins**, original artwork drawn in code. Night is built in:
  a starry sky, a crescent moon, glass controls, a critter peeking over the
  display. Aurora, Dusk, Fireflies, Cave, Temple and Wireframe ship beside it
  in `assets/skins/`. Default Skin in the main menu returns to Night.
- **Polish.** Windows snap to each other's edges and to the screen's, the
  main window carries every window attached to it, and dragging no longer
  lags. The EQ's PRESETS menu has Winamp's seventeen stock presets. Always on
  Top is in the menu bar's new Window menu and on Ctrl-A. `LANTERN_CONFIG_DIR`
  points settings at another folder.
- **M6: shell polish.** Your settings, playlist and window layout are
  remembered between launches. Every window rolls up into Winamp's windowshade
  strip (shade button or double-click the title bar). Menus: right-click or the
  options button for the main menu; the playlist's ADD/REM/SEL/MISC/LIST
  buttons pop up their skinned menus; the EQ's PRESETS loads and saves `.eqf`
  presets. Eject and ⌘O open files; skins load live from the menu. Media keys,
  Control Centre and headphone buttons control playback, and Now Playing shows
  the track. Playback uses markedly less CPU.
- **M5: equaliser and playlist windows.** Both open docked under the main
  window and follow it when it moves; the main window's EQ and PL buttons show
  and hide them. The equaliser's ten bands and preamp now shape the sound, with
  the response curve drawn live. The playlist shows your tracks in the skin's
  font and colours, resizes, scrolls, selects (click, Shift-click, Cmd-click),
  plays on double-click or Enter, and removes with Delete. Add music by passing
  files, folders, M3U/PLS playlists or cue sheets on the command line, or by
  dropping them on any window. Previous, next, shuffle and repeat work, and
  playback stays gapless in playlist order — including across the tracks of a
  cue sheet.
- **M4: complete audio engine.** Gapless playback: pass several files and they
  play back to back with no gap or click at the join. A 10-band equaliser with
  preamp, at Winamp's band frequencies, not yet reachable from the UI; the EQ
  window arrives at M5. Band-limited resampling replaces the stopgap, so
  high-rate files no longer alias. Seeking, stopping and switching tracks fade
  over a few milliseconds instead of clicking.
- **M3: complete main window.** Every classic main-window control works: the
  seek bar, volume and balance sliders, the time display (click it for time
  remaining; it blinks while paused), a 19-bar spectrum analyser in the skin's
  colours, a scrolling title with volume, balance and seek readouts while
  dragging, kbps, kHz and mono/stereo, the title bar's close and minimise
  buttons, always-on-top and double size from the clutter bar, and Winamp's
  keyboard shortcuts (Z X C V B, arrows). A track given on the command line
  starts playing.
- **M2: complete skin parser.** Every classic-skin sprite (all 14 sheets,
  including `eq_ex.bmp` and `nums_ex.bmp`), the `text.bmp` bitmap font, and the
  `viscolor.txt`, `pledit.txt` and `region.txt` files, each as forgiving of
  malformed input as Webamp. The visualiser now falls back to Webamp's default
  palette instead of a placeholder.
- **Shaped windows (macOS, experimental).** Skins with a `region.txt` get a
  window clipped to the region's polygons. Not yet checked visually.
- **Test fixture skin.** A small original-artwork `.wsz` that exercises the
  parser's corner cases, so CI now covers skin parsing.
- **M1: end-to-end vertical slice.** Parses a `.wsz`, renders the classic
  275x116 main window from its sprites, decodes audio through Symphonia into a
  lock-free ring feeding a cpal callback, and wires play/pause/stop to the real
  transport buttons. Borderless window, draggable by the title strip.
- **M0: workspace skeleton and CI.** Three units (`wamp-skin`, `wamp-audio`,
  the `wamp` binary) with module structure and type definitions in place;
  `cargo fmt`, `cargo clippy -D warnings`, `cargo test` and a release build all
  green. Builds on latest stable Rust; no pinned toolchain. CI runs the same four
  steps on macOS using first-party actions only.
- **Rust architecture design** (`docs/design/2026-09-21-rust-architecture.md`).
  Locks fidelity (pixel-exact Winamp 2.9 geometry), platform scope (portable
  core, macOS shell), threading model, crate layout, dependency choices and a
  seven-milestone plan starting from an end-to-end vertical slice.

### Changed

- **Renamed to Lantern** (formerly Wamp). Settings move to
  `~/Library/Application Support/Lantern`; the old folder is read until
  Lantern first saves, so nothing is lost.

### Fixed

- Colours were oversaturated on wide-gamut displays; windows are now
  colour-managed as sRGB, using less CPU than before.
- The spectrum analyser now moves like Winamp's: bars leap and drop away
  instead of hovering at similar heights.
- The lamp beside the play arrow lit red; it lights green while playing.
- Playlist lengths showed 3:46 where Winamp shows 3:45.
- The playlist's menu items (ADD FILE, ADD DIR, LOAD LIST and the rest)
  did nothing; they now open their panels.
- The marquee text sat at the top of its box instead of centred.

## Commit history

Newest first, grouped by day: each commit's subject in bold, its author and any
co-authors, then its message body.

### 2026-10-11

- **feat: Night's glass controls get brighter edges** — Mime Čuvalo, with Claude Opus 5.5 (1M context)

  Full rim light on every edge and a lit top lip — the Bright trial —
  instead of a faint outline, so the controls stand out against the sky.
  Aurora, Dusk and Fireflies share Night's glass and gain it too. The
  edge-trial scaffolding goes.

- **feat: trial five brighter button edges for the Night skin** — Mime Čuvalo, with Claude Opus 5.5 (1M context)

  Night's glass controls were hard to make out. The generator now writes
  five edge treatments to skins/night-*.wsz (gitignored) to compare —
  Bright, Crisp, Neon, Bevel and Cyan — beside the soft edge that still
  ships unchanged. The render tests draw the trials when present.

- **feat: Night becomes the built-in skin; Lantern's themes are committed** — Mime Čuvalo, with Claude Opus 5.5 (1M context)

  The night sky is now the default; Default Skin returns to it. The other
  themes ship in assets/skins/ (Aurora, Dusk, Fireflies, Cave, Temple,
  Wireframe) instead of the gitignored skins/, and the render tests draw
  them all. Grotto and Snowfall are dropped, and Fireflies' frog gives way
  to a field mouse with a pink nose.

- **docs: Winamp's stock skins may live in skins/ locally, never committed** — Mime Čuvalo, with Claude Opus 5.5 (1M context)

- **feat: Get More Skins… opens the Winamp Skin Museum** — Mime Čuvalo, with Claude Opus 5.5 (1M context)

  In the main menu and the menu bar's File menu, beside Load Skin… and
  Default Skin. CLAUDE.md now says Winamp's own stock skins don't belong
  in skins/ even locally.

- **feat: five variations on the night-sky skin** — Mime Čuvalo, with Claude Opus 5.5 (1M context)

  Night gains moods that change the sky, the ground, the creature peeking
  over the display and the playlist panel's scene: Aurora (curtains over
  snow-capped peaks, an owl), Dusk (a magenta horizon, hills, a setting
  sun, a fox), Snowfall (falling snow, snow on every ledge, icicles, a
  full moon, a hare) and Fireflies (tall grass, a swarm, a firefly jar, a
  frog), beside the original Night. Written to skins/ to try; the
  built-in default is unchanged.

- **feat: five skin directions that differ in construction, not colour** — Mime Čuvalo, with Claude Opus 5.5 (1M context)

  All blue, same fonts. Each theme now picks a style that builds the
  surfaces: Cave (rim-lit brick, the default), Grotto (water-worn rock,
  waterfalls, a pool, pebble buttons), Temple (dressed stone panels,
  fluted pillars, a key-pattern frieze, carved buttons), Night (no stone:
  a dithered sky, stars, a moon, glass controls) and Wireframe (a grid,
  glowing outlines with corner brackets, scanlines). The playlist font is
  Verdana throughout.

- **chore: the repository now lives in ~/Sites/lantern** — Mime Čuvalo, with Claude Opus 5.5 (1M context)

- **docs: CHANGELOG notes the rename and brings the history up to date** — Mime Čuvalo, with Claude Opus 5.5 (1M context)

- **feat: five skin directions from one generator** — Mime Čuvalo, with Claude Opus 5.5 (1M context)

  The generator becomes themeable: a palette of colour roles plus switches
  for flora, creatures, glow and scanlines. Lantern (the ink-blue cave)
  stays the built-in default; Ember (a hearth), Orchid (a moonlit temple),
  Phosphor (a green-screen terminal) and Pocket (a light handheld LCD) are
  written to skins/ to try. The render tests draw any skins/lantern-*.wsz
  present, for comparing by eye. Label boxes and unlit lamps use roles
  that contrast in light themes too.

- **chore: rename the project to Lantern** — Mime Čuvalo, with Claude Opus 5.5 (1M context)

  Crates become lantern-skin, lantern-audio and lantern-playlist; the
  binary is lantern; window titles, menus, docs and scripts say Lantern;
  the env vars are LANTERN_CONFIG_DIR and LANTERN_RENDER_DUMP. Settings
  move to Application Support/Lantern, reading the old Wamp folder until
  Lantern first saves, and playlists write #LANTERNSEGMENT while still
  reading #WAMPSEGMENT. The default skin is Lantern too, with LANTERN in
  its title bar. The name is no longer an open decision; clearing it
  before release is.

- **chore: retire the swift branch** — Mime Čuvalo, with Claude Opus 5.5 (1M context)

  The Swift/AppKit implementation is no longer a reference: the branch is
  deleted, and CLAUDE.md, the README and the design doc stop pointing at it.

- **docs: CHANGELOG starts at the Rust rewrite** — Mime Čuvalo, with Claude Opus 5.5 (1M context)

  Drops the Swift-era releases and commits, and the entries that only
  described the pivot away from Swift.

- **docs: fold the git history into CHANGELOG.md** — Mime Čuvalo, with Claude Opus 5.5 (1M context)

  The Swift-era releases (1.1.0, 1.1.1) move in from the swift branch, and
  every commit message follows, newest first with authors and co-authors,
  so the record survives the history being cleared.

- **feat: Lanternwell, brighter — rim light, glowing flora, creatures** — Mime Čuvalo, with Claude Opus 5.5 (1M context)

  Inkier blue stone with periwinkle rim light catching the tops of some
  blocks and the floors of every cut. Glowing ferns along the floors, vines
  hanging with pink buds, yellow sparkles, a lantern on its chain. A small
  creature peeks over the top of the display, and a ghost koi drifts
  through the playlist's open panel. The EQ's labels sit on dark plaques
  so they still read.

- **docs: work directly on main from here on** — Mime Čuvalo, with Claude Opus 5.5 (1M context)

### 2026-10-01

- **docs: Lanternwell, the analyser model's provenance, and changelog** — Mime Čuvalo, with Claude Opus 5.5 (1M context)

- **fix: analyser levels on Winamp's linear scale** — Mime Čuvalo, with Claude Opus 5.5 (1M context)

  Bars sat at similar heights, squeezed by a decibel scale, where Winamp's
  leap and fall to nothing. Levels now follow Webamp's model of Winamp's
  analyser (VisPainter.ts, FFTNullsoft.ts): linear magnitudes tilted by a
  log10 ramp, 75 points averaged in fours, 15px tall with the peak a row
  above. Each point averages the bins across its span, so a pure tone
  cannot fall between points.

- **fix: colour-manage the windows as sRGB, without the per-frame cost** — Mime Čuvalo, with Claude Opus 5.5 (1M context)

  M6 made the windows device RGB to skip Core Animation's CPU conversion
  of every frame, but that sent skin colours to the display raw: beside
  Winamp on a wide-gamut screen, ours were visibly oversaturated. Now the
  window is sRGB and each presented frame is relabelled sRGB (a copy that
  shares its pixels), so nothing converts in-process and the window server
  matches colours on the GPU. Playing measured ~9% of a core, against
  ~16% before.

- **feat: Lanternwell, an original built-in default skin** — Mime Čuvalo, with Claude Opus 5.5 (1M context)

  A quiet cave at night: blue-violet stone, moss, fireflies, LCD digits
  with their unlit segments showing, a lantern by the about hotspot, and
  something watching from a crack in the wall. Every pixel, and the 4x5
  font, is drawn in code by examples/make_default_skin.rs, so the art is
  ours and ships in the binary. It replaces the fallback to a local
  base-2.91, and the main menu gains Default Skin to return to it.

- **fix: playlist lengths cut seconds rather than round, as Winamp's do** — Mime Čuvalo, with Claude Opus 5.5 (1M context)

  A 3:45.9 track showed 3:46 where Winamp shows 3:45, and totals now add
  up whole seconds per track so they agree with the list.

- **fix: light the green lamp beside the play arrow, not the red** — Mime Čuvalo, with Claude Opus 5.5 (1M context)

  The playing state drew the 'working' frame, whose red lamp Winamp lights
  only while busy (buffering a stream). Playing normally shows the green.

### 2026-09-30

- **docs: README on building, running and developing; WAMP_CONFIG_DIR** — Mime Čuvalo, with Claude Opus 5.5 (1M context)

  The README described a pre-implementation repo. It now covers
  prerequisites, running with files and skins, the controls, where
  settings live, and the development loop. WAMP_CONFIG_DIR points settings
  elsewhere, so development runs leave the real ones alone. Changelog
  entries for this list.

- **feat: drag windows ourselves, snapping to each other and the screen** — Mime Čuvalo, with Claude Opus 5.5 (1M context)

  The system drag moved the EQ and playlist only after each asynchronous
  Moved event, so they trailed the main window and the drag dropped
  motion. Title-bar drags are now ours: every pointer move places the
  whole group at once, from the pointer's true screen position.

  Snapping follows Winamp: any window snaps within 10pt to any edge of the
  others, side by side or aligned, and to the screen's usable edges. The
  main window carries everything attached to it, through chains; others
  move alone. Shading, double size and playlist resizing push along the
  windows attached below or beside. Hiding the EQ no longer pulls the
  playlist up. The docked setting goes: positions say it all.

- **feat: Winamp's stock EQ presets under PRESETS > Load** — Mime Čuvalo, with Claude Opus 5.5 (1M context)

  The seventeen presets (Classical to Techno), with values from Webamp's
  presets/builtin.json (MIT), credited in NOTICE. Loading from and saving
  to .eqf files moves beside them.

- **feat: Always on Top in the menu bar, and on Winamp's Ctrl+A** — Mime Čuvalo, with Claude Opus 5.5 (1M context)

  It was only the clutter bar's 7px A and a right-click submenu. The menu
  bar's new Window menu carries a check that follows the A toggle.

- **fix: playlist menu items run on release, so their file panels open** — Mime Čuvalo, with Claude Opus 5.5 (1M context)

  A modal NSOpenPanel started during a mouse-down cancels itself at once,
  so ADD FILE, ADD DIR, LOAD LIST and the rest silently did nothing. Items
  now run on release, as Winamp's do: click a button then an item, or
  press, slide to an item and let go. The menu is painted away before an
  item runs, and a press in another window or losing focus closes it.

- **fix: centre the marquee text in its box (y 24 -> 27, as Webamp)** — Mime Čuvalo, with Claude Opus 5.5 (1M context)

### 2026-09-28

- **docs: M6 done — persistence, menus, the colour-space fix; NOTICE noted** — Mime Čuvalo, with Claude Opus 5.5 (1M context)

- **perf(macos): skip the per-frame colour conversion — ~28% to ~16% CPU** — Mime Čuvalo, with Claude Opus 5.5 (1M context)

  Tagging each window's backing store as device RGB, the space softbuffer
  already tags its frames with, removes Core Animation's CPU colour
  conversion of every frame (vImageConvert_AnyToAny; commit work falls
  from 345 to 102 samples in a 4s profile). Measured by CPU time over 10s
  while playing: 28.3% of a core before, 15.8% after.

  The trade-off is colour management: skin pixels reach the display as
  raw values — as Winamp's did on Windows — which can look slightly more
  saturated on a wide-gamut display. One line to revert if unwanted.

- **feat(macos): media keys and Now Playing** — Mime Čuvalo, with Claude Opus 5.5 (1M context)

  Play/pause, next, previous, stop and scrubbing from the keyboard's media
  keys, Control Centre, headphones and Bluetooth remotes, via
  MPRemoteCommandCenter — not an event tap, which would need Accessibility
  permission. Handlers queue a command and wake the event loop through its
  proxy. Now Playing shows title, artist, length and position, republished
  only when the track, play state or position jumps; the system
  extrapolates the clock in between. Not yet confirmed from an unbundled
  binary: macOS may only route media keys to a bundled app (M7).

- **feat: windowshade mode for all three windows** — Mime Čuvalo, with Claude Opus 5.5 (1M context)

  Each window rolls up to a 14px strip from its shade button or a double-
  click on its title bar, and back. The main window's strip has the mini
  time (blinking when paused), a ten-bar mini analyser in viscolor rows
  4/8/11/14/17, the mini position slider with its left/centre/right thumb
  frames, and hit zones over the transport painted into the background.
  The EQ's strip has mini volume and balance sliders (live with the main
  window's). The playlist's shows the current title and length, and
  resizes sideways only. The main window uses region.txt's [WindowShade]
  polygons while shaded; docked windows follow the height change; shade
  state is saved.

- **feat: native menus, EQ presets (.eqf), and a macOS menu bar** — Mime Čuvalo, with Claude Opus 5.5 (1M context)

  The options button, the O clutter letter and a right-click open the
  main menu (play file/folder, load skin, playback with shuffle/repeat,
  options with always-on-top/double size/time remaining/analyser,
  windows, about, exit); a right-click over the playlist opens its own.
  The playlist's SORT and MISC OPTS open sort (title, file name, path,
  reverse, randomize) and misc menus; the EQ's PRESETS opens load, save
  and reset. Menus are native, via muda — Winamp's were native too —
  and rebuilt on each open so check marks are current.

  .eqf presets (single files and libraries) read and write per the layout
  in Webamp's winamp-eqf (ISC). No Winamp preset values are bundled.

  A macOS menu bar replaces winit's default, whose Quit skipped saving;
  quitting from anywhere now goes through a flag the loop honours. The I
  and V clutter letters show file info and toggle the analyser.

- **feat: the playlist's skinned ADD/REM/SEL/MISC/LIST menus** — Mime Čuvalo, with Claude Opus 5.5 (1M context)

  Each bottom-bar button opens its column of pledit.bmp buttons, standing
  on the button in Webamp's order, with the side bar and a highlight for
  the item under the pointer. Items: add file/folder (URL reports that
  streams are unsupported); remove dead files, all, crop, selected;
  invert, none, all; sort, file info (a native message box), options;
  new, save and load list. The playlist model gains remove_missing,
  stable sort, reverse and randomize, each keeping the current entry.

- **feat: native file dialogs — eject, add files, skins, playlists** — Mime Čuvalo, with Claude Opus 5.5 (1M context)

  Eject (main window, playlist mini transport, or L) opens files and plays
  them, replacing the playlist, as Winamp's does; the playlist's ADD button
  adds files. Dialog actions for adding a folder, loading and saving a
  playlist and loading a skin are in place for the menus. Skins switch
  live: sheets, colours, playlist font and window shape together, with
  the region mask cleared and the window made opaque for rectangular
  skins. Uses rfd, which on macOS shares the objc2 generation already in
  the build.

- **feat: persistence — settings, playlist and window layout survive restarts** — Mime Čuvalo, with Claude Opus 5.5 (1M context)

  settings.ini (plain key = value, forgiving: a damaged key keeps its
  default) and playlist.m3u8 in the platform's config directory, written
  on quit and every ten seconds if anything changed. Kept: volume,
  balance, EQ, shuffle/repeat, time mode, analyser, double size, always
  on top, skin, each window's position/visibility/docking, playlist size
  and current entry. Files on the command line replace the saved playlist
  and play; without them it comes back stopped. Windows are created
  hidden and shown once placed, so nothing flashes at a default spot.

  The M3U8 writer carries cue segments in a #WAMPSEGMENT line other
  players ignore. Paths are made absolute on entry, since the playlist is
  reloaded from elsewhere. Adds the NOTICE file docs/legal.md called for
  once code was first ported from Webamp.

### 2026-09-27

- **docs: M5 done — fourth crate, playlist font, docking** — Mime Čuvalo, with Claude Opus 5.5 (1M context)

- **feat: playlist window and playlist-driven playback** — Mime Čuvalo, with Claude Opus 5.5 (1M context)

  The playlist window lists tracks in the skin's pledit.txt font (a system
  TrueType font via fontdue, rasterised at physical resolution, with
  fallbacks), in its normal/current/normalbg/selectedbg colours, with
  ellipsised titles, right-aligned durations, a draggable scroll handle,
  the running-time and mini-time displays, and a resize grip that snaps
  to the frame's 25x29 grid. Click, Shift-click and Cmd-click select;
  double-click or Enter plays; Delete removes; arrows move; the wheel
  scrolls; the mini transport works.

  Playback now runs through the playlist: files, folders (recursively),
  M3U/PLS playlists and cue sheets from the command line or dropped on any
  window. Previous/next (buttons and Z/B), shuffle and repeat drive what
  the engine queues next, so gapless follows the playlist order and is
  re-queued whenever the playlist or a toggle changes. Tags load on a
  background thread and fill in titles and durations as they arrive. The
  marquee shows the playlist position: "3. Artist - Title (3:45)".

- **feat: equaliser and playlist windows, docked under the main window** — Mime Čuvalo, with Claude Opus 5.5 (1M context)

  The app moves out of main.rs into app/: one Pane per window (winit
  window, softbuffer surface, widget table), events routed by WindowId.
  The EQ and playlist windows open stacked under the main window and
  follow it; dragging one away undocks it, dropping it back near its slot
  re-docks it. The main window's EQ/PL buttons show and hide them; always-
  on-top and minimise apply to all three; double size doubles main and EQ.

  The EQ window is complete: preamp and ten band sliders on the skin's
  prerendered tracks, ON/AUTO toggles, and the response curve as a natural
  cubic spline coloured from eqmain.bmp's strip. Dragging a band updates
  the engine live and narrates in the main marquee ("EQ: 1KHZ: +3.0 DB").

  The playlist window draws its resizable nine-tile frame; its contents
  come next.

- **feat(audio): segment playback for cue tracks; sample-accurate seek** — Mime Čuvalo, with Claude Opus 5.5 (1M context)

  Open and Enqueue now take a Source: a file, or a stretch of one. The
  decoder counts source frames and keeps exactly [start, end), so a cue
  track starts and stops on its frame. Seeking is now sample-accurate too:
  symphonia lands at or before the target and the difference is decoded
  and discarded. When the queued source continues the current one in the
  same file — consecutive cue tracks — the decoder adopts it mid-packet
  without reopening anything, so cue albums are gapless; the seam is
  reported to within 2 frames.

- **feat(playlist): wamp-playlist crate — model, M3U/PLS, cue sheets** — Mime Čuvalo, with Claude Opus 5.5 (1M context)

  A fourth pure crate beside wamp-skin and wamp-audio: the playlist model
  (selection with plain/toggle/extend clicks, current entry that survives
  removals, next/previous with repeat and shuffle, total durations) and
  the formats that fill it. M3U/M3U8 with EXTINF hints, PLS, and cue
  sheets ported from the Swift parser — strict about malformed sheets,
  with basename fallback for foreign FILE paths. Text encoding: BOM, then
  strict UTF-8, then chardetng's guess, which reads the Shift-JIS, CP-1251
  and CP-1252 fixtures correctly.

- **docs: M4 done — seam placement, declick, downmix deferral** — Mime Čuvalo, with Claude Opus 5.5 (1M context)

- **feat(audio): click-free seek, stop and open** — Mime Čuvalo, with Claude Opus 5.5 (1M context)

  A flush now fades the audio already playing out over ~5 ms before
  discarding the ring, and the first audio after it fades in over the
  same span, instead of cutting a waveform mid-cycle. The callback body
  moves into an Output struct so this, underruns and paused flushes are
  unit-tested without an audio device.

- **feat(audio): gapless playback** — Mime Čuvalo, with Claude Opus 5.5 (1M context)

  Command::Enqueue queues the next track; the decoder opens it the moment
  the current file runs dry and keeps filling the same ring. When both
  tracks share a rate and channel count the resampler carries straight
  over, so there is no filter restart at the seam; otherwise the old
  tail is flushed first. The decoder reports where the seam falls in
  frames_played terms (exactly, from the resampler's owed output), and
  the engine holds Event::Advanced back until the audio reaches it.

  A decoder-level test plays two WAV halves of one sine through a
  44.1k -> 48k conversion and checks the output is sample-continuous,
  exactly the right length, and the seam marker within 2 frames. The app
  accepts several files and plays them gaplessly in order, standing in
  for the playlist until M5.

- **feat(audio): 10-band equaliser and preamp in the callback** — Mime Čuvalo, with Claude Opus 5.5 (1M context)

  RBJ-cookbook biquads at Winamp's band centres: one-octave peaking bands
  with low and high shelves at either end, and the preamp as a gain stage
  ahead of them, applied only while the EQ is on. Coefficients are
  computed on the UI thread and reach the callback through an rtrb ring;
  filter state resets on every flush so seeks do not ring. Output is
  clipped to full scale after volume.

- **feat(audio): band-limited resampling with rubato** — Mime Čuvalo, with Claude Opus 5.5 (1M context)

  Replaces M1's linear stopgap with rubato's FFT resampler. Trims the
  filter's startup delay and flushes its tail at end of track, so output
  length is exactly input * ratio — a precondition for gapless seams.
  Tests cover exact length, no leading silence, pitch, and that
  downsampling no longer aliases.

- **docs: M3 done — widget model amendment, frame pacing, present cost** — Mime Čuvalo, with Claude Opus 5.5 (1M context)

- **feat: complete main window** — Mime Čuvalo, with Claude Opus 5.5 (1M context)

  Every classic main-window control now renders and responds, at Webamp's
  coordinates: title bar (focus-aware; close and minimise work), clutter
  bar (always-on-top and double-size work), play-state and work
  indicators, mm:ss time (click for remaining; blinks when paused),
  spectrum analyser (click to switch off), scrolling marquee with
  volume/balance/seek readouts while dragging, kbps and kHz, mono/stereo,
  volume, balance and position sliders, EQ/playlist/shuffle/repeat
  toggles. Winamp's Z X C V B keys and arrow-key seek/volume work too.

  Redraws at 60fps while playing, slower while paused or scrolling, and
  not at all otherwise. The blitter now works row by row with clipping.

- **feat(audio): spectrum analyser** — Mime Čuvalo, with Claude Opus 5.5 (1M context)

  19 bars over a mostly-logarithmic axis with Webamp's default falloff
  and peak behaviour, driven from the tap ring on the UI thread. Physics
  steps at a fixed 60Hz so bar speed does not depend on redraw rate. Each
  bar totals the energy of every bin it spans, so a pure treble tone
  cannot fall between sample points.

- **feat(audio): seek, balance, track info, analyser tap and end-of-track** — Mime Čuvalo, with Claude Opus 5.5 (1M context)

  - Seek flushes the ring through a callback handshake (only the consumer
    can empty an SPSC ring), seeks symphonia, resets decoder and resampler,
    and moves the frame counter to where the reader actually landed.
  - Stop and Open now flush too; previously up to two seconds of the old
    track could play after either.
  - Balance attenuates the far channel in the callback.
  - The callback copies a pre-volume mono mix into a tap ring for the
    analyser.
  - An event channel reports Opened(TrackInfo), OpenFailed and Ended; the
    engine stops itself once the ring drains at end of file.

### 2026-09-26

- **feat(macos): shaped windows from region.txt via a root-layer mask** — Mime Čuvalo, with Claude Opus 5.5 (1M context)

  Closes the M2 region investigation: softbuffer's CoreGraphics backend
  ignores alpha, but it draws into a sublayer, so a mask on the view's root
  layer clips it. One CAShapeLayer per polygon gives union semantics. Uses
  only objc2 crates already in the build; unsafe stays in platform/macos.rs.
  Not yet verified on screen. Also resolves the colorkey open item (Webamp
  applies none) and marks M2 done.

- **test(skin): committed original-artwork fixture skin so CI covers parsing** — Mime Čuvalo, with Claude Opus 5.5 (1M context)

  A generator example draws a small skin (bevelled panels, tinted per
  sheet) containing the parser's known corner cases: nested and uppercase
  paths, a PNG named .bmp, nums_ex.bmp, CRLF text files with comments and
  malformed colours, and a real region.txt. The render test now always runs
  against it. Anchors the skins/ ignore rule to the repo root so the fixture
  directory is not swallowed by it.

- **feat(skin): parse viscolor.txt, pledit.txt and region.txt** — Mime Čuvalo, with Claude Opus 5.5 (1M context)

  Winamp's forgiving INI dialect, the viscolor line format with its many
  annotation styles, pledit colour normalisation (optional '#', overlong
  values), and region polygons for all four window sections. Replaces the
  deliberately-wrong placeholder palette with Webamp's default table.

- **feat(skin): full sprite table, text.bmp font and eq_ex sheet** — Mime Čuvalo, with Claude Opus 5.5 (1M context)

  Ports every classic-skin sprite Webamp models (titlebar, cbuttons,
  numbers/nums_ex, playpaus, monoster, posbar, volume, balance, shufrep,
  eqmain, eq_ex, pledit, text) into resolve(). Adds the text.bmp glyph
  lookup, Skin::resolve for nums_ex's differently-placed minus signs, and
  all_keys()/standard_size() so every key is checked against its sheet.

- **chore: track latest stable Rust instead of pinning a toolchain** — Mime Čuvalo, with Claude Opus 5.5 (1M context)

  rust-toolchain.toml pinned 1.98.1, which made rustup keep a second toolchain
  on disk alongside stable and silently auto-install it whenever any rustup or
  cargo command ran inside the repo. Removed, along with the `rust-version`
  MSRV field, which would only go stale under a latest-stable policy.

  CI now runs `rustup update stable` rather than relying on the pin, and the
  cache key no longer hashes the deleted file.

### 2026-09-22

- **feat: M1 — end-to-end vertical slice** — Mime Čuvalo, with Claude Opus 5 (1M context)

  Parses a .wsz, renders the classic main window from its sprites, decodes
  audio through Symphonia into a lock-free ring feeding a cpal callback, and
  wires the transport buttons to it. The point of this milestone was to prove
  every risky seam at once while each is still shallow, and all of them hold.

  Two open questions from the design doc are now answered by evidence rather
  than assumption:

  - **BI_RLE8 decodes fine.** base-2.91's main.bmp is 8bpp run-length encoded,
    exactly the case flagged as risk #2, and `image` 0.25 handles it — as it
    does every sheet in all four acceptance skins, including the mixed-case and
    all-uppercase ones. No fallback BMP reader needed.
  - **Metadata readers are not optional.** Every real MP3 failed to open with
    "no suitable format reader found" despite the mp3 codec being enabled:
    symphonia's probe cannot find the MP3 sync behind an ID3v2 tag unless the
    `id3v2` *metadata* reader is registered too. Enabling a codec is not enough.
    Now recorded as risk #6 so it is not rediscovered.

  Verification, given that neither the audio nor the window can be checked by
  the usual means here:

  - The headless `play` example compares the engine's own position against wall
    clock. Drift is a constant -25ms — output buffer latency, not accumulating —
    which is what proves 44.1k content is reaching a 48k device at the right
    rate. Pause holds position; resume advances.
  - `main_window_renders_to_a_buffer` draws the window to a plain buffer with no
    display involved, asserts the background covers it and that drawing the
    buttons actually changes pixels, and with WAMP_RENDER_DUMP set writes a PNG
    for inspection by eye. The rendered window is correct: background, visualiser
    well, kbps/kHz labels, and all six transport buttons at their right
    positions.

  Deliberately stopgapped, each marked in code:

  - Resampling is linear interpolation, not rubato. Acceptable while upsampling
    44.1k to 48k, where the artefact is mild HF rolloff; it would alias on
    downsampling and must not ship that way. rubato at M4.
  - viscolor defaults are an obviously-wrong green ramp rather than Winamp's real
    table, so they cannot ship unnoticed. Real values with viscolor.txt at M2.
  - Previous/next/eject log and do nothing; they need a playlist (M5).

  Known gap: tests needing real skins read from the gitignored skins/ directory
  and therefore SKIP in CI rather than fail. Skin parsing is currently only
  verified locally. M2 should commit a small original-artwork skin as a fixture,
  which docs/legal.md requires building anyway.

  Release binary is 2.2 MB.

- **feat: M0 — Rust workspace skeleton and CI** — Mime Čuvalo, with Claude Opus 5 (1M context)

  Three units per the architecture doc: `wamp-skin` (pure parser, bytes in,
  Skin out), `wamp-audio` (headless engine), and the `wamp` binary holding the
  UI and platform shell. `wamp-ui` is deliberately not split — the widget table
  is coupled to the application, so a fourth crate would buy indirection rather
  than testability.

  Dependencies resolved against crates.io rather than guessed, which settles an
  open question from the design doc: winit is 0.30.13, so `ApplicationHandler`
  is the API. Also notable: symphonia is 0.6, newer than the 0.5 line the
  research assumed. `Cargo.lock` is committed.

  No logic yet — bodies are `todo!()` tagged with the milestone that fills them.
  What is here is the shape the design argues for, plus the decisions that are
  easy to lose and expensive to rediscover, encoded where they will be read:

  - `eq.rs` carries the band frequencies as data with a test asserting they are
    Winamp's (70..16000) and not the ISO octave series a fork substituted, plus
    `band_shape()` fixing shelves on the outer bands. Both were real bugs found
    and fixed in Swift; both are easy to reintroduce from first principles.
  - `archive.rs` documents the zip-slip and zip-bomb properties as invariants
    and already implements `basename()`, the function the zip-slip defence rests
    on, with tests for traversal and Windows separators.
  - `blit.rs` records why there is no dirty-rect tracking and why scaling is
    nearest-neighbour only.
  - `geometry.rs` holds the fixed classic dimensions.

  Toolchain pinned to 1.98.1 via rust-toolchain.toml. Workspace lints deny
  unsafe_code across all three units — reasonable for a program whose risky
  input is untrusted archives and whose dependencies handle the FFI.

  CI runs fmt, clippy -D warnings, test and a release build on macOS. It uses
  only first-party actions, including actions/cache rather than the usual
  third-party rust cache, preserving a property the previous workflow had: no
  arbitrary third-party code with access to the build.

  Verified locally: all four steps green, 6 tests pass, release binary builds.

### 2026-09-21

- **docs: Rust architecture design** — Mime Čuvalo, with Claude Opus 5 (1M context)

  Locks the three decisions that would be expensive to reverse later:

  - **Pixel-exact Winamp 2.9 geometry.** 275x116 main and EQ, 275x232 playlist.
    Skins supply pixels; we never move a control. This reverses the Swift
    version, which kept its own layout (mainPlayerHeight 126, scale 1.3) and
    adapted skins to it — the reason skins looked subtly wrong there. It is also
    the only way "25,000 museum skins just work" is a true statement.
  - **Portable core, macOS shell only.** winit + softbuffer + cpal + Symphonia
    throughout; build and ship macOS alone for now; mac-only APIs behind cfg.
  - **Vertical slice first.** M1 proves winit&lt;->softbuffer, Symphonia&lt;->cpal and
    sprite blitting end to end before any of the three is deep.

  Two structural simplifications worth naming, both following from the fact that
  the surface is 31,900 pixels:

  - No dirty-rect tracking. Full repaint every frame is ~7.6 MB/s of memcpy,
    which deletes an entire class of invalidation bugs the Swift version fought.
  - No GUI framework. A widget is a rect, a sprite key and a state enum; hit
    testing is a linear scan over ~25 of them.

  The threading model is specified in full because it is expensive to retrofit:
  decoder thread -> lock-free SPSC ring -> realtime audio callback, with EQ
  coefficients crossing into the callback via a second ring rather than a mutex,
  and the FFT running on the UI thread rather than in the callback.

  Carries forward two EQ corrections already validated in Swift (shelf filters on
  the outer bands; preamp as a gain stage rather than a volume multiplier) and
  keeps Winamp's authentic band frequencies rather than the ISO octave series a
  fork had substituted.

  Also carries forward the Swift ZIP hardening as a requirement, not an
  aspiration: in-memory extraction keyed by lowercased basename makes zip-slip
  structurally impossible, and bomb caps check both the declared header size and
  the streamed byte count.

  Known risk recorded rather than glossed: region.txt shaped windows may not be
  achievable portably through winit/softbuffer, with a macOS objc2 fallback.
  Scheduled for investigation at M2 so it does not surface at M5.

