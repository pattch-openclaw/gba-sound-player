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
//! 5. **By-difference attribution** (`attrib-*` features, timing-only) — each arm
//!    removes exactly one part of the product frame (a kernel, all synthesis,
//!    `dec_chan`, the M/S loop) so the frame can be attributed by difference in
//!    the product image. `attrib-dump` prints one `pfc f= cyc=` row per frame so
//!    arms can be differenced at the same frame index, not only in the mean.
//!    These builds fail their pins by construction and are never quoted as the
//!    product number. Reproduce the whole set with `make uli-attrib`; the method
//!    and results are in `crates/uli-lite/PLAN-OF-RECORD.md` ("Step 0").
//!
//! **Known defect: the `verify` arm does not run on target.** With
//! `--features verify` (which `per-frame` implies) the `ovl` pin fails
//! (`b3613428`/`b607b02e`; `1b4f486e`/`28d04b52` with `per-frame`) and the run
//! stops before the product arms, so the per-frame dump and its operation
//! counts are unavailable. The differing hashes across two builds point at
//! corruption, not arithmetic; the leading hypothesis is `.iwram` headroom
//! (29,688 B of 32,512 in the verify image). Host twins with `verify` pass
//! (`make uli-overlay-test`). For operation counts use `make uli-frame-ops`,
//! which reads them from the bitstream on the host — see PLAN-OF-RECORD.md
//! "Step 0b".
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
use agb::timer::Divider;
use uli_lite::format::{H, M};
use uli_lite::{Album, CodeTables, Decoder, DecoderState, imdct};
use uli_lite_gba as overlay;
use uli_lite_gba::{ClassTables, GbaDecoder, HotState};

/// The committed Phase 0 corpus, byte-for-byte. It is the regression pin: the
/// fixture is C-pinned on **both** outputs, and every product number in the plan
/// of record was measured on it.
// Halfword-aligned: the product bit reader uses aligned `u16` loads (one
// `ldrh` per refill). See `uli_lite::Aligned`.
#[cfg(all(
    not(feature = "album-corpus"),
    not(feature = "adversarial-corpus"),
    not(feature = "cross-corpus")
))]
static ALBUM_STORAGE: uli_lite::Aligned<
    { include_bytes!("../../../crates/uli-lite/tests/fixtures/uli2_tiny.uli").len() },
> = uli_lite::Aligned(*include_bytes!(
    "../../../crates/uli-lite/tests/fixtures/uli2_tiny.uli"
));
#[cfg(all(
    not(feature = "album-corpus"),
    not(feature = "adversarial-corpus"),
    not(feature = "cross-corpus")
))]
static ALBUM: &[u8] = &ALBUM_STORAGE.0;

/// PR E's corpus: `assets/sound/bioluminescent.wav` (the tracked reference
/// master, 258.61 s) encoded at the **product configuration** — 32,768 Hz, hop
/// 512, `--cap 74`, budget scaled to the product's ~102 kbit/s — by the C
/// encoder of record, with `make uli-album-asset`.
///
/// Both the blob and its pin module are **build output** (gitignored): the
/// tracked input is the WAV, the recipe is the make target, and the pin is
/// C-sourced (`scripts/uli_album_pin.py` reads the C reference decoder's own
/// `hash_of_frame_hashes`).
#[cfg(feature = "cross-corpus")]
static ALBUM_CROSS_BLOB: uli_lite::Aligned<{ include_bytes!("../assets/cross/cross.uli").len() }> =
    uli_lite::Aligned(*include_bytes!("../assets/cross/cross.uli"));
#[cfg(feature = "cross-corpus")]
static ALBUM: &[u8] = &ALBUM_CROSS_BLOB.0;
#[cfg(all(feature = "adversarial-corpus", not(feature = "cross-corpus")))]
static ALBUM_ADV_BLOB: uli_lite::Aligned<{ include_bytes!("../assets/adv.uli").len() }> =
    uli_lite::Aligned(*include_bytes!("../assets/adv.uli"));
#[cfg(all(feature = "adversarial-corpus", not(feature = "cross-corpus")))]
static ALBUM: &[u8] = &ALBUM_ADV_BLOB.0;
#[cfg(all(feature = "album-corpus", not(feature = "adversarial-corpus")))]
static ALBUM_MASTER_BLOB: uli_lite::Aligned<{ include_bytes!("../assets/biomaster.uli").len() }> =
    uli_lite::Aligned(*include_bytes!("../assets/biomaster.uli"));
#[cfg(all(feature = "album-corpus", not(feature = "adversarial-corpus")))]
static ALBUM: &[u8] = &ALBUM_MASTER_BLOB.0;

/// The C decoder's pins for the album corpus, generated by
/// `scripts/uli_album_pin.py`. `include!` (not `#[path] mod`) on purpose: the
/// file does not exist in a fresh clone, and a missing `include!` target only
/// matters to a build that asked for the feature, never to `cargo fmt`.
#[cfg(all(feature = "album-corpus", not(feature = "adversarial-corpus")))]
mod album_pin {
    include!("album_pin.rs");
}
#[cfg(feature = "cross-corpus")]
mod album_pin {
    include!("album_pin_cross.rs");
}
#[cfg(all(feature = "adversarial-corpus", not(feature = "cross-corpus")))]
mod album_pin {
    include!("album_pin_adv.rs");
}

/// Caller-owned decoder state, in EWRAM (the ~100 KiB Huffman LUTs dominate).
#[unsafe(link_section = ".ewram")]
static mut STATE: DecoderState = DecoderState::new();

/// The product decoder's code tables (~100 KiB) belong in EWRAM.
#[unsafe(link_section = ".ewram")]
static mut PRODUCT_TABLES: CodeTables = CodeTables::new();

/// The product decoder's hot transform/FFT/overlap buffers (8,192 B) — **in
/// IWRAM**, matching the C reference (`uli_g_X`/`uli_g_buf`/`uli_g_P` are all
/// IWRAM statics there). Coefficient writes and the M/S pass are the hottest
/// memory traffic in the decoder, and EWRAM charges 16-bit wait states on every
/// one of them (C-3).
///
/// This was EWRAM until 2026-11-15. The earlier IWRAM attempt (PR B) hung on
/// target because the *stack* was believed to need ~15 KiB; that figure was an
/// artifact of stack-local twin buffers and is now **measured at 768 B**
/// (`stack prod high-water` below), so the 8,192 B fits with ~1.1 KiB to spare
/// once the superseded v1/v2 FFT kernels stopped consuming IWRAM.
#[unsafe(link_section = ".iwram")]
static mut PRODUCT_HOT: HotState = HotState::new();

/// The per-class decode tables (560 B) — **also in IWRAM**. `rq[m]` is read on
/// every nonzero coefficient (~1,232/frame), so EWRAM's 4 cycles against IWRAM's
/// 1 was a per-coefficient tax; the reference keeps exactly this slice in
/// `uli_g`. Measured −897 mean / −2,498 at the densest frame. The ~96 KiB of
/// Huffman LUTs stay in `PRODUCT_TABLES` (EWRAM).
#[unsafe(link_section = ".iwram")]
static mut PRODUCT_CLASS: ClassTables = ClassTables::new();

/// Shared output buffers for the measurement passes. Keeping them out of the
/// stack matters: the product code and its 8 KiB hot state occupy most of
/// IWRAM, and the bench's ~3 KiB per-pass arrays pushed the stack into the
/// hot state (PR B's first on-target run did exactly that).
#[unsafe(link_section = ".ewram")]
static mut OUT8L: [i8; M] = [0; M];
#[unsafe(link_section = ".ewram")]
static mut OUT8R: [i8; M] = [0; M];
#[unsafe(link_section = ".ewram")]
static mut OUT16L: [i16; M] = [0; M];
#[unsafe(link_section = ".ewram")]
static mut OUT16R: [i16; M] = [0; M];

