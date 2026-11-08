//! The GBA's own Rice codeword kernel: the E10 ARM-mode partition loop,
//! landed through the core's [`RiceKernel`] seam (split-ladder item 6 PR
//! **6a-4**). Correctness only — this module adds no cycle claim, and the
//! default Rice kernel in every configuration stays
//! [`flac_lite::residual::TableCountKernel`]. Cycles are 6a-5's reading.
//!
//! ```text
//! tier 1  CTZ8[w & 0xFF]           a run of 0..=7 in one .iwram byte lookup
//! tier 2  C1 De Bruijn             a run of 8..=31: MUL + >> 27 + one LDRB,
//!                                  flat in q
//! tier 3  C2 word-skip             a run past the window: whole-word zero
//!                                  tests from the byte buffer, 32 bits a pop
//! refill  REV8                     bit-reversed byte merges into the window
//! stitch  REV8 again               the remainder is un-reversed (see below)
//! fold    fold_body.s              §9.2.7.2 unmap, the shared source
//! ```
//!
//! ## What this module is the landing *of*
//!
//! E10 wrote the whole partition sample loop in ARM mode and measured it:
//! **−349 / −348 cycles per sample**, the largest FIXED-side win in the
//! menu (perf-results.md "E10 result"). The kernel was never landed: it lived
//! in a sample patch, `examples/flac_spike/experiments/e10-three-arm-probe/`,
//! whose own docs called the missing step "stitching": the probe drove a
//! vendored copy of the bit reader, not the shipped seam. This module is that
//! stitch — the same `.s` body, called through
//! [`flac_lite::bits::BitReader::kernel_window`] /
//! [`flac_lite::bits::BitReader::set_kernel_window`], which is the bridge PR
//! 1 of item 5 built for exactly this call shape: hand out the window, own the
//! whole partition, hand the window back once.
//!
//! **The lift, and where its source actually is.** `asm/rice_partition.s` is
//! lifted from the E10 patch (the path above is inside the patch, not on main
//! — `plan-of-record` 6a-4 says "lifted from `examples/arm-kernels/`", and
//! for the *fold* that is exactly right but for this kernel it points at the
//! wrong tree; the asm file's header says so, and 6a-4's landing record
//! corrects the plan line). The kernel body is the patch's line-for-line
//! except four deltas, each marked `[6a-4 …]` in the asm and listed in its
//! header: a 12-byte frame instead of 8 (one more stack slot, for the
//! §9.2.7.3 bound), the two §9.2.7.3 guards now *tested* rather than wrapped,
//! and a third status value. The fold is not lifted at all: this unit pastes
//! `examples/arm-kernels/asm/fold_body.s`, the one committed source, exactly
//! as decision 6's sharing rule requires.
//!
//! ## The one thing the plan left the probe to find, restated because it is
//! ## still the part a reader gets wrong
//!
//! The window is **bit-reversed**: bit 0 of `w` is the next stream bit, and
//! bytes merge in reversed (`w |= REV8[byte] << v`). That is what makes the
//! unary run cheap — a run sits at the LSB end, where `CTZ8` and the De Bruijn
//! isolate read it directly — and it costs one thing: FLAC's Rice remainder is
//! MSB-first natural binary (§9.2.7.2), so the `p` bits extracted from the
//! window are the *bit-reversed* coded remainder. The kernel therefore
//! un-reverses it (`y = REV8[lo] << 8 | REV8[hi]`, then `>> 16 − p`). E10
//! measured that tax at ≈8 cycles a sample and reported it as the honest price
//! of the reversed-window shape; it is not a bug to be engineered away here,
//! and 6a-5's reading is where it gets priced again.
//!
//! ## The state-struct ABI, as this crate's own contract
//!
//! The kernel is a leaf function taking its mutable state by pointer. The
//! layout is [`KernelAbi`] — five `u32`, asserted by `offset_of!` const
//! asserts below and by the same numbers in the asm's header. Two views of
//! `ptr`/`end` exist and the bridge maps between them, which is the one piece
//! of the E10 probe's Rust that had to be written rather than lifted: the
//! kernel addresses bytes (it uses `ldrb [r6], #1` — ARMv4 has no
//! base-plus-register byte load with writeback, so it *must* hold an address),
//! while the core's [`KernelWindow`] is slice-relative by definition. The
//! mapping is a base pointer and a subtraction, and it is total:
//!
//! ```text
//!   to the kernel    ptr_abi = base + win.ptr      end_abi = base + win.end
//!   back to the core win.ptr  = abi.ptr  - base    win.end  = abi.end - base
//!                      base = region().as_ptr(), and `region()` is the only
//!                      bound the kernel's reads are allowed to have
//! ```
//!
//! `w`/`v` are the same words in both views, and the invariants the two
//! halves share are one invariant stated twice (bit `i` of `w` is stream bit
//! `P + i`, bits ≥ `v` of `w` are always zero, `ptr == (P + v) / 8` at
//! hand-out) — that is the seam bridge's contract, stated in
//! [`flac_lite::bits`] and not renegotiated here.
//!
//! ## Status, and the three things an exit may leave behind
//!
//! The fifth word is the exit kind ([`KernelStatus`]); the wrapper turns it
//! into a [`flac_lite::Error`] and *nothing else* the caller can see:
//!
//! * [`KernelStatus::Done`] — every sample written, the final window handed
//!   back.
//! * [`KernelStatus::EndOfStream`] — unterminated unary, or a remainder the
//!   window could not cover. The window is handed back **where the walk
//!   stopped**, which is the probe's ratified "restore nothing" rule and does
//!   **not** match the incumbent's per-read atomic restore. The error *kind*
//!   is the same; the cursor is not. On that path the frame is dead either
//!   way, and the difference is witnessed rather than argued
//!   (`tests/arm_from_outside.rs`, the cursor-divergence arm).
//! * [`KernelStatus::GuardRejected`] — a §9.2.7.3 bound fired
//!   (`quotient > MAX >> order`, or a coded number of `u32::MAX`, which is
//!   the one value whose folded sample is `i32::MIN`). The wrapper answers
//!   [`flac_lite::Error::InvalidField`] and hands the window back **as
//!   written**, because the incumbent is not atomic here either: it reads the
//!   whole codeword and *then* asks §9.2.7.3, leaving the reader past it. The
//!   kernel's two reject points are the same two points for the same reason —
//!   G1 after the terminating 1 and before the remainder, G2 after the
//!   remainder and before the store — so a rejected codeword leaves both
//!   mechanisms' cursors identical. That was measured, not assumed: the
//!   original plan here was to discard the state on the theory that the
//!   incumbent's rejection was atomic, and the first run of the guard arm said
//!   the incumbent had consumed 65,553 bits while this path had consumed none.
//!
//! ## Why this kernel owns its tables, and what witnesses that they did not
//! ## drift (Q1's first open half, answered here)
//!
//! [`CTZ8`]/[`REV8`]/[`BRUIJN_INDEX`] are the 544 B E10 was priced with. The
//! core has the same three bytes already — privately, inside
//! `flac_lite::rice_hybrid`, behind `feature = "rice-hybrid"`. Reusing the
//! core's would be the cheaper-looking choice and it is a trap: those tables
//! only *exist* when the core's second mechanism is compiled in, so an
//! overlay kernel that used them would force a second Rice mechanism into
//! every image that links this crate. Item 5 PR 3 measured what that costs on
//! the calibration image when the mechanism is present but *unreachable*
//! (−26.5 M cycles on the baseline walk, +2,984 B of other code re-inlined,
//! `.iwram` 812 → 1,324 B). 6a-4's constraint is that the default
//! configuration must not move at all, so the overlay pays the 544 B it was
//! always planned to pay and keeps the core's feature out of every build that
//! does not ask for it.
//!
//! What keeps the copies honest is not a claim:
//!
//! 1. the three `const fn` builders are lifted **verbatim** from the same
//!    source the kernel came from, so there is one recipe, not a transcription;
//! 2. a host test re-derives every entry of all three tables from a naive
//!    bit-walk oracle (`tests/arm_from_outside.rs`), which is also what the
//!    core's own tests do for its copies;
//! 3. each table's byte digest is committed as a constant in that test, so a
//!    byte that changes *anywhere* in a table trips the test and has to be
//!    explained in a record, not in a diff.
//!
//! ## `unsafe`, counted and named (the budget this crate's docs fixed for 6a-4)
//!
//! Six unsafe items, each carrying its own `#[allow(unsafe_code)]`, so the
//! allowance count and the item count are the same number and a reviewer can
//! take them apart with a grep. `#![deny(unsafe_code)]` is in force at the crate
//! root, so none of these compiles without its allowance, which is what makes
//! the count enforced rather than recorded. The arm in
//! `tests/arm_from_outside.rs` that counts them matches *attribute shape*, not
//! the word `unsafe`: prose about `unsafe` in this file is deliberately not a
//! countable thing.
//!
//! * `asm_unit`'s `global_asm!` — the allowance goes on the module that carries
//!   it (a macro-item attribute does not bind; the module's does, which is this
//!   repo's recorded pitfall);
//! * the `unsafe extern "C"` declaration of `arm_rice_partition` — its own
//!   allowance, on the item, because a declaration is a second unsafe item and
//!   folding it into the module's allowance would make the count lie;
//! * three `unsafe(link_section = ".iwram")]` placement attributes, one per
//!   table, each `#[allow(unsafe_code)]`ed on the item — the same
//!   ratified-exception shape the core uses for `LEADING_ONES`, and a
//!   placement attribute on an immutable compile-time table is not unsafe
//!   *code*. They are behind `cfg_attr(all(target_arch = "arm",
//!   feature = "iwram-pin"))`, so they exist only in a pinned build: **three
//!   unsafe items in a plain `rice-arm` build, six with `iwram-pin` on top, zero
//!   in a default build**.
//! * one `unsafe { … }` call site, [`run_asm`], whose allowance is on the item
//!   so the count stays greppable, and whose `// SAFETY:` block names every
//!   bound it relies on.
//!
//! A default build of this crate still contains none of them: `rice-arm` is
//! off, and `asm/rice_partition.s` is not assembled, not linked and not
//! reachable from anything.
//!
//! ## Domain, and the one input class this kernel does not run
//!
//! The remainder leg builds a 16-bit reversal, so it computes for `order ≤
//! 16` ([`ArmRiceKernel::MAX_ORDER`]). Wider orders are legal FLAC (the 5-bit
//! coding method reaches 30) and are delegated — [`TableCountKernel`] answers
//! them, and the delegation is witnessed against the incumbent at orders
//! 17..=31, 32 and 255. That is a *realization limit of this kernel*, not a
//! profile assumption dressed up as one: the profile's own orders stop at 14
//! (the committed corpus reaches 8), and the delegation exists so that a
//! profile fact never decides what a correct answer is. An alternative — a
//! second, wide remainder leg inside the asm, as the core's Rust hybrid did —
//! is 6a-5's/6b's to weigh against the code size and the extra per-partition
//! branch; it is named here so the next reader sees it was considered, not
//! missed. `order ≥ 32` never reaches the asm either way: the wrapper's
//! delegation covers it, which is the seam's own arithmetic guard.
//!
//! ## What is NOT here
//!
//! No default moved: `decode_rice_partition` still names the incumbent on one
//! line, and nothing in either crate's decode path calls this kernel. No
//! escape partition is decoded here — escapes ride the reader verbatim in the
//! core's driver and never reach a kernel. No timing of any kind: the numbers
//! quoted above are E10's and item 5's, taken elsewhere.

