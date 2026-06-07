---
title: 'Minimal context-mapped controls: move + Space/Enter/M menu'
type: feature
created: "2026-06-07T20:43:46Z"
modified: "2026-06-07T20:43:46Z"
author: Matt Reider
status: started
estimate: "8"
epic: looper
started: "2026-06-07T20:43:46Z"
---

## Goal
Collapse the ~20-key sprawl into a tiny set reused by context (Xbox-style): move, then the same buttons act on whatever's selected.

## Controls (all of them)
- Arrows — move; on a Pad ↑/↓ = volume.
- Tab — jump area: Library → Pads → Timeline.
- Space — Play (focused pad; arrangement on the Timeline).
- Enter — primary: Library load/open · Pad edit-clip (scratch: whip) · Timeline place hit.
- M — context menu for the long tail (per area).
- Esc — back / cancel.

## Per area
- Library: Enter load→selected pad / open folder · M: rename, delete, move, filter.
- Pad: Space play (wiki on scratch) · Enter edit clip (whip on scratch) · ↑↓ volume · M: load, kind, on/off, save, export, unload, phrase.
- Timeline: ←→ step, ↑↓ lane · Space play · Enter toggle hit · M: record, cut, region, clear, render, tempo.

## Notes
- Retire the per-feature keys (j,k,l,a/d/w/s,;,f,u,e,S,O,E,P,C,,/.,-,=,[,],{,},x,R,m,p,t,1-8); methods stay, the menu routes to them.
- Retire the full-screen `t` editor — the focused Timeline strip IS the editor (shows a cursor).
