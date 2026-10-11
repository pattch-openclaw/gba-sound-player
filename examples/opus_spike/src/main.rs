//! Opus spike ROM — PR 4: cycle harness + per-packet cost.
//!
//! PURPOSE (see ../../../OPUS.md → "Measurement plan — start here"): PR 3
//! landed the **on-target decode proof** — both arms green, walk-fold
//! hashes reproducing the generator's `fnv_walk_fold` goldens. PR 4 (this
//! image) adds the **cycle harness**, measurement-plan step 5: after the
//! decode-proof layers pass, every packet of both arms is decoded again
//! inside a free-running 16.78 MHz counter window, and min/max/sum/
//! worst-packet-index per arm go to mGBA serial. This is pure
//! instrumentation: the decode path, the seam, and the vendored decoder
//! are untouched — the timing walk re-hashes its own folded output against
//! the same `fnv_walk_fold` goldens, so the image that prints the numbers
//! also proves it decoded the same samples while timing itself.
//!
//! Counter mechanism (flac_spike PR 3's harness, reused wholesale): timer2
//! at /1 (one tick per CPU cycle at 16.78 MHz, free-running) cascading
//! into timer3, read as a 32-bit composite (low, high, low-stability
//! recheck). Windows larger than u32::MAX (the pair wrapped mid-decode)
//! saturate to `stats::WORST_WRAPPED` — reported as worst and failed
//! loudly, never silently truncated.
//!
//! Measurement discipline (OPUS.md plan item 5 + flac_spike's landed rules,
//! including the E4 layout-sensitivity findings):
//! - **Empty-window calibration**: 8 byte-identical empty windows per arm
//!   inside ONE loop (unconditional stores, nothing logged between them —
//!   logging between calibration windows re-poisons them); the first 3 are
//!   discarded as cold-bus samples, the remaining 5 must come back
//!   raw-identical, and that value is the harness overhead subtracted as
//!   net from every packet window. Raw is reported alongside net.
//! - **IRQs off inside every window**: the window runs inside a
//!   `critical_section` (agb's IME save/disable/restore), with the IME
//!   probe witnessing IME == 0 in-window and IE/IF/IME logged around the
//!   whole timing section.
//! - **Silent timing pass**: the pass is one byte-identical loop through
//!   the production seam; the fold/stats ride OUTSIDE the window
//!   (`walk_region_timed::on_packet`) and NOTHING logs between packets —
//!   a serial write between micro-windows shifts them deterministically.
//!   Summary stats print only after the loop completes.
//! - **What is timed**: exactly one `probe::decode_packet` invocation per
//!   packet — decode + the decoder-internal resampler together (the
//!   internal resampler is part of the product cost, plan item 5).
//! - **Division stays out of the ROM**: per-arm sum + count are logged;
//!   the mean is derived as arithmetic host-side.
//! - **Correctness-before-speed**: the timing pass only runs on an arm
//!   whose decode proof passed, and the timing pass's own walk-fold hash
//!   must reproduce the same golden — a fast wrong decode measures
//!   nothing.
//!
//! Per-packet running-hash logs were REMOVED from the decode-proof pass on
//! Sam's instruction (2026-10-10): they measure the logger, not the walk,
//! and localization lives in the strict host walk dumps (`dump_walk`)
//! anyway. All correctness PINS (region FNV, walk-fold golden, totals,
//! EWRAM placement) stay on the proof pass.
//!
//! Screen verdict follows the spike convention (works on cart, no cable):
//! **blue = every check passed (including budget), red = any failed,
//! purple = never completed.** The verdict line separates "measured" from
//! "within budget" — a budget miss is a valid measurement, not a harness
//! failure.
//!
//! Host-build note: the entire ROM lives behind `cfg(target_arch = "arm")`.
//! `agb` is a target-gated dependency (Cargo.toml), so a host build of this
//! bin must not reference it — on host the bin is a no-op stub. Only the
//! thumbv4t ROM build (make native-opus-rom) compiles the real entry.

#![cfg_attr(target_arch = "arm", no_std)]
#![cfg_attr(target_arch = "arm", no_main)]

#[cfg(target_arch = "arm")]
extern crate alloc;

#[cfg(target_arch = "arm")]
mod rom {
    use agb::display::Rgb;
    use agb::timer::Divider;
    use opus_spike::assets::{CLIPS, OpusClip};
    use opus_spike::checksum::{Fnv1a64, fnv1a64, fold_f32_window};
    use opus_spike::probe::{self, WalkStats};
    use opus_spike::stats::{PacketCost, WORST_WRAPPED, cycles_between};

