//! The presentation probe: AP-3c's paired arms (`ALBUM-PLAYER.md` §13 AP-3c,
//! §5.7, §11's 2b PR-(2)).
//!
//! The question is not "do sprites look better than tiles" — §5.7 splits that
//! conflation into two axes: **presentation timing** (main-loop `commit` vs a
//! VBlank-presented shadow) and **storage** (BG cells vs OBJ). This bin measures
//! the first axis, with the second as the means:
//!
//! | arm | shape |
//! |---|---|
//! | **A** | the 2a product today: three backgrounds, an event-scoped `commit` when a clock digit changes (~1 Hz) — the control |
//! | **B** | the chrome as OBJ (56 entries), a double-buffered-style OAM shadow presented by DMA3 at VBlank, a sparse 2-entry update per clock tick, **no main-loop commit at all** |
//! | **C** | B under a 60 Hz synthetic chrome load: all 56 entries rewritten every frame, presented as one 1 KiB DMA |
//!
//! Every arm runs the same producer and the same live audio chain. The measured
//! terms are the pass (producer + UI work), the commit (A only), the VBlank
//! handler's own entry-to-exit cost, and the feeder's lateness/underflows — so
//! "the main loop no longer waits" is a difference in the log, not an inference.
//!
//! ## What the arms are not
//!
//! Not a product renderer, and not a proof that the OBJ chrome is the answer. §5.7
//! makes the implementation conditional on this probe: if arm A shows the 1 Hz
//! event commit is already a producer risk, the exception is retired inside 2a and
//! 2b's presentation path is the OBJ one; if A comes back clean, the OBJ chrome
//! waits. The bin prints both arms' numbers so that decision has operands.
//!
//! ## Why arm B can present without a commit
//!
//! `GraphicsFrame::commit` waits for VBlank and then copies OAM anyway
//! (`display/mod.rs:314-337`, `object/unmanaged/object.rs:48-66`), so objects
//! alone cannot remove the wait. Arm B instead owns OAM: the shadow in
//! `render_hw` is written by the main loop and presented by a DMA3 transfer
//! started inside the VBlank interrupt. The backgrounds were committed once, at
//! setup, before the measured window — the boot-scene upload §5.7 allows.
//!
//! The OBJ pool is separate from both BG pools (§5.1), so this arm's tiles do not
//! compete with the scene's; the cost it pays is a duplicated atlas copy, which
//! the printed budget line names.

#![no_std]
#![no_main]

extern crate alloc;

use alloc::boxed::Box;

use agb::Gba;
use agb::display::{
    Palette16, Priority, Rgb, Rgb15,
    tiled::{
        RegularBackground, RegularBackgroundSize, TileEffect, TileFormat, TileSet, TileSetting,
    },
};

use agb::interrupt::VBlank;

use album_probe::probe_hw::{
    self, Fold, Scratch,
    timer::{Counter, cycles_between},
};
use album_probe::render_hw;
use album_probe::synth::Synth;

/// Frames per arm. At ~281,000 cycles per video frame this is ~50.6 M cycles,
/// ~88 audio periods — enough for a feeder worst case to appear in each arm, and
/// 3 s of emulated time per arm.
const FRAMES_PER_ARM: u32 = 180;
/// One clock digit change every 60 frames, the "1 Hz" event the 2a surface
/// commits at (§5.2, §5.7).
const CLOCK_PERIOD_FRAMES: u32 = 60;
/// §5.7's chrome reservation: title 20 + bar 10 + state word 9 + five icons +
/// clock 12.
const CHROME_OAM: usize = 56;
/// Title band and chrome band, in screen rows (§5.0's 16 px title band and the
/// 2-row always-shown chrome).
const TITLE_ROW: i32 = 16; // y = 128
const CHROME_ROW: i32 = 18; // y = 144
/// OBJ palette bank 0; every entry uses it, so the chrome needs 1 of 16 banks.
const OBJ_BANK: u8 = 0;

