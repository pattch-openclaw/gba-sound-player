#!/usr/bin/env python3
"""Generate the AP-0 probe's ART1 test card — blob, pin, and an optional PNG twin.

`examples/album_probe/ART1.md` is the pinned layout. This tool and
`examples/album_probe/src/art1.rs` are written against that file independently;
this one is the writer half, and its job is to *refuse* rather than to be
accommodating, for the same reason the reader refuses before it indexes.

The card is procedural, not a photograph: colour bars, a Bayer-4x4-dithered
luminance ramp, an 8-px tile grid and a diagonal edge. A test card shows the
things a probe has to show — tile seams, dither behaviour, palette seams — and it
is reproducible from this file alone. No image is read, no imaging library is
needed (stdlib only: `zlib`, `struct`, `hashlib`, `argparse`), and the same
arguments always produce byte-identical output: the palette is sorted, there is
no timestamp and no randomness anywhere.

Itself deterministic output is not the same as a correct blob, so the tool
re-reads what it wrote through its own parser (`audit_blob`, the `gafp_pack.py`
rule) before it writes the pin, and refuses if the two disagree.

    make album-probe-art                     regenerate the tracked blob + pin
    python3 scripts/album_probe_art.py --self-test
    python3 scripts/album_probe_art.py --png /tmp/probe_art1.png   # for a human
"""

from __future__ import annotations

import argparse
import hashlib
import struct
import sys
import tempfile
import zlib
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent

# --- the card ---------------------------------------------------------------
CARD_W = 120
CARD_H = 120
TILE = 8
TILE_BYTES = 32
HEADER_BYTES = 64
PALETTE_ENTRIES = 16
PALETTE_BANK_BYTES = PALETTE_ENTRIES * 2
MAX_PALETTE = 15
DEFAULT_DITHER = 128
VERSION = 1

BARS_TOP = 0
BARS_BOTTOM = 24
RAMP_TOP = 24
RAMP_BOTTOM = 96

# Multiples of 8 per channel on purpose: `Rgb15` is 5 bits per channel, so these
# round-trip exactly and the "off-palette is an error" refusal can never be
# papered over by a truncation.
WHITE = (0xF8, 0xF8, 0xF8)
YELLOW = (0xF8, 0xF8, 0x00)
CYAN = (0x00, 0xF8, 0xF8)
GREEN = (0x00, 0xF8, 0x00)
MAGENTA = (0xF8, 0x00, 0xF8)
RED = (0xF8, 0x00, 0x00)
BLUE = (0x00, 0x00, 0xF8)
GREY = (0x40, 0x40, 0x40)
BLACK = (0x00, 0x00, 0x00)
GRID = (0x88, 0x88, 0x88)
BAR_COLOURS = (WHITE, YELLOW, CYAN, GREEN, MAGENTA, RED, BLUE, GREY)
DIAGONAL = RED

# The 4x4 ordered dither matrix: pixel (x, y) is white when
#   level * 32 > 255 * (2*M[y%4][x%4] + 1)
# i.e. when the ramp's level beats that cell's threshold. Integer throughout, so
# the same arguments give the same card on any host.
BAYER4 = (
    (0, 8, 2, 10),
    (12, 4, 14, 6),
    (3, 11, 1, 9),
    (15, 7, 13, 5),
)

FNV_OFFSET = 0x811C9DC5
FNV_PRIME = 0x01000193
MASK32 = 0xFFFFFFFF


class Refusal(Exception):
    """A named refusal: the rule, and what it was checked against."""

    def __init__(self, rule: str, detail: str) -> None:
        super().__init__(f"{rule}: {detail}")
        self.rule = rule
        self.detail = detail


# --- the card, drawn --------------------------------------------------------


