#!/usr/bin/env bash
# Drop a properly-typed bug at the top of priority, pre-filled with a body
# template, and open it for editing. This is the frictionless front door
# for the project's "no fix without a bug" rule.
#
# It wraps `am bug`, which in one shot creates the item, sets type=bug,
# strips any estimate, and ranks it to the top of priority. This script
# adds the bug-report body template and opens your editor.
#
# Usage:
#   scripts/file-bug.sh "Bug: <one-line symptom>"
#
# Env:
#   EDITOR              editor to open the new story in (default: vi)
#   FILE_BUG_NO_EDITOR  if set, skip opening the editor (for CI/tests)
set -euo pipefail

if [ "$#" -lt 1 ]; then
  echo "usage: scripts/file-bug.sh \"Bug: <symptom>\"" >&2
  exit 2
fi
TITLE="$*"

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BACKLOG_DIR="${REPO_ROOT}/termkrush" # the project's backlog folder
cd "${BACKLOG_DIR}"

# Create the bug (top of priority, type=bug, no estimate) and recover the
# file name from `am bug`'s output line: "bug created: <file> (...)".
OUT="$(am bug "${TITLE}" 2>/dev/null)"
echo "${OUT}"
FILE="$(printf '%s\n' "${OUT}" | sed -n 's/^bug created: \([^ ]*\) .*/\1/p')"
if [ -z "${FILE}" ] || [ ! -f "${BACKLOG_DIR}/${FILE}" ]; then
  echo "file-bug: could not determine the created story file" >&2
  exit 1
fi

# Fill the body with the bug-report template (replaces the default body).
am set-description "${FILE}" <<'BODY'
## Symptom

<one-line description of what is wrong>

## Repro steps

1.
2.
3.

## Expected

<what should happen>

## Actual

<what happens instead, including any error output>

## Environment

- OS:
- Rust (`rustc --version`):
- TermKrush commit:

## Related story

<slug of the accepted story this defect is against, if known>
BODY

echo "filed: ${BACKLOG_DIR}/${FILE}"

if [ -z "${FILE_BUG_NO_EDITOR:-}" ] && [ -t 1 ]; then
  "${EDITOR:-vi}" "${BACKLOG_DIR}/${FILE}"
else
  echo "(skipping editor; edit ${FILE} to fill in the template, then \`am pull\` to start)"
fi
