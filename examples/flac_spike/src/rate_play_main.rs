//! Rate-play ROM — FLAC decode **into audible playback** at a mixer-domain
//! rate (FLAC.md "Sample-rate reduction" proposal; the playback baseline
//! Sam scoped 2026-10-18: "can we even play back audio at a reduced
//! sampling rate?" — the end-to-end sanity check every later perf/rate
//! claim is judged against).
//!
//! Route (Sam's pick 2026-10-18, over stock-agb-mixer and chunked-play):
//! a **custom IRQ feeder** on the hardware agb's own mixer drives —
//! timer0 overflows once per sample (2^24/rate ticks), timer1 cascades
//! and interrupts once per mixer-domain buffer (CHUNK samples), the
//! handler copies the next ring chunk into a ping-pong pair and re-arms
//! DMA1/DMA2 into FIFO A/B (repeat, fixed destination, FIFO start — agb
//! `hw.rs`'s exact control word). No CPU mixing: the decode burst and the
//! audio drain never contend except for the ring push (µs, IRQs briefly
//! off) — the gapless property stock agb lacks at current decode speeds.
//! Deliberately NOT `agb::Mixer`: the mixer would own timer0/1 and pay
//! per-sample mixing a single stream does not need. This is the shape
//! Phase 2 step 9's "mixer/DMA double-buffer playback" line describes,
//! built inside the rate flow under the coexistence contract.
//!
//! WHY THIS CAN BE GAPLESS (arithmetic, not hope — the FIXED arm's
//! measured numbers at 10,512 Hz, FLAC.md rate ROM entry): worst frame
//! 2,559,507 cycles = 152.5 ms = 1,603 samples drained *during* one
//! decode. Prefill 16 chunks (2,816 samples) puts the lead above that
//! drain before the first post-prefill decode; the producer only starts a
//! frame when free ≥ blocksize, so ready never exceeds ring−2048 and the
//! floor after any burst is (16·176 − 2,048) − 1,603 + 2,048 > 0.
//! Underflows are still counted, never assumed away — the verdict demands
//! zero. (18,157 Hz is the other story: there the drain per worst-frame
//! decode — 2,559,507 c × 18,157 / 2^24 ≈ 2,769 samples — exceeds the
//! 2,048 a frame produces, and the mean drain ≈ 2,616 does too, so the
//! producer loses samples faster than it makes them and NO prefill depth
//! rescues it. Extrapolating the FIXED arm's measured 1,180 c/sample to
//! 18,157 puts mean decode at ≈ 128 % of that frame budget (1,892,369 c/f
//! derived) — at that rate the decoder, not the feeder, is what fails
//! first. The FLAC.md playback entry derives both.)
//!
//! CORRECTNESS-BEFORE-SPEED (unchanged layering): a decode proof on the
//! embedded bytes vs the generator's `fnv_pcm` pin runs BEFORE the sound
//! hardware enables. Then the playback pass decodes the whole track again
//! while feeding, folding every byte it hands the DMA into two FIFO8
//! hashes — judged against the generator's pins (derived from `flac -d`
//! reference PCM by the >>8 truncation agb itself applies;
//! `playback::fold_fifo8` is host-witnessed, generator-vs-library
//! differential). The bytes handed to the sound hardware's DMA registers
//! are therefore proven equal to the reference decode, on the measured
//! image — the pins fold at push time, so the last hop (DMA actually
//! feeding the paired FIFO) is witnessed by the audible check, not the
//! hash. Headless runs have no audio device and ignore DMA scheduling
//! warnings: routing bugs pass blind there (Sam's ears caught one,
//! 2026-10-19 — see the register-map note below).
//!
//! SCOPE CUTS (deliberate): only the FIXED (`-l 0`) arm plays — the LPC
//! arm measured 183.9% mean / 211.6% worst of its own budget at 10,512 Hz
//! (rate ROM entry); no feeder design fixes a 2× deficit. No mixer
//! mixing, no volume/pan, no seek, no loop, no input-wait (boot-to-audio;
//! the serial log is the record). This ROM answers the baseline question;
//! cost measurement of *this path* stays with the perf menu — the pass
//! here logs lead/underflows, without IRQ-off window discipline, so its
//! numbers are system cost, never comparable to the E-menu's windows.

