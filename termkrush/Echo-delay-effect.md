---
title: Echo delay effect
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

Tempo-synced echo for transitions and pads.

## Possible solution

- Single-tap delay locked to deck BPM (1/4, 1/2, 1, 2 beats — selectable).
- Feedback 0..0.85.
- Wet/dry.

## Acceptance

- [ ] Engaging echo at 1/4 beat produces audible quarter-note repeats.
- [ ] Feedback maxes at 0.85 (no runaway).
- [ ] Disabling echo cleanly tails out (no hard cutoff).
