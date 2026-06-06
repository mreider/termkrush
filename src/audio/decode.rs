//! Decode an audio file into interleaved stereo `f32` the mixer can play.
//!
//! `symphonia` opens the file, probes the container, picks the default
//! audio track, and decodes packets into `f32` blocks. We fold those into
//! two channels (mono is duplicated; >2 channels keep L/R), and — when the
//! source sample rate differs from the requested output rate — resample
//! with `rubato`. The result is a flat `Vec<f32>` of `L, R, L, R, …` at
//! the output rate, plus the metadata a deck wants to show: duration,
//! source format, and any ID3 title/artist.
//!
//! Output samples are interleaved stereo in `[-1.0, 1.0]`, matching the
//! [`Sink`](super::Sink) contract the output stream consumes.

use std::fmt;
use std::path::Path;

use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::{MetadataOptions, MetadataRevision, StandardTagKey};
use symphonia::core::probe::Hint;

use rubato::{
    Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction,
};

/// A fully decoded track: interleaved stereo samples plus metadata.
#[derive(Debug, Clone)]
pub struct DecodedAudio {
    /// Interleaved stereo, `L, R, L, R, …`, at [`sample_rate`](Self::sample_rate).
    pub samples: Vec<f32>,
    /// Output sample rate in Hz (after any resampling).
    pub sample_rate: u32,
    /// Always 2 — the pipeline normalizes everything to stereo.
    pub channels: u16,
    /// Source file's native sample rate, before resampling.
    pub source_sample_rate: u32,
    /// Source file's native channel count, before the stereo fold.
    pub source_channels: u16,
    /// Track duration in seconds, measured from the gapless-trimmed frame
    /// count (encoder delay/padding removed), so a lossy source reports its
    /// original length.
    pub duration_secs: f64,
    /// ID3/Vorbis title, if the file carries one.
    pub title: Option<String>,
    /// ID3/Vorbis artist, if the file carries one.
    pub artist: Option<String>,
}

impl DecodedAudio {
    /// Number of stereo frames (one frame == one `L`+`R` pair).
    pub fn frames(&self) -> usize {
        self.samples.len() / 2
    }
}

/// Why a decode failed. Stringly-wraps the backend errors so callers don't
/// depend on symphonia's or rubato's error taxonomy (mirrors
/// [`AudioError`](super::output::AudioError)).
#[derive(Debug)]
pub enum DecodeError {
    /// The file could not be opened.
    Open(String),
    /// No decodable audio track in the container.
    NoTrack,
    /// The track is missing the sample rate / channel layout we need.
    MissingParams,
    /// symphonia failed while probing or decoding.
    Decode(String),
    /// rubato failed to construct or run the resampler.
    Resample(String),
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DecodeError::Open(s) => write!(f, "cannot open audio file: {s}"),
            DecodeError::NoTrack => write!(f, "no decodable audio track in file"),
            DecodeError::MissingParams => write!(f, "track is missing sample rate or channels"),
            DecodeError::Decode(s) => write!(f, "decode error: {s}"),
            DecodeError::Resample(s) => write!(f, "resample error: {s}"),
        }
    }
}

impl std::error::Error for DecodeError {}

