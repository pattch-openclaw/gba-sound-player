//! MSB-first bit reader over an immutable byte slice.
//!
//! STATUS (2026-09-10): **the read path is complete** — `new`, `read_bits`,
//! `peek_bits`, `read_signed`, `read_utf8_coded`, `byte_align`, `read_u8`,
//! `read_wasted_bits`, `bit_position`, `bits_remaining` (Phase 1 step 1,
//! complete in PRs #25–#32; `read_wasted_bits` landed with step 3a as the
//! subframe body's wasted-bits reader), and the CRC helpers `crc8`/`crc16`
//! (Phase 2 step 5, landed 2026-11-03 — table-free; *verification* is gated
//! behind the `crc-check` feature, the release path stays check-free by
//! design).
//!
//! STATUS (2026-11-05): **the window-domain bridge is present and inert** —
//! [`rev32`] plus the hand-out/take-back pair [`BitReader::kernel_window`] /
//! [`BitReader::set_kernel_window`] (split-ladder item 5, PR 1). **Nothing in
//! the decode path calls it**, and PR 2 (2026-11-05, the seam itself) did not
//! change that — deliberately: PR 2's rule is zero behavior change to any
//! decode output, and the core's kernel is the incumbent loop, which reads
//! through the reader's own primitives. PR 1's note said "PR 2 is the PR that
//! does", and that expectation was wrong about its own successor, so it is
//! corrected here rather than left to mislead: the window-domain callers are
//! PR 3's host-side twin (whose kernel state *is* the `w`/`v`/`ptr`/`end` set
//! this type carries) and item 6's `flac-lite-gba` overlay kernel. The reason
//! this PR stood alone still holds — the one assumption item 5 rests on had to
//! be witnessed on its own before any generic, trait, or measurement stood on
//! it — and it now stands under the seam, in `residual`'s module docs and on
//! the `RiceKernel` trait.
//!
//! The roadmap itself lives in FLAC.md → "Phased plan" (single source of truth).
//!
//! FLAC packs its fields MSB-first across byte boundaries, so the whole decoder
//! is built on this one primitive. Design notes for the implementation:
//!
//! - **Refill-accumulator design (E2a, landed alongside E2b):** `acc` is a
//!   left-aligned, MSB-first `u32` window (next bit = bit 31) with invariant
//!   `logical cursor = pos * 8 − valid`. Refill is byte-at-a-time (`ldrb`, no
//!   alignment assumptions) while `valid <= 24`. Fast-path reads (1..=31 bits
//!   when `valid >= n`) and `rice_quotient`'s 8-bit table lookups operate
//!   directly on `acc` in registers; 32-bit reads and cold-path reads fall back
//!   to `peek_at` at the derived logical cursor and resync the window.
//! - ARMv4T (ARM7TDMI) has **no CLZ and no hardware divide** — prefer
//!   shift/add and `& 7` masks over anything multiplicative. Nothing here
//!   divides or modulo-divides by a variable.
//! - Signed fields (LP coefficients, residuals, verbatim samples) are
//!   two's-complement in the field's stated width: sign-extend after reading.
//! - **The seam shape is whole-partition, and this module owns its window
//!   bridge** (split-ladder item 5, PR 1, 2026-11-05). The shape is recorded
//!   here rather than argued because a type now carries it, and the two facts
//!   in this tree that settled it are both cited by name: (1) FLAC.md's E10
//!   entry — the probe "composes the hybrid as a single whole-partition
//!   kernel rather than calling the track's word-in/word-out kernels: the
//!   seam needs the byte-window ABI", so `arm_rice_partition` re-embodies the
//!   tier map around the reader; (2) the per-sample kernel says the same from
//!   its own side — `examples/arm-kernels/src/sample_step.rs`: "Deliberately
//!   NOT here: any rev8 reversal or w_{t+1} straddle merge. Stitching the
//!   reversed-window domain to spec-order streams is the probe's vendored
//!   seam", and that kernel's committed table pins `q = 31` vectors with
//!   expectation `rem = 0` *precisely* to convict a merge-or-reversal reading
//!   of it. So the stitching is the core's job — this module's — and the
//!   stitching is nothing new: it is this reader's own invariant restated in
//!   the other domain, which is what [`KernelWindow`]'s and [`rev32`]'s docs
//!   spell out.
//! - Verbatim / raw-signature subframes and metadata DIDs are byte-aligned:
//!   use [`BitReader::byte_align`], never assume alignment.

use crate::{Error, Result};

/// Octet count of the coded-number form a lead octet starts, per
/// RFC 9639 §9.1.5 Table 18 — or `None` when the byte cannot *start* a coded
/// number (`0x80..=0xBF` are continuation octets; `0xFF` is the single
/// all-ones lead the extended UTF-8 coding never uses).
///
/// This is the *lead* half of Table 18. The other half — the value range each
/// form carries — is enforced inside [`BitReader::coded_number_inner`] (a value
/// below its form's minimum is a non-minimal form, rejected there).
///
/// Public because header width has to be *computable*, not only read:
///
/// * [`crate::frame`]'s header parse cross-checks its own consumed length
///   against this table (see that module's docs) instead of trusting the
///   cursor alone.
/// * A frame-scan candidate filter needs the form width *before* committing to
///   a read: with the worst case known (7 octets — Table 18's own cap) the
///   widest possible frame header is a stated number of bytes, which is both
///   the scan window bound and the `crc-check` coverage bound.
pub fn coded_number_octets(lead: u8) -> Option<usize> {
    match lead {
        0x00..=0x7F => Some(1), // 0xxxxxxx — single octet, whole value
        0x80..=0xBF => None,    // 10xxxxxx — continuation, cannot lead
        0xC0..=0xDF => Some(2), // 110xxxxx
        0xE0..=0xEF => Some(3), // 1110xxxx
        0xF0..=0xF7 => Some(4), // 11110xxx
        0xF8..=0xFB => Some(5), // 111110xx
        0xFC..=0xFD => Some(6), // 1111110x
        0xFE => Some(7),        // 11111110 — zero payload bits in the lead
        0xFF => None,           // 11111111 — unused by the coding
    }
}

/// Smallest value whose **minimal** coded-number form has N octets, indexed by
/// [`coded_number_octets`]'s result — RFC 9639 Table 18's left column, i.e. one
/// past the previous form's maximum (`0x80 = 2^7`, `0x800 = 2^11`,
/// `0x1_0000 = 2^16`, `0x20_0000 = 2^21`, …).
///
/// Only the *lower* bound is checked, and that is complete rather than
/// unfinished: each form's maximum is exactly what its own payload bits can
/// hold — the widths in the code block on [`BitReader::read_utf8_coded`] are
/// Table 18's right column — so the accumulation in
/// [`BitReader::coded_number_inner`] cannot exceed it. Rows 6 and 7 carry the
/// RFC's one real asymmetry: the 6-octet form tops out at 31 bits while the
/// 7-octet form *starts* at 32, so no value can want both, and row 7 is
/// reachable at all only because a 32-bit value does not fit in 6 octets.
const CODED_NUMBER_FORM_MIN: [u32; 8] = [
    0, // row 0 is unreachable: no form has zero octets
    0,
    0x80,
    0x800,
    0x1_0000,
    0x20_0000,
    0x400_0000,
    0x8000_0000,
];

/// A cursor-based, borrow-only bit reader.
///
/// Zero-cost to construct, zero allocation, `Copy`-ish for cheap save/restore in
/// tests. There is no `Seek` — the cursor only moves forward.
#[derive(Clone, Copy, Debug)]
pub struct BitReader<'a> {
    /// Backing bytes (normally a subslice of a `&'static` ROM blob).
    data: &'a [u8],
    /// Bytes consumed — the byte just past the last fully buffered bit.
    /// Invariant: `pos <= data.len()`.
    pos: usize,
    /// Bits buffered, MSB-first: the next bit to consume is bit 31. The
    /// low `32 − valid` bits of `acc` are always zero.
    /// Invariant: `logical cursor = pos * 8 − valid`.
    acc: u32,
    /// Valid bits in `acc` (0..=32).
    valid: u32,
}

