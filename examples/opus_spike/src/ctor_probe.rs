//! Opus spike **constructor-shape probe ROM** — PR 3 harness (OPUS.md, 2026-10-04).
//!
//! WHY THIS ROM EXISTS. The decode-proof ROM (`src/main.rs`) died on-target
//! at decoder construction: region-FNV MATCHed, the walk banner printed, then
//! control flow was destroyed (2026-10-03: mGBA `Jumped to invalid address:
//! F901F900`; 2026-10-04: silent hang, twice — one fault class: a corrupted
//! return path; whether it traps or hangs depends on where the garbage PC
//! lands). A boot log inside the proof image separates only `Err(_)`-return
//! paths; this probe measures below that level, one stage per banner. The
//! image dies at whichever stage kills it, and the LAST printed banner names
//! the killer stage — reading the log tail needs no dependence on the process
//! exiting.
//!
//! MEASURED with this probe's earlier revisions (same tree, same gates):
//! - rev1 (fresh heap): 1 KiB `vec!` alloc/drop **passed** (addr 0x02008370 —
//!   allocator first use, free list, drop/merge alive); `vec![0u8; 200_000]`
//!   **destroyed control flow inside the expression** (jump trap E55EC002) —
//!   unexplained at the time; the fused macro hid alloc vs fill.
//! - rev2: production-shape walk died between the walk banner and packet 1
//!   (jump trap 0x48084808) — fused span: window alloc / decoder alloc /
//!   constructor / first decode.
//! - rev3 (ascending ladder 4 KiB→200 KB, raw `alloc::alloc`, 3-point writes):
//!   ≤ 65,536 B full round-trips **clean** (0x02008370 / 0x02009370 /
//!   0x02011370); **≥ 131,072 B returned NULL — measured, correct
//!   block-allocator behavior after the ascending ladder had bumped ~102 KiB
//!   and freed it**: the merged free chunk (~102 KiB, below the bump top) and
//!   the bump remainder (~126 KiB) are each smaller than the request. The
//!   ladder **fragmented its own heap**, and P2 (which ran after it) died at
//!   the decoder Box OOMing — confounded with capacity, not the construction
//!   shape. Rev3's ordering was a measured mistake; rev4 fixes it.
//! - **Zero `[failed]` lines in every log** although agb's `testing` +
//!   `backtrace` panic handler is compiled in (llvm-nm: `Mgba::print`,
//!   `render_backtrace`, qrcodegen all linked). Under `backtrace`,
//!   `panic_implementation` runs `backtrace::unwind_exception()` — an
//!   r7-chain walk that allocates a `Vec` — **before** the `[failed]` print;
//!   a panic raised while the heap is failing (and/or from the unwinder's
//!   own allocation) plausibly dies before anything prints. Hypothesis, not
//!   measured; the measurement is the absence: **on this image class a
//!   heap-exhaustion panic is indistinguishable from a wild jump**, so the
//!   probe must never OOM at a stage whose death is supposed to mean
//!   something else — every request below must be on-paper satisfiable at
//!   the point it is issued.
//!
//! Rev4 measured (log tail, sha e2addfb1…): **every ladder rung clean**
//! (200,000 → 4,096 B: alloc @0x02008370, 3-point writes, readback, dealloc
//! — free-list reuse/split at scale all good), and **P2 still died between
//! the walk banner and packet 1** (jump trap 0x48084808). That is the
//! localization this probe was built for: on a heap that just served and
//! merged every decoder-scale request cleanly, the production construction
//! shape still destroys control flow before its first callback — heap
//! capacity is measured OUT; the by-value construction (sret temporary in
//! the caller's ~31 KB IWRAM stack frame) is the leading mechanism.
//!
//! REV5 measured (sha 1fb8f668…): **P4 died like TAIL** (jump trap
//! 0x48084808) — caller-side destination propagation does NOT fix the
//! shape: intermediate by-value returns INSIDE the vendored ctor (the
//! sub-decoder constructors building the `Self` literal) survive every
//! caller spelling.
//!
//! REV9 (this revision, frame isolation): rev8 measured P4 `sp@entry =
//! 0x02FC51F0` — still ~241 KB below IWRAM's bottom even though the P4
//! candidate is the in-place shape whose own disassembled frame is
//! `sub sp, #0xb4`. The reading is taken in the ENTRY function's frame:
//! under `lto = "fat"` every stage inlines into main, so main's frame is the
//! union of all of them — including TAIL's by-value repro temporary (~178 KB)
//! — and SP wraps off IWRAM at main's own prologue. That wrap is a loaded
//! gun: every local store from ANY stage lands at 0x02FC_xxxx, which
//! address-aliases into EWRAM (the heap), destroying allocator state long
//! before the stage that owns the temporary runs. Rev8's P4 death was that
//! aliasing, not the candidate. Rev9 pins `#[inline(never)]` on every stage:
//! main's frame stays small, each stage carries its own, and the repro's
//! frame exists only while the repro runs. Reading rule: `sp@P4` inside
//! IWRAM + candidate `ok` + P2 walks green ⇒ the in-place fix holds on
//! target, with TAIL's own-frame death as the same-image negative control.
//!
//! REV8 (2026-10-06): P4 now carries the **landed fix shape**
//! — `probe::new_decoder_in_place` (vendor patch 2: the ctor writes the state
//! through a destination pointer, no by-value hop anywhere). The before/after
//! witness OPUS.md 2026-10-05 designated: P4 SURVIVING (with `sp@P4` inside
//! IWRAM and the box in EWRAM) proves the in-place construction eliminated
//! the frame on target, and TAIL still reproducing the death proves the
//! fault class is the by-value shape, not the build. P2 now walks through
//! the rewired production seam (`walk_region` → `new_decoder_in_place`), so
//! the walk banner → packet 1 span that died on rev4 is the span under test.
//! Rev5's P4 (`new_decoder_boxed`) keeps its measurement as the recorded
//! negative — the function stays in the seam, and TAIL remains the repro
//! control; rev8's expected log: every stage prints through the verdict.
//!
//! REV6/REV7 measured (sha 3b9e4171…, the DECISIVE reading): `read_sp` — an
//! asm SP read, valid because the target spec sets `"frame-pointer":
//! "always"` so SP is never repurposed as a scratch register — printed
//! **SP@entry = 0x02FAF610 in the P4 frame: 0x509F0 ≈ 330 KB below IWRAM's
//! 0x0300_0000 bottom**. The frame holding the construction does not fit
//! IWRAM, let alone the stack: the sret temporary materializes, the frame
//! wraps off IWRAM's bottom into EWRAM (mapped RAM — the writes destroy
//! .bss/heap/allocator state silently, no emulator trap), and the corrupted
//! return path becomes the observed wild jump. **Mechanism proven:** the
//! hang is the by-value construction temporary, heap capacity measured out
//! by the ladder. The fix must construct with no by-value hop anywhere on
//! the path (in-place / boxed ctor in the vendored seam) — the follow-up
//! PR; `new_decoder_boxed` stays as the measured negative.
//!
//! REV5 ORDER (maximize information before the expected death):
//! - **P4 candidate fix shape** (`probe::new_decoder_boxed`): an
//!   `#[inline(always)]` wrapper applying the Box directly to the ctor
//!   result — if destination propagation builds the 177,864 B decoder
//!   straight into the heap allocation, this survives; if the sret temporary
//!   still materializes, it dies exactly like TAIL and the real fix is a
//!   vendor-side boxed constructor. Runs BEFORE TAIL so its answer survives
//!   the expected TAIL death. Host behavior is identical either way — this
//!   is a target-codegen claim witnessed only on-target. REV6 added the SP
//!   witness (`sp@entry` lines at P4 + TAIL) that convicts the stack
//!   directly: inside IWRAM = no big temporary (fault is elsewhere);
//!   below 0x0300_0000 = the sret frame wrapped off IWRAM, mechanism proven.
//! - **TAIL** next (its death is expected and already measured twice on the
//!   proof ROM); P2/P3 keep their rev4 measurements and run after.
//!
//! REV4 ORDER (de-confounded; every stage fits the heap state it sees):
//! - **LADDER (descending)** runs FIRST: 200,000 B on the pristine heap
//!   (~229,664 B free — fits), then each smaller rung re-serves from the
//!   previous rung's freed block (free-list reuse/split at scale). Per rung:
//!   raw `alloc::alloc` (the ADDRESS prints before any write — an OOM here
//!   prints NULL honestly, raw alloc does not panic), first/middle/last byte
//!   writes with banners, readback, dealloc. Answers: allocator-call vs
//!   write-range faults, the size boundary, and rev1's fused-macro death
//!   (if raw 200,000 prints an address, rev1's killer was NOT the raw call).
//! - **P2 production-shape walk** after the ladder's full free: the merged
//!   bottom chunk (~200,000 B) + bump remainder still serve window
//!   (7,680 B) + decoder (177,864 B) on paper, so a death between the walk
//!   banner and packet 1 on this heap is the construction SHAPE (the
//!   sret stack temporary), not capacity.
//! - **P3 fused macro** `vec![0u8; 200_000]` (rev1's killer): green ladder +
//!   green P2 + death here convicts the macro's fused path specifically.
//! - **TAIL** — the 2026-10-03 repro shape `.map(|d| Box::new(d))` verbatim:
//!   the by-value ctor result visibly escaping through a closure, the
//!   documented worst case for destination propagation (a 177,864 B sret
//!   temporary in a ~31 KB IWRAM stack frame). Expected to die; surviving
//!   this build is its own finding (elision changed).
//!
//! WHY A SEPARATE ROM: a ROM that dies can wedge `mgba-test-runner` (it
//! waits for the agb exit signal; a wild-PC loop never sends it) and would
//! make any suite containing it meaningless. This image carries the
//! deliberately-red stages; the proof ROM and the host gate stay
//! green-or-honest. Capture: record sha256, launch runner backgrounded to a
//! log, kill after the tail goes quiet (or mGBA's jump trap exits it).
//!
//! Screen verdict follows the spike convention (blue = ladder + P2 + P3 all
//! passed, red = any check failed); stages that kill the image print their
//! own banner as the last word, so the serial tail discriminates finer than
//! the screen color.