/// Decode `path` to interleaved stereo `f32` at `target_sample_rate`.
///
/// Mono sources are duplicated to both channels; sources with more than
/// two channels keep the first two as L/R. When the source rate already
/// equals `target_sample_rate`, the samples pass through without
/// resampling.
pub fn decode_file(
    path: impl AsRef<Path>,
    target_sample_rate: u32,
) -> Result<DecodedAudio, DecodeError> {
    let path = path.as_ref();

    let file = std::fs::File::open(path).map_err(|e| DecodeError::Open(e.to_string()))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    // The extension is a hint, not a requirement — symphonia still probes
    // the bytes, so a mislabeled file is decoded by content.
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }

    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|e| DecodeError::Decode(e.to_string()))?;

    let mut format = probed.format;

    // First track with a real codec.
    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .ok_or(DecodeError::NoTrack)?;
    let track_id = track.id;
    let params = track.codec_params.clone();

    let source_sample_rate = params.sample_rate.ok_or(DecodeError::MissingParams)?;
    let source_channels = params
        .channels
        .map(|c| c.count() as u16)
        .ok_or(DecodeError::MissingParams)?;
    if source_channels == 0 {
        return Err(DecodeError::MissingParams);
    }

    let mut decoder = symphonia::default::get_codecs()
        .make(&params, &DecoderOptions::default())
        .map_err(|e| DecodeError::Decode(e.to_string()))?;

    // Metadata: read title/artist from the container's current revision.
    let (mut title, mut artist) = (None, None);
    if let Some(rev) = format.metadata().current() {
        read_tags(rev, &mut title, &mut artist);
    }

    // Decode every packet into one planar pair of channels (left/right) at
    // the source rate. Planar suits rubato, which resamples per channel.
    let mut left: Vec<f32> = Vec::new();
    let mut right: Vec<f32> = Vec::new();
    let src_ch = source_channels as usize;

    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            // Clean end of stream: symphonia signals EOF as an IoError.
            Err(SymphoniaError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                break
            }
            Err(SymphoniaError::ResetRequired) => break,
            Err(e) => return Err(DecodeError::Decode(e.to_string())),
        };
        if packet.track_id() != track_id {
            continue;
        }

        match decoder.decode(&packet) {
            Ok(decoded) => {
                let spec = *decoded.spec();
                let mut buf = SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
                buf.copy_interleaved_ref(decoded);
                fold_to_stereo(buf.samples(), src_ch, &mut left, &mut right);
            }
            // A corrupt packet is skipped, not fatal — keep decoding.
            Err(SymphoniaError::DecodeError(_)) => continue,
            Err(e) => return Err(DecodeError::Decode(e.to_string())),
        }
    }

    // Gapless trim: lossy encoders prepend decoder-priming frames
    // (`delay`) and append frames to fill the last block (`padding`).
    // symphonia reports these but does not remove them, so we do — without
    // it an mp3 round-trips ~30ms longer than its source.
    let delay = params.delay.unwrap_or(0) as usize;
    let padding = params.padding.unwrap_or(0) as usize;
    if delay > 0 || padding > 0 {
        let total = left.len();
        let start = delay.min(total);
        let end = total.saturating_sub(padding).max(start);
        left = left[start..end].to_vec();
        right = right[start..end].to_vec();
    }

    // After trimming, the decoded frame count is the true track length.
    let duration_secs = left.len() as f64 / source_sample_rate as f64;

    // Resample to the output rate if needed.
    let (out_l, out_r, out_rate) = if source_sample_rate == target_sample_rate {
        (left, right, source_sample_rate)
    } else {
        let (l, r) = resample_stereo(&left, &right, source_sample_rate, target_sample_rate)?;
        (l, r, target_sample_rate)
    };

    // Interleave back to L, R, L, R, …
    let mut samples = Vec::with_capacity(out_l.len() * 2);
    for (l, r) in out_l.iter().zip(out_r.iter()) {
        samples.push(*l);
        samples.push(*r);
    }

    Ok(DecodedAudio {
        samples,
        sample_rate: out_rate,
        channels: 2,
        source_sample_rate,
        source_channels,
        duration_secs,
        title,
        artist,
    })
}

/// Append one decoded block (interleaved at `src_ch` channels) onto the
/// planar `left`/`right` accumulators, normalizing to stereo: mono is
/// duplicated; two-or-more channels keep the first two as L/R.
fn fold_to_stereo(interleaved: &[f32], src_ch: usize, left: &mut Vec<f32>, right: &mut Vec<f32>) {
    if src_ch == 1 {
        for &s in interleaved {
            left.push(s);
            right.push(s);
        }
    } else {
        for frame in interleaved.chunks(src_ch) {
            left.push(frame[0]);
            right.push(frame[1]);
        }
    }
}

