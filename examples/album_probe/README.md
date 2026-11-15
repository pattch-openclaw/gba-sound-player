# `album_probe` — AP-0, the Album Player's Phase 0 probe record

AP-0 is the measurement pass `ALBUM-PLAYER.md` §13 opens with: three throwaway ROM
images and one host gate that replace §10.2's target percentages with witnesses, and give
§4.5's "~281,000 cycles per frame, to be confirmed by measurement" and §4.4's
`SKIP_PRIME_CHUNKS` their first numbers.

**What this directory is:** the UI spike (3 backgrounds, one 8×8 object, text, a palette
fade, VBlank-paced, with the shipped lossless feeder playing synthetic PCM underneath),
the art spike (a 120×120 `ART1` card on screen from the committed blob), the skip spike
(fade → mark stale → re-seek → silence cover → re-prime → fade in, against the real ring,
swept over *two queue policies* — drain and drop — at *two queue depths*), and the host
suite that pins the ART1 reader, §6's arithmetic, the session clock, the ring geometry,
and what §4.4's drop costs on the shipped `RingFeed`. Each ROM prints `[album-probe] …` lines, a `PASS` verdict, and the runner's
`Tests finished successfully` sentinel.

**What it is not:** a probe is a witness, never a gate input (the `ctor-probe` rule).
There is no `album_player` crate here, no product code, no screens, no bake tool, no
`PcmSource`, no ULI2; nothing here is wired into `flac-check` or `test`, and `album-check`
(AP-0…AP-9) does not exist yet. The shared code is consumed by `#[path]` include only —
`geometry` from `examples/flac_integration`, the `playback` + `checksum` pair from
`examples/flac_spike`, the timer2/3 `Counter` from `examples/e5_fetch` — so no file under
`crates/` or the other `examples/` changed to build this.

Two of those three sources are **spike and instrument directories**, which is a debt this
crate takes on purpose and the plan refuses to carry forward: `flac_spike` and `e5_fetch`
exist to gather information while the FLAC implementation is developed, and an
application that outlives that work cannot be founded on them. `ALBUM-PLAYER.md` §2.2
rules the lift (into project source, proposal `crates/gba-playback`) and makes it Phase 1
work — before any `album_player` module links any of it. Phase 0 borrows the shipped
`RingFeed` precisely *so that* its skip numbers are statements about the ring the product
runs on rather than a model of one; moving that ring is the follow-up, not this PR's job.

## Reproduce

```sh
python3 scripts/album_probe_art.py --self-test   # generator refusal paths + audit
make album-probe-art         # regenerate the tracked blob + pin (byte-identical)
make album-probe-test        # the host half: 52 tests
make native-album-probe-rom  # three .gba images + .text/.iwram/.bss/.rodata
make album-probe             # boot all three headless, one log each
make check                   # every workspace rustfmt-clean
make flac-check              # the shipped product's suite, unaffected
```

`make album-probe` writes `album-probe-ui.log`, `album-probe-art.log` and
`album-probe-skip.log` at the repository root; `make album-probe-test` writes
`album-probe-test.log`. Every number below is copied from the three ROM logs, and each
table's log line is quoted where the columns need it. The three verdicts:

```
[album-probe] PASS ui arms=5 audio=live=yes video=disarmed tile=ime-off underflows=0
[album-probe] PASS art tiles=225 live=346057 disarmed=339013 period=573440
[album-probe] PASS skip policies=[drain, drop] queues=[shallow, deep] primes=[2, 4, 8, 16] underflows=0 audio=live
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
334,686 / 333,461 (round 1), and 346,057 / 339,013 now. The pair is **not** a clean F2 delta:
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
skip policy=drop queue=deep queue_chunks=35 prime_chunks=4 audio=live underflows=0 starved=0 dropped=720 wait_periods_max=0 latency count=20 sum=470155078 min=23314207 max=23595211 wrap=0
```

Both re-read after round 2's synth fix (the source is continuous now, see the UI section):
the deep rows are unchanged to the period — `old_periods` 37/1, `silence_periods` 2/4/8/16 and
38/40/44/52, `to_new_periods` 39/41/45/53 — because the skip sweep never leaves tracks 0↔1
(`track + 1` / `track + N_TRACKS - 1` from 0) and both are longer than the ≤ 37 chunks a
setting queues. Only the clock-derived columns moved at all: the shallow `drop` arm at prime
8 discarded 180 chunks over 20 skips against round 1's 179 (mean 9.00, not 8.95); the sweep
total is now `periods=10097`; and `late_max` read 4 instead of 1 (service jitter, still 12
samples inside the 16-sample grace, `late_over_grace=0`).

