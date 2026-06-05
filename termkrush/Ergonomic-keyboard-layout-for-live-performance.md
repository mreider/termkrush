---
title: Ergonomic keyboard layout for live performance
type: feature
created: "2026-06-05T21:18:13Z"
modified: "2026-06-05T21:19:41Z"
author: Matt Reider
status: unstarted
estimate: "3"
epic: controls
project: termkrush
---

## Problem statement

Today's keys are letter-mnemonics scattered across the board (space=play, s=stop, o=open, +/- volume, < > master, [ ] \ crossfader, arrows seek, j/k/Tab/c). For live performance the layout should be built around **finger placement and two-handed muscle memory**, not "what letter the action starts with."

## Possible solution

- A deck-symmetric, home-row-anchored scheme: **left hand drives deck A, right hand drives deck B**, with the crossfader and shared controls on the center keys / space (thumbs). Cluster each hand's transport, cue/seek, and volume so they fall under the resting fingers.
- Keep destructive keys (quit) out of the performance cluster.
- Rebind every action from the two-deck era to the new scheme in one pass; `?` help stays authoritative.

## Acceptance

- [ ] A documented ergonomic key map exists, organized by finger position (left=A, right=B, center/space=crossfader/common) — not by letter mnemonic.
- [ ] Every existing action (play/pause, stop, seek/scrub, deck volume, master, crossfader, deck focus, load, crate filter) is reachable in the new layout.
- [ ] Help overlay + README cheatsheet reflect the new map; `on_key` tests updated to it.
- [ ] No action is bound to a key merely because it matches the action's name.

## Prerequisites

All current keybindings (transport, seek, volume, master, crossfader, focus, crate) — accepted. **This is first in the batch**: the turntable and sampler stories build their keys on this scheme.