use flac_lite::Error;
use flac_lite::bits::{BitReader, KernelWindow};
use flac_lite::residual::{RiceKernel, TableCountKernel};

// ---------------------------------------------------------------------------
// The three tables (the plan's 544 B budget). Same recipe as the core's
// private copies and as the E10 probe's: `const fn` builders lifted verbatim
// from the patch, so there is one source of the bytes, not three
// transcriptions. Why they are duplicated rather than reused, and what
// witnesses the duplication, is in the module docs ("Why this kernel owns its
// tables").
// ---------------------------------------------------------------------------

/// 256-entry byte → trailing-zero count, `8` = "the run continues past this
/// byte" (the tier-1 handoff). Fed the **low** byte of the bit-reversed
/// window, whose bit 0 is the next stream bit, its answer is the unary
/// quotient whenever the terminating one sits inside those 8 bits.
//  The three placement attributes are 6a-4's counted exceptions (module docs,
//  "`unsafe`, counted and named"). Rides `iwram-pin` exactly like the core's
//  `LEADING_ONES` does: off by default, so a portable consumer of this crate
//  never inherits the GBA link-script contract, and a GBA instrument turns it
//  on to get the zero-wait read. Plain `.iwram`, never `.text_iwram` (E6's
//  pin target: the boot-time copy is the point; execution stays in ROM).
#[cfg_attr(
    all(target_arch = "arm", feature = "iwram-pin"),
    unsafe(link_section = ".iwram")
)]
#[allow(unsafe_code)] // A placement attribute on an immutable compile-time
// table is not unsafe code (edition-2024 lints it as an unsafe
// attribute; the core's LEADING_ONES carries the same note).
pub static CTZ8: [u8; 256] = build_ctz8();

