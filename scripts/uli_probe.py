#!/usr/bin/env python3
"""S8-1's P1 — the frozen-plan probe: hold the plan still, vary nothing, emit integers.

Why this exists. S7's matrix compares whole blobs, so a difference can only be localised
to a container region ("2,204 of 5,212 table bytes differ"). That cannot separate three
layers that produce those bytes: the trainer, the plan fed to it, and the per-frame coder.
The first two can be compared at their own level (P2/P3), but the third needs both encoders
driven at the *same* step and the *same* M/S flag with the *same* table set — a condition
neither CLI offers, since both search. That is what this probe is for, and it is the
like-for-like A/B S3 relied on whose instrument (`tools/rd_ab`, cited by
`scripts/uli_rd_ab.py`) is not in this tree.

What it is. A C program, generated into a temp directory and compiled against the off-tree
reference's own `enc_core.c`, so it calls the reference's `frame_code` and `band_rdo`
directly. Same posture as `gen_uli_band_rdo_fixture.py`, `gen_uli_mdct_fixture.py` and
`gen_uli_default_tables.sh`: no C enters this repository, nothing here builds C, and the
target that runs it is on no gate's path (E-R1 forbids the dependency, not the oracle).

Two modes, both integer/text, both diffable:

  frame      one line per (frame, g, ms): bits, distortion as hex f64 bits, the reader
             cycle estimate, and the frame's own bit stream as hex halfwords. The stream is
             the point: it is what "byte-identical at a frozen plan" means, and the g delta
             is included when `--g-prev` says what the previous frame's step was, so the
             stream is the frame's on-wire bytes given that predecessor.
  band-rdo   one line per frozen (coefficients, class, step, clamp, ncod) case, in exactly
             `tests/fixtures/band_rdo.txt`'s format. This mode is the probe's own
             verification: it must reproduce all 2,400 pinned cases byte for byte, or the
             harness is wrong and nothing else it says can be trusted.

Why `frame` mode does not print per-band magnitudes. `band_rdo` returns them, but the
per-band *choice* is made inside `chan_code`, which is static and reaches its inputs through
`frame_code`'s preamble (the M/S rotation, `step_D`, the magnitude clamp). Reproducing that
preamble here would be a second implementation of arithmetic that has to mean one thing, and
a drift in it would be silent. The magnitudes are therefore reported at the level where they
are defined — the band-rdo mode — and the frame level reports the bit stream, which any
magnitude difference shows up in. `--stats` adds the per-class symbol counts for S8-6.

Usage:
    make uli-probe                                  # the gate: build + self-check
    scripts/uli_probe.py --self-check               # same, directly
    scripts/uli_probe.py --coef coef.f32 --g 100 --ms 0
    scripts/uli_probe.py --coef coef.f32 --g 100 --tables trained.uli:offset:len
    scripts/uli_probe.py --pcm album.f32 --n-samples 2048 --dump-coef coef.f32

Exit status: 0 on success, 1 on a harness/verification failure (never a bare traceback).
"""

import argparse
import os
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
GBA_AUDIO = Path(os.environ.get("GBA_AUDIO", ROOT.parent / "gba-audio"))

# The frozen differential fixture, and the two committed 4-frame coefficient corpora the
# MDCT golden is made of (binary f32, frame-major, 2*M floats per frame).
BAND_RDO = ROOT / "crates/uli-enc/tests/fixtures/band_rdo.txt"
MDCT_IN = ROOT / "crates/uli-enc/tests/fixtures/mdct_in.f32"
MDCT_COEF = ROOT / "crates/uli-enc/tests/fixtures/mdct_ref_coef.f32"

M = 512                    # hop; one frame is 2*M coefficients (L then R)
FRAME_FLOATS = 2 * M
FROZEN_CASES = 2400

