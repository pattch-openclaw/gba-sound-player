//! The playback data path — decode-to-FIFO logic, hardware-free.
//!
//! The rate-play ROM's answer to "can we even play back FLAC at a reduced
//! sampling rate": decoded i32 samples (the driver's output) are truncated
//! to the GBA sound hardware's 8-bit FIFO form and pushed through a chunk
//! ring that a timer-IRQ feeder drains at the hardware rate. Everything
//! that can be witnessed on the host lives HERE, pure and `no_std`: the
//! truncation rule, the two-channel FIFO byte folds, the ring's
//! producer/consumer accounting, the underflow classification. The ROM
//! binary (`src/rate_play_main.rs`) owns only registers, timers, and the
//! interrupt wiring around this logic — the same split as `driver`/
//! `checksum` vs `main.rs` (shared-consumer rule, spike harness notes: the
//! host witness drives the exact function the ROM runs).
//!
//! ## Conversion (measured against agb's own path, not invented)
//!
//! The FIFO sample byte is `sample >> 8` taken as u8 — arithmetic shift,
//! two's-complement truncation of the i16-range value to i8. This mirrors
//! agb 0.25.0 exactly: `agb_sound_converter` bakes WAVs with
//! `(sample >> reduction) as u8` where `reduction = bits - 8`, and the
//! mixer asm (`mixer.s`) reads sample bytes back with `ldrsb`. The
//! generator (`frame_vectors.py rate_assets`) derives the FIFO8 pins with
//! the same rule in Python (whose `>>` on signed ints is also arithmetic);
//! the host witness meets the two implementations on the embedded bytes.
//!
//! ## Ring protocol (chunk-granular consumer)
//!
//! Producer (decode loop): converts and appends whole decoded frames.
//! Consumer (feeder IRQ): takes exactly one mixer-domain buffer of samples
//! (176 at 10,512 Hz) per hardware period. A chunk is consumable only when
//! fully written; the producer credits samples only after they are in the
//! ring. Underflows (consumer ready but producer short) are counted, never
//! silently absorbed: the feed plays zeros and the verdict line reports the
//! count. The final partial chunk after the track ends is tail padding, not
//! an underflow — classification is the consumer's `finished` flag.
//!
//! ## Concurrency contract (one producer flow, one interrupt consumer)
//!
//! ARM7TDMI has no atomic instructions and `thumbv4t-none-eabi` ships no
//! `core::sync::atomic` integer types at all — the contract here is the
//! *critical section*, not atomics: the ROM holds `IME` off (via
//! `critical_section`) around every producer access and the feeder runs
//! with interrupts already off, so producer and consumer never interleave
//! mid-operation on a single core. The counters are plain `usize` and the
//! credit-last rule still holds independently: the producer only writes
//! ring bytes at positions the credited `pushed` has not yet exposed, and
//! the consumer only reads up to the credited value — so even a preemption
//! at an instruction boundary (outside a critical section) touches
//! disjoint bytes. A spin-wait re-enters the critical section each
//! iteration, and releasing/re-borrowing the ring forces a fresh load of
//! the counters — the wait terminates.
//! Mono windows (`right` empty) fold the left channel alone.

use crate::checksum::Fnv1a64;

/// Truncate one i16-range decoded sample to its 8-bit FIFO byte, exactly as
/// agb's WAV converter and mixer read it (`>> 8`, two's complement).
#[inline]
#[must_use]
pub fn fifo_byte(sample: i32) -> u8 {
    // i32 >> is arithmetic in Rust (sign-extending), matching Python's >>
    // in the generator and the `as u8` truncation in agb_sound_converter.
    (sample >> 8) as u8
}

