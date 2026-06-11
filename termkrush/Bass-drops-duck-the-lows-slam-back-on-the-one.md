---
title: 'Bass drops: duck the lows, slam back on the one'
type: feature
created: "2026-06-11T13:33:10Z"
modified: "2026-06-11T13:37:57Z"
author: Matt Reider
status: unstarted
estimate: "3"
project: termkrush
---

## Goal

The classic tension move, automated. Reference grammar: 16 events per hour where the low band drops >10 dB below its local level for 1–16 s, then returns hard.

## Engine spec

- The seeded plan places drop events (~16 per rendered hour, scaled to mix length) at tension points (favoring positions late in a phrase, before a section boundary).
- During a drop the master low band (<~150 Hz) ducks ≥10 dB; the duck starts bar-quantized and the restore lands exactly on a downbeat ("back on the one").
- Implemented as low-shelf/HPF gain automation in the render path; deterministic like everything else.

## Acceptance

- Render test: low-band RMS at scheduled drop windows is ≥10 dB below the surrounding low-band level; full level returns within one beat of the scheduled downbeat.
- Schedule test: event count scales with mix length and is identical across runs.

## Comments

## Attachments
