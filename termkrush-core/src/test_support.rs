//! Shared test rigging for the clip engine (and audio tests generally):
//! deterministic interleaved-stereo signal generators plus simple
//! measurements. Compiled only under `cfg(test)`. The record / trim /
//! sync stories assert against these known inputs so the
//! engine's behavior is pinned to ground truth, not to itself.

/// Interleaved-stereo frame count (samples / 2).
pub fn frames(buf: &[f32]) -> usize {
    buf.len() / 2
}

/// RMS level of a buffer (0 for empty).
pub fn rms(buf: &[f32]) -> f32 {
    if buf.is_empty() {
        return 0.0;
    }
    (buf.iter().map(|s| s * s).sum::<f32>() / buf.len() as f32).sqrt()
}

/// Peak absolute sample.
pub fn peak(buf: &[f32]) -> f32 {
    buf.iter().fold(0.0f32, |m, &s| m.max(s.abs()))
}

/// A constant-amplitude interleaved-stereo buffer of `n` frames — the
/// simplest "known content" for capture / trim assertions.
pub fn flat_stereo(n: usize, amp: f32) -> Vec<f32> {
    vec![amp; n * 2]
}

/// A stereo sine at `freq` Hz for `secs` seconds (amp 0.6) — known content
/// whose RMS survives a faithful copy / resample.
pub fn stereo_sine(freq: f32, secs: f32, rate: u32) -> Vec<f32> {
    let n = (secs * rate as f32) as usize;
    let mut v = Vec::with_capacity(n * 2);
    for i in 0..n {
        let s = (2.0 * std::f32::consts::PI * freq * i as f32 / rate as f32).sin() * 0.6;
        v.push(s);
        v.push(s);
    }
    v
}

/// A stereo click track at `bpm` for `beats` beats: a short decaying tick at
/// the start of each beat, silence between. Ground truth for tempo /
/// loop-sync tests.
pub fn beat_buffer(bpm: f32, beats: usize, rate: u32) -> Vec<f32> {
    let period = (rate as f32 * 60.0 / bpm) as usize; // frames per beat
    let n = period * beats;
    let mut v = vec![0.0f32; n * 2];
    let tick = rate as usize / 100; // ~10 ms
    for b in 0..beats {
        let start = b * period;
        for j in 0..tick {
            let f = start + j;
            if f >= n {
                break;
            }
            let env = (-(j as f32) / (rate as f32 * 0.002)).exp() * 0.8;
            v[f * 2] = env;
            v[f * 2 + 1] = env;
        }
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measurements_on_known_signals() {
        // Flat buffer: rms == peak == amplitude; frame count is samples/2.
        let flat = flat_stereo(100, 0.5);
        assert_eq!(frames(&flat), 100);
        assert!((rms(&flat) - 0.5).abs() < 1e-6);
        assert!((peak(&flat) - 0.5).abs() < 1e-6);
        assert_eq!(rms(&[]), 0.0);
    }

    #[test]
    fn sine_has_expected_rms_and_peak() {
        let s = stereo_sine(440.0, 0.5, 44_100);
        assert_eq!(frames(&s), 22_050);
        assert!(peak(&s) <= 0.6 + 1e-3);
        // RMS of a full-scale sine ≈ amp / √2.
        assert!((rms(&s) - 0.6 / 2.0_f32.sqrt()).abs() < 0.02);
    }

    #[test]
    fn beat_buffer_length_and_ticks() {
        let rate = 44_100;
        let bpm = 120.0;
        let beats = 4;
        let buf = beat_buffer(bpm, beats, rate);
        let period = (rate as f32 * 60.0 / bpm) as usize;
        assert_eq!(frames(&buf), period * beats);
        // Energy at the downbeat, silence mid-beat.
        assert!(buf[0].abs() > 0.1, "tick at beat 0");
        let mid = (period / 2) * 2;
        assert_eq!(buf[mid], 0.0, "silence between beats");
    }
}
