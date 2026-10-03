//! FNV-1a 64-bit + the f32→i16 fold — the spike's checksum pair, shared by
//! host witness and ROM.
//!
//! Same discipline as flac_spike's `checksum` module (the sibling crate this
//! one mirrors): FNV-1a is an anti-corruption/anti-drift pin, not security,
//! and its two-constant spec makes an *independent* second implementation
//! (the generator's Python one) trivial to cross-check — the witness tests
//! re-derive the generator's pins in Rust over the committed blobs.
//!
//! The fold is the Opus gate's answer to a measured fact (step 2,
//! 2026-09-30, re-measured and CORRECTED 2026-10-02): ffmpeg/libopus's float
//! decode and the vendored opus-rs port are NOT bit-identical on raw f32
//! (the port's soft-float path differs from libopus's hard-float by up to
//! ~0.8 i16-LSB), so the honest equivalence class is an **i16 grid**: the
//! witness folds BOTH sides with this one shared function and compares
//! there. A fold shared by generator-side expectations, host witness, and
//! the future ROM proof is the witness; two transcriptions of the rule are
//! how host and ROM would agree with each other's mistake.
//!
//! Which i16 grid is itself a measured finding (2026-10-02): truncating
//! toward zero — the vendored decoder's own i16 output convention — turns
//! the port's sub-LSB float drift into a ±1 LSB disagreement on ~50% of the
//! CELT music arm's samples (240,397 / 480,000). Round-to-nearest collapses
//! the same comparison to 127 mismatches (all exactly ±1 LSB, all drift-band
//! crossings); the SILK arm is 0-mismatch on either convention once aligned.
//! So the grid below is **round half up**: the grid that witnesses decoder
//! equivalence rather than amplifying float drift. Half-up (floor(v+0.5)) is
//! pinned over half-away-from-zero (`f32::round`) at the exact .5 cases so
//! the convention is stated, not inherited; the residual 127 samples are
//! measured, capped, and named by the witness (tests/opus_witness.rs
//! layer 2), never absorbed by loosening this function.

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
    /// matters; the non-`a` FNV-1 multiplies before xoring).
    pub fn update(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.hash ^= u64::from(byte);
            self.hash = self.hash.wrapping_mul(FNV_PRIME);
        }
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

/// The f32→i16 fold: scale by 32768, round HALF UP (`floor(v + 0.5)`),
/// clamp to the i16 range. This is the measured equivalence grid between
/// the vendored port and ffmpeg/libopus (module doc; measured 2026-10-02:
/// SILK 0 mismatches, CELT 127 @ exactly ±1 LSB with this rule vs 240,397
/// with the vendored truncate-toward-zero).
///
/// No float library: the crate is `#![no_std]`, so `floor` is not an
/// inherent method — the half-up rounding is done with a saturating `as
/// i32` (truncation toward zero) plus the one-case correction for a negative
/// remainder, inside rails checked on the rounded value. The rails return
/// before the cast; floor-then-clamp and this agree at every input
/// including ±32767.8/−32768.7 LSB (the Python mirror in
/// `scripts/opus_assets.py` clamps after `math.floor` — same values).
/// `NaN` maps to 0 (explicit guard, mirroring the Python fold); no decoded
/// stream here produces NaN.
pub fn fold_f32_to_i16(sample: f32) -> i16 {
    let rounded = sample * 32768.0 + 0.5; // half-up == floor of this value
    if rounded != rounded {
        return 0; // NaN guard (mirrors the Python fold)
    }
    if rounded >= 32767.0 {
        return 32767;
    }
    if rounded < -32768.0 {
        return -32768;
    }
    // Inside the rails the result lies in [-32768, 32766] — always an i16.
    // `as i32` truncates toward zero, so a negative value with a nonzero
    // fraction needs -1 to become the floor. Both terms are exact f32 here
    // (|value| < 2^15).
    let truncated = rounded as i32;
    if rounded - (truncated as f32) < 0.0 {
        (truncated - 1) as i16
    } else {
        truncated as i16
    }
}

