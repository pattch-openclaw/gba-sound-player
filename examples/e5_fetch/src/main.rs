#![cfg_attr(target_arch = "arm", no_std)]
#![cfg_attr(target_arch = "arm", no_main)]
#![deny(unsafe_code)]
//! # e5-fetch — hardware arbitration of the ROM→IWRAM instruction-fetch ratio
//!
//! E5 is the only measurement that can turn the decoder's mGBA numbers into a
//! real-time claim. The question is narrow: **the same ARM instruction stream**
//! is assembled twice — once into `.text` (cartridge ROM) and once into
//! `.iwram` — and both copies are timed in one image, one calibration, with the
//! same inputs. On mGBA a 32-bit ARM instruction costs ~4 cycles from ROM and
//! ~1.4 from IWRAM; this ROM prices that ratio on silicon.
//!
//! Two things matter on hardware that mGBA hides:
//! * `WAITCNT` (0x0400_0204). `agb`'s boot sets `0x4317` (WS0 = 3,1, prefetch
//!   **enabled**). The ratio is measured in both the boot state and with the
//!   prefetch buffer disabled, because the buffer is the single biggest
//!   confound in a 4→1.4 claim.
//! * No serial console. `agb::println!` only reaches mGBA; the results are
//!   therefore also rendered on the GBA screen (the committed OFL pixel font)
//!   so a stock unit with an EverDrive can be read directly.
//!
//! Run it in mGBA first (`make native-e5-fetch-rom` then
//! `mgba-test-runner e5-fetch.gba`) as the harness check: mGBA should read
//! ~4.0 and ~1.4 cycles/add. The hardware run is the arbitration.

#[cfg(target_arch = "arm")]
extern crate alloc;

#[cfg(target_arch = "arm")]
mod text;
#[cfg(target_arch = "arm")]
#[allow(unsafe_code)]
mod timer;

#[cfg(target_arch = "arm")]
#[allow(unsafe_code)]
mod asm_unit {
    //! The two assembled copies. The allowance is for the edition-2024
    //! `global_asm!` (an unsafe op) and the `unsafe extern` declaration; the
    //! wrappers are the only call sites, and each names its invariant.

    core::arch::global_asm!(include_str!("body.s"));

    unsafe extern "C" {
        fn e5_rom_body(input: u32) -> u32;
        fn e5_iwram_body(input: u32) -> u32;
    }

    /// Call the ROM-resident copy.
    pub fn rom(input: u32) -> u32 {
        // SAFETY: the body is a leaf that reads/writes only r0/r1 (caller-saved
        // in this C ABI) and returns in r0.
        unsafe { e5_rom_body(input) }
    }

    /// Call the IWRAM-resident copy.
    pub fn iwram(input: u32) -> u32 {
        // SAFETY: same body, different section.
        unsafe { e5_iwram_body(input) }
    }
}

#[cfg(target_arch = "arm")]
mod rom {
    use crate::asm_unit;
    use crate::text;
    use crate::timer::{self, Counter};
    use agb::display::{Rgb, Rgb15};
    use alloc::format;
    use alloc::string::String;

    /// `add r0, r0, #1` per call (16 outer iterations × 256 adds).
    const ADDS_PER_CALL: u32 = 4096;
    /// Reported windows per placement, per WAITCNT state.
    const WINDOWS: usize = 5;

    const COLOR_PENDING: Rgb15 = Rgb::new(128, 0, 255).to_rgb15();
    const COLOR_PASS: Rgb15 = Rgb::new(0, 0, 255).to_rgb15();
    const COLOR_FAIL: Rgb15 = Rgb::new(255, 0, 0).to_rgb15();

    /// One WAITCNT state's reading.
    struct Readings {
        rom: u32,
        iwram: u32,
        ratio_x1000: u32,
        rom_per_add_x1000: u32,
        iwram_per_add_x1000: u32,
    }