    const COLOR_PENDING: agb::display::Rgb15 = Rgb::new(128, 0, 255).to_rgb15();
    const COLOR_PASS: agb::display::Rgb15 = Rgb::new(0, 0, 255).to_rgb15();
    const COLOR_FAIL: agb::display::Rgb15 = Rgb::new(255, 0, 0).to_rgb15();

    /// Calibration windows per arm. Determinism is the premise of the
    /// whole harness, so the calibration doubles as a determinism probe.
    /// Shape follows flac_spike's E4 finding: micro-windows are code-layout
    /// sensitive (whatever immediately precedes a window shifts it), so all
    /// 8 run inside ONE byte-identical loop with unconditional stores and
    /// no logging between them; the first 3 are discarded as cold-bus
    /// samples (E1 measured the first window of a pass reading cold), and
    /// the stability requirement applies to the 5 reported windows.
    const CALIB_WINDOWS: usize = 8;
    const CALIB_DISCARDED: usize = 3;

    // GBA interrupt registers (IO region, always mapped, 16-bit): IE, IF,
    // IME. Read-only here — logged to witness the IRQ-off discipline.
    const REG_IE: usize = 0x0400_0200;
    const REG_IF: usize = 0x0400_0202;
    const REG_IME: usize = 0x0400_0208;

    /// One 16-bit read from the GBA I/O region.
    ///
    /// # Safety
    /// `addr` must be an even address in the memory-mapped I/O region
    /// (0x0400_0000..0x0400_0400). Reads there have no software
    /// invariants: the hardware always answers, volatile semantics are
    /// exactly what MMIO wants, and a u16-aligned read is the correct
    /// access width for these registers. Callers pass the REG_* constants.
    unsafe fn io_read_u16(addr: usize) -> u16 {
        unsafe { (addr as *const u16).read_volatile() }
    }

    /// Game Pak ROM mirror base — the mapping a linked-in code address
    /// must fall in for the link witness to mean anything.
    const ROM_MAPPING: core::ops::Range<usize> = 0x0800_0000..0x0D80_0000;

    /// EWRAM bounds for the state-size sanity check (bounds, not the pin
    /// itself — the exact value is pinned host-side where a regression is
    /// a named test failure, not a screen colour).
    const STATE_MIN_SANE: usize = 128 * 1024;
    const STATE_MAX_SANE: usize = 256 * 1024;

    /// EWRAM address range — the heap-allocated decoder state and decode
    /// window must land here (agb's global allocator is EWRAM's block
    /// heap).
    const EWRAM: core::ops::Range<usize> = 0x0200_0000..0x0204_0000;

