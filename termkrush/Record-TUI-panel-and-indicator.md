---
title: Record TUI panel and indicator
type: feature
created: "2026-06-04T09:20:28Z"
modified: "2026-06-04T09:20:28Z"
author: Matt Reider
status: unstarted
estimate: "2"
epic: record
tags: [record, tui]
project: termkrush
---

## Problem statement

The recording engine needs UI affordance.

## Possible solution

- R toggles recording globally.
- A blinking red REC indicator with elapsed time in the top bar.
- Settings overlay (Ctrl-R) chooses format (wav / mp3) and bitrate.

## Acceptance

- [ ] R starts and stops recording with visible indicator.
- [ ] Elapsed counter increments while recording.
- [ ] Format/bitrate setting persists in config.
