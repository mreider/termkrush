---
title: Engine-placed scratches and fader chops with human micro-timing
type: feature
created: "2026-06-11T13:33:04Z"
modified: "2026-06-12T06:34:50Z"
author: Matt Reider
status: started
estimate: "8"
project: termkrush
started: "2026-06-11T19:19:14Z"
---

## Goal

The product's signature. The engine performs the scratching: whip/wiki flurries and fader chops built from the sequence's own tracks, placed like a human — **macro quantized, micro loose**.

Reference grammar: scratch passages are 1–2 s flurries, clustered into a stretch of the mix, starting near beat 2 more often than the downbeat; fader chops are ~50 ms; neither is quantized to 16ths internally (offsets at chance level) — that looseness is what reads as human.

## Engine spec

- **Material**: onset-rich short slices picked deterministically from the sequence's tracks; the surviving whip/wiki scratch voice DSP in core renders the rubs (whip = muted forward, wiki = sounding forward).
- **Placement**: flurry *starts* lock to the bar/beat grid (lean to beat 2); internal whip/wiki timing is jittered from the input-derived seed and deliberately **not** snapped to 16ths. Chops (~50 ms gain cuts) likewise.
- **Density**: a handful of flurries per rendered hour, clustered (not uniform), scaled to mix length.

## Acceptance

- Placement test: flurry starts land on the grid; internal event offsets are non-quantized (distance-to-nearest-16th distribution near chance) yet identical across runs.
- Render test: flurry regions show the scratch signature (alternating direction playback from the chosen slice); chop regions show ~50 ms gain cuts.
- Clustering test: flurries concentrate in a contiguous stretch rather than spreading uniformly.

## Comments

## Attachments

## Rejection notes

- 2026-06-12: PM: no audible scratching in a 4:20 mix — density floor too low (1 flurry, skippable to 0), and flurries are level-matched under the bed so they mask.