/// 256-entry byte → bit-reversed byte (bit `i` → bit `7 − i`). Does double
/// duty: the refill merge `w |= REV8[byte] << v`, and the un-reversal of the
/// Rice remainder (module docs, "the stitch").
#[cfg_attr(
    all(target_arch = "arm", feature = "iwram-pin"),
    unsafe(link_section = ".iwram")
)]
#[allow(unsafe_code)]
pub static REV8: [u8; 256] = build_rev8();

/// 32-entry De Bruijn index: `BRUIJN_INDEX[((1 << b) * BRUIJN_K >> 27) & 31]
/// == b` for every `b < 32`. C1's last step.
#[cfg_attr(
    all(target_arch = "arm", feature = "iwram-pin"),
    unsafe(link_section = ".iwram")
)]
#[allow(unsafe_code)]
pub static BRUIJN_INDEX: [u8; 32] = build_bruijn();

/// The De Bruijn constant C1 multiplies by. Not a rotatable ARM immediate —
/// the kernel keeps it in a literal pool (`ldr r12, .Lk`), which is a
/// measured substrate fact, not a style choice.
const BRUIJN_K: u32 = 0x077CB531;

const fn build_ctz8() -> [u8; 256] {
    let mut table = [0u8; 256];
    let mut i = 0usize;
    while i < 256 {
        let b = i as u8;
        let mut count = 0u8;
        while count < 8 && (b >> count) & 1 == 0 {
            count += 1;
        }
        table[i] = count;
        i += 1;
    }
    table
}

const fn build_rev8() -> [u8; 256] {
    let mut table = [0u8; 256];
    let mut i = 0usize;
    while i < 256 {
        let b = i as u8;
        let mut r = 0u8;
        let mut j = 0usize;
        while j < 8 {
            if (b >> j) & 1 == 1 {
                r |= 1 << (7 - j);
            }
            j += 1;
        }
        table[i] = r;
        i += 1;
    }
    table
}

const fn build_bruijn() -> [u8; 32] {
    let mut table = [0u8; 32];
    let mut b = 0usize;
    while b < 32 {
        let idx = ((1u32 << b).wrapping_mul(BRUIJN_K) >> 27) as usize;
        table[idx] = b as u8;
        b += 1;
    }
    table
}

/// Runtime placement witness for an instrument ROM's boot log: the three
/// tables' own addresses, read at runtime. The offline witness is `nm` on the
/// built ELF. Nothing in this crate, and nothing in any kernel-check, may
/// hard-code a table address — the addresses are what the probe *reports*,
/// never what it assumes. Whether the tables land in `.iwram` at all is
/// reachability-dependent, which is precisely why this is a function and not a
/// constant.
#[must_use]
pub fn tables_iwram_addrs() -> (usize, usize, usize) {
    (
        CTZ8.as_ptr() as usize,
        REV8.as_ptr() as usize,
        BRUIJN_INDEX.as_ptr() as usize,
    )
}

