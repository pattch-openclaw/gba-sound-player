//! On-screen text for the ULI2 player, so a stock GBA can be read without the
//! emulator's debug log (`agb::println!` goes nowhere on hardware).
//!
//! Adapted from `examples/flac_integration/src/text.rs`, carrying its finding
//! with it: a `commit` derives the enabled-background mask from the backgrounds
//! *shown into that frame*, so the background must be `show`n before **every**
//! commit. The player redraws about once a second, and each redraw re-shows the
//! background, which is what keeps the text visible across frames.

use agb::display::{
    Palette16, Priority, Rgb15,
    font::{Font, Layout, LayoutSettings, RegularBackgroundTextRenderer},
    tiled::{RegularBackground, RegularBackgroundSize, TileFormat},
};

/// The committed OFL pixel font (`include_font!` resolves against this crate's
/// manifest directory, so two levels up is the repository root).
static FONT: Font = agb::include_font!(
    "../../assets/fonts/fusion-pixel-8px-proportional-latin-ascii.ttf",
    8
);

const ORIGIN: (i32, i32) = (4, 4);
const MAX_LINE_LENGTH: i32 = 232;

fn settings() -> LayoutSettings {
    LayoutSettings::new().with_max_line_length(MAX_LINE_LENGTH)
}

/// Draw `text` and present one frame. Entry 0 is the backdrop, entry 1 the
/// glyphs — the same convention as the FLAC product ROM.
pub fn show(gfx: &mut agb::display::Graphics<'_>, text: &str, backdrop: Rgb15) {
    let mut colours = [Rgb15::BLACK; 16];
    colours[0] = backdrop;
    colours[1] = Rgb15::WHITE;
    let palette = Palette16::new(colours);

    let mut background = RegularBackground::new(
        Priority::P0,
        RegularBackgroundSize::Background32x32,
        TileFormat::FourBpp,
    );
    let layout = Layout::new(text, &FONT, &settings());
    let mut renderer = RegularBackgroundTextRenderer::new(ORIGIN, 0);
    for group in layout {
        renderer.show(&mut background, &group);
    }

    gfx.set_background_palette(0, &palette);
    let mut frame = gfx.frame();
    background.show(&mut frame);
    frame.commit();
}
