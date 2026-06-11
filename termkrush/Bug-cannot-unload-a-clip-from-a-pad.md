---
title: 'Bug: cannot unload a clip from a pad'
type: bug
created: "2026-06-07T15:42:33Z"
modified: "2026-06-11T13:18:40Z"
author: Matt Reider
status: unstarted
project: termkrush
---

## Symptom
Once a clip is on a pad there's no way to clear it — only overwrite by loading another track.

## Expected
- A key on the focused pad **unloads** it (clears the clip, trim, kind/phrase state, source) back to empty.
