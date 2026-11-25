#!/usr/bin/env python3
"""S8-2's P2 — `--dump-plan`: the plan the reference chose, both passes, and its counts.

Why this exists. S8-3 (P3) can read a blob's *emitted* plan, and it proved the S7 DIFF is
the plan rather than the trainer: on `biomaster` the plan diverges at frame 0. But the
plan the **trainer** is actually handed is the **pass-1** plan, and that plan is not in
the artefact at all — a blob carries the emitted plan and the table set trained on the
previous pass's plan. So "the trainer differs" and "the plan handed to it differs" are
indistinguishable from bytes, and S8-6 would be blind without this instrument. P2 is the
reference build that says which.

What it is. A patched copy of the off-tree reference's own `uli_enc.c`, generated into a
temp directory at run time (no C enters this repository, nothing here builds C, and the
target that runs it is on no gate's path — E-R1 forbids the dependency, not the oracle).
The patch adds `--dump-plan FILE`, which writes:

  pass <tag> attempt N frames NF ng NG bits_sum ... d_sum ... cycles_max ... cycles_sum ...
  plan <tag> attempt N frame F g G ms M bits HEX D HEX cycles C mu HEX
  stats <tag> esc HEX frames HEX ms_frames HEX
  stats <tag> cb|q|p|cen_s|cen_n ... HEX

for the **pass-1** plan (tag `pass1`, the trainer's input), every **pass-2** attempt
(tag `pass2`; the last one is the shipped plan), and every `EncStats` set that
`tables_from_stats` consumed — the four `train-itN` rounds and the `retrain` whose counts
produce the blob's table region. Values that the blob would round are written as hex f64
bit patterns, so a count or a distortion can be compared bitwise rather than through a
printed decimal.

And the gate, which is what makes the dump evidence rather than a narrative. **The
dumped shipped plan must equal the plan recovered from that same run's own blob** — the
instrument checked against the artefact it explains, so a wrong dump cannot masquerade as
a finding. The blob half is `crates/uli-lite/examples/plan_dump.rs`, a plain decode of
the stream (the plan is a delta and a flag bit, so it cannot be read off the container).
It is compared frame by frame on `g` and `ms`, with the header's two cycle fields (which
the encoder derives from the chosen plan) and every seek entry's `g_prev` as
cross-checks. The default cell is S8-4's `fast` cell — generated, no corpus needed, about
three seconds of reference time — so this is an iteration loop, not a matrix.

Usage:
    make uli-dump-plan                      # the gate: fast cell, build, dump, verify
    scripts/uli_dump_plan.py                # same, directly
    scripts/uli_dump_plan.py --album album.f32 --tracks tracks.tsv --out album.uli \
        --dump-plan plan.txt --budget 20000 --cap 74 --threads 4 \
        --plan-dump <built plan_dump binary>

`--plan-dump` is the path to the built example. The Makefile target builds it; when you run
the script by hand, build it once (the target's recipe is the exact command) or pass
`--no-verify` to get the dump by itself. `--train-dump PATH` additionally runs S8-6's
`train-dump` on the same dump, blob and PCM: it rebuilds the initial tables from the train
rounds, feeds the pass-1 plan to `train::collect_frames` every second frame, and requires the
reference blob's 5,212-byte table region. `--work-dir DIR` keeps the generated cell, blob and
dump for inspection instead of a temp directory (the `make uli-train-diff` target uses it).
`CC` and `DUMP_CFLAGS` override the compiler and the flags for a deliberate *different*
oracle — say so where you record the result, because the flags are part of what a dump
means. Exit status: 0 when the gate passes; 2 when the dump
and the blob disagree on the plan, the header cycle fields or a seek entry (the
`uli_accept.py` DIFF convention); 1 on a harness error (missing reference source, build
failure, missing dump section, unparseable dump/blob).
"""

import argparse
import os
import re
import shutil
import struct
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
GBA_AUDIO = Path(os.environ.get("GBA_AUDIO", ROOT.parent / "gba-audio"))
ENC = GBA_AUDIO / "encoder"
REF_SRC = GBA_AUDIO / "src/uli"
GEN_FAST = ROOT / "scripts/gen_uli_fast_cell.py"

