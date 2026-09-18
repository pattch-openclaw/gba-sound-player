//! # flac-lite
//!
//! A minimal FLAC **frame** decoder for the Game Boy Advance: `#![no_std]`,
//! zero dependencies, zero allocation on the decode path.
//!
//! **IMPLEMENTATION STATE (2026-09-18):** `bits::BitReader` is complete for
//! the decode path (PRs #25–#32 plus step 3a's `read_wasted_bits`; only
//! `crc8`/`crc16` remain, parked in Phase 2). `frame::FrameHeader::parse`
//! (step 2) and `subframe::SubframeType::parse` landed against real libFLAC
//! bytes in `tests/`; step 3a's leaves — the wasted-bits reader, the Rice
//! residual sign map (`residual::rice_unmap`), and the `PredictorState`
//! correction (warm-up is per-subframe, not cross-frame) — are in, and step
//! 3b's partition body (`residual::decode_rice_partition`: Rice + §9.2.7.1
//! escape-record) decodes.
//! `format.rs` carries the implemented sample-rate/channel helpers the parse
//! needs; the residual decode path is complete through step 3c (`rice_unmap`,
//! `decode_rice_partition`, `decode_residual`), and step 3d's integrators are
//! in: `subframe::integrate_fixed` (forward-difference cascade),
//! `subframe::integrate_lpc` (§9.2.6 dot product), and
//! `PredictorState::fill` (warm-up read). Step 3e's composition
//! `subframe::decode_subframe` landed against real libFLAC frames in
//! `tests/subframe_body_vectors.txt`: wasted read → §9.2.2 bps gate →
//! warm-up → CONSTANT/VERBATIM/residual dispatch → LPC fields → integrate
//! → `<< wasted` once at block exit (prediction runs on the stripped
//! scale). Step 3f part 1's `stereo::decorrelate` is in: the three
//! recombination transforms with the mid/side LSB parity recovery,
//! generation-witnessed; the side subframe's **bps + 1** storage width and
//! `0b1001`'s side-first orientation were measured on encoder bytes
//! (FLAC.md → step 3f part 1). Step 3f part 2 closes the frame layer:
//! `frame::decode_frame` composes the subframe loop (side slot at the
//! measured bps + 1), `decorrelate`, and the footer consume (`byte_align`
//! + CRC-16 consumed, verified never — Phase 2), witnessed on whole
//! libFLAC frame runs in `tests/frame_run_vectors.txt` — a layout suite
//! replaying the committed table through the primitives, then production
//! `decode_frame` on the same table, both bit-exact against `flac -d`
//! PCM (FLAC.md → step 3f part 2). Phase 1's decode path through
//! `decode_frame` is complete. What remains `todo!()`: `decoder` (the
//! per-frame loop above this layer) and `format::Manifest` —
//! the module layout, types, and signatures are the contract the
//! implementation fills in (FLAC.md → phased plan, step 4 onward).
//! Design rationale and the decision to write this at all live in
//! [`../../../FLAC.md`](../../../../FLAC.md).
//!
//! ## Why this crate exists
//!
//! Mature Rust FLAC decoders are built on `std::io` (`Read`/`Seek`/`BufRead`,
//! `std::io::Error`, `HashMap`, `sync::Arc`), none of which exist on
//! `thumbv4t-none-eabi` (Tier-3 bare-metal, `core` + `alloc` only). Rather than
//! port someone else's architecture, we exploit the fact that we control both
//! ends of the pipeline:
//!
//! - **Offline, owned encoder** → we pin a constrained FLAC profile, so the
//!   decoder only needs the subset actually produced (see `README.md`).
//! - **Memory-mapped, fixed-layout ROM** → no filesystem, so container parsing,
//!   runtime codec registries, and `Seek`-based trickery are replaced by an
//!   offline manifest with a frame-offset index. Seeking is an array lookup.
//!
//! The result: the decoder reads from a `&'static [u8]` through a cursor and
//! writes into caller-provided buffers. No `std`, no `alloc`, no traits from
//! `std::io`.
//!
//! ## Decode pipeline
//!
//! ```text
//! format::Manifest        GAFP header + stream info + frame-offset index
//!   └─ decoder::Decoder   per-frame loop, predictor warm-up across frames
//!        └─ frame         frame header → N subframes
//!             ├─ subframe CONSTANT | VERBATIM | FIXED(0..4) | LPC(<=32)
//!             │    └─ residual  partitioned Rice | Rice2 | escape
//!             └─ stereo   mid/side, left/side, right/side  (stereo only)
//! ```
//!
//! ## Build gates
//!
//! Both gates, from this directory. **The flags are load-bearing** — cargo
//! inherits the repo root's `.cargo/config.toml` by walking parent directories,
//! and both inherited values (`target = thumbv4t-none-eabi`,
//! `build-std = ["core","alloc"]`) break a plain `cargo test` here.
//!
//! ```sh
//! # GATE 1 — GBA target; the gate symphonia could never pass. Must stay clean.
//! cargo +nightly check --release --target thumbv4t-none-eabi -Zbuild-std=core,alloc
//!
//! # GATE 2 — host tests. `-Zbuild-std=` (empty) defeats the inherited
//! # core-recompile; `--target` defeats the inherited GBA default. Both needed.
//! cargo +nightly test -Zbuild-std= --target "$(rustc -vV | sed -n 's|host: ||p')"
//! ```
//!
//! `-Zbuild-std` on gate 1 is *not* "using std": it compiles `core`/`alloc` from
//! source because rustup ships no prebuilt `core` for this Tier-3 target. `std`
//! must never appear in a build-std list here. Full explanation (including the
//! array-merge behaviour that makes a local `build-std = []` ineffective) lives
//! in `README.md`.

