# ULI2 plan of record

[`../../ULI2.md`](../../ULI2.md) is the decision record (why ULI2, what it is,
the licensing and encoder risks). **This file owns the phases, gates, the
measurement ledger and the order of work.**

## Current state (decode 2026-11-19; encoder 2026-11-27)

mGBA, WAITCNT 4/2, 262,144-cycle period, no UI. Both corpora pinned
(`xover=0 hfnz=0`), two runs per image identical to the cycle.

| | worst | mean | of period |
|---|---:|---:|---:|
| 129-frame fixture decode | 193,317 (f98) | 172,109 | 73.75% / 65.66% |
| album decode wall (16,553 frames) | **194,695** (f4,998) | **169,354** | **74.27% / 64.60%** |
| gate total (decode + convert + IRQ + DMA) | **226,152** | **200,624** | **86.27% / 76.53%** |

- **U2, U3 and U4 are met.** U4's ceiling stays the period: **13.73 points of
  margin at the worst frame, 0 starved periods over 16,552**.
- **1.070x the C + ASM reference on the decode wall** (C: decode 69.43%, gate
  73.63%). Of the 12.6-point gate gap, 4.84 points are decode and **7.80 are the
  convert/IRQ/DMA half at ~2.9x C's cost** — the output path, not the codec, is
  now the largest remaining difference.
- **All five hot kernels are hand-written asm**, each twinned bit-exactly on the
  ARM7TDMI (`make uli-kernel-twin`): `pre` 1.00x C, `fft` 1.05–1.09x, `post`
  ~1.03x, `ola` 1.00x, M/S 0.64x (mean).
- **Entropy/dequant is ~35,656 mean, at or below C's 42–48k.** The product
  reader is **closed-world** — no bounds test, no error flag, 16-bit contract —
  with `crates/uli-lite-gba/tests/premise.rs` asserting the premise and
  `uli_lite` remaining the checked oracle.
- **Placement:** `.iwram` 26,496 B of 32,768 (code + `HotState` 8,192 +
  `ClassTables` 560 + `q3`/`clz6` 148); stack high-water 948 B (fixture) / 1,988 B
  (gate); code tables in `.ewram`.
- **Open:** the non-decode gate half; the dense-frame tail of entropy (the
  sub-frame partition needs re-measuring on the product reader first — see "The
  entropy path against the C+ASM reference"); the ULI2 provenance/publish decision (root MIT
  exists; `NOTICE.md` marks the three crates unresolved). These are
  **W4**; the asset path (**W2**) and the app switch (**W3**) are indexed in "Remaining
  ULI2 work" below.
