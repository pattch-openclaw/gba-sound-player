//! The core's **second** Rice mechanism: the portable hybrid Rice kernel, the
//! pure-Rust form of the codeword loop the E10 probe wrote in ARM-mode assembly
//! (split-ladder item 5, PR 3).
//!
//! Why a second mechanism exists, and what this one is for, is in
//! [`crate::residual`]'s module docs; what lives *where* is item 5's
//! "core-vs-overlay split" record. This module is the half of that split the
//! split-ladder proposed as the *portable default*: the three-tier codeword
//! walk, lifted from the probe rather than written again.
//!
//! ```text
//! tier 1  CTZ8[w & 0xFF]                  a run of 0..=7 in one table lookup
//! tier 2  ctz32(w)                        a run of 8..=31: De Bruijn multiply +
//!                                         shift + one `ldrb` — branch-free for
//!                                         any position
//! tier 3  word-skip                       a run past the window: test whole
//!                                         words, 32 bits at a time, then walk
//!                                         a partial word byte by byte
//! refill  REV8                            bit-reversed byte merges, never a
//!                                         bit-wide shift loop
//! ```
//!
//! Those are the same four stages the probe's kernel `.Lhybrid_partition` ran
//! (and, in a different shape, the same ones the core's own
//! [`BitReader::rice_quotient`] runs today: a table-driven count over inverted
//! top bytes, 8 bits at a time). The difference is *where the loop lives*:
//! [`BitReader::rice_quotient`] answers one quotient and returns, so the walk
//! crosses the reader boundary once per sample; [`HybridRiceKernel`] owns the
//! whole partition and keeps the window in registers across samples, talking to
//! the reader only at the partition boundary — which is exactly what
//! [`BitReader::kernel_window`] / [`BitReader::set_kernel_window`] exist for,
//! and the reason PR 1's bridge is the shape it is (whole-partition, not
//! per-sample). The kernel's working state *is* a [`KernelWindow`].
//!
//! **Lifted, not re-invented — and the divergences are stated, not papered
//! over.** The probe's twin lived in `examples/arm-kernels/src/lib.rs`
//! (`twin_partition`, the host-side twin ARM mode's asm was witnessed against)
//! and `rice_partition_kernel` (the seam that fed it a real `BitReader`'s
//! domain). That seam was probe-local, and its docs named the reason: "the
//! core's loop enforces the §9.2.7.3 value limits and this driver does not —
//! the twin's *result* is compared, its raw value is not." Here the kernel is
//! the thing the core's `decode_rice_partition_with` calls, so the guards and
//! the rice unmap are inside the loop (their position is forced: the folded
//! value *is* the sample, so the guard cannot run after it). That is the one
//! real divergence from the probe, and it closes against PR 2's open item: the
//! probe's host-side twin is now redundant, and item 6 should lift *this* one.
//! The other two are smaller and both are the reader's contract, not the
//! probe's:
//!
//! * **`order` up to 30, not 15.** The probe drove only 4-bit Rice parameters.
//!   The 5-bit escape-free range reaches 30, so the remainder may be wider than
//!   two bytes; see [`remainder_from_window`]'s cold leg.
//! * **EOF is atomic the reader's way.** A truncated *unary run* restores the
//!   codeword start (as [`BitReader::rice_quotient`] does); a truncated
//!   *remainder* leaves the cursor exactly past the consumed quotient (as
//!   `read_bits` does). The probe's twin carried a status *value* out of the
//!   loop and the caller reconstructed a position from it — it never had to
//!   reproduce either rule.
//!
//! **Where this twin's boundary pins differ from the probe's, and why that is
//! not a gap.** The kernel crate's committed tables pin a family of boundaries
//! — `ws_vectors.rs` the word-skip's (`q ∈ {0, 31, 32, 63, 64, 65, 96, 127}`,
//! budgets 0..=4, all-zero slices answered as data rather than an error),
//! `ss_vectors.rs` the composite's (`q = 31` at `p ∈ {1, 8, 14}` answered with
//! `rem = 0`, `p = 0` as an order-0 skip, the unterminated family). Those pins
//! are written against the probe's **fixed four-word window with no refill**:
//! `rem = 0` at `q = 31` is what a window that ends at the terminator answers,
//! and "unterminated" is a *status value* the probe returns because its caller
//! owns the reading. A core kernel that answered either way would decode wrong
//! samples: the `q = 31` codeword's remainder lives in the **following** bits,
//! and an unterminated run at the end of the buffer is [`Error::EndOfStream`],
//! not data. So the same boundaries are pinned here in the core's terms — the
//! spelled `qdb == 31` shift leg, the empty window as a *handoff* (never an
//! answer), the `order == 0` mask leg, EOF's two cursor rules above — by unit
//! tests at the bottom of this file, and every one of them is additionally
//! judged against the incumbent and a spec-order oracle at the same bit
//! position. Those are the legs a real stream rarely reaches and a re-write is
//! most likely to get wrong while every real input still passes.
//!
//! **STATUS (2026-11-06, split-ladder item 5 PR 3): implemented, witnessed
//! against the incumbent, behind `feature = "rice-hybrid"`, and NOT the default in
//! any configuration.** Nothing in the crate's decode path calls this kernel:
//! `decode_rice_partition` still names
//! [`crate::residual::TableCountKernel`] on one line, and flipping that line is a
//! decision gated on an on-target cycle reading — the pre-registered rule is in the
//! plan, the reading owes it, and the host this landed on has no runner, so the
//! flip is **PR 4's** and this PR does not quietly make it. What keeps the
//! deferral from drifting is a test: the differential suite asserts at source level
//! that the one line naming the default still names the incumbent.
//!
//! It is reachable and tested through
//! [`crate::residual::decode_rice_partition_with`], which is why that driver is
//! `pub` rather than private: PR 3's differential witness is a caller outside
//! this crate, reading real partitions from `tests/subframe_body_vectors.txt`.
//! Threading the choice through `decode_residual` / the subframe and frame layers
//! is item 6's, not this PR's.
//!
//! **What this module is, in item 5's terms — and what it is not.** The *seam* is
//! PR 2's and its rationale ("why a trait at all", "why monomorphized, never
//! `dyn`", what it costs) lives in [`crate::residual`]'s docs, where a reader of
//! the seam will look; the *bridge* is PR 1's, and this kernel is its first
//! caller outside a test; the *flip* is PR 4's. What lands in the core and what
//! lands in the GBA overlay is recorded under "core-vs-overlay split" in the
//! plan's item-5 section, and this module is the half the split-ladder proposed as
//! the **portable default** — the mechanism in the core, the probe's asm left in
//! the overlay, which is the opposite of what the E10-on-target plan proposed and
//! is recorded there, not re-declared here. Three things it is *not*, stated
//! because a reader might otherwise infer them from their absence: it is **not
//! asm**, so it reaches no real device through this crate — the probe's asm stays
//! in `examples/arm-kernels/`, and nothing was moved or deleted, and its host-side
//! twin is now redundant (item 6 lifts this one); it is **not wired**, so no
//! configuration decodes with it; and it **does not replace the incumbent** —
//! `TableCountKernel` and `rice_quotient` stay, because the loop that decodes
//! today is a controlled input every later rung is measured against.
//!
//! **Where its own witness obligation is discharged.** The plan's PR-3 cell names
//! a "two-mechanism differential witness", and that is a corpus obligation: only a
//! real stream carries real Rice parameters, and only the corpus can convict a
//! mechanism that consumes one bit too many at a real boundary. So the corpus half
//! is `tests/rice_two_mechanism_differential.rs` — four independent walks over
//! every partition in the corpus, described there — and it is *not* here, because
//! `src/` cannot read a corpus. What is here is the legs a real stream rarely
//! reaches and a re-write is most likely to get wrong while every real input still
//! passes: the `qdb == 31` leg, the empty-window handoff, EOF's two cursor rules,
//! the order-0 mask leg, truncation against the incumbent, out-of-range orders, and
//! a spec-order oracle over every length. A test that passes against both
//! mechanisms does not convict either; the in-crate mutation series (M1…M5) is
//! written in the PR record, and it convicts exactly where the real corpus does.
//! And no claim of the kind this host cannot support appears in this file or its
//! tests: this PR makes no GBA real-time claim, and the cycle reading is PR 4's.
//!
//! **Cost, measured rather than asserted.** The three tables are 256 + 256 + 32
//! = **544 bytes**, the exact amount the E10-on-target proposal budgeted for the
//! hybrid — this is the PR that pays it, since PR 1 deliberately carried no
//! data. They are `iwram-pin`-gated exactly as `LEADING_ONES` is (see the two
//! notes above the tables: why pinning them is the same argument, and what the
//! budget line means), and the `nm` witness on a build with them live is in the
//! record. The code the probe paid to measure this kernel's mechanism — the
//! kernel crate's asm, and the probe's `run_kernel_asm` / `.text.asm`
//! relocation — is *not* part of it: the core's version is ordinary Rust, with
//! no asm and no relocation, and item 6's GBA overlay gets to reuse it.
//!
//! But the measured cost of putting this module *in a build* is not its own code,
//! and that is why it sits behind a feature. With `rice-hybrid` on, the calibration
//! instrument's `.text` grows **+2,984 B** and `nm` reports **no symbol from this
//! module in the image at all**: the growth is the decode path's own functions
//! re-inlining around a second kernel body that the linker then drops. The probe
//! arms that locate it are in the PR record: delete this `impl` block and the image
//! is the control's byte for byte; keep the block and empty its body and the image
//! is *still* the control's; keep the body reaching this walk and it costs the whole
//! amount although nothing ever calls the kernel. So the honest reading of "what it
//! costs" is: 544 B of tables, a walk the linker drops when nothing calls it, and
//! roughly 3 KB of *other people's* code for the privilege of having the choice
//! present in a build. That is 4b's lesson ("a second consumer of the decode path
//! changes what the inliner does to the code already linked") measured one rung
//! closer to the decoder, and it is why this module is gated rather than simply
//! added: a default build's image is the pre-PR-3 image, hash for hash, so any
//! later measurement of the flip attributes to the swapped mechanism alone.
//!
//! **Safety.** No `unsafe` *code*: the three statics carry the same
//! `cfg_attr(all(target_arch = "arm", feature = "iwram-pin"),
//! unsafe(link_section = ".iwram"))` placement the crate's ratified exception
//! covers. They are the only new ones, which is why the count a **default** build
//! carries is unchanged at one while a `rice-hybrid` build carries four, all of the
//! same kind — stated in `crate`'s docs rather than assumed. No allocation. No
//! `panic!` on any path: `walk` does **no array indexing at all** — every table and
//! buffer access is through `get`/`get(..)` with the bound checked — and the reason
//! is the one the kernel crate's own tests give: under Miri a table *read* is a UB
//! tripwire, so a wrong index convicts here rather than escaping into a decode path
//! that has no bounds checks to catch it. Every arithmetic step is likewise
//! range-checked by the shape described below (runs counted are never longer than
//! the bits in the region; window state stays within the invariants
//! [`KernelWindow`] states).

