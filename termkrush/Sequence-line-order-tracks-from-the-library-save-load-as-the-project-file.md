---
title: 'Sequence line: order tracks from the library, save/load as the project file'
type: feature
created: "2026-06-11T13:32:41Z"
modified: "2026-06-11T13:37:17Z"
author: Matt Reider
status: unstarted
estimate: "5"
project: termkrush
---

## Goal

The sequence line is the product's only arranging surface: an ordered horizontal lane of track entries. The user drags tracks from the library onto it, reorders, and removes. The same track may appear at multiple positions (1, 3, and 5 is fine — later stories give repeats different material).

## User-visible change

- A permanent sequence lane (CRT-styled) with numbered entries showing track name and a tempo badge when the track has beat marks, or a "needs beats" badge when it doesn't (wired to the tap screen in the beat-tap story).
- Drag from library to insert at any position; drag entries to reorder; a remove control per entry.
- The sequence is the project file: ordered track refs + per-track beat-mark references. Autosaves on change; reopening the app restores the last sequence; save-as/open for multiple projects.

## Acceptance

- Insert, reorder, remove, and duplicate-entry cases covered by tests on the sequence model.
- Round-trip test: save → load reproduces the identical sequence (including repeats and order).
- Renaming/moving a library file in-app keeps sequence entries pointing at the right track.

## Comments

## Attachments
