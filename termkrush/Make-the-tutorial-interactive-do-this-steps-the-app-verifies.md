---
title: 'Make the tutorial interactive: do-this steps the app verifies'
type: feature
created: "2026-06-08T07:04:43Z"
modified: "2026-06-08T07:05:12Z"
author: Matt Reider
status: unstarted
estimate: "8"
epic: tutorial
project: termkrush
---

## Goal
Make the `?` tutorial **interactive**: instead of reading pages, the user is prompted to do each action and the app detects they did it, then advances. Learn by doing.

## Spec (sketch — refine later)
- A tutorial mode with ordered steps; each step shows a prompt ("Tab to a pad", "press Space to play it", "Enter → load a track", "Shift+←/→ to fine-trim", "place a pad on the timeline") and a success condition the app checks from real state/events.
- Highlights/points at the relevant panel; advances on success; Esc exits anytime.
- Covers the same content as the guided walkthrough [[Tutorial-question-mark-opens-a-guided-walkthrough-of-the-controls-and-flow]], including clip-edit precision (Shift = fine) and snip-to-zoom.
- This is the version the PM actually wants; the paged one is the stepping stone.
