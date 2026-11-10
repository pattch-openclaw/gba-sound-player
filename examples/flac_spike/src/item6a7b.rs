//! # 6a-7b — the `agb` half: font, layout, and the fit check
//!
//! The pure half (the fold, the text, the sign conventions) is
//! [`flac_spike::summary`], where host tests can pin it. This module is the part
//! that cannot exist off the GBA target: the committed OFL pixel font, the tiled
//! background it renders into, and the measurement that says the text actually
//! landed on a 240×160 screen.
//!
//! ## Why there is a fit check at all
//!
//! 240×160 at 8 px glyphs is a hard budget, and this font's `line_height()` is
//! **12** (measured, not assumed: the E5 ROM logs it at boot). Twelve lines fit;
//! the summary's full form is [`flac_spike::summary::SCREEN_LINES`] = 10. That
//! leaves two lines of slack, which is not much margin to spend on a rung whose
//! whole output is the screen — so [`measure`] lays the string out with the
//! *same* settings [`show`] renders with and the ROM logs `lines` / `right` /
//! `bottom` / `fits`. A screen that overflows silently drops the number the
//! hardware run exists to read, and this repository does not accept a silent
//! drop where a check is cheap.
//!
//! The template is `examples/e5_fetch/src/text.rs`, deliberately: one renderer,
//! one palette convention (entry 0 backdrop, entry 1 glyphs), one (4,4) origin.
//!
//! ## [`show`] is a one-shot present, and the caller must not commit again
//!
//! `show` builds a `RegularBackground` that lives only for this call, draws the
//! text into it, and commits **one** frame. That commit is the only reason the
//! text is ever visible, and it is also why the calling ROM must stop presenting
//! afterwards.
//!
//! agb 0.25's `TiledGraphicsFrame::commit` derives DISPCNT's background-enable
//! mask from the backgrounds *shown into that frame* (`BackgroundFrame::
//! num_regular`, reset to 0 by `Graphics::frame()`). A commit from a fresh frame
//! therefore writes `enabled_backgrounds = 0` and repaints the backdrop alone —
//! the text is gone, the palette (a direct VRAM write) is not. That is not a
//! hypothesised failure: it is what the first hardware run of this rung showed
//! on a GBA 001, twice — **a blue screen with no text** — because the spike ROM
//! ended in a frame-commit loop left over from its colour-only verdict. No mGBA
//! run could see it, because the harness is headless and nothing was ever
//! displayed to be erased.
//!
//! So the 6a-7 image ends in a bare `loop {}` under `item6a7-wide`, which is
//! E5's shape and is proven on both units. If this module is ever reused, the
//! rule travels with it: after [`show`], present nothing.

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

/// This font's line height, so the ROM's boot log states the budget it is
/// laying out against instead of implying it.
#[must_use]
pub fn line_height() -> i32 {
    FONT.line_height()
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