#![cfg_attr(target_arch = "arm", no_std)]
#![cfg_attr(target_arch = "arm", no_main)]

#[cfg(target_arch = "arm")]
extern crate alloc;

#[cfg(target_arch = "arm")]
mod rom {
    use agb::display::Rgb;
    use agb::external::critical_section::{self, Mutex};
    use agb::interrupt::{Interrupt, add_interrupt_handler};
    use agb::timer::Divider;
    use core::cell::RefCell;

    use flac_lite::subframe::PredictorState;
    use flac_spike::checksum::{Fnv1a64, fnv1a64, fold_i16le_stereo};
    use flac_spike::driver;
    use flac_spike::playback::{Chunk, RingFeed};

    // Generated, temporary module — the same one the rate ROM and its host
    // witness include (make rate-assets).
    pub mod rate {
        include!("rate_assets.rs");
    }

    const COLOR_PASS: agb::display::Rgb15 = Rgb::new(0, 0, 255).to_rgb15();
    const COLOR_FAIL: agb::display::Rgb15 = Rgb::new(255, 0, 0).to_rgb15();
    const COLOR_PENDING: agb::display::Rgb15 = Rgb::new(128, 0, 255).to_rgb15();

    /// agb's own mixer tick clock: the GBA timer clock is 2^24 Hz.
    const MIXER_TICK_HZ: u32 = 16_777_216;

    /// Mixer-domain hardware-buffer sizes: the three rates carry
    /// 176/304/560 samples per buffer (agb 0.25.0 `Frequency::buffer_size`
    /// — the measured readiness fact in FLAC.md's rate proposal).
    const fn buffer_size(rate_hz: u32) -> usize {
        match rate_hz {
            10_512 => 176,
            18_157 => 304,
            32_768 => 560,
            // The generator fail-closes outside the mixer domain; this
            // const fn is only ever instantiated at the pinned rate.
            _ => 560,
        }
    }

    const CHUNK: usize = buffer_size(rate::RATE_HZ);
    /// Ring depth in chunks. 36 chunks at 10,512 Hz = 6,336 samples
    /// ≈ 603 ms; both channels = 12,672 B IWRAM-class (BSS → EWRAM here,
    /// the DMA reads it fine — FIFO underruns are the bus starver, not
    /// this copy path).
    const RING_CHUNKS: usize = 36;
    const RING_SAMPLES: usize = RING_CHUNKS * CHUNK;
    /// Prefill before the feeder arms: 16 chunks = 2,816 samples ≈ 268 ms
    /// of lead > the worst-frame drain (1,603 samples).
    const PREFILL_CHUNKS: usize = 16;

    // --- Hardware (IO region; always mapped) --------------------------------
    // Channel n's register base is 0x0400_00B0 + 0x0C·n (agb `hw.rs`'s
    // dma_*_addr arithmetic) — DMA1 starts at 0x0400_00BC, NOT 0xB0 (that
    // block is DMA0). Sound-FIFO timing is illegal for DMA0, and the FIFO
    // pairing is fixed: DMA1→FIFO A, DMA2→FIFO B (a first draft shifted
    // both blocks one channel low — mGBA discarded the DMA0 scheduling
    // [left silent] and half-tolerated the DMA1→FIFO B mis-pairing [right
    // glitchy]; Sam's ears caught it, 2026-10-19).
    const REG_DMA1_SOURCE: usize = 0x0400_00BC;
    const REG_DMA1_DEST: usize = 0x0400_00C0;
    const REG_DMA1_COUNT: usize = 0x0400_00C4;
    const REG_DMA1_CONTROL: usize = 0x0400_00C6;
    const REG_DMA2_SOURCE: usize = 0x0400_00C8;
    const REG_DMA2_DEST: usize = 0x0400_00CC;
    const REG_DMA2_COUNT: usize = 0x0400_00D0;
    const REG_DMA2_CONTROL: usize = 0x0400_00D2;
    const FIFO_A: usize = 0x0400_00A0;
    const FIFO_B: usize = 0x0400_00A4;
    const SOUNDCNT_H: usize = 0x0400_0082;
    const SOUNDCNT_X: usize = 0x0400_0084;
    const SOUNDBIAS: usize = 0x0400_0088;
    const REG_IE: usize = 0x0400_0200;
    const REG_IF: usize = 0x0400_0202;
    const REG_IME: usize = 0x0400_0208;

