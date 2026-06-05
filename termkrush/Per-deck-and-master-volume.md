---
title: Per-deck and master volume
type: feature
created: "2026-06-04T09:11:00Z"
modified: "2026-06-05T17:40:16Z"
author: Matt Reider
status: accepted
estimate: "2"
epic: one-deck
tags: [deck, mix, one-deck]
project: termkrush
started: "2026-06-05T17:36:25Z"
finished: "2026-06-05T17:40:16Z"
delivered: "2026-06-05T17:40:16Z"
accepted: "2026-06-05T17:40:16Z"
---

## Problem statement

Gain control is non-negotiable for mixing. Need per-deck volume now, plus a master.

## Possible solution

- `Deck::gain: f32` (0.0 .. 1.5; 1.0 = unity, headroom to +3.5 dB).
- `Mixer::master: f32`.
- TUI: `+` / `-` on focused deck nudges by 0.05; master volume keys.
- A tiny dB readout per deck.

## Acceptance

- [x] +/- on a deck changes its level without zipper noise (smoothed). (Per-frame gain ramp; `gain_change_ramps_without_jumping` proves no instantaneous jump, `gain_reaches_target_after_enough_frames` proves convergence.)
- [x] Master gain changes all decks simultaneously. (`Mixer::apply` scales the mixed output; one deck today, sums later. `Mixer` ramp tests.)
- [x] dB readout updates in real-time and matches the gain value. (Panel shows `gain N.NN  ±D.D dB` and a master readout; `fmt_db` + `panel_shows_db_readout_and_master` tests. Readout reflects the target value immediately; audio ramps under it.)

## Implementation notes

- **Smoothing:** both `Deck` and `Mixer` keep a `smoothed` gain that ramps toward the target by at most `1/512` per frame (~12ms at 44.1k), so level changes de-zipper. The displayed value is the *target* (updates instantly), which is what "matches the gain value" means.
- **Range:** `GAIN_MIN=0.0 .. GAIN_MAX=1.5` (+3.5 dB headroom), clamped in `set_gain`/`set_master`.
- **Mixer:** new minimal `mix::Mixer` carrying master gain + `apply(buf)`. Summing multiple decks / crossfade is the later two-deck refactor; today master scales the single deck.
- **Keys:** `+`/`=` and `-` nudge the deck by 0.05; `<`/`>` nudge master. The spec said "Shift +/-" for master, but terminals encode Shift with +/- ambiguously, so `<`/`>` are the unambiguous stand-in (documented in the help overlay and hint row).
- **Verification of "no zipper" is structural** (ramp never jumps > one step/frame) rather than an audio-listening test; smoothing is unit-tested.
