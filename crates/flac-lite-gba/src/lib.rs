//! # flac-lite-gba — the GBA overlay for `flac-lite`
//!
//! The platform-bound half of a decoder whose portable half lives in
//! [`flac_lite`] (`crates/flac-lite/`, this repo). Split-ladder item 6 PR
//! **6a-1** created this crate, 6a-3 put the first module behind a feature, and
//! 6a-4 put the first *kernel* behind a feature. What is still true of every
//! earlier rung, and still the shape this crate is required to keep: a **default
//! build** of this crate has no kernel, no asm, no FFI, no `unsafe` and no
//! module beyond this doc comment — both modules are behind their own features,
//! and no shipped decode path in either crate names this one. What links it
//! today is one instrument ROM (`examples/overlay-kernel-check/`), built with
//! `rice-arm` on, whose only job is to be a correctness witness on the device.
//! What follows is therefore the crate's *contract as planned*, written down once
//! so the decisions in it are not re-litigated inside 6a-3's, 6a-4's or 6a-5's
//! diff. The plan of record — the PR ladder, each rung's witness, and the
//! decisions already settled — is `FLAC.md`, "Split-ladder item 6 — the overlay
//! and the container: plan of record (2026-11-06)".
//!
//! ## Why a separate crate exists at all
//!
//! Item 5 PR 4's one-line brief: **the map is not the finding, the realization
//! is.** The core and the overlay would implement the *same* Rice codeword map,
//! and on the founding platform that has been measured to mean two opposite
//! things: the core's portable realization of that map measured **+42.6 / +42.7
//! cycles per decoded sample slower** than its incumbent (314/314 frames, tables
//! pinned in IWRAM, both arms decoding bit-identically), while E10's ARM-mode
//! realization of the same map measured **−349 / −348 cycles per sample** on the
//! same machine. That is the entire justification for a platform-bound realization
//! layer: not that GBA needs different semantics — it needs the same semantics
//! written in the machine's own idiom — and it is why the core's default Rice
//! kernel stays [`flac_lite::residual::TableCountKernel`] and stays unsealed.
//!
//! ## What this crate is for (the plan, by PR)
//!
//! * **6a-3** (this PR) — the seam, made passable from outside the core, in
//!   Rust, with no asm anywhere in it. `src/rice_twin.rs` + the `rice-twin`
//!   feature is one instrument arm: a `TwinKernel` that **carries** the core's
//!   portable hybrid twin through the seam by an impl written in another crate —
//!   so that a known number (+42.6 c/sample, item 5 PR 4) stays reproducible
//!   through the shipped seam before an unfamiliar one is ever read through it —
//!   and nothing else. Its witness is `tests/twin_from_outside.rs`, a differential
//!   driven from this side of the boundary over the core's own committed corpus.
//!   That witness is what found the one surface bump the rung needed: the core's
//!   `residual::rice_unmap` was `pub(crate)`, and the commit before this one makes
//!   it `pub`. The three `rice_hybrid` tables the same probe named stay private,
//!   for the reasons their new docs give. The visibility question is therefore
//!   closed by an attempt rather than by inspection, and the re-export-versus-
//!   vendor question — this plan recommended re-export because vendoring creates
//!   the drift-witness debt item 4 paid for — is closed the same way: the
//!   forwarding impl compiles with **zero** core-side changes, so the core owns
//!   one copy of that walk. Rung 6a-4's own docs note what changes if it wants
//!   them.
//!   **6a-3 changes no decode path**, and it adds one configuration behind one new
//!   gate (`make overlay-test-twin` — both the thumbv4t compile witness and the
//!   host differential) rather than changing an existing one; the counts of every
//!   existing configuration are unchanged, which is that rung's own rule.
//! * **6a-4** (this PR) — the ARM-mode kernel itself: `src/rice_arm.rs` + the
//!   `rice-arm` feature, assembling `asm/rice_partition.s` (lifted from the E10
//!   probe patch — that kernel was never landed on main, and the plan line's
//!   "`examples/arm-kernels/`" is right about the *fold* and wrong about this
//!   body; `asm/rice_partition.s`'s header carries the correction) together with
//!   `examples/arm-kernels/asm/fold_body.s`, the one committed fold source,
//!   pasted in the same `global_asm!` unit. Its FFI, the state-struct ABI
//!   written as this crate's own contract (`KernelAbi`, asserted by `offset_of!`
//!   consts), the status→`Error` mapping, and the three tables under an
//!   `iwram-pin` feature that *forwards* to the core's existing gate — that
//!   enablement is what the plan assigned here.
//!   **6a-4 moves no decode path** and makes no timing claim: the kernel is
//!   correct-only, and cycles are 6a-5's reading. Its witnesses are
//!   `tests/arm_from_outside.rs` (host differential + boundary arms; the host
//!   runs a label-for-label Rust model of the asm, because there is no ARM
//!   executor here), a kernel-check ROM (`make overlay-kernel-rom`, which runs
//!   the assembled kernel *and* the model against the core's incumbent on the
//!   device), and `objdump -d` of the built ELF for the two standing
//!   no-degradation rules of this repo: four-byte ARM encodings, and a leaf
//!   `pop {…,pc}`.
//! * **the convert word-store kernel**, when it lands (proposal item (d)).
//! * **not** the guarded i32 leg. The split proposal originally listed that as
//!   overlay item (b); item 6 PR 0 moved it into the **core**, because the bound
//!   is computed from data the core's own parse already holds (order, the
//!   coefficients, the sample width), so the width needs no seam, no overlay and
//!   no second instantiation on the platform side. This is the place a reader of
//!   the original proposal finds that corrected; the arithmetic and the guard are
//!   in the plan's "width decision", and 6a-6 landed them in the core.
//!
//! ## What this crate is not
//!
//! * **It owns no decode semantics.** Correctness stays the core's witnesses —
//!   the same committed golden tables, the same `flac -d` bit-exactness. A kernel
//!   here is a *realization*, and the boundary list it owes (the spelled
//!   `q = 31` leg, the `w == 0` handoff, the `p = 0` mask, §9.2.7.3's two
//!   guards, the unterminated/short-remainder **restore-nothing** contract, reads
//!   bounded by the window's own region) is stated in the seam's trait docs, not
//!   renegotiated here. In particular the kernels' loud rejection of escape
//!   partitions and `p > 14` is a **probe fact, never a contract**: the core
//!   stays correct for arbitrary FLAC, and profile facts stay the core's. What
//!   6a-4 actually did with that list is in `src/rice_arm.rs`: the two
//!   §9.2.7.3 guards are *tested* by the kernel rather than wrapped (the probe
//!   left them wrapped, and the seam's incumbent rejects them), the
//!   restore-nothing rule is kept and its divergence from the incumbent's
//!   per-read restore is **witnessed as a difference** rather than hidden, and
//!   orders above this realization's 16 are *delegated* to the incumbent rather
//!   than rejected.
//! * **Not `dyn`, anywhere.** The seam is monomorphized or the realization is
//!   not worth shipping — and that is a measured claim, not a style preference:
//!   item 5 PR 3 measured what the mere *presence* of a second implementor costs
//!   on the live link (+2,984 B of *other* code re-inlined, with not one symbol
//!   of the added mechanism in the image). The E2b lesson ("a −21.7% probe became
//!   +38% on the live link") is why every rung of 6a is witnessed on the link a
//!   ROM builds, not in isolation.
//! * **Not a ROM, and no `agb`.** This is a library. The instruments that link
//!   it are separate crates: a new one from 6a-4,
//!   `examples/overlay-kernel-check/`, is the ARM *correctness* instrument, and
//!   `examples/flac_spike/` is the one that would be the *cycle* instrument if
//!   6a-5 wires a kernel into it. Putting `agb` here would move the mixer/DMA
//!   and `agb` integration work (Phase 2 step 9, explicitly out of item 6) into
//!   the place where a kernel is decided — and note what that distinction is
//!   for: the instrument ROM's `agb` use is a print path and a `#[test_case]`
//!   hook, not a decoder that plays sound.
//! * **Not the container.** Where frames come from — `scan::Frames`,
//!   `format::Manifest`, `scripts/pack_flac.sh`, the rate-play rewire — is 6b,
//!   which changes nothing about how a frame is decoded. `scan` stays off until
//!   6b-1 and no 6a build links the walk.
//! * **No real-time claim.** The core has never made one; the GBA hardware
//!   question stays arbitrated by E5 (mGBA vs silicon) rather than by a build
//!   succeeding here.
//!
//! ## `unsafe`, and its budget
//!
//! `#![deny(unsafe_code)]` is in force from the first line of this crate. 6a-1
//! and 6a-3 contain **zero** `unsafe`; 6a-4 adds the first, in `src/rice_arm.rs`,
//! behind `rice-arm`. The budget this crate's own docs fixed for that rung was
//! "one `SAFETY` block per FFI site, each naming the bounds it relies on", with a
//! counted and named exception list here — the same ratified-exception discipline
//! the core uses for its `.iwram` placement attributes. **Count in a default
//! build: 0. Count with `rice-arm` on: three items — the `global_asm!`, the
//! `unsafe extern "C"` declaration, and one `unsafe { … }` call site whose
//! `// SAFETY:` block names the state-object extent, the output bound, the read
//! bound (`region()`, including the word-skip's four-byte peek) and the table
//! reach of every index. With `iwram-pin` on top of `rice-arm`: six**, the three
//! above plus one `unsafe(link_section = ".iwram")]` placement attribute per
//! table.**
//!
//! Each item carries its own `#[allow(unsafe_code)]` — six allowance attributes
//! in `src/rice_arm.rs`, one per item, which is what makes the count a grep
//! rather than a claim (`tests/arm_from_outside.rs`, the budget arm, counts them
//! by attribute shape and refuses if the number moves). Prose mentions of
//! `unsafe` in that file are not counted by that pattern on purpose.
//!
//! ## Build shape and gates (the plumbing, decided once)
//!
//! * **Standalone workspace** (own `[workspace]`), so the root ROM crate never
//!   adopts it — and **no local `.cargo/config.toml`**, because cargo
//!   concatenates `target.<triple>.rustflags` across config layers and a local
//!   re-declaration would link `-Tgba.ld` twice (a failure mode recorded three
//!   times in this repo). An rlib has no link step, so 6a-1 cannot convict that
//!   leak either way; the rule is inherited from the tree, and the first rung
//!   here where an asm-bearing link step exists is 6a-4 — in an instrument ROM,
//!   not in this crate.
//! * `make overlay-check` — the crate compiles for `thumbv4t-none-eabi`, and
//!   `flac-lite` resolves through the path dependency, for that target. It is a
//!   **compile** witness only: a library has no link step, and 6a-1 deliberately
//!   builds no ROM, so it is *not* a link witness and not a codegen witness.
//! * `make overlay-test` — the crate's host witness, run from the gate-neutral
//!   cwd for the same reason the core's host gates are (the config leak). In
//!   6a-1 that witness is one no-op test; see the test module for what it does
//!   and does not claim.
//! * `make overlay-test-twin` — what 6a-3 adds: the same two witnesses as above
//!   with `--features rice-twin` on, in one gate. Its first arm is the compile
//!   witness that an out-of-crate `RiceKernel` impl builds for `thumbv4t-none-eabi`
//!   (the half `overlay-check` cannot see, since in its default build
//!   `src/rice_twin.rs` does not exist); its second runs the corpus differential
//!   driven from this side of the boundary (`tests/twin_from_outside.rs`,
//!   `required-features = ["rice-twin"]`, so it does not exist in a default build
//!   at all). There is deliberately no `overlay-check-twin`: a gate whose arms
//!   would be identical to a subset of this one is a second name for one
//!   measurement. The rule this rung inherits from the core's own feature-gate
//!   discipline is that a new configuration is a **new gate**, never a broadening
//!   of an existing one — `make overlay-test` still runs exactly what it ran in
//!   6a-1, which is what makes "the default build of this crate still contains no
//!   kernel" a checkable statement rather than a claim.
//! * `make overlay-check-arm`, `make overlay-test-arm` and
//!   `make overlay-kernel-rom` — what 6a-4 adds, same shape: the compile
//!   witness for the assembled kernel for `thumbv4t-none-eabi`, the host
//!   correctness suite (`tests/arm_from_outside.rs`), and the one instrument ROM
//!   that links this crate and runs the assembled kernel on the device against
//!   the core's incumbent. `overlay-kernel-rom` also reports the ROM's section
//!   sizes and the tables' `nm`/`objdump` placement, because the table question
//!   (does a pinned table stay a separate object once nothing in the core needs
//!   it?) is a link question and a link question needs a link step.
//!   `overlay-test` and `overlay-check` are unchanged by this PR — they run the
//!   default build, and the default build still has no kernel in it.
//! * None of these gates is in `flac-gates` (that aggregate is the core's
//!   configurations, and this is a different crate whose features do not exist
//!   there) and none is in `test`. They are hand-invoked, like
//!   `flac-test-crc`/`-scan`/`-rice`, and for the same reason: nothing links
//!   this crate yet, so its gates are plumbing gates.
//! * There is no `flac-portable-check` arm for this crate, and that is the
//!   point: a second bare-metal target would be a *portability* witness for a
//!   crate whose entire purpose is to be platform-bound. Portability is the
//!   core's claim and stays witnessed there.
//! * The standing control-build discipline for adding a crate to this tree is
//!   the E-series instrument check: the calibration instrument's sections come
//!   back **byte-identical** (`.text` `f0f88c33d150…` at 46,008 B, plus the
//!   `nm` anchors) from a warm build and from a `cargo clean` rebuild alike.
//!   6a-1's landing record carries that table; it is the failure mode this PR
//!   exists to isolate — "adding a crate to the tree changes what the existing
//!   ROMs link" — and it is cheap to rule out once, here, rather than paying for
//!   it inside 6a-5's cycle reading.

