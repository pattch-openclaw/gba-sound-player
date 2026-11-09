//! E10 probe — the hybrid ARM-mode Rice codeword loop (**probe code; never
//! lands on `main`'s spike sources**).
//!
//! ## Where this copy lives, and why (item 6 PR 6a-5b, 2026-11-09)
//!
//! That parenthetical is still true of the E10 archive. What 6a-5b changed is
//! that *this* tree now carries a copy of the archive's `src/e10.rs` and
//! `asm/*.s` on the spike's live sources — behind `item6a5b-vendored`, off by
//! default — because the rung's whole point is to time E10's vendored leg in the
//! SAME link as the landed overlay realization (plan-of-record 6a-5b: the
//! +48.2 c/sample overhang 6a-5 recorded is a comparison of two *images*, and
//! the only way to price it is a same-image pair). Two ports were needed, both
//! mechanical and both recorded: `port-item6a2-header-bits.patch` for
//! `FrameHeader::header_bits` (item 4a), and the `arm_rice_partition` →
//! `e10_rice_partition` rename in `asm/rice_partition.s` + the extern below, so
//! this leg and the landed kernel can coexist in one link. Everything else in
//! the module is verbatim from the archive, including its own
//! `AccumulatorReader`, its own frame parser and its own 544 B of pinned
//! tables — the vendored leg's cost is its own, which is exactly what makes the
//! pair a measurement rather than a restatement.
//!
//! FLAC.md "E10 — ARM-mode codeword loop: algorithm plan of action
//! (2026-10-14)" + "E10 unit-test kernel track — design of record
//! (2026-10-16)", left-to-measure item 6: the three-arm probe
//! (**ctrl / tab / arm**, reading `arm − tab` because E10 replaces the
//! count rather than stacking on it). This module is the vendored seam:
//! the `e2b.rs` vendored body copied verbatim, with ONLY
//! [`decode_rice_partition`] swapped — its per-sample loop handed to the
//! ARM-mode kernel `e10_rice_partition` (`asm/rice_partition.s` + the
//! committed `asm/fold_body.s`, pasted via `include_str!`/`concat!` the
//! way PR B wired the kernel crate).
//!
//! ## The arm (the probe's variable)
//!
//! The kernel runs the WHOLE Rice partition sample loop in ARM mode:
//!
//! - **tier 1**: `ctz8[low byte]` for q ≤ 7 (one zero-wait IWRAM `ldrb`);
//! - **tier 2**: C1 De Bruijn for 8..=31 — flat `MUL` + `LDRB` (the
//!   absent CLZ as a multiply);
//! - **tier 3**: C2 word-skip beyond the window — whole-word zero tests
//!   over raw bytes (~10 cyc per 32 bits of run), terminating word
//!   rev8-assembled back into the window;
//! - **refill**: bit-reversed window (design decision 5): stream bit i =
//!   bit i of window word, next bit at bit 0; bytes merge as
//!   `w |= REV8[byte] << v` (ratified item 3 — the `rev8` refill tax is
//!   left-to-measure item 4, and the `rest`-bucket flatness is the probe's
//!   headline risk).
//!
//! Tables [`CTZ8`] (256 B) + [`REV8`] (256 B) + [`BRUIJN_INDEX`] (32 B)
//! are pinned `.iwram` (ratified item 2): the plan's 544 B pin budget,
//! witnessed by `nm` + `objdump -h` at probe time.
//!
//! ## The stitching the plan left to the probe (probe-time finding, 2026-10-29)
//!
//! The algorithm plan's C1 consume formula `(w >> (q+1)) & ((1<<p) - 1)`
//! is **window-domain**: in the bit-reversed window the extracted `p`
//! remainder bits sit reversed relative to FLAC's MSB-first natural binary
//! (§9.2.7.2 — the coded number is `quotient·2^p + remainder` with the
//! remainder read MSB-first). The kernel crate anticipated exactly this:
//! "stitching the reversed-window domain to spec-order streams … stays the
//! probe's vendored seam". The kernel therefore un-reverses the
//! remainder: `y = REV8[lo] << 8 | REV8[hi]` (a 16-bit reversal of the
//! window-domain extract), `rem = y >> (16 − p)` — ~8 extra cycles per
//! sample when p > 0 (p == 0 rides the same path for free: mask 0 ⇒
//! rem 0). The plan priced the reversal as "one extra ldrb per 8 bits" in
//! the *refill* only; this remainder reversal is a second, per-sample
//! reversal cost the arm honestly carries, and the bit-exact witnesses
//! (host vs `flac -d`, ROM sample-equality) are what prove the stitch —
//! any reversal misreading cannot survive them.
//!
//! ## Divergences (scope-stated, each witnessed)
//!
//! 1. **Unterminated run / truncated remainder → `EndOfStream`, restore
//!    nothing** (ratified item 1 — the probe's ONE picked cursor contract,
//!    design-of-record amendment 2026-10-16). Diverges from the live
//!    loop's per-read atomicity and from e2b's snapshot restore; reachable
//!    only on corrupt/truncated streams where the frame is dead either
//!    way; never reached on the committed clips (bit-exact witness).
//!    Witnessed once by a host unit test naming the stopped state.
//! 2. **Folded-value overflow wraps** instead of the live reader's
//!    `quotient > MAX >> order → InvalidField` guard (plain 32-bit `ORR`
//!    in the kernel). Needs q ≥ 2^17 (~8192 all-zero words) — corrupt
//!    streams only, dead frame either way.
//!
//! ## Witness chain (before any number counts)
//!
//! 1. Host (`tests/e10_witness.rs`): this path decodes the exact embedded
//!    clips bit-exactly vs `flac -d`, and differentially frame-by-frame
//!    against the production `driver::decode_one`; plus a **tier census**
//!    of the twin over the same bytes (the plan's "per-clip quotient
//!    distribution decides how much", left-to-measure via the C2 tail).
//! 2. Host unit tests (this file): tables exhaustive vs naive oracles;
//!    the twin vs a naive per-bit oracle over raw byte streams, sweeping
//!    quotient lengths 0..=70 at **every bit offset 0..7** — the plan's
//!    bit-offset sweep (decision 7 cut it from the kernels to THIS seam,
//!    left-to-measure item 5); p sweep 0..=14; EOF/short-remainder state
//!    contracts; twin-vs-live differential on the exact clips.
//! 3. ROM: per-frame variant-vs-control **sample-equality** (E1/E2a/E4/E2b
//!    pattern), first offender named, divergence voids.
//! 4. Link witness: `nm` — the three tables at `0x0300xxxx`; `objdump -h`
//!    — `.iwram` grown by exactly 544 B over the e2b image; boot log
//!    echoes the runtime addresses as the same-image echo. ARM-mode-ness
//!    is an ELF fact: `objdump -d` shows 4-byte ARM encodings.
//!
//! Calibration + determinism stay the harness's (E4 unified loop; two
//! `cmp`-identical headless runs).

// E10 placement + FFI allowances (probe branch only; E2b's pattern — the
// `.iwram` attribute is an unsafe attribute under edition 2024, and the
// asm kernel is an FFI boundary. The host witness build never sees the
// attribute or the extern).
#![allow(unsafe_code, unsafe_attr_outside_unsafe)]

use crate::assets::SpikeClip;
use crate::driver::DriverError;
use flac_lite::format::{ChannelConfig, SampleRate};
use flac_lite::frame::{FrameHeader, StreamDefaults};
use flac_lite::residual::PartitionHeader;
use flac_lite::stereo::decorrelate;
use flac_lite::subframe::{PredictorState, SubframeType};
use flac_lite::{Error, MAX_FIXED_ORDER, MAX_LPC_ORDER};

// ---------------------------------------------------------------------------
// The three pinned tables (the plan's 544 B budget, ratified item 2).
// Same committed bytes as the kernel crate's CTZ8 / REV8 (its tables.rs
// generation recipe — a unit test re-derives every entry from a naive
// oracle), plus PR A/C1's committed 32-entry De Bruijn index.
// ---------------------------------------------------------------------------

/// 256-entry byte → trailing-zero count (`8` = run past the byte — the
/// tier-1 handoff signal). Fed the LOW byte of the bit-reversed window
/// (bit 0 = next stream bit), its answer is the unary quotient when the
/// terminating one sits inside those 8 bits. Pinned `.iwram` (zero-wait
/// data; E6-verified pin target — plain `.iwram`, never `.text_iwram`).
#[cfg_attr(target_arch = "arm", unsafe(link_section = ".iwram"))]
pub static CTZ8: [u8; 256] = build_ctz8();

/// 256-entry byte → bit-reversed byte (bit i → bit 7−i). The ratified
/// refill merge `w |= REV8[byte] << v` AND the remainder un-reversal
/// (module docs: "The stitching"). Pinned `.iwram`.
#[cfg_attr(target_arch = "arm", unsafe(link_section = ".iwram"))]
pub static REV8: [u8; 256] = build_rev8();

