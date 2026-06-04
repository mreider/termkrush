---
title: Download TUI panel
type: feature
created: "2026-06-04T09:20:27Z"
modified: "2026-06-04T09:20:27Z"
author: Matt Reider
status: unstarted
estimate: "3"
epic: download
tags: [download, tui]
project: termkrush
---

## Problem statement

The download backends exist but there is no UI to use them.

## Possible solution

- New panel toggled with d: URL input line, source mode selector (auto / direct / yt-dlp), recent downloads list.
- Auto mode: if URL ends in .mp3, use direct; else use yt-dlp.
- Each active download row shows: URL excerpt, progress bar, speed, ETA, cancel hotkey.
- On completion, the new file appears in the crate list.

## Acceptance

- [ ] d opens the download panel; Esc closes it.
- [ ] URL input accepts paste from clipboard.
- [ ] Multiple downloads run concurrently with independent progress.
- [ ] Cancelled downloads disappear; completed ones move to the crate.
