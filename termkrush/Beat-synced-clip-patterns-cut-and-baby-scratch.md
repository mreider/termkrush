---
title: Beat synced clip patterns cut and baby scratch
type: feature
created: "2026-06-06T14:19:27Z"
modified: "2026-06-06T19:43:39Z"
author: Matt Reider
status: accepted
estimate: "5"
epic: clips
project: termkrush
started: "2026-06-06T19:39:09Z"
finished: "2026-06-06T19:43:39Z"
delivered: "2026-06-06T19:43:39Z"
accepted: "2026-06-06T19:43:39Z"
---

## Intent
Beat-synced playback patterns on a clip, driven by its BPM. Lead with the two named ones: **Cut** (forward audible, reverse muted - "over and silent") and **Baby-scratch** (forward+back, both audible - "over and back"). A pattern is a position trajectory + gate, a function of the beat division.
## Acceptance
- [ ] Per-clip pattern select cycles Straight / Cut / Baby-scratch.
- [ ] Cut and Baby-scratch lock to the clip's BPM at a chosen division (1/8, 1/16).
- [ ] Engine tests assert the trajectory/gate per pattern (forward/reverse, muted segments).
- Depends on Offline BPM detection.
