//! `ART1`, read and refused — the Phase 0 probe's art blob.
//!
//! `examples/album_probe/ART1.md` is the pinned layout; this module and
//! `scripts/album_probe_art.py` are written against that file independently, so
//! the reader is not a description of the writer's habits. Two rules from the
//! spec shape everything here:
//!
//! - **Refuse before indexing.** Every header field is validated, then the
//!   section length it implies is checked against the file, and only then is a
//!   slice taken. Nothing defaults, clamps or re-parses.
//! - **An off-palette colour is an error, never a nearest-match.** The writer
//!   refuses at bake time; on this side the corresponding rule is that a map
//!   entry naming a tile or a bank that does not exist is a refusal, not a
//!   modulo.
//!
//! The checksum is verified **last**, after the semantic checks, and that order
//! is deliberate: a blob that is both wrong and stale then reports the field
//! that is wrong rather than masking it as an integrity failure. A blob whose
//! tile bytes were corrupted still fails — on the checksum — because those bytes
//! carry no other rule.
//!
//! ## The blit unit
//!
//! [`Art1::tile_words`] returns `[u32; 8]`: one 32-bit word per tile row, row 0
//! first, eight 4bpp pixels per word, **low nibble = leftmost pixel** (the
//! hardware convention restated by `ART1.md` — a 4bpp tile is 8 words of 4 bytes,
//! not eight 2-byte words). The ROM's upload is then
//! `for tile in 0..art.n_tiles() { for (y, word) in art.tile_words(tile) … }`
//! plus [`Art1::palette_bank`] per bank — no layout decisions left at the call
//! site.
//!
//! Host-pure: no `agb`, no registers, no allocation, no unsafe.

/// A blob that failed validation, by rule. One variant per refusal in
/// `ART1.md`'s list; the payload is what the rule was checked against, so a
/// screen or a log line can say *which* field was wrong.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// The first four bytes are not `ART1`.
    BadMagic,
    /// The version field is not 1 (the only version this reader knows).
    UnsupportedVersion {
        /// The version the blob declared.
        version: u32,
    },
    /// The tile stride is not 32 bytes (4bpp, the only mode this probe reads).
    TileBytesNot32 {
        /// The stride the blob declared.
        tile_bytes: u32,
    },
    /// A dimension is not a whole number of 8-pixel tiles.
    DimensionNotMultipleOf8 {
        /// The width the blob declared.
        width_px: u32,
        /// The height the blob declared.
        height_px: u32,
    },
    /// A dimension is outside what the visible screen is (`8..=240` × `8..=160`).
    DimensionOutOfRange {
        /// The width the blob declared.
        width_px: u32,
        /// The height the blob declared.
        height_px: u32,
    },
    /// A 4bpp map has 1..=4 palette banks; 0 is unreadable and 5 does not fit.
    PaletteBankCount {
        /// The bank count the blob declared.
        n_palette_banks: u32,
    },
    /// `tile_cols`/`tile_rows` disagree with the dimensions they derive from.
    TileGridInconsistent {
        /// The column count the blob declared.
        tile_cols: u32,
        /// The row count the blob declared.
        tile_rows: u32,
    },
    /// The file is not the length the header's own counts derive.
    LengthInconsistent {
        /// Bytes actually present.
        have: usize,
        /// Bytes the header implies (padding included).
        expected: usize,
    },
    /// The stored checksum is not FNV-1a/32 over palettes ‖ tiles ‖ map.
    ChecksumMismatch {
        /// The value in the header.
        stored: u32,
        /// The value the sections produce.
        computed: u32,
    },
    /// A BG-map entry names a tile the tiles section does not contain.
    MapEntryTileId {
        /// Cell index (row-major, 0 is the top-left tile).
        cell: usize,
        /// The tile id the entry named.
        tile: u16,
        /// Tiles actually present.
        tile_count: u32,
    },
    /// A BG-map entry names a palette bank that was not written.
    MapEntryPaletteBank {
        /// Cell index (row-major, 0 is the top-left tile).
        cell: usize,
        /// The bank the entry named.
        bank: u16,
        /// Banks actually present.
        n_palette_banks: u32,
    },
    /// A word in one of the header's reserved ranges is not zero.
    ReservedWordNonZero {
        /// Byte offset of the word.
        offset: usize,
        /// The value found there.
        value: u32,
    },
}

