#![no_std]
#![no_main]
#![cfg_attr(test, feature(custom_test_frameworks))]
#![cfg_attr(test, test_runner(agb::test_runner::test_runner))]
#![cfg_attr(test, reexport_test_harness_main = "test_main")]

//! Hello-world GBA ROM: solid backdrop colour + milestone logging + 440 Hz tone.
//!
//! We build upon the working display pipeline by re-introducing the PSG tone.
//! 
//! Expected result in mGBA: 
//! 1. A solid orange screen (RGB 255,128,0).
//! 2. A 440 Hz (A4) square wave tone playing indefinitely.
//! 3. No crashes, logs complete in the console.

use agb::display::Rgb;

const HELLO: agb::display::Rgb15 = Rgb::new(255, 128, 0).to_rgb15();

#[agb::entry]
fn entry(mut gba: agb::Gba) -> ! {
    agb::eprintln!("[hello] entry started");

    let mut gfx = gba.graphics.get();
    agb::eprintln!("[hello] graphics acquired");

    gfx.set_background_palette_colour(0, 0, HELLO);
    agb::eprintln!("[hello] backdrop colour set to orange");

    let frame = gfx.frame();
    agb::eprintln!("[hello] frame acquired");
    
    frame.commit();
    agb::eprintln!("[hello] frame committed — screen should now be orange");

    psg::play_tone_a4();
    agb::eprintln!("[hello] PSG tone configured and playing");

    loop {
        agb::halt();
    }
}

/// Direct hardware access to GBA SOUND1 (PSG channel 1).
///
/// agb 0.25 already enables the master sound circuit (`SOUNDCNT_X`) and sets
/// up the sound bias when `agb::Gba` is initialized. We just need to route
/// channel 1 to the left/right outputs and configure the tone.
mod psg {
    // Correct hardware register addresses (gbatek):
    const SOUND1CNT_L: *mut u16 = 0x0400_0060 as *mut u16;
    const SOUND1CNT_H: *mut u16 = 0x0400_0062 as *mut u16;
    const SOUND1CNT_X: *mut u16 = 0x0400_0064 as *mut u16;
    const SOUNDCNT_L:  *mut u16 = 0x0400_0080 as *mut u16;

    pub fn play_tone_a4() {
        unsafe {
            // SOUNDCNT_L (0x0400_0080): Master Volume & LR enable
            // Bits 0-2: Right volume (7 = max)
            // Bits 4-6: Left volume (7 = max)
            // Bit 8: Ch 1 Right enable
            // Bit 12: Ch 1 Left enable
            SOUNDCNT_L.write_volatile(0x1177);

            // SOUND1CNT_L (0x0400_0060): Sweep register
            // 0 = disable sweep
            SOUND1CNT_L.write_volatile(0);

            // SOUND1CNT_H (0x0400_0062): Duty / Length / Envelope
            // Bits 6-7: Duty cycle (2 = 50%) -> 0x0080
            // Bits 12-15: Initial volume (15 = max) -> 0xF000
            // We leave length at 0 (continuous) and envelope step at 0.
            SOUND1CNT_H.write_volatile(0xF080);

            // SOUND1CNT_X (0x0400_0064): Frequency / Control
            // Bits 0-10: Frequency. For 440 Hz, x = 1758.
            // Bit 15: Initial (trigger the note) -> 0x8000
            SOUND1CNT_X.write_volatile(1758 | 0x8000);
        }
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
