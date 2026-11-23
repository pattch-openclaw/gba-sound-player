#!/usr/bin/env python3
"""Generate the glyph atlas: a tracked face's ASCII cut → `src/glyphs.rs`.

ALBUM-PLAYER.md §5.5a, steps 2-5, as a tool. Step 1 — the `pyftsubset` cut that
makes the tracked `.ttf` exist — is already done and is recorded in
`assets/fonts/README.md`. This file reads that cut's `cmap`, `hmtx`, `hhea` and
`glyf`, walks the outlines at the raster the spec declares, packs each glyph into
the tile rows its own line box spans, and emits the module the app compiles.

What is deliberately *not* here:

* No spec parsing of its own. The face arrives through `album_bake.py`'s
  `FaceMetrics`, the one reader of face files in this tree (§9.0's face rows,
  §5.1's line boxes). A second reader would be a second answer to "what does this
  face measure", which is the thing §5.5 refuses.
* No `freetype`, no `ttf-parser`, no third-party rasteriser. §5.5a step 3 asks
  for the walk itself: for each scanline accumulate crossings and fill on odd
  parity — `glyf`'s fill rule is even-odd, not non-zero, and its quadratics are
  3-point with an implied on-curve point between them. Filling samples *pixel
  centres*, so "is this pixel ink" is a point-in-polygon test in integers: at a
  declared raster every contour coordinate of this cut lands on a whole pixel
  (checked, never assumed — see `atlas-not-on-pixel-grid`), so vertices land on
  even half-pixels and sample points on odd ones, and no float is involved.
* No second atlas. The face is the one the selected variant's text elements
  declare. A second face is a second atlas, which §5.5a puts outside the baseline
  (Q19), so this tool refuses rather than quietly shipping two.

§5.5's one-line elision and its `fit` entry point are emitted into the module (§5.5,
2a PR-(2)), and they live here rather than in `text.rs` or `layout.rs` for the reason
§5.5a step 5 states: the atlas's advance table and the bytes that get painted come out
of one walk of one file, so a box refused by `measure` agrees with the pixels by
construction. `layout::Face::measure` stays the per-face mechanism — the instrument
face is not in this cut — and what does not exist twice is the *rule* about where a
line gets cut: that rule is here, and `ELISION_CASES` is where the Rust copy of it and
the bake's Python copy (`album_bake.FaceMetrics.elide`) are compared.

    python3 scripts/album_glyphs.py                 # emit the module (what the bake calls)
    python3 scripts/album_glyphs.py --check         # is the module on disk what this run derives?
    python3 scripts/album_glyphs.py --print         # the report, writing nothing
    python3 scripts/album_glyphs.py --self-test     # every rule above, asserted
"""

from __future__ import annotations

import argparse
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from album_bake import (  # the one spec/face reader in this tree
    DEFAULT_LAYOUT_SPEC,
    ELLIPSIS,
    REPO,
    FaceMetrics,
    FaceSpec,
    Refusal,
    layout_for,
    write_atomic,
)

try:
    from fontTools.pens.boundsPen import ControlBoundsPen
    from fontTools.ttLib import TTFont
except ImportError:  # pragma: no cover - the refusal below says this out loud
    TTFont = None
    ControlBoundsPen = None

ATLAS_RS = REPO / "examples" / "album_player" / "src" / "glyphs.rs"

#: §5.5a step 2: the cut is ASCII. `SLOT` is indexed off the first of them, and a
#: codepoint outside the range has no slot — which is `text.rs`'s `renderable()`
#: question, not a decision this file makes.
FIRST_GLYPH = 0x20
LAST_GLYPH = 0x7E

#: `agb::display::tiled::TileSetting::BLANK` is `TRANSPARENT_TILE_INDEX`, `0xffff`
#: (`agb-0.25.0/src/display/tiled.rs:117,168`). Stated as a number so a generated
#: module needs no agb on the host. A slot carrying it draws nothing and asks the
#: tile pool for nothing, which is why U+0020 — mapped, and with no outline — is
#: the sentinel rather than a tile.
BLANK_TILE = 0xFFFF

#: The two rules a closed outline can be filled by. `glyf`'s is even-odd, and
#: §5.5a step 3 names it rather than leaving it to a rasteriser's default. The
#: other one is in this file for one reason: so the choice is a *tested*
#: difference instead of an unexamined default (`GlyphAtlas._filled`).
EVEN_ODD = "even-odd"
NON_ZERO = "non-zero"

#: The error ids this tool can raise that are *not* §9.0 register rows. §9.0's
#: "2a may refuse only facts about input its own tools can read" cuts both ways: a
#: row of that register is a build failure with a place in the table, and these are
#: not that — each is a fact about this tool's own input. They are listed here so
#: `--self-test` can show each path firing and a reviewer can see the set.
GLYPH_REFUSALS: tuple[tuple[str, str, str], ...] = (
    (
        "atlas-face-ambiguous",
        "a variant whose text elements name two faces",
        "the selected variant's text elements declare more than one face, so there is no single atlas "
        "to generate: §5.5a's baseline is one atlas for one face, and a second face is a second atlas",
    ),
    (
        "atlas-face-not-truetype",
        "a declared face with no `glyf` outlines to walk",
        "the declared face is not a TrueType file with a `glyf` table, so the outline walk described "
        "above cannot run — a CFF face needs a different rasteriser, and saying so beats drawing half "
        "of it",
    ),
    (
        "atlas-not-on-pixel-grid",
        "the same face asked at half its design size",
        "a contour coordinate does not land on a whole pixel at the declared raster, so this tool's "
        "premise — that the cut is already on the grid at that size, which is what §5.5a's step 3 says "
        "and what makes an exact fill possible — is false of this file at this size",
    ),
    (
        "atlas-glyph-outside-cell",
        "a face whose `hhea` box is smaller than the glyphs it carries",
        "a glyph's outline extends outside the one cell and line box the atlas gives it, so the atlas "
        "would clip it and the fit box would then be describing a drawing that never happens",
    ),
    (
        "atlas-ink-not-matching-outline",
        "the rasteriser's own bug path",
        "the pixels this tool filled do not have the bounding box the outline's control points describe "
        "— the ±1 class of mistake §5.5a's step 3 exists to catch, in a stem or a counter",
    ),
)


def _rel(path: Path) -> str:
    return str(path.relative_to(REPO)) if path.is_relative_to(REPO) else str(path)


def _refusal(rule: str, detail: str) -> Refusal:
    return Refusal(rule, detail)


def _rs_str(text: str) -> str:
    """A Rust string literal for `text`, with anything outside the cut escaped by codepoint.

    The elision cases carry strings a reviewer has to be able to read (`"01 - Bio..."`) and
    codepoints the cut has no glyph for, which must stay visible as `\\u{266B}` rather than
    as a byte in a generated file — a generated file is diffed, and an invisible character
    in a diff is a change nobody can see.
    """
    out = ['"']
    for ch in text:
        o = ord(ch)
        if ch == '"':
            out.append('\\"')
        elif ch == "\\":
            out.append("\\\\")
        elif FIRST_GLYPH <= o <= LAST_GLYPH:
            out.append(ch)
        else:
            out.append(f"\\u{{{o:04X}}}")
    out.append('"')
    return "".join(out)


def _ink_bounds(grid: list[list[bool]]) -> tuple[int, int, int, int] | None:
    """The filled pixels as a half-open pixel box, or `None` when nothing was filled.

    Half-open — last ink pixel plus one — because that is the convention the
    outline's own bounds use once scaled, and it is what makes comparing the two
    a comparison instead of an argument about rounding.
    """
    xs = [x for row in grid for x, on in enumerate(row) if on]
    ys = [y for y, row in enumerate(grid) for on in row if on]
    if not xs:
        return None
    return (min(xs), min(ys), max(xs) + 1, max(ys) + 1)


class Glyph:
    """One codepoint: what the face says about it, and the tiles that carry it."""

    def __init__(
        self,
        code: int,
        advance_px: int,
        ink: tuple[int, int, int, int],
        control_px: tuple[int, int, int, int] | None,
        tiles: list[bytes],
        contours: int,
        tile0: int,
    ) -> None:
        self.code = code
        self.char = chr(code)
        self.advance_px = advance_px
        #: Filled-ink bounds in cell pixels, half-open. `(0,0,0,0)` for a glyph
        #: with no outline.
        self.ink = ink
        #: The same box as the outline's control points, scaled: `x0, y_bottom,
        #: x1, y_top` in font units mapped to cell pixels (top-left origin).
        self.control_px = control_px
        self.tiles = tiles
        self.contours = contours
        self.tile0 = tile0

    @property
    def outline(self) -> bool:
        return self.control_px is not None

    @property
    def lead_px(self) -> int:
        """Left bearing: the column inside the cell where the ink starts."""
        return self.ink[0]


