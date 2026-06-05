//! Integration test for the Deck transport, driving the real 10-second
//! fixture through the decode pipeline and the deck's pull-based output.
//! The deck-level state machine is unit-tested in `src/deck/mod.rs`; this
//! proves the two pieces compose on actual decoded audio.

mod common;

use common::fixtures;
use termkrush::audio::decode_file;
use termkrush::deck::{Deck, DeckState};

/// Decode the 10s sine WAV at native rate and load it into a fresh deck.
fn loaded_deck() -> (Deck, usize) {
    let track = decode_file(fixtures::SINE_A440.path(), 44_100).expect("decode fixture");
    let frames = track.frames();
    let mut deck = Deck::new();
    deck.load(track);
    (deck, frames)
}

/// Play to the end in fixed blocks, returning the total stereo frames drawn.
fn play_to_end(deck: &mut Deck) -> usize {
    let mut buf = vec![0.0f32; 4096]; // 2048 stereo frames
    let mut total = 0;
    // Bound the loop so a transport bug can't hang the suite.
    for _ in 0..1_000_000 {
        if !deck.is_playing() {
            break;
        }
        total += deck.fill(&mut buf);
    }
    total
}

#[test]
fn ten_second_fixture_plays_for_ten_seconds() {
    let (mut deck, frames) = loaded_deck();
    assert_eq!(deck.state(), DeckState::Loaded);

    // Reference: the fixture is 10s at 44.1 kHz.
    assert_eq!(frames, 441_000, "fixture should be exactly 10s of frames");

    deck.play();
    let drawn = play_to_end(&mut deck);

    assert_eq!(drawn, frames, "deck plays every frame exactly once");
    let played_secs = drawn as f64 / 44_100.0;
    assert!(
        (played_secs - 10.0).abs() < 0.010,
        "played {played_secs:.4}s, expected ~10s"
    );
    // End-of-track auto-stops and rewinds.
    assert_eq!(deck.state(), DeckState::Stopped);
    assert_eq!(deck.position_frames(), 0);
}

#[test]
fn pause_holds_then_resumes_in_place() {
    let (mut deck, _) = loaded_deck();
    deck.play();
    let mut buf = vec![0.0f32; 4096];
    deck.fill(&mut buf);
    let at = deck.position_frames();
    assert!(at > 0);

    deck.pause();
    assert_eq!(deck.state(), DeckState::Paused);
    assert_eq!(deck.fill(&mut buf), 0, "paused deck draws nothing");
    assert_eq!(deck.position_frames(), at, "paused playhead is frozen");

    deck.play();
    deck.fill(&mut buf);
    assert!(deck.position_frames() > at, "resumes from where it paused");
}

#[test]
fn replay_after_full_play_starts_from_zero() {
    let (mut deck, frames) = loaded_deck();
    deck.play();
    assert_eq!(play_to_end(&mut deck), frames);
    assert_eq!(deck.position_frames(), 0, "rewound after finishing");

    // Press play again: it should start over, not sit at the end.
    deck.play();
    assert_eq!(deck.state(), DeckState::Playing);
    let mut buf = vec![0.0f32; 4096];
    let drawn = deck.fill(&mut buf);
    assert_eq!(drawn, 2048, "replay draws a full block from the start");
    assert_eq!(deck.position_frames(), 2048);
}

#[test]
fn position_seconds_tracks_the_playhead() {
    let (mut deck, _) = loaded_deck();
    deck.play();
    // Draw exactly 1 second of frames.
    let mut buf = vec![0.0f32; 44_100 * 2];
    deck.fill(&mut buf);
    assert!(
        (deck.position_secs() - 1.0).abs() < 1e-6,
        "1s of frames => position 1.0s, got {}",
        deck.position_secs()
    );
}
