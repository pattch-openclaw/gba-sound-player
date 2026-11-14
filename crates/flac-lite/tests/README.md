# tests — what is committed here, and where each expectation comes from

Integration tests live here because the library is `#![no_std]`: an integration test is
compiled as a **separate crate with `std`**, so a suite can use file I/O and `include_bytes!`
while the lib never touches either.

The rule that applies to every file in this directory:

> **An expectation is never the code under test's own output.** Each table is produced by
> something outside the crate — the reference encoder, `metaflac`, or an independent Python
> reading of the same RFC section — and the generator refuses to write the table if its own
> cross-checks disagree. A test that re-emits the crate's output is a tautology; these are not.

This is not pedantry: the project's worst format bug (a frame header counted as 31 bits
instead of 32) entered through a **hand-packed test vector** and survived review *because* it
looked authoritative. Whoever packed those bytes believed 31 bits, and the test asserted 31
bits. A hand-packed vector cannot witness a field-width claim; encoder output can.

## Generated tables and fixtures

| File | What it pins | Regenerate | Produced by |
|---|---|---|---|
| `frame_header_vectors.txt` | real frame headers: field values, widths, coded-number octets, CRC-8 | `scripts/gen_frame_vectors.sh` → `frame_vectors.py emit` | `flac` 1.5.0 + an independent §9.1 reading |
| `frame_run_vectors.txt` | whole-frame runs: offsets, lengths, PCM pins | same script → `emit_runs` | `flac` 1.5.0 + `flac -d` PCM + an independent Python walk |
| `subframe_body_vectors.txt` | subframe bodies per type (CONSTANT/VERBATIM/FIXED/LPC + wasted bits) | same script → `emit_bodies` | `flac` 1.5.0 + independent `Bits` walk + `flac -d` PCM |
| `stream_vectors.txt` | per fixture: the §8.1 block chain, every §8.2 `STREAMINFO` field, `frame_data_offset`, the frame region, and per frame its byte length / header width / block size / footer-gap bits / number / sample-size code, where a mis-counted footer would land, the header-width and number-form censuses, the frame-start filter ladder (sync shape → fields → strict), seek-table points and what each names, the FNV of the reference PCM from each point, and the decoded PCM's MD5/FNV-1a64 | `scripts/gen_stream_vectors.sh` | `metaflac --list` + **three** independent Python readings (§8 structure; §9.1 field/scan filtering with and without the minimal-form rule; a per-frame subframe walk that derives each frame's length from its own fields) + `flac -d` |
| `fixtures/s1…s6_*.flac` | the six streams that table describes, byte-for-byte what `flac` 1.5.0 wrote | same generator | the encode commands live in the script, beside each `encode_command` row |
| `gafp_asset_vectors.txt` | what the **packer** recorded for the four assets below: every manifest field, every offset entry, every frame's length and footer CRC-16, its subframe census slot by slot, per-frame and whole-blob PCM pins, and the three pack-time **refusals** with the reason printed | `scripts/gen_gafp_fixtures.py --check` | `scripts/gafp_pack.py`'s own strict walk + `flac` 1.5.0 + `flac -d` |
| `gfp/*.gfp` | those four packed assets, byte-for-byte what the packer wrote | same generator | `scripts/gafp_pack.py` |

Each fixture's *reason for existing* is in the table's `note` rows and in the module docs of
the suite that reads it. `tests/stream_metadata_layout.rs` explains `s1…s6`: a two-block
container, every real block type, a real 80-bit header, a valid 24-bit stream, a stream that
declares nothing about its own length, and a real multi-point seek table.

The tables are **feature-independent** — they describe bytes, and are written by generators
that never call the crate. The suites that read them are not: `scan_frame_walk_layout.rs` is
`required-features = ["scan"]` and `rice_two_mechanism_differential.rs` is
`required-features = ["rice-hybrid"]`, so a default build does not compile them and
`make flac-test` does not run them. Each has its own gate (`flac-test-scan`, `flac-test-rice`)
for exactly that reason.

## Reading the same bytes a second and third way

`stream_vectors.txt` and `subframe_body_vectors.txt` are each consumed by three suites, and
each consumption asks a different question:

