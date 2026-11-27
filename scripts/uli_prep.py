#!/usr/bin/env python3
"""Prepare an album for the ULI2 encoder.

Copied into this repository from the sibling `gba-audio` project's
`encoder/prep.py`, with its `eval/gba_dac_model.resample` dependency replaced by
the local `scripts/uli_resample.py`. The goal is that `make uli-album-asset` /
`uli-cross-asset` / `uli-two-track-asset` need no `gba-audio` checkout; ffmpeg,
ffprobe, numpy and scipy remain ordinary external tools.

Reads the tracks, resamples them to 32768 Hz stereo and writes
  build/album.f32   float32 interleaved stereo at PCM16 scale, the tracks joined gaplessly
  build/tracks.tsv  n, start, len, title, artist (ASCII-folded for the GBA text display)

Usage
  uli_prep.py SONG_DIR                 every audio file in SONG_DIR, in filename order
  uli_prep.py a.flac b.mp3 c.wav ...   these files, in this order
  uli_prep.py --tracklist list.json    a JSON list of {"file", "title", "artist"} entries
  uli_prep.py                          no default corpus: use one of the forms above

Any format ffmpeg can read works (FLAC, WAV, MP3, AAC/M4A, Ogg, Opus, ...). Mono
files are copied to both channels. Titles and artists come from the files' tags,
falling back to the file name.
Options: --root DIR (repo root, default: the parent of this script's folder),
--out DIR (default ROOT/build), --jobs N (parallel decodes, default 3).
"""
import argparse, json, os, subprocess, sys, unicodedata
import numpy as np
from concurrent.futures import ProcessPoolExecutor, as_completed
from concurrent.futures.process import BrokenProcessPool

SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))
if SCRIPT_DIR not in sys.path:
    sys.path.insert(0, SCRIPT_DIR)
from uli_resample import resample

FS = 32768
MAX_TRACKS = 32
AUDIO_EXT = {'.flac', '.wav', '.mp3', '.m4a', '.aac', '.ogg', '.oga', '.opus', '.aif', '.aiff', '.wv', '.ape', '.wma', '.mka', '.alac'}
ROOT = os.path.dirname(SCRIPT_DIR)


def ascii_fold(s):
    s = unicodedata.normalize('NFKD', s or '').encode('ascii', 'ignore').decode()
    s = ' '.join(s.replace('\t', ' ').split())
    s = s.replace('( ', '(').replace('()', '').strip()
    if s.startswith('(') and s.endswith(')') and s.count('(') == 1:
        s = s[1:-1]
    return s or '?'


def probe(path):
    """Return (sample_rate, tags) using ffprobe."""
    r = subprocess.run(['ffprobe', '-v', 'error', '-select_streams', 'a:0', '-show_entries',
                        'stream=sample_rate:format_tags:stream_tags', '-of', 'json', path],
                       capture_output=True, text=True)
    if r.returncode != 0:
        sys.exit(f'error: ffprobe could not read {path}:\n{r.stderr}')
    j = json.loads(r.stdout)
    st = j['streams'][0]
    tags = {k.lower(): v for k, v in {**j.get('format', {}).get('tags', {}), **st.get('tags', {})}.items()}
    return int(st['sample_rate']), tags


def decode(path):
    """Decode the first audio stream to float32 stereo at its native rate.

    Runs inside a pool worker, so it raises (never sys.exit: SystemExit kills the
    worker and the task never returns). -xerror makes ffmpeg fail on a corrupt
    or truncated stream instead of silently stopping early."""
    r = subprocess.run(['ffmpeg', '-v', 'error', '-xerror', '-i', path, '-map', '0:a:0', '-ac', '2', '-f', 'f32le', '-'],
                       capture_output=True)
    if r.returncode != 0:
        raise RuntimeError(f'ffmpeg could not decode {path}:\n{r.stderr.decode(errors="replace").strip()}')
    return np.frombuffer(r.stdout, np.float32).reshape(-1, 2)


def job(a):
    i, path, fs, tmp = a   # fs: the sample rate from main()'s ffprobe, so each file is probed only once
    x = decode(path)
    y = resample(x, fs, FS).astype(np.float32)
    np.save(os.path.join(tmp, f'{i:02d}.npy'), y)
    return i, len(y), float(np.abs(y).max()) if len(y) else 0.0


