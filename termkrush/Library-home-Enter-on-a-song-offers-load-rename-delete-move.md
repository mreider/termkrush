---
title: 'Library home: Enter on a song offers load rename delete move'
type: feature
created: "2026-06-08T08:21:28Z"
modified: "2026-06-08T08:58:18Z"
author: Matt Reider
status: delivered
estimate: "5"
epic: nav2
project: termkrush
started: "2026-06-08T08:48:28Z"
finished: "2026-06-08T08:58:18Z"
delivered: "2026-06-08T08:58:18Z"
---

## Goal
The Library is home. `Esc` here opens the quit modal.

## Spec
- `Enter` on a song opens an action prompt:
  - `1`–`8` → load the song onto that pad.
  - `Delete` → delete (confirm).
  - `Insert` → rename.
  - `Right` → move (opens the folder-picker, see its story).
- Intuitive; no inline command subtitles — just `?` in the title for help.

## Open
- Mac laptops have no Insert and Delete = Backspace. Need Mac-friendly keys for rename/delete (PM to confirm mapping).
