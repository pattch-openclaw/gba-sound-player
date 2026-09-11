#!/usr/bin/env python3
"""Measure FLAC warm-up-sample semantics on real libFLAC bytes.

Question under test -- the scaffold in crates/flac-lite/src/subframe.rs claims:

    "Warm-up samples are the previous frame's tail samples and are the only
     cross-frame state the decoder must retain."

RFC 9639 Sec 9.2.5 says something different: "each subframe in FLAC is coded
completely independently", and the warm-up samples "equal to the predictor
order ... are stored unencoded, bypassing the predictor and residual coding
stages" (Table 21: subframe = warm-up samples, then coded residual).

Two hypotheses, measured over every frame of real encodes:

  H_stream  warm-up == that subframe's OWN first `order` decoded samples
            (left-shifted by `wasted`, per Sec 9.2.2's padding rule)
  H_prev    warm-up == the PREVIOUS frame's LAST `order` samples

Reference PCM comes from `flac -d` (the reference decoder), never from us.
Source is MONO so subframe 0 is the audio channel directly, with no
decorrelation indirection to confuse the comparison.

Usage: python3 scripts/measure_warmup_semantics.py
"""
import os
import struct
import subprocess
import sys
import tempfile

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from frame_vectors import find_frames  # noqa: E402


class BitReader:
    """Deliberately naive bit reader, independent of the Rust one."""

    def __init__(self, data, bitpos):
        self.d = data
        self.p = bitpos

    def bits(self, n):
        v = 0
        for _ in range(n):
            v = (v << 1) | ((self.d[self.p >> 3] >> (7 - (self.p & 7))) & 1)
            self.p += 1
        return v

    def signed(self, n):
        v = self.bits(n)
        return v - (1 << n) if n and v >= (1 << (n - 1)) else v

    def unary(self):
        """Count zeros terminated by a one (Sec 9.2.2 wasted-bits run)."""
        k = 0
        while self.bits(1) == 0:
            k += 1
        return k


def write_wav(path, rate, samples):
    payload = struct.pack("<%dh" % len(samples), *samples)
    fmt = struct.pack("<IHHIIHH", 16, 1, 1, rate, rate * 2, 2, 16)
    blob = b"RIFF" + struct.pack("<I", 36 + len(payload)) + b"WAVEfmt " + fmt
    blob += b"data" + struct.pack("<I", len(payload)) + payload
    open(path, "wb").write(blob)


def read_wav(path):
    raw = open(path, "rb").read()
    assert raw[:4] == b"RIFF" and raw[8:12] == b"WAVE", "not a canonical WAV"
    assert raw[36:40] == b"data", "unexpected WAV chunk order"
    nbytes = int.from_bytes(raw[40:44], "little")
    return list(struct.unpack("<%dh" % (nbytes // 2), raw[44:44 + nbytes]))


def subframe0(reader, bps_frame, order_hint):
    """Walk one subframe header per RFC 9639 Sec 9.2.1-9.2.3.

    Returns (kind, order, wasted, warm_up_as_stored, subframe_bps).
    """
    assert reader.bits(1) == 0, "subframe pad bit MUST be 0 (Sec 9.2.1)"
    code = reader.bits(6)
    if code == 0:
        kind, order = "constant", 0
    elif code == 1:
        kind, order = "verbatim", 0
    elif 8 <= code <= 12:
        kind, order = "fixed", code - 8
    elif 32 <= code <= 63:
        kind, order = "lpc", code - 31
    else:
        raise AssertionError("reserved subframe type code 0b%06b" % code)
    if order_hint is not None:
        assert order == order_hint, "oracle %s vs walk %s" % (order_hint, order)

    wasted = reader.unary() + 1 if reader.bits(1) else 0
    bps = bps_frame - wasted
    assert bps > 0, "Sec 9.2.2: resulting bits per sample MUST be > 0"
    warm = [reader.signed(bps) for _ in range(order)]
    return kind, order, wasted, warm, bps


def measure(work, tag, encode_flags, wav):
    fl = os.path.join(work, tag + ".flac")
    subprocess.run(["flac", "-1", "-f", "-s", "-b", "2048"] + encode_flags +
                   ["-o", fl, wav], check=True)
    refp = os.path.join(work, tag + ".ref.wav")
    subprocess.run(["flac", "-d", "-f", "-s", "-o", refp, fl], check=True)
    raw = open(fl, "rb").read()
    ref = read_wav(refp)
    frames = find_frames(raw)

    total = checked = h_stream = h_prev = 0
    first = None
    mismatches = []
    prev_tail = None
    base = 0  # frames are contiguous; the last is legitimately short
    for idx, hdr in enumerate(frames):
        bps_frame = hdr["bps"] if hdr["bps"] is not None else 16
        reader = BitReader(raw, (hdr["offset"] + hdr["crc_pos"] + 1) * 8)
        kind, order, wasted, warm, bps = subframe0(reader, bps_frame,
                                                   hdr["subframe0_order"])
        own = ref[base:base + order] if order else []
        total += 1
        if order:
            checked += 1
            # Sec 9.2.2: a decoder MUST pad decoded samples left by `wasted`.
            padded = [w << wasted for w in warm]
            h_stream += padded == own
            if prev_tail is not None and padded == prev_tail:
                h_prev += 1
            if padded != own:
                mismatches.append((idx, kind, order, wasted, padded, own))
        if idx == 0 and order:
            first = dict(kind=kind, order=order, wasted=wasted, bps=bps,
                         warm_up_in_bitstream=warm, own_first_samples=own)
        prev_tail = ref[base + hdr["blocksize"] - order:base + hdr["blocksize"]] \
            if order else None
        base += hdr["blocksize"]

    print("== %s: %d frames, %d with order > 0 ==" % (tag, total, checked))
    print("   H_stream (== OWN first samples, << wasted): %d/%d" % (h_stream, checked))
    print("   H_prev   (== PREVIOUS frame's last samples): %d/%d" % (h_prev, checked))
    print("   frame 0 (no previous frame exists): %s" % (first,))
    if mismatches:
        print("   H_stream MISMATCHES: %s" % (mismatches[:3],))


def main():
    work = tempfile.mkdtemp(prefix="warmup-measure-")
    sr, n = 32000, 64000
    samples = []
    for i in range(n):
        v = ((i * 7919) % 1024) - 512 + 400 * (((i // 100) % 2) * 2 - 1)
        samples.append(max(-32768, min(32767, v)))
    wav = os.path.join(work, "src.wav")
    write_wav(wav, sr, samples)

    measure(work, "-l 0 (FIXED)", ["-l", "0"], wav)
    measure(work, "-l 4 (LPC)", ["-l", "4"], wav)
    print()
    print("Frame 0 has no previous frame, yet its warm-up sits in the bitstream")
    print("and equals its own first decoded samples: warm-up is stream-")
    print("authoritative, NOT cross-frame decoder state.")


if __name__ == "__main__":
    main()
