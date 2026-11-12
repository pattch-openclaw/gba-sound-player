//! The product integration ROM — `flac-lite`'s manifest facade, the overlay's
//! ARM kernel, and a timed decode, in one bootable image.
//!
//! STATUS (6b-1): this was a "does it link and boot" sanity build; it is now the
//! **product path**. It embeds a `GAFP` blob, parses its manifest, decodes every
//! frame through [`flac_lite::Decoder`] — with the overlay's `ArmRiceKernel` as
//! the kernel when the `arm-kernel` feature is on (the default) — folds the PCM
//! into FNV-1a 64, compares it against the perf gate's committed pin, and times
//! the same pass on the same bytes.
//!
//! Three things a reader should know:
//!
//! - **The blob is a stand-in for the packer.** `scripts/gen_gafp_blob.py`
//!   assembles a `GAFP` header + offset table over the perf gate's committed
//!   clip (`spike_l0_frames.bin`) using the spike's own fail-closed generated
//!   table. `scripts/pack_flac.sh` is still a stub; 6b-2's real packer must
//!   produce a **byte-identical** blob for the same frames, which is what turns
//!   the stand-in into a differential.
//! - **Correctness before speed, per pass.** The FNV pin is checked before the
//!   cycle numbers are read out, and the numbers come from the *same* pass that
//!   produced the pinned hash — no "the numbers are from a different decode".
//! - **The kernel is chosen by type**, at the facade call site: `ArmRiceKernel`
//!   under `arm-kernel`, the core's `TableCountKernel` without it. That is what
//!   makes `--no-default-features` a same-ROM A/B for the seam and the kernel
//!   rather than a different program.
//!
//! The BitReader proof from the scaffold is kept: it is the boot-time hardware
//! check that the bit-level primitives read what hand-computed expectations say,
//! and it costs ten bytes of ROM.

// The crate-level attributes and the module layout exist for one reason: this
// binary has to compile *past* the ARM-only parts on a host, not merely for ARM.
// `agb` cannot be built for any non-ARM target at all (its `critical-section`
// enables portable-atomic's `unsafe-assume-single-core`, which is a hard
// `compile_error!` off ARM), and Cargo.toml therefore declares it under
// `cfg(target_arch = "arm")` — see the note there. Everything that touches the
// console, the palettes and the timers lives inside `mod rom` behind the same cfg,
// and the host build gets a stub.
//
// That is not a convenience for its own sake: `make product-play-test` builds this
// crate's library for the host to run the playback model, and an un-gated `use
// agb::` anywhere in the crate makes that gate fail to compile rather than fail to
// run. `src/product_play_main.rs` already had this shape; this file now matches it
// so the two binaries of this crate cannot diverge on where the platform ends.
#![cfg_attr(target_arch = "arm", no_std)]
#![cfg_attr(target_arch = "arm", no_main)]

#[cfg(target_arch = "arm")]
extern crate alloc;

#[cfg(target_arch = "arm")]
mod gafp_pin;
#[cfg(target_arch = "arm")]
mod text;

#[cfg(target_arch = "arm")]
mod rom {
    use crate::{gafp_pin, text};

    use agb::display::Rgb;
    use flac_lite::bits::BitReader;
    use flac_lite::{DecodedFrame, Decoder, FrameBuffers, Manifest};

    /// The packed clip this ROM decodes: real libFLAC frames behind a generated
    /// `GAFP` header (see the module docs).
    static SONG: &[u8] = include_bytes!("../assets/spike_l0.gfp");

    /// The 32,768 Hz / 2048-sample frame budget: `2048 / 32768 s × 16.78 MHz`.
    /// Derived at compile time, never recalled — the same number the perf gate's
    /// `stats::real_time_budget(2048, 32_768)` produces.
    const BUDGET_CYCLES_PER_FRAME: u32 = (2048u64 * 16_780_000 / 32_768) as u32;

