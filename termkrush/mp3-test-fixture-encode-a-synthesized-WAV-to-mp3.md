---
title: 'Mp3 test fixture: encode a synthesized WAV to mp3'
type: chore
created: "2026-06-05T16:32:11Z"
modified: "2026-06-05T16:40:45Z"
author: Matt Reider
status: accepted
started: "2026-06-05T16:32:28Z"
finished: "2026-06-05T16:40:45Z"
delivered: "2026-06-05T16:40:45Z"
accepted: "2026-06-05T16:40:45Z"
---

## Why this is a chore

Reusable test rigging, no end-user feature. The `Symphonia mp3 decode pipeline`
story (and future ID3/metadata tests) need a real `.mp3` to decode, but the
fixture set is all WAV — `gen-fixtures.sh` chose WAV because the project had no
mp3 encoder. Per the project rule, cross-cutting test infra is a chore, not
folded into the feature. This adds one deterministic mp3 fixture so the decode
feature can assert against an actual mp3.

## What needs to happen

- Extend `scripts/gen-fixtures.sh` to encode a synthesized source WAV to mp3
  with `lame` (CBR, fixed settings), producing `tests/fixtures/sine_a440_10s.mp3`
  from the existing CC0 sine. mp3 patents expired in 2017; the synthesized
  source is CC0, so the mp3 is CC0 too.
- Commit the encoded `.mp3` bytes (mp3 encoders are not guaranteed
  byte-identical across `lame` versions, so the committed file is the source of
  truth; the generator documents how it was produced — best-effort regenerate).
- Add a `[[fixture]]` entry to `tests/fixtures/manifest.toml` with
  `format = "mp3"`, duration, sample rate, channels, license.

## Acceptance

- [ ] `tests/fixtures/sine_a440_10s.mp3` exists and is a valid mp3 (decodes).
- [ ] `manifest.toml` has an mp3 entry; the fixtures presence test passes.
- [ ] `gen-fixtures.sh` documents the exact lame invocation.