/// Kernel inputs/outputs live in EWRAM here. The overlay's job is to move the
/// hot ones to IWRAM; this arm deliberately keeps the as-shipped placement.
///
/// NOTE (they look like scalars, they are blocks): `M = 512`, `H = 256`, so
/// `[i32; M]` is **2,048 B**, `[i32; 2*H]` is **2,048 B**, `[i16; M]` is
/// 1,024 B, `[i8; M]` is 512 B. The six `K*` buffers below are **8,704 B** in
/// total, the ten `T_*` twins below them **13,312 B**, plus `ZWANT`/`ZWORK`
/// 4,096 B and the four `OUT*` pins 3,072 B — **29,184 B of EWRAM arrays**,
/// none of it small. They cannot go in IWRAM: the product hot path needs
/// ~15 KiB of stack there, `.iwram` is 32,512 B usable of which ~13.9 KiB is
/// code, which leaves only ~3.6 KiB genuinely free — less headroom than any
/// single block below needs. EWRAM is the correct home for all of them, and
/// the real IWRAM win is the product's `HotState`/coefficient buffers
/// (PLAN-OF-RECORD.md C-3), which is blocked on the same stack.
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
// Twin work buffers (EWRAM, not stack): the product-path decode needs ~15 KiB of
// stack in .iwram, and the twin's buffers used to be locals in `run()`; putting
// them on the stack overflowed it once the asm kernels grew `.iwram` (the PR
// B/C-1 stack class). Statics keep `run()`'s frame tiny.
#[unsafe(link_section = ".ewram")]
static mut ZWANT: [i32; 2 * H] = [0; 2 * H];
#[unsafe(link_section = ".ewram")]
static mut ZWORK: [i32; 2 * H] = [0; 2 * H];
#[unsafe(link_section = ".ewram")]
static mut T_ZC: [i32; 2 * H] = [0; 2 * H];
#[unsafe(link_section = ".ewram")]
static mut T_ZO: [i32; 2 * H] = [0; 2 * H];
#[unsafe(link_section = ".ewram")]
static mut T_YC: [i32; M] = [0; M];
#[unsafe(link_section = ".ewram")]
static mut T_YO: [i32; M] = [0; M];
#[unsafe(link_section = ".ewram")]
static mut T_PC: [i32; H] = [0; H];
#[unsafe(link_section = ".ewram")]
static mut T_PO: [i32; H] = [0; H];
#[unsafe(link_section = ".ewram")]
static mut T_C8: [i8; M] = [0; M];
#[unsafe(link_section = ".ewram")]
static mut T_CO: [i8; M] = [0; M];
#[unsafe(link_section = ".ewram")]
static mut T_C16: [i16; M] = [0; M];
#[unsafe(link_section = ".ewram")]
static mut T_O16: [i16; M] = [0; M];
// M/S twin pair (root-cause audit, 2026-11-16): the Rust loop and the
// block-transfer asm kernel are run on identical inputs and compared. Each is
// `[i32; M]` = 2,048 B, so both live in EWRAM with the other twins.
#[unsafe(link_section = ".ewram")]
static mut T_ML: [i32; M] = [0; M];
#[unsafe(link_section = ".ewram")]
static mut T_MR: [i32; M] = [0; M];
#[unsafe(link_section = ".ewram")]
static mut T_MLO: [i32; M] = [0; M];
#[unsafe(link_section = ".ewram")]
static mut T_MRO: [i32; M] = [0; M];

/// The FFT twiddle table copied into `.iwram`, for the same-image placement
/// A/B: the kernel takes its table pointer as an argument, so one build can
/// time v4 against the cartridge-resident `TW` and against this `.iwram` copy.
/// `.iwram` data is initialised from ROM at boot by the agb startup copy, the
/// same mechanism the `iwram-tables` feature relies on.
///
/// **Off by default, and 2,016 B is why.** Since `iwram-tables` became the
/// product default, `crate::TW` is *already* in `.iwram`, so this copy compares
/// the table against itself and the arm has nothing left to show. It also cost
/// the `--features asm-fft` image its whole budget: that build measured
/// 30,284 B of a 30,320 B region, i.e. no room for the 828 B stack, so the
/// on-target asm twin could not run at all. Gating it here is what makes
/// `make uli-fft-twin` possible.
#[cfg(feature = "tw-iwram-arm")]
#[unsafe(link_section = ".iwram")]
static mut TW_IWRAM: [i32; 504] = uli_lite::tables_data::TW;

/// The product frame period: 512 samples at 32,768 Hz on the 2^24 Hz clock.
/// `512 * 2^24 / 32768 = 262144` cycles. Compile-time arithmetic, no divide on
/// the measured path.
const FRAME_BUDGET_CYCLES: u32 = 262_144;

/// The by-difference attribution arm this image was built with (0 = the product
/// frame). Every log prints it, so a timing-only arm cannot be mistaken for the
/// product number later.
const ATTRIB_MASK: u32 = cfg!(feature = "attrib-pre") as u32
    | (cfg!(feature = "attrib-fft") as u32) << 1
    | (cfg!(feature = "attrib-post") as u32) << 2
    | (cfg!(feature = "attrib-ola") as u32) << 3
    | (cfg!(feature = "attrib-entropy") as u32) << 4
    | (cfg!(feature = "attrib-ms") as u32) << 5
    | (cfg!(feature = "attrib-synth") as u32) << 6
    | (cfg!(feature = "attrib-entropy-nolut") as u32) << 7
    | (cfg!(feature = "attrib-entropy-nodequant") as u32) << 8
    | (cfg!(feature = "attrib-entropy-nowrite") as u32) << 9
    | (cfg!(feature = "attrib-entropy-reader") as u32) << 10;

/// The sub-frame arm this image carries, named so a log is self-describing.
const SUBFRAME_ARM_NAME: &str = if cfg!(feature = "attrib-entropy-reader") {
    "sub=reader"
} else if cfg!(feature = "attrib-entropy-nolut") {
    "sub=nolut"
} else if cfg!(feature = "attrib-entropy-nodequant") {
    "sub=nodequant"
} else if cfg!(feature = "attrib-entropy-nowrite") {
    "sub=nowrite"
} else {
    "sub=none"
};

/// Per-frame bit counts from the control pass, replayed by the
/// `attrib-entropy-reader` arm so the reader's window mechanics can be timed at
/// the true bit volume (`uli_lite_gba::GbaDecoder::replay_bits`). Filled from
/// the portable decoder's `FrameInfo::bits`, which is a property of the album,
/// not of the arm.
#[cfg(feature = "attrib-entropy-reader")]
const MAX_FRAMES: usize = 512;
#[cfg(feature = "attrib-entropy-reader")]
static mut FRAME_BITS: [u32; MAX_FRAMES] = [0; MAX_FRAMES];

/// Read one frame's recorded bit count (only the reader arm uses this).
#[cfg(feature = "attrib-entropy-reader")]
fn frame_bits(f: u32) -> u32 {
    if (f as usize) >= MAX_FRAMES {
        return 0;
    }
    // SAFETY: single-threaded, index bounded above, no reference escapes.
    unsafe {
        core::ptr::read_volatile(
            core::ptr::addr_of!(FRAME_BITS)
                .cast::<u32>()
                .add(f as usize),
        )
    }
}

/// Record one frame's bit count from the control pass.
#[cfg(feature = "attrib-entropy-reader")]
fn set_frame_bits(f: u32, bits: u32) {
    if (f as usize) >= MAX_FRAMES {
        return;
    }
    // SAFETY: as above.
    unsafe {
        core::ptr::write_volatile(
            core::ptr::addr_of_mut!(FRAME_BITS)
                .cast::<u32>()
                .add(f as usize),
            bits,
        )
    }
}