# The oracle of record's float flags (S8-4b), plus the dump code. A probe built with GCC's
# default contraction would fuse multiply-adds the oracle binary does not, and its `D`
# would be a different arithmetic from the one S8 compares — that is why these are pinned
# here and not inherited from the reference Makefile's `-march=native`.
DEFAULT_CFLAGS = "-O2 -std=gnu11 -w -ffp-contract=off -march=armv8-a -fopenmp"
# S8-4's cell: the same flags `scripts/uli_accept.py` uses, so the plan dumped here is a
# plan from a cell the matrix already knows.
FAST_FLAGS = ["--budget", "20000", "--cap", "74", "--threads", "4"]

# The helper code is inserted into a *copy* of `uli_enc.c`, so it may use the reference's
# globals (`T`, `NF`, `Curves`) and its own includes. Nothing below is part of the
# reference; the anchors are asserted to occur exactly once, so a moved source fails loudly
# instead of producing a dump of the wrong thing.
HELPERS_EARLY = r'''
/* ---- P2 (S8-2), appended by scripts/uli_dump_plan.py. No C in this repository. ---- */
static const char *DUMP_PATH = NULL;
static FILE *DUMP_F = NULL;

static unsigned long long p2_hex(double v) { unsigned long long u; memcpy(&u, &v, 8); return u; }

static void p2_open(const char *path) {
    if (!path) return;
    DUMP_F = fopen(path, "w");
    if (!DUMP_F) { perror(path); exit(1); }
    fprintf(DUMP_F, "# uli_dump_plan 1\n");
}

static void p2_stats(const char *tag, const EncStats *st) {
    if (!DUMP_F) return;
    fprintf(DUMP_F, "stats %s esc %016llx frames %016llx ms_frames %016llx\n",
            tag, p2_hex(st->esc), p2_hex(st->frames), p2_hex(st->ms_frames));
    for (int a = 0; a <= T.K; a++)
        for (int b = 0; b <= T.K; b++)
            if (st->cb[a][b] != 0)
                fprintf(DUMP_F, "stats %s cb %d %d %016llx\n", tag, a, b, p2_hex(st->cb[a][b]));
    for (int c = 0; c < T.K; c++) {
        for (int s = 0; s < 81; s++)
            if (st->q[c][s] != 0)
                fprintf(DUMP_F, "stats %s q %d %d %016llx\n", tag, c, s, p2_hex(st->q[c][s]));
        for (int v = 0; v < 256; v++)
            if (st->p[c][v] != 0)
                fprintf(DUMP_F, "stats %s p %d %d %016llx\n", tag, c, v, p2_hex(st->p[c][v]));
        for (int m = 1; m < 16; m++)
            if (st->cen_n[c][m] != 0 || st->cen_s[c][m] != 0) {
                fprintf(DUMP_F, "stats %s cen_s %d %d %016llx\n", tag, c, m, p2_hex(st->cen_s[c][m]));
                fprintf(DUMP_F, "stats %s cen_n %d %d %016llx\n", tag, c, m, p2_hex(st->cen_n[c][m]));
            }
    }
}
'''

# `Curves` is defined after the globals, so the plan dumper is inserted next to it.
HELPERS_CURVES = r'''
/* P2's per-frame plan, in the units `frame_code` reports (bits include the M/S bit and
 * exclude the g delta; D is the derived distortion the coder branched on). */
static void p2_plan(const char *tag, int attempt, const Curves *cv, const int *k,
                    const int *g, const int8_t *ms) {
    if (!DUMP_F) return;
    double sb = 0, sd = 0;
    uint32_t cmax = 0;
    unsigned long long csum = 0;
    for (int64_t f = 0; f < NF; f++) {
        size_t i = (size_t)f * cv->NG + k[f];
        sb += cv->R[i]; sd += cv->D[i];
        if (cv->C[i] > cmax) cmax = cv->C[i];
        csum += cv->C[i];
    }
    fprintf(DUMP_F, "pass %s attempt %d frames %lld ng %d bits_sum %016llx d_sum %016llx cycles_max %u cycles_sum %llu\n",
            tag, attempt, (long long)NF, cv->NG, p2_hex(sb), p2_hex(sd), cmax, csum);
    for (int64_t f = 0; f < NF; f++) {
        size_t i = (size_t)f * cv->NG + k[f];
        fprintf(DUMP_F, "plan %s attempt %d frame %lld g %d ms %d bits %016llx D %016llx cycles %u mu %016llx\n",
                tag, attempt, (long long)f, g[f], (int)ms[f],
                p2_hex(cv->R[i]), p2_hex(cv->D[i]), cv->C[i], p2_hex(cv->MU[i]));
    }
}
'''

