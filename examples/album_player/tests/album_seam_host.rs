//! PR-3a steps (ii) and (iii-A)'s host gate: the seam, its derived geometry, and the backend arms —
//! on the host.
//!
//! Five claims, each with its own tests so a failure names one:
//!
//! 1. **The derived geometry reproduces the shipped FLAC figures exactly.** Every equality cites
//!    `examples/flac_integration/src/geometry.rs:LINE`, so a transcription rot in this app's
//!    derivation — or an unnoticed change to the shipped module — is a red test rather than a
//!    silent geometry change. The *second* arm (32,768 Hz) is checked for form and invariants only,
//!    never for numbers nobody has measured: that is the point of a derivation.
//! 2. **The manifest and the generated pins agree, track for track**, and an audited mutation of
//!    the manifest is still refused (`assets::AUDITED_MUTATIONS`).
//! 3. **Both backend arms satisfy `PcmSource`** — positions in album samples, `pull_frame` bounded
//!    by one frame, `start_track` repositioning, `Ok(0)` at end of stream — and the FLAC arm decodes
//!    the shipped fixture's track to its committed `fnv_pcm` pin, so "the decoder still produces
//!    these samples" is a checked fact and not a shape assertion.
//! 4. **The seam's sample scale survives the int16 boundary** — the ROM's whole path for track 1
//!    (`pull_frame` → i32 → `convert_frame` → `fold_bytes`) reproduces the bake's `fnv_fifo_l` /
//!    `fnv_fifo_r`, so the bytes the ROM would push are the bytes the pins describe.
//! 5. **The verdict words are renderable** — inside the tracked font subset's glyph range and inside
//!    one 30-column row, so a rename to a word the shipped 96-glyph font cannot draw fails here
//!    rather than presenting an empty verdict screen on a cart.
//!
//! Nothing here is a hardware claim: the ROM's `agb` bootstrap and its feeder IRQ are step (iii)'s
//! ROM half, which this file does not execute.

use album_player::assets::{self, TrackPin};
use album_player::backend::stub::{StubSource, StubTrack};
use album_player::geometry::{self, ChainGeometry};
use album_player::meta::{self, Album};
use album_player::nodiv;
use album_player::pcm::{Capability, FrameCostModel, PcmSource, SourceError};

/// The packed manifest, written by `make album-bake`. `include_bytes!` makes a missing blob a
/// compile error naming the path; there is no runtime default and no skip.
const BLOB: &[u8] = assets::ALBUM1;

fn parsed() -> Album<'static> {
    match Album::parse(BLOB) {
        Ok(album) => album,
        Err(refusal) => panic!("the baked ALBM1 was refused: {refusal}"),
    }
}

fn word(blob: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        blob[offset],
        blob[offset + 1],
        blob[offset + 2],
        blob[offset + 3],
    ])
}

// -------------------------------------------------------------------------------------------------
// 1. The derived geometry against the shipped figures
// -------------------------------------------------------------------------------------------------

#[test]
fn flac_arm_reproduces_the_shipped_clock_and_period_figures() {
    // examples/flac_integration/src/geometry.rs:81 — MIXER_TICK_HZ / 16_384.
    assert_eq!(geometry::TICKS_PER_SAMPLE, 1_024);
    // …:120 — the 560-sample hardware buffer every measured row is stated on.
    assert_eq!(geometry::CHUNK_SAMPLES, 560);
    // …:95 — CHUNK_SAMPLES × TICKS_PER_SAMPLE = 34.18 ms of cycles.
    assert_eq!(geometry::PERIOD_CYCLES, 573_440);
    assert_eq!(
        geometry::PERIOD_CYCLES,
        geometry::CHUNK_SAMPLES * geometry::TICKS_PER_SAMPLE
    );
    // …:226 — the count written on every DMA re-arm.
    assert_eq!(geometry::DMA_WORDS, 140);
    // …:59 — the hardware clock itself: 2^24.
    assert_eq!(geometry::MIXER_TICK_HZ, 1 << 24);
}

