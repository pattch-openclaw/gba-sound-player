//! The render probe: AP-3b's four legs, measured on the app's own drawing shape
//! (`ALBUM-PLAYER.md` §13 AP-3b, §11's 2b PR-(2)).
//!
//! Everything §5.1, §5.5, §7 and §10.2 say about UI cost is arithmetic on AP-0's
//! single-tile upload legs. This bin is where those tags come off:
//!
//! | leg | what it measures |
//! |---|---|
//! | **L1** | per-element redraw, changed cells only, live audio: one bar cell, one changed clock digit (two tile rows), and a re-show of an *unchanged* run — the `set_tile_at_pos` early-out path, which has never been timed |
//! | **L2** | a whole scene-region change, 600 cells (30x20), live audio |
//! | **L3** | a `set_priority` switch with both backgrounds resident, timed across a commit boundary — the first `set_priority` call in this tree |
//! | **L4** | the tile-pool leg: stand up art + chrome + a 30x8 list at the 16 px pitch from one shared atlas, then read what agb actually allocated from the maps and the tile data |
//!
//! **No product code draws any of this.** The probe is the instrument; the phase
//! that promotes it is 2b PR-(5), and only if AP-3c says the presentation path
//! should move.
//!
//! ## Why the pool leg reads maps instead of refcounts
//!
//! agb's `reference_counts` is private (`display/tiled/vram_manager.rs`), so a
//! probe cannot print a refcount. What it *can* read is the consequence, and the
//! consequence is what a total-only check misses: every cell's map entry carries
//! the VRAM slot agb allocated for it. If a shared atlas deduped, a repeated glyph
//! appears in many cells with **one** slot; if the renderer fell back to per-cell
//! allocation, every cell has its own slot and the *distinct* count equals the
//! *cell* count. So L4 prints refs (cell references), distinct slots, the maximum
//! multiplicity, and how many slots are shared by two or more cells — and it
//! asserts the maximum is greater than one. A count of one on every slot is the
//! silent-fallback signature, and it is exactly what a total-only check reads as a
//! pass.
//!
//! The hidden scene's map is snapshotted before L3 and compared after it, tile
//! words included, because §5.1's residency risk is that a hidden scene's map
//! points at a tile something else has since freed. A freed slot's data changes
//! (or the map does); either shows up here.
//!
//! ## Measurement discipline
//!
//! Every window is taken with interrupts live (`Counter::read` +
//! `cycles_between`), because the cost in question is what the application pays
//! while the audio chain runs. The producer is topped up between windows and never
//! inside one, so the window is the drawing and not the feeding. The ROM prints
//! sums, counts, minima and maxima; means and shares are derived host-side (§6
//! keeps a divide off the measured path).

#![no_std]
#![no_main]

extern crate alloc;

use alloc::boxed::Box;
use alloc::vec;

use agb::Gba;
use agb::display::{
    Palette16, Priority, Rgb, Rgb15,
    tiled::{
        RegularBackground, RegularBackgroundSize, TileEffect, TileFormat, TileSet, TileSetting,
    },
};

use album_probe::probe_hw::{
    self, Fold, Scratch,
    timer::{Counter, cycles_between},
};
use album_probe::render_hw;
use album_probe::synth::Synth;

/// How many times each L1 element is redrawn (alternating content, so the write
/// is never an early-out unless the leg means it to be).
const ELEMENT_REPS: u32 = 60;
/// Whole 600-cell region repaints. Eight windows is enough for the allocator's
/// first-pass growth to land in one of them and still leave a usable fold.
const REGION_REPS: u32 = 8;
/// Priority switches with both scenes resident.
const SWITCH_REPS: u32 = 8;
/// The scene region's visible rows (16 tile rows = 128 px, §5.0's variant B).
const REGION_ROWS: usize = 16;
/// The list's visible text rows at the 16 px pitch: `floor(128 / 16)` = 8, and
/// the probe writes them as two tile rows each, which is what the pool sum sees.
const LIST_ROWS: usize = 8;
/// The list window is the region's rows at a 16 px pitch, not a number from
/// prose: `LIST_ROWS * 2` tile rows must be the region's 16.
const _: () = assert!(LIST_ROWS * 2 == REGION_ROWS);
/// Columns in the 240 px line the probe actually draws (30 cells, one per tile
/// column — the shipped surface's cell model, §5.2).
const COLS: usize = 30;

