#!/usr/bin/env python3
"""S7's acceptance matrix: the same PCM and the same flags, twice, and then a byte diff.

Why this exists. Every gate up to S6 checks our encoder against *itself*: the blob
round-trips, the container parses, the search fits its budget, the tables satisfy
E-R4. None of that is the acceptance question, which is narrower and harsher: given
one input and one set of knobs, does our encoder emit the reference's *bytes*? A
MATCH here is E-R2's "same input, same flags, same blob" stated as a measurement
rather than a hope; a DIFF is localised to a container region and, when it is in the
stream, to the first seek entry whose bit position or step disagrees.

What it is NOT. Not a gate. It needs the off-tree reference binary at
`$GBA_AUDIO/build/uli_enc`, which E-R1 keeps off `make uli-test`; the target that
runs it is deliberately on no gate path. It is also not a quality comparison: the
`verify:` SNR lines on both sides are full-band time-domain, and E-R7's 32.29 mean /
25.78 worst dB is a codec-only proxy measured elsewhere. The oracle's own codec
proxy (`pass2 proxy mean codec ... (min ...)`) is printed beside it so the two
metrics are never confused for one another.

The cells, and why these five:

  fast        a generated 4 s, two-track album at a binding budget (`--cap 74`) — S8's
              progress signal: small enough to iterate on, and given as its own
              `album.f32` + `tracks.tsv` pair so neither side needs a flag to truncate an
              album (a truncated album has different trained tables, so it would be a
              different cell rather than a smaller one)
  tiny        the committed fixture generator, `--threads 1` — the smallest blob
              whose every region is non-empty
  biomaster   the one-track reference album, the S3/S4 measurement input
  cross60     a 60 s head slice of the 12-track album: real heterogeneous stereo,
              cut to keep the wall clock honest
  off-default the same album at `--cap 60` and half the budget: a binding cap and
              a binding bit target at once, neither at its default

plus one oracle-only row, `--dacw 1` on the tiny input. That row is not a match
expectation: `--dacw 1` is the reference's DAC-weighted objective, which this encoder
does not implement. It is printed so the size of that unimplemented difference is a
number rather than a guess, and so the `--dacw 0` pin used for every comparison above
has a visible alternative.

Usage:
    scripts/uli_accept.py                 # full matrix
    scripts/uli_accept.py --only fast     # one cell (repeatable, or comma-separated)
    scripts/uli_accept.py --list          # cells and flags, runs nothing
    scripts/uli_accept.py --ours PATH     # which uli-enc to compare (default: the
                                          # newest build under crates/uli-enc/target)

`fast` is first because S8-5…S8-7 are iterated on it: during those sub-steps it runs
unpinned and the signal is the diff (first divergent frame, first divergent table field),
not a hash. The other four cells are the confirmation, and every cell is pinned at S8's
gate.

ULI_ACCEPT_ASSETS points at the corpus (default: examples/uli_bench/assets). It is
required only by the cells that use it, so `--only fast` runs on a checkout with no
generated corpus — which is possible precisely because the fast cell is generated.

Exit status: 0 when every cell MATCHes, 2 when any cell DIFFs, 1 on a harness error
(missing input, encoder failure, unparseable container).
"""

import argparse
import hashlib
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
REF = GBA_AUDIO / "build/uli_enc"
OURS = ROOT / "crates/uli-enc/target"
ASSETS = Path(os.environ.get("ULI_ACCEPT_ASSETS", ROOT / "examples/uli_bench/assets"))
GEN_FIXTURE = ROOT / "crates/uli-lite/tests/fixtures/gen_fixture.py"
# S8-4's cell. Generated, never truncated: `--frames` rewrites the track table, which is a
# different input rather than a different setting, and a truncated album's trained tables
# are not a small cell's answer. See scripts/gen_uli_fast_cell.py.
GEN_FAST = ROOT / "scripts/gen_uli_fast_cell.py"

