---
title: Logging and panic hook with tracing
type: feature
created: "2026-06-04T09:11:00Z"
modified: "2026-06-04T12:24:16Z"
author: Matt Reider
status: accepted
estimate: "1"
epic: foundation
tags: [foundation, observability]
project: termkrush
started: "2026-06-04T12:22:48Z"
finished: "2026-06-04T12:24:16Z"
delivered: "2026-06-04T12:24:16Z"
accepted: "2026-06-04T12:24:16Z"
---

## Problem statement

Debugging audio glitches without structured logs is misery. We need `tracing` wired before any audio code lands, and a panic hook that writes a crash log to disk.

## Possible solution

- Add `tracing`, `tracing-subscriber`, `tracing-appender` deps.
- Init subscriber in `main.rs`: human-friendly to stderr (off by default), JSON to `~/.termkrush/log/` when `--log` flag set.
- Custom panic hook flushes the log and writes a one-line crash summary.
- No `println!` outside of CLI subcommands.

## Acceptance

- [ ] `termkrush --log` produces a JSON log file under `~/.termkrush/log/`.
- [ ] Triggering a panic in a test path writes a crash line to the log.
- [ ] `cargo clippy -- -D warnings` is clean.
