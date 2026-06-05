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

_Fills in further as crossfade, scratch, and FX land._

| Key            | Action                                  |
|----------------|-----------------------------------------|
| `/`            | filter the crate; `Esc` clears          |
| `j` / `k`      | move the crate selection                |
| `enter`        | load the highlighted track              |
| `o`            | load the demo track (or `$TERMKRUSH_DEMO_TRACK`) |
| `space`        | play / pause                            |
| `s`            | stop (rewind to 0)                      |
| `←` / `→`      | seek ±5s (`shift` ±30s)                  |
| `,` / `.`      | scrub ±0.1s                             |
| `+` / `-`      | deck volume                             |
| `<` / `>`      | master volume                           |
| `?`            | toggle help                             |
| `q` / `C-c`    | quit                                    |

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
