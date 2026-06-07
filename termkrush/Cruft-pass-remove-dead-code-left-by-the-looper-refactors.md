---
title: 'Cruft pass: remove dead code left by the looper refactors'
type: chore
created: "2026-06-07T18:05:58Z"
modified: "2026-06-07T18:08:48Z"
author: Matt Reider
status: accepted
started: "2026-06-07T18:05:58Z"
delivered: "2026-06-07T18:08:48Z"
accepted: "2026-06-07T18:08:48Z"
project: termkrush
---

## Why
After the clip-edit rework (snip vs truncate) and the BPM-prompt removal, some pub methods were orphaned. Remove dead code; keep the API the looper epic still needs, test-covered.

## Done
- Removed `Mixer::truncate_pad` (replaced by `snip_pad`) and `Mixer::set_pad_kind` (only caller was the deleted BPM prompt) — both unreferenced.
- Audited all core `pub fn`s for zero external references; the rest are in use.
- Kept the I1 launch-quant API (`quantize_beats`/`set_quantize_beats`/`frames_to_next_bar`) — needed by I3/I4 — and added a test so it's exercised.
- Confirmed no `deck`/`crossfader`/`turntable`/`Pattern` remnants remain.
