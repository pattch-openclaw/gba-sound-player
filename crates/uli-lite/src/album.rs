//! ULA2 album container parser.
//!
//! Mirrors `gba-audio/src/uli/uli_album.c`: the header and every section are
//! validated before the decoder sees them, and no data is copied. Unlike the C
//! parser this module reads fields byte-by-byte, so the blob does not need to
//! be 4-byte aligned; the format's own alignment rules for the section
//! offsets are still enforced.

use core::fmt;

use crate::Error;
use crate::format::{
    ALBUM_HEADER_BYTES, M, MAGIC, MAX_TRACKS, RATE, SEEK_ENTRY_BYTES, SEEK_INTERVAL,
    TRACK_ENTRY_BYTES, VERSION,
};

/// Parsed `UliAlbumHeader` (88 bytes in the reference layout).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AlbumHeader {
    pub magic: u32,
    pub version: u16,
    pub header_bytes: u16,
    pub sample_rate: u32,
    pub hop: u32,
    pub n_frames: u32,
    pub n_samples: u32,
    pub n_tracks: u32,
    pub tracks_off: u32,
    pub tables_off: u32,
    pub tables_bytes: u32,
    pub seek_off: u32,
    pub seek_count: u32,
    pub seek_interval: u32,
    pub strings_off: u32,
    pub strings_bytes: u32,
    pub stream_off: u32,
    pub stream_halfwords: u32,
    /// Encoder cycle-model estimate for the heaviest frame, in ARM7 cycles at
    /// the 4/2 wait state. An estimate, not a hardware measurement.
    pub est_max_cycles: u32,
    /// Encoder cycle-model mean frame, in ARM7 cycles at 4/2.
    pub est_mean_cycles: u32,
}

/// Parsed `UliTrackEntry` (24 bytes in the reference layout).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TrackEntry {
    pub start_sample: u32,
    pub n_samples: u32,
    pub preroll_frame: u32,
    pub bitpos: u32,
    pub g_prev: u16,
    pub skip: u16,
    pub title_off: u16,
    pub artist_off: u16,
}

/// Parsed `UliSeekEntry` (8 bytes in the reference layout).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SeekEntry {
    pub bitpos: u32,
    pub g_prev: u16,
}

/// A borrowed, validated ULA2 album.
#[derive(Clone, Copy)]
pub struct Album<'a> {
    blob: &'a [u8],
    header: AlbumHeader,
    tables_off: usize,
    tables_bytes: usize,
    seek_off: usize,
    strings_off: usize,
    strings_bytes: usize,
    stream_off: usize,
    stream_bytes: usize,
}

impl fmt::Debug for Album<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Album")
            .field("header", &self.header)
            .field("tables_bytes", &self.tables_bytes)
            .field("strings_bytes", &self.strings_bytes)
            .field("stream_bytes", &self.stream_bytes)
            .finish_non_exhaustive()
    }
}

fn u16_at(b: &[u8], off: usize) -> Result<u16, Error> {
    let s = b.get(off..off + 2).ok_or(Error::BadAlbum)?;
    Ok(u16::from_le_bytes([s[0], s[1]]))
}