    /// agb `hw.rs`'s sound-DMA control word, transcribed verbatim in
    /// meaning: dest fixed (2<<5) | repeat (1<<9) | transfer words (1<<10)
    /// | sound-FIFO start timing (3<<12) | enable (1<<15).
    const DMA_SOUND_CONTROL: u16 = (2 << 5) | (1 << 9) | (1 << 10) | (3 << 12) | (1 << 15);
    /// One hardware buffer in 32-bit words (176/304/560 are mixer-verified
    /// multiples of 4).
    const DMA_WORDS: u16 = (CHUNK / 4) as u16;

    /// Ping-pong DMA payload, word-aligned (DMA sources must be).
    #[repr(align(4))]
    struct SndBufs {
        l0: [u8; CHUNK],
        r0: [u8; CHUNK],
        l1: [u8; CHUNK],
        r1: [u8; CHUNK],
        next: usize,
    }

    impl SndBufs {
        const fn new() -> Self {
            Self {
                l0: [0u8; CHUNK],
                r0: [0u8; CHUNK],
                l1: [0u8; CHUNK],
                r1: [0u8; CHUNK],
                next: 0,
            }
        }
    }

    static FEED: Mutex<RefCell<RingFeed<RING_SAMPLES>>> = Mutex::new(RefCell::new(RingFeed::new()));
    static BUFS: Mutex<RefCell<SndBufs>> = Mutex::new(RefCell::new(SndBufs::new()));
    /// Playback bookkeeping: the producer's `finished` flag (flips the
    /// feeder's short-chunk classification from underflow to zero-padded
    /// tail) plus the feeder's period/underflow counters.
    /// thumbv4t-none-eabi ships no core atomics, so this rides the same
    /// Mutex+RefCell critical-section discipline as FEED/BUFS: the feeder
    /// runs in IRQ context (interrupts already off) and the producer wraps
    /// every access in `critical_section::with` — one core, so no
    /// read-modify-write can interleave.
    #[derive(Default)]
    struct Stats {
        finished: bool,
        underflows: u32,
        chunks_taken: u32,
    }
    static STATS: Mutex<RefCell<Stats>> = Mutex::new(RefCell::new(Stats {
        finished: false,
        underflows: 0,
        chunks_taken: 0,
    }));

    /// Snapshot of the feeder's underflow count (producer-side read).
    fn underflows() -> u32 {
        critical_section::with(|cs| STATS.borrow_ref(cs).underflows)
    }

    /// # Safety
    /// `addr` must be an even address in the I/O region (callers pass REG_*).
    unsafe fn io_write_u16(addr: usize, value: u16) {
        unsafe { (addr as *mut u16).write_volatile(value) }
    }

    /// # Safety
    /// As `io_write_u16`, for the 32-bit DMA address registers.
    unsafe fn io_write_u32(addr: usize, value: u32) {
        unsafe { (addr as *mut u32).write_volatile(value) }
    }

    /// # Safety
    /// As `io_write_u16`, read flavour.
    unsafe fn io_read_u16(addr: usize) -> u16 {
        unsafe { (addr as *const u16).read_volatile() }
    }

    /// Re-arm both sound DMAs from `lsrc`/`rsrc` (stop, source, dest,
    /// count, go — the agb `enable_dma_for_sound` shape plus an explicit
    /// word count).
    ///
    /// # Safety
    /// Callers pass 4-byte-aligned pointers into the static ping-pong.
    unsafe fn rearm_dma(lsrc: *const u8, rsrc: *const u8) {
        unsafe {
            io_write_u16(REG_DMA1_CONTROL, 0);
            io_write_u32(REG_DMA1_SOURCE, lsrc as u32);
            io_write_u32(REG_DMA1_DEST, FIFO_A as u32);
            io_write_u16(REG_DMA1_COUNT, DMA_WORDS);
            io_write_u16(REG_DMA1_CONTROL, DMA_SOUND_CONTROL);

            io_write_u16(REG_DMA2_CONTROL, 0);
            io_write_u32(REG_DMA2_SOURCE, rsrc as u32);
            io_write_u32(REG_DMA2_DEST, FIFO_B as u32);
            io_write_u16(REG_DMA2_COUNT, DMA_WORDS);
            io_write_u16(REG_DMA2_CONTROL, DMA_SOUND_CONTROL);
        }
    }

