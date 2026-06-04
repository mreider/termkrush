---
title: Two-deck TUI layout
type: feature
created: "2026-06-04T09:15:05Z"
modified: "2026-06-04T09:15:05Z"
author: Matt Reider
status: unstarted
estimate: "3"
epic: two-decks
tags: [tui, two-decks]
project: bigpoppa
---

## Problem statement

The TUI needs to show both decks and the crossfader at once.

## Possible solution

- Vertical split: deck A on left, deck B on right, crossfader bar at the bottom of the mixer row.
- Crate panel collapsible to give decks more room.
- Focused deck has an amber border; unfocused deck has dim border.

## Acceptance

- [ ] At 100x30, both deck panels are fully visible with their position bars.
- [ ] Crossfader position is rendered as a fader graphic at the bottom of the mixer row.
- [ ] Focus border colors match design: amber focused, dim unfocused.
