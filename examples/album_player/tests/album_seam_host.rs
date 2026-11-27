//! PR-3a steps (ii) and (iii-A)'s host gate: the seam, its derived geometry, and the backend arms —
//! on the host — plus PR-3b-i's input seam and session clock.
//!
//! Seven claims, each with its own tests so a failure names one:
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
//! 6. **The input seam delivers the edges §5.3 means, at the frames it says** — debounce, edge
//!    polarity, the autorepeat's exact first-repeat and interval frames, the control map, the BIOS
//!    chord being absent by design, and the REPLAY script's entries landing on their own frame, no
//!    adjacent one, and no later than one frame behind their latch. This is the whole of what the
//!    gate can witness: `mgba-test-runner` cannot press a key, so the script is replayed *at the
//!    keys layer* and the ROM prints which source it used. Nothing here is evidence that a physical
//!    press reaches the ROM (§13 AP-1, AP-7).
//! 7. **The painted position obeys §4.4's three invariants** — never backwards within a track
//!    (including across a pause), 0 at a track start and `n_samples` at its end, with the
//!    `committed − LEAD_SAMPLES` subtraction saturating rather than wrapping when `committed < LEAD`.
//!
//! Nothing here is a hardware claim: the ROM's `agb` bootstrap, its feeder IRQ, the `.iwram` session
//! clock's `commit`/`committed_samples` pair and the two `KeySource` arm implementations are the ROM
//! half, which this file does not execute. The arm artifacts are checked as bytes (`size -A` for the
//! 4-byte `.iwram` the clock costs) and as a run (`[album-rom] input:` naming its source).

use album_player::assets::{self, TrackPin};
use album_player::backend::stub::{StubSource, StubTrack};
use album_player::geometry::{self, ChainGeometry};
use album_player::meta::{self, Album};
use album_player::nodiv;
use album_player::pcm::{Capability, FrameCostModel, PcmSource, SourceError};
use album_player::position::{self, LEAD_SAMPLES, paint_position};

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

/// The skip log's `head_silence = 8` on the two full-master tracks is the **track's own head**, not
/// the wire watch mistaking a quiet passage for silence.
///
/// The seam is the real one (`FlacSource` → `pull_frame` → the same `convert_frame` the producer
/// runs), because neither of the two cheaper sources can decide it: Q15's `silencedetect −42 dB /
/// 0.12 s` says where the *reference master* is quiet, which a near-zero sample satisfies as well as
/// a zero one, and the encoder's own report is not the FIFO bytes. The watch calls a period silent
/// when every one of its bytes is `0x00`, so this test asks exactly that question of the first
/// `8 × 560 = 4,480` samples the ROM's number is a claim about — all zero means the ROM is reporting
/// real music's silence and the log line is right; a non-zero byte inside that span would mean the
/// watch is calling a quiet passage silent and the bound belongs on `to_new` too.
///
/// The ninth chunk is decoded as well, for the other side of the same claim: `head_silence = 8`
/// says the head is exactly eight periods, so the first byte after them must be audible.
#[cfg(feature = "backend-flac")]
#[test]
fn the_masters_head_is_digital_silence_in_the_fifos_own_bytes() {
    use album_player::backend::flac::FlacSource;
    use gba_playback::playback::convert_frame;

    const TRACK: usize = 0;
    // The eight periods the skip log reads (`SkipRecord::head_silence`), plus the ninth chunk that
    // has to be audible for "exactly 8" to be the claim rather than ">= 8".
    const HEAD_CHUNKS: usize = 8;
    const HEAD_SAMPLES: usize = HEAD_CHUNKS * geometry::CHUNK_SAMPLES as usize;
    const PROBE_SAMPLES: usize = HEAD_SAMPLES + geometry::CHUNK_SAMPLES as usize;

    let mut source = FlacSource::new(parsed()).expect("the album opens");
    source
        .start_track(TRACK)
        .expect("track 0's container gates");

    let mut interleaved = vec![0i16; 2 * geometry::FRAME_SAMPLES as usize];
    let mut left = vec![0i32; geometry::FRAME_SAMPLES as usize];
    let mut right = vec![0i32; geometry::FRAME_SAMPLES as usize];
    let mut bytes_l = vec![0u8; PROBE_SAMPLES];
    let mut bytes_r = vec![0u8; PROBE_SAMPLES];
    let mut got = 0usize;
    while got < PROBE_SAMPLES {
        let n = source
            .pull_frame(&mut interleaved)
            .expect("a frame decodes");
        assert_ne!(
            n, 0,
            "track 0 ended inside its own first {PROBE_SAMPLES} samples"
        );
        let n = n.min(PROBE_SAMPLES - got);
        for index in 0..n {
            left[index] = i32::from(interleaved[2 * index]);
            right[index] = i32::from(interleaved[2 * index + 1]);
        }
        convert_frame(
            &left[..n],
            &right[..n],
            &mut bytes_l[got..got + n],
            &mut bytes_r[got..got + n],
        )
        .expect("the decoded frame is inside the int16 profile");
        got += n;
    }

    // The ROM's number: the first eight periods hand the DMA all-zero bytes, so the watch's silence
    // is the track's.
    assert!(
        bytes_l[..HEAD_SAMPLES].iter().all(|&byte| byte == 0)
            && bytes_r[..HEAD_SAMPLES].iter().all(|&byte| byte == 0),
        "the master's first {HEAD_SAMPLES} FIFO samples are not all zero — the watch is calling a \
         quiet passage silent, so `head_silence` is mis-predicated and `to_new` needs the bound"
    );
    // And exactly eight: the ninth chunk is audible on at least one channel, which is why the ROM
    // reads 8 and not more.
    assert!(
        bytes_l[HEAD_SAMPLES..PROBE_SAMPLES]
            .iter()
            .chain(&bytes_r[HEAD_SAMPLES..PROBE_SAMPLES])
            .any(|&byte| byte != 0),
        "the ninth chunk is silent too, so `head_silence` is longer than the 8 the ROM read"
    );
    // The exact index the ROM's comment names, so a change to the asset or the seam moves this
    // test rather than leaving a stale number in the log's own explanation: the master's first
    // non-zero sample is a −1 at 4,951, which `>> 8` turns into 0xFF on both channels.
    for (channel, bytes) in [("left", &bytes_l), ("right", &bytes_r)] {
        let first = bytes
            .iter()
            .position(|&byte| byte != 0)
            .expect("the ninth chunk was just asserted audible");
        assert_eq!(
            first, 4_951,
            "the {channel} channel's first non-zero FIFO byte moved"
        );
        assert_eq!(
            bytes[first], 0xFF,
            "the first audible {channel} byte is not -1 >> 8"
        );
    }
}

// -------------------------------------------------------------------------------------------------
// 5. The verdict screen's words against the tracked font subset
// -------------------------------------------------------------------------------------------------

/// `text::show` rasterizes exactly the glyphs `include_font!` found in the tracked subset and draws
/// them from a (4,4) origin, so a word outside the subset renders as nothing at all and one wider
/// than a row runs off the screen. Both are checked here against every word the ROM actually
/// presents: `PASSED`, `FAILED <TERM>` for each of `text::TERMS`, and each of those with the
/// `PAUSED` suffix. A rename to something we cannot draw fails in `make album-seam-test`, not on a
/// cart.
#[test]
fn the_verdict_words_are_inside_the_tracked_subset_and_one_row() {
    use album_player::text::{self, TERMS};
    let mut words = vec![
        text::PASSED.to_string(),
        text::FAILED.to_string(),
        format!("{} {}", text::PASSED, text::PAUSED),
        format!("{} {}", text::FAILED, text::PAUSED),
    ];
    for term in TERMS {
        // The two shapes the ROM draws for one false term: `FAILED SKIP`, and the same with the
        // paused suffix when the transport was frozen at teardown.
        words.push(format!("{} {term}", text::FAILED));
        words.push(format!("{} {term} {}", text::FAILED, text::PAUSED));
    }
    for word in &words {
        assert!(
            text::renderable(word),
            "{word:?} is not renderable: the tracked subset is U+{:04X}-U+{:04X} and a row is {} \
             columns ({} px at the font's 8 px cell)",
            text::FIRST_GLYPH as u32,
            text::LAST_GLYPH as u32,
            text::COLUMNS,
            text::COLUMNS * 8
        );
    }
    // The term list and the ROM's verdict line are one list: a term added to the screen without a
    // name here (or a name here that the verdict never tests) is caught by the count and the
    // longest-string check below.
    assert_eq!(TERMS.len(), 12, "the verdict has twelve terms");
    let longest = words
        .iter()
        .map(|word| word.chars().count())
        .max()
        .unwrap_or(0);
    assert!(
        longest <= text::COLUMNS,
        "the longest verdict string is {longest} columns"
    );
    // Negative control, so a check that has only ever passed is not mistaken for proof: a glyph the
    // subset does not carry, and a word one column past the row, are both refused.
    assert!(!text::renderable("PASSÃ‰"));
    assert!(!text::renderable(&"A".repeat(text::COLUMNS + 1)));
}

// -------------------------------------------------------------------------------------------------
// 6. The input seam (§5.3, D1) — edges, debounce, autorepeat, the chord, and the REPLAY script
// -------------------------------------------------------------------------------------------------
//
// The arm half (`keys::Hardware`, the `ButtonController` round trip in `keys::Replay`) is not
// executed here; what is executed is every rule `keys.rs` adds to agb, over the same `Keys` machine
// the ROM drives. The REPLAY type is compiled on the host unconditionally for exactly this reason
// (`keys.rs`'s `Replay` cfg), so the script reader is tested as itself rather than as a copy.

use album_player::keys::{
    self, Action, BTN_A, BTN_B, BTN_DOWN, BTN_L, BTN_LEFT, BTN_R, BTN_RIGHT, BTN_SELECT, BTN_START,
    BTN_UP, Cursor, DEBOUNCE_FRAMES, Detected, KeySource, Keys, REPEAT_DELAY_FRAMES,
    REPEAT_INTERVAL_FRAMES, Replay, SOFT_RESET_CHORD, Sampler,
};

/// A press edge is a *debounced* transition, so a raw press at frame `F` acts at `F + 1` — and a
/// held button never acts twice.
#[test]
fn an_edge_is_a_debounced_transition_not_a_contact() {
    let mut keys = Keys::new();
    // Nothing leaves the machine on the frame the contact closes: `DEBOUNCE_FRAMES` frames must
    // agree before the held set moves.
    let first = keys.sample(BTN_A);
    assert_eq!((first.pressed, first.released, first.held), (0, 0, 0));
    let confirmed = keys.sample(BTN_A);
    assert_eq!(
        (confirmed.pressed, confirmed.released, confirmed.held),
        (BTN_A, 0, BTN_A)
    );
    assert_eq!(confirmed.action(), Some(Action::PauseToggle));
    // Holding delivers no second edge, however long it is held.
    for frame in 0..60 {
        assert_eq!(
            keys.sample(BTN_A).pressed,
            0,
            "held frame {frame} re-pressed"
        );
    }
    // Release is the same transition in the other direction, and it costs the same one frame.
    let first_up = keys.sample(0);
    assert_eq!(first_up.released, 0);
    let up = keys.sample(0);
    assert_eq!((up.pressed, up.released, up.held), (0, BTN_A, 0));
}

/// Two frames of agreement is the whole debounce rule: a contact that never agrees with itself for
/// two frames is not a press, and a dropout shorter than the window is not a release.
#[test]
fn a_bounce_shorter_than_the_debounce_window_delivers_nothing() {
    assert_eq!(DEBOUNCE_FRAMES, 2);
    let mut keys = Keys::new();
    // A contact alternating every frame: 100 raw transitions, zero edges. A raw `is_just_pressed`
    // would report 50 presses here, which is the bug the debounce exists to prevent.
    for frame in 0..100u16 {
        let raw = if frame % 2 == 0 { BTN_START } else { 0 };
        let sampled = keys.sample(raw);
        assert_eq!(sampled.pressed, 0, "bounced frame {frame} produced a press");
        assert_eq!(
            sampled.released, 0,
            "bounced frame {frame} produced a release"
        );
        assert_eq!(sampled.held, 0);
    }
    // And a one-frame dropout in the middle of a confirmed press is absorbed: it is shorter than the
    // window, so the held set never moved and the application saw one press, not two.
    let mut keys = Keys::new();
    keys.sample(BTN_START);
    keys.sample(BTN_START);
    assert_eq!(keys.held(), BTN_START);
    assert_eq!(keys.sample(0).released, 0);
    keys.sample(BTN_START);
    let after_dropout = keys.sample(BTN_START);
    assert_eq!((after_dropout.pressed, after_dropout.released), (0, 0));
    assert_eq!(keys.held(), BTN_START);
}

/// The autorepeat's exact frames, asserted against the two constants rather than described: first
/// repeat `REPEAT_DELAY_FRAMES` after the press edge, then every `REPEAT_INTERVAL_FRAMES`.
#[test]
fn a_held_direction_repeats_at_the_documented_delay_and_interval() {
    let mut keys = Keys::new();
    let mut pressed_at = Vec::new();
    let mut repeats = Vec::new();
    for frame in 0..80u16 {
        let sampled = keys.sample(BTN_LEFT);
        if sampled.pressed != 0 {
            pressed_at.push(frame);
            // A press and its list step go through different accessors: `cursor` for the edge,
            // `repeat_cursor` for the burst below.
            assert_eq!(sampled.cursor(), Some(Cursor::Up));
        }
        if sampled.repeated != 0 {
            assert_eq!(sampled.repeated, BTN_LEFT);
            // Left is "up" in a list (§5.3's cursor column), and the repeat is delivered through
            // `repeat_cursor` — never through `action`, which reads `pressed` only.
            assert_eq!(sampled.repeat_cursor(), Some(Cursor::Up));
            repeats.push(frame);
        }
    }
    assert_eq!(pressed_at, vec![1]);
    assert_eq!(repeats[0], 1 + REPEAT_DELAY_FRAMES);
    assert_eq!(repeats, vec![25, 33, 41, 49, 57, 65, 73]);
    for pair in repeats.windows(2) {
        assert_eq!(pair[1] - pair[0], REPEAT_INTERVAL_FRAMES);
    }

    // A second held button does not stop the burst: repeat is per axis, not per frame.
    let mut keys = Keys::new();
    let mut held_burst = 0;
    for _ in 0..40 {
        if keys.sample(BTN_LEFT | BTN_START).repeated != 0 {
            held_burst += 1;
        }
    }
    assert_eq!(held_burst, 2);

    // Both directions of one axis down cancels that axis (agb's `x_tri` convention), so a thumb
    // straddling Left and Right does not scroll at all.
    let mut keys = Keys::new();
    for _ in 0..80 {
        assert_eq!(keys.sample(BTN_LEFT | BTN_RIGHT).repeated, 0);
    }
}

/// §5.3's "now playing" column, one row at a time, plus the two properties that are rules: the map
/// is a pure function of one frame's edges (no time-dependent chord), and Down's restart is an edge
/// and never a repeat.
#[test]
fn the_now_playing_map_is_the_tables_and_needs_no_clock() {
    assert_eq!(
        keys::now_playing_action(BTN_START),
        Some(Action::PauseToggle)
    );
    assert_eq!(keys::now_playing_action(BTN_A), Some(Action::PauseToggle));
    assert_eq!(keys::now_playing_action(BTN_RIGHT), Some(Action::NextTrack));
    assert_eq!(keys::now_playing_action(BTN_R), Some(Action::NextTrack));
    assert_eq!(keys::now_playing_action(BTN_LEFT), Some(Action::PrevTrack));
    assert_eq!(keys::now_playing_action(BTN_L), Some(Action::PrevTrack));
    assert_eq!(
        keys::now_playing_action(BTN_DOWN),
        Some(Action::RestartTrack)
    );
    // §5.3's `—`: Up and B bind nothing while a track plays. **Select is no longer one of
    // them**: 2b PR-(3) gave it §5.3's scene cycle (the card is reachable mid-album), so the
    // row is asserted rather than grouped with the unbound keys.
    for unbound in [BTN_UP, BTN_B, 0] {
        assert_eq!(keys::now_playing_action(unbound), None);
    }
    assert_eq!(
        keys::now_playing_action(BTN_SELECT),
        Some(Action::SelectScene)
    );
    // Select is the last row: a transport press on the same frame wins, so a scene change
    // can never swallow a pause.
    assert_eq!(
        keys::now_playing_action(BTN_START | BTN_SELECT),
        Some(Action::PauseToggle)
    );
    // Two mapped edges in one frame resolve to the table's first row, deterministically — the rule
    // only has to be stable, and it is a `const fn`, so it is also checked at compile time.
    assert_eq!(
        keys::now_playing_action(BTN_A | BTN_RIGHT | BTN_DOWN),
        Some(Action::PauseToggle)
    );
    assert_eq!(
        keys::now_playing_action(BTN_RIGHT | BTN_DOWN),
        Some(Action::NextTrack)
    );

    // No time-dependent chord: a long Left hold (200 frames, i.e. > 3 s — the window §4.4 removed)
    // followed by one Down press restarts the track. Nothing in the signature remembers the Left.
    let mut keys = Keys::new();
    let mut actions = Vec::new();
    for frame in 0..206u16 {
        let raw = match frame {
            0..200 => BTN_LEFT,
            200..202 => 0,
            202..204 => 0,
            _ => BTN_DOWN,
        };
        let sampled = keys.sample(raw);
        if let Some(action) = sampled.action() {
            actions.push(action);
        }
    }
    assert_eq!(actions, vec![Action::PrevTrack, Action::RestartTrack]);

    // Down repeats (it is a direction in a list), but the now-playing binding reads `pressed`, so a
    // held Down restarts once. Applying `action()` to `repeated` would restart every 134 ms; that is
    // why the repeat is delivered through `repeat_cursor` and never through `action`.
    let mut keys = Keys::new();
    let mut restarts = 0;
    let mut down_repeats = 0;
    for _ in 0..40 {
        let sampled = keys.sample(BTN_DOWN);
        if sampled.action() == Some(Action::RestartTrack) {
            restarts += 1;
        }
        if sampled.repeated != 0 {
            down_repeats += 1;
        }
    }
    assert_eq!(restarts, 1);
    assert_eq!(down_repeats, 2);
}

/// L + R + Start belongs to the BIOS (§5.3). The chord has no `Action`, and while it is held the
/// machine delivers no events at all — the limit, that its component edges arrive while the chord is
/// still being assembled, is asserted rather than glossed.
#[test]
fn the_soft_reset_chord_is_never_application_state() {
    assert_eq!(SOFT_RESET_CHORD, BTN_L | BTN_R | BTN_START);
    let mut keys = Keys::new();
    // One frame to confirm the (already fully held) chord, so the loop below starts on a debounced
    // chord rather than on the frame that is still waiting for its second agreement.
    keys.sample(SOFT_RESET_CHORD);
    for frame in 0..40u16 {
        let sampled = keys.sample(SOFT_RESET_CHORD);
        assert_eq!(sampled.held, SOFT_RESET_CHORD);
        assert_eq!(
            (sampled.pressed, sampled.released, sampled.repeated),
            (0, 0, 0),
            "the held chord emitted an edge on frame {frame}"
        );
        assert_eq!(sampled.action(), None);
        assert_eq!(sampled.repeat_cursor(), None);
    }
    // The documented limit: pressed one at a time, the earlier buttons act before the chord exists.
    // Suppressing that would need a time window, which §5.3's "nothing time-dependent" forbids.
    let mut keys = Keys::new();
    keys.sample(BTN_L);
    assert_eq!(keys.sample(BTN_L).action(), Some(Action::PrevTrack));
    keys.sample(BTN_L | BTN_R);
    assert_eq!(keys.sample(BTN_L | BTN_R).action(), Some(Action::NextTrack));
    keys.sample(SOFT_RESET_CHORD);
    let chord = keys.sample(SOFT_RESET_CHORD);
    assert_eq!((chord.pressed, chord.released), (0, 0));
    assert_eq!(chord.action(), None);
}

/// The REPLAY script is the gate's input, so its frame numbers are a contract: an entry at frame `N`
/// must not fire at `N - 1`, and the delivered edge must be on `N` itself (the machine's debounce
/// then costs one more frame, which is stated rather than hidden).
#[test]
fn replay_delivers_each_entry_on_its_own_frame_and_no_adjacent_one() {
    // `(frame, mask)` means "from frame `F` the mask is `M`" — a history, not a delta list.
    const SCRIPT: [(u32, u16); 3] = [(4, BTN_A), (9, 0), (12, BTN_DOWN)];
    let mut source = Replay::new(&SCRIPT);
    let mut keys = Keys::new();
    let mut masks = Vec::new();
    let mut edges = Vec::new();
    let mut pressed = Vec::new();
    for _ in 0..16 {
        let mask = source.sample();
        masks.push(mask);
        edges.push(source.edges());
        pressed.push(keys.sample(mask).pressed);
    }
    assert_eq!(source.entries(), SCRIPT.len());
    assert_eq!(source.frame(), 16);
    // The mask in force, frame by frame: the entry at 4 is *not* in force at 3, and the entry at 9
    // does not linger past 8.
    assert_eq!(
        masks,
        vec![
            0, 0, 0, 0, BTN_A, BTN_A, BTN_A, BTN_A, BTN_A, 0, 0, 0, BTN_DOWN, BTN_DOWN, BTN_DOWN,
            BTN_DOWN
        ]
    );
    // The delivered edge is counted once, on the entry's own frame — not on the frame before and not
    // on the frame after.
    assert_eq!(&edges[..6], &[0, 0, 0, 0, 1, 1]);
    assert_eq!(edges[9], 1);
    assert_eq!(edges[11], 1);
    assert_eq!(edges[12], 2);
    assert_eq!(source.edges(), 2);
    // And through the machine, the application edge lands one frame after the script's transition
    // (the debounce), exactly once per entry.
    let pressed_frames: Vec<usize> = pressed
        .iter()
        .enumerate()
        .filter(|(_, mask)| **mask != 0)
        .map(|(frame, _)| frame)
        .collect();
    assert_eq!(pressed_frames, vec![5, 13]);
}

/// The ROM's `REPLAY_SCRIPT` (`src/album_play_main.rs`), repeated here because a `[[bin]]`'s private
/// `const` is not importable. Its end-of-run line claims **6 edges and 7 autorepeats** — two
/// pause/resume pairs, one inert direction press, and one held direction — so this test is what makes
/// that claim arithmetic rather than hope. If the ROM's script changes, this one changes with it: the
/// numbers in the log are checked against this shape.
const ROM_REPLAY_SCRIPT: [(u32, u16); 12] = [
    (8, keys::BTN_A),
    (14, 0),
    (60, keys::BTN_A),
    (66, 0),
    (120, keys::BTN_RIGHT),
    (126, 0),
    (180, keys::BTN_START),
    (186, 0),
    (300, keys::BTN_START),
    (306, 0),
    (360, keys::BTN_LEFT),
    (440, 0),
];

#[test]
fn the_rom_replay_script_delivers_six_edges_and_seven_repeats() {
    // The ISR-shaped seam: `Sampler::sample` is the vblank handler's half and `take` is the
    // application's, exactly as the ROM splits it.
    let mut source = Replay::new(&ROM_REPLAY_SCRIPT);
    let mut sampler = Sampler::new();
    let mut press_frames = Vec::new();
    let mut repeat_frames = Vec::new();
    for frame in 0..450u32 {
        let sampled = sampler.sample(source.sample());
        if sampled.repeated != 0 {
            repeat_frames.push(frame);
        }
        if let Some(detected) = sampler.take() {
            // One frame after each scripted entry (8, 24, 40), and stamped with the frame it was
            // detected on — never with a frame before it.
            assert_eq!(detected.frame, frame);
            press_frames.push(frame);
        }
    }
    // One edge frame per scripted mask change, one frame after it (the debounce): A at 8, A again at
    // 60, Right at 120, Start at 180, Start again at 300, Left at 360.
    assert_eq!(press_frames, vec![9, 61, 121, 181, 301, 361]);
    assert_eq!(sampler.presses(), 6);
    assert_eq!(sampler.repeats(), 7);
    assert_eq!(sampler.dropped(), 0);
    assert_eq!(source.edges(), 6);
    // Left is held from frame 361 to 440: first repeat REPEAT_DELAY_FRAMES later, then every
    // REPEAT_INTERVAL_FRAMES until the release at 440 clears the burst.
    assert_eq!(repeat_frames[0], 361 + u32::from(REPEAT_DELAY_FRAMES));
    assert_eq!(repeat_frames, vec![385, 393, 401, 409, 417, 425, 433]);
    assert_eq!(source.frame(), 450);
}

