---
title: Symphonia mp3 decode pipeline
type: feature
created: "2026-06-04T09:11:00Z"
modified: "2026-06-04T09:11:00Z"
author: Matt Reider
status: unstarted
estimate: "3"
epic: one-deck
tags: [audio, one-deck]
project: termkrush
---

## Problem statement

Need to turn an mp3 file on disk into a stream of f32 stereo samples that the mixer can consume.

## Possible solution

- `audio/decode.rs` using `symphonia` with the mp3 feature.
- Open file, probe, find the default audio track, build a decoder.
- Wrap as an iterator of `AudioBuffer<f32>` blocks, downmix mono→stereo, resample to the output rate if needed (`rubato`).
- Surface metadata: duration, sample rate, channels, ID3 title/artist.

## Acceptance

- [ ] Loading a fixture mp3 yields the expected duration ±10ms.
- [ ] Decoded RMS for a known fixture is within 1% of a reference value (integration test).
- [ ] Mono mp3s come out stereo.
- [ ] 48 kHz output works when the source is 44.1 kHz (resampled).
