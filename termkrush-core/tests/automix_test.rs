//! Integration: the auto-mix render path on a real decoded fixture.
//!
//! The synthetic unit tests in `src/automix.rs` pin the planner's math;
//! this test runs the whole path a user hits — decode a WAV from disk,
//! feed marks, plan, render — and asserts on output frames and bytes.

mod common;

use std::sync::Arc;

use common::fixtures;
use termkrush_core::audio::decode_file;
use termkrush_core::automix::{plan, render, TrackInput, RENDER_RATE};

/// Decode the 120 BPM click fixture and mark its beats the way a perfect
/// tapper would: every 22050 frames from the first click.
fn click_track() -> TrackInput {
    let fx = fixtures::CLICK_120;
    let audio = decode_file(fx.path(), RENDER_RATE).expect("decode fixture");
    assert_eq!(audio.sample_rate, RENDER_RATE);
    let frames = (audio.samples.len() / 2) as u64;
    let fpb = 22_050u64; // 120 BPM at 44100
    let beats: Vec<u64> = (0..frames / fpb).map(|k| k * fpb).collect();
    TrackInput {
        id: fx.path().to_string_lossy().into_owned(),
        samples: Arc::new(audio.samples),
        beats,
    }
}

#[test]
fn renders_a_decoded_fixture_deterministically() {
    let tracks = vec![click_track()];
    let order = [0usize];

    let p = plan(&tracks, &order).expect("plan");
    assert!((p.master_bpm - 120.0).abs() < 0.01, "bpm {}", p.master_bpm);

    // The 12 s fixture holds 6 whole bars — the section clamps to them.
    assert_eq!(p.sections.len(), 1);
    assert!(p.sections[0].bars <= 6);

    let mix = render(&p, &tracks);
    assert_eq!(mix.len() as u64, p.total_frames() * 2);
    assert!(p.total_frames() > 0);

    // The mix carries signal (the clicks made it through varispeed+gain).
    let peak = mix.iter().fold(0.0f32, |m, &s| m.max(s.abs()));
    assert!(peak > 0.05, "peak {peak}");

    // Same input, same bytes — end to end through the decoder.
    let p2 = plan(&tracks, &order).expect("plan again");
    assert_eq!(p, p2);
    assert_eq!(render(&p2, &tracks), mix);
}
