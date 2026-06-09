# TermKrush

[![CI](https://github.com/mreider/termkrush/actions/workflows/ci.yml/badge.svg)](https://github.com/mreider/termkrush/actions/workflows/ci.yml)
[![codecov](https://codecov.io/gh/mreider/termkrush/branch/main/graph/badge.svg)](https://codecov.io/gh/mreider/termkrush)

A mouse-first **desktop scratch/loop mixer** for people who can't DJ. Load
audio into **clips**, let the software lock every loop to one tempo
automatically, lay down old-school **whip/wiki** scratches by dragging a
platter, arrange it on a **free-track timeline**, and render the mix to a file.
No decks, no turntable skills required.

**Site:** https://mreider.github.io/termkrush

## The idea

Make a good old-school-scratch mix without DJ chops or hardware:

- **Clips, not decks.** Drag a track from the library onto a **clip** as a
  one-shot, a **loop** (repeats, auto-synced to the master tempo), or a
  **scratch** source. Trim it non-destructively in the clip editor.
- **Timing is automatic.** The **MASTER** timeline track sets the tempo; every
  other clip **varispeeds** to it (pitch rides, turntable-style), so beats
  always land together. A per-clip phase nudge (**on-beat / bar / off-beat /
  free**) aligns the actual hit to the grid.
- **Old-school scratching, modeled.** A bottom **platter**: drag a clip onto it,
  then drag the platter (or use ←/→) to *whip* (backward) and *wiki* (forward);
  a held-still platter is silent, like a real one.
- **Arrange & render.** Drag clips onto free timeline tracks, move/copy/paste the
  blocks, scrub the playhead, and render the whole thing to a WAV in your library.
- **CRT identity.** Amber/green on near-black, matching the site.

## Install

```sh
cargo run --release          # launches the desktop app
```

Set `crate_root` (see [Configuration](#configuration)) so your tracks show up.
On Linux you'll need ALSA headers (`libasound2-dev`) and the usual desktop/GL
libraries for a windowed app. Rust 1.75+ (MSRV); the toolchain pin lives in
`rust-toolchain.toml`. For repeated runs during development, `scripts/dev-run.sh`
builds once and reuses the binary (`scripts/dev-run.sh dev` rebuilds then runs).

## Using it

The window is one screen, no modes: a **library** on the left, a **clip grid**
in the middle (which swaps to the clip editor while trimming), the **timeline**
across the top, and the **scratch platter** along the bottom. It's drag-first:

- **Library → clip:** drag a track onto a clip cell to load it (background
  decode). Double-click a track to rename; drag into a folder to move (hold over
  a folder to spring it open); drag onto the trash to delete; click ▶ to preview.
- **Clip → timeline:** drag a clip (by its name) onto a timeline lane. Track 0 is
  the **MASTER** lane and sets the tempo. Click a block to select, drag to move
  (snaps to the beat), `Delete` to remove, `Cmd/Ctrl-C/V` to copy/paste; the
  corner badge cycles the phase (B/R/O/F).
- **Transport:** play/pause, stop, render-to-WAV, add a track, zoom, and scroll
  live in the timeline's top row; click/drag the ruler to scrub.

## Configuration

TermKrush reads an optional config file at
`~/.config/termkrush/config.toml` (or `$XDG_CONFIG_HOME/termkrush/config.toml`).
Every key is optional; missing keys fall back to defaults.

```toml
# Root directory scanned (recursively) for the local library.
# Default: ~/Music/termkrush
crate_root = "~/Music/termkrush"
```

The library shows the audio files under `crate_root` at launch and is
filesystem-managed (drop files in, move them around). A leading `~/` expands to
your home directory.

## Layout

A two-crate workspace keeps the engine UI-free and directly testable:

| Crate / module   | Responsibility                                          |
|------------------|---------------------------------------------------------|
| `termkrush-core` | **headless engine** (no UI dep): audio decode/resample/BPM/stretch, clips, master bus + sampler/scratch/jog voices, the free-track `arrangement`, library, config |
| `termkrush` (bin)| the thin **UI shell**: the egui/eframe desktop app, input mapping, and the audio pump |

## Support

If TermKrush is useful to you: https://buymeacoffee.com/mreider ☕

## License

MIT © 2026 Matt Reider — see [LICENSE](LICENSE).
