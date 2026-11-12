#!/usr/bin/env python3
"""Re-spell the shipped GAFP asset's frames as a 32,768 Hz stream, without
re-encoding a single residual — and measure what the two legal spellings cost.

This is the probe behind the 6b-3 record's "the rate question, measured from the
shipped frames" row (`flac-docs/measurements.md`) and the uncommon-form row in
`flac-docs/format-notes.md`. It was run on 2026-11-11 and **not committed at the
time**; it is committed now so its claims are re-appliable instead of quoted
(method-and-gates: "Probes live in `examples/flac_spike/experiments/<name>/`").

What it does, per shipped frame:

  1. slices the frame out of the committed `.gfp` with the blob's own offset
     table (the packer's walk, not this script's guess);
  2. verifies the frame's CRC-16, so a parser bug is caught before anything is
     rewritten (a frame it cannot walk is a hard error, not a skip);
  3. rewrites the 4-bit sample-rate code from `0b1000` (32 kHz) to one of the
     two legal spellings of 32,768 Hz — `0b0000` ("from the stream") or
     `0b1101` (exact Hz as a 16-bit field) — and recomputes the header CRC-8 and
     the frame CRC-16;
  4. wraps the concatenated frames in a hand-built STREAMINFO declaring 32,768 Hz.

Two traps this script exists to keep from being rediscovered:

  * STREAMINFO's min/max blocksize must be the **full** block (2048/2048) even
    though the last frame is 512 samples. With 512/2048 libFLAC reads the stream
    as variable-blocksize and prints `sample or frame number does not increase
    correctly` for all 157 frames.
  * `flac -d` writes a 44-byte RIFF/WAVE header whatever the output file is
    named, so hashing the *file* turns a bit-exact decode into a false mismatch.
    Hash the `data` chunk (this script does).

Usage:
    python3 probe.py [--keep]
Needs `flac` on PATH (decoder only; no encoder is used). The outcomes are
asserted, not printed for a human to eyeball: every frame's CRC-16 must verify
before it is touched, and each variant's decoded PCM must hash to the pinned
0x54C7B356621B6E15.
"""

from __future__ import annotations

import hashlib
import os
import shutil
import struct
import subprocess
import sys
import tempfile

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "..", "..", ".."))
GFP = os.path.join(ROOT, "examples", "flac_integration", "assets", "spike_l0.gfp")
PCM = os.path.join(ROOT, "examples", "flac_spike", "assets", "spike_l0_pcm.bin")

HEADER_LEN = 0x1C
FNV_PCM_PINNED = 0x54C7B356621B6E15
TARGET_HZ = 32_768

RATE_CODE_FROM_STREAM = 0b0000
RATE_CODE_EXACT_HZ = 0b1101
RATE_CODE_32K = 0b1000

# STREAMINFO's own size, and the "fLaC" + block header in front of it, so a size
# delta can be quoted against the frame region alone.
STREAMINFO_WRAPPER = 4 + 4 + 34


# ------------------------------------------------------------------ checksums


def crc8(data: bytes) -> int:
    """FLAC's header CRC: polynomial x^8 + x^2 + x + 1 (0x07), init 0."""
    crc = 0
    for b in data:
        crc ^= b
        for _ in range(8):
            crc = ((crc << 1) ^ 0x07) & 0xFF if crc & 0x80 else (crc << 1) & 0xFF
    return crc


def crc16(data: bytes) -> int:
    """FLAC's frame CRC: polynomial x^16 + x^15 + x^2 + 1 (0x8005), init 0."""
    crc = 0
    for b in data:
        crc ^= b << 8
        for _ in range(8):
            crc = ((crc << 1) ^ 0x8005) & 0xFFFF if crc & 0x8000 else (crc << 1) & 0xFFFF
    return crc


