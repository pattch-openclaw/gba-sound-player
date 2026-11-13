#!/usr/bin/env python3
"""gen_gafp_fixtures.py — generate the committed GAFP test fixtures (item 6b-2).

Writes `crates/flac-lite/tests/gfp/*.gfp` and the facts table
`crates/flac-lite/tests/gafp_asset_vectors.txt` that
`crates/flac-lite/tests/gafp_asset_facts.rs` reads. Everything goes through the
real packer — `scripts/gafp_pack.py` as a subprocess, because the thing under test
is the packer and not a re-implementation of it in this file — from PCM
synthesized by `scripts/frame_vectors.py` with integer arithmetic only, so the
bytes are reproducible anywhere with the same libFLAC. The generating libFLAC
version is recorded in the table header, exactly as `gen_spike_assets.sh` records
it in `examples/flac_spike/src/assets.rs`.

What the set is for, and why each member is there
------------------------------------------------
C2 says "FIXED-only assets, 2048-sample frames at the device sample rate,
underrun-free on hardware", and the plan puts the FIXED-only guarantee at *pack
time*. So the fixtures exist to make two claims checkable in Rust over real packed
bytes rather than asserted in a comment:

  1. A GAFP the real packer wrote from a real encode decodes through
     `format::Manifest` + `decoder::Decoder` to exactly the reference decoder's
     PCM — per-blob and per-frame FNV-1a-64 over the interleaved 16-bit samples,
     plus first/last sample pins. That is the round-trip leg the `pack_flac.sh`
     stub called step 6, with a real manifest instead of a hand-built blob.
  2. The header field the profile was supposed to be policed by does not carry
     the fact it appears to carry. `x1_lpc_stereo.gfp` is an LPC asset the
     packer will NOT build at the product profile — and its manifest's
     `max_fixed_order` byte still reads 0, which `format::DEFAULT_PROFILE`
     (max 4) accepts. The Rust test states that gap; the `--check` pass re-runs
     the packer's refusal of the same encode and demands the reason name real LPC
     subframes and the measured cost.

Why these are small and the perf clip is not re-packed here: the 160-frame
FIXED-only blob at `examples/flac_integration/assets/spike_l0.gfp` is the measured
C2 asset, and its byte-identity against the real packer is gate `gafp-pack-check`.
These fixtures cover the *shape* of the container contract — mono, independent
stereo, mid/side, a short final frame, an LPC arm — at a size a test table can
carry: 5,000 samples with `-b 2048` is three frames, the last one legitimately 904
samples, which is the case `scripts/pack_flac.sh`'s notes warn about ("never
filter candidates on blocksize == track blocksize").

**Every source here is generated at the device rate, and says so.** Plan item 7-0
and Q14's ruling are the reason: `gafp_pack.py` refuses a stream that declares any
rate other than the clock it budgets against, so a 32,000 Hz fixture could only
ever be a refusal leg here — and a test set that had to *resample* its way to the
product clock would be proving a path the product does not take. The sample counts
and periods are therefore stated at 32,768 Hz like the product clip's: 5,000
samples is 5000/2**15 s, exact in binary, and the periods come from
`frame_vectors.PARTIALS_DEVICE` (Hz-authored), not from periods chosen at another
rate.

The refusal legs write no file: the packer's exit status and its stated reason are
the witness, and they are re-run on every `--check`. Note how each is reached:
the LPC and 44,100 Hz legs are packed as existing encodes (the *native* path —
packing a `.flac` as it stands, so the packer cannot quietly change the frames and
cannot resample), while the 8-bit leg is a real 8-bit FLAC encode, not a WAV the
packer would have converted to 16-bit on the way in.

Usage:
  scripts/gen_gafp_fixtures.py            write the fixtures and the table
  scripts/gen_gafp_fixtures.py --check    rebuild in memory, compare with what is
                                          committed, re-run the refusal legs;
                                          exit 1 on any drift
Exit status: 0 clean; 1 stale/missing output or a regressed refusal; 2 usage or
missing dependencies.

Deps: `flac`/`metaflac` (through the packer), python3 stdlib. ffmpeg is not
needed — nothing here asks the packer to resample.
"""

import hashlib
import os
import shutil
import subprocess
import struct
import sys
import tempfile
import wave

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
sys.path.insert(0, HERE)

import frame_vectors as fv  # noqa: E402
import gafp_pack as gp  # noqa: E402