PROXY = re.compile(r"pass2 proxy mean codec (-?[\d.]+) \(min (-?[\d.]+)\)")
MEAN = re.compile(r"verify: mean (-?[\d.]+) / (-?[\d.]+) dB")
WORST = re.compile(r"verify: worst (-?[\d.]+) / (-?[\d.]+) dB")
PAIR = re.compile(r"(\d+\.\d+)/(\d+\.\d+)")
G_RANGE = re.compile(r"g (?:searched|range) (\d+)\.\.(\d+)")

CYCLE_PERIOD = 262_144  # rd.rs: the cycle model's period, the denominator of the %
SEEK_INTERVAL = 256

# Flags are identical on both sides except for `--rd` (ours) and `--dacw 0` (theirs).
#
# `fast` binds: 20,000 bytes against 5,412 bytes of container overhead leaves ~454
# bits/frame for 257 frames, both encoders write within a hundred bytes of it, and
# `--cap 74` binds on a handful of frames as well (the μ re-code ran on 19 of 257), so the
# cell exercises the budget, the cap and the trained tables at once. Wall clock at four
# threads, measured 2026-11-25: reference 2.4 s, ours 12.2 s.
FAST_FLAGS = ["--budget", "20000", "--cap", "74", "--threads", "4"]
TINY_FLAGS = ["--budget", "300000", "--threads", "1"]
BIOMASTER_FLAGS = ["--budget", "3294000", "--cap", "74", "--threads", "8"]
CROSS_FLAGS = ["--budget", "764500", "--cap", "74", "--threads", "4"]
ADV_FLAGS = ["--budget", "127500", "--cap", "74", "--threads", "4"]
OFF_DEFAULT_FLAGS = ["--budget", "1647000", "--cap", "60", "--threads", "8"]
DACW_FLAGS = ["--dacw", "1", "--budget", "300000", "--threads", "1"]

CROSS_SLICE_SAMPLES = 1_966_080  # 60 s at 32,768 Hz; inside track 01 of cross/


def say(msg: str) -> None:
    print(msg, flush=True)


def fail(msg: str) -> None:
    sys.exit(f"uli_accept: {msg}")


def require(path: Path, why: str = "") -> Path:
    if not path.exists():
        fail(
            f"{path} does not exist{why} (set ULI_ACCEPT_ASSETS if the corpus lives "
            "elsewhere)"
        )
    return path


def run(cmd, cwd: Path | None = None) -> tuple[int, str, str]:
    p = subprocess.run(cmd, cwd=cwd, capture_output=True, text=True)
    return p.returncode, p.stdout, p.stderr


def find_ours(explicit: Path | None) -> Path:
    """The binary under test. The glob below is only a convenience: a second host
    triple's build sorts into it too, and then the lexicographic first is not
    necessarily the one just built, which is why `make uli-accept` names the path."""
    if explicit is not None:
        if not explicit.exists():
            fail(f"--ours {explicit} does not exist; build it first (make uli-accept builds it)")
        # Absolute: the encoders run with cwd set to the work directory, and make
        # hands this path in relative to the repository root.
        return explicit.resolve()
    for profile in ("release", "debug"):
        for target in sorted(OURS.glob(f"*/{profile}/uli-enc")):
            return target.resolve()
    fail(f"no uli-enc binary under {OURS}; build one first (make uli-accept builds it)")


# ------------------------------------------------------------------ container

def header_of(data: bytes) -> dict:
    """The 88-byte header, in the field order album.rs:87-141 reads it."""
    if len(data) < 88:
        raise ValueError(f"blob is {len(data)} bytes, shorter than the 88-byte header")
    u32 = lambda off: struct.unpack_from("<I", data, off)[0]  # noqa: E731
    u16 = lambda off: struct.unpack_from("<H", data, off)[0]  # noqa: E731
    return {
        "magic": u32(0),
        "version": u16(4),
        "header_bytes": u16(6),
        "sample_rate": u32(8),
        "hop": u32(12),
        "n_frames": u32(16),
        "n_samples": u32(20),
        "n_tracks": u32(24),
        "tracks_off": u32(28),
        "tables_off": u32(32),
        "tables_bytes": u32(36),
        "seek_off": u32(40),
        "seek_count": u32(44),
        "seek_interval": u32(48),
        "strings_off": u32(52),
        "strings_bytes": u32(56),
        "stream_off": u32(60),
        "stream_halfwords": u32(64),
        "est_max_cycles": u32(68),
        "est_mean_cycles": u32(72),
    }


