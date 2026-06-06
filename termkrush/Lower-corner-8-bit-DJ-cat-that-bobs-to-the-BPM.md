---
title: Lower-corner 8-bit DJ cat that bobs to the BPM
type: feature
created: "2026-06-05T21:18:14Z"
modified: "2026-06-06T19:31:59Z"
author: Matt Reider
status: accepted
estimate: "3"
epic: mascot
project: termkrush
started: "2026-06-06T19:30:39Z"
finished: "2026-06-06T19:31:58Z"
delivered: "2026-06-06T19:31:59Z"
accepted: "2026-06-06T19:31:59Z"
---

## Intent
An 8-bit DJ (cat) that bobs to the detected BPM, rendered as one equal-sized tile in the uniform grid (not just a lower corner). Far less detail than the reference art.
## Acceptance
- [ ] The DJ tile is the same box size as decks/clip buttons in the grid.
- [ ] It bobs on the beat from the detected BPM; idles when nothing is playing.
- [ ] Low-detail 8-bit; render test (presence + a beat-driven frame change).
- Depends on Offline BPM detection + the uniform grid layout.