/// Check-then-fold both channels' FIFO byte streams into two independent
/// FNV-1a-64 states (left bytes into `hl`, right into `hr`), the playback
/// twin of [`crate::checksum::fold_i16le_stereo`] at the 8-bit FIFO depth.
///
/// Same commit-only-on-success discipline: i16 fit is checked for the whole
/// frame first; a sample outside the profile returns it as `Err` and
/// neither hash moves. Mono windows (`right` empty) fold the left channel
/// alone into `hl`.
pub fn fold_fifo8(
    hl: &mut Fnv1a64,
    hr: &mut Fnv1a64,
    left: &[i32],
    right: &[i32],
) -> Result<(), i32> {
    let mut hl_try = *hl;
    let mut hr_try = *hr;
    if right.is_empty() {
        for &sample in left {
            i16::try_from(sample).map_err(|_| sample)?;
            hl_try.update(&[fifo_byte(sample)]);
        }
    } else {
        for (&l, &r) in left.iter().zip(right.iter()) {
            i16::try_from(l).map_err(|_| l)?;
            i16::try_from(r).map_err(|_| r)?;
            hl_try.update(&[fifo_byte(l)]);
            hr_try.update(&[fifo_byte(r)]);
        }
    }
    *hl = hl_try;
    *hr = hr_try;
    Ok(())
}

/// Why a [`RingFeed::push_frame`] refused. Mirrors
/// [`crate::driver::DriverError`]'s taxonomy: a caller-contract violation
/// names need/have, a stream violation names the sample.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PushError {
    /// The producer started a frame that does not fit the ring's free
    /// capacity — the caller skipped its `free_samples()` gate. Nothing
    /// was written.
    OutOfSpace {
        /// Samples the frame needs per channel.
        need: usize,
        /// Free capacity actually available.
        have: usize,
    },
    /// A decoded sample sits outside the 16-bit profile — the decode
    /// desynchronised. The offending value; nothing was written.
    OutOfProfile(i32),
}

/// What one hardware feeder period should put on the DMA buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Chunk {
    /// `ready >= chunk`: real samples copied into the destination buffers
    /// (both filled exactly `chunk` bytes).
    Full,
    /// Track finished and `0 < ready < chunk`: the first `real` bytes are
    /// real audio, the rest of the destination buffers is zero tail
    /// padding. NOT an underflow.
    Tail {
        /// Real samples at the head of the chunk.
        real: usize,
    },
    /// Nothing ready. The feed plays zeros; the caller counts an underflow
    /// only when the producer is still running (`!finished`).
    Starved,
}

/// A byte-slip ring of 8-bit FIFO samples, planar L/R, chunk-granular
/// consumer. `RING` is the per-channel ring size in samples (= bytes).
///
/// Producer side: [`RingFeed::free_samples`] gates a decode (the caller
/// only starts a frame whose `blocksize` fits), [`RingFeed::push_frame`]
/// converts and appends, crediting samples. Consumer side (under a
/// critical section / IRQ): [`RingFeed::take_chunk`] drains whole chunks
/// at the hardware rate. All counters are monotonic sample counts as
/// atomics (see the module's concurrency contract); ring positions are
/// their `RING` residues, so wrap is arithmetic, not a flag.
pub struct RingFeed<const RING: usize> {
    left: [u8; RING],
    right: [u8; RING],
    /// Samples appended by the producer (monotonic). Plain data: every
    /// shared access happens under the ROM's critical section (module
    /// docs — the ARM7TDMI target has no atomics).
    pushed: usize,
    /// Samples consumed by the feeder (monotonic).
    consumed: usize,
}

