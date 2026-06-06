---
title: Auto-fade transition over N bars
type: feature
created: "2026-06-04T09:15:06Z"
modified: "2026-06-06T20:08:06Z"
author: Matt Reider
status: accepted
estimate: "3"
epic: sync-and-fade
tags: [mix, sync]
project: termkrush
started: "2026-06-06T20:05:57Z"
finished: "2026-06-06T20:08:06Z"
delivered: "2026-06-06T20:08:06Z"
accepted: "2026-06-06T20:08:06Z"
---

## Problem statement

Beat-matched + cued tracks deserve a one-key crossfade.

## Possible solution

- Hotkey f: start the other deck (if stopped) and crossfade from current xfader position to the other side over N bars (default 8, configurable).
- Bar length derived from master BPM (4 beats per bar).
- During the fade, the xfader animates; pressing f again cancels and holds at current position.

## Acceptance

- [ ] f with both tracks loaded, master playing, executes a smooth N-bar crossfade.
- [ ] The other deck starts at its cue point (or from beginning if no cue).
- [ ] Re-pressing f cancels the in-flight fade.
- [ ] Configurable N (e.g. --fade-bars 16).
