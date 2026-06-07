# TermKrush

[![CI](https://github.com/mreider/termkrush/actions/workflows/ci.yml/badge.svg)](https://github.com/mreider/termkrush/actions/workflows/ci.yml)
[![codecov](https://codecov.io/gh/mreider/termkrush/branch/main/graph/badge.svg)](https://codecov.io/gh/mreider/termkrush)

A keyboard-first terminal **scratch/loop mixer**. Everything is a **pad**: load
tracks into pads, let the software lock every loop to one tempo automatically,
lay down old-school **whip/wiki** scratches, arrange it all on a tracker-style
step grid, and render the mix to a file — all from your shell. No decks, no
turntable skills required.

**Site:** https://mreider.github.io/termkrush

> **Status: mid-rebuild.** TermKrush pivoted (2026-06-07) from a two-deck DJ
> model to this pad-based scratch/loop mixer — see `.am/inception.md`. The
> **foundation is in** (headless engine + pads-only TUI: a track list, a 7-pad
> bank, load/trigger/trim, live-mix record, master volume). The pad types
> (loop/scratch/one-shot), automatic loop **BPM sync**, the **whip/wiki**
> scratch model, the **step-grid timeline**, and **render/export** are being
> built one story at a time from the top of the backlog (`termkrush/`). Working
> rules are in `CLAUDE.md`.

## The idea

Make a good old-school-scratch mix without DJ chops or hardware:

- **Everything is a pad.** Load a track into a pad as a **loop** (repeats,
  auto-synced to the master tempo), a **scratch** pad (a short clip the software
  finds the scratch point in, played with **whip/wiki** rubs), or a **one-shot**.
- **Timing is automatic.** The first loop sets the master tempo; every other
  loop varispeeds to it, so beats always land together — fire a pad and it's
  never out of time.
- **Old-school scratching, modeled.** *whip* = backward rub with the forward
  motion muted; *wiki* = forward rub that sounds; chain them into phrases
  ("whip whip wiki-whip").
- **Per-pad volume**, activate/deactivate with a hard cut or soft fade, pads
  stack — no crossfader.
- **Arrange & render.** Place pads on a tempo-locked step grid and render the
  result to a track (WAV native; MP3 import + export). Reload a render onto a
  pad to trim/re-tempo and save back.

## Screenshot

The screen is a uniform grid: the track list on the left, a 7-pad bank, and the
DJ tile.

```
                              TermKrush
        tab focus  j play  l load  a/d/w/s trim  r record  ? help
┌Crate  (3 tracks)───────────┐┌▸ Pad 1──────────┐┌Pad 2────────────┐
│▶ Teo Laza - Doing Too Much ││  ●              ││  ·              │
│  Yarin Primak - Trippin    ││  [░░██████░░]   ││  -- bpm         │
│  Lazerpunk - Hyperdrive    │└─────────────────┘└─────────────────┘
│                            │┌Pad 3────────────┐┌Pad 4────────────┐
│                            ││  ●              ││  ·              │
│                            ││  126 bpm        ││  -- bpm         │
│                            │└─────────────────┘└─────────────────┘
│                            │┌Pad 5────────────┐┌Pad 6────────────┐
│                            ││  ·              ││  ·              │
│                            │└─────────────────┘└─────────────────┘
│                            │┌Pad 7────────────┐┌DJ───────────────┐
│                            ││  ·              ││  =^.^=          │
└────────────────────────────┘└─────────────────┘└─────────────────┘
```

## Install

```sh
cargo run --release          # launches the TUI
```

Set `crate_root` (see [Configuration](#configuration)) so your tracks show up.
On Linux you'll need ALSA headers (`libasound2-dev`). Rust 1.75+ (MSRV); the
toolchain pin lives in `rust-toolchain.toml`. For repeated runs during
development, `scripts/dev-run.sh` builds once and reuses the binary.

## Keyboard cheatsheet

The model is **focus → act**: pick a cell (the track list, a pad, the DJ) with
`tab`/arrows, then a small fixed cluster of keys acts on it.

> The control surface grows with the pad-type and timeline stories; this is the
> current foundation. (The Xbox controller returns in the controls epic.)

| Key            | Action                                            |
|----------------|---------------------------------------------------|
| `tab` / `↑↓←→` | move focus across cells; on the track list, `↑`/`↓` browse |
| `/`            | filter the track list; `enter` loads the highlight onto the focused pad |
| `l`            | load the highlighted track onto the focused pad   |
| `j` / `1`–`7`  | trigger a pad                                     |
| `a` / `d`      | trim the focused pad's in-point ∓ (`shift` = fine) |
| `w` / `s`      | trim the focused pad's out-point ± (`shift` = fine) |
| `,` / `.`      | nudge the focused pad's BPM                        |
| `k`            | assign the latest recording to the focused pad    |
| `r`            | record (resample) the live mix → focused pad, else the stash |
| `[` / `]`      | master volume down / up                           |
| `z`            | hide / show the track list                        |
| `\`            | load the demo track (or `$TERMKRUSH_DEMO_TRACK`)  |
| `?`            | toggle help                                       |
| `esc` / `q`    | quit (confirm `y`/`n`); `C-c` force-quits         |

## Configuration

TermKrush reads an optional config file at
`~/.config/termkrush/config.toml` (or `$XDG_CONFIG_HOME/termkrush/config.toml`).
Every key is optional; missing keys fall back to defaults.

```toml
# Root directory scanned (recursively) for the local track list.
# Default: ~/Music/termkrush
crate_root = "~/Music/termkrush"
```

The track list shows the audio files under `crate_root` at launch; the library
is filesystem-managed (drop files in, move them around). A leading `~/` expands
to your home directory.

## Layout

A two-crate workspace keeps the engine UI-free and directly testable:

| Crate / module   | Responsibility                                          |
|------------------|---------------------------------------------------------|
| `termkrush-core` | **headless engine** (no UI dep): audio decode/resample/BPM/stretch, clip, master bus + sampler pads, library, config |
| `termkrush` (bin)| the thin **TUI shell**: ratatui/crossterm grid, input mapping, the audio pump |

## Support

If TermKrush is useful to you: https://buymeacoffee.com/mreider ☕

## License

MIT © 2026 Matt Reider — see [LICENSE](LICENSE).
