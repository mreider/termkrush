---
title: Turntable platter visuals per deck
type: feature
created: "2026-06-05T21:18:13Z"
modified: "2026-06-05T21:19:41Z"
author: Matt Reider
status: unstarted
estimate: "3"
epic: turntables
project: termkrush
---

## Problem statement

The decks render as text panels. They should look like turntables.

## Possible solution

- Draw a circular platter per deck (ASCII/Unicode) with a position marker that **rotates while playing** and sits still when stopped/paused.
- Rotation reflects the playhead (one revolution = a sensible musical/seconds unit).
- The focused deck's platter is amber, the other dim (matching the existing focus convention).

## Acceptance

- [ ] Each deck shows a round platter with a marker that rotates during play and is still when stopped/paused.
- [ ] Marker position tracks the playhead.
- [ ] Focused platter amber, other dim; both visible at 100x30.

## Prerequisites

Two-deck TUI layout (accepted). Ranked after the ergonomic keyboard layout so it adopts the new keys.
