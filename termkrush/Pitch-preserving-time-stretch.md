---
title: Pitch-preserving time-stretch
type: feature
created: "2026-06-04T09:15:06Z"
modified: "2026-06-04T09:15:06Z"
author: Matt Reider
status: unstarted
estimate: "5"
epic: tempo
tags: [tempo, dsp]
project: termkrush
---

## Problem statement

To match BPMs without chipmunk artifacts, we need pitch-preserving time-stretch in the playback path.

## Possible solution

- Insert a time-stretcher between decode and the mixer: rubato (FFT-based) or a phase-vocoder crate.
- Parameter: tempo ratio (e.g. 0.92 .. 1.08 covers ±8%).
- Quality target: no obvious smearing on a vocal track at ±6%.
- Allocate enough latency budget; the audio callback must stay within deadline at 256-sample buffers.

## Acceptance

- [ ] Setting deck tempo ratio to 1.05 raises tempo by 5% without changing pitch (audible test).
- [ ] No xruns at 256-sample buffer on macOS arm64.
- [ ] Round-trip 1.0 ratio is bit-identical (or within noise floor) compared to bypassed path.
