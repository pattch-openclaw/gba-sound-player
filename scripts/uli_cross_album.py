#!/usr/bin/env python3
"""Fetch and prepare the 12-track CC-licensed measurement album — PR E's E3-b corpus.

This is the only 43-minute, repertoire-broad corpus this project measures: its
worst frames sat in two of its twelve tracks, and the reference decoder publishes
album-scale pins (104 checkpoint frames, 64 model-heavy frames, 12 track starts).
The tracklist and licence notice now live in this repository
(`scripts/data/uli_cross_tracklist.json` and
`scripts/data/uli_cross_LICENSES.md`), so the recipe is self-contained; the audio
itself is downloaded (CC-licensed) and **no blob is committed** — the WAVs, the
f32 intermediate and the encoded album are build output.

The processing recipe is transcribed from the tracklist's `processing` and
`gain_db` per track: trim, fade-out and gain, then 16-bit / 44.1 kHz WAVs, which
`scripts/uli_prep.py` resamples to the codec's 32,768 Hz. Downloads are cached in
`CACHE`, so a re-run is cheap.

`make uli-cross-asset` is the in-repo target: it runs this script, then
`scripts/uli_prep.py`, the in-repo Rust encoder and the Rust pin tool, writing
`examples/uli_bench/assets/cross/` and `src/album_pin_cross.rs` (all gitignored).
`make uli-accept`'s `cross60` cell uses `cross/album.f32`; the fourth slot becomes
`adv` when the adversarial corpus exists. See `ULI2.md` -> "The measurement corpus".

Usage:
  python3 scripts/uli_cross_album.py [TRACKLIST] OUTDIR [--cache DIR]

Then encode (the budget is the reference album's 33,292,288 less the cart room
this project's instrument needs):
  uli-enc --album OUTDIR/album.f32 --tracks OUTDIR/tracks.tsv --out OUTDIR/cross.uli \
          --rd --budget 33100000 --cap 74
"""

import argparse
import json
import os
import re
import subprocess
import sys
import urllib.parse

FS = 44100
DEFAULT_TRACKLIST = os.path.join(os.path.dirname(os.path.abspath(__file__)), "data", "uli_cross_tracklist.json")


def parse_processing(text):
    """(trim_start, trim_end, fade_out_s) from the tracklist's recipe string."""
    start, end, fade = 0.0, None, 0.0
    m = re.search(r"trim\s+([0-9.]+)-(end|[0-9.]+)\s*s", text or "")
    if m:
        start = float(m.group(1))
        end = None if m.group(2) == "end" else float(m.group(2))
    m = re.search(r"([0-9.]+)s\s+fade-out", text or "")
    if m:
        fade = float(m.group(1))
    return start, end, fade


def download(url, dest):
    if os.path.exists(dest) and os.path.getsize(dest) > 0:
        return dest
    os.makedirs(os.path.dirname(dest), exist_ok=True)
    tmp = dest + ".part"
    print(f"  fetching {url.rsplit('/', 1)[-1]} …", flush=True)
    subprocess.check_call(
        ["curl", "-sSfL", "--retry", "3", "-C", "-", "-o", tmp, url],
    )
    os.replace(tmp, dest)
    return dest


def prepare(src, dest, start, end, fade, gain_db):
    dur = (end - start) if end is not None else 0.0
    af = []
    # atrim first: the sibling trims the source, then fades, then resamples.
    if start or end is not None:
        af.append(f"atrim=start={start}" + (f":end={end}" if end is not None else "") + ",asetpts=PTS-STARTPTS")
    if fade > 0:
        # A fade-out is measured from the trimmed length; with no end, from the
        # decoded duration (measured here rather than guessed).
        f = fade
        if end is None:
            probe = subprocess.check_output(
                [
                    "ffprobe", "-v", "error", "-select_streams", "a:0",
                    "-show_entries", "format=duration", "-of", "csv=p=0", src,
                ],
                text=True,
            ).strip()
            f = fade
            af.append(f"afade=t=out:st={max(0.0, float(probe) - start - fade)}:d={f}")
        else:
            af.append(f"afade=t=out:st={max(0.0, dur - f)}:d={f}")
    if gain_db:
        af.append(f"volume={gain_db}dB")
    af.append("aresample=osf=s16:dither_method=triangular")
    subprocess.check_call(
        [
            "ffmpeg", "-y", "-v", "error", "-i", src, "-map", "0:a:0",
            "-map_metadata", "-1", "-ac", "2", "-ar", str(FS),
            "-af", ",".join(af), "-c:a", "pcm_s16le", dest,
        ]
    )


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("tracklist", nargs="?", default=DEFAULT_TRACKLIST)
    ap.add_argument("outdir")
    ap.add_argument("--cache", default=None)
    args = ap.parse_args()

    out = os.path.abspath(args.outdir)
    cache = os.path.abspath(args.cache or os.path.join(out, "src"))
    wavs = os.path.join(out, "wav")
    os.makedirs(wavs, exist_ok=True)
    tracks = json.load(open(args.tracklist, encoding="utf-8"))
    for t in tracks:
        n = int(t["n"])
        name = os.path.basename(urllib.parse.unquote(t["source_url"]))
        local = os.path.join(cache, f"{n:02d}_{name}")
        print(f"[{n:2d}] {t['title'][:50]}", flush=True)
        download(t["source_url"], local)
        start, end, fade = parse_processing(t.get("processing", ""))
        gain = float(t.get("gain_db") or 0.0)
        dest = os.path.join(wavs, f"{n:02d}.wav")
        prepare(local, dest, start, end, fade, gain)
        print(f"  -> {dest} ({os.path.getsize(dest) / 1e6:.1f} MB, trim {start}-{end}, fade {fade})", flush=True)

    # uli_prep.py does the codec-rate resample and the album assembly.
    prep = os.path.join(os.path.dirname(os.path.abspath(__file__)), "uli_prep.py")
    print(f"prep: {prep}", flush=True)
    subprocess.check_call([sys.executable, prep, wavs, "--out", out])
    print(f"done: {out}/album.f32 + tracks.tsv", flush=True)


if __name__ == "__main__":
    main()