PROBE = r'''
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <math.h>
#include "enc_core.c"   /* for the static chan_code, frame_code, band_rdo and rec_of */

#define FRAME_FLOATS (2 * M)   /* L[M] then R[M]: one frame of the coefficient corpus */

/* A table set from the blob format, so the probe can code with *trained* tables — the
 * oracle's or ours. Layout is the format's (enc_core.c:tables_blob): k, ctype[k],
 * rbits[k], esck[k], len_cb[k+1][k+1], len_q[k][81], len_p[k][256], dq[k][16], pad to 4.
 * `tables_default` runs first only so the fields the blob does not carry (mu) are sane. */
static int load_tables(const char *path) {
    FILE *f = fopen(path, "rb");
    if (!f) { perror(path); return -1; }
    static unsigned char b[16384];
    size_t n = fread(b, 1, sizeof b, f);
    fclose(f);
    if (n < 64) { fprintf(stderr, "%s: %zu bytes, too short for a table blob\n", path, n); return -1; }
    size_t o = 0;
    int k = b[o++];
    if (k < 1 || k > ULI_MAXK) { fprintf(stderr, "%s: k = %d out of range\n", path, k); return -1; }
    size_t need = 1 + 3u * k + (size_t)(k + 1) * (k + 1) + (size_t)k * (81 + 256 + 16);
    if (n < need) { fprintf(stderr, "%s: %zu bytes, layout needs %zu for k = %d\n", path, n, need, k); return -1; }   /* the trailing pad to 4 is optional here */
    T.K = k;
    memcpy(T.ctype, b + o, k); o += k;
    memcpy(T.rbits, b + o, k); o += k;
    memcpy(T.esck,  b + o, k); o += k;
    for (int c = 0; c <= k; c++) { memcpy(T.len_cb[c], b + o, k + 1); o += k + 1; }
    for (int c = 0; c < k; c++) { memcpy(T.len_q[c], b + o, 81); o += 81; }
    for (int c = 0; c < k; c++) { memcpy(T.len_p[c], b + o, 256); o += 256; }
    for (int c = 0; c < k; c++) { memcpy(T.dq[c], b + o, 16); o += 16; }
    tables_derive(&T);
    return 0;
}

/* ---------------------------------------------------------------- band-rdo mode */
/* Reads the frozen fixture and re-emits it with every *output* recomputed — the format is
 * gen_uli_band_rdo_fixture.py's exactly, so `diff` is the whole comparison. */
static int band_rdo_mode(const char *in_path, const char *out_path) {
    FILE *in = fopen(in_path, "r"), *o = fopen(out_path, "w");
    if (!in) { perror(in_path); return 1; }
    if (!o) { perror(out_path); return 1; }
    unsigned xb[16];
    int cls, g, mmax, ncod;
    char line[4096];
    int cases = 0;
    while (fgets(line, sizeof line, in)) {
        if (sscanf(line, "%x %x %x %x %x %x %x %x %x %x %x %x %x %x %x %x %d %d %d %d",
                   &xb[0], &xb[1], &xb[2], &xb[3], &xb[4], &xb[5], &xb[6], &xb[7],
                   &xb[8], &xb[9], &xb[10], &xb[11], &xb[12], &xb[13], &xb[14], &xb[15],
                   &cls, &g, &mmax, &ncod) != 20) {
            fprintf(stderr, "band-rdo: cannot parse: %s", line);
            fclose(in); fclose(o);
            return 1;
        }
        float x[16];
        for (int i = 0; i < 16; i++) memcpy(&x[i], &xb[i], 4);
        double d = uli_step[g] / 16.0, lam = 0.13 * d * d;
        int mo[16];
        double bo = 0;
        double J = band_rdo(x, cls, d, mmax, ncod, lam, mo, &bo, NULL);
        if (!isfinite(J)) { fprintf(stderr, "band-rdo: case %d is not representable\n", cases); fclose(in); fclose(o); return 1; }
        /* Recover D from the magnitudes chosen, exactly as the fixture generator does:
         * band_rdo returns J = D + lam*bits, not D. */
        double D = 0;
        for (int i = 0; i < 16; i++) {
            double e = fabs(x[i]) - rec_of(cls, mo[i]) * d;
            D += e * e;
        }
        for (int i = 0; i < 16; i++) fprintf(o, "%08x ", xb[i]);
        fprintf(o, "%d %d %d %d ", cls, g, mmax, ncod);
        { unsigned long long db; memcpy(&db, &D, 8); fprintf(o, "%016llx ", db); }
        fprintf(o, "%d ", (int)bo);
        for (int i = 0; i < 16; i++) fprintf(o, "%d%s", mo[i], i == 15 ? "\n" : " ");
        cases++;
    }
    fclose(in); fclose(o);
    if (cases == 0) { fprintf(stderr, "band-rdo: no cases in %s\n", in_path); return 1; }
    return 0;
}

/* ---------------------------------------------------------------- frame mode */
static void dump_stats(FILE *o, const EncStats *st) {
    fprintf(o, "frames %.0f\n", st->frames);
    fprintf(o, "ms_frames %.0f\n", st->ms_frames);
    fprintf(o, "esc %.0f\n", st->esc);
    for (int a = 0; a <= T.K; a++) for (int s = 0; s <= T.K; s++)
        if (st->cb[a][s] != 0) fprintf(o, "cb %d %d %.0f\n", a, s, st->cb[a][s]);
    for (int c = 0; c < T.K; c++) {
        for (int s = 0; s < 81; s++) if (st->q[c][s] != 0) fprintf(o, "q %d %d %.0f\n", c, s, st->q[c][s]);
        for (int s = 0; s < 256; s++) if (st->p[c][s] != 0) fprintf(o, "p %d %d %.0f\n", c, s, st->p[c][s]);
        for (int m = 0; m < 16; m++) if (st->cen_n[c][m] != 0) {
            unsigned long long sb;
            memcpy(&sb, &st->cen_s[c][m], 8);
            fprintf(o, "cen %d %d %016llx %.0f\n", c, m, sb, st->cen_n[c][m]);
        }
    }
}

static int frame_mode(const char *coef_path, const char *stats_path, int g_prev,
                      double kappa, double mu, int *gs, int ngs, int *mss, int nmss) {
    FILE *in = fopen(coef_path, "rb");
    if (!in) { perror(coef_path); return 1; }
    FILE *stf = NULL;
    if (stats_path) { stf = fopen(stats_path, "w"); if (!stf) { perror(stats_path); fclose(in); return 1; } }
    EncStats st; memset(&st, 0, sizeof st);
    float coef[FRAME_FLOATS];
    int64_t f = 0, lines = 0;
    for (;;) {
        size_t got = fread(coef, sizeof(float), FRAME_FLOATS, in);
        if (got == 0) break;
        if (got != FRAME_FLOATS) {
            fprintf(stderr, "%s: frame %lld is %zu floats, not %d\n", coef_path, (long long)f, got, FRAME_FLOATS);
            fclose(in); if (stf) fclose(stf);
            return 1;
        }
        for (int gi = 0; gi < ngs; gi++) for (int mi = 0; mi < nmss; mi++) {
            int g = gs[gi], ms = mss[mi];
            /* A real bit buffer, because the stream is the witness. Sized from a counting
             * pass so an overflow is impossible rather than merely unlikely. */
            BW cnt = { NULL, 0, 0 };
            bw_put_eg(&cnt, zz(g - g_prev), 0);
            frame_code(coef, g, ms, kappa, mu, &cnt, NULL);
            uint64_t cap = cnt.pos / 16 + 64;
            uint16_t *buf = calloc(cap + 8, 2);
            BW w = { buf, 0, cap + 8 };
            bw_put_eg(&w, zz(g - g_prev), 0);
            FrameRes r = frame_code(coef, g, ms, kappa, mu, &w, stf ? &st : NULL);
            unsigned long long db;
            memcpy(&db, &r.D, 8);
            uint64_t hw = (w.pos + 15) / 16;
            fprintf(stdout, "frame %lld g %d ms %d bits %.0f D %016llx cycles %u halfwords %llu stream ",
                    (long long)f, g, ms, r.bits, db, r.cycles, (unsigned long long)hw);
            for (uint64_t i = 0; i < hw; i++) fprintf(stdout, "%04x", buf[i]);
            fprintf(stdout, "\n");
            free(buf);
            /* The frame's own bit count must account for exactly the bits written, or the
             * probe is measuring a different coder than the one that emits. */
            if (w.pos < 1) { fprintf(stderr, "frame %lld: empty stream\n", (long long)f); fclose(in); if (stf) fclose(stf); return 1; }
            lines++;
        }
        f++;
    }
    fclose(in);
    if (stf) { dump_stats(stf, &st); fclose(stf); }
    if (lines == 0) { fprintf(stderr, "%s: no frames\n", coef_path); return 1; }
    return 0;
}

/* ---------------------------------------------------------------- mdct mode */
/* The convenience that makes a coefficient corpus out of PCM: the reference's own
 * mdct_frame, which our MDCT reproduces bit for bit (tests/mdct.rs). Written frame-major,
 * 2*M little-endian floats per frame — the layout mdct_ref_coef.f32 already has. */
static int mdct_mode(const char *pcm_path, const char *out_path, int64_t n_samples,
                     int64_t first, int64_t count) {
    FILE *in = fopen(pcm_path, "rb");
    if (!in) { perror(pcm_path); return 1; }
    float *album = malloc((size_t)n_samples * 2 * sizeof(float));
    if (!album) { fprintf(stderr, "out of memory for %lld samples\n", (long long)n_samples); fclose(in); return 1; }
    if (fread(album, 8, (size_t)n_samples, in) != (size_t)n_samples) {
        fprintf(stderr, "%s: expected %lld stereo samples\n", pcm_path, (long long)n_samples);
        fclose(in); free(album); return 1;
    }
    fclose(in);
    mdct_init();
    FILE *o = out_path ? fopen(out_path, "wb") : stdout;
    if (!o) { perror(out_path); free(album); return 1; }
    float coef[FRAME_FLOATS];
    for (int64_t f = first; f < first + count; f++) {
        mdct_frame(album, n_samples, f, coef);
        if (out_path) { if (fwrite(coef, 4, FRAME_FLOATS, o) != FRAME_FLOATS) { perror(out_path); free(album); fclose(o); return 1; } }
        else {
            fprintf(o, "frame %lld", (long long)f);
            for (int i = 0; i < FRAME_FLOATS; i++) { unsigned b; memcpy(&b, &coef[i], 4); fprintf(o, " %08x", b); }
            fprintf(o, "\n");
        }
    }
    free(album);
    if (out_path) fclose(o);
    return 0;
}

int main(int argc, char **argv) {
    T.K = 0; tables_default(&T);
    if (argc < 2) { fprintf(stderr, "usage: probe band-rdo|frame|mdct ...\n"); return 2; }
    const char *mode = argv[1];
    if (strcmp(mode, "band-rdo") && strcmp(mode, "frame") && strcmp(mode, "mdct")) {
        fprintf(stderr, "unknown mode %s (band-rdo | frame | mdct)\n", mode);
        return 2;
    }
    if (!strcmp(mode, "band-rdo")) {
        if (argc != 4) { fprintf(stderr, "usage: probe band-rdo IN OUT\n"); return 2; }
        return band_rdo_mode(argv[2], argv[3]);
    }
    /* every other mode takes flags */
    const char *coef = NULL, *stats = NULL, *pcm = NULL, *out = NULL;
    int g_prev = 0, gs[64], ngs = 0, mss[2], nmss = 0;
    double kappa = 0.13, mu = 0.0;
    int64_t n_samples = 0, first = 0, count = 1;
    for (int i = 2; i < argc; i++) {
        if (!strcmp(argv[i], "--coef")) coef = argv[++i];
        else if (!strcmp(argv[i], "--stats")) stats = argv[++i];
        else if (!strcmp(argv[i], "--pcm")) pcm = argv[++i];
        else if (!strcmp(argv[i], "--n-samples")) n_samples = atoll(argv[++i]);
        else if (!strcmp(argv[i], "--frame")) first = atoll(argv[++i]);
        else if (!strcmp(argv[i], "--frames")) count = atoll(argv[++i]);
        else if (!strcmp(argv[i], "--out")) out = argv[++i];
        else if (!strcmp(argv[i], "--tables")) { if (load_tables(argv[++i])) return 1; }
        else if (!strcmp(argv[i], "--g")) { if (ngs >= 64) { fprintf(stderr, "too many --g\n"); return 2; } gs[ngs++] = atoi(argv[++i]); }
        else if (!strcmp(argv[i], "--ms")) { if (nmss >= 2) { fprintf(stderr, "too many --ms\n"); return 2; } mss[nmss++] = atoi(argv[++i]); }
        else if (!strcmp(argv[i], "--mu")) mu = atof(argv[++i]);
        else if (!strcmp(argv[i], "--kappa")) kappa = atof(argv[++i]);
        else if (!strcmp(argv[i], "--g-prev")) g_prev = atoi(argv[++i]);
        else { fprintf(stderr, "unknown option %s\n", argv[i]); return 2; }
    }
    if (pcm) return mdct_mode(pcm, out, n_samples, first, count);
    if (!coef) { fprintf(stderr, "frame mode needs --coef FILE\n"); return 2; }
    if (!ngs) { gs[ngs++] = 60; gs[ngs++] = 100; gs[ngs++] = 140; }
    if (!nmss) { mss[nmss++] = 0; mss[nmss++] = 1; }
    /* The reader's own window: the step index is g - 4*ms, and it must index uli_step.
     * Refused here rather than read out of bounds — a probe that silently codes at a step
     * the format cannot express would be measuring arithmetic no decoder can undo. */
    for (int i = 0; i < ngs; i++) for (int j = 0; j < nmss; j++) {
        int idx = gs[i] - 4 * mss[j];
        if (idx < 0 || idx >= ULI_NSTEP) {
            fprintf(stderr, "g %d with ms %d is step index %d, outside the reader's 0..%d\n",
                    gs[i], mss[j], idx, ULI_NSTEP - 1);
            return 2;
        }
    }
    return frame_mode(coef, stats, g_prev, kappa, mu, gs, ngs, mss, nmss);
}
'''

