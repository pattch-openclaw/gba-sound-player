#!/usr/bin/env python3
"""Prepare album track sources for the packer: probe, read tags, fold, clip.

This is the codec-blind half of the album build (ALBUM-PLAYER.md §9.0): it
produces PCM (or a clip of it) and text, never a bitstream. `scripts/album_bake.py`
drives it and `scripts/gafp_pack.py` does the encoding, so nothing here may
learn what a GAFP frame is.

Phase 1 owns exactly three of the tool's jobs (§9.0's table):

  * the **rate half** — 16,384 Hz stereo (which `gafp_pack.py` also enforces from
    the WAV header, and which is why extracting a clip at the product rate here
    is a convenience, not a second rate contract: the packer still refuses a
    source that declares another rate — Q14);
  * the **tag read** — Vorbis comments via `metaflac`, a RIFF `LIST/INFO` block
    for a WAV, the filename stem as the last resort;
  * the **ASCII fold** (§5.5) — NFKD, drop what is not ASCII, collapse
    whitespace, strip a whole title wrapped in parentheses, because the shipped
    font is a 96-glyph subset and a title with an em-dash must not silently
    force a bigger font.

Phase 3 grows the album half — joining the tracks into one continuous timeline
so `start_sample` is an album position — in the same file, never a rewrite.

    python3 scripts/album_prep.py --self-test
    python3 scripts/album_prep.py --probe assets/sound/bioluminescent.wav
    python3 scripts/album_prep.py --fold "Bioluminescent — Öffnung (Mix)"
"""

from __future__ import annotations

import argparse
import struct
import subprocess
import sys
import unicodedata
import wave
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent

#: The one clock the product plays at (Q14; `geometry.rs::PLAYBACK_RATE_HZ`).
PRODUCT_CLOCK_HZ = 16384

#: The widest title box the app's layout carries, in characters. A folded title
#: longer than this is PRINTED by the bake and never refused — §5.5 makes "fix
#: the title" an album decision, so the tool's job is to say so loudly.
TITLE_BOX_MAX = 28


class Refusal(Exception):
    """A named refusal: the rule, and what it was checked against."""

    def __init__(self, rule: str, detail: str) -> None:
        super().__init__(f"{rule}: {detail}")
        self.rule = rule
        self.detail = detail


# --- text (§5.5) ------------------------------------------------------------


def fold(text: str) -> str:
    """§5.5's ASCII fold, in the order the spec states it.

    NFKD first, so an accented letter becomes base + combining mark and the mark
    is what gets dropped (`Öffnung` → `Offnung`, not `?ffnung`); then everything
    outside ASCII; then whitespace collapsed to one space and trimmed; then a
    whole title wrapped in parentheses loses the wrapping (a parenthesised
    *phrase* inside a longer title is left alone — only a title that is entirely
    parenthesised is a wrapped title).
    """
    decomposed = unicodedata.normalize("NFKD", text)
    ascii_only = "".join(ch for ch in decomposed if ord(ch) < 128)
    collapsed = " ".join(ascii_only.split())
    if len(collapsed) >= 2 and collapsed.startswith("(") and collapsed.endswith(")"):
        inner = collapsed[1:-1].strip()
        # A title that is nothing but a parenthesised empty group is not a title.
        if inner:
            collapsed = inner
    return collapsed


def title_fits(text: str) -> bool:
    """Whether a folded title fits the app's widest title box."""
    return len(text) <= TITLE_BOX_MAX


# --- sources: probe, tags, clip --------------------------------------------


def probe(path: Path) -> dict:
    """The source's own header: rate, channels, bits — never the target's.

    A `.flac` is asked through `metaflac` (already a packer dependency); a WAV
    through the `wave` module, which reads the file's header and not its name.
    Refusing an unreadable source here is better than letting ffmpeg guess.
    """
    suffix = path.suffix.lower()
    if suffix == ".flac":
        out = _run(
            ["metaflac", "--show-sample-rate", "--show-channels", "--show-bps", str(path)],
            "metaflac --show-sample-rate",
        )
        rate, channels, bits = (int(line) for line in out.split() if line.strip())
        return {"rate": rate, "channels": channels, "bits": bits}
    if suffix in (".wav", ".wave"):
        try:
            with wave.open(str(path), "rb") as w:
                return {
                    "rate": w.getframerate(),
                    "channels": w.getnchannels(),
                    "bits": w.getsampwidth() * 8,
                }
        except (wave.Error, EOFError) as exc:
            raise Refusal("unreadable source", f"{path}: {exc}") from exc
    raise Refusal("unreadable source", f"{path}: not a .wav or .flac (Phase 1 packs PCM sources)")


