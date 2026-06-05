//! A deck: one loaded track and its transport state.
//!
//! A deck owns a decoded track (interleaved stereo from
//! [`audio::decode_file`](crate::audio::decode_file)), a playhead, and a
//! transport state. It is **pull-based**: a consumer (the audio pump, and
//! later the mixer) calls [`Deck::fill`] to draw the next block of
//! samples. The deck advances its playhead only while playing and writes
//! silence otherwise, so play/pause/stop are just state changes — the
//! pull side never has to special-case them.
//!
//! Real-time speed falls out of this design: the output device consumes
//! one frame per output sample, and `fill` advances exactly one playhead
//! frame per frame produced, so a 10-second track takes ten seconds.

use crate::audio::DecodedAudio;

/// Transport state of a deck.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeckState {
    /// No track loaded.
    Empty,
    /// Track loaded, playhead at 0, not yet started.
    Loaded,
    /// Advancing the playhead and producing audio.
    Playing,
    /// Holding position; produces silence until resumed.
    Paused,
    /// Halted with the playhead rewound to 0.
    Stopped,
}

/// One loaded track plus its transport. See the module docs for the
/// pull-based contract.
#[derive(Debug)]
pub struct Deck {
    track: Option<DecodedAudio>,
    /// Playhead in stereo frames into `track.samples`.
    pos: usize,
    state: DeckState,
    /// Linear output gain applied in [`fill`](Self::fill).
    gain: f32,
}

impl Default for Deck {
    fn default() -> Self {
        Self::new()
    }
}

impl Deck {
    /// An empty deck with unity gain.
    pub fn new() -> Self {
        Deck {
            track: None,
            pos: 0,
            state: DeckState::Empty,
            gain: 1.0,
        }
    }

    /// Load a decoded track, rewinding to the start and entering
    /// [`DeckState::Loaded`] (ready, not playing). Replaces any prior track.
    pub fn load(&mut self, track: DecodedAudio) {
        self.track = Some(track);
        self.pos = 0;
        self.state = DeckState::Loaded;
    }

    /// Start (or resume) playback. No-op with no track. From `Loaded` or
    /// `Stopped` the playhead is already at 0, so play begins at the start;
    /// from `Paused` it resumes where it left off.
    pub fn play(&mut self) {
        if self.track.is_some() && self.state != DeckState::Playing {
            self.state = DeckState::Playing;
        }
    }

    /// Pause playback, holding the playhead. No-op unless playing.
    pub fn pause(&mut self) {
        if self.state == DeckState::Playing {
            self.state = DeckState::Paused;
        }
    }

    /// Spacebar semantics: pause if playing, otherwise play.
    pub fn toggle(&mut self) {
        match self.state {
            DeckState::Playing => self.pause(),
            DeckState::Loaded | DeckState::Paused | DeckState::Stopped => self.play(),
            DeckState::Empty => {}
        }
    }

    /// Stop and rewind to the start. No-op with no track.
    pub fn stop(&mut self) {
        if self.track.is_some() {
            self.state = DeckState::Stopped;
            self.pos = 0;
        }
    }

    /// Fill `out` (interleaved stereo, even length) with the next frames,
    /// advancing the playhead while playing. Returns the number of stereo
    /// frames actually drawn from the track — `0` when empty, loaded,
    /// paused, or stopped. Any part of `out` not covered by track audio is
    /// written as silence, so the caller can always hand `out` straight to
    /// the output.
    ///
    /// Reaching the end of the track stops the deck and rewinds to 0, so a
    /// subsequent [`play`](Self::play) starts over.
    pub fn fill(&mut self, out: &mut [f32]) -> usize {
        let frames_out = out.len() / 2;

        let track = match (self.state, &self.track) {
            (DeckState::Playing, Some(t)) => t,
            _ => {
                // Not producing: silence the whole buffer.
                out.iter_mut().for_each(|s| *s = 0.0);
                return 0;
            }
        };

        let total = track.frames();
        let avail = total.saturating_sub(self.pos);
        let n = frames_out.min(avail);

        for i in 0..n {
            let src = (self.pos + i) * 2;
            out[i * 2] = track.samples[src] * self.gain;
            out[i * 2 + 1] = track.samples[src + 1] * self.gain;
        }
        // Silence the remainder (end-of-track underrun, or an over-long buffer).
        out[n * 2..].iter_mut().for_each(|s| *s = 0.0);

        self.pos += n;
        if self.pos >= total {
            // Track finished: halt and rewind so the next play restarts it.
            self.state = DeckState::Stopped;
            self.pos = 0;
        }
        n
    }

    /// Current transport state.
    pub fn state(&self) -> DeckState {
        self.state
    }

    /// `true` while actively playing.
    pub fn is_playing(&self) -> bool {
        self.state == DeckState::Playing
    }

    /// Playhead position in stereo frames.
    pub fn position_frames(&self) -> usize {
        self.pos
    }

