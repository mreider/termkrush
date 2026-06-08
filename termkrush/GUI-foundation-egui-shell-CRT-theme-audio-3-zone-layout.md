---
title: 'GUI foundation: egui shell, CRT theme, audio, 3-zone layout'
type: feature
created: "2026-06-08T12:17:10Z"
modified: "2026-06-08T12:27:00Z"
author: Matt Reider
status: delivered
estimate: "5"
epic: gui
started: "2026-06-08T12:18:22Z"
finished: "2026-06-08T12:27:00Z"
delivered: "2026-06-08T12:27:00Z"
---

## Goal
Stand up the egui/eframe desktop app: a window with the CRT amber/green theme, the existing `termkrush-core` engine + cpal audio wired in, and the three zones (timeline strip on top, library panel, pad grid) rendered from real state. No mouse interactions yet (those are stories 2–5) — this is the shell that proves egui + audio + theme together and becomes the default launch.

## Acceptance
- Default `termkrush` (on a desktop) opens an egui window; `--tui` still launches the old TUI; piped / no-display → the version banner (scriptable/CI).
- CRT amber/green theme: dark background, amber + green text, monospace.
- Renders from the engine: library entries, 8 pad cells (track name / kind / volume), the timeline strip — real `termkrush-core` state.
- Audio runs: the mixer pumps to cpal each frame (a triggered pad/preview is audible).
- `scripts/dev-run.sh gui` launches it.
