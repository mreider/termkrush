---
title: 'Bug: loading a full track takes ~60s in dev builds — DSP dependencies (symphonia, rubato/rustfft) compiled unoptimized'
type: bug
created: "2026-06-06T13:17:23Z"
modified: "2026-06-06T13:20:39Z"
author: Matt Reider
status: accepted
started: "2026-06-06T13:20:38Z"
finished: "2026-06-06T13:20:38Z"
delivered: "2026-06-06T13:20:39Z"
accepted: "2026-06-06T13:20:39Z"
project: termkrush
---

## Symptom

After the off-thread fix, loading a full track shows `⏳ loading…` seemingly forever.

## Cause

`scripts/dev-run.sh` builds **debug**, where the audio DSP dependencies run unoptimized. Decoding the user's 140s track measured **61s** in debug vs **0.79s** in release — symphonia (mp3) is slow and rubato/rustfft (resample to the 48 kHz device) is far slower. Off-thread it no longer freezes the UI, but a ~60s decode reads as "never finishes."

## Fix

- `[profile.dev.package."*"] opt-level = 3` (and the same for `profile.test`): compile **dependencies** optimized even in dev/test builds, while our own crate stays unoptimized for fast, debuggable iteration. (Standard for Rust audio/DSP projects.)
- Trimmed the resampler to a 128-tap sinc (from 256) — transparent for playback, ~2x cheaper.

Result: debug decode of the 140s track dropped **61s → 2.4s** (release ~0.4s). Off-thread + the `loading…` indicator, that's a brief, honest wait.

## On WAV vs MP3

WAV would only remove the mp3-decode share (~7s of the 61s); the resample to the device rate dominated. The profile fix addresses the real cause for any format/rate. (A file already at the device rate skips resampling entirely.)

## Verification

- [x] Debug decode 61s → 2.4s (measured on the real 140s track).
- [x] Resample correctness unchanged: `decode_test` (duration ±10ms, RMS within 1%, 44.1→48k) green with the 128-tap sinc.
- [x] fmt / clippy -Dwarnings / full suite green.
