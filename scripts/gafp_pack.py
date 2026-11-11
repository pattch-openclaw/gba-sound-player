#!/usr/bin/env python3
"""The GAFP packer — a real writer, not a stand-in (plan of record, item 6b-2).

`GAFP` is the container `crates/flac-lite` decodes straight out of ROM: a
little-endian manifest, a frame-offset table, then verbatim libFLAC frames. Its
byte layout is the contract in `crates/flac-lite/README.md` → "GAFP container
format", and `crates/flac-lite/src/format.rs::Manifest::parse` is the reader.
Before this file the only writer was `scripts/gen_gafp_blob.py`, which 6b-1
explicitly called a stand-in: it re-cut the frames the perf spike had already
committed and re-used the offsets that generator had already derived. 6b-1 recorded
what that leaves owing — "the real packer's first differential is that it produces
a byte-identical GAFP for the same frames" — and that differential is gate
`gafp-pack-check` step 1 here: the blob this packer builds for the perf clip from
its own source PCM is compared, byte for byte, against
`examples/flac_integration/assets/spike_l0.gfp`.

What "real" means, concretely:

  1. **It derives everything from the bytes.** Frame starts come from the strict
     RFC 9639 §9.1 header parse plus a verifying header CRC-8 — `frame_vectors.py`'
     `find_frames`, the walk `measure` proves exact over 979 real frames. Not a
     bare `0xFF F8` sync scan (measured over-match 1.1x–9.4x), and never filtered
     on "blocksize == the track's blocksize", because the last frame of any track
     whose length is not a whole multiple of the block is legitimately shorter.
  2. **It reads the subframes, not just the headers.** `walk_asset` below walks
     every subframe slot of every frame it is about to pack — type code, wasted
     bits, warm-up, LPC precision/shift/coefficients, the residual partition
     structure, every Rice codeword, the escape records — and requires each frame's
     exit, padded to a byte and followed by its 16-bit footer, to land exactly on
     the next frame's start. That walk is what makes the *profile* guard able to
     say anything, because the one thing a GAFP header cannot state is whether the
     stream contains LPC subframes (see "Why the header field is not the guard").
  3. **It refuses at pack time.** The encode profile is not a comment here: it is
     a verdict over the frames, with a cited measured cost per predictor kind
     (`MEASURED_DECODE`), and it refuses what the target cannot play before it
     writes anything. `flac -d` is then run against the packed stream and compared
     with the PCM that went in, so the frames in the blob are the reference
     decoder's own words about the source, not this script's.
  4. **It re-reads what it wrote.** `audit_blob` parses the finished bytes back
     through an independent reader: header fields, table ordering and tiling,
     region bytes equal to the frames that went in, and every slice
     `[offsets[i] .. offsets[i+1]]` strict-parsing to exactly one frame with a
     verifying CRC-8 and CRC-16. A blob is not accepted because it was written.

Usage:
  scripts/gafp_pack.py [OPTIONS] INPUT OUTPUT.gfp

  INPUT is a `.flac` (packed as it stands — nothing is re-encoded, so the packer
  cannot accidentally change the frames) or audio to be encoded first (`.wav`,
  `.aiff`, `.au`): normalised with ffmpeg only when it does not already match the
  profile, then encoded with the reference command.

Options:
  -r, --rate HZ            target sample rate            (default 32000)
  -b, --blocksize N        samples per subframe, fixed   (default 2048)
  -L, --max-lpc-order N    libFLAC `-l` value            (default 0 = LPC disabled;
                           the measured product rows cover FIXED-0 only)
      --allow-lpc          let real LPC subframes above `-L` through the profile
                           guard (a NON-PRODUCT blob)
      --allow-unmeasured   let predictor kinds or residual shapes with no
                           measured figure at the target rate through the guard
                           (a NON-PRODUCT blob)
      --allow-over-budget  let a predictor kind that IS measured and IS over the
                           measured-cost bar through the guard (a NON-PRODUCT
                           blob)
      --no-midside         do not pass `-m`
      --channels N         1 or 2; default = the input's own channel count
      --no-verify-pcm      skip the `flac -d` vs input-PCM bit-exactness check
      --expect REF.gfp     require the built bytes to equal REF byte for byte
      --check              build in memory and compare with OUTPUT (tripwire)
      --emit-facts TABLE   also write the per-frame/per-subframe facts table
                           `tests/gafp_asset_facts.rs` reads
      --keep-work          keep the temporary working directory and print it

Three separate knobs, on purpose
-------------------------------
`-L` / `--allow-lpc`, `--allow-unmeasured` and `--allow-over-budget` answer three
different questions, and a build that means one must not quietly get another:

  * `-L` is what libFLAC is asked to use, and the profile limit against which
    real subframe orders are checked. "Outside the encode profile we asked for."
    Asking for `-l 4` at a rate where the measured row is over budget is refused
    by `preflight` before a single byte is encoded — the request itself is the
    thing that cannot be satisfied on this target.
  * `--allow-unmeasured` is for a predictor kind this table has no row for, or a
    residual shape outside the envelope its row was measured on. Unmeasured is not
    the same as safe; the plan says so, so the default is to refuse and name the
    nearest measured row instead of inventing a figure.
  * `--allow-over-budget` is for a kind that HAS been measured on hardware at this
    target and is over the decode-half bar — `-l 4` at 32 kHz/2048 is 174.14% of
    the frame for the decode half alone, which is why C2 excludes LPC. The
    measured product row is FIXED-0: `-l 0` disables LPC, but libFLAC still
    chooses FIXED orders 1-4 from the signal, and those orders have no measured
    row here. A real track that lands on FIXED-1..4 is therefore refused, not
    silently played; the product profile is FIXED-0-only in practice.

Whether a blob is a product asset is derived (`Profile.is_product`), not requested:
the C2 profile is the default rate, the default block size and `-l 0`, with no
relaxation. Mono, independent-stereo and mid/side assets built at that profile are
all product assets *if their frames are FIXED-0 and inside the measured residual
envelope*; a `-L 4` asset is not, with or without a relaxation flag. Each
relaxation that was used is recorded in the facts table (`blob relaxations=...`),
so a NON-PRODUCT blob says so in the file that describes it.

Exit status: 0 on a written (or verified) blob; 1 on a profile refusal or any
failed cross-check; 2 on usage/dependency problems.

Dependencies: `flac`/`metaflac` (encode + reference decode + block listing),
`python3` (stdlib), and `ffmpeg` only when the input needs resampling.
`scripts/frame_vectors.py` (stdlib only) supplies the frame walk, the strict
header parser, CRC-8, the bit reader and the deterministic test-PCM synthesis —
reused, not rewritten, as `scripts/pack_flac.sh`'s stub always intended.

Determinism: the *bytes* of the frame region are libFLAC's, so a blob depends on
the libFLAC version; the version is recorded in the facts table. Everything the
packer itself contributes — the header, the offset table, the ordering — is
derived from those bytes and is stable.
"""

import hashlib
import os
import re
import shutil
import struct
import subprocess
import sys
import tempfile
import wave

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
sys.path.insert(0, HERE)

import frame_vectors as fv  # noqa: E402  (the shared harness library)

# ------------------------------------------------------------------ container

MAGIC = b"GAFP"
VERSION = 1
HEADER_LEN = 0x1C

# The container contract's own limits (README.md → "GAFP container format"). The
# packer enforces them because it is the thing that gets to write them.
MAX_FRAMES = 0xFFFF
CHANNELS_ALLOWED = (1, 2)
BPS_ALLOWED = (16,)

