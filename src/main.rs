#![no_std]
#![no_main]
#![cfg_attr(test, feature(custom_test_frameworks))]
#![cfg_attr(test, test_runner(agb::test_runner::test_runner))]
#![cfg_attr(test, reexport_test_harness_main = "test_main")]

use agb::display::Rgb;

const HELLO: agb::display::Rgb15 = Rgb::new(255, 128, 0).to_rgb15();

#[agb::entry]
fn entry(mut gba: agb::Gba) -> ! {
    agb::eprintln!("[hello] entry started, setting up diagnostic loop");

    let mut gfx = gba.graphics.get();
    gfx.set_background_palette_colour(0, 0, HELLO);
    let frame = gfx.frame();
    frame.commit();
    
    agb::eprintln!("[hello] screen should now be orange");

    psg::init_audio_master();

    loop {
        agb::eprintln!("[audio] playing Channel 1 (Sweep/Square)...");
        psg::play_ch1();
        delay(90); // 1.5 seconds

        agb::eprintln!("[audio] playing Channel 2 (Square)...");
        psg::play_ch2();
        delay(90);

        agb::eprintln!("[audio] playing Channel 4 (Noise)...");
        psg::play_ch4();
        delay(90);
    }
}

/// Spin-wait for N vblank periods.
/// Uses raw register reads to guarantee it works without agb event loops.
fn delay(frames: u32) {
    let vcount = 0x0400_0006 as *const u16;
    for _ in 0..frames {
        unsafe {
            // Wait until we are OUT of vblank (VCOUNT < 160)
            while vcount.read_volatile() >= 160 {}
            // Wait until we are IN vblank (VCOUNT >= 160)
            while vcount.read_volatile() < 160 {}
        }
    }
}

mod psg {
    // Master
    const SOUNDCNT_L:  *mut u16 = 0x0400_0080 as *mut u16;
    const SOUNDCNT_H:  *mut u16 = 0x0400_0082 as *mut u16;
    const SOUNDCNT_X_MASTER: *mut u16 = 0x0400_0084 as *mut u16;
    
    // Ch 1
    const SOUND1CNT_L: *mut u16 = 0x0400_0060 as *mut u16;
    const SOUND1CNT_H: *mut u16 = 0x0400_0062 as *mut u16;
    const SOUND1CNT_X: *mut u16 = 0x0400_0064 as *mut u16;

    // Ch 2
    const SOUND2CNT_L: *mut u16 = 0x0400_0068 as *mut u16;
    const SOUND2CNT_H: *mut u16 = 0x0400_006C as *mut u16;

    // Ch 4
    const SOUND4CNT_L: *mut u16 = 0x0400_0078 as *mut u16;
    const SOUND4CNT_H: *mut u16 = 0x0400_007C as *mut u16;

    pub fn init_audio_master() {
        unsafe {
            // 1. Turn on Master Sound Enable FIRST
            SOUNDCNT_X_MASTER.write_volatile(0x0080);
            
            // 2. Enable Channels 1, 2, and 4 on Left and Right. Max volume.
            // Bit 8=Ch1 L, 9=Ch2 L, 11=Ch4 L. (0x0B00)
            // Bit 12=Ch1 R, 13=Ch2 R, 15=Ch4 R. (0xB000)
            // Bits 0-2 = 7 (Left Vol), Bits 4-6 = 7 (Right Vol)
            // 0xBB00 | 0x0077 = 0xBB77
            SOUNDCNT_L.write_volatile(0xBB77); 
            
            // 3. Set PSG volume ratio to 100%
            let snd_cnt_h = SOUNDCNT_H.read_volatile();
            SOUNDCNT_H.write_volatile((snd_cnt_h & !0x0003) | 0x0002);
        }
    }

    pub fn play_ch1() {
        unsafe {
            SOUND1CNT_L.write_volatile(0); // Sweep off
            // Vol=15, Env=Decrease, Step=4, Duty=50%
            SOUND1CNT_H.write_volatile(0xF480);
            // Frequency 1758 (440Hz), Trigger note
            SOUND1CNT_X.write_volatile(1758 | 0x8000); 
        }
    }

    pub fn play_ch2() {
        unsafe {
            // Vol=15, Env=Decrease, Step=4, Duty=50%
            SOUND2CNT_L.write_volatile(0xF480); 
            // Frequency 1758 (440Hz), Trigger note
            SOUND2CNT_H.write_volatile(1758 | 0x8000);
        }
    }

    pub fn play_ch4() {
        unsafe {
            // Vol=15, Env=Decrease, Step=4
            SOUND4CNT_L.write_volatile(0xF400); 
            // Trigger, 7-stage step, counter/clock params
            SOUND4CNT_H.write_volatile(0x8000 | 0x0008); 
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