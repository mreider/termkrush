//! TermKrush engine — the headless core.
//!
//! This crate carries **no UI dependency** (no ratatui/crossterm): it is the
//! pure state + logic + DSP, so it can be exercised method-by-method in tests
//! (and, later, driven by a non-TUI front-end). The `termkrush` binary is a
//! thin shell over it.
//!
//! - [`audio`]    — output device, decoding, resampling, BPM, time-stretch.
//! - [`beats`]    — the per-track tapped-beat cache (tap once, ever).
//! - [`clip`]     — a captured clip: the unit the sampler pads play.
//! - [`mix`]      — the master bus + sampler pads/voices.
//! - [`library`]  — local track list (filesystem).
//! - [`sequence`] — the ordered track sequence (the project file).
//! - [`config`]   — user configuration.
//! - [`logging`]  — tracing setup for the binary.

pub mod arrangement;
pub mod audio;
pub mod beats;
pub mod clip;
pub mod config;
pub mod library;
pub mod logging;
pub mod mix;
pub mod scratch;
pub mod sequence;

/// Shared test rigging (signal generators + measurements) for the engine
/// tests. Compiled only under test.
#[cfg(test)]
pub mod test_support;
