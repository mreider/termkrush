---
title: Sync deck BPM to reference deck
type: feature
created: "2026-06-04T09:15:06Z"
modified: "2026-06-04T09:15:06Z"
author: Matt Reider
status: unstarted
estimate: "3"
epic: sync-and-fade
tags: [tempo, sync]
project: bigpoppa
---

## Problem statement

The whole point of BPM detection: lock one deck's tempo to another with one keystroke.

## Possible solution

- Pick a master deck (the playing one by default).
- Sync hotkey on focused deck: compute ratio = master.bpm / focused.bpm and apply via time-stretch.
- Status indicator in the panel: SYNC lit when active; auto-unlock if the master tempo changes by more than 2 BPM via nudge.

## Acceptance

- [ ] Pressing sync on a 128 BPM deck against a 124 BPM master makes them tempo-match audibly within 100ms.
- [ ] Sync indicator lights up.
- [ ] Sync survives master volume changes and crossfader movement.
