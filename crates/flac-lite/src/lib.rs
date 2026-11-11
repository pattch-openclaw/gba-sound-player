//! # flac-lite
//!
//! A minimal FLAC **frame** decoder: `#![no_std]`, zero dependencies, zero
//! allocation on the decode path — portable pure Rust. The Game Boy Advance
//! (thumbv4t-none-eabi) is the founding platform, not the boundary: the
//! decode path imports nothing platform-specific, and the only unsafe
//! surfaces are `.iwram` placement **attributes** on immutable lookup tables —
//! `bits::LEADING_ONES` and, since split-ladder item 5 PR 3, the hybrid
//! kernel's three tables in [`rice_hybrid`] — all of them behind
//! `all(target_arch = "arm", feature = "iwram-pin")` and off by default (see
//! the `deny(unsafe_code)` note below), so host builds compile with zero
//! unsafe and non-GBA ARM targets link without a `.iwram` section.
//! The GAFP manifest and encode-profile material in `README.md` are
//! product-side container work — a manifest is *one* frame source, not the
//! decoder's contract.
//!
//! **Arithmetic width (item 6a-6, default):** `i32-integrator` is on by
//! default — 32-bit integration where it is provably bit-identical, with the
//! LPC dot product guarded per subframe ([`subframe::lpc_accumulator_fits_i32`])
//! and the i64 leg as the always-correct escape. [`ArithmeticWidth`] reports
//! which leg ran. `--no-default-features` is the audited i64 reference build.
//!
//! **IMPLEMENTATION STATE (2026-11-04):** `bits::BitReader` is complete for
//! the decode path (PRs #25–#32 plus step 3a's `read_wasted_bits`), and
//! `bits::crc8`/`bits::crc16` landed with Phase 2 step 5 (2026-11-03) —
//! header/footer *verification* is wired into `frame` behind the `crc-check`
//! feature; the default build consumes both CRCs unchecked as designed. The
//! split-proposal gate is `make flac-test-crc`. `frame::FrameHeader::parse`
//! (step 2) and `subframe::SubframeType::parse` landed against real libFLAC
//! bytes in `tests/`; step 3a's leaves — the wasted-bits reader, the Rice
//! residual sign map (`residual::rice_unmap`), and the `PredictorState`
//! correction (warm-up is per-subframe, not cross-frame) — are in, and step
//! 3b's partition body (`residual::decode_rice_partition_with`: Rice + §9.2.7.1
//! escape-record) decodes.
//! `format.rs` carries the implemented sample-rate/channel helpers the parse
//! needs; the residual decode path is complete through step 3c (`rice_unmap`,
//! `decode_rice_partition_with`, `decode_residual`), and step 3d's integrators are
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
//! `decode_frame` is complete. The **per-frame loop above the frame layer
//! landed 2026-11-04 as [`driver`]** — a narrow copy of this repo's proven
//! frame-loop driver (`examples/flac_spike/src/driver.rs`), i.e. the
//! split-proposal PR ladder's "`Decoder::decode_next` per-frame loop" rung.
//! It decodes one frame **at its index offset** through a fresh cursor, and
//! walks a whole index through a per-frame callback, over fully borrowed
//! inputs ([`driver::FrameSource`] — the E1 sample's `ClipView` field set).
//! Witness: `tests/frame_run_loop.rs` (this crate's gate, over the committed
//! libFLAC run table) plus a differential test in the spike's own witness
//! suite that runs **both copies** over the same embedded clip bytes, frame by
//! frame. The spike's driver deliberately stays where it is: it is the `ctrl`
//! arm of every recorded E measurement (FLAC.md).
//! Item 4's **other half — the stream layer — landed the same day as
//! [`stream`]**: the file level of an ordinary `.flac` file. It reads §6's
//! `fLaC` magic, walks §8.1's metadata-block chain (borrowed, streaming, zero
//! allocation, reserved types skipped by length, §5's forbidden type refused),
//! parses §8.2's streaminfo, and reports `frame_data_offset` — the byte the
//! frames start at, the number item 4 exists to produce. It is also the first
//! producer of `frame::StreamDefaults` ([`stream::StreamInfo::as_defaults`]),
//! which the 65 kHz goal depends on (every frame of a 65,536 Hz encode uses
//! sample-rate code `0b000`). Two supporting changes came with it:
//! `bits::read_utf8_coded` now **refuses non-minimal coded-number forms** — a
//! deliberate divergence from the reference decoder, measured rather than
//! assumed: `flac -d` accepts a real stream whose frame number is spelled in a
//! longer form once both CRCs are recomputed (the `mutation
//! number-form-overlong` rows of `tests/stream_vectors.txt`), while libFLAC
//! itself never writes one (the generator asserts that census per fixture), and
//! keeping the rule preserves the value→form-width relation the frame layer's
//! width bounds are derived from; and `bits::coded_number_octets`
//! makes a header's width *computable* instead of only observable; and
//! `frame::parse`'s `48..=72`-bit debug assertion — an encoder observation that
//! a real stream broke — is replaced by derived bounds
//! ([`frame::MIN_HEADER_BITS`], [`frame::MAX_ACCEPTED_HEADER_BITS`]), with 48-,
//! 80- and 120-bit headers witnessed end to end. Witness: `stream`'s unit tests
//! plus `tests/stream_metadata_layout.rs` over five real libFLAC fixtures in
//! `tests/fixtures/` and the generated table `tests/stream_vectors.txt`
//! (`scripts/gen_stream_vectors.sh`): a two-block container, a four-block
//! seektable+padding container, a real 80-bit frame header, a real 24-bit
//! stream (refused as `UnsupportedSampleSize`), and an unseekable encode that
//! declares no total sample count, no frame sizes and no MD5 — which is why
//! `stream::StreamInfo::exact_frame_count` is defined only for the
//! fixed-blocksize/known-total case. The metadata layer also confirms what the
//! paragraph above already assumed: there is no metadata-block CRC to own. RFC
//! 9639 §8.1 defines a metadata block as
//! a 4-byte header plus exactly `length` bytes, with no CRC trailer, and the
//! format's only CRCs are the frame header (§9.1.8) and frame footer (§9.3)
//! — both implemented above. What has landed since that sentence was written:
//! `format::Manifest`, `format::Blocksize::samples`, `EncodeProfile::validate`,
//! and the `decoder` facade — item 6b-1 (2026-11-10) made that facade a thin
//! shell over `scan` rather than over `driver`, exactly as predicted here, and it
//! added no third frame loop. Item 6b-2's first half (2026-11-11) made the
//! container real from the packer's side (`scripts/gafp_pack.py`) and gated it.
//! Still open in this area: `SEEKTABLE` interpretation and `Frames::anchored(n)`.
//! Item 4's **second half — the frame walk — landed as the `scan` module and its
//! feature of the same name**, and it
//! answers the two questions 4a left open: how a reader gets from one frame to
//! the next with no index in front of it, and how it knows the stream is over.
//! The format has two answers, and this module implements both and nothing else.
//! (1) **A frame ends itself**: one frame's own fields describe every bit up to
//! its end, so a walk that parses a header and walks the subframe bodies arrives
//! on the next frame's first byte by construction. `scan::Frames` keeps a *bit*
//! cursor for that reason, and the new `frame_bytes` / `frame_footer_gaps_bits`
//! rows are the measured reason a byte-level rule is not the same thing: one
//! stream's frame lengths are `117 116 116 116 116 116 115 116 116 51` — no
//! stride — and a frame's tail is **zero-padding (0–7 bits) plus the 16-bit
//! CRC**, recorded as a 16..23-bit gap (generation asserts the format's bound
//! rather than trusting it), with non-zero padding on 23 of these 35 frames. The
//! table then counts what a reader that treated the footer as exactly 16 bits
//! would be standing on: `footer_slip_positions_strict_headers 0` over those 23
//! bit positions, `footer_byte_truncation_strict_headers 0` over the 30 positions
//! a down-rounded byte cursor lands on — the mistake is refused, loudly, though
//! inside a frame that had already decoded. One of the 23 is sync-SHAPED
//! (`footer_slip_positions_sync_shape 1`): two bytes that look like a frame and
//! fields that are not, which is exactly the case the field leg of the filter
//! exists for. (I had written in this file that a constant-16-bit footer walk
//! "silently slips and the next header still parses"; it does neither, and those
//! rows are what replaced the claim.) A walk that skips the 16-bit CRC and
//! re-anchors **up** to the next byte boundary does land correctly, by arithmetic
//! (the footer ends on a byte boundary, so rounding up absorbs the padding) —
//! which is `frame`'s documented contract, and safe only because the body's bit
//! end was computed by walking the fields. That is the whole difference: `scan`
//! measures the frame's length from its fields, so `bit_position() / 8` after the
//! footer is a measurement rather than a rounding, and it reproduces
//! `frame_bytes` on every frame in both feature builds. Those rows are independent
//! ground truth: the generator walks every frame's subfields with its own
//! reference parser and cross-checks that walk against framing (and the tiling of
//! the whole frame region) before it will write the table. (2) **The end of
//! the bytes is the end of the stream** — §6's "last byte of the last frame, and
//! thus of the whole stream" is the only end every FLAC stream has, while
//! everything else the format offers (§8.2's `total_samples`, the fixed-blocksize
//! frame count 4a derives, §8.2's MD5, §8.3's seek points, §8.2's frame-size
//! fields) is a cross-check or does not exist for this stream; a walk reads to
//! the end of its region and then *checks* the walk against what the stream
//! declared, which is one code path for seekable and unseekable streams alike.
//! Scanning (`scan::next_frame_start`) exists only as **recovery**, never as a
//! frame finder — the measured record is 1223 sync-shaped candidates for 979 real
//! frames, and ~9.4× over-counting alone on noise-like material, so the candidate
//! rule is the two-byte shape plus the full RFC field validation plus the header
//! CRC-8 (performed by `frame::parse` under `crc-check`, and by `scan` itself
//! without it, so a scan result does not move with the feature), bounded per
//! position by the derived header bounds 4a introduced ([`frame::MAX_HEADER_BYTES`],
//! [`frame::MAX_ACCEPTED_HEADER_BITS`]). Where a walk has already lost its
//! boundary, `scan::Frames::resync` applies that search to the walk's own state
//! and re-anchors the numbering **on** the frame it lands on — the frame in
//! progress is abandoned, which is what "a dead frame is dead" ([`frame`]) means
//! for a player, and what the first version of that function got wrong.
//! Coded frame numbers are the walk's third
//! strict leg — anchored at 0 for a whole file, `Frames::anchored`
//! for a range whose position is known, and enforced **before any sample is
//! written**. Witness: `scan`'s unit tests, which claim only the walk's state
//! machine and caller contract over hand-cut regions, plus
//! `tests/scan_frame_walk_layout.rs` over the same five fixtures and the extended
//! `tests/stream_vectors.txt` (`frame_offsets`, `frame_bytes`, `frame_header_bits`,
//! `frame_blocksizes`, `frame_footer_gaps_bits`, `footer_padding_frames` and the
//! four `footer_*_positions_*` counts, `frame_numbers`,
//! `frame_sample_size_codes`, `frame_region_len`,
//! `frame_exact_count_from_declarations`, the three `scan_frame_starts_*` counts,
//! and the reference-decoder PCM rows `pcm_md5` / `pcm_fnv1a64` for the 16-bit
//! fixtures).
//! **The depth boundary applies to locating frames exactly as much as to decoding
//! them**, and that is stated rather than hidden: `s4` (a valid 24-bit encode)
//! is refused by `stream::parse` at its entry, its first frame is refused by a
//! walk handed its region, and the reason its frame starts are not found is the
//! same profile check living inside `frame::parse`. The table is the counter-
//! evidence that this is a capability limit and not a framing one: it describes
//! those ten frames completely — lengths, header widths, sample-size codes
//! (`0b110`: the frames themselves state 24, so no caller-supplied default can
//! argue with the refusal), and the generator's own strict filter locates every
//! one of them. A build that accepted 24-bit samples would need no new framing
//! layer; `tests/scan_frame_walk_layout.rs` is where a reader finds out that the
//! two questions are separate.
//! **The whole module sits behind the `scan` feature, which is off by default**
//! (`make flac-test-scan` runs its suite; `make flac-portable-check` is the only
//! gate that compiles the walk for a non-thumbv4t target). Two reasons, both
//! measured rather than assumed. (1) A GBA ROM should not carry the walk until
//! item 6 rewires the facade onto it: with `scan` off, the E instrument's sections
//! are byte-identical to the 4a build; with it on, `.text` is **+3,120 B** while
//! **no `scan` symbol appears in the image at all** — the walk links for nothing,
//! but its presence in the rlib changes how LLVM inlines the decode path it shares
//! a codegen unit with (`crates/flac-lite/Cargo.toml` and
//! [`../../../FLAC.md`](../../../../FLAC.md) carry the numbers). So the gate defers
//! that cost to the change that will re-anchor the instrument anyway; it does not
//! remove it. (2) Nothing in `scan` is a *profile* layer, so nothing in it belongs
//! in a build that has no use for a walk: it neither defines nor relaxes a profile,
//! and every frame-shape refusal it reports arrives through `frame::parse` — the
//! `s4` refusal above is that check, not a scan one. The walk adds no block-size or
//! sample-rate claim of its own to enforce, which is why `s3`'s 3000-sample blocks
//! (a block size that is not a power of two, and a declared maximum larger than the
//! 1801 samples its last frame actually carries) walk and decode bit-exactly
//! instead of being refused as off-profile. A packer profile is asserted where a
//! manifest pins one ([`format::Manifest`], [`format::EncodeProfile`]) — item 6's
//! seam, which is also where the profile-anchored measurements get re-pinned.
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
//! frame source          borrowed decode inputs. There are two, and which one a
//!                       build links is a feature decision:
//!                         · `format::Manifest` + `scan::Frames` — the product
//!                           path, behind `scan` (item 6b-1). No index in front
//!                           of the frames: each frame ends itself.
//!                         · `driver::FrameSource` (frame index + region +
//!                           stream defaults) — behind `instrument`, the
//!                           measurement path, kept because every E-series cycle
//!                           anchor is attributable to that file's text.
//!                       Two *sources for the same loop*, not two loops.
//!   └─ driver           per-frame loop: index offset → fresh cursor → frame.
//!                       `decoder::Decoder` is the manifest-shaped facade — it
//!                       landed on `scan` (6b-1), not here, and there is no
//!                       third loop anywhere in the crate.
//!        └─ frame       frame header → N subframes
//!             ├─ subframe CONSTANT | VERBATIM | FIXED(0..4) | LPC(<=32)
//!             │    └─ residual  partitioned Rice | Rice2 | escape
//!             └─ stereo   mid/side, left/side, right/side  (stereo only)
//!
//! scan (item 4b — the second frame source, with no index; it decodes through
//!   the same layers below and reads the file level through stream). Behind the
//!   `scan` feature, OFF by default *in this crate*: the walk's cost is measured
//!   (+3,120 B of `.text`), so the images that want it opt in and the ones that
//!   do not are not made to carry it.
//!   └─ scan             frame walk: one frame's own fields end it (bit cursor,
//!                       `frame_bytes` witnessed per frame against the generated
//!                       table), and the end of the region is the end of the
//!                       stream, cross-checked against what it declared.
//!                       `scan::next_frame_start` is recovery only.
//!
//! stream (not in the decode path — it produces a frame source's inputs)
//!   └─ stream           fLaC magic → §8.1 metadata-block walk → §8.2
//!                       streaminfo → `frame_data_offset` +
//!                       `frame::StreamDefaults`. The other half of item 4: an
//!                       ordinary `.flac` file becomes reachable.
//! ```
//!
//! There is no cross-frame predictor state to carry: every subframe carries
//! its own warm-up samples (measured 64/64, 2026-09-10), so `driver`'s
//! `[PredictorState; 2]` is reused scratch, and any frame is independently
//! decodable from its index offset — which is what makes a seek an array
//! lookup instead of a reconstruction.
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
// `deny`, not `forbid`, since 2026-10-22: named exceptions — the `.iwram`
// placement attributes on the decode path's immutable run-length tables.
// Four items, one kind: an unsafe *attribute* on an immutable table, no
// unsafe code anywhere. `bits::LEADING_ONES` is the original, which Sam
// ratified when the rate-play producer-wall measurement put the unpinned
// landing back on the table (FLAC.md, E2b pinning); `rice_hybrid`'s
// `CTZ8` / `REV8` / `BRUIJN_INDEX` joined them on 2026-11-05 with
// split-ladder item 5 PR 3, under the same `iwram-pin` gate and for the same
// reason (the hot tables the E10-on-target proposal budgeted for the hybrid —
// see that module's notes on the pin and on what the tables cost). Those three
// are compiled only when `rice-hybrid` is on, so a default build carries the
// same **one** item it carried before PR 3; the count that grows is the number
// of *gated* placement attributes, not the number any shipped image pays.
// Everything else still fails the build.
#![deny(unsafe_code)]
#![warn(missing_docs)]
// Still load-bearing, and now named rather than assumed: probed 2026-11-11 by
// deleting this attribute and building every gated feature combination, which
// added five warnings, each one a cfg-conditional name whose other arm is what a
// given build compiles — `bits::peek_at`'s `end` binding, the `crc8_bit`
// position in the frame-header parse (read only by the `crc-check` arm), the
// `order_u8` pattern binding in `subframe` (used only by the width arm that is
// compiled out), `subframe::integrate_fixed` (live under `--no-default-features`
// and under `cfg(test)`, dead in the default i32-integrator build), and a
// test-local `pos` in `bits`. Note the scope this attribute does *not* have: an
// integration test is its own crate, so a crate-level allow never covered
// `tests/`, and one unused variable there (`lens`, in
// `tests/scan_frame_walk_layout.rs`) already shows up in three arms of
// `make flac-gates` today. None of the six is a design problem. What *is* a cost
// is that this attribute is crate-wide, so it silences genuine dead code as
// well: C1's closure pass gives each item its own `cfg`, fixes that test
// variable, and deletes this line. That pass edits source, so it owes its own
// control build and section table; 6b-2's packer work touched no Rust in the
// shipped path, so this stopped at measuring rather than fixing.
#![allow(dead_code, unused_variables)]

