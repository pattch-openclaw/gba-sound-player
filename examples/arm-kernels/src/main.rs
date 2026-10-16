//! Kernel-check ROM — the on-target witness of the E10 C1 kernel.
//!
//! PURPOSE (see ../../../FLAC.md → "E10 groundwork"): the ARM-mode
//! assembly kernel cannot run on the host (aarch64 machine, no ARM
//! executor — see the crate docs), so mGBA's ARM7TDMI is the execution
//! oracle: this ROM sweeps `arm_ctz32` over the SAME committed vector
//! table the host twin witnesses (`arm_kernels::vectors::VECTORS`), then
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
    use arm_kernels::vectors::VECTORS;

    const COLOR_PENDING: agb::display::Rgb15 = Rgb::new(128, 0, 255).to_rgb15();
    const COLOR_PASS: agb::display::Rgb15 = Rgb::new(0, 0, 255).to_rgb15();
    const COLOR_FAIL: agb::display::Rgb15 = Rgb::new(255, 0, 0).to_rgb15();

    /// Sweep the committed table through the ARM kernel. Only mismatches
    /// and a summary go to serial (a clean sweep is one line, not 346).
    fn sweep() -> bool {
        agb::println!(
            "[kernels] sweep start: {} vectors vs committed table",
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
                "[kernels] sweep clean: {}/{} vectors exact",
                VECTORS.len(),
                VECTORS.len()
            );
            true
        } else {
            agb::println!(
                "[kernels] sweep FAILED: {} mismatches, first at vector {}",
                mismatches,
                first_offender.unwrap_or(0)
            );
            false
        }
    }

    #[agb::entry]
    fn main(mut gba: agb::Gba) -> ! {
        agb::eprintln!("[kernels] entry started — E10 C1 kernel-check ROM (no timing, no decode)");

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

        let sweep_pass = sweep();
        let pass = sweep_pass && sentinel_ok;

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