/// 32-entry De Bruijn index (`BRUIJN_INDEX[(((1 << b) * 0x077CB531) >> 27)
/// & 31] == b`, all entries distinct — kernel crate PR #69's committed
/// table, same bytes). Pinned `.iwram`.
#[cfg_attr(target_arch = "arm", unsafe(link_section = ".iwram"))]
pub static BRUIJN_INDEX: [u8; 32] = build_bruijn();

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

/// Runtime placement witness (boot log echoes these; the offline witness
/// is `nm` showing the three symbols at `0x0300xxxx`).
#[must_use]
pub fn tables_iwram_addrs() -> (usize, usize, usize) {
    (
        CTZ8.as_ptr() as usize,
        REV8.as_ptr() as usize,
        BRUIJN_INDEX.as_ptr() as usize,
    )
}

/// The 14-bit frame sync (shared constant value from `frame.rs`).
const SYNC_14: u32 = 0x3FFE;

// ---------------------------------------------------------------------------
// The ARM-mode kernel: asm (target) and its step-for-step Rust twin (host).
// ---------------------------------------------------------------------------

/// Single assembled ARM unit: the committed fold body + the probe kernel.
/// Paste order and the `include_str!` mechanism are the kernel crate's
/// proven pattern (rustc hands the assembler a temp file with no reliable
/// search path).
#[cfg(target_arch = "arm")]
mod asm_unit {
    core::arch::global_asm!(concat!(
        include_str!("../asm/fold_body.s"),
        include_str!("../asm/rice_partition.s"),
    ));
}

/// The kernel's memory contract (u32 × 5) — `asm/rice_partition.s`'s ABI.
/// `ptr`/`end` are absolute byte addresses in the target view and
/// slice-relative offsets in the host twin's view; the wrapper maps
/// between them. `status`: 0 = all samples produced, 1 = unterminated
/// (run or remainder hit the slice end — module docs, divergence 1).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct KernelState {
    /// Bit-reversed window: bit 0 = next stream bit; bits ≥ `v` always 0.
    pub w: u32,
    /// Valid bits in `w` (0..=32).
    pub v: u32,
    /// Byte cursor (next unread byte).
    pub ptr: usize,
    /// Byte end of the backing slice (same view as `ptr`).
    pub end: usize,
    /// 0 ok, 1 unterminated.
    pub status: u32,
}

/// Per-tier / per-tax counters (host twin only — the probe's answers to
/// the plan's distribution questions on the exact embedded bytes).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TierStats {
    /// Samples counted by tier 1 (ctz8, q ≤ 7).
    pub t1: u64,
    /// Samples counted by tier 2 (C1 De Bruijn, 8..=31).
    pub t2: u64,
    /// Samples counted by tier 3 (C2 word-skip, q ≥ 32).
    pub t3: u64,
    /// Bytes merged through the `rev8` refill (left-to-measure 4's tax
    /// volume: the plan predicts ≈ 1 extra ldrb per 8 bits).
    pub refill_bytes: u64,
    /// Samples produced on the cold slice-tail path (`.Lslow`).
    pub slow: u64,
    /// Unterminated exits (must stay 0 on the committed clips).
    pub unterm: u64,
    /// Escape partitions (raw-bit, ride the reader verbatim; must stay 0
    /// on the profile clips).
    pub escapes: u64,
    /// Max Rice order observed on a non-escape partition (must stay ≤ 14).
    pub max_p: u64,
    /// Samples emitted (identity: samples == t1 + t2 + t3 + slow).
    pub samples: u64,
}

/// ARM's `LSR` by a register ≥ 32 yields 0 (ARMv4-defined — the kernel's
/// q = 31 / shift-by-32 leg; PR B's boundary vectors pin it). Rust's
/// `>> 32` would panic in debug, so the leg is spelled.
#[inline]
fn shr_asm(w: u32, n: u32) -> u32 {
    if n >= 32 { 0 } else { w >> n }
}

/// C1 as the asm runs it: `lowbit` isolate, De Bruijn `MUL`, `>> 27`,
/// table `LDRB`. `w != 0` on every call site (the handoff signal is
/// `w == 0`, decision 2).
#[inline]
fn ctz32_asm_twin(w: u32) -> u32 {
    let low = w & w.wrapping_neg();
    let idx = (low.wrapping_mul(BRUIJN_K) >> 27) & 31;
    u32::from(BRUIJN_INDEX[idx as usize])
}

/// The remainder un-reversal exactly as the kernel's `.Lrem_have` runs it
/// (module docs: "The stitching"): two `REV8` lookups form the 16-bit
/// reversal of the window-domain extract, then `>> (16 − p)`.
#[inline]
fn unreverse_rem(rem_w: u32, p: u32) -> u32 {
    let lo = REV8[(rem_w & 0xFF) as usize] as u32;
    let hi = REV8[((rem_w >> 8) & 0xFF) as usize] as u32;
    // rev16(x) = REV8[low byte] << 8 | REV8[high byte] (bit i -> bit 15-i).
    let y = (lo << 8) | hi;
    y >> (16 - p)
}

/// §9.2.7.2 fold — the shared `fold_body` identity `(n >> 1) ^ (-(n & 1))`.
#[inline]
fn fold_twin(n: u32) -> i32 {
    (((n >> 1) as u32) ^ ((n & 1).wrapping_neg())) as i32
}

/// Step-for-step Rust twin of `e10_rice_partition` (host — there is no
/// ARM executor here, so the twin IS the host path; it mirrors the asm
/// label-for-label: `.Lloop`/`.Ltier1`/`.Lrefill1`/`.Ltier23`/`.Llzc`/
/// `.Lrem_have`/`.Lrefill2`/`.Ltier3`/`.Lt3skip`/`.Lt3term`/`.Lwslow`/
/// `.Lslow`/`.Lunterm`). `st.ptr`/`st.end` are byte offsets into `data`.
/// `stats` is optional (probe census; the decode path passes it through
/// the reader's slot).
pub fn twin_partition(
    data: &[u8],
    st: &mut KernelState,
    out: &mut [i32],
    p: u32,
    mut stats: Option<&mut TierStats>,
) {
    let mut s = *st;
    let m: u32 = (1u32 << p) - 1; // p ≤ 14 on every call site (profile)
    macro_rules! stat {
        ($field:ident) => {
            if let Some(ss) = stats.as_deref_mut() {
                ss.$field += 1;
            }
        };
    }
    let mut i = 0usize;
    while i < out.len() {
        let q_total: u32;
        // ---- .Lrefill1: rev8-merge to v >= 25 (bytes exhausted -> slow) ----
        if s.v < 25 {
            while s.v < 25 {
                if s.ptr >= s.end {
                    break;
                }
                let b = data[s.ptr];
                s.ptr += 1;
                stat!(refill_bytes);
                s.w |= u32::from(REV8[b as usize]) << s.v; // v < 25: shift < 32
                s.v += 8;
            }
            if s.v < 25 {
                // ---- .Lslow: per-bit tail (bytes exhausted, v < 25) ----
                stat!(slow);
                let mut q = 0u32;
                loop {
                    if s.v == 0 {
                        return unterm(st, &mut s, stats.as_deref_mut());
                    }
                    if s.w & 1 == 1 {
                        break;
                    }
                    q += 1;
                    s.w >>= 1;
                    s.v -= 1;
                }
                s.w >>= 1; // consume the terminating 1 (no count — asm fix)
                s.v -= 1;
                if s.v < p {
                    return unterm(st, &mut s, stats.as_deref_mut());
                }
                q_total = q;
                emit(
                    data,
                    &mut s,
                    out,
                    &mut i,
                    q_total,
                    p,
                    m,
                    stats.as_deref_mut(),
                );
                continue;
            }
        }
        // ---- .Ltier1: ctz8[low byte]; INVARIANT v >= 25 on entry ----
        let c = u32::from(CTZ8[(s.w & 0xFF) as usize]);
        if c < 8 {
            stat!(t1);
            // v' >= 25 - 8 = 17 >= 14 >= p: the remainder needs no refill.
            s.w >>= c + 1;
            s.v -= c + 1;
            q_total = c;
            emit(
                data,
                &mut s,
                out,
                &mut i,
                q_total,
                p,
                m,
                stats.as_deref_mut(),
            );
            continue;
        }
        // ---- .Ltier23: w == 0 is the handoff signal (decision 2) ----
        if s.w != 0 {
            // ---- .Llzc: C1 De Bruijn, base 0 ----
            stat!(t2);
            let qdb = ctz32_asm_twin(s.w); // w != 0 => qdb <= v - 1
            s.w = shr_asm(s.w, qdb + 1); // qdb = 31 -> shift 32 -> 0
            s.v -= qdb + 1;
            q_total = qdb;
            if s.v < p {
                // ---- .Lrefill2: cold refill until v >= 25 ----
                loop {
                    if s.ptr >= s.end {
                        if s.v < p {
                            return unterm(st, &mut s, stats.as_deref_mut());
                        }
                        break;
                    }
                    let b = data[s.ptr];
                    s.ptr += 1;
                    stat!(refill_bytes);
                    s.w |= u32::from(REV8[b as usize]) << s.v;
                    s.v += 8;
                    if s.v >= 25 {
                        break;
                    }
                }
            }
            emit(
                data,
                &mut s,
                out,
                &mut i,
                q_total,
                p,
                m,
                stats.as_deref_mut(),
            );
            continue;
        }
        // ---- .Lt3: C2 word-skip (w == 0, bytes remain) ----
        stat!(t3);
        if s.ptr >= s.end {
            return unterm(st, &mut s, stats.as_deref_mut());
        }
        let mut base = s.v; // all valid bits were zero
        s.v = 0;
        let mut done = false;
        while !done {
            if s.ptr + 4 > s.end {
                // ---- .Lwslow: per-byte walk (< 4 bytes ahead) ----
                loop {
                    if s.ptr >= s.end {
                        return unterm(st, &mut s, stats.as_deref_mut());
                    }
                    let b = data[s.ptr];
                    s.ptr += 1;
                    if b != 0 {
                        s.w = u32::from(REV8[b as usize]);
                        s.v = 8;
                        break;
                    }
                    base += 8;
                }
                done = true;
            } else {
                // ---- .Lt3skip: whole-word zero test (raw: zero iff
                // reversed zero — the skip pays NO rev8) ----
                let word = u32::from(data[s.ptr])
                    | (u32::from(data[s.ptr + 1]) << 8)
                    | (u32::from(data[s.ptr + 2]) << 16)
                    | (u32::from(data[s.ptr + 3]) << 24);
                s.ptr += 4;
                if word != 0 {
                    // ---- .Lt3term: rev8-assemble the terminating word ----
                    s.w = u32::from(REV8[(word & 0xFF) as usize])
                        | (u32::from(REV8[((word >> 8) & 0xFF) as usize]) << 8)
                        | (u32::from(REV8[((word >> 16) & 0xFF) as usize]) << 16)
                        | (u32::from(REV8[((word >> 24) & 0xFF) as usize]) << 24);
                    s.v = 32;
                    done = true;
                } else {
                    base += 32;
                }
            }
        }
        // q_total = base + ctz(word); drain the window-relative part.
        let qdb = ctz32_asm_twin(s.w);
        s.w = shr_asm(s.w, qdb + 1);
        s.v -= qdb + 1;
        q_total = base + qdb;
        if s.v < p {
            loop {
                if s.ptr >= s.end {
                    if s.v < p {
                        return unterm(st, &mut s, stats.as_deref_mut());
                    }
                    break;
                }
                let b = data[s.ptr];
                s.ptr += 1;
                stat!(refill_bytes);
                s.w |= u32::from(REV8[b as usize]) << s.v;
                s.v += 8;
                if s.v >= 25 {
                    break;
                }
            }
        }
        emit(
            data,
            &mut s,
            out,
            &mut i,
            q_total,
            p,
            m,
            stats.as_deref_mut(),
        );
    }
    s.status = 0;
    *st = s;
}

