//! `ALBM1` — the album manifest, read and refused.
//!
//! `ALBUM-PLAYER.md` §4.3 owns the layout; this module and
//! `scripts/album_bake.py` are written against that section independently, so
//! the reader is not a description of the writer's habits. Three rules shape
//! everything here:
//!
//! - **Refuse before indexing.** Every header field is validated, then the
//!   section length it implies is checked against the file, and only then is a
//!   slice taken. Nothing defaults, clamps or re-parses.
//! - **The manifest is authoritative** (§4.3 rule 1). Track boundaries,
//!   durations and titles come from here; the bake cross-checks them against the
//!   backend's container and refuses on disagreement, so the ROM never has to
//!   choose which file to believe.
//! - **The checksum is verified last**, after every semantic rule, so a blob
//!   that is both wrong and stale reports the field that is wrong rather than
//!   masking it as an integrity failure (the `art1` order, for the same reason).
//!   Since `ALBM1` v2 that is the **structure** fold over `table ‖ pool`; the
//!   fold over the asset region is [`Album::verify_assets`], deliberately *not*
//!   run by [`Album::parse`] — a boot may not sweep the cart (§4.3 v2, and
//!   [`boot_audit_bytes`] is the bound it is held to).
//!
//! The §6 reciprocal is *checked*, not taken on trust, and the check itself
//! contains no divide: `recip == floor(2^32 × N / n)` is exactly
//! `recip·n ≤ 2^32·N < (recip+1)·n`, two 64-bit multiplies — which is why the
//! reader can hold the bake to its own arithmetic without carrying the bake's
//! one tagged divide onto the GBA. `N` is [`layout::BAR_CELLS`]: the grid is the
//! selected variant's declared cell count (§6 re-ruled it on 2026-11-20 from a
//! constant in the shared crate to a generated field), so this reader compares a
//! manifest against the module it was linked with rather than against a number
//! written here. That comparison is the boot backstop of §9.0's
//! `bar-cells-disagree` row and it stays: the bake checks the spec, but only the
//! ROM can see a `layout.rs` generated yesterday sitting next to a manifest packed
//! today (R11).
//!
//! Host-pure: no `agb`, no registers, no allocation, no unsafe.

use crate::layout;
use crate::nodiv;

/// The product clock this reader accepts, and the one every track is encoded and
/// read at (Q14: a stream is encoded at the rate it is played at, so an album
/// declaring another rate is not playable at this one).
///
/// PR-3 replaces this with the application's own §4.2b `geometry.rs`, which
/// derives the clock per backend. Until then the app states the fact here rather
/// than borrowing the FLAC track's geometry module (§2.2 forbids the include;
/// §4.2b forbids the app depending on a FLAC-shaped one).
pub const PRODUCT_RATE_HZ: u32 = 16_384;

// §6's arithmetic and this crate's clock are the same power of two. The tie is
// asserted here, at the link site, because `nodiv` deliberately imports no clock
// (see its module docs) — this is the line that would fail if either moved.
const _: () = assert!(PRODUCT_RATE_HZ == 1 << nodiv::RATE_LOG2);

