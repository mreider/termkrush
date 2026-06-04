---
name: file-bug
description: >-
  File a bug story at the top of the TermKrush priority backlog when a test
  fails, a regression appears, or any defect surfaces against accepted work.
  Use whenever the "no fix without a bug" rule applies — before touching code
  to fix something. Creates a properly typed (type=bug, no estimate) story at
  the top of priority with a repro template.
---

# File a bug

TermKrush follows **no fix without a bug**: when a test goes red, a perf
assertion regresses, a flake appears, or a manual check reveals broken
behavior against an *accepted* story, file a bug story first and fix under
it — never piggyback a fix on another story's commit.

## How to file

Run the helper from the repo root:

```sh
scripts/file-bug.sh "Bug: <one-line symptom>"
```

It wraps `am bug`, which creates the item, sets `type: bug`, strips any
estimate, and ranks it to the **top of priority** in one shot — then fills
the body with a repro template and opens it in `$EDITOR`. Set
`FILE_BUG_NO_EDITOR=1` to skip the editor (CI/non-interactive).

Equivalent one-liner without the template/editor step:

```sh
cd termkrush && am bug "Bug: <symptom>"
```

## Fill in the template

The new story body has: **Symptom, Repro steps, Expected, Actual,
Environment, Related story**. Fill each in — the acceptance criteria for
the bug are "the repro no longer reproduces and a test pins it."

## Then work it

```sh
am pull                         # picks up the top-of-priority bug; gate opens
# write a failing test that reproduces it, then make it pass
am finish termkrush/<slug>.md   # bugs shortcut finish -> accepted
```

## Rules

- Bugs are **never estimated**.
- Bugs default to the **top of priority** (the PM can reorder).
- A **rejection is not a bug**: if a delivered story is rejected, fix it
  under the same story and re-deliver. Bugs are for defects against work
  already **accepted**.
