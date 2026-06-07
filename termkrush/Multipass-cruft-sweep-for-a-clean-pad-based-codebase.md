---
title: Multipass cruft sweep for a clean pad based codebase
type: chore
created: "2026-06-07T11:10:18Z"
modified: "2026-06-07T11:11:37Z"
author: Matt Reider
status: unstarted
project: termkrush
---

## Problem
After the rip-out, guarantee the codebase is pure with no legacy carried forward (code or tests).

## Acceptance
- [ ] No dead code, deck/crossfader/pattern leftovers, or unused deps (grep + `clippy -D warnings` clean).
- [ ] No legacy tests — nothing references decks, crossfader, or the old patterns.
- [ ] Core has zero UI dependencies (enforced by the crate boundary).
- [ ] `cargo fmt` + clippy + all suites green; docs/README/help reflect the pad model.
