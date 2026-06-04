---
title: Keyboard jog scratch
type: feature
created: "2026-06-04T09:20:27Z"
modified: "2026-06-04T09:20:27Z"
author: Matt Reider
status: unstarted
estimate: "5"
epic: scratch
tags: [scratch, dsp]
project: termkrush
---

## Problem statement

The scratch feature: real-time playhead manipulation via keyboard.

## Possible solution

- Hold Space (modifier) on focused deck to enter jog mode; arrow keys move the playhead at variable speed.
  - Left / Right: linear scrub.
  - Shift-Left/Right: fast scrub.
  - Tap Left to throw back a short distance with a quick decay.
- Time-domain stretch with cubic interpolation (no pitch correction during jog — pitch should bend, that is the sound).
- Stay inside the marked zone if one exists; wrap or clamp at boundaries.

## Acceptance

- [ ] Holding Space + Left/Right produces audible scratch motion.
- [ ] Releasing Space returns the deck to its prior state (playing or paused).
- [ ] Tap-back sounds like a baby scratch on common test tracks.
- [ ] Zone boundaries clamp the playhead.