/// The ISR-shaped seam's two halves, and the property that makes the frame stamp worth carrying:
/// actions are posted one per frame, stamped with that frame, and a consumer can never be handed an
/// older action after a newer one — however late it takes them and however many a single slot had to
/// displace. This is what the ROM's `detect_latency` measurement rests on (the press is one frame
/// after its scripted mask change) and what a frame boundary cannot reorder.
#[test]
fn the_isr_posts_edges_in_frame_order_and_never_reorders_across_a_boundary() {
    // Frame 1: A confirms (`PauseToggle`). Frame 3: Right confirms while A is still held
    // (`NextTrack`). Frame 5: Down confirms (`RestartTrack`). Nothing on the frames in between.
    const RAW: [u16; 6] = [
        BTN_A,
        BTN_A,
        BTN_A | BTN_RIGHT,
        BTN_A | BTN_RIGHT,
        BTN_A | BTN_RIGHT | BTN_DOWN,
        BTN_A | BTN_RIGHT | BTN_DOWN,
    ];

    // A consumer that takes every frame sees each action once, on its own frame, in order.
    let mut sampler = Sampler::new();
    let mut seen = Vec::new();
    for raw in RAW {
        sampler.sample(raw);
        if let Some(detected) = sampler.take() {
            seen.push(detected);
        }
    }
    assert_eq!(
        seen,
        vec![
            Detected {
                frame: 1,
                action: Action::PauseToggle,
            },
            Detected {
                frame: 3,
                action: Action::NextTrack,
            },
            Detected {
                frame: 5,
                action: Action::RestartTrack,
            },
        ],
        "the frames with a press edge are 1, 3 and 5, and every other frame posts nothing"
    );
    assert_eq!(sampler.sampled(), RAW.len() as u32);
    assert_eq!((sampler.presses(), sampler.dropped()), (3, 0));

    // A consumer slower than the video clock: it takes on frames 3 and 5 only. The slot has already
    // been displaced once by then, and what it hands over can only ever be newer than what it
    // handed over last — an older action after a newer one is the reordering this cannot do.
    let mut sampler = Sampler::new();
    let mut delivered = Vec::new();
    for (index, raw) in RAW.iter().enumerate() {
        sampler.sample(*raw);
        if index == 3 || index == 5 {
            if let Some(detected) = sampler.take() {
                delivered.push(detected);
            }
        }
    }
    assert_eq!(
        delivered,
        vec![
            Detected {
                frame: 3,
                action: Action::NextTrack,
            },
            Detected {
                frame: 5,
                action: Action::RestartTrack,
            },
        ],
        "the slot keeps the newest action; the displaced one is counted, not delivered late"
    );
    assert_eq!(sampler.dropped(), 1);
    for pair in delivered.windows(2) {
        assert!(
            pair[0].frame < pair[1].frame,
            "an action from frame {} arrived after one from frame {}",
            pair[0].frame,
            pair[1].frame
        );
    }
}

/// §4.5's vblank handler samples once per frame, so a scripted press stamped at frame `F` is
/// *detected* at `F + DEBOUNCE_FRAMES - 1` = `F + 1`: **one video frame from the mask change to the
/// detection**, which is the floor §4.4's observable has and the number the ROM measures against
/// `keys::Replay::changed()`. It can never be detected before its own frame, which is the direction
/// that would let a key act on a frame that had not happened yet.
#[test]
fn a_scripted_edge_is_delivered_within_one_frame_of_its_latch() {
    // The whole lag this layer is allowed: the debounce's single confirmation frame.
    const LATCH_TO_DELIVERY: u32 = DEBOUNCE_FRAMES as u32 - 1;
    // Every entry carrying a button is a press entry of this script: each press is released before
    // the next, so `mask != 0` identifies the latch frame of each one.
    let stamps: Vec<u32> = ROM_REPLAY_SCRIPT
        .iter()
        .filter(|(_, mask)| *mask != 0)
        .map(|(frame, _)| *frame)
        .collect();
    let mut source = Replay::new(&ROM_REPLAY_SCRIPT);
    let mut sampler = Sampler::new();
    let mut delivered = Vec::new();
    for _ in 0..450u32 {
        sampler.sample(source.sample());
        if let Some(detected) = sampler.take() {
            delivered.push((
                detected.frame,
                source
                    .mask_changed()
                    .expect("a script knows when its mask changed"),
            ));
        }
    }
    assert_eq!(
        delivered.len(),
        stamps.len(),
        "the script presses {} times but {} actions were delivered",
        stamps.len(),
        delivered.len()
    );
    for ((frame, changed), stamp) in delivered.iter().zip(&stamps) {
        assert_eq!(
            changed, stamp,
            "the mask change the detection is measured from is the script's own entry"
        );
        assert!(
            *frame > *stamp,
            "an action was detected at {frame}, before its scripted latch at {stamp}"
        );
        assert!(
            *frame - *stamp <= LATCH_TO_DELIVERY,
            "the press latched at {stamp} was detected at {frame}: more than {LATCH_TO_DELIVERY} \
             frame(s) from latch to detection"
        );
    }
    assert_eq!(sampler.presses() as usize, stamps.len());
}

// -------------------------------------------------------------------------------------------------
// 7. The session clock (§10.4, ruled option A) and §4.4's painted position
// -------------------------------------------------------------------------------------------------
//
// The counter itself is arm-only (`.iwram`, written from the period IRQ); what is testable on the
// host is the map from it to what the display shows, which is where the invariants live.

/// Within a track the painted position is monotone, a pause holds it still, and a skip's only step
/// backwards is the new track's own 0.
#[test]
fn the_painted_position_never_runs_backwards_within_a_track() {
    const TOTAL: u32 = 491_520;
    let mut previous = 0;
    for committed in (0..=TOTAL).step_by(137) {
        let paint = paint_position(committed, TOTAL);
        assert!(
            paint >= previous,
            "position went backwards at committed={committed}: {previous} -> {paint}"
        );
        // The display may lag the commit but may never lead it.
        assert!(paint <= committed);
        assert!(paint <= TOTAL);
        previous = paint;
    }
    assert_eq!(paint_position(TOTAL, TOTAL), TOTAL);

    // A pause freezes the chain, and the clock is incremented *in* the period IRQ, so a stopped
    // timer stops the clock: the painted value is identical before, during and after the frozen
    // span (three seconds' worth of frames).
    let at_pause = 250_000;
    let frozen = paint_position(at_pause, TOTAL);
    for _ in 0..180 {
        assert_eq!(paint_position(at_pause, TOTAL), frozen);
    }

    // A skip drops the queue and starts the next track at 0 — the only step backwards in the whole
    // sequence, and it lands on 0 rather than on a wrapped or negative value.
    let next_start = paint_position(0, TOTAL);
    assert_eq!(next_start, 0);
    assert!(next_start <= frozen);
    // …and then grows again from it, which is the new track's own invariant 1.
    assert!(paint_position(120_000, TOTAL) > next_start);
}

/// §4.4's two clamps: 0 at a track start and `n_samples` exactly at its end.
#[test]
fn the_painted_position_is_zero_at_a_start_and_the_total_at_its_end() {
    const TOTAL: u32 = 491_520;
    assert_eq!(paint_position(0, TOTAL), 0);
    assert_eq!(paint_position(TOTAL, TOTAL), TOTAL);
    // The end clamp is one-sided and deliberate: `committed` never exceeds the pin, but a stale
    // clock must still not paint past the track's own length.
    assert_eq!(paint_position(TOTAL + 1, TOTAL), TOTAL);
    // A zero-length track is 0 at both ends rather than a subtraction of nothing.
    assert_eq!(paint_position(0, 0), 0);
}

/// `committed − LEAD_SAMPLES` saturates: a position committed but not yet audible paints 0, never a
/// wrapped ~4.29-billion.
#[test]
fn the_lead_subtraction_cannot_underflow() {
    const TOTAL: u32 = 491_520;
    // The lead's composition and its size: two DMA buffers plus the FIFO's half-full window.
    assert_eq!(position::DMA_BUFFERS, 2);
    assert_eq!(
        LEAD_SAMPLES,
        position::DMA_BUFFERS * geometry::CHUNK_SAMPLES + geometry::REARM_GRACE_SAMPLES
    );
    assert_eq!(LEAD_SAMPLES, 1_136);
    // 1,136 samples at 16,384 Hz = 69 ms, the residual §4.4 ruled invisible in a bar or an mm:ss.
    assert_eq!(LEAD_SAMPLES * 1_000 / 16_384, 69);
    for committed in 0..=LEAD_SAMPLES {
        assert_eq!(
            paint_position(committed, TOTAL),
            0,
            "committed={committed} is inside the lead but painted past it"
        );
    }
    assert_eq!(paint_position(LEAD_SAMPLES + 1, TOTAL), 1);
    // Beyond the lead it is a plain subtraction, not a scaled or averaged one.
    assert_eq!(
        paint_position(20_160, TOTAL + 1_000_000),
        20_160 - LEAD_SAMPLES
    );
}

// -------------------------------------------------------------------------------------------------
// 8. The hand-off gain and §4.4's pause contract (PR-3b-ii-B2a)
// -------------------------------------------------------------------------------------------------
//
// The ROM's pause is two halves, and the model keeps them apart the way the image does: the
// *feeder* (a period IRQ: `take_chunk` → the hand-off gain → the session clock) and the *main loop*
// (the transport's decisions, then the hardware work each step names). Both halves are the real
// code — `RingFeed`, `HandOffGain`, `scale_byte` and `Transport` — and only the timers, the two
// DMAs and the `.iwram` clock are modelled, because those are the parts that need a cartridge. The
// claims are stated in periods and samples, the unit the contract is written in.
//
// What this cannot witness: that a pause *sounds* click-free (AP-7, a human with a cart), the
// vblank handler's cost, or anything about mGBA. Those are the ROM run's lines, not this file's.

use album_player::pcm::{AlbumMeta, TrackMeta};
use album_player::transport::{
    FADE_PERIODS, FADE_SAMPLES, GAIN_UNITY, HandOffGain, SKIP_PRIME_CHUNKS, State, Step, Transport,
    pause_to_freeze_bound, resume_to_audio_bound, scale_byte, take_to_handoff_bound,
};
use gba_playback::playback::{Chunk, PushError, RingFeed, convert_frame, fifo_byte};

const CHUNK: usize = geometry::CHUNK_SAMPLES as usize;
const FRAME: usize = geometry::FRAME_SAMPLES as usize;
const RING: usize = geometry::RING_SAMPLES as usize;
const PREFILL: usize = geometry::PREFILL_SAMPLES as usize;

/// The gate track's length — the run `make album-rom` makes (`assets::TRACKS[1]`).
const TOTAL: u32 = 491_520;

/// The fake source's sample `i`, in every channel: a sawtooth that crosses zero, so the byte stream
/// is a closed form and a skipped or repeated sample is a discontinuity rather than a plausible run.
/// It stays inside ±60 FIFO codes, i.e. well inside the 16-bit profile `convert_frame` checks.
fn pattern_sample(i: u32) -> i32 {
    ((i % 512) as i32 - 256) * 60
}

/// A `PcmSource` with no codec: `tracks` of samples in play order, frames of `FRAME_SAMPLES`, and the
/// closed form above. It is the model's producer input, so the seam is exercised through its real
/// trait rather than through a special-shaped helper.
///
/// `lead_silent` is the number of **digital-silence** samples at every track's head. It exists because
/// a track that fades in offends the one question a wire can answer ("are these bytes zero?") and the
/// skip log has a term for it; one track in the ROM's own fixture opens that way.
struct FakeSource {
    at: u32,
    /// The current track's index.
    track: usize,
    /// Track lengths, in album sample order.
    tracks: Vec<u32>,
    /// Samples of digital silence at the head of every track (`0` = opens audibly).
    lead_silent: u32,
}

impl FakeSource {
    /// An album whose tracks are `tracks` samples long, each opening with `lead_silent` zeros.
    fn album(tracks: &[u32], lead_silent: u32) -> Self {
        Self {
            at: 0,
            track: 0,
            tracks: tracks.to_vec(),
            lead_silent,
        }
    }

    /// Samples in the current track.
    fn len(&self) -> u32 {
        self.tracks[self.track]
    }

    /// Sample `i` of the current track: the closed form, gated to silence for the track's own head.
    fn sample(&self, i: u32) -> i32 {
        if i < self.lead_silent {
            0
        } else {
            pattern_sample(i)
        }
    }
}

impl PcmSource for FakeSource {
    fn n_tracks(&self) -> usize {
        self.tracks.len()
    }

    fn track(&self, index: usize) -> Option<TrackMeta> {
        if index >= self.tracks.len() {
            return None;
        }
        let start_sample = self.tracks[..index].iter().sum::<u32>();
        Some(TrackMeta {
            start_sample,
            n_samples: self.tracks[index],
            title: "fake",
            artist: "",
        })
    }

    fn album(&self) -> AlbumMeta {
        AlbumMeta {
            title: "fake",
            artist: "",
            year: 0,
            n_samples: self.tracks.iter().sum(),
            art: None,
        }
    }

    fn rate_hz(&self) -> u32 {
        geometry::RATE_HZ
    }

    fn start_track(&mut self, index: usize) -> Result<(), SourceError> {
        if index >= self.tracks.len() {
            return Err(SourceError::NoSuchTrack {
                index,
                n_tracks: self.tracks.len(),
            });
        }
        self.track = index;
        self.at = 0;
        Ok(())
    }

    fn start_sample(&mut self, _s: u32) -> Result<(), SourceError> {
        Err(SourceError::Unsupported(Capability::Seek))
    }

    fn pull_frame(&mut self, out: &mut [i16]) -> Result<usize, SourceError> {
        if out.len() < 2 * FRAME {
            return Err(SourceError::BufferTooSmall {
                have: out.len() / 2,
                need: FRAME,
            });
        }
        let n = ((self.len() - self.at) as usize).min(FRAME);
        for i in 0..n {
            let value = self.sample(self.at + i as u32) as i16;
            out[2 * i] = value;
            out[2 * i + 1] = value;
        }
        self.at += n as u32;
        Ok(n)
    }

    fn producer_frame(&self) -> FrameCostModel {
        FrameCostModel::NOT_A_DECODER
    }

    fn reset_decoder_state(&mut self) {}
}

/// One pause/resume pair, as the main loop observed it.
#[derive(Clone, Copy, Debug)]
struct Pause {
    /// What the session clock said when the toggle was taken.
    committed_at_toggle: u32,
    /// What it said when the chain was actually frozen — the position the resume must keep.
    frozen_committed: u32,
    /// The hand-off gain at the freeze point: the contract says zero.
    gain_at_freeze: i32,
    /// Feeder periods from the toggle to the freeze (the fade window, in periods).
    fade_periods: u32,
    /// The clock the main loop saw when it took the resume.
    resumed_from: u32,
    /// Video frames the chain spent frozen.
    frozen_frames: u32,
    /// **PR-3c.** Whether the ring could not give the prime's chunk, so the resume took the refill route
    /// instead of the direct prime. The claim of this whole PR is that both routes keep the position:
    /// `dropped()` is zero either way, and this says which of the two paths got there.
    refilled: bool,
    /// Producer frames (one per main-loop pass) the refill spent reaching the prefill's depth. Zero on
    /// the direct route, which is the point: a ring that can prime costs nothing extra.
    refill_passes: u32,
}

impl Pause {
    /// Samples lost across the freeze: zero by contract, because the ring was never touched.
    fn dropped(&self) -> u32 {
        self.resumed_from.saturating_sub(self.frozen_committed)
    }
}

/// One skip, measured the way the ROM's wire watch measures it: the same stamps, the same
/// subtractions, so these numbers and the ROM's `skip:` rows are one arithmetic rather than two.
///
/// **The latch is per record.** `silent_at`/`new_at`/`content_at` are stamped on this record by the
/// feeder scan while `open` is set, so a second hand-off cannot retire, overwrite or orphan the first
/// one's reading — the defect R13's second regression test drives (one global watch re-armed by every
/// hand-off left the first record `open` with `content_at = 0`, and `content_at()` then read
/// `0 − at`). The ROM's fix is the same shape: one wire-watch slot per in-flight record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Skip {
    from: usize,
    to: usize,
    /// The mark: the period index the hand-off's critical section ran in.
    at: u32,
    /// The first all-silent period after the mark, then the first non-silent one after that.
    silent_at: u32,
    new_at: u32,
    /// The first period whose chunk is handed over after the cover countdown.
    content_at: u32,
    underflows_at: u32,
    underflows: u32,
    /// Samples `truncate()` stepped over, and samples the ring held when it ran: a skip that dropped
    /// nothing is a fact about this depth (`ready() < CHUNK`), not a missing truncate.
    dropped: u32,
    queued: u32,
    /// The period the main loop took the post on.
    press_at: u32,
    /// The new track has not reached the wire yet: this record's own latch is still armed.
    open: bool,
}

impl Skip {
    /// Periods from the mark to the first silent chunk on the wire; `1` is the watch's floor.
    fn silent_after_mark(&self) -> u32 {
        self.silent_at.wrapping_sub(self.at)
    }

    /// Stale-track periods heard after the hand-off — the floor subtracted, as the probe's table does.
    fn old(&self) -> u32 {
        self.silent_after_mark().saturating_sub(1)
    }

    /// Periods the wire carried the cover's silence.
    fn silence(&self) -> u32 {
        self.new_at.wrapping_sub(self.silent_at)
    }

    /// Periods from the hand-off to the first non-silent chunk: the probe's own `new_at − at`.
    fn to_new(&self) -> u32 {
        self.new_at.wrapping_sub(self.at)
    }

    /// Periods from the hand-off to the first chunk handed over after the cover countdown.
    ///
    /// `saturating_sub`, mirroring the ROM: an unresolved latch reads `0`, which fails the
    /// content-point term as a number rather than wrapping to 4,294,966,995 in a log line.
    fn content_at(&self) -> u32 {
        self.content_at.saturating_sub(self.at)
    }

    /// Periods of the new track's own digital silence that follow the cover.
    fn head_silence(&self) -> u32 {
        self.to_new().saturating_sub(self.content_at())
    }

    fn underflow_delta(&self) -> u32 {
        self.underflows.saturating_sub(self.underflows_at)
    }

    /// Periods from the loop taking the post to the hand-off: the frame loop's own latency.
    fn press_to_handoff(&self) -> u32 {
        self.at.wrapping_sub(self.press_at)
    }
}

/// The ROM's `Paint` (`album_play_main.rs`), at period granularity: §4.4's three invariants, checked on
/// every frame the loop reads. `start` clears the last read without clearing the regression counter,
/// because a new visit's base is the one intended step back and must not be booked as one.
struct PaintModel {
    total: u32,
    base: u32,
    last: u32,
    frames: u32,
    regressions: u32,
}

impl PaintModel {
    const fn new() -> Self {
        Self {
            total: 0,
            base: 0,
            last: 0,
            frames: 0,
            regressions: 0,
        }
    }

    /// Start a visit: the track's length, and the session clock its own timeline starts from.
    fn start(&mut self, total: u32, base: u32) {
        self.total = total;
        self.base = base;
        self.last = 0;
    }

    /// What the display shows for a session clock reading — the ROM's subtraction, saturating.
    fn position(&self, committed: u32) -> u32 {
        paint_position(committed.saturating_sub(self.base), self.total)
    }

    /// One frame's read, with the monotonicity check.
    fn sample(&mut self, committed: u32) {
        let paint = self.position(committed);
        if paint < self.last {
            self.regressions += 1;
        }
        self.last = paint;
        self.frames += 1;
    }
}

/// What a visit's end sends the run to (ALBUM-PLAYER.md D5).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum VisitEnd {
    /// A non-last track's end: the next track was started through the skip's hand-off, minus the
    /// truncate (there was nothing stale to drop). The `Skip` is the wire's own reading of it.
    Advance(Skip),
    /// The last track's end: the album is over and no producer was restarted.
    AlbumEnd,
}

/// The ROM's two halves at period granularity, with the real ring, the real gain and the real state
/// machine. Every field that is a counter exists because a claim reads it.
struct Model {
    feed: RingFeed<RING>,
    transport: Transport,
    source: FakeSource,
    interleaved: Vec<i16>,
    left: Vec<i32>,
    right: Vec<i32>,
    bytes_l: Vec<u8>,
    bytes_r: Vec<u8>,
    frames: usize,
    finished: bool,
    /// The session clock (`position::commit`): incremented by what each hand-off committed.
    committed: u32,
    /// Periods that served silence while the producer still had work — the judge.
    underflows: u32,
    /// Every chunk hand-off, in order: a feeder period or a prime.
    handoffs: u32,
    /// The hand-offs made by the main loop (the boot prime and one per resume).
    primes: u32,
    /// What each hand-off took OUT of the ring, per channel — the ring's own byte stream.
    ring_l: Vec<u8>,
    /// What the DMA was handed: the same bytes after the gain.
    dma_l: Vec<u8>,
    /// The hand-off gain after each hand-off, so the schedule is readable in periods.
    gains: Vec<i32>,
    pauses: Vec<Pause>,
    /// Feeder periods served — the ROM's `handler_entries`, so a skip's stamps are its own arithmetic
    /// rather than a second convention.
    periods: u32,
    /// Every transition whose hand-off has happened, resolved or not — each record carrying its own
    /// wire latch, which is the ROM's per-record watch slot. A record with `open` set is one whose
    /// latch is still armed, so the feeder scans *these*, not one global watch.
    skips: Vec<Skip>,
    /// The most records that were ever armed at once: the ROM's pool is fixed ([`ROM_WATCH_SLOTS`]), so
    /// a sequence that overlaps more transitions than that would be a named refusal on a cart rather
    /// than a silent overwrite. The model keeps them all, so this is the parity check.
    max_open: usize,
    /// Whether a press waits for the wire watch to resolve before the next press is due
    /// (`skip_after_fade`'s `until_wire`). The ROM's frame loop cannot wait: it takes a press as soon
    /// as the transport is `Playing` again, which is `head_silence` periods before the watch resolves,
    /// so `false` is the regime R13's hardware session was in and the fuzz's second pass drives it.
    wait_for_wire: bool,
    /// §4.4's painted position, sampled once per period (§8d).
    paint: PaintModel,
    /// Σ samples every `truncate()` in this run stepped over: the ring's ledger minus the clock.
    dropped: u32,
    /// The model's video clock, in frames: every feeder period spans [`PERIOD_FRAMES`] of them, and
    /// a producer pass is [`Self::pass_periods`] periods. It exists because R13's press sequences
    /// are spaced in video frames and a spacing shorter than a period has to land *inside* one.
    video_frames: u32,
    /// Feeder periods one producer frame costs — a main-loop pass. **2** is the measured value
    /// (1.04–1.09 M cycles = 1.81–1.9 periods, the origin of the old fixed three-period bound); the
    /// R13 calibration test moves it to 2×/3×/5× without touching any product constant.
    pass_periods: u32,
    /// Every feeder period's verdict, counted, so the period census closes:
    /// `handoffs == primes + full_periods + tail_periods + starved_periods`.
    full_periods: u32,
    tail_periods: u32,
    starved_periods: u32,
    /// Σ samples the producer pushed, and Σ the cover pushed: the ring's `pushed = content + cover`.
    content: u32,
    cover: u32,
    /// The run's visits, opened and closed where the ROM opens and closes one (`visits.begin` /
    /// `visits.close`, `album_play_main.rs:3535-3557`), so the fuzz reads a per-visit ledger instead
    /// of inferring one from the clock.
    visits: Vec<VisitMark>,
    /// Whether the last visit is still open: the ROM's `visits.close` is called once per visit, after
    /// its drain or at the hand-off that ends it, and never twice.
    visit_open: bool,
    /// A resume press the ring routed to PR-3c's refill: the head it was taken on (the no-seek witness)
    /// and the passes spent so far. The frame driver carries it across steps because the burst runs one
    /// frame per pass, in the loop's hands rather than the press's; [`Model::press_resume`] does not need
    /// it because it runs the burst in one call.
    resume_open: Option<ResumeTake>,
}

/// The take of a starved resume, as the frame driver holds it between passes.
#[derive(Clone, Copy, Debug)]
struct ResumeTake {
    /// The session clock at the take — the sample the arm must deliver, and the pair's `dropped()` half.
    head: u32,
    /// Producer frames spent so far, for the record's `refill_passes`.
    passes: u32,
}

/// One visit, in the units the ROM's `Visit` records it in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct VisitMark {
    track: usize,
    /// Where this visit's painted timeline starts: the hand-off's clock plus the cover.
    base: u32,
    /// The producer's frontier when the visit closed, in samples pulled for this visit.
    pushed: u32,
    /// The session clock at the close — the hand-off's instant, before the cover.
    end: u32,
}

/// How many video frames one feeder period spans: `PERIOD_CYCLES / VIDEO_PERIOD_CYCLES` =
/// 573,440 / 280,896 = 2.04, floored to **2** so every scripted spacing is an integer frame count.
/// The rounding is the model's and is stated rather than hidden: it makes a spacing of 5 frames
/// land just inside a period plus its boundary, which is the regime the fuzz is for.
const PERIOD_FRAMES: u32 = 2;

impl Model {
    fn new(total: u32) -> Self {
        Self::album(&[total], 0)
    }

    /// A model over an album of `tracks` lengths whose tracks each open with `lead_silent` zeros.
    fn album(tracks: &[u32], lead_silent: u32) -> Self {
        Self {
            feed: RingFeed::new(),
            transport: Transport::new(),
            source: FakeSource::album(tracks, lead_silent),
            interleaved: vec![0i16; 2 * FRAME],
            left: vec![0i32; FRAME],
            right: vec![0i32; FRAME],
            bytes_l: vec![0u8; FRAME],
            bytes_r: vec![0u8; FRAME],
            frames: 0,
            finished: false,
            committed: 0,
            underflows: 0,
            handoffs: 0,
            primes: 0,
            ring_l: Vec::new(),
            dma_l: Vec::new(),
            gains: Vec::new(),
            pauses: Vec::new(),
            periods: 0,
            skips: Vec::new(),
            max_open: 0,
            wait_for_wire: true,
            paint: PaintModel::new(),
            dropped: 0,
            video_frames: 0,
            pass_periods: 2,
            full_periods: 0,
            tail_periods: 0,
            starved_periods: 0,
            content: 0,
            cover: 0,
            visits: Vec::new(),
            visit_open: false,
            resume_open: None,
        }
    }

    /// Open a visit: the track's length and the session clock its own timeline starts from — the
    /// ROM's `paint.start` + `visits.begin`, which it performs together at every hand-off.
    fn open_visit(&mut self, base: u32) {
        self.paint.start(self.source.len(), base);
        self.visits.push(VisitMark {
            track: self.source.track,
            base,
            pushed: 0,
            end: base,
        });
        self.visit_open = true;
    }

