//! The one surface (§5.2): the band stack, the composite, element-addressed redraw, and the scene
//! slot a `blank` region gets.
//!
//! **One surface, one flow, and the rectangle is the enforcement** (§5.0). A scene is one function
//! receiving a `Region` and nothing else — it cannot draw over the chrome or repaint the title band,
//! because it was never given those cells. This module owns the cells; a scene owns its rectangle.
//! That is the whole abstraction, and it is why there is no runtime geometry here: every coordinate
//! comes from the generated `layout.rs`.
//!
//! **Nothing in 2a repaints on a timer** (§5.2's cadence rule, added 2026-11-20). The cadences this
//! module implements are exactly the declared ones: the title on a track change, the clock when a
//! displayed second changes, the bar when a cell changes, the control icons never (authored art in
//! fixed slots). A frame that changed nothing writes nothing and commits nothing — not an
//! optimisation but §5.1's rule 2 ("partial map rewrites, changed cells only") read literally, and
//! the reason R1b's measured failure (a full-row 1 Hz redraw reaching a DMA's grace window, 104 late
//! of 16,553 periods) cannot recur here.
//!
//! ## Two halves, and the reason for the split
//!
//! The *decision* half is platform-free: which cells should hold which tiles, and which of those
//! differ from what is on the map. It produces a list of [`Write`]s, so the host gate can assert a
//! track change's cost in cells rather than in prose. The *applying* half is
//! `#[cfg(target_arch = "arm")]`: two agb `RegularBackground`s and one commit. This is `keys.rs`'s
//! arrangement and `text.rs`'s, for the same reason: the checks that matter are on the host.
//!
//! ## One glyph per cell, and the bound that makes it sound
//!
//! Text is drawn from §5.5a's static atlas at **one glyph per 8 px cell** — `tile_for(c, row)` at
//! column `col + i`. That is the addressing the pool audit assumes (§5.1's "one allocation per
//! character cell"), the addressing §10.2's write costs are priced at ("20 columns × 2 tile rows =
//! ≤40 tiles"), and the reason a repeated letter is one tile. It is *not* proportional placement,
//! which would need per-pixel compositing into dynamic tiles and would give up both the dedupe and
//! the static atlas.
//!
//! That choice has a precondition worth stating as a check rather than as a belief: **it is sound
//! only while every advance in the face is ≤ one cell.** With advances ≤ 8 px the advance sum of a
//! line never exceeds its cell span, so cutting a line at a cell boundary is the stricter of §5.5's
//! two bounds (a drawn line must satisfy both `measure` and `tilemap_px`). §5.5a's generalization
//! note records the case where that stops holding — a face with generous sidebearings, where the
//! atlas still builds and `measure` exceeds `tilemap_px` — and [`ADVANCE_FITS_A_CELL`] turns that
//! from a silent mis-draw into a build failure. When a future face trips it the fix is that note's
//! refusal, not a second renderer here.
//!
//! ## What 2a draws, and what it deliberately does not
//!
//! Chrome band (clock, key-labels word, five authored control icons), title band (the elided title
//! line and §6's progress bar), and the scene region filled flat by the `blank` scene. **No album
//! art, no track list, no title card, no viz** — those are 2b's and 4's (§11's 2a row), and the scene
//! registry here has exactly one entry.

use crate::chrome;
use crate::geometry;
use crate::glyphs;
use crate::layout;

/// The most cells any declared text element spans (the list window: 30 columns × 2 tile rows).
pub const MAX_CELLS: usize = 60;

/// The most characters a drawn line can hold: the widest declared text element, in cells.
pub const MAX_LINE: usize = layout::SCREEN_COLS as usize;

/// The most cells one batch of writes carries: the scene region, the largest single element a 2a
/// variant declares. `Writes::push` counts an overflow instead of dropping it silently.
pub const MAX_WRITES: usize = layout::REGION.cols as usize * layout::REGION.rows as usize;

/// The most 8 px columns a declared bar can span. B's is 10; the bound is generous and asserted.
pub const MAX_BAR_COLS: usize = 16;

