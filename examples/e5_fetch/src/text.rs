//! A minimal on-screen text screen for the E5 ROM, using the committed OFL
//! pixel font and agb's tiled text renderer.
//!
//! mGBA serial (`agb::println!`) is the only output the existing ROMs have, and
//! it does not exist on hardware. This module is the hardware-readable path:
//! the same numbers rendered on the GBA screen, readable on a stock unit with
//! no flashcart debug channel.

use agb::display::{
    Palette16, Priority, Rgb15,
    font::{Font, Layout, LayoutSettings, RegularBackgroundTextRenderer},
    tiled::{RegularBackground, RegularBackgroundSize, TileFormat},
};

static FONT: Font = agb::include_font!(
    "../../assets/fonts/fusion-pixel-8px-proportional-latin-ascii.ttf",
    8
);

/// The font's line height, so the ROM can report the layout budget.
pub fn line_height() -> i32 {
    FONT.line_height()
}

/// Render `text` on a fresh background. `backdrop` is palette entry 0 (the
/// pass/fail colour); entry 1 is the white glyph colour.
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
    let layout = Layout::new(
        text,
        &FONT,
        &LayoutSettings::new().with_max_line_length(236),
    );
    let mut renderer = RegularBackgroundTextRenderer::new((4, 4), 0);
    for group in layout {
        renderer.show(&mut background, &group);
    }

    gfx.set_background_palette(0, &palette);
    let mut frame = gfx.frame();
    background.show(&mut frame);
    frame.commit();
}