    /// Close the open visit at `end`, with the producer's frontier at that instant — the ROM's
    /// `visits.close(producer.pushed as u32, hand_off.committed)`. A second call for the same visit is
    /// a no-op, which is what makes "close it at the stop, whichever stop that was" safe.
    fn close_visit(&mut self, end: u32) {
        if !self.visit_open {
            return;
        }
        if let Some(visit) = self.visits.last_mut() {
            visit.pushed = self.source.at;
            visit.end = end;
        }
        self.visit_open = false;
    }

    /// The boot prefill: fill the ring with no consumer (`Producer::prefill`).
    fn prefill(&mut self) {
        while (self.feed.pushed() as u32) < PREFILL as u32 {
            assert!(self.produce(), "the fake source ended before the prefill");
        }
    }

    /// One producer frame: pull → widen → convert → push in chunk-sized slices, behind the ROM's
    /// own room gate (`has_room` demands a whole frame's worth of free ring, which is the frame size
    /// the stream may not exceed). Returns false at end of stream.
    fn produce(&mut self) -> bool {
        if self.finished {
            return false;
        }
        assert!(
            self.feed.free_samples() >= FRAME,
            "the producer was not gated on room: {} free, a frame needs {FRAME}",
            self.feed.free_samples()
        );
        let n = self
            .source
            .pull_frame(&mut self.interleaved)
            .expect("the fake source has frames");
        if n == 0 {
            self.finished = true;
            return false;
        }
        for i in 0..n {
            self.left[i] = i32::from(self.interleaved[2 * i]);
            self.right[i] = i32::from(self.interleaved[2 * i + 1]);
        }
        convert_frame(
            &self.left[..n],
            &self.right[..n],
            &mut self.bytes_l[..n],
            &mut self.bytes_r[..n],
        )
        .expect("the fake pattern is inside the 16-bit profile");
        let mut offset = 0;
        while offset < n {
            let end = (offset + CHUNK).min(n);
            self.feed
                .push_bytes(&self.bytes_l[offset..end], &self.bytes_r[offset..end])
                .expect("the room gate above guarantees the whole frame");
            offset = end;
        }
        self.frames += 1;
        self.content += n as u32;
        // The ROM sets `finished` in the same critical section as the final push, so the last short
        // chunk is a classified tail rather than an underflow. A track's own end is not the stream's:
        // only the last track's does that, which is what keeps a skip's old track from ending the run.
        if self.source.at == self.source.len() && self.source.track + 1 == self.source.tracks.len()
        {
            self.finished = true;
        }
        true
    }

    /// Hand the ring's head to the DMA through the gain: the boot prime, or a resume's prime.
    fn prime(&mut self) -> bool {
        let (mut l, mut r) = ([0u8; CHUNK], [0u8; CHUNK]);
        let verdict = self.feed.take_chunk(&mut l, &mut r, CHUNK, self.finished);
        let real = match verdict {
            Chunk::Full => CHUNK,
            Chunk::Tail { real } => real,
            Chunk::Starved => return false,
        };
        self.ring_l.extend_from_slice(&l);
        // The deferral note, as the ROM's feeder takes it: it asks about the slot the previous
        // period handed over, which has just finished playing.
        self.transport.note_hand_off();
        self.transport.apply(&mut l, &mut r);
        self.dma_l.extend_from_slice(&l);
        self.committed += real as u32;
        self.handoffs += 1;
        self.primes += 1;
        self.gains.push(self.transport.gain().gain());
        self.ledger();
        true
    }

    /// One feeder period: `take_chunk` into the ping-pong slot, the hand-off gain on that slot, then
    /// the session clock — the ROM's `feeder`, in the same order. The wire watch's two readings and
    /// the paint sample ride along because they are what the period *did*, not a second run of it.
    fn period(&mut self) -> Chunk {
        assert_ne!(
            self.transport.state(),
            State::Paused,
            "a period was served with the chain frozen"
        );
        assert_ne!(
            self.transport.state(),
            State::Buffering,
            "a period was served while a starved resume was refilling: the chain is frozen in that state, so it has no audio clock to serve"
        );
        // Read *before* the gain spends this chunk's cover count, exactly as the ROM's feeder does: a
        // countdown of one means this is the cover's last chunk, so the next period is the new
        // track's own first. `content_at == 0` is the per-record "waiting for its content point"
        // sentinel: only an armed record that has not been stamped asks, and at most one hand-off's
        // cover can be counting at a time (a second one needs the ramp back, which spends it).
        let last_cover = self
            .skips
            .iter()
            .any(|skip| skip.open && skip.content_at == 0)
            && self.transport.gain().held_chunks() == 1;
        let (mut l, mut r) = ([0u8; CHUNK], [0u8; CHUNK]);
        let verdict = self.feed.take_chunk(&mut l, &mut r, CHUNK, self.finished);
        self.ring_l.extend_from_slice(&l);
        self.transport.note_hand_off();
        self.transport.apply(&mut l, &mut r);
        self.dma_l.extend_from_slice(&l);
        self.handoffs += 1;
        self.gains.push(self.transport.gain().gain());
        match verdict {
            Chunk::Full => {
                self.full_periods += 1;
                self.committed += CHUNK as u32;
            }
            Chunk::Tail { real } => {
                self.tail_periods += 1;
                self.committed += real as u32;
            }
            Chunk::Starved => {
                self.starved_periods += 1;
                if !self.finished {
                    self.underflows += 1;
                }
            }
        }
        self.periods += 1;
        self.video_frames += PERIOD_FRAMES;
        // The wire, after the gain: `0x00` in every byte is the FIFO's own silence verdict, and
        // `take_chunk` zero-fills what a `Starved`/`Tail` chunk did not fill.
        let silent = l.iter().all(|&b| b == 0) && r.iter().all(|&b| b == 0);
        self.watch_update(last_cover, silent);
        self.paint.sample(self.committed);
        self.ledger();
        verdict
    }

    /// One reading of the ROM's wire watch (`SkipRecord`'s two stamps) for **every** armed record,
    /// then close each record whose own new track has reached the wire.
    ///
    /// Every armed record is scanned, not just the newest: that is the whole of the fix. A record's
    /// stamps are its own, so a later hand-off's latch has nothing to overwrite.
    fn watch_update(&mut self, last_cover: bool, silent: bool) {
        let now = self.periods;
        let underflows = self.underflows;
        for skip in self.skips.iter_mut().filter(|skip| skip.open) {
            if last_cover && skip.content_at == 0 {
                // The entry after the one that spent the last cover count: the ROM computes the same
                // index from `handler_entries + 2` inside a feeder entry whose counter has not yet
                // been incremented, and both are the *schedule's* content point, not the wire's.
                skip.content_at = now + 1;
            }
            if skip.new_at == 0 {
                if skip.silent_at == 0 {
                    if silent {
                        skip.silent_at = now;
                    }
                } else if !silent {
                    skip.new_at = now;
                }
            }
            skip.underflows = underflows;
            if skip.new_at != 0 {
                skip.open = false;
            }
        }
    }

    /// The invariant the whole design rests on: the session clock is the ring's own ledger, not a
    /// parallel computation.
    ///
    /// With a truncate in the run the ledger is `consumed − dropped`, not `consumed`: `truncate()`
    /// steps the ring's consumer over the stale span in one assignment, so it is never handed to the
    /// DMA and never reaches the clock. The ROM prints the same three figures (`committed`, `dropped`,
    /// `ledger`) for the same reason.
    fn ledger(&self) {
        assert_eq!(
            self.committed,
            self.feed.consumed() as u32 - self.dropped,
            "the session clock and the ring ledger parted"
        );
    }

    fn run_periods(&mut self, n: u32) {
        for _ in 0..n {
            self.period();
        }
    }

    /// The main loop's frame slice for a pause press: the toggle, then feeder periods until the
    /// transport says the gain has reached silence, then the freeze.
    fn press_pause(&mut self) -> Pause {
        let primable = self.ring_primable();
        assert_eq!(
            self.transport.toggle(true, primable),
            Step::FadeOut,
            "a pause from Playing must fade out"
        );
        let committed_at_toggle = self.committed;
        self.fade_to_freeze(committed_at_toggle)
    }

    /// The feeder periods after a pause's `toggle` returned [`Step::FadeOut`]: serve until the gain
    /// has reached silence and the chunk carrying the ramp has been played out, then record the
    /// freeze. Split from [`Self::press_pause`] so the fuzz can take the transport's own verdict on
    /// a press and then run the same continuation.
    fn fade_to_freeze(&mut self, committed_at_toggle: u32) -> Pause {
        // The same expression the ROM gates its pause rows against, from this model's own pass
        // parameter — so no pause test in this file runs against a cap copied out of an emulator. The
        // loop here takes its turn every period, which is the shortest pass a machine can have; the
        // case where a pass spans several periods is `a_pause_freezes_inside_the_bound_its_own_pass_
        // length_supports`, which drives `tick()` at that cadence instead.
        let bound = pause_to_freeze_bound(self.pass_periods);
        let mut fade_periods = 0;
        loop {
            self.period();
            fade_periods += 1;
            match self.transport.tick() {
                Step::Freeze => break,
                Step::Idle => assert!(
                    fade_periods <= bound,
                    "the fade never reached silence: {} periods against a bound of {} for a {}-period pass",
                    fade_periods,
                    bound,
                    self.pass_periods
                ),
                other => panic!("unexpected step {other:?}"),
            }
        }
        let pause = Pause {
            committed_at_toggle,
            frozen_committed: self.committed,
            gain_at_freeze: self.transport.gain().gain(),
            fade_periods,
            resumed_from: 0,
            frozen_frames: 0,
            refilled: false,
            refill_passes: 0,
        };
        assert!(
            pause.fade_periods <= bound,
            "the freeze took {} feeder periods; this model's bound is {} (a {}-period pass)",
            pause.fade_periods,
            bound,
            self.pass_periods
        );
        self.pauses.push(pause);
        self.pauses.last().cloned().expect("just pushed")
    }

    /// The ROM's `ring_primable`, asked in the ROM's order — the ring first, then the state machine,
    /// because PR-3c routes a resume by the answer (Q18's ruling: a product must not stop).
    ///
    /// The two clauses are `take_chunk`'s own two answers: a ring below `CHUNK` hands a feeder
    /// `Chunk::Starved`, and a finished stream's short tail is a `Chunk::Tail` at any `ready() > 0` —
    /// the same distinction the ROM's `finished` argument selects. `resume_never_routes_a_ring_that_a_prime_would_take`
    /// sweeps `ready()` and compares this to what `take_chunk` actually answers, so the model's
    /// classification cannot drift from the ring's.
    fn ring_primable(&self) -> bool {
        self.feed.ready() >= CHUNK || (self.finished && self.feed.ready() > 0)
    }

    /// The main loop's frame slice for a resume press: the toggle is **routed by the ring** (PR-3c), and
    /// both routes end at the same arm — the prime of the ring's head and the re-arm.
    fn press_resume(&mut self, paused_frames: u32) {
        assert_eq!(self.transport.state(), State::Paused);
        let from = self.committed;
        let primable = self.ring_primable();
        match self.transport.toggle(true, primable) {
            Step::Prime => self.prime_resume(from, paused_frames),
            Step::Buffer => {
                // The ring cannot give the prime's chunk. §4.4's no-seek invariant with a burst inside
                // the pause is these three assertions: the frozen chain serves no period, hands nothing
                // to the DMA, and its clock does not move — so the arm below takes the very head the
                // freeze stopped on, and `dropped()` stays zero for a reason stronger than arithmetic.
                let periods = self.periods;
                let handoffs = self.handoffs;
                let passes = self.refill_to_target();
                assert_eq!(
                    self.periods, periods,
                    "a starved resume served a period: the chain was not frozen"
                );
                assert_eq!(
                    self.handoffs, handoffs,
                    "a starved resume handed a chunk to the DMA"
                );
                assert_eq!(
                    self.committed, from,
                    "a refill moved the session clock — that is a seek, and §4.4 forbids it"
                );
                let last = self.pauses.last_mut().expect("a resume follows a pause");
                last.refilled = true;
                last.refill_passes = passes;
                self.prime_resume(from, paused_frames);
            }
            other => panic!("a resume toggle answered {other:?}"),
        }
    }

    /// PR-3c's burst at the loop's cadence: **one producer frame per main-loop pass**, no periods, until
    /// the ring holds `PREFILL` again — the boot prefill's own target and §4.2b's figure, which is the
    /// whole point (no second, smaller number to argue about). Returns the passes it took.
    ///
    /// `end_buffer` is asked on every pass, which is what the transport's own doc allows ("the caller
    /// asks when the ring reaches the target and may ask on every pass afterwards"): the caller owns the
    /// ring's depth, the state machine owns the state and the ramp. The video clock moves by a pass's
    /// wall time — the decode is real work and takes the same time whether or not any period ran — which
    /// is what makes the frame leg in `the_refill_awaits_no_more_than_the_run_own_pass_price_allows`
    /// a price rather than a count of frames nobody spent.
    fn refill_to_target(&mut self) -> u32 {
        assert_eq!(self.transport.state(), State::Buffering);
        assert!(
            !self.finished,
            "a refill was entered with the stream finished, so its target can never be reached — which is exactly why a finished visit's short tail takes the direct route"
        );
        let mut passes = 0;
        loop {
            if self.feed.ready() >= PREFILL {
                assert_eq!(
                    self.transport.end_buffer(),
                    Step::Prime,
                    "the refill reached its target and did not ask for the prime"
                );
                assert_eq!(
                    self.transport.state(),
                    State::Resuming,
                    "the arm must be asked for, never assumed"
                );
                return passes;
            }
            assert!(
                passes <= geometry::PREFILL_FRAMES,
                "the refill has spent {passes} frames and is at {}; the boot prefill reaches the same target in {}",
                self.feed.ready(),
                geometry::PREFILL_FRAMES
            );
            assert!(
                self.produce(),
                "a refill's producer stopped before its target: the stream ended without the ring reaching the prefill"
            );
            passes += 1;
            self.video_frames += self.pass_periods * PERIOD_FRAMES;
        }
    }

    /// The prime after a resume's `toggle` returned [`Step::Prime`]: the primed chunk is the ramp's
    /// first chunk, and 256 < 560 completes the ramp inside it.
    fn prime_resume(&mut self, from: u32, paused_frames: u32) {
        assert!(
            self.prime(),
            "the ring's head must supply the prime's chunk"
        );
        assert_eq!(self.transport.tick(), Step::Idle);
        assert_eq!(
            self.transport.state(),
            State::Playing,
            "the ramp did not complete in the primed chunk"
        );
        let last = self.pauses.last_mut().expect("a resume follows a pause");
        last.resumed_from = from;
        last.frozen_frames = paused_frames;
    }

    /// The ROM's drain: whole periods until the ring is empty, bounded so a wedge fails here rather
    /// than in a harness timeout.
    fn drain(&mut self) {
        let mut polls = 0;
        while self.feed.ready() > 0 {
            self.period();
            polls += 1;
            assert!(polls < RING + 8, "the drain never finished");
        }
    }

    /// Run the producer to the end of the track, serving a period whenever the ring has no room —
    /// §4.5's loop with the video clock taken out.
    fn produce_to_end(&mut self) {
        while !self.finished {
            if self.feed.free_samples() >= FRAME {
                self.produce();
            } else {
                self.period();
            }
        }
    }

    // ---------------------------------------------------------------------------------------------
    // 8d. §4.4's skip: the hand-off, measured from the wire
    // ---------------------------------------------------------------------------------------------

    /// Push one whole chunk of zeros — §4.4 step 4's cover, and the ROM's `push_cover_chunk`.
    ///
    /// **Whole chunks, always.** A partial push left in the ring while the producer runs can never be
    /// taken (`take_chunk` takes whole chunks only) and starves the DMA forever, so the length is
    /// asserted here rather than trusted: this is the model's copy of the rule, and the assertion is
    /// what would fail if the cover ever became `prime × chunk` in aggregate instead of per push.
    fn push_cover(&mut self) {
        let before = self.feed.pushed();
        let zeros = [0u8; CHUNK];
        assert!(
            self.feed.push_bytes(&zeros, &zeros).is_ok(),
            "the cover's chunk did not fit: {} free",
            self.feed.free_samples()
        );
        assert_eq!(
            self.feed.pushed() - before,
            CHUNK,
            "a cover push was not one whole chunk"
        );
        assert_eq!(
            self.feed.ready() % CHUNK,
            0,
            "the ring holds a partial chunk after a cover push"
        );
        self.cover += CHUNK as u32;
    }

    /// One direction press and the loop's frame slice around it, as the ROM performs it.
    ///
    /// `pass_periods` is how many feeder periods the pass's own producer decode takes — a pass is one
    /// producer frame of decode, so this is the quantity `take_to_handoff_bound` takes. The
    /// press is taken at the top of the pass, the fade is spent by the feeder inside it, and the
    /// hand-off runs at the decision at its end; `pass_periods` therefore *is* the press→hand-off
    /// wait when the pass is long enough, which is what makes the bound checkable here rather than
    /// only in a ROM log.
    ///
    /// The deferral ([`FADE_PERIODS`]) is the floor: a decision taken one period after the press
    /// cannot hand off, because the buffer the ramp was applied to has not yet been played out. The
    /// loop therefore serves one more period and asks again, exactly as the ROM's cheap passes do
    /// after the decode — that extra period is the deferral, and it is part of the measured wait.
    ///
    /// Returns the resolved skip, so a caller asserts on numbers rather than on side effects.
    fn press_skip(&mut self, to: usize, primed_chunks: u32, pass_periods: u32) -> Skip {
        assert!(pass_periods >= 1, "a pass with no decode cannot fade");
        assert_eq!(
            self.transport.skip(),
            Step::FadeOut,
            "a skip from Playing must fade out"
        );
        self.skip_after_fade(to, primed_chunks, pass_periods, true)
    }

    /// A skip's fade, hand-off and settle, for a press whose `skip()` call has already returned
    /// [`Step::FadeOut`] — split from [`Self::press_skip`] so a caller can take the transport's own
    /// verdict on a press and then run the same continuation.
    ///
    /// `until_wire` says where the transition ends. `true` is a press at a human cadence: the model
    /// waits for the wire watch to resolve, which is what makes every number it reports a reading
    /// rather than a prediction. `false` stops as soon as the transport is [`State::Playing`] again
    /// — where the ROM's frame loop is free to take the *next* press — and that is the window R13's
    /// second regression test drives, because a track whose own head is digital silence keeps the
    /// wire watch open for `head_silence` periods after the ramp back has completed.
    fn skip_after_fade(
        &mut self,
        to: usize,
        primed_chunks: u32,
        pass_periods: u32,
        until_wire: bool,
    ) -> Skip {
        let from = self.source.track;
        let press_at = self.periods;
        let mut served = 0;
        for _ in 0..pass_periods {
            self.period();
            served += 1;
        }
        loop {
            match self.transport.tick() {
                Step::SkipHandOff => break,
                Step::Idle => {
                    assert!(
                        served < take_to_handoff_bound(pass_periods),
                        "the deferral never let the hand-off happen"
                    );
                    self.period();
                    served += 1;
                }
                other => panic!("unexpected step {other:?} while fading out"),
            }
        }

        // §4.4 steps 2-5's arming, in one section: the transport's transition (which arms the cover
        // hold), the truncate, and the cover's first whole chunk. A feeder period landing between the
        // truncate and that chunk would be `Starved` and would book an underflow from the window
        // alone — the probe's `truncate_stale` — which is why the model performs them together and
        // `the_truncate_and_the_covers_first_chunk_share_one_section` also shows the bare shape fail.
        assert!(
            self.transport.begin_reprime(primed_chunks),
            "the transport was not in its fade"
        );
        let queued = self.feed.ready() as u32;
        let dropped = self.truncate();
        let committed = self.committed;
        // `transition_hand_off` clears `finished` in this same section (`album_play_main.rs:1737`): it
        // marked the *old* visit's last push, and leaving it set would classify the new track's first
        // starved period as a `Tail` and hide it from the underflow count.
        self.finished = false;
        self.close_visit(committed);
        self.push_cover();
        for _ in 1..primed_chunks {
            self.push_cover();
        }
        // Nothing stale survives the truncate and nothing but whole cover chunks was left behind at its
        // end: this is the same-critical-section rule, read off the ring instead of argued.
        assert_eq!(
            self.feed.ready(),
            primed_chunks as usize * CHUNK,
            "the hand-off left something in the ring besides the cover"
        );

        // The mark is taken **here**, inside the hand-off's section — the same instant the ROM stamps
        // `watch_at` — so the fade-out and the stale-but-silent periods it cost are behind it. The
        // record this arms is the only one these stamps belong to: the latch is per record.
        let at = self.periods;
        let index = self.skips.len();
        self.skips.push(Skip {
            from,
            to,
            at,
            silent_at: 0,
            new_at: 0,
            content_at: 0,
            underflows_at: self.underflows,
            underflows: self.underflows,
            dropped,
            queued,
            press_at,
            open: true,
        });
        self.max_open = self
            .max_open
            .max(self.skips.iter().filter(|skip| skip.open).count());

        // Step 3: the seam call, spent behind the cover, and the new visit's timeline starts after it.
        self.source
            .start_track(to)
            .expect("the skip's target track exists");
        let base = committed + primed_chunks * CHUNK as u32;
        self.open_visit(base);

        self.settle(if until_wire { Some(index) } else { None });
        self.skips[index]
    }

    /// Run a transition's tail: produce while a whole frame fits, serve a period, then take the
    /// pass's transport decision.
    ///
    /// A re-prime that is not fed would starve and book an underflow that belongs to the model, and a
    /// transport left in `Repriming` would refuse the next press — so the loop always feeds.
    /// `wait_for` is the record whose own latch the caller must see resolved (`None` for a caller that
    /// only needs the transport playing again, which is where the ROM's frame loop can take a press).
    fn settle(&mut self, wait_for: Option<usize>) {
        while wait_for.is_some_and(|index| self.skips[index].open)
            || self.transport.state() != State::Playing
        {
            if self.feed.free_samples() >= FRAME {
                assert!(self.produce(), "the new track ended inside the transition");
            } else {
                self.period();
            }
            assert_eq!(
                self.transport.tick(),
                Step::Idle,
                "an unexpected step while the ramp back was completing"
            );
        }
    }

    /// `RingFeed::truncate()`, with the run's own Σ of what it stepped over: the term the ledger and the
    /// ROM's `dropped=` figure both read.
    fn truncate(&mut self) -> u32 {
        let dropped = self.feed.truncate() as u32;
        self.dropped += dropped;
        dropped
    }

    /// Fill the ring until a whole frame no longer fits — the deep queue a skip's truncate is for.
    fn fill_deep(&mut self) {
        while self.feed.free_samples() >= FRAME {
            assert!(self.produce(), "the source ended before the ring filled");
        }
    }

    // ---------------------------------------------------------------------------------------------
    // 8e. A track's end, and the album's (PR-3b-iii-A, ALBUM-PLAYER.md D5)
    // ---------------------------------------------------------------------------------------------

    /// Play this visit to its end and let the ring drain — the ROM's frame loop at a visit's end.
    ///
    /// The producer stops at its own last frame; the feeder then hands the queued span over. The
    /// visit's `finished` is set here because the ROM sets it in the last push's critical section:
    /// a starved period inside the end-of-track fade is therefore a silence, not an underflow.
    fn play_visit_out(&mut self) {
        while self.source.at < self.source.len() {
            if self.feed.free_samples() >= FRAME {
                assert!(self.produce(), "the fake source ended mid-visit");
            } else {
                self.period();
            }
        }
        self.finished = true;
        while self.feed.ready() > 0 {
            self.period();
        }
        assert_eq!(self.feed.ready(), 0, "the visit's own ring did not drain");
    }

    /// The main loop's end-of-visit decision — `album_play_main.rs`'s drained check.
    ///
    /// A non-last track posts the advance through the same transport entry a press uses, then the
    /// same hand-off a skip performs **minus the truncate**: the ring is drained, so there is
    /// nothing stale to drop and `dropped` is zero. The last track posts `AlbumEnd` and starts
    /// nothing — no wrap, because §1 is start-to-finish and §5.3's wrap is the buttons'.
    fn end_of_visit(&mut self) -> VisitEnd {
        assert_eq!(self.source.at, self.source.len(), "the visit is not over");
        assert_eq!(self.feed.ready(), 0, "the visit has not drained");
        let from = self.source.track;
        let next = from + 1;
        if next >= self.source.tracks.len() {
            // The ROM closes the visit after its drain, before the verdict — the album's last visit
            // is no exception (`album_play_main.rs:3696`).
            self.close_visit(self.committed);
            return VisitEnd::AlbumEnd;
        }

        let press_at = self.periods;
        assert_eq!(
            self.transport.skip(),
            Step::FadeOut,
            "a track end must enter the transition from Playing"
        );
        // The fade has no bytes to act on (the ring is empty), so its periods are silence; `finished`
        // is still set, so neither is booked as an underflow. The deferral applies here too — the
        // advance reuses `skip()`/`SkipHandOff`, one mechanism and not a fork — so the transition
        // takes `FADE_PERIODS` periods from the press to the hand-off, exactly as a press's does.
        let mut fade_periods = 0;
        loop {
            self.period();
            fade_periods += 1;
            match self.transport.tick() {
                Step::SkipHandOff => break,
                Step::Idle => assert!(
                    fade_periods < FADE_PERIODS,
                    "the end-of-track fade never settled"
                ),
                other => panic!("unexpected step {other:?} at a track end"),
            }
        }
        assert_eq!(fade_periods, FADE_PERIODS);
        assert!(
            self.transport.begin_reprime(SKIP_PRIME_CHUNKS),
            "the transport was not in its transition"
        );
        let committed = self.committed;
        self.close_visit(committed);
        self.push_cover();
        for _ in 1..SKIP_PRIME_CHUNKS {
            self.push_cover();
        }
        // The watch is the skip ledger's, reused here to answer the one question a track end
        // raises about the wire: is anything of the old track heard after the hand-off? Its latch is
        // this record's own, on the same rule as a press's (see [`Skip`]).
        let at = self.periods;
        let index = self.skips.len();
        self.skips.push(Skip {
            from,
            to: next,
            at,
            silent_at: 0,
            new_at: 0,
            content_at: 0,
            underflows_at: self.underflows,
            underflows: self.underflows,
            dropped: 0,
            queued: 0,
            press_at,
            open: true,
        });
        self.max_open = self
            .max_open
            .max(self.skips.iter().filter(|skip| skip.open).count());
        self.finished = false;
        self.source
            .start_track(next)
            .expect("the advance's target track exists");
        let base = committed + SKIP_PRIME_CHUNKS * CHUNK as u32;
        self.open_visit(base);

        self.settle(Some(index));
        VisitEnd::Advance(self.skips[index])
    }
}