REGION_ORDER = ("header", "tracks", "tables", "seek", "strings", "stream")


def regions_of(data: bytes) -> tuple[dict, dict]:
    """Header, and each region as (offset, bytes), asserted to tile the whole file."""
    h = header_of(data)
    regions = {
        "header": (0, h["header_bytes"]),
        "tracks": (h["tracks_off"], 24 * h["n_tracks"]),
        "tables": (h["tables_off"], h["tables_bytes"]),
        "seek": (h["seek_off"], 8 * h["seek_count"]),
        "strings": (h["strings_off"], h["strings_bytes"]),
        "stream": (h["stream_off"], 2 * h["stream_halfwords"]),
    }
    at = 0
    for name, (start, length) in sorted(regions.items(), key=lambda kv: kv[1][0]):
        if start != at:
            raise ValueError(
                f"regions do not tile the blob: {name} starts at {start}, expected {at} "
                f"(header {h['header_bytes']}, tracks {h['tracks_off']}, "
                f"tables {h['tables_off']}, seek {h['seek_off']}, "
                f"strings {h['strings_off']}, stream {h['stream_off']})"
            )
        at = start + length
    if at != len(data):
        raise ValueError(f"regions end at {at}, blob is {len(data)} bytes")
    return h, regions


def region_of(regions: dict, off: int) -> str:
    for name in REGION_ORDER:
        start, length = regions[name]
        if start <= off < start + length:
            return name
    return f"beyond the last region ({off})"


def diff_report(a: bytes, b: bytes, ra: dict, rb: dict) -> list[str]:
    """Where two blobs first differ, in which region, and how much of each region."""
    lines = []
    n = min(len(a), len(b))
    first = next((i for i in range(n) if a[i] != b[i]), n if len(a) != len(b) else None)
    differing = sum(1 for i in range(n) if a[i] != b[i]) + abs(len(a) - len(b))
    if first is None:
        return ["no differing byte"]
    where = region_of(ra, first) if first < n else "length"
    lines.append(
        f"first differing offset {first} ({where}), {differing} differing bytes"
    )
    # The headline offset is the header in every cell, so say where the blob first
    # disagrees in something other than its own derived sizes.
    content_from = min(ra["tracks"][0], rb["tracks"][0])
    content = next((i for i in range(content_from, n) if a[i] != b[i]), None)
    if content is None:
        lines.append("  no content difference: everything at/after tracks_off agrees")
    else:
        lines.append(
            f"first content difference offset {content} ({region_of(ra, content)})"
        )
    for name in REGION_ORDER:
        sa, la = ra[name]
        sb, lb = rb[name]
        if (sa, la) != (sb, lb):
            lines.append(f"  {name:<8} geometry differs: {sa}+{la} vs {sb}+{lb}")
            continue
        if la == 0:
            continue
        ta, tb = a[sa : sa + la], b[sb : sb + lb]
        d = sum(1 for x, y in zip(ta, tb) if x != y)
        if d:
            lines.append(f"  {name:<8} {d:>7} of {la:>8} bytes differ")
    return lines


def seek_diag(a: bytes, b: bytes, ha: dict, hb: dict) -> list[str]:
    """Bit position and previous step at each seek entry, ours against the reference."""
    entries = []
    for h, data in ((ha, a), (hb, b)):
        off, count = h["seek_off"], h["seek_count"]
        entries.append(
            [
                (struct.unpack_from("<I", data, off + i * 8)[0],
                 struct.unpack_from("<H", data, off + i * 8 + 4)[0])
                for i in range(count)
            ]
        )
    ours, ref = entries
    if len(ours) != len(ref):
        return [f"  seek: {len(ours)} entries vs {len(ref)}"]
    diffs = [(i, o, r) for i, (o, r) in enumerate(zip(ours, ref)) if o != r]
    if not diffs:
        return ["  seek: every entry agrees (bitpos and g_prev)"]
    g_moved = [r[1] - o[1] for o, r in zip(ours, ref) if o[1] != r[1]]
    lines = [
        f"  seek: {len(diffs)} of {len(ours)} entries differ",
        f"  seek g_prev: {len(g_moved)} of {len(ours)} entries move it"
        + (f", max |delta| {max(abs(d) for d in g_moved)}" if g_moved else ""),
    ]
    for i, (bp_o, g_o), (bp_r, g_r) in diffs[:5]:
        lines.append(
            f"    entry {i:>4} (frame {i * SEEK_INTERVAL:>6}): "
            f"bitpos {bp_o} vs {bp_r} (delta {bp_r - bp_o:+}), "
            f"g_prev {g_o} vs {g_r} (delta {g_r - g_o:+})"
        )
    if len(diffs) > 5:
        lines.append(f"    … {len(diffs) - 5} more")
    return lines


