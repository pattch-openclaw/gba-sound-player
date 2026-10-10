#!/usr/bin/env python3
"""opus_assets.py — generate the Opus perf-gate spike's embedded clips.

Companion to frame_vectors.py (FLAC spike); OPUS.md "Measurement plan —
start here (added 2026-09-29)". Same house rules as the FLAC generator:
deterministic integer-synthesized source PCM, fail-closed censuses that
refuse to write a degenerate arm, sha256 + FNV-1a pins, generated Rust
with doc comments (valid Rust string literals, `};` terminators).

Arms (encode profile requested at pack time; the TOC census records truth):
  * opus_silk   — `-application voip` 12 kbps at the 48 kHz API rate =>
    SILK-only WB 20 ms (config 9; internal 16 kHz — the internal resampler
    rides the decode cost, which is part of what the gate measures).
  * opus_music  — `-application audio` 96 kbps at 48 kHz, FB/20 ms: what
    libopus actually picks (measured 2026-09-30: CELT-only FB 20 ms,
    config 31, x501 — the census corrects OPUS.md's 2026-09-29 note that
    TOC carries "no pure-CELT 20 ms config": RFC 6716 Table 2 lists
    CELT-only at 2.5/5/10/20 ms, and configs 28...31 are CELT FB).

Usage: opus_assets.py opus_assets <TMP_DIR> <CRATE_DIR>
Expects in TMP_DIR: silk.opus, music.opus, silk_ref.f32le, music_ref.f32le
(the ref blobs are ffmpeg/libopus FLOAT reference decodes — host ground truth;
the witness folds both sides to i16 with the crate's shared fold).

The generator WRITES silk_walk.f32le / music_walk.f32le into TMP_DIR: the
vendored decoder's own walk over each arm's freshly staged packets, produced
by the crate's dump_walk example (host cargo build — the production seam,
not a second transcription of the decode loop). The per-arm alignment
(`align_shift`) and fold-tolerance (`fold_mismatch`) pins are MEASURED
against it, fail-closed — the 2026-10-02 finding that OpusHead pre_skip is
not always the walk→reference alignment (SILK aligns at pre_skip − 3) is
why discovery runs here on every generation.
"""

import hashlib
import json
import math
import os
import shutil
import struct
import subprocess
import sys

# ---------------------------------------------------------------------------
# FNV-1a 64 — mirrors the Rust implementation in the spike crate (the same
# two-constant spec makes the cross-check trivial; frame_vectors.py carries
# the identical function for the FLAC spike).
# ---------------------------------------------------------------------------

FNV_OFFSET = 0xCBF43CE95DE684B7
FNV_PRIME = 0x00000100000001B3
MASK64 = (1 << 64) - 1


def fnv1a64(data):
    h = FNV_OFFSET
    for byte in data:
        h ^= byte
        h = (h * FNV_PRIME) & MASK64
    return h


def sha256(data):
    return hashlib.sha256(data).hexdigest()


def fold_half_up(x):
    """Mirror of the crate's `checksum::fold_f32_to_i16` (round half up):
    scale by 32768, floor(v + 0.5), clamp to i16.

    The two implementations must land on the same cell at every exact .5:
    this fold discovers the alignment/tolerance pins the Rust witness then
    re-derives from the same bytes, so a cross-language fold divergence
    would poison the pins exactly the way two transcriptions of one rule
    poison a witness (shared-consumer rule). NaN maps to 0, matching Rust's
    saturating `as i32`."""
    v = x * 32768.0
    if v != v:  # NaN guard (no decoded stream here produces NaN)
        return 0
    r = math.floor(v + 0.5)
    if r < -32768:
        return -32768
    if r > 32767:
        return 32767
    return int(r)


def fold_walk_fnv(walk_bytes):
    """FNV-1a 64 over the vendored walk's OWN folded output (i16 LE, half-up
    grid, every walk sample — no alignment shift). This is the ROM decode
    proof's golden: unlike the FLAC gate, Opus cannot pin the on-target PCM
    hash against the REFERENCE (the arms are lossy and the port has measured
    ±1 LSB residuals vs libopus — the walk's hash never equals the
    reference's). The walk-fold hash is a deterministic property of the
    vendored decoder over the exact embedded bytes: the generator measures
    it over the dump_walk output (the production seam), the host witness
    re-derives it in Rust, and the ROM must reproduce it on-target."""
    assert len(walk_bytes) % 4 == 0
    parts = []
    for (s,) in struct.iter_unpack("<f", walk_bytes):
        parts.append(struct.pack("<h", fold_half_up(s)))
    return fnv1a64(b"".join(parts))