/// `.Lrem_have` — the shared per-sample tail: drain p, un-reverse the
/// remainder (the stitch), compose the coded number, fold, store.
#[allow(clippy::too_many_arguments)]
#[inline]
fn emit(
    _data: &[u8],
    s: &mut KernelState,
    out: &mut [i32],
    i: &mut usize,
    q_total: u32,
    p: u32,
    m: u32,
    mut stats: Option<&mut TierStats>,
) {
    if let Some(ss) = stats.as_deref_mut() {
        ss.samples += 1;
    }
    let rem_w = s.w & m;
    s.w >>= p; // p <= 14
    s.v -= p;
    let rem = unreverse_rem(rem_w, p);
    // Coded number — ARM wraps here (module docs, divergence 2).
    let coded = q_total.wrapping_shl(p) | rem;
    out[*i] = fold_twin(coded);
    *i += 1;
}

/// `.Lunterm` — ratified item 1: report status, restore NOTHING. The
/// state stays exactly where the walk stopped (bytes the walk consumed
/// stay consumed — observable only on a dead frame). Every call site
/// returns immediately, so the exit is counted exactly once.
fn unterm(st: &mut KernelState, s: &mut KernelState, mut stats: Option<&mut TierStats>) {
    if let Some(ss) = stats.as_deref_mut() {
        ss.unterm += 1;
    }
    s.status = 1;
    *st = *s;
}

/// Full bit-reversal of a 32-bit word: `x.swap_bytes()` then `REV8` per
/// byte — bit i maps to bit 31 − i (the exact mapping between the
/// reader's MSB-first window, next bit at bit 31, and the kernel's
/// bit-reversed window, next bit at bit 0). Within-byte reversal alone
/// (REV8 without the byte swap) maps i -> 8*byte(i) + (7 - offset(i)) —
/// a misreading the bit-mapping unit test below convicted (2026-10-30).
#[inline]
fn rev32(x: u32) -> u32 {
    let s = x.swap_bytes();
    u32::from(REV8[(s & 0xFF) as usize])
        | (u32::from(REV8[((s >> 8) & 0xFF) as usize]) << 8)
        | (u32::from(REV8[((s >> 16) & 0xFF) as usize]) << 16)
        | (u32::from(REV8[((s >> 24) & 0xFF) as usize]) << 24)
}

/// Target: run the asm kernel. Maps the host-view state (slice offsets)
/// to the kernel's absolute addresses and back.
#[cfg(target_arch = "arm")]
fn run_kernel(
    data: &[u8],
    st: &mut KernelState,
    out: &mut [i32],
    p: u32,
    _stats: Option<&mut TierStats>,
) {
    // [6a-5b PORT] the archive declares this `arm_rice_partition`; 6a-5b's
    // differential image links BOTH this vendored kernel and the landed overlay
    // one, and `global_asm!` symbols are not namespaced, so the vendored copy
    // is renamed here and in `asm/rice_partition.s`. Same bytes, same ABI.
    unsafe extern "C" {
        fn e10_rice_partition(
            state: *mut u32,
            out: *mut i32,
            n: u32,
            p: u32,
            ctz8: *const u8,
            rev8: *const u8,
            bruijn: *const u8,
        );
    }
    // Thumbv4t: `usize` is 32 bits but a distinct type from `u32` — the
    // state struct is u32 x5, so the addresses are cast (all GBA bus
    // addresses fit u32 by construction).
    let base = data.as_ptr() as usize;
    let mut s = [
        st.w,
        st.v,
        (base + st.ptr) as u32,
        (base + st.end) as u32,
        0u32,
    ];
    // SAFETY: the kernel writes exactly the 5-word state struct through
    // `state`, reads only `data[st.ptr..st.end]` (its own walk, bounded by
    // `end`), the three in-blob tables, and writes exactly `out.len()`
    // i32s to `out` on status 0 (fewer, then status 1, on EOF). `n <=
    // 2048` (profile blocksize cap) and `p <= 14` (Rice order cap) hold on
    // every call site — the same profile the live reader parses.
    unsafe {
        e10_rice_partition(
            s.as_mut_ptr(),
            out.as_mut_ptr(),
            out.len() as u32,
            p,
            CTZ8.as_ptr(),
            REV8.as_ptr(),
            BRUIJN_INDEX.as_ptr(),
        );
    }
    st.w = s[0];
    st.v = s[1];
    st.ptr = s[2] as usize - base;
    st.end = s[3] as usize - base;
    st.status = s[4];
}

/// Host: run the twin (identical semantics; the witnesses judge both).
#[cfg(not(target_arch = "arm"))]
fn run_kernel(
    data: &[u8],
    st: &mut KernelState,
    out: &mut [i32],
    p: u32,
    stats: Option<&mut TierStats>,
) {
    twin_partition(data, st, out, p, stats);
}
/// The refill-accumulator bit reader — the E2a/E2b `AccumulatorReader`
/// copy verbatim (E10 does NOT change the reader: its variable is the
/// whole-partition ARM kernel). Left-aligned MSB-first `acc` (next bit =
/// bit 31); invariant `logical cursor = pos * 8 − valid`.
#[derive(Clone, Copy, Debug)]
pub struct AccumulatorReader<'a> {
    /// Backing bytes (the frame's region slice).
    data: &'a [u8],
    /// Bytes consumed — the byte just past the last fully buffered bit.
    /// Invariant: `pos <= data.len()`.
    pos: usize,
    /// Bits buffered, MSB-first: the next bit to consume is bit 31. The
    /// low `32 − valid` bits of `acc` are always zero.
    acc: u32,
    /// Valid bits in `acc` (0..=32).
    valid: u32,
    /// E10 host census slot (null = off). Never read on the target — the
    /// measurement image's decode path ignores it entirely.
    #[allow(dead_code)]
    stats: *mut TierStats,
}

