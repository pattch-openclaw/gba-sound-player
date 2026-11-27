# `album_probe` — AP-0's record, and 2b PR-(2)'s render/presentation legs

AP-0 is the measurement pass `ALBUM-PLAYER.md` §13 opens with: three throwaway ROM
images and one host gate that replace §10.2's target percentages with witnesses, and give
§4.5's "~281,000 cycles per frame, to be confirmed by measurement" and §4.4's
`SKIP_PRIME_CHUNKS` their first numbers. **2b PR-(2) added two more images and the same
shape of gate for AP-3b and AP-3c** — `render_probe` (per-element redraw, a 600-cell
region repaint, a `set_priority` switch, and the live tile pool) and `present_probe` (the
event-commit arm against the OBJ-chrome/VBlank arm and a 60 Hz load), measured at
`ALBUM-PLAYER.md` §13's rows and written up below.

**What this directory is:** the UI spike (3 backgrounds, one 8×8 object, text, a palette
fade, VBlank-paced, with the shipped lossless feeder playing synthetic PCM underneath),
the art spike (a 120×120 `ART1` card on screen from the committed blob), the skip spike
(fade → mark stale → re-seek → silence cover → re-prime → fade in, against the real ring,
swept over *three queue policies* — drain, drop and truncate — at *two queue depths*), and
the host suite that pins the ART1 reader, §6's arithmetic, the session clock, the ring
geometry, and what §4.4's step 2 costs on the shipped `RingFeed`. Each ROM prints `[album-probe] …` lines, a `PASS` verdict, and the runner's
`Tests finished successfully` sentinel.

**What it is not:** a probe is a witness, never a gate input (the `ctor-probe` rule).
There is no `album_player` crate here, no product code, no screens, no bake tool, no
`PcmSource`, no ULI2; nothing here is wired into `flac-check` or `test`, and `album-check`
(AP-0…AP-9) does not exist yet. The shared code now comes from one crate and two `#[path]`
includes — `geometry` from `examples/flac_integration`, the timer2/3 `Counter` from
`examples/e5_fetch`, and `playback` + `checksum` from **`crates/gba-playback`** (a plain path
dependency since 2026-11-15).

That last one used to be part of the debt. The 2026-11-14 record here said Phase 0 borrowed
the shipped `RingFeed` straight out of `examples/flac_spike/` and that moving it was the
follow-up; **`ALBUM-PLAYER.md` §2.2's lift landed on 2026-11-15** — `playback.rs` and
`checksum.rs` MOVED (not copied) into `crates/gba-playback`, `RingFeed::truncate()` landed
with them, and `make flac-check` now includes `make playback-test` (24 host tests, the 12
conversion/ring tests plus 8 checksum tests plus the 4 truncate tests). Phase 0 borrowed the
shipped `RingFeed` precisely *so that* its skip numbers are statements about the ring the
product runs on rather than a model of one; the ring it measures is now the crate's, and the
truncate arm below measures the shipped method rather than an arithmetic sketch of one.
Borrowing `geometry` from `examples/flac_integration` and the `Counter` from
`examples/e5_fetch` is still open debt — §2.2 lifts those separately.

## Reproduce

```sh
python3 scripts/album_probe_art.py --self-test   # generator refusal paths + audit
make album-probe-art         # regenerate the tracked blob + pin (byte-identical)
make album-probe-test        # the host half: 52 tests
make native-album-probe-rom  # five .gba images + .text/.iwram/.bss/.rodata
make album-probe             # boot all five headless, one log each
make check                   # every workspace rustfmt-clean
make flac-check              # the shipped product's suite, unaffected
```

`make album-probe` writes `album-probe-ui.log`, `album-probe-art.log`,
`album-probe-skip.log`, `album-probe-render.log` and `album-probe-present.log` at the
repository root; `make album-probe-test` writes `album-probe-test.log`. Every number below
is copied from the five ROM logs, and each table's log line is quoted where the columns
need it. `make album-probe` also greps the render and presentation logs for their named
legs — a PASS line with a leg quietly missing fails the target. The five verdicts:

```
[album-probe] PASS ui arms=5 audio=live=yes video=disarmed tile=ime-off underflows=0
[album-probe] PASS art tiles=225 live=343032 disarmed=335990 period=573440
[album-probe] PASS skip policies=[drain, drop, truncate] queues=[shallow, deep] primes=[2, 4, 8, 16] underflows=0 audio=live
[album-probe] PASS render legs=4 pool_distinct=460 pool_refs=856 max_refs=45 unchanged_cells=5 hidden=yes underflows=0
[album-probe] PASS present arms=3 commits_a=3 commits_b=0 commits_c=0 vblank_b=3 vblank_c=179 underflows=0
```

**Context for every number:** Linux aarch64 host, `mgba-test-runner` (mGBA headless) on
the ROM image, cycles at 2²⁴ Hz read from timer2→timer3 with interrupts **live** — each
line says `audio=live` or `audio=disarmed`. The probes never write `WAITCNT` (they run at
agb's boot value). These are host- and image-specific emulator numbers, not hardware.

The ROMs print sums and counts, never a divide; the means and percentages below are
derived host-side and the derivation is stated, so it can be re-done.

## What the probes measured

### UI frame cost — `ui_probe`, 60 frames per arm, audio live

An audio period holds 573,440 / 280,896 = **2.04 video frames**, so a per-frame cost occupies
`mean × 2.04` of one 573,440-cycle period — the unit §10.2's table is written in.

**Two readings, side by side.** Round 1's numbers were taken while the synthetic source had
already run out (see "the source no longer runs dry" below), so the `obj` tail and the whole
`full` arm converted silence; round 2's are taken on continuous tone. The raw sums differ by
up to 6,210 cycles, and the difference is **approximately one interleaved feeder service per
arm — not UI work**: each arm's per-frame base (its total with the frames that contain a
feeder service taken out) is identical in both rounds to within 3 cycles/frame.

| arm | live graphics | UI cyc/frame, round 1 → round 2 | share of one period (r2) | feeder cyc/period, round 1 → round 2 | feeder max (r2) |
|---|---|---:|---:|---:|---:|
| `none` | nothing — the audio-only control | 17,697 → 17,698 / 60 = **295** | 602 (0.11%) | 4,986 → 5,033 | 5,035 |
| `bg` | one background + text | 29,585 → 29,471 / 60 = **491** | 1,003 (0.17%) | 5,103 → 5,115 | 5,138 |
| `bg3` | three backgrounds, one scrolling | 52,880 → 52,875 / 60 = **881** | 1,799 (0.31%) | 5,138 → 5,150 | 5,189 |
| `obj` | `bg3` + one animated `DynamicSprite16` | 132,708 → 138,755 / 60 = **2,313** | 4,721 (0.82%) | 5,165 → 5,190 | 5,260 |
| `full` | `obj` + a palette fade every frame | 324,572 → 318,362 / 60 = **5,306** | 10,832 (**1.89%**) | 5,212 → 5,238 | 5,260 |

**How current this table is.** The `round 2` sums above (17,698 / 29,471 / 52,875 / 138,755 /
318,362) are quoted from an earlier image of this same PR; the image this branch ships reads
17,697 / 29,519 / 52,880 / 138,868 / 318,283 — the same ~0.2% image-layout sensitivity the art
section documents, which moves the derived per-frame and share columns in their last digit
only (the `full` arm stays 1.89%). The `feeder` columns are from the shipped image and did not
move, and no claim in this file turns on the difference. AP-4 re-prices this arm with the art
leg anyway.

