---
title: Whip and wiki scratch primitives
type: feature
created: "2026-06-07T11:10:20Z"
modified: "2026-06-07T11:12:11Z"
author: Matt Reider
status: unstarted
estimate: "5"
project: termkrush
---

## Problem
The two old-school scratch motions, modeled in software around the pivot: **whip** = backward rub with the forward motion muted; **wiki** = forward rub that sounds.

## Acceptance
- [ ] `whip` plays the clip backward from/through the pivot with the forward pass muted (the crossfader-cut sound).
- [ ] `wiki` plays a forward rub that sounds.
- [ ] Both are quantized to the master tempo; engine tests assert direction + the muted-forward behavior.