// ---------------------------------------------------------------------------
// The assembled ARM unit (target only: a host assembler would not accept an
// ARM-mode body, and there is no ARM executor here to run it on).
// ---------------------------------------------------------------------------

/// The single assembled unit: the committed fold body, then the partition
/// kernel. `fold_body.s` is *not* vendored here — it is included from the one
/// committed source, shared with the kernel track by textual paste (decision
/// 6: "shared by textual include, not duplicated by hand"; rustc hands the
/// assembler a temp file with no reliable search path, so the paste belongs in
/// the Rust source that ships it). Paste order puts the macro definition
/// first, which is what lets the kernel invoke `fold_body`.
///
/// `#[allow(unsafe_code)]` sits on the module, not the macro item: an
/// edition-2024 `global_asm!` is an unsafe op, and a macro-item attribute is
/// ignored — the module attribute is what binds (this repo's recorded
/// pitfall, `examples/arm-kernels/src/asm.rs`).
#[cfg(target_arch = "arm")]
#[allow(unsafe_code)]
mod asm_unit {
    core::arch::global_asm!(concat!(
        include_str!("../../../examples/arm-kernels/asm/fold_body.s"),
        "\n",
        include_str!("../asm/rice_partition.s"),
    ));
}

/// The kernel's mutable state: five `u32`, in the order and at the offsets
/// `asm/rice_partition.s` loads and stores. Public because the ABI is a
/// contract an instrument may assert, not an implementation detail.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct KernelAbi {
    /// Bit-reversed window: bit 0 is the next stream bit; bits ≥ `v` are 0.
    pub w: u32,
    /// Valid bits in `w` (0..=32).
    pub v: u32,
    /// Next byte the kernel may read, as an **absolute byte address inside
    /// the region** `region()` handed out — see the module docs on the two
    /// views and the map between them.
    pub ptr: u32,
    /// End of the region the kernel may read, same view as `ptr`.
    pub end: u32,
    /// Exit kind ([`KernelStatus`]'s discriminant). Written on every exit.
    pub status: u32,
}

// The ABI is asserted, not documented-and-hoped. These are compile-time: a
// struct that drifts from the asm's offsets does not build.
const _: () = assert!(core::mem::size_of::<KernelAbi>() == 5 * 4);
const _: () = assert!(core::mem::offset_of!(KernelAbi, w) == 0);
const _: () = assert!(core::mem::offset_of!(KernelAbi, v) == 4);
const _: () = assert!(core::mem::offset_of!(KernelAbi, ptr) == 8);
const _: () = assert!(core::mem::offset_of!(KernelAbi, end) == 12);
const _: () = assert!(core::mem::offset_of!(KernelAbi, status) == 16);

/// What an exit of the kernel means. The wrapper maps these onto the core's
/// `Error` and onto the reader's cursor; nothing else distinguishes them for
/// a caller of the seam.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KernelStatus {
    /// Every sample was written.
    Done,
    /// The unary run, or the remainder it was followed by, reached the end of
    /// the region. The window says where the walk stopped.
    EndOfStream,
    /// A §9.2.7.3 value bound fired. No sample was written, and the window
    /// the kernel left is discarded by the wrapper.
    GuardRejected,
}

impl KernelStatus {
    /// Wire value of `Done` (the state struct's fifth word).
    pub const CODE_DONE: u32 = 0;
    /// Wire value of `EndOfStream`.
    pub const CODE_END_OF_STREAM: u32 = 1;
    /// Wire value of `GuardRejected` (6a-4's addition; the E10 probe's kernel
    /// had only the first two).
    pub const CODE_GUARD_REJECTED: u32 = 2;

    #[must_use]
    pub const fn code(self) -> u32 {
        match self {
            Self::Done => Self::CODE_DONE,
            Self::EndOfStream => Self::CODE_END_OF_STREAM,
            Self::GuardRejected => Self::CODE_GUARD_REJECTED,
        }
    }

    /// Read the fifth word back. A value outside the three codes means the
    /// state memory was corrupted (or a kernel that is not this one wrote
    /// it), and is treated as a rejection so the answer is loud rather than a
    /// window nobody asked for.
    #[must_use]
    pub const fn from_code(code: u32) -> Self {
        match code {
            Self::CODE_DONE => Self::Done,
            Self::CODE_END_OF_STREAM => Self::EndOfStream,
            _ => Self::GuardRejected,
        }
    }
}

/// The GBA's own Rice kernel: the E10 ARM-mode partition loop behind the
/// core's seam.
///
/// It is a unit struct: all of its state is the window the seam hands it, and
/// the tables this crate owns. Nothing about it is configurable, because a
/// second configurable thing would be a second mechanism to charge for.
/// The largest Rice order this kernel's remainder leg computes, as a free item
/// so a witness can name it without going through the type (and so the domain
/// rule and the model's own `debug_assert` read from one place).
///
/// The limit is a property of the arithmetic, not a policy: the remainder leg
/// builds a 16-bit reversal of the window-domain bits and shifts it by `16 - p`,
/// so above this order the reversal has no bits to hold and the extracted
/// remainder stops being the coded number. The probe's own profile stopped at 14
/// and could have broken here at 15 or 16 without anything in its corpus saying
/// so (its parameters were `{0, 2, 3, 6, 8}`).
pub const MAX_ORDER: u32 = 16;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ArmRiceKernel;

