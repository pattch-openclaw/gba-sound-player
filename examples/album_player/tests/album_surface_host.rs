//! The surface's host gate (2a PR-(3), §5.2/§5.3/§10.2).
//!
//! Everything here reads the **platform-free** half of `chrome.rs` and `surface.rs`: the authored
//! tiles, the map writes a frame produces, and the geometry those writes land on. That is the part
//! that decides what the screen says, so it is the part a gate can hold. The agb half (two
//! backgrounds, one commit) is compiled by `make album-rom` and judged by a human looking at the
//! screen — §5.6's own rule is that the legibility witness is a person, not a snapshot.
//!
//! What this file deliberately does **not** do: compare any pixels against any renderer. §5.5a's
//! argument applies here too — a second renderer that agreed with the first would prove the two
//! implementations match, not that either is right. The legs below assert *geometry* (which cells,
//! which tiles, how many), and AP-4/AP-7a own art and hardware.

use album_player::{chrome, geometry, glyphs, layout, surface};

/// A 30×20 picture built by applying batches of writes in order, the way the two backgrounds plus a
/// frame compose on the device. `.` is an unwritten cell, which draws the band fill beneath it.
struct Canvas {
    grid: Vec<Vec<char>>,
}

impl Canvas {
    fn new() -> Self {
        Self {
            grid: vec![vec!['.'; layout::SCREEN_COLS as usize]; layout::SCREEN_ROWS as usize],
        }
    }

    fn apply(&mut self, writes: &surface::Writes) {
        for w in writes.as_slice() {
            self.grid[w.y as usize][w.x as usize] = tile_char(w.tile);
        }
    }

    fn row(&self, y: u32) -> String {
        self.grid[y as usize].iter().collect()
    }

    fn print(&self, label: &str) {
        println!("--- {label} ---");
        for (y, row) in self.grid.iter().enumerate() {
            println!("{y:02}|{}|", row.iter().collect::<String>());
        }
    }
}

/// One character per tile, chosen so the bands, the icons, the bar and the glyphs are all
/// distinguishable at a glance. A glyph shows the character itself, which is the point: this is how a
/// reviewer checks that the title band holds the title and not, say, the clock.
fn tile_char(tile: u16) -> char {
    match tile {
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
        t => glyph_char(t),
    }
}

/// The character a glyph tile draws, recovered from the atlas's own slot table, so the composite is
/// checkable against the string the surface was given rather than against a copy of it.
fn glyph_char(tile: u16) -> char {
    if tile == glyphs::BLANK_TILE {
        return '.';
    }
    for code in 0x20u32..=0x7e {
        let c = char::from_u32(code).unwrap();
        if let Some(slot) = glyphs::slot(c) {
            if slot.tile0 == glyphs::BLANK_TILE {
                continue;
            }
            // Both rows of a glyph answer the character, so a line renders as the same text on its
            // two tile rows — which is what a 16 px line box over an 8 px cell looks like, and what
            // makes "the bottom half went missing" a visible difference rather than an assumed one.
            let rows = glyphs::TILE_ROWS_PER_LINE as u16;
            if tile >= slot.tile0 && tile < slot.tile0 + rows {
                return c;
            }
        }
    }
    '?'
}

fn view<'a>(
    title: &'a str,
    elapsed: u32,
    total: u32,
    bar_cell: u32,
    word: &'a str,
) -> surface::View<'a> {
    surface::View {
        title,
        elapsed_samples: elapsed,
        total_samples: total,
        bar_cell,
        state_word: word,
    }
}

/// The fixture album's length in samples: 576 s at 16,384 Hz, which is the `9:36` the clock prints.
const TOTAL: u32 = 576 * geometry::RATE_HZ;

/// The two static layers, in the order `Painted::new` composes them: the scene's backing into the
/// scene background, then the bands and the icons into the surface's.
fn paint_static(ui: &mut surface::Surface, canvas: &mut Canvas) {
    let mut writes = surface::Writes::new();
    surface::Surface::paint_blank(&mut writes);
    canvas.apply(&writes);
    ui.paint_static(&mut writes);
    canvas.apply(&writes);
}

