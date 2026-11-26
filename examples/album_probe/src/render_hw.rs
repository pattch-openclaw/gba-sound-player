//! 2b PR-(2)'s shared ARM-only half: OBJ tile data, the OAM shadow with a
//! VBlank-started DMA3, and the map-register readers the L4 pool leg needs.
//!
//! Two bins use it, and they ask different questions of the same plumbing:
//!
//! - `render_probe` (AP-3b) draws with **backgrounds only** and measures per-cell
//!   redraw, a whole-region repaint, a `set_priority` switch with both scenes
//!   resident, and the live tile-pool allocation. It needs the map readers and the
//!   generated tile banks; it never starts the OAM DMA.
//! - `present_probe` (AP-3c) measures the **presentation path**: arm A is the 2a
//!   shape (three BGs, an event-scoped `commit`), arm B moves the chrome to OBJ
//!   with a double-buffered OAM shadow presented by DMA3 at VBlank, and arm C is B
//!   under a 60 Hz synthetic chrome load.
//!
//! ## What is deliberately *not* agb's
//!
//! §5.7 needs a presentation path that does not block the main loop, and agb
//! 0.25's `GraphicsFrame::commit` always waits for VBlank first
//! (`display/mod.rs:314-337`). `OamFrame::commit` runs *inside* that call, so
//! objects alone cannot remove the wait. Arm B therefore writes OAM through
//! `OBJECT_ATTRIBUTE_MEMORY` (0x0700_0000) itself, from a shadow this module owns,
//! and presents it with a DMA3 transfer started inside the VBlank interrupt. That
//! is the shape §5.7 calls for, not a reimplementation of agb's sprite allocator:
//! the probe places raw OAM entries by hand and copies a hand-built tile bank to
//! the OBJ pool at `0x0601_0000`.
//!
//! The tile pool split this depends on is read out in `ALBUM-PLAYER.md` §5.1: OBJ
//! tile data is a *separate* 32 KiB region at VRAM + 64 KiB, hardware mapping mode
//! 1 is what `agb` selects at boot (`display/object.rs:21-31`), and DISPCNT bit 6
//! (mapping mode) must never be touched by this tree. Nothing here writes that
//! bit.
//!
//! ## Measurement
//!
//! The VBlank handler folds its own entry-to-exit cost into a ledger, the same
//! `sum / max / count` shape `probe_hw::Fold` uses, because a per-frame ISR cost
//! is exactly the addend §5.7's claim is about. Reads use `probe_hw`'s cycle
//! counter protocol (timer2 /1 cascading into timer3, low/high/low) rather than a
//! second instrument.

extern crate alloc;

use agb::external::critical_section::{self, Mutex};
use agb::interrupt::{Interrupt, InterruptHandler, add_interrupt_handler};
use core::cell::RefCell;

use crate::probe_hw::read_cycles_now;

/// Bytes in one 4bpp 8x8 tile.
pub const TILE_BYTES: usize = 32;
/// Glyphs in the probe's hand-authored atlas — the shipped cut's outlined count
/// (§5.1's 94), so the tile term is the same shape the plan prices.
pub const GLYPHS: usize = 94;
/// Two 8x8 tile rows per glyph: an 8x16 text column sprite, or two BG tile rows at
/// the 16 px pitch.
pub const ATLAS_TILES: usize = GLYPHS * 2;
/// Five control icons, ten bar/fill cells, one blank.
pub const ICON_TILES: usize = 16;
/// A 16x16 art box: 256 tiles, the 128x128 term §5.1 audits.
pub const ART_TILES: usize = 256;

/// The OBJ tile-data base (`sprite_allocator/sprite.rs:18-24`, `TILE_SPRITE`).
pub const OBJ_TILE_BASE: usize = 0x0601_0000;
/// OAM attribute memory.
pub const OAM_BASE: usize = 0x0700_0000;
/// The BG tile-data base; a 4bpp tile index addresses `BASE + index * 32`.
pub const BG_TILE_BASE: usize = 0x0600_0000;
const REG_BG0CNT: usize = 0x0400_0008;
const REG_DMA3_SOURCE: usize = 0x0400_00D4;
const REG_DMA3_DEST: usize = 0x0400_00D8;
const REG_DMA3_COUNT: usize = 0x0400_00DC;
const REG_DMA3_CONTROL: usize = 0x0400_00DE;

