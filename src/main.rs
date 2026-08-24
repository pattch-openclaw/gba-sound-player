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
    // --- Audio: 440 Hz (A4) square wave on PSG channel 1 ---
    //
    // agb 0.25 has no PSG (channel 1-4) API — its `mixer` only drives the PCM
    // channels (5-6) via DMA. So we drive SOUND1 directly through raw MMIO.
    // This is safe alongside `#[agb::entry]`: agb never touches PSG channels
    // 1-4, so there is no framework conflict. We keep letting agb own
    // everything else (display, DMA, timing).
    psg::play_tone_a4();

    // --- Graphics: render a title screen ---
    let mut gfx = gba.graphics.get();
    gfx.set_background_palette(0, PALETTE);

    let mut bg = RegularBackground::new(
        Priority::P0,
        RegularBackgroundSize::Background32x32,
        TileFormat::FourBpp,
    );

    let layout = Layout::new(
        "gba-sound-player\nPSG tone: ON\n(440 Hz square,\ninfinite loop)",
        &FONT,
        &LayoutSettings::new().with_max_line_length(200),
    );
    let mut text_renderer = RegularBackgroundTextRenderer::new((4, 4), 1);
    for group in layout {
        text_renderer.show(&mut bg, &group);
    }

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
/// The GBA ARM7 runs at 16,777,216 Hz. SOUND1's frequency register uses:
///     f = 16_777_216 / (2048 - x),  x ∈ [0, 2047]
///
/// For 440 Hz we need x ≈ 1758. The exact 440 Hz is not representable;
/// x = 1758 → ~440.17 Hz, x = 1757 → ~440.60 Hz. We pick x = 1758 (closest).
///
/// NOTE: this module exists as a stepping stone toward a dedicated
/// audio-library crate. It currently lives inline in this crate.
mod psg {
    // GBA SOUND1 (PSG channel 1) registers
    const SOUND1_SWEEP: *mut u16 = 0x0400_00A8 as *mut u16;
    const SOUND1_FREQ:  *mut u16 = 0x0400_00AA as *mut u16;
    const SOUND1_VOL:   *mut u16 = 0x0400_00AC as *mut u16;
    const SOUNDCNT_L:   *mut u16 = 0x0400_0080 as *mut u16;
    const SOUNDCNT_H:   *mut u16 = 0x0400_0084 as *mut u16;

    /// Enable master sound output (SOUNDCNT_H bit 7) and sound bias
    /// (SOUNDCNT_H bit 14) so the PSG channels can actually produce output.
    fn enable_master_sound() {
        unsafe {
            let h = SOUNDCNT_H.read_volatile();
            SOUNDCNT_H.write_volatile(h | (1 << 7) | (1 << 14));
        }
    }

    /// Play a 440 Hz (A4) square wave on PSG channel 1, 50% duty, vol 15/15.
    /// Leaves the channel enabled so it loops indefinitely.
    pub fn play_tone_a4() {
        unsafe {
            // Disable sweep (constant frequency)
            SOUND1_SWEEP.write_volatile(0);

            // x = 1758 → f ≈ 440.17 Hz (closest representable to 440 Hz)
            SOUND1_FREQ.write_volatile(1758u16 << 8);

            // Volume 15/15, duty cycle 50% (bit 6..5 = 0b01)
            SOUND1_VOL.write_volatile(0x77);

            // Enable channel 1 (bit 0), route to L (bit 8) and R (bit 9)
            let l = SOUNDCNT_L.read_volatile();
            SOUNDCNT_L.write_volatile(l | (1 << 0) | (1 << 8) | (1 << 9));

            enable_master_sound();
        }
    }
}