pub mod bits;
// [6b-1] The product facade is a shell over the walk (`scan`), so it exists
// exactly where the walk does. Its module docs carry the consequence: the cost of
// making it available to a ROM is the walk's measured +3,120 B of `.text`, which
// is why it is a feature rather than unconditional.
#[cfg(feature = "scan")]
pub mod decoder;
// [6b-1] The instrument's per-frame loop. Not part of the public contract:
// `decoder` (above) is the product path, and this is the measurement harness's
// index-driven loop, kept compiling precisely so the E-series' cycle numbers
// stay attributable to unchanged source. `#[doc(hidden)]` + an off-by-default
// feature is the demotion; deletion waits for the instrument's retirement.
#[cfg(feature = "instrument")]
#[doc(hidden)]
pub mod driver;
pub mod format;
pub mod frame;
pub mod residual;
// The portable hybrid Rice kernel — split-ladder item 5 PR 3's second
// `residual::RiceKernel` implementor, behind the `rice-hybrid` feature — off
// by default. It is *not* the default the decode path uses, and the module says
// why — and since PR 4 (2026-11-06) "off by default" is a *measured* state, not a
// deferral: the on-target cycle reading the pre-registered flip rule was gated on
// came back slower (+42.6 / +42.7 cycles per decoded sample, +9.71% / +2.64% mean
// per frame, 314/314 frames), so the rule's "does not flip" arm is the one that
// applies and this module stays the portable alternative item 6 lifts and rewrites
// in asm. What the feature decides is only whether a build *carries* it: a measured
// +2,984 B on the calibration instrument with the feature on and **no hybrid
// symbol in the image at all** — the same second-implementor effect 4b recorded
// for the walk, so the reason for the gate and its measured cost live in
// `Cargo.toml`, and the module's own notes carry the 544 B table budget and what
// those tables cost when the kernel is live (`iwram-pin`).
#[cfg(feature = "rice-hybrid")]
pub mod rice_hybrid;
// The frame walk (split-ladder item 4b), behind the `scan` feature — off by
// default. Nothing in this crate links it yet; the reason and the measured
// code-size cost of turning it on are in `Cargo.toml`, and the module docs name
// what its suite is (`make flac-test-scan`) and what proves it portable
// (`make flac-portable-check`).
#[cfg(feature = "scan")]
pub mod scan;
pub mod stereo;
pub mod stream;
pub mod subframe;

