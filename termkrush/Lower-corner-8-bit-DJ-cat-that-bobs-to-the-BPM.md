---
title: Lower-corner 8-bit DJ cat that bobs to the BPM
type: feature
created: "2026-06-05T21:18:14Z"
modified: "2026-06-05T21:19:41Z"
author: Matt Reider
status: unstarted
estimate: "3"
epic: mascot
project: termkrush
---

## Problem statement

Want a small, charming 8-bit DJ cat in a lower corner that moves slightly to the beat — **way less detail** than the reference art.

## Possible solution

- A tiny (~10x6 cell) cat drawn with block/half-block glyphs and the reference palette (ginger fur, red Kangol hat, teal headphones).
- A 2-frame bob driven by a beat clock derived from the **playing deck's BPM + playhead**; subtle motion (a row or two) on the beat / every few beats.
- Lives in a lower corner; never overlaps the deck/crate panels; idles when nothing is playing.

## Acceptance

- [ ] A small DJ cat renders in a lower corner at 100x30 without overlapping the deck/crate panels.
- [ ] It bobs on the beat (beat clock from the playing deck's BPM); motion is subtle and periodic (every beat or few beats).
- [ ] Idles (no bob) when no track is playing or no BPM is known yet; degrades gracefully.
- [ ] Far less detail than the reference — a stylized few-cell cat, not the 96x84 sprite.

## Prerequisites

Offline BPM detection on load (the beat source — currently in progress), transport/position (accepted). Reference (palette + pose only): `~/Downloads/I need a simple 8 bit dj cat/DJ Cat.html`.
