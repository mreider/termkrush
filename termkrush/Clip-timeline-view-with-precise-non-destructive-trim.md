---
title: Clip timeline view with precise non destructive trim
type: feature
created: "2026-06-06T14:19:27Z"
modified: "2026-06-06T19:37:34Z"
author: Matt Reider
status: accepted
estimate: "5"
epic: clips
project: termkrush
started: "2026-06-06T19:32:50Z"
finished: "2026-06-06T19:37:34Z"
delivered: "2026-06-06T19:37:34Z"
accepted: "2026-06-06T19:37:34Z"
---

## Intent
Precisely edit a clip's length on a timeline — non-destructive: adjust in/out bounds only, never modify the underlying audio ("edit them down").
## Acceptance
- [ ] A timeline view shows the clip with movable in/out handles (fine + coarse nudge).
- [ ] Trimming changes playback bounds only; source samples untouched; re-widenable.
- [ ] Tests cover trim math (bounds, clamping) + the timeline render.
