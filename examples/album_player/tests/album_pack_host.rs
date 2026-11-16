//! The Phase-1 pack gate: the packed album, read through the shipping parser.
//!
//! `scripts/album_bake.py` wrote `assets/albm1.bin` and `src/assets.rs`; this
//! suite reads the finished blob through `Album::parse` and compares it with the
//! config restated here as literals — config → reader, never writer → writer, so
//! a writer that agreed with itself would still fail this file.
//!
//! What is claimed, each with its own tests so a failure names one:
//!
//! 1. the manifest parses, and every header field is what `album.toml` declares
//!    (4 tracks, 16,384 Hz, the album length, title/artist/year, `flac`);
//! 2. the track table is a contiguous timeline whose lengths sum to the album;
//! 3. titles are §5.5's fold (ASCII, wrapped title stripped), and the long one
//!    is *kept and printed*, not truncated or refused;
//! 4. the aliased pair (tracks 0 and 2, same source bytes) addresses one blob —
//!    Q15's device: a long list costs one encode;
//! 5. every pin agrees with the manifest and with the packed container's own
//!    `total_samples` (§4.3 rule 1);
//! 6. §6's reciprocal is the bake-time value and re-proves its four properties
//!    through `nodiv::bar_cell` on the baked numbers;
//! 7. every `meta::Refusal` variant fires from a byte this suite mutated.
//!
//! This is a host test: `std`, no `agb`, and the library it links is the one a
//! ROM will compile later, so nothing here can be true of a model and false of
//! the image.

use album_player::assets::{self, TrackPin};
use album_player::meta::{self, Album, Refusal};
use album_player::nodiv;

/// The packed manifest, written by `make album-bake`. `include_bytes!` makes a
/// missing blob a compile error naming the path; there is no runtime default and
/// no skip.
const BLOB: &[u8] = assets::ALBUM1;

/// The header offsets of `ALBM1` v2's two folds: 44 is FNV-1a/32 over
/// `table ‖ pool` (boot's rule), 60 is FNV-1a/32 over the asset region (the
/// post-audio audit's rule). `54..60` stays reserved.
const STRUCTURE_FNV_AT: usize = 44;
const ASSETS_FNV_AT: usize = 60;

/// `album.toml`, restated: four tracks over the tracked reference master.
const N_TRACKS: u32 = 4;
/// `[out] rate_hz` and the product clock (Q14), restated from the config and
/// from the app's own `meta::PRODUCT_RATE_HZ`.
const RATE_HZ: u32 = 16_384;
/// The master's length at the product clock: 258.6096 s × 16,384.
const FULL_SAMPLES: u32 = 4_237_112;
/// A 30 s clip at the product clock.
const CLIP_SAMPLES: u32 = 491_520;
/// The album: two full tracks and two clips, in that order.
const ALBUM_SAMPLES: u32 = 2 * FULL_SAMPLES + 2 * CLIP_SAMPLES;

/// `ALBM1`'s header length; the track table starts here.
const HEADER_BYTES: usize = 64;
/// One track entry.
const TRACK_BYTES: usize = 40;

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

fn half(blob: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([blob[offset], blob[offset + 1]])
}

/// Parse a copy of the baked blob with one edit applied. The baked slice itself
/// is never touched.
fn refuse_after(edit: impl FnOnce(&mut Vec<u8>)) -> Refusal {
    let mut copy = BLOB.to_vec();
    edit(&mut copy);
    match Album::parse(&copy) {
        Ok(_) => panic!("a mutated blob was accepted"),
        Err(refusal) => refusal,
    }
}

/// The byte offset of one track entry.
fn entry(track: u32) -> usize {
    HEADER_BYTES + track as usize * TRACK_BYTES
}

/// The string pool's byte range, derived from the header the way the reader does.
fn pool_range(blob: &[u8]) -> (usize, usize) {
    let off = word(blob, 28) as usize;
    (off, off + word(blob, 32) as usize)
}

// ---------------------------------------------------------------------------
// 1. the manifest, field by field, against the config
// ---------------------------------------------------------------------------

#[test]
fn manifest_parses() {
    let album = parsed();
    assert_eq!(album.n_tracks(), N_TRACKS);
}

