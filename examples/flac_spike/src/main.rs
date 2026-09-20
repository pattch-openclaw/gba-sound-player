//! Perf-gate spike ROM — PR 2: on-target decode correctness.
//!
//! PURPOSE (see ../../../FLAC.md → "Perf gate spike — concrete plan and
//! validation process"): this ROM is the measurement vehicle for the go/no-go
//! perf gate. PR 1 landed the vehicle (boot + embedded region hashes); PR 2 —
//! this — adds the **on-target decode proof**: every frame of both embedded
//! clips is decoded on the target, the decoded PCM folded into FNV-1a-64 as
//! it goes, and the final hash compared against the generator's `fnv_pcm`
//! pins (host-computed from `flac -d` output). What it still does NOT do:
//! time (PR 3), cadence (PR 4).
//!
//! Why the fold is the proof: the ROM decodes its **own embedded bytes** and
//! hashes the result, so correctness is established on the exact image PRs
//! 3–5 will measure — the plan's rule that no perf number is ever reported
//! from an image whose decode is not proven on its own bytes (a fast wrong
//! decode measures nothing). The fold itself is `checksum::fold_i16le_stereo`,
//! a library function the host witness (`make spike-test`) drives through the
//! same `driver::decode_clip` and asserts against the same pins — host and
//! ROM run literally the same fold code, so they cannot share a
//! transcription mistake of the interleave rule.
//!
//! The checks run in strict order, each a named serial checkpoint:
//! 1. asset verification (PR 1): seek-table invariants + region hash pins —
//!    the bytes about to be decoded are the committed bytes;
//! 2. per-arm decode proof: full-clip walk via the driver, per-frame serial
//!    log with the running hash (a mismatch localizes to the first frame
//!    whose running hash diverges from the host's same-sequence dump), then
//!    final-hash-vs-pin;
//! 3. cross-arm agreement: both arms decode the same source, so their
//!    on-target PCM hashes must match each other as well as their pins.
//!
//! Screen verdict follows the BitReader PoC's convention (works on cart with
//! no cable): **blue = every check passed, red = any failed, purple = the
//! checks never completed.** BLUE now means *decode proven on-target* —
//! strictly stronger than PR 1's asset-integrity blue.
//!
//! Host-build note: the entire ROM lives behind `cfg(target_arch = "arm")`.
//! `agb` is a target-gated dependency (Cargo.toml), so a host build of this
//! bin must not reference it — on host the bin is a no-op stub. Only the
//! thumbv4t ROM build (make native-spike-rom) compiles the real entry.

#![cfg_attr(target_arch = "arm", no_std)]
#![cfg_attr(target_arch = "arm", no_main)]

#[cfg(target_arch = "arm")]
extern crate alloc;

#[cfg(target_arch = "arm")]
mod rom {
    use agb::display::Rgb;
    use flac_lite::subframe::PredictorState;
    use flac_spike::assets::{CLIPS, SpikeClip};
    use flac_spike::checksum::{Fnv1a64, fnv1a64, fold_i16le_stereo};
    use flac_spike::driver::{self, DriverError};

    const COLOR_PENDING: agb::display::Rgb15 = Rgb::new(128, 0, 255).to_rgb15();
    const COLOR_PASS: agb::display::Rgb15 = Rgb::new(0, 0, 255).to_rgb15();
    const COLOR_FAIL: agb::display::Rgb15 = Rgb::new(255, 0, 0).to_rgb15();

    /// PR 1 checkpoint: metadata sanity from the manifest, the seek table's
    /// structural invariants, and the region hash pin. Logs everything so
    /// mGBA's serial output is the human-readable record.
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

