---
title: Refactor controls into a minimal select then act model
type: feature
created: "2026-06-06T14:49:48Z"
modified: "2026-06-06T15:04:00Z"
author: Matt Reider
status: accepted
estimate: "5"
epic: controls
project: termkrush
started: "2026-06-06T14:55:10Z"
finished: "2026-06-06T15:03:59Z"
delivered: "2026-06-06T15:03:59Z"
accepted: "2026-06-06T15:04:00Z"
---

## Intent
Collapse the sprawling per-deck keymap into a MINIMAL **focus → act** model: pick a target (Deck A / Deck B / Clip), then a small fixed cluster of action controls operates on it, context-sensitive. Fewer controls, faster to flip what you drive, and a 1:1 shape with a gamepad so the Xbox story maps cleanly. This is the shared control framework both inputs use.

## The model
- **Focus/target**: one of {Deck A, Deck B, Clip slot} is active; a fast selector flips it; the active target is unmistakable in the UI.
- **Action cluster**: a fixed set of verbs (primary / cue / mark-in / mark-out / alt) acts on the focused target; meaning changes by context (deck: play/cue/seek-mark; clip: trigger/pattern/assign/auto-bpm).
- **Continuous controls** stay always-live (scrub, crossfade/auto-fade, jog); keyboard = discrete steps, gamepad = analog.
- **Shift layer** (a modifier key / held bumper) flips the cluster to its secondary set.

## Keyboard shape (example, finalize at align)
- Focus: `Tab` cycles A -> B -> Clips (plus direct focus keys).
- One home-row action cluster drives the focused target (no per-hand duplication).
- Transitions (hard-cut / auto-fade) + master stay as quick keys.

## Acceptance
- [ ] A focus model with a fast selector; active target obvious on screen.
- [ ] One context-sensitive action cluster drives the focused target — no per-deck key duplication.
- [ ] Every current function reachable with materially fewer keys; help reflects the new map.
- [ ] Extensible: future features (clip patterns, trim, auto-bpm) register actions per context.
- [ ] Tests cover focus switching + context dispatch.

## Notes
Supersedes the accepted "Ergonomic keyboard layout for live performance"; the transition (g/h/G/H/space) and pad keys fold into the model. Designed up front so BPM/clips plug into it.