impl ArmRiceKernel {
    /// See [`MAX_ORDER`]. Kept as an associated constant too because the
    /// delegation and the model read it as `Self::MAX_ORDER` / `ArmRiceKernel::MAX_ORDER`,
    /// and the two must not be able to drift.
    pub const MAX_ORDER: u32 = crate::rice_arm::MAX_ORDER;
}

impl RiceKernel for ArmRiceKernel {
    fn decode_partition(
        reader: &mut BitReader<'_>,
        order: u32,
        out: &mut [i32],
    ) -> flac_lite::Result<()> {
        // Wide orders: outside this realization's arithmetic. Forwarded, not
        // refused — `RiceKernel` has no "decline" verb, and the trait's rule
        // is that a kernel may not answer a different sample. `order ≥ 32`
        // lands here too, and `TableCountKernel` answers it with
        // `InvalidField`, which is what the core's own arithmetic guard says.
        if order > Self::MAX_ORDER {
            return TableCountKernel::decode_partition(reader, order, out);
        }
        // The kernel computes `out_end = out + 4n` with a `lsl #2`. The seam's
        // own contract makes that exact: `decode_rice_partition_with` checks
        // `out.len() == usize::from(header.sample_count)` and `sample_count`
        // is a `u16`, so `4n` cannot wrap. Asserted, not assumed, because
        // `RiceKernel::decode_partition` is public and a caller outside the
        // core may call it directly.
        debug_assert!(
            out.len() <= usize::from(u16::MAX),
            "arm_rice_partition computes out_end as out + 4n with lsl #2"
        );

        // `region()` is the only bound the kernel's reads may have. It borrows
        // the reader's *input*, not the `&self` borrow, so it stays readable
        // while the window is being walked; `set_kernel_window` re-bounds
        // against the reader, which is the authority.
        let region = reader.region();
        let window = reader.kernel_window();
        let (after, status) = run(window, region, out, order);
        match status {
            KernelStatus::Done => {
                reader.set_kernel_window(after);
                Ok(())
            }
            // The cursor rule this path carries is the probe's ratified
            // "restore nothing", not the incumbent's per-read restore. The
            // error kind is identical; the cursor is not; the frame is dead
            // either way. Witnessed in tests/arm_from_outside.rs.
            KernelStatus::EndOfStream => {
                reader.set_kernel_window(after);
                Err(Error::EndOfStream)
            }
            // Hand `after` back rather than discarding it. The incumbent is not
            // atomic across a guard-rejected sample: it reads the whole run and
            // *then* asks §9.2.7.3, so its reader sits where the codeword ended.
            // This kernel's two reject points are the same two points — G1 after
            // the terminating 1 and before the remainder, G2 after the remainder
            // and before the store — so handing the state back makes the two
            // paths agree on the cursor as well as on the kind. Discarding here
            // would invent a third behavior no caller of this seam has ever been
            // given. (Written after the first run of the guard arm, which caught
            // the opposite assumption: the incumbent had consumed 65,553 bits of
            // unary run and this path had consumed none.)
            KernelStatus::GuardRejected => {
                reader.set_kernel_window(after);
                Err(Error::InvalidField)
            }
        }
    }
}

/// The host/target split: on `thumbv4t-none-eabi` this calls the assembled
/// kernel; on a host it runs [`model_partition`], because there is no ARM
/// executor in this repository. Same name, same signature, one of the two
/// bodies compiled — which is the shape 6a-3's docs already flagged as the
/// cost of a cross-arch realization ("two halves that cannot be tested
/// together"), and the reason the ROM-side witness exists.
#[cfg(target_arch = "arm")]
fn run(
    window: KernelWindow,
    region: &[u8],
    out: &mut [i32],
    order: u32,
) -> (KernelWindow, KernelStatus) {
    run_asm(&window, region, out, order)
}

#[cfg(not(target_arch = "arm"))]
fn run(
    window: KernelWindow,
    region: &[u8],
    out: &mut [i32],
    order: u32,
) -> (KernelWindow, KernelStatus) {
    model_partition(&window, region, out, order)
}

// The kernel's own entry point, target only. Declared here rather than inside
// `asm_unit` so that the symbol's type sits next to the wrapper that calls it;
// it is the second of the two items the assembly needs, and it carries its own
// allowance for the same reason the `global_asm!` does — see the `unsafe`
// budget at the top of this module.
#[cfg(target_arch = "arm")]
#[allow(unsafe_code)]
unsafe extern "C" {
    fn arm_rice_partition(
        state: *mut u32,
        out: *mut i32,
        n: u32,
        p: u32,
        ctz8: *const u8,
        rev8: *const u8,
        bruijn: *const u8,
    );
}

