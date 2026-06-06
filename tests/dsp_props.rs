//! Property-based tests for the DSP path.
//!
//! Example tests (elsewhere) pin specific known cases; these assert
//! *invariants* that must hold for whole families of inputs, and let
//! `proptest` hunt for counterexamples and shrink them. They run against
//! the real public API — `Deck` and `Mixer` — with no audio device.

use proptest::prelude::*;
use termkrush::audio::DecodedAudio;
use termkrush::deck::Deck;
use termkrush::mix::Mixer;

/// Wrap interleaved-stereo samples as a decoded track (length rounded down
/// to a whole number of stereo frames).
fn track(mut samples: Vec<f32>) -> DecodedAudio {
    if samples.len() % 2 == 1 {
        samples.pop();
    }
    let frames = samples.len() / 2;
    DecodedAudio {
        samples,
        sample_rate: 44_100,
        channels: 2,
        source_sample_rate: 44_100,
        source_channels: 2,
        duration_secs: frames as f64 / 44_100.0,
        title: None,
        artist: None,
    }
}

/// A buffer of interleaved-stereo samples in a sane audio range.
fn samples() -> impl Strategy<Value = Vec<f32>> {
    prop::collection::vec(-2.0f32..2.0, 0..1024)
}

proptest! {
    /// Invariant: the mixer never emits NaN or infinity, for any finite
    /// deck audio and any fader/gain settings. (Catches divide-by-zero,
    /// uninitialised reads, and pathological ramp math.)
    #[test]
    fn mixer_output_is_always_finite(
        a in samples(),
        b in samples(),
        xfade in -1.0f32..=1.0,
        master in 0.0f32..=1.5,
        ga in 0.0f32..=1.5,
        gb in 0.0f32..=1.5,
    ) {
        let mut m = Mixer::new();
        m.cut_to(xfade);
        m.set_master(master);
        m.deck_mut(0).set_gain(ga);
        m.deck_mut(1).set_gain(gb);
        m.deck_mut(0).load(track(a));
        m.deck_mut(1).load(track(b));
        m.deck_mut(0).play();
        m.deck_mut(1).play();

        let mut out = vec![0.0f32; 2048];
        m.fill_mix(&mut out);
        prop_assert!(out.iter().all(|s| s.is_finite()), "non-finite sample in mix output");
    }

    /// Invariant: silence in, silence out — no setting of the crossfader or
    /// gains can conjure signal from two silent decks.
    #[test]
    fn silence_in_silence_out(
        frames in 1usize..2000,
        xfade in -1.0f32..=1.0,
        master in 0.0f32..=1.5,
    ) {
        let mut m = Mixer::new();
        m.cut_to(xfade);
        m.set_master(master);
        m.deck_mut(0).load(track(vec![0.0; frames * 2]));
        m.deck_mut(1).load(track(vec![0.0; frames * 2]));
        m.deck_mut(0).play();
        m.deck_mut(1).play();

        let mut out = vec![9.9f32; frames * 2]; // pre-fill with non-zero
        m.fill_mix(&mut out);
        prop_assert!(out.iter().all(|&s| s == 0.0), "silence produced non-zero output");
    }

    /// Invariant: a deck at unity gain reproduces its input exactly. Unity
    /// is the smoothed gain's resting value, so there is no ramp to settle
    /// — the pull is a faithful copy. (The "gain is linear" property at g=1.)
    #[test]
    fn unity_gain_deck_reproduces_input(s in samples()) {
        let t = track(s);
        let expected = t.samples.clone();
        let mut d = Deck::new();
        d.load(t);
        d.play();

        let mut out = vec![0.0f32; expected.len()];
        d.fill(&mut out);
        prop_assert_eq!(out, expected, "unity-gain deck altered its input");
    }
}
