#![no_std]
#![no_main]
#![cfg_attr(test, feature(custom_test_frameworks))]
#![cfg_attr(test, test_runner(agb::test_runner::test_runner))]
#![cfg_attr(test, reexport_test_harness_main = "test_main")]

pub mod pcm;

use agb::display::Rgb;
use agb::sound::mixer::{Frequency, SoundData};

const HELLO: agb::display::Rgb15 = Rgb::new(255, 128, 0).to_rgb15();

// This should be the target .wav file you want to play.
// Place it in the root directory or adjust the path accordingly.
// Note: agb's include_wav! macro requires the file to be present at compile time.
static TEST_SOUND: SoundData = agb::include_wav!("test.wav");

#[agb::entry]
fn main(mut gba: agb::Gba) -> ! {
    #[cfg(test)]
    test_main();

    agb::eprintln!("[main] entry started, setting up gfx and audio");

    let mut gfx = gba.graphics.get();
    gfx.set_background_palette_colour(0, 0, HELLO);
    
    agb::eprintln!("[main] screen should now be orange");

    let mut mixer = gba.mixer.mixer(Frequency::Hz32768);
    pcm::play_stereo_track_blocking(&mut mixer, &TEST_SOUND);

    loop {
        let frame = gfx.frame();
        
        // agb's software mixer requires frame() to be called in the main loop to process audio
        mixer.frame();
        
        frame.commit();
    }
}
