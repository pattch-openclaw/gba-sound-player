//! Experimental FLAC integration ROM — sanity build for `flac-lite` on target.
//!
//! PURPOSE (see ../../../../FLAC.md): prove that our minimal `no_std` FLAC
//! decoder *bundles* into a real GBA ROM — compiles, links (rust-lld + gba.ld),
//! gets fixed by agb-gbafix, and boots — for `thumbv4t-none-eabi`. It is the
//! build-process counterpart to `make flac-test` (library correct in isolation):
//! together they separate "decoder is wrong" from "decoder doesn't fit the ROM".
//!
//! STATUS: flac-lite's *decoder* is still scaffold (`todo!()` bodies), so this
//! ROM never runs decode code — the link anchor below forces the decoder into
//! the image (so the sanity check is real: code size, linking, and `no_std`
//! compliance are all exercised). What this ROM *does* run is a live
//! **BitReader proof**: a hard-coded byte sequence embedded in the ROM image,
//! read back through `flac_lite::BitReader` in the representative field-read
//! pattern the decoder will use. Every expected value is logged next to the
//! value actually read over the serial log, and the screen reports the verdict
//! visually: **blue = every field matched, red = mismatch** (purple = never
//! ran / aborted). That works on emulator *and* flash cart, so the host-only
//! unit tests in `bits.rs` have an on-hardware counterpart.
//!
//! Once decoding lands, replace the anchor with a real `Manifest::parse` +
//! `Decoder` loop over a packed `.gfp` clip; the BitReader proof stays as the
//! boot-time hardware sanity check.

#![no_std]
#![no_main]

use agb::display::Rgb;
use flac_lite::bits::BitReader;
use flac_lite::{Decoder, Error, FrameBuffers, Manifest};

const COLOR_PENDING: agb::display::Rgb15 = Rgb::new(128, 0, 255).to_rgb15(); // Purple = FLAC experiment, not yet verified
const COLOR_PASS: agb::display::Rgb15 = Rgb::new(0, 0, 255).to_rgb15(); // Blue = BitReader proof passed on this hardware
const COLOR_FAIL: agb::display::Rgb15 = Rgb::new(255, 0, 0).to_rgb15(); // Red = a field read mismatched its expectation

/// Hard-coded test vector: the exact bytes the BitReader proof reads back on
/// hardware. Lives in `.text` (the ROM image), so real hardware reads the same
/// bytes the emulator does — no host-side state involved.
///
/// Chosen so the representative field widths (fixed-width FLAC header-style
/// fields + a UTF-8-style prefix + an unaligned 16-bit window crossing byte
/// boundaries) land on awkward bit offsets, and the expectations below were
/// computed by hand from the bit layout — deliberately NOT by reusing the
/// library code under test.
const BIT_PROOF_BYTES: [u8; 10] = [0x66, 0x4C, 0x61, 0x43, 0x7A, 0xE1, 0xB5, 0x2C, 0xDE, 0xAD];

/// One checked field read: (label, bit width, hand-computed expected value).
/// The widths mirror how the decoder actually consumes a stream: a run of
/// fixed-width header fields, a wide unaligned read, and a final byte-ish read.
const BIT_PROOF_FIELDS: [(&str, u32, u32); 10] = [
    ("sync_like_14", 14, 0x1993), // bits 0..14  = 01100110010011
    ("reserved_1", 1, 0),         // bit  14     = 0
    ("blocking_1", 1, 0),         // bit  15     = 0
    ("sample_size_4", 4, 0x6),    // bits 16..20 = 0110
    ("sample_rate_4", 4, 0x1),    // bits 20..24 = 0001
    ("framing_1", 1, 0),          // bit  24     = 0
    ("channel_8", 8, 0x86),       // bits 25..33 = 10000110 (crosses bytes 3/4)
    ("utf8_like_13", 13, 0x1EB8), // bits 33..46 = 1111010111000 (crosses 4/5)
    ("window_16", 16, 0x6D4B),    // bits 46..62 = 0110110101001011 (unaligned)
    ("crc_like_8", 8, 0x37),      // bits 62..70 = 00110111 (crosses 7/8)
];

/// Write the hard-coded byte sequence to the serial log, hex, MSB-first.
fn log_proof_bytes() {
    agb::println!("[flac-rom] BitReader proof: hard-coded ROM bytes (LSB..MSB):");
    for (i, b) in BIT_PROOF_BYTES.iter().enumerate() {
        agb::println!("[flac-rom]   byte[{}] = 0x{:02X}", i, b);
    }
}