impl<'a> BitReader<'a> {
    /// Wrap a byte slice for bit-level reading.
    pub fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            pos: 0,
            acc: 0,
            valid: 0,
        }
    }

    /// Refill **byte-at-a-time** while `valid <= 24` (after one byte
    /// `valid` lands in 25..=32, so at most one byte load feeds one fast
    /// read). The append at bit `24 − valid` (shift 0..=24) moves `pos`
    /// and `valid` together: cursor-neutral by construction.
    #[inline]
    fn refill(&mut self) {
        while self.valid <= 24 && self.pos < self.data.len() {
            let byte = u32::from(self.data[self.pos]);
            self.acc |= byte << (24 - self.valid);
            self.pos += 1;
            self.valid += 8;
        }
    }

    /// Consume `n` buffered bits (`n <= valid`, caller-guaranteed).
    #[inline]
    fn consume(&mut self, n: u32) {
        self.acc <<= n;
        self.valid -= n;
    }

    /// Read `n` bits (1..=32) as an unsigned value, MSB-first.
    ///
    /// Returns [`Error::EndOfStream`] if fewer than `n` bits remain (cursor
    /// unchanged), and [`Error::InvalidField`] for `n == 0` or `n > 32`, which
    /// are caller bugs rather than stream conditions.
    #[inline]
    pub fn read_bits(&mut self, n: u32) -> Result<u32> {
        self.refill();
        if n >= 1 && n <= 31 && self.valid >= n {
            // acc is zero below bit (32 − valid) ≤ (32 − n), so the
            // extract takes exactly the next n bits, MSB-first.
            let val = self.acc >> (32 - n);
            self.acc <<= n;
            self.valid -= n;
            return Ok(val);
        }
        let val = self.peek_at(self.bit_position(), n)?;
        // Resync the window to sit just past the consumed field. Validated
        // (peek_at succeeded), so the cursor arithmetic stays in range.
        let take = n - self.valid; // n > valid guaranteed by the guard above
        self.valid = 0;
        self.acc = 0;
        self.pos += (take >> 3) as usize;
        let rem = take & 7;
        if rem != 0 {
            let partial = u32::from(self.data.get(self.pos).copied().unwrap_or(0)) << (24 + rem);
            self.acc = partial;
            self.valid = 8 - rem;
            self.pos += 1;
        }
        Ok(val)
    }

    /// Peek at the next `n` bits (1..=32) without advancing the cursor.
    ///
    /// Same width and end-of-stream semantics as [`Self::read_bits`] — same
    /// error variants, cursor untouched on success *and* failure.
    pub fn peek_bits(&self, n: u32) -> Result<u32> {
        if n >= 1 && n <= 32 && self.valid >= n {
            return Ok(self.acc >> (32 - n));
        }
        self.peek_at(self.bit_position(), n)
    }

    /// Read `n` bits (1..=32) as a two's-complement signed value.
    ///
    /// Same width/EOF semantics as [`Self::read_bits`] (delegates to it, so
    /// validation and cursor rules are identical by construction). FLAC
    /// encodes LP coefficients, Rice-partition samples, verbatim samples, and
    /// unaligned warm-up values as raw two's-complement in the field's stated
    /// width — sign extension is pure post-processing.
    ///
    /// Sign extension is the classic **shl-then-arithmetic-shr** idiom: move
    /// the field's sign bit up into bit 31, then let `>>` on `i32` (an
    /// arithmetic shift on ARM, `asr`) drag the sign down through the high
    /// half. No divide, no subtract, and it needs **no special case for
    /// `n == 32`** — there the shifts are both by 0, which Rust defines, and
    /// the result is just the `as i32` reinterpretation.
    pub fn read_signed(&mut self, n: u32) -> Result<i32> {
        let raw = self.read_bits(n)?;
        // n in 1..=32, so both shift amounts are in 0..=31 (never >= 32).
        let shift = 32 - n;
        Ok(((raw << shift) as i32) >> shift)
    }

    /// Read FLAC's UTF-8-style **coded number** (RFC 9639 §9.1.5): the
    /// variable-length prefix code used for frame/sample numbers. FLAC reuses
    /// the UTF-8 shape but extends it to 36-bit values / 7-byte forms —
    /// general-purpose UTF-8 decoders cannot parse these, so this is
    /// deliberately its own reader.
    ///
    /// Leading one-bits of the first byte set the total length; the first
    /// byte's remaining payload bits are the value's top bits, and each
    /// following byte contributes 6 (`10xxxxxx` continuation) bits, MSB-first:
    ///
    /// ```text
    /// 0xxxxxxx                                    → 1 byte,  7 payload bits
    /// 110xxxxx 10xxxxxx                           → 2 bytes, 11 bits
    /// 1110xxxx 10xxxxxx 10xxxxxx                  → 3 bytes, 16 bits
    /// 11110xxx  …                                 → 4 bytes, 21 bits
    /// 111110xx  …                                 → 5 bytes, 26 bits
    /// 1111110x  …                                 → 6 bytes, 31 bits
    /// 11111110 10xxxxxx ×6                        → 7 bytes, 36 bits
    /// ```
    ///
    /// Errors ([`Error::InvalidField`]): a stray continuation lead
    /// (`0b10xxxxxx`, prefix == 1) and an all-ones first byte (`0xFF`,
    /// prefix == 8) are not legal leads; any non-`0b10xxxxxx` continuation
    /// byte is likewise rejected. (The pre-RFC spec's "doubled prefix"
    /// warning applied to `0xFE`, which RFC 9639 legalizes as the 7-byte
    /// lead — only `0xFF` remains invalid.)
    ///
    /// This is a *bit-level* decoder: it enforces neither canonical (shortest)
    /// form nor stream-level meaning. It **does** reject the non-minimal forms
    /// ([`Error::InvalidField`]) — a value that would fit in a shorter form but
    /// is written in a longer one, i.e. UTF-8's overlongs (`C0 80` for 0,
    /// `C1 BF` for 0x7F) and, in Table 18's terms, a value below its form's
    /// minimum.
    ///
    /// This rule has been documented wrong here once and then *measured*
    /// wrong once more, so the whole trace is kept.
    ///
    /// The first version of this doc said the reference decoder accepts
    /// overlongs. A follow-up "measurement" reported the opposite: a real
    /// stream whose frame number `0` was rewritten from `00` to `C0 80`, its
    /// header CRC-8 recomputed, failing `flac -d` with `BAD_HEADER` at 0
    /// samples. The command was real; what it measured was not the form. The
    /// frame footer's CRC-16 covers the frame HEADER too, not just the body
    /// (measured per fixture by the vector generator: `crc16(header+body)`
    /// equals the stored footer, `crc16(body)` does not), so *any* change to a
    /// header's bytes breaks its frame — and a stale header CRC is what yields
    /// `BAD_HEADER`.
    ///
    /// With **both** CRCs recomputed over the widened header, the reference
    /// decoder accepts the form: `flac -d` 1.5.0 decodes a real stream whose
    /// frame number 0 is spelled `C0 80`, `E0 80 80`, or `F0 80 80 80`, MD5
    /// check included. Those streams are generated, not hand-written, and their
    /// exit statuses are pinned as the `mutation number-form-overlong` rows of
    /// `tests/stream_vectors.txt` (`scripts/gen_stream_vectors.sh`). A malformed
    /// continuation sequence — `C0 80 80`, a 2-octet lead with a stray third
    /// byte — *is* rejected by the reference decoder as `BAD_HEADER`; the
    /// structural rules above are shared, minimality is not.
    ///
    /// So the rule here is a deliberate divergence, and a cheap one: libFLAC
    /// never *writes* a non-minimal form (the generator asserts that census per
    /// fixture — `frame_number_forms_all_minimal`), and keeping it preserves the
    /// one-to-one relation between a value and its form width, which is what
    /// `frame::MAX_ACCEPTED_HEADER_BITS` is derived from (a seven-octet lead can
    /// only be a frame number if overlongs are admitted).
    ///
    /// What it is NOT, measured: a reduction in false frame starts. On these
    /// five fixtures `scan_frame_starts_strict` equals
    /// `scan_frame_starts_all_forms_allowed` — the rule removes zero candidate
    /// frame starts from real audio bytes. The scan safety the frame layer has
    /// comes from the field validation and the header CRC-8, not from this
    /// rule; the argument for the rule is canonicity (Table 18 gives each value
    /// exactly one row), consistent with reading the rest of §9.1 by its
    /// structural rules rather than by the reference decoder's tolerance.
    ///
    /// Stream semantics (numbers must match the frame count / be strictly
    /// increasing) still belong to the frame layer, where those invariants are
    /// actually checkable.
    ///
    /// Cursor is **atomic**: a multi-byte read that fails partway (bad lead,
    /// bad continuation, EOF) restores the cursor exactly as it was — the
    /// composite of several `read_bits` calls, rolled back on error.
    pub fn read_utf8_coded(&mut self) -> Result<u64> {
        let start = (self.pos, self.acc, self.valid);
        match self.coded_number_inner() {
            Ok(v) => Ok(v),
            Err(e) => {
                // Atomicity across the whole composite read; the logical cursor
                // only ever moves forward, and a failed number read never moves
                // it at all.
                self.pos = start.0;
                self.acc = start.1;
                self.valid = start.2;
                Err(e)
            }
        }
    }

    /// Consume bits up to the next byte boundary. Returns the number of bits
    /// discarded (0..7).
    ///
    /// FLAC pads the end of a frame's subframe data to a byte boundary before
    /// the CRC-16 footer, and verbatim / raw-signature subframe samples start on
    /// one — both consumers call this first.
    ///
    /// Notably **not** a caller: the frame header's CRC-8. The header's fixed
    /// fields total **32** bits (sync 14 + reserved 1 + blocking 1 + blocksize 4
    /// + sample rate 4 + **channels 4** + sample size 3 + reserved 1), and every
    /// field before the CRC is whole octets, so the CRC-8 always lands on a byte
    /// boundary — bit 40 behind a 1-octet coded number, 48 behind 2, and still
    /// aligned after the optional uncommon blocksize/rate fields (8/16/24 bits).
    /// Aligning there is a no-op.
    ///
    /// Earlier revision of this comment claimed 31 fixed bits with a 3-bit
    /// `channels` field and concluded the header CRC was *never* byte-aligned,
    /// so aligning would drop the CRC's top bit. Both the width and the
    /// conclusion were wrong (measured over 979 real libFLAC frames; pinned by
    /// `tests/frame_header_layout.rs`). [`Self::read_u8`] stays bit-level for the
    /// case that genuinely needs it — the **footer** read below, right after
    /// subframe padding — not the header. FLAC.md → "Correction 1".
    ///
    /// **Infallible, and it cannot overrun the slice**: a slice's bit count is
    /// always a multiple of 8, so aligning any in-bounds cursor lands on a
    /// boundary that also fits. An already-aligned cursor discards 0 and stays
    /// put, which makes the call idempotent.
    ///
    /// Division-free per the module's ARMv4T rule: `(8 - (pos & 7)) & 7` is the
    /// distance to the next boundary with no modulo — an aligned position puts
    /// 8 in the subtraction and the mask folds it back to 0.
    pub fn byte_align(&mut self) -> u32 {
        let drop = self.valid & 7;
        self.acc <<= drop;
        self.valid &= !7;
        drop
    }

    /// Read a `u8` (8 bits) — the frame header's CRC-8 byte, the two bytes of a
    /// CRC-16 footer, and byte-aligned metadata.
    ///
    /// Delegates to [`Self::read_bits`], so EOF behaviour and cursor rules are
    /// identical **by construction** (the same delegation as
    /// [`Self::read_signed`]): a short read yields [`Error::EndOfStream`] and
    /// leaves the cursor untouched; `read_bits(8)` cannot return anything but
    /// the low 8 bits, so the `as u8` truncation is lossless.
    ///
    /// Like the rest of the module this is **bit-level**: it neither requires
    /// nor assumes a byte-aligned cursor. That is load-bearing, not laxness —
    /// the frame header's CRC-8 sits at bit `31 + 8k` (see
    /// [`Self::byte_align`]), so an alignment *requirement* would make the real
    /// header unreadable. Where a field genuinely is byte-aligned (padding
    /// before the CRC-16 footer) the caller contracts to call
    /// [`Self::byte_align`] first; the reader keeps no alignment invariant, so
    /// asserting one here could only ever be a tripwire, and the layer that can
    /// actually check alignment is the frame parser.
    pub fn read_u8(&mut self) -> Result<u8> {
        self.read_bits(8).map(|v| v as u8)
    }

    /// Read FLAC's wasted-bits-per-sample field (§9.2.2), returning `k`.
    ///
    /// Layout: a flag bit. `0` → `k = 0`, nothing follows. `1` → `k − 1`
    /// zero bits terminated by a one follow, so `k ≥ 1`. The caller must
    /// sit on the flag bit — exactly where
    /// [`crate::subframe::SubframeType::parse`] leaves the cursor (its 7-bit
    /// pad+type contract), witnessed against encoder bytes in
    /// `tests/subframe_header_layout.rs`.
    ///
    /// Deliberately **bit-level**: no interpretation of what `k` means. The
    /// §9.2.2 rule "resulting bits per sample MUST be larger than zero" is
    /// `k < frame bps`, which needs the frame's sample size — that check
    /// belongs to the subframe layer, not here (same separation as
    /// profile-gating everywhere else).
    ///
    /// Cursor is **atomic** like [`Self::read_utf8_coded`]: a run that
    /// reaches EOF unterminated restores the cursor to the flag bit and
    /// returns [`Error::EndOfStream`]; on success the cursor sits just past
    /// the terminating one. The run is bounded by the slice, so no separate
    /// length cap is imposed — an unbounded unary is exactly what EOF
    /// rejects.
    pub fn read_wasted_bits(&mut self) -> Result<u32> {
        let start = (self.pos, self.acc, self.valid);
        match self.wasted_inner() {
            Ok(k) => Ok(k),
            Err(e) => {
                self.pos = start.0;
                self.acc = start.1;
                self.valid = start.2;
                Err(e)
            }
        }
    }

    fn wasted_inner(&mut self) -> Result<u32> {
        if self.read_bits(1)? == 0 {
            return Ok(0);
        }
        // Flag set: count the unary zeros; the terminating one is consumed.
        // Failure inside the loop propagates through the atomic wrapper.
        // Deliberately still per-bit: the wasted-bits run is one field per
        // subframe, not the per-sample hot loop E2b measured — do not
        // "optimize" it without a probe of its own.
        let mut zeros: u32 = 0;
        loop {
            if self.read_bits(1)? == 1 {
                return Ok(zeros + 1);
            }
            zeros += 1;
        }
    }

    /// Read a Rice unary quotient (§9.2.7.2): count the zero bits until the
    /// terminating one (consumed), return the count.
    ///
    /// **Table count — E2b + E2a refill-accumulator.** The live hot loop this
    /// replaces (`while read_bits(1)? == 0` in
    /// [`crate::residual::decode_rice_partition`]) was E8's measured cost
    /// center for FIXED (unary 56.2% of gross); the E2b probe measured the
    /// table count riding E2a's `u32` refill-accumulator reader at −21.7% on
    /// the whole FIXED arm (FLAC.md "E2b result"). Each 8-bit window of the
    /// run is answered by one `ldrb` from [`LEADING_ONES`] on the inverted top
    /// byte of `self.acc` (`!(self.acc >> 24) as u8`; runs 0..=7 terminate
    /// inside, 8 means a whole byte of zeros — keep counting per byte, ~5 cyc
    /// per 8 bits instead of per bit), and the subsequent `read_bits(order)`
    /// remainder extract consumes directly from the same buffered `self.acc`
    /// window without re-gathering bytes via `peek_at`.
    ///
    /// Cursor: **atomic** like [`Self::read_utf8_coded`] — an unterminated
    /// run (bits run out before the terminator) restores the cursor to the
    /// codeword start and returns [`Error::EndOfStream`]. The per-bit loop
    /// it replaced left the cursor after the consumed zeros; the difference
    /// is observable only on a dead frame (the stream rule forbids
    /// unbounded runs and `decode_rice_partition`'s caller discards errored
    /// frames wholesale), and atomic is the stronger contract. This is the
    /// E2b sample's one deliberate divergence, carried with its scope note.
    pub fn rice_quotient(&mut self) -> Result<u32> {
        let snap = (self.pos, self.acc, self.valid);
        let mut quotient: u32 = 0;
        loop {
            if self.valid < 8 {
                self.refill();
            }
            if self.valid >= 8 {
                let inv8 = !(self.acc >> 24) as u8;
                let run = u32::from(LEADING_ONES[usize::from(inv8)]);
                if run < 8 {
                    // Terminating one inside this byte: consume run + 1.
                    self.consume(run + 1);
                    return Ok(quotient + run);
                }
                // Whole byte of unary zeros: account it, keep counting.
                quotient += 8;
                self.consume(8);
            } else {
                // Slice tail: 0 < valid < 8 bits remain (valid == 0 is
                // immediate EOF). The valid bits already sit at the top of
                // `acc` (left-aligned window); `!acc >> 24` inverts them in
                // place (bits 7..8−v) with inverted-zeros junk below, so
                // masking everything from bit 7−v down leaves exactly
                // [inverted valid bits][0 stopper][zeros]: the table count
                // is the real run when it terminates inside the tail, and
                // exactly `valid` when the run reaches end-of-slice — EOF.
                let v = self.valid;
                if v == 0 {
                    self.pos = snap.0;
                    self.acc = snap.1;
                    self.valid = snap.2;
                    return Err(Error::EndOfStream);
                }
                let inv8 = (!(self.acc >> 24) as u8) & (0xFFu8 << (8 - v));
                let run = u32::from(LEADING_ONES[usize::from(inv8)]);
                if run >= v {
                    // The run consumed every remaining bit with no real
                    // terminator: unterminated unary (masking guarantees
                    // run <= v, so this is run == v exactly).
                    self.pos = snap.0;
                    self.acc = snap.1;
                    self.valid = snap.2;
                    return Err(Error::EndOfStream);
                }
                self.consume(run + 1);
                return Ok(quotient + run);
            }
        }
    }

    /// Bits remaining in the backing slice from the current cursor.
    #[inline]
    pub fn bits_remaining(&self) -> usize {
        (self.data.len() << 3) - self.bit_position()
    }

    /// Current cursor position in bits (useful for frame-boundary assertions
    /// and for the CRC-8-over-header check, which needs the raw header bytes).
    #[inline]
    pub fn bit_position(&self) -> usize {
        self.pos * 8 - self.valid as usize
    }

    /// Immutable byte view `data[start_bit / 8 .. end_bit / 8]`; both bounds
    /// must be byte-aligned (frame boundaries always are) and in range,
    /// `None` otherwise. Never touches the cursor — this is a *view* over the
    /// backing slice, not a read, so `end_bit` may sit anywhere in range.
    ///
    /// This is the CRC-coverage view: both frame CRCs are defined over whole
    /// bytes of the frame (measured, Phase 2 step 5 — the header CRC-8 always
    /// lands byte-aligned, and the footer CRC-16 covers frame bytes up to the
    /// CRC field, padding bits included), so the whole-byte formulation here
    /// agrees with the bitwise definition for every frame boundary.
    pub fn bytes_between(&self, start_bit: usize, end_bit: usize) -> Option<&'a [u8]> {
        if start_bit > end_bit || start_bit % 8 != 0 || end_bit % 8 != 0 {
            return None;
        }
        self.data.get(start_bit / 8..end_bit / 8)
    }

    /// The whole backing region — the byte view [`KernelWindow::ptr`] and
    /// [`KernelWindow::end`] are offsets *into*, and the authority the
    /// take-back re-bounds against.
    ///
    /// This exists for window-domain kernels (split-ladder item 5): a
    /// [`crate::residual::RiceKernel`] that owns a whole partition walks bytes
    /// itself — tier 3's word-skip and the byte-merging refill — and needs the
    /// same view the window hands it to be able to bound those reads at all.
    /// [`KernelWindow::end`] gives the bound; this gives the bytes to index.
    /// The two are the same length by construction at hand-out
    /// (`kernel_window` sets `end = data.len()`), and a kernel must still use
    /// `end` rather than this slice's length if it wants the window's own
    /// contract rather than the reader's.
    ///
    /// Never touches the cursor, and borrows the region immutably for the
    /// reader's whole lifetime `'a`, so holding it across a read is fine —
    /// reads are `&mut self` over the *cursor*, not over the bytes.
    #[inline]
    pub fn region(&self) -> &'a [u8] {
        self.data
    }

    /// Hand out this reader's bit window in **the kernel's** domain — the
    /// hand-out half of the seam bridge (split-ladder item 5, PR 1; see
    /// [`KernelWindow`] and [`BitReader::set_kernel_window`] for the shape
    /// decision and the take-back half).
    ///
    /// The mapping is exact in both directions, and the two windows' shared
    /// invariants are *one* invariant stated twice:
    ///
    /// ```text
    /// bit i of w  ==  stream bit P + i  ==  bit 31 - i of acc
    ///                 i.e. w = rev32(acc),  P = bit_position()
    ///
    /// "the low 32 - valid bits of acc are zero"  ==  "bits >= v of w are zero"
    /// P + v = pos * 8                             ==  the kernel's byte-pointer
    ///                                               invariant at entry
    /// ```
    ///
    /// So a kernel holding this window is looking at exactly what this reader
    /// is looking at: the same next bit, the same valid count, the same byte
    /// cursor (`ptr`), bounded by the same end (`end`). This neither refills
    /// nor consumes nor moves the cursor — [`Self::refill`] and
    /// [`Self::consume`] stay the only code that touches `acc`/`pos`/`valid`.
    ///
    /// Lifted, not invented: `AccumulatorReader::rice_partition_kernel`
    /// (patch line 1205 of the committed
    /// `examples/flac_spike/experiments/e10-three-arm-probe/e10-three-arm-probe.patch`)
    /// builds exactly this window and resyncs with the same `rev32`; that
    /// probe is where the mapping was measured. The core's side of that seam
    /// is what lands here.
    #[inline]
    pub fn kernel_window(&self) -> KernelWindow {
        KernelWindow {
            w: rev32(self.acc),
            v: self.valid,
            ptr: self.pos,
            end: self.data.len(),
        }
    }

    /// Take a window back — the resync half of the bridge, and the only way
    /// a window gets back into the reader.
    ///
    /// For a window that still satisfies the invariants it was handed out
    /// with ("bits ≥ `v` of `w` are zero", `P + v = ptr * 8`) this is exact,
    /// *including* the cursor the kernel advanced: shifting `n` bits out of
    /// `w` and handing back puts `bit_position()` exactly `n` bits further on
    /// and leaves `acc` exactly what `n` [`Self::consume`] calls would have
    /// left. That is the property PR 2's whole-partition kernel will depend
    /// on, so it is witnessed here as a differential against the live reader
    /// (bit-at-a-time consumption), not asserted.
    ///
    /// **Total by design: no `Result`, no panic, no new error variant.** A
    /// kernel that hands back a dirty, over-wide, or out-of-range window is a
    /// kernel bug, and the three clamps below are what keep that bug
    /// observable as a wrong sample count rather than as a panic inside a
    /// decode path that has none. The alternatives considered and rejected on
    /// the record: returning `Result` (a per-partition error arm on the hot
    /// path PR 2 exists to protect, and a new variant for a condition no real
    /// stream can produce) and `debug_assert!` (the tests that witness these
    /// clamps run in debug, so the witness would panic). Every clamp is
    /// inert for a window this reader produced — the byte-pointer invariant
    /// alone makes `v <= ptr * 8` true for any cursor a legal walk reaches —
    /// and each one is pinned by name in the tests.
    #[inline]
    pub fn set_kernel_window(&mut self, k: KernelWindow) {
        // Bounded by THIS reader's slice, not by `k.end`: `end` is the bound
        // this reader handed out, and it is not the authority here — `self
        // .data` is. A window built against a shorter region cannot extend
        // this reader's, and a window that walked past its own end stops at
        // this one's.
        let pos = k.ptr.min(self.data.len());
        // The window is 32 bits wide, and the reader's own invariant
        // `valid <= pos * 8` has to hold for `bit_position()` to stay in
        // range (see the module's `assert_window` witness). Clamped in
        // `usize` because `pos * 8` is a `usize`; the result is at most 32,
        // so narrowing it back is exact, not a truncation.
        let valid = (k.v as usize).min(32).min(pos * 8) as u32;
        // Keep only the bits the window still counts as valid. Masking `w`
        // to its low `v` bits is the *same* operation, seen from the window
        // side, as "the low 32 - valid bits of acc are zero" seen from the
        // reader side: bit i of w maps to bit 31 - i of acc, so bits >= v of
        // w are exactly the low 32 - v bits of acc. Shift amount is in 0..=31
        // by the clamp above, so there is no shift-by-32 leg here.
        let mask = if valid >= 32 {
            u32::MAX
        } else {
            (1u32 << valid) - 1
        };
        self.pos = pos;
        self.valid = valid;
        self.acc = rev32(k.w & mask);
    }

    /// Read `n` bits (1..=32, MSB-first) starting at absolute bit position
    /// `pos`, without moving the cursor. This is the single place that touches
    /// the bytes; `read_bits` (and later `peek_bits`) layer on top of it.
    ///
    /// Layout: at most `((7 + 32) + 7) >> 3 == 5` bytes can be involved in one
    /// read, so accumulating them into a `u64` (≤ 40 bits) is safe and keeps
    /// every shift below 64 — in particular there is no shift-by-32 trap even
    /// though `n == 32` is legal.
    fn peek_at(&self, pos: usize, n: u32) -> Result<u32> {
        // Width outside 1..=32 is a caller bug; `InvalidField` is the closest
        // stream-agnostic variant the crate's flat error enum offers.
        if n == 0 || n > 32 {
            return Err(Error::InvalidField);
        }
        let total_bits = self.data.len() << 3;
        let end = pos
            .checked_add(n as usize)
            .filter(|&e| e <= total_bits)
            .ok_or(Error::EndOfStream)?;

        let byte_idx = pos >> 3;
        let off = (pos & 7) as u32; // bit offset within the first byte, 0..=7
        // Bytes spanned by bits [pos, end): division-free ceil((off + n) / 8).
        let bytes_needed = (((off + n + 7) >> 3) as usize).min(self.data.len()); // <= 5

        // Gather the spanned bytes, big-endian, into a bottom-aligned u64
        // accumulator (width = bytes_needed * 8 <= 40 bits).
        let mut acc: u64 = 0;
        for &b in &self.data[byte_idx..byte_idx + bytes_needed] {
            acc = (acc << 8) | u64::from(b);
        }
        // In that accumulator the wanted field ends `off` bits below the top,
        // so its LSB sits at bit index (bytes_needed*8 - off - n). That shift
        // is always in 0..40 — never 64, never negative — even for n == 32.
        // The `off` leading bits of the first byte still sit above the field
        // after shifting, so the field must be masked to n bits.
        let shift = bytes_needed as u32 * 8 - off - n;
        let mask = (1u64 << n) - 1; // n in 1..=32, so this cannot overflow u64
        Ok(((acc >> shift) & mask) as u32)
    }

    /// Decode one UTF-8-style coded number (RFC 9639 §9.1.5), advancing the
    /// cursor as it goes. [`Self::read_utf8_coded`] wraps this to make the
    /// composite read atomic — every failure path here returns through the
    /// wrapper, which restores the cursor.
    ///
    /// The form's width comes from [`coded_number_octets`] — Table 18's lead
    /// column written as data, instead of a prefix-counting loop that has to
    /// re-derive it from bit patterns. Shifts and masks throughout, per the
    /// module's no-divide ARMv4T rule.
    fn coded_number_inner(&mut self) -> Result<u64> {
        let lead = self.read_bits(8)? as u8;
        let octets = match coded_number_octets(lead) {
            Some(n) => n,
            // 10xxxxxx cannot lead a number; 0xFF is the one all-ones lead RFC
            // 9639 leaves invalid (0xFE is the legal 7-byte lead, and the
            // pre-RFC "doubled prefix" warning concerned *it*, not 0xFF).
            None => return Err(Error::InvalidField),
        };
        if octets == 1 {
            // 0xxxxxxx → single octet, value is the low 7 bits.
            return Ok(u64::from(lead & 0x7F));
        }
        // Lead-byte payload width is 7 - octets (zero for the 7-byte lead).
        // `0xFFu32 >> (octets + 1)` is that width's mask with no subtraction
        // edge case: octets == 7 shifts a u32 by 8 → 0, which is defined.
        let mut value = u64::from(u32::from(lead) & (0xFFu32 >> (octets + 1)));
        for _ in 1..octets {
            let cont = self.read_bits(8)? as u8;
            if cont & 0xC0 != 0x80 {
                return Err(Error::InvalidField);
            }
            value = (value << 6) | u64::from(cont & 0x3F);
        }
        // Non-minimal form: below what this form exists to carry (see
        // CODED_NUMBER_FORM_MIN). No upper-bound test is needed, and that is a
        // proof rather than an omission: a form's maximum IS its payload
        // capacity, the widest form (7 octets) carries 0 + 6×6 = 36 bits which
        // is exactly the RFC's 36-bit cap, and 36 bits cannot overflow a u64.
        if value < u64::from(CODED_NUMBER_FORM_MIN[octets]) {
            return Err(Error::InvalidField);
        }
        Ok(value)
    }
}

