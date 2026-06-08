---
title: 'GUI free-track timeline: blocks, drag to track, move, copy-paste, render'
type: feature
created: "2026-06-08T12:17:12Z"
modified: "2026-06-08T12:28:32Z"
author: Matt Reider
status: unstarted
estimate: "8"
epic: gui
---

## Goal
Free-track timeline (DAW-style) — replaces the pad-lane step grid; parity + the new block model. Supersedes the old "edit captured blocks", "record-pad-performance", and "retire-step-grid" stories.
## Scope (parity + new)
- Timeline = freely-addable **tracks**; clips are **blocks** with position + length (engine rework of timeline.rs).
- Looper **record** (perform-capture): arm -> tape rolls -> trigger pads -> blocks land launch-quantized on the next bar.
- Drag a clip/pad onto a track; drag blocks to move; drag block edges to trim; **Cmd-C / Cmd-V** copy/paste a block.
- Transport play/pause; **render** to WAV (library); **tempo ±** (global varispeed); **master ±**.
- Auto-BPM (first dropped track sets master tempo; loops lock) — engine, surfaced here.