/// The boot frame: the band stack, the region, the five icons, the first title, `0:00` and a bar at
/// its first cell. This is the composite a reviewer can read and the one a human validates on the
/// ROM, so it is checked here first, in the same cells.
#[test]
fn the_boot_frame_is_the_declared_band_stack_with_a_title_in_it() {
    let mut ui = surface::Surface::new();
    let mut canvas = Canvas::new();
    let mut writes = surface::Writes::new();

    surface::Surface::paint_blank(&mut writes);
    assert_eq!(
        writes.len(),
        480,
        "the region is 30 x 16 cells and nothing else"
    );
    ui.paint_static(&mut writes);
    assert_eq!(
        writes.len(),
        120 + 5,
        "two always-shown bands of 60 cells, plus five icons"
    );
    assert_eq!(writes.dropped(), 0);

    paint_static(&mut ui, &mut canvas);
    assert!(ui.update(&view("Bioluminescent", 0, TOTAL, 0, "PLAYING"), &mut writes));
    canvas.apply(&writes);

    // The bands: the region on top, the title band with a title and a bar, the chrome beneath.
    assert_eq!(
        canvas.row(0),
        "_".repeat(30),
        "the region is not flat-filled"
    );
    assert_eq!(canvas.row(layout::REGION.rows - 1), "_".repeat(30));
    // The title band is two rows: the text on the upper, §6's bar along the lower (§5.0's B row).
    // Both rows are asserted in full, because the bug this leg was tightened after drew row 0
    // correctly and row 1 from the wrong offset — and a substring assertion could not see it.
    let title_row = layout::BANDS[1].screen_row;
    // Built rather than typed: a hand-counted row of thirty characters is how a test ends up
    // asserting its own arithmetic instead of the screen's. `TITLE_TEXT` is 20 cells, `BAR` 10.
    let title_cells = (layout::ELEMENTS[surface::TITLE].w / chrome::TILE_PX) as usize;
    let bar_cells = (layout::ELEMENTS[surface::BAR].w / chrome::TILE_PX) as usize;
    let title = "Bioluminescent";
    assert_eq!(
        canvas.row(title_row),
        format!(
            "{title}{}{}",
            "-".repeat(title_cells - title.len()),
            "-".repeat(bar_cells)
        ),
        "the title band's upper row is not the title then its fill"
    );
    assert_eq!(
        canvas.row(title_row + 1),
        format!(
            "{title}{}{}",
            "-".repeat(title_cells - title.len()),
            ":".repeat(bar_cells)
        ),
        "the title's lower tile row is missing, or the bar is not the band's last ten cells"
    );
    // The chrome band, two rows down: the clock, the state word, then five icons in their slots.
    let chrome_row = canvas.row(layout::BANDS[0].screen_row);
    // The clock is nine characters in a twelve-cell box: the cells it does not use keep the band's
    // own fill rather than being blanked to the backdrop, which is what the `===` run is.
    assert_eq!(
        &chrome_row[..9],
        "0:00/9:36",
        "the clock is not at the left of its box"
    );
    assert_eq!(
        &chrome_row[9..12],
        "===",
        "the clock's spare cells did not keep the band fill"
    );
    assert_eq!(
        &chrome_row[12..19],
        "PLAYING",
        "the state word is not beside the clock"
    );
    assert_eq!(
        &chrome_row[19..21],
        "==",
        "the word's spare cells did not keep the band fill"
    );
    assert_eq!(
        &chrome_row[21..26],
        "SP><R",
        "the five authored icons are not in their slots"
    );
    assert_eq!(
        &chrome_row[26..],
        "====",
        "the chrome band's tail is not its own fill"
    );
    // And the chrome's second tile row: the same text, and the icons' columns are band fill because
    // an icon is one tile tall while a line box is two.
    assert_eq!(
        canvas.row(layout::BANDS[0].screen_row + 1),
        format!("0:00/9:36===PLAYING=={}", "=".repeat(9)),
        "the chrome band's lower row is not the glyphs' second tile row then fill"
    );

    canvas.print("boot frame");
}