**Why `obj` rose 6,047 and `full` fell 6,210 while the UI work did not move.** The arms are
measured with interrupts **live**, so a feeder period that lands between `t0` and `t1` is
inside the arm's `ui` window: `obj` round 2 prints `min=2191 max=8244`, a 6,053-cycle step
that is one service, and `full` round 1 printed `min=5291 max=11373` (a 6,082-cycle step)
while round 2 prints `min=5291 max=5349` — the same base, no service. Take the service out
and the arms reproduce round 1: `obj` 138,755 − 6,053 = 132,702 against 132,708; `full`
318,362 + 6,082 = 324,444 against 324,572 (2 cycles/frame of drift). The phase moved because
the producer's own per-frame cost changed with the fix — real audio costs ~54,000
cycles/frame against ~27,000 for a zero store (round 1's `full` closure read
`producer(1569578)` over 59 frames; round 2's reads `producer(3340492)`), i.e. ~9.4% of a
period, still nowhere near one. **So the two readings are evidence for two claims at once:
the feeder's per-period cost is content-independent (4,986→5,033, 5,103→5,115,
5,138→5,150, 5,165→5,190, 5,212→5,238 — ≤ 1.0% while the bytes went from silence to tone,
and the byte count never changed), and the UI's per-frame cost is too (base identical to
within 3 cycles/frame on every arm).** What round 1 got wrong was not the UI price; it was
the thing a listener would have heard.

`underflows=0` in every arm, and `late_max=0` / `late_over_grace=0` in every feeder line.
The per-period feeder mean divides the feeder sum by the same arm's `periods=` on its
producer line (the feeder line prints no count of its own). The `none` arm's 295
cycles/frame is the instrument floor (frame commit + counter reads), not UI work. One
arm's raw line, for shape —
`ui arm=full audio=live frames=60 ui count=60 sum=318362 min=5291 max=5349 wrap=0` — the
object costs ~1,330 cycles/frame over `bg3` and the palette fade ~3,095 over `obj`, both the
same in the two rounds once each arm's interleaved services are taken out.

Full-arm closure, verbatim from the log:

```
closure arm=full audio=live period_sum=16572808 count=59 = producer(3340492) + feeder(151914) + ui(318362) + residual(12762040)
```

The window is 59 video frames (59 × 280,896 = 16,572,864, read to 16,572,808) ≈ 28.9 audio
periods, which is the `periods=29` on the producer line. The **residual is named**: the
VBlank wait plus main-loop and instrument overhead — the idle poll, not work. It is 77.0%
of the window; the three measured terms are 23.0% (3,810,768 cycles of 59 frames). The
ledger closes *by construction* (`residual = period_sum − producer − feeder − ui`), so it
is not an independent check; what it establishes is that UI + feeder + producer have a
named, small size and the rest of a frame is waiting. Dividing the same `ui` sum by the
closure's own 59-frame window gives 1.92% — one dropped frame higher than the 60-frame
mean, which is the 1.89% above.

#### The source no longer runs dry (round 2's fix, and what it changed)

Round 1's `Synth` zero-filled past the end of the track (`saturating_sub`), track 0 is
65,520 samples and the sweep consumes ≈91,000, so from ~101 periods in the `obj` tail and
the whole `full` arm converted silence, not tone. `Synth::fill_frame` is now **continuous**:
at the end of a track the source **rolls to the next synthetic track** (four of them, track
3 → track 0), restarting `pos` and `phase` at 0 so a rolled track is byte-identical to a
`start_track(next)` re-seek (pinned by a host test). Rolling was chosen over wrapping inside
the track because it keeps every `TRACK_SAMPLES` length an observable boundary — `pos`
reaches it exactly, then the track index changes — and the seam is one compare, no divide.
Nothing else in the probes moved: the skip sweep re-seeks per skip and never leaves tracks
0↔1 (see the skip section), `position.rs` reads manifest lengths, and `Chunk::Tail` comes
from `RingFeed` + `probe_hw::finish()`, not from the source's `real` count. The host suite
now refuses an all-zero frame over 200 chunks (112,000 samples — past one track and past the
whole sweep): `synth_never_runs_out_past_a_track_end`.

### Video period (§4.5) — audio disarmed

```
video_period audio=disarmed count=59 sum=16572864 min=280896 max=280896 wrap=0 (spec ~281000)
```

**280,896 cycles/frame, min = max over 59 windows** — 59.73 Hz, 0.037% below the spec's
"~281,000". §4.5's assumption holds in mGBA. Every window identical to the cycle is an
emulator property (VBlank is synthesised exactly); expect a spread on hardware.

### One 8×8 4bpp tile write, and §7's 225 of them

```
tile_write ime-off overhead=16 first=687 second_raw=742 second_net=726 (x225 = 163350)
tile_loop audio=disarmed x225 count=225 sum=173382 min=751 max=790 wrap=0
```

**726 cycles** for one `set_tile`, net of the measured 16-cycle empty window; the second
write uses a different tileset, so agb's dedupe cannot serve it from cache. ×225 =
**163,350 = 28.5% of a 573,440-cycle period**. The whole 225-call loop reads the counter
around every call and does not subtract that overhead: mean 771 ⇒ **173,382 = 30.2% of a
period**. So §7's 120×120 box is a ~0.3-period job in this shape — inside the ≤ 1 period
§10.2 allows.

### VRAM / OBJ split (the Phase 0 exit criterion)

```
vram bg_char=64 bg_map=6144 obj_tile=32 obj_oam_shadow=1024 obj_palette=32
```

2×32 B pattern tiles, 3×2048 B background maps, one 32 B object tile, the 128-entry OAM
shadow (1,024 B), a 32 B object palette. This is the probe's own byte accounting: the text
renderer's glyph tiles are allocator-managed and deliberately not counted.

### Art upload — `art_probe`, 225 distinct tiles, one discarded warm-up leg

```
art blob: 7748 bytes, 120x120 px, 15x15 tiles = 225, banks=1, checksum=e990182e
art warmup audio=live frames=8 periods=3 feeder_sum=15101 late_max=0
art upload=live     audio=live     bytes=7748 tiles=225 whole=346057 per_tile count=225 sum=279539 min=1096 max=6998 wrap=0
art live feeder: audio=live periods=1 sum=5032 max=5035 late_max=0 underflows=0
art upload=disarmed audio=disarmed bytes=7748 tiles=225 whole=339013 per_tile count=225 sum=271905 min=1096 max=2187 wrap=0
```

| leg | whole upload | share of a period | per-tile mean (sum / 225) | worst tile |
|---|---:|---:|---:|---:|
| audio live | **346,057** | 60.3% (0.60 period) | 1,242 | 6,998 |
| audio disarmed | **339,013** | 59.1% (0.59 period) | 1,208 | 2,187 |

Live − disarmed = **+7,044 cycles (+2.08%)**, and this round it has a named cause: the
upload now **spans one feeder period** (`art live feeder: periods=1 sum=5032`), and that
service lands inside one tile's window — the live worst tile is 6,998 against the 1,096
floor, i.e. one period service plus a tile. That is the one place this probe's "a per-tile
mean is not a worst case" warning bites (1,242 mean, 5.6× worst). §10.2 allows ≤ 1 period
for this job: measured 0.60.

**A third reading, and a caveat about comparability.** The three readings of this table are
334,686 / 333,461 (round 1), and 346,057 / 339,013 (round 2). **A fourth, taken 2026-11-15
after the §2.2 lift added `truncate_stale` to the shared `probe_hw` (so this image's layout
moved and nothing in the art path changed), reads 343,032 / 335,990 — the same 1–3% class of
address-sensitivity, and the reason the "Reproduce" verdict block quotes the newest log
rather than this table's.** The pair is **not** a clean F2 delta:
with the old zero-filling `fill_frame` restored (nothing else changed) the probe reads
334,871 / 333,676, and with the new one plus a harmless 96-byte `.rodata` pad it reads
337,583 / 330,591. All three legs are the same instructions doing the same work — what moves
is the image's layout (`.text` grew 28 bytes, `ART_BLOB` moved 0x0800c7a8 → 0x0800c7e8, and
the pad took it to 0x0800c848), so **these absolute cycle counts are address-sensitive in
mGBA at the 1–3% level and are only comparable within one run's live-vs-disarmed pair**, or
between builds whose layout did not move. The round-1 pair itself drifts ~200 cycles against
a rebuild of the same source, which is the size of that effect when the layout barely moves.