# (anchor, replacement, what). Each anchor must occur exactly once; the patch is applied in
# order and refuses rather than guessing if the reference moves.
ANCHORS = [
    (
        "static double t_start;\n",
        "static double t_start;\n" + HELPERS_EARLY,
        "the globals block (dump state and the stats dumper)",
    ),
    (
        '        else { fprintf(stderr, "unknown option %s\\n", argv[i]); return 1; }\n',
        '        else if (!strcmp(argv[i], "--dump-plan")) DUMP_PATH = argv[++i];\n'
        '        else { fprintf(stderr, "unknown option %s\\n", argv[i]); return 1; }\n',
        "the option parser (the --dump-plan flag)",
    ),
    (
        "    NDAC = 1024.0 * 32768.0 * 32768.0 / 2 * pow(10, DAC_NOISE_DBFS / 10);\n",
        "    NDAC = 1024.0 * 32768.0 * 32768.0 / 2 * pow(10, DAC_NOISE_DBFS / 10);\n"
        "    p2_open(DUMP_PATH);\n",
        "main after the option parse (open the dump before training)",
    ),
    (
        "typedef struct { int NG; int *g0; float *R, *D, *MU; uint32_t *C; int8_t *MS; } Curves;\n",
        "typedef struct { int NG; int *g0; float *R, *D, *MU; uint32_t *C; int8_t *MS; } Curves;\n"
        + HELPERS_CURVES,
        "the Curves typedef (the per-frame plan dumper)",
    ),
    (
        '        fprintf(stderr, "[%6.1fs] train it%d: lam %.4g bits/frame %.1f (target %.1f) wD %.5g esc %.0f\\n", now(), it, lam, sR / n, target, sD, st->esc);\n',
        '        fprintf(stderr, "[%6.1fs] train it%d: lam %.4g bits/frame %.1f (target %.1f) wD %.5g esc %.0f\\n", now(), it, lam, sR / n, target, sD, st->esc);\n'
        '        { char p2tag[32]; snprintf(p2tag, sizeof p2tag, "train-it%d", it); p2_stats(p2tag, st); }\n',
        "train()'s stats (the four rounds that build the initial tables)",
    ),
    (
        '    fprintf(stderr, "[%6.1fs] retrain: bits/frame %.1f with old tables (band-class side info %.1f)\\n", now(), sR / n, cb / n);\n',
        '    fprintf(stderr, "[%6.1fs] retrain: bits/frame %.1f with old tables (band-class side info %.1f)\\n", now(), sR / n, cb / n);\n'
        "    p2_stats(\"retrain\", st);\n",
        "retrain()'s stats (the counts that produce the blob's tables)",
    ),
    (
        "    for (int64_t f = 0; f < NF; f++) { g[f] = cv.g0[f] + k[f]; if (g[f] > GMAX) g[f] = GMAX; ms[f] = cv.MS[(size_t)f * cv.NG + k[f]]; }\n"
        "\n"
        "    /* 5. retrain on the chosen allocation, recompute curves around it */\n",
        "    for (int64_t f = 0; f < NF; f++) { g[f] = cv.g0[f] + k[f]; if (g[f] > GMAX) g[f] = GMAX; ms[f] = cv.MS[(size_t)f * cv.NG + k[f]]; }\n"
        '    p2_plan("pass1", 0, &cv, k, g, ms);\n'
        "\n"
        "    /* 5. retrain on the chosen allocation, recompute curves around it */\n",
        "the pass-1 plan, before the trainer sees it",
    ),
    (
        "        for (int64_t f = 0; f < NF; f++) { g[f] = cv.g0[f] + k[f]; if (g[f] > GMAX) g[f] = GMAX; ms[f] = cv.MS[(size_t)f * cv.NG + k[f]]; }\n"
        "        /* 6. final encode in chunks */\n",
        "        for (int64_t f = 0; f < NF; f++) { g[f] = cv.g0[f] + k[f]; if (g[f] > GMAX) g[f] = GMAX; ms[f] = cv.MS[(size_t)f * cv.NG + k[f]]; }\n"
        '        p2_plan("pass2", attempt, &cv, k, g, ms);\n'
        "        /* 6. final encode in chunks */\n",
        "each pass-2 attempt's plan, before that attempt is encoded and sized",
    ),
]


class HarnessError(Exception):
    pass


class DumpMismatch(HarnessError):
    """The dump and the blob disagree: exit 2 (the uli_accept.py DIFF convention)."""


