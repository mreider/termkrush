---
title: Uniform grid layout with equal sized boxes for decks clips and DJ
type: feature
created: "2026-06-06T14:19:27Z"
modified: "2026-06-06T15:47:41Z"
author: Matt Reider
status: unstarted
estimate: "5"
epic: ux
project: termkrush
---

## Intent
The screen IS the control model: a 2-column grid of equal-sized cells you navigate, where one shared action cluster operates whichever cell is focused. No wasted space, no distinct controls per area. Supersedes the controls-refactor's 3-target Tab and the old mixer/pads layout.

## Layout (12 equal cells, 2 columns)
```
[ Deck A ]   [ Deck B ]
[ Mix·soft]  [ Mix·hard]    soft = auto-fade column, hard = hard-cut column
[ Pad 1 ]    [ Pad 2 ]
[ Pad 3 ]    [ Pad 4 ]
[ Pad 5 ]    [ Pad 6 ]
[ Pad 7 ]    [ DJ ]
```
- Pads grow from 4 -> 7 (engine PADS + grid). DJ is a placeholder tile here; the bobbing cat is its own story.
- Mixer is two cells: soft (timed auto-fade) and hard (instant cut).

## Navigation + control
- **Tab** steps cell-to-cell (wraps Deck A -> ... -> DJ -> Deck A).
- **Arrows** move the focus box spatially across the two columns (up/down/left/right).
- **One action cluster** acts on the focused cell, context-sensitive:
  - Deck: play/pause, cue, jog, volume.
  - Mix·soft: auto-fade to A / to B (duration cycle); Mix·hard: hard-cut to A / B.
  - Pad: assign highlighted crate clip / trigger / (pattern later).
  - DJ: placeholder.

## Acceptance
- [ ] All cells render as equal-sized boxes in a responsive 2-column grid; holds at 100x30+, degrades gracefully smaller.
- [ ] Tab cycles every cell; arrows navigate the grid 2D; focused cell is unmistakable.
- [ ] The shared action cluster drives the focused cell with no per-area key duplication.
- [ ] Mixer split into soft/hard cells; pad bank is 7 + a DJ placeholder tile.
- [ ] Tests: focus navigation (tab + arrows) and per-cell context dispatch; grid golden snapshot.

## Notes
This is the keyboard nav model the Xbox story maps onto (left stick / d-pad = move the focus box; face buttons = the cluster). Build before Xbox.