def read_tags(path: Path) -> dict:
    """Title/artist from the file's tags, the filename stem as the last resort.

    `metaflac` returns Vorbis comment pairs (`NAME=value`); a WAV carries the
    same two fields as RIFF `LIST`/`INFO` subchunks (`INAM`, `IART`). Anything
    else — or a tag that is absent — falls back to the stem, which is what a
    hand-made fixture will have.
    """
    tags: dict[str, str] = {}
    suffix = path.suffix.lower()
    if suffix == ".flac":
        out = _run(["metaflac", "--export-tags-to=-", str(path)], "metaflac --export-tags-to")
        for line in out.splitlines():
            if "=" in line:
                name, value = line.split("=", 1)
                tags.setdefault(name.upper(), value)
    elif suffix in (".wav", ".wave"):
        tags.update(_riff_info_tags(path))
    title = tags.get("TITLE", "").strip() or path.stem
    artist = tags.get("ARTIST", "").strip()
    return {"title": title, "artist": artist}


def _riff_info_tags(path: Path) -> dict:
    """RIFF `LIST`/`INFO` subchunks: INAM is the title, IART the artist.

    Seek-only. The reference master is 149 MB and its `LIST` block sits beside
    the tiny `fmt` chunk, so reading the file to find two strings would make a
    tag lookup cost the whole master. The walk reads 8-byte chunk headers, reads
    a `LIST` body (metadata, small by definition) and seeks over every other
    body — `data`, nearly all of the file, is never touched.
    """
    size = path.stat().st_size
    tags: dict[str, str] = {}
    wanted = {b"INAM": "TITLE", b"IART": "ARTIST"}
    try:
        handle = path.open("rb")
    except OSError as exc:
        raise Refusal("unreadable source", f"{path}: {exc}") from exc
    with handle:
        head = handle.read(12)
        if len(head) < 12 or head[0:4] != b"RIFF" or head[8:12] != b"WAVE":
            raise Refusal("unreadable source", f"{path}: not a RIFF/WAVE file")
        # The container's declared length, clamped to the file. Both halves
        # matter: a truncated master must not walk the reader past the buffer,
        # and a header that declares a shorter container than the file is a
        # statement about what is in it — a chunk after that end is not part of
        # this RIFF and must not be read as if it were.
        end = min(8 + struct.unpack_from("<I", head, 4)[0], size)
        at = 12
        while at + 8 <= end:
            handle.seek(at)
            header = handle.read(8)
            if len(header) < 8:
                break
            chunk_id = header[0:4]
            chunk_size = struct.unpack_from("<I", header, 4)[0]
            body_at = at + 8
            if chunk_id == b"LIST":
                handle.seek(body_at)
                body = handle.read(min(chunk_size, max(end - body_at, 0)))
                sub = 4
                if body[0:4] == b"INFO":
                    while sub + 8 <= len(body):
                        name = body[sub : sub + 4]
                        sub_size = struct.unpack_from("<I", body, sub + 4)[0]
                        value = body[sub + 8 : sub + 8 + sub_size]
                        if name in wanted:
                            tags[wanted[name]] = value.split(b"\x00", 1)[0].decode(
                                "utf-8", "replace"
                            )
                        sub += 8 + sub_size + (sub_size & 1)
            at = body_at + chunk_size + (chunk_size & 1)
    return tags


def normalise(src: Path, dst: Path) -> dict:
    """Bring a whole source into the product profile: 16,384 Hz stereo s16.

    The same command `gafp_pack.py` would run itself when the WAV header does not
    match the profile (`normalise()`, `:600-603`). The bake does it here so the
    prepared file's frame count is an independent statement of the track's
    length — the number the packed container's `total_samples` is then held to
    (§4.3 rule 1).
    """
    _ffmpeg([], src, dst)
    return _check_profile(dst)


def extract(src: Path, start_sample: int, end_sample: int, dst: Path) -> dict:
    """Cut `[start_sample, end_sample)` out of `src` at the product rate.

    The sample indices are in the SOURCE's own rate (from [`probe`]), so a clip
    boundary that was chosen on the master is cut where it was chosen. ffmpeg
    does the resample to 16,384 Hz / stereo / s16 here, which is the same
    command `gafp_pack.py` would run for a whole file (§9.0's "much of which
    `gafp_pack.py` already does from the WAV header"); doing it once per clip
    keeps the packer's input in its own profile, so it encodes rather than
    re-times.

    Returns the probe of the clip it wrote, after checking that it is in the
    product profile — the packer refuses it either way, but a refusal naming
    *this* tool is cheaper to read than one naming ffmpeg's output.
    """
    info = probe(src)
    if end_sample <= start_sample:
        raise Refusal(
            "clip window",
            f"{src.name}: end_sample {end_sample} is not after start_sample {start_sample}",
        )
    available = frames(src)
    if end_sample > available:
        raise Refusal(
            "clip window",
            f"{src.name}: end_sample {end_sample} is past the source's {available} samples",
        )
    rate = info["rate"]
    prefix = [
        "-ss", f"{start_sample / rate:.9f}",
        "-t", f"{(end_sample - start_sample) / rate:.9f}",
    ]
    _ffmpeg(prefix, src, dst)
    return _check_profile(dst)