// ---------------------------------------------------------------------------
// Window-domain bridge (split-ladder item 5, PR 1 — INERT: no decode-path
// call site). The seam itself (`RiceKernel`, the generic over it, and the
// sample loop that defaults to today's table count) is PR 2; the portable
// twin that sits behind it is PR 3. FLAC.md → "Split-ladder item 5 — the
// `RiceKernel` seam: three PRs (plan of record, 2026-11-05)".
// ---------------------------------------------------------------------------

/// Reverse the bit order of a `u32`: bit `i` ↔ bit `31 − i`. The one place
/// the reader's MSB-first `acc` and a kernel's LSB-first `w` are translated,
/// in either direction — the map is its own inverse, so the hand-out and the
/// resync are the same call.
///
/// Written through `core`'s `u32::reverse_bits` deliberately: pure Rust, no
/// table, no divide, no `unsafe` — consistent with this module's ARMv4T rule
/// (no CLZ, no hardware divide). What that lowers to on ARM7TDMI (which has
/// no bit-reverse instruction) is a codegen question, not a claim made here:
/// it is PR 2's gate that measures section bytes and cycles, and PR 3 is the
/// PR that *adds a table*. The E10 sample's `rev32` did ride a `REV8` table
/// (four lookups plus `swap_bytes`); that table is one of the three tables
/// PR 3 budgets at 544 B, and it does not exist in this PR, which carries no
/// data at all.
///
/// `pub` because the other side of the seam is not in this crate: an
/// implementor outside the core (item 6's `flac-lite-gba`) has to be able to
/// state the same mapping, and PR 3's twin is tested against it. Nothing
/// calls it yet.
pub const fn rev32(x: u32) -> u32 {
    x.reverse_bits()
}

/// The kernel-side view of [`BitReader`]'s window — the seam's data contract
/// (split-ladder item 5, PR 1). Produced by [`BitReader::kernel_window`],
/// consumed by [`BitReader::set_kernel_window`]; a kernel is free to mutate
/// one in place between those two calls.
///
/// Four fields, deliberately the same four the probe's kernel ABI used
/// (`w`/`v`/`ptr`/`end` out of the probe's five-field `KernelState`,
/// patch line 548 of the committed e10 patch). **There is no `status` field
/// here**: the probe's `status` (0 = all samples produced, 1 = unterminated)
/// is the *seam's* report, and the reporting shape belongs with the seam that
/// has to act on it — the reader's own end-of-data condition is already
/// [`Error::EndOfStream`], which PR 2 decides how to reach. Adding it now
/// would be engineering an API surface ahead of the PR that uses it.
///
/// Invariants at hand-out (both directions of the same facts — see
/// [`BitReader::kernel_window`] for the mapping they come from):
///
/// - bits ≥ `v` of `w` are zero — equivalently, the low `32 − valid` bits of
///   the reader's `acc` are zero;
/// - `bit_position() + v == ptr * 8` — the reader's last buffered bit is the
///   window's last valid bit, which is the kernel's byte-pointer invariant;
/// - `v <= 32`, `ptr <= end == slice length`.
///
/// A take-back does **not** require them (see
/// [`BitReader::set_kernel_window`] for the totality rule); they are what a
/// correct kernel maintains, and what the bridge's tests pin.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct KernelWindow {
    /// The bit-reversed window: bit 0 is the next stream bit, bit `i` is
    /// stream bit `bit_position() + i`; bits ≥ `v` are zero at hand-out.
    pub w: u32,
    /// Valid bits in `w` (0..=32). The reader's `valid`, unchanged.
    pub v: u32,
    /// Byte cursor — the next byte a kernel may merge in. Same view (a
    /// slice-relative offset) as the reader's own `pos`, unchanged.
    pub ptr: usize,
    /// End of the region a kernel may read, in the same view as `ptr`. The
    /// reader's slice length at hand-out; the take-back re-bounds against
    /// the reader, which is the authority (see
    /// [`BitReader::set_kernel_window`]).
    pub end: usize,
}

/// CRC-8 with FLAC's polynomial: `0x07` (x⁸+x²+x+1), no reflection, init
/// `0x00`, no final xor — over a whole-byte range.
///
/// Used for the frame header (RFC 9639 §9.1.8). The *verification* is gated
/// behind `crc-check` (release consumes the CRC byte unchecked by design);
/// the function itself is always compiled — the planned stream-scan cursor
/// needs strict-field-plus-CRC-8 filtering regardless of feature flags.
///
/// Table-free by design, scoped to *that* gated use: verification under
/// `crc-check` never runs in a release build, so a 256-byte table trades
/// nothing there. The scan-cursor use is the opposite shape — it would run
/// CRC-8 in the default path, on-target, once per candidate header — so a
/// table becomes worth an E-menu measurement only against *that* use, not
/// against `crc-check`.
/// Witnessed by all 15 real libFLAC 1.5.0 headers in
/// `tests/frame_header_vectors.txt` (generator census fail-closed), the
/// published CRC-8 check value `"123456789" → 0xF4`, and a naive bit-serial
/// oracle below.
pub fn crc8(data: &[u8]) -> u8 {
    let mut crc = 0u8;
    for &byte in data {
        crc ^= byte;
        for _ in 0..8 {
            crc = if crc & 0x80 != 0 {
                (crc << 1) ^ 0x07
            } else {
                crc << 1
            };
        }
    }
    crc
}

/// CRC-16 with FLAC's frame polynomial: x¹⁶+x¹⁵+x²+1 — RFC 9639 §9.3 (Frame
/// Footer) states it as a polynomial; `0x05` is the truncated coefficient
/// listing below x¹⁶, shift-register form `0x8005`. (§9.1.8 is the frame
/// *header* CRC-8 — a different CRC, cited here once to keep them apart.)
/// No reflection, **init `0x0000`**, no final xor — over a whole-byte range.
///
/// **Measured correction (2026-11-03, Phase 2 step 5):** this scaffold
/// comment claimed `init 0xFFFF`. It is wrong. `0x0000` is the only init
/// that reproduces the stored footer of every committed real frame
/// (10/10 frames in `tests/frame_run_vectors.txt`, cross-checked by three
/// mechanisms: byte-wise LFSR, bit-serial, and a GF(2) polynomial-remainder
/// computation; the reflected family is excluded outright — no init in the
/// full 0..=0xFFFF sweep fits). The derived check value for
/// `"123456789"` is `0xFEE8`.
///
/// Coverage is frame-wide, which §9.3 states as plainly as the polynomial:
/// "a 16-bit CRC, initialized with 0 … covers the whole frame, excluding the
/// 16-bit CRC but including the sync code" — sync code through the padding
/// bits, CRC bytes excluded. The committed real-frame footers above measure
/// the same window; [`crate::frame`] is where `crc-check` wires it.
pub fn crc16(data: &[u8]) -> u16 {
    let mut crc = 0u16;
    for &byte in data {
        crc ^= u16::from(byte) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ 0x8005
            } else {
                crc << 1
            };
        }
    }
    crc
}

// Unit tests compile as part of the lib under the host test harness (Gate 2,
// run from *outside* the repo — see README "Cargo config leak"). Keep these
// `core`-only: no `std`, and no dependency on any not-yet-implemented method.
/// Run-length table ([`BitReader::rice_quotient`], E2b landed): `LEADING_ONES[b]`
/// = number of leading one-bits of `b` (0..=8, all-ones answers 8).
///
/// **Pinned to plain `.iwram` (Sam's exception, 2026-10-22).** The probe
/// measured the table count with the table pinned; the first landing rode
/// `.rodata` → ROM instead (cartridge-read penalty per 8-bit counting
/// window) because `link_section` is an unsafe attribute and the crate was
/// `#![forbid(unsafe_code)]`. The rate-play ROM's producer-wall
/// measurement reopened the question and the exception was ratified —
/// crate root is now `deny` with this one item `allow`ed. `> iwram
/// AT>rom`: agb's `CommonInit` copies the section ROM→IWRAM at boot (BIOS
/// `CPUSet`), so each window is a zero-wait `ldrb`. Plain `.iwram`, never
/// `.text_iwram` (E6: `.text*(.text*)` captures that name back into ROM).
/// The `cfg` gate keeps the attribute off host builds entirely; the
/// Built-ELF witness is `nm` — the symbol at `0x0300xxxx` proves the pin
/// survives `lto = "fat"`.
///
/// **Feature-gated since 2026-11-02** (FLAC.md → split proposal ladder,
/// the gate held back from the metadata reframe PR): the attribute rides
/// `all(target_arch = "arm", feature = "iwram-pin")` — off by default, so
/// portable consumers and other ARM bare-metal targets build without the
/// GBA link-script contract (no missing-section failure mode). GBA ROM
/// crates opt in to keep the measured zero-wait read. The ratified
/// exception count is unchanged: same single item.
#[cfg_attr(
    all(target_arch = "arm", feature = "iwram-pin"),
    unsafe(link_section = ".iwram")
)]
#[allow(unsafe_code)] // The crate-root exception: a placement attribute on
// an immutable compile-time table is not unsafe
// code (edition-2024 lints it as an unsafe
// attribute); decision recorded in lib.rs + FLAC.md.
static LEADING_ONES: [u8; 256] = build_leading_ones();

