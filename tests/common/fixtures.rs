//! Typed handles to the synthesized test-audio fixtures.
//!
//! Tests refer to a fixture by name — `fixtures::HOUSE_128.path()` — rather
//! than hard-coding a relative path, so moving or renaming a file is a
//! one-line change here. Each handle mirrors a `[[fixture]]` entry in
//! `tests/fixtures/manifest.toml`; the generator that produces the WAVs is
//! `scripts/gen-fixtures.sh`.

use std::path::{Path, PathBuf};

/// One audio fixture and the facts a test needs to assert against.
pub struct Fixture {
    /// Manifest name (stable identifier).
    pub name: &'static str,
    /// File name within `tests/fixtures/`.
    pub file: &'static str,
    /// Exact tempo for the rhythmic fixtures; `None` for tones/noise.
    pub bpm: Option<f32>,
    /// Sample rate in Hz.
    pub sample_rate: u32,
    /// Nominal duration in seconds.
    pub duration_secs: f32,
}

impl Fixture {
    /// Absolute path to the fixture file, resolved from the crate root so
    /// it works regardless of the test's working directory.
    pub fn path(&self) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join(self.file)
    }
}

/// Pure 440 Hz sine (A4). No tempo; useful for tone/decoder sanity checks.
pub const SINE_A440: Fixture = Fixture {
    name: "sine_a440",
    file: "sine_a440_10s.wav",
    bpm: None,
    sample_rate: 44100,
    duration_secs: 10.0,
};

/// Logarithmic 20 Hz -> 20 kHz sweep; exercises the full spectrum.
pub const SWEEP_20_20K: Fixture = Fixture {
    name: "sweep_20_20k",
    file: "sweep_20_20k_10s.wav",
    bpm: None,
    sample_rate: 44100,
    duration_secs: 10.0,
};

/// Metronome click at exactly 120 BPM. Ground truth for BPM detection.
pub const CLICK_120: Fixture = Fixture {
    name: "click_120bpm",
    file: "click_120bpm_12s.wav",
    bpm: Some(120.0),
    sample_rate: 44100,
    duration_secs: 12.0,
};

/// Metronome click at exactly 128 BPM (house tempo). Ground truth for BPM
/// detection and sync tests.
pub const HOUSE_128: Fixture = Fixture {
    name: "house_128",
    file: "click_128bpm_10s.wav",
    bpm: Some(128.0),
    sample_rate: 44100,
    duration_secs: 10.0,
};

/// Seeded white noise. Deterministic; useful for level/RMS assertions.
pub const NOISE_WHITE: Fixture = Fixture {
    name: "noise_white",
    file: "noise_white_5s.wav",
    bpm: None,
    sample_rate: 44100,
    duration_secs: 5.0,
};

/// The same synthesized 440 Hz sine as [`SINE_A440`], encoded to mp3 by
/// `lame` (CBR 192 kbps, mono). The decode-pipeline story asserts against
/// this for real mp3 frames; it is intentionally NOT in [`ALL`], which is
/// the WAV-header presence sweep.
pub const SINE_A440_MP3: Fixture = Fixture {
    name: "sine_a440_mp3",
    file: "sine_a440_10s.mp3",
    bpm: None,
    sample_rate: 44100,
    duration_secs: 10.0,
};

/// Every WAV fixture, for suites that want to sweep the whole set. The mp3
/// fixture is excluded on purpose — the sweep parses WAV headers.
pub const ALL: &[&Fixture] = &[
    &SINE_A440,
    &SWEEP_20_20K,
    &CLICK_120,
    &HOUSE_128,
    &NOISE_WHITE,
];