#![cfg_attr(target_arch = "arm", no_std)]
#![cfg_attr(target_arch = "arm", no_main)]

#[cfg(target_arch = "arm")]
extern crate alloc;

#[cfg(target_arch = "arm")]
mod rom {
    use agb::display::Rgb;
    use alloc::alloc::{Layout, alloc, dealloc};
    use alloc::boxed::Box;
    use alloc::vec::Vec;
    use core::ptr::write_volatile;
    use opus_spike::assets::{CLIPS, OpusClip};
    use opus_spike::checksum::{Fnv1a64, fnv1a64, fold_f32_window};
    use opus_spike::probe;

    const COLOR_PENDING: agb::display::Rgb15 = Rgb::new(128, 0, 255).to_rgb15();
    const COLOR_PASS: agb::display::Rgb15 = Rgb::new(0, 0, 255).to_rgb15();
    const COLOR_FAIL: agb::display::Rgb15 = Rgb::new(255, 0, 0).to_rgb15();

    /// EWRAM address range — every heap allocation under agb's global
    /// allocator (EWRAM's block heap) must land here.
    const EWRAM: core::ops::Range<usize> = 0x0200_0000..0x0204_0000;

    /// Ladder sizes, DESCENDING — rev3 measured that ascending fragments the
    /// heap against itself (requests ≥ 131,072 B correctly NULLed after the
    /// ascending bump+free cycling). Descending, every rung fits the previous
    /// rung's freed block, so each rung exercises free-list reuse and split
    /// at scale, and a death names a fault rather than a budget. The exact
    /// decoder-state size (ROM-witnessed thumbv4t figure) rides mid-sweep.
    const LADDER: &[usize] = &[
        200_000, 177_864, // == size_of::<OpusDecoder>() on thumbv4t, ROM-printed
        131_072, 65_536, 32_768, 4_096,
    ];
    const FUSE_BYTES: usize = 200_000;