const fn build_leading_ones() -> [u8; 256] {
    let mut table = [0u8; 256];
    let mut byte = 0usize;
    while byte < 256 {
        let b = byte as u8;
        let mut count = 0u8;
        let mut mask = 0x80u8;
        while mask != 0 && (b & mask) != 0 {
            count += 1;
            mask >>= 1;
        }
        table[byte] = count;
        byte += 1;
    }
    table
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Independent oracle: bit `i` of the slice, MSB-first within each byte.
    /// Deliberately naive so a shared bug with `peek_at` is unlikely.
    fn ref_bit(data: &[u8], i: usize) -> u32 {
        u32::from((data[i >> 3] >> (7 - (i & 7))) & 1)
    }

    /// Deterministic pseudo-random bytes (LCG) — no_std-friendly, reproducible.
    struct Lcg(u32);
    impl Lcg {
        fn next_u8(&mut self) -> u8 {
            self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (self.0 >> 24) as u8
        }
    }

    // ---- rice_quotient (E2b landed 2026-10-21) -----------------------------

    #[test]
    fn leading_ones_table_matches_naive_oracle_exhaustively() {
        for b in 0..=255usize {
            let mut oracle = 0u8;
            let mut mask = 0x80u8;
            while mask != 0 && (b as u8 & mask) != 0 {
                oracle += 1;
                mask >>= 1;
            }
            assert_eq!(LEADING_ONES[b], oracle, "byte {b:#04X}");
        }
    }

    /// The differential witness: quotients 0..=40 at every bit offset 0..7,
    /// judged against the *old production primitive* — the per-bit
    /// `read_bits(1)` loop the table replaces (different mechanism, same
    /// reader), plus an explicit quotient expectation.
    #[test]
    fn rice_quotient_matches_per_bit_loop_at_every_offset() {
        for off in 0..8usize {
            for q in 0..=40usize {
                // off arbitrary bits, q zeros, terminator, then ones padding.
                let mut bits = [0u8; 96];
                let mut n = 0usize;
                for _ in 0..off {
                    bits[n] = 1;
                    n += 1;
                }
                n += q; // zeros: the array starts zeroed
                bits[n] = 1;
                n += 1;
                while n < bits.len() {
                    bits[n] = 1;
                    n += 1;
                }
                let bytes: [u8; 12] = pack_bits(&bits[..n]);

                // Oracle: the per-bit loop exactly as decode_rice_partition
                // ran it before this landing.
                let mut oracle = BitReader::new(&bytes);
                skip(&mut oracle, off as u32);
                let mut counted = 0u32;
                while oracle.read_bits(1).unwrap() == 0 {
                    counted += 1;
                }

                let mut r = BitReader::new(&bytes);
                skip(&mut r, off as u32);
                let at = r.bit_position();
                let got = r.rice_quotient().expect("terminated run");
                assert_eq!(got, q as u32, "off {off} q {q}");
                assert_eq!(got, counted, "oracle disagreement off {off} q {q}");
                // Cursor sits just past the terminator, like the old loop.
                assert_eq!(r.bit_position(), at + q + 1, "cursor off {off} q {q}");
            }
        }
    }

    #[test]
    fn rice_quotient_eof_is_atomic_across_the_tail() {
        // One-byte slices so the run reaches the true end of data: prefix
        // ones then zeros to the last bit. off = 0 exercises the full-window
        // path walking off the end; off 1..7 exercise the sub-byte tail
        // (remaining < 8 at entry). Unterminated: Err, cursor restored.
        for off in 0..8usize {
            let byte = [((0xFFu16 << (8 - off)) & 0xFF) as u8]; // off ones, rest zeros
            let mut r = BitReader::new(&byte);
            skip(&mut r, off as u32);
            let at = r.bit_position();
            assert_eq!(r.rice_quotient(), Err(Error::EndOfStream), "off {off}");
            assert_eq!(r.bit_position(), at, "atomic restore off {off}");
        }
        // Empty remainder at entry: EOF, cursor untouched.
        let mut r = BitReader::new(&[0x00]);
        r.read_bits(8).unwrap();
        assert_eq!(r.rice_quotient(), Err(Error::EndOfStream));
        assert_eq!(r.bit_position(), 8);
    }

    #[test]
    fn rice_quotient_terminator_as_the_very_last_bit() {
        // The tightest tail: v = 8 − off bits remain and the run is v − 1
        // zeros with the terminator as the final bit of the slice. off = 0
        // is the full-window case (win = 0x01); off 1..7 the sub-byte tail.
        for off in 0..8usize {
            let byte = [(((0xFFu16 << (8 - off)) & 0xFF) | 1) as u8];
            let mut r = BitReader::new(&byte);
            skip(&mut r, off as u32);
            let got = r.rice_quotient().expect("terminator on last bit");
            assert_eq!(got, 7 - off as u32, "off {off}");
            assert_eq!(r.bits_remaining(), 0, "consumed to the last bit off {off}");
        }
    }

    /// Advance past `n` bits; `read_bits(0)` is a caller-bug rejection, so
    /// a zero-length skip must not call it.
    fn skip(r: &mut BitReader<'_>, n: u32) {
        if n > 0 {
            r.read_bits(n).unwrap();
        }
    }

    /// Pack an MSB-first bit vector (naive, one bit at a time — the same
    /// independent-packer discipline as the residual suite). Fixed-size
    /// output keeps the suite allocation-free like the crate itself; zero
    /// padding past the bit vector never reaches a terminated read.
    fn pack_bits<const N: usize>(bits: &[u8]) -> [u8; N] {
        let mut out = [0u8; N];
        for (i, &b) in bits.iter().enumerate() {
            if b == 1 {
                out[i >> 3] |= 1 << (7 - (i & 7));
            }
        }
        out
    }

    // ---- width validation -------------------------------------------------

    #[test]
    fn read_bits_rejects_out_of_range_widths() {
        let mut r = BitReader::new(&[0xFF; 4]);
        assert_eq!(r.read_bits(0), Err(Error::InvalidField));
        assert_eq!(r.read_bits(33), Err(Error::InvalidField));
        assert_eq!(r.read_bits(u32::MAX), Err(Error::InvalidField));
        // Rejected reads must not move the cursor.
        assert_eq!(r.bit_position(), 0);
        assert_eq!(r.read_bits(32).unwrap(), 0xFFFF_FFFF);
    }

    // ---- empty / EOF / cursor invariants ----------------------------------

    #[test]
    fn empty_slice_reports_end_of_stream() {
        let mut r = BitReader::new(&[]);
        assert_eq!(r.bits_remaining(), 0);
        assert_eq!(r.read_bits(1), Err(Error::EndOfStream));
        assert_eq!(r.bit_position(), 0);
    }

    #[test]
    fn read_may_end_exactly_at_last_bit_but_not_one_past() {
        // 0xA55A = 1010_0101_0101_1010; top 15 bits = 0x52AD, last bit = 0.
        let mut r = BitReader::new(&[0xA5, 0x5A]);
        assert_eq!(r.read_bits(15).unwrap(), 0x52AD);
        assert_eq!(r.bits_remaining(), 1);
        assert_eq!(r.read_bits(1).unwrap(), 0); // last bit, exact fit: ok
        assert_eq!(r.bits_remaining(), 0);
        assert_eq!(r.bit_position(), 16);
        assert_eq!(r.read_bits(1), Err(Error::EndOfStream));
        assert_eq!(r.bit_position(), 16); // unchanged after failure
    }

    #[test]
    fn failed_read_leaves_cursor_untouched_mid_stream() {
        let mut r = BitReader::new(&[0b1010_1010]);
        assert_eq!(r.read_bits(3).unwrap(), 0b101);
        assert_eq!(r.read_bits(6), Err(Error::EndOfStream)); // only 5 left
        assert_eq!(r.bit_position(), 3);
        assert_eq!(r.bits_remaining(), 5);
    }

    // ---- hand-computed patterns (MSB-first) --------------------------------

    #[test]
    fn nibbles_within_one_byte() {
        let mut r = BitReader::new(&[0xA5]); // 1010_0101
        assert_eq!(r.read_bits(4).unwrap(), 0xA);
        assert_eq!(r.read_bits(4).unwrap(), 0x5);
        assert_eq!(r.read_bits(1), Err(Error::EndOfStream));
    }

    #[test]
    fn msb_first_bit_order() {
        let mut r = BitReader::new(&[0b1011_0100]);
        let mut got = [0u32; 8];
        for g in &mut got {
            *g = r.read_bits(1).unwrap();
        }
        assert_eq!(got, [1, 0, 1, 1, 0, 1, 0, 0]);
    }

    #[test]
    fn read_spans_byte_boundary() {
        let mut r = BitReader::new(&[0x12, 0x34]); // 0001_0010 0011_0100
        assert_eq!(r.read_bits(12).unwrap(), 0x123);
        assert_eq!(r.read_bits(4).unwrap(), 0x4);
    }

    #[test]
    fn full_32_bit_aligned_read() {
        // The shift-by-32 trap: a `(x >> (32 - n))` formulation with n == 32
        // shifts by 32 and panics/UBs. `n == 32` is legal, so it must work.
        let mut r = BitReader::new(&[0xDE, 0xAD, 0xBF, 0xAC]);
        assert_eq!(r.read_bits(32).unwrap(), 0xDEAD_BFAC);
        assert_eq!(r.bit_position(), 32);
    }

    #[test]
    fn full_32_bit_unaligned_read_spans_five_bytes() {
        // Worst case: off = 3, n = 32 → 5 bytes gathered, accumulator holds 40
        // bits. Reference value: bits [3..35) of 0xABCDEF0123 (40-bit BE) =
        // (0xABCDEF0123 >> 5) & 0xFFFFFFFF = 0x5E6F_7809.
        let mut r = BitReader::new(&[0xAB, 0xCD, 0xEF, 0x01, 0x23]);
        assert_eq!(r.read_bits(3).unwrap(), 0b101);
        assert_eq!(r.read_bits(32).unwrap(), 0x5E6F_7809);
    }

    // ---- peek_bits ----------------------------------------------------------

    #[test]
    fn peek_matches_read_and_never_moves_cursor() {
        // 0xA55A = 1010_0101_0101_1010. Peek a field, confirm read returns the
        // identical value and only read advances the cursor.
        let mut r = BitReader::new(&[0xA5, 0x5A]);
        for _ in 0..3 {
            // Idempotent: repeated peeks are identical...
            assert_eq!(r.peek_bits(7).unwrap(), 0b101_0010); // top 7 bits of 0xA5
            assert_eq!(r.bit_position(), 0); // ...and never move the cursor
        }
        assert_eq!(r.read_bits(7).unwrap(), 0b101_0010);
        assert_eq!(r.bit_position(), 7);
        for _ in 0..2 {
            assert_eq!(r.peek_bits(5).unwrap(), 0b10101); // bits [7..12)
            assert_eq!(r.bit_position(), 7);
        }
        assert_eq!(r.read_bits(5).unwrap(), 0b10101);
        assert_eq!(r.bit_position(), 12);
    }

    #[test]
    fn peek_validates_widths_like_read() {
        // `&self` receiver: this whole test needs no mutable cursor at all.
        let r = BitReader::new(&[0xFF; 4]);
        assert_eq!(r.peek_bits(0), Err(Error::InvalidField));
        assert_eq!(r.peek_bits(33), Err(Error::InvalidField));
        assert_eq!(r.peek_bits(u32::MAX), Err(Error::InvalidField));
        assert_eq!(r.bit_position(), 0);
        // Legal widths still work after the rejections.
        assert_eq!(r.peek_bits(32).unwrap(), 0xFFFF_FFFF);
        assert_eq!(r.bit_position(), 0);
    }

    #[test]
    fn peek_eof_leaves_cursor_untouched() {
        let mut r = BitReader::new(&[0b1010_1010]);
        assert_eq!(r.read_bits(3).unwrap(), 0b101);
        assert_eq!(r.peek_bits(6), Err(Error::EndOfStream)); // only 5 left
        assert_eq!(r.bit_position(), 3);
        assert_eq!(r.peek_bits(5).unwrap(), 0b0_1010); // exactly what remains
        assert_eq!(r.bit_position(), 3);
        // Drain to the end: peek fails at EOF but never before the last read.
        assert_eq!(r.read_bits(5).unwrap(), 0b0_1010);
        assert_eq!(r.peek_bits(1), Err(Error::EndOfStream));
        assert_eq!(r.bit_position(), 8);
    }

    #[test]
    fn peek_32_bits_unaligned_covers_worst_case_span() {
        // Same five-byte worst case as the wide-read test (off = 3, n = 32),
        // but via peek: value must match and cursor must not move.
        let mut r = BitReader::new(&[0xAB, 0xCD, 0xEF, 0x01, 0x23]);
        assert_eq!(r.read_bits(3).unwrap(), 0b101);
        for _ in 0..2 {
            assert_eq!(r.peek_bits(32).unwrap(), 0x5E6F_7809);
            assert_eq!(r.bit_position(), 3);
        }
        assert_eq!(r.read_bits(32).unwrap(), 0x5E6F_7809);
    }

    #[test]
    fn peek_agrees_with_oracle_at_every_position() {
        // Differential sweep over every absolute bit position (alignment is
        // where peek bugs live): peek must equal the oracle bits assembled
        // MSB-first, at every alignment, for every legal width.
        let mut rng = Lcg(0xCAFE_F00D);
        let mut data = [0u8; 16];
        for d in &mut data {
            *d = rng.next_u8();
        }
        let total = data.len() * 8;

        for start in 0..total {
            let byte_start = start >> 3;
            let pad = start & 7;
            for width in 1..=32usize {
                let mut r = BitReader::new(&data[byte_start..]);
                if pad > 0 {
                    let _ = r.read_bits(pad as u32).unwrap();
                }
                let pos = start - (byte_start * 8); // cursor within subslice
                match r.peek_bits(width as u32) {
                    Ok(val) => {
                        assert_eq!(
                            r.bit_position(),
                            pos,
                            "peek must not move cursor: start {start}, width {width}"
                        );
                        let mut expect = 0u32;
                        for j in 0..width {
                            expect = (expect << 1) | ref_bit(&data, start + j);
                        }
                        assert_eq!(val, expect, "start {start}, width {width}");
                    }
                    Err(Error::EndOfStream) => assert!(start + width > total),
                    Err(e) => panic!("unexpected at start {start}, width {width}: {e:?}"),
                }
            }
        }
    }

    // ---- read_signed --------------------------------------------------------

    /// Independent oracle: assemble `w` bits MSB-first from the naive bit
    /// source, then sign-extend by **definition** — if the sign bit is set,
    /// subtract 2^w (in `i64`, so `w == 32` cannot overflow). A genuinely
    /// *different mechanism* than the shl+arith-shr implementation in
    /// `read_signed`, so a shared bug is unlikely.
    fn ref_signed(data: &[u8], start: usize, w: usize) -> i32 {
        let mut raw = 0u32;
        for j in 0..w {
            raw = (raw << 1) | ref_bit(data, start + j);
        }
        if raw & (1 << (w - 1)) == 0 {
            raw as i32 // sign bit clear: the raw bits *are* the value
        } else {
            (raw as i64 - (1i64 << w)) as i32 // raw - 2^w, the two's-complement definition
        }
    }

    #[test]
    fn read_signed_hand_computed_fields() {
        // 0xF1 0x23 = 1111_0001_0010_0011, read whole (16 bits, exact fit).
        let mut r = BitReader::new(&[0xF1, 0x23]);
        assert_eq!(r.read_signed(4).unwrap(), -1); // 1111 → -1
        assert_eq!(r.read_signed(4).unwrap(), 1); // 0001 → +1
        assert_eq!(r.read_signed(5).unwrap(), 4); // 00100 → +4 (sign bit 0)
        assert_eq!(r.read_signed(3).unwrap(), 3); // 011 → +3 (sign bit 0)
        assert_eq!(r.bits_remaining(), 0);
    }

    #[test]
    fn read_signed_unaligned_crosses_byte_boundary() {
        // Chosen so bits [3..15) are exactly 1111_1111_1001 (= -7 in 12-bit
        // two's complement): byte0 = 101_11111, byte1 = 1111_001_0.
        // 0xBF 0xF2 = 1011_1111_1111_0010.
        let mut r = BitReader::new(&[0xBF, 0xF2]);
        assert_eq!(r.read_bits(3).unwrap(), 0b101); // re-align to bit 3
        assert_eq!(r.read_signed(12).unwrap(), -7); // bits [3..15) = 1111_1111_1001
    }

    #[test]
    fn read_signed_extremes_per_width() {
        // All-ones reads as -1 at every width (shl puts a one in bit 31, the
        // arithmetic shr fills the whole high half with ones → all-ones i32).
        // Consecutive widths 1..=32 consume 528 bits → 66 bytes exactly.
        let mut r = BitReader::new(&[0xFF; 66]);
        for n in 1..=32u32 {
            assert_eq!(r.read_signed(n).unwrap(), -1, "width {n}");
        }
        assert_eq!(r.bit_position(), (1..=32u32).sum::<u32>() as usize); // 528 bits

        // ...and the minimum negative (1000…₂) at every width is i32::MIN >> (32-n).
        for n in 1..=32usize {
            let bytes = [0x80, 0, 0, 0, 0]; // bit 0 = 1, rest 0
            let mut r = BitReader::new(&bytes);
            let want = (i32::MIN >> (32 - n)) as i64;
            assert_eq!(r.read_signed(n as u32).unwrap() as i64, want, "width {n}");
        }
    }

    #[test]
    fn read_signed_32_bit_boundary() {
        // n == 32 takes the plain `as i32` branch — the one width the shift
        // trick cannot handle (n - 1 == 31 would misplace the flip).
        let mut r = BitReader::new(&[0xFF, 0xFF, 0xFF, 0xFF]);
        assert_eq!(r.read_signed(32).unwrap(), -1);
        let mut r = BitReader::new(&[0x80, 0, 0, 0]);
        assert_eq!(r.read_signed(32).unwrap(), i32::MIN);
        let mut r = BitReader::new(&[0x7F, 0xFF, 0xFF, 0xFF]);
        assert_eq!(r.read_signed(32).unwrap(), i32::MAX);
    }

    #[test]
    fn read_signed_validates_widths_like_read() {
        let mut r = BitReader::new(&[0xFF; 4]);
        assert_eq!(r.read_signed(0), Err(Error::InvalidField));
        assert_eq!(r.read_signed(33), Err(Error::InvalidField));
        assert_eq!(r.read_signed(u32::MAX), Err(Error::InvalidField));
        assert_eq!(r.bit_position(), 0); // rejected reads must not move the cursor
    }

    #[test]
    fn read_signed_eof_leaves_cursor_untouched() {
        let mut r = BitReader::new(&[0b1111_1111]);
        assert_eq!(r.read_signed(3).unwrap(), -1);
        assert_eq!(r.read_signed(6), Err(Error::EndOfStream)); // only 5 left
        assert_eq!(r.bit_position(), 3);
        assert_eq!(r.read_signed(5).unwrap(), -1); // exact fit on the last bits
        assert_eq!(r.bit_position(), 8);
        assert_eq!(r.read_signed(1), Err(Error::EndOfStream));
        assert_eq!(r.bit_position(), 8);
    }

    #[test]
    fn read_signed_agrees_with_unsigned_plus_oracle() {
        // Differential sweep over every absolute bit position × width:
        //  1. read_signed must equal read_bits on the same bits, sign-extended
        //     with the shl+shr idiom (a different mechanism from the impl),
        //  2. and match the independent assembled-oracle value.
        //  3. and neither read may disturb the other's cursor rules.
        let mut rng = Lcg(0x51ED_31ED);
        let mut data = [0u8; 16];
        for d in &mut data {
            *d = rng.next_u8();
        }
        let total = data.len() * 8;

        for start in 0..total {
            let byte_start = start >> 3;
            let pad = start & 7;
            for width in 1..=32usize {
                // Fresh reader per probe: the cursor only moves forward.
                let mut r = BitReader::new(&data[byte_start..]);
                if pad > 0 {
                    let _ = r.read_bits(pad as u32).unwrap();
                }
                let pos = start - (byte_start * 8); // cursor within subslice

                // Reference values from the unsigned path + independent oracle.
                let unsigned_expect = {
                    let mut u = BitReader::new(&data[byte_start..]);
                    if pad > 0 {
                        let _ = u.read_bits(pad as u32).unwrap();
                    }
                    match u.read_bits(width as u32) {
                        Ok(v) => v,
                        Err(Error::EndOfStream) => {
                            assert!(start + width > total);
                            continue;
                        }
                        Err(e) => panic!("unsigned read failed: {e:?}"),
                    }
                };
                // shl+arith-shr idiom, written out here from the *unsigned*
                // read: move the field's sign bit to bit 31, drag it down.
                let sign_extended =
                    (((unsigned_expect << (32 - width)) as i32) >> (32 - width)) as i64;
                let oracle = ref_signed(&data, start, width) as i64;

                match r.read_signed(width as u32) {
                    Ok(val) => {
                        assert_eq!(val as i64, sign_extended, "start {start}, width {width}");
                        assert_eq!(val as i64, oracle, "start {start}, width {width}");
                        assert_eq!(r.bit_position(), pad + width, "cursor must advance");
                    }
                    Err(Error::EndOfStream) => assert!(start + width > total),
                    Err(e) => panic!("unexpected at start {start}, width {width}: {e:?}"),
                }
            }
        }
    }

    // ---- read_utf8_coded -----------------------------------------------------

    /// Independent encoder for roundtrip tests: builds the canonical
    /// (shortest-form) octet sequence for a value by Table 18 magnitude
    /// boundaries — construction-by-shifting-from-the-top, a deliberately
    /// different mechanism from the decode-under-test (which walks
    /// prefix → continuations), so a shared bug is unlikely.
    ///
    /// Table 18 boundaries (RFC 9639 §9.1.5): total octets and lead base per
    /// magnitude class.
    fn encode_coded(v: u64) -> ([u8; 7], usize) {
        const FORMS: [(u64, u8, u32); 7] = [
            (0x7F, 0x00, 1),          // 0xxxxxxx
            (0x7FF, 0xC0, 2),         // 110xxxxx
            (0xFFFF, 0xE0, 3),        // 1110xxxx
            (0x1F_FFFF, 0xF0, 4),     // 11110xxx
            (0x3FF_FFFF, 0xF8, 5),    // 111110xx: 2+24 = 26 payload bits
            (0x7F_FF_FF_FF, 0xFC, 6), // 1111110x
            (0xF_FFFF_FFFF, 0xFE, 7), // 11111110 + 6 continuations
        ];
        let mut out = [0u8; 7];
        for &(max, base, t) in FORMS.iter() {
            if v <= max {
                // Payload below the lead byte; t == 1 shifts by 0 (identity,
                // and v ≤ 0x7F fits the lead payload exactly).
                out[0] = base | (v >> (6 * (t - 1))) as u8;
                for i in 1..t {
                    out[i as usize] = 0x80 | ((v >> (6 * (t - 1 - i))) & 0x3F) as u8;
                }
                return (out, t as usize);
            }
        }
        panic!("{v:#x} exceeds the 36-bit coded-number range");
    }

    #[test]
    fn read_utf8_coded_single_byte_identity() {
        // 0xxxxxxx is the identity form: every one-byte lead decodes to
        // itself, cursor lands on the next byte.
        for lead in 0x00u8..=0x7F {
            let data = [lead];
            let mut r = BitReader::new(&data);
            assert_eq!(r.read_utf8_coded().unwrap(), u64::from(lead), "{lead:#04X}");
            assert_eq!(r.bit_position(), 8);
        }
    }

    #[test]
    fn read_utf8_coded_rfc_table_vectors() {
        // Every form class from the RFC 9639 Table 18 shape, at minimum,
        // mid, and maximum where distinct — expectations derived by hand
        // from the bit layout (payload bits concatenated MSB-first) and
        // cross-checked against an independent spec-derived oracle script.
        // Includes the RFC §9.1.5 worked example (51 billion).
        let cases: &[(&[u8], u64)] = &[
            (&[0x00], 0),
            (&[0x7F], 127),
            (&[0xC2, 0x80], 0x80),
            (&[0xDF, 0xBF], 0x7FF),
            (&[0xE0, 0xA0, 0x80], 0x800),
            (&[0xEF, 0xBF, 0xBF], 0xFFFF),
            (&[0xF0, 0x90, 0x80, 0x80], 0x1_0000),
            (&[0xF1, 0x80, 0x80, 0x80], 0x4_0000), // lead payload 0b0001 << 18
            (&[0xF7, 0xBF, 0xBF, 0xBF], 0x1F_FFFF),
            (&[0xF8, 0x88, 0x80, 0x80, 0x80], 0x20_0000),
            (&[0xFB, 0xBF, 0xBF, 0xBF, 0xBF], 0x3_FFFF_FF), // 26-bit max of the 5-byte form
            (&[0xFC, 0x84, 0x80, 0x80, 0x80, 0x80], 0x400_0000),
            (&[0xFD, 0xBF, 0xBF, 0xBF, 0xBF, 0xBF], 0x7F_FF_FF_FF),
            // 7-byte lead carries ZERO payload bits, so its minimum value
            // comes from the first continuation: 0b10 << 30 = 2^31. (The all
            // `-0x80` form of the same lead would decode to 0 — below row 7's
            // minimum, i.e. the non-canonical zero, rejected. That case lives
            // in `read_utf8_coded_rejects_non_minimal_forms`.)
            (&[0xFE, 0x82, 0x80, 0x80, 0x80, 0x80, 0x80], 0x8000_0000), // 2^31, table minimum
            (&[0xFE, 0xBF, 0xBF, 0xBF, 0xBF, 0xBF, 0xBF], 0xF_FFFF_FFFF), // 36-bit max
            (&[0xFE, 0xAF, 0x9F, 0xB5, 0xA3, 0xB8, 0x80], 51_000_000_000), // RFC worked example
        ];
        for &(bytes, want) in cases {
            let mut r = BitReader::new(bytes);
            assert_eq!(r.read_utf8_coded().unwrap(), want, "{bytes:02X?}");
            // Whole number consumed exactly one byte per octet.
            assert_eq!(r.bit_position(), bytes.len() * 8, "{bytes:02X?}");
        }
    }

    #[test]
    fn read_utf8_coded_rejects_invalid_forms_cursor_untouched() {
        // Stray continuation leads (10xxxxxx), the all-ones lead (0xFF), and
        // broken continuation bytes are all InvalidField — and because the
        // read is composite, the cursor must be fully restored in every case
        // (nothing before the failure point stays consumed).
        let cases: &[&[u8]] = &[
            &[0x80],             // stray continuation lead
            &[0xBF],             // stray continuation lead (upper)
            &[0xFF],             // prefix == 8: the invalid all-ones lead
            &[0xC2, 0x00],       // second octet is not 10xxxxxx
            &[0xC2, 0xC0],       // continuation bit pattern broken (11…)
            &[0xDF, 0x3F],       // continuation bit pattern broken (00…)
            &[0xE0, 0xA0, 0x00], // third octet not a continuation
        ];
        for &bytes in cases {
            let mut r = BitReader::new(bytes);
            assert_eq!(
                r.read_utf8_coded(),
                Err(Error::InvalidField),
                "{bytes:02X?}"
            );
            assert_eq!(r.bit_position(), 0, "atomic: {bytes:02X?}");
        }
        // Every stray-continuation lead byte at all rejects before consuming
        // any continuation.
        for lead in 0x80u8..0xC0 {
            let data = [lead, 0x80];
            let mut r = BitReader::new(&data);
            assert_eq!(r.read_utf8_coded(), Err(Error::InvalidField), "{lead:#04X}");
            assert_eq!(r.bit_position(), 0);
        }
    }

    #[test]
    fn read_utf8_coded_rejects_non_minimal_forms() {
        // A value written in a longer form than it needs is
        // `[Error::InvalidField]`, and the cursor is untouched (the read is
        // atomic, like every other rejection). The rule is Table 18's left
        // column, i.e. CODED_NUMBER_FORM_MIN.
        //
        // The reference decoder, by contrast, accepts these forms — but only
        // once *both* CRCs are recomputed over the widened header, which is
        // what the `mutation number-form-overlong` rows of
        // tests/stream_vectors.txt pin (`mutation_formN_reference_exit 0` for
        // the 2-, 3- and 4-octet spellings). Leaving the frame footer covering
        // the original header bytes gives FRAME_CRC_MISMATCH, and leaving the
        // header CRC-8 stale gives BAD_HEADER; neither says anything about the
        // form. Being strict here is therefore safe (no real encode is
        // rejected — the generator asserts every frame number in every fixture
        // is minimal) and cheap, and it is a recorded divergence rather than a
        // restatement of FLAC behaviour. See `read_utf8_coded`'s docs for the
        // full trace.
        let cases: &[&[u8]] = &[
            &[0xC0, 0x80],       // 0, the UTF-8 overlong by name
            &[0xE0, 0x80, 0x80], // 0 in 3 octets
            &[0xF0, 0x80, 0x80, 0x80],
            &[0xF8, 0x80, 0x80, 0x80, 0x80],
            &[0xFC, 0x80, 0x80, 0x80, 0x80, 0x80],
            // The all-`0x80` 7-octet form: zero everywhere, including the lead,
            // which carries NO payload bits -- so this is "0 written in 7
            // octets", and it is also what proves the lead contributes nothing.
            &[0xFE, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80],
        ];
        for &bytes in cases {
            let mut r = BitReader::new(bytes);
            assert_eq!(
                r.read_utf8_coded(),
                Err(Error::InvalidField),
                "{bytes:02X?}"
            );
            assert_eq!(r.bit_position(), 0, "atomic: {bytes:02X?}");
        }

        // Exhaustive for the two-byte form: every 0xC0/0xC1 lead pair carries a
        // value below 0x80, so every one of them is non-minimal.
        for lead in 0xC0u8..0xC2 {
            for cont in 0x80u8..0xC0 {
                let data = [lead, cont];
                let value = (u64::from(lead & 0x1F) << 6) | u64::from(cont & 0x3F);
                let mut r = BitReader::new(&data);
                assert_eq!(
                    r.read_utf8_coded(),
                    Err(Error::InvalidField),
                    "{lead:02X} {cont:02X} carries {value:#x}"
                );
                assert_eq!(r.bit_position(), 0, "atomic: {lead:02X} {cont:02X}");
            }
        }

        // The boundary itself, per form: for each form width N the value
        // CODED_NUMBER_FORM_MIN[N] - 1 is exactly the previous form's maximum,
        // so it has BOTH an N-octet spelling (rejected) and an (N-1)-octet one
        // (accepted, and accepted at its own minimum). Same value, two
        // spellings, opposite verdicts -- that is the whole rule in one table.
        // The boundary itself, per form: for each form width N the value
        // CODED_NUMBER_FORM_MIN[N] - 1 is exactly the previous form's maximum,
        // so it has BOTH an N-octet spelling (rejected) and an (N-1)-octet one
        // (accepted). Same value, two spellings, opposite verdicts — that is the
        // whole rule in one table. Every byte pair here was produced and then
        // re-parsed by an independent Python encoder/decoder before being
        // written into this test (the habit behind every byte vector in this
        // repo), not written out by hand.
        let boundaries: &[(&[u8], &[u8], u64, usize)] = &[
            (&[0x7F], &[0xC1, 0xBF], 0x7F, 2),
            (&[0xDF, 0xBF], &[0xE0, 0x9F, 0xBF], 0x7FF, 3),
            (&[0xEF, 0xBF, 0xBF], &[0xF0, 0x8F, 0xBF, 0xBF], 0xFF_FF, 4),
            (
                &[0xF7, 0xBF, 0xBF, 0xBF],
                &[0xF8, 0x87, 0xBF, 0xBF, 0xBF],
                0x1F_FF_FF,
                5,
            ),
            (
                &[0xFB, 0xBF, 0xBF, 0xBF, 0xBF],
                &[0xFC, 0x83, 0xBF, 0xBF, 0xBF, 0xBF],
                0x3FF_FFFF, // 26-bit top: 2^26 - 1
                6,
            ),
            (
                &[0xFD, 0xBF, 0xBF, 0xBF, 0xBF, 0xBF],
                &[0xFE, 0x81, 0xBF, 0xBF, 0xBF, 0xBF, 0xBF],
                0x7F_FF_FF_FF,
                7,
            ),
        ];
        for &(minimal, non_minimal, value, octets) in boundaries {
            assert_eq!(
                u64::from(CODED_NUMBER_FORM_MIN[octets]),
                value + 1,
                "row {octets}'s minimum must be the previous row's maximum + 1"
            );

            let mut r = BitReader::new(non_minimal);
            assert_eq!(
                r.read_utf8_coded(),
                Err(Error::InvalidField),
                "{value:#x} spelled in {octets} octets"
            );
            assert_eq!(r.bit_position(), 0, "atomic");

            let mut r = BitReader::new(minimal);
            assert_eq!(r.read_utf8_coded().unwrap(), value, "{minimal:02X?}");
            assert_eq!(r.bit_position(), minimal.len() * 8);

            // (The accepted spelling is the *shorter* form at its maximum —
            // `value == CODED_NUMBER_FORM_MIN[octets] - 1` above says exactly
            // that. Each row's minimum is witnessed at its own minimum by
            // `read_utf8_coded_rfc_table_vectors`.)
        }
    }

    #[test]
    fn read_utf8_coded_truncation_restores_cursor() {
        // Mid-number EOF is the case atomicity exists for: a 6-byte form cut
        // to 5 octets must leave the cursor exactly where it started.
        let truncated = [0xFC, 0xBF, 0xBF, 0xBF, 0xBF]; // needs 6 octets, has 5
        let mut r = BitReader::new(&truncated);
        assert_eq!(r.read_utf8_coded(), Err(Error::EndOfStream));
        assert_eq!(r.bit_position(), 0);
        // Same bytes + the missing octet → succeeds; the failed attempt
        // consumed nothing.
        // 0xFC lead carries ONE payload bit (0 here), five continuations
        // carry 30 → 0x3F_FF_FF_FF, not the 0x7F… of the 0xFD lead.
        let full = [0xFC, 0xBF, 0xBF, 0xBF, 0xBF, 0xBF];
        let mut r = BitReader::new(&full);
        assert_eq!(r.read_utf8_coded().unwrap(), 0x3F_FF_FF_FF);
        assert_eq!(r.bit_position(), 48);
        // 7-byte lead at EOF after one byte: rejection of an incomplete
        // number never strands the cursor mid-number.
        let mut r = BitReader::new(&[0xFE]);
        assert_eq!(r.read_utf8_coded(), Err(Error::EndOfStream));
        assert_eq!(r.bit_position(), 0);
    }

    #[test]
    fn read_utf8_coded_two_byte_form_exhaustive() {
        // All 30×64 *minimal* two-byte pairs against the by-definition value
        // ((lead & 0x1F) << 6) | (cont & 0x3F) — no table lookup involved.
        // Leads 0xC0/0xC1 are the same arithmetic but land below 0x80, i.e.
        // outside the form's range; they are swept as rejections in
        // `read_utf8_coded_rejects_non_minimal_forms`.
        for lead in 0xC2u8..0xE0 {
            for cont in 0x80u8..0xC0 {
                let data = [lead, cont];
                let mut r = BitReader::new(&data);
                let want = (u64::from(lead & 0x1F) << 6) | u64::from(cont & 0x3F);
                assert_eq!(r.read_utf8_coded().unwrap(), want, "{lead:02X} {cont:02X}");
                assert_eq!(r.bit_position(), 16);
            }
        }
    }

    #[test]
    fn read_utf8_coded_sequential_numbers_pack_back_to_back() {
        // Mixed widths concatenated: 1 + 2 + 7 + 1 octets. The frame header
        // read path (coded number after fixed-width fields) lives or dies
        // with sequential cursor bookkeeping.
        let data = [
            0x00, // 0
            0xC2, 0x80, // 0x80
            0xFE, 0xAF, 0x9F, 0xB5, 0xA3, 0xB8, 0x80, // 51e9
            0x7F, // 127
        ];
        let mut r = BitReader::new(&data);
        assert_eq!(r.read_utf8_coded().unwrap(), 0);
        assert_eq!(r.read_utf8_coded().unwrap(), 0x80);
        assert_eq!(r.read_utf8_coded().unwrap(), 51_000_000_000);
        assert_eq!(r.read_utf8_coded().unwrap(), 127);
        assert_eq!(r.bit_position(), data.len() as usize * 8);
        assert_eq!(r.read_utf8_coded(), Err(Error::EndOfStream));
        assert_eq!(r.bit_position(), data.len() as usize * 8);
    }

    #[test]
    fn read_utf8_coded_roundtrips_at_every_bit_alignment() {
        // Differential sweep: canonical encodings (independent encoder
        // above) placed at every bit alignment 0..7, decoded back. Covers
        // the unaligned 8-bit reads inside the composite decode — where a
        // refill/cursor bug would live — for every form width.
        let boundaries: &[u64] = &[
            0,
            1,
            0x7E,
            0x7F,
            0x80,
            0x7FE,
            0x7FF,
            0x800,
            0xFFFE,
            0xFFFF,
            0x1_0000,
            0x1F_FFFE,
            0x1F_FFFF,
            0x20_0000,
            0x3_FFFF_FE,
            0x3_FFFF_FF,
            0x400_0000,
            0x7F_FF_FF_FE,
            0x7F_FF_FF_FF,
            0x8000_0000,
            0xF_FFFF_FFFE,
            0xF_FFFF_FFFF,
        ];
        let mut rng = Lcg(0xFEED_C0DE);
        let mut samples: [u64; 64 + 22] = [0; 64 + 22];
        let mut n = 0usize;
        for &b in boundaries {
            samples[n] = b;
            n += 1;
        }
        while n < samples.len() {
            let mut v = 0u64;
            for _ in 0..5 {
                v = (v << 8) | u64::from(rng.next_u8());
            }
            samples[n] = v & 0xF_FFFF_FFFF; // clamp into the 36-bit range
            n += 1;
        }

        for &val in &samples[..n] {
            let (enc, len) = encode_coded(val);
            for pad in 0..8usize {
                // Place the number at ABSOLUTE bit `pad`: pack `pad` filler
                // bits then the enc octets bit-by-bit MSB-first. (A byte-copy
                // would put the field at bit 8, not bit `pad` — an early version of
                // this harness read filler at pad == 0 and failed.)
                let total_bits = pad + len * 8;
                let mut data = [0u8; 16];
                for i in 0..total_bits {
                    // Both branches yield an unshifted 0/1 bit; the shift into
                    // the byte happens once here. (An early version pre-shifted
                    // only the filler branch — number bits then landed at the
                    // wrong bit position for any pad > 0.)
                    let bit = if i < pad {
                        // Arbitrary but non-constant filler: a run of zeros
                        // here could mask an off-by-one into the padding.
                        (i as u8 * 37 + 11) & 1
                    } else {
                        let j = i - pad;
                        (enc[j >> 3] >> (7 - (j & 7))) & 1
                    };
                    data[i >> 3] |= bit << (7 - (i & 7));
                }
                let mut r = BitReader::new(&data);
                if pad > 0 {
                    let _ = r.read_bits(pad as u32).unwrap(); // reach the alignment
                }
                let before = r.bit_position();
                assert_eq!(
                    r.read_utf8_coded().unwrap(),
                    val,
                    "val {val:#x}, pad {pad}, enc {:?}",
                    &enc[..len]
                );
                assert_eq!(r.bit_position(), before + len * 8);
            }
        }
    }

    // ---- byte_align ----------------------------------------------------------

    #[test]
    fn byte_align_drops_the_expected_bits_at_every_alignment() {
        // Walk pad = 0..7, then align. Expected drop written independently of
        // the impl's mask: `if pad == 0 { 0 } else { 8 - pad }`.
        let data = [0b1111_0001, 0b0000_1110, 0b1010_1010, 0xFF];
        for pad in 0..8usize {
            let mut r = BitReader::new(&data);
            if pad > 0 {
                let _ = r.read_bits(pad as u32).unwrap();
            }
            let want_drop = if pad == 0 { 0 } else { 8 - pad };
            assert_eq!(r.byte_align() as usize, want_drop, "pad {pad}");
            assert_eq!(r.bit_position(), pad + want_drop, "pad {pad}");
            assert_eq!(r.bit_position() % 8, 0, "pad {pad}: must land aligned");
            assert_eq!(r.bits_remaining(), data.len() * 8 - (pad + want_drop));
        }
    }

    #[test]
    fn byte_align_is_idempotent_on_an_aligned_cursor() {
        let mut r = BitReader::new(&[0xA5, 0x5A]);
        // Bit 0 is a byte boundary: no discard, no movement.
        assert_eq!(r.byte_align(), 0);
        assert_eq!(r.bit_position(), 0);
        assert_eq!(r.read_bits(3).unwrap(), 0b101);
        assert_eq!(r.byte_align(), 5);
        assert_eq!(r.bit_position(), 8);
        // Repeated calls on an aligned cursor are exact no-ops.
        assert_eq!(r.byte_align(), 0);
        assert_eq!(r.byte_align(), 0);
        assert_eq!(r.bit_position(), 8);
    }

    #[test]
    fn byte_align_returns_bits_dropped_not_the_new_position() {
        // Mid-slice at bit 11: the next boundary is bit 16, so the return is 5.
        // An implementation returning the absolute position would say 16.
        let mut r = BitReader::new(&[0u8; 4]);
        assert_eq!(r.read_bits(11).unwrap(), 0);
        assert_eq!(r.byte_align(), 5);
        assert_eq!(r.bit_position(), 16);
    }

    #[test]
    fn byte_align_never_moves_past_the_end_of_the_slice() {
        // Exhaustive over slice length × every reachable bit position. Reaching
        // an arbitrary position uses forward reads only (there is no Seek); 13
        // is an odd chunk width, so positions are hit at every alignment. The
        // invariant under test: aligning an in-bounds cursor stays in bounds —
        // and at EOF (already aligned) it is an exact no-op.
        // Zero-filled: `byte_align` is data-independent, and asserting the
        // skipped reads return 0 keeps them honest without an expectation that
        // depends on the bit pattern's alignment.
        for len in 0..=8usize {
            let all = [0u8; 8];
            let data = &all[..len];
            let total = len * 8;
            for pos in 0..=total {
                let mut r = BitReader::new(data);
                let mut left = pos;
                while left > 0 {
                    let chunk = left.min(13);
                    assert_eq!(r.read_bits(chunk as u32).unwrap(), 0);
                    left -= chunk;
                }
                let before = r.bit_position();
                let want_drop = if before % 8 == 0 { 0 } else { 8 - before % 8 };
                let drop = r.byte_align();
                assert!(drop <= 7, "len {len}, pos {pos}: drop {drop}");
                assert_eq!(drop as usize, want_drop, "len {len}, pos {pos}");
                assert_eq!(r.bit_position(), before + drop as usize);
                assert!(r.bit_position() <= total, "len {len}, pos {pos}: overrun");
                assert_eq!(r.bit_position() % 8, 0);
            }
        }
    }

    // ---- read_u8 -------------------------------------------------------------

    #[test]
    fn read_u8_matches_read_bits_at_every_alignment() {
        // Differential sweep over every absolute bit position, including
        // deliberately unaligned ones: read_u8 must equal read_bits(8) cast
        // down, and match the naive bit-by-bit oracle. This is the test that
        // pins down "read_u8 is a plain 8-bit read, not an aligned-only path".
        let mut rng = Lcg(0xB01F_00D1);
        let mut data = [0u8; 16];
        for d in &mut data {
            *d = rng.next_u8();
        }
        let total = data.len() * 8;

        for start in 0..=(total - 8) {
            let byte_start = start >> 3;
            let pad = start & 7;
            let mut a = BitReader::new(&data[byte_start..]);
            let mut b = BitReader::new(&data[byte_start..]);
            if pad > 0 {
                let _ = a.read_bits(pad as u32).unwrap();
                let _ = b.read_bits(pad as u32).unwrap();
            }
            let want = b.read_bits(8).unwrap() as u8;
            let got = a.read_u8().unwrap();
            assert_eq!(got, want, "start {start}");
            assert_eq!(a.bit_position(), pad + 8, "start {start}");
            let mut oracle = 0u32;
            for j in 0..8 {
                oracle = (oracle << 1) | ref_bit(&data, start + j);
            }
            assert_eq!(u32::from(got), oracle, "start {start}");
        }
    }

    #[test]
    fn read_u8_returns_every_byte_value_in_stream_order() {
        // All 256 byte values, read back in sequence: catches a truncation or
        // bit-order slip that a single aligned read could miss, and pins the
        // cursor advancing exactly 8 bits per call.
        let data: [u8; 256] = core::array::from_fn(|i| i as u8);
        let mut r = BitReader::new(&data);
        for (i, &want) in data.iter().enumerate() {
            assert_eq!(r.read_u8().unwrap(), want, "byte {i}");
            assert_eq!(r.bit_position(), (i + 1) * 8);
        }
        assert_eq!(r.bits_remaining(), 0);
        assert_eq!(r.read_u8(), Err(Error::EndOfStream));
    }

    #[test]
    fn read_u8_eof_leaves_cursor_untouched() {
        let mut r = BitReader::new(&[0b1010_1010]);
        assert_eq!(r.read_bits(3).unwrap(), 0b101);
        assert_eq!(r.read_u8(), Err(Error::EndOfStream)); // only 5 bits left
        assert_eq!(r.bit_position(), 3);
        assert_eq!(r.read_bits(5).unwrap(), 0b0_1010); // exact fit to the end
        assert_eq!(r.read_u8(), Err(Error::EndOfStream));
        assert_eq!(r.bit_position(), 8);
    }

    #[test]
    fn read_u8_agrees_with_peek_bits_unaligned() {
        // Mid-byte on purpose: peek(8) and read_u8 must agree on the same
        // unaligned field, and only read moves the cursor.
        // 0x36 0xC3 = 0011_0110 1100_0011; bits [5..13) = 1101_1000 = 0xD8.
        let mut r = BitReader::new(&[0x36, 0xC3]);
        assert_eq!(r.read_bits(5).unwrap(), 0b0011_0);
        for _ in 0..2 {
            assert_eq!(r.peek_bits(8).unwrap(), 0xD8);
            assert_eq!(r.bit_position(), 5);
        }
        assert_eq!(r.read_u8().unwrap(), 0xD8);
        assert_eq!(r.bit_position(), 13);
    }

    // ---- composition: the patterns the frame layer will actually use ---------

    #[test]
    fn unaligned_coded_number_then_byte_straddling_read() {
        // A SYNTHETIC unaligned composition. Its job is the bit reader: put a
        // UTF-8 coded number mid-byte, then read 8 bits that straddle two
        // source bytes.
        //
        // This is deliberately NOT billed as a FLAC frame header any more. Real
        // headers keep the coded number and the header CRC-8 byte-aligned (the
        // fixed fields sum to 32 bits — see the `byte_align` docs, and
        // `tests/frame_header_layout.rs`, which parses real libFLAC bytes).
        // What *is* real and unaligned is a footer read right after subframe
        // padding; that case lives in `footer_padding_pattern_...` below.
        //
        // History, kept so the mistake is not repeated: this test used to be
        // billed as "a faithful FLAC frame header" and asserted 31 fixed bits
        // with a 3-bit channels field, concluding the header CRC-8 is never
        // byte aligned. Both the width and the conclusion were wrong, and the
        // hand-packed bytes were not a valid header at all — parsed correctly
        // they decode as a 4-channel frame with reserved bits set. The lesson
        // generalises past this test: **a hand-packed vector cannot witness a
        // field-width claim; it only records what the author believed.** Real
        // encoder output is the only witness for that, so that is what
        // tests/frame_header_layout.rs uses.
        //
        // Bytes re-derived with an independent Python bit packer (the habit
        // that earlier caught sync 0x3FF8 being no sync code at all, and
        // blocksize code 9 being 512 rather than 2048). The composition is
        // arbitrary but the coded number must be written in its MINIMAL form,
        // because the reader rejects the others (see `read_utf8_coded`) — an
        // earlier version of these bytes spelled 61 in two octets, which was
        // silently non-minimal and stopped decoding once the rule landed:
        //   10111001 01110100 10110110 11011010
        //   ^^^ lead-in = 0b101, cursor now at bit 3
        //      ^^^^^^^^ 11001011 = 2-octet lead at bit 3, payload 0b01011
        //               ^^^^^^^^ 10100101 continuation, payload 0b100101
        //                       -> coded number 0b01011_100101 = 0x2E5, bit 19
        //                          ^^^^^^^^ bits [19..27) = 0b10110110 = 0xB6,
        //                                   straddling data[2] and data[3]
        let data = [0xB9, 0x74, 0xB6, 0xDA];
        let mut r = BitReader::new(&data);

        assert_eq!(r.read_bits(3).unwrap(), 0b101, "synthetic lead-in");
        assert_eq!(r.bit_position(), 3);
        assert_ne!(r.bit_position() % 8, 0, "setup must be unaligned");

        assert_eq!(r.read_utf8_coded().unwrap(), 0x2E5, "coded number");
        assert_eq!(r.bit_position(), 19, "3 + 16 bits of coded number");
        assert_ne!(r.bit_position() % 8, 0, "still unaligned");

        assert_eq!(r.read_u8().unwrap(), 0xB6, "unaligned 8-bit read");
        assert_eq!(r.bit_position(), 27);
        assert_eq!(r.bits_remaining(), 5);
    }

    #[test]
    fn footer_padding_pattern_aligns_then_reads_the_next_byte() {
        // The other real pattern: subframe data ends mid-byte, padding bits pad
        // to the boundary, then the CRC-16 (two whole bytes) follows. Across
        // every padding width, `byte_align` + `read_u8` must reach the *whole*
        // next byte — never the field the padding bits came from.
        //
        // 0xAA 0x5C 0xA5: target byte is 0x5C. Hand-checked guard below — the
        // seven unaligned 8-bit reads are 0x54, 0xA9, 0x52, 0xA5, 0x4B, 0x97,
        // 0x2E, none of which is 0x5C, so a reader that skipped byte_align
        // could not pass this test.
        let data = [0xAA, 0x5C, 0xA5];
        for pad in 1..8usize {
            let mut r = BitReader::new(&data);
            let _ = r.read_bits(pad as u32).unwrap(); // mid-field padding bits
            assert_eq!(r.byte_align() as usize, 8 - pad, "pad {pad}");
            assert_eq!(r.bit_position(), 8, "pad {pad}");
            assert_eq!(r.read_u8().unwrap(), 0x5C, "pad {pad}");
            assert_eq!(r.read_u8().unwrap(), 0xA5, "pad {pad}: second CRC byte");
            assert_eq!(r.bit_position(), 24, "pad {pad}");

            // The counterfactual: no alignment step reads a different byte.
            let mut naive = BitReader::new(&data);
            let _ = naive.read_bits(pad as u32).unwrap();
            assert_ne!(naive.read_u8().unwrap(), 0x5C, "pad {pad}");
        }
    }

    // ---- position bookkeeping ----------------------------------------------

    #[test]
    fn position_and_remaining_track_reads() {
        let mut r = BitReader::new(&[0u8; 2]); // 16 bits
        assert_eq!(r.bit_position(), 0);
        assert_eq!(r.bits_remaining(), 16);
        let _ = r.read_bits(5).unwrap();
        assert_eq!(r.bit_position(), 5);
        assert_eq!(r.bits_remaining(), 11);
        let _ = r.read_bits(11).unwrap();
        assert_eq!(r.bit_position(), 16);
        assert_eq!(r.bits_remaining(), 0);
    }

    // ---- differential: bit-by-bit vs wide reads vs naive oracle ------------

    #[test]
    fn wide_reads_agree_with_bit_by_bit_on_random_data() {
        let mut rng = Lcg(0xDEAD_BEEF);
        for trial in 0..64 {
            let len = 1 + (rng.next_u8() as usize % 8); // 1..=8 bytes
            let mut data = [0u8; 8];
            for d in data.iter_mut().take(len) {
                *d = rng.next_u8();
            }
            let data = &data[..len];
            let total = len * 8;

            // Oracle: every bit, from the independent ref_bit implementation.
            let mut bit_r = BitReader::new(data);
            for i in 0..total {
                assert_eq!(
                    bit_r.read_bits(1).unwrap(),
                    ref_bit(data, i),
                    "trial {trial}: bit {i} of {data:02X?}"
                );
            }

            // Wide reads (odd widths hit nasty alignments) must equal the
            // same bits assembled one at a time.
            for width in [7u32, 13, 31, 32] {
                let mut wide_r = BitReader::new(data);
                let mut bits_r = BitReader::new(data);
                while wide_r.bits_remaining() >= width as usize {
                    let wide = wide_r.read_bits(width).unwrap();
                    let mut assembled = 0u32;
                    for _ in 0..width {
                        assembled = (assembled << 1) | bits_r.read_bits(1).unwrap();
                    }
                    assert_eq!(wide, assembled, "trial {trial}: width {width}");
                    assert_eq!(wide_r.bit_position(), bits_r.bit_position());
                }
                assert_eq!(wide_r.bits_remaining(), bits_r.bits_remaining());
            }
        }
    }

    #[test]
    fn unaligned_sweeps_cover_every_bit_offset() {
        // For every absolute start bit position and every legal width, check
        // the read against the independent oracle. Exhaustive over
        // (alignment x width) — the space where refill/alignment bugs live.
        // Reaching an arbitrary bit position uses forward reads only (there is
        // no Seek): a byte-aligned subslice plus a `pad`-bit skip.
        let mut rng = Lcg(0x1234_5678);
        let mut data = [0u8; 16];
        for (i, d) in data.iter_mut().enumerate() {
            *d = rng.next_u8() ^ ((i as u8) << 3);
        }
        let total = data.len() * 8;

        for start in 0..total {
            let byte_start = start >> 3;
            let pad = start & 7;
            for width in 1..=32usize {
                let mut r = BitReader::new(&data[byte_start..]);
                if pad > 0 {
                    let _ = r.read_bits(pad as u32).unwrap();
                }
                match r.read_bits(width as u32) {
                    Ok(val) => {
                        assert_eq!(r.bit_position(), pad + width);
                        let mut expect = 0u32;
                        for j in 0..width {
                            expect = (expect << 1) | ref_bit(&data, start + j);
                        }
                        assert_eq!(val, expect, "start {start}, width {width}");
                    }
                    // Only legitimate failure: fewer than `width` bits left
                    // from `start` to the end of the slice.
                    Err(Error::EndOfStream) => assert!(start + width > total),
                    Err(e) => panic!("unexpected error at start {start}, width {width}: {e:?}"),
                }
            }
        }
    }

    // ---- read_wasted_bits --------------------------------------------------

    #[test]
    fn read_wasted_bits_flag_clear_consumes_only_the_flag() {
        let mut r = BitReader::new(&[0b0111_1111]);
        assert_eq!(r.read_wasted_bits().unwrap(), 0);
        assert_eq!(r.bit_position(), 1);
    }

    #[test]
    fn read_wasted_bits_hand_packed_runs() {
        // k=1: flag + terminator, no zeros: bits "11".
        let mut r = BitReader::new(&[0b1100_0000]);
        assert_eq!(r.read_wasted_bits().unwrap(), 1);
        assert_eq!(r.bit_position(), 2);
        // k=3: 1, 0, 0, 1 → "1001".
        let mut r = BitReader::new(&[0b1001_0000]);
        assert_eq!(r.read_wasted_bits().unwrap(), 3);
        assert_eq!(r.bit_position(), 4);
        // k=5: flag, k−1 = 4 zeros, terminator → bits 1,0,0,0,0,1 = 0b100001
        // from the MSB = 0b1000_0100.
        let mut r = BitReader::new(&[0b1000_0100]);
        assert_eq!(r.read_wasted_bits().unwrap(), 5);
        assert_eq!(r.bit_position(), 6);
        // k=8: flag, seven zeros, terminator → 9 bits, crossing into byte 2
        // (bit 8 is byte 1's MSB).
        let mut r = BitReader::new(&[0x80, 0x80]);
        assert_eq!(r.read_wasted_bits().unwrap(), 8);
        assert_eq!(r.bit_position(), 9);
    }

    #[test]
    fn read_wasted_bits_long_run() {
        // k=40: flag, 39 zeros, terminator → 41 bits. Byte 0 = 0x80, bytes
        // 1..5 all zero, bit 40 = byte 5's MSB. Wide runs stay the bits
        // layer's problem to read, not to judge (frame bps gates them).
        let mut r = BitReader::new(&[0x80, 0x00, 0x00, 0x00, 0x00, 0x80]);
        assert_eq!(r.read_wasted_bits().unwrap(), 40);
        assert_eq!(r.bit_position(), 41);
    }

    #[test]
    fn read_wasted_bits_unterminated_run_restores_cursor() {
        // flag + zeros, no terminator before EOF.
        let mut r = BitReader::new(&[0x80, 0x00]);
        assert_eq!(r.read_wasted_bits(), Err(Error::EndOfStream));
        assert_eq!(r.bit_position(), 0);
        // Mid-slice: pad three bits first (0x10's top three bits are 000);
        // the flag is bit 3 (0x10 = 0001_0000), then zeros run to EOF. A
        // failure must restore to three, not zero and not into the run.
        let mut r = BitReader::new(&[0x10, 0x00, 0x00]);
        assert_eq!(r.read_bits(3).unwrap(), 0b000);
        assert_eq!(r.read_wasted_bits(), Err(Error::EndOfStream));
        assert_eq!(r.bit_position(), 3);
        // Empty slice: the flag read itself fails.
        let mut r = BitReader::new(&[]);
        assert_eq!(r.read_wasted_bits(), Err(Error::EndOfStream));
        assert_eq!(r.bit_position(), 0);
    }

    #[test]
    fn read_wasted_bits_matches_naive_oracle_at_every_alignment() {
        // Independent oracle: a plain bit-by-bit loop over `ref_bit`, run
        // from every start position; disagreement or a terminator beyond
        // the slice is a failure either way.
        let mut data = [0u8; 9];
        let mut rng = Lcg(0xCAFE_F00D);
        for d in data.iter_mut() {
            *d = rng.next_u8();
        }
        let total = data.len() * 8;
        for start in 0..total {
            // Naive expectation, computed first.
            let mut want: Option<u32> = None;
            if ref_bit(&data, start) == 0 {
                want = Some(0);
            } else {
                let mut zeros = 0u32;
                let mut pos = start + 1;
                loop {
                    if pos >= total {
                        break;
                    }
                    if ref_bit(&data, pos) == 1 {
                        want = Some(zeros + 1);
                        break;
                    }
                    zeros += 1;
                    pos += 1;
                }
            }
            let mut r = BitReader::new(&data);
            let mut left = start;
            while left > 0 {
                let chunk = left.min(31);
                let _ = r.read_bits(chunk as u32).unwrap();
                left -= chunk;
            }
            match (want, r.read_wasted_bits()) {
                (Some(k), Ok(got)) => {
                    assert_eq!(got, k, "start {start}");
                    // flag(1) + (k−1) zeros + terminator(1): k+1 bits for k≥1,
                    // just the flag for k=0.
                    let consumed = if k == 0 { 1 } else { k + 1 };
                    assert_eq!(r.bit_position(), start + consumed as usize, "start {start}");
                }
                (None, Err(Error::EndOfStream)) => {
                    assert_eq!(
                        r.bit_position(),
                        start,
                        "start {start}: failed read must restore"
                    );
                }
                (want, got) => panic!("start {start}: oracle says {want:?}, reader {got:?}"),
            }
        }
    }

    // ---- E2a accumulator window invariants ---------------------------------

    fn assert_window(r: &BitReader<'_>, where_: &str) {
        assert!(r.pos <= r.data.len(), "{where_}: pos past slice");
        assert!(
            (r.valid as usize) <= r.pos * 8,
            "{where_}: valid exceeds consumed bits"
        );
        assert!(r.valid <= 32, "{where_}: valid > 32");
        let low_mask = match r.valid {
            0 => u32::MAX,
            32.. => 0u32,
            v => (1u32 << (32 - v)) - 1,
        };
        assert_eq!(r.acc & low_mask, 0, "{where_}: acc dirty below valid");
    }

    // ---- crc8 / crc16 (Phase 2 step 5 landed 2026-11-03) -------------------

    /// Naive bit-serial CRC oracles (both widths): shift one bit at a time,
    /// feedback tap XORs the incoming bit — a different mechanism from the
    /// byte-fed LFSR in `crc8`/`crc16`, per the differential-witness rule.
    fn crc8_serial(data: &[u8]) -> u8 {
        let mut crc = 0u8;
        for &byte in data {
            for bi in (0..8).rev() {
                let top = ((crc >> 7) & 1) ^ ((byte >> bi) & 1);
                crc = (crc << 1) & 0xFF;
                if top != 0 {
                    crc ^= 0x07;
                }
            }
        }
        crc
    }

    fn crc16_serial(data: &[u8]) -> u16 {
        let mut crc = 0u16;
        for &byte in data {
            for bi in (0..8).rev() {
                let top = ((crc >> 15) & 1) ^ u16::from((byte >> bi) & 1);
                crc = (crc << 1) & 0xFFFF;
                if top != 0 {
                    crc ^= 0x8005;
                }
            }
        }
        crc
    }

    #[test]
    fn crc_helpers_match_serial_oracles_on_pseudo_random_sweeps() {
        let mut rng = Lcg(0x5EED_C0DE);
        let mut data = [0u8; 256];
        for b in data.iter_mut() {
            *b = rng.next_u8();
        }
        for len in [0usize, 1, 2, 3, 7, 8, 31, 32, 33, 64, 127, 200, 256] {
            let d = &data[..len];
            assert_eq!(crc8(d), crc8_serial(d), "crc8 len {len}");
            assert_eq!(crc16(d), crc16_serial(d), "crc16 len {len}");
        }
    }

    #[test]
    fn crc_helpers_carry_the_witnessed_check_values() {
        // crc8: the published CRC-8 (poly 0x07) check value. crc16: the
        // landing's derived value — three independent mechanisms (byte-fed
        // LFSR, bit-serial, GF(2) polynomial remainder) agree on it AND it
        // reproduces the stored footer of every committed real frame
        // (FLAC.md's step-5 entry; deliberately not a recalled constant).
        assert_eq!(crc8(b"123456789"), 0xF4);
        assert_eq!(crc16(b"123456789"), 0xFEE8);
        // Both CRCs run from a zero init — the measured correction to this
        // scaffold's "init 0xFFFF" comment. Empty input returns the init.
        assert_eq!(crc8(&[]), 0x00);
        assert_eq!(crc16(&[]), 0x0000);
    }

    #[test]
    fn bytes_between_views_match_the_cursor_window() {
        let data = [0xABu8, 0xCD, 0xEF, 0x01, 0x23];
        let r = BitReader::new(&data);
        assert_eq!(r.bytes_between(0, 40), Some(&data[..]));
        assert_eq!(r.bytes_between(8, 24), Some(&data[1..3]));
        assert_eq!(r.bytes_between(8, 8), Some(&[][..]));
        assert_eq!(r.bytes_between(3, 24), None, "unaligned start");
        assert_eq!(r.bytes_between(0, 41), None, "unaligned end");
        assert_eq!(r.bytes_between(24, 8), None, "reversed bounds");
        assert_eq!(r.bytes_between(0, 48), None, "out of range");
        // A view is not a read: the cursor never moves. `r2` is deliberately
        // not `mut` — `bit_position` is `&self`, so this test would not
        // compile if a view ever started advancing the cursor.
        let r2 = BitReader::new(&data);
        let _ = r2.bytes_between(0, 16);
        assert_eq!(r2.bit_position(), 0);
    }

    #[test]
    fn accumulator_window_invariants_hold_on_mixed_width_sweeps() {
        let widths: [u32; 18] = [
            1, 1, 1, 2, 3, 5, 7, 8, 9, 12, 16, 20, 24, 25, 31, 32, 33, 11,
        ];
        for seed in [0x1234_5678u32, 0xDEAD_BEEF, 0x0F0F_F0F0, 0x88C7_7A3B] {
            let mut rng = Lcg(seed);
            let mut data = [0u8; 40];
            for b in data.iter_mut() {
                *b = rng.next_u8();
            }
            let mut r = BitReader::new(&data);
            assert_window(&r, "fresh");
            for round in 0..250usize {
                let n = widths[(round * 7 + data[round % data.len()] as usize) % widths.len()];
                let _ = r.read_bits(n);
                assert_window(&r, "sweep-read");
                for pn in [1u32, 7, 32] {
                    let _ = r.peek_bits(pn);
                    assert_window(&r, "sweep-peek");
                }
            }
        }
    }

    // ---- window-domain bridge (split-ladder item 5, PR 1) ------------------

    // Const-context witness: PR 3's twin needs this translation inside a
    // `const fn` table builder (CTZ8 / REV8 / BRUIJN_INDEX), so `rev32` being
    // `const` is part of the surface this PR ships. A compile-time fact, not a
    // test: 0x1234_5678's reversed form, derived here rather than recalled.
    const REV32_IN_CONST: u32 = rev32(0x1234_5678);
    const _: () = assert!(REV32_IN_CONST == 0x1E6A_2C48);

    /// Naive bit-loop oracle for [`rev32`] — loop over bit positions, a
    /// different mechanism from the shift/mask expansion `reverse_bits` lowers
    /// to. Per this file's differential rule the implementation is never judged
    /// against itself.
    fn ref_rev32(x: u32) -> u32 {
        let mut y = 0u32;
        for i in 0..32 {
            if (x >> i) & 1 == 1 {
                y |= 1 << (31 - i);
            }
        }
        y
    }

    /// The content claim of the bridge, spelled out bit by bit so it never
    /// leans on `rev32`: the reader's valid region (`acc`'s top `valid` bits)
    /// is exactly the low `valid` bits of `w`, and every bit from there down is
    /// zero on both sides. This is the only content witness, so the hand-out,
    /// the exact take-back, and the clamped take-back all go through it.
    fn assert_window_content(r: &BitReader<'_>, k: &KernelWindow, where_: &str, at: usize) {
        for i in 0..32u32 {
            let want = if i < r.valid { (k.w >> i) & 1 } else { 0 };
            let got = (r.acc >> (31 - i)) & 1;
            assert_eq!(
                got,
                want,
                "{where_} step {at}: acc bit {} vs window bit {i}",
                31 - i
            );
        }
    }

    /// What a hand-out owes: the reader's own state, in the other domain.
    /// Field identity (`v`/`ptr`/`end`); the zero tail seen from the **window
    /// side** (bits ≥ `v` of `w` are zero — the same fact `assert_window`
    /// checks from the reader side, and it runs here too); the byte-pointer
    /// invariant `P + v = pos * 8`; the bit-level definition judged against the
    /// raw bytes with the naive `ref_bit` oracle (never against a hand-packed
    /// vector — a hand-packed vector cannot witness a bit-layout claim); and the
    /// two-domain content mapping.
    fn assert_hand_out_is_the_reader(r: &BitReader<'_>, k: &KernelWindow, where_: &str, at: usize) {
        assert_eq!(k.v, r.valid, "{where_} step {at}: v != valid");
        assert_eq!(k.ptr, r.pos, "{where_} step {at}: ptr != pos");
        assert_eq!(k.end, r.data.len(), "{where_} step {at}: end != len");
        assert!(k.v <= 32, "{where_} step {at}: v > 32");
        // The shift-by-32 leg is spelled rather than shifted — the same leg
        // PR 3's kernel docs pin for the q = 31 case.
        let above = if k.v >= 32 { 0 } else { k.w >> k.v };
        assert_eq!(above, 0, "{where_} step {at}: bits >= v of w are not zero");
        assert_eq!(
            r.bit_position() + k.v as usize,
            r.pos * 8,
            "{where_} step {at}: P + v != pos * 8"
        );
        let p = r.bit_position();
        for i in 0..k.v as usize {
            assert_eq!(
                (k.w >> i) & 1,
                ref_bit(r.data, p + i),
                "{where_} step {at}: window bit {i} != stream bit {}",
                p + i
            );
        }
        assert_window(r, where_);
        assert_window_content(r, k, where_, at);
    }

    /// A reader at bit `off` holding exactly `v0` valid bits: filled, then
    /// drained bit-for-bit through the reader's own `consume`. That is the
    /// state a partition's sample loop sits in mid-sample — window partly
    /// consumed, byte cursor untouched — and the take-back test below walks
    /// every count the reader can hold at that alignment.
    fn window_holding<'a>(data: &'a [u8], off: u32, v0: u32) -> BitReader<'a> {
        let mut r = BitReader::new(data);
        skip(&mut r, off);
        r.refill();
        let have = r.valid;
        assert!(v0 <= have, "only counts the window can actually hold");
        // Bit at a time, not `consume(have - v0)`: `consume`'s contract does
        // not actually hold at `n = 32` (its `acc <<= n` is the shift-by-32
        // the reader spells everywhere else — `read_bits` keeps `n = 32` out
        // of its fast path, and `peek_at` takes that width through a `u64`).
        // This helper's job is to *reach* the empty window, not to step on
        // that edge.
        for _ in v0..have {
            r.consume(1);
        }
        r
    }

    /// A reader mid-slice at bit 12 of a 6-byte region: the shape a header
    /// leaves behind (byte cursor at 4, window holding 20 bits).
    fn positioned(data: &[u8]) -> BitReader<'_> {
        let mut r = BitReader::new(data);
        r.read_bits(12).unwrap();
        r
    }

    #[test]
    fn rev32_matches_a_bit_loop_oracle_and_is_its_own_inverse() {
        // Edge values: the two ends of the map, the palindromes, the
        // nibble/byte-symmetric patterns, and one 2-cycle pair (each is the
        // other's image, so a wrong reversal amount shows up as a fixed point).
        const EDGE: [u32; 11] = [
            0,
            1,
            0x8000_0000,
            0x8000_0001, // palindromic: bit 0 and bit 31 trade places
            u32::MAX,
            0x0FFF_FFFF,
            0xF000_0000,
            0x0F0F_F0F0,
            0x1234_5678,
            0xDEAD_BEEF,
            0x1E6A_2C48, // the reverse of the row above
        ];
        let mut cases: [u32; 2090] = [0; 2090]; // 11 + 64 + 2000 + 15 spare
        let mut n = 0usize;
        for x in EDGE {
            cases[n] = x;
            n += 1;
        }
        // Every single-bit position and its complement: both directions of the
        // map, one set bit at a time.
        for i in 0..32 {
            cases[n] = 1u32 << i;
            cases[n + 1] = !(1u32 << i);
            n += 2;
        }
        // Pseudo-random sweep, four LCG bytes per value.
        let mut rng = Lcg(0xB0BA_C0DE);
        while n < 2075 {
            cases[n] = u32::from(rng.next_u8())
                | (u32::from(rng.next_u8()) << 8)
                | (u32::from(rng.next_u8()) << 16)
                | (u32::from(rng.next_u8()) << 24);
            n += 1;
        }
        assert_eq!(n, 2075, "case list assembled");
        for &x in &cases[..n] {
            assert_eq!(rev32(x), ref_rev32(x), "x {x:#010X}");
            // The map is its own inverse — which is why the hand-out and the
            // resync are the same call, here and in the probe.
            assert_eq!(rev32(rev32(x)), x, "round trip x {x:#010X}");
        }
    }

    /// The hand-out half over the live reader's state space: every bit
    /// alignment 0..7 crossed with the refill states a mixed-width sweep
    /// reaches (the same sweep shape as the E2a invariant test), the window
    /// judged at every step — and once more on the exhausted tail, where no
    /// refill is possible at all.
    #[test]
    fn kernel_window_carries_the_readers_window_at_every_alignment() {
        let widths: [u32; 14] = [1, 2, 3, 5, 7, 8, 9, 12, 16, 20, 24, 25, 31, 32];
        let mut seen_valid = [false; 33];
        let mut snapshots = 0usize;
        for (si, seed) in [0x1234_5678u32, 0xDEAD_BEEF, 0x0F0F_F0F0, 0x88C7_7A3B]
            .into_iter()
            .enumerate()
        {
            let mut rng = Lcg(seed);
            let mut data = [0u8; 48];
            for b in data.iter_mut() {
                *b = rng.next_u8();
            }
            for off in 0..8usize {
                let mut r = BitReader::new(&data);
                skip(&mut r, off as u32);
                for round in 0..120usize {
                    let k = r.kernel_window();
                    assert_hand_out_is_the_reader(&r, &k, "sweep", si * 10000 + off * 100 + round);
                    seen_valid[k.v as usize] = true;
                    snapshots += 1;
                    let n = widths[(round * 7 + data[round % data.len()] as usize) % widths.len()];
                    let _ = r.read_bits(n);
                }
                // The exhausted tail: `valid` can only shrink here, and these
                // are the sub-byte states a real last frame sits in.
                let k = r.kernel_window();
                assert_hand_out_is_the_reader(&r, &k, "tail", si * 10000 + off * 100 + 999);
                seen_valid[k.v as usize] = true;
                snapshots += 1;
            }
        }
        // A bit-at-a-time drain from a full window: judged at every step, so
        // it walks `valid` through every count 32..=0 by construction. The
        // mixed-width sweep above hits most of those counts by accident of the
        // refill rule; this leg is what makes the coverage claim below a fact
        // rather than a coincidence.
        let mut tail_data = [0u8; 48];
        for (i, b) in tail_data.iter_mut().enumerate() {
            *b = 0xA7u8.wrapping_mul(i as u8 + 1);
        }
        for off in 0..8usize {
            let mut r = BitReader::new(&tail_data);
            skip(&mut r, off as u32);
            r.refill();
            loop {
                let k = r.kernel_window();
                assert_hand_out_is_the_reader(&r, &k, "drain", off * 100 + k.v as usize);
                seen_valid[k.v as usize] = true;
                snapshots += 1;
                if r.valid == 0 {
                    break;
                }
                r.consume(1);
            }
        }
        // Coverage self-check, so the witness above cannot silently degenerate
        // into a single refill state: every `valid` count 0..=32 is judged, and
        // the total number of judged windows is what the loops above add up to
        // (measured, pinned, and it fails if a loop bound is ever edited out
        // from under the witness).
        let reached = seen_valid.iter().filter(|b| **b).count();
        assert_eq!(reached, 33, "distinct valid counts exercised");
        assert_eq!(snapshots, 4_108, "window snapshots judged");
    }

    /// The snapshot/restore differential: one reader driven with a
    /// hand-out/take-back pair at every step, a twin driven with no bridge
    /// contact at all, both through the same read/peek sequence over the same
    /// bytes — identical results, identical `bit_position()` and
    /// `bits_remaining()`, and an identical window, at every step and at the
    /// end (which for these lengths is deep into EOF).
    #[test]
    fn bridge_round_trip_leaves_every_later_read_unchanged() {
        let widths: [u32; 18] = [
            1, 1, 1, 2, 3, 5, 7, 8, 9, 12, 16, 20, 24, 25, 31, 32, 33, 11,
        ];
        for (si, seed) in [0x1234_5678u32, 0xDEAD_BEEF, 0x0F0F_F0F0, 0x88C7_7A3B]
            .into_iter()
            .enumerate()
        {
            let mut rng = Lcg(seed);
            let mut data = [0u8; 40];
            for b in data.iter_mut() {
                *b = rng.next_u8();
            }
            for off in 0..8usize {
                let mut a = BitReader::new(&data);
                let mut b = BitReader::new(&data);
                skip(&mut a, off as u32);
                skip(&mut b, off as u32);
                for round in 0..250usize {
                    let at = si * 100000 + off * 1000 + round;
                    // The bridge, at every step: handed out and straight back.
                    let snap = a.kernel_window();
                    a.set_kernel_window(snap);
                    assert_window(&a, "round-trip step");
                    assert_window_content(&a, &snap, "round-trip identity", at);
                    // Nothing moved: same next bit, same valid count, same byte
                    // cursor, same bound — judged against the twin that never
                    // touched a window.
                    assert_eq!(a.kernel_window(), b.kernel_window(), "window {at}");
                    assert_eq!(a.bit_position(), b.bit_position(), "cursor {at}");
                    assert_eq!(a.bits_remaining(), b.bits_remaining(), "remaining {at}");

                    let n = widths[(round * 7 + data[round % data.len()] as usize) % widths.len()];
                    assert_eq!(a.read_bits(n), b.read_bits(n), "read {n} at {at}");
                    for pn in [1u32, 7, 32] {
                        assert_eq!(a.peek_bits(pn), b.peek_bits(pn), "peek {pn} at {at}");
                    }
                    assert_eq!(a.bit_position(), b.bit_position(), "cursor after {at}");
                    assert_eq!(a.bits_remaining(), b.bits_remaining(), "rem after {at}");
                    assert_window(&a, "after step");
                }
                assert_eq!(a.bit_position(), b.bit_position(), "final cursor");
                assert_eq!(a.kernel_window(), b.kernel_window(), "final window");
            }
        }
    }

    /// The take-back half, and the property PR 2's kernel will rest on: a
    /// kernel consumes by shifting `w` — its only way to consume — so handing
    /// back a consumed window must install exactly what the same consumption
    /// through the reader's own primitives installs. Judged against a live
    /// reader consuming bit-at-a-time with `consume(1)` (a different
    /// mechanism) at every alignment, at every count a window can hold at that
    /// alignment, and for every `n` such a window can supply. The two states
    /// then have to be identical field-for-field — `pos`, `valid`, `acc`
    /// compared directly, not inferred from observables — and every later read
    /// has to agree.
    #[test]
    fn take_back_after_consuming_from_the_window_matches_bitwise_consumption() {
        let mut seen_n = [false; 33];
        let mut seen_v0 = [false; 33];
        for off in 0..8usize {
            let mut rng = Lcg(0x5EED_1000 + (off as u32) * 64);
            let mut data = [0u8; 40];
            for b in data.iter_mut() {
                *b = rng.next_u8();
            }
            // The largest count this alignment can hold — 32 at a byte
            // boundary, less as the refill rule leaves bits behind — and then
            // every count down from it.
            let have = {
                let mut r = BitReader::new(&data);
                skip(&mut r, off as u32);
                r.refill();
                r.valid
            };
            for v0 in 0..=have {
                seen_v0[v0 as usize] = true;
                for n in 0..=v0 {
                    // The kernel's way: shift the consumed bits out of the
                    // window and leave `ptr` alone — it had enough buffered,
                    // and a kernel only moves `ptr` when it refills.
                    let mut a = window_holding(&data, off as u32, v0);
                    let mut k = a.kernel_window();
                    k.w = if n >= 32 { 0 } else { k.w >> n }; // no shift-by-32 leg
                    k.v -= n;
                    a.set_kernel_window(k);
                    // The reader's way, one bit at a time.
                    let mut b = window_holding(&data, off as u32, v0);
                    for _ in 0..n {
                        b.consume(1);
                    }

                    assert_window(&a, "after kernel consume");
                    assert_window(&b, "after bitwise consume");
                    assert_window_content(&a, &k, "take-back content", n as usize);
                    assert_eq!(a.pos, b.pos, "pos after consuming {n} of {v0}");
                    assert_eq!(a.valid, b.valid, "valid after consuming {n} of {v0}");
                    assert_eq!(a.acc, b.acc, "acc after consuming {n} of {v0}");
                    assert_eq!(a.bit_position(), b.bit_position(), "cursor after {n}");
                    assert_eq!(
                        a.bits_remaining(),
                        b.bits_remaining(),
                        "remaining after {n}"
                    );
                    // The window the kernel holds and the window the reader
                    // would hand out are the same value: the state the next
                    // kernel iteration sees.
                    assert_eq!(k, b.kernel_window(), "window after consuming {n}");
                    seen_n[n as usize] = true;

                    // And every later read agrees, from every width.
                    for m in [1u32, 3, 8, 16, 24, 25, 31, 32] {
                        assert_eq!(
                            a.read_bits(m),
                            b.read_bits(m),
                            "read {m} after consuming {n} of {v0}"
                        );
                        assert_eq!(a.bit_position(), b.bit_position(), "cursor read {m}");
                        assert_window(&a, "after read");
                    }
                }
            }
        }
        // Coverage self-check: every consumption count 0..=32 was exercised,
        // and every window size it could be exercised against — 33 of each,
        // which is what the loops above build at a byte alignment.
        assert_eq!(
            seen_n.iter().filter(|b| **b).count(),
            33,
            "every n exercised"
        );
        assert_eq!(
            seen_v0.iter().filter(|b| **b).count(),
            33,
            "every window size exercised"
        );
    }

    /// Totality: the clamps a hand-made window can trip, each named, each
    /// producing a *well-formed* reader (`assert_window`) whose window is the
    /// input clipped to what the reader can represent. None of these is
    /// reachable for a window this reader handed out — the byte-pointer
    /// invariant alone makes `v <= ptr * 8` true at every cursor a legal walk
    /// reaches, which is what the test above witnesses — so they exist for the
    /// one thing PR 1 cannot control: a window some other code built.
    #[test]
    fn take_back_clamps_windows_that_break_the_contract() {
        let data = [0xE7u8, 0x3C, 0x5A, 0xA5, 0xC3, 0x0F]; // 48 bits

        // (a) `v` over the window's 32-bit width: clipped to 32 — and the
        // cursor that follows from it, wherever a 32-bit window at that byte
        // cursor sits. Self-consistency is the contract; a take-back installs
        // state, it does not only advance it.
        let mut r = positioned(&data);
        let mut k = r.kernel_window();
        k.v = 40;
        r.set_kernel_window(k);
        assert_window(&r, "clamp v over 32");
        assert_eq!(r.valid, 32, "v clipped to the window width");
        assert_eq!(
            r.bit_position(),
            r.pos * 8 - 32,
            "cursor follows the window"
        );
        assert_eq!(r.bit_position(), 0, "a 32-bit window at pos 4 starts at 0");
        assert_window_content(&r, &k, "clamp v over 32", 0);

        // (b) Dirty window: bits at and above `v` non-zero. The zero tail is a
        // both-domains fact, so the dirty bits are dropped rather than carried
        // into `acc` — and they are not counted as extra validity either.
        let mut r = positioned(&data);
        let mut k = r.kernel_window();
        let v = k.v;
        k.w |= !(u32::MAX >> v); // deliberately dirty
        assert_ne!(
            if v >= 32 { 0 } else { k.w >> v },
            0,
            "dirty by construction"
        );
        r.set_kernel_window(k);
        assert_window(&r, "clamp dirty w");
        assert_eq!(r.valid, v, "a dirty tail is not extra validity");
        assert_window_content(&r, &k, "clamp dirty w", 1);

        // (c) `ptr` past the slice: clipped to the slice, so the cursor cannot
        // run off the end of the data. `usize::MAX` is the same leg with a
        // bigger number — and it must not overflow `pos * 8`.
        for (i, ptr) in [data.len() + 7usize, usize::MAX].into_iter().enumerate() {
            let mut r = positioned(&data);
            let mut k = r.kernel_window();
            k.ptr = ptr;
            r.set_kernel_window(k);
            assert_window(&r, "clamp ptr");
            assert_eq!(r.pos, data.len(), "byte cursor clipped to the slice");
            assert_eq!(
                r.bits_remaining(),
                r.valid as usize,
                "nothing past the slice"
            );
            assert_eq!(r.bit_position(), data.len() * 8 - r.valid as usize);
            assert_window_content(&r, &k, "clamp ptr", i);
        }

        // (d) A window whose declared `end` is *shorter* than this reader's
        // slice does not shrink the reader: the reader is the authority on its
        // own region, and `end` is only the bound it handed out. Pinned as a
        // decision, not left to the accident of which clamp is written first.
        let mut r = positioned(&data);
        let mut k = r.kernel_window();
        k.end = 1;
        k.ptr = 5;
        r.set_kernel_window(k);
        assert_window(&r, "stale end");
        assert_eq!(r.pos, 5, "the reader's bound wins over a stale end");
        assert_eq!(r.bit_position(), 40 - r.valid as usize, "cursor at bit 20");
        assert_eq!(r.bits_remaining(), 48 - 20, "and 28 bits are still ahead");
        assert_window_content(&r, &k, "stale end", 3);

        // (e) `v` above what the byte cursor can support: the reader's own
        // invariant is `valid <= pos * 8`, since `bit_position()` subtracts
        // `valid`. Clipped, and the window contributes nothing.
        let mut r = positioned(&data);
        r.set_kernel_window(KernelWindow {
            w: 0xFFFF_FFFF,
            v: 8,
            ptr: 0,
            end: data.len(),
        });
        assert_window(&r, "clamp v over pos times 8");
        assert_eq!(r.valid, 0, "clipped to what pos * 8 can carry");
        assert_eq!(r.acc, 0, "a fully clipped window installs no bits");
        assert_eq!(r.bit_position(), 0, "cursor stays at the start");
        assert_eq!(r.bits_remaining(), data.len() * 8, "the slice is intact");

        // (f) A zero-length window at a mid-slice cursor: legal, and exactly
        // the state a kernel sits in when it has consumed everything it
        // buffered and has not refilled yet.
        let mut r = positioned(&data);
        r.set_kernel_window(KernelWindow {
            w: 0,
            v: 0,
            ptr: 3,
            end: data.len(),
        });
        assert_window(&r, "zero-length window");
        assert_eq!(r.pos, 3);
        assert_eq!(r.valid, 0);
        assert_eq!(r.acc, 0);
        assert_eq!(r.bit_position(), 24);
        assert_eq!(r.bits_remaining(), data.len() * 8 - 24);
        // ...and it refills from that cursor: the next 8 bits are byte 3.
        assert_eq!(r.read_bits(8).unwrap(), u32::from(data[3]));
    }
}