const COLOR_BACKDROP: Rgb15 = Rgb::new(16, 16, 24).to_rgb15();
const COLOR_TEXT: Rgb15 = Rgb15::WHITE;
const COLOR_FILL: Rgb15 = Rgb::new(0, 48, 64).to_rgb15();
const COLOR_ART: Rgb15 = Rgb::new(96, 32, 96).to_rgb15();
const COLOR_PASS: Rgb15 = Rgb::new(0, 0, 255).to_rgb15();
const COLOR_FAIL: Rgb15 = Rgb::new(255, 0, 0).to_rgb15();

/// `TileSet::new`'s two preconditions are stated at the call: the banks come from
/// `render_hw`, which leaks 4-byte-aligned whole-tile buffers.
fn tileset(bytes: &'static [u8]) -> TileSet {
    // SAFETY: `render_hw`'s `leak_tiles` guarantees 4-byte alignment and a whole
    // number of 32-byte 4bpp tiles.
    unsafe { TileSet::new(bytes, TileFormat::FourBpp) }
}

fn setting(tile: u16, bank: u8) -> TileSetting {
    TileSetting::new(tile, TileEffect::new(false, false, bank))
}

/// Fill a rectangle of a 32x32 background, every visible cell explicit — the same
/// rule the product surface follows, and here it also keeps the map reads honest:
/// a cell left at agb's blank index would read whatever tile index 0 happens to
/// be, which is not this probe's allocation.
fn fill(
    bg: &mut RegularBackground,
    x0: usize,
    y0: usize,
    w: usize,
    h: usize,
    ts: &TileSet,
    f: impl Fn(usize, usize) -> u16,
    bank: u8,
) {
    for y in 0..h {
        for x in 0..w {
            bg.set_tile(
                (x0 as i32 + x as i32, y0 as i32 + y as i32),
                ts,
                setting(f(x, y), bank),
            );
        }
    }
}

/// One element's redraw ledger: the fold, plus how many cells one call wrote.
struct Element {
    fold: Fold,
}

/// Top the ring up before a measured window, never inside one.
fn producer(synth: &mut Synth, scratch: &mut Scratch) -> bool {
    probe_hw::fill_to(synth, probe_hw::PREFILL_SAMPLES, scratch).is_ok()
}

/// L1: alternate two tile indices in one cell and fold each `set_tile`.
fn element_cell(
    bg: &mut RegularBackground,
    counter: &mut Counter<'_>,
    ts: &TileSet,
    pos: (usize, usize),
    bank: u8,
    synth: &mut Synth,
    scratch: &mut Scratch,
) -> Option<Element> {
    let mut fold = Fold::new();
    let mut which = false;
    for _ in 0..ELEMENT_REPS {
        if !producer(synth, scratch) {
            return None;
        }
        let tile = if which { 6 } else { 7 };
        which = !which;
        let start = counter.read();
        bg.set_tile((pos.0 as i32, pos.1 as i32), ts, setting(tile, bank));
        fold.observe(cycles_between(start, counter.read()));
    }
    Some(Element { fold })
}

/// L1's unchanged leg: re-show the same `(tileset, index)` a run of times. agb
/// still allocates (a dedupe hit) and releases the old reference before comparing
/// the entries, so this is the honest price of "already showing what you asked
/// for", not zero.
fn element_unchanged(
    bg: &mut RegularBackground,
    counter: &mut Counter<'_>,
    ts: &TileSet,
    pos: (usize, usize),
    tile: u16,
    bank: u8,
    cells: usize,
    synth: &mut Synth,
    scratch: &mut Scratch,
) -> Option<Element> {
    // Put the run on the content the leg will re-show, *outside* the fold: the
    // first write is a change, and folding it would price a changed cell into
    // an unchanged one. The leg then measures the same `(tileset, index)` written
    // again and again.
    for i in 0..cells {
        bg.set_tile(
            (pos.0 as i32 + i as i32, pos.1 as i32),
            ts,
            setting(tile, bank),
        );
    }
    let mut fold = Fold::new();
    for _ in 0..ELEMENT_REPS {
        if !producer(synth, scratch) {
            return None;
        }
        let start = counter.read();
        for i in 0..cells {
            bg.set_tile(
                (pos.0 as i32 + i as i32, pos.1 as i32),
                ts,
                setting(tile, bank),
            );
        }
        fold.observe(cycles_between(start, counter.read()));
    }
    Some(Element { fold })
}

