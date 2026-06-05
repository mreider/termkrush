---
title: Property-based testing for DSP code
type: chore
created: "2026-06-04T09:41:11Z"
modified: "2026-06-05T18:58:59Z"
author: Matt Reider
status: accepted
epic: foundation
tags: [test, dsp, foundation]
project: termkrush
started: "2026-06-05T18:57:17Z"
finished: "2026-06-05T18:58:59Z"
delivered: "2026-06-05T18:58:59Z"
accepted: "2026-06-05T18:58:59Z"
---

## Why this is a chore

DSP code benefits from property testing; specific failing cases come out automatically. Cross-cutting; supports every story under the tempo / scratch / fx epics.

## What needs to happen

- Add `proptest` as a dev-dependency.
- Write 3 example property tests on the audio path.
- Document the pattern in `tests/README.md`.

## Acceptance

- [x] proptest in the dev-dependency tree. (`[dev-dependencies] proptest = "1"`.)
- [x] 3 representative property tests pass. (`tests/dsp_props.rs`: mixer output always finite; silence-in → silence-out for any fader/gains; unity-gain deck reproduces its input.)
- [x] README explains when to reach for a property test vs an example test. (New "Property tests" section: invariants → property, known answers → example.)

## Notes

Properties run against the real public API (`Deck`, `Mixer`) with no device — they exercise the gain ramp, crossfader law, and master in one shot across generated inputs, and shrink any failure. These are the templates the tempo/scratch/fx stories copy.