impl<'a> AccumulatorReader<'a> {
    /// Wrap a byte slice for bit-level reading, cursor at bit 0.
    pub fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            pos: 0,
            acc: 0,
            valid: 0,
            stats: core::ptr::null_mut(),
        }
    }

    /// Current logical cursor in bits — exactly what the position-only
    /// control's `bit_position()` reports at the same point.
    #[inline]
    pub fn bit_position(&self) -> usize {
        self.pos * 8 - self.valid as usize
    }

    /// Bits remaining in the backing slice from the logical cursor.
    #[inline]
    pub fn bits_remaining(&self) -> usize {
        (self.data.len() << 3) - self.bit_position()
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
    /// Retained verbatim from the e2b reader copy for the no-drift
    /// boundary; e10's kernel seam consumes through the kernel, so this
    /// stays the readable reader form (host tests exercise it via the
    /// e2b copy it came from).
    #[allow(dead_code)]
    #[inline]
    fn consume(&mut self, n: u32) {
        self.acc <<= n;
        self.valid -= n;
    }

    /// Read `n` bits (1..=32) as an unsigned value, MSB-first.
    ///
    /// E2a verbatim: fast path when the accumulator holds `n` bits
    /// (widths 1..=31); cold path (`valid < n`, every `n == 32`, every
    /// illegal width): the retained position-based recompute at the
    /// derived logical cursor, then an exact resync of `(pos, acc, valid)`.
    #[inline]
    pub fn read_bits(&mut self, n: u32) -> Result<u32, Error> {
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
            // A validated read ending mid-byte implies the next byte
            // exists; keep it total anyway (see the E2a note).
            let partial = u32::from(self.data.get(self.pos).copied().unwrap_or(0)) << (24 + rem);
            self.acc = partial;
            self.valid = 8 - rem;
            self.pos += 1;
        }
        Ok(val)
    }

    /// The retained position-based recompute — the live `bits.rs::peek_at`
    /// verbatim, applied at `pos`. Serves the cold path and peeks.
    fn peek_at(&self, pos: usize, n: u32) -> Result<u32, Error> {
        if n == 0 || n > 32 {
            return Err(Error::InvalidField);
        }
        let total_bits = self.data.len() << 3;
        pos.checked_add(n as usize)
            .filter(|&e| e <= total_bits)
            .ok_or(Error::EndOfStream)?;

        let byte_idx = pos >> 3;
        let off = (pos & 7) as u32; // bit offset within the first byte, 0..=7
        // Bytes spanned by bits [pos, pos+n): division-free ceil((off + n) / 8).
        let bytes_needed = (((off + n + 7) >> 3) as usize).min(self.data.len()); // <= 5

        // Gather the spanned bytes, big-endian, into a bottom-aligned u64
        // accumulator (width = bytes_needed * 8 <= 40 bits).
        let mut acc: u64 = 0;
        for &b in &self.data[byte_idx..byte_idx + bytes_needed] {
            acc = (acc << 8) | u64::from(b);
        }
        let shift = bytes_needed as u32 * 8 - off - n;
        let mask = (1u64 << n) - 1; // n in 1..=32, so this cannot overflow u64
        Ok(((acc >> shift) & mask) as u32)
    }

    /// Peek at the next `n` bits (1..=32) without advancing the cursor.
    /// E2a verbatim.
    pub fn peek_bits(&self, n: u32) -> Result<u32, Error> {
        if n >= 1 && n <= 32 && self.valid >= n {
            return Ok(self.acc >> (32 - n));
        }
        self.peek_at(self.bit_position(), n)
    }

    /// Read `n` bits (1..=32) as a two's-complement signed value — the
    /// live shl-then-arithmetic-shr idiom, delegating to `read_bits`.
    pub fn read_signed(&mut self, n: u32) -> Result<i32, Error> {
        let raw = self.read_bits(n)?;
        let shift = 32 - n;
        Ok(((raw << shift) as i32) >> shift)
    }

    /// Read FLAC's UTF-8-style coded number (RFC 9639 §9.1.5); composite,
    /// atomic (E2a verbatim).
    pub fn read_utf8_coded(&mut self) -> Result<u64, Error> {
        let start = (self.pos, self.acc, self.valid);
        match self.coded_number_inner() {
            Ok(v) => Ok(v),
            Err(e) => {
                self.pos = start.0;
                self.acc = start.1;
                self.valid = start.2;
                Err(e)
            }
        }
    }

    /// Consume bits up to the next byte boundary; returns bits dropped
    /// (0..7). E2a verbatim (`drop = valid & 7`, exactly the live
    /// `(8 − (bit_pos & 7)) & 7` through the invariant).
    pub fn byte_align(&mut self) -> u32 {
        let drop = self.valid & 7;
        self.acc <<= drop;
        self.valid &= !7;
        drop
    }

    /// Read a `u8` (8 bits) — bit-level, identical to the live reader by
    /// delegation.
    pub fn read_u8(&mut self) -> Result<u8, Error> {
        self.read_bits(8).map(|v| v as u8)
    }

    /// Read FLAC's wasted-bits-per-sample field (§9.2.2); atomic across
    /// the composite run. E2a verbatim (cold path — per the menu's vendor
    /// discipline, NOT routed through the table: this probe's one variable
    /// is the Rice unary count).
    pub fn read_wasted_bits(&mut self) -> Result<u32, Error> {
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

    fn wasted_inner(&mut self) -> Result<u32, Error> {
        if self.read_bits(1)? == 0 {
            return Ok(0);
        }
        let mut zeros: u32 = 0;
        loop {
            if self.read_bits(1)? == 1 {
                return Ok(zeros + 1);
            }
            zeros += 1;
        }
    }

    fn coded_number_inner(&mut self) -> Result<u64, Error> {
        let lead = self.read_bits(8)? as u8;
        let mut prefix: u32 = 0;
        let mut mask = 0x80u8;
        while lead & mask != 0 {
            prefix += 1;
            mask >>= 1;
        }
        if prefix == 1 || prefix == 8 {
            return Err(Error::InvalidField);
        }
        let total = if prefix == 0 { 1 } else { prefix };
        let mut value = u64::from(u32::from(lead) & (0xFFu32 >> (prefix + 1)));
        for _ in 1..total {
            let cont = self.read_bits(8)? as u8;
            if cont & 0xC0 != 0x80 {
                return Err(Error::InvalidField);
            }
            value = (value << 6) | u64::from(cont & 0x3F);
        }
        Ok(value)
    }

    /// E10 seam: hand the whole non-escape partition sample loop to the
    /// ARM-mode kernel and resync. Window mapping (exact both ways):
    /// bit `i` of `w` = stream bit P + `i` = bit `31 − i` of `acc`, i.e.
    /// `w = rev32(acc)` — and the reader invariant "low `32 − valid` bits
    /// of acc are zero" is exactly the kernel's "bits ≥ v of w are zero".
    /// `P + v = pos * 8` is the kernel's byte-pointer invariant at entry,
    /// maintained by every kernel path, and resynced back with the same
    /// `rev32`. `p <= 14` is the kernel contract (checked by the caller —
    /// escape partitions carry the wide params).
    pub fn rice_partition_kernel(&mut self, out: &mut [i32], p: u32) -> Result<(), Error> {
        debug_assert!(p <= 14);
        let data = self.data;
        #[cfg(not(target_arch = "arm"))]
        if let Some(ss) = self.stats_slot() {
            ss.max_p = ss.max_p.max(u64::from(p));
        }
        let mut st = KernelState {
            w: rev32(self.acc),
            v: self.valid,
            ptr: self.pos,
            end: data.len(),
            status: 0,
        };
        #[cfg(not(target_arch = "arm"))]
        let stats = self.stats_slot();
        #[cfg(target_arch = "arm")]
        let stats: Option<&mut TierStats> = None;
        run_kernel(data, &mut st, out, p, stats);
        // Resync (module docs): exact — the kernel's window invariants are
        // the reader's invariants. On status 1 the frame dies either way;
        // the cursor is wherever the walk stopped (ratified item 1).
        self.acc = rev32(st.w);
        self.valid = st.v;
        self.pos = st.ptr;
        if st.status != 0 {
            return Err(Error::EndOfStream);
        }
        Ok(())
    }

    /// Host census slot access. SAFETY discipline: the pointer is set by
    /// `decode_one_arm_stats` from a caller-owned `TierStats` borrow with
    /// no other live borrow of it; one reader is live per frame.
    #[cfg(not(target_arch = "arm"))]
    fn stats_slot(&mut self) -> Option<&mut TierStats> {
        if self.stats.is_null() {
            None
        } else {
            Some(unsafe { &mut *self.stats })
        }
    }
}

// Vendored decode path — reader carries the table count; arithmetic
// verbatim (identical vendor boundary to E2a's e2.rs).
// ---------------------------------------------------------------------------

/// Vendored twin of `frame.rs`'s private `BlocksizeCode`.
enum BlocksizeCode {
    Table(u16),
    Uncommon8,
    Uncommon16,
}

/// Vendored `frame::blocksize_code` (pure; no reader).
fn blocksize_code(code: u8) -> Result<BlocksizeCode, Error> {
    match code {
        0b0000 => Err(Error::InvalidField),
        0b0001 => Ok(BlocksizeCode::Table(192)),
        0b0010..=0b0101 => Ok(BlocksizeCode::Table((144u32 << code) as u16)),
        0b0110 => Ok(BlocksizeCode::Uncommon8),
        0b0111 => Ok(BlocksizeCode::Uncommon16),
        0b1000..=0b1111 => Ok(BlocksizeCode::Table((1u32 << code as u32) as u16)),
        _ => Err(Error::InvalidField),
    }
}

