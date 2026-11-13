//! ULI2 Phase 1: decode-only ROM.
//!
//! This image embeds the committed Phase 0 corpus, parses its ULA2 container,
//! decodes every frame through the portable `uli-lite` core, and checks the
//! album-wide int8 and int16 FNV pins that the C reference produced. It is
//! deliberately not a player: no DMA, no FIFO, no mixer, no ARM kernel. Its
//! job is to make the portable decoder run on the target before the overlay
//! and playback work begin.
//!
//! The decoder state is a `.ewram` static (roughly 100 KiB, dominated by the
//! 10-bit Huffman LUTs). The portable transform/lapping buffers inside it are
//! not yet pinned to IWRAM; that is the overlay's job once the cycle budget is
//! measured.

#![no_std]
#![no_main]

use agb::display::Rgb;
use uli_lite::{Album, Decoder, DecoderState};

/// The committed Phase 0 corpus, byte-for-byte.
static ALBUM: &[u8] = include_bytes!("../assets/uli2_tiny.uli");

/// Caller-owned decoder state. On the GBA this belongs in EWRAM;
/// `DecoderState::new()` is `const`, so the linker places and initialises it
/// without a large runtime stack construction.
#[unsafe(link_section = ".ewram")]
static mut STATE: DecoderState = DecoderState::new();

/// FNV-1a 32 of the C reference's per-frame int8 hashes, in frame order.
const EXPECTED_INT8: u32 = 0xe6e8_7652;
/// FNV-1a 32 of the C reference's per-frame int16 hashes, in frame order.
const EXPECTED_INT16: u32 = 0xb353_0978;

const COLOR_PENDING: agb::display::Rgb15 = Rgb::new(128, 0, 255).to_rgb15();
const COLOR_PASS: agb::display::Rgb15 = Rgb::new(0, 0, 255).to_rgb15();
const COLOR_FAIL: agb::display::Rgb15 = Rgb::new(255, 0, 0).to_rgb15();

fn fnv_bytes(mut h: u32, bytes: &[u8]) -> u32 {
    for &b in bytes {
        h ^= b as u32;
        h = h.wrapping_mul(16_777_619);
    }
    h
}

fn fnv_i8(mut h: u32, l: &[i8], r: &[i8]) -> u32 {
    for &v in l {
        h ^= v as u8 as u32;
        h = h.wrapping_mul(16_777_619);
    }
    for &v in r {
        h ^= v as u8 as u32;
        h = h.wrapping_mul(16_777_619);
    }
    h
}

fn fnv_i16(mut h: u32, l: &[i16], r: &[i16]) -> u32 {
    for &v in l {
        for b in v.to_le_bytes() {
            h ^= b as u32;
            h = h.wrapping_mul(16_777_619);
        }
    }
    for &v in r {
        for b in v.to_le_bytes() {
            h ^= b as u32;
            h = h.wrapping_mul(16_777_619);
        }
    }
    h
}

#[agb::entry]
fn main(mut gba: agb::Gba) -> ! {
    let mut gfx = gba.graphics.get();
    gfx.set_background_palette_colour(0, 0, COLOR_PENDING);
    agb::println!("[uli-spike] ULI2 Phase 1 decode-only ROM");
    agb::println!("[uli-spike] blob: {} bytes", ALBUM.len());

    let album = match Album::parse(ALBUM) {
        Ok(a) => a,
        Err(e) => {
            agb::println!("[uli-spike] FAIL album parse: {:?}", e);
            gfx.set_background_palette_colour(0, 0, COLOR_FAIL);
            loop {
                let frame = gfx.frame();
                frame.commit();
            }
        }
    };

    let state = unsafe { &mut *core::ptr::addr_of_mut!(STATE) };
    let mut decoder = match Decoder::new(album, state) {
        Ok(d) => d,
        Err(e) => {
            agb::println!("[uli-spike] FAIL tables: {:?}", e);
            gfx.set_background_palette_colour(0, 0, COLOR_FAIL);
            loop {
                let frame = gfx.frame();
                frame.commit();
            }
        }
    };
    agb::println!(
        "[uli-spike] frames={} tracks={} tables_k={} stream={} halfwords",
        album.n_frames(),
        album.n_tracks(),
        decoder.state().tables.k,
        album.stream().len() / 2
    );

    let mut l8 = [0i8; 512];
    let mut r8 = [0i8; 512];
    let mut l16 = [0i16; 512];
    let mut r16 = [0i16; 512];
    let mut all8 = 2_166_136_261u32;
    let mut all16 = 2_166_136_261u32;
    let mut frames = 0u32;

    for _ in 0..album.n_frames() {
        if let Err(e) = decoder.decode_frame(&mut l8, &mut r8, Some(&mut l16), Some(&mut r16)) {
            agb::println!("[uli-spike] FAIL decode frame {}: {:?}", frames, e);
            gfx.set_background_palette_colour(0, 0, COLOR_FAIL);
            loop {
                let frame = gfx.frame();
                frame.commit();
            }
        }
        let frame8 = fnv_i8(2_166_136_261, &l8, &r8);
        let frame16 = fnv_i16(2_166_136_261, &l16, &r16);
        all8 = fnv_bytes(all8, &frame8.to_le_bytes());
        all16 = fnv_bytes(all16, &frame16.to_le_bytes());
        frames += 1;
    }

    let diag_ok = decoder.state().xover() == 0 && decoder.state().hfnz() == 0;
    let pass =
        frames == album.n_frames() && all8 == EXPECTED_INT8 && all16 == EXPECTED_INT16 && diag_ok;

    agb::println!(
        "[uli-spike] frames={} int8={:08x} (want {:08x}) int16={:08x} (want {:08x}) xover={} hfnz={}",
        frames,
        all8,
        EXPECTED_INT8,
        all16,
        EXPECTED_INT16,
        decoder.state().xover(),
        decoder.state().hfnz()
    );
    agb::println!(
        "[uli-spike] {}",
        if pass { "PASS" } else { "FAIL: pin mismatch" }
    );
    gfx.set_background_palette_colour(0, 0, if pass { COLOR_PASS } else { COLOR_FAIL });

    loop {
        let frame = gfx.frame();
        frame.commit();
    }
}