/// One cell's new content: where it goes, which tile, and which palette bank the map entry names.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Write {
    pub x: u8,
    pub y: u8,
    pub tile: u16,
    pub bank: u8,
    /// Which tileset the index refers to. **Not optional, and not inferred**: two tilesets share one
    /// 10-bit index space (§5.5a's atlas and §5.3's authored chrome), so a tile number alone cannot
    /// say what it draws. A host renderer that guessed by index range would draw an icon where a
    /// glyph belongs whenever the two happen to collide, and the ROM would be right while the
    /// picture was wrong — so the plan carries the fact the device needs anyway.
    pub source: Source,
}

/// The two tilesets a 2a surface draws from.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Source {
    /// §5.3's authored chrome and §6's bar patterns (`chrome::TILES`).
    Chrome,
    /// §5.5a's glyph atlas (`glyphs::GLYPHS`).
    Glyph,
}

/// A batch of [`Write`]s: what changed, in map coordinates. Fixed capacity because the ROM has no
/// allocator on this path, and bounded by the largest declared element — a generated fact.
pub struct Writes {
    buf: [Write; MAX_WRITES],
    len: usize,
    dropped: usize,
}

impl Writes {
    #[must_use]
    pub const fn new() -> Self {
        const BLANK: Write = Write {
            x: 0,
            y: 0,
            tile: 0xFFFF,
            bank: 0,
            source: Source::Chrome,
        };
        Self {
            buf: [BLANK; MAX_WRITES],
            len: 0,
            dropped: 0,
        }
    }

    /// Forget the batch. Capacity is fixed, so this is the only reuse path.
    pub fn clear(&mut self) {
        self.len = 0;
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.len
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Writes that did not fit. Zero for every geometry this tree declares; a non-zero count is a
    /// spec that outgrew the buffer, and the host gate refuses it by name rather than letting a
    /// partial draw look like a complete one.
    #[must_use]
    pub fn dropped(&self) -> usize {
        self.dropped
    }

    #[must_use]
    pub fn as_slice(&self) -> &[Write] {
        &self.buf[..self.len]
    }

    pub fn push(&mut self, w: Write) {
        if self.len < MAX_WRITES {
            self.buf[self.len] = w;
            self.len += 1;
        } else {
            self.dropped += 1;
        }
    }

    /// Fill a rectangle of cells with one tile. Used for the band fills and the `blank` scene — the
    /// two places a rectangle, rather than a string, decides the cells.
    pub fn fill(&mut self, x0: u32, y0: u32, cols: u32, rows: u32, tile: u16, bank: u8) {
        let mut row = 0;
        while row < rows {
            let mut col = 0;
            while col < cols {
                self.push(Write {
                    x: (x0 + col) as u8,
                    y: (y0 + row) as u8,
                    tile,
                    bank,
                    source: Source::Chrome,
                });
                col += 1;
            }
            row += 1;
        }
    }
}

impl Default for Writes {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------------------------
// The line: §5.5's one-line elision at the bound a cell-addressed renderer actually has
// ---------------------------------------------------------------------------------------------

/// One drawn line: the characters to draw, and whether a marker follows them.
///
/// A fixed buffer rather than a `&str` because the cut owns its bytes on a ROM with no allocator;
/// [`as_str`](Self::as_str) is the only view a caller needs.
pub struct Line {
    buf: [u8; MAX_LINE],
    len: usize,
    cut: bool,
}

impl Line {
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            buf: [0; MAX_LINE],
            len: 0,
            cut: false,
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        // Only ASCII is ever written: `line_for` stops at the first non-ASCII byte rather than
        // slicing a multi-byte character in half.
        core::str::from_utf8(&self.buf[..self.len]).unwrap_or("")
    }

    /// Whether the marker was added, i.e. whether the input was longer than the box holds.
    #[must_use]
    pub fn cut(&self) -> bool {
        self.cut
    }