# ------------------------------------------------------------------ encoders

def encode(kind: str, binary: Path, album: Path, tracks: Path,
           out: Path, flags: list[str], extra: list[str],
           cwd: Path) -> tuple[int, str, str]:
    cmd = [str(binary), "--album", str(album), "--tracks", str(tracks),
           "--out", str(out), *extra, *flags]
    say(f"  running {kind}: {' '.join(cmd)}")
    code, stdout, stderr = run(cmd, cwd=cwd)
    say(f"  {kind} exited {code}")
    return code, stdout, stderr


def mean_worst(stdout: str, stderr: str) -> tuple[tuple[str, str] | None,
                                                  tuple[str, str] | None,
                                                  list[str]]:
    """The verify block's mean and worst figures, and the per-track pcm16 column."""
    text = stdout + "\n" + stderr
    mean = MEAN.search(text)
    worst = WORST.search(text)
    header = re.search(r"per-track time-domain SNR[^\n]*", text)
    tracks = PAIR.findall(header.group(0)) if header else []
    return (
        (mean.group(1), mean.group(2)) if mean else None,
        (worst.group(1), worst.group(2)) if worst else None,
        [p for p, _ in tracks],
    )


def cell_row(ours_out: Path, ref_out: Path, ours_text: tuple[str, str],
             ref_text: tuple[str, str]) -> dict:
    ours_stdout, ours_stderr = ours_text
    ref_stdout, ref_stderr = ref_text
    if not ours_out.exists():
        fail(f"ours did not write {ours_out}")
    if not ref_out.exists():
        fail(f"the reference did not write {ref_out}")
    a, b = ours_out.read_bytes(), ref_out.read_bytes()
    ha, ra = regions_of(a)
    hb, rb = regions_of(b)
    ours_mean, ours_worst, ours_tracks = mean_worst(ours_stdout, ours_stderr)
    _, _, ref_tracks = mean_worst(ref_stdout, ref_stderr)
    proxy = PROXY.search(ref_stdout + ref_stderr)
    g_ours = G_RANGE.search(ours_stdout + ours_stderr)
    g_ref = G_RANGE.search(ref_stdout + ref_stderr)
    return {
        "ours_sha": hashlib.sha256(a).hexdigest(),
        "ref_sha": hashlib.sha256(b).hexdigest(),
        "bytes_ours": len(a),
        "bytes_ref": len(b),
        "h_ours": ha,
        "h_ref": hb,
        "r_ours": ra,
        "r_ref": rb,
        "ours_mean": ours_mean,
        "ours_worst": ours_worst,
        "ours_tracks": ours_tracks,
        "ref_tracks": ref_tracks,
        "proxy": proxy.groups() if proxy else None,
        "g_ours": g_ours.groups() if g_ours else None,
        "g_ref": g_ref.groups() if g_ref else None,
        "diff": diff_report(a, b, ra, rb),
        "seek": seek_diag(a, b, ha, hb),
        "match": a == b,
    }


