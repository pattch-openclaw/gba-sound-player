#!/usr/bin/env python3
"""Bake an album: config → per-track `GAFP` blobs → `ALBM1` → `src/assets.rs`.

The driver, never a second packer (ALBUM-PLAYER.md §9.0's tool table). Every
byte of audio is `scripts/gafp_pack.py`'s: this tool resolves the track list,
prepares each source into the product profile, shells to the packer with
`--master --emit-pin`, reads the packed container's own header back, and writes
the manifest the app reads. A non-zero exit from the packer is this tool's
refusal, with the child's message forwarded verbatim — the encode-profile guard
is *inherited*, not reimplemented.

Phase 1 (A3a) does exactly that and nothing else. The four refusals §9.0 names
are the whole verdict set, and they fire before any write lands:

  (a) a declared rate that is not the product clock (Q14);
  (b) frames outside the encode profile — inherited from `gafp_pack.py`, which
      refuses LPC at the product block size and closes its own audit before it
      writes;
  (c) `ALBM1`'s `n_samples` disagreeing with the packed container's
      `total_samples` (§4.3 rule 1: the manifest is authoritative, so the two
      files are cross-checked and a disagreement is a build failure);
  (d) the writer's own output re-read through the shipping reader
      (§4.3's `audit_blob` rule — `audit_blob` below is meta.rs's rule list at
      byte level, and the host gate parses the finished blob through meta.rs
      itself).

Phase 3 grows the same file: budget arithmetic, minutes-per-cart, `[art]`/`[viz]`
sections and the remaining §9 verdicts. It is never rewritten.

    make album-bake                      bake the tracked fixture
    make album-pack-check                bake twice + the four refusals + the host gate
    python3 scripts/album_bake.py --self-test
"""

from __future__ import annotations

import argparse
import hashlib
import io
import math
import os
import re
import shutil
import struct
import subprocess
import sys
import tempfile
import tomllib
import tokenize
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import album_prep
from album_prep import (
    PRODUCT_CLOCK_HZ,
    Refusal,
    extract,
    fold,
    normalise,
    probe,
    read_tags,
)

# §5.5's fit verdict and §9.0's face rows read a declared face's own tables, so
# the tool that refuses `pitch-not-tileable` has to be able to open a .ttf. The
# import is guarded rather than required: §9.0's `face-not-resolvable` row says
# what an unresolvable face means, and that answer is better than a traceback in
# the middle of a bake. `pyftsubset` (the same package) is what makes §5.5a's cut.
try:
    from fontTools.ttLib import TTFont
except ImportError:  # pragma: no cover - the refusal says this out loud
    TTFont = None

REPO = Path(__file__).resolve().parent.parent
GAFP_PACK = REPO / "scripts" / "gafp_pack.py"
ALBUM_ART = REPO / "scripts" / "album_art.py"
DEFAULT_ALBUM = REPO / "examples" / "album_player" / "album.toml"

# --- ALBM1, pinned here and in src/meta.rs ----------------------------------
#
# Both files are written against ALBUM-PLAYER.md §4.3 independently; this half is
# the writer, and its job is to refuse rather than to be accommodating.
MAGIC = b"ALBM"
GAFP_MAGIC = b"GAFP"
VERSION = 2
HEADER_BYTES = 64
TRACK_BYTES = 40
MAX_TRACKS = 32
POOL_LIMIT = 0x1_0000
#: §6's bar grid is a number the *layout variant* declares — §6: "the choice
#: belongs to the layout variant … The bake computes the reciprocal from the
#: variant's declared width and cell count". It is not a constant of this file
#: any more: the bake reads it out of the variant it is baking for and passes it
#: to the reciprocal, the writer and the audit, so a variant that shipped a
#: 20-cell grid cannot get a 32-cell reciprocal written into its manifest. That
#: is the disagreement §9.0's 2a row `bar-cells-disagree` names, and the reason
#: the host's own check and the pack's reciprocal are two halves of one
#: invariant rather than two numbers.
#:
#: `BAR_CELLS_REGISTERED` survives only for §6's worked example and 2c's sampled
#: invariants, which were measured on the 32-cell grid. The arithmetic does not
#: care what the width is — that invariance *is* 2a's claim, which is why the
#: counts 2c samples (an argument about the number of samples) do not move with
#: the grid — so `--self-test` exercises both grids and says which is which.
BAR_CELLS_REGISTERED = 32
#: v2's two fold offsets: 44 is FNV over `table ‖ pool` (verified at boot), 60 is
#: FNV over the asset region (verified by the post-audio audit). 54..60 stay
#: reserved, so the header stays the documented 64 B.
STRUCTURE_FNV_AT = 44
ASSETS_FNV_AT = 60
#: The bounded content tripwire's spans, in bytes: the first 32 KiB and the last
#: 4 KiB of each track's blob. `src/meta.rs`'s `BOOT_PREFIX_BYTES` /
#: `BOOT_SUFFIX_BYTES` are the same pair, restated here because the writer and the
#: reader are written against §4.3 independently.
CONTENT_PREFIX_BYTES = 32 * 1024
CONTENT_SUFFIX_BYTES = 4 * 1024
BACKENDS = {"stub": 0, "flac": 1}
FNV_OFFSET = 0x811C9DC5
FNV_PRIME = 0x01000193
MASK32 = 0xFFFFFFFF

#: R2's measured real-music bitrate on a 32 MiB cart (§14), the number the
#: census is quoted against. A comment, not a gate: Phase 3 compares for real.
R2_MINUTES_PER_CART = 12.3


class TrackConfig:
    """One `[[track]]`: its source, its optional window, its text."""
    def __init__(self, spec: dict, index: int) -> None:
        self.index = index
        self.file = str(spec.get("file", "")).strip()
        self.title = spec.get("title")
        self.artist = spec.get("artist")
        self.start_sample = spec.get("start_sample")
        self.end_sample = spec.get("end_sample")
        if not self.file:
            raise Refusal("config", f"[[track]] #{index} has no `file`")
        window = (self.start_sample, self.end_sample)
        if (window[0] is None) != (window[1] is None):
            raise Refusal(
                "config",
                f"[[track]] #{index} carries only one of start_sample/end_sample; a clip needs both",
            )
        if self.start_sample is not None:
            if not isinstance(self.start_sample, int) or not isinstance(self.end_sample, int):
                raise Refusal("config", f"[[track]] #{index}: start_sample/end_sample are integers")
            if self.end_sample <= self.start_sample:
                raise Refusal(
                    "config",
                    f"[[track]] #{index}: end_sample {self.end_sample} is not after start_sample {self.start_sample}",
                )

    def source(self) -> Path:
        """The source path, repo-root-relative like every other fixture path."""
        path = Path(self.file)
        return path if path.is_absolute() else REPO / path

    def window(self) -> tuple[int | None, int | None]:
        return (self.start_sample, self.end_sample)


def config_int(path: Path, field: str, value) -> int:
    """A config number that has to be an integer TOML value, refused by name.

    `int()` on a TOML float truncates silently, and on a string raises
    `ValueError` from somewhere in the reader; either way the message names
    Python's type instead of the config line that is wrong.
    """
    if isinstance(value, bool) or not isinstance(value, int):
        raise Refusal("config", f"{path}: {field} is {value!r}, an integer is required")
    return value


class Config:
    """The `[album]` / `[[track]]` / `[out]` / `[layout]` subset, as the bake reads it."""
    def __init__(self, path: Path) -> None:
        self.path = path
        with open(path, "rb") as handle:
            try:
                doc = tomllib.load(handle)
            except tomllib.TOMLDecodeError as exc:
                raise Refusal("config", f"{path}: {exc}") from exc
        unknown = sorted(set(doc) - {"album", "track", "out", "layout", "art"})
        if unknown:
            # D2: the bake reads these four sections and no others. Ignoring the
            # rest would make `album.toml` a config that lies about what it
            # controls. `[layout]` arrived with 2a (§9.1's A13 row: "the bake
            # step cannot generate a layout without being told which one");
            # `[viz]`, `[ui]` and `[game]` are Phase 3's. `[art]` was refused here
            # until 2b PR-(4) parsed it; the box list was 2a's and the box key is now
            # 2b's, as §11's note (iii) scoped it.
            raise Refusal(
                "config-section",
                f"{path}: section(s) {', '.join(unknown)} are not part of Phase 1's "
                "[album]/[[track]]/[out] subset. §9.0 grows this file in Phase 3; "
                "until then a section the bake silently ignored is a config that lies.",
            )
        album = doc.get("album") or {}
        out = doc.get("out") or {}
        layout = doc.get("layout") or {}

        # §5.0, in one field: the config *selects* a variant and never produces
        # geometry. Anything else in this section would be an ad-hoc geometry
        # knob, which is the thing §5.0's band re-rule exists to forbid, so the
        # section has exactly one key and an unknown one is refused by name.
        for key in sorted(set(layout) - {"variant"}):
            raise Refusal(
                "config",
                f"{path}: [layout] {key} is not a field of that section; `variant` is the only one "
                "(§5.0: geometry is declared in layout_spec.toml, and album.toml selects)",
            )
        self.layout_variant = str(layout.get("variant") or "").strip()

        # 2b PR-(4): the album's art data. The allow-list is the §9 example's keys; a
        # key the bake does not implement is refused rather than ignored, and the two
        # modes that are *legal in §7 but not this phase's* (`cover+breathe`, 8bpp)
        # are refused by name so a config cannot silently get a different picture.
        self.art = None
        art = doc.get("art") or {}
        if art:
            allowed = {"cover", "mode", "box", "palette_max", "bit_depth", "bleed", "dither"}
            extra = sorted(set(art) - allowed)
            if extra:
                raise Refusal("config", f"{path}: [art] {', '.join(extra)} is not a field of that section")
            mode = str(art.get("mode", "cover"))
            if mode == "cover+breathe":
                raise Refusal(
                    "config",
                    f"{path}: [art] mode cover+breathe is a per-frame palette cycle (§7) and lands with "
                    "the visualiser's cadence (Phase 4/AP-6), not 2b's event-paced surface",
                )
            if mode not in ("cover", "none"):
                raise Refusal("config", f"{path}: [art] mode {mode!r} is not one of none, cover")
            cover = str(art.get("cover", ""))
            if mode == "cover" and not cover:
                raise Refusal("config", f"{path}: [art] cover is required when mode = cover")
            palette_max = config_int(path, "[art] palette_max", art.get("palette_max", 15))
            if not 1 <= palette_max <= 15:
                raise Refusal("config", f"{path}: [art] palette_max {palette_max} is outside 1..=15")
            bit_depth = config_int(path, "[art] bit_depth", art.get("bit_depth", 4))
            if bit_depth != 4:
                raise Refusal(
                    "config",
                    f"{path}: [art] bit_depth {bit_depth} is the 8bpp alternative §7 rules a side-by-side "
                    "judged on a screen; this phase writes 4bpp only",
                )
            bleed = art.get("bleed", False)
            if not isinstance(bleed, bool):
                raise Refusal("config", f"{path}: [art] bleed must be a boolean")
            dither = str(art.get("dither", "bayer2"))
            if dither not in ("bayer2", "bayer4"):
                raise Refusal("config", f"{path}: [art] dither {dither!r} is not bayer2 or bayer4")
            box = str(art.get("box", "128x128"))
            try:
                box_w, box_h = (int(part) for part in box.lower().split("x", 1))
            except ValueError:
                raise Refusal("config", f"{path}: [art] box {box!r} is not WxH")
            self.art = {
                "mode": mode, "cover": cover, "palette_max": palette_max,
                "dither": dither, "bleed": bleed, "box_w": box_w, "box_h": box_h,
            }

        self.title = str(album.get("title", ""))
        self.artist = str(album.get("artist", ""))
        self.year = config_int(path, "[album] year", album.get("year", 0))
        if not self.title:
            raise Refusal("config", f"{path}: [album] title is required")
        if not 0 <= self.year <= 0xFFFF:
            raise Refusal("config", f"{path}: [album] year {self.year} does not fit ALBM1's u16")

        self.rom = str(out.get("rom", ""))
        self.backend = str(out.get("backend", "flac"))
        self.rate_hz = config_int(path, "[out] rate_hz", out.get("rate_hz", PRODUCT_CLOCK_HZ))
        self.cart_bytes = config_int(path, "[out] cart_bytes", out.get("cart_bytes", 33554432))

        if self.backend not in BACKENDS:
            raise Refusal(
                "backend",
                f"{path}: backend {self.backend!r}, known selectors are "
                f"{sorted(BACKENDS)} (§4.2b: one backend per ROM, chosen at build time)",
            )
        if self.backend != "flac":
            raise Refusal(
                "backend",
                f"{path}: backend {self.backend!r} has no bytes to pack in Phase 1; "
                "§9.0's bake is the packed-GAFP path, and the stub arm lands with the app",
            )
        if self.rate_hz != PRODUCT_CLOCK_HZ:
            # Refusal (a): Q14. The packer would refuse this too, from the WAV
            # header; the config is where the *declaration* is wrong, so the
            # message can name the config line instead of a file it produced.
            raise Refusal(
                "rate",
                f"{path}: declared rate {self.rate_hz} Hz is not the product clock "
                f"{PRODUCT_CLOCK_HZ} Hz (Q14: a stream is encoded at the rate it is played "
                "at — 32,768 Hz against the 16,384 Hz clock is 1.024x, 41.06 cents sharp)",
            )

        tracks = doc.get("track") or []
        if not 1 <= len(tracks) <= MAX_TRACKS:
            # §9.0's `over-track-count` row, re-homed to this tool by 2b PR-(1). It was
            # `track-count` in Phase 1 — the refusal existed, the register row had no
            # owner — and the re-homing is where it gets its register name. The ceiling is
            # ALBM1's own u8 track count, not a taste call: §4.3's header carries it.
            raise Refusal(
                "over-track-count",
                f"{path}: {len(tracks)} tracks, ALBM1 carries 1..{MAX_TRACKS} (§9.0's over-track-count, "
                "re-homed from Phase 3 by 2b PR-(1): a manifest that cannot name its tracks is not a "
                "budget verdict, it is a writer that will truncate)",
            )
        self.tracks = [TrackConfig(spec, index) for index, spec in enumerate(tracks)]


# --- the child: gafp_pack.py ------------------------------------------------


def pack_track(wav: Path, gfp: Path, pin: Path, index: int) -> str:
    """Ask the shipped packer for one blob and its pin; forward any refusal.

    Never `--allow-lpc`, `--allow-unmeasured` or `--allow-over-budget`: a
    relaxation here would be the bake quietly widening the product profile, and
    the point of driving the packer is that this tool owns no encode decision.

    The child prints profile refusals to stdout and usage failures to
    stderr; `stderr or stdout` is `gafp_pack.py`'s own `run()` idiom, so the
    forwarded detail is always the child's message and never a summary of it.
    """
    cmd = [
        sys.executable,
        str(GAFP_PACK),
        "--master",
        "--emit-pin",
        str(pin),
        str(wav),
        str(gfp),
    ]
    proc = subprocess.run(cmd, capture_output=True, text=True)
    if proc.returncode != 0:
        detail = (proc.stderr or proc.stdout).strip()
        raise Refusal(
            "encode-profile",
            f"gafp_pack.py refused track {index} (exit {proc.returncode}): {detail}",
        )
    return proc.stdout


def read_container(path: Path) -> dict:
    """The packed `GAFP` container's own header — re-read, never remembered.

    `ALBM1` records `rate_hz`, `frames`, `max_fixed_order` and the track's
    length from THIS header (§4.3 rule 1: the cross-check is between two files,
    not between the writer and itself).
    """
    data = path.read_bytes()
    if len(data) < 26:
        raise Refusal("container", f"{path}: {len(data)} bytes is shorter than a GAFP header")
    if data[0:4] != GAFP_MAGIC:
        raise Refusal("container", f"{path}: magic is {data[0:4]!r}, not GAFP")
    # Only the four fields ALBM1 records are read back: the rest of GAFP's
    # header (channels, depth, block size) is the packer's own contract, and
    # this tool re-stating it would be a second opinion, not a cross-check.
    return {
        "rate_hz": struct.unpack_from("<I", data, 10)[0],
        "total_samples": struct.unpack_from("<Q", data, 14)[0],
        "frames": struct.unpack_from("<H", data, 22)[0],
        "max_fixed_order": data[7],
    }


def check_sample_agreement(index: int, prepared_samples: int, container: dict) -> None:
    """Refusal (c): §4.3 rule 1, between the prepared source and the container."""
    if container["total_samples"] != prepared_samples:
        raise Refusal(
            "manifest-samples",
            f"track {index}: the prepared source has {prepared_samples} samples, the packed "
            f"container reports {container['total_samples']} — §4.3 rule 1 refuses on "
            "disagreement rather than letting the manifest and the blob drift",
        )


# --- §6: the reciprocal, and the four bake-time assertions ------------------


def bar_recip(n_samples: int, bar_cells: int) -> int:
    """`floor(2^32 × bar_cells / n_samples)` — the host's one divide.

    The grid is the variant's declared cell count (§6), not a constant here, and
    it has **no default**: §6's rule is that a grid is never assumed, so every
    caller names the grid it means (`BAR_CELLS_REGISTERED` is passed explicitly
    at §6's worked example and at the Phase-1 fixture legs, which are the two
    places that are about the retired 32-cell grid on purpose).

    Refuses a track at or below that many samples: at exactly the declared count
    the quotient is 2^32, one past ALBM1's `u32` recip field, and below it larger
    still, so such a track is not one the manifest can describe (and
    `struct.pack_into` would raise a bare `struct.error` instead of a refusal).
    """
    if n_samples <= bar_cells:
        raise Refusal(
            "bar-reciprocal",
            f"{n_samples} samples is at most this variant's {bar_cells} bar cells "
            f"(§6's floor is {BAR_CELLS_FLOOR} cells), so floor(2^32 × {bar_cells} / {n_samples}) "
            "does not fit ALBM1's u32 recip field",
        )
    return (bar_cells << 32) // n_samples


def assert_bar_reciprocal(index: int, n_samples: int, recip: int,
                         bar_cells: int) -> None:
    """The four §6 properties plus the exact-floor inequality, over sampled positions.

    §6 names them as "the bake-time assertions, not hopes": monotonic in
    position, never reaching the grid's cell count, landing on one less than it at the last
    sample, and within one cell of the exact quotient — checked over 1,000..2.6 M
    samples, which is the range the spec states them for, plus the track's own
    endpoints.
    """
    target = bar_cells << 32
    if not (recip * n_samples <= target < (recip + 1) * n_samples):
        raise Refusal(
            "bar-reciprocal",
            f"track {index}: recip {recip:#x} is not floor(2^32 × {bar_cells} / {n_samples})",
        )
    positions = _bar_positions(n_samples)
    previous = 0
    for pos in positions:
        cell = (pos * recip) >> 32
        exact = (pos * bar_cells) // n_samples  # host divide: this is the check
        if cell >= bar_cells:
            raise Refusal("bar-reciprocal", f"track {index}: cell {cell} at pos {pos} reaches the bar")
        if cell < previous:
            raise Refusal(
                "bar-reciprocal",
                f"track {index}: cell went backwards at pos {pos} ({previous} -> {cell})",
            )
        if abs(cell - exact) > 1:
            raise Refusal(
                "bar-reciprocal",
                f"track {index}: cell {cell} at pos {pos} is more than 1 from the exact {exact}",
            )
        previous = cell
    if ((n_samples - 1) * recip) >> 32 != bar_cells - 1:
        raise Refusal(
            "bar-reciprocal",
            f"track {index}: the last sample lands on cell {((n_samples - 1) * recip) >> 32}, "
            f"not {bar_cells - 1}",
        )


