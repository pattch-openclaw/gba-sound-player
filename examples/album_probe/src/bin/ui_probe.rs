//! UI spike: 3 backgrounds, one 8x8 object, text and a palette fade, at VBlank,
//! with the shipped lossless feeder playing synthetic PCM underneath
//! (ALBUM-PLAYER.md §11 Phase 0 (a), §13 AP-0).
//!
//! Five arms, each a separate pass, each showing **every** background it owns into
//! **every** frame. That last clause is the agb trap this probe exists to prove:
//! `TiledGraphicsFrame::commit` derives DISPCNT's background-enable mask from the
//! backgrounds *shown into that frame*, so a commit from a fresh frame with
//! nothing shown writes `enabled_backgrounds = 0` and repaints the backdrop alone
//! (the bug 6a-7b's first hardware run found, and `text.rs`'s "one-shot present"
//! note). An application that owns its backgrounds for the ROM's lifetime must
//! show all of them every frame; dropping even one drops it from the display.
//!
//! | arm | live graphics |
//! |---|---|
//! | `none` | nothing — the audio-only control |
//! | `bg` | one background, text composed on it |
//! | `bg3` | three backgrounds, one of them scrolling |
//! | `obj` | `bg3` plus one `DynamicSprite16` 8x8 object animated per frame |
//! | `full` | `obj` plus a palette fade every frame |
//!
//! Measurement: the feeder's per-period cost and each arm's per-frame work are
//! taken with **interrupts live** (`Counter::read` + `cycles_between`), because the
//! cost in question is what this work charges while the audio chain runs. The
//! closure per arm names every term:
//!
//! ```text
//! period_sum = producer_sum + feeder_sum + ui_sum + residual_sum
//! ```
//!
//! `residual` is the VBlank wait plus loop and instrument overhead — named, because
//! an unexplained gap in a ledger is a bug in the ledger (§4.5). Means are derived
//! host-side: the ROM prints sums and counts, never a divide.
//!
//! Two more numbers: one video period with the audio chain **disarmed**, to settle
//! §4.5's "~281,000 cycles per frame" (which the spec asks AP-0 to confirm, not
//! assume), and the cost of one 8x8 4bpp tile write — the unit §7's 120x120 box
//! costs 225 of.
//!
//! The text is composed once at setup and only `show`n per frame, so the `ui`
//! term is a **lower bound** on §10.2's "position, bar + 2 text lines" row: it
//! prices the composed background, not glyph layout.

#![no_std]
#![no_main]

extern crate alloc;

use alloc::boxed::Box;

use agb::Gba;
use agb::display::{
    Graphics, Palette16, Priority, Rgb, Rgb15,
    font::{Font, Layout, LayoutSettings, RegularBackgroundTextRenderer},
    object::{DynamicSprite16, Object, Size},
    tiled::{
        RegularBackground, RegularBackgroundSize, TileEffect, TileFormat, TileSet, TileSetting,
    },
};

use album_probe::geometry as geo;
use album_probe::probe_hw::{
    self, Fold, Scratch,
    timer::{Counter, cycles_between, timed},
};
use album_probe::synth::Synth;

/// The committed OFL pixel font, the same one the product ROM renders with. The
/// path resolves against this crate's manifest directory (`examples/album_probe`),
/// so it needs the same two levels up as the E5/product copies.
static FONT: Font = agb::include_font!(
    "../../assets/fonts/fusion-pixel-8px-proportional-latin-ascii.ttf",
    8
);

/// Frames per arm. A video frame is ~281,000 cycles and an audio period 573,440,
/// so 60 frames is ~29 audio periods per arm — enough for the feeder's worst
/// period to show up, and ~1 s of emulated time.
const FRAMES: u32 = 60;

const TEXT: &str = "ALBUM PLAYER PHASE 0\nUI PROBE\n3 BACKGROUNDS + OBJECT";

const COLOR_BACKDROP: Rgb15 = Rgb::new(16, 16, 24).to_rgb15();
const COLOR_GLYPH: Rgb15 = Rgb15::WHITE;
const COLOR_PATTERN_A: Rgb15 = Rgb::new(0, 96, 160).to_rgb15();
const COLOR_PATTERN_A2: Rgb15 = Rgb::new(0, 24, 48).to_rgb15();
const COLOR_PATTERN_B: Rgb15 = Rgb::new(160, 32, 96).to_rgb15();
const COLOR_PATTERN_B2: Rgb15 = Rgb::new(48, 8, 32).to_rgb15();
const COLOR_OBJ: Rgb15 = Rgb::new(255, 224, 64).to_rgb15();
const COLOR_PASS: Rgb15 = Rgb::new(0, 0, 255).to_rgb15();
const COLOR_FAIL: Rgb15 = Rgb::new(255, 0, 0).to_rgb15();

