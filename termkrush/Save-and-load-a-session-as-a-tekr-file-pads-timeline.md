---
title: Save and load a session as a .tekr file (pads + timeline)
type: feature
created: "2026-06-08T11:12:20Z"
modified: "2026-06-08T11:12:20Z"
author: Matt Reider
status: unstarted
estimate: "8"
epic: session
project: termkrush
---

## Goal
Persist a whole session (all pads + the timeline) to a `.tekr` file so you can quit termkrush and pick up exactly where you left off.

## Spec
- **Save on quit.** When quitting, ask "Save session? y/n" (fold into / sit beside the existing quit confirm). On yes, write a `.tekr` file into the directory termkrush was launched from (cwd at startup), not the library root.
- **What it stores** (paths, not audio): per-pad → source path, kind (1shot/loop/scratch), trim in/out, gain, active/off, bpm, scratch phrase; the timeline → its blocks/arrangement + bars/steps; master → bpm + gain. Format: JSON via serde (a `.tekr` extension, human-diffable).
- **Load with `L`.** On launch (ties into the splash-screen story as the entry point), an `L` command lists the `.tekr` files in the launch directory and loads the chosen one: restores the timeline + master state, and re-loads each pad by **re-decoding its stored source path** through the existing async decode pipeline (the file holds paths, not samples). Missing sources flag red / are skipped (reuse the unplayable-files treatment).

## Notes / open questions
- Quit flow: "Save session? y / n / cancel" vs a separate prompt — decide at align.
- `.tekr` naming: prompt for a name, or default to a timestamp / `session.tekr`?
- Depends on / relates to: the **splash-screen** story (the `L`-to-load entry point) and **Flag-unplayable-files-in-red** (missing-source handling).
- Could split into "save → .tekr" and "load .tekr (L)" if 8 feels heavy at align.
