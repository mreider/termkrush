---
title: Master timeline as a permanent top strip
type: feature
created: "2026-06-07T17:52:35Z"
modified: "2026-06-07T20:18:55Z"
author: Matt Reider
status: delivered
estimate: "8"
epic: looper
project: termkrush
started: "2026-06-07T18:11:01Z"
delivered: "2026-06-07T20:18:55Z"
---

## Goal
Rework the main view into the looper layout.

## Spec
- **Master timeline = permanent full-width strip at the TOP** (lanes per pad, bar ruler, playhead). Not a modal.
- **8 pads** (was 7), and **remove the DJ panel/cat** entirely (Focus::Dj gone).
- Pads grid + library occupy the lower region.
- **Up / Down arrows = volume**: a focused pad → its volume; the timeline/master focused → master volume. Move focus with **Tab / Shift-Tab + Left/Right** (frees Up/Down).
- Keep play/render controls.
