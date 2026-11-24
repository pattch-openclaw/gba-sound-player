//! The always-shown chrome: §5.1's BG0 — the band fills, §5.3's authored control art, and §6's
//! partial-fill bar patterns.
//!
//! **Authored art, not font glyphs** (§5.3, ruled 2026-11-20). The transport symbols this surface
//! needs (`prev`, `play`, `pause`, `next`, `restart`) do not exist in the declared face — its cut is
//! U+0020–U+007E — and widening the face is a Q9 decision this module must not take silently. So the
//! icons are pixel art authored here, and the reasoning governs every future icon: an icon is not a
//! character.
//!
//! **The source is this table, and the table is the asset.** §5.3 allows "one tracked source image
//! (or a text-bitmap table, if that is easier to read in a diff)". This is the second form, and it is
//! the cheaper one to review: each icon is eight `u8` rows of one bit per pixel, so a diff shows the
//! drawing. What a generated module would otherwise be is derived here at compile time by
//! [`pack`] — one 1bpp row to four 4bpp bytes — which is an arithmetic step, not an art step, and
//! keeping it here means the ROM links no asset blob for nine tiles.
//!
//! **Placeholders are allowed to be ugly, and are not allowed to be indistinguishable** (§5.3). The
//! five icons differ in *shape*, which is what §5.6 requires: a diamond, a triangle, two arrows and
//! a return glyph. Nothing here differs from anything else only in hue, so the gate that reads
//! shape (and the colourblind verifier) both have something to read. The sixth bitmap §5.3 reserves
//! room for — the `PAUSED` badge — is 2b's, and until it lands **the transport state is carried by a
//! word** in `keylabels`, which is the alternative §5.6 names in its own sentence.
//!
//! ## Why the tile count is a compile-time tie to `layout.rs`
//!
//! §5.1's pool audit sums *declared* distinct tiles: `layout::POOL_CHROME_DECLARED`, generated from
//! the spec's `chrome_art_tiles`, whose comment enumerates it as "4 backdrop fills + one 8x8 tile per
//! declared control slot". This module is the thing those tiles actually are, so the two counts are
//! asserted equal at the bottom of the file: an icon added here without the declared number moving
//! is a pool undercount, and it is a build failure rather than a rounding error someone finds on
//! hardware. Same arrangement as `glyphs.rs`'s cross-module asserts, and for the same reason.
//!
//! ## Palette
//!
//! §5.4 assigns bank 0 to text and banks 1–2 to the chrome and the bar. Two banks is all this
//! phase draws with, and the *values* the tiles use are what pick a colour out of the bank the map
//! entry names: the fills and the bar use values 0–3, so one set of tiles serves both banks.
//! Colours ride the blue/yellow axis §5.6's verifier keeps: navy, white, slate, amber — and the bar's
//! fill differs from its track in luminance as well as hue, so a proportion is readable without
//! telling two hues apart.

/// The hardware tile: 8×8 pixels, 4bpp, 32 bytes (§5.0's `tile_px`).
pub const TILE_PX: u32 = 8;

/// Bytes in one 4bpp tile: 8 rows × 8 pixels × 4 bits.
pub const TILE_BYTES: usize = 32;

/// The flat fills that tile the three bands and the region's opaque backing — §5.1's "~4 tiles of
/// backdrop", which the spec's `chrome_art_tiles` comment enumerates as one per band plus the
/// scene's backing.
pub const FILL_TILES: usize = 4;
/// §5.3's control slots: `select`, `start`, `right`, `left`, `down`, one authored tile each.
pub const ICON_TILES: usize = 5;
/// §6's partial-fill patterns: every reachable fill offset inside an 8 px tile (0–7 px), plus the
/// full tile. At this variant's 1 px cells every residue is reachable, which is why it is 8 + 1.
pub const BAR_TILES: usize = 9;

/// Every tile this module authors, in tileset order.
pub const TILE_COUNT: usize = FILL_TILES + ICON_TILES + BAR_TILES;

/// The page fill: palette value 0, i.e. the backdrop. Drawn under everything else once, so a band
/// fill only has to cover its own rows.
pub const FILL_PAGE: u16 = 0;
/// The scene region's opaque backing: palette value 2, the dim slate. §5.2's `blank` row is "a flat
/// fill of the region at a declared palette entry", and this is that entry — chosen so the region
/// rectangle is visible against the page without competing with the text on it.
pub const FILL_SCENE: u16 = 1;
/// The chrome band's fill: a 50 % dither of value 2 on 0, so the band reads as a distinct strip.
pub const FILL_CHROME: u16 = 2;
/// The title band's fill: value 3 (amber) at one pixel in four, sparse enough to sit under a line of
/// text without lowering its contrast.
pub const FILL_TITLE: u16 = 3;

