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
import os
import shutil
import struct
import subprocess
import sys
import tempfile
import tomllib
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
    title_fits,
)

REPO = Path(__file__).resolve().parent.parent
GAFP_PACK = REPO / "scripts" / "gafp_pack.py"
DEFAULT_ALBUM = REPO / "examples" / "album_player" / "album.toml"

# --- ALBM1, pinned here and in src/meta.rs ----------------------------------
#
# Both files are written against ALBUM-PLAYER.md §4.3 independently; this half is
# the writer, and its job is to refuse rather than to be accommodating.
MAGIC = b"ALBM"
GAFP_MAGIC = b"GAFP"
VERSION = 1
HEADER_BYTES = 64
TRACK_BYTES = 40
MAX_TRACKS = 32
POOL_LIMIT = 0x1_0000
BAR_CELLS = 32
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
    """The `[album]` / `[[track]]` / `[out]` subset, validated as the bake reads it."""
    def __init__(self, path: Path) -> None:
        self.path = path
        with open(path, "rb") as handle:
            try:
                doc = tomllib.load(handle)
            except tomllib.TOMLDecodeError as exc:
                raise Refusal("config", f"{path}: {exc}") from exc
        unknown = sorted(set(doc) - {"album", "track", "out"})
        if unknown:
            # D2: Phase 1 reads three sections. Ignoring the others would make
            # `album.toml` a config that lies about what it controls — Phase 3
            # (§9.0) is where `[art]`, `[viz]`, `[ui]` and `[game]` start to
            # mean something, and until then naming them is the refusal.
            raise Refusal(
                "config-section",
                f"{path}: section(s) {', '.join(unknown)} are not part of Phase 1's "
                "[album]/[[track]]/[out] subset. §9.0 grows this file in Phase 3; "
                "until then a section the bake silently ignored is a config that lies.",
            )
        album = doc.get("album") or {}
        out = doc.get("out") or {}

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
            raise Refusal(
                "track-count",
                f"{path}: {len(tracks)} tracks, ALBM1 carries 1..{MAX_TRACKS}",
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


def bar_recip(n_samples: int) -> int:
    """`floor(2^32 × BAR_CELLS / n_samples)` — the host's one divide.

    Refuses a track at or below `BAR_CELLS` samples: at exactly 32 the quotient
    is 2^32, one past ALBM1's `u32` recip field, and below 32 it is larger
    still, so such a track is not one the manifest can describe (and
    `struct.pack_into` would raise a bare `struct.error` instead of a refusal).
    """
    if n_samples <= BAR_CELLS:
        raise Refusal(
            "bar-reciprocal",
            f"{n_samples} samples is at most §6's {BAR_CELLS} bar cells, so "
            f"floor(2^32 × {BAR_CELLS} / {n_samples}) does not fit ALBM1's u32 recip field",
        )
    return (BAR_CELLS << 32) // n_samples


def assert_bar_reciprocal(index: int, n_samples: int, recip: int) -> None:
    """The four §6 properties plus the exact-floor inequality, over sampled positions.

    §6 names them as "the bake-time assertions, not hopes": monotonic in
    position, never reaching `BAR_CELLS`, landing on `BAR_CELLS - 1` at the last
    sample, and within one cell of the exact quotient — checked over 1,000..2.6 M
    samples, which is the range the spec states them for, plus the track's own
    endpoints.
    """
    target = BAR_CELLS << 32
    if not (recip * n_samples <= target < (recip + 1) * n_samples):
        raise Refusal(
            "bar-reciprocal",
            f"track {index}: recip {recip:#x} is not floor(2^32 × {BAR_CELLS} / {n_samples})",
        )
    positions = _bar_positions(n_samples)
    previous = 0
    for pos in positions:
        cell = (pos * recip) >> 32
        exact = (pos * BAR_CELLS) // n_samples  # host divide: this is the check
        if cell >= BAR_CELLS:
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
    if ((n_samples - 1) * recip) >> 32 != BAR_CELLS - 1:
        raise Refusal(
            "bar-reciprocal",
            f"track {index}: the last sample lands on cell {((n_samples - 1) * recip) >> 32}, "
            f"not {BAR_CELLS - 1}",
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
    struct.pack_into("<I", blob, 12, BAR_CELLS)
    struct.pack_into("<I", blob, 16, album_samples)
    struct.pack_into("<I", blob, 20, track_off)
    struct.pack_into("<I", blob, 24, track_len)
    struct.pack_into("<I", blob, 28, pool_off)
    struct.pack_into("<I", blob, 32, pool_len)
    struct.pack_into("<I", blob, 36, asset_off)
    struct.pack_into("<I", blob, 40, asset_len)
    struct.pack_into("<I", blob, 44, fnv1a32_parts(table, pool, asset_bytes))
    struct.pack_into("<H", blob, 48, album_title_off)
    struct.pack_into("<H", blob, 50, album_artist_off)
    struct.pack_into("<H", blob, 52, year)
    blob[track_off:pool_off] = table
    blob[pool_off:asset_off] = pool
    blob[asset_off:] = asset_bytes
    return bytes(blob)


def fnv1a32_parts(*parts: bytes) -> int:
    """FNV-1a/32 over concatenated spans — the checksum meta.rs re-verifies last."""
    value = FNV_OFFSET
    for part in parts:
        for byte in part:
            value ^= byte
            value = (value * FNV_PRIME) & MASK32
    return value


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
    "version": _overwrite(4, b"\x02"),
    "n_tracks": _overwrite(5, b"\x00"),
    "reserved byte 7": _overwrite(7, b"\x01"),
    "rate": _overwrite(8, b"\x00\x80\x00\x00"),
    "bar_cells": _overwrite(12, b"\x40\x00\x00\x00"),
    "checksum": _xor(44, 0xFF),
    "pool padding": _pad_pool,
}


def audit_blob(blob: bytes) -> list[str]:
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
    for offset in range(54, HEADER_BYTES):
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
    if _word(blob, 12) != BAR_CELLS:
        bad.append(f"bar_cells {_word(blob, 12)}")

    track_off = _word(blob, 20)
    track_len = _word(blob, 24)
    pool_off = _word(blob, 28)
    pool_len = _word(blob, 32)
    asset_off = _word(blob, 36)
    asset_len = _word(blob, 40)
    stored_fnv = _word(blob, 44)
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
        if not (recip * length <= BAR_CELLS << 32 < (recip + 1) * length):
            bad.append(f"track {index} recip {recip:#x}, not floor(2^32 × {BAR_CELLS} / {length})")
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

    computed = fnv1a32_parts(tracks, pool, assets)
    if stored_fnv != computed:
        bad.append(f"checksum {stored_fnv:#010x}, sections hash to {computed:#010x}")
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


def assets_rs(config: Config, tracks: list[dict], album_samples: int) -> str:
    """The generated pin module. Deterministic text: no timestamp, no abs path."""
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
                    f"        max_fixed_order: {track['pin']['MAX_FIXED_ORDER']},",
                    "    },",
                ]
            )
        )
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

