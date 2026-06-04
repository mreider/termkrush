---
title: Audio assertion harness with golden snapshots
type: chore
created: "2026-06-04T09:41:11Z"
modified: "2026-06-04T12:19:08Z"
author: Matt Reider
status: accepted
epic: foundation
tags: [test, dsp, foundation]
project: termkrush
started: "2026-06-04T12:17:13Z"
finished: "2026-06-04T12:19:08Z"
delivered: "2026-06-04T12:19:08Z"
accepted: "2026-06-04T12:19:08Z"
---

## Why this is a chore

Cross-cutting test infrastructure for DSP / playback paths.

## What needs to happen

A helper crate / module that makes audio assertions readable:

- `assert_rms_within(samples, expected, tolerance)`: root-mean-square level within tolerance dB.
- `assert_silent(samples, threshold_db)`: peak below threshold over a window.
- `assert_no_clicks(samples)`: scan for discontinuities greater than N samples wide.
- `golden_snapshot(name, samples)`: write expected output once; subsequent runs diff against it.
- Snapshots stored under `tests/golden/`; one-time refresh via `UPDATE_GOLDEN=1 cargo test`.

## Acceptance

- [ ] Helpers callable from any integration test.
- [ ] At least one smoke test in the foundation crate uses each helper.
- [ ] Golden snapshot framework documented in `tests/README.md`.
- [ ] Failing helpers produce a diagnostic that names the offending sample range.
