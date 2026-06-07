//! Shared helpers for the integration test suite.
//!
//! Cargo compiles `tests/common/` as a module that individual integration
//! test files pull in with `mod common;`. Keeping it under `common/` (not
//! a top-level `tests/*.rs`) means Cargo does not try to run it as its own
//! test binary.

// Cargo compiles `common` into every integration test binary, but no
// single binary uses every helper — `fixtures_test` doesn't touch the
// audio helpers, `audio_harness_test` doesn't touch the fixtures, and so
// on. That makes the unused items look dead per-binary and trips
// `-D warnings`. Allowing dead_code on the shared module (and, via lint
// hierarchy, its submodules) is the idiomatic fix for shared test rigging.
#![allow(dead_code)]

pub mod audio;
pub mod fixtures;
