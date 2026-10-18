//! Rate ROM — real audio at a mixer-domain sample rate, measured (FLAC.md
//! "Sample-rate reduction — tooling and profiling proposal (2026-10-11)",
//! Part 1 steps 1+2+3).
//!
//! PURPOSE: answer, with real material instead of synthesized tones, the
//! question the 32 kHz gate could not: does a full track decode inside the
//! real-time budget at a LOWER playback rate? The budget derives at the
//! encode rate (`BUDGET_CYCLES_PER_FRAME` from the generated module, itself
//! `stats::real_time_budget(2048, RATE_HZ)` — derived, never recalled).
//!
//! COEXISTENCE CONTRACT (why this is a separate bin, not an edit to
//! main.rs): the committed spike ROM is the E-menu's calibration anchor —
//! its embedded 32 kHz clips and its measured-image code layout are what
//! every recorded baseline and cross-session control comparison judges.
//! Refactoring main.rs's harness into shared code would move that layout
//! (micro-windows are layout-sensitive by ±4 cycles; control stats drift a
//! small constant per shape — spike harness notes), so the harness below is
//! a deliberate duplication of main.rs's measurement plumbing. What is NOT
//! duplicated is anything carrying a pin: decode goes through the shared
//! `driver`, the PCM fold through the shared `checksum::fold_i16le_stereo`,
//! the stats fold and budget through the shared `stats` — host witness
//! (`make rate-test`) drives those same library functions against the
//! generator's pins on the host before any ROM number counts.
//!
//! Assets are TEMPORARY and generated: `make rate-assets INPUT=… RATE=…`
//! runs scripts/gen_rate_assets.sh, which fail-closes on the mixer domain
//! {10512, 18157, 32768}, encode rate == playback rate per FRAME, the
//! 16-bit/2048-blocksize profile, and the subframe-walk census. The
//! generated modules (src/rate_assets.rs, src/rate_pcm.rs — gitignored,
//! `make clean` removes) are `include!`d below; building this bin without
//! them fails at the `include!` naming the missing file. The bin is
//! `required-features = ["rate-assets"]`, so `make native-spike-rom` /
//! `make spike-test` never compile it.
//!
//! Harness deltas from main.rs (each a measured fact, not a preference):
//! - **soft_reset before EVERY window** (inside `timed`): this pass walks
//!   ~1,328 frames of a full track; the LPC arm at ≈1,770 c/sample is a
//!   projected ≈4.8 G cycles — past the free-running pair's 2^32 (≈256 s)
//!   rollover. FLAC's committed harness never crossed it (biggest pass
//!   1.08 G); the Opus port measured the artifact and the rule: zero the
//!   pair per window, unconditionally and OUTSIDE the timed span, so every
//!   window still enters with byte-identical preceding code (E4 rule) and
//!   the wrap sentinel regains its meaning (end < start can then only mean
//!   THIS window exceeded 2^32; a single frame is ≈3.6 M cycles, far below).
//! - The screen verdict ANDs decode-proof with budget at the DERIVED rate
//!   budget; correctness-before-speed is unchanged: no timing on an arm
//!   whose decode proof failed on its own embedded bytes.
//!
//! Not in scope here (deliberately, Sam 2026-10-17): the mixer/DMA playback
//! path (Phase 2 step 9) and the cadence alternation — this ROM answers
//! "does the DECODE fit the budget at this rate on this material", measured
//! the same IRQ-off way the E-menu measures everything else.

#![cfg_attr(target_arch = "arm", no_std)]
#![cfg_attr(target_arch = "arm", no_main)]

#[cfg(target_arch = "arm")]
extern crate alloc;

#[cfg(target_arch = "arm")]
mod rom {
    use agb::display::Rgb;
    use agb::timer::Divider;
    use flac_lite::subframe::PredictorState;
    use flac_spike::assets::SpikeClip;
    use flac_spike::checksum::{Fnv1a64, fnv1a64, fold_i16le_stereo};
    use flac_spike::driver::{self, DriverError};
    use flac_spike::stats::{FrameCost, WORST_WRAPPED, cycles_between};

