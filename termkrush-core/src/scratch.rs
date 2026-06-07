//! Old-school scratch support.
//!
//! A scratch pad holds a very short clip; the software finds its **scratch
//! point** — the onset where the "needle" naturally pivots — so the whip/wiki
//! rubs land on the sound. This module finds that point; the whip/wiki voice
//! rendering layers on in its own story.

/// Find the scratch pivot: the frame of the strongest onset (energy rise) in
/// an interleaved clip. Returns 0 for very short or empty clips.
pub fn detect_pivot(samples: &[f32], channels: u16) -> usize {
    let ch = channels.max(1) as usize;
    let frames = samples.len() / ch;
    if frames < 4 {
        return 0;
    }
    // ~64 analysis windows across the clip.
    let win = (frames / 64).clamp(1, 4096);
    let n = frames / win;
    if n < 2 {
        return 0;
    }
    // Per-window energy (left channel is representative enough here).
    let energy: Vec<f64> = (0..n)
        .map(|w| {
            (w * win..(w + 1) * win)
                .map(|f| {
                    let s = samples[f * ch] as f64;
                    s * s
                })
                .sum()
        })
        .collect();
    // The window with the largest energy rise from its predecessor.
    let mut best_w = 0;
    let mut best = f64::MIN;
    for w in 1..n {
        let rise = energy[w] - energy[w - 1];
        if rise > best {
            best = rise;
            best_w = w;
        }
    }
    best_w * win
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pivot_lands_on_the_onset() {
        // Silence, then a burst starting at frame 500.
        let frames = 1000;
        let mut clip = vec![0.0f32; frames * 2];
        for f in 500..540 {
            clip[f * 2] = 0.8;
            clip[f * 2 + 1] = 0.8;
        }
        let pivot = detect_pivot(&clip, 2);
        assert!((pivot as i64 - 500).abs() < 25, "pivot ~500, got {pivot}");
    }

    #[test]
    fn tiny_or_silent_clips_pivot_at_zero() {
        assert_eq!(detect_pivot(&[0.0; 4], 2), 0);
        assert_eq!(detect_pivot(&[], 2), 0);
    }
}