PACKER = os.path.join(HERE, "gafp_pack.py")
TESTS = os.path.join(ROOT, "crates", "flac-lite", "tests")
GFP = os.path.join(TESTS, "gfp")
TABLE = os.path.join(TESTS, "gafp_asset_vectors.txt")

# Every accepted fixture is 5,000 samples of the same deterministic synthesis at
# the device rate — short on purpose, with a deliberately ragged final frame. The
# rate is read from `frame_vectors.DEVICE_RATE`, never written here: this file
# cannot name a sample rate that the packer it calls would refuse.
SAMPLES = 5000
SECONDS = SAMPLES / fv.DEVICE_RATE    # 5000/2**15, exact in binary


def fixtures():
    """The accepted set, as data so a new fixture is one more row.

    `src` is always a WAV for the accepted legs: the packer then makes the encode
    decisions itself (`-l`, `-m`/`--no-midside`, block size), which is what a
    product build does, and each fixture's flags are stated in one place.
    """
    return [
        dict(
            name="p0_fixed0_mono_32768_b2048.gfp", src="mono_5000.wav",
            channels=1, lpc=0, midside=False, allow=[],
            why=("product profile, mono: the smallest shape the container has to "
                 "carry and the one the manifest's mono channel-config code is "
                 "written for; 3 frames, the last 904 samples."),
        ),
        dict(
            name="p2_fixed0_stereo_32768_b2048.gfp", src="stereo_5000.wav",
            channels=2, lpc=0, midside=False, allow=[],
            why=("product profile, independent L/R stereo (`--no-midside`): the "
                 "assignment whose two subframes are plain left and right. "
                 "Nothing in this blob exercises side decorrelation, which is "
                 "what makes the mid/side row below load-bearing rather than "
                 "redundant."),
        ),
        dict(
            name="p3_fixed0_midside_32768_b2048.gfp", src="stereo_5000.wav",
            channels=2, lpc=0, midside=True, allow=[],
            why=("product profile, mid/side (`-m`): the decorrelated assignment "
                 "whose side slot is coded at bps + 1 (the measured seam in "
                 "frame_vectors.py, step 3f part 1). The packer reads that width "
                 "from the frames; the crate's decoder has to undo it, and the "
                 "PCM pins below are what prove it did."),
        ),
        dict(
            name="x1_lpc_stereo_32768_b2048.gfp", src="stereo_5000.wav",
            channels=2, lpc=4, midside=True,
            allow=["allow-lpc", "allow-over-budget"],
            why=("NOT a product blob, and the reason this file exists: the same "
                 "source at `-l 4`. The packer needs TWO separate relaxations to "
                 "write it — `--allow-lpc` for the profile limit (which preflight "
                 "refuses up front, before encoding anything) and "
                 "`--allow-over-budget` for the measured-cost bar — and the table "
                 "records both, because `-L 4` is not the C2 profile. The product "
                 "profile refuses this exact encode, and `--check` re-runs that "
                 "refusal. Its manifest still reports max_fixed_order 0: that is "
                 "the whole point of the 6b-2 witness."),
        ),
    ]


def refusals():
    """The refused set: no file, the packer's exit and reason are the witness.

    `want` is a substring the refusal must contain, so the witness is about the
    stated reason and not merely about a non-zero status.
    """
    return [
        dict(name="lpc at the product profile", slug="lpc_at_product_profile",
             src="l4_stereo.flac", extra=[],
             want=["real LPC subframes", "decodes at 174.14% of the frame"],
             why="the asset C2 cannot play, refused by the frames, not by a "
                 "header field that reads 0"),
        dict(name="44,100 Hz at the 32,768 Hz target",
             slug="rate_44100_at_32768_target", src="r44100_stereo.flac",
             extra=[],
             want=["sample rate 44100 Hz is not the target 32768 Hz"],
             why="a rate the container can express and this target has no "
                 "measured path for — and, since Q14 and plan item 7-0, no "
                 "interpretation either: the packer refuses it rather than "
                 "budgeting it against a different clock. Note this is the native "
                 "path — a WAV at 44,100 Hz would have been resampled to the "
                 "target instead"),
        dict(name="8-bit PCM at a 16-bit profile", slug="eight_bit_at_16bit",
             src="eight_bit.flac", extra=[],
             want=["bits per sample 8 is outside the container's [16]"],
             why="the depth the tests/README.md fixture list calls `8bit`: a "
                 "refusal with a reason, arriving before any bytes are written"),
    ]


