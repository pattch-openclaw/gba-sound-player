//! 6a-4 kernel-check ROM — the device-side half of the overlay kernel's
//! correctness witness.
//!
//! # Why this crate exists
//!
//! `crates/flac-lite-gba`'s `ArmRiceKernel` assembles `asm/rice_partition.s`
//! and calls it through an FFI/state-struct ABI. Nothing on this host can execute
//! that instruction stream: the machine is aarch64, `qemu-system-arm` offers no
//! `arm7tdmi` CPU, and there are no `arm-none-eabi-*` binutils to even inspect it
//! closely (see `flac-docs/arm-kernels.md`). So the host suite
//! (`crates/flac-lite-gba/tests/arm_from_outside.rs`) runs a label-for-label Rust
//! model of the same body and witnesses the **contract**; this ROM runs the
//! **assembled kernel** and the model side by side on mGBA's ARM7TDMI, which is
//! the execution oracle. That is the only place the two halves meet.
//!
//! # What it sweeps
//!
//! Both consumers declare `cases.rs` and `corpus.rs` with `#[path]`, so the ROM
//! and the host suite run the *identical* case set, the identical packer-built
//! value anchors and the identical corpus walk — never a second transcription of
//! an expectation:
//!
//! * the committed subframe-body corpus, partition by partition (the same table
//!   `examples/arm-kernels` and the core's own differentials use);
//! * the synthesized boundaries: every order 0..=31 crossed with the tier
//!   boundaries' quotient and remainder patterns, the `p = 0` unary-only law,
//!   both §9.2.7.3 guards, the five end-of-stream shapes, and the orders the
//!   5-bit field cannot hold (delegation).
//!
//! Every check compares three legs where the domain allows it: the core's
//! `TableCountKernel` (the incumbent, the decode path of record), the assembled
//! kernel through the seam, and the model. Where the kernel's arithmetic stops
//! (`order > MAX_ORDER`) the wrapper delegates; the model is not run there and
//! the check says so by counting the delegation, not by skipping silently.
//!
//! # What it deliberately is not
//!
//! No cycle claim of any kind. 6a-4 is correctness-only; E10's −349/−348
//! cycles/sample was measured in a different image with a different driver, and
//! 6a-5 owns the realization reading. No decode path moves, no whole frame or
//! stream is decoded, and nothing here plays sound. ARM-mode-ness of the body is
//! an **ELF fact** (`make overlay-objdump`: four-byte ARM encodings, `pop
//! {…, lr}` + `bx lr`, never `LDM`-to-PC) — a green sweep could also be green on
//! a Thumb build of the same algorithm.
//!
//! # How it runs
//!
//! * `make overlay-kernel-test` — the `#[test_case]` suite under
//!   `mgba-test-runner`. This is the first ROM suite in the repo with tests in it
//!   (the root crate's suite collects zero), which is what 6a-5 inherits.
//! * `make overlay-kernel-rom` — the same sweeps as a boot log, with the
//!   repo's screen convention: blue = every check agreed, red = a mismatch,
//!   purple = never completed.

#![cfg_attr(target_arch = "arm", no_std)]
#![cfg_attr(target_arch = "arm", no_main)]
#![cfg_attr(test, feature(custom_test_frameworks))]
#![cfg_attr(test, test_runner(agb::test_runner::test_runner))]
#![cfg_attr(test, reexport_test_harness_main = "test_main")]

extern crate alloc;

#[path = "cases.rs"]
mod cases;
#[path = "corpus.rs"]
mod corpus;

// ---------------------------------------------------------------------------
// The sweep. Normal `std`-free code, compiled for the target only — the host has
// `crates/flac-lite-gba/tests/arm_from_outside.rs` for the same cases, where the
// FFI leg is the model.
// ---------------------------------------------------------------------------
#[cfg(target_arch = "arm")]
mod sweep {
    use alloc::format;
    use alloc::string::String;
    use alloc::vec::Vec;
    use flac_lite::bits::BitReader;
    use flac_lite::residual::{TableCountKernel, decode_rice_partition_with};
    use flac_lite_gba::rice_arm::{ArmRiceKernel, MAX_ORDER, tables_iwram_addrs};

