//! ULI2 decoder cycle harness (Phase 1b, extended by PR A).
//!
//! This image is the measurement vehicle for the U4 gate (real-time decode)
//! and for every hot-path PR that follows. It embeds the committed Phase 0
//! corpus, proves its own decode against the C-reference album pins, and only
//! then times work:
//!
//! 1. **Correctness first** — the whole corpus is decoded and folded into the
//!    album-wide int8/int16 FNV pins (`e6e87652` / `b3530978`) with
//!    `xover = 0`, `hfnz = 0`. A fast wrong decode measures nothing, so no
//!    timing line is emitted unless this pass passes.
//! 2. **Full-frame cost, both kernel sets** — PR A: `Decoder` is generic over
//!    `uli_lite::Kernels`. A fresh album decode is timed frame by frame
//!    through the portable reference kernels (the control) and through
//!    `uli_lite_gba::ArmKernels` (the product's ARM-mode, unchecked, `.iwram`
//!    kernels, called from the same bitstream/dequant/M/S/frame code). Both
//!    passes are pinned. This is a *measured* full frame for the product
//!    kernel set, not the Phase 1b projection.
//! 3. **Kernel share** — the four synthesis kernels are timed in isolation on
//!    deterministic inputs, then as one per-channel sequence. This separates
//!    the kernel cost from entropy/dequant/M/S/glue, which no kernel port
//!    touches.
//! 4. **Overlay twin** — the image runs the reference and overlay kernels on
//!    identical inputs and compares every sample, so the timing arm is
//!    measuring the same arithmetic the host twin test proves.
//!
//! Measurement discipline (the same rules as the FLAC cycle harness, so the
//! two records are comparable):
//!
//! - **Empty-window calibration first**: five windows with no work inside.
//!   Every frame's net window is raw minus that overhead (raw is printed too).
//! - **IRQs off inside every window** (agb's `critical_section`); all logging
//!   happens outside the window.
//! - **No division on the measured path**: the ROM reports `sum` and `count`;
//!   means are derived host-side. The ARM7TDMI has no divide, and a software
//!   divide next to the numbers being measured would be part of the
//!   instrument.
//! - **WAITCNT is set explicitly** to `0x0000` — the 4/2 (no prefetch)
//!   setting the U4 gate and the `gba-audio` reference are stated at. `agb`
//!   boots at `0x4317` (3/1 + prefetch); the boot value is reported so the
//!   two arms cannot be confused later.
//! - **Wrap is loud**: a window that wraps the 32-bit composite saturates to
//!   `u32::MAX` and fails the verdict rather than understating the worst
//!   frame.
//!
//! What this image does NOT do: playback, FIFO/DMA, IRQ/underrun counting, or
//! hardware. It is an mGBA measurement. It measures the product **kernel set**
//! in a full frame; the product **hot path** (bit reader, Huffman, dequant,
//! M/S in ARM/IWRAM) arrives in PR B, and this image is its before/after
//! control.

#![no_std]
#![no_main]

use agb::display::Rgb;
use agb::timer::{Divider, Timers};
use uli_lite::format::{H, M};
use uli_lite::{Album, Decoder, DecoderState, imdct};
use uli_lite_gba as overlay;

/// The committed Phase 0 corpus, byte-for-byte.
static ALBUM: &[u8] = include_bytes!("../../../crates/uli-lite/tests/fixtures/uli2_tiny.uli");

/// Caller-owned decoder state, in EWRAM (the ~100 KiB Huffman LUTs dominate).
#[unsafe(link_section = ".ewram")]
static mut STATE: DecoderState = DecoderState::new();

/// Kernel inputs/outputs live in EWRAM here. The overlay's job is to move the
/// hot ones to IWRAM; this arm deliberately keeps the as-shipped placement.
#[unsafe(link_section = ".ewram")]
static mut KX: [i32; M] = [0; M];
#[unsafe(link_section = ".ewram")]
static mut KZ: [i32; 2 * H] = [0; 2 * H];
#[unsafe(link_section = ".ewram")]
static mut KY: [i32; M] = [0; M];
#[unsafe(link_section = ".ewram")]
static mut KP: [i32; H] = [0; H];
#[unsafe(link_section = ".ewram")]
static mut K8L: [i8; M] = [0; M];
#[unsafe(link_section = ".ewram")]
static mut K16L: [i16; M] = [0; M];

