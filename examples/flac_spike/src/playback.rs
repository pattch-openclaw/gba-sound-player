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

/// Pack i16-range-checked samples into their FIFO byte form, four samples
/// per iteration (2026-10-24 convert fast path, the ordering's step ②
/// after the word-copy push).
///
/// The probe measured the per-byte pack at ≈ 55 cyc/channel-sample — the
/// same per-byte-loop overhead shape the push CS carried at 52 cyc/byte.
/// The safe shape cuts iterations by four: `chunks_exact_mut(4)` over the
/// word-multiple head (fixed-4 `copy_from_slice` — constant offsets, and
/// no per-element bounds checks; the index-window draft left four
/// `panic_bounds_check` sites visible in the disassembly), the sub-word
/// tail rides the pinned per-sample walk.
///
/// This crate is `#![forbid(unsafe_code)]` and the pinned nightly has no
/// safe reinterpret path: `align_to_mut` is an *unsafe* fn on this
/// toolchain (measured, E0133) and the safe `align_chunks_mut` does not
/// exist yet (measured, E0599) — so no hand-written `u32`-store shape is
/// available. Measured on the rate-play ROM 2026-10-24: harvest ≈ 1.8 ms
/// of the probe's 14.1 ms stage — the safe-loop ceiling; the word-store
/// remainder needs the asm kernel track (FLAC.md rate-play entry,
/// 2026-10-24 amendment).
///
/// Length mismatch rides the old `zip` semantics: writes
/// `min(src.len(), dst.len())` bytes, nothing beyond.
fn pack_fifo(src: &[i32], dst: &mut [u8]) {
    let n = src.len().min(dst.len());
    let (src, _) = src.split_at(n);
    let (dst, _) = dst.split_at_mut(n);
    let full = (n >> 2) << 2;
    let (sm, st) = src.split_at(full);
    let (dm, dt) = dst.split_at_mut(full);
    let mut words = dm.chunks_exact_mut(4);
    let mut s4 = sm.chunks_exact(4);
    for (w, four) in words.by_ref().zip(&mut s4) {
        // Fixed-4 copy: from_ne_bytes lays the bytes in destination-memory
        // order on any target endianness; the iterator shape removes the
        // per-element bounds checks the index-window left behind.
        w.copy_from_slice(&[
            fifo_byte(four[0]),
            fifo_byte(four[1]),
            fifo_byte(four[2]),
            fifo_byte(four[3]),
        ]);
    }
    for (d, &s) in dt.iter_mut().zip(st) {
        *d = fifo_byte(s);
    }
}

/// Whole-frame i16 profile gate with the pinned error identity.
///
/// Fast path is branch-free: a sample fits the i16 profile iff
/// `(s as u32) + 0x8000` fits in `u16`, so OR-ing every sample's shifted
/// value and testing `acc >> 16` decides the whole frame with one branch.
/// On failure (stream desync — cold), a second pass walks the pinned
/// interleaved order (l0, r0, l1, r1, …; mono: left alone, `zip`-truncated
/// like the original) and returns the first offender, so the `Err` value
/// is identical to the per-sample scalar check's.
fn fit_gate(left: &[i32], right: &[i32]) -> Result<(), i32> {
    let mut acc: u32 = 0;
    if right.is_empty() {
        for &s in left {
            acc |= (s as u32).wrapping_add(0x8000);
        }
    } else {
        for (&l, &r) in left.iter().zip(right.iter()) {
            acc |= (l as u32).wrapping_add(0x8000);
            acc |= (r as u32).wrapping_add(0x8000);
        }
    }
    if acc >> 16 == 0 {
        return Ok(());
    }
    // Cold path: reproduce the pinned check order to pick the first
    // offender (the value the caller sees; arithmetic above only proves
    // one exists).
    if right.is_empty() {
        for &l in left {
            i16::try_from(l).map_err(|_| l)?;
        }
    } else {
        for (&l, &r) in left.iter().zip(right.iter()) {
            i16::try_from(l).map_err(|_| l)?;
            i16::try_from(r).map_err(|_| r)?;
        }
    }
    Ok(())
}