/// §5.1's rule 2, as a number: a frame that changed nothing writes nothing; a track change writes one
/// element; a state change writes one band.
#[test]
fn a_still_frame_writes_nothing_and_a_track_change_writes_one_element() {
    let mut ui = surface::Surface::new();
    let mut writes = surface::Writes::new();
    let mut canvas = Canvas::new();
    paint_static(&mut ui, &mut canvas);

    assert!(ui.update(&view("Bioluminescent", 0, TOTAL, 0, "PLAYING"), &mut writes));
    assert!(
        !ui.update(&view("Bioluminescent", 0, TOTAL, 0, "PLAYING"), &mut writes),
        "a frame that changed nothing asked for {} writes",
        writes.len()
    );

    // A different title: only the cells whose glyph differs, and never more than the element's 40.
    assert!(ui.update(
        &view("The Lengthening Day", 0, TOTAL, 0, "PLAYING"),
        &mut writes
    ));
    assert!(
        writes.len() <= 40,
        "a title change wrote {} cells, more than `title_text` has",
        writes.len()
    );

    // A paused transport: the state word's cells, in the chrome band only.
    assert!(ui.update(
        &view("The Lengthening Day", 0, TOTAL, 0, "PAUSED"),
        &mut writes
    ));
    let label_row = (layout::ELEMENTS[surface::KEYLABELS].y / 8) as u8;
    for w in writes.as_slice() {
        assert!(
            w.y == label_row || w.y == label_row + 1,
            "the state word wrote outside its element's two tile rows"
        );
    }
}

/// The clock advances one second at a time — no divide per frame, and no redraw until a displayed
/// digit changes (§5.2's cadence rule for the clock).
#[test]
fn the_clock_bumps_on_whole_seconds_and_never_repaints_between_them() {
    let mut ui = surface::Surface::new();
    let mut writes = surface::Writes::new();
    let rate = geometry::RATE_HZ;

    ui.update(&view("T", 0, TOTAL, 0, "PLAYING"), &mut writes);
    assert_eq!(ui.shown_seconds(), 0);

    // Half a second in: nothing changes.
    assert!(
        !ui.update(&view("T", rate / 2, TOTAL, 0, "PLAYING"), &mut writes),
        "a sub-second step repainted"
    );
    assert_eq!(ui.shown_seconds(), 0);

    // One second in: at most the two digit cells that changed (§10.2 prices this at 2-4 tiles).
    assert!(ui.update(&view("T", rate, TOTAL, 0, "PLAYING"), &mut writes));
    assert_eq!(ui.shown_seconds(), 1);
    assert!(writes.len() <= 4, "one second cost {} cells", writes.len());

    // A jump lands on the right second rather than counting frames: 12 s in one step reads 0:12.
    ui.update(&view("T", 12 * rate, TOTAL, 0, "PLAYING"), &mut writes);
    assert_eq!(ui.shown_seconds(), 12);
}

/// A new visit restarts the clock. §4.4's "0 at a track start" is what makes a skip's title band and
/// its clock agree, and a subtraction of samples would show it going backwards.
#[test]
fn a_new_visit_restarts_the_clock() {
    let mut ui = surface::Surface::new();
    let mut writes = surface::Writes::new();
    let mut canvas = Canvas::new();
    paint_static(&mut ui, &mut canvas);
    let rate = geometry::RATE_HZ;

    ui.update(&view("T", 30 * rate, TOTAL, 30, "PLAYING"), &mut writes);
    canvas.apply(&writes);
    assert_eq!(ui.shown_seconds(), 30);
    assert!(
        canvas
            .row(layout::BANDS[0].screen_row)
            .starts_with("0:30/9:36")
    );

    // A skip: the visit's sample count restarts at 0 rather than continuing.
    ui.update(&view("T", 0, TOTAL, 0, "PLAYING"), &mut writes);
    canvas.apply(&writes);
    assert_eq!(ui.shown_seconds(), 0);
    let chrome_row = canvas.row(layout::BANDS[0].screen_row);
    assert!(
        chrome_row.starts_with("0:00/9:36"),
        "the clock did not restart: {chrome_row:?}"
    );
}