impl<const RING: usize> RingFeed<RING> {
    /// Empty ring. `const` so the ROM can hold one in a static.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            left: [0u8; RING],
            right: [0u8; RING],
            pushed: 0,
            consumed: 0,
        }
    }

    /// Samples appended so far (monotonic) — the producer's ledger.
    #[must_use]
    pub fn pushed(&self) -> usize {
        self.pushed
    }

    /// Samples the feeder has taken (monotonic) — the consumer's ledger.
    /// Safe to poll from the producer while the feeder IRQ writes it:
    /// the poller re-enters its critical section (and re-borrows) each
    /// iteration, which forces a fresh load — the wait terminates.
    #[must_use]
    pub fn consumed(&self) -> usize {
        self.consumed
    }

    /// Ready-but-unconsumed samples.
    ///
    /// Never underflows by construction: `consumed` only advances by
    /// samples actually taken out of the ring ([`RingFeed::take_chunk`]
    /// credits the real bytes, not the DMA period size), so the consumer
    /// cursor can never pass the producer frontier.
    #[must_use]
    pub fn ready(&self) -> usize {
        self.pushed() - self.consumed()
    }

    /// Free capacity for the producer.
    #[must_use]
    pub fn free_samples(&self) -> usize {
        RING - self.ready()
    }

    /// Convert one decoded frame (i32, i16-range) and append it, folding
    /// the produced FIFO bytes into the two channel hashes on the way.
    ///
    /// Rejections write nothing (crate-wide rule): free capacity is checked
    /// for the whole frame before a byte is placed ([`PushError::OutOfSpace`]),
    /// i16 fit through [`fold_fifo8`]'s check pass ([`PushError::OutOfProfile`]);
    /// on `Err` neither the ring nor the hashes move.
    pub fn push_frame(
        &mut self,
        left: &[i32],
        right: &[i32],
        hl: &mut Fnv1a64,
        hr: &mut Fnv1a64,
    ) -> Result<(), PushError> {
        let n = left.len();
        let same = right.is_empty() || right.len() == n;
        let free = self.free_samples();
        if n > free || !same {
            return Err(PushError::OutOfSpace {
                need: n,
                have: free,
            });
        }
        // The fold doubles as the whole-frame fit check (commit-only-on-
        // success): it either validates every sample or moves nothing.
        let mut hl_try = *hl;
        let mut hr_try = *hr;
        fold_fifo8(&mut hl_try, &mut hr_try, left, right).map_err(PushError::OutOfProfile)?;

        // Both slips start at the same residue (channels advance in
        // lockstep); the mono push never touches the right ring.
        let mut at = self.pushed() % RING;
        for &l in left {
            self.left[at] = fifo_byte(l);
            at += 1;
            if at == RING {
                at = 0;
            }
        }
        if !right.is_empty() {
            let mut at_r = self.pushed() % RING;
            for &r in right {
                self.right[at_r] = fifo_byte(r);
                at_r += 1;
                if at_r == RING {
                    at_r = 0;
                }
            }
        }
        // Credit last: the consumer only ever reads up to the credited
        // value, so a preemption here exposes either none or all of the
        // frame — never a partially written one.
        self.pushed += n;
        *hl = hl_try;
        *hr = hr_try;
        Ok(())
    }

    /// Drain one hardware period: copy `chunk` samples (bytes) per channel
    /// from the ring into the destination buffers.
    ///
    /// `finished` marks the producer done (track over): then a short
    /// residue is zero-padded tail ([`Chunk::Tail`]) instead of starving.
    /// `Starved` zero-fills the destinations — the DMA is still re-armed
    /// from them, so the feed keeps moving — and the caller counts the
    /// underflow (only when `!finished`).
    ///
    /// The ledger advances by `real`, the bytes actually taken from the
    /// ring: a Starved period consumes NOTHING (zeros are not ring data,
    /// and a cursor that raced ahead of the producer would strand real
    /// samples behind it forever). This is what keeps `consumed <= pushed`
    /// — and `ready()` non-overflowing — an invariant of the ring itself,
    /// independent of how many starved periods the hardware eats through.
    pub fn take_chunk(
        &mut self,
        dst_l: &mut [u8],
        dst_r: &mut [u8],
        chunk: usize,
        finished: bool,
    ) -> Chunk {
        debug_assert_eq!(dst_l.len(), chunk);
        debug_assert_eq!(dst_r.len(), chunk);
        let ready = self.ready();
        let (real, verdict) = if ready >= chunk {
            (chunk, Chunk::Full)
        } else if finished && ready > 0 {
            (ready, Chunk::Tail { real: ready })
        } else {
            (0, Chunk::Starved)
        };
        let at = self.consumed() % RING;
        let head = (RING - at).min(real);
        dst_l[..head].copy_from_slice(&self.left[at..at + head]);
        dst_l[head..real].copy_from_slice(&self.left[..real - head]);
        dst_r[..head].copy_from_slice(&self.right[at..at + head]);
        dst_r[head..real].copy_from_slice(&self.right[..real - head]);
        dst_l[real..].fill(0);
        dst_r[real..].fill(0);
        self.consumed += real;
        verdict
    }
}

