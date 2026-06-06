---
title: Resample the live mix into a pad record whatever is playing
type: feature
created: "2026-06-06T18:48:53Z"
modified: "2026-06-06T18:48:53Z"
author: Matt Reider
status: unstarted
estimate: "5"
epic: clips
project: termkrush
---

## Intent
"Record whatever's playing" into a pad — the live-resample / overdub move. A pad is just a clip slot; this adds a third clip **source** (alongside assign-a-crate-track and record-a-deck-region): capture the master output (decks + any active pads) into a buffer and drop it on a chosen pad.

## Design (all pads uniform — no special pad)
- A global **record** control: arm → it captures the master mix to a buffer → disarm → that buffer becomes a clip on the **selected** pad.
- Because the capture is the post-everything master, resampling a mix that already includes pads = **layering/overdub**.
- The DJ stays the 8th cell; pads 1-7 are identical slots.

## Acceptance
- [ ] A record arm/disarm control captures the live master output to a clip.
- [ ] The capture lands on the selected pad and plays back like any other clip.
- [ ] Capturing a mix that includes active pads layers correctly (overdub).
- [ ] Recording state is visible (armed indicator); engine test: captured buffer matches fill_mix output.

## Notes
Pairs with "Record a clip from a deck" (region source) and the clip timeline-trim (which edits ANY pad's clip). Tap the master bus in `fill_mix`.
