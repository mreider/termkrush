//! Integration tests for logging + the crash-logging panic hook.
//!
//! These run the built `termkrush` binary as a subprocess (Cargo exposes
//! its path via `CARGO_BIN_EXE_termkrush`) with `TERMKRUSH_DATA_DIR`
//! pointed at a throwaway temp dir, so they never touch `$HOME`.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn unique_tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("termkrush-{}-{}", tag, std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create temp data dir");
    dir
}

fn read_dir_concat(dir: &PathBuf) -> String {
    let mut out = String::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for e in entries.flatten() {
            if let Ok(s) = fs::read_to_string(e.path()) {
                out.push_str(&s);
            }
        }
    }
    out
}

#[test]
fn log_flag_writes_json_log() {
    let data = unique_tmp("logtest");
    let status = Command::new(env!("CARGO_BIN_EXE_termkrush"))
        .arg("--log")
        .env("TERMKRUSH_DATA_DIR", &data)
        .env_remove("RUST_LOG")
        .output()
        .expect("run termkrush --log");
    assert!(status.status.success(), "expected clean exit with --log");

    let log_dir = data.join("log");
    let contents = read_dir_concat(&log_dir);
    assert!(
        contents.contains("starting"),
        "expected a JSON log line mentioning startup under {}, got: {contents:?}",
        log_dir.display()
    );
    // It's JSON lines: each record is an object.
    assert!(
        contents.contains("\"level\":\"INFO\"") || contents.contains("\"fields\""),
        "expected JSON-structured log, got: {contents:?}"
    );

    let _ = fs::remove_dir_all(&data);
}

#[test]
fn panic_writes_crash_line() {
    let data = unique_tmp("crashtest");
    let out = Command::new(env!("CARGO_BIN_EXE_termkrush"))
        .arg("--panic-test")
        .env("TERMKRUSH_DATA_DIR", &data)
        .output()
        .expect("run termkrush --panic-test");
    // A panic exits non-zero (101 under the default unwind hook).
    assert!(!out.status.success(), "panic path should exit non-zero");

    let crash = data.join("log").join("crash.log");
    let line = fs::read_to_string(&crash)
        .unwrap_or_else(|e| panic!("crash.log missing at {}: {e}", crash.display()));
    assert!(line.contains("PANIC at"), "crash line malformed: {line:?}");
    assert!(
        line.contains("synthetic panic for crash-hook test"),
        "crash line should carry the panic message: {line:?}"
    );
    // The crash line names a source location (file:line).
    assert!(
        line.contains("src/main.rs:"),
        "crash line should name a location: {line:?}"
    );

    let _ = fs::remove_dir_all(&data);
}