/// A blob that failed validation, by rule. The payload is what the rule was
/// checked against, so a log line or a screen can say *which* field was wrong.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// The first four bytes are not `ALBM`.
    BadMagic,
    /// The version field is not [`VERSION`] (the only version this reader knows).
    UnsupportedVersion {
        /// The version the blob declared.
        version: u8,
    },
    /// `n_tracks` is outside `1..=32`.
    TrackCount {
        /// The count the blob declared.
        n_tracks: u8,
    },
    /// The declared rate is not the product clock — Q14: a stream is encoded at
    /// the rate it is played at, so an album that declares another rate is not
    /// playable at this one.
    RateNotProductClock {
        /// The rate the blob declared.
        rate_hz: u32,
    },
    /// The bar grid in the manifest is not the one this ROM was built to draw:
    /// [`layout::BAR_CELLS`], the selected variant's declared count, generated
    /// into `src/layout.rs`. §6's grid is per-variant, so the refusal names the
    /// number the linked module declares rather than a constant of its own — a
    /// pack whose reciprocals were computed for another grid would paint a bar
    /// that disagrees with the audio it is drawing.
    BarCellsMismatch {
        /// The cell count the blob declared.
        bar_cells: u32,
    },
    /// The backend selector is not one this reader knows.
    BackendUnknown {
        /// The selector the blob declared.
        backend: u8,
    },
    /// The four section offsets are not `header ‖ table ‖ pool ‖ assets`,
    /// contiguous from the header and 4-byte aligned.
    SectionOrder {
        /// Which pair disagreed, e.g. `"track table"`.
        what: &'static str,
        /// The offset the blob declared.
        offset: u32,
        /// The offset the section before it derives.
        expected: u32,
    },
    /// The file, or a section, is not the length the header's own counts derive.
    LengthInconsistent {
        /// Bytes actually present (or the length the header declared).
        have: usize,
        /// Bytes the header derives (or the limit the field must stay under).
        expected: usize,
    },
    /// The string pool is at or past [`POOL_LIMIT`], so a string offset could not
    /// be a `u16`. Its own rule, because "too many bytes" and "the wrong number
    /// of bytes" are different repairs.
    PoolTooLarge {
        /// The pool length the header declared.
        pool_len: u32,
        /// The limit string offsets imply.
        limit: u32,
    },
    /// The asset region's end does not fit the header's `u32` offset and length:
    /// `asset_off + asset_len` wraps, so no file could contain it.
    AssetRegionOverflow {
        /// The offset the header declared.
        offset: u32,
        /// The length the header declared.
        len: u32,
    },
    /// A padding byte (a byte no string offset names) is not zero.
    PaddingNotZero {
        /// Byte offset inside the string pool.
        offset: usize,
        /// The value found there.
        value: u8,
    },
    /// A byte in one of the header's or a track entry's reserved ranges is not
    /// zero.
    NonZeroReserved {
        /// Byte offset in the blob.
        offset: usize,
        /// The value found there.
        value: u32,
    },
    /// A string offset does not land inside the string pool.
    PoolOffsetOutOfRange {
        /// The offset the entry declared.
        offset: u16,
        /// The pool's length.
        pool_len: u32,
    },
    /// A string in the pool carries a byte outside ASCII.
    TitleNotAscii {
        /// Byte offset inside the pool.
        offset: u16,
        /// The byte found there.
        byte: u8,
    },
    /// A pool string has no NUL terminator before the pool ends.
    MissingTerminator {
        /// The string's offset inside the pool.
        offset: u16,
    },
    /// A track's sample range is empty, does not start where the previous track
    /// ended, or runs past the album. The tracks- do not sum to
    /// `album_samples` case names `track == n_tracks`.
    TrackSampleRange {
        /// Track index, or `n_tracks` for the album total.
        track: u32,
        /// The track's declared first album sample.
        start_sample: u32,
        /// The track's declared length.
        n_samples: u32,
        /// The album's declared length.
        album_samples: u32,
    },
    /// A track's `(offset, len)` does not land inside the asset region.
    AssetOutOfBounds {
        /// Track index.
        track: u32,
        /// The offset the entry declared.
        offset: u32,
        /// The length the entry declared.
        len: u32,
        /// The asset region's length.
        asset_len: u32,
    },
    /// The stored §6 reciprocal is not `floor(2^32 × layout::BAR_CELLS / n_samples)`.
    ReciprocalInconsistent {
        /// Track index.
        track: u32,
        /// The track's declared length.
        n_samples: u32,
        /// The reciprocal the entry declared.
        recip: u32,
    },
    /// The stored structure fold is not FNV-1a/32 over `table ‖ pool`. Checked by
    /// [`Album::parse`] **last**, after every semantic rule.
    ChecksumMismatch {
        /// The value in the header.
        stored: u32,
        /// The value the sections produce.
        computed: u32,
    },
    /// The stored asset fold is not FNV-1a/32 over the asset region. Checked by
    /// [`Album::verify_assets`], which runs **after** the audio: the boot path is
    /// bounded and does not sweep the cart.
    AssetsMismatch {
        /// The value in the header.
        stored: u32,
        /// The value the asset region produces.
        computed: u32,
    },
}

