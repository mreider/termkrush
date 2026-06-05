//! Audio output: a `cpal` stream fed by a lock-free ring buffer.
//!
//! The realtime callback is the consumer end of an `rtrb` SPSC ring; any
//! producer (the future mixer, or the `--test-tone` driver) writes
//! interleaved samples into the producer end. The callback never blocks,
//! allocates, or locks: on underrun it writes silence and bumps an xrun
//! counter rather than stalling.

use std::f32::consts::PI;
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

/// A source of interleaved audio frames.
///
/// `fill` writes one buffer's worth of interleaved samples in `[-1.0,
/// 1.0]`. The buffer length is a whole number of frames (`channels`
/// samples each). The future mixer implements this; [`SineSink`] is the
/// test-tone implementation.
pub trait Sink: Send {
    fn fill(&mut self, out: &mut [f32]);
}

/// A constant-frequency sine, written to every channel of each frame so
/// the tone plays on all outputs regardless of channel count.
pub struct SineSink {
    phase: f32,
    step: f32,
    amp: f32,
    channels: usize,
}

impl SineSink {
    pub fn new(freq: f32, sample_rate: u32, amp: f32, channels: u16) -> Self {
        Self {
            phase: 0.0,
            step: 2.0 * PI * freq / sample_rate as f32,
            amp,
            channels: channels.max(1) as usize,
        }
    }
}

impl Sink for SineSink {
    fn fill(&mut self, out: &mut [f32]) {
        for frame in out.chunks_mut(self.channels) {
            let s = self.amp * self.phase.sin();
            self.phase += self.step;
            if self.phase >= 2.0 * PI {
                self.phase -= 2.0 * PI;
            }
            for slot in frame.iter_mut() {
                *slot = s;
            }
        }
    }
}

/// Pop available samples from `consumer` into `out`, filling any
/// remainder with silence. Returns the number of silence (underrun)
/// samples written — zero means the ring kept up.
pub fn drain_into(consumer: &mut rtrb::Consumer<f32>, out: &mut [f32]) -> usize {
    let mut filled = 0;
    for slot in out.iter_mut() {
        match consumer.pop() {
            Ok(v) => {
                *slot = v;
                filled += 1;
            }
            Err(_) => *slot = 0.0,
        }
    }
    out.len() - filled
}

/// Errors starting the output stream. Stringly-wraps the backend errors
/// so callers don't depend on cpal's error taxonomy.
#[derive(Debug)]
pub enum AudioError {
    NoDevice,
    UnsupportedFormat(String),
    Backend(String),
}

impl fmt::Display for AudioError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AudioError::NoDevice => write!(f, "no default output device"),
            AudioError::UnsupportedFormat(s) => write!(f, "unsupported sample format: {s}"),
            AudioError::Backend(s) => write!(f, "audio backend error: {s}"),
        }
    }
}

impl std::error::Error for AudioError {}

/// A running output stream plus the metadata callers need.
pub struct AudioOutput {
    _stream: cpal::Stream,
    pub sample_rate: u32,
    pub channels: u16,
    xruns: Arc<AtomicU64>,
}

impl AudioOutput {
    /// Open the default output device and start a stream. Returns the
    /// handle and the producer end of the ring; write interleaved f32
    /// samples (matching [`channels`](Self::channels)) into it.
    pub fn start(ring_capacity: usize) -> Result<(Self, rtrb::Producer<f32>), AudioError> {
        let host = cpal::default_host();
        let device = host.default_output_device().ok_or(AudioError::NoDevice)?;
        let config = device
            .default_output_config()
            .map_err(|e| AudioError::Backend(e.to_string()))?;

        let sample_rate = config.sample_rate().0;
        let channels = config.channels();
        tracing::info!(
            sample_rate,
            channels,
            format = ?config.sample_format(),
            "audio: opening output stream"
        );

        if config.sample_format() != cpal::SampleFormat::F32 {
            return Err(AudioError::UnsupportedFormat(format!(
                "{:?}",
                config.sample_format()
            )));
        }

        let (producer, mut consumer) = rtrb::RingBuffer::<f32>::new(ring_capacity);
        let xruns = Arc::new(AtomicU64::new(0));
        let xruns_cb = Arc::clone(&xruns);

        let stream = device
            .build_output_stream(
                &config.into(),
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    let silence = drain_into(&mut consumer, data);
                    if silence > 0 {
                        xruns_cb.fetch_add(1, Ordering::Relaxed);
                    }
                },
                move |err| tracing::error!(error = %err, "audio: output stream error"),
                None,
            )
            .map_err(|e| AudioError::Backend(e.to_string()))?;

        stream
            .play()
            .map_err(|e| AudioError::Backend(e.to_string()))?;

        Ok((
            Self {
                _stream: stream,
                sample_rate,
                channels,
                xruns,
            },
            producer,
        ))
    }

    /// Number of callbacks that hit an underrun (had to emit silence).
    pub fn xruns(&self) -> u64 {
        self.xruns.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rms(buf: &[f32]) -> f32 {
        let s: f64 = buf.iter().map(|&x| (x as f64) * (x as f64)).sum();
        (s / buf.len() as f64).sqrt() as f32
    }

    #[test]
    fn sine_sink_level_and_continuity() {
        let mut sink = SineSink::new(440.0, 44100, 0.5, 2);
        let mut buf = vec![0.0f32; 44100 * 2]; // 1s stereo
        sink.fill(&mut buf);
        // RMS of a 0.5-amplitude sine ≈ 0.354.
        let r = rms(&buf);
        assert!((r - 0.3536).abs() < 0.01, "rms {r}");
        // No discontinuities frame-to-frame (smooth sine).
        for w in buf.windows(2) {
            assert!((w[1] - w[0]).abs() < 0.2, "click: {} -> {}", w[0], w[1]);
        }
    }

    #[test]
    fn sine_sink_writes_all_channels_equally() {
        let mut sink = SineSink::new(1000.0, 48000, 0.8, 2);
        let mut buf = vec![0.0f32; 8];
        sink.fill(&mut buf);
        // Each stereo frame has L == R.
        for frame in buf.chunks(2) {
            assert_eq!(frame[0], frame[1]);
        }
    }

    #[test]
    fn drain_reports_underrun_and_fills_silence() {
        let (mut p, mut c) = rtrb::RingBuffer::<f32>::new(16);
        for i in 0..4 {
            p.push(i as f32).unwrap();
        }
        let mut out = vec![9.0f32; 8];
        let underrun = drain_into(&mut c, &mut out);
        assert_eq!(underrun, 4, "8 requested, 4 available -> 4 silence");
        assert_eq!(&out[0..4], &[0.0, 1.0, 2.0, 3.0]);
        assert_eq!(&out[4..8], &[0.0, 0.0, 0.0, 0.0], "remainder is silence");
    }

    #[test]
    fn drain_no_underrun_when_full_enough() {
        let (mut p, mut c) = rtrb::RingBuffer::<f32>::new(16);
        for _ in 0..8 {
            p.push(0.25).unwrap();
        }
        let mut out = vec![0.0f32; 8];
        assert_eq!(drain_into(&mut c, &mut out), 0);
        assert!(out.iter().all(|&v| v == 0.25));
    }
}
