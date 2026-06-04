---
title: Deck B mirror of Deck A
type: feature
created: "2026-06-04T09:11:01Z"
modified: "2026-06-04T09:11:01Z"
author: Matt Reider
status: unstarted
estimate: "2"
epic: two-decks
tags: [deck, two-decks]
project: termkrush
---

## Problem statement

Mixing requires at least two simultaneous decks.

## Possible solution

- Hoist `Deck` into a slice owned by `Mixer`: `decks: [Deck; 2]`.
- Focus state: which deck transport keys target. `Tab` cycles focus.
- Each deck has its own independent playback state, position, gain.

## Acceptance

- [ ] Both decks can play simultaneously without interrupting one another.
- [ ] `Tab` cycles focus; transport keys only affect the focused deck.
- [ ] Loading into one deck does not disturb the other.