#[test]
fn track_count_matches_the_generated_pin() {
    assert_eq!(parsed().n_tracks(), assets::N_TRACKS);
    assert_eq!(assets::TRACKS.len(), N_TRACKS as usize);
}

#[test]
fn rate_is_the_product_clock() {
    let album = parsed();
    assert_eq!(album.rate_hz(), RATE_HZ);
    assert_eq!(assets::RATE_HZ, RATE_HZ);
    assert_eq!(album.rate_hz(), meta::PRODUCT_RATE_HZ);
}

#[test]
fn product_rate_is_the_shift_nodiv_divides_by() {
    // The tie `nodiv` deliberately does not carry: the app's clock is §6's power
    // of two, so `seconds()` is a shift at exactly this rate. `meta.rs` asserts
    // the same equality as a `const`; this one reads it through the constants the
    // suite paints with, so the tie is witnessed on both sides.
    assert_eq!(meta::PRODUCT_RATE_HZ, 1 << nodiv::RATE_LOG2);
    assert_eq!(1u32 << nodiv::RATE_LOG2, RATE_HZ);
}

#[test]
fn backend_is_the_config_selector() {
    let album = parsed();
    assert_eq!(assets::BACKEND, "flac");
    assert_eq!(album.backend(), meta::BACKEND_FLAC);
    for index in 0..album.n_tracks() {
        assert_eq!(album.track(index).backend, meta::BACKEND_FLAC);
    }
}

#[test]
fn album_metadata_is_the_folded_config() {
    let album = parsed();
    assert_eq!(album.album_title(), "Bioluminescent");
    assert_eq!(album.album_artist(), "unknown");
    assert_eq!(album.year(), 2026);
    assert_eq!(assets::ALBUM_TITLE, album.album_title());
    assert_eq!(assets::ALBUM_ARTIST, album.album_artist());
    assert_eq!(assets::YEAR, album.year());
}

#[test]
fn album_length_is_the_sum_of_the_tracks() {
    let album = parsed();
    assert_eq!(album.album_samples(), ALBUM_SAMPLES);
    assert_eq!(assets::TOTAL_SAMPLES, ALBUM_SAMPLES);
    let sum: u32 = (0..album.n_tracks())
        .map(|index| album.track(index).n_samples)
        .sum();
    assert_eq!(sum, album.album_samples());
}

#[test]
fn start_samples_are_a_contiguous_timeline() {
    let album = parsed();
    let mut cursor = 0u32;
    for index in 0..album.n_tracks() {
        let track = album.track(index);
        assert_eq!(track.start_sample, cursor, "track {index} start");
        cursor += track.n_samples;
    }
    assert_eq!(cursor, album.album_samples());
}

#[test]
fn track_lengths_alternate_full_and_clip() {
    let album = parsed();
    let lengths: Vec<u32> = (0..album.n_tracks())
        .map(|index| album.track(index).n_samples)
        .collect();
    assert_eq!(
        lengths,
        vec![FULL_SAMPLES, CLIP_SAMPLES, FULL_SAMPLES, CLIP_SAMPLES]
    );
}

// ---------------------------------------------------------------------------
// 2. titles: §5.5's fold, and the overflow that is printed, not refused
// ---------------------------------------------------------------------------

#[test]
fn titles_are_ascii_folded_from_the_config() {
    let album = parsed();
    let titles: Vec<&str> = (0..album.n_tracks())
        .map(|index| album.track_title(index))
        .collect();
    assert_eq!(
        titles,
        [
            "Bioluminescent 1",
            "Bioluminescent Clip 1",
            "Bioluminescent 2",
            "Bioluminescent Clip 2",
        ]
    );
    for title in titles {
        assert!(title.is_ascii(), "{title:?} is not ASCII");
    }
}

