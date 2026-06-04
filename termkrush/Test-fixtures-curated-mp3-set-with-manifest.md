---
title: 'Test fixtures: curated mp3 set with manifest'
type: chore
created: "2026-06-04T09:41:11Z"
modified: "2026-06-04T12:16:29Z"
author: Matt Reider
status: accepted
epic: foundation
tags: [test, fixtures, foundation]
project: termkrush
started: "2026-06-04T12:14:12Z"
finished: "2026-06-04T12:16:29Z"
delivered: "2026-06-04T12:16:29Z"
accepted: "2026-06-04T12:16:29Z"
---

## Why this is a chore

Cross-cutting test infrastructure. No user-facing behavior; supports every feature with audio in it.

## What needs to happen

Curate or generate a small set of audio fixtures the test suite can rely on:

- 5–10 mp3 files, 10–30s each, CC-licensed or synthesized (sine sweeps, drum loops, vocal stabs).
- A fixtures manifest (`tests/fixtures/manifest.toml`) recording per-file: published BPM, key (if known), duration, sample rate, source, license.
- A helper module `tests/common/fixtures.rs` exposing typed handles like `fixtures::HOUSE_128` returning the path.
- If the audio cannot be redistributed (license-wise), a script `scripts/fetch-fixtures.sh` that downloads from a known URL; tests skip with a clear message if absent.

## Acceptance

- [ ] Fixtures present in the repo or fetched by script.
- [ ] Manifest documents each file's BPM/key/duration/license.
- [ ] `cargo test` finds and uses them without external network in the default case.
- [ ] License compatibility verified for each file (no CC-NC for redistributed; all-rights-reserved fetched on demand only).
