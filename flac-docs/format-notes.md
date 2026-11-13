# FLAC format notes — measured byte-level facts

> **Scope: FLAC format facts, measured.** Every byte-level finding here is about the FLAC
> format and the `GAFP` container this track builds on it, as witnessed by
> `crates/flac-lite`. Nothing here constrains another codec.

Every claim here is about **FLAC bytes**, and every one carries its witness: an
RFC 9639 section, encoder output from libFLAC 1.5.0, or a committed test/script
that reproduces it. The project rule has no exceptions:

> Any claim in these docs about *format bytes* carries its witness — an RFC
> section, or a measurement with the command that produced it. If neither is
> cited, treat it as a hypothesis, not documentation.

This file replaces the ten "Completed: step …" implementation walk-throughs that
used to sit in `FLAC.md`. Those sections were mostly narrative (what landed, which
gate count moved, which test bug was found); what they actually established about
the format is collected below. Code comments that cite "FLAC.md → step 3a…3f"
resolve here.

Contents:

1. Frame header layout and its three corrections (measured 2026-09-07).
2. Facts established while implementing the frame layer, condensed.
3. Prior assumptions that did not survive measurement (the 28-row table).

## Frame header: measured byte layout — the 31-vs-32-bit correction (2026-09-07)

Step 2 is "parse the frame header", so before writing it the header was measured
rather than recalled: real streams encoded by **libFLAC 1.5.0** (`flac` 1.5.0 via
brew on this host) and parsed byte-by-byte with an independent Python parser
cross-checked against **RFC 9639 §9.1.2–9.1.3**. 979 frames across five streams.
The scaffold was wrong in two ways that both land exactly on step 2.

### The layout (what real encoders emit)

```text
byte 0        byte 1        byte 2        byte 3        byte 4..  last
FF            F8            B8            18            00   CE   46...
└──────────────── fixed fields: 32 bits ────────────┘   └─┬─┘  └┬┘
                                                            │     └ subframe 0
 sync 14 = 0x3FFE (fixed blocksize)                          └ CRC-8 (poly 0x07)
 reserved 1 = 0        blocking 1 = 0 (fixed)
 blocksize 4 = 0xB (2048)   rate 4 = 0x8 (32 kHz)
 channels 4 = 0x1 (L/R)     size 3 = 0x4 (16-bit)   reserved 1 = 0
```

Field order and widths, authoritative (RFC 9639 Table 15/16, confirmed on
libFLAC bytes):

