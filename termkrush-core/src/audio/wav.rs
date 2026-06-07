//! Minimal 16-bit PCM WAV writer — the native render/export format.
//!
//! TermKrush renders the arrangement to interleaved-stereo `f32`; this writes
//! it as a standard 16-bit PCM `.wav` that any player (and our own decoder,
//! via symphonia) reads back.

use std::io::{self, Write};
use std::path::Path;

/// Write interleaved `samples` (range -1..1) to a 16-bit PCM WAV at `path`.
pub fn write_wav(path: &Path, samples: &[f32], sample_rate: u32, channels: u16) -> io::Result<()> {
    let channels = channels.max(1);
    let bits: u16 = 16;
    let block_align = channels * bits / 8;
    let byte_rate = sample_rate * block_align as u32;
    let data_len = (samples.len() * (bits / 8) as usize) as u32;

    let mut f = io::BufWriter::new(std::fs::File::create(path)?);
    f.write_all(b"RIFF")?;
    f.write_all(&(36 + data_len).to_le_bytes())?;
    f.write_all(b"WAVE")?;
    f.write_all(b"fmt ")?;
    f.write_all(&16u32.to_le_bytes())?; // PCM fmt-chunk size
    f.write_all(&1u16.to_le_bytes())?; // format = PCM
    f.write_all(&channels.to_le_bytes())?;
    f.write_all(&sample_rate.to_le_bytes())?;
    f.write_all(&byte_rate.to_le_bytes())?;
    f.write_all(&block_align.to_le_bytes())?;
    f.write_all(&bits.to_le_bytes())?;
    f.write_all(b"data")?;
    f.write_all(&data_len.to_le_bytes())?;
    for &s in samples {
        let v = (s.clamp(-1.0, 1.0) * 32767.0).round() as i16;
        f.write_all(&v.to_le_bytes())?;
    }
    f.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_a_readable_wav_header() {
        let tmp = std::env::temp_dir().join(format!("tk-wav-{}.wav", std::process::id()));
        let samples = vec![0.5f32; 200]; // 100 stereo frames
        write_wav(&tmp, &samples, 44_100, 2).unwrap();
        let bytes = std::fs::read(&tmp).unwrap();
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(&bytes[8..12], b"WAVE");
        // data chunk = 200 samples × 2 bytes.
        let data_len = u32::from_le_bytes([bytes[40], bytes[41], bytes[42], bytes[43]]);
        assert_eq!(data_len, 400);
        assert_eq!(bytes.len(), 44 + 400, "header + PCM data");
        let _ = std::fs::remove_file(&tmp);
    }
}
