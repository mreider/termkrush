//! TermKrush — a keyboard-first terminal DJ application.
//!
//! This is the library crate; the `termkrush` binary (`src/main.rs`) is a
//! thin shell over it. The real work lives in a handful of focused
//! modules, exposed here so both the binary and the integration tests in
//! `tests/` can consume them:
//!
//! - [`audio`]   — output device, decoding, resampling, the realtime path.
//! - [`deck`]    — a single playing track: transport, pitch, cue points.
//! - [`mix`]     — combining decks: crossfader, sync, master bus, FX.
//! - [`tui`]     — the ratatui/crossterm interface and key handling.
//! - [`library`] — local track storage, downloads, metadata.
//! - [`config`]  — user configuration and key bindings.

// Scaffold stage: several module placeholders are not yet wired into the
// binary, so they read as dead code to the compiler. This crate-level
// allow keeps `cargo clippy -- -D warnings` green while the foundation is
// laid; it is removed once the decks and TUI start calling into these
// modules (the one-deck and TUI-shell stories).
#![allow(dead_code)]

pub mod audio;
pub mod config;
pub mod deck;
pub mod library;
pub mod logging;
pub mod mix;
pub mod tui;

/// Shared test rigging (signal generators + measurements) for the clip
/// engine and audio tests. Compiled only under test.
#[cfg(test)]
pub mod test_support;
