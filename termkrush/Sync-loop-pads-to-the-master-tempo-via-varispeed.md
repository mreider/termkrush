---
title: Sync loop pads to the master tempo via varispeed
type: feature
created: "2026-06-07T11:10:20Z"
modified: "2026-06-07T11:11:38Z"
author: Matt Reider
status: unstarted
estimate: "5"
---

## Problem
Loops whose native BPM differs from the master must lock to it via varispeed (pitch rides, platter feel).

## Acceptance
- [ ] A loop's playback speed = master_bpm / its_base_bpm, so its beats land on the master grid.
- [ ] Stacked loops stay in time; engine test on known BPMs (e.g. 90 + 128 → 120).