def _ffmpeg(prefix: list[str], src: Path, dst: Path) -> None:
    """The product-profile decode command, input options before `-i`."""
    _run(
        [
            "ffmpeg", "-hide_banner", "-loglevel", "error", "-y",
            *prefix,
            "-i", str(src),
            "-ar", str(PRODUCT_CLOCK_HZ), "-ac", "2", "-c:a", "pcm_s16le",
            str(dst),
        ],
        "resample",
    )


def _check_profile(path: Path) -> dict:
    got = probe(path)
    if (got["rate"], got["channels"], got["bits"]) != (PRODUCT_CLOCK_HZ, 2, 16):
        raise Refusal(
            "profile",
            f"{path}: produced {got['rate']} Hz / {got['channels']} ch / {got['bits']} bit, "
            f"the profile is {PRODUCT_CLOCK_HZ} Hz / 2 ch / 16 bit",
        )
    return got


def frames(path: Path) -> int:
    """The source's own sample count, from its header or from `metaflac`."""
    if path.suffix.lower() == ".flac":
        out = _run(["metaflac", "--show-total-samples", str(path)], "metaflac --show-total-samples")
        return int(out.strip())
    with wave.open(str(path), "rb") as w:
        return w.getnframes()


def _run(cmd: list[str], why: str) -> str:
    proc = subprocess.run(cmd, capture_output=True, text=True)
    if proc.returncode != 0:
        raise Refusal(
            "external tool",
            f"{why} failed ({' '.join(cmd)}): {(proc.stderr or proc.stdout).strip()}",
        )
    return proc.stdout


# --- self-test --------------------------------------------------------------


FOLD_CASES = (
    # (in, out, why)
    ("Bioluminescent 1", "Bioluminescent 1", "ASCII already: nothing moves"),
    ("Bioluminescent — Öffnung", "Bioluminescent Offnung", "NFKD then drop: the em-dash is dropped, not transliterated"),
    ("  spaced   out  ", "spaced out", "whitespace collapsed and trimmed"),
    ("(Bioluminescent Clip 2)", "Bioluminescent Clip 2", "a whole wrapped title loses its wrapping"),
    ("A (Bioluminescent) Clip", "A (Bioluminescent) Clip", "a parenthesised phrase is not a wrapped title"),
    ("夜のエラー", "", "a title with no ASCII at all folds to nothing, loudly"),
)


def self_test() -> int:
    """Print the fold's cases; a wrong one is a non-zero exit, not a note."""
    failures = 0
    for text, want, why in FOLD_CASES:
        got = fold(text)
        ok = got == want
        failures += 0 if ok else 1
        print(f"{'PASS' if ok else 'FAIL'} fold({text!r}) = {got!r}  [{why}]")
    # The box test is a report, not a refusal: both answers are correct.
    for text in ("Bioluminescent 1", "Bioluminescent Clip 1 - Offnung (Alternate Mix, Take 12)"):
        folded = fold(text)
        print(f"INFO fits({folded!r}) = {title_fits(folded)} (box {TITLE_BOX_MAX})")
    if failures:
        print(f"album_prep self-test: {failures} failure(s)")
        return 1
    print("album_prep self-test: PASS")
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Album track preparation: probe, tags, fold, clip.")
    parser.add_argument("--self-test", action="store_true", help="print the fold cases and exit")
    parser.add_argument("--fold", metavar="TEXT", help="fold one title and print it")
    parser.add_argument("--tags", metavar="PATH", help="print a source's title/artist tags")
    parser.add_argument("--probe", metavar="PATH", help="print a source's rate/channels/bits")
    parser.add_argument(
        "--extract",
        nargs=4,
        metavar=("SRC", "START_SAMPLE", "END_SAMPLE", "DST"),
        help="cut a clip of SRC at the product rate",
    )
    args = parser.parse_args(argv)
    try:
        if args.self_test:
            return self_test()
        if args.fold is not None:
            print(fold(args.fold))
            return 0
        if args.tags:
            tags = read_tags(Path(args.tags))
            print(f"title={tags['title']}\nartist={tags['artist']}")
            return 0
        if args.probe:
            print(probe(Path(args.probe)))
            return 0
        if args.extract:
            src, start, end, dst = args.extract
            print(extract(Path(src), int(start), int(end), Path(dst)))
            return 0
    except Refusal as exc:
        print(f"album_prep: refused — {exc.rule}: {exc.detail}", file=sys.stderr)
        return 1
    parser.print_help()
    return 2


if __name__ == "__main__":
    sys.exit(main())
