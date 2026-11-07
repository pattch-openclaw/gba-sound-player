//! # flac-lite-gba — the GBA overlay for `flac-lite`
//!
//! The platform-bound half of a decoder whose portable half lives in
//! [`flac_lite`] (`crates/flac-lite/`, this repo). Split-ladder item 6 PR
//! **6a-1** created this crate, and
//! 6a-1 is all it is so far: **it exists, and nothing links it.** There is no
//! kernel here, no asm, no FFI, no `unsafe`, no feature, and no module other than
//! this doc comment. What follows is therefore the crate's *contract as planned*,
//! written down once so the decisions in it are not re-litigated inside 6a-3's or
//! 6a-4's diff. The plan of record — the PR ladder, each rung's witness, and the
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
//! * **6a-3** — an implementor of [`flac_lite::residual::RiceKernel`] *from
//!   outside the core*, in Rust: the core's own portable twin carried through the
//!   seam as an instrument arm, so that a known number (+42.6 c/sample) is
//!   reproduced through the shipped path before any unfamiliar one is read. That
//!   PR also owns the visibility questions an external implementor actually hits
//!   (`residual::rice_unmap` is `pub(crate)` today) and the question of whether
//!   the twin is re-exported or vendored — the recommendation there being
//!   re-export, because vendoring creates the drift-witness debt item 4 had to
//!   pay for.
//! * **6a-4** — the ARM-mode kernels themselves (`global_asm!` over the
//!   `.s` bodies this repo already dual-witnesses in `examples/arm-kernels/`),
//!   their FFI and its SAFETY blocks, the state-struct ABI mapping written as
//!   this crate's own contract, and the tables under the core's existing
//!   `iwram-pin` gate. That is also where `iwram-pin` **enablement** lands —
//!   one feature forward, witnessed there rather than in a PR of its own.
//! * **the convert word-store kernel**, when it lands (proposal item (d)).
//! * **not** the guarded i32 leg. The split proposal originally listed that as
//!   overlay item (b); item 6 PR 0 moved it into the **core**, because the bound
//!   is computed from data the core's own parse already holds (order, the
//!   coefficients, the sample width), so the width needs no seam, no overlay and
//!   no second instantiation on the platform side. This is the place a reader of
//!   the original proposal finds that corrected; the arithmetic and the guard are
//!   in the plan's "width decision", and 6a-6 is where they land.
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
//!   stays correct for arbitrary FLAC, and profile facts stay the core's.
//! * **Not `dyn`, anywhere.** The seam is monomorphized or the realization is
//!   not worth shipping — and that is a measured claim, not a style preference:
//!   item 5 PR 3 measured what the mere *presence* of a second implementor costs
//!   on the live link (+2,984 B of *other* code re-inlined, with not one symbol
//!   of the added mechanism in the image). The E2b lesson ("a −21.7% probe became
//!   +38% on the live link") is why every rung of 6a is witnessed on the link a
//!   ROM builds, not in isolation.
//! * **Not a ROM, and no `agb`.** This is a library. The instrument that links
//!   it is an existing one — `examples/flac_spike/` for cycles, the kernel-check
//!   ROM for ARM correctness — and putting `agb` here would move the mixer/DMA
//!   and `agb` integration work (Phase 2 step 9, explicitly out of item 6) into
//!   the place where a kernel is decided.
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
//! `#![deny(unsafe_code)]` is in force from the first line of this crate, and
//! 6a-1 contains **zero** `unsafe`. The first real `unsafe` surface in this
//! repository is 6a-4's asm FFI, and the budget is fixed now so it can be
//! counted later: **one `SAFETY` block per FFI site, each naming the bounds it
//! relies on** (window length, output length, table reach, the region a read is
//! bounded by), with a counted and named exception list in these docs — the same
//! ratified-exception discipline the core uses for its `.iwram` placement
//! attributes. **Current count: 0.**
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
//! * Neither gate is in `flac-gates` (that aggregate is the core's
//!   configurations, and this is a different crate whose features do not exist
//!   yet) and neither is in `test`. They are hand-invoked, like
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

// No modules in 6a-1, deliberately: the first one is 6a-3's kernel arm, and an
// empty module tree is the honest shape of a crate that nothing links.

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