impl core::fmt::Display for Refusal {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match *self {
            Refusal::BadMagic => f.write_str("magic is not ALBM"),
            Refusal::UnsupportedVersion { version } => {
                write!(f, "version {version}, this reader reads {VERSION}")
            }
            Refusal::TrackCount { n_tracks } => {
                write!(f, "n_tracks {n_tracks}, the range is 1..={MAX_TRACKS}")
            }
            Refusal::RateNotProductClock { rate_hz } => write!(
                f,
                "rate {rate_hz} Hz is not the product clock {} Hz (Q14)",
                PRODUCT_RATE_HZ
            ),
            Refusal::BarCellsMismatch { bar_cells } => write!(
                f,
                "bar_cells {bar_cells}, this ROM's generated grid is {}",
                layout::BAR_CELLS
            ),
            Refusal::BackendUnknown { backend } => {
                write!(
                    f,
                    "backend {backend}, known selectors are {BACKEND_STUB}/{BACKEND_FLAC}"
                )
            }
            Refusal::SectionOrder {
                what,
                offset,
                expected,
            } => write!(
                f,
                "{what} at {offset:#x}, the section before it derives {expected:#x}"
            ),
            Refusal::LengthInconsistent { have, expected } => {
                write!(f, "{have} bytes, the header derives or limits {expected}")
            }
            Refusal::PoolTooLarge { pool_len, limit } => write!(
                f,
                "string pool is {pool_len} bytes, u16 string offsets hold fewer than {limit}"
            ),
            Refusal::AssetRegionOverflow { offset, len } => write!(
                f,
                "asset region {offset}+{len} wraps a u32; no file can hold it"
            ),
            Refusal::PaddingNotZero { offset, value } => {
                write!(
                    f,
                    "pool byte {offset} is {value:#04x}, padding must be zero"
                )
            }
            Refusal::NonZeroReserved { offset, value } => {
                write!(f, "reserved slot at {offset} is {value:#x}, must be 0")
            }
            Refusal::PoolOffsetOutOfRange { offset, pool_len } => {
                write!(
                    f,
                    "pool offset {offset} is outside the {pool_len}-byte pool"
                )
            }
            Refusal::TitleNotAscii { offset, byte } => {
                write!(f, "pool byte {offset} is {byte:#04x}, strings are ASCII")
            }
            Refusal::MissingTerminator { offset } => {
                write!(
                    f,
                    "the string at pool offset {offset} is not NUL-terminated"
                )
            }
            Refusal::TrackSampleRange {
                track,
                start_sample,
                n_samples,
                album_samples,
            } => write!(
                f,
                "track {track} covers {start_sample}+{n_samples} samples, the album is {album_samples}"
            ),
            Refusal::AssetOutOfBounds {
                track,
                offset,
                len,
                asset_len,
            } => write!(
                f,
                "track {track} addresses {offset}+{len} of a {asset_len}-byte asset region"
            ),
            Refusal::ReciprocalInconsistent {
                track,
                n_samples,
                recip,
            } => write!(
                f,
                "track {track}: recip {recip:#x} is not floor(2^32 × {0} / {n_samples})",
                layout::BAR_CELLS
            ),
            Refusal::ChecksumMismatch { stored, computed } => write!(
                f,
                "structure checksum {stored:#010x}, table+pool hash to {computed:#010x}"
            ),
            Refusal::AssetsMismatch { stored, computed } => write!(
                f,
                "asset checksum {stored:#010x}, the asset region hashes to {computed:#010x}"
            ),
        }
    }
}

