//! One frame's bits: the frame header, then a band stream per channel.
//!
//! The header order is the decoder's (`uli-lite/src/frame.rs:312-323`): the `g`
//! delta as Exp-Golomb of a zigzagged difference against the previous frame's
//! `g`, then the single M/S flag. `g` is stateful across frames, which is why
//! the writer takes `g_prev` by reference — the seek table and track entries
//! have to store exactly this value.
//!
//! S1 knows two band streams, both with no codec maths in them:
//!
//! * [`FrameKind::Silence`] — the class-context table's EOB symbol immediately,
//!   so every band stays zero;
//! * [`FrameKind::DcImpulse`] — one PAIR-class symbol pair in band 0, i.e. one
//!   non-zero coefficient at index 0. Its *amplitude* is E2's quantiser; here
//!   the magnitude is the smallest the pair arm can carry, and the step index is
//!   chosen so that the pulse is actually observable after synthesis.

use uli_lite::format::{CT_PAIR, CT_ZERO, NBANDS};

use crate::bitwriter::{BitWriter, eg_bits, zz};
use crate::quant::{Band, Chan};
use crate::tables_default::DefaultTables;

/// Which band stream to write.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameKind<'a> {
    Silence,
    DcImpulse,
    /// Real content: one [`Chan`] per channel, in the order the reader consumes
    /// them (left first, then right, matching the M/S convention in `frame.rs`).
    Coded(&'a [Chan; 2]),
}

/// The PAIR class the DC pulse is coded in: class 3 of the default row, the
/// first PAIR class, with `rbits = 0`.
pub const DC_CLASS: usize = 3;

/// The step index DC is coded at. The pulse's magnitude is 1 pair digit, so at
/// `g = 0` the dequantised coefficient is `16 - 3 = 13` and every output sample
/// rounds to zero — the frame would decode correctly and prove nothing. At
/// `g = 160`, `STEP[160] = 2^20`, the coefficient is ~2^24, well under `XLIM`
/// (2^28) and comfortably above the i16 accumulator's rounding.
pub const DC_G: i32 = 160;

/// Write one frame and advance `g_prev`.
pub fn write_frame(
    bw: &mut BitWriter,
    tables: &DefaultTables,
    g_prev: &mut i32,
    g: i32,
    ms: u32,
    kind: FrameKind,
) {
    bw.put_eg(zz(g - *g_prev), 0);
    *g_prev = g;
    bw.put(ms, 1);

    match kind {
        FrameKind::Silence => {
            for _ in 0..2 {
                // The initial context is k (a sentinel past the real classes), so
                // cb[k] carries the first band symbol. Symbol k is immediate EOB.
                put_symbol(bw, tables, tables.k, tables.k);
            }
        }
        FrameKind::Coded(chans) => {
            for chan in chans {
                write_chan(bw, tables, chan);
            }
        }
        FrameKind::DcImpulse => {
            assert!(
                tables.ctype[DC_CLASS] == CT_PAIR && tables.rbits[DC_CLASS] == 0,
                "the DC frame is written against the default tables' class {DC_CLASS} \
                 (PAIR, rbits 0)"
            );
            for _ in 0..2 {
                // Band 0 is a coded PAIR band: eight symbols of two nibbles each.
                put_symbol(bw, tables, tables.k, DC_CLASS);
                // Symbol 16 = nibbles (1, 0): coefficient 0 becomes 1 (+ sign bit
                // below), coefficient 1 stays zero.
                put_class_symbol(bw, tables, DC_CLASS, 16);
                bw.put(0, 1);
                for _ in 0..7 {
                    put_class_symbol(bw, tables, DC_CLASS, 0);
                }
                // Termination after band 0. The context is now the coded class.
                put_symbol(bw, tables, DC_CLASS, tables.k);
            }
        }
    }
}

/// Write one channel's band stream: a class symbol per band from the context
/// table, that band's coefficients if its class codes anything, then the EOB
/// terminator from the class of the last band written.
///
/// The context is the previous *coded class*, not the previous band index, and it
/// starts at `k`. That is what makes the stream position-dependent, and it is why
/// a band's class must be recorded rather than recomputed: the reader has no way
/// to know a class until it has read the symbol the previous class coded it with.
pub fn write_chan(bw: &mut BitWriter, tables: &DefaultTables, chan: &Chan) {
    let k = tables.k;
    let mut ctx = k;
    for band in &chan.bands {
        put_symbol(bw, tables, ctx, band.class);
        ctx = band.class;
        if tables.ctype[band.class] == CT_ZERO {
            continue;
        }
        write_band_coeffs(bw, tables, band);
    }
    // The terminator is conditional, and this is S2's sharpest edge. The reader
    // loops `for b in 0..NBANDS` and returns when it *reads* the EOB symbol — so a
    // channel that codes all `NBANDS` bands never gives it the chance to: the loop
    // is already finished. Emitting the symbol anyway puts three bits in front of
    // the *next channel*, which reads them as its first class — as EOB — and
    // decodes a silently empty channel: the frame parses, "succeeds", and half its
    // spectrum is gone. `enc_core.c` guards the same thing with `if (endB < nb)`.
    if chan.bands.len() < NBANDS {
        put_symbol(bw, tables, ctx, k);
    }
}