    #[must_use]
    pub fn chars(&self) -> usize {
        self.len
    }
}

/// The marker §5.5a chose: three ASCII periods. Re-exported rather than re-typed, so the drawn
/// marker and the measured one are one string.
pub const ELLIPSIS: &str = glyphs::ELLIPSIS;

/// Cut `text` to `cols` cells, adding §5.5's marker when it does not fit.
///
/// **This is the drawing rule, and it is a cell rule.** A cell-addressed renderer's real bound is
/// "characters ≤ cells", which for a face whose advances are ≤ 8 px is *stricter* than §5.5's
/// advance-sum bound — see the module docs and [`ADVANCE_FITS_A_CELL`]. The bake's verdict
/// (`glyphs::elide`, advance-based) decides whether an album's wording is a note, and it is never
/// stricter than this; `tests/album_surface_host.rs` pins that relationship on the shipped face, so
/// the two rules cannot drift into disagreeing about a drawn line.
///
/// A box of fewer cells than the marker holds nothing, which is the state §9.0's `title-fit` refuses
/// at bake time. A shipped variant can never ask for it: the atlas module's suite asserts no declared
/// text box is narrower than the marker.
#[must_use]
pub fn line_for(text: &str, cols: usize) -> Line {
    let mut line = Line::empty();
    let mut count = 0usize;
    let mut whole = true;
    for ch in text.chars() {
        if !ch.is_ascii() {
            // The fold is upstream (§5.5); a multi-byte character in a byte buffer would draw half
            // of itself, which is worse than drawing none of it.
            whole = false;
            break;
        }
        if count >= cols || count >= MAX_LINE {
            whole = false;
            break;
        }
        line.buf[count] = ch as u8;
        count += 1;
    }
    if whole {
        line.len = count;
        return line;
    }
    if cols < ELLIPSIS.len() {
        return Line::empty();
    }
    let keep = cols.min(MAX_LINE) - ELLIPSIS.len();
    line.buf[keep..keep + ELLIPSIS.len()].copy_from_slice(ELLIPSIS.as_bytes());
    line.len = keep + ELLIPSIS.len();
    line.cut = true;
    line
}

// ---------------------------------------------------------------------------------------------
// Element lookup: by identity string, at compile time
// ---------------------------------------------------------------------------------------------

/// Byte-wise string comparison, for `const` contexts (`PartialEq` is not const on this toolchain;
/// the shape `glyphs.rs` and `chrome.rs` carry for the same reason).
const fn streq(a: &str, b: &str) -> bool {
    let a = a.as_bytes();
    let b = b.as_bytes();
    if a.len() != b.len() {
        return false;
    }
    let mut i = 0;
    while i < a.len() {
        if a[i] != b[i] {
            return false;
        }
        i += 1;
    }
    true
}

/// The index of the declared element with id `id`, at compile time.
///
/// A missing id is a compile error naming the id, which is the point: §5.3 keys things by identity,
/// so a spec that renames `title_text` should fail the build rather than draw the title into whatever
/// element happens to sit at index 3 (the drift D9 is about).
const fn element_index(id: &str) -> usize {
    let mut i = 0;
    while i < layout::ELEMENTS.len() {
        if streq(layout::ELEMENTS[i].id, id) {
            return i;
        }
        i += 1;
    }
    panic!("this variant declares no element with this id");
}

/// The chrome band's clock: `elapsed / total` (§5.5, A6).
pub const CLOCK: usize = element_index("clock");
/// The word beside the control icons: §5.6's "a control's drawn state plus a word".
pub const KEYLABELS: usize = element_index("keylabels");
/// The title line in the title band: one line, elided, never wrapped (§5.5).
pub const TITLE: usize = element_index("title_text");
/// §6's progress bar, in the title band's lower row.
pub const BAR: usize = element_index("bar");
/// The authored control art's element; the five slots sit inside it (§5.3).
pub const KEYART: usize = element_index("keyart");

const _: () = assert!(
    layout::ELEMENTS[CLOCK].w / chrome::TILE_PX == 12,
    "the clock's box is not 12 cells: the `M:SS/M:SS` line assumes it"
);
const _: () = assert!(
    layout::ELEMENTS[BAR].w % chrome::TILE_PX == 0
        && (layout::ELEMENTS[BAR].w / chrome::TILE_PX) as usize <= MAX_BAR_COLS,
    "the bar's box is not a whole number of cells, or is wider than the declared bound"
);
/// Whether every declared control slot names the element this module draws icons in.
const fn slots_share_keyart() -> bool {
    let mut i = 0;
    while i < layout::CONTROL_SLOTS.len() {
        if !streq(layout::CONTROL_SLOTS[i].element, "keyart") {
            return false;
        }
        i += 1;
    }
    true
}
const _: () = assert!(
    slots_share_keyart(),
    "a control slot does not sit in the element this module draws icons in"
);

// ---------------------------------------------------------------------------------------------
// The precondition of one-glyph-per-cell, as a compile-time fact
// ---------------------------------------------------------------------------------------------

/// Whether the declared face's widest advance fits in one cell — the premise the module docs state.
///
/// `max_advance_permyriad` is ten-thousandths of a pixel, generated from the same `hmtx` read the
/// atlas packs from, so this is the face's own number rather than a copy of one.
const ADVANCE_FITS_A_CELL: bool = {
    let face = &layout::FACES[0];
    face.max_advance_permyriad <= chrome::TILE_PX * 10_000
};
const _: () = assert!(
    ADVANCE_FITS_A_CELL,
    "the declared face advances more than one 8 px cell per glyph: a one-glyph-per-cell drawing would overlap glyphs and clip the line, so the atlas's cell model is not this face's model"
);

/// The fill tile and palette bank a band's background uses, by the band's declared id.
const fn band_fill(band: &str) -> (u16, u8) {
    if streq(band, "chrome") {
        (chrome::FILL_CHROME, chrome::BANK_CHROME)
    } else if streq(band, "title") {
        (chrome::FILL_TITLE, chrome::BANK_CHROME)
    } else {
        (chrome::FILL_SCENE, chrome::BANK_TEXT)
    }
}

/// The band's first tile row on screen, from the generated band table.
const fn band_row(band: &str) -> u32 {
    let mut i = 0;
    while i < layout::BANDS.len() {
        if streq(layout::BANDS[i].id, band) {
            return layout::BANDS[i].screen_row;
        }
        i += 1;
    }
    panic!("the variant declares no band with this id");
}

/// The scene region's first tile row: 0 for every variant this tree declares, derived rather than
/// assumed.
pub const REGION_ROW: u32 = band_row(layout::REGION.band);

const _: () = assert!(
    REGION_ROW == 0 && layout::REGION.rows + 4 == layout::SCREEN_ROWS,
    "the region is not the top of the screen with two bands beneath it"
);

// ---------------------------------------------------------------------------------------------
// What a frame should say
// ---------------------------------------------------------------------------------------------

/// Everything a frame's drawing depends on, gathered from the run loop.
pub struct View<'a> {
    /// The current track's title, folded (§5.5). Drawn one line, elided, never wrapped.
    pub title: &'a str,
    /// Samples played in this track's visit.
    pub elapsed_samples: u32,
    /// This track's length in samples.
    pub total_samples: u32,
    /// §6's progress cell, `0..=BAR_CELLS`, from `position::paint_position`.
    pub bar_cell: u32,
    /// The transport state as a word (`.5.6`: shape *and* a word; until §5.3's `PAUSED` badge lands
    /// in 2b, the word is the whole of the state's signal).
    pub state_word: &'a str,
}