    // Generated, temporary modules (make rate-assets). `//!` inner docs
    // land legally because the expansion sits at the head of each module
    // body; include! keeps rustfmt from walking generated code.
    pub mod rate {
        include!("rate_assets.rs");
    }

    const COLOR_PASS: agb::display::Rgb15 = Rgb::new(0, 0, 255).to_rgb15();
    const COLOR_FAIL: agb::display::Rgb15 = Rgb::new(255, 0, 0).to_rgb15();
    const COLOR_PENDING: agb::display::Rgb15 = Rgb::new(128, 0, 255).to_rgb15();

    /// Calibration windows per arm: 3 discarded + 5 reported — the E4
    /// verified unified-loop shape (byte-identical iterations, unconditional
    /// stores, NOTHING logged between windows; spike harness notes).
    /// Measured on this ROM's first build (sha b8dcf256…): the committed
    /// 5-logged-windows shape came back 26/22/22/22/22 on BOTH arms —
    /// deterministic, not noise: the first micro-window after the decode-
    /// proof's log burst carries the cold-window artifact (+4) — the same
    /// micro-window sensitivity the harness notes record (the first draft's
    /// conditional store measured 24 vs 22; E1's first window read cold).
    /// The plateau is the overhead; discarding warm-up is the recorded
    /// recovery, and it reproduces the committed harness's 22-cycle figure.
    const CALIBRATION_SAMPLES: usize = 5;
    const CALIBRATION_DISCARDED: usize = 3;

    // GBA interrupt registers (IO region, always mapped, 16-bit).
    const REG_IE: usize = 0x0400_0200;
    const REG_IF: usize = 0x0400_0202;
    const REG_IME: usize = 0x0400_0208;

    /// One 16-bit read from the GBA I/O region.
    ///
    /// # Safety
    /// `addr` must be an even address in the memory-mapped I/O region
    /// (0x0400_0000..0x0400_0400). Callers pass the REG_* constants above.
    unsafe fn io_read_u16(addr: usize) -> u16 {
        unsafe { (addr as *const u16).read_volatile() }
    }

    /// Asset verification (PR 1 shape): seek-table structure + region pin —
    /// the bytes about to be decoded are the generator's bytes.
    fn check_clip(clip: &SpikeClip) -> bool {
        agb::println!(
            "[rate] arm={} frames={} region={}B samples={} rate={} bps={} ch={} max_blocksize={}",
            clip.name,
            clip.frames.len(),
            clip.region.len(),
            clip.total_samples,
            clip.sample_rate_hz,
            clip.bits_per_sample,
            clip.channels,
            clip.max_blocksize
        );
        agb::println!("[rate] census({}): {}", clip.name, clip.census);

        let mut pass = true;
        pass &= !clip.frames.is_empty();
        pass &= clip.frames.first().is_some_and(|f| f.offset == 0);
        let mut ascending = true;
        let mut within_max = true;
        let mut within_region = true;
        let mut prev = clip.frames.first().map_or(0, |f| f.offset);
        for frame in clip.frames.iter().skip(1) {
            ascending &= frame.offset > prev;
            prev = frame.offset;
        }
        for frame in clip.frames {
            within_max &= usize::from(frame.blocksize) <= usize::from(clip.max_blocksize);
            within_region &=
                usize::try_from(frame.offset).is_ok_and(|o| (o + 4) <= clip.region.len());
        }
        pass &= ascending & within_max & within_region;
        agb::println!(
            "[rate] {} seek table: ascending={} within_max={} within_region={}",
            clip.name,
            ascending,
            within_max,
            within_region
        );

        // The encode rate == playback rate == derived-budget rate witness,
        // from the ROM's side: the table's rate field vs the generator's
        // pinned RATE_HZ vs the budget's own construction rate.
        pass &= clip.sample_rate_hz == rate::RATE_HZ;
        let actual = fnv1a64(clip.region);
        let ok = actual == clip.fnv_frames;
        pass &= ok;
        agb::println!(
            "[rate] {} region fnv: expected 0x{:016X} actual 0x{:016X} [{}]",
            clip.name,
            clip.fnv_frames,
            actual,
            if ok { "OK" } else { "MISMATCH" }
        );
        pass
    }

