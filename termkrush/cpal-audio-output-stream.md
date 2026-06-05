---
title: Cpal audio output stream
type: feature
created: "2026-06-04T09:11:00Z"
modified: "2026-06-05T16:21:43Z"
author: Matt Reider
status: accepted
estimate: "3"
epic: one-deck
tags: [audio, one-deck]
project: termkrush
started: "2026-06-04T12:41:21Z"
delivered: "2026-06-04T13:28:08Z"
accepted: "2026-06-05T16:21:43Z"
---

## Problem statement

No audio output yet. Need a working `cpal` stream into which the mixer can write samples.

## Possible solution

- `audio/output.rs`: own the default output device, build a stream at 44.1 kHz / 48 kHz / system rate, stereo f32.
- Lock-free ringbuffer (e.g. `rtrb`) from mixer thread to audio callback.
- `audio::Sink` trait that the (future) mixer implements; the callback pulls fixed-size frames.
- Audible smoke test: feed a 440 Hz sine to the sink, hear a tone on `termkrush --test-tone` (default 2s; pass seconds to extend, e.g. `--test-tone 10`).

## Acceptance

- [ ] `termkrush --test-tone` produces an audible 440 Hz sine on default output. _(stream opens 48 kHz/2ch/F32 and feeds cleanly — needs a human ear to confirm audible.)_
- [x] No xruns or panics under 10s of continuous output. _(`--test-tone 10`: `xruns:0`, exit 0.)_
- [x] Sample rate is logged at startup. _(`audio: opening output stream sample_rate=48000 channels=2 format=F32`.)_
- [ ] Disconnecting the output device does not crash (graceful error in log). _(`start()` returns `AudioError` and the stream error callback logs rather than panicking; physical unplug unverified — for PM check.)_