#[test]
fn flac_arm_reproduces_the_shipped_ring_and_prefill_depths() {
    // …:172 — 36 chunks (not a literal in this app: ceil(1,230 ms × 16,384 / 560)).
    assert_eq!(geometry::RING_CHUNKS, 36);
    // …:175 and …:315.
    assert_eq!(geometry::RING_SAMPLES, 20_160);
    assert_eq!(
        geometry::RING_SAMPLES,
        geometry::RING_CHUNKS * geometry::CHUNK_SAMPLES
    );
    // …:206 — 16 chunks = 546.875 ms of lead before the feeder is armed.
    assert_eq!(geometry::PREFILL_CHUNKS, 16);
    // …:209.
    assert_eq!(geometry::PREFILL_SAMPLES, 8_960);
    assert_eq!(
        geometry::PREFILL_SAMPLES,
        geometry::PREFILL_CHUNKS * geometry::CHUNK_SAMPLES
    );
}

#[test]
fn the_rearm_grace_is_the_fifo_half_full_point() {
    // …:219 — a rate-independent hardware fact: the 32-byte FIFO requests a refill at 16 samples.
    assert_eq!(geometry::REARM_GRACE_SAMPLES, 16);
    assert_eq!(geometry::REARM_GRACE_SAMPLES, 32 / 2);
}

#[test]
fn a_chunk_is_a_whole_number_of_fifo_words() {
    // The 4-word FIFO request is why a chunk must divide by 4, and the DMA word count is that
    // division — the same rule as examples/flac_integration/src/geometry.rs's chunk asserts.
    assert_eq!(geometry::CHUNK_SAMPLES % 4, 0);
    assert_eq!(geometry::DMA_WORDS * 4, geometry::CHUNK_SAMPLES);
}

#[test]
fn the_ring_holds_the_prefill_plus_a_frame() {
    // …:320-321. Two facts, and the second is the flow-control pair: free >= frame, ready >= chunk.
    assert!(geometry::RING_SAMPLES > geometry::PREFILL_SAMPLES + geometry::FRAME_SAMPLES);
    assert!(geometry::RING_SAMPLES >= geometry::FRAME_SAMPLES + geometry::CHUNK_SAMPLES);
    // §4.2b's "PREFILL + one frame <= RING" read the other way: the prefill must leave room for a
    // whole frame before the producer is allowed to start one.
    assert!(geometry::RING_SAMPLES - geometry::PREFILL_SAMPLES >= geometry::FRAME_SAMPLES);
}

#[test]
fn the_product_clock_is_the_nodiv_shift() {
    // D4's tie, at the app's link site: §6's arithmetic and this geometry are one clock.
    assert_eq!(meta::PRODUCT_RATE_HZ, 1 << nodiv::RATE_LOG2);
    assert_eq!(geometry::RATE_HZ, meta::PRODUCT_RATE_HZ);
    assert_eq!(geometry::FLAC.rate_hz, geometry::RATE_HZ);
    // Q14 / item 7-0: identity or refusal, and nothing else resolves.
    assert_eq!(
        geometry::playback_rate_for_stream(geometry::RATE_HZ),
        Some(geometry::RATE_HZ)
    );
    assert_eq!(geometry::playback_rate_for_stream(32_768), None);
    assert_eq!(geometry::playback_rate_for_stream(0), None);
}

#[test]
fn frame_cycles_is_samples_times_ticks() {
    assert_eq!(
        geometry::frame_cycles(geometry::CHUNK_SAMPLES),
        geometry::PERIOD_CYCLES
    );
    // 2,048 samples at 1,024 ticks each (examples/flac_integration/src/geometry.rs's
    // `frame_cycles(2048) == 2_097_152` pin), i.e. 3.657 periods — the fraction matters.
    assert_eq!(geometry::frame_cycles(2_048), 2_097_152);
    assert_eq!(
        geometry::frame_cycles(2_048),
        3 * geometry::PERIOD_CYCLES + 376_832
    );
}

