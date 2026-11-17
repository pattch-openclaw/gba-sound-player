//! The seam: `PcmSource` (ALBUM-PLAYER.md §4.2) and the album-sample types that cross it.
//!
//! Everything above `src/backend/` speaks this trait and nothing else. Three rules from §4.2 shape
//! it, and they are the reason the trait is this narrow:
//!
//! 1. **Position is in samples on the album timeline, always.** The manifest defines that timeline
//!    (§4.3), and every consumer — the session clock, a bar, a log ledger — reads the same number.
//!    Frames, chunks, periods and bit positions are the producer's business and never cross here.
//! 2. **The app never branches on which codec is loaded.** Nothing in this module, or above it,
//!    names a codec. If an operation is unavailable it is a [`Capability`], and a screen tests the
//!    capability rather than a codec name.
//! 3. **`producer_frame()` is part of the API because the budget is part of the product** (§10): the
//!    ROM's own accounting wants the number it is running on, not the number in a document.
//!
//! No allocation, no `unsafe`, no `agb` here — the same module compiles for the host suite and for
//! the GBA image, which is what makes a host claim about the seam mean something about the ROM.

use crate::meta::Refusal;

/// Metadata for one track, in album samples.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TrackMeta {
    /// Album position of the track's first sample.
    pub start_sample: u32,
    /// Length of the track, in samples at [`PcmSource::rate_hz`].
    pub n_samples: u32,
    /// Title as the manifest carries it (§5.5's ASCII fold, already applied at bake time).
    pub title: &'static str,
    /// Artist as the manifest carries it (`""` when unknown).
    pub artist: &'static str,
}

/// Album-level metadata, in album samples.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AlbumMeta {
    /// Album title, ASCII.
    pub title: &'static str,
    /// Album artist, ASCII (`""` when unknown).
    pub artist: &'static str,
    /// Album year (`0` when the config carries none).
    pub year: u16,
    /// Total album length, in samples at [`PcmSource::rate_hz`].
    pub n_samples: u32,
    /// Cover art, when the build carries any. Phase 1 carries none — the `ART1` blob and its
    /// decode are Phase 3's (§8), so this is `None` for every Phase-1 arm rather than a
    /// zero-length placeholder that a screen might try to draw.
    pub art: Option<&'static [u8]>,
}

/// A capability some backend may not have. A missing capability is a *named* absence, never a
/// codec name — §4.2 rule 2.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Capability {
    /// Seek inside a track ([`PcmSource::start_sample`]). Deferred past v1 (Q5): the UI affordance
    /// is the work, and granularity is whatever the linked backend offers.
    Seek,
}

impl core::fmt::Display for Capability {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match *self {
            Capability::Seek => f.write_str("seek within a track (Q5)"),
        }
    }
}

/// Everything that can go wrong at or below the seam.
///
/// A refusal carries the field or the rule it failed, so a boot log names one thing rather than
/// reporting a plausible-sounding partial run (R5). Nothing here defaults or clamps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceError {
    /// The backend has no implementation for this capability.
    Unsupported(Capability),
    /// The requested track index is outside the album.
    NoSuchTrack {
        /// The index the caller asked for.
        index: usize,
        /// Tracks the album actually has.
        n_tracks: usize,
    },
    /// The `ALBM1` manifest was refused; the reader's named rule travels with it.
    Manifest(Refusal),
    /// A backend container was refused (a bad `GAFP` header, a stream outside the profile, …).
    Asset(&'static str),
    /// A generated pin and the manifest disagree. Never resolved by picking one (§4.3 rule 1).
    PinMismatch {
        /// Track index the disagreement is on.
        track: usize,
        /// The field that disagreed, e.g. `"n_samples"`.
        field: &'static str,
    },
    /// `out` cannot hold one whole interleaved frame.
    BufferTooSmall {
        /// Samples per channel the caller's slice can hold.
        have: usize,
        /// Samples per channel the next frame needs.
        need: usize,
    },
}

impl core::fmt::Display for SourceError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match *self {
            SourceError::Unsupported(capability) => {
                write!(f, "this backend has no {capability}")
            }
            SourceError::NoSuchTrack { index, n_tracks } => {
                write!(f, "track {index}, the album has {n_tracks}")
            }
            SourceError::Manifest(refusal) => write!(f, "manifest refused: {refusal}"),
            SourceError::Asset(what) => write!(f, "asset refused: {what}"),
            SourceError::PinMismatch { track, field } => {
                write!(
                    f,
                    "track {track}: the generated pin and the manifest disagree on {field}"
                )
            }
            SourceError::BufferTooSmall { have, need } => write!(
                f,
                "the output slice holds {have} samples per channel, a frame needs {need}"
            ),
        }
    }
}

