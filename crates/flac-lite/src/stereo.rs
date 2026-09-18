//! Stereo decorrelation (RFC 9639 §4.2 / §9.2.1 channel bits).
//!
//! IMPLEMENTED (Phase 1 step 3f, part 1): [`decorrelate`] — the recombination
//! transforms. Frame wiring (the subframe loop that feeds them) is
//! `frame::decode_frame`, the remaining part of 3f.
//!
//! FLAC stores a stereo pair as two subframes plus a channel-assignment code
//! that says how to recombine them. Cheap integer work — one add, one shift,
//! one subtract per sample — and the whole transform is **per-sample
//! independent**: each output pair reads only the inputs at its own index, so
//! "not in place safe" means only *no aliasing tricks across indices* —
//! temporaries within one index are the whole requirement.
//!
//! ## Orientation (which slot holds what)
//!
//! Subframe 0 is the primary slot, subframe 1 the secondary, per the
//! channel-assignment table (RFC 9639 Table 16, restated in
//! [`crate::format::ChannelConfig`]'s docs):
//!
//! | Assignment | Subframe 0 (primary) | Subframe 1 (secondary) |
//! |---|---|---|
//! | `0b1000` left/side | left | side = L−R |
//! | `0b1001` side/right | **side** = L−R | right |
//! | `0b1010` mid/side | mid = (L+R)>>1 | side = L−R |
//!
//! **Corrected 2026-09-15 — the scaffold's orientation note contradicted the
//! crate's own enum docs, and the docs contradicted each other.** This
//! module's scaffold comment claimed "right/side: `right` = primary", i.e.
//! subframe 0 holds the right channel, while
//! [`crate::format::ChannelConfig::RightSide`]'s doc said the opposite
//! ("subframe 0 is the side, subframe 1 is the right"). Neither doc was
//! trusted (project rule); the question was measured. `decorrelate` follows
//! **Table 16**, whose `0b1001` row reads "2 channels: left, right; **stored
//! as side-right** stereo" — storage order named, side first.
//!
//! Witness chain for the orientation (RFC section + encoder measurement, per
//! the FLAC.md rule):
//!
//! * RFC 9639 Table 16 (`0b1001` = "stored as side-right") and §4.2:
//!   "*Side-right*… To decode, the left subblock is restored by **adding**
//!   the samples in the side subframe to the corresponding samples in the
//!   right subframe" — the side is a stored subframe whose position the
//!   decode names first.
//! * libFLAC 1.5.0 `undo_channel_coding` (the format's reference
//!   implementation): the RIGHT_SIDE arm is `output[0][i] += output[1][i]` —
//!   slot 0 accumulates the side into R, so slot 0 holds the side.
//! * Encoder bytes (`drafts/flac3f_width_probe.py`, throwaway): a
//!   near-identical-channels source (R smooth, L = R + tiny quantized noise)
//!   makes libFLAC choose `0b1001` on **63/63 frames**; reading subframe 0's
//!   warm-up (the subframe's own first decoded samples — step 3a's finding)
//!   at bps+1 matches `L−R` from `flac -d` on **12/12 fingerprinted frames**
//!   — subframe 0 is the side, directly from libFLAC bytes. Same script
//!   censuses the other constructions, which is how rare `0b1001` otherwise
//!   is (`-m` picks mid/side on correlated sources — see FLAC.md "What the
//!   encoder actually chooses").
//!
//! The oracle vectors below pin the *math* (encoder-side store → decoder-side
//! recovery of the generated ground truth); the citations above pin the
//! *orientation* — a math oracle alone could not, because `store` and the
//! recovery formulas are the same reading of the spec and would agree
//! even if the reading were wrong.
//!
//! ## The mid/side LSB
//!
//! `mid = (L+R)>>1` drops the LSB of `L+R`, and that LSB is recoverable:
//! `(L+R)` and `(L−R)` have the same parity (their difference is `2R`), so
//! `side & 1` **is** the lost bit. Hence `m_ext = (mid << 1) | (side & 1)`
//! reconstructs `L+R` exactly, and `left = (m_ext + side) >> 1`,
//! `right = (m_ext - side) >> 1` are exact — `m_ext ± side` are even by
//! construction, so the halving never rounds. No rounding term exists:
//! the `&1` recovery *is* the `+1`.
//!
//! (Storage precision — *how many bits* each subframe is decoded at — is the
//! frame layer's seam, `frame::decode_frame`: the side subframe is coded at
//! **frame bps + 1**, §4.2's "the side channel needs one extra bit of bit
//! depth", measured on encoder bytes (FLAC.md → step 3f entries; libFLAC
//! `read_subframe_` does `bps++` on the side slot). `decorrelate` itself is
//! width-agnostic: it recombines decoded samples whatever width produced
//! them, so its vectors exercise values beyond 16-bit range — e.g.
//! `(32767, −32768)` stores side `65535`.)
//!
//! Accumulation follows the crate's wide-intermediate convention: the adds,
//! shifts, and halvings run in `i64` and truncate at the store, so
//! `mid << 1` at the top of the i32 range cannot wrap silently.

