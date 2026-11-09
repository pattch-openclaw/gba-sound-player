//! Split-ladder item 6 PR **6a-4** — the GBA's own Rice kernel, driven through
//! the core's seam from outside `flac-lite`, against the committed corpus the
//! core's own differentials use **plus the boundaries that corpus cannot reach**.
//!
//! The kernel under test is [`flac_lite_gba::rice_arm::ArmRiceKernel`]: the E10
//! ARM-mode partition loop, assembled into `crates/flac-lite-gba`, reached here
//! through nothing but the core's public surface
//! ([`flac_lite::residual::decode_rice_partition_with`],
//! [`flac_lite::bits::BitReader`], [`flac_lite::residual::PartitionHeader`]).
//!
//! # What runs here, and what only the ROM can run
//!
//! There is no ARM executor in this repository (aarch64 host, no
//! `arm-none-eabi-*` binutils, no QEMU — README/agb-build gotchas), so on the
//! host `ArmRiceKernel`'s FFI leg runs
//! [`flac_lite_gba::rice_arm::model_partition`], a label-for-label Rust model of
//! `asm/rice_partition.s`. Read the split honestly:
//!
//! * **this suite** witnesses the wrapper — the window in/out mapping, the
//!   status→`Error` mapping, the domain delegation, both §9.2.7.3 guards, the
//!   end-of-stream contract, the `p = 0` mask, the run-tier *logic* and the
//!   remainder un-reversal *arithmetic* — against an independent value anchor;
//! * **the model** is what is running when this suite is green;
//! * **the assembly** is witnessed by the instrument ROM, which runs the
//!   assembled kernel and the model side by side on mGBA's ARM7TDMI
//!   (`examples/overlay-kernel-check/`, `make overlay-kernel-test`), and by
//!   `objdump -d` of the built ELF for the two standing no-degradation rules
//!   (4-byte ARM encodings; a leaf `pop {…,lr}` + `bx lr`, never `LDM`-to-PC).
//!
//! A green run here therefore does **not** by itself say the `.s` body is correct.
//! It says the contract the `.s` body is written against is exercised, and that a
//! device-side disagreement localizes to the assembly rather than to the
//! algorithm or the ABI. The two halves are joined only in the ROM. That is a
//! property of the platform, not of this suite; 6a-3's record named it in advance
//! ("two halves that cannot be tested together").
//!
//! # The value anchor is a packer, not another decoder
//!
//! Synthesized cases are built by an MSB-first bit packer written from §9.2.7.2's
//! *encoding* text — fold signed→unsigned (`x ≥ 0 → 2x`, `x < 0 → −2x − 1`),
//! unary the quotient, then `order` remainder bits — and the expected sample is
//! the residual that packer was given. Its unfolding is written as the spec's two
//! cases rather than as [`flac_lite::residual::rice_unmap`], which is the
//! arithmetic the production path uses; a shared misreading would survive calling
//! the same function twice, which is why `examples/arm-kernels` keeps its fold
//! twin separate from `residual::rice_unmap` for the same reason. The round trip
//! `spec_unfold(spec_fold(x)) == x` is asserted for every case before any decoder
//! is consulted ([`pack_case`]).
//!
//! Three things are compared per case: the **samples** (against the residuals the
//! case was built from, and against the core's incumbent), the **cursor** (against
//! the bit position the packer knows it wrote, and against the incumbent), and the
//! **error kind**. Three decoders agreeing on samples alone would be a tautology;
//! the packer's values and known bit positions are what make it a measurement.
//! This is the same witness rule the core's own `residual.rs` unit tests state
//! for their packer ("a different mechanism than the decoder's zeros-count
//! loop"), one crate further out.
//!
//! # What the corpus can and cannot witness, counted rather than assumed
//!
//! `the_committed_corpus_still_blinds_the_boundaries_it_was_blinding` measures
//! what the committed vectors actually contain — partitions, parameters, samples,
//! escapes, the largest unary run, whether any codeword comes near either
//! §9.2.7.3 guard — and asserts the blindness the record already claims: no
//! escaped partition, no parameter above 8, no codeword within orders of
//! magnitude of either guard. It is a tripwire, not a boast: widen the corpus and
//! it fails and says so, at which point the synthesized arms stop being the only
//! witness of those boundaries and `flac-docs/open-questions.md` Q10 has to be
//! re-stated. Q10 asked the question; the answer this file gives is that the
//! corpora witness the *map* and not the *boundaries*, so the boundaries come from
//! the packer.
//!
//! # What this file does not do
//!
//! * It moves no default. `residual::decode_residual` still names
//!   [`flac_lite::residual::TableCountKernel`] on one line, and no shipped ROM
//!   links this crate by default. `naming_the_arm_kernel_has_wired_nothing_in`
//!   pins that from outside, including by reading both shipped ROMs' manifests;
//!   the instrument ROM is the one exception and is named as one.
//! * It claims no cycle. E10's −349/−348 cycles per sample was taken in a
//!   different image with a different driver; 6a-4's rule is correctness only and
//!   6a-5 owns the reading.
//! * It does not decode a whole frame or stream through the kernel — nothing above
//!   the seam threads a kernel choice (4a-4/5's own conclusion).
//! * It does not re-anchor the corpus's *sample values* to `flac -d`. That anchor
//!   exists twice already (the core's body-layout tests and this crate's 6a-3
//!   suite, which also does the warm-up and FIXED-order-≤2 arithmetic). Here the
//!   corpus arm is an equality-with-the-incumbent arm, which transfers that
//!   witness rather than repeating it; the non-tautological anchor for *this* PR
//!   is the packer above.
//! * It does not reach a real 5-bit-coded partition (parameters 16..=29 never
//!   occur in libFLAC output, README/flac-format gotcha). The delegation and
//!   guard arms synthesize those parameters.

use flac_lite::Error;
use flac_lite::bits::BitReader;
use flac_lite::residual::{TableCountKernel, decode_rice_partition_with};
use flac_lite_gba::rice_arm::{
    ArmRiceKernel, BRUIJN_INDEX, CTZ8, KernelStatus, MAX_ORDER, REV8, model_partition,
    tables_iwram_addrs,
};

