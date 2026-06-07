//! Structured logging and a crash-logging panic hook.
//!
//! Audio bugs are timing bugs, and timing bugs are invisible without a
//! log. This module wires `tracing` before any audio code runs:
//!
//! - Diagnostics go through `tracing`, never `println!`.
//! - A human-friendly layer writes to stderr, but only when `RUST_LOG` is
//!   set — off by default so a normal run is quiet.
//! - With `--log`, structured JSON lines are written to a daily-rolling
//!   file under the data dir's `log/`.
//! - A panic hook records a one-line crash summary to `log/crash.log`
//!   (always, even without `--log`) and emits a `tracing` error, then
//!   defers to the default hook.
//!
//! The data dir is `~/.termkrush` by default, overridable with
//! `TERMKRUSH_DATA_DIR` (used by tests so they never touch `$HOME`).

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::prelude::*;
use tracing_subscriber::{fmt, EnvFilter};

/// The TermKrush data directory (runtime store + logs).
pub fn data_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("TERMKRUSH_DATA_DIR") {
        return PathBuf::from(d);
    }
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.join(".termkrush")
}

/// Where JSON logs and the crash file live.
pub fn log_dir() -> PathBuf {
    data_dir().join("log")
}

/// Initialize the global tracing subscriber.
///
/// When `log_to_file` is true, JSON lines are appended to a daily-rolling
/// file under [`log_dir`]; the returned [`WorkerGuard`] must be held for
/// the lifetime of the program (drop flushes the non-blocking writer). A
/// human-readable stderr layer is added only when `RUST_LOG` is set.
pub fn init(log_to_file: bool) -> Option<WorkerGuard> {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    // Off by default: only log to stderr when the operator opts in.
    let stderr_layer = if std::env::var_os("RUST_LOG").is_some() {
        Some(fmt::layer().with_writer(std::io::stderr))
    } else {
        None
    };

    let mut guard = None;
    let file_layer = if log_to_file {
        let dir = log_dir();
        if let Err(e) = fs::create_dir_all(&dir) {
            eprintln!("termkrush: cannot create log dir {}: {e}", dir.display());
        }
        let appender = tracing_appender::rolling::daily(&dir, "termkrush.jsonl");
        let (non_blocking, g) = tracing_appender::non_blocking(appender);
        guard = Some(g);
        Some(fmt::layer().json().with_writer(non_blocking))
    } else {
        None
    };

    tracing_subscriber::registry()
        .with(filter)
        .with(stderr_layer)
        .with(file_layer)
        .init();

    guard
}

/// Install a panic hook that records a one-line crash summary to
/// `log/crash.log` and emits a `tracing` error before deferring to the
/// previously installed hook (so the usual backtrace still prints).
pub fn install_panic_hook() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let location = info
            .location()
            .map(|l| format!("{}:{}", l.file(), l.line()))
            .unwrap_or_else(|| "unknown".to_string());
        let message = panic_message(info.payload());

        tracing::error!(location = %location, message = %message, "panic: process crashing");
        write_crash_line(&location, &message);

        default(info);
    }));
}

/// Best-effort extraction of a panic payload's message.
fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "<non-string panic payload>".to_string()
    }
}

/// Append a single crash line to `log/crash.log`. Synchronous and
/// independent of the JSON appender so a crash is recorded even when
/// `--log` was not passed.
fn write_crash_line(location: &str, message: &str) {
    let dir = log_dir();
    let _ = fs::create_dir_all(&dir);
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    if let Ok(mut f) = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("crash.log"))
    {
        let _ = writeln!(f, "{ts} PANIC at {location}: {message}");
    }
}
