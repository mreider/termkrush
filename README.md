# TermKrush

[![CI](https://github.com/mreider/termkrush/actions/workflows/ci.yml/badge.svg)](https://github.com/mreider/termkrush/actions/workflows/ci.yml)
[![codecov](https://codecov.io/gh/mreider/termkrush/branch/main/graph/badge.svg)](https://codecov.io/gh/mreider/termkrush)

A keyboard-first (and **Xbox-first**) terminal DJ app. Two decks with
auto-fade and beat-sync, a seven-pad clip sampler for live recording,
scratching, and beat-matched playback, and an 8-bit DJ cat that bobs to the
beat — all from your shell.

**Site:** https://mreider.github.io/termkrush

> Status: built one story at a time from the top of the backlog. The two-deck
> mix, the clip/scratch sampler, and the keyboard + controller layer are all
> in; release packaging is the current work. The backlog lives under
> `termkrush/`; the working rules are in `CLAUDE.md`.

## What it does

- **Two decks** — load from a local crate, play/pause/cue, jog/scrub, varispeed
  (pitch rides), set a hot cue, and **sync** one deck's tempo to the other.
- **Transitions** — instant hard-cut A↔B, or a hands-free **auto-fade** over a
  set number of seconds *or* bars (synced to the beat).
- **Clip sampler (7 pads)** — fill a pad by recording a region off a deck,
  resampling the live mix, or grabbing a crate track; then trigger it with a
  **pattern**: straight, cut, baby-scratch, transformer, stutter, warble, or
  reverse. Trim any clip on a timeline (non-destructive), and optionally
  **beat-match** it to the active deck (pitch-preserving time-stretch).
- **Focus → act controls** — one small key cluster acts on whatever cell you've
  focused (a deck, the mixer, a pad). Mirrored 1:1 onto an Xbox controller, with
  the right stick as a jog/scratch platter.
- **BPM** — detected on load (cached), shown per deck, manually nudgeable.

## Screenshot

```
                                            TermKrush
                tab focus   j play  k cue   a/d jog   g/h cut  G/H fade   ? help
┌Crate  (3 tracks)─────────────┐┌▸ Deck A  126 BPM─────────────┐┌Deck B  174 BPM───────────────┐
│▶ Teo Laza - Doing Too Much.m…││  ▶ Teo Laza - Doing Too Much ││  ⏏ Yarin Primak - Trippin  lo│
│  Yarin Primak - Trippin.mp3  ││  [░░░░░░]  00:00.0 / 02:39.0 ││  [░░░░░░]  00:00.0 / 02:20.0 │
│  Lazerpunk - Hyperdrive.mp3  ││                              ││                              │
│                              │└──────────────────────────────┘└──────────────────────────────┘
│                              │┌Mix · soft────────────────────┐┌Mix · hard────────────────────┐
│                              ││A + B                         ││A + B                         │
│                              ││auto-fade 1s                  ││master 1.00  +0.0 dB          │
│                              │└──────────────────────────────┘└──────────────────────────────┘
│                              │┌Pad 1─────────────────────────┐┌Pad 2─────────────────────────┐
│                              ││  ● play                      ││  ·                           │
│                              ││  126 bpm                     ││  -- bpm                      │
│                              │└──────────────────────────────┘└──────────────────────────────┘
│                              │┌Pad 3 … Pad 7 ────────────────┐┌DJ────────────────────────────┐
│                              ││  ● play                      ││  =^.^=                       │
│                              ││                              ││  ♫ dj ♫                      │
└──────────────────────────────┘└──────────────────────────────┘└──────────────────────────────┘
```

The screen is a uniform grid of equal cells: the crate browser on the left,
then decks, the soft/hard mixer, the seven clip pads, and the DJ.

## Install

_Placeholder until the first release publishes binaries (v0.1.0 "spins")._

Until then, build from source:

```sh
cargo run --release          # launches the TUI
```

Set `crate_root` (see [Configuration](#configuration)) so your tracks show up,
or press `\` for the bundled demo. On Linux you'll need ALSA + udev headers
(`libasound2-dev libudev-dev`). Rust 1.75+ (MSRV); the toolchain pin lives in
`rust-toolchain.toml`. For repeated runs during development, `scripts/dev-run.sh`
builds once and reuses the binary.

## Keyboard cheatsheet

The model is **focus → act**: pick any cell (a deck, the mixer, a pad), then a
small fixed cluster of keys acts on it — meaning set by what's focused. Fewer
keys, no per-deck duplication, and a 1:1 shape with the gamepad. (Full map is
in the in-app `?` help; an Xbox controller is the preferred input.)

**Xbox controller** (plug it in — picked up automatically; keyboard still
works): `LB`/`RB` focus Deck A/B · D-pad moves the focus box · `A`/`B`/`X`/`Y`
are the action cluster (play / cue / mark·assign / alt) · `LT`/`RT` auto-fade
toward A/B · **right stick** = continuous crossfade, **left stick** =
jog/scratch the focused deck · `Start` quit · `Back` help.

| Key            | Action (on the **focused** target)      |
|----------------|-----------------------------------------|
| `tab`          | step focus through every grid cell (decks · mixer · pads · DJ · crate) |
| `↑↓←→`         | move the focus box around the grid; on the crate, `↑`/`↓` browse the list |
| `j`            | primary — deck play/pause · clip trigger |
| `k`            | secondary — deck cue/stop · on a **pad**: assign the latest recorded clip |
| `l` / `;`      | **deck**: mark-in / mark-out → records that region as a clip; **pad**: `l` assigns the highlighted crate track, `;` cycles the playback pattern (play · cut · scratch · xform · stutter · warble · reverse) |
| `w` / `s`      | value — deck volume · on a **pad**: trim the out-point ± (`shift` = fine) |
| `a` / `d`      | deck: jog/scrub (`shift` = coarse) · on a **pad**: trim the in-point ∓ (non-destructive) |
| `g` / `h`      | hard-cut the mix to deck A / deck B (instant) |
| `G` / `H`      | auto-fade to deck A / deck B over the set duration |
| `space`        | cycle the auto-fade duration — seconds (1/2/4/8 s) or **bars** (2/4/8/16, synced to the active deck's tempo) |
| `1`–`7`        | trigger clip pads directly              |
| `r`            | resample the live mix (arm/disarm) → a clip on the focused pad, else the recordings stash |
| `b`            | beat-match — on a **deck**: sync its tempo (varispeed) to the other deck's BPM; on a **pad**: toggle auto-BPM (clip stretches to the active deck) |
| `,` / `.`      | tempo: varispeed the focused **deck** ∓1% (`shift` = ∓0.1%) — pitch rides with speed, effective BPM = base × speed; on a **pad**, nudge its stored BPM |
| `c` / `v`      | set / jump to the focused deck's hot cue |
| `[` / `]`      | master volume down / up                 |
| `/`            | filter the crate; `↑`/`↓` pick; `enter` load |
| `\`            | load the demo track (or `$TERMKRUSH_DEMO_TRACK`) |
| `z`            | hide / show the crate panel             |
| `?`            | toggle help                             |
| `esc` / `q`    | quit (confirm `y`/`n`); `C-c` force-quits |

## Configuration

TermKrush reads an optional config file at
`~/.config/termkrush/config.toml` (or `$XDG_CONFIG_HOME/termkrush/config.toml`).
Every key is optional; missing keys fall back to defaults.

```toml
# Root directory scanned (recursively) for the local crate of mp3s.
# Default: ~/Music/termkrush
crate_root = "~/Music/termkrush"
```

The crate browser lists every `*.mp3` under `crate_root` at launch; press
`enter` to load the highlighted track into the deck. A leading `~/` in
`crate_root` expands to your home directory.

## Layout

| Module        | Responsibility                                          |
|---------------|---------------------------------------------------------|
| `src/audio`   | output device, decoding, resampling, pitch-preserving time-stretch, BPM detection |
| `src/deck`    | a single track: transport, varispeed, seek/jog, hot cue, clip capture |
| `src/clip`    | a captured clip (samples + tempo) — the sampler's unit    |
| `src/mix`     | deck blend + auto-fade, master bus, the clip/scratch/pattern sampler |
| `src/tui`     | ratatui/crossterm grid UI, focus→act keymap, Xbox mapping |
| `src/library` | local track crate (scan + filter)                       |
| `src/config`  | user configuration                                      |

## Support

If TermKrush is useful to you: https://buymeacoffee.com/mreider ☕

## License

MIT © 2026 Matt Reider — see [LICENSE](LICENSE).