// The case set and the corpus walk are each **one transcription**, `#[path]`-shared
// with the instrument ROM that runs the same code on the device
// (`examples/overlay-kernel-check/`). Both are `alloc`-only sources compiled into
// each consumer separately, so neither becomes an API of this crate: the host test
// gets the same bytes from a std binary, the ROM from a `no_std` one. The data file
// itself is embedded by `corpus.rs`, which is why the path here is the module's and
// not the table's (`tests/twin_from_outside.rs` established that rule).
#[path = "../../../examples/overlay-kernel-check/src/cases.rs"]
mod cases;
#[path = "../../../examples/overlay-kernel-check/src/corpus.rs"]
mod corpus;
use cases::{
    clean_cases, decode_with, eof_cases, guard1_cases, guard2_cases, order_zero_cases,
    out_of_range_cases, pack_case, spec_fold, spec_unfold,
};
use corpus::{corpus_partitions, load_corpus, skip};

/// The arithmetic of §9.2.7.3's two bounds, for every order the field can hold,
/// and where each one stops being reachable at all.
///
/// The first bound is `u32::MAX >> order` = `2^(32-order) − 1`, so a codeword that
/// *exceeds* it is `2^(32-order) + order + 1` bits long, and a codeword that *is*
/// the second bound (the all-ones coded value: quotient exactly at the bound,
/// remainder all-ones) is one bit shorter. In bytes that is 8 KB at order 16, 128
/// KB at order 12, 2 MB at order 8, 512 MB at order 0. Against the largest region
/// FLAC can produce at all — the format caps blocksize at 65,535 (RFC 9639
/// §9.2.7) and sample width at 32 bits, so no frame region exceeds 262,140 bytes
/// — the crossover is **order 12**, and at order 11 it misses only by 43 bits: a
/// unary run that long does not fit in a frame, at any order below 12.
///
/// That is a fact about the format, not about this kernel, and it is why the E10
/// probe could leave both bounds unwrapped and still decode its corpus
/// bit-exactly: its parameters were `{0, 2, 3, 6, 8}`, four orders below the
/// crossover, and no run in its data came within six orders of magnitude of a
/// bound. "The corpus never trips it" was never evidence that leaving it unwrapped
/// was safe; it was evidence that the corpus was blind to it (Q10). 6a-4 wraps both
/// bounds, and the behavioral arms below run at exactly the orders where a bound is
/// both constructible in principle and affordable to construct in a test.
#[test]
fn the_guards_are_reachable_only_at_orders_where_the_field_is_wide() {
    /// The largest a frame region can be at all, from the format's own caps
    /// (blocksize × sample width). Not the profile's cap, which is far lower:
    /// using the format's maximum makes the claim the weakest true one available.
    const MAX_REGION_BYTES: u64 = 65_535 * 32 / 8;

    for order in 0..=31u32 {
        let bound = u64::from(u32::MAX >> order);
        // u64 on purpose: at order 0 the bound is u32::MAX, so `bound + 1` is the
        // one arithmetic step this test would get wrong in u32 — which is the same
        // shift the wrapper's hoisted guard limit performs.
        let all_ones_bits = bound + 1 + u64::from(order); // second bound's codeword
        let past_first_bound = all_ones_bits + 1; // first bound's, one longer
        assert_eq!(
            all_ones_bits,
            (1u64 << (32 - order)) + u64::from(order),
            "order {order}: the closed form of 'quotient at the bound plus an \
             all-ones remainder' is not 2^(32-order) + order"
        );
        if order >= 12 {
            assert!(
                past_first_bound / 8 <= MAX_REGION_BYTES,
                "order {order}: one bounded codeword needs {} bytes, so the bound is \
                 not constructible here and the behavioral guard arms would be vacuous",
                past_first_bound / 8
            );
        } else {
            assert!(
                all_ones_bits / 8 > MAX_REGION_BYTES,
                "order {order}: the *shorter* of the two bounded codewords needs {} \
                 bytes — the claim that both bounds are unreachable below order 12 in \
                 any region FLAC can produce is what licenses starting the guard arms at \
                 12, and it has to stay true or that arm list is wrong",
                all_ones_bits / 8
            );
        }
    }
    // The crossover itself, stated as a number rather than implied by a branch.
    assert_eq!((1u64 << (32 - 11)) + 11, 2_097_163, "order 11's run");
    assert!(
        2_097_163 > MAX_REGION_BYTES * 8,
        "order 11 misses the format's largest region by {} bits",
        2_097_163 - MAX_REGION_BYTES * 8
    );
    assert!((1u64 << (32 - 12)) + 12 <= MAX_REGION_BYTES * 8);
    eprintln!(
        "bounds arithmetic: a §9.2.7.3-sized codeword is 2^(32-order) + order bits, so \
         against the format's largest possible frame region ({MAX_REGION_BYTES} bytes) both \
         bounds are unreachable at orders 0..=11 — by 43 bits at order 11, narrowly \
         enough to be worth stating — and constructible from order 12 up. That is why \
         the probe could leave them unwrapped and stay bit-exact on a corpus whose \
         parameters were all at most 8."
    );
}

// ---------------------------------------------------------------------------
// The arms.
// ---------------------------------------------------------------------------

#[test]
fn the_arm_path_decodes_the_swept_boundaries_to_the_packer_values() {
    let mut cases = clean_cases();
    cases.extend(order_zero_cases());
    let mut clean = 0usize;
    let mut samples = 0usize;
    let mut delegated = 0usize;

    for case in &cases {
        let what = format!("order {} case at {} bits", case.order, case.end_bits);
        let anchor = &case.expected;
        let incumbent = decode_with::<TableCountKernel>(&case.bytes, case.order, anchor.len());
        let arm = decode_with::<ArmRiceKernel>(&case.bytes, case.order, anchor.len());

        assert_eq!(
            incumbent.err, None,
            "{what}: the incumbent, which decodes this by construction, failed: {:?}",
            incumbent.err
        );
        assert_eq!(
            arm.err, incumbent.err,
            "{what}: the ARM path and the incumbent disagree about whether this decodes"
        );
        assert_eq!(
            arm.samples, *anchor,
            "{what}: the ARM path's samples are not the values the packer was given"
        );
        assert_eq!(
            arm.samples, incumbent.samples,
            "{what}: the ARM path disagrees with the incumbent"
        );
        assert_eq!(
            arm.bits_after, case.end_bits,
            "{what}: the ARM path consumed {} bits, the packer wrote {}",
            arm.bits_after, case.end_bits
        );
        assert_eq!(
            arm.bits_after, incumbent.bits_after,
            "{what}: the ARM path landed at {}, the incumbent at {}",
            arm.bits_after, incumbent.bits_after
        );

        clean += 1;
        samples += anchor.len();
        if case.order > MAX_ORDER {
            delegated += 1;
        }
    }

    assert!(clean >= 200, "the sweep produced only {clean} cases");
    assert!(samples >= 300, "the sweep decoded only {samples} samples");
    assert!(
        delegated >= 10,
        "only {delegated} cases ran above order {MAX_ORDER}; the delegation arm must \
         not be thin, because this kernel's remainder leg stops at order 16 and the \
         whole claim of that branch is that the incumbent takes over identically"
    );
    eprintln!(
        "sweep: {clean} cases / {samples} samples, {delegated} cases above order \
         {MAX_ORDER} delegated by design; every one anchored to the packer's values \
         and its own bit positions"
    );
}

