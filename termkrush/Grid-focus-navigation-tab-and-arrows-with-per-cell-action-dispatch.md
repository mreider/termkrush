---
title: Grid focus navigation tab and arrows with per cell action dispatch
type: feature
created: "2026-06-06T17:37:27Z"
modified: "2026-06-06T17:37:27Z"
author: Matt Reider
status: unstarted
estimate: "5"
epic: controls
project: termkrush
---

## Intent (navigation half)
Make every grid cell focusable and operate it with one shared action cluster — the focus->act model extended across the whole 12-cell grid.

## Behaviour
- **Tab** steps cell-to-cell (wraps Deck A -> ... -> DJ -> Deck A).
- **Arrows** move the focus box 2D across the two columns (up/down/left/right).
- One **action cluster** acts on the focused cell, by type:
  - Deck: play/cue/jog/volume/BPM.
  - Mix·soft: auto-fade to A/B (+ duration); Mix·hard: hard-cut to A/B.
  - Pad: assign highlighted crate clip / trigger / BPM.
  - DJ: placeholder.

## Acceptance
- [ ] Tab cycles every cell; arrows navigate 2D; focused cell unmistakable.
- [ ] The shared cluster dispatches correctly per focused cell type; no per-area key duplication.
- [ ] Supersedes the 3-target Tab + the clips_focused/clip_sel overlay.
- [ ] Tests for tab order, arrow nav, and per-cell dispatch.

## Notes
This is the keyboard nav the Xbox story maps onto. Depends on the grid render.