// Bare-metal by construction. `core` only — not even `alloc` is required by the
// decode path (buffers are caller-provided; the manifest is borrowed).
#![no_std]
#![forbid(unsafe_code)]
#![warn(missing_docs)]
// SCAFFOLD-ONLY: silence dead-code/unused noise while bodies are `todo!()`.
// REMOVE this allow once implementations land.
#![allow(dead_code, unused_variables)]

pub mod bits;
pub mod decoder;
pub mod format;
pub mod frame;
pub mod residual;
pub mod stereo;
pub mod subframe;

pub use decoder::{DecodedFrame, Decoder, FrameBuffers};
pub use format::{Blocksize, ChannelConfig, EncodeProfile, Manifest, SampleRate};
pub use frame::FrameHeader;
pub use subframe::{PredictorState, SubframeType};

/// Flat, `core`-only decode error type.
///
/// Deliberately a plain enum: symphonia's `Error::IoError(std::io::Error)` is the
/// single variant that poisons its whole API on this target, and it is exactly
/// what this crate must never need.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// Ran out of bytes mid-read.
    EndOfStream,
    /// Frame sync code (`0b11111110` / `0b11111111`) not found where expected.
    FrameSync,
    /// Reserved/undefined value in a header field (block size, sample rate, etc.).
    InvalidField,
    /// Sample size not supported by this decoder (16-bit is the target).
    UnsupportedSampleSize,
    /// Predictor order exceeds what the caller's buffers can hold.
    UnsupportedPredictorOrder,
    /// Frame violates the constrained encode profile (only checked when the
    /// `strict-profile` feature is on, or when the manifest pins the profile).
    ProfileViolation,
    /// CRC-8 (frame header) mismatch — `crc-check` feature only.
    HeaderCrc,
    /// CRC-16 (footer) mismatch — `crc-check` feature only.
    FrameCrc,
    /// Manifest magic/version/length invalid.
    Manifest,
    /// Manifest declares a stream property this decoder cannot handle.
    UnsupportedStream,
}

/// Crate-local result alias; avoids importing `std::result` anywhere.
pub type Result<T> = core::result::Result<T, Error>;

/// Highest LPC predictor order FLAC defines (RFC 9639 §9.2.6).
pub const MAX_LPC_ORDER: usize = 32;

/// Highest FIXED predictor order FLAC defines.
pub const MAX_FIXED_ORDER: usize = 4;

/// Largest block size representable by the 4-bit block-size code (8 << 11).
pub const MAX_BLOCKSIZE: usize = 32768;
