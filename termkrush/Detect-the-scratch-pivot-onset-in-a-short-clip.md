---
title: Detect the scratch pivot onset in a short clip
type: feature
created: "2026-06-07T11:10:20Z"
modified: "2026-06-07T11:12:11Z"
author: Matt Reider
status: unstarted
estimate: "3"
---

## Problem
A scratch pad holds a very short clip; the software must find the scratch point — the onset where the "needle" pivots — so rubs land on the sound.

## Acceptance
- [ ] An onset/transient detector returns a pivot frame for a short clip.
- [ ] On known content (a click at a known offset) the pivot lands on it (engine test).
- [ ] The pivot is stored on the scratch pad and shown.
