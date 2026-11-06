//! Residual coding: partitioned Rice / Rice2 / escape-record residual.
//!
//! SCAFFOLD STATE (2026-09-11): the residual decode path is complete —
//! [`rice_unmap`] (step 3a), [`decode_rice_partition`] (step 3b), and
//! [`decode_residual`] (step 3c: the §9.2.7 header, per-partition sample
//! counts, and the escape/Rice dispatch). What remains for step 3 is the
//! subframe/frame composition (`subframe::decode_subframe`, integrators,
//! `frame::decode_frame`), not the residual.
//!
//! Every subframe (FIXED or LPC) ends with a residual. This is the hottest code
//! path in the decoder — it decodes the bulk of the bits — so it is where the
//! perf spike's cycle counts will concentrate. Implementation notes:
//!
//! - **Rice decoding is unary + folded magnitude, branch-free where
//!   possible:** count zeros until the terminating one (the quotient —
//!   [`BitReader::rice_quotient`], the E2b table count landed 2026-10-21:
//!   one table lookup per 8-bit window instead of one loop iteration per
//!   zero), read the `order`-bit remainder, combine
//!   `(quotient << order) | remainder` into the folded value, then unfold it
//!   with [`rice_unmap`]. (The module previously sketched a
//!   `negative ? !sample : sample` sign-bit mapping here — that is not
//!   FLAC's residual mapping; see [`rice_unmap`]'s correction note.)
//! - **No division.** Partition counts are shift/mask (`blocksize >> order`,
//!   divisibility via `& (partitions - 1)`); the escaped-partition mapping is
//!   plain two's-complement reads.
//! - **There is no sample-size field in a coded residual** (corrected
//!   2026-09-11 while implementing step 3c). The scaffold (and FLAC.md's
//!   original 3c plan text) claimed a "sample-size header with a `0b111`
//!   unknown escape, derived by scanning partitions" — it does not exist.
//!   §9.2.7's complete layout is: method `u(2)`, partition order `u(4)`, then
//!   per partition a parameter `u(4)`/`u(5)` — and if that parameter is the
//!   escape code, an explicit raw-width `u(5)`. Residual sample width is
//!   implicit in the Rice codewords themselves; nothing is derived or scanned.
//!   Witness: RFC 9639 §9.2.7 read in full, Appendix D Table 38 (a real
//!   libFLAC residual walked bit-exactly: 2 + 4 + parameter, then straight
//!   into quotients), and libFLAC's `read_residual_partitioned_rice_` (reads
//!   nothing but method, partition order, and per-partition param/escape +
//!   raw width). FLAC.md → "Prior assumptions".
//!
//! STATUS (2026-11-06): **the Rice kernel seam is open, and a second
//! implementor exists behind a feature** — [`RiceKernel`] + [`TableCountKernel`]
//! (PR 2), which is the decode path, and
//! [`crate::rice_hybrid::HybridRiceKernel`], the portable hybrid twin, which
//! exists only in a build with `rice-hybrid` on and is not the default in any
//! build (split-ladder item 5, PRs 2–3; the module states what it is and the
//! `Cargo.toml` note states the measured +2,984 B that made it a feature).
//! Zero behavior change from either PR: the one instantiation the crate's decode
//! path uses is still the loop that was already here, chosen on the one line of
//! [`decode_rice_partition`]; and no crate outside this one implements the trait
//! yet — that is item 6 (`flac-lite-gba`, which links none of this into the ROM
//! today). [`decode_rice_partition_with`] became `pub` in PR 3 for one reason:
//! PR 3's witness is a caller *outside* this crate running both mechanisms over
//! the same real partitions (`tests/rice_two_mechanism_differential.rs`, which
//! is `required-features = ["rice-hybrid"]`); nothing above this module threads
//! a kernel choice, which is item 6's to make. Why the hook is whole-partition
//! rather than per-sample is argued on the trait; the window half of the seam
//! (the `rev32` bridge) landed inert in PR 1 and is argued in `bits`' module
//! docs — its only caller is the twin, so the bridge is unreachable in a default
//! build and in any build that does not name a window-domain kernel at all. The
//! record lives in `FLAC.md` under "split-ladder item 5", PR 3.

use crate::bits::BitReader;

/// One partition's header, as read from the stream (§9.2.7/§9.2.7.1).
///
/// [`decode_residual`] reads the parameter field (its width follows the 2-bit
/// coding-method header) and splits this header out of it: the all-ones
/// escape code (`0b1111` / `0b11111`) sets `is_escape` and the 5 raw-width
/// bits fill `raw_bits`; otherwise the unsigned parameter fills `order`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PartitionHeader {
    /// Rice parameter = bits of least-significant (binary remainder) part.
    /// Legal range follows the coding method: 0..=14 under the 4-bit method,
    /// 0..=30 under the 5-bit — the all-ones value of each is the escape code,
    /// so it never lands here.
    pub order: u8,
    /// Escape record (§9.2.7.1): samples in this partition are unencoded,
    /// stored at `raw_bits` each.
    pub is_escape: bool,
    /// Bits per residual sample for an escaped partition. **5-bit field**
    /// ("Directly following the escape code are 5 bits containing the number
    /// of bits with which each residual sample is stored", §9.2.7.1 — the
    /// scaffold's "4-bit field" note was wrong; corrected 2026-09-11 while
    /// implementing 3b). `0` is legal and means all-zero samples stored with
    /// no bits at all.
    pub raw_bits: u8,
    /// Samples in this partition (per §9.2.7's first-partition-minus-order
    /// rule — the caller computes it; this function only checks `out`).
    pub sample_count: u16,
}