/// Header length in bytes. The track table, string pool and asset region follow
/// it immediately.
///
/// v2 keeps the header at 64 B: bytes 0..44 are v1's field list, 44 is the
/// structure fold (v1's checksum, over `table ‖ pool` instead of the whole blob),
/// 48..54 are the album title/artist offsets and year, **54..60 stay reserved**,
/// and 60..64 carry the asset fold. The v1 reserved block was 54..64, so the new
/// field consumes four of its bytes and the documented "64 B header" still holds.
pub const HEADER_BYTES: usize = 64;
/// The structure fold's offset (FNV-1a/32 over `table ‖ pool`).
const STRUCTURE_FNV_AT: usize = 44;
/// The asset fold's offset (FNV-1a/32 over the asset region).
const ASSETS_FNV_AT: usize = 60;
/// One track entry. `n_tracks × 40` is the track table.
pub const TRACK_BYTES: usize = 40;
/// `ALBM1`'s version field for this layout: v2 splits the checksum into the
/// structure fold the boot path verifies and the asset fold the post-audio audit
/// verifies.
pub const VERSION: u8 = 2;
/// How many bytes of a track's asset the boot-time content tripwire folds: its
/// first 32 KiB. With [`BOOT_SUFFIX_BYTES`] this is the whole bounded read the
/// pre-audio path does per track.
pub const BOOT_PREFIX_BYTES: usize = 32 * 1024;
/// The last 4 KiB of a track's asset — the tripwire's suffix half.
pub const BOOT_SUFFIX_BYTES: usize = 4 * 1024;

/// The bytes a cold boot reads out of the cart: the header, the track table, the
/// string pool (at [`POOL_LIMIT`] as a ceiling) and each track's bounded content
/// tripwire.
///
/// **The point is the argument:** this is a function of `n_tracks` alone. It
/// cannot grow with the asset region, which is what keeps an O(cart) sweep off
/// the pre-audio path — the host gate asserts this span stays under 1 MiB and that
/// a byte in the middle of the asset region is accepted at boot (and refused by
/// [`Album::verify_assets`] afterwards).
pub const fn boot_audit_bytes(n_tracks: u32) -> usize {
    HEADER_BYTES
        + n_tracks as usize * TRACK_BYTES
        + POOL_LIMIT
        + n_tracks as usize * (BOOT_PREFIX_BYTES + BOOT_SUFFIX_BYTES)
}
/// The most tracks an album may declare (§9's Phase-3 cap, read here already:
/// a blob that declares more is not one this reader accepts).
pub const MAX_TRACKS: u8 = 32;
/// String offsets are `u16`, so the pool must stay below this.
pub const POOL_LIMIT: usize = 0x1_0000;
/// Backend selector: the silent stub (no decoder linked).
pub const BACKEND_STUB: u8 = 0;
/// Backend selector: packed `GAFP` blobs.
pub const BACKEND_FLAC: u8 = 1;

const FNV_OFFSET_BASIS: u32 = 0x811c_9dc5;
const FNV_PRIME: u32 = 0x0100_0193;

/// FNV-1a/32 over several spans without copying them together — the checksum
/// `ALBM1`'s header carries, over `table ‖ pool ‖ assets` in that order.
pub fn fnv1a32_parts(parts: &[&[u8]]) -> u32 {
    let mut hash = FNV_OFFSET_BASIS;
    for part in parts {
        for &byte in *part {
            hash ^= byte as u32;
            hash = hash.wrapping_mul(FNV_PRIME);
        }
    }
    hash
}

/// A validated manifest, borrowing its sections from the input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Album<'a> {
    rate_hz: u32,
    backend: u8,
    n_tracks: u8,
    album_samples: u32,
    year: u16,
    album_title_off: u16,
    album_artist_off: u16,
    tracks: &'a [u8],
    pool: &'a [u8],
    assets: &'a [u8],
    /// The header's asset fold, kept so [`verify_assets`](Self::verify_assets) can
    /// run after the audio without re-reading the header.
    assets_fnv: u32,
}

