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

use uli_lite::format::CT_PAIR;

use crate::bitwriter::{BitWriter, zz};
use crate::tables_default::DefaultTables;

/// Which band stream to write.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameKind {
    Silence,
    DcImpulse,
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
