---
title: Offline BPM detection on load
type: feature
created: "2026-06-04T09:15:05Z"
modified: "2026-06-06T13:35:05Z"
author: Matt Reider
status: unstarted
estimate: "5"
epic: tempo
tags: [tempo, analysis]
project: termkrush
---

## Problem statement

Sync requires BPM. We need it detected automatically when a track is loaded.

## Possible solution

- Evaluate two options: aubio-rs bindings (Rust wrapper over libaubio) vs pure-Rust onset-detection (bliss-rs or a hand-rolled energy/peak picker).
- Pick the simpler one that gives ±0.5 BPM on a 10-track fixture set of known house/techno tracks.
- Run detection in a background task on load so UI does not block.

## Acceptance

- [ ] All 10 fixture tracks detect within ±0.5 BPM of their published BPM.
- [ ] Detection completes in under 2s per track on a modern laptop.
- [ ] Detection runs off the audio thread; UI stays responsive.
- [ ] Detection result is logged.