impl core::fmt::Display for Refusal {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match *self {
            Refusal::BadMagic => f.write_str("magic is not ART1"),
            Refusal::UnsupportedVersion { version } => {
                write!(f, "version {version}, this reader reads 1")
            }
            Refusal::TileBytesNot32 { tile_bytes } => {
                write!(f, "tile_bytes {tile_bytes}, the only mode is 32 (4bpp)")
            }
            Refusal::DimensionNotMultipleOf8 {
                width_px,
                height_px,
            } => write!(
                f,
                "{width_px}x{height_px} is not a whole number of 8x8 tiles"
            ),
            Refusal::DimensionOutOfRange {
                width_px,
                height_px,
            } => write!(
                f,
                "{width_px}x{height_px} is outside {}x{}..={MAX_WIDTH_PX}x{MAX_HEIGHT_PX}",
                TILE_PX, TILE_PX
            ),
            Refusal::PaletteBankCount { n_palette_banks } => {
                write!(f, "n_palette_banks {n_palette_banks}, the range is 1..=4")
            }
            Refusal::TileGridInconsistent {
                tile_cols,
                tile_rows,
            } => write!(
                f,
                "{tile_cols}x{tile_rows} tiles disagrees with the dimensions"
            ),
            Refusal::LengthInconsistent { have, expected } => {
                write!(f, "{have} bytes, the header derives {expected}")
            }
            Refusal::ChecksumMismatch { stored, computed } => {
                write!(
                    f,
                    "checksum {stored:#010x}, sections hash to {computed:#010x}"
                )
            }
            Refusal::MapEntryTileId {
                cell,
                tile,
                tile_count,
            } => write!(
                f,
                "map cell {cell} names tile {tile}, there are {tile_count}"
            ),
            Refusal::MapEntryPaletteBank {
                cell,
                bank,
                n_palette_banks,
            } => write!(
                f,
                "map cell {cell} names bank {bank}, there are {n_palette_banks}"
            ),
            Refusal::ReservedWordNonZero { offset, value } => {
                write!(f, "reserved word at {offset} is {value:#010x}, must be 0")
            }
        }
    }
}

/// Header length in bytes. The three sections follow it immediately.
pub const HEADER_BYTES: usize = 64;
/// A 4bpp tile: 8 words / 32 bytes.
pub const TILE_BYTES: usize = 32;
/// VRAM words per tile (one row each).
pub const WORDS_PER_TILE: usize = 8;
/// `Rgb15` entries in a palette bank.
pub const PALETTE_ENTRIES: usize = 16;
/// Bytes per palette bank.
pub const PALETTE_BANK_BYTES: usize = PALETTE_ENTRIES * 2;
/// Widest image the visible screen can show.
pub const MAX_WIDTH_PX: u32 = 240;
/// Tallest image the visible screen can show.
pub const MAX_HEIGHT_PX: u32 = 160;
/// Side of one tile, in pixels.
pub const TILE_PX: u32 = 8;
/// `ART1`'s version field for this layout.
pub const VERSION: u32 = 1;

const FNV_OFFSET_BASIS: u32 = 0x811c_9dc5;
const FNV_PRIME: u32 = 0x0100_0193;

/// FNV-1a/32, the checksum `ART1.md` specifies — over the section bytes as they
/// sit in the file (`palettes ‖ tiles ‖ map`, concatenated in that order).
pub fn fnv1a32(bytes: &[u8]) -> u32 {
    fnv1a32_parts(&[bytes])
}

/// FNV-1a/32 over several spans without copying them together.
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