    use crate::cases::{self, Outcome};
    use crate::corpus;

    /// `.iwram`'s extent on this hardware (32 KB at `0x0300_0000`). A pinned table
    /// outside it is not pinned to the zero-wait memory the attribute names.
    const IWRAM_START: usize = 0x0300_0000;
    const IWRAM_END: usize = 0x0300_8000;

    /// One family of checks: how many ran, how many disagreed, and the first
    /// disagreement in words. A ROM log line is a family, so a red screen names
    /// itself instead of pointing at "the sweep".
    pub struct Family {
        pub name: &'static str,
        pub checked: usize,
        pub samples: usize,
        pub mismatches: usize,
        pub first: Option<String>,
        /// Cases the host set contains and this memory cannot construct. Printed,
        /// never silently dropped: a family that ran fewer checks than the host's
        /// has to say which ones and why.
        pub skipped_host_only: usize,
    }

    impl Family {
        fn new(name: &'static str) -> Self {
            Self {
                name,
                checked: 0,
                samples: 0,
                mismatches: 0,
                first: None,
                skipped_host_only: 0,
            }
        }

        pub fn ok(&self) -> bool {
            self.mismatches == 0
        }

        fn fail(&mut self, what: String) {
            self.mismatches += 1;
            if self.first.is_none() {
                self.first = Some(what);
            }
        }

        /// One line for the boot log and one for a `#[test_case]` message.
        pub fn line(&self) -> String {
            let status = if self.ok() { "OK" } else { "FAIL" };
            let skipped = if self.skipped_host_only == 0 {
                String::new()
            } else {
                format!(
                    ", {} host-only case(s) not constructed here",
                    self.skipped_host_only
                )
            };
            match &self.first {
                None => format!(
                    "{}: {} checks / {} samples{} [{}]",
                    self.name, self.checked, self.samples, skipped, status
                ),
                Some(first) => format!(
                    "{}: {} checks / {} samples, {} mismatch(es) [{}] first: {}",
                    self.name, self.checked, self.samples, self.mismatches, status, first
                ),
            }
        }
    }

    /// Compare a leg against the incumbent: error kind, samples, cursor bit
    /// position. The first differing sample index is named, because "samples
    /// differ" is not a useful thing to read on a screen.
    fn compare(f: &mut Family, what: &str, incumbent: &Outcome, other: &Outcome, whose: &str) {
        if other.err != incumbent.err {
            f.fail(format!(
                "{what}: {whose} reports {:?}, the incumbent {:?}",
                other.err, incumbent.err
            ));
        }
        if other.samples != incumbent.samples {
            let at = incumbent
                .samples
                .iter()
                .zip(other.samples.iter())
                .position(|(a, b)| a != b)
                .unwrap_or(incumbent.samples.len().min(other.samples.len()));
            f.fail(format!(
                "{what}: {whose} sample {at} is {:?}, the incumbent's is {:?}",
                other.samples.get(at),
                incumbent.samples.get(at)
            ));
        }
        if other.bits_after != incumbent.bits_after {
            f.fail(format!(
                "{what}: {whose} left the cursor at bit {}, the incumbent at {}",
                other.bits_after, incumbent.bits_after
            ));
        }
    }

    /// Decode a bare partition body that starts at `body_at` bits into `frame`,
    /// through one of the two seam kernels. `cases::model_outcome` cannot be used
    /// for the corpus because it starts at bit 0, and the corpus's partitions do
    /// not — so this is the seam leg and [`cases::model_outcome_at`] is the model
    /// leg, at the same offset.
    fn seam_at<K: flac_lite::residual::RiceKernel>(
        frame: &[u8],
        body_at: usize,
        header: &flac_lite::residual::PartitionHeader,
    ) -> Outcome {
        let mut reader = BitReader::new(frame);
        corpus::skip(&mut reader, body_at);
        let mut out = alloc::vec![0i32; usize::from(header.sample_count)];
        let err = decode_rice_partition_with::<K>(&mut reader, header, &mut out).err();
        Outcome {
            samples: out,
            bits_after: reader.bit_position(),
            err,
        }
    }

