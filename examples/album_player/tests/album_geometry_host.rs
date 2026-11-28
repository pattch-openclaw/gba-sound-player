//! W3.1's host gate: the geometry has two declared arms, and the selected one is
//! the only clock shared code may use.
//!
//! This test is compiled under `--features geometry-uli2`, which selects the ULI2
//! arm *for this host binary only*. The default build remains FLAC, and there is
//! no `backend-uli2` module yet — W3.1 deliberately stops at the geometry/rate
//! seam. The FLAC-asset suites (`album_pack_host`, `album_seam_host`) are gated
//! off under this feature because they read the FLAC-baked `ALBM1`; the ULI2 bake
//! is W3.2.
#![cfg(feature = "geometry-uli2")]

use album_player::{geometry, meta, nodiv};

#[test]
fn the_active_arm_is_uli2_and_its_derivation_is_pinned() {
    assert_eq!(geometry::ACTIVE, geometry::ULI2);
    assert_eq!(geometry::RATE_HZ, 32_768);
    assert_eq!(geometry::RATE_LOG2, 15);
    assert_eq!(geometry::CHUNK_SAMPLES, 512);
    assert_eq!(geometry::FRAME_SAMPLES, 512);
    assert_eq!(geometry::TICKS_PER_SAMPLE, 512);
    assert_eq!(geometry::PERIOD_CYCLES, 262_144);
    assert_eq!(geometry::DMA_WORDS, 128);
    assert_eq!(geometry::RING_CHUNKS, 16);
    assert_eq!(geometry::RING_SAMPLES, 8_192);
    assert_eq!(geometry::PREFILL_CHUNKS, 8);
    assert_eq!(geometry::PREFILL_SAMPLES, 4_096);
    assert_eq!(geometry::PREFILL_FRAMES, 8);
}

#[test]
fn the_uli2_arm_holds_the_same_invariants_as_flac() {
    for arm in [geometry::FLAC, geometry::ULI2] {
        assert!(arm.ring_samples > arm.prefill_samples + arm.frame_samples);
        assert!(arm.ring_samples >= arm.frame_samples + arm.chunk_samples);
        assert_eq!(arm.chunk_samples % 4, 0);
        assert_eq!(arm.dma_words * 4, arm.chunk_samples);
        assert_eq!(arm.period_cycles, arm.chunk_samples * arm.ticks_per_sample);
        assert_eq!(
            (arm.prefill_samples + arm.frame_samples - 1) / arm.frame_samples,
            if arm == geometry::FLAC { 5 } else { 8 }
        );
    }
}

#[test]
fn flac_is_still_the_shipped_arm_and_its_figures_did_not_move() {
    assert_eq!(geometry::FLAC.rate_hz, 16_384);
    assert_eq!(geometry::FLAC.ticks_per_sample, 1_024);
    assert_eq!(geometry::FLAC.period_cycles, 573_440);
    assert_eq!(geometry::FLAC.ring_chunks, 36);
    assert_eq!(geometry::FLAC.prefill_chunks, 16);
}

#[test]
fn the_manifest_clock_is_the_selected_geometry_clock() {
    assert_eq!(meta::PRODUCT_RATE_HZ, geometry::RATE_HZ);
    assert_eq!(meta::PRODUCT_RATE_HZ, 32_768);
    assert_eq!(meta::PRODUCT_RATE_HZ, 1 << geometry::RATE_LOG2);
}

#[test]
fn the_rate_contract_is_identity_or_refusal_for_the_selected_arm() {
    assert_eq!(geometry::playback_rate_for_stream(32_768), Some(32_768));
    assert_eq!(geometry::playback_rate_for_stream(16_384), None);
    assert_eq!(geometry::playback_rate_for_stream(0), None);
}

#[test]
fn nodiv_carries_both_clock_shifts_and_the_uli2_one_is_15() {
    assert_eq!(nodiv::RATE_LOG2_16K, 14);
    assert_eq!(nodiv::RATE_LOG2_32K, 15);
    assert_eq!(nodiv::seconds_at(32_768, nodiv::RATE_LOG2_32K), 1);
    assert_eq!(nodiv::label_at(32_768, nodiv::RATE_LOG2_32K), (0, 1));
}