class GlyphAtlas:
    """The declared face's ASCII cut, walked and packed into 4bpp tile rows.

    Every number comes out of the file at the raster the spec declares. The two
    facts §5.5a step 5 says the module has to carry — that the atlas is
    `glyphs × tile_rows` tiles, and that its line box is no taller than the row
    pitch the layout declares — are established here and re-emitted as
    `const _: ()` asserts, so the generated module cannot be wrong about itself
    when this tool is not running (§5.5a step 5, R11's staleness rule).
    """

    def __init__(self, metrics: FaceMetrics, face_index: int = 0, text_elements: tuple[int, ...] = (),
                 text_element_boxes: tuple[tuple[str, int], ...] = ()) -> None:
        self.metrics = metrics
        self.face_id = metrics.face.id
        self.face_file = metrics.face.file
        self.face_index = face_index
        #: Indices into `layout::ELEMENTS` of the elements that will be drawn with
        #: these bytes. The generated module asserts against the layout module's
        #: own `row_px` for exactly those elements (§5.5a's second fact).
        self.text_elements = text_elements
        #: The same elements as `(id, box width in px)`: the boxes §5.5's fit verdict and
        #: one-line elision are measured against. The generated module reads `row_px` out
        #: of the layout module at compile time; it needs these widths at *emit* time, to
        #: build the elision cases the module's own suite re-runs.
        self.text_element_boxes = text_element_boxes
        self.raster = metrics.raster
        self.upem = metrics.upem
        self.tile_px = metrics.tile_px
        self.line_box_px = metrics.line_box_px
        if self.line_box_px % self.tile_px:
            # §9.0's `pitch-not-tileable` condition met from the other side: the
            # layout generator refuses an element whose *pitch* is not a whole
            # number of tile rows; here it is the line box that would not divide,
            # which means no glyph of this face could be packed at all.
            raise _refusal(
                "pitch-not-tileable",
                f"[[face]] {self.face_id}: the line box at {self.raster} px is {self.line_box_px} px, "
                f"not a whole number of {self.tile_px} px tile rows",
            )
        self.tile_rows = self.line_box_px // self.tile_px
        # The baseline inside the cell, in pixels from the cell's top edge. The
        # cell *is* the line box, so this is `hhea.ascender` scaled — read out of
        # the file, never taken from its filename.
        self.baseline_px = -(-metrics.ascender * self.raster // self.upem)

        if TTFont is None:
            raise _refusal(
                "face-not-resolvable",
                f"[[face]] {self.face_id}: fontTools is not importable, so no declared face can be "
                "read here (`pip install fonttools`)",
            )
        try:
            font = TTFont(str(metrics.path))
        except Exception as exc:  # fontTools raises several shapes for "not a font"
            raise _refusal(
                "atlas-face-not-truetype",
                f"[[face]] {self.face_id}: {_rel(metrics.path)} cannot be opened as a font "
                f"({type(exc).__name__}: {exc})",
            ) from exc
        if "glyf" not in font:
            raise _refusal(
                "atlas-face-not-truetype",
                f"[[face]] {self.face_id}: {_rel(metrics.path)} has no `glyf` table "
                f"(tables: {', '.join(sorted(font.keys()))})",
            )
        self._walk(font)
        self._pack()

    # -- steps 2 and 3: read the cut, walk the outlines ------------------------------------

    def _contours(self, font, name: str) -> list[list[tuple[int, int]]]:
        """The glyph's outlines as closed polygons, in font units.

        `getCoordinates` flattens composites, which is the only reason this tool
        never has to know that they exist.
        """
        glyf = font["glyf"]
        coordinates, ends, _flags = glyf[name].getCoordinates(glyf)
        out: list[list[tuple[int, int]]] = []
        start = 0
        for end in ends:
            out.append([(int(x), int(y)) for (x, y) in coordinates[start : end + 1]])
            start = end + 1
        return out

    def _px(self, value: int) -> int:
        """Font units to whole pixels at the declared raster, or a refusal.

        §5.5a step 3 says "at the design size every contour coordinate is already
        an integer". This is that sentence as a check rather than as an
        assumption: a face read at a size it was not cut for lands its stems
        between pixel centres, and every byte this tool then wrote would describe
        a glyph it never drew.
        """
        if (value * self.raster) % self.upem:
            raise _refusal(
                "atlas-not-on-pixel-grid",
                f"[[face]] {self.face_id}: {value} font units is not a whole number of pixels at "
                f"{self.raster} px ({value} x {self.raster} / {self.upem})",
            )
        return value * self.raster // self.upem

    def _filled(self, polygons: list[list[tuple[int, int]]], rule: str = EVEN_ODD) -> list[list[bool]]:
        """Fill at pixel centres, by one of the two rules. `polygons` are in half-pixels.

        The half-open comparison on the scanline (`(y0 > sy) == (y1 > sy)` skips)
        is what makes a vertex shared by two edges count once and a tangent count
        not at all — the two ways a scanline fill goes wrong on a closed outline.
        The crossing test is an integer comparison because `x_at(sy)` is not.

        `EVEN_ODD` is the rule `glyf` states; `NON_ZERO` exists so the difference
        between the two can be observed with *one* geometry: same scanline, same
        vertex skip, same integer side test, and only the accumulation differing —
        a crossing counted for its parity, or for the direction of the edge that
        produced it. A separate implementation of the second rule lets a bug in the
        geometry masquerade as a difference in the rule, and did: the first version
        of that comparison counted each crossing by which side of the sample point
        it fell on, which is not the non-zero rule at all, and it reported the two
        rules disagreeing across the empty cell beside the glyph.
        """
        if rule not in (EVEN_ODD, NON_ZERO):
            raise AssertionError(f"this tool implements two fill rules, not {rule!r}")
        grid: list[list[bool]] = []
        for row in range(self.line_box_px):
            sy = 2 * row + 1
            line: list[bool] = []
            for col in range(self.tile_px):
                sx = 2 * col + 1
                crossings = 0
                winding = 0
                for poly in polygons:
                    n = len(poly)
                    for i in range(n):
                        x0, y0 = poly[i]
                        x1, y1 = poly[(i + 1) % n]
                        if (y0 > sy) == (y1 > sy):
                            continue
                        den = y1 - y0
                        # `x_at(sy) > sx`, with the denominator's sign moved out so
                        # the comparison stays in integers.
                        num = x0 * den + (sy - y0) * (x1 - x0) - sx * den
                        if den < 0:
                            num, den = -num, -den
                        if num > 0:
                            crossings += 1
                            winding += 1 if y1 > y0 else -1
                line.append(bool(winding) if rule == NON_ZERO else bool(crossings % 2))
            grid.append(line)
        return grid

    def _walk(self, font) -> None:
        cmap = font.getBestCmap()
        self.codes = sorted(c for c in cmap if FIRST_GLYPH <= c <= LAST_GLYPH)
        self.measured: dict[int, dict[str, object]] = {}
        #: Outlined glyphs this cut would draw the same way under the other rule.
        #: Reported, not assumed: it says how much of the choice §5.5a step 3 makes
        #: is observable on the face in front of the tool — which for this face is
        #: nothing, and the self-test's constructed ring is where the rules part.
        self.fill_rule_agrees = 0
        for code in self.codes:
            name = cmap[code]
            glyph = font["glyf"][name]
            contours = self._contours(font, name)
            advance_px = self._px(int(font["hmtx"][name][0]))

            # The outline's bounds, twice: this tool's walk of the control points,
            # and the face's own ControlBoundsPen. Two implementations, so the leg
            # that compares them is not one path read twice.
            pen = ControlBoundsPen(None)
            glyph.draw(pen, font["glyf"])
            pen_bounds = pen.bounds
            xs = [x for poly in contours for (x, _y) in poly]
            ys = [y for poly in contours for (_x, y) in poly]
            if bool(xs) != bool(pen_bounds):
                raise _refusal(
                    "atlas-ink-not-matching-outline",
                    f"[[face]] {self.face_id} U+{code:04X} ({chr(code)!r}): this tool walks "
                    f"{len(contours)} contour(s) and the face's own bounds pen answers {pen_bounds!r}",
                )

            if not xs:
                control_cell = None
            else:
                control_cell = (
                    self._px(min(xs)),
                    self.baseline_px - self._px(max(ys)),
                    self._px(max(xs)),
                    self.baseline_px - self._px(min(ys)),
                )
                if pen_bounds:
                    want = (
                        self._px(int(pen_bounds[0])),
                        self.baseline_px - self._px(int(pen_bounds[3])),
                        self._px(int(pen_bounds[2])),
                        self.baseline_px - self._px(int(pen_bounds[1])),
                    )
                    if control_cell != want:
                        raise _refusal(
                            "atlas-ink-not-matching-outline",
                            f"[[face]] {self.face_id} U+{code:04X} ({chr(code)!r}): this tool's control "
                            f"bounds in the cell are {control_cell} px and the face's own "
                            f"ControlBoundsPen says {want} px — one of the two walks is wrong",
                        )
                # Clipping has to be refused where it starts: an outline that
                # reaches outside the cell would fill *partially*, and the
                # ink-versus-outline comparison below would report that as a
                # rasteriser bug rather than as the font question it is.
                x0, y0, x1, y1 = control_cell
                if x0 < 0 or x1 > self.tile_px or y0 < 0 or y1 > self.line_box_px:
                    raise _refusal(
                        "atlas-glyph-outside-cell",
                        f"[[face]] {self.face_id} U+{code:04X} ({chr(code)!r}): its outline spans "
                        f"{control_cell} px inside a cell that is {self.tile_px} x {self.line_box_px} px "
                        f"with the baseline at row {self.baseline_px} — the atlas gives every glyph one "
                        "cell per line, so this would be clipped, and §5.5's fit verdict would then be "
                        "measuring a drawing that cannot happen",
                    )

            # Fill in cell coordinates: x as-is, y flipped about the baseline.
            polygons = [
                [(2 * self._px(x), 2 * (self.baseline_px - self._px(y))) for (x, y) in poly]
                for poly in contours
            ]
            grid = self._filled(polygons)
            if contours and self._filled(polygons, rule=NON_ZERO) == grid:
                self.fill_rule_agrees += 1
            ink = _ink_bounds(grid)
            if control_cell and ink is None:
                raise _refusal(
                    "atlas-ink-not-matching-outline",
                    f"[[face]] {self.face_id} U+{code:04X} ({chr(code)!r}): the outline spans "
                    f"{control_cell[2] - control_cell[0]} x {control_cell[3] - control_cell[1]} px and "
                    "not one pixel was filled",
                )
            if ink != control_cell:
                raise _refusal(
                    "atlas-ink-not-matching-outline",
                    f"[[face]] {self.face_id} U+{code:04X} ({chr(code)!r}): the outline's bounds say the "
                    f"ink is at {control_cell} px and the filled pixels are at {ink} px — a difference "
                    "of one here is a glyph whose stem or whose counter is lost",
                )
            self.measured[code] = {
                "advance_px": advance_px,
                "control_px": control_cell,
                "ink": ink,
                "contours": len(contours),
                "grid": grid,
            }

    # -- step 4: pack ----------------------------------------------------------------------

    def _pack(self) -> None:
        tile = self.tile_px
        self.glyphs: list[Glyph] = []
        self.blank_tile_rows = 0
        next_tile = 0
        for code in self.codes:
            facts = self.measured[code]
            control = facts["control_px"]
            tiles: list[bytes] = []
            if control:
                grid = facts["grid"]
                for row in range(self.tile_rows):
                    data = bytearray()
                    for y in range(tile):
                        line = grid[row * tile + y]
                        for pair in range(0, tile, 2):
                            # GBA 4bpp: one byte per two pixels, low nibble first —
                            # the convention `examples/album_probe/src/bin/
                            # ui_probe.rs:93` states for the bytes it writes by hand.
                            lo = 1 if line[pair] else 0
                            hi = 1 if line[pair + 1] else 0
                            data.append(lo | (hi << 4))
                    if not any(data):
                        self.blank_tile_rows += 1
                    tiles.append(bytes(data))
            tile0 = BLANK_TILE if not control else next_tile * self.tile_rows
            if control:
                next_tile += 1
            self.glyphs.append(
                Glyph(
                    code=code,
                    advance_px=facts["advance_px"],
                    ink=facts["ink"] or (0, 0, 0, 0),
                    control_px=control,
                    tiles=tiles,
                    contours=int(facts["contours"]),
                    tile0=tile0,
                )
            )
        self.outlined = next_tile
        # §5.5a step 5's first fact: one cell-row of tiles per glyph with an
        # outline, times the rows its line box spans. Nothing is shared and
        # nothing is reserved twice, which is also why §5.1's text term is a
        # *distinct* bound rather than a sum of what a screen draws.
        self.atlas_tiles = self.outlined * self.tile_rows

    # -- the numbers, as the plan asks for them --------------------------------------------

    def measure(self, text: str) -> int:
        """The width the face says a string is, in pixels (§5.5's measured fit).

        A sum of `hmtx` advances with no kerning — §5.5's "advance widths only, no
        kerning". A codepoint with no slot contributes nothing and is named by
        `unwritten`, so a caller never reads "no glyph" as "no width".
        """
        advance = {g.code: g.advance_px for g in self.glyphs}
        return sum(advance.get(ord(c), 0) for c in text)

    def unwritten(self, text: str) -> str:
        have = {g.code for g in self.glyphs}
        return "".join(c for c in text if ord(c) not in have)

    def tilemap_px(self, text: str) -> int:
        """The columns a tilemap spends drawing `text`: one 8 px cell per character.

        Not the same number as `measure`, and the difference is not a bug: a
        tilemap puts one tile in each cell whether or not the glyph fills it, so
        §5.1's capacity column is this figure while §5.5's fit verdict is that
        one. A line has to satisfy both, and `text::fit` is where that is said.
        """
        return len(text) * self.tile_px

    # -- step 5: the module ----------------------------------------------------------------

    def rust_text(self) -> str:
        out: list[str] = []
        add = out.append
        add("//! Generated by `scripts/album_glyphs.py` from the face the selected variant declares:")
        add("//! by `make album-bake` for the variant `album.toml` selects, or by that tool's `--emit`")
        add("//! for a named one. Not tracked, not hand-edited (§5.5a step 5, and §2.3's rule about")
        add("//! generated items).")
        add(f"//! Source: [[face]] {self.face_id} in examples/album_player/layout_spec.toml — that is,")
        add(f"//! {self.face_file} read at {self.raster} px, itself a `pyftsubset` cut of a tracked")
        add("//! parent (§5.5a step 1, recorded in assets/fonts/README.md). The outlines, the advances")
        add("//! and the line box are read out of that file; nothing here is transcribed from")
        add("//! ALBUM-PLAYER.md, and no glyph count anywhere is written down by hand (§5.5a).")
        add("//!")
        add("//! How the bytes were made (§5.5a step 3): the `glyf` outlines were walked at the declared")
        add("//! raster — every contour coordinate in this cut lands on a whole pixel there, and the tool")
        add("//! refuses instead of assuming it — then filled by an even-odd crossing count (the `glyf`")
        add("//! rule, not non-zero) sampled at pixel centres, so no float is involved. Before a byte was")
        add("//! written, the filled pixels' bounding box was compared with the outline's own control")
        add("//! bounds, twice derived: this walk, and the face's ControlBoundsPen. That is where a lost")
        add("//! stem or a filled counter would have stopped the build.")
        add("//!")
        add("//! What a glyph is here: `TILE_ROWS_PER_LINE` tiles, one row of the line box each, in")
        add("//! top-to-bottom order. A glyph is *not* a tile, which is why `tile_for` takes a row index.")
        add("")
        add("/// The face these bytes came from. The asserts at the bottom tie this id, this raster and")
        add("/// this line box to the `layout::FACES` row generated from the same spec entry, so the two")
        add("/// generated modules are checked against each other rather than each being trusted.")
        add(f'pub const FACE_ID: &str = "{self.face_id}";')
        add(f'pub const FACE_FILE: &str = "{self.face_file}";')
        add(f"pub const RASTER_PX: u32 = {self.raster};")
        add(f"pub const UNITS_PER_EM: u32 = {self.upem};")
        add("/// The baseline, in pixels from the top of a glyph cell. The cell *is* the line box: this")
        add("/// is `hhea.ascender` scaled at `RASTER_PX`, the same arithmetic as")
        add("/// `layout::Face::line_box_px`.")
        add(f"pub const BASELINE_PX: u32 = {self.baseline_px};")
        add(f"pub const LINE_BOX_PX: u32 = {self.line_box_px};")
        add(f"pub const TILE_PX: u32 = {self.tile_px};")
        add(f"pub const TILE_ROWS_PER_LINE: usize = {self.tile_rows};")
        add("/// Glyphs in the cut that have an outline, counted from `cmap` and `glyf`.")
        add(f"pub const OUTLINED_GLYPHS: usize = {self.outlined};")
        add("/// The atlas's tile count — and the term §5.1's pool sum uses for this face. The assert")
        add("/// that ties it to `layout::Face::atlas_tiles` is at the bottom of this file: the sum in")
        add("/// the layout module and the atlas that answers it must not be able to drift while both")
        add("/// still compile (R14's arithmetic is otherwise a figure nothing can see).")
        add(f"pub const ATLAS_TILES: usize = {self.atlas_tiles};")
        add("/// The ASCII cut's range, and the number of slots: indexed `code - FIRST_GLYPH`.")
        add(f"pub const FIRST_GLYPH: char = '\\u{{{FIRST_GLYPH:04X}}}';")
        add(f"pub const LAST_GLYPH: char = '\\u{{{LAST_GLYPH:04X}}}';")
        add(f"pub const SLOT_COUNT: usize = {len(self.glyphs)};")
        add("/// `agb::display::tiled::TileSetting::BLANK` (`TRANSPARENT_TILE_INDEX`,")
        add("/// agb-0.25.0/src/display/tiled.rs:117): a tile that draws nothing and takes nothing from")
        add("/// the pool. U+0020 is mapped and has no outline, so its slot holds this rather than a")
        add("/// tile — the same sentinel §5.0's band table uses, for the same reason (§5.5a step 2).")
        add(f"pub const BLANK_TILE: u16 = 0x{BLANK_TILE:04X};")
        add("")
        add("/// What one codepoint costs, and where its ink begins.")
        add("#[derive(Debug, Clone, Copy, PartialEq, Eq)]")
        add("pub struct GlyphSlot {")
        add("    /// The first of this glyph's `TILE_ROWS_PER_LINE` tiles; `BLANK_TILE` for a glyph")
        add("    /// with no outline.")
        add("    pub tile0: u16,")
        add("    /// `hmtx` advance, whole pixels at `RASTER_PX`. `measure` sums these, and §5.5's fit")
        add("    /// verdict compares the sum against a declared box.")
        add("    pub advance_px: u16,")
        add("    /// Left bearing: the column in the cell where the ink starts. A renderer that shifts a")
        add("    /// glyph inside its cell needs it; so does anyone auditing a row's ink extent.")
        add("    pub lead_px: u16,")
        add("}")
        add("")
        add(f"pub const SLOT: [GlyphSlot; {len(self.glyphs)}] = [")
        for g in self.glyphs:
            what = "space" if g.code == 0x20 else f"U+{g.code:04X} {chr(g.code)}"
            if g.code == 0x20:
                what += ": mapped, no outline, so the BLANK sentinel and no tile"
            add(
                f"    GlyphSlot {{ tile0: 0x{g.tile0:04X}, advance_px: {g.advance_px}, "
                f"lead_px: {g.lead_px}, }},  // {what}"
            )
        add("];")
        add("")
        add("/// The number of 32-bit words in `GLYPHS`. A `const` rather than `GLYPHS.len()` because")
        add("/// `GLYPHS` is a `static` (see `tile_set`), and a static's length is not usable in a")
        add("/// constant expression; the assert below is what keeps the two equal.")
        add(f"pub const GLYPHS_WORDS: usize = {self.atlas_tiles * 8};")
        add("")
        add("/// The atlas, 4bpp: one 8x8 tile per row of the line box, in the order `slot()` indexes.")
        add("///")
        add("/// `u32` rather than bytes because `TileSet::new`'s contract is 4-byte alignment and a")
        add("/// whole number of 32-byte tiles (agb-0.25.0/src/display/tiled/vram_manager.rs:153), and a")
        add("/// `u32` array is aligned by construction — the same reason the hand-written tiles in")
        add("/// `examples/album_probe/src/bin/ui_probe.rs:95` are `#[repr(align(4))]`. The length")
        add("/// asserts at the bottom of this file are the other half of that contract.")
        add(f"pub static GLYPHS: [u32; {self.atlas_tiles * 8}] = [")
        for glyph in self.glyphs:
            if not glyph.outline:
                continue
            for row, tile in enumerate(glyph.tiles):
                words = [int.from_bytes(tile[o : o + 4], "little") for o in range(0, 32, 4)]
                add(
                    "    "
                    + ", ".join(f"0x{w:08X}" for w in words)
                    + f", // U+{glyph.code:04X} row {row} of {self.tile_rows}"
                )
        add("];")
        add("")
        add("#[inline]")
        add("const fn slot_index(ch: char) -> Option<usize> {")
        add("    let code = ch as u32;")
        add(f"    if code < {FIRST_GLYPH} || code > {LAST_GLYPH} {{")
        add("        None")
        add("    } else {")
        add(f"        Some((code - {FIRST_GLYPH}) as usize)")
        add("    }")
        add("}")
        add("")
        add("/// This character's slot, or `None` for a character outside the cut. A caller's `None` is")
        add("/// `text.rs`'s `renderable()` answer, not a decision made here: §5.5 keeps unwritten and")
        add("/// overflow as two different verdicts.")
        add("#[inline]")
        add("pub fn slot(ch: char) -> Option<GlyphSlot> {")
        add("    slot_index(ch).map(|i| SLOT[i])")
        add("}")
        add("")
        add("/// The tile index for one row of one glyph's cell, in the sense of §5.5a's API note:")
        add("/// `TileSetting::new(tile_for(c, row).unwrap(), TileEffect::new(false, false, palette))`")
        add("/// for `row` in `0..TILE_ROWS_PER_LINE`. Kept as arithmetic rather than as an agb type so")
        add("/// this module compiles — and can be audited — on a host, where §5.5a's pool legs run.")
        add("/// A blank glyph answers `BLANK_TILE` for every row, which is the same answer either way.")
        add("#[inline]")
        add("pub fn tile_for(ch: char, row: usize) -> Option<u16> {")
        add("    let g = slot(ch)?;")
        add("    if g.tile0 == BLANK_TILE {")
        add("        return Some(BLANK_TILE);")
        add("    }")
        add("    if row >= TILE_ROWS_PER_LINE {")
        add("        return None;")
        add("    }")
        add("    Some(g.tile0 + row as u16)")
        add("}")
        add("")
        add("/// The width the declared face says this string occupies, in whole pixels: a sum of")
        add("/// `hmtx` advances, no kerning (§5.5). A character with no slot contributes nothing and")
        add("/// is counted by `unwritten`, so no caller mistakes “no glyph” for “no width”.")
        add("#[must_use]")
        add("pub fn measure(s: &str) -> u32 {")
        add("    let mut total = 0u32;")
        add("    for ch in s.chars() {")
        add("        if let Some(g) = slot(ch) {")
        add("            total += u32::from(g.advance_px);")
        add("        }")
        add("    }")
        add("    total")
        add("}")
        add("")
        add("/// Codepoints `s` contains that the cut has no glyph for. Non-zero means the string")
        add("/// cannot be drawn at all, which is a different failure from not fitting (§5.5).")
        add("#[must_use]")
        add("pub fn unwritten(s: &str) -> usize {")
        add("    s.chars().filter(|c| slot(*c).is_none()).count()")
        add("}")
        add("")
        add("/// The columns a tilemap spends drawing `s`: one `TILE_PX` cell per character, whether or")
        add("/// not that character's ink reaches the cell edge. This is *not* `measure`, and the two")
        add("/// are not interchangeable: a tilemap can only place a tile on a cell boundary, so a")
        add("/// proportional face still occupies whole cells — §5.1's capacity column is this number.")
        add("/// §5.5's fit verdict is `measure`. A line that will be drawn has to satisfy both, and")
        add("/// `text::fit` is the one place that asks.")
        add("#[must_use]")
        add("pub fn tilemap_px(s: &str) -> u32 {")
        add("    (s.chars().count() as u32).saturating_mul(TILE_PX)")
        add("}")
        add("")
        # §5.5's one-line elision and the one entry point a draw path calls for a text
        # element. They live in this module and nowhere else because of §5.5a step 5's
        # property: the advance table here and the bytes that get painted come out of one
        # walk of one file, so a box refused by `measure` agrees with the pixels by
        # construction. `layout::Face::measure` stays the per-face mechanism (the instrument
        # face is not in this cut); what must not exist twice is the *rule* about where a
        # line is cut. The bake's copy of that rule is `album_bake.FaceMetrics.elide`, and
        # `ELISION_CASES` below is where the two copies are compared on the inputs the rule
        # exists for.
        add("/// §5.5's elision marker: three ASCII periods, never U+2026. §5.5a's reason is stated")
        add("/// in tiles — a real ellipsis is a glyph this atlas would have to carry, which is one")
        add("/// more glyph x two tile rows = 190 tiles and a distinct-pool cap of 190, moving §5.1's")
        add("/// 444/488 to 446/490.")
        add("///")
        add("/// There is a second reason, and it is a fact about this face rather than an argument:")
        add("/// the declared title face's cmap stops at U+007E, so a single ellipsis character would")
        add("/// be *unwritten* here as well as expensive. The layout gate measures that")
        add("/// (`variant B: the real ellipsis would not draw at this face even if it were paid for`),")
        add("/// and it is why the marker must never be written as one character in a config, a source")
        add("/// file, or a generated string.")
        add("///")
        add("/// A constant rather than a literal at the call sites, because the three characters have")
        add("/// to have slots in this cut and the test below reads this value to check that they do.")
        add(f'pub const ELLIPSIS: &str = {_rs_str(ELLIPSIS)};')
        add("")
        add("/// What one box should be drawn with: a run of characters, and whether the marker follows")
        add("/// it. The marker is a flag and not part of the string because it is three glyphs like")
        add("/// any others — a draw path writes its cells the same way it writes the rest, and")
        add("/// concatenating would mean allocating on a ROM to copy a constant.")
        add("#[derive(Debug, Clone, Copy, PartialEq)]")
        add("pub struct Elision<'a> {")
        add("    /// The characters to draw, in order, cut at a whole-glyph boundary.")
        add("    pub text: &'a str,")
        add("    /// Whether `ELLIPSIS` is drawn after `text`.")
        add("    pub ellided: bool,")
        add("}")
        add("")
        add("/// §5.5's one-line rule: measure, and if it does not fit, cut at the last whole glyph that")
        add("/// fits plus an ellipsis. Never wrapped — a 2a text element is one row (§5.0).")
        add("///")
        add("/// `keep` is \"the track number is never cut\" as a number: how many leading characters may")
        add("/// not be cut. A title band drawing a bare title passes 0, because `meta.rs` stores the")
        add("/// folded name with no number on it; a row that composes `NN - Title` passes the length of")
        add("/// its own prefix. (§5.5 uses a middle dot as the separator and this face does not carry")
        add("/// U+00B7, which the layout gate prints as a fact — hence the ASCII `-` in the cases below.")
        add("/// A 2b scene that wants the dot has to name a face that carries it.)")
        add("///")
        add("/// An empty answer with `ellided == false` means not even `keep` characters plus the marker")
        add("/// fit. That is a box too small to render any title at all: the bake refuses it as §9.0's")
        add("/// `title-fit`, so a ROM should never see this answer, and")
        add("/// `no_declared_box_is_narrower_than_the_marker` is the test that holds that line.")
        add("#[must_use]")
        add("pub fn elide(s: &str, box_px: u32, keep: usize) -> Elision<'_> {")
        add("    if measure(s) <= box_px {")
        add("        return Elision { text: s, ellided: false };")
        add("    }")
        add("    let marker = measure(ELLIPSIS);")
        add("    // Byte offsets throughout: no allocation in a draw path. The protected prefix is walked")
        add("    // first, and its width is carried forward, because the second loop needs it as the base.")
        add("    let mut kept = 0usize;")
        add("    let mut cut = 0usize;")
        add("    let mut width = 0u32;")
        add("    for (i, ch) in s.char_indices() {")
        add("        if kept == keep {")
        add("            break;")
        add("        }")
        add("        kept += 1;")
        add("        cut = i + ch.len_utf8();")
        add("        if let Some(g) = slot(ch) {")
        add("            width += u32::from(g.advance_px);")
        add("        }")
        add("    }")
        add("    if width + marker > box_px {")
        add("        return Elision { text: \"\", ellided: false };")
        add("    }")
        add("    // Then the longest further run that still leaves room for the marker. The first character")
        add("    // that does not fit is where the line is cut, which is \"the last whole glyph that fits\"")
        add("    // said as a loop. A character with no slot adds nothing and stays in the run: eliding is a")
        add("    // decision about width, and `unwritten` is the other verdict (§5.5 keeps them apart).")
        add("    // `base` is pinned before the loop because `i` below counts from where the protected prefix")
        add("    // ended: a running `cut` and a `cut` used as the slice start are two different numbers, and")
        add("    // mixing them is how a byte index walks past the end of the string.")
        add("    let base = cut;")
        add("    for (i, ch) in s[base..].char_indices() {")
        add("        let add = match slot(ch) {")
        add("            Some(g) => u32::from(g.advance_px),")
        add("            None => 0,")
        add("        };")
        add("        if width + add + marker > box_px {")
        add("            break;")
        add("        }")
        add("        width += add;")
        add("        cut = base + i + ch.len_utf8();")
        add("    }")
        add("    Elision { text: &s[..cut], ellided: true }")
        add("}")
        add("")
        add("/// One text element's answer before any cell is written: what to draw, and the two verdicts")
        add("/// §5.5 refuses to let a caller merge. This is the front door a draw path in `surface.rs`")
        add("/// calls (2a PR-(3)), and it is one door on purpose — there is no second measurement path")
        add("/// here, `measure` and `unwritten` are the same ones the format legs above audit.")
        add("#[derive(Debug, Clone, Copy)]")
        add("pub struct Fit<'a> {")
        add("    /// What to draw, per §5.5's one-line rule.")
        add("    pub draw: Elision<'a>,")
        add("    /// Characters with no slot in this cut. Non-zero means part of the line cannot be drawn")
        add("    /// at all, which is a different failure from overflowing and is not fixed by eliding —")
        add("    /// the string is cut by width as any other would be.")
        add("    pub unwritten: usize,")
        add("    /// The width the draw path will occupy: `measure(draw.text)` plus the marker when")
        add("    /// `draw.ellided`. Never above `box_px` for a box the bake accepted, which is the")
        add("    /// invariant `fit_never_asks_for_more_than_the_box` checks.")
        add("    pub width_px: u32,")
        add("}")
        add("")
        add("#[must_use]")
        add("pub fn fit(s: &str, box_px: u32, keep: usize) -> Fit<'_> {")
        add("    let draw = elide(s, box_px, keep);")
        add("    Fit {")
        add("        unwritten: unwritten(s),")
        add("        width_px: measure(draw.text) + if draw.ellided { measure(ELLIPSIS) } else { 0 },")
        add("        draw,")
        add("    }")
        add("}")
        add("")
        add("/// §5.5's rule, pinned across its two implementations. Each row is (the string, the box in")
        add("/// px, the protected prefix length, the prefix this tool's Python elision derives for it,")
        add("/// whether the marker follows that prefix, and what the box is). The last column is not a")
        add("/// joke about provenance: the boxes are the widths the selected variant declares, except the")
        add("/// ones labelled as narrower than the marker, which no variant declares and which the bake")
        add("/// refuses (`title-fit`). The two middle columns come from `album_bake.FaceMetrics.elide")
        add("///` — the copy of the rule that decides at bake time. The test below runs the copy that")
        add("/// draws at runtime and compares, because a rule duplicated and never compared is two rules")
        add("/// with one name (R11's argument, applied to a rule instead of a number).")
        add("#[rustfmt::skip]")
        add("pub const ELISION_CASES: &[(&str, u32, usize, &str, bool, &str)] = &[")
        for text, box_px, keep, prefix, ellided, label in self._elision_cases():
            add(
                f"    ({_rs_str(text)}, {box_px}, {keep}, {_rs_str(prefix)}, "
                f"{'true' if ellided else 'false'}, {_rs_str(label)}),"
            )
        add("];")
        add("")
        add("#[cfg(target_arch = \"arm\")]")
        add("pub mod vram {")
        add("    use super::*;")
        add("    use agb::display::tiled::{TileFormat, TileSet};")
        add("")
        add("    /// The atlas as a tileset. The whole of §5.5a's mechanical gotcha lives here, in the")
        add("    /// generated file, because this is the only place both of the things its `unsafe`")
        add("    /// depends on are visible: `GLYPHS`'s type and its length. A caller in `surface.rs`")
        add("    /// would have to re-derive both, and §5.5a's point is that neither is a fact a")
        add("    /// consumer should be left to assume.")
        add("    ///")
        add("    /// SAFETY: `GLYPHS` is a `static [u32; N]`, so its address is 4-byte aligned by")
        add("    /// construction — `TileSet::new`'s first requirement")
        add("    /// (agb-0.25.0/src/display/tiled/vram_manager.rs:153) and the same reason")
        add("    /// `ui_probe.rs:95` reaches for `#[repr(align(4))]` in the one case where it writes a")
        add("    /// tile by hand. And `N` is `ATLAS_TILES * 8` words, a whole number of 32-byte 4bpp")
        add("    /// tiles, which the asserts at the bottom of this file re-check in the compiler.")
        add("    #[must_use]")
        add("    pub fn tile_set() -> TileSet {")
        add("        let bytes: &[u8] = unsafe {")
        add("            core::slice::from_raw_parts(GLYPHS.as_ptr().cast::<u8>(), GLYPHS_WORDS * 4)")
        add("        };")
        add("        // The slice is derived from a `static`, so it outlives the tileset; `TileSet`")
        add("        // stores `&'static [u8]` for exactly that reason.")
        add("        unsafe { TileSet::new(bytes, TileFormat::FourBpp) }")
        add("    }")
        add("}")
        add("")
        add("// §5.5a step 5's two facts, and the tie to the other generated module. Asserts rather")
        add("// than comments because a generated file can be stale, and a host test that reads only")
        add("// one of the two modules cannot see it (R11).")
        add("const _: () = assert!(ATLAS_TILES == OUTLINED_GLYPHS * TILE_ROWS_PER_LINE);")
        add("const _: () = assert!(GLYPHS_WORDS == ATLAS_TILES * 8);")
        add("const _: () = assert!(GLYPHS.len() == GLYPHS_WORDS);")
        add("const _: () = assert!(SLOT.len() == SLOT_COUNT);")
        add("const _: () = assert!(SLOT_COUNT == (LAST_GLYPH as u32 - FIRST_GLYPH as u32 + 1) as usize);")
        add("")
        add(f"/// `FACES[{self.face_index}]` is the row of the layout table this atlas was generated")
        add("/// from — the same `[[face]]` entry, read in the same pass. The pool term in the layout")
        add("/// module and the atlas that answers it are the same number, or the build stops.")
        add("///")
        add("/// `str_eq` rather than `==`: two `&str`s are not comparable in a const context on this")
        add("/// toolchain (`PartialEq` is not a const trait), and an assertion that does not compile is")
        add("/// not an assertion. Byte-wise comparison is const, and says the same thing.")
        add("const fn str_eq(a: &str, b: &str) -> bool {")
        add("    let a = a.as_bytes();")
        add("    let b = b.as_bytes();")
        add("    if a.len() != b.len() {")
        add("        return false;")
        add("    }")
        add("    let mut i = 0;")
        add("    while i < a.len() {")
        add("        if a[i] != b[i] {")
        add("            return false;")
        add("        }")
        add("        i += 1;")
        add("    }")
        add("    true")
        add("}")
        add("")
        add(f"const _: () = assert!(str_eq(crate::layout::FACES[{self.face_index}].id, FACE_ID));")
        add(f"const _: () = assert!(crate::layout::FACES[{self.face_index}].raster == RASTER_PX);")
        add(f"const _: () = assert!(crate::layout::FACES[{self.face_index}].line_box_px == LINE_BOX_PX);")
        add(f"const _: () = assert!(crate::layout::FACES[{self.face_index}].atlas_tiles == ATLAS_TILES as u32);")
        add(f"const _: () = assert!(crate::layout::FACES[{self.face_index}].tile_rows_per_line == TILE_ROWS_PER_LINE as u32);")
        add(f"const _: () = assert!(crate::layout::FACES[{self.face_index}].outlined_glyphs == OUTLINED_GLYPHS as u32);")
        add("")
        add("/// The elements this atlas will be drawn into, by their index in the layout module's own")
        add("/// table — so the second fact §5.5a asks for is a statement about what the spec declares,")
        add("/// not about what this generator remembered. Every element named here must carry a row")
        add("/// pitch at least as tall as this line box, and must name this same line box: a text")
        add("/// element whose pitch is below the face's line box is `pitch-not-tileable` in the bake,")
        add("/// and an atlas that compiled anyway would be the second thing to lie about it (§5.0).")
        slots = ", ".join(str(i) for i in self.text_elements)
        add(f"const TEXT_ELEMENT_SLOTS: [usize; {len(self.text_elements)}] = [{slots}];")
        add("const fn pitch_ok() -> bool {")
        add("    let mut i = 0;")
        add("    while i < TEXT_ELEMENT_SLOTS.len() {")
        add("        let e = &crate::layout::ELEMENTS[TEXT_ELEMENT_SLOTS[i]];")
        add("        if e.row_px < LINE_BOX_PX || e.line_box_px != LINE_BOX_PX {")
        add("            return false;")
        add("        }")
        add("        i += 1;")
        add("    }")
        add("    true")
        add("}")
        add("const _: () = assert!(pitch_ok(), \"a declared text element's pitch is smaller than the line box this atlas draws\");")
        add("const _: () = assert!(TEXT_ELEMENT_SLOTS.len() > 0, \"a variant with no declared text element has nothing to check the atlas against\");")
        add("")
        self._emit_format_legs(add)
        self._emit_host_tests(add)
        return "\n".join(out) + "\n"

    #: The strings `ELISION_CASES` is built from, as (string, protected prefix length).
    #: The first string is one of the tracked fixture's own four titles —
    #: `album_pack_host.rs` pins those, so it is a real string this product ships. The rest are
    #: synthetic, and the generated module says so: §13's AP-3 rule for this row is that a long
    #: title is "2a's box with 2b's fixture", so the *box* in every row below is a box a variant
    #: declares and the over-long strings are inputs the rule exists for, not album claims.
    #:
    #: The third field picks which declared box a row is measured at, and the three kinds are there
    #: on purpose. `title` is the box the element that draws titles declares — the one §9.0's
    #: `title-fit` is a refusal about, and the one a generated table that only used the widest text
    #: box in the variant would never cover. `narrow` is the narrowest declared text box, which is
    #: where an unmapped code point and an over-long line meet (a wide box absorbs both and the
    #: distinction disappears). `wide` is the widest, which in the product layout is 2b's list
    #: window — included because a rule that only works at 160 px is not a rule.
    ELISION_PROBES: tuple[tuple[str, int, str], ...] = (
        ("Bioluminescent 1", 0, "title"),
        ("Bioluminescent Overdriven: An Unreasonably Long Album Title That Will Not Fit", 0, "title"),
        ("01 - Bioluminescent Overdriven: An Unreasonably Long Album Title", 5, "title"),
        ("Bioluminescent Overdriven: An Unreasonably Long Album Title That Will Not Fit", 0, "wide"),
        ("Now Playing \u266b An Unreasonably Long Title That Will Not Fit This Box", 0, "narrow"),
    )

    def _elision_cases(self) -> list[tuple[str, int, int, str, bool, str]]:
        """The rows of the module's `ELISION_CASES`: (input, box, keep, prefix, ellided, label).

        The `prefix`/`ellided` columns are what *this tool* derives, in Python, through
        `album_bake.FaceMetrics.elide` — the same call the bake makes when it decides what a
        title becomes and whether to refuse it. The module's own test then runs its Rust
        `elide` over the same rows and compares. That is the cross-implementation leg §5.5
        needs: the rule exists in two languages because one copy decides at bake time and the
        other draws at runtime, and a rule duplicated and never compared is two rules.

        Two of the rows use a box no variant declares (`marker_px - 1`), because the
        degenerate answer — nothing fits, not even the marker — is otherwise unrepresentable:
        every box the tracked spec declares is wider than three periods. The label column
        says which rows those are, so no reader takes a refused box for a shipped one.
        """
        advance = {g.code: g.advance_px for g in self.glyphs}
        marker_px = sum(advance.get(ord(c), 0) for c in ELLIPSIS)
        boxes = sorted({w for _id, w in self.text_element_boxes})
        if not boxes:
            # Not a §9.0 row and not a `GLYPH_REFUSALS` id: `atlas_for` refuses a variant with no
            # text element before it ever reaches this point, so arriving here with no box means a
            # caller bypassed that check, which is a bug in this file rather than a verdict about a
            # spec. Inventing a refusal id for it would put an unregistered id in the log.
            raise RuntimeError(f"{self.face_id}: no declared text element reached the atlas builder")
        wide, narrow = boxes[-1], boxes[0]

        def owns(box_px: int) -> str:
            """Which declared elements have this box — so a row names the box it was measured at.

            A table row that says only "a declared box" cannot be checked against the spec by a
            reader, and the module's own tests look the width up in `crate::layout::ELEMENTS`; the
            id belongs in the label so a row that stops corresponding to a real element is visible
            in the artifact, not only in a test failure.
            """
            ids = sorted({i for i, w in self.text_element_boxes if w == box_px})
            return f"the declared box of {'/'.join(ids)}, {box_px} px"

        rows: list[tuple[str, int, int, str, bool, str]] = []
        for text, keep, kind in self.ELISION_PROBES:
            if kind == "title":
                titled = sorted(w for i, w in self.text_element_boxes if "title" in i)
                if not titled:
                    # A variant that declares no element whose id says "title" has no title box,
                    # and §9.0's `title-fit` is a refusal about that element. Skipping the row is
                    # honest; measuring a title against the list window would be the row this
                    # table exists to avoid.
                    continue
                box = titled[0]
            else:
                box = narrow if kind == "narrow" else wide
            # A string this cut cannot draw goes to the narrow box even when asked for another:
            # a box wide enough for a long title hides whether an unmapped code point is being
            # treated as zero width or as absent. At the narrow box that is the difference between
            # two rows fitting and one.
            if any(ord(c) not in advance for c in text):
                box = narrow
            rows.append(self._elision_row(text, box, keep, owns(box)))
        for text, keep in (("01 - Bioluminescent", 5), ("Bioluminescent 1", 0)):
            rows.append(self._elision_row(text, marker_px - 1, keep,
                                          f"a box of {marker_px - 1} px: narrower than "
                                          f"{ELLIPSIS!r} at {marker_px} px, which the bake refuses"))
        return rows

    def _elision_row(self, text: str, box_px: int, keep: int, label: str) -> tuple[str, int, int, str, bool, str]:
        """One `ELISION_CASES` row, with the prefix and marker flag derived by the Python path."""
        rendered = self.metrics.elide(text, box_px, keep)
        if not rendered:
            return text, box_px, keep, "", False, label
        if rendered != text and rendered.endswith(ELLIPSIS):
            return text, box_px, keep, rendered[: -len(ELLIPSIS)], True, label
        return text, box_px, keep, rendered, False, label

    def _bit_order_fixture(self) -> tuple[Glyph, list[list[int]]]:
        """The one glyph this module transcribes as a bitmap, and that bitmap.

        Chosen for being the most asymmetric about the cell's vertical centre line in the
        cut, because that is what makes the leg a witness: a glyph symmetric about the
        middle of its cell reads the same with the x order reversed, and an assert about it
        would pass while proving nothing (§9.0's rule about a leg that has only ever
        passed). The rows come from `measured[code][\"grid\"]` — the fill grid, the same
        input `_pack` starts from — and are turned back into words by hand-written Rust.
        Two different paths over the same grid, which is the only way the bit order can be
        checked rather than restated.
        """
        best: tuple[int, Glyph] | None = None
        for g in self.glyphs:
            if not g.outline:
                continue
            x0, _y0, x1, _y1 = g.ink
            score = abs(x0 - (self.tile_px - x1))
            if best is None or score > best[0]:
                best = (score, g)
        assert best is not None, "a cut with no outlined glyph has no atlas to emit"
        glyph = best[1]
        grid = self.measured[glyph.code]["grid"]
        assert isinstance(grid, list)
        rows = []
        for row in range(self.tile_rows):
            for y in range(self.tile_px):
                line = grid[row * self.tile_px + y]
                mask = 0
                for x in range(self.tile_px):
                    if line[x]:
                        mask |= 1 << (self.tile_px - 1 - x)
                rows.append(mask)
        return glyph, rows

    def _emit_format_legs(self, add) -> None:
        """The format facts, asserted inside the artifact (§5.5a step 5).

        These are const fns, not tests: a const assert runs in the compiler, so it holds of
        the ROM build as well as of the host suite — and the ROM build is the one that
        cannot run a test.
        """
        add("")
        add("// The format of the bytes, asserted by the artifact rather than by the tool that wrote")
        add("// them. A const fn because a `const _: () = assert!` runs in the compiler: the ROM build")
        add("// gets these too, and the ROM build is the one that can never run a test.")
        add("")
        add("/// Every pixel of this atlas is at one of two levels: 0 (transparent) or 1. A 4bpp pixel")
        add("/// is one 4-bit index, and `_pack` writes 0 or 1 per pixel, so the index is 0 or 1 — the")
        add("/// ink is palette entry 1, which is the entry the chrome in")
        add("/// `examples/album_probe/src/bin/ui_probe.rs` sets for text. Anything else in here would")
        add("/// be a colour this module does not declare and no palette in the tree has a right to")
        add("/// expect. Checked across every word, not sampled.")
        add("const fn two_levels_only() -> bool {")
        add("    let mut w = 0;")
        add("    while w < GLYPHS.len() {")
        add("        let mut n = 0;")
        add(f"        while n < {self.tile_px} {{")
        add("            let v = (GLYPHS[w] >> (4 * n)) & 0xF;")
        add("            if v != 0 && v != 1 {")
        add("                return false;")
        add("            }")
        add("            n += 1;")
        add("        }")
        add("        w += 1;")
        add("    }")
        add("    true")
        add("}")
        add("const _: () = assert!(two_levels_only(), \"a pixel of this atlas is at a level beyond two\");")
        add("")
        glyph, rows = self._bit_order_fixture()
        add("/// Words per tile: one 32-bit word per row of the cell.")
        add(f"const WORDS_PER_TILE: usize = {self.tile_px};")
        add("")
        add(f"/// `{chr(glyph.code)!r}` (U+{glyph.code:04X}), as a bitmap rather than as words: one byte per")
        add("/// row of the line box, most significant bit first, so the row reads left to right the way")
        add("/// the cell does. Generated from the fill grid — the same input `_pack` starts from — and")
        add("/// reassembled into words below by hand-written Rust that follows the convention this")
        add("/// module documents. Two paths over one grid: the words are `_pack`'s, the reassembly is")
        add("/// written here, and if either side's idea of the bit order changed the two would stop")
        add("/// agreeing. The y order is pinned by the same fixture: this glyph's ink is in the bottom")
        add("/// half of the cell only, so a row order flipped about the baseline moves it.")
        add(f"const BIT_ORDER_CODE: u32 = {glyph.code};")
        add(f"const BIT_ORDER_ROWS: [[u8; {self.tile_px}]; {self.tile_rows}] = [")
        for row in range(self.tile_rows):
            masks = ", ".join(f"0b{m:08b}" for m in rows[row * self.tile_px:(row + 1) * self.tile_px])
            add(f"    [{masks}], // cell row {row}: the top {self.tile_px} px, then the next")
        add("];")
        add("")
        add("/// The word for one row of a cell, from a bitmap row: pixel `x` is the 4-bit index at bit")
        add(f"/// `4 * x` — byte `{self.tile_px // 2 - 1}` last, and within a byte the left pixel in the low nibble,")
        add("/// which is the GBA 4bpp byte order `_pack` writes and the one")
        add("/// `examples/album_probe/src/bin/ui_probe.rs:93` states for the tiles it writes by hand.")
        add("const fn word_from_bitmap_row(row_bits: u8) -> u32 {")
        add("    let mut word = 0u32;")
        add("    let mut x = 0;")
        add(f"    while x < {self.tile_px} {{")
        add(f"        if row_bits & (1 << ({self.tile_px - 1} - x)) != 0 {{")
        add("            word |= 1 << (4 * x);")
        add("        }")
        add("        x += 1;")
        add("    }")
        add("    word")
        add("}")
        add("")
        add("/// A bitmap row read the other way: the mirror image, which is what `_pack` would be")
        add("/// writing if it put the left pixel in the high nibble. `#[cfg(test)]` because its only")
        add("/// caller is the witness below: the leg that runs at compile time is `bit_order_matches`,")
        add("/// and a ROM does not pay for a function that only exists to prove a leg is not vacuous.")
        add("#[cfg(test)]")
        add("const fn mirror_row(row_bits: u8) -> u8 {")
        add("    let mut out = 0u8;")
        add(f"    let mut x = 0;")
        add(f"    while x < {self.tile_px} {{")
        add(f"        if row_bits & (1 << ({self.tile_px - 1} - x)) != 0 {{")
        add("            out |= 1 << x;")
        add("        }")
        add("        x += 1;")
        add("    }")
        add("    out")
        add("}")
        add("")
        add("/// The atlas is `slot(BIT_ORDER_CODE)`'s tiles reassembled from the bitmap above.")
        add("const fn bit_order_matches() -> bool {")
        add(f"    let idx = (BIT_ORDER_CODE - FIRST_GLYPH as u32) as usize;")
        add("    let tile0 = SLOT[idx].tile0 as usize;")
        add("    let mut row = 0;")
        add(f"    while row < {self.tile_rows} {{")
        add(f"        let mut y = 0;")
        add(f"        while y < {self.tile_px} {{")
        add("        let word = GLYPHS[(tile0 + row) * WORDS_PER_TILE + y];")
        add("        if word_from_bitmap_row(BIT_ORDER_ROWS[row][y]) != word {")
        add("            return false;")
        add("        }")
        add("        y += 1;")
        add("        }")
        add("        row += 1;")
        add("    }")
        add("    true")
        add("}")
        add("const _: () = assert!(bit_order_matches(), \"these words are not the bit order this module documents\");")
        add("")
        add("/// Which word in the cell a mirrored reading would produce, for the audit note: the host")
        add("/// test below asserts at least one of them differs, so the leg above is not a symmetric")
        add("/// glyph agreeing with itself. Same `#[cfg(test)]` reasoning as `mirror_row`.")
        add("#[cfg(test)]")
        add("const fn mirrored_would_differ() -> bool {")
        add(f"    let idx = (BIT_ORDER_CODE - FIRST_GLYPH as u32) as usize;")
        add("    let tile0 = SLOT[idx].tile0 as usize;")
        add("    let mut row = 0;")
        add(f"    while row < {self.tile_rows} {{")
        add(f"        let mut y = 0;")
        add(f"        while y < {self.tile_px} {{")
        add("        let word = GLYPHS[(tile0 + row) * WORDS_PER_TILE + y];")
        add("        if word_from_bitmap_row(mirror_row(BIT_ORDER_ROWS[row][y])) != word {")
        add("            return true;")
        add("        }")
        add("        y += 1;")
        add("        }")
        add("        row += 1;")
        add("    }")
        add("    false")
        add("}")
        add("")

    # -- the module's own host suite ---------------------------------------------------------

    def _emit_host_tests(self, add) -> None:
        """The host suite inside the artifact.

        The const fns above hold in the ROM build; these are the claims about the module's
        *API* — what a caller in `surface.rs` may assume — and they only run where a test
        can run. `make album-glyphs-test` runs them by name with a floor on the count, for
        the reason every other gate here has one: a filter that matched nothing is green.

        Each assert message is one string literal on one line. That is not style: a macro's
        message argument is a list of token trees, so `"a "` on one line and `"b"` on the
        next is two token trees and a parse error. The reasoning belongs in the doc comment
        above each test, where it can be as long as it needs to be.
        """
        add("#[cfg(test)]")
        add("mod tests {")
        add("    use super::*;")
        add("")
        add("    /// The bit-order leg above is a witness rather than a tautology: reading the same")
        add("    /// fixture's rows back to front changes at least one word. If this ever fails, this")
        add("    /// cut has gone symmetric about the middle of its cells and the leg above has")
        add("    /// stopped proving anything — pick an asymmetric glyph, which is what the choice in")
        add("    /// the generator is for.")
        add("    #[test]")
        add("    fn bit_order_leg_is_witnessed() {")
        add("        assert!(mirrored_would_differ(), \"U+{:04X} reads the same mirrored\", BIT_ORDER_CODE);")
        add("    }")
        add("")
        add("    /// §5.5a step 4 as the artifact states it: one cell-row per glyph with an outline,")
        add("    /// nothing shared, nothing reserved twice — which is what makes §5.1's text term a")
        add("    /// distinct bound rather than a sum of what a screen happens to draw.")
        add("    #[test]")
        add("    fn an_outlined_glyph_owns_its_cells_and_nothing_shares_one() {")
        add("        let mut cells: Vec<u16> = Vec::new();")
        add("        for g in SLOT.iter() {")
        add("            if g.tile0 == BLANK_TILE {")
        add("                continue;")
        add("            }")
        add("            for row in 0..TILE_ROWS_PER_LINE as u16 {")
        add("                cells.push(g.tile0 + row);")
        add("            }")
        add("        }")
        add("        assert_eq!(cells.len(), ATLAS_TILES, \"the slots address more cells than the atlas holds\");")
        add("        let mut sorted = cells;")
        add("        sorted.sort_unstable();")
        add("        let before = sorted.len();")
        add("        sorted.dedup();")
        add("        assert_eq!(sorted.len(), before, \"two glyphs share a cell — there is no dedupe pass here\");")
        add("        assert_eq!(sorted.first(), Some(&0), \"the packing starts at tile 0\");")
        add("        assert_eq!(sorted.last(), Some(&(ATLAS_TILES as u16 - 1)), \"and ends at the last tile\");")
        add("    }")
        add("")
        add("    /// The blank sentinel is agb's own blank tile, and this cut uses it exactly once: for")
        add("    /// the space character, which the cmap maps and `glyf` draws no contours for. A blank")
        add("    /// glyph answers blank for every row of its line box, which is the same answer either")
        add("    /// way — `tile_for` does not have to know which glyph it is asking about.")
        add("    #[test]")
        add("    fn the_blank_sentinel_is_agb_transparency_and_only_space_uses_it() {")
        add("        assert_eq!(BLANK_TILE, 0xFFFF, \"agb's transparent tile value\");")
        add("        assert_eq!(slot(' ').map(|g| g.tile0), Some(BLANK_TILE));")
        add("        for row in 0..TILE_ROWS_PER_LINE {")
        add("            assert_eq!(tile_for(' ', row), Some(BLANK_TILE));")
        add("        }")
        add("        assert_eq!(")
        add("            SLOT.iter().filter(|g| g.tile0 == BLANK_TILE).count(),")
        add("            1,")
        add("            \"this cut maps U+0020..U+007E, and exactly one of them has no outline\"")
        add("        );")
        add("    }")
        add("")
        add("    /// A character the cut has no glyph for is a `None` and a count, never a zero-width")
        add("    /// drawing: §5.5 keeps \"cannot be drawn\" and \"does not fit\" as two verdicts.")
        add("    #[test]")
        add("    fn a_character_outside_the_cut_has_no_slot_and_is_counted() {")
        add("        for ch in ['\\u{0}', '\\u{2014}', '\\u{00e9}'] {")
        add("            assert!(slot(ch).is_none(), \"U+{:04X} is outside the cut\", ch as u32);")
        add("            assert_eq!(unwritten(ch.to_string().as_str()), 1);")
        add("            assert_eq!(measure(ch.to_string().as_str()), 0, \"and contributes no width\");")
        add("        }")
        add("        assert_eq!(unwritten(\"a\\u{2014}b\"), 1, \"counted, not silently skipped\");")
        add("    }")
        add("")
        add("    /// `measure` is a sum of `hmtx` advances with no kerning, and it is a different")
        add("    /// number from `tilemap_px` for any string narrower than a cell per character — the")
        add("    /// two bounds §5.1's capacity column and §5.5's fit verdict are each about. They are")
        add("    /// not interchangeable, and a face this proportional never lets them coincide.")
        add("    #[test]")
        add("    fn measure_is_the_sum_of_advances_and_not_the_count_of_cells() {")
        add("        for text in [\"L7{\", \"Bioluminescent\", \"0123456789\"] {")
        add("            let want: u32 = text")
        add("                .chars()")
        add("                .map(|c| u32::from(slot(c).expect(\"in the cut\").advance_px))")
        add("                .sum();")
        add("            assert_eq!(measure(text), want, \"{}\", text);")
        add("        }")
        add("        let one = u32::from(slot('i').expect(\"in the cut\").advance_px);")
        add("        assert_eq!(measure(\"iiii\"), 4 * one);")
        add("        assert_eq!(tilemap_px(\"iiii\"), 4 * TILE_PX);")
        add("        assert!(measure(\"iiii\") < tilemap_px(\"iiii\"), \"a tilemap pays whole cells\");")
        add("    }")
        add("")
        add("    /// Only the rows of the line box are addressable, and they tile it exactly — the")
        add("    /// arithmetic `tile_for` is documented to do, and the reason §9.0's")
        add("    /// `pitch-not-tileable` row exists at all.")
        add("    #[test]")
        add("    fn only_the_rows_of_the_line_box_are_addressable() {")
        add("        assert_eq!(")
        add("            TILE_ROWS_PER_LINE * TILE_PX as usize,")
        add("            LINE_BOX_PX as usize,")
        add("            \"the cell rows tile the line box with no remainder\"")
        add("        );")
        add("        let first = slot('A').expect(\"in the cut\").tile0;")
        add("        for row in 0..TILE_ROWS_PER_LINE {")
        add("            assert_eq!(tile_for('A', row), Some(first + row as u16));")
        add("        }")
        add("        assert_eq!(tile_for('A', TILE_ROWS_PER_LINE), None, \"a third row is ink outside the box\");")
        add("        assert!(slot('\\u{2014}').is_none(), \"and a character outside the cut has no rows\");")
        add("    }")
        add("")
        add("    /// The cross-implementation leg. `ELISION_CASES`'s middle columns are what")
        add("    /// `album_bake.FaceMetrics.elide` derived, in Python — the copy of §5.5's rule that")
        add("    /// decides at bake time, when the bake prints an elided title or refuses a box that")
        add("    /// cannot hold one. This runs the copy that draws at runtime and compares it character")
        add("    /// for character. A rule written twice and never compared is two rules with one name.")
        add("    #[test]")
        add("    fn eliding_cuts_where_the_tool_that_wrote_this_file_said() {")
        add("        assert!(!ELISION_CASES.is_empty(), \"no elision cases: the rule is unpinned\");")
        add("        for (input, box_px, keep, prefix, ellided, _label) in ELISION_CASES {")
        add("            let got = elide(input, *box_px, *keep);")
        add("            assert_eq!(got.text, *prefix, \"eliding {input:?} into {box_px} px with keep={keep}\");")
        add("            assert_eq!(got.ellided, *ellided, \"whether the marker follows {input:?}\");")
        add("        }")
        add("    }")
        add("")
        add("    /// Maximality, which is the half of §5.5's sentence a lenient implementation gets")
        add("    /// wrong: the cut must be at the *last* whole glyph that fits, so the character the cut")
        add("    /// dropped is one that would not have fitted. An elision that gave up a glyph early")
        add("    /// would still satisfy the pin above — this is the leg that says it did not.")
        add("    #[test]")
        add("    fn an_elided_line_fits_and_one_more_glyph_would_not() {")
        add("        let mut elided_rows = 0;")
        add("        for (input, box_px, keep, _prefix, ellided, _label) in ELISION_CASES {")
        add("            if !*ellided {")
        add("                continue;")
        add("            }")
        add("            elided_rows += 1;")
        add("            let got = elide(input, *box_px, *keep);")
        add("            let marker = measure(ELLIPSIS);")
        add("            assert!(!got.text.is_empty(), \"an elided line with nothing on it: {input:?}\");")
        add("            assert!(measure(got.text) + marker <= *box_px, \"the elided line outgrew its box: {input:?}\");")
        add("            let rest = &input[got.text.len()..];")
        add("            let Some(next) = rest.chars().next() else {")
        add("                continue;")
        add("            };")
        add("            let adv = match slot(next) {")
        add("                Some(g) => u32::from(g.advance_px),")
        add("                None => 0,")
        add("            };")
        add("            assert!(")
        add("                measure(got.text) + adv + marker > *box_px,")
        add("                \"{next:?} would have fitted too, so this is not the last whole glyph: {input:?}\"")
        add("            );")
        add("        }")
        add("        assert!(elided_rows >= 2, \"the case set lost its over-long strings\");")
        add("    }")
        add("")
        add("    /// §5.5a's marker, as the artifact states it: three ASCII periods, every one of them a")
        add("    /// glyph this cut carries, and not the single character a typographer would write. The")
        add("    /// last assert is the second reason for that choice — §5.5a argues it in tiles (190 of")
        add("    /// them), and this face's cmap says a real ellipsis could not be drawn at all.")
        add("    #[test]")
        add("    fn the_marker_is_three_periods_and_this_cut_carries_every_one_of_them() {")
        add("        assert_eq!(ELLIPSIS, \"...\", \"§5.5a's marker is three ASCII periods\");")
        add("        assert_eq!(unwritten(ELLIPSIS), 0, \"a marker character with no slot would draw blank\");")
        add("        assert_eq!(")
        add("            measure(ELLIPSIS),")
        add("            3 * u32::from(slot('.').expect(\"a period is in the cut\").advance_px),")
        add("            \"three periods, three advances\",")
        add("        );")
        add("        assert!(slot('\\u{2026}').is_none(), \"a real ellipsis is not in this cut\");")
        add("    }")
        add("")
        add("    /// The empty answer from `elide` is the answer to a box the bake refuses, so no box")
        add("    /// this variant declares may produce it. Read from the layout module's own table, not")
        add("    /// from the widths the generator had when it wrote this file — the same reason")
        add("    /// `pitch_ok` reads `row_px` out of `crate::layout::ELEMENTS`.")
        add("    #[test]")
        add("    fn no_declared_text_box_is_narrower_than_the_marker() {")
        add("        assert!(!TEXT_ELEMENT_SLOTS.is_empty(), \"a variant with no text element has no box to check\");")
        add("        for i in TEXT_ELEMENT_SLOTS {")
        add("            let e = &crate::layout::ELEMENTS[i];")
        add("            assert!(")
        add("                e.w >= measure(ELLIPSIS),")
        add("                \"the {} box is {} px, below the {} px marker: no title could be drawn in it\",")
        add("                e.id,")
        add("                e.w,")
        add("                measure(ELLIPSIS),")
        add("            );")
        add("        }")
        add("    }")
        add("")
        add("    /// `fit` is the one entry point a draw path calls, and what it returns has to be")
        add("    /// internally consistent: the width it reports is the width of what it says to draw, and")
        add("    /// that is never more than the box it was asked about. The degenerate rows are skipped")
        add("    /// for the bound (a refused box has no width to respect) and counted, so the case set")
        add("    /// cannot quietly lose them.")
        add("    #[test]")
        add("    fn fit_never_asks_for_more_than_the_box_and_says_what_it_would_draw() {")
        add("        let mut refused_rows = 0;")
        add("        for (input, box_px, keep, _prefix, _ellided, label) in ELISION_CASES {")
        add("            let f = fit(input, *box_px, *keep);")
        add("            assert_eq!(f.width_px, measure(f.draw.text) + if f.draw.ellided { measure(ELLIPSIS) } else { 0 }, \"the reported width is the width of the answer: {input:?}\");")
        add("            if label.contains(\"narrower\") {")
        add("                refused_rows += 1;")
        add("                assert!(f.draw.text.is_empty() && !f.draw.ellided, \"a refused box should answer empty: {input:?}\");")
        add("                continue;")
        add("            }")
        add("            assert!(f.width_px <= *box_px, \"{input:?} would be drawn {0} px into a {1} px box\", f.width_px, *box_px);")
        add("        }")
        add("        assert!(refused_rows >= 1, \"the case set lost the box narrower than the marker\");")
        add("    }")
        add("")
        add("    /// §5.5's two verdicts stay two: a string with a character this cut has no glyph for")
        add("    /// still has a width, is still cut by width, and still reports the missing glyph. If")
        add("    /// eliding ever swallowed `unwritten` — or a blank ever counted as a fit — this is the")
        add("    /// leg that fails.")
        add("    #[test]")
        add("    fn eliding_never_hides_a_missing_glyph_behind_a_fit() {")
        add("        let mut unwritten_rows = 0;")
        add("        for (input, box_px, keep, _prefix, _ellided, _label) in ELISION_CASES {")
        add("            let f = fit(input, *box_px, *keep);")
        add("            assert_eq!(f.unwritten, unwritten(input), \"{input:?} lost its unwritten count\");")
        add("            if f.unwritten > 0 {")
        add("                unwritten_rows += 1;")
        add("                assert!(!f.draw.text.is_empty(), \"a string with unwritten characters was blanked rather than cut: {input:?}\");")
        add("            }")
        add("        }")
        add("        assert!(unwritten_rows >= 1, \"the case set lost the string with a character outside the cut\");")
        add("    }")
        add("}")
        add("")


    # -- the report ------------------------------------------------------------------------

    def report(self) -> list[str]:
        adv = [g.advance_px for g in self.glyphs]
        ink_w = [g.ink[2] - g.ink[0] for g in self.glyphs if g.outline]
        fixture, _rows = self._bit_order_fixture()
        fx_off = abs(fixture.ink[0] - (self.tile_px - fixture.ink[2]))
        return [
            f"  bit-order fixture: {chr(fixture.code)!r} (U+{fixture.code:04X}), its ink sits "
            f"{fx_off} px off the centre of its cell — the most asymmetric glyph in this cut, which "
            "is what makes the module's own read-back of the 4bpp bit order a witness"
            f"  face {self.face_id}: {_rel(self.metrics.path)} at {self.raster} px "
            f"(unitsPerEm {self.upem}, design size {self.metrics.design_px} px)",
            f"  cut: {len(self.codes)} codepoints mapped in U+{FIRST_GLYPH:04X}-U+{LAST_GLYPH:04X}, "
            f"{self.outlined} with an outline, {len(self.codes) - self.outlined} without one",
            f"  line box {self.line_box_px} px = {self.tile_rows} tile row(s) of {self.tile_px} px, "
            f"baseline at row {self.baseline_px}",
            f"  atlas: {self.atlas_tiles} tiles of {self.tile_px}x{self.tile_px} 4bpp = "
            f"{self.atlas_tiles * 32} bytes, one cell per glyph with an outline, nothing shared",
            f"  of those, {self.blank_tile_rows} tile rows came out blank — a dedupe pass could fold "
            "them, and §5.5a's baseline says one cell per glyph, so this tool reports the number and "
            "does not fold it",
            f"  even-odd and non-zero agree on {self.fill_rule_agrees} of {self.outlined} outlined "
            + ("glyphs, so the rule §5.5a step 3 names is not observable on this face — the ring "
               "--self-test draws is where the two rules part"
               if self.fill_rule_agrees == self.outlined else
               f"glyphs, and {self.outlined - self.fill_rule_agrees} of them differ: on this face the "
               "choice §5.5a step 3 makes is visible in the drawing, not only in the code"),
            f"  advances {min(adv)}-{max(adv)} px; ink {min(ink_w)}-{max(ink_w)} px wide against a "
            f"{self.tile_px} px cell; measured width and tilemap footprint differ for every string "
            "narrower than a cell per character",
            "  filled-ink bounds equal the outline's control bounds for all "
            f"{self.outlined} outlined glyphs (two walks: this tool's, and ControlBoundsPen's)",
        ]



# -------------------------------------------------------------------------------------------
# The atlas for a variant
# -------------------------------------------------------------------------------------------

def atlas_for(variant: str = "", spec_path: Path = DEFAULT_LAYOUT_SPEC) -> GlyphAtlas:
    """The atlas for the single face the selected variant's text elements declare."""
    _spec, variant_spec, facts = layout_for(variant, spec_path)
    if len(facts.face_metrics) != 1:
        raise _refusal(
            "atlas-face-ambiguous",
            f"variant {variant_spec.id}'s text elements declare {len(facts.face_metrics)} face(s) "
            f"({', '.join(facts.face_metrics) or 'none'}); §5.5a's baseline is one atlas for the one "
            "face a variant draws its text in, and a second face is a second atlas",
        )
    face_ids = list(facts.face_metrics)
    face_id = face_ids[0]
    metrics = facts.face_metrics[face_id]
    elements = tuple(
        i for i, element in enumerate(facts.elements) if element.kind == "text" and element.face == face_id
    )
    atlas = GlyphAtlas(metrics, face_index=face_ids.index(face_id), text_elements=elements,
                       text_element_boxes=tuple((facts.elements[i].id, facts.elements[i].w)
                                               for i in elements))
    # The tie §5.1's pool sum and §5.5a's first fact both point at: what the
    # layout derivation counted, and what the outline walk actually produced. Two
    # paths over the same file, so neither is taken on trust — if they disagreed,
    # the pool term in the manifest would be a figure nothing could see.
    if atlas.atlas_tiles != metrics.outlined * atlas.tile_rows:
        raise _refusal(
            "audit",
            f"face {face_id}: the layout derivation counts {metrics.outlined} outlined glyphs x "
            f"{atlas.tile_rows} tile rows = {metrics.outlined * atlas.tile_rows} tiles; the "
            f"outline walk produced {atlas.atlas_tiles}",
        )
    return atlas


def emit(variant: str = "", spec_path: Path = DEFAULT_LAYOUT_SPEC, path: Path = ATLAS_RS) -> tuple[GlyphAtlas, str]:
    """Generate the module and write it. `make album-bake` calls this; `--emit` is the same call."""
    atlas = atlas_for(variant, spec_path)
    text = atlas.rust_text()
    write_atomic(path, text.encode("utf-8"))
    return atlas, text


def read_glyph_module_text(text: str, where: str) -> dict[str, object]:
    """A generated `src/glyphs.rs`, read back as facts rather than trusted.

    `album_bake.read_layout_module_text`'s rule for the other generated module:
    read back only the numbers another artifact's claim depends on — the face id,
    the tile count (§5.1's pool term), the line box (every text element's pitch)
    and the raster.
    """
    facts: dict[str, object] = {}
    for line in text.splitlines():
        line = line.strip()
        if not line.startswith("pub const ") or "=" not in line:
            continue
        name = line[len("pub const ") : line.index(":")].strip()
        value = line.split("=", 1)[1].strip().rstrip(";").strip()
        if value.startswith('"') and value.endswith('"'):
            facts[name] = value[1:-1]
            continue
        try:
            facts[name] = int(value, 0)
        except ValueError:
            continue
    missing = {"FACE_ID", "ATLAS_TILES", "LINE_BOX_PX", "RASTER_PX", "GLYPHS_WORDS"} - set(facts)
    if missing:
        raise _refusal("audit", f"{where}: the generated module does not declare {', '.join(sorted(missing))}")
    return facts


def read_glyph_module(path: Path = ATLAS_RS) -> dict[str, object]:
    if not path.exists():
        raise _refusal(
            "audit",
            f"{_rel(path)} does not exist — the bake generates it (§5.5a step 5), and a tree with a "
            "layout module whose pool term names a face but no atlas to answer for it is a sum with "
            "nothing behind it",
        )
    return read_glyph_module_text(path.read_text(), _rel(path))


def check(variant: str = "", spec_path: Path = DEFAULT_LAYOUT_SPEC, path: Path = ATLAS_RS) -> bool:
    """Is the module on disk byte-identical to what this run derives? (§5.5a's `--check`.)"""
    fresh = atlas_for(variant, spec_path).rust_text()
    if not path.exists():
        print(f"album_glyphs: {_rel(path)} does not exist, so there is nothing to check")
        return False
    on_disk = path.read_text()
    if on_disk == fresh:
        print(
            f"album_glyphs: {_rel(path)} is byte-identical to a fresh generation "
            f"({len(fresh.splitlines())} lines)"
        )
        return True
    disk_lines, fresh_lines = on_disk.splitlines(), fresh.splitlines()
    first = next((i for i, (a, b) in enumerate(zip(disk_lines, fresh_lines)) if a != b),
                 min(len(disk_lines), len(fresh_lines)))
    print(
        f"album_glyphs: {_rel(path)} is NOT what this run derives — first difference at line "
        f"{first + 1}: on disk {(disk_lines[first] if first < len(disk_lines) else '<eof>')!r}, "
        f"fresh {(fresh_lines[first] if first < len(fresh_lines) else '<eof>')!r}"
    )
    print(
        f"  ({len(disk_lines)} lines on disk, {len(fresh_lines)} from this run) — the module is "
        "generated (§5.5a step 5); `make album-bake` regenerates it"
    )
    return False


# -------------------------------------------------------------------------------------------
# --self-test
# -------------------------------------------------------------------------------------------

def _face_spec(spec_path: Path, face_id: str, *, raster: int | None = None) -> FaceSpec:
    """One declared face, asked differently.

    Copying every other field verbatim is the point: nothing about the measurement
    should depend on which field the spec happened to carry, and a synthetic face
    should differ from the declared one in exactly the one way the leg is about.
    """
    import tomllib

    doc = tomllib.loads(spec_path.read_text())
    entry = dict(next(f for f in doc["face"] if f["id"] == face_id))
    if raster is not None:
        entry["raster"] = raster
    return FaceSpec(entry, 0)


class _StandinMetrics:
    """The face description `GlyphAtlas` reads, built directly, for three legs.

    Every real path goes through `album_bake.FaceMetrics`, and this tool's own
    self-test does too — but its first rule is that the file is *tracked in git*,
    and a font invented to trip an error path is not. Such a leg would answer
    `face-not-resolvable` and prove nothing about the branch it was aimed at, so
    those three describe the face by hand instead. Only the fields `GlyphAtlas`
    actually reads are here, which is itself the statement of what this tool
    depends on.
    """

    def __init__(
        self, face_id: str, path: Path, *, raster: int, upem: int, tile_px: int, ascender: int, line_box_px: int
    ) -> None:
        self.face = type("Face", (), {"id": face_id, "file": _rel(path)})
        self.path = path
        self.raster, self.upem, self.tile_px = raster, upem, tile_px
        self.ascender, self.line_box_px, self.design_px = ascender, line_box_px, upem
        self.outlined = 0


def _shrunken_hhea_face(spec_path: Path, face_id: str) -> Path:
    """The tracked cut with `hhea`'s box pulled in around it.

    Not a font invented for the leg: the same outlines and the same metrics, with a
    declared line box too small for them. That is the state
    `atlas-glyph-outside-cell` exists for, and it is a state a real face reaches
    when someone edits a spec entry rather than a font.
    """
    font = TTFont(str(_face_spec(spec_path, face_id).path()))
    font["hhea"].ascender, font["hhea"].descender, font["hhea"].lineGap = 700, -100, 0
    path = Path(tempfile.mkdtemp(prefix="album-glyphs-self-test-")) / "shrunken-box.ttf"
    font.save(str(path))
    return path


def self_test() -> int:
    if TTFont is None:
        print("album-glyphs-self-test: fontTools is not importable, so no face can be read here")
        return 1
    failures: list[str] = []
    legs = 0
    print("album-glyphs-self-test: the atlas tool's own rules, asserted by what they are")
    print(f"  {'what':58} {'expected':>16}  {'derived':>16}  verdict   <- where the expected number is from")

    def leg(what: str, want: str, got: str, ok: bool, source: str) -> None:
        nonlocal legs
        legs += 1
        print(f"  {what:58} {want:>16}  {got:>16}  {'ok' if ok else 'FAIL'}   <- {source}")
        if not ok:
            failures.append(f"{what}: expected {want} ({source}), this tool derives {got}")

    def fires(what: str, rule: str, call) -> None:
        nonlocal legs
        legs += 1
        try:
            call()
            got, ok = "no refusal", False
        except Refusal as exc:
            got, ok = exc.rule, exc.rule == rule
        print(f"  {what:58} {rule:>16}  {got:>16}  {'ok' if ok else 'FAIL'}")
        if not ok:
            failures.append(f"{what}: expected refusal {rule}, got {got}")

    atlas = atlas_for("")
    m = atlas.metrics

    # §5.5a step 5's first fact, and the tie to the layout generator's count.
    leg("atlas tiles", str(m.outlined * atlas.tile_rows), str(atlas.atlas_tiles),
        atlas.atlas_tiles == m.outlined * atlas.tile_rows, "album_bake FaceMetrics.outlined x line box")
    leg("glyphs with outlines", str(m.outlined), str(atlas.outlined),
        atlas.outlined == m.outlined, "cmap intersect glyf, counted twice over")
    leg("line box", str(m.line_box_px), str(atlas.line_box_px),
        atlas.line_box_px == m.line_box_px, "hhea triple, both paths")
    leg("tile rows divide the line box", "0", str(atlas.line_box_px % atlas.tile_px),
        atlas.line_box_px % atlas.tile_px == 0, "the same fact §9.0's pitch row refuses")
    leg("baseline is inside the cell", "yes", str(atlas.baseline_px <= atlas.line_box_px),
        atlas.baseline_px <= atlas.line_box_px, "hhea.ascender at the declared raster")
    cells = sum(len(g.tiles) for g in atlas.glyphs if g.outline)
    leg("one cell per glyph, nothing shared", str(atlas.atlas_tiles), str(cells),
        cells == atlas.atlas_tiles and len({g.tile0 for g in atlas.glyphs if g.outline}) == atlas.outlined,
        "§5.5a step 4: no dedupe pass, and no two glyphs share a cell")
    leg("every tile is 32 bytes of 4bpp", "32", str(max(len(t) for g in atlas.glyphs if g.outline for t in g.tiles)),
        all(len(t) == 32 for g in atlas.glyphs if g.outline for t in g.tiles), "8 rows x 4 bytes")
    leg("tile ids cover the atlas exactly once", str(atlas.atlas_tiles),
        str(len(range(atlas.atlas_tiles))),
        sorted(g.tile0 // atlas.tile_rows for g in atlas.glyphs if g.outline) == list(range(atlas.outlined)),
        "the packing order the module's asserts depend on")

    # The space rule: a mapped codepoint with no outline is the BLANK sentinel,
    # costs no tile, and the count drops by exactly that much.
    space = next(g for g in atlas.glyphs if g.code == 0x20)
    leg("U+0020 carries the BLANK sentinel", f"0x{BLANK_TILE:04X}", f"0x{space.tile0:04X}",
        space.tile0 == BLANK_TILE and not space.tiles, "§5.5a step 2")
    leg("mapped, and outlined by one fewer", f"{len(atlas.codes) - 1}", str(atlas.outlined),
        atlas.outlined == len(atlas.codes) - 1, "cmap maps 0x20; glyf gives it no contours")

    # The fill rule, and the ±1 class of bug. `_walk` compares the filled pixels'
    # bounds against the outline's control bounds for every glyph and raises on a
    # mismatch, so arriving here at all is the leg — which is only meaningful if
    # the comparison has something to bite on. A one-pixel stem and an enclosed
    # counter are the two cases an even-odd/non-zero mix-up or an off-by-one
    # changes, so their existence in the cut is asserted, not assumed.
    thin = [g for g in atlas.glyphs if g.outline and g.ink[2] - g.ink[0] == 1]
    counters = [g for g in atlas.glyphs if g.outline and g.contours > 1]
    leg("one-pixel stems exist to be lost", ">0", str(len(thin)), bool(thin), "the ink bounds")
    leg("glyphs drawn from several contours exist", ">0", str(len(counters)), bool(counters),
        "most of this cut is several rectangles, so the crossing count is accumulated across contours "
        "rather than within one — the case a per-contour fill would get wrong")
    leg("every outlined glyph put ink down", str(atlas.outlined),
        str(sum(1 for g in atlas.glyphs if g.outline and g.ink[2] > g.ink[0])),
        all(g.ink[2] > g.ink[0] for g in atlas.glyphs if g.outline), "the fill produced pixels")
    leg("every glyph's ink inside its cell", f"<= {atlas.tile_px}", str(max(g.ink[2] for g in atlas.glyphs)),
        all(g.ink[2] <= atlas.tile_px and g.ink[3] <= atlas.line_box_px for g in atlas.glyphs),
        "the cell and the line box")

    # The fill rule, on a ring drawn here rather than read out of a face.
    #
    # This cut cannot show what §5.5a step 3 decides. The report prints the count,
    # and on this face even-odd and non-zero draw every outlined glyph identically.
    # That is not luck and not a bug either: the face builds its ink from
    # rectangles that share edges but never cover the same pixel twice, so there is
    # no enclosed area for the two rules to disagree about. '#' is the single
    # exception measured in the cut — two contours covering three cells twice, and
    # wound in *opposite* directions, which is the construction on which the two
    # rules agree by design. So a claim about the rule has to be a claim about
    # `_filled`, tested on a ring: an outer square and an inner one, both wound the
    # same way, which is how 339 of this cut's 340 contours are wound.
    ring = [
        [(2 * x, 2 * y) for (x, y) in [(1, 1), (7, 1), (7, 7), (1, 7)]],
        [(2 * x, 2 * y) for (x, y) in [(3, 3), (5, 3), (5, 5), (3, 5)]],
    ]
    even = atlas._filled(ring)
    other = atlas._filled(ring, rule=NON_ZERO)
    counter = {(row, col)
               for row in range(atlas.line_box_px)
               for col in range(atlas.tile_px)
               if even[row][col] != other[row][col]}
    leg("a ring's counter stays empty under the rule this tool uses", "empty",
        "filled" if even[4][4] else "empty", not even[4][4],
        "a constructed ring: two squares, one inside the other, both wound alike")
    leg("and the other rule would have filled that counter", "filled",
        "filled" if other[4][4] else "empty", other[4][4],
        "the leg above is a witness only where the two rules can disagree at all")
    leg("the two rules disagree on the enclosed area and nowhere else", "the 4 px inside the inner square",
        f"{len(counter)} px", counter == {(3, 3), (3, 4), (4, 3), (4, 4)},
        "one geometry, two accumulations: the difference is the interior, never the frame")

    # measure(): the atlas's slots, against the *other* path this tree measures a
    # string with — `album_bake.FaceMetrics.measure_text`, in font units. If a
    # future change to either path moved one of them, this is the leg that says so.
    for text in ("Bioluminescent", "A", "0123456789", "a b", "The Lengthening Day"):
        units, _missing = m.measure_text(text)
        exact = units * atlas.raster % atlas.upem == 0
        want = units * atlas.raster // atlas.upem if exact else -1
        leg(f"measure({text!r})", f"{want} px", f"{atlas.measure(text)} px",
            exact and atlas.measure(text) == want, "FaceMetrics.measure_text (hmtx path) vs the slots")
    leg("unwritten names what has no glyph", "2", str(len(atlas.unwritten("a\u00e9\u4e2d"))),
        atlas.unwritten("a\u00e9\u4e2d") == "é中", "the cut's range, not the caller's")
    leg("a string narrower than its cells costs fewer px than it draws", "yes",
        f"{atlas.measure('iiii')} px vs {atlas.tilemap_px('iiii')} px",
        atlas.measure("iiii") < atlas.tilemap_px("iiii"),
        "the difference `text::fit` has to bound both ways")
    leg("the generator emits the elements it drew", ">0", str(len(atlas.text_elements)),
        len(atlas.text_elements) > 0, "layout_spec.toml [[variant.element]] kind=text")

    # §5.5's one-line elision, as this tool builds it. The rule itself is exercised where it
    # is implemented — `album_bake.FaceMetrics.elide` in the layout self-test, and the
    # generated `glyphs::elide` in the module's own suite — so these legs are about the two
    # things only this tool can know: that the two measurement paths it hands those
    # implementations are the same measurement, and that the case table it writes into the
    # module is well formed. A generated fixture nobody checked is a generated liability: it
    # passes forever whether or not it proves anything.
    marker_px = atlas.measure(ELLIPSIS)
    leg("the marker is three glyphs this cut has", "0", str(len(atlas.unwritten(ELLIPSIS))),
        not atlas.unwritten(ELLIPSIS) and marker_px > 0,
        "§5.5a's marker has to be drawable, and §5.5's fit verdict is worthless if it is not")
    leg("a real ellipsis is not in this cut", "1", str(len(atlas.unwritten("\u2026"))),
        atlas.unwritten("\u2026") == "\u2026",
        "the second reason §5.5a's marker is three periods: this face's cmap stops at U+007E")
    off_grid = [c for c in atlas.codes
                if (m.advance_units.get(chr(c), 0) * atlas.raster) % atlas.upem]
    leg("every advance lands on the pixel grid", "0", str(len(off_grid)), not off_grid,
        "the precondition for `measure` (px sums) and FaceMetrics (font units) to be one number — "
        "off the grid they are two numbers, and every cross-path leg above would be luck")
    boxes = sorted({w for _id, w in atlas.text_element_boxes})
    leg("no box this variant declares is narrower than the marker", f">= {marker_px}",
        f"min {boxes[0]}", boxes and boxes[0] >= marker_px,
        "the degenerate answer belongs to a box the bake refuses (`title-fit`), not to a shipped one")
    # The generated table, checked where it is written: each row's answer must fit, must be
    # maximal (one more glyph would not have fitted), and must be labelled as the kind of box
    # it is. A row whose "narrower than the marker" label went stale would make the module's
    # degenerate-case test count a fitting string as a refusal.
    rows = atlas._elision_cases()
    adv = {g.code: g.advance_px for g in atlas.glyphs}
    bad: list[str] = []
    for text, box_px, keep, prefix, ellided, label in rows:
        got = m.elide(text, box_px, keep)
        want = prefix + ELLIPSIS if ellided else prefix
        if got != want:
            bad.append(f"{text!r}: the row and the rule disagree")
            continue
        width = atlas.measure(prefix or "") + (marker_px if ellided else 0)
        if ellided:
            nxt = text[len(prefix)]
            if width > box_px or width + adv.get(ord(nxt), 0) + marker_px <= box_px:
                bad.append(f"{text!r}: cut is not maximal, or outgrew {box_px} px")
        elif prefix and width > box_px:
            bad.append(f"{text!r}: a whole string wider than its box")
        if ("narrower" in label) != (box_px < marker_px):
            bad.append(f"{text!r}: the box label does not match its width")
        elif "narrower" not in label and not any(
            f"{i}," in label for i, w in atlas.text_element_boxes if w == box_px
        ):
            # A row measured at a declared box must name which one. A table whose labels stopped
            # corresponding to real elements would still make the module's degenerate-box test
            # pass, which is the failure mode a generated fixture has by default.
            bad.append(f"{text!r}: the label does not name a declared element at {box_px} px")
    titled_boxes = {w for i, w in atlas.text_element_boxes if "title" in i}
    leg("the table meets the box the element that draws titles declares", "yes" if titled_boxes else "no title element",
        f"{sum(1 for _t, b, _k, _p, _e, _l in rows if b in titled_boxes)} rows",
        not titled_boxes or any(b in titled_boxes for _t, b, _k, _p, _e, _l in rows),
        "§9.0's `title-fit` is a refusal about that element; a table measured only at the widest "
        "text box in the variant would never reach the box 2a's title band draws")
    leg("the elision table this module ships is well formed", "0 rows wrong", f"{len(bad)} rows wrong",
        not bad and len(rows) >= 6,
        "each row fits, is cut at the last whole glyph, and is labelled for the box it uses")
    leg("and it keeps the three kinds of case", "3 kinds", "3 kinds",
        any(r[4] for r in rows) and any(not r[4] and r[3] for r in rows)
        and any(not r[4] and not r[3] for r in rows),
        "an over-long string, a string that fits whole, and a box that holds neither: a table "
        "missing one of them makes the module's count assertions vacuous")
    # A property of the rule, over every declared box: widening a box can never shorten the
    # answer, and narrowing it can never lengthen it. Off-by-one in a cut search shows up here
    # before it shows up in a screenshot.
    long_text = ("Bioluminescent Overdriven: An Unreasonably Long Album Title That Will Not "
                 "Fit This Box Anywhere")
    saw_degenerate = False
    monotonic = True
    wrong = ""
    for box in boxes:
        prev = None
        for probe in range(box, 0, -1):
            out = m.elide(long_text, probe)
            if atlas.measure(out) > probe:
                monotonic, wrong = False, f"{out!r} outgrew {probe} px"
                break
            if prev is not None and len(out) > prev:
                monotonic, wrong = False, f"{len(out)} chars at {probe} px, {prev} at {probe + 1} px"
                break
            prev = len(out)
            if not out:
                saw_degenerate = True
                break
        if not monotonic:
            break
    leg("a wider box never draws a shorter line", "monotonic", "monotonic" if monotonic else wrong,
        monotonic and saw_degenerate, f"every declared box, scanned down 1 px at a time; the "
        f"degenerate point was {'reached' if saw_degenerate else 'never reached'}")
    leg("eliding is not the same as refusing", "non-empty",
        "non-empty" if m.elide(long_text, boxes[0]) else "empty",
        bool(m.elide(long_text, boxes[0])),
        "the narrowest box this variant declares still gets a cut, not a blank — which is the "
        "difference between §5.5's rule and §9.0's row")

    # Error paths, each by id. These are this tool's own rules, not §9.0 rows —
    # the set is listed in GLYPH_REFUSALS so the count is checkable.
    spec = DEFAULT_LAYOUT_SPEC
    tracked = _face_spec(spec, "ark10").path()
    fires("a variant that draws text in two faces", "atlas-face-ambiguous",
          lambda: _two_face_fires(spec))
    fires("the same face at half its design size", "atlas-not-on-pixel-grid",
          lambda: GlyphAtlas(_StandinMetrics("ark10", tracked, raster=5, upem=atlas.upem,
                                             tile_px=8, ascender=1200, line_box_px=8)))
    fires("a face whose hhea box is smaller than its glyphs", "atlas-glyph-outside-cell",
          lambda: GlyphAtlas(_StandinMetrics("shrunken", _shrunken_hhea_face(spec, "ark10"), raster=10,
                                             upem=atlas.upem, tile_px=8, ascender=700, line_box_px=8)))
    fires("a declared face with no glyf to walk", "atlas-face-not-truetype",
          lambda: GlyphAtlas(_StandinMetrics("licence", REPO / "assets/fonts/OFL.txt", raster=10,
                                             upem=atlas.upem, tile_px=8, ascender=1200, line_box_px=16)))
    print(f"  {'a rasteriser that filled a counter wrong':58} {'(named, not fired)':>16}                  .   "
          "<- the tool's own bug path")
    named = {rule for rule, _how, _d in GLYPH_REFUSALS}
    fired_ids = {"atlas-face-ambiguous", "atlas-not-on-pixel-grid", "atlas-glyph-outside-cell",
                 "atlas-face-not-truetype"}
    for rule in sorted(named - fired_ids):
        print(f"  {rule:58} {'named, not fired':>16}                  .   "
              "<- this tool's own bug path; the comparison it guards runs for every glyph of the "
              "tracked cut above, and a second font invented to trip it would be a font in a tree "
              "that has enough of them")

    print()
    print("  the atlas, in short:")
    for line in atlas.report():
        print(line)
    print()
    print(f"legs: {legs}, failures: {len(failures)}")
    for failure in failures:
        print(f"  FAIL {failure}")
    if legs < 20:
        print(f"  FAIL this self-test collected {legs} legs, and 20 is the floor below which it is not "
              "the same set of claims it was when it was written")
        return 1
    return 1 if failures else 0


def _two_face_spec(spec: Path, budget: int | None = None) -> tuple[str, Path]:
    """The tracked spec with one of its non-product variant's text elements pointed at
    the other declared face. Returns that variant's id and the fixture path.

    A line-level edit, not a re-serialisation: the file's comments and every other
    number in it stay exactly as they are, so the only difference the leg sees is
    the one it is about.

    The variant is found by its `product = false` line rather than by name. Pointing
    a `product = true` variant at a `product_face = false` face is the spec
    contradicting its own flag, and `album_bake` answers *that* with `spec` long
    before any atlas rule is reached — which is the refusal this leg got when it
    edited variant B. Only a variant that may declare the face can get as far as the
    atlas rule.

    `budget` rewrites that variant's declared `tile_budget`; `_two_face_fires`
    explains why the number has to move at all.
    """
    lines = spec.read_text().splitlines()

    def name(line: str) -> str:
        return line.split("=", 1)[1].strip().strip('"')

    def blocks(marker: str) -> list[tuple[int, int]]:
        """Half-open line ranges of every `marker` block in the spec."""
        starts = [i for i, line in enumerate(lines) if line.strip() == marker]
        return list(zip(starts, starts[1:] + [len(lines)]))

    face_ids = [name(lines[i]) for lo, hi in blocks("[[face]]") for i in range(lo, hi)
                if lines[i].strip().startswith("id = ")]
    open_variants = [b for b in blocks("[[variant]]")
                     if any(lines[i].strip() == "product = false" for i in range(*b))]
    if not open_variants:
        raise AssertionError('the tracked spec has no `product = false` variant to declare two faces in')
    lo, hi = open_variants[0]
    declared = {name(lines[i]) for i in range(lo, hi) if lines[i].strip().startswith("face = ")}
    other = next(face for face in face_ids if face not in declared)
    variant_id = next(name(lines[i]) for i in range(lo, hi) if lines[i].strip().startswith("id = "))

    out = list(lines)
    where = max(i for i in range(lo, hi) if out[i].strip().startswith("face = "))
    out[where] = f'face = "{other}"'
    if budget is not None:
        money = next(i for i in range(lo, hi) if out[i].strip().startswith("tile_budget = "))
        out[money] = f"tile_budget = {budget}"
    path = Path(tempfile.mkdtemp(prefix="album-glyphs-two-face-")) / "two_face.toml"
    path.write_text("\n".join(out) + "\n")
    return variant_id, path


def _two_face_fires(spec: Path) -> None:
    """Watch `atlas-face-ambiguous` fire, with §5.1's pool bound lifted once.

    Two faces is two atlases, so a fixture that declares both is over its declared
    `tile_budget` before it is anything else, and `image-tiles-over-budget` — a
    refusal of the layout generator, not of this tool — answers first. The pool sum
    is read out of that refusal's own message and the fixture re-emitted one slot
    above it: the number comes from the tools, not from this file, so a later edit
    to a variant's art term moves the fixture with it instead of breaking the leg.
    """
    variant, path = _two_face_spec(spec)
    try:
        atlas_for(variant, path)
    except Refusal as exc:
        if exc.rule != "image-tiles-over-budget":
            raise
        _, path = _two_face_spec(spec, budget=_pool_sum(exc.detail) + 1)
        atlas_for(variant, path)


def _pool_sum(detail: str) -> int:
    """The `distinct pool sum N` an `image-tiles-over-budget` message prints."""
    marker = "the distinct pool sum "
    at = detail.index(marker) + len(marker)
    return int(detail[at:].split()[0])


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Generate the Album Player's glyph atlas (ALBUM-PLAYER.md §5.5a)")
    parser.add_argument("--emit", action="store_true", help="generate the module (the default)")
    parser.add_argument("--check", action="store_true", help="compare the module on disk with a fresh one")
    parser.add_argument("--print", action="store_true", help="print the report, writing nothing")
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--variant", default="", help="which layout variant to generate for")
    parser.add_argument("--spec", type=Path, default=DEFAULT_LAYOUT_SPEC)
    parser.add_argument("--out", type=Path, default=ATLAS_RS)
    args = parser.parse_args(argv)

    if args.self_test:
        return self_test()
    try:
        if args.check:
            return 0 if check(args.variant, args.spec, args.out) else 1
        if args.print:
            atlas = atlas_for(args.variant, args.spec)
            print(f"album_glyphs: {_rel(args.spec)} — the report for face {atlas.face_id} only, "
                  "nothing was written")
            for line in atlas.report():
                print(line)
            return 0
        atlas, text = emit(args.variant, args.spec, args.out)
        print(f"album_glyphs: wrote {_rel(args.out)} ({len(text.splitlines())} lines, untracked build "
              "output — §2.3's rule about generated items, and .gitignore names it)")
        for line in atlas.report():
            print(line)
        return 0
    except Refusal as exc:
        print(f"album_glyphs: refused ({exc.rule}): {exc.detail}")
        return 1


if __name__ == "__main__":
    sys.exit(main())