// ---------------------------------------------------------------------------------------------
// 8a. The format, and the ramp's shape
// ---------------------------------------------------------------------------------------------

#[test]
fn silence_at_the_hand_off_is_the_centre_zero_not_a_dc_offset() {
    // The format, settled before the ramp was written: `fifo_byte` is `>> 8` to *signed* 8-bit, so
    // the centre is 0x00 and 0x80 is full-scale negative. The probe pushed all-zero chunks and the
    // wire watch called them silent, which agrees — the FIFO is not offset-binary.
    assert_eq!(fifo_byte(0), 0x00);
    assert_eq!(fifo_byte(-32_768), 0x80);
    assert_eq!(fifo_byte(32_767), 0x7F);
    assert_eq!(scale_byte(0x00, GAIN_UNITY), 0x00);
    assert_eq!(scale_byte(0x7F, GAIN_UNITY), 0x7F);
    assert_eq!(scale_byte(0x80, GAIN_UNITY), 0x80);
    // A full-scale alternating pattern ramped to k = 0 is 0x00 in every byte — the centre — and
    // never 0x80, which would be a jump to a rail: exactly the pop the fade exists to avoid.
    let mut left: Vec<u8> = (0..FADE_SAMPLES as usize * 2)
        .map(|i| if i % 2 == 0 { 0x7F } else { 0x80 })
        .collect();
    let mut right = left.clone();
    let mut gain = HandOffGain::full();
    gain.fade_out();
    gain.apply(&mut left, &mut right);
    assert!(
        gain.is_silent(),
        "a 512-sample chunk must complete the ramp"
    );
    assert!(
        left[FADE_SAMPLES as usize..].iter().all(|&b| b == 0x00),
        "the tail of the fade is not 0x00: {:?}",
        &left[FADE_SAMPLES as usize..]
    );
    assert_eq!(left, right, "the two channels must fade together");
    // The last ramped step lands one code from the centre on each side, so the ramp is continuous
    // into silence: k = 1 scales -128 to -1 (0xFF) and +127 to 0 — a DC-offset reading would
    // instead converge on 0x80.
    assert_eq!(scale_byte(0x80, 1), 0xFF);
    assert_eq!(scale_byte(0x7F, 1), 0x00);
    assert_eq!(scale_byte(0x80, 0), 0x00);
    assert_eq!(scale_byte(0x7F, 0), 0x00);
}

#[test]
fn the_hand_off_fade_is_monotone_and_completes_inside_one_chunk() {
    // 256 samples < a 560-sample chunk: a fade finishes inside the buffer that starts playing, which
    // is why the transport can defer the freeze to the boundary after it instead of stopping mid-ramp.
    assert!((FADE_SAMPLES as usize) < CHUNK);
    // A constant positive byte makes the scaled value a monotone function of k, so the byte stream
    // itself witnesses the ramp's shape.
    let mut left = vec![0x7F; CHUNK];
    let mut right = vec![0x7F; CHUNK];
    let mut gain = HandOffGain::full();
    assert!(gain.is_transparent());

    gain.fade_out();
    gain.apply(&mut left, &mut right);
    assert_eq!(left[0], 0x7F, "the first sample is not scaled by full gain");
    for i in 1..CHUNK {
        assert!(
            left[i] <= left[i - 1],
            "fade-out went up at {i}: {} -> {}",
            left[i - 1],
            left[i]
        );
    }
    assert!(
        left[FADE_SAMPLES as usize..].iter().all(|&b| b == 0),
        "the ramp did not reach zero inside one chunk"
    );
    assert!(gain.is_silent());
    assert_eq!(gain.gain(), 0);

    // …and back up, from the centre, equally monotone and complete.
    gain.fade_in();
    let mut up = vec![0x00; CHUNK];
    let mut up_r = vec![0x00; CHUNK];
    // Scale a constant full-scale source by the rising gain: keep the source at 0x7F in the slots,
    // which is what `take_chunk` would have copied.
    up.fill(0x7F);
    up_r.fill(0x7F);
    gain.apply(&mut up, &mut up_r);
    for i in 1..CHUNK {
        assert!(
            up[i] >= up[i - 1],
            "fade-in went down at {i}: {} -> {}",
            up[i - 1],
            up[i]
        );
    }
    assert!(gain.is_full());
    assert_eq!(up[FADE_SAMPLES as usize - 1], 0x7F - 1);
    assert!(up[FADE_SAMPLES as usize..].iter().all(|&b| b == 0x7F));

    // A held gain is a constant scale, and a transparent one writes nothing.
    let mut held_l = vec![0x40; 4];
    let mut held_r = vec![0x40; 4];
    let mut half = HandOffGain::full();
    half.fade_out();
    let mut scratch_l = vec![0x40; 256];
    let mut scratch_r = vec![0x40; 256];
    half.apply(&mut scratch_l, &mut scratch_r);
    assert_eq!(half.gain(), 0);
    // From silent with no step, a chunk is zeroed silence rather than skipped.
    half.hold();
    half.apply(&mut held_l, &mut held_r);
    assert_eq!(held_l, vec![0x00; 4]);
    assert_eq!(held_r, vec![0x00; 4]);
}

/// Silence at the hand-off is the **centre**, and the fade window fits inside one DMA buffer.
///
/// The one thing most likely to be got wrong here is the sample format, and it is a click either
/// way: `fifo_byte` is `>> 8` to signed 8-bit, so `0x00` is silence and `0x80` is full-scale
/// *negative*. The probe pushed all-zero chunks and the wire watch called them silent, which agrees.
/// A fade to `k = 0` must therefore land on `0x00`; `0x80` would be a jump to a rail, i.e. the pop
/// §4.4 forbids.
#[test]
fn the_hand_off_gain_schedule_is_monotone_to_zero_and_transparent_at_full() {
    assert_eq!(FADE_SAMPLES, 256);
    // 256 < 560: the ramp completes inside the buffer that starts playing, which is why a pause can
    // freeze the chain at the next period boundary instead of stopping mid-waveform.
    assert!((FADE_SAMPLES as usize) < CHUNK);
    // 256 samples at 16,384 Hz = 15.6 ms.
    assert_eq!(FADE_SAMPLES as u32 * 1_000 / geometry::RATE_HZ, 15);

    let mut model = Model::new(TOTAL);
    model.prefill();
    assert!(model.prime());
    model.run_periods(4);
    // Steady state: every hand-off so far was byte-for-byte the ring's own data.
    assert_eq!(
        model.dma_l, model.ring_l,
        "an unfaded hand-off changed a byte"
    );

    let pause_at = model.gains.len();
    let pause = model.press_pause();
    assert_eq!(
        pause.fade_periods, FADE_PERIODS,
        "the fade plus its play-out period did not fit in {FADE_PERIODS} periods"
    );
    assert_eq!(
        pause.gain_at_freeze, 0,
        "the chain froze at a non-zero gain"
    );
    // Read as a schedule in periods: full scale right up to the pause, silent from the fade's own
    // period on. (The ramp *inside* that period is the per-sample check below.)
    assert!(model.gains[..pause_at].iter().all(|&g| g == 256));
    assert!(
        model.gains[pause_at..].iter().all(|&g| g == 0),
        "the gain did not stay at zero after the fade: {:?}",
        &model.gains[pause_at..]
    );

    // The fade chunk, sample by sample: the ring's byte scaled by that sample's gain, one step down
    // from full scale. This is the whole mechanism — the gain is applied at the hand-off, and the
    // ring's own bytes are what the unscaled capture shows beside it. It is the second-to-last
    // hand-off: the last one is the play-out period, served at gain zero so the ramped buffer can
    // finish playing before the freeze.
    let faded =
        &model.dma_l[model.dma_l.len() - FADE_PERIODS as usize * CHUNK..model.dma_l.len() - CHUNK];
    let ring = &model.ring_l
        [model.ring_l.len() - FADE_PERIODS as usize * CHUNK..model.ring_l.len() - CHUNK];
    for i in 0..CHUNK {
        let k = (FADE_SAMPLES - i as i32).clamp(0, 256);
        assert_eq!(
            faded[i],
            scale_byte(ring[i], k),
            "sample {i} of the fade is not the ring's byte at gain {k}"
        );
    }
    assert!(
        faded[FADE_SAMPLES as usize..].iter().all(|&b| b == 0x00),
        "the fade's tail is not the centre byte"
    );
    assert!(
        ring[FADE_SAMPLES as usize..].iter().any(|&b| b != 0x00),
        "the ring's own bytes there were silent too, so the fade proves nothing"
    );
    // The play-out period is the deferral: it is served at gain zero, so it is all centre bytes.
    // Cutting the chain here loses nothing audible, which is the whole reason `tick` waits for it.
    assert_eq!(
        &model.dma_l[model.dma_l.len() - CHUNK..],
        &vec![0x00u8; CHUNK][..],
        "the chunk served while the gain was silent is not silence"
    );

    // Resume: the ramp starts at the centre and reaches full scale inside the primed chunk, and the
    // periods after it stay at full scale — the schedule moves one way in each direction.
    let resume_at = model.gains.len();
    model.press_resume(0);
    assert_eq!(model.gains[resume_at], 256);
    let primed = &model.dma_l[model.dma_l.len() - CHUNK..];
    let ring = &model.ring_l[model.ring_l.len() - CHUNK..];
    for i in 0..CHUNK {
        let k = (i as i32).clamp(0, 256);
        assert_eq!(
            primed[i],
            scale_byte(ring[i], k),
            "sample {i} of the ramp-in is not the ring's byte at gain {k}"
        );
    }
    // …and the periods after it are transparent again: the gain is a transition, not a stage.
    let before = model.dma_l.len();
    model.run_periods(2);
    assert_eq!(
        model.dma_l[before..],
        model.ring_l[model.ring_l.len() - 2 * CHUNK..],
        "a period after the ramp-in was still scaled"
    );
}

// ---------------------------------------------------------------------------------------------
// 8b. Pause: what it touches, and what it leaves alone
// ---------------------------------------------------------------------------------------------

/// **Item 1's regression test: the transition is deferred until the ramped chunk has been played.**
///
/// The ramp is applied to the slot handed to the DMA, and that slot *begins* playing at the
/// `rearm_dma` that hands it over. So a `tick` that fires on `gain.is_silent()` alone freezes the
/// chain at sample 0 of the chunk that carries the fade: the listener hears the previous chunk end
/// at unity gain, the 15.6 ms ramp is never heard, and up to a chunk is cut mid-waveform — the
/// splice §4.4 forbids. This test drives the transition one period at a time and asserts the first
/// decision is `Idle` (the buffer was handed over but not yet played), and that the freeze only
/// happens after the next hand-off, whose chunk is silence in every byte. Remove the deferral and
/// the first `tick` returns `Freeze`: this test goes red.
#[test]
fn the_deferral_serves_the_ramped_chunk_before_the_freeze_so_the_fade_reaches_the_speaker() {
    let mut model = Model::new(TOTAL);
    model.prefill();
    assert!(model.prime());
    model.run_periods(8);

    let before = model.dma_l.len();
    let consumed_before = model.feed.consumed();
    let primable = model.ring_primable();
    assert_eq!(
        model.transport.toggle(true, primable),
        Step::FadeOut,
        "a pause from Playing must fade out"
    );

    // Period 1: the ramp is applied to the slot the DMA is about to start playing. The gain is
    // silent at the end of it, but the chunk carrying the silence has only just begun.
    model.period();
    assert!(model.transport.gain().is_silent());
    assert!(
        !model.transport.served_silent_tail(),
        "the feeder claimed the silent tail was served on the ramp's own period"
    );
    assert_eq!(
        model.transport.tick(),
        Step::Idle,
        "the pause froze on the ramp's own period: the fade would never reach the speaker"
    );
    assert_eq!(model.transport.state(), State::Pausing);

    // Period 2: served at gain zero, i.e. handed over at the boundary where the ramped buffer has
    // finished playing. Only now may the chain freeze.
    model.period();
    assert!(model.transport.served_silent_tail());
    assert_eq!(model.transport.tick(), Step::Freeze);
    assert_eq!(model.transport.state(), State::Paused);
    assert_eq!(
        (model.dma_l.len() - before) / CHUNK,
        FADE_PERIODS as usize,
        "the pause did not serve exactly the fade plus its play-out period"
    );

    // The chunk the ramp was applied to: unity gain at its first sample, the declared one-step-per-
    // sample window down to the centre, then the centre for the rest of the buffer.
    let ramped = &model.dma_l[before..before + CHUNK];
    let source = &model.ring_l[before..before + CHUNK];
    for i in 0..CHUNK {
        let k = (FADE_SAMPLES - i as i32).clamp(0, 256);
        assert_eq!(
            ramped[i],
            scale_byte(source[i], k),
            "sample {i} of the ramped chunk is not the ring's byte at gain {k}"
        );
    }
    assert_eq!(ramped[0], source[0], "the fade did not start at full gain");
    assert!(
        ramped[FADE_SAMPLES as usize..].iter().all(|&b| b == 0),
        "the ramped chunk's tail is not silence, so the ramp did not complete"
    );
    assert!(
        source[FADE_SAMPLES as usize..].iter().any(|&b| b != 0),
        "the ring's bytes in the ramp's tail were silent, so this proves nothing"
    );
    // The chunk the freeze lands on: silence in every byte, so cutting it loses no waveform. This
    // is the absorption of the deferral's one period.
    let silent = &model.dma_l[before + CHUNK..before + FADE_PERIODS as usize * CHUNK];
    assert!(
        silent.iter().all(|&b| b == 0),
        "the period served while the gain was silent is not silence"
    );

    // The ring moved by exactly those two periods and the producer was not fed: a pause takes
    // chunks, never drops or repeats one, and the ledger closes across the freeze.
    let served = FADE_PERIODS as usize * CHUNK;
    assert_eq!(model.feed.consumed(), consumed_before + served);
    assert_eq!(
        model.feed.pushed(),
        model.feed.consumed() + model.feed.ready()
    );
    assert_eq!(model.committed, model.feed.consumed() as u32);
    assert_eq!(model.underflows, 0);
}

/// Across a pause/resume the **post-gain** byte stream handed to `rearm_dma` is the source's own
/// stream, contiguous, with the only differences inside the declared ramp windows and the play-out
/// period the freeze cuts.
///
/// This is the transport witness the ROM cannot be. The ROM's `pcm`/`fifo`/`assets` folds are a
/// cold re-decode and a region fold — a **decoder/content** witness, invariant to what the ring and
/// the DMA did — and it has no playback cursor to read. Here every byte the feeder hands over after
/// the gain is recorded, so a dropped, repeated or reordered chunk is a byte difference outside the
/// named windows rather than an inference. What it still cannot witness is the DAC and the speaker
/// (AP-7), or the real period IRQ's timing.
#[test]
fn the_handed_over_stream_is_contiguous_across_a_pause_except_the_declared_ramps() {
    let mut model = Model::new(TOTAL);
    model.prefill();
    assert!(model.prime());
    model.run_periods(7);

    let fade_out = model.dma_l.len() / CHUNK;
    let pause = model.press_pause();
    let ramp_in = model.dma_l.len() / CHUNK;
    model.press_resume(0);
    model.run_periods(6);
    assert_eq!(model.pauses[0].fade_periods, FADE_PERIODS);
    assert_eq!(pause.dropped(), 0);

    // The source's own bytes, one chunk per hand-off. The run is far shorter than the track, so
    // every byte is a real sample rather than `take_chunk`'s tail padding.
    let source: Vec<u8> = (0..model.dma_l.len() as u32)
        .map(|i| fifo_byte(pattern_sample(i)))
        .collect();
    assert_eq!(model.dma_l.len() % CHUNK, 0);
    let chunks = model.dma_l.len() / CHUNK;
    assert_eq!(chunks, ramp_in + 1 + 6);

    for c in 0..chunks {
        let got = &model.dma_l[c * CHUNK..(c + 1) * CHUNK];
        let src = &source[c * CHUNK..(c + 1) * CHUNK];
        if c == fade_out {
            // Window 1: the declared ramp-out, sample for sample.
            let mut last = GAIN_UNITY;
            for i in 0..CHUNK {
                let k = (FADE_SAMPLES - i as i32).clamp(0, 256);
                assert_eq!(
                    got[i],
                    scale_byte(src[i], k),
                    "chunk {c} sample {i}: the ramp-out"
                );
                assert!(k <= last, "the ramp-out went up at sample {i}");
                last = k;
            }
        } else if c == fade_out + 1 {
            // The play-out period: the same source samples at gain zero, and the freeze cuts it, so
            // it is handed over but never heard. No sample is *missing* from the audible stream.
            assert!(
                got.iter().all(|&b| b == 0),
                "the play-out period is not silence at gain zero"
            );
        } else if c == ramp_in {
            // Window 2: the declared ramp-in on the very next source chunk — contiguous with the
            // last heard one, so no sample was skipped or replayed across the freeze.
            let mut last = 0;
            for i in 0..CHUNK {
                let k = (i as i32).clamp(0, 256);
                assert_eq!(
                    got[i],
                    scale_byte(src[i], k),
                    "chunk {c} sample {i}: the ramp-in"
                );
                assert!(k >= last, "the ramp-in went down at sample {i}");
                last = k;
            }
        } else {
            assert_eq!(
                got, src,
                "chunk {c} of the handed-over stream is not the source's"
            );
        }
    }

    // The ledgers close across both transitions: the clock is the ring's consumed span, the producer
    // never ran while frozen, and no truncate is in this run.
    assert_eq!(
        model.feed.pushed(),
        model.feed.consumed() + model.feed.ready()
    );
    assert_eq!(model.committed, model.feed.consumed() as u32);
    assert_eq!(model.underflows, 0);
}

/// A pause fades the **hand-off** and never the ring.
///
/// The whole reason the gain lives in the feeder: the ring holds 1.23 s of already-decoded audio, and
/// a producer-side fade would dim audio that has not been queued to play yet. So a pause must leave
/// the producer's frontier and the queued span alone — only the chunk actually being handed over is
/// scaled — which is what makes the resume need no seek.
#[test]
fn a_pause_fades_the_hand_off_and_never_touches_the_ring() {
    let mut model = Model::new(TOTAL);
    model.prefill();
    assert!(model.prime());
    model.run_periods(6);

    let pushed = model.feed.pushed();
    let consumed = model.feed.consumed();
    let ready = model.feed.ready();
    assert!(ready > CHUNK, "the ring was nearly empty before the pause");

    let pause = model.press_pause();
    let served = pause.fade_periods as usize * CHUNK;

    // Nothing was pushed (the producer is halted by the state machine, not by a truncate) and
    // nothing queued was dropped: the feeder took exactly the chunks it handed over.
    assert_eq!(
        model.feed.pushed(),
        pushed,
        "the ring's producer frontier moved during a fade"
    );
    assert_eq!(
        model.feed.consumed(),
        consumed + served,
        "the feeder committed more than it served"
    );
    assert_eq!(
        model.feed.ready(),
        ready - served,
        "a truncate would have dropped the rest of the queued span"
    );
    assert!(
        model.feed.ready() > 0,
        "a pause that emptied the ring is not resumable"
    );
    // The frozen chain is not a starved one: no period is served, so the counter cannot tick.
    assert_eq!(model.underflows, 0);
    assert_eq!(model.transport.state(), State::Paused);
}

/// A pause freezes the clock and the ring head; a resume continues on the sample it stopped on.
///
/// §4.4's invariants hold across the freeze by construction — the session clock is incremented *in*
/// the period IRQ, so a stopped chain stops the clock — and the resume's exactness is the ledger's:
/// no sample skipped, none repeated, `pushed == consumed` intact.
#[test]
fn a_pause_freezes_the_clock_and_the_ring_head_so_resume_drops_no_sample() {
    let mut model = Model::new(TOTAL);
    model.prefill();
    assert!(model.prime());
    model.run_periods(8);

    let pause = model.press_pause();
    let frozen_committed = model.committed;
    let frozen_ready = model.feed.ready();
    let frozen_paint = paint_position(frozen_committed, TOTAL);
    assert_eq!(pause.frozen_committed, frozen_committed);

    // The chain is stopped: three seconds' worth of video frames pass with no audio period, so
    // neither the clock nor the ring moves and the painted position cannot go backwards.
    for _ in 0..180 {
        assert_eq!(
            model.committed, frozen_committed,
            "the clock ran while frozen"
        );
        assert_eq!(
            model.feed.ready(),
            frozen_ready,
            "the ring moved while frozen"
        );
        assert_eq!(paint_position(model.committed, TOTAL), frozen_paint);
    }
    // §4.4's other two invariants are the map's own (section 7); what a pause adds is that the map
    // is unchanged across the frozen span, which is the equality above.

    let frozen_served = model.feed.consumed();
    model.press_resume(180);
    let pause = model.pauses[0];
    assert_eq!(pause.dropped(), 0, "the resume dropped or replayed samples");
    assert_eq!(pause.resumed_from, frozen_committed);
    // Only the prime's own chunk moved the clock: the resume took the ring's head, which is the
    // sample the freeze stopped on.
    assert_eq!(model.committed, frozen_committed + CHUNK as u32);

    // The ring's byte stream is the closed form, sample for sample, across the pause: nothing was
    // skipped and nothing was replayed.
    let expected: Vec<u8> = (0..model.feed.consumed())
        .map(|i| fifo_byte(pattern_sample(i as u32)))
        .collect();
    assert_eq!(
        model.ring_l[..expected.len()],
        expected[..],
        "the ring's stream is not the source's, so a sample was skipped or repeated"
    );
    assert_eq!(model.feed.consumed(), frozen_served + CHUNK);

    // And it grows again from there, never backwards.
    let before = paint_position(model.committed, TOTAL);
    model.run_periods(4);
    assert!(paint_position(model.committed, TOTAL) >= before);
}

/// **The pause's own leg gets a bound of the run's own making, and the pass length is in it.**
///
/// Every other leg of §4.4's D2 chain carries a bound; the pause's was recorded — the ROM's
/// `PauseRecord::fade_periods` is the feeder's entry count at the freeze minus its entry count at the
/// toggle — and compared to nothing, so `pause=true` meant `gain == 0 && dropped == 0`, which the
/// gate's own summary already called "both arms of a clock-delta tautology". This drives the pause at
/// four pass lengths and holds the observed leg inside `pause_to_freeze_bound(pass)`.
///
/// **The loop is taken once per pass, on purpose.** `Step::Freeze` is only observable from `tick()`,
/// and `tick()` only ever runs inside a main-loop pass — which is the whole difference between this
/// leg and a skip's hand-off, performed by the period ISR on its own cadence. So a machine whose
/// producer frame spans `pass` feeder periods cannot *freeze* any sooner than its next turn, however
/// fast the ramp ran: the leg is the later of the deferral's floor and the loop's cadence, and the
/// readings below say exactly that (`max(pass, FADE_PERIODS)`), which is what makes the pass term in
/// the bound a measured quantity rather than slack. A run that lost the `served_silent` latch, or a
/// ramp that never reached silence, breaks the equality; a loop that stopped calling `tick()` breaks
/// the inequality.
#[test]
fn a_pause_freezes_inside_the_bound_its_own_pass_length_supports() {
    for pass in [1u32, 2, 3, 8] {
        let mut model = Model::new(TOTAL);
        model.pass_periods = pass;
        model.prefill();
        assert!(model.prime());
        model.run_periods(6);

        let bound = pause_to_freeze_bound(pass);
        let primable = model.ring_primable();
        assert_eq!(
            model.transport.toggle(true, primable),
            Step::FadeOut,
            "a pause from Playing must fade out (pass {pass})"
        );

        let mut periods = 0;
        loop {
            // The chain keeps running while `Pausing` — the ramp plays out on the audio clock — but
            // the main loop's turn, and so the only observable `tick()`, comes once per pass.
            for _ in 0..pass {
                model.period();
                periods += 1;
            }
            match model.transport.tick() {
                Step::Freeze => break,
                Step::Idle => assert!(
                    periods < bound,
                    "a {pass}-period pass took {periods} periods to freeze; the bound is {bound}"
                ),
                other => panic!("unexpected step {other:?} for a pause (pass {pass})"),
            }
        }
        assert_eq!(
            model.transport.state(),
            State::Paused,
            "the freeze must move the state (pass {pass})"
        );
        assert_eq!(
            periods,
            pass.max(FADE_PERIODS),
            "the leg must be the later of the deferral's floor and the loop's cadence, not a third number (pass {pass})"
        );
        assert!(
            periods <= bound,
            "a {pass}-period pass froze in {periods} periods against a bound of {bound}"
        );
    }
}

