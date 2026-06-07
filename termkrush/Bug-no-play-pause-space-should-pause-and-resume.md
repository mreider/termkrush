---
title: 'Bug: no play/pause - space should pause and resume'
type: bug
created: "2026-06-07T15:42:33Z"
modified: "2026-06-07T15:51:13Z"
author: Matt Reider
status: started
project: termkrush
started: "2026-06-07T15:51:13Z"
---

## Symptom
There's no way to pause. `space` toggles the transport but starting always resets to the top of the arrangement, so it acts as play/stop, never pause→resume.

## Expected
- `space` = play / **pause** (resume from the current position, not the top).
- A separate **stop** (reset to top) is fine on another key.
- Works both in the timeline editor and the main view.