/// Resample a planar stereo pair from `src_rate` to `dst_rate` with a
/// windowed-sinc resampler. Feeds rubato fixed-size input chunks (padding
/// the final short chunk with silence) and trims the output to the
/// rate-scaled length so the duration stays accurate.
fn resample_stereo(
    left: &[f32],
    right: &[f32],
    src_rate: u32,
    dst_rate: u32,
) -> Result<(Vec<f32>, Vec<f32>), DecodeError> {
    let ratio = dst_rate as f64 / src_rate as f64;
    // A 128-tap sinc is transparent for music playback and ~2x cheaper than
    // 256; the resample runs offline on a background thread, but keeping it
    // snappy matters for the load-time "loading…" wait.
    let params = SincInterpolationParameters {
        sinc_len: 128,
        f_cutoff: 0.95,
        interpolation: SincInterpolationType::Linear,
        oversampling_factor: 128,
        window: WindowFunction::BlackmanHarris2,
    };

    let chunk = 8192;
    let mut resampler = SincFixedIn::<f32>::new(ratio, 2.0, params, chunk, 2)
        .map_err(|e| DecodeError::Resample(e.to_string()))?;

    let frames = left.len();
    let expected_out = (frames as f64 * ratio).round() as usize;
    let mut out_l: Vec<f32> = Vec::with_capacity(expected_out + chunk);
    let mut out_r: Vec<f32> = Vec::with_capacity(expected_out + chunk);

    let mut pos = 0;
    while pos < frames {
        let need = resampler.input_frames_next();
        let end = (pos + need).min(frames);
        let mut in_l = left[pos..end].to_vec();
        let mut in_r = right[pos..end].to_vec();
        // The resampler wants exactly `need` frames; pad the tail chunk.
        in_l.resize(need, 0.0);
        in_r.resize(need, 0.0);

        let out = resampler
            .process(&[in_l, in_r], None)
            .map_err(|e| DecodeError::Resample(e.to_string()))?;
        out_l.extend_from_slice(&out[0]);
        out_r.extend_from_slice(&out[1]);
        pos = end;
    }

    out_l.truncate(expected_out);
    out_r.truncate(expected_out);
    Ok((out_l, out_r))
}

/// Pull title/artist out of a metadata revision, leaving any already-found
/// value in place (so a higher-priority source isn't overwritten).
fn read_tags(rev: &MetadataRevision, title: &mut Option<String>, artist: &mut Option<String>) {
    for tag in rev.tags() {
        match tag.std_key {
            Some(StandardTagKey::TrackTitle) if title.is_none() => {
                *title = Some(tag.value.to_string());
            }
            Some(StandardTagKey::Artist) if artist.is_none() => {
                *artist = Some(tag.value.to_string());
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fold_mono_duplicates_to_both_channels() {
        let mut l = Vec::new();
        let mut r = Vec::new();
        fold_to_stereo(&[0.1, 0.2, 0.3], 1, &mut l, &mut r);
        assert_eq!(l, vec![0.1, 0.2, 0.3]);
        assert_eq!(r, vec![0.1, 0.2, 0.3]);
    }

    #[test]
    fn fold_stereo_splits_lr() {
        let mut l = Vec::new();
        let mut r = Vec::new();
        // Interleaved L,R,L,R
        fold_to_stereo(&[1.0, -1.0, 0.5, -0.5], 2, &mut l, &mut r);
        assert_eq!(l, vec![1.0, 0.5]);
        assert_eq!(r, vec![-1.0, -0.5]);
    }

    #[test]
    fn fold_multichannel_keeps_first_two() {
        let mut l = Vec::new();
        let mut r = Vec::new();
        // One 5.1-ish frame of 6 channels; only ch0/ch1 are kept.
        fold_to_stereo(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0], 6, &mut l, &mut r);
        assert_eq!(l, vec![1.0]);
        assert_eq!(r, vec![2.0]);
    }
}
