---
title: 'GUI library panel: mouse rename, drag to folder, drag to pad, delete button'
type: feature
created: "2026-06-08T12:17:11Z"
modified: "2026-06-08T12:28:32Z"
author: Matt Reider
status: unstarted
estimate: "5"
epic: gui
---

## Goal
Mouse library panel — full parity with the TUI library, in the egui left panel.
## Scope (parity)
- List folders + tracks; one level of subfolders (click to open, back affordance).
- Double-click a track to rename (inline field, no modal).
- Drag a track into a folder to move it; new-folder / rename-folder / delete-folder controls.
- Drag a track onto a pad to load it.
- Highlight + a Delete button to delete a track (inline confirm).
- Click a track to preview (play/stop), reusing the engine preview voice.
- **Flag unplayable / unsupported files in red** (absorbs the old Flag-unplayable story).