# ------------------------------------------------------------------ sources


def write_wav(path, samples, rate, channels, width=2):
    with wave.open(path, "wb") as w:
        w.setnchannels(channels)
        w.setsampwidth(width)
        w.setframerate(rate)
        for s in samples:
            if width == 2:
                w.writeframes(struct.pack("<h", s))
            else:
                # 8-bit PCM in a WAV is unsigned: fold the synthesizer's 16-bit
                # samples into 0..255 by taking a byte. The depth refusal this
                # leg witnesses fires on the STREAMINFO bit depth, and a legal
                # 8-bit stream is all it needs.
                w.writeframes(bytes([(s >> 8) & 0xFF]))


def flac_encode(src, dst, order=0, midside=True):
    """An encode made outside the packer, for the legs that must arrive as an
    existing encode rather than as a fresh one.

    No rate argument: the rate is in the WAV, and an unused parameter here would
    read as if this function chose it. It does not — and after 7-0 the only rate
    worth naming in this file is the one `build_sources` writes into the WAV."""
    cmd = ["flac", "-1", "-f", "-s", "-l", str(order), "-b", "2048"]
    if midside:
        cmd.append("-m")
    subprocess.run(cmd + [src, "-o", dst], check=True)


def build_sources(work):
    def p(name):
        return os.path.join(work, name)

    # The product profile's own cost shape is what these fixtures are meant to
    # reproduce at test size, so the sources are built the way the product clip's
    # are: at the device rate, with periods authored from Hz at that rate. See
    # `frame_vectors.py`'s comment above `PARTIALS_32K` for why the two sets
    # differ by one sample each and why that difference is the whole of Q14.
    def at_device_rate(mono=False):
        return fv.synth_pcm(SECONDS, fv.DEVICE_RATE, fv.PARTIALS_DEVICE,
                            # The fixture's square keeps the 32 Hz its name always
                            # meant; at this rate that is an exact 1024-sample
                            # period, which is also the product clip's side slot.
                            fv.SIDE_PERIOD_DEVICE, 3, 700, 400, mono=mono)

    stereo = at_device_rate()
    mono = at_device_rate(mono=True)
    write_wav(p("stereo_5000.wav"), stereo, fv.DEVICE_RATE, 2)
    write_wav(p("mono_5000.wav"), mono, fv.DEVICE_RATE, 1)
    write_wav(p("eight_bit.wav"), stereo, fv.DEVICE_RATE, 2, width=1)
    flac_encode(p("eight_bit.wav"), p("eight_bit.flac"))

    # The one deliberately off-rate source in the file: the rate refusal leg. It
    # stays 44,100 Hz *because* that is the thing being witnessed — a stream whose
    # declared rate is not the target's clock, arriving as an existing encode so
    # the packer cannot resample it away.
    r44100 = fv.synth_pcm(SECONDS, 44100, fv.PARTIALS_32K, 1000, 3, 700, 400)
    write_wav(p("stereo_44100.wav"), r44100, 44100, 2)
    flac_encode(p("stereo_44100.wav"), p("r44100_stereo.flac"))

    # The LPC leg, encoded outside the packer on purpose: the refusal leg and the
    # relaxed fixture must be about the SAME bytes, so that "the product profile
    # refuses this" and "this file exists only because a guard was relaxed" are
    # statements about one stream, not about two similar ones.
    flac_encode(p("stereo_5000.wav"), p("l4_stereo.flac"), order=4)


def run_packer(src, out, extra, facts=None):
    cmd = [sys.executable, PACKER] + list(extra)
    if facts:
        cmd += ["--emit-facts", facts]
    cmd += [src, out]
    proc = subprocess.run(cmd, capture_output=True, text=True)
    return proc.returncode, proc.stdout + proc.stderr


# --------------------------------------------------------------------- main


