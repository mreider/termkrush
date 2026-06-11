---
title: 'Naive auto-mix render: phrase sections varispeeded and butt-joined on the master grid'
type: feature
created: "2026-06-11T13:32:52Z"
modified: "2026-06-11T13:37:31Z"
author: Matt Reider
status: unstarted
estimate: "8"
project: termkrush
---

## Goal

**The value seam.** With a sequence whose entries all have beat marks, the Render button produces a seamless, beat-locked WAV mix — no scratches, no drops yet, but already a listenable mixtape the PM can A/B against real mixes.

Reference grammar (measured from the 2026-06-11 reference-mix analysis): one master tempo for the whole mix; sections of 8–16 bars (median ~15, occasionally up to 32); boundaries on phrase positions; equal-loudness swaps (median step 0 dB).

## Engine spec

- **Master tempo**: the first entry's fitted tempo. Every section varispeeds to it (pitch rides; existing varispeed engine).
- **Section picker**: per entry, deterministically choose 8–16 phrase-aligned bars from that track (aligned to the track's own tapped downbeats). Repeat entries of the same track must pick different material.
- **Loudness matching**: all sections normalized to a shared loudness target at analysis time; per-section gain is an offset from that.
- **Assembly**: sections butt-joined on the master grid — each starts exactly where the previous ends, on a phrase boundary. Output WAV lands in the library like any track.
- **Determinism**: all choices seeded from the input (track content + order + beat marks); no wall clock, no unseeded randomness.

## Acceptance

- Integration test: fixture tracks with known marks → rendered output asserts on duration (sum of chosen sections), boundary positions on the grid, and per-section RMS within tolerance of the target.
- Repeat-entry test: same track twice in the sequence yields two different sections.
- Same input rendered twice → identical output bytes (local check; cross-platform hardening is its own story).

## Comments

## Attachments