def print_row(name: str, row: dict, flags: list[str]) -> None:
    verdict = "MATCH" if row["match"] else "DIFF"
    say(f"  {verdict} ({' '.join(flags)})")
    say(f"  ours  {row['bytes_ours']:>9} bytes  sha256 {row['ours_sha']}")
    say(f"  ref   {row['bytes_ref']:>9} bytes  sha256 {row['ref_sha']}")
    ho, hr = row["h_ours"], row["h_ref"]
    for field, off in (("est_max_cycles", 68), ("est_mean_cycles", 72)):
        o, r = ho[field], hr[field]
        say(
            f"  {field} @{off}: ours {o:>7} ({100.0 * o / CYCLE_PERIOD:5.1f}%)"
            f"  ref {r:>7} ({100.0 * r / CYCLE_PERIOD:5.1f}%)"
            f"{'' if o == r else '  <- differs'}"
        )
    if row["proxy"]:
        say(f"  oracle proxy codec mean {row['proxy'][0]} (min {row['proxy'][1]})")
    om, ow = row["ours_mean"], row["ours_worst"]
    say(
        "  ours verify mean {} / worst {} dB (full-band time-domain)"
        .format(
            f"{om[0]}/{om[1]}" if om else "n/a",
            f"{ow[0]}/{ow[1]}" if ow else "n/a",
        )
    )
    if row["ref_tracks"]:
        ref_worst = min(float(x) for x in row["ref_tracks"])
        say(f"  ref  verify worst {ref_worst:.2f} dB (min of its per-track column)")
    say(
        "  g range ours {} ref {}".format(
            "..".join(row["g_ours"]) if row["g_ours"] else "n/a",
            "..".join(row["g_ref"]) if row["g_ref"] else "n/a",
        )
    )
    if not row["match"]:
        for line in row["diff"] + row["seek"]:
            say(line)


# ------------------------------------------------------------------ inputs

def make_tiny(work: Path) -> tuple[Path, Path]:
    require(GEN_FIXTURE, " (the committed fixture generator)")
    d = work / "tiny"
    d.mkdir()
    say(f"  generating the fixture (seed 12345) in {d}")
    p = subprocess.run([sys.executable, str(GEN_FIXTURE)], cwd=d,
                       capture_output=True, text=True)
    if p.returncode != 0:
        fail(f"gen_fixture.py exited {p.returncode}: {p.stderr.strip()}")
    return d / "tiny.f32", d / "tiny.tsv"


def make_cross_slice(work: Path) -> tuple[Path, Path]:
    src = require(ASSETS / "cross/album.f32")
    d = work / "cross60"
    d.mkdir()
    want = CROSS_SLICE_SAMPLES * 8
    with src.open("rb") as f:
        head = f.read(want)
    if len(head) != want:
        fail(f"{src} is shorter than one 60 s slice ({len(head)} < {want} bytes)")
    (d / "album.f32").write_bytes(head)
    (d / "tracks.tsv").write_text(
        "# n\tstart\tlen\ttitle\tartist (32768 Hz samples)\n"
        f"1\t0\t{CROSS_SLICE_SAMPLES}\tcross-01-slice\t?\n"
    )
    return d / "album.f32", d / "tracks.tsv"


def make_fast(work: Path) -> tuple[Path, Path]:
    require(GEN_FAST, " (the committed fast-cell generator)")
    d = work / "fast"
    d.mkdir()
    say(f"  generating the fast cell (seed 20261123) in {d}")
    p = subprocess.run([sys.executable, str(GEN_FAST)], cwd=d,
                       capture_output=True, text=True)
    if p.returncode != 0:
        fail(f"gen_uli_fast_cell.py exited {p.returncode}: {p.stderr.strip()}")
    return d / "fast.f32", d / "fast.tsv"


def cell_names() -> list[str]:
    """The cells that would run here, in matrix order. `cross60` becomes `adv` when the
    regenerated adversarial corpus is present, which is how the matrix has always
    behaved: two corpora, one slot, and the slot's name says which one ran."""
    has_adv = (ASSETS / "adv/album.f32").exists()
    return ["fast", "tiny", "biomaster", "adv" if has_adv else "cross60", "off-default"]


