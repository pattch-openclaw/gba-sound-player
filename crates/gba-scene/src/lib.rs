//! `gba-scene` — the vocabulary a scene writes through (2b PR-(4), ALBUM-PLAYER.md §5.0/§5.2).
//!
//! A scene gets a **rectangle** and a **bounded batch of cells**, and nothing else. That is
//! the whole interface: no agb types, no palettes, no tilesets, no geometry decisions, no
//! allocator. The application owns the backgrounds, the tilesets and the commit; a scene
//! crate owns what it draws into its rectangle.
//!
//! **What is deliberately absent.** No `Scene` trait, no registry, no runtime geometry.
//! §16's "no general GUI toolkit" and §11's rule that a scene kind must be statically
//! derivable are why: the registry stays a compile-time table in the application, and the
//! bake audits declared geometry. A trait object here would make a scene's cells
//! unauditable and turn the budget into a hope.
//!
//! The types are platform-free so the host gate can run the same write plan the ROM runs;
//! `Source` names which tileset an index refers to because two tilesets share one 10-bit
//! index space (§5.5a's atlas and §5.3's authored chrome), and a tile number alone cannot
//! say what it draws.
#![cfg_attr(not(test), no_std)]
#![cfg_attr(not(test), forbid(unsafe_code))]
#[cfg(test)]
extern crate std;

/// Visible screen columns: the hardware's 240 px / 8 px tile.
pub const SCREEN_COLS: u32 = 30;
/// Visible screen rows: the hardware's 160 px / 8 px tile.
pub const SCREEN_ROWS: u32 = 20;
/// The most cells one batch carries: the whole visible screen, because a page fill must
/// cover every cell. `Writes::push` reports what did not fit; a dropped write is a spec
/// that outgrew the batch, never a partial draw that looks complete.
pub const MAX_WRITES: usize = (SCREEN_COLS * SCREEN_ROWS) as usize;

/// A rectangle of map cells a scene owns (§5.0's "the rectangle is the enforcement").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    /// First visible column.
    pub x: u32,
    /// First visible row.
    pub y: u32,
    /// Columns the scene may write.
    pub cols: u32,
    /// Rows the scene may write.
    pub rows: u32,
}

impl Rect {
    /// A rectangle, in tiles.
    #[must_use]
    pub const fn new(x: u32, y: u32, cols: u32, rows: u32) -> Self {
        Self { x, y, cols, rows }
    }

    /// Whether `(x, y)` is inside this rectangle.
    #[must_use]
    pub const fn contains(&self, x: u32, y: u32) -> bool {
        x >= self.x && x < self.x + self.cols && y >= self.y && y < self.y + self.rows
    }
}

/// One cell's new content: where it goes, which tile, and which palette bank the map entry
/// names.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Write {
    /// Visible column.
    pub x: u8,
    /// Visible row.
    pub y: u8,
    /// Tile index in the tileset [`Write::source`] names.
    pub tile: u16,
    /// Palette bank the map entry names.
    pub bank: u8,
    /// Which tileset the index refers to; not optional and not inferred.
    pub source: Source,
}

/// The tilesets a surface draws from. Adding one is a compile-time change here, which is
/// the point: a scene cannot invent a tileset the application has not loaded.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Source {
    /// The authored chrome/bar tileset (§5.3, §6).
    Chrome,
    /// The static glyph atlas (§5.5a).
    Glyph,
    /// The album art tileset (§7).
    Art,
}

/// A bounded batch of [`Write`]s, in map coordinates. Fixed capacity: the ROM has no
/// allocator on this path, and the bound is the full visible screen because a page fill
/// must cover it.
pub struct Writes {
    buf: [Write; MAX_WRITES],
    len: usize,
    dropped: usize,
}

impl Writes {
    /// An empty batch.
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

    /// Forget the batch and its overflow count. Capacity is fixed, so this is the only
    /// reuse path; resetting `dropped` makes the count a property of the batch about to be
    /// inspected.
    pub fn clear(&mut self) {
        self.len = 0;
        self.dropped = 0;
    }

    /// Cells in the batch.
    #[must_use]
    pub fn len(&self) -> usize {
        self.len
    }

    /// Whether the batch is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Writes that did not fit. A non-zero count is a spec that outgrew the batch; the host
    /// gate refuses it by name rather than letting a partial draw look complete.
    #[must_use]
    pub fn dropped(&self) -> usize {
        self.dropped
    }

    /// The writes in the batch.
    #[must_use]
    pub fn as_slice(&self) -> &[Write] {
        &self.buf[..self.len]
    }

    /// Append one write, returning whether it fit. A caller that keeps a shadow of the map
    /// must update that shadow **only on `true`**: a dropped write means the map did not
    /// change, so recording it would make the next frame's diff skip the cell forever.
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

    /// Fill a rectangle with one chrome tile (band fills and the `blank` scene).
    pub fn fill(&mut self, x0: u32, y0: u32, cols: u32, rows: u32, tile: u16, bank: u8) {
        self.fill_with(x0, y0, cols, rows, tile, bank, Source::Chrome);
    }

    /// Fill a rectangle with one tile from a named tileset.
    pub fn fill_with(
        &mut self,
        x0: u32,
        y0: u32,
        cols: u32,
        rows: u32,
        tile: u16,
        bank: u8,
        source: Source,
    ) {
        let mut row = 0;
        while row < rows {
            let mut col = 0;
            while col < cols {
                self.push(Write {
                    x: (x0 + col) as u8,
                    y: (y0 + row) as u8,
                    tile,
                    bank,
                    source,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_batch_reports_what_it_dropped_and_clear_resets_it() {
        let mut writes = Writes::new();
        assert!(writes.is_empty());
        for i in 0..MAX_WRITES {
            assert!(writes.push(Write {
                x: (i % SCREEN_COLS as usize) as u8,
                y: (i / SCREEN_COLS as usize) as u8,
                tile: 1,
                bank: 0,
                source: Source::Chrome,
            }));
        }
        assert_eq!(writes.len(), MAX_WRITES);
        assert!(!writes.push(Write {
            x: 0,
            y: 0,
            tile: 1,
            bank: 0,
            source: Source::Chrome
        }));
        assert_eq!(writes.dropped(), 1);
        writes.clear();
        assert!(writes.is_empty());
        assert_eq!(writes.dropped(), 0);
    }

    #[test]
    fn a_rect_is_its_cells_and_nothing_else() {
        let rect = Rect::new(11, 2, 19, 2);
        assert!(rect.contains(11, 2) && rect.contains(29, 3));
        assert!(!rect.contains(10, 2) && !rect.contains(11, 4));
    }

    #[test]
    fn fill_names_the_tileset_it_means() {
        let mut writes = Writes::new();
        writes.fill_with(0, 0, 2, 1, 7, 3, Source::Art);
        assert_eq!(writes.len(), 2);
        assert!(
            writes
                .as_slice()
                .iter()
                .all(|w| w.source == Source::Art && w.bank == 3)
        );
    }
}