    /// PR 2 checkpoint: the on-target decode proof for one arm.
    ///
    /// Decodes every frame of the clip via `driver::decode_clip` (the same
    /// seek-table path the host witness walks), folds each decoded block into
    /// a running FNV-1a-64 through the shared `fold_i16le_stereo`, and
    /// returns the final hash plus decode stats — or `None` if the walk
    /// failed, with the failure mode named on serial before returning.
    ///
    /// Logs are the deliverable as much as the boolean: per-frame lines carry
    /// the running hash so a mismatch localizes to the first frame that
    /// diverged (compare against the host dump of the same fold sequence —
    /// `tests/spike_witness.rs::rom_fold_reproduces_pcm_pins_on_host` runs
    /// the identical fold on identical bytes), and every rejection path
    /// prints its variant, frame index, and evidence.
    fn decode_proof(clip: &SpikeClip) -> Option<(&'static str, u64, usize)> {
        let name = clip.name;
        let max = usize::from(clip.max_blocksize);
        // Decode scratch: one i32 buffer per channel at the track's max
        // blocksize (2048 × 4B = 8KB each; EWRAM has room), reused across
        // frames exactly as playback will reuse them. The tail beyond a
        // frame's own blocksize is load-bearing (the final frame is
        // legitimately short) — the flac-lite buffer contract keeps it
        // untouched and the fold never sees it (windows are sliced to
        // `written` by the driver callback).
        let mut left: alloc::vec::Vec<i32> = alloc::vec![0; max];
        let mut right: alloc::vec::Vec<i32> = alloc::vec![0; max];
        // EWRAM placement is printed as plain hex addresses: the allocator is
        // agb's EWRAM block heap, so these numbers also witness which region
        // the decode scratch actually lives in (PR 4's buffer rotation reads
        // the same addresses; a {:p} would be an opaque placeholder in core).
        agb::println!(
            "[spike] {} checkpoint: decode buffers ready ({} samples × {}B each; left=0x{:08X} right=0x{:08X})",
            name,
            max,
            core::mem::size_of::<i32>(),
            left.as_ptr() as usize,
            right.as_ptr() as usize
        );

        let mut state = [PredictorState::new(), PredictorState::new()];
        let mut hash = Fnv1a64::new();
        // The fold can fail on a desynced decode (a sample outside the
        // 16-bit profile). Record the FIRST offender without aborting the
        // walk: the per-frame log then shows exactly where the running hash
        // starts diverging, and the final-hash comparison below still gets
        // computed as an independent witness of the corruption.
        let mut fold_failure: Option<(usize, i32)> = None;

        agb::println!(
            "[spike] {} decode proof: walking {} frames via seek table",
            name,
            clip.frames.len()
        );
        let walk = driver::decode_clip(clip, &mut left, &mut right, &mut state, |frame, l, r| {
            match fold_i16le_stereo(&mut hash, l, r) {
                Ok(()) => agb::println!(
                    "[spike] {} frame {:>3}/{} off={} bs={} running=0x{:016X}",
                    name,
                    frame,
                    clip.frames.len(),
                    usize::try_from(clip.frames[frame].offset).unwrap_or(usize::MAX),
                    l.len(),
                    hash.finish()
                ),
                Err(sample) => {
                    if fold_failure.is_none() {
                        fold_failure = Some((frame, sample));
                    }
                    agb::println!(
                        "[spike] {} FAIL frame {:>3}: decoded sample {} outside 16-bit profile \
                         (decode desync — hash not folded, walk continues for localization)",
                        name,
                        frame,
                        sample
                    );
                }
            }
        });

        let stats = match walk {
            Ok(stats) => stats,
            Err(err) => {
                // Every DriverError variant names its frame and evidence;
                // this is the PR 2 failure mode for "the decoder or the
                // assets regressed inside this image".
                let described = match &err {
                    DriverError::BufferTooSmall { need, have } => {
                        alloc::format!("buffer too small (need {} have {})", need, have)
                    }
                    DriverError::FrameOffsetOutOfRange { frame, offset, len } => {
                        alloc::format!(
                            "frame {} offset {} past region len {} (table/blob mismatch)",
                            frame,
                            offset,
                            len
                        )
                    }
                    DriverError::MetadataMismatch {
                        frame,
                        table,
                        header,
                    } => {
                        alloc::format!(
                            "frame {} blocksize table={} header={} (asset corruption)",
                            frame,
                            table,
                            header
                        )
                    }
                    DriverError::Decode(e) => {
                        // flac_lite::Error is Debug; {:?} names the variant.
                        alloc::format!("flac-lite rejected the stream: {:?}", e)
                    }
                };
                agb::println!("[spike] {} FAIL decode walk aborted: {}", name, described);
                return None;
            }
        };

        agb::println!(
            "[spike] {} checkpoint: walk complete frames={}/{} samples={}",
            name,
            stats.frames,
            clip.frames.len(),
            stats.samples
        );

        let mut pass = true;

        // Cross-check the walk totals against the manifest: a frame silently
        // skipped would keep the loop "green" while hashing short PCM.
        let expected_samples = usize::try_from(clip.total_samples).unwrap_or(usize::MAX);
        let samples_ok = stats.samples == expected_samples && stats.frames == clip.frames.len();
        pass &= samples_ok;
        agb::println!(
            "[spike] {} totals: frames={} (want {}) samples={} (want {}) [{}]",
            name,
            stats.frames,
            clip.frames.len(),
            stats.samples,
            expected_samples,
            if samples_ok { "OK" } else { "MISMATCH" }
        );

        if let Some((frame, sample)) = fold_failure {
            agb::println!(
                "[spike] {} FAIL first out-of-range sample {} at frame {}",
                name,
                sample,
                frame
            );
            pass = false;
        }

        // The pin: on-target PCM hash vs the generator's host-computed
        // fnv_pcm (reference `flac -d` bytes). This line is PR 2's gate.
        let actual = hash.finish();
        let pin_ok = actual == clip.fnv_pcm;
        pass &= pin_ok;
        agb::println!(
            "[spike] {} pcm fnv: expected 0x{:016X} actual 0x{:016X} [{}]",
            name,
            clip.fnv_pcm,
            actual,
            if pin_ok { "MATCH" } else { "MISMATCH" }
        );

        if pass {
            Some((name, actual, stats.samples))
        } else {
            None
        }
    }

