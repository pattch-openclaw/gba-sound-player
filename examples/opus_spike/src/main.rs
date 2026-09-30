//! Opus spike scaffold ROM — PR 1: the compile/link proof, booted.
//!
//! PURPOSE (see ../../../OPUS.md → "Measurement plan — start here"): PR 1
//! proves the vendored patched `opus-rs` **links** for `thumbv4t-none-eabi`
//! (the real link, not a bare check) and boots an image that witnesses two
//! facts about itself on serial:
//!
//! 1. **Decode-path link witness**: the address of the `probe::decode_packet`
//!    seam lands inside the ROM mapping — the decode path (and the vendored
//!    decoder's call graph through it) really is in this image, not stripped
//!    by `lto = "fat"`.
//! 2. **State-size witness**: the inline decoder-state size (the EWRAM-plan
//!    number, pinned host-side by `tests/scaffold_pins.rs`) prints, bounded
//!    sanity-checked against the 256 KB EWRAM it must one day fit inside
//!    alone (below 128 KiB or above 256 KiB, the EWRAM argument is void —
//!    RED).
//!
//! What this ROM deliberately does NOT do: construct the 178 KB decoder
//! (stack-hostile by construction — the Box-to-EWRAM story is a later PR's
//! design), decode a packet (no embedded bytes until step 2's arms land), or
//! measure any cycle (correctness-before-speed: the harness arrives only
//! after the decode proof). The constructor contract is pinned by the host
//! gate.
//!
//! Screen verdict follows the spike convention (works on cart, no cable):
//! **blue = every check passed, red = any failed, purple = never completed.**

#![cfg_attr(target_arch = "arm", no_std)]
#![cfg_attr(target_arch = "arm", no_main)]

#[cfg(target_arch = "arm")]
mod rom {
    use agb::display::Rgb;
    use opus_spike::probe;

    const COLOR_PENDING: agb::display::Rgb15 = Rgb::new(128, 0, 255).to_rgb15();
    const COLOR_PASS: agb::display::Rgb15 = Rgb::new(0, 0, 255).to_rgb15();
    const COLOR_FAIL: agb::display::Rgb15 = Rgb::new(255, 0, 0).to_rgb15();

    /// Game Pak ROM mirror base — the mapping a linked-in code address must
    /// fall in for the link witness to mean anything.
    const ROM_MAPPING: core::ops::Range<usize> = 0x0800_0000..0x0D80_0000;

    /// EWRAM bounds for the state-size sanity check: 256 KB total EWRAM is
    /// the whole budget the decoder state must fit inside alone (OPUS.md
    /// memory plan); an inline state below 128 KiB would contradict the
    /// measured pin, and above 256 KiB cannot ever fit. Bounds, not the pin
    /// itself — the exact value is pinned host-side where a regression is a
    /// named test failure, not a screen colour.
    const STATE_MIN_SANE: usize = 128 * 1024;
    const STATE_MAX_SANE: usize = 256 * 1024;

    #[agb::entry]
    fn main(mut gba: agb::Gba) -> ! {
        agb::eprintln!(
            "[opus-spike] entry started — opus perf-gate scaffold ROM (PR 1: compile/link proof)"
        );

        let mut gfx = gba.graphics.get();
        gfx.set_background_palette_colour(0, 0, COLOR_PENDING);

        // Check 1 — decode-path link witness. Taking the seam's address
        // forces `probe::decode_packet` and its call graph (the vendored
        // decoder) to survive the link; the address itself proves placement.
        let decode_fn = probe::decode_packet as usize;
        let linked = ROM_MAPPING.contains(&decode_fn);
        agb::println!(
            "[opus-spike] check decode-path link: probe::decode_packet @ 0x{:08X} in 0x08000000..0x0D800000 [{}]",
            decode_fn,
            if linked { "OK" } else { "NOT IN ROM MAPPING" }
        );

        // Check 2 — state-size witness (the EWRAM-plan number). The exact
        // value is pinned by the host gate; the ROM logs it and bounds it.
        let state_bytes = probe::DECODER_STATE_BYTES;
        let bounded = state_bytes >= STATE_MIN_SANE && state_bytes < STATE_MAX_SANE;
        agb::println!(
            "[opus-spike] check decoder state: inline {} bytes, within [{}, {}) EWRAM sanity bounds [{}]",
            state_bytes,
            STATE_MIN_SANE,
            STATE_MAX_SANE,
            if bounded { "OK" } else { "OUT OF BOUNDS" }
        );

        // Context lines: the derived budget the future harness argues
        // against, and the profile the manifest will pin. Logged, not
        // checked (nothing is decoded yet).
        // The per-sample figure divides at COMPILE time only (the spike
        // keeps software divide out of the ROM, flac_spike's rule extended
        // to reporting paths).
        const C_PER_SAMPLE: u32 =
            probe::BUDGET_CYCLES_PER_PACKET / probe::frame_samples(48_000) as u32;
        agb::println!(
            "[opus-spike] context: budget/packet = 20 ms x 16.78 MHz = {} cycles ({} c/sample at {} samples) — derived, host-pinned",
            probe::BUDGET_CYCLES_PER_PACKET,
            C_PER_SAMPLE,
            probe::frame_samples(48_000)
        );
        agb::println!(
            "[opus-spike] scaffold verdict: {} (decode proof + timing arrive with the embedded arms — correctness-before-speed)",
            if linked && bounded {
                "LINK PROVEN — BLUE"
            } else {
                "FAILED — RED"
            }
        );
        gfx.set_background_palette_colour(
            0,
            0,
            if linked && bounded {
                COLOR_PASS
            } else {
                COLOR_FAIL
            },
        );

        loop {
            let frame = gfx.frame();
            frame.commit();
        }
    }
}

/// Host-build stub: the ROM entry exists only for `thumbv4t-none-eabi` (see
/// the module docs). `cargo test` never runs this bin (`test = false`), and
/// nothing ships it — this stub exists so host tooling can parse the crate
/// without the (host-incompatible) `agb` dependency.
#[cfg(not(target_arch = "arm"))]
fn main() {}