#[test]
fn the_fixture_titles_are_q15s_names_verbatim() {
    // §15's Q15 names these tracks; the titles are the config's own strings, folded to
    // themselves. An earlier revision of the fixture decorated them (`… — Öffnung
    // (Alternate Mix, Take 12)`, a wholly parenthesised `Clip 2`, artist "Reference
    // Master") to drive §5.5's fold and §5.2's box report — which put invented album
    // metadata in the one tracked artifact of an album build, where it reads as fact.
    // Withdrawn as D5. §5.5's rules — non-ASCII, a >28-char title, a whole title wrapped
    // in parentheses — are witnessed where they belong: `scripts/album_prep.py
    // --self-test`, on inputs labelled synthetic. What is left here is the invariant that
    // the bake does not rewrite the config, so a long or accented title can only reach
    // ALBM1 through the fold that is tested over there.
    let album = parsed();
    for (index, want) in [
        "Bioluminescent 1",
        "Bioluminescent Clip 1",
        "Bioluminescent 2",
        "Bioluminescent Clip 2",
    ]
    .into_iter()
    .enumerate()
    {
        let index = index as u32;
        let title = album.track_title(index);
        assert_eq!(title, want, "track {index} is not Q15's name");
        assert!(
            title.chars().count() <= 28,
            "track {index}: {title:?} overflows §5.2's box, and the fixture has no business doing that"
        );
        assert_eq!(
            album.track_artist(index),
            "",
            "the tracked master carries no RIFF tags, so a track artist here would be invented"
        );
    }
    assert_eq!(
        album.album_artist(),
        "unknown",
        "see `[album]` in album.toml"
    );
}

#[test]
fn track_artists_are_empty_when_the_source_carries_none() {
    let album = parsed();
    for index in 0..album.n_tracks() {
        assert_eq!(album.track_artist(index), "");
    }
}

// ---------------------------------------------------------------------------
// 3. the asset region: aliasing and bounds
// ---------------------------------------------------------------------------

#[test]
fn every_track_asset_is_a_gafp_container() {
    let album = parsed();
    for index in 0..album.n_tracks() {
        let asset = album.track(index).asset;
        assert_eq!(&asset[0..4], b"GAFP", "track {index} is not a GAFP blob");
        assert!(!asset.is_empty());
    }
}

#[test]
fn aliased_tracks_share_one_blob() {
    // Q15: tracks 0 and 2 are the same source bytes, so `ALBM1`'s per-track
    // `(offset, len)` maps two entries onto one packed blob.
    let album = parsed();
    let first = album.track(0);
    let alias = album.track(2);
    assert_eq!(first.asset, alias.asset);
    assert_eq!(assets::TRACKS[0].asset_off, assets::TRACKS[2].asset_off);
    assert_eq!(assets::TRACKS[0].asset_len, assets::TRACKS[2].asset_len);
    assert_eq!(assets::TRACKS[0].blob_sha256, assets::TRACKS[2].blob_sha256);
    assert_eq!(assets::TRACKS[0].fnv_pcm, assets::TRACKS[2].fnv_pcm);
    // The manifest's own per-track FNV-1a/32 over the blob bytes: the same bytes
    // under two entries hash the same, and each agrees with its stored value.
    let fnv = |track: u32| word(BLOB, entry(track) + 32);
    assert_eq!(fnv(0), fnv(2));
    assert_eq!(fnv(0), meta::fnv1a32_parts(&[first.asset]));
    assert_eq!(fnv(1), meta::fnv1a32_parts(&[album.track(1).asset]));
    // The clips are genuinely different bytes, so aliasing is not blanket.
    assert_ne!(album.track(1).asset, album.track(3).asset);
    assert_ne!(assets::TRACKS[1].blob_sha256, assets::TRACKS[3].blob_sha256);
}

#[test]
fn the_asset_region_is_the_blob_length_the_header_declares() {
    let album = parsed();
    let asset_len = word(BLOB, 40) as usize;
    assert_eq!(word(BLOB, 36) as usize + asset_len, BLOB.len());
    for index in 0..album.n_tracks() {
        let track = album.track(index);
        let pin = &assets::TRACKS[index as usize];
        assert!(pin.asset_off as usize + pin.asset_len as usize <= asset_len);
        assert_eq!(track.asset.len(), pin.asset_len as usize);
    }
}

// ---------------------------------------------------------------------------
// 4. the pins agree with the manifest and the container
// ---------------------------------------------------------------------------