These are this probe's second reading: the upload loop used to carry its cell position as
`cell % cols` / `cell / cols` (a variable divisor, hence a call to the software divide
helper per cell on a core with no divide instruction) and §6's rule now removes it. The
pre-fix readings were 404,853 live and 396,843 disarmed; the whole leg fell 70,167 cycles
live (≈312 per cell) and 63,382 disarmed; the worst tile fell from 7,279 because the
shorter upload no longer spans a feeder period.

Blob facts: 7,748 B committed (`assets/probe_art1.bin`), 225 tiles, 1 palette bank, FNV-1a/32
checksum `e990182e`, **sha256 `4ef603efaf55404ed2bc1f94d45d20344c642877fb907e151897dc6356e20b86`**,
`rom_tiles=7200 rom_palettes=32 rom_map=450`, VRAM price **7,650 B** = 225 × 32 (tiles) +
225 × 2 (map). §7 priced a 120×120 4bpp box at 7.2 KB ROM + 7.2 KB VRAM; the measured tile
bytes are exactly 7,200, and the VRAM price is 450 B over that (the map).

### 2b PR-(2) — the render legs, `render_probe` (AP-3b)

§5.1, §5.5, §7 and §10.2 priced every UI element as arithmetic on AP-0's single-tile
upload legs. This probe is where those tags come off: it measures the app's own drawing
shape with the audio chain live, then measures the tile pool it stands up.

```
render pool audio=disarmed refs=856 distinct=460 shared=189 max_refs=45 pool=512 atlas_tiles=188 icon_tiles=16 art_tiles=256
render pool_terms atlas=188 icons=16 art=256 expected_distinct<=461
render l1_bar_cell count=60 sum=29338 min=480 max=741 wrap=0 cells=1
render l1_clock_digit count=60 sum=28968 min=480 max=538 wrap=0 cells=1
render l1_clock_low count=60 sum=28800 min=480 max=480 wrap=0 cells=1
render l1_unchanged_run count=60 sum=122188 min=2020 max=2629 wrap=0 cells=5
render l2_region_600 count=8 sum=4403499 min=536080 max=556701 wrap=0 cells=600
render l3_set_priority count=8 sum=584 min=71 max=75 wrap=0 cells=2
render l3_commit count=8 sum=1696814 min=124966 max=280360 wrap=0 cells=1
render l3_whole_switch count=8 sum=1699086 min=125258 max=280644 wrap=0 cells=1
render l3_landed flips=8 landed=8 bg0_prio=1 bg1_prio=2 (before=[1, 2, 0, 3])
render hidden_scene map_entries=480 resolves=yes
render audio periods=13 feeder_sum=65337 feeder_max=5028 late_max=0 late_over_grace=0 underflows=0
```

Means are derived host-side from the folds (the ROM prints no divide):

| leg | window | mean | per cell | share |
|---|---|---|---|---|
| L1, one changed bar cell | 1 cell | **489 c** (min 480, max 741) | — | 0.085% of a 573,440 c period |
| L1, one changed clock digit | 2 tile rows | **963 c** | 481 c | 0.168% of a period |
| L1, one *unchanged* cell | 5-cell run | **2,037 c** | **407 c** | 0.071% of a period |
| L2, whole region repaint | 600 cells | **550,437 c** (min 536,080, max 556,701) | **917 c** | 0.96 FLAC period · 2.10 lossy periods · 1.96 video frames |
| L3, the priority store | 2 calls | **73 c** | 36 c | — |
| L3, the whole switch | set_priority + show + commit | **212,386 c** (commit mean 212,102, max 280,360) | — | 0.37 FLAC / 0.81 lossy periods at the mean; the worst is one video period (1.07 lossy) |

Four findings, stated as findings rather than as confirmation:

1. **"Unchanged is near-free" is not what the hardware does.** The plan expected agb's
   `set_tile_at_pos` early-out to make a no-op write nearly free; it is **83% of a changed
   cell** (407 vs 489 c). The source says why: the early-out compares the map entries only
   *after* `add_tile` on the new tile and `remove_tile` on the old one
   (`regular_background.rs:360-378`), so the allocator's hash lookup and the reference
   count are always paid. What the changed-cells rule actually saves is the map store and
   the dirty mark. It is still the right rule — the clock is the measured case: two changed
   cells cost **963 c** where an 11-cell full clock redraw costs 11 × ~481 ≈ **5,291 c**, a
   5.5× saving — but "costs refcount churn and nothing else" should not be sold as
   "nothing".
2. **A whole-region repaint is 0.96 FLAC periods / 2.10 lossy periods**, and 600 calls in
   one window is 1.96 video frames. §5.1's arithmetic put this row at 1.7–2.8 lossy periods;
   the measurement lands inside that band. §5.1's conclusion stands: no whole-region
   rewrite during playback, and the 600-cell number is an *event* price.
3. **A scene switch is a commit price, not a map rewrite**: the `set_priority` store itself
   is **73 c** (two calls), exactly the "tens of cycles" §5.1 assumed, and the switch's real
   cost is the commit that makes the priority word land — mean 212,102 c, worst 280,360 c
   (one video period). That is the price of an event paid where the audio is re-primed
   anyway; the flips were read back from `BGxCNT` (`landed=8` of 8), so the ordering claim
   is witnessed rather than assumed.
4. **The shared atlas dedupes, and a total-only check would not have seen it.** The stand-up
   is art (256 tiles) + atlas (188) + icons (16) over four backgrounds: **856 cell
   references to 460 live slots**, 189 of those slots shared by two or more cells, and one
   glyph slot referenced **45 times**. If the atlas had fallen back to per-cell allocation,
   `distinct` would equal `refs` (856) and the pool would have overflowed 512 — the failure
   mode the plan warned a total-only check reads as a pass. 460/512 = **89.8%** of the
   regular pool; variant B's *declared* audit is 462 at its ceiling, and the probe is a
   different stand-up, so the two are comparable in shape rather than equal by construction.
   The hidden scene's 480 map entries and their tile words are unchanged after the
   switches (`resolves=yes`), which is the §5.1 residency risk checked rather than argued.

Zero underflows and `late_max=0` across 13 audio periods.

### 2b PR-(2) — the presentation arms, `present_probe` (AP-3c)

§5.7 splits "OBJ vs BG" into presentation timing and storage. This probe measures the
timing axis with three arms, all running the same producer and the same live chain:
**A** = the 2a product's shape (three BGs, an event-scoped `commit` when a clock digit
changes ~1 Hz); **B** = the chrome as 56 OBJ entries, a shadow presented by DMA3 at VBlank,
a sparse 2-entry update per tick, **no main-loop commit**; **C** = B under a 60 Hz synthetic
chrome load (all 56 entries rewritten every frame, one 1 KiB DMA per VBlank).

```
present arm=A pass count=180 sum=9147962 min=178 max=121956 wrap=0 work_sum=4257 commit_sum=36665 due=3 commits=3
present feeder arm=A audio=live periods=87 sum=442827 max=5133 late_max=0 late_over_grace=0 underflows=0
present vblank arm=A frames=0 dmas=0 sum=0 max_any=0
present obj_budget atlas_tiles=188 icon_tiles=16 oam=56 of=128 vpal_banks=1 shadow_bytes=1024 per_line_max=30
present arm=B pass count=180 sum=9246871 min=353 max=104208 wrap=0 work_sum=21144 commit_sum=0 due=3 commits=0
present feeder arm=B audio=live periods=88 sum=453412 max=5214 late_max=0 late_over_grace=0 underflows=0
present vblank arm=B frames=3 dmas=6 sum=1419 max_any=473
present arm=C pass count=180 sum=11414487 min=12755 max=116648 wrap=0 work_sum=2283448 commit_sum=0 due=0 commits=0
present feeder arm=C audio=live periods=88 sum=461685 max=5255 late_max=0 late_over_grace=0 underflows=0
present vblank arm=C frames=179 dmas=179 sum=362622 max_any=2077
present compare a_pass_sum=9147962 b_pass_sum=9246871 c_pass_sum=11414487 a_commit_sum=36665 a_commits=3 b_commits=0 c_commits=0 period_cycles=573440
```

