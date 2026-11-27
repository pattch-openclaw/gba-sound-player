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
//! **Why three layers, and what each one owns.** The display is a background for the chrome/title
//! art, a background for the scene region, and a foreground for everything drawn *over* that art
//! (the glyphs, the five control icons, and the bar). They are separate `RegularBackground`s because
//! a transparent pixel reveals the layer below: glyph and icon tiles are mostly transparent, so
//! keeping them on the same background as the band fill would punch backdrop-coloured holes in the
//! art instead of letting it show through. BG0 is the art background, BG1 the scene region, BG2 the
//! foreground — §5.1's own BG0/BG1/BG2 allocation, with the controls treated as foreground art
//! rather than as part of the fill they sit on.
//!
//! **"Changed cells only" is a statement about the plan, not about the VRAM copy.** The write list is
//! element-addressed and agb's `set_tile` is called only for cells whose tile, bank or tileset
//! changed. agb 0.25 has no partial-map commit: `RegularBackground::show` includes a background's
//! map only when `Tiles::is_dirty` says it changed, but a dirty map is then copied **whole**
//! (2,048 B) by `frame.commit()`; a clean background costs no map copy. So a clock tick pays one
//! dirty foreground screenblock (2 KB), a title/bar or scene change pays its own, and a still frame
//! pays none. At 2a's cadence (clock 1 Hz, title/bar on events) that is bounded, and AP-3b (the
//! per-element legs) and AP-3c (the presentation path, which owns the dirty-screenblock cost) are
//! where it is priced. The phrase must not be reread later as "only changed cells reach
//! VRAM": a changed cell costs its background's whole map, and no agb API on this path makes it
//! smaller.
//!
//! ## The chrome's hot elements: what an update costs, and where OBJ would change the answer
//!
//! Four chrome elements move while audio is playing, and every one of them lives on the **foreground**
//! background (BG2): the clock when a displayed second changes, the state word on a transport event,
//! the bar when a §6 cell advances, the title on a track change. The five control icons move once,
//! at boot, and never again. What each costs *per update*, as the cells the diff emits — every figure
//! below is the declared box in `layout::ELEMENTS` and `draw_text`'s per-cell diff, not an estimate:
//!
//! | element | declared box | cells one update writes |
//! |---|---|---|
//! | clock | 12 × 2 | the digits whose glyph differs — one most seconds, two to four across a carry; the `/total` half is static within a track |
//! | state word | 9 × 2 | every column whose glyph differs — `PLAYING`→`PAUSED` is most of them |
//! | bar | 10 × 1 | the one column the fill boundary sits in — §6's grid is `BAR_CELLS = 80` at `BAR_CELL_PX = 1`, so one cell moves it one pixel |
//! | title | 20 × 2 | the columns whose glyph differs, plus the elision marker and the padding cells a shorter line frees |
//!
//! The title row is absent on a variant that declares no title (`TITLE` is `Option`), and the icons
//! are not in this table because they never appear in an update batch after boot.
//!
//! **What those cell counts do not decide.** There is no partial-map commit, so one changed cell and
//! forty changed cells cost the *same* `commit`: the foreground's whole 2,048 B map cloned into the
//! frame and copied again into VRAM (`regular_background.rs:401-416` → `tiled.rs:410`), while the two
//! untouched backgrounds cost nothing. The BG path's cost is therefore **quantised by background, not
//! by cell**. That quantisation is the whole of §5.7's chrome-OBJ argument as a storage question: an
//! OAM shadow write is 8 B for the one sprite that moved, and the frame's OAM DMA is a fixed 1 KiB
//! whether one sprite moved or fifty. BG wins when many cells move together or when the layer is
//! static (the art, the region, the icons — all BG today and all BG in 2b's plan); OBJ wins when a
//! *few* cells move *often*, which is exactly the clock-digit and bar-boundary case above.
//!
//! **And the axis that actually bites is not storage.** `Painted::commit` is a rendezvous with the
//! display before it is a memory copy: agb waits for a VBlank *first* (`display/mod.rs:314-337`), and
//! that wait is a BIOS halt whose length the screen's cadence sets, not the audio's. Moving the chrome
//! to sprites does not remove it — `oam_frame.commit()` runs inside the same call, after the wait
//! (`object/unmanaged/object.rs:48-66`). So a frequently updated chrome is an argument for
//! VBlank-driven presentation **first**, and for OBJ only because OBJ is the renderer that makes that
//! presentation easy: a shadow to write, a DMA to start, no map to invalidate. §5.7 keeps the two
//! axes separate because the 2a answer is "BG, and spaced so the wait is not taken", and the 2b answer
//! is "either renderer, presented from VBlank" — two different questions that one sentence about
//! "OBJ vs BG" would have blurred.
//!
//! ## Two halves, and the reason for the split
//!
//! The *decision* half is platform-free: which cells should hold which tiles, and which of those
//! differ from what is on the map. It produces a list of [`Write`]s, so the host gate can assert a
//! track change's cost in cells rather than in prose. The *applying* half is
//! `#[cfg(target_arch = "arm")]`: three agb `RegularBackground`s and one commit. This is `keys.rs`'s
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
//! atlas still builds and `measure` exceeds `tilemap_px` — and [`FACES_FIT_A_CELL`] turns that
//! from a silent mis-draw into a build failure on **every face a declared text element actually
//! names**, not just the first face in the generated table. When a future face trips it the fix is
//! that note's refusal, not a second renderer here.
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
use crate::nodiv;

