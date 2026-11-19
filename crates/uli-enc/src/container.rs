//! The ULA2 container writer.
//!
//! Field order and endianness are `uli_lite::album::Album::parse`'s (the parser
//! is the authority; `album.rs:109-131`), and the section offsets are the ones
//! the header advertises — no section is found by arithmetic at read time. The
//! layout is the reference's: header, track entries, tables, seek table,
//! strings, frame stream.
//!
//! The cycle estimates are the one field filled in later: they come from the
//! self-verify decode of the finished stream, so [`emit`] leaves them zero and
//! [`patch_cycles`] writes them afterwards.

use uli_lite::format::{
    ALBUM_HEADER_BYTES, M, MAGIC, RATE, SEEK_ENTRY_BYTES, SEEK_INTERVAL, TRACK_ENTRY_BYTES, VERSION,
};

use crate::plan::Plan;
use crate::tables_default::DefaultTables;

/// A frame's position in the stream and the `g` state entering it.
///
/// `g_prev` is the value the decoder must start from to read this frame, i.e.
/// the previous frame's `g` (0 before frame 0) — the seek table and the track
/// entries store exactly this.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FramePos {
    pub bitpos: u64,
    pub g_prev: i32,
}

/// Header offsets of `est_max_cycles` / `est_mean_cycles`, the 17th and 18th
/// fields of `AlbumHeader` (see `uli_lite::album`); they are written after the
/// stream they describe has been decoded.
const EST_MAX_OFFSET: usize = 68;
const EST_MEAN_OFFSET: usize = 72;

/// Write one of the container's two `g_prev` fields.
///
/// The store is `u16` and the reader zero-extends it (`uli_lite::frame`
/// `seek_track`/`seek_entry`), so a negative or enormous `g` would come back as a
/// different frame than the encoder wrote. `GPlan::legal` is what makes this exact
/// — it bounds `g` to `0 .. NSTEP + 4` — and the assert is the cheap witness that
/// the two sides still agree if either bound moves.
fn push_g(b: &mut Vec<u8>, g_prev: i32) {
    debug_assert!(
        u16::try_from(g_prev).is_ok(),
        "g_prev {g_prev} does not survive the container's u16 field; GPlan::validate \
         should have refused the plan that produced it"
    );
    push_u16(b, g_prev as u16);
}

/// Write the whole blob. `stream` is the finished (padded) frame stream.
pub fn emit(plan: &Plan, tables: &DefaultTables, frames: &[FramePos], stream: &[u8]) -> Vec<u8> {
    assert_eq!(
        frames.len(),
        plan.n_frames,
        "one stream position per planned frame"
    );

    let tracks_off = ALBUM_HEADER_BYTES;
    let tables_off = tracks_off + plan.tracks.len() * TRACK_ENTRY_BYTES;
    let seek_off = tables_off + plan.tables_bytes;
    let strings_off = seek_off + plan.seek_count * SEEK_ENTRY_BYTES;
    let stream_off = strings_off + plan.strings.len();
    let stream_halfwords = stream.len() / 2;

    let mut blob = Vec::with_capacity(stream_off + stream.len());
    blob.resize(tracks_off, 0);

    for t in &plan.tracks {
        let bitpos = frames[t.preroll_frame as usize].bitpos;
        let g_prev = frames[t.preroll_frame as usize].g_prev;
        push_u32(&mut blob, t.start_sample);
        push_u32(&mut blob, t.n_samples);
        push_u32(&mut blob, t.preroll_frame);
        push_u32(
            &mut blob,
            u32::try_from(bitpos).expect("the stream fits the format's u32 bit positions"),
        );
        push_g(&mut blob, g_prev);
        push_u16(&mut blob, t.skip);
        push_u16(&mut blob, t.title_off);
        push_u16(&mut blob, t.artist_off);
    }

    blob.extend_from_slice(&tables.to_blob());

    for i in 0..plan.seek_count {
        let f = i * SEEK_INTERVAL as usize;
        push_u32(
            &mut blob,
            u32::try_from(frames[f].bitpos)
                .expect("the stream fits the format's u32 bit positions"),
        );
        push_g(&mut blob, frames[f].g_prev);
        // UliSeekEntry is 8 bytes; the trailing halfword is padding.
        push_u16(&mut blob, 0);
    }

    blob.extend_from_slice(&plan.strings);
    blob.extend_from_slice(stream);

    let mut header = vec![0u8; ALBUM_HEADER_BYTES];
    put_u32(&mut header, 0, MAGIC);
    put_u16(&mut header, 4, VERSION);
    put_u16(&mut header, 6, ALBUM_HEADER_BYTES as u16);
    put_u32(&mut header, 8, RATE);
    put_u32(&mut header, 12, M as u32);
    put_u32(&mut header, 16, plan.n_frames as u32);
    put_u32(&mut header, 20, plan.n_samples);
    put_u32(&mut header, 24, plan.tracks.len() as u32);
    put_u32(&mut header, 28, tracks_off as u32);
    put_u32(&mut header, 32, tables_off as u32);
    put_u32(&mut header, 36, plan.tables_bytes as u32);
    put_u32(&mut header, 40, seek_off as u32);
    put_u32(&mut header, 44, plan.seek_count as u32);
    put_u32(&mut header, 48, SEEK_INTERVAL);
    put_u32(&mut header, 52, strings_off as u32);
    put_u32(&mut header, 56, plan.strings.len() as u32);
    put_u32(&mut header, 60, stream_off as u32);
    put_u32(&mut header, 64, stream_halfwords as u32);
    // est_max_cycles / est_mean_cycles stay zero until `patch_cycles`.
    blob[..ALBUM_HEADER_BYTES].copy_from_slice(&header);

    blob
}

/// Fill in the two cycle-estimate fields of the finished blob.
pub fn patch_cycles(blob: &mut [u8], est_max: u32, est_mean: u32) {
    put_u32(blob, EST_MAX_OFFSET, est_max);
    put_u32(blob, EST_MEAN_OFFSET, est_mean);
}

fn put_u16(b: &mut [u8], off: usize, v: u16) {
    b[off..off + 2].copy_from_slice(&v.to_le_bytes());
}

fn put_u32(b: &mut [u8], off: usize, v: u32) {
    b[off..off + 4].copy_from_slice(&v.to_le_bytes());
}

fn push_u16(b: &mut Vec<u8>, v: u16) {
    b.extend_from_slice(&v.to_le_bytes());
}

fn push_u32(b: &mut Vec<u8>, v: u32) {
    b.extend_from_slice(&v.to_le_bytes());
}