/// The cells one text element currently holds, so a frame writes only the cells that changed.
struct Cells {
    cols: usize,
    rows: usize,
    tiles: [u16; MAX_CELLS],
    banks: [u8; MAX_CELLS],
    /// Which tileset each cell names. Compared as part of the diff: a glyph and a band fill can share
    /// an index, and a redraw that changed only the tileset would otherwise be invisible here.
    sources: [Source; MAX_CELLS],
    seeded: bool,
}

impl Cells {
    const fn new() -> Self {
        Self {
            cols: 0,
            rows: 0,
            tiles: [0xFFFF; MAX_CELLS],
            banks: [0; MAX_CELLS],
            sources: [Source::Chrome; MAX_CELLS],
            seeded: false,
        }
    }

    /// Point this element at a declared box and pre-fill it with the band's own tile, which is what
    /// the band fill already wrote there — so the first frame writes only the glyphs.
    fn seed(&mut self, cols: usize, rows: usize, fill: u16, bank: u8) {
        self.cols = cols;
        self.rows = rows;
        for i in 0..cols * rows {
            self.tiles[i] = fill;
            self.banks[i] = bank;
            self.sources[i] = Source::Chrome;
        }
        self.seeded = true;
    }
}

/// The surface's drawing state: what is on the map, and what the clock has shown.
pub struct Surface {
    clock: Cells,
    labels: Cells,
    title: Cells,
    bar: [u16; MAX_BAR_COLS],
    bar_seeded: bool,
    icons: [u16; chrome::ICON_TILES],
    icons_seeded: bool,
    /// The visit's last sample count, to notice a new visit (a skip restarts the clock at 0).
    visit: u32,
    /// Displayed elapsed seconds, advanced by whole seconds rather than divided per frame.
    seconds: u32,
    /// The sample count at which `seconds` next changes.
    next_second_at: u32,
    /// The track length the total was computed from, so a track change re-derives it.
    total_samples: u32,
    total_seconds: u32,
}