/// One validated track entry, decoded on access.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Track<'a> {
    /// Album position of the track's first sample.
    pub start_sample: u32,
    /// Samples in the track.
    pub n_samples: u32,
    /// §6's progress reciprocal, `floor(2^32 × layout::BAR_CELLS / n_samples)`.
    pub recip: u32,
    /// Frames in the backend's packed blob.
    pub frames: u32,
    /// Highest FIXED predictor order the packer recorded (the blob's byte 7).
    pub max_fixed_order: u8,
    /// The §12.2 measured cost ceiling; 0 in Phase 1 (no measurement yet).
    pub cost_ceiling_permille: u16,
    /// The backend selector for this track.
    pub backend: u8,
    /// `ALBM1`'s title, ASCII, borrowed from the pool.
    pub title: &'a str,
    /// `ALBM1`'s artist, ASCII, borrowed from the pool (`""` when unknown).
    pub artist: &'a str,
    /// This track's `(offset, len)` slice of the album's asset region.
    pub asset: &'a [u8],
}

impl<'a> Album<'a> {
    /// The sample rate every track is encoded and played at.
    pub const fn rate_hz(&self) -> u32 {
        self.rate_hz
    }

    /// The backend selector the album was baked for.
    pub const fn backend(&self) -> u8 {
        self.backend
    }

    /// Tracks in the album.
    pub const fn n_tracks(&self) -> u32 {
        self.n_tracks as u32
    }

    /// The album's length in samples at [`rate_hz`](Self::rate_hz).
    pub const fn album_samples(&self) -> u32 {
        self.album_samples
    }

    /// The album's year (`0` when the config did not carry one).
    pub const fn year(&self) -> u16 {
        self.year
    }