def build_cells(work: Path, wanted: list[str]) -> list[dict]:
    """Build only the selected cells, so `--only fast` needs neither the corpus nor the
    fixture generator: an input is required by the cell that uses it, not by the matrix."""
    has_adv = (ASSETS / "adv/album.f32").exists()
    cells: list[dict] = []
    if "fast" in wanted:
        album, tracks = make_fast(work)
        cells.append({"name": "fast", "album": album, "tracks": tracks,
                      "flags": FAST_FLAGS,
                      "note": "generated 4 s, two tracks, 257 frames — S8's progress signal"})
    if "tiny" in wanted:
        tiny_album, tiny_tracks = make_tiny(work)
        cells.append({"name": "tiny", "album": tiny_album, "tracks": tiny_tracks,
                      "flags": TINY_FLAGS,
                      "note": "generated fixture, 2 s, one track, seed 12345"})
    if "biomaster" in wanted or "off-default" in wanted:
        album = require(ASSETS / "album.f32", " (the biomaster input)")
        tracks = require(ASSETS / "tracks.tsv", " (the biomaster track table)")
        if "biomaster" in wanted:
            cells.append({"name": "biomaster", "album": album, "tracks": tracks,
                          "flags": BIOMASTER_FLAGS,
                          "note": "assets/album.f32 + tracks.tsv (one track, "
                                  "8,474,224 samples)"})
        if "off-default" in wanted:
            cells.append({"name": "off-default", "album": album, "tracks": tracks,
                          "flags": OFF_DEFAULT_FLAGS,
                          "note": "the biomaster input at --cap 60 and half its budget"})
    if "adv" in wanted or "cross60" in wanted:
        if has_adv:
            cells.append({"name": "adv", "album": ASSETS / "adv/album.f32",
                          "tracks": require(ASSETS / "adv/tracks.tsv"),
                          "flags": ADV_FLAGS,
                          "note": "adv/album.f32 + tracks.tsv (regenerated adversarial "
                                  "corpus)"})
        else:
            require(ASSETS / "cross/album.f32", " (the 12-track album)")
            cross_album, cross_tracks = make_cross_slice(work)
            cells.append({"name": "cross60", "album": cross_album, "tracks": cross_tracks,
                          "flags": CROSS_FLAGS,
                          "note": "60 s head slice of cross/album.f32, one "
                                  "1,966,080-sample track"})
    return cells


def describe_cells() -> list[str]:
    return [
        f"fast        generated 4 s, two tracks, {' '.join(FAST_FLAGS)}"
        "  - S8's progress signal",
        f"tiny        generated fixture (seed 12345), {' '.join(TINY_FLAGS)}",
        f"biomaster   assets/album.f32 + tracks.tsv, {' '.join(BIOMASTER_FLAGS)}",
        f"cross60     60 s head slice of cross/album.f32, {' '.join(CROSS_FLAGS)}"
        f"  (adv/album.f32, if present: {' '.join(ADV_FLAGS)})",
        f"off-default assets/album.f32 at {' '.join(OFF_DEFAULT_FLAGS)}",
        f"dacw1       oracle only, tiny input, {' '.join(DACW_FLAGS)}"
        "  - not a match expectation",
    ]


# ------------------------------------------------------------------ main