/// The most cells a single-line text element can hold. 2a's largest is the title's 20×2 = 40; the
/// bound leaves headroom for a clock or label box. A multi-row text window (2b's `list`, 30×16) is
/// not a line and is not drawn through [`draw_text`], which addresses one line box at a time.
pub const MAX_CELLS: usize = 60;

/// The most characters a drawn line can hold: the widest declared text element, in cells.
pub const MAX_LINE: usize = layout::SCREEN_COLS as usize;

/// The most cells one batch of writes carries: the whole visible screen, because a page fill must
/// cover every cell. The region (480), the bands (120) and a frame's elements (≤92) all fit inside
/// it. [`Writes::push`] returns whether the write fit, so a caller never records state for a cell
/// that was dropped.
pub const MAX_WRITES: usize = layout::SCREEN_COLS as usize * layout::SCREEN_ROWS as usize;

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

/// §6's progress cell, in `0..=BAR_CELLS`.
///
/// A distinct type because the painted position it is derived from is **samples**, and the two are
/// both `u32`. Handing a sample count to [`View::bar_cell`] was 2a PR-(3)'s progress-bar bug: a
/// 4.2-million-sample position passed as if it were 0..80 filled every column on the first frame.
/// The only constructors are [`BarCell::new`] for a cell a caller already has (a test, a fixture)
/// and [`BarCell::from_samples`], which applies the track's §6 reciprocal. There is deliberately no
/// `From<u32>`: the compiler, not a reader, should have to be convinced a cell is a cell.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Default)]
pub struct BarCell(u32);

impl BarCell {
    /// A cell a caller already has, clamped to the declared grid.
    #[must_use]
    pub const fn new(cell: u32) -> Self {
        if cell > layout::BAR_CELLS {
            Self(layout::BAR_CELLS)
        } else {
            Self(cell)
        }
    }

    /// The cell a sample position falls in, through the track's §6 reciprocal (`floor(2^32 ×
    /// BAR_CELLS / n_samples)`). This is the only conversion from samples to a cell.
    #[must_use]
    pub const fn from_samples(pos_in_track: u32, recip: u32) -> Self {
        // `new` clamps: a stale or corrupt reciprocal must not index a bar column that is not
        // declared, and the draw path treats a cell as a position rather than a bound.
        Self::new(nodiv::bar_cell(pos_in_track, recip))
    }

    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// A batch of [`Write`]s: what changed, in map coordinates. Fixed capacity because the ROM has no
/// allocator on this path, and bounded by the full visible screen because a page fill must cover it.
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

