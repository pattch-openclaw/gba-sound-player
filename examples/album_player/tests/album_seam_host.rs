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
    BTN_UP, Cursor, DEBOUNCE_FRAMES, KeySource, Keys, REPEAT_DELAY_FRAMES, REPEAT_INTERVAL_FRAMES,
    Replay, SOFT_RESET_CHORD,
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
    // §5.3's `—`: Up, B and Select bind nothing while a track plays.
    for unbound in [BTN_UP, BTN_B, BTN_SELECT, 0] {
        assert_eq!(keys::now_playing_action(unbound), None);
    }
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
/// `const` is not importable. Its end-of-run line claims **3 edges and 7 autorepeats**; this test is
/// what makes that claim arithmetic rather than hope. If the ROM's script changes, this one changes
/// with it — the numbers in the log are checked against this shape.
const ROM_REPLAY_SCRIPT: [(u32, u16); 6] = [
    (8, keys::BTN_A),
    (14, 0),
    (24, keys::BTN_RIGHT),
    (30, 0),
    (40, keys::BTN_LEFT),
    (120, 0),
];

#[test]
fn the_rom_replay_script_delivers_three_edges_and_seven_repeats() {
    let mut source = Replay::new(&ROM_REPLAY_SCRIPT);
    let mut keys = Keys::new();
    let mut press_frames = Vec::new();
    let mut repeat_frames = Vec::new();
    for frame in 0..130u16 {
        let sampled = keys.sample(source.sample());
        if sampled.pressed != 0 {
            press_frames.push(frame);
        }
        if sampled.repeated != 0 {
            repeat_frames.push(frame);
        }
    }
    // One frame after each scripted entry (8, 24, 40), and nothing for the releases.
    assert_eq!(press_frames, vec![9, 25, 41]);
    assert_eq!(source.edges(), 3);
    // Left is held from frame 41 to 120: first repeat REPEAT_DELAY_FRAMES later, then every
    // REPEAT_INTERVAL_FRAMES until the release at 120 clears the burst.
    assert_eq!(repeat_frames[0], 41 + REPEAT_DELAY_FRAMES);
    assert_eq!(repeat_frames, vec![65, 73, 81, 89, 97, 105, 113]);
    assert_eq!(source.frame(), 130);
}

/// §4.5's loop polls on the frame latch, so a scripted press stamped at frame `F` is *latched* at
/// `F` and delivered to the application at `F + DEBOUNCE_FRAMES - 1` = `F + 1`: **one video frame
/// from latch to delivery**, which is the bound §4.4's "within one video frame" observable needs
/// from this layer. The ROM's loop can only be slower than this — it polls on the latch and spends
/// up to one producer slice inside a decode — and it can never be faster, which is the direction
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
    let mut machine = Keys::new();
    let mut delivered = Vec::new();
    for frame in 0..140u32 {
        if machine.sample(source.sample()).action().is_some() {
            delivered.push(frame);
        }
    }
    assert_eq!(
        delivered.len(),
        stamps.len(),
        "the script presses {} times but {} actions were delivered",
        stamps.len(),
        delivered.len()
    );
    for (frame, stamp) in delivered.iter().zip(&stamps) {
        assert!(
            *frame > *stamp,
            "an action was delivered at {frame}, before its scripted latch at {stamp}"
        );
        assert!(
            *frame - *stamp <= LATCH_TO_DELIVERY,
            "the press latched at {stamp} was delivered at {frame}: more than {LATCH_TO_DELIVERY} \
             frame(s) from latch to delivery"
        );
    }
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
