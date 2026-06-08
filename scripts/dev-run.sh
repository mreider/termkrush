#!/usr/bin/env bash
# Build once, run many. The acceptance-time companion to `cargo run` that does
# NOT recompile on every invocation: `build` compiles the binary, and every
# other action runs the *already-built* binary directly, so you can exercise a
# delivered story as many times as acceptance needs with zero rebuild cost.
#
# Recipes map a story's behavior to its run command, so accepting a story is
# "run the recipe" rather than "remember the raw flags".
#
# Usage:
#   scripts/dev-run.sh build            # compile the debug binary once
#   scripts/dev-run.sh                  # run prebuilt binary -> TUI (default)
#   scripts/dev-run.sh tui              # same as default
#   scripts/dev-run.sh tone [secs]      # --test-tone [secs]   (default 2s)
#   scripts/dev-run.sh panic            # --panic-test         (crash-hook path)
#   scripts/dev-run.sh list             # print available recipes
#   scripts/dev-run.sh watch            # cargo watch -x build (rebuild on save)
#   scripts/dev-run.sh <recipe> -- ...  # forward extra args to the binary
#
# Flags:
#   --release   target the release binary (target/release) instead of debug.
#
# Notes:
#   - Only `build` and `watch` invoke cargo. Plain recipes exec the prebuilt
#     binary, so they never recompile. If the binary is missing, a plain recipe
#     auto-builds ONCE and then runs.
#   - Resolves the repo root from its own location, so it works from any cwd.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${REPO_ROOT}"

PROFILE="debug"
CARGO_PROFILE_FLAG=() # empty for debug; --release for release

# Pull out --release wherever it appears; collect the rest as positionals.
ARGS=()
for a in "$@"; do
  case "$a" in
    --release) PROFILE="release"; CARGO_PROFILE_FLAG=(--release) ;;
    *) ARGS+=("$a") ;;
  esac
done
set -- "${ARGS[@]+"${ARGS[@]}"}"

BIN="${REPO_ROOT}/target/${PROFILE}/termkrush"

usage() {
  sed -n '2,30p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
}

build() {
  echo "dev-run: cargo build (${PROFILE})" >&2
  cargo build "${CARGO_PROFILE_FLAG[@]+"${CARGO_PROFILE_FLAG[@]}"}"
}

# Ensure the binary exists without forcing a rebuild when it already does.
# This is the heart of "build once, run many": a plain run only compiles when
# there is nothing to run yet.
ensure_built() {
  if [ ! -x "${BIN}" ]; then
    echo "dev-run: ${PROFILE} binary missing, building once..." >&2
    build
  fi
}

# Collect the forwarded args into the global EXTRA array, dropping a single
# leading "--" so callers can write
#   dev-run.sh tone -- --log
# and have --log reach the binary. Implemented with a global array (not
# `mapfile`) so it runs on macOS's stock bash 3.2.
EXTRA=()
collect_extra() {
  EXTRA=()
  local seen_sep=0 a
  for a in "$@"; do
    if [ "${seen_sep}" -eq 0 ] && [ "${a}" = "--" ]; then
      seen_sep=1
      continue
    fi
    EXTRA+=("${a}")
  done
}

RECIPE="${1:-gui}"
shift || true

case "${RECIPE}" in
  build)
    build
    ;;

  list)
    cat <<'RECIPES'
Recipes (run the prebuilt binary; no recompile):
  dev            build, THEN launch the GUI (use while developing)
  gui            launch the egui desktop app               [default]
  tui            launch the legacy terminal UI             (--tui)
  tone [secs]    play a test tone for [secs] seconds       (--test-tone)
  panic          trigger the panic/crash-hook path         (--panic-test)

Build / watch (invoke cargo):
  build          compile the debug binary once
  watch          cargo watch -x build (rebuild on save)

Flags:
  --release      use the release binary instead of debug

Append `-- <args>` to forward extra flags to termkrush, e.g.
  scripts/dev-run.sh tui -- --log
RECIPES
    ;;

  watch)
    if command -v cargo-watch >/dev/null 2>&1; then
      echo "dev-run: cargo watch -x build (${PROFILE})" >&2
      cargo watch -x "build ${CARGO_PROFILE_FLAG[*]}"
    else
      echo "dev-run: cargo-watch not installed. Install it with:" >&2
      echo "  cargo install cargo-watch" >&2
      exit 1
    fi
    ;;

  dev)
    # Build, then launch the GUI — the one command to run while developing,
    # so you always see your latest changes.
    build
    collect_extra "$@"
    exec "${BIN}" "${EXTRA[@]+"${EXTRA[@]}"}"
    ;;

  gui)
    ensure_built
    collect_extra "$@"
    exec "${BIN}" "${EXTRA[@]+"${EXTRA[@]}"}"
    ;;

  tui)
    ensure_built
    collect_extra "$@"
    exec "${BIN}" --tui "${EXTRA[@]+"${EXTRA[@]}"}"
    ;;

  tone)
    ensure_built
    SECS="2"
    if [ "${1:-}" != "" ] && [ "${1:-}" != "--" ]; then
      SECS="$1"; shift
    fi
    collect_extra "$@"
    exec "${BIN}" --test-tone "${SECS}" "${EXTRA[@]+"${EXTRA[@]}"}"
    ;;

  panic)
    ensure_built
    collect_extra "$@"
    exec "${BIN}" --panic-test "${EXTRA[@]+"${EXTRA[@]}"}"
    ;;

  -h|--help|help)
    usage
    ;;

  *)
    echo "dev-run: unknown recipe '${RECIPE}'" >&2
    echo "Run 'scripts/dev-run.sh list' to see recipes." >&2
    exit 2
    ;;
esac
