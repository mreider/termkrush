---
title: Clip engine test harness for record trim patterns and stretch
type: chore
created: "2026-06-06T14:19:27Z"
modified: "2026-06-06T19:19:17Z"
author: Matt Reider
status: accepted
epic: clips
project: termkrush
started: "2026-06-06T19:17:55Z"
finished: "2026-06-06T19:19:17Z"
delivered: "2026-06-06T19:19:17Z"
accepted: "2026-06-06T19:19:17Z"
---

## Why a chore
Cross-cutting rigging for the clip engine: helpers to build clips from synthetic/fixture audio and assert region capture, trim bounds, pattern trajectories/gates, and stretch ratios. Reused across the clips epic so each feature's tests stay honest.
## Acceptance
- [ ] Helpers construct a clip from known samples and assert capture/trim/pattern/stretch behavior headlessly.
- [ ] Used by the clip stories; cargo test green.
