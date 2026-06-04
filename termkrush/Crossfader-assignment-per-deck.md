---
title: Crossfader assignment per deck
type: feature
created: "2026-06-04T09:20:28Z"
modified: "2026-06-04T09:20:28Z"
author: Matt Reider
status: unstarted
estimate: "3"
epic: multi-deck
tags: [mix, multi-deck]
project: termkrush
---

## Problem statement

A single crossfader cannot meaningfully mix four decks. Need fader assignments.

## Possible solution

- Each deck has an assignment: A, B, or OFF (always full).
- The single crossfader still moves between the A and B groups.
- Hotkey on focused deck: Ctrl-1 = A, Ctrl-2 = B, Ctrl-0 = OFF.

## Acceptance

- [ ] Assigning two decks to A and two to B and moving the fader audibly groups them.
- [ ] OFF-assigned decks always play at unity regardless of fader.
- [ ] Assignment shown in each deck panel.
