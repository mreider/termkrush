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

/// Play a 440 Hz sine for `secs` seconds on the default output. Returns a
/// process exit code: 0 on success, 1 if the output device is
/// unavailable (logged gracefully, never panics).
fn run_test_tone(secs: f32) -> i32 {
    use audio::{AudioOutput, Sink, SineSink};
    use std::time::Duration;

    let (out, mut producer) = match AudioOutput::start(1 << 15) {
        Ok(pair) => pair,
        Err(e) => {
            tracing::error!(error = %e, "audio: cannot start output for --test-tone");
            eprintln!("termkrush: audio output unavailable: {e}");
            return 1;
        }
    };

    let sample_rate = out.sample_rate;
    let channels = out.channels;
    eprintln!("termkrush: playing 440 Hz test tone for {secs:.1}s at {sample_rate} Hz, {channels} ch");

    let mut sink = SineSink::new(440.0, sample_rate, 0.3, channels);
    let total = (sample_rate as f32 * secs) as usize * channels as usize;
    let mut scratch = vec![0.0f32; 1024];
    let mut fed = 0usize;
    while fed < total {
        sink.fill(&mut scratch);
        for &s in &scratch {
            // Spin briefly when the ring is full; the callback drains it.
            while producer.push(s).is_err() {
                std::thread::sleep(Duration::from_micros(200));
            }
            fed += 1;
            if fed >= total {
                break;
            }
        }
    }

    // Let the callback drain the tail before the stream drops.
    std::thread::sleep(Duration::from_millis(250));
    tracing::info!(xruns = out.xruns(), "audio: test tone complete");
    0
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

    // Audible smoke test: play a 440 Hz sine on the default output, then
    // exit. Verifies the cpal stream + ring-buffer path. An optional
    // duration in seconds follows the flag (`--test-tone 10`); defaults to
    // a quick 2s listen.
    if let Some(i) = args.iter().position(|a| a == "--test-tone") {
        let secs = args
            .get(i + 1)
            .and_then(|s| s.parse::<f32>().ok())
            .filter(|s| *s > 0.0)
            .unwrap_or(2.0);
        std::process::exit(run_test_tone(secs));
    }

    // Launch the fullscreen TUI when attached to a real terminal. When
    // stdout is piped (tests, CI, `termkrush | cat`), there is no usable
    // terminal to take over, so just print the version banner and exit —
    // which keeps the binary scriptable.
    use std::io::IsTerminal;
    if std::io::stdout().is_terminal() && !args.iter().any(|a| a == "--no-tui") {
        if let Err(e) = tui::run() {
            tracing::error!(error = %e, "tui exited with error");
            eprintln!("termkrush: {e}");
            std::process::exit(1);
        }
    } else {
        // The version banner is program output (stdout), not a diagnostic.
        println!("{}", version_banner());
    }
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