/// Map the core's window into the kernel's, call the assembled body, map the
/// window back. Target only.
#[cfg(target_arch = "arm")]
#[allow(unsafe_code)] // The one call site in this crate. Its `// SAFETY:` block
// below is the contract; the allowance is on the
// item rather than the block so the count of
// allowances stays grep-checkable (see the module
// header's budget).
fn run_asm(
    window: &KernelWindow,
    region: &[u8],
    out: &mut [i32],
    order: u32,
) -> (KernelWindow, KernelStatus) {
    // Both views agree on `base`: it is the first byte of the region the
    // reader handed out, and the kernel is only ever allowed to read inside
    // that region. An empty region makes `as_ptr()` dangling-but-aligned, and
    // every offset in the map below is then zero — the kernel reads nothing,
    // because its first bound test compares `ptr` against `end`.
    let base = region.as_ptr() as usize;
    let mut abi = KernelAbi {
        w: window.w,
        v: window.v,
        ptr: (base + window.ptr) as u32,
        end: (base + window.end) as u32,
        status: u32::MAX, // overwritten on every exit; MAX makes a kernel
                          // that wrote no status impossible to mistake for `Done`
    };

    // SAFETY: the four things the kernel is trusted with, each bounded by
    // something the caller can see.
    //
    // * `state` is written through as exactly five `u32` at the offsets
    //   asserted by the `offset_of!` asserts above — the only mutable object
    //   the kernel touches. It is a local, and `abi` outlives the call.
    // * `out` receives at most `n = out.len()` i32 writes, and the kernel's
    //   own loop bound is `out_end = out + 4n` (`n ≤ u16::MAX` is asserted at
    //   the wrapper, so `4n` does not wrap). On a status-1 or status-2 exit
    //   fewer are written; the wrapper discards those samples, and `out`'s
    //   remaining entries are left as the caller set them.
    //
    // `end` is the one state word the kernel reads and does not write: it is
    // copied into `r7` at entry and left there, so `abi.end` below is still the
    // value this function set. That is why the map back can subtract `base`
    // from it. (The first landed cut of the asm was wrong about the *stack*
    // slots the three table pointers arrive in — 56/60/64 instead of 48/52/56 —
    // and the instrument ROM convicted it on its first end-of-stream arm; the
    // host model could not, because it takes those pointers as Rust arguments.
    // The offsets are pinned by `tests/arm_from_outside.rs` and by this file's
    // `KernelAbi` asserts.)
    // * reads of the stream run from `base + window.ptr` to
    //   `base + window.end`, i.e. `region[window.ptr..window.end]`: every
    //   load is preceded by a bound test against `end`, including the
    //   word-skip's four-byte peek, which is guarded by `add r12, r6, #4;
    //   cmp r12, r7; bhi .Lwslow` and falls back to a per-byte walk.
    // * the three tables are read by byte index only: `CTZ8[w & 0xFF]` and
    //   `REV8[byte]` are ≤ 255 by construction, `REV8`'s second index in the
    //   remainder leg is `(w & ((1<<p)-1)) >> 8` ≤ 0xFF only for `p ≤ 16`
    //   (`MAX_ORDER`, enforced by the wrapper's delegation), and
    //   `BRUIJN_INDEX[((lowbit * K) >> 27) & 31]` is masked to 31.
    //
    // The shift-leg arithmetic (`lsl`/`lsr` by a register, ≥ 32 giving 0) and
    // the condition-code chains across the fold are pinned by the asm's own
    // comments and by `objdump -d` of the built ELF, not by this block.
    unsafe {
        arm_rice_partition(
            core::ptr::addr_of_mut!(abi).cast::<u32>(),
            out.as_mut_ptr(),
            out.len() as u32,
            order,
            CTZ8.as_ptr(),
            REV8.as_ptr(),
            BRUIJN_INDEX.as_ptr(),
        );
    }

    let status = KernelStatus::from_code(abi.status);
    let after = KernelWindow {
        w: abi.w,
        v: abi.v,
        // Subtracting `base` is exact: the kernel only ever moves `ptr`
        // forward inside the region, and it never writes `end` (it copies it
        // through). `set_kernel_window` re-bounds against the reader anyway,
        // so a wrong map here would be caught as a clamped window rather than
        // silently trusted.
        ptr: (abi.ptr as usize).wrapping_sub(base),
        end: (abi.end as usize).wrapping_sub(base),
    };
    (after, status)
}

/// ARM's `LSR`/`LSL` by a register ≥ 32 yields 0 (ARMv4-defined; the `q = 31`
/// drain-by-32 leg is the hot reason this kernel relies on it). Rust's `>> 32`
/// would panic in debug, so the leg is spelled — same as the probe's twin did.
#[inline]
fn shr_asm(w: u32, n: u32) -> u32 {
    if n >= 32 { 0 } else { w >> n }
}

/// C1 exactly as the asm runs it: isolate the lowest set bit, one `MUL` by the
/// De Bruijn constant, `>> 27`, one table `LDRB`. `w != 0` at every call site
/// — `w == 0` is the handoff signal to the word-skip tier, never an answer.
#[inline]
fn ctz32_asm(w: u32) -> u32 {
    let low = w & w.wrapping_neg();
    let idx = (low.wrapping_mul(BRUIJN_K) >> 27) & 31;
    u32::from(BRUIJN_INDEX[idx as usize])
}

/// The walk's per-partition constants, hoisted once (the kernel does the same
/// at its entry: `m` into `r11`, `MAX >> p` into a frame slot).
#[derive(Clone, Copy, Debug)]
struct Walk {
    /// Rice order.
    p: u32,
    /// `(1 << p) - 1`.
    m: u32,
    /// `u32::MAX >> p` — §9.2.7.3's "the residual must fit in 32 bits".
    limit: u32,
}

/// The kernel's cursor, in slice-relative offsets (the model's view; the
/// kernel's own view is addresses, and only [`run_asm`] deals in that).
#[derive(Clone, Copy, Debug)]
struct Cursor {
    w: u32,
    v: u32,
    ptr: usize,
    end: usize,
}

impl Cursor {
    const fn from_window(k: &KernelWindow) -> Self {
        Self {
            w: k.w,
            v: k.v,
            ptr: k.ptr,
            end: k.end,
        }
    }