def fail_if(cond, msg):
    if cond:
        raise HarnessError(msg)


def apply_patch(src: str) -> str:
    """Insert P2 into the reference's driver; refuse anything that is not exactly one site."""
    out = src
    for anchor, replacement, what in ANCHORS:
        n = out.count(anchor)
        fail_if(
            n != 1,
            f"the reference's uli_enc.c has {n} occurrences of the anchor for {what}, "
            f"expected exactly 1. The reference moved; the dump would describe the wrong "
            f"site, so this instrument refuses rather than guessing.\n"
            f"  anchor: {anchor.splitlines()[0][:120]!r}",
        )
        out = out.replace(anchor, replacement, 1)
    return out


def build(tmpdir: Path) -> tuple[Path, list[str]]:
    """Mirror the reference's encoder tree, patch its driver, build it, return (exe, flags)."""
    fail_if(
        not (ENC / "uli_enc.c").exists(),
        f"no reference encoder source at {ENC}\n"
        f"  uli_dump_plan builds the reference's own driver; set GBA_AUDIO= to the checkout "
        f"that has it (E-R1 keeps this off every gate path, so nothing else will tell you)",
    )
    for name in ("enc_core.c", "enc_core.h", "uli_enc.c"):
        fail_if(not (ENC / name).exists(), f"missing {ENC / name}")
    for name in ("uli_dec_ref.c", "uli_album.c", "uli_tables.c"):
        fail_if(not (REF_SRC / name).exists(), f"missing {REF_SRC / name}")

    mirror = tmpdir / "encoder"
    mirror.mkdir()
    (tmpdir / "src").symlink_to(GBA_AUDIO / "src", target_is_directory=True)
    for name in ("enc_core.c", "enc_core.h"):
        shutil.copy(ENC / name, mirror / name)
    patched = apply_patch((ENC / "uli_enc.c").read_text())
    (mirror / "uli_enc.c").write_text(patched)

    cc = os.environ.get("CC", "cc")
    cflags = os.environ.get("DUMP_CFLAGS", DEFAULT_CFLAGS).split()
    exe = tmpdir / "uli_enc_plan"
    link = [str(REF_SRC / f) for f in ("uli_dec_ref.c", "uli_album.c", "uli_tables.c")]
    argv = [
        cc,
        *cflags,
        "-I",
        str(mirror),
        "-o",
        str(exe),
        str(mirror / "uli_enc.c"),
        str(mirror / "enc_core.c"),
        *link,
        "-lm",
    ]
    cp = subprocess.run(argv, capture_output=True, text=True)
    if cp.returncode != 0:
        raise HarnessError(f"compiling the patched reference failed:\n{cp.stderr.strip()}")
    return exe, cflags


# The dump's three line shapes. A line that matches none of them is a harness error, not
# something to skip: a format change must break this loudly.
PASS_RE = re.compile(
    r"^pass (\S+) attempt (\d+) frames (\d+) ng (\d+) bits_sum ([0-9a-f]{16}) "
    r"d_sum ([0-9a-f]{16}) cycles_max (\d+) cycles_sum (\d+)$"
)
PLAN_RE = re.compile(
    r"^plan (\S+) attempt (\d+) frame (\d+) g (-?\d+) ms ([01]) bits ([0-9a-f]{16}) "
    r"D ([0-9a-f]{16}) cycles (\d+) mu ([0-9a-f]{16})$"
)
STATS_RE = re.compile(r"^stats (\S+) esc ([0-9a-f]{16}) frames ([0-9a-f]{16}) ms_frames ([0-9a-f]{16})$")
STATS_DETAIL_RE = re.compile(
    r"^stats (\S+) (cb|q|p|cen_s|cen_n) (\d+) (\d+) ([0-9a-f]{16})$"
)


def f64(hex_bits: str) -> float:
    # `p2_hex` memcpy's the double into a u64 and prints that integer, so the hex string is
    # the IEEE-754 bit pattern in big-endian order regardless of the host's byte order.
    return struct.unpack(">d", bytes.fromhex(hex_bits))[0]