| Field | Bits | Notes |
|---|---|---|
| `synccode` | 14 | `0b11111111111110` — **the same 14-bit value for both blocking strategies**. RFC 9639 §9.1 defines a *15*-bit sync `0b111111111111100` followed by the strategy bit, which is why a frame starts `0xFFF8`/`0xFFF9`. Not 8 bits, and **not** 16. (Corrected 2026-09-08 while implementing step 2: this row previously read "`…11` variable", implying a second 14-bit sync code. There isn't one — `0x3FFF` is not a sync, it is a sync plus the variable-blocksize bit, which is why `parse` reads 14 bits for sync and validates the strategy separately.) |
| reserved | 1 | must be 0 |
| blocking strategy | 1 | 0 = fixed blocksize (our profile) |
| blocksize code | 4 | full table below |
| sample-rate code | 4 | full table below |
| **channel assignment** | **4** | **4 bits, not 3** — see the correction |
| sample-size code | 3 | 0 = from stream, 1 = 8, 4 = 16, `0b011` reserved |
| reserved | 1 | must be 0 |
| — fixed fields total — | **32** | lands exactly on byte 4 |
| UTF-8 coded number | 8·1..10 | whole octets, always starts byte-aligned |
| uncommon blocksize | 8 \| 16 | only when blocksize code is `0b0110`/`0b0111` |
| uncommon sample rate | 8 \| 16 | only for rate codes `0b1100..0b1110`; **never 24-bit** (corrected 2026-09-08 — the scaffold comment and this row both said `8 \| 16 \| 24` with no witness; §9.1.7 defines 8-bit kHz, 16-bit Hz, 16-bit Hz÷10, and libFLAC 1.5.0 emits exactly those) |
| CRC-8 | 8 | **always byte-aligned** |

**Blocksize codes** (RFC 9639 Table 14, restated correctly 2026-09-08 — the
prose here had the `144·2ᵛ` range one code too high and wrongly called the top
two codes reserved; the independent oracle in `scripts/frame_vectors.py` was
right all along, which is what pinned it):
`0b0000` reserved · `0b0001` 192 · `0b0010..0b0101` = `144·2ᵛ` →
**2:576 3:1152 4:2304 5:4608** · `0b0110` 8-bit uncommon (value + 1) ·
`0b0111` 16-bit uncommon (value + 1) · `0b1000..0b1111` = `2ᵛ` →
**8:256 9:512 10:1024 11:2048 12:4096 13:8192 14:16384 15:32768**.

**Sample-rate codes**: `0b0000` **from stream** · `0b0001` 88.2k ·
`0b0010` 176400 · `0b0011` 192000 · `0b0100` 8k · `0b0101` 16k · `0b0110` 22050 ·
`0b0111` 24k · `0b1000` **32k** · `0b1001` 44100 · `0b1010` 48k · `0b1011` 96k ·
`0b1100` = kHz as 8-bit · `0b1101` = Hz as 16-bit · `0b1110` = Hz÷10 as 16-bit ·
`0b1111` forbidden.
**There is no code for 65,536 Hz.** It can only travel as `0b0000` + stream
default — see the profile section below. (Rates that *do* have uncommon-code
representations — measured 56000 as `0b1100`, 48001 as `0b1101`, 10010 as
`0b1110` — encode **without** `--lax`, unlike 65,536, which has no code at all.
Both paths now carry golden vectors.) **32,768 Hz is in the same family**: no 4-bit
code (`0b1000` is 32,000, and there is no other table entry near it), so it travels
either as `0b1101` + the exact 16-bit Hz — what `flac` 1.5.0 emits when simply asked
for 32,768, at **2 bytes per frame** — or as `0b0000` + a 32,768 stream default, at
**zero** size cost. The second spelling was measured end to end on the product path:
a re-wrap of the shipped 157 frames that way is the same 865,996 bytes, passes
`flac -t`, and decodes to the shipped PCM bit for bit — `measurements.md`, the 6b-3
rate row, and Q14. The probe that measured both spellings is committed
(`examples/flac_spike/experiments/rate-respell-32768/`), and so is the trap: `flac -t`
**exits 0 while printing one warning per frame** when the blocksize fields are written
the wrong way, so a re-wrap check that trusts the exit status passes a stream the
decoder is misreading.

**Channel assignment**: `0b0000` mono · `0b0001` L/R · `0b0010..0b0111` 3–8
channels · `0b1000` left/side · `0b1001` side/right · `0b1010` mid/side ·
`0b1011..0b1111` reserved.

### Correction 1 — channel assignment is 4 bits; fixed fields are 32, not 31

The retired 2026-09-06 `byte_align`/`read_u8` entry (then in `FLAC.md`) and
`bits.rs`'s header-tail unit test both recorded the fixed fields as
`sync 14 + reserved + blocking + blocksize 4 + rate 4 + channels 3 + size 3 +
reserved` = **31 bits**, and drew a conclusion from it: that the header CRC-8
"always sits at bit `31 + 8k`, **never** byte-aligned", so `byte_align()` before
the CRC would drop the CRC's top bit.

Both the width and the conclusion are wrong.

* **Why it matters:** reading 3 bits for channels desynchronises the cursor by
  one bit for everything after it. The header still "parses" — sync matches,
  blocksize decodes to something plausible — and the damage surfaces later: the
  coded number, the CRC byte, and then the subframe type field are all read from
  the wrong offset. Debugging that means staring at subframe code, not header
  code.
* **Why 4 is forced, format-wise:** the field must encode mono, independent
  stereo, three decorrelation modes, 3–8 channel layouts, *and* reserved values.
  Three bits cannot cover mono + stereo + 3 decorrelation modes without stealing
  the 3–8-channel codes; the spec's own table (Table 16) is 4 bits.
* **Measured:** the CRC-8 lands at bit 40 (1-octet coded number) or bit 48
  (2-octet) — `% 8 == 0` on every one of the 979 frames examined. With 32 fixed
  bits, everything before the CRC is whole octets, so **byte-alignment isn't just
  observed, it's structurally guaranteed**: 32 + 8·(coded octets) + 8·(optional
  uncommon fields, which are 8/16/24 bits).
* **What survives:** `read_u8` staying bit-level. Its real justification is the
  **frame footer** — the CRC-16 read that follows subframe data and its padding
  to a byte boundary, which genuinely can end unaligned — plus verbatim /
  raw-signature samples. The header was never the reason.
* **What to delete from memory:** the idea that `byte_align()` before the header
  CRC is *dangerous*. It is a no-op there. Writing it "defensively" would be
  harmless at runtime but would encode a false claim about FLAC, which is how
  this error propagated in the first place.

### Correction 2 — there is no swapped mid/side (`side_bit` was a phantom)

