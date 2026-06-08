---
title: 'Zoomable clip editor: windowed bar, +/- zoom, follow the active handle'
type: feature
created: "2026-06-08T07:56:38Z"
modified: "2026-06-08T07:56:38Z"
author: Matt Reider
status: started
estimate: "5"
started: "2026-06-08T07:56:38Z"
---

## Goal
Real zoom in the clip editor so you can trim to the millisecond, on any song length, without snipping.

## Spec
- The fixed-width bar shows a WINDOW of the clip (not always the whole thing). A zoom level sets the window span: whole clip → 10s → 1s → 100ms → 10ms.
- `+` / `=` zoom in (smaller span); `-` zoom out. Arrows nudge the active handle by ~one column at the current zoom, so precision scales with zoom.
- The window FOLLOWS the active handle: `Tab` switches the in-handle ↔ out-handle and the view jumps to show that end ("scroll to the end").
- A full-clip overview line (minimap) shows where the window sits + both handles; a readout shows the zoom span and the in/out positions in ms.
- Keep: `Space` audition (toggle), `Enter` snip, `Esc` close. Replaces the Shift-fine + snip-to-zoom workarounds for precision.
