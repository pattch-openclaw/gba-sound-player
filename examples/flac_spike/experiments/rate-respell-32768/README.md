# 32,768 Hz by re-spelling, not re-encoding — measured 2026-11-11, **not a product asset**

The probe behind two records: the "rate question, measured from the shipped
frames" row in [`flac-docs/measurements.md`](../../../../flac-docs/measurements.md)
(6b-3) and the uncommon-form row in
[`flac-docs/format-notes.md`](../../../../flac-docs/format-notes.md). It was
**run on 2026-11-11 and not committed at the time**, so the record had to spell
its method out and no reader could re-run it; it is committed now under the
standing rule that probes live in `examples/flac_spike/experiments/<name>/`
([`method-and-gates.md`](../../../../flac-docs/method-and-gates.md)).

**Sample code only**, like the other archives here: nothing in the product path
imports it, and it is not a gate. `python3 probe.py` (needs `flac` on `PATH`;
decoder only, no encoder).

## The question it answers

FLAC's frame header carries the sample rate in a 4-bit code, and **32,768 Hz has
no code of its own** — the table's `0b1000` is 32,000, and that is the asset this
repository ships. So can the shipped asset be made to declare the rate it is
played at, and what does each legal spelling cost?

*(Superseded by plan item 7-0, which landed on 2026-11-13: the product asset now declares
32,768 Hz, spelled the other way — `0b1101` plus a 16-bit Hz field in every frame header,
which is why its frame headers are two bytes longer than the `0b1000` spelling this probe
walked. This probe stays as the measurement that priced the re-encode (**−20 c/frame**,
under the instrument's floor), and it is also why the packer's cost row carries a provenance
caveat: the figure quoted against the current asset was taken on the 32,000-era frames this
probe ran against.)*

| spelling | what it does | cost, measured |
|---|---|---|
| `0b0000` | "take the rate from STREAMINFO" | **0 B/frame — zero. Region 865,340 B in, 865,340 B out.** |
| `0b1101` | exact Hz as a 16-bit field | **2 B/frame, +314 B over the region** (157 frames) |

Both decode to the **pinned PCM** `0x54C7B356621B6E15`, byte-for-byte — which is
the point: *the rate is a spelling*. Re-spelling moves no sample, so it cannot
move the pitch either. The 41 cents lives in the resampling that produced the
source (`Q14`), and only re-encoding at 32,768 Hz from Hz moves it — plan item
7-0. That is exactly why 7-0 must not be satisfied by re-spelling.

## Measured values (2026-11-11, this script, as committed)

```
blob: 865996 B, 157 frames, region 865340 B at 0x290, declares 32000 Hz
frames: 156 of 2048 samples, last 512; every header walks, every CRC-16 verifies
spelling 0b0000 (sample rate comes from STREAMINFO): payload 865340 B (+0 against the shipped region = 0 B/frame); flac -t ok; PCM 0x54C7B356621B6E15 [MATCH], 1280000 B; stream 865382 B = wrapper 42 B + payload
spelling 0b1101 (exact Hz as a 16-bit field): payload 865654 B (+314 against the shipped region = 2 B/frame); flac -t ok; PCM 0x54C7B356621B6E15 [MATCH], 1280000 B; stream 865696 B = wrapper 42 B + payload
```

`0b1101` is also what `flac` 1.5.0 emits when simply asked to encode at 32,768 Hz
(no `--lax` needed), and `crates/flac-lite` already reads that form — its
`s3_wide_48001hz_b3000` fixture is an exact-Hz header (`SampleRate::Explicit(48001)`).

## Two traps, both of which turn a false pass into a true one

1. **STREAMINFO's blocksize fields must carry the FULL block, 2048/2048**, even
   though the last frame is 512 samples — that is what libFLAC's own encodes
   record for a fixed-blocksize stream with a short tail. With `512/2048` libFLAC
   reads the stream as variable-blocksize and prints `sample or frame number does
   not increase correctly` once per frame. **`flac -t` still exits 0 while doing
   it** (this was verified: 157 warnings, status 0), so `flac -t`'s status alone
   is not evidence a re-spelled stream is well-formed. This probe therefore fails
   on *any* warning, not just on a nonzero status, and additionally pins the
   decoded PCM.
2. **`flac -d` writes a 44-byte RIFF/WAVE header whatever the output file is
   called**, so hashing the *file* turns a bit-exact decode into a false mismatch.
   The probe walks the chunks and hashes `data`.

A third, quieter one: the repository's FNV-1a-64 uses its **house offset basis**
`0xCBF43CE95DE684B7`, not the textbook `0xCBF29CE484222325`
(`examples/flac_spike/src/checksum.rs::FNV_OFFSET`,
`scripts/gen_gafp_blob.py::fnv1a64`). The textbook basis produces a mismatch that
is only a different hash.

## What it does not answer

- **The pitch.** Re-spelling moves no sample; the decoded bytes *are* the shipped
  PCM. Nothing here can fix 41 cents.
- **Whether such a blob may ship.** `gafp_pack.py` refuses it, by design:
  `fixed0 x314: no measured decode figure for (32768 Hz, 2048-sample frames,
  fixed0)`. `--allow-unmeasured` writes it under a `this is NOT a product blob`
  note, and the note comes off only when 7-1's **hardware** row exists at
  `(32768, 2048, fixed, 0)` — an mGBA pair cannot fill that key.
- **The cost of the stream 7-0 will actually encode.** This is the shipped
  frames with a field rewritten; a real 32,768 Hz encode has a fresh predictor
  census and ~2.4% more samples for the same playing time, so neither its size
  nor its decode cost is this one's.

## Provenance

The recorded run reconstructed the same two numbers (zero and +314 B, PCM match)
plus two product-image re-runs against the re-spelled blob, which are quoted in
`flac-docs/measurements.md`; the throwaway `.gfp` it produced
(`p32768.gfp`, 865,996 B) is not committed, and this script does not build a
`.gfp` at all — it builds the `.flac` the claims are about. Frame CRC-16s are
verified *before* rewriting, so a parser drift fails loudly instead of silently
re-spelling the wrong bytes.
