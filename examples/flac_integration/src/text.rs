//! On-screen text for the product ROM, so a stock GBA can be read without
//! serial (which exists on emulators, not on hardware).
//!
//! Same renderer, palette convention and (4,4) origin as the 6a-7b summary
//! (`examples/flac_spike/src/item6a7b.rs`) and E5's `text.rs`: entry 0 is the
//! backdrop (the verdict colour), entry 1 the glyphs, and [`measure`] reports
//! the laid-out extent so the ROM can log whether the text fits 240×160.
//!
//! **One-shot present.** `show` builds a `RegularBackground` that lives only for
//! the call and commits one frame. The caller must present nothing afterwards:
//! agb's `TiledGraphicsFrame::commit` derives DISPCNT's background-enable mask
//! from the backgrounds *shown into that frame*, so a commit from a fresh frame
//! writes `enabled_backgrounds = 0` and repaints the backdrop alone. That is not
//! hypothetical — it is the bug 6a-7b's first hardware run found (blue screen, no
//! text). The product ROM therefore ends in `loop {}`, not a commit loop.

use agb::display::{
    Palette16, Priority, Rgb15,
    font::{Font, Layout, LayoutSettings, RegularBackgroundTextRenderer},
    tiled::{RegularBackground, RegularBackgroundSize, TileFormat},
};

/// The committed OFL pixel font, the same subset and size E5 renders with.
/// `include_font!` resolves against the crate's manifest directory (measured:
/// the E5 copy's `../../assets/...` is `examples/e5_fetch/../../assets`, i.e.
/// the repository root), so this crate needs the same two levels, not three.
static FONT: Font = agb::include_font!(
    "../../assets/fonts/fusion-pixel-8px-proportional-latin-ascii.ttf",
    8
);

/// The (4,4) origin and 236 px line budget E5 chose. [`measure`] and [`show`]
/// share these, so the fit check cannot measure a layout nobody renders.
const ORIGIN: (i32, i32) = (4, 4);
const MAX_LINE_LENGTH: i32 = 236;
/// Visible screen, for the other half of the fit check.
const SCREEN_WIDTH: i32 = 240;
const SCREEN_HEIGHT: i32 = 160;

fn settings() -> LayoutSettings {
    LayoutSettings::new().with_max_line_length(MAX_LINE_LENGTH)
}

/// Screen-space extent of `text` under the settings [`show`] renders with.
#[derive(Clone, Copy, Debug)]
pub struct Fit {
    /// Lines used (0 = nothing would be drawn).
    pub lines: i32,
    /// Rightmost pixel column reached.
    pub right: i32,
    /// Bottom-most pixel row reached.
    pub bottom: i32,
}

impl Fit {
    /// Whether every glyph lands inside the visible area.
    #[must_use]
    pub fn fits(&self) -> bool {
        self.right <= SCREEN_WIDTH && self.bottom <= SCREEN_HEIGHT
    }
}

/// Measure `text` exactly as [`show`] will lay it out.
#[must_use]
pub fn measure(text: &str) -> Fit {
    let settings = settings();
    let mut fit = Fit {
        lines: 0,
        right: 0,
        bottom: 0,
    };
    for group in Layout::new(text, &FONT, &settings) {
        let position = group.position();
        let bounds = group.bounds();
        fit.lines = fit.lines.max(group.line() + 1);
        fit.right = fit.right.max(ORIGIN.0 + position.x + bounds.x);
        fit.bottom = fit.bottom.max(ORIGIN.1 + position.y + bounds.y);
    }
    fit
}

/// Render `text` on a fresh background: `backdrop` is palette entry 0, glyphs
/// are white from entry 1. Same shape as E5's `text::show`.
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
    let settings = settings();
    let layout = Layout::new(text, &FONT, &settings);
    let mut renderer = RegularBackgroundTextRenderer::new(ORIGIN, 0);
    for group in layout {
        renderer.show(&mut background, &group);
    }

    gfx.set_background_palette(0, &palette);
    let mut frame = gfx.frame();
    background.show(&mut frame);
    frame.commit();
}