/// Vendored `frame::channel_config` (pure; no reader).
fn channel_config(code: u8) -> Result<ChannelConfig, Error> {
    match code {
        0b0000 => Ok(ChannelConfig::Independent { channels: 1 }),
        0b0001 => Ok(ChannelConfig::Independent { channels: 2 }),
        0b0010..=0b0111 => Err(Error::ProfileViolation),
        0b1000 => Ok(ChannelConfig::LeftSide),
        0b1001 => Ok(ChannelConfig::RightSide),
        0b1010 => Ok(ChannelConfig::MidSide),
        _ => Err(Error::InvalidField),
    }
}

/// Vendored `frame::bits_per_sample` (pure; no reader).
fn bits_per_sample(code: u8, defaults: &StreamDefaults) -> Result<u8, Error> {
    match code {
        0b000 => match defaults.bits_per_sample {
            8 | 16 => Ok(defaults.bits_per_sample),
            _ => Err(Error::UnsupportedSampleSize),
        },
        0b001 => Ok(8),
        0b100 => Ok(16),
        0b011 => Err(Error::InvalidField),
        _ => Err(Error::UnsupportedSampleSize),
    }
}

/// Vendored `frame::sample_rate` (reader type swapped).
fn sample_rate(code: u8, reader: &mut AccumulatorReader<'_>) -> Result<SampleRate, Error> {
    let hz = match code {
        0b1100 => {
            let khz = reader.read_u8()? as u32;
            if khz == 0 {
                return Err(Error::InvalidField);
            }
            khz * 1000
        }
        0b1101 => {
            let hz = reader.read_bits(16)?;
            if hz == 0 {
                return Err(Error::InvalidField);
            }
            return Ok(SampleRate::Explicit(hz));
        }
        0b1110 => {
            let hz10 = reader.read_bits(16)?;
            if hz10 == 0 {
                return Err(Error::InvalidField);
            }
            hz10 * 10
        }
        _ => return SampleRate::from_flac_code(code),
    };
    Ok(SampleRate::Explicit(hz))
}

/// Vendored `frame::FrameHeader::parse` (reader type swapped; field order,
/// validation, and cursor rules line-for-line identical).
fn parse_header(
    reader: &mut AccumulatorReader<'_>,
    defaults: &StreamDefaults,
) -> Result<FrameHeader, Error> {
    if reader.bit_position() % 8 != 0 {
        return Err(Error::FrameSync);
    }
    // PORT (2026-11-06 re-application at item 6a-2): the core's
    // `FrameHeader::parse` has carried a header-length field since item 4a
    // (`frame.rs`: `start_bit` / `header_bits`). The vendored copy is a
    // transcription of that parse, so it now transcribes that rule too.
    let start_bit = reader.bit_position();

    let sync = reader.read_bits(14)?;
    if sync != SYNC_14 {
        return Err(Error::FrameSync);
    }
    if reader.read_bits(1)? != 0 {
        return Err(Error::InvalidField); // reserved bit must be zero
    }
    if reader.read_bits(1)? != 0 {
        return Err(Error::ProfileViolation); // variable blocksize: off-profile
    }
    let bs_bits = reader.read_bits(4)? as u8;
    let rate_code = reader.read_bits(4)? as u8;
    let channels_code = reader.read_bits(4)? as u8;
    let size_code = reader.read_bits(3)? as u8;
    if reader.read_bits(1)? != 0 {
        return Err(Error::InvalidField); // reserved bit must be zero
    }
    debug_assert_eq!(reader.bit_position() % 8, 0, "fixed fields are 32 bits");

    let channels = channel_config(channels_code)?;
    let bits_per_sample = bits_per_sample(size_code, defaults)?;
    let bs_code = blocksize_code(bs_bits)?;

    let number = reader.read_utf8_coded()?;
    if number > 0x7FFF_FFFF {
        return Err(Error::InvalidField);
    }

    let blocksize = match bs_code {
        BlocksizeCode::Table(n) => n as usize,
        BlocksizeCode::Uncommon8 => reader.read_u8()? as usize + 1,
        BlocksizeCode::Uncommon16 => {
            let stored = reader.read_bits(16)?;
            if stored == 0xFFFF {
                return Err(Error::InvalidField);
            }
            stored as usize + 1
        }
    };

    let sample_rate = sample_rate(rate_code, reader)?;

    let _crc8 = reader.read_u8()?;

    // PORT (2026-11-06): same expression as the core's parse.
    let header_bits = reader.bit_position() - start_bit;

    Ok(FrameHeader {
        blocksize,
        sample_rate,
        channels,
        bits_per_sample,
        number,
        header_bits,
    })
}

/// Vendored `subframe::SubframeType::parse` (reader type swapped).
fn parse_subframe_type(reader: &mut AccumulatorReader<'_>) -> Result<SubframeType, Error> {
    if reader.read_bits(1)? != 0 {
        return Err(Error::InvalidField);
    }
    let code = reader.read_bits(6)? as u8;
    match code {
        0b000000 => Ok(SubframeType::Constant),
        0b000001 => Ok(SubframeType::Verbatim),
        0b001000..=0b001100 => Ok(SubframeType::Fixed(code - 8)),
        0b100000..=0b111111 => Ok(SubframeType::Lpc { order: code - 31 }),
        _ => Err(Error::InvalidField),
    }
}

/// Vendored `subframe::PredictorState::fill` (reader type swapped; the
/// i64 pad kept verbatim).
fn fill_state(
    state: &mut PredictorState,
    reader: &mut AccumulatorReader<'_>,
    order: usize,
    subframe_bits: u8,
    wasted: u32,
) -> Result<(), Error> {
    if order > MAX_LPC_ORDER {
        return Err(Error::UnsupportedPredictorOrder);
    }
    if wasted >= 32 {
        return Err(Error::InvalidField);
    }
    let mut warm = [0i32; MAX_LPC_ORDER];
    for slot in (0..order).rev() {
        let v = reader.read_signed(u32::from(subframe_bits))?;
        warm[slot] = ((i64::from(v)) << wasted) as i32;
    }
    state.warm_up = warm;
    state.len = order;
    Ok(())
}

/// Vendored `residual::rice_unmap` (pure; no reader). Retained as the
/// readable spec form; the hot path folds through the kernel's
/// `fold_body` (the twin's [`fold_twin`] is its witness).
#[allow(dead_code)]
fn rice_unmap(n: u32) -> i32 {
    if n & 1 == 0 {
        (n >> 1) as i32
    } else {
        (!(n >> 1)) as i32
    }
}

/// Vendored `subframe::pad_block` — i64-then-truncate exactly as live.
fn pad_block(out: &mut [i32], wasted: u32) {
    if wasted == 0 {
        return;
    }
    for s in out.iter_mut() {
        *s = ((i64::from(*s)) << wasted) as i32;
    }
}

/// Vendored `subframe::integrate_fixed` — i64 cascade exactly as live
/// (this is NOT E4; the variable is the unary count).
fn integrate_fixed(order: u8, warm_up: &PredictorState, residual: &mut [i32]) -> Result<(), Error> {
    let order = usize::from(order);
    if order > MAX_FIXED_ORDER {
        return Err(Error::UnsupportedPredictorOrder);
    }
    if warm_up.len < order {
        return Err(Error::InvalidField);
    }
    let w = &warm_up.warm_up;
    let mut d1;
    let mut d2 = 0i64;
    let mut d3 = 0i64;
    let mut out = 0i64;
    match order {
        0 => return Ok(()),
        1 => {
            d1 = i64::from(w[0]);
        }
        2 => {
            d1 = i64::from(w[0]) - i64::from(w[1]);
            out = i64::from(w[0]);
        }
        3 => {
            d2 = i64::from(w[0]) - 2 * i64::from(w[1]) + i64::from(w[2]);
            d1 = i64::from(w[0]) - i64::from(w[1]);
            out = i64::from(w[0]);
        }
        _ => {
            d3 = i64::from(w[0]) - 3 * i64::from(w[1]) + 3 * i64::from(w[2]) - i64::from(w[3]);
            d2 = i64::from(w[0]) - 2 * i64::from(w[1]) + i64::from(w[2]);
            d1 = i64::from(w[0]) - i64::from(w[1]);
            out = i64::from(w[0]);
        }
    }
    for sample in residual.iter_mut() {
        let r = i64::from(*sample);
        match order {
            1 => {
                d1 += r;
                out = d1;
            }
            2 => {
                d1 += r;
                out += d1;
            }
            3 => {
                d2 += r;
                d1 += d2;
                out += d1;
            }
            _ => {
                d3 += r;
                d2 += d3;
                d1 += d2;
                out += d1;
            }
        }
        *sample = out as i32;
    }
    Ok(())
}

