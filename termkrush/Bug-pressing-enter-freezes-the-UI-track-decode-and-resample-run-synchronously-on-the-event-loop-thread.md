---
title: 'Bug: pressing enter freezes the UI — track decode and resample run synchronously on the event-loop thread'
type: bug
created: "2026-06-06T09:30:03Z"
modified: "2026-06-06T09:34:47Z"
author: Matt Reider
status: accepted
started: "2026-06-06T09:30:12Z"
finished: "2026-06-06T09:34:47Z"
delivered: "2026-06-06T09:34:47Z"
accepted: "2026-06-06T09:34:47Z"
---

## Symptom

With the crate populated, pressing `enter` freezes the UI for seconds (or longer) before the track appears.

## Cause

`enter` ran the **full decode + resample synchronously on the event-loop thread**. For a 2½-minute MP3 — especially resampled to a 48 kHz device — that's seconds of work with no redraw or input handling, i.e. a freeze. (BPM detection was already off-thread; the decode itself was not.)

## Fix

- Decode now runs on a **background thread** (`spawn_decode`); the finished `Decoded { track, bpm, … }` is posted to the UI loop over a channel and applied via `App::place_decoded`. The event-loop thread never decodes/resamples.
- The deck shows **`⏳ loading…`** while the background decode runs (`App.loading` flag, cleared when the track lands).
- Sampler-pad assignment (`apply_pad_assign`) was the same synchronous-decode hazard and now uses the same off-thread path.

## Verification

- [x] `enter_is_non_blocking_sets_loading_then_clears_it`: `apply_load_action` returns immediately, deck flagged loading, not loaded synchronously; once the decode lands the flag clears and the deck is Loaded.
- [x] End-to-end still works: decode → load → play → audible output (`enter_loads_…`, `loaded_track_produces_audible_output…`).
- [x] No decode/resample left on the UI thread.
- [x] fmt / clippy -Dwarnings / 105 lib tests green.
