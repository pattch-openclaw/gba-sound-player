//! A host renderer for the surface — the picture the gate's assertions are about, and the one a
//! person can look at without a cart.
//!
//! **Why this exists.** §5.6 makes the legibility witness a person, and until 2a PR-(3) the only way
//! to see the screen was to flash a ROM. That is too late and too slow: a mis-addressed row is
//! invisible in a log and obvious in a picture. The first hardware run of this phase found exactly
//! such a bug — every glyph lost its bottom tile row — and this module exists so the next one is
//! found in a host run instead.
//!
//! **What it is not.** It is not a second renderer in §5.5's sense: it does not rasterise an outline,
//! and it does not decide anything. It takes the *same* `Write` list the ROM's map gets, the *same*
//! tile bytes the ROM's two tilesets upload, and the *same* palette table, and expands them. If it
//! disagrees with the ROM, the bug is in the expansion — which is why the gate pins a few pixels by
//! value rather than only comparing pictures to themselves.
//!
//! Host-only (`#[cfg(not(target_arch = "arm"))]`): it allocates, and the ROM never needs it.

use crate::chrome;
use crate::glyphs;
use crate::layout;
use crate::surface::{Source, Write, Writes};

/// Words per 4bpp tile in the atlas. `glyphs.rs` asserts `GLYPHS.len() == ATLAS_TILES * 8`, so this
/// is a fact the generated module already guarantees rather than a number this file chose.
const GLYPH_WORDS_PER_TILE: usize = 8;

/// The three layers the ROM shows: the scene region, the chrome/title art, and the foreground
/// (glyphs, icons and bar). Bottom to top, matching the BG numbers in `surface::Painted`.
pub const LAYER_SCENE: usize = 0;
pub const LAYER_CHROME: usize = 1;
pub const LAYER_TEXT: usize = 2;
pub const LAYERS: usize = 3;

/// A 30×20 stack of what a frame left on screen. `None` is an untouched cell; a transparent pixel
/// in a top layer falls through to the layer below, exactly as a GBA background does.
pub struct Canvas {
    cells: [[[Option<Write>; layout::SCREEN_COLS as usize]; layout::SCREEN_ROWS as usize]; LAYERS],
}

impl Canvas {
    #[must_use]
    pub fn new() -> Self {
        Self {
            cells: [[[None; layout::SCREEN_COLS as usize]; layout::SCREEN_ROWS as usize]; LAYERS],
        }
    }

    /// Apply a batch of writes to one layer, exactly as the ROM's map for that background would take
    /// them. Later writes win within a layer; layers are composited by [`pixel`](Self::pixel).
    pub fn apply_layer(&mut self, layer: usize, writes: &Writes) {
        assert!(layer < LAYERS, "layer {layer} is outside the three shown");
        for w in writes.as_slice() {
            self.cells[layer][w.y as usize][w.x as usize] = Some(*w);
        }
    }

    /// The topmost write in a cell whose tile is not the transparent page fill. A padding cell on
    /// the text layer therefore reads as the chrome fill beneath it, which is the picture the
    /// layered ROM shows.
    fn cell_at(&self, y: usize, x: usize) -> Option<&Write> {
        for layer in (0..LAYERS).rev() {
            if let Some(w) = &self.cells[layer][y][x] {
                // The page fill is a *chrome* tile 0; a glyph tile 0 is a different tileset and
                // must not be mistaken for transparency. Same rule as `tile_char`.
                let page = w.source == Source::Chrome && w.tile == chrome::FILL_PAGE;
                if !page && w.tile != 0xFFFF {
                    return Some(w);
                }
            }
        }
        None
    }

    /// The map as characters, one per cell: fills as rules, icons by initial, the bar by fill, and a
    /// glyph as the character it draws. A cell that draws a glyph's second tile row shows the same
    /// character, because that is what a 16 px line box over an 8 px cell looks like.
    #[must_use]
    pub fn ascii(&self) -> Vec<String> {
        (0..layout::SCREEN_ROWS as usize)
            .map(|y| {
                (0..layout::SCREEN_COLS as usize)
                    .map(|x| cell_char(self.cell_at(y, x)))
                    .collect()
            })
            .collect()
    }

