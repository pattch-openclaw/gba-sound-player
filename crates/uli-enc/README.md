# uli-enc

The host-side ULI2 encoder — **steps S1 through S7**. It takes an album and writes one
ULA2 blob that `uli-lite` (and therefore the GBA decoder) accepts:

* `album.f32` — float32 interleaved stereo at PCM16 scale (samples × 32768),
  joined gaplessly at 32,768 Hz;
* `tracks.tsv` — `n start len title artist`, one row per track, in 32,768 Hz
  samples. This pair is the boundary E-R2 fixes, and it is deliberately narrow: samples
  rather than a waveform format, so a decoder in the loop makes it testable byte for byte.

The **track table sets the album length**: `NS = last.start + last.len`, as in the
table, and `album.f32` must be exactly `NS` samples — no padded tail, no
truncated one. A disagreement is refused with both counts before a frame is planned
(`PLAN-OF-RECORD.md` → "S1 — what the review found", the `NS` ruling). Gaps between tracks and a
first track that starts after sample 0 are fine; overlap and descending order are
not, because they make `NS` ambiguous.

## The WAV front end (S5) is a convenience, not a second boundary

`--wav FILE` (repeatable, in play order) decodes **PCM 8/16/24/32 or float 32, one or two
channels, any rate**, joins the tracks gaplessly and encodes the result. Mono is duplicated
to both channels; titles and artists come from the file's `LIST/INFO` `INAM`/`IART` tags,
falling back to the file stem and `?`, and are carried **verbatim** — the encoder does not
fold text. §5.5's fold has one owner, `scripts/album_prep.py::fold`, used by the bake; a
second implementation here would be a second owner of a rule that has to mean one thing
(and a Latin-1-only table without NFKD turned a Japanese title into an empty string). Source
files at 32,768 Hz are copied **without touching a sample**, which is why such a WAV reaches
*the identical blob* its `album.f32` + `tracks.tsv` pair does — `tests/wav.rs` encodes both
and compares the bytes. Any other rate goes through a polyphase Kaiser-windowed sinc
(`resample.rs`), and E-R2 exempts that resample from the byte contract: it is witnessed by
SNR (a 1 kHz tone at 44,100 Hz keeps >80 dB through the filter), never by a hash.

`--emit-pcm DIR` writes back the `album.f32` and `tracks.tsv` the front end assembled, so the
boundary is auditable: the dump round-trips through `Pcm::open` and encodes to the same blob.
A WAV with neither `--rd` nor `--g` is **refused** rather than written as S1's silence — real
audio encoded as silence is the silent-failure class S3's ledger is about.

## The codec

**`g` is bounded on the way out.** `Plan::new` takes the per-frame `GPlan` of
`(g, ms)` and refuses any frame whose step index `g - 4*ms` falls outside the
reader's `0..NSTEP`, because that is the decoder's own check and an unbounded `g`
means a whole album written, every encoder-side check passed, and a `BadFrame` on
hardware. It is the decoder's window; the search's own `GMIN..GMAX` range is narrower
and is a policy about where good rates live — refusing legal albums there would trade a
format guarantee for a heuristic.

**`--rd` searches, and can choose M/S per frame.** The two channels are rotated to
`M = (L+R)/√2`, `S = (L−R)/√2` — orthonormal, so the rotated-domain distortion *is* the
output-domain distortion — and quantised at `step·√2/16`, which is what the reader's plain
integer add inverts. At each step the two domains are weighed on one scale (λ from the
L/R unit, so the two are compared on one scale). `--ms` forces it; absent, the search
decides, and most real stereo picks it.