use crate::format::ChannelConfig;

/// Recombine two decoded subframes into left/right PCM, in place.
///
/// On entry, `left` holds the **primary** subframe (subframe 0) and `right`
/// the **secondary** (subframe 1) — for side/right, that means `left` holds
/// the side (see the orientation table in the module docs). On exit they
/// hold final PCM. Exactly `blocksize` samples are processed; samples beyond
/// `blocksize` in either buffer are left untouched (decode buffers are
/// reused across frames of differing blocksize — the final frame is
/// legitimately short).
///
/// [`ChannelConfig::Independent`] is the identity: mono streams may pass a
/// zero-length `right` (the caller has no secondary subframe), so the
/// length contract for that arm covers `left` only.
///
/// Rejections write nothing (crate rule): both length contracts gate before
/// the first sample is touched; a short buffer is a caller-contract
/// violation → [`crate::Error::InvalidField`].
///
/// The transforms are per-sample independent (each output reads only its own
/// index's inputs), so the in-place update is exact with a within-index
/// temporary; no history, no cross-sample state, no allocation.
pub fn decorrelate(
    config: ChannelConfig,
    blocksize: usize,
    left: &mut [i32],
    right: &mut [i32],
) -> crate::Result<()> {
    match config {
        ChannelConfig::Independent { channels: 1 } => {
            if left.len() < blocksize {
                return Err(crate::Error::InvalidField);
            }
            Ok(())
        }
        ChannelConfig::Independent { .. } => {
            if left.len() < blocksize || right.len() < blocksize {
                return Err(crate::Error::InvalidField);
            }
            Ok(())
        }
        ChannelConfig::LeftSide => {
            if left.len() < blocksize || right.len() < blocksize {
                return Err(crate::Error::InvalidField);
            }
            for (l, s) in left[..blocksize].iter_mut().zip(&mut right[..blocksize]) {
                let (l0, s0) = (*l, *s);
                // L is the primary as stored; R = L − side.
                *l = l0;
                *s = (i64::from(l0) - i64::from(s0)) as i32;
            }
            Ok(())
        }
        ChannelConfig::RightSide => {
            if left.len() < blocksize || right.len() < blocksize {
                return Err(crate::Error::InvalidField);
            }
            for (s, r) in left[..blocksize].iter_mut().zip(&mut right[..blocksize]) {
                let (s0, r0) = (*s, *r);
                // Primary slot holds the SIDE; R is the secondary as stored;
                // L = side + R.
                *s = (i64::from(s0) + i64::from(r0)) as i32;
                *r = r0;
            }
            Ok(())
        }
        ChannelConfig::MidSide => {
            if left.len() < blocksize || right.len() < blocksize {
                return Err(crate::Error::InvalidField);
            }
            for (m, s) in left[..blocksize].iter_mut().zip(&mut right[..blocksize]) {
                let (m0, s0) = (*m, *s);
                // Recover the stolen LSB (side & 1 — parity identity in the
                // module docs), then halve exactly: m_ext ± side are even.
                let m_ext = (i64::from(m0) << 1) | (i64::from(s0) & 1);
                *m = ((m_ext + i64::from(s0)) >> 1) as i32;
                *s = ((m_ext - i64::from(s0)) >> 1) as i32;
            }
            Ok(())
        }
    }
}

