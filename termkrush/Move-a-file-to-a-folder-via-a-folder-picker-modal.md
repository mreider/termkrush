---
title: Move a file to a folder via a folder-picker modal
type: feature
created: "2026-06-08T08:21:28Z"
modified: "2026-06-08T08:58:18Z"
author: Matt Reider
status: delivered
estimate: "8"
epic: nav2
project: termkrush
started: "2026-06-08T08:58:18Z"
finished: "2026-06-08T08:58:18Z"
delivered: "2026-06-08T08:58:18Z"
---

## Goal
Move the selected file into a folder, managing folders inline.

## Spec
- `Right` (from the song action prompt) opens a modal listing subfolders.
- In the modal: `N` new folder (blinking name entry, `Enter` saves), `↑`/`↓` select, `Insert` rename folder, `Delete` delete folder with a confirm modal ("Deleting this folder removes everything inside it — delete? Y/N"), `Enter` choose the folder → move the file there.
- Only ONE level of folders (no sub-sub-dirs).
