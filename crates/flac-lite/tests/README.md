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

## The `.gfp` fixtures that are still planned

The original gate plan targeted the crate's own packed container. Nothing here
uses `.gfp` yet, and that plan is superseded for correctness work by the real
`.flac` tables above; the `.gfp` fixtures remain relevant to the
`format::Manifest` facade step:

| Fixture | Profile axis under test |
|---|---|
| `mono_32k_b2048_f2.gfp` | mono, FIXED low order |
| `stereo_32k_b2048_midside.gfp` | mid/side decorrelation |
| `stereo_32k_b1024_indep.gfp` | independent stereo, smaller block |
| `const_silence.gfp` | CONSTANT subframes (leading/trailing silence) |
| `verbatim_burst.gfp` | VERBATIM subframes (incompressible burst) |
| `escape_partition.gfp` | residual escape records (order 0b1111) |
| `lpc_order14.gfp` | full LPC — decoder-correctness only, not in target profile |
| `8bit.gfp` | 8-bit streams (experimental path) |

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
