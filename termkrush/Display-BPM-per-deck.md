---
title: Display BPM per deck
type: feature
created: "2026-06-04T09:15:06Z"
modified: "2026-06-06T19:59:38Z"
author: Matt Reider
status: accepted
estimate: "1"
epic: tempo
tags: [tempo, tui]
project: termkrush
started: "2026-06-06T19:58:21Z"
finished: "2026-06-06T19:59:38Z"
delivered: "2026-06-06T19:59:38Z"
accepted: "2026-06-06T19:59:38Z"
---

## Problem statement

BPM is detected but not shown.

## Possible solution

- Render BPM in each deck panel in large monospaced digits, next to elapsed time.
- An em-dash placeholder shows while detecting; the detected number replaces it when the background task finishes.

## Acceptance

- [ ] BPM appears in deck panel after load.
- [ ] Placeholder shows during the detection window.
- [ ] Detected BPM matches the cache file content.
