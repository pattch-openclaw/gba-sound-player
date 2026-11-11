#!/usr/bin/env python3
"""Generate the stand-in GAFP blob the integration ROM decodes.

**This is 6b-1's stand-in, not the packer.** `scripts/pack_flac.sh` is still a
stub; the real GAFP writer is 6b-2's rung. What this script produces is a blob
whose *frames* are real libFLAC output (the committed perf-gate clip) and whose
header/table are assembled here from the spike's own committed, fail-closed
generated table — so the container contract gets exercised on real encoder bytes
without pretending the packer exists.

Two consequences, both deliberate and both written down in the plan of record:

  * **6b-2 owes a byte-identical GAFP for the same input.** That turns this
    stand-in from a blind spot into a differential: if the real packer and this
    agree byte for byte on the same frames, the container has one definition
    rather than two.
  * **The offsets are read, not re-derived.** They come from
    `examples/flac_spike/src/assets.rs`, which `gen_spike_assets.sh` writes after
    a strict frame-by-frame walk (footer-gap rule included). Re-scanning here
    would be a second, weaker implementation of the same measurement.

Outputs (both committed inputs to `flac-integration`):

  examples/flac_integration/assets/spike_l0.gfp   the blob the ROM embeds
  examples/flac_integration/src/gafp_pin.rs       frames / samples / PCM pin

Usage:  scripts/gen_gafp_blob.py [--check]
        --check  rebuild in memory and compare with the committed outputs,
                 exiting non-zero on any difference (the regeneration tripwire).
"""

import os
import re
import struct
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)

SPIKE_ASSETS_RS = os.path.join(ROOT, "examples/flac_spike/src/assets.rs")
SPIKE_REGION = os.path.join(ROOT, "examples/flac_spike/assets/spike_l0_frames.bin")
SPIKE_PCM = os.path.join(ROOT, "examples/flac_spike/assets/spike_l0_pcm.bin")
OUT_BLOB = os.path.join(ROOT, "examples/flac_integration/assets/spike_l0.gfp")
OUT_PIN = os.path.join(ROOT, "examples/flac_integration/src/gafp_pin.rs")

MAGIC = b"GAFP"
VERSION = 1
HEADER_LEN = 0x1C


def l0_arm(text):
    """The `L0_FIXED` clip literal from the generated assets.rs."""
    start = text.index("pub const L0_FIXED: SpikeClip = SpikeClip {")
    end = text.index("\n};", start)
    return text[start:end]


def parse_l0(text):
    arm = l0_arm(text)
    frames = [
        (int(off), int(bs))
        for off, bs in re.findall(
            r"FrameMeta \{\s*offset: (\d+),\s*blocksize: (\d+),\s*\}", arm
        )
    ]
    if not frames:
        raise SystemExit("gen_gafp_blob: no FrameMeta entries in L0_FIXED")
    # The `fnv_pcm` pin is the one the spike's own host witness proves; reusing
    # it means the ROM's correctness claim rests on a value already witnessed
    # elsewhere rather than on one this script invented.
    fnv_pcm = int(re.search(r"fnv_pcm: 0x([0-9A-Fa-f]+),", arm).group(1), 16)
    channels = int(re.search(r"channels: (\d+),", arm).group(1))
    bps = int(re.search(r"bits_per_sample: (\d+),", arm).group(1))
    rate = int(re.search(r"sample_rate_hz: (\d+),", arm).group(1))
    return frames, fnv_pcm, channels, bps, rate


