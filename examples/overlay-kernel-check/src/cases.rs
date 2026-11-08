//! The synthesized boundary cases of the ARM Rice kernel, plus the call shapes
//! both of their consumers use — **one transcription**, exactly like
//! [`super::corpus`]'s walk.
//!
//! Consumers (both declare this file with `#[path]`, so a case set cannot land
//! on one side only):
//!
//! * `crates/flac-lite-gba/tests/arm_from_outside.rs` — the host differential.
//!   On a host the `ArmRiceKernel` FFI leg runs [`flac_lite_gba::rice_arm::model_partition`],
//!   a Rust model of the asm; the host therefore witnesses the *contract*, never
//!   the instruction stream.
//! * `examples/overlay-kernel-check/src/main.rs` — the kernel-check ROM. On mGBA's
//!   ARM7TDMI it runs the *assembled* kernel and the model side by side over the
//!   same cases, plus the committed corpus. This is the only place the two halves
//!   meet, because there is no ARM executor on this host.
//!
//! The value anchor is a **packer**, not a second decoder: cases are built from
//! §9.2.7.2's *encoding* text (fold signed→unsigned, unary the quotient, then
//! `order` remainder bits), and each case carries the samples it was built from
//! and the bit position it is known to end at. The unfolding is the spec's two
//! cases, not `residual::rice_unmap` — the production arithmetic — so a shared
//! misreading cannot survive by calling the same function twice.
//!
//! Written against `alloc`, not `std`, so the ROM compiles it; the host test gets
//! the same code from a std binary.

extern crate alloc;

use alloc::vec;
use alloc::vec::Vec;
use flac_lite::Error;
use flac_lite::bits::BitReader;
use flac_lite::residual::{PartitionHeader, RiceKernel};
use flac_lite_gba::rice_arm::{KernelStatus, model_partition};

// ---------------------------------------------------------------------------
// The two call shapes. `decode_with` is the seam (`decode_rice_partition_with`
// through a kernel parameter, header built from `(order, n)`); `model_outcome`
// is the model leg called directly, which is what a device run needs in order to
// have a second, non-asm leg in the same image.
// ---------------------------------------------------------------------------

pub struct Outcome {
    pub samples: Vec<i32>,
    pub bits_after: usize,
    pub err: Option<Error>,
}

/// Decode a bare partition body from bit 0 of `bytes` through the core's seam.
pub fn decode_with<K: RiceKernel>(bytes: &[u8], order: u32, n: usize) -> Outcome {
    let mut reader = BitReader::new(bytes);
    let header = PartitionHeader {
        order: order as u8,
        is_escape: false,
        raw_bits: 0,
        sample_count: n as u16,
    };
    let mut out = alloc::vec![0i32; n];
    let err =
        flac_lite::residual::decode_rice_partition_with::<K>(&mut reader, &header, &mut out).err();
    Outcome {
        samples: out,
        bits_after: reader.bit_position(),
        err,
    }
}

/// Decode a body that starts at bit 0 of `bytes` through [`model_partition`]
/// directly. Only valid at `order <= MAX_ORDER`: above it the model's own
/// `debug_assert` refuses, and the wrapper's delegation (not the model) is what
/// answers there.
pub fn model_outcome(bytes: &[u8], order: u32, n: usize) -> Outcome {
    model_outcome_at(bytes, 0, order, n)
}

/// [`model_outcome`] at a bit offset — the corpus's partitions do not start at bit
/// 0, and the ROM needs the model leg there. The seek is the same three-statement
/// loop `corpus::skip` performs (read up to 32 bits at a time); it is repeated
/// here rather than shared because this file must compile as one unit in both
/// consumers, and the loop is not an expectation that could drift — the corpus
/// arm's cursor comparison would convict a wrong offset immediately.
///
/// Used only by the ROM: on the host, `decode_with::<ArmRiceKernel>` *is* this
/// leg, so calling it here would compare the model with itself.
#[allow(dead_code)]
pub fn model_outcome_at(bytes: &[u8], body_at: usize, order: u32, n: usize) -> Outcome {
    let mut reader = BitReader::new(bytes);
    let mut at = body_at;
    while at >= 32 {
        reader.read_bits(32).expect("seek a word");
        at -= 32;
    }
    if at > 0 {
        reader.read_bits(at as u32).expect("seek the tail");
    }
    let mut out = alloc::vec![0i32; n];
    let window = reader.kernel_window();
    let region = reader.region();
    let (after, status) = model_partition(&window, region, &mut out, order);
    // The same mapping the wrapper performs, so the two legs' cursors are
    // comparable bit positions rather than two representations.
    reader.set_kernel_window(after);
    let err = match status {
        KernelStatus::Done => None,
        KernelStatus::EndOfStream => Some(Error::EndOfStream),
        KernelStatus::GuardRejected => Some(Error::InvalidField),
    };
    Outcome {
        samples: out,
        bits_after: reader.bit_position(),
        err,
    }
}