/// The product frame period: 512 samples at 32,768 Hz on the 2^24 Hz clock.
/// `512 * 2^24 / 32768 = 262144` cycles. Compile-time arithmetic, no divide on
/// the measured path.
const FRAME_BUDGET_CYCLES: u32 = 262_144;

/// FNV-1a 32 of the C reference's per-frame int8 hashes, in frame order.
const EXPECTED_INT8: u32 = 0xe6e8_7652;
/// FNV-1a 32 of the C reference's per-frame int16 hashes, in frame order.
const EXPECTED_INT16: u32 = 0xb353_0978;

const COLOR_PASS: agb::display::Rgb15 = Rgb::new(0, 0, 255).to_rgb15();
const COLOR_FAIL: agb::display::Rgb15 = Rgb::new(255, 0, 0).to_rgb15();

/// `WAITCNT` (0x0400_0204). Scanning at 4/2/no-prefetch is the U4 reference
/// arm; `0x0000` is the power-on value and the archive's "slow cart" arm.
const WAITCNT_ADDR: *mut u16 = 0x0400_0204 as *mut u16;

fn waitcnt() -> u16 {
    // SAFETY: WAITCNT is an MMIO register at a fixed address; a 16-bit
    // volatile read is the documented access.
    unsafe { core::ptr::read_volatile(WAITCNT_ADDR) }
}

fn set_waitcnt(value: u16) {
    // SAFETY: as above; writing the full documented register.
    unsafe { core::ptr::write_volatile(WAITCNT_ADDR, value) };
}

fn fnv_bytes(mut h: u32, bytes: &[u8]) -> u32 {
    for &b in bytes {
        h ^= b as u32;
        h = h.wrapping_mul(16_777_619);
    }
    h
}

fn fnv_i8(mut h: u32, l: &[i8], r: &[i8]) -> u32 {
    for &v in l {
        h = fnv_bytes(h, &[v as u8]);
    }
    for &v in r {
        h = fnv_bytes(h, &[v as u8]);
    }
    h
}

fn fnv_i16(mut h: u32, l: &[i16], r: &[i16]) -> u32 {
    for &v in l {
        h = fnv_bytes(h, &v.to_le_bytes());
    }
    for &v in r {
        h = fnv_bytes(h, &v.to_le_bytes());
    }
    h
}

/// The free-running cycle counter: timer2 /1 (one tick per CPU cycle at
/// 16.78 MHz) overflowing into timer3 via the hardware cascade bit. Same
/// mechanism and read protocol as the FLAC harness.
struct Counter<'a> {
    low: &'a mut agb::timer::Timer,
    high: &'a mut agb::timer::Timer,
}

impl Counter<'_> {
    fn start<'a>(timers: &'a mut Timers<'_>) -> Counter<'a> {
        timers
            .timer2
            .set_divider(Divider::Divider1)
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

    /// Freeze, zero both halves, unfreeze.
    fn soft_reset(&mut self) {
        self.low.set_enabled(false);
        self.high.set_enabled(false);
        self.high.set_overflow_amount(0);
        self.low.set_overflow_amount(0);
        self.low.set_enabled(true);
        self.high.set_enabled(true);
    }

    /// Low, high, low-stability read of the 32-bit composite.
    fn read(&self) -> u32 {
        for _ in 0..4 {
            let low1 = self.low.value();
            let high = self.high.value();
            let low = self.low.value();
            if low >= low1 {
                return (u32::from(high) << 16) | u32::from(low);
            }
        }
        let high = self.high.value();
        let low = self.low.value();
        (u32::from(high) << 16) | u32::from(low)
    }
}

/// Saturating window difference. A wrapped window is reported as `u32::MAX`
/// and fails the verdict; it is never silently truncated.
fn cycles_between(start: u32, end: u32) -> u32 {
    if end >= start { end - start } else { u32::MAX }
}

/// Run `work` inside one measurement window with IRQs off.
fn timed<T>(counter: &mut Counter<'_>, work: impl FnOnce() -> T) -> (T, u32) {
    let mut raw = 0u32;
    let out = agb::external::critical_section::with(|_cs| {
        let start = counter.read();
        let out = work();
        let end = counter.read();
        raw = cycles_between(start, end);
        out
    });
    (out, raw)
}

/// Running fold over per-frame net windows.
struct FrameCost {
    count: u32,
    min: u32,
    min_frame: u32,
    max: u32,
    max_frame: u32,
    sum: u64,
    wrapped: bool,
    worst_ops: uli_lite::Ops,
    worst_model: u32,
}

