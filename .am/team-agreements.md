# Team agreements

Soft rules. The dev pair surfaces conflicts at the relevant moment; the PM can override with a reason.

## Engineering

- Every commit passes `cargo fmt --check` and `cargo clippy -- -D warnings`.
- Every PR runs the full test suite on macOS, Linux, and Windows via GitHub Actions.
- New audio paths get at least one integration test that decodes a fixture file and asserts on output samples or RMS.
- No `unwrap()` / `expect()` in release-mode code paths outside of `main.rs` startup.
- Logging uses `tracing`. No `println!` for diagnostics.

## Tests and bugs (Pivotal flow)

- **Tests are part of the definition of done.** A feature story is not finishable until tests covering its new behavior pass locally and in CI. The acceptance criteria are the spec for those tests.
- **No fix without a bug.** When a test fails (CI or local), or any defect is found against accepted work, file a bug story with `am create-item "Bug: ..." --target priority --position top --type bug`. Then `am pull` it. **Do not silently fix in another story's commit.**
- **Bugs are not estimated.** Strip points if asked to add them.
- **Bugs go to the top.** Filed bugs default to the top of priority. The PM can reorder.
- **Performance regressions are bugs.** A failing perf assertion is a bug; file it, pull it, fix it under that story.
- **Flaky tests are bugs.** A test that passes/fails non-deterministically is a defect — file a bug, do not retry-on-flake.
- **Test infrastructure changes are chores.** New harnesses, fixtures, coverage, CI rigging carry no estimate.
- **Rejected stories don't become new bugs.** If a delivered story is rejected by the PM, that's a rejection on the same story — fix it under the original story and re-deliver. Bugs are for defects against *accepted* work.

## Scope & flow

- Features over 8 points are epics. Split before pulling.
- Bugs and chores carry no point estimate.
- A story is not finished until acceptance criteria render green in a manual demo or an automated test.
- A release marker only lands after the preceding feature stories are accepted.
- Nice-to-haves live in the icebox; do not sneak them into priority without a story.

## Design

- The TUI honors the CRT palette (amber `#ffb000`, green `#45f07d`, dark bg `#060907`) where reasonable.
- The landing page (`docs/`) stays in lockstep with the design language of the wordmark.
- Keyboard-first. Every action reachable from the keyboard before any mouse/touch consideration.

## Releases

- Release tags are `vX.Y.Z`. The `release` story carries `release_date`.
- A release is a date marker only — no status flow, no estimate, no acceptance.
- `v0.1.0 spins` is the first usable release. Nothing tagged before then.