# ---------------------------------------------------------------- the profile
#
# `EncodeProfile` in `crates/flac-lite/src/format.rs` is the same object: allowed
# block sizes, allowed bit depths, a maximum predictor order, and whether full LPC
# is permitted at all. `Manifest::validate`/`EncodeProfile::validate` can only
# check header fields. This file can also see the frames, so the profile guard
# lives here — and the measured half of it has to live here, because the cost of
# a predictor kind at a sample rate is a hardware fact, not a decoder fact.
#
# Why the header field is not the guard
# -------------------------------------
# The manifest's `0x07` byte is "max FIXED predictor order used". A `-l 4` encode
# of real audio is mostly *LPC* subframes, and its maximum FIXED order is whatever
# the rare non-LPC frame happened to use — 0 on the perf clip's `-l 4` arm. So the
# field reads 0 for a stream that is nearly all LPC, and a header-only profile
# check passes it. Measured here rather than asserted (2026-11-11: the `-l 4` arm
# of `scripts/gen_spike_assets.sh`, regenerated from the deterministic
# synthesizer — 157 frames, 314 subframe slots walked by `walk_asset` below):
# the census is `lpc4` x312 + `lpc3` x2, and `max_fixed_order_used` over the same
# walk is 0, which is exactly the value `format::DEFAULT_PROFILE`'s order cap is
# written to police. Note also that the `.flac` files are *generator inputs*, not
# tracked files (only the `*_frames.bin` / `*_pcm.bin` regions are committed, e.g.
# `examples/flac_spike/assets/spike_l4_frames.bin`), so a 6b-2 witness cannot
# depend on a checked-in `.flac`: this packer writes the fixture it needs.
#
# What is allowed, and on what evidence
# ------------------------------------
# `MEASURED_DECODE` below records decode cost *as measured on GBA hardware* for a
# specific (stream rate, block size, predictor kind/order) configuration, as a
# share of the frame it has to fit in. The guard has three verdicts, and the
# third one is not a permission: a configuration with no measured row is refused
# too, because "not yet measured" is not "fits". `--allow-unmeasured` says
# otherwise out loud, and the output is then reported as a NON-PRODUCT blob.
#
# The rows are FIXED-0 and LPC-4 only. `-l 0` disables LPC, not libFLAC's FIXED
# order choice: FIXED-1..4 have no row and are refused. Each row also states the
# residual envelope it was measured on (partition count, wasted bits, escape
# partitions, Rice parameter), and the guard refuses a stream outside it — the
# per-codeword loop is most of the cost, so the kind/order alone cannot inherit
# the figure.
#
# Sources: flac-docs/measurements.md, "6a-7b on hardware: the stack and the
# shortfall" (GBA 001 + GBA SP, two runs each, all four identical to the cycle;
# the `asm` arm of that table is the `rice-arm` kernel the product path uses, and
# it is *kernel-only* — see the row's own note), and the 6b-1 record in
# flac-docs/plan-of-record.md for the same clip end to end through the manifest
# facade.

MEASURED_DECODE = {
    # (stream rate, blocksize, kind, order) -> the decode half of one frame,
    # measured on hardware, as a share of the frame period the hardware gives it
    # (`frame_budget_cycles` below — the row's own `budget_cycles` is there so the
    # share arithmetic is checkable, and `check_rows()` re-derives it on every
    # run).
    (32000, 2048, "fixed", 0): dict(
        mean_pct=42.48,
        worst_pct=42.72,
        cycles_mean=445558,
        budget_cycles=1048576,
        # The residual envelope this row was measured on: one partition, no
        # wasted bits, no escape partition, and Rice parameters no larger than
        # 10. The predictor kind/order alone is not enough to inherit a decode
        # measurement — the residual walk is where most of the time goes — so a
        # stream outside this envelope is treated as unmeasured rather than
        # allowed to borrow this row. These are the largest values in the
        # measured clip, not a second measurement; anything wider is refused
        # until it is measured.
        residual=dict(partitions_max=1, wasted_max=0, escaped_max=0,
                      rice_param_max=10),
        src=("flac-docs/measurements.md, 6a-7b on hardware, `asm` arm, GBA 001 "
             "+ GBA SP, two runs each, identical to the cycle: 445,558 c/frame "
             "= 217.56 c/sample, 42.48% mean / 42.72% worst of the 32,768 Hz "
             "frame. Kernel-only: the 6b-1 product ROM measures the same clip "
             "end to end at 37.83% mean / 38.04% worst through the manifest "
             "facade, and both figures are inside the bar this module states."),
    ),
    (32000, 2048, "lpc", 4): dict(
        mean_pct=173.23,
        worst_pct=174.14,
        cycles_mean=1816753,
        budget_cycles=1048576,
        residual=dict(partitions_max=1, wasted_max=0, escaped_max=0,
                      rice_param_max=10),
        src=("flac-docs/measurements.md, 6a-7b on hardware, `asm` arm, same "
             "instrument and same frames as the row above, `-l 4` arm: "
             "1,816,753 c/frame = 173.23% mean / 174.14% worst. That arm's "
             "census is lpc4 x312 + lpc3 x2, so this is a CLIP mean and a LOWER "
             "bound on what the LPC kind alone costs per frame. Over budget by "
             "itself, before the producer's other stages exist."),
    ),
}

# A transcription slip in a row like these is the failure mode that matters, so
# every row is re-derived at run time: `cycles_mean / budget_cycles` must land on
# `mean_pct`. It is not a hypothetical guard — writing this file got the
# denominator wrong on the first pass (32,000 Hz gives 1,073,742 cycles at this
# script's clock, against 1,048,576 for the 32,768 Hz timer the product uses: a
# 2.4% difference, enough to move 42.49% to 41.49% and hide nothing). Percent.
#
# One honest caveat about the tolerance below: this file budgets with the exact
# 2^24 Hz clock, while the instrument budgets with `stats.rs::GBA_CLOCK_HZ =
# 16_780_000`, i.e. 1,048,750 cycles for the same frame — the number every
# `verdict:` line and every published share in flac-docs/measurements.md and
# perf-results.md is divided by. The two denominators differ by 174 cycles
# (0.0166%), which is under this 0.1 pp tolerance, so a row copied from either
# source re-derives and passes here; e.g. 445,558 c re-derives to 42.49% against
# this budget against the 42.48% a rounded-clock reading gives. That is why rows
# are cited by source and condition, and why the reconciliation of the two clocks
# is written down in flac-docs/method-and-gates.md before item 7-1 puts new
# shares on the same table.
ROW_CONSISTENCY_TOLERANCE_PCT = 0.1


# The guard's numeric bar, stated as a number so a reader can disagree with it:
# a kind may be packed for a target only if its measured WORST frame stays under
# this share of that target's frame. Decode half only — the producer's other
# stages (conversion, mixer, DMA) are item 7-1's measurement, and a row inside
# this bar is a necessary, not a sufficient, condition for C2.
DECODE_SHARE_LIMIT_PCT = 50.0

# What a "frame" is in cycles. The GBA's clock is exactly 2^24 = 16,777,216
# cycles/s — the rounded "16.78 MHz" that `stats.rs::GBA_CLOCK_HZ` budgets with
# is 0.0166% wider, see the note on the tolerance above — and a frame of
# `blocksize` samples at a playback rate of `r` is `blocksize / r` seconds, i.e.
# 2^24 * blocksize / r cycles.
#
# The playback rate is not the stream rate, and for the one configuration that
# has a measured C2 path they differ: a 32,000 Hz asset is played by the product
# path on a 32,768 Hz timer, whose frame period is 2^20 = 1,048,576 cycles —
# 2.4% *tighter* than the stream's own period of 1,073,742. Every percentage in
# flac-docs/measurements.md for the C2 clip is stated *at* 32,768 Hz for exactly
# that reason, and divided by the rounded-clock budget, which is 174 cycles
# looser; the rate is the shared thing, the denominator is the one this script
# has to state for itself. How samples are paced across that difference is item
# 7-1's measurement, not a pack-time question. A stream rate with no entry here
# is budgeted at its own rate — and has no measured row either, so it is refused
# as unmeasured anyway.
GBA_CYCLES_PER_SECOND = 16777216
PLAYBACK_RATE_FOR_STREAM = {32000: 32768}


