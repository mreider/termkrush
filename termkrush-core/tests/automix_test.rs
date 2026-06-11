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

/// FNV-1a 64 over the rendered samples' little-endian bytes: a stable,
/// dependency-free digest of the mix.
fn digest(mix: &[f32]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for s in mix {
        for b in s.to_le_bytes() {
            h ^= b as u64;
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    h
}

/// Decode a click fixture with a STABLE id (not an absolute path — the
/// seed hashes ids, and golden hashes must not depend on checkout paths).
fn fixture_track(fx: &common::fixtures::Fixture, id: &str, fpb: u64) -> TrackInput {
    let audio = decode_file(fx.path(), RENDER_RATE).expect("decode fixture");
    let frames = (audio.samples.len() / 2) as u64;
    let beats: Vec<u64> = (0..frames / fpb).map(|k| k * fpb).collect();
    TrackInput {
        id: id.to_string(),
        samples: std::sync::Arc::new(audio.samples),
        beats,
    }
}

// The golden mix: same fixtures, same order, same marks → this digest,
// byte for byte, on every platform CI runs (the render path holds no
// libm transcendentals, no wall clock, no unordered iteration). When an
// engine story intentionally changes the grammar, this constant moves
// with it — that's the point: the mix changing is always a decision.
const GOLDEN: u64 = 0x2AC8_F4C9_CE09_7C89;

#[test]
fn golden_mix_is_bit_identical_everywhere() {
    let tracks = vec![
        fixture_track(&fixtures::CLICK_120, "click120", 22_050),
        // 128 BPM: 44100·60/128 ≈ 20672 frames per beat.
        fixture_track(&fixtures::HOUSE_128, "click128", 20_672),
    ];
    let order = [0usize, 1, 0];

    let p = plan(&tracks, &order).expect("plan");
    let mix = render(&p, &tracks);
    let d1 = digest(&mix);

    // Repeatable in-process…
    let mix2 = render(&plan(&tracks, &order).expect("plan"), &tracks);
    assert_eq!(d1, digest(&mix2), "same input, different bytes");

    // …and across platforms: CI's three-OS matrix runs this same line.
    assert_eq!(
        d1, GOLDEN,
        "the golden mix changed: got {d1:#018X}. If the grammar change \
         is intentional, update GOLDEN; if not, a nondeterminism crept in."
    );
}

#[test]
fn touching_the_input_changes_the_mix() {
    let tracks = vec![
        fixture_track(&fixtures::CLICK_120, "click120", 22_050),
        fixture_track(&fixtures::HOUSE_128, "click128", 20_672),
    ];
    let base = digest(&render(&plan(&tracks, &[0, 1, 0]).expect("plan"), &tracks));

    // Swap two entries: different mix.
    let swapped = digest(&render(&plan(&tracks, &[1, 0, 0]).expect("plan"), &tracks));
    assert_ne!(base, swapped, "order must reach the seed");

    // Nudge one beat mark: different mix.
    let mut nudged = vec![
        fixture_track(&fixtures::CLICK_120, "click120", 22_050),
        fixture_track(&fixtures::HOUSE_128, "click128", 20_672),
    ];
    nudged[0].beats[3] += 7;
    let nudge = digest(&render(&plan(&nudged, &[0, 1, 0]).expect("plan"), &nudged));
    assert_ne!(base, nudge, "marks must reach the seed");
}
