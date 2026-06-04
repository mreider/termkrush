---
title: Master tap into ring buffer
type: feature
created: "2026-06-04T09:20:28Z"
modified: "2026-06-04T09:20:28Z"
author: Matt Reider
status: unstarted
estimate: "3"
epic: record
tags: [record, architecture]
project: bigpoppa
---

## Problem statement

Recording requires a copy of the final mix bus. Need a tap that does not interfere with playback.

## Possible solution

- Insert a tap on the mixer output (after master gain, before sending to cpal).
- Sample-accurate copy into a lock-free ring buffer drained by a writer thread.
- No allocations / no IO on the audio thread.

## Acceptance

- [ ] Recording on does not increase audio thread CPU by more than 0.5%.
- [ ] Buffer underruns are logged but not audible.
- [ ] Tap can be enabled/disabled without clicks.