fn u32_at(b: &[u8], off: usize) -> Result<u32, Error> {
    let s = b.get(off..off + 4).ok_or(Error::BadAlbum)?;
    Ok(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

fn in_blob(blob: &[u8], off: usize, bytes: usize) -> Result<(), Error> {
    let end = off.checked_add(bytes).ok_or(Error::BadAlbum)?;
    if end <= blob.len() {
        Ok(())
    } else {
        Err(Error::BadAlbum)
    }
}

fn in_blob_n(blob: &[u8], off: u32, n: u32, elem: usize) -> Result<(), Error> {
    let bytes = (n as usize).checked_mul(elem).ok_or(Error::BadAlbum)?;
    in_blob(blob, off as usize, bytes)
}

impl<'a> Album<'a> {
    /// Parse and validate a ULA2 blob.
    pub fn parse(blob: &'a [u8]) -> Result<Self, Error> {
        if blob.len() < ALBUM_HEADER_BYTES {
            return Err(Error::BadAlbum);
        }
        let header = AlbumHeader {
            magic: u32_at(blob, 0)?,
            version: u16_at(blob, 4)?,
            header_bytes: u16_at(blob, 6)?,
            sample_rate: u32_at(blob, 8)?,
            hop: u32_at(blob, 12)?,
            n_frames: u32_at(blob, 16)?,
            n_samples: u32_at(blob, 20)?,
            n_tracks: u32_at(blob, 24)?,
            tracks_off: u32_at(blob, 28)?,
            tables_off: u32_at(blob, 32)?,
            tables_bytes: u32_at(blob, 36)?,
            seek_off: u32_at(blob, 40)?,
            seek_count: u32_at(blob, 44)?,
            seek_interval: u32_at(blob, 48)?,
            strings_off: u32_at(blob, 52)?,
            strings_bytes: u32_at(blob, 56)?,
            stream_off: u32_at(blob, 60)?,
            stream_halfwords: u32_at(blob, 64)?,
            est_max_cycles: u32_at(blob, 68)?,
            est_mean_cycles: u32_at(blob, 72)?,
        };

        if header.magic != MAGIC
            || header.version != VERSION
            || header.header_bytes as usize != ALBUM_HEADER_BYTES
        {
            return Err(Error::BadAlbum);
        }
        if header.sample_rate != RATE || header.hop != M as u32 {
            return Err(Error::BadAlbum);
        }
        if header.n_tracks < 1
            || header.n_tracks as usize > MAX_TRACKS
            || header.n_frames < 1
            || header.seek_interval != SEEK_INTERVAL
        {
            return Err(Error::BadAlbum);
        }
        // Every frame has a seek entry at or before it.
        if header.seek_count < ((header.n_frames - 1) >> 8) + 1 {
            return Err(Error::BadAlbum);
        }
        if header.tracks_off & 3 != 0 || header.seek_off & 3 != 0 || header.stream_off & 1 != 0 {
            return Err(Error::BadAlbum);
        }
        if header.stream_halfwords < 4 || header.stream_halfwords > 0x0fff_ffff {
            return Err(Error::BadAlbum);
        }

        in_blob_n(blob, header.tracks_off, header.n_tracks, TRACK_ENTRY_BYTES)?;
        in_blob(
            blob,
            header.tables_off as usize,
            header.tables_bytes as usize,
        )?;
        in_blob_n(blob, header.seek_off, header.seek_count, SEEK_ENTRY_BYTES)?;
        in_blob(
            blob,
            header.strings_off as usize,
            header.strings_bytes as usize,
        )?;
        in_blob_n(blob, header.stream_off, header.stream_halfwords, 2)?;

        let nbits = header.stream_halfwords as u64 * 16;
        let strings_end = (header.strings_off as usize)
            .checked_add(header.strings_bytes as usize)
            .ok_or(Error::BadAlbum)?;
        if header.strings_bytes != 0 && blob[strings_end - 1] != 0 {
            return Err(Error::BadAlbum);
        }
        for t in 0..header.n_tracks as usize {
            let tr = read_track(blob, header.tracks_off as usize + t * TRACK_ENTRY_BYTES)?;
            if tr.title_off as u32 >= header.strings_bytes
                || tr.artist_off as u32 >= header.strings_bytes
                || tr.preroll_frame >= header.n_frames
                || tr.bitpos as u64 >= nbits
            {
                return Err(Error::BadAlbum);
            }
        }
        for i in 0..header.seek_count as usize {
            let sk = read_seek(blob, header.seek_off as usize + i * SEEK_ENTRY_BYTES)?;
            if sk.bitpos as u64 >= nbits {
                return Err(Error::BadAlbum);
            }
        }

        Ok(Self {
            blob,
            header,
            tables_off: header.tables_off as usize,
            tables_bytes: header.tables_bytes as usize,
            seek_off: header.seek_off as usize,
            strings_off: header.strings_off as usize,
            strings_bytes: header.strings_bytes as usize,
            stream_off: header.stream_off as usize,
            stream_bytes: header.stream_halfwords as usize * 2,
        })
    }

    pub fn header(&self) -> &AlbumHeader {
        &self.header
    }

    pub fn tables(&self) -> &'a [u8] {
        &self.blob[self.tables_off..self.tables_off + self.tables_bytes]
    }

    pub fn strings(&self) -> &'a [u8] {
        &self.blob[self.strings_off..self.strings_off + self.strings_bytes]
    }

    /// The ULA2 stream as little-endian halfword bytes, including the format's
    /// padding halfwords.
    pub fn stream(&self) -> &'a [u8] {
        &self.blob[self.stream_off..self.stream_off + self.stream_bytes]
    }

    pub fn n_tracks(&self) -> usize {
        self.header.n_tracks as usize
    }

    pub fn n_frames(&self) -> u32 {
        self.header.n_frames
    }

    pub fn seek_count(&self) -> usize {
        self.header.seek_count as usize
    }

    pub fn seek_interval(&self) -> u32 {
        self.header.seek_interval
    }

    pub fn track(&self, index: usize) -> Option<TrackEntry> {
        if index >= self.n_tracks() {
            return None;
        }
        read_track(
            self.blob,
            self.header.tracks_off as usize + index * TRACK_ENTRY_BYTES,
        )
        .ok()
    }

    pub fn seek(&self, index: usize) -> Option<SeekEntry> {
        if index >= self.seek_count() {
            return None;
        }
        read_seek(self.blob, self.seek_off + index * SEEK_ENTRY_BYTES).ok()
    }

    /// The NUL-terminated title for a track, as UTF-8 when the tag is valid.
    pub fn title(&self, index: usize) -> Option<&'a str> {
        self.string_field(self.track(index)?.title_off)
    }

    /// The NUL-terminated artist for a track, as UTF-8 when the tag is valid.
    pub fn artist(&self, index: usize) -> Option<&'a str> {
        self.string_field(self.track(index)?.artist_off)
    }

    fn string_field(&self, off: u16) -> Option<&'a str> {
        let start = self.strings_off.checked_add(off as usize)?;
        let end = self.strings_off.checked_add(self.strings_bytes)?;
        if start >= end {
            return None;
        }
        let s = self.blob.get(start..end)?;
        let nul = s.iter().position(|&b| b == 0)?;
        core::str::from_utf8(&s[..nul]).ok()
    }
}

fn read_track(b: &[u8], off: usize) -> Result<TrackEntry, Error> {
    Ok(TrackEntry {
        start_sample: u32_at(b, off)?,
        n_samples: u32_at(b, off + 4)?,
        preroll_frame: u32_at(b, off + 8)?,
        bitpos: u32_at(b, off + 12)?,
        g_prev: u16_at(b, off + 16)?,
        skip: u16_at(b, off + 18)?,
        title_off: u16_at(b, off + 20)?,
        artist_off: u16_at(b, off + 22)?,
    })
}

fn read_seek(b: &[u8], off: usize) -> Result<SeekEntry, Error> {
    Ok(SeekEntry {
        bitpos: u32_at(b, off)?,
        g_prev: u16_at(b, off + 4)?,
    })
}
