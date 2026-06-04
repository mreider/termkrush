---
title: Per-deck and master volume
type: feature
created: "2026-06-04T09:11:00Z"
modified: "2026-06-04T09:11:00Z"
author: Matt Reider
status: unstarted
estimate: "2"
epic: one-deck
tags: [deck, mix, one-deck]
project: bigpoppa
---

## Problem statement

Gain control is non-negotiable for mixing. Need per-deck volume now, plus a master.

## Possible solution

- `Deck::gain: f32` (0.0 .. 1.5; 1.0 = unity, headroom to +3.5 dB).
- `Mixer::master: f32`.
- TUI: `+` / `-` on focused deck nudges by 0.05; `Shift +` / `Shift -` for master.
- A tiny dB readout per deck.

## Acceptance

- [ ] +/- on a deck audibly changes its level without zipper noise (smoothed).
- [ ] Master gain changes all decks simultaneously.
- [ ] dB readout updates in real-time and matches the gain value.
