---
title: 'Deck A: load file, play, pause, stop'
type: feature
created: "2026-06-04T09:11:00Z"
modified: "2026-06-05T17:19:06Z"
author: Matt Reider
status: accepted
estimate: "3"
epic: one-deck
tags: [deck, one-deck]
project: termkrush
started: "2026-06-05T17:04:43Z"
finished: "2026-06-05T17:14:14Z"
delivered: "2026-06-05T17:14:14Z"
accepted: "2026-06-05T17:19:06Z"
---

## Problem statement

A `Deck` is the fundamental unit. v1 has one: load a file, hit play, hit pause, hit stop.

## Possible solution

- `deck/mod.rs` with a `Deck` struct: source, position, state (Loaded / Playing / Paused / Stopped), gain.
- Pull-based: the mixer asks each deck for N samples; deck returns silence when stopped/paused.
- Hotkeys (bound in TUI): `[space]` play/pause, `s` stop, `o` open file (hard-coded demo path until library view lands).

## Acceptance

- [x] Spacebar toggles play/pause on the loaded track and audio matches state. (Playing → track samples; paused → silence; unit + TUI tests.)
- [x] Stop resets the playhead to 0 and silences the deck.
- [x] Re-pressing play after stop starts from the beginning.
- [x] Position advances at real-time speed (verified with a 10s fixture). (`tests/deck_test.rs`: 441000 frames drawn = 10.000s; `position_secs()` tracks the playhead.)

## Implementation notes (for PM review)

- **`Deck` (pull-based):** `fill(&mut [f32]) -> usize` writes interleaved stereo and advances the playhead only while `Playing`; silence otherwise. Reaching end-of-track auto-stops and rewinds. State machine: Empty → Loaded → Playing ⇄ Paused, Stopped (rewinds to 0). 7 unit tests in `src/deck/mod.rs`, 4 integration tests on the real fixture.
- **TUI wiring:** `App` owns a `Deck`; `space`/`s`/`o` mapped in the pure, unit-tested `on_key`. `o` only signals intent (loading is I/O); the event loop decodes the demo track. A status line shows state + `mm:ss.s` position/duration.
- **Audio path:** the deck is pumped into the existing ring buffer **inside the UI event loop** — no extra thread, so the realtime cpal callback stays lock-free (per `output.rs`'s discipline). The track is decoded to the device's sample rate so playback pitch is correct.
- **Demo track:** `o` loads `tests/fixtures/sine_a440_10s.wav` (override with `TERMKRUSH_DEMO_TRACK`), since the library view is a later story.
- **Known v1 limitation:** ~93ms transport latency (the ring depth), and audio is topped up from the UI loop; a dedicated audio thread for tighter latency is a future refinement.

Verify by ear: `scripts/dev-run.sh tui`, then `o` (loads the sine), `space` (play — 440 Hz tone), `space` (pause), `s` (stop), `space` (replays from the start). `q` quits.
