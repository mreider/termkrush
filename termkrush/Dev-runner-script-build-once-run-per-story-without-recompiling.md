---
title: 'Dev runner script: build once, run per-story without recompiling'
type: chore
created: "2026-06-05T16:11:39Z"
modified: "2026-06-05T16:24:25Z"
author: Matt Reider
status: accepted
started: "2026-06-05T16:21:05Z"
finished: "2026-06-05T16:21:05Z"
delivered: "2026-06-05T16:21:05Z"
accepted: "2026-06-05T16:21:05Z"
---

## Why this is a chore

Pure developer-experience glue, no end-user feature. During acceptance the PM
wants to *run* the delivered behavior, often several times, without paying a
`cargo build` on every invocation. Today the only paths are `cargo run`
(rebuilds on each call) or remembering the raw `./target/debug/termkrush`
flags. This chore makes "build once, then run the same binary as many times as
acceptance needs" a one-liner, and gives each story a named recipe to run.

## What needs to happen

A `scripts/dev-run.sh` helper (sibling to `file-bug.sh` / `gen-fixtures.sh`)
that separates **build** from **run**:

- `scripts/dev-run.sh build` — compile the debug binary once (`cargo build`).
- `scripts/dev-run.sh [recipe] [-- extra args]` — run the **already-built**
  binary directly (`./target/debug/termkrush ...`), invoking *no* cargo, so
  repeated acceptance runs are instant and never recompile. Auto-builds once
  only if the binary is missing.
- Named recipes that map a story's behavior to its run command:
  - `tui` (default) — launch the fullscreen TUI.
  - `tone [secs]` — `--test-tone [secs]` (the cpal output-stream story).
  - `panic` — `--panic-test` (the logging/panic-hook story).
  - `list` — print the available recipes.
- `--release` flag to target the release binary instead of debug.
- `watch` — `cargo watch -x build` when cargo-watch is installed; else a hint.
- Pass-through: anything after `--` is forwarded to the binary verbatim.
- Resolves the repo root from its own location, so it works from any cwd.

A short "Running locally for acceptance" note in `CLAUDE.md` pointing at it.

## Acceptance

- [x] `scripts/dev-run.sh build` compiles the debug binary.
- [x] `scripts/dev-run.sh tone 1` plays a 1s test tone using the prebuilt binary without invoking cargo.
- [x] `scripts/dev-run.sh list` prints the recipes.
- [x] `scripts/dev-run.sh panic` exercises the panic path (exit non-zero, crash line logged).
- [x] A plain recipe auto-builds once when the binary is missing, then runs; subsequent runs do not rebuild.
- [x] `CLAUDE.md` documents the runner under acceptance.

## Note

Implemented bash-3.2 compatible (macOS stock bash) — no `mapfile`.
