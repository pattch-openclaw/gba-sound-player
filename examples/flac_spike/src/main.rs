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

#[cfg(target_arch = "arm")]
mod rom {
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
    /// the FLAC.md plan's number (the plan keeps the 32,768-Hz budget even
    /// though the clips encode at 32,000 Hz: 1,048,750 vs the true window
    /// 1,073,920, i.e. 2.34375% stricter than real-time, deliberately).
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

        let (color, label) = if pass {
            (COLOR_PASS, "BLUE")
        } else {
            (COLOR_FAIL, "RED")
        };
        gfx.set_background_palette_colour(0, 0, color);
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
