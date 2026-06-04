---
title: Tag-triggered release pipeline
type: feature
created: "2026-06-04T09:11:00Z"
modified: "2026-06-04T09:11:00Z"
author: Matt Reider
status: unstarted
estimate: "3"
epic: foundation
tags: [foundation, ci, release]
project: termkrush
---

## Problem statement

The site links to "latest release". We need GitHub Releases to publish cross-platform binaries automatically when a `vX.Y.Z` tag is pushed.

## Possible solution

- `.github/workflows/release.yml` triggered on `v*` tags.
- Build matrix: darwin-arm64, darwin-amd64, linux-arm64, linux-amd64, windows-amd64.
- Each job: `cargo build --release --target <triple>`, strip, tar.gz (or zip for windows), sha256.
- `softprops/action-gh-release` uploads all artifacts plus checksums to the release.
- Releases are draft until manually published (safer for first cuts).

## Acceptance

- [ ] Pushing `v0.0.1-rc1` produces a draft release with 5 binary artifacts and 5 sha256 files.
- [ ] Each binary runs locally on its target OS (smoke test).
- [ ] Release body is auto-generated changelog from commits since previous tag.