/// The second arm exists to prove the *formulas*, not to invent measurements.
///
/// 32,768 Hz with a 512-sample chunk is the natural ULI2-shaped arm (exact 512 ticks/sample); the
/// invariants and the derivation rules are asserted, and **no derived value is compared with a
/// shipped figure** because nothing in this repository has measured that arm.
#[test]
fn a_second_rate_derives_the_same_formulas() {
    let arm: ChainGeometry =
        ChainGeometry::derive(32_768, 512, 2_048, geometry::LEAD_MS, geometry::PREFILL_MS);
    assert_eq!(arm.ticks_per_sample, 512);
    assert_eq!(arm.period_cycles, 512 * 512);
    assert_eq!(arm.dma_words, 512 / 4);
    assert_eq!(arm.ring_samples, arm.ring_chunks * 512);
    assert_eq!(arm.prefill_samples, arm.prefill_chunks * 512);
    // The same two invariants the FLAC arm is held to, from the same code path.
    assert!(arm.ring_samples > arm.prefill_samples + arm.frame_samples);
    assert!(arm.ring_samples >= arm.frame_samples + arm.chunk_samples);
    // The ring is a lead-time decision, so the same declared duration at twice the rate is about
    // twice the samples — the depth follows the clock, not a constant.
    assert!(arm.ring_samples >= geometry::RING_SAMPLES);
}

// -------------------------------------------------------------------------------------------------
// 2. The manifest and the generated pins
// -------------------------------------------------------------------------------------------------

#[test]
fn the_album_manifest_parses() {
    let album = parsed();
    assert_eq!(album.n_tracks(), assets::N_TRACKS);
    assert_eq!(album.n_tracks() as usize, assets::TRACKS.len());
    assert_eq!(album.rate_hz(), geometry::RATE_HZ);
    assert_eq!(album.rate_hz(), assets::RATE_HZ);
    assert_eq!(album.album_samples(), assets::TOTAL_SAMPLES);
    assert_eq!(album.album_title(), assets::ALBUM_TITLE);
    assert_eq!(album.album_artist(), assets::ALBUM_ARTIST);
    assert_eq!(album.year(), assets::YEAR);
    assert_eq!(album.backend(), meta::BACKEND_FLAC);
}

#[test]
fn every_track_pin_agrees_with_the_manifest() {
    let album = parsed();
    for (index, pin) in assets::TRACKS.iter().enumerate() {
        let track = album.track(index as u32);
        assert_eq!(track.frames, pin.frames, "track {index}: frames");
        assert_eq!(track.n_samples, pin.n_samples, "track {index}: n_samples");
        assert_eq!(
            track.start_sample, pin.start_sample,
            "track {index}: start_sample"
        );
        assert_eq!(track.recip, pin.recip, "track {index}: recip");
        assert_eq!(
            track.max_fixed_order, pin.max_fixed_order,
            "track {index}: max_fixed_order"
        );
        // The manifest's `(offset, len)` and the pin's length name the same bytes.
        assert_eq!(
            track.asset.len(),
            pin.asset_len as usize,
            "track {index}: asset_len"
        );
        // The timeline is contiguous: each track starts where the previous one ended.
        if index > 0 {
            let previous: &TrackPin = &assets::TRACKS[index - 1];
            assert_eq!(
                track.start_sample,
                previous.start_sample + previous.n_samples,
                "track {index}: the timeline is not contiguous"
            );
        }
    }
    // The same cross-check the boot path runs, through the shipping function (D-C rule 2).
    assert_eq!(
        album_player::backend::verify_pins(&album),
        Ok(()),
        "the generated pins and the manifest disagree"
    );
}

