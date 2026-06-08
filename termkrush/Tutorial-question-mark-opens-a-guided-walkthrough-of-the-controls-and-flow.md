---
title: 'Tutorial: question-mark opens a guided walkthrough of the controls and flow'
type: feature
created: "2026-06-08T07:04:55Z"
modified: "2026-06-08T07:05:12Z"
author: Matt Reider
status: unstarted
estimate: "5"
epic: tutorial
project: termkrush
---

## Goal
`?` opens an in-app tutorial. First version: a short guided walkthrough (paged) that teaches the whole tool, so a newcomer can make a mix without reading docs. (`?` is currently unbound since we went self-documenting; this gives it a purpose.)

## Content to teach
- The five controls: arrows (move; pad ↑↓ = volume), Tab (Library→Pads→Timeline), Space (play), Enter (context menu — its first item is the common action), Esc (back).
- Core flow: browse library → Enter→load onto a pad → Space to play.
- Clip edit (Enter→menu→edit clip): ←/→ coarse, **Shift+←/→ fine (~1ms)**, Tab in/out, Space audition, Enter snip; **snipping shortens the clip so the same-width bar zooms in for finer trimming**.
- Timeline: each lane = a pad (labeled); ←/→ beat, ↑/↓ pad-lane, Enter→place pad, Space plays the arrangement.
- Scratch pad: Space = wiki; record a phrase then tap Space (wiki) / Enter (whip).
- Save/export and render (when those land).

## Notes
- Paged/navigable overlay; Esc closes. This is the static first cut; the interactive version is [[Make-the-tutorial-interactive-do-this-steps-the-app-verifies]].
