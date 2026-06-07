---
title: Fade in and fade out on timeline blocks
type: feature
created: "2026-06-07T18:03:21Z"
modified: "2026-06-07T18:03:21Z"
author: Matt Reider
status: unstarted
estimate: "5"
epic: looper
---

## Goal
When a pad is recorded/added onto the master timeline, its block can have a **fade-in** and **fade-out** envelope (so entries/exits aren't abrupt).

## Spec
- Each timeline block carries fade-in and fade-out lengths (bars/beats), default short or zero.
- Editable on the block (e.g. in the clip/timeline editor); applied during playback + render.
- Depends on the record-to-timeline block model (epic: looper).
