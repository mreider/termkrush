---
title: Optional auto BPM on clip playback via time stretch
type: feature
created: "2026-06-06T14:19:27Z"
modified: "2026-06-06T14:20:49Z"
author: Matt Reider
status: unstarted
estimate: "5"
epic: clips
project: termkrush
---

## Intent
Optionally time-stretch a clip to a target BPM on playback (beat-match to a deck/reference), pitch-preserving. Off = play at native rate.
## Acceptance
- [ ] Per-clip auto-BPM toggle + target (deck A/B or a set BPM).
- [ ] When on, the clip plays time-stretched to the target tempo with pitch preserved.
- [ ] Tests assert output length/tempo matches the target within tolerance.
- Depends on Pitch-preserving time-stretch + Offline BPM detection.