    /// Forget the batch and its overflow count. Capacity is fixed, so this is the only reuse path;
    /// resetting `dropped` here makes the count a property of the batch a caller is about to inspect.
    pub fn clear(&mut self) {
        self.len = 0;
        self.dropped = 0;
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.len
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Writes that did not fit in this batch. Zero for every geometry this tree declares; a non-zero
    /// count is a spec that outgrew the buffer, and the host gate refuses it by name rather than
    /// letting a partial draw look like a complete one.
    #[must_use]
    pub fn dropped(&self) -> usize {
        self.dropped
    }

    #[must_use]
    pub fn as_slice(&self) -> &[Write] {
        &self.buf[..self.len]
    }

    /// Append one write, returning whether it fit. A caller that keeps a shadow of the map
    /// ([`Cells`], the bar's column tiles, the icons) must update that shadow **only on `true`**: a
    /// dropped write means the map did not change, so recording it would make the next frame's diff
    /// skip the cell forever. `false` is also the signal the host gate reads through [`dropped`].
    pub fn push(&mut self, w: Write) -> bool {
        if self.len < MAX_WRITES {
            self.buf[self.len] = w;
            self.len += 1;
            true
        } else {
            self.dropped += 1;
            false
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
/// advance-sum bound — see the module docs and [`FACES_FIT_A_CELL`]. The bake's verdict
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

/// The index of the declared element with id `id`, at compile time, if the variant declares it.
///
/// Variant A declares no title line (§5.0's A row: its title band is art/bar only), so the lookup is
/// optional and [`TITLE`] is the one element a variant may omit. The required elements keep
/// [`element_index`], whose missing id is a compile error: §5.3 keys things by identity, so a spec
/// that renames `clock` should fail the build rather than draw the clock into whatever element
/// happens to sit at index 3 (the drift D9 is about).
const fn element_index_opt(id: &str) -> Option<usize> {
    let mut i = 0;
    while i < layout::ELEMENTS.len() {
        if streq(layout::ELEMENTS[i].id, id) {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// The index of a declared element a variant must carry.
const fn element_index(id: &str) -> usize {
    match element_index_opt(id) {
        Some(index) => index,
        None => panic!("this variant declares no element with this id"),
    }
}

/// The chrome band's clock: `elapsed / total` (§5.5, A6).
pub const CLOCK: usize = element_index("clock");
/// The word beside the control icons: §5.6's "a control's drawn state plus a word".
pub const KEYLABELS: usize = element_index("keylabels");
/// The title line in the title band: one line, elided, never wrapped (§5.5). `None` on variant A,
/// whose 1-row title band deliberately carries no text line.
pub const TITLE: Option<usize> = element_index_opt("title_text");
/// §6's progress bar, in the title band's lower row.
pub const BAR: usize = element_index("bar");
/// The authored control art's element; the five slots sit inside it (§5.3).
pub const KEYART: usize = element_index("keyart");

/// 2b PR-(3)'s title-card spans and picture. `Option`, like [`TITLE`], because a
/// variant that declares no card (the 8 px instrument, §5.0's A) must still compile —
/// and then the card scene is not in [`SceneId::IMPLEMENTED`].
pub const CARD_TITLE: Option<usize> = element_index_opt("card_title");
pub const CARD_ARTIST: Option<usize> = element_index_opt("card_artist");
pub const CARD_YEAR: Option<usize> = element_index_opt("card_year");
pub const CARD_COUNTS: Option<usize> = element_index_opt("card_counts");
pub const CARD_ART: Option<usize> = element_index_opt("card_art");

/// A card is declared whole or not at all: four spans and its picture are one scene, and
/// a variant that declared only some of them would paint a card with a hole in it at
/// run time. The 8 px instrument (A) declares none, which is why these are `Option`s.
const _: () = assert!(
    CARD_TITLE.is_some() == CARD_ARTIST.is_some()
        && CARD_ARTIST.is_some() == CARD_YEAR.is_some()
        && CARD_YEAR.is_some() == CARD_COUNTS.is_some()
        && CARD_COUNTS.is_some() == CARD_ART.is_some(),
    "the title card declares its four spans and its picture together"
);

/// The scenes this ROM implements, in the spec's order (§5.2's registry).
///
/// It is a **filtered view** of the generated `layout::SCENES`: the spec declares
/// `art`, `list`, `title_card` and `blank`, and this PR implements the latter two. A
/// host leg proves every id here exists in `layout::SCENES` and that this relative
/// order is the spec's, so `Select`'s cycle is checked against the declaration rather
/// than against a match arm nobody rereads. Scenes land by extending this table, not
/// by rewriting the cycle.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SceneId {
    /// A12: the album's title card, metadata only while `ART1` is 2b PR-(4)'s.
    TitleCard,
    /// 2a's flat region fill — the boot scene under option (c).
    Blank,
}

impl SceneId {
    /// The implemented scenes, in `layout::SCENES` order. `title_card` precedes
    /// `blank` because the spec declares it first; the order is the registry's, not
    /// this file's taste.
    pub const IMPLEMENTED: [SceneId; 2] = [SceneId::TitleCard, SceneId::Blank];

    /// The spec's name for this scene, as `layout::SCENES[].id` spells it.
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            SceneId::TitleCard => "title_card",
            SceneId::Blank => "blank",
        }
    }

    /// The next implemented scene — §5.3's `Select`, one press per step.
    #[must_use]
    pub const fn next(self) -> SceneId {
        match self {
            SceneId::TitleCard => SceneId::Blank,
            SceneId::Blank => SceneId::TitleCard,
        }
    }
}

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

/// Whether every face a declared text element actually names has advances that fit in one cell —
/// the premise the module docs state.
///
/// `max_advance_permyriad` is ten-thousandths of a pixel, generated from the same `hmtx` read the
/// atlas packs from, so this is the face's own number rather than a copy of one. It walks the
/// elements rather than checking `FACES[0]`: the face order is the spec's declaration order, not a
/// promise that index 0 is the face the text draws in, and a variant whose first face is not its
/// text face would otherwise pass a check about the wrong file.
const fn faces_fit_a_cell() -> bool {
    let mut element = 0;
    while element < layout::ELEMENTS.len() {
        let e = &layout::ELEMENTS[element];
        if streq(e.kind, "text") {
            let mut face = 0;
            let mut found = false;
            while face < layout::FACES.len() {
                if streq(layout::FACES[face].id, e.face) {
                    found = true;
                    if layout::FACES[face].max_advance_permyriad > chrome::TILE_PX * 10_000 {
                        return false;
                    }
                }
                face += 1;
            }
            if !found {
                return false;
            }
        }
        element += 1;
    }
    true
}
const FACES_FIT_A_CELL: bool = faces_fit_a_cell();
const _: () = assert!(
    FACES_FIT_A_CELL,
    "a declared text element's face advances more than one 8 px cell per glyph: a one-glyph-per-cell drawing would overlap glyphs and clip the line, so the atlas's cell model is not this face's model"
);

/// Whether every single-line text element fits [`MAX_CELLS`]. A multi-row window (2b's list) is not
/// drawn by [`draw_text`] and is deliberately exempt.
const fn text_elements_fit_max_cells() -> bool {
    let mut i = 0;
    while i < layout::ELEMENTS.len() {
        let e = &layout::ELEMENTS[i];
        if streq(e.kind, "text") && e.text_rows == 1 && (e.wt * e.ht) as usize > MAX_CELLS {
            return false;
        }
        i += 1;
    }
    true
}
const _: () = assert!(
    text_elements_fit_max_cells(),
    "a single-line text element has more cells than MAX_CELLS can hold"
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

/// Rows occupied by the always-shown bands below the region, derived from the generated table.
const fn always_band_rows() -> u32 {
    let mut total = 0;
    let mut i = 0;
    while i < layout::BANDS.len() {
        if layout::BANDS[i].always {
            total += layout::BANDS[i].rows;
        }
        i += 1;
    }
    total
}

const _: () = assert!(
    REGION_ROW == 0 && layout::REGION.rows + always_band_rows() == layout::SCREEN_ROWS,
    "the region is not the top of the screen with the always-shown bands beneath it"
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
    /// §6's progress cell, `0..=BAR_CELLS`, from [`BarCell::from_samples`] and the track's §6
    /// reciprocal. **Never a sample count**; the type exists so that mistake cannot compile.
    pub bar_cell: BarCell,
    /// The transport state as a word (§5.6: shape *and* a word; until §5.3's `PAUSED` badge lands
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

    /// The transparent page fill: every visible cell of a background set to `chrome::FILL_PAGE`,
    /// which is a real zero tile, not agb's reserved blank tile.
    ///
    /// This is the hardware-robustness layer. An agb `RegularBackground` map starts as tile index 0,
    /// the *reserved* blank tile agb never writes; after a normal BIOS boot VRAM is zero and that
    /// reads as transparent, but agb's own entrypoint says Quick Boot on an Everdrive does not
    /// initialise RAM, and it clears `.bss` for that reason while leaving VRAM alone. A ROM that
    /// leaves visible cells unwritten therefore shows whatever VRAM held on such a loader. Writing
    /// `FILL_PAGE` over every cell makes each visible cell point at a tile this ROM uploaded, so the
    /// surface no longer depends on the loader's VRAM state. The diagnostic `no-page-fill` feature
    /// omits this deliberately, to A/B that hypothesis on hardware (`make album-diff-rom`).
    pub fn paint_page(writes: &mut Writes) {
        writes.clear();
        writes.fill(
            0,
            0,
            layout::SCREEN_COLS,
            layout::SCREEN_ROWS,
            chrome::FILL_PAGE,
            chrome::BANK_TEXT,
        );
    }

    /// The background layer's static content: the always-shown bands' fills. Written once at boot.
    ///
    /// The scene region is its own background and the foreground is its own background (see the ARM
    /// half), because §5.2's switch mechanism needs the region to be a layer of its own and because
    /// glyphs and icons must show the band art through their transparent pixels. `FILL_PAGE` is the
    /// page fill [`paint_page`](Self::paint_page) writes *under* these cells.
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
    }

    /// The foreground layer's static content: §5.3's five control icons, in the slots the variant
    /// declares. Written once at boot, above the band art so the icon's negative space shows it.
    ///
    /// Positions come from the generated table and drawings from `chrome.rs`; neither is transcribed
    /// into the other.
    pub fn paint_icons(&mut self, writes: &mut Writes) {
        writes.clear();
        let mut slot = 0;
        while slot < layout::CONTROL_SLOTS.len() {
            let s = &layout::CONTROL_SLOTS[slot];
            if let Some(tile) = chrome::icon_for_slot(s.key) {
                // Only record the icon as painted if the write fit; otherwise the next frame would
                // believe a tile was on the map that never got there.
                if writes.push(Write {
                    x: (s.px_x / chrome::TILE_PX) as u8,
                    y: (s.px_y / chrome::TILE_PX) as u8,
                    tile,
                    bank: chrome::BANK_CHROME,
                    source: Source::Chrome,
                }) {
                    self.icons[slot] = tile;
                }
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

    /// 2b PR-(3): paint the title card into the scene region.
    ///
    /// Every cell the card names is written with an **opaque** tile — the region fill,
    /// not the foreground's transparent page tile — because a scene must cover its
    /// rectangle (§5.2's backing rule, whose first consumer this is). The picture is a
    /// placeholder fill until `ART1`/`album_art.py` land in 2b PR-(4): the element is
    /// declared and its box painted, so the layout and the pool audit are already the
    /// product's, while the pixels are still a later PR's.
    ///
    /// The counts line is ASCII. §5.5a's cut is U+0020-U+007E and has no middle dot,
    /// so the separator is `-` — the same finding 2a PR-(2) made about U+2026, stated
    /// once here rather than rediscovered as a blank glyph.
    pub fn paint_card(writes: &mut Writes, card: &Card<'_>) {
        writes.clear();
        if let Some(index) = CARD_ART {
            let e = &layout::ELEMENTS[index];
            writes.fill(
                e.x / chrome::TILE_PX,
                e.y / chrome::TILE_PX,
                e.w / chrome::TILE_PX,
                e.h / chrome::TILE_PX,
                chrome::FILL_SCENE,
                chrome::BANK_TEXT,
            );
        }

        let mut counts = [0u8; 32];
        let counts = fmt_counts(card.n_tracks, card.seconds, &mut counts);
        let mut year = [0u8; 8];
        let year = if card.year == 0 {
            None
        } else {
            let n = fmt_u32(u32::from(card.year), &mut year);
            core::str::from_utf8(&year[..n]).ok()
        };
        let artist = if card.artist.is_empty() {
            "UNKNOWN"
        } else {
            card.artist
        };

        // One `Cells` per span: the state is the element's diff, and two spans with the
        // same dimensions but different origins would otherwise share it.
        if let Some(index) = CARD_TITLE {
            let mut cells = Cells::new();
            draw_text_on(
                &mut cells,
                index,
                card.title,
                writes,
                chrome::FILL_SCENE,
                chrome::BANK_TEXT,
            );
        }
        if let Some(index) = CARD_ARTIST {
            let mut cells = Cells::new();
            draw_text_on(
                &mut cells,
                index,
                artist,
                writes,
                chrome::FILL_SCENE,
                chrome::BANK_TEXT,
            );
        }
        if let (Some(index), Some(year)) = (CARD_YEAR, year) {
            let mut cells = Cells::new();
            draw_text_on(
                &mut cells,
                index,
                year,
                writes,
                chrome::FILL_SCENE,
                chrome::BANK_TEXT,
            );
        }
        if let Some(index) = CARD_COUNTS {
            let mut cells = Cells::new();
            draw_text_on(
                &mut cells,
                index,
                counts,
                writes,
                chrome::FILL_SCENE,
                chrome::BANK_TEXT,
            );
        }
    }

    /// Bring the foreground map up to date with `view`, appending only the cells that changed.
    ///
    /// The writes go to the foreground background: glyphs, the bar, and the transparent page tile in
    /// the cells a line does not use, so the band art on the background below shows through.
    ///
    /// Returns whether anything was written. A frame with no change writes nothing and the caller
    /// must not commit — §5.1's rule 2 and §5.2's cadence rule in one return value.
    pub fn update(&mut self, view: &View<'_>, writes: &mut Writes) -> bool {
        writes.clear();
        self.advance_clock(view);
        self.draw_clock(view, writes);
        draw_text(&mut self.labels, KEYLABELS, view.state_word, writes);
        if let Some(title) = TITLE {
            draw_text(&mut self.title, title, view.title, writes);
        }
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
        let bar_cell = view.bar_cell.get();
        let mut col = 0;
        while col < cols {
            let column_start = (col as u32) * chrome::TILE_PX;
            let filled = if bar_cell.saturating_mul(cell_px) <= column_start {
                0
            } else {
                let inside = bar_cell.saturating_mul(cell_px) - column_start;
                if inside > chrome::TILE_PX {
                    chrome::TILE_PX
                } else {
                    inside
                }
            };
            let tile = chrome::bar_tile(filled);
            if self.bar[col] != tile {
                // The shadow records a cell only when the write fit; a dropped write must be
                // retried by a later frame rather than believed.
                if writes.push(Write {
                    x: (base_x + col as u32) as u8,
                    y: base_y as u8,
                    tile,
                    bank: chrome::BANK_CHROME,
                    source: Source::Chrome,
                }) {
                    self.bar[col] = tile;
                }
            }
            col += 1;
        }
    }

    /// The five control icons, which never change after boot: written by [`paint_icons`], exposed so
    /// a future state-carrying icon (2b's `PAUSED` badge) has one place to change.
    #[must_use]
    pub fn icons(&self) -> &[u16; chrome::ICON_TILES] {
        &self.icons
    }

    /// Whether the foreground's static layer (the control icons) has been written.
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
    // Text lives on the foreground layer, so its padding is the *transparent* page tile rather than
    // the band fill. The band fill is on the background below it; writing it here would paint the
    // text layer opaque and hide the art the user wants behind the words.
    draw_text_on(
        cells,
        element,
        text,
        writes,
        chrome::FILL_PAGE,
        chrome::BANK_TEXT,
    );
}

/// [`draw_text`] on a caller-chosen backing cell.
///
/// The title card draws text on the **scene** background, whose empty cells are the
/// region fill and must stay opaque (§5.2's backing rule). `fill`/`band_bank` are the
/// cell a glyph-less position takes: `FILL_PAGE` on the foreground, `FILL_SCENE` on
/// the scene. Everything else — the cell model, the column-not-index rule, the
/// tileset-named writes — is shared, because a second copy of this loop is exactly the
/// bug 2a PR-(3)'s first run found.
fn draw_text_on(
    cells: &mut Cells,
    element: usize,
    text: &str,
    writes: &mut Writes,
    fill: u16,
    band_bank: u8,
) {
    let e = &layout::ELEMENTS[element];
    let cols = (e.w / chrome::TILE_PX) as usize;
    let rows = (e.h / chrome::TILE_PX) as usize;
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
                    // A glyph this cut does not carry, and the blank sentinel (space): the
                    // transparent page tile, so the background below shows through.
                    Some(t) if t != glyphs::BLANK_TILE => (t, chrome::BANK_TEXT, Source::Glyph),
                    _ => (fill, band_bank, Source::Chrome),
                },
                None => (fill, band_bank, Source::Chrome),
            };
            if cells.tiles[index] != tile
                || cells.banks[index] != bank
                || cells.sources[index] != source
            {
                if writes.push(Write {
                    x: (e.x / chrome::TILE_PX + col as u32) as u8,
                    y: (e.y / chrome::TILE_PX + row as u32) as u8,
                    tile,
                    bank,
                    source,
                }) {
                    cells.tiles[index] = tile;
                    cells.banks[index] = bank;
                    cells.sources[index] = source;
                }
            }
            index += 1;
            col += 1;
        }
        row += 1;
    }
}

/// A decimal `u32` into `buf`, returning the bytes written (no padding).
fn fmt_u32(mut value: u32, buf: &mut [u8]) -> usize {
    let mut digit = [0u8; 10];
    let mut n = 0;
    loop {
        digit[n] = b'0' + (value % 10) as u8;
        n += 1;
        value /= 10;
        if value == 0 {
            break;
        }
    }
    let mut out = 0;
    while out < n && out < buf.len() {
        buf[out] = digit[n - 1 - out];
        out += 1;
    }
    out
}

/// `N TRACKS - M MIN` into `buf`, returning it as a `&str`.
///
/// Minutes round **up**: a 9:37 album is a ten-minute card, not a nine-minute one, and
/// this is a metadata line rather than the transport clock (which truncates
/// deliberately, §6). `seconds / 60` is a constant divisor, which the compiler
/// strength-reduces — no `__aeabi_uidiv` is introduced on the card's event path.
fn fmt_counts<'a>(n_tracks: u32, seconds: u32, buf: &'a mut [u8]) -> &'a str {
    let minutes = (seconds + 59) / 60;
    let mut out = 0;
    out += fmt_u32(n_tracks, &mut buf[out..]);
    for byte in b" TRACKS - " {
        if out < buf.len() {
            buf[out] = *byte;
            out += 1;
        }
    }
    out += fmt_u32(minutes, &mut buf[out..]);
    for byte in b" MIN" {
        if out < buf.len() {
            buf[out] = *byte;
            out += 1;
        }
    }
    core::str::from_utf8(&buf[..out]).unwrap_or("")
}

/// The strings the card draws, separated from the album so the host gate can paint a
/// card without a baked manifest. The app fills this from `meta::Album`.
pub struct Card<'a> {
    /// The album title (§5.2's card row).
    pub title: &'a str,
    /// The album artist; empty renders `UNKNOWN` rather than an empty line.
    pub artist: &'a str,
    /// The album year; `0` omits the line.
    pub year: u16,
    /// The number of tracks.
    pub n_tracks: u32,
    /// The album's total length in **seconds**, for the rounded minutes line.
    pub seconds: u32,
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
// The ARM half: the three backgrounds, the palettes, and one commit
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

