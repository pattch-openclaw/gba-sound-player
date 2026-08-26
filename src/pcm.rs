//! Simple PCM audio playback for the Game Boy Advance using agb's MixerController.
//!
//! Uses agb's built-in audio APIs for playing stereo PCM tracks.

use agb::sound::mixer::{Mixer, SoundChannel, SoundData};

/// Play a stereo track using agb's MixerController.
///
/// # Arguments
/// * `mixer` - The active `Mixer` instance
/// * `sound_data` - The `SoundData` containing the audio to play
///
/// # Example
/// ```ignore
/// use gba_sound_player::pcm::play_stereo_track_blocking;
/// use agb::sound::mixer::{Frequency, SoundData};
///
/// static MY_SOUND: SoundData = include_wav!("my_sound.wav");
///
/// pub fn main(mut gba: agb::Gba) -> ! {
///     let mut mixer = gba.mixer.mixer(Frequency::Hz32768);
///     play_stereo_track_blocking(&mut mixer, &MY_SOUND);
///     
///     loop {
///         mixer.frame();
///     }
/// }
/// ```
pub fn play_stereo_track_blocking<'a>(
    mixer: &mut Mixer<'a>,
    sound_data: &'static SoundData,
) {
    let mut channel = SoundChannel::new(*sound_data);
    channel.stereo();
    let _ = mixer.play_sound(channel);
}