- **The encoder: S8 is done — the acceptance matrix MATCHes on every cell (2026-11-26), and
  the pin ledger was discharged on 2026-11-27.** `make uli-accept` compares our `--rd`
  encoder against the off-tree reference on identical PCM and flags. The S7 DIFF's cause is
  **attributed at first order**: the plan differs
  (S8-3 finds it at frame 0 of `biomaster`), so the counts differ, so the tables are
  downstream; at a frozen plan the coder reproduces the reference's bits on 24 of 24 codings.
  **Landed in S8:** S8-1 (the two-sided frozen probe), **S8-2 (P2: `make
  uli-dump-plan`, the reference's own pass-1/pass-2 plan and the per-class counts
  `tables_from_stats` consumed; its gate is the dumped shipped plan against the plan decoded
  from that run's own blob — 257 of 257 frames, both header cycle fields and every seek entry
  on the `fast` cell, and 129 of 129 on the generated `tiny` fixture)**, S8-3 (the plan/table
  diff), **S8-0 (the flag surface: `--stride`/`--fine` and the refine stage deleted,
  ties-to-even, 6 retries)**, **S8-4 (the `fast` cell: `make uli-accept
  ULI_ACCEPT_ONLY=fast`, 2.4 s against our 12.2 s)** and **S8-4b (the oracle of record:
  rebuilt `-ffp-contract=off -march=armv8-a`, `sha256 1991ee6c…`, zero fused multiply-adds,
  with `scripts/uli_probe.py` moved to the same flags)**, and **S8-5 (the coder: `D` derived
  from the incurred `J`, the terminator charged, the `allz` guard in place, every term added
  in the reference's order)**. The frozen differential is now **identical on 24 of 24
  codings** — bits, stream, cycles, counters and distortion — where it began with 76 ULP of
  `D`, one divergent stream and one divergent cycle count. Row 18 came with it
  (`fixtures/band_rdo_clean.txt`, bitwise on all 2,400 cases). **S8-6 (the trainer: `make
  uli-train-diff`; the 5,212-byte table region matches the reference on both the `fast` cell
  and the generated `tiny` fixture, from the dumped stats and from a re-coding of the pass-1
  plan every second frame; rows 19–20 closed, 21 named as a diagnostic)** followed, and then **S8-7 (the
  search) landed on the generated cells (2026-11-26): the encoder now emits the reference's
  bytes — `fast` 19,744 B `sha256 2f21f6d7…`, `tiny` 39,308 B `c398c2d2…`, at both one and
  four threads.** The corpus is generated in-tree (`make uli-album-asset`, `make uli-adversarial-asset`);
  with it, **every cell the matrix runs — `fast`, `tiny`, `biomaster`, `off-default` and
  `adv` — is byte-identical to the oracle**, including `biomaster` after row 25's fix. `cross60`
  is not run while `adv` is present. **S8 is done (2026-11-27): the pin ledger is discharged
  below and §S7 now carries the MATCH re-take; no code difference remains.** The SNR licence
  test S7 left open moves to S9, so S8 kept exactly one target. The four remaining work
  threads are indexed in "Remaining ULI2 work" immediately below; see "S7" for the measured
  table.
- **The encoder: S6 (the decoder-cycle price) is done, and it was the whole of the high-rate
  gap — but only where the cap binds hard.** An over-cap candidate is re-coded under
  `mu · (band symbol + decoded work)` instead of being refused, and the final encode re-codes
  at the price that bought the cap. On 20 s of `bioluminescent` at `--budget 400000 --cap 74`
  the switch is worth **1.44 dB** (39.57 → 41.01) against the reference's 1.48 dB, i.e. parity
  within 0.01 dB at a cap of 74.0% max. At the product album's rate — 199 B/frame, against
  that slice's 2,498 bits/frame — the cap binds on **3.5%** of a `cross` slice's frames and the
  switch is worth **0.02 dB** to the reference, so S5's **0.41 dB `cross` residual is not
  cap-constrained** and S6 leaves it alone. It does not touch the two lower budgets' 0.16 dB
  either (byte-identical at 40,000). See "S6" below.
- **The encoder: S5 (album assembly, the WAV layer, the parallel curves) is done.** It
  encodes a 43.5-minute album (167,222 frames, 12 tracks) inside a passed-in budget with a
  pinned hash; a 32,768 Hz stereo WAV reaches its PCM pair's exact blob and a resampled one
  is held to SNR; the search runs frame-parallel, so `biomaster` (16,553 frames) went from
  **12m16s to 3m46s** at four threads **with byte-identical output**. See "S5" below.
- **The encoder: S4 (trained code tables) is done.** Two passes: the album's own symbol
  counts and per-bucket reconstruction errors rebuild the Huffman lengths and `dq`, then the
  search ships the plan it would actually code. At binding budgets the gap to the reference
  closed to **0.16 dB** (31.19 vs 31.35 at `budget 120000`); the residual 1.15 dB at the top
  budget was cap-constrained frames, not the codebooks — that was S6, now done. Trained
  context codebooks differ per row, which is why the band-class choice became a shortest
  path first.
- **The encoder: S2 (the codec's first real frame) is done.** The forward MDCT
  reproduces the reference's coefficients bit for bit on a frozen fixture, and a
  fixed step now trades rate against distortion the way the format intends:
  ~18 dB at a near-white test album's finest step, and on the real `biomaster`
  album 53 dB at 363 kbit/s (g=60), 38 dB at 108 kbit/s (g=100), with no
  coefficient overflow and none past `NCODED`. S3 is where rate stops being an
  accident of the step and becomes something the encoder chooses.
- **The encoder: S1 (the skeleton) is done.** `crates/uli-enc` takes the
  reference's own PCM pair and writes a ULA2 blob that `uli-lite` decodes, its
  default tables blob is byte-identical to the reference's, and its refusal paths
  (length, budget, step index) fire before a byte is written. S2 onward — MDCT,
  quantisation, the RD search — is where audio appears; the plan, its requirements
  and its steps are under "The encoder — the last missing piece".

## Remaining ULI2 work — the four threads (2026-11-27)

S8 closes the encoder question; what remains is no longer one plan but four independent
threads. This table is the index. Each thread has one owner file; a thread is finished only
when the gate in the last column is measured and its result is recorded there.

| thread | what it covers | owner | state | exit gate |
|---|---|---|---|---|
| **W1 — S8 bookkeeping** | the S8 pin ledger, the S7 MATCH re-take, and the stale status text in `ULI2.md` and `crates/uli-enc/README.md` | this file §S8; `ULI2.md` | **done 2026-11-27** | `make uli-accept` MATCH on every cell; frozen fixtures unchanged; §S7 carries MATCH; no live text still calls S8 open |
| **W2 — asset path onto the in-repo encoder** | point `uli-album-asset`, `uli-adversarial-asset`, `uli-cross-asset` and `uli-two-track-asset` at `crates/uli-enc`; keep the C tools only as the `uli-accept` oracle; emit the pin from the bit-exact Rust `uli-lite` decoder instead of the C `uli_dec`; the recursive `$(MAKE) -C $(GBA_AUDIO)/encoder` build is already pinned clean (2026-11-27: `ULI_ORACLE_CFLAGS` plus `check_uli_oracle.py --expect clean`), so only the switch remains; decide the multi-track `--dacw 0`/`--dacw 1` objective and make a fresh-clone bake work without `GBA_AUDIO`'s **encoder** | this file; `Makefile`; `ULI2.md` → "The measurement corpus" | **next** | the four asset targets build and run the Rust encoder; no asset target needs `$GBA_AUDIO/build/uli_enc`; the generated blobs still decode and pin; the multi-track objective is recorded; a product bake no longer needs either C tool |
| **W3 — the product app switches to ULI2** | `backend-uli2` + `src/backend/uli2.rs`, the derived 32,768 Hz / 512-sample geometry, the ULA2→`ALBM1` manifest seam, the app-side UI cost, and the album-length hardware run | `ALBUM-PLAYER.md` §4.2b, §11 Phase 5, AP-9/R7/Q3 | **open (app-side; can run in parallel with W2)** | `make album-rom BACKEND=uli2` plays an album from a fresh checkout; the three-arm compile and runtime parity pass; the UI fits the measured margin; the hardware run matches the FLAC arm's witness |
| **W4 — codec/player residuals and S9** | the non-decode gate half (7.80 points at ~2.9x C), the dense-frame entropy tail, U5's 1 Hz redraw defect, S9's `--dacw 1` arm and the SNR licence test, and the ULI2 provenance/publish decision (root MIT exists; `NOTICE.md` marks the three crates unresolved) | this file "Current state"; `ULI2.md` §Gates/§Risks; §S9 | **open; only U5's redraw defect can block W3** | each item is measured against its own gate; U5's redraw defect is closed and the UI margin re-taken; S9's arms are admitted only by the SNR licence test |

**Order and meaning for a v1 switch.** W1 is done; W2 is the next codec-track task because it
removes the last build-time encoder dependency on `gba-audio` and makes the asset path
reproducible from a fresh clone. W3 is the actual FLAC→ULI2 product switch and is application
work; it can start once W2's asset shape is agreed, even before every target is switched. W4
is deliberately last: the non-decode half and the dense entropy tail are performance work
with a measured margin, S9 is an improvement track that is only meaningful once exactness is
frozen, and licensing is a publish question. **ULI2 v1 is "finalized" when W1+W2+W3 are
closed and W4's U5 redraw defect and UI-margin measurement are closed; S9 and licensing are
explicitly outside v1.**

**Everything below this line is the ledger that produced those numbers, in
chronological order, and it is kept deliberately: this project's method is that
a claim is worthless without its measured arm and a falsification is worth more
than a projection.** Several intermediate figures are therefore superseded by
the table above — where a passage was contradicted outright it is marked
*Superseded* in place. Read the current state from this block; read the reasoning
from the ledger. Two exceptions worth knowing: the kernel table's `fft` row and
the `pre`/`ola`/M/S ratios were re-derived with bit-exact arms (2026-11-18), and
`post` was the only instruction-named deficit and is closed.

---

**PR C-2 (as C-3a, with C-3c) landed 2026-11-14** — 523,267 → **488,685 worst /
429,339 mean (200% → 186%)**, a measured 35,000 where 90,000 was projected.
**PR C-3b landed the same day**: the operation counters and format-rule
diagnostics moved behind a `verify` feature, and the product build measures
**457,863 worst / 410,912 mean (175% / 157%)** — a further **30,822 / 18,427**,
which is the attribution's prediction met (it predicted 30,482 / ~20,000). The
ladder is now 2526% → 1133% → 698% → 204% → 186% → **175%**. Every twin and pin
is green in both builds; the ledger carries the arms. Next: coefficient buffers
to IWRAM with C-1. Parity (69.4%) still needs the symbol-path codegen gap the
attribution localized.

**PR C-1 landed (2026-11-14): hand-written FFT, three measured arms, and the
product frame moved.** A bit-exact hand-written ARM FFT was taken from
"same speed as Rust" to **15.8% faster than the Rust ARM kernel** in three
measured steps, and then wired into the product decoder:

- **Ladder** (cycles/FFT call, same image, all bit-exact): Rust ARM 85,065 →
  v1 spill 88,205 (*slower*) → **v2 traffic 73,363** → **v3 block transfers
  71,635**. v2 removed the spill/reload traffic (no t-value spilled, loop vars
  resident); v3 added `ldmia`/`stmia` block transfers — the structural
  difference the C reference had. The register-pressure hypothesis C-1 rested
  on is falsified as the *lever*; **memory traffic** is the lever, and it is
  now 18.8% down from v1.
- **Product frame**: `GbaDecoder`'s synthesis now routes through `fft_kernel`,
  so with `asm-fft` the full product frame drops **457,863 → 430,997 worst
  (175% → 164% of the period)** and **410,912 → 384,046 mean (157% → 147%)** —
  −5.9% / −6.5%, a real product-path gain, pins unchanged. Before this wiring
  the asm gains only reached `ArmKernels`, not the product decoder.

**PR C-1 follow-up landed (2026-11-15): the trivial butterfly (v4).** The last
structural difference from C's `TRIV_BFLY_P` is now taken: at j==0 of each
stage the twiddles are exactly (2^30,0)×3, so `cmul(y,1.0) == y>>2` and the
three twiddled outputs are plain sums shifted right 2 — no multiplies and no
twiddle loads. `fft_arm_v4.s` measures **69,034 cycles/call (−2,601, −3.6% vs
v3)**, and the product frame **425,795 / 378,845 (162% / 145%)**, a further
−5,202 worst. Ladder: 85,065 (Rust) → 88,205 (v1) → 73,363 (v2) → 71,635 (v3)
→ **69,034 (v4)**, 2.20× the C record.

**PR C-1 follow-up 3 (2026-11-15): the twiddle table's *location* was the real
twiddle-load cost.** The planned `ldr`->`ldmia` was worth ~1-2%; the table was
sitting in **cartridge ROM** (`.rodata` at `0x0800d640`) while C keeps its tables
in IWRAM, so every hot butterfly paid ROM wait states. A same-image A/B (same
kernel, only the table pointer differs) measures **69,034 → 61,852 cycles/call,
−10.4%**, and the new `iwram-tw` feature takes the product frame **425,795 →
411,431 worst (162% → 157%)**, mean 378,845 → 364,481 (145% → 139%). The FFT is
now **1.97×** the C record — under 2× for the first time. `iwram-tw` costs
2,016 B of `.iwram` (14,860 → 16,876) and is **opt-in** pending the U6 budget
(see the ledger). Lesson recorded: a large part of "why are we slower than C"
was table placement, not kernel codegen.

**Still slower than C+asm, and where to look next** (see the ledger's "PR C-1"
sections for the full analysis): v4 is ~1.97× the C record's ~31,400, and the
remaining known differences are (1) ~~the trivial-butterfly path~~ **done**,
(2) the remaining ~2× is the multiply core and the A..D/output traffic, and
(3) the C record is from a different corpus/image so the ratio is not
like-for-like — the honest sub-goal is to pull the FFT under 31,400 *in this
instrument*. There is no reason this Rust+asm stack should lose to C+asm; the
next research items are in that order in the ledger.

**Step 1 of the critical path landed (2026-11-15): the entropy/dequant rewrite,
and the decode wall is under the frame period for the first time.** Two pieces,
in the order the plan required:

- **The sub-frame instrument** — five timing-only arms
  (`attrib-entropy-{reader,nolut,nodequant,nowrite}` plus an `attrib-count`
  `huff_slow` hit counter) partition the entropy path at the gate frame instead
  of guessing at it. `make uli-attrib-subframe`. It found that the dequant
  arithmetic is **13%** and the bit-reader mechanics **5.5%** of the path: the
  cost is symbol resolution and the per-coefficient work around it.
- **The rewrite** — reader by value in and out of the channel decode, a refill
  contract that guarantees 17 valid bits (so `skip` becomes branchless), a
  long-code search that reads the window instead of `peek`ing up to sixteen
  times, the sign straight off the window's top bit, and a smaller per-band live
  set. The entropy path fell **111,227 → 86,826 at the gate frame** (−22%, the
  projection was ~12% for the period alone) and the product frame went
  **274,198 → 249,835 worst (105% → 95.3% of the period)** and **232,090 →
  220,757 mean (89% → 84.2%)**, pins unchanged in both twin builds.

**Step 1e (2026-11-15): budget work — `.iwram` down 1,352 B and the frame at
83.6%, with one hypothesis falsified.** Three arms, each measured on its own:

- **The Rust control kernels left `.iwram`** — `pre`, `ola_s8` and `ola_s16` are
  the in-image *controls* now that asm kernels exist, so their placement is gated
  on the asm feature being off, exactly as `fft` is gated on `asm-fft`. **−496 B,
  and the frame is byte-identical** (nothing on the gate path calls them).
- **Register relief in `dec_chan` was tried and rejected.** Folding `bad` into a
  `valid` sentinel (−1,672 B of code, `dec_chan` 1,610 → 1,192 instructions) cost
  **+1,450 cycles mean**; adding the intended relief (deriving `esck`/`d15` in the
  rare branches instead of holding them) made it **+3,351**. The reason is
  recorded: indexing `tables` inside those branches **resurrects `tables` as a
  live value across the whole loop**, which is exactly the pressure the change
  was meant to relieve. The register-pressure hypothesis for this kernel is
  falsified — the second such falsification after the FFT's v1.
- **The cursor rewrite landed.** Three slot loops and the band loop compared
  pointers instead of carrying counters, and the verify-only `hfnz` rule became a
  pointer compare against `x + NCODED` instead of an index — which is what let
  the counters go. **−856 B and −665 cycles mean / −2,081 worst.**
  But the mean is the honest number here (the worst frame moved from f93 to
  f125, so worst-vs-worst is not like-for-like), and 665 is a third of what
  `pre`/`ola` got from the same shape: `dec_chan`'s stores were already
  index-scaled by LLVM, so there was less to remove.

Net: **`.iwram` 28,632 → 27,280 B (−1,352), product headroom 828 → 2,196 B**, and
the frame **221,332 → 219,251 worst (84.4% → 83.6%) / 195,154 → 194,489 mean
(74.4% → 74.2%)**, **1.22x → 1.21x** the C wall.

**Step 1d (2026-11-15): the overlap-add, `ola` at C parity, and the <=85% headroom
target met.** `ola` was 34,375 at the gate frame = 17,187/channel = 1.12x C,
and the disassembly put our loop at **33 instructions per iteration against the
reference's 26**: the clamp was 8 instructions per output where the reference
needs 6 (it saturates to int16 with a three-instruction idiom, then shifts right
8 — and `sat8(x >> 8) == sat16(x) >> 8` for every i32), and the addressing was 7
instructions of index arithmetic where the reference keeps six post-indexed
cursors and reads the overlap word with `ldr [b_cursor, Y_minus_P]`. Rewritten in
that shape (`asm/ola_arm.s`, 160 B per format, **26 instructions**):
**34,375 → 30,539 at the gate frame** (−3,836, exactly the frame delta) =
**15,270/channel = 1.00x C**, the first kernel at parity. The product frame is
**225,168 → 221,332 worst (85.9% → 84.4%)** and **198,992 → 195,154 mean (75.9%
→ 74.4%)**, **1.24x → 1.22x** the C wall, pins unchanged and the asm twin green
(`make uli-kernel-twin`, now covering `ola` too). **The <=85% headroom target was
met at that point, with 1,490 cycles of margin** (2,940 after Step 1e), and the
remaining gap to C was 62% entropy / 37% synthesis.

**Step 1c (2026-11-15): the `pre` pre-twiddle, and the frame was at 85.9%.** The
instrument came first and **falsified the projection**: `pre` was not
address-rebuilt and not spilling — LLVM's loop was 15 instructions per output
pair with zero stack traffic — but the reference's is **10**, because it uses
four post-indexed cursors and one `stmia` per pair instead of a computed index
plus split pre/post store pairs. Rewritten in that shape (`asm/pre_arm.s`, 76 B,
10 instructions, no address arithmetic): **26,458 → 23,889 at the gate frame**
(−2,569, exactly the frame delta), **11,945/channel, 1.36x → 1.23x** C. The
product frame is **227,736 → 225,168 worst (86.9% → 85.9%)** and **201,554 →
198,992 mean (76.9% → 75.9%)**, **1.25x → 1.24x** the C wall, pins unchanged and
the asm twin green (`make uli-kernel-twin`, now covering `pre` too). The <=85%
headroom target needs **~2,350**.

**Step 1b (2026-11-15): FFT v5, and the frame was at 86.9%.** The hand-written
FFT took the reference's address shape — an absolute byte cursor with
register-offset addressing, a register-held twiddle pointer, and the destructive
sum formation that needs no moves — for **81,035 → 71,519 at the gate frame**
(−9,516, exactly the frame delta) and **35,760/channel = 1.14x the C record**,
the closest any kernel has come. The product frame is **237,252 → 227,736 worst
(90.5% → 86.9% of the period)** and **211,071 → 201,554 mean (80.5% → 76.9%)**,
**1.30x → 1.25x** the C decode wall, pins unchanged and the asm twin now a real
gate (`make uli-fft-twin` — it had been compiled out of `make uli-bench` since
PR C-1, so that line read `fftasm=true` vacuously). The <=85% headroom target
now needs **~4,900** cycles.

**Step 1a of the critical path landed the same day: the bit window is in
registers.** `dec_chan` destructures the reader into five `let mut` scalars and
`refill`/`take`/`top`/`esc`/`coef` are **macros over them** — the reference's
`REFILL`/`SKIP` shape, and the only Rust shape that did not put the 24-byte
struct back in an alloca (three ABI/promotion attempts had failed). Another
**−12,583 / −9,686**: the product frame is **237,252 worst / 211,071 mean =
90.5% / 80.5%**, **1.30x** the C record, **−36,946** off the worst frame across
the two steps. That flipped the order of work: the deficit is now 42% entropy /
57% synthesis, and the FFT (35% of the frame, 1.29x C) is the only block large
enough to close the ~14,400 the <=85% target still needs.

**90.5% is the decode wall, not the gate total.** Convert + IRQ + DMA and this
tree's UI are not in it, so U4 is still open and playback still does not start;
the headroom target is the <=~85% the critical path names.

**The next lever is measured, not guessed: the bit reader is still in memory.**
`dec_chan`'s disassembly (1,491 ARM instructions, 5,976 B of `.iwram`) addresses
the 24-byte `BitReader` through one base register, so every `refill` and `take`
stores `next`/`hw`/`valid`/`bad` back — and the per-band hoists (`rq`, `step`,
`dq15`) are spilled alongside it. Two ABI experiments and one promotion attempt
did not move it (see "Step 1" below); the reference gets its window into
registers because its `REFILL`/`SKIP` are **macros over locals**, not function
calls through a pointer. That is the next arm. Also: with the entropy path down
23,000, **synthesis is again the larger share of the frame (65%)** and the gap
to C is now split ~58/42 between entropy and synthesis, not 69/31.

The target is **real time inside the hardware's cycle budget** (U4/U5), not a
percentage borrowed from C: the C/asm decoder's 69.4% is a proof that the work
fits this machine, so Rust+asm is expected to as well. See "The target is the
real-time budget" below. The measured ladder is **2526% → … → 175% → 148% → 138%
→ 105% → 99% → 95.3%**. The fast path is the default; **C-3d (2026-11-15)**
removed the int16 output the product never used; **C-3 (2026-11-15)** moved
`HotState` to IWRAM for **−87,040 cycles/frame**; and **Step 1 (2026-11-15)**
rewrote the entropy path for **−24,363**, which is the change that took the
decode wall under the period. **Step 0 of the critical path attributed the frame
by difference** — on the worst frame, entropy/dequant/M-S and synthesis each own
about 40% of the distance to C now. See "Critical path to real time" below;
every item there is a projection until it lands as its own measured arm.

The Phase 1b measurement that forced this rewrite is recorded in
[`../../ULI2.md`](../../ULI2.md) → “Phase 1b — what the on-target measurement
found”. The instrument is [`../../examples/uli_bench`](../../examples/uli_bench)
(`make uli-bench`).

---

## This file, and the record it summarises

**Two files, one topic each.**

| file | what it is | how to read it |
|---|---|---|
| **this file** | current state, architecture, memory layout, the measurement *method*, the closed-world premise, and the reference tables | the parts that stay true between measurements |
| [`RECORD.md`](RECORD.md) | the chronological record: the critical-path steps, every PR, every measured arm, the landing log and the risks as they were understood at the time | read it to learn *why* something is the way it is, or to find the arm behind a number |
| [`../../ULI2.md`](../../ULI2.md) | the decision record: why ULI2, its gates, its risks, its decisions | the front door |
| [`../uli-lite-gba/README.md`](../uli-lite-gba/README.md) | the product path’s architecture and gates | using the codec |

The split is deliberate: this file was 4,600 lines of interleaved reference and chronology, so every reader had to know which half they were in. Nothing was deleted — the record keeps its arms, its falsifications and its superseded drafts, because a claim here is worthless without the measurement behind it.

## Architecture

```
crates/uli-lite/            reference + oracle, not shipped
  album.rs, tables.rs       cold ULA2/table parsing (reused by the product)
  bitstream.rs, frame.rs    safe codec + the Kernels seam
  imdct.rs                  portable synthesis (ARM-mode on ARM targets)
  PLAN-OF-RECORD.md         this file

crates/uli-lite-gba/        the product decoder
  src/lib.rs                ARM-mode, unchecked synthesis kernels in .iwram
  src/decoder.rs (PR B)     GbaDecoder + HotState: the 32-bit bit reader,
                            unchecked Huffman, dequant and M/S hot path
  tests/                    host twins against the core and the corpus pins

examples/uli_bench/         the measurement vehicle
  full frames through both kernel sets, plus per-kernel decomposition
```

The seam is `uli_lite::Kernels` (landed in PR A). `Decoder<K>` defaults to
`PortableKernels`, so every existing caller and test is unchanged; the product
instantiates `Decoder<uli_lite_gba::ArmKernels>`. This lets the bench measure a
**full frame through the product kernels** instead of projecting it from
per-kernel sums.

The product may call the core for cold work (album and table parsing). The hot
decode path is expected to move into `uli-lite-gba` in PR B; the core remains
the control and the host-testable oracle.

---

## Memory layout — mirrored from the C reference (2026-11-15)

Placement is **not a guess and not trial-and-error**. It is read from the C
reference's own linker script (`gba-audio/tools/gba/gba.ld`) and working-set
declarations (`src/uli/gba/uli_gba_int.h`, `uli_gba_dec.iwram.c`). Analysis
only — no C source is incorporated.

### The two projects have opposite defaults

This is the root of the earlier ambiguity:

- **C**: plain `static`/global data goes to **IWRAM** — `.data` and `.bss` are
  both placed in IWRAM. Only things explicitly marked `.ewram`/`.ewram_bss`, or
  compiled into a `*.ewram.o`, go to EWRAM. The linker `ASSERT`s that `≥1 KiB`
  of IWRAM is left for the user stack, so **C's decoder uses essentially no
  stack**: every working buffer is a static in IWRAM.
- **ours (agb)**: `.bss`/`.data` go to **EWRAM**; IWRAM receives only what is
  explicitly `#[link_section = ".iwram"]`. A plain `static` here is the opposite
  of the same declaration in C.

So "which section does this buffer belong in" is answered by C's source, not by
our defaults.

### C's IWRAM working set (measured from the source)

| C object | bytes | meaning |
|---|---:|---|
| `uli_g_X[2][512]` i32 | 4,096 | per-channel coefficient buffer |
| `uli_g_buf[512]` i32 | 2,048 | FFT scratch |
| `uli_g_P[2][256]` i32 | 2,048 | overlap (OLA) state |
| `uli_g_preP[512]` i32 | 2,048 | pre-twiddle table (IWRAM copy) |
| `uli_g_postP[512]` i32 | 2,048 | post-twiddle table |
| `uli_g_twP[504]` i32 | 2,016 | FFT twiddles |
| `uli_g_winP[512]` i32 | 2,048 | window table |
| `uli_g_perm[256]` u16 | 512 | post permutation |
| `uli_g_q3[81]` u8 | 81 | quad-class lookup |
| `UliGState uli_g` | ~600 | bit reader + per-frame state |
| **sum** | **~19,545** | |
| + ARM code (their doc: ~5 KB) | ~5,000 | kernels + IRQ + conv |
| + user stack (assert floor) | ≥1,024 | C uses almost none |
| **total** | **~25.5 KB of 32 KB** | |

The ~100 KB Huffman `UliCodeTables` is the only large object, and it is in
**EWRAM** (`UliGState.T` is documented `/* EWRAM */`). The audio ring buffer is
also EWRAM.

### Ours today vs the C-mirrored target

| buffer | C | ours now | target |
|---|---|---|---|
| X/buf/P (`HotState`) | IWRAM 8,192 | **EWRAM** 8,192 | IWRAM (C-3) |
| tables (perm/pre/post/tw/win) | IWRAM 8,672 | **ROM** (feature copies) | IWRAM |
| synthesis kernels | IWRAM | IWRAM | IWRAM |
| Huffman code tables | EWRAM | EWRAM | EWRAM |
| decoder state | IWRAM ~600 | EWRAM | IWRAM (small) |

### The stack figure in this file was stale — corrected by measurement

Every `~15.0 KiB stack` number in this file (PR B onwards) came from a period
when the bench's twin scratch buffers were **stack locals**. They were moved to
EWRAM statics during PR C-1, and the figure was never re-measured. It is now
measured directly: `examples/uli_bench` fills the free IWRAM stack region below
`sp` with a sentinel before the product pass and scans it afterwards
(`stack prod high-water`).

**Measured: 768 B of descent below `run()`'s frame** — not 15,040 B. Repeatable.

**Refined 2026-11-15 (C-3): 768 B is not the whole requirement.** The probe fills
below the *current* `sp`, so it measures only the descent from `run()`. The real
budget runs from the BIOS stack top (`0x03007F00`; agb never sets `sp`), and the
instrument's total high-water is **≈2,960 B** = 2,192 B above `run()` (entry
wrapper + `agb::Gba`) + 768 B of descent. Write the budget as
`usable = 0x03007F00 - __iwram_end`, and require `usable >= ~2,960 B` for this
instrument. (This is the same lesson one level down: the first figure measured a
*relative* quantity and was read as an absolute one.)

### Consequence: the C layout fits, and two "blocked" items are unblocked

With the real stack cost the arithmetic reverses. Measured in the bench
(`--features asm-fft,iwram-tables`, i.e. C's table layout):

| arm | worst | mean | of period |
|---|---:|---:|---:|
| product, TW only in IWRAM (`asm-fft,iwram-tw`) | 411,431 | 364,481 | 157% |
| **product, all tables in IWRAM (`asm-fft,iwram-tables`)** | **387,879** | **340,929** | **148%** |

`.iwram` = 25,548 B, leaving 6,964 B, minus the 768 B stack → **6,196 B
headroom**. `iwram-tables` was recorded as *crashing* on the stale figure; it
now runs with all twins and pins green. Mean and worst fall together, and the
`ovl` full-frame arm falls to 1,570,540.

This is the placement lesson again, one level up: **the binding constraint was
never the stack, it was a stale measurement.** The next placement step (C-3,
`HotState` → IWRAM, 8,192 B) needs the superseded v1/v2 kernels dropped (1,520 B
frees it) — arithmetic in the C-3 row below.

## Measurement method

Every number in this file follows the Phase 1b rules, so the arms stay
comparable:

- **Correctness before speed.** The measured image proves its own decode
  against the C-reference pins (`e6e87652` / `b3530978`) before any timing
  line is emitted; the timed pass is hashed too.
- `WAITCNT = 0x0000` (4/2, no prefetch) for the recorded arm; the boot value
  (`0x4317`, 3/1 + prefetch) is printed alongside.
- Free-running timer2→timer3 at 16.78 MHz, IRQs off inside every window,
  empty-window overhead measured and subtracted.
- Sums and counts are reported; means are derived host-side. No divide on the
  measured path (ARM7TDMI has none).
- A wrapped window is `u32::MAX` and fails the verdict; it is never truncated.
- **mGBA, not hardware.** mGBA does not model the ROM prefetch buffer, so
  cartridge-resident code is charged pessimistically; this is stated per
  record. U4 keeps a hardware run as its witness.
- Any arm that changes arithmetic must have an on-target twin against the
  reference, not just a host twin.
- **Iteration corpus vs verification corpus (decided 2026-11-14).** The
  129-frame `uli2_tiny` corpus is the iteration corpus: it is what every arm is
  measured on while the decoder is still multiple × over budget, because
  full-album runs would slow the loop without changing which differential is
  next. It is **not** a worst-case witness — gba-audio's hardest frame was
  frame 27,806 of 167,222. A comprehensive worst-frame analysis (FLAC-style:
  min/max with frame index and arm, paired deltas, a second mechanism rather
  than a second transcription — see [`../../flac-docs/measurements.md`](../../flac-docs/measurements.md))
  is **deferred** to PR E, and no real-time claim may rest on the 129-frame
  number alone.

---

### Caveats that must travel with these numbers

- **Per-kernel rows are ±3%** (they disagree by up to ~10% between two
  re-measurements a day apart). The single-kernel deltas sum to 148,347 at f98
  against the measured sequence 147,074 (+0.9%). After 1e the *mean* is the
  comparator of record: the worst frame moved from f93 to f125, so worst-vs-worst
  across images is not like-for-like. The interaction is the FFT's
  data-dependent trivial-butterfly path: removing `pre` feeds it a stale `buf`.
  Quote the sequence row for synthesis.
- **`attrib-entropy` alone is not quotable.** Removing `dec_chan` freezes the bit
  position, so all 129 frames synthesise one stale frame. The entropy row comes
  from `synth` minus `synth-entropy`.
- **`attrib-entropy-nowrite` is not quotable at all** — see the table.
- **The sub-frame arms are substitutions, not pure removals, wherever they touch
  symbol resolution** (`reader`, `nolut`): their bit volume and their coefficient
  counts differ from the real stream, so the delta prices "resolution plus what
  it consumes" with a ~7,200-cycle correction, not the lookup alone. `nodequant`
  and `count` are exact.
- **The wrapper is 432 cycles.** The frame has no glue left to win.
- **83.6% is the worst of a 129-frame iteration corpus, not a worst-case
  guarantee** — C's hardest frame was #27,806 of 167,222. Plan for margin; PR E
  is the real witness.
- Hand-written asm is only worth it once placement is right. Placement is still
  right (code, tables, hot buffers in IWRAM) and Step 1 did not disturb it —
  `.iwram` *shrank* 76 B.

### The kernel table, re-derived with bit-exact arms (2026-11-18, mGBA, WAITCNT 4/2)

**Why the old table was replaced rather than adjusted.** Every by-difference arm
worked by *removing* a stage, and every stage after it then multiplied different
data — so each arm charged the downstream cost change to the part it removed.
The 2026-11-18 `attrib-pre2` arm measured that bias directly: `pre`'s delta came
out **1.61x** too large at the densest frame (31,150 vs a true 19,336), and the
inflation tracked frame density exactly. Three bit-exact arms replace it —
they **add a second, identical instance** of a stage instead of removing one, so
every downstream stage sees the data it would have and the pins still PASS:

| arm | how | bit-exact? |
|---|---|---|
| `attrib-pre2` | `pre` is a pure function of `x` into `z`; call it twice | **yes** |
| `attrib-post2` | `post` reads `z`, writes `y`; call it twice | **yes** |
| `attrib-entropy2` | `bitReader` is `Copy`; replay `dec_chan` from a saved reader | **yes** |
| `attrib-ola` | removal; nothing runs downstream of `ola` | contaminated only via its own stale `p` |
| `attrib-synth` | removes all 8 synthesis calls; no *other* stage changes | **yes** as a total |

`fft` is in-place and `ola` mutates `p`, so neither can be double-called; `fft`
is therefore the one **derived** row (`synth − pre − post − ola`), and it is
flagged as such.

**Clean attribution, 129-frame corpus, product image** (control f83 = 208,867,
mean = 182,132):

| part | at f83 | share | mean | share | source |
|---|---:|---:|---:|---:|---|
| `fft` | 68,694 | 32.9% | 66,079 | 36.3% | **derived** |
| entropy + dequant | 61,751 | 29.6% | 41,972 | 23.0% | bit-exact |
| `ola` | 30,521 | 14.6% | 30,120 | 16.5% | removal (clean) |
| `post` | 23,682 | 11.3% | 23,458 | 12.9% | bit-exact |
| `pre` | 19,336 | 9.3% | 17,669 | 9.7% | bit-exact |
| ms + wrapper | 4,883 | 2.3% | 2,834 | 1.6% | residual |
| synthesis total | 142,233 | 68.1% | 137,326 | 75.4% | removal (`synth`), exact |

Closes exactly by construction. The previous table's per-kernel rows summed to
+5.9% over `synth`; this one to −2.4%, inside the file's own ±3% tolerance.

**Three corrections this buys, against C's per-channel rows doubled:**

| part | old claim | **now** |
|---|---|---|
| `pre` | "31,150 → 1.61x C, the worst ratio left" | **19,336 = 1.00x**; mean 17,669 = **0.91x, ahead of C** |
| `ola` | "1.12x" | 30,521 = **1.00x**; mean 30,120 = **0.98x, ahead** |
| ms | "1.20x" | mean 2,834 = **0.64x, ahead** |

**And one methodological limit the numbers impose.** Because the audit put the
*data* in `Rs` (so multiplies early-terminate), kernel cost now genuinely varies
with frame content, and it varies by more than the gaps being adjudicated:

| part | spread across our 129 frames (max/min) |
|---|---:|
| `pre` | 1.20x |
| `post` | 1.15x |
| `fft` | 1.13x |
| `ola` | 1.08x |
| entropy | 26x (2,478 → 64,227) |

C's entropy row (42,600) sits at our **43rd percentile** — C's worst frame is
synthesis-heavy and entropy-light, ours is the opposite. So a single-frame ratio
against C cannot distinguish a deficit from a frame-mix difference; only an
all-frames mean on the *same* corpus can, and that is why the `cross` row above
is the number to trust.

## The entropy path against the C+ASM reference (2026-11-19)

**Status: nothing here is landed, and nothing here is a measurement of a change.**
It is the comparison that decides what to do next, made by reading
`uli_gba_dec.iwram.c` and our `dec_chan` line by line and disassembling the
built product image.

The FFT is now closed at 1.05x-1.09x C with its whole remainder worth ~5,200, so
entropy/dequant is the only row left with a double-digit deficit. (That figure
comes from the bit-exact `attrib-fft2` arm in the open `perf/uli2-fft-attribution`
PR; this tree's kernel table still reads 1.14x until that lands.) This section is
what the entropy deficit actually consists of.

**Measured shape: a density gap, not a flat one.** The bit-exact
`attrib-entropy2` arm (replays `dec_chan` from a saved `BitReader`; pins PASS)
gives the entropy block for the pair on the 129-frame fixture:

| | cycles/frame | of frame |
|---|---:|---:|
| mean | **41,973** | 23.4% |
| median | 54,868 | — |
| p90 / p99 | 63,456 / 63,963 | — |
| f83 (the control's worst frame) | **61,750** | 30.0% |
| f62 / f98 | 64,226 / 54,737 | 31.6% / — |

The C record's own line is *"entropy decoding about 21–24k cycles per channel"*,
i.e. 42–48k per frame. **On the mean we are already at C; on dense frames we are
1.3–1.4x.** So this is not a fixed overhead to shave off — it is a per-symbol and
per-nonzero-coefficient cost that only shows up on dense frames, which is exactly
why the record's warning that per-part ratios cannot be read off single frames
applies here as much as it did to the kernels.

The sub-frame arms locate it (each removes one layer; `nolut` desynchronises the
stream, so these order the work rather than grading the parts): full path 46,631 ·
coefficient store 2,111 · dequant arithmetic 9,285 · **symbol resolution 27,036** ·
reader mechanics 2,791 · residual (loops, escapes, sign bits, band control) 4,954.

**Structural comparison.**

| | C + ASM reference | Rust + ASM (ours) |
|---|---|---|
| reader state | `{w, n, p}` — **3 words** | `{base, next, end, hw, valid, bad}` — 6 |
| refill | `if (n<16) { w \|= *p++ << (16-n); n += 16; }` — post-indexed, **no bounds test** | `valid<17` **plus a nested `valid<17`** second load, a `next+2 > end` test and a `bad` store |
| refill contract | 16 bits, **refilled again before the raw bits** | **17 bits** up front |
| symbol decode | returns packed `(sym << 5) \| len`, `SKIP` unconditional | `(u16, u32)` tuple; per-symbol `len == 0` branch into `bad` |
| per-class arrays | `ctype`/`rbits`/`esck`/`rq[16][16]` **copied into IWRAM** (`uli_g`) | read from `PRODUCT_TABLES`, **`.ewram` @ 0x02000a00** |
| `q3` table | `uli_g_q3[81]` in IWRAM | `const Q3` in `.rodata` = **cartridge ROM** (0x08038d5c) |
| quad `m == 1` | hoists `r1 = rq[1] * step`, special-cases m == 1 | dropped — see below |
| code | `dec_chan` compact inside ~22.6 KiB of IWRAM | `dec_chan` alone **5,000 B** (`0x1388`) |

**Cause 1 — the reader's safety contract costs the registers the coefficient
loop needs.** The product `dec_chan` opens with `push {r4-r11, lr}` and
`sub sp, sp, #140` (**35 spill slots**), and contains **155 stack references in
1,250 instructions**. The spills are exactly the hot data:

```asm
ldr   r1, [sp, #56]        @ Huffman LUT pointer — every symbol
mvn   r0, #1
and   r0, r0, r7, lsr #21  @ + add: 3 instructions to form the byte offset
ldrh  r0, [r0]

ldr   r4, [sp, #80]        @ rq pointer — every nonzero coefficient
add   r3, r4, r3, lsl #1
ldrsh r7, [r3]
mul   r3, lr, r7

ldr   r4, [sp, #76]        @ dq15 — every m >= 16 coefficient
```

and `step` lives at `[sp, #68]`, reloaded after every `bl huff_slow`
(`ldr lr, [sp, #68]`).

In the inner loops we hold `next`(r4), `end`(r5), `hw`(r7), `valid`(r8),
`bad`(fp) and `step`(lr) — **six** — against C's `p`, `w`, `n`, `step` — four.
The two extra are **`end` and `bad`**, i.e. the bounds check and the error flag,
and those are precisely the registers `rq` and `dq15` would need.

**This re-frames the item, and that matters for how it is sized.** The record
graded this category as a *branch* problem — "almost every item here is a branch,
ARM7TDMI charges 3 cycles for a taken one, remaining ceiling ~2,000" — and that
is also why the two shapes built in the 2026-11-16 sweep failed: the branchless
clamp kept `end` live *and* put it in the load-address chain (+1,352 measured),
and `bad |= over` made `bad` longer-lived. **Neither freed a register.** Read as
register allocation, the same item is worth several times the branch estimate.

**Cause 2 — our per-class tables are in EWRAM and C's are in IWRAM.** At the
bench's WAITCNT 4/2, EWRAM is 4 cycles against IWRAM's 1. The reference
deliberately copies `ctype`, `rbits`, `esck` and `rq[16][16]` into `uli_g` and
leaves only the ~96 KiB of LUTs in EWRAM. Ours reads `rq[m]` from EWRAM on
**every nonzero coefficient** — 616/channel x 2 = 1,232 reads/frame, ~3,700
cycles/frame of avoidable latency. The fix costs **560 B of IWRAM** (`rq` 512 B +
three 16-byte arrays) against **3,876 B free** (`iwram_end` 0x0300673c to `sp`
0x03007660). Same story, smaller: `Q3` (81 B) is in ROM, and the reference keeps
`uli_g_q3` in IWRAM — ~500 cycles/frame across 496 quad groups.

**Cause 3 — the 17-bit contract paid for a run that cannot exceed 16.**
`refill!` demanded 17 valid bits because a raw coefficient reads `r` bits *then*
a sign bit, and 16 + 1 would underflow a 16-bit window. But `r` is only read on
the `pair` path, and **on every corpus this tree owns the PAIR classes have
`rbits <= 5`**:

| corpus | frames | k | max `esck` | `rbits` over PAIR classes |
|---|---:|---:|---:|---:|
| fixture `uli2_tiny` | 129 | 14 | 2 | 0,0,0,0,0,0,1,2,3,4,5 → **max 5** |
| `biomaster` (album) | 16,553 | 14 | 2 | **max 5** |
| `biotwo` | 33,104 | 14 | 2 | **max 5** |
| `adv` (adversarial) | 641 | 14 | 2 | **max 5** |
| `cross` | 167,222 | 14 | 2 | **max 5** |

Measured by parsing the blob through `CodeTables::parse_into`, per class as
`ctype/rbits/esck`: `0/0/0 1/0/0 1/0/0` then eleven `2/0..5/…` — one `ZERO`, **two
`QUAD`** with `r == 0` (where `rbits` is never read), **eleven `PAIR`** with
`r = 0,0,0,0,0,0,1,2,3,4,5`.

That census sits on top of the reference's own `tables_default` (`enc_core.c`) with
**one deviation**:

| field | `tables_default` | every corpus | verdict |
|---|---|---|---|
| `ctype` | `0,1,1,2,2,2,2,2,2,2,2,2,2,2` | identical | the class structure ships unchanged |
| `rbits` | `0,0,0,0,0,0,0,0,0,1,2,3,4,5` | identical | so **E-R4's `rbits ≤ 5` is the format's designed maximum**, not a margin chosen after the fact |
| `esck` | `0,0,**2**,2,2,2,2,2,2,1,1,1,1,1` | `0,0,**0**,2,2,2,2,2,2,1,1,1,1,1` | **class 2 escapes in the default and does not escape in the blob** — training tightened it, in the safe direction |

Two things follow, and both are load-bearing for the encoder. First, `esck` is
**derived**, not copied: the row that reaches the blob is not the row in
`tables_default`, so the encoder must validate its **final, trained** tables against
E-R4 and may not assume the defaults already satisfy it. Second, the bound itself
is the format's own: `max esck = 2` is what the default ships and what training
kept, and `rbits ≤ 5` is unchanged from the default in all five corpora — E-R4 is
the design, stated as a rule.

What genuinely differs per album is the Huffman **length rows**: the tables region
is 5,212 bytes in all five corpora under five different hashes (fixture
`0f5e1b96…`, `biomaster` `89c4eb31…`, `biotwo` `10ac765c…`, `adv` `64809502…`,
`cross` `93ef18f5…`), which is the "trained per album" property showing up exactly
where the plan says it should.

> **Corrected 2026-11-19.** This block previously read "both pair classes have
> `rbits == 0`" and named the two `1/0/0` classes PAIR and the eleven `2/…`
> classes QUAD — the two class names were transposed, and the counts with them.
> The citation was `tests/premise.rs`, which asserted `rbits == 0` on "every PAIR
> class" and passed, because it compared against a local `const CT_PAIR: u8 = 1`
> — which is `CT_QUAD`. The raw path *is* taken (up to 5 raw bits + a sign), so
> the claim was false; the conclusion was not, because the raw pair arm refills
> between the two coefficients of the group (its second `refill!`, `decoder.rs`),
> so no read run exceeds 16 bits. The 16-bit contract stands on that refill, not
> on `r == 0`. The test now imports the format's real constants, asserts
> `rbits <= 5`, carries a negative control that fails if the class id and the
> bound's subject ever part company again, and (a second, unrelated fix) actually
> reads the four generated corpora, whose paths had an off-by-one `../` and
> resolved outside the repository.

The right frame is not "what does a group cost" but **what is the longest run of
reads between two `refill!`s**, since one halfword load always restores exactly
16 valid bits: a band or coefficient symbol is `len <= 16` (LUT 1..=10,
`huff_slow` 11..=16); an escape field is `t = 2z + 1 + esck <= 13` with `z <= 5`
(`CLZ6`'s fast path) — and `esc!` refills after taking it, so the raw digit that
follows is measured from a fresh window at `r + 1 <= 6`. Every run fits 16, the
nested second load exists only for a run that does not occur, and `esck <= 2` is
likewise what makes the escape guard dead: both `t < 32` and `t <= valid` are
unfalsifiable on our data.

Worth naming, because it is the reason E-R4 must live in an encoder: the window
arithmetic alone would tolerate `rbits <= 15` and `esck <= 5`, and the table
parser accepts `rbits <= 16`/`esck <= 24` — **one bit looser than the product
contract at `rbits == 16`**. The parser is not the guard; only the blob's author
is.

**The premise this project can actually rely on.** The product ROM is
closed-world: we control the encoder, the container and the bake, so *"the
stream is well formed" is an input assumption, not a runtime obligation.* It does
not have to be proven during decode, and it does not have to be proven at open
either — it can be **asserted before the ROM is baked**, in the host gate that
already produces the album and its pins, and it is *free* there because that gate
already decodes the whole album with the checked decoder and compares to the
C-pinned hashes.

This is not a new shape for this tree: it is the pattern already used for the
synthesis kernels — `PortableKernels` (checked, host, the oracle) versus
`ArmKernels` (product, unchecked, `unsafe`). The same split applied to the
entropy reader makes the checked `uli_lite::frame` decoder **the oracle for a
premise**, and the product `dec_chan` the unchecked realisation of it. The
portable decoder keeps every guarantee it has (U2 still requires it to reject
malformed frames without panicking on the host); the product build simply does
not carry them.

**Proposals.** Sizes are estimates until each lands as its own arm; every one of
P1-P5 is bit-exact by construction, so the pins stay the witness.

| # | change | expected | risk | prior art |
|---|---|---|---|---|
| **P1** | Closed-world reader in the product build: **no `end`, no `bad`, no bounds test**. Refill becomes `ldrh …, [next], #2` + shift/or. Frees 2 live registers so `rq`/`dq15` become register-resident and the ~1,300 per-frame stack reloads go | **6–10k/frame** (est.) | low *under the premise* — needs the bake gate to be the only entry point; the checked decoder stays as the oracle | C's structure exactly; the *branchless* and `bad \|=` shapes were falsified, this one was never tried |
| **P2** | Copy `rq` + `ctype`/`rbits`/`esck` into IWRAM (560 B) | **~3.7k/frame**, near-certain | very low — placement only, bit-identical | C's `uli_g`; `iwram-tables` was the same move for the synthesis tables (−10.4%) |
| **P3** | 16-bit contract everywhere (justified by pair `rbits <= 5` plus the raw arm's mid-group refill, which caps every read run at 16 bits): removes the nested second load, and is what makes P1's single-load refill correct | 1–2k/frame | very low — measured across all five corpora | C's 16-bit contract |
| **P4** | Re-test C's `r1 = rq[1] * step` + `m == 1` fast path **after P1/P2** | **FALSIFIED 2026-11-19: +2,310 mean worse** | — | the old note blamed spilling; P1 fixed the spilling and it *still* lost — see below |
| **P5** | `Q3` (81 B) and `CLZ6` (64 B) to IWRAM | **LANDED 2026-11-19: −570 mean / −104 worst** | trivial, +148 B IWRAM | `uli_g_q3` in IWRAM |

**P2 LANDED (2026-11-19) — and the estimate above was 4x too high.** Shipped as
`ClassTables`: a 560 B caller-owned IWRAM object (`ctype`/`rbits`/`esck`/`rq`)
that `GbaDecoder::new` fills once per album, kept separate from `HotState` so the
8,192 B invariant and `reset()`'s per-frame contract both survive. Measured,
bit-exact, pins PASS on fixture and album:

| | before | after | delta |
|---|---:|---:|---:|
| entropy + dequant, mean (bit-exact `entropy2`) | 41,973 | **41,075** | **−897** |
| entropy at its worst frame (f62) | 64,226 | **61,728** | **−2,498** |
| 129-frame product frame, worst / mean | 205,793 / 179,058 | **203,394 / 178,168** | −2,399 / −890 |
| album decode wall, worst / mean | 206,101 / 175,280 | **203,913 / 174,532** | −2,188 / −748 |
| gate total, worst / mean | 237,646 / 206,637 | **235,418 / 205,844** | −2,228 / −793 |
| gate margin at the worst frame | 9.35 pts | **10.20 pts** | +0.85 |

`.iwram` grew **784 B**, not 560: the table is 560 (the size assertion confirms
it) and `dec_chan` grew **196 B** (0x1388 → 0x144c) because `class` arrives as a
third pointer argument. Stack high-water 964 → 948 B.

**Why the 3.7k projection was wrong, in the informative direction.** It counted
the access (`ldrsh` from EWRAM, 4 cycles vs 1) and not what the access sits
behind: the disassembly shows `ldr r4,[sp,#80]` — the **`rq` *pointer* reloaded
from the stack** — then `add`, then `ldrsh`, then `mul`. Moving the table to
IWRAM removes the last link and leaves the first two, and the extra pointer
argument adds to the very register pressure that causes the spill. So the win is
real and density-shaped (−2,498 at the densest frame against −897 on the mean),
but it is a fifth of the projection, and the remaining `rq` cost is **P1's** —
it does not go away until `rq`/`dq15` are register-resident.

**P1 LANDED (2026-11-19), with P3 folded in — and this time the projection held.**
The product bit reader is now **closed-world**: no `end`, no `bad`, no bounds test,
no per-symbol `len == 0` test, and a **16-bit** refill contract instead of 17
(P3, justified by the `rbits <= 5` measurement above together with the raw arm's
mid-group refill), which is what makes one load always sufficient. The checked
reader survives behind `checked-reader`, and
every attribution image now builds with it through `attrib-dump` — a by-difference
arm desynchronises the stream and needs `bad` to resynchronise, and keeping the
control image on the same reader keeps the deltas comparable to each other (their
*absolutes* were never the product number).

| | before | after | delta |
|---|---:|---:|---:|
| 129-frame product, worst / mean | 203,394 (f83) / 178,168 | **193,421 (f98) / 172,748** | −9,973 / −5,420 |
| album decode wall, worst / mean | 203,913 / 174,532 | **195,439 / 169,851** | −8,474 / −4,681 |
| gate total, worst / mean | 235,418 / 205,844 | **226,853 / 201,126** | −8,565 / −4,718 |
| gate total, % of period | 89.80% / 78.53% | **86.54% / 76.72%** | −3.26 / −1.81 points |
| margin at the worst frame | 10.20 pts | **13.46 pts** | +3.26 |
| `dec_chan` code | 5,196 B | **3,820 B** | **−1,376 B** |

`.iwram` *shrank* 1,376 B — the closed-world reader deletes more code than the
registers it frees cost — so the P1 register story came with memory back.

**The projected 6–10k was right, unlike P2's.** The worst frame lands at ~9,973
and the mean at ~5,420: the win is again density-shaped, but this time the
mechanism *was* the address chain and the spills, which is what the projection
counted. `end`/`bad` are no longer live inside the coefficient loops, so
`rq`/`dq15`/`step` are register-resident and the per-symbol `ldr [sp,#56]` /
per-coefficient `ldr [sp,#80]` / `ldr [sp,#76]` reloads are gone.

**Entropy/dequant is now at or below C, by subtraction.** The bit-exact arm reads
41,076 on the *checked* reader and nothing but the reader changed, so the product
block is **~35,656 mean** (41,076 − 5,420) against C's 42–48k. The mean is now
ahead of the reference; what remains is the densest frames.

**The premise is now an assertion, not a comment.** `crates/uli-lite-gba/tests/premise.rs`
checks `rbits <= 5` on every PAIR class and `esck <= 2` on every class, for the
committed fixture and for all five generated corpora when present — all of them
measuring `max rbits == 5`, `max esck == 2` on all of them (the class structure is
`tables_default`'s apart from one trained tightening; the length rows differ, as
they must). It also witnesses itself: a negative control rejects a PAIR class at
`rbits == 6`, so the
green cannot come back from checking nothing (it had, once — see the correction
under Cause 3). That is the seed of the bake gate: it is a property of the
**tables in the blob**, so it belongs wherever the shipped album is produced, and
it must run before the ROM exists rather than in it.

**P4 is FALSIFIED (2026-11-19) — and the falsification is the interesting
result.** The change was built exactly as the reference has it: `r1 = rq[1] *
step` hoisted out of the quad group loop, and digit 1 decoded as a sign bit and
nothing else. Bit-exact, pins PASS, and **slower on both corpora**:

| | baseline (P1) | with P4 | delta |
|---|---:|---:|---:|
| 129-frame, worst / mean | 193,421 / 172,748 | 199,121 / 175,077 | +5,700 / **+2,329** |
| album decode wall, worst / mean | 195,439 / 169,851 | 199,532 / 172,161 | +4,093 / **+2,310** |
| gate total, worst / mean | 226,853 / 201,126 | 230,976 / 203,506 | +4,123 / **+2,380** |

Two shapes were measured, because the first one was suspected of being the
problem rather than the idea: the fast path *nested inside* `if mm != 0`
(+2,106 mean) and *tested first*, making C's own two-branch chain `if (m == 1) …
else if (m)` (+2,329). Both lost, so it is not the branch nesting.

**Why the old rejection was right for a reason that had already stopped being
true, and then became true again the other way round.** The note said the hoist
lost because *"its two operands are spilled"* — a symptom of the reader's
register pressure. P1 removed that, so the item deserved the re-test. But P1 and
P2 also **removed the cost the fast path exists to avoid**: `rq[1]` is now an
IWRAM read in a register (1 cycle, not a 4-cycle EWRAM load behind a stack
reload) and the multiply's `Rs` is the small dequant value (1–2 cycles). What is
left is smaller than the price of skipping it — ARM7TDMI charges 3 cycles for a
taken branch and has no predication, and the digit is data-dependent so it
predicts poorly. **P4's value was consumed by the two PRs that unblocked it.**

That is the same lesson the 2026-11-16 loose-ends sweep learned from the other
direction (there: *"almost every item is a branch"*; here: an item that looks
like arithmetic is really a branch once the arithmetic gets cheap), and it is
why an item must be re-sized **after** the changes that alter its inputs, not
before.

**P5 LANDED (2026-11-19): −570 mean / −104 worst** — `Q3` and `CLZ6` moved from
`.rodata` (cartridge) to `.iwram`, as `static` rather than `const` so each is one
object with an address instead of a value the compiler may re-emit at every use.
Album **194,695 / 169,354** (was 195,439 / 169,851); gate **226,152 / 200,624 =
86.27%**, margin **13.73 points**. Cost: **+148 B of IWRAM** (145 B of table).
The projection (0.5–1k) held.

**The sub-frame partition, re-run (2026-11-19, in PR #165).** Mean cycles/frame
on the 129-frame fixture, each arm removing one layer from the full path:

| layer | pre-P2/P5 | **re-run** | share |
|---|---:|---:|---:|
| symbol resolution | 27,036 | **26,175** | 58% |
| loops, escapes, sign bits, band control | 16,350 | **16,035** | — |
| dequant | 9,285 | **8,774** | — |
| residual | 4,954 | **5,187** | — |
| reader mechanics | 2,791 | **2,793** | — |
| coefficient store | 2,111 | **2,003** | — |
| **full path** | 46,631 | **45,381** | 100% |

**The shape is unchanged — symbol resolution is still ~58% of the entropy path —
and the absolute moved only 1,250** (P2's −897 and P5's −570 on this reader).
Two caveats come with that, and the second is the important one:

1. **This is the *checked* reader.** `attrib-dump` now implies `checked-reader`,
   because the `nolut` and `reader` arms desynchronise the stream and need `bad`
   to resynchronise. So these are not the product's numbers: the product's
   entropy block is ~35,656, and the ~9,700 difference is exactly what P1
   removed — the per-symbol LUT-pointer reload and the per-coefficient
   `rq`/`dq15` reloads, which every arm here still pays.
2. **Only two of the six arms desync.** `nolut` and `reader` substitute or remove
   the symbol decode, so the bit cursor diverges and what follows is a different
   workload. **`nodequant` and `nowrite` do not touch the reader at all** — they
   remove the dequant arithmetic and the store — so they are sound on any reader.

**So the next instrument step is not another optimisation.** It is to decouple
`attrib-dump` from `checked-reader` so `nodequant`/`nowrite` can be built against
the **product** reader, and to add a symbol-resolution arm that does not desync
(the 10-bit LUT covers every code on this corpus — `slow_hits=0` — so a
length-preserving substitution exists: read the same number of bits, return a
fixed symbol). Until then "symbol resolution is 58%" is a statement about a
reader the product does not ship, and sizing work from it would repeat P2's
mistake in the other direction.

**Where this leaves the gate against C.** The reference's own record gives
comparable figures — decode wall **69.43%** and gate total **73.63%** at 4/2 — so
the two halves can be compared for the first time:

| | C + ASM | ours (after P5) | delta |
|---|---:|---:|---:|
| decode wall, worst | 182,007 (69.43%) | **194,695 (74.27%)** | **1.070x**, +4.84 pts |
| gate total, worst | 193,017 (73.63%) | **226,152 (86.27%)** | 1.172x, +12.64 pts |
| **non-decode half** (gate − decode) | **11,010 (4.20 pts)** | **31,457 (12.00 pts)** | **2.86x** |

**The decoder is no longer where the gate gap lives.** Of the 12.64 points we are
behind C on the gate, 4.84 are decode and **7.80 are the convert/IRQ/DMA path**,
which costs ~2.9x C's. C's figure also errs *high* (their doc notes the DMA share
is counted twice in the gate total), so the true gap is at least as large. That
makes the player's output half — not the codec — the largest remaining single
difference from the reference, and it is where the next measurement should go.

**P4 deserves its own paragraph, because the record currently treats it as
settled.** `coef!` carries *"C hoists `rq[1] * step` and special-cases `m == 1`
here. Measured in this image, that hoist is not a win — its two operands are
spilled."* That is a description of Cause 1, not a property of the change. The
fixture is **248 quads and 0 pairs**, so `m == 1` is the most common digit in the
stream and the hoist would remove both an **EWRAM** table read and a multiply
from it. It must be re-measured once `rq` and `step` are register-resident.

**What the premise re-opens.** Three items the 2026-11-16 sweep declined
*for safety reasons* become free once the product build carries no error path:

| item | why it was declined | under the premise |
|---|---|---|
| `huff_consume!` length test | "removing it needs a failure signal that is not a per-symbol branch" | no failure signal is needed — ~624 symbols/frame x the branch |
| `esc` fast-path guard | "~2 compares per escape; the parser's `esck <= 24` bound makes `t < 32` live" | measured `esck <= 2`, so `t <= 13` and both compares are dead |
| `huff_slow` "no match" return, band-level `if bad`, `k > MAXK` early-out, base-alignment check | each guards a premise | all become **bake assertions** |

**How each would be falsified.** `attrib-entropy2` gives the clean per-frame
total and every one of P1-P5 is bit-exact, so the pins PASS and the two-runs-to-
the-cycle rule applies unchanged. P2/P5 are readable off the section table
(`objdump -t | grep .iwram`) as well as off cycles. P1 needs a *positive* witness
for its premise, which is the one genuinely new piece of work in the list: the
bake gate must decode the whole album with the checked decoder and fail the
build, not the frame, on any rejection — and the unchecked product build must
still reproduce the pinned hashes.

**Caveats.** C's 21–24k is measured on *their* corpus, ours on the fixture: the
ratio is a diagnostic and the comparison of record is the structural table. The
sub-frame arms are timing-only and `nolut` desynchronises the stream. And P1's
premise is about *our* encoder: a third-party or re-encoded blob with different
tables is not covered unless the bake assertions cover it.

> **One caveat withdrawn (2026-11-19).** This paragraph also said "the fixture has
> **zero pairs**, so two of the five inlined `coef!` expansions never execute
> there — `cross` should confirm before P3 is sized." It is false, and it is the
> same class-name transposition wearing a different hat: believing the only PAIR
> classes were the two with `r == 0` implies the raw pair arm is dead code on our
> data, and therefore that two of the five `coef!` expansions never run. `make
> uli-frame-ops` on the fixture counts **15,984 pair groups and 10,480 raw-field
> reads** over its 129 frames (`adv`: 78,104 / 8,016; `biomaster`: 1,626,832 /
> 670,464 — frame counts match the Cause 3 table, so these are the same corpora).
> Both pair arms and the quad arm execute on the fixture, so all five expansions
> are covered there and P3 needed no confirmation from `cross`. The product path
> executes the same symbols as the portable decoder that produced these counts —
> that is what `make uli-overlay-test`'s full-corpus bit-exact differential
> witnesses.

## The encoder — the last missing piece (plan 2026-11-19; two namespaces, same day)

**Why it blocks the album player.** Everything downstream of the decoder is
ready: `uli-playback` already implements the app's `PcmSource` seam
(`UliSource<'a, D: FrameDecoder>`), `ALBUM-PLAYER.md` §4.2b already specifies
`src/backend/uli2.rs` under `make album-rom BACKEND=flac|uli2`, and the ULA2
container already carries tracks, titles, artists and a seek table. What is
missing was the ability to **make** a `.uli` blob from our own audio: it used to
require the C encoder in `gba-audio`, which the project's rule forbids depending
on. **That gap is closed** — `crates/uli-enc` covers S1–S8 and emits the oracle's
bytes — so the remaining work is W2 (point the asset targets at it) and W3 (the
app's backend), not an encoder.

**What the reference is.** ~890 lines of C plus two Python tools, and the shape
matters more than the size:

| piece | what it does |
|---|---|
| `enc_core.c` | the codec: `tables_default`/`tables_derive`/`tables_from_stats` (Huffman lengths trained from counts, with a floor), the bit writer (`bw_put`, `bw_put_eg`, zigzag), the forward MDCT (`mdct_frame`, orthonormal, hop 512, coefficients ≥500 zeroed), and `frame_code` — an **RD-optimised** per-frame search over the global step `g` and the M/S flag, cost `D + kappa·bits + mu·decoder_cycles` |
| `uli_enc.c` | the CLI and album assembly: `--budget`/`--cap %` sizing, two-pass table training, per-frame parallelism, ULA2 container write |
| `prep.py` | `ffmpeg` decode, resample to 32,768 Hz, mono→stereo, gapless track join, `tracks.tsv` with ASCII-folded titles |
| `gen_tables.py` | regenerates `src/uli/uli_tables.c` — the **synthesis** constants (PERM/PRE/POST/TW/WIN, variant P, N = 1024 / M = 512) and the step table `uli_step[g] = round(2^(g/8))`. **It does not emit the code tables**: the default `ctype`/`rbits`/`esck`/`mu` rows are C literals in `enc_core.c:tables_default`, and the default lengths come from a geometric model through `huff_lengths(·, 0.5)` |

Two of those deserve emphasis because they are not obvious from the format:
`mu` makes the encoder **cost the decoder's cycles in the RD objective**, so it
can trade a little distortion for decode speed — a knob this project values more
than a general-purpose codec would; and the tables are **trained per album**, so
the encoder writes the Huffman codebooks into the blob.

### Two namespaces, and one that belongs to somebody else

| Prefix | Means | Cardinality |
|---|---|---|
| **E-R1 … E-R10** | **Requirements.** Permanent, testable claims about the encoder. An amendment changes the requirement; it never opens a ticket | 10 |
| **S1 … S8** | **Steps.** Concrete work, each discharging named requirements, each with an exit gate. This is the only axis you can be *on* | 8 |
| R5a … R5d | Determinism rules, internal to **E-R5** | 4 |

Nothing else. In particular **observations, measurements and defects are not a
namespace**: they live as *rationale* under the requirement or step they inform,
and are cited by name ("the 1,053-byte row"), never by number. Decision 5 below is
why, and what it replaced.

**`E1`…`E6` are deliberately not step names.** They already mean something else in
this repository: `RECORD.md` and the Makefile use bare `E1`, `E2`, `E3-a`, `E3-b`
for **PR E's measurement gates** (the tail, the gate total, the two pre-hardware
corpora), and the FLAC spikes add a third `E1`–`E10` of experiment arms. The
encoder's steps were `E1…E6` until 2026-11-19 and are `S1…S7` from today; older
sentences in `RECORD.md`/`ULI2.md` that say "E1" about the encoder mean **S1**.

### Requirements

| # | requirement | why |
|---|---|---|
| **E-R1** | In-repo **Rust**, host-only, no C, no `gba-audio` dependency at build or encode time | the standing rule; and a C oracle we cannot ship is not an encoder. Note the posture it does *not* forbid: running the reference as an oracle off-tree and checking in **its bytes** — which is how S1's tables golden works, as it is how `uli-album-asset` already makes a corpus |
| **E-R2** | **The contract boundary is PCM**: `album.f32` + `tracks.tsv` in, one blob out. `album.f32` is float32 **interleaved stereo at PCM16 scale** (samples × 32768), tracks joined gaplessly at 32,768 Hz; `tracks.tsv` is `n start len title artist` in 32,768 Hz samples, and **the track table sets the album length** (`NS = last.start + last.len`) — `album.f32` must be exactly `NS` samples, so a padded tail is a refusal and not an album. A WAV front end (PCM 8/16/24/32, mono or stereo, any rate) is a convenience layer on top, and the resample inside it is **not** held to byte-equality | the reference's own CLI takes exactly this pair, `uli-album-asset` already feeds it, and a source already at 32,768 Hz needs no resampling at all. Holding byte-exactness at the PCM boundary makes it testable; holding it at the WAV boundary would make it depend on which resampling library answered — see "The acceptance question". Who owns `NS` is spelled out because the two encoders disagreed about it once, and the argument is under S1 |
| **E-R3** | Writes exactly the ULA2 container `uli-lite`/`GbaDecoder` parse: one blob, ≤32 tracks, per-track title/artist/preroll/bitpos, seek table every `SEEK_INTERVAL` frames, strings, tables blob | anything else is a second format. The header's `est_max_cycles`/`est_mean_cycles` are container fields too, so their owner is fixed by decision 4 |
| **E-R4** | **The encoder owns the closed-world premise: `rbits ≤ 5` on every PAIR class and `esck ≤ 2`.** Refuse to emit otherwise | the product reader has a 16-bit refill contract and no error path; a raw pair coefficient costs `r+1` bits after the group's refill, so `r ≤ 5` keeps 10 bits of margin. `esck ≤ 2` caps the escape field at `t = 2z+1+esck ≤ 13`. Both are **tighter than the window needs** (`r ≤ 15`, `esck ≤ 5`) and far tighter than the *parser* allows (`rbits ≤ 16`/`esck ≤ 24` — note 16 is already one bit past the contract); only the encoder can make the product's premise true. Measured maxima on all five corpora: `r = 5`, `esck = 2` |
| **E-R5** | **Deterministic**: identical input + identical flags → byte-identical blob — **at any `--threads`, on every host, wherever the bytes come from integers; on the reference host, wherever they cross a float.** The four rules below | pins are the whole verification story, and byte-exactness is unreachable without it. The split clause is an amendment, not a loosening: it says what is actually true of a codec that trains Huffman lengths from a geometric model through `pow`/`sqrt`, and the fix for any table we care about is to make it integers by construction (S1's golden does this for the defaults; S4 owes the same for the trained ones) |
| **E-R6** | **Size is an argument, not a fact about the cartridge**: `--budget BYTES` (default `33_292_288` = 32 MiB − 256 KiB, the reference's own default) and `--cap PCT` (default **74**, i.e. % of the 262,144-cycle period). Compute the container's fixed overhead and the resulting bits/frame target, and **refuse before encoding any frame** if the settings cannot fit — whole album, exit nonzero, both numbers cited | the ROM holds code, art, `viz` streams and more than one asset producer, so no encoder can know the cart layout; and per `ALBUM-PLAYER.md` §10.1 **art and reaction data are carved out of the audio budget, never the other way round**, which is exactly why the audio budget has to be an input |
| **E-R7** | Quality parity with the reference on identical PCM: codec-only SNR within a stated tolerance of **32.29 mean / 25.78 worst dB** at cap 74 | otherwise the format's own record stops describing our encoder. Note the metric is **codec-only**: the reference CLI's own `verify:` line prints a different thing (full-band time-domain SNR of the decoded album against the resampled input, at pcm16 and int8 — e.g. `31.78/30.80` for `biomaster`), so the two numbers are not comparable and must not be quoted against each other |
| **E-R8** | The RD search carries the decoder-cycle term (`mu`), at least as an option | it is the project's distinguishing knob and it feeds the playback-loop budget |
| **E-R9** | A 43-minute album encodes in **minutes**: frame-parallel (parallel compute, ordered emit — decision 1) | the bake is in the edit loop |
| **E-R10** | **Never writes a ROM, and never learns that one exists.** Its output is a file, sized against `--budget`; the aggregation into a cartridge belongs to `scripts/album_bake.py`, which owns `cart_bytes` (§9) and will call several independent asset producers | the tools are independently configurable precisely because the set of assets per ROM is not fixed |

Every requirement has a step that discharges it, and no step exists without one:
E-R1/E-R3/E-R4/E-R10 → S1; E-R5 → S1 (ordering) and S7 (the claim itself);
E-R2 → S1 (boundary) and S5 (WAV layer); E-R6 → S1 (arithmetic) and S5 (real sizes);
E-R7 → S2/S3 measured, S7 accepted, and **S9** now owns the SNR arm it sets; E-R8 → S6; E-R5 → S8.

### Five decisions

Each cheap **only** if taken before the code exists. Two placements they imply —
where the write side lives, and where the E-R4 bounds live — are under "Where it
lives"; the boundary at which quality and byte-exactness are judged is under
"The acceptance question".

**1. The writer plans, then emits — and the byte target is an argument.**
One dependency has to be settled at S1 because it is not obvious: **the default
code tables are not in this repository.** `tables_data.rs` carries the synthesis
constants only, and the default `ctype`/`rbits`/`esck`/`mu` rows are C literals in
`enc_core.c`, which may not be vendored as C source. The precedented route is the
one `tables_data.rs` already takes — transcribe the integers into a Rust module with
a provenance line naming the source, under `publish = false` — and the lengths then
get **computed** from the same geometric model rather than pasted, because a pasted
length table is an opaque blob. What witnesses the computation is not the committed
fixture: every `.uli` corpus in this tree carries *trained* tables, so they diverge
from the defaults at the first length row (byte 43). The witness is a golden of the
reference's own `tables_blob(tables_default)` output, and that is the only thing that
could be — see S1.

The shape of the writer itself: two shapes were on the table — *stream as you go*
(encode frame 0, append; encode frame 1, append) or *plan then emit* (fix tables and
the per-frame content plan, size every frame, then write frames in index order).
**Plan then emit.** Note that the streaming shape was never actually available: every
`SEEK_INTERVAL`-th frame needs a container entry carrying that frame's bit position
and its `g`, and `g` is differentially coded across frames
(`g = g_prev + zigzag(z)`, `frame.rs`), so the encoder must know every frame's size
and every frame's `g` before it can write the header. What the decision really fixes
is *where that requirement gets designed in* — at S1, where the writer is new, rather
than at S5, under tests, measurements and pinned hashes. It also settles the parallel
shape: because the content plan comes from the allocation stage, the frame bodies are
mutually independent once it is fixed, which is what the reference itself does ("final
encode (parallel chunks, concatenated)").

**2. Determinism, stated as four rules** (the mechanism behind E-R5):

| # | rule | cost |
|---|---|---|
| R5a | threads **compute**, never **order**: any work may be parallel, assembly walks frames by index | none — forced by decision 1 |
| R5b | every sum that feeds a decision is reduced in **frame-index order**, never in completion order | a few percent in one loop (IEEE addition is not associative, so an unordered float reduction can flip a near-tie and change every byte after it) |
| R5c | transform constants come from **checked-in integer tables**, not from libm at encode time | one generator script + a checked-in file; the precedent is `uli-lite/src/tables_data.rs`, and S1 applies it to the default code lengths too |
| R5d | every tie names its winner (lower `g`; then L/R over M/S) | none |

The witness is cheap and belongs to S1: encode the same input at `--threads 1` and
`--threads N` and compare the bytes.

**3. Refusal is early, whole-album, and arithmetic.** Compute overhead → derive the
bits/frame target → check the plan → *then* encode, which is the order the
reference uses (`container overhead 5880 bytes; stream target 26304896 bits =
1589.1 bits/frame`). If the chosen `--cap`/`--budget` cannot fit, refuse the whole
album with a nonzero exit citing needed and allowed, in the style of the bake's
other refusals ("with a cited number") — **not** by trimming quality on some tracks
until it squeaks in, since a silent squeeze both varies by input and breaks E-R5. And
`--budget` means **blob bytes**, never cart bytes: the bake is what subtracts code
reserve, art and `viz` from `cart_bytes` and passes the remainder in. S1 extends the
same discipline to two refusals that are not about size at all — a track table that
disagrees with the file, and a `g` the reader cannot look up — because the mechanism
is identical and the alternative in both cases is a blob that passes everything and
then will not play.

**4. The cycle number has one owner: the decode.** The header's
`est_max_cycles`/`est_mean_cycles` are container bytes inside the byte-exactness
comparison, and the two encoders reach them by different routes. The reference
records `cv.C[f*NG + k[f]]` — the cycle count of a *trial* decode performed during
the RD search (`enc_core.c:404`) — so its number is an estimate of a frame it has not
yet emitted. Ours comes from `self_verify`, the real decode of the finished stream,
and the model's constants are verified identical to `uli_dec_ref.h`'s (all 14
`ULI_CY_*`; a silence frame is 156,806 cycles = 59.8 % of the period by either
route). **Ruled: ours is the number**, because it is measured on the bytes that ship
rather than on a stand-in. Two consequences, both S3's problem and neither
discoverable at S1: the trial op counts and the real ones must agree (they can drift
on `long_codes` and `escs`, which depend on the LUT the decoder actually built), and
`self_verify` decodes the album twice — once to check it, once in the search — which
is ~2× the tail on `biomaster`'s 16,553 frames and wants a shared pass by S5.

**5. Two axes, not three.** This plan previously carried a third namespace — ten
lettered findings from S1's review, each with a "lands in slice" column — and it
did not work. Three of the ten turned out not to be work at all: one was a
requirement written more strongly than it could be honoured (E-R5's "on any
host"), one was a decision about who owns a header field (now decision 4), and one
was a measurement result that belongs in the rationale. Meanwhile the queue's
presence quietly implied that "pick up the fourth finding" was a thing a session
could do, and it is not: you can only pick up a step. So the findings are gone as
addresses and survive as **evidence attached to the step or requirement that
decided them**, which is where they were always doing their work. The test that
keeps it honest: *an observation that cannot be attached to a requirement is not a
step, it is a note* — and the two that failed that test (four clippy warnings, a
long-argument hygiene cleanup already done) are notes and are not tracked.

### Where it lives

`crates/uli-enc` — host-only, `publish = false`, depending on `uli-lite` for
`format` and the `Huffman`/`CodeTables` types so the bitstream is defined
**once** and the encoder cannot disagree with the decoder about it. A `uli-enc`
binary is the CLI. Python remains for orchestration only (source decode,
resample, track metadata), which is what `ULI2.md` already says: the rule is
"no C tooling", not "no Python". The one thing that must not happen is a second
copy of the format constants — that is exactly how these docs drifted.

**`uli-lite` stays read-only, and that is intentional.** It is the codec as the
GBA needs it: parse, decode, no writing. So the *write* side of the format lives
in `uli-enc` and nothing is added to `uli-lite` to support it. One consequence is
worth stating rather than discovering later: the tables-blob geometry is not
named constants today, it is arithmetic inside `CodeTables::parse_into` —
`1 + 3k + (k+1)² + k·(81+256+16)` bytes, with class-row offsets `lq = p` and
`lp = lq + 81k` — so `uli-enc` reproduces a layout it is not handed. That
duplication is **tolerated** because it is self-witnessing — get an offset wrong
and `parse_into` rejects the blob — unlike the bounds and defaults, which drift
silently. The residue a round-trip does *not* catch is a blob that parses cleanly
but differs from the reference's bytes, and S1's review found exactly that, 1,053
bytes of it, in the one part of the blob the reader never looks at. That is why the
golden exists and why it is a *byte* comparison rather than a decode: see S1.

**The E-R4 bounds have one home.** They used to exist as a private helper inside
`crates/uli-lite-gba/tests/premise.rs`, where the encoder could not reach them, and
repeating `5` and `2` a second time in `uli-enc` is the failure mode this file has
already documented. Done: `format::ENC_PAIR_RBITS_MAX` / `ENC_ESCK_MAX` plus
`CodeTables::premise()`, returning an allocation-free `Result<PremiseCensus,
PremiseViolation>` (`uli-lite` is `no_std`; host callers format it), called by the
encoder before it emits a byte and by the test as its witness. Validation is
read-side, so this does not reopen the paragraph above.

### Steps, each discharging named requirements

| Step | Delivers | Discharges | Exit gate | State |
|---|---|---|---|---|
| **S1** | Skeleton: PCM boundary, bit writer, plan-then-emit container, default tables, budget arithmetic, silence + DC frames | E-R1, E-R2, E-R3, E-R4, E-R5·R5a, E-R6, E-R10 | blob parses and decodes; `premise()` called by the encoder; `--threads 1` and `--threads N` give the same bytes; **the default tables blob equals the reference's byte for byte**; the frame plan is refused if any `g` leaves the reader's window; the suite runs under `make uli-test` | **done** — 39 tests, `make uli-check` + `make uli-test` green |
| **S2** | Forward MDCT + fixed-step quantisation, default tables | E-R5·R5c, E-R7 (partly) | decodes bit-exactly; SNR sane; rate reported. Fills the `GPlan` S1 fixed at silence, so the step-index bound and the per-frame `frame_bits` walk become live rather than degenerate | **done** |
| **S3** | Rate/distortion quantisation and the `g` search: per-coefficient `J = D + kappa·D²·R`, band-class choice, the spectrum-end decision, λ bisection against the budget, cycle cap | E-R5·R5b/R5d, E-R6, E-R7 (L/R only) | rate/quality within tolerance of the reference's curve at equal budget, **at `ms = 0`**: rate ≤ 1.01×, distortion ≤ 1.03× at fixed `g` (`make uli-rd-ab`); the cap respected; `--threads 1` = `N` byte-identical | **done** |
| **S3b** | **M/S**: the `1/√2` forward rotation, the `√2` step compensation, `ms` in the candidate set (weighed per `g` with one λ, as the reference does), `ms` in the cycle model | E-R7, E-R6 | with `ms` forced, our distortion matches the reference's at fixed `g` (exact at g=100/140); at equal budget the search beats L/R-only on correlated stereo; the SNR gap to the reference closes | **done** |
| **S4** | Two-pass table training: the album's own symbol counts into Huffman lengths, and its own reconstruction errors into `dq`, then a second search with the trained set | E-R4, E-R5, E-R7 | output byte-stable across runs; the trained set is a complete prefix code and stays inside E-R4; training measurably beats the built-in tables at a binding budget; and the band-class choice is a shortest path, since trained context codebooks differ | **done** |
| **S5** | Album assembly (WAV front end, gapless join, tags verbatim), `--budget`/`--cap` on real sizes, frame-parallel search | E-R2, E-R6, E-R9 | a 43-minute album encodes inside a **passed-in** `--budget` at cap ≤ 74; every track decodes (each seeked to its own preroll); hashes pinned; too small refuses **before the first frame**, citing both numbers; a 32,768 Hz WAV reaches the same blob its pre-resampled PCM does, and a resampled WAV is quoted by SNR; the CLI prints the reference's columns | **done** — see "S5" below for the numbers and the defaults-restored failure |
| **S6** | `mu` (decoder-cycle-aware) search: re-code an over-cap frame by dropping its cheapest coefficients instead of refusing the candidate | E-R8 **discharged**; E-R7 **contributed to, not discharged** — its criterion is codec-only and unmeasured, and S7 accepts it | a knob that demonstrably trades SNR for decode cycles, measured on the bench — and, measured 2026-11-20, it closes the last high-rate gap: at `--budget 400000 --cap 74` the equivalent mechanism is worth **1.48 dB** to the oracle, against a 1.15 dB gap | **done** — `--musearch 1` by default; worth **1.44 dB** here (39.57 → 41.01 dB) and the oracle's 41.02 is 0.01 dB above, cap 74.0% max on both arms. See "S6" below |
| **S7** | **The acceptance matrix, as a buildable target** | E-R2, E-R5, E-R7 (and E-R1's oracle posture) | the four cells below run in CI-shaped form and compare SHA-256s against the reference's blobs, including the two header cycle fields | **done; its DIFF verdict is superseded by S8's MATCH (2026-11-27)** — `make uli-accept` runs the cells and the `--dacw 1` row; all five cells now MATCH, and the original DIFF is kept below as the record of what S8 closed |
| **S8** | **Byte-exactness on the reference's `--dacw 0` arm**: attribute the S7 DIFF with instruments rather than bytes (the pass-1 plan is not in the blob), then close every named divergence until the matrix matches — the SNR arm is **withdrawn from S8** and becomes S9 | E-R5 (exactness is fair to ask: the oracle is deterministic), E-R2 (the boundary is PCM), E-R7 (moved to S9 — see "S8 — the plan") | every matrix cell byte-identical at `--dacw 0`, checked by `make uli-accept` with the oracle's sha256 recorded, `make uli-oracle-check ULI_ORACLE_EXPECT=clean` green, and every pin re-recorded | **done (2026-11-27)** — all sub-steps are implemented and the pin ledger is discharged below; with the generated corpus, `make uli-accept` reports **MATCH on every cell it runs: `fast`, `tiny`, `biomaster`, `off-default` and `adv`** (the fourth slot when the adversarial corpus is present). `cross60` is not run while `adv` is present. The S7 MATCH re-take is in §S7 and the pin ledger is discharged in §S8 item 9. The sub-step table at the head of "S8 — the plan" carries each row's `needs`, its status and what it unblocks, so no prerequisite is prose elsewhere |

S7 is a step rather than a property of the plan because it has work in it — a host
target that builds nothing of ours from C, calls the off-tree binary, compares hashes
— and it stood as prose from the gate's declaration until it became that target.

#### S8 — the plan: byte-exactness on the `--dacw 0` arm

S7 established that the oracle is a fixed point we do not reproduce, and that the first
content difference lands inside the trained tables in every cell. S8 makes that difference
**attributable** and then **closes** it. The instruments, the divergence inventory, the
nine sub-steps, each exit gate, the pin discipline and the eight decisions that would
otherwise be relitigated are all below; a reader picking S8 up cold should not need any
other document.

**Where the sub-steps stand (2026-11-25), and this table is the answer to "what is next".**

| sub-step | what it is | needs | status | unblocks |
|---|---|---|---|---|
| **S8-0** | prune the flag surface (delete `--stride`/`--fine` and the refine stage, `RETRY_ATTEMPTS` 6, `g_guess` ties-to-even) | — | **landed 2026-11-25** | S8-7 (rows 5, 6, 7, 11, 12) |
| **S8-1** | P1, the frozen `frame_code` probe, both sides | — | **landed 2026-11-23** | S8-5's gate |
| **S8-2** | P2, `--dump-plan` on a local reference build | an oracle build | **landed 2026-11-25** — `make uli-dump-plan`; the dumped shipped plan is the blob's plan on 257 of 257 `fast`-cell frames (and 129 of 129 `tiny` frames), both header cycle fields included | S8-6's attribution (its gate), and S8-7's diagnosis when a cell refuses |
| **S8-3** | P3, the in-tree plan/table diff (`make uli-plan-diff`) | — | **landed 2026-11-23** | S8-6, S8-7's progress signal |
| **S8-4** | P4, the `fast` cell (`make uli-accept ULI_ACCEPT_ONLY=fast`) | — | **landed 2026-11-25** | the progress signal for S8-5…S8-7 |
| **S8-4b** | the oracle of record: rebuilt contraction-free, its sha256 and fused-multiply map recorded (`make uli-oracle-check`) | — | **landed 2026-11-25** — sha256 `1991ee6c…`, **zero** fused sites | S8-5 items 1–3 arithmetic parity, and the S8 gate's MATCH claim |
| **S8-5** | the coder: distortion parity, tie parity, the sequence terms, the cycle model | S8-1, S8-4, S8-4b — all landed | **landed 2026-11-25** — items 1–4; the frozen differential is **identical on 24 of 24** | S8-6 |
| **S8-6** | the trainer, fed a *matching* pass-1 plan | S8-2, S8-5 | **landed 2026-11-25** — `make uli-train-diff`; the 5,212-byte table region is the reference's on the `fast` cell (257 frames) and `tiny` (129), from both the dumped stats and a re-coding of the pass-1 plan every 2nd frame; integer counts exact, `cen_s` at 6 ULP worst on `fast` and 0 on `tiny` | S8-7 |
| **S8-7** | the search: contiguous windows, initial training and retry | S8-0 … S8-6 | **landed 2026-11-26** — `make uli-accept` reports MATCH on `fast`, `tiny`, `biomaster`, `off-default` and `adv`; row 25 (the last 3-frame residual) is closed by the M/S-bit comparison fix | the S8 gate |

Two rules keep this list from producing dangling prerequisites, which is what happened to
S8-1…S8-4 while they were described only as prose in §7:

1. **A prerequisite is a row here, not a sentence elsewhere.** If a sub-step needs an
   instrument, a cell or an oracle, that thing has its own row and this table names it in
   `needs`. A row may be started only when every row in its `needs` is landed — S8-5 is the
   live example: its item 4 landed early because it needed nothing, and items 1–3 wait on
   S8-4b.
2. **A row is landed when its own gate has been measured, and the measurement is recorded**
   in the prose below and in the landing log. Partial rows say which items are done (S8-5),
   so "in progress" is never mistaken for "nearly finished" or for "not started".

**The premise.** The oracle is deterministic: R5b measured `--threads 1` and `--threads 8`
byte-identical on the fixture, and every S7 run reproduces its own hashes. So "we cannot
match its bytes" is not a property of the format or of the machine, and there is no reason
to accept a difference that is neither an improvement nor required.

**Amended 2026-11-25: "deterministic" means deterministic *for a given oracle binary*.**
The premise was read as a property of the encoder, and S8-4b measured that it is not:
the shipped oracle's float arithmetic depends on how its C was compiled, and it fuses
multiply-adds on the search's own decision path. Two consequences this section now carries:
the target is a *binary* whose sha256 and fused-multiply map are recorded (`make
uli-oracle-check`, inventory row 15), and every claim below is same-host by construction
(E-R5's float clause) rather than by caution. Nothing about determinism is lost — a fixed
binary at a fixed thread count reproduces its own bytes, which is all a target needs — but
"the reference's bytes" was never a well-formed phrase until the binary was named.

**The test a difference has to pass.** If this encoder can genuinely improve on the oracle,
the improvement shows up as SNR at equal rate. A non-exact blob is therefore acceptable
exactly when it is *better* — mean and worst SNR above the oracle's, at no more bytes.
**Not byte-exact and worse is not a stylistic difference; it is a deficiency.**

**That test governs S9, not S8, and the split is the point of this section.** S8's target is
exactness alone; the licence test is what lets an improvement back *in* once exactness
exists to measure it against (see "S9 — reinstating the improvements"). An S8 that also
carried the SNR arm would be able to swap one for the other mid-flight, which is the
failure mode this step exists to remove: S7's two favourable cells are consistent with the
deficiency being masked, so "we are ahead here" is not evidence until we can be identical
there.

**The current state fails that test.** Measured in S7: at `--cap 60` on half the budget ours
is **1,784 bytes larger for 0.19 dB less** SNR; the S5 measurement of the 12-track album
put us 0.41 dB behind (at `mu = 0`, never re-measured at the shipped default); and against
E-R7's shape on a two-track corpus we match the oracle's plain arm on the worst track while
sitting ~0.07 dB under its DAC-weighted one. The two cells where we are level or slightly
ahead (`biomaster` +0.11 dB full-band, `cross60` +0.01) are consistent with the deficiency
being masked by a favourable mix, not with it being absent. The working assumption is
therefore a real deficiency until a cell shows otherwise.

**1. The target, as a sentence.**

> **For every flag combination over the reference's shipped-path surface, with `--dacw 0`,
> our encoder emits a byte-identical blob.**

That is the whole of S8. Its machine expression is `make uli-accept` reporting MATCH on
every cell, and the record of the claim is the table in "S7" with every verdict changed.
Notes on the wording, because each one has been a source of argument:

- **The claim carries the `--dacw 0` qualifier and the qualifier never gets dropped.** The
  reference's *shipped default* is `--dacw 1` (a DAC water-level weighting over an
  8-iteration damped fixed point, `uli_enc.c:229-245`), which we do not implement. Exactness
  is therefore claimed on the reference's **non-default arm**, chosen because that arm is
  what our encoder has always been equivalent to — we have no `--dacw` flag at all — and
  because at `--dacw 0` the reference's own path is the smaller one: `weights_plain` only,
  window centre `g_guess(λ,w) − 7` with `ga == gb`, and no fixed-point iteration. Closing
  the default arm is S9's first line, priced in S7 (0.09 dB of worst track on a two-track
  corpus, 0 bytes on every single-track cell).
- **"Identical settings" means the reference's flag surface**, which is the only surface on
  which the sentence is well formed. Our extra flags must be removed or provably inert; the
  full partition is the first table below.
- **`--threads` needs no pinning.** R5b measured `--threads 1` and `--threads 8`
  byte-identical on the fixture, and S7 reproduced it; it is a setting that does not reach
  the bytes.
- **`--frames` is out of scope.** It truncates the album and rewrites the track table
  (`uli_enc.c:289-293`), so it is a different input, not a different setting.

**2. Which flags are settings.**

| flag | reference | ours today | decision for S8 |
|---|---|---|---|
| `--album`, `--tracks`, `--out`, `--budget`, `--cap`, `--musearch`, `--kappa`, `--threads` | yes | yes | **in the target**, same values both sides |
| `--dacw` | yes, default **1** | **absent** | **pinned to 0**; implementing 1 is S9. The oracle-only `dacw1` row stays in the matrix as a *recorded difference*, never an expectation |
| `--frames` | yes (test truncation) | absent | **out of scope** — it changes the input |
| `--stride`, `--fine` | **no equivalent** | yes, defaults `8`/`4` | **deleted at S8-0.** Not an improvement over the reference: it is a repair for a coarse grid we invented (see the inventory) |
| `--no-train` | no equivalent | yes | kept, **diagnostic only**, out of the target |
| `--g` (fixed step) | no equivalent | yes | kept, **diagnostic only**; this is the mode an A/B uses |
| `--wav`, `--emit-pcm` | no equivalent (`prep.py` is off-tree) | yes | kept; the target is defined on **identical PCM**, and E-R2 already says the resampler is not held to byte-equality |
| `--rd` | the reference always searches | yes, explicit | **on** for every S8 cell |

`--musearch` deserves its own line, because S6 left a 0.04 dB gap against the reference and
the obvious suspect was the μ search. It is not: our ladder (`rd.rs:210-250`) is the
reference's (`uli_enc.c:138-153`) — `hi = λ·0.01`, `×2.5` for at most 14 iterations, then
six geometric bisections (`sqrt(lo·hi)`, `hi·0.3` when `lo == 0`), returning `hi` as μ. The
S6 gap is the plan, not the price.

**3. What is already exact, and what that means for the work.**

Three of the five layers are proven, which is what makes S8 a reproduction of the *search*
rather than a rewrite of the codec:

| layer | status | witness |
|---|---|---|
| forward MDCT | bit-for-bit | `tests/mdct.rs` against `fixtures/mdct_ref_coef.f32` |
| default tables blob | byte-for-byte | `tests/tables_golden.rs` against `fixtures/uli2_default_tables.bin` (5,212 B, `15b477ed…96bcb0`) |
| per-band coding (`band_rdo`) | **2,400 frozen random cases**, magnitudes and `bits` **exactly**; distortion only to 1e-12 relative | `tests/quant.rs::band_rdo_matches_every_frozen_case`, `scripts/gen_uli_band_rdo_fixture.py` |
| container emit at a fixed plan | byte-for-byte | S1's gate; S7's diff shows strings and seek table matching in every cell |
| table trainer *algorithm* | faithful by construction | `train.rs:194` is `enc_core.c:13-32`: same two-minimum merge, same strict `<` tie-break, same `sqrt`-of-counts length-cap retry, same floor 0.2, same `n > 20` / `round(16·mean)` / clamp `−8..15` |

One line in that table is the whole of S8-5's first half: **our distortion is not bit-identical
to the oracle's.** The `band_rdo` test tolerates 1e-12 relative because we sum per tuple then
across tuples, where the oracle sums per tuple inline and recovers a frame's `D` as
`J − λ·(bits − 1)` (`enc_core.c:404`). At 4e11 that is a 1-ULP difference — which is nothing
until it lands on a `<` in the class Viterbi or in `allocate`, and then it is a different
plan, a different table set, and a DIFF that looks exactly like a search bug. It is retired
first for that reason.

**4. The divergence inventory.**

This is the authoritative list. Ordering is by the layer it lives in, so that "which of
these is left?" is answered by reading down the table. **Any new divergence is added here
before it is fixed**, so this list and the work cannot drift apart. Rust anchors are in this
tree; C anchors are the off-tree reference `../gba-audio/encoder/`.

| # | divergence | reference | ours | verdict |
|---|---|---|---|---|
| 1 | distortion *derivation* (not just accumulation order) | frame `D` derived as `J − λ·(bits−1)` from its Lagrangian (`enc_core.c:404`), where `J` sums 32 bands of `D + λ·bits` | a direct sum of squared reconstruction errors | **closed at S8-5 item 1 (2026-11-25)**: the band's `J` is accumulated per tuple (`BandRd::j`) and the frame's `D` is derived from it, and the frozen differential now reports `D` **bitwise on 24 of 24, worst 0 ULP**. It was never fixable by matching order — the fix was reproducing the derivation |
| 2 | EOB charge in the class Viterbi | `V + λ·len_cb[s][EOB] + μ·cyc + D_z[b]` (`enc_core.c:317`) | terminator bits **deliberately unpriced** (`quant.rs:823-825`) | **closed at S8-5 item 3 (2026-11-25)**: charged, and a zero-length EOB is not offered as a stop, which is the same rule the class symbols follow. Invisible on the identical default rows it was argued to be — the change is live on trained rows and pinned by `a_channels_bits_are_the_writers_terminator_included` |
| 3 | the `allz` guard | a band whose every `\|x\| < 0.4·D` may only take class 0 (`enc_core.c:301-307`) | **absent** — `grep -rn '0\.4'` in `crates/uli-enc/src` finds nothing | **closed at S8-5 item 3 (2026-11-25)**: in place, with `a_band_below_four_tenths_of_a_step_may_only_be_class_zero` constructing the decisive case (at `kappa = 0` only the guard keeps such a band silent) |
| 4 | class-Viterbi tie order | `bestEnd` scanned inside the band loop, strict `<`, then the end-at-last-band pass (`enc_core.c:313-326`) | mirrored shape; scan order now identical term for term, including `for s < K` on the end-at-last-band pass | **closed at S8-5 items 1–2 (2026-11-25)**: the instance is gone — frame 1 at `g=100 ms=1` now emits the reference's *bytes*, pinned as an FNV hash in `tests/frozen.rs::the_former_tie_is_the_references_bytes`, and the differential is identical on 24 of 24 |
| 5 | `g_guess` rounding | `lrint` — ties to **even** (`uli_enc.c:48`) | `round_ties_even` (S8-0) | **closed at S8-0.** It was `f64::round` — ties away from zero — and was documented as off the search path, which stops being true at S8-7; that is why it was fixed while the flag surface was being pruned rather than with the window |
| 6 | candidate window (pass 1) | `NG = 28` contiguous, `lo = g_guess(λ_train) − 7` (`uli_enc.c:326-339`) | the same contiguous 28-step window in `rd::Curves::compute` (S8-7) | **closed at S8-7 (2026-11-26)**: the pass-1 plan is byte-identical on the `fast` (257 frames) and `tiny` (129) cells, and its sliding-window rule is `rd::curve`'s |
| 7 | candidate window (pass 2) | `NG = 20` contiguous, `lo = g_pass1 − 9` (`uli_enc.c:349-354`) | the same contiguous 20-step window, centred on the pass-1 answer (S8-7) | **closed at S8-7**: the shipped plan matches on `fast` and `tiny`; `plan_diff` reports `g: every frame agrees` |
| 8 | pass-1 curves | computed with **`musearch = 0`** (`uli_enc.c:341`) | `run(tables, false)` — musearch off | **matches** |
| 9 | initial training | `train()`: analytic `g = g_guess(λ,w)`, **M/S forced to 1**, 4 outer iterations, each a 9-step λ bisection over `±3` in log, every **8th** frame, target `target/NF × 0.985` (`uli_enc.c:85-115,322`) | `train::initial_train`, the same shape (S8-7) | **closed at S8-7**: the four `train-itN` stat sets match P2's dump exactly (all counts and `cen_s`; the 33-frame sample), and the returned λ is the reference's |
| 10 | retrain sampling | every **2nd** frame (`uli_enc.c:349`) | `train::collect_frames` with every 2nd frame (S8-6), wired into the search at S8-7 | **closed at S8-7** — the retrained 5,212-byte table region matches the blob on `fast` and `tiny` |
| 11 | retry loop | 6 attempts; `tgt -= (size−budget)·8·1.02 + 2000` (`uli_enc.c:405`); re-allocates on the **same** curves | 6 attempts on the same curves, with the reference's size expression (S8-7) | **closed at S8-7**: on `fast` attempt 0 is refused at 20,016 bytes against 20,000 and attempt 1 is the reference's plan, byte for byte |
| 12 | `--stride`/`--fine` themselves | no such knob; the sweep is fixed by the window | **gone** (S8-0): the flags, the refine stage and the two `Args` fields are deleted, and the surviving spacing is a constant | **closed at S8-0** |
| 13 | container sizing arithmetic | `target_bits = (budget − overhead)·8 − 64`; the retry's `size` adds the actual regions and `hw` **without** the 16-byte target slack (`uli_enc.c:314-316,398`) | `target_bits` equal, but `Plan.needed` included the slack | **closed at S8-7**: `size_overhead = overhead − 16` is what `needed`/the retry use. Including the slack made the retry target 130.56 bits too small and five `fast` frames' steps differ; with the reference's size, attempt 0 is 20,016 and attempt 1 is identical |
| 14 | the cycle model: **`long_codes`** | the *encoder* never increments `ops.long_codes` — its decoder does (`uli_dec_ref.c:32`), so `uli_cycle_model` is called with that term at zero | `frame_ops` counts every code longer than `LUTB` (`rd.rs:295`) | **localised and matched 2026-11-23**: with the counters printed on both sides, **eight of the nine agreed on every one of the 24 codings**, and the ninth was `long_codes` — ours 26/5/5/7 against the reference's **0/0/0/0** on the four frames at `g=100 ms=0`, which is `40 ×` each frame's cycle delta to the unit. The reference's encoder omits a term its own decoder charges, so `rd::frame_ops` no longer counts it (S8-5 item 4, done): the differential now reads **`cycles` same on 23 of 24**, and the one residual is the equal-cost tie's `escs` (one escape, `45` cycles) — the tie, not the model |
| 15 | **float contraction in the oracle binary** | not a source difference: GCC's default `-ffp-contract=fast` fused 35 multiply-adds at `-O2 -march=native`, **13 of them on the decision path** (`frame_code` 6, `band_rdo` 3, `curves_compute` 2, `allocate` 2) | Rust does not fuse (`fmul`+`fadd`) | **closed at S8-4b (2026-11-25)**: the oracle of record is `sha256 1991ee6c…`, built `-ffp-contract=off -march=armv8-a`, with **zero** fused sites, so parity is attainable by construction. It was **not fixable in Rust**, and it was not cosmetic: the pre-rebuild binary's 24/24 bit agreement on the default tables was luck. `make uli-oracle-check` fails on a different binary or a fused decision path |
| 16 | **`allocate`'s λ bisection totals** | `reduction(+:tot)` over frames inside a `#pragma omp parallel for` (`uli_enc.c:188`), compared to `target` in `f64` | a frame-index-order sum (R5b) | **closed at S8-7 on the generated cells**: the oracle and this encoder each emit the same blob at `--threads 1` and `--threads 4` on `fast` (`sha256 2f21f6…`), and ours matches the oracle at both, so the two totals land on the same side of every probe there |
| 17 | **the oracle is ISA-specific** | `CFLAGS ?= -O2 -march=native` (`encoder/Makefile:10`) | n/a | **the claim's boundary, not a defect**: exactness is same-host, same-binary (E-R5's float clause). The oracle of record pins `-march=armv8-a`, and the S8 gate names its sha256, which `make uli-oracle-check` prints |
| 18 | **the frozen goldens came from a fusing build** | `scripts/gen_uli_band_rdo_fixture.py` compiles with `-O2 -std=gnu11 -w` — GCC's default contraction, i.e. the *old* oracle's flags | our Rust does not fuse, and the contraction-free build of the same C agrees with the committed `D` column only within **2 ULP** (all 2,400 cases: 48 at 1, 6 at 2; every magnitude and bit count exact) | **closed at S8-5 item 1 (2026-11-25)**: `fixtures/band_rdo_clean.txt` is the additional golden (`make uli-band-rdo-fixture-clean`) and our `D` is **bitwise** against it on all 2,400 cases; the fused file stays as the pin it was: the committed fixture is a pin and must not move, so step 1's bitwise-`D` gate needs an **additional** golden from the clean oracle. Measured 2026-11-25; it is the same `D += e*e` fusion as row 15, which is why the frozen-plan differential's worst gap fell 103 → 76 ULP when the probe moved to the clean flags |

| 19 | **context rows with zero counts** | `tables_from_stats` calls `huff_lengths` for every `st->cb[a]` unconditionally (`enc_core.c:82-84`), so an unobserved context gets the balanced 0.2-floor code | `train::build` rebuilt only rows with a positive count, keeping the base/previous row | **closed at S8-6 (2026-11-25)**: rebuilt unconditionally. Measured from P2's dumped stats, **21 bytes of `len_cb`** differed (contexts 9 and 11 on `fast`); **0** after |
| 20 | **the trainer's EOB charge** | `frame_code` charges `cb[ctx][EOB]` only when `endB < nb` (`enc_core.c:385-391`) — a full-band channel gives the reader no EOB to read | `train::collect` charged it for every channel | **closed at S8-6 (2026-11-25)**: guarded on `bands.len() < NBANDS`, matching `frame::write_chan`. Measured against the dumped `retrain` stats: **10 extra terminators in context 1 and 9 in context 2** on `fast` before the fix; counts exact after |
| 21 | **the stats reduction order (`cen_s`)** | `count_frames` accumulates into thread-local `EncStats` and `stats_add`s them in thread order (`uli_enc.c:75-83`), so the sum's order is the OpenMP chunk assignment | a frame-order sum in `collect_frames` | **diagnostic, not a table divergence (S8-6)**: on `fast` (4 threads) the retrain's 12 `cen_s` values differ, worst **6 ULP**; every integer count and `cen_n` are exact, and the consumer `round(16·cen_s/cen_n)` yields the same `dq` and the same table region. `tiny` (`--threads 1`): 0 ULP. Same class as row 16; the corpus cells are the remaining confirmation |
| 22 | **curve storage precision** | `cv.R`, `cv.D`, `cv.MU` are `float`; the allocation and final encode read those narrowed values (`uli_enc.c:157,269`) | `f64` curve vectors, so a near-tie could resolve differently | **closed at S8-7**: `Curve.r/d/mu` are `f32`, and the allocation promotes them to `f64` exactly where C does. Measured: the pass-1 plan differed on 29 of 257 `fast` frames before, **0** after; the shipped plan differed on 5, **0** after |
| 23 | **the M/S header bit in the allocation's rate** | `cv.R = (float)r.bits`, and `frame_code`'s `bits` starts at 1 for the M/S flag (`enc_core.c:401`) | `measure` reports bands only, so the curve's rate was one bit/frame low | **closed at S8-7 and extended at row 25**: the bit is in the curve's rate (`(point.bits + 1) as f32`) **and** in `measure_g`'s per-`g` M/S comparison (`b.D + lam*(b.bits + 1)`); with rows 22 and 24, every matrix cell matches |
| 24 | **the header cycle fields** | `H.est_max/mean_cycles` are `cv.C` at the chosen candidates (`uli_enc.c:451-452`) | patched from the verification decode's re-measured ops | **closed at S8-7**: the header is patched from the final curve/pick (`193,984 / 165,214` on `fast`), and the verification decode remains a diagnostic. On `fast` the old path read `165,160`; it is now the blob's |
| 25 | **the `biomaster` pass-1 plan at three frames** | `curves_compute` chooses the M/S domain with `b.D + lam*b.bits`, where `frame_code`'s `bits` starts at 1 for the M/S flag (`enc_core.c:326-339`) | `measure_g` compared `b.D + lam*bands`; the `+1` cancelled algebraically but not in floating rounding | **closed 2026-11-26**: the bit is now in `measure_g`'s comparison. At frames 13, 16,525 and 16,545 of `biomaster`, the rounded `lam*(bands+1)` vs `lam*bands` had flipped the domain and then the step; after the fix `biomaster` is `sha256 49eb6a11…`, and `make uli-accept` MATCHes every cell it runs |



Rows 1-4 are the coder, 5-13 the search and its constants, 15-17 the oracle's own float
surface — a layer the inventory did not have until S8-4b measured it, and the one layer this
tree cannot fix by editing Rust. Row 8 and the trainer's merge/`dq` algorithm in section 3's
last row are places we are already right — stated so that a later diff is not attributed to
them. Note the distinction that makes row 9 a different claim: the trainer's *algorithm*
matches; the *plan it is handed* does not. **Rows 19-21 were added by S8-6**, from the first
run of a matching plan through the trainer: 19 and 20 are trainer divergences that the table
region did not happen to expose on `fast` (they are fixed anyway, because they are wrong), and
21 is a named diagnostic whose consumer rounds. **Rows 22-24 were added by S8-7**, from the
search: 22 and 23 are the curve's representation and its M/S header bit, and 24 is which
measurement the header's cycle fields carry. Row 13 also moved from "to confirm" to closed.

**Rows 15-17, added 2026-11-25, and the reason the inventory needed a layer.** All three are
properties of the oracle *binary* rather than of its logic, and all three were invisible
until `make uli-oracle-check` printed the map. They are the argument for S8-4b: a divergence
you cannot reproduce is not attributable, and "our search differs" would have absorbed them
one at a time as a mystery.

- **Row 15 is the first-order one.** 13 of the oracle's 35 fused multiply-add sites are on
  the decision path. Every rule in this inventory about matching `J`, `D`, `bits` and
  `cycles` presupposes both sides evaluate the same expression the same way; with the oracle
  fusing and Rust not, they do not, and no amount of search reproduction closes that. The
  remedy is the oracle build, not our code.
- **Row 16 is narrower and survives the rebuild.** `allocate`'s λ bisection compares an
  `f64` total against the target, and that total is an OpenMP `reduction(+:tot)` over
  frames — associated by thread, not by frame index. R5b's `--threads 1` vs `8` agreement is
  evidence that no straddle happened on the fixture, not proof that none can. Ours sums in
  frame order (R5a/R5b), so the two totals are not the same double; it only bites if the
  comparison lands within an ULP. *To verify at S8-7* by running the oracle at two thread
  counts on the `fast` cell and checking the bisection's outcome agrees.
- **Row 17 is the claim's boundary, stated as a row so it cannot be forgotten in the gate
  wording.** `-march=native` makes the oracle ISA-specific, and `g_guess` crosses `log2`
  (row 5's neighbourhood). Exactness is therefore a same-host, same-binary claim (E-R5's
  float clause). This is not a defect to fix; it is what "byte-identical" means here, and
  the S8 gate says so.

**First attribution, measured (P3, 2026-11-23).** On `biomaster` — ours 3,293,696 B
(`f4f3a52a…`, byte-identical to S6's pin, which is how we know the instrument measures the
shipped encoder) against the reference's 3,293,048 B (`990da225…`) — `make uli-plan-diff`
reads both containers and decodes every frame's declared plan. It reports:

- **the plan diverges at frame 0**: ours declares `g = 8` where the reference declares
  `135`. That is row 6's predicted effect observed directly — a frame with nothing worth
  coding, where our full-range candidate set can take the tie-break to `G_MIN` and the
  reference's window around its pass-1 guess cannot reach it;
- across the album **7,059 of 16,553 frames differ in `g`** (widest `|Δg| = 127`, at frame
  0) and **725 differ in the M/S flag** (first at frame 4); the searched ranges are
  `8..119` against `89..135`, the same pair S7's seek diagnostic reported from a
  sample every 256 frames;
- the tables differ in **1,971 of 5,212 bytes, starting at byte 43 = `len_cb[0][0]`
  (2 vs 1)**, of which `len_p` is 1,616 bytes, `len_cb` 154, `len_q` 128 and `dq` 73.

That settles the branch this section could only name: **the counts differ, so the trainer is
not the first cause and the tables are downstream.** It does not yet say *which* search
change is first — the candidate window, the EOB charge and the `allz` guard all move counts —
which is what the frozen-plan differential (S8-5) settles.

**The frozen-plan differential, first run (2026-11-23).** With the plan held still, the
question narrows from "does the search differ" to "does the *coder* differ", and the answer
on the default tables is mostly no. The committed 4-frame coefficient golden at
`g ∈ {60,100,140} × ms ∈ {0,1}` — 24 codings, `mu = 0`:

| field | agreement | note |
|---|---|---|
| `bits` | **24 of 24 identical** | the coder emits the reference's exact bit count at a fixed `(g, ms)` |
| `halfwords` | 24 of 24 | same stream length everywhere |
| `stream` | **23 of 24 identical** | the one difference is frame 1 at `g=100 ms=1`: same 6,334 bits, different content — an equal-cost alternative (inventory row 4) |
| `cycles` | 8 of 24 | the 8 agreements are exactly the frames that code nothing; every coded frame reads **higher** for us, and printing the counters named why: `long_codes` (inventory row 14) |
| `D` | 3 of 24 bitwise | worst 103 ULP, and **not fixable by matching order** — the reference derives `D` (inventory row 1) |

**These numbers are the fusing probe's, and S8-4b re-took them.** With
`scripts/uli_probe.py` on the oracle of record's flags (`-ffp-contract=off -march=armv8-a`)
the frame differential reads: `bits` and `halfwords` 24 of 24, `stream` and `cycles` differing
on the same single tie frame, and **`D` bitwise on 2 of 24 with a worst gap of 76 ULP instead
of 103** — so contraction was ~27 ULP at the worst frame and the derivation (`D` derived, not
summed) is the rest. The table below is kept as the 2026-11-23 arm, not as the current
reading; nothing in its *conclusions* moved.

Three things this changes about the plan. **The coder is not the first cause either**: at a
frozen plan with the default tables it reproduces the reference's bits on every case and its
stream on all but a tie. So S8-5's sequence-term work (rows 2 and 3) is now expected to be
inert on the default tables — which is what the plan claimed — and its remaining content is
**row 14's cycle model**, because `--cap` is derived from it and a systematically high reading
admits fewer candidates. **`D` cannot be compared bitwise**, so S8-5 step 1's gate is
amended: compare `D` within a stated ULP bound and record the derivation, rather than
"tighten the test to compare D bitwise" — that target was unattainable by construction and
this measurement says so before the work started. And the equal-cost stream difference is the
first *instance* row 4 has ever had, so its tie-break check now has a case to reproduce.

**5. Why the blob is not enough, and the four instruments that fix it.**

Training is a pure function of the counts and errors it is handed, so a byte diff cannot
separate "the trainer differs" from "the plan feeding it differs". The obvious experiment —
compare the plan and the symbol counts — **cannot be run against the shipped artefact**: a
blob carries the plan that was *emitted* and the table set that was *trained on the previous
pass's* plan. The pass-1 plan, which is the trainer's actual input, exists only inside the
reference process. That single fact is why S8 needs instruments before it needs edits, and it
is where the earlier version of this section was wrong.

Four instruments, none of them on a gate path (E-R1 forbids the dependency, not the oracle —
the posture the tables/MDCT/`band_rdo` goldens already take):

**P1 — the `frame_code` probe.** `scripts/uli_probe.py`, generating a C probe into a temp
dir that `#include`s `enc_core.c` and drives `frame_code` at a **frozen** `(g, ms)` with a
**given** table set, printing bits, the magnitude vector, ops, cycles and the derived `D`.
This is the like-for-like A/B that S3 relied on, and its instrument has been lost:
`scripts/uli_rd_ab.py`'s own docstring points at a fixed-step A/B in `tools/rd_ab`, and there
is no `tools/` directory in this tree — so S8-5's gate had nothing to run until P1 landed
(2026-11-23). That was the gap P1 fills, and it fills it as a probe over **frozen** `(g, ms, tables)`, which is
what S8-5 actually needs (the whole point is to hold the plan still and vary nothing).
It is built the way the two working generators are built: Python in `scripts/`, C written to
a temp dir at run time, compiled against `$GBA_AUDIO`, no C in this repository and no
build-time dependency. *Gate:* it reproduces all 2,400 `band_rdo` cases, proving the harness
itself before it is trusted.

**P1 is two-sided, and that half is what makes it a differential.** The probe alone can only
say what the oracle does. `uli_enc::frozen::code_at` (with `crates/uli-enc/src/bin/frame-dump.rs`)
is the same coding from this crate at the same frozen `(g, ms, mu, tables)`, in the same units
— one `rd::measure` call, then a `write_frame` of exactly what it measured, so a difference
between *measuring* and *emitting* cannot hide. `scripts/uli_frame_diff.py`
(`make uli-frame-diff`) runs both on one corpus and reports per field, because the fields
mean different things under disagreement: `bits`/`stream` are emitted, `D` is measured,
`cycles` is a model of the decoder.

**P2 — `--dump-plan`.** A local build of the reference with a flag that writes per-frame
`(g, ms, mu, bits, D)` for **both** passes, plus the per-class symbol counts that
`tables_from_stats` consumed. *Gate:* the dumped **shipped** plan equals the plan recovered
by decoding that run's own blob — the instrument checked against the artefact, so a wrong
dump cannot masquerade as a finding.

**P3 — in-tree plan and table diff.** Expose `g` and `ms` on `FrameInfo`
(`crates/uli-lite/src/frame.rs:91`; the decoder already tracks `g` at `:335` — it is simply
not surfaced), and add a field-level report: which table rows differ and in which entries,
which frame's `(g, ms)` diverges first, and the per-class symbol-count delta. This replaces
S7's "2,204 of 5,212 bytes differ" with a cause. It works on **any** blob, ours or the
oracle's, with no reference build. *Gate:* on `biomaster` it names the first divergent frame
and the first divergent field.

**P4 — the fast cell.** See below. *Gate:* it encodes in seconds on both sides and the
budget binds.

**6. The fast cell.**

S7's matrix is ~41 minutes on four cores, which is not an iteration loop. But the reason to
build a small cell is stronger than speed: **truncating the album changes the trained
tables**, so a small cell is not a shortcut to a big cell's answer, it is a cell in its own
right — and it can be a *better* one, because S7 records that `tiny`'s budget does not bind
and its mismatch is therefore largely tie-breaking.

The cell is **`fast`** — **landed 2026-11-25 (S8-4)**: a 4 s, two-track, 257-frame album
given as its own `album.f32` + `tracks.tsv` pair, generated by `scripts/gen_uli_fast_cell.py`
at a fixed seed, at `--budget 20000 --cap 74`, both encoders invoked with identical flags.
Rationale for files rather than `--frames`: `--frames` rewrites the track table, and a
purpose-built pair keeps the cell comparable to the other cells' contract (E-R2) instead of
making it a special case in the harness. Measured: the reference 2.4 s, ours 12.2 s, and the
budget binds on both sides (19,976 vs 19,904 bytes against ~5,412 of overhead) with the cap
binding on 19 of 257 frames. `make uli-accept ULI_ACCEPT_ONLY=fast` runs it alone and needs
no corpus, which is what makes it an iteration loop rather than a matrix.

Roles: `fast` is the **progress signal** during S8-5…S8-7 and the first PASS/FAIL gate;
the four S7 cells stay the **confirmation**. During S8-5…S8-7 the cell runs *unpinned* and
the signal is the diff (first divergent frame, first divergent table field), not a hash —
the hashes are expected to differ while the work is in flight. It is pinned at S8's gate.

**7. The sub-steps.**

Nine rows — S8-0 … S8-7 plus **S8-4b** — and the table at the head of this section is their
order, their dependencies and their status. **They are sub-steps of S8, not new steps**: the
step namespace stays `S1 … S8` (S3b set the precedent for a labelled sub-part of a step, and
S8-4b is the second use of it), and no sub-step is a place you can be *instead of* S8. What
each row is *for* is in its `needs` and `unblocks` columns rather than in a class named here,
because naming classes ("prerequisites", "deletion, any time before S8-7") is what let S8-1…
S8-4 float outside the sequence while two of them were already landed. The one thing kept
from that formulation: **S8-1 … S8-4b measure or pin and change no encoding behaviour**; only
S8-0, S8-5 and S8-7 change bytes. Each sub-step lands as its own commit with its own gate
recorded, and a partial row says which items are done.

**S8-0 — prune the flag surface — landed 2026-11-25.** `--stride`/`--fine` and the
coarse-then-refine split they parameterised are **deleted**: `rd.rs::refine_curve` and
`Curves::refine` are gone, `Curves` no longer carries a stride, the refine-on-retry branch
in `lib.rs` is gone, `g_guess` rounds ties-to-even (`rd.rs` → `round_ties_even`, which is
`lrint`'s rule and inventory row 5), and `RETRY_ATTEMPTS` is 6 (`uli_enc.c:405`). `Args`
lost the two fields, so a caller that still passes either flag gets `unknown argument`
instead of a silently different grid. The `rd.rs` module docs and the crate README that
advertised the two-stage sweep are rewritten, because after the deletion they were false.

*What replaced the grid, and why not `stride = 1`.* What survives is one uniform sweep at a
**constant** spacing (`rd::CANDIDATE_STRIDE = 8`) with no flag and no refinement — the
minimum deletion, because the grid's *shape* is S8-7's (rows 6 and 7 replace it with the
reference's contiguous 28- and 20-step windows). Sweeping `stride = 1` was tried and
rejected: it can only lower distortion, so it would be an output claim in a sub-step that
makes none (decision 3), and it costs ~8x on **every** album encode in the tree — measured
on the `fast` cell at four threads, 12.2 s → 82 s against the oracle's 2.4 s, which would
turn `make uli-accept`'s ~41-minute matrix into hours before S8-7 lands. **S8-7 then
removed the constant with the sweep**; the reference's 28- and 20-step windows replace it
(rows 6 and 7).

*Gate, measured:* `make uli-check` green; `make uli-test` green with **93 tests in 17
suites**, of which the uli-enc half is the whole encoder suite. One test moved rather than
broke: `tests/train.rs`'s binding budget went 8,000 → 6,600 bytes, because 8,000 is the
*least* rate-limited budget that fits that 4-frame album and its trained-vs-built-in gap had
shrunk to +0.43 dB against the test's own 0.25 dB bar; at 6,600 (309 B/frame, the
product-like regime) the same claim measures **+1.11 dB**. The threshold did not move and the
measured table is in the test. The `refining_the_grid_never_costs_quality` test went with the
grid it measured: its 2.6 dB claim was against our own coarse baseline, it stays in
`RECORD.md`, and it returns at S9 as a labelled arm. **No output claim is made**, as the
sub-step required: bytes changed and nothing improved.

**S8-1 — P1, the probe — landed 2026-11-23.** `scripts/uli_probe.py` + `make uli-probe`: a C
probe generated into a temp dir that `#include`s the reference's `enc_core.c` and drives
`frame_code` at a frozen `(g, ms, mu, tables)` and `band_rdo` at frozen per-band inputs. It
takes a **binary f32 coefficient corpus** (`--coef`, `2·M` floats per frame — the layout
`fixtures/mdct_ref_coef.f32` already has, and `--pcm`/`--dump-coef` derives one with the
reference's own `mdct_frame`), a **tables blob** (`--tables`, so a *trained* set can be coded
with), and emits integers only: `bits`, `D` as hex f64 bits, the reader cycle estimate, and
the frame's **bit stream** as hex halfwords with the `g` delta from `--g-prev` — which is what
"byte-identical at a frozen plan" means, and what S8-5's gate will diff. `--stats` writes the
sparse per-class symbol counts for S8-6.

*Gate, measured:* `make uli-probe` reproduces all **2,400** frozen `band_rdo` cases **byte for
byte**, re-derives the committed MDCT coefficient golden from `fixtures/mdct_in.f32`, and
shows frame mode deterministic and unchanged by `--stats`.

**The Rust half landed 2026-11-23** (`uli_enc::frozen` + `src/bin/frame-dump.rs`, paired by
`scripts/uli_frame_diff.py` / `make uli-frame-diff`), because a one-sided probe can only say
what the oracle does. `code_at` is one `rd::measure` and one `write_frame` of exactly what it
measured, so a divergence between measuring and emitting cannot hide, and `DefaultTables::from_blob`
makes a *trained* set an input (it round-trips the oracle's committed default blob, which is
the witness that its layout reading is the format's and not its own). The four tests in
`tests/frozen.rs` pin the frame-level **bit counts** to the reference's measured values — the
one piece of frame-level ground truth this tree has — and `tests/tables_golden.rs` pins
`from_blob` against the oracle's bytes. Two loader checks, both run: the
committed default-tables blob codes **identically** to `tables_default` (it is that table set),
and the trained tables sliced out of `uli2_tiny.uli` code **differently** (5,805 → 5,956 bits
on frame 0 at `g=100`), so `--tables` is a real input rather than a no-op.

Deliberately **not** printed by frame mode: per-band magnitudes. `band_rdo` returns them, but
the per-band *choice* is made inside `chan_code`, which is static and reaches its inputs
through `frame_code`'s preamble; reproducing that preamble in the probe would be a second
implementation of arithmetic that must mean one thing, and a drift in it would be silent. The
magnitudes are reported at the level where they are defined (band-rdo mode), and any
magnitude difference shows up in the stream.

**S8-2 — P2, `--dump-plan` — landed 2026-11-25.** `scripts/uli_dump_plan.py` +
`make uli-dump-plan`: a copy of the reference's own `uli_enc.c`, patched in a temp directory at
run time (no C in the repository, the `uli_probe.py` posture), built with the oracle of
record's float flags, and given `--dump-plan FILE`. It writes the **pass-1** plan before
`retrain` sees it, every **pass-2** attempt before it is encoded and sized (so the last one is
the shipped plan), and every `EncStats` set `tables_from_stats` consumed — the four
`train-itN` rounds and the `retrain` whose counts produce the blob's table region. Per frame
it records `g`, `ms`, `mu`, `bits`, `D` and the cycle estimate; counts and distortions are hex
`f64` so a comparison is bitwise rather than through a printed decimal. The dump also names
the oracle path, the compile flags and the full `argv` that produced it, because "the
reference's plan" was never a well-formed phrase until the binary was named (S8-4b). Since
row 25 it also writes the **pass-1/pass-2 λ, the per-frame `weights_plain` values and the slid
window starts** (`lambda`, `weight`, `g0`); those three comparisons ruled out the λ solution,
the weights and a window/slide difference before the M/S bit was found. The plan gate's parser
skips the extra lines, and `train-dump` ignores them.

The blob half is `crates/uli-lite/examples/plan_dump.rs`: a plain decode of one blob (the plan
is a g delta and a flag bit, so it cannot be read off the container), printing the header, the
seek entries and one `frame F g G ms M ...` line per frame.

*Gate, measured 2026-11-25:* the dumped **shipped** plan equals the plan recovered from that
run's own blob — `g` and `ms` on **257 of 257** `fast`-cell frames, with `est_max_cycles`
193,984 and `est_mean_cycles` 165,214 equal to the blob's, and both seek entries' `g_prev`
equal. Re-run on the generated `tiny` fixture (`--budget 300000 --threads 1`): **129 of 129**
frames, header `193,977` / `181,822`, one seek entry. The instrument also asserts the sampling
the reference uses — four `train` rounds of `ceil(n/8)` frames and one `retrain` of
`ceil(n/2)` — so a section silently missing from the dump fails rather than being noticed
later as a short file. After a green it **proves the comparison bites**: copies with one plan
step, one header cycle field and one seek entry each moved by one must all be refused, so a
gate that had stopped comparing cannot report OK (the same self-check habit `uli_probe.py`
uses). The patch asserts every anchor occurs exactly once and refuses otherwise: if the
reference moves, the dump would describe the wrong site, and the instrument is not allowed to
guess.

*What the gate is and is not.* It proves the dump is the artefact's plan; it says nothing about
whether the plan is right, which is S8-6's and S8-7's question. It also does not compare a
per-frame `bits`/`D` against the blob, because the blob carries neither — those columns are
for the trainer/search differential, not for this gate.

**S8-3 — P3, in-tree instrumentation — landed 2026-11-23.** `FrameInfo` now carries the
frame's declared `g` (`crates/uli-lite/src/frame.rs`; `ms` was already surfaced as
`Ops::ms`), both decoders fill it, and `crates/uli-lite/examples/plan_diff.rs`
(`make uli-plan-diff A=… B=…`) diffs two blobs: the tables **by field** — `field_at(k, off)`
derives the format's layout from `k`, so a differing byte is reported as
`len_cb[4][9]: 5 vs 4` rather than as an offset — and the plan **by frame**, with the first
divergent frame and step for `g` and for `ms`, the counts, the widest `|Δg|`, and both
searched ranges. It needs no reference build, so it works on any pair, and it exits 2 on a
difference (the `uli_accept.py` convention).

*Gate, measured on `biomaster`:* it names **frame 0** as the first divergent frame (ours
`g = 8`, reference `135`) and **`len_cb[0][0]` at byte 43** as the first divergent table
field. The numbers and what they settle are under "First attribution" above. A new test
(`reference_corpus.rs::the_declared_step_is_the_pinned_plan_and_stays_in_the_readers_window`)
holds the field to something it did not produce: the fixture's declared plan must read
19..124, the range S7 measured from the seek diagnostic, and every frame's `g - 4·ms` must
be inside the reader's window.

**S8-4 — P4, the fast cell — landed 2026-11-25.** `scripts/gen_uli_fast_cell.py` +
`make uli-accept ULI_ACCEPT_ONLY=fast`: a generated 4 s, two-track, 257-frame album given as
its own `album.f32` + `tracks.tsv` pair (never a truncated big album — that has different
trained tables, and `--frames` rewrites the track table, which is a different input rather
than a different setting). It is the matrix's first cell and the tests' progress signal.

*Gate, measured at four threads:* **the reference 2.4 s, ours 12.2 s** — seconds, and the
budget binds on both sides: at `--budget 20000 --cap 74` the oracle writes 19,976 bytes and
ours 19,904 against ~5,412 bytes of container overhead, ~454 bits/frame, and the cap binds
too (the μ re-code ran on 19 of 257 frames), so one cell exercises the budget, the cap and
the trained tables. The harness gained `--only` (comma-separated or repeated) and each
cell now requires its own inputs, so the fast cell runs on a checkout with no generated
corpus at all; `--list` names the cells, and `dacw1` pulls in its tiny input.

*First finding, recorded before it was explained:* on the fast cell the **first content
difference is the track table** (2 of 48 bytes, offset 124), which no S7 cell showed. It is
`TrackEntry::g_prev` on track 1 — ours 128, the reference 119 — and it is **downstream, not
a new divergence**: `container.rs` writes `frames[preroll_frame].g_prev`, which is frame
127's step, so the two values differ because our frame 127's step differs. The 128 coinciding
with the preroll frame's own index is a coincidence of this cell, and the writer's index is
right (the reference's `e.preroll_frame ? g[e.preroll_frame - 1] : 0` is the same frame).
The cell's plan diff is the row-6 story again, compressed into 15 seconds.

**S8-4b — the oracle of record.** Byte-exactness is a claim about a *binary*, and the shipped
one's float arithmetic is not reproducible from Rust. `make uli-oracle-check`
(`scripts/check_uli_oracle.py`) reads the off-tree binary and prints three things: its
sha256, the flags its Makefile claims (`CFLAGS ?= -O2 -march=native -std=gnu11 ...`, so
the oracle is ISA-specific), and its fused-multiply map, function by function.

*Measured 2026-11-25 on this aarch64 box:* **35 fused multiply-add sites in ten functions**
(31 scalar `fmadd`, 3 `fmla`, 1 mixed), and **13 of them on the decision path** —
`frame_code` 6, `band_rdo` 3, `curves_compute._omp_fn.0` 2, `allocate._omp_fn.0` 1,
`allocate._omp_fn.1` 1. Those are the coder, the per-band price, the curve pass and the
Lagrangian that picks the plan: values that are compared, branched on and tie-broken in
`f64`, where one ULP is a different plan. Rust does not fuse (`a*b + c` is `fmul`+`fadd` on
the pinned toolchain), so arithmetic parity with this binary **cannot follow from
construction**; the frozen-plan differential's 24/24 agreement on the default tables is
agreement that happened, not parity that is guaranteed. The MDCT's 3 sites are the benign
case (`mdct_frame` narrows to `f32` at `enc_core.c:186`, so a one-ULP double difference
almost never survives), and `main`/`proxy_report`/`now` only print or time.

*Taken the same day — this is the landed record, not a plan.* The rebuild:

```sh
cp "$GBA_AUDIO/build/uli_enc" /tmp/uli_enc.native-fma-71df29fe   # keep the old one
rm -f "$GBA_AUDIO/build/uli_enc"
make -C "$GBA_AUDIO/encoder" CFLAGS='-O2 -std=gnu11 -Wall -Wno-unused-function \
  -ffp-contract=off -march=armv8-a' "$GBA_AUDIO/build/uli_enc"
make uli-oracle-check          # --expect clean is the default now; exits 0
```

**The oracle of record is `sha256 1991ee6c2e9f039cd2bc974abcd193f793c147cfdfd040cba2fd2d009b50ae6b`,
75,944 bytes, zero fused multiply-add sites anywhere** (was 75,544 B / `71df29fe…` / 35 sites).
The superseded binary is kept as the `native` profile in the instrument — `make
uli-oracle-check ULI_ORACLE_EXPECT=native --ref <that file>` still reports the map it had —
because a tree that still holds it should be told what it is, not only that it is wrong. The
instrument now fails on a sha mismatch as well as on a map mismatch (`--allow-rebuild`
tolerates the sha for a deliberate rebuild that will re-record it), because a measurement
taken against a different binary is not a measurement of this oracle.

*The probe had to move with it, and that was a real defect.* `scripts/uli_probe.py` compiled
the reference's C with `CFLAGS ?= -O2 -std=gnu11 -w` — GCC's default contraction, i.e. the
flags of the *old* oracle. A probe that fuses compared against a binary that does not is not
a differential; it is two arithmetics. It now builds with `-ffp-contract=off -march=armv8-a`
and `PROBE_CFLAGS=` as the explicit override, and `make uli-probe`'s own gate caught the
change rather than hiding it.

*What that gate found is inventory row 18.* The committed `band_rdo` golden was generated by
the **fusing** build (`scripts/gen_uli_band_rdo_fixture.py`'s defaults, which are kept so a
regeneration still reproduces the committed bytes). Against it, the contraction-free probe
agrees on **every magnitude and every bit count** across all 2,400 cases and differs only in
`D` — the one field computed as `D += e*e`, which GCC fuses into `fma(e, e, D)` — by **at
most 2 ULP** (48 cases at 1, 6 at 2, measured 2026-11-25). So the probe's gate is now
"everything exact, `D` within 2 ULP, and the tolerance names the reason"; a golden from the
clean oracle would make it 0, and that is S8-5's decision (an *additional* fixture — the
committed one is a pin and must not move).

*The frozen-plan differential, re-taken against the clean probe.* `bits` and `halfwords`
identical on 24 of 24, `stream` and `cycles` differing on the same single tie frame, and
**`D` bitwise on 2 of 24 with a worst gap of 76 ULP — down from 103**. That is the useful
number: contraction was worth ~27 ULP at the worst frame and **the derivation is the rest**,
which is exactly what S8-5 step 1's amendment claims. It also means the earlier "3 of 24
bitwise" was measured against a fusing probe and is superseded rather than wrong.

*Re-taken cells.* The `fast` cell is still a DIFF but its **byte counts now agree** (ours
19,744 = the oracle's 19,744, where the fusing oracle wrote 19,976); the tables differ in
1,986 of 5,212 bytes and the first content difference is still the track table's `g_prev`,
i.e. downstream of the plan. `tiny` is unchanged in size (36,916 vs 39,308) with the first
content difference in the tables. No cell matches, and none was expected to: the plan still
diverges at the low end of the `g` range (fast cell: ours 56..136, the reference 108..141),
which is inventory row 6 and S8-7's work.

*Why not match the fusion in Rust instead.* `f64::mul_add` would reproduce a fused expression
exactly, but only at the sites GCC happened to fuse — a compiler-version artifact, not a
documented property of the reference. That would make our arithmetic a function of someone
else's optimiser decisions and would break on the next oracle rebuild. Turning contraction
off is one flag, is stable across compilers, and leaves the oracle's *source* untouched.

**S8-5 — the coder: FP and tie parity, then the sequence terms.** In this order, and the
order matters:

1. **Distortion parity (inventory #1) — restated twice, and the second restatement is the
target.** The original gate said "compare `D` bitwise"; the 2026-11-23 measurement restated
it as "compare `D` on ULP distance", because the differential showed 103 ULP on a frame whose
coding matches. **That restatement was a diagnostic mistaken for a target, and this is the
amendment (2026-11-25).** `D` is not a report field: it lands on a `<` in the class Viterbi
and in `allocate`, so a 103-ULP difference is a latent plan divergence that no bound can
retire — and row 1's own text says where it comes from. The reference does not *sum* its
distortion, it *derives* it as `J − λ·(bits − 1)` (`enc_core.c:404`), where `J` sums 32 bands
of `D + λ·bits`. The gap is a derivation mismatch, not accumulation noise, so reproducing the
derivation — the same per-tuple inline accumulation and the same fixed-order `J` reduction
(R5b) — makes `D` bitwise equal, which is what the Viterbi needs. *Gate:* `band_rdo`'s
magnitudes and `bits` stay exact (they already are, 2,400 cases) and `D` is **bitwise**
equal to the reference's on that corpus; a ULP-distance assertion may remain in the test only
as a labelled diagnostic with the reason written down. Frame-level reading stays with `make
uli-frame-diff`. Note the ordering this creates: row 15 had to be closed first, because on a
fusing oracle a "bitwise `D`" gate would be measuring the compiler — and it is closed
(S8-4b). **Row 18 is the remaining prerequisite, and it was not visible before the rebuild**:
the committed `band_rdo` golden is a *fused* build's answer, so no implementation of ours can
match its `D` bitwise. Step 1 therefore also owes an **additional** golden generated by the
clean oracle (`scripts/gen_uli_band_rdo_fixture.py` with `CFLAGS=` carrying
`-ffp-contract=off -march=armv8-a`), kept beside the committed one rather than replacing it,
and the bitwise gate runs against that. Measured 2026-11-25: the clean build of the same C
differs from the committed `D` column by at most 2 ULP and in nothing else.
2. **Tie parity (inventory #4).** Assert the Viterbi's tie winner explicitly, as R5d already
requires of the search (lower `g`, then L/R over M/S): here, the lowest band endpoint and
the lowest class index. *Gate:* a test that constructs the tie and names the winner.
3. **The sequence terms (inventory #2, #3).** Restore the EOB charge `λ·len_cb[s][EOB]` and
the `allz` `0.4·D` guard. *Measured expectation for the default tables, 2026-11-23:* the
frozen-plan differential already reproduces the reference's bits on all 24 cases and its
stream on 23 of 24, so these two rows should be **inert** on default tables — as the plan
claimed and as `quant.rs`'s comment argues. They are restored for the **trained** rows, and
the gate below is what proves both halves: unchanged at default, exact at trained. Note that S3's record says we dropped the charge *because* it
agreed better with the oracle at `g = 140` on the default tables; the S6 review already
established that the reference charges it, so the omission was right by accident on identical
rows and is wrong on trained ones. This is where that correction is made. *Gate:* through P1,
over a probe corpus that includes over-cap frames (so the μ path is exercised), our emitted
bit stream is **byte-identical** for frozen `(g, ms, tables)`.
4. **The cycle model (inventory #14) — done 2026-11-23.** `frame_ops` leaves `long_codes`
at zero. The reference's *encoder* never increments it:
the counter exists in `UliOps` for the decoder, which charges it (`uli_dec_ref.c:32`), and
`frame_code` passes `uli_cycle_model` an ops struct whose `long_codes` is therefore always
zero. Ours counted it, so every coded frame read high by exactly `40 × long_codes` and
`--cap` admitted fewer candidates than the reference's. Dropping it in the **encoder's**
counting is the matching change; the product decoder's model stays honest, because that path
decodes real streams and its long codes are real. *Gate, measured:* `make uli-frame-diff`
reports `cycles` **same on 23 of 24** and `long_codes` **same on 24 of 24**; the residual is
the tie frame, where the coding genuinely differs and `escs` moves by one. `tests/frozen.rs`
now pins the reference's cycle estimate for seven `(frame, g, ms)` cases, so the model cannot
drift back.

Two corrections come from the album-level run (`biomaster`, same flags as S7's cell), and both
are worth having *before* the search work rather than after:

- **The header cycle fields cannot match until the plan does.** `est_max_cycles` is the max
  over *the chosen plan*, so it is a function of the model **and** the plan. Before the fix
  ours read `193986/178159` against the reference's `193986/177949` — and the max matched by
  coincidence, because our inflated per-frame estimate happened to peak on the same frame.
  After the fix it reads `194487/178154`: the mean is 5 closer, the max 501 further, because
  admitting the candidates we used to exclude let some frame spend more. So the fix's gate is
  the **per-frame** agreement above, and the header fields become a *consequence* of S8-7.
- **The count of divergent frames is not a distance metric.** `g` differs on **7,059** frames
  before the fix and **7,140** after; `ms` on 725 and 722. The fix is right (the model now
  matches, and the album is 0.02 dB better at 32 bytes more), but "more frames now agree" was
  never the right way to read it: a different admitted set moves choices in both directions,
  and near-ties resolve either way. The metric that matters is S8-7's, not this one.

**Items 1-3 landed 2026-11-25, and the frozen coder is now the reference's.** What changed in
`quant.rs` and `rd.rs`: the band's Lagrangian is accumulated **per tuple**, `d_tuple + λ·bits`,
as `J += best` does, and carried as `BandRd::j`; the frame's `D` is *derived* as
`J − λ·bits` (our bit count excludes the M/S bit the reference's starts at, so its `bits − 1`
is ours); the class Viterbi adds its terms in the reference's order (`V + λ·l + μ·CY + Jb`),
charges the terminator and skips a zero-length EOB; the `allz` guard is in place; and each
band's `dist` is re-accumulated in coefficient order for the band-level diagnostic the
`band_rdo` fixtures compare.

*Gates, all measured:*

| instrument | before | after |
|---|---|---|
| `make uli-frame-diff` (24 codings) | bits 24/24, stream 23/24, cycles 23/24, `D` 2/24 bitwise (worst 76 ULP) | **identical on 24 of 24: bits, stream, cycles, all nine counters and `D` (worst 0 ULP)** |
| `tests/quant.rs` vs `fixtures/band_rdo_clean.txt` | fixture did not exist | **2,400 cases bitwise, `D` included** |
| `tests/quant.rs` vs `fixtures/band_rdo.txt` (fused pin) | `D` within 1e-12 relative | magnitudes and bits exact, `D` within the measured **2 ULP** — row 18's evidence, kept as a pin |
| `tests/frozen.rs` | `D` unpinned; the tie case excluded | `D` **bitwise** pinned on 7 cases, and `the_former_tie_is_the_references_bytes` pins frame 1 at `g=100 ms=1` as an FNV hash of the reference's 397 halfwords |
| `tests/quant.rs` (new) | — | `a_band_below_four_tenths_of_a_step_may_only_be_class_zero` (row 3) and `a_channels_bits_are_the_writers_terminator_included` (row 2) construct their cases rather than reading about them |

The suite is **97 tests in 17 suites**, all green, and `make uli-probe` still reproduces the
2,400 frozen cases (now with `D` at ≤2 ULP against the *fused* golden, which is the same
statement from the probe's side).

*What this settles for S8-6 and S8-7.* The coder is no longer a source of divergence at a
frozen plan, so anything a cell still disagrees about is the **plan** (rows 5–13) — which is
what S8-6 and S8-7 exist to reproduce. It also removes the last excuse for a tuned tolerance:
`D` is compared in the class Viterbi and in `allocate`, so a ULP there was a latent plan
divergence, and the tests now assert equality rather than a bound in both places.

**S8-6 — the trainer — landed 2026-11-25.** `crates/uli-enc/src/bin/train-dump.rs` +
`make uli-train-diff`: P2's dump supplies the four `train-itN` stat sets (which build the
initial tables), the `retrain` set, and the pass-1 plan. The binary reconstructs the initial
tables with `train::build` over the train rounds, then checks the table region two ways:
**(a)** `train::build` from the dumped `retrain` stats, which isolates the table construction;
**(b)** code the dumped pass-1 plan ourselves — every second frame, at its `(g, ms)`, with the
initial tables — then `train::collect_frames` + `train::build`, which adds the coder and the
collector to the chain. Both run in under a second after the reference's 2.6 s dump.

*Gate, measured 2026-11-25:* the **5,212-byte table region, `dq` included, is byte-identical**
to the reference blob's on S8-4's `fast` cell and on the generated `tiny` fixture, from both
(a) and (b). The integer counts (`cb`, `q`, `p`, `cen_n`) match the dumped `retrain` stats
exactly, and the sampled frame count is 129 of 257 / 65 of 129. `cen_s` is reported, not
asserted: it is a sum whose reference-side order is `count_frames`' thread-local
`EncStats`/`stats_add` chunking (row 21), and its consumer is `round(16·cen_s/cen_n)`.
Measured `fast` (4 threads): 12 values differ, worst **6 ULP**, `dq` and the table region
identical; `tiny` (`--threads 1`): **0 ULP**. The shipped encoder's own fast-cell blob is
byte-identical to the pre-change one (the change is inert on our S4 plan path), and the
acceptance cell remains DIFF at 19,780 vs 19,744 B with 2,081 table bytes differing — S8-7's
work, not this row's.

*Two trainer divergences closed, both found by the gate rather than by reading:*
1. **A zero-count context row was kept, not rebuilt** (row 19). The reference calls
   `huff_lengths` for every `st->cb[a]` unconditionally; `train::build` rebuilt only rows with
   a positive count. Measured from the dumped stats: **21 bytes of `len_cb`** differed before
   the fix (contexts 9 and 11, never observed in `fast`'s retrain sample), **0 after**.
2. **The terminator was charged on a full-band channel** (row 20). `frame_code` adds
   `cb[ctx][EOB]` only when `endB < nb`; `collect` added it for every channel. Measured
   against the dumped `retrain` stats: **10 extra terminators in context 1 and 9 in context
   2** on `fast`, exactly the channels that code all 32 bands. The counts are now exact.

Row 21 (the `cen_s` reduction order) is recorded as a named diagnostic, not a defect to fix
by matching an OpenMP schedule: the reference's own chunk assignment is not a stable order,
and its only consumer rounds. It is to be confirmed at S8-7 on all cells, where a half-integer
mean could flip a `dq`.

*What this does not claim, and what remains.* The initial tables are reconstructed from P2's
*dumped* train stats, not from a Rust reproduction of the shape that produced them — row 9's
analytic allocation (`g = g_guess`, M/S forced 1, four iterations, every 4th frame, target
×0.985) is S8-7's, as its row says (`#9·shape`). What S8-6 closes is row 10's sampling
(`collect_frames`) and the trainer's table construction. `make uli-train-diff` is the
instrument; S8-7 wires this trainer into the search's windows and retry.

**S8-7 — the search — landed on the generated cells 2026-11-26.** The encoder now runs the
search in this order: `train::initial_train` (analytic `g_guess`, M/S forced 1,
four iterations, nine λ probes over ±3 in log, every **8th** frame, target `×0.985`) →
`Curves::compute` over the contiguous 28-step pass-1 window centred on `g_guess(λ_train,w) − 7`
→ allocate → retrain on every 2nd frame (S8-6's `collect_frames` + `build`) → the contiguous
20-step pass-2 window centred on `g_pass1 − 9` → six attempts on the same curves → emit. The
window **slides coarser** when its last candidate is over the cap, so a dense frame can
still reach an admissible step. Curve samples are `f32` because they are search measurements
compared at their recorded precision. The header's cycle fields come from the plan's chosen
candidates, and the retry's size excludes the 16-byte target slack.

*Gate, measured 2026-11-26:* **the `fast` and `tiny` cells are byte-identical to the oracle** —
`fast` 19,744 B `sha256 2f21f6d7…` (257 frames, `--budget 20000 --cap 74 --threads 4`), `tiny`
39,308 B `c398c2d2…` (129 frames, `--budget 300000 --threads 1`) — from `make uli-accept
ULI_ACCEPT_ONLY=fast,tiny`, exit 0. `make uli-plan-diff` reports `g: every frame agrees`,
`ms: every frame agrees`, tables identical, both header cycle fields equal; the pass-1 plans
match exactly (257/129 frames). `--threads 1` and `--threads 4` produce the same bytes on both
sides (row 16). The shipped encoder's own suite is green (18 suites / 99 tests), and
`make uli-check` is clean.

*What is measured now.* After the corpus was generated with `make uli-album-asset` and
`make uli-adversarial-asset`, `make uli-accept` runs **`fast`, `tiny`, `biomaster`,
`off-default` and `adv`** and reports **MATCH on every one**, including `biomaster` at
3,293,128 B `49eb6a11…` (`--budget 3294000 --cap 74 --threads 8`). Row 25's last three-frame
residual was the M/S header bit in `measure_g`'s domain comparison; with it added the cell is
exact. `cross60` is not run while `adv` is present — the fourth slot is deliberately one
corpus. The row is landed; `make uli-oracle-check` names `1991ee6c…` and the pin ledger was
discharged on 2026-11-27; every cell MATCHes.

*What the last attribution found, in order.* Four changes were needed after the trainer and
coder were already exact, and each is an inventory row: the curve arrays had to be `float`
(row 22), the allocation's rate needed the M/S header bit `frame_code` starts at (row 23), the
header's cycle fields are `cv.C` and not the verify decode (row 24), and the retry's size
excludes the 16-byte target slack (row 13). The first two changed the plan; row 24 changed only the header's cycle fields; row 13 turned
the last five differing steps into zero. The plan's earlier "every 4th frame" for
`train()` was a transcription error — `train(w, target, 4, 8, …)` is four iterations and the
8th frame — and the dumped `train-itN` stats settle it: our sequential frame-order `cen_s`
matched the oracle's on the 33-frame samples exactly, which is why row 21's ULP gap is a
retrain-only diagnostic.

**S8 gate (the step's exit).** Every cell of `make uli-accept` reports MATCH with `--dacw 0`
and the `dacw1` row is still a recorded difference; P1/P2/P3 and `make uli-train-diff` all
still pass; the pin ledger below is discharged; and the S7 section keeps its DIFF table with a
superseded note rather than a deletion (nothing in this record is deleted).

*Where the gate stands on 2026-11-27.* The corpus was generated by `make uli-album-asset` and
`make uli-adversarial-asset`; `make uli-accept` reports **MATCH on every cell it runs —
`fast`, `tiny`, `biomaster`, `off-default` and `adv`** — with the oracle of record's sha256.
`biomaster` is exact after row 25's fix; `cross60` is not run while `adv` is present. The pin
ledger was discharged on 2026-11-27 (item 9 below), so S8 is closed. `cross60` remains an
unexercised fourth-slot alternative, not an open code difference: the row-25 fix is corpus-
independent, and the cell is re-run when the `cross` corpus is next generated.

The additions from 2026-11-25 and S8-7, so the gate names what it actually claims:

- **the oracle binary is named.** `make uli-oracle-check` exits 0 — the default expectation
  is now the `clean` profile, so it checks both that no fused multiply-add is on the decision
  path and that the binary **is** `sha256 1991ee6c…` — and that sha256 is recorded with the
  MATCH table, because "byte-identical to the reference" is a claim about that artifact (rows
  15-18). A deliberate rebuild re-records it; nothing else may.
- **the fast cell is a gate cell**, not only a progress signal: `make uli-accept
  ULI_ACCEPT_ONLY=fast` must MATCH, and it is the first thing to check after any change to
  the search.
- **rows 16 and 17 are discharged**: the λ-total question is answered by measurement —
  `--threads 1` and `--threads 4` give the same blob on both sides on `fast` — and the gate
  text says the claim is same-host, same-binary.
- **`make uli-test` still passes with its test floor**, because S8-0 through S8-7 change
  bytes and the tree's own suite is what says they changed only what they claim.

**8. Decisions this step settles.**

Stated so that no sub-step relitigates them.

1. **The target is `--dacw 0`.** Reason: we have no DAC objective, that arm is the smaller
  reference path, and it is the arm our encoder has always been. Exactness at the shipped
  default is S9's first line, with S7's price attached.
2. **The SNR licence test moves to S9.** S8's only test is exactness. A cell we currently
  lead is not evidence of anything until it is reproduced.
3. **The improvements leave the shipped path until exactness lands.** `--stride`/`--fine`
  are deleted, not defaulted off, so they cannot silently re-enter as "obviously better" and
  re-break the gate. Their measured 2.6 dB was against our own coarse baseline, not against
  the reference, which is exactly the measurement error S9 must not repeat. **Closed
  2026-11-25 at S8-0**; the one shape constant that survived (`rd::CANDIDATE_STRIDE`) had an
  owner (S8-7) and no flag, and S8-7 removed it with the sweep.
4. **FP parity is retired first, not last**, because a 1-ULP tie flip is indistinguishable
  from a search bug and would poison every later attribution. The existing 2,400-case fixture
  is the instrument; it only needs its tolerance removed. **Amended 2026-11-25, twice, and
  both amendments were forced by measurement:**
  - **Parity is measured against a named binary, so the oracle is pinned like a flag
    (S8-4b).** It is rebuilt with `-ffp-contract=off` and a fixed `-march`, and its sha256 is
    recorded with the gate's MATCH table. The shipped binary fuses 13 multiply-adds on the
    decision path and Rust fuses none (row 15), so exactness against it was never attainable
    by construction. Reproducing GCC's fusion in Rust with `mul_add` is rejected: it makes our
    arithmetic a function of someone else's optimiser decisions, and it breaks on the next
    oracle rebuild.
  - **`D` parity means bitwise, arrived at by reproducing the derivation (S8-5 step 1).** A
    ULP bound is a diagnostic, not a target: `D` is compared in the class Viterbi and in
    `allocate`, so a bound leaves a latent plan divergence in place. The 103 ULP measured on
    2026-11-23 comes from the reference *deriving* `D` as `J − λ·(bits − 1)`
    (`enc_core.c:404`) where we sum it directly, so it is closable by construction.
5. **Tie-break parity is an explicit check, not an emergent property** — inventory #4, with a
  test that constructs the tie.
6. **The blob is not evidence about the trainer.** P2 exists because the pass-1 plan is not
  in the artefact; without it, S8-6 would be blind.
7. **The instruments live where E-R1 allows.** P1 and P2 are off-tree generators/oracle
  builds in the `scripts/gen_uli_*` posture — a Python script that writes C into a temp dir.
  No C enters this repository.
8. **The fast cell is a cell, not a shortcut.** It is pinned at S8's gate and its own reason
  for existing is that the big cells cannot be iterated on and `tiny` cannot diagnose.

**9. The pin ledger — discharged 2026-11-27.**

S8-7 changed every byte we emit, so the pins were handled once at the gate. The resolution of
each row is recorded here; the frozen fixture rows were verified by hash and the acceptance
rows by the MATCH run.

| artefact | source | effect | resolution (2026-11-27) |
|---|---|---|---|
| `crates/uli-enc/tests/fixtures/{uli2_default_tables.bin,mdct_ref_coef.f32,band_rdo.txt}` | the **reference**, frozen | **must not move.** If one does, the regeneration is the bug (the fixtures README already says this) | **unchanged.** `uli2_default_tables.bin 15b477ed…96bcb0`, `mdct_ref_coef.f32 db7f8fc8…37bfc`, `band_rdo.txt da98f5d6…48e7f`; `git status` clean |
| `crates/uli-lite/tests/fixtures/uli2_tiny.uli` | the **C encoder** (S7 decision 2) | **must not move.** It is a decoder pin, not an encoder expectation | **unchanged**, `68dca43a…cb756d`, 39,308 B |
| in-repo determinism tests | our own two runs | comparison is run-to-run, not fixed values; they need `Args` fields removed at S8-0 and no re-pin | **no re-pin.** `make uli-test` is 18 suites / 99 tests green; `--threads 1` and `--threads 4` give the same bytes on `fast` |
| `make uli-accept` cell hashes and the S7 table | our encoder vs the oracle | **re-taken at the gate**, with MATCH and the new hashes recorded in place | **re-taken.** §S7 carries the MATCH table and full SHA-256s for `fast`, `tiny`, `biomaster`, `off-default` and `adv` |
| S5's album numbers (33,291,896 B, `69911924…e656b8`, 31.75/28.56 dB) and the `cross` residual | our encoder | **superseded, not edited.** They stay as the S5 arm with a dated note saying S8's exactness target replaces them; they are no longer the current figure | **dated note added in §S5**; the S5 arm is retained as the record of that measurement |
| `make uli-player-cross`'s 33,088,280 B and ALBUM-PLAYER row 5's 167,232 periods | oracle blob, ROM-scaled | **re-measured once at the gate** if the blob moves; the ROM figure is a downstream claim and must follow, not lead | **not an S8-7 pin**: the `cross` asset is produced by the oracle, not by our encoder. The 2026-11-16 figure predates the clean-oracle rebuild (S8-4b), so it is flagged for re-measurement when the `cross` corpus is next generated. Tracked in **W2**, not S8 |

The S8 pin ledger is therefore closed: `make uli-oracle-check` names `1991ee6c…`, the
five-cell MATCH run is recorded in §S7, and no frozen fixture moved.

**10. S9 — reinstating the improvements.**

Named here so S8's deletions are not permanent by accident and not undone by habit.

S9 owns: the reference's default `--dacw 1` arm (the DAC water-level objective and its damped
fixed point), and every search-side idea S8-0 and S8-7 removed — the fine grid, the full-range
sweep, stacked training, whatever survives. Each returns **as a labelled arm, off by default,
admissible only by the licence test: mean *and* worst SNR above the oracle's at no more bytes,
measured against the reference and not against a previous version of ourselves.** Exactness
stays the gate that says the arm changed only what it claims to change.

**11. Explicitly out of scope for S8.**

- `--dacw 1` (S9), `--frames`, the WAV resampler's output (E-R2 already scopes it out, and the
  target is defined on identical PCM).
- Anything about decode speed, entropy kernels or the product reader — untouched.
- Reproducing the reference's **stderr** (log text, timings, proxy reports). Not bytes.

**12. Residual risks, and why none of them blocks the plan.**

1. **The FP surface is larger than the 2,400 cases cover.** They cover `band_rdo`; they do not
  cover the Viterbi or `allocate`. Mitigation is S8-5 step 1 plus P1 at frame level; if a
  residual appears later it is an inventory row, not a mystery. E-R5's float clause already
  says exactness is claimed on the reference host wherever a float is crossed, and `g_guess`
  crosses `log2`, so exactness is a same-host claim by construction — that is a statement
  about the claim, not a defect in it.
2. **P2's dump could be wrong.** Guarded by cross-checking against the blob (its own gate), so
  the instrument is validated by the artefact it explains.
3. **A divergence exists that this inventory does not have.** Expected; that is why P1/P3
  report *causes* rather than counts, and why the inventory is the first thing a new finding
  touches.
4. **The work may not converge, and the honest output would then be a bound.** If a cell
  refuses to match after S8-5…S8-7, the deliverable is the inventory row it died on, the
  attributed diff, and a re-scoped claim — not a silent return to the SNR arm.

#### S7 — the acceptance matrix (2026-11-21); superseded by S8's MATCH (2026-11-27)

**Three things S7 was not.** It was not a gate: the harness calls the off-tree reference
binary, which E-R1 keeps off `make uli-test`, so `make uli-accept` sits on no gate path
by construction — it is a host measurement to take when the search or the container
changes. It was not an encoder change: not one line of `rd.rs`, `quant.rs`, `train.rs`,
`plan.rs` or `frame.rs` moved, and the run reproduces S6's pins exactly, which is the
evidence that the harness measures the shipped encoder. And it was not the quality
question: every SNR it prints on our side is full-band time-domain, while E-R7's 32.29
mean / 25.78 worst dB is a codec-only proxy (`gba-audio/docs/phase2/1-codec.md:12`,
measured by `eval/batch.py --dac off`), so nothing in this section may be quoted
against E-R7.

**What it is.** `make uli-accept` → `scripts/uli_accept.py`: our `uli-enc --rd` and the
reference on identical PCM and identical flags, comparing the two blobs byte for byte.
Per cell it prints MATCH or DIFF, both sizes and full SHA-256s, the two header cycle
fields at offsets 68 and 72 (raw and percent of 262,144), the oracle's codec proxy, our
mean **and worst** time-domain SNR, and the searched `g` range. A DIFF is localised to a
container region: the first differing byte, the first byte at or after `tracks_off` that
differs, per-region counts, and — when the stream differs — how many seek entries move
`g_prev` and by how much, plus the first five whose bit position or `g_prev` disagrees.
Exit 2 is DIFF, 1 is a harness error; `--list` prints the cells and runs nothing.

**The corpus is generated, not committed.** `make uli-album-asset` turns the tracked
`assets/sound/bioluminescent.wav` (96 kHz / 24-bit) through the sibling's `prep.py` into
`examples/uli_bench/assets/album.f32` + `tracks.tsv`, then the reference blob/decode/pin;
`make uli-adversarial-asset` generates `assets/adv/`, and `make uli-cross-asset` the 12-track
`assets/cross/`. The full recipe and the gitignore division are in
[`../../ULI2.md`](../../ULI2.md) -> "The measurement corpus — how `examples/uli_bench/assets`
is generated and used".

**Re-taken at S8's gate (2026-11-27).** The matrix now reports MATCH on every cell it runs,
against the oracle of record (`sha256 1991ee6c…`, zero fused multiply-add sites). `fast` and
`tiny` are generated in the harness, so they need no corpus; `cross60` is not run while `adv`
is present. This is the current answer:

| cell | flags | bytes | SHA-256 (both sides) | est max @68 | est mean @72 | verdict |
|---|---|---:|---|---:|---:|---|
| `fast` | `--budget 20000 --cap 74 --threads 4` | 19,744 | `2f21f6d713ab25ade49c030f808738791fa3a50d5a4a3d1cb0df420583061e7c` | 193,984 | 165,214 | **MATCH** |
| `tiny` | `--budget 300000 --threads 1` | 39,308 | `c398c2d2f0d56ea7bb890a998b85ec595d87bbee46d9f76ae9b71ffb93f3fa06` | 193,977 | 181,822 | **MATCH** |
| `biomaster` | `--budget 3294000 --cap 74 --threads 8` | 3,293,128 | `49eb6a11da105b1d944263e5fe3c20f54db570897ed5510eeedbe5daf2abee58` | 193,986 | 177,900 | **MATCH** |
| `off-default` | `--budget 1647000 --cap 60 --threads 8` | 75,572 | `809bd7be7ca9d0b47f7b686e44404c7d865abac313a9229fe1744bfcc7d5ae85` | 157,285 | 157,230 | **MATCH** |
| `adv` | `--budget 127500 --cap 74 --threads 4` | 127,380 | `1a0c88566ef5faed1be4875221a59a08fda21be1e8d99a059d0064b0511a41f8` | 193,986 | 177,402 | **MATCH** |

The `--dacw 1` row remains oracle-only and still reports 39,308 B / `c398c2d2…` on `tiny`,
identical to `--dacw 0` there: the DAC-weight objective is S9's, not an expectation here.

**The original S7 answer (2026-11-21), kept as the record of the DIFF:** the four cells,
measured by
`make uli-accept ULI_ACCEPT_ASSETS=/home/sam/Documents/coding/ulc-gba/gba-sound-player/examples/uli_bench/assets`,
every oracle invocation at `--dacw 0`:

| cell | flags | ours B | ref B | ours SHA-256 | ref SHA-256 | est max @68 ours/ref | est mean @72 ours/ref | verdict |
|---|---|---:|---:|---|---|---|---|---|
| `tiny` | `--budget 300000 --threads 1` | 39,124 | 39,308 | `366673df6d5550a8d2d07af0d317a7ec476ce7ae998e820f0ae5a93f82f32051` | `68dca43a36dd9c61dead3d2409d91906a2eae3ed86ce42d850f2022101cb756d` | 193985 / 193977 | 180881 / 182108 | DIFF |
| `biomaster` | `--budget 3294000 --cap 74 --threads 8` | 3,293,696 | 3,293,048 | `f4f3a52aaa58299eb7feb2a6b574117f8eecdb972d4577d7d944f1d302e095c8` | `990da225efc18621dd5689fd378fc183c5f79e8b9f21c4ff580e4225df7f8a73` | 193986 / 193986 | 178159 / 177949 | DIFF |
| `cross60` | `--budget 764500 --cap 74 --threads 4` | 764,220 | 764,196 | `ad96e8b0aa6b4d1a9a5c8cd6686406b3b719aa83c7edb8f6a1d19c9da5874988` | `4fa22172cb421d41e5f4b5d8b51c49fc522b8c4bd6ec2f13a06c909c6617a3fe` | 193986 / 193986 | 176561 / 176876 | DIFF |
| `off-default` | `--budget 1647000 --cap 60 --threads 8` | 73,584 | 71,800 | `cf4f55620fa716848e3bee3e8dcf00eb0359fac40dbb71370e22f6e5669788bd` | `c8b94024a0d8e61fce3a0872cbd97e7746ce8e1d1fce5197dea33270350e9b5a` | 157285 / 157285 | 157191 / 157216 | DIFF |

`cross60` is a 60 s head slice of the 12-track album as one 1,966,080-sample track;
the `adv` cell replaces it only when `adv/album.f32` exists (it does not in this
corpus, so `cross60` ran). The whole matrix is **~41 min** on four cores: the second
run's log spans 2026-11-21 16:46:26 → 17:27:06 and the first run's 15:24:03 → 16:05:08
(`stat` birth/mtime of `/tmp/pi-s7-accept{,2}.log`). The ~48 min this section carried
earlier was never measured.

**Every cell DIFFs, and the divergence is one layer below the container.** In no cell
did the blob match. The header differs only in its derived size and cycle fields —
offset 64 (`stream_halfwords`) and offset 72 (mean cycles) in every cell, plus offset
68 (`est_max_cycles`) on `tiny`, whose two totals round differently (193,985 vs
193,977). The harness counts bytes rather than naming offsets, and the count differs
with that: `header 5 of 88 bytes differ` on `tiny`, `3 of 88` on each of the other
three. The tables region diverges next, one count per cell, all out of 5,212 bytes:
`tiny` 2,626, `biomaster` 1,971, `cross60` 2,204, `off-default` 2,369. The harness's
"first content difference" line — the first byte at or after `tracks_off` that
disagrees — lands inside the tables in every cell: 156 on `tiny`, 155 on `biomaster`,
158 on `cross60`, 155 on `off-default`; past that the two streams run out of step, not
out of a trailing byte. The harness prints the geometry it compared — `biomaster`
`stream geometry differs: 5864+3287832 vs 5864+3287184` — and on `biomaster` some
12,889 of the 3,287,184 stream bytes compared are equal (3,277,088 differing bytes
overall, less the 648-byte length delta and the 2,145 header/table/seek bytes that
differ), so the streams are neither a copy nor unrelated.

The seek diagnostic says where that drift starts, and its delta is **`ref − ours`**:
at frame 256 the reference's bit position is **2,256 bits ahead of ours** on `biomaster`
(ours 172,479 vs ref 174,735) and 1,795 ahead on `cross60` (25,402 vs 27,197), while on
`off-default` **ours is 1,144 bits ahead** (8,023 vs 6,879). Where it reports entries,
`g_prev` barely moves — 30 of `biomaster`'s 65 entries (max |delta| 3) and 6 of
`cross60`'s 16 (max 2) — so the sampled step sequence is nearly the reference's. On
`off-default` both move hard: 56 of 65 entries change `g_prev`, max |delta| **64**. But
the divergence is **localised, not attributed**. The `tiny` cell looks like a
counter-example to the step-choice story until its seek table is counted: 129 frames at
`SEEK_INTERVAL` 256 gives **one** entry, at frame 0, bit position 0 — so its "every entry
agrees" is the *first* frame's `g`, not a step-sequence comparison, and its two streams
still differ by 184 bytes (33,768 against 33,952). Where the entries do diverge the
per-frame step choice is one candidate, not a demonstrated cause — the trained tables,
trained on those choices, sit downstream of whatever it is.

**What does not look like a tie-break is the candidate window.** The chosen `g` ranges are
`8..123` against the reference's `19..124` on `tiny`, and `8..119` against `89..135` on
`biomaster`. Our candidate set is the whole policy range `G_MIN..G_MAX`; the reference's
is a moving ~20-step window around its pass-1 allocation (`cv.NG = 20`, `lo = g[f] - 9`).
The two therefore search *different sets* on any frame whose coarse guess is off, and on a
frame where nothing is worth coding ours can take the tie-break to `G_MIN` where the
reference's window cannot reach it. That is a policy difference the matrix can name, and
it is consistent with the tables diverging first, because the tables are built from the
symbol counts of those very choices. It is a **lead, not a cause**: no run here varied the
window to see what it would buy.

**The EOB-pricing hypothesis is neither confirmed nor refuted, and it is no longer the
first thing to explain.** Its prediction — omitting the charge makes terminating look
cheaper here than to the reference, so *we* spend fewer bits and the reference's bit
position runs ahead — holds on both cap-74 cells and reverses on the cap-60 one. That
cell moves **two** parameters at once (the cap falls to 60 *and* the budget halves to
1,647,000 B, 2.06 dB of SNR against the other cells' 28–32), so it cannot isolate the
cap, and the reversal is as consistent with the rate as with the cap. More decisive than
either: the trained tables differ first in **every** cell, and the tables are trained on
the plan's own choices, so a table divergence is upstream of any per-frame bit-position
argument and sufficient on its own to explain the streams. Whatever fixes
exactness starts at the table trainer, not at the terminator. The honest state of the
DIFF is **unattributed**, and no search change was made to force it.

*Superseded 2026-11-23 by S8-3, in place rather than deleted.* The paragraph above reads
"whatever fixes exactness starts at the table trainer", and that is the one thing the
attribution has since ruled out: on `biomaster` the **plan** differs at frame 0 and 7,059
frames differ in `g`, so the counts fed to the trainer differ and the tables are downstream
of the search. See "First attribution" under §S8. What S7 got right, and this step kept, is
that a byte diff could not say so — it took an instrument that reads the plan.

**What the SNR columns say, and where they hurt.** `biomaster` (one track, so mean and
worst coincide): ours **31.89 mean / 31.89 worst** dB, int8 30.89; the oracle's codec
proxy 31.87 (min 31.87) and its full-band worst 31.78. `cross60`: ours 28.81 / 28.81,
oracle proxy 28.83, oracle family worst 28.80. `tiny`: 21.26 / 21.26 against a 21.67
proxy. The sharpest cell is `off-default`: at `--cap 60` ours is **1,784 bytes larger
and 0.19 dB below** the oracle's full-band 2.25 dB (ours 2.06). The worst-track figure
is the new `verify: worst` line — min over the per-track full-band time-domain SNRs,
asserted equal to `min(per-track)` by a test that also asserts the two per-track ratios
differ and that min is **strictly** below the mean, so a tie cannot pass it — and it is
not E-R7's worst track.

**A multi-track measurement, because every cell above is one track.** The mean and the
worst coincide in all four cells by construction, so E-R7's *worst* was still unmeasured —
and the `--dacw` pin was untested too, since its weights are constant within one track. A
3,200-frame corpus cut from the 12-track album — 30 s of track 01 (samples 0..983,040)
followed by 20 s of track 06 (28,253,515..28,908,875), `tracks.tsv` as two tracks — at
`--budget 637000 --cap 74 --threads 4` measures:

| | pooled codec | mean of tracks (pcm16) | worst track (pcm16) | oracle worst, codec proxy |
|---|---:|---:|---:|---:|
| ours, no DAC weighting | 33.30 | 33.74 | 33.21 | — |
| oracle `--dacw 0` | 33.17 | — | — | 33.28 |
| oracle `--dacw 1` (its default) | 33.27 | — | — | **33.37** |

Three things this settles. **`--dacw` is not inert on multi-track input**: the two oracle
arms differ from byte 65, and the pass trades 0.04 dB of mean proxy (33.64 → 33.60) for
**0.09 dB of worst track** (33.28 → 33.37) — which is precisely the objective E-R7's
second number is stated in. That is also the shape of what we lack: on the mean we sit
with the plain arm and about 0.2 dB ahead of it once the proxy/full-band offset is allowed
for (the proxy reads ~0.09 dB high — 31.87 against 31.78 on `biomaster`), but on the
**worst** track we match `--dacw 0` and sit ~0.07 dB below the DAC-weighted arm. The
matrix's real gap is therefore the one this paragraph works around by hand: every cell is
single-track, and a multi-track cell — not a larger budget on the same track — is what it
needs next.

**Four decisions this step settles.**

1. **`--dacw 0` on every oracle invocation**, because that is `alloc_full`'s
   `weights_plain` path (`uli_enc.c:230-232`) and our search has no DAC-weight
   iteration. Measured, the pin is inert on every cell here. The `dacw1` row runs, on
   the generated fixture,

   ```
   /home/sam/Documents/coding/gba-audio/build/uli_enc --album <work>/tiny/tiny.f32 \
     --tracks <work>/tiny/tiny.tsv --out <work>/tiny-dacw1.uli \
     --dacw 1 --budget 300000 --threads 1
   ```

   and reports 0 bytes differ from the `tiny` cell's own `--dacw 0` blob (39,308 B,
   `68dca43a…cb756d`). The same measurement on the album, at `biomaster`'s flags:

   ```
   /home/sam/Documents/coding/gba-audio/build/uli_enc \
     --album /home/sam/Documents/coding/ulc-gba/gba-sound-player/examples/uli_bench/assets/album.f32 \
     --tracks /home/sam/Documents/coding/ulc-gba/gba-sound-player/examples/uli_bench/assets/tracks.tsv \
     --out /tmp/s7-extra/biomaster-dacw1.uli \
     --budget 3294000 --cap 74 --threads 8
   ```

   yields 3,293,048 B, SHA-256 `990da225…7f8a73` — byte-identical to the matrix's
   `biomaster` `--dacw 0` reference blob in the table above. Both are 0-byte distances
   because the DAC weights are constant within one track and all four cells are
   single-track. The pin matters on a multi-track album, which this matrix does not yet
   include.
2. **The `tiny` fixture is generated, never tracked.** The committed `tiny.uli` is a
   decoder pin (the fixture README says the C encoder made it), not an encoder
   expectation, so the `tiny` cell runs `gen_fixture.py` in a temp directory instead.
3. **The `dacw1` row is oracle-only and labelled "not a match expectation — the dacw
   objective we do not have".** It exists so the size of that unimplemented objective is
   a number rather than a guess; on this matrix it measures 0 bytes, which is the
   single-track finding above rather than a claim about the objective.
4. **E-R7 stays not-discharged, and this is the instrument that says why.** The two
   `verify:` columns are full-band time-domain; E-R7's 32.29 / 25.78 is codec-only. S7
   prints the oracle's own codec proxy beside our mean and worst so the two metrics are
   never interchanged, and the acceptance answer is recorded as a **DIFF**: the four
   cells are not byte-identical to the reference, and that is the measured state.

#### S6 — the cycle price, and the high-rate gap it turns out to be all of

**Two things S6 was not.** It was not a second quantiser: the tuple candidate set, the
band-class shortest path and `J = D + kappa·D²·R` are S3's, and the new argument is a
price on decoder cycles, not a new rule — with it at `0.0` the search reproduces the S5
blob **byte for byte** (the whole album below is the witness). And it was not an answer
to the 0.16 dB residual at the two lower budgets: measured, it does not move it.

**What it is.** The reference re-codes an over-cap frame instead of refusing the
candidate. `quant::band_cycles` is the reference's `band_cyc` (a coded band's decode work
beyond its band symbol, relative to a zero band) built from `CY_*` constants mirrored from
`uli_lite::Ops::cycle_model`; `quantise_chan_rd` takes `mu` and charges
`mu · (CY_BANDSYM + band_cycles)` on every band transition and on the terminator. `rd::point()`
is the reference's `point()`: measure at `mu = 0`, and if the cap is missed raise the price
from `lam · 0.01` by ×2.5 (14 tries), then six geometric bisections down to the smallest
price that still fits. `Curve` carries that `mu` per candidate and `to_content` re-codes at
the winner's price, so the shipped bands are the ones that met the cap rather than a second
search that happens to be near them. A frame no price can bring under the cap returns its
plain coding and the allocation excludes it — the reference's "leave it to allocation". The
measurement pass that feeds training stays at `mu = 0`, as the reference's first
`curves_compute` does; only the pass whose bands ship pays the price.

**Measured, 20 s of `bioluminescent` (655,360 samples, 1,281 frames),**
`--budget 400000 --cap 74 --threads 4`, `--rd` with the shipped `--stride 8 --fine 4`:

| | codec SNR | stream bits | blob | est. cycles max | SHA-256 |
|---|---:|---:|---:|---:|---|
| ours, `--musearch 0` | 39.57 dB | 3,154,706 | 399,736 B | 74.0% | `b32614c3…ab771d` |
| ours, `--musearch 1` | **41.01** dB | 3,154,594 | 399,724 B | 74.0% | `7a63311f…b00ff42` |
| reference, `--musearch 0` | 39.54 dB | 3,154,854 | 399,756 B | 74.0% (verify prints 74.2%) | `f994a199…e4c139` |
| reference, `--musearch 1` | **41.02** dB | 3,156,681 | 399,984 B | 74.0% (verify prints 74.3%) | `a3e949fd…9965c1` |

Three things follow from the table rather than from intent. The ordering is the one the
gate asks for: our re-coded output sits between our own rate-only output and the
reference's re-coded output (39.57 < 41.01 < 41.02). The re-code is worth **1.44 dB** here
against the oracle's **1.48 dB** — same direction, same size, and the `g` range moves the
same way it moves for the oracle (ours 8..112 → 8..108). And the residual at this budget
is **0.01 dB**: on this slice the rate-only search was already 0.03 dB ahead of the
oracle's (39.57 against 39.54), so S6 was not closing a 1.15 dB deficit here — it was a
mechanism both encoders either have or lack, worth ~1.45 dB to either. The 278 of 1,281
frames that were re-coded (22%) are the cap-constrained ones.

**It is a high-rate mechanism, and the product album is not high rate.** The 2,498
bits/frame in the table is a 400 kB budget over 1,281 frames. The shipped album is
**199 B/frame = 1,594 bits/frame**, and at that rate the cap binds on **3.5%** of a
`cross` slice's 3,840 frames: the re-code is worth **0.02 dB to the reference**
(28.78 → 28.80) and **0.01 to us** (28.80 → 28.81) on the same 60 s at the same
764,500-byte budget. So "the high-rate gap was all of it" is true where the cap binds
hard and false at the rate the product ships at — **S5's 0.41 dB `cross` residual is not
cap-constrained, and S6 does not touch it.** The largest measured gap is therefore still
the album-scale one, and its owner is not S6.

**It does not fix the 0.16 dB, and that is measured too.** At `--budget 40000 --cap 74`
the two arms are **byte-identical** (39,732 B, `80ab819a…572e51`, 0 frames re-coded): the
cap does not bind at all there. At `--budget 120000 --cap 74` it binds on **9** frames, so
the blob moves by 4 bytes (`17ed982e…6dd87` vs `4a0a224d…0dc11`) and the codec SNR does
not: **31.19 dB both ways**. Those two SNRs are exactly S4's table (31.19 / 21.67), so the
residual those rows carry is untouched, and the honest statement stays "0.16 dB,
unattributed".

**On the whole album it does something, and less of it.** `biomaster` (16,553 frames),
`--budget 3294000 --cap 74 --threads 4`: musearch 0 → **31.43 dB, 3,293,656 B**
(`6f4f3d66…82be51`), musearch 1 → **31.89 dB, 3,293,696 B** (`f4f3a52a…e095c8`), 1,766 of
16,553 frames (10.7%) re-coded, cap 74.0% max in both. So the blob **does** change and
quality rises 0.46 dB — but the cap binds on a tenth of the frames at this rate rather than
a fifth, which is why the gain is a third of the slice's. The musearch-0 hash is S5's
pinned blob, i.e. the price is a no-op when it is off — so **every S5 gate number in this
file, the `cross` pin included, is the `--musearch 0` arm by construction**. Under the
shipped default those blobs move wherever the cap binds, and no album has been re-pinned
at `--musearch 1`: `cross` at four threads would run over two hours under the multiplier
below, which is the cost argument rather than an oversight.

**What it costs, because a knob that is free is a knob that is lying.** Same slice, same
flags, wall clock: `--musearch 0` **18.1 s**, `--musearch 1` **73.2 s** — a 4.1×
multiplier, against the reference's 7.5 s → 13.9 s (1.9×). Every over-cap candidate is
re-quantised up to 22 times (the seed, fourteen doublings, six bisections) in both
domains, and at a binding cap most fine-step candidates are over it. So S6 widens S5's
E-R9 gap rather than narrowing it: `biomaster` took 3m46s rate-only, and 4× on the slice
predicts ~15 min with the price on. That is a named follow-up of the same shape as the
curve-pass estimate — "the bake is in the edit loop" is still not true — not a defect of
the mechanism.

**Four tests, and one print that refuses to overclaim.** `band_cycles_witness_the_reader_cycle_model`
reconstructs `Ops::cycle_model()` from the mirrored constants and `band_cycles` for a real
channel pair, so a constant that drifts becomes a failing test rather than a cap that
quietly stopped binding; `a_larger_mu_never_costs_more_decoder_cycles` is the monotonicity
the bisection assumes (and that it buys cycles at all); `an_over_cap_candidate_is_re_coded_not_refused`
is the mechanism end to end; `musearch_is_inert_when_the_cap_does_not_bind` is the 40,000-
budget row above. `tests/determinism.rs` now runs the RD path with the re-code at 1 vs 4
threads. The CLI prints `musearch on` in the album line **and** a second line, `musearch: N
of M frames re-coded under a cycle price (mu > 0)` — because "on" and "did something" are
two claims and the second one is the measurement.

#### S5 — assembly, the WAV layer, and the curves that had to move

**Two things S5 was not.** It was not the budget arithmetic — that has been exact since S1,
and the 43-minute run exercises it rather than adding to it. And it was not "the encoder
needs to accept WAV": E-R2's boundary is the PCM pair, and the WAV front end is a
convenience over it whose only correctness claims are that a natively-32,768 Hz file is
untouched and that everything else is held to SNR.

**What it was, and the number that says so: E-R9.** The plan said S5 parallelises the curve
passes; the measurement says they were nearly all of the time. `Curves::compute` and
`refine` were one loop over every frame, so `--threads` reached only the emit pass and the
search ran at one core. Frame-parallel now — contiguous chunks, results assembled by frame
index, each worker owning its coefficient scratch and its frames' float sums (R5a/R5b) —
on `biomaster` (16,553 frames, 14 minutes of audio, `--budget 3294000 --cap 74`):

| | wall | user | blob |
|---|---|---|---|
| S4, four threads | 12m16s | 11m18s | `6f4f3d66…82be51` |
| S5, four threads | **3m46s** | 12m24s | `6f4f3d66…82be51` |

Total CPU time barely moved (11m18s → 12m24s, +9% of contention and memory bandwidth), which
is the honest reading: the work did not get cheaper, it stopped queueing. The blob is
**byte-identical to S4's**, so the parallelisation changed the clock and nothing else — that
is the assertion, not a hope, and `tests/determinism.rs` now covers the RD path at 1 vs 4
threads as well as emit.

**The failure worth recording is that the CLI's defaults are behaviour.** Rewriting the
argument parser for `--wav`/`--emit-pcm` quietly moved `--fine` from 4 to 0 and turned
training off by default; on the 5-frame `corr` fixture the search then emitted 18,331 bits
where S4 emitted 15,664, and the blob hash changed. Nothing in the suite noticed — the RD
tests pass explicit shapes — and the thing that caught it was re-running S4's binary on the
same input and diffing hashes, not reading the diff of the parser. The defaults are restored
(`--fine 4`, training on, `--no-train` the A/B arm), and the equivalence is recorded above
as a blob hash rather than as an intention.

**The WAV layer, and the resampler bug its test found.** `wav.rs` reads RIFF/WAVE including
`WAVE_FORMAT_EXTENSIBLE`, integer PCM 8/16/24/32 and float 32, one or two channels, any
rate; `assemble.rs` duplicates mono, joins the tracks gaplessly, reads `LIST/INFO`
`INAM`/`IART` and falls back to the file stem and `?`. **The tags are carried verbatim:** the
first version of this file also folded them to ASCII, which was wrong for a reason worth
recording — §5.5's fold already has one home (`scripts/album_prep.py::fold`, used by the
bake), and the encoder's copy could not do NFKD, so it mapped Latin-1 and silently dropped
every other script (`fold("日本語")` was the empty string, which the test then pinned). Text
is album policy, decided before `tracks.tsv` exists; the encoder's job is to carry what it
read, and `--emit-pcm` hands the caller the same bytes;
`resample.rs` is a polyphase Kaiser-windowed sinc, 32 taps per phase, each phase normalised
to unity DC. The first version's cutoff was a constant 0.475 of the *source* rate, which is
right for upsampling and wrong for downsampling: a 20 kHz tone at 44,100 Hz came out only
−2.2 dB down, i.e. it aliased straight into the band. `tests/resample.rs` now asserts the
guard band rejects what 32,768 Hz cannot carry, alongside the assertions that matter to the
encoder: 32,768 Hz is a byte copy, DC gain is 1 within 0.05 PCM16 units, and a 1 kHz
passband tone keeps >80 dB both up and down.

**The byte claim is a test, not prose.** `tests/wav.rs` builds the same samples twice — as
`album.f32` + `tracks.tsv` and as a 32,768 Hz stereo 16-bit WAV — encodes both at `--g 100`
and asserts the blobs are equal. `--emit-pcm DIR` writes back the pair the front end
assembled, and a CLI test re-encodes the dump to the same bytes, so the dump is the buffer
that was encoded rather than a second conversion.

**The verify block grew the reference's columns** (S7 needs a readable diff): per-track
time-domain SNR at pcm16 and int8, their means, the pooled album codec SNR that E-R7 is
stated against, and every track is now also **seeked to its own preroll** and decoded — "every
track decodes" as a claim about the track table, not merely about a linear walk. The album
line prints `dacw off`: that pass is not implemented, and printing `0` would claim a knob
ran. (S6 landed as the decoder-cycle price, not as the DAC-weight pass.)

**The gate, measured.** The 43.5-minute `cross` album (167,222 frames) at
`--rd --budget 33292288 --cap 74 --threads 4` encodes to **33,291,896 bytes — 392 spare**
(`69911924…e656b8`), every one of the 12 tracks decodes and is seeked to, `xover 0`,
`hfnz 0`, est. cycles 67.3% mean / 74.0% max. The reference on the same box, inputs and
flags is 33,280,720 bytes (`b3c63657…89ae2a`), 32.16/28.97 dB against our 31.75/28.56 —
a 0.41 dB quality gap, on a different album from S4's 0.16 dB, and the two must not be
quoted as one. The full table, including the one thing S5 does **not** discharge — 40m11s
against the reference's 14m30s, i.e. E-R9's *rationale* rather than its mechanism, because
`measure` codes every candidate exactly — is in
[`RECORD.md`](RECORD.md) → "2026-11-20 — S5". A too-small budget still refuses before the
first frame (S1's `Refusal`, covered by `tests/refusal.rs`).

*Superseded 2026-11-27.* S8-7 makes this encoder emit the oracle's bytes on every matrix
cell, so the 33,291,896 B / `69911924…e656b8` figures above are the S5 arm, not the current
output. They are kept as that measurement, with this note, rather than edited; the current
encoder figures are the MATCH run in §S7.

#### S4 — training, and the ~1 dB it turns out to have been

**What training buys, measured.** The same albums, same budgets, same cap, with the trained
pass on and off, against an independent encoder at the same budget:

| budget | built-in tables | trained | oracle |
|---|---|---|---|
| 400,000 | 39.74 dB | 39.87 | 41.02 |
| 120,000 | 30.28 | **31.19** | 31.35 |
| 40,000 | 20.81 | **21.67** | 21.83 |

So the residual this plan had been calling "~1 dB, probably tables" was in fact tables: at the
two lower budgets the gap to the oracle closes to **0.16 dB**.

**Where the rest goes — one gap with an owner, one without.** Both were measured on
2026-11-20 rather than reasoned about, and the difference matters because the first was
previously asserted on evidence that argued against it.

*The 1.15 dB at the top budget is S6, and the margin is comfortable.* At
`--budget 400000 --cap 74`, switching the oracle's cycle-aware re-coding off costs it
**1.48 dB** (41.02 → 39.54) and moves its chosen `g` range 66..108 → 66..113. So the whole
high-rate gap is that mechanism — an over-cap frame is re-coded by dropping its cheapest
coefficients instead of refusing the candidate, which is what this encoder does today — and it
is worth more than the gap: with it disabled, this encoder is **0.33 dB ahead** of the oracle
(39.87 against 39.54). Nothing else needs to move at high rate; S6 closes it.

*The 0.16 dB at the two lower budgets has **no owner**.* `--musearch` does not touch it —
measured on 2026-11-20 (S6): byte-identical output at `--budget 40000`, and at
`--budget 120000` it re-codes 9 of 1,281 frames, moves the blob 4 bytes and leaves the codec
SNR at **31.19 dB** either way — so no step S1–S7 claims it. It is 3.7% of a dB and may
well be inside tolerance — but **E-R7 has never been measured
on either side**: the requirement is *codec-only* SNR at 32.29 mean / 25.78 worst, and every
number in this section is full-band time-domain. So parity against the actual requirement is
still unmeasured, and that measurement is S7's. Until then the honest statement is "0.16 dB,
unattributed", not "0.16 dB, fine".

**Two things are trained and one is not.** The Huffman **lengths** come from the album's own
symbol counts, and the **reconstruction offsets** `dq` come from the mean error per magnitude
bucket — the second is a *quality* mechanism, not a rate one, and it was not obvious from the
outside that training had two mechanisms rather than one. The class *structure* is not
trained: `ctype`, `rbits` and `esck` are the format's, so E-R4's premise holds across training
by construction — and is still re-checked on the trained bytes, because "by construction" is a
claim and not a witness.

**Two passes, because one is not enough.** Which symbols are emitted depends on the lengths,
and the quantiser prices its candidates with the lengths, so a plan made with the built-in
tables is only an approximation of the plan the trained tables will produce. Pass one is the
measurement; pass two is the album as it will ship. It doubles encode time, which is the price
of the 0.9 dB.

**The shortest path had to exist first.** The per-band class choice was only ever valid
because all 15 default context codebooks are byte-identical — a class symbol then costs the
same from any context, so bands carry no coupling. Trained codebooks differ per context, so
S4 required the band-class choice to become a shortest path over (band, class), shipped one
commit earlier as a provable no-op: bits and distortion identical to the greedy version and to
the oracle's, because on context-independent tables the DP *is* the greedy.

**Determinism.** The counts are integers and the merge is deterministic; the only float in the
path is the length-capping re-merge's `sqrt`, which E-R5's amendment already covers ("wherever
the bytes come from integers; on the reference host, wherever they cross a float"). So a
trained table is reproducible per host and is *not* claimed to be reproducible across libms.
That is the same posture as the default tables before their golden froze them, and the same
way out would be available if a trained set ever needed pinning.

Five tests: training pays for itself at a binding budget, two encodes are byte-identical, the
trained set parses and satisfies the premise, an empty statistic set leaves the base tables
valid, and the shipped tables really differ from the built-in ones.

#### S3 — the ratio was the measurement, and two silent failures nearly hid it

**What closed the rate gap.** S2 quantised at `lam = 0`, so it kept every coefficient
whose nearest level was nonzero. The reference applies `J = D + λ·R` **per tuple** with
`λ = kappa·D²` and offers **zero** as a candidate, so a coefficient that is cheap to lose
is dropped. Measured against the reference's own `frame_code` at fixed `g`, the ratio was
**1.14× / 1.41× / 2.58×** (g = 60/100/140) with `lam = 0`; with the per-tuple RD it is
**0.9997× / 0.9989× / 0.968×**, and the distortion matches the reference's to seven
significant figures at `g = 60` and `g = 100`. At that point the two encoders are making
the same band decisions, not merely similar ones — the strongest evidence available
short of the S7 matrix. Cycles agree to 0.3%, so decision 4 holds in practice.

**Bands have no context coupling in the default tables, which is why there is no
Viterbi.** All 15 band-context codebooks are byte-identical (measured, not assumed), so
a class symbol costs the same from any context and the optimal class sequence is
per-band independent. The spectrum end is therefore an exact prefix scan rather than a
DP. **S4's trained tables break this** and will need the reference's band-class Viterbi.

**The terminator is not priced in the RD objective, deliberately.** The reference
charges `λ` for a coded band and nothing for the EOB symbol. Pricing it — as this
implementation did at first — makes the search terminate earlier than the reference's:
2.5% more distortion and 3 bits/frame at `g = 140`, versus exact agreement once the
charge is dropped. It is the reference's quirk and S7 has to reproduce it.

**Corrected 2026-11-20 (S6 review) — the first sentence is wrong about the reference.**
`enc_core.c:317` charges `λ · le` for the terminator, `le = T.len_cb[s][EOB]`, and
`git blame` dates that line to the reference's first codec commit — a fortnight *before*
the `g = 140` measurement above. So that measurement established something narrower than
the sentence claims: dropping the charge agreed better with the reference's *output* at
that step, which is not the same fact as the reference omitting it. This search still
omits it, so the two encoders price the spectrum end differently wherever context rows
differ, i.e. on the **trained** tables S4 ships (the default set's 15 rows are identical,
so the charge is a constant across termination candidates there and the argmin cannot see
it). S7 must re-run this A/B before it treats the omission as a reproduction target: a
mismatched cell explained by this sentence would otherwise be attributed to the codec.

**Two silent failures, both of the same shape.** First, `--rd` parsed into a field that
`encode` never read: the crate built, 45 tests passed, and the encoder emitted **S1
silence**. Silence costs 8 bits/frame and "the search collapsed to `G_MAX`" costs about
the same on this album, so the rate could not tell them apart — and the `g` range that
would have, I had filtered out of the output with my own grep. Second, filtering class
candidates by headroom meant a band louder than every class was offered **no class**,
fell through to class 0, and vanished: on the loudest frame of the album that was
1.5e11 of energy, and because the class symbol is cheaper than the coefficients the
frame looked **cheaper** than the reference's (0.80× its bits). Only distortion exposed
it, six orders of magnitude high. Both are why `make uli-test` now counts the tests that
reported rather than inferring health from silence, `Args` is destructured exhaustively
so a field cannot be added and ignored, and the harness prints four columns instead of
one.

**M/S landed (S3b), and it needed its own step for the reason predicted.** The
derivation held: forward rotation by `1/√2` (orthonormal, so the rotated-domain squared
error *is* the output-domain squared error — the two output channels err by
`(e_M ± e_S)/√2`, whose squares sum to `e_M² + e_S²`), quantise at `step_bar·√2/16`, and
the reader's plain integer add finishes the job. That open question — whether a
rotated-domain `D` is comparable to an L/R one for the allocation's `w·D + λR` — is not a
matter of opinion: it is true by orthonormality, and `tests/quant.rs` asserts the rotation
preserves energy so a future edit cannot quietly break it.

Measured, `ms` forced, against the reference's `frame_code` on 200 frames:

| g | ours bits/frame | ref | ours distortion | ref | cycles |
|---|---|---|---|---|---|
| 60 | 3789.7 | 3791.3 | 9.310014e3 | 9.298935e3 | 212098 / 211852 |
| 100 | 825.4 | 826.4 | 4.159351e6 | 4.159351e6 | 182292 / 182092 |
| 140 | 58.8 | 59.8 | 7.937612e8 | 7.937612e8 | 160504 / 160504 |

Distortion is **exact** at g=100 and g=140; bits differ by exactly the `ms` header bit the
reference counts and `measure` excludes. Cycles differ by a multiple of 40 at the fine
steps and by **zero** at g=140 — which is the `long_codes` term, since the reference's
encoder never sets it and a coarse step produces no long codes. That is the S7 conflict
decision 4 predicted, now with evidence, and it is recorded rather than papered over.

One rule the port had to get exactly right, and it is not obvious: at each `g` the two
domains are quantised with **their own** unit (`step_D(g, ms)`) but compared against each
other with `λ` from the **ms = 0** unit, so they are weighed on one scale. Weighing each on
its own unit's λ would systematically favour whichever had the larger one. And the cycle
cap is a constraint, not a term: a candidate over the cap loses before `J` is consulted.

At equal budget the search now lands close to the reference on rate and much closer on
quality (`make uli-rd-ab`, 20 s of `bioluminescent`):

| budget | before S3b | after S3b | reference |
|---|---|---|---|
| 400,000 | 157.6 kbit/s / 33.66 dB | 157.6 / **37.57** | 157.7 / 40.98 |
| 120,000 | 45.7 / 26.89 | 45.7 / **29.87** | 45.8 / 31.41 |
| 40,000 | 13.7 / 17.81 | 13.7 / **20.57** | 13.8 / 21.81 |

The gap went from 4.0–7.3 dB (before S3b) to **1.2–3.4 dB**, and 93.3% of frames chose M/S (the
reference reports 87.1% on its album). What remains is *not* the search — the fixed-`g` A/B
above shows the quantiser making the reference's decisions exactly. It is the two things
the reference still does and we do not: **trained code tables (S4)** and a second
allocation pass over the frame weights (`--dacw`). S4's gate is where that attribution gets
settled; until then it is a hypothesis with the right shape, not a claim.

**The gate** is `make uli-rd-ab`: same PCM, same budgets, four columns (bytes, fit,
kbit/s, SNR, chosen `g`), nonzero exit if any row overshoots. It is deliberately off
every gate's path — it needs the off-tree reference binary, which E-R1 keeps out of
`make uli-test`. Full-pipeline rates now agree within 0.1–0.8% at three budgets. Both
encoders' SNR here is **full-band time-domain**; E-R7's 32.29/25.78 dB is codec-only and
the two must not be quoted against each other.

#### S2 — the terminator is conditional, and only real audio could say so

**A channel that codes all `NBANDS` bands owes no EOB symbol.** The reader loops
`for b in 0..NBANDS` and returns when it *reads* the terminator, so when the loop
exhausts first the terminator is never consumed — and a writer that emits one
regardless leaves three bits in front of the **next channel**, which reads them as
its own first class. Since `cb[ctx][k]` is the EOB code, that class *is* `k`, and
the channel terminates immediately. The frame then decodes **without error** and
silently drops half its spectrum: right container, right size accounting, wrong
audio. `enc_core.c` guards the same thing with `if (endB < nb)`; ours guards it with
`if chan.bands.len() < NBANDS`.

S1 could not have found this. Its frames coded zero bands, which is the one case
where the terminator is unambiguously owed, so every round-trip it owned was right.
What surfaced it was the first dense album through the first real quantiser, and
what pinned it down is now the S2 gate's strongest assertion: for every frame, the
reader's own `bitpos` advance must equal the bits the plan sized, and the decoded
`ops` counts must equal the ones the encoder decided to write. That is a
consumption-level check rather than a "did it error" check, and the difference is
this bug — an error-free wrong decode.

Two smaller things the same pass settled. The **candidate set is `{0, floor t, floor t + 1}`,
not the nearest level**, and it is *complete*: a level reconstructs to `m − dq/16`, so the
best level for `t` is `t + dq/16`, which lies in `(floor t, floor t + 1]` for every `dq` in
`0..16` — `floor t − 1` is never optimal. **Corrected 2026-11-20:** this paragraph used to
claim the opposite, that `floor t − 1` was sometimes closest and was being declined, which
has the sign of the `dq` shift backwards. The code was right; the explanation made a
complete candidate set look like a shortcut. `tests/quant.rs` asserts the rule against an
independent restatement, and `tests/fixtures/band_rdo.txt` pins 2,400 cases against a frozen
oracle. And the **analysis constants are checked in** —
`mdct_tables.rs`, generated from `mdct_init`'s expressions by
`scripts/gen_uli_mdct_tables.py`, which is R5c applied to the encoder for the first
time; the window is stored as its symmetric half and the frozen fixture proves the
mirroring is bit-exact, not merely algebraically equal.

#### S1 — what the review found, and what each one decided

S1 is on `feat/uli2-encoder-s1`. It landed green — the round-trip real, the
container field-for-field against `Album::parse`, the cycle model's constants
against `uli_dec_ref.h` — and a review then found the residue "Where it lives"
predicted. What follows is that review's content in the only form worth keeping:
each item where the decision it produced lives.

**The method, because it is the reusable part.** Diff a *section* against a built
oracle, not a whole blob against a hope:

```sh
scripts/gen_uli_default_tables.sh          # or: make uli-default-tables
# compiles "$GBA_AUDIO"/encoder/enc_core.c with the reference's own CFLAGS,
# calls tables_default + tables_blob, and writes the fixture
```

`enc_core.c` needs nothing but the real `uli_tables.c` to link, so this witnesses
the Rust derivation without making C a build, link or vendoring dependency of
anything we ship — E-R1 untouched. Run against `DefaultTables::build().to_blob()`
it gave **1,053 differing bytes of 5,212** and, more usefully, localised them: `k`,
`ctype`, `rbits`, `esck`, all 15 `len_cb` rows and **every live coefficient length
row** matched byte for byte. So the geometric model, `huff_lengths` (two-min merge,
scan-order ties, `sqrt + floor` re-merge) and the canonical assignment were correct
as transcribed — a positive result S1 had no witness for before, and the reason the
golden is worth its bytes.

**Each class carries one coefficient row, and the reader never looks at the other.**
`to_blob` wrote `cls[c].len` into both the `len_q` and the `len_p` row of every class.
`cls[c]` holds 256 lengths for a PAIR class and 81 for a QUAD class, so each PAIR
class leaked the head of its pair lengths into its QUAD row and each QUAD class
leaked its row into the head of its PAIR row: 11×81 + 2×81 = 1,053 bytes. Its doc
comment asserted the opposite, which is the shape of the whole finding — the code and
the claim about the code were written separately. *Why the gate missed it:*
`parse_into` selects the row by `ctype` and never reads the other, and every
round-trip hands `uli-lite` the **same** `DefaultTables` object the writer used, so
those bytes are invisible to a green suite by construction. *Why it is wrong and not
merely different:* the reference reaches zeros by `memset` in `tables_default` and
never rewrites the unused row in `tables_from_stats` — verified zero in all five
committed corpora, so it is a format invariant and we were the only producer breaking
it. *What was done:* write the row only for its own `ctype`, and refuse a row that
does not fit its slot — `coeff_row`/`write_row` assert the width, so re-enacting the
bug panics at the call site (`a 81-byte row cannot hold 256 lengths without truncating
them`) instead of writing bytes nobody reads; the silence was the defect, not the row
choice. *Witnessed* by `tests/fixtures/uli2_default_tables.bin` (5,212 B,
`15b477ed…`, generated from C **before** the fix, so it cannot be our own output), by
a structural test of the invariant in our blob *and* in the golden so writer and
checker cannot agree with each other and nobody else, and by comparing the transcribed
rows against every corpus on disk — 43 bytes, which had never been asserted of
anything. Confirmed red: re-enacting it fails 5 of 39 tests with the diagnosis in the
message.

**The track table owns `NS`.** `pcm.rs` had taken the length from the *file size* and
required every track to end within it; `uli_enc.c:read_tracks` computes
`NS = TR[NT-1].start + TR[NT-1].len` and asserts the file is exactly `NS*8` bytes.
Padded tail or short last track, the two encoders disagree on `NS`, hence `NF`, hence
the seek table, hence every byte after the header — for reasons that have nothing to
do with the codec, at the exact boundary E-R2 pins. Ruled: mirror the reference. Two
corollaries fell out of reading the oracle rather than guessing, and both are now in
code: **contiguity is not required** (the reference never checks it; E-R2's "joined
gaplessly" describes what `prep.py` emits, and a gap is silence that still costs
frames), while **ascending, non-overlapping rows are** — the reference assumes
`TR[NT-1]` is the furthest end without checking, so our stricter refusal is what makes
both encoders read one `NS`. Five cases in `tests/pcm.rs`, and reverting the
comparison fails exactly three of them, leaving the gap and mid-start cases green —
which is the assertion that the rule refuses the right thing and not everything. One
divergence is recorded rather than fixed: the reference parses rows with
`strtok(line, "\t\n")`, which **collapses an empty field**, so a track with no title
takes the artist's string as its title. Ours keeps the empty tag. Reproducing a
field-shifting bug is not byte-exactness worth buying, but it means an S7 input must
carry no empty tag.

**`g` is bounded by the reader's window, on the way out.** `write_frame` accepted any
`i32`, the container stored `g_prev as u16`, the decoder zero-extends it and then
requires `0 ≤ g − 4·ms < NSTEP` (192). S2 proposes `g` from a quantiser, so the first
out-of-window step index would have been written happily — whole album, every
encoder-side check passed — and rejected on hardware as `BadFrame`. Ruled as a
`GPlan` of `(g, ms)` per frame taken as an argument by `Plan::new`, because `g`
decides both whether the frame decodes and how big it is, so the budget walk and the
refusal belong in the same place; S1's stream size is now derived frame by frame
instead of `n_frames ×` a constant, same exactness and correct the moment the plan
stops being uniform. Three forks: the bound is the **decoder's**, not the reference's
`GMIN..GMAX` search window (copying that would refuse legal albums and conflate a
container limit with S3's policy); `GPlan::new` **does not validate** — an
unconstructable-invalid plan leaves the rule untestable, so the constructor accepts
and `Plan::new` refuses, and reverting only the `validate()` call fails exactly one
test while the six pure-rule tests stay green, which is the distinction being paid for
(a validator nobody calls is the same bug again); and `u16` representability is
**derived, not double-checked** (`ms` is one bit, so a legal `g` is `0 ≤ g < 196`),
with a `debug_assert!` at the truncation so the two sides speak if `NSTEP` ever moves.
Seven tests in `tests/gplan.rs`, one of which hand-builds an out-of-window stream past
the plan and shows the container parsing it and the reader refusing frame 0 — the
rule's whole argument in one test, unreachable through `encode` now. It also closed a
triple derivation of `g_prev` for free: one `GPlan::g_prev`, with a `debug_assert_eq!`
tying the worker's running state to it, since a seek landing on a different state than
the linear decode walks is an album that plays wrong from the second track on.

**A gate that never runs is not a gate.** S1 added `uli-enc` to `uli-check`'s `fmt`
list and to nothing else, so the round-trip — the thing S1 exists to provide — ran
only when someone remembered. `make uli-test` now builds and runs it through the
Makefile's existing host-workspace pattern (`GATE_NEUTRAL_CWD` + `--manifest-path` +
`--target $(HOST_TRIPLE)`). The same mistake was sitting in `uli-enc/Cargo.toml`,
which claimed its `[workspace]` kept the root's target config out: cargo discovers
`.cargo/config.toml` by walking up from the **cwd**, so building in place inherits
`build-std` and the GBA target and fails with ~150 errors about `Vec` and a missing
`#[panic_handler]` that say nothing about the code. A workspace isolates the
dependency graph; a neutral cwd is still mandatory. Corrected where it was written.

**Fragments worth keeping, with no home of their own.** The `div_ceil`-style clippy
warnings are notes, and the one in `bitwriter.rs` is a deliberate transcription of the
reference's `(bits + 15) / 16 + 4` that wants an `#[allow]` with that reason rather
than a rewrite. `read_bits` shifts in `u64`, so `((hi<<16|lo) << b) >> (32-n)` never
discards the `b` bits above the window — measured wrong on 416 of 512 `(off, n)` pairs
taken alone — and nothing corrupts only because its one caller feeds `put`, whose
mask throws the high garbage away; the fix is to do the arithmetic in the width the
formula assumes (`u32`) and to give the function a test with `start % 16 != 0`, which
the current one does not have, though the real assembly hits that path every other
frame because a silence frame is 8 bits. It threatens E-R5 through a future edit
rather than today, so it is S1 closeout, not a step.

### The acceptance question, answered (revised 2026-11-19 — it is measurable, and it was measured)

**Target gate: given the same PCM and the same flags, the Rust encoder reproduces
the C encoder's blob byte-for-byte.** The plan first treated this as a hope with a
documented fallback; it is now a gate that runs today. Re-encoding off-tree with
the reference at `gba-audio/encoder` reproduced two committed corpora **exactly** —
the 129-frame fixture and the 16,553-frame `biomaster` album — and `--threads 1`
agreed with `--threads 8` byte for byte. The dated entry, with commands and
hashes, is in [`RECORD.md`](RECORD.md) → "the reference encoder reproduces its own
corpora". This is S7's spec.

So the gate stands, with three refinements:

- **Measured at the PCM boundary** (`album.f32` + `tracks.tsv` → blob), not at the
  WAV boundary. The earlier reason for exempting the resampler was the wrong
  reason: it is not that `ffmpeg`'s filter is unmatchable, it is that the
  reference's own PCM preparation is **library-dispatched** — `prep.py` calls
  `gba_dac_model.resample`, which takes its FFT path when the rate ratio's
  denominator fits in 2¹⁶ and otherwise falls through to `soxr` *if installed*,
  then to `scipy.resample_poly`. The same flags can therefore yield different
  bytes on two machines depending on which library answers, which is not a
  property worth pinning a codec gate to. A source already at 32,768 Hz bypasses
  the resampler entirely — which is why the fixture has no resampler in its path.
- **The proof is a matrix, not an exhaustive sweep.** Fixture at `--threads 1`;
  `biomaster` at its build settings (`--budget 3294000 --cap 74 --threads 8`);
  one point from `adv` or `cross`; **plus one deliberately off-default sweep**
  (say `--cap 60`, budget halved). Without that last cell both encoders could
  agree by sharing a lucky path and the gate would pass for nothing.
- **It is host-qualified.** The reference builds with `-O2 -march=native`, so
  "byte-identical" means byte-identical *on the reference host* (this machine,
  where both reproductions above ran). Fine for a dev gate; worth naming so nobody
  reads it as a cross-platform guarantee — which is what E-R5's split clause now
  says, and why the default tables are frozen rather than computed at encode time.
  One datum, recorded here: the golden came out identical with and without
  `-march=native` on this host.

**Standing on this gate does not break E-R1, and this needs saying.** E-R1 forbids
a *build, link or vendoring* dependency on `gba-audio`; it has never forbidden
running the reference as an oracle, which is what `uli-album-asset` already does
when it shells out to `$(GBA_AUDIO)/build/uli_enc` to make a corpus. So the
byte-exactness gate is a host target that builds nothing of ours from C source,
calls the off-tree binary, and compares SHA-256s — the same posture the project
took for the decoder's C-pinned hashes, with the oracle one notch further out.

If exactness nevertheless proves unreachable on some input, the fallback gate is
unchanged in shape: **same rate and quality within tolerance, all E-R4 bounds
held, and a bit-exact decode** — with the C-produced blobs frozen as the reference
corpus and the Rust encoder's output pinned separately. But it is now a fallback
for a *cell* of the matrix, not for the gate: the reproductions above show the
reference is reachable on both a synthetic fixture and a real album.

Two aids fall out of the same experiment, and taking them makes a future mismatch
localise instead of becoming a bisect: the reference prints its intermediates
(`bits/frame`, λ per training iteration, the `retrain … (±x%)` delta, `g range
89..135`, `M/S frames 87.1%`, `verify: decoded N frames OK`, SNR), so `uli-enc`
should print the same columns (S5).

**Corrected 2026-11-20:** this paragraph used to end "`musearch` showed no activity on either
reproduced corpus, so S6 stays off the byte-exactness critical path *for these two*". That was
true of the two corpora and the settings it was measured on — where the cycle cap did not bind
— and it was then generalised beyond what it had earned. Measured since, on 20 s of
`bioluminescent` at `--budget 400000 --cap 74`, switching the cycle-aware re-coding off costs
the oracle **1.48 dB** (41.02 → 39.54) and moves its chosen `g` range from 66..108 to 66..113.
So it is not inert wherever the cap binds, and S6 is on the critical path for exactly the
regime where the cap is a constraint. The lesson is the one this file keeps relearning: a
measurement is true of the setting it was taken at.

### What it does not have to do

Not a general-purpose codec; not portable (host only); no encoder-side seeking or
streaming; no obligation to accept non-WAV input directly; it does not validate
its own output — `uli-lite` is the oracle, in the same round-trip test that
already exists for the decoder. And, per decision 3 and **E-R10**: it does not
write ROMs, does not know `cart_bytes`, and does not decide what else shares the
cartridge. It is one asset producer among several, and the bake aggregates them.

## Gate verdicts (the claims live in ULI2.md §Gates)

**One owner per topic:** the gate *claims* are stated in
[`../../ULI2.md`](../../ULI2.md) §Gates, which is the acceptance record. This is
the dated history of their *verdicts*, which belongs to the ledger because every
one of them moved on a measurement.

| gate | verdict | when |
|---|---|---|
| U1 — reference | met; redefined to "oracle, not portable product" | 2026-11-14 |
| U2 — bit-exact host | met from Phase 0 | 2026-11-13 |
| U3 — bit-exact product | met for the kernel set | 2026-11-14 (PR A) |
| U4 — real-time decode | met — decode wall under the period | 2026-11-15 (Step 1) |
| | met with the gate total and 13.73 pts of margin | 2026-11-19 |
| U5 — product playback | met on hardware (GBA SP), three ROMs, 0 underruns; one open defect (104 late re-arms from the 1 Hz redraw) | 2026-11-16 |
| U6 — memory | **corrected, then met**: the "≈15.0 KiB stack" figure was an artifact of stack-local twin buffers and is withdrawn; the measured high-water is 948 B (bench) / 1,988 B (gate), `.iwram` 26,496 of 32,768 B | 2026-11-15, 2026-11-19 |
| U7 — licensing/spec | deferred by decision: `publish = false`, no license work | 2026-11-14 |

---
### Two lessons from S3's first attempt (2026-11-19), kept because both were expensive

**1. A rate cannot distinguish "the search failed" from "the search never ran".**
S3's first attempt reported 0.512 kbit/s and I diagnosed it twice — first as the
cycle cap doing real work, then as a λ-scale mismatch between `w·D` and `λ·R`. Both
stories were wrong, and both were refuted by the same fact: the cause was that the
`--rd` flag never reached `encode()` at all, so the encoder emitted **S1 silence**.
Silence costs 8 bits/frame; `G_MAX` on this album costs about the same; the numbers
were indistinguishable by design. The field that *would* have separated them — the
chosen `g` range, which I added to the report for exactly this reason — was in the
output and I filtered it away with my own grep before reading it.

So: the report prints the step and the λ unconditionally, on one line, and the
`g` range is never again an optional field. Generalised, the rule is that when a
number is consistent with two explanations, the diagnosis is not a diagnosis, and
the fix is the discriminating measurement rather than a cleverer story.

**2. `target_bits − 3.5·NF` in the reference is the frame header, not slack.**
I read that constant as the reference papering over loose accounting and wrote so in
this file. It is not: a candidate's measured cost necessarily omits the differentially
coded `g` delta and the M/S bit, because those belong to the *sequence* rather than to
any one candidate, and with `g` varying they cost ~5 bits/frame. Without the allowance
the search overshoots the budget and trips its own E-R6 refusal — which is how it
presented once the plumbing was fixed. The allowance is taken here now that its reason
is known, and the residue (44–88 bytes at three budgets) is stream padding plus the
bisection solving on curve-measured bits instead of the real stream. The reference
closes that with a retry loop — `tgt -= (size - budget) * 8 * 1.02 + 2000`, its
"final encode attempt N" — and S3's next step is that loop, not a tuned constant.

Corollary for the harness: `assert` that `--rd` *changed* the album. A test that only
checks the output parses and decodes passes happily on silence, which is how this
survived a full green suite.