/// A validated `ART1` blob, borrowing its sections from the input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Art1<'a> {
    /// Image width in pixels.
    pub width_px: u32,
    /// Image height in pixels.
    pub height_px: u32,
    /// Palette banks the blob carries (`1..=4`).
    pub n_palette_banks: u32,
    /// Tiles per row (== `width_px / 8`).
    pub tile_cols: u32,
    /// Tile rows (== `height_px / 8`).
    pub tile_rows: u32,
    /// The Bayer strength the tool printed and recorded (`0..=255`, informational).
    pub dither_strength: u32,
    /// FNV-1a/32 of the tool's pre-quantisation RGB bytes (informational).
    pub source_hash: u32,
    /// The header's checksum, already re-verified by [`parse`].
    pub checksum: u32,
    /// `n_palette_banks × 32` bytes, bank 0 first, `Rgb15` entries in order.
    pub palettes: &'a [u8],
    /// `tile_cols × tile_rows × 32` bytes, row-major tile order.
    pub tiles: &'a [u8],
    /// `tile_cols × tile_rows` little-endian BG-map entries, same order.
    pub map: &'a [u8],
}

impl Art1<'_> {
    /// How many tiles the blob carries.
    pub const fn n_tiles(&self) -> u32 {
        self.tile_cols * self.tile_rows
    }

    /// The BG-map entry for one cell (row-major, 0 is the top-left tile):
    /// `tile | hflip<<10 | vflip<<11 | palette_bank<<12`, as the blob stores it.
    ///
    /// # Panics
    ///
    /// If `cell >= n_tiles()`. Validation already refused a blob whose entries
    /// name objects that do not exist; this is for callers iterating the map.
    pub fn map_entry(&self, cell: u32) -> u16 {
        assert!(cell < self.n_tiles(), "map cell {cell} out of range");
        let at = cell as usize * 2;
        u16::from_le_bytes([self.map[at], self.map[at + 1]])
    }

    /// The 8 VRAM words of one tile — the blit unit. Row 0 first; word `y` holds
    /// all eight pixels of row `y`, low nibble leftmost (so the blob's byte order
    /// is the hardware's nibble order, little-endian per word).
    ///
    /// # Panics
    ///
    /// If `tile >= n_tiles()`.
    pub fn tile_words(&self, tile: u32) -> [u32; WORDS_PER_TILE] {
        assert!(tile < self.n_tiles(), "tile {tile} out of range");
        let at = tile as usize * TILE_BYTES;
        let mut words = [0u32; WORDS_PER_TILE];
        for (y, word) in words.iter_mut().enumerate() {
            let row = at + y * 4;
            *word = u32::from_le_bytes([
                self.tiles[row],
                self.tiles[row + 1],
                self.tiles[row + 2],
                self.tiles[row + 3],
            ]);
        }
        words
    }

    /// One palette bank as the `Rgb15` entries the ROM uploads, entry 0 first.
    ///
    /// # Panics
    ///
    /// If `bank >= n_palette_banks` (the filter `parse` applies to the map too).
    pub fn palette_bank(&self, bank: u32) -> [u16; PALETTE_ENTRIES] {
        assert!(
            bank < self.n_palette_banks,
            "palette bank {bank} out of range"
        );
        let at = bank as usize * PALETTE_BANK_BYTES;
        let mut entries = [0u16; PALETTE_ENTRIES];
        for (i, entry) in entries.iter_mut().enumerate() {
            let off = at + i * 2;
            *entry = u16::from_le_bytes([self.palettes[off], self.palettes[off + 1]]);
        }
        entries
    }
}

