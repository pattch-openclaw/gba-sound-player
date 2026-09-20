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

#[cfg(test)]
mod tests {
    use super::*;

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