// Unit tests compile as part of the lib under the host test harness (Gate 2,
// run from *outside* the repo — see README "Cargo config leak"). `core`-only,
// like the rest of the crate's unit tests.
//
// Witness rule (generation, not assertion — the discipline FLAC.md records
// firing on every step so far): the vectors were **emitted mechanically** by
// `drafts/flac3f_stereo_oracle.py` (throwaway, outside the repo). For each
// true (L, R) pair the oracle computes the *stored* subframe pair with the
// encoder-side formulas (side = L−R, mid = (L+R)>>1 arithmetic); the test
// then requires `decorrelate` to recover the original (L, R). A wrong
// decoder-side formula, a wrong orientation, or a broken LSB recovery fails
// recovery on generated data — it cannot pass by agreeing with a shared
// hand-transcription, because nothing here is hand-packed and the constants
// are inserted from the oracle's own output.
//
// What these vectors do and do not witness, stated exactly: they pin the
// *transform math* per mode (store by the encoder-side formula, require
// recovery). The *orientation* of side/right — which slot holds what — is a
// claim about encoder bytes, and it is witnessed separately (module docs:
// RFC Table 16 + §4.2 + libFLAC `undo_channel_coding`, plus a measured
// 63/63-frame `0b1001` encode whose subframe-0 warm-up fingerprints as the
// side). PR B's end-to-end PCM diff over real libFLAC stereo frames re-witnesses
// both together, bit-exact against `flac -d`.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::Error;

    // ---- oracle-generated vectors (drafts/flac3f_stereo_oracle.py) --------
    // PAIRS is ground truth; the *_STORED arrays are the encoder-side pair
    // for each mode, generated from PAIRS — never hand-written.

    // (L, R) pairs: hand-picked extremes + LCG sweep, all in the
    // 16-bit signed domain. Stored pairs + recovery asserted below.
    const PAIRS: [(i32, i32); 55] = [
        (0, 0),
        (1, 0),
        (0, 1),
        (-1, 0),
        (0, -1),
        (1, 1),
        (-1, -1),
        (1, -1),
        (32767, 32767),
        (-32768, -32768),
        (32767, -32768),
        (-32768, 32767),
        (32767, 1),
        (-32768, -2),
        (32766, -32767),
        (-32767, 32766),
        (5, -4),
        (-5, 4),
        (2, 4),
        (-2, -4),
        (16645, -2222),
        (-12345, 6789),
        (100, -100),
        (-5325, 25311),
        (1023, 13336),
        (-12680, 22638),
        (23538, 9761),
        (-17335, 6713),
        (-9263, -19161),
        (636, 14655),
        (15004, 22780),
        (-11520, 10799),
        (-17324, -11881),
        (8435, 25486),
        (11424, 28339),
        (6860, -16172),
        (-11670, -10130),
        (18802, 29326),
        (-15230, 234),
        (-25503, 892),
        (18233, 9967),
        (-5429, 15027),
        (6919, 4486),
        (-20044, -7493),
        (-7132, -4962),
        (5908, -16975),
        (4913, 28330),
        (31224, 11878),
        (-2352, 12096),
        (6048, 24189),
        (-8634, 22202),
        (-29279, -5744),
        (29983, 3800),
        (16643, 21834),
        (-16438, 23643),
    ];

    // stored left_side subframe pairs (subframe0, subframe1)
    const LEFT_SIDE_STORED: [(i32, i32); 55] = [
        (0, 0),
        (1, 1),
        (0, -1),
        (-1, -1),
        (0, 1),
        (1, 0),
        (-1, 0),
        (1, 2),
        (32767, 0),
        (-32768, 0),
        (32767, 65535),
        (-32768, -65535),
        (32767, 32766),
        (-32768, -32766),
        (32766, 65533),
        (-32767, -65533),
        (5, 9),
        (-5, -9),
        (2, -2),
        (-2, 2),
        (16645, 18867),
        (-12345, -19134),
        (100, 200),
        (-5325, -30636),
        (1023, -12313),
        (-12680, -35318),
        (23538, 13777),
        (-17335, -24048),
        (-9263, 9898),
        (636, -14019),
        (15004, -7776),
        (-11520, -22319),
        (-17324, -5443),
        (8435, -17051),
        (11424, -16915),
        (6860, 23032),
        (-11670, -1540),
        (18802, -10524),
        (-15230, -15464),
        (-25503, -26395),
        (18233, 8266),
        (-5429, -20456),
        (6919, 2433),
        (-20044, -12551),
        (-7132, -2170),
        (5908, 22883),
        (4913, -23417),
        (31224, 19346),
        (-2352, -14448),
        (6048, -18141),
        (-8634, -30836),
        (-29279, -23535),
        (29983, 26183),
        (16643, -5191),
        (-16438, -40081),
    ];

    // stored side_right subframe pairs (subframe0, subframe1)
    const SIDE_RIGHT_STORED: [(i32, i32); 55] = [
        (0, 0),
        (1, 0),
        (-1, 1),
        (-1, 0),
        (1, -1),
        (0, 1),
        (0, -1),
        (2, -1),
        (0, 32767),
        (0, -32768),
        (65535, -32768),
        (-65535, 32767),
        (32766, 1),
        (-32766, -2),
        (65533, -32767),
        (-65533, 32766),
        (9, -4),
        (-9, 4),
        (-2, 4),
        (2, -4),
        (18867, -2222),
        (-19134, 6789),
        (200, -100),
        (-30636, 25311),
        (-12313, 13336),
        (-35318, 22638),
        (13777, 9761),
        (-24048, 6713),
        (9898, -19161),
        (-14019, 14655),
        (-7776, 22780),
        (-22319, 10799),
        (-5443, -11881),
        (-17051, 25486),
        (-16915, 28339),
        (23032, -16172),
        (-1540, -10130),
        (-10524, 29326),
        (-15464, 234),
        (-26395, 892),
        (8266, 9967),
        (-20456, 15027),
        (2433, 4486),
        (-12551, -7493),
        (-2170, -4962),
        (22883, -16975),
        (-23417, 28330),
        (19346, 11878),
        (-14448, 12096),
        (-18141, 24189),
        (-30836, 22202),
        (-23535, -5744),
        (26183, 3800),
        (-5191, 21834),
        (-40081, 23643),
    ];

    // stored mid_side subframe pairs (subframe0, subframe1)
    const MID_SIDE_STORED: [(i32, i32); 55] = [
        (0, 0),
        (0, 1),
        (0, -1),
        (-1, -1),
        (-1, 1),
        (1, 0),
        (-1, 0),
        (0, 2),
        (32767, 0),
        (-32768, 0),
        (-1, 65535),
        (-1, -65535),
        (16384, 32766),
        (-16385, -32766),
        (-1, 65533),
        (-1, -65533),
        (0, 9),
        (-1, -9),
        (3, -2),
        (-3, 2),
        (7211, 18867),
        (-2778, -19134),
        (0, 200),
        (9993, -30636),
        (7179, -12313),
        (4979, -35318),
        (16649, 13777),
        (-5311, -24048),
        (-14212, 9898),
        (7645, -14019),
        (18892, -7776),
        (-361, -22319),
        (-14603, -5443),
        (16960, -17051),
        (19881, -16915),
        (-4656, 23032),
        (-10900, -1540),
        (24064, -10524),
        (-7498, -15464),
        (-12306, -26395),
        (14100, 8266),
        (4799, -20456),
        (5702, 2433),
        (-13769, -12551),
        (-6047, -2170),
        (-5534, 22883),
        (16621, -23417),
        (21551, 19346),
        (4872, -14448),
        (15118, -18141),
        (6784, -30836),
        (-17512, -23535),
        (16891, 26183),
        (19238, -5191),
        (3602, -40081),
    ];

    // ---- recovery on generated vectors -------------------------------------

    fn recover(mode: ChannelConfig, stored: &[(i32, i32)], expect: &[(i32, i32)]) {
        for (i, (&(p, s), &(l, r))) in stored.iter().zip(expect.iter()).enumerate() {
            let mut left = [p];
            let mut right = [s];
            decorrelate(mode, 1, &mut left, &mut right)
                .unwrap_or_else(|e| panic!("{mode:?} pair {i}: {e:?}"));
            assert_eq!((left[0], right[0]), (l, r), "{mode:?} pair {i}");
        }
    }

    #[test]
    fn left_side_recovers_generated_pairs() {
        recover(ChannelConfig::LeftSide, &LEFT_SIDE_STORED, &PAIRS);
    }

    #[test]
    fn side_right_recovers_generated_pairs() {
        recover(ChannelConfig::RightSide, &SIDE_RIGHT_STORED, &PAIRS);
    }

    #[test]
    fn mid_side_recovers_generated_pairs() {
        recover(ChannelConfig::MidSide, &MID_SIDE_STORED, &PAIRS);
        // The extremes that make the LSB load-bearing: (32767, -32768) stores
        // mid -1 (the >>1 of -1) + side 65535; without `| (side & 1)` the
        // recovery would land on (32766, -32769) — both outputs off by one. Pinned alone
        // so a regression names itself.
        let mut left = [-1i32];
        let mut right = [65535i32];
        decorrelate(ChannelConfig::MidSide, 1, &mut left, &mut right).unwrap();
        assert_eq!((left[0], right[0]), (32767, -32768));
    }

    #[test]
    fn exhaustive_small_domain_recovers_every_pair() {
        // Full brute force over [-16,16]^2 in all three modes: the *stored*
        // pairs are computed here with the encoder-side formulas (an
        // independent spec reading, not the impl's decoder-side one), so this
        // is the generation witness run live, not a transcription.
        for l in -16..=16i32 {
            for r in -16..=16i32 {
                let side = l - r;
                let cases: [(ChannelConfig, [i32; 1], [i32; 1]); 3] = [
                    (ChannelConfig::LeftSide, [l], [side]),
                    (ChannelConfig::RightSide, [side], [r]),
                    (ChannelConfig::MidSide, [(l + r) >> 1], [side]),
                ];
                for (mode, p, s) in cases {
                    let (mut left, mut right) = (p, s);
                    decorrelate(mode, 1, &mut left, &mut right).unwrap();
                    assert_eq!((left[0], right[0]), (l, r), "{mode:?} for ({l}, {r})");
                }
            }
        }
    }

    #[test]
    fn independent_is_the_identity_and_mono_tolerates_empty_right() {
        // Independent stereo: buffers untouched.
        let mut left = [7i32, -9];
        let mut right = [11i32, -13];
        decorrelate(
            ChannelConfig::Independent { channels: 2 },
            2,
            &mut left,
            &mut right,
        )
        .unwrap();
        assert_eq!(left, [7, -9]);
        assert_eq!(right, [11, -13]);

        // Mono: zero-length right is the documented contract; left untouched.
        let mut left = [5i32, 6];
        decorrelate(
            ChannelConfig::Independent { channels: 1 },
            2,
            &mut left,
            &mut [],
        )
        .unwrap();
        assert_eq!(left, [5, 6]);
    }

    #[test]
    fn processes_exactly_blocksize_samples() {
        // Reused buffers carry a stale tail (the final frame of a track is
        // legitimately short); decorrelate must touch only [..blocksize].
        let mut left = [100i32, 200, 999, 998];
        let mut right = [30i32, 40, 997, 996];
        decorrelate(ChannelConfig::LeftSide, 2, &mut left, &mut right).unwrap();
        assert_eq!(left, [100, 200, 999, 998]);
        assert_eq!(right, [70, 160, 997, 996]);
    }

    #[test]
    fn rejections_write_nothing() {
        // Short right (the secondary is missing) — every decorrelated mode.
        for mode in [
            ChannelConfig::LeftSide,
            ChannelConfig::RightSide,
            ChannelConfig::MidSide,
            ChannelConfig::Independent { channels: 2 },
        ] {
            let mut left = [1i32, 2];
            let mut right = [3i32];
            assert_eq!(
                decorrelate(mode, 2, &mut left, &mut right),
                Err(Error::InvalidField)
            );
            assert_eq!(left, [1, 2], "{mode:?}: left untouched");
            assert_eq!(right, [3], "{mode:?}: right untouched");
        }

        // Short left — every mode, including mono (caller bookkeeping bug).
        let mut left = [1i32];
        let mut right = [2i32, 3];
        for mode in [
            ChannelConfig::Independent { channels: 1 },
            ChannelConfig::Independent { channels: 2 },
            ChannelConfig::LeftSide,
            ChannelConfig::RightSide,
            ChannelConfig::MidSide,
        ] {
            assert_eq!(
                decorrelate(mode, 2, &mut left, &mut right),
                Err(Error::InvalidField)
            );
            assert_eq!(left, [1], "{mode:?}: left untouched");
        }

        // blocksize 0 over empty slices is the empty transform, not an error.
        assert_eq!(
            decorrelate(ChannelConfig::MidSide, 0, &mut [], &mut []),
            Ok(())
        );
    }
}
