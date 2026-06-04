# Team agreements

Soft rules. The dev pair surfaces conflicts at the relevant moment; the PM can override with a reason.

## Engineering

- Every commit passes `cargo fmt --check` and `cargo clippy -- -D warnings`.
- Every PR runs the full test suite on macOS, Linux, and Windows via GitHub Actions.
- New audio paths get at least one integration test that decodes a fixture file and asserts on output samples or RMS.
- No `unwrap()` / `expect()` in release-mode code paths outside of `main.rs` startup.
- Logging uses `tracing`. No `println!` for diagnostics.

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