| arm | pass mean (180 frames) | commits | UI work | VBlank ISR | audio periods | lateness / underflows |
|---|---|---|---|---|---|---|
| A, event commit | 50,822 c | 3 (mean **12,222 c**) | 4,257 c total | — | 87 | 0 / 0 |
| B, OBJ + sparse | 51,371 c | **0** | 21,144 c total (3 ticks) | 3 presents, 6 × 8 B DMAs, **473 c max** | 88 | 0 / 0 |
| C, OBJ at 60 Hz | **63,414 c** | **0** | 2,283,448 c (12,686 c/frame) | 179 full 1 KiB DMAs, **2,026 c mean / 2,077 max** | 88 | 0 / 0 |

**The verdict is a negative one, and it is the answer §5.7 asked for.** Arm A is clean: at
its declared ~1 Hz cadence the commit costs 12,222 c (2.1% of a period), takes agb's fast
path rather than the VBlank halt — exactly what §5.7 says a commit every few seconds does —
and the producer never misses (`late_max=0`, no underflows across 87 periods). So AP-3c does
**not** force the OBJ chrome inside 2a; §5.7's "otherwise it waits for 2b" applies, and 2b
PR-(5) is the implementation PR, not an emergency. Arm B shows there is nothing to win at
this cadence either: its pass mean is +1.1% of A's, inside the run's noise, because a 1 Hz
commit that does not halt is already cheap.

What B and C *do* establish is the path's price for the thing the product has not built
yet. A sparse tick costs one 8-byte DMA per changed cell and **473 c** of VBlank service; a
60 Hz moving chrome costs **12,686 c/frame** in the main loop plus **2,026 c/frame** in the
VBlank handler (~5.2% of a video frame together), with zero underflows and zero lateness.
That is the ceiling AP-6's viz budget and 2b's badge/scroll work should be priced against —
and the 56-entry reservation is printed with it: 188 + 16 OBJ tiles (~6.4 KiB of the
separate 32 KiB pool), 56 of 128 OAM, 1 of 16 VPAL banks, 30 sprites on the title line and
26 on the chrome row.

**Hardware is still AP-7a's.** These are mGBA numbers on this host; the VBlank ISR's cost
is the one an emulator is least entitled to predict, which is why it is measured here as a
fold rather than assumed from §5.7's "~256 cycles at 32-bit".

### Skip sweep — `skip_probe`, 20 skips per setting × 4 arms, audio live, fade 256

§4.4 rules that a skip **drops** the queue ("the queued chunks are dropped, not played").
Round 1's probe only ever *drained* it, from a queue 2–16 chunks deep, which is neither
§4.4's policy nor the shipped depth. Both policies are measured now, at both depths, and
every log line names its arm (`policy=drain|drop`, `queue=shallow|deep`) so a row cannot be
read as another arm's:

