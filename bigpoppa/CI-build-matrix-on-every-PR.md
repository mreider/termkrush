---
title: CI build matrix on every PR
type: feature
created: "2026-06-04T09:11:00Z"
modified: "2026-06-04T09:11:00Z"
author: Matt Reider
status: unstarted
estimate: "3"
epic: foundation
tags: [foundation, ci]
project: bigpoppa
---

## Problem statement

Cross-platform Rust audio is the #1 place this project will break. We need PR-time builds on every target before any feature work lands.

## Possible solution

- `.github/workflows/ci.yml`:
  - Matrix: ubuntu-latest, macos-14 (arm64), windows-latest.
  - Steps: checkout, cargo cache, `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`, `cargo build --release`.
  - Linux build also installs ALSA dev headers (`libasound2-dev`).
- Required status checks on `main`.

## Acceptance

- [ ] Opening a PR triggers all three platform builds.
- [ ] A `cargo fmt` violation fails the build.
- [ ] A clippy warning fails the build.
- [ ] Test job passes on all three platforms.
