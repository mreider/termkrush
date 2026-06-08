---
title: Save a pad or timeline to the library with ctrl-S
type: feature
created: "2026-06-08T08:21:28Z"
modified: "2026-06-08T08:22:01Z"
author: Matt Reider
status: unstarted
estimate: "5"
epic: nav2
project: termkrush
---

## Goal
Save what's in a pad — or the timeline — back to the Library.

## Spec
- `Ctrl-S` (Win) / `Cmd-S` (Mac) saves the selected pad's clip to the library; if the timeline is selected, saves the rendered timeline.
- Opens a name modal; after `Enter`, choose where: root or an existing subdir (one level only).