    /// The surface as the display sees it: BG0 the chrome/title art, BG1 the scene region, BG2 the
    /// foreground (glyphs, control icons and the bar). All three are always shown.
    ///
    /// **Three backgrounds, and why each is separate.** §5.2's switch mechanism is a priority write,
    /// so the scene region needs to be a layer of its own. The foreground is a layer of its own for a
    /// different reason: glyph and icon tiles are mostly transparent, and a transparent pixel reveals
    /// the layer below. On one background with the band fills, every character and icon would punch a
    /// backdrop-coloured hole in the art instead of letting it show through; on separate layers the
    /// text sits *over* the fill, which is the composition the plan's BG0/BG2 split exists for.
    ///
    /// All three are shown into every frame. agb derives DISPCNT's enable mask from the backgrounds
    /// shown into *that* frame (`display/tiled.rs:372-377`), so dropping one from a frame does not
    /// merely skip it — it disables it. That is the trap `text.rs` documents, and the reason
    /// [`commit`](Self::commit) takes `&mut self` and shows all three.
    pub struct Painted {
        /// BG0: the always-shown bands' fills and, later, their art.
        background: RegularBackground,
        /// BG1: the scene region.
        scene: RegularBackground,
        /// BG2: the glyphs, the five control icons and the bar, drawn over BG0.
        foreground: RegularBackground,
    }