    /// The frame as RGBA, nearest-neighbour scaled by `scale`.
    ///
    /// Pixel values are the atlas's own 4bpp indices (0..=3) looked up in the palette bank the map
    /// entry names — so a colour bug shows up here in the same place it would show up on hardware.
    #[must_use]
    pub fn rgba(&self, scale: u32) -> Vec<u8> {
        let w = layout::SCREEN_COLS * chrome::TILE_PX;
        let h = layout::SCREEN_ROWS * chrome::TILE_PX;
        let mut out = Vec::with_capacity((w * scale * h * scale * 4) as usize);
        for py in 0..h {
            for _ in 0..scale {
                for px in 0..w {
                    let value = self.pixel(px, py);
                    let rgb = palette_rgb(value.1, value.0);
                    for _ in 0..scale {
                        out.extend_from_slice(&rgb);
                        out.push(0xFF);
                    }
                }
            }
        }
        out
    }

    /// The colour index and bank at a pixel: the topmost layer whose pixel is not transparent, or
    /// the backdrop. This is the composition the hardware performs, per pixel.
    #[must_use]
    pub fn pixel(&self, px: u32, py: u32) -> (u8, u8) {
        let cell_x = (px / chrome::TILE_PX) as usize;
        let cell_y = (py / chrome::TILE_PX) as usize;
        let tx = px % chrome::TILE_PX;
        let ty = py % chrome::TILE_PX;
        for layer in (0..LAYERS).rev() {
            let Some(w) = &self.cells[layer][cell_y][cell_x] else {
                continue;
            };
            let page = w.source == Source::Chrome && w.tile == chrome::FILL_PAGE;
            if w.tile == 0xFFFF || page {
                continue;
            }
            let value = tile_pixel(w.source, w.tile, tx, ty);
            if value != 0 {
                return (value, w.bank);
            }
        }
        (0, chrome::BANK_TEXT)
    }

    /// Write the frame as a PNG at `scale`, plus a 30×20 ASCII companion beside it.
    pub fn save(&self, path: &str, scale: u32) -> std::io::Result<()> {
        let w = layout::SCREEN_COLS * chrome::TILE_PX * scale;
        let h = layout::SCREEN_ROWS * chrome::TILE_PX * scale;
        std::fs::write(path, png(w, h, &self.rgba(scale)))?;
        let ascii: String = self
            .ascii()
            .iter()
            .enumerate()
            .map(|(y, row)| format!("{y:02}|{row}|\n"))
            .collect();
        std::fs::write(format!("{path}.txt"), ascii)
    }
}

impl Default for Canvas {
    fn default() -> Self {
        Self::new()
    }
}

/// One character per cell, chosen so every drawn thing is distinguishable at a glance.
#[must_use]
pub fn cell_char(cell: Option<&Write>) -> char {
    let Some(w) = cell else {
        return '.';
    };
    if w.tile == 0xFFFF {
        return '.';
    }
    match w.source {
        Source::Glyph => glyph_char(w.tile),
        Source::Chrome => match w.tile {
            chrome::FILL_PAGE => ' ',
            chrome::FILL_SCENE => '_',
            chrome::FILL_CHROME => '=',
            chrome::FILL_TITLE => '-',
            chrome::ICON_SELECT => 'S',
            chrome::ICON_START => 'P',
            chrome::ICON_PREV => '<',
            chrome::ICON_NEXT => '>',
            chrome::ICON_RESTART => 'R',
            t if (chrome::BAR_FIRST..chrome::BAR_FIRST + chrome::BAR_TILES as u16).contains(&t) => {
                match t - chrome::BAR_FIRST {
                    0 => ':',
                    8 => '#',
                    _ => '|',
                }
            }
            _ => '?',
        },
    }
}

/// The character a glyph tile draws, recovered from the atlas's own slot table — so the picture is
/// checkable against the string the surface was given rather than against a copy of it.
#[must_use]
fn glyph_char(tile: u16) -> char {
    let rows = glyphs::TILE_ROWS_PER_LINE as u16;
    for code in 0x20u32..=0x7e {
        let c = char::from_u32(code).unwrap();
        if let Some(slot) = glyphs::slot(c) {
            if slot.tile0 == glyphs::BLANK_TILE {
                continue;
            }
            if tile >= slot.tile0 && tile < slot.tile0 + rows {
                return c;
            }
        }
    }
    '?'
}

