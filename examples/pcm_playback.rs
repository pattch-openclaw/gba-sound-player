#![no_std]
#![no_main]

use agb::display::Rgb;
use agb::sound::mixer::{Frequency, Mixer, SoundChannel, SoundData};

const HELLO: agb::display::Rgb15 = Rgb::new(255, 128, 0).to_rgb15();

// This should be the target .wav file you want to play.
// Note: agb's include_wav! macro requires the file to be present at compile time.
static TEST_SOUND: SoundData = agb::include_wav!("assets/sound/test.wav");

/// Play a stereo track using agb's MixerController.
pub fn play_stereo_track_blocking<'a>(mixer: &mut Mixer<'a>, sound_data: &'static SoundData) {
    let mut channel = SoundChannel::new(*sound_data);
    channel.stereo();
    let _ = mixer.play_sound(channel);
}

#[agb::entry]
fn main(mut gba: agb::Gba) -> ! {
    agb::eprintln!("[pcm_playback] entry started, setting up gfx and audio");

    let mut gfx = gba.graphics.get();
    gfx.set_background_palette_colour(0, 0, HELLO);

    agb::eprintln!("[pcm_playback] screen should now be orange");

    let mut mixer = gba.mixer.mixer(Frequency::Hz32768);
    play_stereo_track_blocking(&mut mixer, &TEST_SOUND);

    loop {
        let frame = gfx.frame();

        // agb's software mixer requires frame() to be called in the main loop to process audio
        mixer.frame();

        frame.commit();
    }
}