/// Convert one decoded frame (i32, i16-range) into its 8-bit FIFO byte
/// form, writing into caller-provided destinations.
///
/// The whole-frame i16 fit check runs BEFORE any destination byte is
/// written (rejections write nothing, the crate-wide rule): a sample
/// outside the profile returns it as `Err` with the destinations untouched.
/// Mono windows (`right` empty) convert the left channel alone and leave
/// `dst_r` untouched. This is the pure producer-side half of a push — no
/// ring, no hashes, no IRQ-off window needed (the playback push-window fix
/// hoists conversion+fit out of the critical section; [`RingFeed::push_bytes`]
/// is the only part that needs protecting).
///
/// 2026-10-24 fast path (the ordering's step ②): the fit gate is
/// [`fit_gate`] (branch-free OR-accumulate, pinned error identity on the
/// cold path) and the packs go through [`pack_fifo`] (four samples per
/// iteration). Byte-for-byte equality against the pinned scalar shape —
/// including `Err`-value identity and the write-nothing rule — is
/// witnessed in the suite.
pub fn convert_frame(
    left: &[i32],
    right: &[i32],
    dst_l: &mut [u8],
    dst_r: &mut [u8],
) -> Result<(), i32> {
    let n = left.len();
    let stereo = !right.is_empty();
    debug_assert_eq!(dst_l.len(), n);
    debug_assert!(!stereo || (dst_r.len() == n && right.len() == n));
    fit_gate(left, right)?;
    if stereo {
        pack_fifo(right, dst_r);
    }
    pack_fifo(left, dst_l);
    Ok(())
}

