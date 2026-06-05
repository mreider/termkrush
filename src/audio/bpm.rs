//! Offline tempo (BPM) detection.
//!
//! Pure Rust, no `libaubio` C dependency — keeps the static build clean.
//! The pipeline is the classic one: an onset-strength envelope (positive
//! energy flux per hop) autocorrelated against itself, then the lag with
//! the strongest periodicity over a tempo band is read back as BPM.
//!
//! It runs offline (on the whole decoded buffer), so the caller should
//! invoke it on a background thread — see the TUI's load path.

/// Tempo search band. Dance music lives well inside this; band-limiting
/// also sidesteps most octave (half/double-tempo) confusion.
const MIN_BPM: f32 = 80.0;
const MAX_BPM: f32 = 185.0;

/// Hop size (samples) for the onset envelope. Smaller → finer lag
/// resolution; 128 at 44.1 kHz gives a ~344 Hz envelope, plenty for the
/// fractional-lag autocorrelation to resolve sub-BPM.
const HOP: usize = 128;

/// Detect the dominant tempo of interleaved-stereo (or mono) `samples` in
/// BPM. Returns `None` when the clip is too short or carries no rhythmic
/// energy. Accuracy on a clean beat is well within ±0.5 BPM.
pub fn detect_bpm(samples: &[f32], channels: u16, sample_rate: u32) -> Option<f32> {
    if sample_rate == 0 {
        return None;
    }
    let env = onset_envelope(samples, channels);
    // Need at least a couple of seconds of envelope to trust a tempo.
    let frame_rate = sample_rate as f32 / HOP as f32;
    if env.len() < (frame_rate * 2.0) as usize {
        return None;
    }

    // Fine scan over the tempo band; raw (un-normalized, mean-removed)
    // autocorrelation favors the fundamental over its sub-multiples.
    let mut best_bpm = 0.0f32;
    let mut best_score = f32::NEG_INFINITY;
    let steps = ((MAX_BPM - MIN_BPM) / 0.1).round() as usize;
    for i in 0..=steps {
        let bpm = MIN_BPM + i as f32 * 0.1;
        let lag = 60.0 * frame_rate / bpm; // fractional frames per beat
        let score = autocorr_frac(&env, lag);
        if score > best_score {
            best_score = score;
            best_bpm = bpm;
        }
    }

    // No periodic structure found (silence / noise): score collapses to ~0.
    if best_score <= 0.0 {
        return None;
    }
    Some(best_bpm)
}

/// Onset-strength envelope: per-hop energy, half-wave-rectified first
/// difference (energy flux), then mean-removed so the autocorrelation of
/// non-periodic regions sits near zero.
fn onset_envelope(samples: &[f32], channels: u16) -> Vec<f32> {
    let ch = channels.max(1) as usize;
    let frames = samples.len() / ch;
    let nhops = frames / HOP;
    if nhops == 0 {
        return Vec::new();
    }

    // Per-hop energy of the mono mix.
    let mut energy = vec![0.0f32; nhops];
    for (h, e) in energy.iter_mut().enumerate() {
        let mut sum = 0.0f32;
        for f in 0..HOP {
            let frame = h * HOP + f;
            // Average the channels to mono.
            let mut m = 0.0f32;
            for c in 0..ch {
                m += samples[frame * ch + c];
            }
            m /= ch as f32;
            sum += m * m;
        }
        *e = sum;
    }

    // Half-wave-rectified energy flux.
    let mut env = vec![0.0f32; nhops];
    for h in 1..nhops {
        env[h] = (energy[h] - energy[h - 1]).max(0.0);
    }
    let mean = env.iter().sum::<f32>() / env.len() as f32;
    for v in &mut env {
        *v -= mean;
    }
    env
}

/// Autocorrelation of `env` at a fractional `lag` (in frames), linearly
/// interpolating between samples. Raw sum (not normalized by overlap), so
/// shorter lags with more aligned energy — i.e. the fundamental period —
/// outscore their longer sub-multiples.
fn autocorr_frac(env: &[f32], lag: f32) -> f32 {
    let n = env.len();
    let li = lag.floor() as usize;
    let frac = lag - li as f32;
    let mut sum = 0.0f32;
    for k in 0..n {
        let j = k + li;
        if j + 1 >= n {
            break;
        }
        let interp = env[j] * (1.0 - frac) + env[j + 1] * frac;
        sum += env[k] * interp;
    }
    sum
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A mono click track: a short impulse at the start of every beat for
    /// `secs` seconds at `bpm`. Clean ground truth for the detector.
    fn click_track(bpm: f32, secs: f32, sample_rate: u32) -> Vec<f32> {
        let n = (sample_rate as f32 * secs) as usize;
        let period = (sample_rate as f32 * 60.0 / bpm).round() as usize;
        let mut buf = vec![0.0f32; n];
        let mut beat = 0;
        while beat * period < n {
            // 5 ms decaying tick.
            let start = beat * period;
            for j in 0..(sample_rate as usize / 200) {
                let idx = start + j;
                if idx >= n {
                    break;
                }
                let env = (-(j as f32) / (sample_rate as f32 * 0.002)).exp();
                buf[idx] = env;
            }
            beat += 1;
        }
        buf
    }

    #[test]
    fn detects_known_tempos_within_half_bpm() {
        let sr = 44_100;
        for &bpm in &[
            90.0, 100.0, 110.0, 120.0, 124.0, 128.0, 135.0, 140.0, 150.0, 174.0,
        ] {
            let track = click_track(bpm, 12.0, sr);
            let got = detect_bpm(&track, 1, sr).expect("should detect a tempo");
            assert!(
                (got - bpm).abs() <= 0.5,
                "bpm {bpm}: detected {got} (off by {:.2})",
                got - bpm
            );
        }
    }

    #[test]
    fn detects_tempo_on_stereo_input() {
        let sr = 44_100;
        let mono = click_track(128.0, 12.0, sr);
        // Interleave the same signal to both channels.
        let mut stereo = Vec::with_capacity(mono.len() * 2);
        for s in mono {
            stereo.push(s);
            stereo.push(s);
        }
        let got = detect_bpm(&stereo, 2, sr).expect("detect stereo");
        assert!((got - 128.0).abs() <= 0.5, "stereo: detected {got}");
    }

    #[test]
    fn silence_has_no_tempo() {
        let sr = 44_100;
        assert_eq!(detect_bpm(&vec![0.0; sr as usize * 4], 1, sr), None);
    }

    #[test]
    fn too_short_returns_none() {
        let sr = 44_100;
        assert_eq!(detect_bpm(&vec![0.1; sr as usize / 2], 1, sr), None);
    }
}
