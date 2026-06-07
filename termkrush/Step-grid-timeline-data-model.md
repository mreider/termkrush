---
title: Step grid timeline data model
type: feature
created: "2026-06-07T11:10:21Z"
modified: "2026-06-07T12:33:17Z"
author: Matt Reider
status: started
estimate: "3"
project: termkrush
started: "2026-06-07T12:33:17Z"
---

## Problem
The arrangement is a tempo-locked tracker grid: bars/beats × lanes (one per pad). This story is the headless model.

## Acceptance
- [ ] A timeline model of bars/beats with one lane per pad; steps are quantized to the grid.
- [ ] Place/clear a pad hit at a step; query what plays at a given tick.
- [ ] Pure + unit-tested (no UI).
