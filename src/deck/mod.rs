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
    /// Playhead in stereo frames into `track.samples`. Fractional so the
    /// deck can play at non-unity `speed` (varispeed) via interpolation.
    pos: f64,
    /// Varispeed multiplier: 1.0 = native; >1 faster (and higher-pitched),
    /// <1 slower. Pitch rides with speed (the easy, record-on-a-platter way).
    speed: f32,
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
    /// Detected *base* tempo in BPM (native), filled in asynchronously after
    /// load. The effective tempo reported by [`bpm`](Self::bpm) is this times
    /// the varispeed `speed`.
    bpm: Option<f32>,
}

/// Varispeed range: half to double speed (±1 octave of pitch).
pub const SPEED_MIN: f32 = 0.5;
pub const SPEED_MAX: f32 = 2.0;

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
            pos: 0.0,
            speed: 1.0,
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
        self.pos = 0.0;
        self.speed = 1.0;
        self.state = DeckState::Loaded;
        self.name = None;
        self.bpm = None;
    }

    /// Record the detected *base* tempo (BPM), set asynchronously once
    /// analysis finishes; ignored with no track loaded.
    pub fn set_bpm(&mut self, bpm: f32) {
        if self.track.is_some() {
            self.bpm = Some(bpm);
        }
    }

    /// Nudge the varispeed multiplier by `delta`, clamped. Pitch and tempo
    /// rise/fall together; the effective BPM ([`bpm`](Self::bpm)) follows.
    pub fn nudge_speed(&mut self, delta: f32) {
        self.speed = (self.speed + delta).clamp(SPEED_MIN, SPEED_MAX);
    }

    /// The current varispeed multiplier (1.0 = native).
    pub fn speed(&self) -> f32 {
        self.speed
    }

    /// Set the varispeed multiplier directly (clamped). Used by deck sync.
    pub fn set_speed(&mut self, speed: f32) {
        self.speed = speed.clamp(SPEED_MIN, SPEED_MAX);
    }

    /// The detected *base* tempo (without varispeed), if known.
    pub fn base_bpm(&self) -> Option<f32> {
        self.bpm
    }

    /// Capture the interleaved-stereo samples between two frame positions
    /// (non-destructive — the track is untouched). Bounds are clamped and
    /// ordered, so `capture(out, in)` works too; an empty/invalid range
    /// yields an empty buffer. Used to record a clip off the deck.
    pub fn capture(&self, a: usize, b: usize) -> Vec<f32> {
        let Some(track) = &self.track else {
            return Vec::new();
        };
        let total = track.frames();
        let (lo, hi) = (a.min(b).min(total), a.max(b).min(total));
        track.samples[lo * 2..hi * 2].to_vec()
    }

    /// Effective tempo in BPM = detected base × varispeed, if a base is known.
    pub fn bpm(&self) -> Option<f32> {
        self.bpm.map(|b| b * self.speed)
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
            self.pos = 0.0;
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
        let frame = secs.max(0.0) * rate;
        if frame as usize >= total {
            self.pos = total as f64; // clamp to EOF
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
        let step = self.speed as f64;

        let target = self.gain;
        let mut g = self.smoothed_gain;
        let mut n = 0; // output frames actually drawn from the track
        for i in 0..frames_out {
            let p0 = self.pos as usize;
            if p0 >= total {
                break;
            }
            // Ramp the applied gain toward the target by at most one step.
            if g < target {
                g = (g + GAIN_RAMP_STEP).min(target);
            } else if g > target {
                g = (g - GAIN_RAMP_STEP).max(target);
            }
            // Linear interpolation between adjacent frames for varispeed; the
            // final frame interpolates with itself (p1 clamped).
            let p1 = (p0 + 1).min(total - 1);
            let frac = (self.pos - p0 as f64) as f32;
            let a = p0 * 2;
            let b = p1 * 2;
            out[i * 2] = (track.samples[a] * (1.0 - frac) + track.samples[b] * frac) * g;
            out[i * 2 + 1] =
                (track.samples[a + 1] * (1.0 - frac) + track.samples[b + 1] * frac) * g;
            self.pos += step;
            n += 1;
        }
        self.smoothed_gain = g;
        // Silence the remainder (end-of-track underrun, or an over-long buffer).
        out[n * 2..].iter_mut().for_each(|s| *s = 0.0);

        if self.pos as usize >= total {
            // Track finished: halt and rewind so the next play restarts it.
            self.state = DeckState::Stopped;
            self.pos = 0.0;
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

    /// Playhead position in stereo frames (floored).
    pub fn position_frames(&self) -> usize {
        self.pos as usize
    }

    /// Playhead position in seconds (0 with no track).
    pub fn position_secs(&self) -> f64 {
        match &self.track {
            Some(t) if t.sample_rate > 0 => self.pos / t.sample_rate as f64,
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
    fn varispeed_consumes_source_faster_and_stays_finite() {
        let rate = 1000;
        let mut d = Deck::new();
        d.load(track(rate as usize, 0.3, rate)); // 1s = 1000 frames
        d.nudge_speed(1.0); // 1.0 + 1.0 clamped to the 2.0 max
        assert!((d.speed() - 2.0).abs() < 1e-6);
        d.play();
        let mut buf = vec![0.0f32; 200]; // 100 output frames
        let n = d.fill(&mut buf);
        assert_eq!(n, 100, "produced 100 output frames");
        // At 2x, ~200 source frames were consumed for 100 output frames.
        assert!(
            (d.position_frames() as i64 - 200).abs() <= 1,
            "advanced ~2x source"
        );
        assert!(
            buf.iter().all(|s| s.is_finite()),
            "varispeed output is finite"
        );
    }

    #[test]
    fn capture_clamps_orders_and_copies_region() {
        let mut d = Deck::new();
        d.load(track(100, 0.5, 1000)); // 100 frames
        assert_eq!(d.capture(10, 30).len(), 40, "20 frames * 2 samples");
        assert_eq!(d.capture(30, 10).len(), 40, "bounds get ordered");
        assert_eq!(d.capture(90, 999).len(), 20, "clamped to track end");
        assert!(d.capture(50, 50).is_empty(), "empty range");
        assert!(d.capture(10, 30).iter().all(|&s| (s - 0.5).abs() < 1e-6));
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
