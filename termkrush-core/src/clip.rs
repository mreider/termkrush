//! A clip: a captured region of audio with metadata, the unit the sampler
//! pads play. Sources are uniform — a recorded region, a track, or a resample of the live mix all become a `Clip`. Samples are
//! interleaved stereo at the mix rate (decoded/captured already at the
//! output rate), so a pad voice can play one back directly.
//!
//! Editing (timeline trim) and pad-type behaviour layer on top in later
//! stories; this owns the captured audio + its bounds + tempo.

/// A captured audio clip.
#[derive(Debug, Clone)]
pub struct Clip {
    /// Interleaved stereo samples at the mix rate.
    pub samples: Vec<f32>,
    /// Source tempo in BPM, if known (e.g. the source track.s detected BPM at
    /// capture time). Used for loop sync later.
    pub bpm: Option<f32>,
    /// A short label for the clip (e.g. the source + position).
    pub name: String,
}

impl Clip {
    /// A clip from captured samples.
    pub fn new(samples: Vec<f32>, bpm: Option<f32>, name: impl Into<String>) -> Self {
        Clip {
            samples,
            bpm,
            name: name.into(),
        }
    }

    /// Length in stereo frames.
    pub fn frames(&self) -> usize {
        self.samples.len() / 2
    }
}
