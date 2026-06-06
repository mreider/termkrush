---
title: Integration tests for every keyboard command end-to-end
type: chore
created: "2026-06-06T09:06:02Z"
modified: "2026-06-06T09:06:36Z"
author: Matt Reider
status: unstarted
epic: foundation
project: termkrush
---

## Why this is a chore

Cross-cutting test infrastructure. The unit tests cover `on_key` (key → `Action` + in-memory state) but **nothing exercises the real command flow through the event loop** — decode, load onto a deck, mix. That gap let "enter does nothing" ship green. This chore closes it: every command is driven end-to-end and its observable effect asserted.

## What needs to happen

- A headless harness that drives a sequence of key events into `App` and applies the resulting actions — including the load actions that decode a real fixture and land a track on a deck — without a TTY or audio device.
- A test per command family: play/pause, cue/stop, seek (near/far), fine scrub, deck volume, master, crossfader (slide + center), focus toggle, crate filter + nav, **enter-loads-a-real-track**, demo-load, collapse, help, quit.
- Use the committed audio fixtures as the crate so loading actually decodes.

## Acceptance

- [ ] Every keyboard command has an end-to-end test asserting its effect on App/mixer/deck state (not just the returned `Action`).
- [ ] `enter` on a populated crate is proven to decode and load the selected track onto the focused deck.
- [ ] Tests run with no TTY / no audio device (CI-safe); `cargo test` green.
