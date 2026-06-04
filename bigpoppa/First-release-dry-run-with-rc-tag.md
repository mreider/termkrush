---
title: First release dry-run with rc tag
type: chore
created: "2026-06-04T09:15:06Z"
modified: "2026-06-04T09:15:06Z"
author: Matt Reider
status: unstarted
epic: release-v0-1
tags: [release, ci]
project: bigpoppa
---

## Why this is a chore

Verifies CI infra; no product change.

## What needs to happen

- Push v0.1.0-rc1 tag.
- Confirm the release workflow produces all 5 artifacts.
- Download each on its target OS, run, confirm TUI opens.
- Delete the rc release once verified; remove the tag.

## Acceptance

- [ ] All 5 platform artifacts present on the draft release.
- [ ] Each artifact runs the TUI on its target OS.
- [ ] Checksums match.