use crate::Error;
use crate::bits::{BitReader, KernelWindow, rev32};
use crate::residual::{RiceKernel, rice_unmap};

// ── Tables ────────────────────────────────────────────────────────────────

/// The De Bruijn multiplier the [`BRUIJN_INDEX`] lookup is built for: for any
/// 32-bit `x` with at most one low bit set, `(x * BRUIJN_K) >> 27` is a
/// distinct 5-bit value per `x`.
const BRUIJN_K: u32 = 0x077C_B531;

/// Trailing-zero counts for every byte: `CTZ8[b]` is the number of trailing
/// zeros, with **8 meaning "the run passes this byte"** — a sentinel, not an
/// answer. Exhaustive, so no bounds question arises: it is indexed by a masked
/// byte. (E10's kernel contract, `docs/plan-notes/e10-three-arm-kernel-approach.md`
/// §"Kernel contracts" #1; built exhaustively there too, for exactly the same
/// reason PR 1's table rule applies — a non-exhaustive table with a clamp would
/// move a defect into the least-tested index, and a probe whose tier-1 leg had a
/// guard-less `ldrb` had to be able to read any byte at all.)
#[cfg_attr(
    all(target_arch = "arm", feature = "iwram-pin"),
    unsafe(link_section = ".iwram")
)]
#[allow(unsafe_code)] // table placement only: the crate's ratified exception
static CTZ8: [u8; 256] = build_ctz8();

/// Byte bit-reversal, used by the refill merge and the word-skip's cold leg:
/// a stream byte read MSB-first *is* `REV8[byte]` in the reversed window (E10's
/// "byte order / bit order" identity, and the one that `e10_three_arm_readme`
/// records as having been misread once — "within-byte reversal alone" is not a
/// bit mapping; the window mapping of [`KernelWindow`] is what makes it exact).
/// Exhaustive for the same reason as [`CTZ8`].
#[cfg_attr(
    all(target_arch = "arm", feature = "iwram-pin"),
    unsafe(link_section = ".iwram")
)]
#[allow(unsafe_code)] // table placement only: the crate's ratified exception
static REV8: [u8; 256] = build_rev8();

/// De Bruijn index table: `BRUIJN_INDEX[((1 << b) * BRUIJN_K) >> 27] == b` for
/// every `b` in `0..32`. 32 bytes, indexed by the 5 high bits of a product —
/// the multiply-and-shift is what makes the index total, so there is no clamp
/// and no miss path.
#[cfg_attr(
    all(target_arch = "arm", feature = "iwram-pin"),
    unsafe(link_section = ".iwram")
)]
#[allow(unsafe_code)] // table placement only: the crate's ratified exception
static BRUIJN_INDEX: [u8; 32] = build_brujn_index();

/// `CTZ8` as a const fold (same shape as [`crate::bits`]'s `LEADING_ONES`):
/// `ctz` = number of trailing zeros, `8` if none.
const fn build_ctz8() -> [u8; 256] {
    let mut t = [0u8; 256];
    let mut i = 0;
    while i < 256 {
        let b = i as u8;
        let mut c = 0u8;
        if b == 0 {
            c = 8;
        } else {
            let mut v = b;
            while v & 1 == 0 {
                v >>= 1;
                c += 1;
            }
        }
        t[i] = c;
        i += 1;
    }
    t
}

/// `REV8` as a const fold.
const fn build_rev8() -> [u8; 256] {
    let mut t = [0u8; 256];
    let mut i = 0;
    while i < 256 {
        let b = i as u8;
        let mut r = 0u8;
        let mut j = 0;
        while j < 8 {
            r = (r << 1) | ((b >> j) & 1);
            j += 1;
        }
        t[i] = r;
        i += 1;
    }
    t
}

/// `BRUIJN_INDEX` as a const fold, and the only table in the crate that is
/// *sparse by construction*: the fold writes exactly 32 entries because the
/// product `(1 << b) * BRUIJN_K` is injective mod 2^32 for single-bit inputs.
/// Any zero left in it would mean the multiplier is not a De Bruijn constant,
/// which [`ctz32_matches_naive_walk`] would then convict — the table is not
/// hand-copied, so the property is checked rather than transcribed.
const fn build_brujn_index() -> [u8; 32] {
    let mut t = [0u8; 32];
    let mut b = 0;
    while b < 32 {
        let low = 1u32 << b;
        let idx = low.wrapping_mul(BRUIJN_K) >> 27;
        t[idx as usize] = b;
        b += 1;
    }
    t
}

// ── Primitives ────────────────────────────────────────────────────────────

/// Tier 2's count: trailing zeros of `w`, by De Bruijn multiply + shift + one
/// `ldrb` from [`BRUIJN_INDEX`] — no binary search, and branch-free for any
/// run position. `w == 0` returns **32, a sentinel meaning "no run end inside
/// the window"**, never a quotient: the call sites guarantee a non-zero `w`, and
/// keeping the sentinel is what lets the empty window be a *state* rather than
/// an answer (E10 kernel contracts #3: "the empty window is state, not
/// answer").
#[inline]
fn ctz32(w: u32) -> u32 {
    let low = w & w.wrapping_neg();
    if low == 0 {
        return 32;
    }
    // `>> 27` of a u32 is by construction in 0..=31 — no mask, no bounds
    // question, and the table is exhaustive over that range (see the fold).
    u32::from(BRUIJN_INDEX[(low.wrapping_mul(BRUIJN_K) >> 27) as usize])
}

/// The shift every count path ends with, spelled the way E10's kernel contract
/// #2 spells the `q = 31` leg: `w >> (qdb + 1)` is a shift *by 32*, which the
/// ARM instruction set answers as zero, and Rust does not answer at all.
///
/// A `match` with an explicit arm, because the leg is reached from `qdb == 31`
/// — which is `0xFFFFFFFF`'s run, a real codeword the corpus does not happen to
/// contain — and a silent wrap there would fold a wrong sample into the decode
/// instead of failing. `w >> 32` written directly is the same claim, untestable
/// in Rust; E10 measured the claim rather than trusting the mapping table that
/// implied it — `ss_vectors.rs`'s `q = 31` rows (answered `rem = 0`) and
/// `ws_vectors.rs`'s `q = 31 / q = 32` boundary pins are that measurement, in
/// the probe's no-refill window domain (see the module note on where the two
/// sets of pins differ). The leg here is the same shift with the refill on the
/// other side of it, and it is witnessed against the incumbent and the
/// spec-order oracle at the same bit position, not against the probe's table.
#[inline]
fn shift_past_run(w: u32, qdb: u32) -> u32 {
    match qdb {
        31 => 0,
        _ => w >> (qdb + 1),
    }
}