TABLE_HEADER = """# flac-lite GAFP asset facts -- GENERATED
# DO NOT EDIT BY HAND. Regenerate with: scripts/gen_gafp_fixtures.py
# Packer: scripts/gafp_pack.py (the real GAFP writer, item 6b-2).
# Frame walk: scripts/frame_vectors.py find_frames/parse_frame_header -- strict
#   RFC 9639 9.1 header + verifying CRC-8, never a bare sync scan; subframe walk:
#   gafp_pack.walk_subframe. Source PCM: scripts/frame_vectors.py synth_pcm
#   (integer-exact, no RNG). Reference PCM: `flac -d` of the stream the frames
#   came from -- a second implementation, not the encoder's own word.
# encoder: %(version)s
#
# Every accepted blob here declares 32,768 Hz, and so does every source WAV this
# file synthesizes. That is not incidental: since Q14 (ruled 2026-11-11) and plan
# item 7-0, `gafp_pack.py` refuses any stream whose declared rate is not the clock
# its measured rows are budgeted against, so an off-rate source could only ever
# reach this table as a refusal leg. `rate_44100_at_32768_target` is that leg, and
# the reason it is kept. A rate that *looks* close to the target — 32,000 Hz,
# 1.024x, +41.06 cents — is refused exactly like 44,100 Hz is: the declared rate is
# a validity check, never something the packer reinterprets.
#
# One block per committed blob under tests/gfp/, in the order gen_gafp_fixtures.py
# builds them, then the refusal legs. A block is: `# source:`/`# flags:`/`# why:`
# comments, the packer's own `profile`/`relaxations`/`verdict` data rows and any
# `# note:`/`# refused:` lines it has to say, then one `blob` row (file name, byte
# count, sha256, the manifest's own fields, and the PCM totals), then a `frame`
# row per frame and a `subframe` row per slot. Offsets in `region_offset` are RELATIVE TO THE START OF THE FRAME
# REGION -- that is what `Manifest::frame_region()` hands a caller -- and
# `blob_offset` is the absolute position, i.e. the value written to the offset
# table. The bit fields (`header_bits`, `entry_bit`, `exit_bit`, `padded`,
# `end_bit`) are ABSOLUTE bit positions in the source `.flac`, which includes the
# metadata before the frame region; they are not region-relative. No field value
# in a data row contains a space or an `=`, with one deliberate exception:
# `reason=`, which is quoted packer output and is therefore always the LAST field
# of its row. Prose lives in comment lines, so a reader in Rust can parse this
# without a parser.
#
# `pcm_fnv1a64` / `pcm_first` / `pcm_last` are the REFERENCE decoder's samples,
# hashed in the order and width the existing tables use: interleaved left-then-
# right, little-endian, 16-bit, FNV-1a 64 at the repo's offset basis
# 0xcbf43ce95de684b7 (`frame_vectors.fnv1a64`, mirrored in the Rust suites). The
# `blob` row's hash folds every frame; each `frame` row's hash starts fresh, so a
# wrong frame is localized as well as detected.
#
# Each block's `verdict` row is what the packer decided at the profile it built
# the blob at, and whether that profile is the C2 product one. The `refusal` rows
# below are the arms where it decided to write nothing at all; those have no file,
# and the witness is the exit status plus the reason it printed, keyed by a slug a
# Rust test can name.
#
# `sha256` is a provenance pin for people and for the generator's own `--check`
# (which compares committed bytes against a rebuild); the Rust suite pins `bytes=`
# and every frame's `bytes=` instead, because a hash function inside a test would
# be a second unverified thing in the gate. The PCM hashes below are the semantic
# pins, and they are computed by the Rust code.
#
# The three stereo rows share one `pcm_fnv1a64` by construction: same source PCM,
# three different frame sets (independent, mid/side, LPC). That is the shape of
# the witness the Rust suite runs — that identical decoder output can come from
# bytes that are not identical, and that the field the profile was supposed to be
# policed by (`max_fixed_order`) reads 0 in all three.
"""