// Unit tests ride the rate-assets gate: the module itself always
// compiles (shared lib), but its suite belongs to `make rate-test` —
// `make spike-test`'s counts stay untouched (coexistence contract).
#[cfg(all(test, feature = "rate-assets"))]
mod tests {
    extern crate alloc;
    use super::*;
    use alloc::vec::Vec;

    #[test]
    fn fifo_byte_is_arithmetic_truncation() {
        // Derived from the rule's definition, cross-checked against the
        // agb converter shape `(v >> 8) as u8`:
        assert_eq!(fifo_byte(0), 0x00);
        assert_eq!(fifo_byte(32_512), 0x7F); // i16 max -> 127
        assert_eq!(fifo_byte(-32_768), 0x80); // i16 min -> -128
        assert_eq!(fifo_byte(-300), 0xFE); // -300 >> 8 = -2 (arithmetic)
        assert_eq!(fifo_byte(255), 0x00); // below the first FIFO step
    }

    #[test]
    fn fold_rejects_out_of_profile_and_moves_nothing() {
        let mut hl = Fnv1a64::new();
        let mut hr = Fnv1a64::new();
        let before = (hl.finish(), hr.finish());
        let err = fold_fifo8(&mut hl, &mut hr, &[0, 32_768], &[0, 0])
            .expect_err("out-of-profile must reject");
        assert_eq!(err, 32_768);
        assert_eq!((hl.finish(), hr.finish()), before);
    }

    #[test]
    fn push_rejects_oversized_frame_and_writes_nothing() {
        let mut ring = RingFeed::<8>::new();
        let mut hl = Fnv1a64::new();
        let mut hr = Fnv1a64::new();
        let left: Vec<i32> = core::iter::repeat(256).take(9).collect();
        let right: Vec<i32> = core::iter::repeat(-256).take(9).collect();
        let err = ring
            .push_frame(&left, &right, &mut hl, &mut hr)
            .expect_err("9 > 8 free must reject");
        assert_eq!(err, PushError::OutOfSpace { need: 9, have: 8 });
        assert_eq!(ring.pushed(), 0);
        assert_eq!(hl.finish(), Fnv1a64::new().finish());
        assert_eq!(hr.finish(), Fnv1a64::new().finish());
    }