/// §6's bar: the filled pixel boundary is the cell count, the column it lands in is `cell / 8`, and a
/// full bar is exactly the element's width. Every fill position is walked, because the bar is the one
/// element whose every frame is a different picture.
#[test]
fn the_bar_fills_left_to_right_and_stops_at_its_box() {
    let mut ui = surface::Surface::new();
    let mut writes = surface::Writes::new();
    let bar = &layout::ELEMENTS[surface::BAR];
    let bar_col = bar.x / chrome::TILE_PX;
    let bar_row = (bar.y / chrome::TILE_PX) as u8;
    let cols = (bar.w / chrome::TILE_PX) as usize;

    // The bar's drawn state across frames: a frame writes only the columns that changed, so the
    // assertion has to be about what the *map* holds, not about one batch.
    let mut per_column = [0u32; surface::MAX_BAR_COLS];
    for cell in 0..=layout::BAR_CELLS {
        ui.update(&view("T", 0, TOTAL, cell, "PLAYING"), &mut writes);
        for w in writes.as_slice() {
            if w.y == bar_row && w.x as u32 >= bar_col {
                per_column[(w.x as u32 - bar_col) as usize] = (w.tile - chrome::BAR_FIRST) as u32;
            }
        }
        if cell == 0 {
            assert_eq!(
                per_column[..cols].iter().sum::<u32>(),
                0,
                "the first frame wrote fill into an empty bar"
            );
            continue;
        }
        let boundary = (cell * layout::BAR_CELL_PX) / chrome::TILE_PX;
        for (i, filled) in per_column[..cols].iter().enumerate() {
            let want = if (i as u32) < boundary {
                chrome::TILE_PX
            } else if i as u32 == boundary {
                (cell * layout::BAR_CELL_PX) % chrome::TILE_PX
            } else {
                0
            };
            assert_eq!(
                *filled, want,
                "at cell {cell}, column {i} is not {want} px of fill"
            );
        }
    }
    assert_eq!(
        layout::BAR_CELLS * layout::BAR_CELL_PX,
        bar.w,
        "the bar's cell grid does not fill its declared box"
    );
    assert_eq!(chrome::bar_tile(chrome::TILE_PX), chrome::BAR_FIRST + 8);
}

/// §5.5's one-line rule at the bound a cell-addressed renderer has: no drawn line is longer than its
/// box, the marker is always three ASCII periods, and a line that fits is untouched.
#[test]
fn a_drawn_line_never_exceeds_its_box_and_the_marker_is_three_periods() {
    assert_eq!(surface::ELLIPSIS, "...");
    for cols in 4..=30usize {
        for text in [
            "",
            "i",
            "Bioluminescent",
            "The Lengthening Day (Remastered)",
            "iiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiii",
        ] {
            let line = surface::line_for(text, cols);
            assert!(
                line.chars() <= cols,
                "{text:?} at {cols} cells drew {} characters",
                line.chars()
            );
            if line.cut() {
                assert!(
                    line.as_str().ends_with("..."),
                    "{:?} is cut without a marker",
                    line.as_str()
                );
                assert_eq!(line.chars(), cols, "a cut line does not fill its box");
            } else {
                assert_eq!(line.as_str(), text, "a fitting line was changed");
            }
        }
    }
    // Three cells hold the marker and nothing else; two cannot hold it, and that is the state
    // `title-fit` refuses at bake time rather than a line a ROM draws.
    assert_eq!(surface::line_for("abcd", 3).as_str(), "...");
    assert_eq!(surface::line_for("abcd", 2).as_str(), "");
}

