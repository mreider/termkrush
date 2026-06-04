---
title: Per-deck FX bus
type: feature
created: "2026-06-04T09:20:27Z"
modified: "2026-06-04T09:20:27Z"
author: Matt Reider
status: unstarted
estimate: "3"
epic: fx
tags: [fx, architecture]
project: termkrush
---

## Problem statement

Need an architecture before individual FX so that adding more is cheap.

## Possible solution

- Per-deck FX chain: ordered list of Effect trait objects with a process method.
- Wet/dry per slot.
- TUI: small FX row per deck panel showing which effects are on and their wet level.
- One effect ships with this story as a smoke test: a unity-gain pass-through effect to prove the chain works.

## Acceptance

- [ ] The pass-through chain leaves audio unchanged.
- [ ] Adding/removing the pass-through from the chain has no audible effect but changes the FX row display.
- [ ] No allocations in the audio callback during effect processing.
