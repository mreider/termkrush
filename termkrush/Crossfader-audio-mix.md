---
title: Crossfader audio mix
type: feature
created: "2026-06-04T09:15:05Z"
modified: "2026-06-04T09:15:05Z"
author: Matt Reider
status: unstarted
estimate: "3"
epic: two-decks
tags: [mix, two-decks]
project: termkrush
---

## Problem statement

The point of two decks is mixing between them. Need a crossfader.

## Possible solution

- A signed mix coefficient in -1.0 .. +1.0 (-1 = A only, +1 = B only, 0 = both at unity).
- Linear curve for v0.1; alternate curves are an icebox story.
- Hotkeys: `[` and `]` slide the crossfader by 0.05; `\` returns to center.
- Per-sample apply in the mix callback.

## Acceptance

- [ ] Sliding the crossfader from -1 to +1 produces a smooth transition between the two decks.
- [ ] At 0, both decks play at unity gain together.
- [ ] No zipper noise during slow slides (parameter smoothed).