def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--list", action="store_true",
                    help="print the cells and their flags, run nothing")
    ap.add_argument("--ours", type=Path, default=None, metavar="PATH",
                    help="the uli-enc binary to compare (default: the first build "
                         "found under crates/uli-enc/target)")
    ap.add_argument("--only", default=None, metavar="NAMES",
                    help="run only these cells (comma-separated or repeated). `cross60` "
                         "and `adv` name the same slot; `dacw1` names the oracle-only row "
                         "and pulls in its tiny input")
    args = ap.parse_args()

    if args.list:
        say(f"assets {ASSETS}")
        say(f"cells  {', '.join(cell_names())}  (then the dacw1 row)")
        for line in describe_cells():
            say(f"  {line}")
        return 0

    ours = find_ours(args.ours)
    if not REF.exists():
        fail(f"no reference encoder at {REF} (make -C {GBA_AUDIO}/encoder)")

    wanted = cell_names()
    with_dacw = True
    if args.only:
        asked = [n for part in args.only.split(",") for n in [part.strip()] if n]
        known = set(cell_names()) | {"cross60", "adv", "dacw1"}
        unknown = [n for n in asked if n not in known]
        if unknown:
            fail(f"unknown cell(s) {', '.join(unknown)}; known: "
                 + ", ".join(sorted(known)))
        with_dacw = "dacw1" in asked
        # `dacw1` is a row over the tiny input, so asking for it asks for tiny.
        if with_dacw and "tiny" not in asked:
            asked.append("tiny")
        wanted = [n for n in wanted if n in asked]
        # The one corpus slot: either name selects it, and the slot reports which ran.
        if ("cross60" in asked or "adv" in asked) and not any(
                n in wanted for n in ("cross60", "adv")):
            wanted.append("cross60")

    work = Path(tempfile.mkdtemp(prefix="uli-accept-"))
    errors: list[str] = []
    diffs: list[str] = []
    tiny_ref = None
    try:
        say(f"assets {ASSETS}")
        say(f"ours   {ours}")
        say(f"ref    {REF}")
        cells = build_cells(work, wanted)
        for cell in cells:
            name = cell["name"]
            say("")
            say(f"== {name}: {cell['note']}")
            ours_out = work / f"{name}-ours.uli"
            ref_out = work / f"{name}-ref.uli"
            code, ours_stdout, ours_stderr = encode(
                "ours", ours, cell["album"], cell["tracks"], ours_out,
                cell["flags"], ["--rd"], work)
            if code != 0:
                errors.append(f"{name}: ours exited {code}: {ours_stderr.strip()[-300:]}")
                say(f"  ERROR ours exited {code}")
                continue
            code, ref_stdout, ref_stderr = encode(
                "ref", REF, cell["album"], cell["tracks"], ref_out,
                cell["flags"] + ["--dacw", "0"], [], work)
            if code != 0:
                errors.append(f"{name}: ref exited {code}: {ref_stderr.strip()[-300:]}")
                say(f"  ERROR ref exited {code}")
                continue
            try:
                row = cell_row(ours_out, ref_out, (ours_stdout, ours_stderr),
                               (ref_stdout, ref_stderr))
            except ValueError as e:
                errors.append(f"{name}: {e}")
                say(f"  ERROR {e}")
                continue
            print_row(name, row, cell["flags"])
            if not row["match"]:
                diffs.append(name)
            if name == "tiny":
                tiny_ref = ref_out

        # The dacw row: oracle only, on the tiny input, and explicitly not a match.
        if tiny_ref is None and not with_dacw:
            say("")
            say("dacw1 row skipped: this run did not select it (and it needs the tiny cell)")
        if tiny_ref is not None:
            say("")
            say("== dacw1: oracle only on the tiny input, --dacw 1")
            say("   not a match expectation — the dacw objective we do not have")
            dacw_out = work / "tiny-dacw1.uli"
            code, _, dacw_err = encode(
                "ref", REF,
                work / "tiny" / "tiny.f32", work / "tiny" / "tiny.tsv",
                dacw_out, DACW_FLAGS, [], work)
            if code != 0:
                errors.append(f"dacw1: ref exited {code}: {dacw_err.strip()[-300:]}")
                say(f"  ERROR ref exited {code}")
            else:
                d1 = dacw_out.read_bytes()
                d0 = tiny_ref.read_bytes()
                n = min(len(d1), len(d0))
                dist = sum(1 for i in range(n) if d1[i] != d0[i]) + abs(len(d1) - len(d0))
                say(f"  oracle --dacw 1 blob {len(d1)} bytes  sha256 {hashlib.sha256(d1).hexdigest()}")
                say(f"  oracle --dacw 0 blob {len(d0)} bytes (the tiny cell above)")
                say(f"  distance from its own --dacw 0 run: {dist} bytes differ")
    finally:
        shutil.rmtree(work, ignore_errors=True)

    say("")
    say("Reminder: every SNR above is full-band time-domain. E-R7's 32.29 mean /")
    say("25.78 worst dB is a codec-only proxy; do not quote one against the other.")
    if errors:
        for e in errors:
            print(f"ERROR {e}", file=sys.stderr)
        return 1
    if diffs:
        print(f"DIFF in {', '.join(diffs)}", file=sys.stderr)
        return 2
    say("OK: every cell is byte-identical to the reference")
    return 0


if __name__ == "__main__":
    sys.exit(main())