def parse_dump(text: str) -> dict:
    """Parse a dump into passes and stats.

    `passes[(tag, attempt)]` is insertion-ordered by the file, so the last `pass2` attempt
    is the shipped one: the attempt loop dumps before it checks the size, and the loop
    only exits with a blob when one attempt fits.
    """
    passes: dict[tuple[str, int], dict] = {}
    stats: list[dict] = []
    for n, line in enumerate(text.splitlines(), 1):
        if not line or line.startswith("#"):
            continue
        m = PASS_RE.match(line)
        if m:
            tag, attempt = m.group(1), int(m.group(2))
            passes[(tag, attempt)] = {
                "frames": int(m.group(3)),
                "ng": int(m.group(4)),
                "bits_sum": f64(m.group(5)),
                "d_sum": f64(m.group(6)),
                "cycles_max": int(m.group(7)),
                "cycles_sum": int(m.group(8)),
                "plan": [],
            }
            continue
        m = PLAN_RE.match(line)
        if m:
            tag, attempt = m.group(1), int(m.group(2))
            key = (tag, attempt)
            fail_if(key not in passes, f"dump line {n}: a plan line before its pass header")
            passes[key]["plan"].append(
                {
                    "frame": int(m.group(3)),
                    "g": int(m.group(4)),
                    "ms": int(m.group(5)),
                    "bits": f64(m.group(6)),
                    "D": f64(m.group(7)),
                    "cycles": int(m.group(8)),
                    "mu": f64(m.group(9)),
                }
            )
            continue
        m = STATS_RE.match(line)
        if m:
            stats.append(
                {
                    "tag": m.group(1),
                    "esc": f64(m.group(2)),
                    "frames": f64(m.group(3)),
                    "ms_frames": f64(m.group(4)),
                    "values": {},
                }
            )
            continue
        m = STATS_DETAIL_RE.match(line)
        if m:
            fail_if(
                not stats or stats[-1]["tag"] != m.group(1),
                f"dump line {n}: a stats detail line before its section header",
            )
            stats[-1]["values"][(m.group(2), int(m.group(3)), int(m.group(4)))] = f64(
                m.group(5)
            )
            continue
        raise HarnessError(f"dump line {n} does not parse: {line[:160]!r}")
    fail_if(not passes, "the dump has no pass header — the patch did not run")
    return {"passes": passes, "stats": stats}


def parse_blob(text: str) -> dict:
    """Parse `plan_dump`'s output: the header, the seek entries and the decoded plan."""
    header: dict[str, int] = {}
    seeks: list[dict] = []
    frames: list[dict] = []
    for n, line in enumerate(text.splitlines(), 1):
        if not line or line.startswith("#"):
            continue
        t = line.split()
        if t[0] == "blob":
            continue
        if t[0] == "header":
            pairs = dict(zip(t[1::2], t[2::2]))
            try:
                header = {k: int(v) for k, v in pairs.items()}
            except ValueError:
                raise HarnessError(f"plan_dump line {n}: non-integer header: {line!r}")
            continue
        if t[0] in ("seek", "frame"):
            # Both print a bare index first (`seek I frame F ...`, `frame F g G ...`).
            pairs = dict(zip(t[2::2], t[3::2]))
            try:
                row = {k: int(v) for k, v in pairs.items()}
                row["index"] = int(t[1])
            except (ValueError, IndexError):
                raise HarnessError(f"plan_dump line {n} does not parse: {line[:160]!r}")
            (seeks if t[0] == "seek" else frames).append(row)
            continue
        raise HarnessError(f"plan_dump line {n} does not parse: {line[:160]!r}")
    fail_if(not header, "plan_dump printed no header")
    fail_if(not frames, "plan_dump decoded no frames")
    return {"header": header, "seeks": seeks, "frames": frames}


def first_plan_difference(dumped: list[dict], decoded: list[dict]) -> str | None:
    """Describe the first frame whose `g` or `ms` differs, or None if the plans agree."""
    for f, (a, b) in enumerate(zip(dumped, decoded)):
        if (a["g"], a["ms"]) != (b["g"], b["ms"]):
            which = []
            if a["g"] != b["g"]:
                which.append(f"g dumped {a['g']} vs blob {b['g']}")
            if a["ms"] != b["ms"]:
                which.append(f"ms dumped {a['ms']} vs blob {b['ms']}")
            return f"frame {f}: " + ", ".join(which)
    return None