| policy | prime | old audio periods | old audio ms | silence periods | silence ms | **to_new periods** | **to_new ms** | discarded/skip | underflows |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| drain | 2 | **37** | **1,264.6** | 2 | 68.4 | 39 | 1,333.0 | 0 | 0 |
| drain | 4 | **37** | **1,264.6** | 4 | 136.7 | 41 | 1,401.4 | 0 | 0 |
| drain | 8 | **37** | **1,264.6** | 8 | 273.4 | 45 | 1,538.1 | 0 | 0 |
| drain | 16 | **37** | **1,264.6** | 16 | 546.9 | 53 | 1,811.5 | 0 | 0 |
| drop | 2 | 1 | 34.2 | 38 | 1,298.8 | 39 | 1,333.0 | 36 | 0 |
| drop | 4 | 1 | 34.2 | 40 | 1,367.2 | 41 | 1,401.4 | 36 | 0 |
| drop | 8 | 1 | 34.2 | 44 | 1,503.9 | 45 | 1,538.1 | 36 | 0 |
| drop | 16 | 1 | 34.2 | 52 | 1,777.3 | 53 | 1,811.5 | 36 | 0 |

All 20 skips of every deep setting read the same `min` and `max` (the stale window is a
fixed number of chunks), so these are means that are also minima and maxima.

**The measurement §4.4's 1.23 s sentence is about: the old track's audio keeps reaching the
DMA for 37 periods = 1,264.6 ms** — 36 queued chunks plus the period in flight when the mark
lands, identical at every prime setting because the cover is queued *behind* the queue.
(`deep` fills 35 chunks and lets the fade-out push take the 36th; the product's own producer
waits for room for a 2,048-sample frame, so it settles at `RING − 2,048` ≈ 32 chunks ≈ 33
periods — the arm is the ceiling of the same regime, not a different one.)

**And the drop policy does not shorten it.** On the deep rows `to_new_periods` is *identical*
on the drain and drop rows at every setting (39/41/45/53 periods = 1.33/1.40/1.54/1.81 s).
What the policy decides is what is *heard* during that window: the drain plays 37 periods of
the old track, the drop replaces them with 38/40/44/52 periods of silence. The `1` on the
drop rows is the period in flight when the mark lands — inferred from the delivery order,
not measured on the wire: the drop arm's watch never observes a non-silent chunk, so
`silent_at` is `at + 1` on all 20 skips. **Neither policy is inside §4.4's ≤ ~0.6 s at the
deep depth — the ceiling of the shipped ~32-chunk regime, so the shipped depth is no
faster.** The silence cover (`SKIP_PRIME_CHUNKS`, 2–16 periods = 68–547 ms) is a rounding
error next to the queue.

#### The shallow arm — round 1's rows, re-derived and relabelled

| policy | prime | old audio periods (mean, min/max) | silence periods | to_new periods (mean, min/max) | to_new ms | discarded/skip | underflows |
|---|---:|---|---:|---|---:|---:|---:|
| drain | 2 | 4.65 (93/20), 4/17 | 2 | 6.65 (133/20), 6/19 | 227.3 | 0 | 0 |
| drain | 4 | 6.0, 6/6 | 4 | 10.0, 10/10 | 341.8 | 0 | 0 |
| drain | 8 | 10.0, 10/10 | 8 | 18.0, 18/18 | 615.2 | 0 | 0 |
| drain | 16 | 18.0, 18/18 | 16 | 34.0, 34/34 | 1,162.1 | 0 | 0 |
| drop | 2 | 1.0, 1/1 | 5.7 (114/20), 5/19 | 6.7 (134/20), 6/20 | 229.0 | 3.7 | 0 |
| drop | 4 | 1.0, 1/1 | 9 | 10.0, 10/10 | 341.8 | 5 | 0 |
| drop | 8 | 1.0, 1/1 | 17.0 (340/20), 17/17 | 18.0, 18/18 | 615.2 | 9.00 | 0 |
| drop | 16 | 1.0, 1/1 | 33 | 34.0, 34/34 | 1,162.1 | 17 | 0 |

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
`periods=10097 feeder_sum=224062697 feeder_max=34560 wrap=0 late_max=4 late_over_grace=0`; that worst period
is the probe's own silence-run scan — 34,560 against the UI probe's 5,260 is the scan's
~29,000 cycles — an instrument cost, not a shipped one.

#### The ceiling, honestly: consuming-and-discarding costs one period per queued chunk

The DMA cannot be emptied faster than the hardware reads it. `RingFeed` cannot truncate —
`consumed` is private and only `take_chunk` moves it — so every implementation of "dropped,
not played" that the *application* can write costs exactly one period per stale chunk, and
at the ring's 36-chunk ceiling that is 36 × 34.18 = **1,230.5 ms**. Both host tests pin the arithmetic
against the real ring type (`discarding_a_full_ring_costs_one_period_per_queued_chunk`,
`a_skip_at_the_shipped_depth_costs_the_queue_plus_the_cover`). That test name says "shipped
depth" and means the ceiling; the identifier is the one older than the correction in
"The deep arm" above, and the README's prose is what should be believed.

A **truncate** — advancing the consumer cursor past the queue in one step — would delete
exactly that window, leaving only the cover in front of the new track:

| prime | measured `to_new` (both policies) | with a truncate | what a truncate buys |
|---:|---|---:|---:|
| 2 | 39 periods = 1,333.0 ms | 3 periods = **102.5 ms** | 36 periods = 1,230.5 ms |
| 4 | 41 periods = 1,401.4 ms | 5 periods = **170.9 ms** | 36 periods = 1,230.5 ms |
| 8 | 45 periods = 1,538.1 ms | 9 periods = **307.6 ms** | 36 periods = 1,230.5 ms |
| 16 | 53 periods = 1,811.5 ms | 17 periods = **581.1 ms** | 36 periods = 1,230.5 ms |

