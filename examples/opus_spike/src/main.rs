//! Opus spike ROM — PR 3: the on-target decode proof.
//!
//! PURPOSE (see ../../../OPUS.md → "Measurement plan — start here"): PR 1
//! proved the vendored patched `opus-rs` **links** for `thumbv4t-none-eabi`;
//! PR 2 landed the two embedded arms + the host witness. PR 3 (this image)
//! **executes the decode on-target**: every packet of both arms walks
//! through the production `probe::decode_clip` seam — the same transcription
//! the host witness drove and the generator measured the pins against — and
//! folds the decoded PCM into a running FNV-1a-64 through the shared
//! `checksum::fold_f32_window` grid.
//!
//! The pin per arm is `fnv_walk_fold`: the hash of the **vendored walk's own
//! folded output**, measured by the generator over the `dump_walk` dump of
//! this exact seam. It is deliberately NOT the reference-PCM hash: Opus is
//! lossy and the port drifts ±1 LSB from libopus (127 measured residuals on
//! the music arm), so the walk's hash can never equal the reference's — the
//! reference comparison stays the host witness's layer (OPUS.md, step-2
//! findings). The chain that makes the on-target number meaningful is
//! therefore: region-FNV proves the ROM carries the generator's bytes →
//! walk-fold hash reproduces the host walk → host witness (PR 2) proves that
//! walk meets the ffmpeg/libopus reference at the measured alignment.
//!
//! Checkpoints, per arm (mirroring flac_spike PR 2's boot-check shape):
//!
//! 1. region FNV pin — the embedded bytes are the generator's bytes;
//! 2. decode proof — full sequential walk via the production seam, per
//!    packet a running-hash log line (localizes any divergence to the first
//!    diverging packet), walk totals cross-checked against the manifest, and
//!    the final folded hash against the `fnv_walk_fold` golden;
//! 3. EWRAM placement witness — the heap-allocated decoder state and decode
//!    window (PR 3's Box-via-`alloc` pattern) report addresses inside
//!    0x0200_0000..0x0204_0000, making the OPUS.md memory plan a checked
//!    fact instead of PR 1's static bound.
//!
//! What this ROM deliberately does NOT do: measure any cycle (the harness
//! arrives only once this proof is green — correctness-before-speed), or
//! compare decoded output against the reference (the ROM never embeds
//! reference PCM; it pins them by hash host-side).
//!
//! Screen verdict follows the spike convention (works on cart, no cable):
//! **blue = every check passed, red = any failed, purple = never completed.**

#![cfg_attr(target_arch = "arm", no_std)]
#![cfg_attr(target_arch = "arm", no_main)]

#[cfg(target_arch = "arm")]
extern crate alloc;

#[cfg(target_arch = "arm")]
mod rom {
    use agb::display::Rgb;
    use opus_spike::assets::{CLIPS, OpusClip};
    use opus_spike::checksum::{Fnv1a64, fnv1a64, fold_f32_window};
    use opus_spike::probe::{self, WalkStats};

    const COLOR_PENDING: agb::display::Rgb15 = Rgb::new(128, 0, 255).to_rgb15();
    const COLOR_PASS: agb::display::Rgb15 = Rgb::new(0, 0, 255).to_rgb15();
    const COLOR_FAIL: agb::display::Rgb15 = Rgb::new(255, 0, 0).to_rgb15();

    /// Game Pak ROM mirror base — the mapping a linked-in code address must
    /// fall in for the link witness to mean anything.
    const ROM_MAPPING: core::ops::Range<usize> = 0x0800_0000..0x0D80_0000;

    /// EWRAM bounds for the state-size sanity check: 256 KB total EWRAM is
    /// the whole budget the decoder state must fit inside alone (OPUS.md
    /// memory plan); an inline state below 128 KiB would contradict the
    /// measured pin, and above 256 KiB cannot ever fit. Bounds, not the pin
    /// itself — the exact value is pinned host-side where a regression is a
    /// named test failure, not a screen colour.
    const STATE_MIN_SANE: usize = 128 * 1024;
    const STATE_MAX_SANE: usize = 256 * 1024;

    /// EWRAM address range — the heap-allocated decoder state and window
    /// must land here (agb's global allocator is EWRAM's block heap).
    const EWRAM: core::ops::Range<usize> = 0x0200_0000..0x0204_0000;