    /// The committed corpus, partition by partition. This is the arm that makes a
    /// kernel swap visible to the corpus at all (Q10's standing complaint: the
    /// corpus cannot tell one kernel from another on its own).
    pub fn corpus_family() -> Family {
        let mut f = Family::new("corpus partitions");
        for v in corpus::load_corpus() {
            if v.kind != "fixed" && v.kind != "lpc" {
                continue;
            }
            for part in corpus::corpus_partitions(&v) {
                if part.header.is_escape {
                    continue;
                }
                let order = u32::from(part.header.order);
                let n = usize::from(part.header.sample_count);
                let what = format!("vector {} parameter {order} ({n} samples)", v.label);
                let incumbent = seam_at::<TableCountKernel>(&v.frame, part.body_at, &part.header);
                let arm = seam_at::<ArmRiceKernel>(&v.frame, part.body_at, &part.header);
                f.checked += 1;
                f.samples += n;
                compare(&mut f, &what, &incumbent, &arm, "the assembled kernel");
                if order <= MAX_ORDER {
                    let model = cases::model_outcome_at(&v.frame, part.body_at, order, n);
                    compare(&mut f, &what, &incumbent, &model, "the model");
                }
            }
        }
        f
    }

    /// One clean-case leg, shared by the sweep and the `p = 0` arm.
    fn clean_case(f: &mut Family, case: &cases::Case, law: bool) {
        let n = case.expected.len();
        let what = format!(
            "order {} case at {} bits, {n} samples",
            case.order, case.end_bits
        );
        let incumbent = cases::decode_with::<TableCountKernel>(&case.bytes, case.order, n);
        let arm = cases::decode_with::<ArmRiceKernel>(&case.bytes, case.order, n);
        f.checked += 1;
        f.samples += n;

        if incumbent.err.is_some() {
            f.fail(format!(
                "{what}: the incumbent failed ({:?}) on a case the packer built to decode",
                incumbent.err
            ));
            return;
        }
        // The packer's values, which no decoder produced. That is the anchor.
        if arm.samples != case.expected {
            f.fail(format!(
                "{what}: the assembled kernel did not return the packer's samples"
            ));
        }
        if arm.bits_after != case.end_bits {
            f.fail(format!(
                "{what}: the assembled kernel consumed {} bits, the packer wrote {}",
                arm.bits_after, case.end_bits
            ));
        }
        compare(f, &what, &incumbent, &arm, "the assembled kernel");
        if case.order <= MAX_ORDER {
            let model = cases::model_outcome(&case.bytes, case.order, n);
            compare(f, &what, &incumbent, &model, "the model");
        }
        if law {
            // p = 0's unary-only bit law, a different expression from the one the
            // packer laid the bits out with (see the host suite's arm of the same
            // name): a codeword is `folded + 1` bits, and the folded value IS the
            // quotient at this order.
            let want: usize = case
                .expected
                .iter()
                .map(|v| usize::try_from(cases::spec_fold(*v)).expect("a quotient") + 1)
                .sum();
            if want != case.end_bits {
                f.fail(format!(
                    "{what}: the unary-only law says {want} bits, the case is {} bits",
                    case.end_bits
                ));
            }
        }
    }

    /// Every order 0..=31, one order's cases alive at a time (the whole sweep is
    /// ~6,500 cases, which does not fit in EWRAM beside the 256 KB heap).
    pub fn clean_family() -> Family {
        let mut f = Family::new("swept boundaries");
        for order in 0..=31u32 {
            for case in cases::clean_cases_for(order) {
                clean_case(&mut f, &case, false);
            }
        }
        f
    }

