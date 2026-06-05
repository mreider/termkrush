---
title: Two-deck TUI layout
type: feature
created: "2026-06-04T09:15:05Z"
modified: "2026-06-05T18:55:59Z"
author: Matt Reider
status: accepted
estimate: "3"
epic: two-decks
tags: [tui, two-decks]
project: termkrush
started: "2026-06-05T18:53:31Z"
finished: "2026-06-05T18:55:59Z"
delivered: "2026-06-05T18:55:59Z"
accepted: "2026-06-05T18:55:59Z"
---

## Problem statement

The TUI needs to show both decks and the crossfader at once.

## Possible solution

- Vertical split: deck A on left, deck B on right, crossfader bar at the bottom of the mixer row.
- Crate panel collapsible to give decks more room.
- Focused deck has an amber border; unfocused deck has dim border.

## Acceptance

- [x] At 100x30, both deck panels are fully visible with their position bars. (`both_deck_panels_and_bars_visible_at_100x30`.)
- [x] Crossfader position is rendered as a fader graphic at the bottom of the mixer row. (`draw_mixer_panel` renders `A |───●───| B` over the master line in a bordered "Mixer" panel beneath the decks.)
- [x] Focus border colors match design: amber focused, dim unfocused. (`deck_border(focused)` → amber / DarkGray; `deck_border_is_amber_focused_dim_unfocused`.)

## Implementation notes

- **Layout:** body is now crate (left, fixed 32 cols, collapsible) + mixer area (right). The mixer area stacks a decks row (A | B side by side, 50/50) over a 4-high bordered "Mixer" row holding the crossfader fader graphic and master readout.
- **Collapsible crate:** `c` toggles `App.crate_collapsed`; when collapsed the decks take the full width. (Listed in the story's solution; not an acceptance gate, but cheap and useful.)
- **Focus color:** extracted `deck_border(focused)` (amber focused, dim unfocused) — replaces the earlier play-vs-focus heuristic so the rule matches the design exactly and is unit-testable. The `▸` marker still flags focus too.
- Hint row trimmed to fit 80 cols (full keymap lives in `?` help, which gained `c`).

## Note (review)

This wraps the two-decks epic (Deck B + crossfader + this layout). TUI is verified headlessly (TestBackend: panel titles, position-bar/crossfader glyphs, border-color helper) — the visual proportions are best confirmed by running it at 100x30. Crate fixed at 32 cols; at narrow widths (<90) the side-by-side decks get tight but stay functional.