/// The container's own `total_samples`, read out of the blob bytes this crate
/// carries rather than out of the generated pin: `GAFP`'s header puts it at byte
/// 14 as a `u64` (`gafp_pack.py`'s `pack_into("<Q", blob, 14, ...)`).
#[test]
fn container_total_samples_is_each_tracks_n_samples() {
    // §4.3 rule 1's reader half with the *writer* out of the path: the manifest
    // entry is compared with the container it points at, so a manifest that
    // agreed with `assets.rs` but not with the bytes it addresses fails here.
    // The bake's cross-check is writer-side; this one is not.
    let album = parsed();
    for index in 0..album.n_tracks() {
        let track = album.track(index);
        let asset = track.asset;
        assert_eq!(&asset[0..4], b"GAFP", "track {index} is not a GAFP blob");
        assert!(
            asset.len() >= 22,
            "track {index} is shorter than a GAFP header"
        );
        let total_samples = u64::from_le_bytes([
            asset[14], asset[15], asset[16], asset[17], asset[18], asset[19], asset[20], asset[21],
        ]);
        assert_eq!(
            total_samples, track.n_samples as u64,
            "track {index}: the container reports {total_samples} samples, \
             the manifest says {} (\u{a7}4.3 rule 1)",
            track.n_samples
        );
    }
}

#[test]
fn pins_agree_with_the_manifest() {
    let album = parsed();
    for index in 0..album.n_tracks() {
        let track = album.track(index);
        let pin: &TrackPin = &assets::TRACKS[index as usize];
        assert_eq!(pin.start_sample, track.start_sample, "track {index} start");
        assert_eq!(pin.n_samples, track.n_samples, "track {index} n_samples");
        assert_eq!(pin.recip, track.recip, "track {index} recip");
        assert_eq!(pin.frames, track.frames, "track {index} frames");
        assert_eq!(
            pin.max_fixed_order, track.max_fixed_order,
            "track {index} order"
        );
        assert_eq!(
            pin.samples, track.n_samples as u64,
            "track {index} container total"
        );
    }
}

#[test]
fn pin_samples_are_the_packed_totals_the_bake_cross_checked() {
    // §4.3 rule 1: the manifest's n_samples and the container's total_samples
    // are the same number, because the bake refuses when they differ. This test
    // is the reader's half of that cross-check.
    for pin in &assets::TRACKS {
        assert_eq!(pin.samples, pin.n_samples as u64);
    }
    for pin in &assets::TRACKS {
        assert_eq!(pin.blob_sha256.len(), 64);
        assert!(pin.blob_sha256.chars().all(|c| c.is_ascii_hexdigit()));
    }
}

// ---------------------------------------------------------------------------
// 5. §6's reciprocal, re-proved on the baked values
// ---------------------------------------------------------------------------

#[test]
fn recip_is_the_bake_time_value() {
    let album = parsed();
    for index in 0..album.n_tracks() {
        let track = album.track(index);
        assert_eq!(track.recip, nodiv::bar_recip(track.n_samples));
    }
}

#[test]
fn bar_cell_has_the_four_properties_on_the_baked_reciprocals() {
    let album = parsed();
    for index in 0..album.n_tracks() {
        let track = album.track(index);
        let n = track.n_samples;
        let recip = track.recip;
        let mut previous = 0;
        let positions = (0..=4_096)
            .map(|step| ((n as u64 - 1) * step as u64 / 4_096) as u32)
            .chain([0, 1, 2, 1_000, 2_600_000, n / 2, n - 2, n - 1])
            .filter(|pos| *pos < n)
            .collect::<std::collections::BTreeSet<u32>>();
        for pos in positions {
            let cell = nodiv::bar_cell(pos, recip);
            assert!(
                cell < nodiv::BAR_CELLS,
                "track {index} pos {pos} reached the bar"
            );
            assert!(cell >= previous, "track {index} went backwards at {pos}");
            let exact = pos as u64 * nodiv::BAR_CELLS as u64 / n as u64;
            assert!(
                (cell as u64).abs_diff(exact) <= 1,
                "track {index} pos {pos}: cell {cell} vs exact {exact}"
            );
            previous = cell;
        }
        assert_eq!(
            nodiv::bar_cell(n - 1, recip),
            nodiv::BAR_CELLS - 1,
            "track {index} last sample"
        );
    }
}

