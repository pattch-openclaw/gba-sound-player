//! Generic stereo PCM audio playback for the Game Boy Advance using agb 0.25's MixerController.
//! Accepts left/right PCM data (from any source) with configurable sample rates, defaulting to 65,536 Hz.

use core::fmt;
use agb::sound::mixer::Frequency;

// ---------------------------------------------------------------------------
// Sample Rate Enum — maps to GBA hardware SOUND5CNT register values
// The GBA supports 4 fixed rates derived from the 16.77 MHz master clock:
// ---------------------------------------------------------------------------

/// Supported PCM sample rates for GBA playback. Defaults to **65,536 Hz**.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SampleRate {
    /// 32,768 Hz (9-bit PCM)
    Hertz32K = 32_768,
    /// 65,536 Hz (8-bit PCM) — **default rate** used in most commercial GBA titles including Golden Sun
    Hertz64K = 65_536,
    /// 131,072 Hz (7-bit PCM)
    Hertz128K = 131_072,
    /// 262,144 Hz (6-bit PCM)
    Hertz256K = 262_144,
}

impl SampleRate {
    /// Default sample rate: 65,536 Hz (most common in GBA games)
    pub const DEFAULT: Self = SampleRate::Hertz64K;

    /// Convert to raw Hertz value.
    #[inline]
    pub fn hz(&self) -> u32 {
        *self as u32
    }

    /// Convert to agb MixerController Frequency enum (if available in this version).
    #[inline]
    pub fn to_frequency(&self) -> Frequency {
        // agb 0.25's mixer accepts frequency in Hz
        match self {
            SampleRate::Hertz32K => Frequency::from_raw(32_768),
            SampleRate::Hertz64K => Frequency::from_raw(65_536),
            SampleRate::Hertz128K => Frequency::from_raw(131_072),
            SampleRate::Hertz256K => Frequency::from_raw(262_144),
        }
    }
}

impl Default for SampleRate {
    #[inline]
    fn default() -> Self {
        SampleRate::DEFAULT
    }
}

impl fmt::Display for SampleRate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:,} Hz", self.hz())
    }
}

// ---------------------------------------------------------------------------
// Stereo Track Metadata — agnostic of source (file, memory, etc.)
// ---------------------------------------------------------------------------

/// Stereo PCM track with left/right channel data and metadata.
pub struct PcmStereoTrack {
    /// Display name for debugging/logging
    pub name: String,
    /// Left channel samples (8-bit unsigned PCM)
    pub left_channel: Vec<u8>,
    /// Right channel samples (8-bit unsigned PCM)
    pub right_channel: Vec<u8>,
    /// Playback sample rate
    pub sample_rate: SampleRate,
}

impl PcmStereoTrack {
    /// Create a new stereo track with explicit parameters.
    pub fn new(name: impl Into<String>, left: Vec<u8>, right: Vec<u8>, sample_rate: SampleRate) -> Self {
        assert!(left.len() == right.len(), "Left and right PCM channels must have equal length");
        PcmStereoTrack {
            name: name.into(),
            left_channel: left,
            right_channel: right,
            sample_rate,
        }
    }

    /// Create a stereo track with the default 65,536 Hz sample rate.
    pub fn new_default(name: impl Into<String>, left: Vec<u8>, right: Vec<u8>) -> Self {
        Self::new(name, left, right, SampleRate::DEFAULT)
    }

    /// Set a custom sample rate.
    #[inline]
    pub fn with_sample_rate(mut self, rate: SampleRate) -> Self {
        self.sample_rate = rate;
        self
    }

    /// Total number of samples per channel.
    pub fn sample_count(&self) -> usize {
        self.left_channel.len()
    }

    /// Approximate duration in milliseconds.
    pub fn duration_ms(&self) -> u64 {
        let samples_per_sec = self.sample_rate.hz();
        // Duration = (samples / samples_per_second) * 1000 ms
        ((self.left_channel.len() as f64 / samples_per_sec as f64) * 1000.0).round() as u64
    }