def draw_card(dither: int) -> list[list[tuple[int, int, int]]]:
    """Draw the test card in 24-bit RGB. Same `dither` in, same card out."""
    fb = [[BLACK] * CARD_W for _ in range(CARD_H)]

    # Colour bars: eight equal columns, each a flat colour.
    for y in range(BARS_TOP, BARS_BOTTOM):
        for x in range(CARD_W):
            fb[y][x] = BAR_COLOURS[x * len(BAR_COLOURS) // CARD_W]

    # Luminance ramp, ordered-dithered between black and white. The ramp is in
    # *density*, not in grey levels: every pixel is black or white, which is why
    # the card's colour count stays small and honest.
    for y in range(RAMP_TOP, RAMP_BOTTOM):
        for x in range(CARD_W):
            t = 255 * x // (CARD_W - 1)
            level = dither * t // 255
            threshold = 2 * BAYER4[y % 4][x % 4] + 1
            fb[y][x] = WHITE if level * 32 > 255 * threshold else BLACK

    # A diagonal edge across the ramp band: 45 degrees, two pixels wide, so a
    # tile seam that eats a column is visible as a step in the line.
    for y in range(RAMP_TOP, RAMP_BOTTOM):
        for x in (y, y + 1):
            if 0 <= x < CARD_W:
                fb[y][x] = DIAGONAL

    # The tile grid last, so every seam is drawn over whatever is under it.
    for y in range(CARD_H):
        for x in range(CARD_W):
            if x % TILE == 0 or y % TILE == 0:
                fb[y][x] = GRID

    return fb


def rgb15(colour: tuple[int, int, int]) -> int:
    r, g, b = colour
    return (r >> 3) | ((g >> 3) << 5) | ((b >> 3) << 10)


def rgb15_exact(colour: tuple[int, int, int]) -> bool:
    r5, g5, b5 = colour[0] >> 3, colour[1] >> 3, colour[2] >> 3
    return colour == (r5 << 3, g5 << 3, b5 << 3)


def colour_set(fb: list[list[tuple[int, int, int]]]) -> list[tuple[int, int, int]]:
    """The exact distinct-colour set, sorted: the palette is the set used."""
    return sorted({px for row in fb for px in row})


def quantise(
    fb: list[list[tuple[int, int, int]]],
    palette: list[tuple[int, int, int]],
) -> list[list[int]]:
    """RGB frame buffer -> palette indices. Off-palette is an error, never a
    nearest-match (ART1.md, and ALBUM-PLAYER.md §7's "how a cover quietly turns
    to mud")."""
    lookup = {}
    for index, colour in enumerate(palette):
        if not rgb15_exact(colour):
            raise Refusal(
                "colour not Rgb15-exact",
                f"{colour} needs more than 5 bits per channel",
            )
        lookup[colour] = index
    indices = []
    for row in fb:
        line = []
        for colour in row:
            try:
                line.append(lookup[colour])
            except KeyError:
                raise Refusal("off-palette colour", f"{colour} is not in the palette")
        indices.append(line)
    return indices


def pack_tiles(indices: list[list[int]], w: int, h: int) -> bytes:
    """Tiles in row-major order; a 4bpp tile is 8 words, row y is word y, and
    pixel x of a row is nibble x with the low nibble leftmost (ART1.md)."""
    out = bytearray()
    for tile_y in range(h // TILE):
        for tile_x in range(w // TILE):
            for row in range(TILE):
                word = 0
                for col in range(TILE):
                    pixel = indices[tile_y * TILE + row][tile_x * TILE + col]
                    word |= pixel << (4 * col)
                out += struct.pack("<I", word)
    return bytes(out)


def pack_map(w: int, h: int) -> bytes:
    """BG-map entries in tile order: `tile | hflip<<10 | vflip<<11 | bank<<12`.
    One bank, no flips: this probe's card is not a sprite sheet."""
    out = bytearray()
    for cell in range((w // TILE) * (h // TILE)):
        out += struct.pack("<H", cell)
    return bytes(out)


# --- the blob ---------------------------------------------------------------


def fnv1a32(*parts: bytes) -> int:
    value = FNV_OFFSET
    for part in parts:
        for byte in part:
            value ^= byte
            value = (value * FNV_PRIME) & MASK32
    return value


def build_blob(fb: list[list[tuple[int, int, int]]], dither: int, palette_max: int) -> tuple[bytes, dict]:
    """Frame buffer -> (blob bytes, pin fields). Refuses before it emits."""
    w, h = CARD_W, CARD_H
    colours = colour_set(fb)
    if len(colours) > palette_max:
        raise Refusal(
            "palette too large",
            f"{len(colours)} distinct colours > --palette-max {palette_max}",
        )
    if len(colours) > PALETTE_ENTRIES:
        raise Refusal(
            "palette too large",
            f"{len(colours)} distinct colours do not fit {PALETTE_ENTRIES} entries",
        )
    indices = quantise(fb, colours)

    n_banks = 1
    palettes = bytearray()
    for bank in range(n_banks):
        for entry in range(PALETTE_ENTRIES):
            at = bank * PALETTE_ENTRIES + entry
            colour = colours[at] if at < len(colours) else None
            palettes += struct.pack("<H", 0 if colour is None else rgb15(colour))

    tiles = pack_tiles(indices, w, h)
    bg_map = pack_map(w, h)
    checksum = fnv1a32(bytes(palettes), tiles, bg_map)

    source_bytes = b"".join(bytes(px) for row in fb for px in row)
    source_hash = fnv1a32(source_bytes)

    header = struct.pack(
        "<4sIIIIIIIII",
        b"ART1",
        VERSION,
        w,
        h,
        TILE_BYTES,
        n_banks,
        w // TILE,
        h // TILE,
        dither,
        source_hash,
    )
    header += struct.pack("<II", 0, 0)  # reserved, 8 bytes
    header += struct.pack("<I", checksum)
    header += struct.pack("<III", 0, 0, 0)  # reserved, 12 bytes
    assert len(header) == HEADER_BYTES, len(header)

    body = header + bytes(palettes) + tiles + bg_map
    blob = body + b"\x00" * (-len(body) % 4)

    pin = {
        "WIDTH_PX": w,
        "HEIGHT_PX": h,
        "TILE_COLS": w // TILE,
        "TILE_ROWS": h // TILE,
        "N_TILES": (w // TILE) * (h // TILE),
        "N_PALETTE_BANKS": n_banks,
        "DITHER_STRENGTH": dither,
        "PALETTE_COLOURS": len(colours),
        "SOURCE_HASH": source_hash,
        "BLOB_BYTES": len(blob),
        "CHECKSUM": checksum,
        "SHA256_HEX": hashlib.sha256(blob).hexdigest(),
    }
    return blob, pin


# --- the reader half: the tool audits what it wrote -------------------------


def parse_blob(blob: bytes) -> dict:
    """ART1.md's rules, in the same order `src/art1.rs` applies them: header
    fields, then length, then map semantics, then the checksum last (so a blob
    that is both wrong and stale reports the field that is wrong)."""
    if len(blob) < HEADER_BYTES:
        raise Refusal("file length inconsistent", f"{len(blob)} < {HEADER_BYTES}")
    magic, version, w, h, tile_bytes, banks, cols, rows, dither, source_hash = struct.unpack_from(
        "<4sIIIIIIIII", blob, 0
    )
    if magic != b"ART1":
        raise Refusal("bad magic", magic.decode("latin-1"))
    for offset in (40, 44, 52, 56, 60):
        value = struct.unpack_from("<I", blob, offset)[0]
        if value:
            raise Refusal("non-zero reserved word", f"{value:#010x} at {offset}")
    if version != VERSION:
        raise Refusal("version", f"{version} != {VERSION}")
    if tile_bytes != TILE_BYTES:
        raise Refusal("tile_bytes", f"{tile_bytes} != {TILE_BYTES}")
    if w % TILE or h % TILE:
        raise Refusal("dimension not a multiple of 8", f"{w}x{h}")
    if not (TILE <= w <= 240 and TILE <= h <= 160):
        raise Refusal("dimension out of range", f"{w}x{h}")
    if not 1 <= banks <= 4:
        raise Refusal("n_palette_banks", f"{banks} not in 1..=4")
    if cols != w // TILE or rows != h // TILE:
        raise Refusal("tile grid inconsistent", f"{cols}x{rows} vs {w}x{h}")

    n_tiles = cols * rows
    palette_bytes = banks * PALETTE_BANK_BYTES
    tile_section = n_tiles * TILE_BYTES
    map_section = n_tiles * 2
    unpadded = HEADER_BYTES + palette_bytes + tile_section + map_section
    expected = unpadded + (-unpadded % 4)
    if len(blob) != expected:
        raise Refusal("file length inconsistent", f"{len(blob)} != {expected}")
    if blob[unpadded:] != b"\x00" * (expected - unpadded):
        raise Refusal("file length inconsistent", "non-zero padding")

    palettes = blob[HEADER_BYTES : HEADER_BYTES + palette_bytes]
    tiles_at = HEADER_BYTES + palette_bytes
    tiles = blob[tiles_at : tiles_at + tile_section]
    map_at = tiles_at + tile_section
    bg_map = blob[map_at : map_at + map_section]

    for cell in range(n_tiles):
        entry = struct.unpack_from("<H", bg_map, cell * 2)[0]
        if entry & 0x03FF >= n_tiles:
            raise Refusal("map entry tile id", f"cell {cell} names {entry & 0x03FF}")
        if entry >> 12 >= banks:
            raise Refusal("map entry palette bank", f"cell {cell} names {entry >> 12}")

    stored = struct.unpack_from("<I", blob, 48)[0]
    computed = fnv1a32(palettes, tiles, bg_map)
    if stored != computed:
        raise Refusal("checksum mismatch", f"{stored:#010x} != {computed:#010x}")

    return {
        "width_px": w,
        "height_px": h,
        "tile_cols": cols,
        "tile_rows": rows,
        "n_tiles": n_tiles,
        "n_palette_banks": banks,
        "dither_strength": dither,
        "source_hash": source_hash,
        "checksum": stored,
        "blob_bytes": len(blob),
    }


def pin_source(pin: dict) -> str:
    """The `src/art_pin.rs` the blob is committed with. Plain `pub const` items,
    one per line, no trailing whitespace, so rustfmt has nothing to rewrite."""
    return (
        "//! Generated by `scripts/album_probe_art.py`; do not edit.\n"
        "//!\n"
        "//! The pin for `assets/probe_art1.bin` — what the blob must say. The host\n"
        "//! suite checks the committed bytes against these numbers, so the tool that\n"
        "//! made them is the witness and a hand-edit would be a claim nobody can\n"
        "//! regenerate. Regenerate with `make album-probe-art`.\n"
        "\n"
        f"pub const WIDTH_PX: u32 = {pin['WIDTH_PX']};\n"
        f"pub const HEIGHT_PX: u32 = {pin['HEIGHT_PX']};\n"
        f"pub const TILE_COLS: u32 = {pin['TILE_COLS']};\n"
        f"pub const TILE_ROWS: u32 = {pin['TILE_ROWS']};\n"
        f"pub const N_TILES: u32 = {pin['N_TILES']};\n"
        f"pub const N_PALETTE_BANKS: u32 = {pin['N_PALETTE_BANKS']};\n"
        f"pub const PALETTE_COLOURS: u32 = {pin['PALETTE_COLOURS']};\n"
        f"pub const DITHER_STRENGTH: u32 = {pin['DITHER_STRENGTH']};\n"
        f"pub const SOURCE_HASH: u32 = 0x{pin['SOURCE_HASH']:08x};\n"
        f"pub const BLOB_BYTES: usize = {pin['BLOB_BYTES']};\n"
        f"pub const CHECKSUM: u32 = 0x{pin['CHECKSUM']:08x};\n"
        f'pub const SHA256_HEX: &str = "{pin["SHA256_HEX"]}";\n'
    )


def write_png(path: Path, fb: list[list[tuple[int, int, int]]]) -> None:
    """A PNG twin for a human: 8-bit RGB, no palette, filter 0. `zlib` and
    `struct` only, which is why the blob never depends on this file."""
    raw = bytearray()
    for row in fb:
        raw.append(0)
        for pixel in row:
            raw += bytes(pixel)

    def chunk(tag: bytes, data: bytes) -> bytes:
        return (
            struct.pack(">I", len(data))
            + tag
            + data
            + struct.pack(">I", zlib.crc32(tag + data) & MASK32)
        )

    ihdr = struct.pack(">IIBBBBB", CARD_W, CARD_H, 8, 2, 0, 0, 0)
    png = (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", ihdr)
        + chunk(b"IDAT", zlib.compress(bytes(raw), 9))
        + chunk(b"IEND", b"")
    )
    path.write_bytes(png)


# --- commands ---------------------------------------------------------------


def generate(out: Path, pin_path: Path, png: Path | None, palette_max: int, dither: int) -> dict:
    fb = draw_card(dither)
    blob, pin = build_blob(fb, dither, palette_max)

    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_bytes(blob)

    # audit_blob: a blob is not accepted because it was written. Re-read the file
    # that landed on disk and put it through this tool's own reader.
    audited = parse_blob(out.read_bytes())
    for key, value in audited.items():
        if pin[key.upper()] != value:
            raise Refusal("audit disagrees with the pin", f"{key}: {pin[key]} != {value}")
    if hashlib.sha256(out.read_bytes()).hexdigest() != pin["SHA256_HEX"]:
        raise Refusal("audit disagrees with the pin", "sha256 changed between write and read")

    if png is not None:
        write_png(png, fb)

    pin_path.parent.mkdir(parents=True, exist_ok=True)
    pin_path.write_text(pin_source(pin), encoding="utf-8")

    vram = (pin["N_TILES"] * TILE_BYTES) + (pin["N_TILES"] * 2)
    print("album_probe_art: ART1 probe card")
    print(f"  blob        : {out} ({pin['BLOB_BYTES']} bytes)")
    print(
        f"  card        : {pin['WIDTH_PX']}x{pin['HEIGHT_PX']} px, "
        f"{pin['TILE_COLS']}x{pin['TILE_ROWS']} tiles ({pin['N_TILES']} tiles)"
    )
    print(
        f"  palette     : {pin['PALETTE_COLOURS']} colours in {pin['N_PALETTE_BANKS']} "
        f"bank of {PALETTE_ENTRIES} (Rgb15-exact)"
    )
    print(f"  dither      : Bayer 4x4, strength {pin['DITHER_STRENGTH']}/255")
    print(f"  vram price  : {vram} B (tiles + map, 4bpp)")
    print(f"  source_hash : 0x{pin['SOURCE_HASH']:08x}")
    print(f"  checksum    : 0x{pin['CHECKSUM']:08x} (FNV-1a/32 over palettes|tiles|map)")
    print(f"  sha256      : {pin['SHA256_HEX']}")
    print(f"  pin         : {pin_path}")
    if png is not None:
        print(f"  png         : {png} (for a human; not read by the ROM)")
    return pin


def self_test() -> int:
    """Cover the refusal paths and audit a round trip of this tool's own output."""
    failures = 0

    def check(name: str, fn) -> None:
        nonlocal failures
        try:
            fn()
        except (Refusal, AssertionError) as exc:
            failures += 1
            print(f"FAIL {name}: {exc}")
        else:
            print(f"PASS {name}")

    with tempfile.TemporaryDirectory() as tmp:
        tmpdir = Path(tmp)
        out = tmpdir / "probe_art1.bin"
        pin = tmpdir / "art_pin.rs"
        first = generate(out, pin, None, MAX_PALETTE, DEFAULT_DITHER)
        blob = out.read_bytes()

        def determinism() -> None:
            second_out = tmpdir / "again.bin"
            second_pin = tmpdir / "again_pin.rs"
            second = generate(second_out, second_pin, None, MAX_PALETTE, DEFAULT_DITHER)
            assert second_out.read_bytes() == blob, "same args produced different bytes"
            assert second_pin.read_text() == pin.read_text(), "pin differs between runs"
            assert second == first

        def round_trip() -> None:
            parsed = parse_blob(blob)
            assert parsed["blob_bytes"] == first["BLOB_BYTES"]
            assert parsed["checksum"] == first["CHECKSUM"]
            assert parsed["n_tiles"] == first["N_TILES"]
            assert parsed["width_px"] == CARD_W and parsed["height_px"] == CARD_H

        def palette_too_large() -> None:
            try:
                build_blob(draw_card(DEFAULT_DITHER), DEFAULT_DITHER, 4)
            except Refusal as exc:
                assert exc.rule == "palette too large", exc
            else:
                raise AssertionError("a 4-colour budget was accepted")

        def off_palette() -> None:
            try:
                quantise([[BLACK, (1, 2, 3)]], [BLACK])
            except Refusal as exc:
                assert exc.rule in ("off-palette colour", "colour not Rgb15-exact"), exc
            else:
                raise AssertionError("an off-palette colour was accepted")

        def not_rgb15_exact() -> None:
            try:
                quantise([[(1, 2, 3)]], [(1, 2, 3)])
            except Refusal as exc:
                assert exc.rule == "colour not Rgb15-exact", exc
            else:
                raise AssertionError("a non-Rgb15 colour was accepted")

        def mutate(offset: int, value: int, rule: str) -> None:
            bad = bytearray(blob)
            bad[offset] = value
            try:
                parse_blob(bytes(bad))
            except Refusal as exc:
                assert exc.rule == rule, f"expected {rule!r}, got {exc.rule!r}"
            else:
                raise AssertionError(f"mutation at {offset} was accepted")

        def bad_magic() -> None:
            mutate(0, ord("X"), "bad magic")

        def bad_version() -> None:
            mutate(4, 2, "version")

        def bad_tile_bytes() -> None:
            mutate(16, 16, "tile_bytes")

        def bad_reserved() -> None:
            mutate(40, 1, "non-zero reserved word")

        def bad_checksum() -> None:
            mutate(48, blob[48] ^ 0xFF, "checksum mismatch")

        def bad_length() -> None:
            try:
                parse_blob(blob[:-4])
            except Refusal as exc:
                assert exc.rule == "file length inconsistent", exc
            else:
                raise AssertionError("a short blob was accepted")

        check("determinism (same args => identical blob + pin)", determinism)
        check("round trip (parse what was written)", round_trip)
        check("refusal: palette too large", palette_too_large)
        check("refusal: off-palette colour", off_palette)
        check("refusal: colour not Rgb15-exact", not_rgb15_exact)
        check("refusal: bad magic", bad_magic)
        check("refusal: bad version", bad_version)
        check("refusal: bad tile_bytes", bad_tile_bytes)
        check("refusal: non-zero reserved word", bad_reserved)
        check("refusal: checksum mismatch", bad_checksum)
        check("refusal: file length inconsistent", bad_length)

    if failures:
        print(f"album_probe_art self-test: {failures} failure(s)")
        return 1
    print("album_probe_art self-test: PASS")
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Generate the AP-0 probe's ART1 test card.")
    parser.add_argument(
        "--out",
        default=str(REPO / "examples/album_probe/assets/probe_art1.bin"),
        help="blob path (default: the tracked probe blob)",
    )
    parser.add_argument(
        "--pin",
        default=str(REPO / "examples/album_probe/src/art_pin.rs"),
        help="pin module path (default: the tracked probe pin)",
    )
    parser.add_argument("--png", default=None, help="optional PNG twin, for a human")
    parser.add_argument(
        "--palette-max",
        type=int,
        default=MAX_PALETTE,
        help=f"refuse above this many distinct colours (default {MAX_PALETTE})",
    )
    parser.add_argument(
        "--dither",
        type=int,
        default=DEFAULT_DITHER,
        help=f"Bayer strength 0..=255, printed and recorded (default {DEFAULT_DITHER})",
    )
    parser.add_argument("--self-test", action="store_true", help="exercise the refusal paths")
    args = parser.parse_args(argv)

    if args.self_test:
        return self_test()
    if not 0 <= args.dither <= 255:
        print("album_probe_art: --dither must be 0..=255", file=sys.stderr)
        return 1
    if args.palette_max < 1:
        print("album_probe_art: --palette-max must be at least 1", file=sys.stderr)
        return 1

    png = None if args.png is None else Path(args.png)
    try:
        generate(Path(args.out), Path(args.pin), png, args.palette_max, args.dither)
    except Refusal as exc:
        print(f"album_probe_art: refused — {exc}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
