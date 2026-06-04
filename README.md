# TermKrush

A keyboard-first terminal DJ application. Pull tracks locally, beat-match and
crossfade up to four decks, keyboard-jog scratch over marked zones, add minimal
FX (filter, echo, reverb), and record the master to wav/mp3 — all from your
shell.

> Status: early foundation. Built one story at a time from the top of the
> backlog. See the backlog under `termkrush/` and the working rules in
> `CLAUDE.md`.

## Build

```sh
cargo build
cargo run        # prints the version banner today
```

Rust 1.75+ (MSRV). The toolchain pin lives in `rust-toolchain.toml`.

## Layout

| Module        | Responsibility                                          |
|---------------|---------------------------------------------------------|
| `src/audio`   | output device, decoding, resampling, realtime path      |
| `src/deck`    | a single track: transport, pitch, cue/loop, scratch     |
| `src/mix`     | crossfader, sync, master bus, FX                        |
| `src/tui`     | ratatui/crossterm interface and key handling            |
| `src/library` | local track storage, downloads, metadata                |
| `src/config`  | user configuration and key bindings                     |

## License

MIT © Matt Reider
