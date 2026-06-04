---
title: Refactor mixer for N decks
type: feature
created: "2026-06-04T09:20:28Z"
modified: "2026-06-04T09:20:28Z"
author: Matt Reider
status: unstarted
estimate: "5"
epic: multi-deck
tags: [architecture, mix, multi-deck]
project: bigpoppa
---

## Problem statement

We capped at 2 decks for v0.1. Expanding to 3-4 means lifting the hard-coded array and rethinking the crossfader.

## Possible solution

- Mixer holds a Vec of Deck values with up to 4 entries.
- Replace the single crossfader with a pair of fader assignments (see next story).
- All FX, sync, scratch logic must take a deck index, not a hard A/B.

## Acceptance

- [ ] 4 decks load and play concurrently.
- [ ] All transport, sync, FX hotkeys target the focused deck regardless of index.
- [ ] Existing two-deck behavior unchanged when only 2 decks are loaded.