def verify(dump: dict, blob: dict, say: bool = True) -> None:
    """The gate: the dumped shipped plan is the plan the blob declares."""
    passes = dump["passes"]
    n = blob["header"]["n_frames"]
    fail_if(("pass1", 0) not in passes, "the dump has no pass-1 plan — the trainer's input is missing")
    p1 = passes[("pass1", 0)]
    fail_if(
        p1["frames"] != n or len(p1["plan"]) != n,
        f"the pass-1 dump covers {p1['frames']} frames ({len(p1['plan'])} lines), the blob "
        f"declares {n}",
    )
    fail_if(
        p1["ng"] != 28,
        f"the pass-1 window is NG = {p1['ng']}, expected 28 (inventory row 6)",
    )

    shipped = [(tag, att) for (tag, att) in passes if tag == "pass2"]
    fail_if(not shipped, "the dump has no pass-2 plan — the shipped plan was not captured")
    tag, attempt = shipped[-1]
    p2 = passes[(tag, attempt)]
    fail_if(
        p2["frames"] != n,
        f"the shipped dump covers {p2['frames']} frames, the blob declares {n}",
    )
    fail_if(
        len(p2["plan"]) != n,
        f"the shipped dump has {len(p2['plan'])} plan lines for {n} frames",
    )
    fail_if(
        p2["ng"] != 20,
        f"the pass-2 window is NG = {p2['ng']}, expected 20 (inventory row 7)",
    )
    diff = first_plan_difference(p2["plan"], blob["frames"])
    if diff is not None:
        raise DumpMismatch(f"the dumped shipped plan differs from the blob's: {diff}")

    # The header cycle fields are derived from the chosen plan's cycle estimate, so they
    # cross-check that the dump's `cycles` column is the one the container was built from.
    if p2["cycles_max"] != blob["header"]["est_max_cycles"]:
        raise DumpMismatch(
            f"est_max_cycles: dump {p2['cycles_max']} vs blob "
            f"{blob['header']['est_max_cycles']}"
        )
    want_mean = p2["cycles_sum"] // n
    if want_mean != blob["header"]["est_mean_cycles"]:
        raise DumpMismatch(
            f"est_mean_cycles: dump {want_mean} vs blob "
            f"{blob['header']['est_mean_cycles']}"
        )
    # Every seek entry records the plan's own g delta predecessor; the blob carries them.
    for s in blob["seeks"]:
        f = s["frame"]
        want = p2["plan"][f - 1]["g"] if f > 0 else 0
        if s["g_prev"] != want:
            raise DumpMismatch(
                f"seek {s['frame']} g_prev: blob {s['g_prev']} vs dumped plan {want}"
            )

    # The sections S8-6/S8-7 will consume must be present, and their frame counts must be
    # the sampling the reference uses (every 8th for `train`, every 2nd for `retrain`).
    train = [s for s in dump["stats"] if s["tag"].startswith("train-it")]
    retrain = [s for s in dump["stats"] if s["tag"] == "retrain"]
    fail_if(len(train) != 4, f"the dump has {len(train)} train rounds, expected 4")
    fail_if(len(retrain) != 1, f"the dump has {len(retrain)} retrain stat sets, expected 1")
    want_train = (n + 7) // 8
    for s in train:
        fail_if(
            s["frames"] != want_train,
            f"{s['tag']} covers {s['frames']:g} frames, expected ceil(n/8) = {want_train}",
        )
    want_retrain = (n + 1) // 2
    fail_if(
        retrain[0]["frames"] != want_retrain,
        f"retrain covers {retrain[0]['frames']:g} frames, expected ceil(n/2) = {want_retrain}",
    )

    if say:
        print(f"  pass1      {p1['frames']} frames, ng {p1['ng']}")
        print(f"  pass2      {n} frames, attempt {attempt}, ng {p2['ng']}")
        print(f"  plan       g and ms identical to the blob on all {n} frames")
        print(
            f"  header     est max/mean cycles {p2['cycles_max']}/{want_mean} — identical to the blob"
        )
        print(f"  seek       {len(blob['seeks'])} entries, g_prev identical")
        print(
            f"  stats      4 train rounds ({want_train} frames each) + retrain "
            f"({want_retrain} frames)"
        )