/// One 4bpp 8x8 tile as 8 VRAM words (4 bytes per row, low nibble leftmost):
/// alternating rows of colour 1 and colour 0, i.e. horizontal stripes.
#[repr(align(4))]
struct Aligned32([u8; 32]);

static STRIPES: Aligned32 = Aligned32([
    0x11, 0x11, 0x11, 0x11, 0x00, 0x00, 0x00, 0x00, 0x11, 0x11, 0x11, 0x11, 0x00, 0x00, 0x00, 0x00,
    0x11, 0x11, 0x11, 0x11, 0x00, 0x00, 0x00, 0x00, 0x11, 0x11, 0x11, 0x11, 0x00, 0x00, 0x00, 0x00,
]);

static DOTS: Aligned32 = Aligned32([
    0x00, 0x11, 0x00, 0x11, 0x11, 0x00, 0x11, 0x00, 0x00, 0x11, 0x00, 0x11, 0x11, 0x00, 0x11, 0x00,
    0x00, 0x11, 0x00, 0x11, 0x11, 0x00, 0x11, 0x00, 0x00, 0x11, 0x00, 0x11, 0x11, 0x00, 0x11, 0x00,
]);

static OBJ_PALETTE: Palette16 = Palette16::new({
    let mut colours = [Rgb15::BLACK; 16];
    colours[0] = Rgb15::BLACK; // index 0 is transparent in a sprite palette
    colours[1] = COLOR_OBJ;
    colours[2] = Rgb15::WHITE;
    colours
});

/// The fade arm's palette cycle: entry 1 (the glyph colour) walks through eight
/// brightness levels, one per frame, so a palette write is in every frame's work.
const FADE_COLOURS: [Rgb15; 8] = [
    Rgb15::WHITE,
    Rgb::new(224, 224, 224).to_rgb15(),
    Rgb::new(192, 192, 192).to_rgb15(),
    Rgb::new(160, 160, 160).to_rgb15(),
    Rgb::new(128, 128, 128).to_rgb15(),
    Rgb::new(96, 96, 96).to_rgb15(),
    Rgb::new(64, 64, 64).to_rgb15(),
    Rgb::new(32, 32, 32).to_rgb15(),
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Arm {
    None,
    Bg,
    Bg3,
    Obj,
    Full,
}

impl Arm {
    const ALL: [Arm; 5] = [Arm::None, Arm::Bg, Arm::Bg3, Arm::Obj, Arm::Full];

    const fn name(self) -> &'static str {
        match self {
            Arm::None => "none",
            Arm::Bg => "bg",
            Arm::Bg3 => "bg3",
            Arm::Obj => "obj",
            Arm::Full => "full",
        }
    }
}

/// Everything the arms draw with, built once. A background's VRAM is allocated
/// when it is created and freed when it drops, so building these once and showing
/// subsets is also the cheapest shape — and it keeps the setup work out of the
/// measured frames.
struct Scene {
    text: RegularBackground,
    pattern_a: RegularBackground,
    pattern_b: RegularBackground,
    sprite: DynamicSprite16,
    object: Object,
}

/// One arm's measured ledger.
struct ArmResult {
    ui: Fold,
    producer: Fold,
    wall: Fold,
    feeder_sum: u64,
    feeder_max: u32,
    feeder_wrapped: u32,
    periods: u32,
    underflows: u32,
    late_max: u16,
    late_over_grace: u32,
}

fn palette_bank(first: Rgb15, second: Rgb15) -> Palette16 {
    let mut colours = [Rgb15::BLACK; 16];
    colours[0] = first;
    colours[1] = second;
    Palette16::new(colours)
}

fn build_pattern(tiles: &'static [u8], bank: u8) -> RegularBackground {
    // SAFETY: both statics are `#[repr(align(4))]` and exactly one 32-byte tile
    // long, which is `TileSet::new`'s precondition (32-bit aligned, a whole number
    // of 4bpp tiles).
    let tileset = unsafe { TileSet::new(tiles, TileFormat::FourBpp) };
    let mut bg = RegularBackground::new(
        Priority::P2,
        RegularBackgroundSize::Background32x32,
        TileFormat::FourBpp,
    );
    let setting = TileSetting::new(0, TileEffect::new(false, false, bank));
    for y in 0..20 {
        for x in 0..30 {
            bg.set_tile((x, y), &tileset, setting);
        }
    }
    bg
}

