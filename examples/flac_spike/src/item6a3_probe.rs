//! Split-ladder item 6 PR **6a-3** — the codegen arm, in the calibration
//! instrument. Not a decode path, and it never runs.
//!
//! The one question this instrument exists to answer for 6a-3 is stated in the
//! plan of record (FLAC.md, "Split-ladder item 6", rung 6a-3):
//!
//! > does an **out-of-crate** implementor still trigger the **+2,984 B** of
//! > *other* code under `lto = "fat"`?
//!
//! The number is item 5 PR 3's, and its shape is the surprising part. With
//! `rice-hybrid` on, that build's `nm` reports **no `rice_hybrid` symbol in the
//! image at all** — neither the walk nor its tables — and `.text` grows by 2,984
//! B entirely in *other* code: `residual::decode_residual` +1,888,
//! `subframe::decode_subframe` +688, a newly outlined
//! `bits::BitReader::read_bits.815` +388, `subframe::integrate_lpc` +20. PR 3's
//! own probes put the trigger at "a reachable kernel body", not at a call: emptying
//! the kernel body returns the control's bytes, and reaching the walk costs the
//! whole 2,984 B "regardless of whether anything ever calls the kernel".
//!
//! So the reading under test is a **codegen-unit** reading: the cost lands on
//! whatever shares a codegen unit with a reachable kernel body. That reading has a
//! variable PR 3 could not vary — its kernel lived in the same crate as the decode
//! path. 6a-3's arm lives in another crate. Under a fat LTO a crate boundary is not
//! a codegen-unit boundary, and the claim that 6a-4 rests on is that this seam
//! costs nothing *beyond* what the in-core form already cost; this file is how that
//! gets measured rather than assumed.
//!
//! # The arms, and the one variable
//!
//! Three builds of this crate, and the code below is identical in all three:
//!
//! * **A** — the shipped tree, no probe. `.text` `f0f88c33d150…` at 46,008 B: the
//!   anchor every record in this item compares against.
//! * **A′** — this file present, both features **off**: an empty module and a new
//!   path-dependency edge in the manifest. The control for "adding the edge and the
//!   file costs a ROM nothing".
//! * **T** — `--features item6a3-twin`: the kept kernel body is
//!   `flac_lite_gba::rice_twin::TwinKernel`'s, i.e. **an impl declared in another
//!   crate**, which forwards to the core's hybrid twin.
//! * **H** — `--features item6a3-hybrid`: the kept kernel body is
//!   `flac_lite::rice_hybrid::HybridRiceKernel`'s, reached through the same call
//!   shape one line away.
//!
//! T and H have the **same core feature set** (`item6a3-twin` turns on the overlay,
//! whose own `rice-twin` forwards to `flac-lite/rice-hybrid`, so `rice-hybrid` is
//! on in T as well) and the **same probe shape**. The only variable is which crate
//! the `impl RiceKernel` block lives in. That is the comparison the plan asks for,
//! and it is not the same question as "re-run PR 3's number": PR 3 varied a feature
//! against a tree with no such probe, and its probe called the incumbent from a cold
//! arm. Re-reading that number here would be a different experiment.
//!
//! A fourth arm is inside T at no extra build: [`never_called`] is an instantiation
//! of the same generic over the same foreign kernel that nothing references. If the
//! plan's "an arm that is never called is measured only as a link-shape change"
//! clause is right, `nm` will not contain it.
//!
//! # What this is not
//!
//! It is not run. Nothing in the ROM's decode loop, its decode-proof pass, its
//! asset verification or its cycle harness calls any of this; the entry point never
//! reaches it. The cycle rows this instrument prints are therefore a **control** for
//! 6a-3, not a measurement of it: they say the harness and the image are unchanged
//! by the edge, and 6a-3 owes no cycle figure of its own (the plan's word for an
//! arm of this shape is "measured only as a link-shape change"). The correctness of
//! what the foreign kernel *does* is witnessed by the host differential
//! (`crates/flac-lite-gba/tests/twin_from_outside.rs`), not here.
//!
//! The kept functions carry `#[used]` on purpose: they must survive to the final
//! image so that their own symbols are visible in `nm` and subtractable from the
//! section delta, and they must survive *without* any artificial FFI edge — the
//! probe's call site is a plain Rust call, because an `extern "C"` boundary would
//! itself inhibit inlining and contaminate the thing being measured. Whether `used`
//! actually survives a fat LTO is verified from the symbol table, not assumed (see
//! the record); if it had not, this file would need `no_mangle` instead.

use flac_lite::bits::BitReader;
use flac_lite::residual::PartitionHeader;

/// The shape shared by both kept arms: one call site of the core's generic entry,
/// with the kernel named only in the type parameter. Written once per arm because
/// the type parameter is the variable; nothing else differs.
macro_rules! keep_arm {
    ($name:ident, $kernel:path) => {
        /// Kept by `used`, never reached by the ROM. Its body being reachable is
        /// the whole experiment: that is the state PR 3 identified as the trigger.
        #[unsafe(no_mangle)]
        extern "Rust" fn $name(
            reader: &mut BitReader<'_>,
            header: &PartitionHeader,
            out: &mut [i32],
        ) -> i32 {
            match flac_lite::residual::decode_rice_partition_with::<$kernel>(reader, header, out) {
                Ok(()) => 0,
                Err(_) => 1,
            }
        }
    };
}

#[cfg(feature = "item6a3-twin")]
mod twin_arm {
    use super::*;

    keep_arm!(__item6a3_keep_twin, flac_lite_gba::rice_twin::TwinKernel);

    /// Present in the graph, referenced by nothing. Expected to be absent from
    /// `nm`, which is the "never called" leg of this rung measured rather than
    /// asserted.
    #[allow(dead_code)]
    fn __item6a3_never_called_twin(
        reader: &mut BitReader<'_>,
        header: &PartitionHeader,
        out: &mut [i32],
    ) -> i32 {
        match flac_lite::residual::decode_rice_partition_with::<flac_lite_gba::rice_twin::TwinKernel>(
            reader, header, out,
        ) {
            Ok(()) => 0,
            Err(_) => 1,
        }
    }
}

#[cfg(feature = "item6a3-hybrid")]
mod hybrid_arm {
    use super::*;

    keep_arm!(
        __item6a3_keep_hybrid,
        flac_lite::rice_hybrid::HybridRiceKernel
    );
}
