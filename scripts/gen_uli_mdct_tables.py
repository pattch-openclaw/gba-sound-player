#!/usr/bin/env python3
"""Regenerate `crates/uli-enc/src/mdct_tables.rs`.

The encoder's forward MDCT needs the same six tables the C reference builds in
`mdct_init()` (`gba-audio/encoder/enc_core.c:141-151`) — but as **checked-in
constants**, not as libm calls at encode time. That is E-R5's rule R5c: "transform
constants come from checked-in tables, not from libm at encode time", and the
precedent is `uli-lite/src/tables_data.rs`. It matters twice over here: a codec
whose constants depend on which libm answered cannot honour E-R5's any-host half,
and the reference's own values are these exact expressions in `double`.

Python's `math.cos`/`math.sin`/`math.sqrt` call the same libm the reference links
against, and `repr(float)` prints the shortest string that round-trips — which
Rust's correctly-rounded `f64` parser reads back to the identical bits. So these
are the reference's constants, not approximations of them. `mdct.rs`'s test
`the_tables_are_the_reference_own` proves it against a C dump.

Only half of `win` is stored: `WIN[i] = sin(pi*(i+0.5)/2M)` is symmetric about
`i = M`, `WIN[2M-1-i] == WIN[i]`, so `mdct.rs` mirrors the index. Everything else
is stored at full length and in the reference's own order.

Usage:  scripts/gen_uli_mdct_tables.py [--check]
"""

import math
import sys
from pathlib import Path

M = 512
N = 2 * M  # the analysis window length, and the DCT-IV's 256-point FFT is H = M/2

OUT = Path(__file__).resolve().parent.parent / "crates/uli-enc/src/mdct_tables.rs"


def emit(name: str, doc: str, values: list, ty: str = "f64") -> str:
    body = ", ".join(repr(v) if ty == "f64" else str(v) for v in values)
    return f"/// {doc}\npub const {name}: [{ty}; {len(values)}] = [{body}];\n"


def main() -> int:
    check = "--check" in sys.argv[1:]

    # mdct_init: the 256-point complex FFT's twiddles, only the first 128 are
    # ever indexed (k * step < 128 for every stage).
    fft_c = [math.cos(-2 * math.pi * k / 256) for k in range(128)]
    fft_s = [math.sin(-2 * math.pi * k / 256) for k in range(128)]

    # The DCT-IV pre/post twiddles.
    pre_c = [math.cos(-math.pi * (4 * k + 1) / (4.0 * M)) for k in range(256)]
    pre_s = [math.sin(-math.pi * (4 * k + 1) / (4.0 * M)) for k in range(256)]
    post_c = [math.cos(-math.pi * k / M) for k in range(256)]
    post_s = [math.sin(-math.pi * k / M) for k in range(256)]

    # 8-bit reversal, the one integer table.
    brev = [int(f"{k:08b}"[::-1], 2) for k in range(256)]

    # Orthonormal sine window, stored half (symmetric).
    win = [math.sin(math.pi * (i + 0.5) / N) for i in range(M)]

    sc = math.sqrt(2.0 / M)

    src = [
        "//! Checked-in constants for the encoder's forward MDCT (E-R5, rule R5c).",
        "//!",
        "//! Regenerate with `scripts/gen_uli_mdct_tables.py`; the generator owns this",
        "//! file and the comments below are its comments. These are the doubles the C",
        "//! reference computes in `mdct_init()` (`gba-audio/encoder/enc_core.c`), stored",
        "//! rather than called for, so the encode path touches no libm and the values do",
        "//! not depend on which math library answers. They are `f64` rather than the",
        "//! decoder's Q31 `i32` on purpose: the analysis side is host-side and the",
        "//! reference is `double` — matching its arithmetic is what keeps S7's",
        "//! byte-exactness reachable, and none of this ships in a ROM.",
        "",
    ]
    src.append(emit("FFT_C", "FFT twiddle real parts, `cos(-2*pi*k/256)`, `k < 128`.", fft_c))
    src.append(emit("FFT_S", "FFT twiddle imaginary parts, `sin(-2*pi*k/256)`, `k < 128`.", fft_s))
    src.append(emit("PRE_C", "DCT-IV pre-twiddle real parts, `cos(-pi*(4k+1)/(4M))`.", pre_c))
    src.append(emit("PRE_S", "DCT-IV pre-twiddle imaginary parts, `sin(-pi*(4k+1)/(4M))`.", pre_s))
    src.append(emit("POST_C", "DCT-IV post-twiddle real parts, `cos(-pi*k/M)`.", post_c))
    src.append(emit("POST_S", "DCT-IV post-twiddle imaginary parts, `sin(-pi*k/M)`.", post_s))
    src.append(emit("BREV", "8-bit index reversal, as `mdct_init` builds it.", brev, "u16"))
    src.append(emit("WIN", "Orthonormal sine window, the first half; `win[2M-1-i] == win[i]`.", win))
    src.append(f"/// The orthonormal DCT-IV output scale, `sqrt(2/M)`.\npub const SC: f64 = {sc!r};\n")
    src.append(
        "\n".join(
            [
                "/// The windowed sample at analysis index `i`, mirroring the stored half.",
                "#[inline]",
                "pub fn win(i: usize) -> f64 {",
                "    debug_assert!(i < 2 * super::M);",
                "    WIN[if i < WIN.len() { i } else { 2 * WIN.len() - 1 - i }]",
                "}",
                "",
            ]
        )
    )

    text = "\n".join(src)
    if check:
        current = OUT.read_text() if OUT.exists() else ""
        if current != text:
            print(f"{OUT} is stale; run scripts/gen_uli_mdct_tables.py", file=sys.stderr)
            return 1
        print(f"{OUT.name}: current")
        return 0
    OUT.write_text(text)
    doubles = len(fft_c) + len(fft_s) + len(pre_c) + len(pre_s) + len(post_c) + len(post_s) + len(win)
    print(f"wrote {OUT} ({doubles} doubles + {len(brev)} indices + SC)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