// ---------------------------------------------------------------------------
// The packer: the value anchor, written from §9.2.7.2's encoding direction.
// ---------------------------------------------------------------------------

/// Naive MSB-first bit packer: one bit at a time, so it shares no arithmetic with
/// `BitReader`'s window/`peek_at` machinery, and it grows its own buffer so a
/// case cannot silently run off the end of a hand-sized one.
pub struct Packer {
    buf: Vec<u8>,
    pos: usize,
}

impl Packer {
    pub fn new(len_bytes: usize) -> Self {
        Self {
            buf: vec![0u8; len_bytes],
            pos: 0,
        }
    }
    pub fn put(&mut self, value: u32, width: u32) {
        assert!(width <= 32, "a test packer does not need wider fields");
        for i in (0..width).rev() {
            let byte = self.pos >> 3;
            if self.buf.len() <= byte {
                // Grow EXACTLY. `Vec::resize`/`reserve` double amortised, and that
                // doubling is what made this packer unusable on the target: the
                // 128 KB order-12 §9.2.7.3 case grew a 128 KB buffer into a
                // 262,144-byte allocation, and the GBA's EWRAM is 256 KB. The ROM
                // allocates these cases on the device, so the growth policy is part
                // of the shared code's contract, not a host detail. (Convicted by
                // the ROM's guard arm: `memory allocation of 262144 bytes failed`.)
                // Growth policy is shared code, so it is a target property:
                //   * ask only when the CAPACITY is short — `reserve_exact` is
                //     relative to `len`, and asking once per byte re-allocated
                //     and copied the whole buffer each byte (an O(n²) the ROM's
                //     guard arm measured at 33.8 G emulated cycles);
                //   * grow by 5/4, not by doubling — amortised doubling turns the
                //     order-12 §9.2.7.3 case into a 262,144-byte allocation, and
                //     the GBA's EWRAM is 256 KB;
                //   * floor at 64 bytes, not 4 KB — a 4 KB floor made each of the
                //     ~6,500 swept cases a 4 KB allocation, and the ROM ran out of
                //     EWRAM on the *clean* arm after the guard arm had passed.
                if self.buf.capacity() < byte + 1 {
                    let wanted = (byte + 1).max(self.buf.capacity() * 5 / 4).max(64);
                    self.buf.reserve_exact(wanted - self.buf.len());
                }
                self.buf.resize(byte + 1, 0);
            }
            let bit = ((value >> i) & 1) as u8;
            self.buf[byte] |= bit << (7 - (self.pos & 7));
            self.pos += 1;
        }
    }
    pub fn zeros(&mut self, mut n: usize) {
        while n > 0 {
            self.put(0, 1);
            n -= 1;
        }
    }
    pub fn finish(mut self) -> Vec<u8> {
        // Exactly the bits written: for the end-of-stream arms the region has to
        // stop where the codeword stops, or "unterminated" would mean "there were
        // four more zero bytes" and the guard would fire instead of the EOF.
        self.buf.truncate(self.pos.div_ceil(8));
        self.buf
    }
}

/// §9.2.7.2's unfolding, as the spec's two cases — deliberately NOT
/// `residual::rice_unmap`.
pub fn spec_unfold(bits: u32) -> i32 {
    if bits & 1 == 0 {
        (bits >> 1) as i32
    } else {
        (!(bits >> 1)) as i32
    }
}

/// §9.2.7.2's folding: `x ≥ 0 → 2x`, `x < 0 → −2x − 1`.
pub fn spec_fold(x: i32) -> u32 {
    if x >= 0 {
        (2i64 * i64::from(x)) as u32
    } else {
        (-2i64 * i64::from(x) - 1) as u32
    }
}

/// One synthesized partition: the bits, and what the bits are supposed to mean.
pub struct Case {
    pub order: u32,
    /// Samples the packer was asked to encode, in order. For an arm that is not
    /// meant to decode, the prefix that must still come out correct.
    pub expected: Vec<i32>,
    /// Samples to ask the decoder for.
    pub want: usize,
    /// Bit position a correct decode of the packed codewords stops at.
    pub end_bits: usize,
    pub bytes: Vec<u8>,
}