    #[agb::entry]
    fn main(mut gba: agb::Gba) -> ! {
        agb::eprintln!(
            "[probe] entry started — opus constructor-shape probe (PR 3 harness, 2026-10-06 rev9: P4 = in-place ctor, per-stage frames)"
        );

        let mut gfx = gba.graphics.get();
        gfx.set_background_palette_colour(0, 0, COLOR_PENDING);

        let mut pass = true;

        // LADDER (descending) — raw allocator calls + 3-point writes per
        // size, each rung satisfiable from the freed block above it.
        for size in LADDER {
            pass &= ladder_step(*size);
        }

        // P4 — the candidate fix shape. Runs BEFORE TAIL (rev5): its answer
        // must survive TAIL's expected death.
        let candidate = candidate_shape_probe();
        agb::println!(
            "[probe] P4 READING: candidate boxed-ctor shape {} on-target",
            if candidate {
                "SURVIVED"
            } else {
                "DIED/REJECTED"
            }
        );

        // P2 — production construction shape per arm, on the post-ladder
        // heap. Rev4 measured: dies between banner and packet 1. Rev8:
        // the seam now constructs via vendor patch 2 (in-place, no by-value
        // hop), so this span is the FIX under test — expected: the full walk
        // prints through the golden.
        for clip in CLIPS {
            pass &= walk_probe(clip);
        }

        // P3 — the fused macro path (rev1's killer), last of the probes.
        pass &= fused_vec_probe();

        agb::println!(
            "[probe] verdict {} — screen {} (ladder + P2 walks + P3 fused; P4 candidate reading above; TAIL runs after this line as the repro control)",
            if pass { "PASSED" } else { "FAILED" },
            if pass { "BLUE" } else { "RED" }
        );
        gfx.set_background_palette_colour(0, 0, if pass { COLOR_PASS } else { COLOR_FAIL });

        // TAIL — repro shape, outside the verdict: its expected death is
        // not a probe failure. Rev8 moves it LAST (rev5 ran it between P4
        // and P2 because P4 was the candidate whose answer had to survive
        // the death): now the in-place shape IS the production seam, so
        // every information-bearing stage — P4 survival, P2's full walk on
        // the same heap, the verdict — prints first, and TAIL's death is
        // the log's last word. Same image, same heap, one differential:
        // the in-place construction survives, the by-value repro still dies.
        bisect_tail();

        loop {
            let frame = gfx.frame();
            frame.commit();
        }
    }

