---
title: TUI deck panel
type: feature
created: "2026-06-04T09:11:00Z"
modified: "2026-06-05T17:34:43Z"
author: Matt Reider
status: accepted
estimate: "2"
epic: one-deck
tags: [tui, one-deck]
project: termkrush
started: "2026-06-05T17:34:42Z"
finished: "2026-06-05T17:34:42Z"
delivered: "2026-06-05T17:34:43Z"
accepted: "2026-06-05T17:34:43Z"
---

## Problem statement

The deck has state but no visual representation in the TUI.

## Possible solution

- Widget showing: track title (ID3 fallback to filename), elapsed / total time, transport state, gain.
- Position bar: simple block-character progress (no real waveform yet — that is icebox).
- Color coding: amber for the playing deck, green accent on transport hint row.

## Acceptance

- [x] Loading a track updates the title and total time within one frame. (`panel_shows_title_and_total_time_on_load`.)
- [x] The elapsed counter ticks visibly during play. (`panel_elapsed_advances_then_freezes_and_glyph_changes`.)
- [x] Position bar fills proportionally to elapsed/total. (`progress_bar_fills_proportionally`.)
- [x] Pausing freezes the elapsed counter and changes the transport indicator glyph (▶ → ⏸).

## Implementation notes

- `draw_deck_panel` renders a bordered "Deck A" box: title (ID3 `title()` → filename via new `Deck::display_name`), `glyph state   gain N.NN`, a `[████░░░░]` proportional bar, and `mm:ss.s / mm:ss.s`. Border is amber while playing, dim otherwise; the key-hint row is the green accent.
- Pure helpers `progress_bar(frac, width)` and `transport_glyph(state)` are unit-tested directly; the panel rendering is checked headlessly via `TestBackend`.
- New `Deck` accessors: `display_name()`, `gain()`, and `load_named()` (records the file name for the title fallback). `load_demo_track` now passes the basename.
- Replaced the placeholder single-line status from the Deck A story with this panel.
