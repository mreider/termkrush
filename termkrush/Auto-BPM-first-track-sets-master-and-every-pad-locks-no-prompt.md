---
title: 'Auto-BPM: first track sets master and every pad locks, no prompt'
type: feature
created: "2026-06-07T17:52:35Z"
modified: "2026-06-07T17:54:05Z"
author: Matt Reider
status: unstarted
estimate: "3"
epic: looper
---

## Goal
The first dropped track sets the master tempo; every pad locks to it silently. No prompt. (Supersedes the delivered "ask on mismatch" bug.)

## Spec
- Remove the bpm_prompt / "Sync all tracks?" modal.
- place_decoded: first track with a BPM sets master; every loaded pad auto-syncs to it; later tracks adopt the master (varispeed), no prompt.
- Test: drop A@120 → master 120; drop B@140 → both locked to 120, no prompt.