/// Decode a complete coded residual (§9.2.7) into `out`: the residual header,
/// then every partition body via [`decode_rice_partition`].
///
/// `out` receives the samples as *residuals* (the caller in
/// [`crate::subframe`] integrates them through the predictor). `blocksize` is
/// the frame's block size and `predictor_order` the subframe's predictor order
/// (0 for CONSTANT/VERBATIM — those carry no residual at all and never reach
/// this function).
///
/// **Stream layout (§9.2.7, complete — see the module note on the phantom
/// sample-size field):** coding method `u(2)` (`0b00` Rice 4-bit parameters,
/// `0b01` Rice 5-bit; `0b10..0b11` reserved → [`Error::InvalidField`]),
/// partition order `u(4)`, then 2^order partitions, each opening with its
/// parameter — `u(4)`/`u(5)` following the method; the all-ones value
/// (`0b1111` / `0b11111`) is the escape (§9.2.7.1), followed by the raw
/// width `u(5)`.
///
/// **Sample counts (§9.2.7):** partition 0 carries
/// `(blocksize >> partition_order) - predictor_order` residual samples (its
/// first `order` slots belong to the warm-up samples), every other partition
/// carries `blocksize >> partition_order`. Total: `blocksize - order`, which
/// is the required `out` length (a mismatch is a caller contract violation,
/// like [`decode_rice_partition`]'s).
///
/// **Stream MUSTs enforced as [`Error::InvalidField`]** (all three are
/// stream-validity rules, checkable frame-locally, not profile gating):
/// - block size evenly divisible by the partition count — checked without
///   division: `blocksize & (partitions - 1) == 0` (so odd blocksizes admit
///   only partition order 0);
/// - `blocksize >> partition_order > predictor_order` (e.g. blocksize 4096,
///   order 4 ⇒ partition order ≤ 9);
/// - the reserved coding-method codes `0b10`/`0b11`.
///
/// Cursor: lands exactly at the end of the last partition body on success;
/// per-read atomicity on failure, same contract as
/// [`decode_rice_partition`] (a failed frame is discarded wholesale, so a
/// partially filled `out` is undefined).
pub fn decode_residual(
    reader: &mut BitReader<'_>,
    blocksize: usize,
    predictor_order: usize,
    out: &mut [i32],
) -> crate::Result<()> {
    // Caller contract: the residual carries one sample per blockslot the
    // warm-up did not take. Checked up front, before any bit is consumed
    // (same rule as decode_rice_partition's out-length check). checked_sub:
    // order > blocksize is a caller bug to reject, not a subtraction to
    // panic on.
    let Some(total) = blocksize.checked_sub(predictor_order) else {
        return Err(crate::Error::InvalidField);
    };
    if out.len() != total {
        return Err(crate::Error::InvalidField);
    }
    // §9.2.7 Table 23: the coding method selects the parameter width and the
    // escape value; both methods decode identically otherwise.
    let (plen, escape) = match reader.read_bits(2)? {
        0b00 => (4u32, 15u32),
        0b01 => (5, 31),
        _ => return Err(crate::Error::InvalidField),
    };
    let partition_order = reader.read_bits(4)? as usize;
    let partitions = 1usize << partition_order;
    // "The partition order MUST be such that the block size is evenly
    // divisible by the number of partitions" — power-of-two divisor, so a
    // mask, not a divide (ARMv4T rule).
    if blocksize & (partitions - 1) != 0 {
        return Err(crate::Error::InvalidField);
    }
    let partition_samples = blocksize >> partition_order;
    // "(block size >> partition order) is larger than the predictor order".
    // Implies blocksize > predictor_order, and that every partition count
    // below is >= 1.
    if predictor_order >= partition_samples {
        return Err(crate::Error::InvalidField);
    }
    // Walk partitions without index arithmetic into `out`: `rest` shrinks by
    // exactly each partition's count, and the checks above make every
    // `split_at_mut` in range.
    let mut rest = out;
    for partition in 0..partitions {
        let count = if partition == 0 {
            partition_samples - predictor_order
        } else {
            partition_samples
        };
        let param = reader.read_bits(plen)?;
        let header = if param == escape {
            // §9.2.7.1: the raw width rides directly behind the escape code.
            let raw_bits = reader.read_bits(5)? as u8;
            PartitionHeader {
                order: 0,
                is_escape: true,
                raw_bits,
                sample_count: count as u16,
            }
        } else {
            PartitionHeader {
                order: param as u8,
                is_escape: false,
                raw_bits: 0,
                sample_count: count as u16,
            }
        };
        let (head, tail) = rest.split_at_mut(count);
        decode_rice_partition(reader, &header, head)?;
        rest = tail;
    }
    Ok(())
}

/// One **whole** non-escape Rice partition, as a callable — the seam this
/// module exists to have (split-ladder item 5, PR 2, 2026-11-05: FLAC.md →
/// "Split-ladder item 5 — the `RiceKernel` seam: three PRs (plan of record)").
///
/// The shape is written here *in a type* rather than argued, which is what
/// makes a wrong shape cost one small PR instead of a design argument
/// mid-diff. Both facts that settled it are already in this tree, they are
/// cited in `bits`' module docs (the module that owns the window bridge), and
/// they agree with each other:
///
/// - **Whole-partition, not per-sample.** The proposal's `sample_step`
///   signature (word window + budget + order in; value/consumed/state out)
///   cannot host any kernel that has ever produced a number: E10 composes
///   "the hybrid as a single whole-partition kernel rather than calling the
///   track's word-in/word-out kernels — the seam needs the byte-window ABI",
///   and `examples/arm-kernels/src/sample_step.rs` declines the reversal and
///   straddle-merge stitching from its own side. A per-sample hook has nowhere
///   to put a loop, nowhere to hold a window across samples.
/// - **The kernel owns that loop.** One call per partition body, not one per
///   sample: a kernel refills, consumes, keeps its own window, and decides
///   when the partition is finished. That is what lets the probe's shape —
///   `arm_rice_partition` and its host twin `twin_partition`, both whole-
///   partition over a `w`/`v`/`ptr`/`end` state — be implemented by an overlay
///   instead of re-derived from scratch.
///
/// **Not sealed**, deliberately: `flac-lite-gba` (item 6) implementing this is
/// the point of the seam. PR 2 ships exactly one implementor —
/// [`TableCountKernel`], the loop that already lived here — and **no external**
/// one, which is item 6 landing rather than a gap in this one. **Monomorphized,
/// never `dyn`**: the method below is an associated function, which makes the
/// trait non-object-safe, so a vtable is not expressible rather than merely
/// discouraged. That is on purpose — the E2b lesson is that a seam's cost shows
/// up as codegen coupling, so the seam may not be a dynamic call. PR 3 measured
/// that coupling directly, and it is larger than it looks: adding a second
/// implementor to this trait costs **+2,984 B of *other* code** on the
/// calibration instrument, all of it re-inlined decode-path code, with **not one
/// hybrid symbol in the image** (`Cargo.toml` carries the attributed reading). So
/// the twin is feature-gated (`rice-hybrid`), and the default-flip decision stays
/// attributable to the swapped mechanism alone.
///
/// Reachable to other crates as `flac_lite::residual::RiceKernel`. Within this
/// crate there are two implementors, one of them conditional: [`TableCountKernel`]
/// (the default the decode path uses, always compiled) and
/// [`crate::rice_hybrid::HybridRiceKernel`] (PR 3's portable twin, compiled only
/// with `rice-hybrid` on, reachable through [`decode_rice_partition_with`] but
/// not wired into any decode-path call site in any configuration). Outside this
/// crate there is still none — that is item 6 landing, not a gap in this one.
pub trait RiceKernel {
    /// Produce `out.len()` Rice-coded residuals (§9.2.7.2) into `out`, from a
    /// cursor sitting on the first bit of the partition body, at Rice parameter
    /// `order`.
    ///
    /// What every kernel owes the core. Every item here is *existing* core
    /// behavior, stated on the trait because a kernel now owns the part of the
    /// loop that implements it — none of it is new contract this PR invents:
    ///
    /// - **`order` is the parameter as read from the header**: `0..=14` under
    ///   the 4-bit coding method, `0..=30` under the 5-bit. The escape code
    ///   never reaches a kernel — [`decode_rice_partition_with`] owns the
    ///   §9.2.7.1 escape path. And the encode profile's limits are the core's
    ///   to enforce, never a kernel's to assume: the overlay kernels' loud
    ///   rejection of escape partitions and `p > 14` is a probe fact, not a
    ///   contract, and the core stays correct for arbitrary FLAC.
    /// - **§9.2.7.3's two stream MUSTs are the kernel's to enforce**, because
    ///   a kernel is what computes the folded value: `quotient > (u32::MAX >>
    ///   order)` (not representable in 32 bits) and `folded == u32::MAX` (the
    ///   unique pre-image of `i32::MIN`, which [`rice_unmap`] deliberately
    ///   does *not* reject) are both [`Error::InvalidField`](crate::Error).
    /// - **Running out of bits is [`Error::EndOfStream`](crate::Error).** This
    ///   is where the probe's `KernelState::status` field went: PR 1's
    ///   [`KernelWindow`](crate::bits::KernelWindow) deliberately does not
    ///   carry it, because this seam is the thing that has to act on it and the
    ///   reader's own end-of-data condition is already that variant.
    /// - **Cursor rules are exactly as they were before the seam existed:** on
    ///   success the cursor sits precisely past the partition body; on error
    ///   only per-read atomicity holds (an unterminated unary quotient restores
    ///   to that codeword's start — [`BitReader::rice_quotient`]'s atomicity),
    ///   because an errored frame is discarded wholesale and partially written
    ///   `out` samples are undefined.
    ///
    /// A kernel that keeps its own bit window uses
    /// [`BitReader::kernel_window`] / [`BitReader::set_kernel_window`]: that
    /// pair *is* the probe's `AccumulatorReader::rice_partition_kernel` seam
    /// method (patch line 1205 of the committed `e10-three-arm-probe.patch`),
    /// lifted into the crate by PR 1. One trap then belongs to every
    /// window-domain implementor, and to none of PR 2's default: do **not**
    /// express the empty window with [`BitReader::consume`] — its `acc <<= n`
    /// is a shift-by-32 at `n = 32`, a fact PR 1's landing record hands forward
    /// rather than changing. Spell the leg (`if n >= 32 { 0 }`, ARM's
    /// `LSR`-by-≥32-yields-0 rule), which is also the `q = 31` leg PR 3's twin
    /// docs and the kernel crate's committed table pin.
    fn decode_partition(
        reader: &mut BitReader<'_>,
        order: u32,
        out: &mut [i32],
    ) -> crate::Result<()>;
}