#[test]
fn p_equal_zero_carries_the_whole_value_in_the_unary_run() {
    // p = 0 is the order where an un-reversal bug, a mask bug and a
    // remainder-leg-skip bug are all invisible at once, because the remainder is
    // empty either way. examples/arm-kernels records the same trap for its own
    // unmap probe ("the trap: p = 0 hides everything"), so it gets its own arm.
    //
    // What this adds over the sweep is the bit law. At order 0 a codeword is
    // exactly `folded + 1` bits, where `folded` is the spec's folded value and at
    // this order *is* the quotient — a different expression from the one
    // `pack_case`/`end_bits_of` used to lay the bits out, so a remainder leg that
    // ran when the mask said it must not would show up as a cursor disagreement
    // here rather than as a shared mistake.
    let mut bits = 0usize;
    let mut codewords = 0usize;
    for case in order_zero_cases() {
        let n = case.expected.len();
        let incumbent = decode_with::<TableCountKernel>(&case.bytes, 0, n);
        let arm = decode_with::<ArmRiceKernel>(&case.bytes, 0, n);
        assert_eq!(arm.samples, case.expected, "p = 0 samples");
        assert_eq!(arm.samples, incumbent.samples, "p = 0 vs the incumbent");
        assert_eq!(arm.err, incumbent.err, "p = 0 error kind");
        assert_eq!(arm.bits_after, case.end_bits, "p = 0 cursor");

        let law: usize = case
            .expected
            .iter()
            .map(|v| usize::try_from(spec_fold(*v)).unwrap() + 1)
            .sum();
        assert_eq!(
            law, case.end_bits,
            "p = 0: the unary-only bit law says {law} bits, the case is {} bits",
            case.end_bits
        );
        for v in &case.expected {
            assert_eq!(
                spec_unfold(spec_fold(*v)),
                *v,
                "p = 0: at this order the folded value is the whole sample, so the \
                 anchor's own round trip is the whole of its correctness"
            );
            codewords += 1;
        }
        bits += case.end_bits;
    }
    assert!(
        codewords >= 13 && bits > 2000,
        "the p = 0 arm degenerated: {codewords} codewords, {bits} bits"
    );
    eprintln!(
        "p = 0: {codewords} codewords / {bits} bits, all of it unary; samples, error \
         kind and cursor all exact against the unary-only bit law"
    );
}

#[test]
fn both_guards_reject_at_the_same_point_the_incumbent_does() {
    let mut rejections = 0usize;
    let mut at = Vec::new();
    for case in guard1_cases().into_iter().chain(guard2_cases()) {
        let what = format!(
            "order {} guard case ({} bits, {} bytes of region)",
            case.order,
            case.end_bits,
            case.bytes.len()
        );
        let incumbent = decode_with::<TableCountKernel>(&case.bytes, case.order, case.want);
        let arm = decode_with::<ArmRiceKernel>(&case.bytes, case.order, case.want);
        assert_eq!(
            incumbent.err,
            Some(Error::InvalidField),
            "{what}: the incumbent did not reject a §9.2.7.3 codeword, so the case \
             stopped being a guard case"
        );
        assert_eq!(
            arm.err, incumbent.err,
            "{what}: the ARM path answers a different kind than the incumbent — and \
             through the seam the kind is the sample's kind, because the caller turns \
             any kernel error into Error::InvalidField"
        );
        assert_eq!(
            arm.bits_after, incumbent.bits_after,
            "{what}: a rejected codeword left the cursor at {} while the incumbent \
             left it at {} — the incumbent reads the whole codeword and only then \
             asks §9.2.7.3, so its rejection is not atomic, and this path has to \
             stop at the same two points it does",
            arm.bits_after, incumbent.bits_after
        );
        rejections += 1;
        at.push(case.order);
    }
    assert!(
        rejections >= 11,
        "only {rejections} guard cases ran; each bound needs arms at several orders, \
         because the region that reaches a bound grows as 2^(32-order)"
    );

    // The near-miss controls: a codeword sitting exactly ON each bound, and one
    // step below it. Both must be accepted, and their samples must be what the
    // packer wrote. A guard with an off-by-one would reject the bound itself, and
    // nothing else in this file or in the committed corpus would notice: the
    // corpus's longest unary run is 15 and these bounds are 65,535.
    let mut controls = 0usize;
    for order in 12..=16u32 {
        for q in [u32::MAX >> order, (u32::MAX >> order) - 1] {
            // The packer's own encode direction produces the codeword for this
            // value, so the control is built the way a real stream would be
            // built rather than by hand-counting zeros.
            let coded = (q << order) | (u32::MAX >> (32 - order)) >> 1; // rem clear of all-ones
            let v = spec_unfold(coded);
            let case = pack_case(order, &[v]);
            assert_eq!(
                case.end_bits,
                usize::try_from(u64::from(q) + 1 + u64::from(order)).expect("control length"),
                "order {order} control at q={q}: the packer did not produce the \
                 codeword this control is named for"
            );
            let incumbent = decode_with::<TableCountKernel>(&case.bytes, case.order, case.want);
            let arm = decode_with::<ArmRiceKernel>(&case.bytes, case.order, case.want);
            assert_eq!(
                incumbent.err, None,
                "order {order} control at q={q}: the incumbent rejected a codeword \
                 sitting on the bound — §9.2.7.3 says MAX >> order is allowed"
            );
            assert_eq!(
                arm.err, incumbent.err,
                "order {order} control at q={q}: the ARM path rejects what the \
                 incumbent accepts, which is an off-by-one in [6a-4 G1] or [6a-4 G2]"
            );
            assert_eq!(
                arm.samples, case.expected,
                "order {order} control at q={q}: the accepted sample is not the \
                 packer's value"
            );
            assert_eq!(
                arm.bits_after, incumbent.bits_after,
                "order {order} control at q={q}: the accepted cursor differs"
            );
            controls += 1;
        }
    }
    assert!(
        controls == 10,
        "{controls} controls: one per order the guards are tested at, at the bound \
         and one below it"
    );
    eprintln!(
        "guards: {rejections} rejections at orders {at:?}, and {controls} near-miss \
         controls at the bound and one below it all accepted identically — the bounds \
         are tested, not wrapped, and they are not off by one"
    );
}