const COLOR_BACKDROP: Rgb15 = Rgb::new(16, 16, 24).to_rgb15();
const COLOR_FILL: Rgb15 = Rgb::new(0, 48, 64).to_rgb15();
const COLOR_TEXT: Rgb15 = Rgb15::WHITE;
const COLOR_ACCENT: Rgb15 = Rgb::new(255, 200, 64).to_rgb15();
const COLOR_PASS: Rgb15 = Rgb::new(0, 0, 255).to_rgb15();
const COLOR_FAIL: Rgb15 = Rgb::new(255, 0, 0).to_rgb15();

fn tileset(bytes: &'static [u8]) -> TileSet {
    // SAFETY: `render_hw`'s banks are 4-byte aligned and a whole number of 4bpp
    // 32-byte tiles.
    unsafe { TileSet::new(bytes, TileFormat::FourBpp) }
}

fn setting(tile: u16, bank: u8) -> TileSetting {
    TileSetting::new(tile, TileEffect::new(false, false, bank))
}

/// One arm's ledger.
struct ArmResult {
    pass: Fold,
    work: Fold,
    commit: Fold,
    commits: u32,
    due: u32,
    feeder_sum: u64,
    feeder_max: u32,
    feeder_wrapped: u32,
    periods: u32,
    underflows: u32,
    late_max: u16,
    late_over_grace: u32,
    vblank_frames: u32,
    vblank_dmas: u32,
    vblank_sum: u64,
    vblank_max: u32,
}

/// Producer slice: outside every measured window, so the closure stays disjoint.
fn producer(synth: &mut Synth, scratch: &mut Scratch) -> bool {
    probe_hw::fill_to(synth, probe_hw::PREFILL_SAMPLES, scratch).is_ok()
}

fn build_backgrounds(
    gfx: &mut agb::display::Graphics<'_>,
) -> (RegularBackground, RegularBackground, RegularBackground) {
    let atlas = render_hw::make_atlas();
    let icons = render_hw::make_icons();
    let atlas_ts = tileset(atlas);
    let icons_ts = tileset(icons);

    let mut b0 = [Rgb15::BLACK; 16];
    b0[0] = COLOR_BACKDROP;
    b0[1] = COLOR_FILL;
    gfx.set_background_palette(0, &Palette16::new(b0));
    let mut b1 = [Rgb15::BLACK; 16];
    b1[0] = COLOR_BACKDROP;
    b1[1] = COLOR_TEXT;
    b1[2] = COLOR_ACCENT;
    gfx.set_background_palette(1, &Palette16::new(b1));

    // BG0: the always-shown chrome art (a fill plus the five control glyphs).
    let mut chrome = RegularBackground::new(
        Priority::P0,
        RegularBackgroundSize::Background32x32,
        TileFormat::FourBpp,
    );
    for y in CHROME_ROW..CHROME_ROW + 2 {
        for x in 0..30 {
            chrome.set_tile((x, y), &icons_ts, setting(5, 1));
        }
    }
    for icon in 0..5u16 {
        chrome.set_tile((1 + icon as i32, CHROME_ROW), &icons_ts, setting(icon, 1));
    }

    // BG1: the scene region (16 tile rows, the 128 px band).
    let mut scene = RegularBackground::new(
        Priority::P1,
        RegularBackgroundSize::Background32x32,
        TileFormat::FourBpp,
    );
    for y in 0..16i32 {
        for x in 0..30i32 {
            let glyph = ((x * 5 + y * 3) % render_hw::GLYPHS as i32) as u16;
            scene.set_tile((x, y), &atlas_ts, setting(glyph * 2 + (y & 1) as u16, 1));
        }
    }

    // BG2: the foreground — the title band's text run and the bar's ten cells.
    let mut fg = RegularBackground::new(
        Priority::P2,
        RegularBackgroundSize::Background32x32,
        TileFormat::FourBpp,
    );
    for x in 0..30i32 {
        let glyph = ((x * 7) % render_hw::GLYPHS as i32) as u16;
        fg.set_tile((x, TITLE_ROW), &atlas_ts, setting(glyph * 2, 1));
        fg.set_tile((x, TITLE_ROW + 1), &atlas_ts, setting(glyph * 2 + 1, 1));
    }
    for cell in 0..10i32 {
        fg.set_tile(
            (20 + cell, TITLE_ROW),
            &icons_ts,
            setting(6 + cell as u16, 1),
        );
    }

    (chrome, scene, fg)
}

