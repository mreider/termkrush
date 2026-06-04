---
title: Crossfader cut and momentary kill
type: feature
created: "2026-06-04T09:20:27Z"
modified: "2026-06-04T09:20:27Z"
author: Matt Reider
status: unstarted
estimate: "2"
epic: scratch
tags: [mix, scratch]
project: bigpoppa
---

## Problem statement

Cut scratches need to slam the fader. Tapping backslash should snap to one side.

## Possible solution

- Backslash snap-toggles crossfader between center and the focused-deck-only side.
- Tap-and-hold style: holds while pressed, returns to prior position on release.
- Smoothing disabled in this mode (deliberate snap).

## Acceptance

- [ ] Single tap snaps the fader to the focused deck's side, then back.
- [ ] No clicks despite the snap (sample-aligned cut).
- [ ] Returns to the pre-tap fader position on release.
