//! Perf-gate spike ROM — PR 3: cycle harness + per-frame cost.
//!
//! PURPOSE (see ../../../FLAC.md → "Perf gate spike — concrete plan and
//! validation process"): this ROM is the measurement vehicle for the go/no-go
//! perf gate. PR 1 landed the vehicle (boot + embedded region hashes); PR 2
//! added the **on-target decode proof** (every frame of both embedded clips
//! decoded, PCM folded into FNV-1a-64, compared against the generator's
//! `fnv_pcm` pins); PR 3 — this — adds the **cycle harness**: after the
//! decode-proof layers pass, every frame of both arms is decoded again inside
//! a free-running 16.78 MHz counter window, and min/max/sum/worst-frame-index
//! per arm go to mGBA serial. What it still does NOT do: cadence/buffer
//! alternation (PR 4), verdict table/hardware run (PR 5).
//!
//! Counter mechanism: timer2 at /1 (1 cycle/tick at 16.78 MHz, free-running
//! from 0) cascading into timer3, read as a 32-bit composite (low, high,
//! low-stability recheck). Chosen over the DIV result counter because the
//! timer pair is documented GBA hardware driven entirely through agb 0.25's
//! own safe API (`set_divider`/`set_cascade`/`set_enabled`/`value`), while
//! the DIV idle-cycle-counter behavior would rest on an unverified emulator
//! claim. Windows larger than u32::MAX (the pair wrapped mid-decode) cannot
//! be recovered from two composite reads and saturate to
//! `stats::WORST_WRAPPED` — reported as worst and failed loudly, never
//! silently truncated.
//!
//! Measurement discipline (FLAC.md plan, PR 3 line):
//! - **Empty-window calibration first**: the window with no work inside is
//!   sampled 5 times; its (stable, deterministic) raw value is the harness
//!   overhead subtracted from every frame. Raw is reported alongside net on
//!   every line. The calibration samples' own net is the ≈0 witness.
//! - **IRQs off inside every window**: the window runs inside a
//!   `critical_section` (agb's own IME-disable mechanism, safe API), with
//!   IME/IE/IF register values logged on serial before, inside, and after.
//! - **Division stays out of the ROM**: per-arm sum + count are logged; the
//!   mean is derived as arithmetic host-side (the decode path's
//!   division-free rule extends to the reporting path).
//! - **Correctness-before-speed**: the timing pass only runs on an arm whose
//!   PR 2 decode proof passed, and the screen verdict ANDs the decode-proof
//!   layer with the budget check — timing numbers only count on an image
//!   whose decode passed on its own embedded bytes, and the log says so.
//!
//! Why the fold is the proof: the ROM decodes its **own embedded bytes** and
//! hashes the result, so correctness is established on the exact image PRs
//! 3–5 will measure — the plan's rule that no perf number is ever reported
//! from an image whose decode is not proven on its own bytes (a fast wrong
//! decode measures nothing). The fold itself is `checksum::fold_i16le_stereo`,
//! a library function the host witness (`make spike-test`) drives through the
//! same `driver::decode_clip` and asserts against the same pins — host and
//! ROM run literally the same fold code, so they cannot share a
//! transcription mistake of the interleave rule.
//!
//! The checks run in strict order, each a named serial checkpoint:
//! 1. asset verification (PR 1): seek-table invariants + region hash pins —
//!    the bytes about to be decoded are the committed bytes;
//! 2. per-arm decode proof: full-clip walk via the driver, per-frame serial
//!    log with the running hash (a mismatch localizes to the first frame
//!    whose running hash diverges from the host's same-sequence dump), then
//!    final-hash-vs-pin;
//! 3. cross-arm agreement: both arms decode the same source, so their
//!    on-target PCM hashes must match each other as well as their pins.
//!
//! Screen verdict follows the BitReader PoC's convention (works on cart with
//! no cable): **blue = every check passed, red = any failed, purple = the
//! checks never completed.** BLUE now means *decode proven on-target* —
//! strictly stronger than PR 1's asset-integrity blue.
//!
//! Host-build note: the entire ROM lives behind `cfg(target_arch = "arm")`.
//! `agb` is a target-gated dependency (Cargo.toml), so a host build of this
//! bin must not reference it — on host the bin is a no-op stub. Only the
//! thumbv4t ROM build (make native-spike-rom) compiles the real entry.

#![cfg_attr(target_arch = "arm", no_std)]
#![cfg_attr(target_arch = "arm", no_main)]

#[cfg(target_arch = "arm")]
extern crate alloc;

// Split-ladder item 6 PR 6a-3's codegen arm (see that file's header for the arms
// and the one variable). Gated so that NEITHER feature compiles it at all: the
// control build of this instrument is then "the manifest edge exists and the file
// is in the tree", which is the exact thing 6a-1's control discipline asks to be
// proven free — that adding a crate to this tree does not change what the shipped
// ROMs link. A file no build compiles is the honest shape of that control; the
// record says so rather than dressing it up.
#[cfg(any(feature = "item6a3-twin", feature = "item6a3-hybrid"))]
mod item6a3_probe;

// [6a-7b] the on-screen summary's `agb` half (see `src/item6a7b.rs`): the font,
// the layout, and the fit check — everything that cannot exist off the GBA
// target. Declared at the crate root so the file is `src/item6a7b.rs` rather
// than `src/rom/item6a7b.rs`; gated on `item6a7-wide` so every image except the
// 6a-7 one keeps the bytes its record quotes.
#[cfg(all(target_arch = "arm", feature = "item6a7-wide"))]
mod item6a7b;

#[cfg(target_arch = "arm")]
mod rom {
    // [6a-7b] the summary's `agb` half — see the declaration at the crate root.
    #[cfg(feature = "item6a7-wide")]
    use crate::item6a7b;
    use agb::display::Rgb;
    use agb::timer::Divider;
    use flac_lite::subframe::PredictorState;
    use flac_spike::assets::{CLIPS, SpikeClip};
    use flac_spike::checksum::{Fnv1a64, fnv1a64, fold_i16le_stereo};
    use flac_spike::driver::{self, DriverError};
    use flac_spike::stats::{FrameCost, WORST_WRAPPED, cycles_between, real_time_budget};

    const COLOR_PENDING: agb::display::Rgb15 = Rgb::new(128, 0, 255).to_rgb15();
    const COLOR_PASS: agb::display::Rgb15 = Rgb::new(0, 0, 255).to_rgb15();
    const COLOR_FAIL: agb::display::Rgb15 = Rgb::new(255, 0, 0).to_rgb15();

    /// Real-time decode budget per frame at the initial profile, derived
    /// through the shared `stats::real_time_budget` at compile time (never
    /// recalled, never divides at runtime): 2048 samples / 32768 Hz = 1/16 s
    /// = 0.0625 s; 0.0625 s × 16,780,000 Hz = 1,048,750 cycles/frame —
    /// the FLAC.md plan's number.
    ///
    /// Since plan item 7-0 this is also the window the embedded frames
    /// actually get: a stream is encoded at the rate it is played at, so the
    /// budget the screen verdict compares against and the stream's own
    /// nominal frame period are one and the same number —
    /// `tests/spike_witness.rs` pins the two derivations meeting.
    /// Before 7-0 the clips declared 32,000 Hz, whose true window is
    /// 1,073,920: this budget was then 2.34375% tighter than real-time at
    /// the declared rate, and that gap was the Q14 defect, not a margin.
    /// The screen verdict compares each arm's worst NET window against this.
    const BUDGET_CYCLES_PER_FRAME: u32 = real_time_budget(2048, 32_768);

    /// Calibration samples per arm. Determinism is the premise of the whole
    /// harness, so the calibration doubles as a determinism probe: 5 empty
    /// windows must come back raw-identical before any frame is timed.
    const CALIBRATION_SAMPLES: usize = 5;

    // GBA interrupt registers (IO region, always mapped, 16-bit): IE, IF,
    // IME. Read-only here — logged to witness the IRQ-off discipline.
    const REG_IE: usize = 0x0400_0200;
    const REG_IF: usize = 0x0400_0202;
    const REG_IME: usize = 0x0400_0208;

    /// One 16-bit read from the GBA I/O region.
    ///
    /// # Safety
    /// `addr` must be an even address in the memory-mapped I/O region
    /// (0x0400_0000..0x0400_0400). Reads there have no software invariants:
    /// the hardware always answers, volatile semantics are exactly what MMIO
    /// wants, and a u16-aligned read is the correct access width for these
    /// registers. Callers pass the REG_* constants above.
    unsafe fn io_read_u16(addr: usize) -> u16 {
        unsafe { (addr as *const u16).read_volatile() }
    }