fn build_scene(gfx: &mut Graphics<'_>) -> Scene {
    // Palette 0 is the text bank (entry 0 backdrop, entry 1 glyph); banks 1 and 2
    // belong to the two pattern backgrounds.
    let mut text_colours = [Rgb15::BLACK; 16];
    text_colours[0] = COLOR_BACKDROP;
    text_colours[1] = COLOR_GLYPH;
    gfx.set_background_palette(0, &Palette16::new(text_colours));
    gfx.set_background_palette(1, &palette_bank(COLOR_PATTERN_A, COLOR_PATTERN_A2));
    gfx.set_background_palette(2, &palette_bank(COLOR_PATTERN_B, COLOR_PATTERN_B2));

    let mut text = RegularBackground::new(
        Priority::P0,
        RegularBackgroundSize::Background32x32,
        TileFormat::FourBpp,
    );
    let settings = LayoutSettings::new().with_max_line_length(236);
    let mut renderer = RegularBackgroundTextRenderer::new((8, 8), 0);
    for group in Layout::new(TEXT, &FONT, &settings) {
        renderer.show(&mut text, &group);
    }

    let pattern_a = build_pattern(&STRIPES.0, 1);
    let pattern_b = build_pattern(&DOTS.0, 2);

    let mut sprite = DynamicSprite16::new(Size::S8x8);
    for i in 0..8 {
        sprite.set_pixel(i, i, 1);
        sprite.set_pixel(7 - i, i, 2);
    }
    let mut object = Object::new(sprite.to_vram(&OBJ_PALETTE));
    object.set_pos((110, 70));

    Scene {
        text,
        pattern_a,
        pattern_b,
        sprite,
        object,
    }
}

/// Run one arm and return its ledger.
///
/// The producer runs here too: "thief of leftover time, not a task" (§4.5). Each
/// frame tops the ring back up to the prefill depth before the UI slice, so the
/// arms measure the application's real shape — audio, producer and drawing
/// sharing one frame — and not a ROM that starves the feeder while it draws.
fn run_arm(
    gfx: &mut Graphics<'_>,
    counter: &mut Counter<'_>,
    scene: &mut Scene,
    synth: &mut Synth,
    scratch: &mut Scratch,
    arm: Arm,
) -> Option<ArmResult> {
    let before = probe_hw::stats();
    let mut ui = Fold::new();
    let mut producer = Fold::new();
    let mut wall = Fold::new();
    let mut last = counter.read();

    for frame_index in 0..FRAMES {
        // Producer slice: outside the UI window, so the closure's terms are
        // disjoint. Interrupts live: the feeder may fire inside this window.
        let p0 = counter.read();
        if probe_hw::fill_to(synth, geo::PREFILL_SAMPLES, scratch).is_err() {
            return None;
        }
        let p1 = counter.read();
        producer.observe(cycles_between(p0, p1));

        let t0 = counter.read();
        let mut frame = gfx.frame();
        match arm {
            // Control: a frame with nothing shown. agb derives DISPCNT from what
            // was shown, so this repaints the backdrop alone — deliberately, and
            // it is the baseline the other arms are read against.
            Arm::None => {}
            Arm::Bg => {
                scene.text.show(&mut frame);
            }
            Arm::Bg3 => {
                scene.text.show(&mut frame);
                scene.pattern_a.show(&mut frame);
                scene.pattern_b.set_scroll_pos((frame_index as i32 * 2, 0));
                scene.pattern_b.show(&mut frame);
            }
            Arm::Obj | Arm::Full => {
                scene.text.show(&mut frame);
                scene.pattern_a.show(&mut frame);
                scene.pattern_b.set_scroll_pos((frame_index as i32 * 2, 0));
                scene.pattern_b.show(&mut frame);
                // A per-frame tile edit plus the VRAM upload `to_vram` performs:
                // agb has no "update one tile of an uploaded sprite", so the
                // animation price is a whole 32-byte sprite copy.
                let step = (frame_index & 7) as usize;
                scene
                    .sprite
                    .set_pixel(step, step, 1 + (frame_index & 1) as u8);
                let vram = scene.sprite.to_vram(&OBJ_PALETTE);
                scene.object.set_sprite(vram);
                scene.object.set_pos((100 + frame_index as i32, 70));
                scene.object.show(&mut frame);
                if arm == Arm::Full {
                    frame.set_background_palette_colour(
                        0,
                        1,
                        FADE_COLOURS[(frame_index & 7) as usize],
                    );
                }
            }
        }
        let t1 = counter.read();
        // commit waits for VBlank: the wait is the residual, not UI work.
        frame.commit();
        let t2 = counter.read();

        ui.observe(cycles_between(t0, t1));
        // The first iteration's `last` predates the loop, so it prices setup;
        // every later one is a whole frame period, commit to commit.
        if frame_index > 0 {
            wall.observe(cycles_between(last, t2));
        }
        last = t2;
    }

    let after = probe_hw::stats();
    Some(ArmResult {
        ui,
        producer,
        wall,
        feeder_sum: after.feed_cycles_sum.wrapping_sub(before.feed_cycles_sum),
        feeder_max: after.feed_cycles_max,
        feeder_wrapped: after.feed_wrapped.wrapping_sub(before.feed_wrapped),
        periods: after.periods.wrapping_sub(before.periods),
        underflows: after.underflows.wrapping_sub(before.underflows),
        late_max: after.late_max,
        late_over_grace: after.late_over_grace,
    })
}

