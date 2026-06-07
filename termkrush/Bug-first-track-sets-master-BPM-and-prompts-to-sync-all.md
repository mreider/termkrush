---
title: 'Bug: first track sets master BPM and prompts to sync all'
type: bug
created: "2026-06-07T15:42:33Z"
modified: "2026-06-07T16:09:22Z"
author: Matt Reider
status: finished
project: termkrush
started: "2026-06-07T16:07:04Z"
finished: "2026-06-07T16:09:22Z"
---

## Symptom
Loading the first track shows no BPM-based guidance; master tempo is only seeded by the first *loop* pad, and there's no way to make everything share one tempo.

## Expected
- When a track is loaded (esp. the first / pad 1), its detected BPM is the candidate master.
- Prompt: "Adjust BPM for all tracks?" — **yes** sets every loaded pad's loop sync to that master so they're all the same tempo and synced.
