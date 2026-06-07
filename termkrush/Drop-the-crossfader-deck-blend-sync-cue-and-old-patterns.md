---
title: Drop the crossfader deck blend sync cue and old patterns
type: feature
created: "2026-06-07T11:10:18Z"
modified: "2026-06-07T11:46:17Z"
author: Matt Reider
status: accepted
estimate: "3"
project: termkrush
started: "2026-06-07T11:40:58Z"
delivered: "2026-06-07T11:46:17Z"
accepted: "2026-06-07T11:46:17Z"
---

## Problem
Deck-era mixing (crossfader, auto-fade deck-blend, deck sync, deck hot cue) and the old playback pattern set (cut / transformer / stutter / warble / reverse) are obsolete under the pad model.

## Acceptance
- [ ] Crossfader, auto-fade deck-blend, deck sync, and deck cue code + keys removed.
- [ ] The `Pattern` enum and its voice logic removed (the real scratch model replaces it).
- [ ] Master bus + per-pad one-shot playback remain; build + all tests green; no dead references.