def discover_alignment(walk_bytes, ref_bytes, pre_skip):
    """Measure where the vendored decoder's walk meets the ffmpeg/libopus
    reference: scan shifts around OpusHead `pre_skip`, fold both sides with
    the shared round-half-up grid, return (shift, mismatches, max_abs_lsb).

    Why measured, never recalled (2026-10-02 finding): the header's
    pre_skip is NOT always the alignment — the SILK arm aligns at
    pre_skip − 3 (the vendored SILK chain carries 3 fewer leading-delay
    samples than ffmpeg/libopus's trim; 0 grid mismatches there), the CELT
    arm aligns exactly at pre_skip. An earlier draft compared at pre_skip
    for both arms and printed 478,439/480,000 "mismatches" on SILK."""
    assert len(walk_bytes) % 4 == 0 and len(ref_bytes) % 4 == 0
    walk = [fold_half_up(s) for s in struct.unpack(f"<{len(walk_bytes) // 4}f", walk_bytes)]
    ref = [fold_half_up(s) for s in struct.unpack(f"<{len(ref_bytes) // 4}f", ref_bytes)]
    n = len(ref)
    if len(walk) < n:
        raise SystemExit("alignment discovery: walk shorter than reference")
    lo = max(0, pre_skip - 16)
    hi = min(pre_skip + 16, len(walk) - n)
    best = None  # (mismatches, max_abs_lsb, shift); ties prefer shift nearer pre_skip
    for shift in sorted(range(lo, hi + 1), key=lambda s: abs(s - pre_skip)):
        mismatches = 0
        max_lsb = 0
        cap = best[0] if best is not None else n  # abandon hopeless scans early
        for a, b in zip(walk[shift:shift + n], ref):
            d = a - b
            if d:
                mismatches += 1
                ad = -d if d < 0 else d
                if ad > max_lsb:
                    max_lsb = ad
                if mismatches >= cap:
                    break
        if best is None or mismatches < best[0]:
            best = (mismatches, max_lsb, shift)
        if best[0] == 0:
            break
    return best[2], best[0], best[1]


def _tool(kind):
    # Mirror the Makefile's toolchain selection: rustup nightly when present,
    # bare tool otherwise (the container carries nightly without rustup).
    if shutil.which("rustup"):
        return ["rustup", "run", "nightly", kind]
    return [kind]


def host_triple():
    out = subprocess.run(_tool("rustc") + ["-vV"], check=True,
                         capture_output=True, text=True).stdout
    for line in out.splitlines():
        if line.startswith("host: "):
            return line.split(": ", 1)[1].strip()
    raise SystemExit("dump_walk: cannot determine host triple from rustc -vV")


def dump_vendored_walk(spike_src, host, tmp, arm, region_path, lengths):
    """Run the crate's dump_walk example (host target) over the freshly
    staged region + packet-length table and return its raw f32-LE walk
    bytes. cargo runs from TMP (never from inside the repo): the root
    .cargo/config.toml would otherwise leak the thumbv4t build target —
    the same gate-neutrality rule `make opus-test` follows. --target is
    passed explicitly so the host build is stated, never inherited."""
    lengths_path = os.path.join(tmp, f"{arm}_lengths.bin")
    with open(lengths_path, "wb") as f:
        for ln in lengths:
            f.write(struct.pack("<I", ln))
    out_path = os.path.join(tmp, f"{arm}_walk.f32le")
    cmd = _tool("cargo") + ["run", "--release", "--quiet",
                            "--manifest-path", os.path.join(spike_src, "Cargo.toml"),
                            "--target", host, "--features", "host-tools",
                            "--example", "dump_walk", "--",
                            region_path, lengths_path, out_path]
    done = subprocess.run(cmd, cwd=tmp, capture_output=True, text=True)
    if done.returncode != 0:
        sys.stderr.write(done.stderr)
        raise SystemExit(f"dump_walk failed for {arm} (exit {done.returncode})")
    return open(out_path, "rb").read()


def rust_str(s):
    """A valid Rust string literal. Python's repr() quotes with single
    quotes — invalid Rust. json.dumps emits double-quoted literals with
    Rust-compatible escapes, but its ensure_ascii DEFAULTS TO True — the
    escaped form it produces, \\uXXXX, is valid JSON and INVALID Rust
    (Rust writes \\u{XXXX}); it bit this generator on the em-dash in an
    arm's `why` string (compile gate, 2026-10-01). Pass
    ensure_ascii=False: the docs' own generated-Rust rule."""
    return json.dumps(s, ensure_ascii=False)