// SAFETY: all callers pass fixed I/O or VRAM addresses.
unsafe fn io_read_u16(addr: usize) -> u16 {
    unsafe { (addr as *const u16).read_volatile() }
}

// SAFETY: all callers pass fixed I/O or VRAM addresses.
unsafe fn io_write_u16(addr: usize, value: u16) {
    unsafe { (addr as *mut u16).write_volatile(value) }
}

// SAFETY: all callers pass fixed I/O addresses.
unsafe fn io_write_u32(addr: usize, value: u32) {
    unsafe { (addr as *mut u32).write_volatile(value) }
}

// ------------------------------------------------------------ tile banks

/// Fill one 4bpp tile: `f(col, row)` returns a palette index 0-15.
fn put_tile(bytes: &mut [u8], index: usize, f: impl Fn(usize, usize) -> u8) {
    let base = index * TILE_BYTES;
    for row in 0..8 {
        for col in 0..8 {
            let byte = base + row * 4 + col / 2;
            let shift = if col & 1 == 0 { 0 } else { 4 };
            bytes[byte] = (bytes[byte] & !(0x0f << shift)) | ((f(col, row) & 0x0f) << shift);
        }
    }
}

/// Leak a 4-byte-aligned `Vec<u32>` as a byte slice: `TileSet::new`'s two
/// preconditions are 32-bit alignment and a whole number of tiles, and a plain
/// `Vec<u8>` only guarantees the second. Allocation happens at boot, never in a
/// measured window (allocation in an interrupt is agb's own prohibition).
fn leak_tiles(words: usize) -> &'static mut [u8] {
    let words = alloc::vec![0u32; words];
    let leaked: &'static mut [u32] = alloc::boxed::Box::leak(words.into_boxed_slice());
    // SAFETY: `[u32]` is 4-byte aligned and at least as aligned as `[u8]`; the
    // length is exactly `words * 4` bytes, a whole number of 32-byte tiles.
    unsafe { core::slice::from_raw_parts_mut(leaked.as_mut_ptr() as *mut u8, leaked.len() * 4) }
}

/// The 188-tile glyph atlas: tile `2g` is the top half of glyph `g`, `2g+1` the
/// bottom, so an 8x16 sprite (or two BG tile rows) addresses both.
///
/// The shapes are synthetic on purpose. This probe measures *allocation and
/// scheduling*, not legibility: what matters is that every cell names one static
/// `(tileset, index)` key, because that is the key agb's VRAM manager dedupes on
/// (`vram_manager.rs:852-897`) — and therefore the key whose repetition L4 checks
/// against the live map.
pub fn make_atlas() -> &'static [u8] {
    let bytes = leak_tiles(ATLAS_TILES * TILE_BYTES / 4);
    for glyph in 0..GLYPHS {
        put_tile(bytes, glyph * 2, |col, row| {
            if row == 0 || col == 0 {
                1
            } else if (glyph * 3 + row * 5 + col * 7) % 6 == 0 {
                2
            } else if (glyph + row + col) % 11 == 0 {
                3
            } else {
                0
            }
        });
        put_tile(bytes, glyph * 2 + 1, |col, row| {
            if row == 7 || col == 7 {
                1
            } else if (glyph * 5 + row * 3 + col * 11) % 7 == 0 {
                2
            } else {
                0
            }
        });
    }
    bytes
}

/// Sixteen small tiles: five distinct control glyphs (triangle, two bars, `|<`,
/// `>|`, a return arrow) and eleven bar/fill cells. The control glyphs differ in
/// **shape**, not only in hue, so a probe screenshot can satisfy §5.6's rule.
pub fn make_icons() -> &'static [u8] {
    let bytes = leak_tiles(ICON_TILES * TILE_BYTES / 4);
    // 0: play triangle, 1: pause bars, 2: |<, 3: >|, 4: return arrow.
    for icon in 0..5 {
        put_tile(bytes, icon, |col, row| match icon {
            0 => {
                if col >= row / 2 && col <= 6 - row / 2 {
                    1
                } else {
                    0
                }
            }
            1 => {
                if (2..=3).contains(&col) || (5..=6).contains(&col) {
                    1
                } else {
                    0
                }
            }
            2 => {
                if col == 1 || (col >= 2 && row >= 2 && row <= 5 && 1 + (row - 2) / 2 == col - 2) {
                    1
                } else {
                    0
                }
            }
            3 => {
                if col == 6 || (col <= 5 && row >= 2 && row <= 5 && 6 - (row - 2) / 2 == col + 2) {
                    1
                } else {
                    0
                }
            }
            _ => {
                if row == 2 || (col == 6 && row <= 2) {
                    1
                } else {
                    0
                }
            }
        });
    }
    // 5: solid fill (used for backdrops and the page fill); 6..: ten bar levels,
    // each a partial fill of the cell in the second palette index.
    put_tile(bytes, 5, |_, _| 1);
    for level in 0..10 {
        put_tile(bytes, 6 + level, |col, _| if col <= level { 2 } else { 1 });
    }
    bytes
}

