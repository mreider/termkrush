---
title: 'Launch quantization engine: master bar clock and next-bar triggers'
type: feature
created: "2026-06-07T17:52:35Z"
modified: "2026-06-07T17:57:29Z"
author: Matt Reider
status: accepted
estimate: "5"
epic: looper
started: "2026-06-07T17:54:45Z"
delivered: "2026-06-07T17:57:29Z"
accepted: "2026-06-07T17:57:29Z"
project: termkrush
---

## Goal
A master musical clock + launch quantization in the engine, so a triggered pad starts on the next bar line and never mid-bar.

## Spec
- The Mixer tracks a continuous transport position (frames) once master BPM is set; expose current bar/beat and frames-to-next-bar.
- A global Quantize setting (default 1 bar). `trigger_quantized(pad)` defers the voice start to the next quantize boundary — holds it, then starts exactly on the line.
- `fill_mix` releases pending triggers as their boundary passes.
- Headless tests: a trigger placed mid-bar produces no sound until the next bar line, then starts on it (quant = 1 bar); frames-to-next-bar math.