/// The two bounds agree where §5.5's advance verdict is the stricter one, and the drawing rule is the
/// stricter one everywhere else — the claim `surface.rs`'s module docs make. On this face every
/// advance is ≤ 8 px, so the cell bound is never looser; this leg pins the relationship rather than
/// restating it.
#[test]
fn the_drawing_bound_is_the_stricter_of_the_two_fit_bounds() {
    let title = &layout::ELEMENTS[surface::TITLE];
    let cols = (title.w / chrome::TILE_PX) as usize;
    let mut cell_bound_won = 0;
    for text in [
        "Bioluminescent",
        "Bioluminescent Overdriven: An Unreasonably Long Album Title",
        "iiiiiiiiiiiiiiiiiiiiiiiiiiii",
        "WWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWW",
    ] {
        let elided = glyphs::elide(text, title.w, 0);
        let line = surface::line_for(text, cols);
        assert!(
            glyphs::measure(elided.text) <= title.w,
            "the advance elision produced a line wider than the box"
        );
        if elided.text.len() > line.chars() {
            cell_bound_won += 1;
        }
        assert!(
            glyphs::measure(line.as_str()) <= title.w,
            "the drawn line is wider than the box by advance: {:?}",
            line.as_str()
        );
    }
    assert!(
        cell_bound_won > 0,
        "no sample exercised the case where the cell bound is stricter, so this leg proves nothing"
    );
}

/// §5.3's tile counts are the pool audit's terms, and the icons are distinguishable by shape (not by
/// hue, which §5.6 forbids as a sole carrier).
#[test]
fn the_authored_tiles_are_the_declared_pool_terms_and_are_distinct() {
    assert_eq!(
        chrome::FILL_TILES + chrome::ICON_TILES,
        layout::POOL_CHROME_DECLARED as usize,
        "the chrome tiles and §5.1's declared term disagree"
    );
    assert_eq!(chrome::BAR_TILES, layout::BAR_PATTERN_TILES as usize);
    let mut seen: Vec<&[u8; chrome::TILE_BYTES]> = Vec::new();
    for icon in [
        chrome::ICON_SELECT,
        chrome::ICON_START,
        chrome::ICON_PREV,
        chrome::ICON_NEXT,
        chrome::ICON_RESTART,
    ] {
        let tile = &chrome::TILES[icon as usize];
        assert!(
            !seen.iter().any(|t| *t == tile),
            "two control icons share a bitmap"
        );
        seen.push(tile);
    }
    assert_eq!(layout::CONTROL_SLOTS.len(), chrome::ICON_TILES);
    for slot in &layout::CONTROL_SLOTS {
        assert!(
            chrome::icon_for_slot(slot.key).is_some(),
            "{} has no icon",
            slot.key
        );
    }
}

/// The palettes: §5.4's four declared roles, entry 0 the backdrop in every bank, and the bar's fill
/// distinguishable from its track by luminance as well as hue (§5.6's verifier).
#[test]
fn the_palettes_are_the_declared_roles_and_the_bar_reads_without_hue() {
    for bank in [&chrome::PALETTE_TEXT, &chrome::PALETTE_CHROME] {
        assert_eq!(bank[0], chrome::NAVY);
        assert_eq!(bank[1], chrome::WHITE);
        assert_eq!(bank[2], chrome::SLATE);
        assert_eq!(bank[3], chrome::AMBER);
        for entry in &bank[4..] {
            assert_eq!(
                *entry,
                chrome::NAVY,
                "an undeclared palette entry is not the backdrop"
            );
        }
    }
    // Luminance, not hue: white against slate differs by a wide margin, so a deuteranope reads the
    // proportion from brightness alone.
    let lum = |v: u16| -> u32 {
        let r = (v & 0x1f) as u32;
        let g = ((v >> 5) & 0x1f) as u32;
        let b = ((v >> 10) & 0x1f) as u32;
        3 * r + 6 * g + b
    };
    assert!(
        lum(chrome::WHITE) > lum(chrome::SLATE) * 3,
        "the bar's fill and its track are not distinguishable by luminance"
    );
}

/// §5.6's "a scene cannot invalidate a measurement": the region's fills never touch a chrome cell,
/// and the surface's own writes never leave its two bands.
#[test]
fn the_scene_region_and_the_chrome_bands_cannot_overlap() {
    let mut writes = surface::Writes::new();
    surface::Surface::paint_blank(&mut writes);
    for w in writes.as_slice() {
        assert!(
            w.y < layout::REGION.rows as u8,
            "the scene wrote below its region: row {}",
            w.y
        );
        assert!(
            (w.x as u32) < layout::REGION.cols,
            "the scene wrote past its column"
        );
    }

    let mut ui = surface::Surface::new();
    ui.paint_static(&mut writes);
    let chrome_row = layout::BANDS[0].screen_row;
    let title_row = layout::BANDS[1].screen_row;
    for w in writes.as_slice() {
        let y = w.y as u32;
        assert!(
            y == chrome_row || y == chrome_row + 1 || y == title_row || y == title_row + 1,
            "the surface wrote outside its two bands: row {y}"
        );
    }
}

