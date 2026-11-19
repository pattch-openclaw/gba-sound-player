//! PCM and track-list input: the encoder's contract boundary (E-R2).
//!
//! `album.f32` is float32 **interleaved stereo at PCM16 scale** (samples ×
//! 32768, joined gaplessly at 32,768 Hz) and `tracks.tsv` is the reference's own
//! `n start len title artist` table in 32,768 Hz samples. S1 does not read the
//! sample values at all — its frames are content-independent — but the *length*
//! is the frame count, so it is settled here rather than discovered while
//! writing the container.
//!
//! **The length belongs to the track table.** `Pcm::open` takes
//! `n_samples = last.start + last.len`, exactly as `uli_enc.c:read_tracks`
//! computes `NS`, and then requires the file to be *precisely* that long; a
//! disagreement is a refusal citing both counts, never a truncated album and
//! never a tail invented from the file size. Before this ruling the encoder read
//! the length off the file and only required the tracks to fit inside it, so the
//! two encoders given the same pair could disagree on `NS` — hence `NF`, hence
//! the seek table, hence every byte after the header — for reasons that have
//! nothing to do with the codec. That is the `NS` ruling in `PLAN-OF-RECORD.md` → "S1 review
//! — the fix plan".
//!
//! Two things the reference does *not* require, and neither do we: contiguity
//! between tracks (E-R2's "gaplessly" describes what `prep.py` emits, not a
//! constraint on input — a gap is silence that still costs frames), and a first
//! track starting at zero. Overlap and descending order *are* refused, because
//! they make `NS` ambiguous: with ascending non-overlapping rows the last row is
//! the furthest end, which is what the reference's `TR[NT-1]` assumes without
//! checking.

use std::path::Path;

use crate::Error;
use uli_lite::format::MAX_TRACKS;

/// One track as the container will carry it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrackInput {
    pub start_sample: u32,
    pub n_samples: u32,
    pub title: String,
    pub artist: String,
}

/// A validated album length and track table.
#[derive(Clone, Debug)]
pub struct Pcm {
    /// Interleaved stereo frames: **the track table's end**, `last.start +
    /// last.len`, which the file must match byte for byte (see the module doc).
    pub n_samples: u32,
    pub tracks: Vec<TrackInput>,
}

impl Pcm {
    /// Read `tracks.tsv`, take the album length from it, and confirm
    /// `album.f32` is exactly that long.
    pub fn open(album: &Path, tracks: &Path) -> Result<Self, Error> {
        // The file's own size is checked only for well-formedness here: it is
        // evidence about the track table, not the other way round.
        let size = std::fs::metadata(album)
            .map_err(|e| Error::Pcm(format!("{}: {e}", album.display())))?
            .len();
        if size % 8 != 0 {
            return Err(Error::Pcm(format!(
                "{}: {size} bytes is not a multiple of 8 (interleaved stereo f32)",
                album.display()
            )));
        }
        let file_samples = size / 8;

        let text = std::fs::read_to_string(tracks)
            .map_err(|e| Error::Tracks(format!("{}: {e}", tracks.display())))?;

        let mut parsed = Vec::new();
        for (lineno, raw) in text.lines().enumerate() {
            let lineno = lineno + 1;
            let line = raw.strip_suffix('\r').unwrap_or(raw);
            let trimmed = line.trim_start();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            let fields: Vec<&str> = line.split('\t').collect();
            let [n, start, len, title, artist] = fields.as_slice() else {
                return Err(Error::Tracks(format!(
                    "{}:{lineno}: expected 5 tab-separated fields (n start len title artist), \
                     found {}",
                    tracks.display(),
                    fields.len()
                )));
            };
            let n = parse_field(tracks, lineno, "n", n)?;
            let start = parse_field(tracks, lineno, "start", start)?;
            let len = parse_field(tracks, lineno, "len", len)?;

            if parsed.len() >= MAX_TRACKS {
                return Err(Error::Tracks(format!(
                    "{}:{lineno}: more than {MAX_TRACKS} tracks (the format's limit)",
                    tracks.display()
                )));
            }
            if n != parsed.len() as u32 + 1 {
                return Err(Error::Tracks(format!(
                    "{}:{lineno}: track number {n} is not {} (the rows must be numbered 1.. \
                     in order)",
                    tracks.display(),
                    parsed.len() + 1
                )));
            }
            // The sum is the reference's `NS` candidate, so it must not wrap; it
            // is not compared against a length here, because the length *is* the
            // last row's sum (see the module doc).
            start.checked_add(len).ok_or_else(|| {
                Error::Tracks(format!(
                    "{}:{lineno}: start {start} + len {len} overflows",
                    tracks.display()
                ))
            })?;
            if let Some(prev) = parsed.last() {
                let prev: &TrackInput = prev;
                let prev_end = prev.start_sample + prev.n_samples;
                if start < prev_end {
                    return Err(Error::Tracks(format!(
                        "{}:{lineno}: track starts at {start}, before the previous track's \
                         end ({prev_end}); the reference's tracks are non-overlapping and \
                         ascending",
                        tracks.display()
                    )));
                }
            }
            // The container stores both tags NUL-terminated inside one region; a
            // NUL inside a tag would silently truncate it.
            for (what, value) in [("title", title), ("artist", artist)] {
                if value.as_bytes().contains(&0) {
                    return Err(Error::Tracks(format!(
                        "{}:{lineno}: {what} contains a NUL byte",
                        tracks.display()
                    )));
                }
            }

            parsed.push(TrackInput {
                start_sample: start,
                n_samples: len,
                title: (*title).to_string(),
                artist: (*artist).to_string(),
            });
        }

        if parsed.is_empty() {
            return Err(Error::Tracks(format!(
                "{}: no tracks (the format requires 1..={MAX_TRACKS})",
                tracks.display()
            )));
        }

        // The reference's `NS = TR[NT-1].start + TR[NT-1].len`. Ascending and
        // non-overlapping were enforced row by row, so this is also the furthest
        // end any track claims.
        let last = parsed
            .last()
            .expect("the track list is not empty, checked just above");
        let n_samples = u64::from(last.start_sample) + u64::from(last.n_samples);
        if n_samples != file_samples {
            return Err(Error::Pcm(format!(
                "the album length is the track table's, and {} ends at sample {n_samples} \
                 (track {} starts at {} and runs {}) while {} holds {file_samples} samples: \
                 {} must be exactly {n_samples} samples ({} bytes). The encoder neither drops \
                 a tail it was not told about nor invents one.",
                tracks.display(),
                parsed.len(),
                last.start_sample,
                last.n_samples,
                album.display(),
                album.display(),
                n_samples * 8,
            )));
        }
        let n_samples = u32::try_from(n_samples).map_err(|_| {
            Error::Pcm(format!(
                "{n_samples} samples exceeds the format's u32 sample count"
            ))
        })?;

        Ok(Self {
            n_samples,
            tracks: parsed,
        })
    }
}

fn parse_field(tracks: &Path, lineno: usize, what: &str, text: &str) -> Result<u32, Error> {
    text.trim().parse().map_err(|_| {
        Error::Tracks(format!(
            "{}:{lineno}: {what} {text:?} is not a non-negative integer",
            tracks.display()
        ))
    })
}