    #[agb::entry]
    fn main(mut gba: agb::Gba) -> ! {
        agb::eprintln!(
            "[opus-spike] entry started — opus perf-gate ROM decode proof (PR 3: on-target walk + fold + golden)"
        );

        let mut gfx = gba.graphics.get();
        gfx.set_background_palette_colour(0, 0, COLOR_PENDING);

        // Check 1 — decode-path link witness (PR 1's gate, kept: the decode
        // path must survive lto = "fat" for everything below to mean
        // anything).
        let decode_fn = probe::decode_packet as *const () as usize;
        let linked = ROM_MAPPING.contains(&decode_fn);
        agb::println!(
            "[opus-spike] check decode-path link: probe::decode_packet @ 0x{:08X} in 0x08000000..0x0D800000 [{}]",
            decode_fn,
            if linked { "OK" } else { "NOT IN ROM MAPPING" }
        );

        // Check 2 — state-size witness (the EWRAM-plan number; exact value
        // pinned host-side, logged and bound here).
        let state_bytes = probe::DECODER_STATE_BYTES;
        let bounded = state_bytes >= STATE_MIN_SANE && state_bytes < STATE_MAX_SANE;
        agb::println!(
            "[opus-spike] check decoder state: inline {} bytes, within [{}, {}) EWRAM sanity bounds [{}]",
            state_bytes,
            STATE_MIN_SANE,
            STATE_MAX_SANE,
            if bounded { "OK" } else { "OUT OF BOUNDS" }
        );

        // Context line: the derived budget the NEXT PR argues against. The
        // per-sample figure divides at COMPILE time only (the spike keeps
        // software divide out of the ROM, flac_spike's rule extended to
        // reporting paths).
        const C_PER_SAMPLE: u32 =
            probe::BUDGET_CYCLES_PER_PACKET / probe::frame_samples(48_000) as u32;
        agb::println!(
            "[opus-spike] context: budget/packet = 20 ms x 16.78 MHz = {} cycles ({} c/sample at {} samples) — derived, host-pinned; timing itself is the NEXT PR (correctness-before-speed)",
            probe::BUDGET_CYCLES_PER_PACKET,
            C_PER_SAMPLE,
            probe::frame_samples(48_000)
        );

        // Checkpoint 2 — the on-target decode proof, per arm (gate order:
        // speech first). pass arms AND into the screen verdict.
        let mut decode_pass = linked && bounded;
        for clip in CLIPS {
            decode_pass &= check_region(clip);
            decode_pass &= decode_proof(clip);
        }

        agb::println!(
            "[opus-spike] PR 3 verdict {} — screen {} (decode proof {} arms, golden = fnv_walk_fold, no cycle counting)",
            if decode_pass { "PASSED" } else { "FAILED" },
            if decode_pass { "BLUE" } else { "RED" },
            CLIPS.len()
        );
        gfx.set_background_palette_colour(0, 0, if decode_pass { COLOR_PASS } else { COLOR_FAIL });

        loop {
            let frame = gfx.frame();
            frame.commit();
        }
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

    /// PR 3 checkpoint: the on-target decode proof for one arm.
    ///
    /// Walks every packet via `probe::decode_clip` (the production seam —
    /// the same transcription `dump_walk` measured the pins against and the
    /// host witness drove), folds each decoded window into a running
    /// FNV-1a-64 through the shared `fold_f32_window`, and judges the final
    /// hash against the arm's `fnv_walk_fold` golden. Logs are the
    /// deliverable as much as the boolean: per-packet running-hash lines
    /// localize a divergence to the first packet whose fold diverged
    /// (compare against the host walk dump of the same seam), and every
    /// walk-rejection path prints its named error and packet index.
    fn decode_proof(clip: &OpusClip) -> bool {
        let name = clip.name;
        agb::println!(
            "[opus-spike] {} decode proof: walking {} packets via the production seam",
            name,
            clip.packets
        );

        // The 2026-10-03/04 debug bisect block lived here; the 2026-10-04
        // constructor-shape probe ROM (`src/ctor_probe.rs`) now owns the
        // localization (heap capacity, the production shape, and the repro
        // shape as a deliberately-red tail control), so this image carries
        // ONLY the production shape — the shape the host witness and the
        // generator's pins were measured through.
        let mut hash = Fnv1a64::new();
        let mut walked = 0usize;
        let walk = probe::decode_clip(clip, |window| {
            fold_f32_window(&mut hash, window);
            walked += 1;
            agb::println!(
                "[opus-spike] {} packet {:>3}/{} running=0x{:016X}",
                name,
                walked,
                clip.packets,
                hash.finish()
            );
        });

        let stats: WalkStats = match walk {
            Ok(stats) => stats,
            Err((msg, packet)) => {
                // Every tiling/decode guard names its failure at its index;
                // this is the failure mode for "the decoder or the assets
                // regressed inside this image".
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

        // EWRAM placement witness (PR 3's memory-plan check): the decode
        // state and window are heap allocations; under agb the global
        // allocator is EWRAM's block heap, and these addresses witness that
        // claim on this image.
        let ewram_ok = EWRAM.contains(&stats.decoder_addr) && EWRAM.contains(&stats.window_addr);
        pass &= ewram_ok;
        agb::println!(
            "[opus-spike] {} heap placement: decoder=0x{:08X} window=0x{:08X} within EWRAM {} [{}]",
            name,
            stats.decoder_addr,
            stats.window_addr,
            if ewram_ok { "OK" } else { "OUT" },
            if ewram_ok { "OK" } else { "NOT IN EWRAM" }
        );

        // Cross-check the walk totals against the manifest: a packet
        // silently skipped would keep the loop "green" while hashing short
        // PCM. Judged on the driver's own tallies — the callback's counter
        // is derived from the same walk, so comparing them would guard
        // nothing (the tautology rule, OPUS.md step-2 findings).
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
        // (measured over the dump_walk output of the SAME seam). This line
        // is PR 3's gate.
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
}

/// Host-build stub: the ROM entry exists only for `thumbv4t-none-eabi` (see
/// the module docs). `cargo test` never runs this bin (`test = false`), and
/// nothing ships it — this stub exists so host tooling can parse the crate
/// without the (host-incompatible) `agb` dependency.
#[cfg(not(target_arch = "arm"))]
fn main() {}
