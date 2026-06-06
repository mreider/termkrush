---
title: Xbox controller support as the primary input mapped down to keyboard
type: feature
created: "2026-06-06T14:49:48Z"
modified: "2026-06-06T14:50:12Z"
author: Matt Reider
status: unstarted
estimate: "5"
epic: controls
project: termkrush
---

## Intent
An Xbox controller is the **preferred** way to play TermKrush — it gives the continuous control a terminal can't: the **right stick is the jog/scratch platter**. Everything is operable from the pad via the same focus -> act model; the keyboard is the fallback mapping with parity.

## Proposed mapping
- **LB / RB**: focus Deck A / Deck B.
- **D-pad**: select within context (crate item / clip slot / marker).
- **Left stick**: scrub the focused deck / menu nav.
- **Right stick X**: crossfade + **jog/scratch** the focused deck (continuous — real scratching).
- **LT / RT**: analog auto-fade toward A / B (tap a face button for an instant hard cut).
- **A / B / X / Y**: the context action cluster (deck: play / cue / mark-in / mark-out; clip: trigger / pattern / assign / auto-bpm).
- **Start**: quit modal. **Back/View**: help. Held bumper = shift layer.

## Acceptance
- [ ] A gamepad is detected/paired (via `gilrs`); hot-plug tolerated; absence is graceful (keyboard still works).
- [ ] Every function is reachable from the pad through the focus -> act model.
- [ ] Right stick gives continuous jog/scratch on the focused deck.
- [ ] The pad mapping mirrors the keyboard model (parity); a config/doc lists both.
- [ ] The mapping layer (pad event -> Action) is unit-tested headlessly (no hardware needed in CI).

## Notes
Depends on the controls refactor (shared model). Xbox is the headline; the same layer can host other XInput pads.