    fn measure_state(counter: &mut Counter<'_>, waitcnt: u16) -> Option<Readings> {
        timer::set_waitcnt(waitcnt);
        counter.soft_reset();
        let overhead = timer::calibrate(counter)?;

        let mut rom_sum = 0u64;
        let mut iwram_sum = 0u64;
        for _ in 0..WINDOWS {
            let (rom_out, raw) = timer::timed(counter, || asm_unit::rom(0));
            if rom_out != ADDS_PER_CALL {
                return None;
            }
            rom_sum += u64::from(raw.saturating_sub(overhead));

            let (iwram_out, raw) = timer::timed(counter, || asm_unit::iwram(0));
            if iwram_out != ADDS_PER_CALL {
                return None;
            }
            iwram_sum += u64::from(raw.saturating_sub(overhead));
        }

        let rom = (rom_sum / WINDOWS as u64) as u32;
        let iwram = (iwram_sum / WINDOWS as u64) as u32;
        if iwram == 0 {
            return None;
        }
        Some(Readings {
            rom,
            iwram,
            ratio_x1000: (u64::from(rom) * 1000 / u64::from(iwram)) as u32,
            rom_per_add_x1000: (u64::from(rom) * 1000 / u64::from(ADDS_PER_CALL)) as u32,
            iwram_per_add_x1000: (u64::from(iwram) * 1000 / u64::from(ADDS_PER_CALL)) as u32,
        })
    }

    /// `x.yyy` from a value scaled by 1000, without floating point.
    fn fixed3(value: u32) -> String {
        format!("{}.{:03}", value / 1000, value % 1000)
    }

    /// Log one state on both channels and append its compact screen lines.
    /// Three lines per state so both prefetch states fit the 240x160 screen.
    fn report(screen: &mut String, tag: &str, readings: &Readings) {
        agb::println!(
            "*** e5 state {:?}: rom={} iwram={} ratio_x1000={} rom_per_add_x1000={} iwram_per_add_x1000={}",
            tag,
            readings.rom,
            readings.iwram,
            readings.ratio_x1000,
            readings.rom_per_add_x1000,
            readings.iwram_per_add_x1000
        );
        screen.push_str(&format!(
            "{} ROM {} IWRAM {}\n",
            tag, readings.rom, readings.iwram
        ));
        screen.push_str(&format!("{} ratio {}\n", tag, fixed3(readings.ratio_x1000)));
        screen.push_str(&format!(
            "{} add {}/{}\n",
            tag,
            fixed3(readings.rom_per_add_x1000),
            fixed3(readings.iwram_per_add_x1000)
        ));
    }

    #[agb::entry]
    fn main(mut gba: agb::Gba) -> ! {
        let mut gfx = gba.graphics.get();
        gfx.set_background_palette_colour(0, 0, COLOR_PENDING);

        let boot = timer::waitcnt();
        agb::println!(
            "*** e5 boot waitcnt=0x{:04X} prefetch={}",
            boot,
            timer::prefetch_enabled(boot)
        );
        agb::println!("*** e5 body adds_per_call={}", ADDS_PER_CALL);

        let mut timers = gba.timers.timers();
        let mut counter = Counter::start(&mut timers);

        let prefetch_on = boot | (1 << 14);
        let prefetch_off = boot & !(1 << 14);

        let on = measure_state(&mut counter, prefetch_on);
        let off = measure_state(&mut counter, prefetch_off);

        // Restore the boot configuration before anything else reads it.
        timer::set_waitcnt(boot);

        let mut ok = on.is_some() && off.is_some();
        let mut screen = String::new();
        screen.push_str("E5 FETCH ARBITRATION\n");
        screen.push_str(&format!("WAITCNT 0x{:04X}\n", boot));

        match &on {
            Some(readings) => report(&mut screen, "ON", readings),
            None => {
                ok = false;
                screen.push_str("PREFETCH ON: VOID\n");
            }
        }
        match &off {
            Some(readings) => report(&mut screen, "OFF", readings),
            None => {
                ok = false;
                screen.push_str("PREFETCH OFF: VOID\n");
            }
        }
        screen.push_str(if ok { "PASS" } else { "HARNESS FAILED" });

        agb::println!(
            "*** e5 verdict: {}",
            if ok { "HARNESS OK" } else { "HARNESS FAILED" }
        );

        text::show(&mut gfx, &screen, if ok { COLOR_PASS } else { COLOR_FAIL });

        loop {}
    }
}