    /// The album title, ASCII.
    pub fn album_title(&self) -> &'a str {
        self.string(self.album_title_off)
    }

    /// The album artist, ASCII (`""` when unknown).
    pub fn album_artist(&self) -> &'a str {
        self.string(self.album_artist_off)
    }

    /// The track at `index`, decoded from its 40-byte entry.
    ///
    /// # Panics
    ///
    /// If `index >= n_tracks()`. Validation already refused a blob whose entries
    /// name samples or asset bytes that do not exist; this is for callers
    /// iterating the album.
    pub fn track(&self, index: u32) -> Track<'a> {
        assert!(index < self.n_tracks(), "track {index} out of range");
        let at = index as usize * TRACK_BYTES;
        let entry = &self.tracks[at..at + TRACK_BYTES];
        let asset_off = word(entry, 12) as usize;
        let asset_len = word(entry, 16) as usize;
        Track {
            start_sample: word(entry, 0),
            n_samples: word(entry, 4),
            recip: word(entry, 8),
            frames: word(entry, 20),
            max_fixed_order: entry[29],
            cost_ceiling_permille: half(entry, 30),
            backend: entry[28],
            title: self.string(half(entry, 24)),
            artist: self.string(half(entry, 26)),
            asset: &self.assets[asset_off..asset_off + asset_len],
        }
    }

    /// Track `index`'s title, ASCII.
    pub fn track_title(&self, index: u32) -> &'a str {
        self.track(index).title
    }

    /// Track `index`'s artist, ASCII (`""` when unknown).
    pub fn track_artist(&self, index: u32) -> &'a str {
        self.track(index).artist
    }

    /// §6's progress cell for `pos_in_track` samples into `track`: one UMULL and
    /// a shift, through the same [`nodiv::bar_cell`] the probe proved.
    pub fn elapsed_cell(&self, track: u32, pos_in_track: u32) -> u32 {
        nodiv::bar_cell(pos_in_track, self.track(track).recip)
    }

    /// The asset region's bytes.
    pub const fn asset_region(&self) -> &'a [u8] {
        self.assets
    }

    /// Verify the fold over the whole asset region — the **post-audio** pass.
    ///
    /// [`parse`](Self::parse) deliberately does not do this: a cold boot may not
    /// sweep the cart (see [`boot_audit_bytes`]), so v2 moved the O(asset-region)
    /// fold here, after the audio deadline no longer applies. A caller runs it and
    /// reports the result; nothing in `parse` depends on it.
    pub fn verify_assets(&self) -> Result<(), Refusal> {
        let computed = fnv1a32_parts(&[self.assets]);
        if computed == self.assets_fnv {
            Ok(())
        } else {
            Err(Refusal::AssetsMismatch {
                stored: self.assets_fnv,
                computed,
            })
        }
    }

    /// Parse and validate a blob. The one door into [`Album`]: a value of that type
    /// exists only if every rule in this module's list held.
    pub fn parse(blob: &'a [u8]) -> Result<Album<'a>, Refusal> {
        if blob.len() < HEADER_BYTES {
            return Err(Refusal::LengthInconsistent {
                have: blob.len(),
                expected: HEADER_BYTES,
            });
        }
        if &blob[0..4] != b"ALBM" {
            return Err(Refusal::BadMagic);
        }

        let version = blob[4];
        let n_tracks = blob[5];
        let backend = blob[6];
        let rate_hz = word(blob, 8);
        let bar_cells = word(blob, 12);
        let album_samples = word(blob, 16);
        let track_off = word(blob, 20);
        let track_len = word(blob, 24);
        let pool_off = word(blob, 28);
        let pool_len = word(blob, 32);
        let asset_off = word(blob, 36);
        let asset_len = word(blob, 40);
        let stored_structure_fnv = word(blob, STRUCTURE_FNV_AT);
        let stored_assets_fnv = word(blob, ASSETS_FNV_AT);
        let album_title_off = half(blob, 48);
        let album_artist_off = half(blob, 50);
        let year = half(blob, 52);

        // The header's reserved bytes first: they are header-only, so reporting one
        // cannot mask a field the file's length or sections also depend on. v2's
        // asset fold occupies 60..64, so the reserved run is 54..60.
        if blob[7] != 0 {
            return Err(Refusal::NonZeroReserved {
                offset: 7,
                value: blob[7] as u32,
            });
        }
        for (offset, &value) in blob[54..ASSETS_FNV_AT].iter().enumerate() {
            if value != 0 {
                return Err(Refusal::NonZeroReserved {
                    offset: 54 + offset,
                    value: value as u32,
                });
            }
        }

        if version != VERSION {
            return Err(Refusal::UnsupportedVersion { version });
        }
        if !(1..=MAX_TRACKS).contains(&n_tracks) {
            return Err(Refusal::TrackCount { n_tracks });
        }
        if backend != BACKEND_STUB && backend != BACKEND_FLAC {
            return Err(Refusal::BackendUnknown { backend });
        }
        if rate_hz != PRODUCT_RATE_HZ {
            return Err(Refusal::RateNotProductClock { rate_hz });
        }
        if bar_cells != layout::BAR_CELLS {
            return Err(Refusal::BarCellsMismatch { bar_cells });
        }

        // Section order: header ‖ table ‖ pool ‖ assets, contiguous and 4-byte
        // aligned. Nothing is sliced until these hold, and a `u32` overflow is a
        // section-order failure rather than a wrap.
        let aligned = [track_off, track_len, pool_off, pool_len, asset_off]
            .iter()
            .all(|value| value % 4 == 0);
        if !aligned || track_off != HEADER_BYTES as u32 {
            return Err(Refusal::SectionOrder {
                what: "track table",
                offset: track_off,
                expected: HEADER_BYTES as u32,
            });
        }
        let Some(table_end) = track_off.checked_add(track_len) else {
            return Err(Refusal::SectionOrder {
                what: "string pool",
                offset: pool_off,
                expected: u32::MAX,
            });
        };
        if pool_off != table_end {
            return Err(Refusal::SectionOrder {
                what: "string pool",
                offset: pool_off,
                expected: table_end,
            });
        }
        let Some(pool_end) = pool_off.checked_add(pool_len) else {
            return Err(Refusal::SectionOrder {
                what: "asset region",
                offset: asset_off,
                expected: u32::MAX,
            });
        };
        if asset_off != pool_end {
            return Err(Refusal::SectionOrder {
                what: "asset region",
                offset: asset_off,
                expected: pool_end,
            });
        }
        let expected_table = n_tracks as u32 * TRACK_BYTES as u32;
        if track_len != expected_table {
            return Err(Refusal::LengthInconsistent {
                have: track_len as usize,
                expected: expected_table as usize,
            });
        }
        if pool_len as usize >= POOL_LIMIT {
            return Err(Refusal::PoolTooLarge {
                pool_len,
                limit: POOL_LIMIT as u32,
            });
        }
        let Some(end) = asset_off.checked_add(asset_len) else {
            return Err(Refusal::AssetRegionOverflow {
                offset: asset_off,
                len: asset_len,
            });
        };
        if blob.len() != end as usize {
            return Err(Refusal::LengthInconsistent {
                have: blob.len(),
                expected: end as usize,
            });
        }

        // Sections, now that their extents are known to be inside the file.
        let tracks = &blob[track_off as usize..table_end as usize];
        let pool = &blob[pool_off as usize..pool_end as usize];
        let assets = &blob[asset_off as usize..end as usize];

        // Track entries: reserved bytes, sample ranges, asset bounds, reciprocal —
        // and every string they name, so a bad offset is reported before the pool is
        // walked for padding.
        let mut cursor = 0u32;
        for index in 0..n_tracks as u32 {
            let at = index as usize * TRACK_BYTES;
            let entry = &tracks[at..at + TRACK_BYTES];
            for offset in 36..TRACK_BYTES {
                if entry[offset] != 0 {
                    return Err(Refusal::NonZeroReserved {
                        offset: HEADER_BYTES + at + offset,
                        value: entry[offset] as u32,
                    });
                }
            }
            let start_sample = word(entry, 0);
            let n_samples = word(entry, 4);
            let Some(end_sample) = start_sample.checked_add(n_samples) else {
                return Err(Refusal::TrackSampleRange {
                    track: index,
                    start_sample,
                    n_samples,
                    album_samples,
                });
            };
            if n_samples == 0 || start_sample != cursor || end_sample > album_samples {
                return Err(Refusal::TrackSampleRange {
                    track: index,
                    start_sample,
                    n_samples,
                    album_samples,
                });
            }
            cursor = end_sample;

            let track_asset_off = word(entry, 12);
            let track_asset_len = word(entry, 16);
            let fits = track_asset_off
                .checked_add(track_asset_len)
                .is_some_and(|track_end| track_asset_len > 0 && track_end <= asset_len);
            if !fits {
                return Err(Refusal::AssetOutOfBounds {
                    track: index,
                    offset: track_asset_off,
                    len: track_asset_len,
                    asset_len,
                });
            }

            let recip = word(entry, 8);
            if !reciprocal_is_exact(n_samples, recip) {
                return Err(Refusal::ReciprocalInconsistent {
                    track: index,
                    n_samples,
                    recip,
                });
            }

            for offset in [half(entry, 24), half(entry, 26)] {
                string_at(pool, offset)?;
            }
        }
        if cursor != album_samples {
            return Err(Refusal::TrackSampleRange {
                track: n_tracks as u32,
                start_sample: cursor,
                n_samples: 0,
                album_samples,
            });
        }

        string_at(pool, album_title_off)?;
        string_at(pool, album_artist_off)?;

        // Padding: every byte of the pool belongs to a NUL-terminated string, and a
        // non-empty string has to be one an entry actually points at — which is how
        // a non-zero padding byte is caught even when it happens to be ASCII.
        let mut at = 0usize;
        while at < pool.len() {
            let start = at;
            while at < pool.len() && pool[at] != 0 {
                at += 1;
            }
            if at == pool.len() {
                return Err(Refusal::MissingTerminator {
                    offset: start as u16,
                });
            }
            if at > start
                && !is_referenced(tracks, n_tracks, album_title_off, album_artist_off, start)
            {
                return Err(Refusal::PaddingNotZero {
                    offset: start,
                    value: pool[start],
                });
            }
            at += 1;
        }

        // The structure fold, over the kilobytes boot is allowed to read. The asset
        // region's own fold is the post-audio audit's (`verify_assets`); leaving it
        // out of `parse` is what makes a cold boot bounded.
        let computed = fnv1a32_parts(&[tracks, pool]);
        if stored_structure_fnv != computed {
            return Err(Refusal::ChecksumMismatch {
                stored: stored_structure_fnv,
                computed,
            });
        }

        Ok(Album {
            rate_hz,
            backend,
            n_tracks,
            album_samples,
            year,
            album_title_off,
            album_artist_off,
            tracks,
            pool,
            assets,
            assets_fnv: stored_assets_fnv,
        })
    }

    /// The `&str` a pool offset names. Validation is [`Album::parse`]'s job; this is the
    /// accessor side of the same rule, and its panics cannot fire on an `Album`.
    fn string(&self, offset: u16) -> &'a str {
        let rest = &self.pool[offset as usize..];
        let len = rest
            .iter()
            .position(|&b| b == 0)
            .expect("parse validated every referenced string's terminator");
        core::str::from_utf8(&rest[..len])
            .expect("parse validated every referenced string as ASCII")
    }
}