    /// One ladder rung: allocate raw (NO fill), print the address BEFORE any
    /// write, then write first/middle/last byte with a banner each, read
    /// back, dealloc. The last banner before silence names the stage:
    /// death before `alloc ok` = the allocator call; at `w0` = bad pointer;
    /// at `wmid`/`wlast` = bad range at that offset; after all writes = the
    /// dealloc/free path. A NULL is printed, never panicked — raw
    /// `alloc::alloc` reports failure as null, and this probe must never
    /// die in an OOM panic at a stage whose death has another meaning.
    fn ladder_step(size: usize) -> bool {
        agb::println!(
            "[probe] L{}: alloc::alloc({}) — allocator call only, address prints before any write",
            size,
            size
        );
        let layout = match Layout::from_size_align(size, 8) {
            Ok(l) => l,
            Err(e) => {
                agb::println!("[probe] L{}: layout rejected: {:?}", size, e);
                return false;
            }
        };
        let ptr = unsafe { alloc(layout) };
        if ptr.is_null() {
            // Honest budget answer (rev3's rule): the heap cannot serve this
            // size at this point. Name it and keep the image alive.
            agb::println!(
                "[probe] L{}: ALLOC RETURNED NULL — heap cannot serve this size here",
                size
            );
            return false;
        }
        let addr = ptr as usize;
        let ewram_ok = EWRAM.contains(&addr);
        agb::println!(
            "[probe] L{}: alloc ok addr=0x{:08X} within EWRAM [{}] — allocator call survived, writes next",
            size,
            addr,
            if ewram_ok { "OK" } else { "NOT IN EWRAM" }
        );

        agb::println!("[probe] L{}: w0 write byte at offset 0", size);
        unsafe { write_volatile(ptr, 0xA5) };
        agb::println!("[probe] L{}: wmid write byte at offset {}", size, size / 2);
        unsafe { write_volatile(ptr.add(size / 2), 0x5A) };
        agb::println!("[probe] L{}: wlast write byte at offset {}", size, size - 1);
        unsafe { write_volatile(ptr.add(size - 1), 0xC3) };

        let rb0 = unsafe { core::ptr::read_volatile(ptr) };
        let rbm = unsafe { core::ptr::read_volatile(ptr.add(size / 2)) };
        let rbl = unsafe { core::ptr::read_volatile(ptr.add(size - 1)) };
        let rb_ok = rb0 == 0xA5 && rbm == 0x5A && rbl == 0xC3;
        agb::println!(
            "[probe] L{}: readback 0x{:02X}/0x{:02X}/0x{:02X} [{}]",
            size,
            rb0,
            rbm,
            rbl,
            if rb_ok { "OK" } else { "CORRUPT" }
        );

        unsafe { dealloc(ptr, layout) };
        agb::println!(
            "[probe] L{}: dealloc ok — full alloc/write/readback/free round-trip clean",
            size
        );
        ewram_ok && rb_ok
    }