| arm | §4.4 step 2 | queue when the skip starts |
|---|---|---|
| `drain` / `shallow` | the producer stops; the queued chunks **play out** to `ready() == 0` (round 1's implementation) | `prime_chunks` chunks |
| `drop` / `shallow` | the feeder **consumes and discards** each queued chunk: `take_chunk` still runs, the bytes handed to the DMA are silence | `prime_chunks` chunks |
| `drain` / `deep` | as `drain` / `shallow` | 35 of 36 chunks + the fade-out push = **36** — the 36-chunk ring's maximum depth, a ceiling; the shipped producer settles at `RING − 2,048` ≈ 32 chunks (unmeasured, see NOT VERIFIED) |
| `drop` / `deep` | as `drop` / `shallow` | 36 chunks |

`drop` is implemented in `probe_hw`'s feeder, not in the ring: the producer records the
ring frontier when it marks the queue stale, and every chunk the feeder takes below that
frontier is consumed (so `consumed` advances and the DMA gets a whole buffer per period) and
then overwritten with zeros. The window closes itself when the cursor passes the frontier,
and the producer re-primes immediately — each push waits for the discard that frees its
chunk — so **no `RingFeed` method was added and none was needed**.

Three quantities per row, all read off the bytes the DMA was actually handed, by the
feeder's wire watch (`first all-zero chunk after the skip`, then `first non-zero chunk after
that`). They are the *same* measurement in every arm, which is what makes the
drain-vs-drop comparison a comparison:

- `old_periods` — periods from the skip until the first silent chunk on the wire: how long
  the **old track's audio kept reaching the DMA**;
- `silence_periods` — the all-zero gap on the wire;
- `to_new_periods` — periods from the skip until **new-track audio reaches the DMA**:
  §4.4's "audio of the new track begins". 1 period = 34.18 ms, so §4.4's ≤ ~0.6 s is
  **17.6 periods**. A first skip of an arm can start from the bootstrap prefill rather than
  the arm's depth, which is visible as the `max` on the shallow rows below.

#### The deep arm — the shipped ring's ceiling price (the number F1 was missing)

```
skip policy=drain queue=deep queue_chunks=35 prime_chunks=4 audio=live old_periods count=20 sum=740 min=37 max=37 wrap=0
skip policy=drain queue=deep queue_chunks=35 prime_chunks=4 audio=live silence_periods count=20 sum=80 min=4 max=4 wrap=0
skip policy=drain queue=deep queue_chunks=35 prime_chunks=4 audio=live to_new_periods count=20 sum=820 min=41 max=41 wrap=0
skip policy=drop queue=deep queue_chunks=35 prime_chunks=4 audio=live old_periods count=20 sum=20 min=1 max=1 wrap=0
skip policy=drop queue=deep queue_chunks=35 prime_chunks=4 audio=live silence_periods count=20 sum=800 min=40 max=40 wrap=0
skip policy=drop queue=deep queue_chunks=35 prime_chunks=4 audio=live to_new_periods count=20 sum=820 min=41 max=41 wrap=0
skip policy=drop queue=deep queue_chunks=35 prime_chunks=4 audio=live underflows=0 starved=0 dropped=720 wait_periods_max=0 latency count=20 sum=470155087 min=23314219 max=23595183 wrap=0
skip policy=truncate queue=deep queue_chunks=35 prime_chunks=4 audio=live old_periods count=20 sum=20 min=1 max=1 wrap=0
skip policy=truncate queue=deep queue_chunks=35 prime_chunks=4 audio=live silence_periods count=20 sum=80 min=4 max=4 wrap=0
skip policy=truncate queue=deep queue_chunks=35 prime_chunks=4 audio=live to_new_periods count=20 sum=100 min=5 max=5 wrap=0
skip policy=truncate queue=deep queue_chunks=35 prime_chunks=4 audio=live underflows=0 starved=0 dropped=720 wait_periods_max=0 latency count=20 sum=114542008 min=5562822 max=5898809 wrap=0
```

Both re-read after round 2's synth fix (the source is continuous now, see the UI section):
the drain/drop deep rows are unchanged to the period — `old_periods` 37/1 (the drop `1` is
the watch's floor, not a delivery: see the table's † note), `silence_periods`
2/4/8/16 and 38/40/44/52, `to_new_periods` 39/41/45/53 — because the skip sweep never leaves tracks 0↔1
(`track + 1` / `track + N_TRACKS - 1` from 0) and both are longer than the ≤ 37 chunks a
setting queues. Only the clock-derived columns moved at all: the shallow `drop` arm at prime
8 discarded 180 chunks over 20 skips against round 1's 179 (mean 9.00, not 8.95), and
`late_max` read 4 instead of 1 (service jitter, still 12 samples inside the 16-sample grace,
`late_over_grace=0`). Every period row on this page — plus `silence_chunks_max`, `dropped`
and `wait_periods_max` — is identical to the pre-truncate round-2 image's, diffed line for
line: the truncate arm adds 4 settings and 4 × 20 × 36 = 2,880 discarded chunks to the
sweep's `dropped` tally, and it changed no shared row. The two images' **totals** are not
comparable and are not a witness: the play phase is VBlank-quantised, so the control printed
`periods=10097 feeder_sum=224062697 feeder_max=34560 wrap=0 late_max=4 late_over_grace=0` and
this image prints `periods=11000 feeder_sum=245259002 feeder_max=34563 wrap=0 late_max=4
late_over_grace=0`, its `latency` sums moving by up to a period per skip for the same reason
(this image's own code layout moved). Read the per-setting rows below, not the totals.

| policy | prime | `old_periods` (raw) | **old periods delivered** | old audio ms | silence periods | silence ms | **to_new periods** | **to_new ms** | discarded/skip | underflows |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| drain | 2 | 37 | **36** | **1,230.5** | 2 | 68.4 | 39 | 1,333.0 | 0 | 0 |
| drain | 4 | 37 | **36** | **1,230.5** | 4 | 136.7 | 41 | 1,401.4 | 0 | 0 |
| drain | 8 | 37 | **36** | **1,230.5** | 8 | 273.4 | 45 | 1,538.1 | 0 | 0 |
| drain | 16 | 37 | **36** | **1,230.5** | 16 | 546.9 | 53 | 1,811.5 | 0 | 0 |
| drop | 2 | 1 † | **0** | 0.0 | 38 | 1,298.8 | 39 | 1,333.0 | 36 | 0 |
| drop | 4 | 1 † | **0** | 0.0 | 40 | 1,367.2 | 41 | 1,401.4 | 36 | 0 |
| drop | 8 | 1 † | **0** | 0.0 | 44 | 1,503.9 | 45 | 1,538.1 | 36 | 0 |
| drop | 16 | 1 † | **0** | 0.0 | 52 | 1,777.3 | 53 | 1,811.5 | 36 | 0 |
| truncate | 2 | 1 † | **0** | 0.0 | 2 | 68.4 | **3** | **102.5** | 36 | 0 |
| truncate | 4 | 1 † | **0** | 0.0 | 4 | 136.7 | **5** | **170.9** | 36 | 0 |
| truncate | 8 | 1 † | **0** | 0.0 | 8 | 273.4 | **9** | **307.6** | 36 | 0 |
| truncate | 16 | 1 † | **0** | 0.0 | 16 | 546.9 | **17** | **581.1** | 36 | 0 |

† **One instrument reading, one convention, all three arms.** The raw column is what the log
prints, and `old_periods` is defined above as *periods from the mark until the first silent
chunk on the wire* — so its last period is already silence, and the delivered column is that
reading **minus one**, arithmetic shown: drain 37 − 1 = 36 (the whole queue — those chunks
*are* the old track), drop and truncate 1 − 1 = 0. No arm can read 0 raw: `silent_at` latches
at `at + 1` when the first post-mark chunk is silent, so 1 is the wire watch's floor, not a
delivery. That the two non-drain arms deliver nothing is a separate fact from the mechanism —
`drop` takes the stale chunk and fills the buffer with zeros before re-arming the DMA
(`probe_hw::feeder`); `truncate` steps over the queued span, the fade-out chunk included,
without delivering it. **Every row in this table satisfies `to_new = delivered + silence + 1`**
— 36 + 2 + 1 = 39, 0 + 38 + 1 = 39, 0 + 2 + 1 = 3 — which is the check that the columns are
one metric read three ways and not three metrics.

All 20 skips of every deep setting read the same `min` and `max` (the stale window is a
fixed number of chunks), so these are means that are also minima and maxima.

**The truncate arm is the one that shortens the window, and it reads `to_new = prime + 1` at
every setting** (`3/5/9/17` periods, `min == max`, 20 skips each). The 36-chunk queue
contributes nothing at all: `RingFeed::truncate` steps the consumer cursor to the producer's
frontier inside the same critical section `take_chunk` runs in, so the stale span is never
delivered — the fade-out chunk included, which is why **zero periods of old-track audio
reach the DMA** — and the cover is immediately the newest data. The silence gap is exactly
the cover (`silence_chunks_max` = 2/4/8/16 chunks = 68.4/136.7/273.4/546.9 ms), not the cover
*plus* 36 discards.

**The measurement §4.4's 1.23 s sentence is about: the old track's audio keeps reaching the
DMA for 36 periods = 1,230.5 ms** (raw `old_periods` 37 — the 36 deliveries plus the boundary
period the † note prices), identical at every prime setting because the cover is queued
*behind* the queue: the 36 chunks resident at the mark go out one per period. The chunk
already in the DMA when the mark lands was handed *before* it and is in neither column.
(`deep` fills 35 chunks and lets the fade-out push take the 36th; the product's own producer
waits for room for a 2,048-sample frame, so it settles at `RING − 2,048` ≈ 32 chunks ≈ 33
periods — the arm is the ceiling of the same regime, not a different one.)

**And the drop policy does not shorten it.** On the deep rows `to_new_periods` is *identical*
on the drain and drop rows at every setting (39/41/45/53 periods = 1.33/1.40/1.54/1.81 s) —
only the truncate row moves, and it moves all the way.
What the policy decides is what is *heard* during that window: the drain plays 36 periods of
the old track (raw 37), the drop replaces them with 38/40/44/52 periods of silence, the
truncate with the cover alone. Neither drop nor truncate lets a single period of the old track through —
the drop's buffer is zeroed before the DMA re-arms, the truncate's span is never taken — and
both watches read `old_periods` 1 only because `silent_at` is `at + 1` on all 20 skips, the
metric's floor, not a delivery (see the table's † note). **Neither drain nor drop is inside
§4.4's ≤ ~0.6 s at the deep depth — the ceiling of the shipped ~32-chunk regime, so the
shipped depth is no faster. The truncate arm is inside it at every prime setting measured
(102.5–581.1 ms).** The silence cover (`SKIP_PRIME_CHUNKS`, 2–16 periods = 68–547 ms) is a
rounding error next to the queue — until the queue is truncated, when it becomes the entire
cost.

#### The shallow arm — round 1's rows, re-derived and relabelled

| policy | prime | `old_periods` raw (mean, min/max) | old periods delivered | silence periods | to_new periods (mean, min/max) | to_new ms | discarded/skip | underflows |
|---|---:|---|---|---:|---|---:|---:|---:|
| drain | 2 | 4.65 (93/20), 4/17 | 3.65 (73/20), 3/16 | 2 | 6.65 (133/20), 6/19 | 227.3 | 0 | 0 |
| drain | 4 | 6.0, 6/6 | 5.0, 5/5 | 4 | 10.0, 10/10 | 341.8 | 0 | 0 |
| drain | 8 | 10.0, 10/10 | 9.0, 9/9 | 8 | 18.0, 18/18 | 615.2 | 0 | 0 |
| drain | 16 | 18.0, 18/18 | 17.0, 17/17 | 16 | 34.0, 34/34 | 1,162.1 | 0 | 0 |
| drop | 2 | 1 †, 1/1 | **0** | 5.7 (114/20), 5/19 | 6.7 (134/20), 6/20 | 229.0 | 3.7 | 0 |
| drop | 4 | 1 †, 1/1 | **0** | 9 | 10.0, 10/10 | 341.8 | 5 | 0 |
| drop | 8 | 1 †, 1/1 | **0** | 17.0 (340/20), 17/17 | 18.0, 18/18 | 615.2 | 9.00 | 0 |
| drop | 16 | 1 †, 1/1 | **0** | 33 | 34.0, 34/34 | 1,162.1 | 17 | 0 |

† Same convention as the deep table, and the delivered column is the raw reading minus one
*per skip* — so a mean of 4.65 over 20 skips is 3.65 delivered, its min/max 4/17 are 3/16, and
the raw sums (93, 133, …) are not the delivered sums. The `drop` rows' raw 1 is the
instrument's floor (`silent_at = at + 1`), so nothing of the old track reaches the DMA after
the mark; the `drain` rows' readings are the old track playing out.

The shallow arm's `to_new` is `queue + 1 + cover + 1` ≈ `2 × prime + 2` periods, so it reads
the *sum* of the two knobs this sweep varies. That is why the two depths are not redundant:
the shallow rows isolate the cover's cost (2–16 periods), and the deep rows read the queue
at the ring's ceiling (36 chunks — the shipped ~32-chunk depth is left unmeasured). `prime 16`
(the boot prefill's depth, which §4.4 says a skip has no cold-start reason to reuse) misses
the observable at *both* depths.

The `silence_chunks_max` line is still the longest run of all-zero chunks the feeder fed:
for `drain` rows it re-states the cover exactly (`prime` chunks × 560 = `gap_samples`, by
construction), and for `drop` rows it is the discards **plus** the cover (e.g. 36 + 4 = 40 at
`deep`, prime 4). The sweep's feeder total is
`periods=11000 feeder_sum=245259002 feeder_max=34563 wrap=0 late_max=4 late_over_grace=0` (the
20-setting sweep); that worst period
is the probe's own silence-run scan — 34,560 against the UI probe's 5,260 is the scan's
~29,000 cycles — an instrument cost, not a shipped one.

#### The ceiling, honestly: consuming-and-discarding costs one period per queued chunk

The DMA cannot be emptied faster than the hardware reads it, so *consume-and-discard* costs
exactly one period per stale chunk: at the ring's 36-chunk ceiling, 36 × 34.18 = **1,230.5
ms**. Two host tests pin that arithmetic against the real ring type
(`discarding_a_full_ring_costs_one_period_per_queued_chunk`,
`a_skip_at_the_shipped_depth_costs_the_queue_plus_the_cover`); both live in
`examples/album_probe/tests/album_probe_host.rs` and drive `album_probe::playback::RingFeed`,
i.e. the crate's type now, not a model of it.

**`RingFeed::truncate()` landed with the §2.2 lift on 2026-11-15**, so this is no longer a
sketch: the `truncate` arm in "The deep arm" above drives the shipped method and its four
`playback-test` unit tests, and the rows below are the emulator's readings rather than
the ring's arithmetic.

| prime | measured `to_new`, drain/drop | measured `to_new`, truncate | what truncate buys |
|---:|---|---:|---:|
| 2 | 39 periods = 1,333.0 ms | 3 periods = **102.5 ms** | 36 periods = 1,230.5 ms |
| 4 | 41 periods = 1,401.4 ms | 5 periods = **170.9 ms** | 36 periods = 1,230.5 ms |
| 8 | 45 periods = 1,538.1 ms | 9 periods = **307.6 ms** | 36 periods = 1,230.5 ms |
| 16 | 53 periods = 1,811.5 ms | 17 periods = **581.1 ms** | 36 periods = 1,230.5 ms |

Two things the measurement settles, and one it deliberately does not:

- **The queue's depth stops mattering.** Every truncate row is `prime + 1` periods at the
  ring's maximum depth (35 queued chunks + the fade-out's 36th). The 36-chunk term that
  dominates drain and drop is gone, not reduced.
- **§4.4's ≤ ~0.6 s holds at prime 4** — 5 periods = 170.9 ms, 12.6 periods of margin — and
  also at prime 16 (17 periods = 581.1 ms, inside by half a period). At prime 2 it is
  102.5 ms. So the answer is **not** insensitive to prime: it is linear in it
  (`to_new = prime + 1`), and the budget the observable sets only starts to bind somewhere
  past prime 16. With a truncate in the ring, `SKIP_PRIME_CHUNKS` is not a gap-budget
  question any more — what it still buys is the length of the silence the listener hears
  between tracks, which this probe cannot hear. **The constant stays un-chosen here**
  (§4.4 below): it is a Phase-1 product choice, and this PR is not the place to make it.

#### AP-0's question, adopted into `ALBUM-PLAYER.md` §15 as Q13b — including the part AP-0 got wrong

> **Q13b — may the shared feeder *truncate* the ring at a skip?** §4.4 step 2 says the
> queued chunks are "dropped, not played", but `RingFeed` offers no way to drop them:
> `consumed` is private and only `take_chunk` moves it, so the only drop the application can
> perform is *consume-and-discard*, which costs one hardware period (34.18 ms) per queued
> chunk. AP-0 measured it at the ring's ceiling depth: 36 chunks discarded = **1,230.5 ms** of
> silence before the new track can reach the DMA, so **both** available policies read
> 39–53 periods (1.33–1.81 s) against §4.4's ≤ ~0.6 s, while a truncate would read 3–17
> periods (**102.5–581.1 ms**) and come inside it at every prime setting. The change is a new
> method on `examples/flac_spike/src/playback.rs`, which is compiled into both shipped
> product images and witnessed by `make flac-check` — the codec track's file, not the
> application's, which is exactly the boundary Q13 draws. **Proposal:** the ring gains
> `truncate()` (equivalently `skip_to_producer()`) that sets `consumed = pushed` under the
> same critical section `take_chunk` uses, leaving `consumed <= pushed` and "a starved period
> consumes nothing" intact, plus a host test that a truncated ring reads `ready() == 0` and
> delivers the next push whole. Until this is ruled on, `SKIP_PRIME_CHUNKS` cannot be
> chosen: with drain-or-drop the only lever left is the queue's steady-state depth, and that
> is a decode-margin decision (a shallow queue means less room for a slow FLAC frame), not a
> skip-policy one.

**Landed 2026-11-15.** The paragraph above reads "the codec track's file, not the
application's, which is exactly the boundary Q13 draws", and that was wrong in a way worth
keeping visible: `examples/flac_spike/` is a **spike directory** — code that lives there
exists to gather information while the Rust + ASM FLAC implementation is developed, and is
meant to be thrown away. Nothing that answers to "the codec track" ever owned the ring; the
ring was simply parked somewhere it should not be depended on. `ALBUM-PLAYER.md` §2.2 now
rules that no Phase 1 module may `#[path]`-include a spike or instrument directory, and the
lift has happened: `playback.rs` and `checksum.rs` MOVED (not copied) to
`crates/gba-playback`, so the quoted `examples/flac_spike/src/playback.rs` above no longer
exists — the crate named in the table row is its home now. The
probe and both product images depend on that crate, `RingFeed::truncate()` landed on it with
four host tests, and `make flac-check` gained `make playback-test`. The skip ROM's own
`truncate` arm is the measurement the ruling owed — see the table above. What the lift did
*not* do is choose `SKIP_PRIME_CHUNKS` (§4.4 below); the `e5_fetch` cycle counter and
`geometry.rs` remain borrowed, so §2.2's debt list is shorter but not empty.

#### Where `underflows = 0` is a measurement and where it is structural

The reviewer's check, answered where it can be answered:

- **`drain` arms: a live risk, every skip.** `wait_consumed()` returns with `ready() == 0`:
  the ring is empty and one period boundary away from a `Starved` period with the producer
  still running, which is exactly what the feeder counts as an underflow. The producer
  then pushes the cover. 0 underflows in 160 drain skips is a measurement of how fast that
  push is against a 573,440-cycle period (≈5,000 cycles for a synth chunk), not a
  foregone conclusion.
- **`drop` arms: structural while the window is open.** The ring holds ~35 chunks when the
  mark lands and the producer's pushes wait for the discards that free room, so a period
  that finds the ring empty cannot happen *while stale chunks remain* — the drop arm's
  `starved=0` is a weaker statement than the drain arm's, and it is why the two are
  reported separately.
- **`truncate` arms: structural for one period, then a margin.** Step 2's
  `probe_hw::truncate_stale` truncates and pushes the cover's first chunk inside one critical
  section, so the feeder never observes an empty ring with the producer still running: it
  either took its chunk before that section (the ring was full) or it takes the cover's head
  after it. That head is what makes the first period safe by construction; the rest of the
  cover merely has to land inside the period that head buys, and a chunk push is ~1% of a
  period (5,000 cycles against 573,440), so the margin is wide but it is a margin, not a
  proof. 0 underflows in 80 truncate skips (`starved=0`, `dropped=720` per setting = 20 × 36
  whole chunks, 2,880 across the four settings) is that combination — not the drain arm's
  timing measurement, and not a structural guarantee either.
- **Structurally impossible in all three:** the fade-out push itself, which always lands in a
  non-empty ring.
- **Not witnessed at all:** whether a *real* decoder could miss a period during a skip's
  re-prime. The synth's chunk costs ~1% of a period; the shipped FLAC worst frame is 18.99%
  of one. That risk is the product's (AP-1), not this probe's — see NOT VERIFIED.

The `latency` column is the whole skip: t0 (just before the fade-out push) to after the
fade-in and the ≥3 periods of play that follow, and it is period-bounded rather than
frame-bounded. Round 1's note that the six settling frames cost ~100 ms was wrong — its own
rows imply those frames cost ~2 periods, and this probe's `gfx.frame()` returns as soon as
VBlank has happened, so a frame count is not a time budget here. `latency` can therefore be
read as "`to_new` + the settling play", and `to_new_periods` is the number the decision
belongs on.

## §10.2, target → measured

| consumer | §10.2 target/allowed | AP-0 result | who owns the number |
|---|---|---|---|
| FLAC decode, worst frame | 398,255 (18.99%) | **not re-measured here** — shipped figure carried forward | the codec record |
| feeder (convert + push + IRQ) | shipped figure, carried in | 5,238 cyc/period mean, 5,260 max (`full` arm) = **0.92%** | measured here, shipped path |
| UI: key read, position, bar + 2 text lines, palette fade | **≤ 2% of audio time** | **1.89%** (5,306 cyc/frame ⇒ 10,832 of a 573,440-cycle period; 1.92% of the closure window) | **AP-0 measured — a lower bound** |
| viz tick (1–2 ROM reads + table + tiles/sprites) | ≤ 1% | not measured | AP-6 |
| art upload (rare, on screen change) | ≤ 1 period, off the audio path | 343,032 live = **0.60 period** (335,990 disarmed; fourth reading of the same code, see the art section) | AP-0 first data point; AP-4 owns |
| game tick (≤ 12 objects) | ≤ 3%, admitted by watermark | not measured | AP-8 |
| **reserve** | **≥ 20%** | not measured | after AP-6 and AP-8 |

The UI row is the one Phase 0 exists to replace, and it lands inside the target — but as a
**lower bound**: the text is composed once at setup and only `show`n per frame, so the
measured slice prices the composed background, not glyph layout. §10.2's claim that must
survive ("`underflows = 0` **with the UI and a viz enabled**") is half-witnessed here: the
UI arm runs with `underflows=0`; the viz half is AP-6's.

Row by row, what is still a target: viz ≤ 1% (AP-6), game ≤ 3% (AP-8), the ≥ 20% reserve
(only meaningful once AP-6/AP-8 add their terms), and the art row's *AP-4* judgement (this
is its first data point, not its verdict). The FLAC 18.99% and the feeder's shipped cost
are carried in from the product log, not re-measured by a probe.

