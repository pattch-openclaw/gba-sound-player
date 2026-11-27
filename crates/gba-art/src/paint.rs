//! Write a parsed `ART1` into a scene rectangle.
//!
//! The image's map entries name a tile index and a bank-relative palette; the caller supplies
//! the **base palette bank** the application loaded the ART1 palettes into (variant B declares
//! art banks 3..6, §5.4). The tile index is the ART1's own index into its tile section, which is
//! the tileset the application built from the same slice.
//!
//! The rectangle is the enforcement: cells outside [`Rect`] are never pushed, so an art box that
//! overruns its region cannot happen at run time even if a spec were wrong.

use gba_scene::{Rect, Source, Write, Writes};

use crate::art1::Art1;

/// Paint `art` into `rect`, using `base_bank` as palette bank 0 of the image.
///
/// Returns the number of cells pushed. A cell whose map entry names a tile or bank outside the
/// blob is an `ART1` validation failure and cannot reach here (`art1::parse` refuses it).
pub fn paint(art: &Art1<'_>, rect: Rect, base_bank: u8, writes: &mut Writes) -> usize {
    paint_at(art, 0, 0, rect, base_bank, writes)
}

/// Paint a crop of `art` starting at `(src_col, src_row)` into `rect`.
///
/// The card's picture is a centre crop of the album's one ART1, so the scene crate needs a
/// source offset rather than a second image (2b PR-(4b)).
pub fn paint_at(
    art: &Art1<'_>,
    src_col: u32,
    src_row: u32,
    rect: Rect,
    base_bank: u8,
    writes: &mut Writes,
) -> usize {
    let cols = (art.tile_cols - src_col).min(rect.cols);
    let rows = (art.tile_rows - src_row).min(rect.rows);
    let mut pushed = 0;
    let mut row = 0;
    while row < rows {
        let mut col = 0;
        while col < cols {
            let entry = art.map_entry((src_row + row) * art.tile_cols + src_col + col);
            writes.push(Write {
                x: (rect.x + col) as u8,
                y: (rect.y + row) as u8,
                tile: entry & 0x03FF,
                bank: base_bank + ((entry >> 12) & 0x0F) as u8,
                source: Source::Art,
            });
            pushed += 1;
            col += 1;
        }
        row += 1;
    }
    pushed
}