# ---------------------------------------------------------------------------
# Ogg container reader (the deletion target of the product profile — but the
# pack input IS Ogg/Opus from ffmpeg, so the demuxer lives here, witnesses
# included: page CRC-32 verified, packet boundaries from the lacing table).
# ---------------------------------------------------------------------------

def _crc_table():
    # Ogg CRC: polynomial 0x04c11db7, MSB-first, init 0, no final xor.
    table = []
    for i in range(256):
        r = i << 24
        for _ in range(8):
            r = ((r << 1) ^ (0x04C11DB7 if r & 0x80000000 else 0)) & 0xFFFFFFFF
        table.append(r)
    return table


_CRC_TABLE = _crc_table()


def ogg_crc(data):
    crc = 0
    for byte in data:
        crc = ((crc << 8) ^ _CRC_TABLE[((crc >> 24) & 0xFF) ^ byte]) & 0xFFFFFFFF
    return crc


def read_ogg_packets(path):
    """Return (packets, serial, pages_verified). Packets are the raw payload
    bytes reassembled across pages by the lacing table. Every page's CRC-32
    is verified; page/serial/continuity invariants fail closed.

    Header layout (RFC 3533 §6): bytes 0..4 capture 'OggS', 4 version,
    5 header_type, 6..14 granule (u64 LE), 14..18 serial, 18..22 sequence,
    22..26 CRC-32, 26 segment count, 27.. lacing table. The CRC is computed
    over the whole page with the CRC-32 field itself (22..26) set to zero.
    """
    data = open(path, "rb").read()
    pos = 0
    packets = []
    serial = None
    expect_seq = 0
    partial = None  # open packet carried across a page boundary
    pages = 0
    while pos < len(data):
        if data[pos:pos + 4] != b"OggS":
            raise SystemExit(f"ogg: bad capture pattern at {pos}")
        (ver, flags, granule, my_serial, seq, crc_stored,
         n_seg) = struct.unpack_from("<BBQIIIB", data, pos + 4)
        if ver != 0:
            raise SystemExit(f"ogg: unsupported stream version {ver} at {pos}")
        seg_table = data[pos + 27:pos + 27 + n_seg]
        if len(seg_table) != n_seg:
            raise SystemExit("ogg: truncated segment table")
        head_end = pos + 27 + n_seg
        body = sum(seg_table)
        page = (data[pos:pos + 22] + b"\0\0\0\0" + data[pos + 26:head_end]
                + data[head_end:head_end + body])
        crc = ogg_crc(page)
        if crc != crc_stored:
            raise SystemExit(f"ogg: page CRC mismatch at seq {seq} "
                             f"(stored 0x{crc_stored:08X}, computed 0x{crc:08X})")
        if serial is None:
            if not (flags & 0x02):
                raise SystemExit("ogg: first page lacks the BOS flag")
            serial = my_serial
        elif my_serial != serial:
            raise SystemExit("ogg: stream serial changed mid-file")
        if seq != expect_seq:
            raise SystemExit(f"ogg: page sequence gap ({seq} after {expect_seq - 1})")
        expect_seq += 1
        pages += 1
        off = head_end
        for lacing in seg_table:
            chunk = data[off:off + lacing]
            off += lacing
            if partial is not None:
                partial += chunk
            else:
                partial = chunk
            if lacing < 255:
                packets.append(bytes(partial))
                partial = None
        pos = off
        if partial is not None and flags & 0x04:
            # EOS with an unterminated packet: a truncated final packet.
            raise SystemExit("ogg: EOS page ends mid-packet")
    if partial is not None:
        raise SystemExit("ogg: file ends with an unterminated packet")
    return packets, serial, pages


# ---------------------------------------------------------------------------
# Opus TOC decoding (RFC 6716 §3.2.1) + OpusHead (§3.3.1)
# ---------------------------------------------------------------------------