    #[test]
    fn ring_wraps_and_preserves_stream_order() {
        // Ring smaller than a few frames: forces multiple wraps on both
        // slips while the consumer drains in 3-sample chunks.
        let mut ring = RingFeed::<12>::new();
        let mut hl = Fnv1a64::new();
        let mut hr = Fnv1a64::new();
        let mut want_l: Vec<u8> = Vec::new();
        let mut want_r: Vec<u8> = Vec::new();
        let mut got_l: Vec<u8> = Vec::new();
        let mut got_r: Vec<u8> = Vec::new();
        let mut dl = [0u8; 3];
        let mut dr = [0u8; 3];
        for f in 0..6u32 {
            let left: Vec<i32> = (0..4).map(|i| ((f * 4 + i) as i32 - 8) * 256).collect();
            // 8i32 arithmetic on purpose: the pattern must go negative
            // (from f = 2 onward); usize 8 - f*4 - i overflows instead.
            let right: Vec<i32> = (0..4).map(|i| (8i32 - (f * 4 + i) as i32) * 1337).collect();
            want_l.extend(left.iter().map(|&s| fifo_byte(s)));
            want_r.extend(right.iter().map(|&s| fifo_byte(s)));
            ring.push_frame(&left, &right, &mut hl, &mut hr).unwrap();
            while ring.ready() >= 3 {
                assert_eq!(ring.take_chunk(&mut dl, &mut dr, 3, false), Chunk::Full);
                got_l.extend_from_slice(&dl);
                got_r.extend_from_slice(&dr);
            }
        }
        assert_eq!(ring.pushed(), 24);
        // Producer finished: drain the residue (Tail), then stop.
        loop {
            let v = ring.take_chunk(&mut dl, &mut dr, 3, true);
            match v {
                Chunk::Full => {
                    got_l.extend_from_slice(&dl);
                    got_r.extend_from_slice(&dr);
                }
                Chunk::Tail { real } => {
                    got_l.extend_from_slice(&dl[..real]);
                    got_r.extend_from_slice(&dr[..real]);
                    break;
                }
                Chunk::Starved => break,
            }
        }
        assert_eq!(got_l, want_l);
        assert_eq!(got_r, want_r);
    }

    #[test]
    fn underflow_and_tail_are_classified_apart() {
        let mut ring = RingFeed::<8>::new();
        let mut hl = Fnv1a64::new();
        let mut hr = Fnv1a64::new();
        ring.push_frame(&[256, 512], &[256, 512], &mut hl, &mut hr)
            .unwrap();
        let mut dl = [0xABu8; 4];
        let mut dr = [0xABu8; 4];
        // Producer running, only 2 ready of 4: Starved, destinations zeroed.
        assert_eq!(ring.take_chunk(&mut dl, &mut dr, 4, false), Chunk::Starved);
        assert!(dl.iter().all(|&b| b == 0) && dr.iter().all(|&b| b == 0));
        // The DMA still moved (destinations zero-filled for the re-arm),
        // but zeros are not ring data: the consumer cursor stays put and
        // the residue ahead of it remains real samples, not skipped ones.
        assert_eq!(ring.consumed(), 0);
        // Same starvation after the track is finished, with a residue: Tail.
        // The two earlier real samples were never consumed (a starved
        // period plays zeros and consumes NOTHING), so the tail is the
        // whole residue: 256, 512, then the fresh 768 — FIFO order.
        ring.push_frame(&[768], &[768], &mut hl, &mut hr).unwrap();
        assert_eq!(
            ring.take_chunk(&mut dl, &mut dr, 4, true),
            Chunk::Tail { real: 3 }
        );
        assert_eq!(&dl[..3], &[1u8, 2, 3]);
        assert!(dl[3..].iter().all(|&b| b == 0));
    }

    #[test]
    fn fold_meets_a_second_independent_implementation() {
        // Differential witness for the conversion + channel split: re-derive
        // the two channel hashes over hand-laid-out byte vectors (the
        // interleave split L,R,L,R...), over a pattern that wraps, hits
        // negatives, and crosses i8 boundaries.
        let left: Vec<i32> = (0..40).map(|i| (i - 20) * 511).collect();
        let right: Vec<i32> = (0..40).map(|i| (20 - i) * 1337).collect();
        let mut hl = Fnv1a64::new();
        let mut hr = Fnv1a64::new();
        fold_fifo8(&mut hl, &mut hr, &left, &right).unwrap();
        let l_bytes: Vec<u8> = left.iter().map(|&s| fifo_byte(s)).collect();
        let r_bytes: Vec<u8> = right.iter().map(|&s| fifo_byte(s)).collect();
        assert_eq!(hl.finish(), crate::checksum::fnv1a64(&l_bytes));
        assert_eq!(hr.finish(), crate::checksum::fnv1a64(&r_bytes));
    }
}