/// The `Select` icon: a diamond. §5.3's `Select` is the scene cycle, and a diamond is the one shape
/// that is neither an arrow nor a transport symbol.
pub const ICON_SELECT: u16 = 4;
/// The `Start`/`A` icon: a play triangle. This is the *affordance* — what the button does — and the
/// transport's own state is the word beside it (§5.6's alternative, stated in the module docs).
pub const ICON_START: u16 = 5;
/// The `Left`/`L` icon: a bar and a left-pointing triangle.
pub const ICON_PREV: u16 = 6;
/// The `Right`/`R` icon: a right-pointing triangle and a bar.
pub const ICON_NEXT: u16 = 7;
/// The `Down` icon: a return-to-start glyph (`|>` against `<|` is the pair that would be confusable
/// with prev/next, so this is a house: a roof over a body).
pub const ICON_RESTART: u16 = 8;

/// The first bar pattern: an empty column. `BAR_TILES` of them run from 0 to 8 filled pixels.
pub const BAR_FIRST: u16 = 9;

/// The palette bank §5.4 gives text, and the one the scene's backing draws in.
pub const BANK_TEXT: u8 = 0;
/// The palette bank §5.4 gives the chrome and the bar.
pub const BANK_CHROME: u8 = 1;

/// One `Rgb15` from 8-bit components, as agb packs it: 5 bits per channel, red low.
///
/// Platform-free on purpose — it is arithmetic on a `u16`, and `agb::display::Rgb15` is a tuple
/// struct over exactly this value. The ARM half converts with `Rgb15::new`, so the colours can be
/// host-tested (and are: `tests/album_surface_host.rs` reads them for contrast and distinctness).
pub const fn rgb15(r: u8, g: u8, b: u8) -> u16 {
    ((r as u16 >> 3) | ((g as u16 >> 3) << 5) | ((b as u16 >> 3) << 10)) & 0x7FFF
}

/// The backdrop: a very dark navy. Entry 0 of every bank is the transparent colour, so it is also
/// what a cell with a blank tile shows.
pub const NAVY: u16 = rgb15(8, 8, 24);
/// The glyph and icon colour.
pub const WHITE: u16 = rgb15(240, 240, 240);
/// The dim tone: the region's backing, the bar's track, and the chrome band's dither.
pub const SLATE: u16 = rgb15(40, 48, 80);
/// §5.4's accent, used sparingly: the title band's sparse fill.
pub const AMBER: u16 = rgb15(200, 168, 64);

/// §5.4's bank 0: 0 backdrop, 1 glyph, 2 dim, 3 accent — the same convention `text.rs` shipped with
/// entries 0 and 1, so a screen drawn by either renderer agrees about what entry 0 means.
pub const PALETTE_TEXT: [u16; 16] = palette(NAVY, WHITE, SLATE, AMBER);
/// §5.4's banks 1–2, same four values in the same slots. Two banks with identical contents is
/// deliberate for 2a: the bar's fill and its track are the same two colours as the glyph and the
/// dim tone, so there is one thing to look at when the palette is judged and one thing to change.
pub const PALETTE_CHROME: [u16; 16] = palette(NAVY, WHITE, SLATE, AMBER);

/// A 16-entry bank: the four declared roles, the rest the backdrop.
const fn palette(e0: u16, e1: u16, e2: u16, e3: u16) -> [u16; 16] {
    let mut out = [e0; 16];
    out[1] = e1;
    out[2] = e2;
    out[3] = e3;
    out
}

// ---------------------------------------------------------------------------------------------
// The authored drawings — one `u8` per row, one bit per pixel, so a diff shows the picture.
// ---------------------------------------------------------------------------------------------

/// `Select` — a diamond, the one non-transport shape in the row.
const SELECT_BITS: [u8; 8] = [
    0b00011000, 0b00111100, 0b01111110, 0b11111111, 0b11111111, 0b01111110, 0b00111100, 0b00011000,
];

/// `Start`/`A` — a play triangle, the affordance rather than the state (see the module docs).
const START_BITS: [u8; 8] = [
    0b00010000, 0b00011100, 0b00011111, 0b00011111, 0b00011111, 0b00011111, 0b00011100, 0b00010000,
];

/// `Left`/`L` — a bar and a left-pointing triangle: previous track.
const PREV_BITS: [u8; 8] = [
    0b11000000, 0b11000110, 0b11000111, 0b11000111, 0b11000111, 0b11000111, 0b11000110, 0b11000000,
];