#[test]
fn an_audited_mutation_of_the_manifest_is_refused() {
    type Mutation = fn(&mut Vec<u8>);
    let mutations: [(&str, Mutation); 9] = [
        ("magic", |blob| blob[0] = b'X'),
        ("version", |blob| blob[4] = 3),
        ("n_tracks", |blob| blob[5] = 0),
        ("reserved byte 7", |blob| blob[7] = 1),
        ("rate", |blob| {
            blob[8..12].copy_from_slice(&32_768u32.to_le_bytes())
        }),
        ("bar_cells", |blob| {
            blob[12..16].copy_from_slice(&64u32.to_le_bytes())
        }),
        ("structure checksum", |blob| blob[44] ^= 0xFF),
        ("assets checksum", |blob| blob[60] ^= 0xFF),
        // The pool's trailing pad byte, after the last NUL.
        ("pool padding", |blob| {
            let pool_end = word(blob, 36) as usize;
            blob[pool_end - 1] = b'Z';
        }),
    ];
    assert_eq!(
        mutations.map(|(name, _)| name),
        assets::AUDITED_MUTATIONS,
        "the mutation list this suite applies is not the one the bake emitted"
    );
    for (name, edit) in mutations {
        let mut copy = BLOB.to_vec();
        edit(&mut copy);
        // v2 has two reader rules: the structure fold is `parse`'s, the asset fold is
        // `verify_assets`'s (the post-audio audit). Each mutation has an explicit
        // test below saying which rule refuses it.
        let refused = match Album::parse(&copy) {
            Err(_) => true,
            Ok(album) => album.verify_assets().is_err(),
        };
        assert!(
            refused,
            "the audited mutation {name:?} was accepted by meta.rs"
        );
    }
}

#[test]
fn boot_checks_the_structure_fold_and_the_audit_checks_the_asset_fold() {
    // The v2 split, one rule at a time: a structure-fold byte is a boot refusal,
    // an asset-fold byte is accepted at boot and refused by the post-audio audit.
    let mut structure = BLOB.to_vec();
    structure[44] ^= 0xFF;
    assert!(
        matches!(
            Album::parse(&structure),
            Err(meta::Refusal::ChecksumMismatch { .. })
        ),
        "a structure-fold mutation was not a boot refusal"
    );

    let mut assets = BLOB.to_vec();
    assets[60] ^= 0xFF;
    let album = Album::parse(&assets).expect("the asset fold is the audit's rule, not boot's");
    assert!(
        matches!(
            album.verify_assets(),
            Err(meta::Refusal::AssetsMismatch { .. })
        ),
        "a mutated asset fold survived the post-audio audit"
    );
}

#[test]
fn boot_pin_check_does_not_sweep_the_asset_region() {
    // The boot cross-check is the tripwire, not a fold: a byte in the middle of the
    // album's asset region must pass `verify_pins` and fail only the post-audio
    // audit. If someone makes `verify_pins` read the whole region, this test fails.
    let asset_off = word(BLOB, 36) as usize;
    let asset_len = word(BLOB, 40) as usize;
    let mut copy = BLOB.to_vec();
    copy[asset_off + asset_len / 2] ^= 0xFF;
    let album = Album::parse(&copy).expect("the structure fold is unaffected");
    assert_eq!(
        album_player::backend::verify_pins(&album),
        Ok(()),
        "the boot pin check read a byte outside every content tripwire"
    );
    assert!(
        album.verify_assets().is_err(),
        "the post-audio audit did not catch the byte the boot path is allowed to skip"
    );
}

