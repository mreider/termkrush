---
title: Property-based testing for DSP code
type: chore
created: "2026-06-04T09:41:11Z"
modified: "2026-06-04T09:41:11Z"
author: Matt Reider
status: unstarted
epic: foundation
tags: [test, dsp, foundation]
project: termkrush
---

## Why this is a chore

DSP code benefits from property testing; specific failing cases come out automatically. Cross-cutting; supports every story under the tempo / scratch / fx epics.

## What needs to happen

- Add `proptest` as a dev-dependency.
- Write 3 example property tests on the audio path (any of: gain is linear, silence in → silence out, idempotence of tempo ratio 1.0, no NaN in output for any input).
- Document the pattern in `tests/README.md` so feature stories can copy it.

## Acceptance

- [ ] proptest in the dev-dependency tree.
- [ ] 3 representative property tests pass.
- [ ] README explains when to reach for a property test vs an example test.
