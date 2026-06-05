---
title: Per-deck track switching from the loaded list
type: feature
created: "2026-06-05T21:18:13Z"
modified: "2026-06-05T21:19:41Z"
author: Matt Reider
status: unstarted
estimate: "3"
epic: turntables
project: termkrush
---

## Problem statement

Need an easy way to choose which loaded track sits on each turntable: toggle the active deck and swap that deck's track from a list of loaded tracks, without disturbing the other deck.

## Possible solution

- A "loaded tracks" shortlist (tracks pulled from the crate this session) to pick from.
- A per-deck "load the focused deck from the list" action under the ergonomic layout; deck-toggle already exists.
- Swapping deck A's track leaves deck B playing untouched.

## Acceptance

- [ ] Toggle which deck is focused, clearly indicated.
- [ ] Load/replace the focused deck's track by picking from the loaded list (and/or the crate); the other deck keeps playing uninterrupted.
- [ ] A short loaded/recent-tracks list is shown to choose from.

## Prerequisites

Local crate view (accepted), Ergonomic keyboard layout, Turntable platter visuals.
