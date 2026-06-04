---
title: MP3 encoder
type: feature
created: "2026-06-04T09:20:28Z"
modified: "2026-06-04T09:20:28Z"
author: Matt Reider
status: unstarted
estimate: "5"
epic: record
tags: [record, io, encode]
project: termkrush
---

## Problem statement

WAV is huge. Most users want mp3.

## Possible solution

- Evaluate options: mp3lame-encoder (bindings to libmp3lame) vs shine (pure-Rust, lower quality but no deps).
- Default to libmp3lame at 320kbps CBR; fall back to shine if lame is not buildable on the target.
- Bitrate configurable.

## Acceptance

- [ ] Recording produces a valid mp3 at 320 kbps.
- [ ] mp3 plays in another player and the duration is correct.
- [ ] Build works on all three CI platforms.
