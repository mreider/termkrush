//! Smoke tests for the audio assertion harness in `tests/common/audio.rs`.
//! Exercises every helper on a happy path, and proves each failing helper
//! produces a diagnostic that names the offending sample range.

mod common;

use common::audio::{assert_no_clicks, assert_rms_within, assert_silent, golden_snapshot};
use std::f32::consts::PI;

/// A mono sine buffer: `secs` seconds at `sr` Hz, amplitude `amp`.
fn sine(freq: f32, sr: u32, secs: f32, amp: f32) -> Vec<f32> {
    let n = (sr as f32 * secs) as usize;
    (0..n)
        .map(|i| amp * (2.0 * PI * freq * i as f32 / sr as f32).sin())
        .collect()
}

#[test]
fn rms_within_accepts_known_level() {
    // A sine of amplitude 0.5 has RMS 0.5/sqrt(2) ≈ 0.3536 -> ≈ -9.03 dBFS.
    let buf = sine(440.0, 44100, 1.0, 0.5);
    assert_rms_within(&buf, -9.03, 0.2);
}

#[test]
fn silent_accepts_zeros() {
    let buf = vec![0.0f32; 1024];
    assert_silent(&buf, -60.0);
}

#[test]
fn no_clicks_on_smooth_sine() {
    // Adjacent steps of a 440 Hz / 44.1 kHz sine are ~0.03, well under the
    // click threshold.
    let buf = sine(440.0, 44100, 1.0, 0.6);
    assert_no_clicks(&buf);
}

#[test]
fn golden_snapshot_round_trips() {
    // Short, fully deterministic buffer pinned under tests/golden/.
    let buf = sine(220.0, 8000, 0.25, 0.4);
    golden_snapshot("sine_220_smoke", &buf);
}

// ---- failure diagnostics: each names the offending range ----

#[test]
#[should_panic(expected = "click detected between samples")]
fn no_clicks_reports_discontinuity_index() {
    // A hard jump from -0.9 to +0.9 between samples 2 and 3.
    let buf = vec![0.0, 0.1, -0.9, 0.9, 0.0];
    assert_no_clicks(&buf);
}

#[test]
#[should_panic(expected = "peaks at")]
fn silent_reports_loudest_sample() {
    let mut buf = vec![0.0f32; 64];
    buf[40] = 0.5; // loud sample at a known index
    assert_silent(&buf, -60.0);
}

#[test]
#[should_panic(expected = "dB from expected")]
fn rms_within_reports_measured_level() {
    let buf = sine(440.0, 44100, 1.0, 0.5); // ≈ -9 dB, not -30
    assert_rms_within(&buf, -30.0, 1.0);
}