/// The bound is a function of a measurement, never a number copied out of this emulator (R13).
///
/// Pinned so the shape cannot rot back into a constant: it grows one period per period of pass, it
/// floors at the deferral's own period count plus the alignment period, and it can never sit below
/// `take_to_handoff_bound` — a pause's leg contains the same ramp and the same "the ramped chunk must
/// be served" condition, and adds the loop's turn on top. For reference, at the pass this repo measures
/// on mGBA (3 periods) the value is 6 where the literal that once guarded the model's own continuation
/// was 4: the same class of error, wearing an integer instead of a threshold.
#[test]
fn the_pause_bound_moves_with_the_machine_it_is_measured_on() {
    assert_eq!(pause_to_freeze_bound(0), FADE_PERIODS + 1);
    for pass in 1..=12u32 {
        assert_eq!(
            pause_to_freeze_bound(pass),
            pause_to_freeze_bound(pass - 1) + 1,
            "the bound must move one period per period of pass, because the loop's turn is one pass"
        );
        assert!(
            pause_to_freeze_bound(pass) >= take_to_handoff_bound(pass),
            "a pause's leg carries the same deferral as a skip's hand-off, so it cannot be bounded tighter"
        );
    }
    // A pass measured at 2 periods here and at 4 periods on a real cart both stay inside their own
    // bound; the old literal could not tell those machines apart, which is why it was not a bound.
    assert!(pause_to_freeze_bound(2) < pause_to_freeze_bound(4));
}

// --------------------------------------------------------------------------------=============
// 8c-bis. PR-3c: a resume the ring cannot prime (§4.4's no-seek invariant across a refill)
// ---------------------------------------------------------------------------------------------
//
// Q18's ruling (§15): a product whose ring is too short to hand the prime's chunk **does not stop**;
// the listener expects audio to resume. `Transport` now routes that press by what the ring answers —
// `toggle(true, primable)` returns `Step::Buffer` and the chain stays frozen while the producer brings
// the ring back to `PREFILL_SAMPLES`, one frame per main-loop pass — and §4.4's invariant is what
// separates that from a seek: the ring's head, and so the session clock, does not move.
//
// Why this section exists in the host suite at all: on every ROM arm this repo has measured, the ring
// still held `PREFILL_SAMPLES` when the resume was pressed, so the ROM's row prints `refills=0` and no
// hardware witness exists for the branch. The model is currently the only place it is reachable. That is
// a fact about the fixtures, not a reason to leave the branch untested — and it is why the row prints
// the count even when it is zero.

/// The boundary allowance the ROM gives any leg that ends at an audio period: `calc_boundary_frames` at
/// the video period, the same expression `album_play_main.rs` pins to `MAX_BOUNDARY_FRAMES == 3`, and
/// the figure the pause and hand-off bounds use (§10.3's worst-case alignment, R13's rule that a bound
/// is priced off a measured period rather than off an assumption about which main-loop pass notices).
const RESUME_BOUNDARY_FRAMES: u32 =
    (geometry::PERIOD_CYCLES + ROM_VIDEO_PERIOD_CYCLES - 1) / ROM_VIDEO_PERIOD_CYCLES;

/// PR-3c's headline: the press that cannot prime is **not** a stop and **not** a seek.
///
/// The fixture is the case the ROM's own runs never reach, built so nothing else in the run is unusual:
/// the ring is drained to 160 samples by *full* periods (every one of them had a whole chunk to take,
/// so `underflows` stays zero), and the pause's fade takes two more whole chunks before it freezes. At
/// the resume the ring holds 160 samples — less than the prime's chunk — so the transport refuses the
/// prime and asks for a refill instead.
///
/// What is then claimed is the whole of §11 PR-3c's item 4, in the units the repo already keeps:
///
/// * **the wait ends, in frames the run already pays for** — the burst is `PREFILL_FRAMES` producer
///   frames, which is the boot prefill's own cost and geometry.rs's pinned constant, not a new number;
/// * **no sample is lost or replayed** — `dropped()` is zero, as it is for a direct resume;
/// * **the position never jumps** — asserted inside [`Model::press_resume`] where it is a panic on the
///   model itself: the burst serves no period, hands nothing to the DMA, and moves no clock, so the arm
///   takes the same head the freeze stopped on;
/// * **the stream is continuous** — every byte the ring ever handed the gain is still the source's, in
///   order: the same closed-form comparison §8c's scripted-pause run makes, which is the only form of
///   "no seek" a stream can prove rather than assert.
#[test]
fn a_resume_the_ring_cannot_prime_refills_the_ring_and_delivers_the_same_stream() {
    let mut model = Model::new(TOTAL);
    model.prefill();
    assert!(model.prime());
    // 15 full periods: the ring comes to rest at 1,280 samples — two chunks and a bit — with every
    // hand-off full, which is what lets this test read `underflows == 0` at the end instead of a count
    // it would have to explain.
    model.run_periods(15);
    assert_eq!(model.feed.ready(), 1_280);
    assert_eq!(model.underflows, 0, "the fixture starved before the pause");

    let pause = model.press_pause();
    assert_eq!(pause.fade_periods, FADE_PERIODS);
    // The ring the resume is pressed onto: too short for a prime, and the stream is nothing near its
    // end, so a `Tail` chunk is not on offer either. This is the pair of conditions `Step::Buffer` is
    // for, and both are asserted here rather than left to the transport's answer alone.
    assert_eq!(model.feed.ready(), 160);
    assert!(
        !model.finished,
        "a finished stream's short ring is a tail, not a starvation"
    );
    assert!(!model.ring_primable());

    let frames_before = model.video_frames;
    model.press_resume(0);
    let pause = model
        .pauses
        .last()
        .copied()
        .expect("the pause was recorded");

    // The route, stated as a count rather than as a state nobody can see afterwards.
    assert!(pause.refilled, "the resume took the direct prime anyway");
    assert_eq!(
        pause.refill_passes,
        geometry::PREFILL_FRAMES,
        "the refill cost {} frames; the boot prefill reaches this same target in {}",
        pause.refill_passes,
        geometry::PREFILL_FRAMES
    );
    assert_eq!(
        model.transport.state(),
        State::Playing,
        "a starved resume did not hand the chain back to the audio clock"
    );
    // The chain was frozen for the whole burst, so the only video the burst spent is the decode's: the
    // pass price the bound test reads. `pass_periods` is the model's parameter (2 here), one decode per
    // pass — so the leg is `PREFILL_FRAMES` passes of that price, and no period was served to hide it.
    assert_eq!(
        model.video_frames - frames_before,
        geometry::PREFILL_FRAMES * model.pass_periods * PERIOD_FRAMES,
        "the refill's video cost is not the pass price the model was told to use"
    );

    // Nothing was lost or gained: the clock the resume saw is the clock the freeze stopped on.
    assert_eq!(
        pause.dropped(),
        0,
        "a starved resume dropped or replayed samples"
    );
    assert_eq!(pause.resumed_from, pause.frozen_committed);
    assert_eq!(pause.gain_at_freeze, 0);

    // And the run continues to its end with the ring fed, so the stream comparison below covers the
    // refill itself: the frames the burst pushed are in the same order the source made them.
    model.run_periods(4);
    model.produce_to_end();
    model.drain();

    assert_eq!(model.underflows, 0, "a starved resume booked an underflow");
    assert_eq!(model.feed.pushed() as u32, TOTAL);
    assert_eq!(model.feed.consumed() as u32, TOTAL);
    assert_eq!(model.committed, TOTAL);
    let mut expected: Vec<u8> = (0..TOTAL).map(|i| fifo_byte(pattern_sample(i))).collect();
    expected.resize(model.ring_l.len(), 0);
    assert_eq!(
        model.ring_l, expected,
        "the ring's stream diverged across a refill"
    );
}

/// The other half of the branch: a ring that can prime still primes, and the refill is not entered.
///
/// PR-3c must not cost the common case anything. Both fixtures below differ only in how many periods
/// were played before the pause, so the two rows differ in one condition — the depth the resume finds.
#[test]
fn a_resume_the_ring_can_prime_never_enters_the_refill() {
    for (periods, expect_refilled) in [(15u32, true), (14, false)] {
        let mut model = Model::new(TOTAL);
        model.prefill();
        assert!(model.prime());
        model.run_periods(periods);
        let ready_at_press = model.feed.ready();
        model.press_pause();
        let ready_at_resume = model.feed.ready();
        assert_eq!(
            ready_at_resume,
            ready_at_press - FADE_PERIODS as usize * CHUNK,
            "the fade took something other than whole chunks (pass {periods})"
        );
        assert_eq!(
            model.ring_primable(),
            !expect_refilled,
            "a ring holding {ready_at_resume} samples at the resume is routed to {}",
            if expect_refilled {
                "a refill"
            } else {
                "a prime"
            }
        );
        model.press_resume(0);
        let pause = model
            .pauses
            .last()
            .copied()
            .expect("the pause was recorded");
        assert_eq!(pause.refilled, expect_refilled);
        assert_eq!(
            pause.refill_passes,
            u32::from(expect_refilled) * geometry::PREFILL_FRAMES,
            "a resume that needed no refill spent frames anyway, and one that did spent the wrong count"
        );
        assert_eq!(pause.dropped(), 0, "a resume lost samples (pass {periods})");
    }
}

/// The `primable` the ROM passes is the ring's own verdict, not a guess about the future (§14 R13).
///
/// So it is checked against the answer it is meant to predict: for every depth at the boundary, and for
/// both values of `finished`, the expression the ROM's `ring_primable()` and the model's
/// [`Model::ring_primable`] share is compared with what `RingFeed::take_chunk` actually replies. A
/// `Starved` reply and a `false` must agree exactly — a `true` that the ring then refuses is the
/// half-empty prime the doc names as the bug this PR is not, and a `false` that the ring would have
/// served is a product that needlessly waited.
#[test]
fn resume_routes_by_what_the_ring_answers_not_by_what_the_state_hopes() {
    // The shared expression, written out so this test is about its two clauses rather than about the
    // two places that use it: the second clause is `take_chunk`'s `finished` argument, which turns a
    // short ring at the end of a stream into a `Tail` instead of a `Starved`.
    let primable = |ready: usize, finished: bool| ready >= CHUNK || (finished && ready > 0);

    // Every depth within a chunk of the boundary, and the two depths that matter beyond it: a full
    // chunk, a chunk plus one, and the whole ring.
    let mut depths = vec![0usize, 1, 279, CHUNK - 1, CHUNK, CHUNK + 1, 2 * CHUNK, RING];
    depths.dedup();
    for ready in depths {
        for finished in [false, true] {
            let mut model = Model::new(TOTAL);
            let zeros = vec![0u8; ready];
            model
                .feed
                .push_bytes(&zeros, &zeros)
                .expect("the depth fits the ring");
            assert_eq!(model.feed.ready(), ready, "the fixture depth is wrong");
            let (mut l, mut r) = ([0u8; CHUNK], [0u8; CHUNK]);
            let verdict = model.feed.take_chunk(&mut l, &mut r, CHUNK, finished);
            let took = match verdict {
                // The reply the ROM must not receive after a `true`: the prime would hand the ramp a
                // short chunk and the chain would start with silence inside it.
                Chunk::Starved => false,
                Chunk::Full => true,
                // A `Tail` is the prime a finished visit asks for — §4.2b's short chunk, not an error.
                Chunk::Tail { .. } => true,
            };
            assert_eq!(
                primable(ready, finished),
                took,
                "a ring holding {ready} with finished={finished} answers {} while the routing expression says {}",
                if took { "a chunk" } else { "Starved" },
                primable(ready, finished)
            );
        }
    }
}

/// The refill's target and its cost are the boot prefill's, and the room gate never refuses a pass.
///
/// Two claims in one, both of which the design rests on:
///
/// * **no new number.** From every depth that can start a burst (`ready() < CHUNK`), reaching
///   `PREFILL_SAMPLES` takes exactly `PREFILL_FRAMES` frames — the constant geometry.rs already pins as
///   a compile-time fact, here checked against the real ring rather than against arithmetic on constants.
///   A burst that stopped one frame short would be a second, smaller target; one that ran past it is a
///   refill that never decided to end.
/// * **the wait always ends.** `produce()` asserts the ROM's flow-control gate (`free_samples() >=
///   FRAME`) on every pass, so if a burst could ever start with too little free ring for one frame, the
///   model would panic instead of quietly spinning. It never does: the entry depths that can start a
///   burst are the shortest rings the stream can be holding.
#[test]
fn a_refill_reaches_the_prefill_target_in_the_boot_prefill_s_own_frame_count() {
    for ready in [0usize, 1, 160, 279, CHUNK - 1] {
        let mut model = Model::new(TOTAL);
        let zeros = vec![0u8; ready];
        model
            .feed
            .push_bytes(&zeros, &zeros)
            .expect("the depth fits the ring");
        let mut passed = 0;
        while model.feed.ready() < PREFILL {
            assert!(
                passed < geometry::PREFILL_FRAMES,
                "a burst from {ready} has spent {passed} frames and is at {}; the target is reached in {}",
                model.feed.ready(),
                geometry::PREFILL_FRAMES
            );
            assert!(
                model.produce(),
                "a burst's producer stopped at {} with the target at {PREFILL}",
                model.feed.ready()
            );
            passed += 1;
        }
        assert_eq!(
            passed,
            geometry::PREFILL_FRAMES,
            "a burst from {ready} reached the target in {passed} frames"
        );
        // One frame less would have left the ring below the target — i.e. the burst ended on the
        // target, not on a count.
        assert!(model.feed.ready() - FRAME < PREFILL);
    }
}

/// PR-3c's caller decision, priced the way R13 requires: the bound is a function of the run's own
/// measured pass length, and the leg is what the model actually spent.
///
/// `resume_to_audio_bound` asks for the worst producer pass **in video frames** and the boundary
/// allowance the ROM gives any leg that ends at an audio period. It is deliberately the same two
/// quantities the take and pause bounds use — the leg this one bounds is the same decode, seen from the
/// other side of a pause. Four pass lengths are swept so the point is visible in the numbers: at every
/// length the observed wait holds inside the bound, the bound moves with the machine, and the wait is
/// zero when the ring could prime, which is the only case in which a bound of zero is honest.
#[test]
fn the_refill_awaits_no_more_than_the_run_own_pass_price_allows() {
    // The shape, pinned so the bound cannot quietly become a constant: the frames the prefill's own
    // decode needs, at the pass length this machine pays, plus the alignment allowance — and nothing
    // else. `resume_to_audio_bound(0, …)` is the floor: the press-to-arm wait of a ring that needed no
    // refill, which is the alignment alone.
    assert_eq!(
        resume_to_audio_bound(0, RESUME_BOUNDARY_FRAMES),
        RESUME_BOUNDARY_FRAMES
    );
    for pass in [1u32, 2, 4, 8] {
        let pass_frames = pass * PERIOD_FRAMES;
        assert_eq!(
            resume_to_audio_bound(pass_frames, RESUME_BOUNDARY_FRAMES),
            geometry::PREFILL_FRAMES * pass_frames + RESUME_BOUNDARY_FRAMES,
            "the bound must move with the pass length it is priced at (pass {pass} = {pass_frames} frames)"
        );
        assert!(
            resume_to_audio_bound(pass_frames + PERIOD_FRAMES, RESUME_BOUNDARY_FRAMES)
                > resume_to_audio_bound(pass_frames, RESUME_BOUNDARY_FRAMES),
            "a slower machine may not inherit a faster one's bound"
        );
    }

    // The leg, measured. One fixture per pass length, at the depth the headline test uses, with the
    // model's pass parameter varied — the same sweep `a_pause_leg_holds_inside_its_bound` makes for the
    // pause's leg.
    for pass in [1u32, 2, 4, 8] {
        let mut model = Model::new(TOTAL);
        model.pass_periods = pass;
        model.prefill();
        assert!(model.prime());
        model.run_periods(15);
        model.press_pause();
        assert_eq!(model.feed.ready(), 160, "the fixture starved (pass {pass})");

        let frames_before = model.video_frames;
        model.press_resume(0);
        let pause = model
            .pauses
            .last()
            .copied()
            .expect("a resume follows a pause");
        assert!(pause.refilled);
        let leg = model.video_frames - frames_before;
        let bound = resume_to_audio_bound(pass * PERIOD_FRAMES, RESUME_BOUNDARY_FRAMES);
        assert!(
            leg <= bound,
            "a resume on a 160-sample ring took {leg} video frames at a {pass}-period pass; the bound is {bound}"
        );
        // The bound is not slack it happens to cover: the wait is the prefill's own frame count, and
        // the boundary term is the only headroom, exactly as it is for the take and pause legs.
        assert_eq!(leg, geometry::PREFILL_FRAMES * pass * PERIOD_FRAMES);
        assert!(
            bound - leg <= RESUME_BOUNDARY_FRAMES,
            "the leg left {} frames of headroom, more than the {RESUME_BOUNDARY_FRAMES}-frame alignment allowance the bound is priced with",
            bound - leg
        );
    }
}

/// `Buffering` feeds the ring and times no pass: the two questions the main loop asks a state are kept
/// apart, and this is where the difference lives.
///
/// `feeds()` is the producer's gate — a state whose ring is not being consumed may still be filled, and
/// `Buffering` is that state. `pass_timeable()` is the *timer* question: a pass may only be measured
/// against an audio counter that is running. Conflating them does not misplace a frame: it makes every
/// refill pass a false underflow, and the row's `underflows` figure stops meaning "a period asked and
/// the ring refused".
///
/// The two sets are the same except here, and the table is asserted for all four states the pause path
/// walks through, so the exception is visible rather than implied: `Playing` (the loop's normal pass, a
/// real decode against a running counter) is both; `Paused` is neither; `Buffering` is one only; and
/// `Resuming` — the arm's own pass, which would read a delta straight across a timer restart, and the
/// ramp's passes, which contain no decode at all — is neither again.
#[test]
fn buffering_feeds_the_ring_and_times_no_pass() {
    let mut model = Model::new(TOTAL);
    model.prefill();
    assert!(model.prime());
    model.run_periods(15);
    assert!(
        model.transport.state().feeds() && model.transport.state().pass_timeable(),
        "the loop's ordinary pass both feeds and times: that is the baseline the other rows are read against"
    );
    model.press_pause();
    assert_eq!(model.transport.state(), State::Paused);
    assert!(
        !(model.transport.state().feeds() || model.transport.state().pass_timeable()),
        "a paused chain neither decodes nor runs a counter"
    );

    let primable = model.ring_primable();
    assert_eq!(
        model.transport.toggle(true, primable),
        Step::Buffer,
        "a 160-sample ring cannot give the prime its first chunk"
    );
    assert_eq!(model.transport.state(), State::Buffering);
    assert!(
        model.transport.state().feeds(),
        "the refill is a feeding state: the producer is the only thing that can end this wait"
    );
    assert!(
        !model.transport.state().pass_timeable(),
        "a frozen chain has no audio counter to measure a pass against"
    );
    // The burst at the loop's own cadence: one frame of decode per pass, and the pass's decision call
    // answers nothing — a `tick()` that acted here would be a timeout, and Q18 ruled that the wait ends
    // when the ring is full, not when a clock says so.
    for frame in 0..geometry::PREFILL_FRAMES {
        assert!(
            model.produce(),
            "the refill's producer stopped at frame {frame}"
        );
        assert_eq!(
            model.transport.tick(),
            Step::Idle,
            "the refill's tick acted"
        );
        assert_eq!(
            model.transport.state(),
            State::Buffering,
            "the refill ended on a tick, at frame {frame}"
        );
    }
    assert!(model.feed.ready() >= PREFILL);
    assert_eq!(model.transport.end_buffer(), Step::Prime);
    assert_eq!(model.transport.state(), State::Resuming);
    assert!(
        !(model.transport.state().feeds() || model.transport.state().pass_timeable()),
        "the arm's own pass would read a delta straight across a timer restart, and the ramp's passes hold no decode: `Resuming` belongs to neither gate"
    );
}

// ---------------------------------------------------------------------------------------------
// 8c. The run, through scripted pauses
// ---------------------------------------------------------------------------------------------

/// Three scripted pauses book **no underflow**, and every hand-off is either a period or a prime.
///
/// The ledger coherence the ROM's `feed:` line has to keep with a pause in the run: the total number
/// of chunk hand-offs is `ceil(n_samples / chunk)` whether or not the run was paused, but a resume's
/// prime is a hand-off the feeder did not make — so `handler_entries + primes` is the number to read,
/// not `handler_entries` alone.
#[test]
fn scripted_pauses_book_no_underflow_and_every_hand_off_is_a_period_or_a_prime() {
    let mut model = Model::new(TOTAL);
    model.prefill();
    assert!(model.prime());
    assert_eq!(model.primes, 1);

    let mut pauses = 0;
    for mark in [1usize, 40, 120] {
        while model.frames < mark {
            if model.feed.free_samples() >= FRAME {
                assert!(model.produce(), "the source ended early at frame {mark}");
            } else {
                model.period();
            }
        }
        model.press_pause();
        pauses += 1;
        // A second of video frames spent frozen, which is a pause a listener would call a pause.
        model.press_resume(60);
        assert_eq!(
            model.transport.state(),
            State::Playing,
            "the resume did not return to Playing"
        );
    }
    model.produce_to_end();
    model.drain();

    // The three claims, in the ledger's own units.
    assert_eq!(model.underflows, 0, "a pause booked an underflow");
    assert_eq!(model.feed.pushed() as u32, TOTAL);
    assert_eq!(model.feed.consumed() as u32, TOTAL);
    assert_eq!(model.feed.ready(), 0);
    assert_eq!(model.committed, TOTAL);
    assert_eq!(model.frames, 240);
    // Every hand-off: 876 full chunks + the 400-sample tail, with three of them primes.
    assert_eq!(model.handoffs, 878);
    assert_eq!(model.primes, 1 + pauses);
    assert_eq!(model.handoffs - model.primes, 877 - pauses);
    // §4.4's end clamp, with the pauses in the run.
    assert_eq!(paint_position(model.committed, TOTAL), TOTAL);
    // No pause lost a sample.
    assert_eq!(model.pauses.len(), pauses as usize);
    for pause in &model.pauses {
        assert_eq!(pause.dropped(), 0, "a scripted pause dropped samples");
        assert_eq!(pause.gain_at_freeze, 0);
        assert_eq!(pause.fade_periods, FADE_PERIODS);
    }

    // And the bytes the DMA was handed are the source's, in order, with only the fades' scaling on
    // top — the unscaled capture is the ring's own stream, and it is continuous.
    let mut expected: Vec<u8> = (0..TOTAL).map(|i| fifo_byte(pattern_sample(i))).collect();
    // The tail hand-off is zero-padded to a whole chunk by `take_chunk`.
    expected.resize(model.ring_l.len(), 0);
    assert_eq!(
        model.ring_l, expected,
        "the ring's stream diverged across a pause"
    );
}

/// The producer must not be resumed into a full ring.
///
/// D3's cost, stated where it bites: a paused ROM holds `ready()` samples it is not consuming, so a
/// push attempted against the frozen ring is refused — with the ledger untouched — and the producer's
/// flow control is what keeps that from being an error path.
#[test]
fn a_frozen_ring_refuses_a_push_and_resume_delivers_it() {
    let mut model = Model::new(TOTAL);
    model.prefill();
    assert!(model.prime());
    // Fill the ring until a whole frame no longer fits: that is where `has_room` stops the producer.
    while model.feed.free_samples() >= FRAME {
        assert!(model.produce());
    }
    let pushed = model.feed.pushed();

    let free_before = model.feed.free_samples();
    assert!(
        free_before < FRAME,
        "the ring was not at its flow-control limit"
    );
    model.press_pause();
    // The pause serves the fade and its play-out period, so the frozen ring holds more room than the
    // producer's flow-control gate allows — and it is still frozen, so nothing is pushed into it.
    let free = model.feed.free_samples();
    assert_eq!(free, free_before + FADE_PERIODS as usize * CHUNK);
    // A push one sample larger than the frozen room is refused, and nothing is credited. 0x80 is
    // chosen because the fake source cannot produce it (its bytes span ±60 codes), so it is a marker.
    let marker = vec![0x80u8; free + 1];
    assert_eq!(
        model.feed.push_bytes(&marker, &marker),
        Err(PushError::OutOfSpace {
            need: free + 1,
            have: free
        })
    );
    assert_eq!(
        model.feed.pushed(),
        pushed,
        "a refused push credited samples"
    );
    assert_eq!(model.underflows, 0, "a frozen chain looked starved");

    // After a resume, periods free room and the next frame goes through — the same frame, not a
    // re-decoded one.
    model.press_resume(0);
    model.run_periods(4);
    assert!(
        model.produce(),
        "the ring did not accept a frame after the resume"
    );
    model.produce_to_end();
    model.drain();

    assert_eq!(model.feed.pushed() as u32, TOTAL);
    assert_eq!(model.feed.consumed() as u32, TOTAL);
    assert!(
        !model.ring_l.contains(&0x80),
        "the refused frame's bytes reached the ring"
    );
}

// ---------------------------------------------------------------------------------------------
// 8d. Skip: the hand-off, measured off the wire (PR-3b-ii-B2b)
// ---------------------------------------------------------------------------------------------
//
// The ROM's evidence for §4.4's skip is a `skip:` row per hand-off, measured by a wire watch in the
// feeder. This section is the same machine — the real `RingFeed`, the real `Transport`, the real
// `HandOffGain` — driven by a fake source and read the same way, so the ROM's rows are checkable
// arithmetic rather than a log nobody can reproduce.
//
// The one thing this cannot witness is the cost of a real decode or the real frame loop's cadence:
// `pass_periods` is the model's *parameter* for a pass, and the bound below is what the ROM measures.

/// A track long enough for several visits against a 36-chunk ring, and an exact multiple of a chunk so
/// a visit can be played to its last sample without a classified tail.
const SKIP_TRACK: u32 = 117 * CHUNK as u32;

