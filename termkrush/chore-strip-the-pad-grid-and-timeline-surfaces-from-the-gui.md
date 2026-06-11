---
title: 'Chore: strip the pad grid and timeline surfaces from the GUI'
type: chore
created: "2026-06-11T13:32:36Z"
modified: "2026-06-11T14:20:30Z"
author: Matt Reider
status: started
project: termkrush
started: "2026-06-11T14:20:30Z"
---

## Goal

Clear the decks for the 2026-06-11 auto-mix pivot (see `.am/inception.md`): remove every retired performance/arranging surface from the GUI so the sequence-line stories land on a clean three-surface app (library / sequence line / beat-tap).

## Scope

- Remove the pad grid, the master-timeline strip, the platter scratch widget, record-arm, and their key/mouse bindings, menus, and hint text.
- `termkrush-core` stays intact: decode, varispeed, mixer, render, beat-grid fit, library. Dead engine code paths that existed *only* for pads/timeline (pad kinds, block move/copy/paste, launch-quantized capture) may be pruned where unreferenced.
- The clip/beat-tap editor remains reachable from the library (it becomes the beat-marking surface in a later story).

## Acceptance

- App launches showing the library and an empty main area, CRT styling intact.
- No pad/timeline modules referenced from the GUI crate; `cargo test` green; `cargo fmt --check` and `cargo clippy -- -D warnings` clean.
- Existing audio integration tests for surviving paths (decode, varispeed, render) still pass.

## Comments

## Attachments