/// A 256-tile art box, the 128x128 term §5.1 sums against the pool. Every tile
/// index is its own `(tileset, index)` key, so the allocation is exactly 256 slots
/// whatever the pixels do — which is the point: this term is arithmetic, and L4
/// checks it against the live allocation.
pub fn make_art() -> &'static [u8] {
    let bytes = leak_tiles(ART_TILES * TILE_BYTES / 4);
    for tile in 0..ART_TILES {
        put_tile(bytes, tile, |col, row| {
            (((tile * 7 + row * 13 + col * 5) % 15) + 1) as u8
        });
    }
    bytes
}

/// Copy a 4bpp tile bank to the OBJ pool at `OBJ_TILE_BASE`, one 32-bit word at a
/// time. The OBJ pool is disjoint from both BG pools (§5.1), so this cannot
/// disturb a background's allocation.
pub fn copy_to_obj(tiles: &[u8]) {
    let words = tiles.len() / 4;
    for i in 0..words {
        let word = u32::from_le_bytes([
            tiles[i * 4],
            tiles[i * 4 + 1],
            tiles[i * 4 + 2],
            tiles[i * 4 + 3],
        ]);
        // SAFETY: `i < tiles.len()/4`, and the OBJ pool is 1024 tiles = 32 KiB —
        // callers pass the 188 + 16 tiles §5.7's chrome plan needs.
        unsafe { io_write_u32(OBJ_TILE_BASE + i * 4, word) };
    }
}

// ------------------------------------------------------------- OAM shadow

/// One OAM entry, packed as the two 32-bit words the shadow stores.
///
/// attr0: y (0-7), affine (8), **disable (9)**, mode (10-11), mosaic (12),
/// 256-colour (13), shape (14-15). attr1: x (0-8), flips/affine (9-13), size
/// (14-15). attr2: tile index (0-9), priority (10-11), palette bank (12-15).
#[must_use]
pub fn entry(x: i32, y: i32, tile: u16, tall: bool, palette: u8, priority: u8) -> [u32; 2] {
    let shape: u16 = if tall { 2 } else { 0 }; // vertical rectangle 8x16 / square 8x8
    let attr0 = (y as u16 & 0xff) | (shape << 14);
    let attr1 = x as u16 & 0x01ff;
    let attr2 =
        (tile & 0x03ff) | (u16::from(priority & 3) << 10) | (u16::from(palette & 0x0f) << 12);
    [
        u32::from(attr0) | (u32::from(attr1) << 16),
        u32::from(attr2),
    ]
}

/// A hidden entry, used to initialise the shadow and to take the chrome down
/// before a verdict takeover.
#[must_use]
pub fn hidden() -> [u32; 2] {
    [1 << 9, 0]
}

struct Oam {
    /// The authoritative OAM image. 128 entries x 8 B = 1 KiB.
    shadow: [u32; 256],
    /// Slots changed since the last publish — the sparse presentation path.
    dirty: [u16; 128],
    dirty_n: usize,
    /// Present the whole shadow (arm C, and any full rebuild).
    full: bool,
    /// The main loop has finished writing this frame.
    pending: bool,
}

static OAM: Mutex<RefCell<Oam>> = Mutex::new(RefCell::new(Oam {
    shadow: [1 << 9; 256],
    dirty: [0; 128],
    dirty_n: 0,
    full: false,
    pending: false,
}));

/// The VBlank handler's ledger: how many frames it presented, how many DMA
/// transfers that took, and what the handler cost per entry.
#[derive(Clone, Copy)]
pub struct VBlankStats {
    /// VBlanks that presented a frame.
    pub frames: u32,
    /// DMA3 transfers started (1 for a full 1 KiB present, one per dirty entry
    /// for a sparse one).
    pub dmas: u32,
    /// Entry-to-exit cost, summed and worst.
    pub cost_sum: u64,
    pub cost_max: u32,
    /// Windows that wrapped the composite — a failed instrument.
    pub wrapped: u32,
}