    /// PR 1 checkpoint: metadata sanity from the manifest, the seek table's
    /// structural invariants, and the region hash pin. Logs everything so
    /// mGBA's serial output is the human-readable record.
    fn check_clip(clip: &SpikeClip) -> bool {
        agb::println!(
            "[spike] arm={} frames={} region={}B samples={} rate={} bps={} ch={} max_blocksize={}",
            clip.name,
            clip.frames.len(),
            clip.region.len(),
            clip.total_samples,
            clip.sample_rate_hz,
            clip.bits_per_sample,
            clip.channels,
            clip.max_blocksize
        );
        agb::println!("[spike] census({}): {}", clip.name, clip.census);
        agb::println!("[spike] why:  {}", clip.why);

        let mut pass = true;

        // Seek-table structure (the GAFP stand-in): frame 0 at region start,
        // offsets strictly ascending, every blocksize within the manifest's
        // max, every offset inside the blob. A truncated or hand-edited blob
        // trips these before any decode.
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
            // 4 bytes is the smallest legal frame header (sync + fields + a
            // 1-octet coded number + CRC-8 minimum is 6, but this is a cheap
            // bounds sanity pin, not a parser — the parser is the real gate).
            // try_from, not from: core never implements From<u32> for usize.
            within_region &=
                usize::try_from(frame.offset).is_ok_and(|o| (o + 4) <= clip.region.len());
        }
        pass &= ascending & within_max & within_region;
        agb::println!(
            "[spike] {} seek table: ascending={} within_max={} within_region={}",
            clip.name,
            ascending,
            within_max,
            within_region
        );