/// `Right`/`R` — a right-pointing triangle and a bar: next track, the mirror of [`PREV_BITS`].
const NEXT_BITS: [u8; 8] = [
    0b00000011, 0b01100011, 0b11100011, 0b11100011, 0b11100011, 0b11100011, 0b01100011, 0b00000011,
];

/// `Down` — restart the current track: a roof over a body, because the obvious "return arrow" is an
/// arrow, and §5.6's rule is that this must not be confusable with prev/next at a glance.
const RESTART_BITS: [u8; 8] = [
    0b00011000, 0b00111100, 0b01111110, 0b00111100, 0b00111100, 0b00111100, 0b00111100, 0b00111100,
];

/// Expand one 1bpp row into four 4bpp bytes: bit 7 of `row` is the leftmost pixel, and the low
/// nibble of each output byte is the leftmost of its pair — the order `ui_probe.rs:93` states and
/// `glyphs.rs`'s bit-order leg proves for the atlas.
const fn pack_row(row: u8) -> [u8; 4] {
    let mut out = [0u8; 4];
    let mut i = 0;
    while i < 4 {
        let left = (row >> (7 - 2 * i)) & 1;
        let right = (row >> (6 - 2 * i)) & 1;
        // GBA 4bpp: the low nibble is the left pixel of the pair. Writing this the
        // other way round swaps every adjacent pair, which is invisible in the
        // tileset's distinctness check and in the ASCII composite (both read the
        // same packed bytes) but obvious on a screen; the unit test below unpacks
        // every icon row against its authored bitmap.
        out[i] = left | (right << 4);
        i += 1;
    }
    out
}

/// One authored 1bpp icon as a 32-byte 4bpp tile of value 1 (the lit entry).
const fn pack(bits: [u8; 8]) -> [u8; TILE_BYTES] {
    let mut out = [0u8; TILE_BYTES];
    let mut y = 0;
    while y < 8 {
        let row = pack_row(bits[y]);
        let mut x = 0;
        while x < 4 {
            out[y * 4 + x] = row[x];
            x += 1;
        }
        y += 1;
    }
    out
}

/// A solid tile: every pixel the same palette value. Used for the page (0) and the region's
/// backing (2).
const fn solid(value: u8) -> [u8; TILE_BYTES] {
    [(value << 4) | value; TILE_BYTES]
}

/// A 2×2 checker between two values — a 50 % dither, which reads as one tone between them. The
/// chrome band's fill: distinct from both the page and the region at the values it uses.
const fn checker(a: u8, b: u8) -> [u8; TILE_BYTES] {
    let mut out = [0u8; TILE_BYTES];
    let mut y = 0usize;
    while y < 8 {
        let mut x = 0usize;
        while x < 8 {
            let v = if (x + y) % 2 == 0 { a } else { b };
            // Left pixel of the pair is the low nibble.
            if x % 2 == 0 {
                out[y * 4 + x / 2] |= v;
            } else {
                out[y * 4 + x / 2] |= v << 4;
            }
            x += 1;
        }
        y += 1;
    }
    out
}

/// One pixel in four, on the diagonal — a sparse tone that tints a band without lowering the
/// contrast of text drawn over it.
const fn sparse(a: u8, b: u8) -> [u8; TILE_BYTES] {
    let mut out = [0u8; TILE_BYTES];
    let mut y = 0usize;
    while y < 8 {
        let mut x = 0usize;
        while x < 8 {
            let v = if x % 4 == y % 4 { a } else { b };
            if x % 2 == 0 {
                out[y * 4 + x / 2] |= v;
            } else {
                out[y * 4 + x / 2] |= v << 4;
            }
            x += 1;
        }
        y += 1;
    }
    out
}

/// One bar pattern: the leftmost `filled` pixels at value 1, the rest at value 2 (the track). `filled`
/// is clamped to 0..=8, and the nine distinct values are exactly [`BAR_TILES`].
const fn bar(filled: usize) -> [u8; TILE_BYTES] {
    let mut out = [0u8; TILE_BYTES];
    let mut y = 0usize;
    while y < 8 {
        let mut x = 0usize;
        while x < 8 {
            let v = if x < filled { 1 } else { 2 };
            if x % 2 == 0 {
                out[y * 4 + x / 2] |= v;
            } else {
                out[y * 4 + x / 2] |= v << 4;
            }
            x += 1;
        }
        y += 1;
    }
    out
}