def fnv1a64(data):
    """The repository's FNV-1a 64 — its *house* offset basis, not the standard
    one. Mirrored in `examples/flac_spike/src/checksum.rs` (`FNV_OFFSET`) and in
    the integration ROM, because the pins this script emits have to agree with
    the ones the perf gate already proved."""
    h = 0xCBF43CE95DE684B7
    for byte in data:
        h ^= byte
        h = (h * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return h


def build():
    assets = open(SPIKE_ASSETS_RS, "r", encoding="utf-8").read()
    frames, fnv_pcm, channels, bps, rate = parse_l0(assets)
    region = open(SPIKE_REGION, "rb").read()

    # The packer records the highest FIXED order the encode used. The l0 arm is
    # `flac -1 -l 0`, i.e. FIXED-only, and its census says `fixed0` throughout,
    # so order 0 is the true maximum — not a default.
    max_fixed_order = 0

    # Cross-check the borrowed pin against the committed reference PCM: the
    # spike's `fnv_pcm` was produced by folding `flac -d`'s output, so hashing
    # `spike_l0_pcm.bin` here must reproduce it. If it does not, the pin and the
    # blob it is supposed to describe have drifted apart — which is exactly the
    # failure a borrowed pin would otherwise hide.
    reference = open(SPIKE_PCM, "rb").read()
    if fnv1a64(reference) != fnv_pcm:
        raise SystemExit(
            "gen_gafp_blob: the committed PCM no longer hashes to L0_FIXED.fnv_pcm "
            "(assets.rs and spike_l0_pcm.bin disagree)"
        )

    total_samples = sum(bs for _, bs in frames)
    table_end = HEADER_LEN + 4 * len(frames)
    blob = bytearray(table_end + len(region))

    blob[0:4] = MAGIC
    blob[4] = VERSION
    blob[5] = channels
    blob[6] = bps
    blob[7] = max_fixed_order
    struct.pack_into("<H", blob, 8, max(bs for _, bs in frames))
    struct.pack_into("<I", blob, 10, rate)
    struct.pack_into("<Q", blob, 14, total_samples)
    struct.pack_into("<H", blob, 22, len(frames))
    struct.pack_into("<I", blob, 24, table_end)
    for i, (offset, _bs) in enumerate(frames):
        struct.pack_into("<I", blob, HEADER_LEN + 4 * i, table_end + offset)
    blob[table_end:] = region

    pin = (
        "//! Generated by `scripts/gen_gafp_blob.py` — do not edit.\n"
        "//!\n"
        "//! Facts about the embedded `assets/spike_l0.gfp` blob, taken from the perf\n"
        "//! gate's committed, host-witnessed pins so the ROM's correctness claim\n"
        "//! rests on a value something else already proved. `FNV_PCM` is\n"
        "//! `flac_spike::assets::L0_FIXED.fnv_pcm`: FNV-1a 64 over the clip's\n"
        "//! interleaved i16 little-endian stereo PCM as `flac -d` produced it.\n"
        "\n"
        "/// Frames in the blob (the manifest's frame count must equal this).\n"
        f"pub const FRAMES: usize = {len(frames)};\n"
        "\n"
        "/// Interchannel samples per channel in the blob.\n"
        f"pub const SAMPLES: u64 = {total_samples};\n"
        "\n"
        "/// The reference PCM pin.\n"
        f"pub const FNV_PCM: u64 = 0x{fnv_pcm:016X};\n"
        "\n"
        "/// Highest FIXED predictor order the packer recorded for this clip.\n"
        f"pub const MAX_FIXED_ORDER: u8 = {max_fixed_order};\n"
    )
    return bytes(blob), pin, frames


def main(argv):
    check = "--check" in argv
    blob, pin, frames = build()
    if check:
        ok = True
        if not os.path.exists(OUT_BLOB):
            print(f"MISSING {OUT_BLOB}")
            ok = False
        elif open(OUT_BLOB, "rb").read() != blob:
            print(f"STALE   {OUT_BLOB}")
            ok = False
        if not os.path.exists(OUT_PIN):
            print(f"MISSING {OUT_PIN}")
            ok = False
        elif open(OUT_PIN, "r", encoding="utf-8").read() != pin:
            print(f"STALE   {OUT_PIN}")
            ok = False
        print(
            "gen_gafp_blob: %s (%d frames, %d bytes)"
            % ("clean" if ok else "regeneration needed", len(frames), len(blob))
        )
        return 0 if ok else 1

    os.makedirs(os.path.dirname(OUT_BLOB), exist_ok=True)
    with open(OUT_BLOB, "wb") as f:
        f.write(blob)
    with open(OUT_PIN, "w", encoding="utf-8") as f:
        f.write(pin)
    print(
        "gen_gafp_blob: wrote %s (%d frames, %d bytes) and %s"
        % (os.path.relpath(OUT_BLOB, ROOT), len(frames), len(blob), os.path.relpath(OUT_PIN, ROOT))
    )
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