    /// The free-running cycle counter: timer2 /1 (one tick per CPU cycle
    /// at 16.78 MHz) overflowing into timer3 via the hardware cascade bit.
    ///
    /// Read order for the 32-bit composite is low, high, low: if the
    /// second low is not >= the first, the low half wrapped between the
    /// reads and the high half may be one stale, so the composite is
    /// re-read. The race window is ~2 memory reads wide against a
    /// 65,536-cycle tick period, so one retry resolves it; the fallback
    /// accept-after-retries is deterministic on mGBA and still bounded.
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
            // place (the reload registers stay 0, so overflows re-enter
            // free running from 0 — this is what makes it free-running).
            counter.soft_reset();
            counter
        }

        /// Freeze, zero both halves, unfreeze — used between passes so a
        /// pass never depends on accumulated overflow from an earlier
        /// pass.
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
            // Pathological: the wrap raced every attempt (cannot happen
            // with IRQs off at ~2 reads vs a 65,536-cycle period; if it
            // somehow did, the value is still a deterministic u32 and
            // windows degrade to saturating windows, loudly).
            high = self.high.value();
            low = self.low.value();
            (u32::from(high) << 16) | u32::from(low)
        }
    }

    /// Run `work` inside one measurement window: IME off (agb's critical
    /// section — its acquire/release is exactly an IME save/disable/
    /// restore around the closure), counter composite read before and
    /// after, window = saturating difference via the shared fold rule in
    /// `stats::cycles_between`. All logging happens OUTSIDE the window —
    /// a serial write inside would measure the logger, not the decode.
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

    /// Manifest sanity from the embedded table + the region FNV pin: the
    /// bytes walked below are the generator's bytes. Cross-language hash
    /// agreement + image integrity in one line (flac_spike PR 2's shape).
    fn check_region(clip: &OpusClip) -> bool {
        agb::println!(
            "[opus-spike] arm={} packets={} region={}B walk_samples={} rate={} ch={} pre_skip={} align_shift={} fold_mismatch={}",
            clip.name,
            clip.packets,
            clip.region.len(),
            clip.walk_samples,
            clip.sample_rate_hz,
            clip.channels,
            clip.pre_skip,
            clip.align_shift,
            clip.fold_mismatch
        );

        let actual = fnv1a64(clip.region);
        let ok = actual == clip.fnv_packets;
        agb::println!(
            "[opus-spike] {} region fnv: expected 0x{:016X} actual 0x{:016X} [{}]",
            clip.name,
            clip.fnv_packets,
            actual,
            if ok { "MATCH" } else { "MISMATCH" }
        );
        ok
    }

    /// PR 3 checkpoint (kept): the on-target decode proof for one arm.
    ///
    /// Walks every packet via `probe::decode_clip` (the production seam),
    /// folds each decoded window into a running FNV-1a-64, and judges the
    /// final hash against the arm's `fnv_walk_fold` golden. The
    /// per-packet running-hash logs were removed (Sam 2026-10-10: they
    /// measure the logger; localization lives in the strict host walk
    /// dumps) — the pins stay: heap placement, walk totals, final hash.
    fn decode_proof(clip: &OpusClip) -> bool {
        let name = clip.name;
        agb::println!(
            "[opus-spike] {} decode proof: walking {} packets via the production seam",
            name,
            clip.packets
        );

        let mut hash = Fnv1a64::new();
        let walk = probe::decode_clip(clip, |window| {
            fold_f32_window(&mut hash, window);
        });

        let stats: WalkStats = match walk {
            Ok(stats) => stats,
            Err((msg, packet)) => {
                // Every tiling/decode guard names its failure at its
                // index; this is the failure mode for "the decoder or the
                // assets regressed inside this image".
                agb::println!(
                    "[opus-spike] {} FAIL decode walk aborted at packet {}: {}",
                    name,
                    packet,
                    msg
                );
                return false;
            }
        };

        let mut pass = true;

        // EWRAM placement witness: the decode state and window are heap
        // allocations; under agb the global allocator is EWRAM's block
        // heap, and these addresses witness that claim on this image.
        let ewram_ok = EWRAM.contains(&stats.decoder_addr) && EWRAM.contains(&stats.window_addr);
        pass &= ewram_ok;
        agb::println!(
            "[opus-spike] {} heap placement: decoder=0x{:08X} window=0x{:08X} within EWRAM [{}]",
            name,
            stats.decoder_addr,
            stats.window_addr,
            if ewram_ok { "OK" } else { "NOT IN EWRAM" }
        );

        // Cross-check the walk totals against the manifest: a packet
        // silently skipped would keep the loop "green" while hashing
        // short PCM.
        let totals_ok = stats.packets == clip.packets
            && usize::try_from(clip.walk_samples).is_ok_and(|want| want == stats.samples);
        pass &= totals_ok;
        agb::println!(
            "[opus-spike] {} totals: packets={} (want {}) samples={} (want {}) [{}]",
            name,
            stats.packets,
            clip.packets,
            stats.samples,
            clip.walk_samples,
            if totals_ok { "OK" } else { "MISMATCH" }
        );

        // The pin: on-target walk-fold hash vs the generator's golden
        // (measured over the dump_walk output of the SAME seam).
        let actual = hash.finish();
        let pin_ok = actual == clip.fnv_walk_fold;
        pass &= pin_ok;
        agb::println!(
            "[opus-spike] {} walk-fold fnv: expected 0x{:016X} actual 0x{:016X} [{}]",
            name,
            clip.fnv_walk_fold,
            actual,
            if pin_ok { "MATCH" } else { "MISMATCH" }
        );

        if !pass {
            agb::println!(
                "[opus-spike] {} FAIL arm verdict: decode proof failed",
                name
            );
        }
        pass
    }

    /// PR 4: one arm's timing pass. Runs ONLY after that arm's decode
    /// proof passed (correctness-before-speed). Returns `Some((fold,
    /// overhead, fits_budget, walk_hash_matches_golden))` whenever the
    /// harness completed — calibration stable, IME off in-window, every
    /// packet timed, no composite wrap, and the timed walk re-folded to
    /// the same golden. `None` is reserved for harness failure (numbers
    /// void); every such failure names itself on serial. A budget miss is
    /// NOT a harness failure — it rides in `fits_budget`, so the verdict
    /// line distinguishes "arms measured" from "arms inside budget".
    fn timing_pass(
        clip: &OpusClip,
        counter: &mut Counter<'_>,
    ) -> Option<(PacketCost, u32, bool, bool)> {
        let name = clip.name;

        // Calibration: CALIB_WINDOWS byte-identical empty windows in ONE
        // loop. The work inside is a single IME register read — the
        // smallest possible probe, which also witnesses IME == 0 inside
        // every window. The loop body is unconditional stores only, and
        // NOTHING is logged between windows (flac_spike E4: logging
        // between windows re-poisons them; E1: a pass's first window
        // measures cold).
        let mut raws = [0u32; CALIB_WINDOWS];
        let mut imes = [0xFFFFu16; CALIB_WINDOWS];
        for i in 0..CALIB_WINDOWS {
            let (probe, raw) = timed(counter, || {
                // SAFETY: REG_IME is an even address in the I/O region.
                unsafe { io_read_u16(REG_IME) }
            });
            raws[i] = raw;
            imes[i] = probe;
        }
        // Reporting happens only after the whole calibration loop closed.
        let overhead = raws[CALIB_DISCARDED];
        let reported = &raws[CALIB_DISCARDED..];
        let stable = reported.iter().all(|r| *r == overhead);
        let ime_clear = imes.iter().all(|v| *v == 0);
        agb::println!(
            "*** {} calibration: windows={} discarded={} raws=[{} {} {} | {} {} {} {} {}] overhead_estimate={} stable={} ime_inside_0={}",
            name,
            CALIB_WINDOWS,
            CALIB_DISCARDED,
            raws[0],
            raws[1],
            raws[2],
            raws[3],
            raws[4],
            raws[5],
            raws[6],
            raws[7],
            overhead,
            stable,
            ime_clear
        );
        if !stable {
            agb::println!(
                "*** {} FAIL calibration unstable — harness noise, numbers void",
                name
            );
            return None;
        }
        if !ime_clear {
            agb::println!(
                "*** {} FAIL a calibration window saw IME != 0 — windows were interruptable",
                name
            );
            return None;
        }

        // The timing pass itself: the production seam driven through the
        // measurement closure. Silent by construction — on_packet folds
        // and observes outside the window with no logging; a summary
        // prints only after the loop completes.
        agb::println!(
            "*** {} timing pass: walking {} packets via the production seam (IRQ-off windows, silent loop)",
            name,
            clip.packets
        );
        let mut cost = PacketCost::new();
        let mut hash = Fnv1a64::new();
        // PER-WINDOW soft_reset, measured 2026-10-10 first run: the
        // free-running 32-bit composite rolls over every 2^32 cycles
        // (~256 s emulated); an Opus pass spans ~3x that span, so any
        // window straddling the rollover read end < start and got the
        // sentinel (music "max = 4294967273@220" — a ~15.4 M cycles/packet
        // arm reporting one 4.3 G packet is the rollover artifact, not a
        // measurement). FLAC's pass never straddled (its biggest sum was
        // 1.08 G < 2^32), so the inherited harness had no defense. Zeroing
        // the pair before every window guarantees each window starts near
        // 0, restoring the sentinel's true meaning: end < start now can
        // only mean THIS packet's window exceeded 2^32 (stuck counter) —
        // a real failure, failed loudly. The reset runs outside the timed
        // span (start is read after it) and unconditionally, so every
        // window still has byte-identical preceding code (E4 rule).
        let mut measure = |decode: &mut dyn FnMut()| -> u32 {
            counter.soft_reset();
            let mut raw = 0u32;
            agb::external::critical_section::with(|_cs| {
                let start = counter.read();
                decode();
                let end = counter.read();
                raw = cycles_between(start, end);
            });
            raw
        };
        let walk = probe::walk_region_timed(
            clip.region,
            clip.index.iter().map(|e| (e.offset, e.len)),
            clip.sample_rate_hz,
            clip.channels,
            &mut measure,
            |packet, window, raw| {
                // Outside the measurement window by construction: fold +
                // stats ride here, so the window measures decode +
                // resampler and nothing else. SENTINEL ORDER MATTERS
                // (measured 2026-10-10 first run: netting first turned
                // WORST_WRAPPED 0xFFFFFFFF into 0xFFFFFFE9, so the fold's
                // `cycles == WORST_WRAPPED` check could never fire and a
                // genuine mid-window composite wrap printed wrapped=false
                // with max = 0xFFFFFFFF-22): the sentinel rides
                // un-netted, only real windows get saturating_sub (which
                // additionally keeps a below-calibration reading from
                // wrapping to a huge net).
                fold_f32_window(&mut hash, window);
                if raw == WORST_WRAPPED {
                    cost.observe(packet, WORST_WRAPPED);
                } else {
                    cost.observe(packet, raw.saturating_sub(overhead));
                }
            },
        );

        let stats = match walk {
            Ok(stats) => stats,
            Err((msg, packet)) => {
                // PR 3 proved this arm decodes; a failure here is the
                // harness regressing the image, not the stream's fault.
                agb::println!(
                    "*** {} FAIL timed walk aborted at packet {}: {}",
                    name,
                    packet,
                    msg
                );
                return None;
            }
        };

        let mut pass = stats.packets == clip.packets && cost.count == clip.packets;
        if !pass {
            agb::println!(
                "*** {} FAIL timed packets={} (walk {}) expected={}",
                name,
                cost.count,
                stats.packets,
                clip.packets
            );
        }
        if cost.wrapped {
            pass = false;
            agb::println!(
                "*** {} FAIL a window exceeded the 32-bit composite (counter wrapped mid-packet): packet {} reported at sentinel {}",
                name,
                cost.max_packet,
                WORST_WRAPPED
            );
        }

        // Same-image witness that the instrumented walk decoded what the
        // proof walk decoded: the timed pass re-folds and re-pins the
        // arm's fnv_walk_fold golden.
        let actual = hash.finish();
        let hash_ok = actual == clip.fnv_walk_fold;
        pass &= hash_ok;
        agb::println!(
            "*** {} timed walk-fold fnv: expected 0x{:016X} actual 0x{:016X} [{}]",
            name,
            clip.fnv_walk_fold,
            actual,
            if hash_ok { "MATCH" } else { "MISMATCH" }
        );

        agb::println!(
            "*** {} stats: count={} min={}@{} max={}@{} sum={} wrapped={} (nets: raw minus overhead={})",
            name,
            cost.count,
            cost.min,
            cost.min_packet,
            cost.max,
            cost.max_packet,
            cost.sum,
            cost.wrapped,
            overhead
        );
        agb::println!(
            "*** {} mean = sum/count = {}/{} — derived host-side (ROM division-free)",
            name,
            cost.sum,
            cost.count
        );
        if cost.wrapped {
            // Reporting rule for a wrapped arm (the wrap witness above
            // already failed the verdict): sum/max/mean from a fold that
            // contains the sentinel are LOWER BOUNDS on the true cost —
            // the wrapped packet's true cycles exceed the composite's
            // whole span. Say it in the log so the numbers are quoted
            // with their meaning.
            agb::println!(
                "*** {} WRAP: at least one window exceeded the 32-bit composite; that packet's true cost is > {} cycles, so sum/mean above are LOWER BOUNDS",
                name,
                WORST_WRAPPED
            );
        }
        let fits = cost.max <= probe::BUDGET_CYCLES_PER_PACKET;
        agb::println!(
            "*** {} budget: worst_packet={} worst_net={} vs budget={} [{}]",
            name,
            cost.max_packet,
            cost.max,
            probe::BUDGET_CYCLES_PER_PACKET,
            if fits { "FITS" } else { "OVER BUDGET" }
        );
        if pass {
            Some((cost, overhead, fits, hash_ok))
        } else {
            None
        }
    }

    #[agb::entry]
    fn main(mut gba: agb::Gba) -> ! {
        agb::eprintln!(
            "[opus-spike] entry started — opus perf-gate ROM (PR 4: cycle harness + per-packet cost)"
        );

        let mut gfx = gba.graphics.get();
        gfx.set_background_palette_colour(0, 0, COLOR_PENDING);

        // Check 1 — decode-path link witness (PR 1's gate, kept: the
        // decode path must survive lto = "fat" for everything below to
        // mean anything).
        let decode_fn = probe::decode_packet as *const () as usize;
        let linked = ROM_MAPPING.contains(&decode_fn);
        agb::println!(
            "[opus-spike] check decode-path link: probe::decode_packet @ 0x{:08X} in 0x08000000..0x0D800000 [{}]",
            decode_fn,
            if linked { "OK" } else { "NOT IN ROM MAPPING" }
        );

        // Check 2 — state-size witness (the EWRAM-plan number; exact
        // value pinned host-side, logged and bound here).
        let state_bytes = probe::DECODER_STATE_BYTES;
        let bounded = state_bytes >= STATE_MIN_SANE && state_bytes < STATE_MAX_SANE;
        agb::println!(
            "[opus-spike] check decoder state: inline {} bytes, within [{}, {}) EWRAM sanity bounds [{}]",
            state_bytes,
            STATE_MIN_SANE,
            STATE_MAX_SANE,
            if bounded { "OK" } else { "OUT OF BOUNDS" }
        );

        // Context line: the derived budget every timing line below is
        // judged against.
        const C_PER_SAMPLE: u32 =
            probe::BUDGET_CYCLES_PER_PACKET / probe::frame_samples(48_000) as u32;
        agb::println!(
            "[opus-spike] context: budget/packet = 20 ms x 16.78 MHz = {} cycles ({} c/sample at {} samples) — derived, host-pinned",
            probe::BUDGET_CYCLES_PER_PACKET,
            C_PER_SAMPLE,
            probe::frame_samples(48_000)
        );

        // Checkpoint — the on-target decode proof, per arm (speech
        // first). pass arms AND into the screen verdict.
        let mut decode_pass = linked && bounded;
        for clip in CLIPS {
            decode_pass &= check_region(clip);
            decode_pass &= decode_proof(clip);
        }

        // Checkpoint — the PR 4 timing passes. Correctness-before-speed:
        // an arm is timed only if its decode proof passed on these
        // bytes. Any failed proof skips timing for every arm and keeps
        // the verdict red — the screen never shows a budget verdict
        // computed from an unproven image, and the log says so.
        let mut pass = decode_pass;
        agb::println!(
            "[opus-spike] checkpoint: decode proof {} — timing pass {}",
            if decode_pass { "PASSED" } else { "FAILED" },
            if decode_pass {
                "engaged"
            } else {
                "SKIPPED (correctness-before-speed: no numbers from an unproven image)"
            }
        );
        let mut timing: [Option<(PacketCost, u32, bool, bool)>; 2] = [None, None];
        if decode_pass {
            // Interrupt-register witnesses around the whole timing
            // section (outside any window). The inside-window witness
            // (IME == 0) is printed by each arm's calibration; here: the
            // state the harness inherits and must restore (agb's
            // entrypoint boots with IME = 1).
            // SAFETY: REG_IE / REG_IF / REG_IME are even I/O-region addresses.
            agb::println!(
                "[opus-spike] irq state before timing: IE=0x{:04X} IF=0x{:04X} IME={}",
                unsafe { io_read_u16(REG_IE) },
                unsafe { io_read_u16(REG_IF) },
                unsafe { io_read_u16(REG_IME) }
            );
            let mut timers = gba.timers.timers();
            let mut counter = Counter::start(&mut timers);
            for (slot, clip) in CLIPS.iter().enumerate() {
                timing[slot] = timing_pass(clip, &mut counter);
                counter.soft_reset();
                // Red on EITHER failure mode — harness failure (None) or
                // a completed measurement that misses budget — but the
                // two counts stay separate in the verdict line: a budget
                // miss is a valid measurement, not a failed harness.
                pass &= timing[slot].as_ref().is_some_and(|t| t.2 && t.3);
            }
            agb::println!(
                "[opus-spike] irq state after timing: IE=0x{:04X} IF=0x{:04X} IME={} (IME=1 means the harness restored it)",
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
            "[opus-spike] PR 4 verdict {} — screen {} (decode proof={}, measured {}/{}, timed-walk hash agree {}/{}, within budget {}/{})",
            if pass { "PASSED" } else { "FAILED" },
            label,
            decode_pass,
            timing.iter().filter(|t| t.is_some()).count(),
            CLIPS.len(),
            timing.iter().flatten().filter(|t| t.3).count(),
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

/// Host-build stub: the ROM entry exists only for `thumbv4t-none-eabi`
/// (see the module docs). `cargo test` never runs this bin
/// (`test = false`), and nothing ships it — this stub exists so host
/// tooling can parse the crate without the (host-incompatible) `agb`
/// dependency.
#[cfg(not(target_arch = "arm"))]
fn main() {}