# The probe includes enc_core.c, which calls uli_cycle_model (uli_dec_ref.c) and
# uli_album_parse (uli_album.c) and reads uli_step (uli_tables.c), so those link.
LINK = ("src/uli/uli_tables.c", "src/uli/uli_dec_ref.c", "src/uli/uli_album.c")


class HarnessError(Exception):
    pass


def build(tmpdir: Path) -> Path:
    """Write the probe into `tmpdir`, compile it against the off-tree reference, return it."""
    enc = GBA_AUDIO / "encoder"
    if not (enc / "enc_core.c").exists():
        raise HarnessError(
            f"no reference encoder source at {enc}\n"
            f"  uli-probe compiles the reference's own enc_core.c; set GBA_AUDIO= to the "
            f"checkout that has it (E-R1 keeps this off every gate path, so nothing else "
            f"will tell you it is missing)"
        )
    probe = tmpdir / "probe.c"
    probe.write_text(PROBE)
    cc = os.environ.get("CC", "cc")
    cflags = os.environ.get("CFLAGS", "-O2 -std=gnu11 -w").split()
    exe = tmpdir / "probe"
    argv = [cc, *cflags, "-I", str(enc), "-o", str(exe), str(probe),
            *(str(GBA_AUDIO / p) for p in LINK), "-lm"]
    cp = subprocess.run(argv, capture_output=True, text=True)
    if cp.returncode != 0:
        raise HarnessError(f"compiling the probe failed:\n{cp.stderr.strip()}")
    return exe


