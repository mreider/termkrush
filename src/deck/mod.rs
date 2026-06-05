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
    /// Target linear output gain (what the dB readout shows). Set by
    /// [`set_gain`](Self::set_gain); `fill` ramps toward it.
    gain: f32,
    /// The gain actually applied right now; ramps toward `gain` one step
    /// per frame so level changes don't click (zipper noise).
    smoothed_gain: f32,
    /// Source display name (e.g. file name), used when the track carries
    /// no ID3 title. See [`display_name`](Self::display_name).
    name: Option<String>,
    /// Detected tempo in BPM, filled in asynchronously after load.
    bpm: Option<f32>,
}

/// Allowed gain range: silence up to +3.5 dB of headroom (1.5x linear).
pub const GAIN_MIN: f32 = 0.0;
pub const GAIN_MAX: f32 = 1.5;

/// Max change in applied gain per frame, so a jump de-zippers over a few
/// milliseconds instead of clicking. `1/512` traverses unity in ~12ms at
/// 44.1 kHz.
const GAIN_RAMP_STEP: f32 = 1.0 / 512.0;

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
            smoothed_gain: 1.0,
            name: None,
            bpm: None,
        }
    }

    /// Set the target gain, clamped to `[GAIN_MIN, GAIN_MAX]`. The applied
    /// gain ramps toward it in [`fill`](Self::fill) to avoid zipper noise;
    /// the value returned by [`gain`](Self::gain) updates immediately.
    pub fn set_gain(&mut self, gain: f32) {
        self.gain = gain.clamp(GAIN_MIN, GAIN_MAX);
    }

    /// Nudge the target gain by `delta` (clamped). Bound to `+`/`-` in the UI.
    pub fn nudge_gain(&mut self, delta: f32) {
        self.set_gain(self.gain + delta);
    }

    /// Load a decoded track, rewinding to the start and entering
    /// [`DeckState::Loaded`] (ready, not playing). Replaces any prior track.
    pub fn load(&mut self, track: DecodedAudio) {
        self.track = Some(track);
        self.pos = 0;
        self.state = DeckState::Loaded;
        self.name = None;
        self.bpm = None;
    }

    /// Record the detected tempo (BPM). Set asynchronously once background
    /// analysis finishes; ignored if no track is loaded.
    pub fn set_bpm(&mut self, bpm: f32) {
        if self.track.is_some() {
            self.bpm = Some(bpm);
        }
    }

    /// Detected tempo in BPM, if analysis has completed.
    pub fn bpm(&self) -> Option<f32> {
        self.bpm
    }

    /// Like [`load`](Self::load), but also records a display name (the
    /// source file name) for the panel to fall back to when the track has
    /// no ID3 title.
    pub fn load_named(&mut self, track: DecodedAudio, name: impl Into<String>) {
        self.load(track);
        self.name = Some(name.into());
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

    /// Move the playhead to an absolute position in seconds (clamped to
    /// `[0, end]`). Seeking at or past the end clamps to EOF and stops the
    /// deck. The transport state is otherwise preserved. No-op with no
    /// track.
    ///
    /// To avoid a click at the splice, the applied gain is reset so the
    /// audio fades back in from silence over the ramp (~12ms) — the
    /// pull-model equivalent of muting one buffer across the seek.
    pub fn seek(&mut self, secs: f64) {
        let Some(track) = &self.track else {
            return;
        };
        let total = track.frames();
        let rate = track.sample_rate.max(1) as f64;
        let frame = (secs.max(0.0) * rate).round() as usize;
        if frame >= total {
            self.pos = total; // clamp to EOF
            self.state = DeckState::Stopped;
        } else {
            self.pos = frame;
        }
        self.smoothed_gain = 0.0; // declick: ramp the level back up post-seek
    }

    /// Seek by a relative offset in seconds (negative seeks backward),
    /// clamped to the track bounds. Bound to the arrow / scrub keys.
    pub fn seek_by(&mut self, delta_secs: f64) {
        self.seek(self.position_secs() + delta_secs);
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

        let target = self.gain;
        let mut g = self.smoothed_gain;
        for i in 0..n {
            // Ramp the applied gain toward the target by at most one step.
            if g < target {
                g = (g + GAIN_RAMP_STEP).min(target);
            } else if g > target {
                g = (g - GAIN_RAMP_STEP).max(target);
            }
            let src = (self.pos + i) * 2;
            out[i * 2] = track.samples[src] * g;
            out[i * 2 + 1] = track.samples[src + 1] * g;
        }
        self.smoothed_gain = g;
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

    /// Title of the loaded track, if any (ID3/metadata only).
    pub fn title(&self) -> Option<&str> {
        self.track.as_ref().and_then(|t| t.title.as_deref())
    }

    /// What to show for the track: the ID3 title if present, else the
    /// source file name, else `None` when nothing is loaded.
    pub fn display_name(&self) -> Option<&str> {
        self.title().or(self.name.as_deref())
    }

    /// Current linear output gain (1.0 == unity).
    pub fn gain(&self) -> f32 {
        self.gain
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

    #[test]
    fn set_gain_clamps_to_range() {
        let mut d = Deck::new();
        d.set_gain(99.0);
        assert_eq!(d.gain(), GAIN_MAX);
        d.set_gain(-5.0);
        assert_eq!(d.gain(), GAIN_MIN);
    }

    #[test]
    fn nudge_gain_steps_and_clamps() {
        let mut d = Deck::new();
        d.nudge_gain(0.05);
        assert!((d.gain() - 1.05).abs() < 1e-6);
        for _ in 0..100 {
            d.nudge_gain(-0.05);
        }
        assert_eq!(d.gain(), GAIN_MIN, "nudging down clamps at min");
    }

    #[test]
    fn gain_change_ramps_without_jumping() {
        let mut d = Deck::new();
        d.load(track(10_000, 1.0, 44_100));
        d.play();
        d.set_gain(0.0); // ask for silence from unity
        let mut buf = [0.0f32; 4]; // 2 frames
        d.fill(&mut buf);
        // First frame dropped by only one ramp step — no click to silence.
        assert!(
            buf[0] > 0.99,
            "instantaneous jump to silence (zipper): {}",
            buf[0]
        );
        assert!(buf[0] < 1.0, "gain should have begun ramping down");
        // The target value is visible immediately even though audio lags.
        assert_eq!(d.gain(), 0.0);
    }

    #[test]
    fn gain_reaches_target_after_enough_frames() {
        let mut d = Deck::new();
        d.load(track(10_000, 1.0, 44_100));
        d.play();
        d.set_gain(0.5);
        let mut buf = vec![0.0f32; 8192]; // 4096 frames >> ramp length
        d.fill(&mut buf);
        // Tail has fully ramped to the 0.5 target.
        let last = buf[buf.len() - 1];
        assert!((last - 0.5).abs() < 1e-4, "expected ~0.5, got {last}");
    }

    #[test]
    fn seek_moves_playhead_to_absolute_position() {
        let mut d = Deck::new();
        d.load(track(1000, 0.5, 100)); // 10s at rate 100
        d.seek(3.0);
        assert_eq!(d.position_frames(), 300);
        assert!((d.position_secs() - 3.0).abs() < 1e-9);
    }

    #[test]
    fn seek_by_is_relative_and_clamps_at_zero() {
        let mut d = Deck::new();
        d.load(track(1000, 0.5, 100));
        d.seek(2.0);
        d.seek_by(1.5);
        assert!((d.position_secs() - 3.5).abs() < 1e-9);
        d.seek_by(-100.0); // backward past the start
        assert_eq!(d.position_frames(), 0);
    }

    #[test]
    fn seek_past_eof_clamps_and_stops() {
        let mut d = Deck::new();
        d.load(track(1000, 0.5, 100));
        d.play();
        d.seek(9_999.0); // way past the 10s end
        assert_eq!(d.position_frames(), 1000, "clamped to EOF");
        assert_eq!(d.state(), DeckState::Stopped, "EOF seek stops the deck");
    }

    #[test]
    fn seek_within_track_preserves_play_state() {
        let mut d = Deck::new();
        d.load(track(1000, 0.5, 100));
        d.play();
        d.seek(4.0);
        assert_eq!(d.state(), DeckState::Playing);
    }

    #[test]
    fn seek_declicks_by_fading_in() {
        let mut d = Deck::new();
        d.load(track(50_000, 0.5, 44_100));
        d.play();
        // Ramp the gain to full (>512 frames), so we'd hear a click on a
        // raw splice.
        let mut buf = vec![0.0f32; 4096];
        d.fill(&mut buf);
        assert!(
            (buf[buf.len() - 1] - 0.5).abs() < 1e-4,
            "level should be full pre-seek"
        );

        d.seek(0.2); // a within-track jump
        let mut after = [0.0f32; 8];
        d.fill(&mut after);
        // The first post-seek sample fades in from silence rather than
        // jumping to full level — no click.
        assert!(
            after[0].abs() < 0.05,
            "post-seek should fade in (declick), got {}",
            after[0]
        );
    }

    #[test]
    fn bpm_is_none_until_set_and_clears_on_reload() {
        let mut d = Deck::new();
        assert_eq!(d.bpm(), None);
        d.set_bpm(128.0); // no track loaded -> ignored
        assert_eq!(d.bpm(), None);

        d.load(track(100, 0.5, 44_100));
        assert_eq!(d.bpm(), None, "fresh load has no tempo yet");
        d.set_bpm(128.0);
        assert_eq!(d.bpm(), Some(128.0));

        d.load(track(100, 0.5, 44_100));
        assert_eq!(d.bpm(), None, "reloading clears the stale tempo");
    }
}