/// Build a partition that is meant to decode cleanly.
pub fn pack_case(order: u32, values: &[i32]) -> Case {
    // Sized exactly up front. Not an optimisation: the swept cases are ~6,500
    // small allocations, and on the target any growth slack multiplies by that
    // count against 256 KB of EWRAM. `end_bits_of` is the same expression the
    // assertion below checks the packer against, so a miscount here is convicted
    // by this function's own end-of-function assert, not silently absorbed.
    let mut p = Packer::new(end_bits_of(order, values).div_ceil(8));
    for v in values {
        let folded = spec_fold(*v);
        p.zeros((folded >> order) as usize);
        p.put(1, 1);
        if order > 0 {
            p.put(folded & u32::MAX >> (32 - order), order);
        }
    }
    // The anchor's own round trip, before any decoder is consulted: the spec's
    // two-case unfolding has to invert the two-case folding for every sample in
    // the case set, or the cases are measuring the anchor's arithmetic and not
    // the kernel's.
    for v in values {
        let f = spec_fold(*v);
        assert_eq!(
            spec_unfold(f),
            *v,
            "the anchor is not a bijection at {f:#x} — the case set is wrong, \
             not the kernel"
        );
    }
    let end_bits = end_bits_of(order, values);
    let bytes = p.finish();
    assert_eq!(
        bytes.len() * 8,
        end_bits.div_ceil(8) * 8,
        "the packer wrote {end_bits} bits into {} bytes",
        bytes.len()
    );
    Case {
        order,
        expected: values.to_vec(),
        want: values.len(),
        end_bits,
        bytes,
    }
}

/// The bit length of a list of clean codewords, computed a second time and by a
/// different expression than the packer's counter, so the cursor anchor does not
/// inherit the packer's arithmetic.
fn end_bits_of(order: u32, values: &[i32]) -> usize {
    values
        .iter()
        .map(|v| {
            let f = spec_fold(*v);
            usize::try_from(f >> order).unwrap() + 1 + usize::try_from(order).unwrap()
        })
        .sum()
}

/// Hand-write a partition body's bits (for the arms that are not a clean encode:
/// truncated tails, guard-sized runs).
pub fn raw_case(order: u32, want: usize, expected: &[i32], f: impl FnOnce(&mut Packer)) -> Case {
    let mut p = Packer::new(0);
    f(&mut p);
    let end_bits = p.pos;
    Case {
        order,
        expected: expected.to_vec(),
        want,
        end_bits,
        bytes: p.finish(),
    }
}

// ---------------------------------------------------------------------------
// The case set. Every family names the boundary it exists for.
// ---------------------------------------------------------------------------

/// Quotient values the run tiers hand between them: 0 is an immediate terminator,
/// 7/8 is the CTZ8-byte boundary, 15/16 and 31/32 are the De Bruijn/word-skip
/// boundaries, 63/64/127/128/255 cross whole-word and multi-word skips, and 1000
/// is a run longer than the 32-bit window at any order.
fn quotients() -> Vec<u32> {
    vec![
        0, 1, 2, 6, 7, 8, 9, 14, 15, 16, 17, 23, 24, 30, 31, 32, 33, 40, 63, 64, 127, 128, 255,
        1000,
    ]
}

/// Remainder bit patterns that catch an un-reversal going wrong. The window-domain
/// extraction hands the kernel `order` bits in the wrong order for a spec
/// remainder, so the patterns here are chosen to be *asymmetric*: `1 << (n-1)`
/// un-reverses to `1`, `0xAAAA`/`0x5555` and `0xA55A`/`0x5AA5` differ from their
/// reversals, and all-ones/all-zeros are the two that would silently survive a
/// missing reversal — which is why they are here too, as the floor rather than as
/// the test.
fn remainders(order: u32) -> Vec<u32> {
    if order == 0 {
        return vec![0];
    }
    let mask = u32::MAX >> (32 - order);
    let mut r = vec![0u32, mask, 1, mask - 1, mask / 2, 1 << (order - 1)];
    if order >= 4 {
        r.extend([0b1010_1010 & mask, 0b0101_0101 & mask]);
    }
    if order >= 16 {
        r.extend([0xA55A, 0x5AA5]);
    }
    r.sort_unstable();
    r.dedup();
    r.retain(|x| *x <= mask);
    r
}

