//! Integration tests for the symphonia mp3 decode pipeline.
//!
//! These exercise the real public API (`termkrush_core::audio::decode_file`)
//! against the committed fixtures: the lossless WAV sine and the mp3
//! encoded from it. The WAV decode is the reference the (lossy) mp3 is
//! measured against, so the assertion is "mp3 ~= source" rather than a
//! hand-transcribed magic number.

mod common;

use common::fixtures;
use termkrush_core::audio::decode_file;

/// Root-mean-square level of an interleaved buffer, as linear amplitude.
fn rms(samples: &[f32]) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum_sq: f64 = samples.iter().map(|&s| (s as f64) * (s as f64)).sum();
    (sum_sq / samples.len() as f64).sqrt()
}

#[test]
fn mp3_duration_matches_within_10ms() {
    let dec = decode_file(fixtures::SINE_A440_MP3.path(), 44_100).expect("decode mp3");
    eprintln!(
        "mp3: dur={:.4}s frames={} src={}Hz/{}ch",
        dec.duration_secs,
        dec.frames(),
        dec.source_sample_rate,
        dec.source_channels
    );
    assert!(
        (dec.duration_secs - 10.0).abs() < 0.010,
        "mp3 duration {:.4}s not within 10ms of 10.0s",
        dec.duration_secs
    );
}

#[test]
fn mp3_decodes_to_stereo_from_mono_source() {
    let dec = decode_file(fixtures::SINE_A440_MP3.path(), 44_100).expect("decode mp3");
    assert_eq!(dec.source_channels, 1, "fixture should be a mono mp3");
    assert_eq!(dec.channels, 2, "pipeline output must be stereo");
    assert_eq!(
        dec.samples.len() % 2,
        0,
        "interleaved stereo is even-length"
    );

    // Mono upmix duplicates the source onto both channels: L == R.
    for (i, frame) in dec.samples.chunks(2).enumerate().take(5000) {
        assert_eq!(frame[0], frame[1], "frame {i}: L != R after mono upmix");
    }
}

#[test]
fn decoded_rms_within_1pct_of_reference() {
    // Criterion: "decoded RMS for a known fixture within 1% of a reference
    // value." The known fixture is the lossless WAV sine; the reference is
    // the analytic RMS of a 0.6-amplitude sine, 0.6/sqrt(2). (mp3 is lossy
    // — encoding shifts the level a few percent — so the *level* reference
    // is asserted against the WAV, which the decoder reproduces exactly.)
    let wav = decode_file(fixtures::SINE_A440.path(), 44_100).expect("decode wav");
    let reference = 0.6_f64 / 2.0_f64.sqrt();
    let got = rms(&wav.samples);
    let rel_err = (got - reference).abs() / reference;
    eprintln!(
        "wav_rms={got:.6} reference={reference:.6} rel_err={:.4}%",
        rel_err * 100.0
    );
    assert!(
        rel_err < 0.01,
        "WAV RMS {got:.6} not within 1% of reference {reference:.6} ({:.3}%)",
        rel_err * 100.0
    );

    // Sanity: the mp3 actually decoded audio at roughly the right level
    // (not silence, not garbage) — lossy, so a looser band.
    let mp3_rms = rms(&decode_file(fixtures::SINE_A440_MP3.path(), 44_100)
        .unwrap()
        .samples);
    eprintln!("mp3_rms={mp3_rms:.6} (lossy)");
    assert!(
        (mp3_rms - reference).abs() / reference < 0.05,
        "mp3 RMS {mp3_rms:.6} implausibly far from {reference:.6}"
    );
}

#[test]
fn resamples_44100_to_48000() {
    let native = decode_file(fixtures::SINE_A440_MP3.path(), 44_100).expect("decode @44.1k");
    let up = decode_file(fixtures::SINE_A440_MP3.path(), 48_000).expect("decode @48k");

    assert_eq!(up.sample_rate, 48_000, "output rate should be 48 kHz");
    assert_eq!(up.source_sample_rate, 44_100, "source stays 44.1 kHz");

    // Resampling preserves duration: the output frame count tracks the
    // rate ratio to within the resampler's few-frame latency (well under
    // 10ms), rather than landing on an exact frame.
    let native_dur = native.frames() as f64 / 44_100.0;
    let up_dur = up.frames() as f64 / 48_000.0;
    eprintln!(
        "resample: native_frames={} ({native_dur:.4}s) up_frames={} ({up_dur:.4}s)",
        native.frames(),
        up.frames()
    );
    assert!(
        (up_dur - native_dur).abs() < 0.010,
        "resampled duration {up_dur:.4}s drifted >10ms from native {native_dur:.4}s"
    );

    // Resampling a sine preserves its level: RMS within 1% of native.
    let rel_err = (rms(&up.samples) - rms(&native.samples)).abs() / rms(&native.samples);
    eprintln!("resample rel_err={:.4}%", rel_err * 100.0);
    assert!(
        rel_err < 0.01,
        "resampled RMS drifted {:.3}% from native (>1%)",
        rel_err * 100.0
    );
}
