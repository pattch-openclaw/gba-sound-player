//! ULI2/ULA2 format constants from `gba-audio/src/uli/uli_format.h`.

/// ULA2 album magic: `"ULA2"` little-endian.
pub const MAGIC: u32 = 0x3241_4C55;
/// ULA2 container version.
pub const VERSION: u16 = 1;
/// The format's fixed sample rate.
pub const RATE: u32 = 32_768;
/// Hop = coefficients per channel per frame; also the output samples per frame.
pub const M: usize = 512;
/// M / 2: the overlap length and the complex-FFT size.
pub const H: usize = 256;
/// Coefficients per band.
pub const BANDW: usize = 16;
/// Bands per channel.
pub const NBANDS: usize = 32;
/// Coefficients 0..NCODED-1 may be nonzero; the rest must be zero by format rule.
pub const NCODED: usize = 500;
/// Maximum number of band classes (including class 0 = zero band).
pub const MAXK: usize = 16;
/// Maximum Huffman code length.
pub const MAXLEN: usize = 16;
/// First-level Huffman LUT width. This is an implementation detail, not bitstream.
pub const LUTB: usize = 10;
/// Number of quantizer steps in `tables_data::STEP`.
pub const NSTEP: usize = 192;
/// Encoder guarantee: |X| < 2^28 per coded channel (< 2^29 after M/S).
pub const XLIM: i64 = 1 << 28;
/// Frames between seek entries (4 seconds at 32,768 Hz).
pub const SEEK_INTERVAL: u32 = 256;
/// Format limit on the number of tracks.
pub const MAX_TRACKS: usize = 32;
/// `sizeof(UliAlbumHeader)` in the C reference.
pub const ALBUM_HEADER_BYTES: usize = 88;
/// `sizeof(UliTrackEntry)` in the C reference.
pub const TRACK_ENTRY_BYTES: usize = 24;
/// `sizeof(UliSeekEntry)` in the C reference.
pub const SEEK_ENTRY_BYTES: usize = 8;

/// Band class 0: the band is zero.
pub const CT_ZERO: u8 = 0;
/// Band class QUAD: four coefficients per Huffman symbol.
pub const CT_QUAD: u8 = 1;
/// Band class PAIR: two coefficients per Huffman symbol.
pub const CT_PAIR: u8 = 2;