/// The clean sweep: every order the kernel is asked about up to 31, every
/// quotient, every remainder pattern, plus one mixed partition per order that
/// packs several families back to back so the window/refill state crosses a
/// sample boundary while the tiers change underneath it.
///
/// Orders above [`MAX_ORDER`] are in here too — that is the delegation arm, and
/// it is deliberately inside the same loop rather than beside it, because the
/// claim is "identical behaviour at every order", not "identical behaviour up to
/// 16 and something else after".
// Used only by the host consumer: the ROM takes these one at a time
// (`clean_cases_for`), because five guard cases alive at once exceed EWRAM.
#[allow(dead_code)]
pub fn clean_cases() -> Vec<Case> {
    (0..=31u32).flat_map(clean_cases_for).collect()
}

/// One order's slice of [`clean_cases`]. Same cases, same order of construction —
/// this exists so a consumer with 256 KB of RAM (the ROM) can hold one order's
/// cases at a time instead of the whole sweep. The host keeps using
/// [`clean_cases`], which is defined *through* this function, so the two cannot
/// drift apart.
pub fn clean_cases_for(order: u32) -> Vec<Case> {
    let mut cases = Vec::new();
    for &q in &quotients() {
        for &rem in &remainders(order) {
            // Two cases have to be skipped because they are not clean at this
            // order, and both are witnessed in their own arms below: a
            // quotient above u32::MAX >> order (§9.2.7.3's first bound), and
            // the coded value u32::MAX itself (the second bound, whose
            // unfolded sample is i32::MIN).
            if q > u32::MAX >> order {
                continue;
            }
            let folded = (q << order) | rem;
            if folded == u32::MAX {
                continue;
            }
            cases.push(pack_case(order, &[spec_unfold(folded)]));
        }
    }
    let mut mixed = Vec::new();
    for &q in &[0u32, 7, 8, 15, 16, 31, 32, 64] {
        for &rem in remainders(order).iter().take(3) {
            if q > u32::MAX >> order {
                continue;
            }
            let folded = (q << order) | rem;
            if folded != u32::MAX {
                mixed.push(spec_unfold(folded));
            }
        }
    }
    if !mixed.is_empty() {
        cases.push(pack_case(order, &mixed));
    }
    cases
}

/// `p = 0` spelled out: the mask is empty, the folded value *is* the quotient,
/// and nothing of the remainder leg may run. This is also the order where an
/// un-reversal bug would be invisible, which is the same trap
/// `examples/arm-kernels` records for its own unmap probe ("the trap: p = 0
/// hides everything").
pub fn order_zero_cases() -> Vec<Case> {
    let mut cases = order_zero_affordable_cases();
    cases.push(order_zero_extreme_case());
    cases
}

/// The `p = 0` cases a 256 KB machine can hold. Same construction as
/// [`order_zero_cases`], without the extreme below, which is 2 MB of zero
/// bytes — bigger than the GBA's whole EWRAM, so the ROM cannot construct it
/// (the kernel-check ROM's own test takes this set and *reports* the one case it
/// does not run, rather than the gap being filtered away invisibly).
pub fn order_zero_affordable_cases() -> Vec<Case> {
    vec![
        pack_case(0, &[0, -1, 1, -2, 2, -16, 16, -501, 500]),
        pack_case(
            0,
            &[
                spec_unfold(31),
                spec_unfold(32),
                spec_unfold(255),
                spec_unfold(256),
            ],
        ),
    ]
}

/// The run-length extremity: at `p = 0` the folded value *is* the unary run, so
/// `spec_unfold(0x00FF_FFFF)` is a codeword 16,777,215 zero bits long — 2 MB.
/// Host-only by arithmetic, not by choice; the host's `p = 0` arm is where this
/// bound is witnessed.
pub fn order_zero_extreme_case() -> Case {
    pack_case(0, &[spec_unfold(0x00FF_FFFF)])
}
// ---------------------------------------------------------------------------
// The case set, continued. Every family names the boundary it exists for.
// ---------------------------------------------------------------------------
/// §9.2.7.3's first bound, behaviorally: a quotient above `u32::MAX >> order`, at
/// the orders where reaching it is affordable (see
/// [`both_bounds_are_reachable_only_at_orders_where_the_field_is_wide`] for where
/// that is, and for why orders 0..=11 have no behavioral arm here — they would
/// need a megabyte-to-gigabyte region each).
// Used only by the host consumer: the ROM takes these one at a time
// (`guard1_case`), because five guard cases alive at once exceed EWRAM.
#[allow(dead_code)]
pub fn guard1_cases() -> Vec<Case> {
    (12..=16u32).rev().map(guard1_case).collect()
}

