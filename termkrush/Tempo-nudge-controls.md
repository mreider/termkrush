---
title: Tempo nudge controls
type: feature
created: "2026-06-04T09:15:06Z"
modified: "2026-06-04T09:15:06Z"
author: Matt Reider
status: unstarted
estimate: "2"
epic: tempo
tags: [tempo, tui]
project: termkrush
---

## Problem statement

Beyond sync, DJs hand-nudge tempo to ride a beat. Expose that.

## Possible solution

- Hotkeys on focused deck: Ctrl-+ / Ctrl-- nudge tempo by ±1 BPM; Ctrl-Shift-+/- nudges by ±0.1 BPM.
- Visual: nudge readout next to BPM, shows the adjusted BPM.
- Holding the key auto-repeats with acceleration.

## Acceptance

- [ ] Nudge changes effective playback tempo audibly.
- [ ] Nudge readout updates each press.
- [ ] Released key holds the new tempo.
