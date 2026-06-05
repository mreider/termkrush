---
title: Crossfader between the turntables
type: feature
created: "2026-06-05T21:18:14Z"
modified: "2026-06-05T21:19:41Z"
author: Matt Reider
status: unstarted
estimate: "2"
epic: turntables
project: termkrush
---

## Problem statement

The crossfader audio path is done (Crossfader audio mix, accepted), but it should sit **visually between the two platters** and read as crossing A↔B in the turntable view.

## Possible solution

- Render the crossfader fader graphic centered between the deck platters, reflecting the current position.
- Moved by the ergonomic crossfader keys; the existing audio behavior is unchanged.

## Acceptance

- [ ] Crossfader graphic sits between the two turntables and shows the current position.
- [ ] The ergonomic crossfader keys move it; centering works; audio behavior unchanged from the accepted crossfader story.

## Prerequisites

Turntable platter visuals; Crossfader audio mix (accepted); Ergonomic keyboard layout.
