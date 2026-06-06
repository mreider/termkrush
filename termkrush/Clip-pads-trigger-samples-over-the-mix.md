---
title: Clip pads trigger samples over the mix
type: feature
created: "2026-06-05T21:18:14Z"
modified: "2026-06-06T09:25:28Z"
author: Matt Reider
status: accepted
estimate: "5"
epic: sampler
project: termkrush
started: "2026-06-06T09:19:06Z"
finished: "2026-06-06T09:25:28Z"
delivered: "2026-06-06T09:25:28Z"
accepted: "2026-06-06T09:25:28Z"
---

## Problem statement

Want programmable buttons that play short clips on demand, mixed on top of whatever the decks are playing — DJ stabs / one-shots.

## Acceptance

- [x] At least 4 pads, each assignable to a clip (from the crate / loaded list). (`PADS=4`; `!@#$` assigns the highlighted crate track to pads 1–4, decoded in the event loop.)
- [x] Pressing a pad plays its clip once, mixed atop the decks, without interrupting deck playback. (`1`–`4` trigger; `Mixer::trigger_pad` → one-shot voice summed in `fill_mix` after the deck crossfade; `pad_plays_over_a_playing_deck`.)
- [x] Overlapping triggers mix (polyphonic); a finished one-shot frees its voice. (`pads_are_polyphonic`, `pad_assign_trigger_and_one_shot_lifecycle`.)
- [x] Pad keys fit the layout; pad state is visible. (`1`–`4` / `!@#$`; Mixer panel shows `pads 1● 2· …  voices N`; `pads_readout_reflects_assignment`.)

## Implementation notes

- **Engine (Mixer):** `pads: [Option<Arc<Vec<f32>>>; 4]` + a `Vec<SampleVoice>` of `(Arc clip, pos)`. `trigger_pad` pushes a voice (sharing the clip Arc → cheap polyphony); `mix_voices` sums each voice's next block into the mix **after** the deck crossfade and **before** master (so pads play over whatever's on the decks, independent of the fader), dropping finished one-shots.
- **Assignment is I/O:** `!@#$` (shift+1–4) set a pending `(pad, path)`; the event loop's `apply_pad_assign` decodes and binds it — same testable-lift pattern as the load path, so it's covered end-to-end (`shift_number_assigns_selected_clip_to_a_pad` decodes the real fixture).
- **UI:** the Mixer panel gained a `pads 1● 2· 3· 4·   voices N` line.
- 104 lib tests; fmt/clippy/-Dwarnings green.

## Note (review)

Keys: `1`–`4` trigger, `!@#$` (shift+1–4) assign the highlighted crate track (terminals encode shift+number as those symbols). No per-pad gain/choke yet — one-shots play to the end and sum at unity; that + pad gain can be a follow-up.
