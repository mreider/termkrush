---
title: Cpal audio output stream
type: feature
created: "2026-06-04T09:11:00Z"
modified: "2026-06-04T09:11:00Z"
author: Matt Reider
status: unstarted
estimate: "3"
epic: one-deck
tags: [audio, one-deck]
project: bigpoppa
---

## Problem statement

No audio output yet. Need a working `cpal` stream into which the mixer can write samples.

## Possible solution

- `audio/output.rs`: own the default output device, build a stream at 44.1 kHz / 48 kHz / system rate, stereo f32.
- Lock-free ringbuffer (e.g. `rtrb`) from mixer thread to audio callback.
- `audio::Sink` trait that the (future) mixer implements; the callback pulls fixed-size frames.
- Audible smoke test: feed a 440 Hz sine to the sink, hear a tone for 1 second on `big-poppa --test-tone`.

## Acceptance

- [ ] `big-poppa --test-tone` produces an audible 440 Hz sine on default output.
- [ ] No xruns or panics under 10s of continuous output.
- [ ] Sample rate is logged at startup.
- [ ] Disconnecting the output device does not crash (graceful error in log).
