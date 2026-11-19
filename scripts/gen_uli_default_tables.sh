#!/bin/sh
# Regenerate `crates/uli-enc/tests/fixtures/uli2_default_tables.bin`: the tables
# blob the **C reference** produces for `tables_default`, which is what
# `uli_enc::DefaultTables::build().to_blob()` must reproduce byte for byte.
#
# Why a checked-in blob and not a live call: E-R1 forbids any build, link or
# vendoring dependency on gba-audio. This is the same posture `uli-album-asset`
# already takes — the oracle runs off-tree, by hand, when a table changes, and
# only its bytes enter the repository. `uli-enc`'s own gate (`make uli-test`)
# reads the file and builds no C.
#
# What it witnesses: the default code *lengths*, which `uli-enc` computes from
# the geometric model rather than transcribing. Nothing else in the tree can —
# every committed `.uli` corpus carries **trained** tables (`tables_from_stats`),
# so the fixture diverges from these bytes at exactly the first length row.
# See PLAN-OF-RECORD.md → "S1 — what the review found", and E-R5.
#
# Usage:
#   scripts/gen_uli_default_tables.sh            # writes the fixture
#   GBA_AUDIO=/path/to/gba-audio scripts/gen_uli_default_tables.sh
#
# Requires $GBA_AUDIO/encoder/enc_core.c. Exits nonzero if the tree is absent.
set -eu

GBA_AUDIO="${GBA_AUDIO:-$(cd "$(dirname "$0")/../.." && pwd)/gba-audio}"
OUT_DIR="$(cd "$(dirname "$0")/.." && pwd)/crates/uli-enc/tests/fixtures"
OUT="$OUT_DIR/uli2_default_tables.bin"
ENC="$GBA_AUDIO/encoder"
U="$GBA_AUDIO/src/uli"

[ -f "$ENC/enc_core.c" ] || {
    echo "gen_uli_default_tables: no reference encoder at $ENC (set GBA_AUDIO=)" >&2
    exit 1
}

# The reference's own CFLAGS, copied from $ENC/Makefile. They are not incidental:
# `-march=native` is part of why the byte-exactness gate is host-qualified
# (PLAN-OF-RECORD.md → "The acceptance question"), and generating the golden with
# different float codegen than the reference builds with would quietly pin a blob
# the reference does not produce on that host.
CFLAGS="${CFLAGS:--O2 -march=native -std=gnu11 -Wall -Wno-unused-function}"
CC="${CC:-cc}"

PROBE="$(mktemp -d)"
trap 'rm -rf "$PROBE"' EXIT

cat > "$PROBE/dump_tables.c" <<'EOF'
/* Dump `tables_blob(tables_default)` — the reference's own words, no stubs: the
 * real uli_tables.c supplies uli_step, which enc_core.c links against. */
#include <stdio.h>
#include <stdlib.h>
#include "enc_core.h"

extern EncTables T;

int main(int argc, char **argv) {
    static unsigned char out[8192];
    tables_default(&T);
    size_t n = tables_blob(&T, out);
    if (argc > 1) {
        FILE *f = fopen(argv[1], "wb");
        if (!f) { perror(argv[1]); return 1; }
        if (fwrite(out, 1, n, f) != n) { fprintf(stderr, "short write\n"); return 1; }
        fclose(f);
    }
    fprintf(stderr, "tables_default -> tables_blob = %zu bytes, K = %d\n", n, T.K);
    return 0;
}
EOF

# -Wno-unused-function because enc_core.c is compiled whole for one call.
$CC $CFLAGS -I"$ENC" -o "$PROBE/dump_tables" \
    "$PROBE/dump_tables.c" "$ENC/enc_core.c" "$U/uli_tables.c" -lm

mkdir -p "$OUT_DIR"
"$PROBE/dump_tables" "$OUT"

# A blob that is not 4-aligned, or whose `k` byte lies, means the geometry moved
# under us; refuse to freeze it rather than pinning a nonsense fixture.
bytes=$(wc -c < "$OUT" | tr -d ' ')
[ "$bytes" -eq 5212 ] || {
    echo "gen_uli_default_tables: expected 5212 bytes, got $bytes — the tables" >&2
    echo "  geometry moved; fix to_blob/parse_into before re-freezing this" >&2
    exit 1
}

echo "wrote $OUT ($bytes bytes)"
if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$OUT" | sed 's| .*/uli2_default_tables.bin$|  uli2_default_tables.bin|'
else
    shasum -a 256 "$OUT"
fi
