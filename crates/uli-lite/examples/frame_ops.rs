//! Print the per-frame operation counts of a ULA2 album from the portable
//! decoder.
//!
//! This is the instrument for attributing the *entropy* half of a frame
//! (PLAN-OF-RECORD.md, "Step 0b — entropy pre-analysis"): symbol counts are a
//! property of the bitstream, not of the image that decodes it, so they are
//! read here on the host rather than from a `verify` emulator build. Do not
//! derive cycles from this tool — it counts operations, not time; pair its rows
//! with a timed arm from `examples/uli_bench` (`make uli-attrib`).
//!
//! Columns: `frame bits huff band pair quad nz zc esc raw long ms`, where
//! `huff` is the number of Huffman symbols the frame reads — one band-class
//! symbol per band plus one magnitude symbol per pair/quad group, i.e.
//! `band + pair/2 + quad/4` — and `long` is how many of those take the
//! `huff_slow` path (a code longer than the 10-bit LUT).
//!
//! Usage:
//!   cargo run --manifest-path crates/uli-lite/Cargo.toml \
//!     --target <host-triple> --example frame_ops -- <album.uli> [frame ...]
//!
//! With no frame arguments it prints every frame and a short summary naming the
//! frames with the most bits, symbols and nonzero coefficients.

use std::env;
use std::fs;
use std::process;

use uli_lite::format::M;
use uli_lite::{Album, Decoder, DecoderState, Ops};

struct Row {
    frame: u32,
    bits: u32,
    huff: u32,
    ops: Ops,
}

fn arg_or_die<T>(r: Result<T, String>) -> T {
    match r {
        Ok(v) => v,
        Err(e) => {
            eprintln!("frame_ops: {e}");
            process::exit(1);
        }
    }
}

fn main() {
    let mut args = env::args().skip(1);
    let path = match args.next() {
        Some(p) => p,
        None => {
            eprintln!("usage: frame_ops <album.uli> [frame ...]");
            process::exit(2);
        }
    };
    let want: Vec<u32> = args.filter_map(|a| a.parse().ok()).collect();

    let blob = arg_or_die(fs::read(&path).map_err(|e| format!("{path}: {e}")));
    let album = arg_or_die(Album::parse(&blob).map_err(|e| format!("{path}: {e:?}")));

    let mut state = DecoderState::new();
    let mut dec =
        arg_or_die(Decoder::new(album, &mut state).map_err(|e| format!("{path}: tables: {e:?}")));

    let (mut l8, mut r8) = ([0i8; M], [0i8; M]);
    let (mut l16, mut r16) = ([0i16; M], [0i16; M]);
    let mut rows = Vec::with_capacity(album.n_frames() as usize);
    for _ in 0..album.n_frames() {
        let info = arg_or_die(
            dec.decode_frame(&mut l8, &mut r8, Some(&mut l16), Some(&mut r16))
                .map_err(|e| format!("frame decode: {e:?}")),
        );
        let ops = info.ops;
        rows.push(Row {
            frame: info.frame,
            bits: info.bits,
            huff: ops.bandsyms + ops.pairs / 2 + ops.quads / 4,
            ops,
        });
    }

    println!("frame bits huff band pair quad nz zc esc raw long ms");
    for r in &rows {
        if !want.is_empty() && !want.contains(&r.frame) {
            continue;
        }
        let o = &r.ops;
        println!(
            "{} {} {} {} {} {} {} {} {} {} {} {}",
            r.frame,
            r.bits,
            r.huff,
            o.bandsyms,
            o.pairs,
            o.quads,
            o.nz,
            o.zc,
            o.escs,
            o.raws,
            o.long_codes,
            o.ms
        );
    }
    if !want.is_empty() {
        return;
    }

    let most_bits = rows.iter().max_by_key(|r| r.bits);
    let most_syms = rows.iter().max_by_key(|r| r.huff);
    let most_nz = rows.iter().max_by_key(|r| r.ops.nz);
    println!();
    println!("frames={}", rows.len());
    if let Some(r) = most_bits {
        println!("most bits: frame {} ({} bits)", r.frame, r.bits);
    }
    if let Some(r) = most_syms {
        println!(
            "most symbols: frame {} ({} symbols, {} long codes = {}%)",
            r.frame,
            r.huff,
            r.ops.long_codes,
            r.ops.long_codes.saturating_mul(100) / r.huff.max(1)
        );
    }
    if let Some(r) = most_nz {
        println!(
            "most nonzero coefficients: frame {} ({} of {} slots)",
            r.frame,
            r.ops.nz,
            r.ops.nz + r.ops.zc
        );
    }
}
