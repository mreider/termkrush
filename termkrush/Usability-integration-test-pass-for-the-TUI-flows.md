---
title: Usability + integration test pass for the TUI flows
type: chore
created: "2026-06-07T15:42:34Z"
modified: "2026-06-07T15:43:00Z"
author: Matt Reider
status: unstarted
project: termkrush
---

## Why
Unit tests verified mechanics but not the user experience, so usability gaps (no pause, no unload, unusable trim) shipped uncaught. Add coverage that exercises real flows headlessly + a manual smoke checklist.

## Acceptance
- Integration/render tests that walk end-to-end flows (load→trigger→trim→arrange→play→render) via the public App API.
- A short manual smoke checklist in the repo for things only a human at a tty can confirm.
- Note clearly in story notes which acceptance points need the PM's hands-on check.