    const fn window(&self) -> KernelWindow {
        KernelWindow {
            w: self.w,
            v: self.v,
            ptr: self.ptr,
            end: self.end,
        }
    }
}

/// `.Lrem_have` — the tail every sample goes through, whatever tier counted
/// its run: drain `p` bits, un-reverse the remainder, compose the coded
/// number, apply both §9.2.7.3 guards, fold, store. `Some(status)` means the
/// guard fired and no sample was written.
#[inline]
fn tail(
    c: &mut Cursor,
    walk: &Walk,
    out: &mut [i32],
    i: &mut usize,
    q_total: u32,
) -> Option<KernelStatus> {
    let rem_w = c.w & walk.m;
    c.w = shr_asm(c.w, walk.p);
    c.v -= walk.p;
    // [6a-4 G1] after the drain, not before it: the incumbent reads the run AND
    // the remainder and only then asks §9.2.7.3, so a rejected codeword leaves
    // its reader past the whole codeword. The model and the asm both follow that
    // position, and the guard arm in tests/arm_from_outside.rs compares cursor
    // bit positions, not just error kinds — which is how the ordering got fixed
    // rather than reasoned about.
    if q_total > walk.limit {
        return Some(KernelStatus::GuardRejected);
    }
    // The stitch: two REV8 lookups form the 16-bit reversal of the
    // window-domain extract, then `>> (16 − p)`. Exact for p ≤ 16, which is
    // this kernel's domain (MAX_ORDER).
    let lo = u32::from(REV8[usize::from((rem_w & 0xFF) as u8)]);
    let hi = u32::from(REV8[usize::from(((rem_w >> 8) & 0xFF) as u8)]);
    let rem = ((lo << 8) | hi) >> (16 - walk.p);
    // Coded number. `wrapping_shl` because ARM's shift wraps and this model
    // mirrors the asm; a quotient too large for the shift is rejected above,
    // so on the accepted path the wrap never carries a real value away.
    let coded = q_total.wrapping_shl(walk.p) | rem;
    // [6a-4 G2] §9.2.7.3's second guard, on the pre-fold value: `u32::MAX` is
    // the one coded number whose sample is `i32::MIN`, and the spec forbids it.
    // The asm tests the folded value for `i32::MIN` instead — the fold is a
    // bijection, so the two are the same test, and the asm's spelling is two
    // instructions instead of re-forming a 32-bit constant.
    if coded == u32::MAX {
        return Some(KernelStatus::GuardRejected);
    }
    out[*i] = flac_lite::residual::rice_unmap(coded);
    *i += 1;
    None
}

