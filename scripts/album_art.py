#!/usr/bin/env python3
"""`album_art.py` — the album's cover -> `ART1` (2b PR-(4), ALBUM-PLAYER.md §7).

The probe's generator (`album_probe_art.py`) draws a procedural test card and is
stdlib-only because the probe may not depend on an image library. This tool is
the *product* one: it reads the album's real cover, downsamples it to a **declared**
box, derives a palette of at most `palette_max` colours by median cut, applies an
ordered (Bayer) dither between the two nearest palette entries, and emits the same
`ART1` layout the probe's reader already pins (`ART1.md`). The byte format is
deliberately shared: §7's conventions (4bpp tile order, map-entry encoding, Rgb15
banks, "off-palette is an error") are hardware facts, and a second format would
only be a second opportunity to disagree.

**Decoding uses `ffmpeg`, which the album bake already requires** for audio. It is
not an optional nicety: a JPEG is not a stdlib-decodable format, and §7's
"Pillow is not in the build environment" note is stale (the venv pins Pillow for
`album_glyphs.py`); using the tool that is already a dependency keeps the art
gate's dependency list identical to `album-bake`'s.

The writer refuses before it emits (the `gafp_pack.py` rule): a box that is not a
multiple of 8, a source too large for `palette_max` with quantisation off, a
`bleed` image larger than the screen, or an `ART1` section that does not add up.

    python3 scripts/album_art.py --cover assets/images/bioluminescent.jpeg \\
        --box 120x120 --out examples/album_player/assets/art1.bin \\
        --pin examples/album_player/src/art_pin.rs --report /tmp/art.txt
"""
from __future__ import annotations

import argparse
import hashlib
import struct
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(REPO / "scripts"))

TILE = 8
TILE_BYTES = 32
HEADER_BYTES = 64
PALETTE_ENTRIES = 16
VERSION = 1
MASK32 = 0xFFFFFFFF
FNV_OFFSET = 0x811C9DC5
FNV_PRIME = 0x01000193
MAX_W = 240
MAX_H = 160

#: Ordered dither matrices. `bayer2` is the probe's 2x1 pattern with a second row
#: so the same code can also offer `bayer4`; both are threshold matrices over
#: their own denominator, and the report prints which one ran.
BAYER = {
    "bayer2": ([[0, 2], [3, 1]], 4),
    "bayer4": (
        [
            [0, 8, 2, 10],
            [12, 4, 14, 6],
            [3, 11, 1, 9],
            [15, 7, 13, 5],
        ],
        16,
    ),
}


class Refusal(Exception):
    def __init__(self, rule: str, detail: str) -> None:
        super().__init__(f"{rule}: {detail}")
        self.rule = rule
        self.detail = detail


def rgb15(colour: tuple[int, int, int]) -> int:
    r, g, b = colour
    return (r >> 3) | ((g >> 3) << 5) | ((b >> 3) << 10)


def snap15(colour: tuple[int, int, int]) -> tuple[int, int, int]:
    """The colour the hardware can actually show: 5 bits per channel, expanded."""
    r, g, b = colour
    return ((r >> 3) << 3, (g >> 3) << 3, (b >> 3) << 3)


def fnv1a32(*parts: bytes) -> int:
    value = FNV_OFFSET
    for part in parts:
        for byte in part:
            value ^= byte
            value = (value * FNV_PRIME) & MASK32
    return value


def check_box(width: int, height: int) -> None:
    if width % TILE or height % TILE:
        raise Refusal("box not tileable", f"{width}x{height} is not a multiple of {TILE} px")
    if not (TILE <= width <= MAX_W and TILE <= height <= MAX_H):
        raise Refusal("box outside the screen", f"{width}x{height} is outside 8..={MAX_W}x{MAX_H}")


