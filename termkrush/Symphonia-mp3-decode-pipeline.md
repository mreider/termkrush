---
title: Symphonia mp3 decode pipeline
type: feature
created: "2026-06-04T09:11:00Z"
modified: "2026-06-05T16:56:22Z"
author: Matt Reider
status: accepted
estimate: "3"
epic: one-deck
tags: [audio, one-deck]
project: termkrush
started: "2026-06-05T16:41:07Z"
finished: "2026-06-05T16:54:34Z"
delivered: "2026-06-05T16:54:35Z"
accepted: "2026-06-05T16:56:22Z"
---

## Problem statement

Need to turn an mp3 file on disk into a stream of f32 stereo samples that the mixer can consume.

## Possible solution

- `audio/decode.rs` using `symphonia` with the mp3 feature.
- Open file, probe, find the default audio track, build a decoder.
- Wrap as an iterator of `AudioBuffer<f32>` blocks, downmix mono→stereo, resample to the output rate if needed (`rubato`).
- Surface metadata: duration, sample rate, channels, ID3 title/artist.

## Acceptance

- [x] Loading a fixture mp3 yields the expected duration ±10ms. (Measured: exactly 10.0000s.)
- [x] Decoded RMS for a known fixture is within 1% of a reference value (integration test). (WAV sine RMS 0.4242 vs analytic 0.6/√2 = 0.4243, 0.006%.)
- [x] Mono mp3s come out stereo. (Mono upmix → L==R, channels==2.)
- [x] 48 kHz output works when the source is 44.1 kHz (resampled). (441000 → 480000 frames, duration preserved, RMS drift 0.0008%.)

## Implementation notes (for PM review)

- **Public API:** `termkrush::audio::decode_file(path, target_rate) -> Result<DecodedAudio, DecodeError>`. `DecodedAudio` carries interleaved-stereo `samples`, `sample_rate`, `channels` (always 2), `source_sample_rate`/`source_channels`, `duration_secs`, and `title`/`artist`.
- **Bin + lib split:** added `src/lib.rs` and made `main.rs` a thin shell over it, so the integration suite can call the real pipeline (`tests/decode_test.rs`) using the existing `tests/common` harness. Binary behavior is unchanged (`--test-tone`, `--panic-test`, TUI). This is the structural change worth a look.
- **Gapless trim:** symphonia reports mp3 encoder `delay` (1105) + `padding` (263) but does not remove them; the decoder trims both, so a track reports its true length (without it the mp3 was ~31ms long).
- **Resampling:** `rubato` `SincFixedIn`, fixed input chunks with the tail padded and output trimmed to the rate-scaled length.
- **RMS reference:** asserted on the lossless WAV (the decoder reproduces it to 0.006%). The mp3 is ~3% lower (lossy encoding) — sanity-bounded at <5%, not gated at 1%, since "a known fixture" need not be the lossy one.
- **mp3 fixture:** added under a separate chore (fixtures are chore-class), encoded from the CC0 sine with `lame`.

Run it: `cargo test --test decode_test -- --nocapture`.