/// A step-for-step Rust model of `arm_rice_partition`, label-for-label over
/// the asm's own structure (`.Lloop`/`.Lrefill1`/`.Ltier1`/`.Ltier23`/
/// `.Llzc`/`.Lrefill2`/`.Ltier3`/`.Lt3skip`/`.Lwslow`/`.Lt3term`/
/// `.Lrem_have`/`.Lslow`/`.Lunterm`/`.Lvalerr`).
///
/// **Its authority.** On a host it is what `ArmRiceKernel` runs, so the
/// wrapper, the window mapping, the status mapping and the delegation are all
/// testable there. On the target it is *reachable but not used by the
/// kernel*: the 6a-4 kernel-check ROM runs it as a second leg precisely so a
/// disagreement localizes to "the assembly" rather than to "the algorithm and
/// the ABI". It has no independent authority on correctness — the core's
/// incumbent and `flac -d` remain the judges, and the differential against
/// them is what this model is witnessed by. Keeping it in the same cfg as the
/// asm (`rice-arm`) is deliberate: the ROM that needs the second leg is the
/// ROM that needs the first.
#[must_use]
pub fn model_partition(
    window: &KernelWindow,
    region: &[u8],
    out: &mut [i32],
    order: u32,
) -> (KernelWindow, KernelStatus) {
    debug_assert!(
        order <= ArmRiceKernel::MAX_ORDER,
        "the model mirrors the kernel's domain; wider orders are the wrapper's delegation"
    );
    let walk = Walk {
        p: order,
        m: (1u32 << order) - 1,
        limit: u32::MAX >> order,
    };
    let mut c = Cursor::from_window(window);
    let mut i = 0usize;

    // `.Lloop`
    while i < out.len() {
        let q_total: u32;

        // `.Lrefill1` — merge bytes until the window covers a codeword's
        // worst case (25: 8 for the tier-1 byte plus the widest order this
        // kernel computes, with the drain's headroom). Bytes gone first?
        // `.Lslow`, below.
        if c.v < 25 {
            while c.v < 25 {
                if c.ptr >= c.end {
                    break;
                }
                // `v < 25` keeps the merge shift below 32 by the entry rule.
                c.w |= u32::from(REV8[usize::from(region[c.ptr])]) << c.v;
                c.ptr += 1;
                c.v += 8;
            }
            if c.v < 25 {
                // `.Lslow` / `.Lslowrun` — cold tail: the region is exhausted
                // and fewer than 25 bits are left. A per-bit walk with full
                // bounds, which is what gives the EOF contract its uniform
                // shape without touching the hot path.
                let mut q = 0u32;
                loop {
                    if c.v == 0 {
                        return (c.window(), KernelStatus::EndOfStream);
                    }
                    if c.w & 1 == 1 {
                        break;
                    }
                    q += 1;
                    c.w >>= 1;
                    c.v -= 1;
                }
                // `.Lslowterm`: consume the terminating one — it is a
                // terminator, not a count (the probe found this as a fix; the
                // model keeps it).
                c.w >>= 1;
                c.v -= 1;
                if c.v < walk.p {
                    return (c.window(), KernelStatus::EndOfStream);
                }
                q_total = q;
                if let Some(status) = tail(&mut c, &walk, out, &mut i, q_total) {
                    return (c.window(), status);
                }
                continue;
            }
        }

        // `.Ltier1` — entry invariant: `v >= 25`.
        let q8 = u32::from(CTZ8[usize::from((c.w & 0xFF) as u8)]);
        if q8 < 8 {
            // `q ≤ 7`, so `v' = v − (q+1) ≥ 25 − 8 = 17 ≥ 16 ≥ p`: the
            // remainder provably needs no refill. Unconditional *because* of
            // the entry invariant, which is what removed a cmp+branch from the
            // hot path.
            let drain = q8 + 1;
            c.w = shr_asm(c.w, drain);
            c.v -= drain;
            q_total = q8;
            if let Some(status) = tail(&mut c, &walk, out, &mut i, q_total) {
                return (c.window(), status);
            }
            continue;
        }

        // `.Ltier23` — `w == 0` is the handoff signal, never an answer.
        if c.w != 0 {
            // `.Llzc` — C1. `w != 0` means a set bit sits inside the valid
            // bits, so `qdb ≤ v − 1` and the drain of `qdb + 1` is ≤ v. The
            // `qdb == 31` case drains by 32, which is the shift that yields 0.
            let qdb = ctz32_asm(c.w);
            let drain = qdb + 1;
            c.w = shr_asm(c.w, drain);
            c.v -= drain;
            q_total = qdb;
            if c.v < walk.p {
                // `.Lrefill2` — cold: refill to 25, or discover the region is
                // out and let the window's valid bits answer.
                loop {
                    if c.ptr >= c.end {
                        if c.v < walk.p {
                            return (c.window(), KernelStatus::EndOfStream);
                        }
                        break;
                    }
                    c.w |= u32::from(REV8[usize::from(region[c.ptr])]) << c.v;
                    c.ptr += 1;
                    c.v += 8;
                    if c.v >= 25 {
                        break;
                    }
                }
            }
            if let Some(status) = tail(&mut c, &walk, out, &mut i, q_total) {
                return (c.window(), status);
            }
            continue;
        }

        // `.Ltier3` — C2 word-skip. The window is empty of set bits and bytes
        // remain: every valid bit was zero, so the run so far is `v`, and the
        // window is drained.
        if c.ptr >= c.end {
            return (c.window(), KernelStatus::EndOfStream);
        }
        let mut base = c.v;
        c.v = 0;
        let mut done = false;
        while !done {
            if c.ptr + 4 > c.end {
                // `.Lwslow` — fewer than four bytes ahead: walk bytes. A zero
                // byte is +8 and continues; the first non-zero byte enters the
                // window, reversed.
                loop {
                    if c.ptr >= c.end {
                        return (c.window(), KernelStatus::EndOfStream);
                    }
                    let b = region[c.ptr];
                    c.ptr += 1;
                    if b != 0 {
                        c.w = u32::from(REV8[usize::from(b)]);
                        c.v = 8;
                        break;
                    }
                    base += 8;
                }
                done = true;
            } else {
                // `.Lt3skip` — whole-word zero test on RAW bytes: a word is
                // zero exactly when its reversal is, so the skip pays no
                // REV8. This is the tier that makes a long run O(q/32).
                let word = u32::from(region[c.ptr])
                    | (u32::from(region[c.ptr + 1]) << 8)
                    | (u32::from(region[c.ptr + 2]) << 16)
                    | (u32::from(region[c.ptr + 3]) << 24);
                c.ptr += 4;
                if word != 0 {
                    // `.Lt3term` — the terminating word does pay REV8: its
                    // bits enter the window.
                    c.w = u32::from(REV8[usize::from((word & 0xFF) as u8)])
                        | (u32::from(REV8[usize::from(((word >> 8) & 0xFF) as u8)]) << 8)
                        | (u32::from(REV8[usize::from(((word >> 16) & 0xFF) as u8)]) << 16)
                        | (u32::from(REV8[usize::from(((word >> 24) & 0xFF) as u8)]) << 24);
                    c.v = 32;
                    done = true;
                } else {
                    base += 32;
                }
            }
        }
        // The run's window-relative part is answered by C1, then the base is
        // added — exactly the `.Llzc` the asm jumps to with `base` already in
        // `r2`.
        let qdb = ctz32_asm(c.w);
        let drain = qdb + 1;
        c.w = shr_asm(c.w, drain);
        c.v -= drain;
        q_total = base + qdb;
        if c.v < walk.p {
            loop {
                if c.ptr >= c.end {
                    if c.v < walk.p {
                        return (c.window(), KernelStatus::EndOfStream);
                    }
                    break;
                }
                c.w |= u32::from(REV8[usize::from(region[c.ptr])]) << c.v;
                c.ptr += 1;
                c.v += 8;
                if c.v >= 25 {
                    break;
                }
            }
        }
        if let Some(status) = tail(&mut c, &walk, out, &mut i, q_total) {
            return (c.window(), status);
        }
    }

    (c.window(), KernelStatus::Done)
}
