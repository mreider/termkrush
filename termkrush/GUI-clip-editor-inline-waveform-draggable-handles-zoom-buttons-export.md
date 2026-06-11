---
title: 'GUI clip editor inline: waveform, draggable handles, zoom buttons, export'
type: feature
created: "2026-06-08T12:17:11Z"
modified: "2026-06-08T13:00:58Z"
author: Matt Reider
status: delivered
estimate: "5"
epic: gui
started: "2026-06-08T12:57:36Z"
finished: "2026-06-08T13:00:58Z"
delivered: "2026-06-08T13:00:58Z"
project: termkrush
---

## Goal
Inline waveform clip editor (no modal) — parity with the TUI clip editor.
## Scope (parity)
- Waveform render; draggable in/out trim **handles**; +/- zoom buttons; window follows the active handle.
- Play/stop audition at the active handle (start-over feel).
- Trim is the edit, kept on close with a discard option (TUI save-on-close behaviour).
- Export button (WAV to the library).
