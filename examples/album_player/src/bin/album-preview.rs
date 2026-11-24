//! `make album-preview` — the surface as pictures, without a cart.
//!
//! Builds the same boot frame `album_play_main.rs` builds (§5.2's option (c): the chrome/title art,
//! the region, the icons, the start track's title, `0:00` and an empty bar), applies the same map
//! writes the ROM applies, and writes it out as a PNG at 1× and 4× plus the 30×20 character grid
//! beside each. A second frame — paused, a minute in, with the bar a third full — is written too, so
//! the two states that change cells can be compared.
//!
//! **The title, the length and the bar cell come from the album**, not from a string typed here:
//! `assets::ALBM1` is the same manifest the ROM parses, `assets::TRACKS` the same pins, and the bar
//! cell is `surface::BarCell::from_samples` on that track's own §6 reciprocal. A preview with a
//! literal length would agree with itself and with nothing else.
//!
//! **It is an instrument, not a witness.** §5.6's legibility rule is judged by a person on a real
//! screen; this exists so that person is not the first to see the layout, and so a mis-addressed row
//! is a host run rather than a flashing session. What it *does* share with the ROM is the code that
//! decides the cells (`surface`), the tiles both tilesets upload (`chrome`, `glyphs`) and the
//! palette table — so a picture that is wrong here is wrong on hardware for the same reason.
//!
//! Host-only: `src/bin/` is not built for the GBA target (`make album-rom` names `--bin album-play`),
//! and this file would not link there — it wants `std::fs`.

use album_player::{assets, chrome, geometry, meta::Album, preview, surface};

/// The gate's start track (the ROM's `START_TRACK`); the product starts from track 0 in 2b.
const START_TRACK: usize = 1;

fn main() {
    let out = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "album-preview".to_string());

    let album = match Album::parse(assets::ALBUM1) {
        Ok(album) => album,
        Err(refusal) => {
            eprintln!("album-preview: the manifest was refused: {refusal}");
            std::process::exit(1);
        }
    };
    let track = album.track(START_TRACK as u32);
    let title = album.track_title(START_TRACK as u32);
    let total_samples = assets::TRACKS[START_TRACK].n_samples;

    let mut ui = surface::Surface::new();
    let mut canvas = preview::Canvas::new();
    let mut writes = surface::Writes::new();

    // The three layers, bottom to top: scene, chrome/title art, foreground (glyphs, icons, bar).
    // The page fill is transparent and is what stops any visible cell depending on agb's reserved
    // blank tile; the preview draws it and then falls through it, which is what the hardware does.
    surface::Surface::paint_page(&mut writes);
    canvas.apply_layer(preview::LAYER_SCENE, &writes);
    surface::Surface::paint_blank(&mut writes);
    canvas.apply_layer(preview::LAYER_SCENE, &writes);

    surface::Surface::paint_page(&mut writes);
    canvas.apply_layer(preview::LAYER_CHROME, &writes);
    ui.paint_static(&mut writes);
    canvas.apply_layer(preview::LAYER_CHROME, &writes);

    surface::Surface::paint_page(&mut writes);
    canvas.apply_layer(preview::LAYER_TEXT, &writes);
    ui.paint_icons(&mut writes);
    canvas.apply_layer(preview::LAYER_TEXT, &writes);

    let boot = surface::View {
        title,
        elapsed_samples: 0,
        total_samples,
        bar_cell: surface::BarCell::new(0),
        state_word: "PLAYING",
    };
    ui.update(&boot, &mut writes);
    canvas.apply_layer(preview::LAYER_TEXT, &writes);
    write(&canvas, &format!("{out}-1x.png"));
    for row in canvas.ascii() {
        println!("{row}");
    }

    let elapsed = 61 * geometry::RATE_HZ;
    let later = surface::View {
        title,
        elapsed_samples: elapsed,
        total_samples,
        bar_cell: surface::BarCell::from_samples(elapsed, track.recip),
        state_word: "PAUSED",
    };
    ui.update(&later, &mut writes);
    canvas.apply_layer(preview::LAYER_TEXT, &writes);
    write(&canvas, &format!("{out}-4x.png"));

    println!();
    println!(
        "wrote {out}-1x.png (1x, 240x160), {out}-4x.png (4x, 960x640) and a .txt grid beside each"
    );
    println!(
        "album: {} tracks, track {START_TRACK} \"{title}\" {total_samples} samples; palette: bank 0/1 = navy/white/slate/amber (§5.4's four roles), tiles = atlas 188 + chrome {}",
        album.n_tracks(),
        chrome::TILE_COUNT
    );
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