/// The binary remainder, read out of the *reversed* window and put back the
/// right way round.
///
/// `rem_w` is the low `order` bits of `w`, which hold the next `order` stream
/// bits nearest-first — so `rem_w`'s bit `j` is stream bit `order-1-j`, i.e.
/// `rem_w` is the wanted value with its bits reversed. The warm leg handles
/// `order ≤ 16` with two `REV8` lookups + `>> (16 - order)` — the shape E10 PR B
/// shipped as its `.Lrem_have` leg ("the kernel un-reverses them (`rev16` via
/// two `REV8` lookups + `>> (16−p)`)", the PR B record) — and the cold leg the
/// identity `rev_p(x) = rev32(x << (32-p))` for `order` in `17..=31`.
///
/// Which orders actually reach this function is [`emit`]'s policy (a remainder
/// the window cannot hold goes through the reader); the legs are written across
/// the whole 17..=31 range so a window answer never depends on that policy.
/// The cold leg is **not the probe's**: the probe's kernel drove
/// 4-bit parameters only and its committed grid stops at `p = 14`
/// (`ss_vectors.rs`'s `p ∈ {0,1,2,3,4,8,14}`), so nothing in this tree has ever
/// measured a remainder that wide. It exists because the core's Rice range does:
/// `0b11111` is the *escape*, so the 5-bit parameter space reaches order 30 and
/// a remainder can be wider than two bytes.
///
/// `order == 0` returns 0 without touching a table — the `order = 0` skip the
/// probe pinned as its own row (`ss_vectors.rs`: "the p = 0 order-0 skip at
/// q = 0"). Here the mask is zero anyway, so the leg is only about not shifting
/// by 32 in the cold arm and not indexing a table with a meaningless value.
#[inline]
fn remainder_from_window(rem_w: u32, order: u32) -> u32 {
    if order == 0 {
        return 0;
    }
    if order <= 16 {
        let lo = u32::from(REV8[(rem_w & 0xFF) as usize]);
        let hi = u32::from(REV8[((rem_w >> 8) & 0xFF) as usize]);
        // `lo` is the low byte reversed, i.e. the wanted value's high byte.
        ((lo << 8) | hi) >> (16 - order)
    } else {
        // `order` in 17..=31: `rem_w` holds at most `order` bits, so the left
        // shift is exact and never drops one.
        rev32(rem_w << (32 - order))
    }
}

/// `REV8`'s use as a 32-bit window load, the form the word-skip's cold leg
/// needs: four table lookups and the three shifts the byte load already pays
/// for. Applied to a word assembled **in stream order in the low byte** (the
/// little-endian assembly the code below does), it is byte-wise reversal with
/// **no** byte swap:
///
/// ```text
/// w = REV8[b0] | REV8[b1] << 8 | REV8[b2] << 16 | REV8[b3] << 24
/// ```
///
/// because bit 0 of the window is the *first* stream bit, which is the MSB of
/// the earliest byte — and `REV8` puts a byte's MSB at its bit 0. The two
/// claims "a bit-reversed load" and "a byte-swapped bit-reversal" are one
/// apart, and E10's record (`e10_three_arm_readme`) says this exact distinction
/// was misread once — "within-byte reversal alone" is not a bit mapping. So it
/// is witnessed, twice over, by [`rev4_continues_the_window_the_way_rev32_does`]:
/// against [`rev32`] of the same four bytes assembled **big-endian** (which is
/// what the reader's own `acc` would hold, and what `kernel_window` would hand
/// out), and against the byte-level continuation the window contract is made
/// of.
#[inline]
fn rev4(word: u32) -> u32 {
    let b0 = u32::from(REV8[(word & 0xFF) as usize]);
    let b1 = u32::from(REV8[((word >> 8) & 0xFF) as usize]);
    let b2 = u32::from(REV8[((word >> 16) & 0xFF) as usize]);
    let b3 = u32::from(REV8[((word >> 24) & 0xFF) as usize]);
    b0 | (b1 << 8) | (b2 << 16) | (b3 << 24)
}

// ── The kernel ────────────────────────────────────────────────────────────

/// Why a walk stopped before filling `out`, with the window state the reader
/// must be left in.
///
/// The variant is decided by *which read* ran out, and the state carries that:
/// a truncated unary run hands back the snapshot taken at the codeword's start
/// (so the failed read consumed nothing — `BitReader::rice_quotient`'s rule,
/// reproduced by construction because the snapshot is taken at the same bit),
/// and a truncated remainder hands back the state exactly past the consumed
/// quotient (`read_bits`' rule). An over-large folded value hands back the
/// state after the whole codeword, because the codeword *was* consumed.
struct WalkStop {
    at: KernelWindow,
    err: Error,
}

impl WalkStop {
    /// The region ended in the middle of a unary run: nothing consumed.
    fn unterminated(at: KernelWindow) -> Self {
        Self {
            at,
            err: Error::EndOfStream,
        }
    }
}

/// Merge whole bytes into the window until it holds [`REFILL_TARGET`] bits —
/// the same rule [`BitReader`] runs internally (its `refill` tops up while
/// `valid ≤ 24`), stated in the window's own domain instead of the reader's.
///
/// The shift is safe by the loop condition: `v < 25` means `v ≤ 24`, so
/// `REV8[byte] << v` never reaches bit 32 and never needs a mask. That is the
/// whole reason the target is 25 and not 24 — E10's kernel contract #1, and the
/// reader's own choice for the same reason. It is also why 25 is a **ceiling on
/// what a merge can produce**, not a tunable: starting from ≤ 24, one merged byte
/// cannot take the window past 32, and starting from 25..=31 no byte can be
/// merged at all without dropping bits. [`emit`] re-asks for the remainder width
/// when this top-up is not enough, and past order 25 the remainder may go
/// through the reader instead (see `emit`'s two legs).
fn refill(data: &[u8], k: &mut KernelWindow) {
    while k.v < REFILL_TARGET && k.ptr < k.end {
        let Some(&b) = data.get(k.ptr) else {
            break;
        };
        k.w |= u32::from(REV8[usize::from(b)]) << k.v;
        k.v += 8;
        k.ptr += 1;
    }
}

/// The bit count the window is topped up to: the reader's threshold
/// (`valid ≤ 24` ⇒ refill) inverted, so that a full merge always lands inside
/// the 32-bit window and the largest single need below it (a 30-bit remainder,
/// or 25 bits before a tier-1/tier-2 count) is satisfied without a second call.
const REFILL_TARGET: u32 = 25;

/// The portable hybrid Rice kernel: [`RiceKernel`]'s second implementor, the
/// three-tier walk over [`KernelWindow`] state.
///
/// [`crate::residual`] decides whether this is the default; today it is not,
/// and the module docs say why. What is *not* a decision: the kernel must
/// satisfy the trait contract exactly as the incumbent does — same samples, same
/// errors, same cursor behaviour — and the differential tests are that claim.
///
/// The contract's §9.2.7.3 half is enforced here (the fold is the sample, so
/// the guard cannot live outside the loop). The one shape the contract does not
/// cover is `order ≥ 32`, which a parsed stream cannot produce (the 5-bit
/// method's `11111` is the escape) but a caller-built `ResidualHeader` can: the
/// cold arm hands such a partition to [`crate::residual::TableCountKernel`] for
/// the whole partition, so the error *and* the cursor behaviour of an
/// out-of-range order is the incumbent's by identity rather than re-derived.
/// It is reached once per partition, never per sample.
pub struct HybridRiceKernel;

impl RiceKernel for HybridRiceKernel {
    fn decode_partition(
        reader: &mut BitReader<'_>,
        order: u32,
        out: &mut [i32],
    ) -> crate::Result<()> {
        if order >= 32 {
            return crate::residual::TableCountKernel::decode_partition(reader, order, out);
        }
        let data = reader.region();
        let mut k = reader.kernel_window();
        let mask = (1u32 << order) - 1; // `order ≤ 31`: the cold arm took the rest
        if let Err(stop) = walk(data, reader, &mut k, out, order, mask) {
            reader.set_kernel_window(stop.at);
            return Err(stop.err);
        }
        reader.set_kernel_window(k);
        Ok(())
    }
}