def _bar_positions(n_samples: int) -> list[int]:
    """The positions the four properties are checked at: boundaries + ~4,096 samples."""
    last = n_samples - 1
    positions = {0, 1, 2, n_samples // 2, last - 2, last - 1, last}
    low, high = 1_000, min(2_600_000, last)
    if high > low:
        step = max(1, (high - low) // 4_096)
        positions.update(range(low, high, step))
    return sorted(pos for pos in positions if 0 <= pos <= last)


# --- the manifest -----------------------------------------------------------


def build_pool(strings: list[str]) -> tuple[bytes, list[int]]:
    """Concatenate NUL-terminated ASCII strings, 4-byte padded, sharing duplicates.

    Returns the pool bytes and one offset per input string (the same list order),
    so an entry's `title`/`artist` field is written from what was validated.
    """
    pool = bytearray()
    seen: dict[str, int] = {}
    offsets: list[int] = []
    for text in strings:
        if not text.isascii():
            raise Refusal("title-ascii", f"pool string {text!r} is not ASCII after the fold")
        if text in seen:
            offsets.append(seen[text])
            continue
        offset = len(pool)
        if offset >= POOL_LIMIT:
            raise Refusal("pool", f"the string pool reached {offset} bytes, the limit is {POOL_LIMIT}")
        seen[text] = offset
        pool += text.encode("ascii") + b"\x00"
        offsets.append(offset)
    while len(pool) % 4:
        pool.append(0)
    if len(pool) >= POOL_LIMIT:
        raise Refusal("pool", f"the padded string pool is {len(pool)} bytes, the limit is {POOL_LIMIT}")
    return bytes(pool), offsets


def build_albm1(
    album_title: str,
    album_artist: str,
    year: int,
    backend: int,
    tracks: list[dict],
    asset_bytes: bytes,
    bar_cells: int,
) -> bytes:
    """Assemble the manifest: header ‖ track table ‖ string pool ‖ asset region."""
    strings = [album_title, album_artist]
    for track in tracks:
        strings.extend((track["title"], track["artist"]))
    pool, offsets = build_pool(strings)
    album_title_off, album_artist_off = offsets[0], offsets[1]
    offsets = offsets[2:]

    n_tracks = len(tracks)
    track_off = HEADER_BYTES
    track_len = n_tracks * TRACK_BYTES
    pool_off = track_off + track_len
    pool_len = len(pool)
    asset_off = pool_off + pool_len
    asset_len = len(asset_bytes)
    album_samples = sum(track["n_samples"] for track in tracks)

    table = bytearray(track_len)
    cursor = 0
    for index, track in enumerate(tracks):
        at = index * TRACK_BYTES
        title_off, artist_off = offsets[index * 2], offsets[index * 2 + 1]
        if cursor != track["start_sample"]:
            # Never silent: a config-derived timeline that disagrees with the
            # running sum is a writer bug, and the reader would refuse it.
            raise Refusal(
                "track-timeline",
                f"track {index} starts at {track['start_sample']}, the running sum is {cursor}",
            )
        cursor += track["n_samples"]
        struct.pack_into("<I", table, at, track["start_sample"])
        struct.pack_into("<I", table, at + 4, track["n_samples"])
        struct.pack_into("<I", table, at + 8, track["recip"])
        struct.pack_into("<I", table, at + 12, track["asset_off"])
        struct.pack_into("<I", table, at + 16, track["asset_len"])
        struct.pack_into("<I", table, at + 20, track["frames"])
        struct.pack_into("<H", table, at + 24, title_off)
        struct.pack_into("<H", table, at + 26, artist_off)
        table[at + 28] = track["backend"]
        table[at + 29] = track["max_fixed_order"]
        struct.pack_into("<H", table, at + 30, track["cost_ceiling_permille"])
        struct.pack_into("<I", table, at + 32, track["fnv"])
        # 36..40 reserved, already zero.

    if cursor != album_samples:
        raise Refusal("track-timeline", f"the tracks sum to {cursor}, the album to {album_samples}")

    blob = bytearray(asset_off + asset_len)
    blob[0:4] = MAGIC
    blob[4] = VERSION
    blob[5] = n_tracks
    blob[6] = backend
    blob[7] = 0
    struct.pack_into("<I", blob, 8, PRODUCT_CLOCK_HZ)
    struct.pack_into("<I", blob, 12, bar_cells)
    struct.pack_into("<I", blob, 16, album_samples)
    struct.pack_into("<I", blob, 20, track_off)
    struct.pack_into("<I", blob, 24, track_len)
    struct.pack_into("<I", blob, 28, pool_off)
    struct.pack_into("<I", blob, 32, pool_len)
    struct.pack_into("<I", blob, 36, asset_off)
    struct.pack_into("<I", blob, 40, asset_len)
    struct.pack_into("<I", blob, STRUCTURE_FNV_AT, fnv1a32_parts(table, pool))
    struct.pack_into("<H", blob, 48, album_title_off)
    struct.pack_into("<H", blob, 50, album_artist_off)
    struct.pack_into("<H", blob, 52, year)
    struct.pack_into("<I", blob, ASSETS_FNV_AT, fnv1a32_parts(asset_bytes))
    blob[track_off:pool_off] = table
    blob[pool_off:asset_off] = pool
    blob[asset_off:] = asset_bytes
    return bytes(blob)


def fnv1a32_parts(*parts: bytes) -> int:
    """FNV-1a/32 over concatenated spans — the fold `meta.rs` re-verifies."""
    value = FNV_OFFSET
    for part in parts:
        for byte in part:
            value ^= byte
            value = (value * FNV_PRIME) & MASK32
    return value


def content_pins(blob: bytes) -> tuple[int, int]:
    """The bounded content tripwire's two pins: the first 32 KiB and last 4 KiB.

    `meta::prefix_fnv` / `meta::suffix_fnv` fold the same spans, and the ROM
    compares them with these pins at boot (`backend::verify_pins`). A byte in the
    middle of the blob is deliberately outside both spans: that is the whole point
    of the bound, and the full fold that still covers it is `ASSETS_FNV_AT`'s,
    verified after the audio.
    """
    prefix = fnv1a32_parts(blob[:CONTENT_PREFIX_BYTES])
    suffix = fnv1a32_parts(blob[max(0, len(blob) - CONTENT_SUFFIX_BYTES):])
    return prefix, suffix


def _overwrite(offset: int, value: bytes):
    def apply(bad: bytearray) -> None:
        bad[offset : offset + len(value)] = value

    return apply


def _xor(offset: int, mask: int):
    def apply(bad: bytearray) -> None:
        bad[offset] ^= mask

    return apply


def _pad_pool(bad: bytearray) -> None:
    # The pool's trailing pad byte, after the last NUL.
    bad[_word(bad, 36) - 1] = ord("Z")


#: The drift gate's **one** source (§9.0's self-audit re-read): each named mutation
#: `--self-test` feeds to `audit_blob`, and the names travel into `src/assets.rs`,
#: so the host suite
#: (`refuses_every_mutation_the_bake_self_test_feeds_to_audit_blob`) applies this
#: same list to the shipping `meta::parse` instead of transcribing it a second
#: time. A rule one side drops and the other keeps is then a test failure, not a
#: silent divergence.
AUDIT_MUTATIONS = {
    "magic": _overwrite(0, b"X"),
    "version": _overwrite(4, b"\x03"),
    "n_tracks": _overwrite(5, b"\x00"),
    "reserved byte 7": _overwrite(7, b"\x01"),
    "rate": _overwrite(8, b"\x00\x80\x00\x00"),
    "bar_cells": _overwrite(12, b"\x40\x00\x00\x00"),
    "structure checksum": _xor(STRUCTURE_FNV_AT, 0xFF),
    "assets checksum": _xor(ASSETS_FNV_AT, 0xFF),
    "pool padding": _pad_pool,
}


def audit_blob(blob: bytes, bar_cells: int) -> list[str]:
    """Re-read the finished manifest byte for byte, exactly as meta.rs will.

    Nothing here trusts `build_albm1`: every field is read from the bytes, every
    length is derived from the header's own counts, and the checksum is checked
    last. A problem list is a refusal — "a blob is not accepted because it was
    written" (§4.3). The shipping reader is the other half of this rule: the
    host gate parses this same blob through `meta::parse`.
    """
    bad: list[str] = []
    if len(blob) < HEADER_BYTES:
        return [f"length {len(blob)} is shorter than the {HEADER_BYTES}-byte header"]
    if blob[0:4] != MAGIC:
        bad.append(f"magic {blob[0:4]!r}")
    version, n_tracks, backend = blob[4], blob[5], blob[6]
    if blob[7] != 0:
        bad.append(f"reserved byte 7 is {blob[7]}")
    for offset in range(54, ASSETS_FNV_AT):
        if blob[offset] != 0:
            bad.append(f"reserved byte {offset} is {blob[offset]}")
            break
    if version != VERSION:
        bad.append(f"version {version}")
    if not 1 <= n_tracks <= MAX_TRACKS:
        bad.append(f"n_tracks {n_tracks}")
    if backend not in BACKENDS.values():
        bad.append(f"backend {backend}")
    rate_hz = _word(blob, 8)
    if rate_hz != PRODUCT_CLOCK_HZ:
        bad.append(f"rate {rate_hz}")
    if _word(blob, 12) != bar_cells:
        bad.append(f"bar_cells {_word(blob, 12)}")

    track_off = _word(blob, 20)
    track_len = _word(blob, 24)
    pool_off = _word(blob, 28)
    pool_len = _word(blob, 32)
    asset_off = _word(blob, 36)
    asset_len = _word(blob, 40)
    stored_fnv = _word(blob, STRUCTURE_FNV_AT)
    stored_assets_fnv = _word(blob, ASSETS_FNV_AT)
    if track_off != HEADER_BYTES or pool_off != track_off + track_len or asset_off != pool_off + pool_len:
        bad.append("sections are not header ‖ table ‖ pool ‖ assets")
    if any(value % 4 for value in (track_off, track_len, pool_off, pool_len, asset_off)):
        bad.append("a section offset or length is not 4-byte aligned")
    if track_len != n_tracks * TRACK_BYTES:
        bad.append(f"track table is {track_len} bytes for {n_tracks} tracks")
    if pool_len >= POOL_LIMIT:
        bad.append(f"string pool is {pool_len} bytes")
    if asset_off + asset_len != len(blob):
        bad.append(f"{len(blob)} bytes, the header derives {asset_off + asset_len}")
        return bad

    tracks = blob[track_off : track_off + track_len]
    pool = blob[pool_off : pool_off + pool_len]
    assets = blob[asset_off : asset_off + asset_len]
    album_samples = _word(blob, 16)
    album_title_off, album_artist_off = _half(blob, 48), _half(blob, 50)

    cursor = 0
    for index in range(n_tracks):
        at = index * TRACK_BYTES
        entry = tracks[at : at + TRACK_BYTES]
        for offset in range(36, TRACK_BYTES):
            if entry[offset] != 0:
                bad.append(f"track {index} reserved byte {offset} is {entry[offset]}")
                break
        start, length = _word(entry, 0), _word(entry, 4)
        if length == 0 or start != cursor or start + length > album_samples:
            bad.append(f"track {index} covers {start}+{length} of {album_samples}")
        cursor = start + length
        offset, size = _word(entry, 12), _word(entry, 16)
        if size == 0 or offset + size > asset_len:
            bad.append(f"track {index} addresses {offset}+{size} of {asset_len}")
        recip = _word(entry, 8)
        if not (recip * length <= bar_cells << 32 < (recip + 1) * length):
            bad.append(f"track {index} recip {recip:#x}, not floor(2^32 × {bar_cells} / {length})")
        for field, offset in (("title", _half(entry, 24)), ("artist", _half(entry, 26))):
            bad.extend(f"track {index} {field}: {problem}" for problem in _string_problem(pool, offset))
    if cursor != album_samples:
        bad.append(f"the tracks sum to {cursor}, the album declares {album_samples}")

    for name, offset in (("album title", album_title_off), ("album artist", album_artist_off)):
        bad.extend(f"{name}: {problem}" for problem in _string_problem(pool, offset))

    # Padding: every byte belongs to a NUL-terminated string, and a non-empty
    # string has to be one an entry points at — that is how a non-zero padding
    # byte is caught even when it happens to be ASCII.
    referenced = {album_title_off, album_artist_off}
    for index in range(n_tracks):
        entry = tracks[index * TRACK_BYTES : (index + 1) * TRACK_BYTES]
        referenced.update((_half(entry, 24), _half(entry, 26)))
    at = 0
    while at < pool_len:
        start = at
        while at < pool_len and pool[at] != 0:
            at += 1
        if at == pool_len:
            bad.append(f"pool string at {start} is not NUL-terminated")
            break
        if at > start and start not in referenced:
            bad.append(f"pool byte {start} is non-zero padding ({pool[start]:#04x})")
        at += 1

    computed = fnv1a32_parts(tracks, pool)
    if stored_fnv != computed:
        bad.append(f"structure checksum {stored_fnv:#010x}, table+pool hash to {computed:#010x}")
    computed_assets = fnv1a32_parts(assets)
    if stored_assets_fnv != computed_assets:
        bad.append(
            f"asset checksum {stored_assets_fnv:#010x}, the asset region hashes to {computed_assets:#010x}"
        )
    return bad


def _word(bytes_: bytes, offset: int) -> int:
    return struct.unpack_from("<I", bytes_, offset)[0]


def _half(bytes_: bytes, offset: int) -> int:
    return struct.unpack_from("<H", bytes_, offset)[0]


def _string_problem(pool: bytes, offset: int) -> list[str]:
    if offset >= len(pool):
        return [f"offset {offset} is outside the {len(pool)}-byte pool"]
    rest = pool[offset:]
    end = rest.find(b"\x00")
    if end < 0:
        return [f"offset {offset} is not NUL-terminated"]
    for index, byte in enumerate(rest[:end]):
        if byte >= 0x80:
            return [f"byte {offset + index} is {byte:#04x}, strings are ASCII"]
    return []


# --- generated Rust ---------------------------------------------------------


def read_pin_module(path: Path) -> dict:
    """The child's `--emit-pin` module, read as facts (never re-derived).

    The values are what `gafp_pack.py` measured on the blob it wrote, so the pin
    module is the third file the bake cross-checks rather than a second writer
    with opinions: FRAMES/SAMPLES are held to the container header it just read.
    """
    facts: dict[str, int] = {}
    for line in path.read_text().splitlines():
        line = line.strip()
        if not line.startswith("pub const ") or "=" not in line:
            continue
        name = line[len("pub const ") : line.index(":")].strip()
        value = line.split("=", 1)[1].strip().rstrip(";").strip()
        facts[name] = int(value, 0)
    missing = {
        "FRAMES", "SAMPLES", "FNV_PCM", "FNV_FIFO_L", "FNV_FIFO_R", "MAX_FIXED_ORDER",
    } - set(facts)
    if missing:
        raise Refusal("pin", f"{path}: generated pin module is missing {sorted(missing)}")
    return facts


def assets_rs(config: Config, tracks: list[dict], album_samples: int,
              facts: "VariantFacts", longest_title: tuple[float, str],
              art: dict | None = None) -> str:
    """The generated pin module. Deterministic text: no timestamp, no abs path.

    It carries the album-side half of §5.5's fit verdict — the box lives on the
    `layout::` side — so that a host test can compare a pixel width against a
    pixel box rather than a character count against a character constant.
    """
    if facts.title_element is not None:
        fit_note = (
            f"/// Measured at {facts.title_element.face}, the face variant "
            f"{facts.variant.id} declares for element {facts.title_element.id}; the box that\n"
            f"/// element declares is {facts.title_element.w} px, generated into `src/layout.rs`. Two\n"
            "/// halves of one verdict, one per side of the seam, and the host suite is what\n"
            "/// compares them. Pixels, never characters: the character-count boxes this replaces\n"
            "/// (`album_prep.title_fits`, `album_prep.TITLE_BOX_MAX`, `meta::MAX_TITLE`) are gone."
        )
    else:
        fit_note = (
            f"/// Variant {facts.variant.id} declares no text line in a title band, so it declares no\n"
            "/// box, and this measurement is 0 by definition rather than by omission: §5.0's A\n"
            "/// carries its title band as art. Said in the module instead of assumed by a test."
        )
    pins = []
    for track in tracks:
        pins.append(
            "\n".join(
                [
                    "    TrackPin {",
                    f"        frames: {track['frames']},",
                    f"        samples: {dec(track['n_samples'])},",
                    f"        start_sample: {dec(track['start_sample'])},",
                    f"        n_samples: {dec(track['n_samples'])},",
                    f"        recip: {hex32(track['recip'])},",
                    f"        asset_off: {dec(track['asset_off'])},",
                    f"        asset_len: {dec(track['asset_len'])},",
                    f'        blob_sha256: "{track["blob_sha256"]}",',
                    f"        fnv_pcm: {hex64(track['pin']['FNV_PCM'])},",
                    f"        fnv_fifo_l: {hex64(track['pin']['FNV_FIFO_L'])},",
                    f"        fnv_fifo_r: {hex64(track['pin']['FNV_FIFO_R'])},",
                    f"        prefix_fnv: {hex32(track['prefix_fnv'])},",
                    f"        suffix_fnv: {hex32(track['suffix_fnv'])},",
                    f"        max_fixed_order: {track['pin']['MAX_FIXED_ORDER']},",
                    "    },",
                ]
            )
        )
    if art is None:
        art_consts = f'''/// 2b PR-(4): no art in this album (`[art]` absent or `mode = "none"`). The art
/// scene draws nothing and the card has no picture; the variant's ceiling still bounds
/// `POOL_IMAGE` because there is no selection to lower it.
pub const HAS_ART: bool = false;
pub const ART1: &[u8] = &[];
pub const ART_BOX_PX_W: u32 = 0;
pub const ART_BOX_PX_H: u32 = 0;
pub const ART_TILES: u32 = 0;
pub const ART_PALETTE_BANKS: u32 = 0;
pub const ART1_BYTES: usize = 0;
pub const ART1_SHA256: &str = "";
pub const POOL_IMAGE: u32 = {facts.pool_image};
pub const POOL_DISTINCT: u32 = {facts.pool_distinct};
const _: () = assert!(
    POOL_DISTINCT
        == POOL_IMAGE
            + crate::layout::POOL_TEXT_DISTINCT
            + crate::layout::POOL_CHROME_DECLARED
            + crate::layout::BAR_PATTERN_TILES,
    "the album pool audit does not add up"
);
const _: () = assert!(
    POOL_DISTINCT <= crate::layout::TILE_BUDGET,
    "the album pool exceeds tile_budget"
);
const _: () = assert!(
    POOL_IMAGE <= crate::layout::IMAGE_TILES_MAX,
    "the selected image exceeds image_tiles"
);
'''
    else:
        art_consts = f'''/// 2b PR-(4): the album's cover as the selected `[art] box`. `assets/art1.bin` is
/// generated by `scripts/album_art.py`; the host gate re-reads it through `gba_art::art1` and
/// checks these fields against the blob, and the compile-time asserts below are the album's own
/// budget (Decision B: the image term is the box this album ships, not the variant's ceiling).
pub const HAS_ART: bool = true;
pub const ART1: &[u8] = include_bytes!("../assets/art1.bin");
pub const ART_BOX_PX_W: u32 = {art["w"]};
pub const ART_BOX_PX_H: u32 = {art["h"]};
pub const ART_TILES: u32 = {art["tiles"]};
pub const ART_PALETTE_BANKS: u32 = {art["banks"]};
pub const ART1_BYTES: usize = {art["bytes"]};
pub const ART1_SHA256: &str = "{art["sha256"]}";
pub const POOL_IMAGE: u32 = {facts.pool_image};
pub const POOL_DISTINCT: u32 = {facts.pool_distinct};
const _: () = assert!(
    POOL_DISTINCT
        == POOL_IMAGE
            + crate::layout::POOL_TEXT_DISTINCT
            + crate::layout::POOL_CHROME_DECLARED
            + crate::layout::BAR_PATTERN_TILES,
    "the album pool audit does not add up"
);
const _: () = assert!(
    POOL_DISTINCT <= crate::layout::TILE_BUDGET,
    "the album pool exceeds tile_budget"
);
const _: () = assert!(
    POOL_IMAGE <= crate::layout::IMAGE_TILES_MAX,
    "the selected image exceeds image_tiles"
);
'''

    return f'''//! Generated by `scripts/album_bake.py` — do not edit.
//!
//! The packed album this crate reads: `assets/albm1.bin` (ALBM1, the manifest
//! §4.3 makes authoritative) plus the per-track pins the bake read back from
//! each packed `GAFP` container. `make album-bake` rewrites this file;
//! `make album-pack-check` bakes twice and compares bytes, which is the evidence
//! these are *generated* rather than "whatever is lying around" (§2.3, §9.0).
//!
//! Nothing here is tracked and nothing here is hand-edited. A fresh clone has no
//! `assets/albm1.bin`, so this module only exists after a bake — `make check`
//! formats the crate through `scripts/fmt_with_generated_stub.sh`'s GENERATED
//! override, and `include_bytes!` makes a missing blob a compile error naming
//! the path rather than a runtime default.

/// The manifest, `assets/albm1.bin` (ALBM1 v{VERSION}).
pub const ALBUM1: &[u8] = include_bytes!("../assets/albm1.bin");

/// Tracks in the album (`ALBM1`'s header `n_tracks`).
pub const N_TRACKS: u32 = {len(tracks)};

/// The product clock every track is encoded and read at (Q14).
pub const RATE_HZ: u32 = {PRODUCT_CLOCK_HZ};

/// `[out] backend`.
pub const BACKEND: &str = "{config.backend}";

/// Album length in samples at [`RATE_HZ`].
pub const TOTAL_SAMPLES: u32 = {dec(album_samples)};

/// `[album]` title, ASCII-folded at bake time (§5.5).
pub const ALBUM_TITLE: &str = {rust_str(config.title)};

/// `[album]` artist, ASCII-folded at bake time (§5.5).
pub const ALBUM_ARTIST: &str = {rust_str(config.artist)};

/// `[album]` year (`0` when the config carries none).
pub const YEAR: u16 = {config.year};

/// The layout variant this ROM's geometry comes from. `layout::LAYOUT_ID` is the
/// same string from the other generator, and
/// `tests/album_pack_host.rs`'s `the_pinned_album_and_the_generated_module_name_one_layout_variant`
/// asserts the two agree (`make album-layout-test` runs that leg by name): a manifest that names
/// one variant and a module that names another is exactly the staleness R11 describes, and a build
/// that compiles cannot see it. What that leg cannot see either, because `ALBM1`'s header carries
/// no variant field, is a module regenerated for a different variant declaring the same grid —
/// §11's D9 says so, and §6 is where a spec digest would be ruled.
pub const LAYOUT_VARIANT: &str = "{facts.variant.id}";

{art_consts}
{fit_note}
pub const TITLE_LONGEST: &str = {rust_str(longest_title[1])};
pub const TITLE_LONGEST_PX: u32 = {round(longest_title[0])};

/// What a track's packed blob must say, as the bake read it back.
#[derive(Debug, Clone, Copy)]
pub struct TrackPin {{
    /// Frames in the packed blob (the container's own count).
    pub frames: u32,
    /// Interchannel samples per channel in the blob (`total_samples`).
    pub samples: u64,
    /// Album position of the track's first sample.
    pub start_sample: u32,
    /// Samples in the track.
    pub n_samples: u32,
    /// §6's progress reciprocal, `floor(2^32 × {facts.variant.bar_cells} / n_samples)` — the grid
    /// variant {facts.variant.id} declares, generated into `src/layout.rs` as `BAR_CELLS`.
    pub recip: u32,
    /// Offset of the blob inside `ALBM1`'s asset region.
    pub asset_off: u32,
    /// Length of the blob.
    pub asset_len: u32,
    /// sha256 of the packed blob — the Q15 witness (track index → blob sha256).
    pub blob_sha256: &'static str,
    /// FNV-1a/64 over the reference PCM (`flac -d`).
    pub fnv_pcm: u64,
    /// Left channel's 8-bit FIFO bytes, in stream order (played-form witness).
    pub fnv_fifo_l: u64,
    /// Right channel's 8-bit FIFO bytes, in stream order (played-form witness).
    pub fnv_fifo_r: u64,
    /// FNV-1a/32 over the blob's first 32 KiB — the boot content tripwire's prefix
    /// half (`meta::prefix_fnv`). A boot may read only this much per track.
    pub prefix_fnv: u32,
    /// FNV-1a/32 over the blob's last 4 KiB — the tripwire's suffix half
    /// (`meta::suffix_fnv`).
    pub suffix_fnv: u32,
    /// Highest FIXED predictor order the packer recorded.
    pub max_fixed_order: u8,
}}

/// One entry per track, in play order.
pub const TRACKS: [TrackPin; {len(tracks)}] = [
{chr(10).join(pins)}
];

/// The named mutations `scripts/album_bake.py --self-test` feeds to its own audit,
/// in the order it applies them. The host suite applies this same list to
/// `meta::parse`, so a rule the Python audit still applies and the reader no
/// longer applies — or the reverse — is a test failure rather than a silent
/// divergence.
pub const AUDITED_MUTATIONS: [&str; {len(AUDIT_MUTATIONS)}] = [
{chr(10).join(f'    {rust_str(name)},' for name in AUDIT_MUTATIONS)}
];
'''


def dec(value: int) -> str:
    return f"{value:_}"


def hex32(value: int) -> str:
    return f"0x{value:08x}"


def hex64(value: int) -> str:
    return f"0x{value:016x}"


def rust_str(text: str) -> str:
    escaped = text.replace("\\", "\\\\").replace('"', '\\"')
    return f'"{escaped}"'


# --- the bake ---------------------------------------------------------------


def _relative(path: Path) -> str:
    """A repo-relative path for a log line, without ever failing on one that is not."""
    return str(path.relative_to(REPO)) if path.is_relative_to(REPO) else str(path)


def write_atomic(path: Path, data: bytes) -> None:
    """Write `path` through `path.tmp`, so a reader never sees a half file."""
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_name(path.name + ".tmp")
    tmp.write_bytes(data)
    os.replace(tmp, path)


def prepare_art(config: Config, facts: "VariantFacts", assets_dir: Path) -> dict | None:
    """Generate the album's ART1 from `[art]`, and move the image term to the selection.

    2b PR-(4): the box is the first album-side *geometry-adjacent* key, so it is validated
    against the variant's derived list here (the `art-box-not-derived` row, whose validator
    was 2a's and whose key is 2b's). The tool is the one that owns the pixels; this function
    owns only "which box, which mode, and does the selected term fit".
    """
    art = config.art
    if art is None or art["mode"] == "none":
        return None
    cover = Path(art["cover"])
    if not cover.is_absolute():
        cover = REPO / cover
    if not cover.exists():
        raise Refusal("art-source", f"[art] cover {cover} does not exist")
    if not git_tracked(cover):
        raise Refusal(
            "art-source",
            f"[art] cover {cover} is not tracked in git — AP-4's reproduction claim is about a "
            "source anyone who clones the tree can re-run, exactly as §5.5's face rule is",
        )
    if art["bleed"]:
        width, height = 240, 160
    else:
        width, height = art["box_w"], art["box_h"]
        if width % 8 or height % 8 or width != height:
            raise Refusal(
                "art-box-not-derived",
                f"[art] box {width}x{height} is not a square multiple of 8; the derived boxes are "
                + ", ".join(f"{b.side * 8}x{b.side * 8}" for b in facts.art_boxes),
            )
        side = width // 8
        if side not in [box.side for box in facts.art_boxes]:
            raise Refusal(
                "art-box-not-derived",
                f"[art] box {width}x{height} is not one the selected variant {facts.variant.id} derives; "
                f"the legal boxes are 8..{facts.art_ceiling.width_px if facts.art_ceiling else 0} px in 8 px steps",
            )
    tiles = (width // 8) * (height // 8)
    facts.select_image(tiles)
    assets_dir.mkdir(parents=True, exist_ok=True)
    out = assets_dir / "art1.bin"
    cmd = [
        sys.executable, str(ALBUM_ART),
        "--cover", str(cover),
        "--box", f"{width}x{height}",
        "--dither", art["dither"],
        "--palette-max", str(art["palette_max"]),
        "--out", str(out),
    ]
    if art["bleed"]:
        cmd.append("--bleed")
    proc = subprocess.run(cmd, capture_output=True, text=True)
    if proc.returncode != 0:
        raise Refusal(
            "art",
            (proc.stderr or proc.stdout).strip() or f"album_art.py exited {proc.returncode}",
        )
    blob = out.read_bytes()
    return {
        "w": width,
        "h": height,
        "tiles": tiles,
        "banks": 1,
        "bytes": len(blob),
        "sha256": hashlib.sha256(blob).hexdigest(),
        "report": (proc.stdout or "").strip(),
    }


def bake(config_path: Path) -> int:
    config = Config(config_path)
    crate_dir = config_path.parent
    assets_dir = crate_dir / "assets"
    assets_rs_path = crate_dir / "src" / "assets.rs"
    layout_rs_path = crate_dir / "src" / "layout.rs"
    # 2b PR-(1)'s second rendering: the same derivation as sorted `key=value`. Build
    # output like the two generated modules, written by the same bake, and never tracked
    # (§2.3's rule: a report is regenerated, not committed). It sits beside the tracked
    # spec rather than in `src/` because nothing links it.
    report_path = crate_dir / "layout_report.txt"

    # `album-layout-gen` (§9.0's owner of nine register rows) runs here, on
    # purpose before any audio is touched: §6's reciprocal is computed against the
    # selected variant's declared cell count, so resolving the layout after the
    # packing would mean writing a manifest whose bar arithmetic describes a grid
    # the ROM has no way to draw. Nothing about this is a separate build step —
    # §5.0's step 5 and §11's PR-(1) row are explicit that `make album-bake` is
    # what runs the generator.
    spec, variant, facts = layout_for(config.layout_variant)
    # 2b PR-(4): the album's cover is generated *after* the layout is resolved (the box is
    # checked against the variant it derives) and *before* anything is written, so a refused
    # box never leaves a stale ART1 or a module whose pool term disagrees with the image.
    art_meta = prepare_art(config, facts, assets_dir)
    longest_title: tuple[float, str] = (0.0, "")

    print(f"album_bake: {config_path.relative_to(REPO) if config_path.is_relative_to(REPO) else config_path}")
    print(
        f"album_bake: {len(config.tracks)} tracks, {config.backend} backend, "
        f"{PRODUCT_CLOCK_HZ} Hz stereo, cart {dec(config.cart_bytes)} B"
    )
    print(
        f"album_bake: layout: variant {variant.id} from "
        f"{DEFAULT_LAYOUT_SPEC.relative_to(REPO)}, bar grid {facts.variant.bar_cells} cells of "
        f"{facts.bar_cell_px} px = {facts.bar_px} px, region {facts.region[0]}x{facts.region[1]} tiles, "
        f"tile_budget {variant.tile_budget} — the grid every reciprocal below is computed for (§6)"
    )

    if art_meta is not None:
        print(f"album_bake: art: {art_meta['w']}x{art_meta['h']} = {art_meta['tiles']} tiles, "
              f"{art_meta['bytes']} B, sha256 {art_meta['sha256'][:16]}… (the pool term is the selection, "
              "not the variant's ceiling — Decision B, 2b PR-(4))")
        for line in art_meta["report"].splitlines():
            print("  " + line)

    work = Path(tempfile.mkdtemp(prefix="album-bake-"))
    try:
        prepared_cache: dict[tuple[str, int | None, int | None], tuple[Path, int]] = {}
        packed_cache: dict[str, dict] = {}
        tags_cache: dict[str, dict] = {}
        asset_region = bytearray()
        tracks: list[dict] = []
        cursor = 0
        for track in config.tracks:
            source = track.source()
            if not source.exists():
                raise Refusal("source", f"track {track.index}: {source} does not exist")
            key = (str(source), *track.window())
            if key not in prepared_cache:
                wav = work / f"track{track.index:02d}.wav"
                start, end = track.window()
                if start is None:
                    normalise(source, wav)
                else:
                    extract(source, start, end, wav)
                prepared_cache[key] = (wav, album_prep.frames(wav))
            wav, prepared_samples = prepared_cache[key]
            if prepared_samples == 0:
                raise Refusal("manifest-samples", f"track {track.index}: the prepared source is empty")
            digest = hashlib.sha256(wav.read_bytes()).hexdigest()

            if digest in packed_cache:
                entry = packed_cache[digest]
                print(
                    f"album_bake: track {track.index}: same source bytes as track "
                    f"{entry['index']} (sha256 {digest[:12]}) — one blob, aliased"
                )
            else:
                gfp = work / f"track{track.index:02d}.gfp"
                pin = work / f"track{track.index:02d}_pin.rs"
                pack_track(wav, gfp, pin, track.index)
                container = read_container(gfp)
                # Refusal (c): the two files' own headers must agree.
                check_sample_agreement(track.index, prepared_samples, container)
                if container["rate_hz"] != PRODUCT_CLOCK_HZ:
                    raise Refusal(
                        "rate",
                        f"track {track.index}: the packed container declares {container['rate_hz']} Hz, "
                        f"not the product clock {PRODUCT_CLOCK_HZ} Hz",
                    )
                recip = bar_recip(container["total_samples"], facts.variant.bar_cells)
                assert_bar_reciprocal(track.index, container["total_samples"], recip,
                                      facts.variant.bar_cells)
                blob = gfp.read_bytes()
                prefix_fnv, suffix_fnv = content_pins(blob)
                entry = {
                    "index": track.index,
                    "container_samples": container["total_samples"],
                    "frames": container["frames"],
                    "max_fixed_order": container["max_fixed_order"],
                    "recip": recip,
                    "pin": read_pin_module(pin),
                    "blob_sha256": hashlib.sha256(blob).hexdigest(),
                    "fnv": fnv1a32_parts(blob),
                    "prefix_fnv": prefix_fnv,
                    "suffix_fnv": suffix_fnv,
                    "asset_off": len(asset_region),
                    "asset_len": len(blob),
                    "blob": blob,
                }
                if entry["pin"]["SAMPLES"] != container["total_samples"]:
                    raise Refusal(
                        "manifest-samples",
                        f"track {track.index}: the pin module's SAMPLES "
                        f"{entry['pin']['SAMPLES']} disagrees with the container's "
                        f"{container['total_samples']}",
                    )
                if entry["pin"]["FRAMES"] != container["frames"]:
                    raise Refusal(
                        "manifest-samples",
                        f"track {track.index}: the pin module's FRAMES {entry['pin']['FRAMES']} "
                        f"disagrees with the container's {container['frames']}",
                    )
                if entry["pin"]["MAX_FIXED_ORDER"] != container["max_fixed_order"]:
                    raise Refusal(
                        "manifest-samples",
                        f"track {track.index}: the pin module's MAX_FIXED_ORDER "
                        f"{entry['pin']['MAX_FIXED_ORDER']} disagrees with the container's "
                        f"{container['max_fixed_order']}",
                    )
                packed_cache[digest] = entry
                asset_region += blob

            if str(source) not in tags_cache:
                tags_cache[str(source)] = read_tags(source)
            tags = tags_cache[str(source)] if track.title is None or track.artist is None else {}
            title = fold(track.title if track.title is not None else tags["title"])
            artist = fold(track.artist if track.artist is not None else tags["artist"])
            if not title:
                raise Refusal("title", f"track {track.index}: the folded title is empty")
            # §5.5's fit verdict, measured: the box is the width the selected
            # variant declares for the element that draws titles, and the width
            # is the folded string's advance sum at the face that element names.
            # The character-count box this replaces (`album_prep.title_fits`,
            # `TITLE_BOX_MAX`) is gone — D9's discharge says a count of
            # characters is not a box.
            #
            # What happens when a title is longer than the box is §5.5's rule, and
            # since 2a PR-(2) both halves of it exist. The over-long title is
            # elided to the last whole glyph that fits plus `...`, and the note in
            # the log says what will actually be drawn; that stays printed rather
            # than refused, exactly as §5.5 rules, because shortening somebody's
            # album title is an album decision and a person should see it in the
            # build output rather than be told to go change the file. The case
            # that *is* refused is the one elision cannot serve — a box that will
            # not hold the marker at all — and that is §9.0's `title-fit`, a spec
            # describing a box too small to render any title. `keep=0` because what
            # 2a's title band draws is the bare title: `meta.rs` stores the folded
            # name with no number prefixed, so the "never cut the track number"
            # half of the rule belongs to whichever scene composes `NN · Title`
            # (2b's list), and it passes that prefix's length here.
            fits, width, unwritten, box_px = facts.title_fit(title)
            rendered, whole = facts.title_render(title)
            if not fits:
                print(
                    f"album_bake: note: track {track.index} title {title!r} measures {width:.0f} px at "
                    f"{facts.title_element.face}'s declared raster, over the {box_px} px the "
                    f"{facts.variant.id} variant declares for element {facts.title_element.id}"
                    + (f"; no glyph for {unwritten!r}" if unwritten else "")
                    + (f" — §5.5's one-line elision draws {rendered!r} instead" if not whole else "")
                )
            longest_title = max(longest_title, (width, title))
            tracks.append(
                {
                    "start_sample": cursor,
                    "n_samples": entry["container_samples"],
                    "recip": entry["recip"],
                    "asset_off": entry["asset_off"],
                    "asset_len": entry["asset_len"],
                    "frames": entry["frames"],
                    "max_fixed_order": entry["max_fixed_order"],
                    "backend": BACKENDS[config.backend],
                    "cost_ceiling_permille": 0,
                    "title": title,
                    "artist": artist,
                    "fnv": entry["fnv"],
                    "prefix_fnv": entry["prefix_fnv"],
                    "suffix_fnv": entry["suffix_fnv"],
                    "blob_sha256": entry["blob_sha256"],
                    "pin": entry["pin"],
                }
            )
            cursor += entry["container_samples"]

        album_title = fold(config.title)
        album_artist = fold(config.artist)
        # The same rule, and the same path, for the two strings the config declares
        # rather than the tracks: they are drawn in the same element (§5.0's element
        # table has one text span for the album's name), so they get the same verdict
        # and the same elision. `longest_title` is the widest *unelided* string the
        # album asked for, which is what the report's line is about.
        for what, text in (("[album] title", album_title), ("[album] artist", album_artist)):
            if not text:
                continue
            fits, width, unwritten, box_px = facts.title_fit(text)
            rendered, whole = facts.title_render(text)
            if not fits:
                print(
                    f"album_bake: note: {what} {text!r} measures {width:.0f} px at "
                    f"{facts.title_element.face}'s declared raster, over the {box_px} px the "
                    f"{facts.variant.id} variant declares for element {facts.title_element.id}"
                    + (f"; no glyph for {unwritten!r}" if unwritten else "")
                    + (f" — §5.5's one-line elision draws {rendered!r} instead" if not whole else "")
                )
            longest_title = max(longest_title, (width, text))
        if not album_title:
            raise Refusal("title", "[album] title folds to nothing")
        blob = build_albm1(
            album_title,
            album_artist,
            config.year,
            BACKENDS[config.backend],
            tracks,
            bytes(asset_region),
            bar_cells=facts.variant.bar_cells,
        )

        # Refusal (d): the writer's output, re-read structurally, before any
        # write lands. The host gate re-reads it through the shipping meta.rs.
        problems = audit_blob(blob, facts.variant.bar_cells)
        if problems:
            raise Refusal("audit", "; ".join(problems))

        written = {f"track{entry['index']:02d}.gfp" for entry in packed_cache.values()}
        # 2b PR-(1)'s cart verdict, asked before the artifacts land: the code reserve is
        # reserved *before* the encoder sizes the audio (§10.1), so the only honest moment
        # to say "this album does not fit" is after the manifest's size is known and before
        # a byte is written. `over-budget` and `over-track-count` are this tool's now
        # (§9.0's register row, re-homed from Phase 3 by the same PR).
        budget = rom_budget(facts, albm1_bytes=len(blob), audio_bytes=len(asset_region),
                            cart_bytes=config.cart_bytes)
        for entry in packed_cache.values():
            write_atomic(assets_dir / f"track{entry['index']:02d}.gfp", entry["blob"])
        write_atomic(assets_dir / "albm1.bin", blob)
        # Generated in the same pass, from the same derivation, as the manifest:
        # there is deliberately no window in which a tree carries a manifest whose
        # reciprocals were computed for one grid next to a module that declares
        # another. §9.0's `bar-cells-disagree` row is about the pair, and §6 names
        # the pair; `meta.rs`'s boot refusal stays as the backstop for a module
        # that went stale by some route this tool does not control (R11).
        write_atomic(layout_rs_path, layout_rs_text(facts).encode("utf-8"))
        written = read_layout_module(layout_rs_path)
        assert_bar_grid_agrees(
            facts.variant.bar_cells,
            bar_albm1_value(blob),
            int(written["BAR_CELLS"]),
            f"{_relative(DEFAULT_LAYOUT_SPEC)} variant {facts.variant.id}",
        )
        if written["LAYOUT_ID"] != facts.variant.id:
            raise Refusal(
                "audit",
                f"{_relative(layout_rs_path)} declares variant {written['LAYOUT_ID']!r}, this bake selected "
                f"{facts.variant.id!r} — the module that would compile is not the one this manifest was "
                "written for (§5.0)",
            )
        write_atomic(assets_rs_path, assets_rs(config, tracks, cursor, facts, longest_title, art_meta).encode("utf-8"))
        write_atomic(report_path, report_text(spec, [facts], budget=budget,
                                              budget_variant=facts.id).encode("utf-8"))
        # A track that left the config leaves its blob behind otherwise, and the
        # determinism leg would then be comparing a directory that carries a
        # track the manifest no longer names. `assets/` is generated (§2.3), so
        # this deletes only bytes a previous bake of this tool wrote.
        for stale in sorted(assets_dir.glob("track*.gfp")):
            if stale.name not in written:
                stale.unlink()
                print(f"album_bake: removed stale {stale.name} — no track in the config addresses it")

        print(
            f"album_bake: layout audit — {_relative(DEFAULT_LAYOUT_SPEC)} declares "
            f"{', '.join(v.id for v in spec.variants)}, this album selected {facts.variant.id} "
            "(§5.0's generator runs inside this bake; the numbers below are derived from the spec and the "
            "tracked face files, and none of them is a constant in this file)"
        )
        for line in facts.lines(gate=variant.product):
            print(line)
        if not variant.product:
            print(
                f"  note: the selected variant is not product-eligible by its own `product` field; this ROM is "
                "an instrument, and §13's AP-3 would report it skipped with that reason (a fact about the "
                "config, not a refusal — §5.0's field exists so the choice is stated, not assumed)"
            )
        for other in spec.variants:
            if other.id == variant.id:
                continue
            try:
                other_facts = derive_variant(spec, other)
            except Refusal as exc:
                print(f"  variant {other.id}: not derivable — {exc.rule}: {exc.detail}")
                continue
            for line in other_facts.lines(gate=other.product):
                print(line)
        if longest_title[1]:
            box = facts.title_element.w if facts.title_element else 0
            print(
                f"  title fit (§5.5, measured at the declared face, never counted in characters): longest "
                f"folded string {longest_title[1]!r} = {longest_title[0]:.0f} px against a "
                f"{box} px box"
                + ("" if box else " — this variant declares no title text element, so it declares no box")
                + "; an over-long title is printed as a note, not refused (§5.5), and one-line elision "
                "belongs to the text module (2a PR-2), which is what makes any title renderable"
            )
        print(
            f"album_bake: layout: wrote {_relative(layout_rs_path)} "
            f"({len(layout_rs_text(facts).splitlines())} lines, untracked build output — §9.0's duty (i): "
            ".gitignore and `make clean` both name it) and {_relative(report_path)} (the same "
            "derivation as sorted key=value; `make album-layout-test` holds it byte-identical across two runs)"
        )
        print_census(config, tracks, bytes(asset_region), cursor, assets_dir, assets_rs_path)
        return 0
    finally:
        shutil.rmtree(work, ignore_errors=True)


def print_census(
    config: Config,
    tracks: list[dict],
    asset_bytes: bytes,
    album_samples: int,
    assets_dir: Path,
    assets_rs_path: Path,
) -> None:
    """The Q16 contribution: per-track cost of carriage, honestly labelled.

    These are asset-level means, not a worst-frame decode row — Q16 is the codec
    track's to close, and §10.2b says so. What the app can hand across is what a
    real pack of real music actually cost.
    """
    seconds_total = album_samples / PRODUCT_CLOCK_HZ
    print("album_bake: Q16 census (asset-level mean, not a worst-frame decode row)")
    for index, track in enumerate(tracks):
        seconds = track["n_samples"] / PRODUCT_CLOCK_HZ
        kbit_s = track["asset_len"] * 8 / seconds / 1000 if seconds else 0.0
        print(
            f"  track {index}: "
            f"{track['title']!r} "
            f"{dec(track['n_samples'])} samples, {dec(track['asset_len'])} B, "
            f"{kbit_s:.1f} kbit/s, recip 0x{track['recip']:08x}, "
            f"sha256 {track['blob_sha256'][:12]}"
        )
    # Q15 says report the asset region as it is ("11.8 MB of a 32 MiB cart"), and
    # the minutes-per-cart figure as a *duration-weighted* mean so aliasing — a
    # fixture device, not a codec property — cannot make the cart look bigger
    # than it is. Both numbers are printed, labelled, for the same reason.
    encoded_bytes = sum(track["asset_len"] for track in tracks)
    mean_kbit_s = encoded_bytes * 8 / seconds_total / 1000 if seconds_total else 0.0
    minutes_per_cart = (
        config.cart_bytes / (mean_kbit_s * 1000 / 8) / 60 if mean_kbit_s else 0.0
    )
    blobs = len({track["asset_off"] for track in tracks})
    print(
        f"  totals: {dec(len(asset_bytes))} B in the asset region of "
        f"{dec(config.cart_bytes)} B cart ({100 * len(asset_bytes) / config.cart_bytes:.1f}%), "
        f"{blobs} blob(s), {seconds_total / 60:.2f} album minutes"
    )
    print(
        f"  {mean_kbit_s:.1f} kbit/s duration-weighted asset-level mean => "
        f"~{minutes_per_cart:.1f} min/cart "
        f"(R2 measured ~{R2_MINUTES_PER_CART} min of real music; the region is "
        f"{'smaller than' if len(asset_bytes) < encoded_bytes else 'equal to'} the sum of the "
        f"tracks because an aliased pair shares one blob)"
    )
    print(f"album_bake: wrote {assets_dir}/albm1.bin, {blobs} blob(s), {assets_rs_path}")


# --- album-layout-gen: the tracked spec, the derived facts, the refusals ------
#
# §5.0's generator, and it lives inside this file on purpose: `make album-bake`
# is already the step that turns a tracked config into generated Rust, and §5.0's
# step 5 is the second half of that same step. A separate build target would put
# a second tool on a phase's critical path (§11's rule), and §11's 2a PR-1 row
# asks for it in these words: "`album-layout-gen` inside the existing
# `scripts/album_bake.py` (no new build target for it)".
#
# What it owns, because every one of these is a *derived* number and the plan
# spends a corrective (D9) on the cost of transcribing one:
#   * the band table and the region, from tile rows (§5.0);
#   * the legal art-box list, and the pool term taken at that list's ceiling — at
#     2a there is no album-side box to read, so the ceiling is what gets audited
#     (§7, §9.0's row);
#   * every fit box, from the declared element's box and the declared face's real
#     advance widths — never a character count (§5.5);
#   * the text-row counts, as `floor(element_height / row_px)`: a text-row count
#     is a *consequence* of a pitch, and §11's D9 records the doubled arithmetic
#     that follows from quoting one instead;
#   * the bar grid, fed *back* into the reciprocal arithmetic above so a variant's
#     cell count and the pack's reciprocals cannot drift apart (§6);
#   * the tile-pool audit per variant, with **both bounds** printed (§5.1).
#
# And §9.0's nine refusals 2a owns, each asserted by `id` by `--layout-self-test`
# on a spec written to be wrong about it. Seven read nothing but the spec and the
# tracked face files; `bar-cells-disagree` also reads the manifest, which is the
# whole point of that row; `title-fit` reads a *box* the spec declares, which is the
# whole point of its row (§5.5's elision is what turns an over-long title into a note,
# so the only title case left that is a build failure is a box too small to render one).

DEFAULT_LAYOUT_SPEC = REPO / "examples/album_player/layout_spec.toml"
DEFAULT_LAYOUT_RS = REPO / "examples/album_player/src" / "layout.rs"

#: §6's floor, quoted from the section. Its ceiling is arithmetic rather than
#: taste — `bar_recip` fits a `u32` while `bar_cells < n_samples`, which no real
#: track is near — and that ceiling is checked where the sample counts are known
#: (`bar_recip`, below). What this tool adds is one derived condition, for the
#: reason §6's own no-divide rule gives: a cell must be a whole number of pixels
#: inside the box the variant declares, or drawing the bar from a cell count needs
#: a divide in the UI path.
BAR_CELLS_FLOOR = 16

#: §9.0's register, the rows whose tool this is. Held as data so `--layout-self-
#: test` can assert each one fires, and so a refusal can quote the condition it
#: refused instead of paraphrasing it.
LAYOUT_REGISTER = (
    ("layout-variant-unknown", "`album.toml`'s `[layout] variant` names something the spec does not declare"),
    ("bands-not-tiling", "the band table does not tile exactly the declared screen rows, or a declared box falls outside its region"),
    ("pitch-not-tileable", "an element's row_px is not a whole number of tile rows or is below the declared face's line box at its declared raster, or that raster is neither a design size nor a whole multiple of one"),
    ("face-not-resolvable", "a declared face is not a tracked asset, or its licence rows are absent"),
    ("image-tiles-over-budget", "the resident-set pool sum — every declared scene's own tiles (resident, plus a projection for scenes not yet built) + the shared text term + authored chrome art — exceeds the declared tile_budget, or the declared resident set is above §5.1's layer cap"),
    ("scene-minimum", "a scene's own declared minimum cannot be met at the declared geometry"),
    ("bar-cells-out-of-range", "declared bar_cells is below §6's floor, or is not a whole number of pixels per cell in the box the variant declares. The third §6 range condition — a track so short its reciprocal would not fit the pack's u32 — is refused at the track, not at the grid, and so by the Phase-1 id `bar-reciprocal` (§9.0's `bar-cells-out-of-range` row says the same)"),
    ("bar-cells-disagree", "declared bar_cells is not the grid the ALBM1 reciprocals were computed for, or not the value in layout.rs"),
    ("title-fit", "a title the one-line elision still cannot render: the box the variant declares for the element that draws titles is narrower than the protected prefix plus the ellipsis marker at the declared face and raster (§5.5, D9). An over-long title on a box that is merely small is elided and printed as a note, so the input this row refuses is a box, not a string — which is why its leg narrows a box"),
)

#: §9.0's two cart-budget rows, **re-homed from Phase 3 by 2b PR-(1)** (§11's 2b note:
#: "a generator that cannot say 'this album does not fit' is not a budget tool"). They
#: are a separate register because their inputs are an album and a track list, not
#: `layout_spec.toml` — and because the 2a register's count (nine) is a claim §13's
#: AP-3 row makes and this one must not silently inflate. `over-track-count` is the
#: Phase-1 `track-count` refusal under its §9.0 name: the row always existed, the
#: register is what had it unowned.
BUDGET_REGISTER = (
    ("over-budget", "code reserve (§10.1's 256 KiB) + ALBM1 (with its audio asset region inside it) + the art term exceeds the declared cart"),
    ("over-track-count", "more tracks than ALBM1 can carry (more than 32), refused at the config before a byte is packed"),
)

#: The tenth row of the same register, listed so it is never counted: 2a builds
#: the validator and the derived list, 2b ships the `[art]` key it reads (D2 keeps
#: that whole section a named refusal through 2a). It is reported named-and-
#: skipped, in the idiom AP-3 uses for a non-product variant — "rather than
#: claiming nine" (§13's AP-3 row, §9.0's note under its register).
ART_ROW_NOT_FIREABLE_YET = (
    "art-box-not-derived",
    "[art] box is not one the selected variant derives",
    "2a owns the derived list and the validator; the value this refusal reads lives in "
    "album.toml's [art], which D2 keeps a `config-section` refusal through 2a",
)

#: Element kinds the spec may declare. Anything else is a spec error: §5.0's
#: element table is small on purpose, so a kind not in it is a typo.
ELEMENT_KINDS = ("text", "art", "bar")
#: §5.2's scene table as a **closed vocabulary** (2b PR-(1), ALBUM-PLAYER.md §11's
#: 2b note). A scene's `kind` is how its tile terms are derived, and it is declared
#: rather than read off the scene's name: the audit sums a *resident set*, so a
#: name-keyed rule ("a scene called `art` holds an image") would make a second
#: album's scene set a code change instead of a config one. The terms below are the
#: scene's *own* tiles — what it adds to the variant:
#:   * `fill`      — nothing: a flat region fill is the shared region-backing tile,
#:                   already counted in the chrome term;
#:   * `text`      — nothing: glyphs come from the variant's one shared atlas (§5.5a's
#:                   one face per variant), which is a variant term and not a scene's;
#:   * `art`       — the image term at the variant's derived ceiling (no album-side box
#:                   exists until `[art]` lands in 2b, §5.0);
#:   * `composite` — the sum of the image terms of the art elements it declares, and
#:                   the shared atlas if it declares text.
SCENE_KINDS = ("fill", "text", "art", "composite")
ATLAS_MODES = ("static", "per-cell")
PALETTE_ROLES = ("text", "chrome", "art", "viz", "game")

#: §10.1's cart budget, re-homed to this tool by 2b PR-(1) (§9.0's register row:
#: "a generator that cannot say 'this album does not fit' is not a budget tool").
#: The code reserve is the 256 KiB §10.1 reserves *before* the encoder sizes audio;
#: a 4 bpp tile is 32 B, which is what prices an art box until `album_art.py` exists.
CODE_RESERVE_BYTES = 256 * 1024
ART_TILE_BYTES = 32
REPORT_SCHEMA = "album-layout-report/1"

#: 2b PR-(2)'s measured coefficients: the render probe's live-audio legs, from
#: `examples/album_probe/src/bin/render_probe.rs` (AP-3b's L1-L4), run headless on
#: mGBA with the tree's usual arm (WAITCNT 4/2) on 2026-11-25. The bin prints sums,
#: counts, minima and maxima; the means below are derived on the host, which is the
#: rule the ROM follows (§6 keeps a divide off the measured path).
#:
#: These are **mGBA numbers**. §13 AP-3b says the hardware leg for each is AP-7's,
#: and this file keeps that distinction rather than upgrading an emulator reading to
#: a law: every term below names which row it came from, and the report's
#: `[measurement render]` block carries the raw folds so a later run can disagree
#: with these numbers visibly instead of silently.
MEASURED_RENDER_AT = (
    "2b PR-(2) render probe (AP-3b L1-L4), mGBA headless, WAITCNT 4/2, audio live, "
    "2026-11-25; means derived host-side from the bin's fold sums and counts"
)
#: L1, one changed bar cell (8x8) — the element row §10.2 prices as 1-2 tiles.
MEASURED_BAR_CELL_LIVE = 489
#: L1, one clock digit: two tile rows at the 16 px pitch.
MEASURED_CLOCK_DIGIT_LIVE = 963
#: L1, one cell re-shown with the `(tileset, index)` it already holds. The plan
#: expected "near-free" (agb's early-out); the measurement says 83% of a changed
#: cell, because `set_tile_at_pos` adds and removes a reference *before* it compares
#: the map entries (`regular_background.rs:360-378`). The saving is the map write
#: and the dirty mark, not the allocator churn.
MEASURED_UNCHANGED_CELL_LIVE = 407
#: L2, a whole 600-cell region repaint (30x20), mean of eight windows.
MEASURED_REGION_600_LIVE = 550_437
#: L2's per-cell figure: the coefficient the arithmetic terms below use.
MEASURED_REGION_PER_CELL_LIVE = 917
#: L3, the priority store itself: `set_priority` is two stores and a field write.
MEASURED_SET_PRIORITY_WRITE_LIVE = 73
#: L3, the whole switch: set_priority + show + commit with both scenes resident.
#: The commit is the price (mean 212,102 c, max 280,360 c — one video period's halt
#: is the worst case, and §5.1's "tens of cycles" was right about the *write* only).
MEASURED_SCENE_SWITCH_LIVE = 212_386
#: L4: the stand-up of art (256) + atlas (188) + icons (16), 856 cell references.
MEASURED_POOL_DISTINCT = 460
MEASURED_POOL_REFS = 856
MEASURED_POOL_SHARED = 189
MEASURED_POOL_MAX_REFS = 45
MEASURED_POOL_SLOTS = 512

#: The per-tile coefficient the *arithmetic* terms (title line, the declared
#: region) multiply by. It is L2's per-cell figure, not AP-0's 1,242: the probe
#: measured the app's own repaint shape, and the difference is the allocator's
#: dedupe on repeated tiles rather than a correction to AP-0's upload leg.
CYCLE_PER_TILE_LIVE = MEASURED_REGION_PER_CELL_LIVE
CYCLE_COEFFICIENT_SOURCE = (
    "2b PR-(2) render probe L2: 600-cell region repaint, live audio, "
    f"{MEASURED_REGION_600_LIVE} c / 600 cells = {MEASURED_REGION_PER_CELL_LIVE} c per cell "
    "(mean of 8 windows); the arithmetic terms below multiply by it, and the terms the "
    "probe measured directly are tagged measured instead"
)


def git_tracked(path: Path) -> bool:
    """Is this path tracked in git? (§5.5's face rule, and the only way to ask it.)

    A face that is *present* but untracked is exactly the state that matters: a
    clone of this tree has no such file, and a fit or pool leg that depends on a
    file only one machine has is that machine's gate, not the plan's.
    `git ls-files --error-unmatch` is the question, not a proxy for it.
    """
    try:
        rel = path.resolve().relative_to(REPO)
    except ValueError:
        return False
    proc = subprocess.run(
        ["git", "-C", str(REPO), "ls-files", "--error-unmatch", str(rel)],
        capture_output=True,
    )
    return proc.returncode == 0


def ceil_div(value: int, by: int) -> int:
    return -(-value // by)


def _checked(doc: dict, allowed: tuple[str, ...], where: str) -> dict:
    """Unknown keys are refused, never ignored.

    The same reasoning as `config-section`: a spec key this generator silently
    ignores is a spec that lies about what it controls, and a typo in `row_px`
    would otherwise become a default nobody wrote.
    """
    unknown = sorted(set(doc) - set(allowed))
    if unknown:
        raise Refusal(
            "spec",
            f"{where}: unknown key(s) {', '.join(unknown)}; this spec declares {', '.join(allowed)}",
        )
    return doc


def _int(doc: dict, field: str, where: str, minimum: int | None = None) -> int:
    value = doc.get(field)
    if isinstance(value, bool) or not isinstance(value, int):
        raise Refusal("spec", f"{where}: {field} is required and must be an integer")
    if minimum is not None and value < minimum:
        raise Refusal("spec", f"{where}: {field} {value} is below {minimum}")
    return value


class FaceSpec:
    """One `[[face]]`: a tracked cut, the raster it is declared at, its licence rows."""

    KEYS = ("id", "file", "raster", "product_face", "licence", "parent")

    def __init__(self, doc: dict, index: int) -> None:
        _checked(doc, self.KEYS, f"[[face]] #{index}")
        self.index = index
        self.id = str(doc.get("id", ""))
        self.file = str(doc.get("file", ""))
        self.raster = doc.get("raster")
        self.product_face = doc.get("product_face")
        self.licence = list(doc.get("licence", []))
        self.parent = str(doc.get("parent", ""))
        for field in ("id", "file", "licence"):
            if not getattr(self, field):
                raise Refusal("spec", f"[[face]] #{index}: `{field}` is required (§5.5: a tracked cut, and its licence rows)")
        if not isinstance(self.raster, int) or isinstance(self.raster, bool) or self.raster <= 0:
            raise Refusal("spec", f"[[face]] {self.id or index}: raster must be a positive pixel size")
        if not isinstance(self.product_face, bool):
            raise Refusal("spec", f"[[face]] {self.id}: product_face must be a boolean (§5.5's ruling is a property of the asset, not of a variant's wish)")
        if not self.file.endswith(".ttf"):
            raise Refusal("spec", f"[[face]] {self.id}: {self.file} is not a .ttf")

    def path(self) -> Path:
        path = Path(self.file)
        return path if path.is_absolute() else REPO / path

    def licence_paths(self) -> list[Path]:
        out = []
        for name in self.licence:
            path = Path(name)
            out.append(path if path.is_absolute() else REPO / path)
        return out


#: §5.5's one-line elision marker, and §5.5a's reason it is three periods rather than
#: U+2026: a real ellipsis is a glyph the atlas has to carry, which is one more glyph ×
#: two tile rows = 190 tiles and a distinct-pool cap of 190, moving §5.1's 444/488 to
#: 446/490. Three periods cost three advances and no table entry, so every number in
#: §5.1's tables stays exactly as printed. The marker travels into the generated text
#: module as `ELLIPSIS`; the leg in `--layout-self-test` compares the two.
ELLIPSIS = "..."


class FaceMetrics:
    """What a declared face's own tables say at a declared raster.

    Every number here comes out of the file the spec names — §9.0's `pitch-not-
    tileable` row says the tool reads "the tracked face's line metrics", and §5.1
    is emphatic that the refusal "cites the declared face's metrics rather than a
    range of allowed numbers". Nothing is transcribed from ALBUM-PLAYER.md, which
    is exactly why this can disagree with the document, and did not.
    """

    def __init__(self, face: FaceSpec, tile_px: int) -> None:
        self.face = face
        self.path = face.path()
        self.raster = face.raster
        self.tile_px = tile_px
        if not self.path.exists():
            raise Refusal(
                "face-not-resolvable",
                f"[[face]] {face.id}: {face.file} does not exist, so the declared face cannot be read "
                "(§5.5a's step 1 is the `pyftsubset` cut that makes it exist)",
            )
        if not git_tracked(self.path):
            raise Refusal(
                "face-not-resolvable",
                f"[[face]] {face.id}: {face.file} is present but not tracked in git, so a clone of this tree "
                "cannot read it — and a variant whose face resolves only on one machine is a gate that passed "
                "on that machine (§5.5, §13's AP-3 rule about what a green says it did not look at)",
            )
        missing = [
            str(path.relative_to(REPO))
            for path in face.licence_paths()
            if not path.exists() or not git_tracked(path)
        ]
        if missing:
            raise Refusal(
                "face-not-resolvable",
                f"[[face]] {face.id}: licence rows absent or untracked: {', '.join(missing)} (§5.5's licence rows)",
            )
        if TTFont is None:
            # "A declared face is not a tracked asset" is the row's condition, and
            # without a reader *no* face in the spec is resolvable. The honest
            # answer is that row, not a traceback in the middle of a bake.
            raise Refusal(
                "face-not-resolvable",
                f"[[face]] {face.id}: fontTools is not importable, so no declared face can be read here "
                "(`pip install fonttools`) — the metrics are what §9.0's face rows cite",
            )
        self._measure()

    def _measure(self) -> None:
        font = TTFont(str(self.path))
        upem = self.upem = font["head"].unitsPerEm
        cmap = font.getBestCmap()
        glyph = font["glyf"]
        hmtx = font["hmtx"]
        head = font["hhea"]
        self.ascender, self.descender, self.line_gap = head.ascender, head.descender, head.lineGap
        self.mapped = sorted(cmap)
        self.mapped_count = len(self.mapped)
        raster = self.raster

        def on_grid(value: int) -> bool:
            """Does this many font units land on a whole pixel at this raster?

            Integer arithmetic, on purpose: neither 4/800 nor 5/1000 is exactly
            representable in binary floating point, so a float-epsilon test calls a
            landable advance off the grid. That is how the first version of this
            method reported 95 off-grid codepoints where §5.1's table says 94 —
            a discrepancy that was in the tool, not in the table.
            """
            return (int(value) * raster) % upem == 0

        # `design_px`: the smallest pixel size at which this face is entirely on
        # the pixel grid — its design size, derived rather than read out of a
        # filename. Every coordinate and advance in the cut is an integer multiple
        # of `g`, and scaled by raster/upem it stays integral exactly when
        # upem/gcd(g, upem) divides the raster, which is the statement §5.5's rule
        # ("a design size or a whole multiple of one") makes about a pixel font.
        # The measured counts below are what make it a measurement rather than a
        # formula agreeing with itself.
        values: set[int] = set()
        outlined = 0
        for code in self.mapped:
            name = cmap[code]
            values.add(int(hmtx[name][0]))
            contoured = glyph[name]
            if not contoured.numberOfContours:
                continue
            outlined += 1
            coordinates = contoured.getCoordinates(glyph)[0]
            values.update(int(x) for (x, _) in coordinates)
            values.update(int(y) for (_, y) in coordinates)
        shared = 0
        for value in values:
            shared = math.gcd(shared, abs(value))
        self.design_px = upem // math.gcd(shared or upem, upem)
        self.outlined = outlined

        # Which glyphs are off the grid at the declared raster, in both senses
        # §5.1's grid column separates, and the advance table its capacity column
        # comes from. `off_grid_outlined` counts the ones whose *ink* lands between
        # the tile grid's lines — that is the "94 of 94" column; `off_grid` also
        # counts a glyph whose ink is fine but whose advance is fractional, which is
        # a real defect at half scale (a 2.5 px space) and is why a *declared*
        # raster is held to the stricter of the two rather than to the one that
        # matches a table's denominator.
        self.advance_units: dict[str, int] = {}
        self.advances: dict[str, float] = {}
        printable_units: list[int] = []
        off_grid_ink = 0
        off_grid_any = 0
        tallest_ink_units = 0
        for code in self.mapped:
            name = cmap[code]
            units = int(hmtx[name][0])
            char = chr(code)
            self.advance_units[char] = units
            self.advances[char] = units * raster / upem
            ink_off = False
            contoured = glyph[name]
            if contoured.numberOfContours:
                coordinates = contoured.getCoordinates(glyph)[0]
                ys = [y for (_, y) in coordinates]
                tallest_ink_units = max(tallest_ink_units, max(ys) - min(ys))
                ink_off = any(not on_grid(x) or not on_grid(y) for (x, y) in coordinates)
            if ink_off:
                off_grid_ink += 1
            if ink_off or not on_grid(units):
                off_grid_any += 1
            if 0x21 <= code <= 0x7E:
                printable_units.append(units)
        self.off_grid = off_grid_any
        self.off_grid_outlined = off_grid_ink
        self.grid_aligned = off_grid_any == 0
        self.printable = len(printable_units)
        mean_units = sum(printable_units) / len(printable_units) if printable_units else 0.0
        self.avg_advance_px = mean_units * raster / upem
        self.max_advance_units = max(printable_units) if printable_units else 0
        self.max_advance_px = self.max_advance_units * raster / upem
        self.ink_px = tallest_ink_units * raster / upem
        # §5.1's "chars in a 240 px line" column: a pessimistic full-width capacity
        # figure, floor of a line edge to edge across the screen, never a floor on
        # what a list with margins and a duration column can show.
        self.cells_in_240px = math.floor(240 / self.avg_advance_px) if self.avg_advance_px else 0

        # The line box, from the face's own `hhea` triple in integer units at the
        # declared raster: ceil of (ascender - descender + lineGap) x raster / upem.
        # This is the number §5.1's table is — 16 px for `ark10` at 10, 12 px for
        # `fusion8` at 8, 13 px for `ark10` at 8 (a figure no rounding rule in prose
        # produced: it is ceil(12.8)) — and it is the arithmetic behind §5.0's "the
        # title band's 2 rows = 16 px is a floor, not a taste call", because the
        # smallest line box either tracked face produces at any legal raster is two
        # tile rows.
        height = (self.ascender - self.descender + self.line_gap) * raster
        self.line_box_px = ceil_div(height, upem)
        self.min_row_px = self.tile_px * ceil_div(self.line_box_px, self.tile_px)
        self.tile_rows_per_line = ceil_div(self.line_box_px, self.tile_px)

    def measure_text(self, text: str) -> tuple[int, str]:
        """The width a string occupies, in the file's own units, and what has no glyph.

        Units, not pixels: §5.5's fit verdict is then an integer comparison against
        a box, with no rounding step between measuring a string and deciding whether
        it fits in it.
        """
        total = 0
        missing = ""
        for char in text:
            units = self.advance_units.get(char)
            if units is None:
                missing += char
            else:
                total += units
        return total, missing

    def width_px(self, text: str) -> tuple[float, str]:
        """The same width in pixels, for a log line a person reads."""
        units, missing = self.measure_text(text)
        return units * self.raster / self.upem, missing

    def fits(self, text: str, box_px: int) -> tuple[bool, float, str]:
        """§5.5's fit verdict: a measured pixel width compared to a declared box.

        Never a count of characters, and never a float comparison: the same
        inequality as `width_px(text) <= box_px`, done before the division.
        """
        units, missing = self.measure_text(text)
        return units * self.raster <= box_px * self.upem, units * self.raster / self.upem, missing

    def fits_units(self, units: int, box_px: int) -> bool:
        """The same verdict with the measuring already done.

        `elide` needs it per candidate cut, and a cut is a prefix of a string it has
        already measured — so the inequality lives here once and both callers use it.
        """
        return units * self.raster <= box_px * self.upem

    def elide(self, text: str, box_px: int, keep: int = 0) -> str:
        """§5.5's one-line elision: `text` as it can be drawn inside `box_px`, never wrapped.

        The rule as §5.5 states it: *"measure, and if it does not fit, cut at the last whole
        glyph that fits plus an ellipsis; **the track number is never cut**, because `NN ·` is
        the part a listener uses to ask for the song"*. `keep` is that sentence as a number:
        the count of leading code points that may not be cut. A caller drawing a bare title —
        which is all 2a's title band draws — passes 0; a list row drawing `NN · Title` passes
        the length of its prefix. Cutting at a code-point boundary is not a simplification:
        cutting mid-glyph is what "the last whole glyph that fits" rules out.

        Returns `""` when not even `keep` code points plus the marker fit the box. That is not
        an album's fault, and it is the one condition §9.0's `title-fit` row refuses: a box
        narrower than the ellipsis at the declared face and raster cannot render any title at
        all, elided or not.

        Code points the face has no glyph for are left in place at width zero, as
        `measure_text` counts them: eliding is a decision about width, and §5.5 keeps "no
        glyph" a separate verdict. `fits` still reports them.
        """
        chars = list(text)
        # Prefix sums of the advances, so each candidate cut costs one addition. The empty
        # prefix is index 0 and `cut[n]` is the width of the first n code points.
        cut = [0]
        for char in chars:
            cut.append(cut[-1] + self.advance_units.get(char, 0))
        if self.fits_units(cut[-1], box_px):
            return text
        keep = max(0, min(keep, len(chars)))
        marker = sum(self.advance_units.get(char, 0) for char in ELLIPSIS)
        if not self.fits_units(cut[keep] + marker, box_px):
            return ""
        # `cut[n]` is a prefix sum from 0, so for any n >= keep it already includes the
        # protected prefix: the width of "prefix + the first n code points + marker" is one
        # addition, and the first n from the top that fits is "the last whole glyph that
        # fits plus an ellipsis".
        for n in range(len(chars), keep - 1, -1):
            if self.fits_units(cut[n] + marker, box_px):
                return "".join(chars[:n]) + ELLIPSIS
        return "".join(chars[:keep]) + ELLIPSIS


class BandSpec:
    KEYS = ("id", "rows", "always")

    def __init__(self, doc: dict, where: str) -> None:
        _checked(doc, self.KEYS, where)
        self.id = str(doc.get("id", ""))
        if not self.id:
            raise Refusal("spec", f"{where}: id is required")
        self.rows = _int(doc, "rows", f"[[band]] {self.id}", 1)
        always = doc.get("always")
        if not isinstance(always, bool):
            raise Refusal("spec", f"{where}: always must be a boolean (§5.0: what is always shown is declared)")
        self.always = always
        # Filled in by the derivation, bottom-up: the always-shown bands are the
        # bottom of the screen and the region is everything above them (§5.0's
        # figure: region, then title band, then chrome at y=144).
        self.screen_row = 0


class SceneSpec:
    KEYS = ("name", "phase", "band", "kind", "resident", "min_visible_rows", "min_boxes", "shares_image")

    def __init__(self, doc: dict, where: str) -> None:
        _checked(doc, self.KEYS, where)
        self.name = str(doc.get("name", ""))
        self.phase = str(doc.get("phase", ""))
        self.band = str(doc.get("band", ""))
        if not self.name or not self.band or not self.phase:
            raise Refusal("spec", f"{where}: name, phase and band are required (§5.2's scene registry; `phase` is which phase builds it)")
        # 2b PR-(1): the two fields that turn §5.2's scene table into something the
        # audit can sum. Both are required, because a default would be the count-shaped
        # assumption this PR exists to remove: a scene with no declared kind has no
        # derivable tile term, and silence about residency is how a scene's tiles get
        # discovered at run time instead of refused at bake time (R14).
        self.kind = str(doc.get("kind", ""))
        if self.kind not in SCENE_KINDS:
            raise Refusal(
                "spec",
                f"{where}: kind {self.kind!r} is not one of {', '.join(SCENE_KINDS)} — §5.2's scene "
                "vocabulary is closed, because the resident-set audit derives a scene's tile term from "
                "this field (ALBUM-PLAYER.md §11's 2b PR-(1))",
            )
        resident = doc.get("resident")
        if not isinstance(resident, bool):
            raise Refusal(
                "spec",
                f"{where}: resident must be a boolean. §5.1's residency is per scene: a count cannot say "
                "*which* scene holds tiles, and the audit sums the scenes a ROM actually keeps resident",
            )
        self.resident = resident
        # 2b PR-(3): a scene that draws *another scene's* image (the title card's cover
        # vs the art scene's). Declaring it is what keeps the projection sum from
        # counting one tracked asset twice; the validation lives in `_scenes`, where
        # the named scene's derived box list is available.
        self.shares_image = str(doc.get("shares_image", ""))
        self.min_visible_rows = None
        self.min_boxes = None
        for field in ("min_visible_rows", "min_boxes"):
            if doc.get(field) is not None:
                value = _int(doc, field, f"[[scene]] {self.name}", 0)
                setattr(self, field, value)


class ElementSpec:
    KEYS = ("id", "kind", "band", "scene", "x0", "y0", "x1", "y1", "face", "row_px", "banks")

    def __init__(self, doc: dict, where: str) -> None:
        _checked(doc, self.KEYS, where)
        self.id = str(doc.get("id", ""))
        self.band = str(doc.get("band", ""))
        if not self.id or not self.band:
            raise Refusal("spec", f"{where}: id and band are required")
        where = f"[[element]] {self.id}"
        self.kind = str(doc.get("kind", ""))
        if self.kind not in ELEMENT_KINDS:
            raise Refusal("spec", f"{where}: kind {self.kind!r} is not one of {', '.join(ELEMENT_KINDS)}")
        self.scene = str(doc.get("scene", ""))
        self.face = str(doc.get("face", ""))
        self.row_px = doc.get("row_px", 0)
        if self.row_px is None:
            self.row_px = 0
        self.row_px = _int(doc, "row_px", where, 0) if doc.get("row_px") is not None else 0
        if self.kind == "text" and self.row_px <= 0:
            raise Refusal("spec", f"{where}: a text element needs row_px in pixels — the pitch is declared and the row count is derived (§5.0, §11's D9)")
        self.banks = list(doc.get("banks", []))
        if not all(isinstance(bank, int) and not isinstance(bank, bool) and 0 <= bank <= 15 for bank in self.banks):
            raise Refusal("spec", f"{where}: banks are palette bank indices 0..15 (§5.4)")
        for field in ("x0", "y0", "x1", "y1"):
            setattr(self, field, _int(doc, field, where))
        if self.x1 < self.x0 or self.y1 < self.y0:
            raise Refusal("spec", f"{where}: x1/y1 come before x0/y0")
        if self.kind != "text" and self.row_px:
            raise Refusal("spec", f"{where}: a {self.kind} element declares no pitch — row_px belongs to text")
        if self.kind == "text" and not self.face:
            raise Refusal("spec", f"{where}: a text element names no face; the face is a per-element declared field (Q19, §5.5)")

    def tiles(self) -> tuple[int, int, int, int]:
        """x, y, cols, rows — tile coordinates, band-local, as declared."""
        return (self.x0, self.y0, self.x1 - self.x0 + 1, self.y1 - self.y0 + 1)


class ControlSpec:
    KEYS = ("key", "band", "slot", "x0", "y0", "x1", "y1")

    def __init__(self, doc: dict, where: str) -> None:
        _checked(doc, self.KEYS, where)
        self.key = str(doc.get("key", ""))
        self.band = str(doc.get("band", ""))
        self.slot = str(doc.get("slot", ""))
        if not self.key or not self.band or not self.slot:
            raise Refusal("spec", f"{where}: key, band and slot are required (§5.3's control table: the slots are declared, so the art can be authored against them)")
        for field in ("x0", "y0", "x1", "y1"):
            setattr(self, field, _int(doc, field, f"[[control]] {self.key}"))
        if self.x1 != self.x0 or self.y1 != self.y0:
            raise Refusal("spec", f"[[control]] {self.key}: a control slot is one tile — §5.3's controls are one authored art tile each")


class PaletteSpec:
    KEYS = ("role", "banks")

    def __init__(self, doc: dict, where: str) -> None:
        _checked(doc, self.KEYS, where)
        self.role = str(doc.get("role", ""))
        if self.role not in PALETTE_ROLES:
            raise Refusal("spec", f"{where}: role {self.role!r} is not one of {', '.join(PALETTE_ROLES)} (§5.4's allocation)")
        self.banks = list(doc.get("banks", []))
        if not self.banks or not all(isinstance(bank, int) and not isinstance(bank, bool) and 0 <= bank <= 15 for bank in self.banks):
            raise Refusal("spec", f"{where}: banks is a non-empty list of palette bank indices 0..15")


class VariantSpec:
    """One `[[variant]]`: §5.0's two mock rows, written as fields instead of prose."""

    KEYS = (
        "id", "product", "tile_budget", "image_tiles", "bar_cells",
        "chrome_art_tiles", "glyph_atlas", "band", "scene", "element", "control", "palette",
    )

    def __init__(self, doc: dict, index: int) -> None:
        _checked(doc, self.KEYS, f"[[variant]] #{index}")
        self.id = str(doc.get("id", ""))
        if not self.id:
            raise Refusal("spec", f"[[variant]] #{index}: id is required")
        where = f"[[variant]] {self.id}"
        product = doc.get("product")
        if not isinstance(product, bool):
            raise Refusal("spec", f"{where}: product must be a boolean (§5.0's field: which variant a ROM ships and which is an instrument — AP-3's snapshot rule reads it)")
        self.product = product
        for field in ("tile_budget", "image_tiles", "bar_cells", "chrome_art_tiles"):
            setattr(self, field, _int(doc, field, where, 0))
        self.glyph_atlas = str(doc.get("glyph_atlas", ""))
        if self.glyph_atlas not in ATLAS_MODES:
            raise Refusal("spec", f"{where}: glyph_atlas must be one of {', '.join(ATLAS_MODES)} — §5.1's two bounds are these two builds, and the audit reads which one is claimed")
        self.bands = [BandSpec(item, f"{where} [[band]] #{i}") for i, item in enumerate(doc.get("band") or [])]
        self.scenes = [SceneSpec(item, f"{where} [[scene]] #{i}") for i, item in enumerate(doc.get("scene") or [])]
        # 2b PR-(1): residency is declared per scene (`SceneSpec.resident`) and the
        # count is *derived*. The old `resident_scenes = N` field was the count-shaped
        # assumption the ladder names: it could not say which scene was resident, so
        # the audit had to assume one scene's terms and could not be walked. This
        # attribute survives as the derived count because §5.1's hardware cap is a
        # limit on the number of content layers, and `layout.rs` carries it as
        # `RESIDENT_SCENES` for the compile-time assert.
        self.resident_scenes = sum(1 for scene in self.scenes if scene.resident)
        self.elements = [ElementSpec(item, f"{where} [[element]] #{i}") for i, item in enumerate(doc.get("element") or [])]
        self.controls = [ControlSpec(item, f"{where} [[control]] #{i}") for i, item in enumerate(doc.get("control") or [])]
        self.palettes = [PaletteSpec(item, f"{where} [[palette]] #{i}") for i, item in enumerate(doc.get("palette") or [])]
        for field, items, key in (
            ("band", self.bands, "id"), ("scene", self.scenes, "name"), ("element", self.elements, "id"),
            ("control", self.controls, "key"), ("palette", self.palettes, "role"),
        ):
            names = [getattr(item, key) for item in items]
            if len(set(names)) != len(names):
                raise Refusal("spec", f"{where}: duplicate [[{field}]] entry")
        if not self.bands:
            raise Refusal("spec", f"{where}: at least one [[band]] — §5.0's band table is the layout, and the thing generated at compile time is a band table with a slot")


class LayoutSpec:
    """The tracked spec, parsed and self-consistent (§5.0)."""

    KEYS = ("meta", "face", "variant")
    META_KEYS = ("screen_tiles", "map_tiles", "tile_px", "pool_slots", "resident_scene_max")

    def __init__(self, path: Path) -> None:
        self.path = path
        try:
            with open(path, "rb") as handle:
                doc = tomllib.load(handle)
        except (OSError, tomllib.TOMLDecodeError) as exc:
            raise Refusal("spec", f"{path}: {exc}") from exc
        _checked(doc, self.KEYS, str(path))
        meta = _checked(doc.get("meta") or {}, self.META_KEYS, f"{path}: [meta]")
        if not meta:
            raise Refusal("spec", f"{path}: [meta] is required — the hardware facts the spec is checked against")
        for field in self.META_KEYS:
            value = meta.get(field)
            if field.endswith("tiles"):
                if not (isinstance(value, list) and len(value) == 2
                        and all(isinstance(n, int) and not isinstance(n, bool) and n > 0 for n in value)):
                    raise Refusal("spec", f"{path}: [meta] {field} is a pair of positive tile counts")
                setattr(self, field, (value[0], value[1]))
            else:
                setattr(self, field, _int(meta, field, f"{path}: [meta]", 1))
        if self.map_tiles[0] < self.screen_tiles[0] or self.map_tiles[1] < self.screen_tiles[1]:
            raise Refusal("spec", f"{path}: [meta] map_tiles is smaller than screen_tiles — the map is the hardware's, the screen is the visible part of it")
        if self.pool_slots > 512:
            # agb 0.25's regular pool is 512 4bpp slots and the affine pool is a
            # *different* 510 with a fallback path between them (§5.1). A spec
            # naming 1,024 is reading the two as one pool, which is the error
            # §5.1's rewrite exists to prevent — so the bound is the larger of the
            # two real pools, not a number above both of them. Under 512 is a
            # smaller claim, not a wrong one; `album-layout-test` asks this guard at
            # 512, 256 and 1,024 because a refusal nothing fires is not a guard.
            raise Refusal("spec", f"{path}: [meta] pool_slots {self.pool_slots} is not a pool agb 0.25 hands out (§5.1: 512 regular, 510 affine, two pools — 1,024 is those two read as one, which is the mistake this row refuses)")
        self.faces = [FaceSpec(item, index) for index, item in enumerate(doc.get("face") or [])]
        self.variants = [VariantSpec(item, index) for index, item in enumerate(doc.get("variant") or [])]
        if not self.variants:
            raise Refusal("spec", f"{path}: no [[variant]] declared — a layout spec with no variant is a description of nothing")
        if len({variant.id for variant in self.variants}) != len(self.variants):
            raise Refusal("spec", f"{path}: duplicate [[variant]] id")
        if len({face.id for face in self.faces}) != len(self.faces):
            raise Refusal("spec", f"{path}: duplicate [[face]] id")

    def face(self, face_id: str) -> FaceSpec:
        for face in self.faces:
            if face.id == face_id:
                return face
        raise Refusal("spec", f"{self.path}: [[face]] {face_id} is not declared by this spec")

    def variant(self, variant_id: str) -> VariantSpec:
        for variant in self.variants:
            if variant.id == variant_id:
                return variant
        raise Refusal(
            "layout-variant-unknown",
            f"variant {variant_id!r} is not declared by {self.path.relative_to(REPO) if self.path.is_relative_to(REPO) else self.path}; "
            f"this spec declares {', '.join(variant.id for variant in self.variants)} (§5.0: config selects a "
            "variant, it never produces runtime geometry)",
        )

    def measures(self, variant: VariantSpec, tile_px: int) -> dict[str, FaceMetrics]:
        """The faces this variant's text elements declare, read once each."""
        out: dict[str, FaceMetrics] = {}
        for element in variant.elements:
            if element.kind != "text":
                continue
            face = self.face(element.face)
            if face.id not in out:
                out[face.id] = FaceMetrics(face, tile_px)
        return out


class DerivedElement:
    """An element with its geometry resolved: tiles as declared, pixels as drawn."""

    def __init__(self, spec: ElementSpec, band: BandSpec, tile_px: int) -> None:
        self.spec = spec
        self.band = band
        self.kind = spec.kind
        self.id = spec.id
        self.scene = spec.scene
        self.face = spec.face
        self.row_px = spec.row_px
        self.banks = list(spec.banks)
        self.xt, self.yt, self.wt, self.ht = spec.tiles()
        self.x = self.xt * tile_px
        self.y = (band.screen_row + spec.y0) * tile_px
        self.w = self.wt * tile_px
        self.h = self.ht * tile_px
        self.line_box_px = 0
        self.text_rows = 0
        self.cells_per_row = 0
        self.max_chars = 0
        self.metrics: FaceMetrics | None = None


class ArtBox:
    """One derived legal art box, in tiles and pixels, with where it lands."""

    def __init__(self, side: int, element: DerivedElement, tile_px: int) -> None:
        self.side = side
        self.tiles = side * side
        self.width_px = self.height_px = side * tile_px
        # Centring inside the element's own box. An odd remainder is a four-pixel
        # off-centre, which is what §7's note about no square being perfectly
        # tile-centred in a 17-row region is saying; a fit report prints pixels
        # rather than describing them.
        spare_x, spare_y = element.wt - side, element.ht - side
        self.x_off = (spare_x // 2) * tile_px
        self.y_off = (spare_y // 2) * tile_px
        self.off_centre_x = (spare_x % 2) * (tile_px // 2)
        self.off_centre_y = (spare_y % 2) * (tile_px // 2)

    def __str__(self) -> str:
        return f"{self.width_px}x{self.height_px} ({self.tiles} tiles)"


class SceneTerm:
    """One scene's own tile term, derived from its declared kind (2b PR-(1)).

    "Own" is the word that carries the arithmetic: the band backdrops and the glyph
    atlas belong to the *variant*, shared by every scene that draws over or from them,
    so pricing them per scene would charge one 188-tile atlas twice the moment a second
    text scene was declared. A scene owns only what its kind adds — an image at the
    variant's derived ceiling for `art`, the art elements it declares for `composite`,
    and nothing for `fill`/`text` (the region backing tile and the atlas are the
    variant's shared terms, counted once).

    `audited_as` is the second half of the schema: a declared scene that is not resident
    is still *audited*, as a projection. The budget is the variant's, so landing a scene
    moves its term from `projection` to `resident` and changes no total — which is the
    property that lets 2b's scenes land one PR at a time without re-opening the budget
    (and the reason the audit is not "only what is resident today").
    """

    def __init__(self, scene: SceneSpec, own_tiles: int, terms: dict[str, int], shares_atlas: bool) -> None:
        self.scene = scene
        self.name = scene.name
        self.kind = scene.kind
        self.resident = scene.resident
        self.own_tiles = own_tiles
        self.terms = terms
        self.shares_atlas = shares_atlas

    @property
    def audited_as(self) -> str:
        if self.scene.shares_image:
            return f"shared({self.scene.shares_image})"
        return "resident" if self.resident else "projection"

    def __str__(self) -> str:
        detail = " + ".join(f"{name} {tiles}" for name, tiles in self.terms.items()) or "no own tiles"
        return f"{self.name} [{self.kind}] {detail} = {self.own_tiles}, audited as {self.audited_as}"


class VariantFacts:
    """Everything derived about one variant, and the pool audit its sum gets."""

    def __init__(self, spec: LayoutSpec, variant: VariantSpec) -> None:
        self.spec = spec
        self.variant = variant
        self.tile_px = spec.tile_px
        self.id = variant.id
        self.product = variant.product
        self.bands = {band.id: band for band in variant.bands}
        self.region_band: BandSpec | None = None
        self.region: tuple[int, int, int, int] = (0, 0, 0, 0)  # cols, rows, px, px
        self.elements: list[DerivedElement] = []
        self.control_slots: list[ControlSpec] = []
        self.face_metrics: dict[str, FaceMetrics] = {}
        self.art_boxes: list[ArtBox] = []
        self.art_ceiling: ArtBox | None = None
        self.art_element: DerivedElement | None = None
        self.bar_element: DerivedElement | None = None
        self.bar_px = 0
        self.bar_cell_px = 0
        self.bar_pattern_tiles = 0
        self.title_element: DerivedElement | None = None
        self.text_upper = 0
        self.text_distinct = 0
        self.atlas_tiles = 0
        self.chrome_derived = 0
        self.pool_image = 0
        self.pool_distinct = 0
        self.pool_upper = 0
        self.scene_terms: list[SceneTerm] = []
        self.resident_own = 0
        self.projection_own = 0
        self.audited_own = 0
        self.notes: list[str] = []
        self._bands()
        self._resolve_faces()
        self._elements()
        self._bar()
        self._art()
        self._scene_terms()
        self._scenes()
        self._controls()
        self._pool()

    # -- the derivation, in the order the register reads top to bottom --------

    def _bands(self) -> None:
        variant = self.variant
        total = sum(band.rows for band in variant.bands)
        if total != self.spec.screen_tiles[1]:
            raise Refusal(
                "bands-not-tiling",
                f"[[variant]] {variant.id}: the [[band]] rows sum to {total} tile rows; [meta] screen_tiles says "
                f"{self.spec.screen_tiles[1]}. §5.0's band table *is* the screen, and the region is what is "
                "left over — which is why a row count in this spec is a tile row and never a text row",
            )
        swappable = [band for band in variant.bands if not band.always]
        if len(swappable) != 1:
            raise Refusal(
                "spec",
                f"[[variant]] {variant.id}: exactly one band may carry `always = false`, because that band *is* "
                f"the scene region §5.0's rectangle rule hands to a scene — this spec names {len(swappable)}",
            )
        row = self.spec.screen_tiles[1]
        for band in variant.bands:
            row -= band.rows
            band.screen_row = row
        region = swappable[0]
        self.region_band = region
        self.region = (
            self.spec.screen_tiles[0], region.rows,
            self.spec.screen_tiles[0] * self.tile_px, region.rows * self.tile_px,
        )
        for scene in variant.scenes:
            band = self.bands.get(scene.band)
            if band is None:
                raise Refusal("spec", f"[[scene]] {scene.name}: band {scene.band!r} is not declared by [[variant]] {variant.id}")
            if band.always:
                raise Refusal(
                    "spec",
                    f"[[scene]] {scene.name}: a scene is drawn in a band it can be swapped out of. §5.0's "
                    "enforcement is that a scene is one function receiving a region and nothing else, so it "
                    "cannot be handed the chrome it was never given",
                )

    def _resolve_faces(self) -> None:
        self.face_metrics = self.spec.measures(self.variant, self.tile_px)
        for face_id, metrics in self.face_metrics.items():
            if metrics.design_px and metrics.raster % metrics.design_px:
                raise Refusal(
                    "pitch-not-tileable",
                    f"[[face]] {face_id}: raster {metrics.raster} px is not a whole multiple of the design size "
                    f"this file measures ({metrics.design_px} px, from the gcd of its own coordinates) — at "
                    f"{metrics.raster} px, {metrics.off_grid_outlined} of {metrics.outlined} glyphs that carry ink "
                    f"land off the pixel grid (and {metrics.off_grid} of {metrics.mapped_count} mapped codepoints "
                    "counting a fractional advance too), so the atlas would be a resampling rather than a "
                    "rasterisation (§5.5's rule, "
                    "and §5.1's last table row is the cautionary tale: a half-scale raster fits the pool and "
                    "puts every glyph off-grid)",
                )
            if not metrics.grid_aligned:
                raise Refusal(
                    "pitch-not-tileable",
                    f"[[face]] {face_id}: at {metrics.raster} px, {metrics.off_grid} of {metrics.mapped_count} "
                    "mapped codepoints are off the pixel grid in ink or advance "
                    f"({metrics.off_grid_outlined} of them in ink) — not tileable at that size, and nothing in "
                    "the pool arithmetic objects to it, which is why this is a face rule and not a pool rule (§5.1)",
                )

    def _elements(self) -> None:
        variant = self.variant
        banks_by_role: dict[str, set[int]] = {}
        for palette in variant.palettes:
            if palette.role in banks_by_role:
                raise Refusal("spec", f"[[variant]] {variant.id}: [[palette]] {palette.role} is declared twice")
            overlap = set(palette.banks) & set().union(*banks_by_role.values() or [set()])
            if overlap:
                raise Refusal("spec", f"[[variant]] {variant.id}: banks {sorted(overlap)} appear in two [[palette]] roles (§5.4's allocation is a partition of 16 banks)")
            banks_by_role[palette.role] = set(palette.banks)
        allocated = set().union(*banks_by_role.values()) if banks_by_role else set()
        screen_cols, screen_rows = self.spec.screen_tiles
        seen: set[str] = set()
        for spec in variant.elements:
            band = self.bands.get(spec.band)
            if band is None:
                raise Refusal("spec", f"[[element]] {spec.id}: band {spec.band!r} is not declared by [[variant]] {variant.id}")
            element = DerivedElement(spec, band, self.tile_px)
            problems = []
            if element.xt + element.wt > screen_cols:
                problems.append(f"columns {element.xt}..{element.xt + element.wt - 1} against {screen_cols}")
            if element.yt + element.ht > band.rows:
                problems.append(f"rows {element.yt}..{element.yt + element.ht - 1} against band {band.id}'s {band.rows}")
            if element.y + element.h > screen_rows * self.tile_px:
                problems.append("past the bottom of the screen")
            if problems:
                raise Refusal(
                    "bands-not-tiling",
                    f"[[variant]] {variant.id} [[element]] {spec.id}: the declared box falls outside its region"
                    " — " + "; ".join(problems) + " (§9.0's row, second half: the same row that checks the bands tiles 20 rows)",
                )
            if spec.banks and not set(spec.banks) <= allocated:
                raise Refusal(
                    "spec",
                    f"[[variant]] {variant.id} [[element]] {spec.id}: banks {sorted(set(spec.banks) - allocated)} "
                    "are outside this variant's declared [[palette]] allocation (§5.4). §9.0 gives the palette "
                    "refusals to 2b, where they are about real images; this is a spec contradicting itself",
                )
            if spec.scene:
                if spec.scene not in {scene.name for scene in variant.scenes}:
                    raise Refusal("spec", f"[[element]] {spec.id}: scene {spec.scene!r} is not registered by [[variant]] {variant.id}")
                owner = next(scene for scene in variant.scenes if scene.name == spec.scene)
                if self.bands[owner.band] is not band:
                    raise Refusal("spec", f"[[element]] {spec.id}: its band disagrees with the band [[scene]] {spec.scene} names")
            elif any(scene.name == spec.scene for scene in variant.scenes):
                pass
            if spec.id in seen:
                raise Refusal("spec", f"[[variant]] {variant.id}: duplicate element id {spec.id}")
            seen.add(spec.id)
            if element.kind == "text":
                metrics = self.face_metrics.get(element.face)
                if metrics is None:  # unreachable: `measures` covers every text element
                    raise Refusal("spec", f"[[element]] {spec.id}: its face was not read")
                if element.row_px % self.tile_px:
                    raise Refusal(
                        "pitch-not-tileable",
                        f"[[variant]] {variant.id} [[element]] {spec.id}: row_px {element.row_px} is not a whole "
                        f"number of {self.tile_px} px tile rows. agb allocates tiles; a pitch between two of them "
                        "is a text row that cannot be drawn, and §5.0's first correction to its own table is "
                        "exactly this confusion",
                    )
                if element.row_px < metrics.line_box_px:
                    raise Refusal(
                        "pitch-not-tileable",
                        f"[[variant]] {variant.id} [[element]] {spec.id}: row_px {element.row_px} px is below the "
                        f"line box the declared face measures at its declared raster ({metrics.line_box_px} px, "
                        f"from hhea ascender {metrics.ascender} / descender {metrics.descender} at "
                        f"{metrics.raster} px, upem {metrics.upem}). The smallest legal pitch for this face here "
                        f"is {metrics.min_row_px} px — §5.0's title-band floor is this arithmetic, cited against "
                        "the face as §9.0's row requires",
                    )
                element.metrics = metrics
                element.line_box_px = metrics.line_box_px
                # A row count is a *consequence* (§11's D9). `cells_per_row` is the
                # pessimistic full-width figure §5.1's table computes from the mean
                # advance; a fit verdict uses the maximum, and the max-advance span
                # below is the conservative character figure a snapshot prints — the
                # refusal itself measures pixels.
                element.text_rows = element.h // element.row_px
                if metrics.avg_advance_px:
                    element.cells_per_row = math.floor(element.w / metrics.avg_advance_px)
                if metrics.max_advance_px:
                    element.max_chars = math.floor(element.w / metrics.max_advance_px)
            self.elements.append(element)
        if variant.product:
            for face_id, metrics in self.face_metrics.items():
                if not metrics.face.product_face:
                    raise Refusal(
                        "spec",
                        f"[[variant]] {variant.id} is `product = true` and declares [[face]] {face_id}, which "
                        "declares itself *not* a product face (§5.5's ruling that the shipped 8 px cut is an "
                        "instrument and verdict face). The register names no row for this because it is the spec "
                        "contradicting its own flag, and §13's AP-3 rule is about *omitting* a variant, not "
                        "about a variant claiming two things at once",
                    )
        # The title band is the always-shown band adjacent to the region (above
        # the chrome, below the region: §5.0's figure). A variant whose title band
        # carries no text line — §5.0's A — has no fit box, and this says so.
        chrome = self._chrome_band()
        title_bands = [band for band in variant.bands if band.always and band is not chrome]
        self.title_element = next(
            (element for element in reversed(self.elements) if element.kind == "text" and element.band in title_bands),
            None,
        )

    def _chrome_band(self) -> BandSpec | None:
        always = [band for band in self.variant.bands if band.always]
        return max(always, key=lambda band: band.screen_row) if always else None

    def _bar(self) -> None:
        variant = self.variant
        bars = [element for element in self.elements if element.kind == "bar"]
        if len(bars) > 1:
            raise Refusal("spec", f"[[variant]] {variant.id}: {len(bars)} bar elements — the bar is one widget in one band (§6)")
        if not bars:
            self.notes.append("this variant declares no bar element, so there is no progress geometry to derive")
            return
        element = bars[0]
        self.bar_element = element
        self.bar_px = element.w
        cells = variant.bar_cells
        if cells < BAR_CELLS_FLOOR:
            raise Refusal(
                "bar-cells-out-of-range",
                f"[[variant]] {variant.id} declares bar_cells {cells}, below §6's floor of {BAR_CELLS_FLOOR} "
                + (f"cells ({100.0 / cells:.1f}% of the track per ±1 cell)" if cells else "cells (no grid at all)")
                + ". The floor is a floor because the honest cost of a short bar is precision, not pixels",
            )
        if self.bar_px % cells:
            raise Refusal(
                "bar-cells-out-of-range",
                f"[[variant]] {variant.id}: bar_cells {cells} does not divide the {self.bar_px} px the "
                f"[[element]] {element.id} box declares ({self.bar_px / cells:.3f} px per cell). A cell that is "
                "not a whole pixel turns a cell count into a divide when it is drawn, and §6's AP-2 leg refuses "
                "a divide in the playback and UI path. Pick a cell count that divides the declared box, or widen "
                "the box — the reciprocal arithmetic itself is happy with either",
            )
        self.bar_cell_px = self.bar_px // cells
        self.bar_pattern_tiles = self._pattern_tiles()

    def _pattern_tiles(self) -> int:
        """§6's "partial-fill pattern tiles", derived instead of estimated.

        A cell is `bar_cell_px` wide, so the fill boundary can only land on those
        steps inside a tile: the distinct partial tiles are exactly the reachable
        residues, plus the fully-filled tile a track at 100% needs. §6 estimated
        "~5"; this counts.
        """
        if not self.bar_cell_px:
            return 0
        reachable = {(cell * self.bar_cell_px) % self.tile_px for cell in range(self.variant.bar_cells + 1)}
        return len(reachable) + 1

    def _art(self) -> None:
        """§7's legal box list, derived: every square this variant can draw."""
        sharing = {scene.name for scene in self.variant.scenes if scene.shares_image}
        for element in self.elements:
            if element.kind != "art" or not element.scene:
                continue
            if element.scene in sharing:
                # Not the variant's box list: this element draws the image another
                # scene already derives, and `_scenes` checks it against that list.
                continue
            self.art_element = element
            side = min(element.wt, element.ht, math.isqrt(self.variant.image_tiles))
            if side < 1:
                self.notes.append(
                    f"element {element.id}: image_tiles {self.variant.image_tiles} and the element's "
                    f"{element.wt}x{element.ht} tile box derive no legal box at all"
                )
                continue
            self.art_boxes = [ArtBox(s, element, self.tile_px) for s in range(1, side + 1)]
            self.art_ceiling = self.art_boxes[-1]
            declared_square = math.isqrt(self.variant.image_tiles) ** 2
            if declared_square != self.variant.image_tiles:
                self.notes.append(
                    f"[[variant]] {self.variant.id}: image_tiles {self.variant.image_tiles} is not a square; the largest "
                    f"box it can express is {declared_square} tiles ({declared_square ** 0.5:.0f}²), so "
                    f"{self.variant.image_tiles - declared_square} declared tiles buy nothing"
                )
            room = math.isqrt(min(element.wt, element.ht)) ** 2
            if self.variant.image_tiles < room:
                self.notes.append(
                    f"the region would physically hold a {math.isqrt(room)}x{math.isqrt(room)} box ({room} "
                    f"tiles); it is the declared image_tiles {self.variant.image_tiles} that stops it, which is "
                    "the distinction §5.0 draws between scene_region and image_tiles"
                )

    def _scenes(self) -> None:
        # 2b PR-(3): `shares_image` names another scene whose *same tracked image* this
        # one draws. Two checks, both facts about the spec: the named scene has an art
        # element to share, and this scene's box is no larger than the box that scene
        # derives — otherwise the sharing would under-count a genuinely bigger image.
        by_name = {scene.name: scene for scene in self.variant.scenes}
        for scene in self.variant.scenes:
            if not scene.shares_image:
                continue
            target = by_name.get(scene.shares_image)
            if target is None or target.name == scene.name:
                raise Refusal(
                    "spec",
                    f"[[variant]] {self.variant.id} [[scene]] {scene.name}: shares_image "
                    f"{scene.shares_image!r} names no other scene of this variant. The field is a "
                    "claim that two scenes draw one image, and a claim needs a subject",
                )
            own = [e for e in self.elements if e.scene == scene.name and e.kind == "art"]
            theirs = [e for e in self.elements if e.scene == target.name and e.kind == "art"]
            if not own:
                raise Refusal(
                    "spec",
                    f"[[variant]] {self.variant.id} [[scene]] {scene.name}: shares_image is declared "
                    "and the scene has no art element — there is nothing to share, and the scene's "
                    "own tile term would be 0 for a reason nobody wrote down",
                )
            if not theirs:
                raise Refusal(
                    "spec",
                    f"[[variant]] {self.variant.id} [[scene]] {scene.name}: shares_image names "
                    f"{target.name}, which declares no art element — an image shared with nothing",
                )
            own_side = min(own[0].wt, own[0].ht)
            their_side = min(theirs[0].wt, theirs[0].ht, math.isqrt(self.variant.image_tiles))
            if own_side < 1 or own_side > their_side:
                raise Refusal(
                    "art-box-not-derived",
                    f"[[variant]] {self.variant.id} [[scene]] {scene.name}: its art box is "
                    f"{own_side}x{own_side} tiles and {target.name}'s derived ceiling is "
                    f"{their_side}x{their_side} — a shared image cannot be larger than the image it "
                    "shares, and this is the box validator §11's PR-(3) hands the card",
                )

        for scene in self.variant.scenes:
            if scene.min_visible_rows is not None:
                rows = [element.text_rows for element in self.elements if element.scene == scene.name and element.kind == "text"]
                if not rows:
                    self.notes.append(
                        f"scene-minimum / {scene.name}: min_visible_rows {scene.min_visible_rows} is declared, "
                        f"but this variant declares no text element for that scene — named and skipped, because "
                        f"inventing a span for a scene another phase ({scene.phase}) owns would be arithmetic "
                        "nobody asked for (§5.2's minima are per scene, so the answer here is that there is no geometry yet)"
                    )
                    continue
                if max(rows) < scene.min_visible_rows:
                    raise Refusal(
                        "scene-minimum",
                        f"[[variant]] {self.variant.id} [[scene]] {scene.name}: min_visible_rows {scene.min_visible_rows}, "
                        f"but the declared geometry holds {max(rows)} text rows at the declared pitch. §5.2's own "
                        "row's number, not a global one, and a list under it has stopped being a list",
                    )
            if scene.min_boxes is not None:
                owned = [element for element in self.elements if element.scene == scene.name and element.kind == "art"]
                if not owned:
                    self.notes.append(
                        f"scene-minimum / {scene.name}: min_boxes {scene.min_boxes} is declared and no art element "
                        "of this variant addresses that scene (named and skipped, §5.2)"
                    )
                    continue
                if len(self.art_boxes) < scene.min_boxes:
                    largest = self.art_ceiling.width_px if self.art_ceiling else 0
                    raise Refusal(
                        "scene-minimum",
                        f"[[variant]] {self.variant.id} [[scene]] {scene.name}: min_boxes {scene.min_boxes} and this "
                        f"variant derives {len(self.art_boxes)} legal box(es); the largest is {largest} px. "
                        f"image_tiles {self.variant.image_tiles} is what bounds it, not the region",
                    )

    def _scene_terms(self) -> None:
        """The resident-set walk: one own-tile term per declared scene (2b PR-(1)).

        Each scene's term comes from its declared `kind` and the elements it owns — never
        from its name, and never from a count. §5.1's residency ruling holds *every* scene
        the variant may show at once, so the audit's job is to sum that set; the projection
        half keeps a declared-but-not-yet-built scene in the sum, because a budget that
        only counted today's scenes would let the next one's tiles be discovered at run
        time, which is R14's panic.
        """
        for scene in self.variant.scenes:
            elements = [element for element in self.elements if element.scene == scene.name]
            terms: dict[str, int] = {}
            if scene.kind in ("art", "composite") and not scene.shares_image:
                image = 0
                for element in elements:
                    if element.kind != "art":
                        continue
                    side = min(element.wt, element.ht, math.isqrt(self.variant.image_tiles))
                    image += max(side, 0) ** 2
                if not elements and scene.kind == "art":
                    self.notes.append(
                        f"scene {scene.name}: kind `art` but it declares no art element, so its own tile "
                        "term is 0 — the image term is only ever a box the scene can actually draw"
                    )
                if image:
                    terms["image"] = image
            # `fill`: 0 — the flat fill is the region backing tile, already in the chrome
            # term. `text`: 0 — glyphs are the variant's shared atlas, one face per variant
            # (§5.5a), and counting it per scene would double it.
            self.scene_terms.append(
                SceneTerm(scene, sum(terms.values()), terms,
                          shares_atlas=any(element.kind == "text" for element in elements))
            )

    def _controls(self) -> None:
        slots = {element.id: element for element in self.elements if element.kind == "art"}
        for control in self.variant.controls:
            target = slots.get(control.slot)
            if target is None:
                raise Refusal(
                    "spec",
                    f"[[variant]] {self.variant.id} [[control]] {control.key}: slot {control.slot!r} names no art element "
                    "of this variant (§5.3: the slots are declared so the authored art can be made against them)",
                )
            if target.band.id != control.band:
                raise Refusal("spec", f"[[control]] {control.key}: band {control.band!r} disagrees with its slot's band {target.band.id}")
            # Both sides in tile coordinates, band-local: the control's declared
            # slot has to sit inside the art element it is drawn on (§5.3's slots
            # are declared precisely so the art can be authored against them).
            if (control.x0 < target.xt or control.x1 > target.xt + target.wt - 1
                    or control.y0 < target.yt or control.y1 > target.yt + target.ht - 1):
                raise Refusal(
                    "bands-not-tiling",
                    f"[[variant]] {self.variant.id} [[control]] {control.key}: its slot sits outside the art element "
                    f"{target.id} it belongs to — a box outside its region, which is the second half of §9.0's "
                    "band row, and the art is authored against these rectangles",
                )
            self.control_slots.append(control)

    def select_image(self, tiles: int) -> None:
        """Move the image term from the variant's ceiling to the album's selected box.

        2b PR-(4)/Decision B: the *album* is what ships a box, so the budget the ROM
        asserts against is the box it actually draws. `layout.rs` keeps the variant's
        ceiling terms (`POOL_UPPER`); the selected term and the `POOL_DISTINCT` sum live
        in the album-scoped `assets.rs`, so a variant's geometry module stays
        album-independent and AP-3's snapshots keep comparing like with like.
        """
        if tiles > self.variant.image_tiles:
            raise Refusal(
                "image-tiles-over-budget",
                f"[[variant]] {self.variant.id}: the selected art box needs {tiles} tiles, above the "
                f"declared image_tiles {self.variant.image_tiles}",
            )
        self.pool_image = tiles
        for term in self.scene_terms:
            if "image" in term.terms:
                term.terms["image"] = tiles
                term.own_tiles = sum(term.terms.values())
        self.resident_own = sum(term.own_tiles for term in self.scene_terms if term.resident)
        self.projection_own = sum(term.own_tiles for term in self.scene_terms if not term.resident)
        self.audited_own = self.resident_own + self.projection_own
        self.pool_distinct = (
            self.audited_own + self.text_distinct + self.variant.chrome_art_tiles + self.bar_pattern_tiles
        )

    def _pool(self) -> None:
        variant = self.variant
        # The image term at the variant's derived ceiling: at 2a there is no
        # album-side box to read, so this is the honest term (§9.0's row).
        self.pool_image = self.art_ceiling.tiles if self.art_ceiling else 0
        used: dict[str, int] = {}
        for element in self.elements:
            if element.kind != "text" or element.metrics is None:
                continue
            self.text_upper += element.text_rows * element.cells_per_row * element.metrics.tile_rows_per_line
            used[element.face] = max(used.get(element.face, 0), element.metrics.tile_rows_per_line)
        # The distinct bound is the atlas: every glyph in the declared cut times the
        # tile rows its line box spans. §5.1's table quotes 188 for a 94-glyph cut
        # at a 16 px line box; that number is derived here from the file, which is
        # how §5.1's own instruction not to quote a glyph count anywhere is obeyed.
        self.atlas_tiles = sum(
            self.face_metrics[face_id].outlined * tile_rows
            for face_id, tile_rows in used.items()
        )
        self.text_distinct = self.atlas_tiles
        # The declared chrome count is what the audit uses (it is the enumerated
        # number a reviewer argued with); the derived count is printed beside it, so
        # a disagreement is visible rather than assumed away.
        self.chrome_derived = len(variant.bands) + 1 + len(self.control_slots)
        # 2b PR-(1)'s resident-set walk. The scene terms are the only addend that is
        # per scene; the atlas, the chrome art and the bar's pattern tiles are the
        # variant's shared tiles and are counted once, because they are shared by
        # whichever scenes draw over them (a second text scene must not buy a second
        # 188-tile atlas). `resident + projection` is the audited set: a scene that has
        # not landed yet is still priced, and landing it moves the term and changes no
        # total — the property that lets 2b land its scenes one PR at a time.
        self.resident_own = sum(term.own_tiles for term in self.scene_terms if term.resident)
        self.projection_own = sum(term.own_tiles for term in self.scene_terms if not term.resident)
        self.audited_own = self.resident_own + self.projection_own
        self.pool_distinct = self.audited_own + self.text_distinct + variant.chrome_art_tiles + self.bar_pattern_tiles
        self.pool_upper = self.audited_own + self.text_upper + variant.chrome_art_tiles + self.bar_pattern_tiles
        if variant.resident_scenes > self.spec.resident_scene_max:
            resident = ", ".join(term.name for term in self.scene_terms if term.resident) or "none"
            raise Refusal(
                "image-tiles-over-budget",
                f"[[variant]] {variant.id} declares {variant.resident_scenes} resident scene(s) ({resident}) "
                f"by their per-scene `resident` flags, above the cap of {self.spec.resident_scene_max} this "
                "spec's own [meta] states: Mode 0 has four layers and one of them belongs to the chrome "
                "permanently, so the cap is a hardware fact and not a preference (§5.0, §5.1). Residency is "
                "one of that row's two terms, and it is now a declared set rather than a count",
            )
        if self.pool_distinct > variant.tile_budget:
            raise Refusal(
                "image-tiles-over-budget",
                f"[[variant]] {variant.id}: the distinct pool sum {self.pool_distinct} exceeds the declared "
                f"tile_budget {variant.tile_budget} — scenes resident {self.resident_own} + projection "
                f"{self.projection_own} (the audited set, {self.audited_own}), shared text {self.text_distinct} "
                "at the distinct bound, chrome "
                f"{variant.chrome_art_tiles} declared plus {self.bar_pattern_tiles} bar pattern tiles. "
                "This is the bound a build cannot buy back (§5.1): the upper bound is printed too, and both "
                "are in the report. The per-scene terms are in the walk above, so a scene that cannot be "
                "held is named rather than summed into a total",
            )
        if self.pool_upper > variant.tile_budget and variant.glyph_atlas != "static":
            raise Refusal(
                "image-tiles-over-budget",
                f"[[variant]] {variant.id}: the upper pool sum {self.pool_upper} exceeds tile_budget "
                f"{variant.tile_budget}, and this variant declares glyph_atlas = {variant.glyph_atlas!r}. The "
                "upper bound is exactly what a per-cell renderer allocates — §5.1: 8 rows × 44 cells × 2 tile "
                "rows = 704 slots against 512, before a single art tile, and the six-row fallback does not "
                "rescue it either",
            )

    # -- printed, and generated ---------------------------------------------

    def title_fit(self, text: str) -> tuple[bool, float, str, int]:
        """(fits, width in px, unwritten characters, the box in px), measured.

        D9's discharge says this is the only kind of title box in this build: a
        box is a pixel width the variant declares, and a verdict is a string
        measured against it with the face's own advance widths. When a variant
        declares no title text element — a title band carried as art, as §5.0's A
        does — there is no box to measure against, and the answer says that
        (`box_px` 0, `fits` vacuously true) rather than borrowing a face and
        inventing a limit.
        """
        if self.title_element is None or self.title_element.metrics is None:
            return True, 0.0, "", 0
        fits, width, missing = self.title_element.metrics.fits(text, self.title_element.w)
        return fits, width, missing, self.title_element.w

    def title_render(self, text: str, keep: int = 0) -> tuple[str, bool]:
        """(the string the text module will be handed, whether that is `text` whole).

        §5.5's other half, and the half PR-1 left unpaid: an over-long title is not drawn
        overflowing and is not refused, it is *elided* — cut at the last whole glyph that
        fits plus `ELLIPSIS`, with `keep` leading code points protected (§5.5: "the track
        number is never cut"). The measuring is the same `FaceMetrics` path `title_fit`
        uses, so the box the bake decides on and the box the device measures are one path
        in two languages, and the atlas module's own suite is what pins that they agree.

        Refuses with §9.0's `title-fit` id in the one case elision cannot serve: a box that
        will not hold the protected prefix plus the marker at all. That is a spec describing
        a box too small to render any title, so it is a build failure and not an album's
        problem — which is exactly why it is the case the register's row names, and why an
        over-long title on a sane box stays a note in the log (§5.5: an album decision).
        """
        if self.title_element is None or self.title_element.metrics is None:
            return text, True
        rendered = self.title_element.metrics.elide(text, self.title_element.w, keep)
        if not rendered:
            raise Refusal(
                "title-fit",
                f"element {self.title_element.id} of [[variant]] {self.variant.id} declares a "
                f"{self.title_element.w} px box, and at {self.title_element.face}'s declared raster not "
                f"even the {keep} protected leading code points plus the {len(ELLIPSIS)}-code-point "
                f"elision marker {ELLIPSIS!r} fit it. §5.5's one-line rule has nothing to render at "
                "this width, elided or not — a box narrower than the ellipsis is a spec error, not a "
                "long title (a long title is elided and printed as a note; see the row of §9.0 this "
                "refusal is)",
            )
        return rendered, rendered == text

    def face_lines(self) -> list[str]:
        lines = []
        for face_id, metrics in self.face_metrics.items():
            lines.append(
                f"    face {face_id}: {metrics.face.file} at {metrics.raster} px"
                f" | design size {metrics.design_px} px ({metrics.raster // max(metrics.design_px, 1)}x),"
                f" {metrics.mapped_count} mapped codepoints / {metrics.outlined} outlined / {metrics.printable} printable"
                f" | grid at this raster: {'all on-grid' if metrics.grid_aligned else f'{metrics.off_grid_outlined}/{metrics.outlined} ink off, {metrics.off_grid}/{metrics.mapped_count} counting advances'}"
                f" | line box {metrics.line_box_px} px (hhea {metrics.ascender}/{metrics.descender} upem {metrics.upem})"
                f" -> smallest legal pitch {metrics.min_row_px} px = {metrics.tile_rows_per_line} tile rows"
                f" | tallest ink {metrics.ink_px:.1f} px"
                f" | advance max {metrics.max_advance_px:g} px, mean {metrics.avg_advance_px:.4f} px printable"
                f" -> {metrics.cells_in_240px} chars in a 240 px line"
                " (§5.1's table: every one of those numbers is read from the file, not copied from the document)"
            )
        return lines

    def lines(self, gate: bool) -> list[str]:
        variant = self.variant
        out = [
            f"layout {variant.id}: product={str(variant.product).lower()}"
            + (", fit and pool arithmetic gated" if gate else ", not product-eligible: printed in full, and a product gate reports it skipped with this reason"),
        ]
        out += self.face_lines()
        out.append(
            f"  bands (§5.0, bottom-up): screen {self.spec.screen_tiles[0]}x{self.spec.screen_tiles[1]} tiles"
            f" = {self.spec.screen_tiles[0] * self.tile_px}"
            f"x{self.spec.screen_tiles[1] * self.tile_px} px, tile {self.tile_px} px,"
            f" map {self.spec.map_tiles[0]}x{self.spec.map_tiles[1]} ({self.spec.map_tiles[0] - self.spec.screen_tiles[0]} columns and "
            f"{self.spec.map_tiles[1] - self.spec.screen_tiles[1]} rows off-screen), regular pool {self.spec.pool_slots} slots"
        )
        for band in variant.bands:
            role = "region (swappable)" if band is self.region_band else "always shown"
            out.append(
                f"    band {band.id}: {band.rows} tile rows at screen row {band.screen_row} (y={band.screen_row * self.tile_px} px) [{role}]"
            )
        out.append(
            f"    region: {self.region[0]}x{self.region[1]} tiles = {self.region[2]}x{self.region[3]} px"
            f" = {self.region[1]} tile rows; one scene at a time, and a scene is given this rectangle and nothing else"
        )
        for element in self.elements:
            out.append(
                f"  element {element.id} [{element.kind}] {element.w}x{element.h} px"
                f" at screen ({element.x},{element.y}) = band {element.band.id} local"
                f" ({element.xt},{element.yt})..({element.xt + element.wt - 1},{element.yt + element.ht - 1})"
                + (f" scene {element.scene}" if element.scene else "")
                + (f" banks {element.banks}" if element.banks else "")
            )
            if element.kind == "text" and element.metrics is not None:
                out.append(
                    f"    {element.text_rows} text rows = {element.h} px / {element.row_px} px pitch (derived: a row count is a consequence),"
                    f" {element.cells_per_row} cells/row at the mean advance {element.metrics.avg_advance_px:.2f} px;"
                    f" a fit verdict measures the folded string against {element.w} px, never a character count (§5.5)"
                )
        if self.art_boxes:
            ceiling = self.art_ceiling
            shown = [box for box in self.art_boxes if box.side % max(1, math.ceil(len(self.art_boxes) / 8)) == 0]
            out.append(
                "  derived art boxes (§7's legal list): "
                + ", ".join(f"{box.width_px}x{box.height_px}" for box in shown)
                + f" … ({len(self.art_boxes)} boxes from 1x1 to {ceiling.width_px}x{ceiling.height_px};"
                f" the ceiling is {ceiling.tiles} tiles of a declared image_tiles {variant.image_tiles})"
            )
            out.append(
                f"    centring at the ceiling: +{ceiling.x_off} px x, +{ceiling.y_off} px y inside the element,"
                f" off-centre by {ceiling.off_centre_x} px x and {ceiling.off_centre_y} px y"
                " — these are the real pixel offsets §7 asks a fit report to print"
            )
        if self.bar_element is not None:
            out.append(
                f"  bar: {variant.bar_cells} cells across {self.bar_px} px = {self.bar_cell_px} px per cell;"
                f" ±1 cell is {100.0 / variant.bar_cells:.2f}% of the track (§6 prints that percentage beside the geometry that produced it);"
                f" {self.bar_pattern_tiles} partial-fill pattern tiles derived, not estimated"
            )
        if self.title_element is not None:
            element = self.title_element
            out.append(
                f"  fit box (derived, §5.5): element {element.id} is {element.w} px wide; at {element.face}'s maximum advance"
                f" {element.metrics.max_advance_px:g} px a conservative span is {element.max_chars} characters,"
                " and the fit leg itself measures pixels. No character-count box is a constant in this build (D9)"
            )
        else:
            out.append(
                "  fit box: this variant declares no text line in a title band, so §5.5's fit box does not exist for it"
                " — said rather than assumed (§5.0's A carries a text-free title band)"
            )
        out.append(
            f"  pool audit at the variant's own ceiling, both bounds (§5.1, R14): scenes resident "
            f"{self.resident_own} + projection {self.projection_own} (the audited set, {self.audited_own})"
            f" + shared text distinct {self.text_distinct} / upper {self.text_upper}"
            f" + chrome {variant.chrome_art_tiles} declared ({self.chrome_derived} derived:"
            f" {len(variant.bands)} band backdrops + 1 region backing + {len(self.control_slots)} control tiles)"
            f" + bar pattern {self.bar_pattern_tiles}  =>  distinct {self.pool_distinct}, upper {self.pool_upper}"
            f"  against tile_budget {variant.tile_budget} of a {self.spec.pool_slots}-slot regular pool"
        )
        out.append(
            "    the scene addend is walked, not assumed (§11's 2b PR-(1)): a scene's own tiles are its "
            "declared kind — `art` is the image at the derived ceiling, `composite` the art elements it "
            "declares, `fill` and `text` own nothing because the region backing tile and the glyph atlas "
            "are the variant's shared terms. The audit is resident + projection on purpose: the budget "
            "belongs to the variant, so a scene landing moves its term between those two columns and "
            "changes no total — which is what lets 2b's scenes land one PR at a time"
        )
        if self.pool_upper > variant.tile_budget and variant.glyph_atlas == "static":
            out.append(
                f"    FLAG, not a refusal: the upper bound {self.pool_upper} is over the budget and this variant declares"
                " the static glyph atlas, which is the build that makes the distinct bound real (§5.1: that row"
                " flags the atlas variant rather than refusing it). The margin is not what it looks like:"
                f" {variant.tile_budget - self.pool_distinct} slots remain, and a second scene's tiles and the chrome's"
                " art come out of those"
            )
        for term in self.scene_terms:
            mine = [element.id for element in self.elements if element.scene == term.name]
            out.append(
                f"  scene {term.name} (phase {term.scene.phase}, band {term.scene.band}, kind {term.kind}, "
                f"resident={str(term.resident).lower()}): own tiles {term.own_tiles}"
                f" ({' + '.join(f'{name} {tiles}' for name, tiles in term.terms.items()) or 'none: shared'}),"
                f" audited as {term.audited_as}"
                + (", shares the variant's glyph atlas" if term.shares_atlas else "")
                + f": {' '.join(mine) or 'no element declared at this phase'}"
            )
        phases = sorted({scene.phase for scene in variant.scenes})
        resident_names = ", ".join(term.name for term in self.scene_terms if term.resident) or "none"
        projection_names = ", ".join(term.name for term in self.scene_terms if not term.resident) or "none"
        out.append(
            f"  residency: {variant.resident_scenes} scene(s) declared resident ({resident_names}) by their "
            f"`resident` flags, hardware cap {self.spec.resident_scene_max} (one of four layers belongs to the "
            f"chrome); the declared scene set also projects {projection_names}, and the pool audit prices both "
            "(resident + projection). The scenes this spec registers span phases "
            f"{', '.join(phases)} — {len(variant.scenes)} scenes, and §5.1's four residency options differ by "
            "two orders of magnitude so the *option* is what a report names"
        )
        for note in self.notes:
            out.append(f"  note: {note}")
        return out


def derive_variant(spec: LayoutSpec, variant: VariantSpec) -> VariantFacts:
    return VariantFacts(spec, variant)


def select_variant(config_variant: str, spec: LayoutSpec) -> VariantSpec:
    """§5.0: the config names a variant; it never produces geometry."""
    return spec.variant(config_variant)


def product_variants(spec: LayoutSpec) -> list[VariantSpec]:
    """§5.0's `product` field: the variants that may be shipped at all."""
    return [variant for variant in spec.variants if variant.product]


def layout_for(config_variant: str,
               spec_path: Path = DEFAULT_LAYOUT_SPEC) -> tuple[LayoutSpec, VariantSpec, VariantFacts]:
    """The spec, the variant this album selects, and everything it derives.

    This is the bake's first act, before a single sample is packed. §6's
    reciprocal is computed against the selected variant's declared cell count, so
    a layout resolved *after* the packing is a manifest whose numbers describe a
    different grid than the ROM draws — and §6's whole point is that the bar and
    the clock never disagree.
    """
    spec = LayoutSpec(spec_path)
    if config_variant:
        variant = spec.variant(config_variant)
    else:
        # `[layout] variant` absent is not "no layout": which variant ships is a
        # field of the spec (§5.0), and if exactly one variant declares itself
        # product-eligible then there is nothing for the config to add. Two or
        # zero is the spec being ambiguous about what a ROM ships, which is a
        # spec error, not a default.
        eligible = product_variants(spec)
        if len(eligible) != 1:
            raise Refusal(
                "spec",
                f"{spec_path}: [layout] variant is absent and this spec declares {len(eligible)} "
                "product-eligible variant(s); §5.0's `product` field says which variants may ship, so "
                "either the spec names exactly one or the config names one",
            )
        variant = eligible[0]
    return spec, variant, derive_variant(spec, variant)


def read_layout_module(path: Path) -> dict[str, object]:
    """A generated `layout.rs` on disk, read back as facts rather than trusted."""
    return read_layout_module_text(path.read_text(), str(path))


def read_layout_module_text(text: str, where: str) -> dict[str, object]:
    """A generated `layout.rs`, read back as facts rather than trusted.

    The same reason `read_pin_module` exists: the tool checks the file it wrote.
    Only the two numbers that reach the manifest and the ROM's own refusal are
    read — the bar grid and the variant id — because those are the two a stale
    or half-written module could get wrong without anything else noticing.
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
    missing = {"BAR_CELLS", "LAYOUT_ID"} - set(facts)
    if missing:
        raise Refusal(
            "audit",
            f"{where}: the generated layout module declares no {sorted(missing)}, so the agreement "
            "check has nothing to compare (§9.0's `bar-cells-disagree` row)",
        )
    return facts


def read_advance_table(text: str, face_id: str, where: str) -> dict[int, int]:
    """A generated `ADVANCES_<FACE>` table, read back as `{codepoint: font units}`.

The generated module carries the face's advance widths so the *Rust* side can measure a
    string instead of counting characters (D9 item 2, §5.5). That table is written from this
    tool's own `hmtx` read, so a leg that compared it with itself would prove nothing: the
    point of reading it back here is that the bytes a Rom links are the bytes this tool
    measured, and a transcription slip between the two is exactly the sort of thing a
    review cannot see in a 700-line generated file.
    """
    ident = "ADVANCES_" + "".join(c if c.isalnum() else "_" for c in face_id).upper()
    marker = f"pub const {ident}:"
    start = text.find(marker)
    if start < 0:
        raise Refusal(
            "audit",
            f"{where}: the generated module declares no {ident}, so a fit verdict on the device "
            "would have to count characters (§5.5, D9 item 2)",
        )
    body_start = text.find("= [", start)
    body_end = text.find("];", body_start)
    if body_start < 0 or body_end < 0:
        raise Refusal("audit", f"{where}: {ident} has no `= [ … ];` body to read")
    table: dict[int, int] = {}
    for point, units in re.findall(r"\(\s*(0x[0-9a-fA-F]+|\d+)\s*,\s*(\d+)\s*\)", text[body_start:body_end]):
        table[int(point, 0)] = int(units)
    if not table:
        raise Refusal("audit", f"{where}: {ident} parsed to no entries")
    return table


def measure_from_table(table: dict[int, int], text: str, raster: int, upem: int,
                       box_px: int) -> tuple[bool, float, str]:
    """The same verdict as `FontMetrics.fits`, computed from a read-back table.

The arithmetic is the *Rust* side's, restated: units summed over mapped characters,
    `units*raster <= box*upem` for the verdict, and unmapped characters named rather than
    silently zero-width. Deliberately not `FontMetrics.fits` with another argument — that
    would be one path twice.
    """
    units = 0
    missing = ""
    for char in text:
        width = table.get(ord(char))
        if width is None:
            missing += char
        else:
            units += width
    return units * raster <= box_px * upem, units * raster / upem, missing


def assert_bar_grid_agrees(declared: int, pack: int, layout: int, context: str) -> None:
    """§9.0's `bar-cells-disagree`, on the bake side.

    Three artifacts have to carry one number: the spec's declared cell count, the
    grid `ALBM1`'s reciprocals were computed for, and the value in the generated
    `layout.rs` the ROM compiles. This is the check that aims at the spec;
    `meta.rs`'s boot-time refusal is kept as well, because it is the only thing
    that catches a stale generated module linked against yesterday's manifest (R11's
    staleness failure, invisible to every host test — §6 rules that both stay).
    """
    if not (declared == pack == layout):
        raise Refusal(
            "bar-cells-disagree",
            f"bar_cells: {context} declares {declared}, the ALBM1 header carries {pack}, the generated "
            f"layout module says {layout}. §6: a ROM would draw a bar out of reciprocals computed for a "
            "different grid — the three must agree, and this is the cheapest guard on the one place where a "
            "layout number reaches a decoder's arithmetic",
        )


def bar_albm1_value(blob: bytes) -> int:
    """The grid the pack's own reciprocals were written against (header word 12)."""
    return _word(blob, 12)


def numbers_block(facts: VariantFacts) -> list[str]:
    """The arithmetic as a sorted `key=value` block — §13's machine-readable form.

    This is what a snapshot holds: numbers, not prose. A reviewer diffing it sees
    exactly which value moved, and the file does not churn because a sentence in
    the report changed. §13's AP-3 rule that a report's "machine-readable form is
    the point" is why this exists alongside the prose report rather than instead
    of it.
    """
    variant = facts.variant
    ceiling = facts.art_ceiling
    rows: list[str] = [
        f"variant={facts.id}",
        f"product={str(facts.product).lower()}",
        f"tile_px={facts.tile_px}",
        f"screen_tiles={facts.spec.screen_tiles[0]}x{facts.spec.screen_tiles[1]}",
        f"map_tiles={facts.spec.map_tiles[0]}x{facts.spec.map_tiles[1]}",
        f"pool_slots={facts.spec.pool_slots}",
        f"bands_bottom_up=" + ",".join(
            f"{band.id}:{band.screen_row}:{band.rows}:{'always' if band.always else 'swappable'}"
            for band in facts.variant.bands
        ),
        f"region_band={facts.region_band.id}",
        f"region_tiles={facts.region[0]}x{facts.region[1]}",
        f"region_px={facts.region[2]}x{facts.region[3]}",
        f"atlas_mode={variant.glyph_atlas}",
        f"image_tiles_declared={variant.image_tiles}",
        f"art_element={facts.art_element.id if facts.art_element else 'none'}",
        f"art_max_side_tiles={max((box.side for box in facts.art_boxes), default=0)}",
        f"art_boxes_derived={len(facts.art_boxes)}",
        f"art_ceiling_tiles={facts.pool_image}",
        f"art_ceiling_px={ceiling.width_px if ceiling else 0}",
        f"art_ceiling_off_centre_px={ceiling.off_centre_x if ceiling else 0},{ceiling.off_centre_y if ceiling else 0}",
        f"bar_element={facts.bar_element.id if facts.bar_element else 'none'}",
        f"bar_cells={variant.bar_cells}",
        f"bar_px={facts.bar_px}",
        f"bar_cell_px={facts.bar_cell_px}",
        f"bar_pattern_tiles={facts.bar_pattern_tiles}",
        f"bar_precision_permyriad={round(1_000_000 / variant.bar_cells) if variant.bar_cells else 0}",
        f"pool_image={facts.pool_image}",
        f"pool_scene_resident={facts.resident_own}",
        f"pool_scene_projection={facts.projection_own}",
        f"pool_scene_audited={facts.audited_own}",
        f"pool_text_distinct={facts.text_distinct}",
        f"pool_text_upper={facts.text_upper}",
        f"pool_chrome_declared={variant.chrome_art_tiles}",
        f"pool_chrome_derived={facts.chrome_derived}",
        f"pool_distinct={facts.pool_distinct}",
        f"pool_upper={facts.pool_upper}",
        f"tile_budget={variant.tile_budget}",
        f"pool_upper_over_budget={'true' if facts.pool_upper > variant.tile_budget else 'false'}",
        f"resident_scenes={variant.resident_scenes}",
        f"resident_scene_max={facts.spec.resident_scene_max}",
        f"scenes_declared={len(variant.scenes)}",
        f"scene_phases={','.join(sorted({scene.phase for scene in variant.scenes}))}",
        f"control_slots={len(facts.control_slots)}",
        f"title_fit_box={'absent' if facts.title_element is None else str(facts.title_element.w)}",
    ]
    if facts.title_element is not None:
        rows.append(f"title_fit_element={facts.title_element.id}")
        rows.append(f"title_fit_max_chars={facts.title_element.max_chars}")
    for face_id, metrics in facts.face_metrics.items():
        rows += [
            f"face_{face_id}_file={metrics.face.file}",
            f"face_{face_id}_raster={metrics.raster}",
            f"face_{face_id}_upem={metrics.upem}",
            f"face_{face_id}_design_px={metrics.design_px}",
            f"face_{face_id}_mapped_codepoints={metrics.mapped_count}",
            f"face_{face_id}_outlined_glyphs={metrics.outlined}",
            f"face_{face_id}_printable={metrics.printable}",
            f"face_{face_id}_off_grid_at_declared_raster={metrics.off_grid}",
            f"face_{face_id}_off_grid_ink_at_declared_raster={metrics.off_grid_outlined}",
            f"face_{face_id}_line_box_px={metrics.line_box_px}",
            f"face_{face_id}_min_row_px={metrics.min_row_px}",
            f"face_{face_id}_tile_rows_per_line={metrics.tile_rows_per_line}",
            f"face_{face_id}_atlas_tiles={metrics.outlined * metrics.tile_rows_per_line}",
            f"face_{face_id}_tallest_ink_permyriad={round(metrics.ink_px * 10_000)}",
            f"face_{face_id}_max_advance_permyriad={round(metrics.max_advance_px * 10_000)}",
            f"face_{face_id}_mean_advance_permyriad={round(metrics.avg_advance_px * 10_000)}",
            f"face_{face_id}_chars_in_240px={metrics.cells_in_240px}",
        ]
    for element in facts.elements:
        rows += [
            f"element_{element.id}_kind={element.kind}",
            f"element_{element.id}_band={element.band.id}",
            f"element_{element.id}_scene={element.scene or 'none'}",
            f"element_{element.id}_tiles={element.wt}x{element.ht}@{element.xt},{element.yt}",
            f"element_{element.id}_px={element.w}x{element.h}@{element.x},{element.y}",
        ]
        if element.kind == "text":
            rows += [
                f"element_{element.id}_face={element.face}",
                f"element_{element.id}_row_px={element.row_px}",
                f"element_{element.id}_text_rows={element.text_rows}",
                f"element_{element.id}_cells_per_row={element.cells_per_row}",
                f"element_{element.id}_text_tile_upper={element.text_rows * element.cells_per_row * element.metrics.tile_rows_per_line}",
                f"element_{element.id}_fit_px={element.w}",
                f"element_{element.id}_max_chars_at_max_advance={element.max_chars}",
            ]
    for scene in variant.scenes:
        rows.append(f"scene_{scene.name}_phase={scene.phase}")
        rows.append(f"scene_{scene.name}_band={scene.band}")
        rows.append(f"scene_{scene.name}_kind={scene.kind}")
        rows.append(f"scene_{scene.name}_resident={str(scene.resident).lower()}")
        if scene.min_visible_rows is not None:
            rows.append(f"scene_{scene.name}_min_visible_rows={scene.min_visible_rows}")
        if scene.min_boxes is not None:
            rows.append(f"scene_{scene.name}_min_boxes={scene.min_boxes}")
        if scene.shares_image:
            rows.append(f"scene_{scene.name}_shares_image={scene.shares_image}")
    for term in facts.scene_terms:
        rows.append(f"scene_{term.name}_own_tiles={term.own_tiles}")
        rows.append(f"scene_{term.name}_audited_as={term.audited_as}")
        rows.append(f"scene_{term.name}_shares_glyph_atlas={str(term.shares_atlas).lower()}")
        for name, tiles in term.terms.items():
            rows.append(f"scene_{term.name}_term_{name}={tiles}")
    for control in facts.control_slots:
        rows.append(f"control_{control.key}_slot={control.slot}@{control.x0},{control.y0}")
    return sorted(rows)


def measurement_block() -> list[str]:
    """2b PR-(2): the render probe's raw folds, in the report rather than in prose.

    A coefficient without its measurement is a number somebody chose; the point of
    AP-3b was that the plan's UI figures stopped being arithmetic on AP-0's upload
    leg. Putting the folds here (not only in the probe's README) is what lets a
    reader of *the* machine-readable artifact see which terms are measured, on what
    instrument, and with what spread — and lets a later hardware run disagree with
    them visibly.
    """
    return [
        "schema=album-layout-measurement/1",
        "render_probe_bin=examples/album_probe/src/bin/render_probe.rs",
        f"render_probe_at={MEASURED_RENDER_AT}",
        f"render_l1_bar_cell_live_cycles={MEASURED_BAR_CELL_LIVE}",
        "render_l1_bar_cell_live_spread=min 480 / max 741 over 60 folds",
        f"render_l1_clock_digit_live_cycles={MEASURED_CLOCK_DIGIT_LIVE}",
        "render_l1_clock_digit_live_cells=2 (two tile rows at the 16 px pitch)",
        f"render_l1_unchanged_cell_live_cycles={MEASURED_UNCHANGED_CELL_LIVE}",
        "render_l1_unchanged_live_note=83% of a changed cell: the early-out saves the map write, "
        "not the add/remove reference pair — 'near-free' was the plan's expectation, not the answer",
        f"render_l2_region_600_live_cycles={MEASURED_REGION_600_LIVE}",
        "render_l2_region_600_live_spread=min 536,080 / max 556,701 over 8 windows",
        f"render_l2_per_cell_live_cycles={MEASURED_REGION_PER_CELL_LIVE}",
        f"render_l3_set_priority_live_cycles={MEASURED_SET_PRIORITY_WRITE_LIVE}",
        f"render_l3_switch_live_cycles={MEASURED_SCENE_SWITCH_LIVE}",
        "render_l3_switch_live_spread=set_priority 73 c + show/commit mean 212,102 c, max 280,360 c",
        f"render_l4_refs={MEASURED_POOL_REFS}",
        f"render_l4_distinct={MEASURED_POOL_DISTINCT}",
        f"render_l4_shared={MEASURED_POOL_SHARED}",
        f"render_l4_max_refs={MEASURED_POOL_MAX_REFS}",
        f"render_l4_pool_slots={MEASURED_POOL_SLOTS}",
        "render_l4_note=refs are cell references read from the maps, distinct are live VRAM slots, "
        "and max_refs > 1 is what a shared atlas looks like; a per-cell fallback shows distinct == refs",
    ]


def cycles_block(facts: VariantFacts) -> list[str]:
    """The report's cost estimate, with each term's measurement named.

    2b PR-(2) flipped the terms AP-3b's render bin measured from `arithmetic` to
    `measured`: a changed bar cell, a changed clock digit, and the scene switch. The
    title line and the declared region stay `arithmetic` on a *measured* per-cell
    coefficient, because the bin measured a 600-cell window rather than those
    elements — arithmetic on a measured coefficient is still arithmetic, and the tag
    says which one a term is.
    """
    rows = [
        f"cycles_coefficient_per_tile={CYCLE_PER_TILE_LIVE}",
        f"cycles_coefficient_source={CYCLE_COEFFICIENT_SOURCE}",
        f"cycles_measured_at={MEASURED_RENDER_AT}",
    ]

    def measured(what: str, cycles: int, cells: int, source: str) -> None:
        rows.append(f"cycles_{what}_cycles={cycles}")
        rows.append(f"cycles_{what}_cells={cells}")
        rows.append(f"cycles_{what}_tag=measured")
        rows.append(f"cycles_{what}_source={source}")

    def term(what: str, tiles: int, source: str) -> None:
        rows.append(f"cycles_{what}_tiles={tiles}")
        rows.append(f"cycles_{what}_cycles={tiles * CYCLE_PER_TILE_LIVE}")
        rows.append(f"cycles_{what}_tag=arithmetic")
        rows.append(f"cycles_{what}_source={source}")

    # One changed bar cell: L1 measures one cell, and 1-2 is the declared worst case
    # for a cell that straddles a tile boundary (§10.2's bar row).
    measured("bar_cell", MEASURED_BAR_CELL_LIVE if facts.bar_element is not None else 0,
             1 if facts.bar_element is not None else 0,
             "render probe L1, one changed 8x8 cell, live audio; §10.2's bar row, whose "
             f"declared worst case is 2 tiles for a straddling cell (element "
             f"{facts.bar_element.id if facts.bar_element else 'none'})")
    # One changed clock digit: L1 measures the two tile rows the 16 px line box spans.
    clock = next((element for element in facts.elements if element.kind == "text" and element.id == "clock"), None)
    measured("clock_digit", MEASURED_CLOCK_DIGIT_LIVE if clock is not None else 0,
             2 if clock is not None else 0,
             "render probe L1, the digit's two tile rows, live audio; §10.2's clock row")
    # A title line on a track change: the element's whole box, because the fill cost is
    # the element's cells and not the string's length (§10.2's title row, re-cut at the
    # product pitch). Not measured by L1/L2 at this size, so arithmetic on L2's
    # per-cell coefficient rather than a measured term pretending to be one.
    title_tiles = facts.title_element.wt * facts.title_element.ht if facts.title_element is not None else 0
    term("title_line", title_tiles,
         "§10.2's title row: the declared element's cell count, not a character count, "
         "multiplied by L2's measured per-cell coefficient")
    # A whole scene-region change: the region's visible cells (§10.2's partial-map row,
    # whose 600-cell figure is its rounded version; this is the declared rectangle).
    term("region_repaint", facts.region[0] * facts.region[1],
         "§10.2's partial-map row: the declared region's visible cells multiplied by L2's "
         "measured per-cell coefficient; L2's 600-cell window total is in [measurement render]")
    # The switch: measured, and named as a switch rather than as the priority write.
    measured("scene_switch", MEASURED_SCENE_SWITCH_LIVE, 2,
             "render probe L3, `set_priority` + show + commit with both scenes resident, "
             f"live audio; the priority store itself is {MEASURED_SET_PRIORITY_WRITE_LIVE} c, "
             "so the commit is the price — an event cost paid where the audio is re-primed")
    return rows


def rom_budget(facts: VariantFacts, *, albm1_bytes: int | None, audio_bytes: int | None,
               cart_bytes: int | None) -> dict[str, int | str | None]:
    """§10.1's cart addends, and the two budget verdicts re-homed by 2b PR-(1).

    `over-budget` is `code + ALBM1 + art`, where ALBM1 already carries the packed audio
    region — the three addends the register row names. Art is priced at the variant's
    derived ceiling in 4 bpp tiles (32 B each) until `album_art.py` exists in 2b PR-(4);
    `viz` is Phase 3's `[viz]` and is not parsed, so it is named as absent rather than
    counted as zero. A standalone report (`--emit-report`) has no album, so ALBM1 and the
    audio term are `None` and the total is **not computed** — the honest answer, and the
    reason the bake is what refuses.
    """
    art_bytes = facts.pool_image * ART_TILE_BYTES
    total = None if albm1_bytes is None else CODE_RESERVE_BYTES + albm1_bytes + art_bytes
    if total is not None and cart_bytes is not None and total > cart_bytes:
        raise Refusal(
            "over-budget",
            f"[[variant]] {facts.id}: code reserve {CODE_RESERVE_BYTES} B + ALBM1 {albm1_bytes} B "
            f"(audio assets {audio_bytes} B are inside it) + art {art_bytes} B = {total} B, over the "
            f"declared cart {cart_bytes} B. §10.1 reserves the code before the encoder sizes the audio, "
            "so a code growth can never silently eat samples; this is the verdict that says the album "
            "does not fit (2b PR-(1) re-homed it from Phase 3, §9.0's register row)",
        )
    return {
        "code_reserve_bytes": CODE_RESERVE_BYTES,
        "albm1_bytes": albm1_bytes,
        "audio_assets_bytes": audio_bytes,
        "art_bytes": art_bytes,
        "art_tile_bytes": ART_TILE_BYTES,
        "viz_bytes": None,
        "total_bytes": total,
        "cart_bytes": cart_bytes,
        "verdict": "not computed" if total is None else ("fits" if cart_bytes is None or total <= cart_bytes else "over"),
    }


def budget_block(budget: dict[str, int | str | None] | None) -> list[str]:
    if budget is None:
        return [
            "rom_code_reserve_bytes=not computed",
            "rom_albm1_bytes=not computed",
            "rom_audio_assets_bytes=not computed",
            "rom_art_bytes=not computed",
            "rom_viz_bytes=not computed",
            "rom_total_bytes=not computed",
            "rom_cart_bytes=not computed",
            "rom_verdict=not computed (no album in this invocation; the bake is what refuses over-budget)",
        ]
    return [f"rom_{key}={budget[key]}" for key in (
        "code_reserve_bytes", "albm1_bytes", "audio_assets_bytes", "art_bytes", "art_tile_bytes",
        "viz_bytes", "total_bytes", "cart_bytes", "verdict",
    )]


def report_text(spec: LayoutSpec, facts_list: list[VariantFacts],
                budget: dict[str, int | str | None] | None = None,
                budget_variant: str = "",
                skipped: list[tuple[str, str]] | None = None) -> str:
    """One derivation, two renderings — the prose (`VariantFacts.lines`) and this block.

    2b PR-(1): the artifact this phase has to leave behind is the spec -> `layout.rs` ->
    report pipeline. The report is deliberately the sorted `key=value` form §13 calls the
    machine-readable one, so a reviewer diffs numbers rather than sentences and the file
    does not churn when prose changes. Two runs of the same spec must produce identical
    bytes (`make album-layout-test` holds that), so nothing here can carry a timestamp or
    an absolute path.
    """
    lines = [
        f"schema={REPORT_SCHEMA}",
        f"spec={_relative(spec.path)}",
        "generated_by=album-layout-gen (scripts/album_bake.py)",
        f"hardware_screen_tiles={spec.screen_tiles[0]}x{spec.screen_tiles[1]}",
        f"hardware_map_tiles={spec.map_tiles[0]}x{spec.map_tiles[1]}",
        f"hardware_tile_px={spec.tile_px}",
        f"hardware_pool_slots={spec.pool_slots}",
        f"hardware_resident_scene_max={spec.resident_scene_max}",
        f"layout_code_reserve_bytes={CODE_RESERVE_BYTES}",
        "layout_source=ALBUM-PLAYER.md §10.1 (the code reserve is 256 KiB); §5.1/R14 for the pool terms",
    ]
    lines.append("")
    lines.append("[measurement render]")
    lines.extend(measurement_block())
    for facts in facts_list:
        lines.append("")
        lines.append(f"[variant {facts.id}]")
        lines.extend(numbers_block(facts))
        lines.extend(cycles_block(facts))
        lines.extend(budget_block(budget if facts.id == budget_variant else None))
    # A variant the generator refused is *named in the report*, not silently absent
    # (§13's AP-3 rule: a green says what it did not look at). No numbers are emitted
    # for it, because there are none to emit.
    for variant_id, refusal in skipped or []:
        lines.append("")
        lines.append(f"[variant {variant_id}]")
        lines.append("derivable=false")
        lines.append(f"refusal={refusal}")
    return "\n".join(lines) + "\n"


def _rs(value: object) -> str:
    return str(value)


def layout_rs_text(facts: VariantFacts) -> str:
    """The generated module: a band table with a slot in it, and the derived facts.

    §5.0's conclusion, in Rust: *the thing generated at compile time is a band
    table with a slot in it for a scene*. The const asserts re-check in the
    compiler what the generator checked in Python, because §6's reasoning about
    why both the boot refusal and the bake check stay applies most sharply here —
    a stale generated module linked against yesterday's manifest is invisible to
    every host test, and this module is where a generated number reaches the ROM
    (R11's staleness rule).
    """
    variant, spec = facts.variant, facts.spec
    spec_rel = spec.path.relative_to(REPO) if spec.path.is_relative_to(REPO) else spec.path
    ceiling = facts.art_ceiling
    out: list[str] = []
    add = out.append
    add("//! Generated by `scripts/album_bake.py`: by `make album-bake` for the variant `album.toml`")
    add("//! selects, or by `--emit-layout` for a named one (the variant-B snapshot leg of")
    add("//! `make album-layout-test` uses that, so variant A's arithmetic is pinned without inventing an")
    add("//! A-shaped album). Not tracked, not hand-edited (§5.0, and §2.3's rule about generated items).")
    add(f"//! Source: {spec_rel} [[variant]] {variant.id}, plus the tracked .ttf files that section's")
    add("//! [[face]] rows name. Nothing in this file is transcribed from ALBUM-PLAYER.md: §5.0's band")
    add("//! table, §5.1's line metrics and pool arithmetic, §5.5's fit boxes, §6's bar grid and §7's legal")
    add("//! box list are all derived here from a tracked spec and the fonts' own tables.")
    add("//!")
    add("//! Why every count in it is derived: §11's D9 records what happens when one is not. The text-row")
    add("//! counts below are `element height / declared pitch`, the art ceiling is `min(region, region,")
    add("//! image_tiles)` and not the 240x128 §5.0 originally wrote down, and no fit box anywhere is a")
    add("//! character count. Where a number from the plan is quoted at all, it is labelled as a floor next")
    add("//! to the derived value it is not (`BAR_CELLS_FLOOR` beside `BAR_CELLS`), because the same")
    add("//! conflation is what D9 is about.")
    add("//!")
    add("//! The const asserts re-check, in the compiler, what the generator checked before writing this.")
    add("//! That is deliberate: §6 keeps the boot-time check *and* the bake check because a stale generated")
    add("//! module is invisible to host tests that read only the manifest, and §11's R11 makes the same")
    add("//! point about a green that cannot see a stale generated file.")
    add("")
    add("/// Tile geometry, from `[meta]`: every other number here is in these units (§5.0).")
    add(f"pub const TILE_PX: u32 = {_rs(facts.tile_px)};")
    add(f"pub const SCREEN_COLS: u32 = {_rs(spec.screen_tiles[0])};")
    add(f"pub const SCREEN_ROWS: u32 = {_rs(spec.screen_tiles[1])};")
    add(f"pub const MAP_COLS: u32 = {_rs(spec.map_tiles[0])};")
    add(f"pub const MAP_ROWS: u32 = {_rs(spec.map_tiles[1])};")
    add("/// The regular 4bpp pool. The affine pool is a *different* 510 with a fallback path between the")
    add("/// two, so a sum that would fit 1,024 is not evidence about this one (§5.1).")
    add(f"pub const POOL_SLOTS: u32 = {_rs(spec.pool_slots)};")
    add("/// Mode 0 has four layers and one belongs to the chrome permanently, so this is a hardware fact")
    add("/// rather than a preference (§5.0).")
    add(f"pub const RESIDENT_SCENE_MAX: u32 = {_rs(spec.resident_scene_max)};")
    add("")
    add("/// Which variant this module is, and whether it is the product's (§5.0's `product` field).")
    add("/// Stated for the boot log's attribution, and nothing more: `ALBM1`'s header carries no")
    add("/// variant field, so `meta::parse` never reads these two. What it does compare is the grid")
    add("/// below — `BAR_CELLS` against the manifest's `bar_cells` — and that single comparison is")
    add("/// the whole of the stale-module backstop (§6, §9.0's `bar-cells-disagree` row).")
    add(f'pub const LAYOUT_ID: &str = "{variant.id}";')
    add(f"pub const LAYOUT_PRODUCT: bool = {str(variant.product).lower()};")
    add(f"pub const TILE_BUDGET: u32 = {_rs(variant.tile_budget)};")
    add(f"pub const IMAGE_TILES_MAX: u32 = {_rs(variant.image_tiles)};")
    add(f'pub const GLYPH_ATLAS: &str = "{variant.glyph_atlas}";')
    add(f"pub const RESIDENT_SCENES: u32 = {_rs(variant.resident_scenes)};")
    add("")
    add("/// The declared scene registry (§5.2), in the spec's order — which is also the order")
    add("/// a `Select` cycle walks, so the order is a field and not a comment in a match arm.")
    add("/// `phase` is the phase that builds it: a declared scene with no implementation yet is")
    add("/// still listed, and the app's own table is a *filtered* view of this one (2b PR-(3):")
    add("/// a host leg proves every implemented scene exists here and keeps this relative order).")
    add("pub struct SceneDecl {")
    add("    pub id: &'static str,")
    add("    pub kind: &'static str,")
    add("    pub phase: &'static str,")
    add("    pub resident: bool,")
    add("    pub shares_image: &'static str,")
    add("}")
    add(f"pub const SCENES: [SceneDecl; {len(variant.scenes)}] = [")
    for scene in variant.scenes:
        add("    SceneDecl {")
        add(f'        id: "{scene.name}",')
        add(f'        kind: "{scene.kind}",')
        add(f'        phase: "{scene.phase}",')
        add(f"        resident: {str(scene.resident).lower()},")
        add(f'        shares_image: "{scene.shares_image}",')
        add("    },")
    add("];")
    add("")
    add("/// The progress bar's geometry (§6). `BAR_CELLS_FLOOR` is the plan's floor, quoted beside the")
    add("/// variant's derived grid so the two cannot be conflated; D9's point is exactly that conflation.")
    add("/// The declared grid comes from the box of `BAR_ELEMENT` divided into cells of whole pixels, and")
    add("/// `BAR_PATTERN_TILES` counts §6's partial-fill pattern tiles from the reachable fill residues")
    add("/// rather than estimating them.")
    add(f"pub const BAR_CELLS_FLOOR: u32 = {_rs(BAR_CELLS_FLOOR)};")
    add(f"pub const BAR_CELLS: u32 = {_rs(variant.bar_cells)};")
    add(f"pub const BAR_PX: u32 = {_rs(facts.bar_px)};")
    add(f"pub const BAR_CELL_PX: u32 = {_rs(facts.bar_cell_px)};")
    add(f"pub const BAR_PATTERN_TILES: u32 = {_rs(facts.bar_pattern_tiles)};")
    add(f'pub const BAR_ELEMENT: &str = "{facts.bar_element.id if facts.bar_element else ""}";')
    add("")
    add("/// The variant's shared pool terms, at its derived **ceiling** (§5.1, R14). The image term")
    add("/// and the audited sum are album-scoped (2b PR-(4)'s Decision B: the album selects the box),")
    add("/// so they are generated into `assets.rs`; this module stays a function of the spec alone.")
    add(f"pub const POOL_TEXT_DISTINCT: u32 = {_rs(facts.text_distinct)};")
    add(f"pub const POOL_TEXT_UPPER: u32 = {_rs(facts.text_upper)};")
    add(f"pub const POOL_CHROME_DECLARED: u32 = {_rs(variant.chrome_art_tiles)};")
    add(f"pub const POOL_CHROME_DERIVED: u32 = {_rs(facts.chrome_derived)};")
    add(f"pub const POOL_UPPER: u32 = {_rs(facts.pool_upper)};")
    add("")
    add("/// One band of §5.0's band table. `rows` is tile rows: the spec's unit, because agb allocates")
    add("/// tiles and §5.0's first correction to its own table is that a text-row count and a tile-row")
    add("/// count are different numbers. No text-row count is stored anywhere here, for the same reason.")
    add("pub struct Band {")
    add("    pub id: &'static str,")
    add("    /// First screen row of this band from the top. The bands are generated bottom-up because")
    add("    /// that is how §5.0's figure is drawn: region, then title band, then chrome at the bottom.")
    add("    pub screen_row: u32,")
    add("    pub rows: u32,")
    add("    pub px_y: u32,")
    add("    pub px_h: u32,")
    add("    pub always: bool,")
    add("}")
    add(f"pub const BANDS: [Band; {len(variant.bands)}] = [")
    for band in variant.bands:
        add("    Band {")
        add(f'        id: "{band.id}",')
        add(f"        screen_row: {band.screen_row},")
        add(f"        rows: {band.rows},")
        add(f"        px_y: {band.screen_row * facts.tile_px},")
        add(f"        px_h: {band.rows * facts.tile_px},")
        add(f"        always: {str(band.always).lower()},")
        add("    },")
    add("];")
    add("/// Bands tile the screen exactly. §5.0's first refusal (`bands-not-tiling`) is about this sum.")
    add("pub const fn band_rows_total() -> u32 {")
    add("    let mut total = 0;")
    add("    let mut i = 0;")
    add("    while i < BANDS.len() {")
    add("        total += BANDS[i].rows;")
    add("        i += 1;")
    add("    }")
    add("    total")
    add("}")
    add("")
    add("/// The scene region: the one rectangle a scene is given, and nothing else (§5.0). The scene")
    add("/// registry's `draw(region)` boundary *is* the enforcement, which is why the region is a generated")
    add("/// constant rather than something a scene asks the screen for.")
    add("pub struct Region {")
    add("    pub band: &'static str,")
    add("    pub cols: u32,")
    add("    pub rows: u32,")
    add("    pub px_w: u32,")
    add("    pub px_h: u32,")
    add("}")
    add("pub const REGION: Region = Region {")
    add(f'    band: "{facts.region_band.id}",')
    add(f"    cols: {facts.region[0]},")
    add(f"    rows: {facts.region[1]},")
    add(f"    px_w: {facts.region[2]},")
    add(f"    px_h: {facts.region[3]},")
    add("};")
    add("pub const fn region_is_inside_screen() -> bool {")
    add("    REGION.cols <= SCREEN_COLS && REGION.rows <= SCREEN_ROWS")
    add("        && REGION.px_w == REGION.cols * TILE_PX")
    add("        && REGION.px_h == REGION.rows * TILE_PX")
    add("}")
    add("")
    add("/// A declared face and what its own tables say at its declared raster (§5.1, §5.5). The line box")
    add("/// is the number §5.0's title-band floor rests on and §5.1's pool arithmetic is built on, read out")
    add("/// of the .ttf rather than copied from the plan — which is why §9.0's `pitch-not-tileable` row can")
    add("/// cite the declared face's metrics instead of a range of allowed numbers.")
    add("pub struct Face {")
    add("    pub id: &'static str,")
    add("    pub file: &'static str,")
    add("    pub raster: u32,")
    add("    /// The smallest pixel size at which this file is entirely on the pixel grid, derived from the")
    add("    /// gcd of its own coordinates. §5.5's rule is that a declared raster must be this or a whole")
    add("    /// multiple of it; the counts below are what make that a measurement rather than a formula.")
    add("    pub design_px: u32,")
    add("    pub upem: u32,")
    add("    pub outlined_glyphs: u32,")
    add("    pub mapped_codepoints: u32,")
    add("    pub line_box_px: u32,")
    add("    pub min_row_px: u32,")
    add("    pub tile_rows_per_line: u32,")
    add("    /// The distinct bound's text term for this face: every outlined glyph times the tile rows its")
    add("    /// line box spans (§5.1's `distinct cap` column, from the cut that actually exists).")
    add("    pub atlas_tiles: u32,")
    add("    /// Advances in ten-thousandths of a pixel, so a fit verdict on the device stays integer")
    add("    /// arithmetic. The mean is what §5.1's `chars in a line` column is computed from; the maximum")
    add("    /// is what a conservative span is computed from. Neither is what the fit verdict itself uses:")
    add("    /// that measures the string, in pixels, against a box (§5.5).")
    add("    pub max_advance_permyriad: u32,")
    add("    pub mean_advance_permyriad: u32,")
    add("    /// Every mapped code point's advance width, in this face's own font units, as")
    add("    /// `(codepoint, units)` sorted by codepoint. This is the table §5.5's fit verdict")
    add("    /// measures a string with: a verdict computed from a mean or a maximum advance is")
    add("    /// a bound, not a measurement, and D9 item 2 is about the difference. Read from the")
    add("    /// same .ttf `hmtx` table the bake read, so the two cannot drift.")
    add("    pub advances: &'static [(u32, u16)],")
    add("}")
    # One table per face, named from the face id so the module is readable and a
    # duplicate id is a compile error rather than a silent last-wins.
    for face_id, metrics in facts.face_metrics.items():
        ident = "ADVANCES_" + "".join(c if c.isalnum() else "_" for c in face_id).upper()
        units = sorted(metrics.advance_units.items(), key=lambda kv: ord(kv[0]))
        add(f"/// `{face_id}`'s advances, from `{metrics.face.file}`'s `hmtx` and `head.unitsPerEm` {metrics.upem}.")
        add(f"pub const {ident}: [(u32, u16); {len(units)}] = [")
        row: list[str] = []
        for char, value in units:
            row.append(f"(0x{ord(char):02x}, {value}),")
            if len(row) == 8:
                add("    " + " ".join(row))
                row = []
        if row:
            add("    " + " ".join(row))
        add("];")
        add("")
    add(f"pub const FACES: [Face; {len(facts.face_metrics)}] = [")
    for face_id, metrics in facts.face_metrics.items():
        ident = "ADVANCES_" + "".join(c if c.isalnum() else "_" for c in face_id).upper()
        add("    Face {")
        add(f'        id: "{face_id}",')
        add(f'        file: "{metrics.face.file}",')
        add(f"        raster: {metrics.raster},")
        add(f"        design_px: {metrics.design_px},")
        add(f"        upem: {metrics.upem},")
        add(f"        outlined_glyphs: {metrics.outlined},")
        add(f"        mapped_codepoints: {metrics.mapped_count},")
        add(f"        line_box_px: {metrics.line_box_px},")
        add(f"        min_row_px: {metrics.min_row_px},")
        add(f"        tile_rows_per_line: {metrics.tile_rows_per_line},")
        add(f"        atlas_tiles: {metrics.outlined * metrics.tile_rows_per_line},")
        add(f"        max_advance_permyriad: {round(metrics.max_advance_px * 10_000)},")
        add(f"        mean_advance_permyriad: {round(metrics.avg_advance_px * 10_000)},")
        add(f"        advances: &{ident},")
        add("    },")
    add("];")
    add("")
    add("/// §5.5's fit verdict for one string against one declared box, measured rather than counted.")
    add("pub struct Fit {")
    add("    /// The string's width in pixels, for a report line. Truncating: the verdict below is the")
    add("    /// inequality, done before any division.")
    add("    pub width_px: u32,")
    add("    /// Whether the string fits the box it was measured against: `units × raster ≤ box × upem`,")
    add("    /// the same inequality `scripts/album_bake.py`'s `FontMetrics::fits` uses, so the host gate")
    add("    /// and the bake cannot disagree about a verdict.")
    add("    pub inside_box: bool,")
    add("    /// Code points this face has no glyph for. §5.5 keeps this verdict separate from width:")
    add("    /// an unwritten character is not an elision, and a title that renders partly blank is a")
    add("    /// different failure from a title that overflows.")
    add("    pub unwritten: usize,")
    add("}")
    add("")
    add("impl Face {")
    add("    /// The advance width of `ch` in this face's own font units, or `None` when the face has no")
    add("    /// glyph for it. A linear scan over a 95-entry table: this is a measurement path, never the")
    add("    /// paint path, and §6's ban on divides is why the paint path carries a reciprocal instead.")
    add("    pub fn advance_units(&self, ch: char) -> Option<u32> {")
    add("        let code = ch as u32;")
    add("        for &(point, units) in self.advances {")
    add("            if point == code {")
    add("                return Some(units as u32);")
    add("            }")
    add("        }")
    add("        None")
    add("    }")
    add("")
    add("    /// §5.5's fit verdict: measure `text` from this face's own advances and compare it with a")
    add("    /// declared box in pixels. Never a count of characters, and never a float comparison.")
    add("    pub fn measure(&self, text: &str, box_px: u32) -> Fit {")
    add("        let mut units: u32 = 0;")
    add("        let mut unwritten = 0usize;")
    add("        for ch in text.chars() {")
    add("            match self.advance_units(ch) {")
    add("                Some(width) => units += width,")
    add("                None => unwritten += 1,")
    add("            }")
    add("        }")
    add("        Fit {")
    add("            // A divide for the reported width only: the verdict below is the same inequality")
    add("            // without one, and this function is called from the host gates, not from a repaint.")
    add("            width_px: units * self.raster / self.upem,")
    add("            inside_box: units * self.raster <= box_px * self.upem,")
    add("            unwritten,")
    add("        }")
    add("    }")
    add("}")
    add("")
    add("/// One element of the layout: a box owned by a band or by a scene (§5.0's second correction — a")
    add("/// chrome element's geometry belongs to the band, not to a scene). Coordinates are as declared")
    add("/// (tiles, band-local) *and* resolved (pixels, screen-absolute), because the drawing path needs one")
    add("/// and a fit report needs the other.")
    add("pub struct Element {")
    add("    pub id: &'static str,")
    add('    pub kind: &\'static str, // "text" | "art" | "bar" (§5.0\'s element table)')
    add("    pub band: &'static str,")
    add('    pub scene: &\'static str, // "" for a band-owned element: chrome, title band')
    add('    pub face: &\'static str, // "" when the element draws no text')
    add("    pub xt: u32,")
    add("    pub yt: u32,")
    add("    pub wt: u32,")
    add("    pub ht: u32,")
    add("    pub x: u32,")
    add("    pub y: u32,")
    add("    pub w: u32,")
    add("    pub h: u32,")
    add("    /// The declared pitch, and the declared face's own line box: a pitch below the line box is")
    add("    /// `pitch-not-tileable`, and the line box is not a constant this file could have made up.")
    add("    pub row_px: u32,")
    add("    pub line_box_px: u32,")
    add("    /// Derived: `h / row_px`. §11's D9 is why this is a generated consequence rather than a")
    add("    /// number anyone writes down: quoting it turns one screen into three.")
    add("    /// Derived from the declared pitch: `h / row_px`, the cells a row holds at the declared")
    add("    /// face's mean advance, and the conservative span at its maximum advance. The fit verdict")
    add("    /// itself is none of these: it measures the folded string in pixels against `w` (§5.5).")
    add("    pub text_rows: u32,")
    add("    pub cells_per_row: u32,")
    add("    pub max_chars: u32,")
    add("    /// §5.4's allocation: which banks this element may use, from the spec's [[palette] role rows.")
    add("    pub banks: &'static [u8],")
    add("}")
    add(f"pub const ELEMENTS: [Element; {len(facts.elements)}] = [")
    for element in facts.elements:
        add("    Element {")
        add(f'        id: "{element.id}",')
        add(f'        kind: "{element.kind}",')
        add(f'        band: "{element.band.id}",')
        add(f'        scene: "{element.scene}",')
        add(f'        face: "{element.face}",')
        for field in ("xt", "yt", "wt", "ht", "x", "y", "w", "h", "row_px"):
            add(f"        {field}: {getattr(element, field)},")
        add(f"        line_box_px: {element.line_box_px},")
        add(f"        text_rows: {element.text_rows},")
        add(f"        cells_per_row: {element.cells_per_row},")
        add(f"        max_chars: {element.max_chars},")
        add(f"        banks: &{element.banks},")
        add("    },")
    add("];")
    add("/// Look a generated rectangle up by the name the spec uses. A scene or a renderer asks for a box")
    add("/// by name; nothing here hard-codes an index, which is the difference between a band table with a")
    add("/// slot in it and a layout hard-coded in application source (§5.0).")
    add("pub fn element(id: &str) -> Option<&'static Element> {")
    add("    let mut i = 0;")
    add("    while i < ELEMENTS.len() {")
    add("        if ELEMENTS[i].id == id {")
    add("            return Some(&ELEMENTS[i]);")
    add("        }")
    add("        i += 1;")
    add("    }")
    add("    None")
    add("}")
    add("pub fn band(id: &str) -> Option<&'static Band> {")
    add("    let mut i = 0;")
    add("    while i < BANDS.len() {")
    add("        if BANDS[i].id == id {")
    add("            return Some(&BANDS[i]);")
    add("        }")
    add("        i += 1;")
    add("    }")
    add("    None")
    add("}")
    add("pub fn face(id: &str) -> Option<&'static Face> {")
    add("    let mut i = 0;")
    add("    while i < FACES.len() {")
    add("        if FACES[i].id == id {")
    add("            return Some(&FACES[i]);")
    add("        }")
    add("        i += 1;")
    add("    }")
    add("    None")
    add("}")
    add("/// Integer square root, because the art ceiling is a derived fact and this file re-checks the")
    add("/// derivation rather than trusting it (§7: the validator rejects a declared box it cannot name, so")
    add("/// the list has to be reproducible here, not only in Python).")
    add("const fn isqrt(value: u32) -> u32 {")
    add("    let mut root = 0;")
    add("    while (root + 1) * (root + 1) <= value {")
    add("        root += 1;")
    add("    }")
    add("    root")
    add("}")
    add("const fn element_rows_are_derived() -> bool {")
    add("    let mut i = 0;")
    add("    while i < ELEMENTS.len() {")
    add("        let e = &ELEMENTS[i];")
    add("        if e.x + e.w > SCREEN_COLS * TILE_PX || e.y + e.h > SCREEN_ROWS * TILE_PX {")
    add("            return false;")
    add("        }")
    add("        if e.row_px % TILE_PX != 0 || (e.row_px != 0 && e.row_px < e.line_box_px) {")
    add("            return false;")
    add("        }")
    add("        if e.row_px != 0 && e.text_rows * e.row_px > e.h {")
    add("            return false;")
    add("        }")
    add("        i += 1;")
    add("    }")
    add("    true")
    add("}")
    add("")
    if facts.art_element is not None:
        art = facts.art_element
        max_side = max(box.side for box in facts.art_boxes)
        add("/// §7's legal box list, as a derived rule rather than a list of transcribed numbers: every")
        add("/// square this variant can actually draw. §9.0's `art-box-not-derived` row is 2b's refusal, on")
        add("/// a value `[art] box` will carry; 2a owns this list and the validator, so what follows is the")
        add("/// function that refusal calls, plus its ceiling and the centring offsets §7 says a fit report")
        add("/// has to print in real pixels (which is also why §5.0's 120x120 in a 136 px region is a")
        add("/// 4 px off-centre rather than a mismatch, and why `Q7`'s art-box question is a decision 2b")
        add("/// makes against this list rather than a number this file fixes).")
        add("pub struct ArtBox {")
        add("    pub side_tiles: u32,")
        add("    pub tiles: u32,")
        add("    pub px: u32,")
        add("    pub offset_x_px: u32,")
        add("    pub offset_y_px: u32,")
        add("    pub off_centre_x_px: u32,")
        add("    pub off_centre_y_px: u32,")
        add("}")
        add(f"pub const ART_ELEMENT: &str = \"{art.id}\";")
        add(f"pub const ART_ELEMENT_INDEX: usize = {facts.elements.index(art)};")
        add(f"pub const ART_MAX_SIDE: u32 = {max_side};")
        add(f"pub const ART_CEILING: ArtBox = ArtBox {{")
        add(f"    side_tiles: {ceiling.side},")
        add(f"    tiles: {ceiling.tiles},")
        add(f"    px: {ceiling.width_px},")
        add(f"    offset_x_px: {ceiling.x_off},")
        add(f"    offset_y_px: {ceiling.y_off},")
        add(f"    off_centre_x_px: {ceiling.off_centre_x},")
        add(f"    off_centre_y_px: {ceiling.off_centre_y},")
        add("};")
        add("pub const fn art_box_is_legal(side_tiles: u32) -> bool {")
        add("    side_tiles >= 1 && side_tiles <= ART_MAX_SIDE")
        add("}")
        add("/// Where a box of that side is centred inside the element's own tile box, and how far off")
        add("/// centre it ends up: in pixels, because §5.0's note that no square is perfectly")
        add("/// tile-centred in a 17-row region is only actionable in pixels.")
        add("pub const fn art_offset_x_px(side_tiles: u32) -> u32 {")
        add("    if side_tiles > ELEMENTS[ART_ELEMENT_INDEX].wt {")
        add("        0")
        add("    } else {")
        add("        ((ELEMENTS[ART_ELEMENT_INDEX].wt - side_tiles) / 2) * TILE_PX")
        add("    }")
        add("}")
        add("pub const fn art_off_centre_x_px(side_tiles: u32) -> u32 {")
        add("    if side_tiles > ELEMENTS[ART_ELEMENT_INDEX].wt {")
        add("        0")
        add("    } else {")
        add("        ((ELEMENTS[ART_ELEMENT_INDEX].wt - side_tiles) % 2) * (TILE_PX / 2)")
        add("    }")
        add("}")
        add("const fn art_ceiling_is_derived() -> bool {")
        add("    let e = &ELEMENTS[ART_ELEMENT_INDEX];")
        add("    let room = if e.wt < e.ht { e.wt } else { e.ht };")
        add("    let budget = isqrt(IMAGE_TILES_MAX);")
        add("    let limit = if room < budget { room } else { budget };")
        add("    ART_MAX_SIDE == limit && ART_CEILING.tiles == limit * limit")
        add("        && ART_CEILING.px <= REGION.px_w && ART_CEILING.px <= REGION.px_h")
        add("        && ART_CEILING.tiles <= IMAGE_TILES_MAX")
        add("}")
    else:
        add("/// No element of this variant draws scene art, so §7's box list is empty and")
        add("/// `art-box-not-derived` has nothing to validate against yet. Said rather than assumed.")
        add("pub const ART_MAX_SIDE: u32 = 0;")
        add("pub const fn art_box_is_legal(_side_tiles: u32) -> bool {")
        add("    false")
        add("}")
    add("")
    add("/// The control positions (§5.3): declared, not assumed by the art, and one tile each, so the")
    add("/// authored keyart can be made against them and 2c has positions to name rather than invent.")
    add("pub struct ControlSlot {")
    add("    pub key: &'static str,")
    add("    pub band: &'static str,")
    add("    pub element: &'static str,")
    add("    pub xt: u32,")
    add("    pub yt: u32,")
    add("    pub px_x: u32,")
    add("    pub px_y: u32,")
    add("}")
    add(f"pub const CONTROL_SLOTS: [ControlSlot; {len(facts.control_slots)}] = [")
    for control in facts.control_slots:
        band = facts.bands[control.band]
        add("    ControlSlot {")
        add(f'        key: "{control.key}",')
        add(f'        band: "{control.band}",')
        add(f'        element: "{control.slot}",')
        add(f"        xt: {control.x0},")
        add(f"        yt: {control.y0},")
        add(f"        px_x: {control.x0 * facts.tile_px},")
        add(f"        px_y: {(band.screen_row + control.y0) * facts.tile_px},")
        add("    },")
    add("];")
    add("const fn control_slots_are_inside_screen() -> bool {")
    add("    let mut i = 0;")
    add("    while i < CONTROL_SLOTS.len() {")
    add("        let c = &CONTROL_SLOTS[i];")
    add("        if (c.xt + 1) * TILE_PX > SCREEN_COLS * TILE_PX {")
    add("            return false;")
    add("        }")
    add("        if (c.yt + 1) * TILE_PX + (c.px_y - c.yt * TILE_PX) > SCREEN_ROWS * TILE_PX {")
    add("            return false;")
    add("        }")
    add("        i += 1;")
    add("    }")
    add("    true")
    add("}")
    add("")
    add("// The self-checks. Each one is something the generator already refused to derive past; keeping it")
    add("// here costs nothing when the module is fresh and catches the one case no host test can see: this")
    add("// file, unchanged, linked against a manifest baked from a different spec (§6's reasoning about why")
    add("// the boot-time bar check *and* the bake-time check both stay; §11's R11 about staleness).")
    add("const _: () = assert!(band_rows_total() == SCREEN_ROWS, \"bands do not tile the screen\");")
    add("const _: () = assert!(region_is_inside_screen(), \"the region is not inside the screen\");")
    add("const _: () = assert!(element_rows_are_derived(), \"a pitch is not tileable or a box is off-screen\");")
    add("const _: () = assert!(control_slots_are_inside_screen(), \"a control slot is off-screen\");")
    add("const _: () = assert!(TILE_BUDGET <= POOL_SLOTS, \"a declared budget exceeds the pool agb has\");")
    add("const _: () = assert!(RESIDENT_SCENES <= RESIDENT_SCENE_MAX, \"more scenes than the hardware has\");")
    add("const _: () = assert!(")
    add("    BAR_CELLS == 0")
    add("        || (BAR_CELLS >= BAR_CELLS_FLOOR")
    add("            && BAR_PX == BAR_CELLS * BAR_CELL_PX")
    add("            && BAR_PX % BAR_CELLS == 0),")
    add("    \"the declared bar grid is below §6's floor, or its cells are not whole pixels\"")
    add(");")
    if facts.art_element is not None:
        add("const _: () = assert!(art_ceiling_is_derived(), \"the art ceiling is not the derived value\");")
    add("")
    add("/// §5.4's allocation as the spec declares it, for anything that draws: a bank this names is in")
    add("/// one role only. 2b's palette refusals (`palette-bank-out-of-allocation`, `palette-role-collision`,")
    add("/// `palette-role-unavailable`) read real images and belong to that phase; this is the part 2a can")
    add("/// state without an image, and it is generated rather than re-typed so the two cannot drift.")
    add("pub struct PaletteRole {")
    add("    pub role: &'static str,")
    add("    pub banks: &'static [u8],")
    add("}")
    add(f"pub const PALETTE: [PaletteRole; {len(variant.palettes)}] = [")
    for palette in variant.palettes:
        add("    PaletteRole {")
        add(f'        role: "{palette.role}",')
        add(f"        banks: &{palette.banks},")
        add("    },")
    add("];")
    add("pub const fn palette_banks_total() -> usize {")
    add("    let mut total = 0;")
    add("    let mut i = 0;")
    add("    while i < PALETTE.len() {")
    add("        total += PALETTE[i].banks.len();")
    add("        i += 1;")
    add("    }")
    add("    total")
    add("}")
    add("const _: () = assert!(palette_banks_total() <= 16, \"§5.4's allocation exceeds 16 banks\");")
    add("")
    return "\n".join(out) + "\n"


# --- the register's legs: `--layout-self-test` --------------------------------

#: A tracked face this repo already has, for the fixtures: an untracked or absent
#: file would fire `face-not-resolvable` for every leg and prove nothing.
FIXTURE_FACE_FILE = "assets/fonts/fusion-pixel-8px-proportional-latin-ascii.ttf"

FIXTURE_BASE = """
[meta]
screen_tiles = [30, 20]
map_tiles = [32, 32]
tile_px = 8
pool_slots = 512
resident_scene_max = 3

[[face]]
id = "fixture"
file = "assets/fonts/fusion-pixel-8px-proportional-latin-ascii.ttf"
raster = 8
product_face = true
licence = ["assets/fonts/fusion-pixel-OFL.txt", "assets/fonts/fusion-pixel-LICENSES"]

[[variant]]
id = "F"
product = true
tile_budget = 512
image_tiles = 196
bar_cells = 20
chrome_art_tiles = 9
glyph_atlas = "static"
band = [{id="chrome", rows=2, always=true}, {id="title", rows=2, always=true}, {id="scene", rows=16, always=false}]
palette = [{role="text", banks=[0]}, {role="chrome", banks=[1]}, {role="art", banks=[3]}, {role="viz", banks=[4]}, {role="game", banks=[5]}]
scene = [{name="art", phase="2b", band="scene", kind="art", resident=true, min_boxes=1}, {name="list", phase="2b", band="scene", kind="text", resident=true, min_visible_rows=4}, {name="title_card", phase="2b", band="scene", kind="composite", resident=false, shares_image="art"}]
element = [
  {id="clock", kind="text", band="chrome", x0=0, y0=0, x1=11, y1=1, face="fixture", row_px=16, banks=[0]},
  {id="keyart", kind="art", band="chrome", x0=20, y0=0, x1=29, y1=1, banks=[1]},
  {id="title_text", kind="text", band="title", x0=0, y0=0, x1=19, y1=1, face="fixture", row_px=16, banks=[0]},
  {id="bar", kind="bar", band="title", x0=20, y0=1, x1=29, y1=1},
  {id="art", kind="art", band="scene", scene="art", x0=0, y0=0, x1=29, y1=15, banks=[3]},
  {id="list", kind="text", band="scene", scene="list", x0=0, y0=0, x1=29, y1=15, face="fixture", row_px=16, banks=[0]},
  {id="card_art", kind="art", band="scene", scene="title_card", x0=0, y0=0, x1=9, y1=9, banks=[3]},
]
control = [
  {key="start", band="chrome", slot="keyart", x0=21, y0=0, x1=21, y1=0},
  {key="select", band="chrome", slot="keyart", x0=20, y0=0, x1=20, y1=0},
  {key="a", band="chrome", slot="keyart", x0=29, y0=0, x1=29, y1=0},
  {key="b", band="chrome", slot="keyart", x0=28, y0=1, x1=28, y1=1},
  {key="right", band="chrome", slot="keyart", x0=27, y0=0, x1=27, y1=0},
]
""".strip()

#: Each leg is (register id, how it is exercised, mutations). The point of
#: generating a spec per leg is §9.0's requirement that the tool's own error
#: paths be *asserted*: every leg names its refusal id, and the run says out loud
#: which of them it watched fire.
FIXTURE_LEGS = (
    ("bands-not-tiling", "a band's rows no longer tile the screen", ("rows=16, always=false", "rows=15, always=false")),
    ("bands-not-tiling", "an element box reaches past its region", ('x1=29, y1=15, face="fixture"', 'x1=30, y1=15, face="fixture"')),
    ("pitch-not-tileable", "a pitch that is not a whole number of tile rows", ('x1=29, y1=15, face="fixture", row_px=16', 'x1=29, y1=15, face="fixture", row_px=12')),
    ("pitch-not-tileable", "a pitch a whole number of rows but below the face's line box", ('x1=29, y1=15, face="fixture", row_px=16', 'x1=29, y1=15, face="fixture", row_px=8')),
    ("pitch-not-tileable", "a declared raster that is not a multiple of the file's design size", ("raster = 8", "raster = 4")),
    ("face-not-resolvable", "a declared face file that does not exist", ("fusion-pixel-8px-proportional-latin-ascii", "no-such-face-at-all")),
    ("face-not-resolvable", "a declared face that is present but not tracked in git", ("fusion-pixel-8px-proportional-latin-ascii", "layout-self-test-untracked")),
    ("face-not-resolvable", "a licence row that is absent", ('licence = ["assets/fonts/fusion-pixel-OFL.txt", "assets/fonts/fusion-pixel-LICENSES"]', 'licence = ["assets/fonts/not-a-licence.txt"]')),
    ("image-tiles-over-budget", "the distinct pool sum over the declared budget", ("tile_budget = 512", "tile_budget = 300")),
    ("image-tiles-over-budget", "the upper pool sum over budget for a per-cell atlas", ('glyph_atlas = "static"', 'glyph_atlas = "per-cell"')),
    ("image-tiles-over-budget", "more resident scenes than the cap the spec states", ("resident_scene_max = 3", "resident_scene_max = 1")),
    ("scene-minimum", "a scene's own row minimum above what the geometry holds", ("min_visible_rows=4", "min_visible_rows=9")),
    ("scene-minimum", "a scene's own box minimum above the derived list", ("min_boxes=1", "min_boxes=99")),
    ("bar-cells-out-of-range", "a declared grid below §6's floor", ("bar_cells = 20", "bar_cells = 8")),
    ("bar-cells-out-of-range", "a grid that does not divide the declared box into whole pixels", ("bar_cells = 20", "bar_cells = 30")),
)

#: The §9.0 row this tool cannot reach from the fixture's own geometry, so its leg
#: mutates a box: the title element of the fixture variant narrowed to a single tile
#: column (8 px), which is narrower than the three-period marker at that face at that
#: raster (12 px, measured in the leg). Held as data for the same reason the other legs
#: are: `_fixture_text` refuses to apply a mutation whose left side has gone missing from
#: the fixture, so a stale leg is an assertion rather than a silent no-op.
TITLE_BOX_ONE_COLUMN = (
    'band="title", x0=0, y0=0, x1=19, y1=1, face="fixture"',
    'band="title", x0=0, y0=0, x1=0, y1=1, face="fixture"',
)

#: §5.1's measured table, transcribed from ALBUM-PLAYER.md and re-measured here from the
#: font files themselves. The document says of that table that its numbers "were recomputed
#: from the files, in one pass, with the method stated, and every derived number in this
#: section follows from that one table" — so what *this* tool answers for the same face at
#: the same raster is a check on both sides at once: if the plan's table and the generator
#: disagree, one of them is wrong, and neither is allowed to assume which. The columns are
#: the document's, in the document's order. `grid` is the ink count the document's
#: "grid-aligned?" cell prints ("no — 94 of 94" → 94), or None where the cell says yes.
MEASURED_FACE_TABLE = (
    # face, raster, the document's label, grid, line box, min row_px, ink, mean advance,
    #   cells in a 240 px line, slots per text row (upper bound, distinct cap)
    ("fusion8", 4, "0.5×", 94, 6, 8, 4.0, 2.28, 105, 105, 94),
    ("fusion8", 8, "1× (design size)", None, 12, 16, 8.0, 4.56, 52, 104, 188),
    ("fusion8", 12, "1.5×", 94, 18, 24, 12.0, 6.85, 35, 105, 282),
    ("fusion8", 16, "2×", None, 24, 24, 16.0, 9.13, 26, 78, 282),
    ("ark10", 5, "0.5×", 94, 8, 8, 4.5, 2.72, 88, 88, 94),
    ("ark10", 8, "0.8×", 94, 13, 16, 7.2, 4.35, 55, 110, 188),
    ("ark10", 10, "1× (design size)", None, 16, 16, 9.0, 5.44, 44, 88, 188),
    ("ark10", 12, "1.2×", 94, 20, 24, 10.8, 6.52, 36, 108, 282),
    ("ark10", 16, "1.6×", 94, 26, 32, 14.4, 8.70, 27, 108, 376),
    ("ark10", 20, "2×", None, 32, 32, 18.0, 10.87, 22, 88, 376),
)

#: §5.1's second table: the same faces at the pitch each row declares, in a 128 px region,
#: against a 512-slot regular pool. These rows are *exercised*, not transcribed: each row
#: becomes a probe spec — bands that tile 20 rows, one text element at the row's `row_px`,
#: an art element whose derived ceiling is the 128×128 = 256 tiles the table's first column
#: adds — and this tool derives it. The table's ✔/✘ is the same question
#: `image-tiles-over-budget` asks, so the marks come out as raise/no-raise rather than as
#: transcription. The document's second art term (160×120 = 300 tiles) is priced against
#: the text term this tool derived, because 300 is not a box any variant derives: it is not
#: a square, and §7's boxes are squares — the column is an illustration, and saying so is
#: part of reproducing it.
INTERACTION_TABLE = (
    # face, raster, row_px, rows in a 128 px region, text upper bound, text distinct cap,
    #   with a 128×128 (256) art term: (total at the distinct bound, fits 512?, total at
    #   the upper bound or None where the document prints none), same for a 160×120 (300)
    ("fusion8", 8, 16, 8, 832, 188, (444, True, 1088), (488, True, 1132)),
    ("ark10", 10, 16, 8, 704, 188, (444, True, 960), (488, True, 1004)),
    ("fusion8", 16, 24, 5, 390, 282, (538, False, None), (582, False, None)),
    ("ark10", 20, 32, 4, 352, 376, (632, False, None), (676, False, None)),
    ("fusion8", 4, 8, 16, 1680, 94, (350, True, None), (394, True, None)),
)

#: §9.0's D9 half, stated as an executable claim: a number that appears in more than one
#: artifact must be generated from one source. These are the strings that would mean a
#: character-count or fixed-grid box is still owning something; the search is over code, so
#: prose about the old number does not count. The names come from D9 ("geometry is stated
#: three times in the tree") as the three copies this PR removes, and the rule they are
#: removed by is §5.5's: "every text element's fit box now comes from the variant's declared
#: *span* (a pixel width — §5.0) and the face's real metrics". Quoted by section, because a
#: leg that invents its own source sentence is worse than one that cites no sentence at all.
BANNED_CHARACTER_BOX = ("TITLE_BOX_MAX", "def title_fits", "MAX_TITLE", "chars().count() <= 28")
#
# What this leg does NOT prove, stated here so nobody reads it as a wider claim: it is a
# grep for four retired *names*, so it answers "did the 28-character box come back?" and
# nothing else. One character-count bound is live in product code and stays live by
# decision: `examples/album_player/src/text.rs:65`'s `COLUMNS = 30`, the verdict line, which
# is 30 columns of the instrument face's 8 px cell = the whole 240 px row — a measured cell
# width, not a box that stands in for one, and witnessed by `album_seam_host.rs:811-831`.
# §5.5's ruling retires the *fit* box made of characters; converting the verdict line to the
# same measured fit is 2a PR-(2)'s, with `measure`/`fit`.
# Split by whose code it is. A library that owns a character box and a host test that
# re-establishes one are two different pieces of work, so the legs stay separate and each
# failure names one of them.
BANNED_SEARCH_PRODUCT = (
    "scripts/album_bake.py", "scripts/album_prep.py", "scripts/album_pack.py",
    "examples/album_player/src", "crates/gba-playback/src",
)
BANNED_SEARCH_TESTS = ("examples/album_player/tests",)


def _blank_python(src: str) -> str:
    """`src` with string literals and comments blanked, line numbers preserved."""
    python_comment = re.compile(r"#.*$")
    any_quote = re.compile(r"'(?:[^'\\]|\\.)*'" r'|"(?:[^"\\]|\\.)*"')
    lines = src.splitlines()
    try:
        toks = list(tokenize.generate_tokens(io.StringIO(src).readline))
    except (tokenize.TokenError, IndentationError, SyntaxError):
        return "\n".join(any_quote.sub("", python_comment.sub("", ln)) for ln in lines)
    grid = [list(ln) for ln in lines]
    text_tokens = {tokenize.STRING, tokenize.COMMENT}
    for name in ("FSTRING_START", "FSTRING_MIDDLE", "FSTRING_END"):
        code = getattr(tokenize, name, None)
        if code is not None:
            text_tokens.add(code)
    for tok in toks:
        if tok.type not in text_tokens:
            continue
        for row in range(tok.start[0], tok.end[0] + 1):
            cols = grid[row - 1]
            lo = tok.start[1] if row == tok.start[0] else 0
            hi = tok.end[1] if row == tok.end[0] else len(cols)
            for col in range(min(lo, len(cols)), min(hi, len(cols))):
                cols[col] = " "
    return "\n".join("".join(row) for row in grid)


def _blank_rust(src: str) -> str:
    """`src` with comments (line and block) and string literals removed, line numbers kept."""
    out: list[str] = []
    in_block = False
    for line in src.splitlines():
        keep: list[str] = []
        i = 0
        if in_block:
            end = line.find("*/")
            if end < 0:
                out.append("")
                continue
            in_block, i = False, end + 2
        while i < len(line):
            if line.startswith("//", i):
                break
            if line.startswith("/*", i):
                end = line.find("*/", i + 2)
                in_block = end < 0
                i = len(line) if in_block else end + 2
                continue
            if line[i] == '"':
                j = i + 1
                while j < len(line) and line[j] != '"':
                    j += 2 if line[j] == "\\" else 1
                i = j + 1
                continue
            keep.append(line[i])
            i += 1
        out.append("".join(keep))
    return "\n".join(out)


def _banned_box_hits(paths: tuple[str, ...]) -> list[str]:
    """Where a retired character-count box is *used*, not mentioned: `path:line: hit`.

    The ban is on code that owns a fit. The sentences that retire the old number — this
    tool's own tuple of banned strings, the wording of the leg that greps for them, the
    generated module's doc comment that says those names are gone — name it without using
    it, and a grep that reported those would be quoting itself. So literals and comments
    are blanked first (Python through the tokenizer, so a triple-quoted block is blanked
    whole; Rust through a scanner for `//`, `/* */` and `"…"`), and each matching line is
    reported once with every pattern it matched.
    """
    hits: list[str] = []
    for rel in paths:
        target = REPO / rel
        if target.is_file():
            found = [target]
        elif target.is_dir():
            found = sorted(list(target.rglob("*.rs")) + list(target.rglob("*.py")))
        else:
            continue
        for path in found:
            text = path.read_text(encoding="utf-8", errors="replace")
            code = _blank_python(text) if path.suffix == ".py" else _blank_rust(text)
            for no, line in enumerate(code.splitlines(), 1):
                matched = [banned for banned in BANNED_CHARACTER_BOX if banned in line]
                if matched:
                    hits.append(
                        f"{path.relative_to(REPO)}:{no}: {', '.join(matched)} in {line.strip()[:110]}"
                    )
    return hits


def _probe_spec_text(face: FaceSpec, raster: int, row_px: int) -> str:
    """A one-face, one-pitch variant, written in the syntax the real spec uses.

    §5.1's interaction table is a claim about what a face at a pitch *costs*, so the way
    to reproduce it is to build that variant and let this tool's own budget gate answer,
    not to read its totals off the table. The probe is stripped to exactly the two terms
    the table's column adds:

      * the list element's box is **128 px tall** (16 tile rows), because that is the
        region the table's column names — "text rows in a 128 px region" — and it is also
        §5.0's region figure. A probe measured against a taller region would be measuring
        something the table does not claim, and the row counts would not be comparable;
      * no bar element and `chrome_art_tiles = 0`, so the sum this tool compares to the
        table's ✔/✘ is image + text and nothing the table does not price. `bar_cells` is
        still declared: a variant with no bar is a shape the plan does not describe, and
        the field would then be the only kind nothing in this run exercises.

    The scene set mirrors the real spec's (`art` and `list`, both 2b, plus the 2a `blank`)
    including `min_visible_rows`, because §5.2's scene minimums are part of what a variant
    has to survive before it ever reaches the budget gate.

    `product_face = false` on the probe face, deliberately. The generator refuses a product
    face at a raster that is not a whole multiple of the file's design size — which is a
    rule the plan endorses (§5.1's "reducing or adding in between is ruled out") and one a
    register row above already watches fire. But the table's rows include such rasters (its
    0.5× rows are there to price what happens if someone insists), so a probe that claimed
    product eligibility would be refused before it could be measured, and the refusal would
    be about the wrong thing.
    """
    parent_line = f'parent = "{face.parent}"\n' if face.parent else ""
    region_rows = 20 - 2
    licence = ", ".join(f'"{row}"' for row in face.licence)
    return f"""[meta]
screen_tiles = [30, 20]
map_tiles = [32, 32]
tile_px = 8
pool_slots = 512
resident_scene_max = 3

[[face]]
id = "probe"
file = "{face.file}"
raster = {raster}
product_face = false           # a measurement of a raster the product may not use — see above
licence = [{licence}]
{parent_line}
[[variant]]
id = "P"
product = false
tile_budget = 512
image_tiles = 256            # 128x128: the first term of every row of the table
bar_cells = 16               # declared, unused — see the comment above
chrome_art_tiles = 0         # no chrome term: the sum is the two terms the table prices
glyph_atlas = "static"

[[variant.palette]]
role = "text"
banks = [0]
[[variant.palette]]
role = "chrome"
banks = [1, 2]
[[variant.palette]]
role = "art"
banks = [3, 4, 5, 6]
[[variant.palette]]
role = "viz"
banks = [7, 8, 9, 10]
[[variant.palette]]
role = "game"
banks = [11, 12, 13, 14, 15]

[[variant.band]]
id = "chrome"
rows = 2
always = true
[[variant.band]]
id = "scene"
rows = {region_rows}
always = false

[[variant.scene]]
name = "art"
phase = "2b"
band = "scene"
kind = "art"
resident = false
min_boxes = 1
[[variant.scene]]
name = "list"
phase = "2b"
band = "scene"
kind = "text"
resident = false
min_visible_rows = 4
[[variant.scene]]
name = "blank"
phase = "2a"
band = "scene"
kind = "fill"
resident = true

[[variant.element]]
id = "art"
kind = "art"
scene = "art"
band = "scene"
x0 = 0
y0 = 0
x1 = 29
y1 = {region_rows - 1}

[[variant.element]]
id = "list"
kind = "text"
scene = "list"
band = "scene"
x0 = 0
y0 = 0
x1 = 29
y1 = 15                      # 16 tile rows = 128 px: the region the column names
face = "probe"
row_px = {row_px}
"""


def subset_reproduces(face: FaceSpec) -> tuple[bool, str]:
    """Re-run the face's cut from its declared parent and compare bytes.

    §5.5a's step 1 is a `pyftsubset` cut of a tracked parent, so the tracked file
    is build output that was checked in — and build output that cannot be rebuilt
    is a file whose provenance nobody can argue with. The codepoints re-subset
    with are the ones the tracked cut itself maps, so the leg assumes nothing about
    what an "ASCII cut" means; the flags are the ones `assets/fonts/README.md`
    records, because a subsetter's output is only reproducible under the flags that
    made it.
    """
    if TTFont is None:
        return False, "fontTools is not importable"
    parent = Path(face.parent) if Path(face.parent).is_absolute() else REPO / Path(face.parent)
    cut = face.path()
    codepoints = sorted(c for c in TTFont(str(cut)).getBestCmap() if c)
    unicodes = ",".join(f"U+{code:04X}" for code in codepoints)
    with tempfile.TemporaryDirectory(prefix="album-layout-subset-") as tmpdir:
        rebuilt = Path(tmpdir) / "rebuilt.ttf"
        proc = subprocess.run(
            ["pyftsubset", str(parent), f"--unicodes={unicodes}", f"--output-file={rebuilt}",
             "--layout-features=", "--no-hinting", "--desubroutinize"],
            capture_output=True, text=True,
        )
        if proc.returncode:
            detail = (proc.stderr.strip().splitlines() or ["pyftsubset failed"])[-1]
            return False, detail[:120]
        tracked_sha = hashlib.sha256(cut.read_bytes()).hexdigest()
        rebuilt_sha = hashlib.sha256(rebuilt.read_bytes()).hexdigest()
    return tracked_sha == rebuilt_sha, f"tracked {tracked_sha[:12]}, rebuilt {rebuilt_sha[:12]}"


def _fixture_text(mutations: tuple[str, str] | None) -> str:
    if mutations is None:
        return FIXTURE_BASE
    old, new = mutations
    if old not in FIXTURE_BASE:
        raise AssertionError(f"fixture leg is stale: {old!r} is not in the fixture spec")
    return FIXTURE_BASE.replace(old, new, 1)


def _write_spec(directory: Path, text: str, name: str = "layout_fixture.toml") -> Path:
    path = directory / name
    path.write_text(text + "\n")
    return path


def layout_self_test() -> int:
    """`--layout-self-test`: the register's legs, asserted by id (§9.0).

    AP-3's bake-leg rule, applied to this phase: each of §9.0's nine refusals is
    watched to fire, by id, on a spec written to be wrong about it — not that an
    error of some kind came out. Two of those rows need inputs 2a does not have
    yet, and §13's own rule is to name them and say why rather than let a count
    be a claim.
    """
    failures: list[str] = []
    print("album-layout-self-test: §9.0's register, legs asserted by id")
    print(f"  {'refusal id':28}  {'exercised by':58}  outcome")
    fired: set[str] = set()

    with tempfile.TemporaryDirectory(prefix="album-layout-self-test-") as tmpdir:
        tmp = Path(tmpdir)
        # The untracked-face leg needs a real file that git does not know about.
        untracked = REPO / "assets/fonts/layout-self-test-untracked.ttf"
        shutil.copy2(REPO / FIXTURE_FACE_FILE, untracked)
        try:
            for refusal_id, how, mutations in FIXTURE_LEGS:
                fixture = LayoutSpec(_write_spec(tmp, _fixture_text(mutations)))
                try:
                    derive_variant(fixture, fixture.variants[0])
                    note = "DID NOT FIRE"
                    failures.append(f"{refusal_id}: {how} — no refusal at all")
                except Refusal as exc:
                    if exc.rule != refusal_id:
                        note = f"WRONG REFUSAL ({exc.rule})"
                        failures.append(f"{refusal_id}: {how} — got {exc.rule}: {exc.detail}")
                    else:
                        note = "fired ✓"
                        fired.add(refusal_id)
                print(f"  {refusal_id:28}  {how:58}  {note}")
        finally:
            untracked.unlink(missing_ok=True)

    # The two legs that are calls rather than specs.
    spec = LayoutSpec(DEFAULT_LAYOUT_SPEC)
    # The one register row whose input is a *box* rather than a spec-wide structural
    # fault: `title-fit`, reached by narrowing a declared title element to one tile column
    # and asking for a title. Note what is *not* in this list — a long title on a real
    # variant, which is elided and printed (§5.5: an album decision); the legs for that are
    # in the reproduction section below, where the negative direction of this row is
    # asserted too.
    def title_box_narrower_than_the_marker() -> None:
        with tempfile.TemporaryDirectory(prefix="album-title-fit-") as d:
            tiny = LayoutSpec(_write_spec(Path(d), _fixture_text(TITLE_BOX_ONE_COLUMN),
                                         "title-fit.toml"))
        derive_variant(tiny, tiny.variants[0]).title_render("a title of any length at all")

    for refusal_id, how, call in (
        ("layout-variant-unknown", "asking for a variant the spec does not declare", lambda: spec.variant("Z")),
        ("bar-cells-disagree", "three artifacts carrying three different cell counts", lambda: assert_bar_grid_agrees(20, 32, 20, "the fixture's [layout]")),
        ("title-fit", "a title element one tile column wide: no rendering, elided or not", title_box_narrower_than_the_marker),
    ):
        try:
            call()
            note = "DID NOT FIRE"
            failures.append(f"{refusal_id}: {how} — no refusal at all")
        except Refusal as exc:
            if exc.rule != refusal_id:
                note = f"WRONG REFUSAL ({exc.rule})"
                failures.append(f"{refusal_id}: {how} — got {exc.rule}: {exc.detail}")
            else:
                note = "fired ✓"
                fired.add(refusal_id)
        print(f"  {refusal_id:28}  {how:58}  {note}")

    missing = [row[0] for row in LAYOUT_REGISTER if row[0] not in fired]
    print(f"  register rows fired: {len(fired)} of {len(LAYOUT_REGISTER)}")
    for row in missing:
        print(f"  {row:28}  DID NOT FIRE — this tool's own error path is unproven")
        failures.append(f"{row}: never exercised")
    print(f"  named and skipped (not counted, §9.0's note under its register):")
    print(f"    {ART_ROW_NOT_FIREABLE_YET[0]}: {ART_ROW_NOT_FIREABLE_YET[1]}")
    print(f"      why not fired here: {ART_ROW_NOT_FIREABLE_YET[2]}")

    # -- the two budget rows 2b PR-(1) re-homed from Phase 3 ---------------------
    #
    # Call legs rather than spec mutations, because neither input is a field of
    # `layout_spec.toml`: `over-budget` reads an album's ALBM1 bytes and the cart size,
    # and `over-track-count` reads the track list. Both directions are checked — the
    # refusal fires by name, and the same arithmetic at a cart wide enough does not —
    # because a row that refused every album would satisfy the first direction and mean
    # nothing.
    budget_fired: set[str] = set()
    facts_a = derive_variant(spec, spec.variants[0])

    def cart_budget_overflow() -> None:
        rom_budget(facts_a, albm1_bytes=400 * 1024, audio_bytes=380 * 1024, cart_bytes=512 * 1024)

    def cart_budget_fits() -> dict:
        return rom_budget(facts_a, albm1_bytes=200 * 1024, audio_bytes=180 * 1024,
                          cart_bytes=4 * 1024 * 1024)

    def track_ceiling_refused() -> None:
        text = _write_config(budget_tmp / "tracks" / "album.toml").read_text()
        stanzas = "\n".join(
            f'[[track]]\nfile = "assets/sound/track{i:02d}.wav"\ntitle = "T{i}"\n'
            for i in range(MAX_TRACKS)
        )
        path = budget_tmp / "tracks" / "album.toml"
        path.write_text(text + "\n" + stanzas)
        Config(path)

    def track_ceiling_fits() -> None:
        Config(_write_config(budget_tmp / "tracks-ok" / "album.toml"))

    budget_tmp = Path(tempfile.mkdtemp(prefix="album-budget-legs-"))

    for refusal_id, how, call in (
        ("over-budget", f"code reserve + ALBM1 400 KiB (audio inside it) on a 512 KiB cart at variant {facts_a.id}",
         cart_budget_overflow),
        ("over-track-count", f"{MAX_TRACKS + 1} tracks in one config (ALBM1 carries {MAX_TRACKS})",
         track_ceiling_refused),
    ):
        try:
            call()
            note = "DID NOT FIRE"
            failures.append(f"{refusal_id}: {how} — no refusal at all")
        except Refusal as exc:
            if exc.rule != refusal_id:
                note = f"WRONG REFUSAL ({exc.rule})"
                failures.append(f"{refusal_id}: {how} — got {exc.rule}: {exc.detail}")
            else:
                note = "fired ✓"
                budget_fired.add(refusal_id)
        print(f"  {refusal_id:28}  {how:58}  {note}")
    budget_missing = [row[0] for row in BUDGET_REGISTER if row[0] not in budget_fired]
    print(f"  budget rows fired: {len(budget_fired)} of {len(BUDGET_REGISTER)}")
    for row in budget_missing:
        print(f"  {row:28}  DID NOT FIRE — this tool's own error path is unproven")
        failures.append(f"{row}: never exercised")
    # The negative directions, so neither row is satisfied by a tool that refuses
    # everything: total is exactly the three addends, and a legal cart does not refuse.
    fits = cart_budget_fits()
    print(f"  {'over-budget, negative':28}  {'a 4 MiB cart holds a 200 KiB ALBM1':58}  "
          f"{'no refusal' if fits['verdict'] == 'fits' else 'FAIL'}")
    if fits["verdict"] != "fits":
        failures.append("over-budget: a cart wider than the addends was still refused")
    try:
        track_ceiling_fits()
        note = "no refusal"
    except Refusal as exc:
        note = f"WRONG REFUSAL ({exc.rule})"
        failures.append(f"over-track-count negative: a legal track list was refused ({exc.rule})")
    print(f"  {'over-track-count, negative':28}  {'a one-track config':58}  {note}")

    # The scene schema itself, self-tested: the two new fields are required, the kind
    # vocabulary is closed, and the retired count is no longer a key. The live spec is the
    # negative control for all of this — it derives — so a tool that refused every scene
    # would fail the register and reproduction legs above.
    schema_fired = 0
    for how, mutation in (
        ("a scene kind outside §5.2's closed vocabulary",
         ('name="art", phase="2b", band="scene", kind="art"',
          'name="art", phase="2b", band="scene", kind="mosaic"')),
        ("a scene that declares no `resident` flag",
         ("resident=true, min_boxes=1", "min_boxes=1")),
        ("the retired variant-level `resident_scenes` count",
         ("tile_budget = 512", "tile_budget = 512\nresident_scenes = 1")),
        ("a scene sharing an image with a scene that does not exist",
         ('shares_image="art"', 'shares_image="no_such_scene"')),
    ):
        try:
            fixture = LayoutSpec(_write_spec(budget_tmp, _fixture_text(mutation), "schema.toml"))
            derive_variant(fixture, fixture.variants[0])
            note = "DID NOT FIRE"
            failures.append(f"scene schema: {how} — no refusal at all")
        except Refusal as exc:
            if exc.rule != "spec":
                note = f"WRONG REFUSAL ({exc.rule})"
                failures.append(f"scene schema: {how} — got {exc.rule}: {exc.detail}")
            else:
                note = "fired ✓"
                schema_fired += 1
        print(f"  {'scene schema':28}  {how:58}  {note}")
    print(f"  scene schema legs fired: {schema_fired} of 4")
    if schema_fired != 4:
        failures.append("scene schema: a required field or the closed kind vocabulary is not asserted")
    shutil.rmtree(budget_tmp, ignore_errors=True)

    # -- the reproduction legs: this tool must reproduce the plan's arithmetic -------
    #
    # Every row prints where its expected number comes from, because that is the question
    # D9 asks. `plan` = a figure ALBUM-PLAYER.md prints. `spec` = a box or field
    # layout_spec.toml declares, for which the plan prints no figure at all — and those
    # rows compute the expected value a *second* way, from the raw spec rather than from
    # the derived object, so the comparison still runs through two paths instead of one
    # path twice. A leg that agrees with itself is not evidence.
    legs = 0

    def leg(what: str, doc_cell: str, got_cell: str, ok: bool, source: str) -> None:
        nonlocal legs
        legs += 1
        print(f"  {what:56} {doc_cell:>15}  {got_cell:>15}  {'ok' if ok else 'FAIL'}   <- {source}")
        if not ok:
            failures.append(f"{what}: expected {doc_cell} ({source}), this tool derives {got_cell}")

    def note(what: str, got_cell: str, why: str) -> None:
        print(f"  {what:56} {'-':>15}  {got_cell:>15}  .      <- {why}")

    def face_at(face_id: str, raster: int) -> FaceSpec:
        """The same declared face, asked at another raster.

        §5.1's tables ask about sizes the spec does not declare, and the answer has to
        come from the outlines at that size — so the face is rebuilt with a different
        `raster` and everything else copied verbatim, `product_face` included: nothing about
        the measurement should depend on which field the spec happened to carry. Whether the
        probe that uses the rebuilt face claims product eligibility is the probe's business,
        and `_probe_spec_text` says why it does not.
        """
        face = faces[face_id]
        if face.raster == raster:
            return face
        return FaceSpec(
            {
                "id": face.id, "file": face.file, "raster": raster,
                "product_face": face.product_face, "licence": list(face.licence),
                "parent": face.parent,
            },
            face.index,
        )

    def agree(what: str, fn) -> bool:
        """True when `fn` refuses nothing — the negative direction of a register row.

        Used only where the point is that a correct triple does *not* get refused. The
        positive direction is the register leg above, which is what makes this one mean
        anything rather than a tautology.
        """
        try:
            fn()
            return True
        except Refusal:
            return False

    def nodiv_grid_constant() -> int | None:
        """The bar grid `crates/gba-playback/src/nodiv.rs` still owns as a constant.

        D9 item 1 says this must stop being a source: the grid belongs to the selected
        variant, and a shared crate that carries one becomes the thing a manifest is
        validated against by accident. Reading the file rather than knowing the answer is
        the point — a leg that cannot tell whether the constant is gone is not a leg.

        So the read is *not* guarded: an unreadable `nodiv.rs` raises and the self-test
        dies with a traceback naming the file, which is honest. A `try/except OSError`
        here returned `None`, and `None` is the value that says "no constant, leg green" —
        the one way this check could pass without looking.
        """
        source = (REPO / "crates/gba-playback/src/nodiv.rs").read_text()
        for line in source.splitlines():
            if "pub const BAR_CELLS" in line and "=" in line:
                try:
                    return int(line.split("=", 1)[1].split(";")[0].replace("u32", "").strip(), 0)
                except ValueError:
                    continue
        return None

    def list_element_of(facts: VariantFacts):
        """The variant's track-list text element, found by what it is for.

        Looking it up by id would be a name, not a role; the role is what §5.1's column
        describes ("text rows in a 128 px region"). A variant with no list element is
        `None` here and its legs say so.
        """
        for scene in facts.variant.scenes:
            if scene.name != "list":
                continue
            for element in facts.elements:
                if element.kind == "text" and element.scene == scene.name:
                    return element
        return None

    print()
    print("reproduction legs: the plan's arithmetic, re-derived from the files")
    print(f"  {'what':56} {'ALBUM-PLAYER.md':>15}  {'derived':>15}  verdict   <- the expected number's source")

    faces = {face.id: face for face in spec.faces}
    facts_by_variant = {variant.id: derive_variant(spec, variant) for variant in spec.variants}

    # §5.1's measured table. The plan says of it that its numbers "were recomputed from
    # the files, in one pass, with the method stated, and every derived number in this
    # section follows from that one table" — so re-measuring them here checks both sides
    # at once: if the table and the generator disagree, one of them is wrong, and neither
    # gets to assume which. (One of them was: the first draft of these legs expected
    # variant A's list at the ark face's capacities and the tool answered with the fusion
    # face's. The spec was right, the legs were wrong, and the legs are what changed.)
    print()
    print("  §5.1's measured table, read from the .ttf outlines and hhea rather than from the table")
    for face_id, raster, label, grid, line_box, min_row, ink, avg, cells, upper, cap in MEASURED_FACE_TABLE:
        metrics = FaceMetrics(face_at(face_id, raster), spec.tile_px)
        want = (line_box, min_row, ink, avg, cells, upper, cap)
        got = (
            metrics.line_box_px, metrics.min_row_px, round(metrics.ink_px, 1),
            round(metrics.avg_advance_px, 2), metrics.cells_in_240px,
            metrics.cells_in_240px * metrics.tile_rows_per_line,
            metrics.outlined * metrics.tile_rows_per_line,
        )
        names = ("line box", "min row_px", "tallest ink", "mean advance", "cells in 240 px",
                 "slots per text row (upper)", "slots per text row (cap)")
        bad = [name for name, a, b in zip(names, want, got) if a != b]
        grid_ok = (grid is None and metrics.off_grid_outlined == 0) or (
            grid is not None and metrics.off_grid_outlined == grid)
        leg(f"{Path(faces[face_id].file).name} @ {raster} px ({label})",
            "/".join(str(x) for x in want), "/".join(str(x) for x in got),
            not bad and grid_ok,
            "plan §5.1's measured table; its “grid-aligned?” cell prints an ink count, “94 of 94”"
            + (f"; off by: {', '.join(bad)}" if bad else "")
            + ("" if grid_ok else f"; grid: the plan prints {grid}, this tool finds {metrics.off_grid_outlined} of {metrics.outlined} ink-carrying glyphs off-grid"))
        note(f"    …the advance half of the same question at {raster} px",
             f"{metrics.off_grid}/{metrics.mapped_count}",
             "not a column of the plan's table. This tool also refuses a fractional *advance* at a "
             "declared raster, which an ink count cannot see: a space carries no ink, so §5.1's 94-of-94 "
             "denominator is outlined glyphs and the declared-raster check has to use the mapped count")

    # §5.1's interaction table, exercised as specs rather than transcribed: every row
    # becomes a probe variant carrying that face at that pitch in a 128 px region, and the
    # plan's ✔/✘ marks come out of this tool's own `image-tiles-over-budget` gate asked at
    # a 512-slot budget — the same refusal the register rows above watch fire.
    print()
    with tempfile.TemporaryDirectory(prefix="album-layout-probes-") as tmpdir:
        tmp = Path(tmpdir)

        def derive_probe(path: Path) -> tuple[VariantFacts | None, str]:
            """Derive a probe, and say which refusal it raised instead, if any.

            The table's terms are budget-independent (rows come from a box, the atlas term
            from a face); only its ✔/✘ column is a question about a budget. So a probe is
            derived twice: at a budget wide enough to see every term, and at the plan's 512
            to ask the plan's question. Answering both with one derive would mean the rows
            that the plan marks ✘ could not be measured at all — and reproducing only the
            rows that pass is how a table like this gets rubber-stamped.
            """
            probe_spec = LayoutSpec(path)
            try:
                return derive_variant(probe_spec, probe_spec.variants[0]), ""
            except Refusal as exc:
                return None, exc.rule

        print()
        print("  §5.1's [meta] pool guard, asked at the values it exists for")

        def probe_meta_pool(pool: int) -> str:
            """`[meta] pool_slots = <pool>` as the spec parser answers it: accepted, or refused."""
            try:
                LayoutSpec(_write_spec(tmp, _fixture_text(("pool_slots = 512", f"pool_slots = {pool}")),
                                       f"meta-pool-{pool}.toml"))
            except Refusal as exc:
                return f"refused: {exc.rule}"
            return "accepted"

        # §5.1's two pools, asked at the values the guard exists for. A refusal nothing
        # fires is not a guard, and this one had no leg at all: the case its own comment
        # names (1,024 — the two pools counted as one) sat on the wrong side of a `>`.
        for pool, expect in ((512, "accepted"), (256, "accepted"), (1024, "refused: spec")):
            got = probe_meta_pool(pool)
            leg(f"[meta] pool_slots = {pool} against §5.1's two pools", expect, got, got == expect,
                "plan §5.1: agb 0.25's regular pool is 512 4bpp slots and the affine pool is a "
                "*different* 510, so 512 is the largest number a spec may claim for the pool this "
                "app draws from and anything below it is a smaller claim, not a wrong one. 1,024 is "
                "the two read as one — the error §5.1's rewrite exists to prevent — and it reaches "
                "`POOL_SLOTS` in the generated module and the audit line that prints the pool, so it "
                "has to refuse here")

        print()
        print("  §5.1's interaction table: each row becomes a probe spec this tool derives")
        for face_id, raster, row_px, rows, upper, cap, art256, art300 in INTERACTION_TABLE:
            name = f"{face_id} @ {raster} px, pitch {row_px}"
            measured = FaceMetrics(face_at(face_id, raster), spec.tile_px)
            text = _probe_spec_text(face_at(face_id, raster), raster, row_px)
            wide_text = text.replace("tile_budget = 512", "tile_budget = 4096")
            facts, fired_wide = derive_probe(
                _write_spec(tmp, wide_text, f"probe-{face_id}-{raster}-{row_px}-wide.toml"))
            element = list_element_of(facts) if facts else None
            built = facts is not None and element is not None
            if built:
                rows_got = element.text_rows
                upper_got, cap_got, art_got = facts.text_upper, facts.text_distinct, facts.pool_image
                terms_src = (f"derived through a probe variant carrying that face at that pitch: the row count "
                             f"comes from the 128 px box the plan's column names, the per-row cell count from "
                             f"the face's advances, and both tile terms from the plan's own definitions of the "
                             f"two bounds (R14's distinction, priced both ways)")
            else:
                # This row cannot be built at all: the generator refuses a declared face raster
                # that is not a whole multiple of the file's design size. That is not the leg
                # failing — it is the row's own conclusion, and the plan says so in the row
                # itself ("a half-scale raster fits the pool and puts every glyph off-grid"). The
                # numbers are still checkable: they are arithmetic on the measurements this tool
                # printed for that face at that size above, which is how the plan got them too.
                rows_got = 128 // row_px
                upper_got = rows_got * measured.cells_in_240px * measured.tile_rows_per_line
                cap_got = measured.outlined * measured.tile_rows_per_line
                art_got = (128 // spec.tile_px) ** 2
                terms_src = (f"not built: the generator refuses a face raster that is not a whole multiple of "
                             f"the design size this file measures ({measured.design_px} px) — §5.5's rule, and "
                             f"this row's own conclusion. Its terms are arithmetic on the measurements printed "
                             f"above for that face at that size: rows from a 128 px box (128 // {row_px}), "
                             f"cells from the mean advance ({measured.cells_in_240px} in 240 px), the atlas "
                             f"term from the ink-carrying glyph count ({measured.outlined} x "
                             f"{measured.tile_rows_per_line} tile rows)")
            leg(f"{name}: rows in 128 px, then its two tile terms",
                f"{rows} / {upper} / {cap}", f"{rows_got} / {upper_got} / {cap_got}",
                (rows_got, upper_got, cap_got) == (rows, upper, cap), terms_src)
            leg(f"{name}: the 128x128 art term that column adds", "256", str(art_got), art_got == 256,
                ("the probe declares a 16x16-tile box and the image term is taken at the *derived* largest "
                 "square (§7, §5.0's `image_tiles` ceiling): 16x16 = 256, the plan's figure for that column "
                 "reached as a derivation" if built else
                 f"arithmetic on the plan's own region figure at this spec's tile size: 128 px / "
                 f"{spec.tile_px} px = {128 // spec.tile_px} tiles a side, squared = 256. Not reached through "
                 "a variant, because this row's raster is refused before any variant exists"))
            total, fits, _ = art256
            if built:
                # NB: `fired` is the register's set of refusal ids and this loop must not
                # rebind it — an earlier cut named the local `fired`, which turned the
                # summary's `len(fired)` into the length of one id string.
                at512_facts, fired_rule = derive_probe(
                    _write_spec(tmp, text, f"probe-{face_id}-{raster}-at-512.toml"))
                shown = (at512_facts or facts).pool_distinct
                verdict = not fired_rule
                sum_src = ("plan §5.1's mark for that row, derived as raise/no-raise at "
                           "`image-tiles-over-budget` — the gate its ✔/✘ column is asking about, not a "
                           "transcription of the column") + (f" (fired: {fired_rule})" if fired_rule else "")
            else:
                shown = cap_got + art_got
                verdict = shown <= 512
                sum_src = ("arithmetic on the two terms above, asked at the plan's 512. The budget gate never "
                           "sees this variant, because its raster is refused first — which is the point of the "
                           "row, and what the register row above watches fire")
            leg(f"{name}: the sum at a 512-slot budget",
                f"{total} {'Y' if fits else 'N'}", f"{shown} {'Y' if verdict else 'N'}",
                shown == total and verdict == fits, sum_src)
            if not built:
                leg(f"{name}: can this row be built as a variant at all?",
                    "no — the plan rules it out for the product", f"refused: {fired_wide}",
                    fired_wide == "pitch-not-tileable",
                    "§5.5's rule on declared rasters, which this tool enforces and the plan endorses; the "
                    "table prices the row anyway because its whole purpose is to show that a half-scale raster "
                    "is cheap on tiles and ruinous on ink")
            total2, fits2, _ = art300
            leg(f"{name}: the same text term at the plan's second art term (160x120)",
                f"{total2} {'Y' if fits2 else 'N'}",
                f"{cap + 300} {'Y' if cap + 300 <= 512 else 'N'}",
                cap + 300 == total2 and (cap + 300 <= 512) == fits2,
                "priced against this tool's derived text term, not derived end to end: 300 tiles is not a "
                "box any variant derives — 160x120 is not a square and §7's boxes are squares — so that "
                "column is an illustration, and the honest leg is the arithmetic that joins it")
            leg(f"{name}: is a 160x120 box reachable as a derived box?",
                "illustration", "no: not a square", math.isqrt(300) ** 2 != 300,
                "arithmetic on the plan's own figure: it derives squares (§7), so the 300-tile term cannot "
                "come from a variant, and the row above has to say which half of it it derived")
            if art256[2] is not None:
                # The plan's parenthetical: the same variant measured at the *upper* bound —
                # one allocation per character cell, which is what today's renderer does — is
                # the thing §5.1's warning is about. Derived by re-declaring the probe with
                # `glyph_atlas = "per-cell"`, at a budget wide enough to read the number off.
                per_cell_text = wide_text.replace('glyph_atlas = "static"', 'glyph_atlas = "per-cell"')
                wide_facts, fired_wide_pc = derive_probe(
                    _write_spec(tmp, per_cell_text, f"probe-{face_id}-{raster}-per-cell-wide.toml"))
                leg(f"{name}: the same probe measured at the upper bound",
                    f"upper {art256[2]} N", f"{wide_facts.pool_upper} {'N' if wide_facts.pool_upper > 512 else 'Y'}",
                    wide_facts.pool_upper == art256[2] and wide_facts.pool_upper > 512,
                    f"plan §5.1's “(upper {art256[2]:,}) ×” for that row — measured at the bound its own "
                    "warning names (`RegularBackgroundTextRenderer`, one allocation per character cell)")
                tight = LayoutSpec(_write_spec(
                    tmp, text.replace('glyph_atlas = "static"', 'glyph_atlas = "per-cell"'),
                    f"probe-{face_id}-{raster}-per-cell-at-512.toml"))
                try:
                    derive_variant(tight, tight.variants[0])
                    refused = False
                except Refusal as exc:
                    refused = exc.rule == "image-tiles-over-budget"
                leg(f"{name}: that upper bound at 512 is refused, and says which bound",
                    "N", "image-tiles-over-budget" if refused else "did not fire", refused,
                    "plan §5.1's “flags the atlas variant rather than refusing it”, which is why the "
                    "refusal names the bound it is complaining about — see the register row above")

    # The two declared variants, against §5.0's mock rows and §5.1's measured columns.
    for variant_id, facts in facts_by_variant.items():
        variant = spec.variant(variant_id)
        a_variant = variant_id == "A"
        element = list_element_of(facts)
        print()
        print(f"  variant {variant_id}: what the spec declares, against the plan's rows for it")
        region_rows = variant.bands[-1].rows
        leg(f"variant {variant_id}: scene region",
            f"{spec.screen_tiles[0] * spec.tile_px}x{region_rows * spec.tile_px} px",
            f"{facts.region[2]}x{facts.region[3]} px",
            facts.region == (spec.screen_tiles[0], region_rows, spec.screen_tiles[0] * spec.tile_px,
                             region_rows * spec.tile_px)
            and region_rows == (17 if a_variant else 16) and variant.bands[-1].id == "scene",
            f"plan §5.0's {variant_id} row: a {region_rows}-row scene region. The rows are the plan's; "
            "the pixels are this tool's multiplication, which is what §5.0's own comment does")
        leg(f"variant {variant_id}: the declared band stack tiles the screen",
            str(spec.screen_tiles[1]), str(sum(band.rows for band in variant.bands)),
            sum(band.rows for band in variant.bands) == spec.screen_tiles[1],
            "`bands-not-tiling`'s own question asked of the declared stack; the plan describes the stack "
            "row by row and never prints its sum, so the sum is derived here")
        side = 15 if a_variant else 16
        leg(f"variant {variant_id}: the largest art box this variant may draw",
            f"{side * 8} px / {side * side} tiles",
            f"{facts.art_ceiling.width_px if facts.art_ceiling else 0} px / "
            f"{facts.art_ceiling.tiles if facts.art_ceiling else 0} tiles",
            facts.art_ceiling is not None and facts.art_ceiling.side == side
            and facts.art_ceiling.tiles == side * side and facts.pool_image == side * side,
            f"plan §5.0's {variant_id} row. The declared `image_tiles` {variant.image_tiles} is what stops "
            f"the {variant.bands[-1].rows}-row region from holding a bigger square — §5.0's own "
            "scene_region != image_tiles distinction, and the reason the spec's comment says “what stops "
            "both is the pool”")
        leg(f"variant {variant_id}: every legal art box is derived, not hand-listed",
            f"{side} boxes, 1..{side} tiles a side", f"{len(facts.art_boxes)} boxes",
            len(facts.art_boxes) == side and facts.art_boxes[-1] is facts.art_ceiling,
            "§7: the box list is derived from the region and the budget; 2a draws one placeholder at the "
            "largest size that survives every album, and 2b's projection is printed, not claimed")
        list_cells, list_upper = (52, 832) if a_variant else (44, 704)
        face_used = element.metrics.face.file if element else "none"
        leg(f"variant {variant_id}: cells per row in the list",
            str(list_cells), str(element.cells_per_row if element else 0),
            element is not None and element.cells_per_row == list_cells,
            f"plan §5.1's measured row for the face this variant declares its list in: "
            f"{Path(face_used).name if element else 'no list element'}")
        leg(f"variant {variant_id}: text rows in its list region", "8",
            str(element.text_rows if element else 0),
            element is not None and element.text_rows == 8,
            f"plan §5.1's interaction table (“text rows in a 128 px region | 8 rows”). This variant's "
            f"region is {facts.region[3]} px and its list element spans {element.h if element else 0} px "
            f"of it, which at the declared {element.row_px if element else 0} px pitch floors to the same "
            "8 rows: the plan's 128 px figure and this variant's taller region meet at 8, and neither "
            "number is copied into the other")
        list_own_upper = (element.text_rows * element.cells_per_row * element.metrics.tile_rows_per_line
                          if element else 0)
        leg(f"variant {variant_id}: both pool terms for that list element",
            f"upper {list_upper} / cap 188", f"upper {list_own_upper} / cap {facts.text_distinct}",
            list_own_upper == list_upper and facts.text_distinct == 188,
            "plan §5.1's two columns for the face this variant draws its text in: the upper bound (one "
            "allocation per character cell) and the atlas cap (“≤ 188”, the ink-carrying glyphs of the cut "
            "times the tile rows a line box spans). The two are different numbers by R14's argument, so "
            "they are computed by two different paths here — cells x rows x tile-rows, and glyph count x "
            "tile-rows")
        note(f"variant {variant_id}: the same sum across every text element in the variant",
             f"upper {facts.text_upper} / atlas {facts.atlas_tiles}",
             "not the plan's figure and not a disagreement with it: §5.1's columns are per face and per "
             "element, while a variant's total is the sum over all of its text elements — the clock and the "
             "key labels are text too. §5.0's mock row prices that second thing, and it is the number the "
             "budget gate actually compares; printing both is what keeps the two from being confused for "
             "each other")
        if a_variant:
            leg("variant A: title-fit box", "none (art/bar)", "none",
                facts.title_element is None,
                "plan §5.0's A row: a title band carrying no text line. The absence is the thing to "
                "assert — with no declared box this tool must say there is no box rather than borrow a face")
        else:
            box_px = (facts.title_element.spec.x1 - facts.title_element.spec.x0 + 1) * spec.tile_px
            leg("variant B: the title-fit box's width, from the raw declared element",
                f"{box_px} px", f"{facts.title_element.w} px", facts.title_element.w == box_px,
                "spec: B's [[element]] title_text (x0=0..x1=19 = 20 columns), whose comment cites §5.5. "
                "The plan prints no figure for this width on purpose — D9's point is that the box is "
                "declared and measured against, not quoted. §5.0's B mock priced an 80x16 px glyph box, "
                "which is the box the *current* 2a text module draws; this variant declares a 160 px one, "
                "and the fit rule is the same measurement at either width")
            max_advance = facts.title_element.metrics.max_advance_px
            # The measurement itself has no figure in the plan to compare against — that is
            # the point of D9 item 2 — so the leg that is a *check* is the one that tests it
            # against the bound the plan does print: §5.1's tallest-advance column is an upper
            # bound per glyph, so a measured width must never exceed chars x max_advance. If a
            # per-glyph sum ever exceeds the per-glyph maximum, one of the two is wrong.
            for probe_text in ("Bioluminescent", "Bioluminescent Overdriven",
                                "An unreasonably long album title that will not fit anywhere"):
                fits_it, width_px, unwritten, box = facts.title_fit(probe_text)
                bound_px = len(probe_text) * max_advance
                leg(f"variant B: measured width of {probe_text!r} against the plan's per-glyph bound",
                    f"<= {bound_px:.1f} px", f"{width_px:.1f} px", width_px <= bound_px,
                    f"arithmetic on plan §5.1's tallest-advance column ({max_advance} px) times the "
                    f"{len(probe_text)} code points of that title — a bound, not the string's width. The "
                    "plan itself warns that a mean advance is not a capacity bound, so this is the honest "
                    "two-path check: a per-glyph sum that beat the per-glyph maximum would be a measurement bug")
                leg(f"variant B: does {probe_text!r} fit the declared {box} px box?",
                    "measured, not counted", f"{width_px:.1f} px {'fits' if fits_it else 'does not fit'}",
                    fits_it is (width_px <= box) and box == box_px,
                    f"no plan figure for this string, so the leg tests the rule rather than a number: fit "
                    f"is the sum of {Path(facts.title_element.metrics.face.file).name}'s advances against "
                    f"the declared box, and {len(probe_text)} characters is not a measurement of anything. "
                    "The bake prints a title that does not fit rather than refusing it (§5.5: an album "
                    "decision, not a build failure); a host test is what asserts a shipped album fits")
                # The third value `title_fit` returns is *unmapped code points*, not elided
                # ones, so no leg here may call it elision: §5.5's elision happens at draw
                # time in the text module and is a host-test claim. What this layer can
                # check is the precondition under which `unwritten` would stay empty — that
                # the face's cmap covers the title — and it checks it by two paths, because
                # an all-passing probe set proves nothing on its own.
                direct_units, direct_missing = facts.title_element.metrics.measure_text(probe_text)
                leg(f"variant B: every code point of {probe_text!r} is mapped by the title face",
                    "0 unwritten", f"{len(direct_missing)} unwritten",
                    direct_missing == "" and unwritten == direct_missing,
                    f"spec/measurement: {len(set(probe_text))} distinct code points in that title, asked "
                    "of the declared title face twice — once through `title_fit`'s own path ("
                    "`fits` -> `measure_text`) and once through `measure_text` directly, so a glyph missing "
                    "from the cmap would show as a disagreement between the paths before it showed as a "
                    "blank box. The ark ASCII cut this variant declares is what makes the answer yes; a "
                    "title outside it is not elided here, it is unwritten, and the leg below is the "
                    "negative control that keeps this one from being a tautology")
            unmapped_probe = "\u266b Albums"
            _, probe_missing = facts.title_element.metrics.measure_text(unmapped_probe)
            leg("variant B: a code point outside the cmap is reported, not silently zero-width",
                "\u266b unwritten", f"{probe_missing!r} unwritten",
                probe_missing == "\u266b",
                "arithmetic on the same measurement path: a glyph the face does not carry contributes "
                    "nothing to the width and *is* named in the return value, which is what makes the "
                    "mapping leg above a check rather than a pass-by-default. §5.5 keeps the verdict "
                    "about width separate from the verdict about glyphs for exactly this reason")
            # The device's own measurement path, read back out of the generated module: the table
            # written into `src/layout.rs` must produce the *same* verdict as the Python metrics it
            # was generated from, for a string that fits and one that does not. Without this leg the
            # Rust side's `Face::measure` is only ever exercised by a test that shares its table.
            module_text = layout_rs_text(facts)
            title_face_id = facts.title_element.face
            metrics = facts.title_element.metrics
            table = read_advance_table(module_text, title_face_id, "the generated title face")
            for probe, box in (("Bioluminescent 1", facts.title_element.w),
                               ("An unreasonably long album title that will not fit anywhere",
                                facts.title_element.w)):
                rust_fits, rust_px, rust_missing = measure_from_table(
                    table, probe, metrics.raster, metrics.upem, box)
                bake_fits, bake_px, bake_missing = metrics.fits(probe, box)
                leg(f"variant B: the generated advance table reproduces {probe!r}'s verdict",
                    f"{bake_px:.1f} px {'fits' if bake_fits else 'overflows'}",
                    f"{rust_px:.1f} px {'fits' if rust_fits else 'overflows'}",
                    (rust_fits, round(rust_px, 4), rust_missing) == (bake_fits, round(bake_px, 4), bake_missing),
                    "two paths for one verdict: the left number comes from `FontMetrics.fits` (the "
                    f"tracked .ttf's `hmtx`), the right from the {len(table)}-entry table the generator "
                    "wrote into `src/layout.rs`, parsed back out of that file and summed with the "
                    "device's own inequality (`units × raster ≤ box × upem`). This is what makes "
                    "D9's replacement real: the ROM measures the title, it does not count characters. "
                    "The two probes straddle the declared box — a shipped title that fits and one that "
                    "does not — because a leg whose both halves answer `fits` is satisfied by a "
                    "measurement that always returns true, which is the failure a one-sided leg "
                    "would be")
            leg("variant B: the read-back table covers exactly the face's mapped code points",
                f"{metrics.mapped_count} mapped", f"{len(table)} entries",
                len(table) == metrics.mapped_count
                and set(table) == {ord(c) for c in metrics.advance_units},
                "spec/measurement: the generated table is not allowed to grow or lose a code point "
                "between the font and the device — a missing entry would show as `unwritten` at "
                "runtime and a spurious one would be a glyph this face does not have, and both are "
                "invisible in a generated file a reviewer reads past")

            # -- §5.5's one-line elision, measured against that same declared box -------
            #
            # PR-1 deliberately left this half unpaid (its own comment here said so), so these
            # legs are 2a PR-(2)'s. The rule is stated in §5.5 with two parts — "cut at the last
            # whole glyph that fits plus an ellipsis", and "the track number is never cut" — and
            # each gets a leg that could actually fail. The first is checked by *maximality*, not
            # by the result fitting: an elision that cut two glyphs early also fits its box, and a
            # leg that only checks that would be satisfied by it. The second is checked by asking
            # for a protected prefix and reading it back out of the answer.
            box_px = facts.title_element.w
            over_long = ("Bioluminescent Overdriven: An Unreasonably Long Album Title That "
                         "Will Not Fit This Box")
            renders_whole, whole_short = facts.title_render("Bioluminescent")
            leg("variant B: a title that fits is returned whole, marker untouched",
                "'Bioluminescent' unchanged", f"{'unchanged' if whole_short else 'elided'}",
                renders_whole == "Bioluminescent" and whole_short,
                f"arithmetic on the declared {box_px} px box: that title measures "
                f"{metrics.width_px(renders_whole)[0]:.0f} px, which is inside it, so the elision "
                "must be the identity on it. The fixture's shipped titles are all in this class, "
                "which is why the over-long probe below is synthetic — §13's AP-3 row calls that "
                "out for this row (`title-fit is 2a's box with 2b's fixture`)")
            rendered, whole = facts.title_render(over_long)
            cut_units = metrics.measure_text(rendered)[0]
            leg("variant B: the over-long title comes back elided, and the elided string fits",
                f"<= {box_px} px", f"{metrics.width_px(rendered)[0]:.0f} px", rendered.endswith(ELLIPSIS)
                and metrics.fits_units(cut_units, box_px) and not whole,
                f"arithmetic on the same measurement path: {over_long!r} measures "
                f"{metrics.width_px(over_long)[0]:.0f} px against a {box_px} px box, and the elided "
                "rendering must come in under it with the marker appended — the box is the declared "
                "one (§5.0's element span), not a limit this tool invented (D9's whole point)")
            prefix = rendered[:-len(ELLIPSIS)]
            one_more = prefix + over_long[len(prefix)] + ELLIPSIS
            leg("variant B: the cut is at the last whole glyph that fits, not an earlier one",
                "one more glyph must not fit", f"+{over_long[len(prefix)]!r} -> "
                f"{metrics.width_px(one_more)[0]:.0f} px", not metrics.fits_units(
                    metrics.measure_text(one_more)[0], box_px),
                "the maximality half of §5.5's sentence, checked by measuring the string that the "
                "rule must have rejected: the same cut with one more glyph of the original added. "
                "A leg that only checked that the answer fits is satisfied by an elision that gave "
                "up early; this one is not")
            marker_units = [metrics.advance_units.get(c) for c in ELLIPSIS]
            leg("variant B: the marker is three ASCII periods, and this face carries every one of them",
                "'...' all mapped", f"{ELLIPSIS!r} advances {marker_units}",
                ELLIPSIS == "..." and all(u is not None for u in marker_units),
                "plan §5.5a, which rules this marker rather than U+2026 because a real ellipsis is "
                "a glyph the atlas has to carry: +1 glyph x 2 tile rows = 190 tiles and a distinct "
                "pool cap of 190, moving §5.1's 444/488 to 446/490. Measured here for a second "
                f"reason that figure does not cover: {Path(metrics.face.file).name} has no U+2026 in "
                "its cmap at all, so a real ellipsis at this title face would be *unwritten* as well "
                "as expensive — the leg below is that measurement")
            _, ellipsis_missing = metrics.measure_text("\u2026")
            leg("variant B: the real ellipsis would not draw at this face even if it were paid for",
                "\u2026 unwritten", f"{ellipsis_missing!r} unwritten", ellipsis_missing == "\u2026",
                "arithmetic on the read-back of the same cmap the device is given: the declared "
                "title face is a latin-ASCII cut, and U+2026 is outside it. §5.5a's argument for "
                "three periods is stated in tiles; this is the same answer in glyphs, from the file "
                "rather than from the argument — and the reason the marker must never be written as "
                "a single character in a source file, a config, or a generated string")
            keep = 5
            numbered = f"01 - {over_long}"
            renders_numbered, _ = facts.title_render(numbered, keep=keep)
            leg("variant B: the protected prefix survives the cut (§5.5: the track number is never cut)",
                f"first {keep} code points kept", f"{renders_numbered[:keep]!r} kept",
                renders_numbered.startswith(numbered[:keep]) and renders_numbered.endswith(ELLIPSIS)
                and len(renders_numbered) < len(numbered)
                and metrics.fits_units(metrics.measure_text(renders_numbered)[0], box_px),
                "§5.5's sentence verbatim — \u201cthe track number is never cut, because `NN \u00b7` is the "
                "part a listener uses to ask for the song\u201d — exercised as a `keep` argument, which is "
                "what that sentence becomes in code. 2a's title band passes 0, because `meta.rs` "
                "stores the folded name with no number on it; the scene that composes a numbered row "
                "(2b's list) passes its own prefix length, and this is the leg that says the "
                "mechanism is there for it to use")
            leg("variant B: the separator §5.5's example uses is not a glyph this title face carries",
                "\u00b7 unwritten", f"{metrics.measure_text('\u00b7')[1]!r} unwritten",
                metrics.measure_text("\u00b7")[1] == "\u00b7",
                "a fact about the row above, not a bug in it: §5.5 names `NN \u00b7` with a middle dot, and "
                f"{Path(metrics.face.file).name}'s latin-ASCII cut does not carry U+00B7 (its cmap "
                "stops at U+007E). A scene that composes a numbered row must therefore use an ASCII "
                "separator or name a face that carries the dot; the prefix leg above uses `-` for "
                "exactly that reason. 2b owns the choice, and this leg is what stops it being made "
                "by accident — an unwritten separator draws nothing and changes no width")
            never_refused = agree("a 4,000-character title at B's declared box",
                                  lambda: facts.title_render("x" * 4000))
            leg("variant B: no title, however long, is a refusal at a box this variant really declares",
                "elided, never refused", "elided" if never_refused else "refused", never_refused,
                "the negative direction of §9.0's `title-fit` row: the refusal's input is a box, not "
                "a string, and the register leg that fires it narrows a box to one tile column. "
                "Without this leg the register row would be satisfied by a tool that refuses every "
                "title it has never seen")
        bar_box_px = ((facts.bar_element.spec.x1 - facts.bar_element.spec.x0 + 1) * spec.tile_px
                      if facts.bar_element else 0)
        if a_variant:
            leg("variant A: bar grid as declared vs §6's starting default",
                "18 / 72 px / 4 px", f"{variant.bar_cells} / {bar_box_px} px / {facts.bar_cell_px} px",
                variant.bar_cells == 18 and bar_box_px == 72 and facts.bar_cell_px == 4,
                "plan §6, which names 18 cells in 72 px as A's starting default; the 72 px is what A's "
                "title band element spans")
        else:
            leg("variant B: bar grid from the declared box",
                f"{bar_box_px} px / {variant.bar_cells} cells",
                f"{facts.bar_cell_px} px per cell",
                facts.bar_element is not None and facts.bar_px == bar_box_px
                and bar_box_px == 80 and facts.bar_px % variant.bar_cells == 0 and facts.bar_cell_px == 1,
                "spec: B's [[element]] bar box (x0=20..x1=29 of the title band's lower row = 10 columns = "
                "80 px) and `bar_cells = 80`. The plan quotes no cell count for B — it quotes the floor "
                "and A's default — so this row's expected value comes from the raw element and the "
                "generator's rule that a cell must be a whole pixel. That rule is what turned this "
                "field over: the spec used to say 30 against the same 80 px box and was refused")
        leg(f"variant {variant_id}: bar grid at or above §6's floor", ">= 16", str(variant.bar_cells),
            variant.bar_cells >= BAR_CELLS_FLOOR,
            "plan §6's floor of 16 cells, whose stated reason is precision rather than pixels "
            "(6.2% of the track per +-1 cell)")
        # Two paths for the pattern-tile count. `_pattern_tiles` takes the reachable
        # residues of a cell step modulo the tile; this renders the bar at every fill
        # position and collects the partial tiles that actually appear in a row of them.
        # They agree only if the residue argument describes what gets drawn.
        rendered = set()
        for filled in range(variant.bar_cells + 1):
            for start in range(0, facts.bar_px, spec.tile_px):
                lit = max(0, min(spec.tile_px, filled * facts.bar_cell_px - start))
                if 0 < lit < spec.tile_px:
                    rendered.add("#" * lit + "." * (spec.tile_px - lit))
        leg(f"variant {variant_id}: partial-fill pattern tiles, counted two ways",
            f"{len(rendered)} partial + empty + full", f"{facts.bar_pattern_tiles} derived",
            facts.bar_pattern_tiles == len(rendered) + 2,
            "arithmetic: a row of the bar rendered cell by cell can only show the partial fills its own "
            f"cell step can produce, so the distinct tiles are the partials that appear ({len(rendered)}) "
            "plus the empty and full tiles any bar needs. Counted twice on purpose — a count taken from "
            "the same residues the generator reports would be one path agreeing with itself")
        if a_variant:
            leg("variant A: §6's pattern-tile figure, as the document now states it",
                "3 (2 residues + the full tile)", str(facts.bar_pattern_tiles),
                facts.bar_pattern_tiles == 3,
                "plan §6 as corrected on 2026-11-20: it now states the rule — the residues a cell step "
                "reaches inside an 8 px tile, plus the full tile — and the figure that rule gives for A's "
                "4 px cells, in place of the earlier “~5 extra pattern tiles”. That correction *is* the "
                "disagreement this leg used to record: the estimate was a guess at “a handful”, and a 4 px "
                "step reaches only two interior offsets in an 8 px tile, so the document moved to the "
                "derived figure the way §5.1's table carries its method — not this row keeping a rounder "
                "number alive")
        else:
            note(f"variant {variant_id}: pattern tiles at 1 px cells", str(facts.bar_pattern_tiles),
                 "no plan figure for B, and none needed: 1 px cells make every residue inside an 8 px tile "
                 "reachable, so the same rule gives 8 + 1. Printed to show the derivation runs on both "
                 "grids, not only on the one the plan priced")
        leg(f"variant {variant_id}: the chrome term, declared and derived",
            f"declared {variant.chrome_art_tiles}", f"derived {facts.chrome_derived}",
            facts.chrome_derived == variant.chrome_art_tiles,
            "two paths for one number: the spec declares `chrome_art_tiles = 9` with its enumeration in a "
            "comment (§5.1's band backdrops + the region's opaque backing + §5.3's key slots), and the "
            "generator counts the same things from the declared bands and control slots")
        leg(f"variant {variant_id}: §5.3's control slots",
            "5 keys", f"{len(variant.controls)} declared, {len(facts.control_slots)} inside their slots",
            len(variant.controls) == 5 and len(facts.control_slots) == 5,
            "plan §5.3: the five keys 2a shows. Each control must land inside a declared art slot, which "
            "is what the second number counts; it is also the second path for the chrome term above")

    # --- the bar grid's flow: declared -> reciprocals -> ALBM1 -> generated module ---
    print()
    print("  the bar grid's whole flow (D9 item 1): one declared number, three artifacts that must carry it")
    n_samples = 1_966_080
    for variant_id, facts in facts_by_variant.items():
        variant = spec.variant(variant_id)
        cells = variant.bar_cells
        recip = bar_recip(n_samples, cells)
        leg(f"variant {variant_id}: reciprocal for a 120 s track fits a u32",
            "yes", str(recip), 0 <= recip < 2**32,
            f"arithmetic on the declared grid: ({cells} << 32) // {n_samples} — 120 s at the product rate, "
            "an arithmetic stand-in for a real album's track. The reciprocal is a property of a grid *and* "
            "a length, which is why it is per track and the grid is declared once (§6)")
        blob = build_albm1(
            "Bioluminescent", "unknown", 2026, 1,
            [{
                "start_sample": 0, "n_samples": n_samples, "recip": recip, "asset_off": 0,
                "asset_len": 0, "frames": 0, "max_fixed_order": 2, "backend": 1,
                "cost_ceiling_permille": 0, "title": "Self Test 1", "artist": "unknown",
                "fnv": fnv1a32_parts(b""),
            }],
            b"", bar_cells=cells,
        )
        carried = bar_albm1_value(blob)
        module = read_layout_module_text(layout_rs_text(facts), f"generated module for variant {variant_id}")
        leg(f"variant {variant_id}: the ALBM1 header carries the declared grid",
            str(cells), str(carried), carried == cells,
            "declared -> `build_albm1(..., bar_cells=)` -> the u32 at header offset 12 -> read back out of "
            "the bytes, which is the only way the pack half of `bar-cells-disagree` can be checked here")
        leg(f"variant {variant_id}: the generated src/layout.rs declares the same grid",
            str(cells), str(module["BAR_CELLS"]), int(module["BAR_CELLS"]) == cells,
            "the same declared number as it reaches the generated Rust module — and the module is read back, "
            "because a file this tool wrote itself is exactly the sort of thing it should not trust")
        leg(f"variant {variant_id}: declared / pack / module agree",
            "agree", "agree",
            agree(f"assert_bar_grid_agrees({cells}, {carried}, {module['BAR_CELLS']})",
                  lambda cells=cells, carried=carried, module=module: assert_bar_grid_agrees(
                      cells, carried, int(module["BAR_CELLS"]), f"variant {variant_id}")),
            "`bar-cells-disagree` in the negative — the three-way check passes on the real spec. The "
            "negative control that makes that mean anything is the register row above, where three "
            "different numbers do get refused by name")
        if variant_id != "A":
            owned = nodiv_grid_constant()
            leg("the shared crate must not own a bar grid as a constant",
                "no such constant", f"nodiv.rs declares BAR_CELLS = {owned}" if owned is not None else "none",
                owned is None,
                "D9 item 1, stated where it can actually be checked. `crates/gba-playback/src/nodiv.rs` "
                "carried `BAR_CELLS = 32` until 2a PR-1, and `meta::parse` validated a manifest against it; "
                "the grids this spec declares are 18 and 80, so that constant was only ever a third "
                "opinion. It is gone and the boot check compares a manifest against the generated module "
                "instead — this leg is the guard that keeps it gone, and §6 keeps the boot check as the "
                "hardware backstop, so it stays")
            leg("and neither artifact this tool writes carries the old value",
                "not 32", str(carried), carried != 32 and int(module["BAR_CELLS"]) != 32,
                "the same claim about the two artifacts under test: 80 is B's grid, and nothing in this "
                "build should be able to produce 32 by default")
        if variant_id == "A":
            leg("the fine grid's worked example still computes with the grid as an input",
                "32-cell example", "grid-parameterised",
                bar_recip(12_582_912, 32) == (32 << 32) // 12_582_912
                and bar_recip(12_582_912, 80) == (80 << 32) // 12_582_912,
                "not a quoted figure: the same function answers for any declared grid, so a reader can "
                "re-run §6's arithmetic at 32 cells without that grid being anybody's product choice")
        for n, want_refused in ((1, True), (cells, True), (cells + 1, False)):
            # `bar_recip` refuses at that boundary instead of returning a value past u32, so
            # the thing this leg can observe is the refusal *by id* — the arithmetic form
            # (`>= 2**32`) is unreachable code below the guard, and a leg written on it would
            # be checking a branch that never runs.
            try:
                bar_recip(n, cells)
            except Refusal as exc:
                rule = exc.rule
            else:
                rule = None
            leg(f"variant {variant_id}: {n} sample(s) against a {cells}-cell grid",
                "refused by id" if want_refused else "packs",
                rule or "packs",
                rule == "bar-reciprocal" if want_refused else rule is None,
                "arithmetic: at exactly {c} samples floor(2^32 x {c} / n) is 2^32, one past ALBM1's u32 "
                "recip field, and below that larger still — so a track no longer than the grid cannot be "
                "given a reciprocal at all, and `build_albm1` would raise a bare `struct.error` instead. "
                "Not a plan figure: a consequence of grids this fine (1 px cells), and the reason the "
                "reciprocal is refused before the bytes are written rather than caught at boot".format(c=cells))

    # --- where a character-count box must no longer be ----------------------------
    print()
    print("  D9 item 2: no character-count box may own a fit — in the product, or in its tests")
    hits = _banned_box_hits(BANNED_SEARCH_PRODUCT)
    leg("none of the four retired character-count names survives in the product's own code",
        "none", f"{len(hits)} hit(s)", not hits,
        "§5.5's ruling, quoted: “every text element's fit box now comes from the variant's declared "
        "*span* (a pixel width — §5.0) and the face's real metrics, and the bake tool still prints any "
        "title that will not fit its box”. D9 is the record of the three copies that rule removed — this "
        "file's old prose, the `chars().count() <= 28` host guard, and `TITLE_BOX_MAX = 28`. Searched over "
        "`scripts/` and the two `src/` trees with literals and comments blanked first: the sentences that "
        "retire the old name — this tool's own list of banned strings, the generated module's doc comment "
        "saying those names are gone — would otherwise each count as a hit, and a grep that reports its "
        "own wording is quoting itself")
    for hit in hits:
        print(f"      {hit}")
    test_hits = _banned_box_hits(BANNED_SEARCH_TESTS)
    leg("and the host tests do not re-establish one either",
        "none", f"{len(test_hits)} hit(s)", not test_hits,
        "the same strings, looked for in `examples/album_player/tests`: a test asserting "
        "`chars().count() <= 28` puts the retired box back as the thing a build is checked against, "
        "which is how a number meant to die becomes load-bearing again. D9 item 2's copy of the number "
        "was exactly this test's guard; the fixture's title test now sits on the measured side — the "
        "declared box in the generated layout module, against the declared face's advances — and this "
        "leg is the guard that it stays there")
    for hit in test_hits:
        print(f"      {hit}")

    # --- the faces themselves: provenance, not folklore ---------------------------
    print()
    print("  every metric above came from a file: the declared faces have to be tracked and reproducible")
    for face in spec.faces:
        tracked = git_tracked(face.path())
        leg(f"{Path(face.file).name}: tracked in git", "yes", "yes" if tracked else "no", tracked,
            "§9.0's `face-not-resolvable` row: the question is `git ls-files --error-unmatch`, not a proxy "
            "for it. A face that exists only on this machine is a build artifact that escaped, and this "
            "tree has been bitten by exactly that once")
        for lic in face.licence_paths():
            lic_tracked = git_tracked(lic)
            leg(f"{Path(face.file).name}: licence {lic.relative_to(REPO)}", "yes",
                "yes" if lic_tracked else "no", lic_tracked,
                "§5.5's licence rows: a face may not be used without the licence it ships with, and the "
                "spec's `licence` rows are the only place that says where it came from")
        if face.parent:
            parent = Path(face.parent) if Path(face.parent).is_absolute() else REPO / Path(face.parent)
            parent_tracked = git_tracked(parent)
            leg(f"{Path(face.file).name}: declared parent {Path(face.parent).name} is tracked",
                "yes", "yes" if parent_tracked else "no", parent_tracked,
                "the `parent` row is a claim about provenance, so it has to be checkable by anyone who "
                "clones the tree")
            if parent_tracked:
                same, detail = subset_reproduces(face)
                leg(f"{Path(face.file).name}: regenerable from that parent",
                    "byte-identical", "byte-identical" if same else f"differs ({detail})", same,
                    "§5.5a's step 1 is a `pyftsubset` cut of a tracked parent. The codepoints re-subset "
                    "with are the ones the tracked cut itself maps, so nothing about the command is "
                    "assumed; if the bytes did not match, the tracked file would be a file whose "
                    "provenance nobody can check")
            else:
                note(f"{Path(face.file).name}: no parent to check", "skipped",
                     "the declared parent is not tracked, so the cut's provenance is a claim in a README "
                     "rather than something this tool can re-run")
        else:
            note(f"{Path(face.file).name}: declared as the tracked asset itself", "no parent row",
                 "the spec says so out loud: this cut came from an upstream release, and the tree does not "
                 "carry a parent file to re-run the subset against. That is also why §5.5 rules it "
                 "non-product: its chain of provenance stops at a download")

    # --- 2b PR-(2): the measured terms are in the report, and said out loud ---------
    #
    # The render probe measured L1-L4; this leg is the pipeline's promise that the
    # numbers arrived and are labelled as measurements rather than as arithmetic
    # that happens to look precise. It reads the *rendered report*, not the
    # constants, because a constant nobody emits is not a report.
    print()
    print("  2b PR-(2): AP-3b's measured terms are in the report, tagged and sourced")
    report = report_text(spec, list(facts_by_variant.values()))
    leg("the report carries a [measurement render] block",
        "present", "present" if "[measurement render]" in report else "absent",
        "[measurement render]" in report,
        "2b PR-(2): the folds a coefficient came from belong in the machine-readable artifact, "
        "not only in the probe's README")
    leg("that block names the instrument and the date",
        "render_probe_at=", "render_probe_at=" if "render_probe_at=" in report else "absent",
        "render_probe_at=" in report,
        "an un-attributed cycle number is the thing §5.1's measured table exists to avoid; the "
        "probe's README and this line have to say mGBA vs the cart")
    leg("the bar-cell term is `measured`, not `arithmetic`",
        "measured", "measured" if "cycles_bar_cell_tag=measured" in report else "arithmetic",
        "cycles_bar_cell_tag=measured" in report,
        "AP-3b L1 measured one changed bar cell at the app's cadence; PR-(2) is where that tag "
        "flips (`cycles_block`'s docstring said so before the measurement existed)")
    leg("the clock-digit term is `measured` too",
        "measured", "measured" if "cycles_clock_digit_tag=measured" in report else "arithmetic",
        "cycles_clock_digit_tag=measured" in report,
        "AP-3b L1's two-tile-row leg; the clock is the element R1b's hardware defect was about, so "
        "it is not left as an estimate")
    leg("the scene switch is measured, and no longer an excluded term",
        "measured / absent",
        ("measured / absent"
         if "cycles_scene_switch_tag=measured" in report
         and "cycles_excluded_scene_switch_term" not in report
         else "still excluded"),
        "cycles_scene_switch_tag=measured" in report
        and "cycles_excluded_scene_switch_term" not in report,
        "AP-3b L3: the priority store is 73 c and the commit is ~212,000 c — an event price, not "
        "the 'tens of cycles' §5.1 assumed. Keeping the excluded line beside the measured one "
        "would let two answers stand at once")
    leg("the pool leg's shared-slot evidence is in the report",
        f"max_refs>{1}", f"max_refs={MEASURED_POOL_MAX_REFS}",
        MEASURED_POOL_MAX_REFS > 1 and MEASURED_POOL_DISTINCT <= MEASURED_POOL_SLOTS,
        "AP-3b L4: refs are cell references and distinct are live slots; a per-cell fallback reads "
        "distinct == refs, which is the failure mode a total-only check cannot see")
    leg("the measured pool stand-up fits the 512-slot regular pool",
        f"<={MEASURED_POOL_SLOTS}", str(MEASURED_POOL_DISTINCT),
        MEASURED_POOL_DISTINCT <= MEASURED_POOL_SLOTS,
        "art 256 + atlas 188 + icons 16 = 460, read back from the maps by the ROM rather than "
        "summed here; R14's panic is what an over-fill would find instead")
    leg("the coefficient the arithmetic terms use is L2's, not AP-0's upload leg",
        "L2 per-cell", f"{CYCLE_PER_TILE_LIVE} c",
        CYCLE_PER_TILE_LIVE == MEASURED_REGION_PER_CELL_LIVE,
        "the title-line and region terms multiply by it; the probe measured the app's repaint "
        "shape, and the arithmetic terms say so in their own `_source` line")

    # --- 2b PR-(4), Decision B: the album's box is the audited image term -----------------
    print()
    print("  2b PR-(4): selecting a smaller album box lowers the audited pool term")
    selected = derive_variant(spec, spec.variants[0])
    ceiling = selected.art_ceiling.tiles if selected.art_ceiling else 0
    before = selected.pool_distinct
    selected.select_image(144)  # 96x96 = 12x12 tiles
    leg("a 96x96 box moves the image term off the ceiling and the sum with it",
        f"{before - (ceiling - 144)}", f"{selected.pool_distinct}",
        selected.pool_distinct == before - (ceiling - 144),
        "Decision B (2b PR-(4)): the image term is the selected box, and `assets.rs` carries "
        "the selected `POOL_IMAGE`/`POOL_DISTINCT` while `layout.rs` keeps the variant's ceiling "
        "(`POOL_UPPER`), so a variant module stays album-independent")

    print()
    if failures:
        print(f"SELF-TEST FAILED: {len(failures)} leg(s)")
        for failure in failures:
            print(f"  {failure}")
        return 1
    print(f"SELF-TEST PASS: {len(fired)} refusal ids watched fire, {legs} reproduction legs agree, and "
          "the arithmetic this tool runs is the arithmetic the plan prints.")
    print("  note: `meta::parse`'s boot-time bar check is the same row's other half and is asserted in")
    print("  Rust — `make album-layout-test` runs this and that together, because a Python tool cannot")
    print("  exercise a ROM that has to refuse (§6, §9.0).")
    return 0
# --- self-test --------------------------------------------------------------


def _write_config(
    path: Path, *, rate_hz: int = PRODUCT_CLOCK_HZ, year: str = "2026", sections: str = ""
) -> Path:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        f"""{sections}
[album]
title  = "Self Test"
artist = "album_bake"
year   = {year}

[out]
rom        = "album-player.gba"
backend    = "flac"
rate_hz    = {rate_hz}
cart_bytes = 33554432

[[track]]
file  = "assets/sound/bioluminescent.wav"
title = "Self Test 1"
""",
        encoding="utf-8",
    )
    return path


def _test_wav(path: Path, seconds: float = 1.0) -> Path:
    """A deterministic product-profile WAV: 16,384 Hz, stereo, 16-bit.

    Three simultaneous tones, not one: a single sine is cheap enough for libFLAC
    to encode with a short FIXED predictor, and this file exists to be an input
    only an LPC frame can carry (the self-test's refusal (b) needs one).
    """
    import math
    import wave

    frames = int(seconds * PRODUCT_CLOCK_HZ)
    samples = bytearray()
    for index in range(frames):
        t = index / PRODUCT_CLOCK_HZ
        value = int(
            9_000 * math.sin(2 * math.pi * 220.0 * t)
            + 6_000 * math.sin(2 * math.pi * 331.0 * t + 1.1)
            + 4_000 * math.sin(2 * math.pi * 587.0 * t + 2.2)
        )
        samples += struct.pack("<hh", value, value)
    with wave.open(str(path), "wb") as handle:
        handle.setnchannels(2)
        handle.setsampwidth(2)
        handle.setframerate(PRODUCT_CLOCK_HZ)
        handle.writeframes(bytes(samples))
    return path


def _fake_albm1_assets() -> tuple[bytes, bytes]:
    """Two tracks' worth of arbitrary blob bytes, for the audit round trip.

    The audit is about `ALBM1`'s structure; what a backend's container says is
    the adapter's business in Phase 3, so these bytes only have to be distinct.
    """
    return b"GAFP-fake-blob-0", b"GAFP-fake-blob-1"


def self_test() -> int:
    """Exercise all four refusals and the audit, with no repo state required."""
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
        root = Path(tmp)

        def declared_rate_refused() -> None:
            album = _write_config(root / "rate" / "album.toml", rate_hz=32768)
            try:
                bake(album)
            except Refusal as exc:
                assert exc.rule == "rate", f"expected rule 'rate', got {exc.rule!r}"
                print(f"  refused — {exc.rule}: {exc.detail}")
            else:
                raise AssertionError("a 32,768 Hz declaration was accepted")
            assert not (root / "rate" / "assets").exists(), "a refused bake wrote assets"
            assert not (root / "rate" / "src").exists(), "a refused bake wrote a pin module"

        def unknown_section_refused() -> None:
            album = _write_config(root / "section" / "album.toml", sections="[viz]\nstreams = []")
            try:
                bake(album)
            except Refusal as exc:
                assert exc.rule == "config-section", exc.rule
                assert "Phase 3" in exc.detail, exc.detail
                print(f"  refused — {exc.rule}: {exc.detail}")
            else:
                raise AssertionError("[viz] was silently ignored")

        def lpc_refused_by_the_child() -> None:
            wav = _test_wav(root / "lpc.wav")
            flac = root / "lpc.flac"
            proc = subprocess.run(
                ["flac", "-1", "-f", "-s", "-l", "8", "-b", "2048", "-m", str(wav), "-o", str(flac)],
                capture_output=True,
                text=True,
            )
            assert proc.returncode == 0, f"flac -l 8 failed: {proc.stderr}"
            gfp = root / "lpc.gfp"
            try:
                pack_track(flac, gfp, root / "lpc_pin.rs", 0)
            except Refusal as exc:
                assert exc.rule == "encode-profile", exc.rule
                assert "LPC" in exc.detail, exc.detail
                print(f"  refused — {exc.rule}: {exc.detail}")
            else:
                raise AssertionError("an LPC blob was accepted in master mode")
            assert not gfp.exists(), "a refused track wrote a blob"

        def sample_disagreement_refused() -> None:
            wav = _test_wav(root / "agree.wav", seconds=0.5)
            gfp = root / "agree.gfp"
            pack_track(wav, gfp, root / "agree_pin.rs", 0)
            container = read_container(gfp)
            prepared = album_prep.frames(wav)
            check_sample_agreement(0, prepared, container)
            data = bytearray(gfp.read_bytes())
            struct.pack_into("<Q", data, 14, container["total_samples"] - 2048)
            patched = root / "patched.gfp"
            patched.write_bytes(bytes(data))
            try:
                check_sample_agreement(0, prepared, read_container(patched))
            except Refusal as exc:
                assert exc.rule == "manifest-samples", exc.rule
                print(f"  refused — {exc.rule}: {exc.detail}")
            else:
                raise AssertionError("a container missing 2,048 samples was accepted")

        def tiny_track_refused() -> None:
            # §6's reciprocal is a u32 field: 32 samples would need 2^32, and
            # `struct.pack_into` would raise a bare struct.error instead of a
            # refusal.
            try:
                bar_recip(n_samples=BAR_CELLS_REGISTERED, bar_cells=BAR_CELLS_REGISTERED)
            except Refusal as exc:
                assert exc.rule == "bar-reciprocal", exc.rule
                print(f"  refused — {exc.rule}: {exc.detail}")
            else:
                raise AssertionError(f"a {BAR_CELLS_REGISTERED}-sample track got a u32-overflowing recip")

        def non_integer_config_refused() -> None:
            album = _write_config(root / "integer" / "album.toml", year="2026.5")
            try:
                bake(album)
            except Refusal as exc:
                assert exc.rule == "config", exc.rule
                assert "year" in exc.detail, exc.detail
                print(f"  refused — {exc.rule}: {exc.detail}")
            else:
                raise AssertionError("a TOML float year was truncated into ALBM1's u16")

        def audit_round_trip() -> None:
            first, second = _fake_albm1_assets()
            tracks = [
                {
                    "start_sample": 0,
                    "n_samples": 4_237_112,
                    "recip": bar_recip(4_237_112, BAR_CELLS_REGISTERED),
                    "asset_off": 0,
                    "asset_len": len(first),
                    "frames": 2_068,
                    "max_fixed_order": 2,
                    "backend": 1,
                    "cost_ceiling_permille": 0,
                    "title": "Self Test 1",
                    "artist": "",
                    "fnv": fnv1a32_parts(first),
                },
                {
                    "start_sample": 4_237_112,
                    "n_samples": 491_520,
                    "recip": bar_recip(491_520, BAR_CELLS_REGISTERED),
                    "asset_off": len(first),
                    "asset_len": len(second),
                    "frames": 240,
                    "max_fixed_order": 3,
                    "backend": 1,
                    "cost_ceiling_permille": 0,
                    "title": "Self Test Clip",
                    "artist": "",
                    "fnv": fnv1a32_parts(second),
                },
            ]
            blob = build_albm1("Self Test", "album_bake", 2026, 1, tracks, first + second,
                               bar_cells=BAR_CELLS_REGISTERED)
            problems = audit_blob(blob, BAR_CELLS_REGISTERED)
            assert not problems, problems
            # Every mutation is refused by the same structural rule the reader
            # applies; the shipping parser is driven by the host gate, which
            # parses the blob a real bake wrote and applies this same named set
            # (`refuses_every_mutation_the_bake_self_test_feeds_to_audit_blob`,
            # fed from the list `assets_rs` emits), so the two cannot drift.
            for name, mutate in AUDIT_MUTATIONS.items():
                bad = bytearray(blob)
                mutate(bad)
                problems = audit_blob(bytes(bad), BAR_CELLS_REGISTERED)
                assert problems, f"an audited mutation ({name}) was accepted"
            print(f"  audit: round trip ok, {len(AUDIT_MUTATIONS)} mutations refused")

        def content_tripwire_is_bounded() -> None:
            # `blob` is longer than prefix + suffix, so the middle byte is in
            # neither span: that is the property the boot path relies on.
            blob = bytes(range(256)) * 512
            prefix, suffix = content_pins(blob)
            assert prefix == fnv1a32_parts(blob[:CONTENT_PREFIX_BYTES])
            assert suffix == fnv1a32_parts(blob[-CONTENT_SUFFIX_BYTES:])
            middle = bytearray(blob)
            middle[len(blob) // 2] ^= 0xFF
            assert content_pins(bytes(middle)) == (prefix, suffix), (
                "a byte outside both spans moved a content pin — the tripwire is not bounded"
            )
            for at in (0, len(blob) - 1):
                edge = bytearray(blob)
                edge[at] ^= 0xFF
                assert content_pins(bytes(edge)) != (prefix, suffix), at
            print("  bounded — the content pins cover only the first 32 KiB and last 4 KiB")

        def riff_walk_is_bounded_by_the_declared_size() -> None:
            # A RIFF header that lies about the container's length. The `LIST`
            # below is one the reader does find when the header is honest (the
            # positive control), so the negative case is about the bound and not
            # about a malformed fixture.
            value = b"Hidden Title\x00"
            sub = b"INAM" + struct.pack("<I", len(value)) + value
            if len(sub) & 1:
                sub += b"\x00"
            body = b"INFO" + sub
            chunk = b"LIST" + struct.pack("<I", len(body)) + body
            data = _test_wav(root / "riff.wav", seconds=0.1).read_bytes()

            honest = bytearray(data + chunk)
            struct.pack_into("<I", honest, 4, len(honest) - 8)
            (root / "honest.wav").write_bytes(bytes(honest))
            tags = album_prep.read_tags(root / "honest.wav")
            assert tags["title"] == "Hidden Title", tags

            # The same file, with the container declared to end before its
            # `LIST`: the chunk is outside this RIFF, so the title is the stem.
            liar = bytearray(data + chunk)
            struct.pack_into("<I", liar, 4, len(data) - 8)
            (root / "liar.wav").write_bytes(bytes(liar))
            tags = album_prep.read_tags(root / "liar.wav")
            assert tags["title"] == "liar", f"a chunk past the declared end was read: {tags}"
            print("  bounded — a LIST after the declared end is not read")

            # A `LIST` whose declared body runs past the end of the file: the
            # read is clamped to what remains, so it neither raises nor invents
            # a value from bytes that are not there.
            truncated = bytearray(data + chunk[: 8 + 6])
            struct.pack_into("<I", truncated, 4, len(truncated) - 8)
            (root / "short.wav").write_bytes(bytes(truncated))
            tags = album_prep.read_tags(root / "short.wav")
            assert tags["title"] == "short", f"a truncated LIST produced {tags}"
            print("  bounded — a LIST body past the end of the file is clamped")

        # (b) needs the flac CLI; (c)/(d) need the shipped packer.
        check("(a) declared rate 32,768 Hz refused before any write", declared_rate_refused)
        check("D2 unknown section refused by name", unknown_section_refused)
        check("(b) LPC blob refused by the real gafp_pack.py", lpc_refused_by_the_child)
        check("(c) container total_samples -2,048 refused", sample_disagreement_refused)
        check("(d) audit re-reads the writer's own output", audit_round_trip)
        check("a track at the 32-cell bar refused (u32 recip)", tiny_track_refused)
        check("a non-integer config value refused by name", non_integer_config_refused)
        check("RIFF tag walk clamped to min(declared, remaining)", riff_walk_is_bounded_by_the_declared_size)
        check("content tripwire pins cover only the bounded spans", content_tripwire_is_bounded)

    if failures:
        print(f"album_bake self-test: {failures} failure(s)")
        return 1
    print("album_bake self-test: PASS")
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Bake an album: GAFP blobs + ALBM1 + assets.rs, and generate layout.rs (§5.0)."
    )
    parser.add_argument("album", nargs="?", default=str(DEFAULT_ALBUM), help="album.toml to bake")
    parser.add_argument("--self-test", action="store_true", help="exercise the four refusals and the audit")
    # `album-layout-gen` is a step inside this tool, not a build target of its own
    # (§11's PR-(1) row: "no new build target for it — `make album-bake` already
    # runs it"). These are the ways its own output is reached for a *test* rather
    # than for a ROM: `make album-layout-test` uses them, and `make album-bake`
    # never has to.
    parser.add_argument("--layout-self-test", action="store_true",
                      help="assert §9.0's register rows by id, and re-measure the tracked faces (§13's AP-3 bake leg)")
    parser.add_argument("--print-layout", action="store_true",
                      help="derive and print the audit for every variant the spec declares; write nothing")
    parser.add_argument("--emit-layout", action="store_true",
                      help="write src/layout.rs only; the numbers stay derived from the spec")
    parser.add_argument("--emit-report", default="", metavar="PATH",
                      help="write the machine-readable layout report (2b PR-(1)); '-' writes stdout. "
                           "No album is read, so the ROM addends say 'not computed' — the bake is where "
                           "`over-budget` is asked")
    parser.add_argument("--layout-only", action="store_true", help=argparse.SUPPRESS)
    parser.add_argument("--variant", default="", help="ask for a variant other than [layout]'s (a test instrument)")
    parser.add_argument("--spec", default=str(DEFAULT_LAYOUT_SPEC), help="the layout spec to read (a test instrument)")
    args = parser.parse_args(argv)
    try:
        if args.self_test:
            return self_test()
        if args.layout_self_test:
            return layout_self_test()
        if args.print_layout or args.emit_layout or args.emit_report:
            spec_path = Path(args.spec).resolve()
            spec = LayoutSpec(spec_path)
            if args.print_layout:
                print(
                    f"album-layout-gen: {_relative(spec_path)} declares every variant; "
                    "a product gate reports a non-product one as skipped with its reason (§13's AP-3 rule)"
                )
                for variant in spec.variants:
                    try:
                        derived = derive_variant(spec, variant)
                    except Refusal as exc:
                        print(f"  variant {variant.id}: not derivable — {exc.rule}: {exc.detail}")
                        continue
                    for line in derived.lines(gate=variant.product):
                        print(line)
            if args.emit_layout:
                facts = layout_for(args.variant, spec_path)[2]
                write_atomic(DEFAULT_LAYOUT_RS, layout_rs_text(facts).encode("utf-8"))
                print(f"album-layout-gen: wrote {_relative(DEFAULT_LAYOUT_RS)}")
            if args.emit_report:
                # Every declared variant, in declaration order, and a variant that cannot
                # be derived is *named* rather than silently dropped (AP-3's rule) — the
                # report is the spec's artifact, not one album's.
                facts_list: list[VariantFacts] = []
                skipped: list[tuple[str, str]] = []
                for variant in spec.variants:
                    try:
                        facts_list.append(derive_variant(spec, variant))
                    except Refusal as exc:
                        print(f"album-layout-gen: variant {variant.id} not derivable — {exc.rule}: {exc.detail}")
                        skipped.append((variant.id, exc.rule))
                text = report_text(spec, facts_list, skipped=skipped)
                if args.emit_report == "-":
                    sys.stdout.write(text)
                else:
                    write_atomic(Path(args.emit_report), text.encode("utf-8"))
                    print(f"album-layout-gen: wrote {args.emit_report}")
            return 0
        return bake(Path(args.album).resolve())
    except Refusal as exc:
        print(f"album_bake: refused — {exc.rule}: {exc.detail}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
