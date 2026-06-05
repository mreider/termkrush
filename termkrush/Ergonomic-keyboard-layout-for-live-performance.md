---
title: Ergonomic keyboard layout for live performance
type: feature
created: "2026-06-05T21:18:13Z"
modified: "2026-06-05T21:30:49Z"
author: Matt Reider
status: accepted
estimate: "3"
epic: controls
project: termkrush
started: "2026-06-05T21:23:50Z"
finished: "2026-06-05T21:30:48Z"
delivered: "2026-06-05T21:30:49Z"
accepted: "2026-06-05T21:30:49Z"
---

## Problem statement

Today's keys are letter-mnemonics scattered across the board (space=play, s=stop, o=open, +/- volume, < > master, [ ] \ crossfader, arrows seek, j/k/Tab/c). For live performance the layout should be built around **finger placement and two-handed muscle memory**, not "what letter the action starts with."

## Possible solution

- Deck-symmetric, home-row-anchored: **left hand drives deck A, right hand drives deck B**, crossfader between the hands, globals off the play cluster.

## Acceptance

- [x] A documented ergonomic key map exists, organized by finger position (left=A, right=B, crossfader between) — not by letter mnemonic.
- [x] Every existing action is reachable: play/pause (f/j), cue-stop (d/k), volume (w·s / o·l), seek (e·r / i·u, shift=far), crossfader (g/h, space=center), master ([ / ]), fine scrub (, / . on focused), deck focus (tab), crate (/ filter, ↑/↓, enter), demo (\), hide crate (z).
- [x] Help overlay + README cheatsheet reflect the new map; `on_key` tests updated to it (109 tests green).
- [x] No action is bound to a key because it matches the action's name.

## Implementation notes

- Transport is now applied **directly to deck A or deck B** by which hand's key you press (not the focused deck). `focus` (tab) now only steers crate loads and fine scrub, plus the visual highlight.
- Finger logic: index home = play (f/j), middle home = cue (d/k), ring column = volume (w·s / o·l), index/middle top row = seek (e·r / i·u); the two index inner-reach keys g/h are the crossfader, space (thumb) recenters.
- `q`/`Ctrl-C` quit stay off the play cluster; `\` loads the demo (was `o`); `z` hides the crate (was `c`).
- This is the keymap the turntable + sampler stories build on (why it ranked first).

## Note (review)

Ergonomics verified by construction + the full `on_key` test suite; the *feel* of the layout is best confirmed by playing it (`scripts/dev-run.sh tui`). The scheme is opinionated — easy to tweak individual keys later since they're all in one match.