/// L2: rewrite all 600 visible cells of a 30x20 background with content that has
/// changed, one whole window per fold sample.
fn region_repaint(
    bg: &mut RegularBackground,
    counter: &mut Counter<'_>,
    ts: &TileSet,
    bank: u8,
    synth: &mut Synth,
    scratch: &mut Scratch,
) -> Option<Fold> {
    let mut fold = Fold::new();
    for pass in 0..REGION_REPS {
        if !producer(synth, scratch) {
            return None;
        }
        let start = counter.read();
        for y in 0..20usize {
            for x in 0..30usize {
                let tile = ((x + y * 3 + pass as usize) % 25) as u16;
                bg.set_tile((x as i32, y as i32), ts, setting(tile, bank));
            }
        }
        fold.observe(cycles_between(start, counter.read()));
    }
    Some(fold)
}

/// What L4 read out of the maps.
struct Pool {
    /// Cell references: every visible cell of the four backgrounds.
    refs: u32,
    /// Distinct VRAM slots those cells point at — the live tile count.
    distinct: u32,
    /// How many slots two or more cells share.
    shared: u32,
    /// The largest number of cells pointing at one slot.
    max_refs: u32,
    /// The list window's map, snapshotted for the hidden-scene leg.
    list_map: Box<[u16]>,
    /// First tile word of every distinct slot, for the freeze check.
    tile_words: Box<[u32]>,
    /// The list window's five background control registers, for the switch leg.
    bg_priorities: [u8; 4],
}

/// The windows L4 counts, as `(bg index, x0, y0, w, h)`:
/// BG0 art (16x16 at the origin), BG1 list (region rows), BG2 chrome (the two
/// bottom rows), BG3 foreground (the title band's two rows, §5.0's 2-row band).
/// Show order is the probe's and is what fixes the indices — agb assigns BGx to
/// the order backgrounds are `show`n into a frame.
const WINDOWS: [(usize, usize, usize, usize, usize); 4] = [
    (0, 0, 0, 16, 16),
    (1, 0, 0, COLS, LIST_ROWS * 2),
    (2, 0, 18, COLS, 2),
    (3, 0, 16, COLS, 2),
];

fn read_pool() -> Pool {
    let mut counts = [0u32; 1024];
    let mut refs = 0u32;
    for &(bg, x0, y0, w, h) in &WINDOWS {
        for y in 0..h {
            for x in 0..w {
                let index = (render_hw::map_entry(bg, x0 + x, y0 + y) & 0x03ff) as usize;
                counts[index] = counts[index].wrapping_add(1);
                refs = refs.wrapping_add(1);
            }
        }
    }
    let mut distinct = 0u32;
    let mut shared = 0u32;
    let mut max_refs = 0u32;
    let mut tile_words = vec![0u32; 1024].into_boxed_slice();
    for (index, &count) in counts.iter().enumerate() {
        if count > 0 {
            distinct = distinct.wrapping_add(1);
            if count > 1 {
                shared = shared.wrapping_add(1);
                if count > max_refs {
                    max_refs = count;
                }
            }
            tile_words[index] = render_hw::bg_tile_word(index);
        }
    }

    let mut list_map = vec![0u16; COLS * LIST_ROWS * 2].into_boxed_slice();
    for y in 0..LIST_ROWS * 2 {
        for x in 0..COLS {
            list_map[y * COLS + x] = render_hw::map_entry(1, x, y);
        }
    }

    let mut bg_priorities = [0u8; 4];
    for (bg, slot) in bg_priorities.iter_mut().enumerate() {
        *slot = render_hw::bg_priority(bg);
    }

    Pool {
        refs,
        distinct,
        shared,
        max_refs,
        list_map,
        tile_words,
        bg_priorities,
    }
}

/// The hidden-scene leg: the map entries and tile words a *hidden* background
/// still points at must be the ones it pointed at before the switch. A slot freed
/// by another scene's rewrite shows up as changed data here.
fn hidden_resolves(pool: &Pool) -> bool {
    for y in 0..LIST_ROWS * 2 {
        for x in 0..COLS {
            let now = render_hw::map_entry(1, x, y);
            if now != pool.list_map[y * COLS + x] {
                return false;
            }
            let index = (now & 0x03ff) as usize;
            if render_hw::bg_tile_word(index) != pool.tile_words[index] {
                return false;
            }
        }
    }
    true
}

