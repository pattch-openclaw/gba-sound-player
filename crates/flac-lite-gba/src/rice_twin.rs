//! The 6a-3 arm: item 5's portable twin, carried through the seam from
//! **outside** the core.
//!
//! Split-ladder item 6 PR **6a-3**. This module's entire content is one
//! `impl [`flac_lite::residual::RiceKernel`]` written in a *different crate*, and
//! that is the point of it: every earlier rung of the ladder had the trait and
//! both of its implementors inside `flac-lite`, so "the seam is implementable
//! from outside" was an assumption carried into 6a-4 — the rung where an ARM-mode
//! asm kernel has to implement it over an FFI boundary, with no way back to a
//! private item. This PR moves that question to where the only variable is
//! visibility.
//!
//! ## Carried, not vendored
//!
//! [`TwinKernel`] does not re-implement the hybrid walk. It forwards to
//! [`flac_lite::rice_hybrid::HybridRiceKernel`], the twin item 5 PR 3 wrote and
//! PR 4 measured (+42.6 / +42.7 cycles per decoded sample *slower* than the
//! incumbent on the founding platform — see `FLAC.md`, split-ladder item 5).
//!
//! That choice is deliberate, and it is the one the plan of record recommends:
//! vendoring the walk here would create a second copy of a map that has been
//! corrected twice (the `q = 31` / `w == 0` leg, §9.2.7.3's two guards), i.e.
//! exactly the drift-witness debt item 4 had to pay for — and 6a-3's claim is
//! that *the core's own mechanism* runs from outside the crate, which a vendored
//! copy cannot establish. What this crate adds is the impl block, i.e. the
//! cross-crate name resolution and the orphan-rule shape 6a-4 will need, plus the
//! witness that the resulting arm decodes what the core's mechanisms decode.
//!
//! One limit of "carried" worth naming here rather than discovering later:
//! [`HybridRiceKernel`] is itself not a second implementation for `order >= 32` — it
//! forwards that branch straight to
//! [`flac_lite::residual::TableCountKernel`]. So `TwinKernel` is the incumbent, one
//! call deep, for that out-of-range parameter. That branch never fires in a
//! conforming stream (the 5-bit coding method tops out at `order == 30`, and
//! `0b11111` is the escape), so it is not a hole in the differential; but it means
//! "the arm is a distinct mechanism" holds on `order <= 30` and not on `order >= 32`.
//! 6a-4 replaces this body with an asm kernel and inherits the question of what
//! *it* does at that boundary.
//!
//! ## What this arm is, and is not, wired into
//!
//! It is an **instrument arm**: it exists to be instantiated by a caller that
//! wants both mechanisms in one build (see `tests/twin_from_outside.rs`, the
//! out-of-crate differential this PR's witness is). Nothing in either crate's
//! decode path names it — [`flac_lite::residual::decode_rice_partition`] still
//! names [`flac_lite::residual::TableCountKernel`] on one line, by PR 4's
//! measurement, and threading a kernel choice through `decode_residual` /
//! `decode_subframe` / `decode_frame` is 6a-5's change, taken with the cycle
//! reading that decides it. So a default build of this crate contains no kernel at
//! all, and a build with `rice-twin` on contains one that no production path
//! calls.
//!
//! ## The feature, and why it forwards to a core feature
//!
//! `rice-twin` is this crate's own gate, and it turns on `flac-lite/rice-hybrid`
//! — because "carried" means the code being carried is the core's, and the core
//! keeps that code behind its own gate. A core feature that a caller has to know
//! about is a real cost, and it is stated here rather than hidden: the
//! alternative was a second copy of the walk behind an overlay-only gate, which
//! would have made the overlay feature self-sufficient at the price of the drift
//! debt above.

use flac_lite::bits::BitReader;
use flac_lite::residual::RiceKernel;
use flac_lite::rice_hybrid::HybridRiceKernel;

/// The core's hybrid twin, named from this crate and implemented here.
///
/// A unit struct with a forwarding [`RiceKernel`] impl — deliberately not a re-export
/// (`pub use ... as TwinKernel`), because then this crate would implement nothing,
/// and "implements nothing" is precisely the thing 6a-4 cannot afford to have
/// assumed. The body is one call.
///
/// Why forwarding is not a cheat at the level this PR is measured at: the seam is
/// monomorphized, so an instantiated `TwinKernel` becomes the twin's own walk in
/// the same image; there is no call layer left at `-C opt-level=z` with `lto =
/// "fat"` to hide behind. Whether that instantiation is *free* in the image, and
/// whether a second implementor living in another crate re-triggers the +2,984 B
/// that item 5 PR 3 measured for a second implementor living in the *same* crate,
/// is 6a-3's open codegen question and its section table answers it, not this doc.
pub struct TwinKernel;

impl RiceKernel for TwinKernel {
    /// Forward the whole partition to the core's twin: same window handling, same
    /// spelled `q = 31` / `w == 0` leg, same §9.2.7.3 guards, same
    /// restore-nothing contract on an unterminated final codeword.
    ///
    /// The one thing worth saying about the call itself: it is written as a trait
    /// call on the concrete type rather than as a call to an inherent method,
    /// because `HybridRiceKernel` has no inherent methods — the trait *is* its
    /// surface, and that is what makes this impl the first one outside the core.
    fn decode_partition(
        reader: &mut BitReader<'_>,
        order: u32,
        out: &mut [i32],
    ) -> flac_lite::Result<()> {
        <HybridRiceKernel as RiceKernel>::decode_partition(reader, order, out)
    }
}