#[cfg(feature = "scan")]
pub use decoder::{DecodedFrame, Decoder, FrameBuffers};
pub use format::{Blocksize, ChannelConfig, EncodeProfile, Manifest, SampleRate};
pub use frame::FrameHeader;
pub use subframe::{ArithmeticWidth, PredictorState, SubframeType};

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
    /// The bytes disagree with the declarations in front of them: a blob or a
    /// walk was told a frame count or a sample count, and the frames it holds
    /// are not that number. Both sides are carried because the numbers *are* the
    /// diagnosis — `declared 157 / actual 156` says the table lost an entry,
    /// while the reverse says it gained one.
    StreamMismatch {
        /// The count the header pinned.
        declared: u64,
        /// The count the bytes actually produced.
        actual: u64,
    },
    /// A frame's coded number is not the one a walk expected — RFC 9639 §9.1.5
    /// numbering was not contiguous. Carries both so a resync decision can be
    /// made from the numbers rather than re-derived.
    FrameNumberMismatch {
        /// The number the walk expected next.
        expected: u64,
        /// The number the frame actually carries.
        found: u64,
    },
    /// Manifest declares a stream property this decoder cannot handle.
    UnsupportedStream,
    /// Not a FLAC stream: the `fLaC` magic (`stream::MAGIC`) is absent or
    /// truncated ([`stream`] — RFC 9639 §6). The near miss worth naming is a
    /// WAV header, which is what a `.wav` file actually starts with.
    StreamMagic,
    /// The metadata-block chain ends in the middle of a block: no block carried
    /// §8.1's last-metadata-block flag, or a block's declared length runs past
    /// the end of the bytes. The reference decoder reports
    /// `FLAC__STREAM_DECODER_END_OF_STREAM` / `…_READ_METADATA` for these two
    /// shapes respectively (measured, 2026-11-04).
    MetadataTruncated,
    /// No streaminfo block, or one not in first position. §8.2 requires it as
    /// the first metadata block; libFLAC's decoder only warns and carries on
    /// (it can take stream properties from frame headers instead), which this
    /// crate cannot — `frame::StreamDefaults` has no other producer
    /// ([`stream::StreamInfo::as_defaults`]).
    StreamInfoMissing,
    /// More than one streaminfo block: §8.2 ("There MUST be no more than one
    /// streaminfo metadata block per FLAC stream"), and the reference decoder's
    /// own words are "more than one STREAMINFO found".
    DuplicateStreamInfo,
}

/// Crate-local result alias; avoids importing `std::result` anywhere.
pub type Result<T> = core::result::Result<T, Error>;

/// Highest LPC predictor order FLAC defines (RFC 9639 §9.2.6).
pub const MAX_LPC_ORDER: usize = 32;

/// Highest FIXED predictor order FLAC defines.
pub const MAX_FIXED_ORDER: usize = 4;

/// Largest block size representable by the 4-bit block-size code (8 << 11).
pub const MAX_BLOCKSIZE: usize = 32768;
