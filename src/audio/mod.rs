//! Audio engine: output device, decoding, resampling, and the realtime
//! callback path.
//!
//! Planned crates (decided per their owning stories): `cpal` for device
//! output, `symphonia` for decoding mp3/wav/flac, `rubato` for
//! resampling and time-stretch, `hound` + an mp3 encoder for recording.
//!
//! Everything here is a placeholder until the audio-output story lands.

/// Initialize the audio subsystem (open the default output device, start
/// the stream). Returns once the engine is ready to accept decks.
///
/// Placeholder: wired up in the audio-output foundation story.
pub fn init() {
    // intentionally empty for the scaffold
}
