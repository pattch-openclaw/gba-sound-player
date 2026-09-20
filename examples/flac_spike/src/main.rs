//! Perf-gate spike ROM — PR 1: scaffold + embedded clips.
//!
//! PURPOSE (see ../../../FLAC.md → "Perf gate spike — concrete plan and
//! validation process"): this ROM is the measurement vehicle for the go/no-go
//! perf gate. PR 1 lands the vehicle only: it boots, logs each arm's metadata
//! from the generated manifest, and verifies the embedded frame regions by
//! FNV-1a against the generator's pins. What it does NOT do yet: decode
//! (PR 2's on-target checksum — the gate every perf number must pass), time
//! (PR 3), cadence (PR 4).
//!
//! Why hash-check the regions here at all: `include_bytes!` pins the bytes
//! into the image, and the same two FNV constants + loop are computed by the
//! Python generator. Recomputing them on target (and in the host witness)
//! proves the Python and Rust hash implementations agree AND that the bytes
//! in THIS image are the committed bytes — the exact precondition PR 3–5's
//! timings stand on ("no number is trusted from an image whose decode isn't
//! proven on its own embedded bytes").
//!
//! Screen verdict follows the BitReader PoC's convention (works on cart with
//! no cable): **blue = every pin matched, red = a mismatch, purple = the
//! checks never completed.**
//!
//! Host-build note: the entire ROM lives behind `cfg(target_arch = "arm")`.
//! `agb` is a target-gated dependency (Cargo.toml), so a host build of this
//! bin must not reference it — on host the bin is a no-op stub. Only the
//! thumbv4t ROM build (make native-spike-rom) compiles the real entry.

#![cfg_attr(target_arch = "arm", no_std)]
#![cfg_attr(target_arch = "arm", no_main)]

#[cfg(target_arch = "arm")]
mod rom {
    use agb::display::Rgb;
    use flac_spike::assets::{CLIPS, SpikeClip};
    use flac_spike::checksum::fnv1a64;

    const COLOR_PENDING: agb::display::Rgb15 = Rgb::new(128, 0, 255).to_rgb15();
    const COLOR_PASS: agb::display::Rgb15 = Rgb::new(0, 0, 255).to_rgb15();
    const COLOR_FAIL: agb::display::Rgb15 = Rgb::new(255, 0, 0).to_rgb15();

    /// Check one embedded arm: metadata sanity from the manifest, the seek
    /// table's structural invariants, and the region hash pin. Logs everything
    /// so mGBA's serial output is the human-readable record.
    fn check_clip(clip: &SpikeClip) -> bool {
        agb::println!(
            "[spike] arm={} frames={} region={}B samples={} rate={} bps={} ch={} max_blocksize={}",
            clip.name,
            clip.frames.len(),
            clip.region.len(),
            clip.total_samples,
            clip.sample_rate_hz,
            clip.bits_per_sample,
            clip.channels,
            clip.max_blocksize
        );
        agb::println!("[spike] census({}): {}", clip.name, clip.census);
        agb::println!("[spike] why:  {}", clip.why);

        let mut pass = true;

        // Seek-table structure (the GAFP stand-in): frame 0 at region start,
        // offsets strictly ascending, every blocksize within the manifest's
        // max, every offset inside the blob. A truncated or hand-edited blob
        // trips these before any decode.
        pass &= !clip.frames.is_empty();
        pass &= clip.frames.first().is_some_and(|f| f.offset == 0);
        let mut ascending = true;
        let mut within_max = true;
        let mut within_region = true;
        let mut prev = clip.frames.first().map_or(0, |f| f.offset);
        for frame in clip.frames.iter().skip(1) {
            ascending &= frame.offset > prev;
            prev = frame.offset;
        }
        for frame in clip.frames {
            within_max &= usize::from(frame.blocksize) <= usize::from(clip.max_blocksize);
            // 4 bytes is the smallest legal frame header (sync + fields + a
            // 1-octet coded number + CRC-8 minimum is 6, but this is a cheap
            // bounds sanity pin, not a parser — the parser is the real gate).
            // try_from, not from: core never implements From<u32> for usize.
            within_region &=
                usize::try_from(frame.offset).is_ok_and(|o| (o + 4) <= clip.region.len());
        }
        pass &= ascending & within_max & within_region;
        agb::println!(
            "[spike] {} seek table: ascending={} within_max={} within_region={}",
            clip.name,
            ascending,
            within_max,
            within_region
        );

        // The region pin: recompute over the embedded bytes and compare to the
        // generator's value. Cross-language hash agreement + image integrity
        // in one line.
        let actual = fnv1a64(clip.region);
        let ok = actual == clip.fnv_frames;
        pass &= ok;
        agb::println!(
            "[spike] {} region fnv: expected 0x{:016X} actual 0x{:016X} [{}]",
            clip.name,
            clip.fnv_frames,
            actual,
            if ok { "OK" } else { "MISMATCH" }
        );
        pass
    }

    #[agb::entry]
    fn main(mut gba: agb::Gba) -> ! {
        agb::eprintln!("[spike] entry started — perf-gate spike ROM (PR 1: scaffold + assets)");

        let mut gfx = gba.graphics.get();
        gfx.set_background_palette_colour(0, 0, COLOR_PENDING);

        let mut pass = true;
        for clip in CLIPS {
            pass &= check_clip(clip);
        }

        let (color, label) = if pass {
            (COLOR_PASS, "BLUE")
        } else {
            (COLOR_FAIL, "RED")
        };
        gfx.set_background_palette_colour(0, 0, color);
        agb::eprintln!(
            "[spike] asset verification {} — screen {}",
            if pass { "PASSED" } else { "FAILED" },
            label
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