/// Parse and validate a blob. The one door into [`Art1`]: a value of that type
/// exists only if every rule in `ART1.md`'s list held.
pub fn parse(blob: &[u8]) -> Result<Art1<'_>, Refusal> {
    if blob.len() < HEADER_BYTES {
        return Err(Refusal::LengthInconsistent {
            have: blob.len(),
            expected: HEADER_BYTES,
        });
    }
    if &blob[0..4] != b"ART1" {
        return Err(Refusal::BadMagic);
    }

    let version = word(blob, 4);
    let width_px = word(blob, 8);
    let height_px = word(blob, 12);
    let tile_bytes = word(blob, 16);
    let n_palette_banks = word(blob, 20);
    let tile_cols = word(blob, 24);
    let tile_rows = word(blob, 28);
    let dither_strength = word(blob, 32);
    let source_hash = word(blob, 36);

    // The reserved words first: they are header-only, so reporting one cannot
    // mask a field the file's length or sections also depend on.
    for offset in [40, 44, 52, 56, 60] {
        let value = word(blob, offset);
        if value != 0 {
            return Err(Refusal::ReservedWordNonZero { offset, value });
        }
    }

    if version != VERSION {
        return Err(Refusal::UnsupportedVersion { version });
    }
    if tile_bytes != TILE_BYTES as u32 {
        return Err(Refusal::TileBytesNot32 { tile_bytes });
    }
    if width_px % TILE_PX != 0 || height_px % TILE_PX != 0 {
        return Err(Refusal::DimensionNotMultipleOf8 {
            width_px,
            height_px,
        });
    }
    if !(TILE_PX..=MAX_WIDTH_PX).contains(&width_px)
        || !(TILE_PX..=MAX_HEIGHT_PX).contains(&height_px)
    {
        return Err(Refusal::DimensionOutOfRange {
            width_px,
            height_px,
        });
    }
    if !(1..=4).contains(&n_palette_banks) {
        return Err(Refusal::PaletteBankCount { n_palette_banks });
    }
    if tile_cols != width_px / TILE_PX || tile_rows != height_px / TILE_PX {
        return Err(Refusal::TileGridInconsistent {
            tile_cols,
            tile_rows,
        });
    }

    // Length is derived from the header's own counts, never inferred from the
    // file size — that check is why the header carries them (§ART1.md).
    let n_tiles = (tile_cols * tile_rows) as usize;
    let palette_bytes = n_palette_banks as usize * PALETTE_BANK_BYTES;
    let tile_section = n_tiles * TILE_BYTES;
    let map_section = n_tiles * 2;
    let unpadded = HEADER_BYTES + palette_bytes + tile_section + map_section;
    let expected = unpadded.div_ceil(4) * 4;
    if blob.len() != expected {
        return Err(Refusal::LengthInconsistent {
            have: blob.len(),
            expected,
        });
    }

    // Sections, now that their extents are known to be inside the file.
    let palettes = &blob[HEADER_BYTES..HEADER_BYTES + palette_bytes];
    let tiles_at = HEADER_BYTES + palette_bytes;
    let tiles = &blob[tiles_at..tiles_at + tile_section];
    let map_at = tiles_at + tile_section;
    let map = &blob[map_at..map_at + map_section];

    // Semantic refusals before the integrity one: a map entry naming a tile that
    // does not exist is a more specific fact than "the bytes changed".
    for cell in 0..n_tiles {
        let at = cell * 2;
        let entry = u16::from_le_bytes([map[at], map[at + 1]]);
        let tile = entry & 0x03ff;
        if tile as u32 >= n_tiles as u32 {
            return Err(Refusal::MapEntryTileId {
                cell,
                tile,
                tile_count: n_tiles as u32,
            });
        }
        let bank = entry >> 12;
        if bank as u32 >= n_palette_banks {
            return Err(Refusal::MapEntryPaletteBank {
                cell,
                bank,
                n_palette_banks,
            });
        }
    }

    let computed = fnv1a32_parts(&[palettes, tiles, map]);
    let stored = word(blob, 48);
    if stored != computed {
        return Err(Refusal::ChecksumMismatch { stored, computed });
    }

    Ok(Art1 {
        width_px,
        height_px,
        n_palette_banks,
        tile_cols,
        tile_rows,
        dither_strength,
        source_hash,
        checksum: stored,
        palettes,
        tiles,
        map,
    })
}

fn word(blob: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        blob[offset],
        blob[offset + 1],
        blob[offset + 2],
        blob[offset + 3],
    ])
}