#[test]
fn an_unterminated_codeword_fails_the_same_kind_and_the_record_says_where_the_cursor_is() {
    let mut kinds = 0usize;
    let mut diverged = 0usize;
    for case in eof_cases() {
        let what = format!(
            "eof arm order {} ({} bits packed, {} asked for)",
            case.order, case.end_bits, case.want
        );
        let incumbent = decode_with::<TableCountKernel>(&case.bytes, case.order, case.want);
        let arm = decode_with::<ArmRiceKernel>(&case.bytes, case.order, case.want);
        assert_eq!(
            incumbent.err,
            Some(Error::EndOfStream),
            "{what}: the incumbent did not report end of stream ({:?}) — the case is no \
             longer an end-of-stream case, and this arm would be vacuous",
            incumbent.err
        );
        assert_eq!(
            arm.err, incumbent.err,
            "{what}: the ARM path reports a different kind than the incumbent"
        );
        let written = case.expected.len();
        assert_eq!(
            arm.samples[..written],
            case.expected[..],
            "{what}: the samples written before the failure are not the packer's"
        );
        assert_eq!(
            incumbent.samples[..written],
            case.expected[..],
            "{what}: the incumbent disagrees with the packer — the packer or the \
             case's `want` is wrong"
        );
        if arm.bits_after != incumbent.bits_after {
            diverged += 1;
        }
        kinds += 1;
    }
    assert_eq!(kinds, 5, "all five end-of-stream arms must run");
    eprintln!(
        "end-of-stream arms: {kinds} cases, {diverged} leave the cursor somewhere the \
         incumbent does not. Permitted by the trait (only per-read atomicity is \
         promised), ratified by the probe at the partition level, and kept here: the \
         frame is unread and the stream is dead either way. Q1's question, answered \
         by measurement rather than by argument."
    );
}

#[test]
fn an_order_the_field_cannot_hold_is_rejected_by_the_path_that_takes_it() {
    for order in out_of_range_cases() {
        let bytes = [0b0111_1111u8; 24];
        let incumbent = decode_with::<TableCountKernel>(&bytes, order, 1);
        let arm = decode_with::<ArmRiceKernel>(&bytes, order, 1);
        assert_eq!(
            incumbent.err,
            Some(Error::InvalidField),
            "order {order}: the incumbent should reject an order the 5-bit field \
             cannot hold"
        );
        assert_eq!(
            arm.err, incumbent.err,
            "order {order}: the delegation answers a different kind"
        );
        assert_eq!(
            arm.bits_after, incumbent.bits_after,
            "order {order}: the delegation moved the cursor differently"
        );
    }
}

#[test]
fn the_model_and_the_wrapper_agree_on_status_and_on_the_domain_rule() {
    // The wrapper is the only thing between the kernel's three status words and
    // the core's two error values, and it is also what refuses to run the
    // kernel's arithmetic above MAX_ORDER. On the host both halves are reachable
    // directly, so this is where the mapping gets tested rather than asserted in
    // a comment.
    let mut counts = [0usize; 3];

    for case in clean_cases().into_iter().take(400) {
        let mut out = vec![0i32; case.expected.len()];
        let win = BitReader::new(&case.bytes).kernel_window();
        let (_, status) = model_partition(&win, &case.bytes, &mut out, case.order);
        assert_eq!(
            status,
            KernelStatus::Done,
            "order {} clean case",
            case.order
        );
        assert_eq!(out, case.expected, "order {} model samples", case.order);
        counts[0] += 1;
    }
    for case in guard1_cases().into_iter().chain(guard2_cases()) {
        if case.order > MAX_ORDER {
            continue; // the wrapper would not call the model at that order
        }
        let mut out = vec![0i32; case.want];
        let win = BitReader::new(&case.bytes).kernel_window();
        let (after, status) = model_partition(&win, &case.bytes, &mut out, case.order);
        assert_eq!(
            status,
            KernelStatus::GuardRejected,
            "order {} guard",
            case.order
        );
        // The kernel stored an advanced state before rejecting, and the wrapper
        // hands it back rather than discarding it (the incumbent's rejection is
        // not atomic either). Both halves of that decision are visible here.
        assert!(
            after.ptr >= win.ptr,
            "the kernel did advance state on a rejection"
        );
        counts[2] += 1;
    }
    for case in eof_cases() {
        let mut out = vec![0i32; case.want];
        let win = BitReader::new(&case.bytes).kernel_window();
        let (_, status) = model_partition(&win, &case.bytes, &mut out, case.order);
        assert_eq!(status, KernelStatus::EndOfStream, "end-of-stream arm");
        counts[1] += 1;
    }

    assert_eq!(KernelStatus::Done.code(), 0);
    assert_eq!(KernelStatus::EndOfStream.code(), 1);
    assert_eq!(KernelStatus::GuardRejected.code(), 2);
    for (code, expect) in [
        (0, KernelStatus::Done),
        (1, KernelStatus::EndOfStream),
        (2, KernelStatus::GuardRejected),
        (3, KernelStatus::GuardRejected),
        (u32::MAX, KernelStatus::GuardRejected),
    ] {
        assert_eq!(
            KernelStatus::from_code(code),
            expect,
            "status word {code:#x} mapped wrong — an unknown word must not be \
             trusted as a window"
        );
    }
    assert!(
        counts[0] == 400 && counts[1] == 5 && counts[2] >= 8,
        "status coverage was not what this arm is for: {counts:?} \
         [done, end_of_stream, guard_rejected]"
    );
    eprintln!("status coverage: {counts:?} [done, end_of_stream, guard_rejected]");
}

#[test]
#[cfg(debug_assertions)]
fn the_model_refuses_the_domain_the_wrapper_delegates() {
    // The delegation is a property of the arithmetic, not a formality: the model's
    // (and the asm's) remainder leg builds a 16-bit reversal, so above MAX_ORDER
    // it stops being a model of anything. The wrapper's `order > MAX_ORDER` branch
    // is what keeps that unreachable, and the model says so about itself — a
    // debug_assert the host build can see and the target build cannot.
    let bytes = [0u8; 64];
    let win = BitReader::new(&bytes).kernel_window();
    let mut out = [0i32; 1];
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        model_partition(&win, &bytes, &mut out, MAX_ORDER + 1)
    }));
    assert!(
        r.is_err(),
        "the model accepted order {} — then the domain limit lives only in the \
         wrapper's branch, and the thing that decides it is the arithmetic",
        MAX_ORDER + 1
    );
}

