# TermKrush

[![CI](https://github.com/mreider/termkrush/actions/workflows/ci.yml/badge.svg)](https://github.com/mreider/termkrush/actions/workflows/ci.yml)
[![codecov](https://codecov.io/gh/mreider/termkrush/branch/main/graph/badge.svg)](https://codecov.io/gh/mreider/termkrush)

A **deterministic auto-mixer** for people who can't DJ. Put your tracks on a
**sequence line** in the order you want, **tap each track's beat once**, hit
**render** — and the engine produces a continuous old-school-scratch mix:
one master tempo, seamless phrase-aligned transitions, whip/wiki scratches
and bass drops placed like a human, dynamics that breathe. Same input, same
mix, bit for bit. No decks, no pads, no knobs, no DJ skills.

**Site:** https://mreider.github.io/termkrush

## The idea

A great mix turns out to be a small, *measurable* grammar — we analyzed a
professional hour-long scratch mix and the engine applies what we found:

- **One master tempo.** The first track in your sequence sets it; everything
  else varispeeds to that grid (pitch rides, turntable-style). The grid never
  moves for the whole mix.
- **Phrase blocks.** Each sequence entry contributes 8–16 bars, aligned to
  that track's own downbeats. The same track can appear at positions 1, 3,
  and 5 — the engine picks different material each time.
- **Seamless swaps.** Tracks are loudness-matched, so the default transition
  is an invisible swap on a phrase boundary; hard cuts land as punctuation,
  fades are rare — exactly the proportions of the reference mix.
- **Macro quantized, micro human.** The engine's scratches and fader chops
  start on the grid but keep deliberately loose internal timing (seeded, so
  it's reproducible) — that looseness is what sounds human.
- **Tension moves.** Bass drops that slam back on the one; an energy arc that
  rises and falls in waves instead of ramping.
- **You curate, it executes.** The only inputs are the track order and the
  tapped beats. There are zero knobs by design.

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

One window, three surfaces, no modes:

- **Library (left).** Your folders of `.wav`/`.mp3`, filesystem-managed.
  Click ▶ to preview; double-click to rename; drag into a folder to move
  (hold to spring it open); drag onto the trash to delete. The **pencil**
  opens the beat-tap editor. Tracks you've tapped wear their BPM in green.
- **Beat-tap (center).** Play the track and tap the **↓ arrow** on each
  beat — a least-squares fit averages your taps into an exact tempo and
  downbeat (shown live). Click the waveform to add/remove a mark, drag the
  handles to trim, **save** to keep the marks forever. You tap a track
  *once, ever* — marks persist and follow renames and moves.
- **Sequence line (bottom).** Drag tracks in, in the order you want them —
  repeats welcome. Drag chips to reorder, X to remove. Chips show each
  track's tempo or a click-to-tap **needs beats** badge; the header tells
  you when the sequence is **ready to render**. Everything autosaves — the
  sequence *is* the project file.

Then **render** (engine stories in progress — see `docs/SPEC.md` §6): the
mix writes into your library as a WAV (MP3 export bundled, no external
tools), and re-rendering the same sequence reproduces it exactly.

## Configuration

TermKrush reads an optional config file at
`~/.config/termkrush/config.toml` (or `$XDG_CONFIG_HOME/termkrush/config.toml`).
Every key is optional; missing keys fall back to defaults.

```toml
# Root directory scanned for the local library.
# Default: ~/Music/termkrush
crate_root = "~/Music/termkrush"
```

The sequence (`sequence.txt`) and the beat-mark cache (`beats.txt`) live next
to the config file — plain text, human-readable, diff-able.

## Layout

A two-crate workspace keeps the engine UI-free and directly testable:

| Crate / module   | Responsibility                                          |
|------------------|---------------------------------------------------------|
| `termkrush-core` | **headless engine** (no UI dep): audio decode/resample, varispeed, the beat-grid fit, the `sequence` (project file), the `beats` cache, mixer + voices (incl. the whip/wiki scratch DSP the engine performs with), library, config — and the mix grammar engine as its stories land |
| `termkrush` (bin)| the thin **UI shell**: the egui/eframe desktop app (library · beat-tap · sequence line) and the audio pump |

## Support

If TermKrush is useful to you: https://buymeacoffee.com/mreider ☕

## License

MIT © 2026 Matt Reider — see [LICENSE](LICENSE).
