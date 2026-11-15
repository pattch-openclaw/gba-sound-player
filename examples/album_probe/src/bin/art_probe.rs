//! Art spike: a hand-written `ART1` on screen from the committed blob, with the
//! upload's price measured (ALBUM-PLAYER.md §11 Phase 0 (b), §13 AP-0).
//!
//! The blob is validated through [`album_probe::art1::parse`] at boot — the same
//! reader the host suite drives — and a refusal is printed **by name** and fails
//! the run before anything is uploaded. That order is the point: a probe that
//! uploaded a blob it could not validate would measure VRAM traffic for an image
//! nobody proved was the image.
//!
//! ## What the upload measures
//!
//! agb 0.25's `RegularBackground` has **no bulk tile or map upload** — the only
//! doors are `set_tile` (one cell, one tile copy), `set_tile_dynamic16/256`,
//! `swap_tiles` and `fill_with(&TileData)`. So the price of putting a 120x120
//! 4bpp image on screen is one `set_tile` per cell, 225 of them, which is exactly
//! what this probe times. The committed blob's 225 tiles are all distinct
//! (`art_pin::N_TILES`), so the map pass is a genuine 225-tile VRAM copy and the
//! per-tile mean is a copy, not a dedupe hit.
//!
//! The same upload is timed twice: once with the audio chain **live** (the
//! question risk R3 asks — do VRAM writes on the shared bus cost more while the
//! sound DMA runs?) and once **disarmed**. Every number carries its label.
//!
//! ROM `.text`/`.iwram`/`.bss` come from the built ELF (`make album-probe-rom`
//! prints them with `size -A`), never from a second map build; the static sizes
//! below are `size_of` facts about this image.

#![no_std]
#![no_main]

extern crate alloc;

use alloc::boxed::Box;

use agb::Gba;
use agb::display::{
    Graphics, Palette16, Priority, Rgb15,
    tiled::{
        RegularBackground, RegularBackgroundSize, TileEffect, TileFormat, TileSet, TileSetting,
    },
};

use album_probe::art_pin;
use album_probe::art1::{self, Art1};
use album_probe::probe_hw::{
    self, Fold, Scratch,
    timer::{Counter, cycles_between},
};
use album_probe::synth::Synth;

/// The committed blob, copied into a 4-byte-aligned static so `TileSet::new`'s
/// alignment precondition (it hands the byte slice to VRAM as `u32`s) holds. The
/// tiles section starts at a multiple of 4 inside it, so every tile address is
/// 4-aligned too.
#[repr(align(4))]
struct AlignedBlob([u8; art_pin::BLOB_BYTES]);

static ART_BLOB: AlignedBlob = AlignedBlob(*include_bytes!("../../assets/probe_art1.bin"));

/// Frames of ordinary VBlank-paced running before the live upload, so "audio
/// live" means `periods > 0` and the feeder has actually serviced periods — an
/// upload that finished before the first period boundary would be labelled live
/// and measure nothing of the kind.
const WARMUP_FRAMES: u32 = 8;

const COLOR_BACKDROP: Rgb15 = Rgb15::new(0);
const COLOR_PASS: Rgb15 = agb::display::Rgb::new(0, 0, 255).to_rgb15();
const COLOR_FAIL: Rgb15 = agb::display::Rgb::new(255, 0, 0).to_rgb15();

/// What one upload cost.
struct Upload {
    /// The whole job — palettes, background allocation, every map cell — as one
    /// interrupt-live window.
    whole: u32,
    /// Each `set_tile` call on its own, so the mean and the worst are recoverable
    /// without a divide in the ROM.
    per_tile: Fold,
    /// The background, kept alive so the image is genuinely on screen.
    background: RegularBackground,
}

