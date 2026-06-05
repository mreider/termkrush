---
title: Seek and scrub position
type: feature
created: "2026-06-04T09:11:00Z"
modified: "2026-06-05T17:44:03Z"
author: Matt Reider
status: accepted
estimate: "2"
epic: one-deck
tags: [deck, one-deck]
project: termkrush
started: "2026-06-05T17:41:15Z"
finished: "2026-06-05T17:44:03Z"
delivered: "2026-06-05T17:44:03Z"
accepted: "2026-06-05T17:44:03Z"
---

## Problem statement

Need to be able to jump around in a track and scrub for cueing.

## Possible solution

- `Deck::seek(seconds)`.
- Hotkeys: `←`/`→` jump ±5s; `Shift ←/→` jump ±30s; `,`/`.` nudge ±0.1s.
- Seeking avoids clicks.

## Acceptance

- [x] Arrow keys move the playhead by the documented amount. (`←/→` ±5s, `Shift+←/→` ±30s, `,`/`.` ±0.1s — `arrow_keys_seek_the_deck`, `shift_arrow_seeks_far_and_eof_stops`, `comma_period_scrub_finely`.)
- [x] Seeking past EOF clamps to EOF and stops the deck. (`seek_past_eof_clamps_and_stops`.)
- [x] No audible click on seek. (Declick: the seek resets the applied gain so audio fades back in from silence over the ramp; `seek_declicks_by_fading_in` shows the first post-seek sample is attenuated, not a full-amplitude jump.)

## Implementation notes

- `Deck::seek(secs)` sets the playhead to an absolute position (clamped to `[0, end]`); at/over EOF it clamps to the end and stops. `seek_by(delta)` is the relative form used by the keys. Transport state is otherwise preserved.
- **Declick:** rather than literally pausing the callback for a buffer, the pull-model equivalent — `seek` resets `smoothed_gain` to 0 so the post-seek audio fades in over the ~12ms gain ramp, masking the splice discontinuity. Verified structurally (first sample attenuated), not by an audio-listening test.
- Keys wired in the pure `on_key`: `←/→` ±5s, `Shift+←/→` ±30s, `,`/`.` ±0.1s; help overlay + hint row updated. Panel already renders `position_secs`, so the readout tracks seeks for free.