**Per frame and per band it applies `J = D + kappa·D²·R`**, which is
what lets a coefficient be *dropped* rather than coded — the difference between S2's
rate at a fixed step and the rate a distortion-only quantiser spends. `--cap` constrains
it: a candidate whose reader cycle model exceeds it is not offered. The bit target is then
solved by bisecting λ, and the plan is rebuilt until the *real* stream fits the budget.
The candidate grid is a **single uniform sweep, and it has no flag** (S8-0). The two-stage
sweep this README used to describe — a coarse grid, then one-step resolution around each
frame's coarse optimum — is deleted rather than defaulted off: it was a repair for a coarse
grid this encoder invented, and the acceptance target is the reference's flag surface, on
which the candidate set is not a setting. It returns at S9 only as a labelled arm admitted by
the SNR licence test. The sweep's *shape* is S8-7's work: the reference searches contiguous
windows of 28 and then 20 steps, and reproducing them is what byte-exactness needs.

**`--train` is the shipped path and `--no-train` the A/B arm (S4).** One pass measures the
album's own symbol counts and reconstruction errors, which rebuild the Huffman lengths and
`dq`; the second pass searches and codes with those tables, and that second plan is the one
that ships. It costs a second search and buys ~0.9 dB at a binding budget.

**`--threads N` covers the search, not just the emit.** Frame bodies were already
independent (`Curves` was not): `Curves::compute` is now frame-parallel, and
that is where a 43-minute album's minutes go. Assembly is by frame index and each frame's
float sums stay in their written order, so `--threads 1` and `--threads N` produce the same
bytes (R5a/R5b) — `tests/determinism.rs` witnesses it for the RD path as well as emit.

**`--musearch` re-codes an over-cap frame instead of refusing it (S6), and it is on by
default.** When `--cap` would exclude a candidate, the search raises a price on decoder
cycles — `mu · (band symbol + the band's decode work)` on every band transition, the same
constants the reader's `cycle_model` uses — until the frame fits, then bisects to the
smallest price that does, and the final encode re-codes at exactly that price. A frame no
price can bring under the cap is left for the allocation to exclude, as the reference does.
Measured on 20 s of the reference album at `--budget 400000 --cap 74`, the switch is worth
**1.44 dB** (39.57 → 41.01 dB) with `est. cycles max` at 74.0% both ways, against the
reference's 1.48 dB — 22% of frames are re-coded there. That is a **high-rate** figure: at
the album's own 199 B/frame only 3.5% of a `cross` slice's frames bind the cap and the
switch is worth 0.02 dB, so it is not the lever for the album-scale gap. It is inert where
the cap does not bind: at `--budget 40000` the blob is byte-identical with it on and off, and at
`--budget 120000` it moves 9 frames' bytes and not the SNR. `--musearch 0` is the A/B arm;
the CLI prints the album line's `musearch on|off` **and** how many frames were actually
re-coded, because a flag being on is not the same claim as the price having bound. It is
not free: on that binding slice the wall clock goes 18.1 s → 73.2 s (4.1×), against the
reference's 1.9×, because every over-cap candidate is re-quantised up to 22 times in both
domains.

