//! PR 4 host witness: the **timed walk seam** (`probe::walk_region_timed`)
//! and the ROM harness's fold wiring (`stats::PacketCost`).
//!
//! The ROM's timing pass drives `walk_region_timed` with a real timer
//! window; this suite drives the *same* seam with synthetic windows so the
//! three contract legs the ROM depends on are witnessed on the host:
//!
//! 1. **Identity** — with a pass-through measurement closure, the timed
//!    walk is indistinguishable from `walk_region` (the pinned-walk
//!    spelling): same `WalkStats`, bit-identical windows, and the fold of
//!    its output reproduces the arm's `fnv_walk_fold` golden. This is the
//!    host half of the ROM's "timed walk-fold fnv MATCH" line: if the
//!    instrumented walk ever decoded different work than the pinned walk,
//!    this test goes red before any ROM does.
//! 2. **Raw passthrough + fold wiring** — `measure` runs once per packet,
//!    the raw value it returns reaches `on_packet` unchanged and in order,
//!    and folding that synthetic sequence through `PacketCost` yields the
//!    min/max/sum/worst-index the arithmetic says it should (division-free;
//!    derived from the sequence, never recalled).
//! 3. **Guard order** — the manifest tiling guards fire **before**
//!    `measure` is ever reached (a timed pass that decoded a lying
//!    manifest would measure work the product never ships), while the
//!    decode rejection happens *inside* a window (measure ran exactly
//!    once) and names its packet.
//! 4. **Caller contract** — a measurement closure that invokes the decode
//!    zero or two times fails with the named contract error at its index,
//!    never a silently skipped or double-decoded packet (the state machine
//!    decoder makes double-decode desync exactly the mistake to refuse).
//!
//! Negative controls are each paired with the identity positive control
//! from layer 1: the guard rejects because of the corruption, not because
//! everything fails.

use opus_spike::assets::{CLIPS, OpusClip};
use opus_spike::checksum::{Fnv1a64, fold_f32_window};
use opus_spike::probe::{self, WalkStats};
use opus_spike::stats::PacketCost;

/// Synthetic raw cycle values handed back by a pass-through `measure`
/// closure — deterministic, per-packet distinct, and deliberately not
/// monotone (a monotone sequence would hide a worst-index tie bug and a
/// min-index tie bug simultaneously).
fn synthetic_raw(packet: usize) -> u32 {
    // (packet * 7919 + 31) mod 503 — 7919 and 503 are coprime, so the
    // sequence permutes 0..502 before wrapping: every index distinct
    // within the first 503 packets (the clips are 501 packets).
    ((packet as u64 * 7919 + 31) % 503) as u32
}

