---
title: Auto fade and hard cut between decks replacing the manual crossfader
type: feature
created: "2026-06-06T14:19:26Z"
modified: "2026-06-06T14:45:11Z"
author: Matt Reider
status: accepted
estimate: "5"
epic: transitions
project: termkrush
started: "2026-06-06T14:38:15Z"
finished: "2026-06-06T14:45:11Z"
delivered: "2026-06-06T14:45:11Z"
accepted: "2026-06-06T14:45:11Z"
---

## Intent
Replace the manual crossfader (poor terminal UX) with automation: an instant hard-cut A<->B and a hands-free timed auto-fade. Supersedes the manual crossfader control (Crossfader audio mix / Crossfader between the turntables) and folds in "auto-fade over N bars".
## Acceptance
- [ ] Keys hard-cut the mix to A-only / B-only instantly.
- [ ] A key triggers an auto-fade to A / to B over a selectable duration (1/2/4/8 s), running hands-free via the existing smoothed crossfade.
- [ ] Manual nudge keys (g/h/space) and the `A --o-- B` bar are removed; the mixer row shows blend + fade state + duration.
- [ ] Underlying A*gainA + B*gainB mix unchanged; tests cover hard-cut + the timed-fade trajectory.