    /// The feeder: one mixer-domain buffer per timer1 overflow (every
    /// CHUNK samples). Copies the next ring chunk into the idle ping-pong
    /// buffer and re-arms both DMAs. Never logs, never allocates (the agb
    /// interrupt-handler contract). Both static borrows are held across
    /// the whole handler, which doubles as the race gate: the producer's
    /// push/spin only touches the ring inside a critical section (IME
    /// off), so feeder and producer never interleave mid-copy.
    fn feeder(cs: critical_section::CriticalSection<'_>) {
        let finished = {
            let mut stats = STATS.borrow_ref_mut(cs);
            stats.chunks_taken += 1;
            stats.finished
        };
        let mut bufs = BUFS.borrow_ref_mut(cs);
        let mut feed = FEED.borrow_ref_mut(cs);
        let slot = bufs.next;
        // Raw pointers to the slot BEFORE filling: the static never moves,
        // so they stay valid past the borrow — rearm_dma consumes them
        // once the copy below is done.
        let (lp, rp) = if slot == 0 {
            (bufs.l0.as_ptr(), bufs.r0.as_ptr())
        } else {
            (bufs.l1.as_ptr(), bufs.r1.as_ptr())
        };
        let verdict = {
            // Destructure to field borrows: two `IndexMut` slices off
            // `bufs` in one expression double-borrow the whole struct.
            let SndBufs { l0, r0, l1, r1, .. } = &mut *bufs;
            let (dl, dr) = if slot == 0 {
                (&mut l0[..], &mut r0[..])
            } else {
                (&mut l1[..], &mut r1[..])
            };
            feed.take_chunk(dl, dr, CHUNK, finished)
        };
        bufs.next = 1 - slot;
        drop(feed);
        drop(bufs);
        // SAFETY: lp/rp are aligned static-array pointers; the DMA was
        // told about the *other* buffer last period and a 44-word burst
        // (µs) is long done by the next period (2.6 ms).
        unsafe { rearm_dma(lp, rp) }
        if verdict == Chunk::Starved && !finished {
            STATS.borrow_ref_mut(cs).underflows += 1;
        }
    }

    /// Asset verification (rate ROM shape): seek-table structure + the
    /// region pin — the bytes about to be decoded and heard are the
    /// generator's bytes.
    fn check_clip(clip: &flac_spike::assets::SpikeClip) -> bool {
        let mut pass = !clip.frames.is_empty();
        pass &= clip.frames.first().is_some_and(|f| f.offset == 0);
        let mut ascending = true;
        let mut prev = clip.frames.first().map_or(0, |f| f.offset);
        for frame in clip.frames.iter().skip(1) {
            ascending &= frame.offset > prev;
            prev = frame.offset;
        }
        pass &= ascending;
        pass &= clip.sample_rate_hz == rate::RATE_HZ;
        let actual = fnv1a64(clip.region);
        let ok = actual == clip.fnv_frames;
        agb::println!(
            "[rateplay] {} region fnv: frames={} region={}B expected 0x{:016X} actual 0x{:016X} [{}]",
            clip.name,
            clip.frames.len(),
            clip.region.len(),
            clip.fnv_frames,
            actual,
            if ok { "OK" } else { "MISMATCH" }
        );
        pass && ok
    }

    /// Decode proof before the sound hardware enables (correctness-before-
    /// speed, unchanged): full walk via the shared driver, i16 fold vs the
    /// generator's PCM pin.
    fn decode_proof(clip: &flac_spike::assets::SpikeClip) -> bool {
        let max = usize::from(clip.max_blocksize);
        let mut left: alloc::vec::Vec<i32> = alloc::vec![0; max];
        let mut right: alloc::vec::Vec<i32> = alloc::vec![0; max];
        let mut state = [PredictorState::new(), PredictorState::new()];
        let mut hash = Fnv1a64::new();
        let mut fold_ok = true;
        let walk = driver::decode_clip(clip, &mut left, &mut right, &mut state, |_, l, r| {
            if fold_i16le_stereo(&mut hash, l, r).is_err() {
                fold_ok = false;
            }
        });
        let pass = match walk {
            Ok(stats) => {
                fold_ok
                    && stats.frames == clip.frames.len()
                    && stats.samples == usize::try_from(clip.total_samples).unwrap_or(usize::MAX)
                    && hash.finish() == clip.fnv_pcm
            }
            Err(err) => {
                agb::println!("[rateplay] FAIL decode proof: {:?}", err);
                false
            }
        };
        agb::println!(
            "[rateplay] decode proof {}: pcm fnv expected 0x{:016X} [{}]",
            clip.name,
            clip.fnv_pcm,
            if pass { "MATCH" } else { "MISMATCH" }
        );
        pass
    }

