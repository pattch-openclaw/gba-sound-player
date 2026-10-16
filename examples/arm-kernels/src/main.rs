//! Kernel-check ROM — the on-target witness of the E10 ARM kernels.
//!
//! PURPOSE (see ../../../FLAC.md → "E10 groundwork"): the ARM-mode
//! assembly kernels cannot run on the host (aarch64 machine, no ARM
//! executor — see the crate docs), so mGBA's ARM7TDMI is the execution
//! oracle: this ROM sweeps `arm_ctz32` over `arm_kernels::vectors::VECTORS`
//! and `arm_rice_unmap` over `arm_kernels::unmap_vectors::UNMAP_VECTORS` —
//! the SAME committed tables the host twins witness — then
//! reports per the repo's screen convention: **blue = every vector
//! matched, red = mismatch, purple = never completed**. Works on cart as
//! well as emulator.
//!
//! What this ROM deliberately is NOT: no decode-path integration, no
//! cycle windows, no timing of any kind. The kernel's speed is the E10
//! probe's question, gated on the probe plan (FLAC.md); this ROM answers
//! only "does the assembly compute the table's answers on ARM7TDMI
//! semantics". The ARM-mode-ness of the body is witnessed offline from
//! the built ELF (`objdump -d`: 4-byte ARM encodings) — a passing sweep
//! alone cannot distinguish modes, since a Thumb build of the same
//! algorithm would also pass.

#![cfg_attr(target_arch = "arm", no_std)]
#![cfg_attr(target_arch = "arm", no_main)]

/// Host-build stub: the ROM entry exists only for `thumbv4t-none-eabi`
/// (agb is a target-gated dependency). `cargo test` never builds this bin
/// (`test = false` in Cargo.toml); the stub exists so host tooling can
/// parse the crate without the host-incompatible agb graph.
#[cfg(not(target_arch = "arm"))]
fn main() {}

#[cfg(target_arch = "arm")]
mod rom {
    use agb::display::Rgb;
    use arm_kernels::ctz32::ctz32_asm;
    use arm_kernels::rice_unmap::rice_unmap_asm;
    use arm_kernels::unmap_vectors::UNMAP_VECTORS;
    use arm_kernels::vectors::VECTORS;

    const COLOR_PENDING: agb::display::Rgb15 = Rgb::new(128, 0, 255).to_rgb15();
    const COLOR_PASS: agb::display::Rgb15 = Rgb::new(0, 0, 255).to_rgb15();
    const COLOR_FAIL: agb::display::Rgb15 = Rgb::new(255, 0, 0).to_rgb15();

    /// Sweep the C1 committed table through the ARM kernel. Only mismatches
    /// and a summary go to serial (a clean sweep is one line, not 346).
    fn sweep_ctz32() -> bool {
        agb::println!(
            "[kernels] ctz32 sweep start: {} vectors vs committed table",
            VECTORS.len()
        );
        let mut mismatches = 0usize;
        let mut first_offender: Option<usize> = None;
        for (i, (w, expected)) in VECTORS.iter().enumerate() {
            let got = ctz32_asm(*w);
            if got != *expected {
                mismatches += 1;
                if first_offender.is_none() {
                    first_offender = Some(i);
                    agb::println!(
                        "[kernels] FIRST MISMATCH vector {i}: w=0x{:08X} expected={} got={}",
                        w,
                        expected,
                        got
                    );
                }
            }
        }
        if mismatches == 0 {
            agb::println!(
                "[kernels] ctz32 sweep clean: {}/{} vectors exact",
                VECTORS.len(),
                VECTORS.len()
            );
            true
        } else {
            agb::println!(
                "[kernels] ctz32 sweep FAILED: {} mismatches, first at vector {}",
                mismatches,
                first_offender.unwrap_or(0)
            );
            false
        }
    }

    /// Sweep the PR A fold's committed table through the ARM kernel — same
    /// shape as the C1 sweep, judged against the SAME bytes the host twin
    /// judges (shared-consumer rule).
    fn sweep_rice_unmap() -> bool {
        agb::println!(
            "[kernels] rice_unmap sweep start: {} vectors vs committed table",
            UNMAP_VECTORS.len()
        );
        let mut mismatches = 0usize;
        let mut first_offender: Option<usize> = None;
        for (i, (n, expected)) in UNMAP_VECTORS.iter().enumerate() {
            let got = rice_unmap_asm(*n);
            if got != *expected {
                mismatches += 1;
                if first_offender.is_none() {
                    first_offender = Some(i);
                    agb::println!(
                        "[kernels] FIRST MISMATCH vector {i}: n=0x{:08X} expected={} got={}",
                        n,
                        expected,
                        got
                    );
                }
            }
        }
        if mismatches == 0 {
            agb::println!(
                "[kernels] rice_unmap sweep clean: {}/{} vectors exact",
                UNMAP_VECTORS.len(),
                UNMAP_VECTORS.len()
            );
            true
        } else {
            agb::println!(
                "[kernels] rice_unmap sweep FAILED: {} mismatches, first at vector {}",
                mismatches,
                first_offender.unwrap_or(0)
            );
            false
        }
    }

    #[agb::entry]
    fn main(mut gba: agb::Gba) -> ! {
        agb::eprintln!("[kernels] entry started — E10 kernel-check ROM (no timing, no decode)");

        let mut gfx = gba.graphics.get();
        gfx.set_background_palette_colour(0, 0, COLOR_PENDING);

        // The sentinel contract gets its own line: "run past the window"
        // (w == 0 -> 32) is the shape the probe's refill logic keys on, so
        // it is called out explicitly rather than hiding inside the sweep.
        let sentinel = ctz32_asm(0);
        let sentinel_ok = sentinel == 32;
        agb::println!(
            "[kernels] sentinel: ctz32(0) = {} [{}]",
            sentinel,
            if sentinel_ok { "OK" } else { "FAIL" }
        );

        // The fold's convention anchor gets the same explicit treatment:
        // the RFC §9.2.7.2 worked example (folded 38 → +19) is the exact
        // assertion that convicted step 3a's sign-flipped scaffold doc, so
        // it is called out on-target, not left inside the sweep. The
        // total-fold extreme (0xFFFFFFFF → i32::MIN, the §9.2.7.3-forbidden
        // value this fold maps but never rejects) is the other pinned seam.
        let rfc = rice_unmap_asm(38);
        let rfc_ok = rfc == 19;
        agb::println!(
            "[kernels] rfc-example: rice_unmap(38) = {} [{}]",
            rfc,
            if rfc_ok { "OK" } else { "FAIL" }
        );
        let extreme = rice_unmap_asm(0xFFFF_FFFF);
        let extreme_ok = extreme == i32::MIN;
        agb::println!(
            "[kernels] total-fold-extreme: rice_unmap(0xFFFFFFFF) = {} [{}]",
            extreme,
            if extreme_ok { "OK" } else { "FAIL" }
        );

        let sweep_pass = sweep_ctz32() && sweep_rice_unmap();
        let pass = sweep_pass && sentinel_ok && rfc_ok && extreme_ok;

        let (color, label) = if pass {
            (COLOR_PASS, "BLUE")
        } else {
            (COLOR_FAIL, "RED")
        };
        gfx.set_background_palette_colour(0, 0, color);
        agb::eprintln!(
            "[kernels] verdict {} — screen {}",
            if pass { "PASSED" } else { "FAILED" },
            label
        );

        loop {
            let frame = gfx.frame();
            frame.commit();
        }
    }
}