/// Upload the blob: palettes into banks 0..n, then one `set_tile` per map cell.
fn upload(gfx: &mut Graphics<'_>, counter: &mut Counter<'_>, art: &Art1<'static>) -> Upload {
    let start = counter.read();

    for bank in 0..art.n_palette_banks {
        let entries = art.palette_bank(bank);
        let mut colours = [Rgb15::BLACK; 16];
        for (dst, src) in colours.iter_mut().zip(entries.iter()) {
            *dst = Rgb15::new(*src);
        }
        gfx.set_background_palette(bank as u8, &Palette16::new(colours));
    }

    // SAFETY: `art.tiles` points into `ART_BLOB`, which is `repr(align(4))`, and
    // is a whole number of 32-byte 4bpp tiles (both validated by `art1::parse`
    // and pinned by `art_pin`). The blob outlives the background.
    let tileset = unsafe { TileSet::new(art.tiles, TileFormat::FourBpp) };
    let mut background = RegularBackground::new(
        Priority::P1,
        RegularBackgroundSize::Background32x32,
        TileFormat::FourBpp,
    );

    let mut per_tile = Fold::new();
    let cols = art.tile_cols;
    // §6 keeps a divide off the measured path, so `pos` is carried as a (col, row)
    // pair and bumped per cell. A divide by a runtime divisor cannot be
    // strength-reduced on thumbv4t: `cell % cols` / `cell / cols` compiles to a
    // `bl __aeabi_uidiv` call per cell inside the loop, and the carried pair is
    // what removes that call.
    let mut pos = (0i32, 0i32);
    for cell in 0..art.n_tiles() {
        let entry = art.map_entry(cell);
        let setting = TileSetting::new(
            entry & 0x03ff,
            TileEffect::new(
                entry & (1 << 10) != 0,
                entry & (1 << 11) != 0,
                ((entry >> 12) & 0x0f) as u8,
            ),
        );
        let t = counter.read();
        background.set_tile(pos, &tileset, setting);
        per_tile.observe(cycles_between(t, counter.read()));
        pos.0 += 1;
        if pos.0 == cols as i32 {
            pos = (0, pos.1 + 1);
        }
    }

    Upload {
        whole: cycles_between(start, counter.read()),
        per_tile,
        background,
    }
}

fn report(label: &str, art: &Art1<'static>, up: &Upload) {
    agb::println!(
        "[album-probe] art upload={} audio={} bytes={} tiles={} whole={} per_tile count={} sum={} min={} max={} wrap={}",
        label,
        if label == "live" { "live" } else { "disarmed" },
        art_pin::BLOB_BYTES,
        art.n_tiles(),
        up.whole,
        up.per_tile.count,
        up.per_tile.sum,
        up.per_tile.min,
        up.per_tile.max,
        up.per_tile.wrapped
    );
}

