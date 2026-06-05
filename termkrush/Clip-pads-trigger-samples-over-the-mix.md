---
title: Clip pads trigger samples over the mix
type: feature
created: "2026-06-05T21:18:14Z"
modified: "2026-06-05T21:19:41Z"
author: Matt Reider
status: unstarted
estimate: "5"
epic: sampler
project: termkrush
---

## Problem statement

Want programmable buttons that play short clips on demand, mixed on top of whatever the decks are playing — DJ stabs / one-shots.

## Possible solution

- N assignable pads; each holds a decoded clip. Pressing a pad triggers **one-shot** playback summed into the master alongside the decks.
- Overlapping triggers mix (polyphonic); a finished one-shot frees its voice.
- Pad keys sit under the ergonomic layout; pad state shown in the UI.
- Extends `Mixer` to sum sampler voices next to the decks (the mixer already owns the summing path).

## Acceptance

- [ ] At least 4 pads, each assignable to a clip (from the crate / loaded list).
- [ ] Pressing a pad plays its clip once, mixed atop the decks, without interrupting deck playback.
- [ ] Overlapping triggers mix; a finished one-shot releases its voice.
- [ ] Pad keys fit the ergonomic layout; pad state is visible.

## Prerequisites

Mixer summing (Deck B mirror / Crossfader, accepted), decode pipeline (accepted), Ergonomic keyboard layout. Relates to the iceboxed "Sampler pads" / "DJ stabs sampler bank" — this is the concrete v1.
