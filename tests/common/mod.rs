//! Shared helpers for the integration test suite.
//!
//! Cargo compiles `tests/common/` as a module that individual integration
//! test files pull in with `mod common;`. Keeping it under `common/` (not
//! a top-level `tests/*.rs`) means Cargo does not try to run it as its own
//! test binary.

pub mod fixtures;