/// The walk itself: one loop over `out`, three tiers per codeword, the window
/// kept in `k` across samples.
///
/// Label references in the comments are the probe's (`e10_rice_arm.s`'s local
/// labels for exactly these legs), because the legs are what the kernel crate's
/// committed vectors pin and a re-write is judged against them. The one leg the
/// probe has that this does not is `.Lslow`'s status return: here the state
/// *is* the answer (`v == 0` and no bytes ⇒ unterminated), which is item 5's
/// "the empty window is state, not answer" applied to the boundary.
fn walk(
    data: &[u8],
    reader: &mut BitReader<'_>,
    k: &mut KernelWindow,
    out: &mut [i32],
    order: u32,
    mask: u32,
) -> Result<(), WalkStop> {
    for sample in out.iter_mut() {
        // The codeword's start, kept for the atomicity rule above. A window is
        // four `Copy` words, so this is a register copy, not a spill — and it
        // sits at the codeword start rather than at the `.Lrefill1` join
        // because that is where the reader's own snapshot sits (the two are the
        // same bit: the previous codeword ended exactly here).
        let start = *k;

        // ── .Lrefill1: whole bytes up to a full-enough window, then either a
        //    merged path or the per-bit tail (nothing else consumes one bit at
        //    a time). ────────────────────────────────────────────────────────
        if k.v < REFILL_TARGET {
            refill(data, k);
            if k.v < REFILL_TARGET {
                // ── .Lslow: the region is exhausted inside the codeword. The
                //    tail counts bits one at a time; the window's emptiness is
                //    what says the run has no terminator. ────────────────────
                let mut q = 0u32;
                loop {
                    if k.v == 0 {
                        return Err(WalkStop::unterminated(start));
                    }
                    if k.w & 1 == 1 {
                        break;
                    }
                    q += 1;
                    k.w >>= 1;
                    k.v -= 1;
                }
                k.w >>= 1; // the terminating 1: consumed, not counted
                k.v -= 1;
                emit(data, reader, k, sample, q, order, mask)?;
                continue;
            }
        }

        // ── .Ltier1: one `ldrb` answers a run of 0..=7. ────────────────────
        let c = u32::from(CTZ8[(k.w & 0xFF) as usize]);
        if c < 8 {
            // `v ≥ 25` on this path, so the run lies entirely in valid bits and
            // `c + 1 ≤ 8 ≤ v`; the shift and the count are both in range.
            k.w >>= c + 1;
            k.v -= c + 1;
            emit(data, reader, k, sample, c, order, mask)?;
            continue;
        }

        // ── .Ltier23: the low byte is all zeros, so the run passes it. `w == 0`
        //    is the *handoff signal* here, not a quotient — the whole window
        //    being zeros is a state that says "go count bytes", and a run of
        //    that length cannot be answered by any count of the window. ──────
        if k.w != 0 {
            // ── .Llzc: tier 2, base 0. A non-zero `w` with valid bits above the
            //    run means the run ends inside the window: `qdb ≤ v-1`, so
            //    `qdb + 1 ≤ v` and the shift/count are in range. ─────────────
            let qdb = ctz32(k.w);
            k.w = shift_past_run(k.w, qdb);
            k.v -= qdb + 1;
            emit(data, reader, k, sample, qdb, order, mask)?;
            continue;
        }

        // ── .Lt3: tier 3. Everything valid was a zero; count the bytes. ─────
        if k.ptr >= k.end {
            return Err(WalkStop::unterminated(start));
        }
        let mut base = k.v;
        k.v = 0; // `w` is already zero — that is what this branch tested
        // .Lwskip4: the region's byte cursor is aligned (the byte-pointer
        // invariant), so whole words can be tested as words. A zero word is 32
        // zeros; the first non-zero word is loaded reversed, `REV8`'s use #2.
        loop {
            match data.get(k.ptr..k.ptr + 4) {
                Some(four) => {
                    let word = u32::from(four[0])
                        | (u32::from(four[1]) << 8)
                        | (u32::from(four[2]) << 16)
                        | (u32::from(four[3]) << 24);
                    k.ptr += 4;
                    if word != 0 {
                        k.w = rev4(word);
                        k.v = 32;
                        break;
                    }
                    base += 32;
                }
                None => {
                    // .Lwslow: fewer than four bytes ahead — the per-byte walk.
                    // The probe's own record treats this leg as load-bearing
                    // (the 254-sample tail of its corpus ended in a 2-byte
                    // partial word), so it is a leg, not an edge.
                    loop {
                        let Some(&b) = data.get(k.ptr) else {
                            return Err(WalkStop::unterminated(start));
                        };
                        k.ptr += 1;
                        if b != 0 {
                            k.w = u32::from(REV8[usize::from(b)]);
                            k.v = 8;
                            break;
                        }
                        base += 8;
                    }
                    break;
                }
            }
        }
        // From here it is the tail of .Llzc: the word is non-zero, so the run
        // ends inside it, at `qdb ≤ v-1` again.
        let qdb = ctz32(k.w);
        k.w = shift_past_run(k.w, qdb);
        k.v -= qdb + 1;
        // The run's total length: the bits already known to be zeros, plus the
        // count inside the word just loaded. Plain `+`, like the incumbent's
        // `quotient += 8` loop in `rice_quotient`: a run long enough to overflow
        // `u32` is a region of more than 2^32 bits (512 MB) of zeros, and both
        // mechanisms answer that the same way — a debug-build arithmetic panic,
        // a wrapped value in release. Neither is a stream any decoder is asked
        // about, and making one of them saturate instead would give *different*
        // answers for the same stream, which is the one thing the pair may not
        // do.
        let q_total = base + qdb;
        emit(data, reader, k, sample, q_total, order, mask)?;
    }
    Ok(())
}

/// Drain the binary remainder and emit one sample — the probe's `.Lnomask` /
/// `.Lrem_have` / guard / fold sequence, with the core's value limits.
///
/// Two legs, and the split is made by what a 32-bit window can actually serve:
///
/// * **The window leg** (`v ≥ order`): mask, shift, un-reverse — the probe's
///   `.Lrem_have` shape, and the one it measured. It covers every order a
///   [`refill`] can reach, i.e. the whole 4-bit parameter space and the 5-bit
///   one up to 25.
/// * **The reader leg** (`v < order` after the top-up): hand the window back and
///   take the remainder through [`BitReader::read_bits`]. It is not a stitch and
///   it is not new arithmetic — it is the primitive the *incumbent* uses for
///   every remainder, so this leg is bit-exact and cursor-exact with the
///   incumbent by identity, and it answers MSB-first so no reversal is needed at
///   all. It exists because a window merge can never leave the window wider
///   than 32 bits while starting from ≤ 24, so an order-26..=31 remainder is
///   not always gatherable inside the window; and because `read_bits` is atomic
///   per read, which is the one cursor rule this kernel would otherwise have to
///   re-derive for itself.
///
/// So the remainder's own shortness is the one error this can raise that is
/// *not* fully atomic: a short remainder leaves the cursor exactly past the
/// consumed quotient, because that is where the failed read leaves it. The
/// §9.2.7.3 rejections leave it after the whole codeword, because by then the
/// codeword was consumed. Both are the incumbent's behaviour — the wide leg
/// reaches it through the same call the incumbent makes — and
/// [`truncated_and_guard_legs_match_the_incumbent`] witnesses it.
fn emit(
    data: &[u8],
    reader: &mut BitReader<'_>,
    k: &mut KernelWindow,
    sample: &mut i32,
    quotient: u32,
    order: u32,
    mask: u32,
) -> Result<(), WalkStop> {
    let after_quotient = *k;
    if k.v < order {
        refill(data, k);
    }
    let remainder = if k.v >= order {
        let rem_w = k.w & mask;
        k.w >>= order; // `order ≤ 31`, and `v ≥ order` holds on this leg
        k.v -= order;
        remainder_from_window(rem_w, order)
    } else {
        reader.set_kernel_window(after_quotient);
        let value = match reader.read_bits(order) {
            Ok(value) => value,
            Err(err) => {
                return Err(WalkStop {
                    at: after_quotient,
                    err,
                });
            }
        };
        *k = reader.kernel_window();
        value
    };
    // §9.2.7.3, in the same position the incumbent has them (after the read —
    // which is where the folded value is available to test).
    if quotient > (u32::MAX >> order) {
        return Err(WalkStop {
            at: *k,
            err: Error::InvalidField,
        });
    }
    let folded = (quotient << order) | remainder;
    if folded == u32::MAX {
        return Err(WalkStop {
            at: *k,
            err: Error::InvalidField,
        });
    }
    *sample = rice_unmap(folded);
    Ok(())
}

