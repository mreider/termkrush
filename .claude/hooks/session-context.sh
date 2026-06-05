#!/usr/bin/env bash
# TermKrush SessionStart hook: prints `am brief` (project vision + dashboard +
# next pull + WIP + agreements) so every session begins with the same shared
# context the PM and dev pair would have on Pivotal's web UI.
#
# Output goes into the session's context as a system reminder.

set -uo pipefail

# Derive the repo root from this script's own location so a fresh clone
# (any path, any machine) gets full session context without editing this
# file. Falls back to git toplevel if BASH_SOURCE is unavailable.
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." 2>/dev/null && pwd)"
REPO_ROOT="${REPO_ROOT:-$(git rev-parse --show-toplevel 2>/dev/null)}"

if ! command -v am >/dev/null 2>&1; then
  echo "agilemarkdown (am) is not on PATH. Install it to get backlog context:"
  echo "  go install github.com/mreider/agilemarkdown@latest   # or build the sibling agilemarkdown repo"
  exit 0
fi

cd "$REPO_ROOT" 2>/dev/null || exit 0

# `am brief` (agilemarkdown >= 2026.06) prints the whole onboarding blob in
# one shot: project vision (from .am/inception.md), the dashboard, the next
# pull, work in progress, and the working agreements. The grep is defensive
# against older builds that leaked an autocomplete warning onto stdout.
am brief 2>/dev/null | grep -v "can't set bash autocomplete" || true

echo
echo "_Coach gate: code edits are refused while no story is \`started\` — \`am pull\`"
echo "the top of priority first. The backlog (termkrush/*.md) and the \`.am/\`,"
echo "\`.claude/\`, \`.cursor/\` paths are always editable._"
