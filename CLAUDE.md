# TermKrush — agent rules of engagement

This repo is built **the Pivotal way**, top-of-backlog first, one story at a time. The rules below are mandatory for any agent (Claude Code, Cursor, Copilot, future tools) operating in this repo. They are enforced by hooks in `.claude/settings.json`; please do not work around the enforcement — escalate to the PM (Matt) instead.

## Agent setup (fresh clone / new machine)

The entire backlog, the project vision (`.am/inception.md`), the team agreements, and these rules are committed plain-markdown, so you can orient by reading even with no tooling. But the rituals, gates, and dashboard need the **`am` binary** (agilemarkdown). If `am next` isn't found:

```
go install github.com/mreider/agilemarkdown@latest    # puts `am` on your PATH
# ...or build the sibling repo if you have it checked out:
#   (cd ../agilemarkdown && go build -o "$(go env GOPATH)/bin/am" .)
```

`am` powers the `SessionStart` context, the `.mcp.json` MCP server (`am mcp`), and the enforcement hooks. The hooks resolve the repo root from their own location, so the checkout path doesn't matter. Once `am` is on PATH, start a session normally (below) — `am inception --show` is the fastest way to load the project's why/what before pulling.

## How we work

1. **Start a session by reading the dashboard.** The `SessionStart` hook prints `am dashboard` + `am next` into context automatically. If you don't see it, run those commands manually before doing anything else.
2. **Always pull from the top.** Use `am next` to see the top of priority. Do not skip ahead, re-rank, or invent new stories without the PM's explicit say-so.
3. **Align before you code.** Run `am align <slug>` (or `/am-align`) on the story you're about to pull. Restate the intent in one paragraph and confirm with the PM. If anything is ambiguous, ask before writing code — confidently building the wrong thing is the failure mode the seam is here to prevent.
4. **Pull, then code.** `am pull` is `next` + `start` in one shot (it auto-picks the top of priority). Or `am start <path-to-story.md>` once you've already aligned on one. Code edits on any file outside the backlog/agent infra are refused while no story is in `started` (see "Enforcement" below).
5. **Drive the state machine.** `am start` → `am finish` (dev done) → `am deliver` (staged). **Stop there.** The dev pair does not accept its own work. Wait for the PM (Matt) to run `am accept` or `am reject`.
6. **Commit per story.** Each commit message should reference the story slug, e.g. `Scaffold-Rust-project-layout: cargo init, module skeletons`. PRs target one story at a time.
7. **Sync after status changes.** `am sync` regenerates derived views and pushes. The release stories carry a `release_date`; don't change those without the PM.

## Running locally for acceptance

When the PM (or you) wants to *run* a delivered story to verify it, use
`scripts/dev-run.sh` rather than `cargo run` — it builds once, then runs the
already-built binary on every later call, so repeated acceptance runs don't
recompile:

```
scripts/dev-run.sh build        # compile the debug binary once
scripts/dev-run.sh list         # see the per-story recipes
scripts/dev-run.sh gui          # launch the egui desktop app  (default)
scripts/dev-run.sh tui          # launch the legacy terminal UI (--tui)
scripts/dev-run.sh tone 3       # 3s test tone              (cpal output story)
scripts/dev-run.sh panic        # crash-hook path           (logging story)
scripts/dev-run.sh tone 1 -- --log   # forward extra flags after `--`
```

Only `build` and `watch` invoke cargo; plain recipes exec
`target/debug/termkrush` directly. Add `--release` to target the release
binary. When a story adds a new run-mode, add a recipe for it so accepting that
story is "run the recipe".

## Hard rules (refuse if asked to violate)

- **8-point cap.** Features over 8 points are epics. Refuse and offer to split.
- **Bugs and chores carry no estimate.** Strip points if asked to add them.
- **Dev pair does not accept.** Never flip status to `accepted` yourself. Stage at `delivered` and wait.
- **Releases are date markers.** No status flow on `type: release`. They land only after the preceding feature stories are accepted.
- **No release before v0.1.0 krush.** The first usable bar is the auto-mix MVP: build a track list, tap beats once per track, order tracks on the **sequence line** (repeats allowed), and **render a continuous mix where the grammar engine handles everything** — one master tempo (varispeed), 8–16-bar phrase sections, equal-loudness transitions on phrase boundaries, engine-placed scratches/chops, bass drops, and a breathing energy arc — **deterministically (same input → bit-identical mix), with zero knobs**. (The two-deck model was retired in the 2026-06-07 pivot, pads/looper arrived the same day, the GUI pivot followed 2026-06-08, and the pad/timeline model was retired wholesale by the 2026-06-11 auto-mix pivot — see `.am/inception.md`.) Nothing is tagged before that story is delivered and accepted.
- **No fix without a bug.** When a test fails or any defect surfaces against accepted work, file a bug story before touching code. The fix lives under the bug story, not piggybacked on another story's commit. (See "When tests fail" below.)