/// One pixel of a tile, as its 4bpp colour index. `tx`/`ty` are 0..8.
#[must_use]
fn tile_pixel(source: Source, tile: u16, tx: u32, ty: u32) -> u8 {
    match source {
        Source::Chrome => {
            // Authored tiles: 8 rows × 4 bytes, low nibble = the left pixel of the pair
            // (`ui_probe.rs:93`'s order, and `chrome::pack_row` writes it that way).
            let bytes = &chrome::TILES[tile as usize];
            let byte = bytes[(ty * 4 + tx / 2) as usize];
            if tx % 2 == 0 { byte & 0x0F } else { byte >> 4 }
        }
        Source::Glyph => {
            // The atlas: one `u32` per row, pixel `x` at bit `4x`, low nibble first — asserted by
            // the generated module's own bit-order leg.
            let at = (tile as usize) * GLYPH_WORDS_PER_TILE + ty as usize;
            let word = glyphs::GLYPHS[at];
            ((word >> (4 * tx)) & 0x0F) as u8
        }
    }
}

/// A palette entry, expanded from the GBA's 5-bit-per-channel `Rgb15` to 8-bit RGB: replicate the top
/// bits into the low ones so `0x1F` becomes `255` rather than `248`.
#[must_use]
pub fn palette_rgb(bank: u8, value: u8) -> [u8; 3] {
    let table = if bank == chrome::BANK_CHROME {
        &chrome::PALETTE_CHROME
    } else {
        &chrome::PALETTE_TEXT
    };
    let raw = table[value as usize & 0x0F];
    let expand = |v: u16| -> u8 {
        let five = (v & 0x1F) as u8;
        (five << 3) | (five >> 2)
    };
    [expand(raw), expand(raw >> 5), expand(raw >> 10)]
}

// ---------------------------------------------------------------------------------------------
// PNG, written without a dependency: uncompressed deflate, which is a legal zlib stream and keeps
// this file's output byte-identical between runs (a compressed encoding would too, but only for a
// fixed deflate implementation, and a dependency is not worth it for a preview).
// ---------------------------------------------------------------------------------------------

/// A PNG containing `rgba` (8-bit, colour type 6) at `width`×`height`.
#[must_use]
pub fn png(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    assert_eq!(
        rgba.len() as u32,
        width * height * 4,
        "the pixel buffer is not w*h*4"
    );
    let mut out = Vec::new();
    out.extend_from_slice(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);

    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]); // 8-bit, RGBA, deflate, adaptive filter, no interlace
    chunk(&mut out, b"IHDR", &ihdr);

    // Raw scanlines: a filter byte (0 = none) then the row.
    let mut raw = Vec::with_capacity(rgba.len() + height as usize);
    for y in 0..height as usize {
        raw.push(0);
        raw.extend_from_slice(&rgba[y * width as usize * 4..(y + 1) * width as usize * 4]);
    }
    chunk(&mut out, b"IDAT", &zlib_stored(&raw));
    chunk(&mut out, b"IEND", &[]);
    out
}

/// A zlib stream whose deflate blocks are *stored* (BTYPE=00), so no compressor is needed.
fn zlib_stored(data: &[u8]) -> Vec<u8> {
    let mut out = vec![0x78, 0x01]; // deflate, 32 KiB window, no dictionary, fastest
    let mut i = 0;
    while i < data.len() {
        let n = (data.len() - i).min(0xFFFF);
        let last = i + n == data.len();
        out.push(if last { 1 } else { 0 }); // BFINAL, BTYPE=00
        out.extend_from_slice(&(n as u16).to_le_bytes());
        out.extend_from_slice(&(!(n as u16)).to_le_bytes());
        out.extend_from_slice(&data[i..i + n]);
        i += n;
    }
    if data.is_empty() {
        out.extend_from_slice(&[1, 0, 0, 0xFF, 0xFF]);
    }
    out.extend_from_slice(&adler32(data).to_be_bytes());
    out
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], body: &[u8]) {
    out.extend_from_slice(&(body.len() as u32).to_be_bytes());
    let mut crc_input = Vec::with_capacity(4 + body.len());
    crc_input.extend_from_slice(kind);
    crc_input.extend_from_slice(body);
    out.extend_from_slice(kind);
    out.extend_from_slice(body);
    out.extend_from_slice(&crc32(&crc_input).to_be_bytes());
}

/// CRC-32 (the PNG/zlib polynomial, reflected), table-free for clarity.
#[must_use]
pub fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc ^= u32::from(b);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

/// Adler-32, the zlib trailer.
#[must_use]
pub fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &byte in data {
        a = (a + u32::from(byte)) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}