impl Surface {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            clock: Cells::new(),
            labels: Cells::new(),
            title: Cells::new(),
            bar: [0xFFFF; MAX_BAR_COLS],
            bar_seeded: false,
            icons: [0xFFFF; chrome::ICON_TILES],
            icons_seeded: false,
            visit: 0,
            seconds: 0,
            next_second_at: 0,
            total_samples: 0,
            total_seconds: 0,
        }
    }

    /// The static layer for the *surface* background: the always-shown bands' fills and §5.3's five
    /// control icons. Written once at boot.
    ///
    /// The scene region's backing is the other background's static layer (see the ARM half), because
    /// §5.2's switch mechanism needs the region to be a layer of its own. The bands tile the screen
    /// exactly (§5.0's first refusal, asserted in `layout.rs`), so `chrome::FILL_PAGE` — the declared
    /// fourth fill — is the one 2a never writes: it is what a variant with a gap between its bands
    /// would draw, and the declared count is asserted either way in `chrome.rs`.
    pub fn paint_static(&mut self, writes: &mut Writes) {
        writes.clear();
        let mut i = 0;
        while i < layout::BANDS.len() {
            let band = &layout::BANDS[i];
            // The region is the scene background's, not this one's.
            if band.always {
                let (tile, bank) = band_fill(band.id);
                writes.fill(
                    0,
                    band.screen_row,
                    layout::SCREEN_COLS,
                    band.rows,
                    tile,
                    bank,
                );
            }
            i += 1;
        }

        // §5.3's control art, in the slots the variant declares: positions from the generated
        // table, drawings from `chrome.rs`, neither transcribed into the other.
        let mut slot = 0;
        while slot < layout::CONTROL_SLOTS.len() {
            let s = &layout::CONTROL_SLOTS[slot];
            if let Some(tile) = chrome::icon_for_slot(s.key) {
                writes.push(Write {
                    x: (s.px_x / chrome::TILE_PX) as u8,
                    y: (s.px_y / chrome::TILE_PX) as u8,
                    tile,
                    bank: chrome::BANK_CHROME,
                    source: Source::Chrome,
                });
                self.icons[slot] = tile;
            }
            slot += 1;
        }
        self.icons_seeded = true;
    }

    /// The scene's own static layer: §5.2's `blank` row, "a flat fill of the region at a declared
    /// palette entry". One rectangle, nothing else — which is why 2a needs no asset and no art tool.
    pub fn paint_blank(writes: &mut Writes) {
        writes.clear();
        writes.fill(
            0,
            REGION_ROW,
            layout::REGION.cols,
            layout::REGION.rows,
            chrome::FILL_SCENE,
            chrome::BANK_TEXT,
        );
    }

    /// Bring the map up to date with `view`, appending only the cells that changed.
    ///
    /// Returns whether anything was written. A frame with no change writes nothing and the caller
    /// must not commit — §5.1's rule 2 and §5.2's cadence rule in one return value.
    pub fn update(&mut self, view: &View<'_>, writes: &mut Writes) -> bool {
        writes.clear();
        self.advance_clock(view);
        self.draw_clock(view, writes);
        draw_text(&mut self.labels, KEYLABELS, view.state_word, writes);
        draw_text(&mut self.title, TITLE, view.title, writes);
        self.draw_bar(view, writes);
        !writes.is_empty()
    }

    /// Advance the displayed second by whole seconds, never by a divide per frame.
    ///
    /// §5.2's cadence rule for the clock is "at most when a displayed digit changes", and this is
    /// that sentence as arithmetic: a compare per frame, a counter bump per second. It also makes the
    /// new-visit case exact — a skip restarts the position at 0, which a subtraction of samples would
    /// show as going backwards, and this shows as a clock that starts again.
    fn advance_clock(&mut self, view: &View<'_>) {
        let rate = geometry::RATE_HZ;
        if view.elapsed_samples < self.visit {
            self.seconds = 0;
            self.next_second_at = 0;
        }
        self.visit = view.elapsed_samples;
        if self.next_second_at == 0 {
            self.next_second_at = rate;
        }
        while view.elapsed_samples >= self.next_second_at {
            self.seconds = self.seconds.saturating_add(1);
            self.next_second_at = self.next_second_at.saturating_add(rate);
        }
        if view.total_samples != self.total_samples {
            // Once per track change, not per frame: the divide is on the visit's length, which is the
            // same reason §6's bar arithmetic avoids one.
            self.total_samples = view.total_samples;
            self.total_seconds = if rate == 0 {
                0
            } else {
                view.total_samples / rate
            };
        }
    }

    fn draw_clock(&mut self, _view: &View<'_>, writes: &mut Writes) {
        let mut buf = [0u8; 16];
        let mut n = fmt_mmss(self.seconds, &mut buf);
        buf[n] = b'/';
        n += 1;
        n += fmt_mmss(self.total_seconds, &mut buf[n..]);
        let text = core::str::from_utf8(&buf[..n]).unwrap_or("");
        draw_text(&mut self.clock, CLOCK, text, writes);
    }

    /// §6's bar: one tile per 8 px column, of which only the fill offset moves.
    fn draw_bar(&mut self, view: &View<'_>, writes: &mut Writes) {
        let e = &layout::ELEMENTS[BAR];
        let cols = (e.w / chrome::TILE_PX) as usize;
        if !self.bar_seeded {
            for i in 0..MAX_BAR_COLS {
                self.bar[i] = u16::MAX;
            }
            self.bar_seeded = true;
        }
        let base_x = e.x / chrome::TILE_PX;
        let base_y = e.y / chrome::TILE_PX;
        let cell_px = layout::BAR_CELL_PX;
        let mut col = 0;
        while col < cols {
            let column_start = (col as u32) * chrome::TILE_PX;
            let filled = if view.bar_cell.saturating_mul(cell_px) <= column_start {
                0
            } else {
                let inside = view.bar_cell.saturating_mul(cell_px) - column_start;
                if inside > chrome::TILE_PX {
                    chrome::TILE_PX
                } else {
                    inside
                }
            };
            let tile = chrome::bar_tile(filled);
            if self.bar[col] != tile {
                writes.push(Write {
                    x: (base_x + col as u32) as u8,
                    y: base_y as u8,
                    tile,
                    bank: chrome::BANK_CHROME,
                    source: Source::Chrome,
                });
                self.bar[col] = tile;
            }
            col += 1;
        }
    }

    /// The five control icons, which never change after boot: written by [`paint_static`], exposed so
    /// a future state-carrying icon (2b's `PAUSED` badge) has one place to change.
    #[must_use]
    pub fn icons(&self) -> &[u16; chrome::ICON_TILES] {
        &self.icons
    }

    /// Whether the static layer has been written.
    #[must_use]
    pub fn static_painted(&self) -> bool {
        self.icons_seeded
    }

    /// The displayed elapsed second, exposed for the boot line: a surface that says what it thinks
    /// the clock reads is testable against the log.
    #[must_use]
    pub fn shown_seconds(&self) -> u32 {
        self.seconds
    }
}