/// The composite, printed by the gate on request (`--nocapture`): the boot frame, then one frame
/// mid-track and paused. Not an assertion — the picture the assertions above are about, so a failure
/// has something to look at beside the numbers.
#[test]
fn the_composite_is_printable_for_a_human() {
    let mut ui = surface::Surface::new();
    let mut canvas = Canvas::new();
    paint_static(&mut ui, &mut canvas);
    let mut writes = surface::Writes::new();
    ui.update(&view("Bioluminescent", 0, TOTAL, 0, "PLAYING"), &mut writes);
    canvas.apply(&writes);
    canvas.print("0:00, playing");

    ui.update(
        &view(
            "Bioluminescent",
            61 * geometry::RATE_HZ,
            TOTAL,
            30,
            "PAUSED",
        ),
        &mut writes,
    );
    canvas.apply(&writes);
    canvas.print("1:01, paused (bar at 30/80 > one third)");
}

/// **Every write names a tile that exists in the tileset it names.** The leg this file was missing.
///
/// agb copies a tile's bytes out of the tileset a `set_tile` names with a raw pointer
/// (`vram_manager.rs:962-1000`), and its bounds check lives in `TileSet::get_tile_data`, which that
/// path never calls. So an index that is valid for one tileset and not the other does not fault and
/// does not panic — it copies whatever follows the smaller tileset in ROM into VRAM. That is what
/// 2a PR-(3)'s first ROM did to every glyph: the atlas index went to the 18-tile chrome set, and the
/// screen showed correct positions with garbage glyphs. A picture cannot see this (both tilesets are
/// the same size in the picture); only the index can.
#[test]
fn every_write_names_a_tile_that_exists_in_the_tileset_it_names() {
    let mut ui = surface::Surface::new();
    let mut writes = surface::Writes::new();
    let mut check = |writes: &surface::Writes, what: &str| {
        for w in writes.as_slice() {
            match w.source {
                surface::Source::Glyph => {
                    assert!(
                        w.tile < glyphs::ATLAS_TILES as u16,
                        "{what}: glyph tile {} is past the atlas's {} tiles",
                        w.tile,
                        glyphs::ATLAS_TILES
                    );
                    assert_ne!(
                        w.tile,
                        glyphs::BLANK_TILE,
                        "{what}: a glyph write names the blank sentinel"
                    );
                }
                surface::Source::Chrome => {
                    assert!(
                        w.tile < chrome::TILE_COUNT as u16,
                        "{what}: chrome tile {} is past the {} authored tiles",
                        w.tile,
                        chrome::TILE_COUNT
                    );
                }
            }
        }
    };

    surface::Surface::paint_blank(&mut writes);
    check(&writes, "the scene's backing");
    ui.paint_static(&mut writes);
    check(&writes, "the static layer");

    // A boot frame, a long title, every second of a minute, every bar cell, and every state word: the
    // four things whose tile indices are computed rather than declared.
    for (title, word) in [
        ("Bioluminescent", "PLAYING"),
        (
            "Bioluminescent Overdriven: An Unreasonably Long Album Title",
            "PAUSED",
        ),
        ("iiiiiiiiiiiiiiiiiiiiiiiiiiii", "BUFFER"),
        ("", "SKIP"),
    ] {
        for second in [0u32, 7, 59, 61, 599] {
            for bar_cell in [0u32, 1, 40, layout::BAR_CELLS] {
                ui.update(
                    &view(title, second * geometry::RATE_HZ, TOTAL, bar_cell, word),
                    &mut writes,
                );
                check(&writes, "a frame");
            }
        }
    }
}
