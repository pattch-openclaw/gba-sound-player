#![no_std]
#![no_main]
#![cfg_attr(test, feature(custom_test_frameworks))]
#![cfg_attr(test, test_runner(agb::test_runner::test_runner))]
#![cfg_attr(test, reexport_test_harness_main = "test_main")]

//! Hello-world GBA ROM: solid backdrop colour + milestone logging.
//!
//! This exercises the absolute minimum of the agb display pipeline
//! (we just change the hardware backdrop colour, avoiding tiles/palettes).
//! We use this to verify the toolchain, ROM fixing, and emulator are all
//! working before we add audio or text on top.
//!
//! We also emit a short log of milestones via `agb::eprintln!`. That macro
//! talks to mGBA over a cheat-code handshake and prints to the mGBA **console**
//! (the terminal mGBA is running in / a `--log-file`). It does nothing on real
//! hardware, so it's safe to leave in. If the console stops logging part-way
//! through, that line is where the ROM died.
//!
//! Expected result in mGBA: a solid orange screen (RGB 255,128,0), no crash.

use agb::display::Rgb;

/// Unmistakable colour: orange (255, 128, 0). Not black, not white — if we see
/// *this*, the display pipeline is working.
const HELLO: agb::display::Rgb15 = Rgb::new(255, 128, 0).to_rgb15();

#[agb::entry]
fn entry(mut gba: agb::Gba) -> ! {
    agb::eprintln!("[hello] entry started");

    // --- Graphics: change the backdrop colour ---
    // Palette 0, colour 0 is the hardware backdrop colour. Anything not covered
    // by a tile or sprite will show this colour.
    let mut gfx = gba.graphics.get();
    agb::eprintln!("[hello] graphics acquired");

    gfx.set_background_palette_colour(0, 0, HELLO);
    agb::eprintln!("[hello] backdrop colour set to orange");

    let mut frame = gfx.frame();
    agb::eprintln!("[hello] frame acquired");
    
    frame.commit();
    agb::eprintln!("[hello] frame committed — screen should now be orange");

    loop {
        agb::halt();
    }
}

#[cfg(test)]
mod tests {
    #[test_case]
    fn test_sanity_check(_gba: &mut agb::Gba) {
        agb::eprintln!("[test] sanity check running");
        assert_eq!(1, 1, "basic math works");
    }
}