    /// The `p = 0` law, its own line because it is the order where an
    /// un-reversal, a mask and a skipped-remainder bug are all invisible at once.
    pub fn order_zero_family() -> Family {
        let mut f = Family::new("p = 0 unary-only law");
        for case in cases::order_zero_affordable_cases() {
            clean_case(&mut f, &case, true);
        }
        // Named, not silent: the extreme case is a 2 MB region (16,777,215 unary
        // bits at p = 0), which is larger than EWRAM. The host's arm of the same
        // name runs it; this one says so.
        f.skipped_host_only = 1;
        f
    }

    /// Both §9.2.7.3 guards. One order's region at a time: the order-12 case is
    /// 128 KB on its own.
    pub fn guard_family() -> Family {
        let mut f = Family::new("section 9.2.7.3 guards");
        // The host runs 12..=16. This machine cannot: a guard case's region is
        // `2^(32-order) + order` bits, so order 12 needs 131 KB and order 13 needs
        // 65 KB — "the §9.2.7.3 bound, constructed", which is the point of the arm
        // — against 256 KB of EWRAM shared with the runtime, the allocator's
        // metadata and this test harness. 14..=16 are 32/16/8 KB and fit; the two
        // the ROM cannot build are counted in `skipped_host_only`, never silently
        // dropped.
        const ROM_GUARD_ORDERS: u32 = 14;
        f.skipped_host_only = (12..ROM_GUARD_ORDERS).len();
        for order in (ROM_GUARD_ORDERS..=16u32).rev() {
            guard_case(&mut f, cases::guard1_case(order), order, "first bound");
            guard_case(&mut f, cases::guard2_case(order), order, "second bound");
        }
        // The order-30 second bound reaches the guard only through the wrapper's
        // delegation — which is exactly the property that arm exists to check.
        guard_case(
            &mut f,
            cases::guard2_case(30),
            30,
            "second bound, delegated",
        );
        f
    }

    fn guard_case(f: &mut Family, case: cases::Case, order: u32, which: &str) {
        use flac_lite::Error;

        let what = format!(
            "order {order} {which} ({} bytes of region)",
            case.bytes.len()
        );
        let incumbent = cases::decode_with::<TableCountKernel>(&case.bytes, order, case.want);
        let arm = cases::decode_with::<ArmRiceKernel>(&case.bytes, order, case.want);
        f.checked += 1;

        if incumbent.err != Some(Error::InvalidField) {
            f.fail(format!(
                "{what}: the incumbent answered {:?}, not a §9.2.7.3 rejection — the \
                 case is no longer a guard case",
                incumbent.err
            ));
            return;
        }
        if arm.err != incumbent.err {
            f.fail(format!(
                "{what}: the assembled kernel answered {:?}, the incumbent {:?}",
                arm.err, incumbent.err
            ));
        }
        if arm.bits_after != incumbent.bits_after {
            f.fail(format!(
                "{what}: the rejection left the cursor at bit {}, the incumbent's at {}",
                arm.bits_after, incumbent.bits_after
            ));
        }
        if order <= MAX_ORDER {
            let model = cases::model_outcome(&case.bytes, order, case.want);
            if model.err != arm.err || model.bits_after != arm.bits_after {
                f.fail(format!(
                    "{what}: the model and the assembled kernel disagree on the \
                     rejection ({:?} at bit {} vs {:?} at bit {})",
                    model.err, model.bits_after, arm.err, arm.bits_after
                ));
            }
        }
    }

