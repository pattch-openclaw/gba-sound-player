//! The single assembled ARM unit for the PR B kernels (decision 6's
//! sharing made real — see the module docs and FLAC.md "E10 unit-test
//! kernel track — design of record (2026-10-16)").
//!
//! The fold's single committed source (`asm/fold_body.s`) is textually
//! pasted at BOTH use sites — the standalone `arm_rice_unmap` (PR A's
//! kernel, now carried by the shared source) and the composite
//! `arm_sample_step`. Each use assembles its own copy, so the built binary
//! carries the fold's instructions twice either way — decision 6's zero
//! runtime difference versus duplication, and one source to review.
//!
//! The paste happens through `include_str!`/`concat!` rather than an
//! assembler-level `.include`: rustc hands the assembler a temp file with
//! no reliable search path, so the textual include is done in the Rust
//! source that ships it (verified against host clang with the same
//! concatenation order: fold_body, rice_unmap, word_skip, refill_merge,
//! sample_step — 2026-10-27/28).
//!
//! **No-drift witness** (PR A's recorded encodings surviving the move to
//! the shared source): `objdump -d` of every built kernel ELF must keep
//! `arm_rice_unmap` at `e2001001`/`e2611000`/`e1a000a0`/`e0200001`
//! (FLAC.md PR A entry; re-verified at this wiring 2026-10-28).
//!
//! Edition-2024 `global_asm!` is an unsafe op and a macro-item `#[allow]`
//! is ignored — the module attribute is what binds (the crate's recorded
//! pitfall; `ctz32` carries the same note for its own unit).

/// The reviewed `unsafe_code` exception for the PR B kernels: one
/// `global_asm!` unit assembled from the five committed sources under
/// `asm/`. Both witnesses (host twins + the kernel-check ROM sweep) judge
/// exactly these bytes.
#[cfg(target_arch = "arm")]
#[allow(unsafe_code)]
mod unit {
    core::arch::global_asm!(concat!(
        include_str!("../asm/fold_body.s"),
        include_str!("../asm/rice_unmap.s"),
        include_str!("../asm/word_skip.s"),
        include_str!("../asm/refill_merge.s"),
        include_str!("../asm/sample_step.s"),
    ));
}