def playback_rate(stream_rate):
    return PLAYBACK_RATE_FOR_STREAM.get(stream_rate, stream_rate)


def frame_budget_cycles(stream_rate, blocksize):
    return int(round(GBA_CYCLES_PER_SECOND * blocksize
                     / playback_rate(stream_rate)))


def check_rows():
    """Re-derive every published share from its own cycle figure. A hard failure,
    because a row that does not agree with the frame period it cites is a row
    nobody can trust."""
    bad = []
    for key, row in MEASURED_DECODE.items():
        derived = 100.0 * row["cycles_mean"] / row["budget_cycles"]
        if abs(derived - row["mean_pct"]) > ROW_CONSISTENCY_TOLERANCE_PCT:
            bad.append("%s: %.2f cycles / %d cycles = %.2f%%, but the row says "
                       "%.2f%%" % (key, row["cycles_mean"], row["budget_cycles"],
                                   derived, row["mean_pct"]))
        if row["budget_cycles"] != frame_budget_cycles(key[0], key[1]):
            bad.append("%s: cites %d cycles, this target's frame period is %d"
                       % (key, row["budget_cycles"],
                          frame_budget_cycles(key[0], key[1])))
    return bad


def check_guard_shapes():
    """Prove the guard's FIXED-0 and residual-envelope decisions without an
    encoder. This is a self-check, not a substitute for a hardware row: it asserts
    the *policy* that has no measured figure yet — FIXED-1..4 and shapes outside
    the measured envelope must be refused, while the measured FIXED-0 shape must
    be accepted — so a later edit cannot silently widen the guard."""
    info = {"sample_rate": 32000, "bits-per-sample": 16, "channels": 2,
            "maximum blocksize": 2048, "total samples": 2048}
    profile = Profile()

    def slot(kind, order, partitions=1, wasted=0, escaped=0, rice=10):
        return dict(kind=kind, order=order, partitions=partitions,
                    wasted=wasted, escaped=escaped, max_rice_param=rice)

    def verdict(s):
        frame = dict(number=0, blocksize=2048, rate=32000, channels=2,
                     crc_ok=True, subframes=[s])
        return profile_verdict(info, [frame], profile)

    ok, reasons, _ = verdict(slot("fixed", 0))
    assert ok, "the measured FIXED-0 shape must stay accepted: %s" % reasons
    for name, s, needle in [
        ("FIXED-1", slot("fixed", 1), "no measured decode figure"),
        ("partition order > 0", slot("fixed", 0, partitions=2),
         "residual shape outside the measured row"),
        ("wasted bits", slot("fixed", 0, wasted=1),
         "residual shape outside the measured row"),
        ("escape partition", slot("fixed", 0, escaped=1),
         "residual shape outside the measured row"),
        ("Rice p > 10", slot("fixed", 0, rice=11),
         "residual shape outside the measured row"),
    ]:
        ok, reasons, _ = verdict(s)
        assert not ok and any(needle in r for r in reasons), \
            "%s should be refused with %r, got %s" % (name, needle, reasons)


# Sample rates the container may carry, and the reason 44,100 is not among the
# ones the packer will build a product blob at: `crates/flac-lite` implements the
# eight standard 4-bit rate codes, and the 32 kHz target is the only one with a
# measured C2 path. A rate outside `MEASURED_DECODE` is refused as unmeasured.
DEFAULT_RATE = 32000
DEFAULT_BLOCKSIZE = 2048
DEFAULT_MAX_LPC_ORDER = 0
BLOCKSIZES_ALLOWED = (1024, 2048)


class Profile(object):
    """The encode profile a blob is built against, as data."""

    def __init__(self):
        self.rate = DEFAULT_RATE
        self.blocksize = DEFAULT_BLOCKSIZE
        self.max_lpc_order = DEFAULT_MAX_LPC_ORDER
        self.allow_lpc = False
        self.allow_unmeasured = False
        self.allow_over_budget = False
        self.channels = None          # None = take the input's own
        self.midside = True
        self.verify_pcm = True

    def is_product(self):
        """True only for the profile C2 is played at: the default rate and block
        size, `-l 0`, and no relaxed guard. Derived rather than a flag, because
        "non-product" is a fact about the request and not a checkbox: a blob
        built at `-L 4` is not a product asset whether or not anyone passed a
        relaxation flag, and a mono or independent-stereo asset built at the
        defaults IS — mid/side versus independent L/R is not a product axis, and
        neither is channel count, which is why neither appears here.

        This says what was *requested*, not what libFLAC emitted: `-l 0` is not
        FIXED-0, so `profile_verdict` still has to see the frames. A stream that
        lands on FIXED-1..4 or outside the measured residual envelope is refused
        by the default guard and never reaches a product blob."""
        return (self.rate == DEFAULT_RATE
                and self.blocksize == DEFAULT_BLOCKSIZE
                and self.max_lpc_order == DEFAULT_MAX_LPC_ORDER
                and not self.relaxations())

    def describe(self):
        return ("rate=%d blocksize=%d -l %d midside=%s channels=%s "
                "allow_lpc=%s allow_unmeasured=%s allow_over_budget=%s"
                % (self.rate, self.blocksize, self.max_lpc_order,
                   "on" if self.midside else "off",
                   self.channels if self.channels else "as-input",
                   self.allow_lpc, self.allow_unmeasured,
                   self.allow_over_budget))

    def relaxations(self):
        """Which guard was relaxed to let this through, if any. Recorded in the
        facts table, so a NON-PRODUCT blob says so in the file that describes
        it, and a Rust witness can assert which relaxation it stands on."""
        return [name for name, on in (
            ("allow-lpc", self.allow_lpc),
            ("allow-unmeasured", self.allow_unmeasured),
            ("allow-over-budget", self.allow_over_budget)) if on]


# ------------------------------------------------------------------ encoding


def deps(required_audio=False):
    missing = [t for t in ("flac", "metaflac") if shutil.which(t) is None]
    if required_audio and shutil.which("ffmpeg") is None:
        missing.append("ffmpeg")
    if missing:
        raise SystemExit(
            "gafp_pack: missing dependencies on PATH: %s" % ", ".join(missing))


def run(cmd, why):
    proc = subprocess.run(cmd, capture_output=True, text=True)
    if proc.returncode != 0:
        raise SystemExit("gafp_pack: %s failed (%s): %s"
                         % (why, " ".join(cmd),
                            (proc.stderr or proc.stdout).strip()))
    return proc.stdout


def flac_version():
    out = run(["flac", "--version"], "flac --version")
    m = re.search(r"libFLAC (\S+)", out)
    return m.group(1) if m else out.strip().splitlines()[0]


def needs_normalise(path, profile):
    """Decide, from the WAV header alone, whether ffmpeg has to run.

    Only PCM s16le wav is inspected; anything else is normalised (or refused by
    `pack_audio`). This keeps the pipeline honest: a source that already matches
    the target is passed through untouched, so a pack never quietly resamples.
    """
    try:
        with wave.open(path, "rb") as w:
            return not (w.getsampwidth() == 2
                        and w.getframerate() == profile.rate
                        and (profile.channels is None
                             or w.getnchannels() == profile.channels))
    except wave.Error:
        return True


def normalise(path, dst, profile):
    if not needs_normalise(path, profile):
        shutil.copyfile(path, dst)
        return False
    run(["ffmpeg", "-hide_banner", "-loglevel", "error", "-y", "-i", path,
         "-ar", str(profile.rate), "-sample_fmt", "s16",
         "-c:a", "pcm_s16le"]
        + (["-ac", str(profile.channels)] if profile.channels else [])
        + [dst], "sample-rate/format normalisation")
    return True