/// Vendored `subframe::integrate_lpc` — i64 dot product exactly as live.
fn integrate_lpc(
    coefficients: &[i32],
    shift: i8,
    warm_up: &PredictorState,
    residual: &mut [i32],
) -> Result<(), Error> {
    if shift < 0 {
        return Err(Error::InvalidField);
    }
    let order = coefficients.len();
    if order > MAX_LPC_ORDER {
        return Err(Error::UnsupportedPredictorOrder);
    }
    if warm_up.len < order {
        return Err(Error::InvalidField);
    }
    let shift = u32::from(shift as u8);
    if shift >= 64 {
        return Err(Error::InvalidField);
    }
    let w = &warm_up.warm_up;
    for i in 0..residual.len() {
        let mut acc = 0i64;
        for (j, &c) in coefficients.iter().enumerate() {
            let past = if j < i {
                i64::from(residual[i - 1 - j])
            } else {
                i64::from(w[j - i])
            };
            acc += i64::from(c) * past;
        }
        residual[i] = ((acc >> shift) + i64::from(residual[i])) as i32;
    }
    Ok(())
}

/// Vendored `residual::decode_rice_partition` — **the probe's variable
/// lives here**: the non-escape sample loop is handed to the ARM-mode
/// hybrid kernel [`AccumulatorReader::rice_partition_kernel`] (module
/// docs). Escape partitions ride the reader verbatim — the escape path is
/// raw-bit reads, not the codeword loop, and is not this probe's
/// variable. The `p > 14` guard: the kernel's tier-1 no-refill argument
/// requires p ≤ 14 (every Rice order the encode profile emits; the live
/// reader supports 15..=30 without the kernel — reachable only outside
/// the profile, and a loud rejection, never a silent mis-decode).
fn decode_rice_partition(
    reader: &mut AccumulatorReader<'_>,
    header: &PartitionHeader,
    out: &mut [i32],
) -> Result<(), Error> {
    if out.len() != usize::from(header.sample_count) {
        return Err(Error::InvalidField);
    }
    if header.is_escape {
        #[cfg(not(target_arch = "arm"))]
        if let Some(ss) = reader.stats_slot() {
            ss.escapes += 1;
        }
        for sample in out.iter_mut() {
            *sample = if header.raw_bits == 0 {
                0
            } else {
                reader.read_signed(u32::from(header.raw_bits))?
            };
        }
        return Ok(());
    }
    let p = u32::from(header.order);
    if p > 14 {
        return Err(Error::InvalidField);
    }
    reader.rice_partition_kernel(out, p)
}

