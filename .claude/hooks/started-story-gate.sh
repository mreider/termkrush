#!/usr/bin/env bash
# Bigpoppa: enforce Pivotal-style WIP discipline.
#
# Refuses Edit / Write / NotebookEdit on any file inside the bigpoppa repo
# unless at least one story is in `started` state. This makes code edits
# illegal without an explicit pulled story, the same rule Pivotal teams
# enforced manually.
#
# Always allowed (backlog and agent infra; required for managing the work):
#   - bigpoppa/bigpoppa/*.md      (story files)
#   - bigpoppa/.am/*              (config, cache, generated views)
#   - bigpoppa/.claude/*          (agent config and hooks)
#   - bigpoppa/.cursor/*          (cursor rules)
#   - bigpoppa/CLAUDE.md, AGENTS.md, .github/copilot-instructions.md
#   - bigpoppa/.gitignore
#   - any path outside the bigpoppa repo (e.g. /tmp/ scratch files)
#
# Exits:
#   0 — allowed (the tool call proceeds)
#   2 — refused (Claude Code shows the message to the user and aborts)

set -euo pipefail

REPO_ROOT="/Users/matt/cutting/bigpoppa"
BACKLOG_DIR="$REPO_ROOT/bigpoppa"

input="$(cat)"

tool_name="$(printf '%s' "$input" | python3 -c 'import json,sys
d=json.load(sys.stdin); print(d.get("tool_name",""))' 2>/dev/null || true)"

case "$tool_name" in
  Edit|Write|NotebookEdit) ;;
  *) exit 0 ;;
esac

file_path="$(printf '%s' "$input" | python3 -c 'import json,sys
d=json.load(sys.stdin)
ti=d.get("tool_input",{})
print(ti.get("file_path") or ti.get("notebook_path") or "")' 2>/dev/null || true)"

# Outside the repo: never gate. Inside /tmp, /Users/.claude, etc., allowed.
case "$file_path" in
  "$REPO_ROOT"/*) ;;
  *) exit 0 ;;
esac

# Always-allowed paths inside the repo (backlog and agent infra).
case "$file_path" in
  "$BACKLOG_DIR"/*.md) exit 0 ;;
  "$REPO_ROOT"/.am/*)  exit 0 ;;
  "$REPO_ROOT"/.claude/*) exit 0 ;;
  "$REPO_ROOT"/.cursor/*) exit 0 ;;
  "$REPO_ROOT"/.github/copilot-instructions.md) exit 0 ;;
  "$REPO_ROOT"/CLAUDE.md|"$REPO_ROOT"/AGENTS.md|"$REPO_ROOT"/.gitignore) exit 0 ;;
  "$REPO_ROOT"/.am/team-agreements.md|"$REPO_ROOT"/.am/inception.md|"$REPO_ROOT"/.am/learnings.md) exit 0 ;;
esac

# Count stories currently in `started` state. The `|| true` keeps a no-match
# from tripping `set -e` / pipefail.
started_count=0
if compgen -G "$BACKLOG_DIR/*.md" >/dev/null; then
  started_count=$( { grep -l '^status: started$' "$BACKLOG_DIR"/*.md 2>/dev/null || true; } | wc -l | tr -d ' ')
fi

if [ "${started_count:-0}" -eq 0 ]; then
  rel="${file_path#$REPO_ROOT/}"
  {
    echo "Coach refused: $tool_name on $rel"
    echo
    echo "Rule: code changes happen on a started story. No story is in progress."
    echo "Next:"
    echo "  am next                                # show the top of the backlog"
    echo "  am align bigpoppa/<story>.md           # restate the story, confirm with PM"
    echo "  am pull                                # next + start the top of priority"
    echo "  am start bigpoppa/<story>.md           # start a specific story"
    echo
    echo "Edits to the backlog itself (bigpoppa/*.md, .am/*, .claude/*) are allowed regardless."
  } >&2
  exit 2
fi

exit 0
