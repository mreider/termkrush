---
title: Record the timeline into a pad (overwrite confirm if not empty)
type: feature
created: "2026-06-08T10:58:25Z"
modified: "2026-06-08T10:58:25Z"
author: Matt Reider
status: unstarted
estimate: "5"
epic: looper
project: termkrush
---

## Goal
From the timeline, record the arrangement (what plays) into a pad, so you can bounce a section to a single pad and re-use it.

## Spec
- A Timeline menu action "record to pad" → pick a pad (1-8); if the pad isn't empty, "are you sure? y/n" overwrite confirm.
- Renders the timeline (or its loop region) and assigns the result to the chosen pad.
- Replaces the idea the user had about "rec phrase" — that pad-menu item was actually scratch-phrase recording; this is the real "capture the timeline to a pad" feature.