### §4.5 — the frame

`~281,000 cycles/frame` → **280,896 measured, min = max over 59 windows** (59.73 Hz). The
period is 573,440 and a 2,048-sample frame is 3.657 periods, as geometry pins them.

### §4.4 — `SKIP_PRIME_CHUNKS`: un-chosen, and no longer blocked

*Status 2026-11-15 (updated): the mechanism (truncate the ring at step 2) is ruled and
**landed** — `RingFeed::truncate()` moved with the §2.2 lift into `crates/gba-playback`, the
skip ROM's `truncate` arm measures the shipped method, and `make playback-test` witnesses it
on the host. The constant itself is still un-chosen: it is a Phase-1 product choice, and this
record is not the place to make it.*

**What changed, and what did not.** Before the truncate landed, the number §4.4's observable
turned on was the queue's depth: at the ceiling depth drain and drop both read 39–53 periods
(1.33–1.81 s) against the ≤ ~0.6 s observable, and the cover accounted for only 2–16 of
those periods (`to_new ≈ queue + 1 + cover + 1`). With the truncate measured, the queue term
is gone and `to_new = prime + 1` at every prime in the sweep: **102.5 / 170.9 / 307.6 / 581.1
ms**, all inside the observable. So the observable no longer constrains the constant; what it
still buys is the silence the listener hears between tracks, which is a musical decision and
unmeasured here.