/// One order's §9.2.7.3 first-bound case. The order-12 case is a 128 KB region,
/// so the ROM (and any other 256 KB consumer) must be able to take these one at a
/// time; [`guard1_cases`] is defined through this, so the host and the ROM run
/// the identical construction.
pub fn guard1_case(order: u32) -> Case {
    let q = usize::try_from((u32::MAX >> order) + 1).expect("fits in a test's usize");
    raw_case(order, 1, &[], |p| {
        p.zeros(q);
        p.put(1, 1);
        p.put(0, order);
    })
}

/// §9.2.7.3's second bound: the coded value `u32::MAX`, whose unfolded sample is
/// `i32::MIN`, which the spec forbids in a stream (`flac-format.md`: the encoder
/// will not emit it; a decoder seeing `i32::MIN` is not decoding a valid stream).
///
/// The order-30 case is the core's own cheap construction for this bound — at
/// order 30, unary 3 plus an all-ones remainder is `0xFFFFFFFF`. This kernel does
/// not compute order 30, so that case reaches it only through delegation, and so
/// it witnesses something the in-domain arms cannot: **delegating a wide order
/// does not lose a guard.**
// Used only by the host consumer: the ROM takes these one at a time
// (`guard2_case`), because five guard cases alive at once exceed EWRAM.
#[allow(dead_code)]
pub fn guard2_cases() -> Vec<Case> {
    let mut cases: Vec<Case> = (12..=16u32).rev().map(guard2_case).collect();
    // The order-30 case reaches the bound only through delegation (see this
    // family's doc comment), and it is a 4-byte region — kept in the plural
    // constructor because the ROM takes the in-domain cases one at a time and
    // then this one separately.
    cases.push(guard2_case(30));
    cases
}

/// One order's §9.2.7.3 second-bound case, for the same RAM reason as
/// [`guard1_case`].
pub fn guard2_case(order: u32) -> Case {
    if order == 30 {
        return raw_case(30, 1, &[], |p| {
            p.zeros(3);
            p.put(1, 1);
            p.put(0x3FFF_FFFF, 30);
        });
    }
    raw_case(order, 1, &[], |p| {
        p.zeros((u32::MAX >> order) as usize);
        p.put(1, 1);
        p.put(u32::MAX & (u32::MAX >> (32 - order)), order);
    })
}

/// End-of-stream arms: a unary run that never terminates, a codeword whose
/// remainder is cut short, a clean sample followed by a truncated run, an empty
/// region, and a word-skipping run that walks off the end of the bytes.
///
/// The trait promises only *per-read* atomicity on an error, and Q1 asked whether
/// this kernel keeps the incumbent's per-read restore. It does not: the probe
/// ratified "restore nothing" at the partition level, and 6a-4 kept it. Both
/// readings are permitted by the seam and the frame is dead either way; what this
/// arm does is pin which side each mechanism is on, require the samples written
/// before the failure to agree, and count how often the two cursors differ — so
/// the difference is a measurement in the record rather than a sentence.
pub fn eof_cases() -> Vec<Case> {
    vec![
        // Unary run with no terminator inside the region.
        raw_case(4, 2, &[], |p| p.zeros(200)),
        // Terminated unary, remainder cut mid-word: 11 of 16 bits.
        raw_case(16, 2, &[], |p| {
            p.zeros(3);
            p.put(1, 1);
            p.put(0b1010_1010_101, 11);
        }),
        // One clean sample, then a run that never terminates: the samples written
        // before the failure have to match the packer's.
        raw_case(6, 2, &[7], |p| {
            let folded = spec_fold(7);
            p.zeros((folded >> 6) as usize);
            p.put(1, 1);
            p.put(folded & 0b11_1111, 6);
            p.zeros(300);
        }),
        // Empty region, one sample asked for.
        raw_case(3, 1, &[], |_p| {}),
        // A window-exhausting run at the very end of the region: the tier-3
        // word-skip is walking bytes when the bytes stop.
        raw_case(8, 1, &[], |p| p.zeros(600)),
    ]
}

/// `order >= 32` is not a Rice parameter at all (5-bit field, escape excluded).
/// The incumbent answers `InvalidField`; the delegation has to answer the same.
pub fn out_of_range_cases() -> Vec<u32> {
    vec![32, 33, 40, 64, 128, 255]
}