/// What one frame costs the producer, in **permille of one period** — integer only.
///
/// Permille because a percentage would lose the third digit that makes the FLAC row comparable
/// across measurements, and integer because the GBA has no FPU and §6's arithmetic rule applies to
/// every number the ROM carries: no float, no divide.
///
/// A period is `geometry::PERIOD_CYCLES` (573,440 cycles at 16,384 Hz), so 237 permille is 23.7%
/// of the period. The model is *declared* by the backend alongside the samples it produces, not
/// measured by the app.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameCostModel {
    /// Decode alone, permille of one period.
    pub decode_permille: u16,
    /// Total work per frame (decode + convert + push + feeder), permille of one period.
    pub work_permille: u16,
    /// The worst single frame, permille of one period.
    pub worst_frame_permille: u16,
}

impl FrameCostModel {
    /// The FLAC arm's row: **real audio through the master path**, C2's hardware run.
    ///
    /// Source: `flac-docs/measurements.md` → "The numbers of record", the row of record
    /// (`flac-docs/measurements.md:66`) — work/frame 32.208% → **322**, worst frame 35.831% →
    /// **358**, decode 23.656% → **237**.
    ///
    /// **What these numbers are not.** §10.2b's warning, stated in the same words the app's log
    /// owes: they are **asset-level means over a real master, and one whole-asset worst frame — not
    /// a per-frame census.** The asset was the first 9.9 s of `bioluminescent.wav` (80 frames,
    /// 162,304 samples, census `constant ×4, fixed1 ×102, fixed2 ×53, fixed3 ×1`), so a track
    /// whose census differs can cost more than 358 permille on a single frame. Q16 is the ask that
    /// turns this row into per-order measurements; until it lands, an album is budgeted against
    /// this aggregate and the label travels with it (§10.2b, Q16).
    pub const FLAC: FrameCostModel = FrameCostModel {
        decode_permille: 237,
        work_permille: 322,
        worst_frame_permille: 358,
    };

    /// A backend that synthesises its samples and has nothing to measure: all three fields are 0
    /// because it is not a decoder, not because it is free.
    pub const NOT_A_DECODER: FrameCostModel = FrameCostModel {
        decode_permille: 0,
        work_permille: 0,
        worst_frame_permille: 0,
    };
}

/// The only audio interface the application layer sees (§4.2, verbatim).
///
/// Implemented by `src/backend/`'s selected arm, which is the only place a codec crate is named.
/// The trait is deliberately object-safe so the host suite can drive any arm through
/// `&mut dyn PcmSource`, which is the compile-time form of §4.2 rule 2.
pub trait PcmSource {
    /// Number of tracks in the album.
    fn n_tracks(&self) -> usize;

    /// Metadata for track `i`, or `None` when `i` is outside the album.
    fn track(&self, i: usize) -> Option<TrackMeta>;

    /// Album-level metadata.
    fn album(&self) -> AlbumMeta;

    /// Playback rate for the whole album, Hz (16,384 for FLAC, 32,768 for ULI2).
    fn rate_hz(&self) -> u32;

    /// Select track `i` and position at its first sample, ready to [`pull_frame`](Self::pull_frame).
    ///
    /// This is the skip/preroll seam (§4.4); a short, bounded delay is accepted product behaviour.
    fn start_track(&mut self, i: usize) -> Result<(), SourceError>;

    /// Position at album sample `s`.
    ///
    /// **A2's follow-up, and deferred past v1 (Q5):** the UI affordance is the work, and the
    /// granularity is whatever the linked backend offers, so no Phase-1 backend implements it. A
    /// backend that does not support it returns
    /// [`SourceError::Unsupported`] with [`Capability::Seek`] — it must not emulate one, because a
    /// seek that silently replays from the track start is a wrong position the session clock would
    /// believe.
    fn start_sample(&mut self, s: u32) -> Result<(), SourceError>;

    /// Decode at most one frame into `out`, interleaved int16 stereo (left, right, left, …), and
    /// return the number of **samples per channel** written.
    ///
    /// `Ok(0)` means end of stream. `out` must hold at least two int16s per sample of the next
    /// frame; a shorter slice is [`SourceError::BufferTooSmall`], never a partial frame.
    fn pull_frame(&mut self, out: &mut [i16]) -> Result<usize, SourceError>;

    /// What a frame costs this backend, for the ROM's budget audit (§10).
    fn producer_frame(&self) -> FrameCostModel;

    /// Cold-start the decoder after a pause or a skip: drop every bit of decoded state so the next
    /// pull re-reads from the current track's first sample.
    fn reset_decoder_state(&mut self);
}
