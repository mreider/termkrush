---
title: Turntable platter visuals per deck
type: feature
created: "2026-06-05T21:18:13Z"
modified: "2026-06-05T21:34:34Z"
author: Matt Reider
status: accepted
estimate: "3"
epic: turntables
project: termkrush
started: "2026-06-05T21:32:11Z"
finished: "2026-06-05T21:34:34Z"
delivered: "2026-06-05T21:34:34Z"
accepted: "2026-06-05T21:34:34Z"
---

## Problem statement

The decks render as text panels. They should look like turntables.

## Possible solution

- Draw a small record/platter per deck with a marker that rotates while playing and is still when stopped/paused; rotation reflects the playhead.
- Focused deck's platter amber, the other dim.

## Acceptance

- [x] Each deck shows a round platter with a marker that rotates during play and is still when stopped/paused. (`platter_bucket` derives the rim position from the playhead, which only advances while playing.)
- [x] Marker position tracks the playhead. (`platter_marker_walks_around_the_rim_with_the_playhead`.)
- [x] Focused platter amber, other dim; both visible at 100x30. (Existing focus-border + `deck_panel_renders_the_platter` / `both_deck_panels_and_bars_visible_at_100x30`.)

## Implementation notes

- A 3-row record outline (`╭─╮ │·│ ╰─╯`) with a `◆` marker placed at one of 8 rim positions (N..NW). `platter_bucket(position_secs)` = `(pos / 1.8s rev * 8) mod 8` — so the marker walks the rim while the playhead advances and freezes when transport stops. No separate animation clock.
- The platter sits on the left of each deck panel; the readout (name, state·gain, elapsed/total) reads to its right, with the position bar beneath. Focus still drives the amber/dim border (`deck_border`).
- Marker is `◆` (distinct from the crossfader's `●`) so the tests stay specific.

## Note (review)

It's a stylized few-cell record, not a literal spinning disc — the marker stepping around the rim reads as rotation at ~30fps redraw. Verified headlessly (marker placement + rotation math + render); the motion is best seen live (`scripts/dev-run.sh tui`, load a track, play).