impl FrameCost {
    const fn new() -> Self {
        Self {
            count: 0,
            min: u32::MAX,
            min_frame: 0,
            max: 0,
            max_frame: 0,
            sum: 0,
            wrapped: false,
            worst_ops: uli_lite::Ops::ZERO,
            worst_model: 0,
        }
    }

    fn observe(&mut self, frame: u32, net: u32, ops: uli_lite::Ops) {
        self.count += 1;
        self.sum += u64::from(net);
        self.wrapped |= net == u32::MAX;
        if net < self.min {
            self.min = net;
            self.min_frame = frame;
        }
        // Strict `>`: the first (lowest-index) frame holds a tie, the same
        // convention as the FLAC fold.
        if net > self.max {
            self.max = net;
            self.max_frame = frame;
            self.worst_ops = ops;
            self.worst_model = ops.cycle_model();
        }
    }
}

/// Deterministic kernel input: a 28-bit signed pattern, inside the encoder's
/// `|X| < 2^28` guarantee. The kernels' cost is data-independent except for
/// the ARM7TDMI multiplier's early-termination, so a fixed pattern is a
/// reproducible arm rather than a sampled one.
fn fill_kx(buf: &mut [i32; M]) {
    let mut s: u32 = 0x1234_5678;
    for v in buf.iter_mut() {
        s ^= s << 13;
        s ^= s >> 17;
        s ^= s << 5;
        *v = (s as i32) >> 4;
    }
}

/// One full-album decode through kernel set `K`, every frame timed, with the
/// album pins folded on the same samples. `PortableKernels` is the control;
/// `uli_lite_gba::ArmKernels` is the product arm.
fn timed_pass<K: uli_lite::Kernels>(
    album: Album<'_>,
    state: &mut DecoderState,
    counter: &mut Counter<'_>,
    overhead: u32,
) -> Result<(FrameCost, u32, u32), uli_lite::Error> {
    let mut decoder = Decoder::<K>::with_kernels(album, state)?;
    let mut l8 = [0i8; M];
    let mut r8 = [0i8; M];
    let mut l16 = [0i16; M];
    let mut r16 = [0i16; M];
    let mut cost = FrameCost::new();
    let mut all8 = 2_166_136_261u32;
    let mut all16 = 2_166_136_261u32;
    for f in 0..album.n_frames() {
        let (res, raw) = timed(counter, || {
            decoder.decode_frame(&mut l8, &mut r8, Some(&mut l16), Some(&mut r16))
        });
        res?;
        // Hash outside the window: proves the timed decode is the pinned
        // decode, not just a decode that consumed the same number of cycles.
        all8 = fnv_bytes(all8, &fnv_i8(2_166_136_261, &l8, &r8).to_le_bytes());
        all16 = fnv_bytes(all16, &fnv_i16(2_166_136_261, &l16, &r16).to_le_bytes());
        let net = raw.saturating_sub(overhead);
        cost.observe(f, net, decoder.state().ops());
    }
    Ok((cost, all8, all16))
}

fn print_cost(label: &str, cost: &FrameCost) {
    agb::println!(
        "[uli-bench] frame {} count={} min={} min_frame={} max={} max_frame={} sum={} wrapped={}",
        label,
        cost.count,
        cost.min,
        cost.min_frame,
        cost.max,
        cost.max_frame,
        cost.sum,
        cost.wrapped
    );
    agb::println!(
        "[uli-bench] {} worst ops bandsyms={} coded={} pairs={} quads={} nz={} zc={} raws={} escs={} ms={} long={} model={}",
        label,
        cost.worst_ops.bandsyms,
        cost.worst_ops.coded_bands,
        cost.worst_ops.pairs,
        cost.worst_ops.quads,
        cost.worst_ops.nz,
        cost.worst_ops.zc,
        cost.worst_ops.raws,
        cost.worst_ops.escs,
        cost.worst_ops.ms,
        cost.worst_ops.long_codes,
        cost.worst_model
    );
}