/// Run the on-hardware BitReader proof and log expected-vs-actual for every
/// field. Returns true only if every field read exactly as expected, the
/// cursor finished on the hand-computed boundary, and the tail check read the
/// expected last bits.
fn bitreader_proof() -> bool {
    log_proof_bytes();

    let mut reader = BitReader::new(&BIT_PROOF_BYTES);
    let mut pass = true;

    for (label, width, expected) in BIT_PROOF_FIELDS {
        match reader.read_bits(width) {
            Ok(actual) => {
                let ok = actual == expected;
                if !ok {
                    pass = false;
                }
                agb::println!(
                    "[flac-rom] {} ({} bits): expected 0x{:X} actual 0x{:X} [{}]",
                    label,
                    width,
                    expected,
                    actual,
                    if ok { "OK" } else { "MISMATCH" }
                );
            }
            Err(e) => {
                pass = false;
                agb::println!(
                    "[flac-rom] {} ({} bits): expected 0x{:X} actual <error {:?}> [MISMATCH]",
                    label,
                    width,
                    expected,
                    e
                );
            }
        }
    }

    // Cursor bookkeeping: hand-computed total of the field widths above.
    const EXPECTED_END_BITS: usize = 70;
    let pos = reader.bit_position();
    let pos_ok = pos == EXPECTED_END_BITS;
    let rem = reader.bits_remaining();
    const EXPECTED_REMAINING: usize = (BIT_PROOF_BYTES.len() * 8) - EXPECTED_END_BITS;
    let rem_ok = rem == EXPECTED_REMAINING;
    pass &= pos_ok & rem_ok;
    agb::println!(
        "[flac-rom] cursor: expected pos={} remaining={} actual pos={} remaining={} [{}]",
        EXPECTED_END_BITS,
        EXPECTED_REMAINING,
        pos,
        rem,
        if pos_ok && rem_ok { "OK" } else { "MISMATCH" }
    );

    // One final read consumes the remaining 10 bits (0b10_1010_1101): the EOF
    // path stays honest — after this, `read_bits(1)` must fail.
    match reader.read_bits(10) {
        Ok(actual) => {
            const EXPECTED_TAIL: u32 = 0x2AD; // bits 70..80 of the vector
            let ok = actual == EXPECTED_TAIL;
            pass &= ok;
            agb::println!(
                "[flac-rom] tail_10 (10 bits): expected 0x{:X} actual 0x{:X} [{}]",
                EXPECTED_TAIL,
                actual,
                if ok { "OK" } else { "MISMATCH" }
            );
        }
        Err(e) => {
            pass = false;
            agb::println!(
                "[flac-rom] tail_10: expected 0x2AD actual <error {:?}> [MISMATCH]",
                e
            );
        }
    }

    let eof_ok = reader.read_bits(1).is_err();
    pass &= eof_ok;
    agb::println!(
        "[flac-rom] read at EOF: expected Err actual {} [{}]",
        if eof_ok { "Err" } else { "Ok(?!)" },
        if eof_ok { "OK" } else { "MISMATCH" }
    );

    pass
}

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
    gfx.set_background_palette_colour(0, 0, COLOR_PENDING);
    agb::eprintln!("[flac-rom] screen should now be purple (proof pending)");

    // Presence check only: the anchor is linked but deliberately not called.
    agb::eprintln!(
        "[flac-rom] flac-lite linked into ROM image (anchor at {:p})",
        &FLAC_LINK_ANCHOR as *const _
    );

    // The proof itself: reads the hard-coded ROM bytes through BitReader and
    // logs every expected-vs-actual pair. Screen colour reports the verdict so
    // it is verifiable on real hardware with no cable attached.
    let pass = bitreader_proof();
    let (color, label) = if pass {
        (COLOR_PASS, "BLUE")
    } else {
        (COLOR_FAIL, "RED")
    };
    gfx.set_background_palette_colour(0, 0, color);
    agb::eprintln!(
        "[flac-rom] BitReader proof {} — screen {}",
        if pass { "PASSED" } else { "FAILED" },
        label
    );

    loop {
        let frame = gfx.frame();
        frame.commit();
    }
}
