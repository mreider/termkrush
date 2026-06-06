---
title: 'Global backlog work-gate: block edits in any am repo when no story is started (cwd-independent hook)'
type: chore
created: "2026-06-06T13:31:13Z"
modified: "2026-06-06T13:35:05Z"
author: Matt Reider
status: accepted
started: "2026-06-06T13:31:27Z"
finished: "2026-06-06T13:35:05Z"
delivered: "2026-06-06T13:35:05Z"
accepted: "2026-06-06T13:35:05Z"
project: termkrush
---

## Why this is a chore

Cross-cutting workflow enforcement. The backlog-first rule was real but **unenforced for our setup**: the per-repo `started-story-gate.sh` only fires when Claude Code is launched *from* the termkrush repo. This session runs from `~/chats` and edits repo files by absolute path, so termkrush's `.claude/settings.json` (and its gate) never load — edits sailed through ungated.

## What was done

- **`.claude/hooks/am-work-gate.sh`** — a generalized, cwd-independent PreToolUse gate. Instead of assuming a fixed repo, it keys off the *edited file's* path: walk up to the nearest `.am/` (the am repo root), find its `_priority.md` stories dir, and apply the rule there.
  - Not an Edit/Write/NotebookEdit → allow.
  - File not inside an am repo → allow (non-am projects untouched).
  - Bare `.am/` with no `_priority.md` backlog → allow (not managed yet).
  - Backlog files / `.am/` / `.claude/` / `.cursor/` / `CLAUDE.md` etc. → allow (you must manage the work).
  - ≥1 story `started` → allow; otherwise **BLOCK (exit 2)** with the rule + next move.
- **Installed globally**: copied to `~/.claude/hooks/am-work-gate.sh` and wired into `~/.claude/settings.json` PreToolUse for Edit/Write/NotebookEdit, so it fires for **every** session regardless of launch dir.
- Documented in `CLAUDE.md` (Enforcement section).

## Acceptance

- [x] Edits in an am repo with no started story are refused (exit 2, coach message). Verified across 7 scenarios (block / allow / story-file / .claude / non-am / bare-.am / started).
- [x] Non-am projects and partially-initialized repos are never gated.
- [x] Works regardless of the directory the agent launched from (keys off the edited file path).
- [x] Canonical script version-controlled in the repo; installed copy + global wiring in `~/.claude`.

## Follow-up (not in scope)

Productize into `am init` so the binary ships the script and installs the global hook automatically (needs am's own backlog stood up first). Tracked as the original "build it into the binary" idea, deferred.
