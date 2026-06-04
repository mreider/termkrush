---
title: Seek and scrub position
type: feature
created: "2026-06-04T09:11:00Z"
modified: "2026-06-04T09:11:00Z"
author: Matt Reider
status: unstarted
estimate: "2"
epic: one-deck
tags: [deck, one-deck]
project: termkrush
---

## Problem statement

Need to be able to jump around in a track and scrub for cueing.

## Possible solution

- `Deck::seek(seconds)`.
- Hotkeys: `<-`/`->` jump ±5s; `Shift <-/->` jump ±30s; `,`/`.` nudge ±0.1s.
- Seeking pauses the audio callback for one buffer to avoid clicks.

## Acceptance

- [ ] Arrow keys move the playhead by the documented amount.
- [ ] Seeking past EOF clamps to EOF and stops the deck.
- [ ] No audible click on seek.
