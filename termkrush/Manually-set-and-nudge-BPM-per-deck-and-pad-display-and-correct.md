---
title: Manually set and nudge BPM per deck and pad display and correct
type: feature
created: "2026-06-06T17:10:02Z"
modified: "2026-06-06T17:13:21Z"
author: Matt Reider
status: accepted
estimate: "3"
epic: tempo
started: "2026-06-06T17:10:02Z"
finished: "2026-06-06T17:13:21Z"
delivered: "2026-06-06T17:13:21Z"
accepted: "2026-06-06T17:13:21Z"
---

## Intent
Give the user a BPM they control. Today BPM is detected and shown read-only; there's no way to correct a wrong guess or set tempo for a pad. This adds manual **set + nudge** per deck and per pad — the read/correct half. No audible tempo change yet (that's pitch-preserving time-stretch); this owns the *value* that stretch / sync / auto-bpm will later read.

## Behaviour
- Focused **deck**: `,` / `.` nudge its BPM down/up by 1 (`shift` = 0.1). A user-set BPM **locks** so async detection won't clobber it; the panel shows the value.
- Focused **Clips**: `,` / `.` nudge the **selected pad's** BPM (stored per pad for later auto-bpm/patterns).
- Sensible default (120) when nothing is set/detected yet; clamped to a musical range.

## Acceptance
- [ ] `,`/`.` adjust the focused deck's BPM (±1, shift ±0.1); shown in the deck panel.
- [ ] In Clips focus, `,`/`.` adjust the selected pad's BPM.
- [ ] A manually-set deck BPM survives a later detection result (lock).
- [ ] No playback-speed change (documented: needs time-stretch).
- [ ] Tests for deck + pad nudge/lock; help/README updated.