What AP-0 can state, all measured: `underflows=0` and `starved=0` in all 400 skips across the
20 settings; the old track keeps reaching the DMA for 36 periods (1,230.5 ms) at the ceiling
depth under `drain` and for **none** of them under `drop` or `truncate`; the drop policy buys
*silence instead of old-track audio* and buys **no time at all**; the truncate buys the whole
queue's worth of time (1,230.5 ms) and leaves only the cover in front of the new track.

Round 1's `SKIP_PRIME_CHUNKS = 4` reading (4,007,200 cycles = 238.8 ms) was a **shallow
drain**, not §4.4's policy at §4.4's depth: the equivalent row here is
`drain`/`shallow`/prime 4 → 10 periods (341.8 ms) to new-track audio, and the ceiling-depth
(`deep`) row is 41 periods (1,401.4 ms).

## NOT VERIFIED

Nothing below can be witnessed headless, and this record claims no audio or video
correctness from an mGBA run.

- **Real sound.** The synthetic PCM's samples are never captured, so no waveform is
  checked: the fades, the silence cover, and whether a late-re-arm splice is inaudible are
  unheard. *Settling run:* boot `album-probe-skip.gba` on an EverDrive in a GBA 001 and a
  GBA SP with line-out recorded, 20 skips per prime setting, and listen/scope for a click
  at each seam. (The probes print through mGBA's log, so a hardware run first needs the
  numbers rendered on screen with the committed font, the `e5_fetch` pattern.)
- **Real cartridge wait states.** mGBA serves the ROM image at zero wait and the probes
  never write `WAITCNT`, so every cycle above is a best case for code, tables and the
  `include_bytes!` blob fetched from ROM. *Settling run:* the same three images on the
  same two units with `WAITCNT` at the product setting, numbers read off the screen; the
  tile-write, art-upload and feeder maxima are the terms most at risk.
