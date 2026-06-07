---
title: Retire the step-grid cell-toggle and tracker-only tests
type: chore
created: "2026-06-07T17:52:36Z"
modified: "2026-06-07T17:54:05Z"
author: Matt Reider
status: unstarted
epic: looper
---

## Why
The looper record model replaces the tracker cell-toggle. Once record-to-timeline lands, remove the dead cell-toggle UI + tracker-only tests, keeping the timeline data model (lanes/steps) that playback + render still use.
