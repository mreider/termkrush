---
title: Split a headless core from the TUI shell and remove decks
type: feature
created: "2026-06-07T11:09:28Z"
modified: "2026-06-07T11:18:24Z"
author: Matt Reider
status: started
estimate: "5"
started: "2026-06-07T11:18:24Z"
---

## Problem
The code is built around two decks and a fat `App` in `tui/mod.rs` that mixes state, logic, audio plumbing, and rendering. The pivot removes decks entirely and needs a headless, directly-testable core split from a thin UI shell.

## Acceptance
- [ ] A headless **core** (its own crate, `termkrush-core`) holds all state + logic and imports **no** UI (ratatui/crossterm/gilrs).
- [ ] The **shell** (bin/`tui`) only maps input → core commands and renders core state → frames.
- [ ] The `deck` module and all deck UI/state are removed; the app still builds, runs, and renders (track list + pad bank + DJ).
- [ ] Core logic is unit-testable by calling methods directly; surviving engine tests (clip, mixer voices, library, bpm) are ported and green.