    const COLOR_PENDING: agb::display::Rgb15 = Rgb::new(128, 0, 255).to_rgb15(); // purple = running
    const COLOR_PASS: agb::display::Rgb15 = Rgb::new(0, 0, 255).to_rgb15(); // blue = every proof passed
    const COLOR_FAIL: agb::display::Rgb15 = Rgb::new(255, 0, 0).to_rgb15(); // red = something failed

    /// Hard-coded test vector: the exact bytes the BitReader proof reads back on
    /// hardware. Lives in `.text` (the ROM image), so real hardware reads the same
    /// bytes the emulator does — no host-side state involved.
    ///
    /// Chosen so the representative field widths (fixed-width FLAC header-style
    /// fields + a UTF-8-style prefix + an unaligned 16-bit window crossing byte
    /// boundaries) land on awkward bit offsets, and the expectations below were
    /// computed by hand from the bit layout — deliberately NOT by reusing the
    /// library code under test.
    const BIT_PROOF_BYTES: [u8; 10] = [0x66, 0x4C, 0x61, 0x43, 0x7A, 0xE1, 0xB5, 0x2C, 0xDE, 0xAD];

    /// One checked field read: (label, bit width, hand-computed expected value).
    const BIT_PROOF_FIELDS: [(&str, u32, u32); 10] = [
        ("sync_like_14", 14, 0x1993),
        ("reserved_1", 1, 0),
        ("blocking_1", 1, 0),
        ("sample_size_4", 4, 0x6),
        ("sample_rate_4", 4, 0x1),
        ("framing_1", 1, 0),
        ("channel_8", 8, 0x86),
        ("utf8_like_13", 13, 0x1EB8),
        ("window_16", 16, 0x6D4B),
        ("crc_like_8", 8, 0x37),
    ];

