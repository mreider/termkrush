---
title: Awesome UX revamp functional great-looking turntable interface
type: feature
created: "2026-06-06T09:06:02Z"
modified: "2026-06-06T09:06:36Z"
author: Matt Reider
status: unstarted
estimate: "8"
epic: ux
project: termkrush
---

## Problem statement

The interface works mechanically but isn't yet *awesome* — it needs a deliberate visual + interaction pass so it looks great AND plays great. Open questions to settle: does the crossfader actually feel good and is it too wide? Are the proportions right? Is the focus obvious? Does an empty state guide you? Does it look like gear you'd want to perform on?

## Possible solution (revamp pass)

- **Layout & proportions:** rebalance crate / decks / mixer; right-size the crossfader (currently full-width — likely too wide); make the two turntables the visual anchor.
- **Turntables:** bigger, better platters; clear spin; tonearm / label; per-deck level + BPM legible at a glance.
- **Crossfader & mixer:** a fader that reads and feels like a real crossfader (sensible width, A/B ends, center detent), master + cue legible.
- **Color & hierarchy:** consistent CRT palette, focused deck unmistakable, dim vs active, no cramped/cut text.
- **Functional polish:** confirm crossfader/volume/seek feel right (step sizes, smoothing), empty/loading/error states are clear, help is discoverable.

## Acceptance

- [ ] Side-by-side decks read as turntables and are the focal point at 100x30+.
- [ ] Crossfader is right-sized (not full-width), clearly A↔B with a center mark, and feels good to sweep.
- [ ] Focused deck is unmistakable; levels, BPM, position, and master are legible; no truncated/cramped text.
- [ ] A first-run/empty state clearly tells you how to get tracks playing.
- [ ] It demonstrably looks awesome (screenshot review) and every control still works (covered by the integration tests).

## Prerequisites

Integration tests for every keyboard command (so the revamp can't silently break controls); the turntable / crossfader / keymap stories. Likely **decomposes** into sub-stories during planning — this is the headline.