## When tests fail

CI red, a local test failing, a perf assertion regressing, a manual verification revealing broken behavior against an *accepted* story — every one of those is a **bug**. File it first, then fix under it. The fastest path is the `/file-bug` skill or the helper script (both wrap `am bug`, which creates the item, sets `type: bug`, strips the estimate, and ranks it to the top of priority in one shot):

```
scripts/file-bug.sh "Bug: <one-line symptom>"   # creates + templates the body + opens $EDITOR
# ...or, plain: (cd termkrush && am bug "Bug: <symptom>")
am pull                                          # pick it up next; the gate now allows code edits
# ...write a failing test that reproduces it, then make it pass...
am finish termkrush/<slug>.md                    # bugs shortcut finish -> accepted
```

- Bugs land at the **top of priority** by default. The PM can reorder.
- Bugs are **never estimated**. `coach-check` will refuse `set_estimate` on a bug-typed story.
- A flaky test is a bug. A perf regression is a bug. A docs/build/CI break against accepted work is a bug.
- **Rejection ≠ bug.** If the PM rejects a delivered story, that is `am reject` with a reason; fix under the *same* story and re-deliver. Bugs are only for defects against work already accepted.

## Test infrastructure

Tests for new behavior live with the feature story that adds the behavior — the acceptance criteria are the spec. Shared rigging — fixtures, audio assertion helpers, golden snapshots, coverage tools, CI tweaks — are **chores** (no estimate). If you find yourself wanting to add cross-cutting test infra inside a feature story, stop and file a chore instead; it keeps the feature scope honest and the chore reusable.

## Soft rules (warn, do not refuse)

`.am/team-agreements.md` is the working agreement. Read it every session. Surface conflicts as nudges, quote the agreement line, offer a choice. If the same agreement is overridden three times in a row, append a note to `.am/learnings.md` via `am record-learning "..."`.

## Enforcement

`.claude/hooks/started-story-gate.sh` runs as a PreToolUse hook on `Edit`, `Write`, and `NotebookEdit`. It refuses with the rule + next-move pattern unless at least one story in `termkrush/*.md` has `status: started`. **Always allowed regardless** (so you can manage the backlog itself):

- `termkrush/*.md` — story files
- `.am/*` — config, cache, generated views, team agreements, inception, learnings
- `.claude/*`, `.cursor/*` — agent config
- `CLAUDE.md`, `AGENTS.md`, `.github/copilot-instructions.md`, `.gitignore`
- Anything outside the repo root (e.g. `/tmp/` scratch files)

`.claude/hooks/coach-gate.sh` runs as a PreToolUse hook on the gated MCP tools (`mcp__agilemarkdown__set_status` and `mcp__agilemarkdown__set_estimate`); it enforces the hard rules at state-change time.

**Global (cwd-independent) gate.** `started-story-gate.sh` only fires when the
session is launched *from this repo*. Agents often run from a parent directory
(editing files here by absolute path), so the per-repo hook never triggers and
the rule goes unenforced. `.claude/hooks/am-work-gate.sh` closes that gap: it is
installed into the user's **global** `~/.claude/settings.json` (copied to
`~/.claude/hooks/`) and keys off the *edited file's* path — it walks up to the
file's `.am/` repo and applies the same "no edits without a started story" rule
to **any** am-managed backlog, regardless of where the session launched.
Non-am projects and repos with a bare `.am/` (no `_priority.md` backlog yet) are
never gated. (Productizing this into `am init` so the binary ships + installs it
is a future item.)

The `SessionStart` hook prints the backlog state into context at session start.

## The coach stance

The shared coach guidance lives in `.claude/agilemarkdown-coach.md`. Read it; it explains the role split (dev pair vs PM), the alignment moment, refusal patterns, and the CLI ↔ MCP name map.

@.claude/agilemarkdown-coach.md