// ── Unit tests: the tables, the primitives, the legs ──────────────────────
//
// The tables and primitives here have no independent implementation anywhere
// else in the crate, so their oracle is a naive bit walk written in the test
// (the rule bits.rs states for its own oracles: derived from the spec text, not
// from the function under test). The *kernel's* differential against the
// incumbent over real partitions is the integration test
// `tests/rice_two_mechanism_differential.rs`; these tests are the hand-built
// windows the corpus does not contain, where a two-mechanism comparison is most
// likely to miss because neither mechanism's caller reaches them.

#[cfg(test)]
mod tests {
    extern crate alloc;
    use alloc::format;
    use alloc::vec;
    use alloc::vec::Vec;

    use super::*;
    use crate::residual::TableCountKernel;

    // ── the three-way witness every case below runs ──────────────────────
    //
    // `want` is the *expected* answer: a successful case must land `want.len()`
    // samples with no error; a failing one must have written exactly the prefix
    // it reached and then stopped — `BitReader` guarantees a partially-read
    // sample is never published (a failed `read_bits` consumes nothing), so the
    // count after an error is a fact about the cursor, not a detail. Any real
    // prefix would do for the scratch, but a poison value makes a stale or short
    // write show up as itself rather than as a plausible leftover from the
    // previous iteration.
    const SENTINEL: i32 = i32::MIN;

    /// Run one case through both mechanisms and pin the two of them together:
    /// the samples, the count each wrote, the error, and the reader's bit
    /// position. The last is the half a sample comparison cannot see — a kernel
    /// that consumed the wrong number of bits can still produce the right
    /// samples for the partition it was handed, and then corrupt every partition
    /// after it.
    fn compare_three_way(
        label: &str,
        bytes: &[u8],
        bit_offset: usize,
        order: u32,
        n: usize,
        want: &[i32],
        expect: Option<Error>,
    ) {
        // The buffer is exactly `n` slots: `out.len()` *is* the partition's
        // sample count, and a kernel asked for one sample more than the fixture
        // contains reports a truthful `EndOfStream` for a partition that is
        // actually fine. The count each wrote is then `n` minus the untouched
        // poison at the end — and a mechanism that published *more* than the
        // walk did is caught by the length mismatch in the sample comparison.
        let mut twin = vec![SENTINEL; n];
        let mut r1 = BitReader::new(bytes);
        if bit_offset > 0 {
            r1.read_bits(bit_offset as u32).unwrap();
        }
        let twin_err = HybridRiceKernel::decode_partition(&mut r1, order, &mut twin).err();
        let twin_written = twin.iter().position(|x| *x == SENTINEL).unwrap_or(n);
        let twin_pos = r1.bit_position();

        let mut inc = vec![SENTINEL; n];
        let mut r2 = BitReader::new(bytes);
        if bit_offset > 0 {
            r2.read_bits(bit_offset as u32).unwrap();
        }
        let inc_err = TableCountKernel::decode_partition(&mut r2, order, &mut inc).err();
        let inc_written = inc.iter().position(|x| *x == SENTINEL).unwrap_or(n);
        let inc_pos = r2.bit_position();

        // The two mechanisms first: samples as far as either wrote, the cursor,
        // the error. `expect.is_none()` cases additionally pin the count to the
        // oracle's length, so a mechanism that ran past where the oracle stopped
        // is convicted rather than excused.
        let written = twin_written.max(inc_written);
        assert_eq!(
            &twin[..written],
            &inc[..written],
            "{label}: mechanisms disagree"
        );
        assert_eq!(
            twin_pos, inc_pos,
            "{label}: the cursor disagrees after the walk"
        );
        assert_eq!(
            twin_err, inc_err,
            "{label}: the error disagrees — this kernel's boundary leg is not the incumbent's"
        );
        assert_eq!(
            twin_err, expect,
            "{label}: both mechanisms answer a case the spec-order walk does not"
        );
        assert_eq!(
            inc_written, twin_written,
            "{label}: the published count disagrees"
        );
        if expect.is_none() {
            assert_eq!(twin_written, want.len(), "{label}: wrong count");
        }
        assert_eq!(
            &twin[..twin_written],
            &want[..twin_written.min(want.len())],
            "{label}: samples disagree with the spec-order walk"
        );
    }