    /// Arm the sound hardware: FIFO routes (A=left, B=right, 100%), master
    /// enable, bias PWM 8-bit/65536 Hz — agb `hw.rs`'s enable sequence,
    /// transcribed because those functions are `pub(super)`.
    fn enable_sound() {
        let sound_control: u16 = (1 << 2) // sound A volume 100%
            | (1 << 9) // A to LEFT out
            | (1 << 11) // A FIFO reset
            | (1 << 3) // sound B volume 100%
            | (1 << 12) // B to RIGHT out
            | (1 << 15); // B FIFO reset
        // SAFETY: fixed I/O addresses.
        unsafe {
            io_write_u16(SOUNDCNT_H, sound_control);
            io_write_u16(SOUNDCNT_X, 1 << 7);
            io_write_u16(SOUNDBIAS, io_read_u16(SOUNDBIAS) | (1 << 14));
        }
    }

    /// Push one decoded frame window into the ring (feeder briefly off —
    /// the ring borrow is held inside a critical section on both sides).
    fn push(left: &[i32], right: &[i32], hl: &mut Fnv1a64, hr: &mut Fnv1a64) -> bool {
        critical_section::with(|cs| {
            FEED.borrow_ref_mut(cs)
                .push_frame(left, right, hl, hr)
                .is_ok()
        })
    }

    /// The producer's flow-control spin: wait until the feeder has drained
    /// `blocksize` of space. NEVER hold the FEED borrow across an iteration
    /// (edition-2024 temporaries live to the end of the enclosing
    /// statement — the borrow would be held while waiting and the feeder
    /// needs it in IRQ context; deadlock, caught by review before compile).
    fn wait_for_space(blocksize: usize) {
        loop {
            let free = critical_section::with(|cs| {
                let feed = FEED.borrow_ref(cs);
                let free = feed.free_samples();
                drop(feed);
                free
            });
            if free >= blocksize {
                return;
            }
        }
    }