        // The region pin: recompute over the embedded bytes and compare to the
        // generator's value. Cross-language hash agreement + image integrity
        // in one line.
        let actual = fnv1a64(clip.region);
        let ok = actual == clip.fnv_frames;
        pass &= ok;
        agb::println!(
            "[spike] {} region fnv: expected 0x{:016X} actual 0x{:016X} [{}]",
            clip.name,
            clip.fnv_frames,
            actual,
            if ok { "OK" } else { "MISMATCH" }
        );
        pass
    }

    /// PR 2 checkpoint: the on-target decode proof for one arm.
    ///
    /// Decodes every frame of the clip via `driver::decode_clip` (the same
    /// seek-table path the host witness walks), folds each decoded block into
    /// a running FNV-1a-64 through the shared `fold_i16le_stereo`, and
    /// returns the final hash plus decode stats — or `None` if the walk
    /// failed, with the failure mode named on serial before returning.
    ///
    /// Logs are the deliverable as much as the boolean: per-frame lines carry
    /// the running hash so a mismatch localizes to the first frame that
    /// diverged (compare against the host dump of the same fold sequence —
    /// `tests/spike_witness.rs::rom_fold_reproduces_pcm_pins_on_host` runs
    /// the identical fold on identical bytes), and every rejection path
    /// prints its variant, frame index, and evidence.
    fn decode_proof(clip: &SpikeClip) -> Option<(&'static str, u64, usize)> {
        let name = clip.name;
        let max = usize::from(clip.max_blocksize);
        // Decode scratch: one i32 buffer per channel at the track's max
        // blocksize (2048 × 4B = 8KB each; EWRAM has room), reused across
        // frames exactly as playback will reuse them. The tail beyond a
        // frame's own blocksize is load-bearing (the final frame is
        // legitimately short) — the flac-lite buffer contract keeps it
        // untouched and the fold never sees it (windows are sliced to
        // `written` by the driver callback).
        let mut left: alloc::vec::Vec<i32> = alloc::vec![0; max];
        let mut right: alloc::vec::Vec<i32> = alloc::vec![0; max];
        // EWRAM placement is printed as plain hex addresses: the allocator is
        // agb's EWRAM block heap, so these numbers also witness which region
        // the decode scratch actually lives in (PR 4's buffer rotation reads
        // the same addresses; a {:p} would be an opaque placeholder in core).
        agb::println!(
            "[spike] {} checkpoint: decode buffers ready ({} samples × {}B each; left=0x{:08X} right=0x{:08X})",
            name,
            max,
            core::mem::size_of::<i32>(),
            left.as_ptr() as usize,
            right.as_ptr() as usize
        );

        let mut state = [PredictorState::new(), PredictorState::new()];
        let mut hash = Fnv1a64::new();
        // The fold can fail on a desynced decode (a sample outside the
        // 16-bit profile). Record the FIRST offender without aborting the
        // walk: the per-frame log then shows exactly where the running hash
        // starts diverging, and the final-hash comparison below still gets
        // computed as an independent witness of the corruption.
        let mut fold_failure: Option<(usize, i32)> = None;

        agb::println!(
            "[spike] {} decode proof: walking {} frames via seek table",
            name,
            clip.frames.len()
        );
        let walk = driver::decode_clip(clip, &mut left, &mut right, &mut state, |frame, l, r| {
            match fold_i16le_stereo(&mut hash, l, r) {
                Ok(()) => agb::println!(
                    "[spike] {} frame {:>3}/{} off={} bs={} running=0x{:016X}",
                    name,
                    frame,
                    clip.frames.len(),
                    usize::try_from(clip.frames[frame].offset).unwrap_or(usize::MAX),
                    l.len(),
                    hash.finish()
                ),
                Err(sample) => {
                    if fold_failure.is_none() {
                        fold_failure = Some((frame, sample));
                    }
                    agb::println!(
                        "[spike] {} FAIL frame {:>3}: decoded sample {} outside 16-bit profile \
                         (decode desync — hash not folded, walk continues for localization)",
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
                // Every DriverError variant names its frame and evidence;
                // this is the PR 2 failure mode for "the decoder or the
                // assets regressed inside this image".
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
                        // flac_lite::Error is Debug; {:?} names the variant.
                        alloc::format!("flac-lite rejected the stream: {:?}", e)
                    }
                };
                agb::println!("[spike] {} FAIL decode walk aborted: {}", name, described);
                return None;
            }
        };

        agb::println!(
            "[spike] {} checkpoint: walk complete frames={}/{} samples={}",
            name,
            stats.frames,
            clip.frames.len(),
            stats.samples
        );

        let mut pass = true;

        // Cross-check the walk totals against the manifest: a frame silently
        // skipped would keep the loop "green" while hashing short PCM.
        let expected_samples = usize::try_from(clip.total_samples).unwrap_or(usize::MAX);
        let samples_ok = stats.samples == expected_samples && stats.frames == clip.frames.len();
        pass &= samples_ok;
        agb::println!(
            "[spike] {} totals: frames={} (want {}) samples={} (want {}) [{}]",
            name,
            stats.frames,
            clip.frames.len(),
            stats.samples,
            expected_samples,
            if samples_ok { "OK" } else { "MISMATCH" }
        );

        if let Some((frame, sample)) = fold_failure {
            agb::println!(
                "[spike] {} FAIL first out-of-range sample {} at frame {}",
                name,
                sample,
                frame
            );
            pass = false;
        }

        // The pin: on-target PCM hash vs the generator's host-computed
        // fnv_pcm (reference `flac -d` bytes). This line is PR 2's gate.
        let actual = hash.finish();
        let pin_ok = actual == clip.fnv_pcm;
        pass &= pin_ok;
        agb::println!(
            "[spike] {} pcm fnv: expected 0x{:016X} actual 0x{:016X} [{}]",
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

    /// The free-running cycle counter: timer2 /1 (one tick per CPU cycle at
    /// 16.78 MHz) overflowing into timer3 via the hardware cascade bit.
    ///
    /// Read order for the 32-bit composite is low, high, low: if the second
    /// low is not ≥ the first, the low half wrapped between the reads and the
    /// high half may be one stale, so the composite is re-read. The race
    /// window is ~2 memory reads wide against a 65,536-cycle tick period, so
    /// one retry resolves it; the fallback accept-after-retries is
    /// deterministic on mGBA and still bounded (off by at most one low-half
    /// period, which `cycles_between`/`worst_wrapped` would surface anyway).
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
            // Zero the pair at setup so the first window starts at a known
            // place (the reload registers stay 0, so overflows re-enter free
            // running from 0 — this is what makes it a free-running counter).
            counter.soft_reset();
            counter
        }

        /// Freeze, zero both halves, unfreeze — used between passes so a pass
        /// never depends on accumulated overflow from an earlier pass.
        fn soft_reset(&mut self) {
            self.low.set_enabled(false);
            self.high.set_enabled(false);
            self.high.set_overflow_amount(0);
            self.low.set_overflow_amount(0);
            self.low.set_enabled(true);
            self.high.set_enabled(true);
        }

        fn read(&self) -> u32 {
            let mut high; // last observed high half
            let mut low;
            for _ in 0..4 {
                let low1 = self.low.value();
                high = self.high.value();
                low = self.low.value();
                if low >= low1 {
                    return (u32::from(high) << 16) | u32::from(low);
                }
            }
            // Pathological: the wrap raced every attempt (cannot happen with
            // IRQs off at ~2 reads vs a 65,536-cycle period; if it somehow
            // did, the value is still a deterministic u32 and windows degrade
            // to saturating windows, loudly).
            high = self.high.value();
            low = self.low.value();
            (u32::from(high) << 16) | u32::from(low)
        }
    }

    /// Run `work` inside one measurement window: IME off (agb's critical
    /// section — its acquire/release is exactly an IME save/disable/restore
    /// around the closure), counter composite read before and after, window
    /// = saturating difference via the shared fold rule in
    /// `stats::cycles_between`. All logging happens OUTSIDE the window — a
    /// serial write inside would measure the logger, not the decode.
    fn timed<T>(counter: &mut Counter<'_>, work: impl FnOnce() -> T) -> (T, u32) {
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

    /// One arm's timing pass. Runs ONLY after that arm's PR 2 decode proof
    /// passed (correctness-before-speed). Returns `Some((fold, overhead,
    /// fits_budget))` whenever the harness completed — calibration stable,
    /// IME off in-window, every frame timed, no composite wrap. `None` is
    /// reserved for harness failure (numbers void), and every such failure
    /// names itself on serial. A budget miss is NOT a harness failure: the
    /// measurement is valid evidence either way, so it comes back in the
    /// bool — the verdict line then distinguishes "arms measured" from
    /// "arms inside budget" instead of collapsing both into a count.
    fn timing_pass(clip: &SpikeClip, counter: &mut Counter<'_>) -> Option<(FrameCost, u32, bool)> {
        let name = clip.name;
        let max = usize::from(clip.max_blocksize);
        let mut left: alloc::vec::Vec<i32> = alloc::vec![0; max];
        let mut right: alloc::vec::Vec<i32> = alloc::vec![0; max];
        let mut state = [PredictorState::new(), PredictorState::new()];

        // Calibration: 5 empty windows (the work inside is a single IME
        // register read — the smallest possible probe, which also witnesses
        // IME == 0 inside the window). Deterministic emulator + deterministic
        // ROM ⇒ all 5 raws must be identical; that stability is what licenses
        // subtracting the value as harness overhead.
        let mut ime_inside = 0xFFFFu16;
        let mut raws = [0u32; CALIBRATION_SAMPLES];
        for (i, slot) in raws.iter_mut().enumerate() {
            // Every calibration window runs byte-identical code inside: one
            // IME probe (which also witnesses IME == 0 in-window). First-run
            // bookkeeping stays OUTSIDE the window — the first draft recorded
            // the first probe inside window 1 only, and mGBA measured exactly
            // that: raw 24 on sample 1 vs 22 on samples 2-5. The harness was
            // timing a conditional store, not its own overhead.
            let (probe, raw) = timed(counter, || {
                // SAFETY: REG_IME is an even address in the I/O region.
                unsafe { io_read_u16(REG_IME) }
            });
            if i == 0 {
                ime_inside = probe;
            }
            *slot = raw;
            agb::println!(
                "*** {} calib sample {}/{} raw={} ime_inside={}",
                name,
                i + 1,
                CALIBRATION_SAMPLES,
                raw,
                probe
            );
        }
        let overhead = raws[0];
        let stable = raws.iter().all(|r| *r == overhead);
        agb::println!(
            "*** {} calibration: overhead_estimate={} stable={} nets=[{} {} {} {} {}] (≈0 witness: all raws == estimate)",
            name,
            overhead,
            stable,
            overhead.saturating_sub(overhead),
            raws[1].saturating_sub(overhead),
            raws[2].saturating_sub(overhead),
            raws[3].saturating_sub(overhead),
            raws[4].saturating_sub(overhead)
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

        // The pass itself: one timed decode_one per frame, in seek-table
        // order, through the same driver seam (decode logic untouched).
        agb::println!(
            "*** {} timing pass: walking {} frames via seek table (IRQ-off windows)",
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
                    agb::println!(
                        "*** {} frame {:>3}/{} bs={} raw={} net={}",
                        name,
                        frame_index,
                        clip.frames.len(),
                        written,
                        raw,
                        net
                    );
                }
                Err(err) => {
                    // PR 2 proved this arm decodes; a failure here is the
                    // harness regressing the image, not the stream's fault.
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
                "*** {} FAIL a window exceeded the 32-bit composite (counter wrapped mid-frame): frame {} reported at sentinel {}",
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
        let fits = cost.max <= BUDGET_CYCLES_PER_FRAME;
        agb::println!(
            "*** {} budget: worst_frame={} worst_net={} vs budget={} [{}]",
            name,
            cost.max_frame,
            cost.max,
            BUDGET_CYCLES_PER_FRAME,
            if fits { "FITS" } else { "OVER BUDGET" }
        );
        if pass {
            Some((cost, overhead, fits))
        } else {
            None
        }
    }

    // -----------------------------------------------------------------------
    // 6a-5 — the realisation reading. Arms: ctrl, the Rust mechanism (6a-3's
    // `TwinKernel`), and the ARM embodiment (6a-4's `ArmRiceKernel`), reached
    // through the *product* call path (`driver::decode_one_with` →
    // `frame::decode_frame_with`), never through a vendored decode loop. This
    // is the rung's whole point: E10/E9 priced vendored copies, and an arm that
    // only exists in a probe cannot answer whether the win reaches the product.
    //
    // Three features, off by default, so the shipped instrument image is
    // unchanged:
    //   item6a5-rust  the Rust arm only (+ the overlay's `rice-twin`)
    //   item6a5-asm   the asm arm only  (+ the overlay's `rice-arm`) — the
    //                 *product shape*: one extra kernel, no second mechanism
    //   item6a5-arms  both, for the three-way reading in one link
    // 6a-3 measured why the last of those cannot supply the control: a second
    // `RiceKernel` body in the link re-decides inlining and moves the default
    // path by ~168–199 K cycles/frame. So `ctrl` is read in *this* image
    // alongside the arms (paired, same-image), and the shipped-baseline
    // comparison comes from the default image's own control run.
    //
    // 6a-5b adds a fourth window, `vend`, under `item6a5b-vendored` (which
    // implies `item6a5-asm` — a leg with nothing to be differential against
    // measures nothing): `flac_spike::e10::decode_one_arm`, the E10 archive's vendored
    // decode path, verbatim but for the two ports its own archive README names
    // (item 4a's `header_bits`, and `arm_rice_partition` -> `e10_rice_partition`
    // so its kernel and the landed one can coexist in one link — `global_asm!`
    // symbols are not namespaced). It is not a `RiceKernel` arm and shares no
    // code with the landed path: its own reader, frame parser, partition loop,
    // ARM kernel and 544 B of pinned tables.
    //
    // Why it exists: 6a-5's stack projection put the landed leg +48.2 c/sample
    // over E10's vendored leg — 5.2 K frames of margin on the decision. That
    // number is the only comparison in these records taken across TWO images,
    // which method-and-gates.md forbids, and whose own next paragraph measures
    // the class of effect at −36,543/−202,529 c/frame on UNCHANGED control code.
    // Static facts bound the seam bridge at ~0.15 c/sample (both legs make the
    // same 2 calls/frame through one in-crate generic, both keep a u32 word
    // cursor, both pin their tables to zero-wait IWRAM), and the two kernels'
    // bodies differ by exactly 5 instructions per sample — so the overhang is
    // either priced in one link here or it stops being a fact at all. The
    // reading is `asm − vend`, per frame, same link; the pre-registered fork
    // (±5.75 c/sample about the ~9 c/sample the 5-instruction delta predicts) is
    // in plan-of-record 6a-5b, and neither branch unlands 6a-5.
    // -----------------------------------------------------------------------
    #[cfg(any(feature = "item6a5-rust", feature = "item6a5-asm"))]
    mod item6a5 {
        use super::*;
        use flac_lite::residual::TableCountKernel;
        #[cfg(feature = "item6a5-asm")]
        use flac_lite_gba::rice_arm::ArmRiceKernel;
        #[cfg(feature = "item6a5-rust")]
        use flac_lite_gba::rice_twin::TwinKernel;
        #[cfg(feature = "item6a7-wide")]
        use flac_spike::wide;
        // [6a-7b] the summary the screen prints. Only the 6a-7 image has a
        // screen, so the parameter that carries it exists only in that build —
        // see the note on `pass`'s signature.
        #[cfg(feature = "item6a7-wide")]
        use flac_spike::summary::{ClipSummary, Leg, Summaries};

        /// E9's window shape: zero the counter pair before every window so the
        /// wrap sentinel means "this window exceeded 2^32" rather than "the pass
        /// did", and so every window enters with byte-identical preceding code
        /// (the reset rides outside the start→end span). A three-arm pass over
        /// both clips spans billions of cycles.
        fn timed_reset<T>(counter: &mut Counter<'_>, work: impl FnOnce() -> T) -> (T, u32) {
            let mut raw = 0u32;
            let out = agb::external::critical_section::with(|_cs| {
                counter.soft_reset();
                let start = counter.read();
                let out = work();
                let end = counter.read();
                raw = cycles_between(start, end);
                out
            });
            (out, raw)
        }

        /// Net without destroying the sentinel (Opus rule, convicted in E9):
        /// `WORST_WRAPPED - overhead` can never equal the sentinel, so a genuine
        /// wrap would print `wrapped=false` one subtraction below it.
        fn net_sentinel(raw: u32, overhead: u32) -> u32 {
            if raw == WORST_WRAPPED {
                raw
            } else {
                raw.saturating_sub(overhead)
            }
        }

        /// Paired per-frame deltas, which are the number of record (percentages
        /// are denominator-dependent; absolute cycles survive a changed one).
        #[derive(Default)]
        struct Paired {
            sum: i64,
            max: i32,
            max_frame: usize,
            faster: usize,
            slower: usize,
            equal: usize,
        }

        impl Paired {
            fn observe(&mut self, frame: usize, ctrl: u32, arm: u32) {
                let delta = ctrl as i64 - arm as i64; // positive = arm is faster
                self.sum += delta;
                let d = i32::try_from(delta).unwrap_or(if delta > 0 { i32::MAX } else { i32::MIN });
                self.max = self.max.max(d);
                if d == self.max {
                    self.max_frame = frame;
                }
                if delta > 0 {
                    self.faster += 1;
                } else if delta < 0 {
                    self.slower += 1;
                } else {
                    self.equal += 1;
                }
            }
        }

        /// Per-arm decode proof on the arm's OWN walk, against the same embedded
        /// PCM pin the shipped proof uses. Correctness-before-speed, per arm: an
        /// arm is timed only if it decodes the committed bytes to the pinned PCM.
        fn proof<K: flac_lite::residual::RiceKernel>(clip: &SpikeClip, arm: &'static str) -> bool {
            let max = usize::from(clip.max_blocksize);
            let mut left: alloc::vec::Vec<i32> = alloc::vec![0; max];
            let mut right: alloc::vec::Vec<i32> = alloc::vec![0; max];
            let mut state = [PredictorState::new(), PredictorState::new()];
            let mut hash = Fnv1a64::new();
            let mut fold_ok = true;
            let walk = driver::decode_clip_with::<K, _>(
                clip,
                &mut left,
                &mut right,
                &mut state,
                |_frame, l, r| {
                    if fold_i16le_stereo(&mut hash, l, r).is_err() {
                        fold_ok = false;
                    }
                },
            );
            let stats_ok = match &walk {
                Ok(stats) => {
                    stats.frames == clip.frames.len()
                        && stats.samples
                            == usize::try_from(clip.total_samples).unwrap_or(usize::MAX)
                }
                Err(err) => {
                    agb::println!(
                        "[spike] {} 6a5 arm {} FAIL: walk rejected: {:?}",
                        clip.name,
                        arm,
                        err
                    );
                    false
                }
            };
            let actual = hash.finish();
            let pin_ok = fold_ok && actual == clip.fnv_pcm;
            agb::println!(
                "[spike] {} 6a5 arm {} pcm fnv: expected 0x{:016X} actual 0x{:016X} walk={} [{}]",
                clip.name,
                arm,
                clip.fnv_pcm,
                actual,
                stats_ok,
                if pin_ok && stats_ok {
                    "MATCH"
                } else {
                    "MISMATCH"
                }
            );
            pin_ok && stats_ok
        }

        /// The per-arm timing pass. One timed window per enabled arm per frame,
        /// all in the same link; equality of the written samples against `ctrl`
        /// is checked OUTSIDE the windows, per frame, and a divergence voids the
        /// arm's numbers (E1/E2a/E4/E9 pattern). With `item6a5b-vendored` there
        /// is a fourth window — the E10 archive's vendored path — paired against
        /// the landed ARM arm rather than only against `ctrl`; that pair is what
        /// 6a-5b's record is.
        fn pass(
            clip: &SpikeClip,
            counter: &mut Counter<'_>,
            // [6a-7b] `cfg` on a function PARAMETER is what keeps every other
            // image in this crate byte-identical: with `item6a7-wide` off the
            // signature is the pre-6a-7b one, so no call, no register and no
            // word of `pass` changes for the default, 6a-5 and 6a-5b builds.
            // The summary is recorded once, here, after the last window.
            #[cfg(feature = "item6a7-wide")] slot: usize,
            #[cfg(feature = "item6a7-wide")] summaries: &mut Summaries,
        ) -> bool {
            let name = clip.name;
            let max = usize::from(clip.max_blocksize);
            let mut ctrl_l: alloc::vec::Vec<i32> = alloc::vec![0; max];
            let mut ctrl_r: alloc::vec::Vec<i32> = alloc::vec![0; max];
            let mut state_c = [PredictorState::new(), PredictorState::new()];
            // [6a-7] the i64 width leg: a separate decoder, so its buffers and
            // predictor state are its own (6a-5b's separation rule).
            #[cfg(feature = "item6a7-wide")]
            let mut wide_l: alloc::vec::Vec<i32> = alloc::vec![0; max];
            #[cfg(feature = "item6a7-wide")]
            let mut wide_r: alloc::vec::Vec<i32> = alloc::vec![0; max];
            #[cfg(feature = "item6a7-wide")]
            let mut wide_state = [PredictorState::new(), PredictorState::new()];

            #[cfg(feature = "item6a5-rust")]
            let mut rust_l: alloc::vec::Vec<i32> = alloc::vec![0; max];
            #[cfg(feature = "item6a5-rust")]
            let mut rust_r: alloc::vec::Vec<i32> = alloc::vec![0; max];
            #[cfg(feature = "item6a5-rust")]
            let mut state_r = [PredictorState::new(), PredictorState::new()];
            #[cfg(feature = "item6a5-asm")]
            let mut asm_l: alloc::vec::Vec<i32> = alloc::vec![0; max];
            #[cfg(feature = "item6a5-asm")]
            let mut asm_r: alloc::vec::Vec<i32> = alloc::vec![0; max];
            #[cfg(feature = "item6a5-asm")]
            let mut state_a = [PredictorState::new(), PredictorState::new()];
            // [6a-5b] the vendored leg's own buffers and its own `PredictorState`
            // pair: `e10::decode_one_arm` carries its own history, and neither it
            // nor the landed legs may touch the other's state. That separation is
            // also what makes the pair a measurement rather than a restatement.
            #[cfg(feature = "item6a5b-vendored")]
            let mut vend_l: alloc::vec::Vec<i32> = alloc::vec![0; max];
            #[cfg(feature = "item6a5b-vendored")]
            let mut vend_r: alloc::vec::Vec<i32> = alloc::vec![0; max];
            #[cfg(feature = "item6a5b-vendored")]
            let mut state_v = [PredictorState::new(), PredictorState::new()];

            // Calibration: one byte-identical loop, 3 discarded + 5 reported,
            // unconditional stores, nothing logged between windows (a
            // micro-window is code-layout sensitive; E4's verified shape).
            const DISCARDED: usize = 3;
            const TOTAL: usize = DISCARDED + CALIBRATION_SAMPLES;
            let mut calib = [0u32; TOTAL];
            let mut probes = [0xFFFFu16; TOTAL];
            for i in 0..TOTAL {
                let (probe, raw) = timed_reset(counter, || {
                    // SAFETY: REG_IME is an even address in the I/O region.
                    unsafe { io_read_u16(REG_IME) }
                });
                probes[i] = probe;
                calib[i] = raw;
            }
            let overhead = calib[DISCARDED];
            let stable = (DISCARDED..TOTAL).all(|i| calib[i] == overhead);
            let ime_inside = probes[DISCARDED];
            agb::println!(
                "*** {} 6a5 calibration: overhead={} stable={} ime_inside={} raws=[{} {} {} {} {}] arms={}",
                name,
                overhead,
                stable,
                ime_inside,
                calib[DISCARDED],
                calib[DISCARDED + 1],
                calib[DISCARDED + 2],
                calib[DISCARDED + 3],
                calib[DISCARDED + 4],
                cfg!(feature = "item6a5-rust") as u8
                    + cfg!(feature = "item6a5-asm") as u8
                    + cfg!(feature = "item6a5b-vendored") as u8
                    + cfg!(feature = "item6a7-wide") as u8
                    + 1
            );
            if !stable {
                agb::println!("*** {} 6a5 FAIL calibration unstable — numbers void", name);
                return false;
            }
            if ime_inside != 0 {
                agb::println!(
                    "*** {} 6a5 FAIL IME={} inside window — windows were interruptable",
                    name,
                    ime_inside
                );
                return false;
            }

            let mut ctrl = FrameCost::new();
            #[cfg(feature = "item6a7-wide")]
            let mut wide = FrameCost::new();
            #[cfg(feature = "item6a7-wide")]
            let mut wide_paired = Paired::default();
            #[cfg(feature = "item6a5-rust")]
            let mut rust = FrameCost::new();
            #[cfg(feature = "item6a5-rust")]
            let mut rust_paired = Paired::default();
            #[cfg(feature = "item6a5-asm")]
            let mut asm = FrameCost::new();
            #[cfg(feature = "item6a5-asm")]
            let mut asm_paired = Paired::default();
            #[cfg(feature = "item6a5b-vendored")]
            let mut vend = FrameCost::new();
            // 6a-5b's reading. `observe(frame, a, b)` computes `a − b`, so this
            // is `asm − vend`: a POSITIVE sum means the vendored leg is the
            // faster one — the direction the +48.2 c/sample projection claimed.
            // `faster`/`slower` are counted in that direction and printed under
            // names that say which leg won.
            #[cfg(feature = "item6a5b-vendored")]
            let mut vend_paired = Paired::default();
            // The same leg against this image's control, which is the shape
            // E10's own −349 c/sample reading had. It is not the rung's number
            // (that is the pair above); it is what makes the two records
            // comparable in kind rather than in percentage.
            #[cfg(feature = "item6a5b-vendored")]
            let mut vend_ctrl_paired = Paired::default();
            let mut ok = true;

            agb::println!(
                "*** {} 6a5 pass: {} frames, one IRQ-off window per arm per frame (paired)",
                name,
                clip.frames.len()
            );
            for frame_index in 0..clip.frames.len() {
                let (ctrl_res, ctrl_raw) = timed_reset(counter, || {
                    driver::decode_one_with::<TableCountKernel>(
                        clip,
                        frame_index,
                        &mut ctrl_l,
                        &mut ctrl_r,
                        &mut state_c,
                    )
                });
                let written = match ctrl_res {
                    Ok(w) => w,
                    Err(err) => {
                        agb::println!(
                            "*** {} 6a5 FAIL frame {}: ctrl rejected after its proof passed: {:?}",
                            name,
                            frame_index,
                            err
                        );
                        return false;
                    }
                };
                // [6a-7] the width leg: the same timed shape as every other
                // arm, so `ctrl − wide` prices arithmetic width and nothing else.
                #[cfg(feature = "item6a7-wide")]
                let wide_raw = {
                    let (res, raw) = timed_reset(counter, || {
                        wide::decode_one_wide(
                            clip,
                            frame_index,
                            &mut wide_l,
                            &mut wide_r,
                            &mut wide_state,
                        )
                    });
                    match res {
                        Ok(w) if w == written => raw,
                        other => {
                            agb::println!(
                                "*** {} 6a7 FAIL frame {}: wide returned {:?} (ctrl wrote {})",
                                name,
                                frame_index,
                                other,
                                written
                            );
                            return false;
                        }
                    }
                };
                #[cfg(feature = "item6a5-rust")]
                let rust_raw = {
                    let (res, raw) = timed_reset(counter, || {
                        driver::decode_one_with::<TwinKernel>(
                            clip,
                            frame_index,
                            &mut rust_l,
                            &mut rust_r,
                            &mut state_r,
                        )
                    });
                    match res {
                        Ok(w) if w == written => raw,
                        other => {
                            agb::println!(
                                "*** {} 6a5 FAIL frame {}: rust returned {:?} (ctrl wrote {})",
                                name,
                                frame_index,
                                other,
                                written
                            );
                            return false;
                        }
                    }
                };
                #[cfg(feature = "item6a5-asm")]
                let asm_raw = {
                    let (res, raw) = timed_reset(counter, || {
                        driver::decode_one_with::<ArmRiceKernel>(
                            clip,
                            frame_index,
                            &mut asm_l,
                            &mut asm_r,
                            &mut state_a,
                        )
                    });
                    match res {
                        Ok(w) if w == written => raw,
                        other => {
                            agb::println!(
                                "*** {} 6a5 FAIL frame {}: asm returned {:?} (ctrl wrote {})",
                                name,
                                frame_index,
                                other,
                                written
                            );
                            return false;
                        }
                    }
                };
                // [6a-5b] the fourth window, laid out exactly like the others —
                // same `timed_reset` shape, same sentinel rule — so the two legs
                // differ in what they decode, not in how they are timed.
                #[cfg(feature = "item6a5b-vendored")]
                let vend_raw = {
                    let (res, raw) = timed_reset(counter, || {
                        flac_spike::e10::decode_one_arm(
                            clip,
                            frame_index,
                            &mut vend_l,
                            &mut vend_r,
                            &mut state_v,
                        )
                    });
                    match res {
                        Ok(w) if w == written => raw,
                        other => {
                            agb::println!(
                                "*** {} 6a5 FAIL frame {}: vend returned {:?} (ctrl wrote {})",
                                name,
                                frame_index,
                                other,
                                written
                            );
                            return false;
                        }
                    }
                };

                // Equality witness, outside the windows: a variant's numbers only
                // count on a frame whose samples are bit-identical to the
                // proven-correct control's.
                #[cfg(feature = "item6a7-wide")]
                if wide_l[..written] != ctrl_l[..written] || wide_r[..written] != ctrl_r[..written]
                {
                    agb::println!(
                        "*** {} 6a7 FAIL frame {}: wide samples differ from ctrl — numbers void",
                        name,
                        frame_index
                    );
                    ok = false;
                }
                #[cfg(feature = "item6a5-rust")]
                if rust_l[..written] != ctrl_l[..written] || rust_r[..written] != ctrl_r[..written]
                {
                    agb::println!(
                        "*** {} 6a5 FAIL frame {}: rust samples differ from ctrl — numbers void",
                        name,
                        frame_index
                    );
                    ok = false;
                }
                #[cfg(feature = "item6a5-asm")]
                if asm_l[..written] != ctrl_l[..written] || asm_r[..written] != ctrl_r[..written] {
                    agb::println!(
                        "*** {} 6a5 FAIL frame {}: asm samples differ from ctrl — numbers void",
                        name,
                        frame_index
                    );
                    ok = false;
                }
                // [6a-5b] correctness before speed, same strength as the arms':
                // the vendored leg has to produce the SAME samples through its own
                // reader and its own kernel, or its cycles measure nothing.
                #[cfg(feature = "item6a5b-vendored")]
                if vend_l[..written] != ctrl_l[..written] || vend_r[..written] != ctrl_r[..written]
                {
                    agb::println!(
                        "*** {} 6a5 FAIL frame {}: vend samples differ from ctrl — numbers void",
                        name,
                        frame_index
                    );
                    ok = false;
                }

                let ctrl_net = net_sentinel(ctrl_raw, overhead);
                ctrl.observe(frame_index, ctrl_net);
                #[cfg(feature = "item6a7-wide")]
                let wide_net = {
                    let n = net_sentinel(wide_raw, overhead);
                    wide.observe(frame_index, n);
                    wide_paired.observe(frame_index, ctrl_net, n);
                    n
                };
                #[cfg(feature = "item6a5-rust")]
                let rust_net = {
                    let n = net_sentinel(rust_raw, overhead);
                    rust.observe(frame_index, n);
                    rust_paired.observe(frame_index, ctrl_net, n);
                    n
                };
                #[cfg(feature = "item6a5-asm")]
                let asm_net = {
                    let n = net_sentinel(asm_raw, overhead);
                    asm.observe(frame_index, n);
                    asm_paired.observe(frame_index, ctrl_net, n);
                    n
                };
                #[cfg(feature = "item6a5b-vendored")]
                let vend_net = {
                    let n = net_sentinel(vend_raw, overhead);
                    vend.observe(frame_index, n);
                    vend_paired.observe(frame_index, asm_net, n);
                    vend_ctrl_paired.observe(frame_index, ctrl_net, n);
                    n
                };
                // One print per arm set: `#[cfg]` on a macro argument is not
                // portable expression-position Rust, so the three shapes are
                // written out. The build picks one.
                #[cfg(all(feature = "item6a5-rust", feature = "item6a5-asm"))]
                agb::println!(
                    "*** {} 6a5 frame {:>3}/{} bs={} ctrl={} rust={} asm={}",
                    name,
                    frame_index,
                    clip.frames.len(),
                    written,
                    ctrl_net,
                    rust_net,
                    asm_net
                );
                #[cfg(all(feature = "item6a5-rust", not(feature = "item6a5-asm")))]
                agb::println!(
                    "*** {} 6a5 frame {:>3}/{} bs={} ctrl={} rust={}",
                    name,
                    frame_index,
                    clip.frames.len(),
                    written,
                    ctrl_net,
                    rust_net
                );
                #[cfg(all(feature = "item6a5-asm", not(feature = "item6a5-rust")))]
                agb::println!(
                    "*** {} 6a5 frame {:>3}/{} bs={} ctrl={} asm={}",
                    name,
                    frame_index,
                    clip.frames.len(),
                    written,
                    ctrl_net,
                    asm_net
                );
                // [6a-7] its own line, so every earlier frame print quoted in
                // these records stays byte-for-byte as it is.
                #[cfg(feature = "item6a7-wide")]
                agb::println!(
                    "*** {} 6a7 frame {:>3}/{} wide={}",
                    name,
                    frame_index,
                    clip.frames.len(),
                    wide_net
                );
                // [6a-5b] its own line, so the frame prints every earlier 6a-5
                // record quotes stay byte-for-byte as they are. Printing rides
                // outside the timed windows either way.
                #[cfg(feature = "item6a5b-vendored")]
                agb::println!(
                    "*** {} 6a5 frame {:>3}/{} vend={}",
                    name,
                    frame_index,
                    clip.frames.len(),
                    vend_net
                );
            }

            // `#[cfg]` on an operand of `||` is not expression-position Rust
            // either; spell the three shapes.
            #[cfg(all(feature = "item6a5-rust", feature = "item6a5-asm"))]
            let wrapped = ctrl.wrapped || rust.wrapped || asm.wrapped;
            #[cfg(all(feature = "item6a5-rust", not(feature = "item6a5-asm")))]
            let wrapped = ctrl.wrapped || rust.wrapped;
            #[cfg(all(feature = "item6a5-asm", not(feature = "item6a5-rust")))]
            let wrapped = ctrl.wrapped || asm.wrapped;
            // [6a-5b] `item6a5b-vendored` implies `item6a5-asm` in the manifest,
            // so one of the spellings above defines `wrapped` whenever this
            // shadowing line compiles.
            #[cfg(feature = "item6a5b-vendored")]
            let wrapped = wrapped || vend.wrapped;
            #[cfg(feature = "item6a7-wide")]
            let wrapped = wrapped || wide.wrapped;
            if wrapped {
                agb::println!(
                    "*** {} 6a5 FAIL a window exceeded the 32-bit composite",
                    name
                );
                return false;
            }

            agb::println!(
                "*** {} 6a5 ctrl stats: count={} min={}@{} max={}@{} sum={}",
                name,
                ctrl.count,
                ctrl.min,
                ctrl.min_frame,
                ctrl.max,
                ctrl.max_frame,
                ctrl.sum
            );
            #[cfg(feature = "item6a5-rust")]
            {
                agb::println!(
                    "*** {} 6a5 rust stats: count={} min={}@{} max={}@{} sum={}",
                    name,
                    rust.count,
                    rust.min,
                    rust.min_frame,
                    rust.max,
                    rust.max_frame,
                    rust.sum
                );
                agb::println!(
                    "*** {} 6a5 rust paired: sum_delta={} (ctrl-minus-arm, positive = faster) mean_x1000={} max_delta={}@{} faster={} slower={} equal={}",
                    name,
                    rust_paired.sum,
                    if rust.count > 0 {
                        rust_paired.sum * 1000 / rust.count as i64
                    } else {
                        0
                    },
                    rust_paired.max,
                    rust_paired.max_frame,
                    rust_paired.faster,
                    rust_paired.slower,
                    rust_paired.equal
                );
            }
            #[cfg(feature = "item6a5-asm")]
            {
                agb::println!(
                    "*** {} 6a5 asm stats: count={} min={}@{} max={}@{} sum={}",
                    name,
                    asm.count,
                    asm.min,
                    asm.min_frame,
                    asm.max,
                    asm.max_frame,
                    asm.sum
                );
                agb::println!(
                    "*** {} 6a5 asm paired: sum_delta={} mean_x1000={} max_delta={}@{} faster={} slower={} equal={}",
                    name,
                    asm_paired.sum,
                    if asm.count > 0 {
                        asm_paired.sum * 1000 / asm.count as i64
                    } else {
                        0
                    },
                    asm_paired.max,
                    asm_paired.max_frame,
                    asm_paired.faster,
                    asm_paired.slower,
                    asm_paired.equal
                );
            }
            // [6a-5b] the rung's own numbers, printed last so the two pairs read
            // in the order the plan pre-declared: leg against leg, then the leg
            // against this image's control.
            #[cfg(feature = "item6a5b-vendored")]
            {
                agb::println!(
                    "*** {} 6a5 vend stats: count={} min={}@{} max={}@{} sum={}",
                    name,
                    vend.count,
                    vend.min,
                    vend.min_frame,
                    vend.max,
                    vend.max_frame,
                    vend.sum
                );
                agb::println!(
                    "*** {} 6a5b asm-vs-vend paired: sum_delta={} (asm-minus-vend; POSITIVE = the vendored leg is faster) mean_x1000={} max_delta={}@{} vend_faster={} asm_faster={} equal={}",
                    name,
                    vend_paired.sum,
                    if vend.count > 0 {
                        vend_paired.sum * 1000 / vend.count as i64
                    } else {
                        0
                    },
                    vend_paired.max,
                    vend_paired.max_frame,
                    vend_paired.faster,
                    vend_paired.slower,
                    vend_paired.equal
                );
                agb::println!(
                    "*** {} 6a5 vend-vs-ctrl paired: sum_delta={} (ctrl-minus-vend, positive = faster) mean_x1000={} max_delta={}@{} faster={} slower={} equal={}",
                    name,
                    vend_ctrl_paired.sum,
                    if vend.count > 0 {
                        vend_ctrl_paired.sum * 1000 / vend.count as i64
                    } else {
                        0
                    },
                    vend_ctrl_paired.max,
                    vend_ctrl_paired.max_frame,
                    vend_ctrl_paired.faster,
                    vend_ctrl_paired.slower,
                    vend_ctrl_paired.equal
                );
            }
            // [6a-7] the width marginal: `ctrl − wide`, positive = the i64 leg
            // completed fewer cycles (it should be negative — wide is the
            // reference the i32 default replaced).
            #[cfg(feature = "item6a7-wide")]
            {
                agb::println!(
                    "*** {} 6a7 wide stats: count={} min={}@{} max={}@{} sum={}",
                    name,
                    wide.count,
                    wide.min,
                    wide.min_frame,
                    wide.max,
                    wide.max_frame,
                    wide.sum
                );
                agb::println!(
                    "*** {} 6a7 width paired: sum_delta={} (ctrl-minus-wide; NEGATIVE = the i64 leg is slower) mean_x1000={} max_delta={}@{} wide_faster={} i32_faster={} equal={}",
                    name,
                    wide_paired.sum,
                    if wide.count > 0 {
                        wide_paired.sum * 1000 / wide.count as i64
                    } else {
                        0
                    },
                    wide_paired.max,
                    wide_paired.max_frame,
                    wide_paired.faster,
                    wide_paired.slower,
                    wide_paired.equal
                );
            }
            #[cfg(all(feature = "item6a5-rust", feature = "item6a5-asm"))]
            agb::println!(
                "*** {} 6a5 budget: ctrl_worst={} rust_worst={} asm_worst={} vs {}",
                name,
                ctrl.max,
                rust.max,
                asm.max,
                BUDGET_CYCLES_PER_FRAME
            );
            #[cfg(all(feature = "item6a5-rust", not(feature = "item6a5-asm")))]
            agb::println!(
                "*** {} 6a5 budget: ctrl_worst={} rust_worst={} vs {}",
                name,
                ctrl.max,
                rust.max,
                BUDGET_CYCLES_PER_FRAME
            );
            #[cfg(all(feature = "item6a5-asm", not(feature = "item6a5-rust")))]
            agb::println!(
                "*** {} 6a5 budget: ctrl_worst={} asm_worst={} vs {}",
                name,
                ctrl.max,
                asm.max,
                BUDGET_CYCLES_PER_FRAME
            );
            #[cfg(feature = "item6a5b-vendored")]
            agb::println!(
                "*** {} 6a5 budget: vend_worst={} vs {}",
                name,
                vend.max,
                BUDGET_CYCLES_PER_FRAME
            );
            #[cfg(feature = "item6a7-wide")]
            agb::println!(
                "*** {} 6a7 budget: wide_worst={} vs {}",
                name,
                wide.max,
                BUDGET_CYCLES_PER_FRAME
            );
            // [6a-7b] the screen's copy of this clip's three legs. Taken here,
            // after every window has closed and every per-frame line has been
            // printed, so nothing on the screen can be written to by a later
            // window and the serial record above stays the source of the
            // undivided numbers. `Leg` copies sum/max/count only — no `mean`
            // runs in this function.
            #[cfg(feature = "item6a7-wide")]
            summaries.record(
                slot,
                ClipSummary {
                    count: ctrl.count,
                    budget: BUDGET_CYCLES_PER_FRAME,
                    ctrl: Leg::of(&ctrl),
                    wide: Leg::of(&wide),
                    asm: Leg::of(&asm),
                },
            );
            ok
        }

        /// [6a-5b] The vendored leg's decode proof: the same embedded-PCM pin and
        /// the same fold, walked through the leg's own per-frame path (`e10` is a
        /// probe: it has no `decode_clip_with`). The mono contract is the driver's
        /// contract — an empty right window — so the bytes folded are identical
        /// across legs and a MISMATCH here can only mean the leg drifted.
        #[cfg(feature = "item6a5b-vendored")]
        fn vendored_proof(clip: &SpikeClip, arm: &'static str) -> bool {
            let max = usize::from(clip.max_blocksize);
            let mut left: alloc::vec::Vec<i32> = alloc::vec![0; max];
            let mut right: alloc::vec::Vec<i32> = alloc::vec![0; max];
            let mut state = [PredictorState::new(), PredictorState::new()];
            let mut hash = Fnv1a64::new();
            let mut fold_ok = true;
            let mut walk_ok = true;
            let mut frames = 0usize;
            let mut samples = 0usize;
            for frame_index in 0..clip.frames.len() {
                match flac_spike::e10::decode_one_arm(
                    clip,
                    frame_index,
                    &mut left,
                    &mut right,
                    &mut state,
                ) {
                    Ok(written) => {
                        let right_window: &[i32] = if clip.channels == 2 {
                            &right[..written]
                        } else {
                            &[]
                        };
                        if fold_i16le_stereo(&mut hash, &left[..written], right_window).is_err() {
                            fold_ok = false;
                        }
                        frames += 1;
                        samples += written;
                    }
                    Err(err) => {
                        agb::println!(
                            "[spike] {} 6a5 arm {} FAIL: walk rejected at frame {}: {:?}",
                            clip.name,
                            arm,
                            frame_index,
                            err
                        );
                        walk_ok = false;
                        break;
                    }
                }
            }
            let actual = hash.finish();
            let stats_ok = walk_ok
                && frames == clip.frames.len()
                && samples == usize::try_from(clip.total_samples).unwrap_or(usize::MAX);
            let pin_ok = fold_ok && actual == clip.fnv_pcm;
            agb::println!(
                "[spike] {} 6a5 arm {} pcm fnv: expected 0x{:016X} actual 0x{:016X} walk={} [{}]",
                clip.name,
                arm,
                clip.fnv_pcm,
                actual,
                stats_ok,
                if pin_ok && stats_ok {
                    "MATCH"
                } else {
                    "MISMATCH"
                }
            );
            pin_ok && stats_ok
        }

        /// [6a-7] the width leg's decode proof: the same embedded-PCM pin and
        /// fold, walked through the vendored i64 path's own per-frame entry.
        /// `wide` is a probe decoder, so it has no `decode_clip_with`; the
        /// bytes folded are the driver's contract (empty right window on mono),
        /// so a MISMATCH can only mean the leg drifted.
        #[cfg(feature = "item6a7-wide")]
        fn wide_proof(clip: &SpikeClip, arm: &'static str) -> bool {
            let max = usize::from(clip.max_blocksize);
            let mut left: alloc::vec::Vec<i32> = alloc::vec![0; max];
            let mut right: alloc::vec::Vec<i32> = alloc::vec![0; max];
            let mut state = [PredictorState::new(), PredictorState::new()];
            let mut hash = Fnv1a64::new();
            let mut fold_ok = true;
            let mut walk_ok = true;
            let mut frames = 0usize;
            let mut samples = 0usize;
            for frame_index in 0..clip.frames.len() {
                match wide::decode_one_wide(clip, frame_index, &mut left, &mut right, &mut state) {
                    Ok(written) => {
                        let right_window: &[i32] = if clip.channels == 2 {
                            &right[..written]
                        } else {
                            &[]
                        };
                        if fold_i16le_stereo(&mut hash, &left[..written], right_window).is_err() {
                            fold_ok = false;
                        }
                        frames += 1;
                        samples += written;
                    }
                    Err(err) => {
                        agb::println!(
                            "[spike] {} 6a7 arm {} FAIL: walk rejected at frame {}: {:?}",
                            clip.name,
                            arm,
                            frame_index,
                            err
                        );
                        walk_ok = false;
                        break;
                    }
                }
            }
            let actual = hash.finish();
            let stats_ok = walk_ok
                && frames == clip.frames.len()
                && samples == usize::try_from(clip.total_samples).unwrap_or(usize::MAX);
            let pin_ok = fold_ok && actual == clip.fnv_pcm;
            agb::println!(
                "[spike] {} 6a7 arm {} pcm fnv: expected 0x{:016X} actual 0x{:016X} walk={} [{}]",
                clip.name,
                arm,
                clip.fnv_pcm,
                actual,
                stats_ok,
                if pin_ok && stats_ok {
                    "MATCH"
                } else {
                    "MISMATCH"
                }
            );
            pin_ok && stats_ok
        }

        /// Per-arm decode proofs, then the timing pass. Runs only when every arm
        /// decoded the committed bytes to the pinned PCM.
        fn proofs_then_pass<K: flac_lite::residual::RiceKernel>(
            clip: &SpikeClip,
            arm: &'static str,
            all_ok: &mut bool,
        ) {
            let ok = proof::<K>(clip, arm);
            *all_ok &= ok;
        }

        pub fn run(
            clip: &SpikeClip,
            counter: &mut Counter<'_>,
            // [6a-7b] see `pass`: both parameters exist only in the 6a-7 image.
            #[cfg(feature = "item6a7-wide")] slot: usize,
            #[cfg(feature = "item6a7-wide")] summaries: &mut Summaries,
        ) -> bool {
            let mut proofs_ok = true;
            proofs_then_pass::<TableCountKernel>(clip, "ctrl", &mut proofs_ok);
            #[cfg(feature = "item6a5-rust")]
            proofs_then_pass::<TwinKernel>(clip, "rust", &mut proofs_ok);
            #[cfg(feature = "item6a5-asm")]
            proofs_then_pass::<ArmRiceKernel>(clip, "asm", &mut proofs_ok);
            // [6a-5b] correctness before speed, before any timing: the vendored
            // leg is timed only if it decodes the committed bytes to the pin.
            #[cfg(feature = "item6a5b-vendored")]
            {
                let ok = vendored_proof(clip, "vend");
                proofs_ok &= ok;
            }
            // [6a-7] correctness before speed, before any timing: the i64
            // width leg is timed only if it decodes the committed bytes to the
            // pin AND agrees with the control frame by frame.
            #[cfg(feature = "item6a7-wide")]
            {
                let ok = wide_proof(clip, "wide");
                proofs_ok &= ok;
            }
            agb::println!(
                "*** {} 6a5: per-arm decode proof {} — timing pass {}",
                clip.name,
                if proofs_ok { "PASSED" } else { "FAILED" },
                if proofs_ok {
                    "engaged"
                } else {
                    "SKIPPED (numbers void)"
                }
            );
            if !proofs_ok {
                return false;
            }
            // [6a-7b] the extra arguments exist only in the 6a-7 image (`cfg`
            // on a *parameter* is stable; `cfg` on a call argument is not), so
            // the two shapes are spelled out. The build picks one.
            #[cfg(feature = "item6a7-wide")]
            {
                pass(clip, counter, slot, summaries)
            }
            #[cfg(not(feature = "item6a7-wide"))]
            {
                pass(clip, counter)
            }
        }
    }

    #[agb::entry]
    fn main(mut gba: agb::Gba) -> ! {
        agb::eprintln!(
            "[spike] entry started — perf-gate spike ROM (PR 3: cycle harness + per-frame cost)"
        );

        let mut gfx = gba.graphics.get();
        gfx.set_background_palette_colour(0, 0, COLOR_PENDING);

        // Checkpoint 1 — asset verification (PR 1's gate): the bytes about to
        // be decoded are the committed generator output.
        let mut pass = true;
        for clip in CLIPS {
            pass &= check_clip(clip);
        }
        agb::println!(
            "[spike] checkpoint: asset verification {} (region pins — decode below runs on these bytes)",
            if pass { "PASSED" } else { "FAILED" }
        );
        // A failed region pin means the image drifted from its pins; decoding
        // drifted bytes could still "succeed" against nothing, so the decode
        // proof only carries meaning when the assets passed. Run it anyway —
        // its logs are diagnostic — but the verdict is already red.
        let assets_pass = pass;

        // Checkpoint 2 — the on-target decode proof, per arm.
        let mut proofs: [Option<(&'static str, u64, usize)>; 2] = [None, None];
        for (slot, clip) in CLIPS.iter().enumerate() {
            proofs[slot] = decode_proof(clip);
            pass &= proofs[slot].is_some();
        }

        // Checkpoint 3 — cross-arm agreement: both arms encode the same
        // source, so their on-target PCM hashes must match each other. Two
        // pins matching independently already implies it; the explicit line
        // witnesses that neither arm decoded into the other's buffers.
        if let (Some((_, h0, _)), Some((_, h1, _))) = (proofs[0], proofs[1]) {
            let agree = h0 == h1;
            pass &= agree;
            agb::println!(
                "[spike] cross-arm pcm: l0=0x{:016X} l4=0x{:016X} agree={} [{}]",
                h0,
                h1,
                agree,
                if agree { "OK" } else { "MISMATCH" }
            );
        }

        // Checkpoint 4 — the PR 3 timing pass. Correctness-before-speed:
        // an arm is timed only if its decode proof passed on these bytes.
        // Any failed proof skips timing for every arm and keeps the verdict
        // red — the screen never shows a budget verdict computed from an
        // unproven image, and the log says exactly that.
        let proofs_pass = proofs.iter().all(Option::is_some);
        pass &= proofs_pass;
        agb::println!(
            "[spike] checkpoint: decode proof {} — timing pass {}",
            if proofs_pass { "PASSED" } else { "FAILED" },
            if proofs_pass {
                "engaged"
            } else {
                "SKIPPED (correctness-before-speed: no numbers from an unproven image)"
            }
        );
        // [6a-7b] the screen's own verdict is *validity*: the spike's `pass`
        // ANDs the budget check, and the LPC clip is 167% of the 32,768 Hz
        // frame by design, so a screen coloured from `pass` would be red on a
        // valid measurement. `harness_ok` therefore covers the assets, the
        // decode proofs and every leg pass — everything a reader needs in
        // order to trust the numbers — and the budget verdict stays on serial.
        #[cfg(feature = "item6a7-wide")]
        let mut summaries = flac_spike::summary::Summaries::new();
        #[cfg(feature = "item6a7-wide")]
        let mut harness_ok = assets_pass && proofs_pass;

        let mut timing: [Option<(FrameCost, u32, bool)>; 2] = [None, None];
        if proofs_pass {
            // Interrupt-register witnesses around the whole timing section.
            // The inside-window witness (IME == 0) is printed by each arm's
            // calibration probe; here: the state the harness inherits and
            // must restore (agb's entrypoint boots with IME = 1).
            // SAFETY: REG_IE / REG_IF / REG_IME are even I/O-region addresses.
            agb::println!(
                "[spike] irq state before timing: IE=0x{:04X} IF=0x{:04X} IME={}",
                unsafe { io_read_u16(REG_IE) },
                unsafe { io_read_u16(REG_IF) },
                unsafe { io_read_u16(REG_IME) }
            );
            let mut timers = gba.timers.timers();
            let mut counter = Counter::start(&mut timers);
            for (slot, clip) in CLIPS.iter().enumerate() {
                timing[slot] = timing_pass(clip, &mut counter);
                // 6a-5's arm pass, when the instrument is built with an arm
                // feature on. It is a separate reading from `timing_pass` (which
                // is the shipped-path control of record) and it never runs in a
                // default build.
                #[cfg(any(feature = "item6a5-rust", feature = "item6a5-asm"))]
                {
                    // [6a-7b] same two shapes as `run`'s own tail: the 6a-7
                    // image's pass also leaves a summary for the screen.
                    #[cfg(feature = "item6a7-wide")]
                    {
                        let ok = item6a5::run(clip, &mut counter, slot, &mut summaries);
                        pass &= ok;
                        harness_ok &= ok;
                    }
                    #[cfg(not(feature = "item6a7-wide"))]
                    {
                        pass &= item6a5::run(clip, &mut counter);
                    }
                }
                // Red on EITHER failure mode — harness failure (None) or a
                // completed measurement that misses budget — but the two
                // counts stay separate in the verdict line below: a budget
                // miss is a valid measurement, not a failed harness.
                pass &= timing[slot].as_ref().is_some_and(|t| t.2);
            }
            agb::println!(
                "[spike] irq state after timing: IE=0x{:04X} IF=0x{:04X} IME={} (IME=1 means the harness restored it)",
                unsafe { io_read_u16(REG_IE) },
                unsafe { io_read_u16(REG_IF) },
                unsafe { io_read_u16(REG_IME) }
            );
        }

        // `label` names the backdrop colour in the log; only the build whose
        // screen is the plain verdict uses it, so it is defined there (`color`
        // is the palette colour both builds set).
        #[cfg(not(feature = "item6a7-wide"))]
        let (color, label) = if pass {
            (COLOR_PASS, "BLUE")
        } else {
            (COLOR_FAIL, "RED")
        };
        #[cfg(feature = "item6a7-wide")]
        let color = if pass { COLOR_PASS } else { COLOR_FAIL };
        gfx.set_background_palette_colour(0, 0, color);
        // [6a-7b] In the 6a-7 image the screen is the summary, whose verdict is
        // validity; saying "screen RED" here would describe a screen that is
        // blue. Two shapes, one build each — the same rule the arm prints use.
        #[cfg(feature = "item6a7-wide")]
        agb::eprintln!(
            "[spike] PR 3 BUDGET verdict {} (assets={}, decode proof={}, measured {}/{}, within budget {}/{}) — the screen shows the 6a7b summary; its verdict is validity, not budget",
            if pass { "PASSED" } else { "FAILED" },
            assets_pass,
            proofs_pass,
            timing.iter().filter(|t| t.is_some()).count(),
            CLIPS.len(),
            timing.iter().flatten().filter(|t| t.2).count(),
            CLIPS.len()
        );
        #[cfg(not(feature = "item6a7-wide"))]
        agb::eprintln!(
            "[spike] PR 3 verdict {} — screen {} (assets={}, decode proof={}, measured {}/{}, within budget {}/{})",
            if pass { "PASSED" } else { "FAILED" },
            label,
            assets_pass,
            proofs_pass,
            timing.iter().filter(|t| t.is_some()).count(),
            CLIPS.len(),
            timing.iter().flatten().filter(|t| t.2).count(),
            CLIPS.len()
        );

        // [6a-7b] the hardware-readable summary. Only the 6a-7 image has one:
        // every other image in this crate keeps the bytes its record quotes,
        // which is why this whole block is feature-gated rather than
        // conditional at runtime.
        //
        // Two channels, one string: the screen a stock GBA shows and the serial
        // log an mGBA run prints are the SAME bytes (`flac_spike::summary::
        // render`), so the mGBA reference for a hardware reading is taken from
        // this image and cannot drift from what the unit displays.
        #[cfg(feature = "item6a7-wide")]
        {
            // The font's own metrics decide whether ten lines fit; a screen that
            // overflows silently drops the number the rung exists to read.
            agb::println!("*** 6a7b font line_height={}", item6a7b::line_height());
            for (clip, _) in summaries.iter() {
                agb::println!("*** 6a7b clip {} = {}", clip.name, clip.why);
            }
            let text = flac_spike::summary::render(&summaries, harness_ok);
            let fit = item6a7b::measure(&text);
            agb::println!(
                "*** 6a7b fit: lines={} right={} bottom={} fits={} (screen 240x160)",
                fit.lines,
                fit.right,
                fit.bottom,
                fit.fits()
            );
            for line in text.lines() {
                agb::println!("[spike] 6a7b screen| {}", line);
            }
            agb::eprintln!(
                "*** 6a7b verdict: {} (validity only — the budget verdict is the PR 3 line above)",
                if harness_ok {
                    "HARNESS OK"
                } else {
                    "HARNESS FAILED"
                }
            );
            item6a7b::show(
                &mut gfx,
                &text,
                if harness_ok { COLOR_PASS } else { COLOR_FAIL },
            );
        }

        // [6a-7b] The 6a-7 image leaves the summary on screen and stops
        // presenting. This is not cosmetic and it is not a style choice — it is
        // the bug the first hardware run found (2026-11-10, GBA 001, twice: a
        // blue screen with no text).
        //
        // `TiledGraphicsFrame::commit` computes DISPCNT's enabled-background
        // mask from the backgrounds *shown into that frame* (`BackgroundFrame::
        // num_regular`, which `Graphics::frame()` resets to 0 — agb 0.25,
        // `src/display/tiled.rs`). So `show()`'s own commit paints the ten
        // lines, and then the next iteration of the pre-existing commit loop
        // builds a fresh frame with no background, writes
        // `enabled_backgrounds = 0`, and leaves the backdrop alone: blue, empty.
        // The palette survives because it is a direct VRAM write — which is why
        // the failure looked like "the palette worked and the tiles did not".
        //
        // No mGBA run could see it: the harness is headless, so nothing was ever
        // displayed to be erased. E5's ROM ends in a bare `loop {}` for exactly
        // this reason and its table is proven on both units; this is that shape.
        #[cfg(feature = "item6a7-wide")]
        loop {}
        #[cfg(not(feature = "item6a7-wide"))]
        loop {
            let frame = gfx.frame();
            frame.commit();
        }
    }
}

/// Host-build stub: the ROM entry exists only for `thumbv4t-none-eabi` (see
/// the module docs). `cargo test` never runs this bin (`test = false`), and
/// nothing ships it — this stub exists so host tooling can parse the crate
/// without the (host-incompatible) `agb` dependency.
#[cfg(not(target_arch = "arm"))]
fn main() {}
