---
title: Scaffold Rust project layout
type: feature
created: "2026-06-04T09:05:27Z"
modified: "2026-06-04T09:11:00Z"
author: Matt Reider
status: unstarted
estimate: "2"
epic: foundation
tags: [foundation, rust]
project: termkrush
---

## Problem statement

There is no Rust project yet. We need a workspace that the rest of the backlog can build on, with a layout that anticipates the audio engine, the TUI, and the cross-platform release pipeline.

## Possible solution

- `cargo init --bin` at the repo root.
- Crate name `termkrush`, binary name `termkrush`.
- Modules stubbed: `audio/`, `tui/`, `deck/`, `mix/`, `library/`, `config/`.
- Rust edition 2021, MSRV 1.75 pinned in `rust-toolchain.toml`.
- `.gitignore` covers `target/`, `.termkrush/`, `*.mp3`.
- `Cargo.toml` metadata: license MIT, repository, description, keywords.

## Acceptance

- [ ] `cargo build` succeeds on macOS arm64.
- [ ] `cargo build` succeeds on Linux amd64 (verify in CI later).
- [ ] Binary `termkrush` runs and prints "TermKrush v<version>" then exits.
- [ ] Module skeletons compile (empty `pub fn` placeholders are fine).
- [ ] License field, repo URL, keywords present in Cargo.toml.