# TOC rules per RFC 6716 Table 2 — re-read from the fetched RFC text
# (2026-09-30), not recalled. That check is what caught the draft table's
# double failure on measured encoder output: a draft ordering read the real
# config 31 as "reserved" (rejecting 501 valid CELT-FB packets) and mislabelled
# the config layout around it. Measured on this host's libopus output: config 9
# = SILK-only WB 20 ms (voip arm), config 31 = CELT-only FB 20 ms (music arm).
# NOTE (measured 2026-09-30): the vendored decoder's own `bandwidth_from_toc`
# deviates from the RFC for CELT bandwidths — it maps bits 0/1/2/3 to
# MB/WB/SWB/FB where Table 2 says NB/WB/SWB/FB (off by one, affecting configs
# 16–19). The embedded arms pin configs 9 and 31, where the vendored reading
# and the RFC agree; the host witness (this PR) compares against ffmpeg's
# reference decode, so a mode/bandwidth misreading on any packet shows up as a
# decode mismatch, never silently.

def toc_decode(toc_byte):
    config = toc_byte >> 3
    if toc_byte & 0x80:
        mode = "CELT-only"
        bw = ("NB", "WB", "SWB", "FB")[(toc_byte >> 5) & 3]
        dur = (2.5, 5, 10, 20)[config & 3]
    elif toc_byte & 0x60 == 0x60:
        mode = "Hybrid"
        bw = ("SWB", "FB")[(toc_byte >> 4) & 1]
        dur = (10, 20)[config & 1]
    else:
        mode = "SILK-only"
        bw = ("NB", "MB", "WB")[config >> 2]
        dur = (10, 20, 40, 60)[config & 3]
    return config, mode, bw, dur, toc_byte & 3


def parse_opus_head(packet):
    if packet[:8] != b"OpusHead":
        raise SystemExit("first packet is not OpusHead")
    version, channels = packet[8], packet[9]
    pre_skip, input_rate, gain, family = struct.unpack_from("<HiBB", packet, 10)
    if version != 1:
        raise SystemExit(f"OpusHead version {version} unsupported")
    if family != 0:
        raise SystemExit(f"mapping family {family} unsupported (mono profile pins family 0)")
    return {"version": version, "channels": channels, "pre_skip": pre_skip,
            "input_sample_rate": input_rate, "gain": gain, "family": family}


# ---------------------------------------------------------------------------
# Arm specification + fail-closed acceptance
# ---------------------------------------------------------------------------

ARMS = {
    "silk": {
        "label": "opus_silk",
        "why": "SILK-only speech arm: libopus -application voip at the 48 kHz "
               "API rate (census records the mode/bandwidth it actually chose); "
               "the speech-first profile's decode cost incl. the internal resampler",
        "rate": 48000,
        "accept_mode": "SILK-only",
        "accept_configs": {1, 5, 9},  # SILK-only 20 ms (NB/MB/WB)
    },
    "music": {
        "label": "opus_music",
        "why": "music arm: libopus -application audio at API 48 kHz FB 20 ms — "
               "the mode libopus actually emits at this profile (census records it)",
        "rate": 48000,
        # Acceptance is "whatever single config the encoder pinned, census-
        # recorded"; the fail-closed rules below reject drift (multiple
        # configs, wrong duration, multi-frame packets), not a pre-recall.
        "accept_mode": None,
        "accept_configs": None,
    },
}


def census_arm(arm, packets, head):
    """Walk audio packets; return (census_str, config). Rejects anything the
    measurement profile cannot stand: multi-frame packets (the product pins
    one frame per packet), non-20 ms frames, config drift within an arm,
    and — for the SILK arm — any non-SILK mode."""
    spec = ARMS[arm]
    counts = {}
    for i, pkt in enumerate(packets):
        if len(pkt) < 1:
            raise SystemExit(f"{arm}: empty packet {i}")
        config, mode, bw, dur, code = toc_decode(pkt[0])
        if code != 0:
            raise SystemExit(
                f"{arm}: packet {i} carries {code + 1} frames (code {code}); "
                "the profile pins one frame per packet — re-encode or widen the manifest")
        if dur != 20:
            raise SystemExit(f"{arm}: packet {i} frame duration {dur} ms, not 20")
        key = (config, mode, bw, dur)
        counts[key] = counts.get(key, 0) + 1
    if len(counts) != 1:
        raise SystemExit(
            f"{arm}: profile not pinned — {len(counts)} distinct TOC configs "
            f"{sorted((c[0], c[1], c[3]) for c in counts)}; the pack refuses to "
            "write an arm whose decode cost is a blend")
    (config, mode, bw, dur), n = next(iter(counts.items()))
    if spec["accept_mode"] is not None and mode != spec["accept_mode"]:
        raise SystemExit(f"{arm}: census mode {mode}, expected {spec['accept_mode']} "
                         f"(the arm's whole point — refusing a degenerate comparison)")
    if spec["accept_configs"] is not None and config not in spec["accept_configs"]:
        raise SystemExit(f"{arm}: config {config} outside the accepted set "
                         f"{sorted(spec['accept_configs'])}")
    if head["pre_skip"] < 0 or head["pre_skip"] >= dur * spec["rate"] // 1000:
        raise SystemExit(f"{arm}: pre_skip {head['pre_skip']} outside one frame")
    dur_label = int(dur) if dur == int(dur) else dur
    census = f"config {config} {mode} {bw} {dur_label} ms x{n} packets (code 0, one frame each)"
    return census, config