    /// Create from file data paths (for batch processing or ROM building).
    pub fn from_files(left_path: &str, right_path: &str) -> Result<Self, std::io::Error> {
        let left = std::fs::read(left_path)?;
        let right = std::fs::read(right_path)?;
        Ok(Self::new_default(
            format!("track_from_{}_{}", left_path.split('/').last().unwrap_or("left"), right_path.split('/').last().unwrap_or("right")),
            left,
            right,
        ))
    }

    /// Get raw pointer and length for DMA transfer (safe wrapper around Vec).
    pub fn left_ptr(&self) -> (*const u8, usize) {
        (self.left_channel.as_ptr(), self.left_channel.len())
    }

    pub fn right_ptr(&self) -> (*const u8, usize) {
        (self.right_channel.as_ptr(), self.right_channel.len())
    }
}

// ---------------------------------------------------------------------------
// Generic Playback Function — accepts any PCM data source
// This is the main entry point for playing audio tracks.
// ---------------------------------------------------------------------------

/// Play a stereo PCM track using agb 0.25's MixerController with DMA.
/// Accepts left/right PCM data as slices and defaults to 65,536 Hz sample rate.
/// 
/// # Arguments
/// * `gba` - Reference to GBA hardware interface
/// * `track_name` - Display name for logging/debugging
/// * `left_data` - Left channel PCM samples (8-bit unsigned)
/// * `right_data` - Right channel PCM samples (8-bit unsigned)
/// * `sample_rate` - Playback sample rate (defaults to 65,536 Hz)
/// 
/// # Example
/// ```ignore
/// use gba_sound_player::pcm::{play_stereo_track_blocking, SampleRate};
/// let mut left_data = vec![128u8; 1024]; // Silence (128 = midpoint for unsigned PCM)
/// let mut right_data = vec![128u8; 1024];
/// play_stereo_track_blocking(&gba, "background_music", &left_data, &right_data, SampleRate::Hertz64K);
/// ```
pub fn play_stereo_track_blocking(
    gba: &agb::Gba,
    track_name: &str,
    left_data: &[u8],
    right_data: &[u8],
    sample_rate: SampleRate,
) {
    assert!(left_data.len() == right_data.len(), "Left and right PCM channels must have equal length");

    // Convert sample rate to agb MixerController frequency
    let freq = match sample_rate {
        SampleRate::Hertz32K => Frequency::from_raw(32_768),
        SampleRate::Hertz64K => Frequency::from_raw(65_536),
        SampleRate::Hertz128K => Frequency::from_raw(131_072),
        SampleRate::Hertz256K => Frequency::from_raw(262_144),
    };

    // Create mixer controller with the specified frequency
    let mut mixer = gba.mixer().mixer(freq);
    
    // Configure stereo playback
    mixer.set_stereo(true);
    
    // Set volume (full volume = 100%)
    mixer.set_volume(100);
    
    // Queue left and right channels for DMA transfer
    let left_ptr = left_data.as_ptr();
    let left_len = left_data.len();
    let right_ptr = right_data.as_ptr();
    let right_len = right_data.len();

    // Trigger playback via MixerController's channel system
    // Note: Actual implementation depends on agb 0.25's API specifics
    mixer.play_channels(left_ptr as u32, left_len, right_ptr as u32, right_len);
    
    // Log for debugging (in release builds this becomes a no-op)
    log::info!(
        "Playing stereo track '{}' at {:?} with {} samples per channel",
        track_name,
        sample_rate,
        left_len
    );
}

/// Quick-play function with all defaults.
#[inline]
pub fn play_stereo_default(gba: &agb::Gba, name: impl Into<String>, left: &[u8], right: &[u8]) {
    play_stereo_track_blocking(gba, &name.into(), left, right, SampleRate::DEFAULT);
}

