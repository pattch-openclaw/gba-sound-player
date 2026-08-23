#![no_std]
#![no_main]

use agb::display::font::{Font, Layout, LayoutSettings, RegularBackgroundTextRenderer};
use agb::display::tiled::{RegularBackground, RegularBackgroundSize, TileFormat};
use agb::display::{Palette16, Priority, Rgb15};
use agb::include_font;

/// Font asset (Shiny Ars / ark-pixel, SIL OFL).
static FONT: Font = include_font!("assets/fonts/ark-pixel-10px-proportional-latin.ttf", 10);

/// Simple two-colour palette: black background, white text.
static PALETTE: &Palette16 = {
    let mut palette = [Rgb15::BLACK; 16];
    palette[1] = Rgb15::WHITE;
    &Palette16::new(palette)
};

#[agb::entry]
fn entry(mut gba: agb::Gba) -> ! {
    // --- Graphics: render a title screen ---
    let mut gfx = gba.graphics.get();
    gfx.set_background_palette(0, PALETTE);

    let mut bg = RegularBackground::new(
        Priority::P0,
        RegularBackgroundSize::Background32x32,
        TileFormat::FourBpp,
    );

    let layout = Layout::new(
        "gba-audio\nPSG tone: ON\n(~731 Hz square,\n2 s loop)",
        &FONT,
        &LayoutSettings::new().with_max_line_length(200),
    );
    let mut text_renderer = RegularBackgroundTextRenderer::new((4, 4), 1);
    for group in layout {
        text_renderer.show(&mut bg, &group);
    }

    // --- Audio: play a tone on PSG channel 1 ---
    psg::enable_master_sound();
    psg::play_tone(2_000_000); // ~2 s of spin-loop iterations

    // --- Render once and hold the frame ---
    let mut frame = gfx.frame();
    bg.show(&mut frame);
    frame.commit();

    loop {
        core::hint::spin_loop();
    }
}

/// Minimal direct hardware access to GBA SOUND1 (PSG channel 1).
///
/// The GBA ARM7 runs at ~16.777 MHz. SOUND1's frequency register uses:
///     f = 16_777_216 / (2048 - x),  x ∈ [0, 2047]
///
/// 440 Hz is *not* reachable (needs x ≈ 2224, out of range), so we pick the
/// closest achievable note: x = 1819 → ~731 Hz (D5).
mod psg {
    // GBA SOUND1 (PSG channel 1) registers
    const SOUND1_SWEEP: *mut u16 = 0x0400_00A8 as *mut u16;
    const SOUND1_FREQ:  *mut u16 = 0x0400_00AA as *mut u16;
    const SOUND1_VOL:   *mut u16 = 0x0400_00AC as *mut u16;
    const SOUNDCNT_L:   *mut u16 = 0x0400_0080 as *mut u16;
    const SOUNDCNT_H:   *mut u16 = 0x0400_0084 as *mut u16;

    /// Enable master sound output (SOUNDCNT_H bit 7).
    pub fn enable_master_sound() {
        unsafe {
            let h = SOUNDCNT_H.read_volatile();
            SOUNDCNT_H.write_volatile(h | (1 << 7));
        }
    }

    /// Play a ~731 Hz square wave on PSG channel 1 for `cycles` loop iterations.
    pub fn play_tone(cycles: u32) {
        unsafe {
            // Disable sweep (constant frequency)
            SOUND1_SWEEP.write_volatile(0);

            // x = 1819 → f ≈ 731 Hz
            SOUND1_FREQ.write_volatile(1819u16 << 8);

            // Volume 7/7, duty cycle 50%
            SOUND1_VOL.write_volatile(0x77);

            // Enable channel 1 (bit 0), route to L (bit 8) and R (bit 9)
            let l = SOUNDCNT_L.read_volatile();
            SOUNDCNT_L.write_volatile(l | (1 << 0) | (1 << 8) | (1 << 9));

            // Spin for ~2 seconds
            let mut i = 0;
            while i < cycles {
                i = i.wrapping_add(1);
                core::hint::spin_loop();
            }

            // Mute channel 1
            SOUNDCNT_L.write_volatile(SOUNDCNT_L.read_volatile() & !(1 << 0));
        }
    }
}
