---
title: Varispeed nudge deck playback speed pitch rides along driven by the BPM keys
type: feature
created: "2026-06-06T17:36:04Z"
modified: "2026-06-06T19:16:51Z"
author: Matt Reider
status: accepted
estimate: "3"
epic: tempo
project: termkrush
started: "2026-06-06T19:12:41Z"
finished: "2026-06-06T19:16:50Z"
delivered: "2026-06-06T19:16:50Z"
accepted: "2026-06-06T19:16:51Z"
---

## Intent
Make the BPM keys *do* something audible, the easy way: varispeed. Reading the track faster/slower raises/lowers tempo AND pitch together (record-on-a-platter). Cheap — a fractional read with linear interpolation. Pitch-*preserving* speed change is the separate, harder time-stretch story.

## Behaviour
- Per-deck `speed` multiplier (1.0 = native), clamped to a sane range (e.g. 0.5x-2.0x).
- The existing `,`/`.` BPM keys drive it: nudging changes `speed` so the deck plays faster/slower; the deck panel's BPM = base x speed (so the shown BPM tracks the ratio).
- `fill()` reads the source at `speed` via linear interpolation between frames; seek/position/EOF stay correct.

## Acceptance
- [ ] A deck plays faster/slower by its speed multiplier; pitch rides along.
- [ ] `,`/`.` change actual playback speed; panel BPM reflects base x speed.
- [ ] Output stays click-free and finite; position/seek/EOF still correct under varispeed (property + unit tests).
- [ ] Pitch-preserving remains the time-stretch story (documented).