/// Every tile this module authors, in tileset order: the four fills, the five icons, the nine bar
/// patterns (§5.1's declared terms, in the order the constants above name).
pub const TILES: [[u8; TILE_BYTES]; TILE_COUNT] = [
    solid(0),           // FILL_PAGE
    solid(2),           // FILL_SCENE
    checker(2, 0),      // FILL_CHROME
    sparse(3, 0),       // FILL_TITLE
    pack(SELECT_BITS),  // ICON_SELECT
    pack(START_BITS),   // ICON_START
    pack(PREV_BITS),    // ICON_PREV
    pack(NEXT_BITS),    // ICON_NEXT
    pack(RESTART_BITS), // ICON_RESTART
    bar(0),             // BAR_FIRST + 0
    bar(1),
    bar(2),
    bar(3),
    bar(4),
    bar(5),
    bar(6),
    bar(7),
    bar(8), // the full column
];

/// The same tiles as 4bpp words, which is the form `TileSet::new` wants and the form that is
/// guaranteed 4-byte aligned as a `static`.
pub const TILE_WORDS: usize = TILE_COUNT * 8;

const fn words() -> [u32; TILE_WORDS] {
    let mut out = [0u32; TILE_WORDS];
    let mut t = 0;
    while t < TILE_COUNT {
        let mut w = 0;
        while w < 8 {
            let b = w * 4;
            out[t * 8 + w] = u32::from_le_bytes([
                TILES[t][b],
                TILES[t][b + 1],
                TILES[t][b + 2],
                TILES[t][b + 3],
            ]);
            w += 1;
        }
        t += 1;
    }
    out
}

/// The pixels of the whole authored set, as words.
pub static CHROME_WORDS: [u32; TILE_WORDS] = words();

// ---------------------------------------------------------------------------------------------
// The control-slot map: §5.3's "slots keyed by identity, not by pixels".
// ---------------------------------------------------------------------------------------------

/// Which authored tile belongs to the control slot whose identity string is `key`.
///
/// The *positions* are not here: they are `layout::CONTROL_SLOTS`, generated from the spec's
/// `[[variant.control]]` rows, which is the other half of §5.3's split — the variant declares where a
/// control sits, this module declares what it looks like. Keying on the identity string is what makes
/// a renamed slot a build failure rather than an icon in the wrong place: [`every_slot_has_icon`]
/// walks the generated table and refuses a key with no drawing.
#[must_use]
pub const fn icon_for_slot(key: &str) -> Option<u16> {
    if streq(key, "select") {
        Some(ICON_SELECT)
    } else if streq(key, "start") {
        Some(ICON_START)
    } else if streq(key, "right") {
        Some(ICON_NEXT)
    } else if streq(key, "left") {
        Some(ICON_PREV)
    } else if streq(key, "down") {
        Some(ICON_RESTART)
    } else {
        None
    }
}

/// Byte-wise string comparison, for `const` contexts: `PartialEq` is not stable as a const trait on
/// this toolchain, and `glyphs.rs` reached the same conclusion for its cross-module face-id check.
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

/// Every control slot the variant declares has a drawing.
const fn every_slot_has_icon() -> bool {
    let mut i = 0;
    while i < crate::layout::CONTROL_SLOTS.len() {
        if !matches!(icon_for_slot(crate::layout::CONTROL_SLOTS[i].key), Some(_)) {
            return false;
        }
        i += 1;
    }
    true
}

/// The bar pattern for a column: how many of its eight pixels are filled.
#[must_use]
pub const fn bar_tile(filled_px: u32) -> u16 {
    let k = if filled_px > 8 { 8 } else { filled_px };
    BAR_FIRST + k as u16
}

// ---------------------------------------------------------------------------------------------
// The ties to the other generated module. §5.1's audit sum and §6's pattern count are *declared*
// numbers in the spec; these are the tiles they claim exist, so the two are asserted equal here —
// a build failure in the ROM build as well as on the host, which a `#[test]` can never be.
// ---------------------------------------------------------------------------------------------

const _: () = assert!(
    FILL_TILES + ICON_TILES == crate::layout::POOL_CHROME_DECLARED as usize,
    "the authored chrome tiles and §5.1's declared chrome term disagree"
);
const _: () = assert!(
    BAR_TILES == crate::layout::BAR_PATTERN_TILES as usize,
    "§6's pattern-tile count and the authored bar patterns disagree"
);
const _: () = assert!(
    ICON_TILES == crate::layout::CONTROL_SLOTS.len(),
    "one authored icon per declared control slot, or the slot table has a hole"
);
const _: () = assert!(
    every_slot_has_icon(),
    "a declared control slot has no drawing"
);
const _: () = assert!(
    TILE_COUNT * TILE_BYTES == TILE_WORDS * 4,
    "the tileset is not a whole number of 4bpp tiles"
);

