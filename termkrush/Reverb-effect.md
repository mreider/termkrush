---
title: Reverb effect
type: feature
created: "2026-06-04T09:20:27Z"
modified: "2026-06-04T09:20:28Z"
author: Matt Reider
status: unstarted
estimate: "3"
epic: fx
tags: [fx, dsp]
project: termkrush
---

## Problem statement

Reverb for breakdowns / vocal flourishes.

## Possible solution

- Simple FDN reverb (or Freeverb-style) — algorithmic, no impulse response loading.
- Two knobs: size and wet.
- CPU budget under 2% per deck.

## Acceptance

- [ ] Engaging reverb audibly adds tail.
- [ ] Size knob changes tail length perceptibly.
- [ ] No clicks engaging/disengaging.
