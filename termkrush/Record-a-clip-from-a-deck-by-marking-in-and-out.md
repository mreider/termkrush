---
title: Record a clip from a deck by marking in and out
type: feature
created: "2026-06-06T14:19:26Z"
modified: "2026-06-06T19:24:31Z"
author: Matt Reider
status: accepted
estimate: "5"
epic: clips
project: termkrush
started: "2026-06-06T19:20:20Z"
finished: "2026-06-06T19:24:30Z"
delivered: "2026-06-06T19:24:31Z"
accepted: "2026-06-06T19:24:31Z"
---

## Intent
The clip is the instrument. Mark in/out on the focused deck (while playing or scrubbed) to capture that region as a clip buffer of any length. Extends the pad/voice engine (Clip pads); the clip source becomes a recorded region, not a whole file.
## Acceptance
- [ ] Set-in / set-out keys on the focused deck define a region; the captured clip holds its samples + the source BPM.
- [ ] Clips may be short or long; capture never modifies the track.
- [ ] A recorded clip is listed/selectable; engine test: record a known region -> expected length/samples.
