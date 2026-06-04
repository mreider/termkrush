---
title: Coverage reporting in CI
type: chore
created: "2026-06-04T09:41:11Z"
modified: "2026-06-04T12:32:34Z"
author: Matt Reider
status: accepted
epic: foundation
tags: [test, ci, foundation]
project: termkrush
started: "2026-06-04T12:30:28Z"
finished: "2026-06-04T12:32:34Z"
delivered: "2026-06-04T12:32:34Z"
accepted: "2026-06-04T12:32:34Z"
---

## Why this is a chore

Visibility into test coverage; no user-facing change. Helps the PM see whether stories are honestly covered.

## What needs to happen

- Use `cargo-llvm-cov` in the Linux job of `.github/workflows/ci.yml`.
- Generate LCOV; upload to Codecov (or a comparable free service) on each PR and on main.
- Add a coverage badge to the README once the first feature lands.
- Fail PRs that drop coverage by more than 2 percentage points (after the first 5 feature stories have landed, so we have a baseline).

## Acceptance

- [ ] Coverage report rendered on every PR.
- [ ] Coverage badge on README.
- [ ] Threshold gate active and visible in CI output.