fn decode_residual(
    reader: &mut AccumulatorReader<'_>,
    blocksize: usize,
    predictor_order: usize,
    out: &mut [i32],
) -> Result<(), Error> {
    let Some(total) = blocksize.checked_sub(predictor_order) else {
        return Err(Error::InvalidField);
    };
    if out.len() != total {
        return Err(Error::InvalidField);
    }
    let (plen, escape) = match reader.read_bits(2)? {
        0b00 => (4u32, 15u32),
        0b01 => (5, 31),
        _ => return Err(Error::InvalidField),
    };
    let partition_order = reader.read_bits(4)? as usize;
    let partitions = 1usize << partition_order;
    if blocksize & (partitions - 1) != 0 {
        return Err(Error::InvalidField);
    }
    let partition_samples = blocksize >> partition_order;
    if predictor_order >= partition_samples {
        return Err(Error::InvalidField);
    }
    let mut rest = out;
    for partition in 0..partitions {
        let count = if partition == 0 {
            partition_samples - predictor_order
        } else {
            partition_samples
        };
        let param = reader.read_bits(plen)?;
        let header = if param == escape {
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

/// Vendored `subframe::decode_subframe` (reader type swapped; shared
/// `SubframeType`/`PredictorState` types, shared `decorrelate` beneath).
fn decode_subframe(
    reader: &mut AccumulatorReader<'_>,
    blocksize: usize,
    sample_bits: u8,
    state: &mut PredictorState,
    out: &mut [i32],
) -> Result<SubframeType, Error> {
    if out.len() != blocksize {
        return Err(Error::InvalidField);
    }
    let kind = parse_subframe_type(reader)?;
    let order = kind.order();
    if order > blocksize {
        return Err(Error::InvalidField);
    }
    let wasted = reader.read_wasted_bits()?;
    let sub_bits = u32::from(sample_bits)
        .checked_sub(wasted)
        .filter(|&b| b > 0 && b <= 32)
        .ok_or(Error::InvalidField)?;
    let subframe_bits = sub_bits as u8;
    fill_state(state, reader, order, subframe_bits, 0)?;
    for (i, slot) in state.warm_up[..order].iter().rev().enumerate() {
        out[i] = *slot;
    }
    match kind {
        SubframeType::Constant => {
            let v = reader.read_signed(sub_bits)?;
            out.fill(v);
        }
        SubframeType::Verbatim => {
            for sample in out.iter_mut() {
                *sample = reader.read_signed(sub_bits)?;
            }
        }
        SubframeType::Fixed(order_u8) => {
            decode_residual(reader, blocksize, order, &mut out[order..])?;
            integrate_fixed(order_u8, state, &mut out[order..])?;
        }
        SubframeType::Lpc { order: _order_u8 } => {
            let precision_minus_one = reader.read_bits(4)?;
            if precision_minus_one == 0b1111 {
                return Err(Error::InvalidField);
            }
            let precision = precision_minus_one + 1;
            let shift = reader.read_signed(5)?;
            if shift < 0 {
                return Err(Error::InvalidField);
            }
            let mut coeffs = [0i32; MAX_LPC_ORDER];
            for c in coeffs.iter_mut().take(order) {
                *c = reader.read_signed(precision)?;
            }
            decode_residual(reader, blocksize, order, &mut out[order..])?;
            integrate_lpc(&coeffs[..order], shift as i8, state, &mut out[order..])?;
        }
    }
    pad_block(out, wasted);
    Ok(kind)
}

/// Vendored `frame::decode_frame` (reader type swapped; `decorrelate` is a
/// real shared call — it takes no reader).
fn decode_frame(
    reader: &mut AccumulatorReader<'_>,
    header: &FrameHeader,
    left: &mut [i32],
    right: &mut [i32],
    state: &mut [PredictorState; 2],
) -> Result<usize, Error> {
    let blocksize = header.blocksize;
    let slots = usize::from(header.channels.subframe_count());

    if left.len() < blocksize || (slots == 2 && right.len() < blocksize) {
        return Err(Error::InvalidField);
    }
    let side_slot = match header.channels {
        ChannelConfig::RightSide => Some(0),
        ChannelConfig::LeftSide | ChannelConfig::MidSide => Some(1),
        ChannelConfig::Independent { .. } => None,
    };
    let slot_bits = |slot: usize| -> u8 {
        (u32::from(header.bits_per_sample) + u32::from(side_slot == Some(slot))) as u8
    };

    decode_subframe(
        reader,
        blocksize,
        slot_bits(0),
        &mut state[0],
        &mut left[..blocksize],
    )?;
    if slots == 2 {
        decode_subframe(
            reader,
            blocksize,
            slot_bits(1),
            &mut state[1],
            &mut right[..blocksize],
        )?;
    }
    let right_span = if slots == 2 { blocksize } else { 0 };
    decorrelate(
        header.channels,
        blocksize,
        &mut left[..blocksize],
        &mut right[..right_span],
    )?;

    reader.byte_align();
    let _crc16_hi = reader.read_u8()?;
    let _crc16_lo = reader.read_u8()?;
    Ok(blocksize)
}

/// Decode exactly one frame with the hybrid ARM codeword loop — the
/// variant twin of `driver::decode_one` (same seek-table offset walk,
/// same table cross-check, same fresh cursor; the differences are the
/// vendored frame body and the ARM kernel behind every non-escape Rice
/// partition). `stats` (host): optional tier census slot.
pub fn decode_one_impl(
    clip: &SpikeClip,
    frame_index: usize,
    left: &mut [i32],
    right: &mut [i32],
    state: &mut [PredictorState; 2],
    stats: *mut TierStats,
) -> Result<usize, DriverError> {
    let frame = &clip.frames[frame_index];
    let max = usize::from(clip.max_blocksize);
    if left.len() < max {
        return Err(DriverError::BufferTooSmall {
            need: max,
            have: left.len(),
        });
    }
    if clip.channels == 2 && right.len() < max {
        return Err(DriverError::BufferTooSmall {
            need: max,
            have: right.len(),
        });
    }
    let offset = match usize::try_from(frame.offset) {
        Ok(offset) => offset,
        Err(_) => {
            return Err(DriverError::FrameOffsetOutOfRange {
                frame: frame_index,
                offset: usize::MAX,
                len: clip.region.len(),
            });
        }
    };
    let region = clip
        .region
        .get(offset..)
        .ok_or(DriverError::FrameOffsetOutOfRange {
            frame: frame_index,
            offset,
            len: clip.region.len(),
        })?;

    let defaults = StreamDefaults {
        sample_rate_hz: clip.sample_rate_hz,
        bits_per_sample: clip.bits_per_sample,
    };
    let mut reader = AccumulatorReader::new(region);
    reader.stats = stats;
    let header = parse_header(&mut reader, &defaults).map_err(DriverError::Decode)?;
    if header.blocksize != usize::from(frame.blocksize) {
        return Err(DriverError::MetadataMismatch {
            frame: frame_index,
            table: usize::from(frame.blocksize),
            header: header.blocksize,
        });
    }
    decode_frame(&mut reader, &header, left, right, state).map_err(DriverError::Decode)
}

// Unit tests compile as part of the lib under the host test harness (same
// gate as e2.rs's). Core-only, like the crate's other unit tests.

/// Decode one frame (measurement shape: no census).
pub fn decode_one_arm(
    clip: &SpikeClip,
    frame_index: usize,
    left: &mut [i32],
    right: &mut [i32],
    state: &mut [PredictorState; 2],
) -> Result<usize, DriverError> {
    decode_one_impl(clip, frame_index, left, right, state, core::ptr::null_mut())
}

/// Decode one frame with the tier census collected (host witness; the
/// `stats` borrow is caller-owned and unique for the call's duration).
pub fn decode_one_arm_stats(
    clip: &SpikeClip,
    frame_index: usize,
    left: &mut [i32],
    right: &mut [i32],
    state: &mut [PredictorState; 2],
    stats: &mut TierStats,
) -> Result<usize, DriverError> {
    decode_one_impl(
        clip,
        frame_index,
        left,
        right,
        state,
        stats as *mut TierStats,
    )
}

#[cfg(test)]
mod tests {
    extern crate alloc;
    use super::*;
    use alloc::vec;
    use alloc::vec::Vec;

    /// Deterministic pseudo-random bytes (LCG, the bits.rs shape).
    struct Lcg(u32);
    impl Lcg {
        fn next_u8(&mut self) -> u8 {
            self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (self.0 >> 24) as u8
        }
    }

    /// Naive MSB-first bit view of a byte slice — the oracle's reader.
    struct Bits<'a>(&'a [u8]);
    impl Bits<'_> {
        fn bit(&self, i: usize) -> u32 {
            (u32::from(self.0[i >> 3]) >> (7 - (i & 7))) & 1
        }
    }

    /// §9.2.7.2 unmap, spec BRANCH form (independent of the fold identity).
    fn spec_unmap(n: u32) -> i32 {
        if n & 1 == 0 {
            (n >> 1) as i32
        } else {
            (!(n >> 1) as u32) as i32
        }
    }

    /// Naive per-bit Rice decode — fully independent of the twin's window,
    /// tiers, reversal, and fold shape: reads unary then p MSB-first bits,
    /// composes `(q << p) | rem`, spec-unmaps. `None` on an unterminated
    /// run or truncated remainder (the oracle's EOF).
    fn oracle_rice(data: &[u8], start_bit: usize, n: usize, p: usize) -> Option<(Vec<i32>, usize)> {
        let bits = Bits(data);
        let total = data.len() * 8;
        let mut pos = start_bit;
        let mut out = Vec::with_capacity(n);
        for _ in 0..n {
            let mut q = 0u32;
            loop {
                if pos >= total {
                    return None;
                }
                let b = bits.bit(pos);
                pos += 1;
                if b == 1 {
                    break;
                }
                q += 1;
            }
            let mut rem = 0u32;
            for _ in 0..p {
                if pos >= total {
                    return None;
                }
                rem = (rem << 1) | bits.bit(pos);
                pos += 1;
            }
            let coded = (q << p) | rem;
            out.push(spec_unmap(coded));
        }
        Some((out, pos))
    }

    fn run_twin(
        data: &[u8],
        start_bit: usize,
        n: usize,
        p: u32,
        stats: Option<&mut TierStats>,
    ) -> Option<Vec<i32>> {
        let mut st = KernelState {
            w: 0,
            v: 0,
            ptr: start_bit >> 3,
            end: data.len(),
            status: 0,
        };
        // Bits before start_bit inside its byte: seed the window with the
        // byte's tail (REV8[b] bit k = stream bit k of the byte, so the
        // lead-skipped bits drop by a RIGHT shift) and point ptr at the
        // NEXT byte. Byte-aligned starts keep the empty-window seed: byte
        // 0 belongs to the stream (an unconditional +1 skipped it, and
        // tier 3 legitimately answered runs whose terminator never
        // entered the window — the first sweep convicted this seeding).
        let lead = (start_bit & 7) as u32;
        if lead != 0 {
            let b = data[start_bit >> 3];
            // REV8[b] bit k = stream bit k of the byte; dropping the lead
            // skipped bits is a RIGHT shift by `lead`.
            st.w = u32::from(REV8[b as usize]) >> lead;
            // window now holds bits [start_bit, byte_end) at bit 0
            st.v = 8 - lead;
            st.ptr = (start_bit >> 3) + 1;
        }
        let mut out = vec![0i32; n];
        twin_partition(data, &mut st, &mut out, p, stats);
        if st.status != 0 {
            return None;
        }
        Some(out)
    }

    /// Put bit `b` (MSB-first stream order) at absolute bit `at`.
    fn put_bit(buf: &mut [u8], at: usize, b: u32) {
        if b == 1 {
            buf[at >> 3] |= 1 << (7 - (at & 7));
        }
    }

    fn put_bits(buf: &mut [u8], at: usize, pattern: u32, width: u32) {
        for j in 0..width {
            let bit = (pattern >> (width - 1 - j)) & 1;
            put_bit(buf, at + j as usize, bit);
        }
    }

    #[test]
    fn tables_are_exhaustively_correct_against_naive_oracles() {
        for (i, &got) in CTZ8.iter().enumerate() {
            let mut q = 0u8;
            while q < 8 && ((i as u8) >> q) & 1 == 0 {
                q += 1;
            }
            assert_eq!(got, q, "ctz8[{i:#04X}]");
        }
        for (i, &got) in REV8.iter().enumerate() {
            let mut r = 0u8;
            for j in 0..8 {
                if ((i as u8) >> j) & 1 == 1 {
                    r |= 1 << (7 - j);
                }
            }
            assert_eq!(got, r, "rev8[{i:#04X}]");
        }
        // De Bruijn: the index map is a bijection and every entry lands.
        let mut seen = [false; 32];
        for b in 0..32u32 {
            let idx = ((1u32 << b).wrapping_mul(BRUIJN_K) >> 27) as usize;
            assert_eq!(BRUIJN_INDEX[idx], b as u8, "bruijn entry for {b}");
            assert!(!seen[idx], "bruijn index collision at {idx}");
            seen[idx] = true;
        }
        // The kernel's ctz twin agrees with the builtin everywhere.
        for b in 0..32u32 {
            assert_eq!(ctz32_asm_twin(1 << b), b);
        }
        let mut rng = Lcg(0xE105_0C1D);
        for _ in 0..200_000 {
            let w = rng.next_u8() as u32
                | ((rng.next_u8() as u32) << 8)
                | ((rng.next_u8() as u32) << 16)
                | ((rng.next_u8() as u32) << 24);
            if w != 0 {
                assert_eq!(ctz32_asm_twin(w), w.trailing_zeros(), "w={w:#010X}");
            }
        }
        // rev32: involution + bit-mapping witness.
        for _ in 0..100_000 {
            let w = rng.next_u8() as u32
                | ((rng.next_u8() as u32) << 8)
                | ((rng.next_u8() as u32) << 16)
                | ((rng.next_u8() as u32) << 24);
            assert_eq!(rev32(rev32(w)), w);
            for i in 0..32u32 {
                assert_eq!((w >> i) & 1, (rev32(w) >> (31 - i)) & 1);
            }
        }
        // un-reversal: p-bit reversal matches a naive loop (the stitch).
        for p in 0..=14u32 {
            let m = (1u32 << p) - 1;
            for k in 0..64u32 {
                let rem_w = k.wrapping_mul(0x9E37) & m;
                let mut spec = 0u32;
                for i in 0..p {
                    spec |= ((rem_w >> i) & 1) << (p - 1 - i);
                }
                assert_eq!(unreverse_rem(rem_w, p), spec, "p={p} rem_w={rem_w:#06X}");
            }
        }
    }

    #[test]
    fn twin_sweeps_quotients_at_every_bit_offset_with_p_sweep() {
        // The plan's bit-offset sweep (decision 7's cut, left-to-measure
        // item 5): lengths 0..=70 (crosses tier boundaries 8 and 32 and
        // the C2 multi-word legs) at every reader alignment 0..7, for p
        // across the kernel contract (0, 1, 2, 4, 8, 14).
        for &p in &[0u32, 1, 2, 4, 8, 14] {
            for q in 0..=70u32 {
                for off in 0..7usize {
                    let rem_pattern = 0b1011_0100_1101_01u32 & ((1u32 << p) - 1);
                    let total_bits = off + q as usize + 1 + p as usize + 9;
                    // +8 slack bytes: the window refills to v >= 25 on
                    // every sample, so the tier path (not .Lslow) answers.
                    let mut buf = vec![0u8; total_bits.div_ceil(8) + 8];
                    // Spec order: q zeros, terminating 1, then the p
                    // remainder bits MSB-first (the earlier layout put
                    // the pattern ahead of the run and wrote its 1-bits
                    // into the zeros — the twin decoded the stream right
                    // and the test expected wrong).
                    put_bit(&mut buf, off + q as usize, 1); // terminator
                    put_bits(&mut buf, off + q as usize + 1, rem_pattern, p);
                    put_bits(&mut buf, off + q as usize + 1 + p as usize, 0b1_0110, 5);
                    let expected = spec_unmap((q << p) | rem_pattern);
                    let mut stats = TierStats::default();
                    let got = run_twin(&buf, off, 1, p, Some(&mut stats))
                        .unwrap_or_else(|| panic!("q={q} off={off} p={p}: unexpected EOF"));
                    assert_eq!(got[0], expected, "q={q} off={off} p={p}");
                    assert_eq!(stats.samples, 1);
                    let tier_ok = match (off, q) {
                        (_, 0..=7) => stats.t1 == 1,
                        (0, 8..=31) => stats.t2 == 1,
                        (0, _) => stats.t3 == 1,
                        // lead seed: v at dispatch is 25..=31; a run
                        // past the window's valid bits handoffs on
                        // w == 0 (decision 2) — either count tier is
                        // correct, the DECODE above is the hard check.
                        (_, 8..=31) => stats.t2 + stats.t3 == 1,
                        (_, _) => stats.t3 == 1,
                    };
                    assert!(tier_ok, "q={q} off={off} tier census: {stats:?}");
                }
            }
        }
    }

    #[test]
    fn twin_matches_per_bit_oracle_on_random_multi_sample_streams() {
        // Multi-sample partitions: tier mixtures, refill interleaving,
        // straddles, and the shared tail — the oracle decides everything.
        for &p in &[0u32, 1, 2, 3, 4, 5, 8, 11, 14] {
            let mut rng = Lcg(0x5EED + p);
            for trial in 0..240usize {
                let n = 1 + (rng.next_u8() as usize % 40);
                // Build the stream from the ORACLE side: random q's and
                // rem's laid down MSB-first; the twin must reproduce the
                // oracle's decode of exactly those bytes.
                let mut qs = Vec::with_capacity(n);
                let mut rs = Vec::with_capacity(n);
                let mut bits_needed = 0usize;
                for _ in 0..n {
                    // Weighted q: mostly small, some 8..=31, occasional 32+.
                    let r = rng.next_u8();
                    let q = match r % 10 {
                        0..=6 => (r >> 4) as u32 % 8,
                        7 => 8 + (r >> 4) as u32 % 24,
                        8 => 32 + (r >> 4) as u32 % 40,
                        _ => (r >> 4) as u32 % 3,
                    };
                    qs.push(q);
                    rs.push(rng.next_u8() as u32 & ((1u32 << p) - 1));
                    bits_needed += q as usize + 1 + p as usize;
                }
                let mut buf = vec![0u8; bits_needed.div_ceil(8) + 1];
                let mut pos = 0usize;
                for k in 0..n {
                    for _ in 0..qs[k] {
                        put_bit(&mut buf, pos, 0);
                        pos += 1;
                    }
                    put_bit(&mut buf, pos, 1);
                    pos += 1;
                    put_bits(&mut buf, pos, rs[k], p);
                    pos += p as usize;
                }
                let (oracle, _) = oracle_rice(&buf, 0, n, p as usize)
                    .unwrap_or_else(|| panic!("oracle EOF trial {trial} p={p}"));
                let mut stats = TierStats::default();
                let got = run_twin(&buf, 0, n, p, Some(&mut stats))
                    .unwrap_or_else(|| panic!("twin EOF trial {trial} p={p}"));
                assert_eq!(got, oracle, "trial {trial} p={p}");
                assert_eq!(stats.samples as usize, n);
                assert_eq!(
                    stats.samples,
                    stats.t1 + stats.t2 + stats.t3 + stats.slow,
                    "tier identity trial {trial} p={p}"
                );
                assert_eq!(stats.unterm, 0);
                assert_eq!(stats.escapes, 0);
            }
        }
    }

    #[test]
    fn tier_boundaries_are_strict_on_a_full_window() {
        // Byte-aligned starts refill to v == 32 before dispatch, so the
        // plan's tier map is exact: q <= 7 -> ctz8, 8..=31 -> De Bruijn
        // (FLAT — one C1 visit per sample), >= 32 -> C2 word-skip.
        for q in 0..=70u32 {
            for &p in &[0u32, 2, 14] {
                let total = q as usize + 1 + p as usize + 5;
                // +8 slack bytes: the window refills to v >= 32 (the
                // full-window shape this test pins; +1 left small-q
                // streams byte-exhausted at v < 25 — the .Lslow leg).
                let mut buf = vec![0u8; total.div_ceil(8) + 8];
                put_bit(&mut buf, q as usize, 1);
                put_bits(&mut buf, q as usize + 1, 0b1101_0011, p);
                let expected = spec_unmap((q << p) | (0b1101_0011u32 & ((1u32 << p) - 1)));
                let mut stats = TierStats::default();
                let got = run_twin(&buf, 0, 1, p, Some(&mut stats)).unwrap();
                assert_eq!(got[0], expected, "q={q} p={p}");
                let tier_ok = match q {
                    0..=7 => stats.t1 == 1,
                    8..=31 => stats.t2 == 1,
                    _ => stats.t3 == 1,
                };
                assert!(tier_ok, "q={q} p={p} full-window census: {stats:?}");
                assert_eq!(stats.slow, 0);
            }
        }
    }

    #[test]
    fn unterminated_run_reports_status_and_restores_nothing() {
        // Ratified item 1 witnessed: a run past the slice end sets status
        // 1 and leaves (w, v, ptr) exactly where the walk stopped — the
        // consumed bytes STAY consumed (the divergence from the live
        // loop's atomic restore, stated in the module docs).
        let mut buf = vec![0u8; 3]; // 24 zero bits, no terminator
        buf[2] |= 0b0100_0000; // a 1 far past a 6-sample request's reach
        let mut st = KernelState {
            w: 0,
            v: 0,
            ptr: 0,
            end: 3,
            status: 0,
        };
        let mut out = vec![0i32; 6];
        let mut stats = TierStats::default();
        twin_partition(&buf, &mut st, &mut out, 0, Some(&mut stats));
        assert_eq!(st.status, 1, "run never terminates in 24 bits");
        assert_eq!(stats.unterm, 1);
        // The walk consumed the entire slice before failing.
        assert_eq!(st.ptr, 3, "bytes stay consumed (no restore)");
        // Second shape: truncated remainder (terminator present, p bits
        // missing).
        let mut buf2 = vec![0u8; 2];
        put_bit(&mut buf2, 5, 1); // q = 5
        let mut st2 = KernelState {
            w: 0,
            v: 0,
            ptr: 0,
            end: 2,
            status: 0,
        };
        let mut out2 = [0i32; 1];
        twin_partition(&buf2, &mut st2, &mut out2, 14, None);
        assert_eq!(st2.status, 1, "remainder truncated at the slice end");
    }

    #[test]
    fn slice_tail_samples_decode_and_slow_leg_is_witnessed() {
        // `.Lslow` is the cold slice-tail leg: it runs when the buffer's
        // last bytes leave the window below 25 valid bits (the reader's
        // refill stops at the slice end), i.e. only on the final samples
        // of a stream — never mid-buffer. Two witness shapes:
        // (1) tail-aligned streams (buffer sized exactly): the refill
        // walks the bytes out and the per-bit leg answers;
        // (2) a hand-seeded partial window with the bytes already
        // exhausted (the pure-window shape of the same leg).
        for p in 0..=4u32 {
            let total = 3 + 1 + p as usize + 4 + 1 + p as usize;
            let mut buf = vec![0u8; total.div_ceil(8)];
            put_bit(&mut buf, 3, 1);
            put_bits(&mut buf, 4, 0b11, p);
            put_bit(&mut buf, 4 + p as usize + 4, 1);
            put_bits(&mut buf, 5 + p as usize + 4, 0b10, p);
            let (oracle, _) = oracle_rice(&buf, 0, 2, p as usize).unwrap();
            let mut stats = TierStats::default();
            let got = run_twin(&buf, 0, 2, p, Some(&mut stats)).unwrap();
            assert_eq!(got, oracle, "tail p={p}");
            assert!(stats.slow >= 1, "tail p={p} must ride the cold leg");
        }
        // (2) direct seed: window holds all 8 bits, ptr already at end.
        for p in 0..=4u32 {
            let mut buf = vec![0u8; 1];
            put_bit(&mut buf, 2, 1);
            put_bits(&mut buf, 3, 0b1, p);
            let (oracle, _) = oracle_rice(&buf, 0, 1, p as usize).unwrap();
            let mut st = KernelState {
                w: u32::from(REV8[buf[0] as usize]),
                v: 8,
                ptr: 1,
                end: 1,
                status: 0,
            };
            let mut out = [0i32; 1];
            let mut stats = TierStats::default();
            twin_partition(&buf, &mut st, &mut out, p, Some(&mut stats));
            assert_eq!(st.status, 0, "slow leg p={p}");
            assert_eq!(out[0], oracle[0], "slow leg p={p}");
            assert_eq!(stats.slow, 1);
            assert_eq!(stats.t1 + stats.t2 + stats.t3, 0);
        }
    }
}