/// FNV-1a/32 over the first [`BOOT_PREFIX_BYTES`] of a track's asset — the prefix
/// half of the bounded content tripwire `backend::verify_pins` holds to the bake's
/// pin. A shorter asset folds whole; a boot must not read past the bound.
pub fn prefix_fnv(asset: &[u8]) -> u32 {
    fnv1a32_parts(&[&asset[..asset.len().min(BOOT_PREFIX_BYTES)]])
}

/// FNV-1a/32 over the last [`BOOT_SUFFIX_BYTES`] of a track's asset — the suffix
/// half of the same tripwire. An asset shorter than the suffix folds whole, so the
/// two spans simply overlap; the pin is computed the same way by the bake.
pub fn suffix_fnv(asset: &[u8]) -> u32 {
    let at = asset.len().saturating_sub(BOOT_SUFFIX_BYTES);
    fnv1a32_parts(&[&asset[at..]])
}

/// Should `recip` be `floor(2^32 × N / n_samples)` on the grid `N` this ROM was
/// generated for ([`layout::BAR_CELLS`])? Two 64-bit multiplies and no divide —
/// exactly the inequality the floor satisfies.
fn reciprocal_is_exact(n_samples: u32, recip: u32) -> bool {
    let target = (layout::BAR_CELLS as u64) << 32;
    (recip as u64) * (n_samples as u64) <= target
        && ((recip as u64) + 1) * (n_samples as u64) > target
}