def run(exe: Path, *args: str) -> subprocess.CompletedProcess:
    cp = subprocess.run([str(exe), *args], capture_output=True, text=True)
    if cp.returncode != 0:
        raise HarnessError(f"probe {' '.join(args)} exited {cp.returncode}:\n{cp.stderr.strip()}")
    return cp


def first_diff(mine: str, theirs: str, what: str) -> None:
    """Raise with the first differing line and field, or return if identical."""
    a, b = mine.splitlines(), theirs.splitlines()
    if a == b:
        return
    for i, (la, lb) in enumerate(zip(a, b)):
        if la == lb:
            continue
        fa, fb = la.split(), lb.split()
        for j, (xa, xb) in enumerate(zip(fa, fb)):
            if xa != xb:
                raise HarnessError(
                    f"{what}: line {i + 1} field {j + 1} differs: probe {xa!r}, golden {xb!r}\n"
                    f"  probe : {la[:160]}\n  golden: {lb[:160]}"
                )
        raise HarnessError(f"{what}: line {i + 1} differs ({len(fa)} vs {len(fb)} fields)")
    raise HarnessError(f"{what}: {len(a)} lines from the probe, {len(b)} from the golden")


def self_check(exe: Path, tmpdir: Path) -> None:
    """The gate: the probe is only worth its output if it reproduces pinned truth."""
    # 1. band_rdo, all 2,400 frozen cases, byte for byte.
    out = tmpdir / "band_rdo.txt"
    run(exe, "band-rdo", str(BAND_RDO), str(out))
    mine = out.read_text()
    golden = BAND_RDO.read_text()
    if len(mine.splitlines()) != FROZEN_CASES:
        raise HarnessError(
            f"band-rdo: {len(mine.splitlines())} lines, expected {FROZEN_CASES}"
        )
    first_diff(mine, golden, "band_rdo reproduction")
    print(f"  band_rdo   {FROZEN_CASES} frozen cases reproduced byte for byte")

    # 2. The MDCT convenience against the committed coefficient golden.
    coef = tmpdir / "coef.f32"
    run(exe, "mdct", "--pcm", str(MDCT_IN), "--n-samples", "2048",
        "--frame", "0", "--frames", "4", "--out", str(coef))
    if coef.read_bytes() != MDCT_COEF.read_bytes():
        raise HarnessError(
            "mdct: the probe's coefficients differ from tests/fixtures/mdct_ref_coef.f32 — "
            "the PCM path is not the reference's own transform"
        )
    print(f"  mdct       4 frames of {FRAME_FLOATS} coefficients match the golden")

    # 3. frame mode runs, is deterministic, and the stats path does not move the totals.
    plain = run(exe, "frame", "--coef", str(MDCT_COEF), "--g", "100", "--ms", "0").stdout
    again = run(exe, "frame", "--coef", str(MDCT_COEF), "--g", "100", "--ms", "0").stdout
    first_diff(again, plain, "frame determinism")

    stats = tmpdir / "stats.txt"
    with_stats = run(exe, "frame", "--coef", str(MDCT_COEF), "--g", "100", "--ms", "0",
                     "--stats", str(stats)).stdout
    first_diff(with_stats, plain, "frame totals with --stats")
    if not stats.read_text().strip():
        raise HarnessError("--stats wrote nothing for a frame that codes")

    lines = plain.splitlines()
    if len(lines) != 4:
        raise HarnessError(f"frame mode: {len(lines)} lines for 4 frames")
    # Shape: exactly the documented keys, in order, with a stream that fills its halfwords.
    want = ["frame", "g", "ms", "bits", "D", "cycles", "halfwords", "stream"]
    for i, line in enumerate(lines):
        keys = line.split()[0::2]
        if keys != want:
            raise HarnessError(f"frame mode: line {i + 1} keys {keys} != {want}")
        hw = int(line.split()[line.split().index("halfwords") + 1])
        stream = line.split()[-1]
        if len(stream) != 4 * hw:
            raise HarnessError(
                f"frame mode: line {i + 1} declares {hw} halfwords but prints "
                f"{len(stream) // 4} ({len(stream)} hex chars)"
            )
        if hw == 0:
            raise HarnessError(f"frame mode: line {i + 1} has an empty stream")
    print(f"  frame      4 frames at g=100 ms=0: deterministic, stats-inert, shape ok")


