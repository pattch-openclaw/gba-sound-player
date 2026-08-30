//! symphonia-bundle-flac compile probe — EXPECTED TO FAIL COMPILATION.
//!
//! See ../../FLAC.md for the full evaluation. This file records the intended
//! integration shape (decode FLAC from memory-mapped ROM into the agb mixer)
//! so the compile failures are the *crate's*, not the probe's.
//!
//! Failure chain (first abort; depends on cargo build order, both reproduce):
//!   symphonia-bundle-flac → symphonia-core → lazy_static 1.5.0
//!     lazy_static: `extern crate std;` → E0463 can't find crate for `std`
//!   symphonia-bundle-flac → symphonia-core → num-complex → num-traits 0.2.19
//!     num-traits: `extern crate std;` → E0463 can't find crate for `std`
//!
//! Even if lazy_static were feature-gated away, symphonia-core still has
//! hard `std` blockers (Error::IoError(std::io::Error), std::io Read/Seek
//! reader stack, std::collections::HashMap in the codec registry) — see FLAC.md.

#![no_std]
#![no_main]

use agb::display::Rgb;
use agb::sound::mixer::Frequency;

// Force the crate under test to compile. Deliberately no specific API use so
// any errors reported belong to symphonia itself, not to this probe.
use symphonia_bundle_flac as _flac_probe;

const TEAL: agb::display::Rgb15 = Rgb::new(0, 128, 128).to_rgb15();

// In a working world: FLAC bytes embedded in the ROM (cartridge ROM is
// memory-mapped and seekable — no filesystem needed for a MediaSource).
// static TRACK: &[u8] = include_bytes!("../../assets/sound/probe.flac");

#[agb::entry]
fn main(mut gba: agb::Gba) -> ! {
    agb::eprintln!("[flac_probe] entry started");

    let mut gfx = gba.graphics.get();
    gfx.set_background_palette_colour(0, 0, TEAL);

    // Integration shape we would build once decoding compiles:
    //   1. symphonia bundle_flac::FlacReader over a cursor into TRACK
    //   2. decode frames → interleaved i16 samples
    //   3. feed agb Mixer (see examples/pcm_playback.rs for the mixer half)
    let mut mixer = gba.mixer.mixer(Frequency::Hz32768);

    agb::eprintln!("[flac_probe] compiled?! (this was expected to fail)");

    loop {
        let frame = gfx.frame();
        mixer.frame();
        frame.commit();
    }
}