/// Fold pre-converted FIFO bytes into the two channel hashes: left bytes
/// into `hl`, right bytes (when present) into `hr`.
///
/// The bytes-level twin of [`fold_fifo8`] — same fold, minus the fit check
/// (bytes are already truncated). The host witness
/// [`mod@tests`] proves byte-for-byte agreement with [`fold_fifo8`] over
/// the same frame, so the ROM's hoisted fold path and the pinned
/// sample-fold path cannot drift.
pub fn fold_bytes(hl: &mut Fnv1a64, hr: &mut Fnv1a64, bytes_l: &[u8], bytes_r: &[u8]) {
    hl.update(bytes_l);
    if !bytes_r.is_empty() {
        hr.update(bytes_r);
    }
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

    /// Append pre-converted FIFO bytes (one decoded frame, planar L/R):
    /// free-capacity check, ring copy, credit last.
    ///
    /// This is the ONLY part of a push that needs the ROM's critical
    /// section: the conversion and fold ([`convert_frame`],
    /// [`fold_bytes`]) are producer-private pure work and run with IRQs
    /// live (the 2026-10-21 push-window fix — audit `a4f7560` measured a
    /// ~64 ms IME-off fold+copy swallowing 3,724 feeder periods). The copy
    /// itself was measured too: the 2026-10-22 position-clock probe timed
    /// the per-byte slip walk at ~52 cyc/byte (12.7 ms per push, 75.9 % of
    /// a period IME-off per frame — exactly the observed swallow ratio).
    /// The 2026-10-23 rewrite hands each contiguous span to one
    /// `copy_from_slice`: the target's memcpy moves word-wide wherever the
    /// span alignment admits it (this crate is `#![forbid(unsafe_code)]`,
    /// so no slice reinterpretation — the word path is the memcpy
    /// dispatch, not hand-written word loads), replacing the hand-rolled
    /// per-byte walk inside the IME-off window. [`Self::push_frame`]'s
    /// per-byte loop stays as the pinned reference the suite witnesses
    /// byte-for-byte equality against.
    ///
    /// Rejections write nothing and credit nothing; mono windows
    /// (`bytes_r` empty) slip the left ring only, same as
    /// [`Self::push_frame`]. The free check reads the live `consumed` —
    /// under the CS it is exact, and outside one a stale read only ever
    /// UNDERstates free (`consumed` only grows), so a passing check can
    /// never overestimate capacity.
    pub fn push_bytes(&mut self, bytes_l: &[u8], bytes_r: &[u8]) -> Result<(), PushError> {
        let n = bytes_l.len();
        let stereo = !bytes_r.is_empty();
        let free = self.free_samples();
        if n > free || (stereo && bytes_r.len() != n) {
            return Err(PushError::OutOfSpace {
                need: n,
                have: free,
            });
        }
        // Both channels start at the same residue (they advance in
        // lockstep), so one wrap split serves both: `[at, RING)` then
        // `[0, rem)`, each span copied whole. The split is a pure
        // relocation of the same bytes — witnessed byte-equal against
        // push_frame's per-byte walk in the suite.
        let at = self.pushed % RING;
        let head = (RING - at).min(n);
        let (l_head, l_tail) = bytes_l.split_at(head);
        self.left[at..at + head].copy_from_slice(l_head);
        self.left[..n - head].copy_from_slice(l_tail);
        if stereo {
            let (r_head, r_tail) = bytes_r.split_at(head);
            self.right[at..at + head].copy_from_slice(r_head);
            self.right[..n - head].copy_from_slice(r_tail);
        }
        // Credit last, same contract as push_frame.
        self.pushed += n;
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
    fn convert_rejects_out_of_profile_and_writes_nothing() {
        let mut dl = [0xABu8; 2];
        let mut dr = [0xABu8; 2];
        let err = convert_frame(&[0, 40_000], &[0, 0], &mut dl, &mut dr)
            .expect_err("out-of-profile must reject");
        assert_eq!(err, 40_000);
        assert_eq!(dl, [0xAB; 2]);
        assert_eq!(dr, [0xAB; 2]);
    }

    #[test]
    fn convert_error_identity_first_offender_interleaved_order() {
        // The fit pass keeps the pinned check order (l, r, l, r, ...): the
        // Err value is the FIRST offender in interleaved stream order,
        // with destinations untouched. Here left[1] = 40 000 is out, but
        // right[0] = 50 000 comes first in the interleave — the old scalar
        // path rejects 50 000, so the fast path must too.
        let mut dl = [0xCDu8; 2];
        let mut dr = [0xCDu8; 2];
        let err = convert_frame(&[0, 40_000], &[50_000, 0], &mut dl, &mut dr)
            .expect_err("out-of-profile must reject");
        assert_eq!(err, 50_000);
        assert_eq!(dl, [0xCD; 2]);
        assert_eq!(dr, [0xCD; 2]);
        // Mono windows leave dst_r alone even for word-multiple lengths.
        let mut dl = [0xEFu8; 8];
        let mut dr = [0xEFu8; 8];
        convert_frame(
            &[256, -256, 256, -256, 256, -256, 256, -256],
            &[],
            &mut dl,
            &mut dr,
        )
        .unwrap();
        assert_eq!(dr, [0xEF; 8], "mono must not touch dst_r");
        assert_eq!(dl, [1, 0xFF, 1, 0xFF, 1, 0xFF, 1, 0xFF]);
    }

    #[test]
    fn convert_fast_path_matches_byte_walk_across_residues() {
        // The convert fast path (2026-10-24: branch-free fit gate +
        // four-sample pack window) must be byte-indistinguishable from the
        // per-byte shape for every dst residue and length. Same meet as
        // the push word-copy test: fast path vs the per-byte intent
        // (fifo_byte per sample — exactly what the pinned push_frame walk
        // stores) vs nothing beyond the slice written.
        // dst slices start at offsets 0..=3 of the destination buffers
        // (word-matched and mismatched middles) and lengths 1..=17 cross
        // word boundaries in every head/tail pairing, stereo and mono.
        // Pattern goes negative and crosses i8 boundaries on purpose.
        let lpat: Vec<i32> = (0..64)
            .map(|i| ((i * 137) % 65_536) as i32 - 32_768)
            .collect();
        let rpat: Vec<i32> = (0..64)
            .map(|i| ((i * 311 + 977) % 65_536) as i32 - 32_768)
            .collect();
        let mut dl = [0u8; 96];
        let mut dr = [0u8; 96];
        for off in 0..4usize {
            for n in 1..=17usize {
                let left = &lpat[off..off + n];
                let right = &rpat[off..off + n];
                // Guard bytes: one byte before and after the slice must
                // never move.
                dl[off + n] = 0xA5;
                dr[off + n] = 0xA5;
                if off > 0 {
                    dl[off - 1] = 0x5A;
                    dr[off - 1] = 0x5A;
                }
                convert_frame(left, right, &mut dl[off..off + n], &mut dr[off..off + n]).unwrap();
                let want_l: Vec<u8> = left.iter().map(|&s| fifo_byte(s)).collect();
                let want_r: Vec<u8> = right.iter().map(|&s| fifo_byte(s)).collect();
                assert_eq!(&dl[off..off + n], want_l.as_slice(), "left off={off} n={n}");
                assert_eq!(
                    &dr[off..off + n],
                    want_r.as_slice(),
                    "right off={off} n={n}"
                );
                assert_eq!(dl[off + n], 0xA5, "left wrote past dst off={off} n={n}");
                assert_eq!(dr[off + n], 0xA5, "right wrote past dst off={off} n={n}");
                if off > 0 {
                    assert_eq!(dl[off - 1], 0x5A, "left wrote before dst off={off} n={n}");
                    assert_eq!(dr[off - 1], 0x5A, "right wrote before dst off={off} n={n}");
                }
            }
        }
        // Mono across residues: dst_r untouched, left word-packed.
        for off in 0..4usize {
            let n = 11usize;
            let mut dl = [0u8; 96];
            let mut dr = [0x77u8; 96];
            convert_frame(&lpat[off..off + n], &[], &mut dl[off..off + n], &mut dr).unwrap();
            let want_l: Vec<u8> = lpat[off..off + n].iter().map(|&s| fifo_byte(s)).collect();
            assert_eq!(&dl[off..off + n], want_l.as_slice(), "mono off={off}");
            assert!(
                dr.iter().all(|&b| b == 0x77),
                "mono touched dst_r off={off}"
            );
        }
    }

    #[test]
    fn push_bytes_path_matches_push_frame_byte_for_byte() {
        // The ROM's hoisted three-step push (convert → fold_bytes →
        // push_bytes) must be indistinguishable from the pinned push_frame:
        // same drained stream, same hashes, same ledger — stereo and mono,
        // across a ring wrap.
        // Ring holds the whole scripted sequence (5×3 stereo + 4 mono = 19
        // ≤ 24) so both rings see identical push sequences before the
        // drain comparison starts.
        let mut a = RingFeed::<24>::new();
        let mut b = RingFeed::<24>::new();
        let (mut hl_a, mut hr_a) = (Fnv1a64::new(), Fnv1a64::new());
        let (mut hl_b, mut hr_b) = (Fnv1a64::new(), Fnv1a64::new());
        for f in 0..5u32 {
            let left: Vec<i32> = (0..3).map(|i| ((f * 3 + i) as i32 - 5) * 777).collect();
            let right: Vec<i32> = (0..3).map(|i| (7i32 - (f * 3 + i) as i32) * 511).collect();
            a.push_frame(&left, &right, &mut hl_a, &mut hr_a).unwrap();
            let mut dl = [0u8; 3];
            let mut dr = [0u8; 3];
            convert_frame(&left, &right, &mut dl, &mut dr).unwrap();
            fold_bytes(&mut hl_b, &mut hr_b, &dl, &dr);
            b.push_bytes(&dl, &dr).unwrap();
        }
        // Mono frame rides the same equality.
        let mono: Vec<i32> = (0..4).map(|i| (i - 2) * 1024).collect();
        a.push_frame(&mono, &[], &mut hl_a, &mut hr_a).unwrap();
        let mut dl = [0u8; 4];
        let mut dr = [0u8; 4];
        convert_frame(&mono, &[], &mut dl, &mut dr).unwrap();
        fold_bytes(&mut hl_b, &mut hr_b, &dl[..4], &[]);
        b.push_bytes(&dl[..4], &[]).unwrap();

        assert_eq!((a.pushed(), a.consumed()), (b.pushed(), b.consumed()));
        let mut got_a: Vec<(Vec<u8>, Vec<u8>)> = Vec::new();
        let mut got_b: Vec<(Vec<u8>, Vec<u8>)> = Vec::new();
        loop {
            // Each ring is taken exactly once per iteration: one take per
            // ring per period, or the comparison desyncs.
            let mut al = [0u8; 4];
            let mut ar = [0u8; 4];
            let mut bl = [0u8; 4];
            let mut br = [0u8; 4];
            let va = a.take_chunk(&mut al, &mut ar, 4, true);
            let vb = b.take_chunk(&mut bl, &mut br, 4, true);
            assert_eq!(va, vb, "classification must agree");
            match va {
                Chunk::Full => {
                    got_a.push((al.to_vec(), ar.to_vec()));
                    got_b.push((bl.to_vec(), br.to_vec()));
                }
                Chunk::Tail { real } => {
                    got_a.push((al[..real].to_vec(), ar[..real].to_vec()));
                    got_b.push((bl[..real].to_vec(), br[..real].to_vec()));
                    break;
                }
                Chunk::Starved => break,
            }
        }
        assert_eq!(got_a, got_b, "drained streams diverged");
        assert_eq!(hl_a.finish(), hl_b.finish(), "left hash diverged");
        assert_eq!(hr_a.finish(), hr_b.finish(), "right hash diverged");
    }

    #[test]
    fn push_bytes_word_copy_matches_byte_walk_across_residues() {
        // The span-copy push (2026-10-23: whole-span copy_from_slice — the
        // target memcpy's word path where alignment admits it) must be
        // byte-indistinguishable from the pinned per-byte walk in
        // push_frame for every residue pairing the ROM can hit. src slices start at offsets 0..=3 of a
        // pattern buffer while the dst residue cycles with `pushed % 12`
        // (odd frame sizes) — covering both the residue-matched (word
        // bulk) and mismatched (byte-fallback) paths — and sizes 1..=5 on
        // a 12-byte ring force wraps on both channels. Reference path:
        // push_frame over samples b << 8 (fifo_byte(b << 8) == b for every
        // u8), so its drained bytes must equal the pattern bytes the word
        // path was handed — a three-way meet: word walk, byte walk,
        // intent.
        let mut byte_ring = RingFeed::<12>::new();
        let mut word_ring = RingFeed::<12>::new();
        let mut hl = Fnv1a64::new();
        let mut hr = Fnv1a64::new();
        let pattern: Vec<u8> = (0..64u32).map(|i| (i * 37 + 11) as u8).collect();
        let mut want_l: Vec<u8> = Vec::new();
        let mut want_r: Vec<u8> = Vec::new();

        // Drain one 3-sample period from both rings in lockstep; the
        // classification must agree and the bytes must match.
        let mut got_l: Vec<u8> = Vec::new();
        let mut got_r: Vec<u8> = Vec::new();
        let mut drain = |a: &mut RingFeed<12>, b: &mut RingFeed<12>, tag: &str| {
            let mut al = [0u8; 3];
            let mut ar = [0u8; 3];
            let mut bl = [0u8; 3];
            let mut br = [0u8; 3];
            let va = a.take_chunk(&mut al, &mut ar, 3, false);
            let vb = b.take_chunk(&mut bl, &mut br, 3, false);
            assert_eq!(va, vb, "classification diverged ({tag})");
            if va == Chunk::Full {
                assert_eq!(al, bl, "left bytes diverged ({tag})");
                assert_eq!(ar, br, "right bytes diverged ({tag})");
                got_l.extend_from_slice(&al);
                got_r.extend_from_slice(&ar);
            }
        };

        let mut push_no = 0u32;
        for _ in 0..3u32 {
            for off in 0..4usize {
                for n in 1..=5usize {
                    let l = &pattern[off..off + n];
                    let r = &pattern[off + 7..off + 7 + n];
                    want_l.extend_from_slice(l);
                    want_r.extend_from_slice(r);
                    // fifo_byte((b as i8) << 8) == b for every u8: the i8
                    // read keeps the i32 in the i16 profile (the fit check
                    // would reject bytes >= 0x80 as u8 << 8) and the
                    // arithmetic >> 8 then ldrsb's back to the exact byte.
                    let left: Vec<i32> = l.iter().map(|&b| i32::from(b as i8) << 8).collect();
                    let right: Vec<i32> = r.iter().map(|&b| i32::from(b as i8) << 8).collect();
                    byte_ring
                        .push_frame(&left, &right, &mut hl, &mut hr)
                        .unwrap();
                    word_ring.push_bytes(l, r).unwrap();
                    push_no += 1;
                    // Keep both rings room for the next push, draining in
                    // lockstep so nothing but the push path is under test.
                    while word_ring.ready() >= 6 {
                        drain(&mut byte_ring, &mut word_ring, "steady drain");
                    }
                }
            }
        }
        assert_eq!(push_no, 60);
        // Producer finished: drain the residue from both rings (Tail then
        // stop) and compare the complete streams against the intent.
        loop {
            let mut al = [0u8; 3];
            let mut ar = [0u8; 3];
            let mut bl = [0u8; 3];
            let mut br = [0u8; 3];
            let va = byte_ring.take_chunk(&mut al, &mut ar, 3, true);
            let vb = word_ring.take_chunk(&mut bl, &mut br, 3, true);
            assert_eq!(va, vb, "classification must agree");
            match va {
                Chunk::Full => {
                    assert_eq!(al, bl, "left residue diverged (word vs byte)");
                    assert_eq!(ar, br, "right residue diverged (word vs byte)");
                    got_l.extend_from_slice(&al);
                    got_r.extend_from_slice(&ar);
                }
                Chunk::Tail { real } => {
                    assert_eq!(&al[..real], &bl[..real], "left tail diverged");
                    assert_eq!(&ar[..real], &br[..real], "right tail diverged");
                    got_l.extend_from_slice(&al[..real]);
                    got_r.extend_from_slice(&ar[..real]);
                    break;
                }
                Chunk::Starved => break,
            }
        }
        assert_eq!(byte_ring.pushed(), word_ring.pushed(), "ledgers diverged");
        assert_eq!(
            got_l, want_l,
            "left stream diverged (word vs byte vs intent)"
        );
        assert_eq!(
            got_r, want_r,
            "right stream diverged (word vs byte vs intent)"
        );
    }

    #[test]
    fn push_bytes_rejects_oversized_frame_and_credits_nothing() {
        let mut ring = RingFeed::<4>::new();
        let err = ring
            .push_bytes(&[1, 2, 3, 4, 5], &[1, 2, 3, 4, 5])
            .expect_err("5 > 4 free must reject");
        assert_eq!(err, PushError::OutOfSpace { need: 5, have: 4 });
        assert_eq!(ring.pushed(), 0);
        // Length mismatch on the right channel is rejected too.
        let err = ring
            .push_bytes(&[1, 2], &[3])
            .expect_err("mismatch must reject");
        assert_eq!(err, PushError::OutOfSpace { need: 2, have: 4 });
        assert_eq!(ring.pushed(), 0);
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