    /// Continue the fed decode from `start_frame` (prefill consumed the
    /// head), then drain and judge the FIFO8 pins against the generator.
    fn play_rest(
        clip: &flac_spike::assets::SpikeClip,
        start_frame: usize,
        state: &mut [PredictorState; 2],
        hl: &mut Fnv1a64,
        hr: &mut Fnv1a64,
    ) -> (usize, usize, u32) {
        let max = usize::from(clip.max_blocksize);
        let mut left: alloc::vec::Vec<i32> = alloc::vec![0; max];
        let mut right: alloc::vec::Vec<i32> = alloc::vec![0; max];
        let mut frames_done = start_frame;

        for frame_index in start_frame..clip.frames.len() {
            let blocksize = usize::from(clip.frames[frame_index].blocksize);
            wait_for_space(blocksize);
            let written = match driver::decode_one(clip, frame_index, &mut left, &mut right, state)
            {
                Ok(written) => written,
                Err(err) => {
                    agb::println!(
                        "[rateplay] FAIL frame {}: driver rejected after proof passed: {:?}",
                        frame_index,
                        err
                    );
                    return (frames_done, 0, underflows());
                }
            };
            let right_window = if clip.channels == 2 {
                &right[..written]
            } else {
                &[]
            };
            if !push(&left[..written], right_window, hl, hr) {
                agb::println!(
                    "[rateplay] FAIL frame {}: push rejected (out of profile?)",
                    frame_index
                );
                return (frames_done, 0, underflows());
            }
            frames_done += 1;
            if frame_index % 128 == 0 {
                let lead = critical_section::with(|cs| {
                    let feed = FEED.borrow_ref(cs);
                    let lead = feed.ready();
                    drop(feed);
                    lead
                });
                agb::println!(
                    "[rateplay] frame {:>4}/{} bs={} lead={}",
                    frame_index,
                    clip.frames.len(),
                    written,
                    lead
                );
            }
        }
        agb::println!(
            "[rateplay] producer finished (all {} frames decoded)",
            clip.frames.len()
        );
        critical_section::with(|cs| STATS.borrow_ref_mut(cs).finished = true);

        // Drain to the end: whole periods take Full chunks, the residue
        // lands as one zero-padded Tail — the ledger advances by real
        // bytes only, so consumed reaches pushed exactly when the last
        // real sample has been handed to the DMA. (A Starved period
        // consumes nothing; the producer is still running until FINISHED,
        // and the verdict demands zero underflows anyway.)
        loop {
            let done = critical_section::with(|cs| {
                let feed = FEED.borrow_ref(cs);
                let done = feed.consumed() >= feed.pushed();
                drop(feed);
                done
            });
            if done {
                break;
            }
        }

        let l_ok = hl.finish() == rate::PLAYBACK_FIFO8_L;
        let r_ok = hr.finish() == rate::PLAYBACK_FIFO8_R;
        let underflows = underflows();
        let (pushed, consumed) = critical_section::with(|cs| {
            let feed = FEED.borrow_ref(cs);
            let pair = (feed.pushed(), feed.consumed());
            drop(feed);
            pair
        });
        agb::println!(
            "[rateplay] fifo8 L expected 0x{:016X} actual 0x{:016X} [{}]",
            rate::PLAYBACK_FIFO8_L,
            hl.finish(),
            if l_ok { "MATCH" } else { "MISMATCH" }
        );
        agb::println!(
            "[rateplay] fifo8 R expected 0x{:016X} actual 0x{:016X} [{}]",
            rate::PLAYBACK_FIFO8_R,
            hr.finish(),
            if r_ok { "MATCH" } else { "MISMATCH" }
        );
        agb::println!(
            "[rateplay] feed: pushed={} consumed={} chunks={} underflows={}",
            pushed,
            consumed,
            critical_section::with(|cs| STATS.borrow_ref(cs).chunks_taken),
            underflows
        );
        let samples_ok = pushed == usize::try_from(clip.total_samples).unwrap_or(usize::MAX);
        agb::println!(
            "[rateplay] samples pushed={} expected={} [{}]",
            pushed,
            clip.total_samples,
            if samples_ok { "OK" } else { "MISMATCH" }
        );
        if !(l_ok && r_ok && samples_ok) {
            // Judge the verdict through the underflow channel (force nonzero).
            return (frames_done, pushed, underflows.max(1));
        }
        (frames_done, pushed, underflows)
    }