    /// The five end-of-stream shapes. Kinds and the samples written before the
    /// failure must agree with the packer; the *cursor* is the one place this
    /// kernel deliberately differs from the incumbent (6a-4 keeps the probe's
    /// ratified "restore nothing" at partition level, while the incumbent restores
    /// per read). That divergence is a reported number, not a failure — the frame
    /// is dead either way — but the model must agree with the assembled kernel
    /// exactly, because they are one mechanism in two realizations.
    pub fn eof_family() -> (Family, usize) {
        use flac_lite::Error;

        let mut f = Family::new("end of stream");
        let mut diverged = 0usize;
        for case in cases::eof_cases() {
            let what = format!(
                "order {} eof arm ({} bits packed, {} asked for)",
                case.order, case.end_bits, case.want
            );
            let incumbent =
                cases::decode_with::<TableCountKernel>(&case.bytes, case.order, case.want);
            let arm = cases::decode_with::<ArmRiceKernel>(&case.bytes, case.order, case.want);
            f.checked += 1;

            if incumbent.err != Some(Error::EndOfStream) {
                f.fail(format!(
                    "{what}: the incumbent answered {:?}, not end of stream",
                    incumbent.err
                ));
                continue;
            }
            let written = case.expected.len();
            if arm.err != incumbent.err {
                f.fail(format!(
                    "{what}: the assembled kernel answered {:?}",
                    arm.err
                ));
            }
            if arm.samples[..written] != case.expected[..] {
                f.fail(format!(
                    "{what}: the samples written before the failure are not the packer's"
                ));
            }
            if arm.bits_after != incumbent.bits_after {
                diverged += 1;
            }
            if case.order <= MAX_ORDER {
                let model = cases::model_outcome(&case.bytes, case.order, case.want);
                if model.err != arm.err || model.bits_after != arm.bits_after {
                    f.fail(format!(
                        "{what}: the model and the assembled kernel disagree on the \
                         unterminated end ({:?} at bit {} vs {:?} at bit {})",
                        model.err, model.bits_after, arm.err, arm.bits_after
                    ));
                }
            }
        }
        (f, diverged)
    }

    /// Orders the 5-bit field cannot hold: the incumbent answers `InvalidField`,
    /// and the wrapper's delegation has to answer the same thing at the same
    /// cursor.
    pub fn out_of_range_family() -> Family {
        use flac_lite::Error;

        let mut f = Family::new("orders the field cannot hold");
        let bytes = [0b0111_1111u8; 24];
        for order in cases::out_of_range_cases() {
            let incumbent = cases::decode_with::<TableCountKernel>(&bytes, order, 1);
            let arm = cases::decode_with::<ArmRiceKernel>(&bytes, order, 1);
            f.checked += 1;
            if incumbent.err != Some(Error::InvalidField) {
                f.fail(format!(
                    "order {order}: the incumbent answered {:?}",
                    incumbent.err
                ));
            }
            if arm.err != incumbent.err || arm.bits_after != incumbent.bits_after {
                f.fail(format!(
                    "order {order}: the delegation answered {:?} at bit {}, the \
                     incumbent {:?} at bit {}",
                    arm.err, arm.bits_after, incumbent.err, incumbent.bits_after
                ));
            }
        }
        f
    }

    /// The three tables' placement, at runtime. This is the half the host cannot
    /// witness: the bytes are pinned by the host suite's FNV digests and
    /// re-derivation, and *where they live* is a property of this link only. The
    /// ROM is built with `iwram-pin` on (its manifest), which is where 6a-4's
    /// enablement lands.
    pub fn tables_family() -> Family {
        let mut f = Family::new("table placement (.iwram)");
        let (ctz8, rev8, bruijn) = tables_iwram_addrs();
        f.checked += 1;
        for (name, addr, bytes) in [
            ("CTZ8", ctz8, 256usize),
            ("REV8", rev8, 256),
            ("BRUIJN_INDEX", bruijn, 32),
        ] {
            if !(IWRAM_START..IWRAM_END).contains(&addr) {
                f.fail(format!(
                    "{name} at {addr:#010x} is outside .iwram ({IWRAM_START:#010x}..{IWRAM_END:#010x}) — \
                     iwram-pin is not on, or the link script did not place the section"
                ));
            }
            if addr + bytes > IWRAM_END {
                f.fail(format!("{name} runs past the end of .iwram"));
            }
        }
        // They are three distinct objects, and the kernel is handed all three
        // pointers — a shared address would make one table stand in for another.
        if ctz8 == rev8 || rev8 == bruijn || ctz8 == bruijn {
            f.fail(format!(
                "the tables overlap: CTZ8 {ctz8:#010x}, REV8 {rev8:#010x}, BRUIJN {bruijn:#010x}"
            ));
        }
        f
    }

