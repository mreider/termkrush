---
title: Per-deck track switching from the loaded list
type: feature
created: "2026-06-05T21:18:13Z"
modified: "2026-06-05T21:39:33Z"
author: Matt Reider
status: accepted
estimate: "3"
epic: turntables
project: termkrush
started: "2026-06-05T21:35:45Z"
finished: "2026-06-05T21:39:33Z"
delivered: "2026-06-05T21:39:33Z"
accepted: "2026-06-05T21:39:33Z"
---

## Problem statement

Need an easy way to choose which loaded track sits on each turntable: toggle the active deck and swap that deck's track from a list of loaded tracks, without disturbing the other deck.

## Possible solution

- A "loaded tracks" shortlist (tracks pulled this session) to pick from.
- Per-deck loading targets the focused deck; deck-toggle (tab) already exists.
- Swapping deck A leaves deck B playing.

## Acceptance

- [x] Toggle which deck is focused, clearly indicated. (`tab` cycles focus; the `▸` marker + amber border show it — from the keymap + platter stories.)
- [x] Load/replace the focused deck's track by picking from the list (crate `enter`); the other deck keeps playing uninterrupted. (Load targets `mixer.deck_mut(focus)`; decks are independent — `decks_are_independent`, `each_hand_drives_its_own_deck_independently`.)
- [x] A short loaded/recent-tracks list is shown to pick from. (New "Loaded" panel under the crate; `loaded_panel_lists_recent_tracks`, `note_loaded_dedups_caps_and_orders`.)

## Implementation notes

- Most of the *switching* mechanics were already in place: `tab` focus (keymap), crate `↑/↓` + `enter` loads into the focused deck (crate-view), and loading one deck never touches the other (Mixer owns independent decks). This story adds the **session "Loaded" shortlist** and verifies the flow.
- `App.recent` (most-recent-first, de-duplicated, capped at 6) is updated by the event loop after each *successful* load (`load_into` now returns whether the decode succeeded). Rendered as a bordered "Loaded" panel beneath the crate browser.

## Note (review)

Loading-into-focused and deck independence are exercised at the deck/mixer level (the actual decode runs in the event loop, which isn't unit-tested). The shortlist is read-only for now — re-picking from it routes through the crate `enter` path; a dedicated "load Nth recent" key can come later (kept off the number row, which the clip-pads story will use).