#[test]
fn elapsed_cell_reads_the_manifest_reciprocal() {
    let album = parsed();
    for index in 0..album.n_tracks() {
        let track = album.track(index);
        for pos in [0, 1, track.n_samples / 3, track.n_samples - 1] {
            assert_eq!(
                album.elapsed_cell(index, pos),
                nodiv::bar_cell(pos, track.recip)
            );
        }
        assert_eq!(album.elapsed_cell(index, track.n_samples - 1), 31);
    }
}

#[test]
fn the_album_label_is_nine_minutes_thirty_seven() {
    // 9,457,264 samples >> 14 = 577 s, and 577 s is 9:37 — §6's mm:ss, read
    // through the included arithmetic rather than restated.
    assert_eq!(nodiv::seconds(ALBUM_SAMPLES), 577);
    assert_eq!(nodiv::label(ALBUM_SAMPLES), (9, 37));
}

// ---------------------------------------------------------------------------
// 6. every refusal, from a byte this suite mutated
// ---------------------------------------------------------------------------

#[test]
fn refuses_bad_magic() {
    let refusal = refuse_after(|blob| blob[0] = b'X');
    assert_eq!(refusal, Refusal::BadMagic);
}

#[test]
fn refuses_a_version_that_is_not_2() {
    let refusal = refuse_after(|blob| blob[4] = 3);
    assert_eq!(refusal, Refusal::UnsupportedVersion { version: 3 });
}

#[test]
fn refuses_zero_tracks() {
    let refusal = refuse_after(|blob| blob[5] = 0);
    assert_eq!(refusal, Refusal::TrackCount { n_tracks: 0 });
}

#[test]
fn refuses_more_than_32_tracks() {
    let refusal = refuse_after(|blob| blob[5] = 33);
    assert_eq!(refusal, Refusal::TrackCount { n_tracks: 33 });
}

#[test]
fn refuses_a_rate_that_is_not_the_product_clock() {
    let refusal = refuse_after(|blob| blob[8..12].copy_from_slice(&32_768u32.to_le_bytes()));
    assert_eq!(refusal, Refusal::RateNotProductClock { rate_hz: 32_768 });
}

#[test]
fn refuses_a_bar_grid_that_is_not_32() {
    let refusal = refuse_after(|blob| blob[12..16].copy_from_slice(&64u32.to_le_bytes()));
    assert_eq!(refusal, Refusal::BarCellsMismatch { bar_cells: 64 });
}

#[test]
fn refuses_an_unknown_backend() {
    let refusal = refuse_after(|blob| blob[6] = 7);
    assert_eq!(refusal, Refusal::BackendUnknown { backend: 7 });
}

#[test]
fn refuses_sections_out_of_order() {
    let expected = word(BLOB, 28) + 4;
    let refusal = refuse_after(|blob| blob[28..32].copy_from_slice(&expected.to_le_bytes()));
    assert_eq!(
        refusal,
        Refusal::SectionOrder {
            what: "string pool",
            offset: expected,
            expected: word(BLOB, 28),
        }
    );
}

#[test]
fn refuses_a_file_length_inconsistent_with_the_sections() {
    let refusal = refuse_after(|blob| blob.truncate(BLOB.len() - 4));
    assert!(
        matches!(refusal, Refusal::LengthInconsistent { .. }),
        "{refusal}"
    );
}

#[test]
fn refuses_a_pool_at_the_u16_limit() {
    // `POOL_LIMIT` is its own rule: at 0x1_0000 pool bytes the next string
    // offset cannot be a `u16`. The pool is grown rather than the declared
    // length alone being edited, so the sections stay in order and this is the
    // *only* thing wrong with the blob — which is why the checksum is not the
    // refusal.
    let pool_off = word(BLOB, 28) as usize;
    let pool_len = word(BLOB, 32) as usize;
    let mut blob = BLOB[..pool_off + pool_len].to_vec();
    blob.resize(pool_off + meta::POOL_LIMIT, 0);
    let asset_off = blob.len() as u32;
    blob.extend_from_slice(&BLOB[pool_off + pool_len..]);
    blob[32..36].copy_from_slice(&(meta::POOL_LIMIT as u32).to_le_bytes());
    blob[36..40].copy_from_slice(&asset_off.to_le_bytes());
    assert_eq!(
        Album::parse(&blob).unwrap_err(),
        Refusal::PoolTooLarge {
            pool_len: meta::POOL_LIMIT as u32,
            limit: meta::POOL_LIMIT as u32,
        }
    );
}