    /// The corpus census: what the committed vectors do and do not contain,
    /// asserted rather than assumed (Q10). Same numbers as the host suite's arm;
    /// if the two ever disagree, one of the two consumers is reading a different
    /// table.
    pub fn census_family() -> Family {
        let mut f = Family::new("corpus blindness census");
        let mut partitions = 0usize;
        let mut samples = 0usize;
        let mut escapes = 0usize;
        let mut max_q = 0u32;
        let mut max_p = 0u32;
        for v in corpus::load_corpus() {
            if v.kind != "fixed" && v.kind != "lpc" {
                continue;
            }
            for part in corpus::corpus_partitions(&v) {
                if part.header.is_escape {
                    escapes += 1;
                    continue;
                }
                partitions += 1;
                let p = u32::from(part.header.order);
                max_p = max_p.max(p);
                samples += usize::from(part.header.sample_count);
                for &q in &part.quotients {
                    max_q = max_q.max(q);
                }
            }
        }
        f.checked = partitions;
        f.samples = samples;
        if escapes != 0 {
            f.fail(format!(
                "the corpus gained {escapes} escaped partition(s) — Q10's blindness list \
                 is out of date and has to be re-stated rather than this check widened"
            ));
        }
        if max_p > 8 {
            f.fail(format!(
                "the corpus's largest parameter is now {max_p}; the record says 8"
            ));
        }
        if max_q >= 64 {
            f.fail(format!(
                "the corpus's longest unary run is now {max_q}; tier 3 starts at 16 and \
                 the word-skip at 32, so the synthesized arms stop being their only witness"
            ));
        }
        f
    }

    // The boot path's aggregate: the `#[test_case]` suite runs the families one at
    // a time so a failure names its boundary, and never calls `all()`.
    #[allow(dead_code)]
    pub struct Report {
        pub families: Vec<Family>,
        /// End-of-stream arms whose cursor differs from the incumbent's. Permitted
        /// by the trait and reported, never asserted to be zero.
        pub eof_cursor_divergence: usize,
    }

    impl Report {
        #[allow(dead_code)]
        pub fn ok(&self) -> bool {
            self.families.iter().all(Family::ok)
        }
    }

    /// Every family, in the order the boot log prints them.
    #[allow(dead_code)]
    pub fn all() -> Report {
        let (eof, eof_cursor_divergence) = eof_family();
        Report {
            families: alloc::vec![
                corpus_family(),
                clean_family(),
                order_zero_family(),
                guard_family(),
                eof,
                out_of_range_family(),
                tables_family(),
                census_family(),
            ],
            eof_cursor_divergence,
        }
    }
}

#[cfg(not(target_arch = "arm"))]
fn main() {}

#[cfg(target_arch = "arm")]
#[agb::entry]
fn main(gba: agb::Gba) -> ! {
    #[cfg(test)]
    {
        // The harness installs the `Gba` the test cases receive and never returns.
        agb::test_runner::agb_start_tests(gba, test_main);
    }
    #[cfg(not(test))]
    {
        boot_log(gba);
    }
}