/// §4.4 step 5's own number: the new track's first chunk reaches the wire `prime + 1` periods after
/// the hand-off, measured from the START of the hand-off, at every prime the probe swept.
///
/// The probe measured `to_new = prime + 1` with `underflows = 0` at every prime including 2
/// (`examples/album_probe/README.md` → "The deep arm": 3, 5, 9 and 17 periods at 2, 4, 8 and 16).
/// This drives the app's own `begin_reprime` order and reads the same number off the wire, so the
/// shipped machine and the measured one are one arithmetic — and the shipped prime, 4, is a row of
/// that table rather than a number chosen beside it.
#[test]
fn a_skip_lands_the_new_track_prime_plus_one_periods_after_the_hand_off() {
    assert_eq!(
        SKIP_PRIME_CHUNKS, 4,
        "the shipped prime is one of the swept rows"
    );
    for (prime, probe_row) in [(2u32, 3u32), (4, 5), (8, 9), (16, 17)] {
        let mut model = Model::album(&[SKIP_TRACK; 3], 0);
        model.prefill();
        assert!(model.prime());
        model.paint.start(SKIP_TRACK, 0);
        model.run_periods(4);
        model.fill_deep();

        let skip = model.press_skip(1, prime, 1);
        assert_eq!(
            skip.content_at(),
            prime + 1,
            "prime {prime}: the machinery's content point moved"
        );
        assert_eq!(
            skip.to_new(),
            probe_row,
            "prime {prime}: the probe's row was {probe_row} periods"
        );
        // An audibly-opening track, so the wire's transition IS the machinery's: the two figures agree
        // and `head_silence` is zero. The next test is the one where they differ.
        assert_eq!(skip.head_silence(), 0);
        assert_eq!(skip.silence(), prime, "the cover's own silence, in periods");
        assert_eq!(
            skip.old(),
            0,
            "prime {prime}: stale audio after the hand-off"
        );
        assert_eq!(skip.underflow_delta(), 0, "prime {prime}: an underflow");
        assert_eq!(skip.press_to_handoff(), FADE_PERIODS);
    }
}

/// A skip's **post-gain** stream after the hand-off is the cover's silence, then the new track's own
/// bytes under the declared ramp-in — not one old-track byte survives the truncate.
///
/// `old == 0` is checked on the same run and is the weaker witness: during the ramped tail the gain
/// is zero, so a stale chunk would read as silence too. This test reads the bytes the DMA was
/// actually handed: the chunk after the fade-out is `0x00` throughout, the whole cover is `0x00`
/// chunk for chunk, and the first new chunk is the new source's samples at the ramp-in's gain.
#[test]
fn a_skips_post_gain_stream_is_the_cover_then_the_new_tracks_own_bytes() {
    let mut model = Model::album(&[SKIP_TRACK; 2], 0);
    model.prefill();
    assert!(model.prime());
    model.paint.start(SKIP_TRACK, 0);
    model.run_periods(4);
    model.fill_deep();
    let skip = model.press_skip(1, SKIP_PRIME_CHUNKS, 1);

    // The mark is the hand-off's period; every hand-off appends one chunk, so its chunk index is the
    // period index. The chunk served *at* the mark is the last of the old track, and its tail is
    // already silence from the ramp.
    let at = skip.at as usize;
    assert_eq!(
        model.dma_l.len() / CHUNK,
        at + 1 + SKIP_PRIME_CHUNKS as usize + 1
    );
    let faded = &model.dma_l[at * CHUNK..(at + 1) * CHUNK];
    assert!(
        faded[FADE_SAMPLES as usize..].iter().all(|&b| b == 0),
        "the last old-track chunk did not finish on the ramp's silent tail"
    );

    // Every cover chunk is whole and exactly silent. This is what "nothing stale is served after the
    // hand-off" means in bytes: the old track's source bytes at these indices are non-zero, so a
    // survivor would show here.
    for c in at + 1..=at + SKIP_PRIME_CHUNKS as usize {
        assert_eq!(
            &model.dma_l[c * CHUNK..(c + 1) * CHUNK],
            &vec![0u8; CHUNK][..],
            "cover chunk {c} is not silence, so something survived the truncate"
        );
    }

    // The new track's first chunk, under the ramp-in from the centre. `start_track` reset the source
    // to sample 0, so its bytes are the closed form from there.
    let first_new = at + SKIP_PRIME_CHUNKS as usize + 1;
    assert_eq!(first_new - at, skip.content_at() as usize);
    let got = &model.dma_l[first_new * CHUNK..(first_new + 1) * CHUNK];
    let mut last = 0;
    for i in 0..CHUNK {
        let k = (i as i32).clamp(0, 256);
        assert_eq!(
            got[i],
            scale_byte(fifo_byte(pattern_sample(i as u32)), k),
            "sample {i} of the new track's first chunk is not its byte at gain {k}"
        );
        assert!(k >= last, "the ramp-in went down at sample {i}");
        last = k;
    }
    assert_eq!(got[0], 0x00, "the ramp-in did not start at the centre");
    assert!(
        got[FADE_SAMPLES as usize..]
            .iter()
            .enumerate()
            .all(|(j, &b)| b == fifo_byte(pattern_sample(FADE_SAMPLES as u32 + j as u32))),
        "the ramp-in did not return to full gain inside the new track's first chunk"
    );

    // The ledger and the ring: the truncate accounts for the queued span exactly, and nothing was
    // replayed across the hand-off.
    assert_eq!(
        model.feed.pushed(),
        model.feed.consumed() + model.feed.ready()
    );
    assert_eq!(
        model.committed + model.dropped,
        model.feed.consumed() as u32,
        "the clock plus the truncate's span is the ring's ledger"
    );
    assert_eq!(skip.old(), 0);
    assert_eq!(skip.underflow_delta(), 0);
    assert_eq!(model.underflows, 0);
}

/// A skip at a deep queue drops the queued span, books no underflow and hears no stale period, at
/// every one of `N` hand-offs.
///
/// This is the truncate's whole justification: before it, a skip had to drain a 1.23 s queue. The
/// queue is deliberately nearly the whole ring at each mark, and `dropped` is checked against what
/// was actually queued — minus the fade's own chunk, which was handed over, not dropped.
#[test]
fn a_deep_queue_skip_drops_the_queued_span_and_books_no_underflow() {
    const N: u32 = 5;
    let visits: [usize; N as usize + 1] = [0, 1, 2, 0, 1, 2];
    let mut model = Model::album(&[SKIP_TRACK; 3], 0);
    model.prefill();
    assert!(model.prime());
    model.paint.start(SKIP_TRACK, 0);
    model.run_periods(4);

    let mut dropped_total = 0u32;
    let mut queued_least = u32::MAX;
    for step in 0..N {
        model.fill_deep();
        let queued = model.feed.ready() as u32;
        assert!(
            queued >= RING as u32 - CHUNK as u32,
            "step {step}: the queue was not deep — {queued} samples ready"
        );
        queued_least = queued_least.min(queued);
        let skip = model.press_skip(visits[step as usize + 1], SKIP_PRIME_CHUNKS, 1);
        assert_eq!(
            skip.dropped + FADE_PERIODS * CHUNK as u32,
            queued,
            "step {step}: the truncate dropped {} of {queued} queued samples",
            skip.dropped
        );
        dropped_total += skip.dropped;
        assert_eq!(
            skip.old(),
            0,
            "step {step}: a stale period reached the wire"
        );
        assert_eq!(
            skip.underflow_delta(),
            0,
            "step {step}: the hand-off booked an underflow"
        );
        assert_eq!(skip.content_at(), SKIP_PRIME_CHUNKS + 1);
        assert_eq!(
            skip.to_new(),
            SKIP_PRIME_CHUNKS + 1,
            "step {step}: this fixture's tracks open audibly"
        );
        assert_eq!(skip.from, visits[step as usize]);
        assert_eq!(skip.to, visits[step as usize + 1]);
    }
    assert_eq!(model.skips.len(), N as usize);
    assert!(
        dropped_total >= (queued_least - FADE_PERIODS * CHUNK as u32) * N,
        "the truncate stepped over less than was queued (least queue {queued_least})"
    );
    assert_eq!(model.underflows, 0, "the run booked an underflow");
    // The ledger still closes with N truncates in it: the covers are in `pushed` and in `consumed`.
    assert_eq!(
        model.feed.pushed() as u32,
        model.feed.consumed() as u32 + model.feed.ready() as u32
    );
    assert_eq!(
        model.committed + model.dropped,
        model.feed.consumed() as u32,
        "the clock plus the dropped span is the ring's ledger"
    );
}

/// The truncate and the cover's first whole chunk share one critical section — and the bare shape
/// fails, which is what makes the first half an assertion rather than a tautology.
///
/// The probe spent a run on this (`probe_hw.rs:840-875`, `truncate_stale`): a bare truncate leaves a
/// window in which a feeder period finds an empty ring while the producer still has work, and that
/// period is `Starved` — an underflow that is an artifact of the window, not of the audio.
#[test]
fn the_truncate_and_the_covers_first_chunk_share_one_section() {
    // (a) the shipped order: no starved period, no stale period, no underflow.
    let mut model = Model::album(&[SKIP_TRACK; 2], 0);
    model.prefill();
    assert!(model.prime());
    model.paint.start(SKIP_TRACK, 0);
    model.run_periods(4);
    model.fill_deep();
    let skip = model.press_skip(1, SKIP_PRIME_CHUNKS, 1);
    assert_eq!(skip.underflow_delta(), 0);
    assert_eq!(skip.old(), 0);

    // (b) the bare truncate, with the producer still holding work: one starved period, booked.
    let mut bare = Model::album(&[SKIP_TRACK; 2], 0);
    bare.prefill();
    assert!(bare.prime());
    bare.paint.start(SKIP_TRACK, 0);
    bare.run_periods(4);
    bare.fill_deep();
    assert_eq!(bare.transport.skip(), Step::FadeOut);
    bare.period();
    // One period only ramps: the buffer it was applied to is still playing, so the hand-off must
    // not happen yet (that is the deferral, and the whole point of this arm's window).
    assert_eq!(bare.transport.tick(), Step::Idle);
    bare.period();
    assert_eq!(bare.transport.tick(), Step::SkipHandOff);
    assert!(bare.transport.begin_reprime(SKIP_PRIME_CHUNKS));
    let dropped = bare.truncate();
    assert!(dropped > 0, "the bare arm had nothing to truncate");
    bare.source.start_track(1).expect("track 1 exists");
    let before = bare.underflows;
    assert_eq!(
        bare.period(),
        Chunk::Starved,
        "a bare truncate did not leave the feeder an empty ring"
    );
    assert_eq!(
        bare.underflows,
        before + 1,
        "the window's starved period was not booked"
    );
}

/// Every cover push is one whole chunk, and a partial span in the ring can never be handed over.
///
/// `take_chunk` takes whole chunks only, so a partial push left in the ring while the producer runs
/// starves the DMA forever. The model asserts the length at every cover push (`Model::push_cover`) and
/// this test witnesses the rule from the other side, with a deliberately partial ring.
#[test]
fn a_cover_push_is_a_whole_chunk_and_a_partial_span_starves() {
    let mut model = Model::album(&[SKIP_TRACK; 2], 0);
    model.prefill();
    assert!(model.prime());
    model.paint.start(SKIP_TRACK, 0);
    model.run_periods(4);
    model.fill_deep();
    // `press_skip` asserts the ring holds exactly `prime` whole chunks at the mark; read it here too,
    // because a reader of this test should see the claim rather than infer it.
    let skip = model.press_skip(1, SKIP_PRIME_CHUNKS, 1);
    assert_eq!(skip.content_at(), SKIP_PRIME_CHUNKS + 1);

    // Now the negative half: drain what whole chunks can take, step over the sub-chunk remainder (a
    // stream's own tail, not a skip), push half a chunk, and watch a period starve rather than hand
    // over a headless buffer.
    while model.feed.ready() >= CHUNK {
        model.period();
    }
    let _ = model.truncate();
    assert_eq!(model.feed.ready(), 0);
    let half = [0x11u8; CHUNK / 2];
    assert!(model.feed.push_bytes(&half, &half).is_ok());
    let before = model.underflows;
    assert_eq!(
        model.period(),
        Chunk::Starved,
        "take_chunk handed over a partial chunk"
    );
    assert_eq!(
        model.feed.ready() as usize,
        CHUNK / 2,
        "a starved period consumed the partial span"
    );
    assert_eq!(model.underflows, before + 1);
}

/// §4.4's painted position across a skip: 0 at each track's start, never backwards inside a visit,
/// and `n_samples` at its end.
///
/// The mechanism is the visit's clock base — the hand-off's instant plus the cover — so the cover is
/// committed to the session clock (it is handed to the DMA) and yet is not part of any track. That is
/// what makes "0 at a track start" true again once a run can skip.
#[test]
fn the_painted_position_keeps_the_invariants_across_a_skip() {
    let mut model = Model::album(&[SKIP_TRACK; 3], 0);
    model.prefill();
    assert!(model.prime());
    model.paint.start(SKIP_TRACK, 0);
    model.run_periods(6);
    assert!(
        model.paint.position(model.committed) > 0,
        "the first visit never painted"
    );

    model.fill_deep();
    let base_before = model.paint.base;
    model.press_skip(1, SKIP_PRIME_CHUNKS, 1);
    let base = model.paint.base;
    assert!(
        base > base_before,
        "the new visit did not get its own clock base"
    );
    // The hand-off's instant is `base` minus the cover, so the visit's own clock is not yet positive
    // there — which is the saturating subtraction the ROM relies on.
    assert_eq!(
        model
            .paint
            .position(base - SKIP_PRIME_CHUNKS * CHUNK as u32),
        0
    );
    assert_eq!(model.paint.position(base), 0, "not 0 at the track's start");
    assert_eq!(
        model.paint.position(base + LEAD_SAMPLES),
        0,
        "the first audible sample is not at 0"
    );

    // Play this visit out, never producing past the track's own samples.
    while model.paint.position(model.committed) < SKIP_TRACK {
        if model.source.at < model.source.len() && model.feed.free_samples() >= FRAME {
            assert!(model.produce());
        } else {
            model.period();
        }
    }
    assert_eq!(
        model.paint.position(model.committed),
        SKIP_TRACK,
        "the visit did not end on the track's total"
    );
    assert_eq!(
        model.paint.regressions, 0,
        "the painted position ran backwards across a skip"
    );
    assert_eq!(
        model.paint.position(model.committed + CHUNK as u32),
        SKIP_TRACK
    );
}

/// The skip log's `head_silence` is the **new track's own lead-in**, not the hand-off's — and the
/// frame loop's wait, which is the part the app owns, is bounded separately.
///
/// This is the ROM's finding written as a check. Three of that run's seven skips read `to_new = 13`
/// beside a gated `expected 5`, and the two tracks they landed on are the two that carry the album's
/// full master from offset 0: its head is 4,480 samples of digital silence (273 ms at 16,384 Hz), and
/// the first non-zero sample there is a **−1** at index 4,951, inside the ninth chunk, which
/// `fifo_byte`'s `>> 8` turns into `0xFF` — a full-scale-negative byte. So the wire stops calling the
/// track silent exactly 8 periods after its own first chunk. Nothing in the
/// hand-off moved: `content_at` was 5 on every one of the seven. So a track that fades in offends the
/// one question a wire can answer, and `to_new = content_at + head_silence` is a fact about the track.
///
/// What the *app* owns is the press → hand-off wait, and that one is bounded: a pass is one producer
/// frame of decode, the feeder spends one period fading inside it, so the wait is the pass's length in
/// periods and `take_to_handoff_bound(pass)` is the ceiling. The last block shows the bound can fail,
/// which is what makes it a gate rather than a decoration.
#[test]
fn a_skip_onto_a_track_that_opens_silent_reads_as_the_tracks_own_lead_in() {
    const LEAD_CHUNKS: u32 = 3;
    let lead = LEAD_CHUNKS * CHUNK as u32;

    let mut model = Model::album(&[SKIP_TRACK; 3], lead);
    model.prefill();
    assert!(model.prime());
    model.paint.start(SKIP_TRACK, 0);
    model.run_periods(4);
    model.fill_deep();
    let skip = model.press_skip(1, SKIP_PRIME_CHUNKS, 1);

    assert_eq!(
        skip.content_at(),
        SKIP_PRIME_CHUNKS + 1,
        "the machinery's transition moved: it is always the cover plus one"
    );
    assert_eq!(
        skip.head_silence(),
        LEAD_CHUNKS,
        "the wire's delay is not the track's lead-in"
    );
    assert_eq!(skip.to_new(), SKIP_PRIME_CHUNKS + 1 + LEAD_CHUNKS);
    assert_eq!(skip.old(), 0);
    assert_eq!(skip.underflow_delta(), 0);

    // The same head, with nothing skipped: the first non-silent DMA chunk is the same one, so the
    // lead-in belongs to the track and not to the hand-off.
    let mut cold = Model::album(&[SKIP_TRACK], lead);
    cold.prefill();
    assert!(cold.prime());
    cold.paint.start(SKIP_TRACK, 0);
    cold.run_periods(LEAD_CHUNKS + 2);
    let first_audible = cold
        .dma_l
        .chunks(CHUNK)
        .position(|c| c.iter().any(|&b| b != 0))
        .expect("the fake track is not silent forever");
    assert_eq!(
        first_audible as u32, LEAD_CHUNKS,
        "the track's own head is not the lead-in the skip log reported"
    );

    // The frame loop's bound, in the units the ROM gates it in: the wait is the pass's length, and
    // the bound is derived from that same pass (R13: a fixed period count was the emulator's pass
    // rounded up, and would fail a correct slower cart).
    for pass in 1..=6 {
        let mut m = Model::album(&[SKIP_TRACK; 2], 0);
        m.prefill();
        assert!(m.prime());
        m.run_periods(4);
        let skip = m.press_skip(1, SKIP_PRIME_CHUNKS, pass);
        assert_eq!(
            skip.press_to_handoff(),
            pass.max(FADE_PERIODS),
            "a {pass}-period pass did not set the press→hand-off wait (floor: the {FADE_PERIODS}-period deferral)"
        );
        assert!(skip.press_to_handoff() <= take_to_handoff_bound(pass));
    }
    // The bound is exactly one pass plus the deferral, so a slower *machine* stays inside it (that
    // is the fix's purpose) while a loop that needs a *second* pass does not — which is the shape the
    // ROM's `ok()` refuses. Stated arithmetically so the gate is not a decoration.
    assert_eq!(take_to_handoff_bound(6), 7);
    assert!(
        2 * 6 > take_to_handoff_bound(6),
        "a hand-off that needed two passes would pass the one-pass bound"
    );
}

/// A non-last track's end is an **advance**, not the album's end: the producer finishing plus a
/// drained ring posts the next track through the same hand-off a skip uses — with no truncate,
/// because the ring holds nothing stale — and the wire hears no old-track audio after it.
///
/// This is D5's first half on the host, with the real `RingFeed`, the real `Transport` and the
/// fake source. The numbers are the skip ledger's, so the ROM's own `feed:`/`traversal:` lines and
/// this test are one arithmetic rather than two.
#[test]
fn a_track_end_advances_to_the_next_track_with_no_stale_audio_or_underflow() {
    // Longer than the ring (so the transition's own feeding cannot reach the track's end before
    // the wire has resolved) and a whole number of chunks (no classified tail at the visit's end).
    const VISIT: u32 = 117 * CHUNK as u32;
    let mut model = Model::album(&[VISIT, VISIT], 0);
    model.prefill();
    assert!(model.prime());
    model.paint.start(VISIT, 0);
    model.run_periods(3);

    model.play_visit_out();
    let committed_before = model.committed;
    let advance = match model.end_of_visit() {
        VisitEnd::Advance(skip) => skip,
        VisitEnd::AlbumEnd => panic!("a non-last track's end was taken for the album's"),
    };

    assert_eq!(advance.from, 0);
    assert_eq!(advance.to, 1, "the advance did not land on the next track");
    assert_eq!(
        advance.dropped, 0,
        "the advance truncated a ring that had already drained"
    );
    assert_eq!(
        advance.old(),
        0,
        "stale audio from the ending track followed the hand-off"
    );
    assert_eq!(
        advance.underflow_delta(),
        0,
        "the advance booked an underflow: the fade's silence was not a tail"
    );
    assert_eq!(
        advance.content_at(),
        SKIP_PRIME_CHUNKS + 1,
        "the advance's transition is not the skip's cover plus one period"
    );
    // The wire, in bytes: the fade's two periods (the ring was drained, so both are starved
    // silence), the whole cover, and then the new track's first chunk under the declared ramp-in.
    // Nothing of the ending track can appear here — `old == 0` is checked above, but this is the
    // byte-level statement of the same thing.
    let at = advance.at as usize;
    assert_eq!(
        model.dma_l.len() / CHUNK,
        at + 1 + SKIP_PRIME_CHUNKS as usize + 1
    );
    for c in at..at + SKIP_PRIME_CHUNKS as usize + 1 {
        assert_eq!(
            &model.dma_l[c * CHUNK..(c + 1) * CHUNK],
            &vec![0u8; CHUNK][..],
            "chunk {c} of the advance is not silence"
        );
    }
    let first_new = at + SKIP_PRIME_CHUNKS as usize + 1;
    let got = &model.dma_l[first_new * CHUNK..(first_new + 1) * CHUNK];
    for i in 0..CHUNK {
        let k = (i as i32).clamp(0, 256);
        assert_eq!(
            got[i],
            scale_byte(fifo_byte(pattern_sample(i as u32)), k),
            "sample {i} of the advance's new track is not its byte at gain {k}"
        );
    }
    assert_eq!(
        model.feed.pushed(),
        model.feed.consumed() + model.feed.ready()
    );
    assert_eq!(
        model.committed + model.dropped,
        model.feed.consumed() as u32
    );
    assert_eq!(model.underflows, 0);
    assert_eq!(model.source.track, 1);
    assert_eq!(
        model.transport.state(),
        State::Playing,
        "the advance did not finish the transition"
    );
    assert!(
        model.committed > committed_before,
        "the advance handed nothing over"
    );
    // The visit's own timeline: 0 at its new base, and no frame ever painted below the last.
    assert_eq!(model.paint.position(model.paint.base), 0);
    assert_eq!(
        model.paint.regressions, 0,
        "the painted position ran backwards through the advance"
    );
}

/// The last track's end is the album's: it posts `AlbumEnd`, wraps nowhere (§5.3's wrap is the
/// buttons'), and the producer is not restarted on a phantom track 0.
#[test]
fn the_albums_last_track_end_posts_album_end_and_restarts_nothing() {
    const VISIT: u32 = 2 * PREFILL as u32;
    let mut model = Model::album(&[VISIT], 0);
    model.prefill();
    assert!(model.prime());
    model.paint.start(VISIT, 0);
    model.run_periods(2);

    model.play_visit_out();
    assert_eq!(model.end_of_visit(), VisitEnd::AlbumEnd);
    assert_eq!(
        model.source.track, 0,
        "the album-end path wrapped to track 0"
    );
    assert_eq!(model.source.at, VISIT);
    assert_eq!(model.feed.ready(), 0, "the last visit did not drain");
    let frames = model.frames;
    assert!(
        !model.produce(),
        "the producer was restarted after the album's last track"
    );
    assert_eq!(
        model.frames, frames,
        "a frame was decoded after the album's end"
    );
    assert_eq!(
        model.paint.regressions, 0,
        "the painted position ran backwards at the album's end"
    );
}

// ---------------------------------------------------------------------------------------------
// 8f. R13's property fuzz: any press sequence, any spacing (ALBUM-PLAYER.md §14 R13)
// ---------------------------------------------------------------------------------------------
//
// The three scripted scenarios (§8d, §8e and the ROM's REPLAY scripts) each answer a question that
// was already known when the script was written. R13 is the opposite: a human pressed keys in a
// pattern nobody scripted and the cart ended in `FAILED`. So this section drives the same model with
// random press sequences and asserts the properties that must hold for *any* of them — never a
// scenario's numbers. A fuzz over `limit=16` would only re-find the caps §14 already names; the
// properties below are the audio and ledger claims.
//
// A failure here means a transport/ledger property that some press sequence breaks, shrunk to the
// minimal prefix that still fails and printed with the seed that draws it. It is **not** a claim
// about the cartridge: §10.5/R3's discipline is that a green here is a headless green, and AP-7
// owns hardware. What the fuzz cannot reach is stated where it matters — see the note on
// `skip_after_fade`'s `until_wire`, and the two `#[ignore]`d regressions at the end of this section,
// which cover the two defects R13's investigation already proved.

/// `VISIT_LOG` and `SKIP_LOG` (`album_play_main.rs:260,264`): the two bounded logs R13's suspect
/// (b) is about, copied because the ROM's ledger lives in a bin that a host gate cannot link. The
/// next round lifts the ledger into the library, and then these literals go.
const ROM_VISIT_LOG: usize = 24;
const ROM_SKIP_LOG: usize = 16;

/// `WATCH_SLOTS` — the ROM's wire-watch pool, one slot per in-flight transition, and the same depth as
/// `SKIP_LOG` so every transition that can own a row owns its own latch.
const ROM_WATCH_SLOTS: usize = ROM_SKIP_LOG;

/// The fuzz's album: four tracks, two of them not a whole number of chunks so a visit's last period
/// is a classified `Tail` rather than a full chunk. Its length is chosen so a case's script ends at
/// the album's own end (or at [`CASE_FRAMES`]) inside the gate's time budget.
const FUZZ_TRACKS: [u32; 4] = [
    120 * CHUNK as u32,
    96 * CHUNK as u32 + 240,
    132 * CHUNK as u32,
    108 * CHUNK as u32 + 400,
];

/// The video-frame budget one case may spend: the album is sized to end inside it, so a script is
/// clamped rather than cut mid-transition.
const CASE_FRAMES: u32 = 1_200;

/// The widest inter-press gap the script asks for, in video frames — 600 frames ≈ 10 s, six times a
/// track of this album, so a gap that wide straddles a track's end.
const MAX_SPACING: u32 = 600;