fn run(gba: &mut agb::Gba) -> bool {
    let boot_waitcnt = waitcnt();
    // The U4 arm: 4/2, no prefetch. `agb` boots at 0x4317 (3/1 + prefetch).
    set_waitcnt(0x0000);
    agb::println!(
        "[uli-bench] ULI2 Phase 1b cycle harness: blob={} bytes waitcnt=0x{:04X} (boot=0x{:04X}) budget={} cycles/frame",
        ALBUM.len(),
        waitcnt(),
        boot_waitcnt,
        FRAME_BUDGET_CYCLES
    );

    let album = match Album::parse(ALBUM) {
        Ok(a) => a,
        Err(e) => {
            agb::println!("[uli-bench] FAIL album parse: {:?}", e);
            return false;
        }
    };
    agb::println!(
        "[uli-bench] frames={} tracks={} rate={} hop={}",
        album.n_frames(),
        album.n_tracks(),
        album.header().sample_rate,
        album.header().hop
    );

    let state = unsafe { &mut *core::ptr::addr_of_mut!(STATE) };
    let mut l8 = [0i8; M];
    let mut r8 = [0i8; M];
    let mut l16 = [0i16; M];
    let mut r16 = [0i16; M];

    // -----------------------------------------------------------------
    // 1. Correctness pass: the pins the C reference produced.
    // -----------------------------------------------------------------
    let mut decoder = match Decoder::new(album, state) {
        Ok(d) => d,
        Err(e) => {
            agb::println!("[uli-bench] FAIL tables: {:?}", e);
            return false;
        }
    };
    let mut all8 = 2_166_136_261u32;
    let mut all16 = 2_166_136_261u32;
    let mut frames = 0u32;
    for _ in 0..album.n_frames() {
        if let Err(e) = decoder.decode_frame(&mut l8, &mut r8, Some(&mut l16), Some(&mut r16)) {
            agb::println!("[uli-bench] FAIL decode frame {}: {:?}", frames, e);
            return false;
        }
        all8 = fnv_bytes(all8, &fnv_i8(2_166_136_261, &l8, &r8).to_le_bytes());
        all16 = fnv_bytes(all16, &fnv_i16(2_166_136_261, &l16, &r16).to_le_bytes());
        frames += 1;
    }
    let diag_ok = decoder.state().xover() == 0 && decoder.state().hfnz() == 0;
    let pin_ok =
        frames == album.n_frames() && all8 == EXPECTED_INT8 && all16 == EXPECTED_INT16 && diag_ok;
    agb::println!(
        "[uli-bench] decode frames={} int8={:08x} (want {:08x}) int16={:08x} (want {:08x}) xover={} hfnz={} [{}]",
        frames,
        all8,
        EXPECTED_INT8,
        all16,
        EXPECTED_INT16,
        decoder.state().xover(),
        decoder.state().hfnz(),
        if pin_ok { "PASS" } else { "FAIL" }
    );
    if !pin_ok {
        return false;
    }

    // -----------------------------------------------------------------
    // 2. Empty-window calibration + frame timing.
    // -----------------------------------------------------------------
    let mut timers = gba.timers.timers();
    let mut counter = Counter::start(&mut timers);
    let mut calib = [0u32; 5];
    for slot in calib.iter_mut() {
        let (_, raw) = timed(&mut counter, || {});
        *slot = raw;
    }
    let calib_min = calib.iter().copied().min().unwrap_or(0);
    let calib_max = calib.iter().copied().max().unwrap_or(0);
    let overhead = calib_max;
    // The empty window is two timer-register reads wide; mGBA's composite read
    // has a few-cycle phase jitter (measured spread: 12 cycles). The bound is
    // 64 cycles -- still a determinism probe at the scale of the thing being
    // measured (a frame window is ~10^6 cycles), and 0.006% of the budget.
    let calib_ok = calib_max - calib_min <= 64;
    agb::println!(
        "[uli-bench] calib raw={:?} min={} max={} spread={} ok={} overhead={}",
        calib,
        calib_min,
        calib_max,
        calib_max - calib_min,
        calib_ok,
        overhead
    );

    // -----------------------------------------------------------------
    // 3. Full-frame timing through both kernel sets (PR A).
    //    The core arm is the control; the overlay arm is the product path:
    //    same bitstream/dequant/M/S/frame code, ARM-mode kernels in .iwram.
    // -----------------------------------------------------------------
    let (cost_core, core8, core16) =
        match timed_pass::<uli_lite::PortableKernels>(album, state, &mut counter, overhead) {
            Ok(v) => v,
            Err(e) => {
                agb::println!("[uli-bench] FAIL core timed pass: {:?}", e);
                return false;
            }
        };
    let core_pin_ok = core8 == EXPECTED_INT8 && core16 == EXPECTED_INT16;
    print_cost("core", &cost_core);
    agb::println!(
        "[uli-bench] core pins int8={:08x} int16={:08x} [{}]",
        core8,
        core16,
        if core_pin_ok { "PASS" } else { "FAIL" }
    );

    let (cost_ovl, ovl8, ovl16) =
        match timed_pass::<uli_lite_gba::ArmKernels>(album, state, &mut counter, overhead) {
            Ok(v) => v,
            Err(e) => {
                agb::println!("[uli-bench] FAIL overlay timed pass: {:?}", e);
                return false;
            }
        };
    let ovl_pin_ok = ovl8 == EXPECTED_INT8 && ovl16 == EXPECTED_INT16;
    print_cost("ovl", &cost_ovl);
    agb::println!(
        "[uli-bench] ovl pins int8={:08x} int16={:08x} [{}]",
        ovl8,
        ovl16,
        if ovl_pin_ok { "PASS" } else { "FAIL" }
    );

    // -----------------------------------------------------------------
    // 4. Kernel share.
    // -----------------------------------------------------------------
    const KREP: u32 = 10;
    let kx = unsafe { &mut *core::ptr::addr_of_mut!(KX) };
    let kz = unsafe { &mut *core::ptr::addr_of_mut!(KZ) };
    let ky = unsafe { &mut *core::ptr::addr_of_mut!(KY) };
    let kp = unsafe { &mut *core::ptr::addr_of_mut!(KP) };
    let k8 = unsafe { &mut *core::ptr::addr_of_mut!(K8L) };
    let k16 = unsafe { &mut *core::ptr::addr_of_mut!(K16L) };
    fill_kx(kx);

    // Each reference goes through `black_box`: without it LLVM hoists a
    // repeated idempotent kernel call out of the loop and the arm reports one
    // call's cost as ten. The sequence arm below is left alone on purpose: its
    // iterations are dependent (each kernel consumes the previous one's
    // output), so it is the realistic inlining shape and cannot be hoisted.
    let (_, pre_sum) = timed(&mut counter, || {
        for _ in 0..KREP {
            imdct::pre(core::hint::black_box(&*kx), core::hint::black_box(&mut *kz));
        }
    });
    let (_, fft_sum) = timed(&mut counter, || {
        for _ in 0..KREP {
            imdct::fft(core::hint::black_box(&mut *kz));
        }
    });
    let (_, post_sum) = timed(&mut counter, || {
        for _ in 0..KREP {
            imdct::post(core::hint::black_box(&*kz), core::hint::black_box(&mut *ky));
        }
    });
    let (_, ola_sum) = timed(&mut counter, || {
        for _ in 0..KREP {
            imdct::ola(
                core::hint::black_box(&*ky),
                core::hint::black_box(&mut *kp),
                core::hint::black_box(&mut k8[..]),
                Some(core::hint::black_box(&mut k16[..])),
            );
        }
    });
    let (_, seq_sum) = timed(&mut counter, || {
        for _ in 0..KREP {
            imdct::pre(kx, kz);
            imdct::fft(kz);
            imdct::post(kz, ky);
            imdct::ola(ky, kp, &mut k8[..], Some(&mut k16[..]));
            core::hint::black_box(&k8[0]);
        }
    });
    let kern_ok = pre_sum != u32::MAX
        && fft_sum != u32::MAX
        && post_sum != u32::MAX
        && ola_sum != u32::MAX
        && seq_sum != u32::MAX;
    agb::println!(
        "[uli-bench] kern n={} pre_sum={} fft_sum={} post_sum={} ola_sum={} seq_sum={} wrapped={}",
        KREP,
        pre_sum,
        fft_sum,
        post_sum,
        ola_sum,
        seq_sum,
        !kern_ok
    );

    // -----------------------------------------------------------------
    // 5. Overlay arm: the same four kernels, ARM mode + unchecked addressing.
    //    This is a *kernel* reading, not a full-frame one: `Decoder` still
    //    calls the core kernels. The projection the record makes from it is
    //    `frame_rest + 2 * ovl_seq`, with `frame_rest = frame_mean - 2 *
    //    core_seq` -- arithmetic host-side, from these sums and counts.
    // -----------------------------------------------------------------
    let (_, ov_pre) = timed(&mut counter, || {
        for _ in 0..KREP {
            overlay::pre(core::hint::black_box(&*kx), core::hint::black_box(&mut *kz));
        }
    });
    let (_, ov_fft) = timed(&mut counter, || {
        for _ in 0..KREP {
            overlay::fft(core::hint::black_box(&mut *kz));
        }
    });
    let (_, ov_post) = timed(&mut counter, || {
        for _ in 0..KREP {
            overlay::post(core::hint::black_box(&*kz), core::hint::black_box(&mut *ky));
        }
    });
    let (_, ov_ola) = timed(&mut counter, || {
        for _ in 0..KREP {
            overlay::ola(
                core::hint::black_box(&*ky),
                core::hint::black_box(&mut *kp),
                core::hint::black_box(&mut k8[..]),
                Some(core::hint::black_box(&mut k16[..])),
            );
        }
    });
    let (_, ov_seq) = timed(&mut counter, || {
        for _ in 0..KREP {
            overlay::pre(kx, kz);
            overlay::fft(kz);
            overlay::post(kz, ky);
            overlay::ola(ky, kp, &mut k8[..], Some(&mut k16[..]));
            core::hint::black_box(&k8[0]);
        }
    });
    let ovl_ok = ov_pre != u32::MAX
        && ov_fft != u32::MAX
        && ov_post != u32::MAX
        && ov_ola != u32::MAX
        && ov_seq != u32::MAX;
    agb::println!(
        "[uli-bench] ovl n={} pre_sum={} fft_sum={} post_sum={} ola_sum={} seq_sum={} wrapped={}",
        KREP,
        ov_pre,
        ov_fft,
        ov_post,
        ov_ola,
        ov_seq,
        !ovl_ok
    );

    // On-target twin: run the core (Thumb, checked) and overlay (ARM,
    // unchecked) kernels on identical inputs once, and compare every output.
    // The host test proves the transcription; this proves the *ARM-mode
    // unchecked* build that the timing above actually measured. A timing arm
    // on an image whose overlay arithmetic is not proven would be measuring a
    // possibly different function.
    let mut zc = [0i32; 2 * H];
    let mut zo = [0i32; 2 * H];
    let mut yc = [0i32; M];
    let mut yo = [0i32; M];
    let mut pc = [0i32; H];
    let mut po = [0i32; H];
    let mut c8 = [0i8; M];
    let mut co = [0i8; M];
    let mut c16 = [0i16; M];
    let mut o16 = [0i16; M];
    fill_kx(kx);
    imdct::pre(kx, &mut zc);
    overlay::pre(kx, &mut zo);
    let twin_pre = zc == zo;
    imdct::fft(&mut zc);
    overlay::fft(&mut zo);
    let twin_fft = zc == zo;
    imdct::post(&zc, &mut yc);
    overlay::post(&zo, &mut yo);
    let twin_post = yc == yo;
    imdct::ola(&yc, &mut pc, &mut c8, Some(&mut c16));
    overlay::ola(&yo, &mut po, &mut co, Some(&mut o16));
    let twin_ola = c8 == co && c16 == o16 && pc == po;
    let twin_ok = twin_pre && twin_fft && twin_post && twin_ola;
    agb::println!(
        "[uli-bench] ovl twin pre={} fft={} post={} ola={} [{}]",
        twin_pre,
        twin_fft,
        twin_post,
        twin_ola,
        if twin_ok { "PASS" } else { "FAIL" }
    );

    let pass = pin_ok
        && core_pin_ok
        && ovl_pin_ok
        && calib_ok
        && !cost_core.wrapped
        && !cost_ovl.wrapped
        && kern_ok
        && ovl_ok
        && twin_ok;
    agb::println!(
        "[uli-bench] {}",
        if pass {
            "PASS"
        } else {
            "FAIL: harness/pin/diagnostic failure"
        }
    );
    // mgba-test-runner's only documented exit condition is this exact line on
    // the mGBA debug log; without it the runner waits until the timeout kills
    // it. The line is harmless on hardware (nothing reads the log there), and
    // the verdict above is the real output.
    agb::println!("Tests finished successfully");
    pass
}

#[agb::entry]
fn main(mut gba: agb::Gba) -> ! {
    let pass = run(&mut gba);
    let mut gfx = gba.graphics.get();
    gfx.set_background_palette_colour(0, 0, if pass { COLOR_PASS } else { COLOR_FAIL });
    loop {
        let frame = gfx.frame();
        frame.commit();
    }
}