    /// Playhead position in seconds (0 with no track).
    pub fn position_secs(&self) -> f64 {
        match &self.track {
            Some(t) if t.sample_rate > 0 => self.pos as f64 / t.sample_rate as f64,
            _ => 0.0,
        }
    }

    /// Loaded track duration in seconds (0 with no track).
    pub fn duration_secs(&self) -> f64 {
        self.track.as_ref().map(|t| t.duration_secs).unwrap_or(0.0)
    }

    /// Title of the loaded track, if any.
    pub fn title(&self) -> Option<&str> {
        self.track.as_ref().and_then(|t| t.title.as_deref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A synthetic decoded track: `frames` stereo frames where every
    /// sample is `level`, so audio-vs-silence is trivial to check.
    fn track(frames: usize, level: f32, rate: u32) -> DecodedAudio {
        DecodedAudio {
            samples: vec![level; frames * 2],
            sample_rate: rate,
            channels: 2,
            source_sample_rate: rate,
            source_channels: 2,
            duration_secs: frames as f64 / rate as f64,
            title: None,
            artist: None,
        }
    }

    fn is_silent(buf: &[f32]) -> bool {
        buf.iter().all(|&s| s == 0.0)
    }

    #[test]
    fn empty_deck_is_silent_and_ignores_transport() {
        let mut d = Deck::new();
        assert_eq!(d.state(), DeckState::Empty);
        d.play();
        d.toggle();
        d.stop();
        assert_eq!(d.state(), DeckState::Empty, "no track => no transport");
        let mut buf = [9.0f32; 8];
        assert_eq!(d.fill(&mut buf), 0);
        assert!(is_silent(&buf), "empty deck fills silence");
    }

    #[test]
    fn spacebar_toggles_play_pause_and_audio_matches_state() {
        let mut d = Deck::new();
        d.load(track(1000, 0.5, 44_100));
        assert_eq!(d.state(), DeckState::Loaded);

        // Loaded => silence until played.
        let mut buf = [0.0f32; 8];
        assert_eq!(d.fill(&mut buf), 0);
        assert!(is_silent(&buf));

        // Space => play => audio.
        d.toggle();
        assert_eq!(d.state(), DeckState::Playing);
        let drawn = d.fill(&mut buf);
        assert_eq!(drawn, 4, "4 stereo frames into an 8-sample buffer");
        assert!(buf.iter().all(|&s| s == 0.5), "playing => track audio");

        // Space again => pause => silence, playhead held.
        d.toggle();
        assert_eq!(d.state(), DeckState::Paused);
        let held = d.position_frames();
        let mut buf2 = [1.0f32; 8];
        assert_eq!(d.fill(&mut buf2), 0);
        assert!(is_silent(&buf2), "paused => silence");
        assert_eq!(d.position_frames(), held, "paused playhead does not move");
    }

    #[test]
    fn stop_rewinds_and_silences() {
        let mut d = Deck::new();
        d.load(track(1000, 0.5, 44_100));
        d.play();
        let mut buf = [0.0f32; 200];
        d.fill(&mut buf);
        assert!(d.position_frames() > 0);

        d.stop();
        assert_eq!(d.state(), DeckState::Stopped);
        assert_eq!(d.position_frames(), 0, "stop rewinds to 0");

        let mut buf2 = [7.0f32; 8];
        assert_eq!(d.fill(&mut buf2), 0);
        assert!(is_silent(&buf2), "stopped => silence");
    }

    #[test]
    fn play_after_stop_starts_from_beginning() {
        let mut d = Deck::new();
        d.load(track(1000, 0.5, 44_100));
        d.play();
        let mut buf = [0.0f32; 400];
        d.fill(&mut buf); // advance 200 frames
        d.stop();
        d.play();
        assert_eq!(d.position_frames(), 0, "replay starts at 0");
        let mut buf2 = [0.0f32; 8];
        d.fill(&mut buf2);
        assert_eq!(d.position_frames(), 4, "advances from the start again");
    }

    #[test]
    fn playhead_advances_one_frame_per_output_frame() {
        let rate = 44_100;
        let mut d = Deck::new();
        d.load(track(rate as usize, 0.3, rate)); // 1 second
        d.play();

        // Pull the whole second in 1000-frame blocks; count what we drew.
        let mut buf = vec![0.0f32; 2000]; // 1000 stereo frames
        let mut total = 0usize;
        while d.is_playing() {
            total += d.fill(&mut buf);
        }
        assert_eq!(total, rate as usize, "drew exactly 1s of frames");
        // End-of-track rewound and stopped.
        assert_eq!(d.state(), DeckState::Stopped);
        assert_eq!(d.position_frames(), 0);
    }

    #[test]
    fn gain_default_is_unity() {
        let mut d = Deck::new();
        d.load(track(10, 1.0, 44_100));
        d.play();
        let mut buf = [0.0f32; 4];
        d.fill(&mut buf);
        assert!(buf.iter().all(|&s| s == 1.0), "unity gain passes through");
    }
}