// ---------------------------------------------------------------------------
// Table ownership: Q1's other open half, resolved here as "this crate owns
// copies" and witnessed by re-derivation rather than by a second transcription.
// ---------------------------------------------------------------------------

#[test]
fn the_tables_are_the_bytes_they_claim_and_their_addresses_are_reported_not_assumed() {
    // Why copies and not the core's: `flac-lite`'s CTZ8/REV8/BRUIJN_INDEX are
    // private to `rice-hybrid`, and reaching for them would force a second Rice
    // mechanism into every image that links this crate — item 5 PR 3 measured
    // that cost (−26.5 M cycles, +2,984 B) for a mechanism that was never called.
    // What is checked here is that the copies are the bytes the naive definition
    // says they are, from the definition rather than from a transcription.
    let (ctz8_addr, rev8_addr, bruijn_addr) = tables_iwram_addrs();
    assert_ne!(ctz8_addr, 0);
    assert_ne!(rev8_addr, 0);
    assert_ne!(bruijn_addr, 0);

    for b in 0u32..256 {
        let mut want = 0u8;
        while want < 8 && (b >> want) & 1 == 0 {
            want += 1;
        }
        assert_eq!(CTZ8[b as usize], want, "CTZ8[{b}]");
        let mut rev = 0u8;
        for i in 0..8 {
            if (b >> i) & 1 == 1 {
                rev |= 1 << (7 - i);
            }
        }
        assert_eq!(REV8[b as usize], rev, "REV8[{b}]");
    }
    // The De Bruijn perfect hash, over the constant the kernel's literal pool
    // loads: the index expression is the kernel's, the property is that the table
    // inverts it for all 32 positions.
    const K: u32 = 0x077C_B531;
    for b in 0usize..32 {
        let idx = (1u32 << b).wrapping_mul(K) as usize >> 27;
        assert_eq!(
            BRUIJN_INDEX[idx], b as u8,
            "BRUIJN_INDEX[{idx}] must be {b} — the index expression and the table \
             have to be inverses, because the kernel trusts one to find the other"
        );
    }

    // A third anchor, so the digests below are not just a second copy of the
    // definition: FNV-1a over each table's bytes. A table that changes — here, or
    // in the core's copy if this crate ever reaches for it — trips these
    // constants, and the change belongs in a record rather than in a diff. The
    // core's `rice_hybrid` unit tests keep the same kind of anchor for its own
    // copies ("if these constants hold, the core's copies are these bytes").
    let digest = |t: &[u8]| {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for b in t {
            h = (h ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3);
        }
        h
    };
    assert_eq!(
        digest(&CTZ8),
        0x8c1a_bbbe_191f_512a,
        "CTZ8 changed byte-for-byte — the drift this repo pays for elsewhere"
    );
    assert_eq!(
        digest(&REV8),
        0x7492_6a86_12ae_c825,
        "REV8 changed byte-for-byte"
    );
    assert_eq!(
        digest(&BRUIJN_INDEX),
        0x335e_4a83_b157_6845,
        "BRUIJN_INDEX changed byte-for-byte"
    );
    eprintln!(
        "tables: CTZ8 @ {ctz8_addr:#x}, REV8 @ {rev8_addr:#x}, BRUIJN_INDEX @ \
         {bruijn_addr:#x} (host addresses). The device-side placement witness is the \
         instrument ROM's own test and `nm`/`objdump` on its ELF — no kernel-check \
         hard-codes a table address, per flac-docs/open-questions.md Q1."
    );
}

// ---------------------------------------------------------------------------
// The lift's own boundaries: what was brought, what was changed, and what was
// not wired in.
// ---------------------------------------------------------------------------