fn report(label: &str, fold: &Fold, cells: u32) {
    agb::println!(
        "[album-probe] render {} count={} sum={} min={} max={} wrap={} cells={}",
        label,
        fold.count,
        fold.sum,
        fold.min,
        fold.max,
        fold.wrapped,
        cells
    );
}

fn run(gba: &mut Gba) -> bool {
    let atlas = render_hw::make_atlas();
    let icons = render_hw::make_icons();
    let art = render_hw::make_art();
    let atlas_ts = tileset(atlas);
    let icons_ts = tileset(icons);
    let art_ts = tileset(art);

    let mut gfx = gba.graphics.get();
    // Bank 0: backdrop + the region fill. Bank 1: text, icons, bar. Bank 2: art.
    let mut b0 = [Rgb15::BLACK; 16];
    b0[0] = COLOR_BACKDROP;
    b0[1] = COLOR_FILL;
    gfx.set_background_palette(0, &Palette16::new(b0));
    let mut b1 = [Rgb15::BLACK; 16];
    b1[0] = COLOR_BACKDROP;
    b1[1] = COLOR_TEXT;
    b1[2] = Rgb::new(255, 200, 64).to_rgb15();
    gfx.set_background_palette(1, &Palette16::new(b1));
    let mut b2 = [Rgb15::BLACK; 16];
    b2[0] = COLOR_BACKDROP;
    b2[1] = COLOR_ART;
    b2[2] = Rgb::new(160, 96, 160).to_rgb15();
    b2[3] = Rgb::new(64, 160, 96).to_rgb15();
    gfx.set_background_palette(2, &Palette16::new(b2));

    // The four layers, built in the order the probe will show them, which is what
    // fixes BG0..BG3 for the register reads below.
    let mut art_bg = RegularBackground::new(
        Priority::P1,
        RegularBackgroundSize::Background32x32,
        TileFormat::FourBpp,
    );
    fill(
        &mut art_bg,
        0,
        0,
        16,
        16,
        &art_ts,
        |x, y| (y * 16 + x) as u16,
        2,
    );

    let mut list_bg = RegularBackground::new(
        Priority::P2,
        RegularBackgroundSize::Background32x32,
        TileFormat::FourBpp,
    );
    // Eight text rows at the 16 px pitch: two tile rows per row, one glyph per
    // cell, twenty cells of a repeating title-and-duration shape.
    for row in 0..LIST_ROWS {
        for tile_row in 0..2 {
            for col in 0..COLS {
                let glyph = ((col * 7 + row * 3 + tile_row) % render_hw::GLYPHS) as u16;
                list_bg.set_tile(
                    (col as i32, (row * 2 + tile_row) as i32),
                    &atlas_ts,
                    setting(glyph * 2 + tile_row as u16, 1),
                );
            }
        }
    }

    let mut chrome_bg = RegularBackground::new(
        Priority::P0,
        RegularBackgroundSize::Background32x32,
        TileFormat::FourBpp,
    );
    // The two always-shown rows (§5.0): a fill, five control icons, ten bar cells.
    fill(&mut chrome_bg, 0, 18, COLS, 2, &icons_ts, |_, _| 5, 1);
    for icon in 0..5u16 {
        chrome_bg.set_tile((1 + icon as i32, 18), &icons_ts, setting(icon, 1));
    }
    for cell in 0..10u16 {
        chrome_bg.set_tile((18 + cell as i32, 19), &icons_ts, setting(6 + cell, 1));
    }

    let mut fg_bg = RegularBackground::new(
        Priority::P3,
        RegularBackgroundSize::Background32x32,
        TileFormat::FourBpp,
    );
    // The title band's text run and the bar, drawn from the same shared atlas.
    for col in 0..COLS {
        let glyph = ((col * 5) % render_hw::GLYPHS) as u16;
        fg_bg.set_tile((col as i32, 16), &atlas_ts, setting(glyph * 2, 1));
        fg_bg.set_tile((col as i32, 17), &atlas_ts, setting(glyph * 2 + 1, 1));
    }

    // One boot commit: the only commit before the chain starts (§5.7's legitimate
    // sites), and what writes the maps L4 reads.
    {
        let mut frame = gfx.frame();
        art_bg.show(&mut frame);
        list_bg.show(&mut frame);
        chrome_bg.show(&mut frame);
        fg_bg.show(&mut frame);
        frame.commit();
    }

    // ---------------------------------------------------------------- L4 first
    //
    // The pool leg is a setup fact, so it runs before the audio chain is armed:
    // the maps are written, the allocation is standing, and no commit is needed
    // inside a measured window. `refs` counts cells; `distinct` counts slots.
    let pool = read_pool();
    let mut pass = pool.distinct <= 512 && pool.max_refs > 1;
    agb::println!(
        "[album-probe] render pool audio=disarmed refs={} distinct={} shared={} max_refs={} pool=512 atlas_tiles={} icon_tiles={} art_tiles={}",
        pool.refs,
        pool.distinct,
        pool.shared,
        pool.max_refs,
        render_hw::ATLAS_TILES,
        render_hw::ICON_TILES,
        render_hw::ART_TILES
    );
    // The three terms this stands up, printed separately so the total is
    // checkable by hand: 256 art + 188 atlas + 16 icons = 460 slots, plus the
    // shared page fill.
    agb::println!(
        "[album-probe] render pool_terms atlas={} icons={} art={} expected_distinct<={}",
        render_hw::ATLAS_TILES,
        render_hw::ICON_TILES,
        render_hw::ART_TILES,
        render_hw::ATLAS_TILES + render_hw::ICON_TILES + render_hw::ART_TILES + 1
    );
    if pool.max_refs <= 1 {
        agb::println!(
            "[album-probe] FAIL render: every tile slot is referenced once — the shared-atlas dedupe did not happen, which a total-only check would read as a pass"
        );
    }

    // ------------------------------------------------------- arm the audio
    let mut scratch = Box::new(Scratch::new());
    let mut synth = Synth::new();
    let handler = {
        let mut timers = unsafe { gba.timers.all_timers() }; // SAFETY: no agb::sound::mixer::Mixer is instantiated in this ROM, so timer0-3 are unowned.
        probe_hw::bootstrap(&mut timers, &mut synth, &mut scratch)
    };
    let Some(_handler) = handler else {
        agb::println!("[album-probe] FAIL render: bootstrap could not prime a whole chunk");
        agb::println!("Tests finished successfully");
        return false;
    };
    let mut timer_pool = gba.timers.timers();
    let mut counter = Counter::start(&mut timer_pool);
    let before = probe_hw::stats();

    // ------------------------------------------------------------------- L1
    let bar = element_cell(
        &mut fg_bg,
        &mut counter,
        &atlas_ts,
        (24, 17),
        1,
        &mut synth,
        &mut scratch,
    );
    let clock = element_cell(
        &mut fg_bg,
        &mut counter,
        &atlas_ts,
        (10, 16),
        1,
        &mut synth,
        &mut scratch,
    );
    // A clock digit occupies two tile rows; the leg above folds one cell, so the
    // second row is folded as its own element and the pair is the digit's price.
    let clock_low = element_cell(
        &mut fg_bg,
        &mut counter,
        &atlas_ts,
        (10, 17),
        1,
        &mut synth,
        &mut scratch,
    );
    // The unchanged run: five title cells re-shown with the tile they already
    // hold. This is the early-out path — refcount churn and no map dirty.
    let unchanged = element_unchanged(
        &mut fg_bg,
        &mut counter,
        &atlas_ts,
        (0, 17),
        0,
        1,
        5,
        &mut synth,
        &mut scratch,
    );

    let (Some(bar), Some(clock), Some(clock_low), Some(unchanged)) =
        (bar, clock, clock_low, unchanged)
    else {
        agb::println!("[album-probe] FAIL render: producer push refused during L1");
        agb::println!("Tests finished successfully");
        return false;
    };

    report("l1_bar_cell", &bar.fold, 1);
    report("l1_clock_digit", &clock.fold, 1);
    report("l1_clock_low", &clock_low.fold, 1);
    report("l1_unchanged_run", &unchanged.fold, 5);
    agb::println!(
        "[album-probe] render l1_digit sum={} count={} cells=2",
        clock.fold.sum + clock_low.fold.sum,
        clock.fold.count
    );
    pass &= bar.fold.wrapped == 0
        && clock.fold.wrapped == 0
        && clock_low.fold.wrapped == 0
        && unchanged.fold.wrapped == 0;

    // ------------------------------------------------------------------- L2
    let region = region_repaint(
        &mut art_bg,
        &mut counter,
        &art_ts,
        2,
        &mut synth,
        &mut scratch,
    );
    let Some(region) = region else {
        agb::println!("[album-probe] FAIL render: producer push refused during L2");
        agb::println!("Tests finished successfully");
        return false;
    };
    report("l2_region_600", &region, 600);
    pass &= region.wrapped == 0;

    // ------------------------------------------------------------------- L3
    //
    // Both content backgrounds are resident and shown every frame, so the switch
    // is the priority write itself — not a map rewrite and not an upload. The
    // priority value only reaches BGxCNT on the next `show` + `commit`
    // (`regular_background.rs:435-447`), so the leg times the flip across the
    // commit boundary and reads the registers back to prove it landed.
    let mut prio = Fold::new();
    let mut commit = Fold::new();
    let mut whole = Fold::new();
    let mut landed = 0u32;
    for i in 0..SWITCH_REPS {
        if !producer(&mut synth, &mut scratch) {
            agb::println!("[album-probe] FAIL render: producer push refused during L3");
            agb::println!("Tests finished successfully");
            return false;
        }
        let start = counter.read();
        let t0 = counter.read();
        art_bg.set_priority(if i & 1 == 0 {
            Priority::P2
        } else {
            Priority::P1
        });
        list_bg.set_priority(if i & 1 == 0 {
            Priority::P1
        } else {
            Priority::P2
        });
        prio.observe(cycles_between(t0, counter.read()));
        let t1 = counter.read();
        {
            let mut frame = gfx.frame();
            art_bg.show(&mut frame);
            list_bg.show(&mut frame);
            chrome_bg.show(&mut frame);
            fg_bg.show(&mut frame);
            frame.commit();
        }
        commit.observe(cycles_between(t1, counter.read()));
        whole.observe(cycles_between(start, counter.read()));
        // BG0 is art and BG1 is list (show order above). After an even flip art
        // is P2/list P1; after an odd one the reverse.
        let (want_art, want_list) = if i & 1 == 0 { (2, 1) } else { (1, 2) };
        if render_hw::bg_priority(0) == want_art && render_hw::bg_priority(1) == want_list {
            landed = landed.wrapping_add(1);
        }
    }
    report("l3_set_priority", &prio, 2);
    report("l3_commit", &commit, 1);
    report("l3_whole_switch", &whole, 1);
    agb::println!(
        "[album-probe] render l3_landed flips={} landed={} bg0_prio={} bg1_prio={} (before={:?})",
        SWITCH_REPS,
        landed,
        render_hw::bg_priority(0),
        render_hw::bg_priority(1),
        pool.bg_priorities
    );
    pass &= landed == SWITCH_REPS && prio.wrapped == 0 && commit.wrapped == 0 && whole.wrapped == 0;

    // The hidden scene after all of the above: its map and its tile data must be
    // the ones L4 snapshotted.
    let resolves = hidden_resolves(&pool);
    agb::println!(
        "[album-probe] render hidden_scene map_entries={} resolves={}",
        COLS * LIST_ROWS * 2,
        if resolves { "yes" } else { "no" }
    );
    pass &= resolves;

    let after = probe_hw::stats();
    let underflows = after.underflows.wrapping_sub(before.underflows);
    agb::println!(
        "[album-probe] render audio periods={} feeder_sum={} feeder_max={} late_max={} late_over_grace={} underflows={}",
        after.periods.wrapping_sub(before.periods),
        after.feed_cycles_sum.wrapping_sub(before.feed_cycles_sum),
        after.feed_cycles_max,
        after.late_max,
        after.late_over_grace,
        underflows
    );
    pass &= underflows == 0;

    if pass {
        agb::println!(
            "[album-probe] PASS render legs=4 pool_distinct={} pool_refs={} max_refs={} unchanged_cells=5 hidden=yes underflows=0",
            pool.distinct,
            pool.refs,
            pool.max_refs
        );
    } else {
        agb::println!(
            "[album-probe] FAIL render: a wrapped window, a refcount that reads as a fallback, a switch that did not land, a hidden map that moved, or an underflow — see the lines above"
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
