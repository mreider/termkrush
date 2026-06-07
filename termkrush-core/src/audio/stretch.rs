//! Pitch-preserving time-stretch (offline).
//!
//! Beat-matching without "chipmunk" artifacts needs to change a clip's
//! duration *without* changing its pitch — unlike plain resampling,
//! which lets pitch ride with speed. This is an overlap-add (OLA) stretch:
//! the signal is cut into overlapping Hann windows that are re-spaced at the
//! target ratio and summed back. Because the windows themselves are never
//! resampled, pitch is preserved; only the spacing (duration) changes.
//!
//! OLA is the simplest member of the WSOLA / phase-vocoder family — clean
//! for the small ratios beat-matching needs (±~8%), at the cost of some
//! smearing on sharp transients at larger ratios. Kept as a shelved future
//! "warp" option; it runs offline on a clip, not in the realtime callback.

use std::f32::consts::PI;

/// Window length (samples per channel) for the OLA stretch.
const WIN: usize = 1024;

/// Time-stretch interleaved-stereo (or mono) `samples` by `ratio`
/// (>1 = longer/slower, <1 = shorter/faster), preserving pitch. A ratio of
/// 1.0 returns the input unchanged (bit-identical round-trip).
pub fn time_stretch(samples: &[f32], channels: u16, ratio: f32) -> Vec<f32> {
    let ch = channels.max(1) as usize;
    if (ratio - 1.0).abs() < 1e-6 || samples.len() < ch * WIN {
        return samples.to_vec(); // identity / too short to window
    }
    // De-interleave, stretch each channel, re-interleave to the shortest.
    let planar: Vec<Vec<f32>> = (0..ch)
        .map(|c| {
            samples
                .iter()
                .skip(c)
                .step_by(ch)
                .copied()
                .collect::<Vec<_>>()
        })
        .map(|x| ola(&x, ratio))
        .collect();
    let frames = planar.iter().map(|p| p.len()).min().unwrap_or(0);
    let mut out = Vec::with_capacity(frames * ch);
    for f in 0..frames {
        for p in &planar {
            out.push(p[f]);
        }
    }
    out
}

/// Overlap-add stretch of one channel by `ratio` (pitch-preserving).
fn ola(x: &[f32], ratio: f32) -> Vec<f32> {
    let hop_s = WIN / 2; // fixed synthesis hop (50% overlap)
    let hop_a = ((hop_s as f32 / ratio).round() as usize).max(1); // analysis hop
    let out_len = (x.len() as f32 * ratio).round() as usize;
    let mut y = vec![0.0f32; out_len + WIN];
    let mut norm = vec![0.0f32; out_len + WIN];
    let hann: Vec<f32> = (0..WIN)
        .map(|i| 0.5 - 0.5 * (2.0 * PI * i as f32 / WIN as f32).cos())
        .collect();

    let (mut a, mut s) = (0usize, 0usize);
    while a + WIN <= x.len() && s + WIN <= y.len() {
        for i in 0..WIN {
            y[s + i] += x[a + i] * hann[i];
            norm[s + i] += hann[i];
        }
        a += hop_a;
        s += hop_s;
    }
    for i in 0..y.len() {
        if norm[i] > 1e-6 {
            y[i] /= norm[i]; // normalize the overlapped window sum
        }
    }
    y.truncate(out_len);
    y
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{frames, stereo_sine};

    #[test]
    fn ratio_one_is_identity() {
        let s = stereo_sine(440.0, 0.2, 44_100);
        assert_eq!(time_stretch(&s, 2, 1.0), s);
    }

    #[test]
    fn length_scales_with_ratio() {
        let s = stereo_sine(220.0, 0.5, 44_100); // 22_050 frames
        let longer = time_stretch(&s, 2, 1.5);
        let shorter = time_stretch(&s, 2, 0.75);
        // Within a window of the target length.
        assert!((frames(&longer) as i64 - (22_050.0 * 1.5) as i64).abs() < WIN as i64);
        assert!((frames(&shorter) as i64 - (22_050.0 * 0.75) as i64).abs() < WIN as i64);
    }

    /// Zero-crossings per output sample ≈ pitch. It must survive a stretch.
    fn zcr(buf: &[f32], channels: u16) -> f32 {
        let ch = channels as usize;
        let left: Vec<f32> = buf.iter().step_by(ch).copied().collect();
        let crossings = left
            .windows(2)
            .filter(|w| w[0].signum() != w[1].signum())
            .count();
        crossings as f32 / left.len() as f32
    }

    #[test]
    fn pitch_is_preserved() {
        let s = stereo_sine(440.0, 0.5, 44_100);
        let stretched = time_stretch(&s, 2, 1.5); // 50% slower, same pitch
        let before = zcr(&s, 2);
        let after = zcr(&stretched, 2);
        assert!(
            (before - after).abs() / before < 0.05,
            "pitch (zero-cross rate) preserved: {before} vs {after}"
        );
        assert!(stretched.iter().all(|s| s.is_finite()));
    }
}