impl Default for Surface {
    fn default() -> Self {
        Self::new()
    }
}

/// Lay one line into an element's cells, writing only the cells that differ.
///
/// A free function rather than a method, because it borrows one field of [`Surface`] mutably while
/// the caller still needs the rest: a method would need `&mut self` and `&mut self.clock` at once.
fn draw_text(cells: &mut Cells, element: usize, text: &str, writes: &mut Writes) {
    let e = &layout::ELEMENTS[element];
    let cols = (e.w / chrome::TILE_PX) as usize;
    let rows = (e.h / chrome::TILE_PX) as usize;
    let (fill, band_bank) = band_fill(e.band);
    if !cells.seeded || cells.cols != cols || cells.rows != rows {
        cells.seed(cols, rows, fill, band_bank);
    }
    let line = line_for(text, cols);
    let bytes = line.as_str().as_bytes();
    let mut index = 0;
    let mut row = 0;
    while row < rows {
        let mut col = 0;
        while col < cols {
            // `col`, not `index`: the character for a cell is decided by its column, and the same
            // character occupies every tile row of its cell (§5.5a's glyph is one 8 px column wide
            // and `TILE_ROWS_PER_LINE` tall). Indexing the string by the row-major cell index was
            // the bug 2a PR-(3)'s first hardware run found: row 0 drew the line correctly and row 1
            // read past its end, so every glyph lost its bottom half and the screen showed
            // unreadable fragments in the right places.
            let (tile, bank, source) = match bytes.get(col).copied() {
                Some(byte) => match glyphs::tile_for(byte as char, row) {
                    // A glyph this cut does not carry, and the blank sentinel (space): the band's
                    // own fill, so a cleared cell keeps the band instead of punching a hole through
                    // a blank map entry to the backdrop.
                    Some(t) if t != glyphs::BLANK_TILE => (t, chrome::BANK_TEXT, Source::Glyph),
                    _ => (fill, band_bank, Source::Chrome),
                },
                None => (fill, band_bank, Source::Chrome),
            };
            if cells.tiles[index] != tile
                || cells.banks[index] != bank
                || cells.sources[index] != source
            {
                writes.push(Write {
                    x: (e.x / chrome::TILE_PX + col as u32) as u8,
                    y: (e.y / chrome::TILE_PX + row as u32) as u8,
                    tile,
                    bank,
                    source,
                });
                cells.tiles[index] = tile;
                cells.banks[index] = bank;
                cells.sources[index] = source;
            }
            index += 1;
            col += 1;
        }
        row += 1;
    }
}

