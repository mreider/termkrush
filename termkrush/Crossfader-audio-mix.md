---
title: Crossfader audio mix
type: feature
created: "2026-06-04T09:15:05Z"
modified: "2026-06-05T18:51:38Z"
author: Matt Reider
status: accepted
estimate: "3"
epic: two-decks
tags: [mix, two-decks]
project: termkrush
started: "2026-06-05T18:47:13Z"
finished: "2026-06-05T18:51:37Z"
delivered: "2026-06-05T18:51:38Z"
accepted: "2026-06-05T18:51:38Z"
---

## Problem statement

The point of two decks is mixing between them. Need a crossfader.

## Possible solution

- A signed mix coefficient in -1.0 .. +1.0 (-1 = A only, +1 = B only, 0 = both at unity).
- Linear curve for v0.1; alternate curves are an icebox story.
- Hotkeys: `[` and `]` slide the crossfader by 0.05; `\` returns to center.
- Per-sample apply in the mix callback.

## Acceptance

- [x] Sliding the crossfader from -1 to +1 produces a smooth transition between the two decks. (`full_left_is_deck_a_only_full_right_is_deck_b_only`; per-frame ramped position.)
- [x] At 0, both decks play at unity gain together. (`center_plays_both_at_unity`; linear law `xfade_gains(0) = (1,1)`.)
- [x] No zipper noise during slow slides (parameter smoothed). (`xfade_slide_is_smoothed`: a hard jump moves only one ramp step on the first frame.)

## Implementation notes

- **Linear law** `xfade_gains(pos) = (1 - max(0,pos), 1 + min(0,pos))`: center leaves both at unity, the off-side deck ramps to silence toward each end. Alternate curves stay iceboxed.
- **Per-frame in `fill_mix`:** each deck fills its own scratch, then per frame the position ramps toward target (`1/512`/frame) and the A/B gains scale the two decks into the mix before master. So the signal chain is deck gain → crossfader → master, each independently smoothed.
- **Keys:** `[` toward A, `]` toward B, `\` re-center (`Mixer::nudge_xfade`/`center_xfade`). A `|───●───|` slider readout (`crossfader_bar`) renders between the decks and the master line; hint row + help updated.

## Note (review)

Crossfade is the gain-staging I flagged on Deck B: at full-A or full-B only one deck is heard, and at center both play. There's still no clip ceiling on the summed/centered mix (two loud tracks at center can exceed 0 dBFS) — a limiter/headroom pass is a separate concern. Smoothing verified structurally; TUI verified headlessly.