    /// P2: the production construction shape (`walk_region`'s
    /// `Box::new(new_decoder(..)?)`) driven over the exact embedded arm:
    /// region FNV, full walk folded through the shared grid, progress lines
    /// (a stuck counter names the packet; silence after the banner names the
    /// pre-decode span), walk totals, EWRAM placement, `fnv_walk_fold` golden.
    #[inline(never)] // rev9 frame isolation — see the IWRAM const note
    fn walk_probe(clip: &OpusClip) -> bool {
        let expected_region = clip.fnv_packets;
        let actual_region = fnv1a64(clip.region);
        let region_ok = actual_region == expected_region;
        agb::println!(
            "[probe] P2 {} region fnv: expected 0x{:016X} actual 0x{:016X} [{}]",
            clip.name,
            expected_region,
            actual_region,
            if region_ok { "MATCH" } else { "MISMATCH" }
        );
        agb::println!(
            "[probe] P2 {} walking {} packets via the production seam",
            clip.name,
            clip.packets
        );

        let mut hash = Fnv1a64::new();
        let mut walked = 0usize;
        let walk = probe::decode_clip(clip, |window| {
            fold_f32_window(&mut hash, window);
            walked += 1;
            if walked == 1 || walked % 100 == 0 || walked == clip.packets {
                agb::println!(
                    "[probe] P2 {} packet {:>3}/{} running=0x{:016X}",
                    clip.name,
                    walked,
                    clip.packets,
                    hash.finish()
                );
            }
        });
        let stats = match walk {
            Ok(stats) => stats,
            Err((msg, packet)) => {
                agb::println!(
                    "[probe] P2 {} walk FAILED at packet {}: {}",
                    clip.name,
                    packet,
                    msg
                );
                return false;
            }
        };

        let ewram_ok = EWRAM.contains(&stats.decoder_addr) && EWRAM.contains(&stats.window_addr);
        let totals_ok = stats.packets == clip.packets
            && usize::try_from(clip.walk_samples).is_ok_and(|want| want == stats.samples);
        let actual_fold = hash.finish();
        let golden_ok = actual_fold == clip.fnv_walk_fold;
        agb::println!(
            "[probe] P2 {} walk ok: packets={} (want {}) samples={} (want {}) decoder=0x{:08X} window=0x{:08X} ewram={} totals={}",
            clip.name,
            stats.packets,
            clip.packets,
            stats.samples,
            clip.walk_samples,
            stats.decoder_addr,
            stats.window_addr,
            if ewram_ok { "OK" } else { "OUT" },
            if totals_ok { "OK" } else { "MISMATCH" }
        );
        agb::println!(
            "[probe] P2 {} walk-fold fnv: expected 0x{:016X} actual 0x{:016X} [{}]",
            clip.name,
            clip.fnv_walk_fold,
            actual_fold,
            if golden_ok { "MATCH" } else { "MISMATCH" }
        );
        region_ok && ewram_ok && totals_ok && golden_ok
    }