# ---------------------------------------------------------------------------
# Emission
# ---------------------------------------------------------------------------

def rustdoc(w, text):
    for line in text.splitlines():
        w("    /// " + line if line.strip() else "    ///")


def emit(crate, encoder_version, per_arm):
    lines = []
    w = lines.append
    w("//! opus_spike embedded clips — GENERATED")
    w("//! DO NOT EDIT BY HAND.  Regenerate:  scripts/gen_opus_assets.sh")
    w("//! Generator: scripts/opus_assets.py opus_assets (Ogg demux with CRC-32")
    w("//! verification, fail-closed TOC censuses; OPUS.md 'Measurement plan').")
    w(f"//! Encoder: {encoder_version}")
    w("//!")
    w("//! sha256 pins (regeneration tripwires; the witness tests carry the")
    w("//! FNV-1a pins in code, these are for humans reviewing a regen diff):")
    for arm in per_arm:
        w(f"//!   {arm['packets_file']:28s} region {sha256(arm['packets_bytes'])}")
        w(f"//!   {arm['ref_file']:28s} pcm    {sha256(arm['ref_bytes'])}")
        w(f"//!   census({arm['label']}): {arm['census']}")
    w("//!")
    w("//! Offsets are BYTE OFFSETS WITHIN THE EMBEDDED PACKET REGION (packet 0")
    w("//! at 0) — the product manifest's (offset, length) index, OPUS.md.")
    w("//! Unlike the FLAC gate these packets are NOT independently decodable:")
    w("//! Opus decode is a state machine, so the walk is sequential per clip.")
    w("//!")
    w("//! fnv_* are FNV-1a 64-bit: fnv_packets over the region bytes, fnv_ref_pcm")
    w("//! over the reference PCM bytes (32-bit float LE mono, ffmpeg/libopus")
    w("//! pre-skip applied — host ground truth, never embedded), and")
    w("//! fnv_walk_fold over the VENDORED WALK's own folded output (i16 LE,")
    w("//! half-up grid, all walk_samples — the ROM decode proof's golden;")
    w("//! measured over the dump_walk output, i.e. the production seam).")
    w("")
    w("/// One packet's placement in the region: byte offset and length.")
    w("/// `Clone + Copy`: two plain ints.")
    w("#[derive(Clone, Copy)]")
    w("pub struct PacketMeta {")
    rustdoc(w, "Byte offset of the packet within the embedded region.")
    w("    pub offset: u32,")
    rustdoc(w, "Packet length in bytes (TOC byte included).")
    w("    pub len: u16,")
    w("}")
    w("")
    w("/// One embedded arm: metadata, the packet index, the `include_bytes!`")
    w("/// region, and the hash pins. The reference PCM lives only in the")
    w("/// `assets/*_ref.bin` files (host-test ground truth) — the ROM image")
    w("/// never embeds reference data.")
    w("/// `Clone + Copy`: every field is a plain int or a `'static` reference —")
    w("/// the negative-control tests rebuild a corrupt clip by struct-update.")
    w("#[derive(Clone, Copy)]")
    w("pub struct OpusClip {")
    rustdoc(w, "Arm label for logs.")
    w("    pub name: &'static str,")
    rustdoc(w, "What this arm measures (generated; logged at ROM boot).")
    w("    pub why: &'static str,")
    rustdoc(w, "Measured TOC census (generator refuses a broken census).")
    w("    pub census: &'static str,")
    rustdoc(w, "RFC 6716 TOC config the census proved for every packet.")
    w("    pub toc_config: u8,")
    rustdoc(w, "Decoder API sample rate in Hz (the profile pins one per arm).")
    w("    pub sample_rate_hz: u32,")
    rustdoc(w, "Samples per decoded frame at the API rate (20 ms pinned).")
    w("    pub frame_samples: u16,")
    rustdoc(w, "Channel count (1 for both arms — the mono profile).")
    w("    pub channels: u8,")
    rustdoc(w, "OpusHead pre-skip in samples at the API rate: the encoder")
    rustdoc(w, "delay the reference PCM has ALREADY removed. NOT necessarily")
    rustdoc(w, "the walk→reference alignment — see `align_shift` (measured")
    rustdoc(w, "2026-10-02: the SILK arm aligns 3 samples earlier).")
    w("    pub pre_skip: u16,")
    rustdoc(w, "MEASURED walk→reference alignment in samples: the shift")
    rustdoc(w, "where the folded vendored walk meets the folded reference")
    rustdoc(w, "(generator `discover_alignment` over the dump_walk output;")
    rustdoc(w, "the witness re-derives it from the production walk and the")
    rustdoc(w, "two implementations must agree).")
    w("    pub align_shift: u16,")
    rustdoc(w, "Fold-grid mismatches at `align_shift` on the round-half-up")
    rustdoc(w, "grid (the measured equivalence class — NOT the vendored")
    rustdoc(w, "truncate): the witness asserts exactly this count and every")
    rustdoc(w, "residual ≤ 1 LSB. Measured 2026-10-09 on the strict")
    rustdoc(w, "(host-simd OFF) equivalence class: SILK 0; CELT 126 of")
    rustdoc(w, "480,000 (soft-float drift crossing grid boundaries; the")
    rustdoc(w, "pre-patch-5 NEON host measured 127 — see OPUS.md).")
    w("    pub fold_mismatch: u32,")
    rustdoc(w, "Packet count.")
    w("    pub packets: usize,")
    rustdoc(w, "Total samples a full packet walk produces at the API rate")
    rustdoc(w, "(packets x frame_samples, before pre-skip alignment).")
    w("    pub walk_samples: u32,")
    rustdoc(w, "Reference PCM bytes (host ground truth; 32-bit float LE mono).")
    rustdoc(w, "The witness folds BOTH sides with the crate's shared")
    rustdoc(w, "`checksum::fold_f32_to_i16` (round half up) and compares on the")
    rustdoc(w, "i16 grid from `align_shift` — the measured equivalence class")
    rustdoc(w, "of float decoders (step-2 findings 2026-10-02: raw f32 is NOT")
    rustdoc(w, "bit-exact; per-arm `fold_mismatch` counts the residuals).")
    w("    pub ref_bytes_len: u32,")
    rustdoc(w, "FNV-1a 64 of `region`.")
    w("    pub fnv_packets: u64,")
    rustdoc(w, "FNV-1a 64 of the reference PCM bytes (f32-LE; NOT embedded).")
    w("    pub fnv_ref_pcm: u64,")
    rustdoc(w, "FNV-1a 64 of the VENDORED WALK's folded output (every")
    rustdoc(w, "walk sample folded through the shared round-half-up grid,")
    rustdoc(w, "i16 LE bytes; no alignment shift). The ROM decode proof's")
    rustdoc(w, "golden: the port is NOT bit-exact with the reference, so the")
    rustdoc(w, "ROM pins its own decoder's deterministic output, and the")
    rustdoc(w, "reference comparison stays the host witness's layer")
    rustdoc(w, "(OPUS.md 2026-10-03: the FLAC fnv_pcm pattern does not")
    rustdoc(w, "transfer to a lossy codec).")
    w("    pub fnv_walk_fold: u64,")
    rustdoc(w, "File name (under `assets/`) of the reference PCM blob.")
    w("    pub ref_file: &'static str,")
    rustdoc(w, "The packet index: packet 0 at offset 0, ascending, tiling exactly.")
    w("    pub index: &'static [PacketMeta],")
    rustdoc(w, "The raw packet region — what `include_bytes!` embeds in the ROM.")
    w("    pub region: &'static [u8],")
    w("}")
    w("")
    for arm in per_arm:
        w(f"const {arm['label'].upper()}_INDEX: &[PacketMeta] = &[")
        for (off, ln) in arm["index"]:
            w(f"    PacketMeta {{ offset: {off}, len: {ln} }},")
        # `];` closes a slice literal — `};` (the struct-initializer habit)
        # is a mismatched-delimiter lexer error (compile gate, 2026-10-01).
        w("];")
        w("")
        w(f"/// {arm['label']} — {arm['census']}.")
        w(f"pub const {arm['label'].upper()}: OpusClip = OpusClip {{")
        w(f"    name: {rust_str(arm['label'])},")
        w(f"    why: {rust_str(arm['why'])},")
        w(f"    census: {rust_str(arm['census'])},")
        w(f"    toc_config: {arm['config']},")
        w(f"    sample_rate_hz: {arm['rate']},")
        w(f"    frame_samples: {arm['frame_samples']},")
        w("    channels: 1,")
        w(f"    pre_skip: {arm['pre_skip']},")
        w(f"    align_shift: {arm['align_shift']},")
        w(f"    fold_mismatch: {arm['fold_mismatch']},")
        w(f"    packets: {arm['n_packets']},")
        w(f"    walk_samples: {arm['walk_samples']},")
        w(f"    ref_bytes_len: {len(arm['ref_bytes'])},")
        # Rust hex literals are `0x`-lowercase — `:#018X` emits `0X`, a lexer
        # error (generated-Rust discipline: the compile gate is the reviewer).
        w(f"    fnv_packets: {arm['fnv_packets']:#018x},")
        w(f"    fnv_ref_pcm: {arm['fnv_ref_pcm']:#018x},")
        w(f"    fnv_walk_fold: {arm['fnv_walk_fold']:#018x},")
        w(f"    ref_file: {rust_str(arm['ref_file'])},")
        w(f"    index: {arm['label'].upper()}_INDEX,")
        w(f"    region: include_bytes!(\"../assets/{arm['packets_file']}\"),")
        w("};")
        w("")
    w("/// Both arms, in gate order (speech first — the decision hinges on it).")
    w("pub const CLIPS: &[&OpusClip] = &[&OPUS_SILK, &OPUS_MUSIC];")
    w("")

    out = os.path.join(crate, "src", "assets.rs")
    with open(out, "w") as f:
        f.write("\n".join(lines))
    print(f"  wrote {out}")