#[test]
fn the_content_tripwire_refuses_a_stale_prefix_or_suffix_byte() {
    // `verify_pins` is the boot cross-check, and it holds the first 32 KiB and last
    // 4 KiB of every blob to the bake's pins. A byte inside either span is a named
    // `PinMismatch`; the full-fold audit is what covers the bytes in between.
    let asset_off = word(BLOB, 36) as usize;

    let mut prefix = BLOB.to_vec();
    prefix[asset_off] ^= 0xFF;
    let album = Album::parse(&prefix).expect("the structure fold is unaffected");
    assert_eq!(
        album_player::backend::verify_pins(&album),
        Err(SourceError::PinMismatch {
            track: 0,
            field: "prefix_fnv",
        })
    );

    let mut suffix = BLOB.to_vec();
    // The last byte of **track 1's** blob (track 0's suffix is a different span), so
    // the refusal names track 1 and not whichever track happens to be last.
    let pin = &assets::TRACKS[1];
    let track_end = asset_off + pin.asset_off as usize + pin.asset_len as usize;
    suffix[track_end - 1] ^= 0xFF;
    let album = Album::parse(&suffix).expect("the structure fold is unaffected");
    assert_eq!(
        album_player::backend::verify_pins(&album),
        Err(SourceError::PinMismatch {
            track: 1,
            field: "suffix_fnv",
        })
    );
}

// -------------------------------------------------------------------------------------------------
// 3. The seam, on the stub arm
// -------------------------------------------------------------------------------------------------

/// Three tracks whose boundaries are the caller's, not a container's. Track 0 is longer than one
/// frame (4,000 = 2,048 + 1,952), track 2 is shorter (1,000), so the frame-bounding rules are
/// exercised at both ends without a decoder.
static STUB_TRACKS: [StubTrack; 3] = [
    StubTrack {
        start_sample: 0,
        n_samples: 4_000,
        title: "stub one",
        artist: "nobody",
    },
    StubTrack {
        start_sample: 4_000,
        n_samples: 2_500,
        title: "stub two",
        artist: "nobody",
    },
    StubTrack {
        start_sample: 6_500,
        n_samples: 1_000,
        title: "stub three",
        artist: "",
    },
];

#[test]
fn stub_positions_are_album_samples() {
    let source = StubSource::new(&STUB_TRACKS);
    assert_eq!(source.n_tracks(), STUB_TRACKS.len());
    assert_eq!(source.rate_hz(), geometry::RATE_HZ);
    for (index, track) in STUB_TRACKS.iter().enumerate() {
        let meta = source.track(index).expect("an in-range track");
        assert_eq!(meta.start_sample, track.start_sample);
        assert_eq!(meta.n_samples, track.n_samples);
        assert_eq!(meta.title, track.title);
        assert_eq!(meta.artist, track.artist);
    }
    // Album length is the end of the last track, in album samples — not a frame or chunk count.
    assert_eq!(source.album().n_samples, 7_500);
    assert_eq!(source.album().title, "stub album");
    assert_eq!(source.album().art, None);
}

#[test]
fn stub_pull_frame_fills_at_most_one_frame_and_reports_its_length() {
    let mut source = StubSource::new(&STUB_TRACKS);
    let mut out = vec![0i16; 2 * geometry::FRAME_SAMPLES as usize];

    let n = source.pull_frame(&mut out).expect("first frame");
    assert_eq!(n, geometry::FRAME_SAMPLES as usize);
    // The triangle's first samples: phase 0 gives 0, and track 0 carries no offset. Left, right.
    assert_eq!(&out[..4], &[0, 0, 64, -64]);

    // Track 0 has 4,000 samples, so the second frame is short and must say so.
    let n = source.pull_frame(&mut out).expect("last frame");
    assert_eq!(n, 4_000 - geometry::FRAME_SAMPLES as usize);

    // A slice that cannot hold a whole frame is a named refusal, never a partial frame.
    let mut small = vec![0i16; 2 * 1_000];
    let mut fresh = StubSource::new(&STUB_TRACKS);
    assert_eq!(
        fresh.pull_frame(&mut small),
        Err(SourceError::BufferTooSmall {
            have: 1_000,
            need: geometry::FRAME_SAMPLES as usize,
        })
    );
}

#[test]
fn stub_start_track_repositions() {
    let mut source = StubSource::new(&STUB_TRACKS);
    let mut out = vec![0i16; 2 * geometry::FRAME_SAMPLES as usize];
    source.pull_frame(&mut out).expect("first frame");
    source.start_track(1).expect("track 1 exists");
    let n = source.pull_frame(&mut out).expect("track 1's first frame");
    assert_eq!(n, geometry::FRAME_SAMPLES as usize);
    // Back at sample 0 of track 1: phase 0, plus the per-track offset of 1,000.
    assert_eq!(&out[..2], &[1_000, -1_000]);
}