/// The 16 coefficients of one coded band: `BANDW / 2` pair symbols or `BANDW / 4`
/// quad symbols, and after each symbol the per-coefficient tails in reader order.
///
/// The split between symbol and tail is the whole design of the class system: the
/// tuple symbol carries a *clipped* magnitude (nibble 15, digit 2) for every
/// coefficient in the tuple, and the escapes, raw bits and signs are then read one
/// coefficient at a time. Writing them grouped any other way desynchronises the
/// stream silently, because nothing in the bitstream marks the boundary.
fn write_band_coeffs(bw: &mut BitWriter, tables: &DefaultTables, band: &Band) {
    let class = band.class;
    let r = tables.rbits[class] as u32;
    let esck = tables.esck[class] as u32;
    let per = if tables.ctype[class] == CT_PAIR { 2 } else { 4 };

    for group in band.m.chunks_exact(per) {
        if per == 2 {
            let nib = [
                head_of(tables, class, group[0]),
                head_of(tables, class, group[1]),
            ];
            put_class_symbol(bw, tables, class, (nib[0] as usize) * 16 + nib[1] as usize);
        } else {
            let dg = [
                head_of(tables, class, group[0]),
                head_of(tables, class, group[1]),
                head_of(tables, class, group[2]),
                head_of(tables, class, group[3]),
            ];
            put_class_symbol(
                bw,
                tables,
                class,
                27 * dg[0] as usize + 9 * dg[1] as usize + 3 * dg[2] as usize + dg[3] as usize,
            );
        }
        for &v in group {
            write_coeff_tail(bw, tables, class, v, r, esck);
        }
    }
}

/// The part of a magnitude that rides in the tuple symbol: `m >> rbits` for PAIR
/// (clipped at the escape nibble) and `min(m, 2)` for QUAD.
fn head_of(tables: &DefaultTables, class: usize, m: i32) -> u32 {
    let a = m.unsigned_abs();
    if tables.ctype[class] == CT_PAIR {
        (a >> tables.rbits[class]).min(15)
    } else {
        a.min(2)
    }
}

/// One coefficient's tails, in the order the reader takes them: escape, raw
/// field, sign. `r == 0` means the class has no raw field at all, and the sign
/// bit exists only for a nonzero magnitude — the reader's `mm != 0 && get(1)`
/// short-circuits, so writing one for a zero coefficient shifts the whole frame.
fn write_coeff_tail(
    bw: &mut BitWriter,
    tables: &DefaultTables,
    class: usize,
    m: i32,
    r: u32,
    esck: u32,
) {
    let a = m.unsigned_abs();
    if tables.ctype[class] == CT_PAIR {
        let h = a >> r;
        if h >= 15 {
            bw.put_eg(h - 15, esck);
        }
        if r != 0 {
            bw.put(a & ((1u32 << r) - 1), r);
        }
    } else if a >= 2 {
        bw.put_eg(a - 2, esck);
    }
    if a != 0 {
        bw.put(u32::from(m < 0), 1);
    }
}

/// The exact bit count [`write_chan`] produces for this channel.
pub fn chan_bits(tables: &DefaultTables, chan: &Chan) -> u64 {
    let mut bw = BitWriter::new();
    write_chan(&mut bw, tables, chan);
    bw.pos()
}

/// Bits an escape costs, for the rate accounting that keeps `Plan` honest.
pub fn escape_bits(m: i32, r: u32, esck: u32, is_pair: bool) -> u32 {
    let a = m.unsigned_abs();
    if is_pair {
        let h = a >> r;
        if h >= 15 { eg_bits(h - 15, esck) } else { 0 }
    } else if a >= 2 {
        eg_bits(a - 2, esck)
    } else {
        0
    }
}

/// The exact bit count [`write_frame`] produces for these arguments.
pub fn frame_bits(tables: &DefaultTables, g_prev: i32, g: i32, ms: u32, kind: FrameKind) -> u64 {
    let mut bw = BitWriter::new();
    let mut gp = g_prev;
    write_frame(&mut bw, tables, &mut gp, g, ms, kind);
    bw.pos()
}

fn put_symbol(bw: &mut BitWriter, tables: &DefaultTables, context: usize, symbol: usize) {
    let codes = &tables.cb[context];
    let len = codes.symbol_len(symbol);
    assert!(
        len > 0,
        "context table {context} has no code for symbol {symbol}; writing nothing would \
         silently desynchronise the stream"
    );
    bw.put(codes.symbol_code(symbol), len as u32);
}

fn put_class_symbol(bw: &mut BitWriter, tables: &DefaultTables, class: usize, symbol: usize) {
    let codes = &tables.cls[class];
    let len = codes.symbol_len(symbol);
    assert!(
        len > 0,
        "class table {class} has no code for symbol {symbol}; writing nothing would \
         silently desynchronise the stream"
    );
    bw.put(codes.symbol_code(symbol), len as u32);
}
