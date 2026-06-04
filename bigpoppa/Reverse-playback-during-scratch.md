---
title: Reverse playback during scratch
type: feature
created: "2026-06-04T09:20:27Z"
modified: "2026-06-04T09:20:27Z"
author: Matt Reider
status: unstarted
estimate: "3"
epic: scratch
tags: [scratch, dsp]
project: bigpoppa
---

## Problem statement

Scratching includes reversing audio. The decode pipeline currently only plays forward.

## Possible solution

- Negative tempo ratio in the playback engine drives a reverse-read of the decoded buffer.
- A ring buffer of recent samples (e.g. last 4s) supports cheap reverse without re-decoding.
- The jog handler can set tempo to negative values.

## Acceptance

- [ ] Manually setting deck tempo to -1.0 plays the track backwards intelligibly.
- [ ] Reverse during jog scratch sounds correct.
- [ ] Reversing more than 4s back triggers a re-decode without an audible glitch.