    /// P3: rev1's killer as its own stage — the `vec![0u8; N]` macro (alloc
    /// + fill fused). Green ladder + death here convicts the macro's path
    /// (capacity layout, fill, or their interaction), not the raw allocator.
    #[inline(never)] // rev9 frame isolation — see the IWRAM const note
    fn fused_vec_probe() -> bool {
        agb::println!(
            "[probe] P3 fused: vec![0u8; {}] — alloc+fill macro path (rev1 died inside this expression)",
            FUSE_BYTES
        );
        let v: Vec<u8> = alloc::vec![0u8; FUSE_BYTES];
        let addr = v.as_ptr() as usize;
        let ewram_ok = EWRAM.contains(&addr);
        agb::println!(
            "[probe] P3 fused ok: addr=0x{:08X} len={} within EWRAM [{}]",
            addr,
            v.len(),
            if ewram_ok { "OK" } else { "NOT IN EWRAM" }
        );
        drop(v);
        agb::println!("[probe] P3 fused: dropped ok");
        ewram_ok
    }

    /// Read SP — rev6's decisive witness. With `-Cforce-frame-pointers`, a
    /// function holding a large sret temporary has SP decremented in the
    /// prologue: SP read at entry equals (stack top ≈ 0x03007F00) minus the
    /// frame size. A ~178 KB temporary therefore reads SP BELOW 0x03000000 —
    /// the frame wraps off IWRAM into EWRAM (mapped RAM: the smash writes
    /// silently, destroying .bss/heap/allocator state — which is what turns
    /// the corrupted return into the observed wild jumps). No temporary:
    /// SP stays in the normal few-hundred-byte frame. One asm line, prints
    /// BEFORE the call, survives every death mode below it.
    #[inline(never)]
    fn read_sp() -> usize {
        let sp: usize;
        unsafe {
            core::arch::asm!("mov {}, sp", out(reg) sp, options(nomem, preserves_flags, nostack))
        };
        sp
    }

    /// IWRAM range — a frame-pointer witness reading outside this range at
    /// function entry names a stack temporary that wrapped off IWRAM.
    /// Rev9: meaningful only because the stage functions are
    /// `#[inline(never)]` — with them inlined into main, the entry frame is
    /// the union of every stage's temporaries (TAIL's 178 KB repro included)
    /// and every stage's SP reading is convicted by stages that have not
    /// run yet (rev8's measured confound).
    const IWRAM: core::ops::Range<usize> = 0x0300_0000..0x0300_8000;

