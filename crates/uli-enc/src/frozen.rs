//! Coding a frame at a **held-still** plan: the Rust half of S8-1's probe.
//!
//! S7's matrix could only compare whole blobs, so a difference was localised to a region
//! and never to a cause — and S8-1 built the missing instrument on the *reference* side
//! (`scripts/uli_probe.py`, which drives the oracle's own `frame_code` at a frozen
//! `(g, ms, tables)`). This module is the other half: the same coding, from this crate, at
//! the same frozen plan, reported in the same units, so the two can be diffed by
//! `scripts/uli_frame_diff.py`.
//!
//! It deliberately exposes no search. `rd::measure` is the one function that codes a frame
//! at a given step, and everything here does is call it and then write the bytes it
//! measured — which is the property being tested: that what we *measure* and what we
//! *emit* are the same coding.

use uli_lite::format::{M, NSTEP};

use crate::Error;
use crate::bitwriter::BitWriter;
use crate::frame::{self, FrameKind};
use crate::rd;
use crate::tables_default::DefaultTables;

/// One frame coded at a frozen `(g, ms)`, in the units S8-1's probe reports.
#[derive(Clone, Debug)]
pub struct FrameDump {
    /// Bits the frame occupies — **including the `ms` bit, excluding the `g` delta**. That
    /// split is the reference's `FrameRes.bits` convention, and it is not arbitrary: the
    /// `g` delta depends on the previous frame's step, so it belongs to the sequence rather
    /// than to this frame. Matching the convention is what makes the numbers comparable.
    pub bits: u64,
    /// Squared error of the reconstruction, in analysis units — the same domain the
    /// reference reports its `D` in.
    pub dist: f64,
    /// The reader's own cycle model for this frame.
    pub cycles: u32,
    /// The frame's bits as they go on the wire: the `g` delta from `g_prev`, the `ms` bit,
    /// then the bands. The last halfword may be partially filled; `stream_bits` says how
    /// many bits are meaningful.
    pub stream: Vec<u16>,
    /// Bits written to `stream`.
    pub stream_bits: u64,
    /// The reader-operation counts the cycle model is a function of. Reported because the
    /// model has twelve weighted terms, and "our estimate is 0.5% high" cannot say which one
    /// moved; the counts can (S8's inventory row 14).
    pub ops: uli_lite::Ops,
}

/// Code one frame at a frozen `(g, ms, mu, tables)`.
///
/// `g_prev` is the previous frame's step, so the emitted stream carries the same `g` delta
/// the reference emits for the same predecessor (`--g-prev` on the probe). Pass `0` for the
/// first frame of a corpus.
///
/// Refuses a step the reader cannot express: the step index is `g - 4*ms`, and a `g` outside
/// that window would be arithmetic no decoder can undo — which is the same refusal the
/// probe makes, so a disagreement about *legality* cannot masquerade as a disagreement
/// about coding.
pub fn code_at(
    tables: &DefaultTables,
    coef: &[f32; 2 * M],
    g: i32,
    ms: u32,
    kappa: f64,
    mu: f64,
    g_prev: i32,
) -> Result<FrameDump, Error> {
    let idx = g - 4 * ms as i32;
    if !(0..NSTEP as i32).contains(&idx) {
        return Err(Error::Plan(format!(
            "g = {g} with ms = {ms} is step index {idx}, outside the reader's 0..{}",
            NSTEP - 1
        )));
    }
    if ms > 1 {
        return Err(Error::Plan(format!("ms = {ms} is not a flag (0 or 1)")));
    }

    // One measurement, then one write of exactly what was measured.
    let (point, chans) = rd::measure(coef, tables, g, ms != 0, kappa, mu)?;

    let mut bw = BitWriter::new();
    let mut prev = g_prev;
    frame::write_frame(&mut bw, tables, &mut prev, g, ms, FrameKind::Coded(&chans));
    // Recomputed from the same `chans` the writer was handed, so the counts describe what
    // was emitted rather than what was measured.
    let ops = crate::rd::frame_ops(tables, &chans, ms != 0);

    Ok(FrameDump {
        // `measure` counts the bands only — its own comment explains why the `g` delta is
        // the sequence's business and the `ms` bit the flag's. The reference counts the
        // `ms` bit in and the delta out, so this adds exactly one and nothing else.
        bits: point.bits + 1,
        dist: point.dist,
        cycles: point.cycles,
        stream: bw.halfwords().to_vec(),
        stream_bits: bw.pos(),
        ops,
    })
}
