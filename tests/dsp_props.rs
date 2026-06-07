//! Property tests for the master bus + sampler pads.
//!
//! `proptest` hunts for counterexamples and shrinks them. They run against
//! the real public API — `Mixer` — with no audio device. The invariants:
//! whatever pads play and whatever the master gain, the output is always
//! finite; and silent pads can never conjure signal.

use proptest::prelude::*;

use termkrush::mix::Mixer;

/// A short interleaved-stereo clip of arbitrary samples.
fn clip() -> impl Strategy<Value = Vec<f32>> {
    prop::collection::vec(-1.0f32..=1.0, 2..512)
}

proptest! {
    /// Invariant: the mixed output is finite for any pad content and any
    /// master gain. (Catches NaN/inf from the voice/gain path.)
    #[test]
    fn mixer_output_is_always_finite(
        a in clip(),
        b in clip(),
        master in 0.0f32..=1.5,
        frames in 1usize..256,
    ) {
        let mut m = Mixer::new();
        m.set_master(master);
        m.assign_pad(0, a);
        m.assign_pad(1, b);
        m.trigger_pad(0);
        m.trigger_pad(1);
        let mut out = vec![0.0f32; frames * 2];
        m.fill_mix(&mut out);
        prop_assert!(out.iter().all(|s| s.is_finite()), "non-finite sample in the mix");
    }

    /// Invariant: silence in, silence out — no master gain can make signal
    /// from a silent pad.
    #[test]
    fn silence_in_silence_out(master in 0.0f32..=1.5, frames in 1usize..256) {
        let mut m = Mixer::new();
        m.set_master(master);
        m.assign_pad(0, vec![0.0; 128]);
        m.trigger_pad(0);
        let mut out = vec![0.0f32; frames * 2];
        m.fill_mix(&mut out);
        prop_assert!(out.iter().all(|&s| s == 0.0), "silence produced signal");
    }
}
