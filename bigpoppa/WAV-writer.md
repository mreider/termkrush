---
title: WAV writer
type: feature
created: "2026-06-04T09:20:28Z"
modified: "2026-06-04T09:20:28Z"
author: Matt Reider
status: unstarted
estimate: "2"
epic: record
tags: [record, io]
project: bigpoppa
---

## Problem statement

Need to write the tapped audio to disk as WAV first (encoder-free, simplest format).

## Possible solution

- hound crate.
- 16-bit PCM stereo at output sample rate.
- File path under ~/Music/bigpoppa/_recordings/ named by timestamp.

## Acceptance

- [ ] Recording for 30s produces a 30s WAV at the configured sample rate.
- [ ] WAV plays correctly in another player.
- [ ] Stopping recording flushes and closes the file (verifiable size in header).