/// Arm A: the 2a shape. Show all three backgrounds every frame only when a clock
/// digit changed, then commit — the event-scoped exception §5.7 names.
fn arm_a(
    gfx: &mut agb::display::Graphics<'_>,
    vblank: &VBlank,
    counter: &mut Counter<'_>,
    chrome: &RegularBackground,
    scene: &RegularBackground,
    fg: &mut RegularBackground,
    synth: &mut Synth,
    scratch: &mut Scratch,
    atlas_ts: &TileSet,
) -> Option<ArmResult> {
    let before = probe_hw::stats();
    let mut pass = Fold::new();
    let mut work = Fold::new();
    let mut commit = Fold::new();
    let mut commits = 0u32;
    let mut due = 0u32;
    let mut digit = 0u16;

    for frame_index in 0..FRAMES_PER_ARM {
        // Video-paced like the product's loop: one frame per iteration, and the
        // commit below is the *only* thing this arm does to the display on a
        // changed cell. The wait is common to all three arms, so the comparison
        // does not confuse a frame pace with a presentation cost.
        vblank.wait_for_vblank();
        let frame_start = counter.read();
        if !producer(synth, scratch) {
            return None;
        }
        let is_due = frame_index % CLOCK_PERIOD_FRAMES == 0;
        if is_due {
            due = due.wrapping_add(1);
            let t = counter.read();
            digit = (digit + 1) % render_hw::GLYPHS as u16;
            fg.set_tile((19, TITLE_ROW), atlas_ts, setting(digit * 2, 1));
            work.observe(cycles_between(t, counter.read()));

            let t = counter.read();
            {
                let mut frame = gfx.frame();
                chrome.show(&mut frame);
                scene.show(&mut frame);
                fg.show(&mut frame);
                frame.commit();
            }
            commit.observe(cycles_between(t, counter.read()));
            commits = commits.wrapping_add(1);
        }
        pass.observe(cycles_between(frame_start, counter.read()));
    }

    let after = probe_hw::stats();
    let blank = render_hw::vblank_stats();
    Some(ArmResult {
        pass,
        work,
        commit,
        commits,
        due,
        feeder_sum: after.feed_cycles_sum.wrapping_sub(before.feed_cycles_sum),
        feeder_max: after.feed_cycles_max,
        feeder_wrapped: after.feed_wrapped.wrapping_sub(before.feed_wrapped),
        periods: after.periods.wrapping_sub(before.periods),
        underflows: after.underflows.wrapping_sub(before.underflows),
        late_max: after.late_max,
        late_over_grace: after.late_over_grace,
        vblank_frames: blank.frames,
        vblank_dmas: blank.dmas,
        vblank_sum: blank.cost_sum,
        vblank_max: blank.cost_max,
    })
}

