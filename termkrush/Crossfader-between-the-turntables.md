---
title: Crossfader between the turntables
type: feature
created: "2026-06-05T21:18:14Z"
modified: "2026-06-05T21:41:27Z"
author: Matt Reider
status: accepted
estimate: "2"
epic: turntables
project: termkrush
started: "2026-06-05T21:40:11Z"
finished: "2026-06-05T21:41:27Z"
delivered: "2026-06-05T21:41:27Z"
accepted: "2026-06-05T21:41:27Z"
---

## Problem statement

The crossfader audio path is done (Crossfader audio mix, accepted), but it should sit visually between the two platters and read as crossing A↔B in the turntable view.

## Possible solution

- Render the crossfader fader graphic so it spans between the deck platters, reflecting position.
- Moved by the ergonomic crossfader keys; audio behavior unchanged.

## Acceptance

- [x] Crossfader graphic sits between the two turntables and shows the current position. (Full-width fader in the Mixer row directly beneath the decks — `A ───●─── B`, A under deck A, B under deck B; `crossfader_stretches_across_between_the_decks`.)
- [x] The ergonomic crossfader keys move it; centering works; audio behavior unchanged. (`g`/`h` slide, `space` centers — `gh_slide_and_space_centers_crossfader`; the accepted audio crossfade is untouched.)

## Implementation notes

- `draw_mixer_panel` now sizes the fader to the panel width (was a fixed 21 cells): `A {bar} B` with the rail stretching across the full deck span, so the handle's travel maps left→A, right→B. Panel titled "Mixer · crossfader".
- The keys (g/h/space) and the `Mixer::xfade` audio law were already in place from the ergonomic-keymap and crossfader-audio stories; this is the visual placement between the decks.

## Note (review)

A DJ crossfader lives horizontally beneath/between the platters — that's the placement here (Mixer row under the side-by-side decks, fader spanning A→B), rather than a vertical seam. Verified headlessly (span + A/B ends + handle).