/// FNV-1a 32 of the C reference's per-frame int8 hashes, in frame order.
#[cfg(all(
    not(feature = "album-corpus"),
    not(feature = "adversarial-corpus"),
    not(feature = "cross-corpus")
))]
const EXPECTED_INT8: u32 = 0xe6e8_7652;
#[cfg(any(
    feature = "album-corpus",
    feature = "adversarial-corpus",
    feature = "cross-corpus"
))]
const EXPECTED_INT8: u32 = album_pin::EXPECTED_INT8;

/// FNV-1a 32 of the C reference's per-frame int16 hashes, when the corpus has
/// one. The 129-frame fixture is C-pinned on both outputs. PR E's album corpus
/// is C-pinned on **int8** only: the C decoder publishes an int8 hash
/// catalogue (`vectors.txt`), and its PCM16 output is per-track rather than
/// per-frame, so the album's int16 hash is printed and recorded here — never
/// silently promoted to a pin.
#[cfg(all(
    not(feature = "album-corpus"),
    not(feature = "adversarial-corpus"),
    not(feature = "cross-corpus")
))]
const INT16_PIN: Option<u32> = Some(0xb353_0978);
#[cfg(any(
    feature = "album-corpus",
    feature = "adversarial-corpus",
    feature = "cross-corpus"
))]
const INT16_PIN: Option<u32> = None;

/// Appended to every pin verdict so an unpinned int16 cannot read as a pass.
const PIN16_NOTE: &str = if INT16_PIN.is_none() {
    " int16-unpinned"
} else {
    ""
};

/// True when the int16 album hash is either the C pin or (album corpus) not
/// pinned at all — the verdict must never turn a missing pin into evidence.
fn int16_ok(h: u32) -> bool {
    match INT16_PIN {
        Some(pin) => h == pin,
        None => true,
    }
}

/// PR E's gate-total arm: the product decode with the real output stage (ring,
/// timer IRQ, FIFO DMA) running inside the measured window. Only compiled when
/// asked for, because it is a witness and not part of the decode-wall record.
#[cfg(feature = "gate")]
mod gate;

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
    /// Windows measured since the last zeroing. See [`Counter::arm`].
    windows: u32,
}

/// How many windows may be measured before the composite is zeroed again. The
/// counter wraps every 256 s of emulated time (2^32 cycles at 16.78 MHz) and a
/// pass over PR E's album corpus is longer than that in the slower kernel sets
/// (~1,280 s for the portable-core arm), so zeroing only between passes is not
/// enough. 1,024 windows is ~79 s even in the slowest arm — a quarter of the
/// wrap — and a window never spans a zeroing, because it happens before the
/// start read.
const WINDOWS_PER_RESET: u32 = 1024;

