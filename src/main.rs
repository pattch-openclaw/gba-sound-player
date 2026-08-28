#![no_std]
#![no_main]
#![cfg_attr(test, feature(custom_test_frameworks))]
#![cfg_attr(test, test_runner(agb::test_runner::test_runner))]
#![cfg_attr(test, reexport_test_harness_main = "test_main")]

use agb::display::Rgb;

use ferrosintesis_flac;
use flac;

const BACKGROUND_COLOR: agb::display::Rgb15 = Rgb::new(0, 128, 128).to_rgb15(); // Teal

#[agb::entry]
fn main(mut gba: agb::Gba) -> ! {
    #[cfg(test)]
    test_main();

    agb::eprintln!("[main] entry started, setting up gfx");

    let mut gfx = gba.graphics.get();
    gfx.set_background_palette_colour(0, 0, BACKGROUND_COLOR);
    
    agb::eprintln!("[main] ready for FLAC decoding exploration");

    // Minimal reference to force compilation of these crates
    let _ = ferrosintesis_flac::decode_mono16;
    let _ = flac::StreamReader::<&[u8]>::new;

    loop {
        let frame = gfx.frame();
        frame.commit();
    }
}