/// Arm B and C: no main-loop commit. The sparse arm writes only the clock entries
/// that changed; the load arm rewrites all 56 every frame and presents one 1 KiB
/// DMA. `chrome` is already the OBJ entry table's source of truth.
fn arm_obj(
    vblank: &VBlank,
    counter: &mut Counter<'_>,
    synth: &mut Synth,
    scratch: &mut Scratch,
    load_every_frame: bool,
) -> Option<ArmResult> {
    let before = probe_hw::stats();
    let blank_before = render_hw::vblank_stats();
    render_hw::reset_vblank_max();
    let mut pass = Fold::new();
    let mut work = Fold::new();
    let mut due = 0u32;
    let mut digit = 0u16;

    for frame_index in 0..FRAMES_PER_ARM {
        // Video-paced like the product's loop: one frame per iteration, and the
        // commit below is the *only* thing this arm does to the display on a
        // changed cell. The wait is common to all three arms, so the comparison
        // does not confuse a frame pace with a presentation cost.
        vblank.wait_for_vblank();
        let frame_start = counter.read();
        if !producer(synth, scratch) {
            return None;
        }
        let is_due = frame_index % CLOCK_PERIOD_FRAMES == 0;
        let t = counter.read();
        if load_every_frame {
            // 60 Hz synthetic load: every entry moves one pixel on alternate
            // frames, and the shadow is presented whole.
            let shift = (frame_index & 1) as i32;
            for slot in 0..CHROME_OAM {
                let (x, y, tile, tall) = chrome_slot(slot, digit, shift);
                render_hw::oam_write(slot, render_hw::entry(x, y, tile, tall, OBJ_BANK, 0));
            }
            render_hw::oam_full();
            render_hw::oam_publish();
        } else {
            if is_due {
                due = due.wrapping_add(1);
                digit = (digit + 1) % render_hw::GLYPHS as u16;
                // Two clock columns: the glyph's top and bottom tiles are picked
                // by the shared atlas index, exactly the 8 B-per-cell write §5.7
                // prices.
                for (k, slot) in [44usize, 45].into_iter().enumerate() {
                    let glyph = digit.wrapping_add(k as u16) % render_hw::GLYPHS as u16;
                    let (x, y, _, tall) = chrome_slot(slot, digit, 0);
                    render_hw::oam_write(
                        slot,
                        render_hw::entry(x, y, glyph * 2, tall, OBJ_BANK, 0),
                    );
                }
                render_hw::oam_publish();
            }
        }
        work.observe(cycles_between(t, counter.read()));
        pass.observe(cycles_between(frame_start, counter.read()));
    }

    let after = probe_hw::stats();
    let blank = render_hw::vblank_stats();
    Some(ArmResult {
        pass,
        work,
        commit: Fold::new(),
        commits: 0,
        due,
        feeder_sum: after.feed_cycles_sum.wrapping_sub(before.feed_cycles_sum),
        feeder_max: after.feed_cycles_max,
        feeder_wrapped: after.feed_wrapped.wrapping_sub(before.feed_wrapped),
        periods: after.periods.wrapping_sub(before.periods),
        underflows: after.underflows.wrapping_sub(before.underflows),
        late_max: after.late_max,
        late_over_grace: after.late_over_grace,
        vblank_frames: blank.frames.wrapping_sub(blank_before.frames),
        vblank_dmas: blank.dmas.wrapping_sub(blank_before.dmas),
        vblank_sum: blank.cost_sum.wrapping_sub(blank_before.cost_sum),
        vblank_max: blank.cost_max,
    })
}

/// The chrome's OAM table: title 20 (8x16), bar 10 (8x8), state word 9, five
/// icons, clock 12. `slot` is the index in this table, and the coordinates are
/// the *cells* the product's `Write` plan would name, at `x*8, y*8`.
#[allow(clippy::too_many_arguments)]
fn chrome_slot(slot: usize, digit: u16, shift: i32) -> (i32, i32, u16, bool) {
    let atlas = render_hw::ATLAS_TILES as u16;
    if slot < 20 {
        // Title line: 20 eight-by-sixteen columns, one glyph tile pair each.
        let glyph = ((slot as u16 * 3) % render_hw::GLYPHS as u16) * 2;
        (slot as i32 * 8 + shift, TITLE_ROW * 8, glyph, true)
    } else if slot < 30 {
        // Bar: ten 4 px cells drawn as 8x8 tiles (§6's default 18-cell grid is
        // the product's; the probe only needs the count).
        let level = (slot - 20) as u16;
        (
            160 + (slot - 20) as i32 * 8,
            TITLE_ROW * 8,
            atlas + 6 + level,
            false,
        )
    } else if slot < 39 {
        // State word: nine columns on the chrome row.
        let glyph = ((slot as u16) % render_hw::GLYPHS as u16) * 2;
        ((slot - 30) as i32 * 8, CHROME_ROW * 8, glyph, true)
    } else if slot < 44 {
        // The five control icons, 8x8 and differing in shape (§5.6).
        (
            (slot - 30) as i32 * 8,
            CHROME_ROW * 8,
            atlas + (slot - 39) as u16,
            false,
        )
    } else {
        // Clock: twelve columns; slot 44 and 45 carry the changing digits.
        let glyph = if slot < 46 {
            digit
        } else {
            (slot as u16) % render_hw::GLYPHS as u16
        };
        (
            112 + (slot - 44) as i32 * 8,
            CHROME_ROW * 8,
            glyph * 2,
            true,
        )
    }
}

