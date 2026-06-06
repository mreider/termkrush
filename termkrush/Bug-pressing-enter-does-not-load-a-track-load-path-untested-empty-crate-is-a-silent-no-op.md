---
title: 'Bug: pressing enter does not load a track (load path untested, empty crate is a silent no-op)'
type: bug
created: "2026-06-06T09:06:02Z"
modified: "2026-06-06T09:11:19Z"
author: Matt Reider
status: accepted
project: termkrush
started: "2026-06-06T09:11:19Z"
finished: "2026-06-06T09:11:19Z"
delivered: "2026-06-06T09:11:19Z"
accepted: "2026-06-06T09:11:19Z"
---

## Symptom

Pressing `enter` does nothing — no track loads. Tests were green, so the failure was invisible.

## Cause

Two compounding issues: (1) `crate_root` defaulted to `~/Music/termkrush` (nonexistent), so the crate was empty and `enter` had nothing to select → silent `Action::None`; (2) the real load path (event loop → `decode_file` → `deck.load`) was never integration-tested, so a no-op load couldn't be caught.

## Fix

- Load step lifted out of `event_loop` into the testable `apply_load_action` and proven end-to-end: `enter` on a populated crate decodes the real fixture and lands it on the focused deck, and the loaded track plays audible output (integration-tests chore).
- Empty-crate state now shows wrapped, actionable guidance ("set crate_root in your config.toml") instead of a silent/truncated line (crate-truncation bug).
- Empty crate `enter` is a safe no-op (`enter_on_empty_crate_is_noop`).

## Verification

- [x] End-to-end: populated crate → `enter` → focused deck holds the decoded track (`enter_loads_the_selected_track_end_to_end`).
- [x] Loading works in-app once `crate_root` is configured (verified: config → scan finds the 2 tracks; the same `apply_load_action`/`load_into` path the event loop runs is covered).
- [x] cargo test green (96 lib + integration).
