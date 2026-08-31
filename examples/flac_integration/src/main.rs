//! Experimental FLAC integration ROM — sanity build for `flac-lite` on target.
//!
//! PURPOSE (see ../../../../FLAC.md): prove that our minimal `no_std` FLAC
//! decoder *bundles* into a real GBA ROM — compiles, links (rust-lld + gba.ld),
//! gets fixed by agb-gbafix, and boots — for `thumbv4t-none-eabi`. It is the
//! build-process counterpart to `make flac-test` (library correct in isolation):
//! together they separate "decoder is wrong" from "decoder doesn't fit the ROM".
//!
//! STATUS: flac-lite is scaffold (`todo!()` bodies). This ROM therefore NEVER
//! executes decoder code — the link anchor below forces the decoder into the
//! image (so the sanity check is real: code size, linking, and `no_std`
//! compliance are all exercised) while the running ROM just shows a colour and
//! logs, like the baseline. Once decoding lands, replace the anchor with a real
//! `Manifest::parse` + `Decoder` loop over a packed `.gfp` clip.

#![no_std]
#![no_main]

use agb::display::Rgb;
use flac_lite::{Decoder, Error, FrameBuffers, Manifest};

const BACKGROUND_COLOR: agb::display::Rgb15 = Rgb::new(128, 0, 255).to_rgb15(); // Purple = FLAC experiment

/// Link anchor: reference decoder machinery without ever running it.
///
/// `#[used]` + a `static` holding a fn pointer keeps `flac_lite`'s code and
/// data in the linked image (the linker cannot dead-strip it), but nothing in
/// the entry path calls it — so the scaffold's `todo!()` panics can never fire
/// on hardware/emulator.
#[used]
static FLAC_LINK_ANCHOR: fn(&[u8]) -> Result<(), Error> = flac_smoke;

fn flac_smoke(blob: &[u8]) -> Result<(), Error> {
    // Type-level exercise of the public API surface (never invoked at runtime).
    let manifest = Manifest::parse(blob)?;
    let mut decoder = Decoder::new(&manifest)?;
    let _ = (
        manifest.sample_rate_hz(),
        manifest.blocksize(),
        manifest.channels(),
        manifest.frame_count(),
    );
    let mut scratch = [0i32; 2048 * 2];
    let (left, right) = scratch.split_at_mut(2048);
    let mut bufs = FrameBuffers::new(left, right);
    let _ = decoder.decode_next(&mut bufs);
    Ok(())
}

#[agb::entry]
fn main(mut gba: agb::Gba) -> ! {
    agb::eprintln!("[flac-rom] entry started — flac-lite integration sanity build");

    let mut gfx = gba.graphics.get();
    gfx.set_background_palette_colour(0, 0, BACKGROUND_COLOR);
    agb::eprintln!("[flac-rom] screen should now be purple");

    // Presence check only: the anchor is linked but deliberately not called.
    agb::eprintln!(
        "[flac-rom] flac-lite linked into ROM image (anchor at {:p})",
        &FLAC_LINK_ANCHOR as *const _
    );

    loop {
        let frame = gfx.frame();
        frame.commit();
    }
}
