---
title: Clip engine test harness for record trim patterns and stretch
type: chore
created: "2026-06-06T14:19:27Z"
modified: "2026-06-06T14:20:49Z"
author: Matt Reider
status: unstarted
epic: clips
project: termkrush
---

## Why a chore
Cross-cutting rigging for the clip engine: helpers to build clips from synthetic/fixture audio and assert region capture, trim bounds, pattern trajectories/gates, and stretch ratios. Reused across the clips epic so each feature's tests stay honest.
## Acceptance
- [ ] Helpers construct a clip from known samples and assert capture/trim/pattern/stretch behavior headlessly.
- [ ] Used by the clip stories; cargo test green.
