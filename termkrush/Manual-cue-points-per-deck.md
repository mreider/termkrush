---
title: Manual cue points per deck
type: feature
created: "2026-06-04T09:15:06Z"
modified: "2026-06-06T20:05:19Z"
author: Matt Reider
status: accepted
estimate: "3"
epic: sync-and-fade
tags: [deck, cue]
project: termkrush
started: "2026-06-06T20:02:30Z"
finished: "2026-06-06T20:05:19Z"
delivered: "2026-06-06T20:05:19Z"
accepted: "2026-06-06T20:05:19Z"
---

## Problem statement

Phase alignment matters at least as much as tempo. We need at least one cue point per deck so the user can drop on the 1.

## Possible solution

- One memory cue per deck (multiple cues = icebox).
- Hotkey c on focused deck: drop cue at current position.
- Shift-c: jump to cue.
- Cue position visible on the position bar as a small marker.
- Cues persist in the per-file cache.

## Acceptance

- [ ] c drops a cue at the current playhead.
- [ ] Shift-c jumps back to the cue from anywhere in the track.
- [ ] Cue marker appears on the position bar.
- [ ] Cue persists across termkrush restarts.