/// `M:SS` into `buf`, returning the bytes written. Minutes lose their leading zero only above 9;
/// the fixture album is 9:37 and a track is under ten minutes, but the arithmetic is general because
/// a title is allowed to be longer than the one this tree ships.
fn fmt_mmss(seconds: u32, buf: &mut [u8]) -> usize {
    let minutes = seconds / 60;
    let secs = seconds % 60;
    let mut digit = [0u8; 10];
    let mut n = 0;
    let mut m = minutes;
    loop {
        digit[n] = b'0' + (m % 10) as u8;
        n += 1;
        m /= 10;
        if m == 0 {
            break;
        }
    }
    let mut out = 0;
    let mut i = 0;
    while i < n {
        if out < buf.len() {
            buf[out] = digit[n - 1 - i];
            out += 1;
        }
        i += 1;
    }
    if out < buf.len() {
        buf[out] = b':';
        out += 1;
    }
    if out < buf.len() {
        buf[out] = b'0' + (secs / 10) as u8;
        out += 1;
    }
    if out < buf.len() {
        buf[out] = b'0' + (secs % 10) as u8;
        out += 1;
    }
    out
}

// ---------------------------------------------------------------------------------------------
// The ARM half: the two backgrounds, the palettes, and one commit
// ---------------------------------------------------------------------------------------------

#[cfg(target_arch = "arm")]
pub use arm::{Painted, Tilesets};

#[cfg(target_arch = "arm")]
mod arm {
    use super::*;
    use agb::display::Graphics;
    use agb::display::Priority;
    use agb::display::tiled::{
        RegularBackground, RegularBackgroundSize, TileEffect, TileFormat, TileSet, TileSetting,
    };

