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
    const SOUNDCNT_H:  *mut u16 = 0x0400_0082 as *mut u16;
    const SOUNDCNT_X_MASTER: *mut u16 = 0x0400_0084 as *mut u16;
    const SOUNDBIAS:   *mut u16 = 0x0400_0088 as *mut u16;

    pub fn play_tone_a4() {
        unsafe {
            agb::eprintln!("[audio] --- BEFORE INIT ---");
            agb::eprintln!("[audio] SOUNDCNT_L (0x80): {:#06x}", SOUNDCNT_L.read_volatile());
            agb::eprintln!("[audio] SOUNDCNT_H (0x82): {:#06x}", SOUNDCNT_H.read_volatile());
            agb::eprintln!("[audio] SOUNDCNT_X (0x84): {:#06x}", SOUNDCNT_X_MASTER.read_volatile());
            agb::eprintln!("[audio] SOUNDBIAS  (0x88): {:#06x}", SOUNDBIAS.read_volatile());

            // 1. SOUNDCNT_X (0x0400_0084): Master Sound Enable
            // Bit 7 turns on the entire sound circuit.
            // While `agb` might do this during init, setting it explicitly ensures it's on.
            SOUNDCNT_X_MASTER.write_volatile(0x0080);

            // 2. SOUNDCNT_L (0x0400_0080): DMG Master Volume & LR enable
            // Bits 0-2: Left volume (7 = max)
            // Bits 4-6: Right volume (7 = max)
            // Bit 8: Ch 1 Left enable
            // Bit 12 (0xC): Ch 1 Right enable
            // 0x1177 sets max volume and enables Ch 1 on both left and right outputs.
            SOUNDCNT_L.write_volatile(0x1177);

            // 3. SOUNDCNT_H (0x0400_0082): PSG Volume Ratio
            // Bits 0-1: Output sound ratio for chan. 1-4 (0=25%, 1=50%, 2=100%)
            // We do a read-modify-write to preserve Direct Sound settings (if any), setting PSG volume to 100%.
            let mut snd_cnt_h = SOUNDCNT_H.read_volatile();
            snd_cnt_h = (snd_cnt_h & !0x0003) | 0x0002;
            SOUNDCNT_H.write_volatile(snd_cnt_h);

            // 4. SOUND1CNT_L (0x0400_0060): Sweep register
            // 0 = disable sweep
            SOUND1CNT_L.write_volatile(0);

            // 5. SOUND1CNT_H (0x0400_0062): Duty / Length / Envelope
            // Bits 6-7: Wave duty cycle (2 = 50%) -> 0x0080
            // Bits 12-15: Initial volume (15 = max) -> 0xF000
            // Envelope step time (bits 8-10) is 0 (disabled).
            SOUND1CNT_H.write_volatile(0xF080);

            // 6. SOUND1CNT_X (0x0400_0064): Frequency / Control
            // Bits 0-10: Frequency. For 440 Hz, x = 1758.
            // Bit 15: Initial (trigger the note) -> 0x8000
            SOUND1CNT_X.write_volatile(1758 | 0x8000);

            agb::eprintln!("[audio] --- AFTER INIT ---");
            agb::eprintln!("[audio] SOUNDCNT_L (0x80): {:#06x}", SOUNDCNT_L.read_volatile());
            agb::eprintln!("[audio] SOUNDCNT_H (0x82): {:#06x}", SOUNDCNT_H.read_volatile());
            agb::eprintln!("[audio] SOUNDCNT_X (0x84): {:#06x}", SOUNDCNT_X_MASTER.read_volatile());
            agb::eprintln!("[audio] SOUND1CNT_L (0x60): {:#06x}", SOUND1CNT_L.read_volatile());
            agb::eprintln!("[audio] SOUND1CNT_H (0x62): {:#06x}", SOUND1CNT_H.read_volatile());
            agb::eprintln!("[audio] SOUND1CNT_X (0x64): {:#06x}", SOUND1CNT_X.read_volatile());
            agb::eprintln!("[audio] SOUNDBIAS  (0x88): {:#06x}", SOUNDBIAS.read_volatile());
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
