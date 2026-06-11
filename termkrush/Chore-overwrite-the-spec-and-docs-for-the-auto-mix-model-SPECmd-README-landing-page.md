---
title: 'Chore: overwrite the spec and docs for the auto-mix model (SPEC.md, README, landing page)'
type: chore
created: "2026-06-11T14:59:05Z"
modified: "2026-06-11T15:10:08Z"
author: Matt Reider
status: accepted
started: "2026-06-11T15:04:02Z"
delivered: "2026-06-11T15:10:08Z"
accepted: "2026-06-11T15:10:08Z"
project: termkrush
---

## Goal

The PM wants the written product to match the 2026-06-11 auto-mix pivot *now*: every doc that still describes pads/timeline/platter gets overwritten for the sequence-line + mix-grammar-engine model.

## Scope

- `docs/SPEC.md` — full respec: the grammar table with the measured reference-mix numbers, the three surfaces, done vs backlog vs retired, out of scope.
- `README.md` — product description, "Using it" for the three surfaces, config (sequence.txt / beats.txt), layout table.
- `index.html` (landing page) — copy only (title, eyebrow, tagline, the three feature cells → sequence / tap / krush); CRT styling untouched.
- `docs/SMOKE.md` — was still the retired TUI checklist; rewritten as the GUI checklist for the current surfaces + pending render checks.
- `CLAUDE.md` — drop the dead `dev-run.sh tui` recipe line.
- Housekeeping: dedupe the DNS story (merged the real body into the original slug, deleted the husk duplicate).

## Acceptance

- No doc tells a user to drag a pad, use a timeline, or scratch a platter.
- SPEC §2 carries the measured grammar numbers so the engine stories can cite it.
- Landing page copy renders correctly (structure/styles untouched).

## Comments

## Attachments