#[test]
fn the_fold_is_pasted_from_the_one_committed_source_and_writes_no_flags() {
    // Decision 6's sharing rule, pinned in code: the fold body is not vendored
    // into this crate. `rice_arm.rs` pastes examples/arm-kernels/asm/fold_body.s
    // into the same `global_asm!` unit as the partition kernel — which is also why
    // the macro has to be pasted first, since rustc hands the assembler a temp
    // file with no reliable search path (examples/arm-kernels/src/asm.rs).
    let asm_dir = concat!(env!("CARGO_MANIFEST_DIR"), "/asm");
    let mut files: Vec<String> = std::fs::read_dir(asm_dir)
        .unwrap_or_else(|e| panic!("cannot read {asm_dir}: {e}"))
        .map(|e| {
            e.expect("an asm entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    files.sort();
    assert_eq!(
        files,
        vec!["rice_partition.s"],
        "6a-4 adds exactly one asm file here; a second is either a vendored fold \
         (forbidden by decision 6) or a second kernel, and either belongs in a diff \
         that says which"
    );

    let here = std::fs::read_to_string(format!("{asm_dir}/rice_partition.s"))
        .expect("the partition kernel's source");
    let code_only = here
        .lines()
        .map(|l| l.split('@').next().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        !code_only.contains("fold_body:") && !code_only.contains(".macro"),
        "the partition kernel must not restate the fold: the shared source is \
         examples/arm-kernels/asm/fold_body.s, dual-witnessed as macro and expansion \
         in examples/arm-kernels"
    );

    let src = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/rice_arm.rs"))
        .expect("the module that assembles it");
    assert!(
        src.contains(r#"include_str!("../../../examples/arm-kernels/asm/fold_body.s")"#),
        "the fold is pasted from the one committed source"
    );
    assert!(
        src.contains(r#"include_str!("../asm/rice_partition.s")"#),
        "the partition kernel is pasted from this crate's asm/"
    );
    assert!(
        src.find("examples/arm-kernels/asm/fold_body.s") < src.find("../asm/rice_partition.s"),
        "the fold body is pasted BEFORE the partition kernel in the same unit, so \
         the macro exists before its first use"
    );

    // The property the shared fold's own header insists on, re-checked from here
    // because this kernel depends on it: the fold writes no condition flags, and
    // the kernel's callers keep live flag state across the fold site.
    let fold = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../examples/arm-kernels/asm/fold_body.s"
    ))
    .expect("the shared fold body");
    let mut ops = 0usize;
    for line in fold.lines() {
        let code = line.split('@').next().unwrap_or("").trim();
        if code.is_empty()
            || code.starts_with('.')
            || code.starts_with('#')
            || code.starts_with('\\')
        {
            continue;
        }
        let opcode = code.split_whitespace().next().unwrap_or("");
        ops += 1;
        assert!(
            !opcode.ends_with('s') && !opcode.starts_with("cmp") && !opcode.starts_with("teq"),
            "the shared fold writes flags ({opcode}) — every caller's flag chain \
             across the fold site is then wrong, and this kernel's is"
        );
    }
    assert!(ops >= 4, "the shared fold body read as {ops} instructions");
}

#[test]
fn the_landed_kernel_is_the_probe_s_with_the_recorded_deltas_and_no_loose_ends() {
    // The lift is line-for-line except the four deltas the file's own header
    // marks. Pinning the markers is how "we test the guards instead of wrapping
    // them" stays true after the next person edits the assembly.
    let asm = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/asm/rice_partition.s"))
        .expect("the partition kernel's source");
    for marker in ["[6a-4 FRAME]", "[6a-4 G1]", "[6a-4 G2]", "[6a-4 ST]"] {
        assert!(
            asm.contains(marker),
            "{marker} is missing from asm/rice_partition.s — that marker is the \
             difference between this kernel and the E10 probe, and it is exactly what \
             the guard and status arms above test"
        );
    }
    let code: String = asm
        .lines()
        .map(|l| l.split('@').next().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n");
    // The probe's legs must still be there: the guard exit, the un-reversal, the
    // two refills, the word-skip. The three tables are deliberately NOT in this
    // list: the kernel is handed them by pointer through the argument slots, so
    // their names appear in this unit only in comments — which is what the slot
    // checks at the end of this test and the call-site checks below cover
    // instead. An earlier version of this arm looked for the names in code and
    // was wrong about what the unit contains.
    for needle in [".Lvalerr", ".Lwslow", ".Lrefill2", ".Lrem_have"] {
        assert!(code.contains(needle), "{needle} disappeared from the unit");
    }
    // The two standing no-degradation rules, from the unit's own text. The real
    // check is `objdump -d` of the built ELF (`make overlay-objdump`); this arm
    // is the cheap source-level shadow of it.
    assert!(
        code.contains("bx      lr") || code.contains("bx lr"),
        "an interwork leaf return: on ARMv4 only BX/BLX switch mode, and the return \
         must not leave CPSR in Thumb state (README/llvm-integration gotcha; 3 cycles \
         per call and 4 bytes per table lost to LDM-to-PC in the first pass — \
         flac-docs/arm-kernels.md)"
    );
    assert!(
        code.contains("pop     {{r4-r11, lr}}") || code.contains("pop     {r4-r11, lr}"),
        "the epilogue pops the registers it pushed and leaves lr for the bx"
    );
    assert!(
        !code.to_lowercase().contains("ldm"),
        "no LDM in this unit: an LDM whose base register is also in its own register \
         list is unpredictable on ARMv4 (arm-kernels.md pitfall 2), and LDM-to-PC is \
         the return form this repo measured against"
    );
    // The frame and the argument slots have to agree with what the Rust side
    // passes, or the tables land in the wrong registers.
    let abi = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/rice_arm.rs"))
        .expect("the module that declares the ABI");
    // AAPCS arithmetic, not a transcription: r0-r3 carry the first four
    // arguments, so the tables arrive as arguments 5, 6 and 7 at [sp_entry],
    // [sp_entry+4], [sp_entry+8]. The prologue moves sp down by 36 (nine pushes)
    // + 12 (the frame), so the same words are at 48/52/56 from the new sp.
    // Pinned here because the first landed cut used 56/60/64 — the probe's
    // 44/48/52 offsets with the frame's +4 applied as if it were +8 — and every
    // host-side arm stayed green: on the host the pointers are Rust arguments to
    // the model, so a wrong stack slot is not a wrong answer until the asm runs.
    // The kernel-check ROM's first end-of-stream arm is what convicted it.
    for slot in ["[sp, #48]", "[sp, #52]", "[sp, #56]"] {
        assert!(
            code.contains(slot),
            "the argument slot {slot} is missing — the frame is 12 bytes after \
             push {{r4-r11, lr}} (36 B), so the three table pointers sit at \
             48/52/56 and the guard limit at 8"
        );
    }
    for slot in ["[sp, #60]", "[sp, #64]"] {
        assert!(
            !code.contains(slot),
            "{slot} is read by the kernel body, but 36 + 12 is 48: the slots are \
             48/52/56, and a read at 60/64 lands in the caller's frame. Every host \
             arm would still be green — the model receives the pointers as Rust \
             arguments, not through the stack"
        );
    }
    assert!(
        abi.contains("offset_of") && abi.contains("const _: () = assert!"),
        "the state-struct ABI is pinned at compile time with offset_of! in the module \
         that declares it — an assertion on a type this file cannot see"
    );
    // The three tables reach the kernel only as arguments. If one of those three
    // lines changed, the kernel would be reading someone else's bytes and every
    // table witness in this file would still pass.
    for arg in ["CTZ8.as_ptr()", "REV8.as_ptr()", "BRUIJN_INDEX.as_ptr()"] {
        assert!(
            abi.contains(arg),
            "the call site no longer passes {arg} — the kernel's tables are named by \
             the wrapper, not by the assembly, and that indirection is the whole \
             reason the table drift arms above have any force"
        );
    }
}

/// The `unsafe` budget, counted by attribute shape rather than by the word.
///
/// 6a-4 is where this repository's first `unsafe` lands, and both crate docs
/// promised a counted, named budget rather than a paragraph. Counting prose would
/// be silly, so this counts *attributes*: under `#![deny(unsafe_code)]` an unsafe
/// item cannot compile without one, so the number of allowances is the number of
/// items, and the pair is what a reviewer can check without running anything.
#[test]
fn the_unsafe_budget_is_counted_by_attribute_shape_not_by_prose() {
    let src = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/rice_arm.rs"))
        .expect("the module that carries the unsafe");

    let allowances = src
        .lines()
        .filter(|l| l.trim_start().starts_with("#[allow(unsafe_code)"))
        .count();
    // 6a-4's own module: one allowance per unsafe item it declares.
    assert_eq!(
        allowances, 6,
        "src/rice_arm.rs now carries {allowances} allowance attributes, not 6 — the \
         budget both crate docs state (global_asm module, extern declaration, three \
         .iwram placement attributes, one FFI call site) has moved"
    );

    // Attribute shape, so prose about the attributes does not count.
    let code = |l: &&str| !l.trim_start().starts_with("//");
    let placements = src
        .lines()
        .filter(|l| l.contains("unsafe(link_section") && code(l))
        .count();
    assert_eq!(
        placements, 3,
        "{placements} placement attributes: three tables, one attribute each, all of \
         them behind cfg_attr(all(target_arch = 'arm', feature = 'iwram-pin'))"
    );
    assert_eq!(
        src.lines()
            .filter(|l| l.contains("core::arch::global_asm!") && code(l))
            .count(),
        1,
        "more than one assembly unit in the overlay: 6a-4 is one kernel"
    );
    assert_eq!(
        src.lines()
            .filter(|l| l.trim_start().starts_with("unsafe extern"))
            .count(),
        1,
        "one extern declaration, for the one kernel this PR lands"
    );
    assert_eq!(
        src.lines()
            .filter(|l| {
                let t = l.trim_start();
                t.starts_with("unsafe {") || t.starts_with("unsafe\n{")
            })
            .count(),
        1,
        "one FFI call site: the docs promised 'one SAFETY block per FFI site' and \
         a second call site is what makes that promise need re-reading"
    );
    assert_eq!(
        src.lines()
            .filter(|l| l.trim_start().starts_with("// SAFETY:"))
            .count(),
        1,
        "the one call site has exactly one SAFETY block"
    );

    // And the default build: the module that would carry any of this is not
    // compiled at all. That is witnessed by `make overlay-check` (no features)
    // and by the crate docs' "0 in a default build" claim; the arm below makes
    // the claim about the *cfg* rather than about a reading of the file.
    assert!(
        src.contains("#[cfg(target_arch = \"arm\")]\nmod asm_unit")
            || src.contains("#[allow(unsafe_code)]\nmod asm_unit"),
        "the assembly module is no longer a module of its own, so the module-level \
         allowance that a global_asm! needs (a macro-item attribute does not bind) \
         has nowhere to sit"
    );
    let lib =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/lib.rs")).expect("lib");
    assert!(
        lib.contains("#![deny(unsafe_code)]"),
        "the crate-root deny is what makes the allowance count the item count"
    );
    assert!(
        !lib.contains("\n#[allow(unsafe_code") && !lib.contains("#![allow(unsafe_code"),
        "the crate root must not open unsafe on itself: the module is the unit of \
         the budget"
    );
    let twin = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/rice_twin.rs"))
        .expect("the 6a-3 module");
    assert!(
        !twin.contains("unsafe"),
        "6a-3's pure-Rust arm still contains no unsafe; 6a-4's budget is named per \
         item and must not have leaked sideways"
    );

    eprintln!(
        "unsafe budget: 6 allowance attributes in src/rice_arm.rs = 6 unsafe items \
         (global_asm! module, extern declaration, 3 .iwram placements behind \
         iwram-pin, 1 FFI call site) behind one crate-root #![deny(unsafe_code)]; \
         3 of the 6 exist only when iwram-pin is on, 0 in a default build"
    );
}

#[test]
fn naming_the_arm_kernel_has_wired_nothing_in() {
    // The core's default, pinned from outside — the same pin the core's own
    // differential and this crate's 6a-3 suite put on it.
    // 6a-5 moved the default's line up to the caller-facing entry (the chain is
    // generic now); this pin follows it. See `residual::decode_residual`'s docs.
    let source = include_str!("../../flac-lite/src/residual.rs");
    let body = source
        .split_once("\npub fn decode_residual(")
        .expect("the default's entry point must exist under that exact name")
        .1
        .split("\n}")
        .next()
        .expect("and have a body");
    assert!(
        body.contains("decode_residual_with::<TableCountKernel>"),
        "the default still names TableCountKernel by measurement (item 5 PR 4). 6a-4 \
         moves no decode path: 6a-5's cycle reading is the thing allowed to"
    );
    assert!(
        !body.contains("ArmRiceKernel"),
        "no kernel from this crate is the core's default"
    );

    // This crate's side: the kernel exists to be instantiated by a witness or by
    // the instrument ROM, and no module tree reaches it.
    let src_dir = concat!(env!("CARGO_MANIFEST_DIR"), "/src");
    let mut files: Vec<String> = std::fs::read_dir(src_dir)
        .unwrap_or_else(|e| panic!("cannot read {src_dir}: {e}"))
        .map(|e| {
            e.expect("a src entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    files.sort();
    assert_eq!(
        files,
        vec!["lib.rs", "rice_arm.rs", "rice_twin.rs"],
        "6a-3 added one module and 6a-4 one more; a third arrives with the decode \
         path or the ROM that uses it, not before"
    );
    for file in &files {
        let text = std::fs::read_to_string(format!("{src_dir}/{file}"))
            .unwrap_or_else(|e| panic!("read {file}: {e}"));
        let naming: Vec<String> = text
            .lines()
            .map(|l| l.to_string())
            .filter(|l| !(l.starts_with("//") || l.starts_with("///") || l.starts_with("//!")))
            .filter(|l| l.contains("ArmRiceKernel"))
            .collect();
        if file == "rice_arm.rs" {
            assert!(
                naming.len() >= 2,
                "the kernel's own file must declare the type and implement the trait"
            );
        } else {
            assert!(
                naming.is_empty(),
                "{file} names the kernel in code: {naming:?} — a kernel that reaches a \
                 module tree has become a decode path by accident, which is how item 5 \
                 PR 3's default flipped"
            );
        }
    }

    // And the shipped ROMs: "nothing links it" is a manifest fact, and a comment
    // cannot say it. Both shipped ROM manifests are read; the instrument ROM is
    // the single exception and is asserted as one.
    for manifest in [
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../examples/flac_spike/Cargo.toml"
        ),
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../examples/flac_integration/Cargo.toml"
        ),
    ] {
        let text = std::fs::read_to_string(manifest).expect("a shipped ROM manifest");
        for line in text.lines() {
            let l = line.trim();
            if l.starts_with("flac-lite-gba") {
                assert!(
                    l.contains("optional = true"),
                    "{manifest} depends on the overlay unconditionally: {l} — no \
                     shipped ROM's default build may change in this PR; the only \
                     unconditional link is the instrument ROM's own manifest"
                );
            }
        }
    }
    let instrument = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../examples/overlay-kernel-check/Cargo.toml"
    );
    let text = std::fs::read_to_string(instrument).expect("the instrument ROM's manifest");
    assert!(
        text.contains(r#""rice-arm""#),
        "{instrument} must link the kernel with rice-arm on: that ROM is the only \
         device-side witness this PR has, and a ROM that does not enable the feature \
         witnesses the default build, which this PR does not change"
    );
    // The double-`-Tgba.ld` rule, checked as a filesystem fact rather than as a
    // string: cargo CONCATENATES `target.<triple>.rustflags` across config layers,
    // so a local `.cargo/config.toml` in this workspace would link `-Tgba.ld`
    // twice (README, agb-build gotcha 2). The manifest's own comment *names* that
    // file, which is exactly why a `contains(".cargo/config.toml")` check was the
    // wrong instrument here — a comment tripped it.
    let local_config = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../examples/overlay-kernel-check/.cargo/config.toml"
    );
    assert!(
        !std::path::Path::new(local_config).exists(),
        "the instrument ROM is a standalone workspace and must inherit the root \
         config: a local .cargo/config.toml at {local_config} would double-link \
         -Tgba.ld (examples/arm-kernels/Cargo.toml gotcha)"
    );
}

// ---------------------------------------------------------------------------
// The corpus arm, and the count of what the corpus can witness.
// ---------------------------------------------------------------------------

#[test]
fn the_arm_path_decodes_every_committed_partition_exactly_as_the_default_does() {
    let mut partitions = 0usize;
    let mut samples = 0usize;
    let mut delegated = 0usize;
    let mut params: Vec<u32> = Vec::new();

    for v in load_corpus() {
        if v.kind != "fixed" && v.kind != "lpc" {
            continue;
        }
        let what = format!("vector {}", v.label);
        for part in corpus_partitions(&v) {
            if part.header.is_escape {
                continue;
            }
            partitions += 1;
            params.push(u32::from(part.header.order));
            if u32::from(part.header.order) > MAX_ORDER {
                delegated += 1;
            }
            let n = usize::from(part.header.sample_count);
            let incumbent = {
                let mut r = BitReader::new(&v.frame);
                skip(&mut r, part.body_at);
                let mut out = vec![0i32; n];
                let e =
                    decode_rice_partition_with::<TableCountKernel>(&mut r, &part.header, &mut out);
                (out, r.bit_position(), e.err())
            };
            let arm = {
                let mut r = BitReader::new(&v.frame);
                skip(&mut r, part.body_at);
                let mut out = vec![0i32; n];
                let e = decode_rice_partition_with::<ArmRiceKernel>(&mut r, &part.header, &mut out);
                (out, r.bit_position(), e.err())
            };
            assert_eq!(
                incumbent.2, arm.2,
                "{what}: the incumbent and the ARM path disagree about whether a \
                 partition decodes (parameter {}, {n} samples)",
                part.header.order
            );
            if let Some(e) = incumbent.2 {
                panic!("{what}: the incumbent failed: {e:?}");
            }
            assert_eq!(
                arm.0, incumbent.0,
                "{what}: the ARM path disagrees with the incumbent about the samples \
                 (parameter {}, {n} samples)",
                part.header.order
            );
            assert_eq!(
                arm.1, incumbent.1,
                "{what}: the ARM path landed at bit {}, the incumbent at {}",
                arm.1, incumbent.1
            );
            samples += n;
        }
    }
    params.sort_unstable();
    params.dedup();
    assert_eq!(
        delegated, 0,
        "the corpus now contains {delegated} partitions above order {MAX_ORDER} — \
         good news about the encoder, but it means the synthesized delegation arms \
         above stopped being the only witness of that path and Q10's coverage claims \
         in flac-docs/open-questions.md have to be re-stated"
    );
    assert!(
        partitions >= 4 && samples >= 512 && params.len() >= 2,
        "the corpus yielded {partitions} partitions / {samples} samples / {params:?} — \
         regenerate it with scripts/gen_frame_vectors.sh before trusting this arm"
    );
    eprintln!(
        "corpus: {partitions} Rice partitions / {samples} samples, parameters {params:?} \
         — equality with the default, whose own anchor is flac -d (crates/flac-lite/ \
         tests/body_layout.rs) and 6a-3's value assertions"
    );
}

#[test]
fn the_committed_corpus_still_blinds_the_boundaries_it_was_blinding() {
    // Q10, measured. Not a boast about the corpus: a statement of what it cannot
    // witness, in numbers, so that the synthesized arms above are understood as
    // the only witness of those boundaries and so this sentence goes stale loudly
    // if the corpora are ever widened.
    let mut partitions = 0usize;
    let mut samples = 0usize;
    let mut escapes = 0usize;
    let mut params: Vec<u32> = Vec::new();
    let mut max_q = 0u32;
    let mut near_guard1 = 0usize;
    let mut near_guard2 = 0usize;

    for v in load_corpus() {
        if v.kind != "fixed" && v.kind != "lpc" {
            continue;
        }
        for part in corpus_partitions(&v) {
            if part.header.is_escape {
                escapes += 1;
                continue;
            }
            partitions += 1;
            let p = u32::from(part.header.order);
            params.push(p);
            samples += usize::from(part.header.sample_count);
            let bound = u32::MAX >> p;
            for &q in &part.quotients {
                max_q = max_q.max(q);
                if q > bound {
                    near_guard1 += 1;
                }
                // "Within one quotient of the bound" — i.e. whether the corpus
                // comes near the second guard at all.
                if q + 1 >= bound {
                    near_guard2 += 1;
                }
            }
        }
    }
    params.sort_unstable();
    params.dedup();

    assert_eq!(
        escapes, 0,
        "the corpus has gained an escaped partition. Q10's blindness list is out of \
         date, the record has to be re-stated rather than this assertion quietly \
         widened, and the escape path — which never enters a kernel — stops being \
         described correctly."
    );
    assert!(
        params.iter().all(|&p| p <= 8),
        "the corpus's largest Rice parameter is now {params:?}; the record says 8, and \
         parameters above that (including the 5-bit coding method's range) are what \
         this suite's synthesized arms exist for"
    );
    assert!(
        max_q < 64,
        "the corpus's longest unary run is now {max_q}. The run tiers are only \
         exercised by the synthesized arms while this holds: tier 3 begins at 16, \
         the word-skip at 32."
    );
    assert_eq!(
        (near_guard1, near_guard2),
        (0, 0),
        "a corpus codeword came within one quotient of a §9.2.7.3 bound \
         ({near_guard1} past the first, {near_guard2} near the second) — the guards \
         are no longer unexercised by real data, and that belongs in a record about \
         the corpora, not in a widened assertion here"
    );
    eprintln!(
        "corpus blindness, measured: {partitions} partitions / {samples} samples / \
         parameters {params:?} / 0 escapes / longest unary run {max_q} against a \
         smallest bound of {} — the guards are unreachable by this data by ~10^6 in \
         samples. Q10's answer stands: these corpora witness the map, not the edges.",
        params
            .iter()
            .map(|&p| u32::MAX >> p)
            .min()
            .unwrap_or(u32::MAX)
    );
}