def collect(args):
    if args.tracklist:
        base = os.path.dirname(os.path.abspath(args.tracklist))
        items = json.load(open(args.tracklist))
        return [(os.path.join(base, t['file']), t.get('title'), t.get('artist')) for t in items]
    if not args.inputs:
        sys.exit('error: no audio input; give a folder, audio files, or --tracklist')
    files = []
    for p in args.inputs:
        if os.path.isdir(p):
            files += sorted(os.path.join(p, f) for f in os.listdir(p)
                            if os.path.splitext(f)[1].lower() in AUDIO_EXT and not f.startswith('.'))
        else:
            files.append(p)
    return [(f, None, None) for f in files]


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('inputs', nargs='*', help='a folder of songs, or song files in play order')
    ap.add_argument('--tracklist', help='JSON list of {"file", "title", "artist"}')
    ap.add_argument('--root', default=ROOT)
    ap.add_argument('--out', default=None)
    ap.add_argument('--jobs', type=int, default=3)
    args = ap.parse_args()
    out = args.out or os.path.join(args.root, 'build')
    os.makedirs(out, exist_ok=True)
    TMP = os.path.join(out, 'prep_tmp')
    os.makedirs(TMP, exist_ok=True)

    tracks = collect(args)
    if not tracks:
        sys.exit('error: no audio files found')
    if len(tracks) > MAX_TRACKS:
        sys.exit(f'error: {len(tracks)} tracks; the format holds at most {MAX_TRACKS}')
    for f, _, _ in tracks:
        if not os.path.isfile(f):
            sys.exit(f'error: not found: {f}')

    meta, rates = [], []
    for f, title, artist in tracks:
        fs, tags = probe(f)
        rates.append(fs)
        title = title or tags.get('title') or os.path.splitext(os.path.basename(f))[0]
        artist = artist or tags.get('artist') or tags.get('album_artist') or ''
        meta.append((ascii_fold(title)[:120], ascii_fold(artist)[:150]))

    print(f'decoding and resampling {len(tracks)} tracks to {FS} Hz ...', flush=True)
    # ProcessPoolExecutor, not multiprocessing.Pool: if a worker dies (killed,
    # crashed) the pending futures fail with BrokenProcessPool instead of
    # waiting forever. A worker's exception is re-raised here by f.result().
    res = []
    ex = ProcessPoolExecutor(max(1, args.jobs))
    futs = {ex.submit(job, (i, f, fs, TMP)): f for i, ((f, _, _), fs) in enumerate(zip(tracks, rates))}
    try:
        for fut in as_completed(futs):
            try:
                res.append(fut.result())
            except BrokenProcessPool:   # every pending future fails, so we can't tell which track it was
                print('error: a decode worker died (killed or crashed, e.g. out of memory) while preparing the tracks',
                      file=sys.stderr, flush=True)
                sys.exit(1)
            except Exception as e:
                msg = str(e) if isinstance(e, RuntimeError) else f'{type(e).__name__}: {e}'
                print(f'error: could not prepare {futs[fut]}:\n{msg}', file=sys.stderr, flush=True)
                sys.exit(1)
    finally:   # on error: drop the queued tracks, wait only for the ones already decoding
        ex.shutdown(wait=True, cancel_futures=True)
    res.sort()
    lens = [n for _, n, _ in res]
    for (i, n, peak), (f, _, _) in zip(res, tracks):
        warn = '  (peaks above full scale; it will clip)' if peak > 1.0 else ''
        print(f'  {i + 1:2d}. {os.path.basename(f)}: {n / FS:7.1f} s{warn}', flush=True)

    starts = np.concatenate([[0], np.cumsum(lens)[:-1]]).astype(np.int64)
    tot = int(sum(lens))
    mm = np.memmap(os.path.join(out, 'album.f32'), np.float32, 'w+', shape=(tot, 2))
    for i, s in enumerate(starts):
        y = np.load(os.path.join(TMP, f'{i:02d}.npy'))
        mm[s:s + len(y)] = y * 32768.0
        os.remove(os.path.join(TMP, f'{i:02d}.npy'))
    mm.flush()
    del mm
    os.rmdir(TMP)

    with open(os.path.join(out, 'tracks.tsv'), 'w') as f:
        f.write('# n\tstart\tlen\ttitle\tartist (32768 Hz samples)\n')
        for i, (s, n, (title, artist)) in enumerate(zip(starts, lens, meta)):
            f.write(f'{i + 1}\t{s}\t{n}\t{title}\t{artist or "?"}\n')

    minutes = tot / FS / 60
    kbps = (33292288 * 8) / (tot / FS) / 1000
    print(f'album: {len(tracks)} tracks, {minutes:.1f} min -> about {kbps:.0f} kbit/s in a 32 MiB cart')
    if minutes > 70:
        print('warning: albums much longer than about an hour get noticeably lower quality')


if __name__ == '__main__':
    main()