impl VBlankStats {
    const fn new() -> Self {
        Self {
            frames: 0,
            dmas: 0,
            cost_sum: 0,
            cost_max: 0,
            wrapped: 0,
        }
    }
}

static VBLANK: Mutex<RefCell<VBlankStats>> = Mutex::new(RefCell::new(VBlankStats::new()));

/// Write one OAM entry into the shadow and mark it dirty. The two words are
/// written inside one critical section so a VBlank cannot sample half an entry;
/// the section is two `u32` stores (tens of cycles), far inside the 16-sample
/// re-arm grace the audio ISR needs.
pub fn oam_write(slot: usize, words: [u32; 2]) {
    critical_section::with(|cs| {
        let mut oam = OAM.borrow_ref_mut(cs);
        oam.shadow[slot * 2] = words[0];
        oam.shadow[slot * 2 + 1] = words[1];
        let dirty_n = oam.dirty_n;
        if !oam.full && dirty_n < oam.dirty.len() {
            oam.dirty[dirty_n] = slot as u16;
            oam.dirty_n = dirty_n + 1;
        }
    });
}

/// Present the whole shadow next VBlank (used by a full rebuild: arm C, and the
/// chrome's initial paint).
pub fn oam_full() {
    critical_section::with(|cs| {
        let mut oam = OAM.borrow_ref_mut(cs);
        oam.full = true;
        oam.dirty_n = 0;
    });
}

/// Hand this frame's writes to the VBlank handler.
pub fn oam_publish() {
    critical_section::with(|cs| OAM.borrow_ref_mut(cs).pending = true);
}

/// Start a DMA3 transfer and wait for the enable bit to clear.
///
/// The wait is the point: the shadow must not change while the DMA reads it, and
/// the CPU cost of the transfer is what §5.7 says a VBlank-presented chrome pays
/// instead of the main loop's halt. The bound makes a stuck emulator fail a leg
/// rather than hang the run.
///
/// # Safety
/// `src` must be 4-byte aligned and point at `words * 4` readable bytes that stay
/// alive for the transfer.
unsafe fn dma3_words(src: *const u32, dest: usize, words: usize) {
    unsafe {
        io_write_u16(REG_DMA3_CONTROL, 0);
        io_write_u32(REG_DMA3_SOURCE, src as u32);
        io_write_u32(REG_DMA3_DEST, dest as u32);
        io_write_u16(REG_DMA3_COUNT, words as u16);
        // enable | 32-bit transfers; source and destination both increment;
        // timing 0 = immediate, which starts the transfer as the word is written.
        io_write_u16(REG_DMA3_CONTROL, 0x8000 | (1 << 10));
        for _ in 0..100_000 {
            if io_read_u16(REG_DMA3_CONTROL) & 0x8000 == 0 {
                break;
            }
        }
    }
}

/// The VBlank handler. Borrows the shadow for the whole transfer, which is what
/// keeps the main loop from writing it mid-copy: the main loop's writes take the
/// same lock inside a critical section, and the handler runs with interrupts
/// masked.
fn vblank_isr() {
    let entered = read_cycles_now();
    let (frames, dmas) = critical_section::with(|cs| {
        let mut oam = OAM.borrow_ref_mut(cs);
        if !oam.pending {
            return (0u32, 0u32);
        }
        let mut dmas = 0u32;
        if oam.full {
            // SAFETY: the shadow is a 4-byte-aligned `[u32; 256]` static, 1 KiB;
            // the borrow keeps the main loop out for the transfer; the destination
            // is OAM's attribute memory.
            unsafe { dma3_words(core::ptr::addr_of!(oam.shadow) as *const u32, OAM_BASE, 256) };
            oam.full = false;
            dmas = 1;
        } else {
            let n = oam.dirty_n;
            for i in 0..n {
                let slot = oam.dirty[i] as usize;
                let src = core::ptr::addr_of!(oam.shadow[slot * 2]);
                // SAFETY: `slot < 128`, so `src` is inside the shadow and 8-byte
                // aligned; the entry is 8 bytes at OAM + slot*8.
                unsafe { dma3_words(src, OAM_BASE + slot * 8, 2) };
                dmas += 1;
            }
            oam.dirty_n = 0;
        }
        oam.pending = false;
        (1u32, dmas)
    });

    if frames == 0 {
        return;
    }
    let cost = crate::probe_hw::timer::cycles_between(entered, read_cycles_now());
    critical_section::with(|cs| {
        let mut stats = VBLANK.borrow_ref_mut(cs);
        stats.frames = stats.frames.wrapping_add(frames);
        stats.dmas = stats.dmas.wrapping_add(dmas);
        if cost == u32::MAX {
            stats.wrapped = stats.wrapped.wrapping_add(1);
        } else {
            stats.cost_sum = stats.cost_sum.wrapping_add(u64::from(cost));
            if cost > stats.cost_max {
                stats.cost_max = cost;
            }
        }
    });
}

