---
title: Jog scratch voice DSP in the core engine
type: feature
created: "2026-06-08T12:17:12Z"
modified: "2026-06-08T12:53:30Z"
author: Matt Reider
status: delivered
estimate: "5"
epic: gui
started: "2026-06-08T12:49:50Z"
finished: "2026-06-08T12:53:30Z"
delivered: "2026-06-08T12:53:30Z"
project: termkrush
---

## Goal
A live, position-controlled jog/scratch voice in termkrush-core (the engine half of scratching).
## Scope
- A voice with a playhead position + instantaneous velocity through a clip; reads/resamples by velocity (pitch rides speed; reverse = whip, forward = wiki).
- API to set velocity/position per frame from the front-end; persistent playhead (continuity) + spring-to-cue on idle.
- Headless DSP tests (advance, reverse, velocity->pitch, cue reset).
