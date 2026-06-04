---
title: TUI shell with ratatui
type: feature
created: "2026-06-04T09:11:00Z"
modified: "2026-06-04T09:11:00Z"
author: Matt Reider
status: unstarted
estimate: "2"
epic: foundation
tags: [foundation, tui]
project: bigpoppa
---

## Problem statement

Need a running TUI to hang the rest of the UI off. Right now the binary just prints and exits.

## Possible solution

- Add `ratatui`, `crossterm` deps.
- Alternate screen + raw mode on start, restore on exit (including on panic).
- Event loop with 30 Hz redraw cap.
- Splash widget: centered wordmark "big poppa" in amber, tagline in green-on-dark.
- Quit on `q` or `Ctrl-C`. `?` shows a help overlay (stub for now).

## Acceptance

- [ ] `big-poppa` opens fullscreen TUI on a 80x24 terminal without breaking layout.
- [ ] `q` exits cleanly, terminal returns to normal state.
- [ ] Panicking inside the event loop restores the terminal before printing the crash.
- [ ] Splash uses the CRT palette colors from the design.
