//! Integration test for the synthesized audio fixtures.
//!
//! It proves the fixture set is present and well-formed without any
//! network access and without a decoder dependency (the decode story adds
//! symphonia): it parses the WAV headers directly and checks each file's
//! sample rate, channel count, and duration against the typed handle in
//! `tests/common/fixtures.rs`, which in turn mirrors the manifest.

mod common;

use common::fixtures::{self, Fixture};
use std::fs;

/// Minimal WAV header facts we assert on.
struct WavInfo {
    sample_rate: u32,
    channels: u16,
    bits_per_sample: u16,
    duration_secs: f32,
}

fn u16le(b: &[u8]) -> u16 {
    u16::from_le_bytes([b[0], b[1]])
}
fn u32le(b: &[u8]) -> u32 {
    u32::from_le_bytes([b[0], b[1], b[2], b[3]])
}

/// Parse just enough of a canonical PCM WAV to recover format + duration.
/// Walks the chunk list rather than assuming fixed offsets, so a slightly
/// different writer (extra chunks) would still parse.
fn parse_wav(bytes: &[u8]) -> WavInfo {
    assert!(bytes.len() >= 12, "file too short to be a WAV");
    assert_eq!(&bytes[0..4], b"RIFF", "missing RIFF magic");
    assert_eq!(&bytes[8..12], b"WAVE", "missing WAVE magic");

    let mut pos = 12;
    let mut sample_rate = 0u32;
    let mut channels = 0u16;
    let mut bits = 0u16;
    let mut data_size = 0u32;
    let mut saw_fmt = false;
    let mut saw_data = false;

    while pos + 8 <= bytes.len() {
        let id = &bytes[pos..pos + 4];
        let size = u32le(&bytes[pos + 4..pos + 8]) as usize;
        let body = pos + 8;
        match id {
            b"fmt " => {
                channels = u16le(&bytes[body + 2..body + 4]);
                sample_rate = u32le(&bytes[body + 4..body + 8]);
                bits = u16le(&bytes[body + 14..body + 16]);
                saw_fmt = true;
            }
            b"data" => {
                data_size = size as u32;
                saw_data = true;
            }
            _ => {}
        }
        // Chunks are word-aligned: an odd size carries a pad byte.
        pos = body + size + (size & 1);
    }

    assert!(saw_fmt, "no fmt chunk");
    assert!(saw_data, "no data chunk");
    let bytes_per_frame = channels as u32 * (bits as u32 / 8);
    let frames = data_size as f32 / bytes_per_frame as f32;
    WavInfo {
        sample_rate,
        channels,
        bits_per_sample: bits,
        duration_secs: frames / sample_rate as f32,
    }
}

fn check(fx: &Fixture) {
    let path = fx.path();
    let bytes = fs::read(&path)
        .unwrap_or_else(|e| panic!("fixture {} missing at {}: {e}", fx.name, path.display()));
    let info = parse_wav(&bytes);

    assert_eq!(
        info.sample_rate, fx.sample_rate,
        "{}: sample rate {} != expected {}",
        fx.name, info.sample_rate, fx.sample_rate
    );
    assert_eq!(info.channels, 1, "{}: expected mono", fx.name);
    assert_eq!(info.bits_per_sample, 16, "{}: expected 16-bit", fx.name);
    assert!(
        (info.duration_secs - fx.duration_secs).abs() < 0.05,
        "{}: duration {:.3}s != expected {:.3}s",
        fx.name,
        info.duration_secs,
        fx.duration_secs
    );
}

#[test]
fn all_fixtures_present_and_well_formed() {
    assert!(!fixtures::ALL.is_empty(), "no fixtures registered");
    for fx in fixtures::ALL {
        check(fx);
    }
}

#[test]
fn click_tracks_carry_known_bpm() {
    // The rhythmic fixtures are the ones with ground-truth tempo; the
    // tones and noise deliberately have none.
    assert_eq!(fixtures::HOUSE_128.bpm, Some(128.0));
    assert_eq!(fixtures::CLICK_120.bpm, Some(120.0));
    assert_eq!(fixtures::SINE_A440.bpm, None);
    assert_eq!(fixtures::NOISE_WHITE.bpm, None);
}