// One more, and it is the one a diffuse palette would silently break: §5.6 says a state must be
// carried by more than hue, so two icons that differ must differ in shape. The four transport icons
// are pairwise different bitmaps, which is the cheap half of that rule; the `PAUSED` badge that
// carries the other half is 2b's (see the module docs).
const fn icons_are_distinct() -> bool {
    let icons = [SELECT_BITS, START_BITS, PREV_BITS, NEXT_BITS, RESTART_BITS];
    let mut i = 0;
    while i < icons.len() {
        let mut j = i + 1;
        while j < icons.len() {
            let mut same = true;
            let mut y = 0;
            while y < 8 {
                if icons[i][y] != icons[j][y] {
                    same = false;
                }
                y += 1;
            }
            if same {
                return false;
            }
            j += 1;
        }
        i += 1;
    }
    true
}
const _: () = assert!(
    icons_are_distinct(),
    "two control icons are the same drawing"
);

// ---------------------------------------------------------------------------------------------
// The ARM half: the one thing that needs agb is turning the words into a sets.
// ---------------------------------------------------------------------------------------------

#[cfg(target_arch = "arm")]
pub mod vram {
    use super::*;
    use agb::display::Palette16;
    use agb::display::Rgb15;
    use agb::display::tiled::{TileFormat, TileSet};

    /// The chrome, as one tileset. Same arrangement and same reasoning as `glyphs::vram::tile_set`:
    /// the `unsafe` lives beside the two facts it depends on — `CHROME_WORDS` is a `static [u32; N]`,
    /// so 4-byte aligned by construction, and `N` is a whole number of 32-byte tiles, which the
    /// asserts above re-check in the compiler.
    ///
    /// SAFETY: `TileSet::new` requires a 32-byte-aligned slice whose length is a multiple of the tile
    /// size (agb-0.25.0/src/display/tiled/vram_manager.rs:58-68).
    #[must_use]
    pub fn tile_set() -> TileSet {
        let bytes: &[u8] = unsafe {
            core::slice::from_raw_parts(CHROME_WORDS.as_ptr().cast::<u8>(), TILE_WORDS * 4)
        };
        unsafe { TileSet::new(bytes, TileFormat::FourBpp) }
    }

    /// A palette bank from this module's platform-free `u16` table.
    #[must_use]
    pub fn palette(bank: &[u16; 16]) -> Palette16 {
        let mut colours = [Rgb15::BLACK; 16];
        let mut i = 0;
        while i < 16 {
            colours[i] = Rgb15::new(bank[i]);
            i += 1;
        }
        Palette16::new(colours)
    }
}

// ---------------------------------------------------------------------------------------------
// The packing rule and its witness. These are unit tests rather than host-gate legs because
// they read the authored bitmaps directly; the layout gate also checks one known row of
// `TILES` so a change to the packing cannot reach a ROM without a gate seeing it.
// ---------------------------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const ICONS: [([u8; 8], u16); 5] = [
        (SELECT_BITS, ICON_SELECT),
        (START_BITS, ICON_START),
        (PREV_BITS, ICON_PREV),
        (NEXT_BITS, ICON_NEXT),
        (RESTART_BITS, ICON_RESTART),
    ];

    /// Unpack a 4bpp tile row the way the hardware does: low nibble is the left pixel.
    fn unpack(tile: u16, y: usize) -> u8 {
        let mut row = 0u8;
        for x in 0..8 {
            let byte = TILES[tile as usize][y * 4 + x / 2];
            let value = if x % 2 == 0 { byte & 0x0F } else { byte >> 4 };
            row |= value << (7 - x);
        }
        row
    }

    #[test]
    fn every_icon_row_packs_low_nibble_left() {
        for (bits, tile) in ICONS {
            for y in 0..8 {
                assert_eq!(
                    unpack(tile, y),
                    bits[y],
                    "icon tile {tile} row {y} does not round-trip through the packed bytes"
                );
            }
        }
    }

    #[test]
    fn pack_row_puts_the_left_pixel_in_the_low_nibble() {
        // SELECT row 0 is `0b00011000`: pixels 3 and 4 lit.
        assert_eq!(pack_row(0b00011000), [0x00, 0x10, 0x01, 0x00]);
        // SELECT row 2 is `0b01111110`, whose first and last pairs are asymmetric.
        assert_eq!(pack_row(0b01111110), [0x10, 0x11, 0x11, 0x01]);
    }
}
