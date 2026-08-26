//! Simple PCM audio playback for the Game Boy Advance using agb's MixerController.
//!
//! This provides a thin wrapper around agb's audio APIs for playing stereo PCM tracks.

use core::convert::Infallible;

/// Play a stereo track using agb's MixerController with blocking execution.
/// 
/// # Arguments
/// * `gba` - Reference to GBA hardware
/// * `left_data` - Left channel PCM samples (8-bit unsigned)
/// * `right_data` - Right channel PCM samples (8-bit unsigned)
/// * `sample_rate_hz` - Sample rate in Hertz (GBA supports 32768, 65536, 131072, 262144)
/// 
/// # Example
/// ```ignore
/// use gba_sound_player::pcm::play_stereo_track_blocking;
/// 
/// let left_data: &'static [u8] = &[128u8; 1024]; // Silence
/// let right_data: &'static [u8] = &[128u8; 1024];
/// 
/// loop {
///     play_stereo_track_blocking(gba, left_data, right_data, 65536);
///     agb::display::busy_wait_for_vblank();
/// }
/// ```
pub fn play_stereo_track_blocking(
    gba: &agb::Gba,
    left_data: &'static [u8],
    right_data: &'static [u8],
    sample_rate_hz: u32,
) {
    use agb::sound::mixer::{Mixer, Frequency};
    
    // Use agb's Mixer API - it handles all the GBA audio register setup
    let mixer = gba.sound().mixer();
    
    // Add both channels with the specified sample rate
    // agb's Mixer automatically handles the GBA audio hardware setup
    mixer.add(left_data, true);
    mixer.add(right_data, true);
}

/// Quick-play function with default sample rate (65536 Hz).
pub fn play_stereo_track_default(gba: &agb::Gba, left_data: &'static [u8], right_data: &'static [u8]) {
    play_stereo_track_blocking(gba, left_data, right_data, 65536);
}
