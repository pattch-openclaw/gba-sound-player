//! 6a-5 arms — the host half of the realisation reading's correctness gate.
//!
//! `required-features = ["item6a5-arms"]` in the manifest: a default `make
//! spike-test` does not build this file, which is what keeps "the shipped
//! instrument image is unchanged by this rung" a checked statement rather than a
//! claim.
//!
//! # What this witnesses, and what only the ROM can
//!
//! Three kernels are driven through the **product** call path — the same
//! `driver::decode_one_with` → `frame::decode_frame_with` shape the ROM times —
//! and each must decode the committed clip to the committed `fnv_pcm` pin
//! (the generator's `flac -d` reference PCM, hashed):
//!
//! * `ctrl` — the core's `TableCountKernel`, the shipped default;
//! * `rust` — the overlay's `TwinKernel` (6a-3), forwarding to the core's
//!   `HybridRiceKernel` — the mechanism item 5 PR 4 measured at +42.6 c/sample;
//! * `asm` — the overlay's `ArmRiceKernel` (6a-4).
//!
//! On this host the asm arm's FFI leg is `rice_arm::model_partition`, a
//! label-for-label Rust model of the assembly — so this suite proves the
//! *kernel contract* and the *wiring*, never the instruction stream. The
//! assembled body is witnessed by `make overlay-kernel-test` on mGBA; this suite
//! proves the same arms decode the real clips identically through the seam.
//!
//! The per-frame differential is the second layer: every mechanism must produce
//! byte-identical samples frame window by frame window. That is the property the
//! ROM's timing pass depends on (its equality witness voids a frame whose
//! samples diverge), so a host run that finds a divergence means the ROM's
//! numbers would be void rather than interesting — and it says so before anyone
//! runs an emulator.

use flac_lite::residual::{RiceKernel, TableCountKernel};
use flac_lite_gba::rice_arm::ArmRiceKernel;
use flac_lite_gba::rice_twin::TwinKernel;
use flac_spike::assets::CLIPS;
use flac_spike::checksum::{Fnv1a64, fold_i16le_stereo};
use flac_spike::driver;

/// Walk one clip through `K` and return (frames, samples, PCM FNV-1a-64).
fn walk<K: RiceKernel>(clip: &flac_spike::assets::SpikeClip) -> (usize, usize, u64) {
    let max = usize::from(clip.max_blocksize);
    let mut left = vec![0i32; max];
    let mut right = vec![0i32; max];
    let mut state = [flac_lite::subframe::PredictorState::new(); 2];
    let mut hash = Fnv1a64::new();
    let stats = driver::decode_clip_with::<K, _>(
        clip,
        &mut left,
        &mut right,
        &mut state,
        |_frame, l, r| {
            fold_i16le_stereo(&mut hash, l, r)
                .expect("a decoded sample outside the 16-bit profile");
        },
    )
    .unwrap_or_else(|e| panic!("{}: an arm rejected the committed clip: {e:?}", clip.name));
    (stats.frames, stats.samples, hash.finish())
}

/// Every arm reaches the pinned reference PCM. This is the correctness-before-
/// speed gate the ROM's timing pass inherits: an arm whose walk does not land on
/// the pin is never timed.
#[test]
fn every_arm_decodes_the_committed_clips_to_the_pinned_pcm() {
    for clip in CLIPS {
        let ctrl = walk::<TableCountKernel>(clip);
        let rust = walk::<TwinKernel>(clip);
        let asm = walk::<ArmRiceKernel>(clip);
        for (name, (frames, samples, pcm)) in [("ctrl", ctrl), ("rust", rust), ("asm", asm)] {
            assert_eq!(
                frames,
                clip.frames.len(),
                "{}: {name} decoded {frames} frames of {}",
                clip.name,
                clip.frames.len()
            );
            assert_eq!(
                samples,
                usize::try_from(clip.total_samples).expect("total samples fit usize"),
                "{}: {name} decoded {samples} samples",
                clip.name
            );
            assert_eq!(
                pcm, clip.fnv_pcm,
                "{}: {name}'s PCM hash is not the committed pin — the arm is not \
                 decoding the reference samples",
                clip.name
            );
        }
        eprintln!(
            "{}: ctrl/rust/asm all reach the pin 0x{:016X} ({} frames, {} samples)",
            clip.name, clip.fnv_pcm, ctrl.0, ctrl.1
        );
    }
}

