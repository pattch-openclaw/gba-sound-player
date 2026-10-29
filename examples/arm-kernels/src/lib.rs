//! # arm-kernels — isolated ARM-mode kernels for E10 groundwork
//!
//! E10 ("ARM-mode hand-written codeword loop", FLAC.md) is a measurement
//! project gated on the E-probe plan. This crate is deliberately **not**
//! part of that: each kernel here is a pure word-in/word-out function,
//! unit-testable in isolation — no decode-path integration, no cycle
//! measurement, no ROM timing. Integration and measurement stay gated on
//! the E10 probe plan (FLAC.md "E10 algorithm plan of action 2026-10-14").
//!
//! ## Dual-witness testing strategy
//!
//! Every kernel carries two independent witnesses against **one committed
//! golden table** (`vectors::VECTORS` — the repo's shared-consumer rule; a
//! second ROM-side transcription of the expectation would be the
//! hand-packed-vector failure):
//!
//! 1. **Host twin** (`make kernel-test`): the same algorithm as the
//!    assembly, written in plain Rust, judged against a naive bit-walk
//!    oracle plus the committed table. The oracle is independent of both
//!    `bit_length` and the De Bruijn identity, so a shared misreading
//!    cannot make twin and table agree.
//! 2. **On-target sweep** (kernel-check ROM on mGBA): the ARM assembly is
//!    swept over the *same committed table* on real ARM semantics. The host
//!    cannot execute ARM code (this machine is aarch64; QEMU here has no
//!    arm7tdmi CPU), so the emulator is the execution oracle — the mGBA
//!    ARM7TDMI, exactly the core the shipping ROMs are measured on.
//!
//! A kernel is "working" when both witnesses are green on the same table.
//! The assembly's ARM-mode-ness is witnessed offline from the built ELF
//! (`objdump -d`: the kernel body is 4-byte ARM encodings, not a Thumb
//! transcription) — a passing sweep alone cannot tell ARM from Thumb, since
//! a Thumb build of the same algorithm would also pass.
//!
//! ## Kernels
//!
//! - [`ctz32`] — **C1, De Bruijn multiply-index count** (the CLZ-as-a-MUL
//!   shape): the hybrid's flat 8 ≤ q ≤ 31 tier in the plan of action.
//!   Converts the absent ARMv4T `CLZ` into the documented single-cycle
//!   hardware `MUL`: isolate the lowest set bit, multiply by the De Bruijn
//!   constant, index a 32-entry table. Answers `ctz(w)` for any `w`, with
//!   the sentinel `32` for `w == 0` ("run past the window" in the probe's
//!   bit-reversed-window shape).
//! - [`rice_unmap`] — **PR A, the folded-residual sign fold** (§9.2.7.2):
//!   the sample-step's independent leaf — four straight-line instructions
//!   (`AND`/`RSB`/`LSR`/`EOR`, no table, no branch, no flag writes) for the
//!   measured identity `(n >> 1) ^ (-(n & 1))`. The host witness additionally
//!   anchors the sign convention against the encoder-side fold, so a
//!   step-3a-style sign misreading cannot survive the composition.
//! - [`word_skip`] — **PR B, C2 word-wide continuation skip**: whole-word
//!   rotate-immediate zero tests make the past-8-bit unary continuation
//!   O(q/32) (total run from bit 0 — the caller never adds; unterminated
//!   as status; budget = slice length).
//! - [`sample_step`] — **PR B, the composite sample-step**: owns the
//!   hybrid's tier dispatch (tier-1 `ctz8` → C1 De Bruijn → C2), the
//!   pinned remainder extract, the drain count q+1+p, and the fold — the
//!   comprehensive correctness demonstration of the kernel track.
//! - [`refill_merge`] — **PR B, the `rev8` refill merge** (the ratified
//!   shape witness; its cost is the probe's left-to-measure question).
//!
//! Standalone workspace; cargo config is **inherited** from the repo root —
//! a local `.cargo/config.toml` here would double `-Tgba.ld` at link time
//! (FLAC.md "Second leak variant"). Same pattern as `examples/flac_spike/`.

#![no_std]
#![deny(unsafe_code)]
#![warn(missing_docs)]

pub mod asm;
pub mod ctz32;
pub mod refill_merge;
pub mod rice_unmap;
pub mod rm_vectors;
pub mod sample_step;
pub mod ss_vectors;
pub mod tables;
pub mod unmap_vectors;
pub mod vectors;
pub mod word_skip;
pub mod ws_vectors;