    #[agb::entry]
    fn main(mut gba: agb::Gba) -> ! {
        agb::eprintln!(
            "[spike] entry started — perf-gate spike ROM (PR 2: on-target decode correctness)"
        );

        let mut gfx = gba.graphics.get();
        gfx.set_background_palette_colour(0, 0, COLOR_PENDING);

        // Checkpoint 1 — asset verification (PR 1's gate): the bytes about to
        // be decoded are the committed generator output.
        let mut pass = true;
        for clip in CLIPS {
            pass &= check_clip(clip);
        }
        agb::println!(
            "[spike] checkpoint: asset verification {} (region pins — decode below runs on these bytes)",
            if pass { "PASSED" } else { "FAILED" }
        );
        // A failed region pin means the image drifted from its pins; decoding
        // drifted bytes could still "succeed" against nothing, so the decode
        // proof only carries meaning when the assets passed. Run it anyway —
        // its logs are diagnostic — but the verdict is already red.
        let assets_pass = pass;

        // Checkpoint 2 — the on-target decode proof, per arm.
        let mut proofs: [Option<(&'static str, u64, usize)>; 2] = [None, None];
        for (slot, clip) in CLIPS.iter().enumerate() {
            proofs[slot] = decode_proof(clip);
            pass &= proofs[slot].is_some();
        }

        // Checkpoint 3 — cross-arm agreement: both arms encode the same
        // source, so their on-target PCM hashes must match each other. Two
        // pins matching independently already implies it; the explicit line
        // witnesses that neither arm decoded into the other's buffers.
        if let (Some((_, h0, _)), Some((_, h1, _))) = (proofs[0], proofs[1]) {
            let agree = h0 == h1;
            pass &= agree;
            agb::println!(
                "[spike] cross-arm pcm: l0=0x{:016X} l4=0x{:016X} agree={} [{}]",
                h0,
                h1,
                agree,
                if agree { "OK" } else { "MISMATCH" }
            );
        }

        let (color, label) = if pass {
            (COLOR_PASS, "BLUE")
        } else {
            (COLOR_FAIL, "RED")
        };
        gfx.set_background_palette_colour(0, 0, color);
        agb::eprintln!(
            "[spike] PR 2 verdict {} — screen {} (assets={}, decode+pcm pins={})",
            if pass { "PASSED" } else { "FAILED" },
            label,
            assets_pass,
            proofs.iter().all(Option::is_some)
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
