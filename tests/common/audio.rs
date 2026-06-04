//! Readable audio assertions and a golden-snapshot harness for the
//! integration suite.
//!
//! Samples are mono `f32` in the nominal range `[-1.0, 1.0]`. Level
//! helpers work in decibels relative to full scale (dBFS): `0 dB` is a
//! peak of `1.0`, silence approaches `-inf`. Every helper that can fail
//! panics with a message that names the offending sample range, so a red
//! test points straight at the spot in the buffer.

use std::fs;
use std::path::{Path, PathBuf};

/// Convert a linear amplitude (>= 0) to dBFS. Zero maps to -inf.
fn lin_to_db(x: f32) -> f32 {
    if x <= 0.0 {
        f32::NEG_INFINITY
    } else {
        20.0 * x.log10()
    }
}

/// Root-mean-square level of the buffer as a linear amplitude.
fn rms_linear(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum_sq: f64 = samples.iter().map(|&s| (s as f64) * (s as f64)).sum();
    (sum_sq / samples.len() as f64).sqrt() as f32
}

/// Index and value of the largest-magnitude sample.
fn peak(samples: &[f32]) -> (usize, f32) {
    let mut idx = 0;
    let mut mag = 0.0f32;
    for (i, &s) in samples.iter().enumerate() {
        if s.abs() > mag {
            mag = s.abs();
            idx = i;
        }
    }
    (idx, mag)
}

/// Assert the RMS level (dBFS) is within `tolerance` dB of `expected`.
///
/// `expected` and `tolerance` are both in decibels. Panics naming the
/// measured level if it falls outside the band.
pub fn assert_rms_within(samples: &[f32], expected: f32, tolerance: f32) {
    let db = lin_to_db(rms_linear(samples));
    let delta = (db - expected).abs();
    assert!(
        delta <= tolerance,
        "RMS {db:.2} dBFS is {delta:.2} dB from expected {expected:.2} dB \
         (tolerance {tolerance:.2} dB) over {} samples",
        samples.len()
    );
}

/// Assert the buffer is silent: its peak sits below `threshold_db` dBFS.
///
/// Panics naming the loudest sample's index and level.
pub fn assert_silent(samples: &[f32], threshold_db: f32) {
    let (idx, mag) = peak(samples);
    let db = lin_to_db(mag);
    assert!(
        db < threshold_db,
        "expected silence below {threshold_db:.1} dBFS, but sample {idx} \
         peaks at {db:.2} dBFS ({mag:.4} linear)"
    );
}

/// Largest absolute step between adjacent samples that is still considered
/// "smooth". A jump beyond this reads as a click/discontinuity. Chosen
/// well above the per-sample slope of a full-scale 20 kHz tone at 44.1 kHz
/// (~0.9) would be too loose, so this is tuned for the synthesized
/// fixtures and crossfade ramps the suite actually exercises.
pub const CLICK_STEP_THRESHOLD: f32 = 0.5;

/// Assert there are no clicks: no adjacent-sample jump exceeds
/// [`CLICK_STEP_THRESHOLD`]. Panics naming the first offending index pair
/// and the size of the jump.
pub fn assert_no_clicks(samples: &[f32]) {
    for (i, w) in samples.windows(2).enumerate() {
        let step = (w[1] - w[0]).abs();
        assert!(
            step <= CLICK_STEP_THRESHOLD,
            "click detected between samples {i} and {} (step {step:.4} > {CLICK_STEP_THRESHOLD})",
            i + 1
        );
    }
}

/// Directory holding committed golden buffers.
fn golden_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden")
}

/// Compare `samples` against a committed golden snapshot named `name`.
///
/// The snapshot is stored as raw little-endian `f32` at
/// `tests/golden/<name>.f32`. On the first run (or with `UPDATE_GOLDEN=1`)
/// the snapshot is written and the check passes; afterward the buffer must
/// match within a small epsilon. A mismatch panics naming the sample range
/// that diverged. This is how DSP output is pinned: refresh deliberately,
/// never silently.
pub fn golden_snapshot(name: &str, samples: &[f32]) {
    let dir = golden_dir();
    let path = dir.join(format!("{name}.f32"));
    let update = std::env::var("UPDATE_GOLDEN").is_ok();

    if update || !path.exists() {
        fs::create_dir_all(&dir).expect("create golden dir");
        let mut bytes = Vec::with_capacity(samples.len() * 4);
        for &s in samples {
            bytes.extend_from_slice(&s.to_le_bytes());
        }
        fs::write(&path, &bytes).unwrap_or_else(|e| panic!("write golden {name}: {e}"));
        eprintln!(
            "golden_snapshot: wrote {} ({} samples){}",
            path.display(),
            samples.len(),
            if update { " [UPDATE_GOLDEN]" } else { " [new]" }
        );
        return;
    }

    let raw = fs::read(&path).unwrap_or_else(|e| panic!("read golden {name}: {e}"));
    assert_eq!(
        raw.len() % 4,
        0,
        "golden {name} is not a whole number of f32 values"
    );
    let expected: Vec<f32> = raw
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect();

    assert_eq!(
        samples.len(),
        expected.len(),
        "golden {name}: length {} != snapshot {} \
         (refresh with UPDATE_GOLDEN=1 if this is intended)",
        samples.len(),
        expected.len()
    );

    const EPS: f32 = 1e-6;
    let mut first = None;
    let mut last = 0usize;
    let mut count = 0usize;
    for (i, (&got, &want)) in samples.iter().zip(expected.iter()).enumerate() {
        if (got - want).abs() > EPS {
            if first.is_none() {
                first = Some(i);
            }
            last = i;
            count += 1;
        }
    }
    if let Some(start) = first {
        let g = samples[start];
        let w = expected[start];
        panic!(
            "golden {name} differs in {count} sample(s), range [{start}..={last}]; \
             first at {start}: got {g:.6}, want {w:.6} \
             (refresh with UPDATE_GOLDEN=1 if this is intended)"
        );
    }
}
