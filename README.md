# TermKrush

[![CI](https://github.com/mreider/termkrush/actions/workflows/ci.yml/badge.svg)](https://github.com/mreider/termkrush/actions/workflows/ci.yml)
[![codecov](https://codecov.io/gh/mreider/termkrush/branch/main/graph/badge.svg)](https://codecov.io/gh/mreider/termkrush)

A keyboard-first terminal DJ application. Pull tracks locally, beat-match and
crossfade up to four decks, keyboard-jog scratch over marked zones, add minimal
FX (filter, echo, reverb), and record the master to wav/mp3 — all from your
shell.

**Site:** https://mreider.github.io/termkrush

> Status: early foundation, built one story at a time from the top of the
> backlog. The backlog lives under `termkrush/`; the working rules are in
> `CLAUDE.md`.

## Screenshot

_Placeholder — a TUI capture lands once the deck and mixer views exist._

```
┌─ TermKrush ─────────────────────────────────────────────┐
│  (decks, crossfader, and library browser render here)    │
└──────────────────────────────────────────────────────────┘
```

## Install

_Placeholder until the first release publishes binaries (v0.1.0 "spins")._

Until then, build from source:

```sh
cargo build --release
cargo run            # prints the version banner today
```

Rust 1.75+ (MSRV). The toolchain pin lives in `rust-toolchain.toml`.

## Keyboard cheatsheet

The model is **focus → act**: pick a target (Deck A · Deck B · Clips), then a
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
| `l` / `;`      | on a **deck**: mark-in / mark-out → records that region as a clip; on a **pad**: `l` assigns the highlighted crate track |
| `w` / `s`      | value — deck volume · clips pick slot    |
| `a` / `d`      | jog/scrub the focused deck (`shift` = coarse) |
| `g` / `h`      | hard-cut the mix to deck A / deck B (instant) |
| `G` / `H`      | auto-fade to deck A / deck B over the set duration |
| `space`        | cycle the auto-fade duration (1 / 2 / 4 / 8 s) |
| `1`–`7`        | trigger clip pads directly              |
| `r`            | resample the live mix (arm/disarm) → a clip on the focused pad, else the recordings stash |
| `,` / `.`      | tempo: varispeed the focused **deck** ∓1% (`shift` = ∓0.1%) — pitch rides with speed, effective BPM = base × speed; on a **pad**, nudge its stored BPM |
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
| `src/audio`   | output device, decoding, resampling, realtime path      |
| `src/deck`    | a single track: transport, pitch, cue/loop, scratch     |
| `src/mix`     | crossfader, sync, master bus, FX                        |
| `src/tui`     | ratatui/crossterm interface and key handling            |
| `src/library` | local track storage, downloads, metadata                |
| `src/config`  | user configuration and key bindings                     |

## Support

If TermKrush is useful to you: https://buymeacoffee.com/mreider ☕

## License

MIT © 2026 Matt Reider — see [LICENSE](LICENSE).