#![no_std]
#![deny(unsafe_code)]

// 6a-1's empty module tree was the honest shape of a crate that nothing links.
// 6a-3 changed that by exactly one module, and 6a-4 by exactly one more, each
// behind its own feature: a default build of this crate still contains no
// kernel, so the claim "no shipped decode path links it" is still true of the
// default build after this PR.
#[cfg(feature = "rice-arm")]
pub mod rice_arm;
#[cfg(feature = "rice-twin")]
pub mod rice_twin;

#[cfg(test)]
mod tests {
    /// 6a-1's one test, and it is a no-op **on purpose**: this PR adds no
    /// behavior, so there is nothing here yet that could fail. It exists so that
    /// `make overlay-test` runs a real test binary instead of an empty suite;
    /// its green means "this crate compiles as a `no_std` library for the host
    /// triple and resolves the core through its path dependency", and nothing
    /// beyond that.
    ///
    /// The one statement in it is inert by construction: `type_name` resolves a
    /// type path at compile time and hands back a string it never interprets.
    /// Naming [`flac_lite::Error`] witnesses the dependency edge — it makes no
    /// claim about what the core *does*. It is deliberately **not** the seam:
    /// whether the seam is implementable from outside the core is 6a-3's
    /// question, and answering it here by accident would spend the one thing
    /// that rung exists to find out.
    #[test]
    fn nothing_yet_except_the_dependency_edge() {
        let path = core::any::type_name::<flac_lite::Error>();
        assert!(
            !path.is_empty(),
            "the core's public surface must be nameable from the overlay"
        );
    }
}
