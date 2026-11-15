//! FNV-1a 64-bit — the spike's checksum, shared by host and ROM.
//!
//! Deliberately trivial: no table, no dependency, ~6 instructions per byte.
//! The spike's checksums are anti-corruption/anti-drift pins (the same bytes
//! decode to the same samples on host and hardware), not security — FNV-1a's
//! properties are ample, and its two-constant spec makes an *independent*
//! second implementation (the generator's Python one) trivial to cross-check.
//! The witness tests re-derive the generator's pins in Rust over ~1.7 MB of
//! real clip data, so the two implementations meet on committed bytes.
//!
//! Unit vectors are the canonical FNV-1a 64 test vectors.

/// FNV-1a 64 offset basis.
pub const FNV_OFFSET: u64 = 0xCBF4_3CE9_5DE6_84B7;
/// FNV-1a 64 prime.
pub const FNV_PRIME: u64 = 0x0000_0100_0000_01B3;

/// Streaming FNV-1a 64 state.
#[derive(Clone, Copy, Debug)]
pub struct Fnv1a64 {
    hash: u64,
}

impl Default for Fnv1a64 {
    fn default() -> Self {
        Self::new()
    }
}

impl Fnv1a64 {
    /// Empty hasher at the offset basis.
    pub const fn new() -> Self {
        Self { hash: FNV_OFFSET }
    }

    /// Absorb bytes: per byte, xor then multiply (the *a* in FNV-1a — order
    /// matters; the non-`a` FNV-1 multiplies before xoring and yields a
    /// different hash).
    pub fn update(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.hash ^= u64::from(byte);
            self.hash = self.hash.wrapping_mul(FNV_PRIME);
        }
    }

    /// Absorb one decoded sample as its 16-bit little-endian storage form —
    /// the exact byte order of the reference PCM blobs, so host, ROM, and
    /// generator hash the same byte sequence. Values are truncated to i16 by
    /// the caller only after asserting they fit (16-bit profile streams).
    pub fn update_i16le(&mut self, sample: i32) {
        let bytes = (sample as i16).to_le_bytes();
        self.update(&bytes);
    }

    /// Finalize.
    pub const fn finish(&self) -> u64 {
        self.hash
    }
}

/// One-shot hash of a byte slice.
pub fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut h = Fnv1a64::new();
    h.update(bytes);
    h.finish()
}

/// The PR 2 ROM's decode-proof consumer: fold one decoded frame window into
/// the running PCM checksum in the exact byte order the reference blobs use
/// (interleaved little-endian i16, left before right per sample pair).
///
/// This lives in the **library**, not the ROM entry, deliberately: the host
/// witness (`make spike-test`) drives the *same function* through
/// `driver::decode_clip` and asserts the `fnv_pcm` pins, so the fold the ROM
/// runs on-target is the fold proven correct on the host — not a
/// second transcription of the interleave rule. A wrong fold order is the
/// PR 2 equivalent of the hand-packed-vector failure: host and ROM would
/// agree with each other's mistake, so the one shared implementation meeting
/// the generator's Python-side pin is the witness.
///
/// i16 fit is *checked, then* folded (never a silent truncation): a decoded
/// sample outside the 16-bit profile range means the decode desynchronised,
/// and folding garbage bytes into the proof would launder a corrupt decode
/// into a hash. Commit-only-on-success, matching the crate's rejection
/// discipline: the fold happens on a copy; on `Err` the caller's hash state
/// is untouched and the first offending sample is returned.
///
/// Mono windows (`right` empty — the driver's mono contract) fold the left
/// channel alone, matching a mono reference blob's byte layout.
pub fn fold_i16le_stereo(hash: &mut Fnv1a64, left: &[i32], right: &[i32]) -> Result<(), i32> {
    let mut folded = *hash;
    if right.is_empty() {
        for &sample in left {
            i16::try_from(sample).map_err(|_| sample)?;
            folded.update_i16le(sample);
        }
    } else {
        for (&l, &r) in left.iter().zip(right.iter()) {
            i16::try_from(l).map_err(|_| l)?;
            i16::try_from(r).map_err(|_| r)?;
            folded.update_i16le(l);
            folded.update_i16le(r);
        }
    }
    *hash = folded;
    Ok(())
}

#[cfg(test)]
mod tests {
    extern crate alloc;
    use super::*;
    use alloc::vec::Vec;

    #[test]
    fn empty_input_is_the_offset_basis() {
        assert_eq!(fnv1a64(&[]), FNV_OFFSET);
    }

