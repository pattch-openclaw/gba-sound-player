//! 6a-7 — the width leg's host witness.
//!
//! The ROM times `ctrl` (the core's i32 default), `wide` (the vendored pre-6a-6
//! i64 path in `src/wide.rs`) and `asm` (the landed ARM kernel over the i32
//! default) in one calibration. This suite is the correctness gate those
//! numbers inherit: the wide leg must decode the committed bytes to the
//! committed `flac -d` PCM pin **and** agree with the product path frame
//! window by frame window. A divergence would mean the ROM's `ctrl − wide`
//! pair measures two different decodes, not two widths of one.
//!
//! `required-features = ["item6a7-wide"]`: a default build has no `wide`
//! module, and the suite's absence is not a green.

use flac_lite::subframe::PredictorState;
use flac_spike::assets::CLIPS;
use flac_spike::checksum::{Fnv1a64, fold_i16le_stereo};
use flac_spike::driver;
use flac_spike::wide;

#[test]
fn wide_leg_matches_the_reference_pcm_and_the_control() {
    for clip in CLIPS {
        let max = usize::from(clip.max_blocksize);
        let mut wide_l = vec![0i32; max];
        let mut wide_r = vec![0i32; max];
        let mut wide_state = [PredictorState::new(); 2];
        let mut ctrl_l = vec![0i32; max];
        let mut ctrl_r = vec![0i32; max];
        let mut ctrl_state = [PredictorState::new(); 2];

        let mut hash = Fnv1a64::new();
        let mut samples = 0usize;
        for (index, _frame) in clip.frames.iter().enumerate() {
            let wide_written =
                wide::decode_one_wide(clip, index, &mut wide_l, &mut wide_r, &mut wide_state)
                    .unwrap_or_else(|e| {
                        panic!("{}: wide rejected frame {index}: {e:?}", clip.name)
                    });
            let ctrl_written =
                driver::decode_one(clip, index, &mut ctrl_l, &mut ctrl_r, &mut ctrl_state)
                    .unwrap_or_else(|e| {
                        panic!("{}: ctrl rejected frame {index}: {e:?}", clip.name)
                    });

            assert_eq!(
                wide_written, ctrl_written,
                "{}: frame {index} wrote a different blocksize",
                clip.name
            );
            assert_eq!(
                wide_l[..wide_written],
                ctrl_l[..wide_written],
                "{}: frame {index} left channel differs from the i32 control",
                clip.name
            );
            assert_eq!(
                wide_r[..wide_written],
                ctrl_r[..wide_written],
                "{}: frame {index} right channel differs from the i32 control",
                clip.name
            );

            fold_i16le_stereo(&mut hash, &wide_l[..wide_written], &wide_r[..wide_written])
                .expect("decoded sample outside the 16-bit profile");
            samples += wide_written;
        }

        assert_eq!(
            samples,
            usize::try_from(clip.total_samples).expect("total samples fit usize"),
            "{}: wide leg decoded the wrong sample count",
            clip.name
        );
        assert_eq!(
            hash.finish(),
            clip.fnv_pcm,
            "{}: wide PCM != the committed reference pin",
            clip.name
        );
    }
}
