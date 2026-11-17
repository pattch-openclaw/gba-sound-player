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
    FADE_SAMPLES, GAIN_UNITY, HandOffGain, State, Step, Transport, scale_byte,
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

/// A `PcmSource` with no codec: one track of `total` samples, frames of `FRAME_SAMPLES`, and the
/// closed form above. It is the model's producer input, so the seam is exercised through its real
/// trait rather than through a special-shaped helper.
struct FakeSource {
    at: u32,
    total: u32,
}

impl FakeSource {
    fn new(total: u32) -> Self {
        Self { at: 0, total }
    }
}

impl PcmSource for FakeSource {
    fn n_tracks(&self) -> usize {
        1
    }

    fn track(&self, index: usize) -> Option<TrackMeta> {
        (index == 0).then_some(TrackMeta {
            start_sample: 0,
            n_samples: self.total,
            title: "fake",
            artist: "",
        })
    }

    fn album(&self) -> AlbumMeta {
        AlbumMeta {
            title: "fake",
            artist: "",
            year: 0,
            n_samples: self.total,
            art: None,
        }
    }

    fn rate_hz(&self) -> u32 {
        geometry::RATE_HZ
    }

    fn start_track(&mut self, index: usize) -> Result<(), SourceError> {
        if index != 0 {
            return Err(SourceError::NoSuchTrack { index, n_tracks: 1 });
        }
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
        let n = ((self.total - self.at) as usize).min(FRAME);
        for i in 0..n {
            let value = pattern_sample(self.at + i as u32) as i16;
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
}

impl Pause {
    /// Samples lost across the freeze: zero by contract, because the ring was never touched.
    fn dropped(&self) -> u32 {
        self.resumed_from.saturating_sub(self.frozen_committed)
    }
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
    total: u32,
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
}

impl Model {
    fn new(total: u32) -> Self {
        Self {
            feed: RingFeed::new(),
            transport: Transport::new(),
            source: FakeSource::new(total),
            interleaved: vec![0i16; 2 * FRAME],
            left: vec![0i32; FRAME],
            right: vec![0i32; FRAME],
            bytes_l: vec![0u8; FRAME],
            bytes_r: vec![0u8; FRAME],
            frames: 0,
            total,
            finished: false,
            committed: 0,
            underflows: 0,
            handoffs: 0,
            primes: 0,
            ring_l: Vec::new(),
            dma_l: Vec::new(),
            gains: Vec::new(),
            pauses: Vec::new(),
        }
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
        // The ROM sets `finished` in the same critical section as the final push, so the last short
        // chunk is a classified tail rather than an underflow.
        if self.feed.pushed() as u32 == self.total {
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
    /// the session clock — the ROM's `feeder`, in the same order.
    fn period(&mut self) -> Chunk {
        assert_ne!(
            self.transport.state(),
            State::Paused,
            "a period was served with the chain frozen"
        );
        let (mut l, mut r) = ([0u8; CHUNK], [0u8; CHUNK]);
        let verdict = self.feed.take_chunk(&mut l, &mut r, CHUNK, self.finished);
        self.ring_l.extend_from_slice(&l);
        self.transport.apply(&mut l, &mut r);
        self.dma_l.extend_from_slice(&l);
        self.handoffs += 1;
        self.gains.push(self.transport.gain().gain());
        match verdict {
            Chunk::Full => self.committed += CHUNK as u32,
            Chunk::Tail { real } => self.committed += real as u32,
            Chunk::Starved => {
                if !self.finished {
                    self.underflows += 1;
                }
            }
        }
        self.ledger();
        verdict
    }

    /// The invariant the whole design rests on: the session clock is the ring's own ledger, not a
    /// parallel computation.
    fn ledger(&self) {
        assert_eq!(
            self.committed,
            self.feed.consumed() as u32,
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
        assert_eq!(
            self.transport.toggle(true),
            Step::FadeOut,
            "a pause from Playing must fade out"
        );
        let committed_at_toggle = self.committed;
        let mut fade_periods = 0;
        loop {
            self.period();
            fade_periods += 1;
            match self.transport.tick() {
                Step::Freeze => break,
                Step::Idle => assert!(fade_periods < 4, "the fade never reached silence"),
                other => panic!("unexpected step {other:?}"),
            }
        }
        assert_eq!(self.transport.state(), State::Paused);
        let pause = Pause {
            committed_at_toggle,
            frozen_committed: self.committed,
            gain_at_freeze: self.transport.gain().gain(),
            fade_periods,
            resumed_from: 0,
            frozen_frames: 0,
        };
        self.pauses.push(pause);
        self.pauses.last().cloned().expect("just pushed")
    }

    /// The main loop's frame slice for a resume press: the toggle asks for the re-prime, the prime
    /// takes the ring's head (applying the ramp to that chunk), and the chain is re-armed.
    fn press_resume(&mut self, paused_frames: u32) {
        assert_eq!(self.transport.state(), State::Paused);
        let from = self.committed;
        assert_eq!(
            self.transport.toggle(true),
            Step::Prime,
            "a toggle from Paused must ask for the re-prime"
        );
        assert!(
            self.prime(),
            "the ring's head must supply the prime's chunk"
        );
        // The primed chunk is the ramp's first chunk, and 256 < 560 completes the ramp inside it.
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
    // is why a pause can freeze the chain at the next period boundary instead of stopping mid-ramp.
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
        pause.fade_periods, 1,
        "the fade did not complete inside one period"
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
    // ring's own bytes are what the unscaled capture shows beside it.
    let faded = &model.dma_l[model.dma_l.len() - CHUNK..];
    let ring = &model.ring_l[model.ring_l.len() - CHUNK..];
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
        assert_eq!(pause.fade_periods, 1);
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

    model.press_pause();
    // The fade's own chunk is a period, so the frozen ring has room for exactly what it served.
    let free = model.feed.free_samples();
    assert!(free < FRAME, "the ring was not full: {free} free");
    // A frame-sized push into the frozen ring is refused, and nothing is credited. 0x80 is chosen
    // because the fake source cannot produce it (its bytes span ±60 codes), so it is a marker.
    assert_eq!(
        model
            .feed
            .push_bytes(&vec![0x80; FRAME], &vec![0x80; FRAME]),
        Err(PushError::OutOfSpace {
            need: FRAME,
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
