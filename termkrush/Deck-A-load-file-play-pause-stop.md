---
title: 'Deck A: load file, play, pause, stop'
type: feature
created: "2026-06-04T09:11:00Z"
modified: "2026-06-04T09:11:00Z"
author: Matt Reider
status: unstarted
estimate: "3"
epic: one-deck
tags: [deck, one-deck]
project: termkrush
---

## Problem statement

A `Deck` is the fundamental unit. v1 has one: load a file, hit play, hit pause, hit stop.

## Possible solution

- `deck/mod.rs` with a `Deck` struct: source, position, state (Loaded / Playing / Paused / Stopped), gain.
- Pull-based: the mixer asks each deck for N samples; deck returns silence when stopped/paused.
- Hotkeys (bound in TUI): `[space]` play/pause on focused deck, `s` stop, `o` open file dialog (placeholder dialog for now — just hard-code a path until library view lands).

## Acceptance

- [ ] Spacebar toggles play/pause on the loaded track and audio matches state.
- [ ] Stop resets the playhead to 0 and silences the deck.
- [ ] Re-pressing play after stop starts from the beginning.
- [ ] Position advances at real-time speed (verified with a 10s fixture).