def decode(cover: Path, width: int, height: int) -> list[list[tuple[int, int, int]]]:
    """Decode + resize with ffmpeg. Square boxes from square covers need no crop;
    a non-square box keeps the centre crop, and the report prints the source."""
    if not cover.exists():
        raise Refusal("cover missing", str(cover))
    # `scale=w:h:flags=lanczos` fills the box exactly. A cover whose aspect ratio
    # differs is centre-cropped first so the picture is not stretched; the cover
    # this tree tracks is 3000x3000, so the crop is a no-op for it.
    vf = (
        f"scale={width}:{height}:flags=lanczos:force_original_aspect_ratio=increase,"
        f"crop={width}:{height}"
    )
    proc = subprocess.run(
        [
            "ffmpeg", "-v", "error", "-i", str(cover),
            "-vf", vf, "-frames:v", "1",
            "-f", "rawvideo", "-pix_fmt", "rgb24", "-",
        ],
        check=False,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    if proc.returncode != 0:
        raise Refusal("cover undecodable", proc.stderr.decode("utf-8", "replace").strip())
    raw = proc.stdout
    expected = width * height * 3
    if len(raw) != expected:
        raise Refusal("cover undecodable", f"ffmpeg returned {len(raw)} bytes, expected {expected}")
    return [
        [tuple(raw[(y * width + x) * 3 : (y * width + x) * 3 + 3]) for x in range(width)]
        for y in range(height)
    ]


def median_cut(fb: list[list[tuple[int, int, int]]], max_colours: int) -> list[tuple[int, int, int]]:
    """A weighted median cut over the exact colours, snapped to Rgb15.

    Deterministic: buckets keep insertion order, the split channel is chosen by
    range then channel order, and ties break on the sorted colour. The result is
    deduplicated after snapping, so two source colours in one 5-bit cell really
    are one palette entry.
    """
    counts: dict[tuple[int, int, int], int] = {}
    for row in fb:
        for colour in row:
            counts[colour] = counts.get(colour, 0) + 1
    buckets: list[list[tuple[tuple[int, int, int], int]]] = [sorted(counts.items())]
    while len(buckets) < max_colours:
        # Split the bucket with the largest channel range * count.
        best = None
        for index, bucket in enumerate(buckets):
            if len(bucket) < 2:
                continue
            ranges = [
                max(c[ch] for c, _ in bucket) - min(c[ch] for c, _ in bucket) for ch in range(3)
            ]
            weight = max(ranges) * sum(n for _, n in bucket)
            if best is None or weight > best[0]:
                best = (weight, index, ranges)
        if best is None:
            break
        _, index, ranges = best
        channel = ranges.index(max(ranges))
        bucket = buckets.pop(index)
        total = sum(n for _, n in bucket)
        running = 0
        split = 1
        for position, (_, n) in enumerate(bucket):
            running += n
            if running * 2 >= total:
                split = min(position + 1, len(bucket) - 1)
                break
        buckets.append(bucket[:split])
        buckets.append(bucket[split:])
    palette = []
    for bucket in buckets:
        total = sum(n for _, n in bucket)
        r = sum(c[0] * n for c, n in bucket) // total
        g = sum(c[1] * n for c, n in bucket) // total
        b = sum(c[2] * n for c, n in bucket) // total
        colour = snap15((r, g, b))
        if colour not in palette:
            palette.append(colour)
    if not palette:
        raise Refusal("empty palette", "the cover has no pixels")
    if len(palette) > PALETTE_ENTRIES:
        raise Refusal("palette too large", f"{len(palette)} colours do not fit {PALETTE_ENTRIES} entries")
    return palette


def nearest_two(
    colour: tuple[int, int, int], palette: list[tuple[int, int, int]]
) -> tuple[int, int, float]:
    """The two nearest palette entries and the normalised distance to the first."""
    distances = sorted(
        (sum((colour[i] - entry[i]) ** 2 for i in range(3)), index)
        for index, entry in enumerate(palette)
    )
    d0, i0 = distances[0]
    if len(distances) == 1:
        return i0, i0, 0.0
    d1, i1 = distances[1]
    if d1 == d0:
        return i0, i1, 0.0
    return i0, i1, (d0**0.5) / ((d0**0.5) + (d1**0.5))


def dither_indices(
    fb: list[list[tuple[int, int, int]]],
    palette: list[tuple[int, int, int]],
    dither: str,
) -> list[list[int]]:
    matrix, denominator = BAYER[dither]
    size = len(matrix)
    out = []
    for y, row in enumerate(fb):
        line = []
        for x, colour in enumerate(row):
            first, second, fraction = nearest_two(colour, palette)
            threshold = (matrix[y % size][x % size] + 0.5) / denominator
            line.append(second if fraction > threshold else first)
        out.append(line)
    return out


def pack_tiles(indices: list[list[int]], width: int, height: int) -> bytes:
    out = bytearray()
    for tile_y in range(height // TILE):
        for tile_x in range(width // TILE):
            for row in range(TILE):
                word = 0
                for col in range(TILE):
                    word |= (indices[tile_y * TILE + row][tile_x * TILE + col] & 0xF) << (4 * col)
                out += struct.pack("<I", word)
    return bytes(out)


def pack_map(width: int, height: int) -> bytes:
    # One palette bank, no flips: the ART1's `palette_bank` field is bank-relative.
    return b"".join(struct.pack("<H", cell) for cell in range((width // TILE) * (height // TILE)))


def build_blob(
    fb: list[list[tuple[int, int, int]]],
    palette: list[tuple[int, int, int]],
    indices: list[list[int]],
    width: int,
    height: int,
    dither: str,
    dither_strength: int,
) -> tuple[bytes, dict]:
    if len(palette) > PALETTE_ENTRIES:
        raise Refusal("palette too large", f"{len(palette)} > {PALETTE_ENTRIES} entries")
    for row in indices:
        for index in row:
            if not 0 <= index < len(palette):
                raise Refusal("off-palette index", f"index {index} with {len(palette)} entries")
    palettes = bytearray()
    for entry in range(PALETTE_ENTRIES):
        palettes += struct.pack("<H", rgb15(palette[entry]) if entry < len(palette) else 0)
    tiles = pack_tiles(indices, width, height)
    bg_map = pack_map(width, height)
    checksum = fnv1a32(bytes(palettes), tiles, bg_map)
    source_hash = fnv1a32(bytes(channel for row in fb for px in row for channel in px))
    header = struct.pack(
        "<4sIIIIIIIII",
        b"ART1",
        VERSION,
        width,
        height,
        TILE_BYTES,
        1,
        width // TILE,
        height // TILE,
        dither_strength,
        source_hash,
    )
    header += struct.pack("<II", 0, 0)
    header += struct.pack("<I", checksum)
    header += struct.pack("<III", 0, 0, 0)
    if len(header) != HEADER_BYTES:
        raise Refusal("header", f"{len(header)} bytes, expected {HEADER_BYTES}")
    # The file is four sections, in ART1.md's order:
    #   header 64 B + palettes 32 B/bank + tiles 32 B/cell + map 2 B/cell.
    # Header, palettes and tiles are multiples of 4. The map is 2 B/cell, so the
    # body is 0 or 2 bytes short of a multiple of 4 depending on whether the cell
    # count is even or odd. ART1.md rounds the file up to a multiple of 4 with
    # trailing zeros: a section after the map starts word-aligned, and the reader
    # derives the same padded length and refuses any other (`art1.rs:376-389`).
    #
    #   box       cells  body = 64+32+32c+2c  body % 4  pad  file
    #   128x128   256    8,800               0         0    8,800
    #   120x120   225    7,746               2         2    7,748
    #    96x96    144    4,992               0         0    4,992
    #   104x104   169    5,842               2         2    5,844
    #
    # `(-len) % 4` is the non-negative distance to the next multiple of 4 (Python's
    # `%` returns 0..3 for a positive modulus); for this format it is only ever
    # 0 or 2. The pad is not part of the checksum (palettes, tiles and map are);
    # it is structural only.
    body = header + bytes(palettes) + tiles + bg_map
    blob = body + b"\x00" * (-len(body) % 4)
    pin = {
        "WIDTH_PX": width,
        "HEIGHT_PX": height,
        "TILE_COLS": width // TILE,
        "TILE_ROWS": height // TILE,
        "N_TILES": (width // TILE) * (height // TILE),
        "N_PALETTE_BANKS": 1,
        "PALETTE_COLOURS": len(palette),
        "DITHER": dither,
        "BLOB_BYTES": len(blob),
        "SOURCE_HASH": source_hash,
        "CHECKSUM": checksum,
        "SHA256_HEX": hashlib.sha256(blob).hexdigest(),
    }
    return blob, pin


def parse_blob(blob: bytes) -> dict:
    """The writer's own audit, in the order `src/art1.rs` reads it."""
    if len(blob) < HEADER_BYTES or blob[:4] != b"ART1":
        raise Refusal("magic", "not an ART1 blob")
    _, version, width, height, tile_bytes, banks, cols, rows, dither, source_hash = struct.unpack(
        "<4sIIIIIIIII", blob[:40]
    )
    checksum = struct.unpack("<I", blob[48:52])[0]
    if version != VERSION or tile_bytes != TILE_BYTES or banks != 1:
        raise Refusal("header", "version/tile_bytes/banks disagree with this tool")
    if cols != width // TILE or rows != height // TILE:
        raise Refusal("header", "tile counts disagree with the dimensions")
    sections = banks * PALETTE_ENTRIES * 2 + cols * rows * TILE_BYTES + cols * rows * 2
    if len(blob) < HEADER_BYTES + sections:
        raise Refusal("length", f"{len(blob)} bytes, sections need {HEADER_BYTES + sections}")
    palettes = blob[HEADER_BYTES : HEADER_BYTES + banks * PALETTE_ENTRIES * 2]
    tiles_at = HEADER_BYTES + banks * PALETTE_ENTRIES * 2
    tiles = blob[tiles_at : tiles_at + cols * rows * TILE_BYTES]
    map_at = tiles_at + cols * rows * TILE_BYTES
    bg_map = blob[map_at : map_at + cols * rows * 2]
    if fnv1a32(palettes, tiles, bg_map) != checksum:
        raise Refusal("checksum", "the writer's own sections do not hash to its header")
    for cell in range(cols * rows):
        entry = struct.unpack("<H", bg_map[cell * 2 : cell * 2 + 2])[0]
        if (entry & 0x03FF) >= cols * rows or ((entry >> 12) & 0xF) >= banks:
            raise Refusal("map", f"cell {cell} names a tile or bank outside the blob")
    return {
        "width": width,
        "height": height,
        "tiles": cols * rows,
        "checksum": checksum,
        "source_hash": source_hash,
        "dither": dither,
    }


def write_pin(path: Path, pin: dict) -> None:
    lines = [
        "//! Generated by `scripts/album_art.py` — do not edit (2b PR-(4), §7).",
        "//! The tool's own audit of the `ART1` blob the app includes; the host gate",
        "//! re-reads the blob through the shipping parser and compares these fields.",
        "",
    ]
    for key, value in pin.items():
        if isinstance(value, str):
            lines.append(f'pub const {key}: &str = "{value}";')
        elif isinstance(value, bool):
            lines.append(f"pub const {key}: bool = {str(value).lower()};")
        else:
            lines.append(f"pub const {key}: usize = {value};")
    lines.append("")
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("\n".join(lines))


def report_lines(
    cover: Path, width: int, height: int, palette: list, pin: dict, bleed: bool
) -> list[str]:
    lines = [
        f"cover={cover}",
        f"box={width}x{height} tiles={width // TILE}x{height // TILE} n_tiles={pin['N_TILES']}",
        f"palette_colours={len(palette)} entries={PALETTE_ENTRIES} banks={pin['N_PALETTE_BANKS']}",
        f"dither={pin['DITHER']} source_hash={pin['SOURCE_HASH']:08x}",
        f"blob_bytes={pin['BLOB_BYTES']} checksum={pin['CHECKSUM']:08x} sha256={pin['SHA256_HEX']}",
    ]
    if bleed:
        hidden_rows = 4  # title 2 + chrome 2 of the 20 visible rows
        hidden = hidden_rows / 20
        lines.append(
            f"bleed=true occluded_rows={hidden_rows} of 20 visible "
            f"({hidden:.0%} of the image is under the bands; a declared safe area cut is a refusal)"
        )
    else:
        lines.append("bleed=false (region-fit box: no row is hidden by a band)")
    return lines


def generate(
    cover: Path, width: int, height: int, dither: str, palette_max: int, bleed: bool
) -> tuple[bytes, dict, list]:
    check_box(width, height)
    fb = decode(cover, width, height)
    palette = median_cut(fb, palette_max)
    indices = dither_indices(fb, palette, dither)
    blob, pin = build_blob(fb, palette, indices, width, height, dither, 128)
    parse_blob(blob)
    return blob, pin, palette


def self_test() -> int:
    failures = 0

    def check(what: str, ok: bool, why: str) -> None:
        nonlocal failures
        print(f"  {what:58} {'ok' if ok else 'FAIL'}   <- {why}")
        if not ok:
            failures += 1

    # A synthetic 16x16 image: four exact Rgb15 quadrants.
    fb = [
        [(0, 0, 0) if (x < 8) == (y < 8) else (248, 248, 248) for x in range(16)]
        for y in range(16)
    ]
    palette = median_cut(fb, 15)
    check("median cut returns the two synthetic colours", len(palette) == 2,
          "a two-colour image must not invent a third")
    indices = dither_indices(fb, palette, "bayer2")
    blob, pin = build_blob(fb, palette, indices, 16, 16, "bayer2", 128)
    check("the writer's blob re-reads", parse_blob(blob)["tiles"] == 4, "round-trip through parse_blob")
    check("two runs are byte-identical",
          build_blob(fb, palette, dither_indices(fb, palette, "bayer2"), 16, 16, "bayer2", 128)[0] == blob,
          "determinism is a gate, not an intention")
    many = sorted({(i * 8, 0, 0) for i in range(17)})
    for what, fn, rule in (
        ("a box off the tile grid", lambda: check_box(12, 8), "box not tileable"),
        ("a box past the screen", lambda: check_box(248, 8), "box outside the screen"),
        ("an index outside the palette",
         lambda: build_blob(fb, palette, [[15] * 16 for _ in range(16)], 16, 16, "bayer2", 128),
         "off-palette index"),
        ("a palette above the entry count",
         lambda: build_blob(fb, many, [[0] * 16 for _ in range(16)], 16, 16, "bayer2", 128),
         "palette too large"),
    ):
        try:
            fn()
            check(what, False, f"expected {rule}; it was accepted")
        except Refusal as exc:
            check(what, exc.rule == rule, f"refused by {exc.rule}")
    print()
    print(f"SELF-TEST {'PASS' if failures == 0 else 'FAILED'}")
    return 0 if failures == 0 else 1


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cover", type=Path)
    parser.add_argument("--box", default="120x120")
    parser.add_argument("--bleed", action="store_true")
    parser.add_argument("--dither", default="bayer2", choices=sorted(BAYER))
    parser.add_argument("--palette-max", type=int, default=15)
    parser.add_argument("--out", type=Path)
    parser.add_argument("--pin", type=Path)
    parser.add_argument("--report", type=Path)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args(argv)
    if args.self_test:
        return self_test()
    if args.cover is None or args.out is None:
        parser.error("--cover and --out are required (or --self-test)")
    try:
        width, height = (int(part) for part in args.box.lower().split("x", 1))
    except ValueError:
        parser.error(f"--box {args.box!r} is not WxH")
    if args.bleed:
        width, height = MAX_W, MAX_H
    if not (1 <= args.palette_max <= PALETTE_ENTRIES - 1):
        parser.error("--palette-max must be 1..=15 (entry 0 is the backdrop)")
    try:
        blob, pin, palette = generate(args.cover, width, height, args.dither, args.palette_max, args.bleed)
    except Refusal as exc:
        print(f"album_art: {exc}", file=sys.stderr)
        return 2
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_bytes(blob)
    if args.pin is not None:
        write_pin(args.pin, pin)
    lines = report_lines(args.cover, width, height, palette, pin, args.bleed)
    if args.report is not None:
        args.report.parent.mkdir(parents=True, exist_ok=True)
        args.report.write_text("\n".join(lines) + "\n")
    print("album_art: wrote", args.out, f"({pin['BLOB_BYTES']} B, {pin['N_TILES']} tiles, "
          f"{len(palette)} colours, dither {pin['DITHER']})")
    for line in lines:
        print("  " + line)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
