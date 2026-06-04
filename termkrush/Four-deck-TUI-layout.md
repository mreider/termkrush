---
title: Four-deck TUI layout
type: feature
created: "2026-06-04T09:20:28Z"
modified: "2026-06-04T09:20:28Z"
author: Matt Reider
status: unstarted
estimate: "3"
epic: multi-deck
tags: [tui, multi-deck]
project: termkrush
---

## Problem statement

The two-deck layout does not fit four.

## Possible solution

- 2x2 grid of deck panels above the mixer row.
- Smaller per-deck height; the crate panel collapses to a sliver by default.
- Focus moves through 1-2-3-4 with Tab and Shift+Tab.
- Number keys 1-4 jump focus directly.

## Acceptance

- [ ] Layout renders cleanly at 120x36.
- [ ] All four decks show essential state simultaneously.
- [ ] Number-key focus works.
