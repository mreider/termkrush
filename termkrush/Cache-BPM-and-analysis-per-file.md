---
title: Cache BPM and analysis per file
type: feature
created: "2026-06-04T09:15:06Z"
modified: "2026-06-06T19:57:38Z"
author: Matt Reider
status: accepted
estimate: "2"
epic: tempo
tags: [tempo, cache]
project: termkrush
started: "2026-06-06T19:55:39Z"
finished: "2026-06-06T19:57:38Z"
delivered: "2026-06-06T19:57:38Z"
accepted: "2026-06-06T19:57:38Z"
---

## Problem statement

Re-running BPM detection every load is wasteful. Persist results.

## Possible solution

- Cache file at ~/.termkrush/cache/<sha1-of-abspath>.json with bpm, sample_rate, duration, analyzed_at, version.
- Invalidate when file mtime changes.
- Cache version constant bumped when the detector changes.

## Acceptance

- [ ] First load of a track writes a cache entry.
- [ ] Subsequent loads of the same unchanged file read from cache (<10ms).
- [ ] Touching the source file invalidates the cache.
