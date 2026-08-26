//! Simple PCM audio playback for the Game Boy Advance using agb's MixerController.
//!
//! Uses agb's built-in audio APIs for playing stereo PCM tracks.

use agb::sound::mixer::{Frequency, SoundChannel, SoundData};

/// Play a stereo track using agb's MixerController.
///
/// # Arguments
/// * `gba` - Reference to GBA hardware
/// * `sound_data` - The `SoundData` containing the audio to play
/// * `sample_rate` - The `Frequency` to play back at
///
/// # Example
/// ```ignore
/// use gba_sound_player::pcm::play_stereo_track_blocking;
/// use agb::sound::mixer::{Frequency, SoundData};
///
/// static MY_SOUND: SoundData = include_wav!("my_sound.wav");
///
/// pub fn main(mut gba: agb::Gba) -> ! {
///     play_stereo_track_blocking(&mut gba, &MY_SOUND, Frequency::Hz65536);
///     
///     loop {
///         // Need to call mixer.frame() in your game loop!
///     }
/// }
/// ```
pub fn play_stereo_track_blocking(
    gba: &mut agb::Gba,
    sound_data: &'static SoundData,
    sample_rate: Frequency,
) {
    let mut mixer = gba.mixer.mixer(sample_rate);
    let mut channel = SoundChannel::new(*sound_data);
    channel.stereo();
    let _ = mixer.play_sound(channel);
}
