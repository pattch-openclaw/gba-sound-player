//! The output stage: FIFO bytes out of planar int8, and the speaker fold-down.
//!
//! Two decisions live here, and the plan makes both of them PR D's:
//!
//! 1. **The decoder always produces full stereo.** A speaker build does not
//!    shorten synthesis to one channel — that is not a fold-down. It decodes both
//!    channels and folds here, after the decoder.
//! 2. **Which fold the speaker gets is a measured choice**, so both are built and
//!    both are exercised: `Average` is `(L+R)/2` (the mono mix the C project's
//!    driver uses), `LeftOnly` is single-channel. They are *not* equivalent:
//!    `LeftOnly` is not a fold-down — hard-panned content vanishes, and on an
//!    M/S-coded frame `L = M + S` reproduces mid+side rather than mid.
//!
//! The byte convention is the GBA's: the 8-bit FIFO carries a **signed** sample
//! in two's complement, so ULI2's fused int8 output is written bit-for-bit
//! (`i8 as u8`) and the hardware's `SOUNDBIAS` supplies the mid-level. That is
//! why ULI2's "convert" is a byte copy rather than the FLAC path's PCM16 `>> 8`.

/// Which output regime the player is in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Profile {
    /// Headphones: stereo as decoded, one FIFO per channel.
    Headphone,
    /// The built-in speaker: decode stereo, then fold to mono in this stage.
    Speaker,
}

/// How a [`Profile::Speaker`] build folds stereo to mono.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FoldHow {
    /// `(L + R) >> 1`, arithmetic shift, i.e. floor division by two: the C
    /// project's driver mix, kept identical so the comparison is like-for-like.
    Average,
    /// `L` only. Cheaper, and *not* a fold-down (see the module docs).
    LeftOnly,
}

/// Fold one frame and write FIFO bytes for both channels.
///
/// `out_l`/`out_r` are written in full; a speaker build writes the same mono
/// signal to both, because the hardware still has two FIFOs and the fold has to
/// happen somewhere — this is that somewhere.
///
/// # Panics
/// Panics if `out_l`, `out_r` or `r` is shorter than `l`, or if `r` has a
/// different length from `l` (a decoder invariant, checked once per frame rather
/// than per sample).
pub fn fold_into(
    l: &[i8],
    r: &[i8],
    out_l: &mut [u8],
    out_r: &mut [u8],
    profile: Profile,
    how: FoldHow,
) {
    assert_eq!(l.len(), r.len(), "channel lengths differ");
    assert!(
        out_l.len() >= l.len() && out_r.len() >= l.len(),
        "output too small"
    );
    match profile {
        Profile::Headphone => {
            for (i, (&left, &right)) in l.iter().zip(r.iter()).enumerate() {
                out_l[i] = left as u8;
                out_r[i] = right as u8;
            }
        }
        Profile::Speaker => {
            for (i, (&left, &right)) in l.iter().zip(r.iter()).enumerate() {
                let mono = match how {
                    FoldHow::Average => ((i16::from(left) + i16::from(right)) >> 1) as i8,
                    FoldHow::LeftOnly => left,
                };
                out_l[i] = mono as u8;
                out_r[i] = mono as u8;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{FoldHow, Profile, fold_into};

    /// Headphones are the identity on both channels, byte for byte: ULI2's int8
    /// *is* the FIFO byte, which is the property the gate total depends on.
    #[test]
    fn headphone_output_is_the_decoded_bytes_verbatim() {
        let l = [0i8, 1, -1, 127, -128];
        let r = [5i8, -5, 100, -100, 0];
        let (mut ol, mut or) = ([0u8; 5], [0u8; 5]);
        fold_into(
            &l,
            &r,
            &mut ol,
            &mut or,
            Profile::Headphone,
            FoldHow::Average,
        );
        assert_eq!(ol, [0u8, 1, 0xFF, 0x7F, 0x80]);
        assert_eq!(or, [5u8, 0xFB, 100, 0x9C, 0]);
    }

    /// The two speaker folds are not equivalent, and the difference is the
    /// plan's whole reason for measuring both: on panned content `LeftOnly`
    /// deletes the right channel while `Average` keeps both at half level.
    #[test]
    fn the_two_speaker_folds_disagree_on_panned_content() {
        let l = [100i8, -100, 127];
        let r = [-100i8, 100, -128];
        let (mut al, mut ar) = ([0u8; 3], [0u8; 3]);
        let (mut bl, mut br) = ([0u8; 3], [0u8; 3]);
        fold_into(&l, &r, &mut al, &mut ar, Profile::Speaker, FoldHow::Average);
        fold_into(
            &l,
            &r,
            &mut bl,
            &mut br,
            Profile::Speaker,
            FoldHow::LeftOnly,
        );

        // A mono signal reaches both FIFOs either way.
        assert_eq!(al, ar);
        assert_eq!(bl, br);
        // The folds differ, which is the claim.
        assert_ne!(al, bl);
        // Average is symmetric in the channels; LeftOnly is not.
        let (mut cl, mut cr) = ([0u8; 3], [0u8; 3]);
        fold_into(&r, &l, &mut cl, &mut cr, Profile::Speaker, FoldHow::Average);
        assert_eq!(al, cl);
        // (100 + -100) >> 1 = 0; (127 + -128) >> 1 = -1 >> 1 = -1 (floor), i.e.
        // 0xFF, against L's 127 — floor division is the documented rounding.
        assert_eq!(al, [0u8, 0, 0xFF]);
        assert_eq!(bl, [100u8, 0x9C, 127]);
    }

    /// Averaging identical channels is the identity, so a mono-encoded frame is
    /// not attenuated by the fold. Floor division is the documented rounding.
    #[test]
    fn averaging_identical_channels_is_the_identity() {
        let l = [1i8, -1, 127, -128, 64];
        let (mut al, mut ar) = ([0u8; 5], [0u8; 5]);
        let (mut hl, mut hr) = ([0u8; 5], [0u8; 5]);
        fold_into(&l, &l, &mut al, &mut ar, Profile::Speaker, FoldHow::Average);
        fold_into(
            &l,
            &l,
            &mut hl,
            &mut hr,
            Profile::Headphone,
            FoldHow::Average,
        );
        assert_eq!(al, hl);
        assert_eq!(al, [1u8, 0xFF, 127, 0x80, 64]);
    }
}