#[test]
fn stub_track_out_of_range_is_none() {
    let mut source = StubSource::new(&STUB_TRACKS);
    assert_eq!(source.track(STUB_TRACKS.len()), None);
    assert_eq!(
        source.start_track(STUB_TRACKS.len()),
        Err(SourceError::NoSuchTrack {
            index: STUB_TRACKS.len(),
            n_tracks: STUB_TRACKS.len(),
        })
    );
}

#[test]
fn stub_end_of_stream_is_ok_zero() {
    let mut source = StubSource::new(&STUB_TRACKS);
    let mut out = vec![0i16; 2 * geometry::FRAME_SAMPLES as usize];
    source.start_track(2).expect("track 2 exists");
    assert_eq!(source.pull_frame(&mut out), Ok(1_000));
    assert_eq!(source.pull_frame(&mut out), Ok(0));
    assert_eq!(source.pull_frame(&mut out), Ok(0));
}

#[test]
fn start_sample_is_unsupported_on_every_arm() {
    // Q5: seek is deferred, and a backend that has no seek says so by capability — never by
    // replaying from the track start and reporting a position the session clock would believe.
    let mut stub = StubSource::new(&STUB_TRACKS);
    assert_eq!(
        stub.start_sample(1_234),
        Err(SourceError::Unsupported(Capability::Seek))
    );

    #[cfg(feature = "backend-flac")]
    {
        let mut flac =
            album_player::backend::flac::FlacSource::new(parsed()).expect("the album opens");
        assert_eq!(
            flac.start_sample(1_234),
            Err(SourceError::Unsupported(Capability::Seek))
        );
    }
}

#[test]
fn the_stub_arm_drives_the_seam_with_no_codec_or_platform_in_its_module() {
    // The literal claim — that a stub image links no decoder at all — is observed by the
    // `album-seam-test` target's `--no-default-features --features backend-stub` build, because a
    // single test binary is compiled with this crate's default features. What this test proves is
    // the structural half: the stub arm satisfies the whole seam through `&mut dyn PcmSource`, so
    // the app half has no codec type to name (§4.2 rule 2), and its cost model says "not a
    // decoder" rather than pretending to be free.
    let mut source = StubSource::new(&STUB_TRACKS);
    let seam: &mut dyn PcmSource = &mut source;
    assert_eq!(seam.rate_hz(), geometry::RATE_HZ);
    assert_eq!(seam.producer_frame(), FrameCostModel::NOT_A_DECODER);

    let mut out = vec![0i16; 2 * geometry::FRAME_SAMPLES as usize];
    let n = seam.pull_frame(&mut out).expect("a synthetic frame");
    assert_eq!(n, geometry::FRAME_SAMPLES as usize);
    // Synthetic and deterministic: the same call on a fresh source gives the same bytes.
    let mut again = vec![0i16; 2 * n];
    let mut fresh = StubSource::new(&STUB_TRACKS);
    assert_eq!(fresh.pull_frame(&mut again), Ok(n));
    assert_eq!(again, out[..2 * n]);
}

// -------------------------------------------------------------------------------------------------
// 3b. The FLAC arm, on the host, against the shipped fixture
// -------------------------------------------------------------------------------------------------