/// Whether any entry's title/artist offset is `at` — the reference test the
/// padding walk needs, without allocating a list of the offsets.
fn is_referenced(
    tracks: &[u8],
    n_tracks: u8,
    album_title_off: u16,
    album_artist_off: u16,
    at: usize,
) -> bool {
    if at as u16 == album_title_off || at as u16 == album_artist_off {
        return true;
    }
    for index in 0..n_tracks as usize {
        let entry = &tracks[index * TRACK_BYTES..(index + 1) * TRACK_BYTES];
        if at as u16 == half(entry, 24) || at as u16 == half(entry, 26) {
            return true;
        }
    }
    false
}

/// Resolve one pool string for validation: in range, ASCII, NUL-terminated.
fn string_at(pool: &[u8], offset: u16) -> Result<&str, Refusal> {
    let at = offset as usize;
    if at >= pool.len() {
        return Err(Refusal::PoolOffsetOutOfRange {
            offset,
            pool_len: pool.len() as u32,
        });
    }
    let rest = &pool[at..];
    let Some(len) = rest.iter().position(|&b| b == 0) else {
        return Err(Refusal::MissingTerminator { offset });
    };
    let bytes = &rest[..len];
    for (index, &byte) in bytes.iter().enumerate() {
        if !byte.is_ascii() {
            return Err(Refusal::TitleNotAscii {
                offset: offset + index as u16,
                byte,
            });
        }
    }
    Ok(core::str::from_utf8(bytes).expect("ASCII is valid UTF-8"))
}

fn word(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}

fn half(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}
