//! MP3 export via a bundled LAME (libmp3lame is vendored + compiled by
//! `mp3lame-encoder`, so the binary needs no external tool at runtime).
//!
//! Decoding mp3 is handled by symphonia (see [`decode`](super::decode)); this
//! is the write side, for sharing a render as an `.mp3`.

use std::io;
use std::path::Path;

use mp3lame_encoder::{
    max_required_buffer_size, Bitrate, Builder, FlushNoGap, InterleavedPcm, Quality,
};

fn lame_err<E: std::fmt::Debug>(e: E) -> io::Error {
    io::Error::other(format!("mp3 encode: {e:?}"))
}

/// Encode interleaved `samples` (range -1..1) to an MP3 at `path` (192 kbps).
pub fn export_mp3(path: &Path, samples: &[f32], sample_rate: u32, channels: u16) -> io::Result<()> {
    let channels = channels.max(1) as u8;
    let pcm: Vec<i16> = samples
        .iter()
        .map(|&s| (s.clamp(-1.0, 1.0) * 32767.0).round() as i16)
        .collect();

    let mut builder = Builder::new().ok_or_else(|| lame_err("LAME init failed"))?;
    builder.set_num_channels(channels).map_err(lame_err)?;
    builder.set_sample_rate(sample_rate).map_err(lame_err)?;
    builder.set_brate(Bitrate::Kbps192).map_err(lame_err)?;
    builder.set_quality(Quality::Best).map_err(lame_err)?;
    let mut enc = builder.build().map_err(lame_err)?;

    // Explicit, generously-sized output buffer (the `*_to_vec` helpers can
    // under-size for interleaved input, corrupting memory in libmp3lame).
    let mut out: Vec<u8> = Vec::with_capacity(max_required_buffer_size(pcm.len()) + 7200);
    let n = enc
        .encode(InterleavedPcm(&pcm), out.spare_capacity_mut())
        .map_err(lame_err)?;
    // SAFETY: lame initialized `n` bytes in the reserved capacity.
    unsafe { out.set_len(n) };

    out.reserve(7200); // headroom for the flush frame
    let m = enc
        .flush::<FlushNoGap>(out.spare_capacity_mut())
        .map_err(lame_err)?;
    unsafe { out.set_len(out.len() + m) };

    std::fs::write(path, &out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::stereo_sine;

    #[test]
    fn export_mp3_round_trips_through_the_decoder() {
        let tmp = std::env::temp_dir().join(format!("tk-mp3-{}.mp3", std::process::id()));
        let src = stereo_sine(440.0, 1.0, 44_100); // 1 s
        export_mp3(&tmp, &src, 44_100, 2).unwrap();
        assert!(std::fs::metadata(&tmp).unwrap().len() > 0, "wrote an mp3");
        // It decodes back to roughly the source duration (lossy, so ~).
        let dec = crate::audio::decode_file(&tmp, 44_100).expect("decode the mp3 we wrote");
        let secs = dec.samples.len() as f64 / 2.0 / 44_100.0;
        assert!((secs - 1.0).abs() < 0.1, "round-trip ~1s, got {secs:.3}");
        let _ = std::fs::remove_file(&tmp);
    }
}
