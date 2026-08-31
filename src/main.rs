#![no_std]
#![no_main]
#![cfg_attr(test, feature(custom_test_frameworks))]
#![cfg_attr(test, test_runner(agb::test_runner::test_runner))]
#![cfg_attr(test, reexport_test_harness_main = "test_main")]

use agb::display::Rgb;

// FLAC integration lives in examples/symphonia_flac_probe/ (see FLAC.md).
// The root ROM deliberately carries no FLAC dependency so the baseline stays buildable.

const BACKGROUND_COLOR: agb::display::Rgb15 = Rgb::new(0, 128, 128).to_rgb15(); // Teal

#[agb::entry]
fn main(mut gba: agb::Gba) -> ! {
    #[cfg(test)]
    test_main();

    agb::eprintln!("[main] entry started, setting up gfx");

    let mut gfx = gba.graphics.get();
    gfx.set_background_palette_colour(0, 0, BACKGROUND_COLOR);

    agb::eprintln!("[main] ready for FLAC decoding exploration");

    loop {
        let frame = gfx.frame();
        frame.commit();
    }
}