def main() -> int:
    ap = argparse.ArgumentParser(
        description="S8-1's frozen-plan probe (see the module docstring).",
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    ap.add_argument("--self-check", action="store_true",
                    help="build and verify the probe against pinned truth (the gate, and the default)")
    ap.add_argument("--coef", type=Path, help="binary f32 coefficient corpus, 2*M floats per frame")
    ap.add_argument("--g", type=int, action="append", dest="gs", help="frozen step (repeatable)")
    ap.add_argument("--ms", type=int, action="append", dest="mss", choices=(0, 1),
                    help="frozen M/S flag (repeatable; default both)")
    ap.add_argument("--mu", type=float, default=0.0, help="frozen cycle price (default 0)")
    ap.add_argument("--kappa", type=float, default=0.13, help="the RD price (default 0.13)")
    ap.add_argument("--g-prev", type=int, default=0,
                    help="previous frame's step, so the stream carries the g delta")
    ap.add_argument("--tables", type=Path, help="a tables blob (k, ...) to code with")
    ap.add_argument("--stats", type=Path, help="write the per-class symbol counts here (S8-6's input)")
    ap.add_argument("--pcm", type=Path, help="derive coefficients from this album.f32")
    ap.add_argument("--n-samples", type=int, default=0, help="album length for --pcm")
    ap.add_argument("--frame", type=int, default=0, help="first frame for --pcm")
    ap.add_argument("--frames", type=int, default=1, help="how many frames for --pcm")
    ap.add_argument("--dump-coef", type=Path, help="with --pcm: write coefficients here (binary f32)")
    args = ap.parse_args()

    interactive = args.coef or args.pcm
    if not interactive and not args.self_check:
        args.self_check = True     # bare `uli_probe.py` means the gate

    try:
        with tempfile.TemporaryDirectory() as td:
            tmpdir = Path(td)
            exe = build(tmpdir)
            if args.self_check:
                print("uli-probe: building the frozen-plan probe against the reference")
                self_check(exe, tmpdir)
                if not interactive:
                    print("uli-probe: OK — the harness reproduces pinned truth")
                    return 0
            if args.pcm:
                if not args.n_samples:
                    raise HarnessError("--pcm needs --n-samples (the album's length in samples)")
                argv = ["mdct", "--pcm", str(args.pcm), "--n-samples", str(args.n_samples),
                        "--frame", str(args.frame), "--frames", str(args.frames)]
                if args.dump_coef:
                    argv += ["--out", str(args.dump_coef)]
                cp = subprocess.run([str(exe), *argv], capture_output=False)
                if cp.returncode != 0:
                    raise HarnessError("mdct mode failed")
                return 0
            if args.coef:
                argv = ["frame", "--coef", str(args.coef),
                        "--mu", repr(args.mu), "--kappa", repr(args.kappa),
                        "--g-prev", str(args.g_prev)]
                for g in args.gs or []:
                    argv += ["--g", str(g)]
                for ms in args.mss or []:
                    argv += ["--ms", str(ms)]
                if args.tables:
                    if not args.tables.exists():
                        raise HarnessError(f"no table blob at {args.tables}")
                    argv += ["--tables", str(args.tables)]
                if args.stats:
                    argv += ["--stats", str(args.stats)]
                cp = subprocess.run([str(exe), *argv])
                if cp.returncode != 0:
                    raise HarnessError("frame mode failed")
            return 0
    except HarnessError as e:
        print(f"uli-probe: {e}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