# ---------------------------------------------------------------------------

def opus_assets(tmp, crate, encoder_version):
    os.makedirs(os.path.join(crate, "assets"), exist_ok=True)
    # The crate whose dump_walk example measures the pins is the repo's crate
    # — the output `crate` may be a scratch directory for verification runs
    # (override with OPUS_SPIKE_CRATE when vendoring elsewhere).
    spike_src = os.environ.get("OPUS_SPIKE_CRATE") or os.path.normpath(
        os.path.join(os.path.dirname(os.path.abspath(__file__)),
                     "..", "examples", "opus_spike"))
    host = host_triple()
    per_arm = []
    for arm in ("silk", "music"):
        spec = ARMS[arm]
        label = spec["label"]
        ogg = os.path.join(tmp, f"{arm}.opus")
        packets, serial, pages = read_ogg_packets(ogg)
        print(f"  {label}: {pages} pages, {len(packets)} packets (serial {serial})")
        head = parse_opus_head(packets[0])
        if len(packets) < 2 or packets[1][:8] != b"OpusTags":
            raise SystemExit(f"{label}: second packet is not OpusTags")
        audio = packets[2:]
        if head["channels"] != 1:
            raise SystemExit(f"{label}: OpusHead channels {head['channels']}, mono profile pins 1")
        census, config = census_arm(arm, audio, head)
        print(f"  {label}: {census}")
        print(f"  {label}: pre_skip={head['pre_skip']} input_rate={head['input_sample_rate']}")

        region = b"".join(audio)
        offset = 0
        index = []
        for pkt in audio:
            index.append((offset, len(pkt)))
            offset += len(pkt)
        path = os.path.join(crate, "assets", f"{label}_packets.bin")
        with open(path, "wb") as f:
            f.write(region)
        ref_path = os.path.join(tmp, f"{arm}_ref.f32le")
        ref_bytes = open(ref_path, "rb").read()

        frame_samples = spec["rate"] * 20 // 1000
        walk = len(audio) * frame_samples
        pre_skip = head["pre_skip"]
        ref_samples = len(ref_bytes) // 4  # f32-LE ground truth

        # The vendored decoder's own walk over these exact staged bytes —
        # measured pins, never recalled. The 2026-09-29 draft claimed the
        # walk [pre_skip ..] is always the reference's prefix; the 2026-10-02
        # witness run proved that FALSE for the SILK arm (true alignment:
        # pre_skip − 3 — the old draft guard only checked coverage, so it
        # passed while the witness compared 478,439/480,000 misaligned
        # samples).
        walk_bytes = dump_vendored_walk(
            spike_src, host, tmp, arm, path, [ln for (_off, ln) in index])
        if len(walk_bytes) % 4 != 0 or len(walk_bytes) // 4 != walk:
            raise SystemExit(
                f"{label}: walk dump {len(walk_bytes) // 4} samples, manifest says "
                f"{walk} (packets x frame) — the dump and the manifest disagree")

        # Alignment + fold tolerance, MEASURED (see discover_alignment).
        # Fail-closed on drift, three ways:
        #   - the measured shift stays within 16 samples of pre_skip —
        #     further away is a toolchain change, not a delay difference;
        #   - every grid mismatch is exactly ±1 LSB (the float-drift band:
        #     the port's soft-float path differs from libopus by < 1 LSB) —
        #     anything larger is a decode divergence, not rounding;
        #   - the mismatch count stays under 1% of samples — a drifting
        #     fraction means a mode/geometry change, not residual drift.
        # Measured 2026-10-02 (ffmpeg 9.0.1, vendored opus-rs 0.1.34, round
        # half up): silk shift 309 (= pre_skip − 3), 0 mismatches; music
        # shift 312 (= pre_skip), 127 mismatches, all ±1 LSB.
        # Re-measured 2026-10-09 through the strict default (vendor patch 5:
        # host-simd feature OFF = the thumbv4t scalar equivalence class):
        # music 126 mismatches — the fused NEON tails accounted for one
        # grid crossing of the 2026-10-02 count. Silk unchanged at 0.
        align_shift, fold_mismatch, fold_max_lsb = discover_alignment(
            walk_bytes, ref_bytes, pre_skip)
        if abs(align_shift - pre_skip) > 16:
            raise SystemExit(
                f"{label}: alignment failed closed — measured shift {align_shift} is "
                f"{align_shift - pre_skip} from OpusHead pre_skip {pre_skip}; re-measure "
                "the trim/delay convention before trusting any comparison")
        if fold_max_lsb > 1:
            raise SystemExit(
                f"{label}: fold grid failed closed — {fold_mismatch} mismatches with "
                f"max |delta| {fold_max_lsb} LSB > 1; a delta beyond the float-drift "
                "band is a decode divergence, not rounding")
        if fold_mismatch > ref_samples // 100:
            raise SystemExit(
                f"{label}: fold tolerance failed closed — {fold_mismatch} mismatches "
                f"exceed 1% of {ref_samples} samples; suspect mode/drift change, "
                "not residual drift")
        print(f"  {label}: align_shift={align_shift} (pre_skip {pre_skip}), "
              f"fold mismatches={fold_mismatch}, max |delta|={fold_max_lsb} LSB")
        # Coarse geometry sanity, re-expressed at the measured alignment:
        # the walk covers the reference and over-covers by at most 2 frames
        # of encoder padding (measured both arms 2026-09-30: 501 x 960 =
        # 480,960 walk vs 480,000 reference — over-coverage, never under).
        if walk < align_shift + ref_samples or walk - align_shift - ref_samples > 2 * frame_samples:
            raise SystemExit(
                f"{label}: alignment failed closed — walk {walk} samples, align_shift "
                f"{align_shift}, reference {ref_samples} samples; ffmpeg's "
                "pre-skip/tail trimming no longer matches the recorded convention, "
                "re-measure before trusting the comparison")

        per_arm.append({
            "label": label, "why": spec["why"], "census": census, "config": config,
            "rate": spec["rate"], "frame_samples": frame_samples,
            "pre_skip": pre_skip, "n_packets": len(audio),
            "walk_samples": walk, "ref_bytes": ref_bytes,
            "align_shift": align_shift, "fold_mismatch": fold_mismatch,
            "packets_bytes": region, "index": index,
            "packets_file": f"{label}_packets.bin",
            "ref_file": f"{label}_ref.bin",
            "fnv_packets": fnv1a64(region), "fnv_ref_pcm": fnv1a64(ref_bytes),
            "fnv_walk_fold": fold_walk_fnv(walk_bytes),
        })

    for arm in per_arm:
        with open(os.path.join(crate, "assets", arm["ref_file"]), "wb") as f:
            f.write(arm["ref_bytes"])
    emit(crate, encoder_version, per_arm)


if __name__ == "__main__":
    if len(sys.argv) != 4 or sys.argv[1] != "opus_assets":
        raise SystemExit(__doc__)
    encoder = os.environ.get("OPUS_ENCODER_VERSION", "libopus (version unset)")
    opus_assets(sys.argv[2], sys.argv[3], encoder)