def prove_the_gate_bites(dump: dict, blob: dict) -> None:
    """The gate is only worth its green if a wrong dump is refused.

    Three copies, one mutation each, and the *same* comparison: a plan step moved by one, a
    header cycle field moved by one, and a seek entry's `g_prev` moved by one. Every one must
    raise `DumpMismatch`; a mutation that passes is a gate that would not have caught the
    thing it exists to catch (the self-check `scripts/uli_probe.py` makes of its own harness).
    """
    import copy

    shipped = [(tag, att) for (tag, att) in dump["passes"] if tag == "pass2"][-1]
    trials = []

    def moved_step(d, b):
        d["passes"][shipped]["plan"][0]["g"] += 1

    trials.append(("a plan step", moved_step))

    def moved_header(d, b):
        d["passes"][shipped]["cycles_max"] += 1

    trials.append(("a header cycle field", moved_header))

    if blob["seeks"]:
        def moved_seek(d, b):
            b["seeks"][-1]["g_prev"] += 1

        trials.append(("a seek g_prev", moved_seek))

    for what, mutate in trials:
        d, b = copy.deepcopy(dump), copy.deepcopy(blob)
        mutate(d, b)
        try:
            verify(d, b, say=False)
        except DumpMismatch:
            continue
        raise HarnessError(
            f"the gate accepted a dump with {what} moved by one — it would not catch a "
            f"wrong dump, so its green means nothing"
        )
    print(f"  self-check  a moved plan step, header field and seek entry are all refused")


def run_reference(exe: Path, album: Path, tracks: Path, out: Path, plan: Path, flags: list[str]) -> None:
    argv = [
        str(exe),
        "--album",
        str(album),
        "--tracks",
        str(tracks),
        "--out",
        str(out),
        "--dump-plan",
        str(plan),
        *flags,
    ]
    cp = subprocess.run(argv, capture_output=True, text=True)
    if cp.returncode != 0:
        tail = "\n".join(cp.stderr.splitlines()[-12:])
        raise HarnessError(
            f"the patched reference exited {cp.returncode}:\n{tail}\n  argv: {' '.join(argv)}"
        )
    for line in cp.stderr.splitlines()[-6:]:
        print(f"  ref| {line}")


def gate_inputs(tmpdir: Path) -> tuple[Path, Path, list[str]]:
    """S8-4's fast cell, generated into `tmpdir`; no corpus required."""
    fail_if(not GEN_FAST.exists(), f"missing {GEN_FAST}")
    cell = tmpdir / "fast"
    cell.mkdir(parents=True, exist_ok=True)
    cp = subprocess.run(
        [sys.executable, str(GEN_FAST)], cwd=cell, capture_output=True, text=True
    )
    if cp.returncode != 0:
        raise HarnessError(f"generating the fast cell failed:\n{cp.stderr.strip()}")
    return cell / "fast.f32", cell / "fast.tsv", FAST_FLAGS