    /// P4 — the landed fix shape (rev8): `probe::new_decoder_in_place`
    /// delegates to vendor patch 2's `OpusDecoder::new_in_place`, which
    /// writes the state through a destination pointer — no by-value `Self`
    /// return at any level (rev5 measured that caller-side destination
    /// propagation via `new_decoder_boxed` DIES like TAIL: the by-value hops
    /// are INSIDE the vendored ctor, so the fix had to move into the
    /// construction path itself). The SP witness stays: `sp@P4` at entry —
    /// inside IWRAM = no big temporary (the fix's on-target claim);
    /// below 0x0300_0000 = the frame still wraps off IWRAM (patch failed
    /// on target despite host witnesses). Survival here is the designated
    /// before/after witness (OPUS.md 2026-10-05).
    #[inline(never)] // rev9 frame isolation — see the IWRAM const note
    fn candidate_shape_probe() -> bool {
        let sp = read_sp();
        agb::println!(
            "[probe] P4 sp@entry=0x{:08X} within IWRAM [{}] — {}",
            sp,
            if IWRAM.contains(&sp) { "OK" } else { "OUT" },
            if IWRAM.contains(&sp) {
                "no stack temporary in this frame"
            } else {
                "STACK TEMPORARY CONFIRMED: frame wrapped off IWRAM"
            }
        );
        agb::println!(
            "[probe] P4 candidate: new_decoder_in_place — vendor patch 2 in-place ctor (no by-value hop anywhere)"
        );
        let result = probe::new_decoder_in_place(48_000, 1);
        match result {
            Ok(b) => {
                let addr: *const _ = &*b;
                let in_ewram = EWRAM.contains(&(addr as usize));
                agb::println!(
                    "[probe] P4 candidate ok: decoder box @ 0x{:08X} within EWRAM [{}]",
                    addr as usize,
                    if in_ewram { "OK" } else { "NOT IN EWRAM" }
                );
                drop(b);
                agb::println!("[probe] P4 candidate: dropped ok");
                in_ewram
            }
            Err(e) => {
                agb::println!("[probe] P4 candidate: ctor rejected: {}", e);
                false
            }
        }
    }

    /// The 2026-10-03/04 repro shape verbatim: the ctor result visibly
    /// escaping through a closure — the documented worst case for
    /// destination propagation, where a 177,864 B sret temporary lands in
    /// the caller's ~31 KB stack frame. Expected: banner prints, next line
    /// never does (or the wild PC traps). If `box ok` prints, elision
    /// changed between builds — a finding either way; neither changes the
    /// verdict above.
    #[inline(never)] // rev9: the repro's own frame wraps — that IS the finding; isolation keeps it from poisoning the stages before it
    fn bisect_tail() {
        let sp = read_sp();
        agb::println!(
            "[probe] TAIL sp@entry=0x{:08X} within IWRAM [{}]",
            sp,
            if IWRAM.contains(&sp) { "OK" } else { "OUT" }
        );
        agb::println!(
            "[probe] TAIL entering .map(|d| Box::new(d)) shape — silence or jump-trap after this line reproduces the 2026-10-03 constructor hang"
        );
        let probe_box = probe::new_decoder(48_000, 1).map(|d| Box::new(d));
        match &probe_box {
            Ok(b) => {
                let addr: *const _ = &**b;
                agb::println!(
                    "[probe] TAIL box ok @ 0x{:08X} — shape did NOT reproduce this build (elision changed)",
                    addr as usize
                );
            }
            Err(e) => agb::println!("[probe] TAIL ctor rejected: {}", e),
        }
        drop(probe_box);
        agb::println!("[probe] TAIL dropped ok");
    }
}

/// Host-build stub: the probe ROM exists only for `thumbv4t-none-eabi` (see
/// the module docs and the `[[bin]]` gate in Cargo.toml). Mirrors the proof
/// ROM's stub so host tooling can parse the crate without `agb`.
#[cfg(not(target_arch = "arm"))]
fn main() {}
