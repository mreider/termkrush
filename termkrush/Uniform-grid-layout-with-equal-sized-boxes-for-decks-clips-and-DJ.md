---
title: Uniform grid layout with equal sized boxes for decks clips and DJ
type: feature
created: "2026-06-06T14:19:27Z"
modified: "2026-06-06T17:45:43Z"
author: Matt Reider
status: accepted
estimate: "5"
epic: ux
project: termkrush
started: "2026-06-06T17:37:27Z"
finished: "2026-06-06T17:45:43Z"
delivered: "2026-06-06T17:45:43Z"
accepted: "2026-06-06T17:45:43Z"
---

## Intent (render half)
Lay the screen out as a 2-column grid of equal-sized cells. This story is the **render**; the tab/arrow navigation + per-cell action dispatch is its own follow-up.

## Layout (12 equal cells, 2 columns)
```
[ Deck A ]   [ Deck B ]
[ Mix·soft]  [ Mix·hard]
[ Pad 1 ]    [ Pad 2 ]
[ Pad 3 ]    [ Pad 4 ]
[ Pad 5 ]    [ Pad 6 ]
[ Pad 7 ]    [ DJ ]
```
- Pad bank grows 4 -> 7 (engine PADS + grid); direct triggers become 1-7.
- Mixer becomes two cells: soft (auto-fade) + hard (hard-cut), each showing its state.
- DJ is a placeholder tile (the bobbing cat is its own story).

## Acceptance
- [ ] All 12 cells render as equal-sized boxes in a responsive 2-column grid; holds at 100x30+, degrades gracefully smaller.
- [ ] Decks, mixer soft/hard, 7 pads, DJ placeholder all present; no truncation/overlap.
- [ ] The currently-focused cell is visibly highlighted (using the existing focus state).
- [ ] PADS = 7; `1`-`7` trigger; pad cells show number + filled/empty + bpm.
- [ ] Grid golden-snapshot test; existing render tests updated.

## Follow-up
Tab/arrow focus navigation across all cells + shared per-cell action dispatch — separate story.
