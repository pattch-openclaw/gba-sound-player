# tests — what is committed here, and where each expectation comes from

Integration tests live here because the library itself is `#![no_std]`: an
integration test is compiled as a **separate crate with `std`**, so tests can
use normal file I/O and `include_bytes!` while the lib never touches either.

The rule that applies to every file in this directory: **an expectation is
never the code under test's own output.** Each table is produced by something
outside the crate — the reference encoder/`metaflac`, or an independent Python
reading of the same RFC section — and the generator refuses to write the table
if its own cross-checks disagree. A test that re-emits the crate's output is a
tautology; these are not.

## Generated tables and fixtures

| File | What it pins | Regenerate | Produced by |
|---|---|---|---|
| `frame_header_vectors.txt` | real frame headers: field values, widths, coded-number octets, CRC-8 | `scripts/gen_frame_vectors.sh` → `frame_vectors.py emit` | `flac` 1.5.0 + independent §9.1 reading |
| `frame_run_vectors.txt` | whole-frame runs: offsets, lengths, PCM pins | same script → `frame_vectors.py emit_runs` | `flac` 1.5.0 + `flac -d` PCM + independent Python walk |
| `subframe_body_vectors.txt` | subframe bodies per type (CONSTANT/VERBATIM/FIXED/LPC + wasted bits) | same script → `frame_vectors.py emit_bodies` | `flac` 1.5.0 + independent Bits walk + `flac -d` PCM |
| `stream_vectors.txt` | per-file: the §8.1 block chain, every §8.2 `STREAMINFO` field, `frame_data_offset`, the frame region and **every frame's own byte length / header width / block size / footer-gap bits / number / sample-size code**, where a mis-counted footer would
land (the count of frames carrying padding, and whether the strict filter or only
the bare sync shape matches at those positions), the header-width census, frame-number **form** censuses, the frame-start filter ladder (sync shape only → fields only → strict), what the stream's own declarations can say about its frame count, one number-form mutation + `flac -d` exit codes, and the decoded PCM's MD5/FNV-1a64 | `scripts/gen_stream_vectors.sh` | `metaflac --list` + **three** independent Python readings (§8 structure; §9.1 field/scan filtering with and without the minimal-form rule; a per-frame walk of the subframe bodies that derives each frame's length from its own fields) + `flac -d` PCM for the 16-bit fixtures |
| `fixtures/s1…s5_*.flac` | the five streams the table describes, byte-for-byte what `flac` 1.5.0 wrote | same script (`stream_vectors.py emit`) | `flac` 1.5.0 (the encode commands live in the script next to each `encode_command` row) |
| `gafp_asset_vectors.txt` | what the **packer** recorded for the four packed assets below: every manifest field, every offset-table entry, every frame's length and footer CRC-16, its subframe census slot by slot, per-frame and whole-blob PCM pins, and the three pack-time **refusals** with the reason printed | `scripts/gen_gafp_fixtures.py --check` (which drives `scripts/gafp_pack.py`) | the packer's own strict frame/subframe walk + `flac` 1.5.0 encode + `flac -d` PCM |
| `gfp/p0…x1.gfp` | the four packed GAFP assets that table describes, byte-for-byte what the packer wrote | same generator | `scripts/gafp_pack.py` (the real GAFP writer, item 6b-2) |

Each fixture's *reason for existing* is in the table's `note` rows and in the
module docs of the suite that reads it — `tests/stream_metadata_layout.rs`
explains `s1…s5` (two-block container, every real block type, a real 80-bit
header, a valid 24-bit stream, a stream that declares nothing about its own
length). The same table is read a second way by
`tests/scan_frame_walk_layout.rs` (item 4b's frame walk): it asks the crate to
walk those frames with no index, and compares every frame start, byte length,
header width, block size, frame number and every sample against the rows — plus
the PCM hash, which came out of `flac -d`. It also takes the table's
byte-truncation positions (where a walk that rounded a non-byte-aligned frame end
*down* would stand) and asks this crate's own frame filter each one: it must
refuse that position and report the real frame's start, and the count of positions
where the table's reading accepts a header has to equal the count this crate's
accepts. Where the two readings disagree about a *stream*, the table refuses to be
written; where the crate disagrees with the table, a test fails. The 24-bit fixture is deliberately included in
that suite's framing and depth-boundary tests and deliberately absent from its
PCM comparisons: this crate declines 24-bit decode at the capability boundary,
and the test says so by name rather than skipping quietly. That suite is only
built when the walk is: `tests/scan_frame_walk_layout.rs` is
`required-features = ["scan"]` in `Cargo.toml`, so a default-feature build does
not compile it and `make flac-test` does not run it — `make flac-test-scan`
compiles the walk for the GBA target and runs this suite over the same rows, and
`make flac-test-scan` is deliberately not part of `check-gba-all` because the
thing that must not carry the walk yet is the ROM. The tables themselves are
feature-independent: they are descriptions of the bytes, written by generators
that never call the crate.

A **third** reading of `subframe_body_vectors.txt` is
`tests/rice_two_mechanism_differential.rs` (split-ladder item 5 PR 3): it asks the
crate to decode *every partition of every coded subframe body in the table* by both
of its Rice mechanisms — the production `TableCountKernel` and the feature-gated
`rice_hybrid::HybridRiceKernel` — and requires four independent walks to agree:
(1) a bit-by-bit structure walk that re-derives the residual header, every
partition header and every body width straight from §9.2.5's prose, ignoring
values, and records where each partition's parameter field begins; (2) the
incumbent, decoding from that recorded position; (3) the twin, same position; and
(4) the table's own `exit_bits`, generated by an independent `Bits` walk beside
libFLAC's encoder. Positions are compared mechanism-vs-mechanism *and*
mechanism-vs-width-walk at every partition boundary, so a kernel that consumed one
bit too many is convicted at the partition where it happened rather than at the end
of a block of wrong samples. Two of the vectors additionally pin the *values* to
the reference decoder alone — for a FIXED subframe of order ≤ 2 the residual is the
forward difference of `pcm_samples` at the wasted-stripped scale, so the expected
residuals follow from `flac -d` output without consulting either mechanism; the LPC
vectors ride on `subframe_body_layout.rs`, which already proves the default path's
samples bit-exactly against `flac -d`, so "the twin equals the default here"
transfers that proof. As run: **4 coded vectors, 11 Rice partitions, 1,015 samples,
parameters {0, 2, 3, 6, 8}**, with tripwires in the test that fail the run if the
corpus ever yields fewer partitions, one parameter value, or a token number of
samples. Two limits, stated here because the suite cannot state them by passing:
the corpus contains **no escaped partition** (the escape path belongs to
`decode_rice_partition_with`, which this suite drives through rather than tests, and
whose own witnesses are the layout suites), and its widest Rice parameter is **8** —
so the 5-bit-parameter remainder legs (`order` 17…31) are covered by the module's
own spec-text sweep, not by real streams. The second mechanism is not the default
anywhere, and since PR 4 that is a measured state rather than a deferral: PR 4 took
the on-target cycle reading the flip was gated on (2026-11-06) and the hybrid came
back at **+42.6 / +42.7 cycles per decoded sample slower** than the incumbent
(+9.71% / +2.64% mean per frame, 314/314 frames, 0 inside the layout-noise band).
The suite's second test therefore asserts at source level that the one line naming
the default still names `TableCountKernel` — which is what keeps that decision from
drifting in either direction, and what makes any future flip an attributable change
that owes a newer measurement. Like the walk's suite,
this one is only built with its feature: `required-features = ["rice-hybrid"]`, so a
default-feature build does not compile it and `make flac-test` does not run it —
`make flac-test-rice` compiles the twin for the GBA target and runs it, and it is
deliberately not part of `check-gba-all` for the same reason `flac-test-scan`
isn't: no ROM links that code yet. The same table rows are also the corpus the
module's own unit tests (`src/rice_hybrid.rs`) are paired with in the sense that
they pin the legs the real corpus does not reach; neither set of tests reads the
other's inputs.

## The packed `.gfp` fixtures: what is written, and what is refused (item 6b-2)

The crate's own packed container was always meant to have fixtures, and now it
does. `gfp/` holds four blobs written by `scripts/gafp_pack.py` — the real GAFP
writer, not the 6b-1 stand-in generator — over PCM that `scripts/frame_vectors.py`
synthesizes with integer arithmetic only, encoded by `flac` 1.5.0.
`gafp_asset_vectors.txt` is the facts table the same packer emits beside them,
and `tests/gafp_asset_facts.rs` reads both back through `Manifest`,
`scan::Frames` and `Decoder`: every offset-table entry compared with where a
strict decode walk actually lands, every frame's footer CRC-16 recomputed from the
bytes the packer put in the region, and the PCM checked against the `flac -d` pins
frame by frame so a wrong frame names itself. `scripts/gen_gafp_fixtures.py` is
the generator; `make gafp-pack-check` is the gate that runs it in `--check` mode
and also requires the packer's bytes to equal the committed **product** blob
(`examples/flac_integration/assets/spike_l0.gfp`) byte for byte.

The names carry the profile facts, because the profile is what these fixtures are
for: `p` = accepted at the product profile, `x` = accepted only because a guard was
relaxed, then the predictor kind, channel count, target rate and block size.

| File | What it covers |
|---|---|
| `gfp/p0_fixed0_mono_32k_b2048.gfp` | mono, FIXED-0, and a **904-sample final frame** (5,000 samples in 2048-sample blocks — the ragged tail is the point) |
| `gfp/p2_fixed0_stereo_32k_b2048.gfp` | independent stereo, same PCM |
| `gfp/p3_fixed0_midside_32k_b2048.gfp` | mid/side: the side slot coded at bps + 1 and decorrelated back |
| `gfp/x1_lpc_stereo_32k_b2048.gfp` | real **LPC-4** subframes — written only under `--allow-lpc --allow-over-budget`, and recorded as NON-PRODUCT |
| `gafp_asset_vectors.txt` | the packer's record of all four, plus three refusal rows |

The three stereo blobs are **one 5,000-sample source encoded three ways**, so they
carry one PCM pin and three different frame sets. That is deliberate, and it is the
reason the set exists: the header's max-FIXED-order byte reads `0` for all three —
including the all-LPC one, where it is *not* a mistake, because an all-LPC stream
genuinely has no FIXED subframe of order above zero — and
`format::EncodeProfile::validate` accepts all three, including under a hand-made
`max_fixed_order: 0` profile. Decoded, all three are correct and identical.
Nothing on this side of the line can tell the 42.48%-of-the-frame asset from the
174.14%-of-the-frame one; the pack-time subframe census can, and the fixture
records which guard did the work and what the measured cost was. Item 6b-2 of
`flac-docs/plan-of-record.md` has the numbers and the refusal text.

**Still planned, and not covered by the four above:** 1024-sample blocks
(`stereo_32k_b1024_indep` in the original plan), CONSTANT and VERBATIM
subframe shapes as *packed* assets, an escaped residual partition
(`order 0b1111`), and a packed `lpc_order14.gfp`: `x1` reaches order 4, the
highest LPC order any real-stream table here carries is **8**
(`subframe_body_vectors.txt`), so orders above 8 are untested against anything an
encoder actually wrote. Two
entries from that original list are instead answered by refusal rows, and that is
the honest state rather than an oversight: an 8-bit stream is refused at pack time
(`bits per sample 8 is outside the container's [16]`, and its `-l 0` subframes
land on VERBATIM, a predictor kind this target has never measured — so the answer
is a refusal naming an unmeasured kind, not a guessed figure), and 44.1 kHz is
refused at a 32,000 Hz target by both the stream-level and the frame-level
reading. Neither is an asset this crate is expected to play.

Hand-built bit patterns for `bits::BitReader` stay as `#[cfg(test)]` unit tests
inside `src/` — `std` is reachable to unit tests only through the host harness,
so they stay `core`-only and assert on explicit values.

## Negative / robustness cases

Truncated frame, corrupt sync, reserved field values (`0b1111` sample rate,
reserved subframe/metadata types), unsupported sample size, non-monotonic
manifest offsets, offsets pointing outside the blob, wrong magic/version. Each
must yield the specific `Error` variant — never a panic, never an infinite
loop. (On a ROM a panic is a frozen game; fuzzing this later is cheap and
worth it.) Where a negative case is a *policy* rather than a format rule —
refusing a non-minimal coded number, refusing a missing leading `STREAMINFO`,
refusing a 24-bit depth — the divergence from the reference decoder is
recorded with a measurement, not asserted: see `bits::read_utf8_coded`'s docs
and the `mutation*` rows of `stream_vectors.txt`.

Item 4b's negative cases are **hand-cut regions of the committed fixtures**
(a slice ending in the middle of a frame, a slice ending on a frame boundary
holding less audio than the file declared, bytes appended after the last frame,
a real frame spliced with one byte deleted, a real frame copied to the end of
the region), not new fixture files: each of those is a claim about the walk's
behaviour rather than about what encoders write, and a fixture would have
implied the latter. The one thing a fixture *is* needed for — a stream that
cannot tell you its own length — already exists as `s5`.

The two end-of-region **claims** are staged the same way — a fixture's real frames
handed a count that is not theirs — because a self-consistent file cannot
contradict its own frame count while agreeing with its sample count (both numbers
come from the same arithmetic in `STREAMINFO`), so that arm is reachable only when
a claim and a region come from different streams. Staging it also pins which of
the two diagnoses answers first when both are contradicted.

## The width differential (`width_two_mechanism_differential.rs`)

Item 6a-6's narrow-leg witness, built only with `i32-integrator` on
(`required-features`). It has no table of its own: it reads
`subframe_body_vectors.txt` and pins the narrow integrators to the same
`flac -d` PCM the default path has always been witnessed against, checks the
guard at its boundary (including E4's two failing shapes), and checks the
per-frame report's combination. The i64 leg is the companion configuration
(`make flac-test-wide`), pinned to the same oracle — the pair is the
differential.