/// Frame-window equality: the arms' samples must be bit-identical to `ctrl`'s on
/// every frame, not merely equal as a whole-clip hash. The ROM's timing pass
/// voids an arm on the first divergent frame; this is the same rule, run where
/// a divergence can be inspected.
#[test]
fn every_arm_agrees_with_the_control_frame_window_by_frame_window() {
    for clip in CLIPS {
        let max = usize::from(clip.max_blocksize);
        let mut ctrl_l = vec![0i32; max];
        let mut ctrl_r = vec![0i32; max];
        let mut state_c = [flac_lite::subframe::PredictorState::new(); 2];
        let mut rust_l = vec![0i32; max];
        let mut rust_r = vec![0i32; max];
        let mut state_r = [flac_lite::subframe::PredictorState::new(); 2];
        let mut asm_l = vec![0i32; max];
        let mut asm_r = vec![0i32; max];
        let mut state_a = [flac_lite::subframe::PredictorState::new(); 2];

        let mut windows = 0usize;
        for frame_index in 0..clip.frames.len() {
            let w = driver::decode_one(clip, frame_index, &mut ctrl_l, &mut ctrl_r, &mut state_c)
                .expect("ctrl decoded the committed clip");
            let wr = driver::decode_one_with::<TwinKernel>(
                clip,
                frame_index,
                &mut rust_l,
                &mut rust_r,
                &mut state_r,
            )
            .expect("rust decoded the committed clip");
            let wa = driver::decode_one_with::<ArmRiceKernel>(
                clip,
                frame_index,
                &mut asm_l,
                &mut asm_r,
                &mut state_a,
            )
            .expect("asm decoded the committed clip");
            assert_eq!(
                (wr, wa),
                (w, w),
                "{} frame {frame_index}: arms wrote different lengths (ctrl {w}, rust {wr}, asm {wa})",
                clip.name
            );
            assert_eq!(
                rust_l[..w],
                ctrl_l[..w],
                "{} frame {frame_index}: rust left channel differs from ctrl",
                clip.name
            );
            assert_eq!(
                rust_r[..w],
                ctrl_r[..w],
                "{} frame {frame_index}: rust right channel differs from ctrl",
                clip.name
            );
            assert_eq!(
                asm_l[..w],
                ctrl_l[..w],
                "{} frame {frame_index}: asm left channel differs from ctrl",
                clip.name
            );
            assert_eq!(
                asm_r[..w],
                ctrl_r[..w],
                "{} frame {frame_index}: asm right channel differs from ctrl",
                clip.name
            );
            windows += 1;
        }
        assert!(windows > 0, "{}: no frame windows compared", clip.name);
        eprintln!(
            "{}: {windows} frame windows identical across all three arms",
            clip.name
        );
    }
}

/// The arm features are the *only* difference between a control image and an
/// arm image — pinned as a manifest fact, because every number this rung records
/// depends on it. Reading the manifest rather than a comment: a feature that
/// silently dragged a second mechanism in would make the "product shape" image
/// (asm only) a lie, and 6a-3 measured what a second mechanism's mere presence
/// costs (~168–199 K cycles/frame).
#[test]
fn the_arm_features_select_exactly_the_mechanisms_they_name() {
    let manifest = include_str!("../Cargo.toml");
    let line = manifest
        .lines()
        .find(|l| l.trim_start().starts_with("flac-lite-gba = "))
        .expect("the overlay dependency line");
    assert!(
        !line.contains("rice-twin") && !line.contains("rice-arm"),
        "the overlay dependency carries a kernel feature unconditionally: {line} — \
         `item6a5-asm` must be able to link the overlay WITHOUT the Rust mechanism, \
         or the product-shape image is not the product shape"
    );
    let feature = |name: &str| -> &str {
        manifest
            .lines()
            .map(str::trim)
            .find(|l| l.starts_with(name) && l.contains('='))
            .unwrap_or_else(|| panic!("feature {name} is not declared"))
    };
    let rust = feature("item6a5-rust");
    assert!(
        rust.contains("flac-lite-gba/rice-twin") && !rust.contains("flac-lite-gba/rice-arm"),
        "the Rust arm must name the overlay's rice-twin and NOT its rice-arm: {rust}"
    );
    let asm = feature("item6a5-asm");
    assert!(
        asm.contains("flac-lite-gba/rice-arm")
            && !asm.contains("rice-twin")
            && !asm.contains("rice-hybrid"),
        "the asm arm must name the overlay's rice-arm and nothing else: {asm} — an \
         image that also carries the Rust mechanism is not the product shape, and \
         6a-3 measured what a second mechanism's presence costs"
    );
    assert!(
        feature("item6a5-arms").contains("item6a5-rust")
            && feature("item6a5-arms").contains("item6a5-asm"),
        "the three-way feature must be exactly the two arms"
    );

    // [6a-5b] the vendored leg, same discipline applied one rung later. It may
    // name only the module it is (no core/overlay feature — its whole value is
    // that it shares no code with the landed path), and it must imply the asm
    // arm: a differential leg with no landed leg in the link measures nothing,
    // and "the pair is in one image" belongs in the manifest, not in a Makefile
    // convention. Conversely no pre-6a-5b feature may pull it in: every number
    // recorded before this rung was taken in an image with no vendored decoder.
    let vend = feature("item6a5b-vendored");
    assert!(
        vend.contains("item6a5-asm")
            && !vend.contains("item6a5-rust")
            && !vend.contains("flac-lite")
            && !vend.contains("rice-twin")
            && !vend.contains("rice-arm")
            && !vend.contains("rice-hybrid"),
        "the 6a-5b leg must add only its own vendored module and imply the asm arm: {vend}"
    );
    for name in [
        "item6a5-rust",
        "item6a5-asm",
        "item6a5-arms",
        "item6a3-twin",
        "item6a3-hybrid",
    ] {
        assert!(
            !feature(name).contains("item6a5b-vendored"),
            "{name} must not pull the 6a-5b leg in: {vend} — an image that carries a \
             second decoder is not the image the earlier records name"
        );
    }
}
