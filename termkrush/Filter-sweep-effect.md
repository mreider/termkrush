---
title: Filter sweep effect
type: feature
created: "2026-06-04T09:20:27Z"
modified: "2026-06-04T09:20:27Z"
author: Matt Reider
status: unstarted
estimate: "3"
epic: fx
tags: [fx, dsp]
project: termkrush
---

## Problem statement

The single most-used DJ effect: filter sweep.

## Possible solution

- 2-pole resonant low-pass / high-pass (state-variable filter).
- Single knob in the UI: 1 selects LP mode, 2 selects HP mode, arrow keys move cutoff.
- Default Q tuned for sweeps (no self-oscillation).

## Acceptance

- [ ] Sweeping low-pass from open to closed audibly removes highs.
- [ ] High-pass mode audibly removes lows.
- [ ] No instability across the range; CPU under 1% per deck.