**With `--g N`, S2 runs.** The forward MDCT reproduces a too-independent-to-be-lucky
golden of its own coefficients **bit for bit** (`tests/fixtures/mdct_in.f32` →
`mdct_ref_coef.f32`, regenerated by `make uli-mdct-fixture`), its constants are checked in
rather than called for (E-R5's rule R5c), and the fixed-step quantiser offers the same
candidate set — `{0, floor t, floor t + 1}`, first minimum wins — rather than rounding
honestly, which would encode different magnitudes and so a different blob. Without `--g` or
`--rd` the album is S1's silence, because the container and the refusals should not depend
on the codec.

## What it prints

The CLI prints **the reference's columns** (`uli_enc.c`), because S7's acceptance matrix
compares the two encoders and a diff has to be readable: the album summary, the container
overhead and bit target, the written size against the budget, the `g` range and M/S share,
the verify block's per-track `pcm16 / int8` SNR with the mean and worst lines over them,
and the cycle estimates as percentages of the 262,144-cycle period. Two deliberate differences: it prints `dacw off`
where the reference prints its DAC-weight pass number (that pass does not exist here, and a
made-up `0` would read as a knob that ran), and it reports **both** the pooled album codec
SNR and the mean of the per-track column, because E-R7 is stated against the pooled number
while the reference's `verify:` line is the mean.

## The acceptance matrix (S7)

`make uli-accept` runs `scripts/uli_accept.py`: this encoder with `--rd` and the C
reference on identical PCM and identical flags, then compares the two blobs byte for
byte. Four cells (the generated fixture; the whole `biomaster` album; a 60 s `cross`
slice; the album at `--cap 60` and half its budget) plus one oracle-only `--dacw 1` row
that exists to price the objective this encoder does not implement. Per cell: MATCH or
DIFF, both SHA-256s and sizes, the header cycle fields at offsets 68/72, the oracle's
codec proxy, our mean **and worst** SNR, the searched `g` range — and on a DIFF, the
first differing offset with its region, the first content difference at or after
`tracks_off`, per-region diff counts, how many seek entries move `g_prev` and by how
much, and the first **five** seek entries whose bit position or `g_prev` disagrees.
Exit 2 is DIFF, 1 is a harness error; `--list` runs nothing.

Three things to know before reading its output. It needs the **off-tree reference
binary** (`$GBA_AUDIO/build/uli_enc`), so it is deliberately on no gate path — run it
when the search or the container changes. The corpus is not in this checkout: pass
`ULI_ACCEPT_ASSETS=` (to make or in the environment) pointing at
`examples/uli_bench/assets` in the sibling `ulc-gba` tree; the default is the local,
gitignored path. The binary under test is named by the Makefile (`--ours`); without it
the script globs `crates/uli-enc/target`, where a second host triple's build can sort
first. And the oracle always gets `--dacw 0`, the `weights_plain` objective our search
is written against — measured, that pin is inert on every cell here, because the DAC
weights only reweight *between* tracks and every cell is single-track.

The SNR columns are **full-band time-domain** on both sides. E-R7's 32.29 mean /
25.78 worst dB is a **codec-only proxy** measured elsewhere; the oracle's own codec
proxy is printed beside ours so the two are never interchanged.

**Its verdict is DIFF on every cell**, and that is the measured state rather than a harness
fault: the first content difference lands inside the **trained tables** in all four, and it is
**unattributed**. Closing it is S8, whose plan — five instruments, a 21-row divergence
inventory, nine sub-steps — is in
[`../uli-lite/PLAN-OF-RECORD.md`](../uli-lite/PLAN-OF-RECORD.md) → "S8 — the plan". Until it
closes, the encoder matches the reference's **`--dacw 0`** arm only in intent, never in bytes.

## What it is not (yet)

* **No in-place streaming**: the album's samples are held in memory (1.7 GB peak on the
  43.5-minute album), which is fine for the bake and is not a format property.

A DC frame (`FrameKind::DcImpulse`) is implemented and round-trip tested: "DC" here means
*one non-zero band-0 coefficient*, a structural witness that a non-zero band codes and
decodes — its amplitude is the quantiser's business, not an audio claim.

## Run

```sh
cargo run --bin uli-enc -- --album album.f32 --tracks tracks.tsv --out album.uli \
    [--budget 33292288] [--cap 74] [--threads N] [--rd] [--no-train] [--musearch 0|1] \
    [--g 100] [--kappa X] [--ms]

cargo run --bin uli-enc -- --wav a.wav --wav b.wav --out album.uli --rd \
    [--emit-pcm DIR]
```

There is a second binary, `frame-dump`, which exists for one purpose: coding frames at a
**frozen** `(g, ms, tables)` and printing the reference's own units, so this coder can be
compared against the oracle with the *search* held out of the picture (`make uli-frame-diff`
does that comparison; `PLAN-OF-RECORD.md` → S8-1 and S8-5 are why it exists). It is a
diagnostic, not a product path:

```sh
cargo run --bin frame-dump -- --coef coef.f32 [--g 60 --g 100] [--ms 0] \
    [--tables trained.bin] [--mu X] [--g-prev N]
```

It prints the album line and the sizing line
(`container overhead N bytes; stream target N bits = N.N bits/frame`) and exits
nonzero, citing needed and allowed bytes, if the album does not fit the budget —
before writing `--out`.

A third diagnostic is the reference's own plan, not ours: `make uli-dump-plan` builds a patched
copy of the oracle's driver off-tree (in a temp directory, no C in this repository) and writes
its **pass-1** plan, every **pass-2** attempt and the per-class counts `tables_from_stats`
consumed. Its gate is that the dumped shipped plan equals the plan decoded from that run's own
blob, so the instrument is checked against the artefact it explains. It is S8-2's instrument
and it does not change this crate. S8-6 consumes it: `make uli-train-diff` rebuilds the initial
tables from the dumped train rounds, feeds the pass-1 plan to `train::collect_frames` every
second frame, and requires the reference blob's 5,212-byte table region — `PLAN-OF-RECORD.md`
→ S8-6. It is the trainer the search work (S8-7) will wire in, not a product path.

## Test

```sh
make uli-test
```

Or by hand, **from a neutral directory** and with the host triple: the repository
root's `.cargo/config.toml` pins the GBA target and `-Zbuild-std`, and cargo
discovers that file by walking up from the *current directory* — so running
`cargo test` inside `crates/uli-enc/` builds for thumbv4t and fails with a storm of
`cannot find type Vec` errors that say nothing about this code. (Being a standalone
workspace isolates the dependency graph; it does not isolate the target.)

```sh
mkdir -p /tmp/gba-sound-player-host-gate && cd /tmp/gba-sound-player-host-gate
cargo test --manifest-path <repo>/crates/uli-enc/Cargo.toml --target <host-triple>
```

The gates that matter, and they are not the same question:

- `tests/roundtrip.rs` — the output is parsed by `uli_lite::Album`, its tables by
  `CodeTables::parse_into`, and every frame is decoded. The decoder, not this crate, says
  whether the blob is real.
- `tests/tables_golden.rs` — the default tables blob equals the **pinned golden's bytes**,
  frozen at `tests/fixtures/uli2_default_tables.bin` and regenerated by
  `make uli-default-tables`. A round-trip structurally cannot catch a blob that parses and
  decodes but differs from the golden, and that difference is the acceptance gate.
- `tests/quant.rs` — the quantiser differentially against 2,400 frozen oracle cases
  (`make uli-band-rdo-fixture`), plus the consumption check that the reader's bit position
  advance equals the bits the plan sized; and, for S6, that the mirrored cycle constants
  rebuild the reader's own model, that a rising `mu` never costs more cycles, and that an
  over-cap candidate is re-coded into the cap rather than dropped.
- `tests/train.rs` — a trained table set is a *usable* set (complete prefix codes, still
  inside the closed-world premise), training pays for itself, and two encodes are byte-identical.
- `tests/wav.rs` — a 32,768 Hz stereo WAV reaches the PCM pair's exact bytes; a resampled
  one is held to SNR; every PCM depth lands on one scale; tags stay verbatim.
- `tests/resample.rs` — the product rate is an untouched copy, DC gain is one, a passband
  tone keeps >80 dB, and the guard band rejects what 32,768 Hz cannot carry.
- `tests/determinism.rs` — `--threads 1` and `--threads 4` agree byte for byte on both the
  silence and the searched album.

`tests/gplan.rs` and `tests/pcm.rs` witness the two refusals above. One of `gplan.rs`'s tests
is the counter-example for the whole slice: it hand-builds an out-of-window stream past
`Plan::new` and shows the container parsing it happily and the reader then refusing frame 0 —
which is what the encoder used to do on its own.
