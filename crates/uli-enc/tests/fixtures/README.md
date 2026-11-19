# `uli2_default_tables.bin` — the reference's default code tables

5,212 bytes: `tables_blob(tables_default)` as the sibling `gba-audio` C encoder
produces them. It is the **byte-exactness witness** for
[`src/tables_default.rs`](../../src/tables_default.rs)'s `to_blob()` — the only
thing in this repository that says the Huffman lengths this crate *computes* are
the lengths the reference *has*.

The layout is the format's, not ours: `k`, then `ctype`/`rbits`/`esck` (14 bytes
each), then 15 context rows of 15 bytes, then 14 `len_q` rows of 81, then 14
`len_p` rows of 256, then 14 `dq` rows of 16, zero-padded to 4. Both
`CodeTables::parse_into` and `DefaultTables::to_blob` derive those offsets from
`k`, so this file is what stops the two from agreeing with each other and nobody
else.

## Why the unused rows are zeros, and why that is pinned

Each class carries exactly **one** coefficient row. `parse_into` selects it by
`ctype` — `len_q` for QUAD, `len_p` for PAIR — and never reads the other, and the
reference leaves that row at the value `memset` gave it (`tables_from_stats`
rewrites only the row matching `ctype`). Verified all-zero in every committed
corpus.

S1 shipped with the writer putting each class's lengths into *both* rows: 11 PAIR
classes × 81 bytes plus 2 QUAD classes × 81 = **1,053 of 5,212 bytes wrong**, on a
branch where every test passed. A round-trip cannot see those bytes by
construction — the same `DefaultTables` object is on both sides, and the decoder
skips them. So the invariant is asserted twice: once against our blob, once
against this file, in
[`../tables_golden.rs`](../tables_golden.rs). See
[`PLAN-OF-RECORD.md`](../../../../uli-lite/PLAN-OF-RECORD.md) → "S1 — what the
review found".

## Regenerate

```sh
scripts/gen_uli_default_tables.sh          # GBA_AUDIO= overrides the location
# or
make uli-default-tables
```

It compiles `enc_core.c` off-tree with the reference's own `CFLAGS` (`-O2
-march=native`) and links the real `uli_tables.c`, so `uli_step` is not stubbed.
Nothing in this repository *builds* that C: no gate depends on this target, and
E-R1 forbids the dependency, not the oracle — the same posture as
`uli-album-asset`.

Expected content, so a regeneration that changes it is read before it is accepted:

```
sha256  15b477edd45acc10bbf572ec1f27e8d81d8c74647845609ac330f345a096bcb0
size    5212 bytes, k = 14
```

**Do not regenerate to make a test pass.** The blob is the reference's output; the
encoder is the thing on trial. If they disagree, one of the two is wrong, and
regenerating picks whichever answer is more convenient.

## What this file cannot witness

Nothing about the *trained* tables. Every committed `.uli` corpus carries tables
from `tables_from_stats`, so they differ from these bytes starting at the first
length row (byte 43) — which is also why the corpora were never a witness for the
defaults, however much S1's comments claimed they were. The corpora do agree with
`ctype`/`rbits`/`esck` (bytes 0..43), and
[`../tables_golden.rs`](../tables_golden.rs) now asserts exactly that and no more.
