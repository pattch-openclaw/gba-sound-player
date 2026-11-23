//! `make album-preview` — the surface as pictures, without a cart.
//!
//! Builds the same boot frame `album_play_main.rs` builds (§5.2's option (c): the band stack, the
//! region, the icons, the first track's title, `0:00` and an empty bar), applies the same map writes
//! the ROM applies, and writes it out as a PNG at 1× and 4× plus the 30×20 character grid beside
//! each. A second frame — paused, a minute in, with the bar a third full — is written too, so the
//! two states that change cells can be compared.
//!
//! **It is an instrument, not a witness.** §5.6's legibility rule is judged by a person on a real
//! screen; this exists so that person is not the first to see the layout, and so a mis-addressed row
//! is a host run rather than a flashing session. What it *does* share with the ROM is the code that
//! decides the cells (`surface`), the tiles both tilesets upload (`chrome`, `glyphs`) and the
//! palette table — so a picture that is wrong here is wrong on hardware for the same reason.
//!
//! Host-only: `src/bin/` is not built for the GBA target (`make album-rom` names `--bin album-play`),
//! and this file would not link there — it wants `std::fs`.

use album_player::{chrome, geometry, preview, surface};

/// The fixture album's length: 576 s at 16,384 Hz, which is the `9:36` the clock prints.
const TOTAL: u32 = 576 * geometry::RATE_HZ;

fn main() {
    let out = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "album-preview".to_string());

    let mut ui = surface::Surface::new();
    let mut canvas = preview::Canvas::new();
    let mut writes = surface::Writes::new();

    // The two static layers, in the order `Painted::new` composes them: the region's backing into the
    // scene background, then the bands and the control icons into the surface's.
    surface::Surface::paint_blank(&mut writes);
    canvas.apply(&writes);
    ui.paint_static(&mut writes);
    canvas.apply(&writes);

    let boot = surface::View {
        title: "Bioluminescent",
        elapsed_samples: 0,
        total_samples: TOTAL,
        bar_cell: 0,
        state_word: "PLAYING",
    };
    ui.update(&boot, &mut writes);
    canvas.apply(&writes);
    write(&canvas, &format!("{out}-1x.png"));
    for row in canvas.ascii() {
        println!("{row}");
    }

    let later = surface::View {
        title: "Bioluminescent",
        elapsed_samples: 61 * geometry::RATE_HZ,
        total_samples: TOTAL,
        bar_cell: (layout_bar_cells() * 3) / 8,
        state_word: "PAUSED",
    };
    ui.update(&later, &mut writes);
    canvas.apply(&writes);
    write(&canvas, &format!("{out}-4x.png"));

    println!();
    println!(
        "wrote {out}-1x.png (1x, 240x160), {out}-4x.png (4x, 960x640) and a .txt grid beside each"
    );
    println!(
        "palette: bank 0/1 = navy/white/slate/amber (§5.4's four roles), tiles = atlas 188 + chrome {}",
        chrome::TILE_COUNT
    );
}

fn layout_bar_cells() -> u32 {
    album_player::layout::BAR_CELLS
}

fn write(canvas: &preview::Canvas, path: &str) {
    let scale = if path.contains("-4x") { 4 } else { 1 };
    match canvas.save(path, scale) {
        Ok(()) => {}
        Err(error) => {
            eprintln!("{path}: {error}");
            std::process::exit(1);
        }
    }
}
