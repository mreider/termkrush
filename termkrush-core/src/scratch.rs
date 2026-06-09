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

/// Find percussive onsets (energy-rise peaks) across an interleaved clip, in
/// frames — used to draw "where the beat is" markers. Peaks above an adaptive
/// threshold, spaced at least ~80 ms apart so a single hit isn't double-counted.
pub fn detect_onsets(samples: &[f32], channels: u16, sample_rate: u32) -> Vec<usize> {
    let ch = channels.max(1) as usize;
    let frames = samples.len() / ch;
    let win = (sample_rate as usize / 100).max(64); // ~10 ms windows
    let n = frames / win;
    if n < 4 {
        return Vec::new();
    }
    // Per-window energy (channel 0 is representative), then positive flux.
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
    let flux: Vec<f64> = (0..n)
        .map(|w| {
            if w == 0 {
                0.0
            } else {
                (energy[w] - energy[w - 1]).max(0.0)
            }
        })
        .collect();
    let mean = flux.iter().sum::<f64>() / n as f64;
    let var = flux.iter().map(|x| (x - mean) * (x - mean)).sum::<f64>() / n as f64;
    let thresh = mean + 1.5 * var.sqrt();
    let min_gap = ((sample_rate as usize * 80 / 1000) / win).max(1); // ~80 ms
    let mut onsets = Vec::new();
    let mut last: Option<usize> = None;
    for w in 1..n - 1 {
        let peak = flux[w] > thresh && flux[w] >= flux[w - 1] && flux[w] >= flux[w + 1];
        if peak && last.map_or(true, |l| w - l >= min_gap) {
            onsets.push(w * win);
            last = Some(w);
        }
    }
    onsets
}

/// One linear move of the read head across the record, gated by `gain`
/// (the crossfader): `0.0` = muted (silent), `1.0` = audible.
#[derive(Debug, Clone, Copy)]
pub struct Stroke {
    /// Start read frame (absolute).
    pub from: f64,
    /// End read frame (absolute).
    pub to: f64,
    /// Output gain over the stroke (0 muted, 1 audible).
    pub gain: f32,
    /// How many output frames the stroke spans.
    pub dur: f64,
}

/// A **whip**: push the record forward with the fader closed (muted), then
/// pull it back audible — the classic backward "whip" swoosh.
pub fn whip(pivot: usize, slice: usize) -> Vec<Stroke> {
    let (p, s) = (pivot as f64, slice as f64);
    vec![
        Stroke {
            from: p,
            to: p + s,
            gain: 0.0,
            dur: s,
        }, // forward, muted
        Stroke {
            from: p + s,
            to: p,
            gain: 1.0,
            dur: s,
        }, // back, audible
    ]
}

/// A **wiki**: forward audible then back audible — both motions sound (the
/// "wik-i"). Combined with whips this builds wiki-whip / whip-wiki phrases.
pub fn wiki(pivot: usize, slice: usize) -> Vec<Stroke> {
    let (p, s) = (pivot as f64, slice as f64);
    vec![
        Stroke {
            from: p,
            to: p + s,
            gain: 1.0,
            dur: s,
        }, // forward, audible
        Stroke {
            from: p + s,
            to: p,
            gain: 1.0,
            dur: s,
        }, // back, audible
    ]
}

/// One unit in a scratch phrase.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScratchUnit {
    Whip,
    Wiki,
}

/// Expand a phrase of units into one back-to-back stroke list (each unit a
/// whip or wiki around `pivot`, `slice` frames per half-rub) — tempo-locked
/// when `slice` is derived from the beat grid.
pub fn phrase_strokes(units: &[ScratchUnit], pivot: usize, slice: usize) -> Vec<Stroke> {
    let mut out = Vec::new();
    for u in units {
        match u {
            ScratchUnit::Whip => out.extend(whip(pivot, slice)),
            ScratchUnit::Wiki => out.extend(wiki(pivot, slice)),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_onsets_finds_spaced_hits() {
        // 4 clicks at interior frames in a 2s stereo clip @ 8 kHz (none at 0,
        // which falls in the skipped first window).
        let sr = 8000u32;
        let want = [800usize, 4000, 8000, 12000];
        let mut samples = vec![0.0f32; (sr as usize * 2) * 2];
        for &f in &want {
            for j in 0..32 {
                let idx = (f + j) * 2;
                if idx < samples.len() {
                    samples[idx] = 0.9; // a short loud burst (channel 0)
                }
            }
        }
        let onsets = detect_onsets(&samples, 2, sr);
        assert_eq!(onsets.len(), 4, "one per click, got {onsets:?}");
        for (got, w) in onsets.iter().zip(want) {
            assert!(
                (*got as i64 - w as i64).abs() <= sr as i64 / 100 + 1,
                "{got} vs {w}"
            );
        }
    }

    #[test]
    fn phrase_expands_to_concatenated_strokes() {
        let units = [ScratchUnit::Whip, ScratchUnit::Wiki];
        let s = phrase_strokes(&units, 100, 50);
        assert_eq!(s.len(), 4, "two units × two strokes");
        assert_eq!(s[0].gain, 0.0, "whip forward muted first");
        assert_eq!(s[2].gain, 1.0, "wiki forward audible");
    }

    #[test]
    fn whip_mutes_forward_sounds_backward() {
        let w = whip(100, 50);
        assert_eq!(w.len(), 2);
        assert_eq!(w[0].gain, 0.0, "forward muted");
        assert_eq!(w[1].gain, 1.0, "backward audible");
        assert!(
            w[0].to > w[0].from && w[1].to < w[1].from,
            "forward then back"
        );
    }

    #[test]
    fn wiki_sounds_both_ways() {
        assert!(wiki(100, 50).iter().all(|s| s.gain == 1.0));
    }

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
