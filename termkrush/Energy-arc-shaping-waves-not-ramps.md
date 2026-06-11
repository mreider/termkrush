---
title: 'Energy-arc shaping: waves not ramps'
type: feature
created: "2026-06-11T13:33:15Z"
modified: "2026-06-11T19:37:57Z"
author: Matt Reider
status: started
estimate: "5"
project: termkrush
started: "2026-06-11T19:37:57Z"
---

## Goal

Good mixes breathe. Reference grammar: the loudness arc oscillates between ~0.4 and ~0.7 of peak on a ~6–8-minute cycle (it never just ramps), and the low/high spectral balance warms over the back half.

## Engine spec

- A mix-level planner shapes the seeded decisions the earlier stories made locally: section choice within each track (louder vs quieter material), per-section gain offsets, chop/flurry density, and drop placement all bend toward a target arc — waves with a ~6–8 min period inside the ~0.4–0.7 envelope, scaled to mix length.
- A gentle low-end tilt rises over the back half of the mix.
- The user's track order is never changed; the arc is achieved within it.

## Acceptance

- Render test on a long fixture mix: the smoothed RMS arc shows oscillation (multiple peaks/troughs inside the envelope), not a monotonic ramp; the low/high band ratio in the final third exceeds the first third.
- Determinism: identical arc across runs for the same input.

## Comments

## Attachments