    /// Free-running cycle counter: timer2 at /1 (1 tick = 1 CPU cycle at 16.78 MHz)
    /// cascading into timer3, read as a 32-bit composite. The perf gate's mechanism,
    /// reused rather than reinvented — it is documented hardware behaviour driven
    /// entirely through agb's safe API.
    struct Counter<'a> {
        low: &'a mut agb::timer::Timer,
        high: &'a mut agb::timer::Timer,
    }

    impl Counter<'_> {
        fn start<'a>(timers: &'a mut agb::timer::Timers<'_>) -> Counter<'a> {
            timers
                .timer2
                .set_divider(agb::timer::Divider::Divider1)
                .set_enabled(true);
            timers
                .timer3
                .set_cascade(true)
                .set_interrupt(false)
                .set_enabled(true);
            let mut counter = Counter {
                low: &mut timers.timer2,
                high: &mut timers.timer3,
            };
            counter.soft_reset();
            counter
        }

        fn soft_reset(&mut self) {
            self.low.set_enabled(false);
            self.high.set_enabled(false);
            self.high.set_overflow_amount(0);
            self.low.set_overflow_amount(0);
            self.low.set_enabled(true);
            self.high.set_enabled(true);
        }

        fn read(&self) -> u32 {
            let mut high;
            let mut low;
            for _ in 0..4 {
                let low1 = self.low.value();
                high = self.high.value();
                low = self.low.value();
                if low >= low1 {
                    return (u32::from(high) << 16) | u32::from(low);
                }
            }
            high = self.high.value();
            low = self.low.value();
            (u32::from(high) << 16) | u32::from(low)
        }
    }

    /// FNV-1a 64 over the clip's interleaved i16 little-endian PCM — byte for byte
    /// the fold `flac_spike::checksum::fold_i16le_stereo` applies (same offset
    /// basis, same prime, `sample as i16`, left then right per sample), because the
    /// pin this is compared against was produced by that code.
    struct Fnv1a64 {
        hash: u64,
    }

    impl Fnv1a64 {
        /// The **house** offset basis, not the standard FNV one. This repository's
        /// pins (`flac_spike::assets::*::fnv_pcm`, `scripts/frame_vectors.py`'s
        /// `fnv1a64`) all use `0xCBF43CE95DE684B7`; the standard `0xCBF29CE484222325`
        /// produces a different value for the same bytes. Matching the pins is the
        /// whole job here, so the constant is theirs — and it is written down rather
        /// than copied silently, because a reader who knows FNV will notice.
        const OFFSET_BASIS: u64 = 0xCBF4_3CE9_5DE6_84B7;
        const PRIME: u64 = 0x0000_0100_0000_01b3;

        const fn new() -> Self {
            Self {
                hash: Self::OFFSET_BASIS,
            }
        }

        fn update(&mut self, bytes: &[u8]) {
            for &byte in bytes {
                self.hash ^= u64::from(byte);
                self.hash = self.hash.wrapping_mul(Self::PRIME);
            }
        }

        fn sample(&mut self, sample: i32) {
            self.update(&(sample as i16).to_le_bytes());
        }

        fn finish(&self) -> u64 {
            self.hash
        }
    }

    /// Write the hard-coded byte sequence to the serial log, hex, LSB-first.
    fn log_proof_bytes() {
        agb::println!("[flac-rom] BitReader proof: hard-coded ROM bytes (LSB..MSB):");
        for (i, b) in BIT_PROOF_BYTES.iter().enumerate() {
            agb::println!("[flac-rom]   byte[{}] = 0x{:02X}", i, b);
        }
    }

    /// The boot-time BitReader proof: every field read exactly as hand-computed, the
    /// cursor finishes on the hand-computed boundary, and the tail reads then EOFs.
    fn bitreader_proof() -> bool {
        log_proof_bytes();

        let mut reader = BitReader::new(&BIT_PROOF_BYTES);
        let mut pass = true;
        for (label, width, expected) in BIT_PROOF_FIELDS {
            match reader.read_bits(width) {
                Ok(actual) => {
                    let ok = actual == expected;
                    pass &= ok;
                    agb::println!(
                        "[flac-rom] {} ({} bits): expected 0x{:X} actual 0x{:X} [{}]",
                        label,
                        width,
                        expected,
                        actual,
                        if ok { "OK" } else { "MISMATCH" }
                    );
                }
                Err(e) => {
                    pass = false;
                    agb::println!(
                        "[flac-rom] {} ({} bits): expected 0x{:X} actual <error {:?}> [MISMATCH]",
                        label,
                        width,
                        expected,
                        e
                    );
                }
            }
        }

        const EXPECTED_END_BITS: usize = 70;
        let pos = reader.bit_position();
        let rem = reader.bits_remaining();
        const EXPECTED_REMAINING: usize = (BIT_PROOF_BYTES.len() * 8) - EXPECTED_END_BITS;
        let pos_ok = pos == EXPECTED_END_BITS && rem == EXPECTED_REMAINING;
        pass &= pos_ok;
        agb::println!(
            "[flac-rom] cursor: expected pos={} remaining={} actual pos={} remaining={} [{}]",
            EXPECTED_END_BITS,
            EXPECTED_REMAINING,
            pos,
            rem,
            if pos_ok { "OK" } else { "MISMATCH" }
        );

        match reader.read_bits(10) {
            Ok(actual) => {
                const EXPECTED_TAIL: u32 = 0x2AD;
                let ok = actual == EXPECTED_TAIL;
                pass &= ok;
                agb::println!(
                    "[flac-rom] tail_10 (10 bits): expected 0x{:X} actual 0x{:X} [{}]",
                    EXPECTED_TAIL,
                    actual,
                    if ok { "OK" } else { "MISMATCH" }
                );
            }
            Err(e) => {
                pass = false;
                agb::println!(
                    "[flac-rom] tail_10: expected 0x2AD actual <error {:?}> [MISMATCH]",
                    e
                );
            }
        }

        let eof_ok = reader.read_bits(1).is_err();
        pass &= eof_ok;
        agb::println!(
            "[flac-rom] read at EOF: expected Err actual {} [{}]",
            if eof_ok { "Err" } else { "Ok(?!)" },
            if eof_ok { "OK" } else { "MISMATCH" }
        );

        pass
    }

    /// One frame through the kernel this build names.
    ///
    /// The two shapes are identical apart from the type argument, which is the
    /// point: the seam is a delegation at the call site, not a different loop.
    #[cfg(feature = "arm-kernel")]
    fn decode_one<'b>(
        decoder: &mut Decoder<'_>,
        bufs: &'b mut FrameBuffers<'_>,
    ) -> flac_lite::Result<Option<DecodedFrame<'b>>> {
        decoder.decode_next_with::<flac_lite_gba::rice_arm::ArmRiceKernel>(bufs)
    }

    #[cfg(not(feature = "arm-kernel"))]
    fn decode_one<'b>(
        decoder: &mut Decoder<'_>,
        bufs: &'b mut FrameBuffers<'_>,
    ) -> flac_lite::Result<Option<DecodedFrame<'b>>> {
        decoder.decode_next(bufs)
    }

    /// What the correctness pass produced.
    struct Proof {
        frames: usize,
        samples: u64,
        pcm_fnv: u64,
    }

    /// Parse the manifest and check it against the generated pins and the encode
    /// profile. Shared by both passes so they cannot disagree about what they are
    /// decoding.
    fn manifest() -> Option<Manifest<'static>> {
        let manifest = match Manifest::parse(SONG) {
            Ok(m) => m,
            Err(e) => {
                agb::println!("[flac-rom] FAIL manifest: {:?}", e);
                return None;
            }
        };
        agb::println!(
            "[flac-rom] manifest: frames={} samples={} rate={} ch={} bps={} block={} max_fixed_order={}",
            manifest.frame_count(),
            manifest.total_samples(),
            manifest.sample_rate_hz(),
            manifest.channels(),
            manifest.bits_per_sample(),
            manifest.blocksize(),
            manifest.max_fixed_order()
        );
        if manifest.frame_count() != gafp_pin::FRAMES
            || manifest.total_samples() != gafp_pin::SAMPLES
            || manifest.max_fixed_order() != gafp_pin::MAX_FIXED_ORDER
        {
            agb::println!("[flac-rom] FAIL manifest disagrees with the generated pin");
            return None;
        }
        if flac_lite::format::DEFAULT_PROFILE
            .validate(&manifest)
            .is_err()
        {
            agb::println!("[flac-rom] FAIL manifest outside the encode profile");
            return None;
        }
        Some(manifest)
    }

    /// Pass 1 — correctness: decode every frame, fold the PCM, compare the pin.
    ///
    /// Deliberately **not** timed. The first version of this ROM timed this pass and
    /// reported 1.95 M c/frame for a kernel the instrument measures at 0.42 M: the
    /// window was pricing the FNV fold (a per-sample function call over 4096 samples
    /// per frame) alongside the decode. Correctness and speed are two passes for the
    /// same reason the perf gate separates them — a number must not include the
    /// instrument.
    fn proof_pass() -> Option<Proof> {
        let manifest = manifest()?;
        let mut decoder = Decoder::new(&manifest).ok()?;
        let capacity = FrameBuffers::required_len(&manifest);
        let mut left = [0i32; 2048];
        let mut right = [0i32; 2048];
        if capacity > left.len() {
            agb::println!(
                "[flac-rom] FAIL block size {} exceeds the static buffers",
                capacity
            );
            return None;
        }
        let mut bufs = FrameBuffers::new(&mut left, &mut right);

        let mut hash = Fnv1a64::new();
        let mut frames = 0usize;
        let mut samples = 0u64;
        loop {
            match decode_one(&mut decoder, &mut bufs) {
                Ok(Some(frame)) => {
                    let written = frame.left.len();
                    for i in 0..written {
                        hash.sample(frame.left[i]);
                        if manifest.channels() == 2 {
                            hash.sample(frame.right[i]);
                        }
                    }
                    frames += 1;
                    samples += written as u64;
                }
                Ok(None) => break,
                Err(e) => {
                    agb::println!("[flac-rom] FAIL decode: {:?}", e);
                    return None;
                }
            }
        }
        let pcm_fnv = hash.finish();
        agb::println!(
            "[flac-rom] proof: frames={} samples={} pcm_fnv=0x{:016X} expected=0x{:016X}",
            frames,
            samples,
            pcm_fnv,
            gafp_pin::FNV_PCM
        );
        if frames != gafp_pin::FRAMES
            || samples != gafp_pin::SAMPLES
            || pcm_fnv != gafp_pin::FNV_PCM
        {
            agb::println!("[flac-rom] FAIL the pass disagrees with its pins");
            return None;
        }
        agb::println!("[flac-rom] decode proof PASSED (pin matched on these bytes)");
        Some(Proof {
            frames,
            samples,
            pcm_fnv,
        })
    }

    /// Pass 2 — the timed reading: one counter window per frame, **around the decode
    /// call only**. Nothing else runs inside a window (no hashing, no logging), which
    /// is the shape the perf gate uses, so the numbers are comparable to its record.
    fn timed_pass(counter: &mut Counter<'_>) -> Option<(u64, u32, usize)> {
        let manifest = manifest()?;
        let mut decoder = Decoder::new(&manifest).ok()?;
        let capacity = FrameBuffers::required_len(&manifest);
        let mut left = [0i32; 2048];
        let mut right = [0i32; 2048];
        if capacity > left.len() {
            return None;
        }
        let mut bufs = FrameBuffers::new(&mut left, &mut right);

        let mut sum = 0u64;
        let mut worst = 0u32;
        let mut frames = 0usize;
        let mut failed = false;

        // IRQs off for the whole pass (the calibration is not re-derived here: the
        // window is per frame, the counter is free-running, and the companion
        // `stats::cycles_between` saturating rule is not needed at these lengths).
        agb::external::critical_section::with(|_cs| {
            counter.soft_reset();
            loop {
                let began = counter.read();
                let outcome = decode_one(&mut decoder, &mut bufs);
                let took = counter.read().wrapping_sub(began);
                match outcome {
                    Ok(Some(frame)) => {
                        let _ = frame;
                        sum += u64::from(took);
                        worst = worst.max(took);
                        frames += 1;
                    }
                    Ok(None) => break,
                    Err(_e) => {
                        failed = true;
                        break;
                    }
                }
            }
        });

        if failed || frames == 0 {
            return None;
        }
        Some((sum, worst, frames))
    }

    /// Cycles as a percentage of the frame budget, scaled by 1000 (`39961` is
    /// "39.961 %") — integer arithmetic, no float in a `no_std` ROM.
    fn pct_x1000_u64(cycles: u64) -> u64 {
        cycles * 100_000 / u64::from(BUDGET_CYCLES_PER_FRAME)
    }

    #[agb::entry]
    fn main(mut gba: agb::Gba) -> ! {
        agb::eprintln!("[flac-rom] entry started — product integration ROM");

        let mut gfx = gba.graphics.get();
        gfx.set_background_palette_colour(0, 0, COLOR_PENDING);
        agb::eprintln!("[flac-rom] decoding starts now; everything is logged when the pass ends");

        // Note the kernel this image was built with: it is the same ROM source for
        // both arms of the A/B, so the log has to say which one ran.
        agb::println!(
            "[flac-rom] kernel: {}",
            if cfg!(feature = "arm-kernel") {
                "rice-arm (flac-lite-gba ArmRiceKernel)"
            } else {
                "TableCountKernel (core default)"
            }
        );
        agb::println!("[flac-rom] blob: {} bytes", SONG.len());
        agb::println!(
            "[flac-rom] budget: {} cycles/frame (2048 @ 32768 Hz)",
            BUDGET_CYCLES_PER_FRAME
        );

        let bit_pass = bitreader_proof();

        let proof = proof_pass();
        let mut pass = bit_pass && proof.is_some();
        let mut reading: Option<(u64, u64, u64, usize)> = None; // mean, worst, frames

        if let Some(proof) = &proof {
            let mut timers = gba.timers.timers();
            let mut counter = Counter::start(&mut timers);
            match timed_pass(&mut counter) {
                Some((sum, worst, frames)) => {
                    let mean = sum / frames as u64;
                    let mean_x1000 = pct_x1000_u64(mean);
                    let worst_x1000 = pct_x1000_u64(u64::from(worst));
                    reading = Some((mean, u64::from(worst), u64::from(mean_x1000), frames));
                    agb::println!(
                        "[flac-rom] timing: frames={} total={} cycles mean={} c/frame worst={} c/frame",
                        frames,
                        sum,
                        mean,
                        worst
                    );
                    agb::println!(
                        "[flac-rom] budget: mean {}.{:03}% worst {}.{:03}% of a 32768 Hz frame",
                        mean_x1000 / 1000,
                        mean_x1000 % 1000,
                        worst_x1000 / 1000,
                        worst_x1000 % 1000
                    );
                    agb::println!(
                        "[flac-rom] proof kept: {} frames, {} samples, pcm 0x{:016X}",
                        proof.frames,
                        proof.samples,
                        proof.pcm_fnv
                    );
                }
                None => {
                    pass = false;
                    agb::println!("[flac-rom] FAIL timed pass — numbers void");
                }
            }
        }

        let (color, label) = if pass {
            (COLOR_PASS, "BLUE")
        } else {
            (COLOR_FAIL, "RED")
        };
        gfx.set_background_palette_colour(0, 0, color);
        agb::eprintln!(
            "[flac-rom] verdict {} — screen {} (bitreader={}, decode={})",
            if pass { "PASSED" } else { "FAILED" },
            label,
            bit_pass,
            proof.is_some()
        );

        // The numbers, on the screen: serial exists on emulators, not on a stock
        // unit. Same string the log carries, so the two channels cannot disagree.
        use alloc::format;
        use alloc::string::String;
        let mut screen = String::new();
        screen.push_str("FLAC INTEGRATION\n");
        screen.push_str(&format!(
            "K {}\n",
            if cfg!(feature = "arm-kernel") {
                "rice-arm"
            } else {
                "default"
            }
        ));
        match reading {
            Some((mean, worst, mean_x1000, frames)) => {
                screen.push_str(&format!(
                    "mean {} {}.{:03}%\n",
                    mean,
                    mean_x1000 / 1000,
                    mean_x1000 % 1000
                ));
                screen.push_str(&format!(
                    "worst {} {}.{:03}%\n",
                    worst,
                    pct_x1000_u64(worst) / 1000,
                    pct_x1000_u64(worst) % 1000
                ));
                screen.push_str(&format!("{} frames\n", frames));
            }
            None => screen.push_str("timing VOID\n"),
        }
        screen.push_str(if pass { "PASS" } else { "FAIL" });
        for line in screen.lines() {
            agb::println!("[flac-rom] screen| {}", line);
        }
        let fit = text::measure(&screen);
        agb::println!(
            "[flac-rom] fit: lines={} right={} bottom={} fits={}",
            fit.lines,
            fit.right,
            fit.bottom,
            fit.fits()
        );
        text::show(&mut gfx, &screen, color);

        // Present nothing after `show`: a commit from a fresh frame disables the
        // background it just painted (6a-7b's hardware finding).
        loop {}
    }
}

/// The host arm of this binary: nothing to run, because the thing it does is drive
/// sound hardware. The host-facing behaviour of this crate is its library and the
/// model in `tests/`, not this main.
#[cfg(not(target_arch = "arm"))]
fn main() {}