/// Fold a decoded/reference f32 window into the running checksum in the
/// exact byte order of the reference blobs (i16 little-endian, one sample
/// per frame — both arms are mono). Commit-only-on-success is trivial here
/// (no fallible step), but the fold is deliberately an `&mut Fnv1a64`
/// update so driver and tests share one folding loop shape.
pub fn fold_f32_window(hash: &mut Fnv1a64, window: &[f32]) {
    for &sample in window {
        hash.update(&fold_f32_to_i16(sample).to_le_bytes());
    }
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
        // Derived, not recalled — same history as flac_spike's identical
        // test (cross-derived in two runtimes, anchored on the 32-bit FNV
        // pins everyone agrees on). The cross-language witness is separate
        // and stronger: the Rust impl re-derives the Python generator's
        // pins over the committed opus blobs in tests/opus_witness.rs.
        assert_eq!(fnv1a64(b"a"), 0x7688_568A_8EB3_B7A2);
        assert_eq!(fnv1a64(b"foobar"), 0xBE02_4B72_1F26_767E);
    }

    #[test]
    fn fold_is_scale_round_half_up_clamp() {
        // The measured equivalence grid, pinned at its discriminating edges:
        // scale, both clamp rails, and the half-up rule at exact .5 and near
        // .5 (the cases that separate half-up from truncate-toward-zero and
        // from round-half-away-from-zero; /32768 and *32768 are exact binary
        // operations, so every LSB fraction below is exact).
        assert_eq!(fold_f32_to_i16(0.0), 0);
        assert_eq!(fold_f32_to_i16(1.0), 32767); // 32768 clamps
        assert_eq!(fold_f32_to_i16(-1.0), -32768); // exact, no clamp
        assert_eq!(fold_f32_to_i16(0.5), 16384);
        assert_eq!(fold_f32_to_i16(-0.5), -16384);
        // Truncate-vs-round discriminators (2026-09-30's grid said 0, 0, 1):
        assert_eq!(fold_f32_to_i16(1.0 / 32768.0), 1);
        assert_eq!(fold_f32_to_i16(0.9 / 32768.0), 1); // half-up (truncate: 0)
        assert_eq!(fold_f32_to_i16(-0.9 / 32768.0), -1); // half-up (truncate: 0)
        assert_eq!(fold_f32_to_i16(1.9 / 32768.0), 2); // half-up (truncate: 1)
        // Exact .5 half-up vs half-AWAY-from-zero (Python's floor(v+0.5) is
        // half-up; f32::round would give -2 here):
        assert_eq!(fold_f32_to_i16(-1.5 / 32768.0), -1);
        assert_eq!(fold_f32_to_i16(1.5 / 32768.0), 2);
        assert_eq!(fold_f32_to_i16(-0.5 / 32768.0), 0); // floor(-0.5 + 0.5) = 0; f32::round would give -1
        assert_eq!(fold_f32_to_i16(10.0), 32767); // saturates high
        assert_eq!(fold_f32_to_i16(-10.0), -32768); // saturates low
        // Rails round before clamping: 32767.8 LSB scales past the i16 top
        // and clamps — never wraps.
        assert_eq!(fold_f32_to_i16(32767.8 / 32768.0), 32767);
        assert_eq!(fold_f32_to_i16(-32768.7 / 32768.0), -32768);
    }

    #[test]
    fn window_fold_matches_one_shot_over_folded_bytes() {
        let window = [0.0f32, 0.25, -0.25, 1.0, -1.0];
        let mut h = Fnv1a64::new();
        fold_f32_window(&mut h, &window);
        let mut expected = [0u8; 10];
        for (i, &s) in window.iter().enumerate() {
            expected[i * 2..i * 2 + 2].copy_from_slice(&fold_f32_to_i16(s).to_le_bytes());
        }
        assert_eq!(h.finish(), fnv1a64(&expected));
    }
}