impl Counter<'_> {
    /// The cycle counter from an explicit pair of timers (timer2/timer3 in this
    /// image). A split constructor because PR E's gate arm also needs timer0 and
    /// timer1, and a single `&mut Timers` cannot be shared with them.
    fn start_split<'a>(
        low: &'a mut agb::timer::Timer,
        high: &'a mut agb::timer::Timer,
    ) -> Counter<'a> {
        low.set_divider(Divider::Divider1).set_enabled(true);
        high.set_cascade(true)
            .set_interrupt(false)
            .set_enabled(true);
        let mut counter = Counter {
            low,
            high,
            windows: 0,
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
        self.windows = 0;
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

/// Longest window a real frame can plausibly take: 64 frame periods. This is a
/// *counter-integrity* bound, not a speed one — the control arms in this image
/// are legitimately several periods per frame (the portable core is ~10), so a
/// tight bound would fire on honest work. A torn or crossed 32-bit read shows up
/// at the 2^16·2^16 or 2^32 scale, four orders of magnitude above this.
const SUSPECT_CYCLES: u32 = 1 << 24;

/// Saturating window difference. A wrapped window is reported as `u32::MAX`
/// and fails the verdict; it is never silently truncated.
fn cycles_between(start: u32, end: u32) -> u32 {
    if end >= start { end - start } else { u32::MAX }
}

/// Run `work` inside one measurement window with IRQs off.
fn timed<T>(counter: &mut Counter<'_>, work: impl FnOnce() -> T) -> (T, u32) {
    // Zero the composite on a fixed cadence so no window can cross the 32-bit
    // wrap (the long corpus made this necessary; a crossed wrap read as a
    // 4,294,967,xxx-cycle "frame" and would have inflated `max` and `sum`).
    if counter.windows >= WINDOWS_PER_RESET {
        counter.soft_reset();
    }
    counter.windows += 1;
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

/// Like [`timed`], but the window runs with **interrupts live**. PR E's gate arm
/// is the only caller: the feeder's IRQ and the sound DMA have to land inside the
/// frame's cost, or the arm measures the decode wall a second time. The read
/// protocol's low/high/low stability check is what keeps a preempted read safe.
fn timed_irq<T>(counter: &mut Counter<'_>, work: impl FnOnce() -> T) -> (T, u32) {
    if counter.windows >= WINDOWS_PER_RESET {
        counter.soft_reset();
    }
    counter.windows += 1;
    let start = counter.read();
    let out = work();
    let end = counter.read();
    (out, cycles_between(start, end))
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
    /// Windows longer than [`SUSPECT_CYCLES`]: a torn or crossed 32-bit
    /// counter read, never a real frame. A witness at 16,553 frames needs a
    /// pass of ~217 s of emulated time against the counter's 256 s wrap, so a
    /// long corpus is exactly when this fires; it fails the verdict rather than
    /// inflating `max`/`sum`.
    suspect: u32,
    suspect_frame: u32,
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
            suspect: 0,
            suspect_frame: 0,
            worst_ops: uli_lite::Ops::ZERO,
            worst_model: 0,
        }
    }

    fn observe(&mut self, frame: u32, net: u32, ops: uli_lite::Ops) {
        self.count += 1;
        self.sum += u64::from(net);
        self.wrapped |= net == u32::MAX;
        if net > SUSPECT_CYCLES {
            self.suspect += 1;
            self.suspect_frame = frame;
        }
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

/// Read the stack pointer.
#[inline(always)]
fn sp_now() -> usize {
    let r: usize;
    // SAFETY: reading sp has no side effects.
    unsafe { core::arch::asm!("mov {}, sp", out(reg) r) };
    r
}

/// IWRAM stack high-water probe. Fills the free stack region below the current
/// `sp` with a sentinel; after the work under test runs, the lowest address
/// still holding the sentinel marks the deepest stack touch. Replaces the stale
/// "~15 KiB product stack" figure with a measurement.
const STACK_SENTINEL: u32 = 0xA5A5_A5A5;

unsafe extern "C" {
    static __iwram_end: u8;
}

#[inline(never)]
fn stack_probe_fill() -> (usize, usize) {
    // SAFETY: writes only to the region strictly below the current sp, which the
    // ARM ABI leaves free for callees; IRQs are off in the measured windows.
    unsafe {
        let lo = core::ptr::addr_of!(__iwram_end) as usize;
        let hi = sp_now() & !3;
        let mut p = lo;
        while p + 4 <= hi {
            core::ptr::write_volatile(p as *mut u32, STACK_SENTINEL);
            p += 4;
        }
        (lo, hi)
    }
}

/// Bytes of stack used below the fill ceiling, given the fill's (lo, hi).
#[inline(never)]
fn stack_probe_used(lo: usize, hi: usize) -> usize {
    // SAFETY: reads the same region the fill wrote.
    unsafe {
        let mut p = lo;
        while p + 4 <= hi {
            if core::ptr::read_volatile(p as *const u32) != STACK_SENTINEL {
                return hi - p;
            }
            p += 4;
        }
    }
    0
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
#[inline(never)]
fn timed_pass<K: uli_lite::Kernels>(
    album: Album<'_>,
    state: &mut DecoderState,
    counter: &mut Counter<'_>,
    overhead: u32,
    l8: &mut [i8; M],
    r8: &mut [i8; M],
    l16: &mut [i16; M],
    r16: &mut [i16; M],
) -> Result<(FrameCost, u32, u32), uli_lite::Error> {
    let mut decoder = Decoder::<K>::with_kernels(album, state)?;
    // The 32-bit composite wraps every 256 s of emulated time; one
    // pass over the album corpus is ~217 s, so a pass must start from zero.
    counter.soft_reset();
    let mut cost = FrameCost::new();
    let mut all8 = 2_166_136_261u32;
    let mut all16 = 2_166_136_261u32;
    for f in 0..album.n_frames() {
        let (res, raw) = timed(counter, || {
            decoder.decode_frame(l8, r8, Some(l16), Some(r16))
        });
        let info = res?;
        // The reader-only arm replays the *true* per-frame bit count. This pass
        // is the portable decoder, so the count is the album's, not the arm's.
        #[cfg(feature = "attrib-entropy-reader")]
        set_frame_bits(f, info.bits);
        #[cfg(not(feature = "attrib-entropy-reader"))]
        let _ = &info;
        // Hash outside the window: proves the timed decode is the pinned
        // decode, not just a decode that consumed the same number of cycles.
        all8 = fnv_bytes(all8, &fnv_i8(2_166_136_261, l8, r8).to_le_bytes());
        all16 = fnv_bytes(all16, &fnv_i16(2_166_136_261, l16, r16).to_le_bytes());
        let net = raw.saturating_sub(overhead);
        cost.observe(f, net, decoder.state().ops());
    }
    Ok((cost, all8, all16))
}

/// One full-album decode through the GBA-first product decoder: ARM-mode,
/// unchecked hot path, 32-bit bit reader, product tables in EWRAM and hot
/// buffers in IWRAM.
#[inline(never)]
/// The production (headphone) product pass: **int8 only**, no int16 buffer.
/// `ola_kernel` therefore takes the fused int8 path and the decoder never pays
/// for the pre-DAC int16 write. This is C-3d's "one output format per arm":
/// the dual-output pass below exists to pin both hashes, not because the product
/// writes both.
fn product_pass_s8(
    album: Album<'_>,
    tables: &mut CodeTables,
    class: &mut ClassTables,
    hot: &mut HotState,
    counter: &mut Counter<'_>,
    overhead: u32,
    l8: &mut [i8; M],
    r8: &mut [i8; M],
) -> Result<(FrameCost, u32), uli_lite::Error> {
    let mut decoder = GbaDecoder::new(album, tables, class, hot)?;
    // The 32-bit composite wraps every 256 s of emulated time; one
    // pass over the album corpus is ~217 s, so a pass must start from zero.
    counter.soft_reset();
    let mut cost = FrameCost::new();
    let mut all8 = 2_166_136_261u32;
    for f in 0..album.n_frames() {
        #[cfg(feature = "attrib-entropy-reader")]
        decoder.replay_bits(frame_bits(f));
        let (res, raw) = timed(counter, || decoder.decode_frame(l8, r8, None, None));
        res?;
        all8 = fnv_bytes(all8, &fnv_i8(2_166_136_261, l8, r8).to_le_bytes());
        let net = raw.saturating_sub(overhead);
        cost.observe(f, net, decoder.ops());
        // Attribution dump (see the `attrib-dump` feature): printed outside the
        // window, so the number is the same one the summary reports. This is
        // what makes a by-difference arm comparable at a *given* frame index
        // instead of only in the mean.
        #[cfg(feature = "attrib-dump")]
        agb::println!("[uli-bench] pfc f={} cyc={}", f, net);
    }
    Ok((cost, all8))
}

fn product_pass(
    album: Album<'_>,
    tables: &mut CodeTables,
    class: &mut ClassTables,
    hot: &mut HotState,
    counter: &mut Counter<'_>,
    overhead: u32,
    l8: &mut [i8; M],
    r8: &mut [i8; M],
    l16: &mut [i16; M],
    r16: &mut [i16; M],
) -> Result<(FrameCost, u32, u32), uli_lite::Error> {
    let mut decoder = GbaDecoder::new(album, tables, class, hot)?;
    // The 32-bit composite wraps every 256 s of emulated time; one
    // pass over the album corpus is ~217 s, so a pass must start from zero.
    counter.soft_reset();
    let mut cost = FrameCost::new();
    let mut all8 = 2_166_136_261u32;
    let mut all16 = 2_166_136_261u32;
    for f in 0..album.n_frames() {
        let (res, raw) = timed(counter, || {
            decoder.decode_frame(l8, r8, Some(l16), Some(r16))
        });
        res?;
        all8 = fnv_bytes(all8, &fnv_i8(2_166_136_261, l8, r8).to_le_bytes());
        all16 = fnv_bytes(all16, &fnv_i16(2_166_136_261, l16, r16).to_le_bytes());
        let net = raw.saturating_sub(overhead);
        cost.observe(f, net, decoder.ops());
        #[cfg(feature = "per-frame")]
        {
            // Attribution rows, printed outside the window. `cyc` already has
            // the calibration overhead subtracted, so a host-side fit sees the
            // same number the summary reports.
            let o = decoder.ops();
            agb::println!(
                "[uli-bench] pf f={} cyc={} bandsyms={} coded={} pairs={} quads={} nz={} zc={} raws={} escs={} ms={} long={}",
                f,
                net,
                o.bandsyms,
                o.coded_bands,
                o.pairs,
                o.quads,
                o.nz,
                o.zc,
                o.raws,
                o.escs,
                o.ms,
                o.long_codes
            );
        }
    }
    Ok((cost, all8, all16))
}

fn print_cost(label: &str, cost: &FrameCost, counted: bool) {
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
    if cost.suspect > 0 {
        agb::println!(
            "[uli-bench] {} INSTRUMENT: {} window(s) over {} cycles (first at frame {}): torn/wrapped counter, not a frame",
            label,
            cost.suspect,
            SUSPECT_CYCLES,
            cost.suspect_frame
        );
    }
    if !counted {
        agb::println!(
            "[uli-bench] {} worst ops: not counted in a product build (rebuild with --features verify)",
            label
        );
        return;
    }
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

#[cfg(not(feature = "play-only"))]
fn run(gba: &mut agb::Gba) -> bool {
    // Whole-instrument stack high-water probe: fill the free IWRAM below this
    // frame and report the deepest touch at the end of the run, so the budget is
    // sized from the instrument's real maximum rather than one pass's.
    let (run_lo, run_hi) = stack_probe_fill();
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
    agb::println!(
        "[uli-bench] attrib mask=0x{:x} pre={} fft={} post={} ola={} synth={} entropy={} ms={} {}",
        ATTRIB_MASK,
        cfg!(feature = "attrib-pre"),
        cfg!(feature = "attrib-fft"),
        cfg!(feature = "attrib-post"),
        cfg!(feature = "attrib-ola"),
        cfg!(feature = "attrib-synth"),
        cfg!(feature = "attrib-entropy"),
        cfg!(feature = "attrib-ms"),
        SUBFRAME_ARM_NAME
    );
    if ATTRIB_MASK != 0 {
        agb::println!(
            "[uli-bench] ATTRIBUTION ARM: pins fail by construction; quote only the timed window"
        );
    }

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
    // Which corpus this image measures. The fixture is the regression pin; the
    // album is PR E's tail witness. Logs must say which, because every absolute
    // number in them is corpus-specific.
    #[cfg(all(not(feature = "album-corpus"), not(feature = "adversarial-corpus")))]
    agb::println!("[uli-bench] corpus=fixture (Phase 0, 129 frames, C-pinned int8+int16)");
    #[cfg(feature = "cross-corpus")]
    agb::println!(
        "[uli-bench] corpus=cross (E3-b: the sibling 12-track album) pin_frames={} int8_pin={:08x}",
        album_pin::N_FRAMES,
        EXPECTED_INT8
    );
    #[cfg(feature = "adversarial-corpus")]
    agb::println!(
        "[uli-bench] corpus=adversarial (E3-a: synthetic density set) pin_frames={} int8_pin={:08x} int16_pin=none",
        album_pin::N_FRAMES,
        EXPECTED_INT8
    );
    #[cfg(all(feature = "album-corpus", not(feature = "adversarial-corpus")))]
    {
        agb::println!(
            "[uli-bench] corpus=album (PR E: tracked master at 32768 Hz, product config) pin_frames={} int8_pin={:08x} int16_pin=none",
            album_pin::N_FRAMES,
            EXPECTED_INT8
        );
        if album.n_frames() != album_pin::N_FRAMES {
            agb::println!(
                "[uli-bench] FAIL album frames={} but the C pin covers {}",
                album.n_frames(),
                album_pin::N_FRAMES
            );
            return false;
        }
    }
    // The reader-replay arm re-times the bit window at the true bit volume from
    // a fixed 512-entry table. It is a timing-only attribution arm for the
    // 129-frame fixture; on a longer corpus it has nowhere to put the volume,
    // so it refuses rather than measuring zeros (a stale table entry would be
    // indistinguishable from a frame that needs no bits).
    #[cfg(feature = "attrib-entropy-reader")]
    if album.n_frames() as usize > MAX_FRAMES {
        agb::println!(
            "[uli-bench] FAIL attrib-entropy-reader needs n_frames <= {} (table), album has {}: rerun this arm without the album corpus",
            MAX_FRAMES,
            album.n_frames()
        );
        return false;
    }

    let state = unsafe { &mut *core::ptr::addr_of_mut!(STATE) };
    let l8 = unsafe { &mut *core::ptr::addr_of_mut!(OUT8L) };
    let r8 = unsafe { &mut *core::ptr::addr_of_mut!(OUT8R) };
    let l16 = unsafe { &mut *core::ptr::addr_of_mut!(OUT16L) };
    let r16 = unsafe { &mut *core::ptr::addr_of_mut!(OUT16R) };

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
        if let Err(e) = decoder.decode_frame(l8, r8, Some(l16), Some(r16)) {
            agb::println!("[uli-bench] FAIL decode frame {}: {:?}", frames, e);
            return false;
        }
        all8 = fnv_bytes(all8, &fnv_i8(2_166_136_261, l8, r8).to_le_bytes());
        all16 = fnv_bytes(all16, &fnv_i16(2_166_136_261, l16, r16).to_le_bytes());
        frames += 1;
    }
    let diag_ok = decoder.state().xover() == 0 && decoder.state().hfnz() == 0;
    let pin_ok = frames == album.n_frames() && all8 == EXPECTED_INT8 && int16_ok(all16) && diag_ok;
    agb::println!(
        "[uli-bench] decode frames={} int8={:08x} (want {:08x}) int16={:08x} xover={} hfnz={} [{}]{}",
        frames,
        all8,
        EXPECTED_INT8,
        all16,
        decoder.state().xover(),
        decoder.state().hfnz(),
        if pin_ok { "PASS" } else { "FAIL" },
        PIN16_NOTE
    );
    if !pin_ok {
        return false;
    }

    // -----------------------------------------------------------------
    // 2. Empty-window calibration + frame timing.
    // -----------------------------------------------------------------
    // All four timers. The cycle counter owns timer2/timer3; PR E's gate arm
    // needs timer0 (the sample clock) and timer1 (the feeder period). `agb`
    // reserves 0/1 for its sound system, which this image does not instantiate —
    // the gate arm drives the sound FIFOs itself, exactly as the product-play ROM
    // does. The two borrows are disjoint fields, which is why the counter takes
    // its timers by reference rather than a `Timers`.
    // SAFETY: no other agb component in this image uses timer0/timer1.
    let mut all_timers = unsafe { gba.timers.all_timers() };
    let mut counter = Counter::start_split(&mut all_timers.timer2, &mut all_timers.timer3);
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
    let (cost_core, core8, core16) = match timed_pass::<uli_lite::PortableKernels>(
        album,
        state,
        &mut counter,
        overhead,
        l8,
        r8,
        l16,
        r16,
    ) {
        Ok(v) => v,
        Err(e) => {
            agb::println!("[uli-bench] FAIL core timed pass: {:?}", e);
            return false;
        }
    };
    let core_pin_ok = core8 == EXPECTED_INT8 && int16_ok(core16);
    print_cost("core", &cost_core, true);
    agb::println!(
        "[uli-bench] core pins int8={:08x} int16={:08x} [{}]{}",
        core8,
        core16,
        if core_pin_ok { "PASS" } else { "FAIL" },
        PIN16_NOTE
    );

    let (cost_ovl, ovl8, ovl16) = match timed_pass::<uli_lite_gba::ArmKernels>(
        album,
        state,
        &mut counter,
        overhead,
        l8,
        r8,
        l16,
        r16,
    ) {
        Ok(v) => v,
        Err(e) => {
            agb::println!("[uli-bench] FAIL overlay timed pass: {:?}", e);
            return false;
        }
    };
    let ovl_pin_ok = ovl8 == EXPECTED_INT8 && int16_ok(ovl16);
    print_cost("ovl", &cost_ovl, true);
    agb::println!(
        "[uli-bench] ovl pins int8={:08x} int16={:08x} [{}]{}",
        ovl8,
        ovl16,
        if ovl_pin_ok { "PASS" } else { "FAIL" },
        PIN16_NOTE
    );

    // PR B: the GBA-first product decoder (ARM/IWRAM hot path, 32-bit bit
    // reader, unchecked Huffman/dequant/M/S) with its tables in EWRAM and its
    // hot buffers in IWRAM.
    let product_tables = unsafe { &mut *core::ptr::addr_of_mut!(PRODUCT_TABLES) };
    let product_class = unsafe { &mut *core::ptr::addr_of_mut!(PRODUCT_CLASS) };
    let product_hot = unsafe { &mut *core::ptr::addr_of_mut!(PRODUCT_HOT) };
    let (probe_lo, probe_hi) = stack_probe_fill();
    let (cost_prod, prod8, prod16) = match product_pass(
        album,
        product_tables,
        product_class,
        product_hot,
        &mut counter,
        overhead,
        l8,
        r8,
        l16,
        r16,
    ) {
        Ok(v) => v,
        Err(e) => {
            agb::println!("[uli-bench] FAIL product timed pass: {:?}", e);
            return false;
        }
    };
    let prod_pin_ok = prod8 == EXPECTED_INT8 && int16_ok(prod16);

    // C-3d: the production (int8-only) product pass. Same decoder, same frames,
    // but no int16 buffer, so OLA takes the fused int8 path. Its int8 hash must
    // equal the dual pass's int8 pin - same samples, less work.
    let (cost_prod8, prod8_only) = {
        let (lo, hi) = stack_probe_fill();
        match product_pass_s8(
            album,
            product_tables,
            product_class,
            product_hot,
            &mut counter,
            overhead,
            l8,
            r8,
        ) {
            Ok(v) => {
                let used = stack_probe_used(lo, hi);
                agb::println!("[uli-bench] stack prod8 high-water {} B", used);
                v
            }
            Err(e) => {
                agb::println!("[uli-bench] FAIL int8-only product pass: {:?}", e);
                return false;
            }
        }
    };
    print_cost("prod8", &cost_prod8, cfg!(feature = "verify"));
    // `attrib-count` only: how many symbols took the canonical search off the
    // LUT, summed over every pass in this image. 0 in a build without it.
    agb::println!(
        "[uli-bench] prod8 slow_hits={}",
        uli_lite_gba::slow_hit_count()
    );
    agb::println!(
        "[uli-bench] prod8 pin int8={:08x} (want {:08x}) [{}]",
        prod8_only,
        EXPECTED_INT8,
        if prod8_only == EXPECTED_INT8 {
            "PASS"
        } else {
            "FAIL"
        }
    );

    // PR E: the gate total. Same decoder, same frames, int8-only as the product
    // writes them, with the shipping `RingFeed` and the real timebase (timer0
    // sample clock, timer1 feeder period, DMA1/2 on the sound FIFOs) running.
    #[cfg(feature = "gate")]
    let gate_ok = {
        let (lo, hi) = stack_probe_fill();
        match gate::gate_pass(
            album,
            product_tables,
            product_class,
            product_hot,
            &mut counter,
            &mut all_timers.timer0,
            &mut all_timers.timer1,
            overhead,
            l8,
            r8,
        ) {
            Ok(rep) => {
                let used = stack_probe_used(lo, hi);
                agb::println!("[uli-bench] stack gate high-water {} B", used);
                print_cost("gate", &rep.cost, cfg!(feature = "verify"));
                agb::println!(
                    "[uli-bench] gate feeder periods_per_sec={} serviced={} full_takes={} starved_entries={} pushed={} consumed={} ring={} frames_over_period={} worst_over={}",
                    gate::periods_per_sec(),
                    rep.serviced,
                    rep.full_takes,
                    rep.starved_entries,
                    rep.pushed,
                    rep.consumed,
                    rep.pushed - rep.consumed,
                    rep.over_period,
                    rep.over_period_worst
                );
                agb::println!("[uli-bench] gate first_starve_frame={}", rep.first_starve);
                // The default arm does not hash in the paced loop (a player does
                // not), so it has no pin of its own to check: its correctness
                // witness is the untimed passes in this same image. `gate-hash`
                // hashes every frame and can check the album pin directly.
                let pin = if cfg!(feature = "gate-hash") {
                    rep.all8 == EXPECTED_INT8
                } else {
                    true
                };
                if cfg!(feature = "gate-hash") {
                    agb::println!(
                        "[uli-bench] gate pin int8={:08x} (want {:08x}) [{}]",
                        rep.all8,
                        EXPECTED_INT8,
                        if pin { "PASS" } else { "FAIL" }
                    );
                } else {
                    agb::println!(
                        "[uli-bench] gate verify=off (player model: no in-loop hashing; pins witnessed by the untimed passes)"
                    );
                }
                pin && rep.cost.suspect == 0
            }
            Err(e) => {
                agb::println!("[uli-bench] FAIL gate pass: {:?}", e);
                false
            }
        }
    };
    #[cfg(not(feature = "gate"))]
    let gate_ok = true;

    let prod_stack_used = stack_probe_used(probe_lo, probe_hi);
    agb::println!(
        "[uli-bench] stack prod high-water {} B below sp=0x{:08x} (iwram_end=0x{:08x})",
        prod_stack_used,
        probe_hi,
        probe_lo
    );
    print_cost("prod", &cost_prod, cfg!(feature = "verify"));
    agb::println!(
        "[uli-bench] prod pins int8={:08x} int16={:08x} [{}]{}",
        prod8,
        prod16,
        if prod_pin_ok { "PASS" } else { "FAIL" },
        PIN16_NOTE
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
    // C-3d: OLA per output format. The production (headphone) path writes int8
    // only, so `olas8` is the arm the OLA gate should be read against; `olas16`
    // is the pre-DAC profile. The dual `ola` arm above is the bench-pin
    // configuration the C record never timed.
    let (_, ola_dual_sum) = timed(&mut counter, || {
        for _ in 0..KREP {
            overlay::ola(
                core::hint::black_box(&*ky),
                core::hint::black_box(&mut *kp),
                core::hint::black_box(&mut k8[..]),
                Some(core::hint::black_box(&mut k16[..])),
            );
        }
    });
    let (_, ola_s8_sum) = timed(&mut counter, || {
        for _ in 0..KREP {
            overlay::ola_s8(
                core::hint::black_box(&*ky),
                core::hint::black_box(&mut *kp),
                core::hint::black_box(&mut k8[..]),
            );
        }
    });
    let (_, ola_s16_sum) = timed(&mut counter, || {
        for _ in 0..KREP {
            overlay::ola_s16(
                core::hint::black_box(&*ky),
                core::hint::black_box(&mut *kp),
                core::hint::black_box(&mut k16[..]),
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
        && ola_dual_sum != u32::MAX
        && ola_s8_sum != u32::MAX
        && ola_s16_sum != u32::MAX
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
    agb::println!(
        "[uli-bench] ola-fmt n={} dual={} s8={} s16={} (overlay kernel, same family)",
        KREP,
        ola_dual_sum,
        ola_s8_sum,
        ola_s16_sum
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

    // -----------------------------------------------------------------
    // 6. asm FFT arm (PR C-1, `asm-fft` feature): the hand-written ARM FFT in
    //    `.iwram`, timed in isolation against the same-shape Rust ARM FFT, and
    //    twinned on the ARM7TDMI. The twin below is the witness that licenses
    //    quoting this cycle number.
    // -----------------------------------------------------------------
    #[cfg(feature = "asm-fft")]
    let asm_ok: bool = {
        let (_, asm_fft_sum) = timed(&mut counter, || {
            for _ in 0..KREP {
                overlay::fft_asm_call(core::hint::black_box(&mut *kz));
            }
        });
        agb::println!(
            "[uli-bench] asmfft n={} fft_sum={} wrapped={}",
            KREP,
            asm_fft_sum,
            asm_fft_sum == u32::MAX
        );
        let (_, asm_fft2_sum) = timed(&mut counter, || {
            for _ in 0..KREP {
                overlay::fft_asm_v2_call(core::hint::black_box(&mut *kz));
            }
        });
        agb::println!(
            "[uli-bench] asmfft2 n={} fft_sum={} wrapped={}",
            KREP,
            asm_fft2_sum,
            asm_fft2_sum == u32::MAX
        );
        let (_, asm_fft3_sum) = timed(&mut counter, || {
            for _ in 0..KREP {
                overlay::fft_asm_v3_call(core::hint::black_box(&mut *kz));
            }
        });
        agb::println!(
            "[uli-bench] asmfft3 n={} fft_sum={} wrapped={}",
            KREP,
            asm_fft3_sum,
            asm_fft3_sum == u32::MAX
        );
        let (_, asm_fft4_sum) = timed(&mut counter, || {
            for _ in 0..KREP {
                overlay::fft_asm_v4_call(core::hint::black_box(&mut *kz));
            }
        });
        agb::println!(
            "[uli-bench] asmfft4 n={} fft_sum={} wrapped={}",
            KREP,
            asm_fft4_sum,
            asm_fft4_sum == u32::MAX
        );
        // v5 (item 1 of the critical path): absolute byte cursor, register-held
        // twiddle pointer and the reference's destructive sum formation. This is
        // the SHIPPED kernel, in `.iwram`; v1..v4 are in `.text` and their
        // numbers here are fetch-polluted ROM readings.
        let (_, asm_fft5_sum) = timed(&mut counter, || {
            for _ in 0..KREP {
                overlay::fft_asm_v5_call(core::hint::black_box(&mut *kz));
            }
        });
        agb::println!(
            "[uli-bench] asmfft5 n={} fft_sum={} wrapped={}",
            KREP,
            asm_fft5_sum,
            asm_fft5_sum == u32::MAX
        );
        // Same kernel, twiddle table copied into `.iwram`: the placement A/B
        // (see TW_IWRAM). Gated, because with `iwram-tables` the shipped table
        // is already there and the 2,016 B copy broke this image's budget.
        #[cfg(feature = "tw-iwram-arm")]
        let asm_fft4iw_sum = {
            let tw_iwram = core::ptr::addr_of!(TW_IWRAM) as *const i32;
            let (_, sum) = timed(&mut counter, || {
                for _ in 0..KREP {
                    overlay::fft_asm_v4_tw_call(core::hint::black_box(&mut *kz), tw_iwram);
                }
            });
            agb::println!(
                "[uli-bench] asmfft4iw n={} fft_sum={} wrapped={}",
                KREP,
                sum,
                sum == u32::MAX
            );
            sum
        };
        #[cfg(not(feature = "tw-iwram-arm"))]
        let asm_fft4iw_sum = { 0u32 };
        asm_fft_sum != u32::MAX
            && asm_fft5_sum != u32::MAX
            && asm_fft2_sum != u32::MAX
            && asm_fft3_sum != u32::MAX
            && asm_fft4_sum != u32::MAX
            && asm_fft4iw_sum != u32::MAX
    };
    #[cfg(not(feature = "asm-fft"))]
    let asm_ok: bool = {
        agb::println!("[uli-bench] asmfft: not in this build (rebuild with --features asm-fft)");
        true
    };

    // On-target twin: run the core (Thumb, checked) and overlay (ARM,
    // unchecked) kernels on identical inputs once, and compare every output.
    // The host test proves the transcription; this proves the *ARM-mode
    // unchecked* build that the timing above actually measured. A timing arm
    // on an image whose overlay arithmetic is not proven would be measuring a
    // possibly different function.
    let zc = unsafe { &mut *core::ptr::addr_of_mut!(T_ZC) };
    let zo = unsafe { &mut *core::ptr::addr_of_mut!(T_ZO) };
    let yc = unsafe { &mut *core::ptr::addr_of_mut!(T_YC) };
    let yo = unsafe { &mut *core::ptr::addr_of_mut!(T_YO) };
    let pc = unsafe { &mut *core::ptr::addr_of_mut!(T_PC) };
    let po = unsafe { &mut *core::ptr::addr_of_mut!(T_PO) };
    let c8 = unsafe { &mut *core::ptr::addr_of_mut!(T_C8) };
    let co = unsafe { &mut *core::ptr::addr_of_mut!(T_CO) };
    let c16 = unsafe { &mut *core::ptr::addr_of_mut!(T_C16) };
    let o16 = unsafe { &mut *core::ptr::addr_of_mut!(T_O16) };
    fill_kx(kx);
    imdct::pre(kx, zc);
    overlay::pre(kx, zo);
    let twin_pre = zc == zo;
    imdct::fft(zc);
    overlay::fft(zo);
    let twin_fft = zc == zo;
    imdct::post(zc, yc);
    overlay::post(zo, yo);
    let twin_post = yc == yo;
    // C-3d: the fused int8/int16 kernels must reproduce the dual kernel's halves
    // exactly. OLA mutates the overlap state `p`, so replay each format from the
    // same starting `p`.
    let po_save = *po;
    imdct::ola(yc, pc, c8, Some(c16));
    overlay::ola(yo, po, co, Some(o16));
    let twin_ola = c8 == co && c16 == o16 && pc == po;

    let kb8 = unsafe { &mut *core::ptr::addr_of_mut!(K8L) };
    let kb16 = unsafe { &mut *core::ptr::addr_of_mut!(K16L) };
    *po = po_save;
    overlay::ola_s8(yo, po, kb8);
    let twin_ola_s8 = c8 == &kb8[..] && &po[..] == &pc[..];
    // The asm overlap-add kernels (asm-ola) must reproduce the Rust ones,
    // including the mutated overlap state: replay each from the same `p`.
    #[cfg(feature = "asm-ola")]
    let twin_ola_asm = {
        let oc8 = unsafe { &mut *core::ptr::addr_of_mut!(K8L) };
        *po = po_save;
        overlay::ola_s8_asm_call(yo, po, oc8);
        let s8_ok = kb8 == oc8 && &po[..] == &pc[..];
        let oc16 = unsafe { &mut *core::ptr::addr_of_mut!(K16L) };
        *po = po_save;
        overlay::ola_s16_asm_call(yo, po, oc16);
        let s16_ok = kb16 == oc16 && &po[..] == &pc[..];
        *po = po_save;
        s8_ok && s16_ok
    };
    #[cfg(not(feature = "asm-ola"))]
    let twin_ola_asm = true;
    *po = po_save;
    overlay::ola_s16(yo, po, kb16);
    let twin_ola_s16 = c16 == &kb16[..] && &po[..] == &pc[..];
    #[cfg(feature = "asm-fft")]
    let twin_fft_asm = {
        // Rust ARM FFT -> want; then each asm variant on the same pre-FFT input.
        let want = unsafe { &mut *core::ptr::addr_of_mut!(ZWANT) };
        let work = unsafe { &mut *core::ptr::addr_of_mut!(ZWORK) };
        overlay::pre(kx, want);
        overlay::fft(want); // Rust ARM-mode FFT
        overlay::pre(kx, work);
        overlay::fft_asm_call(work); // v1
        let v1_ok = *work == *want;
        overlay::pre(kx, work);
        overlay::fft_asm_v2_call(work); // v2
        let v2_ok = *work == *want;
        overlay::pre(kx, work);
        overlay::fft_asm_v3_call(work); // v3
        let v3_ok = *work == *want;
        overlay::pre(kx, work);
        overlay::fft_asm_v4_call(work); // v4
        let v4_ok = *work == *want;
        overlay::pre(kx, work);
        overlay::fft_asm_v5_call(work); // v5 (the shipped kernel)
        let v5_ok = *work == *want;
        v1_ok && v2_ok && v3_ok && v4_ok && v5_ok
    };
    #[cfg(not(feature = "asm-fft"))]
    let twin_fft_asm = true;
    #[cfg(feature = "asm-pre")]
    let twin_pre_asm = {
        use core::ptr::addr_of_mut;
        let want = unsafe { &mut *addr_of_mut!(ZWANT) };
        let work = unsafe { &mut *addr_of_mut!(ZWORK) };
        overlay::pre(kx, want);
        overlay::pre_asm_call(kx, work);
        *work == *want
    };
    #[cfg(not(feature = "asm-pre"))]
    let twin_pre_asm = true;
    #[cfg(feature = "asm-post")]
    let twin_post_asm = {
        use core::ptr::addr_of_mut;
        // `post` reads `z` and writes `y`, so the same input buffer feeds both.
        let want = unsafe { &mut *addr_of_mut!(T_YC) };
        let work = unsafe { &mut *addr_of_mut!(T_YO) };
        overlay::post(kz, want);
        overlay::post_asm_call(kz, work);
        *work == *want
    };
    #[cfg(not(feature = "asm-post"))]
    let twin_post_asm = true;
    #[cfg(feature = "asm-ms")]
    let twin_ms_asm = {
        use core::ptr::addr_of_mut;
        let cl = unsafe { &mut *addr_of_mut!(T_ML) };
        let cr = unsafe { &mut *addr_of_mut!(T_MR) };
        let ol = unsafe { &mut *addr_of_mut!(T_MLO) };
        let or = unsafe { &mut *addr_of_mut!(T_MRO) };
        // A pattern that hits the shapes the transform can get wrong: zero,
        // small positives/negatives, the int32 rails (where `>>1`-style code
        // would differ), and values whose sum wraps.
        let mut i = 0usize;
        while i < M {
            let k = i as i32;
            cl[i] = match i % 8 {
                0 => 0,
                1 => k,
                2 => -k,
                3 => i32::MIN,
                4 => i32::MAX,
                5 => i32::MAX - 1,
                6 => i32::MIN + 1,
                _ => (k.wrapping_mul(37)) ^ 0x5a5a5a5a,
            };
            cr[i] = match i % 8 {
                0 => 0,
                1 => -k,
                2 => k.wrapping_mul(3),
                3 => i32::MAX,
                4 => i32::MIN,
                5 => 1,
                6 => -1,
                _ => (k.wrapping_mul(91)) ^ 0x12345678,
            };
            i += 1;
        }
        // SAFETY: both pairs are M-length rows; M is a multiple of 4.
        unsafe {
            let (l0, r0) = (cl.as_mut_ptr(), cr.as_mut_ptr());
            let (l1, r1) = (ol.as_mut_ptr(), or.as_mut_ptr());
            core::ptr::copy_nonoverlapping(l0, l1, M);
            core::ptr::copy_nonoverlapping(r0, r1, M);
            overlay::ms(l0, r0, M);
            overlay::ms_asm_call(l1, r1, M);
        }
        *ol == *cl && *or == *cr
    };
    #[cfg(not(feature = "asm-ms"))]
    let twin_ms_asm = true;
    let twin_ok = twin_pre
        && twin_fft
        && twin_post
        && twin_ola
        && twin_ola_s8
        && twin_ola_s16
        && twin_fft_asm
        && twin_pre_asm
        && twin_ola_asm
        && twin_ms_asm
        && twin_post_asm;
    agb::println!(
        "[uli-bench] ovl twin pre={} fft={} post={} ola={} ola8={} ola16={} fftasm={} preasm={} olaasm={} msasm={} postasm={} [{}]",
        twin_pre,
        twin_fft,
        twin_post,
        twin_ola,
        twin_ola_s8,
        twin_ola_s16,
        twin_fft_asm,
        twin_pre_asm,
        twin_ola_asm,
        twin_ms_asm,
        twin_post_asm,
        if twin_ok { "PASS" } else { "FAIL" }
    );

    let pass = pin_ok
        && core_pin_ok
        && ovl_pin_ok
        && prod_pin_ok
        && prod8_only == EXPECTED_INT8
        && calib_ok
        && !cost_core.wrapped
        && !cost_ovl.wrapped
        && !cost_prod.wrapped
        && cost_core.suspect == 0
        && cost_ovl.suspect == 0
        && cost_prod.suspect == 0
        && cost_prod8.suspect == 0
        && gate_ok
        && kern_ok
        && ovl_ok
        && asm_ok
        && twin_ok;
    agb::println!(
        "[uli-bench] run stack high-water {} B (iwram_end=0x{:08x}, ceiling=0x{:08x})",
        stack_probe_used(run_lo, run_hi),
        run_lo,
        run_hi
    );
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

/// The **audio-only** ROM (`--features play-only`): one paced pass over the
/// corpus — decode → convert → ring → dual FIFO DMA, with the real 32,768 Hz
/// timebase — and nothing else. No pins pass, no control arms, no attribution,
/// no per-frame hashing. Its runtime is the corpus's own length, and the only
/// outputs are what you hear and a solid screen colour (blue = the feeder
/// finished with no starved period and no counter-integrity fault, red =
/// otherwise).
///
/// It exists because the instrument is a *witness*, and a witness that spends 90
/// minutes decoding the same album five extra ways is not something to flash to
/// answer "does it play". Numbers, if you need them, come from `make uli-gate`.
#[cfg(feature = "play-only")]
fn run(gba: &mut agb::Gba) -> bool {
    // The U4 arm's cartridge timing, so what you hear is what was measured.
    set_waitcnt(0x0000);
    agb::println!(
        "[uli-play] audio-only ROM: blob={} bytes waitcnt=0x{:04X}",
        ALBUM.len(),
        waitcnt()
    );
    let album = match Album::parse(ALBUM) {
        Ok(a) => a,
        Err(e) => {
            agb::println!("[uli-play] FAIL album parse: {:?}", e);
            return false;
        }
    };
    #[cfg(feature = "album-corpus")]
    agb::println!("[uli-play] corpus=album (tracked master at 32,768 Hz, product config)");
    #[cfg(not(feature = "album-corpus"))]
    agb::println!("[uli-play] corpus=fixture (synthetic, C-pinned)");
    agb::println!(
        "[uli-play] frames={} tracks={} rate={} hop={} ({:.2} s)",
        album.n_frames(),
        album.n_tracks(),
        album.header().sample_rate,
        album.header().hop,
        (album.n_frames() as f32 * 512.0) / album.header().sample_rate as f32
    );

    let tables = unsafe { &mut *core::ptr::addr_of_mut!(PRODUCT_TABLES) };
    let class = unsafe { &mut *core::ptr::addr_of_mut!(PRODUCT_CLASS) };
    let hot = unsafe { &mut *core::ptr::addr_of_mut!(PRODUCT_HOT) };
    let l8 = unsafe { &mut *core::ptr::addr_of_mut!(OUT8L) };
    let r8 = unsafe { &mut *core::ptr::addr_of_mut!(OUT8R) };

    // SAFETY: no other agb component in this image uses timer0/timer1 — the
    // sound FIFOs are driven directly here, as in the product-play ROM.
    let mut all_timers = unsafe { gba.timers.all_timers() };
    let mut counter = Counter::start_split(&mut all_timers.timer2, &mut all_timers.timer3);
    // The same empty-window calibration the instrument uses, so both images
    // subtract the same overhead.
    let mut calib = [0u32; 5];
    for slot in calib.iter_mut() {
        let (_, raw) = timed(&mut counter, || {});
        *slot = raw;
    }
    let overhead = calib.iter().copied().min().unwrap_or(0);
    let calib_ok = calib.iter().max().copied().unwrap_or(0) - overhead <= 64;
    agb::println!("[uli-play] calib min={} spread_ok={}", overhead, calib_ok);

    let ok = match gate::gate_pass(
        album,
        tables,
        class,
        hot,
        &mut counter,
        &mut all_timers.timer0,
        &mut all_timers.timer1,
        overhead,
        l8,
        r8,
    ) {
        Ok(rep) => {
            let frames = rep.cost.count;
            let mean = if frames > 0 {
                rep.cost.sum / u64::from(frames)
            } else {
                0
            };
            agb::println!(
                "[uli-play] played frames={} worst={} (f{}) mean={} of {} per frame",
                frames,
                rep.cost.max,
                rep.cost.max_frame,
                mean,
                FRAME_BUDGET_CYCLES
            );
            agb::println!(
                "[uli-play] feeder serviced={} full_takes={} starved={} pushed={} consumed={} ring={} over_period={}",
                rep.serviced,
                rep.full_takes,
                rep.starved_entries,
                rep.pushed,
                rep.consumed,
                rep.pushed - rep.consumed,
                rep.over_period
            );
            calib_ok && rep.starved_entries == 0 && rep.cost.suspect == 0
        }
        Err(e) => {
            agb::println!("[uli-play] FAIL gate pass: {:?}", e);
            false
        }
    };
    agb::println!(
        "[uli-play] {}",
        if ok {
            "PASS: the corpus played to its end with no starved period"
        } else {
            "FAIL: see the feeder line above"
        }
    );
    agb::println!("Tests finished successfully");
    ok
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
