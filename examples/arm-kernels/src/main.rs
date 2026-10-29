//! Kernel-check ROM — the on-target witness of the E10 ARM kernels.
//!
//! PURPOSE (see ../../../FLAC.md → "E10 groundwork"): the ARM-mode
//! assembly kernels cannot run on the host (aarch64 machine, no ARM
//! executor — see the crate docs), so mGBA's ARM7TDMI is the execution
//! oracle: this ROM sweeps every kernel over its committed table —
//! `arm_ctz32` (`vectors::VECTORS`), `arm_rice_unmap`
//! (`unmap_vectors::UNMAP_VECTORS`), and the PR B trio `arm_word_skip`
//! (`ws_vectors::WS_VECTORS`), `arm_sample_step`
//! (`ss_vectors::SS_VECTORS`), `arm_refill_merge` (`rm_vectors::RM_VECTORS`)
//! — the SAME committed tables the host twins witness — then reports per
//! the repo's screen convention: **blue = every vector matched, red =
//! mismatch, purple = never completed**. Works on cart as well as
//! emulator. The untouched C1/PR A sweeps ride along as the control: they
//! must reproduce their own records in the same image.
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
    use arm_kernels::refill_merge::refill_merge_asm;
    use arm_kernels::rice_unmap::rice_unmap_asm;
    use arm_kernels::rm_vectors::RM_VECTORS;
    use arm_kernels::sample_step::sample_step_asm;
    use arm_kernels::ss_vectors::SS_VECTORS;
    use arm_kernels::unmap_vectors::UNMAP_VECTORS;
    use arm_kernels::vectors::VECTORS;
    use arm_kernels::word_skip::word_skip_asm;
    use arm_kernels::ws_vectors::WS_VECTORS;

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

    /// Sweep the C2 committed table through the ARM kernel — same shape
    /// as the existing sweeps, judged against the SAME bytes the host
    /// twin judges (shared-consumer rule).
    fn sweep_word_skip() -> bool {
        agb::println!(
            "[kernels] word_skip sweep start: {} vectors vs committed table",
            WS_VECTORS.len()
        );
        let mut mismatches = 0usize;
        let mut first_offender: Option<usize> = None;
        for (i, v) in WS_VECTORS.iter().enumerate() {
            let words = [v.0, v.1, v.2, v.3];
            let got = word_skip_asm(&words, v.4);
            if got != (v.5, v.6, v.7) {
                mismatches += 1;
                if first_offender.is_none() {
                    first_offender = Some(i);
                    agb::println!(
                        "[kernels] FIRST MISMATCH ws vector {i}: {:#010X} {:#010X} {:#010X} {:#010X} b={} expected=({},{},{}) got=({},{},{})",
                        v.0,
                        v.1,
                        v.2,
                        v.3,
                        v.4,
                        v.5,
                        v.6,
                        v.7,
                        got.0,
                        got.1,
                        got.2
                    );
                }
            }
        }
        if mismatches == 0 {
            agb::println!(
                "[kernels] word_skip sweep clean: {}/{} vectors exact",
                WS_VECTORS.len(),
                WS_VECTORS.len()
            );
            true
        } else {
            agb::println!(
                "[kernels] word_skip sweep FAILED: {} mismatches, first at vector {}",
                mismatches,
                first_offender.unwrap_or(0)
            );
            false
        }
    }

    /// Sweep the composite's committed table through the ARM kernel.
    fn sweep_sample_step() -> bool {
        agb::println!(
            "[kernels] sample_step sweep start: {} vectors vs committed table",
            SS_VECTORS.len()
        );
        let mut mismatches = 0usize;
        let mut first_offender: Option<usize> = None;
        for (i, v) in SS_VECTORS.iter().enumerate() {
            let words = [v.0, v.1, v.2, v.3];
            let got = sample_step_asm(&words, v.4, v.5);
            if got != (v.6, v.7, v.8, v.9, v.10, v.11) {
                mismatches += 1;
                if first_offender.is_none() {
                    first_offender = Some(i);
                    agb::println!(
                        "[kernels] FIRST MISMATCH ss vector {i}: {:#010X} {:#010X} {:#010X} {:#010X} b={} p={} expected=({},{},{},{},{},{}) got=({},{},{},{},{},{})",
                        v.0,
                        v.1,
                        v.2,
                        v.3,
                        v.4,
                        v.5,
                        v.6,
                        v.7,
                        v.8,
                        v.9,
                        v.10,
                        v.11,
                        got.0,
                        got.1,
                        got.2,
                        got.3,
                        got.4,
                        got.5
                    );
                }
            }
        }
        if mismatches == 0 {
            agb::println!(
                "[kernels] sample_step sweep clean: {}/{} vectors exact",
                SS_VECTORS.len(),
                SS_VECTORS.len()
            );
            true
        } else {
            agb::println!(
                "[kernels] sample_step sweep FAILED: {} mismatches, first at vector {}",
                mismatches,
                first_offender.unwrap_or(0)
            );
            false
        }
    }

    /// Sweep the refill-merge table through the ARM kernel.
    fn sweep_refill_merge() -> bool {
        agb::println!(
            "[kernels] refill_merge sweep start: {} vectors vs committed table",
            RM_VECTORS.len()
        );
        let mut mismatches = 0usize;
        let mut first_offender: Option<usize> = None;
        for (i, (window, byte, expected)) in RM_VECTORS.iter().enumerate() {
            let got = refill_merge_asm(*window, *byte);
            if got != *expected {
                mismatches += 1;
                if first_offender.is_none() {
                    first_offender = Some(i);
                    agb::println!(
                        "[kernels] FIRST MISMATCH rm vector {i}: w={:#010X} byte={} expected={:#010X} got={:#010X}",
                        window,
                        byte,
                        expected,
                        got
                    );
                }
            }
        }
        if mismatches == 0 {
            agb::println!(
                "[kernels] refill_merge sweep clean: {}/{} vectors exact",
                RM_VECTORS.len(),
                RM_VECTORS.len()
            );
            true
        } else {
            agb::println!(
                "[kernels] refill_merge sweep FAILED: {} mismatches, first at vector {}",
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

        // ---- PR B contract lines: the seams the design of record pins,
        // each asserted on TARGET, not folded into the sweeps ----
        // C2, decision 4's boundary: budget 0 answers (0, 0, 0) without
        // reading a word of the slice.
        let ws_b0 = word_skip_asm(&[0; 4], 0);
        let ws_b0_ok = ws_b0 == (0, 0, 0);
        agb::println!(
            "[kernels] ws-budget-zero: word_skip(zeros, 0) = ({}, {}, {}) [{}]",
            ws_b0.0,
            ws_b0.1,
            ws_b0.2,
            if ws_b0_ok { "OK" } else { "FAIL" }
        );
        // C2, decision 2: q is the TOTAL from bit 0 — the q = 32 slice is
        // answered 32 by the kernel; the caller never adds a "32".
        let ws_tot = word_skip_asm(&[0, 1, 0, 0], 4);
        let ws_tot_ok = ws_tot == (32, 2, 1);
        agb::println!(
            "[kernels] ws-total-from-bit0: word_skip([0,1,0,0], 4) = ({}, {}, {}) [{}]",
            ws_tot.0,
            ws_tot.1,
            ws_tot.2,
            if ws_tot_ok { "OK" } else { "FAIL" }
        );
        // C2, decision 3: the unterminated run is STATUS data — q keeps
        // the walk's answer (32 per zero word), never a cap artifact.
        let ws_un = word_skip_asm(&[0; 4], 3);
        let ws_un_ok = ws_un == (96, 3, 0);
        agb::println!(
            "[kernels] ws-unterminated: word_skip(zeros, 3) = ({}, {}, {}) [{}]",
            ws_un.0,
            ws_un.1,
            ws_un.2,
            if ws_un_ok { "OK" } else { "FAIL" }
        );
        // The composite's drift sentinel (decision 7's pinned extract):
        // q = 31 with remainder-domain bits live in words[1..3] — the
        // remainder is 0 and the residual is the fold of 31 << 8 alone.
        // Any merge-or-reversal reading of the window prints nonzero rem.
        let ss_sent = sample_step_asm(&[0x8000_0000, 0xDEAD_BEEF, 0xCAFE_F00D, 0x1234_5678], 4, 8);
        let ss_sent_ok = ss_sent == (31, 0, 3968, 40, 2, 1);
        agb::println!(
            "[kernels] ss-shift32-sentinel: sample_step(q31, dirty tail, p=8) = ({}, {}, {}, {}, {}, {}) [{}]",
            ss_sent.0,
            ss_sent.1,
            ss_sent.2,
            ss_sent.3,
            ss_sent.4,
            ss_sent.5,
            if ss_sent_ok { "OK" } else { "FAIL" }
        );
        // The composite's tier dispatch boundaries (decision 2): q = 7 the
        // ctz8 tier, q = 8 the De Bruijn tier, q = 32 the C2 tier — the
        // single-bit slices answer their own q from the right tier.
        let ss_t1 = sample_step_asm(&[1 << 7, 0, 0, 0], 4, 0);
        let ss_t2 = sample_step_asm(&[1 << 8, 0, 0, 0], 4, 0);
        let ss_t3 = sample_step_asm(&[0, 1, 0, 0], 4, 0);
        let ss_tiers_ok = ss_t1 == (7, 0, -4, 8, 1, 1)
            && ss_t2 == (8, 0, 4, 9, 1, 1)
            && ss_t3 == (32, 0, 16, 33, 2, 1);
        agb::println!(
            "[kernels] ss-tier-boundaries: q7={} q8={} q32={} [{}]",
            ss_t1.0,
            ss_t2.0,
            ss_t3.0,
            if ss_tiers_ok { "OK" } else { "FAIL" }
        );
        // The ratified refill merge's placement line (ratified item 3):
        // byte 0x01's FIRST bit lands at the window top — REV8[1] = 0x80
        // merged at bit 24, the barrel-shifter cycle doing the placement.
        let rm_place = refill_merge_asm(0x00AB_CDEF, 0x01);
        let rm_place_ok = rm_place == 0x80AB_CDEF;
        agb::println!(
            "[kernels] rm-placement: refill_merge(0x00ABCDEF, 0x01) = {:#010X} [{}]",
            rm_place,
            if rm_place_ok { "OK" } else { "FAIL" }
        );

        let sweep_pass = sweep_ctz32()
            && sweep_rice_unmap()
            && sweep_word_skip()
            && sweep_sample_step()
            && sweep_refill_merge();
        let pass = sweep_pass
            && sentinel_ok
            && rfc_ok
            && extreme_ok
            && ws_b0_ok
            && ws_tot_ok
            && ws_un_ok
            && ss_sent_ok
            && ss_tiers_ok
            && rm_place_ok;

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