`format::ChannelConfig::MidSide { side_bit }` was documented as "the swapped-pair
flag (assignment 0b101 vs 0b110)". FLAC has no swapped mid/side variant. Codes
`0b0101` and `0b0110` are **6-channel and 7-channel** layouts. The three
decorrelation codes are distinct and unambiguous, so nothing in the format could
ever set that flag. Removed: a phantom field invites a phantom branch, and
"unknown/never-taken" branches are exactly what a `strict-profile` audit trips over.

### Correction 3 — `-l 4` does not mean "FIXED order 0–4"

The profile said `-l 4` caps predictors at FIXED order 0–4. `flac --help` says:
`-l, --max-lpc-order=#   Max LPC order; 0 => only fixed predictors`. So **`-l N`
is a maximum *LPC* order**, and only `-l 0` is FIXED-only. Measured on the same
10 s source (census over every frame of each encode):

| Encode | Subframe 0 across the whole stream | Decorrelation chosen |
|---|---|---|
| `-l 0` | `fixed0` ×157 | mid-side ×157 |
| `-l 4` | `lpc4` ×156, `lpc3` ×1 | mid-side ×157 |

So a `-l 4` clip is **mostly full-LPC frames**. Three things follow, and they
cascade into the roadmap:

1. **Step 3 (the perf gate's decode math) needs more than FIXED.** If the spike
   embeds a `-l 4` clip, the decoder must decode LPC or it will reject nearly
   every frame. Either the spike embeds a **`-l 0`** clip for the FIXED arm
   *and* a `-l ≤4` clip for the LPC arm (this is now the plan — the gate compares
   FIXED vs LPC cost, which is the actual decision), or step 3 must grow LPC.
2. **The `strict-profile` reject rule changes shape.** "Reject LPC" is not the
   same switch as "reject predictor order > 4". The gate's decision rule
   ("FIXED fits, LPC doesn't → make `-l 4` a hard constraint") needs restating:
   the enforceable constraint is **max predictor order ≤ N plus an explicit
   fixed/LPC flag**, not a FIXED-only assumption.
3. **The profile wording is fixed** (see below): "max predictor order ≤ N,
   `-l 0` for FIXED-only", never "FIXED order 0–4".

### Correction 4 — `-l 0` is FIXED-only, not FIXED-0 (2026-11-11)

Correction 3 fixed the `-l` reading, but "`-l 0` for FIXED-only" was then read by
the packer plan as "FIXED-0". It is not: `-l 0` disables LPC, and libFLAC still
chooses FIXED orders 1–4 from the signal. Measured on real material in
[`playback-results.md`](playback-results.md): the `-l 0` arm emitted
`constant0 x10` plus `fixed0 x3` and `fixed1/2`. The packer's product guard is
therefore **FIXED-0-only** in practice; `scripts/gafp_pack.py`'s
`MEASURED_DECODE` has no FIXED-1..4 rows, and CONSTANT/VERBATIM are also
unmeasured there. `-l 0` is an LPC switch, not a fixed-order guarantee.

### The final frame is legitimately short

A track's last frame carries a **smaller blocksize** than STREAMINFO's maximum:
320,000 samples at blocksize 2048 is 156 full frames plus one 512-sample frame
(visible in the `stereo-last` golden vector: blocksize code `0b1001` = 512 where
the track uses 2048). Anything that filters candidate frames by "blocksize must
equal the track's blocksize" therefore **silently drops the last frame of every
track**. This is not speculation — a first cut of the vector harness did exactly
that, an assertion fired, and the "bad parse" was the harness being over-strict
about a legal stream. Corollary for the packer: frame count is
`ceil(total_samples / blocksize)`, and the manifest must carry the last frame's
sample count (or the decoder reads it per-frame from the header, which is what
`FrameHeader::blocksize` is for).

### Frame sync scanning: what actually filters

The plan lists "locate frames by sync scan" as the spike's frame finder. Measured
over real encodes (`0xFF 0xF8`-shape candidates vs. the true frame count):

| Stream | real frames | sync-shape candidates | + CRC-8 only |
|---|---|---|---|
| `l4_stereo` | 157 | 214 | 157 |
| `l0_stereo` | 157 | 200 | 157 |
| `l4_mono` | 313 | 330 | 313 |
| `l4_silence` | 32 | 32 | 32 |
| `r65k` | 320 | 447 | 320 |
| **total** | **979** | **1223** | **979** |

A second, different source (sine + hash-noise material) was worse: **1476**
candidates for 157 real frames (~9.4×), because noise-like payload bytes happen
to look like sync + reserved-clear far more often. Two conclusions:

* **Sync shape alone is not a frame finder.** 1.1× on clean synthetic tones,
  up to ~10× on noisy material. It's a *recovery* mechanism (`Error::FrameSync`),
  not an index.
* **CRC-8 alone is close but not exact**: the `r65k` and second-source streams
  admit 1–2 false positives each (a payload byte sequence that both looks like a
  header and happens to CRC). What *was* exact, on all 979 frames of both
  sources: **strict RFC field validation + CRC-8**, checked as "frame numbers are
  exactly `0..N-1` in stream order". That is the filter the harness uses.

**Decision for the spike (step 4):** locate frames with a **hand-computed offset
table** generated offline (option (a) from the earlier review) — zero new decoder
code, and it matches the container's real design, where the manifest's offset
table *is* the seek mechanism. (`crc8` has since landed behind `crc-check`, but the
frame finder still does not rely on it.) Do not ship sync-scan-without-a-filter.

### What the encoder actually chooses (cite this instead of guessing)

* `-l 0` → FIXED only; `-l N` → LPC up to order N (`-l 4` produced `lpc4`).
* `-m` (try mid/side per frame) chose **mid/side on every frame** of a
  mid/side-shaped source; on a source with uncorrelated channels it chose
  independent L/R for every frame. Left/side and side/right **do** appear,
  just rarely from `-m`-default sources — corrected 2026-09-16 by
  construction: a near-identical-channels source (R a smooth triangle, L = R
  + tiny quantized noise) makes libFLAC 1.5.0 choose **side/right on 63/63
  frames**, and an identical-quantized pair produced left/side on 63/63; the
  anti-phase source that looks like the obvious side/right bait picks
  **mid/side** instead (the encoder's per-frame cost model wins the argument,
  not intuition). The golden-vector spec still treats them as **optional**
  vectors — the *committed* vector source does not produce them — and
  `--no-mid-side` / `-B` remain the levers if forced coverage is needed. The
  corrected claim matters because "never" was load-bearing for skipping
  orientation witnesses; see step 3f part 1.
* 65,536 Hz requires **`--lax`** (outside FLAC's streamable subset); every frame
  then carries sample-rate code `0b0000`, i.e. the `FromStreamDefault` path.
* Digitally silent input → **CONSTANT** subframes, and `flac` still emits a
  normal header + Rice-coded residual structure around them.

### `--force-utf8-legacy-noop` is not a real flag

Both `crates/flac-lite/README.md` and `scripts/pack_flac.sh` carry a reference
encode command ending in `--force-utf8-legacy-noop`. libFLAC 1.5.0 rejects it:
`flac: unrecognized option` (the only UTF-8-related flag is `--no-utf8-convert`,
which is about tag charsets, not frame numbers). Both places now use the plain
command. Frame numbers are UTF-8-coded regardless — that is a spec property, not
something to be configured — which is exactly why `read_utf8_coded` exists and why
the 6-byte/7-byte header pair is in the vector set.


---

## Facts established while implementing the frame layer

Condensed from the retired step 3a–3f records. Each row names the witness that
pins it; none is a recall. Tests live in `crates/flac-lite/tests/`, generators in
`scripts/`.

### Subframe header and body order

| Fact | Witness |
|---|---|
| The subframe type is one pad bit + a **6-bit** code (Table 19). Order lives *inside* that code: FIXED = v−8, LPC = v−31. **Nothing follows the type field.** | `tests/subframe_header_layout.rs`; 941 real subframe-0 headers over seven encode profiles cross-checked against `flac --analyze` |
| The wasted-bits flag comes **before** the warm-up samples, and must: warm-up width is `subframe bps × order`, and `subframe bps = frame bps − wasted`. | same suite; the library reader is called from `parse`'s exit cursor and must agree with the independent oracle on every vector |
| The LPC precision field is **not** in the type field. §9.2.6 Table 22: `u(4)` = precision−1 (`0b1111` forbidden) sits in the **body after the warm-up**, then `s(5)` shift, then the coefficients. | `tests/subframe_body_vectors.txt` + `lpc-256` mutations |
| The `s(5)` shift is rejected when negative **before** the coefficient reads; `shift ≥ 64` is refused rather than trusted. | `integrate_lpc` refusal tests |
| Predictor coefficients are stored **most-recent-past first**; the warm-up prefix is stored oldest-first in the stream. | stage-3d oracle vectors; `decode_subframe` composition |

### Wasted bits

| Fact | Witness |
|---|---|
| `read_wasted_bits` is the §9.2.2 flag + unary run, **cursor-atomic** (an unterminated run restores the flag bit and returns `EndOfStream`); the slice bounds it, so there is no length cap to get wrong. | `bits` unit tests: flag-clear, k = 1/3/5/8, over-long k = 40, mid-slice restore, alignment sweep vs naive oracle |
| The `<< wasted` multiply runs **once, at block exit, after prediction** — not at the warm-up read. Prediction runs on the wasted-stripped codec scale. | measured on the LPC × wasted crossing: pad-first is wrong on 252/256 samples, pad-after on 0/256. Committed vectors `fixed1-wasted5-256` (presence) and `lpc-wasted-256` (order) |
| Padding before prediction is **not** equivalent: the LPC dot product's arithmetic `>> shift` floors, and flooring does not commute with left-shifting. A FIXED cascade (pure addition) cannot discriminate the two orders. | same crossing vectors |

### Warm-up / cross-frame state

| Fact | Witness |
|---|---|
| Warm-up samples live **inside each subframe's own header**, unencoded; "each subframe in FLAC is coded completely independently" (§9.2.5). Frame 0 has warm-up and no previous frame. | `scripts/measure_warmup_semantics.py`: over real `-l 0` and `-l 4` encodes, 64/64 frames carry their **own** first decoded samples, 0/64 match the previous frame's tail |
| Consequence: frames are independently decodable, so a manifest offset is a safe seek by construction. `PredictorState` is per-subframe scratch, never history. | same measurement; `tests/frame_run_loop.rs` (reverse-order decode) |
| The warm-up doubles as the block's first `order` samples: FIXED/LPC write the prefix from state (reversed to stream order) and decode `out[order..]`. | stage-3e composition + `flac -d` PCM diff |

### Residual (partitioned Rice)

| Fact | Witness |
|---|---|
| `rice_unmap` is **even → `n >> 1`, odd → `!(n >> 1)`** (the encoder folds `x ≥ 0 → 2x`, `x < 0 → −2x − 1`). The scaffold's "odd → (n+1)/2, even → −n/2" was sign-flipped. | RFC §9.2.7.2 worked example: folded 38 unfolds to **+19**; pinned as a unit assertion |
| The map stays total (`0xFFFF_FFFF` → `i32::MIN`); **rejecting** that value is `decode_rice_partition`'s job (§9.2.7.3), not the map's. | stage-3a/3b seam tests |
| `order == 0` skips the remainder read entirely — it must branch, because `read_bits(0)` is `InvalidField` by design. | stage-3b unit tests |
| Overflow guard: `checked_shl` is **not** sufficient — it only rejects shift *amounts* ≥ 32 and wraps bits out of the value (quotient 4 at order 30 yielded `folded = 0`). Guard on the quotient (`quotient > u32::MAX >> order`) with `order >= 32` short-circuited first. | the rejection test that failed with `Ok(())` |
| An escape partition's raw width field is **5 bits** (§9.2.7.1), so widths reach 31; `raw_bits == 0` means all-zero samples stored with no bits. | stage-3b tests; the "4-bit field" scaffold note was wrong |
| A coded residual has **no sample-size header** and no `0b111` "unknown" escape. §9.2.7 is method `u(2)`, partition order `u(4)`, per-partition parameter (`u(4)`/`u(5)`, escape adds `u(5)`). Residual width is implicit in the codewords. | RFC §9.2.7 read in full + Appendix D Table 38 + libFLAC `read_residual_partitioned_rice_` (reads nothing else). A planned 8-phantom-bit read would have desynced every frame |
| Partition sample counts: partition 0 carries `(blocksize >> order) − predictor_order`, the rest `blocksize >> order`; total `blocksize − order`. Divisible blocksize, `blocksize >> order > order`, no reserved methods. | stage-3c tests; RFC Appendix D.2.7's 15 residual values (Table 39) reproduced bit-exactly |

### Stereo and the frame footer

| Fact | Witness |
|---|---|
| The side subframe is coded at **bps + 1**, not bps − 1 (§4.2: "the side channel needs one extra bit of bit depth"). A read at bps desyncs every decorrelated frame. | anti-phase encode: side warm-up at bps+1 equals `flac -d`'s L−R on 24/24 frames, 12 of them values a 16-bit field cannot hold; at bps it disagrees 24/24. libFLAC `read_subframe_` does `bps++` on the side slot |
| For `0b1001` (right/side) subframe **0 is the side**; for `0b1000` (left/side) and `0b1010` (mid/side) it is subframe 1 — the slot libFLAC increments (slot 0 for right/side, slot 1 for left/mid-side). | Table 16 "stored as side-right"; §4.2 decode wording; `undo_channel_coding` `output[0][i] += output[1][i]`; near-identical-channels construction emits `0b1001` on 63/63 frames |
| Mid/side recovery is exact with no `+1`: `m_ext = (mid << 1) \| (side & 1)`, then `(m_ext ± side) >> 1`. The `& 1` **is** the rounding term. | generation-witnessed oracle (`stereo::decorrelate`); 55 pairs incl. the side-saturating `(32767, −32768)` |
| A frame's tail is **zero-padding (0–7 bits) plus the 16-bit CRC**, i.e. a 16..23-bit gap. Frame lengths are not a stride (`117 116 116 116 116 116 115 116 116 51`). | `tests/scan_frame_walk_layout.rs` over 35 real frames; 23 of them carry non-zero padding |
| A constant-16-bit footer walk does **not** silently slip (0 strict-header parses at all 23 slipped positions — 1 of the 23 is sync-shaped but field-refusing — and 0 strict headers at the 30 down-rounded byte positions); re-anchoring *up* to the next byte boundary lands correctly by arithmetic. Both are safe only because the body's bit end was computed by walking the fields. | same suite's `footer_slip_positions_*` / `footer_byte_truncation_*` rows |
| CRC-16 init is **0x0000**. The scaffold's 0xFFFF was corrected; the reflected "FLAC variant" reading is excluded outright. | CRC-16 measured across 10 real frames: sole init consistent with all of them; three-way mechanism agreement; derived check 0xFEE8 |
| Header CRC-8 coverage is sync through the last header field, **excluding** the CRC byte itself; the header CRC is always byte-aligned (the footer is the reason `read_u8` must stay bit-level). | `tests/crc_check.rs`; `tests/frame_header_layout.rs` |
| There is **no metadata-block CRC**. A metadata block is a 4-byte header plus exactly `length` bytes (§8.1); the format's only CRCs are header and frame footer. | `stream` module + RFC §8.1; no probe reproduced anything else |

### Header width, coded numbers, and stream layer

| Fact | Witness |
|---|---|
| Real frame headers are **6 to 9 bytes** wide. `parse` accepts 48..=120 bits (`MIN_HEADER_BITS` / `MAX_ACCEPTED_HEADER_BITS`); the widest legal *spelling* is 128 bits, and a 7-octet coded number cannot occur in a fixed-blocksize stream (§9.1.5 caps the frame number at 31 bits / 6 bytes). | `tests/fixtures/s3_wide_48001hz_b3000.flac` — a valid stream whose first header is **80 bits**; 48/80/120-bit headers witnessed end to end |
| Non-minimal coded-number forms are refused. This is a **deliberate divergence**: `flac -d` accepts a real stream whose frame number is spelled longer once both CRCs are recomputed; libFLAC itself never writes one. Keeping the rule preserves the value→form-width relation the width bounds are derived from. | `tests/stream_vectors.txt` `mutation number-form-overlong` rows + the generator's per-fixture census |
| The stream layer is: `fLaC` magic, §8.1 metadata-block chain (borrowed, streaming, reserved types skipped by length, forbidden type refused), §8.2 streaminfo, then `frame_data_offset`. | `tests/stream_metadata_layout.rs` over six real fixtures: two-block, seektable+padding, real multi-point seek table, 80-bit header, real 24-bit stream (refused), unseekable encode with no total/frame sizes/MD5 |
| An `EncodeProfile` is enforcible only as **max predictor order plus a fixed/LPC flag** — never as "reject LPC", and never as a subframe-kind census. | Correction 3 below; the rate flow's real-material census (silence encodes CONSTANT even under `-l 0`) |

## Prior assumptions that did not survive measurement

Kept deliberately, since each one looked like a reasonable note and cost real
time. The pattern is worth naming: **every one of these was a claim about a
format recorded from recall rather than from bytes**, and every one was in a file
that read like documentation, which is what made it survive review.

| Prior assumption | Reality (measured) | How it was caught |
|---|---|---|
| Frame header fixed fields = 31 bits, `channels 3` | 32 bits, `channels 4` | Python parser over 979 real frames; assertion #3 above |
| Header CRC-8 "never byte-aligned"; `byte_align` there would corrupt it | Always byte-aligned; `byte_align` is a no-op | CRC bit position `% 8` over all frames |
| `read_u8` must be bit-level *because of the header CRC* | True, because of the **footer** (post-padding CRC-16) | Layout walk; footer case checked separately |
| `MidSide { side_bit }` = "swapped pair, 0b101 vs 0b110" | No such variant; `0b0101`/`0b0110` are 6/7-channel | RFC Table 16 vs the scaffold's own comment |
| `-l 4` → "predictors capped at FIXED order 0–4" | `-l` = max **LPC** order; `-l 4` emitted `lpc4`; FIXED-only needs `-l 0` | `flac --help` + per-frame census of both encodes |
| A `-l 4` clip is a reasonable FIXED-path test asset | It is mostly LPC frames — the spike would reject ~all of it | Census of subframe types across a full encode |
| Filter candidate frames by the track's blocksize | The **final frame is legitimately short** → drops the last frame of every track | Harness assertion fired on a legal stream |
| Sync-scan needs no filter ("nearly free") | 1.1×–9.4× over-match; CRC-8 alone still admits 1–2 false positives | `measure` over two different source types |
| 65,536 Hz is reachable with a rate code | No 4-bit code exists; needs `--lax` + code `0b0000` on every frame | `flac` refused without `--lax`; all 320 frames code 0 |
| `bits_per_sample: u8` in the header struct | Code `0b000` would silently yield a bogus number → needs `StreamDefaults` | Signature review against the measured rate-code-0 case |
| `--force-utf8-legacy-noop` in reference encode commands | Not a libFLAC flag — 1.5.0 exits 1 with `unrecognized option` | Ran the command |
| Hand-packed vectors + prose notes are sufficient evidence of layout | They record the author's belief, and two reviewers agreed with a wrong one | This whole pass; vectors are now encoder-derived |
| The 14-bit sync is `0x3FFE` for fixed blocksize, `0x3FFF` for variable | §9.1's sync is **15** bits (`0b111111111111100`) plus the strategy bit; `0x3FFE` covers **both** strategies | Writing `parse` against §9.1 — a 14-bit `0x3FFF` check would reject variable-blocksize streams at the sync instead of at the strategy bit |
| Uncommon sample rate is stored as 8 \| 16 \| **24** bits | §9.1.7: 8-bit kHz, 16-bit Hz, 16-bit Hz÷10 — **never 24-bit** | §9.1.7 + libFLAC 1.5.0 encodes of 56000/48001/10010 Hz; a 24-bit read strands the cursor 8 bits short of the CRC-8 |
| Blocksize codes `0b1110..0b1111` are "reserved"; `144·2ᵛ` starts at `0b0101` | Table 14: `0b0010..0b0101` are the `144·2ᵛ` family, and `0b1110`/`0b1111` are **16384/32768** | Table 14 while writing the blocksize table test; the Python oracle had it right, the prose did not |
| Real headers are 6 or 7 bytes (asserted over the golden set) | **6 to 9** bytes: the uncommon blocksize/rate octets lengthen the header | Regenerating vectors with the tail/rate streams — the old assertion failed immediately, which is what it was for |
| A hand-written STREAMINFO may record the short tail in its min/max blocksize fields | libFLAC records the **full-block** extremes — 2048/2048 for a 2048-block stream whose last block is 512 — and `total_samples` is what carries the short tail. Writing 512/2048 instead makes the reference decoder treat the stream as variable-blocksize and print `sample or frame number does not increase correctly` for every frame — **as a warning: `flac -t` still exits 0**, so the status is not the test | `flac -t` over a re-wrap of the shipped 157 frames, both ways: 157 warnings vs `ok`. Also: `flac -d` writes a 44-byte RIFF header whatever the output file is named, so hashing the file rather than its `data` chunk turns a bit-exact decode into a false mismatch |
| LPC `precision_bits` rides in the subframe-type field (`0b00`→15-bit, `0b01`→16-bit) | The type field carries **no** precision. §9.2.6 Table 22: `u(4)` = precision−1 (0b1111 forbidden) sits in the **body**, after the warm-up samples, then `s(5)` shift, then coefficients | Reading §9.2.6 to implement `SubframeType::parse`; confirmed by walking the field at the real cursor on `-l 4` encodes |
| `SubframeType::parse` reads the type field "plus the order bits that follow" | Nothing follows. Order lives *inside* the 6-bit code: FIXED = v−8, LPC = v−31 (Table 19) | Table 19 while writing `parse` — reading trailing order bits would strand the cursor inside the wasted-bits run |
| Subframe layout `[type][warm-up][wasted bits][residual]` | `[type][wasted bits][warm-up][residual]` — wasted precedes warm-up, and must: warm-up width is `subframe bps × order`, and subframe bps = frame bps − wasted | §9.2.5/§9.2.6 width formulas while placing `parse`'s exit cursor |
| `EndOfStream` is reachable mid-field (a 1-byte slice is too short) | Degenerate: the field is 7 bits and reads are byte-granular, so any non-empty slice holds it; a 1-byte `0xFF` is a pad-bit rejection, not EOF | Writing the EOF test — the reader disagreed with the assertion, and was right |
| Warm-up samples are "the previous frame's tail samples", the only cross-frame state the decoder retains | Warm-up lives **inside each subframe's own header**, unencoded; §9.2.5: "each subframe in FLAC is coded completely independently". Frame 0 has warm-up and no previous frame. Consequence: frames are independently decodable, `seek_frame` is safe by construction, `PredictorState` is scratch, not history | `scripts/measure_warmup_semantics.py`: over real `-l 0` and `-l 4` encodes, every frame's warm-up equals its OWN first decoded samples (64/64) and none equal the previous frame's tail (0/64). Found while implementing step 3a, before any code depended on the myth |
| `rice_unmap` scaffold doc: "odd → (n+1)/2, even → −n/2" (`negative = n & 1`) | Sign-flipped. §9.2.7.2 folds `x ≥ 0 → 2x`, `x < 0 → −2x − 1`, so the decode is **even → `n>>1`, odd → `!(n>>1)`** | The RFC's own worked example (folded 38 → +19; the scaffold mapping returns −19), pinned as a unit assertion |
| Escape partition `raw_bits` is a "4-bit field" (`PartitionHeader` doc) | §9.2.7.1: **5 bits** follow the escape code (so widths reach 31, and the 4-bit note would truncate a legal escape partition) | Reading §9.2.7.1 to implement 3b's escape branch |
| `quotient.checked_shl(order)` is a sufficient folded-value overflow guard | `checked_shl` only rejects shift *amounts* ≥ 32 and **wraps** value bits shifted out: quotient 4 at order 30 yielded `folded = 0` | The 3b rejection test asserted `InvalidField` and got `Ok(())` — the impl disagreed with the assertion, and the assertion's author was wrong about `checked_shl`, not the code |
| A coded residual carries a **sample-size header** with a `0b111` "unknown" escape, derived by scanning partitions (scaffold module doc + the original 3c plan line) | **No such field exists.** §9.2.7 is complete with method `u(2)` + partition order `u(4)` + per-partition parameter (`u(4)`/`u(5)`, escape adds raw width `u(5)`); residual width is implicit in the codewords, nothing is derived | Implementing 3c against §9.2.7 read in full + Appendix D Table 38 (real residual: header, parameter, straight into quotients) + libFLAC's `read_residual_partitioned_rice_` (reads nothing else). Caught before any code depended on it — the planned 8 phantom-bit read would have desynced every frame |
| A hand-packed byte can witness a residual-header rejection (`0b0000_0001` = "partition order 1") | MSB-first that byte is method `00` + order `0b0000` = 0 — **legal** for the test's blocksize; the test would have passed on an unrelated EOF | The 3c probe test disagreed with its own bytes; fixed structurally by routing every header probe through the `Packer` oracle so named fields are encoded fields |
| The side subframe is coded at frame **bps − 1** (3f plan line, inherited by `decode_subframe`'s "caller's `-1`" seam note: "mid/side & friends code the side at bps−1") | The side is coded at **bps + 1** — the opposite direction. §4.2: "The side channel needs one extra bit of bit depth, as the subtraction can produce sample values twice as large"; libFLAC 1.5.0 `read_subframe_` does `bps++` on the side slot (channel 0 for right-side, channel 1 for left/mid-side). A `−1` would desync every decorrelated frame and reject legal warm-ups. | Anti-phase encode (R = −L, \|L−R\| beyond 16-bit): side warm-up read at bps+1 == `flac -d`'s L−R on 24/24 frames, 12 of them values the 16-bit field **cannot hold**; the same read at bps disagrees on 24/24. `drafts/flac3f_width_probe.py` (throwaway), libFLAC 1.5.0 |
| Scaffold `stereo.rs`: "right/side: `right` = primary" (subframe 0 holds R) — flatly contradicting `format::ChannelConfig::RightSide`'s doc ("subframe 0 is the side"); two crate docs disagreed and neither was trusted | Subframe 0 is the **side**. Table 16: "`0b1001` — 2 channels: left, right; **stored as side-right** stereo"; §4.2's decode wording ("the left subblock is restored by adding the samples in the side subframe to the … right subframe"); libFLAC `undo_channel_coding` RIGHT_SIDE arm: `output[0][i] += output[1][i]`. | The rarity excuse lived in this file's own "encoder choices" bullet ("may never appear"), which is why no witness had ever existed — and it was false: the near_smooth construction (R smooth triangle, L = R + tiny quantized noise) emits **0b1001 on 63/63 frames**, and subframe 0's warm-up (read at bps+1) equals L−R from `flac -d` on 12/12 fingerprinted frames. `drafts/flac3f_width_probe.py` |

**Rule going forward:** any claim in these docs about *format bytes* carries its
witness — an RFC section, or a measurement with the command that produced it. If
neither is cited, treat it as a hypothesis, not documentation.

