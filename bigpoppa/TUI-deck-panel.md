---
title: TUI deck panel
type: feature
created: "2026-06-04T09:11:00Z"
modified: "2026-06-04T09:11:00Z"
author: Matt Reider
status: unstarted
estimate: "2"
epic: one-deck
tags: [tui, one-deck]
project: bigpoppa
---

## Problem statement

The deck has state but no visual representation in the TUI.

## Possible solution

- Widget showing: track title (ID3 fallback to filename), elapsed / total time, transport state, gain.
- Position bar: simple block-character progress (no real waveform yet — that is icebox).
- Color coding: amber for the playing deck, green accent on transport hint row.

## Acceptance

- [ ] Loading a track updates the title and total time within one frame.
- [ ] The elapsed counter ticks visibly during play.
- [ ] Position bar fills proportionally to elapsed/total.
- [ ] Pausing freezes the elapsed counter and changes the transport indicator glyph.