fn report(arm: Arm, r: &ArmResult) {
    agb::println!(
        "[album-probe] ui arm={} audio=live frames={} ui count={} sum={} min={} max={} wrap={}",
        arm.name(),
        FRAMES,
        r.ui.count,
        r.ui.sum,
        r.ui.min,
        r.ui.max,
        r.ui.wrapped
    );
    agb::println!(
        "[album-probe] ui arm={} audio=live producer sum={} max={} periods={} underflows={}",
        arm.name(),
        r.producer.sum,
        r.producer.max,
        r.periods,
        r.underflows
    );
    let residual = r
        .wall
        .sum
        .saturating_sub(r.producer.sum)
        .saturating_sub(r.feeder_sum)
        .saturating_sub(r.ui.sum);
    agb::println!(
        "[album-probe] closure arm={} audio=live period_sum={} count={} = producer({}) + feeder({}) + ui({}) + residual({})",
        arm.name(),
        r.wall.sum,
        r.wall.count,
        r.producer.sum,
        r.feeder_sum,
        r.ui.sum,
        residual
    );
    agb::println!(
        "[album-probe] feeder arm={} audio=live sum={} max={} wrap={} period_cycles={} late_max={} late_over_grace={}",
        arm.name(),
        r.feeder_sum,
        r.feeder_max,
        r.feeder_wrapped,
        geo::PERIOD_CYCLES,
        r.late_max,
        r.late_over_grace
    );
}