/// The boot path: run every family, print one line each, then hold the screen at
/// blue (all agreed) or red (a mismatch). Purple — the pending colour — means the
/// ROM never got to the end, which is the only state a crash can produce.
#[cfg(all(target_arch = "arm", not(test)))]
fn boot_log(mut gba: agb::Gba) -> ! {
    use agb::display::Rgb;

    const COLOR_PENDING: agb::display::Rgb15 = Rgb::new(128, 0, 255).to_rgb15();
    const COLOR_PASS: agb::display::Rgb15 = Rgb::new(0, 0, 255).to_rgb15();
    const COLOR_FAIL: agb::display::Rgb15 = Rgb::new(255, 0, 0).to_rgb15();

    let mut gfx = gba.graphics.get();
    gfx.set_background_palette_colour(0, 0, COLOR_PENDING);

    agb::eprintln!(
        "[6a-4] entry started — overlay kernel-check ROM (correctness only: no timing, \
         no decode path, no sound)"
    );

    let report = sweep::all();
    for family in &report.families {
        agb::println!("[6a-4] {}", family.line());
    }
    agb::println!(
        "[6a-4] end-of-stream cursor divergence: {} arm(s) leave the cursor where the \
         incumbent does not (permitted by the seam; the frame is dead either way)",
        report.eof_cursor_divergence
    );
    let ok = report.ok();
    agb::println!(
        "[6a-4] verdict: {} ({} families)",
        if ok {
            "BLUE — every check agreed"
        } else {
            "RED — mismatch"
        },
        report.families.len()
    );
    gfx.set_background_palette_colour(0, 0, if ok { COLOR_PASS } else { COLOR_FAIL });

    loop {
        let frame = gfx.frame();
        frame.commit();
    }
}

// ---------------------------------------------------------------------------
// The `#[test_case]` suite — the first ROM suite in this repo with a test in it.
// One case per family, so a failure names its boundary instead of "the sweep".
// ---------------------------------------------------------------------------
#[cfg(all(test, target_arch = "arm"))]
mod tests {
    use agb::Gba;

    /// Every case in this module ends the same way: run one family, print its
    /// line (so the emulator log carries the counts even on success), assert.
    macro_rules! family_case {
        ($name:ident, $family:expr, $what:literal) => {
            #[test_case]
            fn $name(_gba: &mut Gba) {
                let family = $family;
                agb::println!("[6a-4] {}", family.line());
                assert!(family.ok(), "{}: {}", $what, family.line());
                assert!(family.checked > 0, "{}: the family ran zero checks", $what);
            }
        };
    }

    family_case!(
        the_committed_corpus_decodes_identically_through_the_assembled_kernel,
        crate::sweep::corpus_family(),
        "corpus partitions"
    );
    family_case!(
        the_swept_boundaries_decode_identically_through_the_assembled_kernel,
        crate::sweep::clean_family(),
        "swept boundaries"
    );
    family_case!(
        p_equal_zero_carries_the_whole_value_in_the_unary_run,
        crate::sweep::order_zero_family(),
        "p = 0"
    );
    family_case!(
        both_guards_reject_at_the_same_point_the_incumbent_does,
        crate::sweep::guard_family(),
        "section 9.2.7.3 guards"
    );
    family_case!(
        orders_the_field_cannot_hold_are_rejected_by_the_delegation,
        crate::sweep::out_of_range_family(),
        "out-of-range orders"
    );
    family_case!(
        the_three_tables_are_pinned_inside_iwram,
        crate::sweep::tables_family(),
        "table placement"
    );
    family_case!(
        the_committed_corpus_still_blinds_the_boundaries_it_was_blinding,
        crate::sweep::census_family(),
        "corpus census"
    );

    #[test_case]
    fn an_unterminated_codeword_fails_the_same_kind_and_the_model_agrees(_gba: &mut Gba) {
        let (family, diverged) = crate::sweep::eof_family();
        agb::println!("[6a-4] {}", family.line());
        agb::println!(
            "[6a-4] end-of-stream cursor divergence: {diverged} of {} arm(s) — reported, \
             not failed: the trait promises only per-read atomicity",
            family.checked
        );
        assert!(family.ok(), "end of stream: {}", family.line());
        assert_eq!(family.checked, 5, "all five end-of-stream arms must run");
    }
}
