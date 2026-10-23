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
//! CORRECTNESS (amended 2026-10-22, Sam: audio hot path only). This is a
//! product-shape ROM: nothing that does not contribute to sound runs here.
//! The full-track decode proof (removed 2026-10-20) and the FIFO8 fold +
//! boot region-FNV asset check (removed 2026-10-22) were witness work —
//! the fold paid an emulated 64-bit multiply per byte (ARM7TDMI has no
//! 64-bit MUL) on the producer wall, and the asset bytes are verified
//! before the ROM is built, not at boot. The chain of custody for the
//! staged bytes now lives entirely in the gates: the generator's
//! fail-closed census (`make rate-assets`) and the host witness
//! (`make rate-test`), which folds the embedded bytes through the library
//! path against the generator's FIFO8 pins. This ROM trusts the staged
//! assets and only measures what it plays: the underflow count, the
//! period audit, and the sample ledger still judge the verdict; the last
//! hop (DMA actually feeding the paired FIFO) is witnessed by the audible
//! check. Headless runs have no audio device and ignore DMA scheduling
//! warnings: routing bugs pass blind there (Sam's ears caught one,
//! 2026-10-19 — see the register-map note below). The spike ROM and the
//! rate ROM (`rate_main.rs`) keep their decode proofs and pin folds —
//! they are the research instruments; this ROM is the thing researched.
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
    use flac_spike::driver;
    use flac_spike::playback::{Chunk, RingFeed, convert_frame};

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
    // Timer data registers live at 0x0400_0100 + 4·n (agb `timer.rs`'s own
    // arithmetic): timer1 = 0x104, timer2 = 0x108.
    const REG_TM1_DATA: usize = 0x0400_0104;
    const REG_TM2_DATA: usize = 0x0400_0108;

    /// timer1's reload value at arming (`set_overflow_amount(CHUNK)` writes
    /// `0 − CHUNK`), so `TM1_DATA − TM1_RELOAD` at handler entry is the
    /// number of samples (timer0 ticks) since the LAST timer1 overflow.
    const TM1_RELOAD: u16 = 0u16.wrapping_sub(CHUNK as u16);
    /// Rearm grace in timer0 ticks: the sound DMA refills the 32-byte FIFO
    /// when it falls to half (16 samples), so a handler that rearms within
    /// ~16 samples of the buffer boundary replaces the DMA source before
    /// the repeat controller reloads the old one. Beyond this grace the DMA
    /// reloads the latched source — the old buffer's tail plays again
    /// (inserted samples) and the late swap splices mid-waveform (pop).
    const REARM_GRACE_TICKS: u16 = 16;

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
    ///
    /// The period-audit fields (added 2026-10-20) witness the feeder→DMA
    /// seam the FIFO8 pins cannot reach. Timer2 cascades from timer1 and so
    /// counts hardware periods even while IME is off; TM1_DATA times how
    /// long each IRQ sat pending past its overflow.
    ///  - `coalesced`: Σ(gap−1) over feeder entries (gap from the timer2
    ///    delta) — overflows latched into IF and coalesced into one later
    ///    IRQ: full-buffer replays, guaranteed (the rearm was a whole
    ///    period late).
    ///  - `late_servicings`: feeder entries whose TM1_DATA says the IRQ sat
    ///    more than `REARM_GRACE_TICKS` past the overflow — the DMA had
    ///    already reloaded the old source, so these serviced periods replay
    ///    a tail and splice mid-waveform (the pop + inserted samples).
    ///    A late rearm *within* the grace is harmless (source swapped
    ///    before the repeat reload) and is not counted.
    ///  - `push_swallowed`: overflow events counted by reading the period
    ///    counter across each push critical section — the attribution
    ///    channel: push_swallowed ≈ coalesced + late_servicings convicts
    ///    the push window itself; a large gap points elsewhere.
    #[derive(Default)]
    struct Stats {
        finished: bool,
        underflows: u32,
        chunks_taken: u32,
        coalesced: u32,
        max_period_gap: u16,
        last_period: u16,
        late_servicings: u32,
        max_delay_ticks: u16,
        push_swallowed: u32,
        pushes_with_swallow: u32,
    }
    static STATS: Mutex<RefCell<Stats>> = Mutex::new(RefCell::new(Stats {
        finished: false,
        underflows: 0,
        chunks_taken: 0,
        coalesced: 0,
        max_period_gap: 0,
        last_period: 0,
        late_servicings: 0,
        max_delay_ticks: 0,
        push_swallowed: 0,
        pushes_with_swallow: 0,
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
        // Period audit, read at entry before anything else can delay it:
        //  - timer2 (cascade from timer1, hardware-counted even with IME
        //    off) gives the number of overflows this IRQ delivered: gap 1
        //    = one overflow, gap N>1 = N−1 coalesced replays.
        //  - TM1_DATA (also counting under IME-off) times how long the IRQ
        //    sat pending past its overflow: delay > grace ⇒ the repeat DMA
        //    had already reloaded the old buffer — this servicing is a late
        //    splice, the pop/insertion event, even at gap 1.
        //  (agb's asm prologue + this read cost ~10² cycles ≈ <1 tick at
        //  10,512 ticks/s, far under the 16-tick grace it measures.)
        let (now, delay) = unsafe {
            (
                io_read_u16(REG_TM2_DATA),
                io_read_u16(REG_TM1_DATA).wrapping_sub(TM1_RELOAD),
            )
        }; // SAFETY: fixed I/O addresses.
        let finished = {
            let mut stats = STATS.borrow_ref_mut(cs);
            let gap = now.wrapping_sub(stats.last_period);
            stats.last_period = now;
            if gap > 1 {
                stats.coalesced += u32::from(gap - 1);
                if gap > stats.max_period_gap {
                    stats.max_period_gap = gap;
                }
            }
            // gap > 1 ⇒ the last overflow was ≥ one full period ago and
            // TM1_DATA has re-wrapped; the coalesced count already owns
            // that case — only gap-1 entries time a sub-period delay.
            if gap == 1 && delay > REARM_GRACE_TICKS {
                stats.late_servicings += 1;
                if delay > stats.max_delay_ticks {
                    stats.max_delay_ticks = delay;
                }
            }
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

    /// Push one decoded frame window into the ring.
    ///
    /// **2026-10-22 (Sam): the FIFO8 fold is gone from this ROM.** What
    /// remains is only what produces sound: the fit check + FIFO conversion
    /// run IRQ-live (producer-private pure work over locals + scratch),
    /// then the ring byte copy + credit runs inside the critical section
    /// (~2·n byte stores, µs). History: the fold first rode inside the CS
    /// (the audit `a4f7560` measured ~64 ms IME-off per push swallowing
    /// ~3.8 feeder periods), was hoisted IRQ-live 2026-10-21, and is now
    /// removed entirely — per-byte FNV-64 on the ARM7TDMI is an emulated
    /// 64-bit multiply on the producer wall for witness work that
    /// contributes no audio (module docs).
    ///
    /// Period audit (kept as the regression witness): reading the hardware
    /// period counter around the CS attributes any remaining swallowed
    /// overflows to this (now µs-scale) window — it must read 0. Prefill
    /// pushes run before the counter is armed (delta 0 regardless).
    fn push(left: &[i32], right: &[i32], scratch_l: &mut [u8], scratch_r: &mut [u8]) -> bool {
        let n = left.len();
        let stereo = !right.is_empty();
        // IRQ-live half: check fit + convert (rejections write nothing).
        let dst_r: &mut [u8] = if stereo { &mut scratch_r[..n] } else { &mut [] };
        if convert_frame(left, right, &mut scratch_l[..n], dst_r).is_err() {
            return false;
        }
        let src_r: &[u8] = if stereo { &scratch_r[..n] } else { &[] };
        // IRQ-off half: ring copy + credit only.
        let before = unsafe { io_read_u16(REG_TM2_DATA) }; // SAFETY: fixed I/O address.
        let ok = critical_section::with(|cs| {
            FEED.borrow_ref_mut(cs)
                .push_bytes(&scratch_l[..n], src_r)
                .is_ok()
        });
        let swallowed = unsafe { io_read_u16(REG_TM2_DATA) }.wrapping_sub(before); // SAFETY: fixed I/O address.
        if swallowed > 0 {
            critical_section::with(|cs| {
                let mut stats = STATS.borrow_ref_mut(cs);
                stats.push_swallowed += u32::from(swallowed);
                stats.pushes_with_swallow += 1;
            });
        }
        ok
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
    /// head), then drain and judge the sample ledger.
    fn play_rest(
        clip: &flac_spike::assets::SpikeClip,
        start_frame: usize,
        state: &mut [PredictorState; 2],
        scratch_l: &mut [u8],
        scratch_r: &mut [u8],
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
                        "[rateplay] FAIL frame {}: driver rejected mid-playback: {:?}",
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
            if !push(&left[..written], right_window, scratch_l, scratch_r) {
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

        let underflows = underflows();
        let (pushed, consumed) = critical_section::with(|cs| {
            let feed = FEED.borrow_ref(cs);
            let pair = (feed.pushed(), feed.consumed());
            drop(feed);
            pair
        });
        let chunks = critical_section::with(|cs| STATS.borrow_ref(cs).chunks_taken);
        agb::println!(
            "[rateplay] feed: pushed={} consumed={} chunks={} underflows={}",
            pushed,
            consumed,
            chunks,
            underflows
        );
        // Period audit (diagnostic, 2026-10-20): the timer2 witness keeps
        // counting while IME is off, so elapsed − serviced is periods the
        // hardware played but the feeder never swapped — the DMA replayed
        // the old buffer. late = gap-1 services rearmed past the DMA
        // grace; push_swallowed attributes the swallowed overflows to the
        // push critical section (theory: fold+copy with IME off collides
        // with the period boundary).
        let (coalesced, max_gap, late, max_delay, swallowed, pushes_sw) =
            critical_section::with(|cs| {
                let s = STATS.borrow_ref(cs);
                (
                    s.coalesced,
                    s.max_period_gap,
                    s.late_servicings,
                    s.max_delay_ticks,
                    s.push_swallowed,
                    s.pushes_with_swallow,
                )
            });
        let elapsed = unsafe { io_read_u16(REG_TM2_DATA) }; // SAFETY: fixed I/O address.
        agb::println!(
            "[rateplay] period audit: hw_elapsed={elapsed} serviced={chunks} coalesced={coalesced} max_gap={max_gap} late={late} max_delay_ticks={max_delay} grace={REARM_GRACE_TICKS} push_swallowed={swallowed} pushes_with_swallow={pushes_sw}"
        );
        let replays = coalesced + late;
        let attributed = swallowed >= replays;
        agb::println!(
            "[rateplay] audit verdict: replays={replays} (coalesced={coalesced} late={late}) of {chunks} serviced periods; attributed_to_push_window={attributed} (swallowed={swallowed} in {pushes_sw} pushes)"
        );
        let samples_ok = pushed == usize::try_from(clip.total_samples).unwrap_or(usize::MAX);
        agb::println!(
            "[rateplay] samples pushed={} expected={} [{}]",
            pushed,
            clip.total_samples,
            if samples_ok { "OK" } else { "MISMATCH" }
        );
        if !samples_ok {
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

        // 2026-10-22: no boot asset check — the staged bytes are verified
        // at generation time (fail-closed census + host witness); this ROM
        // trusts them and measures only what it plays.
        let mut pass = true;

        {
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
            // Producer-private conversion scratch (one frame per channel,
            // FIFO bytes): written/folded with IRQs live, only the bytes
            // cross into the ring inside the CS.
            let mut scratch_l: alloc::vec::Vec<u8> = alloc::vec![0; max];
            let mut scratch_r: alloc::vec::Vec<u8> = alloc::vec![0; max];
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
                if !push(
                    &left[..written],
                    right_window,
                    &mut scratch_l,
                    &mut scratch_r,
                ) {
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
                let mut timers = unsafe { gba.timers.all_timers() }; // SAFETY: agb's Mixer is never instantiated in this ROM — timer0/1/2 are unowned.
                // Period-audit witness FIRST (armed before timer1 can
                // overflow): timer2 cascades from timer1, counting every
                // period in hardware — IRQ delivery cannot stall it, so it
                // is ground truth for periods elapsed vs serviced.
                timers
                    .timer2
                    .set_cascade(true)
                    .set_interrupt(false)
                    .set_overflow_amount(0)
                    .set_enabled(true);
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
                    play_rest(clip, next_frame, &mut state, &mut scratch_l, &mut scratch_r);
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
                timers.timer2.set_enabled(false);
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
        }

        let (color, label) = if pass {
            (COLOR_PASS, "BLUE")
        } else {
            (COLOR_FAIL, "RED")
        };
        gfx.set_background_palette_colour(0, 0, color);
        agb::eprintln!(
            "[rateplay] verdict {} — screen {}",
            if pass { "PASSED" } else { "FAILED" },
            label
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