/// The whole of track 1 (the clip the ROM plays in step iii) through `FlacSource`, folded with the
/// same FNV-1a/64 the bake pinned — the decode the ROM will run, on the host, checked against
/// `fnv_pcm` rather than against its own output.
///
#[cfg(feature = "backend-flac")]
#[test]
fn opening_the_flac_arm_starts_no_track() {
    // `FlacSource::new` no longer gates track 0 (the fixture's 12,293,131-byte master
    // the ROM never plays). A pull before `start_track` is an empty stream, not a decode.
    use album_player::backend::flac::FlacSource;

    let mut source = FlacSource::new(parsed()).expect("the album opens");
    let mut out = vec![0i16; 2 * geometry::FRAME_SAMPLES as usize];
    assert_eq!(
        source.pull_frame(&mut out),
        Ok(0),
        "an unselected album must not decode"
    );
    source
        .start_track(0)
        .expect("track 0 still gates on demand");
    assert!(matches!(source.pull_frame(&mut out), Ok(n) if n > 0));
}

/// The fixture blobs are generated by `make album-bake` and present in this build (`include_bytes!`
/// makes a missing one a compile error). `flac-lite` is platform-free, so this arm decodes on the
/// host through the core's `TableCountKernel`; `arm-kernel` adds the ARM kernel only under
/// `target_arch = "arm"`, which is why this test compiles and runs here at all.
#[cfg(feature = "backend-flac")]
#[test]
fn the_flac_arm_decodes_the_shipped_track_to_its_pcm_pin() {
    use album_player::backend::flac::FlacSource;
    use gba_playback::checksum::Fnv1a64;

    const TRACK: usize = 1;
    let pin = &assets::TRACKS[TRACK];

    let mut source = FlacSource::new(parsed()).expect("the album opens");
    assert_eq!(source.rate_hz(), geometry::RATE_HZ);
    assert_eq!(source.producer_frame(), FrameCostModel::FLAC);
    assert_eq!(source.n_tracks(), assets::TRACKS.len());
    let meta = source.track(TRACK).expect("track 1 exists");
    assert_eq!(meta.start_sample, pin.start_sample);
    assert_eq!(meta.n_samples, pin.n_samples);

    source
        .start_track(TRACK)
        .expect("track 1's container gates");
    let mut out = vec![0i16; 2 * geometry::FRAME_SAMPLES as usize];
    let mut hash = Fnv1a64::new();
    let mut samples = 0u64;
    let mut frames = 0u32;
    loop {
        let n = source.pull_frame(&mut out).expect("a frame decodes");
        if n == 0 {
            break;
        }
        assert!(
            n <= geometry::FRAME_SAMPLES as usize,
            "pull_frame returned more than one frame"
        );
        for &sample in &out[..2 * n] {
            hash.update_i16le(sample as i32);
        }
        samples += n as u64;
        frames += 1;
    }

    assert_eq!(
        frames, pin.frames,
        "frame count against the container's pin"
    );
    assert_eq!(
        samples, pin.samples,
        "sample count against the container's pin"
    );
    assert_eq!(
        hash.finish(),
        pin.fnv_pcm,
        "the decoded PCM does not hash to the bake's fnv_pcm pin"
    );

    // End of stream is sticky: a second pull is still 0, not a wrap to the track's start.
    assert_eq!(source.pull_frame(&mut out), Ok(0));
}