/// The step cap: a case that has not stopped by here is wedged, which is a violation, not a budget.
const CASE_STEPS: u32 = 20_000;

/// The fuzz's seeds, and the cases per seed — 8 × 256 = **2,048** cases per run. A failure prints
/// its case's seed and its script, and `XorShift32::new(seed)` draws that script again on any
/// machine: the generator's state is the whole of its state.
const FUZZ_SEEDS: [u32; 8] = [
    0x5EED_0001,
    0x5EED_7A3B,
    0x1BAD_B002,
    0xC0DE_2222,
    0x0FF1_CE03,
    0xD15C_0004,
    0xB0A7_0005,
    0xF00D_0006,
];
const CASES_PER_SEED: u32 = 256;

/// The fuzz's PRNG: xorshift32, host-only, no dependency. §6's no-divide rule is about the ROM's
/// measured paths; this multiplies nothing and divides only here.
struct XorShift32(u32);

impl XorShift32 {
    /// A stream from `seed`. Zero is xorshift's fixed point, so it is remapped rather than rejected
    /// — every seed of [`FUZZ_SEEDS`] (and every per-case seed derived from one) is usable.
    const fn new(seed: u32) -> Self {
        Self(if seed == 0 { 0x9E37_79B9 } else { seed })
    }

    fn next_u32(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x
    }

    /// A value in `0..n`.
    fn below(&mut self, n: u32) -> u32 {
        assert!(n > 0, "an empty range has no draw");
        self.next_u32() % n
    }
}

/// The four presses §5.3 maps to a now-playing action.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Press {
    Next,
    Prev,
    Restart,
    Pause,
}

impl Press {
    fn draw(rng: &mut XorShift32) -> Self {
        match rng.below(4) {
            0 => Press::Next,
            1 => Press::Prev,
            2 => Press::Restart,
            _ => Press::Pause,
        }
    }
}

/// One case: the presses and the video-frame gap before each. `seed` is carried so a violation can
/// print the reproduction without a second argument.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Script {
    seed: u32,
    presses: Vec<Press>,
    spacings: Vec<u32>,
}

impl Script {
    /// Draw a case: 1..=40 presses, and each gap drawn uniformly over 5..=cap where `cap` is the
    /// case budget shared out over the presses (clamped to [`MAX_SPACING`]).
    ///
    /// The sharing is the one compromise this fuzz makes and it is stated rather than hidden: a
    /// 40-press script with 600-frame gaps spans four minutes of audio and would not fit the gate's
    /// 30 s, so a long script gets short gaps (down to 5 frames — shorter than a producer pass and
    /// shorter than a fade) and a short script gets the whole 5..=600 range. Both regimes matter for
    /// R13 and both are covered on every run; which one a given case is in is a property of its
    /// press count, not of chance.
    fn draw(seed: u32) -> Self {
        let mut rng = XorShift32::new(seed);
        let n = 1 + rng.below(40);
        let cap = (CASE_FRAMES / n).clamp(8, MAX_SPACING);
        let mut presses = Vec::with_capacity(n as usize);
        let mut spacings = Vec::with_capacity(n as usize);
        for _ in 0..n {
            presses.push(Press::draw(&mut rng));
            spacings.push(5 + rng.below(cap - 4));
        }
        Self {
            seed,
            presses,
            spacings,
        }
    }

    /// The first `n` presses, with their gaps.
    fn prefix(&self, n: usize) -> Self {
        Self {
            seed: self.seed,
            presses: self.presses[..n].to_vec(),
            spacings: self.spacings[..n].to_vec(),
        }
    }
}

/// One property that did not hold, with the numbers that show it.
struct Violation {
    property: &'static str,
    detail: String,
}

/// What the drive loop must decide after a step — the ROM's frame loop's own two branches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Drive {
    /// A pass or a period was served.
    Stepped,
    /// The visit's producer finished and its ring drained: the loop's end-of-visit decision.
    VisitEnd,
}

/// How a case stopped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stop {
    AlbumEnd,
    /// The case's frame budget, with the script or the album unfinished.
    Budget,
    /// The step cap. A wedge is a violation, not a budget.
    Wedge,
}

/// The transport's verdict on one press, as the ROM's two entries give it.
enum Pressed {
    /// A direction press the transport took: the hand-off it measured.
    Skip(Skip),
    /// A pause: the toggle's `FadeOut`, taken through to the freeze.
    Freeze,
    /// A resume: the toggle's `Prime`, taken through to the re-armed chain.
    Resume,
    /// **PR-3c.** A resume the ring routed to the refill: taken, and the chain is still frozen in
    /// `Buffering`. Its own variant rather than `Resume`, because the property that reads the state after
    /// a taken resume would otherwise be told the chain is running when it is not — and because a run
    /// that met this branch has to say so in a count, not in an absence.
    Refill,
    /// The transport refused it (`Step::Ignored`): counted, never dropped in silence.
    Refused,
}

/// One case's findings and the counts R13's classification reads.
struct CaseReport {
    violations: Vec<Violation>,
    presses: u32,
    accepted: u32,
    refused: u32,
    freeze_presses: u32,
    resume_presses: u32,
    /// **PR-3c.** Resume presses the ring routed to the refill. Counted apart from `resume_presses`
    /// because a green that cannot tell the two routes apart is not evidence that the new one ran.
    refills: u32,
    skip_presses: u32,
    advances: u32,
    /// Pressed skips that legitimately dropped nothing (`truncate()` of a drained ring) — R13's
    /// suspect (c): the ROM gates `dropped` at `1..=19040`, so a correct skip can fail that gate.
    /// Each is kept with the ring depth it found, which is the fact that decides the question.
    dropped_zero: u32,
    dropped_zero_rows: Vec<DroppedZero>,
    /// Skip records whose own latch the wire never resolved: the ROM reads `content_at = 0 − at` for
    /// one, so a non-zero count here is the defect R13's second regression test pins.
    unresolved: u32,
    visits: usize,
    skips: usize,
    /// The run's visit rows, and the producer's frontier: what the ROM's `Visits` totals are summed
    /// from, so a test can apply the ROM's own arithmetic to this run's numbers.
    marks: Vec<VisitMark>,
    content_pushed: u32,
    /// The three figures the hand-off ledger is a sum of: the cover's chunks, the starved periods, and
    /// every hand-off the run made. A test applies the ROM's `expected_hand_offs` to these.
    cover: u32,
    starved: u32,
    hand_offs: u32,
    /// The most transitions that were armed at once — the ROM's wire-watch pool's depth requirement.
    max_open: usize,
}

/// One pressed skip that stepped over no samples, with the ring depth it found. `dropped >= 1` is
/// `SkipRecord::dropped_is_a_truncate`'s claim (`album_play_main.rs:1990`); a skip whose `queued` is
/// under one chunk cannot satisfy it, so these rows are what tells a property from a bound that
/// encodes the gate's own press spacing.
struct DroppedZero {
    seed: u32,
    /// The press's index in its script, and the gap the script gave it, in video frames.
    press: usize,
    gap: u32,
    /// Feeder periods between this hand-off's mark and the previous record's: how deep the ring could
    /// be, given the transition cadence.
    hand_off_gap: u32,
    /// Samples the ring held when the truncate ran.
    queued: u32,
}

impl CaseReport {
    fn new() -> Self {
        Self {
            violations: Vec::new(),
            presses: 0,
            accepted: 0,
            refused: 0,
            freeze_presses: 0,
            resume_presses: 0,
            refills: 0,
            skip_presses: 0,
            advances: 0,
            dropped_zero: 0,
            dropped_zero_rows: Vec::new(),
            unresolved: 0,
            visits: 0,
            skips: 0,
            marks: Vec::new(),
            content_pushed: 0,
            cover: 0,
            starved: 0,
            hand_offs: 0,
            max_open: 0,
        }
    }

    fn violate(&mut self, property: &'static str, detail: String) {
        self.violations.push(Violation { property, detail });
    }
}

impl Model {
    /// One iteration of the ROM's §4.5 frame loop, at the granularity the video clock gives it.
    ///
    /// A producer pass is one frame of decode (one `produce`) plus the feeder periods that elapse
    /// while it runs — [`Self::pass_periods`] of them, which is what makes a pass's cost a parameter
    /// rather than an assumption (R13's calibration question). Every other step is one period, and a
    /// frozen chain costs video frames and nothing else. The gates are the ROM's: the producer runs
    /// only in a feeding state with room for a whole frame, and a drained visit is the loop's own
    /// decision, returned rather than taken so the caller can check the visit's last painted value.
    fn drive_step(&mut self) -> Drive {
        if self.transport.state() == State::Paused {
            self.video_frames += 1;
            return Drive::Stepped;
        }
        // PR-3c: a starved resume's refill at the loop's real cadence — one producer frame per pass, no
        // periods at all (the chain is frozen), and the arm taken by the pass that reaches the target.
        // This lives in the driver and not inline in the press so the fuzz walks the multi-pass shape,
        // including a press landing in the middle of a burst.
        if self.transport.state() == State::Buffering {
            self.video_frames += self.pass_periods * PERIOD_FRAMES;
            if self.feed.ready() >= PREFILL {
                let take = self
                    .resume_open
                    .take()
                    .expect("a refill has a take that its press recorded");
                // §4.4's no-seek invariant, at the moment the arm takes it: the burst changed the ring's
                // depth and nothing else. The clock the ROM would print at the arm is the one the press
                // latched, or the resume has seeked.
                assert_eq!(
                    self.committed, take.head,
                    "a refill moved the session clock {}→{} — that is a seek",
                    take.head, self.committed
                );
                assert_eq!(
                    self.transport.end_buffer(),
                    Step::Prime,
                    "a refill reached its target and was not armed"
                );
                self.prime_resume(take.head, 0);
                let last = self.pauses.last_mut().expect("a refill follows a pause");
                last.refilled = true;
                last.refill_passes = take.passes;
            } else {
                assert!(
                    !self.finished,
                    "a refill entered with the stream finished, so its target can never be reached"
                );
                assert!(
                    self.produce(),
                    "a refill's producer stopped before its target"
                );
                let open = self
                    .resume_open
                    .as_mut()
                    .expect("a refill has a take that its press recorded");
                open.passes += 1;
            }
            return Drive::Stepped;
        }
        if self.source.at == self.source.len()
            && self.feed.ready() == 0
            && self.finished
            && self.transport.state() == State::Playing
        {
            return Drive::VisitEnd;
        }
        if self.transport.state().feeds() && !self.finished && self.feed.free_samples() >= FRAME {
            // The source ending on this pull is the visit's end, not an error: `produce` sets
            // `finished` on the zero-length pull, exactly where the ROM sets it in the last push's
            // own critical section.
            self.produce();
            for _ in 0..self.pass_periods {
                self.period();
            }
        } else {
            self.period();
        }
        Drive::Stepped
    }

    /// Take one press at the top of a pass, and return the transport's own verdict on it.
    ///
    /// The verdict is the transport's, not the driver's: `toggle` and `skip` are the two entries the
    /// ROM has, and their `Ignored` arm is the count of refused presses (§10.5: a silently dropped
    /// press is what the honesty discipline forbids). A direction press is taken as one pass's worth
    /// of decode ([`Self::pass_periods`]) — the ROM's press is at the top of a pass, so that is its
    /// press→hand-off wait.
    fn drive_press(&mut self, press: Press) -> Pressed {
        let wait_for_wire = self.wait_for_wire;
        match press {
            Press::Pause => {
                let at = self.committed;
                let primable = self.ring_primable();
                match self.transport.toggle(true, primable) {
                    Step::FadeOut => {
                        self.fade_to_freeze(at);
                        Pressed::Freeze
                    }
                    Step::Prime => {
                        self.prime_resume(at, 0);
                        Pressed::Resume
                    }
                    Step::Buffer => {
                        // Taken, not refused — the pause's record is the place the difference is kept,
                        // and the head latched here is what the arm is later compared against.
                        //
                        // One case is modelled differently here, on purpose and on the record: with the
                        // stream finished and the ring empty, this model keeps the run in `Buffering` (its
                        // pass budget ends and the press is counted unresolved, which the parity rule
                        // allows), where the ROM ends the album at the press — nothing is left to decode,
                        // so that is §15 Q13's truly-empty case rather than PR-3c's shallow ring, and
                        // §11's PR-3c item keeps it out of scope. `refill_to_target`'s `!finished`
                        // assertion is the same claim one level in: this harness asserts that combination
                        // instead of driving it.
                        self.resume_open = Some(ResumeTake {
                            head: at,
                            passes: 0,
                        });
                        Pressed::Refill
                    }
                    Step::Ignored => Pressed::Refused,
                    other => panic!("a toggle answered {other:?}"),
                }
            }
            _ => {
                if self.transport.skip() != Step::FadeOut {
                    return Pressed::Refused;
                }
                let n = self.source.tracks.len();
                let to = match press {
                    Press::Next => (self.source.track + 1) % n,
                    Press::Prev => (self.source.track + n - 1) % n,
                    Press::Restart => self.source.track,
                    Press::Pause => unreachable!("handled above"),
                };
                Pressed::Skip(self.skip_after_fade(
                    to,
                    SKIP_PRIME_CHUNKS,
                    self.pass_periods,
                    wait_for_wire,
                ))
            }
        }
    }
}

/// Run one script through the model and check every property. Never panics on a property: the
/// violations are collected so the caller can shrink and report them.
///
/// The properties are the §4.4 audio and ledger claims, stated against any sequence: no period
/// starves while the producer owes data, the ring's ledger closes in all three forms, the painted
/// position is monotone and starts at 0 with `n_samples` at a clean end, every press is either taken
/// as exactly one transition or counted as refused, every skip's hand-off lands at the machinery's
/// content point with no wrapped subtraction, and no transition leaves the transport mid-flight.
///
/// One parity question this model *cannot* answer is the ROM's own: it issues exactly one command
/// per press, so "a press that reached the ISR vanished in the slot before the loop took it"
/// (§5.3's seam, `Input::take`) is unreachable here. The model's parity is the other side of it —
/// one press, one record, no press counted twice — and it is checked against the records left
/// behind.
/// `wait_for_wire` is the regime: `true` lets every press wait for its own wire watch, which is what a
/// human's cadence does; `false` lets the next press land as soon as the transport is `Playing` again,
/// which is what the ROM's frame loop actually does and is the window a single global watch could not
/// survive (R13's second defect).
fn run_case(script: &Script, wait_for_wire: bool) -> CaseReport {
    let mut report = CaseReport::new();
    // Half the seeds give every track a silent head, so a skip lands on a track whose own digital
    // silence is indistinguishable from the cover on the wire (`Skip::head_silence`).
    let lead = if script.seed & 1 == 0 {
        0
    } else {
        3 * CHUNK as u32
    };
    let mut model = Model::album(&FUZZ_TRACKS, lead);
    model.wait_for_wire = wait_for_wire;
    model.prefill();
    assert!(model.prime());
    model.open_visit(0);

    // Press `i` is due at the sum of the gaps before it — an absolute frame, so a gap shorter than
    // the transition that precedes it makes the next press land as soon as the transport is settled.
    let mut due = Vec::with_capacity(script.spacings.len());
    let mut at = 0u32;
    for spacing in &script.spacings {
        at += spacing;
        due.push(at);
    }

    let mut next = 0usize;
    let mut stop = None;
    let mut steps = 0u32;
    while stop.is_none() {
        steps += 1;
        if steps > CASE_STEPS {
            report.violate(
                "wedge",
                format!("the run took more than {CASE_STEPS} steps without stopping"),
            );
            stop = Some(Stop::Wedge);
            break;
        }
        if next < script.presses.len() && model.video_frames >= due[next] {
            let press = script.presses[next];
            next += 1;
            report.presses += 1;
            let before = model.transport.state();
            let visits_before = model.visits.len();
            let verdict = model.drive_press(press);
            let after = model.transport.state();
            match verdict {
                Pressed::Refused => {
                    report.refused += 1;
                    if after != before {
                        report.violate(
                            "press-parity",
                            format!("a refused {press:?} moved the transport {before:?}→{after:?}"),
                        );
                    }
                }
                Pressed::Freeze => {
                    report.accepted += 1;
                    report.freeze_presses += 1;
                    if after != State::Paused {
                        report.violate(
                            "transition-state",
                            format!("a pause from {before:?} left {after:?}, not Paused"),
                        );
                    }
                }
                Pressed::Resume => {
                    report.accepted += 1;
                    report.resume_presses += 1;
                    if after != State::Playing {
                        report.violate(
                            "transition-state",
                            format!("a resume from {before:?} left {after:?}, not Playing"),
                        );
                    }
                }
                Pressed::Refill => {
                    report.accepted += 1;
                    report.refills += 1;
                    if after != State::Buffering {
                        report.violate(
                            "transition-state",
                            format!(
                                "a starved resume from {before:?} left {after:?}, not Buffering — the whole point of PR-3c is that the chain stays frozen and the ring is refilled"
                            ),
                        );
                    }
                }
                Pressed::Skip(skip) => {
                    report.accepted += 1;
                    report.skip_presses += 1;
                    if after != State::Playing {
                        report.violate(
                            "transition-state",
                            format!("a skip left the transport {after:?}, not Playing"),
                        );
                    }
                    if model.visits.len() == visits_before {
                        report.violate(
                            "position-start",
                            "an accepted skip opened no visit".to_string(),
                        );
                    } else if model.paint.position(model.paint.base) != 0 {
                        report.violate(
                            "position-start",
                            format!(
                                "a visit started at {} instead of 0",
                                model.paint.position(model.paint.base)
                            ),
                        );
                    }
                    if skip.dropped as usize > RING - FADE_PERIODS as usize * CHUNK {
                        report.violate(
                            "skip-dropped",
                            format!("dropped {} of a {RING}-sample ring", skip.dropped),
                        );
                    }
                    if skip.dropped == 0 {
                        // R13's suspect (c), kept as a row rather than a count: the ROM gates
                        // `dropped >= 1` (`dropped_is_a_truncate`), and a skip pressed onto a ring
                        // shallower than one chunk cannot satisfy that. The depth is the fact that
                        // decides whether the gate is a property or the gate script's own spacing.
                        report.dropped_zero += 1;
                        let previous = model.skips.len().checked_sub(2);
                        report.dropped_zero_rows.push(DroppedZero {
                            seed: script.seed,
                            press: next - 1,
                            gap: script.spacings[next - 1],
                            hand_off_gap: previous
                                .map_or(0, |index| skip.at.saturating_sub(model.skips[index].at)),
                            queued: skip.queued,
                        });
                    }
                }
            }
        } else if next == script.presses.len() && model.video_frames >= CASE_FRAMES {
            stop = Some(Stop::Budget);
        } else {
            match model.drive_step() {
                Drive::Stepped => {}
                Drive::VisitEnd => {
                    // §4.4's end clamp, at the one moment it is a claim about the whole visit: a
                    // drained visit paints its track's total, not a sample short of it.
                    let total = model.source.len();
                    let painted = model.paint.position(model.committed);
                    if painted != total {
                        report.violate(
                            "position-end",
                            format!("a drained visit painted {painted} of {total}"),
                        );
                    }
                    match model.end_of_visit() {
                        VisitEnd::Advance(skip) => {
                            report.advances += 1;
                            if skip.dropped != 0 {
                                report.violate(
                                    "advance-dropped",
                                    format!("an advance truncated {} samples", skip.dropped),
                                );
                            }
                        }
                        VisitEnd::AlbumEnd => stop = Some(Stop::AlbumEnd),
                    }
                }
            }
        }
    }

    // ---- the properties, read off the finished run
    //
    // A case that stopped on its budget is still mid-visit: the ROM closes the visit after the drain
    // that a verdict follows, so the ledger is closed here for every stop rather than only for an
    // album end.
    model.close_visit(model.committed);
    //
    // Every period's three-verdict census closes, and no starved period was unaccounted: `period`
    // books an underflow for every starve the producer still owed data for, so `underflows == 0`
    // is the statement that every silence was a classified `Tail` padding or a track end's fade.
    if model.underflows != 0 {
        report.violate(
            "underflow",
            format!(
                "{} period(s) starved while the producer had work",
                model.underflows
            ),
        );
    }
    if model.periods != model.full_periods + model.tail_periods + model.starved_periods {
        report.violate(
            "period-census",
            format!(
                "{} periods, but {} full + {} tail + {} starved",
                model.periods, model.full_periods, model.tail_periods, model.starved_periods
            ),
        );
    }
    if model.handoffs as u32 != model.primes + model.periods {
        report.violate(
            "hand-off-census",
            format!(
                "{} hand-offs, but {} primes + {} periods",
                model.handoffs, model.primes, model.periods
            ),
        );
    }
    // The ring's ledger closes in all three of its forms. (`period` and `prime` already assert
    // `committed + dropped == consumed` every hand-off — the model's `ledger()` — so that identity
    // is checked 1,000 times a case rather than once.)
    if model.feed.pushed() as u32 != model.content + model.cover {
        report.violate(
            "ring-ledger-content",
            format!(
                "pushed {}, but content {} + cover {}",
                model.feed.pushed(),
                model.content,
                model.cover
            ),
        );
    }
    if model.feed.pushed() != model.feed.consumed() + model.feed.ready() {
        report.violate(
            "ring-ledger-ready",
            format!(
                "pushed {}, but consumed {} + ready {}",
                model.feed.pushed(),
                model.feed.consumed(),
                model.feed.ready()
            ),
        );
    }
    if stop == Some(Stop::AlbumEnd) && model.feed.ready() != 0 {
        report.violate(
            "album-end-ring",
            format!("the album ended with {} samples queued", model.feed.ready()),
        );
    }
    // The per-visit ledger sums to the producer's frontier, over *every* visit — the property the
    // ROM's bounded log has to preserve rather than the truncated sum it computes today.
    let visit_pushed: u32 = model.visits.iter().map(|visit| visit.pushed).sum();
    if visit_pushed != model.content {
        report.violate(
            "visit-ledger",
            format!(
                "visits sum {visit_pushed}, but the producer pushed {}",
                model.content
            ),
        );
    }
    if model.paint.regressions != 0 {
        report.violate(
            "position-monotone",
            format!("{} painted-position regression(s)", model.paint.regressions),
        );
    }
    // The per-visit rows are the ROM's `feed:` ledger: each visit's span is the subtraction
    // `end − base` (`album_play_main.rs:3961`) and its `pushed` is compared against the track's own
    // length (`:3712`, `OVER-PUSHED`). Neither may wrap or overrun.
    for (index, visit) in model.visits.iter().enumerate() {
        if visit.end < visit.base {
            report.violate(
                "visit-wrap",
                format!("visit {index}: base={} end={}", visit.base, visit.end),
            );
        }
        if visit.pushed > FUZZ_TRACKS[visit.track] {
            report.violate(
                "visit-over-pushed",
                format!(
                    "visit {index} pushed {} of track {}'s {} samples",
                    visit.pushed, visit.track, FUZZ_TRACKS[visit.track]
                ),
            );
        }
    }
    // "Stopped while audio is still expected" in its one reachable sense: the album reached its end,
    // so the chain must not be left frozen or mid-transition. A `Paused` state is only ever the
    // listener's, and `Model::period` asserts that no frozen chain is served.
    if stop == Some(Stop::AlbumEnd) && model.transport.state() != State::Playing {
        report.violate(
            "album-end-state",
            format!(
                "the album ended with the transport {:?}",
                model.transport.state()
            ),
        );
    }
    // The ROM's wire-watch pool is `WATCH_SLOTS` slots deep, one per in-flight transition. The model
    // keeps every record, so this is the parity check that the pool is deep enough for any sequence the
    // corpus reaches: a deeper overlap would be a named refusal on a cart (`transition_hand_off`), not
    // a silent overwrite, and this is what says that refusal is unreachable in practice.
    if model.max_open > ROM_WATCH_SLOTS {
        report.violate(
            "watch-pool",
            format!(
                "{} transitions were in flight at once, past the ROM's {ROM_WATCH_SLOTS} wire-watch \
                 slots",
                model.max_open
            ),
        );
    }
    // Every skip's measurement is a subtraction the ROM performs with `saturating_sub`, so each is
    // checked in the form that cannot hide a wrap: the raw fields must be ordered.
    //
    // A record still waiting when the case stopped is expected — the run was cut inside a transition,
    // before the new track's audio could reach the wire — and it is counted rather than failed. A
    // record left waiting **while a later one resolved** is the defect: the scan is shared, so audio
    // reaching the wire after a later mark would have resolved the earlier record too. That is the
    // signature of one latch re-armed by every hand-off (R13's second defect).
    for (index, skip) in model.skips.iter().enumerate() {
        if skip.open {
            report.unresolved += 1;
            if model.skips[index + 1..].iter().any(|later| !later.open) {
                report.violate(
                    "skip-unresolved",
                    format!(
                        "skip {index} ({}→{}) was still waiting for the wire while a later record \
                         had resolved: its own latch was orphaned, and `content_at()` would read \
                         `0 − {}`",
                        skip.from, skip.to, skip.at
                    ),
                );
            }
            continue;
        }
        if skip.silent_at < skip.at || skip.new_at < skip.silent_at || skip.content_at < skip.at {
            report.violate(
                "skip-wrap",
                format!(
                    "skip {index}: at={} silent_at={} new_at={} content_at={} is not ordered",
                    skip.at, skip.silent_at, skip.new_at, skip.content_at
                ),
            );
        }
        if skip.content_at() != SKIP_PRIME_CHUNKS + 1 {
            report.violate(
                "skip-content-point",
                format!(
                    "skip {index}: content_at() = {} periods, not {}",
                    skip.content_at(),
                    SKIP_PRIME_CHUNKS + 1
                ),
            );
        }
        if skip.underflow_delta() != 0 {
            report.violate(
                "skip-underflow",
                format!(
                    "skip {index} booked {} underflow(s)",
                    skip.underflow_delta()
                ),
            );
        }
    }
    // Parity, counted from the other side: every accepted press left exactly one record and no
    // accepted press left two. (The model issues exactly one command per press, so a press vanishing
    // in the ISR slot — the ROM's own parity question — is not reachable from here; see `run_case`'s
    // doc.)
    if model.pauses.len() as u32 != report.freeze_presses {
        report.violate(
            "press-parity",
            format!(
                "{} freeze(s), {} pause pair(s)",
                report.freeze_presses,
                model.pauses.len()
            ),
        );
    }
    // Every freeze is resumed except at most the one the run may end in: no pause vanishes, and
    // none is counted twice. PR-3c adds a second route to the same end, so a take that the ring sent to
    // the refill counts towards the same total — and a run that ends inside a burst leaves one open in
    // `Buffering`, exactly as one that ends paused leaves one open in `Paused`.
    let unresolved = u32::from(matches!(
        model.transport.state(),
        State::Paused | State::Buffering
    ));
    if report.resume_presses + report.refills != report.freeze_presses - unresolved {
        report.violate(
            "press-parity",
            format!(
                "{} freeze(s), {} resume(s) + {} refill(s), ending {:?}",
                report.freeze_presses,
                report.resume_presses,
                report.refills,
                model.transport.state()
            ),
        );
    }
    let skip_rows = model.skips.len() - report.advances as usize;
    if skip_rows as u32 != report.skip_presses {
        report.violate(
            "press-parity",
            format!(
                "{} skip press(es), {skip_rows} skip row(s)",
                report.skip_presses
            ),
        );
    }
    report.visits = model.visits.len();
    report.skips = model.skips.len();
    report.marks = model.visits.clone();
    report.content_pushed = model.content;
    report.cover = model.cover;
    report.starved = model.starved_periods;
    report.hand_offs = model.handoffs;
    report.max_open = model.max_open;
    report
}