    #[agb::entry]
    fn main(mut gba: agb::Gba) -> ! {
        agb::eprintln!("[rateplay] entry — FLAC decode -> FIFO playback baseline");
        agb::eprintln!(
            "[rateplay] input={} sha256={} rate={} Hz chunk={} ring={}x{} prefill={}x{}",
            rate::INPUT_NAME,
            rate::INPUT_SHA256,
            rate::RATE_HZ,
            CHUNK,
            RING_CHUNKS,
            CHUNK,
            PREFILL_CHUNKS,
            CHUNK
        );

        let mut gfx = gba.graphics.get();
        gfx.set_background_palette_colour(0, 0, COLOR_PENDING);

        // Only the FIXED arm plays (module docs: the LPC arm is 2× over
        // its own budget at this rate — measured, no feeder fixes that).
        let clip = rate::RATE_CLIPS[0];
        agb::println!(
            "[rateplay] playing arm {} ({}); LPC arm skipped — measured over budget at {} Hz",
            clip.name,
            clip.why,
            rate::RATE_HZ
        );

        let assets_pass = check_clip(clip);
        let proof_pass = decode_proof(clip);
        let mut pass = assets_pass && proof_pass;

        if pass {
            // SAFETY: the handler never allocates and only touches the
            // statics above (agb's interrupt-handler contract).
            let _handler = unsafe { add_interrupt_handler(Interrupt::Timer1, feeder) };
            enable_sound();

            // Prefill BEFORE arming the timers: decode + push with the
            // feeder off until the lead exceeds the worst-frame drain.
            let max = usize::from(clip.max_blocksize);
            let mut left: alloc::vec::Vec<i32> = alloc::vec![0; max];
            let mut right: alloc::vec::Vec<i32> = alloc::vec![0; max];
            let mut state = [PredictorState::new(), PredictorState::new()];
            let mut hl = Fnv1a64::new();
            let mut hr = Fnv1a64::new();
            let prefill_target = PREFILL_CHUNKS * CHUNK;
            let mut prefilled = 0usize;
            let mut next_frame = 0usize;
            while prefilled < prefill_target && next_frame < clip.frames.len() {
                let written =
                    match driver::decode_one(clip, next_frame, &mut left, &mut right, &mut state) {
                        Ok(written) => written,
                        Err(err) => {
                            agb::println!(
                                "[rateplay] FAIL prefill frame {}: {:?}",
                                next_frame,
                                err
                            );
                            pass = false;
                            break;
                        }
                    };
                let right_window = if clip.channels == 2 {
                    &right[..written]
                } else {
                    &[]
                };
                if !push(&left[..written], right_window, &mut hl, &mut hr) {
                    agb::println!(
                        "[rateplay] FAIL prefill push rejected at frame {}",
                        next_frame
                    );
                    pass = false;
                    break;
                }
                prefilled += written;
                next_frame += 1;
            }
            agb::println!(
                "[rateplay] prefilled {} samples in {} frames (target {}), feeder armed",
                prefilled,
                next_frame,
                prefill_target
            );

            if pass {
                // Arm: timer0 ticks once per sample (overflow 65536−1596 →
                // 1596 ticks @ 2^24 = 1/10512 s), timer1 cascades and
                // interrupts once per CHUNK.
                let mut timers = unsafe { gba.timers.all_timers() }; // SAFETY: agb's Mixer is never instantiated in this ROM — timer0/1 are unowned.
                timers
                    .timer0
                    .set_divider(Divider::Divider1)
                    .set_interrupt(false)
                    .set_overflow_amount((MIXER_TICK_HZ / rate::RATE_HZ) as u16)
                    .set_enabled(true);
                timers
                    .timer1
                    .set_cascade(true)
                    .set_interrupt(true)
                    .set_overflow_amount(CHUNK as u16)
                    .set_enabled(true);
                agb::println!(
                    "[rateplay] irq armed: IE=0x{:04X} IME={} (witness: timer1 bit set / IME 1)",
                    unsafe { io_read_u16(REG_IE) },
                    unsafe { io_read_u16(REG_IME) }
                );

                let (frames, samples, underflows) =
                    play_rest(clip, next_frame, &mut state, &mut hl, &mut hr);
                agb::println!(
                    "[rateplay] playback verdict: frames={}/{} samples={} underflows={}",
                    frames,
                    clip.frames.len(),
                    samples,
                    underflows
                );
                pass &= underflows == 0 && frames == clip.frames.len() && samples != 0;

                // Silence + teardown.
                timers.timer1.set_interrupt(false).set_enabled(false);
                timers.timer0.set_enabled(false);
                // SAFETY: fixed I/O addresses.
                unsafe {
                    io_write_u16(REG_DMA1_CONTROL, 0);
                    io_write_u16(REG_DMA2_CONTROL, 0);
                    io_write_u16(SOUNDCNT_X, 0);
                }
                agb::println!(
                    "[rateplay] teardown: IE=0x{:04X} IF=0x{:04X} IME={}",
                    unsafe { io_read_u16(REG_IE) },
                    unsafe { io_read_u16(REG_IF) },
                    unsafe { io_read_u16(REG_IME) }
                );
            }
        } else {
            agb::println!(
                "[rateplay] playback SKIPPED — correctness-before-speed (assets/proof failed)"
            );
        }

        let (color, label) = if pass {
            (COLOR_PASS, "BLUE")
        } else {
            (COLOR_FAIL, "RED")
        };
        gfx.set_background_palette_colour(0, 0, color);
        agb::eprintln!(
            "[rateplay] verdict {} — screen {} (assets={}, proof={}, playback={})",
            if pass { "PASSED" } else { "FAILED" },
            label,
            assets_pass,
            proof_pass,
            pass
        );

        loop {
            let frame = gfx.frame();
            frame.commit();
        }
    }
}

/// Host-build stub (same dual-target pattern as rate_main.rs).
#[cfg(not(target_arch = "arm"))]
fn main() {}