    /// On-target decode proof, one arm (PR 2 shape): full-clip walk through
    /// the shared driver, PCM folded through the shared fold, final hash vs
    /// the generator's `flac -d` pin.
    fn decode_proof(clip: &SpikeClip) -> Option<(&'static str, u64, usize)> {
        let name = clip.name;
        let max = usize::from(clip.max_blocksize);
        let mut left: alloc::vec::Vec<i32> = alloc::vec![0; max];
        let mut right: alloc::vec::Vec<i32> = alloc::vec![0; max];
        agb::println!(
            "[rate] {} checkpoint: decode buffers ready ({} samples × {}B each; left=0x{:08X} right=0x{:08X})",
            name,
            max,
            core::mem::size_of::<i32>(),
            left.as_ptr() as usize,
            right.as_ptr() as usize
        );

        let mut state = [PredictorState::new(), PredictorState::new()];
        let mut hash = Fnv1a64::new();
        let mut fold_failure: Option<(usize, i32)> = None;

        agb::println!(
            "[rate] {} decode proof: walking {} frames via seek table",
            name,
            clip.frames.len()
        );
        let walk = driver::decode_clip(clip, &mut left, &mut right, &mut state, |frame, l, r| {
            match fold_i16le_stereo(&mut hash, l, r) {
                Ok(()) => {
                    if frame % 64 == 0 || frame + 1 == clip.frames.len() {
                        // Full tracks are 1,328 frames/arm; log every 64th
                        // running hash (a mismatch localizes between two
                        // logged frames) instead of all — the committed
                        // 157-frame clips could afford every line.
                        agb::println!(
                            "[rate] {} frame {:>4}/{} off={} bs={} running=0x{:016X}",
                            name,
                            frame,
                            clip.frames.len(),
                            usize::try_from(clip.frames[frame].offset).unwrap_or(usize::MAX),
                            l.len(),
                            hash.finish()
                        );
                    }
                }
                Err(sample) => {
                    if fold_failure.is_none() {
                        fold_failure = Some((frame, sample));
                    }
                    agb::println!(
                        "[rate] {} FAIL frame {:>4}: decoded sample {} outside 16-bit profile (walk continues for localization)",
                        name,
                        frame,
                        sample
                    );
                }
            }
        });

        let stats = match walk {
            Ok(stats) => stats,
            Err(err) => {
                let described = match &err {
                    DriverError::BufferTooSmall { need, have } => {
                        alloc::format!("buffer too small (need {} have {})", need, have)
                    }
                    DriverError::FrameOffsetOutOfRange { frame, offset, len } => {
                        alloc::format!(
                            "frame {} offset {} past region len {} (table/blob mismatch)",
                            frame,
                            offset,
                            len
                        )
                    }
                    DriverError::MetadataMismatch {
                        frame,
                        table,
                        header,
                    } => {
                        alloc::format!(
                            "frame {} blocksize table={} header={} (asset corruption)",
                            frame,
                            table,
                            header
                        )
                    }
                    DriverError::Decode(e) => {
                        alloc::format!("flac-lite rejected the stream: {:?}", e)
                    }
                };
                agb::println!("[rate] {} FAIL decode walk aborted: {}", name, described);
                return None;
            }
        };

        let expected_samples = usize::try_from(clip.total_samples).unwrap_or(usize::MAX);
        let mut pass = stats.samples == expected_samples && stats.frames == clip.frames.len();
        agb::println!(
            "[rate] {} totals: frames={} (want {}) samples={} (want {}) [{}]",
            name,
            stats.frames,
            clip.frames.len(),
            stats.samples,
            expected_samples,
            if pass { "OK" } else { "MISMATCH" }
        );

        if let Some((frame, sample)) = fold_failure {
            agb::println!(
                "[rate] {} FAIL first out-of-range sample {} at frame {}",
                name,
                sample,
                frame
            );
            pass = false;
        }

        let actual = hash.finish();
        let pin_ok = actual == clip.fnv_pcm;
        pass &= pin_ok;
        agb::println!(
            "[rate] {} pcm fnv: expected 0x{:016X} actual 0x{:016X} [{}]",
            name,
            clip.fnv_pcm,
            actual,
            if pin_ok { "MATCH" } else { "MISMATCH" }
        );

        if pass {
            Some((name, actual, stats.samples))
        } else {
            None
        }
    }

