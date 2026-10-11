# Legal & provenance

Orientation, not legal advice. These are the constraints the project is built
under; they exist to keep provenance clean so the question never has to be
argued.

## 1. The hard rule: never read the Winamp source

Llama Group published the Winamp source in September 2024 under the "Winamp
Collaborative License" 1.0/1.0.1 — which forbade distributing modified
versions, and so is not open source in any useful sense. It was pulled shortly
after, when it turned out to contain Dolby, Fraunhofer and Microsoft code that
Llama had no right to redistribute. Copies still circulate.

**Do not look for it, read it, or let a coding agent fetch it.**

Everything below rests on clean-room provenance: this project derives from
public format documentation and from Webamp (MIT). That defense is complete
right up until someone can show access to the original source.

## 2. Approved sources

| Source | License | Use |
|---|---|---|
| [Webamp](https://github.com/captbaritone/webamp) | MIT | Sprite coordinates, skin parsing behavior, bitmap font layout, stock EQ preset values |
| Community `.wsz` format write-ups | public | Format structure |
| Observation of skin files themselves | n/a | Corner cases |

Anything ported from Webamp keeps its MIT notice. The `NOTICE` file at the
repository root credits captbaritone/webamp and its ISC-licensed
`winamp-eqf`, and lists what each was used for. Update it when something new
is ported.

## 3. What is safe

- **The format.** `.wsz` is a renamed ZIP. File formats aren't copyrightable;
  reading one for interoperability is settled ground.
- **Sprite coordinates.** "`cbuttons.bmp` at (0,0,23,18) is the play button" is
  a functional fact required for compatibility, not creative expression.
- **Layout and behavior.** A 275x116 window with a transport bar, LCD and
  spectrum analyzer. Trade dress protection over software look-and-feel is weak,
  and functional arrangement is largely unprotectable.

## 4. What is not ours

- **The stock Winamp skin artwork.** The default 2.9x bitmaps are copyrighted
  artwork owned by Llama Group. A pixel-faithful recreation is a derivative work
  whether drawn in a paint program or in code. The Swift version carried a
  hand-coded recreation of `base-2.91` (`Skinning/ClassicSprites/`); it is
  deliberately **not** carried forward. **The built-in default skin must be
  original artwork** — same geometry (functional), different pixels.
- **Third-party skins.** The ~25,000 skins on skins.webamp.org are individual
  artists' copyrighted work. *Loading* a skin the user supplies is fine — that is
  what any image viewer does. *Bundling* skins in this repo is redistribution.
  `skins/` is therefore gitignored: keep them locally for parser development,
  never commit them.

## 5. Trademark

Separate from copyright, and actively enforced since the winamp.com relaunch.
Avoid:

- The name **Winamp** in the product name, bundle ID, domain or store listing
- The **lightning bolt** logo
- **"It really whips the llama's ass"** and the llama iconography

Descriptive/nominative use is fine and necessary: "plays classic Winamp 2.x
skins", "a Winamp-compatible player".

**Decided 2026-10-11:** the project is **Lantern**. The inherited name, `Wamp`,
was close enough to Winamp to draw a trademark lawyer's eye; Lantern shares
nothing with it. Clearance of the new name (existing apps, marks, domains) is
still to do before a public release.

## 6. Codec patents

New obligation in the Rust rewrite. The Swift version called AVFoundation, so
Apple held the codec licenses and we inherited them. Shipping our own decoders
(Symphonia) makes this ours.

| Codec | Status |
|---|---|
| MP3 | **Clear.** Fraunhofer terminated licensing in April 2017; patents expired. |
| FLAC, Vorbis, Opus | **Clear by design.** Royalty-free; Opus carries an explicit multi-party patent pledge. |
| ALAC | **Clear.** Apache 2.0 with express patent grant (Apple, 2011). |
| AAC | **Murky.** Core AAC-LC patents from the late 90s have largely expired; HE-AAC's SBR/PS layer is younger. In practice ffmpeg ships AAC decoding everywhere and free decoders are not pursued. Reassess before charging money or shipping to the App Store. |

Decoding is low-risk; encoding is where patent exposure is real. We only decode.

## 7. Precedent

Webamp has existed since 2014: a complete, pixel-faithful, clean-room
reimplementation of the Winamp 2 UI, MIT-licensed, highly visible, hosting an
archive of tens of thousands of skins. It has never been sued, and Llama Group
has engaged with it cordially. A native player in the same tradition is a
smaller target.

## 8. Checklist

- [ ] Never read the WCL Winamp source
- [ ] Built-in default skin is original artwork
- [ ] `skins/` stays gitignored
- [x] `NOTICE` credits Webamp (and winamp-eqf) — added at M6; it should have
  landed with the M2 sprite port
- [ ] Pick a name with more daylight from the Winamp trademark
