---
title: 'GUI pad grid: drag-to-change-kind, volume, clear, click play'
type: feature
created: "2026-06-08T12:17:11Z"
modified: "2026-06-08T12:28:32Z"
author: Matt Reider
status: unstarted
estimate: "5"
epic: gui
---

## Goal
Mouse pad grid — parity with the TUI pad menu, no modals.
## Scope (parity)
- Each pad cell: track name, kind, volume, on/off, loaded/empty.
- Drag-and-drop to change kind (1shot / loop / scratch).
- Volume slider; on/off toggle; Clear button (empties the pad).
- Click to play/pause (toggle), reusing the no-stack rule.
- Export button → write the pad's trimmed clip to the library as WAV (absorbs the Ctrl-S/save-to-library story).
- Open the inline clip editor (story #4).
