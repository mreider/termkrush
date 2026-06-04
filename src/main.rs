//! TermKrush — a keyboard-first terminal DJ application.
//!
//! This is the binary entry point. The real work lives in a handful of
//! focused modules, each stubbed here so the rest of the backlog has a
//! place to grow into:
//!
//! - [`audio`]   — output device, decoding, resampling, the realtime path.
//! - [`deck`]    — a single playing track: transport, pitch, cue points.
//! - [`mix`]     — combining decks: crossfader, sync, master bus, FX.
//! - [`tui`]     — the ratatui/crossterm interface and key handling.
//! - [`library`] — local track storage, downloads, metadata.
//! - [`config`]  — user configuration and key bindings.
//!
//! At this stage every module exposes a placeholder so the workspace
//! compiles end to end; behavior arrives story by story.

// Scaffold stage: the module placeholders below are intentionally not yet
// wired into `main`, so they read as dead code to the compiler. This
// crate-level allow keeps `cargo clippy -- -D warnings` green while the
// foundation is laid; it is removed once the decks and TUI start calling
// into these modules (the one-deck and TUI-shell stories).
#![allow(dead_code)]

mod audio;
mod config;
mod deck;
mod library;
mod logging;
mod mix;
mod tui;

/// The human-facing version string, e.g. "TermKrush v0.1.0".
fn version_banner() -> String {
    format!("TermKrush v{}", env!("CARGO_PKG_VERSION"))
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let log_to_file = args.iter().any(|a| a == "--log");

    // Keep the guard alive for the whole run: dropping it flushes the
    // non-blocking JSON writer.
    let _log_guard = logging::init(log_to_file);
    logging::install_panic_hook();

    tracing::info!(version = env!("CARGO_PKG_VERSION"), "termkrush starting");

    // Hidden hook so an integration test can exercise the panic path and
    // assert a crash line was written.
    if args.iter().any(|a| a == "--panic-test") {
        panic!("synthetic panic for crash-hook test");
    }

    // The version banner is program output (stdout), not a diagnostic.
    println!("{}", version_banner());
}

#[cfg(test)]
mod tests {
    use super::*;

    // The scaffold story's acceptance criterion: the binary prints
    // "TermKrush v<version>". Lock the banner shape so a future refactor
    // can't silently change what the freshly built binary greets with.
    #[test]
    fn banner_has_name_and_version() {
        let banner = version_banner();
        assert!(banner.starts_with("TermKrush v"), "got: {banner}");
        // The version segment is non-empty and comes from Cargo metadata.
        assert_eq!(banner, format!("TermKrush v{}", env!("CARGO_PKG_VERSION")));
        assert!(banner.len() > "TermKrush v".len());
    }
}
