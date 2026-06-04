---
title: Bug-filing helper script and skill
type: chore
created: "2026-06-04T09:41:11Z"
modified: "2026-06-04T12:21:30Z"
author: Matt Reider
status: accepted
epic: foundation
tags: [process, test, foundation]
project: termkrush
started: "2026-06-04T12:20:06Z"
finished: "2026-06-04T12:21:30Z"
delivered: "2026-06-04T12:21:30Z"
accepted: "2026-06-04T12:21:30Z"
---

## Why this is a chore

Process glue. Makes the "no fix without a bug" rule frictionless.

## What needs to happen

A small `am`-flavored helper that drops a properly typed bug story at the top of priority with a one-liner:

- A shell function or script `scripts/file-bug.sh "Bug: <symptom>"` that:
  - calls `am create-item ... --target priority --position top`
  - calls `am set-type <path> bug` (strips any estimate)
  - opens the new file in the user's editor for body fill-in.
- A `.claude/skills/file-bug/SKILL.md` so `/file-bug` works in Claude Code.
- A short section in CLAUDE.md / AGENTS.md pointing at it.

## Acceptance

- [ ] `scripts/file-bug.sh "Bug: test_x failed on macos"` creates the story at top of priority with type=bug, no estimate.
- [ ] `/file-bug` skill is discoverable in Claude Code.
- [ ] Bug body template includes: symptom, repro steps, expected, actual, env, related story.