/// Register the VBlank handler. Hold the returned handle for the arm's lifetime;
/// dropping it stops presentation.
#[must_use]
pub fn start_vblank_oam() -> InterruptHandler {
    // SAFETY: the handler allocates nothing and logs nothing — it writes DMA3
    // registers and two statics. agb's interrupt-handler contract.
    unsafe { add_interrupt_handler(Interrupt::VBlank, |_| vblank_isr()) }
}

/// Stop presenting and take every OAM entry down, so a verdict colour or a BG
/// takeover is not covered by stale sprites.
pub fn stop_vblank_oam() {
    // SAFETY: fixed DMA3 register; disabling the channel is valid at any time.
    unsafe {
        io_write_u16(REG_DMA3_CONTROL, 0);
        for slot in 0..128 {
            io_write_u16(OAM_BASE + slot * 8, 1 << 9);
            io_write_u16(OAM_BASE + slot * 8 + 2, 0);
            io_write_u16(OAM_BASE + slot * 8 + 4, 0);
            io_write_u16(OAM_BASE + slot * 8 + 6, 0);
        }
    }
}

/// A copy of the VBlank ledger.
#[must_use]
pub fn vblank_stats() -> VBlankStats {
    critical_section::with(|cs| *VBLANK.borrow_ref(cs))
}

/// Zero the per-arm maximum, leaving the cumulative frame/DMA counts and the cost
/// sum alone. The probe calls this at an arm's start so `max` is that arm's worst
/// entry rather than the worst ever seen — the setup's 1 KiB paint is not arm B's.
pub fn reset_vblank_max() {
    critical_section::with(|cs| VBLANK.borrow_ref_mut(cs).cost_max = 0);
}

// ---------------------------------------------------------------- maps

/// `BGxCNT` for background `bg` (0-3).
#[must_use]
pub fn bg_cnt(bg: usize) -> u16 {
    // SAFETY: BGxCNT lives at 0x0400_0008 + 2*bg, an even I/O address.
    unsafe { io_read_u16(REG_BG0CNT + bg * 2) }
}

/// Screen base block index from `BGxCNT` bits 8-12.
#[must_use]
pub fn screen_base_index(bg: usize) -> usize {
    ((bg_cnt(bg) >> 8) & 0x1f) as usize
}

/// One map entry: `(x, y)` in the background's 32x32 map. Bits 0-9 are the tile
/// index, 10 hflip, 11 vflip, 12-15 the palette bank.
#[must_use]
pub fn map_entry(bg: usize, x: usize, y: usize) -> u16 {
    // `BGxCNT`'s screen-base field is a *block number measured from VRAM base*,
    // not an index into the map pool's start: block 16 is the pool's first map at
    // VRAM + 32 KiB. Adding the pool base here would add 32 KiB twice, which reads
    // the 16 KiB gap agb never allocates — the first run of this probe did exactly
    // that and every cell came back as tile index 0.
    let base = BG_TILE_BASE + screen_base_index(bg) * 0x800;
    // SAFETY: the address is inside one of the eight 2 KiB screenblocks and is
    // even; (x, y) is bounded by the caller to the 32x32 map.
    unsafe { io_read_u16(base + (y * 32 + x) * 2) }
}

/// The first word of the 4bpp tile at `index` in the BG tile pool, so a leg can
/// say whether a map entry still points at live tile data.
#[must_use]
pub fn bg_tile_word(index: usize) -> u32 {
    // SAFETY: indices come from live map entries (10-bit), and 1024 tiles * 32 B
    // is the BG tile-data region.
    unsafe { ((BG_TILE_BASE + index * TILE_BYTES) as *const u32).read_volatile() }
}

/// `BGxCNT`'s priority field (bits 0-1). Used to prove a switch landed at commit.
#[must_use]
pub fn bg_priority(bg: usize) -> u8 {
    (bg_cnt(bg) & 3) as u8
}