/// The core's Rice kernel as it already stands — the E2b table count, driven
/// through the reader's own primitives — as the default instantiation of
/// [`RiceKernel`].
///
/// **Nothing in this type changes what it does.** Its body is the loop that has
/// been `decode_rice_partition`'s non-escape sample loop since step 3b
/// (2026-09-11), counted by [`BitReader::rice_quotient`] since E2b
/// (2026-10-21): it arrives here with no edit beyond the indentation that
/// puts a free function's body inside a method. That is deliberate — PR 2's
/// whole claim is that the default impl **is** today's code, so the only thing
/// the PR can possibly cost samples is the plumbing, which is why its gate is a
/// codegen reading plus a cycle reading and not a new test suite.
///
/// It stays live whichever way PR 3's measurement lands: that differential
/// needs **two** mechanisms side by side (FLAC.md's item-5 invariant: do not
/// delete `bits::rice_quotient` or `LEADING_ONES` — two live mechanisms are
/// what the comparison consumes, and `LEADING_ONES` at `0x0300022c` is an `nm`
/// anchor cited by E2b, by E10 and by the `iwram-pin` witness). If the twin
/// comes in slower than the bar, this stays the core's default and says so.
///
/// No `#[inline]`, no `#[inline(never)]`, no generic parameter and no `repr`
/// was added to this type or around it in PR 2: what the inliner does with the
/// seam *is* what PR 2 measures, and pre-empting that decision here would
/// spend the one measurement the item exists to make.
///
/// **PR 3 landed without flipping the default, so this is still the core's
/// mechanism** — and the reason is a fact about the host, not a measured
/// negative about the twin: the reading PR 3's rule is gated on is a GBA cycle
/// reading, and there is no cycle-measurement environment here (no
/// `mgba-test-runner` and no equivalent). The rule stays pre-declared and
/// un-ran; `FLAC.md`'s item-5 record names the run that owes it.
///
/// One measured fact about that fork, so it is not rediscovered at the flip: a
/// flip *does* retire the incumbent from the image. At the one-line flip probe the
/// twin's walk replaced this loop and `bits::BitReader::rice_quotient` (336 B)
/// and `LEADING_ONES` (256 B, at `0x0300022c`) both left the ELF — nothing
/// reachable called them — which is why the IWRAM delta at the flip reads as
/// +256 B net rather than the +544 B the E10 budget line suggests. The item's
/// "do not delete them" invariant is therefore about the *build*, not about what
/// the linker happens to keep, and it is the `rice-hybrid` gate that keeps both
/// mechanisms observable in one checkout.
pub struct TableCountKernel;

impl RiceKernel for TableCountKernel {
    fn decode_partition(
        reader: &mut BitReader<'_>,
        order: u32,
        out: &mut [i32],
    ) -> crate::Result<()> {
        for sample in out.iter_mut() {
            // Unary quotient: count zeros until the terminating one — the E2b
            // table count (BitReader::rice_quotient), landed 2026-10-21: this
            // per-sample loop was E8's measured cost center for FIXED (unary
            // 56.2% of gross); E2b priced the table count at −21.7% on the
            // whole FIXED arm. Bit-exact replacement, atomic on EOF.
            let quotient = reader.rice_quotient()?;
            // Binary remainder — only when the parameter says so. `order == 0`
            // is unary-only words; there is nothing to read.
            let remainder = if order == 0 {
                0
            } else {
                reader.read_bits(order)?
            };
            // §9.2.7.3: a residual must be representable in 32 bits, so a folded
            // value that does not fit `u32` cannot occur in a conforming stream.
            // Guard on the *quotient* against `MAX >> order`: `checked_shl` is NOT
            // that check — it only rejects a shift *amount* >= 32 and wraps bits
            // shifted out of the value (found by test, 2026-09-11: quotient 4 at
            // order 30 silently yielded folded = 0). The `order >= 32` arm
            // short-circuits before the shift, so `MAX >> order` is never a
            // shift-by-32. Under the profile order <= 30 (0b11111 is the escape),
            // so this is belt-and-braces for a bad caller header.
            if order >= 32 || quotient > (u32::MAX >> order) {
                return Err(crate::Error::InvalidField);
            }
            let folded = (quotient << order) | remainder;
            if folded == u32::MAX {
                // The i32::MIN pre-image: forbidden in a stream (§9.2.7.3).
                return Err(crate::Error::InvalidField);
            }
            *sample = rice_unmap(folded);
        }
        Ok(())
    }
}

