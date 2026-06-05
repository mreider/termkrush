//! Audio engine: output device, decoding, resampling, and the realtime
//! callback path.
//!
//! Planned crates (decided per their owning stories): `cpal` for device
//! output (here), `symphonia` for decoding, `rubato` for resampling and
//! time-stretch, `hound` + an mp3 encoder for recording.

pub mod decode;
pub mod output;

pub use decode::{decode_file, DecodeError, DecodedAudio};
pub use output::{AudioOutput, SineSink, Sink};