    /// A bit-serial oracle for the whole partition walk, written from RFC
    /// 9639 §9.2.5's text alone: the unary run is "a number of zero bits
    /// followed by a one bit", the remainder is `order` bits MSB-first, the
    /// value is `(q << order) | rem` un-Zig-Zag-mapped.
    ///
    /// The *values* are the RFC's. The *cursor* on a failure is the reader's,
    /// because that is what the two mechanisms are contractually required to
    /// reproduce: `BitReader::rice_quotient` snapshots at the codeword start and
    /// restores it when the run reaches the end of the slice, so the run is one
    /// read for cursor purposes even though this walk counts it bit by bit. A
    /// walk that reported the position after the zeros it had counted would be
    /// describing a different cursor, and the comparison would convict the wrong
    /// party (which is exactly what it did, once).
    struct Oracle<'a> {
        r: BitReader<'a>,
    }

    impl Oracle<'_> {
        fn read_bit(&mut self) -> Result<u32, Error> {
            self.r.read_bits(1)
        }

        /// Returns (samples, error, bit position after the partition).
        fn partition(&mut self, order: u32, count: usize) -> (Vec<i32>, Option<Error>, usize) {
            let mut out = Vec::new();
            for _ in 0..count {
                let start = self.r.bit_position();
                let mut q = 0u32;
                loop {
                    match self.read_bit() {
                        Ok(0) => q += 1,
                        Ok(1) => break,
                        Ok(_) => unreachable!(),
                        // The unary run consumed nothing as far as the contract
                        // is concerned: report the codeword start.
                        Err(e) => return (out, Some(e), start),
                    }
                }
                // The remainder *is* a single `read_bits`: atomic, and the
                // consumed quotient stays where it is.
                let rem = if order == 0 {
                    0
                } else {
                    match self.r.read_bits(order) {
                        Ok(v) => v,
                        Err(e) => return (out, Some(e), self.r.bit_position()),
                    }
                };
                if order >= 32 || q > (u32::MAX >> order) {
                    return (out, Some(Error::InvalidField), self.r.bit_position());
                }
                let folded = (q << order) | rem;
                if folded == u32::MAX {
                    return (out, Some(Error::InvalidField), self.r.bit_position());
                }
                out.push(rice_unmap(folded));
            }
            (out, None, self.r.bit_position())
        }
    }

    /// Pack bits MSB-first, the naive way.
    struct Packer {
        bytes: Vec<u8>,
        cur: u32,
        valid: u32,
    }

    impl Packer {
        fn new() -> Self {
            Self {
                bytes: Vec::new(),
                cur: 0,
                valid: 0,
            }
        }
        fn push(&mut self, n: u32, v: u32) {
            for i in (0..n).rev() {
                self.cur = (self.cur << 1) | ((v >> i) & 1);
                self.valid += 1;
                if self.valid == 8 {
                    self.bytes.push(self.cur as u8);
                    self.cur = 0;
                    self.valid = 0;
                }
            }
        }
        fn codeword(&mut self, q: u32, order: u32, rem: u32) {
            for _ in 0..q {
                self.push(1, 0);
            }
            self.push(1, 1);
            self.push(order, rem);
        }
        fn finish(mut self) -> Vec<u8> {
            if self.valid > 0 {
                self.bytes.push((self.cur << (8 - self.valid)) as u8);
            }
            self.bytes
        }
    }

    /// Run the twin over `bytes`, starting at bit offset `off`, for `count`
    /// samples; returns (samples, error, bit position after).
    fn twin(
        bytes: &[u8],
        off: usize,
        order: u32,
        count: usize,
    ) -> (Vec<i32>, Option<Error>, usize) {
        let mut r = BitReader::new(bytes);
        if off > 0 {
            r.read_bits(off as u32).unwrap();
        }
        let mut out = vec![0i32; count];
        let err = HybridRiceKernel::decode_partition(&mut r, order, &mut out).err();
        (out, err, r.bit_position())
    }

    /// The same through the incumbent, for the legs where the incumbent's
    /// behaviour is the contract (the two must agree everywhere).
    fn incumbent(
        bytes: &[u8],
        off: usize,
        order: u32,
        count: usize,
    ) -> (Vec<i32>, Option<Error>, usize) {
        let mut r = BitReader::new(bytes);
        if off > 0 {
            r.read_bits(off as u32).unwrap();
        }
        let mut out = vec![0i32; count];
        let err =
            crate::residual::TableCountKernel::decode_partition(&mut r, order, &mut out).err();
        (out, err, r.bit_position())
    }

    fn naive_ctz32(w: u32) -> u32 {
        if w == 0 {
            return 32;
        }
        let mut c = 0;
        while w & (1 << c) == 0 {
            c += 1;
        }
        c
    }

    fn naive_rev(x: u32, width: u32) -> u32 {
        let mut r = 0u32;
        for j in 0..width {
            r |= ((x >> j) & 1) << (width - 1 - j);
        }
        r
    }

    #[test]
    fn ctz8_is_exhaustive_and_uses_8_as_the_sentinel() {
        // The table is the tier-1 answer for every byte, so every entry is
        // defined. The spec-adjacent reading of "8 means past the byte" is that
        // a full byte of zeros is a *handoff*, not a quotient of 8: E10 kernel
        // contracts #1.
        for b in 0u32..256 {
            let byte = b as u8;
            let want = if byte == 0 {
                8
            } else {
                let mut c = 0;
                while (byte >> c) & 1 == 0 {
                    c += 1;
                }
                c as u8
            };
            assert_eq!(CTZ8[b as usize], want, "byte {b:#04X}");
        }
    }

    #[test]
    fn rev8_matches_a_naive_reversal() {
        for b in 0u32..256 {
            assert_eq!(REV8[b as usize], naive_rev(b, 8) as u8, "byte {b:#04X}");
        }
    }

    #[test]
    fn brujn_index_is_a_permutation_of_the_counts() {
        // Two claims at once: the fold produced exactly the 32 answers, and no
        // index is shared (an unmasked lookup into a collided table would
        // silently answer a wrong run length).
        let mut seen = [false; 32];
        for b in 0..32u32 {
            let idx = ((1u32 << b).wrapping_mul(BRUIJN_K)) >> 27;
            assert!(!seen[idx as usize], "index {idx} collides");
            seen[idx as usize] = true;
            assert_eq!(BRUIJN_INDEX[idx as usize], b as u8, "count {b}");
        }
        assert!(seen.iter().all(|s| *s), "the table is not total");
    }

    #[test]
    fn the_three_tables_cost_exactly_the_544_bytes_the_plan_budgeted() {
        // The E10-on-target plan budgeted 544 bytes of tables for this hybrid,
        // and PR 3's row in the split-ladder table names that number as its
        // cost line (FLAC.md, item 5). This is that number's compile-time
        // reading: `size_of_val` on the three items, so widening an entry type
        // — the easy way to grow a table by accident — fails here rather than
        // in a size diff weeks later.
        //
        // What it is NOT: a ROM reading. Nothing links this kernel yet, so no
        // image pays 544 B today, and the `nm`-on-the-Built-ELF placement
        // witness the `iwram-pin` story normally cites (`LEADING_ONES` at
        // `0x0300022c`) has no subject here. That reading belongs to the PR
        // that wires the twin; the record says so rather than implying it ran.
        let ctz = core::mem::size_of_val(&CTZ8);
        let rev = core::mem::size_of_val(&REV8);
        let bruijn = core::mem::size_of_val(&BRUIJN_INDEX);
        assert_eq!((ctz, rev, bruijn), (256, 256, 32), "table sizes changed");
        assert_eq!(
            ctz + rev + bruijn,
            544,
            "the hybrid's table budget is 544 B — against the 32 KB IWRAM budget \
             E10 ratified item 2 speaks to"
        );
    }

    #[test]
    fn ctz32_matches_a_naive_bit_walk() {
        for w in 0u32..=65_536 {
            assert_eq!(ctz32(w), naive_ctz32(w), "w {w:#010X}");
        }
        for b in 0..32u32 {
            for delta in [0u32, 1, u32::MAX] {
                let w = (1u32 << b).wrapping_add(delta);
                assert_eq!(ctz32(w), naive_ctz32(w), "w {w:#010X}");
            }
        }
        assert_eq!(ctz32(u32::MAX), 0);
        assert_eq!(ctz32(0xFFFF_FFE0), 5);
        assert_eq!(ctz32(0x8000_0000), 31);
        // The sentinel: `w == 0` is a state, never an answer.
        assert_eq!(ctz32(0), 32);
    }

    #[test]
    fn rev4_continues_the_window_the_way_rev32_does() {
        // `rev4` is `REV8`'s use as a 32-bit window load: four lookups plus the
        // shifts the byte load already pays for. The claim is a *continuation*
        // of the window, not `u32::reverse_bits` of the word the word-skip
        // assembles — those differ by a byte swap, and E10's record says this
        // exact distinction was misread once ("within-byte reversal alone" is
        // not a bit mapping). Witnessed three ways at once, because the whole
        // tier-3 leg's correctness rests on this one function.
        let mut x = 0x2E91_F7C3u32;
        for _ in 0..4096 {
            // The four bytes in stream order: `b[0]` earliest.
            let b = [
                x & 0xFF,
                (x >> 8) & 0xFF,
                (x >> 16) & 0xFF,
                (x >> 24) & 0xFF,
            ];
            // What the word-skip assembles: stream order in the low byte.
            let stream_order = b[0] | (b[1] << 8) | (b[2] << 16) | (b[3] << 24);
            // What the reader's own `acc` holds for the same four bytes — the
            // first stream bit in the high bit of the high byte, which is what
            // [`rev32`] is defined against in `bits::kernel_window`. Not the
            // same word; that difference is the whole point of the test.
            let reader_order = (b[0] << 24) | (b[1] << 16) | (b[2] << 8) | b[3];
            // (1) the two mechanisms produce the same *window*, which is the
            //     property the word-skip's cold load is relying on;
            assert_eq!(rev4(stream_order), rev32(reader_order), "x {x:#010X}");
            // (2) the byte-level statement the window contract is made of;
            assert_eq!(
                rev4(stream_order),
                u32::from(REV8[b[0] as usize])
                    | (u32::from(REV8[b[1] as usize]) << 8)
                    | (u32::from(REV8[b[2] as usize]) << 16)
                    | (u32::from(REV8[b[3] as usize]) << 24),
                "x {x:#010X}: per-byte continuation"
            );
            // (3) and the bit mapping itself: window bit `i` is stream bit `i`,
            //     i.e. the MSB-first bit of byte `i/8`.
            for i in 0..32u32 {
                let want = (b[(i / 8) as usize] >> (7 - (i % 8))) & 1;
                assert_eq!(
                    (rev4(stream_order) >> i) & 1,
                    want,
                    "x {x:#010X}: window bit {i} is not stream bit {i}"
                );
            }
            x = x.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        }
        // The per-byte tail is the same function at width 8: a one-byte
        // stream-order word reverses to `REV8`, which is what that leg stores.
        for byt in 0..=255u32 {
            assert_eq!(rev4(byt), u32::from(REV8[byt as usize]), "byte {byt}");
        }
        // And the two fixed points, which is as close as this gets to saying
        // what the function is *not*: a palindrome-in-bytes word is its own
        // continuation only when the bytes themselves are palindromic.
        assert_eq!(rev4(0), 0);
        assert_eq!(rev4(u32::MAX), u32::MAX);
    }

    #[test]
    fn remainder_from_window_matches_the_spec_text_at_every_order() {
        // The spec: the remainder is `order` bits, most-significant first. The
        // window holds those same bits reversed. So the answer is the naive
        // reversal of the extract — and the two legs (≤ 16 via `REV8`,
        // 17..=31 via `rev32`) must both get there.
        for order in 0..=31u32 {
            let cases: Vec<u32> = match order {
                0 => vec![0, 1, 0xFF, u32::MAX], // masked to nothing anyway
                _ if order <= 8 => (0..=255u32).take(1 << order).collect(),
                _ => {
                    let mut v = vec![0, 1, (1 << (order - 1)), (1 << order) - 1];
                    let mut x = 0x9E37_7F9Fu32;
                    for _ in 0..256 {
                        v.push(x & (((1u32 << order) - 1) | 1));
                        x = x.wrapping_mul(0x85EB_CA6B).wrapping_add(7);
                    }
                    v
                }
            };
            for extract in cases {
                let extract = extract & ((1u32 << order) - 1);
                let want = naive_rev(extract, order);
                assert_eq!(
                    remainder_from_window(extract, order),
                    want,
                    "order {order} extract {extract:#010X}"
                );
            }
        }
        // The `p = 0` mask leg, spelled out: no bits, so no value, whatever
        // `order == 0` would have shifted or indexed.
        assert_eq!(remainder_from_window(0, 0), 0);
        assert_eq!(remainder_from_window(u32::MAX, 0), 0);
    }

    #[test]
    fn every_codeword_length_and_offset_matches_the_spec_order_oracle() {
        // The tiers are chosen by *run length*, so a sweep of run lengths is the
        // thing that walks them: 0..=7 tier 1, 8..=31 tier 2 (including 31, the
        // shift-by-32 leg), 32..=400 tier 3 (the word-skip, its 4-byte step, and
        // its per-byte tail). Off-bit-offset because the bridge's clamps are
        // only inert when the starting window is not always full.
        for order in [0u32, 1, 2, 4, 8, 9, 15, 16, 17, 24, 30] {
            for off in 0..8usize {
                let mut p = Packer::new();
                p.push(off as u32, 0);
                let lengths: Vec<u32> = (0..=64)
                    .chain([70, 100, 127, 128, 129, 255, 256, 400])
                    .collect();
                for &q in &lengths {
                    let rem = if order == 0 {
                        0
                    } else {
                        // a recognizable remainder, high bit set where it fits
                        ((1u32 << (order - 1)) | 0x1555) & ((1u32 << order) - 1)
                    };
                    p.codeword(q, order, rem);
                }
                let bytes = p.finish();
                let count = lengths.len();
                let mut o = Oracle {
                    r: BitReader::new(&bytes),
                };
                if off > 0 {
                    o.r.read_bits(off as u32).unwrap();
                }
                let (want, want_err, _want_pos) = o.partition(order, count);
                compare_three_way(
                    &format!("order {order}, off {off}, lengths {lengths:?}"),
                    &bytes,
                    off,
                    order,
                    count,
                    &want,
                    want_err,
                );
            }
        }
    }

    #[test]
    fn the_q_31_and_empty_window_legs_answer_the_way_the_kernel_crate_pinned() {
        // Two boundary states the kernel crate's committed tables pin, read
        // here in the core's domain. `q = 31` is the only unary value that ends
        // a 32-bit window exactly full, and the empty window is what the
        // word-skip's cold load hands back; `ss_vectors.rs`'s q=31 rows and
        // `rm_vectors.rs`'s zero-window rows exist to convict a merge-or-reverse
        // reading of *its* kernel. What they do **not** pin is what a core
        // kernel does with those states, and the difference matters:
        //
        // * `q = 31` is a legal quotient. It is rejected only where the *fold*
        //   of it is: `q > u32::MAX >> order` first trips at `order = 28`, so
        //   at every order the probe drove (≤ 14) it is an ordinary sample.
        // * An empty window is not a value. A sample of `0` is
        //   `rice_unmap(0)`, so a kernel that answered with its window would
        //   publish `0`; the core's contract is to refill and keep walking, or
        //   to report a truncated run.
        for order in [0u32, 1, 2, 4] {
            for off in 0..8usize {
                // `q = 31` with a real remainder, followed by a second codeword:
                // the first drains the window to exactly empty, so the second is
                // answered from a cold window.
                let mut p = Packer::new();
                p.push(off as u32, 0);
                p.codeword(31, order, (1 << order) - 1);
                p.codeword(0, order, 0);
                let bytes = p.finish();
                let mut o = Oracle {
                    r: BitReader::new(&bytes),
                };
                if off > 0 {
                    o.r.read_bits(off as u32).unwrap();
                }
                let (want, want_err, want_pos) = o.partition(order, 2);
                compare_three_way(
                    &format!("q = 31 order {order} off {off}, then a cold-window codeword"),
                    &bytes,
                    off,
                    order,
                    2,
                    &want,
                    want_err,
                );
                assert_eq!(
                    want_err, None,
                    "q = 31 is a legal quotient at order {order}"
                );
                assert_eq!(want.len(), 2, "both codewords are answered");
                assert_eq!(
                    want_pos,
                    off + 33 + 2 * order as usize,
                    "32 + order for the first codeword, 1 + order for the second"
                );

                // The cold-window hand-back: `w` is `0` with `v = 0` after the
                // first codeword, and the second still gets its real value (not
                // the window's own `0`, which would unmap to `0` too).
                assert_eq!(want[1], 0, "q = 0, rem = 0 is the sample 0");
                assert_ne!(
                    want[0], 0,
                    "and q = 31 at order {order} is not the sample 0"
                );
            }
        }
        // The one case `ss_vectors.rs`'s q=31 rows look like they describe: the
        // window is exactly full, `q = 31`, and the remainder is empty — which
        // here is *not* a rejection: 31 folds to 31 at `order = 0` and unmaps to
        // -16. A kernel that rejected it would be reading a probe row into the
        // core's contract.
        let mut p = Packer::new();
        p.codeword(31, 0, 0);
        let bytes = p.finish();
        let mut o = Oracle {
            r: BitReader::new(&bytes),
        };
        let (want, want_err, _) = o.partition(0, 1);
        assert_eq!(want, vec![-16], "rice_unmap(31)");
        compare_three_way(
            "q = 31 at order 0 is a sample, not a rejection",
            &bytes,
            0,
            0,
            1,
            &want,
            want_err,
        );

        // And the orders where the value limit does reject it. The bound is on
        // the *fold*, not the run: `q > u32::MAX >> order` first trips for
        // `q = 31` at `order = 28`, and `order = 27` sits exactly on the
        // boundary (`MAX >> 27 == 31`, and the test is a strict `>`). That is
        // also why the probe never saw this leg: its grid stops at `p = 14`.
        for (order, expect) in [
            (7u32, None),
            (27, None),
            (28, Some(Error::InvalidField)),
            (30, Some(Error::InvalidField)),
        ] {
            let mut p = Packer::new();
            p.codeword(31, order, 0);
            let bytes = p.finish();
            let mut o = Oracle {
                r: BitReader::new(&bytes),
            };
            let (want, want_err, _) = o.partition(order, 1);
            assert_eq!(
                want_err, expect,
                "the spec-order walk must answer the pre-shift bound at order {order}"
            );
            compare_three_way(
                &format!("q = 31 at order {order} (the value-limit leg)"),
                &bytes,
                0,
                order,
                1,
                &want,
                want_err,
            );
        }
        // A run of 32 zeros with no bit after it: the empty window in the middle
        // of a run, which is a *truncated* codeword and must report it as one,
        // with the cursor back at the codeword's start.
        let bytes = vec![0u8; 4];
        let mut o = Oracle {
            r: BitReader::new(&bytes),
        };
        let (want, want_err, want_pos) = o.partition(0, 1);
        assert_eq!(
            want_err,
            Some(Error::EndOfStream),
            "a run with no terminator"
        );
        assert!(want.is_empty());
        assert_eq!(want_pos, 0, "and the cursor is back at the codeword start");
        compare_three_way(
            "32 zero bits and nothing after them",
            &bytes,
            0,
            0,
            1,
            &want,
            want_err,
        );
    }

    #[test]
    fn long_zero_runs_use_the_word_skip_and_its_per_byte_tail() {
        // Tier 3 in both shapes: 4-byte-aligned words, and a run that ends in a
        // partial word (1..3 bytes) — the leg E10's record calls load-bearing.
        for tail in 0..4usize {
            for run in [32u32, 33, 63, 64, 65, 100, 130] {
                let mut p = Packer::new();
                p.push(tail as u32, 0);
                p.codeword(run, 3, 0b101);
                p.codeword(1, 3, 0b011); // keep the walk going across the word
                let bytes = p.finish();
                let mut o = Oracle {
                    r: BitReader::new(&bytes),
                };
                if tail > 0 {
                    o.r.read_bits(tail as u32).unwrap();
                }
                let (want, want_err, want_pos) = o.partition(3, 2);
                let (got, got_err, got_pos) = twin(&bytes, tail, 3, 2);
                assert_eq!(got, want, "run {run} tail {tail}");
                assert_eq!(got_err, want_err, "run {run} tail {tail}");
                assert_eq!(got_pos, want_pos, "run {run} tail {tail}");
            }
        }
    }

    #[test]
    fn truncated_and_guard_legs_match_the_incumbent_exactly() {
        // Three boundary behaviours the trait's contract puts on the kernel,
        // each compared against the incumbent's cursor *and* error, because the
        // reader's atomicity rules are per-read and the twin owns the loop.
        //
        // (a) a truncated unary run — nothing consumed;
        // (b) a truncated remainder — the cursor stays past the quotient;
        // (c) the §9.2.7.3 rejections — the cursor stays after the codeword.
        //
        // All three are run at every bit offset, so the reader's byte/valid
        // split and the twin's byte/window split are exercised at once.
        for off in 0..8usize {
            for order in [0u32, 1, 5, 16, 17, 30] {
                // (a) a run that reaches the end of the region with no
                //     terminator, with the region ending mid-byte at each width.
                for keep in 0..=3usize {
                    let mut p = Packer::new();
                    p.push(off as u32, 0);
                    for _ in 0..40 {
                        p.push(1, 0);
                    }
                    let mut bytes = p.finish();
                    bytes.truncate(bytes.len().saturating_sub(keep));
                    if bytes.is_empty() {
                        continue;
                    }
                    let mut o = Oracle {
                        r: BitReader::new(&bytes),
                    };
                    if off > 0 {
                        o.r.read_bits(off as u32).unwrap();
                    }
                    let (want, want_err, want_pos) = o.partition(order, 1);
                    assert_eq!(want_err, Some(Error::EndOfStream), "(a) spec answer");
                    assert!(want.is_empty(), "(a) no sample is published");
                    assert_eq!(
                        want_pos, off,
                        "(a) the cursor goes back to the codeword's start"
                    );
                    compare_three_way(
                        &format!("(a) order {order} off {off} keep {keep}"),
                        &bytes,
                        off,
                        order,
                        1,
                        &want,
                        want_err,
                    );
                }
                // (b) a codeword whose remainder is cut off by the end: the
                //     cursor must stay exactly past the consumed quotient,
                //     which is `read_bits`' rule and not the twin's to invent.
                for extra in 0..order {
                    let mut p = Packer::new();
                    p.push(off as u32, 0);
                    for _ in 0..9 {
                        p.push(1, 0);
                    }
                    p.push(1, 1); // terminator
                    if extra > 0 {
                        p.push(extra, 0b1010_1010);
                    }
                    let mut bytes = p.finish();
                    // Drop whole bytes from the end so the remainder is short.
                    let drop = ((order - extra) as usize + 7) / 8;
                    if drop >= 1 && bytes.len() > drop {
                        bytes.truncate(bytes.len() - drop);
                    }
                    let mut o = Oracle {
                        r: BitReader::new(&bytes),
                    };
                    if off > 0 {
                        o.r.read_bits(off as u32).unwrap();
                    }
                    let (want, want_err, want_pos) = o.partition(order, 1);
                    // Which read ran out is decided by the fixture, so the
                    // expected cursor is derived from it rather than assumed:
                    // the quotient is 10 bits, so cutting it short truncates the
                    // *run* (case (a)'s rule — back to the start), while any
                    // region that holds the terminator leaves the cursor past the
                    // consumed quotient (case (b)'s rule).
                    let avail = bytes.len() * 8 - off;
                    let (want_pos_want, rule) = if avail < 10 {
                        (
                            off,
                            "the terminator was cut too, so this is a truncated run",
                        )
                    } else {
                        (
                            off + 10,
                            "the cursor stays exactly past the consumed quotient",
                        )
                    };
                    assert_eq!(want_err, Some(Error::EndOfStream), "(b) spec answer");
                    assert!(want.is_empty(), "(b) no sample is published");
                    assert_eq!(want_pos, want_pos_want, "(b) {rule}");
                    compare_three_way(
                        &format!("(b) order {order} off {off} extra {extra}"),
                        &bytes,
                        off,
                        order,
                        1,
                        &want,
                        want_err,
                    );
                }
                // (c) the §9.2.7.3 value-limit rejections, reached honestly.
                //     Both arms need a quotient at the boundary, so the sweep is
                //     over the orders where a constructible unary run can *be*
                //     at that boundary: `q = MAX >> order` needs 2^(32-order)−1
                //     zeros, which is 4095 at order 20, 1023 at 24 and 3 at 30.
                //     At order 4 and below the boundary is 2^27 zeros or more,
                //     i.e. no stream anyone decodes, and the arms are pinned
                //     instead by `residual`'s own unit tests, which call the
                //     guard arithmetic directly.
                if order >= 20 {
                    let top = u32::MAX >> order;
                    let full = (1u32 << order) - 1;
                    for (q, rem, expect) in [
                        (top, full, Some(Error::InvalidField)),  // folded == u32::MAX
                        (top + 1, 0, Some(Error::InvalidField)), // out of 32 bits
                        (top, full - 1, None),                   // one below: the same bits, valid
                    ] {
                        let mut p = Packer::new();
                        p.push(off as u32, 0);
                        for _ in 0..q {
                            p.push(1, 0);
                        }
                        p.push(1, 1);
                        p.push(order, rem);
                        let bytes = p.finish();
                        let mut o = Oracle {
                            r: BitReader::new(&bytes),
                        };
                        if off > 0 {
                            o.r.read_bits(off as u32).unwrap();
                        }
                        let (want, want_err, want_pos) = o.partition(order, 1);
                        assert_eq!(
                            want_err, expect,
                            "(c) the spec-order walk must answer the boundary at order {order} q {q}"
                        );
                        // The cursor after a rejection is *after the codeword*:
                        // the codeword was consumed before the fold was tested,
                        // which is the position the incumbent has it at.
                        assert_eq!(want_pos, off + q as usize + 1 + order as usize);
                        compare_three_way(
                            &format!("(c) order {order} q {q} rem {rem:#x}"),
                            &bytes,
                            off,
                            order,
                            1,
                            &want,
                            want_err,
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn an_out_of_range_order_behaves_as_the_incumbent_does() {
        // `order ≥ 32` is a caller-built-header case (a parsed stream cannot
        // reach it); the cold arm delegates, so the behaviour is identical by
        // construction — witnessed rather than trusted, including the cursor.
        for order in [32u32, 33, 255] {
            let mut p = Packer::new();
            p.push(3, 0);
            for _ in 0..5 {
                p.push(1, 0);
            }
            p.push(1, 1);
            p.push(32, 0xDEAD_BEEF);
            let bytes = p.finish();
            let (inc, inc_err, inc_pos) = incumbent(&bytes, 3, order, 2);
            let (got, got_err, got_pos) = twin(&bytes, 3, order, 2);
            assert_eq!(got, inc, "order {order}");
            assert_eq!(got_err, inc_err, "order {order}");
            assert_eq!(got_pos, inc_pos, "order {order}");
            assert_eq!(got_err, Some(Error::InvalidField));
        }
    }

    #[test]
    fn an_empty_partition_consumes_nothing_and_uses_no_window() {
        // The trait says `out.len()` is the partition's sample count. Zero
        // samples must leave the reader exactly where it was — including a
        // partially-consumed byte, where the hand-out/take-back clamps are the
        // only thing standing between a no-op and a resync that moves the
        // cursor.
        for off in 0..8usize {
            let bytes = [0b0101_0110, 0b1100_0001];
            let (inc, inc_err, inc_pos) = incumbent(&bytes, off, 4, 0);
            let (got, got_err, got_pos) = twin(&bytes, off, 4, 0);
            assert_eq!(got_pos, off);
            assert_eq!(got_err, None);
            assert!(got.is_empty());
            assert_eq!(got, inc);
            assert_eq!(got_err, inc_err);
            assert_eq!(got_pos, inc_pos);
        }
    }

    #[test]
    fn mixed_partitions_walk_the_same_way_as_the_hand_built_ones() {
        // Real-shaped partitions: short runs and long runs interleaved, a
        // partition ending exactly at a byte boundary, and one ending in the
        // middle of a codeword's remainder.
        for order in [0u32, 2, 4, 8, 13, 30] {
            let runs = [0u32, 1, 7, 8, 15, 16, 31, 32, 33, 40, 64, 71];
            for shift in 0..runs.len() {
                let mut p = Packer::new();
                p.push((shift * 3) as u32 % 8, 0);
                for (i, &q) in runs.iter().cycle().take(runs.len()).enumerate() {
                    let rem = if order == 0 {
                        0
                    } else {
                        (0xA5A5u32 >> i) & ((1u32 << order) - 1)
                    };
                    p.codeword(q, order, rem);
                }
                let bytes = p.finish();
                let off = ((shift * 3) as usize) % 8;
                let mut o = Oracle {
                    r: BitReader::new(&bytes),
                };
                if off > 0 {
                    o.r.read_bits(off as u32).unwrap();
                }
                let (want, want_err, _) = o.partition(order, runs.len());
                // At `order ≥ 26` this sweep deliberately runs *past* the
                // §9.2.7.3 boundary (the limit is on the fold, so a run of 7 is
                // legal at order 24 and rejected at 30), and the test's claim is
                // that both mechanisms stop exactly where the spec-order walk
                // does — same sample, same error, same cursor. The boundary
                // itself, and the cursor position after a rejection, are pinned
                // by `truncated_and_guard_legs_match_the_incumbent_exactly`.
                compare_three_way(
                    &format!("mixed order {order} shift {shift}"),
                    &bytes,
                    off,
                    order,
                    runs.len(),
                    &want,
                    want_err,
                );
            }
        }
    }
}