    /// The free-running cycle counter: timer2 /1 cascading into timer3,
    /// read as a 32-bit composite (low, high, low-stability recheck).
    /// Same mechanism as the committed harness; see its module docs for
    /// why the timer pair beats the DIV counter.
    struct Counter<'a> {
        low: &'a mut agb::timer::Timer,
        high: &'a mut agb::timer::Timer,
    }

    impl Counter<'_> {
        fn start<'a>(timers: &'a mut agb::timer::Timers<'_>) -> Counter<'a> {
            timers
                .timer2
                .set_divider(Divider::Divider1)
                .set_enabled(true);
            timers
                .timer3
                .set_cascade(true)
                .set_interrupt(false)
                .set_enabled(true);
            let mut counter = Counter {
                low: &mut timers.timer2,
                high: &mut timers.timer3,
            };
            counter.soft_reset();
            counter
        }

        /// Freeze, zero both halves, unfreeze.
        fn soft_reset(&mut self) {
            self.low.set_enabled(false);
            self.high.set_enabled(false);
            self.high.set_overflow_amount(0);
            self.low.set_overflow_amount(0);
            self.low.set_enabled(true);
            self.high.set_enabled(true);
        }

        fn read(&self) -> u32 {
            let mut high;
            let mut low;
            for _ in 0..4 {
                let low1 = self.low.value();
                high = self.high.value();
                low = self.low.value();
                if low >= low1 {
                    return (u32::from(high) << 16) | u32::from(low);
                }
            }
            high = self.high.value();
            low = self.low.value();
            (u32::from(high) << 16) | u32::from(low)
        }
    }

    /// Run `work` inside one measurement window. The rate-pass delta: the
    /// counter is soft-reset at the top, OUTSIDE the window, before every
    /// timed span (calibration included, so every window still enters with
    /// byte-identical preceding code). A pass that can span > 2^32 cycles
    /// must not free-run across windows (Opus port's measured rollover
    /// artifact); per-window zeroing makes end < start mean only THIS
    /// window exceeded 2^32. All logging happens outside the window.
    fn timed<T>(counter: &mut Counter<'_>, work: impl FnOnce() -> T) -> (T, u32) {
        counter.soft_reset();
        let mut raw = 0u32;
        let out = agb::external::critical_section::with(|_cs| {
            let start = counter.read();
            let out = work();
            let end = counter.read();
            raw = cycles_between(start, end);
            out
        });
        (out, raw)
    }

    /// One arm's timing pass (PR 3 shape + per-window reset). A budget miss
    /// is NOT a harness failure: it is the measurement, and the verdict
    /// line distinguishes the two.
    fn timing_pass(clip: &SpikeClip, counter: &mut Counter<'_>) -> Option<(FrameCost, u32, bool)> {
        let name = clip.name;
        let max = usize::from(clip.max_blocksize);
        let mut left: alloc::vec::Vec<i32> = alloc::vec![0; max];
        let mut right: alloc::vec::Vec<i32> = alloc::vec![0; max];
        let mut state = [PredictorState::new(), PredictorState::new()];

        let mut ime_inside = 0xFFFFu16;
        let mut raws = [0u32; CALIBRATION_DISCARDED + CALIBRATION_SAMPLES];
        // One byte-identical loop: every iteration — discarded warm-up and
        // reported alike — enters timed() with exactly the code the previous
        // iteration ran (unconditional stores, no logging between windows;
        // logging happens after the whole loop). That is what makes the
        // reported windows' stability meaningful rather than the first
        // window's cold artifact mislabeled as noise.
        for slot in raws.iter_mut() {
            let (probe, raw) = timed(counter, || {
                // SAFETY: REG_IME is an even address in the I/O region.
                unsafe { io_read_u16(REG_IME) }
            });
            ime_inside = probe;
            *slot = raw;
        }
        let reported = &raws[CALIBRATION_DISCARDED..];
        for (i, raw) in raws.iter().enumerate() {
            agb::println!(
                "*** {} calib {}/{} raw={} ({})",
                name,
                i + 1,
                raws.len(),
                raw,
                if i < CALIBRATION_DISCARDED {
                    "discarded warm-up"
                } else {
                    "reported"
                }
            );
        }
        let overhead = reported[0];
        let stable = reported.iter().all(|r| *r == overhead);
        agb::println!(
            "*** {} calibration: overhead_estimate={} stable={} (≈0 witness: reported raws all == estimate; discarded = cold-window warm-up)",
            name,
            overhead,
            stable
        );
        if !stable {
            agb::println!(
                "*** {} FAIL calibration unstable — harness noise, numbers void",
                name
            );
            return None;
        }
        if ime_inside != 0 {
            agb::println!(
                "*** {} FAIL IME={} inside window (expected 0) — windows were interruptable",
                name,
                ime_inside
            );
            return None;
        }

        agb::println!(
            "*** {} timing pass: walking {} frames via seek table (IRQ-off windows, per-window counter reset)",
            name,
            clip.frames.len()
        );
        let mut cost = FrameCost::new();
        for frame_index in 0..clip.frames.len() {
            let (result, raw) = timed(counter, || {
                driver::decode_one(clip, frame_index, &mut left, &mut right, &mut state)
            });
            match result {
                Ok(written) => {
                    let net = raw.saturating_sub(overhead);
                    cost.observe(frame_index, net);
                    if frame_index % 64 == 0 || frame_index + 1 == clip.frames.len() {
                        agb::println!(
                            "*** {} frame {:>4}/{} bs={} raw={} net={}",
                            name,
                            frame_index,
                            clip.frames.len(),
                            written,
                            raw,
                            net
                        );
                    }
                }
                Err(err) => {
                    agb::println!(
                        "*** {} FAIL frame {}: driver rejected in timing pass after decode proof passed: {:?}",
                        name,
                        frame_index,
                        err
                    );
                    return None;
                }
            }
        }

        let mut pass = cost.count == clip.frames.len();
        if !pass {
            agb::println!(
                "*** {} FAIL timed frames={} expected={}",
                name,
                cost.count,
                clip.frames.len()
            );
        }
        if cost.wrapped {
            pass = false;
            agb::println!(
                "*** {} FAIL a window exceeded the 32-bit composite (per-window reset — so this frame alone spanned > 2^32): frame {} at sentinel {}",
                name,
                cost.max_frame,
                WORST_WRAPPED
            );
        }
        agb::println!(
            "*** {} stats: count={} min={}@{} max={}@{} sum={} wrapped={}",
            name,
            cost.count,
            cost.min,
            cost.min_frame,
            cost.max,
            cost.max_frame,
            cost.sum,
            cost.wrapped
        );
        agb::println!(
            "*** {} mean = sum/count = {}/{} — derived host-side (ROM division-free)",
            name,
            cost.sum,
            cost.count
        );
        let fits = cost.max <= rate::BUDGET_CYCLES_PER_FRAME;
        agb::println!(
            "*** {} budget: worst_frame={} worst_net={} vs budget={} ({} Hz, derived) [{}]",
            name,
            cost.max_frame,
            cost.max,
            rate::BUDGET_CYCLES_PER_FRAME,
            rate::RATE_HZ,
            if fits { "FITS" } else { "OVER BUDGET" }
        );
        if pass {
            Some((cost, overhead, fits))
        } else {
            None
        }
    }

    #[agb::entry]
    fn main(mut gba: agb::Gba) -> ! {
        agb::eprintln!("[rate] entry started — rate ROM: real audio, derived budget");
        agb::eprintln!(
            "[rate] input={} sha256={} rate={} Hz budget/frame={}",
            rate::INPUT_NAME,
            rate::INPUT_SHA256,
            rate::RATE_HZ,
            rate::BUDGET_CYCLES_PER_FRAME
        );

        let mut gfx = gba.graphics.get();
        gfx.set_background_palette_colour(0, 0, COLOR_PENDING);

        // Checkpoint 1 — asset verification.
        let mut pass = true;
        for clip in rate::RATE_CLIPS.iter().copied() {
            pass &= check_clip(clip);
        }
        agb::println!(
            "[rate] checkpoint: asset verification {} (region pins — decode below runs on these bytes)",
            if pass { "PASSED" } else { "FAILED" }
        );
        let assets_pass = pass;

        // Checkpoint 2 — the on-target decode proof, per arm.
        let mut proofs: [Option<(&'static str, u64, usize)>; 2] = [None, None];
        for (slot, clip) in rate::RATE_CLIPS.iter().enumerate() {
            proofs[slot] = decode_proof(clip);
            pass &= proofs[slot].is_some();
        }

        // Checkpoint 3 — cross-arm agreement: both arms encode the same
        // normalized source, so their PCM hashes must match each other.
        if let (Some((_, h0, _)), Some((_, h1, _))) = (proofs[0], proofs[1]) {
            let agree = h0 == h1;
            pass &= agree;
            agb::println!(
                "[rate] cross-arm pcm: l0=0x{:016X} l4=0x{:016X} agree={} [{}]",
                h0,
                h1,
                agree,
                if agree { "OK" } else { "MISMATCH" }
            );
        }

        // Checkpoint 4 — timing. Correctness-before-speed: no numbers from
        // an unproven image.
        let proofs_pass = proofs.iter().all(Option::is_some);
        pass &= proofs_pass;
        agb::println!(
            "[rate] checkpoint: decode proof {} — timing pass {}",
            if proofs_pass { "PASSED" } else { "FAILED" },
            if proofs_pass {
                "engaged"
            } else {
                "SKIPPED (correctness-before-speed: no numbers from an unproven image)"
            }
        );
        let mut timing: [Option<(FrameCost, u32, bool)>; 2] = [None, None];
        if proofs_pass {
            // SAFETY: REG_IE / REG_IF / REG_IME are even I/O-region addresses.

            agb::println!(
                "[rate] irq state before timing: IE=0x{:04X} IF=0x{:04X} IME={}",
                unsafe { io_read_u16(REG_IE) },
                unsafe { io_read_u16(REG_IF) },
                unsafe { io_read_u16(REG_IME) }
            );
            let mut timers = gba.timers.timers();
            let mut counter = Counter::start(&mut timers);
            for (slot, clip) in rate::RATE_CLIPS.iter().enumerate() {
                timing[slot] = timing_pass(clip, &mut counter);
                pass &= timing[slot].as_ref().is_some_and(|t| t.2);
            }
            agb::println!(
                "[rate] irq state after timing: IE=0x{:04X} IF=0x{:04X} IME={} (IME=1 means the harness restored it)",
                unsafe { io_read_u16(REG_IE) },
                unsafe { io_read_u16(REG_IF) },
                unsafe { io_read_u16(REG_IME) }
            );
        }

        let (color, label) = if pass {
            (COLOR_PASS, "BLUE")
        } else {
            (COLOR_FAIL, "RED")
        };
        gfx.set_background_palette_colour(0, 0, color);
        agb::eprintln!(
            "[rate] verdict {} — screen {} (assets={}, decode proof={}, measured {}/{}, within budget {}/{})",
            if pass { "PASSED" } else { "FAILED" },
            label,
            assets_pass,
            proofs_pass,
            timing.iter().filter(|t| t.is_some()).count(),
            rate::RATE_CLIPS.len(),
            timing.iter().flatten().filter(|t| t.2).count(),
            rate::RATE_CLIPS.len()
        );

        loop {
            let frame = gfx.frame();
            frame.commit();
        }
    }
}

/// Host-build stub (same dual-target pattern as main.rs): the ROM entry
/// exists only for `thumbv4t-none-eabi`; `cargo test` never builds this bin
/// (`test = false`, and `required-features` keeps it out of default builds).
#[cfg(not(target_arch = "arm"))]
fn main() {}