/// Decode one partition's samples into `out` (RFC 9639 §9.2.7.2 Rice, or
/// §9.2.7.1 escape-record raw samples) with the kernel `K` producing the
/// non-escape body.
///
/// Split out from [`decode_residual`] so it can be unit-tested on its own —
/// this is the inner loop. The parameter field (and the escape code that
/// selects between the two partition forms) is read by the caller; this
/// function consumes the partition *body* only, from a cursor sitting on the
/// first body bit.
///
/// **What the driver owns, and what [`RiceKernel`] owns.** The two contract
/// halves are separated along the seam: this function keeps the caller
/// contract, the §9.2.7.1 escape record and the cursor rules; the kernel owns
/// the §9.2.7.2 codeword walk and the §9.2.7.3 value-limit rejections, since
/// it is the code computing the folded value. The split is exactly the code
/// that was already here — the escape block below has not been edited since
/// 3b, and the Rice half is the same loop, carried across with no edit at all.
///
/// Contract: `out.len()` must equal `header.sample_count`. The caller
/// (step 3c's [`decode_residual`]) slices `out` per partition; a mismatch is
/// a caller bug, not a stream condition, and yields `InvalidField` without
/// moving the cursor.
///
/// **Escape partition (§9.2.7.1):** `raw_bits` signed two's-complement bits
/// per sample (via [`BitReader::read_signed`], which owns sign extension).
/// `raw_bits == 0` is legal and means every sample is 0 with **no bits
/// consumed at all** — the partition is just its escape code and `0b00000`
/// (which the caller read). Escaped partitions never reach a kernel: the
/// profile-bound things the overlay kernels reject are the core's to handle
/// here, and `K` is chosen per *partition*, not per stream.
///
/// Cursor: on success it sits exactly past the partition body. On error it
/// stays where the failing read left it (per-read atomicity only, the same
/// rule as the header parses — an errored frame is discarded wholesale, so
/// partially written `out` samples are undefined). One refinement since the
/// E2b landing: an unterminated unary quotient restores the cursor to that
/// codeword's start ([`BitReader::rice_quotient`]'s atomicity) rather than
/// leaving it after the consumed zeros — observable only on a dead frame,
/// which the caller discards wholesale anyway.
///
/// **`pub` since PR 3 (split-ladder item 5), for one reason and with one
/// limit.** PR 3's gate is a *two-mechanism differential on real partitions*,
/// and the witness that reads the committed corpus lives outside this crate —
/// `tests/rice_two_mechanism_differential.rs` instantiates this with both
/// [`TableCountKernel`] and [`crate::rice_hybrid::HybridRiceKernel`] over every
/// non-escape partition of every real subframe body in
/// `tests/subframe_body_vectors.txt`, and against an independent spec-order
/// oracle. The limit: the *caller-facing* layers still do not take a kernel —
/// [`decode_residual`], [`crate::subframe`] and [`crate::frame`] keep calling
/// [`decode_rice_partition`], and threading a choice up through them (and
/// through `scan`/`driver`) is item 6's decision, not a side effect of this
/// signature being visible.
pub fn decode_rice_partition_with<K: RiceKernel>(
    reader: &mut BitReader<'_>,
    header: &PartitionHeader,
    out: &mut [i32],
) -> crate::Result<()> {
    if out.len() != usize::from(header.sample_count) {
        // Caller contract violation (slice length must track the header).
        return Err(crate::Error::InvalidField);
    }
    if header.is_escape {
        // §9.2.7.1: fixed-width raw residuals; 0 bits means all-zero samples
        // and consumes nothing ("the partition contains nothing except the
        // escape code and 0b00000").
        for sample in out.iter_mut() {
            *sample = if header.raw_bits == 0 {
                0
            } else {
                reader.read_signed(u32::from(header.raw_bits))?
            };
        }
        return Ok(());
    }
    K::decode_partition(reader, u32::from(header.order), out)
}

/// [`decode_rice_partition_with`] at the core's default kernel,
/// [`TableCountKernel`] — which is why no existing caller changed, and why the
/// module's unit tests (which built this partition body directly, from before
/// the seam existed) keep doing so against the incumbent rather than against a
/// test-only stand-in.
///
/// This is the one place the default is chosen, and one line wide by design:
/// PR 3's "default flip" is exactly this call's type argument, so a bad number
/// is attributable either to the generic seam or to the swapped mechanism, never
/// to both (FLAC.md's item-5 invariant: do not fold the twin flip into PR 2).
///
/// **It still names [`TableCountKernel`] after PR 3.** The twin
/// ([`crate::rice_hybrid::HybridRiceKernel`]) landed behind the hook, witnessed
/// against this mechanism sample-by-sample on the committed corpus, and the
/// pre-declared bar it has to clear is an *on-target cycle* bar that this host
/// cannot read. Nothing here flipped, and the non-flip is recorded rather than
/// left implied — see `FLAC.md`, "split-ladder item 5", PR 3.
fn decode_rice_partition(
    reader: &mut BitReader<'_>,
    header: &PartitionHeader,
    out: &mut [i32],
) -> crate::Result<()> {
    decode_rice_partition_with::<TableCountKernel>(reader, header, out)
}

/// Unfold a Rice-coded residual: folded magnitude → signed sample
/// (RFC 9639 §9.2.7.2).
///
/// The encoder folds signed → unsigned as `x ≥ 0 → 2x`, `x < 0 → −2x − 1`
/// ("for positive numbers, the representation is the number doubled. For
/// negative numbers, the representation is the number multiplied by -2 and
/// with 1 subtracted"). So the decoder's inverse is: **even `n → n >> 1`,
/// odd `n → !(n >> 1)`**.
///
/// ```text
///  0 →  0    1 → −1    2 →  1    3 → −2    4 →  2    38 → 19    39 → −20
/// ```
///
/// **Corrected 2026-09-10:** this scaffold's doc claimed the mirrored
/// convention ("odd → (n+1)/2, even → −n/2", `negative = n & 1`), which
/// sign-flips every decoded residual. §9.2.7.2's own worked example is the
/// witness: folded 38 unfolds to **+19**; the scaffold's mapping returns
/// −19. FLAC.md → step 3a entry.
///
/// (Rendered: `0 → 0`, `1 → −1`, `2 → 1`, `3 → −2`, `38 → 19`, `39 → −20`.)
///
/// Pure map, no validation: §9.2.7.3 forbids residual values equal to
/// `i32::MIN` (folded input `0xFFFF_FFFF`), and enforcing that is the
/// partition decoder's job — it sees the stream and can fail the frame
/// (step 3b). Here the mapping stays total and returns `i32::MIN` for that
/// input, pinned by a test so the division of responsibility is explicit.
///
/// `pub(crate)` since PR 3, so [`crate::rice_hybrid`]'s kernel can fold through
/// this rather than restating the map — a second copy of a mapping that was
/// corrected once (see the note above) is a second place the correction can be
/// wrong. It stays out of the public API.
pub(crate) fn rice_unmap(n: u32) -> i32 {
    if n & 1 == 0 {
        (n >> 1) as i32
    } else {
        (!(n >> 1)) as i32
    }
}