/// The manifest, `assets/albm1.bin` (ALBM1 v1).
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
    /// §6's progress reciprocal, `floor(2^32 × 32 / n_samples)`.
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


def write_atomic(path: Path, data: bytes) -> None:
    """Write `path` through `path.tmp`, so a reader never sees a half file."""
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_name(path.name + ".tmp")
    tmp.write_bytes(data)
    os.replace(tmp, path)


def bake(config_path: Path) -> int:
    config = Config(config_path)
    crate_dir = config_path.parent
    assets_dir = crate_dir / "assets"
    assets_rs_path = crate_dir / "src" / "assets.rs"

    print(f"album_bake: {config_path.relative_to(REPO) if config_path.is_relative_to(REPO) else config_path}")
    print(
        f"album_bake: {len(config.tracks)} tracks, {config.backend} backend, "
        f"{PRODUCT_CLOCK_HZ} Hz stereo, cart {dec(config.cart_bytes)} B"
    )

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
                recip = bar_recip(container["total_samples"])
                assert_bar_reciprocal(track.index, container["total_samples"], recip)
                blob = gfp.read_bytes()
                entry = {
                    "index": track.index,
                    "container_samples": container["total_samples"],
                    "frames": container["frames"],
                    "max_fixed_order": container["max_fixed_order"],
                    "recip": recip,
                    "pin": read_pin_module(pin),
                    "blob_sha256": hashlib.sha256(blob).hexdigest(),
                    "fnv": fnv1a32_parts(blob),
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
            if not title_fits(title):
                # §5.5: the bake PRINTS a title that will not fit its box; the
                # album can be fixed at bake time, and refusing here would make
                # a long title a build failure instead of an album decision.
                print(
                    f"album_bake: note: track {track.index} title {title!r} is "
                    f"{len(title)} chars, wider than the {album_prep.TITLE_BOX_MAX}-char box"
                )
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
                    "blob_sha256": entry["blob_sha256"],
                    "pin": entry["pin"],
                }
            )
            cursor += entry["container_samples"]

        album_title = fold(config.title)
        album_artist = fold(config.artist)
        if not album_title:
            raise Refusal("title", "[album] title folds to nothing")
        blob = build_albm1(
            album_title,
            album_artist,
            config.year,
            BACKENDS[config.backend],
            tracks,
            bytes(asset_region),
        )

        # Refusal (d): the writer's output, re-read structurally, before any
        # write lands. The host gate re-reads it through the shipping meta.rs.
        problems = audit_blob(blob)
        if problems:
            raise Refusal("audit", "; ".join(problems))

        written = {f"track{entry['index']:02d}.gfp" for entry in packed_cache.values()}
        for entry in packed_cache.values():
            write_atomic(assets_dir / f"track{entry['index']:02d}.gfp", entry["blob"])
        write_atomic(assets_dir / "albm1.bin", blob)
        write_atomic(assets_rs_path, assets_rs(config, tracks, cursor).encode("utf-8"))
        # A track that left the config leaves its blob behind otherwise, and the
        # determinism leg would then be comparing a directory that carries a
        # track the manifest no longer names. `assets/` is generated (§2.3), so
        # this deletes only bytes a previous bake of this tool wrote.
        for stale in sorted(assets_dir.glob("track*.gfp")):
            if stale.name not in written:
                stale.unlink()
                print(f"album_bake: removed stale {stale.name} — no track in the config addresses it")

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
            album = _write_config(root / "section" / "album.toml", sections="[art]\ncover = 'x.png'")
            try:
                bake(album)
            except Refusal as exc:
                assert exc.rule == "config-section", exc.rule
                assert "Phase 3" in exc.detail, exc.detail
                print(f"  refused — {exc.rule}: {exc.detail}")
            else:
                raise AssertionError("[art] was silently ignored")

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
                bar_recip(BAR_CELLS)
            except Refusal as exc:
                assert exc.rule == "bar-reciprocal", exc.rule
                print(f"  refused — {exc.rule}: {exc.detail}")
            else:
                raise AssertionError(f"a {BAR_CELLS}-sample track got a u32-overflowing recip")

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
                    "recip": bar_recip(4_237_112),
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
                    "recip": bar_recip(491_520),
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
            blob = build_albm1("Self Test", "album_bake", 2026, 1, tracks, first + second)
            problems = audit_blob(blob)
            assert not problems, problems
            # Every mutation is refused by the same structural rule the reader
            # applies; the shipping parser is driven by the host gate, which
            # parses the blob a real bake wrote and applies this same named set
            # (`refuses_every_mutation_the_bake_self_test_feeds_to_audit_blob`,
            # fed from the list `assets_rs` emits), so the two cannot drift.
            for name, mutate in AUDIT_MUTATIONS.items():
                bad = bytearray(blob)
                mutate(bad)
                problems = audit_blob(bytes(bad))
                assert problems, f"an audited mutation ({name}) was accepted"
            print(f"  audit: round trip ok, {len(AUDIT_MUTATIONS)} mutations refused")

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

    if failures:
        print(f"album_bake self-test: {failures} failure(s)")
        return 1
    print("album_bake self-test: PASS")
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Bake an album: GAFP blobs + ALBM1 + assets.rs.")
    parser.add_argument("album", nargs="?", default=str(DEFAULT_ALBUM), help="album.toml to bake")
    parser.add_argument("--self-test", action="store_true", help="exercise the four refusals and the audit")
    args = parser.parse_args(argv)
    try:
        if args.self_test:
            return self_test()
        return bake(Path(args.album).resolve())
    except Refusal as exc:
        print(f"album_bake: refused — {exc.rule}: {exc.detail}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
