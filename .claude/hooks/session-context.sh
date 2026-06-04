#!/usr/bin/env bash
# TermKrush SessionStart hook: prints the dashboard, next pull, and any started
# stories so every session begins with the same shared context the PM and dev
# pair would have on Pivotal's web UI.
#
# Output goes into the session's context as a system reminder.

set -uo pipefail

REPO_ROOT="/Users/matt/cutting/termkrush"

if ! command -v am >/dev/null 2>&1; then
  echo "agilemarkdown (am) is not on PATH; install it to see backlog context."
  exit 0
fi

cd "$REPO_ROOT" 2>/dev/null || exit 0

echo "## TermKrush backlog state"
echo
echo "### Dashboard"
echo '```'
am dashboard 2>/dev/null | grep -v "can't set bash autocomplete" || true
echo '```'
echo
echo "### Top of priority"
echo '```'
am next 2>/dev/null | grep -v "can't set bash autocomplete" || true
echo '```'
echo

# Started stories (WIP)
started=()
if compgen -G "$REPO_ROOT/termkrush/*.md" >/dev/null; then
  while IFS= read -r f; do
    [ -n "$f" ] && started+=("$f")
  done < <(grep -l '^status: started$' "$REPO_ROOT"/termkrush/*.md 2>/dev/null || true)
fi

if [ ${#started[@]} -gt 0 ]; then
  echo "### Stories in progress (WIP)"
  for f in "${started[@]}"; do
    title=$(grep -m1 '^title:' "$f" | sed 's/^title: *//; s/^"//; s/"$//')
    rel="${f#$REPO_ROOT/}"
    echo "- $title  ($rel)"
  done
else
  echo "### Stories in progress"
  echo "_None. The next code edit will be refused until \`am pull <slug>\` starts a story._"
fi
echo
echo "_The coach gate refuses code edits while no story is started. The backlog "
echo "and \`.claude/\`, \`.am/\`, \`.cursor/\` paths are always editable._"