/// `run_case`, with a model assertion turned into a violation rather than a panic: a failed
/// `assert!` inside the model is a finding too, and it must carry its seed like any other.
fn run_case_or_panic(script: &Script, wait_for_wire: bool) -> CaseReport {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        run_case(script, wait_for_wire)
    })) {
        Ok(report) => report,
        Err(payload) => {
            let mut report = CaseReport::new();
            report.violate("model-panic", panic_message(&*payload));
            report
        }
    }
}

/// A caught panic's message, in the form `panic!` was given it.
fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_string()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "<a panic payload that is not a string>".to_string()
    }
}

/// The minimal prefix of `script` that still fails: presses dropped from the end while the prefix
/// still fails, then each spacing pulled back to the script's floor. `fails` is supplied so the
/// shrinker itself is testable against a predicate whose answer is known.
fn shrink(script: &Script, fails: &dyn Fn(&Script) -> bool) -> Script {
    let mut best = script.clone();
    loop {
        let mut reduced = false;
        while best.presses.len() > 1 {
            let shorter = best.prefix(best.presses.len() - 1);
            if fails(&shorter) {
                best = shorter;
                reduced = true;
            } else {
                break;
            }
        }
        for index in 0..best.spacings.len() {
            if best.spacings[index] == 5 {
                continue;
            }
            let mut tighter = best.clone();
            tighter.spacings[index] = 5;
            if fails(&tighter) {
                best = tighter;
                reduced = true;
            }
        }
        if !reduced {
            return best;
        }
    }
}

/// R13's classification, aggregated over the run: how many cases would trip a *measurement* (b) or
/// a *scenario-encoded bound* (c) rather than the audio property (a) the fuzz asserts. Nothing here
/// fails the test — it is the census that says whether the two non-audio suspects are reachable at
/// all, which is what separates "a transport defect" from "a verdict that outran its witness".
#[derive(Default)]
struct Census {
    cases: u32,
    presses: u32,
    accepted: u32,
    refused: u32,
    max_visits: usize,
    max_skips: usize,
    visit_overflow: u32,
    skip_overflow: u32,
    dropped_zero: u32,
    /// Every one of those, with the ring depth and the spacing the press had: the rows that tell a
    /// property (`dropped >= 1`) from a bound shaped by the gate script's cadence.
    dropped_zero_rows: Vec<DroppedZero>,
    /// Records still waiting for their own latch when the case stopped: legitimate (a case can be cut
    /// inside a transition), so this is reported and not asserted — `skip-unresolved` asserts the
    /// defect, which is an earlier record orphaned while a later one resolved.
    unresolved: u32,
    /// The deepest overlap any case reached: the wire-watch pool's depth requirement, in slots.
    max_open: usize,
}

impl Census {
    fn observe(&mut self, report: &CaseReport) {
        self.cases += 1;
        self.presses += report.presses;
        self.accepted += report.accepted;
        self.refused += report.refused;
        self.max_visits = self.max_visits.max(report.visits);
        self.max_skips = self.max_skips.max(report.skips);
        if report.visits > ROM_VISIT_LOG {
            self.visit_overflow += 1;
        }
        if report.skips > ROM_SKIP_LOG {
            self.skip_overflow += 1;
        }
        self.dropped_zero += report.dropped_zero;
        self.dropped_zero_rows
            .extend(report.dropped_zero_rows.iter().map(|row| DroppedZero {
                seed: row.seed,
                press: row.press,
                gap: row.gap,
                hand_off_gap: row.hand_off_gap,
                queued: row.queued,
            }));
        self.unresolved += report.unresolved;
        self.max_open = self.max_open.max(report.max_open);
    }
}

/// One pass over the whole corpus at one press regime: the census, and every finding shrunk to the
/// minimal prefix that still fails.
///
/// The corpus does not depend on the regime — the same `FUZZ_SEEDS`/`CASES_PER_SEED` scripts are
/// replayed both ways — so a finding is comparable across the two passes and a printed seed names one
/// script, not one script per mode.
fn fuzz(wait_for_wire: bool) -> (Census, Vec<(u32, Violation, Script)>) {
    let mut census = Census::default();
    let mut failures: Vec<(u32, Violation, Script)> = Vec::new();
    for seed in FUZZ_SEEDS {
        for case in 0..CASES_PER_SEED {
            // A per-case seed in the table's own family, so a printed seed is one of a bounded set
            // of streams rather than a value the run invented.
            let case_seed = seed.wrapping_add(case.wrapping_mul(0x9E37_79B9));
            let script = Script::draw(case_seed);
            let report = run_case_or_panic(&script, wait_for_wire);
            census.observe(&report);
            if let Some(violation) = report.violations.into_iter().next() {
                // A panic is reported where it stands: shrinking it would re-panic once per
                // candidate and bury the finding in the hook's output.
                let shrunk = if violation.property == "model-panic" {
                    script.clone()
                } else {
                    shrink(&script, &|candidate| {
                        !run_case_or_panic(candidate, wait_for_wire)
                            .violations
                            .is_empty()
                    })
                };
                failures.push((case_seed, violation, shrunk));
            }
        }
    }
    (census, failures)
}

/// Emulator-measured timing, named with the line that declares it and the unit it is expressed in.
/// These are the figures a cartridge can legitimately differ on: each one is a *measurement*, not a
/// product constant (§10.5: every timing figure in cycles; R3: an emulator figure is not a hardware
/// claim). Nothing here changes any of them — the test only says what flips if they are wrong.
///
/// `VIDEO_PERIOD_CYCLES` — `album_play_main.rs:618`, cycles per vblank (59.73 Hz, AP-0).
const ROM_VIDEO_PERIOD_CYCLES: u32 = 280_896;
/// The producer frame the old `MAX_PASS_FRAMES` was derived from: 1.04–1.09 M cycles of
/// uninterruptible decode, measured on mGBA (`album_play_main.rs`'s `PASS_MARGIN_CYCLES` docs).
const ROM_PASS_CYCLES: u32 = 1_090_000;
/// The **removed** `MAX_PASS_FRAMES = 4 + 1 + 1` — the emulator-derived ceiling the bounds round
/// took out of the ROM. Kept here only so the finding's before-picture stays checkable.
const ROM_OLD_MAX_PASS_FRAMES: u32 = 6;
/// The gate run's own `pass_max` (`album-test.log:33`, `bound 5`).
const ROM_MEASURED_PASS_FRAMES: u32 = 5;
/// The gate run's own vblank-handler cost (`album-test.log:31`, `worst=1 samples`).
const ROM_MEASURED_HANDLER_SAMPLES: u32 = 1;

#[test]
fn every_press_sequence_holds_the_transports_properties() {
    // The PRNG's reproducibility, which is what makes a printed seed a reproduction: the same seed
    // draws the same script, and different seeds draw different ones.
    assert_eq!(Script::draw(0x5EED_0001), Script::draw(0x5EED_0001));
    assert_ne!(Script::draw(0x5EED_0001), Script::draw(0x5EED_0002));

    // The shrinker, against a predicate whose answer is known: when only the press count matters,
    // the minimal script is the first three presses at the gap floor.
    let long = Script {
        seed: 0,
        presses: vec![Press::Next; 6],
        spacings: vec![40, 41, 42, 43, 44, 45],
    };
    let minimal = shrink(&long, &|candidate| candidate.presses.len() >= 3);
    assert_eq!(minimal.presses.len(), 3);
    assert!(minimal.spacings.iter().all(|&gap| gap == 5));

    // Two regimes over the same corpus, because R13's hardware session was in the second one and the
    // first cannot reach it. `true` is a human cadence: every press waits for its own wire watch to
    // resolve, which is what makes every number a reading rather than a prediction. `false` is the
    // ROM's frame loop: the next press lands as soon as the transport is `Playing` again, which is
    // `head_silence` periods *before* that watch resolves — the window a single global watch could
    // not survive (R13's second proven defect), and the one this fuzz previously could not enter.
    for (wait_for_wire, regime) in [
        (true, "press waits for the wire"),
        (false, "press on the transport, no wire wait"),
    ] {
        let (census, failures) = fuzz(wait_for_wire);
        assert_eq!(
            census.cases,
            (FUZZ_SEEDS.len() as u32) * CASES_PER_SEED,
            "the fuzz must run every case it claims [{regime}]"
        );
        assert!(census.cases >= 2_000);

        // The census is the finding, not the assertion: (a) is the property set, and these counts
        // are R13's (b) a measurement overflowing and (c) a bound that encodes the script.
        eprintln!(
            "R13 fuzz [{regime}]: {} cases, {} presses ({} accepted, {} refused); visits max {}, \
             skips max {}; past VISIT_LOG={ROM_VISIT_LOG}: {} case(s); past SKIP_LOG={ROM_SKIP_LOG}: \
             {} case(s); pressed skips that dropped 0: {}; skip rows still waiting at the stop: {}; \
             most transitions in flight at once: {} of {ROM_WATCH_SLOTS} slot(s)",
            census.cases,
            census.presses,
            census.accepted,
            census.refused,
            census.max_visits,
            census.max_skips,
            census.visit_overflow,
            census.skip_overflow,
            census.dropped_zero,
            census.unresolved,
            census.max_open
        );
        // Every pressed skip that stepped over nothing, with the ring depth it found. R13's suspect
        // (c) is the bounds round's question — `dropped >= 1` is a bound, so these rows are printed
        // rather than asserted on here.
        for row in &census.dropped_zero_rows {
            eprintln!(
                "R13 fuzz [{regime}] dropped=0: seed {:#010X} press {} gap {} frames, {} period(s) \
                 after the previous hand-off; the ring held {} whole chunk(s) + {} sample(s)",
                row.seed,
                row.press,
                row.gap,
                row.hand_off_gap,
                row.queued / CHUNK as u32,
                row.queued % CHUNK as u32
            );
        }
        // If no case reached the caps, this run says nothing about suspect (b) — the census would be
        // blind rather than clean, and that is worth failing over.
        assert!(
            census.visit_overflow > 0 && census.skip_overflow > 0,
            "no case in {} exceeded VISIT_LOG={ROM_VISIT_LOG}/SKIP_LOG={ROM_SKIP_LOG}, so the census \
             is blind [{regime}]: visits max {}, skips max {}",
            census.cases,
            census.max_visits,
            census.max_skips
        );
        // The property R13's second defect broke is `skip-unresolved` in the set above: a record
        // orphaned behind a later, resolved one. A record still waiting at the stop is *not* a
        // finding — a case can be cut inside a transition — so `census.unresolved` is printed with
        // the rows and not asserted on.

        let mut message = String::new();
        for (seed, violation, shrunk) in &failures {
            message.push_str(&format!(
                "\nseed {seed:#010X} property {}: {}\n  shrunk to {} press(es) {:?} at gaps {:?}",
                violation.property,
                violation.detail,
                shrunk.presses.len(),
                shrunk.presses,
                shrunk.spacings
            ));
        }
        assert!(
            failures.is_empty(),
            "{} case(s) broke a property [{regime}]:{message}",
            failures.len()
        );
    }
}

#[test]
fn a_slower_pass_no_longer_flips_the_pass_cost_terms() {
    // R13's finding, made arithmetic — and then the fix, made arithmetic. A pass cost used to enter
    // two gated terms as a **frame count measured on mGBA**:
    //
    //   `MAX_PASS_FRAMES = ceil(1.09 M c / 280,896 c) + 2 edges = 4 + 2 = 6`
    //   `SKIP_PRESS_TO_HANDOFF_PERIODS = 3` (the same 1.85–1.9-period pass, rounded up)
    //
    // The old ceiling has *no* arithmetic margin at 1× (the gate run only measures 5 because its
    // passes averaged 1.04–1.09 M c and its edges overlapped), so a decode even slightly slower than
    // the emulator's reaches 7 frames and fails `reaction` — and a cartridge has wait states mGBA
    // does not (R3). That is the false red this round removes: the bound is now the run's own
    // measured producer frame plus one period and one vblank, all in cycles, so it scales with the
    // machine.
    assert_eq!(
        ROM_PASS_CYCLES.div_ceil(ROM_VIDEO_PERIOD_CYCLES) + 2,
        ROM_OLD_MAX_PASS_FRAMES,
        "the arithmetic at 1x must saturate the old ceiling, which is the finding"
    );
    assert!(ROM_MEASURED_PASS_FRAMES <= ROM_OLD_MAX_PASS_FRAMES);
    // The pass's non-decode work, in the ROM's own derivation: the frame slice the loop serves at the
    // top of every pass, bounded by one vblank (`PASS_MARGIN_CYCLES`). Modelling it as one video
    // frame is the conservative end of that bound.
    let service = ROM_VIDEO_PERIOD_CYCLES;
    let margin = ROM_VIDEO_PERIOD_CYCLES;
    for (scale, old_reaction, old_skip, reaction, skip, starves) in [
        (1u32, true, true, true, true, false),
        (2, false, false, true, true, true),
        (3, false, false, true, true, true),
        (5, false, false, true, true, true),
    ] {
        let frame = ROM_PASS_CYCLES * scale;
        let pass = frame + service;
        let periods = frame.div_ceil(geometry::PERIOD_CYCLES);
        let frames = frame.div_ceil(ROM_VIDEO_PERIOD_CYCLES) + 2;
        // The old, emulator-derived terms — printed and asserted only where they flipped, so the fix
        // has its before-picture. They are no longer in the ROM at all.
        assert_eq!(
            frames <= ROM_OLD_MAX_PASS_FRAMES,
            old_reaction,
            "{scale}× pass = {frames} video frames against the removed MAX_PASS_FRAMES = \
             {ROM_OLD_MAX_PASS_FRAMES}"
        );
        assert_eq!(
            periods.max(FADE_PERIODS) <= 3,
            old_skip,
            "{scale}× pass = {periods} periods against the removed fixed bound of 3"
        );
        // The new bound: the run's measured worst producer frame + one period + one vblank, in
        // cycles. It holds at every scale because the measured frame is *inside* the bound.
        let bound = frame + geometry::PERIOD_CYCLES + margin;
        assert_eq!(
            pass <= bound,
            reaction,
            "{scale}× pass = {pass}c against its own bound {bound}c"
        );
        // The skip's bound is the measured pass in periods plus the deferral; a 2× pass gets 5
        // periods rather than the old refusal at 4.
        let skip_bound = take_to_handoff_bound(periods);
        assert_eq!(
            periods.max(FADE_PERIODS) <= skip_bound,
            skip,
            "{scale}× pass = {periods} periods against take_to_handoff_bound = {skip_bound}"
        );

        // The audio consequence is **not** a bound and does not move with one: a pass pushes FRAME
        // samples and spans `periods` periods, so the ring's steady state needs
        // `FRAME ≥ periods × CHUNK`. Below that it drains at a fixed rate and the feeder books
        // underflows. That is the term a cartridge's slower decode should still flip — an audio
        // fact, witnessed by the model, not a frame count measured on the wrong box.
        let mut model = Model::album(&[600 * CHUNK as u32], 0);
        model.pass_periods = periods;
        model.prefill();
        assert!(model.prime());
        model.open_visit(0);
        let mut drained = false;
        while !drained && model.underflows == 0 {
            match model.drive_step() {
                Drive::Stepped => {}
                Drive::VisitEnd => drained = true,
            }
        }
        assert_eq!(
            model.underflows > 0,
            starves,
            "{scale}× pass: {} underflow(s) after {} passes over {} of {} samples",
            model.underflows,
            model.frames,
            model.source.at,
            model.source.len()
        );
        eprintln!(
            "R13 calibration: {scale}x pass = {frame}c decode + {service}c service = {pass}c = \
             {periods} period(s) = {} frames; old: reaction={old_reaction} skip={old_skip}; \
             new: reaction={reaction} (bound {bound}c) skip={skip} (bound {skip_bound}p); \
             ring_keeps_up={} ({} underflow(s), ring first spent at sample {} of {})",
            pass.div_ceil(ROM_VIDEO_PERIOD_CYCLES),
            !starves,
            model.underflows,
            model.source.at,
            model.source.len()
        );
    }

    // The two terms a pass does **not** touch, stated so the next round does not confuse them. The
    // sample is the vblank ISR's, so `cadence` sees one frame per vblank whatever the decode does;
    // and `isr` is the handler's own cost in samples of the audio clock, measured at 1 of the FIFO's
    // 16 — sixteen times its headroom, not two.
    assert_eq!(ROM_MEASURED_HANDLER_SAMPLES, 1);
    assert!(16 * ROM_MEASURED_HANDLER_SAMPLES <= geometry::REARM_GRACE_SAMPLES);
    // The audit's two terms belong to the *period service*, in samples: `late` past the 16-sample
    // grace and `coalesced` past a whole 560-sample period. Against the section's own ~1% estimate
    // of a period, 2× and 3× stay inside the grace; 5× crosses it — so `audit`, not `reaction`, is
    // what a slower *feeder* would flip.
    let estimate = geometry::CHUNK_SAMPLES / 100;
    assert!(3 * estimate <= geometry::REARM_GRACE_SAMPLES);
    assert!(5 * estimate > geometry::REARM_GRACE_SAMPLES);
    assert!(5 * estimate < geometry::CHUNK_SAMPLES);
}

/// **R13's first proven defect: the bounded log's totals.**
///
/// `Visits::pushed_total()` used to sum `self.list()`, i.e. only the rows that fitted in
/// `VISIT_LOG` = 24; `Visits::begin` counted the overflow and returned, and `Visits::close` wrote the
/// totals into the newest row only. The verdict compared that truncated sum against the ring's own
/// counter (`visits_ok`) and summed `expected_hand_offs` the same way, so a session with more than 24
/// visits reported *two* failures where the audio was intact — exactly R13's suspect (b).
///
/// The fix is on the ROM side: `Visits::begin`/`close` accumulate running totals over every visit and
/// the two verdict terms read those, so the log's depth is a print bound and nothing else. This test
/// applies the ROM's arithmetic to a session of 32 visits and 30 skips, and asserts the half that was
/// red — the truncated sums — is what it used to be, while the running totals close.
///
/// The run is sound: `run_case` asserts every audio and ledger property, so a failure here is the
/// ledger's arithmetic and nothing else.
#[test]
fn a_session_past_the_logs_totals_like_the_verdict_expects() {
    // 30 Next presses at a 12-frame cadence: more visits than VISIT_LOG and more skips than
    // SKIP_LOG, on an album whose tracks are long enough that no visit drains on its own.
    let script = Script {
        seed: 0,
        presses: vec![Press::Next; 30],
        spacings: vec![12; 30],
    };
    let report = run_case(&script, true);
    assert!(
        report.violations.is_empty(),
        "the session itself is not sound, so this test would not be about the log: {:?}",
        report
            .violations
            .iter()
            .map(|violation| (violation.property, violation.detail.as_str()))
            .collect::<Vec<_>>()
    );
    assert!(
        report.marks.len() > ROM_VISIT_LOG && report.skips > ROM_SKIP_LOG,
        "the session made {} visit(s) and {} skip row(s); the test needs more than \
         {ROM_VISIT_LOG}/{ROM_SKIP_LOG}",
        report.marks.len(),
        report.skips
    );

    // The ROM's arithmetic on this run's own rows, both halves of it. The truncated sums are the
    // pre-fix behaviour, kept as the red half: they must *not* close, or this test would not be about
    // the log's depth at all.
    let spans = |rows: &[VisitMark]| -> u32 {
        rows.iter()
            .map(|visit| (visit.end - visit.base).div_ceil(CHUNK as u32))
            .sum()
    };
    let kept: u32 = report.marks[..ROM_VISIT_LOG]
        .iter()
        .map(|mark| mark.pushed)
        .sum();
    let running: u32 = report.marks.iter().map(|mark| mark.pushed).sum();
    let kept_hand_offs =
        report.cover / CHUNK as u32 + spans(&report.marks[..ROM_VISIT_LOG]) + report.starved;
    let running_hand_offs = report.cover / CHUNK as u32 + spans(&report.marks) + report.starved;
    assert_ne!(
        kept,
        running,
        "the session of {} visits must push more past VISIT_LOG={ROM_VISIT_LOG} than inside it, or \
         the two sums coincide and this test witnesses nothing",
        report.marks.len()
    );
    // The running total `Visits::close` accumulates: `visits_ok`'s left-hand side.
    assert_eq!(
        running, report.content_pushed,
        "the per-track ledger's running total is {running}, but the producer pushed {} — the sum \
         must be over every visit, not over the {ROM_VISIT_LOG} rows that fit in VISIT_LOG (which \
         reads {kept})",
        report.content_pushed
    );
    // And the hand-off ledger's per-visit half, the same way: `expected_hand_offs` adds
    // `ceil(span / CHUNK)` for every visit, including the ones the log could not keep.
    assert_ne!(
        kept_hand_offs, running_hand_offs,
        "the truncated hand-off sum must differ from the running one"
    );
    assert_eq!(
        running_hand_offs,
        report.hand_offs,
        "the hand-off ledger's per-visit sum over every visit is {running_hand_offs}, but the run \
         made {} hand-off(s): cover {} + visits {} + starved {} (the log's own rows alone read \
         {kept_hand_offs})",
        report.hand_offs,
        report.cover / CHUNK as u32,
        spans(&report.marks),
        report.starved
    );
}

/// **R13's second proven defect: a skip record whose own watch was overwritten.**
///
/// The ROM kept **one** wire watch (`stats.watch_*`), re-armed by every hand-off, and `Skips::resolve`
/// wrote that watch's reading into `records[recorded - 1]` — the newest open record. The transport is
/// `Playing` again as soon as the ramp back completes, but the watch cannot resolve until the new
/// track's own audio reaches the wire. On a track that opens on digital silence (three chunks here;
/// eight periods on the gate fixture's full-master tracks) those are different moments, so a second
/// press inside that window re-armed the watch and the first record stayed `open` with
/// `content_at = 0` — and `content_at()` was `0.wrapping_sub(at)`, the huge figure in the ROM log.
/// `Skips::ok()` requires `!self.open()`, so the run failed with no audio wrong.
///
/// The fix is one wire-watch slot **per in-flight record** (the ROM) and one latch per record (this
/// model): every recorded transition resolves from its own latch, and `content_at()` saturates rather
/// than wrapping, so a future ordering mistake reads as `0` — printed as `unresolved` — and never as
/// 4,294,966,995. The second `skip_after_fade(…, false)` below stops exactly where the ROM's frame
/// loop is free to take the next press.
#[test]
fn a_second_skip_keeps_the_first_hand_offs_own_content_point() {
    let lead = 3 * CHUNK as u32;
    let mut model = Model::album(&FUZZ_TRACKS, lead);
    model.prefill();
    assert!(model.prime());
    model.open_visit(0);

    // Skip 1, to the point where the transport is Playing and the wire has not settled.
    assert_eq!(
        model.transport.skip(),
        Step::FadeOut,
        "a skip from Playing must fade out"
    );
    let first = model.skip_after_fade(1, SKIP_PRIME_CHUNKS, 2, false);
    assert!(
        first.open,
        "the first watch resolved before the window opened, so this test's premise is wrong"
    );
    assert_eq!(
        model.transport.state(),
        State::Playing,
        "a second press needs the transport Playing, as the ROM's loop sees it"
    );

    // Skip 2, inside the window: the second hand-off re-arms the one watch.
    assert_eq!(model.transport.skip(), Step::FadeOut);
    model.skip_after_fade(2, SKIP_PRIME_CHUNKS, 2, true);

    assert_eq!(model.skips.len(), 2);
    let second = model.skips[1];
    assert!(!second.open, "the second skip never resolved");
    assert_eq!(
        second.content_at(),
        SKIP_PRIME_CHUNKS + 1,
        "the second skip's own content point is not the machinery's"
    );

    let first = model.skips[0];
    assert!(
        !first.open && first.new_at != 0,
        "the first skip's record never resolved: content_at={}, so content_at() would read 0 − {} \
         and `Skips::ok()` would be false for a run whose audio is intact — a later hand-off must not \
         retire or orphan an earlier record's own latch",
        first.content_at,
        first.at
    );
}