def main(argv):
    if any(a not in ("--check",) for a in argv):
        raise SystemExit("gen_gafp_fixtures: unknown argument(s): %s"
                         % " ".join(a for a in argv if a != "--check"))
    check = "--check" in argv
    for tool in ("flac", "metaflac"):
        if shutil.which(tool) is None:
            raise SystemExit("gen_gafp_fixtures: %s not on PATH" % tool)
    if not os.path.exists(PACKER):
        raise SystemExit("gen_gafp_fixtures: no packer at %s" % PACKER)
    if not check:
        os.makedirs(GFP, exist_ok=True)

    work = tempfile.mkdtemp(prefix="gafp-fixtures-")
    failures = []
    blocks = []
    try:
        def p(name):
            return os.path.join(work, name)
        build_sources(work)

        for spec in fixtures():
            args = ["--channels", str(spec["channels"]), "-L", str(spec["lpc"])]
            if not spec["midside"]:
                args.append("--no-midside")
            for name in spec["allow"]:
                args.append("--" + name)
            facts = p(spec["name"] + ".facts")
            code, log = run_packer(p(spec["src"]), p("out.gfp"), args, facts)
            out = os.path.join(GFP, spec["name"])
            if code != 0:
                failures.append("%s: the packer exited %d:\n%s"
                                % (spec["name"], code, log))
                continue
            # The generator, not the packer, decides where the committed file
            # goes -- the packer wrote it to the work dir.
            data = open(p("out.gfp"), "rb").read()
            if check:
                committed = open(out, "rb").read() if os.path.exists(out) else None
                if committed != data:
                    failures.append("%s: %s" % (
                        "not committed" if committed is None
                        else "STALE: committed %d bytes, packer wrote %d"
                             % (len(committed), len(data))))
            else:
                open(out, "wb").write(data)
            text = open(facts).read()
            # The packer named the file it wrote (the work-dir copy); the
            # committed name is this generator's, so the table has to carry it.
            text = text.replace("blob name=out.gfp", "blob name=%s"
                                % spec["name"], 1)
            # What the generator knows and the packer cannot — which source, which
            # flags, why this row exists — goes in comment lines, so every
            # `key=value` row in this table stays machine-readable and no field
            # value ever contains a space or an `=`.
            text = ("# source: %s\n# flags: %s\n# why: %s\n%s"
                    % (spec["src"], " ".join(args), spec["why"], text))
            blocks.append(text.rstrip() + "\n")

        refusal_rows = []
        for spec in refusals():
            code, log = run_packer(p(spec["src"]), p("refused.gfp"),
                                   spec["extra"])
            wrote = os.path.exists(p("refused.gfp"))
            missing = [w for w in spec["want"] if w not in log]
            if code == 0 or wrote:
                failures.append("refusal leg %r: the packer accepted it%s"
                                % (spec["name"], " and wrote a file"
                                   if wrote else ""))
            elif missing:
                failures.append("refusal leg %r: refused, but the message never "
                                "says %r:\n%s" % (spec["name"], missing, log))
            reason = " ".join(l.strip() for l in log.splitlines()
                              if l.strip().startswith("- "))
            # Machine-readable row first, prose in comments: `reason=` is the last
            # field precisely so it can hold anything.
            block = ["# why: %s" % spec["why"],
                     "# wanted in the refusal: %s"
                     % " ; ".join(spec["want"]),
                     "refusal slug=%s exit=%d wrote_file=%s reason=%s"
                     % (spec["slug"], code, "true" if wrote else "false",
                        reason)]
            refusal_rows.append("\n".join(block))
            if os.path.exists(p("refused.gfp")):
                os.remove(p("refused.gfp"))

        if failures:
            for f in failures:
                print("gen_gafp_fixtures: FAIL %s" % f)
            return 1

        text = (TABLE_HEADER % dict(version=gp.flac_version())
                + "\n# ---- refusal legs: no file; the exit status and the "
                  "printed reason are the witness ----\n"
                + "\n".join(refusal_rows) + "\n\n"
                + "\n".join(blocks))

        if check:
            current = open(TABLE).read() if os.path.exists(TABLE) else None
            if current != text:
                print("gen_gafp_fixtures: STALE %s" % TABLE)
                if current is not None:
                    cl, tl = current.splitlines(), text.splitlines()
                    for i in range(min(len(cl), len(tl))):
                        if cl[i] != tl[i]:
                            print("  first difference at line %d:\n"
                                  "    committed:     %s\n"
                                  "    regenerated:   %s" % (i + 1, cl[i], tl[i]))
                            break
                    if len(cl) != len(tl):
                        print("  line count %d committed vs %d regenerated"
                              % (len(cl), len(tl)))
                return 1
            print("gen_gafp_fixtures: clean — %d blobs match their committed "
                  "bytes, %d refusal legs still refused with their stated "
                  "reasons, table matches"
                  % (len(fixtures()), len(refusals())))
            return 0

        open(TABLE, "w").write(text)
        for spec in fixtures():
            path = os.path.join(GFP, spec["name"])
            print("gen_gafp_fixtures: wrote tests/gfp/%s %d bytes sha256 %s"
                  % (spec["name"], os.path.getsize(path),
                     hashlib.sha256(open(path, "rb").read()).hexdigest()[:16]))
            print("    # %s" % spec["why"])
        print("gen_gafp_fixtures: wrote %s (%d lines)"
              % (os.path.relpath(TABLE, ROOT), len(text.splitlines())))
        return 0
    finally:
        shutil.rmtree(work, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