- **Real bus contention.** mGBA is not a bus model; the live-vs-disarmed art delta
  (**+7,044, +2.08%**, one feeder period inside one tile's window) and the tile loop's
  751–790 spread are emulator artefacts until repeated on hardware. *Settling run:* both
  art legs on hardware, and the `full` arm's `underflows` and `late_max` under a real
  DMA/CPU arbitration.
- **Cross-build comparability of the art numbers.** The art legs move 1–3% when nothing but
  the image layout changes (measured three ways in the art section: 334,686 with round 1's
  build, 334,871 with round 1's synth in round 2's build, 337,583 with a 96-byte `.rodata`
  pad and no code change). mGBA's fetch timing is address-sensitive, so an art cycle count
  means something only inside one image. *Settling run:* the same three builds on hardware,
  which charges real wait states and will either repeat the pattern or dissolve it.
- **What is on the screen.** Objects, three simultaneous backgrounds, the palette fade and
  the ART1 card are never looked at — the ROMs print numbers about them, which is not the
  same as showing them. This includes the agb `commit`/DISPCNT trap the ui probe is built
  around. *Settling run:* `mgba` with a GUI window, boot each image, and confirm by eye (or
  a screenshot) that all three backgrounds persist every frame, the object animates, the
  fade moves, and the 120×120 card renders with its colour bars, dither ramp and tile grid.
- **The 16-sample re-arm grace.** `late_max=0` in the UI and art arms, and `late_max=4`
  with `late_over_grace=0` over the skip sweep's 11,000 periods: mGBA never delayed the
  period IRQ meaningfully, so the grace path was never exercised — the probe proves the
  *normal* path is nowhere near it, nothing more.
  *Settling run:* hardware, where IRQ latency is real, reading `late_max`/`late_over_grace`
  and `underflows` with the feeder running; a deliberately shortened period can exercise
  the window on the emulator but cannot price it.
- **What a skip sounds like.** The drop's silence and the drain's old-track playout are
  different *audible* events with the same measured length, and nothing here was heard: the
  probe can say which bytes went to the DMA, not what they sounded like. *Settling run:* the
  same skip ROM on hardware with line-out recorded, both policies, listening for the seam and
  for a click at the discard-to-new-audio boundary.
- **The truncate now has numbers, and what they still cannot say.** `RingFeed::truncate`
  landed with the §2.2 lift, so 102.5–581.1 ms is an emulator reading (`to_new` 3/5/9/17
  periods over 20 skips per prime, `min == max`) rather than the geometry's arithmetic. What
  is unverified is the part no counter can reach: whether stepping the consumer cursor over
  a queued span sounds like a clean seam or like the track starting late, which needs the
  recorded-line-out run below. **And the skip this arm measures is a hard cut, not a faded
  one:** `truncate_stale` steps over the fade-out chunk along with the rest of the queue, so
  no fade is delivered, and the chunk already in the DMA when the mark lands is spliced
  straight into silence. §4.4's observable row — "Clicks / pops at the transition: none:
  fades, never a mid-waveform splice" — is therefore **not** met by the mechanism this PR
  lands; it is met by the sequence, and the splice at a truncate is AP-1's to hear (or to
  re-shape, by truncating to the fade's end rather than to the producer's frontier). The four `playback-test` host tests witness the ring's side of
  it only (`ready() == 0`, `consumed() == pushed()`, the next frame delivered whole).
- **A real decoder during a skip's re-prime.** The probe's producer is a synth chunk that
  costs ~5,000 cycles against a 573,440-cycle period, so "the producer keeps up while the
  queue is discarded and re-primed" is a property of the probe. The shipped FLAC worst frame
  is 18.99% of a period, and whether *that* survives a skip's re-prime is AP-1's witness, not
  this one's.
- **The product's own queue depth.** "The shipped producer settles at `RING − 2,048` ≈ 32
  chunks" is read from `product_play_main.rs::wait_for_room`'s demand, not measured by a
  probe at AP-0. *Settling run:* a `[pp-rom]` line printing `ready()` at a skip would make it
  a measurement; the deep arm here is the ceiling of the same regime.
- **Hardware cycle fidelity generally.** One emulator, one host, one tree state. The
  video period's exact equality, the zero-wait fetches and the deterministic IRQ timing are
  mGBA properties; re-running on the two units is what turns these into hardware numbers.

## Scope notes and caveats

Deliberately **not built** in Phase 0 (per the brief): no `album_player` crate, no screens
or UI framework, no bake tool, no `PcmSource`/decoder pairing, no `ALVZ1` viz, no game, no
`ALBM1` manifest, no PNG/PIL art pipeline. The art card is procedural, the PCM is
synthesised, the skip's source is the same synth — every one of those is a stand-in chosen
so a probe can be thrown away.

Caveats that qualify the numbers:

- **The art live-vs-disarmed delta is n=1 per leg, on one emulator.** The first live reading
  was +158k cycles (+39%) because agb's cold VRAM allocator ran inside the measured window;
  a discarded warm-up upload now runs first, and round 2's figure is +7,044 (+2.08%), of
  which one feeder period (5,032) is a named cause and the rest is the layout sensitivity
  above. One sample, one emulator, warm allocator — an artefact to re-measure, not a budget
  line.
- **Drain and drop read almost the same latency and different audio; truncate reads both
  different.** Measured, not argued: `to_new_periods` is *identical* on the drain and drop
  deep rows (39/41/45/53) and on the
  shallow rows at primes 4/8/16 (10/18/34), and within one period at shallow prime 2 — drain
  mean 6.65 (sum 133, max 19) against drop 6.7 (sum 134, max 20), a difference the first
  skip's bootstrap prefill explains, not the policy — because consume-and-discard costs one
  period per queued chunk exactly as playing it does. The drop's contribution is that the
  window is *silent* instead of the old track. The truncate arm was measured at the deep
  depth only (that is the ceiling §4.4's row is about), and there it reads 3/5/9/17 periods —
  the one place the policy itself moves `to_new`.
  Round 1's "plus ~100 ms of settle" note was wrong: the settling frames cost ~2 periods
  there, and the play phase is now bounded in periods (≥ 3) instead of frames, so `latency`
  reads `to_new` plus that play.
- **The cycle counter is soft-reset between settings** (`Counter::soft_reset`, the
  instrument's own invariant). Without it one deep-drop window straddled the 32-bit
  counter's 256 s period in a 342 s sweep and was dropped (`wrap=1`); with it no window can.
  `feed_wrapped=0` on the totals line is the feeder's own half of the same check.
- **The UI number is a lower bound** (text composed at setup), and the `none` arm's 295
  cycles/frame is instrument floor, not work.
- **The UI probes' `obj` and `full` arms fed silence partway through in round 1.** The
  synth never left track 0 (65,520 samples = 117 chunks) while the sweep consumes ≈91,000,
  so `Synth::fill_frame` ran off the end and zero-filled from ~101 periods in — inside the
  `obj` arm — and the whole `full` arm converted silence. **Fixed in round 2** (the source
  rolls to the next track, see "the source no longer runs dry" above) and both readings are
  quoted side by side there: the per-frame UI base and the feeder's per-period cost did not
  move (≤ 1.0% on the feeder, ≤ 3 cycles/frame on the UI base, the raw arm totals differing
  by approximately one interleaved feeder service each), so round 1's *numbers* were sound even
  though what they measured in those two arms' tails was not tone. What round 1 got wrong is
  what a listener would have heard, and nothing here was heard either way.
- **The VRAM/OBJ split is the probe's byte accounting**, not a runtime query, and omits the
  text renderer's allocator-managed glyph tiles.
- **The skip sweep's feeder max (34,560) is a probe instrument** (the silence-run scan), not
  a shipped cost; the shipped-feeder figure is the UI probes' 5,260 worst period.
- **`late_max=0` is one emulator's ordering**, not proof the grace is redundant. The skip
  sweep's total reads `late_max=4 late_over_grace=0` over 11,000 periods: four services
  entered up to 4 samples into their period, all twelve samples inside the grace. The marks are
  cumulative across the sweep (they are not part of `reset_marks`'s per-setting reset), so
  that is the whole run, not the last setting.
- **`wait_periods_max=16` at `drain`/`shallow`/prime 2** is the first skip playing out the
  bootstrap's 16-chunk prefill, not a property of a 2-chunk ring. The same artefact shows on
  the drop rows at that setting as `silence_periods` max 19 (`dropped` 17 + the 2-chunk
  cover). `wait_periods_max=0` on every drop row is by construction: the drop never blocks
  on the queue, so `dropped` (36 at the deep depth) is that window's length instead.

## Cleanup

`make clean-outputs` removes `album-probe*.log` along with the images and `.sav` files. The
witness that stays is the committed generator output — `assets/probe_art1.bin` and
`src/art_pin.rs`, tracked, byte-identical on regeneration (`make album-probe-test` refuses
if they are not).

That tracking is a **development instrument**, not a precedent. ALBUM-PLAYER.md §2.3 sorts by
what an asset is for: the bytes a gate exists to read — a host gate whose whole job is these
exact bytes, from a deterministic in-tree generator — are tracked, and the bytes that belong to
a particular album or a particular generated ROM are not. **Phase 1's album audio is in that
second class:** its per-track `GAFP` blobs, `ALBM1` and `src/assets.rs` are generated at build
time from a tracked track list and gitignored, exactly as `make master-pack` already treats
`master.gfp` and `master_pin.rs`. Nothing about this probe's blob should be read as permission
to commit one.
