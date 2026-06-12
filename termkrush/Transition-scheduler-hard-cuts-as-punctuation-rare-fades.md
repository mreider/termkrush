---
title: 'Transition scheduler: hard cuts as punctuation, rare fades'
type: feature
created: "2026-06-11T13:32:58Z"
modified: "2026-06-12T07:17:52Z"
author: Matt Reider
status: delivered
estimate: "3"
project: termkrush
started: "2026-06-11T19:10:36Z"
delivered: "2026-06-12T07:17:52Z"
---

## Goal

Move from "playlist" toward "mix": vary how sections hand over. Reference grammar: of 91 transitions, the dominant move is the equal-loudness swap; ~¼ are hard cuts used as punctuation; ramped fades are rare (~1 in 20).

## Engine spec

- An input-seeded schedule assigns each boundary a transition type: swap (default), hard cut (~25%), short fade (~5%).
- Hard cuts may step level (>6 dB allowed) for punctuation; fades are musical lengths (beats, not seconds); everything stays on the phrase boundary.
- Deterministic: same sequence → same schedule.

## Acceptance

- Unit test: over a long synthetic sequence the schedule's type distribution lands within tolerance of 70/25/5 and is identical across runs.
- Render test: cut boundaries show the level step in the output; fade boundaries show a monotonic ramp of the scheduled beat-length; all boundaries remain grid-aligned.

## Comments

## Attachments

## Rejection notes

- 2026-06-12: PM: no fades heard — 5% over ~4 boundaries rounds to never; short mixes need guaranteed variety.