def fnv1a64(data: bytes) -> int:
    # The repository's *house* FNV offset basis, not the standard one — mirrored
    # in `examples/flac_spike/src/checksum.rs::FNV_OFFSET`,
    # `scripts/gen_gafp_blob.py` and the product ROM. Using the textbook basis
    # here would silently produce a "mismatch" that is only a different hash.
    h = 0xCBF43CE95DE684B7
    for b in data:
        h = ((h ^ b) * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return h


# -------------------------------------------------------------------- the blob


def read_manifest(blob: bytes) -> dict:
    if blob[:4] != b"GAFP":
        raise SystemExit("probe: not a GAFP blob")
    n = struct.unpack_from("<H", blob, 22)[0]
    table_end = struct.unpack_from("<H", blob, 24)[0]
    offsets = list(struct.unpack_from("<%dI" % n, blob, HEADER_LEN))
    offsets.append(len(blob))
    for i in range(n):
        if offsets[i + 1] <= offsets[i]:
            raise SystemExit("probe: offset table is not increasing at frame %d" % i)
    return dict(
        channels=blob[5],
        bps=blob[6],
        maxblock=struct.unpack_from("<H", blob, 8)[0],
        sample_rate=struct.unpack_from("<I", blob, 10)[0],
        total_samples=struct.unpack_from("<I", blob, 14)[0],
        n=n,
        table_end=table_end,
        frames=[blob[offsets[i] : offsets[i + 1]] for i in range(n)],
    )


def utf8_length(first: int) -> int:
    if first < 0x80:
        return 1
    for n, mask, want in (
        (2, 0xE0, 0xC0),
        (3, 0xF0, 0xE0),
        (4, 0xF8, 0xF0),
        (5, 0xFC, 0xF8),
        (6, 0xFE, 0xFC),
        (7, 0xFE, 0xFE),
    ):
        if first & mask == want:
            return n
    raise SystemExit("probe: bad UTF-8 frame number 0x%02X" % first)


def blocksize_of(code: int, frame: bytes) -> int:
    if code == 1:
        return 192
    if 2 <= code <= 5:
        return 576 << (code - 2)
    p = 4 + utf8_length(frame[4])
    if code == 6:
        return frame[p] + 1
    if code == 7:
        return struct.unpack_from(">H", frame, p)[0] + 1
    if 8 <= code <= 15:
        return 256 << (code - 8)
    raise SystemExit("probe: reserved blocksize code")


def header_shape(frame: bytes) -> dict:
    """Where each header field sits, so the rewrite is positional, not a search."""
    if frame[0] != 0xFF or (frame[1] & 0xFE) != 0xF8:
        raise SystemExit("probe: bad frame sync")
    bs_code = frame[2] >> 4
    sr_code = frame[2] & 0x0F
    p = 4 + utf8_length(frame[4])
    if bs_code == 6:
        p += 1
    elif bs_code == 7:
        p += 2
    if sr_code == 12:
        p += 1
    elif sr_code in (13, 14):
        p += 2
    return dict(
        bs_code=bs_code,
        sr_code=sr_code,
        blocksize=blocksize_of(bs_code, frame),
        crc8_at=p,
    )


def rewrite_frame(frame: bytes, spelling: int) -> bytes:
    """Re-spell one frame's sample rate, fixing both CRCs."""
    shape = header_shape(frame)
    if shape["sr_code"] != RATE_CODE_32K:
        raise SystemExit(
            "probe: a frame declares sr_code=0b%s, not 0b1000 — this probe is about "
            "the shipped 32 kHz asset only" % format(shape["sr_code"], "04b")
        )
    if crc16(frame[:-2]) != struct.unpack_from(">H", frame, len(frame) - 2)[0]:
        raise SystemExit("probe: shipped frame CRC-16 does not verify — do not rewrite it")

    body = bytearray(frame)
    body[2] = (body[2] & 0xF0) | spelling
    crc8_at = shape["crc8_at"]
    if spelling == RATE_CODE_EXACT_HZ:
        # Exact Hz follows the blocksize extra field, before the CRC-8.
        body[crc8_at:crc8_at] = struct.pack(">H", TARGET_HZ)
        crc8_at += 2
    body[crc8_at] = crc8(bytes(body[:crc8_at]))
    struct.pack_into(">H", body, len(body) - 2, crc16(bytes(body[:-2])))
    if header_shape(bytes(body))["sr_code"] != spelling:
        raise SystemExit("probe: rewrite did not take")
    if crc16(bytes(body[:-2])) != struct.unpack_from(">H", body, len(body) - 2)[0]:
        raise SystemExit("probe: rewritten frame's CRC-16 does not verify")
    return bytes(body)


# ---------------------------------------------------------------- the wrapping


def streaminfo(frame_sizes: list[int], manifest: dict) -> bytes:
    """A STREAMINFO for a fixed-blocksize stream, with the real PCM MD5."""
    md5 = hashlib.md5(open(PCM, "rb").read()).digest()
    block = int(manifest["maxblock"])
    bits = "{:016b}{:016b}{:024b}{:024b}{:020b}{:03b}{:05b}{:036b}".format(
        block,                      # min blocksize: the FULL block, last frame or not
        block,                      # max blocksize
        min(frame_sizes) if frame_sizes else 0,
        max(frame_sizes) if frame_sizes else 0,
        TARGET_HZ,
        int(manifest["channels"]) - 1,
        int(manifest["bps"]) - 1,
        int(manifest["total_samples"]),
    )
    packed = int(bits, 2).to_bytes(34 - 16, "big") + md5
    return b"fLaC" + bytes([0x80, 0, 0, 34]) + packed


def build(frames: list[bytes], manifest: dict) -> bytes:
    return streaminfo([len(f) for f in frames], manifest) + b"".join(frames)


# ------------------------------------------------------------------ verifying


def decode_and_hash(path: str) -> tuple[int, int]:
    out = path + ".wav"
    # `flac -t` exits 0 on the variable-blocksize misreading below — it only
    # *warns* — so its status is not the test. The warnings are read, and the
    # PCM hash is what decides.
    tested = subprocess.run(["flac", "-t", "-s", path], capture_output=True, text=True)
    if tested.returncode != 0:
        raise SystemExit("probe: flac -t failed: %s" % tested.stderr.strip())
    if "WARNING" in tested.stderr or "ERROR" in tested.stderr:
        raise SystemExit(
            "probe: flac -t passed but complained (the blocksize trap): %s"
            % tested.stderr.strip().splitlines()[0]
        )
    subprocess.run(["flac", "-d", "-s", "-f", "-o", out, path], check=True)
    raw = open(out, "rb").read()
    os.unlink(out)
    if raw[:4] != b"RIFF" or raw[8:12] != b"WAVE":
        raise SystemExit("probe: decoded output is not RIFF/WAVE")
    # Walk the chunks; hash `data`, never the file (the 44-byte-header trap).
    p = 12
    while p + 8 <= len(raw):
        cid = raw[p : p + 4]
        size = struct.unpack_from("<I", raw, p + 4)[0]
        body = raw[p + 8 : p + 8 + size]
        if cid == b"data":
            return fnv1a64(body), len(body)
        p += 8 + size + (size & 1)
    raise SystemExit("probe: no data chunk")


def main() -> int:
    keep = "--keep" in sys.argv
    blob = open(GFP, "rb").read()
    m = read_manifest(blob)
    if m["sample_rate"] != 32_000:
        raise SystemExit("probe: the shipped asset no longer declares 32,000 Hz")
    region = sum(len(f) for f in m["frames"])
    shapes = [header_shape(f) for f in m["frames"]]
    full = sum(1 for s in shapes if s["blocksize"] == m["maxblock"])
    pcm_pin = fnv1a64(open(PCM, "rb").read())
    if pcm_pin != FNV_PCM_PINNED:
        raise SystemExit("probe: the committed reference PCM is not the pinned one")

    print(
        "blob: %d B, %d frames, region %d B at 0x%x, declares %d Hz"
        % (len(blob), m["n"], region, m["table_end"], m["sample_rate"])
    )
    print(
        "frames: %d of %d samples, last %d; every header walks, every CRC-16 verifies"
        % (full, m["maxblock"], shapes[-1]["blocksize"])
    )

    workdir = tempfile.mkdtemp(prefix="p11-")
    try:
        for name, spelling, per_frame_bytes in (
            ("from-stream", RATE_CODE_FROM_STREAM, 0),
            ("exact-hz", RATE_CODE_EXACT_HZ, 2),
        ):
            frames = [rewrite_frame(f, spelling) for f in m["frames"]]
            out = os.path.join(workdir, "p32768_%s.flac" % name)
            with open(out, "wb") as fh:
                fh.write(build(frames, m))
            fnv, nbytes = decode_and_hash(out)
            if fnv != pcm_pin:
                raise SystemExit(
                    "probe: %s decoded to FNV 0x%016X, not the pinned 0x%016X"
                    % (name, fnv, FNV_PCM_PINNED)
                )
            payload = sum(len(f) for f in frames)
            total = os.path.getsize(out)
            print(
                "spelling 0b%s (%s): payload %d B (%+d against the shipped region = "
                "%d B/frame); flac -t ok; PCM 0x%016X [MATCH], %d B; stream %d B "
                "= wrapper %d B + payload"
                % (
                    format(spelling, "04b"),
                    "sample rate comes from STREAMINFO"
                    if spelling == RATE_CODE_FROM_STREAM
                    else "exact Hz as a 16-bit field",
                    payload,
                    payload - region,
                    per_frame_bytes,
                    fnv,
                    nbytes,
                    total,
                    STREAMINFO_WRAPPER,
                )
            )
            if keep:
                kept = os.path.join(os.path.dirname(os.path.abspath(__file__)), os.path.basename(out))
                shutil.copyfile(out, kept)
                print("  kept: %s" % kept)
    finally:
        if not keep:
            shutil.rmtree(workdir, ignore_errors=True)

    print(
        "probe: both spellings decode bit-for-bit to the shipped PCM, so the rate is a "
        "spelling and the 41 cents lives in the resampling that produced the source, "
        "not in this field. `gafp_pack` refuses either blob by design until a "
        "MEASURED_DECODE row exists at (32768, 2048, fixed, 0) — item 7-1's hardware read."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