- **`scan_frame_walk_layout.rs`** asks the crate to walk those frames **with no index** and
  compare every frame start, byte length, header width, block size, frame number and every
  sample against the rows — plus the PCM hash, which came out of `flac -d`. It also starts
  walks **from the seek table's own points** (the byte an entry names, the frame number that
  byte actually carries, the audio from there to the end, hashed from `flac -d`'s output for
  exactly that suffix) and refuses both a wrong anchor and the usual wrong reading of an
  offset. Where the Python reading and this crate disagree about a *stream*, the table refuses
  to be written; where the crate disagrees with the table, a test fails. The 24-bit fixture is
  deliberately included in the framing/depth-boundary tests and deliberately absent from the
  PCM comparisons, because this crate declines 24-bit decode at the capability boundary and
  the test says so by name rather than skipping quietly.
- **`rice_two_mechanism_differential.rs`** decodes *every partition of every coded subframe
  body in the table* by both Rice mechanisms — the production `TableCountKernel` and the
  feature-gated `HybridRiceKernel` — and requires four independent walks to agree: a
  bit-by-bit structure walk that re-derives every header and body width straight from
  §9.2.5's prose, ignoring values; the incumbent, from the position that walk recorded; the
  twin, from the same position; and the table's own `exit_bits`, generated by an independent
  `Bits` walk beside libFLAC's encoder. Positions are compared mechanism-vs-mechanism **and**
  mechanism-vs-width-walk at every partition boundary, so a kernel that consumed one bit too
  many is convicted at the partition where it happened, not at the end of a block of wrong
  samples. As run: 4 coded vectors, 11 Rice partitions, 1,015 samples, parameters
  {0, 2, 3, 6, 8} — with tripwires that fail the run if the corpus ever yields fewer
  partitions, one parameter value, or a token number of samples. Its second test asserts **at
  source level** that the one line naming the default still names `TableCountKernel`, which is
  what keeps the declined flip from drifting in either direction.
