//! The verdict, in words: the minimum that puts `PASSED` or `FAILED` on screen.
//!
//! Generalised from `examples/flac_integration/src/text.rs` — the shipped renderer
//! `product_play_main.rs` uses for its own verdict — but carrying only what a two-word
//! verdict needs: the font include and one draw/present call. There is no `measure`,
//! no wrapping, no ellipsis, no palette bank beyond 0/1 and no dynamic tiles;
//! ALBUM-PLAYER.md §4.1 names that layout engine as Phase 2's A5, so growing it here
//! would pre-empt it. A glyph this subset does not carry is a reason to pick a
//! different word, not to grow the renderer.
//!
//! **Why a word at all.** Fix 2 signalled the verdict with a lamp's hue, and a hue is
//! exactly what a colourblind verifier cannot read. The word is the signal; the
//! backdrop colour is the belt to its braces: PASSED on blue, FAILED on red, which
//! rides the blue/yellow axis that deuteranopia and protanopia preserve.
//!
//! **One shot, one commit.** [`show`] builds a `RegularBackground` that lives only for
//! the call, shows it into one frame and commits; every caller then ends in `loop {}`.
//! agb's `TiledGraphicsFrame::commit` derives DISPCNT's background-enable mask from the
//! backgrounds *shown into that frame*, so a commit from a fresh frame with nothing
//! shown writes `enabled_backgrounds = 0` and repaints the backdrop alone — the bug
//! 6a-7b's first hardware run found (blue screen, no text). Presenting a background here
//! keeps the mask right, so the visible result is palette entry 0 plus the glyphs, in
//! one commit, and an empty `text` is the backdrop-only present the "running" state
//! draws. Never commit again afterwards: nothing is shown into that second frame.
//!
//! The font is the tracked OFL subset at `assets/fonts/fusion-pixel-8px-proportional-latin-ascii.ttf`
//! (`assets/fonts/README.md`, `fusion-pixel-OFL.txt`): 96 glyphs, U+0020–U+007E, 6.9 KB.
//! `include_font!` rasterizes every glyph in the file, which is why the subset exists.

/// The PASS verdict: the word, and the whole of what the screen says.
pub const PASSED: &str = "PASSED";

/// The FAIL verdict. A refusal's *reason* stays in the log — on-screen text is two
/// words in this PR, and §13's R5 gives the named `DATA ERROR` verdict to Phase 2's
/// screens rather than to a reason-renderer invented here.
pub const FAILED: &str = "FAILED";

/// The first glyph the tracked subset carries (`U+0020`, space).
pub const FIRST_GLYPH: char = '\u{20}';

/// The last glyph the tracked subset carries (`U+007E`, `~`).
pub const LAST_GLYPH: char = '\u{7E}';

/// One row of the verdict text: 30 columns × the subset font's 8 px cell is the
/// screen's whole 240 px, so a word of at most this many characters is inside the
/// display without measuring anything. Measured, the widest subset glyph advances
/// 6 px and today's words lay out at 30/28 px; the bound is the font's cell, not
/// today's words, so it does not need revisiting when a word changes.
pub const COLUMNS: usize = 30;

/// Whether `text` is renderable: every character inside the tracked subset's glyph
/// range and no more than [`COLUMNS`] of them.
///
/// `include_font!` rasterizes exactly the glyphs the file holds — a character outside
/// the subset has no glyph and draws as nothing at all, not as a fallback box, which is
/// the failure this check turns into a red host test instead of a blank screen.
#[must_use]
pub fn renderable(text: &str) -> bool {
    text.chars()
        .all(|c| (FIRST_GLYPH..=LAST_GLYPH).contains(&c))
        && text.chars().count() <= COLUMNS
}

#[cfg(target_arch = "arm")]
use agb::display::{
    Graphics, Palette16, Priority, Rgb15,
    font::{Font, Layout, LayoutSettings, RegularBackgroundTextRenderer},
    tiled::{RegularBackground, RegularBackgroundSize, TileFormat},
};

/// The tracked OFL subset (see the module docs). `include_font!` resolves against the
/// crate's manifest directory, so two levels up from `examples/album_player` is the
/// repository root — the same form the shipped copy uses from `examples/flac_integration`.
#[cfg(target_arch = "arm")]
static FONT: Font = agb::include_font!(
    "../../assets/fonts/fusion-pixel-8px-proportional-latin-ascii.ttf",
    8
);

/// The (4,4) origin the shipped renderer uses, kept so this screen reads the same.
#[cfg(target_arch = "arm")]
const ORIGIN: (i32, i32) = (4, 4);

/// Present `text` on a fresh background in one commit: `backdrop` is palette entry 0,
/// the glyphs are white from entry 1.
///
/// An empty `text` presents the backdrop alone, which is the "still running" screen.
/// See the module docs for why the background is shown into the frame even then, and
/// why the caller must never commit again after this returns.
#[cfg(target_arch = "arm")]
pub fn show(gfx: &mut Graphics<'_>, text: &str, backdrop: Rgb15) {
    let mut colours = [Rgb15::BLACK; 16];
    colours[0] = backdrop;
    colours[1] = Rgb15::WHITE;

    let mut background = RegularBackground::new(
        Priority::P0,
        RegularBackgroundSize::Background32x32,
        TileFormat::FourBpp,
    );
    let mut renderer = RegularBackgroundTextRenderer::new(ORIGIN, 0);
    // Default settings: left aligned, no drop shadow, palette entry 1, no line limit —
    // a verdict is one word, so there is nothing to wrap and nothing to measure.
    let layout = Layout::new(text, &FONT, &LayoutSettings::new());
    for group in layout {
        renderer.show(&mut background, &group);
    }

    gfx.set_background_palette(0, &Palette16::new(colours));
    let mut frame = gfx.frame();
    background.show(&mut frame);
    frame.commit();
}