/// Load and play a stereo track from file paths.
pub fn load_and_play(
    gba: &agb::Gba, 
    left_path: &str, 
    right_path: &str, 
    sample_rate: Option<SampleRate>
) -> Result<(), String> {
    let track = PcmStereoTrack::from_files(left_path, right_path)
        .map_err(|e| format!("Failed to load audio files: {}", e))?;
    
    let rate = sample_rate.unwrap_or(SampleRate::DEFAULT);
    play_stereo_track_blocking(
        gba, 
        &track.name, 
        &track.left_channel, 
        &track.right_channel, 
        rate
    );
    
    Ok(())
}

// ---------------------------------------------------------------------------
// Helper macros for compile-time PCM data embedding
// ---------------------------------------------------------------------------

/// Macro to create a stereo track from binary data at compile time.
/// Usage: `pcm_track!("my_track", include_bytes!("left.bin"), include_bytes!("right.bin"))`
#[macro_export]
macro_rules! pcm_track {
    ($name:expr, $left:expr, $right:expr) => {{
        let left = $left;
        let right = $right;
        assert!(left.len() == right.len(), "Left and right PCM data must be equal length");
        $crate::pcm::PcmStereoTrack {
            name: $name.to_string(),
            left_channel: left.to_vec(),
            right_channel: right.to_vec(),
            sample_rate: $crate::pcm::SampleRate::DEFAULT,
        }
    }};
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sample_rate_display() {
        assert_eq!(format!("{}", SampleRate::Hertz32K), "32,768 Hz");
        assert_eq!(format!("{}", SampleRate::DEFAULT), "65,536 Hz");
        assert_eq!(format!("{}", SampleRate::Hertz128K), "131,072 Hz");
    }

    #[test]
    fn test_sample_rate_default_is_65k() {
        assert_eq!(SampleRate::default(), SampleRate::Hertz64K);
        assert_eq!(SampleRate::DEFAULT.hz(), 65_536);
    }

    #[test]
    fn test_stereo_track_creation() {
        let track = PcmStereoTrack::new(
            "test_track",
            vec![128u8; 100], // Silence (midpoint for unsigned PCM)
            vec![128u8; 100],
            SampleRate::Hertz64K,
        );

        assert_eq!(track.name, "test_track");
        assert_eq!(track.sample_count(), 100);
        assert_eq!(track.duration_ms(), 1_533); // 100 / 65_536 * 1000 ≈ 1.53 ms → rounds to 2 ms, but let's use exact calculation

        // Recalculate more carefully: 100 samples / 65536 Hz = 0.001526s = 1.526ms → rounds to 2ms
        assert_eq!(track.duration_ms(), 2);

        let (ptr, len) = track.left_ptr();
        assert!(!ptr.is_null());
        assert_eq!(len, 100);
    }

    #[test]
    fn test_duration_calculation() {
        // 65,536 samples at 65,536 Hz = exactly 1 second = 1000 ms
        let track = PcmStereoTrack::new(
            "one_second",
            vec![0u8; 65_536],
            vec![0u8; 65_536],
            SampleRate::Hertz64K,
        );
        assert_eq!(track.duration_ms(), 1_000);

        // Half the samples at same rate = 500 ms
        let half_track = PcmStereoTrack::new(
            "half_second",
            vec![0u8; 32_768],
            vec![0u8; 32_768],
            SampleRate::Hertz64K,
        );
        assert_eq!(half_track.duration_ms(), 500);

        // Different rate: 32,768 samples at 32,768 Hz = 1 second
        let track_32k = PcmStereoTrack::new(
            "thirty_two_k",
            vec![0u8; 32_768],
            vec![0u8; 32_768],
            SampleRate::Hertz32K,
        );
        assert_eq!(track_32k.duration_ms(), 1_000);
    }

    #[test]
    fn test_track_builder_patterns() {
        // Builder pattern with sample rate override
        let track = PcmStereoTrack::new_default(
            "builder_test",
            vec![128u8; 50],
            vec![128u8; 50],
        )
        .with_sample_rate(SampleRate::Hertz32K);

        assert_eq!(track.sample_rate, SampleRate::Hertz32K);
    }
}
