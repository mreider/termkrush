---
title: Deck B mirror of Deck A
type: feature
created: "2026-06-04T09:11:01Z"
modified: "2026-06-05T18:43:43Z"
author: Matt Reider
status: accepted
estimate: "2"
epic: two-decks
tags: [deck, two-decks]
project: termkrush
started: "2026-06-05T18:37:44Z"
finished: "2026-06-05T18:43:42Z"
delivered: "2026-06-05T18:43:42Z"
accepted: "2026-06-05T18:43:43Z"
---

## Problem statement

Mixing requires at least two simultaneous decks.

## Possible solution

- Hoist `Deck` into a slice owned by `Mixer`: `decks: [Deck; 2]`.
- Focus state: which deck transport keys target. `Tab` cycles focus.
- Each deck has its own independent playback state, position, gain.

## Acceptance

- [x] Both decks can play simultaneously without interrupting one another. (`Mixer::fill_mix` sums both; `fill_mix_sums_playing_decks`, `transport_affects_only_focused_deck_and_decks_play_together`.)
- [x] `Tab` cycles focus; transport keys only affect the focused deck. (`tab_cycles_focus`, `transport_affects_only_focused_deck_...`.)
- [x] Loading into one deck does not disturb the other. (`decks_are_independent`; load/transport go through `mixer.deck_mut(focus)`.)

## Implementation notes

- **Mixer owns the decks:** `Mixer { decks: [Deck; DECKS=2], master, .. }` with `deck(i)`/`deck_mut(i)` and `fill_mix(out)` — zeroes the buffer, sums each deck's `fill` (a stopped/paused deck contributes silence), then applies master. The audio pump now calls `mixer.fill_mix` instead of one deck + apply.
- **Focus:** `App.focus: usize`; `Tab` cycles `0..DECKS`; all transport keys act on `focused_mut()` (= `mixer.deck_mut(focus)`); `o`/crate-load decode into the focused deck.
- **UI:** the right pane now stacks two deck panels (A/B) over the master line. The focused deck is marked `▸` and its border brightened; a playing deck is amber.
- **Foundation for the rest of the epic:** the array + `fill_mix` is where the crossfader and N-deck generality slot in next; summing has no headroom management yet (master can attenuate) — the crossfader story will own gain-staging between decks.

## Note (review)

Two-deck mixing is now real but there's no crossfader yet (next story), so both decks currently sum at full level — pushing both loud can exceed 0 dBFS. Acceptable for this increment; gain-staging lands with the crossfader. TUI verified headlessly (TestBackend); interactive feel best confirmed by running it.