// Unit tests compile as part of the lib under the host test harness (Gate 2,
// run from *outside* the repo — see README "Cargo config leak"). `core`-only,
// like the rest of the crate's unit tests.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hand_vectors_per_rfc_9_2_7_2() {
        // 0 and the sign alternation are the mapping's whole personality.
        assert_eq!(rice_unmap(0), 0);
        assert_eq!(rice_unmap(1), -1);
        assert_eq!(rice_unmap(2), 1);
        assert_eq!(rice_unmap(3), -2);
        assert_eq!(rice_unmap(4), 2);
        assert_eq!(rice_unmap(5), -3);
        // The RFC §9.2.7.2 worked example: Rice parameter 3, folded value 38
        // (unary 4 << 3 | binary 6). The spec unfolds it to +19; the retired
        // scaffold convention would give -19. This assertion is the witness.
        assert_eq!(rice_unmap(38), 19);
        assert_eq!(rice_unmap(39), -20);
    }

    #[test]
    fn roundtrips_the_encoders_fold() {
        // The encoder's fold, written straight from the spec text in i64 so
        // the extremes cannot overflow the intermediate.
        let fold = |x: i32| -> u32 {
            let v = if x >= 0 {
                2 * i64::from(x)
            } else {
                -2 * i64::from(x) - 1
            };
            u32::try_from(v).expect("fold must fit u32 for any i32")
        };
        // Dense around zero (where off-by-one signs live), sparse-wide, plus
        // the exact edges. i32::MIN folds to the §9.2.7.3-forbidden value;
        // it is exercised separately in `extremes_and_the_forbidden_value`.
        for x in -2000i32..=2000 {
            assert_eq!(rice_unmap(fold(x)), x, "roundtrip {x}");
        }
        for x in (i32::MIN + 1..=i32::MAX).step_by(1_000_003) {
            assert_eq!(rice_unmap(fold(x)), x, "roundtrip {x}");
        }
        assert_eq!(rice_unmap(fold(i32::MAX)), i32::MAX);
        assert_eq!(rice_unmap(fold(i32::MIN + 1)), i32::MIN + 1);
    }

    #[test]
    fn matches_subtraction_oracle_at_both_ends() {
        // Independent oracle via a different mechanism: odd n = 2k+1 folds
        // -(k+1), computed by negate-and-subtract rather than bitwise NOT.
        // The intermediate is i64: at n = 0xFFFF_FFFF the negation of
        // 2^31 must not overflow the i32 intermediate (that input is the
        // §9.2.7.3-forbidden one, and the oracle still has to name it).
        let oracle = |n: u32| -> i32 {
            if n & 1 == 0 {
                (n >> 1) as i32
            } else {
                (-(i64::from(n >> 1) + 1)) as i32
            }
        };
        for n in 0..65_536u32 {
            assert_eq!(rice_unmap(n), oracle(n), "n {n}");
        }
        // The high window: near u32::MAX the folded magnitudes are the huge
        // residuals; sweep the last 64Ki values (65535 candidates every
        // other parity, cheap).
        for k in 0..32_768u32 {
            let n = u32::MAX - 2 * k;
            assert_eq!(rice_unmap(n), oracle(n), "n {n:#x}");
            let n = u32::MAX - (2 * k + 1);
            assert_eq!(rice_unmap(n), oracle(n), "n {n:#x}");
        }
    }

    #[test]
    fn extremes_and_the_forbidden_value() {
        // The largest legal positive residual (fold of i32::MAX).
        assert_eq!(rice_unmap(0xFFFF_FFFE), i32::MAX);
        // n = 0xFFFF_FFFF maps to i32::MIN — the value §9.2.7.3 forbids in a
        // stream. This pure map returns it (mapping is total); rejecting it
        // is decode_rice_partition's contract with the stream (step 3b).
        assert_eq!(rice_unmap(0xFFFF_FFFF), i32::MIN);
    }

    // ---- decode_rice_partition (step 3b) ----------------------------------
    //
    // Witness rule per Sam's 2026-09-11 direction: reuse what exists and
    // cover edge cases with unit tests; the frame_vectors.py per-partition
    // oracle extension is deferred (it only pays off once 3c/3e make the
    // whole residual path live). Witnesses here: the RFC's own worked
    // example (§9.2.7.2), hand-derived codewords, and a differential sweep
    // against an independent bit packer that encodes straight from the
    // spec text (fold → unary → remainder) — a different mechanism than the
    // decoder's zeros-count loop.

    use crate::Error;

    /// Independent MSB-first bit packer. Deliberately naive (one bit at a
    /// time) so a shared bug with `BitReader`/`peek_at` is unlikely.
    struct Packer<'a> {
        buf: &'a mut [u8],
        pos: usize,
    }

    impl<'a> Packer<'a> {
        fn new(buf: &'a mut [u8]) -> Self {
            buf.iter_mut().for_each(|b| *b = 0);
            Self { buf, pos: 0 }
        }
        fn put(&mut self, value: u32, width: u32) {
            for i in (0..width).rev() {
                let bit = ((value >> i) & 1) as u8;
                self.buf[self.pos >> 3] |= bit << (7 - (self.pos & 7));
                self.pos += 1;
            }
        }
    }

    /// Write one Rice codeword straight from §9.2.7.2's encoding text:
    /// fold signed → unsigned (`x ≥ 0 → 2x`, `x < 0 → −2x − 1`), unary the
    /// quotient (`folded >> order`), then `order` remainder bits. `order`
    /// must be ≥ 1 here; order-0 words are packed explicitly in the test
    /// that needs them.
    fn rice_put(p: &mut Packer<'_>, order: u32, residual: i32) {
        let folded: u32 = if residual >= 0 {
            (2i64 * i64::from(residual)) as u32
        } else {
            (-2i64 * i64::from(residual) - 1) as u32
        };
        let quotient = folded >> order;
        let remainder = folded & (((1u32 << order) - 1) as u32);
        for _ in 0..quotient {
            p.put(0, 1);
        }
        p.put(1, 1);
        p.put(remainder, order);
    }

    fn rice_header(order: u8, sample_count: u16) -> PartitionHeader {
        PartitionHeader {
            order,
            is_escape: false,
            raw_bits: 0,
            sample_count,
        }
    }

    fn escape_header(raw_bits: u8, sample_count: u16) -> PartitionHeader {
        PartitionHeader {
            order: 0,
            is_escape: true,
            raw_bits,
            sample_count,
        }
    }

    #[test]
    fn rfc_9_2_7_2_worked_example() {
        // The RFC's own example: Rice parameter 3, folded residual 38 →
        // unary 4 = 0b00001, binary 6 = 0b110, codeword 0b00001110.
        // Unfolds to +19.
        let mut r = BitReader::new(&[0b0000_1110]);
        let mut out = [0i32; 1];
        decode_rice_partition(&mut r, &rice_header(3, 1), &mut out).unwrap();
        assert_eq!(out, [19]);
        assert_eq!(r.bit_position(), 8); // exact-fit, no over/under-read
    }

    #[test]
    fn hand_packed_multi_sample_partition() {
        // Residuals [0, -1, 2, -2] at order 1. Folds via §9.2.7.2 (`x ≥ 0 →
        // 2x`, `x < 0 → −2x − 1`): [0, 1, 4, 3] — note fold(−2) = **3**, not
        // 5 (5 is fold(−3); an earlier draft of this vector mis-derived it and
        // the decoder faithfully returned −3, i.e. the test was wrong and the
        // impl right — FLAC.md's recurring lesson).
        // Words (unary quotient, then `order` remainder bits):
        //   fold 0 → q0 "1"   r0 "0"  = 10
        //   fold 1 → q0 "1"   r1 "1"  = 11
        //   fold 4 → q2 "001" r0 "0"  = 0010
        //   fold 3 → q1 "01"  r1 "1"  = 011
        // = 1011_0010_011 → 0xB2, 0x60. 11 bits consumed; the 5 slack bits
        // must NOT be eaten.
        let mut r = BitReader::new(&[0xB2, 0x60]);
        let mut out = [0i32; 4];
        decode_rice_partition(&mut r, &rice_header(1, 4), &mut out).unwrap();
        assert_eq!(out, [0, -1, 2, -2]);
        // 2 + 2 + 4 + 3 = 11 codeword bits.
        assert_eq!(r.bit_position(), 11);
    }

    #[test]
    fn order_zero_partitions_are_unary_only() {
        // order 0 ⇒ remainder width 0 ⇒ nothing is read (§9.2.7.2: the
        // parameter "is the number of bits in the least-significant part").
        // `read_bits(0)` returns InvalidField by design, so the width-0 case
        // must skip the remainder read entirely — a counterfactual this test
        // enforces: calling read_bits(0) would error, and over-consuming a
        // bit would break the cursor.
        // Folds [0, 1, 2] (residuals [0, -1, 1]) → "1", "01", "001"
        // = 1_01_001 → 101_001 00 → 0xA4.
        let mut r = BitReader::new(&[0xA4]);
        let mut out = [0i32; 3];
        decode_rice_partition(&mut r, &rice_header(0, 3), &mut out).unwrap();
        assert_eq!(out, [0, -1, 1]);
        assert_eq!(r.bit_position(), 6); // exactly the 6 unary bits
    }

    #[test]
    fn escape_partition_raw_twos_complement() {
        // §9.2.7.1: raw signed two's-complement at raw_bits width. The
        // RFC's example is included: with 3 bits, -1 is 0b111.
        // [-1, 2, -2] at 3 bits → 111 010 110 → 0xEB, 0x0 (1 slack bit).
        let mut r = BitReader::new(&[0xEB, 0x00]);
        let mut out = [0i32; 3];
        decode_rice_partition(&mut r, &escape_header(3, 3), &mut out).unwrap();
        assert_eq!(out, [-1, 2, -2]);
        assert_eq!(r.bit_position(), 9);
    }

    #[test]
    fn escape_partition_zero_bits_consumes_nothing() {
        // §9.2.7.1: "the number of bits ... is 0 ... all residual samples
        // have a value of 0 and no bits are used". The partition is *just*
        // the escape code + 0b00000 (the caller's), so the empty rest of the
        // buffer must be left untouched.
        let mut r = BitReader::new(&[0xAB]); // must not be read
        let mut out = [0xFFi32; 3];
        decode_rice_partition(&mut r, &escape_header(0, 3), &mut out).unwrap();
        assert_eq!(out, [0, 0, 0]);
        assert_eq!(r.bit_position(), 0); // zero bits consumed
    }

    #[test]
    fn rejects_the_forbidden_i32_min_residual() {
        // §9.2.7.3: residual MUST NOT be i32::MIN, i.e. folded 0xFFFF_FFFF.
        // Reachable witness without a 4-billion-bit unary run: order 30
        // (legal in a 5-bit parameter), quotient 3, remainder all-ones:
        // folded = (3 << 30) | 0x3FFF_FFFF = 0xFFFF_FFFF.
        // Bits: 0001 (unary 3) + 30 ones = 34 bits.
        let mut r = BitReader::new(&[0x1F, 0xFF, 0xFF, 0xFF, 0xC0]);
        let mut out = [0i32; 1];
        assert_eq!(
            decode_rice_partition(&mut r, &rice_header(30, 1), &mut out),
            Err(Error::InvalidField)
        );
    }

    #[test]
    fn rejects_quotients_that_cannot_fit_u32() {
        // A folded value ≥ 2^32 cannot exist under §9.2.7.3 either. Order
        // 30, quotient 4: 4 << 30 overflows u32 → checked_shl rejects.
        // Bits: 00001 (unary 4) + 30 zero remainder bits = 35 bits.
        let mut r = BitReader::new(&[0x08, 0x00, 0x00, 0x00, 0x00]);
        let mut out = [0i32; 1];
        assert_eq!(
            decode_rice_partition(&mut r, &rice_header(30, 1), &mut out),
            Err(Error::InvalidField)
        );
    }

    #[test]
    fn out_slice_length_must_match_sample_count() {
        // Caller-contract violation, checked before any bit is consumed.
        let mut r = BitReader::new(&[0xFF; 4]);
        let mut out = [0i32; 1];
        assert_eq!(
            decode_rice_partition(&mut r, &rice_header(1, 2), &mut out),
            Err(Error::InvalidField)
        );
        assert_eq!(r.bit_position(), 0);
    }

    #[test]
    fn truncated_stream_fails_as_end_of_stream() {
        // Order 2, two samples; only the first codeword + two unary zeros
        // of the second fit. First sample must decode; second fails as EOF
        // with per-read (not per-partition) cursor atomicity.
        // Sample 1: fold 15 → q=3 ("0001"), r=3 ("11") → 0b0001_11xx.
        let mut r = BitReader::new(&[0b0001_1100]);
        let mut out = [-7i32; 2];
        assert_eq!(
            decode_rice_partition(&mut r, &rice_header(2, 2), &mut out),
            Err(Error::EndOfStream)
        );
        // fold 15 is odd → !(15 >> 1) = !7 = -8 (hand-computed, not via the
        // map under test elsewhere): the first sample must have landed.
        assert_eq!(out[0], -8);
        assert_eq!(out[1], -7); // second never written
    }

    #[test]
    fn rice_sweep_vs_packer_oracle() {
        // Encode straight from the spec text, decode through the impl, and
        // compare — at every Rice parameter 1..=14 (0 is covered by the
        // hand-built order-0 test) and at every start bit alignment, so
        // byte-crossing quotients and remainders are witnessed rather than
        // just the aligned happy path.
        const RESIDUALS: [i32; 12] = [0, -1, 1, -2, 2, 19, -20, 3, -4, 5, 6, -7];
        for order in 1u32..=14 {
            for align in 0u32..8 {
                let mut buf = [0u8; 96];
                let mut p = Packer::new(&mut buf);
                // `align` filler bits: put() with width 0 is a no-op, so the
                // aligned case needs no special case.
                p.put(0xAB & ((1 << align) - 1), align);
                for &x in &RESIDUALS {
                    rice_put(&mut p, order, x);
                }
                let packed_bits = p.pos;
                drop(p); // release &mut buf before the reader borrows it
                let mut r = BitReader::new(&buf);
                if align > 0 {
                    // Walk past the filler — `read_bits(0)` would be an
                    // InvalidField, hence the guard.
                    r.read_bits(align).unwrap();
                }
                let mut out = [0i32; RESIDUALS.len()];
                decode_rice_partition(&mut r, &rice_header(order as u8, 12), &mut out)
                    .unwrap_or_else(|e| panic!("order {order} align {align}: {e:?}"));
                assert_eq!(out, RESIDUALS, "order {order} align {align}");
                // The cursor lands exactly past the last codeword — no bit
                // of slack eaten, none left inside the word.
                assert_eq!(
                    r.bit_position(),
                    packed_bits,
                    "order {order} align {align}: cursor"
                );
            }
        }
    }

    // ---- decode_residual (step 3c) ---------------------------------------

    /// Write a residual header (§9.2.7): coding method + partition order.
    fn residual_header(p: &mut Packer<'_>, method: u32, partition_order: u32) {
        p.put(method, 2);
        p.put(partition_order, 4);
    }

    /// The RFC 9639 Appendix D.2.7 witness: **real libFLAC output** (Example
    /// File 2, vendor "reference libFLAC 1.3.3") — from the first frame's
    /// first byte (0x88) through the last residual byte (0xAB), plus the one
    /// byte at 0xAC where subframe 1 begins (its type byte witnesses the
    /// cursor landing exactly at the subframe boundary).
    ///
    /// The RFC walks this exact file bit-by-bit (Tables 36–39) and verifies
    /// the stream's MD5 (D.2.9): these bytes and the published intermediate
    /// values are encoder ground truth — no new encode, no hand-packing.
    ///
    /// Frame: blocksize 16 (uncommon 8-bit code), side-right @ 16-bit, so
    /// subframe 0 is the side at 17 bits; FIXED order 1, wasted 0, warm-up
    /// 4302. Residual: 4-bit-parameter Rice, partition order 0, parameter 11.
    const APPENDIX_D_FRAME: [u8; 37] = [
        0xFF, 0xF8, 0x69, 0x98, 0x00, 0x0F, 0x99, 0x12, // header + subframe type
        0x08, 0x67, 0x01, 0x62, 0x3D, 0x14, 0x42, 0x99, // warm-up, header, words
        0x8F, 0x5D, 0xF7, 0x0D, 0x6F, 0xE0, 0x0C, 0x17, // Rice codewords
        0xCA, 0xEB, 0x21, 0x00, 0x0E, 0xE7, 0xA7, 0x7A, // Rice codewords
        0x24, 0xA1, 0x59, 0x0C, // last codewords; 0xAC+0 = subframe 1
        0x12, // subframe 1 type byte: pad 0 + Fixed(1) + wasted flag 0
    ];

    /// RFC 9639 Table 39: the 15 residual samples of that subframe, published
    /// by the spec itself ("All data in this appendix has been thoroughly
    /// verified").
    const APPENDIX_D_TABLE39: [i32; 15] = [
        3194, -1297, 1228, -943, 952, -696, 768, -524, 599, -401, -13172, -316, 274, -267, 134,
    ];

    #[test]
    fn decode_residual_reproduces_rfc_appendix_d() {
        use crate::frame::{FrameHeader, StreamDefaults};
        use crate::subframe::SubframeType;

        let defaults = StreamDefaults {
            sample_rate_hz: 44100,
            bits_per_sample: 16,
        };
        let mut r = BitReader::new(&APPENDIX_D_FRAME);

        // Layer 1: the frame header, through the already-witnessed parse.
        let header = FrameHeader::parse(&mut r, &defaults).expect("real frame header");
        assert_eq!(header.blocksize, 16, "uncommon 8-bit blocksize 0x0F + 1");

        // Layer 2: subframe header, through the witnessed type parse + wasted.
        let st = SubframeType::parse(&mut r).expect("real subframe header");
        assert_eq!(st, SubframeType::Fixed(1), "Table 37");
        assert_eq!(r.read_wasted_bits().unwrap(), 0, "Table 37: no wasted bits");

        // Layer 3: the warm-up — 17 bits = (16 + 1) × order 1, the side
        // subframe's +1. `PredictorState::fill` (3d) will do this read; here
        // it is a bare read_signed so the residual test owns one honest step.
        let warm_up = r.read_signed(17).unwrap();
        assert_eq!(warm_up, 4302, "Table 40's warm-up sample");

        // Layer 4: the residual — the step 3c subject.
        let mut out = [0i32; 15]; // blocksize 16 − order 1
        decode_residual(&mut r, 16, 1, &mut out).expect("real residual");
        assert_eq!(out, APPENDIX_D_TABLE39, "RFC Table 39, bit-exact");

        // Cursor: 288 bits = byte 0xAC+0, exactly where the RFC's next subframe
        // begins — no field over/under-read anywhere in the chain.
        assert_eq!(r.bit_position(), 36 * 8, "residual ends at 0xAC+0");
        // And what follows is a real subframe header: pad 0, FIXED order 1.
        assert_eq!(
            SubframeType::parse(&mut r).unwrap(),
            SubframeType::Fixed(1),
            "subframe 1 begins exactly at the cursor"
        );
    }

    #[test]
    fn multi_partition_counts_and_cursor_exact() {
        // blocksize 8, order 2, partition order 1 → two partitions of 4
        // slots: partition 0 carries 4 − 2 = 2 residual samples, partition 1
        // carries 4 (§9.2.7's count rules — the first-partition penalty is
        // the whole point of this test). Deliberately different parameters
        // per partition: a mis-wired parameter desyncs the cursor immediately.
        // 2 + 4 = 6 = blocksize − order.
        let residuals: [i32; 6] = [-3, 7, 0, -1, 2, -2];
        let mut buf = [0u8; 64];
        let mut p = Packer::new(&mut buf);
        residual_header(&mut p, 0b00, 1);
        p.put(1, 4); // partition 0: parameter 1
        for &x in &residuals[..2] {
            rice_put(&mut p, 1, x);
        }
        p.put(2, 4); // partition 1: parameter 2
        for &x in &residuals[2..] {
            rice_put(&mut p, 2, x);
        }
        let packed = p.pos;
        drop(p);
        let mut r = BitReader::new(&buf);
        let mut out = [0i32; 6]; // 8 − 2
        decode_residual(&mut r, 8, 2, &mut out).unwrap();
        assert_eq!(out, residuals, "partition boundary values");
        assert_eq!(r.bit_position(), packed, "cursor at pack end");
    }

    #[test]
    fn mixed_escape_and_rice_partitions() {
        // blocksize 8, order 0, partition order 1: partition 0 Rice-coded
        // (parameter 1), partition 1 escaped (0b1111 + 5-bit width 4, raw
        // two's-complement samples). All values fit 4-bit two's complement,
        // so masked == original.
        let mut buf = [0u8; 32];
        let mut p = Packer::new(&mut buf);
        residual_header(&mut p, 0b00, 1);
        p.put(1, 4);
        for &x in &[0i32, -1, 2, -3] {
            rice_put(&mut p, 1, x);
        }
        p.put(0b1111, 4); // escape
        p.put(4, 5); // raw width
        for &x in &[-4i32, 3, 0, -1] {
            p.put((x as u32) & 0xF, 4);
        }
        let packed = p.pos;
        drop(p);
        let mut r = BitReader::new(&buf);
        let mut out = [0i32; 8]; // 8 − 0
        decode_residual(&mut r, 8, 0, &mut out).unwrap();
        assert_eq!(out, [0, -1, 2, -3, -4, 3, 0, -1]);
        assert_eq!(r.bit_position(), packed);
    }

    #[test]
    fn escape_zero_width_inside_residual_consumes_nothing() {
        // §9.2.7.1: an escaped partition with width 0 carries no bits at all —
        // every sample is 0. blocksize 4, order 0, partition order 0, single
        // escaped partition: header 6 bits + escape 4 + width 5 = 15 bits and
        // nothing else.
        let mut buf = [0u8; 8];
        let mut p = Packer::new(&mut buf);
        residual_header(&mut p, 0b00, 0);
        p.put(0b1111, 4); // escape
        p.put(0, 5); // width 0
        let packed = p.pos; // 15 bits — no body follows
        drop(p);
        let mut r = BitReader::new(&buf);
        let mut out = [-9i32; 4];
        decode_residual(&mut r, 4, 0, &mut out).unwrap();
        assert_eq!(out, [0, 0, 0, 0]);
        assert_eq!(
            r.bit_position(),
            packed,
            "width-0 escape reads zero body bits"
        );
    }

    #[test]
    fn five_bit_method_reaches_wider_parameters() {
        // Method 0b01: 5-bit parameters (escape 0b11111). Parameter 20 is
        // unrepresentable under the 4-bit method, so this witnesses the
        // parameter width actually following the method bits.
        let residuals = [0i32, -1, 3, -4];
        let mut buf = [0u8; 32];
        let mut p = Packer::new(&mut buf);
        residual_header(&mut p, 0b01, 0);
        p.put(20, 5);
        for &x in &residuals {
            rice_put(&mut p, 20, x);
        }
        let packed = p.pos;
        drop(p);
        let mut r = BitReader::new(&buf);
        let mut out = [0i32; 4]; // 4 − 0
        decode_residual(&mut r, 4, 0, &mut out).unwrap();
        assert_eq!(out, residuals);
        assert_eq!(r.bit_position(), packed);
    }

    #[test]
    fn rejects_reserved_coding_methods() {
        // Table 23: 0b10 and 0b11 are reserved. blocksize 1 / order 0: the
        // contract check passes, and the method read is what rejects — before
        // the partition-order read, so the probe never reaches a body.
        for method in [0b10u32, 0b11] {
            assert_eq!(
                residual_probe(method, 0, 1, 0),
                Err(Error::InvalidField),
                "method {method:#04b} must be InvalidField"
            );
        }
    }

    /// Pack a residual-header probe stream (method + partition order) and
    /// assert the decode outcome — header bits always via the `Packer`, never
    /// hand-packed byte arithmetic: an early draft of these tests packed
    /// `0b0000_0001` meaning "partition order 1", but MSB-first that byte is
    /// method `00` + order `0b0000` = 0, which is legal for blocksize 9, so
    /// the test passed… on the wrong rule. The packer is the only honest way
    /// to name these fields.
    fn residual_probe(
        method: u32,
        partition_order: u32,
        blocksize: usize,
        predictor_order: usize,
    ) -> crate::Result<()> {
        let mut buf = [0u8; 2];
        let mut p = Packer::new(&mut buf);
        residual_header(&mut p, method, partition_order);
        drop(p);
        // `buf` holds 16 bits: header + 10 junk bits, enough for any
        // header-level rejection and enough to EOF any body-level probe.
        let total = blocksize - predictor_order;
        let mut r = BitReader::new(&buf);
        // Fixed stack scratch sliced to length (core-only tests: no `vec!`).
        // 4096 covers the widest probe (blocksize 4096 − order 4 = 4086).
        let mut scratch = [0i32; 4096];
        let out = &mut scratch[..total];
        decode_residual(&mut r, blocksize, predictor_order, out)
    }

    #[test]
    fn rejects_indivisible_blocksize() {
        // "the block size MUST be evenly divisible by the number of
        // partitions": blocksize 9, partition order 1.
        assert_eq!(
            residual_probe(0b00, 1, 9, 0),
            Err(Error::InvalidField),
            "9 samples cannot split into 2 partitions"
        );
        // Only partition order 0 is legal for odd blocksizes — and order 0
        // gets through the divisibility gate (it then EOFs on the parameter
        // read, which proves the gate passed it rather than rejected it).
        assert_eq!(
            residual_probe(0b00, 0, 9, 0),
            Err(Error::EndOfStream),
            "order 0 must pass the divisibility gate"
        );
    }

    #[test]
    fn rejects_partitions_too_small_for_the_predictor() {
        // blocksize 16, order 4, partition order 2 → 16 >> 2 = 4, NOT larger
        // than 4 (the §9.2.7 MUST).
        assert_eq!(
            residual_probe(0b00, 2, 16, 4),
            Err(Error::InvalidField),
            "16 >> 2 = 4 is not larger than predictor order 4"
        );
        // The RFC's own worked bound: blocksize 4096, order 4 ⇒ partition
        // order 9 is the largest legal; order 10 gives 4096 >> 10 = 4.
        assert_eq!(
            residual_probe(0b00, 10, 4096, 4),
            Err(Error::InvalidField),
            "4096 >> 10 = 4 is not larger than predictor order 4"
        );
        // The boundary itself must PASS the gate: order 9 → 4096 >> 9 = 8 > 4.
        // (It then EOFs on the first parameter read — 10 junk bits aren't a
        // partition body — which is the proof the MUST let it through.)
        assert_eq!(
            residual_probe(0b00, 9, 4096, 4),
            Err(Error::EndOfStream),
            "partition order 9 is the largest legal for blocksize 4096"
        );
    }

    #[test]
    fn out_length_mismatch_is_a_cursor_free_contract_violation() {
        // blocksize − order must equal out.len(); rejected before any read.
        let mut r = BitReader::new(&[0x00; 4]);
        let mut out = [0i32; 7]; // blocksize 8 order 0 wants 8
        assert_eq!(
            decode_residual(&mut r, 8, 0, &mut out),
            Err(Error::InvalidField)
        );
        assert_eq!(r.bit_position(), 0, "contract check precedes every read");
        // order > blocksize is a caller bug to reject, not a panic to hit.
        let mut out = [0i32; 0];
        assert_eq!(
            decode_residual(&mut r, 2, 3, &mut out),
            Err(Error::InvalidField)
        );
    }

    #[test]
    fn escape_sweep_raw_widths_vs_packer_oracle() {
        // Same rule for the escape path: pack masked words at every legal
        // raw width 1..=31 (the 5-bit field's whole range; the GBA profile
        // tops out at 16, the format does not), decode via read_signed, and
        // pin the cursor.
        //
        // Expectations are derived from the *packed* bits, not from the
        // original sample: a value only round-trips at widths wide enough to
        // hold it (-128 needs 8 bits), and asserting the raw samples back
        // would encode exactly the "recalled" bug this project keeps
        // catching. The sign-extension oracle is subtract-based
        // (`m ≥ 2^(w-1) → m − 2^w`), a different mechanism than
        // read_signed's shl-then-asr.
        const SAMPLES: [i32; 10] = [0, -1, 1, -2, 2, -64, 63, -128, 32_767, -32_768];
        for raw in 1u32..=31 {
            let mask = (1u32 << raw) - 1;
            let half = 1i64 << (raw - 1);
            let whole = 1i64 << raw;
            let mut buf = [0u8; 64];
            let mut p = Packer::new(&mut buf);
            let mut expect = [0i32; SAMPLES.len()];
            for (i, &x) in SAMPLES.iter().enumerate() {
                let masked = (x as u32) & mask;
                p.put(masked, raw);
                let m = i64::from(masked);
                expect[i] = if m >= half { m - whole } else { m } as i32;
            }
            let packed_bits = p.pos;
            drop(p); // release &mut buf before the reader borrows it
            let mut r = BitReader::new(&buf);
            let mut out = [0i32; SAMPLES.len()];
            decode_rice_partition(
                &mut r,
                &escape_header(raw as u8, SAMPLES.len() as u16),
                &mut out,
            )
            .unwrap_or_else(|e| panic!("raw {raw}: {e:?}"));
            assert_eq!(out, expect, "raw {raw}");
            assert_eq!(r.bit_position(), packed_bits, "raw {raw}: cursor");
        }
    }
}