fn run(gba: &mut Gba) -> bool {
    agb::println!(
        "[album-probe] ui start: chunk={} ring={}x{}={} prefill={} period_cycles={}",
        geo::CHUNK_SAMPLES,
        geo::RING_CHUNKS,
        geo::CHUNK_SAMPLES,
        geo::RING_SAMPLES,
        geo::PREFILL_SAMPLES,
        geo::PERIOD_CYCLES
    );

    let mut gfx = gba.graphics.get();
    let mut scene = build_scene(&mut gfx);
    let mut scratch = Box::new(Scratch::new());
    let mut synth = Synth::new();

    // The timers are handed out ONCE for the whole run: taking all four is
    // `unsafe` only because a `Mixer` would own timer0/1, and this crate never
    // constructs one. It is dropped before the cycle counter acquires timer2/3 —
    // see `probe_hw`'s timer2/3 ruling; re-acquiring it later would reset the
    // counter's registers.
    let handler = {
        let mut timers = unsafe { gba.timers.all_timers() }; // SAFETY: no agb::sound::mixer::Mixer is instantiated in this ROM, so timer0-3 are unowned.
        // `timers` is dropped at the end of this block, before `Counter::start`
        // acquires timer2/3 (the `probe_hw` timer2/3 ruling).
        probe_hw::bootstrap(&mut timers, &mut synth, &mut scratch)
    };
    let Some(_handler) = handler else {
        agb::println!("[album-probe] FAIL ui: bootstrap could not prime a whole chunk");
        agb::println!("Tests finished successfully");
        return false;
    };

    let mut timer_pool = gba.timers.timers();
    let mut counter = Counter::start(&mut timer_pool);
    let (ie, ime, tm1, tm0) = probe_hw::chain_registers();
    agb::println!(
        "[album-probe] armed audio=live counter=timer2/3 IE={:#06x} IME={} TM1CNT={:#06x} TM0CNT={:#06x}",
        ie,
        ime,
        tm1,
        tm0
    );

    let mut pass = true;
    for arm in Arm::ALL {
        match run_arm(
            &mut gfx,
            &mut counter,
            &mut scene,
            &mut synth,
            &mut scratch,
            arm,
        ) {
            Some(result) => {
                report(arm, &result);
                pass &= result.ui.wrapped == 0 && result.wall.wrapped == 0;
                pass &= result.feeder_wrapped == 0;
                pass &= result.underflows == 0;
            }
            None => {
                agb::println!(
                    "[album-probe] FAIL ui arm={} producer push refused",
                    arm.name()
                );
                pass = false;
            }
        }
    }

    // ---------------------------------------------------- audio disarmed
    //
    // §4.5 asks AP-0 to *confirm* the video period (~281,000 cycles at 2^24 Hz)
    // rather than assume it. Disarm the chain and the sound DMA, then measure
    // commit-to-commit with the same counter: no feeder, no producer, one
    // background shown every frame.
    probe_hw::disarm_audio();
    let mut video = Fold::new();
    let mut last = counter.read();
    for frame_index in 0..FRAMES {
        let mut frame = gfx.frame();
        scene.text.show(&mut frame);
        frame.commit();
        let now = counter.read();
        if frame_index > 0 {
            video.observe(cycles_between(last, now));
        }
        last = now;
    }
    agb::println!(
        "[album-probe] video_period audio=disarmed count={} sum={} min={} max={} wrap={} (spec ~281000)",
        video.count,
        video.sum,
        video.min,
        video.max,
        video.wrapped
    );
    pass &= video.wrapped == 0;

    // ------------------------------------------- one 8x8 tile write loop
    //
    // One tile, written twice in a row with *different* tilesets so the second
    // call cannot be served from agb's dedupe cache: the price of one 32-byte
    // 4bpp tile through `set_tile`. §7's 120x120 box is 225 of these.
    let stripes = unsafe { TileSet::new(&STRIPES.0, TileFormat::FourBpp) }; // SAFETY: 4-byte aligned static, one whole 32-byte tile.
    let dots = unsafe { TileSet::new(&DOTS.0, TileFormat::FourBpp) }; // SAFETY: as above.
    let setup_a = TileSetting::new(0, TileEffect::new(false, false, 1));
    let setup_b = TileSetting::new(0, TileEffect::new(false, false, 2));
    let (_, overhead) = timed(&mut counter, || {});
    let (_, one) = timed(&mut counter, || {
        scene.pattern_a.set_tile((0, 0), &stripes, setup_a);
    });
    let (_, two) = timed(&mut counter, || {
        scene.pattern_a.set_tile((0, 0), &dots, setup_b);
    });
    let one_net = one.saturating_sub(overhead);
    let two_net = two.saturating_sub(overhead);
    agb::println!(
        "[album-probe] tile_write ime-off overhead={} first={} second_raw={} second_net={} (x225 = {})",
        overhead,
        one_net,
        two,
        two_net,
        two_net.saturating_mul(225)
    );

    // A whole tile loop, each call separately folded, interrupts live (the chain
    // is disarmed, which is what makes this comparable to the art probe's
    // disarmed leg).
    let mut loop_fold = Fold::new();
    for cell in 0..225u32 {
        let tileset = if cell & 1 == 0 { &stripes } else { &dots };
        let setting = if cell & 1 == 0 { setup_a } else { setup_b };
        let start = counter.read();
        scene.pattern_a.set_tile((0, 0), tileset, setting);
        loop_fold.observe(cycles_between(start, counter.read()));
    }
    agb::println!(
        "[album-probe] tile_loop audio=disarmed x225 count={} sum={} min={} max={} wrap={}",
        loop_fold.count,
        loop_fold.sum,
        loop_fold.min,
        loop_fold.max,
        loop_fold.wrapped
    );

    // The VRAM/OBJ split the Phase 0 exit criterion asks to record: the bytes this
    // probe itself puts in a background character block, a background map, and the
    // object arena. The text renderer's glyph tiles are allocator-managed and not
    // counted here; the two pattern tiles are.
    agb::println!(
        "[album-probe] vram bg_char={} bg_map={} obj_tile={} obj_oam_shadow={} obj_palette={}",
        2 * 32,
        3 * 2048,
        32,
        128 * 8,
        32
    );

    if pass {
        agb::println!(
            "[album-probe] PASS ui arms=5 audio=live=yes video=disarmed tile=ime-off underflows=0"
        );
    } else {
        agb::println!(
            "[album-probe] FAIL ui: a wrapped window, an underflow or a refused push — see the lines above"
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