    /// The surface as the display sees it: §5.1's BG0 (chrome + title band) and BG1 (the scene
    /// region), both always shown.
    ///
    /// **Two backgrounds, not one.** The scene's rectangle is its own background because §5.2's
    /// switch mechanism is a priority write, and a switch needs the incoming scene to be a layer of
    /// its own. 2a has one scene, so nothing switches yet — but the *shape* is what 2b builds into,
    /// and a single background would have to be re-cut to get there.
    ///
    /// Both are shown into every frame. agb derives DISPCNT's enable mask from the backgrounds shown
    /// into *that* frame (`display/tiled.rs:372-377`), so dropping one from a frame does not merely
    /// skip it — it disables it. That is the trap `text.rs` documents, and the reason [`commit`]
    /// takes `&mut self` and shows both.
    pub struct Painted {
        surface: RegularBackground,
        scene: RegularBackground,
    }

    impl Painted {
        #[must_use]
        pub fn new(gfx: &mut Graphics<'_>, tiles: &Tilesets) -> Self {
            let mut surface = RegularBackground::new(
                Priority::P1,
                RegularBackgroundSize::Background32x32,
                TileFormat::FourBpp,
            );
            let mut scene = RegularBackground::new(
                Priority::P2,
                RegularBackgroundSize::Background32x32,
                TileFormat::FourBpp,
            );
            gfx.set_background_palette(
                chrome::BANK_TEXT,
                &chrome::vram::palette(&chrome::PALETTE_TEXT),
            );
            gfx.set_background_palette(
                chrome::BANK_CHROME,
                &chrome::vram::palette(&chrome::PALETTE_CHROME),
            );

            let mut writes = Writes::new();
            Surface::paint_blank(&mut writes);
            apply_to(&mut scene, &writes, tiles);

            let mut s = Surface::new();
            s.paint_static(&mut writes);
            apply_to(&mut surface, &writes, tiles);

            Self { surface, scene }
        }

        /// Apply a batch of writes to the surface's own map (the chrome and the title band).
        pub fn apply(&mut self, writes: &Writes, tiles: &Tilesets) {
            apply_to(&mut self.surface, writes, tiles);
        }

        /// Show both backgrounds into one frame and commit it.
        pub fn commit(&mut self, gfx: &mut Graphics<'_>) {
            let mut frame = gfx.frame();
            self.scene.show(&mut frame);
            self.surface.show(&mut frame);
            frame.commit();
        }
    }

    /// The two tilesets a surface draws from, as values that live for the run.
    ///
    /// **A write names which one it means, and this is why the plan carries `Source` at all.** agb's
    /// `set_tile` takes the tileset *by reference* and copies the tile's bytes from it with a raw
    /// pointer, bounds check included only in `TileSet::get_tile_data` — which `set_tile` does not
    /// call (`display/tiled/vram_manager.rs:962-1000`, `tile_set.tiles.as_ptr().add(offset)`). So a
    /// glyph index handed to the chrome tileset reads past its end in ROM, silently and without a
    /// panic: 2a PR-(3)'s first run sent every glyph tile to the 18-tile chrome set, which is exactly
    /// the "correct positions, garbage glyphs" the screen showed. The type now makes that a compile
    /// error rather than a picture.
    pub struct Tilesets {
        pub chrome: TileSet,
        pub glyphs: TileSet,
    }

    impl Tilesets {
        #[must_use]
        pub fn new() -> Self {
            Self {
                chrome: chrome::vram::tile_set(),
                glyphs: glyphs::vram::tile_set(),
            }
        }

        const fn for_source(&self, source: Source) -> &TileSet {
            match source {
                Source::Chrome => &self.chrome,
                Source::Glyph => &self.glyphs,
            }
        }
    }

    impl Default for Tilesets {
        fn default() -> Self {
            Self::new()
        }
    }

    fn apply_to(bg: &mut RegularBackground, writes: &Writes, tiles: &Tilesets) {
        for w in writes.as_slice() {
            bg.set_tile(
                (w.x as i32, w.y as i32),
                tiles.for_source(w.source),
                TileSetting::new(w.tile, TileEffect::new(false, false, w.bank)),
            );
        }
    }
}