    impl Painted {
        #[must_use]
        pub fn new(gfx: &mut Graphics<'_>, tiles: &Tilesets) -> Self {
            let mut background = RegularBackground::new(
                Priority::P1,
                RegularBackgroundSize::Background32x32,
                TileFormat::FourBpp,
            );
            let mut scene = RegularBackground::new(
                Priority::P2,
                RegularBackgroundSize::Background32x32,
                TileFormat::FourBpp,
            );
            let mut foreground = RegularBackground::new(
                Priority::P0,
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

            page_fill(&mut background, &mut writes, tiles);
            let mut s = Surface::new();
            s.paint_static(&mut writes);
            apply_to(&mut background, &writes, tiles);

            page_fill(&mut scene, &mut writes, tiles);
            Surface::paint_blank(&mut writes);
            apply_to(&mut scene, &writes, tiles);

            page_fill(&mut foreground, &mut writes, tiles);
            s.paint_icons(&mut writes);
            apply_to(&mut foreground, &writes, tiles);

            Self {
                background,
                scene,
                foreground,
            }
        }

        /// Apply a batch of writes to the foreground map (the glyphs, icons and bar).
        pub fn apply(&mut self, writes: &Writes, tiles: &Tilesets) {
            apply_to(&mut self.foreground, writes, tiles);
        }

        /// Apply a batch of writes to the **scene** map — the title card's layer (2b PR-(3)).
        ///
        /// The scene background was written once at construction (the page fill and the
        /// blank fill) and has been static since 2a. A card is the first scene to draw
        /// at run time, and it draws here rather than on the foreground because §5.0's
        /// rectangle rule is the whole split: the scene owns its cells, the foreground
        /// owns the chrome's.
        pub fn apply_scene(&mut self, writes: &Writes, tiles: &Tilesets) {
            apply_to(&mut self.scene, writes, tiles);
        }

        /// Show the three backgrounds into one frame and commit it.
        ///
        /// The show order fixes the BG numbers: background BG0, scene BG1, foreground BG2 — the
        /// allocation §5.1's layer table names, so the comment above and the commit agree.
        ///
        /// **The commit is not element-addressed.** The write plan is, but agb has no partial-map
        /// commit: `show()` includes a background's map only when it is dirty, and a dirty map is
        /// copied whole (2,048 B). A clean background costs no map copy, so a clock tick pays one
        /// foreground screenblock and a still frame pays none; OAM's own commit still runs on every
        /// `frame.commit()` (it copies the object count and clears the unused entries), a small
        /// fixed cost even with no objects. At 2a's cadence this is bounded; AP-3b (per-element) and
        /// AP-3c (the presentation path, whose row names the dirty-screenblock cost) price it.
        /// Nothing here writes VRAM cell by cell.
        ///
        /// **The first thing this call does is wait, and that is why the main loop is not supposed to
        /// call it while audio is running (§5.7).** `GraphicsFrame::commit` runs
        /// `VBlank::wait_for_vblank()` before it copies anything (`display/mod.rs:317`), and that is
        /// `swi 0x05` — `VBlankIntrWait`, a BIOS **halt**, not a spin (`syscall.rs:25-29`). Its fast
        /// path is narrower than it looks (`interrupt.rs:417-427`): the previous call leaves
        /// `last_waited_number = n + 1` where `n` was the vblank count then, and the fast path is
        /// `last_waited_number < n'` — so **two** vblanks must have elapsed since the last commit for
        /// this one to return without halting. One commit per video frame therefore halts every time,
        /// for up to `VIDEO_PERIOD_CYCLES` = 280,896 c, which is AP-0's measured number and
        /// 0.49 of the 573,440-cycle feeder period: exactly the shape a per-frame chrome would be,
        /// and the reason that is 2b's VBlank-presentation work rather than a rate this call site
        /// can raise. Commits seconds apart — 2a's cadence — take the fast path as a matter of
        /// arithmetic.
        ///
        /// **What the halt pauses, and what it cannot.** The audio chain is not the thing being
        /// paused: the Timer1 ISR still fires during the halt, and agb's display path masks no
        /// interrupts and holds no lock the ISR needs (checked against 0.25.0 — no `critical_section`
        /// or `interrupt::disable` anywhere under `src/display/`), so the period's `take_chunk` +
        /// `rearm_dma` stay the ISR's business (`album_play_main.rs`'s handler). What stops is the
        /// **producer**, for up to one video period, and the ring's prefill is what absorbs that.
        /// `PASS_MARGIN_CYCLES` is the same reservation written down as a bound: one video frame per
        /// main-loop pass, which is enough for one worst-case commit and is spent by it — so the
        /// invariant this call site has to keep is *at most one commit per pass*, and
        /// `Paint::due`'s latch compare is what keeps it (`sample()` stamps the latch before the
        /// commit, so a second serve cannot happen in the same frame).
        ///
        /// **So: an exception, measured, not a model.** §5.7's rule names two unconditional commit
        /// sites — this surface's boot upload and the verdict takeover, neither of which has a live
        /// producer to starve — and treats a main-loop commit during playback as an interim exception
        /// that AP-3c either retires (arm B, chrome as OBJ presented by a VBlank DMA) or keeps with a
        /// number on it (arm A, this path). Nothing here should be read as licensed to commit per
        /// frame: the fix for a chrome that needs 60 Hz is never a tighter loop around this call.
        pub fn commit(&mut self, gfx: &mut Graphics<'_>) {
            let mut frame = gfx.frame();
            self.background.show(&mut frame);
            self.scene.show(&mut frame);
            self.foreground.show(&mut frame);
            frame.commit();
        }
    }

    /// Write the transparent page fill over every visible cell of `bg`, unless the hardware
    /// diagnostic feature `no-page-fill` deliberately omits it.
    ///
    /// The default (fill present) is the robust build: every visible cell points at a tile this ROM
    /// uploaded, so a Quick-Boot loader that leaves VRAM uninitialised cannot show through agb's
    /// reserved blank tile. The `no-page-fill` build is the A/B half of `make album-diff-rom`.
    fn page_fill(bg: &mut RegularBackground, writes: &mut Writes, tiles: &Tilesets) {
        #[cfg(not(feature = "no-page-fill"))]
        {
            Surface::paint_page(writes);
            apply_to(bg, writes, tiles);
        }
        #[cfg(feature = "no-page-fill")]
        {
            let _ = (bg, writes, tiles);
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