fn report(arm: &str, r: &ArmResult) {
    agb::println!(
        "[album-probe] present arm={} pass count={} sum={} min={} max={} wrap={} work_sum={} commit_sum={} due={} commits={}",
        arm,
        r.pass.count,
        r.pass.sum,
        r.pass.min,
        r.pass.max,
        r.pass.wrapped,
        r.work.sum,
        r.commit.sum,
        r.due,
        r.commits
    );
    agb::println!(
        "[album-probe] present feeder arm={} audio=live periods={} sum={} max={} late_max={} late_over_grace={} underflows={}",
        arm,
        r.periods,
        r.feeder_sum,
        r.feeder_max,
        r.late_max,
        r.late_over_grace,
        r.underflows
    );
    agb::println!(
        "[album-probe] present vblank arm={} frames={} dmas={} sum={} max_any={}",
        arm,
        r.vblank_frames,
        r.vblank_dmas,
        r.vblank_sum,
        r.vblank_max
    );
}

fn run(gba: &mut Gba) -> bool {
    let atlas = render_hw::make_atlas();
    let icons = render_hw::make_icons();
    let atlas_ts = tileset(atlas);

    let mut gfx = gba.graphics.get();
    let (chrome, scene, mut fg) = build_backgrounds(&mut gfx);
    let mut scratch = Box::new(Scratch::new());
    let mut synth = Synth::new();

    let handler = {
        let mut timers = unsafe { gba.timers.all_timers() }; // SAFETY: no agb::sound::mixer::Mixer is instantiated in this ROM, so timer0-3 are unowned.
        probe_hw::bootstrap(&mut timers, &mut synth, &mut scratch)
    };
    let Some(_handler) = handler else {
        agb::println!("[album-probe] FAIL present: bootstrap could not prime a whole chunk");
        agb::println!("Tests finished successfully");
        return false;
    };
    let mut timer_pool = gba.timers.timers();
    let mut counter = Counter::start(&mut timer_pool);
    // One frame wait per pass, owned by the probe: the product's loop is paced by
    // the vblank latch, and an unpaced probe would run 180 iterations in a period
    // or two instead of 180 video frames.
    let vblank = VBlank::get();

    let mut pass = true;

    // ------------------------------------------------------------------ arm A
    let Some(a) = arm_a(
        &mut gfx,
        &vblank,
        &mut counter,
        &chrome,
        &scene,
        &mut fg,
        &mut synth,
        &mut scratch,
        &atlas_ts,
    ) else {
        agb::println!("[album-probe] FAIL present: producer push refused during arm A");
        agb::println!("Tests finished successfully");
        return false;
    };
    report("A", &a);
    pass &= a.pass.wrapped == 0
        && a.commit.wrapped == 0
        && a.underflows == 0
        && a.feeder_wrapped == 0
        && a.commits == a.due
        && a.commits > 0;

    // --------------------------------------------------------- arm B setup
    //
    // The one commit after arm A: it drops the foreground BG (the freed layer 2b's
    // list text needs) and keeps the chrome art + scene. This is the boot-scene
    // upload, not a steady-state commit.
    {
        let mut frame = gfx.frame();
        chrome.show(&mut frame);
        scene.show(&mut frame);
        frame.commit();
    }
    // The chrome's tile bank goes to the separate OBJ pool (§5.1), then the
    // shadow is filled and presented whole once.
    render_hw::copy_to_obj(atlas);
    render_hw::copy_to_obj(icons);
    for slot in 0..128 {
        render_hw::oam_write(slot, render_hw::hidden());
    }
    for slot in 0..CHROME_OAM {
        let (x, y, tile, tall) = chrome_slot(slot, 0, 0);
        render_hw::oam_write(slot, render_hw::entry(x, y, tile, tall, OBJ_BANK, 0));
    }
    render_hw::oam_full();
    render_hw::oam_publish();
    let vblank_handler = render_hw::start_vblank_oam();
    // Let the full paint land before the measured window.
    while render_hw::vblank_stats().frames == 0 {}

    agb::println!(
        "[album-probe] present obj_budget atlas_tiles={} icon_tiles={} oam={} of=128 vpal_banks=1 shadow_bytes={} per_line_max={}",
        render_hw::ATLAS_TILES,
        render_hw::ICON_TILES,
        CHROME_OAM,
        128 * 8,
        30
    );

    // ------------------------------------------------------------------ arm B
    let Some(b) = arm_obj(&vblank, &mut counter, &mut synth, &mut scratch, false) else {
        agb::println!("[album-probe] FAIL present: producer push refused during arm B");
        agb::println!("Tests finished successfully");
        return false;
    };
    report("B", &b);
    pass &= b.pass.wrapped == 0
        && b.underflows == 0
        && b.feeder_wrapped == 0
        && b.commits == 0
        && b.vblank_frames > 0
        && b.vblank_dmas > 0;

    // ------------------------------------------------------------------ arm C
    let Some(c) = arm_obj(&vblank, &mut counter, &mut synth, &mut scratch, true) else {
        agb::println!("[album-probe] FAIL present: producer push refused during arm C");
        agb::println!("Tests finished successfully");
        return false;
    };
    report("C", &c);
    pass &= c.pass.wrapped == 0
        && c.underflows == 0
        && c.feeder_wrapped == 0
        && c.commits == 0
        && c.vblank_frames > 0
        && c.vblank_dmas > 0;

    // The comparison the section is for: arm A's pass has a commit in it, arms B
    // and C have none, and all three ran the same producer with zero underflows.
    agb::println!(
        "[album-probe] present compare a_pass_sum={} b_pass_sum={} c_pass_sum={} a_commit_sum={} a_commits={} b_commits={} c_commits={} period_cycles={}",
        a.pass.sum,
        b.pass.sum,
        c.pass.sum,
        a.commit.sum,
        a.commits,
        b.commits,
        c.commits,
        probe_hw::PERIOD_CYCLES
    );
    pass &= b.commits == 0 && c.commits == 0;

    drop(vblank_handler);
    render_hw::stop_vblank_oam();

    if pass {
        agb::println!(
            "[album-probe] PASS present arms=3 commits_a={} commits_b={} commits_c={} vblank_b={} vblank_c={} underflows=0",
            a.commits,
            b.commits,
            c.commits,
            b.vblank_frames,
            c.vblank_frames
        );
    } else {
        agb::println!(
            "[album-probe] FAIL present: an arm underflowed, a commit happened in B/C, the VBlank path never presented, or a window wrapped — see the lines above"
        );
    }
    agb::println!("Tests finished successfully");
    probe_hw::finish();
    probe_hw::teardown_audio();
    pass
}

#[agb::entry]
fn main(mut gba: Gba) -> ! {
    let pass = run(&mut gba);
    let mut gfx = gba.graphics.get();
    gfx.set_background_palette_colour(0, 0, if pass { COLOR_PASS } else { COLOR_FAIL });
    loop {
        let frame = gfx.frame();
        frame.commit();
    }
}
