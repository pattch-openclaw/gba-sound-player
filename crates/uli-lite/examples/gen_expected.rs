//! Generate the ULI2 fixture's expected-vector file from a ULA2 album.
//!
//! This replaces the previous `gen_expected.c` helper. It uses the portable
//! `uli-lite` decoder (the crate under test), so it is a **maintenance /
//! snapshot** tool, not an independent oracle. The committed
//! `tests/fixtures/uli2_tiny_expected.txt` remains the frozen reference
//! originally pinned against the C implementation; do not regenerate it merely
//! to make a failing decoder test pass.
//!
//! Usage:
//!   cargo run --manifest-path crates/uli-lite/Cargo.toml \
//!     --target <host-triple> --example gen_expected -- \
//!     crates/uli-lite/tests/fixtures/uli2_tiny.uli \
//!     crates/uli-lite/tests/fixtures/uli2_tiny_expected.txt

use std::env;
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::process;

use uli_lite::{Album, Decoder, DecoderState, Ops};

const M: usize = 512;

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
        h = fnv_bytes(h, &v.to_le_bytes());
    }
    for &v in r {
        h = fnv_bytes(h, &v.to_le_bytes());
    }
    h
}

fn usage() -> String {
    "usage: gen_expected <album.uli> <out.txt>".to_string()
}

fn main() {
    if let Err(e) = run() {
        eprintln!("gen_expected: {e}");
        eprintln!("{}", usage());
        process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut args = env::args().skip(1);
    let album_path = args.next().ok_or_else(usage)?;
    let out_path = args.next().ok_or_else(usage)?;
    if args.next().is_some() {
        return Err("too many arguments".to_string());
    }

    let blob = fs::read(&album_path).map_err(|e| format!("read {album_path}: {e}"))?;
    let album = Album::parse(&blob).map_err(|e| format!("album parse: {e:?}"))?;
    let mut state = Box::new(DecoderState::new());
    let mut decoder = Decoder::new(album, &mut state).map_err(|e| format!("tables: {e:?}"))?;

    let mut out =
        BufWriter::new(File::create(&out_path).map_err(|e| format!("create {out_path}: {e}"))?);

    writeln!(
        out,
        "# frame int8_fnv int16_fnv bits bitpos bandsyms coded_bands pairs quads nz zc raws escs ms long_codes cycles xover hfnz"
    )
    .map_err(write_err)?;
    let h = album.header();
    writeln!(
        out,
        "# header frames {} samples {} tracks {} k {} max_cycles {} mean_cycles {}",
        h.n_frames,
        h.n_samples,
        h.n_tracks,
        decoder.state().tables.k,
        h.est_max_cycles,
        h.est_mean_cycles
    )
    .map_err(write_err)?;

    let mut l8 = [0i8; M];
    let mut r8 = [0i8; M];
    let mut l16 = [0i16; M];
    let mut r16 = [0i16; M];
    let mut all8 = 2_166_136_261u32;
    let mut all16 = 2_166_136_261u32;

    for frame in 0..album.n_frames() {
        let info = decoder
            .decode_frame(&mut l8, &mut r8, Some(&mut l16), Some(&mut r16))
            .map_err(|e| format!("decode frame {frame}: {e:?}"))?;
        let h8 = fnv_i8(fnv_i8(2_166_136_261, &l8, &[]), &[], &r8);
        let h16 = fnv_i16(fnv_i16(2_166_136_261, &l16, &[]), &[], &r16);
        all8 = fnv_bytes(all8, &h8.to_le_bytes());
        all16 = fnv_bytes(all16, &h16.to_le_bytes());

        let Ops {
            bandsyms,
            coded_bands,
            pairs,
            quads,
            nz,
            zc,
            raws,
            escs,
            ms,
            long_codes,
        } = info.ops;
        writeln!(
            out,
            "{} {:08x} {:08x} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {}",
            frame,
            h8,
            h16,
            info.bits,
            info.bitpos,
            bandsyms,
            coded_bands,
            pairs,
            quads,
            nz,
            zc,
            raws,
            escs,
            ms,
            long_codes,
            info.cycles,
            decoder.state().xover(),
            decoder.state().hfnz()
        )
        .map_err(write_err)?;
    }

    writeln!(out, "# all8 {:08x} all16 {:08x}", all8, all16).map_err(write_err)?;
    out.flush().map_err(|e| format!("flush {out_path}: {e}"))?;
    Ok(())
}

fn write_err(e: std::io::Error) -> String {
    format!("write: {e}")
}