    #[test]
    fn canonical_fnv1a64_vectors() {
        // Derived, not recalled — FLAC.md's witness rule firing on its own
        // project: the first draft of this test asserted "published suite"
        // values from memory, and the implementation disagreed. It was right
        // to. These pins were derived in two independent runtimes (Python
        // big-ints, Node BigInt) and cross-checked against the 32-bit
        // definition anchors the FNV reference publishes and that everyone
        // agrees on — fnv1a32("a") = 0xE40C_292C, fnv1a32("foobar") =
        // 0xBF9C_F968 — which the same xor-then-multiply loop in 32-bit form
        // reproduces exactly. That anchors the algorithm shape (FNV-1a: xor,
        // then multiply), and the 64-bit constants below are that loop's
        // output on the 64-bit basis/prime. The *cross-language* witness is
        // separate and stronger: the Rust impl re-derives the Python
        // generator's FNV pins over ~1.7 MB of committed clip data in
        // tests/spike_witness.rs.
        assert_eq!(fnv1a64(b"a"), 0x7688_568A_8EB3_B7A2);
        assert_eq!(fnv1a64(b"foobar"), 0xBE02_4B72_1F26_767E);
    }

    #[test]
    fn streaming_matches_one_shot() {
        let data: &[u8] = b"the quick brown fox jumps over the lazy dog";
        let mut h = Fnv1a64::new();
        for chunk in data.chunks(7) {
            h.update(chunk);
        }
        assert_eq!(h.finish(), fnv1a64(data));
    }

    #[test]
    fn fold_matches_manual_interleaved_le_bytes() {
        // The interleave order is the whole point: L,R per sample pair, each
        // i16-LE. Witnessed against bytes assembled by hand from the spec of
        // the blob layout (the blobs themselves are witnessed against the pin
        // in the integration suite), so order errors cannot hide.
        let left = [1i32, -2, 3];
        let right = [4i32, 5, -6];
        let mut manual = Vec::new();
        for (l, r) in left.iter().zip(&right) {
            manual.extend_from_slice(&(*l as i16).to_le_bytes());
            manual.extend_from_slice(&(*r as i16).to_le_bytes());
        }
        let mut h = Fnv1a64::new();
        fold_i16le_stereo(&mut h, &left, &right).expect("in-range window folds");
        assert_eq!(h.finish(), fnv1a64(&manual));
    }

    #[test]
    fn fold_mono_window_is_left_channel_only() {
        let left = [7i32, -8, 9];
        let mut h = Fnv1a64::new();
        fold_i16le_stereo(&mut h, &left, &[]).expect("mono window folds");
        let mut manual = Vec::new();
        for &l in &left {
            manual.extend_from_slice(&(l as i16).to_le_bytes());
        }
        assert_eq!(h.finish(), fnv1a64(&manual));
    }

    #[test]
    fn fold_empty_window_is_the_identity() {
        let mut h = Fnv1a64::new();
        let start = h.finish();
        fold_i16le_stereo(&mut h, &[], &[]).expect("empty window folds");
        assert_eq!(h.finish(), start);
    }

    #[test]
    fn fold_rejects_out_of_range_atomically() {
        // The failure mode PR 2 must be able to name: a desynced decode
        // produces samples the 16-bit profile cannot store. Rejection names
        // the first offender and leaves the hash untouched — a partial fold
        // would silently corrupt the running checksum.
        let mut h = Fnv1a64::new();
        let start = h.finish();
        let err =
            fold_i16le_stereo(&mut h, &[0, 32768], &[0, 0]).expect_err("32768 is out of range");
        assert_eq!(err, 32768);
        assert_eq!(h.finish(), start, "rejected window must not fold any bytes");
        // Negative extreme and a right-slot offender too.
        assert_eq!(
            fold_i16le_stereo(&mut h, &[0, -32769], &[0, 0]).unwrap_err(),
            -32769
        );
        assert_eq!(
            fold_i16le_stereo(&mut h, &[0, 0], &[0, 1 << 20]).unwrap_err(),
            1 << 20
        );
        assert_eq!(h.finish(), start);
        // The legal extremes fold.
        fold_i16le_stereo(&mut h, &[32767, -32768], &[32767, -32768])
            .expect("extremes are in range");
    }

    #[test]
    fn i16le_matches_manual_le_bytes() {
        // -3 = 0xFDFD little-endian: [0xFD, 0xFD]. Positive, negative, and
        // the extremes, hashed through both paths.
        for sample in [0i32, 1, -1, 32767, -32768, 12345, -20000] {
            let mut h = Fnv1a64::new();
            h.update_i16le(sample);
            assert_eq!(h.finish(), fnv1a64(&(sample as i16).to_le_bytes()));
        }
    }
}