def main() -> int:
    ap = argparse.ArgumentParser(
        description="S8-2's --dump-plan instrument (see the module docstring).",
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    ap.add_argument("--album", type=Path, help="album.f32 (default: generate S8-4's fast cell)")
    ap.add_argument("--tracks", type=Path, help="tracks.tsv (required with --album)")
    ap.add_argument("--out", type=Path, help="blob to write (required with --album)")
    ap.add_argument("--dump-plan", type=Path, help="plan dump to write (default: <out>.plan)")
    ap.add_argument("--plan-dump", type=Path, help="the built uli-lite plan_dump example")
    ap.add_argument(
        "--train-dump",
        type=Path,
        help="the built uli-enc train-dump binary; runs S8-6's trainer gate on the dump",
    )
    ap.add_argument("--budget", type=int, help="--budget for the reference")
    ap.add_argument("--cap", type=float, help="--cap for the reference (percent)")
    ap.add_argument("--threads", type=int, default=4, help="--threads (default 4)")
    ap.add_argument("--dacw", type=int, default=0, help="--dacw (default 0: S8's arm)")
    ap.add_argument("--musearch", type=int, default=1, help="--musearch (default 1)")
    ap.add_argument("--kappa", type=float, default=0.13, help="kappa (default 0.13)")
    ap.add_argument("--no-verify", action="store_true", help="write the dump, skip the blob gate")
    ap.add_argument("--keep", action="store_true", help="keep the temp directory and print it")
    ap.add_argument(
        "--work-dir",
        type=Path,
        help="write the cell, blob and dump here instead of a temp dir (implies --keep)",
    )
    args = ap.parse_args()

    if args.album is not None:
        if args.tracks is None or args.out is None:
            ap.error("--album needs --tracks and --out")
    else:
        for name in ("tracks", "out", "dump_plan", "budget", "cap"):
            if getattr(args, name) is not None:
                ap.error(f"--{name.replace('_', '-')} needs --album")

    if not args.no_verify and args.album is None:
        if args.plan_dump is None:
            ap.error(
                "the gate needs --plan-dump (build it with `make uli-dump-plan`, or pass "
                "the path from `cargo build --example plan_dump`); --no-verify skips the blob check"
            )
        fail_if(
            not args.plan_dump.exists(),
            f"--plan-dump {args.plan_dump} does not exist — build it first",
        )
    if args.train_dump is not None:
        fail_if(
            not args.train_dump.exists(),
            f"--train-dump {args.train_dump} does not exist — build it first",
        )

    if args.work_dir is not None:
        args.work_dir.mkdir(parents=True, exist_ok=True)
        tmpdir = args.work_dir
        temp = None
    elif args.keep:
        tmpdir = Path(tempfile.mkdtemp(prefix="uli-dump-plan-"))
        temp = None
    else:
        temp = tempfile.TemporaryDirectory()
        tmpdir = Path(temp.name)
    try:
        if args.album is None:
            album, tracks, base_flags = gate_inputs(tmpdir)
            flags = [
                *base_flags,
                "--dacw",
                str(args.dacw),
                "--musearch",
                str(args.musearch),
                "--kappa",
                str(args.kappa),
            ]
            out = tmpdir / "fast.uli"
            plan = tmpdir / "fast.plan"
            label = "fast (generated)"
        else:
            album, tracks = args.album, args.tracks
            out = args.out
            plan = args.dump_plan or out.with_name(out.name + ".plan")
            flags = [
                "--dacw",
                str(args.dacw),
                "--musearch",
                str(args.musearch),
                "--threads",
                str(args.threads),
            ]
            if args.budget is not None:
                flags += ["--budget", str(args.budget)]
            if args.cap is not None:
                flags += ["--cap", str(args.cap)]
            flags += ["--kappa", str(args.kappa)]
            label = f"{album}"

        print(f"uli-dump-plan: P2, the reference's own plan ({label})")
        exe, cflags = build(tmpdir)
        run_reference(exe, album, tracks, out, plan, flags)
        fail_if(not plan.exists(), f"the reference exited 0 but wrote no dump at {plan}")

        # Record what produced the dump, so a dump is evidence about a named build and a
        # named flag set rather than an anonymous text file.
        body = plan.read_text()
        header = [
            f"# oracle {GBA_AUDIO}",
            f"# cflags {' '.join(cflags)}",
            f"# argv --album {album} --tracks {tracks} --out {out} --dump-plan {plan} {' '.join(flags)}",
        ]
        plan.write_text("\n".join(header) + "\n" + body)

        dump = parse_dump(plan.read_text())
        print(f"uli-dump-plan: wrote {out} and {plan}")

        if args.no_verify:
            print("uli-dump-plan: --no-verify, the blob gate was not run")
        elif args.album is not None and args.plan_dump is None:
            print("uli-dump-plan: no --plan-dump, the blob gate was not run", file=sys.stderr)
        else:
            cp = subprocess.run(
                [str(args.plan_dump), str(out)], capture_output=True, text=True
            )
            if cp.returncode != 0:
                raise HarnessError(f"plan_dump exited {cp.returncode}:\n{cp.stderr.strip()}")
            blob = parse_blob(cp.stdout)
            print("uli-dump-plan: verifying the dumped shipped plan against the blob's own plan")
            verify(dump, blob)
            prove_the_gate_bites(dump, blob)
            print("uli-dump-plan: OK — the dump is the blob's plan, both passes and the counts")

        if args.train_dump is not None:
            print("uli-dump-plan: S8-6, feeding the pass-1 plan to the trainer")
            cp = subprocess.run(
                [
                    str(args.train_dump),
                    "--dump",
                    str(plan),
                    "--blob",
                    str(out),
                    "--album",
                    str(album),
                    "--tracks",
                    str(tracks),
                    "--kappa",
                    str(args.kappa),
                ],
                capture_output=True,
                text=True,
            )
            if cp.stdout:
                print(cp.stdout, end="")
            if cp.stderr:
                print(cp.stderr, end="", file=sys.stderr)
            if cp.returncode == 2:
                return 2
            if cp.returncode != 0:
                raise HarnessError(f"train-dump exited {cp.returncode}")
        return 0
    except DumpMismatch as e:
        print(f"uli-dump-plan: DIFF — {e}", file=sys.stderr)
        return 2
    except HarnessError as e:
        print(f"uli-dump-plan: {e}", file=sys.stderr)
        return 1
    finally:
        if temp is not None:
            temp.cleanup()
        elif args.work_dir is not None:
            print(f"uli-dump-plan: wrote {args.work_dir}")
        else:
            print(f"uli-dump-plan: kept {tmpdir}")


if __name__ == "__main__":
    sys.exit(main())
