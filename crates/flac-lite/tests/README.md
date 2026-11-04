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
| `stream_vectors.txt` | per-file: the §8.1 block chain, every §8.2 `STREAMINFO` field, `frame_data_offset`, header-width census, frame-number **form** censuses, scan-frame-start counts, one number-form mutation + `flac -d` exit codes | `scripts/gen_stream_vectors.sh` | `metaflac --list` + two independent Python readings (§8 structure, §9.1 with/without the minimal-form rule) |
| `fixtures/s1…s5_*.flac` | the five streams the table describes, byte-for-byte what `flac` 1.5.0 wrote | same script (`stream_vectors.py emit`) | `flac` 1.5.0 (the encode commands live in the script next to each `encode_command` row) |

Each fixture's *reason for existing* is in the table's `note` rows and in the
module docs of the suite that reads it — `tests/stream_metadata_layout.rs`
explains `s1…s5` (two-block container, every real block type, a real 80-bit
header, a valid 24-bit stream, a stream that declares nothing about its own
length).

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