def encode(wav, dst, profile):
    """The reference encode command (README → "Reference encode command")."""
    cmd = ["flac", "-1", "-f", "-s",
           "-l", str(profile.max_lpc_order),
           "-b", str(profile.blocksize)]
    if profile.midside:
        cmd.append("-m")
    cmd += [wav, "-o", dst]
    run(cmd, "encode")
    run(["flac", "-t", "-s", dst], "encoder self-check (`flac -t`)")
    return dst


# ------------------------------------------------------------- reading frames


def read_native(path):
    """Bytes + STREAMINFO + the strict frame walk of a native `.flac`."""
    data = open(path, "rb").read()
    info = fv.streaminfo(path)
    frames = fv.find_frames(data)
    fv.check_stream(os.path.basename(path), frames, info)
    start = fv.first_frame_offset(data)
    if not frames or frames[0]["offset"] != start:
        raise SystemExit("gafp_pack: %s: the first frame found (%s) is not at "
                         "the end of the metadata blocks (%d)"
                         % (path, frames[0]["offset"] if frames else None, start))
    return data, info, frames, start


SIDE_SLOT_BY_CHANNEL_CODE = {0b1000: 1, 0b1001: 0, 0b1010: 1}


def walk_asset(data, frames, info):
    """Walk every subframe slot of every frame; return per-frame facts.

    The walk is *structural*: it reads the fields that decide a frame's shape and
    skips the payload, and its exit position is checked against the next frame's
    start on every frame. That arithmetic is the same one `spike_census` has been
    asserting for the perf gate; here it is the packer's own guarantee that the
    offset table it is about to write agrees with what the frames say about
    themselves.

    Side-slot width follows the measured seam (`frame_vectors.py`, step 3f part
    1): a decorrelated frame codes its side subframe at bps + 1.
    """
    out = []
    for i, header in enumerate(frames):
        bps = header["bps"] or info["bits-per-sample"]
        bit = header["offset"] * 8 + header["header_bits"]
        b = fv.Bits(data, bit)
        slots = []
        side_slot = SIDE_SLOT_BY_CHANNEL_CODE.get(header["chan_code"])
        for slot in range(header["subframes"]):
            slot_bps = bps + 1 if slot == side_slot else bps
            slots.append(walk_subframe(b, header["blocksize"], slot_bps))
        exit_bit = b.pos
        padded = exit_bit + ((8 - exit_bit % 8) % 8)
        end_bit = (frames[i + 1]["offset"] * 8 if i + 1 < len(frames)
                   else len(data) * 8)
        if padded + 16 != end_bit:
            raise SystemExit(
                "gafp_pack: frame %d: subframe exit %d + footer gap does not "
                "land on the next frame (expected end bit %d) — the walk and "
                "the frame table disagree about where this frame ends"
                % (header["number"], exit_bit, end_bit))
        frame_bytes = data[header["offset"]:header["offset"] + (end_bit // 8)
                           - header["offset"]]
        crc16_at = end_bit // 8 - 2
        want = int.from_bytes(data[crc16_at:end_bit // 8], "big")
        got = fv.crc16(data[header["offset"]:crc16_at])
        if want != got:
            raise SystemExit(
                "gafp_pack: frame %d: footer CRC-16 is 0x%04X, computed 0x%04X "
                "over the frame's own bytes" % (header["number"], want, got))
        facts = dict(
            index=i, number=header["number"], offset=header["offset"],
            bytes=end_bit // 8 - header["offset"], blocksize=header["blocksize"],
            rate=header["rate_hz"] or info["sample_rate"],
            channels=header["subframes"], bps=bps,
            decorrelation=header["decorrelation"], chan_code=header["chan_code"],
            header_bits=header["header_bits"], crc_ok=header["crc_ok"],
            crc16_ok=True, crc16=want, exit_bit=exit_bit,
            padded=padded, end_bit=end_bit, subframes=slots,
        )
        out.append(facts)
    return out


def walk_subframe(b, blocksize, sample_bits):
    """One subframe, fields in §9.2 order, payload skipped, facts returned.

    Field order is the one `frame_vectors.walk_subframe` reads in full and the
    one `crates/flac-lite/src/subframe.rs` decodes: pad bit, 6-bit type, the
    wasted-bits flag and its unary run, warm-up (`order` values at the subframe's
    own width), then for LPC the u(4) precision-minus-one (0b1111 forbidden), the
    s(5) shift (MUST NOT be negative), `order` signed coefficients, and the
    residual: 2-bit method, 4-bit partition order, then per partition a
    `plen`-bit Rice parameter — `escape` meaning "raw, `raw_bits` wide per
    sample" and anything else meaning `count` Rice codewords of unary-quotient +
    `param` remainder bits.
    """
    entry_bit = b.pos
    if b.read(1) != 0:
        raise SystemExit("gafp_pack: subframe pad bit set")
    code = b.read(6)
    if code == 0:
        kind, order = "constant", 0
    elif code == 1:
        kind, order = "verbatim", 0
    elif 8 <= code <= 12:
        kind, order = "fixed", code - 8
    elif code >= 32:
        kind, order = "lpc", code - 31
    else:
        raise SystemExit("gafp_pack: reserved subframe type code %s" % bin(code))
    wasted = 0
    if b.read(1):
        wasted = 1
        while b.read(1) == 0:
            wasted += 1
    width = sample_bits - wasted
    if width <= 0:
        raise SystemExit("gafp_pack: §9.2.2 violated: bps=%d wasted=%d"
                         % (sample_bits, wasted))
    b.skip(order * width)
    facts = dict(kind=kind, order=order, wasted=wasted, sample_bits=width,
                 lpc_precision=None, lpc_shift=None, partitions=0,
                 max_rice_param=0, escaped=0, raw_bits=0, codewords=0,
                 entry_bit=entry_bit, exit_bit=b.pos)
    if kind == "constant":
        b.skip(width)
    elif kind == "verbatim":
        b.skip(blocksize * width)
    else:
        if kind == "lpc":
            precision = b.read(4)
            if precision == 0b1111:
                raise SystemExit("gafp_pack: forbidden LPC precision code")
            shift = b.read_signed(5)
            if shift < 0:
                raise SystemExit("gafp_pack: §9.2.6: shift MUST NOT be "
                                 "negative (%d)" % shift)
            b.skip(order * (precision + 1))
            facts["lpc_precision"] = precision + 1
            facts["lpc_shift"] = shift
        method = b.read(2)
        if method not in (0, 1):
            raise SystemExit("gafp_pack: reserved residual method %d" % method)
        plen, escape = (4, 15) if method == 0 else (5, 31)
        porder = b.read(4)
        partitions = 1 << porder
        facts["partitions"] = partitions
        per = blocksize >> porder
        seen = 0
        for p in range(partitions):
            count = per - (order if p == 0 else 0)
            param = b.read(plen)
            if param == escape:
                raw = b.read(5)
                b.skip(count * raw)
                facts["escaped"] += 1
                facts["raw_bits"] = max(facts["raw_bits"], raw)
            else:
                facts["max_rice_param"] = max(facts["max_rice_param"], param)
                for _ in range(count):
                    q = 0
                    while b.read(1) == 0:
                        q += 1
                        if q > 800:
                            raise SystemExit("gafp_pack: runaway Rice unary run")
                    b.skip(param)
                    facts["codewords"] += 1
            seen += count
        if seen != blocksize - order:
            raise SystemExit("gafp_pack: residual covers %d samples, expected %d"
                             % (seen, blocksize - order))
    facts["exit_bit"] = b.pos
    return facts


# ------------------------------------------------------------- the guard


def _values(vals):
    """Sample values as one token, channels left to right — the shape the
    `frame_run` tables use for their expected samples, in a form a test can
    split on `:`."""
    return ":".join(str(v) for v in vals)


def frame_census(facts):
    """Tally (kind, order) over every subframe slot of the whole asset."""
    tally = {}
    for frame in facts:
        for s in frame["subframes"]:
            key = (s["kind"], s["order"] if s["kind"] in ("fixed", "lpc") else 0)
            tally[key] = tally.get(key, 0) + 1
    return tally


def residual_census(facts):
    """Worst residual shape seen for each (kind, order) in the asset.

    `MEASURED_DECODE` records a decode cost for a predictor kind/order *at a
    residual shape*: partitions, wasted bits, escape partitions and Rice
    parameters all move the per-codeword loop. The guard therefore compares the
    asset's worst slot against the row's `residual` envelope instead of letting
    the kind/order alone inherit the measurement.
    """
    out = {}
    for frame in facts:
        for s in frame["subframes"]:
            key = (s["kind"], s["order"] if s["kind"] in ("fixed", "lpc") else 0)
            row = out.setdefault(key, dict(count=0, partitions=0, wasted=0,
                                           escaped=0, max_rice_param=0))
            row["count"] += 1
            row["partitions"] = max(row["partitions"], s["partitions"])
            row["wasted"] = max(row["wasted"], s["wasted"])
            row["escaped"] = max(row["escaped"], s["escaped"])
            row["max_rice_param"] = max(row["max_rice_param"],
                                        s["max_rice_param"])
    return out


def nearest_measured(rate, blocksize, kind, order):
    """The closest measured row, so a refusal names the figure a reader would
    compare with: same rate and block size first, then the same predictor kind,
    then the nearest order."""
    def score(row_key):
        mr, mb, mk, mo = row_key
        return (0 if mr == rate else 1,
                0 if mb == blocksize else 1,
                0 if mk == kind else 1,
                abs(mo - order))
    best = min(MEASURED_DECODE, key=score) if MEASURED_DECODE else None
    return (best, MEASURED_DECODE[best]) if best else (None, None)


def profile_verdict(info, facts, profile):
    """Return (ok, reasons, report). Fail-closed: an unrecognised shape is a
    reason, not a permission."""
    reasons = []
    report = []
    census = frame_census(facts)
    residual = residual_census(facts)
    frames = facts

    def bad(msg):
        reasons.append(msg)

    if info["sample_rate"] != profile.rate:
        bad("sample rate %d Hz is not the target %d Hz"
            % (info["sample_rate"], profile.rate))
    if info["bits-per-sample"] not in BPS_ALLOWED:
        bad("bits per sample %d is outside the container's %s"
            % (info["bits-per-sample"], list(BPS_ALLOWED)))
    if info["channels"] not in CHANNELS_ALLOWED:
        bad("%d channels is outside the container's %s"
            % (info["channels"], list(CHANNELS_ALLOWED)))
    if profile.channels is not None and info["channels"] != profile.channels:
        bad("the asset has %d channels, the profile asked for %d"
            % (info["channels"], profile.channels))
    blocks = [f["blocksize"] for f in frames]
    if any(b != profile.blocksize for b in blocks[:-1]):
        bad("a non-final frame departs from the profile blocksize %d (%s)"
            % (profile.blocksize, sorted(set(blocks))))
    if max(blocks) > profile.blocksize:
        bad("a frame carries %d samples, above the profile blocksize %d"
            % (max(blocks), profile.blocksize))
    for f in frames:
        if f["rate"] != profile.rate:
            bad("frame %d resolves a sample rate of %d Hz, not %d"
                % (f["number"], f["rate"], profile.rate))
        if f["channels"] != info["channels"]:
            bad("frame %d carries %d subframes, the track has %d channels"
                % (f["number"], f["channels"], info["channels"]))
        if not f["crc_ok"]:
            bad("frame %d has a header CRC-8 that does not verify" % f["number"])
    expected = -(-info["total samples"] // info["maximum blocksize"])
    if len(frames) != expected:
        bad("%d frames where STREAMINFO implies %d" % (len(frames), expected))
    if len(frames) > MAX_FRAMES:
        bad("%d frames does not fit the manifest's u16 count" % len(frames))
    if info["maximum blocksize"] not in BLOCKSIZES_ALLOWED:
        bad("block size %d is outside the container contract %s"
            % (info["maximum blocksize"], list(BLOCKSIZES_ALLOWED)))

    # The predictor question, asked of the frames rather than of the header.
    budget = frame_budget_cycles(profile.rate, profile.blocksize)
    timer = playback_rate(profile.rate)
    for (kind, order), count in sorted(census.items()):
        row = MEASURED_DECODE.get((profile.rate, profile.blocksize, kind, order))
        label = "%s%s" % (kind, order if kind in ("fixed", "lpc") else "")
        if kind == "lpc" and not profile.allow_lpc and order > profile.max_lpc_order:
            bad("real LPC subframes (order %d) x%d: `-l %d` is the maximum LPC "
                "order the profile permits, and LPC is not what this target is "
                "built from" % (order, count, profile.max_lpc_order))
        if row is None:
            if not profile.allow_unmeasured:
                near_key, nrow = nearest_measured(
                    profile.rate, profile.blocksize, kind, order)
                hint = ("no measured decode figure for (%d Hz, %d-sample frames, "
                        "%s)" % (profile.rate, profile.blocksize, label))
                if nrow:
                    hint += (" — the nearest measured row is (%d Hz, %d, %s, "
                             "order %d): %.2f%% mean, %.2f%% worst, cited in "
                             "flac-docs/measurements.md"
                             % (near_key[0], near_key[1], near_key[2],
                                near_key[3], nrow["mean_pct"],
                                nrow["worst_pct"]))
                if kind == "fixed" and order > 0:
                    hint += (". FIXED orders 1-4 are not measured at this target: "
                             "`-l 0` disables LPC but does not force libFLAC's "
                             "FIXED order choice, so the product profile is "
                             "FIXED-0-only, not merely FIXED-only")
                bad("%s x%d: %s; measure it or pass --allow-unmeasured"
                    % (label, count, hint))
            else:
                report.append("%s x%d: UNMEASURED at %d Hz/%d — accepted only "
                              "because --allow-unmeasured was passed; this is "
                              "NOT a product blob" % (label, count, profile.rate,
                                                      profile.blocksize))
            continue
        # A measured row is valid only for the residual shape it was measured
        # on. `row["residual"]` is the envelope from the clip in the row's
        # source; anything wider is unmeasured and must be refused (or waived
        # by `--allow-unmeasured`, which marks the blob NON-PRODUCT).
        shape = residual.get((kind, order))
        limit = row.get("residual")
        if shape is not None and limit is not None:
            over = []
            if shape["partitions"] > limit["partitions_max"]:
                over.append("partitions %d > %d"
                            % (shape["partitions"], limit["partitions_max"]))
            if shape["wasted"] > limit["wasted_max"]:
                over.append("wasted=%d > %d"
                            % (shape["wasted"], limit["wasted_max"]))
            if shape["escaped"] > limit["escaped_max"]:
                over.append("escaped partitions %d > %d"
                            % (shape["escaped"], limit["escaped_max"]))
            if shape["max_rice_param"] > limit["rice_param_max"]:
                over.append("max Rice parameter %d > %d"
                            % (shape["max_rice_param"],
                               limit["rice_param_max"]))
            if over:
                msg = ("%s x%d: residual shape outside the measured row: %s. "
                       "The measured row covers partitions <= %d, wasted <= %d, "
                       "escape partitions <= %d and Rice parameter <= %d; "
                       "measure this shape or pass --allow-unmeasured"
                       % (label, count, ", ".join(over),
                          limit["partitions_max"], limit["wasted_max"],
                          limit["escaped_max"], limit["rice_param_max"]))
                if profile.allow_unmeasured:
                    report.append("%s — accepted only because "
                                  "--allow-unmeasured was passed; this is NOT "
                                  "a product blob" % msg)
                else:
                    bad(msg)
        verdict = ("measured %.2f%% mean / %.2f%% worst of the %d-cycle frame "
                   "(%d cycles for %d samples at the %d Hz playback timer, for a "
                   "%d Hz stream) — %s"
                   % (row["mean_pct"], row["worst_pct"], row["budget_cycles"],
                      row["cycles_mean"], profile.blocksize, timer,
                      profile.rate, row["src"]))
        if row["budget_cycles"] != budget:
            bad("internal: row for %s cites %d cycles, this target's frame "
                "period is %d" % (label, row["budget_cycles"], budget))
        if row["worst_pct"] > DECODE_SHARE_LIMIT_PCT:
            if not profile.allow_over_budget:
                bad("the asset uses %s x%d, and that predictor at this target "
                    "decodes at %.2f%% of the frame worst-case (bar set here at "
                    "%.1f%%, decode half only). %s The flag that writes this "
                    "anyway is --allow-over-budget, and it marks the blob "
                    "NON-PRODUCT."
                    % (label, count, row["worst_pct"], DECODE_SHARE_LIMIT_PCT,
                       verdict))
            else:
                report.append("cost bar WAIVED by --allow-over-budget for %s x%d: %s "
                              "This blob is not a C2 candidate, and the facts "
                              "table records the waiver."
                              % (label, count, verdict))
        else:
            report.append("%s x%d: %s" % (label, count, verdict))
    return (not reasons), reasons, report


def preflight(profile):
    """Refuse an impossible profile before encoding anything."""
    if profile.blocksize not in BLOCKSIZES_ALLOWED:
        raise SystemExit(
            "gafp_pack: block size %d is outside the container contract %s"
            % (profile.blocksize, list(BLOCKSIZES_ALLOWED)))
    if profile.channels is not None and profile.channels not in CHANNELS_ALLOWED:
        raise SystemExit("gafp_pack: %d channels is outside the container's %s"
                         % (profile.channels, list(CHANNELS_ALLOWED)))
    if (profile.max_lpc_order > 0 and not profile.allow_lpc
            and (profile.rate, profile.blocksize, "lpc",
                 profile.max_lpc_order) in MEASURED_DECODE):
        row = MEASURED_DECODE[(profile.rate, profile.blocksize, "lpc",
                              profile.max_lpc_order)]
        raise SystemExit(
            "gafp_pack: refusing to build this profile at all: `-l %d` means the "
            "encode may emit real LPC subframes, and LPC at %d Hz with %d-sample "
            "frames measures %.2f%% mean / %.2f%% worst of the frame on hardware "
            "— decode alone, before the producer's other stages. FIXED-only is "
            "`-l 0`, but the measured product row is FIXED-0: `-l 0` disables "
            "LPC, not libFLAC's FIXED order choice. Pass --allow-lpc to build a "
            "NON-PRODUCT blob anyway, and note that a kind measured over the "
            "decode-half bar needs --allow-over-budget as well: the profile limit "
            "and the measured cost are different questions."
            % (profile.max_lpc_order, profile.rate, profile.blocksize,
               row["mean_pct"], row["worst_pct"]))


# ------------------------------------------------------------------ writing


def max_fixed_order_used(facts):
    """The manifest's `0x07` byte: the highest FIXED order actually used.

    Read from the frames, never from the encode flags — and note what it does NOT
    say, which is the whole reason the profile guard reads the frames instead: a
    stream that is entirely LPC has a maximum FIXED order of 0.
    """
    order = 0
    for f in facts:
        for s in f["subframes"]:
            if s["kind"] == "fixed":
                order = max(order, s["order"])
    return order


def build_blob(data, facts, info, order):
    """Assemble the container bytes. The frame region is copied verbatim — the
    packer never re-encodes, never pads, never reorders."""
    region = data[facts[0]["offset"]:]
    n = len(facts)
    table_end = HEADER_LEN + 4 * n
    blob = bytearray(table_end + len(region))
    blob[0:4] = MAGIC
    blob[4] = VERSION
    blob[5] = info["channels"]
    blob[6] = info["bits-per-sample"]
    blob[7] = order
    struct.pack_into("<H", blob, 8, info["maximum blocksize"])
    struct.pack_into("<I", blob, 10, info["sample_rate"])
    struct.pack_into("<Q", blob, 14, info["total samples"])
    struct.pack_into("<H", blob, 22, n)
    struct.pack_into("<I", blob, 24, table_end)
    for i, f in enumerate(facts):
        struct.pack_into("<I", blob, HEADER_LEN + 4 * i, table_end + f["offset"]
                         - facts[0]["offset"])
    blob[table_end:] = region
    return bytes(blob), region, table_end


def audit_blob(blob, facts, data, info, order):
    """Re-read the written bytes. Nothing here is allowed to trust the writer:
    the header fields are re-read from the blob, the frame starts come from the
    blob's own table, and each slice is strict-parsed and CRC-checked in place."""
    bad = []
    if blob[0:4] != MAGIC:
        bad.append("magic")
    if blob[4] != VERSION:
        bad.append("version 0x%02X" % blob[4])
    if (blob[5], blob[6]) != (info["channels"], info["bits-per-sample"]):
        bad.append("channels/bps do not match the stream")
    if blob[7] != order:
        bad.append("recorded max FIXED order does not match the census")
    n = len(facts)
    if struct.unpack_from("<H", blob, 22)[0] != n:
        bad.append("frame count")
    if struct.unpack_from("<I", blob, 24)[0] != HEADER_LEN + 4 * n:
        bad.append("frame-data offset does not follow the table")
    if struct.unpack_from("<H", blob, 8)[0] != info["maximum blocksize"]:
        bad.append("block size")
    if struct.unpack_from("<I", blob, 10)[0] != info["sample_rate"]:
        bad.append("sample rate")
    if struct.unpack_from("<Q", blob, 14)[0] != info["total samples"]:
        bad.append("total samples")

    offsets = [struct.unpack_from("<I", blob, HEADER_LEN + 4 * i)[0]
               for i in range(n)]
    if offsets != sorted(offsets) or len(set(offsets)) != n:
        bad.append("offset table is not strictly ascending")
    if offsets and offsets[0] != HEADER_LEN + 4 * n:
        bad.append("first frame does not start where the frame data begins")
    region_start = HEADER_LEN + 4 * n
    if blob[region_start:] != data[facts[0]["offset"]:]:
        bad.append("frame region is not the frames that went in")
    # Every table slice is exactly one frame, read the way the decoder reads it:
    # strict header parse at the stated offset, then the frame's own extent, then
    # the footer. The slices must tile the region with no gap or overlap.
    ends = offsets[1:] + [len(blob)]
    for i, (a, b) in enumerate(zip(offsets, ends)):
        if b - a != facts[i]["bytes"]:
            bad.append("slice %d is %d bytes, the walk measured %d"
                       % (i, b - a, facts[i]["bytes"]))
            continue
        head = fv.parse_frame_header(blob, a)
        if head is None or not head["crc_ok"]:
            bad.append("slice %d does not begin with a valid frame header" % i)
            continue
        if head["number"] != i:
            bad.append("slice %d carries frame number %d" % (i, head["number"]))
        if head["blocksize"] != facts[i]["blocksize"]:
            bad.append("slice %d resolves a different blocksize" % i)
        if fv.crc16(blob[a:b - 2]) != int.from_bytes(blob[b - 2:b], "big"):
            bad.append("slice %d's footer CRC-16 does not verify" % i)
    total = sum(f["blocksize"] for f in facts)
    if total != info["total samples"]:
        bad.append("the frames' own block sizes sum to %d, not %d"
                   % (total, info["total samples"]))
    return bad


# ---------------------------------------------------------------- facts table


def facts_table(path, blob, sha, facts, info, profile, verdict, reasons, report,
                version, pcm):
    """The table `tests/gafp_asset_facts.rs` reads: one blob, every frame, every
    subframe slot, at the exactness the Rust walk is compared against, and the
    reference decoder's samples pinned per blob and per frame.

    Frame offsets here are RELATIVE TO THE START OF THE FRAME REGION, because
    that is what a walk over `Manifest::frame_region()` sees; the blob-absolute
    offset is recorded alongside so the table also witnesses the offset table.
    """
    n = len(facts)
    pcm_blob, pcm_per = pcm_rows(pcm, facts, info["channels"])
    w = []
    w.append("# flac-lite GAFP asset facts -- GENERATED by scripts/gafp_pack.py")
    w.append("# do not edit; regenerate with scripts/gen_gafp_fixtures.py")
    w.append("# encoder: %s" % version)
    # The profile and the verdict are data rows, not comments: the point of the
    # x1 fixture is a claim about which guard was relaxed to write it, and a test
    # should read that from the table rather than infer it from prose.
    w.append("profile rate=%d blocksize=%d max_lpc_order=%d midside=%s "
             "channels=%s allow_lpc=%s allow_unmeasured=%s "
             "allow_over_budget=%s"
             % (profile.rate, profile.blocksize, profile.max_lpc_order,
                "true" if profile.midside else "false",
                profile.channels if profile.channels else "as-input",
                "true" if profile.allow_lpc else "false",
                "true" if profile.allow_unmeasured else "false",
                "true" if profile.allow_over_budget else "false"))
    w.append("relaxations list=%s"
             % (",".join(profile.relaxations()) or "none"))
    w.append("verdict decision=%s product=%s"
             % ("accept" if verdict else "refuse",
                "true" if profile.is_product() else "false"))
    for r in reasons:
        w.append("# refused: %s" % r)
    for r in report:
        w.append("# note: %s" % r)
    w.append("blob name=%s sha256=%s bytes=%d frames=%d rate=%d blocksize=%d "
             "channels=%d bps=%d max_fixed_order=%d total_samples=%d "
             "frame_data=%d pcm_fnv1a64=%s pcm_values=%d pcm_bytes_len=%d "
             "pcm_first=%s pcm_last=%s"
             % (os.path.basename(path), sha, len(blob), n, info["sample_rate"],
                info["maximum blocksize"], info["channels"],
                info["bits-per-sample"], max_fixed_order_used(facts),
                info["total samples"], HEADER_LEN + 4 * n,
                "0x%016X" % pcm_blob["fnv"], pcm_blob["values"],
                pcm_blob["bytes_len"], _values(pcm_blob["first"]),
                _values(pcm_blob["last"])))
    region_start = HEADER_LEN + 4 * n
    for f, pin in zip(facts, pcm_per):
        w.append("frame index=%d number=%d blob_offset=%d region_offset=%d "
                 "bytes=%d blocksize=%d rate=%d subframes=%d bps=%d "
                 "decorrelation=%s header_bits=%d exit_bit=%d padded=%d "
                 "end_bit=%d crc16=%04x pcm_fnv1a64=%s pcm_values=%d "
                 "pcm_first=%s pcm_last=%s"
                 % (f["index"], f["number"], f["offset"] + region_start
                    - facts[0]["offset"], f["offset"] - facts[0]["offset"],
                    f["bytes"], f["blocksize"], f["rate"], f["channels"],
                    f["bps"], f["decorrelation"], f["header_bits"],
                    f["exit_bit"], f["padded"], f["end_bit"], f["crc16"],
                    "0x%016X" % pin["fnv"], pin["values"],
                    _values(pin["first"]), _values(pin["last"])))
        for slot, s in enumerate(f["subframes"]):
            w.append("subframe slot=%d kind=%s order=%d wasted=%d "
                     "sample_bits=%d lpc_precision=%s lpc_shift=%s "
                     "partitions=%d max_rice_param=%d escaped=%d raw_bits=%d "
                     "codewords=%d entry_bit=%d exit_bit=%d"
                     % (slot, s["kind"], s["order"],
                        s["wasted"], s["sample_bits"],
                        s["lpc_precision"] if s["lpc_precision"] is not None
                        else "none",
                        s["lpc_shift"] if s["lpc_shift"] is not None else "none",
                        s["partitions"], s["max_rice_param"], s["escaped"],
                        s["raw_bits"], s["codewords"], s["entry_bit"],
                        s["exit_bit"]))
    return "\n".join(w) + "\n"


# -------------------------------------------------------------------- pcm


def wav_samples(path):
    """The PCM that went in, as signed 16-bit values in interleaved order — the
    same shape `fv.reference_pcm` returns, so the comparison is value-for-value
    and not byte-layout-by-accident."""
    with wave.open(path, "rb") as w:
        if w.getsampwidth() != 2:
            raise SystemExit("gafp_pack: %s: expected a 16-bit reference decode, "
                             "got %d-byte samples"
                             % (path, w.getsampwidth()))
        data = w.readframes(w.getnframes())
    return list(struct.unpack("<%dh" % (len(data) // 2), data))


def pcm_rows(pcm, facts, channels):
    """Pin the reference decoder's samples per blob and per frame.

    Hashed exactly the way the existing tables hash PCM — `fv.fnv1a64` over the
    whole little-endian interleaved stream (`stream_vectors.txt`'s
    `pcm_fnv1a64`), and the same fold the Rust suites run (`left.to_le_bytes()`
    then right, per sample). Each frame's own hash starts fresh so a wrong frame
    is localized as well as detected; the blob's hash is one fold over the whole
    buffer, so a Rust loop that folds as it decodes lands on it exactly.

    The frame boundaries here come from the frames' own `blocksize` fields, and
    the sum is checked against the decoder's output — a table that pinned samples
    to the wrong frames would be worse than no table.
    """
    pcm_bytes = struct.pack("<%dh" % len(pcm), *pcm)
    blob = dict(fnv=fv.fnv1a64(pcm_bytes), values=len(pcm),
                bytes_len=len(pcm_bytes),
                first=pcm[:channels], last=pcm[-channels:])
    per = []
    pos = 0
    for f in facts:
        count = f["blocksize"] * channels
        chunk = pcm_bytes[pos:pos + count * 2]
        if len(chunk) != count * 2:
            raise SystemExit(
                "gafp_pack: frame %d wants %d samples but the reference decode "
                "has only %d values left at byte %d — the frames and the "
                "decoder's output disagree about where this frame ends"
                % (f["number"], count, len(pcm) - pos // 2, pos))
        vals = struct.unpack("<%dh" % count, chunk)
        per.append(dict(fnv=fv.fnv1a64(chunk), values=count,
                        first=list(vals[:channels]), last=list(vals[-channels:])))
        pos += count * 2
    if pos != len(pcm_bytes):
        raise SystemExit(
            "gafp_pack: the frames' own block sizes account for %d samples, the "
            "reference decoder emitted %d — refusing to write a table that pins "
            "samples to frames" % (pos // 2, len(pcm)))
    return blob, per


def verify_pcm(source_wav, ref):
    """The reference decoder against the PCM that went in. Not the encoder's own
    word: `flac -d` is a second implementation, and this is the same bit
    comparison the frame-run tables already make (`frame_vectors.reference_pcm`)."""
    want = wav_samples(source_wav)
    if ref != want:
        first = next((i for i, (a, b) in enumerate(zip(ref, want)) if a != b),
                     min(len(ref), len(want)))
        raise SystemExit(
            "gafp_pack: the packed stream does not decode to the source PCM "
            "(%d values out, %d values in; first difference at value %d: %s vs "
            "%s) — the frames in this blob are not the source's frames"
            % (len(ref), len(want), first,
               ref[first] if first < len(ref) else None,
               want[first] if first < len(want) else None))
    return len(want)


# --------------------------------------------------------------------- main


USAGE = __doc__.split("Usage:")[1].split("Dependencies:")[0].strip()


def parse_args(argv):
    profile = Profile()
    args = []
    i = 0
    while i < len(argv):
        a = argv[i]
        if a in ("-h", "--help"):
            print(USAGE)
            return None, profile, None, None, False, None
        elif a in ("-r", "--rate"):
            i += 1
            profile.rate = int(argv[i])
        elif a in ("-b", "--blocksize"):
            i += 1
            profile.blocksize = int(argv[i])
        elif a in ("-L", "--max-lpc-order"):
            i += 1
            profile.max_lpc_order = int(argv[i])
        elif a == "--channels":
            i += 1
            profile.channels = int(argv[i])
        elif a == "--allow-lpc":
            profile.allow_lpc = True
        elif a == "--allow-unmeasured":
            profile.allow_unmeasured = True
        elif a == "--allow-over-budget":
            profile.allow_over_budget = True
        elif a == "--no-midside":
            profile.midside = False
        elif a == "--no-verify-pcm":
            profile.verify_pcm = False
        elif a == "--expect":
            i += 1
            args.append(("expect", argv[i]))
        elif a == "--check":
            args.append(("check", True))
        elif a == "--emit-facts":
            i += 1
            args.append(("facts", argv[i]))
        elif a == "--keep-work":
            args.append(("keep", True))
        elif a.startswith("-"):
            raise SystemExit("gafp_pack: unknown option %s\n\n%s" % (a, USAGE))
        else:
            args.append(("pos", a))
        i += 1
    positional = [v for kind, v in args if kind == "pos"]
    opts = {k: v for k, v in args if k != "pos"}
    if len(positional) != 2:
        raise SystemExit("gafp_pack: INPUT and OUTPUT are both required\n\n%s"
                         % USAGE)
    return positional, profile, opts.get("facts"), opts.get("expect"), \
        bool(opts.get("check")), bool(opts.get("keep"))


def main(argv):
    positional, profile, facts_out, expect, check, keep = parse_args(argv)
    if positional is None:
        return 0
    src, out = positional
    for b in check_rows():
        raise SystemExit("gafp_pack: MEASURED_DECODE does not check out: %s" % b)
    try:
        check_guard_shapes()
    except AssertionError as e:
        raise SystemExit("gafp_pack: guard self-check failed: %s" % e)
    if not os.path.exists(src):
        raise SystemExit("gafp_pack: no such input: %s" % src)
    ext = os.path.splitext(src)[1].lower()
    native = ext == ".flac"
    deps(required_audio=not native)

    work = tempfile.mkdtemp(prefix="gafp-pack-")
    try:
        if native:
            source_wav = None
            packed = src
            print("gafp_pack: packing an existing encode as it stands: %s" % src)
        else:
            preflight(profile)
            audio = os.path.join(work, "audio.wav")
            normalise(src, audio, profile)
            with wave.open(audio, "rb") as w:
                print("gafp_pack: source PCM %d Hz, %d ch, %d bit, %d frames"
                      % (w.getframerate(), w.getnchannels(),
                         w.getsampwidth() * 8, w.getnframes()))
            packed = os.path.join(work, "encode.flac")
            encode(audio, packed, profile)
            source_wav = audio

        data, info, frames, _ = read_native(packed)
        print("gafp_pack: streaminfo: rate=%d ch=%d bps=%d blocksize=%d "
              "total_samples=%d" % (info["sample_rate"], info["channels"],
                                    info["bits-per-sample"],
                                    info["maximum blocksize"],
                                    info["total samples"]))
        facts = walk_asset(data, frames, info)
        print("gafp_pack: %d frames, %d subframe slots walked, every footer "
              "CRC-16 verified, every frame's exit lands on the next frame"
              % (len(facts), sum(f["channels"] for f in facts)))
        census = frame_census(facts)
        print("gafp_pack: predictor census: %s"
              % ", ".join("%s%s x%d" % (k, o if k in ("fixed", "lpc") else "", c)
                          for (k, o), c in sorted(census.items())))
        ok, reasons, report = profile_verdict(info, facts, profile)
        for r in report:
            print("gafp_pack: note: %s" % r)
        if not ok:
            print("gafp_pack: REFUSED by the encode profile (%s):"
                  % profile.describe())
            for r in reasons:
                print("  - %s" % r)
            print("gafp_pack: nothing written. This is the guard working: the "
                  "asset is not something this target can play.")
            return 1

        order = max_fixed_order_used(facts)
        blob, region, table_end = build_blob(data, facts, info, order)
        bad = audit_blob(blob, facts, data, info, order)
        if bad:
            for b in bad:
                print("gafp_pack: AUDIT FAILURE: %s" % b)
            print("gafp_pack: the packer's own re-read of the bytes it wrote "
                  "does not agree with its own walk. Nothing was written.")
            return 1
        # The reference decoder runs for two reasons, and it is the same run
        # either way: `flac -d` is a second implementation, so the PCM it returns
        # is either compared against what went in (an encode pass) or pinned as
        # the expected output of these exact frames (any pass, with a facts
        # table). Neither is the encoder's own word.
        need_pcm = bool(facts_out) or bool(profile.verify_pcm and source_wav)
        ref = fv.reference_pcm(packed) if need_pcm else None
        if ref is not None and profile.verify_pcm and source_wav:
            n = verify_pcm(source_wav, ref)
            print("gafp_pack: `flac -d` reproduces the source PCM exactly "
                  "(%d interleaved sample values)" % n)
        if facts_out:
            if ref is None:
                raise SystemExit("gafp_pack: --emit-facts needs the reference "
                                 "decoder's samples; refusing to write a table "
                                 "without PCM pins")
            sha = hashlib.sha256(blob).hexdigest()
            text = facts_table(out, blob, sha, facts, info, profile, ok, reasons,
                               report, flac_version(), ref)
            if check:
                current = (open(facts_out).read()
                           if os.path.exists(facts_out) else None)
                if current != text:
                    print("gafp_pack: STALE   %s" % facts_out)
                    return 1
            else:
                os.makedirs(os.path.dirname(facts_out) or ".", exist_ok=True)
                open(facts_out, "w").write(text)
                print("gafp_pack: wrote %s" % facts_out)

        if check:
            same = (os.path.exists(out)
                    and open(out, "rb").read() == blob)
            print("gafp_pack: %s %s (%d frames, %d bytes)"
                  % ("clean" if same else "STALE", out, len(frames), len(blob)))
            if not same:
                return 1
        elif expect is not None:
            # A comparison, not a write: this is the gate's byte-identity leg.
            want = open(expect, "rb").read()
            if want != blob:
                print("gafp_pack: BYTE-DIFFERS %s vs %s (%d vs %d bytes)"
                      % (out, expect, len(blob), len(want)))
                for i in range(min(len(blob), len(want))):
                    if blob[i] != want[i]:
                        print("gafp_pack:   first difference at 0x%X: %02X vs "
                              "%02X" % (i, blob[i], want[i]))
                        break
                return 1
            print("gafp_pack: byte-identical to %s (%d bytes, %d frames)"
                  % (expect, len(blob), len(frames)))
        else:
            os.makedirs(os.path.dirname(out) or ".", exist_ok=True)
            open(out, "wb").write(blob)
            print("gafp_pack: wrote %s: %d frames, region %d bytes at 0x%X, "
                  "total %d bytes, sha256 %s"
                  % (out, len(frames), len(region), table_end, len(blob),
                     hashlib.sha256(blob).hexdigest()))
        if not profile.is_product():
            print("gafp_pack: NOTE: this is NOT a C2 product asset: %s."
                  % ("relaxed guard(s) " + ", ".join(profile.relaxations())
                     if profile.relaxations() else
                     "built at a profile other than the C2 one (%s)"
                     % profile.describe()))
        return 0
    finally:
        if keep:
            print("gafp_pack: work kept in %s" % work)
        else:
            shutil.rmtree(work, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
