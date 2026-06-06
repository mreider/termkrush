---
title: Integration tests for every keyboard command end-to-end
type: chore
created: "2026-06-06T09:06:02Z"
modified: "2026-06-06T09:10:55Z"
author: Matt Reider
status: accepted
epic: foundation
project: termkrush
started: "2026-06-06T09:07:16Z"
finished: "2026-06-06T09:10:55Z"
delivered: "2026-06-06T09:10:55Z"
accepted: "2026-06-06T09:10:55Z"
---

## Why this is a chore

Cross-cutting test infrastructure. The unit tests covered `on_key` (key → `Action` + in-memory state) but **nothing exercised the real command flow through the event loop** — decode, load onto a deck, mix. That gap let "enter does nothing" ship green. This closes it.

## What needs to happen

- Lift the event loop's load step into a testable `apply_load_action` (no TTY/audio).
- Drive commands end-to-end and assert observable state, decoding a real fixture for the load path.

## Acceptance

- [x] Every keyboard command has coverage asserting its effect, not just the returned `Action`. (`every_command_key_maps_to_an_action` sweeps the full key set; per-command state tests already exist.)
- [x] `enter` on a populated crate is proven to decode + load the selected track onto the focused deck. (`enter_loads_the_selected_track_end_to_end` — decodes the real WAV fixture, deck ends Loaded with the right name + 10s duration.)
- [x] Whole chain key→audio proven: `loaded_track_produces_audible_output_when_played` (select→enter→decode→load→play→`fill_mix` → non-silent output).
- [x] Runs with no TTY / no audio device; `cargo test` green (96 lib + integration).

## Implementation notes

- `apply_load_action(app, action, rate, bpm_tx)` carries the event loop's `OpenFile`/`LoadSelected` handling; `event_loop` now just calls it. Tests inject a real fixture path + a throwaway channel and assert the deck loaded — the exact step that was previously untested.
- Fixture: the committed CC0 `sine_a440_10s.wav`, decoded for real, so a broken decode/load can't pass.