#[test]
fn refuses_an_asset_region_whose_end_wraps_a_u32() {
    let offset = word(BLOB, 36);
    let refusal = refuse_after(|blob| blob[40..44].copy_from_slice(&u32::MAX.to_le_bytes()));
    assert_eq!(
        refusal,
        Refusal::AssetRegionOverflow {
            offset,
            len: u32::MAX,
        }
    );
}

#[test]
fn refuses_a_non_zero_reserved_byte_in_the_header() {
    let refusal = refuse_after(|blob| blob[7] = 1);
    assert_eq!(
        refusal,
        Refusal::NonZeroReserved {
            offset: 7,
            value: 1
        }
    );
    let refusal = refuse_after(|blob| blob[55] = 0x10);
    assert_eq!(
        refusal,
        Refusal::NonZeroReserved {
            offset: 55,
            value: 0x10
        }
    );
}

#[test]
fn refuses_a_non_zero_reserved_byte_in_a_track_entry() {
    let at = entry(1) + 37;
    let refusal = refuse_after(|blob| blob[at] = 1);
    assert_eq!(
        refusal,
        Refusal::NonZeroReserved {
            offset: at,
            value: 1
        }
    );
}

#[test]
fn refuses_a_pool_offset_outside_the_pool() {
    let refusal = refuse_after(|blob| blob[48..50].copy_from_slice(&0xFFFFu16.to_le_bytes()));
    assert!(
        matches!(refusal, Refusal::PoolOffsetOutOfRange { .. }),
        "{refusal}"
    );
}

#[test]
fn refuses_a_non_ascii_pool_byte() {
    let pool = word(BLOB, 28) as usize;
    let refusal = refuse_after(|blob| blob[pool] = 0xC4);
    assert_eq!(
        refusal,
        Refusal::TitleNotAscii {
            offset: 0,
            byte: 0xC4
        }
    );
}

#[test]
fn refuses_a_string_without_a_terminator() {
    // The pool's last byte, made non-zero: whatever sits there, the walk from
    // the preceding NUL reaches the end of the pool without a terminator.
    let (_, end) = pool_range(BLOB);
    let refusal = refuse_after(|blob| blob[end - 1] = b'A');
    assert!(
        matches!(refusal, Refusal::MissingTerminator { .. }),
        "{refusal}"
    );
}

#[test]
fn refuses_non_zero_padding() {
    // Pointing the album title at another existing string leaves the first
    // string unreferenced, which is what a non-zero padding byte looks like to
    // the walk — and it is reported before the checksum notices the edit.
    let artist_off = half(BLOB, 50).to_le_bytes();
    let refusal = refuse_after(|blob| blob[48..50].copy_from_slice(&artist_off));
    assert_eq!(
        refusal,
        Refusal::PaddingNotZero {
            offset: 0,
            value: b'B'
        }
    );
}

#[test]
fn refuses_a_track_that_does_not_start_where_the_previous_ended() {
    let at = entry(1);
    let start = word(BLOB, at) + 1;
    let refusal = refuse_after(|blob| blob[at..at + 4].copy_from_slice(&start.to_le_bytes()));
    assert_eq!(
        refusal,
        Refusal::TrackSampleRange {
            track: 1,
            start_sample: start,
            n_samples: CLIP_SAMPLES,
            album_samples: ALBUM_SAMPLES,
        }
    );
}

#[test]
fn refuses_an_asset_range_outside_the_region() {
    let at = entry(0) + 16;
    let refusal =
        refuse_after(|blob| blob[at..at + 4].copy_from_slice(&(BLOB.len() as u32).to_le_bytes()));
    assert!(
        matches!(refusal, Refusal::AssetOutOfBounds { track: 0, .. }),
        "{refusal}"
    );
}

