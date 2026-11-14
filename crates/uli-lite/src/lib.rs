#![no_std]
#![deny(unsafe_code)]
#![warn(missing_debug_implementations)]

//! `uli-lite` — a minimal, `no_std`, zero-dependency, zero-allocation decoder
//! for the ULI2 bitstream and the ULA2 album container.
//!
//! ULI2 is the fixed-point MDCT codec shipped by the sibling `gba-audio`
//! project. This crate is the portable core of the Rust port: it contains no
//! GBA registers, no `agb` dependency, no allocation, no float, and no
//! platform-bound assembly. The GBA overlay (`uli-lite-gba`) owns the ARM
//! kernels and the `.iwram`/`.ewram` placement.
//!
//! The public surface is deliberately narrow:
//!
//! * [`Album`] parses and validates a ULA2 blob without copying it.
//! * [`Decoder`] borrows an [`Album`] and caller-owned [`DecoderState`], and
//!   decodes frames into caller-owned output slices.
//!
//! The normative arithmetic is the integer C reference in
//! `gba-audio/src/uli/uli_dec_ref.c`. The core is tested bit-exactly against
//! that reference; see `tests/`.
//!
//! Licensing of the ULI2 format/reference is still being resolved; the crate is
//! `publish = false` until that is settled. See `ULI2.md`.

pub mod album;
pub mod bitstream;
pub mod format;
pub mod frame;
// The synthesis kernels are an implementation seam, not part of the product
// API: the GBA overlay replaces them and the on-target benchmark times them.
// `kernels` exposes them for exactly those two consumers; the default build
// keeps the narrow facade (the overlay is the only expected feature user).
#[cfg(feature = "kernels")]
pub mod imdct;
#[cfg(not(feature = "kernels"))]
mod imdct;
pub mod tables;

// The synthesis tables (PRE/POST/TW/WIN/PERM) are needed by the overlay that
// reimplements the kernels with unchecked addressing. Exposed only under
// `kernels`, and hidden from docs: they are constants of the format's
// reference implementation, not API.
#[cfg(feature = "kernels")]
#[doc(hidden)]
pub mod tables_data;
#[cfg(not(feature = "kernels"))]
mod tables_data;

pub use album::{Album, AlbumHeader, SeekEntry, TrackEntry};
pub use frame::{Decoder, DecoderState, FrameInfo, Ops};
pub use tables::CodeTables;

/// Errors returned by the decoder and container parser.
///
/// The variants are coarse on purpose: the decoder is a real-time component
/// and a product ROM generally treats any malformed input the same way (stop
/// or skip the frame). Host tests can still distinguish parse stages.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// The ULA2 header, a section bound, or a track/seek entry is invalid.
    BadAlbum,
    /// The tables blob is truncated or has an unsupported shape.
    BadTables,
    /// The bitstream is truncated, malformed, or runs out of padding.
    BadBitstream,
    /// A frame violates a format invariant (for example an out-of-range
    /// quantizer index or an impossible coefficient run).
    BadFrame,
    /// An output slice is shorter than the frame's 512 samples.
    OutputTooSmall,
    /// The decoder has reached the end of the album.
    EndOfStream,
    /// A track or seek index is outside the album.
    BadSeek,
}