Every truncate row is inside §4.4's observable; every measured row is outside it. The
truncate would need a new method on `examples/flac_spike/src/playback.rs`, a file whose own
docs say changing it changes both shipped product images and whose witness is
`make flac-check` — **so this change was not made**, and the `SKIP_PRIME_CHUNKS` decision is
**blocked** on the owner ruling below. **Ruled 2026-11-14: truncate is the mechanism.**
Ruled again on 2026-11-15 about *whose* ruling it was: the blocker is now
`ALBUM-PLAYER.md` §2.2's lift of this feeder out of a spike directory into project
source, which this application owns. The value stays un-chosen here; nothing is
waiting on another track.

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

**Corrected 2026-11-15.** The paragraph above reads "the codec track's file, not the
application's, which is exactly the boundary Q13 draws", and that was wrong in a way
worth keeping visible: `examples/flac_spike/` is a **spike directory** — code that lives
there exists to gather information while the Rust + ASM FLAC implementation is
developed, and is meant to be thrown away. Nothing that answers to "the codec track"
ever owned the ring; the ring is simply parked somewhere it should not be depended on.
`ALBUM-PLAYER.md` §2.2 now rules that no Phase 1 module may `#[path]`-include a spike or
instrument directory, and that `playback.rs` (with `checksum.rs`, and the `e5_fetch`
cycle counter this probe also borrows) is lifted into project source first — proposal:
`crates/gba-playback`. **`truncate()` lands on the lifted type, and the application owns
both moves.** The lift is a measured change rather than a rename: `playback.rs` is
compiled into both shipped product images, so moving it re-baselines their bytes, and its
unit suite has to travel with it or `make flac-check` quietly stops witnessing it. That
work is deliberately out of this PR — see "What this directory is not" above.

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
- **Structurally impossible in both:** the fade-out push itself, which always lands in a
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
| art upload (rare, on screen change) | ≤ 1 period, off the audio path | 346,057 live = **0.60 period** (339,013 disarmed) | AP-0 first data point; AP-4 owns |
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

### §4.4 — `SKIP_PRIME_CHUNKS`: **blocked**, not chosen

*Status 2026-11-15: the decision (truncate the ring at step 2) is ruled, and so is the
ownership — `ALBUM-PLAYER.md` §2.2 lifts the feeder out of `examples/flac_spike/` into
project source that this application owns, and `truncate()` lands with it. The value is
still un-chosen because that lift has not happened yet, not because another track has to
agree.*

**No value is chosen, because the number §4.4's observable turns on is the queue's depth, not
the cover.** At the ceiling depth both available policies read 39–53 periods (1.33–1.81 s)
to new-track audio against the ≤ ~0.6 s observable, and the cover accounts for only 2–16 of
those periods. The shallow rows price the cover by itself (2–16 periods = 68–547 ms) and put
the depth term beside it: `to_new ≈ queue + 1 + cover + 1`. To come inside the observable
with drain-or-drop the queue would have to be ≤ 13 chunks at prime 2 (`17.6 − 2 − 2`), which
is a decode-margin decision (a shallow queue is less room for a slow FLAC frame), not a
skip-policy one. With a truncate, every prime in the sweep fits: 102.5 / 170.9 / 307.6 /
581.1 ms.

The decision is therefore blocked on **Q13b** (proposed above, beside Q13 in §15 — the same
"where does the codec track end and the app begin" boundary). What AP-0 can state now, all
measured: `underflows=0` and `starved=0` in all 320 skips across the 16 settings; the old
track keeps reaching the DMA for 37 periods (1,264.6 ms) at the ceiling depth; the drop
policy buys *silence instead of old-track audio* and buys **no time at all** under the
shipped `RingFeed`; and the cover is the small term.

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
  with `late_over_grace=0` over the skip sweep's 10,097 periods: mGBA never delayed the
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
- **The truncate's numbers are arithmetic, not a measurement.** A `RingFeed` truncate was
  not implemented (Q13b), so 102.5–581.1 ms is what the ring's own geometry says a
  one-step cursor advance would leave in front of the new track — host-side arithmetic, not
  an emulator reading, and it assumes nothing else changes (the cover and fade-in still have
  to be pushed).
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
- **The skip's two policies read almost the same latency and different audio.** Measured,
  not argued: `to_new_periods` is *identical* on the deep rows (39/41/45/53) and on the
  shallow rows at primes 4/8/16 (10/18/34), and within one period at shallow prime 2 — drain
  mean 6.65 (sum 133, max 19) against drop 6.7 (sum 134, max 20), a difference the first
  skip's bootstrap prefill explains, not the policy — because consume-and-discard costs one
  period per queued chunk exactly as playing it does. The drop's contribution is that the
  window is *silent* instead of the old track.
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
  sweep's total reads `late_max=4 late_over_grace=0` over 10,097 periods: four services
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