/// Drive `walk_region_timed` with the pass-through closure: `measure`
/// invokes decode exactly once and reports `synthetic_raw(packet)`;
/// `on_packet` collects bit-patterns of every window and its raw.
#[allow(clippy::type_complexity)]
fn pass_through_walk(
    clip: &OpusClip,
    region: &[u8],
) -> Result<
    (
        WalkStats,
        Vec<Vec<u32>>,
        Vec<u32>,
        usize,
        Vec<(usize, usize, u32)>,
    ),
    (&'static str, usize),
> {
    let mut windows: Vec<Vec<u32>> = Vec::new();
    let mut raws: Vec<u32> = Vec::new();
    let mut measure_calls = 0usize;
    let mut order: Vec<(usize, usize, u32)> = Vec::new();
    let mut packet_index = 0usize;
    let stats = probe::walk_region_timed(
        region,
        clip.index.iter().map(|e| (e.offset, e.len)),
        clip.sample_rate_hz,
        clip.channels,
        &mut |decode| {
            let i = packet_index;
            packet_index += 1;
            measure_calls += 1;
            decode();
            synthetic_raw(i)
        },
        |packet, window, raw| {
            order.push((packet, window.len(), raw));
            windows.push(window.iter().map(|&s| s.to_bits()).collect());
            raws.push(raw);
        },
    )?;
    Ok((stats, windows, raws, measure_calls, order))
}

/// Layer 1 + 1b: identity against the pinned walk, and the timed walk's
/// own folded output reproducing the ROM decode proof's golden.
#[test]
fn timed_walk_is_indistinguishable_from_the_pinned_walk() {
    for clip in CLIPS {
        // Reference leg: the pinned walk (what the generator measured the
        // fnv_walk_fold pins over).
        let mut reference_windows: Vec<Vec<u32>> = Vec::new();
        let reference = probe::decode_clip(clip, |window| {
            reference_windows.push(window.iter().map(|&s| s.to_bits()).collect());
        })
        .expect("pinned walk accepts the committed manifest");

        let (stats, windows, raws, measure_calls, order) =
            pass_through_walk(clip, clip.region).expect("timed walk must accept the same manifest");

        // packets/samples ONLY — deliberately NOT assert_eq!(stats,
        // reference): WalkStats carries the heap addresses of each walk's
        // own decoder Box + window Box, and the ~178 KB state is a
        // large-allocation (mmap class on the host) whose address is not
        // stable across allocations — two walks of the same manifest
        // legitimately land at different addresses, and under parallel
        // test load that is a flake (caught 2026-10-10: the whole-struct
        // comparison failed once in five runs). The ROM's EWRAM witness
        // judges each walk's OWN addresses; the cross-walk contract is
        // the tallies and the samples, which the assertions below cover
        // bit-exactly.
        assert_eq!(stats.packets, reference.packets, "{}: packets", clip.name);
        assert_eq!(stats.samples, reference.samples, "{}: samples", clip.name);
        assert_eq!(
            windows, reference_windows,
            "{}: timed walk decoded bit-different samples than the pinned walk",
            clip.name
        );
        assert_eq!(
            measure_calls, clip.packets,
            "{}: measure ran {} times, one per packet expected",
            clip.name, measure_calls
        );
        assert_eq!(raws.len(), clip.packets, "{}: raw count", clip.name);
        for (i, (packet, window_len, raw)) in order.iter().enumerate() {
            assert_eq!(*packet, i, "{}: packet index out of order", clip.name);
            assert_eq!(
                *window_len,
                probe::frame_samples(clip.sample_rate_hz),
                "{}: window geometry at {}",
                clip.name,
                i
            );
            assert_eq!(
                *raw,
                synthetic_raw(i),
                "{}: raw passthrough at {}",
                clip.name,
                i
            );
        }

        // The golden leg: fold the timed walk's windows through the shared
        // grid — this is exactly what the ROM's timing pass prints as
        // "timed walk-fold fnv".
        let mut hash = Fnv1a64::new();
        for bits in &windows {
            let window: Vec<f32> = bits.iter().map(|b| f32::from_bits(*b)).collect();
            fold_f32_window(&mut hash, &window);
        }
        assert_eq!(
            hash.finish(),
            clip.fnv_walk_fold,
            "{}: timed-walk fold does not reproduce the fnv_walk_fold golden",
            clip.name
        );
    }
}

/// Layer 2: the ROM's fold wiring — `PacketCost` folded from the timed
/// walk's own per-packet raws (minus a constant overhead, like the ROM's
/// netting) yields min/max/sum/worst-index derived from the synthetic
/// sequence.
#[test]
fn packet_cost_fold_wires_to_the_walks_raw_sequence() {
    const OVERHEAD: u32 = 7;
    for clip in CLIPS {
        let (_, _, raws, _, _) = pass_through_walk(clip, clip.region).unwrap();
        let mut cost = PacketCost::new();
        for (i, raw) in raws.iter().enumerate() {
            cost.observe(i, raw.saturating_sub(OVERHEAD));
        }
        assert_eq!(cost.count, clip.packets);
        assert!(!cost.wrapped);
        // Derive the expectation from the sequence itself (second
        // independent pass; ties keep the FIRST index like the fold).
        let nets: Vec<u32> = raws.iter().map(|r| r.saturating_sub(OVERHEAD)).collect();
        let mut min = u32::MAX;
        let mut min_i = 0;
        let mut max = 0u32;
        let mut max_i = 0usize;
        let mut sum = 0u64;
        for (i, &n) in nets.iter().enumerate() {
            sum += u64::from(n);
            if n < min {
                min = n;
                min_i = i;
            }
            if n > max {
                max = n;
                max_i = i;
            }
        }
        assert_eq!(cost.min, min);
        assert_eq!(cost.min_packet, min_i);
        assert_eq!(cost.max, max);
        assert_eq!(cost.max_packet, max_i);
        assert_eq!(cost.sum, sum);
        assert_eq!(cost.mean(), Some(sum / clip.packets as u64));
    }
}

/// Layer 3a: the manifest tiling guards fire BEFORE any measurement — a
/// timed pass must never open a window over a manifest the pinned walk
/// would also reject.
#[test]
fn guard_rejections_never_reach_the_measure_closure() {
    for clip in CLIPS {
        // Four corruption shapes the tiling guard claims to catch; each
        // must name itself at its index with measure_calls == 0.
        let cases: Vec<(&str, Vec<(u32, u16)>, usize)> = {
            let mut past_region: Vec<(u32, u16)> =
                clip.index.iter().map(|e| (e.offset, e.len)).collect();
            let last = past_region.len() - 1;
            past_region[last].1 += 1; // final length past region end

            let mut gap: Vec<(u32, u16)> = clip.index.iter().map(|e| (e.offset, e.len)).collect();
            gap[2].1 -= 1; // cursor != next offset at index 3

            let mut empty = clip
                .index
                .iter()
                .map(|e| (e.offset, e.len))
                .collect::<Vec<(u32, u16)>>();
            empty[4].1 = 0; // empty packet at 4

            let short_table: Vec<(u32, u16)> = clip
                .index
                .iter()
                .map(|e| (e.offset, e.len))
                .take(clip.packets - 1)
                .collect();
            // Final cursor = region_len minus the dropped entry's length:
            // the tiling pass blames the LAST processed entry, index
            // count-1 = packets-2.

            vec![
                ("manifest runs past region", past_region, last),
                ("manifest gap/overlap", gap, 3),
                ("manifest empty packet", empty, 4),
                (
                    "manifest does not tile region",
                    short_table,
                    clip.packets - 2,
                ),
            ]
        };
        for (want_error, entries, want_index) in cases {
            let mut measure_calls = 0usize;
            let result = probe::walk_region_timed(
                clip.region,
                entries.into_iter().collect::<Vec<_>>().into_iter(),
                clip.sample_rate_hz,
                clip.channels,
                &mut |decode| {
                    measure_calls += 1;
                    decode();
                    0
                },
                |_packet, _window, _raw| {},
            );
            assert_eq!(
                result.err(),
                Some((want_error, want_index)),
                "{}: corrupt manifest {want_error:?} must fail at {want_index} (identity positive control: the real table walks Ok in the layer-1 test)",
                clip.name
            );
            assert_eq!(
                measure_calls, 0,
                "{}: {want_error:?} must reject before any measurement window opens",
                clip.name
            );
        }
    }
}

/// Layer 3b: a packet the decoder itself rejects fails INSIDE the window
/// (measure ran exactly once for it) and names its index. The corruption
/// keeps the manifest tiling intact — only the packet's TOC grows the
/// geometry past the seam's pinned 960-sample window, which is exactly
/// the manifest-vs-TOC mismatch the seam doc says must be fail-closed.
#[test]
fn decode_rejection_names_its_packet_with_one_measure_call() {
    for clip in CLIPS {
        // TOC byte: bits 7..3 = config, bit 2 = stereo, bits 1..0 = frame
        // count code. The census pins every packet mono, code 0, one
        // frame; rewrite packet 0's config to the 60 ms duration of the
        // same family (silk: 9 → 11 WB-60; celt FB 20: 31 stays 31
        // config family — for CELT flip the frame-count code to 3 = up
        // to 3 frames instead, also exceeding the 960-sample window).
        let mut region: Vec<u8> = clip.region.to_vec();
        let toc = region[0];
        if clip.toc_config == 9 {
            region[0] = (toc & 0b0000_0111) | (11 << 3); // SILK WB 60 ms
        } else {
            region[0] = (toc & 0b1111_1100) | 0b11; // CELT vbr, 3 frames
        }
        let mut measure_calls = 0usize;
        let result = probe::walk_region_timed(
            &region,
            clip.index.iter().map(|e| (e.offset, e.len)),
            clip.sample_rate_hz,
            clip.channels,
            &mut |decode| {
                measure_calls += 1;
                decode();
                42
            },
            |_packet, _window, _raw| {},
        );
        assert_eq!(
            result.err(),
            Some(("decode rejected packet", 0)),
            "{}: TOC-geometry mismatch past the pinned window must be rejected at packet 0",
            clip.name
        );
        assert_eq!(
            measure_calls, 1,
            "{}: decode rejection happens inside the window — measure ran {} times",
            clip.name, measure_calls
        );
    }
}

/// Layer 4: the caller contract — `measure` must invoke decode exactly
/// once per packet. Zero invocations would silently skip; two would
/// desync the state machine. Both fail with the named error at the packet
/// index (here, packet 0 in both cases).
#[test]
fn measure_closure_must_invoke_decode_exactly_once() {
    let clip = CLIPS[0];
    // Zero invocations.
    let mut result = probe::walk_region_timed(
        clip.region,
        clip.index.iter().map(|e| (e.offset, e.len)),
        clip.sample_rate_hz,
        clip.channels,
        &mut |_decode| 0,
        |_packet, _window, _raw| {},
    );
    assert_eq!(
        result.err(),
        Some(("measure closure must invoke decode exactly once", 0)),
        "zero invocations must be refused, not skipped"
    );
    // Two invocations.
    result = probe::walk_region_timed(
        clip.region,
        clip.index.iter().map(|e| (e.offset, e.len)),
        clip.sample_rate_hz,
        clip.channels,
        &mut |decode| {
            decode();
            decode();
            0
        },
        |_packet, _window, _raw| {},
    );
    assert_eq!(
        result.err(),
        Some(("measure closure must invoke decode exactly once", 0)),
        "double invocation must be refused, not double-decoded"
    );
}