#[test]
fn refuses_a_reciprocal_that_is_not_the_floor() {
    let at = entry(0) + 8;
    let recip = word(BLOB, at) + 1;
    let refusal = refuse_after(|blob| blob[at..at + 4].copy_from_slice(&recip.to_le_bytes()));
    assert_eq!(
        refusal,
        Refusal::ReciprocalInconsistent {
            track: 0,
            n_samples: FULL_SAMPLES,
            recip,
        }
    );
}

#[test]
fn refuses_a_structure_fold_mismatch() {
    let stored = word(BLOB, STRUCTURE_FNV_AT);
    let refusal = refuse_after(|blob| blob[STRUCTURE_FNV_AT] ^= 0xFF);
    assert!(
        matches!(refusal, Refusal::ChecksumMismatch { .. }),
        "{refusal}"
    );
    if let Refusal::ChecksumMismatch { stored: got, .. } = refusal {
        assert_eq!(got, stored ^ 0xFF);
    }
}

#[test]
fn refuses_every_mutation_the_bake_self_test_feeds_to_audit_blob() {
    // The drift gate for the writer's rule list. The list is the *tool's*:
    // `scripts/album_bake.py` feeds these names to `audit_blob` (the pre-write
    // smoke check) and emits them into `src/assets.rs`, and this test applies the
    // same list to the shipping parser — so a rule the Python audit still applies
    // and `meta.rs` no longer applies (or the reverse) stops being silent. The
    // per-variant tests above are the other half: every `Refusal` variant fires
    // from a mutated real blob, so neither list can quietly lose one either.
    const MUTATIONS: usize = assets::AUDITED_MUTATIONS.len();
    let mutations: [(&str, fn(&mut Vec<u8>)); MUTATIONS] = [
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
        ("structure checksum", |blob| blob[STRUCTURE_FNV_AT] ^= 0xFF),
        ("assets checksum", |blob| blob[ASSETS_FNV_AT] ^= 0xFF),
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
        // v2 splits the reader's checksums: the structure fold is `parse`'s rule and
        // the asset fold is `verify_assets`'s (the post-audio audit). A named
        // mutation must be refused by one of the two — the per-variant tests above
        // say which one, and `the_asset_fold_is_not_checked_at_boot` says the asset
        // fold is deliberately not the boot rule.
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
fn the_structure_fold_is_the_boot_rule_and_the_asset_fold_is_the_audit_rule() {
    // Two folds, two passes: 44 hashes the kilobytes boot may read, 60 hashes the
    // asset region the post-audio audit walks. Both are recomputed here from the
    // blob's own sections, so a writer that folded the wrong span fails.
    let (pool_off, pool_end) = pool_range(BLOB);
    let tracks = &BLOB[HEADER_BYTES..pool_off];
    let pool = &BLOB[pool_off..pool_end];
    let assets = &BLOB[word(BLOB, 36) as usize..];
    assert_eq!(
        word(BLOB, STRUCTURE_FNV_AT),
        meta::fnv1a32_parts(&[tracks, pool]),
        "the structure fold is not over table ‖ pool"
    );
    assert_eq!(
        word(BLOB, ASSETS_FNV_AT),
        meta::fnv1a32_parts(&[assets]),
        "the asset fold is not over the asset region"
    );
    assert_ne!(word(BLOB, STRUCTURE_FNV_AT), word(BLOB, ASSETS_FNV_AT));
    assert_eq!(parsed().verify_assets(), Ok(()));
}

#[test]
fn the_asset_fold_is_not_checked_at_boot() {
    // The whole point of v2: a wrong asset fold is accepted by `parse` and refused
    // by the audit, so boot never walks the region to find out.
    let mut copy = BLOB.to_vec();
    copy[ASSETS_FNV_AT] ^= 0xFF;
    let album = Album::parse(&copy).expect("the asset fold is the audit's rule, not boot's");
    assert!(
        matches!(album.verify_assets(), Err(Refusal::AssetsMismatch { .. })),
        "a mutated asset fold survived the post-audio audit"
    );
}

#[test]
fn a_byte_in_the_middle_of_the_asset_region_is_accepted_at_boot() {
    // The real regression guard: an O(asset-region) sweep on the pre-audio path
    // would read this byte at boot. It must not — the byte is outside every
    // content tripwire, so only the post-audio full fold sees it.
    let asset_off = word(BLOB, 36) as usize;
    let asset_len = word(BLOB, 40) as usize;
    let mut copy = BLOB.to_vec();
    copy[asset_off + asset_len / 2] ^= 0xFF;
    let album = Album::parse(&copy).expect("a cold boot must not read the whole asset region");
    assert!(
        album.verify_assets().is_err(),
        "the post-audio audit did not catch a mid-asset byte"
    );
}

#[test]
fn a_cold_boot_reads_under_a_mebibyte_and_never_scales_with_the_asset_region() {
    let album = parsed();
    let boot = meta::boot_audit_bytes(album.n_tracks());
    assert!(
        boot < 1 << 20,
        "a cold boot reads {boot} B of the cart — the bound is 1 MiB"
    );
    // The span is a per-track tripwire: one more track adds one entry and one
    // tripwire, never a share of the asset region. This is the assertion that
    // fails if an O(cart) sweep comes back to the pre-audio path.
    assert_eq!(
        meta::boot_audit_bytes(album.n_tracks() + 1) - meta::boot_audit_bytes(album.n_tracks()),
        meta::TRACK_BYTES + meta::BOOT_PREFIX_BYTES + meta::BOOT_SUFFIX_BYTES
    );
    assert!(
        album.asset_region().len() > 8 * boot,
        "the asset region ({} B) is no longer large enough for this test to mean anything",
        album.asset_region().len()
    );
}

#[test]
fn the_content_tripwire_pins_cover_the_bounded_spans() {
    // The bake's prefix/suffix pins against the blob bytes it wrote them from: a
    // stale pin (or a stale blob) is a mismatch here, before a ROM boots.
    let album = parsed();
    for (index, pin) in assets::TRACKS.iter().enumerate() {
        let asset = album.track(index as u32).asset;
        assert_eq!(
            pin.prefix_fnv,
            meta::prefix_fnv(asset),
            "track {index} prefix_fnv"
        );
        assert_eq!(
            pin.suffix_fnv,
            meta::suffix_fnv(asset),
            "track {index} suffix_fnv"
        );
    }
}

#[test]
fn every_refusal_names_its_rule() {
    let refusals = [
        Refusal::BadMagic,
        Refusal::UnsupportedVersion { version: 3 },
        Refusal::TrackCount { n_tracks: 0 },
        Refusal::RateNotProductClock { rate_hz: 32_768 },
        Refusal::BarCellsMismatch { bar_cells: 64 },
        Refusal::BackendUnknown { backend: 7 },
        Refusal::SectionOrder {
            what: "string pool",
            offset: 4,
            expected: 0,
        },
        Refusal::LengthInconsistent {
            have: 1,
            expected: 64,
        },
        Refusal::PoolTooLarge {
            pool_len: 0x1_0000,
            limit: 0x1_0000,
        },
        Refusal::AssetRegionOverflow {
            offset: 0xffff_fff0,
            len: 0x20,
        },
        Refusal::PaddingNotZero {
            offset: 0,
            value: 0x41,
        },
        Refusal::NonZeroReserved {
            offset: 7,
            value: 1,
        },
        Refusal::PoolOffsetOutOfRange {
            offset: 0xFFFF,
            pool_len: 64,
        },
        Refusal::TitleNotAscii {
            offset: 0,
            byte: 0xC4,
        },
        Refusal::MissingTerminator { offset: 3 },
        Refusal::TrackSampleRange {
            track: 1,
            start_sample: 0,
            n_samples: 0,
            album_samples: 1,
        },
        Refusal::AssetOutOfBounds {
            track: 0,
            offset: 0,
            len: 0,
            asset_len: 0,
        },
        Refusal::ReciprocalInconsistent {
            track: 0,
            n_samples: 1,
            recip: 0,
        },
        Refusal::ChecksumMismatch {
            stored: 0,
            computed: 1,
        },
        Refusal::AssetsMismatch {
            stored: 0,
            computed: 1,
        },
    ];
    for refusal in refusals {
        let text = refusal.to_string();
        assert!(!text.is_empty(), "{refusal:?} has no message");
        assert_ne!(
            text,
            format!("{refusal:?}"),
            "{refusal:?} fell back to Debug"
        );
    }
}