- **`width_two_mechanism_differential.rs`** has no table of its own: it reads
  `subframe_body_vectors.txt` and pins the narrow (i32) integrators to the same `flac -d` PCM
  the default path has always been witnessed against, checks the guard at its boundary
  (including E4's two failing shapes) and checks the per-frame width report's combination. The
  i64 leg is the companion *configuration* (`make flac-test-wide`), pinned to the same oracle
  — the pair is the differential, not one suite.

## The packed `.gfp` fixtures: what is written, and what is refused

`gfp/` holds four blobs written by `scripts/gafp_pack.py` over PCM that
`scripts/frame_vectors.py` synthesizes with integer arithmetic only, encoded by `flac` 1.5.0;
`gafp_asset_vectors.txt` is the facts table the same packer emits beside them, and
`gafp_asset_facts.rs` reads both back through `Manifest`, `scan::Frames` and `Decoder` — every
offset entry compared with where a strict decode walk actually lands, every frame's footer
CRC-16 recomputed from the bytes the packer put in the region, and the PCM checked against the
`flac -d` pins frame by frame so a wrong frame names itself. `make gafp-pack-check` is the gate
that runs the generator in `--check` mode and also requires the packer's bytes to equal the
committed **product** blob (`examples/flac_integration/assets/spike_l0.gfp`).

The names carry the profile facts, because the profile is what these fixtures exist for:
`p` = accepted at the product profile, `x` = accepted only because a guard was relaxed, then
predictor kind, channel count, target rate and block size.

| File | What it covers |
|---|---|
| `p0_fixed0_mono_16384_b2048.gfp` | mono, FIXED-0, and a **904-sample final frame** (5,000 samples in 2048-sample blocks — the ragged tail is the point) |
| `p2_fixed0_stereo_16384_b2048.gfp` | independent stereo, same PCM |
| `p3_fixed0_midside_16384_b2048.gfp` | mid/side: the side slot coded at bps + 1 and decorrelated back |
| `x1_lpc_stereo_16384_b2048.gfp` | real **LPC-4** subframes — written only under `--allow-lpc --allow-over-budget`, and recorded as NON-PRODUCT |
| `gafp_asset_vectors.txt` | the packer's record of all four, plus three refusal rows |

The three stereo blobs are **one 5,000-sample source encoded three ways**: one PCM pin, three
different frame sets. That is deliberate, and it is the reason the set exists — the header's
max-FIXED-order byte reads `0` for all three, including the all-LPC one, where it is *not* a
mistake (an all-LPC stream genuinely has no FIXED subframe above order zero), and
`EncodeProfile::validate` accepts all three. Decoded, all three are correct and identical.
Nothing on this side of the line can tell the measured asset from the 4× more expensive one;
the pack-time **subframe census** can, and the fixture records which guard did the work and
what the measured cost was.

**The product guard is FIXED-0 and envelope-limited, not merely FIXED-only.** `-l 0` disables
LPC; libFLAC still chooses FIXED orders 1–4 from the signal, and those orders — like
CONSTANT/VERBATIM, a partition order above 0, wasted bits, an escape partition, or Rice
p > 10 — have no measured row at the product target. The packer refuses them as unmeasured and
writes nothing; `--allow-unmeasured` writes a NON-PRODUCT blob and records the waiver. (The
`--master` path for real programme material is the deliberate exception, and it says so on
every shape it accepts.) The four committed assets sit inside the measured shape: one
partition, `wasted = 0`, no escapes, Rice p ≤ 10 and FIXED-0 — the LPC one via the
over-budget waiver.

**What the four do not cover, and why that is the honest state rather than an oversight.**
There is no committed 1024-sample-block asset, no packed CONSTANT or VERBATIM asset, no packed
escaped partition, and no packed LPC order > 4 asset (`x1` reaches 4; the highest LPC order any
real-stream table here carries is **8**). Two entries from the original wish-list are answered
by **refusal rows** instead, which is a witness too: an 8-bit stream is refused at pack time
(`bits per sample 8 is outside the container's [16]`, and its `-l 0` subframes land on
VERBATIM — a predictor kind this target has never measured), and 44,100 Hz is refused at the
product target by both the stream-level and the frame-level reading
(`rate_44100_at_16384_target`). Note that a rate which *looks* close — 32,000 Hz, exactly
1.024×, +41.06 cents — is refused exactly like 44,100 Hz is: the declared rate is a validity
check and never something the packer reinterprets.

## Negative and robustness cases

Truncated frame, corrupt sync, reserved field values (`0b1111` sample rate, reserved
subframe/metadata types), unsupported sample size, non-monotonic manifest offsets, offsets
pointing outside the blob, wrong magic or version. Each must yield the specific `Error`
variant — never a panic, never an infinite loop. Where a negative case is a **policy** rather
than a format rule — refusing a non-minimal coded number, refusing a missing leading
`STREAMINFO`, refusing a 24-bit depth — the divergence from the reference decoder is recorded
with a measurement, not asserted: see `bits::read_utf8_coded`'s docs and the `mutation*` rows
of `stream_vectors.txt`.

libFLAC never emits an invalid stream, so every refusal test **mutates one field of a real
vector at runtime** and asserts the specific variant; the untouched bytes keep the widths
honest. Item 4b's negative cases are hand-cut regions of the committed fixtures (a slice ending
mid-frame, a slice ending on a frame boundary holding less audio than the file declared, bytes
appended after the last frame, a real frame spliced with one byte deleted, a real frame copied
to the end of the region) rather than new fixture files — each of those is a claim about the
walk's behaviour, not about what encoders write, and a fixture would have implied the latter.
The one thing a fixture *is* needed for — a stream that cannot tell you its own length —
already exists as `s5`. The two end-of-region **claims** are staged the same way (a fixture's
real frames handed a count that is not theirs), because a self-consistent file cannot
contradict its own frame count while agreeing with its sample count — both numbers come from
the same arithmetic in STREAMINFO — so that arm is reachable only when a claim and a region
come from different streams. Staging it also pins which of the two diagnoses answers first when
both are contradicted.

Hand-built bit patterns for `bits::BitReader` stay as `#[cfg(test)]` unit tests inside `src/`:
`std` is reachable to unit tests only through the host harness, so they stay `core`-only and
assert on explicit values.