/// Step (iii)'s cheap proof that the seam's **sample scale** is right, before a ROM exists to get
/// it wrong: the exact path the ROM drives for the whole of track 1 —
/// `FlacSource::pull_frame` (interleaved i16) → sign-extend to the decoder's i32 →
/// `playback::convert_frame` → `playback::fold_bytes` in push order — must reproduce the bake's
/// `fnv_fifo_l` / `fnv_fifo_r`.
///
/// Both sides fold the same bytes, and this is where that is checked rather than assumed
/// (`scripts/gafp_pack.py:1260`'s `fifo_pins`): the pin is `(sample >> 8) & 0xFF` per channel over
/// the reference PCM in stream order, and `playback::fifo_byte` is `(sample >> 8) as u8` — only bits
/// 15..8 of the i16-range sample survive, so widening the seam's i16 back to i32 is lossless for the
/// FIFO byte, and FNV over one channel's bytes is streaming, so per-frame folding order equals the
/// whole-stream order the pin was folded in. A failure here is a seam-scale finding, not a test to
/// relax: it would mean the int16 boundary moved a sample's scale relative to the i32 the pins were
/// computed from.
#[cfg(feature = "backend-flac")]
#[test]
fn the_flac_arm_folds_track_1s_fifo_bytes_to_the_bake_pins() {
    use album_player::backend::flac::FlacSource;
    use gba_playback::checksum::Fnv1a64;
    use gba_playback::playback::{convert_frame, fold_bytes};

    const TRACK: usize = 1;
    let pin = &assets::TRACKS[TRACK];

    let mut source = FlacSource::new(parsed()).expect("the album opens");
    source
        .start_track(TRACK)
        .expect("track 1's container gates");

    let capacity = 2 * geometry::FRAME_SAMPLES as usize;
    let mut interleaved = vec![0i16; capacity];
    let mut left = vec![0i32; geometry::FRAME_SAMPLES as usize];
    let mut right = vec![0i32; geometry::FRAME_SAMPLES as usize];
    let mut bytes_l = vec![0u8; geometry::FRAME_SAMPLES as usize];
    let mut bytes_r = vec![0u8; geometry::FRAME_SAMPLES as usize];

    let mut hl = Fnv1a64::new();
    let mut hr = Fnv1a64::new();
    let mut samples = 0u64;
    loop {
        let n = source
            .pull_frame(&mut interleaved)
            .expect("a frame decodes");
        if n == 0 {
            break;
        }
        // The seam's form is interleaved i16; the FIFO conversion is defined over the decoder's
        // i32, so widen back exactly the way the ROM will (a checked cast, not a reinterpretation).
        for index in 0..n {
            left[index] = i32::from(interleaved[2 * index]);
            right[index] = i32::from(interleaved[2 * index + 1]);
        }
        let (l, r) = (&left[..n], &right[..n]);
        convert_frame(l, r, &mut bytes_l[..n], &mut bytes_r[..n])
            .expect("the decoded frame is inside the int16 profile");
        fold_bytes(&mut hl, &mut hr, &bytes_l[..n], &bytes_r[..n]);
        samples += n as u64;
    }

    assert_eq!(samples, pin.samples, "sample count against the bake's pin");
    assert_eq!(
        hl.finish(),
        pin.fnv_fifo_l,
        "the left FIFO bytes do not hash to the bake's fnv_fifo_l pin"
    );
    assert_eq!(
        hr.finish(),
        pin.fnv_fifo_r,
        "the right FIFO bytes do not hash to the bake's fnv_fifo_r pin"
    );
}

// -------------------------------------------------------------------------------------------------
// 5. The verdict screen's words against the tracked font subset
// -------------------------------------------------------------------------------------------------

/// `text::show` rasterizes exactly the glyphs `include_font!` found in the tracked subset and draws
/// them from a (4,4) origin, so a word outside the subset renders as nothing at all and one wider
/// than a row runs off the screen. Both are checked here against the words the ROM actually presents:
/// a rename to something we cannot draw fails in `make album-seam-test`, not on a cart.
#[test]
fn the_verdict_words_are_inside_the_tracked_subset_and_one_row() {
    for word in [album_player::text::PASSED, album_player::text::FAILED] {
        assert!(
            album_player::text::renderable(word),
            "{word:?} is not renderable: the tracked subset is U+{:04X}-U+{:04X} and a row is {} \
             columns ({} px at the font's 8 px cell)",
            album_player::text::FIRST_GLYPH as u32,
            album_player::text::LAST_GLYPH as u32,
            album_player::text::COLUMNS,
            album_player::text::COLUMNS * 8
        );
    }
    // Negative control, so a check that has only ever passed is not mistaken for proof: a glyph the
    // subset does not carry, and a word one column past the row, are both refused.
    assert!(!album_player::text::renderable("PASSÃ‰"));
    assert!(!album_player::text::renderable(
        &"A".repeat(album_player::text::COLUMNS + 1)
    ));
}