fn run(gba: &mut Gba) -> bool {
    // --------------------------------------------------------- validate first
    let blob = &ART_BLOB.0;
    let art = match art1::parse(blob) {
        Ok(art) => art,
        Err(refusal) => {
            agb::println!("[album-probe] FAIL art: blob refused at boot: {}", refusal);
            agb::println!("Tests finished successfully");
            return false;
        }
    };
    let blob_bytes = if blob.len() == art_pin::BLOB_BYTES {
        art_pin::BLOB_BYTES
    } else {
        agb::println!(
            "[album-probe] FAIL art: blob is {} bytes, the pin says {}",
            blob.len(),
            art_pin::BLOB_BYTES
        );
        agb::println!("Tests finished successfully");
        return false;
    };
    if art.n_tiles() != art_pin::N_TILES
        || art.tile_cols != art_pin::TILE_COLS
        || art.tile_rows != art_pin::TILE_ROWS
        || art.checksum != art_pin::CHECKSUM
    {
        agb::println!(
            "[album-probe] FAIL art: blob disagrees with the pin (tiles {} vs {}, cols {} vs {}, rows {} vs {}, checksum {:08x} vs {:08x})",
            art.n_tiles(),
            art_pin::N_TILES,
            art.tile_cols,
            art_pin::TILE_COLS,
            art.tile_rows,
            art_pin::TILE_ROWS,
            art.checksum,
            art_pin::CHECKSUM
        );
        agb::println!("Tests finished successfully");
        return false;
    }
    agb::println!(
        "[album-probe] art blob: {} bytes, {}x{} px, {}x{} tiles = {}, banks={}, checksum={:08x}",
        blob_bytes,
        art.width_px,
        art.height_px,
        art.tile_cols,
        art.tile_rows,
        art.n_tiles(),
        art.n_palette_banks,
        art.checksum
    );
    agb::println!(
        "[album-probe] art statics: bss_ring={} bss_pingpong={} heap_scratch={} rom_blob={} rom_tiles={} rom_palettes={} rom_map={}",
        probe_hw::RING_SAMPLES * 2,
        4 * probe_hw::CHUNK,
        core::mem::size_of::<Scratch>(),
        blob_bytes,
        art.n_tiles() as usize * art1::TILE_BYTES,
        art.n_palette_banks as usize * art1::PALETTE_BANK_BYTES,
        art.n_tiles() as usize * 2
    );

    let mut gfx = gba.graphics.get();
    gfx.set_background_palette_colour(0, 0, COLOR_BACKDROP);
    let mut scratch = Box::new(Scratch::new());
    let mut synth = Synth::new();

    // The timers are handed out ONCE: `all_timers` for the audio chain, dropped
    // before the cycle counter takes timer2/3 (the `probe_hw` timer2/3 ruling).
    let handler = {
        let mut timers = unsafe { gba.timers.all_timers() }; // SAFETY: no agb::sound::mixer::Mixer is instantiated in this ROM, so timer0-3 are unowned.
        probe_hw::bootstrap(&mut timers, &mut synth, &mut scratch)
    };
    let Some(_handler) = handler else {
        agb::println!("[album-probe] FAIL art: bootstrap could not prime a whole chunk");
        agb::println!("Tests finished successfully");
        return false;
    };
    let mut timer_pool = gba.timers.timers();
    let mut counter = Counter::start(&mut timer_pool);

    // Warm the chain up so the live upload happens with periods on the clock and
    // the feeder having run. The ring's prefill (16 chunks) covers this.
    let mut last = counter.read();
    let mut warm = Fold::new();
    for _ in 0..WARMUP_FRAMES {
        let frame = gfx.frame();
        frame.commit();
        let now = counter.read();
        warm.observe(cycles_between(last, now));
        last = now;
    }
    agb::println!(
        "[album-probe] art warmup audio=live frames={} periods={} feeder_sum={} late_max={}",
        WARMUP_FRAMES,
        probe_hw::stats().periods,
        probe_hw::stats().feed_cycles_sum,
        probe_hw::stats().late_max
    );

    // A *discarded* upload first. The live leg used to run first, and it pays
    // agb's VRAM allocator and hash-map growth on its first pass; the disarmed leg
    // then ran warm and looked cheap for a reason that has nothing to do with the
    // audio chain. One throwaway pass makes the two measured legs comparable.
    let discard = upload(&mut gfx, &mut counter, &art);
    drop(discard.background);
    {
        let frame = gfx.frame();
        frame.commit();
    }

    let before = probe_hw::stats();

    // ------------------------------------------------------- the live upload
    let live = upload(&mut gfx, &mut counter, &art);
    {
        let mut frame = gfx.frame();
        live.background.show(&mut frame);
        frame.commit();
    }
    let after = probe_hw::stats();
    report("live", &art, &live);
    agb::println!(
        "[album-probe] art live feeder: audio=live periods={} sum={} max={} late_max={} underflows={}",
        after.periods.wrapping_sub(before.periods),
        after.feed_cycles_sum.wrapping_sub(before.feed_cycles_sum),
        after.feed_cycles_max,
        after.late_max,
        after.underflows.wrapping_sub(before.underflows)
    );
    let live_underflows = after.underflows.wrapping_sub(before.underflows);

    // Free the first background's tiles (a commit runs agb's VRAM gc), then
    // disarm the chain and time the identical upload: the R3 comparison.
    drop(live.background);
    {
        let frame = gfx.frame();
        frame.commit();
    }
    probe_hw::disarm_audio();
    let off = upload(&mut gfx, &mut counter, &art);
    report("disarmed", &art, &off);
    {
        let mut frame = gfx.frame();
        off.background.show(&mut frame);
        frame.commit();
    }

    // The upload's price against one audio period: §10.2's row is "<= 1 period,
    // off the audio path". Printed as the two operands so the reader divides.
    agb::println!(
        "[album-probe] art whole live={} disarmed={} period_cycles={}",
        live.whole,
        off.whole,
        probe_hw::PERIOD_CYCLES
    );

    let pass = live.whole != u32::MAX
        && off.whole != u32::MAX
        && live.per_tile.wrapped == 0
        && off.per_tile.wrapped == 0
        && warm.wrapped == 0
        && live_underflows == 0;
    if pass {
        agb::println!(
            "[album-probe] PASS art tiles={} live={} disarmed={} period={}",
            art.n_tiles(),
            live.whole,
            off.whole,
            probe_hw::PERIOD_CYCLES
        );
    } else {
        agb::println!(
            "[album-probe] FAIL art: a wrapped upload window or an underflow while the image was uploaded"
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
