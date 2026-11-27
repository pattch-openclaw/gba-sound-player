# ALBUM-PLAYER.md — proposal: the Album Player application

Status: **proposal opened 2026-11-14; Phase 0 (AP-0) landed the same day, Phase 1 has since
landed through PR-3c, and the plan has been reviewed, re-ruled and corrected many times since** —
the five probe ROMs are built, run and measured, and their numbers live in
[`examples/album_probe/README.md`](examples/album_probe/README.md); §11's phase log and §13's
gate rows are the record of what has been built since (the pack step, the seam, the first ROM
that plays a track, the transport and keys, the resume refill — and **AP-1 passing on the
instrument that opened it**, with four hardware legs recorded at AP-7a). **The next unclaimed
work is §11's Phase 2, split into 2a and 2b on 2026-11-20: 2a is the surface itself (A5, A7 →
AP-3: the tracked layout spec, the generated `src/layout.rs`, the persistent chrome, one `blank`
scene, and a ROM a person can play on it); 2b is what goes in the region (A8, A11, A12 → AP-4:
art, the track-list scene, the title-card scene)**, which also carries the
two things PR-3c handed it (§5.3). **2a's first PR is scoped in the note under §11's phase table
(2026-11-20), and the one refusal inside AP-3 whose *input* 2a does not parse — `art-box-not-derived`
— is named there, in §9.0's register and in §13's AP-3 row rather than left to be rediscovered.
2b's tool-first ladder was scoped 2026-11-25, and its **PR-(1) — the per-scene schema, the
resident-set walk and the machine-readable report, with no new pixels — has landed** (§11's 2b
note), as has **PR-(2), AP-3b's four render legs and AP-3c's three presentation arms, with their
measured coefficients checked into the generator** (§11's 2b note), and **PR-(3), the title card —
the schema's first consumer, `shares_image`, the checked scene registry and `Select`'s scene cycle**
(§11's 2b note; Q20 (ii)/(iii) answered in §15). The next unclaimed step is PR-(4), album art.**
It is a plan to be argued with, a scope
statement, and a budget the first probe ROM has to contradict usefully — and it did:
AP-0 refuted one row of §4.4's observable table, and the fix is recorded there rather
than quietly applied. Seven things have already been settled
in review and are recorded inline rather than left open: **a skip is allowed a
short, bounded delay (§4.4)**; **a skip drops the queue by *truncating* the ring, and
the application grows that method on its own feeder once §2.2's lift lands (§4.4,
Q13b — ruled 2026-11-14 on AP-0's measurement)**; **the session clock is one `u32` in `.iwram`
(§10.4)**; **what this application waited on was the lossy backend's real-time decode (U4), then
its playback (U5) and output stage (PR D) — and **that wait is over: all three are met as of this
checkout** (U4 with 13.73 points of margin at the worst frame, U5 on a GBA SP with zero underruns
across three player ROMs, PR D shipped as `crates/uli-playback`; §2, §10.2, §11, R1, re-read
2026-11-19 against [`ULI2.md`](ULI2.md) §Gates, which supersedes the "not met — 105% worst"
reading this file carried from the 2026-11-15 ledger). What an album-length lossy build now waits
on is **app-side work and assets**, not a codec: the asset targets still run the oracle (W2), the app has no ULI2 backend (W3), and this
application's own UI cost inside a measured margin (R1b)**; **a generated ROM's
assets are build output, never inventory — a tracked track list plus the already shipped
`master-pack` flow generate the blobs, the manifest and the pin module at build time, while the
development instruments and the reference master stay tracked (§2.3, ruled 2026-11-15)**; and
therefore **the bake tool is split across two phases:
packing a declared track list is Phase 1 work, its verdict set stays in Phase 3 (§3 A3a/A3b,
§9.0, §11)**; and **the interface is one surface, not five screens: a persistent bottom chrome
of transport glyphs and an `elapsed / total` clock, a title row that carries the progress bar,
and above them a region where swappable *scenes* live — with all geometry declared in a tracked
layout spec and generated into compile-time constants, so the runtime is never a configurable
GUI (§5, ruled 2026-11-20 in review of this file; it supersedes the five-screen / "visual in the
top 96 px" plan of §5 and the 160×96 art box of Q7, while leaving every measurement in this file
exactly where it was)**. Every number below is either
(a) transcribed from this repository with its source named, (b) arithmetic on such
a number, with the arithmetic shown, or (c) written as a question. Nothing is an
estimate dressed as a measurement, and nothing is inherited from another project's
word.

> **Scope: this file is the *application* line of work, and nothing else.** It
> governs the future `album-player` ROM crate, its UI/viz/tooling modules, the
> album-metadata manifest the application owns, and the `album-*` Make targets.
> It **does not** decide anything about a codec, and it reopens no codec
> decision: [`FLAC.md`](FLAC.md), [`ULI2.md`](ULI2.md), [`ULC.md`](ULC.md) and
> [`OPUS.md`](OPUS.md) each keep their own scope and their own sense of "the
> product path", and their rulings travel into this file only by citation. Where
> this plan needs something a codec track owes (its playback gate, the ring
> geometry), it says so and **gates on it rather than assuming it**.
>
> **The one architectural law, stated twice because it is the thing a player like
> this gets wrong: the album application is built to play an album through
> *either* FLAC or ULI2 decoding, and is therefore coupled to neither.** The app
> sees one `PcmSource` trait (§4.2) and one application manifest (§4.3). No
> screen, layout constant, tool, asset format or ROM module may branch on which
> decoder is linked in, may parse a codec's own container, or may assume a codec
> container's fields exist. Both backends are first-class: the app is demoable
> end-to-end on FLAC alone today, and the lossy backend is a swap behind the same
> seam, not a rewrite. **(That last clause is no longer only a design claim: as of
> 2026-11-19 the lossy backend meets its real-time gate, its player serves a full album with zero
> underruns in mGBA — a 12-track, 43.5-minute album in one 32 MiB ROM (`make uli-player-cross`,
> 33,407,264 B of cart; that arm was measured 2026-11-16, before the decode pass that won the
> margin, so its percentages are the older ones) — and U5 is met on hardware with three player
> ROMs. The swap is still unwritten *in this application*: there is no `src/backend/uli2.rs`. But
> the shape has been witnessed at album length on the other side of the seam.)**

---

## 1. The product in one paragraph

One cartridge, one album. `make album-rom ALBUM=my-album/album.toml` takes a
folder of WAV/FLAC files plus a cover image and produces a ≤ 32 MiB `.gba` that
boots to an album title card, plays the record start to finish, and lets you
pause, skip, browse the track list and watch the artwork move, with the track
title, elapsed and remaining time and a progress bar always on screen — at the
fidelity the audio stack has already been witnessed at, on real hardware, with
zero underruns. A second, optional mode puts a rudimentary toy game behind the
music. Everything else in this document is that sentence unpacked into parts that
can each be built, measured and refused.

---

## 2. What this app stands on (and what it does not have to build)

| Thing | Where | State | What it gives the app |
|---|---|---|---|
| Lossless decode → sound at 16,384 Hz | `crates/flac-lite`, `crates/flac-lite-gba`, `examples/flac_integration/src/product_play_main.rs` | **accepted, C1+C2 closed 2026-11-13**; real audio on two GBA units, `underflows = 0` | a working producer/consumer playback pipeline today, at ~19% of audio time |
| The geometry (clock, chunk, ring, prefill, re-arm grace) | `examples/flac_integration/src/geometry.rs` | shipped, host-modelled (`make product-play-test`) | every timing number in §4 and §10, transcribed not re-derived |
| The feeder (ring, chunk classification, 8-bit convert, DMA/timer/IRQ discipline) | `crates/gba-playback/src/playback.rs` (`RingFeed`), linked by the FLAC product ROM, the instrument and the album probe | shipped and depended on; **lifted out of the spike by §2.2 on 2026-11-15** (PR-1), with `RingFeed::truncate()` landed and `make playback-test` as its host gate | pause, skip and track-end are *states of this feeder*, not new machinery |
| A second decoder in the tree — and a second player | `crates/uli-lite`, `crates/uli-lite-gba`, `crates/uli-playback`, `examples/uli_player/` | **The precondition this row used to state is met.** ULI2 runs in real time at 32,768 Hz with compute to spare: over the 16,553-frame `biomaster` album corpus the **decode wall is 194,695 worst / 169,354 mean = 74.27% / 64.60%** of the 262,144-cycle period, and the **gate total (decode + convert + IRQ + DMA) is 226,152 / 200,624 = 86.27% / 76.53%** — **13.73 points of margin at the worst frame, `frames_over_period=0`, zero starved periods over 16,552** (mGBA, WAITCNT 4/2, no UI in that arm). **U4 met (2026-11-19)** — the decode wall first came inside the period on 2026-11-15, but the ledger kept U4 **open** until the gate total was re-taken against the written ceiling; that re-take is the pair of numbers above. **U5 met on hardware**: GBA SP, 2026-11-16, three player ROMs, two runs each, **0 underruns**, 94.52% / 95.89% / 94.56% worst (headphone / speaker / two-track), with **one open defect — a 1 Hz text redraw delays the feeder past the DMA's grace on 104 of 16,553 periods**, audible splices, no underruns, so that player's audio default is an end-only screen until its UI is non-blocking. **U6 met** (2026-11-19): `.iwram` 26,496 B of 32,768, stack high-water 948 B (bench) / 1,988 B (gate), code tables in `.ewram`. *(corrected 2026-11-19: this row read "U4 (real-time decode) is NOT met … 274,198 worst / 232,090 mean = 105% / 89%", which was the 2026-11-15 ledger and is superseded there; the ladder to here continues Step 1 (2026-11-15, decode wall under the period) → PR D/E → the closed-world pass → 86.27%. The authority for these numbers is [`ULI2.md`](ULI2.md) §Gates and [`PLAN-OF-RECORD.md`](crates/uli-lite/PLAN-OF-RECORD.md) → "Current state", never this file)* | the lossy backend is not a future candidate for the album-length build, it is a present one — and everything still missing is **on this side of the seam** (§11 Phase 5): `src/backend/uli2.rs`, the derived 32,768 Hz / 512-sample geometry (§4.2b), and **assets**, since the encoder is now in-repo and byte-exact (S1–S8) but the on-tree asset targets still run the reference (W2) and this app's adapter/geometry is W3 (R7, Q3). What that backend now owes *this app* is the number R1b names — whether a UI fits inside 13.73 points. Nothing above the seam changes either way: that ordering is invisible by construction |
| Two codec containers, and neither is the application's | `GAFP` (`crates/flac-lite`), `ULA2` (`crates/uli-lite`) | both owned by codec tracks, both pinned by those tracks' evidence | **the reason A1/A6/A10/A11 are answered by an application manifest and not by either container** (§4.3). The app must not make a product feature depend on a field one codec happens to carry and the other does not |
| On-screen text with a measured fit check | `examples/flac_integration/src/text.rs` | shipped | A10/A11/A12's font, layout settings and the "does it fit 240×160" verdict |
| Refuse-at-pack-time tooling pattern | `scripts/gafp_pack.py` (profile guard, `MEASURED_DECODE`, `flac -d` differential, self-audit re-read) | shipped and gated (`make gafp-pack-check`) | the shape of `scripts/album_bake.py` in §9: the bake tool is a *verdict*, not a converter |
| Generate an asset at build time, pin it, link it | `make master-pack INPUT=track.wav` → `scripts/gafp_pack.py --master --emit-pin` (ffmpeg normalises **only** when the WAV header differs from the target — "a pack never quietly resamples"), the `master-asset` feature switching `include_bytes!` between the fixture and the generated blob (`examples/flac_integration/src/product_play_main.rs:233-236`), and `.gitignore` ignoring **both** `examples/flac_integration/assets/master.gfp` and `src/master_pin.rs` | **shipped, and it is this repo's answer for real music** — the C2 asset is "the first 9.9 s of `bioluminescent.wav`" (`flac-docs/measurements.md`) | **the mechanism §2.3 generalises from one track to N.** Phase 1's album is neither committed nor hand-made; it is *packed* |

### 2.1 This plan is self-contained

The scope of this project is this repository. Nothing outside it is treated as an
authority, a donor, or a compatibility target:

- **No other project's code, assets, byte layouts or screenshots are cited as
  justification for a decision here,** and none is imported. Where this file
  proposes a format (`ALBM1`, `ART1`, `ALVZ1`), a control map (§5.3), an art
  pipeline (§7) or a reaction-data layout (§8), it is designed here, from this
  hardware's constraints and this repository's measurements.
- **Nor is any other project's work assumed as a tool.** The bake tool calls an
  encode step by name and by contract (§9); if no in-tree implementation of it
  exists, that is stated as a dependency of the lossy backend (Q3) rather than
  quietly satisfied by reaching outside the repository.
- What *is* fair to stand on is what lives here: the accepted FLAC playback path,
  the geometry and feeder modules, the shipped text renderer, the packer patterns,
  the gate conventions — the rows of the table above, each of which is a file in
  this tree.

This is not a new rule for the repository, and it is worth naming the in-tree
precedent: the codec tracks already froze the same boundary on 2026-11-14 — **"no C
source incorporated, no build/link/vendoring dependency, every artefact this
project builds, tests or ships lives in this repository"**
([`ULI2.md`](ULI2.md) → constraints; [`PLAN-OF-RECORD.md`](crates/uli-lite/PLAN-OF-RECORD.md)
→ PR C provenance). This plan simply applies the rule one layer up, to the
application: an app that owns its own manifest, art format, reaction-data format and
tools is also an app that can be built, tested and shipped from this checkout alone.

---

### 2.2 Spike directories are not a foundation — lift what this app depends on (ruled 2026-11-15)

`examples/flac_spike/` and `examples/e5_fetch/` are **spikes and instruments**: they
exist to gather information while the Rust + ASM FLAC implementation is developed, and
their Make entrypoints have already been retired once (`flac-docs/history.md`,
"Retired Make entrypoints"). An application line that will outlive that development
cannot stand on them — a directory whose purpose is to be thrown away is not a
foundation, and a product feature whose implementation lives there has no owner.

So the rule for this application: **no Phase 1 module may `#[path]`-include, or
depend on, anything under a spike or instrument directory. What this app needs from
one gets lifted into general project source first.** Concretely:

| what the app needs | where it lives today | where it goes |
|---|---|---|
| `RingFeed` — the ring, chunk classification, `convert_frame`, `fold_bytes` (`§4.4`'s states of the feeder) | `crates/gba-playback/src/playback.rs` — **landed 2026-11-15** | a real crate, `crates/gba-playback`, linked by the app, the FLAC product ROM (`flac_integration` re-exports it), the instrument and the album probe alike. Discharged, Phase 1 PR-1 |
| `Fnv1a64` / the fold helpers `playback.rs` reaches as `crate::checksum` | `crates/gba-playback/src/checksum.rs` — **landed 2026-11-15** | the same crate, which is where it went: it travels with its consumer, so `flac_spike`, `flac_integration` and `album_probe` all re-export the pair from one place now |
| the timer2→timer3 cycle `Counter`, `timed`, `calibrate` (§10's instrument) | `examples/e5_fetch/src/timer.rs`, with private copies in `examples/uli_bench/` | the same crate or a sibling `…-bench` support crate — one implementation, and `uli_bench`'s copy stops being a third transcription |

`examples/flac_integration/src/geometry.rs` is **not** in this list, because the
product crate is not a spike and because §4.2b already rules that the app derives and
owns its own `geometry.rs` per backend; the FLAC module stays where it is and remains
that track's to change. Phase 0's probe crate deliberately *did* lean on the spike
files by `#[path]` — that is exactly the debt this section names, taken on purpose
because the probes are throwaway. **It was discharged by the lift, not by the probes:
Phase 1 PR-1 landed `crates/gba-playback` on 2026-11-15** (`playback.rs` + `checksum.rs`
moved, `RingFeed::truncate()` with them, `make playback-test` added to `make flac-check`),
and the probe now takes the crate as a path dependency.

Two consequences worth writing down before anyone does the move:

1. **The lift was a measured change, not a file rename — and it measured.** `playback.rs`
   is compiled into both shipped product images, so moving it re-baselined their bytes and
   hashes: the four ROMs' `sha256`/section tables were re-recorded in
   `flac-docs/measurements.md` as a *re-baseline from the move* (a comment edit alone moves a
   ROM hash). Actual gate deltas at PR-1: **`spike-test` 25 → 17** (the 8 `checksum::tests`
   left the spike workspace), **`playback-test` +24** (12 playback + 8 checksum + the 4 new
   truncate tests), so `flac-check`'s aggregate moved by exactly that net −8/+24. The tree's
   own rule for a shared file — "changing that file changes this crate's images too, and
   both crates' gates are the witness that it didn't" — now has three consumers, and the
   witness is the existing `flac-check` (which gained `playback-test`) + `product-play-test`
   + `album-probe-test` set run together, not a new gate set.
2. **It settled Q13b's ownership, as predicted.** `truncate()` (§4.4) is a method on the
   ring, and the ring is what the lift moves — the reason it looked like a codec-track ask
   was that the ring was parked in a spike directory. **The application owns it:** the lift
   landed first (2026-11-15, PR-1), `RingFeed::truncate()` landed with it, and
   `SKIP_PRIME_CHUNKS` is no longer blocked on a request to another track — it is a Phase-1
   product choice, still un-chosen (§4.4).

This was deliberately **not** part of the Phase 0 PR: Phase 0's job was to measure, and a
probe that carried a refactor of the product path with it would have made its own
numbers hard to read. It landed as Phase 1 PR-1 instead, which is where §11 puts it.

### 2.3 Per-album assets are build output; scaffolding assets are tracked (ruled 2026-11-15)

The framing that survived review is **not** "nothing derived is committed" — that is a side
effect. This repository builds an end-user product; its masters will be swapped out over time
and many ROMs will be produced with different music in them. So the assets that belong to
*one album or one generated ROM* are the product's inventory and the user's to keep, not the
repository's — the build pipeline must not become the tree. The assets that exist to
**scaffold development** are a different category entirely: instruments, tracked and verified
against, with the reference master among them. Two questions, in this order, decide anything:

| ask | if yes |
|---|---|
| **Does it belong to one album, or to one generated ROM** — would swapping the music change it? | **build output**: generated at build time, gitignored, removed by `make clean`, and pinned so a stale copy is loud (R11) |
| **Is it a fixture of the development instrument** — a `ctrl` arm, an iteration corpus, a format fixture, a probe's bytes, or the reference master everything is derived from? | **tracked**: those exact bytes are what a gate reads and what a quoted number refers to |

The tree already behaves this way for real audio, and that behaviour — not a new prohibition —
is what §4.1 should have cited:

| evidence | what it establishes |
|---|---|
| `make master-pack INPUT=track.wav` → `gafp_pack.py --master --emit-pin` | the shipped path from a real master to a playable asset: normalise if (and only if) the header differs from 16,384 Hz / 16-bit / stereo / 2048-sample frames, encode, pack, and **emit the Rust pin module the ROM checks its own embedded blob against** |
| `.gitignore`, "Master-audio build artifacts (7-3)" | `assets/master.gfp` **and** `src/master_pin.rs` are ignored: "generated by `make master-pack` from `assets/sound/*.wav`; the tracked product asset is never overwritten" |
| `.gitignore`, the rate-asset rule | "its generated clips + modules are TEMPORARY like ROMs — regenerate with `make rate-assets INPUT=… RATE=…`, removed by `make clean`" |
| the census of tracked `.wav`/`.flac`/`.bin`/`.gfp` in the tree | two source WAVs (`assets/sound/bioluminescent.wav`, and `bioluminescent-32768hz-8bit-stereo.wav` — the same performance at 32,768 Hz / 8-bit / stereo, named for what it is since 2026-11-15), synthetic spike blobs, a handful of KB of crate fixtures, and one 7,748-byte `ART1` whose tracked-ness `.gitignore` justifies as "the evidence the script ran". **Nothing else that carries recorded audio is committed** |
| `flac-docs/measurements.md` — "The master asset is the first 9.9 s of `bioluminescent.wav`" | the C2 hardware witness is *derived* from the one tracked reference master, not stored beside it |

**Every binary tracked today is in the second class — instruments, not inventory.** The
census is a record of the instruments, and is not a precedent to extend to album content:

| tracked scaffolding asset | why it is the instrument, not inventory |
|---|---|
| `examples/flac_spike/assets/spike_l{0,4}_{frames,pcm}.bin` | the perf gate's `ctrl` arm: integer-synthesized source PCM ("byte-identical anywhere" — `gen_spike_assets.sh`) with the libFLAC version and per-blob sha256 recorded as a regeneration tripwire. Untracking it makes the whole measured ladder a function of whoever's encoder is installed |
| `crates/uli-lite/tests/fixtures/uli2_tiny.uli` (sha `68dca43a…`, 39,308 B) | the 129-frame iteration corpus every ULI2 arm is measured against — 2 s of deliberately varied synthetic audio, encoded by the **off-tree** C reference, so untracking it would leave the corpus unregenerable from this checkout (§2.1) |
| `crates/flac-lite/tests/fixtures/*.flac`, `crates/flac-lite/tests/gfp/*.gfp` | format fixtures and frozen expected vectors, a few KB each |
| `examples/album_probe/assets/probe_art1.bin` + `src/art_pin.rs` (7,748 B) | AP-0's art probe: the ROM `include_bytes!`es it, the host gate re-verifies it byte for byte, and `ART1.md` is explicit that it is "a probe layout, not the shipping format" — superseded at AP-4, and generalising to nothing |
| `assets/sound/bioluminescent.wav` — and `bioluminescent-32768hz-8bit-stereo.wav`, which is **the same performance** (correlation +1.000, measured at t = 10/60/200 s) rendered at `pcm_u8` / 32,768 Hz / 2 channels / 8 bits | the reference master: the *input* every product figure derives from (the C2 asset is its first 9.9 s) and Q15's Phase 1 album material; the second WAV is its 32,768 Hz-era 8-bit render, regenerable from the master by `scripts/convert_stereo.sh`. Tracking an input is right; tracking a ROM's packed copy of it is not |

**What crosses the seam for a product build:**

| tracked | generated at build time (ignored, regenerable, pinned) |
|---|---|
| `examples/album_player/album.toml` — **the test album's** track list: which files, in what order, what titles (§9) | each track's `GAFP` blob; `ALBM1`; `ART1`; `ALVZ1`; the generated `src/assets.rs` (`include_bytes!` + sha256 pins — `master_pin.rs`'s shape, N entries wide) |
| the reference master under `assets/sound/` (Q15) | — |

A user's own `album.toml` is *not* committed either: what is tracked is the fixture that makes
AP-1 runnable from a fresh clone, not an album.

Four consequences, all of them this plan's to honour:

1. **Nothing is hand-packed, and no per-ROM artifact is committed.** Phase 1's audio is *packed
   by the build*, from the tracked track list, by the Phase 1 subset of the §9 tool.
   `master-pack` already does exactly this for one track; the album's work is to make it N
   tracks wide, which is a loop plus a manifest writer.
2. **§12.1's one-implementation rule decides where that code goes.** `scripts/album_bake.py`
   is written once, in Phase 1, at its minimum scope, and Phase 3 *grows its refusals*. There
   is never a Phase 3 rewrite of a Phase 1 packer, and never a second path from `album.toml`
   to a `.gba`.
3. **Reproducibility moves from "a committed blob still matches" to "two builds match"** — the
   same shift `gafp-pack-check` already makes for one packer. This is why pack-twice-and-compare
   enters **AP-1** rather than waiting for AP-5: it is nearly free once the packer exists
   (`gafp_pack.py` already reproduces its blob byte for byte), and it is much cheaper to
   establish in Phase 1 than to retrofit onto a five-subsystem image in Phase 3.
4. **The cost of not committing is staleness, and pins are the mitigation** (R11). A build
   that silently links yesterday's blob is precisely the failure `--emit-pin` exists to make
   loud at boot; the app inherits that mechanism instead of inventing one, `make album-rom`
   depends on the pack step so a build cannot skip it, and `make clean` has to know the
   generated paths from day one, in the `master-pack` idiom.

**Scoped to this application, on purpose (ruled 2026-11-15).** Other ROM lines in
[README.md](README.md) → "Project ideas" keep their own judgement: a player that performs a
handful of scale notes has every reason to commit those assets, since for it the scaffolding
*is* the product. Nothing here binds them, and the repo-level documents state the mechanics
(`master-pack` generates, `--emit-pin` pins, the ROM verifies) rather than this rule. Inside
this application the rule has no exception for per-album artifacts: a generated ROM's bytes are
never in the tree — and equally, no instrument loses its tracked bytes to it.

The distinction is worth keeping sharp in both directions. `probe_art1.bin` is tracked because
a host gate reads *those* bytes, not because a generated blob is allowed in; the album's
per-track `GAFP` blobs are not tracked because they belong to whichever album was built, not
because nothing derived may ever be stored.

---

## 3. Requirements — the fifteen asks, as buildable items

`Phase` is §11's numbering; `Gate` is §13's. Fifteen asks, sixteen rows — A3 is one ask that
§2.3 forces into two phases (A3a packs, A3b judges; §9.0). Two items are marked **[reach]**
(§12) and the album-length build is marked **[waiting]** — waiting means "this app
cannot close it, a codec track owns the precondition", which is stated so nobody
quietly drops it. **As of 2026-11-19 that precondition is met, so the label is kept only for the
part that is still not this app's to close**: the lossy backend's real-time decode (**U4 met**,
74.27% worst on the decode wall / 86.27% on the gate total, [`ULI2.md`](ULI2.md) §Gates), its
playback (**U5 met on hardware**, zero underruns) and its output stage (**PR D shipped, as
[`crates/uli-playback`](crates/uli-playback)**) are all in this tree. What an album-length
lossy build still waits on is **assets** — the encoder is now in-repo (`crates/uli-enc`,
S1–S8, 2026-11-26, byte-exact) but **the asset path is not wired and this app has no ULI2 backend** (W2/W3, R7, Q3 —
`make uli-album-asset` still bakes with the reference, by design), so `BACKEND=uli2` cannot be
baked from a fresh clone until W2/W3 land — and **this application's own non-audio cost
inside the 13.73 points that are left** (R1b).

| # | Requirement | Phase | Gate | Notes / dependency |
|---|---|---|---|---|
| A1 | Multiple tracks on one cartridge | 1 | AP-1 | An application manifest indexes N tracks over whichever backend is linked (§4.3) — the same screen, the same code, both codecs. The index is `ALBM1`'s **internal per-track map of ROM locations and sample boundaries**, generated at build time (§2.3, §4.3), and it is not the A11 screen. `make album-rom BACKEND=flac\|uli2` (§4.2b). |
| A2 | Skip next / previous **on the cartridge itself** | 1 | AP-1 | `PcmSource::start_track` + a fade + a ring re-prime; a short, bounded delay is accepted product behaviour, not a defect (§4.4). Buttons, not the emulator. |
| A3a | Tool: a declared track list → a packed ROM (**the blobs are generated, never committed**) | **1** | AP-1 | `scripts/album_bake.py` at its Phase 1 scope: `album.toml`'s track list → one `GAFP` per track via `gafp_pack.py --master --emit-pin` → `ALBM1` + generated `src/assets.rs` → ROM (§2.3, §9.0). Refuses the two things Phase 1 can check: rate identity, and manifest-vs-container disagreement. |
| A4 | Play / pause | 1 | AP-1 | fade-in/fade-out over the DMA boundary, silent-ring state, no click (§4.4). |
| A5 | General UI layout support | **2a** bands / **2b** region | AP-3 | **one** surface, not five screens: a band table for the always-shown chrome, generated from a tracked layout spec into `src/layout.rs`, plus a host snapshot twin of **every** declared variant — snapshots for all of them, fit verdicts and pool audits only for the product-eligible ones (§13's AP-3 row, §5.0's `product` field) (§5). The declared fields include the per-variant **`tile_budget`** the tile pools are audited against (§5.1, R14) — declared here, refused at bake (§9, §9.0), measured by AP-3/AP-3b (§13). (**Table integrity was checked over the whole file, and the check has a history worth putting in the register it protects.** On `origin/main` it reports §9.1's phase table as malformed — a three-column header (`Phase \| Ships \| Exit gate`) over seven rows that hold two cells each — and that defect predates this branch. This branch's fourth commit repaired it by re-heading that table to the two columns its rows actually have, and the *same* commit introduced three new ones: rows A5, A8 and A13 were each left split across two physical lines, which to GitHub is a one-cell continuation row, so three requirement rows silently lost their Notes column. The next commit's own check found them and joined them; HEAD reports **33 table blocks, 0 mismatched rows**, and every pipe inside a code span in a table cell is escaped (`BACKEND=flac\|uli2`), because an unescaped one is a cell boundary and not text. Recorded rather than quietly fixed: this is the table the rest of the file cites by requirement ID, and the check is not self-congratulation: it found a table malformed on the branch's own base — seven rows sitting under a header that does not match them — and three split rows this branch introduced itself. The only way either class gets seen is a check that runs over the whole file rather than over the diff. |
| A6 | Elapsed time / **total** time | 1 | AP-2 | *(pair re-ruled 2026-11-20 from elapsed/remaining — §5.2, §6; the arithmetic is untouched, and the shipped screen's wording change rides in 2a's `chrome.rs`.)* shifts, not divides, because both clocks are powers of two (§6). |
| A7 | Progress bar | **2a** | AP-2 | bake-time reciprocal per track; monotonic; bar ↔ time agreement (§6). The reciprocal is generated and host-tested today; the widget — a small right-hand bar sharing the title band — is 2a. |
| A8 | Album art, even low-res | **2b** | AP-4 | 4bpp palettised tiles + ordered dither, generated by our own host tool (§7). *(Was Phase 2; moved with the 2a/2b split — 2a is deliberately art-free so the layout and the transport ship first, §11.)* An art box is *declared* in `[art]`, so that section lands with **2b**, not Phase 3 (§9.0 corrected 2026-11-20); and the input this feature needs does not exist in the tree — there is no cover anywhere outside `target/` (R15) |
| A10 | Track titles | 1 | AP-1 | From the application manifest's string pool, never the codec container; ASCII-folded at bake time (§5.5). |
| A11 | Track list | **2b** | AP-3 | a **scene in the region**, not a screen (§5.2): **a window of `floor(region height ÷ row_px)` rows** — 8 rows at the 16 px product pitch **when the text path is §5.5's static glyph atlas; the per-cell dynamic path caps the same window at five rows by pool arithmetic, which is why that atlas is 2a's and not this row's to invent** — derived from the spec, not a constant like the "≤ 32 rows" this row carried until 2026-11-20; scroll + cursor + now-playing marker, over a chrome that never moves. |
| A12 | Opening menu / title card with the album title | **2b** | AP-3 | boot → title-card **scene** → "press Start"; it draws inside its own region, and the chrome it shows is the surface's, not its own (§5.2). |
| A13 | Tool that *configures* the album and bakes the ROM | 3 (schema: **1**) | AP-5 | `album.toml` is the single input; the ROM is a function of it (§9). The file's `[album]`/`[[track]]`/`[out]` core is **Phase 1's**, because A3a needs it; `[layout] variant=` arrives with **2a**, because the bake step cannot generate a layout without being told which one (§5, §9); `[art]`/`[viz]`/`[game]` and the budget verdict arrive in Phase 3.. **Corrected 2026-11-20 against the §11 re-ruling: `[art]` arrives with 2b, not Phase 3**, because that is where art is declared and refused (§7, §9.0) — **with the split named so it is not re-litigated in review: the box *validator* is 2a's (`album-layout-gen` derives the legal box list and takes the pool term at the variant's ceiling, §5.0, §9.0's `art-box-not-derived` row), while the `[art]` key itself stays a `config-section` refusal through 2a (D2) and rides in with 2b's parser**; `[viz]`/`[ui]`/`[game]` stay Phase 3's; the album+art+code size/budget verdict moved to **2b PR-(1)** (re-homed 2026-11-25, §11's 2b note — a per-album verdict is that tool's output) |
| A15 | **[reach]** Background toy game (T-Rex / pong / breakout / snake) | 6 | AP-8 | admitted by a ring-watermark rule under a hard cycle cap (§12.2). |

**Three things in this plan share the name "track list", and two of them are not a
screen.** *Track list* (A3a, §9) is the tracked config input — `[[track]]` entries naming
files, order, titles, artists. `ALBM1`'s per-track table (§4.3) is the **generated internal
map**: for each track, its `(offset, len)` of asset bytes in the ROM, its `start_sample` and
`n_samples` on the album timeline, its string-pool offsets and its §6 reciprocal. The *track
list scene* (A11, §5.2) is the user-visible browser, drawn into the scene region in Phase **2b**,
and it never owns a band. When this file says
"the track list indexes the album", it means the second thing, which the first thing
generates.

Deferred-but-named, so they are not forgotten: seek-to-position within a track
(A2's natural next step, and the first feature where the two backends visibly
differ in granularity — the app's answer is a `PcmSource` capability query, §4.2,
not an `if`), resume-from-last-track via SRAM (agb ships
`sram`/`flash`/`eeprom`), speaker output profile (the mechanism now exists in the lossy track's
output stage — `crates/uli-playback/src/fold.rs`, Q8), and — **[waiting]**, and it is waiting on
different things than it was — album-length builds on the lossy backend: its decode (**U4, met**),
playback (**U5, met on hardware**) and output stage (**PR D, shipped**) are no longer the blocker
as of 2026-11-19 (§2, R1); what blocks them is a **`uli2` asset path in this tree** (R7, Q3) and
this application's `src/backend/uli2.rs` adapter plus its measured UI cost (R1b).

---

## 4. Architecture

### 4.1 Where the code lives

**Proposal:** a standalone workspace crate `examples/album_player/` — the same
shape as `examples/flac_integration/` and `examples/uli_bench/` (own `Cargo.toml`,
own committed `Cargo.lock`, own `target/`, `[[bin]]` per ROM image, `tests/` for
the host twins), built by `make album-rom`.

**`make album-rom` packs before it links.** The assets and the generated pin module are
build output (§2.3), so a fresh clone turns the tracked track list into a ROM with nothing
fetched, nothing hand-made and nothing audio committed — the same shape as
`make master-play-rom INPUT=track.wav` today, with N tracks instead of one.

Why not the root crate: `src/main.rs` is today a deliberately dependency-free
teal baseline, and the isolation rule that keeps a broken experiment from taking
the baseline down is written into [README.md](README.md) → "The FLAC sanity-check
triangle". The app gets that protection for free at the cost of one promotion
step later. **`README.md`'s layered sketch (root ROM composes audio + input
libraries) is the end state**: when the app is the thing this repository ships,
moving it to the root crate is a deliberate, separately-reviewed change, not an
accident of where the first file was written.

```
examples/album_player/
  src/main.rs            boot, the scene selection, the frame scheduler (§4.5)
  src/pcm.rs             PcmSource — the codec seam (§4.2)
  src/geometry.rs        period/ring/prefill arithmetic DERIVED per backend (§4.2b)                  ← host-twin'd
  src/backend/{mod,flac,uli2,stub}.rs   the only place a codec crate is named (§4.2b)
  src/playback.rs        ring states: play / fade-out / silence / re-prime / fade-in / end
  src/position.rs        the 32-bit session clock the UI reads (§4.4)                                ← host-twin'd
  src/keys.rs            edge detect, repeat, chord, debounce                                        ← host-twin'd
  src/layout.rs          GENERATED from layout_spec.toml — the selected variant's constants (§5.0)   ← host-twin'd, const-asserted
  src/text.rs            the shipped renderer, generalised from examples/flac_integration/src/text.rs
  src/art.rs             tile/map/palette upload, palette banks, fade
  src/surface.rs         THE one surface: the band stack, the composite, element-addressed redraw, input routing (§5.2)
  src/chrome.rs          the always-shown bands: title line, bar + clock, control art in declared slots (§5.3)
  src/scene/{blank,list,title_card,art}.rs   the scene slot — one Region, one function (§5.2)
  src/viz.rs             ALVZ1 reader (§8)
  src/game/{core,pong,snake}.rs   (§12.2, phase 6)
  src/meta.rs            ALBM1 album-metadata reader (§4.3)
  album.toml             TRACKED — the track list: which files, in what order, what titles/artists, and which layout variant this ROM is (§9, §5.0)
  layout_spec.toml       TRACKED — the declared variants: band rows, element spans, bar geometry, face per element, image boxes, palette banks (§5.0)
  assets/                GENERATED, gitignored — per-track GAFP + ALBM1 + ART1 + ALVZ1 (§2.3)
  src/assets.rs          GENERATED, gitignored — include_bytes! + sha256 pins over the above (master_pin.rs's shape, N wide)
  tests/                 host twins + snapshot checks
```

**One line of that tree is superseded as of 2026-11-20, and the line above is the record
rather than an edit that erases it:** `src/screens/{title,menu,nowplaying,tracklist,pause,
error}.rs` was the five-screen plan, and it is replaced by `surface.rs` + `chrome.rs` +
`scene/*` (§5.0, §5.2). Two things survive that change untouched. **The verdict path has no
`screens/` file to live in and never did**: `DATA ERROR` / `PASSED` / `FAILED` is drawn by
`album_play_main.rs` and `main.rs` today, host-tested and hardware-witnessed — **AP-1's ROM arms
print the verdict, and AP-7a legs (2)–(4) are the blue screen reading `PASSED` on a real unit** —
and it stays exactly there (§5.2). (**Cited correctly as of 2026-11-20: an earlier draft of this
sentence said "AP-3", which is a gate that has not landed and never witnessed anything about this
screen.**) And `src/art.rs`, `src/viz.rs`,
`src/game/*`, `src/meta.rs`, `src/position.rs`, `src/keys.rs`, `src/geometry.rs` and every
host twin in `tests/` are planned exactly as listed, with nothing in this re-rule changing what
they are or are for — which is the actual test of whether a
layout change was cheap: **no landed measurement moves, and no module that was not asked to
change has to.**

### 4.2 The seam is `PcmSource`, and it was already specified

[ULI2.md](ULI2.md) → "Phase 2" item 4 asks for exactly this: *"Make the playback
layer codec-agnostic with a small `PcmSource` trait, so FLAC remains a
compile-time alternative and an A/B control rather than dead code."* The app is
where that gets built — **and it has now been built twice, which this paragraph did not say when
it was written (2026-11-19)**. `examples/album_player/src/pcm.rs` is Phase 1 PR-3a's and is the
trait this section specifies. **A second, same-named `PcmSource` exists on the codec side**: [`crates/uli-playback`](crates/uli-playback) →
`source.rs` (PR D, U5 met on hardware) carries `PcmSource` / `UliSource` with a **different**
shape — planar int8 frames, `seek_track` / `track_index` / `samples_into_track` per **track**, and
`track_title()` read from the `ULA2` container's strings. The two must not be conflated: this
section's rules (positions in **album samples**, the manifest authoritative, interleaved i16) are
what `pcm.rs` implements, and the app's `backend/uli2.rs` will *wrap* `UliSource` behind it and
cross-check that container's tables (§4.3 rule 1, Q13). The trait is deliberately narrow — 9 methods, no lifetime
games, everything positional in **album samples**:

```rust
pub trait PcmSource {
    fn n_tracks(&self) -> usize;
    fn track(&self, i: usize) -> Option<TrackMeta>;      // start_sample, n_samples, title, artist
    fn album(&self) -> AlbumMeta;                        // title, artist, year, art handle, n_samples
    fn rate_hz(&self) -> u32;                            // 16_384 (FLAC) | 32_768 (ULI2)
    fn start_track(&mut self, i: usize) -> Result<(), SourceError>;   // positions at the preroll frame
    fn start_sample(&mut self, s: u32) -> Result<(), SourceError>;    // A2 follow-up; 4 s / 125 ms granularity
    /// Decode at most one frame into `out` (interleaved int16). Caller owns `out`.
    fn pull_frame(&mut self, out: &mut [i16]) -> Result<usize, SourceError>;
    fn producer_frame(&self) -> FrameCostModel;          // worst/mean cycles, for the budget audit
    fn reset_decoder_state(&mut self);                   // cold-start after a pause or a skip
}
```

Three rules fall out of this and are worth stating because they are where a naive
player leaks the codec into the UI:

1. **Position is in samples on the album timeline, always.** The manifest
   ("track boundaries … in album samples", §4.3) is what defines that timeline,
   and every consumer of the clock — bar, mm:ss, reaction-data index, the game's
   admission rule — reads the same number. Frames, chunks, periods and bit
   positions are the producer's business and never cross this seam.
2. **The app never branches on which codec is loaded.** No `if flac { … }` in a
   screen, in a layout constant, or in the bake tool's budget arithmetic. If a
   screen needs something one backend cannot do, `PcmSource` gains a **capability
   query**, and the *capability* is what the screen tests and what the gate
   exercises — so a feature is written once against a boolean the app already
   knows how to ask for, not twice against a codec name.
3. **`producer_frame()` is part of the API because the budget is part of the
   product** (§10): the ROM's own accounting wants the number it is running on,
   not the number in a document.

### 4.2b One ROM, one backend, chosen by cargo — and geometry that follows

**Both backends are first-class and neither is the app's owner.** A release image
links exactly one (the decoder state and the `.iwram` kernel sets do not fit side by
side carelessly, and a player has no business carrying two of them), so the backend
is a **build-time feature**, not a runtime mode:

```
src/backend/mod.rs    the only module that knows an implementor exists
src/backend/flac.rs   links flac-lite / flac-lite-gba        — feature "backend-flac"
src/backend/uli2.rs   links uli-lite  / uli-lite-gba         — feature "backend-uli2"
src/backend/stub.rs   synthetic PCM, no codec linked at all  — host UI/layout tests
```

`make album-rom BACKEND=flac|uli2` picks it; **AP-9 (§13) builds all three arms and
runs the same screen and layout suite against each**, which is the actual test that
the coupling is absent — a UI that passes on `stub` is a UI that cannot know which
codec it is driving. The `stub` arm is also the fast inner loop: no decoder, no
emulator, seconds.

The consequence that a two-backend plan has to face immediately: **the shipped
geometry is FLAC-shaped.** `geometry.rs` is a set of compile-time constants with
compile-time pins — `PLAYBACK_RATE_HZ = 16_384`, `TICKS_PER_SAMPLE = 1_024`,
`CHUNK_SAMPLES = 560`, `PERIOD_CYCLES = 573_440`, and a wall of
`const _: () = assert!(…)` proving nobody rotted a transcription. The lossy backend
lives at 32,768 Hz with a 512-sample frame, so **every one of those numbers is
different and none of them may be hardcoded in a screen**. So:

- `src/geometry.rs` (the app's own) **derives** the period arithmetic from the
  backend's declared `rate_hz` and frame size, keeping `geometry.rs`'s discipline
  — every constant traced to a hardware or backend fact, and the same class of
  `const` assertions. It is parameterised per backend at compile time, so the pins
  are still compile-time facts, not runtime hopes.
- The rules it re-derives, quoted from the shipped file so they are not lost:
  a chunk must be a multiple of **4 words** (the FIFO refills in 4-word requests);
  the ring depth is a **lead-time** decision (`RING_CHUNKS × CHUNK_SAMPLES`
  ≈ 1.23 s at the shipped geometry) expressed in seconds and then divided up, so
  the same product behaviour lands on either clock; the prefill sets a floor under
  the depth (`PREFILL + one frame ≤ RING`); `REARM_GRACE_SAMPLES = 16` is hardware
  and does not change with the rate.
- The **period counter** that the shipped geometry uses for its audit is 16-bit
  and needs the 32-bit session clock of §10.4 (ruled 2026-11-14: one `u32` in
  `.iwram`) — one more reason the app keeps its own clock rather than borrowing an
  instrument sized for a 9.9-second test clip.
- The two backends' rate/geometry differences must never surface as UI behaviour.
  They surface, if at all, as **capabilities** on the seam: seek granularity, frame
  size, declared rate, cost model.

### 4.3 The manifest: the application owns the album, the codecs own their streams

Both backends already have a container of their own, and they are **different** —
which is precisely why the application may not lean on either one:

| The app needs | `GAFP` (FLAC) has | the lossy backend's container has | therefore owned by |
|---|---|---|---|
| Track count and order | one stream per blob | its own track table | **app** |
| Track boundaries + durations | `total_samples` per blob | its own sample offsets | **app** (in album samples) |
| Track titles / artists | ✘ | its own string pool | **app** |
| Album title / artist / year | ✘ | ✘ | **app** |
| Cover art | ✘ | ✘ | **app** |
| Reaction data (loudness / beat / bands) | ✘ | ✘ | **app** |
| Per-track progress reciprocal (§6) | ✘ | ✘ | **app** |
| Per-track decode-cost ceiling (§12.2 watermark) | ✘ | ✘ | **app**, measured at bake time |
| Frame-level seek granularity | 125 ms (one 2,048-sample frame at 16,384 Hz) | finer or coarser, by its own design | backend, exposed as a **capability**, never as a constant |

Three rows exist and two do not, in each column, and the pairs do not line up. Any
feature built directly on one codec's container is a feature the other backend
cannot have without an app-level `if` — so the app stores what it needs **itself**,
in one manifest, in its own units, and the backend's container is an input to the
bake tool and an internal detail of the adapter, nothing more.

**The three blobs the application owns** — each a header + sections, 4-byte
aligned, written by a tool that refuses before it emits and read back by a
validating parser that refuses before it indexes (the discipline
`scripts/gafp_pack.py` and `crates/uli-lite/src/album.rs` both already model, at
one layer of remove):

- `ALBM1` — **the manifest, and the only album-structural thing the UI reads:** the
  internal per-track map the player decodes from (this file's other "track list" — §3's
  terminology note; A11 is the screen).
  Album title/artist/year, `n_tracks`, and per track: string-pool offsets for
  title and artist, `start_sample`, `n_samples`, `rate_hz`, the §6 progress
  reciprocal, the §12.2 measured per-frame cost ceiling, the backend selector, and
  the `(offset, len)` of the asset bytes that track draws from. For the lossless
  backend that is one embedded `GAFP` blob per track — which is what turns N
  lossless clips into an album. For the lossy backend it is whatever that track
  ships as, addressed by the adapter.
- `ART1` — palettised tile sets, BG maps and `Rgb15` palettes (§7).
- `ALVZ1` — the reaction-data streams (§8), indexed on the manifest's album-sample
  timeline.

**Two rules make the decoupling real rather than aspirational:**

1. **The manifest is authoritative.** Track boundaries, durations and titles come
   from `ALBM1`, always. The bake tool *cross-checks* it against whatever the
   backend's own container reports and **refuses on disagreement** — so the two
   can never drift, and the UI never has to choose which one to believe.
2. **No app module imports a codec crate.** The adapter crate/binaries are the
   only link site; a screen that wanted a container field would have to add the
   dependency, and the module boundary is what makes that visible in review. The
   host twins for the UI (§5.2, §6) run against a `StubSource` that returns
   synthetic PCM — no decoder linked at all — which is the test that proves the
   coupling really is absent.

Why the app is not simply folded into a codec container: the codec tracks' byte
layouts are pinned by their own evidence sets (corpora, hashes, packer
round-trips) and are their owners' decisions to change. An application iterating
on UI features has no business re-versioning a codec format, and a codec track has
no business owning an album's cover art.

**One boundary to settle across the tracks, recorded rather than assumed.**
[`ULI2.md`](ULI2.md) → Phase 2 item 3 (and PR D in the plan of record) says *"Drive
the UI from the `ULA2` track/seek/strings tables"*, which predates the
EITHER-backend requirement and points the other way from §4.3. Both readings are
coherent for their own scope, so the difference is a question for the owner
(**Q13**), not a licence to quietly disagree. The proposal here keeps the
application's side of it: the adapter may read `ULA2`'s tables — indeed it must, to
position the decoder — and **cross-checks them against the manifest, refusing at
bake time on any mismatch**, so "the UI is driven from the ULA2 tables" and "the UI
is driven from `ALBM1`" become the same statement wherever the two agree, and a
build failure where they do not.

Whatever the answer, one rule is not optional: **the app's blobs are validated at
bake time and re-read by the reader that ships**, exactly as `gafp_pack.py` does
(`audit_blob`: "a blob is not accepted because it was written"). Every app blob
gets a host gate that builds it, parses it back, and compares against the source
inputs field by field.

### 4.4 Skipping and pausing — and why a short delay is the right answer

**Ruling, because it is a product decision and someone will otherwise "fix" it:**
a skip is *allowed to take a moment*. A cartridge player with a deep audio queue
fading out, re-seeking and re-priming is normal behaviour that every real deck
performs, and the alternative — instant skipping — is bought by shortening the
buffer that protects playback from a long decode frame. So the contract is stated
in terms of what the user may observe, not in cycles:

| observable | v1 target (measured at AP-1, not asserted) |
|---|---|
| Screen changes to the new track (title, index, bar reset) | **press → audible transition: 18 video frames ≈ 306 ms measured** (corrected 2026-11-17 from "≤ 3 frames ≈ 50 ms", which was arithmetic about the sample-and-period floor with no measurement of the loop behind it — see the chain below) |
| Audio of the new track begins | ≤ ~0.6 s, and that number is `SKIP_PRIME_CHUNKS`, a policy knob (§4.4 below). **The precondition is met: step 2 truncates the ring (Q13b, landed 2026-11-15). Measured at the ring's ceiling depth, the truncate reads `SKIP_PRIME_CHUNKS + 1` periods — 3/5/9/17 periods = 102.5/170.9/307.6/581.1 ms for prime 2/4/8/16 — inside this row at every setting; drain and consume-and-discard still read 39–53 periods = 1.33–1.81 s.** |
| Clicks / pops at the transition | none: fades, never a mid-waveform splice |
| Underruns attributed to a skip | 0, including the silence fed during the re-prime |

**The one-frame row was wrong, and the measurement that says so is the reason input is sampled in
the vblank handler (ruled 2026-11-17, PR-3b ii-A/ii-B1).** A producer frame of the lossless path —
`pull_frame` → widen → `convert_frame` → push — costs **1.04–1.09 M cycles = 3.7–3.9 video frames and
is uninterruptible**, because a FLAC frame is the atomic unit of decode. So no poll rate in the main
loop can meet "within one video frame": ii-A measured one poll per 7.5 frames before the loop was even
restructured. The sample therefore lives in the vblank ISR, which is the only code in this system that
runs at video rate, and the observable is restated as a floor with two named terms rather than a wish:

| term | bound | why it is that number |
|---|---|---|
| press → edge detected | ≤ 1 video frame (16.7 ms) | the keypad is sampled once per frame in the vblank ISR; measured `detect_latency max=1` |
| detected → action taken | **3 frames measured** (was claimed ≤ 2) | the action is taken at the top of a main-loop pass, and a pass is bounded by a producer frame of 3.7–3.9 video frames — **a transition cannot straddle a decode frame**, so this leg has a floor set by the codec's own unit of work; measured `take_latency max=3`, gated against the run's own worst pass |
| taken → hand-off (a skip) | ≤ `take_to_handoff_bound(pass)` = pass + 1 periods — **4 at this run's measured 3-period pass**, 2 measured | the ramp must be **served** before the truncate — the deferral below. Corrected 2026-11-19 from "≤ 3 periods (bound), 2 measured": the 3 was mGBA's measured decode rounded up to a period count, so it was a threshold wearing a bound, and a real cart's longer pass would have failed a correct run (R13) |
| taken → freeze (**a pause's arm — not part of the total below**) | ≤ `pause_to_freeze_bound(pass)` = `FADE_PERIODS` + one alignment period + one pass — **6 at this run's measured 3-period pass**, 2 measured | the same deferral, plus the loop's next turn: `Step::Freeze` is observable only from `tick()`, and `tick()` runs only inside a main-loop pass — where a skip's hand-off is performed by the period ISR on its own cadence. Host-side, the same expression bounds the model's own pause continuation, which until now ran against a literal `4` |
| hand-off → the new track's first sample | `SKIP_PRIME_CHUNKS + 1` = 5 periods | D2, unchanged |
| **total, press → audible** | **18 frames ≈ 306 ms** | the legs summed, printed as the ROM's `latency:` line and gated **per leg**, not as one number |

**And ≈ 580 ms onto tracks 0 and 2** — the two full-master visits — because this recording opens with
**4,480 samples of exact digital silence**: measured through the real seam, those FIFO bytes are `0x00` on
both channels and the first non-zero byte is at index **4,951**, which is why the wire watch reads 8 more
silent periods. That is music, not machinery — but it puts the *worst* case right at §4.4's ≤ ~0.6 s
ceiling rather than comfortably inside it. Re-making that choice (`SKIP_PRIME_CHUNKS = 2` would recover
~68 ms) belongs to real ears at Phase 5, not to arithmetic now.

**A fade must be served, not merely handed over (ruled 2026-11-17, review round).** The first cut applied
the ramp to the slot handed to the DMA and then froze or truncated *immediately* — at which point the DMA
was at sample ~0 of the very chunk carrying the whole ramp, so the listener heard the **previous** chunk
end mid-waveform at unity gain, up to 560 samples were dropped, and the 15.6 ms fade was never audible on
either pause or skip. §4.4's own "none: fades, never a mid-waveform splice" row is what this violated, in
code that cites that row. A transition now waits for the hand-off *after* the ramped chunk has been served
(`served_silent`, latched by the feeder), at a cost of exactly one period. The difference between a fade
in the code and a fade the listener hears.

**pause / resume — the contract, defined here because it existed nowhere before PR-3b.** A4 asked for
pause, §2's feeder row asserted it would be a state rather than new machinery, and neither said what it
means. `pause`: ramp the **hand-off** gain to zero, then **freeze** — both sound DMAs and the timer chain
stopped, producer halted, the ring exactly as it was. `resume`: re-prime both DMA buffers from the ring
head (a *prime*, not a seek — the head is the sample the freeze stopped on), re-arm in the shipped order,
ramp back up. No truncate, no seek, no re-decode; the session clock stops with the chain, which is why a
paused clock cannot tick. Rejected, reasons in the code: *truncate-and-re-decode* (discard-forward from
the **track start** — ~77 s of CPU minutes in, since the seam has no seek, Q5) and *keep-feeding-silence*
(the queued 1.23 s plays out before any fade begins, and `committed` keeps advancing so the displayed time
runs while "paused").

**The pause's leg is bounded now (2026-11-19).** Every row of the chain above was gated and this one was
only printed. The ROM has recorded `fade_periods` since PR-3b-ii-B2a — the feeder's entry count at the
freeze minus its count at the toggle, in the audio clock's machine-independent unit (`audio_clock.md`) —
and printed it under that name on every row, but compared it to nothing: `pause=true` was `gain=0 &&
dropped=0`, which this gate's own summary already called "both arms of a clock-delta tautology". A run
that took ten periods to reach `Step::Freeze` would have printed the same `[PASS]`, and the only witness
that a pause is *instant* was a person reading a `2` off a log — this gate's own floor wording, "a green
that names nothing is not a green", with the polarity reversed: the number was printed, and nothing read
it. `pause_to_freeze_bound` (the row above) closes it from the same measurement the skip's bound uses —
this run's worst main-loop pass — because a pause, unlike a hand-off, is performed by the main loop.
Each row now prints the leg as `action→freeze=N periods (bound M)`, the shape the skip rows already
used, and the gate reads that row independently of the term. At this repo's measured pass the bound is 6
periods and the legs read **2**: the mechanism's floor, because `State::feeds()` is false while
`Pausing` and so the loop's next turn comes quickly. The headroom is stated rather than hidden — what
this closes is an *unbounded* leg, not a claim that a pause is instantaneous, and the host model is held
to the same expression (its own pause continuation was capped by a literal `4` — the same class of thing
R13 removed from the ROM's bounds one commit earlier).

**The inter-track gap is a number now, not an admission**: **214 / 229 ms** (6.3 / 6.7 periods) at a
track-end boundary, printed on each `advance:` row with its decomposition — tail padding, two starved
periods, and the cover. Four of those periods are the `SKIP_PRIME_CHUNKS` **constant**, so the figure is
honestly labelled *derived*; only the padding and starved counts are measured. See Q4.

Two things this costs, recorded because both are load-bearing later. **The ISR must stay inside the
re-arm grace**: there is no interrupt nesting on the ARM7TDMI, so a vblank handler running when timer1
overflows delays the feeder, and the entire margin there is `REARM_GRACE_SAMPLES = 16` samples — so the
handler brackets its own body on the audio clock and the worst bracket is a **verdict term**, measured
at **1 sample = 1,024 c**, i.e. 6% of the grace (bracketing needs the period witness folded in, because
timer1 reloads rather than counting from zero — a naive bracket reported 64,977 samples for a handler
that took one). And **the main loop still runs at producer cadence** — `position` updated on 1,129 of
1,791 frames — so when Phase 2 draws, its drawing must be driven off the **vblank latch**, not the
main-loop iteration, or a screen change will wait up to four frames for a decoder that has nothing to do
with it.

The shipped geometry queues **1.23 s** of decoded audio
(`RING_CHUNKS 36 × CHUNK_SAMPLES 560 = 20,160` samples at 16,384 Hz, per
`geometry.rs`), plus the ≤ 34 ms buffer the DMA is draining and the 32-byte FIFO.
That is a *feature*: it is what lets the producer run flat out between periods.
It is also, plainly, what would play 1.23 s of the old track if a skip merely
re-seeked the producer. So a skip does not re-seek and keep going — it **drops and
re-primes**:

```
skip(direction)
  1. fade out over FADE_SAMPLES (256 = 15.6 ms at 16,384 Hz, 7.8 ms at 32,768)
  2. truncate the ring: the queued chunks are dropped, not played   ← Q13b, landed (PR-1)
  3. source.start_track(i ± 1)                  — the seam call; per-backend seek is adapter-internal
  4. feed silence chunks so the DMA keeps getting a whole chunk every period
  5. re-prime the ring to SKIP_PRIME_CHUNKS
  6. fade in
```

Step 4 is why the delay costs no robustness: the feeder never runs out of a chunk
to hand the DMA during a skip, so an underrun counter stays a real measurement of
the *producer*, not an artefact of the UI. And step 5 is why the delay is short and
tunable: `geometry.rs` justifies the boot prefill (`PREFILL_CHUNKS = 16` → 547 ms)
as cold-start protection — the decode tables, the `.iwram` kernel and the frame's
ROM bytes are all cold. During a skip **none of that is cold**. So the skip path
gets its own, smaller watermark, and the host model (the `product-play-test`
sibling) is what says whether `SKIP_PRIME_CHUNKS = 4` still survives the measured
per-frame cost, rather than this file guessing it.

**What AP-0 measured, and the ruling it forced (2026-11-14; the third row measured
2026-11-15).** Step 2 says *dropped, not played*, and the probe wrote all three versions of
that against the real
`RingFeed`, at the ring's **ceiling** — 35 queued chunks plus the fade push, filling all 36
(`examples/album_probe/README.md` → "The deep arm"; 20 skips per arm, `underflows = 0`
throughout). The shipped producer's steady state is `RING − 2,048` ≈ 32 chunks, the same
regime and one quarter shallower, so these rows are the ceiling of it rather than a
different case:

| step 2, at the ring's 36-chunk ceiling | old track still handed to the DMA | silence handed to the DMA | until new-track audio |
|---|---|---|---|
| drain (play the queue out) | **36 periods = 1,230 ms** (raw `old_periods` 37) | prime | **39–53 periods = 1.33–1.81 s** |
| consume-and-discard (take the chunks, hand the DMA zeros) | **0 periods** — the stale chunk is taken and then overwritten with zeros before the DMA re-arms (raw 1) | 38–52 periods | **39–53 periods — identical** |
| truncate (`consumed = pushed` in one step) | **0 periods** — the queued span, fade-out chunk included, is stepped over (raw 1) | prime | **3–17 periods = 102.5–581.1 ms — measured 2026-11-15**, `min == max` over 20 skips at each prime, on the shipped method |

**One metric, read three ways.** The instrument reports `old_periods` = periods from the mark
until the *first silent chunk* on the wire, so its last period is already silence and the
old-track **deliveries** are that reading minus one: 36 for the drain (those chunks *are* the
old track), 0 for the two drops. The **1** the two non-drain rows read is the wire watch's
floor rather than a delivery — with the arm's chunks silent, `watch.silent_at` latches at the
first period boundary after the arm (`at + 1`), so a policy whose chunks are silent from the
first period after the mark cannot read 0 (`min == max == 1` over 20 skips per arm, the log
line in `examples/album_probe/README.md` → "The deep arm"). Every row satisfies
`to_new = delivered + silence + 1`, which is the check that the three arms are comparable.

Drain and consume-and-discard are the same length, because without a truncate
`RingFeed::consumed` moves only by `take_chunk` — one hardware period per queued
chunk — so discarding a deep queue buys silence *instead of* the old track, not
time. Only the truncate meets the ≤ ~0.6 s observable above, and it meets it because
it removes the queue term instead of trading against it: `to_new` becomes
`SKIP_PRIME_CHUNKS + 1` periods regardless of how full the ring was.

**Ruled: truncate is the mechanism — landed 2026-11-15 (Phase 1 PR-1).**
`crates/gba-playback/src/playback.rs` carries `RingFeed::truncate()` (equivalently
`skip_to_producer()`) setting `consumed = pushed` inside the same critical section
`take_chunk` uses, so `consumed <= pushed` and "a starved period consumes nothing"
hold unchanged, and `make playback-test` witnesses it: `ready() == 0` afterwards, the
producer ledger untouched, and the next push delivered whole — including across a ring
wrap. The skip probe's `truncate` arm drives the shipped method on mGBA and reads the
3/5/9/17-period table above, so the mechanism and its number are both measured rather
than derived. `SKIP_PRIME_CHUNKS` itself stays **un-chosen** in this PR: with the queue
term gone, the observable no longer decides it — what is left to choose is the length
of silence the listener hears between tracks, which is a Phase-1 product decision and
not a budget one.

**The clock the UI shows.** `position.rs` stays deliberately boring, because §4.4's
drop-and-reprime removes the hard case: the feeder already counts every chunk it
commits to the DMA (the product ROM's `pushed = consumed = SAMPLES` invariant is
that counter), so

```
paint_position = committed_samples − LEAD_SAMPLES      // one named constant
```

with `LEAD_SAMPLES` chosen once from the ring geometry and living in the same
shared-constants module `geometry.rs` established (ROM and host model read the one
file). Track starts are exact, because the counter is reset at the drop. The only
residual error is a constant lead during steady playback, which the eye cannot see
in a bar or a mm:ss readout. The invariants worth gating on, and all checkable in a
host test (§13 AP-2):

- the bar and the mm:ss **never move backwards** within a track, across pause, or
  across a skip;
- both are **0 at a track start** and reach the manifest's `n_samples` at its end;
- the displayed total equals what the bake tool printed for that track.

What is *not* designed here, and why: no block-map or audible-position ledger
mapping each DMA buffer back to a sample range. It exists to buy sub-frame
position accuracy, which this product does not need once skipping is a
fade-and-reprime. Correspondingly the control map (§5.3) uses **no
time-dependent chord** — "previous track if within 3 s of the start" is exactly the
rule that would need the ledger, so it is replaced by a dedicated restart button.
If a future feature wants that chord, the ledger is the thing to add and AP-2 is
the gate that would then have to hold.

### 4.5 The frame

Two clocks meet here: the audio clock (a period is 34.18 ms / 573,440 cycles at
16,384 Hz, from `PERIOD_CYCLES`) and the video clock (~59.7 Hz ⇒ ~281,000 cycles
per frame at 2²⁴ Hz, **to be confirmed by measurement in AP-0, not assumed**). The
audio timer IRQ already owns the tightest deadline in the system, and its
discipline is inherited whole: `REARM_GRACE_SAMPLES = 16`, 4-word FIFO refill
granularity, `push` inside CHUNK-sized critical sections (FLAC 7-2), the
median-of-three clock read. So:

```
audio timer IRQ  : hand the DMA the next chunk; classify it; count it.
                   Nothing else. Ever. No drawing, no analysis, no key reads.
VBlank           : latch keys; set "frame admitted"; (later: start the frame's UI slice)
main loop        : loop {
    while ring_ready >= CHUNK && !frame_deadline { producer.pull_frame(); convert; push; }
    if vblank_latch { position.update(); ui.tick_and_draw(); viz.tick(); game.tick(); }
}
```

The producer is the *thief of leftover time*, not a task; the UI and the game are
guests admitted only while the ring has headroom (§12.2 generalises this to
everything that is not audio). The existing ROM measures stages with interrupts
live and separates **work share** from **wait**, and keeps the closure identity
`audio time = stages + feeder + drain spin + residual` with the residual named.
The app keeps that identity verbatim and adds UI terms to it, because an
unexplained gap in a ledger is a bug in the ledger.

**Measured, PR-3b-ii-A (2026-11-17, the gate track, mGBA).** The sketch's inner
`while` cannot poll the latch more often than once per producer frame, because one
frame of this stream — `pull_frame` → widen → `convert_frame` → push — costs
1.04–1.09 M cycles: **3.7–3.9 of AP-0's 280,896-cycle video frames, all of them
uninterruptible.** So the row above is implemented as written (the vblank handler
latches; the main loop polls it) and the ROM *prints* the cadence instead of
asserting one: `cadence: video_frames=1790 input_polls=1116 ratio=1116/1790
max_gap=4 frames limit=5` — one poll per frame while the ring is full, one per ≤4
while the decoder is running, against PR-3a's one per 7.5 frames. §4.4's "screen
changes within one video frame of the button press" is **not** met by that and
cannot be from the main loop: it needs the key *sample* (not merely the latch) in
the vblank handler, or a decode sliced below one frame — a decision for ii-B,
recorded here rather than glossed. The audio side is untouched by the restructure
(`underflows=0`, `coalesced=0`, `late=0`, `handler_entries=877` before and after).

One shipped gotcha to carry into the UI design, because it bit 6a-7b's first
hardware run: agb's `GraphicsFrame::commit` derives DISPCNT's
background-enable mask from the backgrounds *shown into that frame* — so a commit
from a fresh frame with nothing shown writes `enabled_backgrounds = 0` and repaints
the backdrop alone (`text.rs:19`'s "One-shot present" note — and note that comment
names the type `TiledGraphicsFrame`, which is not what agb 0.25.0 calls it:
`display/mod.rs:297` is `GraphicsFrame`, and `tiled.rs:382-389` is where the mask is
derived; the fact in the comment is right and only the name is wrong, and the comment
should be fixed when that file is next touched, not argued about here). The app is by
construction *not* one-shot: it owns its backgrounds for the ROM's lifetime and
shows every live one every frame, and AP-0 exists partly to prove that on screen.

---

## 5. The surface: one band stack, and the scenes that swap into it (A5, A11, A12; the frame A8/A9 sit in)

240×160 px, mode 0 (tiled, 4 BGs). Everything is 8×8 tiles: 30 columns × 20 rows.
The 8 px font (`assets/fonts/fusion-pixel-8px-proportional-latin-ascii.ttf`, a
96-glyph U+0020–U+007E subset, OFL) is proportional, so text is laid out by the
shipped renderer and *measured*, never assumed to fit.

### 5.0 The band model *(re-ruled 2026-11-20; supersedes this section's 96/64 split and Q7's answer — Q7's row is kept and marked in §15)*

The old mock — "visual in the top 96 px, text in the bottom 64 px" — was justified
arithmetically, and both halves of that arithmetic still hold: a multiple of 12 rows puts
art on a tile boundary, and 8 rows of 8 px is exactly a title/artist/time block. What
does not survive review is the word **text** for the bottom band. A bottom half that is a
static block of metadata, with everything above it swapped wholesale, is the five-screen
model in disguise — and what it costs is the one thing a player is judged on: whether the
transport state is still readable while something else changes.

So: **one band stack, laid from the bottom up**, where the bottom bands are **always
shown** and everything above them is a swappable **scene**.

```
┌────────────────────────────────────────┐   y=0 — scene region: 16 tile rows = 128 px = 8 text rows at a 16 px pitch
│              SCENE REGION              │
│                                        │
│    art · track list · title card ·     │
│       later: visualiser, a game        │
│                                        │
│                                        │
│                                        │
│                                        │
│                                        │
│                                        │
│                                        │
│                                        │
│                                        │
│                                        │
│                                        │
│                                        │
├────────────────────────────────────────┤   y=128 — title band: 2 tile rows = 16 px (a floor, not a taste call)
│ 03 · Shellbound                        │
│                   ▓▓▓▓▓▓▓▓▒░░░░░░░░░   │
├────────────────────────────────────────┤   y=144 — chrome: 2 tile rows, always shown, controls are art
│                                        │
│ ◀◀    ▶/❚❚    ▶▶          1:47 / 3:12  │
└────────────────────────────────────────┘   y=160
```

- **Chrome** (2 rows): the transport controls, and the clock beside them — **elapsed /
  total**, not elapsed / remaining (§6, ruled 2026-11-20). Always shown: a control you
  cannot see is not a control. And the two requirements PR-3c handed to this layer (§5.3)
  — a transport state visible at all, and a press whose meaning depends on state — are both
  *chrome* facts, which is a cleaner home than the "A11 affordance" §11's scoping called
  them.
- **Title band** (**2 rows = 16 px in the picture above, and that height is a floor**: the
  smallest line box any face tracked here produces is 12 px, which is two 8 px tile rows — §5.1's
  measured table, §5.0's `pitch-not-tileable` refusal. A variant that declares **no text line** in
  this band — §5.0's variant A, art and bar only — can take it down to 1 row and gets a 136 px
  region for it): A6's track number and
  title, **one line, elided, never wrapped** (§5.5), with **A7's bar in the same tile row,
  at the right.** The bar is deliberately small: the mm:ss pair carries the exact position
  and the bar carries the *gestalt*, which needs a proportion rather than a ruler. The cost
  of shrinking it is a precision cost, so the bake report prints the ±1-cell error as a
  percentage for the geometry actually declared (§6). **Its width and cell count are declared
  fields of the variant, not numbers in this file** — for A the recommended starting default is
  **72 px in 18 cells of 4 px** using §6's partial-fill pattern tiles, because the coarse
  alternative (9 cells of 8 px) would put that error at 11% of the track, and §6 sets a floor
  of 16 cells. The bar drawn above shows exactly that default — **18 cells of one character
  each**, eight filled and a partial — because a diagram whose cell count disagrees with the
  number three lines above it is the kind of detail a review has to spend time on; the drawing is
  schematic only in that a cell is a character, not 4 px. This corrects the old mock, which gave
  the bar its own row: it does not need one.
- **Scene region** (everything above): one rectangle, one scene at a time.

**The rectangle is the enforcement, not a style rule.** A scene is one function receiving
a `Region` and nothing else: it cannot draw over the chrome, move a control, or repaint the
title band, because it was never given those cells — and the chrome cannot be moved by a
scene, because the surface owns it. That is the whole abstraction: one registry, one
function, **zero runtime geometry** — which is the ruling §16 already makes ("no general GUI
toolkit"), and the reason the thing generated at compile time is a *band table with a slot*
rather than a set of screens. **(Corrected while writing this section, 2026-11-20: an earlier
draft of this paragraph attributed that conclusion to Q7. Q7 is, and always was, about art box
size and default mode — it says nothing about runtime layout. The conclusion is this file's own
from §16, and it is cited as that instead of as someone else's.)**

**And one consequence the rectangle rule does not carry by itself, found by reading agb while
checking the switch mechanism (2026-11-20): a priority switch is only a *visual* switch if the
incoming scene paints its rect opaquely.** agb 0.25 has no way to re-point a `RegularBackground` at
a different map — the `Rc<Screenblock>` is fixed at `new()` and there is no `set_screenblock`
(`display/tiled/regular_background.rs:150-160`) — and DISPCNT's enables are derived from what a frame
showed rather than settable per background (§5.1). So "both scenes resident, switch by priority"
means the outgoing scene's BG stays *enabled and beneath* the incoming one, and every transparent
pixel in the incoming scene shows the outgoing scene through it — glyph gaps over the last screen's
content. The rule that makes the mechanism sound is therefore part of the scene contract rather than
a style note: **a scene declares `backing = true` and covers its rectangle with opaque tiles in its
own bank (one repeated tile plus its own map entries, both allocated once) unless it is only ever
shown over a declared picture.** That is §5.5's band-backing rule applied one layer down; AP-3b's L3
leg checks it by switching and screenshotting, because "a switch is only a priority write" is exactly
the claim that fails silently when the incoming scene is transparent.

**Geometry is declared here, not decided.** These rows are a starting point, and taste is
the reason they must stay movable: the honest way to let layout change while it is being
evaluated is to make it a *lookup*, not a rewrite.

**Two readings this table needs and did not carry. (1) Every count in it is a _tile_ row (8 px),
because that is the unit the hardware and agb allocate in; the text-row counts in parentheses come
from the declared `row_px`, and at a product pitch the two are never the same number.** The first
version of this table printed "17 rows / 16 rows" as though they were text rows, and every row count
downstream of it in §5.2 came out doubled — which is the single most expensive arithmetic error this
PR has corrected. **(2) The variants are not two attempts at the same thing: A is declared
`product = false`, an instrument that exists to answer the legibility and dither questions with the
8 px cut and a text-free title band, and B is `product = true`, the one a ROM ships.** AP-3 snapshots
both and gates fit and pool arithmetic on B alone, printing A as skipped-with-reason (§13).

| variant | chrome | title band | scene region | region canvas | tiles in canvas | the largest square art box that fits — **and what centring actually costs** |
|---|---|---|---|---|---|---|
| **A** — instrument face (§5.5's 8 px cut), and a title band that carries **no text line** | 2 rows | 1 row, art/bar only | **17 rows** (136 px — which at a declared 16 px pitch holds 8 text rows, not 17: see the pitch floor below) | 240×136 | 510 | **120×120** (15×15): vertically exact, one row of margin each side; horizontally **not centring-able** — 15 tiles in 30 columns leaves 7.5 each side, so it sits at column 7 or 8, **4 px off true centre**. In fact **no square is perfectly tile-centred in A**, which is a fact to print in the fit report rather than a fact to argue about |
| **B** — first product-shaped variant: a declared product face at a 16 px pitch (§5.5) | 2 rows | 2 rows (bar along the lower) | **16 rows** (128 px = **8** rows of text at 16 px pitch) | 240×128 | 480 | **128×128** (16×16): **exact on both axes** — it fills the region's rows and takes 7 columns of margin each side. A 15-tile image cannot be tile-centred in 16 rows, and that is the whole difference between the two rows of this column |

Both keep §7's 120×120 legal, and neither is final: **2a declares A and B**, and changing
one's mind afterwards is one config line and a re-run of a host test. **And no default can be
chosen from paper at all (stated at review, 2026-11-20): which variant is right depends on what
real dithered cover art looks like on real screens at those two band heights, on whether the
declared face is legible at 1× (§5.6), and the honest
expectation is that seeing it will change the UI more than it confirms a layout.** So 2a declares
and snapshots **both** (AP-3), 2b renders real art into both on hardware, and the default is
picked with a screen in hand — with the trade named rather than hidden, because it is not a
neutral choice: **A buys the 17-row scene region, and in exchange cannot centre any square art in
that region at all; B centres a 16×16 exactly and gives up a row of region and title to get it**
(last column of the table above, and the centring bullet under it). Whoever
picks the default should be looking at a rendered ROM when they do it, not at this table.

Two findings from
that table, both arguments for generating it rather than writing it down:

- **A region is a canvas, not an image budget.** A BG map entry is 16 bits carrying a **10-bit
  tile index**, so one background can address indices 0–1023 = the first 32 KiB of tile data =
  **two** 16 KiB character blocks; the blocks are alignment boundaries a block allocator keeps,
  not a display limit (§5.1 names agb's pools, which is where the real ceiling is). A 240×128
  or 240×136 canvas is 480–510 tiles: that is most of what agb 0.25.0's *regular* (BG) tile
  allocator can hand out at all — **512 4bpp slots in total, not per block** — so it leaves
  nothing for the glyph, chrome, bar and list tiles that draw from the same pool. So the spec
  separates `scene_region` (where a scene may draw) from `image_tiles` (what an image may
  occupy), and AP-4's VRAM audit checks the second. **The terms of that sum are computed from the
  declared fields, not estimated (2026-11-20):** an image term is `(box_w/8) × (box_h/8)` tiles —
  225 for 120×120, 256 for 128×128, 480–510 for a region canvas — and the text term is
  `rows × cells_per_row × ceil(line_box/8)` bounded above by the visible-cell count and below by
  `distinct_glyphs_on_screen × ceil(line_box/8)`, both halves read off the declared `row_px` and the
  declared face's own metrics (§5.1's measured table). A region canvas leaves essentially nothing
  for text and chrome; a 128×128 box beside an 8-row list at a 16 px pitch fits only through the
  distinct-glyph bound of §5.1 — **and that bound is reachable only by building the shared glyph
  atlas (§5.5), not by measuring a renderer that allocates per cell, so the leg (§13 AP-3b L4) is
  there to verify the refcounts rather than to discover the bound.** The
  tile arithmetic is unambiguous and belongs in the plan; **every cycle figure drawn from it is
  arithmetic too until a ROM measures the renderer** (§10.2's caveat, AP-3).
  **Ruled 2026-11-20, while scoping 2a's first PR: at 2a that image term is taken at the variant's
  own ceiling** — the largest box the variant derives, which is what its declared `image_tiles` is
  for — because the sum is an audit of a *variant*, never of an album, and `album.toml`'s `[art]`
  section is not parsed at 2a at all (D2's `config-section` refusal stays in force for it until 2b,
  §11's note (iii)). When `[art]` does land, its `box` key **selects from a list that is already
  derived and already audited** and can only lower the term: no album can raise a budget its variant
  was not refused against. One owner for the arithmetic (`album-layout-gen`), one phase for the key
  that names a particular box. That is what §9.0's register row now records, and why "nine 2a
  refusals" is annotated as eight fireable plus one named-and-skipped wherever it appears.
- **Centring is arithmetic, and in one axis it is not available here at all.** The region is
  **30 tiles wide — an even number — so any odd-tile-wide image is off true centre by half a
  tile** (4 px for a 15-tile image), and no even-tile-wide square fits variant A's **17** rows
  exactly. That is the arithmetic behind the last column, and it is the class of defect that
  should surface at bake time, in a report that prints the real pixel offsets, and not on
  hardware where someone can only look at it and be quietly unhappy. The same reasoning is why
  `scene_region` and `image_tiles` are separate fields: the region is where a scene *may*
  draw; the image budget is what a picture *can* occupy, and they are different questions.

**Two fields that did not exist in the earlier version of this section, and the arithmetic they
carry (2026-11-20, following the review of this PR).**
`row_px` is the **line pitch** in pixels for the rows of text an element draws: it must be a whole
number of 8 px tile rows (16, 24, 32) and at least the declared face's line box at its declared
raster. It is a declared field rather than a constant of this file for the same reason the bar grid
is one — a row count is a *consequence* of geometry, so quoting one (the old mock's "17 rows of
text", §5.5's old 8-row/30-cell framing) is the same mistake D9 records from a different direction.
**What it settles, stated plainly because it is the interaction the review flagged as "hard": a
line box, not a tile and not the tile pool, sets the smallest legal pitch; the pitch sets how many
text rows a region holds; and rows × tile-rows-per-line is what competes with art for the 512-slot
pool.** Measured against the two `.ttf` files this branch tracks, the floor is 16 px for every
raster of either face (§5.1's measured table), so **a band that draws text is at least two tile
rows tall** — and the 1-row title band in the diagram above is only expressible as a band that
draws *no text*, which is what variant A's row now says instead of "8 px face". `product` is the
variant's own flag for "may this ship", and it exists so the development instrument (§5.5's 8 px
ASCII cut) can stay declared without being mistaken for a product choice, and so AP-3's fit and
pool gates know which variants they are asked about (§13).

**It stays compile-time constant.** The tracked source is one file belonging to the
application — `examples/album_player/layout_spec.toml`, a table of named variants (band
rows, per-element tile spans, **per-element `row_px` (the line pitch: see below)**, bar cells and
pixel width, face per element, image box, `image_tiles` budget, per-variant `tile_budget` and
`resident_scenes` (§5.1), palette banks, and a `product = true/false` flag) — and the bake step's
layout emitter — **`album-layout-gen`**, the generator inside `scripts/album_bake.py` that §9.0's
register names, run by `make album-bake` and gated by `make album-layout-test`, which also emits
§5.5a's glyph atlas — generates `src/layout.rs` from it:
`pub const`s — including, from 2b PR-(3), the declared `SCENES` registry that `Select`'s cycle is a
filtered view of — `const _: () = assert!(…)` assertions that the bands tile exactly 20 rows and
that every declared box sits inside its own region, the derived legal art boxes (§7), the
derived title-fit box (§5.5), the derived list window — **visible rows = `floor(region_height /
row_px)`, never a row count in prose** (§5.2) — and the control slot table
(§5.3). **Config selects a variant; it never produces runtime geometry.** The precedent is
already twice in this tree (`scripts/frame_vectors.py` generates the rows §8's mock
consumes; `--emit-pin` generates a Rust module), and §2.3's instrument/inventory test
applies cleanly: the spec belongs to the application, not to an album, so it is tracked;
`layout.rs` is build output — pinned, `clean`ed, regenerated by `album-bake`.
`album.toml`'s part shrinks to `[layout] variant = "A"`, and an unknown variant is a named
refusal in D2's existing idiom, before a line of Rust is compiled.

Two guardrails, because "configurable" is otherwise how "unverified" arrives: **the
declared set is enumerable and `album-layout-test` snapshots every product-eligible declared variant**
(§5.0's `product` field; the others are named and skipped with the reason printed, §13 AP-3) — a
variant nobody snapshots is a variant nobody shipped — and **the ROM prints
its layout id** on the line where it already prints its input source and its fit verdict,
because an AP-7a hardware report that cannot be attributed to a geometry is a data point
nobody can use later.

And one new Make target, because it is what makes taste cheap: **`make album-preview
LAYOUT=A TITLE=… ART=cover.png`** — a host composite of one variant at 1× and 4× (ASCII +
PNG) built by the same code that bakes, under §12.1's one-implementation rule, the same
rule that makes A14's web UI a skin rather than a rewrite.

### 5.1 Layer budget

| layer | contents | tile-data cost (4bpp, 32 B/tile) — **tile data, not map cells**; the two ceilings are different things and this table has always conflated them | notes |
|---|---|---|---|
| BG0 | flat backdrop + progress-bar track + **the two chrome bands and the title band** | ~4 tiles of backdrop + the authored control/bar/title art — **far fewer tiles than the ~90 map cells those 3 bands occupy**, because a flat band repeats one tile across many cells, and the tile count comes from the control art (§5.3), not the band area + glyph tiles (see BG2) | the verdict-colour trick already in `text.rs`; **the chrome lives on a BG that is never hidden**, which is what keeps the DISPCNT enable mask non-empty (§4.5's agb gotcha) and makes "the controls are always there" structural rather than a convention |
| BG1 | album art (§7), scene `art` | 120×120 px → 15×15 = 225 tiles ≈ 7.2 KB; a region canvas is 480–510 tiles; `bleed = true` full-screen 600 tiles ≈ 19.2 KB | the mode is picked per album at bake time from the variant's derived list, and the **tile-pool** arithmetic below is what makes full-bleed the expensive one — a paragraph later, corrected 2026-11-20, because the reason this row used to give (a character-block count) was wrong |
| BG2 | text + progress-bar fill + list rows (scene `list`, `title_card`) | **not a constant, and not the face's glyph count either.** agb's text renderer allocates tile data *per rendered glyph* from the same pool (§5.1, R14), so this row's standing term is the glyph tiles a single frame can display — the declared boxes divided by the declared face's advance widths — plus whatever a hidden scene's map still points at. A face's *inventory* is a compile-time and ROM-size cost, not a standing tile cost (§5.5, Q19), so the number comes from the face the variant declares read at bake time, and neither 96 (the shipped development subset, `assets/fonts/README.md`) nor 64 is ever it | agb's text renderer + dynamic tiles. **The one row of this table that cannot be settled from the plan**, and the reason a per-variant `tile_budget` must be a declared field (§5.1, below) |
| BG3 | reserved (viz bars / a second scene held resident) | ≤ 256 tiles | see the switch mechanism below: a scene held here costs **a whole map, 2,048 B** — agb allocates a full 32×32 screenblock per background whatever region is drawn into it (`SCREENBLOCK_SIZE = 0x800`, `display/tiled.rs:293`; **eight such maps exist in total**, `:299`) — plus tile residency, and buys a switch that is a **priority** write. That figure was "1,200 B/screen" until this review; both numbers were wrong, in opposite directions |
| OBJ | cursor, sprite animation frames, "PAUSED" badge, and the §5.7 chrome candidate | second VRAM half | **new for this tree**: nothing shipped has drawn a sprite. AP-0 proves the *plumbing* (the OAM shadow and one object tile are allocated and counted) — **that is not the same as a product using OBJ, and Phase 2a's product does not** (§5.3, §11). §5.7's OBJ Chrome plan is the first candidate: ~56 OAM / ~202–206 OBJ tiles (~6.3–6.6 KiB of the separate 32 KiB pool) / 1 VPAL bank; the list stays BG. AP-6 owns the viz budget; AP-3c is the presentation probe. |
| palettes | 16 BG palettes × 15 usable colours, 16 OBJ palettes | — | allocation in §5.4 |

**What the tile pools actually are (corrected 2026-11-20 at review — that paragraph's
arithmetic was wrong, and correcting it makes §7's full-bleed option *cheaper*, not dearer).**
It said "one 16 KiB character block holds 512 4bpp tiles, so a full-bleed 600-tile image
cannot fit in one block". Neither half of that is how the hardware or agb works.

- **A map entry is 16 bits with a 10-bit tile index** (hardware; agb packs exactly that — a
  `u16` `tile_id` plus a `TileEffect` carrying 3 palette bits, 2 flip bits and a priority bit,
  `display/tiled.rs:125-128`). So **one background can point at tile indices 0–1023**: the
  first 32 KiB of tile data, i.e. **two** character blocks, by construction. A character block
  is an alignment boundary a *block allocator* keeps, not a display boundary — and §7's entire
  ruling (art under the title band, per-tile palette selection, `cover+breathe`) already
  depends on that map-index freedom, so the old claim contradicted a decision elsewhere in the
  plan.
- **The limit is agb 0.25.0's allocator, and it is readable in the sources this build resolves
  — so it belongs here, not in an AP.** `display/tiled/vram_manager/tile_allocator.rs:9` ends
  the affine pool at VRAM + 16 KiB, and `:25-37` opens two pools: **affine** from VRAM + 64 B
  with `256*2 − 2` = **510 4bpp slots**, and **regular (BG)** from VRAM + 16 KiB with
  `CHARBLOCK_SIZE*2/32 − 256*2` = **512 4bpp slots in total, not per block**. `:40-46`: when
  the regular pool is exhausted, `alloc_for_regular` **falls back into the affine pool**
  (splitting its 8bpp blocks in half). Exhaust both and agb *panics* — `"Ran out of video RAM
  for tiles"` — on the ROM, at run time (§14's R14).
- **Maps are a separate pool**: screenblocks of **2,048 B** starting at VRAM + 32 KiB, **eight**
  of them (`display/tiled.rs:292-300`), and `new_regular_background` takes one whole
  screenblock per background (`:398-404`) whether the drawn region is 30×17 or 32×32. That is
  why AP-0's probe log prints `bg_map=6144` for three backgrounds — 3 × 2,048
  (`examples/album_probe/README.md:183`) — and why this section's "1,200 B per full screen"
  was wrong.
- **OBJ tile data: read out of agb, and it is a *separate* pool (answered 2026-11-20 — this
  bullet used to say "a fact to read out of agb before Phase 4", and now it is read).**
  `object/sprites/sprite_allocator/sprite.rs:18-24` is the whole answer: `pub const TILE_SPRITE:
  usize = 0x06010000;` behind a `BlockAllocator` over `TILE_SPRITE .. TILE_SPRITE + 1024 * 8 * 4`
  — **32 KiB at VRAM + 64 KiB** — and like the BG pools it is a run-time allocator: sprite bytes
  are copied out of a ROM slice, `Drop` hands the block back, and the OAM tile index is
  `(ptr − 0x0601_0000) / 32` (`sprite.rs:92-94, 137-178`). The mapping is not an unknown either:
  `object.rs:30`, inside `initilise_oam`, runs `display_control.set_obj_character_mapping(true)` —
  **hardware mapping mode 1, which is the mode `TILE_SPRITE` is written for**, and nothing else in
  the crate touches that bit. The partition is therefore disjoint and total:
  BG tile data `0x600_0008..0x600_4000` (affine) then `0x600_4000..0x600_8000` (regular — R14's
  512-slot pool); BG maps `0x600_8000..0x600_C000` (the eight screenblocks, `tiled.rs:294-300`);
  **`0x600_C000..0x0601_0000` is a 16 KiB gap agb never allocates and no app-side API can
  reach** — the room is real and not usable, which matters to the trade above and should not be
  mistaken for headroom; and OBJ tiles `0x601_0000..0x0601_8000`, i.e. **1,024 4bpp tiles (512 at
  8bpp)** by `1024 * 8 * 4 = 32,768`. Four consequences, and they are the opposite shape to the
  ones this bullet feared: *(i)* a sprite does not compete with the BG pools — §7's "sprites must
  not be used to buy back the VRAM a full-bleed image spends" stands, on the real reason (the
  regions are adjacent, not shared) rather than the one it gave; *(ii)* a sprite has its **own**
  ceiling, which nothing in this plan declares and no BG-side refusal would ever see, so a
  sprite-heavy viz is a separate budget with its own number and **AP-6 owes that audit before A9's
  beat-driven sprite is designed** (§11 Phase 4, and the answer is not "free"); *(iii)* **DISPCNT
  bit 6 is load-bearing**: mode 0 would put 4bpp OBJ data at `0x600_4000`, which is exactly where
  agb's *regular BG pool begins* — a collision far worse than the one this bullet was worried
  about — so no `album-*` code may write that bit, and in agb only `tiled/registers.rs:11-19`
  defines the register and `tiled.rs:382-389` writes the BG half of it; *(iv)* AP-0's
  `obj_tile=32` term is the probe's own counter, not an agb region — which is why the search for
  an `OBJ_TILE` constant came up empty and was never a sign the mapping was undeclared.

So a 600-tile image **is** placeable — 600 art tiles + a glyph cache + the chrome art is inside
the ~1,020 slots the two pools can serve — but it **cannot** come out of the 512-slot regular
pool the chrome and glyphs draw from, so it necessarily spills into the affine pool and into
that pool's fallback behaviour. Whether that placement, and what it costs the text pool, is
acceptable is **an AP-3/AP-4 audit of pool state, not of character blocks** (§14's R14). What §7
rules is unchanged — legal, default off, audit-gated — for a smaller and different reason. And
one thing the plan should do instead of auditing after the fact: **compute the per-variant tile
sum at bake time and refuse there**, which is the price the residency ruling below actually
charges.

**How a scene is switched, and what it costs (corrected 2026-11-20 at review: the number in
this paragraph was not a map-write measurement).** The text this replaces took AP-0's upload
leg — `whole=346,057` against `per_tile sum=279,539`, so ~66,500 c over 225 cells, "≈295 c per
map cell by subtraction" — as the price of repointing a cell. It is not the price of a map
write, and three facts from the sources this build resolves matter more than the number did:

- **agb 0.25.0 has no map-only write path for a regular background.** Its public surface is
  `set_tile` (`regular_background.rs:197`), `set_tile_dynamic16` / `set_tile_dynamic256`,
  `swap_tiles` (`:278`), `fill_with`, `set_scroll_pos`, `set_priority` (`:435`) — and
  `set_tile` calls `set_tile_at_pos` (`:359`), which does the tile allocation, the refcount
  add/drop **and the map-entry write in the same call**. So AP-0's per-tile figures (726 c
  disarmed, 1,242 c live) *already include every map write in that leg*; the two legs differ in
  what surrounds the call, not in whether a cell gets repointed.
- **Therefore the 66.5 k difference is the cost of the loop around the calls**, not of a cell:
  ~295 c per iteration, against the probe's own measured per-call counter overhead of tens of
  cycles (raw 771 vs net 726, `examples/album_probe/README.md:169-176`). Nothing in this tree
  measures a map-only write, because agb has no map-only write to measure.
- **A whole-region change is therefore 600 `set_tile` calls: ~0.44 M c at the 726 c figure,
  0.66–0.75 M c at the live 1,096–1,242 c figures** — 2.5–4× the ~177 k c this section
  quoted, and **1.7–2.8 lossy periods** (a lossy period is 512 samples at 32,768 Hz = 262,144 c,
  §4.2b; a FLAC period is `PERIOD_CYCLES = 573,440`, `examples/album_player/src/geometry.rs:154-190`),
  i.e. 12–21 lossy periods' *entire* spare margin (13.73 points of 262,144 c ≈ 36.0 k c,
  §10.2). A whole-region rewrite during playback is out, and by a wider margin than was written.
- **An unchanged cell is cheaper, and it is not free — the measurement corrected the plan's
  expectation (2026-11-25).** `set_tile_at_pos` returns before touching the map when the old and
  new entries are equal (`:375-378`), but only *after* `add_tile` on the new tile and `remove_tile`
  on the old (`:360-373`), so the allocator's lookup and the reference count are always paid. AP-3b
  L1 measured an unchanged cell at **407 c against 489 c changed — 83%, not "near-free"**. The
  element-addressed rule (rule 2 below) still earns its place: the saving is the map store and the
  dirty mark, and the changed-cells clock case is 963 c for two cells against ≈5,291 c for an
  11-cell redraw. But the claim to keep is "tracks changed cells", not "a re-show costs nothing".

**Which is how this section reads now: an ordering, with the arithmetic left as arithmetic.** The
figures above are single-tile-write numbers multiplied by cell counts — enough to rank the three
mechanisms, **not a measurement of this application's renderer, which no ROM has run yet**.
**AP-3b (§13) is that measurement, and the honest thing to say about it is that it is not a new
instrument: most of one is already in the tree.** `examples/album_probe/src/bin/art_probe.rs:94-135`
already times per-cell `set_tile` in both a live-audio and an audio-disarmed leg through a `Fold`
accumulator (`examples/album_probe/src/probe_hw.rs:307-345`) fed by the timer2/3 cascade `Counter`
(`examples/e5_fetch/src/timer.rs`, `#[path]`-shared into `probe_hw`), with an 8-frame warmup and a
discarded first pass (`art_probe.rs:57, 196-202`); the live-producer-every-frame shape this app needs
is `ui_probe.rs:251-285`; arms are compile-time tables per bin (`ui_probe.rs:133-140`), so a leg is
a new `src/bin/*.rs` plus a `[[bin]]` entry in `examples/album_probe/Cargo.toml:45-66` plus two
Makefile lines (`:1149-1154`, `:1164-1172`), and the verdict is the existing grep-able
`[album-probe] PASS` shape. What does **not** exist anywhere in the tree: any call to
`set_priority` in code (`grep -rn set_priority --include=*.rs`, outside `target/`, is empty —
only this plan names it), a region change of ~600 cells, and a leg that asks the
pools to over-fill on purpose to watch R14's panic happen. So: one new arm, three legs, and every
"arithmetic" tag in this file and §10.2 has a number behind it. **That measurement landed
2026-11-25 (2b PR-(2)) and its numbers are in §13's AP-3b row and
`examples/album_probe/README.md`**: the 600-cell whole-region change is 550,437 c (917 c/cell,
2.10 lossy periods), the switch is 212,386 c with a 73 c priority store, and the stand-up is 460 of
512 slots with the shared atlas proven by 45 references on one glyph slot. The ordering:

1. **Pre-build the tile data and the tilemap of every scene the variant allows, keep them
   resident, and switch by BG priority** — the mechanism is named precisely this time, because
   "a DISPCNT/layer-enable write" implied an enable bit the app does not have: agb has no
   per-background hide on a `RegularBackground`. A background is *not shown* by leaving it out
   of a frame, and agb derives DISPCNT's enable bits from what the frame showed
   (`display/tiled.rs:384-389`), so with three scenes live the mask is never empty (§4.5's
   gotcha resolves itself here rather than needing care). The one real primitive is
   `set_priority` (`regular_background.rs:435`): both scenes are shown into every frame and the
   visible one wins by priority — **and the timing of that flip is part of the mechanism, not an
   implementation detail**: `set_priority` only stores the value (`:435-438`), and agb writes the
   control word in `bg_ctrl_value()` when that background is next `show()`n (`:440-447`), so a
   switch lands at the next commit and never mid-frame, and an in-measurement priority flip has to
   be timed across a frame boundary — one of AP-3b's legs (§13). That is a register write of tens
   of cycles per frame and a
   per-frame `show()` of more than one background — which is why AP-3 measures the switch
   *rather than* assuming it; and the price of the ruling, stated plainly, is in the next
   paragraph.
2. **Partial map rewrites, changed cells only** — for content that changes in place (the
   list scene scrolling a row, a title line). ≤ the element's cell count, addressed, never
   a whole screen (§10.2).
3. **Tile uploads, only when a scene's tiles are not resident** — art scenes, the first
   time a scene is shown. Never on a switch between resident scenes, and always sliced
   across periods (§10.2's row, which this replaces).

**What ruling 1 trades, stated instead of discovered late.** It buys time with VRAM: a scene
change costs a priority write instead of an upload — *and* it requires the incoming scene to cover
its own rectangle opaquely, since the outgoing scene stays enabled underneath it (§5.2's scene
contract, added 2026-11-20) — and the price is that **every scene the
variant allows holds its tile data and its map at the same time**, in one 512-slot pool, with
only eight maps available in total and one of them permanently the chrome's. So the sum moves to
bake time: `layout_spec.toml` declares a per-variant `tile_budget` — region `image_tiles` + the
declared face's glyph cache + authored control/bar art + list-text tiles — the bake adds it up
and **refuses before compiling** if a variant cannot hold its own scene set (the same shape as
§7's derived-art-box refusal, and AP-3's snapshot table carries the refusal as a result) —
**with the image term taken at the variant's derived ceiling for as long as `[art]` stays a refused
section (§5.0)** — with AP-4's link-time audit as the check that the sum was honest.
**A list window at the 16 px product pitch plus a
128×128 cover plus the chrome's own art is where this starts to bite — that combination needs
~444 slots at the *best* bound and more than 950 at the pessimistic one, against 512 (§5.1's
measured tables); a third resident content scene, or full-bleed art, is where it does not fit at
all.** That is the trade, and it is a deliberate one:
**VRAM for time, priced in this section rather than at AP-4.**

**And the ceiling is not only tiles; it is layers, and it is a hardware number (2026-11-20).**
Mode 0 has four background layers, this section's own table has always carried BG3 as "reserved",
and the chrome permanently owns BG0 plus one of the eight maps — so the honest cap is
**`resident_scenes ≤ 3`, declared in the variant and refused at bake time above that** (§9.0's
register). Three consequences follow from the number rather than being negotiated in a scene
module: *(i)* the list and the art scene take two of the three, which leaves exactly one, so a
`title_card` cannot also be resident and §5.2's "list + art + card" reading of residency was
asking for four; *(ii)* a variant that wants more than three content scenes at once is asking for
the re-point or upload path (options (c)/(d) below), not for a fourth layer; *(iii)* AP-3b's switch
leg must name *which* cap it was measured under, because a three-layer switch and a two-layer
switch with a layer free are not the same measurement. Nothing in the plan was violating this — it
had no number to violate, which is how §5.2 could describe three resident scenes while its own
table listed four scenes and this table reserved one layer.

**The text term of that sum, measured (2026-11-20).** The rule above says the glyph term comes
from the declared face's metrics; here is what those metrics are, off the two `.ttf` files this
branch tracks (FontTools: `head`/`hhea`/`OS/2` line metrics and per-glyph advance widths). Both
files are `glyf` outlines with **no bitmap strike in either** (`glyf`+`loca`), which is the fact
that decides what sizes may be declared: agb rasterises an *outline* at whatever pixel size it is
asked for, so a raster that is neither the design size nor a whole multiple of it puts glyph stems
between the tile grid's lines.

Re-measured on this branch (2026-11-20, second pass) with FontTools 4.68.0: `hhea` line metrics
for the line box, `ControlBoundsPen` for the tallest real ink, per-glyph advances for capacity, and
a grid test that asks whether **every outline coordinate and the advance** land on integers at the
scaled raster. The denominator in the grid column is **94**: `maxp` reports 96 glyphs in the shipped subset and
its `cmap` maps 95 codepoints (matching `assets/fonts/README.md:8,13` — the 96th glyph is
`.notdef`), and of the 95 mapped ones every one has an outline except U+0020, which has an advance
and no ink. So 94 is "the glyphs that can actually put pixels on a tile", and it is also the ceiling
on distinct glyph images a list screen can draw, because an authored ASCII cut can only ever draw
what the cut contains — the parent `ark-pixel` file's **2,881 glyphs / 2,880 mapped codepoints /
349,400 B** never reach the ROM, and that is why §5.5 treats a *cut* as the unit of every font
decision rather than a face.

| face @ declared raster | scale | grid-aligned? | line box (`hhea`) | **min `row_px`** | tallest ink | avg advance (printable) | chars in a 240 px line | tile slots **per text row** (upper / distinct cap) |
|---|---|---|---|---|---|---|---|---|
| `fusion-pixel-8px` @ 4 | 0.5× | **no — 94 of 94** | 6 | 8 | 4.0 | 2.28 | 105 | 105 / ≤94 |
| `fusion-pixel-8px` @ 8 (design size) | 1× | **yes** | 12 | **16** | 8.0 | 4.56 | 52 | 104 / ≤188 |
| `fusion-pixel-8px` @ 12 | 1.5× | **no — 94 of 94** | 18 | 24 | 12.0 | 6.85 | 35 | 105 / ≤282 |
| `fusion-pixel-8px` @ 16 | 2× | **yes** | 24 | 24 | 16.0 | 9.13 | 26 | 78 / ≤282 |
| `ark-pixel-10px` @ 5 | 0.5× | **no — 94 of 94** | 8 | 8 | 4.5 | 2.72 | 88 | 88 / ≤94 |
| `ark-pixel-10px` @ 8 | 0.8× | **no — 94 of 94** | 13 | **16** | 7.2 | 4.35 | 55 | 110 / ≤188 |
| `ark-pixel-10px` @ 10 (design size) | 1× | **yes** | 16 | **16** | 9.0 | 5.44 | 44 | 88 / ≤188 |
| `ark-pixel-10px` @ 12 | 1.2× | **no — 94 of 94** | 20 | 24 | 10.8 | 6.52 | 36 | 108 / ≤282 |
| `ark-pixel-10px` @ 16 | 1.6× | **no — 94 of 94** | 26 | 32 | 14.4 | 8.70 | 27 | 108 / ≤376 |
| `ark-pixel-10px` @ 20 | 2× | **yes** | 32 | 32 | 18.0 | 10.87 | 22 | 88 / ≤376 |

Three things this table says that the earlier version of it did not, all of them cheap to get wrong:
- **At a non-design scale the off-grid count is not a fraction, it is everything** — 94 of 94 at
  1.2×, 1.5×, 1.6× and 0.8×. Only 1× and 2× put every coordinate on the grid, and the half-scale
  rasters are off-grid too. A halved pixel face is not a smaller pixel face; it is a blurry one with
  4–4.5 px of ink. That is the *arithmetic* behind §5.5's "design size or a whole multiple" rule and
  behind `pitch-not-tileable`, not a taste preference.
- **The ink column is not the em column.** `ark-pixel` at its 10 px design size draws 9.0 px of
  tallest ink inside a 16 px line box; `fusion-pixel` at 8 px draws 8.0 inside 12. Anyone pricing a
  band from the design size over-prices the ink and under-prices the box, and the box is what
  `row_px` has to clear.
- **Capacity is a pessimistic full-width number.** "chars in a 240 px line" assumes a line edge to
  edge across the whole screen; a list with margins and a right-hand duration column sees fewer, so
  every count below is an upper bound on what fits, never a floor.

The last column is a **pair** on purpose, because the two bounds differ by four to eight times and
which one the ROM is actually living under decides whether a scene set fits:
- **upper** = every glyph cell a line could show × the tile rows its line box spans, which is how
  agb allocates: one `DynamicTile16` per drawn cell (`display/font/tiled.rs:126,139-140`);
- **distinct cap** = the number of *different* glyph images the cut can draw × tile rows per line —
  the term reached **only when repeats are the same `(tileset, index)` key**, which is what agb's
  VRAM manager dedupes on, with a reference count (`display/tiled/vram_manager.rs:707,728`).
  **That is a property of a shared tileset, not of the text path this tree has, and the first
  version of this bullet claimed it for agb's text renderer (corrected 2026-11-20 from source):**
  a dynamic tile is given its **own allocated slot** in the regular region and reports that slot as
  its index — `tile_set()` hands back a `TileSet` over the whole VRAM area (`vram_manager.rs:287-297`)
  and `tile_id()` is the offset of *this* tile's buffer inside it (`:299-302`), with the buffer
  itself owned per instance (`:224-227`) — so two cells drawing the same glyph occupy two slots and
  are two keys, never one. **The per-cell bound is therefore the only bound the dynamic path can
  reach, and "identical content dedupes" is true of a shared tileset, not of this one.** Sharing needs one *common* tileset addressed by
  index, which in agb means a compile-time `TileSet { tiles: &'static [u8] }`
  (`vram_manager.rs:45-48`) written with `set_tile` (`regular_background.rs:197`) — the path AP-0
  measured. So the two columns are not two estimates of one build: **they are two builds**, and
  which one a variant uses is a 2a decision with a pool consequence (§5.5's static glyph atlas),
  not something AP-3b can discover about agb's internals. **This file's old `~287` sat in the
  middle of that range with no leg behind it, and the earlier version of this very table printed
  bounds it could not reproduce** (its `@ 8` row mixed one tile row per line with two; its
  `@ 20` line box came from a different rounding than the `@ 16` row above it) — so the numbers
  above were recomputed from the files, in one pass, with the method stated, and every derived
  number in this section follows from that one table.

**The interaction §5.2's list rows and the pool have with each other, stated as arithmetic:**

(128 px region, 240 px wide lines — the same pessimism as above; ✔/✘ are computed at the
**distinct** bound, i.e. the best case a variant can honestly claim.)

The two art terms, so the last two columns are checkable by hand: **128×128 = 16×16 = 256 tiles**,
**160×120 = 20×15 = 300 tiles**; each total is `text term + art term` against the 512-slot regular
pool. *(The 160×120 column was wrong in this pass's first attempt, computed at 240 tiles — which is
a 128×120 box, a shape from the pre-band layout this PR retired. Printing the terms is what let the
error be found rather than admired.)*

| declared face @ raster | `row_px` | text rows in a 128 px region | text term, **upper** | text term, **distinct cap** | + 128×128 art (256) | + 160×120 art (300) |
|---|---|---|---|---|---|---|
| `fusion-pixel-8px` @ 8 | 16 | 8 | 832 | ≤188 | 444 ✔ *(upper 1,088 ✘)* | 488 ✔ *(upper 1,132 ✘)* |
| `ark-pixel-10px` @ 10 | 16 | 8 | 704 | ≤188 | 444 ✔ *(upper 960 ✘)* | 488 ✔ *(upper 1,004 ✘)* |
| `fusion-pixel-8px` @ 16 | 24 | 5 | 390 | ≤282 | 538 ✘ | 582 ✘ |
| `ark-pixel-10px` @ 20 | 32 | 4 | 352 | ≤376 | 632 ✘ | 676 ✘ |
| `fusion-pixel-8px` @ 4 (half-scale) | 8 | 16 | 1,680 | ≤94 | 350 ✔ | 394 ✔ |

**Read the two 16 px rows as the plan's position on the shared-glyph build, and as a failure on the
one the tree ships.** Both are ✔ at the distinct bound and ✘ at the upper one, and per the bullet
above the upper one is exactly what `RegularBackgroundTextRenderer` allocates today: **8 rows × 44
cells × 2 tile rows = 704 slots against 512, before a single art tile** — and the six-row fallback
does not rescue it either, because `6 × 44 × 2 = 528` is over the pool on its own. Five rows fit
without art (440) and not with a 128×128 cover (696). So the distinct column is not an outcome to
measure and hope for: **it is a build — the static glyph atlas of §5.5**, which is 94 glyphs × 2
tile rows = **188 tiles, ≈6 KB of ROM**, rasterised once from the tracked `.ttf` at the declared
raster and addressed by index, after which repeats really are one tile. §9.0's
`image-tiles-over-budget` row therefore flags the *atlas* variant rather than refusing it, and
AP-3b's pool leg prints what that allocation really did with refcounts rather than assuming it.
The margin is also not what it looks like: **the chrome's own art
(§5.3's controls, §6's bar-pattern tiles) and any second scene's tiles come out of the 68 slots
that a 444 leaves against 512**, so a plan that wants eight list rows *and* a 128×128 cover is
committing to the refcount behaviour of a text renderer no ROM in this tree has ever exercised.

Read it as a ruling, in the order the rows teach:
- **Eight list rows and a 128×128 cover can coexist inside the regular pool — at a 16 px pitch, and
  only because the distinct bound is what a list of repeated letters actually draws.** That is the
  whole of the "bigger font" answer at the size of this screen: 16 px is not a taste sweet spot, it
  is the largest pitch at which the app keeps eight rows and an art box, and it is already tight.
- **A face whose line box wants 24 px loses three of the eight rows to the pitch alone and then
  spends the pool on itself anyway** — it fails at both bounds. So does 32 px. There is no version
  of "bigger" that is free here.
- **The last row is the cautionary one, and it shows why the floor is a *face* rule and not a pool
  rule.** A half-scale raster gives an 8 px pitch, sixteen rows and a pool term that fits with room
  to spare — with 4 px of ink and every glyph off-grid. Nothing in the pool arithmetic objects to
  it; the reason it is refused is §5.6 (a person has to be able to read it), and that is why
  `pitch-not-tileable` cites the declared face's metrics rather than a range of allowed numbers.
  **The old mock's 8 px text rows were never a layout option — they were the absence of a line box
  in the arithmetic.**
- So if the product wants a bigger face *and* eight rows, the lever is neither the layout nor the
  pool: **it is a face with a shorter line box than its ink wants** — a real ~12 px design face
  whose `hhea` pair stays inside 16 px, which is not something either tracked file can be
  persuaded into. §5.5 allows that asset and Q19 keeps the question open; and 2a does not need it,
  because the 16 px cut above is measurable today.

**The residency risk that trade carries, named with options rather than resolved.** agb
reference-counts tiles by *(tileset, index)*, not by what a map happens to point at:
`set_tile_at_pos` does `add_tile` on the new tile and `remove_tile` on the old one (`:364-373`).
A hidden scene's map keeps its tiles allocated only while nothing rewrites *those* cells; if two
scenes share a glyph cache and one rewrites its cells against a different tileset or index, the
other scene's text can end up pointing at a freed tile, and agb's own failure mode is a panic on
the ROM. Options, in the order I would try them:

- **(a) One tileset per scene, no sharing.** The invariant is trivial; the cost is duplicate
  tile data wherever two scenes both draw text — digits for the clock and the list rows, at
  minimum. Only safe by construction, and the most expensive.
- **(b) Share the cache, and never rewrite a hidden scene's cells.** The hidden scene's map is
  frozen; only the visible scene — plus the chrome, which is never hidden — writes. This is what
  ruling 1 and rule 2 already imply, it costs nothing, and the discipline it demands is stated
  in §5.2's contract: **a scene's cells belong to whoever is showing them.**
- **(c) Keep tile data resident, re-point the incoming scene's map on the switch.** Correct and
  simple, and it re-pays the whole-region cost above on every switch — admissible only where a
  switch is rare *and* the audio is being re-primed anyway (a boot, a track change).
- **(d) Do not hold it resident; pay the upload on every show, sliced.** No pool pressure, and
  the worst kind of user-visible cost; the only honest use of it is an *art* scene, which pays
  once per image and can be sliced (§7, §10.2).

AP-3's switch measurement says which of these it was taken under — (b) and (c) differ by two
orders of magnitude at the exact row §10.2 cares about — and whichever the shell picks belongs in
`scene.rs`'s header comment as the invariant, the way `meta.rs:55-62` states its own.


### 5.2 The surface, its scenes, and the boot flow

One surface, one flow. What changes is which scene holds the region.

```
boot ──► scene `title_card` (A12) ──Start──► the default scene ──Select──► the next scene
         album title / artist / year,       `art` if the album ships art,
         N tracks · M minutes,             else `blank` (2a) / `list` (2b)
         "PRESS START"
         (the chrome is already up: the surface draws it before any scene runs)

         ⚠ the flow above is 2b's. 2a has no `title_card` scene to boot into: it boots to §5.2's
            option (c) — chrome + title band + `blank`, audio auto-starting, no press required.

   any scene ──► ERROR / verdict (→ §14 R5): "DATA ERROR" + the named term, auto-skip, stop at end
```

**2a's boot state is settled: option (c) below** — chrome + title band + `blank`, audio
auto-starting as the gate ROM does today. It came up at review on 2026-11-20 as an open question and
it is decided here rather than left to 2a's PR, for the reason this file gives for everything else: a
row that names no option ships the smallest one by default, and then a product decision has been made
by a layout phase without anyone making it. The diagram above starts at `title_card`, which is a 2b
scene, so it is not 2a's flow; §11's 2a row names (c). The options were on the table like this, and
the reasons for the choice are mostly the reasons against the other two:

- **(a) Boot straight into playback, exactly as the ROM does today** — chrome + `blank`, audio
  auto-starts, and AP-1's harness keeps working unchanged (its replayed key log needs no new
  press, and its "audio begins within 1.5 s of boot" term still measures what it measures today,
  578–580 ms). Costs nothing, verifies nothing new; and it means the first thing a person sees on
  a real unit is a record already running, which is a product decision made by default rather than
  on purpose.
- **(b) Surface up, transport waiting** — boot raises chrome + `blank` with a *chrome* state
  ("PRESS START" in the title band, not a scene), and Start starts audio. No new scene, no new
  asset, so it stays inside 2a's scope, and it exercises the chrome's drawn states on hardware —
  which is the only way 2a can be called "more playable than the gate ROM" and mean it. Against
  it: it is a boot-time input requirement, and **it changes what AP-1's row measures.** That row's
  term is "audio begins within 1.5 s of boot", measured on a run where nothing is pressed, and the
  script is replayed through `keys.rs` (§13's AP-1 row) with no Start press at t=0 today. So
  either that log grows a press and the term becomes "within 1.5 s of the press that starts it"
  (a different term, and a second one to keep), or the 1.5 s term is measured on an auto-start
  arm alongside a gated one. **That has to be said before 2a's PR, not discovered inside a green
  gate that quietly moved.**
- **(c) Boot into a minimal now-playing state — chosen.** Chrome + title band + `blank`, audio
  starts on its own at track 0, and a person's first press is a transport action. It is the only one
  of the three that puts the title band, the clock and the control column in front of a human
  **without any of 2b's content**, and it does it on an audio path byte-identical to the gate ROM's,
  so **AP-1's witness and its 1.5 s term survive untouched** — which is the whole trick of the
  choice: the visible surface changes, the measured contract does not. Its one cost is (a)'s cost and
  it is stated rather than absorbed: *"it started playing by itself"* is a decision about a player,
  not about a layout, and this file declines to let a layout phase make it silently. Q20 (§16) is
  where the surface order and the press-before-audio question are answered, in 2b, with composites in
  hand; **the PR that lands a press before the first sample restates AP-1 in that same PR.**

Not on the table: booting into a scene 2a does not have. **If 2a's row still names none of (a),
(b), (c), the phase ships (a) by default and the title-card flow stays untested until 2b — which
is a decision worth making in words rather than inheriting from a phase table.**

**Landed: option (c), 2a PR-(3), 2026-11-23.** The surface is `src/surface.rs` and `src/chrome.rs`.
What the ROM draws, and nothing else: the always-shown bands from `layout::BANDS` (the clock
`m:ss/m:ss`, the state word, the five authored control icons on the chrome row; the title line and
§6's bar on the title band) and the scene region filled flat by `blank`. The audio path is unchanged
— nothing is pressed before the first sample — so AP-1's term still measures what it measured.

**The display is three backgrounds, not one composite.** BG0 is the chrome/title art, BG1 the scene
region, BG2 the foreground (glyphs, control icons and the bar). The split is what lets text sit *over*
a coloured/textured chrome rather than punching backdrop-coloured holes through it: a glyph or icon
tile is mostly transparent, and a transparent pixel reveals the layer below. It also gives 2b's scene
switch the separate region layer §5.2 already required. Every visible cell of all three backgrounds is
written explicitly with the transparent `chrome::FILL_PAGE` before content, because a map cell left at
agb's reserved blank tile index reads whatever VRAM held — mGBA initialises it to zero, while a
Quick-Boot loader does not (agb's own entrypoint clears `.bss` for that reason and leaves VRAM alone).
The diagnostic `no-page-fill` feature omits that fill for a hardware A/B (`make album-diff-rom`).

Four things about the implementation that belong in the plan rather than in a diff:

- **Text is placed one glyph per 8 px cell**, from §5.5a's static atlas (`tile_for(c, row)`), which
  is the addressing §5.1's pool audit and §10.2's write costs already assume. That is sound only
  while every advance in the declared face is ≤ one cell, and `surface.rs` makes that a
  `const _: () = assert!` over **every face a declared text element names**, not just `FACES[0]` —
  so a future face that trips it is a build failure rather than a line whose glyphs overlap.
- **The draw path's cut is a cell cut, not an advance cut.** For a face with advances ≤ 8 px the
  cell bound is the stricter of §5.5's two bounds, so "characters ≤ cells" is what a drawn line
  must satisfy; `surface::line_for` implements that and the host gate pins its relationship to
  `glyphs::elide` (the advance verdict the bake prints notes from) on the shipped face. §5.5a's
  generalization note is where the premise lives.
- **The bar's cell is a distinct type.** `BarCell` is derived from the painted sample position and the
  track's §6 reciprocal; it cannot be constructed from a bare `u32` sample count, which is how the
  first ROM filled the bar on its first frame. The foreground's transparent padding is
  `chrome::FILL_PAGE`, so the band art below it shows through.
- **The reference is `chrome.rs`'s authored tile table**, a `u8`-per-row bitmap block in the spirit
  of `ui_probe.rs`'s hand-written tiles, not a font glyph and not an image file: five placeholder
  icons (diamond, play triangle, `|<`, `>|`, a return glyph) that differ in **shape** as §5.6
  requires. Until §5.3's `PAUSED` badge lands in 2b, **the transport state is carried by the word**
  in `keylabels` (`PLAYING`, `PAUSED`, `BUFFER`, …) — the alternative §5.6 names in its own
  sentence — and the icon is the *affordance*, not the state.

**Two bugs the first run of that surface had, both now caught on the host — recorded because each is a
class, not an incident (2026-11-23).** The first ROM built from it drew correct *positions* with
garbage tile content, and the host gate passed. *(i)* `draw_text` indexed the line's characters by the
row-major cell index instead of by the column, so tile row 0 drew the text and tile row 1 read past
its end: every glyph lost its bottom half. *(ii)* Every write was sent to the **chrome** tileset,
including glyph indices — and agb copies a tile's bytes out of the tileset a write names with a raw
pointer whose bounds check lives in `TileSet::get_tile_data`, which `set_tile` never calls
(`vram_manager.rs:962-1000`). So an atlas index into an 18-tile set read past the end of that set in
ROM and uploaded whatever followed it, silently: no panic, no refusal, just wrong pixels. Three
things came out of it, and the third is the one that generalises: a `Write` now carries the **tileset
it means** (`surface::Source`) rather than leaving it to the caller, `apply_to` dispatches on it, and
`tests/album_surface_host.rs` has a leg that walks every planned write and asserts the index exists
**in the set it names** — because a picture cannot see this one (both tilesets look the same size in
an image) and the device cannot report it (the path is unchecked by design). The row bug is caught by
the same file's composite: the title's two rows are now asserted *in full*, which is what would have
shown it.

**Follow-up fixes in the same PR (review, 2026-11-24), because the same class kept appearing:**
the progress bar was handed `position::paint_position`'s **sample** count as if it were a §6 cell, so
it filled every column on the first frame; it is now a distinct `surface::BarCell` derived from the
track's reciprocal, and a sample count cannot compile in its place. The icon packer wrote the left
pixel of each pair in the **high** nibble while every other authoring function and the atlas used the
low one; the five placeholders were pair-swapped, and the distinctness check and the ASCII composite
could not see it because both read the packed bytes — a unit test now unpacks every icon row against
its bitmap. The gate's composite read `w.tile` without `w.source`, so an early-punctuation glyph whose
atlas index collides with a chrome index was printed as a fill; the composite and the preview's layer
fall-through are source-aware. And the display is **three backgrounds** (art / scene / foreground)
with an explicit transparent page fill over every visible cell, plus the `no-page-fill` diagnostic
and `make album-diff-rom` for the hardware question below.

**And the instrument behind that choice, so it is not made blind (re-checked 2026-11-20 — the first
version of this paragraph invented the mechanism it described).** Boot-to-audio **is** measured and
printed, with its decomposition: the ROM starts a timer-pair clock before it reads the manifest and
prints `boot: parse/open/gate/prefill/total` in cycles, then that same total in ms
(`album_play_main.rs:3866-3877`). Both logs this tree keeps record it — `album-test.log:9-10`:
9,477,295 c = **564 ms**; `album-rom-run.log:9-10`: 9,744,862 c = **580 ms** — and in both, the
`open` and `prefill` legs are 98.8 % of both totals (4.34 M + 5.02 M of 9.48 M; 4.34 M + 5.29 M
of 9.74 M), which is §4.3/§6's asset-reading answer rather than
a UI one. **What does not exist is a threshold.** No 1,500 ms constant, no boot-budget check, no
`Refusal::BootTooSlow`: those three were this paragraph's own invention. `BOOT_BUDGET`, `BootTooSlow`
and a 1,500 ms threshold are absent from `examples/`, `crates/`, `scripts/` and `tests/`. So the 1.5 s term is produced for a
human and compared by nobody — which is exactly §13's AP-1 point, and it is now stated off the code
rather than off a half-remembered constant.
For (b): nothing needs building either. `Replay::new(&[(0, keys::BTN_START), …])` applies on the first
poll, because `advance()` takes every entry with `at <= frame` (`keys.rs:633-640`; the type is
`keys.rs:564`, the constructor `:592`) — the current `GATE_REPLAY_SCRIPT` simply happens to start at
frame 8 (`album_play_main.rs:1338-1339`) — so a Start press at frame 0 is expressible in the harness
that already drives the gates, and the boot clock would start at frame 0 rather than at the press,
since that is what "from boot" names. What (b) actually costs is the product decision, not a rig.

| scene | phase | what it draws **in the region** | geometry, all derived | refuses |
|---|---|---|---|---|
| `blank` | **2a** | a flat fill of the region at a declared palette entry | region cells, nothing else | — its whole purpose is that 2a needs no asset, no art tool and no new font |
| `list` (A11) | **2b** | rows `NN Title … m:ss`, cursor, now-playing mark, and an album header (`n/N`, album total — §6) | **visible rows = `floor(region height ÷ row_px)`** — 8 rows for a 128 px region at the 16 px pitch §5.5's measured floor makes minimum, not the "17/16" the earlier version of this table claimed (those were *tile* rows counted as text rows, §5.0); **an album longer than the window is the normal case, not the overflow case** — the fixture is a 12-track album in a 8-row window, generated by the host pack helpers rather than committed (§2.3), and the window scrolls with the cursor as a **tile**, not a sprite (§5.3) | a variant whose region holds under ~4 rows at its declared pitch, where a list has stopped being a list; and, from 2a, a declared `row_px` that is not a whole tile-row count or is below the declared face's line box (§5.5) |
| `title_card` (A12) | **2b** | album art when present, album title/artist/year, `N tracks · M minutes`, "PRESS START" | art box from §7's derived list; every text span derived | an art box that is not inside its own region |
| `art` (A8) | **2b**, AP-4 | the cover, at one of the variant's declared boxes | §7 | an image above the variant's `image_tiles`, or one whose tile data exceeds what the variant's declared `tile_budget` can serve — and note that in agb that is a **pool** question, not a character-block one, and running out is a panic on hardware rather than a refusal at the bake (§5.1, R14) |
| `viz` (A9), game (A15) | 4, 6 | §8, §12.2 | — | — |

**What that table is doing, which is the point of the model and not decoration:**

- **Geometry is derived, never repeated.** The list's window comes out of a division on the
  region *by the declared line pitch* (§5.0's `row_px`), so picking a variant resizes the list and
  changing the face re-derives it; there is no "17 rows" (or any other row count) in a Rust file, and
  no `TITLE_BOX_MAX`-shaped sibling of it (§11's D9 named the duplication that was in the tree and
  records which PR removed each copy of it).
- **A scene cannot invalidate a measurement.** A scene draws into dynamic tiles and its own
  map (§5.1) and may not touch a chrome cell, so an AP-3 cost line for the chrome and the
  bar stays true whichever scene is showing. That is what makes the per-element cost
  records of §10.2 durable across scene work.

**The verdict screen is not a scene, deliberately.** The `DATA ERROR` / `PASSED` /
`FAILED` path in `album_play_main.rs` and `main.rs` is shipped, host-tested (it is what AP-1's
verdict terms print) and hardware-witnessed (**AP-7a legs (2)–(4)** — including the colourblind
verifier reading `PASSED` on a blue screen, which is §5.6's scheme witnessed on a real unit), and
it takes the whole screen — the
*only* full-screen takeover this plan allows, and the rule that makes it legal is that **a
takeover is permitted exactly when the audio path has stopped**: nothing is playing, so no
redraw competes with a DMA deadline (§10.2). Re-homing it as a scene would reopen landed,
witnessed code for a layout reason, and its §5.6 standing as "the model, not the
exception" is unchanged.

**Paused is chrome, not a screen.** The transport state is *shown* by the chrome — the
control's drawn state, plus a word per §5.6's rule that a state is carried by more than
hue. No pause menu exists in 2a or 2b: settings stay a §16 non-goal, and the only runtime
control the plan currently wants is Select (§5.3), a scene change, which is a register
write (§5.1) and not a menu.

**A scene change must not upload tiles while audio is playing.** Scenes that can appear
during playback are resident and switch by **BG priority** (§5.1's ordering, which names the
one agb API that does it); an art
scene pays its upload once, on first show, sliced across periods (§10.2).

**A scene's redraw cadence is declared, not defaulted (added at review, 2026-11-20).** Nothing
in 2a or 2b has a frame-paced redraw, and most of it has no *periodic* one either: `blank` and
`art` change on input, `title_card` once per boot, the title line on a track change, and the
clock at most when a displayed digit changes. The genuinely frequent redraws belong to the
interactive scenes — viz (Phase 4) and the game (Phase 6) — which are exactly the scenes that
own their own cost rows (AP-6, AP-8). Two consequences, both of which are the point of this
rule: the new model must not inherit the old one's habit of repainting on a timer, because **the
only UI latency this plan has ever measured on hardware is a single 1 Hz text redraw reaching a
DMA's grace window** (§14's R1b — 104 late of 16,553 periods), and an element-addressed shell
fixes that cheaply by touching only the digit cells whose glyph differs; and every cost row
§10.2 carries is meaningless unless it names the cadence it is priced at, because a per-tile
figure multiplied by the wrong frequency is the same class of error as the one R1b measured.

**Presentation has its own cadence, and it is §5.7's.** The draw decision is element-addressed;
while audio is playing it should be presented from VBlank (OAM DMA or a dirty screenblock), not by a
blocking main-loop `commit`. The 2a 1 Hz event commit is the measured interim exception, not the
model.

### 5.3 The control map (A2: "on the cartridge itself")

The only input surface is the ten buttons, so every feature has to be reachable
with them and nothing may depend on a keyboard, a mouse or an emulator menu.
agb's `ButtonController` gives edges (`is_just_pressed`) and tri-state directions
(`x_tri`, `y_tri`); `keys.rs` adds repeat and chord handling on top, host-twin'd so
a scripted key log is exactly the same input the test harness feeds the ROM
(§13 AP-1).

| Button | Now playing | Track list / menu | Notes |
|---|---|---|---|
| **Start** or **A** | pause ⇄ resume (faded, §4.4) | confirm / play the highlighted track | the two "do the obvious thing" buttons are deliberately equivalent |
| **Right** or **R** | next track | cursor down | fade → drop → re-prime; ≤ ~0.6 s, never a click |
| **Left** or **L** | previous track | cursor up | same machinery, wraps at the album ends |
| **Down** | **restart the current track** | page down | replaces the usual "press Left twice quickly" chord on purpose (§4.4) |
| **Up** | — | page up | |
| **Select** | cycle the **scene**: `art` → `list` → `title_card` → `blank` (the order is the variant's registry order, a declared field, and **Q20 owns it** — this row cited "new Q21", which is the full-bleed art question). **Implemented 2b PR-(3)** as a filtered view: `blank` ⇄ `title_card`, with the two 2b scenes still declared and skipped by name until they land | same, and the list scene's own cursor keys are in the row above | one button, one presentation change. **The old justification for this row — that `blank` is the position which "gives the cart's CPU back" — is deleted with the model it belonged to:** under §5.1 a scene switch is a priority write whichever scene is showing, so `blank` buys nothing a resident scene does not already give. The scenes where switching *off* would actually return CPU are the ones doing per-period work — viz (§8) and `cover+breathe` (§7) — and neither is in this cycle |
| **B** | — (reserved: it is the game's action button) | back / cancel | |
| **L + R + Start** | soft reset (the BIOS chord) | — | always available, never application state |
| **hold L at boot** | program `WAITCNT = 0x0000` (4/2, no prefetch) | — | the conservative-cartridge arm this project's records already measure against: a slow reproduction cart gets a documented fallback instead of a mystery |

Deliberately absent: anything requiring two buttons *within a time window*, and
anything that reads differently in an emulator than on hardware — **with one honest exception, stated
here because this file's gates depend on it.** The emulator cannot press a key at all
(§13 AP-1), so the gate replays a scripted key log through `keys.rs`, which is the same state machine
the hardware path drives but is *not* the hardware path: the `ButtonController` round trip and every
physical-press claim are owed to AP-7a, and the ROM logs which input source it used so no run can imply
a press it never saw.

**Re-homed here, 2026-11-19 (PR-3c), because §11's scoping put it in the wrong place.** Two requirements
came out of PR-3c and belong to this layer, not to the transport that grew around them. *(1) A transport
state being visible at all.* §11 called it "the A11 affordance"; that name is wrong, and the correction
lives here rather than in the scoping that got it wrong: A11 is the **track-list screen** (§2's affordance
table, Phase 2, AP-3 — not in this tree), and no affordance row covers a transport-state indicator at all
(A6/A7 are time and progress; the pause menu named in §11's phase table has no id of its own). The
correct home is the shell's PR — §5.2's now-playing / pause-menu surface — or a new affordance row, which
this file does not mint on PR-3c's behalf. Nothing is needed from a future screen that PR-3c does not
already provide: a named state, a count, and a bounded leg. *(2) What a press means in each state.* A
second A during a refill is refused, and so is a skip; that decision is in `transport.rs` today only
because there is no layer above it yet. It is the first input this layer should own when it is built —
context-dependence is the reason this section put the layer above `keys.rs` in the first place (#157).

**Status 2026-11-20: both are now owned by this layer by design, not by promise.** (1) is the
**chrome band** (§5.0): the control's drawn state plus a word (§5.6), on a layer no scene
can hide. (2) is `surface.rs`'s routing: one press means different things to the chrome, to
the list scene's cursor, and to the surface's own Select — and the refusal PR-3c implemented
(a second A during a refill is refused, and so is a skip) is expected to move out of
`transport.rs` into that routing when 2a builds it. The mis-naming above is left in place
because the correction is the record — and note that the surface the 2026-11-19 sentence named
(“§5.2's now-playing / pause-menu surface”) **no longer exists anywhere in this file**: §5.2 was
re-ruled on 2026-11-20 into one surface with scenes, and there is no pause menu in 2a or 2b
(§5.0, §11). So a reader looking for the thing PR-3c was handed will find this paragraph, not a
screen.

**The controls are drawn art, never font glyphs (ruled 2026-11-20).** The transport
symbols this surface needs (`prev`, `play`, `pause`, `next`) do not exist in the shipped
face — the subset is U+0020–U+007E (`assets/fonts/README.md`), and widening it is a **Q9**
decision this section must not take silently. So they are **authored pixel art**, and the
same reasoning governs every future icon (a badge, a cursor shape): an icon is not a
character.

- **The asset is the source; the container is a variant field.** One tracked source image
  (or a text-bitmap table, if that is easier to read in a diff) is the asset; the bake step
  generates a const tileset plus the slot table. §2.3's test applies cleanly: UI chrome is
  product-independent, so it is **instrument-class and tracked** — like the font subset and
  the fixture, unlike an album's art — and the generated module is pinned by R11's sha
  mechanism like every other generated asset.
- **Slots are keyed by identity, not by pixels:** `PREV PLAY PAUSE NEXT`, with headroom for
  `RESTART` and the `PAUSED` badge (§5.2). Placeholders are sentinel tiles at those indices
  and they are allowed to be ugly — **but not to be indistinguishable**: §5.6's rule means
  the play and pause placeholders must differ in *shape* (a triangle against two bars), or
  the state has to be carried by a word until the real art lands. Sentinels differing only
  in hue would fail a gate this file already has, and drawing shape-having placeholders
  costs nothing.
- **Snapshots assert geometry and slot identity, never art pixels** — otherwise 2a's
  snapshots churn the moment anyone draws anything. Art pixels get their own locked fixture
  in AP-4's existing shape ("reproducible byte-for-byte from the source, and re-reads
  through the shipping parser"), which is the shape the ART1 probe already uses: checksum
  and sha256 recorded with the blob.
- **Tiles now; OBJ when motion *or presentation latency* earns it** (`obj = true` stays a
  legal variant field). For a static control row, tiles in the chrome's BG add nothing new,
  stay inside banks 1–2 (§5.4), and keep the always-shown BG the model depends on. OBJ
  would buy overlay-anywhere, movement, per-object semi-transparency and — the new reason —
  a VBlank-presented chrome that does not block the main loop at `commit`; the presentation
  policy and the chrome-only plan are §5.7. **AP-0 proved the plumbing is there, and no
  product ROM in this tree has drawn a sprite**, so the first OBJ is still an audited step
  (AP-3c/AP-6), not a free one. The full list remains a tile scene (30×8 = 240 cells > 128
  OAM entries); the chrome is the candidate, the list is not.
- **Judged at 1×.** §5.6's added rule applies to icons as much as to faces.

### 5.4 Palette allocation (the constraint that shapes the art)

| bank | use |
|---|---|
| 0 | text: 0 backdrop, 1 glyph, 2 dim, 3 accent (extends `text.rs`'s shipped entry-0/1 convention) |
| 1–2 | progress bar + chrome (lit/dim/peak) |
| 3–6 | album art, up to 4×15 colours (or 1 palette + a 256-colour mode-2 tile bank if we want 8-bit art — §7) |
| 7–10 | visualisation |
| 11–15 | reserved for the game (§12.2) |

Fade-in/out and the dimming behind a **paused state or a scene change** are done by
rewriting palette entries (`set_background_palette_colour`), not by touching tiles — cheap,
and it gives those transitions a 60 Hz cost of tens of cycles. ("Behind a modal" was the
old wording; there are no modals in 2a or 2b — §5.2.)

**Banks are declared, not hand-allocated.** The variant carries a palette bank per element
(§5.0), because which tiles need colours is exactly as much a layout fact as which cells
they occupy — and those numbers are what §13 AP-4's audit checks the ROM against.

**The mechanical fact that makes text-over-art cheap, which this file never stated:** a BG
map entry selects its 16-colour bank **per tile** (bits 12–14, restated in §7's
hardware-conventions bullet). Band tiles can therefore be opaque in bank 0 while art tiles
behind them use banks 3–6 **on the same layer**, with no blending — which matters, because
§16 rules out alpha blending beyond palette tricks, and a dithered band edge *is* a palette
trick (§7's technique). So text over a picture is a composition problem — §5.6's luminance
rule, and the opaque-band rule below — and not a hardware one. The hardware costs of a large
picture are the residency and time costs in §5.1 and §10.2, and nothing else.

### 5.5 Text rules: one line, a derived fit box, and a face per element

Titles/artists come from tags and are **ASCII-folded at bake time** (Unicode NFKD,
drop what is not ASCII, collapse whitespace, strip a whole title wrapped in
parentheses), because the shipped font is a 96-glyph subset and `include_font!`
rasterises every glyph in the file (that is
why the subset exists — `assets/fonts/README.md`). Extending the subset is a
cheap, explicit decision (**Q9**), not something a title with an em-dash should
force silently. **And that 96-glyph subset is a development instrument, not a product face**: it
is the face the witnessed verdict screens use today (AP-7a), it is expected to be replaced by an
authored cut (Q19), and so no tile budget, cache size or fit box anywhere in this plan may be
stated as "96" — the number comes from whatever face the variant declares (§5.1's BG2 row).

**Ruled at the second review (2026-11-20), because "instrument" was doing a lot of work in that
sentence without a consequence attached: the 8 px cut is too small to ship.** What follows from it,
in the order that can actually be enforced:
- **A product variant may not declare it.** §5.0's `product = true` variant must declare a face whose
  ink survives the GBA screen at 1× — which is §5.6's judgement, made on `album-preview`'s 1×
  composite with a real title in it, not at 4× and not from a glyph count. The 8 px cut stays
  declared only by a variant with `product = false` (the development/verdict instrument AP-7a's
  screens are witnessed in), and AP-3 prints which variants are product-eligible and which are not.
- **Keeping the asset is not the same as shipping it, and shipping only what is used is a rule, not a
  hope.** The `.ttf` stays tracked: it is genuinely useful (narrowest advances of anything in the
  tree, smallest ROM footprint, the face every witnessed screen so far is drawn in). **What the ROM
  carries is one declared face compiled into one atlas**, and the path to that face is owned by
  generated `layout.rs`. agb's `include_font!` is not eliminated by that: it is what draws the
  *transient* text — the boot fit/verdict line, and the instrument ROMs — and those dynamic tiles are
  released with the renderer that allocated them, so they are a spike at boot rather than a standing
  term. The standing glyph term is the atlas's 188 tiles, and there is one face in the image, not
  two. An unused face reaching the ROM is a defect, not a rounding error: **a linked-but-unused face
  costs ROM bytes (§10.1) and, if anything ever draws with it, a second tileset competing for the
  same 512-slot pool (R14).** AP-3's map-file leg flags it (§13), because nothing else in the tree
  would notice.
- **A new face is allowed, and sought.** Nothing here says the first product-eligible face is the
  final one; the measured facts below say exactly what a better face would have to be (a shorter line
  box than its ink wants, at a raster that lands on the tile grid), and a genuinely better asset —
  e.g. a 12 px design face — is a tracked subset plus its licence rows (§5.5's own convention,
  `assets/fonts/README.md`), which is a cheap change to the plan and an expensive change to nothing.
  **What is not allowed is an unmeasured face: a variant may not declare a size it has not been
  checked against the grid, because then the fit boxes and the pool term are guesses again.**

- **How a face gets into a ROM, since Q19 cannot be asked without it (measured 2026-11-20).**
  There is no face asset in this tree and no tool that makes one: `include_font!` compiles the
glyphs out of the TTF **at build time** (`text.rs:91` points at the TTF path directly), the
Makefile has no rule mentioning a face, a `.gfp`, or `assets/fonts/`, and `assets/fonts/` holds
no generator script — the plan's Phase 1 row's phrase "the font include" is the whole mechanism.
The numbers that fall out of the tree, so Q19 is argued with facts: the tracked subset is **96
glyphs / 95 mapped codepoints / 6,920 B** (U+0020–U+007E; `hhea` ascent 900, descent −300, upem
800, i.e. **a 12 px line box at an 8 px em**, which is what §5.0's `row_px` floor is read from);
the only `8×16` in this tree is
`examples/flac_integration/src/product_play_main.rs:205`, an **8-column × 16-row text window**, not a
cell size — the earlier version of this line attributed that phrase to `text.rs`, which never says
it (citation slip corrected at the second review); **the parent
latin face is not in the tree at all**, so any figure for "the same font at full Latin coverage"
is not checkable here, and this file should not carry one; the second OFL face already in the tree,
`ark-pixel-10px-proportional-latin.ttf`, is **2,881 glyphs / 349 KB**, which is exactly why the
`pyftsubset` convention in `assets/fonts/README.md` exists and why a second face means a second
*cut* before it means a second anything else.

- **What the tracked faces will and will not rasterise cleanly, and what they cost per row
  (measured twice on this branch, 2026-11-20; the second pass redid the first pass's numbers with
  the method stated in one place, and the table with line boxes, advances and off-grid counts is
  **§5.1's**, which is where the pool arithmetic needs it — this section is the *decision* half of
  those numbers, not a second copy of them).** Both files are `glyf` outlines with no embedded bitmap
  strike, so agb rasterises them at whatever size it is asked for, and the only rasters where every
  outline coordinate lands on the pixel grid are the design size and an exact 2× of it:
  `fusion-pixel-8px` at em 8 or em 16, `ark-pixel-10px` at em 10 or em 20. **At every other scale
  the count is 94 of 94 — not "some glyphs are soft", but nothing in the cut is on the grid** (0.8×,
  1.2×, 1.5×, 1.6×, 0.5×: all of them), and an off-grid glyph still has an advance width, so the fit
  boxes hide it and the 1× composite does not. **So a declared raster is a design size or an exact
  2× of one, and that is a bake refusal (`pitch-not-tileable`, §9.0), not advice.** The line box
  comes with the raster — 12 px at em 8, 16 px at em 10, 24 px at em 16, 32 px at em 20 — and
  §5.0's `row_px` must be at least that, rounded up to a whole 8 px tile row.
- **The first product-eligible face, for the variant that needs one now: an ASCII cut of the
  `ark-pixel-10px` face already tracked here** — grid-exact at its design size, a 16 px line box
  which is the smallest pitch §5.0's rule allows, **9.0 px of tallest measured ink inside that box
  (against 8.0 px inside 12 px for the current instrument)** — i.e. genuinely larger on screen,
  already OFL, already in `assets/fonts/`, and needing only the same `pyftsubset` cut the 8 px face's
  README documents plus its licence rows. It is the candidate because it is *checkable from this tree
  today*, not because it is the best face available.
- **And the size question has a smaller answer set than "16 or 20 px" suggests, which the second
  measurement pass caught.** `row_px` must be a whole number of 8 px tile rows (§5.0), so **a 20 px
  line height is not expressible on this screen: the candidates are 16 px and 24 px, and above that
  32 px.** At 24 px a 128 px region holds five text rows instead of eight and the pool term spends
  itself (§5.1's interaction table) — and neither tracked file reaches a grid-exact 24 px line box
  anyway (`fusion-pixel` @16 gets there exactly and is unusable for a list at 26 characters a line;
  `ark-pixel` @12/@16 would, but they are off-grid and refused). **So the visual review's real
  choice is 16 px, or 16 px with a better asset.** `album-preview`'s 1× composite is still where the
  judgement is made — what changes is that it is judging whether *this* 16 px cut is legible enough
  to ship, and if the answer is no, the next step is a different face file, not a bigger number.
  A face designed at ~12 px with a line box that still fits 16 px would buy the margin back and is
  worth looking for; it is not required for 2a, which only needs a declared face it can measure.
- And the cost shape is not the one §5.1's layer table
implies: a face's inventory costs **build time and ROM bytes**, never a standing VRAM term,
because agb's background text path hands every drawn tile cell a `DynamicTile16` through
`set_tile_dynamic16` (`font/tiled.rs:126-140`) — so on that path the standing term is what a frame
displays (§5.1's BG2 row, R14) and **nothing dedupes at all**: each dynamic tile gets its own slot
and reports that slot's index inside the shared VRAM tileset (`vram_manager.rs:287-302`), so the same
glyph drawn twice is two keys and two tiles. This sentence said "identical content dedupes" until
2026-11-20 — true of a shared `TileSet`, false of `DynamicTile16`. Dedupe
is what the **static glyph atlas** buys — the cut rasterised once, at its declared raster, into one
`TileSet` and addressed by index with `set_tile` (94 glyphs × 2 tile rows = 188 tiles ≈ 6 KB of ROM
for the 16 px cut, allocated once and refcounted per map cell) — and it is the same path AP-0's
per-tile figure was measured on (`set_tile`, `ui_probe.rs:198`, `art_probe.rs:124`), which is why
§10.2's costs describe the atlas build rather than the dynamic one. **The atlas is 2a's** (it reads
the tracked `.ttf` and nothing else, so it adds no tool from a later phase, §9.0/§11), and it is
what makes an eight-row list fit at 16 px at all: without it the per-cell path caps the list at
five rows with no art, by arithmetic rather than by measurement.

**That is why a second face is affordable to explore and dangerous to ship**: exploring it is one
`pyftsubset` invocation plus a build (no hardware, no ROM burn-in), while shipping two means two
tilesets competing for the same 512-slot regular pool, and §14 R14 is the panic that finds it out

Two consequences of that choice, both worth writing down before the code exists, because both are
the kind of thing that is discovered late:
- **Rows are placed, not stacked.** agb's `Layout` advances each line by `font.line_height`
  (`display/font/layout.rs:453`), which equals the declared `row_px` only for a face whose line box
  *is* the pitch. So the atlas path does not use `Layout` for a list at all: row *i* is written at
  `y = i × row_px / 8` tile rows, which is the only way §5.0's pitch rule is expressible when the
  face's line box is shorter than the pitch. (For the 16 px cut the two coincide — that coincidence
  is a convenience, not the rule.)
- **One rasteriser, or the preview and the ROM disagree.** The atlas bytes are what the product
  ROM draws *and* what `album-preview`'s 1× composite draws, so §12.1's one-implementation rule
  covers text as well as geometry: a second rasteriser that reproduces "roughly" the same glyph
  would put the legibility judgement (§5.6) on different pixels from the ones a listener sees. The
  host tool is the single rasteriser, it runs from the same tracked `.ttf` at the same declared
  raster, and the atlas it emits is build output like `layout.rs` — regenerated by the bake, never
  hand-edited, never tracked.

**One line, elided — not wrapped (ruled 2026-11-20; the rule exists in code as of 2a PR-(2),
2026-11-22).** The title band is one row (§5.0), so
the rule is a truncation rule: measure, and if it does not fit, cut at the last whole glyph
that fits plus an ellipsis; **the track number is never cut**, because `NN ·` is the part a
listener uses to ask for the song. `text.rs`'s own docstring still defers the whole of this,
verbatim: *“There is no `measure`, no wrapping, no ellipsis, no palette bank beyond 0/1 and no
dynamic tiles; ALBUM-PLAYER.md §4.1 names that layout engine as Phase 2's A5, so growing it here
would pre-empt it”* — and the test-side comment at `text.rs:119` says the same thing shorter:
*“a verdict is one row, so there is nothing to wrap and nothing to measure.”* Both of those
comments are still true **of `text.rs`**, which draws no text from the atlas until PR-(3); what
has changed is that they are no longer true of the tree. PR-(2) made the rule twice, in the two
places a copy of it has to live, and pinned the two against each other:

- **the bake's copy** — `scripts/album_bake.py`'s `FaceMetrics.elide`, reached through
  `LayoutFacts.title_render`, which is what decides whether an over-long title becomes a printed
  note (it does, always: §5.5 has never made an album's wording a build failure) or a refusal
  (only when the box is narrower than the marker plus the protected prefix — §9.0's `title-fit`
  row, which is now a refusal that fires and is fired by name in the layout self-test). The
  protected-prefix argument (`keep`) is "the track number is never cut" as a number; 2a's title
  band passes 0, because `meta.rs` stores the folded name with no number on it.
- **the runtime copy** — generated into `src/glyphs.rs` as `ELLIPSIS`, `elide` and `fit`
  (§5.5a's module is the only place the atlas's advance table can be both the measurement and the
  pixels; §5.5a step 5's property is why the rule lives there and not in `text.rs`). `fit` is the
  entry point a draw path calls, and it returns the three things §5.5 refuses to let a caller
  merge: what to draw, whether the marker follows it, and how many characters the cut has no glyph
  for.

The two copies are not allowed to drift: the generated module ships a table of cases whose expected
answers the *Python* path derived, and its own test suite re-runs them through the Rust `elide`
(§5.5a's gate runs that suite with a floor on the count). The marker is three ASCII periods for
the reason §5.5a states in tiles, and — a fact about this face rather than an argument — a single
U+2026 is not in the declared title face's cmap at all, which the layout gate prints. The marker's
*width* is never written down: it is the measured sum of the three periods' advances, so a face
change moves it — §5.5a records what is parameterized and what is not, and the raster probes that
say which faces this model can and cannot pack.

**Non-title text is legged by the same mechanism, and the honest split is between boxes and
strings.** The *boxes* of every declared text element are walked in both languages: the generator
filters `[[variant.element]] kind = "text"` into `TEXT_ELEMENT_SLOTS`, and both `pitch_ok()` (a
`const` leg, so the ROM build checks it too) and the module's
`no_declared_text_box_is_narrower_than_the_marker` test iterate that list against
`crate::layout::ELEMENTS`; the layout self-test's pitch and box legs do the same from the spec
side. The generated case table reaches three of those boxes by
design — the title element's (160 px in the product layout, the one §9.0's `title-fit` is a refusal
about), the widest text box in the variant (the list window, 240 px, which is 2b's box and 2a's
rule), and the narrowest (the key-label column, 72 px, where an unmapped code point and an
over-long line meet and a wide box would hide the difference) — plus the box no variant declares
that is narrower than the marker. A generator leg in the atlas self-test refuses a table that skips
the title element's box, because "measured at the widest box" is how a fit rule passes without ever
meeting the row it governs. What is *not* legged is the string a surface will draw in a clock or a
key label, for the plain reason that no such string is composed until `surface.rs` exists: those
strings and their fit legs arrive with the PR that composes them (3 for chrome, 2b for the list
rows), and until then the mechanism is legged at their boxes with inputs labelled synthetic — the
same rule §13's D5 gives for not putting invented metadata in a tracked album.

That is also the whole of what `text.rs`'s sentence lists as missing: measure,
wrapping-then-eliding (one line, never wrapped, at 2a), and the palette banks beyond 0/1 that
§5.4 declares per element — the last of which is still PR-(3)'s, because a palette bank is a
drawing decision and PR-(3) is the PR that draws.

**The fit box is derived, and it lives in one place, not three (ruled 2026-11-20; the two
copies in code removed by 2a PR-1).** The number used to sit in three places with no sync
between them: this section's old prose ("a 28-character worst case"), a Rust host-test guard
(`assert!(title.chars().count() <= 28, …)`, then at `album_pack_host.rs:241`), and
`TITLE_BOX_MAX = 28` in `scripts/album_prep.py:48`. That duplication is recorded as §11's
**D9**, with each copy's fate written there. This file removed its own copy at the ruling;
2a PR-1 removed the two in code, so every text element's fit box now comes from the variant's
declared *span* (a pixel width — §5.0) and the face's real metrics, and the bake tool still
prints any title that will not fit its box, so an album can still be *fixed at bake time*. The
8-row/30-cell framing this section used is also dropped **for fit boxes**: what a text element
may hold is advance width in pixels, and the honest number is the one the tool prints. The one
character bound still standing in product code is `text.rs:65`'s `COLUMNS = 30` on the verdict
line — 30 columns of the instrument face's 8 px cell is the whole 240 px row, so that is a
measured cell count rather than a box standing in for one. Converting it to the same measured fit
is **PR-(3)'s, not PR-(2)'s**, and the reason is a missing field rather than a missing mechanism:
the mechanism is in the tree now (`layout::Face::measure` at any declared face, `glyphs::fit` at
the product one), but the verdict row is not a declared element in `layout_spec.toml`, so there is
no box to measure it against until `surface.rs` declares one (it is witnessed at
`album_seam_host.rs:811-831`, which is why the retired-name grep does not pretend to cover it).

**Which face draws which element is a declared field — and the 8 px face is an instrument,
not a product face (opened, not decided: new Q19).** The instinct that fusion-pixel at 8 px
is right for iterating and wrong for a finished product is worth taking seriously, and the
honest way to hold it is to make the face a per-element variant field, so the question is
answerable by looking at a preview rather than by re-litigating a plan. The budget framing,
in cycles rather than taste:

| element | how often it changes | face the arithmetic permits | the arithmetic (**all of it is the disarmed leg — see the note under this table**) |
|---|---|---|---|
| bar cell | only when it changes *cell* — at most **once per declared cell** (a field of the variant, §6; not "1/32 of a track" as this row said until 2026-11-20), and no more often than that | no glyphs | 1–2 tile writes per change ≈ **0.7–1.5 k c** at AP-0's measured 726 c/tile |
| clock `1:47 / 3:12` | **when a displayed digit changes** — at most once a second, and then only the cells whose glyph differs. "Once a second" is R1b's measured habit, not a cadence this design wants (§5.2's rule) | **the smallest declared face**, and this is the row where a pitch costs real cycles | **re-cut at the 16 px pitch (2026-11-20).** A full redraw of the 11-character clock at a 16 px pitch is 11 characters × **two tile rows** each ≈ 12–22 tile writes ≈ **9–16 k c** (at the instrument face's 1 tile row, half that: ~6–10 tiles ≈ 4–7 k c). But the periodic event is not a redraw, it is **one or two changed digit cells ≈ 2–4 tile writes ≈ 1.5–3 k c per second** — and that is the only number that is a permanent addend. R1b's measured failure was precisely a **full-row** 1 Hz text redraw: 104 of 16,553 periods late, which is why "redraw only the cells that differ" is load-bearing at a large pitch and was survivable at a small one |
| title line | on a track change | **a large pitch is affordable here** | **the declared box, in cells: variant B's `title_text` is 20 tile columns × two tile rows = at most 40 tile writes ≈ 29 k c disarmed / 44–50 k c live** at the 16 px pitch. This row was priced at "28 characters (the box's limit, §5.5)" until 2a PR-1 removed that limit — there is no character box to price against any more, and 160 px holds ~29 characters of ark10 only by the accident of its 5.44 px mean advance (the fixture's longest title measures 103 px of the 160 px). The cell count is the cost; the character count never was. Paid rarely, with §12.2's ring lead paying for it |
| title card (A12) | once per boot | **the largest pitch this app may use** | paid once, and it is the screen where a face is actually being read; at a 16 px pitch its three text spans are on the order of 10⁵ c of writes, which a boot path with seconds of headroom absorbs (§10.5) |

**Re-cut at the second review (2026-11-20), because every text row above had been priced at the
instrument face's one-tile-row cell: at the 16 px pitch §5.0/§5.5 make the product floor, every
glyph cell spans two tile rows, so every text cost in this table doubles.** That is the honest form
of the pitch decision — it is a cost column, not a taste question — and it is also why the
changed-cells rule (§5.1's redraw rule) is what makes a large pitch affordable at all: the clock's
real periodic bill is 1–2 characters a second, not 11.
Nothing in this table is frame-paced, and the two rows that are periodic are periodic at
*seconds*, not frames — which is the point of the "how often" column being the second column
rather than a footnote. No scene in 2a or 2b needs a 1 Hz redraw: the clock's worst case is one
digit cell per second and the bar's is at most one cell per declared cell (§6), which is a variant field, and both are cheaper than
the row-level redraw R1b measured on hardware. The scenes that *do* want frequent redraws are
the interactive ones — viz (§8), the game (§11 Phase 6) — and neither is a text row.

The point is the **ordering**, which is not a taste question: a face's cost is paid per
changed cell, so a larger pitch is cheap on content that changes rarely and expensive on
content that changes every second. A player that sets its clock in 16 px is paying its most
frequent UI bill on its least informative text. All four rows are **upper bounds from
AP-0's single-tile write figure**, and AP-3 re-prices them per element (§10.2) — what is not
in doubt is their ratio. **What is in doubt is the absolute number, and this table should be read
as an ordering only until it is measured:** the 726 c/tile it is built on is AP-0's
cold-cache, audio-*disarmed*, IME-off leg, while the same probe's live-audio per-tile floor is
1,096 c with a 1,242 c mean (`examples/album_probe/README.md:169-176, 202-207`) — so anything
spent during playback is 1.5–2× what these rows say, and the compute bandwidth actually left for
rendering on each backend is unmeasured by anyone (§10.2's caveat).

Two mechanical consequences, both for 2a rather than asserted here: mixing faces means **two
atlases — 188 tiles each at the 16 px cut, so two faces cost 376 of the 512-slot pool before a single
art tile**, which is why the per-element face field is a budget fact (§5.1, R14) and an AP-3 record
rather than a taste note; and **any second face cut must itself be a subset** —
the `pyftsubset` command is already in `assets/fonts/README.md`, the family is OFL, and
per §2.1 it lands tracked in-tree with its licence files, not referenced from upstream.

**A new rule for every band drawn over a scene: it carries its own backing.** Opaque tiles
in its own bank (§5.4), or a declared dither. This is a legibility rule, not a style rule:
§5.6's measured pitfall — pure blue on black at 0.072 against 0.000 relative luminance — is
what happens when a band inherits whatever the picture happened to put under it. The bake
report should say what it computed for each band, in the file's usual style of printing the
number instead of asserting the taste.

### 5.5a The glyph atlas, as a build — the baseline 2a implements

The atlas was described above by what it must *buy* — dedupe, 188 tiles, one rasteriser — and §11's
2a row and AP-3b's L4 leg are the two places that depend on it. This is the same thing by what it
must be *made of*, because "rasterise the face into a `TileSet`" is where a phase quietly acquires an
unbounded tool if nobody bounds it. The rule that bounds it is §9.0's:
2a may refuse only facts about input its own tools can read, and the atlas reads **one tracked
`.ttf`** and nothing else. So the baseline is deliberately the smallest thing that produces correct
text, and everything not in this list is out of 2a rather than unfinished.

**Scope of the baseline, stated as cuts** (each one is a decision with a reason, not a TODO):

- **One face, one raster.** `ark-pixel-10px`'s ASCII cut at its design size (10), in a 16 px cell.
  Per-element faces stay a declared field that resolves to the *same* atlas in the baseline — a
  second face is a second atlas (§5.1: 376 of 512 tiles), so it is Q19's work, not 2a's.
- **4bpp, one palette bank, ink = index 1, background = index 0 (transparent).** No dither, no
  8bpp, no per-glyph colour. §5.4's banks are already allocated; the atlas just picks one.
- **No kerning, no hints, no antialiasing, no sub-pixel positions.** Advances are integers at the
  design raster (the grid test in §5.1's measured pass is 0 of 94 off-grid there), so integer
  placement is exact and `measure()` is bit-identical to what the ROM draws. A fit box that refuses
  on `measure()` therefore agrees with the pixels *by construction* — which is the property
  `title-fit` and one-line elision actually need.
- **Coverage is `cmap ∩ U+0020–U+007E`.** Space costs nothing: it is `TileSetting::BLANK`, i.e. the
  `0xffff` sentinel (`display/tiled.rs:117,168-169`) that `set_tile_at_pos` skips allocating for
  (`regular_background.rs:364`) — so the pool sum in §5.1 assumes nothing it has to take on faith. A
  character outside coverage is the same failure it is today — `text.rs:74`'s `renderable()` — not a
  new refusal.
- **Elision is three ASCII periods, not U+2026.** A one-cell ellipsis would be a declared extra
  slot: +1 glyph × 2 tile rows = **190 tiles and a distinct cap of 190**, moving §5.1's 444/488 to
  446/490. The baseline keeps `...` so every number in §5.1's tables stays exactly as printed; a
  real ellipsis tile is a one-line addition later, and the arithmetic it costs is written down here
  rather than discovered in a review.

**The build, in the order it is written** — one script, one generated module, one Make step, one
check. Names are the ones §9.0's register and §5.0's emitter already use.

1. **The cut is an asset step, not code.** `pyftsubset` with the exact command already in
   `assets/fonts/README.md`, output committed under `assets/fonts/` beside the two faces already
   there, with its licence rows (§2.1). This is the step that makes the product variant's declared
   face *resolvable* at all — today `assets/fonts/` holds the 349 KB full face and no ASCII cut of
   it — so it comes first, and `face-not-resolvable` is what fires if it is skipped. **Landed
   (2a PR-2): the cut is 95 mapped codepoints, U+0020–U+007E, 94 with outlines** — `pyftsubset`
   maps the `.notdef` glyph onto U+0020 rather than dropping it, which is the whole of the
   difference between "94 glyphs" as a count of drawings and 95 as a count of the range. The
   generated module does not restate either number; they are derived, printed, and asserted (§5.5a
   step 5's rule, applied to this file's own prose).
2. **`scripts/album_glyphs.py`** reads that cut and emits `examples/album_player/src/glyphs.rs`
   (generated, gitignored, regenerated — the `--emit-pin`/`master_pin.rs` convention, `Makefile:180`).
   Inputs by table, so there are no hidden ones: `cmap` (coverage), `hmtx` (advances), `hhea`
   (line box, baseline). It walks U+0020–U+007E, rasterises each glyph with an **integer even-odd
   scanline fill** over the `glyf` contours at the declared raster, and packs each into **two 8×8
   4bpp tiles** (32 B each) — one tile row per 8 px of a 16 px cell.
   *Why a hand-rolled scanline is the right answer and not a shortcut:* at the design size every
   contour coordinate is already an integer (that 0-of-94 grid result), so the fill is exact
   arithmetic over rectangles and steps — **no freetype, no antialiasing, no second rasteriser**, and
   the pixels are reproducible byte for byte from the `.ttf` by anyone who runs the script.
   *What landed, so the sentence above is checkable rather than intended:* the fill samples pixel
   **centres** and compares edges in doubled (half-pixel) integer coordinates, so no float appears
   anywhere and a vertex shared by two edges is counted once — the skip is `(y0 > sy) == (y1 > sy)`,.
   which is also what makes a tangent contribute nothing. `glyf`'s quadratics are 3-point with an
   implied on-curve point between the two off-curve ones, which `glyf`'s own `getCoordinates` flattens
   into the polygon, so composites need no special case either. The generator **refuses** rather than
   assuming three things it is entitled to check: that a contour coordinate lands on a whole pixel at
   the declared raster, that a glyph's outline stays inside the one cell the atlas gives it, and that
   the pixels it filled have the bounding box the outline's control points describe.
3. **What the module contains**, and nothing more: `GLYPHS` (the packed tile data, emitted as a
   `static [u32; N]` for the alignment reason in the next item), `SLOT` (`[GlyphSlot; 95]` indexed
   `code - FIRST_GLYPH`: `tile0`, `advance_px`, `lead_px`), `BASELINE_PX`, `LINE_BOX_PX`,
   `ATLAS_TILES`, `measure(&str) -> u32` (sum of `advance_px`, no kerning), `unwritten(&str) ->
   usize` (so "no glyph" and "does not fit" stay two answers, §5.5), `slot(char)` and
   `tile_for(char, row) -> Option<u16>` — **row-indexed, because a glyph is not a tile** (§5.5's
   first paragraph; `surface.rs` writes `tile0` and `tile0 + 1`, which is step 6 below as a
   signature) — and a `vram::tile_set()` that builds the one `TileSet` inside the generated file,
   where both of the things its `unsafe` depends on are visible, rather than at each call site.
   **And, because the advance table and the painted bytes come from one walk of one file, §5.5's
   one-line elision is emitted here and nowhere else**: `ELLIPSIS` (three ASCII periods, for the
   cost reason stated above), `elide(&str, box_px, keep) -> Elision` — where `keep` is what "the
   track number is never cut" looks like as a parameter — and `fit(&str, box_px, keep) -> Fit`, the
   entry point a draw path calls, returning what to draw, whether the marker follows it, and how
   many characters this cut has no glyph for: **three numbers, not one verdict** (§5.5's rule that
   "no glyph" and "does not fit" stay two answers, enforced in the type rather than in prose). A
   separate `text` or `layout` module would put a second measurement path between the table and the
   pixels. `ELISION_CASES` is the cross-check that keeps the two copies of the rule one rule: rows
   whose expected prefix and marker flag were derived by the **Python** path (`album_bake.FaceMetrics
   .elide`, the same call the bake makes behind `title_render`), pinned into the artifact so the
   module's own test runs its Rust `elide` against them. Each row names the declared element its box
   came from — the title element's, the widest text box in the variant, the narrowest — and two rows
   sit at a box narrower than the marker, labelled as not a real declared box, because that is the
   case the bake refuses as `title-fit` and it would otherwise be unrepresentable in the table.
   Codepoints outside the cut appear in the emitted strings as visible `\u{266B}` escapes, never as
   invisible bytes in a generated diff.
   Plus `const _: () = assert!(…)` for the facts the rest of the tree computes with: `ATLAS_TILES ==
   glyphs_with_outlines × TILE_ROWS_PER_LINE`; `GLYPHS.len() == ATLAS_TILES × 8` words; and the
   same `FACE_ID`, raster, line box, tile-rows-per-line, outlined-glyph count and `ATLAS_TILES` read
   out of **`layout::FACES[0]`** — the other generated module, from the same spec entry in the same
   pass — plus every `[[variant.element]] kind = "text"` element's `row_px ≥ LINE_BOX_PX`. That last
   set is why the atlas and the layout module are asserted *against each other* rather than each
   being trusted: a stale `glyphs.rs` on a disk is a file that compiles and lies about the face.
4. **Two mechanical gotchas, written down because both are silent when wrong.** `TileSet::new` is
   **unsafe** and requires a 32-bit-aligned slice whose length is a multiple of the tile size
   (`vram_manager.rs:59-66`), so `GLYPHS` is emitted as `[u32; N]` and the `TileSet` is built from
   `core::slice::from_raw_parts(GLYPHS.as_ptr().cast::<u8>(), N * 4)` — aligned and sized by
   construction, without a runtime copy. (The `N * 4` has to be a named const, `GLYPHS_WORDS`, and
   not `GLYPHS.len() * 4`: `GLYPHS` must be a `static` for its address to be `'static`, and a
   static's length is not usable in a constant expression — so the length agreement is one of the
   asserts above. The byte order inside each word is little-endian, which is the GBA's, and the
   nibble order is low-pixel-left, the same convention the one hand-written 4bpp tile in this repo
   states at `examples/album_probe/src/bin/ui_probe.rs:93`.) And a background map names a `u16` tile
   id with 10 usable bits, so 188 is nowhere near the 1,023-tile addressability limit but *is* inside
   it — the atlas is one allocation, not a per-glyph cache. **One consequence of that last sentence,
   found while writing the generator rather than while reading agb:** a BG map's tile index is 10 bits,
   and a `DynamicTile16` carries a *tileset* reference beside it (`tiled.rs:181`), so a background
   already has two ways to name a drawing and only one of them is capped at 1,023. `set_tile` keeps
   the tileset (`regular_background.rs:373`), which means the 1,023 figure is **per `TileSet`** and the
   atlas's 188 tile ids are nowhere near it — and the cap that the atlas is actually near is the
   pool's 512 slots. §5.1's "addressable but not resident" wording stands; it was the *number* it
   was quoted against that moved.
5. **The step is `make album-bake`, and the check is `make album-glyphs-test`.** The atlas is
   generated where `layout.rs` already is — one bake command, and `scripts/album_glyphs.py` imports
   `scripts/album_bake.py`'s `layout_for` and `FaceMetrics` rather than reading `layout_spec.toml`
   or a face file itself, which is what keeps "one emitter" true in the sense that mattered: there is
   one reader of face metrics in this tree, and a second emitter that re-derived the line box would
   be a second answer to it. `album-bake` calls it after `layout.rs` exists, so the two generated
   modules can be asserted against each other. `album_glyphs.py --check` (the
   `gen_gafp_blob.py --check` convention, `Makefile:489`) proves the untracked generated module is
   byte-identical to a fresh run. `make album-glyphs-test` is its self-test and that check, and it
   exists because §9.1's Make table has a target per *gate*, not per generated file — the step that
   writes the atlas stays inside `album-bake` (§5.0's step 5).
6. **Runtime use is two writes per changed glyph cell**, at `y = i × row_px / 8` (§5.5's
   rows-are-placed rule): `set_tile((x, row), &ATLAS, TileSetting::new(SLOT[c].tile0, BANK))` and
   the same for `tile0 + 1`. A clock digit change is 2 writes ≈ 1.5–3 k c (§10.2), a title line is
   `28 × 2 = 56` writes, and a scrolled list row is `44 × 2 = 88` — which is the arithmetic §5.1's
   bounds were always describing.

**Acceptance, and what is deliberately not measured yet.** `album_glyphs.py --check` is
byte-identity; `make album-glyphs-test` asserts every `SLOT.advance_px` equals the TTF's advance at
the declared raster (against `album_bake.py`'s `hmtx` path, a *different* path over the same file)
and every glyph's filled-ink bounding box equals its own outline's control bounds — twice derived,
by this tool's walk of the contours and by the face's `ControlBoundsPen` — plus that even-odd and
non-zero agree on a ring, which is the one way to test the fill rule without a second rasteriser.
It also checks the **format of the bytes inside the artifact**, not in the tool that wrote them:
every pixel of `GLYPHS` is at one of two levels (transparent, or palette entry 1 — the entry the
text colour in `ui_probe.rs` uses), and the 4bpp bit order is asserted by reassembling one glyph
from a bitmap transcription of itself that the generator writes into the module: pixel `x` of a row
is the 4-bit index at bit `4x`, low nibble of the byte first, which is the order
`ui_probe.rs:93` states for the tiles it writes by hand. Two paths over one grid — the words come
from `_pack`, the reassembly is written in the module — and the fixture glyph is the most
x-asymmetric one in the cut (`,`, 6 px off the centre of its cell in the tracked cut), with a test
asserting that its *mirror* does not match, so the leg cannot be a symmetric glyph agreeing with
itself. The module then carries its own host tests for its API — one cell per glyph with nothing
shared, agb's blank tile used exactly once and by space alone, no slot outside the cut, and
`measure` strictly below `tilemap_px` for a proportional face — and the target runs them filtered to
`glyphs::` behind a floor on the count, for the reason every floor here exists: a filter that
matched nothing is green.
That is a *geometry* validator, so it can never quietly become the second rasteriser §5.5 forbids;
it does not compare any pixels with any renderer, and the ROM's own leg is §5.6's composite and
AP-7a. AP-3's snapshot leg records slot identity per variant; and AP-3b's L4 leg reads the live
tile **allocation** on a repeated glyph's tile — it must be ≥ 2, which is the one number that proves
dedupe rather than assuming it. **Reading it takes one line of agb vendored, and that is stated in
step 3 above**, because `TileReferenceManager`'s `counts` field is private (in agb 0.25.0 as this
tree builds, `vram_manager.rs:398`) and there is no accessor for it: the leg will be L4's
measurement, not L4's measurement *plus* the one field that would make it self-checking. **Not in the baseline:**
per-element faces, 8bpp, a one-cell ellipsis, wrapping, kerning, sub-pixel placement, OBJ text, and any
use of agb's `Layout` for a list. Each is a declared field or a later phase, and none of them is on
2a's critical path. **The one qualified exception is §5.7's chrome-only OBJ candidate**: it keeps
this cell model and the list's BG path, and if AP-3c lands it the exclusion above is amended to
"the chrome may be OBJ; the list stays cell-addressed", not to a second line-layout rule.

**How far this generalizes, and the one premise that is a premise (measured 2026-11-22).** Written
down because the obvious next question is "what if we change the font?", and the answer is split
cleanly in two — the *rule* is parameterized, the *drawing model* is not — with one silent gap
between them. Everything below is from probes run against the two tracked faces at several rasters,
not from reading the code.

**Parameterized, and it regenerates: every number the elision and the fit depend on.** The rule takes
`(text, box_px, keep)` and nothing else (`glyphs::elide`, `glyphs::fit`; `FaceMetrics.elide`), so the
box is a caller's number and `keep` is a code-point count. What fills those in comes out of the file
the spec names, on every emit: `unitsPerEm` and `hhea` → `line_box_px`, `tile_rows_per_line`,
`min_row_px` (`album_bake.py:1494-1601`); `hmtx` → each `advance_px` and therefore `measure`, plus
`cells_per_row = floor(w / avg_advance)` (`album_bake.py:2132`); the marker's width as the sum of the
three periods' advances (`album_glyphs.py:985` — nothing states 12 px, it is measured, and it would
become 9.6 px or 24 px without a code change); every case-table box from the declared elements' `w`
in `layout_spec.toml`; and the degenerate rows as `marker_px - 1`. Change the face or the raster and
the marker, the cut points, the case table, the pool terms and the boot line all move together.

**Three constraints, checked, each with its own refusal.** The atlas needs, at the declared raster:
(1) a line box that is a whole number of 8 px tile rows — `pitch-not-tileable`, because a glyph cell
is the line box and there is no such thing as a partial tile row; (2) every contour coordinate and
every `hmtx` advance on a whole pixel — `atlas-not-on-pixel-grid`, which is §5.5a step 3's "at the
design size every contour coordinate is already an integer" as a check rather than an assumption;
(3) every glyph's ink inside **one 8 px column** and the line box — `atlas-glyph-outside-cell`
(`album_glyphs.py:422`). The measured chain for the tracked 10 px face, which is the demonstration
that the three are independent and fire in order:

| the same `.ttf` at | line box | advances on grid | verdict |
|---|---|---|---|
| 8 px | 13 px | 60 of 95 off | `pitch-not-tileable` (13 is not 8 × n) |
| **10 px (design size, shipped)** | **16 px = 2 rows** | **all 95** | **emits: 188 tiles, 6,016 B, marker 12 px** |
| 12 px | 20 px | 60 of 95 off | `pitch-not-tileable` (the grid failure behind it would be next) |
| 15 px | 24 px = 3 rows | 38 of 95 off | `atlas-not-on-pixel-grid` |
| 20 px | 32 px = 4 rows | all 95 | `atlas-glyph-outside-cell` (ink is 2 cells wide) |
| 30 px | 48 px = 6 rows | all 95 | `atlas-glyph-outside-cell` |

The 8 px instrument face (`fusion8`, §5.0's variant A) **cannot have an atlas at any raster tried**:
at design size its line box is 12 px, at 12 px raster 18 px, and at 16 px raster 24 px (tile-aligned)
but its ink then spans two cells. `atlas_for("A")` refuses with `pitch-not-tileable`, which is why the
atlas exists for the product variant only and why A's title band carries no text element at all.
**That limit is this model's, and it is worth naming precisely because it is the first thing an 8 px
product face would hit:** the cell height *is* `hhea`'s line box, so the box must divide into tile
rows exactly — there is no path that pads a 12 px line box up to a 16 px pitch, even though the
layout side already rounds a pitch that way (`min_row_px = tile_px × ⌈line_box / tile_px⌉`,
`album_bake.py:1600`). Padding the cell to the declared pitch and offsetting the baseline inside it
is the change such a face would need, and it is a smaller change than a second rasteriser — but it
moves every glyph's row count, the atlas size and the pool term at once.

**What "one 8 px column per glyph" means, and the gap in it.** The module addresses a glyph as
`tile0 + row` (`glyphs.rs:392`, `tile_for`) and spends `TILE_PX` of map per character
(`tilemap_px`, `glyphs.rs:431`) — one cell, always. That is the premise the *pool* arithmetic rests
on (§5.1's "upper bound = one allocation per character cell" times the tile rows a line box spans),
so a glyph two cells wide would not be a bigger atlas, it would be a different sum. **Ink** is
checked against it; **the atlas does not check advance.** On the tracked cut that omission is
invisible: the widest advance is exactly 8 px (`%`), so `measure` and `tilemap_px` can never
contradict each other on the product face. On a face with generous sidebearings they would: bumping
`M` and `W` to a 10 px advance on a scratch copy of the same file **emits a 188-tile atlas with no
complaint**, and that table answers `measure("MW") = 20 px` against `tilemap_px("MW") = 16 px` — a
fit verdict would approve a line the cell-addressed draw path then places 8 px per character, which
is not the line that was measured, and the pool term (cells × rows) would be an undercount for it.
**Where that stands after 2a PR-(3):** the draw path now refuses to *compile* such a face —
`surface.rs` asserts `max_advance_permyriad <= TILE_PX × 10,000` for every face a declared text
element names, because one glyph per cell is only sound while every advance fits one — so the failure is loud at the consumer. What is still
missing is the emitter-side half, and it belongs to the first PR that changes the face rather than
to a refactor: refuse an advance wider than `TILE_PX` in `album_glyphs.py`
(`atlas-glyph-outside-cell`, or a sibling id if the message reads better), which also makes the
atlas's own case table honest for such a face, and add the self-test leg —
`album_glyphs.py:1732` bounds ink alone. A `const _: () = assert!` over `SLOT` inside the generated
module is the third form of the same sentence and is *not* written, because the generated module
does not know which cell model its consumer uses; the consumer's assert is the one that cannot be
forgotten.

**If the face changes, in this order.** (i) `layout_spec.toml`'s `[[face]]` entry (`:60-88`) — `file`,
`raster`, `licence`, `parent` are the whole switch; (ii) the `pyftsubset --unicodes=U+0020-007E` cut
committed under `assets/fonts/` beside its licence rows (`assets/fonts/README.md:51-59`), which is
what makes the entry resolvable at all; (iii) `make album-bake`, which regenerates `src/layout.rs`
**and** `src/glyphs.rs` from that one spec entry — and the atlas module's six `const _` asserts
refuse to compile if `layout.rs` was left behind, which is the drift this arrangement exists to
catch; (iv) `make album-glyphs-test`. **A 10 px replacement is close to free** if it lands on the
grid and keeps ink inside a cell: it is the same shape of face as `ark10`, and the entire elision
mechanism follows the measured marker width. **A 20 px face is a model change, not a swap**: with
ink 10–16 px wide it cannot live in one 8 px column, so either the cut is redrawn to fit a cell
(what `ark10` is — its ink is 7 px wide at raster 10) or the atlas grows a per-face `cell_cols`,
which moves `tile0 + row` addressing, `tilemap_px`, the pool upper bound in §5.1 and §5.5a step 6's
write cost, and it would want `fit` to satisfy the cell bound as well as the advance bound. Two
literal numbers test that boundary from the spec side: `MEASURED_FACE_TABLE`
(`album_bake.py:3421`) keys §5.1's measured table by `(face, raster)` — a new pair is a new row, and
a face absent from it is simply unlegged by that table — and a layout leg asserts
`facts.text_distinct == 188` (`album_bake.py:4173`), so a font change fails a gate by number rather
than by nobody noticing. The prose that quotes `94 glyphs × 2 tile rows, 6,016 B` (§5.1, here, and
§11's 2a row) is the third copy, and this file's own rule about numbers in prose applies to it.

### 5.6 Every on-screen state must be readable by the person who reads it (ruled 2026-11-16, corrected the same day)

**The person who verifies this product is colourblind** — and, specifically, they can separate **full
red from full blue easily** and cannot separate **violet from blue**. That is a hardware constraint on
the UI in the same class as 240×160 or 15 colours per bank, and it applies to the test ROMs' verdicts
as much as to the player's screens: a verdict nobody can read is a gate that never runs.

**The rule: a state must be carried by something other than colour, and the tree already ships that
something — `text.rs`.** A word is unambiguous to any verifier and to any future one whose vision
differs. Colour may accompany it as a second channel. Within colour, the usable axis is the
long/short-wavelength one (red vs blue survives deuteranopia and protanopia, the common cases); the
unusable one here is **two short-wavelength neighbours** like violet and blue. Also worth keeping from
the correction below: **pure blue against black is a 0.072-vs-0.000 relative-luminance step**, so a
"clearly visible" blue-on-black signal may be announcing nothing at all.

`album-play`'s scheme is therefore the boring one the rest of the repository already used: **white
backdrop while running, then BLUE + the word `PASSED`, or RED + the word `FAILED`** — and a boot
refusal gets red + `FAILED` with its named reason in the log.

**How this went wrong twice in one day, recorded because the failure mode is repeatable.** A PR brief
of mine said "no `text.rs`, no screens" to keep scope on the seam, so the verdict had to be a lamp;
the lamp was violet→blue and unreadable, so it became a blink; the blink's contrast was checked against
nobody, so it blinked pure blue (luminance 0.072) at black; blue-vs-black being invisible, the answer
became a *tone* as well — three inventions, ~500 lines of annunciator, and the whole time the
correct answer was sitting in `examples/flac_integration/src/text.rs`, used by `product_play_main.rs`
for exactly this. Two lessons, both cheaper than the code they would have saved:

- **Ask the verifier what they can perceive before designing the signal**, not after shipping one.
- **When a restriction of your own making causes a feature, remove the restriction, not the feature.**
  And: over-correcting a real constraint into an absolute one ("hue may never carry anything") is its
  own bug — it made §5.2's `ERROR` screen, which spells out "DATA ERROR" in words, suddenly
  "non-compliant" on paper. That claim is withdrawn; it is compliant, and it was always compliant.

Carried forward for Phase 2, in one line each: §5.1's verdict-colour trick stays, **paired with a
word**; §5.4's bar peak must not be hue-only (shape or luminance, plus the mm:ss beside it); §5.2's
`ERROR` screen is the model, not the exception.

**Carried forward, second edition (2026-11-20): the same rule governs size and shape, not only
hue — and it is judged at the device's scale.** "A signal the verifier cannot perceive" is the
defect; a 6-pixel-tall glyph or a control placeholder with no shape is the same defect in a
different channel. So: **a face or an icon is judged at 1× on hardware, or in an emulator at
exactly 1×, and never on a host mock scaled up.** `make album-preview` (§5.0) emits both
scales for exactly this reason: the 1× image is the legibility test, and the 4× image is for
checking dither and tile alignment. The three-inventions history above is the cautionary
precedent, and it transfers to shapes and sizes as directly as it applied to colour.

### 5.7 Presentation first, and what OBJ is for *(ruled 2026-11-24)*

**The rule, as a constraint rather than a preference.** While audio is playing, the main loop may not
block on presenting the display. Audio is the first-class citizen; the visuals are not allowed to make
the audio producer miss a period. There are exactly two legitimate `commit` sites: the boot scene
upload (before the chain starts) and the verdict takeover (after it stops). An **event-scoped**
main-loop commit is an *interim* exception — allowed only where §10.2's measured cadence keeps it away
from the DMA grace, and never for a per-frame UI. The 2a surface's 1 Hz clock commit is the current
example; AP-3c measures whether it can stay.

**Why `commit` is the wrong primitive for the steady state.** agb 0.25's
`GraphicsFrame::commit` calls `VBlank::wait_for_vblank` before it presents anything
(`display/mod.rs:314-337`). That call has a fast path, and it is **one VBlank narrower than "not more
than one commit per frame"**: the previous call leaves `last_waited_number` at the vblank count then
plus one, and the fast path is that value being strictly below the count now (`interrupt.rs:417-427`),
so **two** VBlanks must have elapsed since the last commit for this one to return without halting.
Otherwise it executes `swi 0x05` — `VBlankIntrWait`, a BIOS **halt** until the next VBlank, not a busy
spin (`syscall.rs:25-29`). One commit per video frame therefore halts every time, for up to
`VIDEO_PERIOD_CYCLES` = 280,896 c; a commit every few seconds therefore never halts at all. The Timer1
audio ISR still runs while the CPU is halted, so the audio chain is not the thing being paused; the
**main-loop producer** is. It waits once per commit, not once per changed cell. And the wait does not
depend on the renderer: `Object`/`OamFrame` are shown into the same frame, and `oam_frame.commit()`
runs *inside* `GraphicsFrame::commit`, after the wait (`object/unmanaged/object.rs:48-66`). Objects
alone do not remove it.

**What that does and does not put at risk, since the answer decides 2a.** The halt stops the producer
for up to one video period, which is 274 samples of a 560-sample feeder period — 0.49 of the period
whose cycles are `PERIOD_CYCLES` = 573,440, the same arithmetic as §5.2's "one period = 2.04 video
frames". It cannot delay the audio ISR (the ISR is an interrupt, and agb's display path masks no
interrupts and takes no lock the feeder holds — checked against 0.25.0), so the exposure is the
**ring's** slack, absorbed by the 546 ms prefill, not the `REARM_GRACE_SAMPLES` = 16-sample window —
that window is the *vblank handler's* bound, because the handler and the feeder are two IRQs at one
priority on a core with no nesting (§4.5). So a 2a commit at its declared cadence is not a splice
waiting to happen, and the honest statement of the rule is two clauses rather than one:

- **the steady state is the problem, not the call site.** `PASS_MARGIN_CYCLES` reserves exactly one
  video period for the whole frame slice a pass serves — take, position read, transport tick, record
  scan, one paint sample, and one possible commit halt. One worst-case halt spends that reservation by
  itself. Move the chrome to per-frame and every pass starts needing that slack, which is the same
  sentence as "the display schedules the producer"; that is what §5.7 forbids, and it is why the
  bound is written as *at most one commit per pass* in `Paint::sample`'s and `refresh_surface`'s docs
  rather than left as a rate that happens to be low.
- **"it is only 1 Hz" is a measurement, not an argument.** R1b's single 1 Hz text redraw on the
  sibling player came out 104 late of 16,553 periods on real hardware — audible splices, no underruns,
  and every headless term green. Nothing about the album surface's cadence reproduces *that* mechanism
  (this draw path holds no lock the feeder needs and dirties one background at a time), but the reason
  the 2a commit is called an exception in the first place is that this tree has one hardware witness
  that a low-frequency main-loop redraw is not automatically free.

**So, should 2a be committing from the loop at all?** Yes, and only as the control arm: it is the
cheapest thing that satisfies §5.2's cadence rule and §5.6's legibility need, it is gated to one
commit per pass by the latch compare, and it is what AP-3c's arm **A** exists to measure against arm
**B**. 2b's chrome — a badge, a scroll, a beat-driven bar — must not inherit this call site; it is
presented from VBlank. If AP-3c shows arm A costing the producer anything worth naming, the exception
is retired inside 2a and §11's 2a row is amended there; if it comes back clean, the OBJ chrome stays
2b's first commit and this paragraph is where the shipped 2a behaviour is justified rather than
apologised for. **AP-3c came back clean (2026-11-25, 2b PR-(2))**: arm A's 1 Hz commit costs a mean
12,222 c, takes agb's fast path instead of the VBlank halt, and the feeder shows `late_max=0` and
zero underflows across 87 periods. So the exception stays for 2a, the OBJ chrome remains 2b PR-(5)'s
implementation, and the 60 Hz arm's 12,686 c/frame + 2,026 c/frame VBlank handler is the ceiling a
*per-frame* chrome would have to justify — measured, not assumed.

**The path that removes it is VBlank-driven presentation.** The draw decision writes a shadow; the
VBlank ISR or a VBlank-started DMA copies it while the main loop continues. For OAM that is one 1 KiB
shadow (128 × 8 B), double-buffered and swapped before the VBlank DMA; for a BG it is the dirty
screenblock (2 KiB, or a set of changed entries). The CPU still spends the copy cycles — a 1 KiB OAM
DMA is ~256 cycles at 32-bit — but the producer is not blocked. `embassy-agb` is the async equivalent.
That is the policy this plan adopts: **present from VBlank where the hardware and agb allow it, and
treat a main-loop `commit` as a measured exception, not the model.**

**Two independent axes, which the earlier "OBJ vs BG" question conflated.** The scheduling win is the
first row; OBJ is one storage choice that makes the first row clean for the chrome and the viz.

| axis | choices | what it decides |
|---|---|---|
| **presentation timing** | main-loop `commit` vs VBlank DMA/ISR | whether the producer can be paused by the display |
| **storage/renderer** | BG cells + map vs OBJ sprites | where the pixels live; the tile pools, OAM, fit model |

**2a today, as the control.** Three backgrounds — BG0 art, BG1 scene, BG2 foreground — with the
runtime committing only when a cell changed. agb's `RegularBackground::show` includes a map only when
`Tiles::is_dirty` says it changed, but a dirty map is copied **whole** (2,048 B) by `commit`; clean
backgrounds cost no map copy (`regular_background.rs:401-416`). `OamFrame::commit` also runs on every
commit and clears the unused OAM entries, a small fixed cost even with no objects. A clock tick
therefore pays one dirty foreground screenblock (2 KiB) plus the VBlank wait (usually the fast path);
a still frame pays neither. The three-background split is the layering baseline; §5.7 does not change
it until AP-3c says the presentation path should.

**OBJ Chrome — the first candidate, and the plan.** The scope is the chrome, not the list: title (20 cells),
clock (12), state word (9), five icons, bar (10) — **~56 OAM entries of 128**, an 8×16 sprite for each
text column and an 8×8 sprite for each icon/bar cell. The chrome rows carry at most 26 sprites on a
line, well inside any sane per-line budget. The list is deliberately not in this plan: its 30 × 8 =
240-cell window cannot be 128 sprites, and §5.2 already says it scrolls as a tile.

What it needs:

- the glyph atlas and the icon/bar tiles copied into **OBJ VRAM**: 188 glyph tiles + 14 icon/bar
  tiles = **202 tiles ≈ 6.3 KiB** of the separate 32 KiB / 1,024-tile pool (206 / ≈6.6 KiB if the
  four fills are duplicated for a chrome-art variant). The OBJ tile index is the atlas's own tile
  index; the 8×16 sprite uses `tile0` and `tile0+1`;
- one **OBJ palette bank** (16 colours) for the same white/slate/amber values, separate from the BG
  palettes (§5.4);
- a **double-buffered OAM shadow** (2 KiB) and a VBlank-started DMA3 source swap; unused entries
  hidden; the OAM DMA stopped and the chrome entries hidden before the verdict's `text::show`;
- the **cell geometry and `Write` plan stay**: the chrome's `Write` x/y are the cells, the sprite is
  placed at `x*8, y*8`; the `draw_text`/`draw_bar` diff and the host geometry legs are unchanged.
  This is not a proportional-layout change; sub-pixel placement is a separate, later decision;
- **the list, title-card and art scenes stay BG**. The freed foreground BG is exactly the layer 2b's
  list text needs over the scene art; the chrome OAM sits above BG0/BG1 by priority. One atlas, two
  consumers (OBJ chrome, BG list), not two faces or two atlases.

What it buys: no main-loop commit for chrome; OAM-shadow writes of 8 B per changed cell; a 1 KiB OAM
DMA at VBlank instead of a 2 KiB dirty screenblock copy plus the wait; the producer keeps running.
What it costs: the OAM/atlas/VPAL/DMA subsystem, the double-buffer race rules, the VBlank DMA's bus
contention with the sound DMA1/2, a preview/host-gate OBJ compositor, and the plan's §5.3/§5.5
"OBJ text" exclusion amended for the chrome (the list's cell model is untouched).

**Memory pressure today: none on the OBJ side.** The product images draw no sprites: OBJ VRAM 0 / 32
KiB, OAM 0 / 128, VPAL 0 / 16 banks (AP-0's probe is the only sprite this tree has ever drawn). The
tight pool is the BG side: variant B's audit is `POOL_DISTINCT = 462 / 512` at its declared ceiling,
with the runtime used-glyph set smaller — and **AP-3b's L4 stand-up (2026-11-25) read the same shape
back from the maps: 460 live slots for art 256 + atlas 188 + icons 16, with 189 slots shared and one
glyph slot carrying 45 references**, so the dedupe is measured rather than assumed. The chrome plan
above is ~202–206 / 1,024 OBJ tiles,
56 / 128 OAM and 1 / 16 VPAL; a viz has ~72 entries and ~25 KiB left. **The scarce shared resource is
the OAM/per-line budget, not OBJ VRAM.** The chrome OBJ does not free the 188-tile glyph atlas from
the BG pool (the list still draws from it); it frees the foreground map and the icon/bar BG tiles, and
the OBJ atlas is a duplicate. That is the price of the presentation path.

**OBJ Scenes and the viz — the second dimension.** A scene or visualization can choose BG or OBJ, and
the choice is a pool/budget question rather than a style one:

| | BG tiles/map | OBJ sprites |
|---|---|---|
| memory | the shared 512-slot BG pool; a region canvas is 480–510 tiles | the separate 32 KiB / 1,024-tile OBJ pool |
| motion | map rewrites (dirty screenblock, 2 KiB) | per-object x/y/tile; no map dirty |
| per-frame | a commit if the map changes | OAM DMA, ~1 KiB/VBlank if dirty |
| limits | `tile_budget`, residency (R14) | 128 OAM, per-line OBJ budget, VPAL banks |
| good for | full-bleed/static art, list rows, repeated bands | moving bars, particles, a beat-driven badge |

The chrome's 56 OAM and ~6.3 KiB leave **~72 OAM and ~25 KiB** for a visualization, before any BG
side is considered; the scarce shared resource is the **OAM/per-line budget**, not OBJ VRAM. AP-6
declares and audits that ceiling (tiles, OAM, VPAL) before A9's first moving sprite is designed, and
the chrome reservation is part of the same audit. The BG pool remains the tight one: variant B's
`POOL_DISTINCT` is 462/512 at its declared ceiling, and moving the chrome to OBJ frees the foreground
map and the icon/bar BG tiles, **not** the 188-tile glyph atlas (the list still draws from it); the OBJ
copy is a duplicate, not a relief. That is the honest trade: the chrome OBJ is affordable in OBJ VRAM
and pays in duplicated atlas bytes, while the BG pool keeps the text atlas for 2b's list.

**Where this is scoped, investigated, and implemented.**

| phase | what | why here |
|---|---|---|
| **2a** | **this section** (the policy), and **AP-3c**: a paired L1 probe — (A) the current BG foreground + event commit, (B) the chrome as OBJ + double-buffered OAM shadow + VBlank DMA3, (C) B under a 60 Hz synthetic chrome load. Measure main-loop pass cycles, `paint.due` cadence, `late`/`underflows`, VBlank/ISR/DMA cost, and the dirty-screenblock commit cost. **The 2a product still draws no OBJ**; the probe is an instrument. If AP-3c shows the 1 Hz/event commit is already a producer risk, the chrome OBJ + VBlank path is a **2a follow-up** (AP-3b/AP-3c) and the 2a row and §5.1 note are amended in that PR. Otherwise it waits for 2b. | 2a owns the chrome and AP-3b's render-leg instrument; the question is measurable now; the product phase does not get a new renderer without the measurement. |
| **2b** | **implement the chrome OBJ + VBlank OAM DMA at the start** (unless 2a already did): the list, title-card and art scenes stay BG; the freed foreground BG becomes the list's text layer over the scene art; the scene/list map changes are presented from VBlank too (dirty screenblock), the same policy; the scene switch; no viz. | 2b adds the scenes and needs the freed BG; the chrome is already there; the OAM/DMA path is shared infrastructure. |
| **4** | the **viz (A9) may use OBJ**; AP-6 declares/audits the OBJ tile/OAM/VPAL budget, including the chrome's 56-entry reservation; the first *moving* OBJ is Phase 4's beat-driven thing; the BG-vs-OBJ trade-off above is decided per viz, not for the list. | the viz is the first thing that actually moves; the chrome is static-ish and is the presentation-path proof, not the motion proof. |
| **6** | the game (A15) reuses the same OBJ budget; AP-8 owns its arm. | same rule. |

Not scoped here: the **list as OBJ** (240 cells > 128 entries); **full-bleed art as OBJ** (a BG is the
right tool); the **viz in 2b** (Phase 4, AP-6); the **chrome OBJ as a 2a product feature** unless
AP-3c forces it. The VBlank **policy** applies from 2a onward; the current 1 Hz event commit is the
interim exception the policy names, not a precedent to copy.

**The measurement is the gate.** AP-3c's paired arms use the existing probe feeder and
`[album-probe] PASS` shape (`examples/album_probe`), headless on mGBA and on AP-7a's cart; the
control and OBJ arms print the same cycle columns and the same `late`/`underflows` terms, so the claim
"the main loop no longer waits" is a number rather than an inference. If the probe wins, the 2b
implementation is the same `Write` plan with an OAM applier; if it does not, the plan has saved a
renderer.

---

## 6. Time, and a progress bar, with no divide (A6, A7)

The ARM7TDMI has no divide and this tree avoids `__aeabi_*div*` on any measured
path ([`PLAN-OF-RECORD.md`](crates/uli-lite/PLAN-OF-RECORD.md): "No divide on the
measured path"). The clocks are powers of two, which makes the easy half free:

| quantity | how, at 2¹⁵ = 32,768 Hz (2¹⁴ at 16,384) | cost |
|---|---|---|
| seconds (album position) | `pos >> 15` (`>> 14`) | 1 cycle |
| frame ↔ sample | a shift by the backend's `log2(frame)` (a hop-512 frame is `frame << 9`) | shift |
| `ALVZ1` tick (§8) | `tick(pos) = (pos * 30) / rate`, done as `(pos >> 15) * 30 + (((pos & 32767) * 30) >> 15)` at 32,768 Hz — a shift-and-multiply that avoids the 64-bit product | 3 multiplies |
| mm:ss from seconds | `m = (sec * 0x4444445) >> 32; s = sec - m*60` — one `UMULL`, **verified exact for `sec < 2^20`** by a host test (an album needs `sec < 2^12`) | ~10 cycles |
| elapsed in a track, and its total | `pos - start`, and `n` read straight from `ALBM1` | 1 sub, 0 adds |

**What the chrome shows is `elapsed / total` (re-ruled 2026-11-20, from `elapsed /
remaining`).** The arithmetic above is untouched — this is a display ruling, not a numeric
one — and the reason is the one a person notices the first time they use a player: a
remaining figure changes on every redraw and cannot be compared against the bar beside it,
while `total` is the number the manifest states and the list scene prints per row, so the
two surfaces agree by construction. **`remaining` is still computed wherever it is needed**
(one subtract, and §4.4's fade and skip paths already live on those positions); it is simply
no longer what the chrome prints. The row this supersedes is marked in §3's A6, and §13
AP-2's already-passed claim text is amended rather than re-run: it demanded "elapsed +
remaining match the durations the pack step printed" and now reads "elapsed + total".

The bar *is* a divide (`pos_in_track / track_len × bar_px`) and there is no
integer shortcut that is also honest. Two candidate designs, and the choice is a
measurement, not a preference:

1. **Bake-time reciprocal (recommended).** `ALBM1`'s track entry stores
   `recip = floor(2^32 × BAR_CELLS / n_samples)` (the 64-bit divide is done by the
   *tool*, on the host, where a divide is free), and the ROM computes
   `cell = (pos_in_track as u64 * recip) >> 32` — one `UMULL`, one shift.
   Checked on the host over track lengths from 1,000 to 2.6 M samples: `cell`
   is monotonic in `pos`, never reaches `BAR_CELLS` (no overrun), lands on
   `BAR_CELLS-1` at the last sample, and differs from the exact quotient by at
   most **one cell**. Those four properties are the bake-time assertions, not
   hopes.
2. Precomputed 32-entry threshold table per track: `bar = first k where pos < thresh[k]`.
   Zero multiplies, a fixed 128 B/track in ROM, and it makes the monotonicity
   invariant trivially checkable on the host.

**The grid is declared per layout variant, not fixed in prose (2026-11-20).** This section
used to say the bar is "a fixed grid of **32 or 64** equal cells", which made a widget
width a constant of this file. It is now a field of the variant (§5.0) — `bar_cells` and
`bar_px` — with a **floor of 16 cells** recommended, because the honest cost of a short bar
is not pixels, it is precision: with the grid at *N* cells, the ±1-cell error this section
already proves is **100/N percent of the track**, so the bake report prints that percentage
beside the geometry that produced it (32 cells → 3.1%; 18 → 5.6%; 9 → 11%). Fine cells are
cheap where the grid is narrow: a bar may use *partial-fill pattern tiles* (tiles filled
to each reachable fill offset, plus one empty and one full), so a 72-pixel-wide bar in the
right of the title band can still be 18 cells of 4 px rather than 9 of 8 — at the cost of
**3 pattern tiles, of which only 1 is extra** over the empty/full pair any bar needs. That
figure replaced “~5 extra pattern tiles”, which was a guess at *a handful*; the count follows
from the geometry and `--layout-self-test` counts it rather than quoting it. A fill boundary
can only land on a cell step, so the distinct partial tiles are exactly the residues of
`bar_px / bar_cells` inside an 8 px tile: a 4 px step reaches two of them (0 px, which is the
empty tile already paid for, and 4 px), and the finished track needs the full tile — 2 + 1 =
**3**. At variant B's 1 px cells every residue is reachable and the same rule gives 8 + 1 =
**9**, which is why this section states the rule and the figure belongs to the variant (the
two paths the self-test runs are the residue set and a render of the bar at every fill
position, agreeing).

**One invariant that has to be written down because it joins geometry to audio arithmetic.**
The reciprocals are computed *per track at bake time* from the cell count —
`scripts/album_bake.py` does `recip = ((1 << 32) * bar_cells) // n_samples` with `bar_cells`
read from the selected variant (`bar_recip(n_samples, bar_cells)` at `album_bake.py:328`), so a
variant that declares a different count cannot be baked against the old one by accident:
`BAR_CELLS_REGISTERED = 32` survives at `:104` as a value two call sites pass *by name* — §6's
worked example (`:4274`, and the leg beside it that asks the same function at 32 and at 80) and
the Phase-1 fixture legs that build a synthetic `ALBM1` (`:4529`, `:4553`, `:4567`, `:4580` and `:4581`) — and
it is the default of nothing: `bar_recip` (`:328`), `assert_bar_reciprocal` (`:352`),
`build_albm1` (`:436`) and `audit_blob` (`:581`) all require the grid as an argument, so there is
no path through this tool that gets a grid without choosing one. That is what makes the next
sentence checkable rather than hopeful: **the declared `bar_cells`, `ALBM1`'s reciprocals and
`layout.rs` must agree, and `album-layout-test` asserts that they do.** That is the
cheapest possible guard on the one place where a layout number and a decoder's arithmetic
touch, and it is the reason the layout spec owns the bar rather than the app module.

**What that agreement costs today: the invariant is the *agreement*, not the value 32 (re-ruled
2026-11-20, and discharged by 2a's first PR).** The cell count is no longer hard-coded on either
side of the seam. It is declared once in the tracked spec
(`examples/album_player/layout_spec.toml`, per variant), generated into the app's gitignored
`src/layout.rs` as `layout::BAR_CELLS`, and passed into the arithmetic as an argument —
`bar_recip(n_samples, bar_cells)` (`crates/gba-playback/src/nodiv.rs:105`), whose count the bake
supplies from the selected variant (`scripts/album_bake.py:1001`,
`recip = bar_recip(container["total_samples"], facts.variant.bar_cells)`). The shared crate
therefore holds no grid of its own, and a second opinion there is what §9.0's `bar-cells-disagree`
row and the layout self-test's banned-box leg exist to catch. `bar_cell(pos, recip)`
(`nodiv.rs:121`) never needed the count — the grid is already inside the reciprocal — so **the
runtime path did not change when the grid stopped being a constant; only the guard moved.**

A variant declaring `bar_cells = 18` (§5.0's starting default for A) is now simply a variant with
an 18-cell bar: the grid is generated for it like the rest of its geometry, and the bake
pre-flights the reciprocals against that value rather than against 32. What remains is the refusal,
kept deliberately: `meta::parse` compares the manifest's `bar_cells` against the linked
`layout::BAR_CELLS` (`examples/album_player/src/meta.rs:595`, `Refusal::BarCellsMismatch`, whose
message now reads *"bar_cells {n}, this ROM's generated grid is {N}"*, `:229-233`) and re-derives
the reciprocal against the same value (`:827-830`); the host suite's refusal arm mutates the header
to `layout::BAR_CELLS - 1` rather than a literal (`tests/album_pack_host.rs:596-605`). **The
boot-time refusal is kept, not replaced** — ruled 2026-11-20 while scoping 2a's first PR — because
`meta.rs:595` is the only place that catches a stale generated `layout.rs` linked against
yesterday's manifest, R11's staleness failure, invisible to every host test. The two bake checks
and that backstop all stay, exactly as §9.0's row says: `bar-cells-disagree` (declared value ≡ the
grid the reciprocals were computed for ≡ `layout.rs`) and `bar-cells-out-of-range` (**a floor of 16
as this section already recommends, and a ceiling from arithmetic rather than taste: the reciprocal
fits a `u32` while `bar_cells < n_samples`, which no real track is anywhere near, so the ceiling is
the precision rule above**). A 5-cell bar is arithmetically legal and 40 px of 8 px cells is a clean
shape; what the rule does is make its ±1 cell = 20 % of the track a *printed* number in the bake
report next to the geometry that produced it, so the coarse bar is an authored choice rather than an
oversight.

**AP-2 is the gate that says these
agree:** the bar cell at refresh *n* and the printed elapsed must derive from the
same session-clock read and must never disagree by more than one cell, and the ROM must
not divide where it is not allowed to (a symbol fact on the linked image, in the
`make overlay-objdump` style — "no `__aeabi_u*div*` in the playback + UI path").

Total time per track is the **manifest's**, not a guess and not the codec's:
`ALBM1`'s `n_samples` for that track (§4.3), which the bake tool cross-checks
against the container the backend actually produced and refuses if they disagree.
A `make album-bake` run prints the full track list with computed durations, which
is the same number the ROM will show — a differential between the tool's
arithmetic and the ROM's, in the `gafp-pack-check` tradition.

---

## 7. Album art (A8)

**"Even if at a low resolution" is the right instinct, and the constraint is
palette, not pixels.** GBA mode 0 tiles are 4bpp: 16 colours per tile, chosen
from an `Rgb15` (5 bits per channel — 32,768 colours addressable, 256 BG entries
live at once). Plan:

- **Source → art pipeline (host, Python, stdlib — corrected 2026-11-20; this bullet used to say "PIL"):**
  **Pillow is not a dependency of this tree and is not in the build environment either** — which the
  probe’s own note records about its own tooling: *examples/album_probe/ART1.md:88-91*, “no image file in
  the repository and no Python imaging library in the toolchain (‘import PIL’ fails on this host)”. The one
  image generator in the tree is stdlib-only and already implements most of what the rest of this bullet
  specifies: `scripts/album_probe_art.py`, 614 lines — a deterministic generator (a vertical gradient plus
  a diagonal phase, 11 palette entries at a documented `0r8g4b` encoding, a 2×1 Bayer dither, a hard
  `PALETTE_MAX` overflow panic, and 4bpp bit-packing to exactly the byte order §9.0’s test vectors pin).
  **So `album_art.py` is that tool pointed at a real cover, not a first attempt at one** — and the gap
  between the two is not the quantiser, it is the cover (R15).
  The pipeline proper, as the rest of this bullet describes it: downsample the
  cover to a **declared** box — the legal list is *derived* from the selected
  variant (§5.0), never typed, and the derivation is the next bullet — quantise to
  ≤ 15 colours per chosen palette bank (median-cut/octree, then a hard refusal if
  the palette count is exceeded rather than a silent collapse), order-dither
  (Bayer 4×4) *before* quantisation with a strength the tool prints, then emit
  4bpp tiles + a BG map + palettes into `ART1`.
  Conventions are hardware facts, restated here so the tool has a single spec to
  satisfy: a 4bpp tile is 8 words / 32 bytes, pixel *x* of a row is nibble *x*
  with the low nibble on the left, a BG map entry is
  `tile | hflip<<10 | vflip<<11 | palette_bank<<12`, and **an off-palette source
  colour is an error, never a nearest-match** — that refusal is what keeps art
  honest, because a silent nearest-match is how a cover quietly turns to mud.
- **Which boxes are legal is derived from the variant, and anything else is a refusal.**
  Legal means: (i) dimensions are multiples of 8 px; (ii) the box sits **inside that
  variant's scene region** — an art box that overruns a band is not a taste disagreement,
  it is a bug; and (iii) the tile count is within the variant's declared `image_tiles`, so
  the image and the glyph/chrome/bar tiles fit the variant's declared `tile_budget`, so they can be served
  from agb's tile pool at once (§5.1, R14). The
  old candidate list — **120×120** centred, **160×96** for the animation layout, **240×160**
  full-bleed for a no-viz build — is superseded by that derivation, and each of the three
  lands differently: **120×120 stays legal and is the largest square variant A's rows admit
  with a tile margin** — centred vertically, 4 px off centre horizontally, which is §5.0's
  last column saying so out loud rather than a footnote; **160×96 loses its reason to exist**
  — the 96 px band it was sized
  for is gone, and a visualiser is now a scene holding the whole region (§8); **240×160
  survives only as an explicit `bleed = true`,** priced immediately below rather than listed
  beside free options.
- **Full-bleed, priced (2026-11-20 — the question deserved an answer, not a bullet).** "Not
  a native mode" is the wrong worry. Mode 0 composes *tiles*, so a full-bleed picture is
  the tile system working normally, and the two costs people assume are not the costs:
  - **Legibility is free.** A map entry picks its palette bank per tile (§5.4), so the bands
    are opaque over the art with no blending at all — which is also how this plan honours
    §16's "no alpha blending beyond palette tricks": a dithered band edge *is* the palette
    trick, already §7's technique.
  - **ROM is nearly free, and the plan should stop implying otherwise.** 19.2 KB of art is
    ~**0.40 s** of lossless audio at the 380.3 kbit/s the reference master measured (§11's
    pack report) or ~**1.5 s** at ULI2's 12,735 B/s (§15 Q1). The "art is carved out of the
    audio budget" rule below is real and costs under two seconds of program; attaching the
    number is what stops the rule being applied as folklore.
  - **VRAM is where it gets tight — though not for the reason this bullet used to give.** It
    said 600 tiles = 19,200 B "does not fit in one 16 KiB character block, which holds 512
    tiles", and that is not the limit: a BG map entry carries a **10-bit tile index**, so one
    background addresses indices 0–1023 = 32 KiB of tile data = two blocks, and 600 tiles is
    inside that (§5.1, corrected). A 240×128 region canvas (480 tiles) and a full-bleed image
    (600 tiles) are both placeable *in principle*; what they are not is **co-resident with the
    chrome and the glyph cache inside agb's regular (BG) pool, which is 512 4bpp slots in
    total**, with a fallback into the affine pool behind it (§5.1 cites the allocator). So:
    a 225-tile cover leaves ~287 slots — **not "roughly what the text and chrome need", which was
    this bullet's old wording**. §5.1's measured interaction table prices a list window at
    **704 slots at the visible-cell bound** (8 rows × 44 cells × 2 tile rows, the chosen face at the
    16 px pitch) against a 512-slot pool, so it fits only through the distinct-glyph bound (≤188 for
    a 94-glyph ASCII cut) — **reachable only by building §5.5a's shared glyph atlas rather than by
    measuring the per-cell one, whose refcounts AP-3b's pool leg checks prove.** A region-canvas
    image leaves ~30, which serves no text at all; **full-bleed cannot be served from that pool at
    all and has to spill**,
    which makes it an AP-3/AP-4 record of *pool state and fallback behaviour* rather than of
    block placement (§14's R14) — and the per-variant `tile_budget` refusal (§5.1) is
    what should catch it before a ROM is built.
  - **Time is the real price — and this bullet is arithmetic, not a measurement (marked at
    review, 2026-11-20).** Scaled from AP-0's measured 225-tile upload (whole 346,057 c live,
    per-tile mean 1,242 c; worst tile 6,998 c = 5.6× that mean, one image, one arm, n=1):
    **~0.75–0.9 M cycles ≈ 1.3–1.6 FLAC periods ≈ 3–3.5 lossy periods ≈ 20–26× the margin one
    lossy period can spare** (§10.2's row). The period bases are named so the row can be checked:
    a FLAC period is `PERIOD_CYCLES = 573,440` — a 560-sample chunk at 16,384 Hz × 1,024 c per
    sample, `examples/album_player/src/geometry.rs:154-190` — and a lossy period is 512 samples
    at 32,768 Hz = 262,144 c (§4.2b). So it is only ever paid *sliced*: at ≤ one period's spare
    margin per period that is **~21–26 periods — 0.33–0.41 s of audio time on the lossy arm, at
    15.6 ms a period**; on the FLAC arm the slice budget is that arm's own (its margin is not
    36 k c — §10.2b re-prices it) and its lead is 1,230 ms, so the wall-clock window **depends on
    which backend is playing, as does the lead that pays for it** (§12.2). The old single "~0.4 s
    out of the ring lead" is therefore not a number to quote without its arm. And note the
    default 225-tile box is already ~9.6× that same margin (346,057 against ~36.0 k c), which is
    why §10.2's rule is
    about slicing rather than about keeping art small. **AP-4 measures the 600-tile leg rather
    than inheriting this one**: the figure being scaled was measured once, on one image, and
    scaling it by 600/225 is the arithmetic kind of row this file normally refuses to leave alone.
  - **Palette is the artistic price.** The same four banks (§5.4) spread over 4× the pixels
    make the dither more visible at the same colour count; and the 8bpp alternative below is
    14,400 B at 120×120 but **38,400 B full-bleed — 600 tiles at 64 B, past the ~510 8bpp tiles
    agb's two pools can serve at all (§5.1), which is where that
    option dies** for a full-screen image.
  **Ruling: `bleed = true` stays legal**, gated by AP-4's existing audit ("never exceeds its
  tile/palette allocation (linker map + VRAM audit)", which now names the full-bleed arm)
  plus an **occlusion report** the tool prints: which rows will sit under the bands, what
  fraction of the pixels will never be seen (~20–25% at 240×160 with two-row chrome and a
  one-row title band), and a refusal if a declared safe area would be cut. Default stays
  region-fit, because composing for what is actually visible is the cheaper discipline. The
  reason to keep the option is a product one: a title card with the cover *behind* the
  bands is a better screen than a framed thumbnail.
- **Cost, and who pays it.** 120×120 at 4bpp is 7.2 KB of ROM and 7.2 KB of VRAM
  (measured: 7,200 B of tile bytes and a 7,650 B VRAM price including the map —
   [`examples/album_probe/README.md`](examples/album_probe/README.md)); full-screen is 19.2 KB
  each, and in seconds of program that is 0.40 s and 1.51 s (bullet above). Art is carved
  out of the **audio budget**, exactly as this plan rules (§10.1): **art and reaction data
  are carved out of the audio budget, never the other way round,** so `album.toml`'s art
  choice visibly changes the kbit/s the encoder targets, and the bake tool reports both —
  including, from 2a, the derived box list for the selected variant so `album.toml` cannot
  name a box the variant does not allow (§5.0). **At 2a that list is generated and printed, while
  `[art]` itself is still refused as a section (D2): the check which makes this sentence true is
  raised by 2b's parser against that list, and until then the pool sum is taken at the variant's
  derived ceiling (§5.0, §9.0's register).**
- **Two art modes to start:** `cover` (still image, one bank) and
  `cover+breathe` (the same tiles with a 1-palette-per-frame hue/brightness cycle,
  ~15 cycles/frame, which reads as a "living" cover for free). Runtime tile
  changes are available (`set_tile_dynamic16`, `replace_tile`) but any per-frame
  tile rewrite is VRAM DMA over the same bus the audio DMA uses, so it is a
  measured decision, not a free flourish.
- **The 256-colour alternative, priced.** Mode 0 can also address **8bpp** tiles,
  which buys one 256-colour BG palette bank at double the bytes per tile (64 B per
  8×8): 120×120 art becomes 14,400 B of ROM *and* 14,400 B of tile data — and that is already
  **nearly half of everything agb's pools can serve at 8bpp**, since its two pools manage the
  equivalent of ~510 8bpp-tile slots (the regular pool's 512 4bpp slots = 256 of 8bpp, plus the
  affine pool's 255; §5.1 cites the source) — crowding out anything else drawing from them. And
  an 8bpp full-screen image is
  **38,400 B = 600 tiles, which is past that ceiling outright: agb cannot allocate 8bpp
  full-bleed at all**, whatever the hardware's map bits would have allowed. That is a stronger
  objection than this bullet's old reason, "over two character blocks" — wrong in the same way
  §5.1's was, and a capacity fact beats an alignment one. So this is where the full-bleed option
  above loses
  this alternative rather than gaining it. The better-looking option for a title
  card, at a real VRAM price. **Default is 4bpp across up to 4 banks (§5.4);** the
  8bpp variant is an AP-4 side-by-side judged on a real screen, not on a diagram — and
  "a real screen", for that judgement, means §5.6's 1× rule and `make album-preview`'s 4×
  image only as an aid.
- **Non-goal:** full-motion video, per-frame art streams, or an art decoder. The
  GBA has no bus bandwidth for that alongside real-time decode, and the
  dithered-still-with-palette-motion result is what "album art on a cartridge"
  should look like.

---

## 8. Visualisation (A9) — "like old Windows Media Player"

The honest framing: the WMP-era animations worked because the PC analysed the
audio cheaply and the machine had spare cycles. On the FLAC path this tree has
**~68% of audio time left after the decoder on real music** (work/frame 32.208% and decode
23.656% on hardware, C2's master run — §10.2b; the synthetic FIXED-0 fixture's 18.99% /
~81% is a shape an album's frames never contain), and on the
album-length path **a measured amount: 13.73 points at the worst frame**. U4 is **met** (2026-11-19 — decode
wall 74.27% worst / 64.60% mean; **gate total, i.e. decode + convert + IRQ + DMA, 86.27% worst /
76.53% mean**, [`ULI2.md`](ULI2.md) §Gates), and a visualiser is per-period work, so the number
it has to fit inside is that worst-frame one and not the 23.47 points the mean would suggest. *(corrected
2026-11-19: this paragraph and six others read "U4 is not met — 105% worst / 89% mean", which
was the 2026-11-15 ledger and is superseded there.)* And the caution is measured, not
hypothetical: the lossy track's own player, on hardware (measured 2026-11-16, before the decode
pass that won today's margin — the defect is the point, not the percentage), measures
**94.52%–95.89% worst** with a
**1 Hz text redraw** delaying the feeder past the DMA's grace on **104 of 16,553 periods**
([`ULI2.md`](ULI2.md) → U5) — one redraw a second, audible splices, no underruns. So the CPU a
real-time decoder frees up is not automatically the visualiser's; it is the gate total's, and
§10.2 is where the claim gets written — with AP-6/AP-8 (R1b) as the debt that settles it.

**Ruling for this plan: analysis is offline; the GBA only reads and draws.** The
player's whole job is to hit an audio deadline on a 16.78 MHz core with no FPU;
"what is the loudness / the beat / the spectrum right now" is a question the bake
tool answers once per album, on a machine with gigahertz to spare, and writes down
as a table the ROM walks forward through. So the app defines its own reaction-data
format, `ALVZ1`, and this is where its whole design fits in one paragraph:

- **Timeline: 30 ticks/s of *album* time**, indexed by the same album-sample clock
  the manifest defines (§4.3) — so a seek, a skip and a pause cost the visualiser
  nothing but an index recomputation, and the tick is stable across backends
  whatever their frame or chunk sizes are. 30 Hz is half the refresh rate, which is
  the coarsest tick that still reads as continuous motion, and it is what keeps the
  data small.
- **Streams, each independently present or absent** (an absent stream's offset is 0,
  and the bake refuses to store a stream the chosen visualiser does not read):
  `loud` — one byte per tick, 4 bits of level per channel, dB-scaled against the
  album's own 99.5th percentile so a quiet record still uses the full 0–15 range,
  instant attack and a ~20 dB/s release (a falling meter is what the eye reads as
  "animation"); `beat` — one bit per tick, an onset flag with a refractory period
  around 0.2 s so it marks musical beats rather than every transient; `bands` —
  six bytes per 15 Hz band-tick, twelve log-spaced bands from ~40 Hz to the
  Nyquist the codec even reproduces, 4 bits each, per-band percentile scaling.
- **Size, by arithmetic, not by hearsay:** `loud` + `beat` = 33.75 B/s ⇒ **~91 KB
  for a 45-minute album**; `bands` = 90 B/s ⇒ **~243 KB**. That comes straight out
  of the audio budget (§10.1), which is the honest price of a spectrum bar versus a
  level meter, and the bake tool prints both. The header is a magic, a version,
  section offsets and the album length in samples, 4-byte aligned — the same shape
  as every other blob in this plan.
- **Cost on target:** one or two `u8` ROM reads per 30 Hz tick plus the drawing.
  That is the claim AP-6 measures; nothing here asserts it.

Choosing this over on-target analysis also sidesteps two traps: analysing the
post-DAC 8-bit signal (which is not the music, it is the DAC's problem) and paying
for an FFT on a core that is already spending most of its period finishing a frame.

Runtime analysis stays on the table as a **Phase 4 experiment, not the plan**: the
int16 pre-DAC tap exists on both backends (`PcmSource::pull_frame` hands the app
int16, §4.2), and a 4-band recursive filterbank over 512–2,048 samples per frame is
a few hundred cycles of integer multiply-accumulate. If it is ever pursued it
enters as a second `ALVZ` source behind the same reader, with a measured row, and
its bar is that it must not move the audio cost ledger at all. That is the standard,
written down before the number.

What to draw, in ascending ambition (each is its own PR-sized thing with its own
budget row):

| viz | streams | elements | note |
|---|---|---|---|
| `bar` | loud | 2 rows of segments + peak-hold | the LED meter; the simplest honest thing, and the one that proves the timeline end to end |
| `wave` | loud | a scrolling 240-px history in a dynamic tilemap | cheap, and very much the WMP-era look |
| `skyline` | bands | 12 bars + peak caps + a dim reflection | the classic EQ; costs the 243 KB |
| `deck` / `critter` | loud + beat | reels that spin with the music, a thing that bobs on the beat | the animation tier; sprite work (§5.1's OBJ pool) |

`beat` is what makes any of them read as *reactive* rather than *random*: a sprite
that hops on an onset, sleeps while paused and startles on a skip costs one bit per
tick and reads as more alive than a hundred cycles of bar math. Those three states
are free from the player's side — pause and skip are already events the UI knows
about (§4.4).

**Where a visualiser lives now (2026-11-20).** This section was written against the old
"visual in the top 96 px" picture, and the re-rule changes its frame without changing its
arithmetic. A visualiser is a **scene in the region** (§5.2) — Phase 4 and AP-6 are
unchanged, and the drawing area is whatever the selected variant's region is — 16 tile rows
(128 px) in B, 17 (136 px) in A, §5.0; **tile rows, and at a 16 px product pitch eight text rows,
not sixteen or seventeen** — the distinction §5.0's diagram now spells out because this sentence
didn't. Two consequences worth writing here rather than only in §5: the `wave` row's
"scrolling 240-px history in a dynamic tilemap" is precisely the **region canvas** §5.1 warns is
not an image budget, so a full-width history competes with the tiles the other scenes need; and
`deck`/`critter` are the first things in this application that would draw an OBJ for real —
legal, and the honest justification for revisiting §5.3's ruling that the controls stay tiles
until motion earns sprites. The ledger does not move: the `viz tick` row stays AP-6's, and a
scene change into or out of a viz during playback is a resident-scene switch (§5.1), not a
redraw.

---

## 9. The bake tools (A3a, A3b, A13)

Two entry points, one code path, and the ROM is a pure function of the config.

```
album.toml (TRACKED) ──► scripts/album_bake.py ──► per-track GAFP via gafp_pack.py --master --emit-pin
                                                  ├──► ALBM1 + ART1 + ALVZ1        (GENERATED, §2.3)
                                                  ├──► src/assets.rs (include_bytes! + pins)
                                                  ├──► cargo build --release --target thumbv4t-none-eabi
                                                  └──► agb-gbafix  ──► album-player.gba  (never tracked)
```

### 9.0 One tool, landed early: the packer lands in Phase 1, the art refusals at 2b, the rest of the verdict set in Phase 3

This is one tool, landed early at its minimum scope, and what moves between the phases is its
**refusal set**, not its existence: packing a declared track list is Phase 1's because nothing
downstream can decode N tracks without the `ALBM1` only a packer writes (§12.1's
one-implementation rule). **What moved, when the §11 split re-ruled the phases, is where the *art*
half of the verdict set lands: art is a 2b feature (A8), and an art box is declared in `[art]`, so
`[art]` and the **image-side** refusals — palette count, an off-palette source colour, an image
whose tile data exceeds what the variant can serve — land at 2b with AP-4, not at Phase 3.**
**(Corrected at the second review, 2026-11-20: that sentence was wrong in two of its five slots.**
The `tile_budget` *sum* and the "`[art] box` the variant does not derive" refusal are facts about the
**spec**, and the tool that reads the spec exists at 2a — so they are 2a's, exactly as §9's own 2a
list said, while this paragraph said 2b. The same paragraph and the same §9 sentence each counted
`>32 tracks` once, in two different phases. Neither is a cosmetic slip: which phase refuses something
decides which tool must exist for that phase to finish, and a 2a that refuses facts about images is
a 2a waiting on a tool 2b builds. The list below is now a **register** with an owner per row, and
every other refusal list in this file points at it instead of restating it.) The size/budget verdict, `[viz]`, `[ui]` and `[game]` stay Phase 3's. (Corrected 2026-11-25: `over-budget` and `over-track-count` moved to **2b PR-(1)** — §11's 2b note — because the tool-first ladder needs a per-album verdict to be the generator's output, not a later phase's; the `[viz]`-dependent rows still stay Phase 3's.)
(This corrects the section's own earlier story, which said the size-and-art verdicts both needed
sections "that Phase 3 introduces" — a leftover of the pre-split plan, and the kind of thing a phase
table quietly keeps asserting after the ruling that superseded it.)

| piece | **Phase 1** (AP-1) | **2b** (AP-4) for the art half / **Phase 3** (AP-5) for the rest |
|---|---|---|
| `scripts/album_bake.py` | exists, and carries the whole audio path: read the config's `[album]`/`[[track]]`/`[out]` subset → per track, `gafp_pack.py --master --emit-pin` → write `ALBM1` (the §4.3 internal map: `(offset, len)`, `start_sample`, `n_samples`, string offsets, the §6 reciprocal) → write `src/assets.rs` → link | *the same file*, grown: budget arithmetic, minutes-per-cart, `[art]`/`[viz]`/`[game]`, every remaining §9 refusal |
| `scripts/album_prep.py` | its rate half — 16,384 Hz stereo (much of which `gafp_pack.py` already does from the WAV header) and the tag read + ASCII fold (§5.5) | its album half — join the tracks into one continuous timeline, so `start_sample` is an album position rather than a file offset |
| `scripts/album_art.py` | not built | **2b**: built and refusing (§7) — and it exists already in probe form (§9's tool row), so 2b's art work is pointed at a cover, which this tree does not have (R15) |
| `scripts/album_viz.py` | not built | **Phase 3**, for Phase 4 (§8) |
| determinism | **pack twice, compare bytes** — nearly free once the packer exists, and it is what makes "generated" mean "regenerable" rather than "whatever is lying around" (§2.3) | two full `make album-rom` runs → byte-identical `.gba` |
| refusals that fire | declared rate ≠ product clock (Q14); frames outside the encode profile (inherited from `gafp_pack.py`); **`ALBM1`'s `n_samples` disagreeing with the packed container's `total_samples`** (§4.3 rule 1); the writer's own output re-read through the shipping `meta.rs` parser (§4.3's `audit_blob` rule); **and, from 2a PR-1, no fifth item: the title's fit left this refusal set when `TITLE_BOX_MAX` did, and is a measured verdict the bake prints (§5.5) — the count stays the four this row and AP-5's both name, and `title-fit` comes back as a refusal only with PR-(2)'s elision (A5, D9)** | **per the register below**, which owns this cell now. It no longer restates a list here — the restatement had drifted: the art-box and `tile_budget` refusals are 2a's, the image-side ones are 2b's, and `>32 tracks` was claimed by two phases at once |

**The refusal register — one row per refusal, with the tool and the phase that owns it (2026-11-20).**
This exists because the same list was kept in four places (§9.0's table cell, §9's prose, §11's phase
rows, §13's AP-3 row) and they disagreed: two refusals were double-booked, the palette-count refusal
was assigned to 2b here and to Phase 3 there, and the `[art]` config section was moved to 2b by this
section while §11's Phase 3 row still said it grew there. **The rule that decides ownership, stated
once: a phase may refuse only facts about input that phase's own tools can already read.** So 2a
refuses facts about the *spec* — including facts about a declared face, whose metrics a Python tool
can read out of a tracked `.ttf` — and never anything about an actual image, a real palette or a
rendered cover, because those belong to `album_art.py`, which is 2b's to build. **No phase may own a
refusal whose tool it does not build, and no phase's critical path may run through a tool a later
phase builds.** Each row is `id`-named so a test can assert on a refusal by name rather than by
quoting a message.

| id | the condition | which tool raises it | which phase builds that tool | where it is reported |
|---|---|---|---|---|
| `layout-variant-unknown` | `[layout] variant` names nothing in `layout_spec.toml` (D2's idiom, shipped) | `album_bake.py` reading config + spec | **2a** | bake log, named |
| `bands-not-tiling` | the declared band table does not tile exactly 20 tile rows, or a declared box falls outside its region | `album-layout-gen` | **2a** | bake log + AP-3 snapshot |
| `pitch-not-tileable` | an element's `row_px` is not a whole number of 8 px tile rows, or is below the declared face's line box at its declared raster (§5.0, §5.1's measured table) | `album-layout-gen`, reading the tracked face's line metrics | **2a** | bake log |
| `face-not-resolvable` | a declared face is not a tracked asset, or its licence rows are absent (§5.5) | `album-layout-gen` + the atlas it generates from the declared face | **2a** | bake log; and the variant stops being product-eligible, which AP-3 states out loud rather than skipping silently (§13) |
| `image-tiles-over-budget` | `image_tiles` + the text term (§5.1's upper **and** distinct bounds) + authored chrome art exceed the variant's declared `tile_budget` / `resident_scenes` (R14). **At 2a the image term is taken at the variant's derived ceiling — the largest box that variant derives — not at any album's selection, because there is no album-side box to read yet (§5.0)** | `album-layout-gen`, from declared fields + face metrics — no image required | **2a** | bake log, **both bounds printed**, and the ceiling the image term was taken at. **From 2b PR-(3) the row also reads `shares_image`:** a scene whose image *is* another scene's (the title card's cover vs the art scene's) declares the other scene, counts **0** extra image tiles, and prints `audited_as=shared(<scene>)` — one tracked cover shown by two scenes is one 256-tile term, not two, and the bake refuses a `shares_image` naming a scene with no art element or a box larger than that scene's derived ceiling |
| `art-box-not-derived` | `[art] box` is not one the selected variant derives — a spec fact, refused before any image is opened (§7) | `album-layout-gen` — it owns the derived box list, at 2a and before any image exists | **2a** for the validator and the derived list; **2b** for the `[art]` key the refusal reads (see the note below this table) | 2a: the derived box list and the pool term at that ceiling, printed in the bake log; **2b: the refusal itself**, the first time `[art]` is parsed at all — D2 keeps that whole section a named refusal through 2a |
| `scene-minimum` | a scene's own declared minimum cannot be met at the declared geometry (a list under ~4 visible rows at the declared pitch; an art box outside its region) — the minima are §5.2's scene table's, per scene, not a global number | `album-layout-gen` + §5.2's table | **2a** | bake log |
| `bar-cells-out-of-range` | declared `bar_cells` below §6's floor, or not a whole number of pixels per cell in the box the variant declares. The third range condition §6 names — a reciprocal that would not fit a `u32`, which needs `bar_cells ≥ n_samples` — is **refused at the track, not at the grid**, under the Phase-1 id `bar-reciprocal` (`album_bake.py:328`, and the self-test asks it at `n = 1`, `n = cells` and `n = cells + 1` for both variants); it is listed here because §6 states the three together, and a reader looking for the u32 case must find which id owns it | `album_bake.py` — it is the tool that computes `recip` | **2a** (the grid's two conditions); the u32 condition's id is **Phase 1**'s | bake log |
| `bar-cells-disagree` | declared `bar_cells` ≠ the grid the `ALBM1` reciprocals were computed for, or ≠ `layout.rs` | **at boot:** `meta.rs:595-596` (`Refusal::BarCellsMismatch`), with the reciprocal re-derived from the same const at `meta.rs:827-832` (called at `:721`) — i.e. on hardware; **in the bake:** `album_bake.py:2654` (`assert_bar_grid_agrees`), three artifacts asked at once | **2a — landed with PR-1.** The grid is `layout::BAR_CELLS`, generated; `nodiv::BAR_CELLS` is deleted and `bar_recip` takes the count as an argument (`crates/gba-playback/src/nodiv.rs:105`); the host legs are `tests/album_pack_host.rs:487-534` and `:597-606`. The boot refusal is **kept**, and it compares one number: a stale generated module that keeps the grid and changes anything else boots without a word (D9 states the limit) | bake log; the boot refusal stays as the backstop |
| `title-fit` | **a title the one-line elision still cannot render**: the box the selected variant declares for the element that draws titles is narrower than the protected prefix plus the ellipsis marker, at the declared face and raster (§5.5, D9). **An over-long title is not this row** — that case is measured, elided, and printed as a note, because it is an album's wording and not a build failure | `album_bake.py`: `LayoutFacts.title_render` (`:2368`) over `FaceMetrics.elide` (`:1642`), reached from the bake's title path at `:1073`. The runtime answer to the same question is the generated `glyphs::elide` returning empty-without-the-marker (§5.5a), and `no_declared_text_box_is_narrower_than_the_marker` in that module's suite is what keeps a shipped variant from ever asking for it | **2a — landed with PR-(2), 2026-11-22.** The measurement is PR-1's (`title_fit`, `:2352`, still what the printed note reports); the elision and this refusal are PR-(2)'s. The leg is `title_box_narrower_than_the_marker` in `--layout-self-test`: a fixture spec whose title element is narrowed to one tile column (8 px) against a marker that measures 12 px at the declared face. The **negative direction** is a leg beside it — no title, however long, is refused at a box this variant really declares — because a tool that refused every title would satisfy the first leg and mean nothing | bake log, named; AP-3 |
| `atlas-face-ambiguous` | the selected variant's text elements declare more than one face, which is more than one static atlas; §5.5a puts a second atlas outside the baseline (Q19), so this is a spec fact and not a request for a flag | `scripts/album_glyphs.py` (§5.5a's emitter), over the same `album_bake.layout_for` the layout generator uses — one spec reader, not two | **2a PR-(2)** | atlas gate log (`make album-glyphs-test`), named |
| `atlas-face-not-truetype` | a declared face has no `glyf` outlines to walk (a CFF or bitmap face), so §5.5a step 3's fill rule has nothing to apply to and drawing "half of it" would be a silent approximation | `scripts/album_glyphs.py` | **2a PR-(2)** | atlas gate log, named |
| `atlas-not-on-pixel-grid` | a contour coordinate or an `hmtx` advance does not land on a whole pixel at the declared raster — i.e. the face is being read at a size it was not cut for, which is the premise §5.5a step 3 states and step 5's measurement legs depend on | `scripts/album_glyphs.py` | **2a PR-(2)** | atlas gate log, named |
| `atlas-glyph-outside-cell` | a glyph's outline extends outside the one cell and line box the atlas gives it (an `hhea` triple smaller than the glyphs the face carries), so the atlas would clip it and the fit box would then describe a drawing that never happens | `scripts/album_glyphs.py` | **2a PR-(2)** | atlas gate log, named |
| `atlas-ink-not-matching-outline` | the pixels this tool filled do not have the bounding box the same outline's control points describe — the ±1 class of mistake in a stem or a counter. **This one is the tool's own bug path, not a fact about any input**, and it is printed as *named and not fired*: the comparison runs for every outlined glyph of the tracked cut, and a second font invented to trip it would be a second font in a tree that has enough of them | `scripts/album_glyphs.py` | **2a PR-(2)** | atlas gate log, printed as named-not-fired rather than hidden |
| `rate-bridge` / `frames-out-of-profile` / `n-samples-mismatch` | Q14's rate rule; frames outside the encode profile; `ALBM1`'s totals ≠ the packed container's | `album_bake.py` / `album_prep.py` / `gafp_pack.py` | **Phase 1, shipped** | bake log (AP-1, AP-5) |
| `art-palette-count` / `art-off-palette-source` / `art-image-tiles-exceed` | an actual image needs more palettes than §5.4 allocates, carries an off-palette source colour, or exceeds the variant's image term | `album_art.py` — the only tool that reads an image | **2b** (AP-4) | bake log |
| `over-budget` / `over-track-count` / `unused-viz-streams` / `audit-ceiling` | cart budget (album + art + viz + code reserve), the track ceiling, `ALVZ1` streams the chosen animation never reads, §10.4's 16-bit period ceiling (37.4 min at 16,384 Hz) | `album_bake.py` | **Phase 3** (A3b) for `unused-viz-streams` / `audit-ceiling`; **2b PR-(1)** for `over-budget` / `over-track-count`, re-homed 2026-11-25 (§11's 2b note: a generator that cannot say "this album does not fit" is not a budget tool, and the two `[viz]` rows keep their Phase-3 home because they read `[viz]`). `over-track-count` was claimed by both 2a and Phase 3 until the 2026-11-20 review. **Landed 2026-11-25:** `over-budget` is `code reserve + ALBM1 + art` refused before any byte is written, and `over-track-count` is the Phase-1 `track-count` refusal under its register name; both fire by id in `--layout-self-test` (`budget rows fired: 2 of 2`) | bake log (AP-5) |

Four refusals are Phase 1's and already fire (AP-5's row counts the same four — that count is
unchanged); **fifteen are 2a's** — ten about the spec and the layout it generates, five about the
glyph atlas §5.5a adds — and every one of them is checkable from the spec plus the tracked
face files: **no image, no art tool, no visualiser tool and no ROM among them.** That sentence *is*
the plan for 2a — it is what makes 2a schedulable on its own rather than waiting on 2b's tools, and
§11's note under the phase table is where it is stated to the phase table.

**Two of those fifteen cannot fire at 2a, and neither reason is a tool (the first was found
2026-11-20 while scoping 2a's first PR; the second is stated here rather than discovered in a
gate).** `art-box-not-derived` is the row above with a split owner. 2a builds
and runs the validator: `album-layout-gen` derives the legal box list, prints it, and takes the
`tile_budget` image term at the ceiling that list implies (§5.0). But the value it refuses —
`[art] box` — sits in `album.toml`, and D2 keeps that whole section a named `config-section`
refusal until 2b (A13's row, §11's note (iii)). Opening `[art]` at 2a for one key would need a
per-key allow-list in the config reader for a value nothing in 2a reads — no `album_art.py`, no
image, no art scene — and would make A13's "arrives with 2b" true and false in the same sentence.
So the ruling is: **the box list is 2a's, the box key is 2b's, and the pool sum is audited at the
variant's derived ceiling either way** — which is what stops 2b from being able to raise a budget
2a never refused against. Its practical consequence, written where it would otherwise be
rediscovered mid-gate: **`make album-layout-test` reports nine of the ten layout rows firing, plus
that one named and skipped with its reason**, and `make album-glyphs-test` reports four of the five
atlas ids firing, plus `atlas-ink-not-matching-outline` named and not fired — that one is the
rasteriser's own bug path, its comparison runs against every outlined glyph of the tracked cut on
every emit, and the fixture that would trip it is a second font in a tree that already has enough
of them. Both gates print what they did not fire, in the same "a green gate always says what it did
not look at" idiom AP-3 already uses for non-product variants. Nothing in the no-tooling-from-the-future rule
moves: no phase owns a refusal whose tool it does not build, and no phase's critical path runs
through a tool a later phase builds.

Two duties the split makes explicit, both Phase 1's: **(i)** land the `.gitignore` entries for
every generated album path, in the `master-pack` idiom (rule in the comment, not a bare
filename — see the header comment on the `*.gba` rule), and teach `make clean` the same paths;
**(ii)** keep `album.toml` — never the assets — as the tracked artifact a reviewer reads, so an
album is reviewable and rebuildable by someone who does not have this machine's `~/Music`.

| Tool | Job |
|---|---|
| `scripts/album_bake.py` | the driver: read config, resolve tracks, check budget, call the rest, write the generated Rust pin module, link the ROM. **Refuses before it writes** (see below). **Its Phase 1 subset is A3a (§9.0): pack a declared track list, nothing else — same file, grown in Phase 3, never rewritten.** |
| `scripts/album_prep.py` | ffmpeg → the product rate, stereo; joins the tracks into one continuous album timeline; reads tags → titles/artists and ASCII-folds them (§5.5). **This is where `ALBM1`'s `start_sample` / `n_samples` come from, and it is codec-blind: it produces PCM, not a bitstream** |
| `scripts/album_art.py` | image → `ART1` (median-cut + dither + tiles/map/palettes + palette-count refusal) — **exists in probe form, and the plan should say so rather than describing it as new**: `scripts/album_probe_art.py` (stdlib generator: palette from the colours actually used, 2×1 Bayer dither, `PALETTE_MAX` panic, 4bpp packing), `examples/album_probe/ART1.md` (a byte spec with a SHA-256 over a 5-art golden corpus, and a `C3` section that refuses out-of-range indices in both the index plane and the palette plane), `art1.rs` (strict little-endian reader, `validate()`, `decode_region()`), `art_pin.rs` (the byte/decode pins), and `art_probe.rs` (the ROM-side decode-then-paint legs). What is missing is a **real cover in this tree** (R15) and the declared-box path, not the machinery |
| `scripts/album_viz.py` | album PCM → `ALVZ1` (§8). Also codec-blind: it analyses the same prepared PCM whichever backend is going to be linked |
| `scripts/gafp_pack.py` *(shipped)* | per-track GAFP for the FLAC source — **the bake calls it once per track**, and it already performs the ffmpeg normalisation, the 16,384 Hz / 16-bit / 2048-frame rate-and-profile refusal (Q14), the `flac -d` differential and `--emit-pin` |
| *lossy encode step* | **not this track's to solve, and settled for v1 by the codec track:** the C encoder is still used **off-tree** to produce assets, nothing in this repository builds or vendors its C ([`ULI2.md`](ULI2.md) → risks 5–6). **Status 2026-11-19: an in-tree encoder has stopped being an unowned risk and become the codec track's last planned milestone** — requirements, three settled decisions, slices with exit gates and an acceptance question answered by measurement live in [`PLAN-OF-RECORD.md`](crates/uli-lite/PLAN-OF-RECORD.md) → "The encoder — the last missing piece"; **amended 2026-11-27: the slices and S8 landed, so the encoder is byte-exact; a `uli2` asset set still means running the C encoder with `GBA_AUDIO=` set (W2/R7, Q3) — not because no encoder exists, but because the asset targets and this app's path onto the in-repo one are not wired (W2/W3).** So *"an already-encoded asset plus a tracklist"* for that backend: `album-bake` takes the asset bytes, decodes nothing, and does the same manifest/art/viz/cost work as the lossless path. Phases 0–4 are demoable on the lossless backend alone; **Q3's encoder question is settled (S1–S8, byte-exact); W2/W3 are the remaining work.** |

**`album.toml` (the whole configuration, so A14 is a form over one file):**

```toml
[album]
title  = "Night Errors"
artist = "sila"
year   = 2026
source = "my-album/"          # or an explicit [[track]] list below

[out]
rom          = "album-player.gba"
backend = "flac"                # → `BACKEND=`: one backend per ROM, chosen at build time (§4.2b)
rate_hz      = 16384           # the packer's rate contract: identity or refusal (Q14)
cart_bytes   = 33_554_432      # 32 MiB default; a smaller cart is a budget, not an error

[art]
cover       = "cover.png"
mode        = "cover"          # none | cover | cover+breathe
box         = "120x120"        # must be one of the boxes the selected variant DERIVES (§7);
                               # an unlisted box is a refusal, not a resize
palette_max = 15
bit_depth   = 4                # 8 is legal at a real VRAM price; never at full-bleed (§7)
bleed       = false            # true = draw behind the bands; 600 tiles the regular pool cannot serve at all — a tile_budget/pool question, not a block one (§5.1, R14, §7)
dither      = "bayer4"

[viz]
streams = ["loud", "beat"]     # → ALVZ1
default = "bar"

[layout]
variant = "A"                  # a name in layout_spec.toml (§5.0). Band heights, element spans,
                               # bar cells, faces, image boxes and palette banks all come from that
                               # variant: this file selects, it does not geometry. An unknown name is
                               # a refusal before a line of Rust compiles, and `make album-preview
                               # LAYOUT=A` is how a variant gets looked at before it gets baked.

[game]
enabled = false                # §12.2; "pong" | "snake" when on

# [[track]]                        # explicit form, in play order
# file = "01 shellbound.flac"
# title = "Shellbound"
# artist = "sila"
```

**The bake step is a verdict** — in Phase 3 the whole set, in Phase 1 the four refusals
§9.0 names. It refuses, with a cited number, for: over budget
(album + art + viz + code reserve); more than 32 tracks; a declared rate that is
not the product clock (the `geometry.rs::RATE_BRIDGE` rule, which is FLAC
plan-of-record **Q14** — reinterpretation
is how the 2.4%-sharp defect got away); a track whose frames are outside the
encode profile; art needing more palettes than allocated; a title that overflows
its box; a `viz` stream the chosen animation does not consume; and a total album
length beyond what the shipped geometry can *audit* with its 16-bit period counter
(**37.4 minutes at 16,384 Hz, only ~18.7 at 32,768** — the `const` pin in
`geometry.rs` names the ceiling, and §10.4 owns widening it). **From 2a, the layout rows of §9.0's
register — `layout-variant-unknown`, `bands-not-tiling`, `pitch-not-tileable`,
`face-not-resolvable`, `image-tiles-over-budget`, `art-box-not-derived`, `scene-minimum`,
`bar-cells-out-of-range`, `bar-cells-disagree` — every one of them checkable from the spec plus the
tracked face files, with no image, no art tool and no ROM in sight (§9.0's rule for what a phase may
refuse, and the reason this paragraph no longer enumerates a count).** Of that list, one is a 2a
validator with a 2b input — `art-box-not-derived` reads `album.toml`'s `[art]`, which stays a named
`config-section` refusal until 2b — so that enumeration is a list of **ownership**, not a count of
what 2a's own instrument can fire: eight fire, the ninth is reported named-and-skipped with its
reason (§9.0's note under the refusal register, §11's note (iii)). The two `bar_cells` rows moved
to 2a because the *constant* moved: the same refusal that shipped as a boot-time header check
(`meta.rs:582-584` when this row was written) is now `meta.rs:595-596` against the generated
`layout::BAR_CELLS`, with `assert_bar_grid_agrees` (`album_bake.py:2654`) asking the declared spec,
the packed header and the generated module in one call, and §6 names where each of them lives. That last minimum
is not a global number — §5.2's scene table
carries it per scene, because the list refuses a region under ~4 rows ("where a list has
stopped being a list"), the art scene refuses a box outside its own region, and the `blank`
scene refuses nothing, which is what makes it 2a's.
Refusing is correct
behaviour until then: the app must not silently wrap the clock it measures itself
against. It
re-reads everything it wrote through the shipping parsers, and a second run on the
same inputs must produce a **byte-identical** `.gba` (a real gate, and a strict
improvement on today's reproducibility, since ROM header/entry/nintendo-logo
bytes are fixed by `agb-gbafix` at a pinned version). **The layout spec sits on the tracked
side of that line, deliberately:** it belongs to the application and not to an album, so it
is inventory in §2.3's instrument sense like the font subset and the fixture, while
`src/layout.rs` is build output like every other generated module in this paragraph —
pinned, `clean`ed, regenerated, and covered by the byte-identity claim below. Because nothing here is committed
(§2.3), that byte-identity claim — not a stored blob — *is* the evidence that the
generated assets mean what the config says they mean; its audio-only half is checked from
Phase 1.

### 9.0a The configuration model: two masters, three scopes, and the crate seam *(ruled 2026-11-26, 2b PR-(4))*

The data model, pinned here because 2b's art work is the first feature that touches all of it:

- **Two tracked masters, different scopes.** `layout_spec.toml` is the **variant library**: geometry, bands, elements, faces, art boxes, control slots, palette banks, and each scene's kind/phase/resident. It is reusable across albums and gated on its own (AP-3). `album.toml` is the **per-album master for one ROM**: metadata, the `[[track]]` list, backend/out, `[layout] variant` (a *name*), `[art]` (cover, mode, box, palette/dither/bleed), later `[viz]`/`[game]`. **Albums select names, never numbers**: if the album carried geometry, every album would become a new gate and AP-3's per-variant snapshots would stop meaning anything (§11's ladder rule ii).
- **Scene data vs scene geometry vs scene code.** *Data* is album-scoped: the art scene's image (`ART1`), the list's tracks (`ALBM1`), the card's metadata + counts + cover. *Geometry* is spec-scoped: band rows, element boxes, `row_px`, art boxes. *Code* is the app/crates. "Which scenes are included" has three separate meanings and they stay separate: **declared** (the spec's `SCENES`), **implemented** (the ROM's compile-time `SceneId` table, a filtered view), and **resident** (the audit flag). No per-album scene-set key is needed now; `[art] mode = "none"` simply leaves the art scene drawing nothing and the default scene on `blank`.
- **Three generated scopes, not one monolith.** `layout.rs` is **spec + face files only** — one variant, one module, album-independent, so AP-3 snapshots and `--emit-layout` compare like with like. `assets.rs` is **album-scoped**: `ALBM1`, the track pins, the `ART1` blob, and the **selected** image term (`POOL_IMAGE`, `POOL_DISTINCT`) with the album's compile-time asserts. `layout_report.txt` spans both. **Decision B, ruled here:** the image term is the album's selected box, not the variant's ceiling — a 120×120 or 96×96 album audits and asserts against what it ships, and the ceiling stays in `layout.rs` only as `POOL_UPPER` for the variant. A 96×96 selection is witnessed in the layout self-test; the fixture's 128×128 equals B's ceiling, so it does not exercise the difference by itself.
- **The crate seam (`gba-scene` + `gba-art`).** `crates/gba-scene` is the **vocabulary** a scene writes through: `Rect`, `Write`/`Writes`, `Source` (which tileset an index names), the screen-sized batch. `crates/gba-art` is its first **instance**: the `ART1` reader (lifted out of `examples/album_probe`, one implementation for the probe, the app and their gates) and `paint(art, rect, base_bank, &mut Writes)`. The app keeps the backgrounds, tilesets, commit and the compile-time scene registry. Guardrails: the sink is region-scoped and static, there is no `Scene` trait and no dynamic registry — §11's rule that a scene kind must be statically derivable and §16's "no general GUI toolkit" are what keep the bake able to audit a scene at all. `ART1` v1 is **ratified as the shipping format** rather than re-invented: every rule in it is a hardware fact, and the probe's spec and reader are the reference.
- **Deferred by name, not silently.** `[art] mode = "cover+breathe"` is a per-frame palette cycle and belongs to the visualiser's cadence (Phase 4/AP-6); `bit_depth = 8` is the side-by-side §7 rules a screen; `bleed = true` is legal in §7 but no variant here declares the tiles for it (the tool prints the occlusion report; the budget refuses); `[viz]`/`[game]` and per-album scene sets are later phases.

### 9.1 The Make surface this implies

Same naming discipline as the rest of the tree (`native-*` / `podman-*` twins, all
listed by `make help`, all gated through `toolchain-check`, all running `format`
as a pre-step and never gating on tests):

| Target | Does | Lands |
|---|---|---|
| `make album-bake ALBUM=album.toml` | runs the bake: assets + generated pin module, prints the size/budget/duration table, refuses what it cannot fit | **1** (track list → blobs → manifest); 3 (budget/art/viz verdicts) |
| `make album-rom ALBUM=album.toml` | `album-bake` → cargo → `agb-gbafix` → `album-player.gba` (this is §1's one command) | **1 — landed 2026-11-15 (PR-3a)**; `album-bake` is a prerequisite so a build cannot skip the pack step (R11) |
| `make album-player-rom` | the same bin **without** `deep-audit` — a *player* artifact: it plays the album and answers within ~1 s of the last sample, but does not verify decoded content, and says `pcm=SKIPPED, fifo=SKIPPED, assets=SKIPPED — built without deep-audit` plus a boot NOTE line so it can never be mistaken for the gate artifact | **1 (landed, PR-3a fix 3)** |
| `make album-seam-test` | AP-2/AP-9's host leg: the derived geometry against the shipped FLAC figures, every track's `ALBM1`-vs-pin agreement, the eight `AUDITED_MUTATIONS` refusals, `PcmSource` on both arms, and the shipped track folded to its **FIFO pins on the host** — plus a stub-only build that must link no decoder | **1 (landed, PR-3a)** — a name §9.1 did not plan; it is the seam's gate, and `album-test` is the ROM's |
| `make album-probe-rom` | the Phase 0 throwaway probes | 0 (landed) |
| `make album-test` | AP-1/AP-2: scripted-key playback ROM in headless mGBA + audio-capture diff | **1 — not landed by PR-3a** (no keys yet; deferred to PR-3b). Its audio-capture leg also has no tool today: the installed `mgba-test-runner` (agb v0.25.0) takes a ROM and nothing else, so AP-1's content witness is on-ROM — the ring ledger plus the pins PR-3a folds — and the sample-for-sample capture diff is owed where it is genuinely needed (AP-6/AP-8, §12.2) |
| `make album-layout-test` | AP-3: host snapshots and fit verdicts for **every product-eligible declared variant** (§5.0's `product` field; a variant whose declared face cannot be resolved is reported as **skipped, by name with its reason**, never silently omitted — §13's AP-3 rule) — band tiling, the derived box list, the `bar_cells` ↔ `ALBM1` reciprocals agreement, one-line elision, the row counts derived from `row_px` (§5.0), and the per-element/switch cost record — **plus, from 2a PR-(3), the surface's own host legs** (`tests/album_surface_host.rs`, floored at 18): the boot frame read back as a 30×20 composite (the band stack, the title, the clock, the state word, five icons in their slots, the bar's ten columns), §5.1's redraw rule as a write count (a still frame writes nothing; a track change writes one element), the clock bumping on whole seconds and restarting on a new visit, every bar fill position drawing its own pixel count, a painted sample position converting to a `BarCell`, the icon tiles packing left-pixel-low-nibble, the composite reading the tileset a write names, a full batch reporting its drops, the page fill covering every visible cell, the text layer falling through to the band art, and the drawn line never exceeding its box — **plus, from 2b PR-(1), the resident-set walk's legs and the two budget rows**: the per-scene `kind`/`resident` schema (a scene's own tiles derived from its kind, the shared atlas/chrome/bar terms counted once, `resident + projection` printed), `over-budget` and `over-track-count` fired by id with a negative direction each, and `examples/album_player/layout_report.txt` byte-identical across two `--emit-report` runs with a one-flipped-byte negative control, holding the per-scene terms, the ROM addends and the cycle terms (all tagged `arithmetic` until PR-(2)) | **2a, extended 2b PR-(1)** |
| `make album-glyphs-test` | **§5.5a's atlas gate, 2a PR-(2).** The generator's own self-test at a leg floor; four of §9.0's five `atlas-*` ids fired by name on fixture specs, the fifth printed as named-not-fired with its reason; the generated `src/glyphs.rs` byte-identical across two emits and equal to what is on disk, with one flipped pixel as the negative control for that comparison; the module compiling for the host so its `const _: () = assert!` legs run and tie it to `src/layout.rs` (one face, one raster, one line box, one outlined count); and that module's **own floored suite** — the bit-order leg proved non-vacuous by its mirror failing to match, one cell per glyph, agb's blank tile used by nothing but space, no slot outside the cut, `measure` against `tilemap_px`, and §5.5's elision re-run against the answers the bake's Python copy derived. **Host-only, and not a prerequisite of `album-rom`**: the ROM gets the atlas through `album-bake`, and a ROM build should not depend on a font library | **2a PR-(2)** — a name this file proposes rather than one §9.1 planned, and deliberately **separate from `album-layout-test`**: §5.5a's critical path runs through fontTools, and a geometry-only layout refusal has no business depending on one |
| `make album-preview LAYOUT=… TITLE=… ART=cover.png` | a **host composite** of one variant at 1× and 4× (ASCII + PNG), from the same code that bakes (§5.0, §12.1) — the tool that makes layout taste a config change instead of a rewrite, and the 1× image §5.6's legibility rule requires | **2a — the surface half landed 2026-11-23** (`album-preview-1x.png`, `-4x.png`, and a `.txt` grid beside each, from `src/preview.rs`: the same `Write` list the ROM's map takes, the same tile bytes both tilesets upload, the same palette table; `LAYOUT`/`TITLE`/`ART` arguments are 2b's, since 2a has no art and one variant's worth of text) — and at 2a it composites **the surface**: bands, chrome, control slots and the `blank` region, plus the declared art boxes and any `TITLE=` string as *host instruments* the 2a ROM never draws. That is deliberate, and it is the line §11's 2a note states: the preview checks the generated geometry, the fit boxes and the bands before any of them is on hardware, and a composite of region content 2a does not render is **an instrument, not a witness** — AP-3's variant snapshots are the geometry witness and AP-4's the art one |
| `make album-art-check` / `album-bake-check` / `album-viz-test` | AP-4 / AP-5 / AP-6 | 2 / 3 / 4 |
| `make album-web` | AP-8's local config server (never a build dependency) | 6 |
| **`make album-check`** | the app's aggregate, the way `flac-check` is the FLAC track's | grows with the phases |

`make clean` removes every generated album path (blobs, `ALBM1`, `ART1`, `ALVZ1`,
`src/assets.rs`, **`src/layout.rs` (2a)**, **`layout_report.txt` (2b PR-(1), the machine-readable form of the same derivation)**, `album-player.gba`) and keeps the tracked fixture
config and the tracked `layout_spec.toml` — the same division
the master and rate assets already obey (§2.3). A user's own album folder and its config are
never in the tree either; the thing tracked is a gate fixture, not an album.

---

## 10. Budgets — written down before anything is claimed

`PLAN-OF-RECORD.md`'s rule for U4 applies to the whole application: *"Start from
the measured gate total, add this tree's UI, feeder and animation budget, and
write the number down before the first real-time claim."* Here it is.

### 10.1 ROM (32 MiB = 33,554,432 B)

| item | budget | basis |
|---|---|---|
| code + tables + headers + metadata | **256 KiB** | a reserve, chosen as ~0.8% of the cart; the real number is read out of the linker map at AP-5 and will be far smaller. What matters is that it is *reserved before* the encoder sizes the audio, so a code growth never silently eats samples. **First real read-out, 2026-11-16 (PR-3a):** `album-play`'s audited image is `.text` 77,696 B — ~30% of the reserve, with no screen yet. (Its `.rodata` is dominated by the album asset itself, so the clean code-plus-data figure stays AP-5's linker-map job rather than being guessed by subtraction.) **§5.6's text renderer costs +11,516 B `.text` and +7,620 B `.rodata`** (96 rasterised glyphs) = +18,940 B per ROM, measured against a baseline build of the same tree; paid once and shared with Phase 2 |
| `ALBM1` | ~64 B + 32 B/track + strings | §4.3 |
| `ART1` | 7–20 KiB | §7 |
| `ALVZ1` | 33.75 B/s for `loud`+`beat` (~91 KB per 45 min) + 90 B/s for `bands` (~243 KB) | §8's arithmetic: 1 B per 30 Hz tick (4+4 bits), 1 bit per tick, 6 B per 15 Hz band-tick |
| audio | **the remainder, and the loser** | art/viz take from the audio budget by rule (§7) |

### 10.2 CPU (per audio period, in cycles at 2²⁴ Hz)

FLAC geometry: `PERIOD_CYCLES = 573,440` (34.18 ms); a 2,048-sample frame is
2,097,152 cycles = 3.657 periods.

| consumer | measured/allowed | status |
|---|---|---|
| FLAC decode, **the synthetic FIXED-0 perf fixture** | worst frame 398,255 (18.99%) in the product log; 445,558 c mean / 21.25% and 21.36% worst on hardware | measured, shipped — **and it is the wrong row for an album** (§10.2b) |
| FLAC decode, **real music** (C2's master run, hardware) | decode **527,339 c = 23.656%**; work/frame **32.208%**; worst frame **35.831%** | measured, shipped — **this is the album's line** (`flac-docs/measurements.md`) |
| feeder (convert+push+IRQ) | shipped figure in the product log; carried in | measured, shipped |
| **content fold on the push path** — FNV-1a-64 over the bytes as they are committed, 12,288 B/frame | **cannot live here: 553 underflows in 1,430 periods** | **measured 2026-11-15 (PR-3a, `album-play`, mGBA, one run — the outcome is the observation, not a per-byte cycle count).** Measured two days later by the same ROM's own cycle clock (§10.5): the two post-audio folds cost **561,887,676 c for 240 frames = ~2,341,000 c per 2,048-sample frame, against a 2,097,152 c frame budget — 112% of a frame in folding alone**, which is precisely the starvation observed. (For scale, FNV-1a-**32** over ROM bytes measured ~29 c/byte in the same build — the 64-bit fold over audio is an order of magnitude worse per byte, and that gap is worth knowing before anyone budgets a fold by intuition.) So the fold belongs off the audio deadline: `album-play` proves its pins in a post-audio re-decode, the same shape the product ROM already uses (`product_play_main.rs:1690-1712`). This is the application's **first non-audio cost line handed across to §10.2's ledger** (R1b) — and it is a *refusal*, not a budget |
| **boot audit** — `ALBM1` v2 structure fold + per-track 32 KiB/4 KiB content tripwire | **589 ms of CPU** (`parse` 25,477 + `open` 4,328,610 + `gate` 62,009 + `prefill` 5,461,122 c) | **measured 2026-11-16 (PR-3a fix 2, `CycleCounter`, mGBA — cycles, so it transfers to hardware, §10.5)**. Bounded by construction and guarded by a host test (`boot_audit_bytes < 1 MiB`), because the first cut of this row was **52.56 s** (R12) |
| **post-audio proof + asset audit** — cold re-decode, the two content folds, the full asset fold | **75 s of CPU** (proof 817,429,485 c + assets 439,233,721 c) | **measured, same run.** Off the audio deadline, so it buys with a waiting listener and a lamp rather than with a drop — but it is a real per-run cost for `make album-test` and for Phase 5's hardware sessions, where it is now gated by the `deep-audit` feature — `make album-player-rom` drops it and prints `SKIPPED` terms plus a boot NOTE line so a player artifact is never mistaken for the gate one (§11, fix 3) |
| UI: key read, position, bar + 2 text lines, palette fade | **1.89% of a period measured** (AP-0's `full` arm: 5,306 c/frame → 10,832 c of a 573,440 c period; ≤ 2% target met) | measured, shipped (AP-0) — **but the characterisation of the arm is corrected here, and it cuts both ways.** The probe's arms are *cumulative*: `bg` = one BG + text, `bg3` = three BGs with one scrolling, `obj` = `bg3` + an animated `DynamicSprite16`, `full` = `obj` + a palette fade every frame (`examples/album_probe/README.md`). So `full` was never "one composed text screen with a palette fade": it is **three backgrounds, a live sprite, a per-frame fade and text**, and it is properly read as (i) **an upper bound for a 2a frame**, which has two BGs, no OBJ, and element-addressed redraw (§5.1) instead of a whole-screen composition, and (ii) **a lower bound for a full player**, which adds art, viz, sprites and scene changes on top. Both halves are statements about arms rather than one number wearing two hats, and neither is measured — AP-3's render leg is what turns them into figures |
| **UI, per element — the unit is an element, not a row or a screen** (§5.1's redraw rule) | upper bounds from AP-0's measured **726 c per 4bpp tile write — which is the audio-*disarmed*, IME-off, cold-cache leg** (`examples/album_probe/README.md:169-176`). The same cells at the **live** figures the same probe reports (floor **1,096 c**, mean **1,242 c**, `:202-207`) are **1.5–1.7× larger, and that is the arm the audio deadline lives in**: a bar-cell change (§5.5: only when a cell changes; at most one per declared cell, §6) 1–2 tiles ≈ **0.7–1.5 k c disarmed / 1.1–2.6 k c live**; the clock row's *event*, **only the digits that changed and at most once a second**, 1–2 characters × the tile rows the line box spans = **2–4 tiles ≈ 1.5–3 k c / 2.5–5 k c** (a full 11-character clock redraw, which is what R1b's late run was doing every second, would be ~22 tiles ≈ **16 k c / 27 k c** — the difference between those two numbers *is* the changed-cells rule, and it is why this row is itemised); a title line, on a track change, **the declared box in cells: variant B's `title_text` is 20 tile columns × two tile rows = at most 40 tiles ≈ 29 k c disarmed / 44–50 k c live** (this row priced it as "28 characters (the box's limit, §5.5) × two tile rows ≈ 56 tiles ≈ 41 k c / 69 k c" until 2a PR-1 deleted that limit; the box is a 160 px span now, and the fill cost is the element's cell count — the same 40 tiles whether the string reaches 103 px or the full 160, which is the point of pricing cells and not characters) (§5.5's table; **re-cut at the 16 px product pitch on 2026-11-20 — every one of these figures previously assumed the 8 px instrument face's one-tile-row cell, which is why they were all understated by half**) | **re-priced by AP-3b in the live arm (2b PR-(2), 2026-11-25): a changed bar cell 489 c, a changed clock digit 963 c (two tile rows), one unchanged cell 407 c, a 600-cell region 550,437 c (917 c/cell); the rows above are these cells × the declared counts, and the generator's report tags each term `measured` or `arithmetic` accordingly. The old wording here — "arithmetic on AP-0's figure, not a measurement" — is kept as the record of what this row was before the probe existed.** They are itemised because the failure mode is already measured and is not hypothetical: R1b's hardware arm had **one 1 Hz text redraw** late on **104 of 16,553 periods**. A bar tick and a clock tick are different addends on different cadences, and a design that repaints a whole band to move one cell charges the cheap event at the expensive one's price. **The cadences are not free-floating: they come from §5.5's table and §5.2's rule, and nothing in 2a or 2b is frame-paced** — R1b's fix is a *changed-cell* redraw, not a faster one, and every row here has to name the cadence it is priced at or it is not a cost, it is a guess |
| **scene switch between resident scenes** | a `set_priority` write, with both scenes shown into every frame and the visible one winning on priority — **there is no per-BG hide/enable to write in agb 0.25.0**, and the frame mechanics (DISPCNT derived from what a frame showed) plus the exact API are in §5.1, which is where the old wording, "a layer enable/priority write", is corrected | "tens of cycles" is an **assumption**, not a figure, and it is precisely the number the residency ruling is buying — so **AP-3 measures the switch on ROM with both scenes mapped**, records which of §5.1's four residency options it was taken under (they differ by two orders of magnitude), and prices the per-frame `show()` of more than one background that this mechanism actually requires |
| **partial map rewrite** — a list scroll, a title line, any in-place content change | ≤ the element's cell count × **AP-0's per-tile figure, because agb has no cheaper path: `set_tile` *is* the map write** (allocation, refcount and the map entry in one call — §5.1's correction, with the source cited). **This row's old "~295 c/cell, by subtraction from AP-0's upload leg" is withdrawn**: that 66.5 k gap is the loop around 225 calls, not the price of a cell, and nothing in this tree measures a map-only write because agb has no map-only write to measure. So the honest arithmetic for a whole region (600 cells) is **0.44 M c disarmed, 0.66–0.75 M c live ≈ 1.7–2.8 lossy periods = 12–21× that period's whole spare margin** (13.73 points of 262,144 c ≈ 36.0 k c — the margin this row and §10.5 quote, so the comparison is against a named number and not a feeling), **not** the ~177 k c / 0.68 period this row used to claim, and it is **not an admissible per-press cost** — further from admissible than it was. What is cheaper: an unchanged cell, which `set_tile_at_pos` skips before it touches the map (`regular_background.rs:375-378`) — **measured by AP-3b L1 at 407 c against 489 c changed (83%, not free), because the skip happens after `add_tile`/`remove_tile` (`:360-373`)** — which is still the argument for element-addressed redraw | same arithmetic, **now known to be a floor rather than an upper bound**, since the real per-cell cost is `set_tile`'s whole call and AP-3 measures it per element in the live arm. This row still exists because a claim in review of this re-rule said a map rewrite was "tens of cycles" and AP-0's log already said otherwise — and the same review pass corrected *this* row's number, which is recorded in §5.1 rather than quietly fixed |
| viz tick (1–2 ROM reads + table + tiles/sprites) | **≤ 1% target, unmeasured** | AP-6 |
| **art / scene tile upload — never on a switch between resident scenes; only on an image's first show, and always sliced** | measured at 225 tiles: **346,057 c whole** (0.60 FLAC periods), per-tile mean **1,242 c** live, worst tile **6,998 c**. Scaled to a 600-tile full-bleed image: **~0.75–0.9 M c ≈ 1.3–1.6 FLAC periods ≈ 3–3.5 lossy periods ≈ 20–26× the margin one lossy period can spare** ⇒ paid out as ~26 periods ≈ **0.4 s** of the ring lead (§12.2) | **measured at 225 tiles (AP-0)**; the 600-tile row is arithmetic on that measurement and AP-4 measures it — including whether a 600-tile image can be placed at all (§5.1). Replaces this row's old wording, "≤ 1 period, off the audio path", which was both optimistic at 225 tiles (**≈9.6× one lossy period's spare margin**, so it was never *inside* a period's headroom) and silent about the only thing that makes it affordable: slicing. Note the shape of the row — the rule is about *slicing and residency*, not about keeping art small |
| game tick (pong/snake, ≤ 12 objects) | ≤ 3%, admitted by watermark only | AP-8 |
| **reserve** | **≥ 20%** | headroom for flash-cart fetch behaviour (§14 R3) and IRQ jitter. **This row is written against the lossless path and cannot be inherited by the lossy one: at today's gate total the entire space above the codec there is 13.73 points (mGBA, no UI, §10.5), and the lossy player's own hardware arms left ~4–5 points at the worst frame ([`ULI2.md`](ULI2.md) → U5). Phase 5's geometry follows the backend (§4.2b), and so does this line — it is re-struck per backend, not carried over** |

The claim that must survive is `underflows = 0` **with the UI, a scene changing, and a viz
enabled**, on hardware, not `underflows = 0` with the screen off.

**Caveat on every "arithmetic" row above (added at review, 2026-11-20).** The per-element,
per-switch and per-map rows in this table are single-tile-write figures from AP-0 multiplied by
cell counts — in one case by the wrong leg, which is why the map-rewrite row above changed. They
are enough to **rank** mechanisms (which is all §5.1, §5.5 and §7 use them for) and they are not
a measurement of this application's renderer, which no ROM has ever run: how much compute
bandwidth is actually left for rendering during playback, on each backend, is a number nobody in
this tree has. Before 2b commits to a scene set, **AP-3 owes one rough ROM leg for it** — one
probe arm, live audio, three measurements (an element redraw, a whole-region change, a priority
switch), printed as this section's rows print cycles (§10.2 — this sentence pointed at §10.5, which is the
pre-audio path and prints no cycle table, until 2026-11-20), and a sentence back here reporting what came out.
Until then this section's UI rows are an **ordering, not a budget**, and the ≥20% reserve row is
the thing that leg gets measured against rather than a ceiling already known to be clear.

### 10.5 The pre-audio path is a budget line like any other (measured 2026-11-16, PR-3a fix 2)

**Rule, stated because the first album ROM broke it and no gate could see it: verification cost may
never sit in front of the speaker.** A check whose cost is proportional to the *cart* belongs in the
bake tool, in a host gate, or after the audio — never between boot and the first DMA. The bound is
structural: boot may read O(album structure + a fixed per-track window), and nothing O(album bytes).

What happened: a person played `album-player.gba` and reported **55 s of silence, then 30 s of audio,
then silence and no verdict**. They were right, and the harness was blind to it — headless mGBA models
no ROM wait states (R3) and ran the entire image in ~4 s, so every gate in this PR was green while the
product was unusable. The ROM's own cycle clock, added in the same fix, says the pre-audio path cost
**881,869,357 c = 52.56 s**, and it reproduces the human's number to within their stopwatch. Two
causes, one symptom: `Album::parse` folded the whole 15,104,892-byte asset region (§4.3's
audit-the-blob discipline, applied at the wrong moment), and `backend::open` parsed the manifest a
second time, so the sweep ran **twice**. The shipped product ROM has never done this — its boot gate
is counts plus folds of the *decoded output* (`gafp_pin.rs`), and its asset is 38× smaller. This plan
priced none of it.

After (`ALBM1` v2 — `fnv_structure` at boot, `fnv_assets` as an explicit post-audio audit gate, plus a
bounded per-track 32 KiB/4 KiB tripwire so R11's stale blob stays loud for milliseconds):
**9,882,264 c = 589 ms**, with `boot_audit_bytes` asserted ` < 1 MiB` on the host so an O(cart) boot
cannot silently return. The scaling is honest and tracked: ~36 KiB per track, so a 32-track album is
~2.4 s and would need the windows shrunk — a Phase 3 budget decision, not a surprise.

Two things this bought that the plan should keep. **Cycles, not wall time:** mGBA headless ran this ROM
7.5× faster than real time, so every timing claim in the album line is made in cycles and divided by
16,777,216 in the log — the only unit that transfers to hardware. And **a ROM a human cannot read is a
ROM a human cannot verify**: PR-3a shipped with no display code by design, which turned a pass into
"silent forever" indistinguishable from a hang. It now paints the backdrop violet while working and
blue/red at the verdict, the shipped `report()` idiom, at a cost of one palette write.

**What AP-0's UI figure does and does not buy (accepted 2026-11-15).** The measured
1.89% is taken as the budget line for a **basic** UI: text drawn from a composed
background, one or two simple sprites, palette fades. That is what the probe built, that
is what Phases 1–2 need, and the figure is realistic for it. It is **not** a measurement of
richer rendering — per-frame tile rewrites, animated art, viz bars redrawn from a table,
sprite-heavy scenes — none of which the probe drew; §7's warning that a per-frame VRAM
rewrite is "a measured decision, not a free flourish" stands unmeasured. Those get their
own rows when they are built (AP-4, AP-6) and the budget iterates around them. Nobody
should read the ≤ 2% line as covering them, and equally nobody should hold Phase 1 for
numbers only a viz or an animation can produce.

**One consequence of the band model belongs here, because it is the point of the model
(2026-11-20).** A persistent chrome makes the chrome's cost *amortisable*: the same cells,
the same tiles and the same bank are on screen every frame, so a change is one element's
cell count rather than a screen's worth of compositing, and the per-element rows above are
directly measurable against it. The old plan's five screens had the opposite property — each
screen was a different composition, so every screen change was a full redraw and no cost line
survived a transition. This is also why §13 AP-3's records are per element and per switch:
a number attached to a screen cannot be reused by the next scene, and a number attached to an
element can.

And the app's lines above are **not** free against `100% − decode%`. The non-decode half of the
gate is the term that has moved, and it moved against us: the C reference put its **69.43%**
decode wall inside a **73.63%** gate total, i.e. **~4.2 points for everything that is not the
decoder**; the Rust arm's 2026-11-19 witness puts **74.27%** inside **86.27%**, i.e.
**31,457 cycles = 12.00 points** of convert + IRQ + DMA — ULI2's U4 attributes **7.80 points** of
the 12.6-point gap to C to that half alone, at **~2.9× C's cost**. So the arithmetic to plan
against is: **13.73 points** between this app's worst-frame budget and the period, of which the
app's own UI/viz/game addend is one term and the codec's non-decode half is another that is
**already spent** — closing that half is listed as still open in [`ULI2.md`](ULI2.md), and is not
this app's to do.
(That 69.4% is a *diagnostic*, not a target: the codec retired "match C" as an acceptance
criterion on 2026-11-15, and what it wrote down as a ceiling is the period, measured at the gate
total — corrected 2026-11-19, when this paragraph still read "works to ≤ ~85% of the period" as
if nothing had been measured since.)
**U4 is met (2026-11-19), and that ceiling is no longer a hypothetical either — it is a measured
margin with this application's cost as the term missing from it.** The gate total sits at
**86.27% worst / 76.53% mean with no UI in the arm**, and [`ULI2.md`](ULI2.md) §Gates states the
remaining question in exactly these words: *"Whether that margin survives the UI is the open
question."* So the cross-track debt runs one way, now with a number attached to it: the app owes
the codec track a measured UI + viz + game cost (AP-0, AP-6, AP-8), in the same units the ledger
uses, **against 13.73 points of worst-frame headroom** — AP-0's basic-UI line (1.89%) plus the
feeder (0.92%) is the only part of that addend that exists today, and the shell (Phase 2), art
(AP-4), viz (AP-6) and game (AP-8) lines are owed. **AP-0 has delivered the basic-UI line;
AP-6/AP-8 owe the viz and game lines.** Until then every percentage in the table above is a
target with no witness, which is why the status column's wording is deliberate. Two instruments must not be
subtracted from each other on the way to an answer: U4's 86.27% is **mGBA, WAITCNT 4/2, no UI**,
and the lossy player's 94.52%–95.89% worst is **hardware, 2026-11-16, with a 1 Hz text redraw and 104 late
re-arms** ([`ULI2.md`](ULI2.md) → U4, U5). They bracket the question; they are not a difference.
And the ≤ ~85% bar quoted here for years is the codec's *decode-wall* headroom target
([`RECORD.md`](crates/uli-lite/RECORD.md) → "Critical path to real time"), which is met on that
wall (74.27%); **the gate total is 1.27 points over it at the worst frame**, and what ULI2
actually decided to write down as a ceiling is the period, not 85%.

### 10.2b Which decode row an album is budgeted against (measured 2026-11-15)

An album's frames are **not** the frames the perf gate measured, and this is not a tuning
choice the app can make:

- `MEASURED_DECODE` (`scripts/gafp_pack.py`) holds **one row at the product block size** —
  `(2048, "fixed", 0)`, 21.25% mean / 21.36% worst, pinned to `spike_l0_frames.bin`'s hashes.
  FIXED orders 1–4 are **not measured at this target**.
- Real music contains **no FIXED-0 frames.** A 30 s mid-album slice encodes as
  `fixed1 ×261, fixed2 ×216, fixed3 ×3`; the head 9.9 s as `constant ×4, fixed1 ×102,
  fixed2 ×53, fixed3 ×1` — reproducing the census `flac-docs/measurements.md` records for the
  C2 asset exactly. And it cannot be forced: `-l 0` disables *LPC*, it does not pin libFLAC's
  FIXED order — the packer says so itself ("the product profile is FIXED-0-only, not merely
  FIXED-only"), which is why `master-pack` needs `--master` to wave real audio through the
  guard at all.
- LPC is not an escape either: `-l 4` on the same 30 s gives `lpc4 ×316` and the guard quotes
  **86.64% mean / 87.07% worst of the frame** for that shape (6a-7b, hardware) — over budget
  before any other stage exists, and it marks the blob NON-PRODUCT.
- The encode constraint costs essentially nothing at pack time: FIXED-only is 1,372,290 B for
  30 s against `-l 4`'s 1,347,782 B — **365.9 vs 359.4 kbit/s, 1.8%** — and everything at
  decode time: **527,339 c against 445,558 c, ~18% more cycles per frame**.

So Phases 1–4 budget the picture against **real music's** row: decode 23.656%, total work
32.208% of a period on hardware, leaving **≈68%** rather than the ≈81% the fixture implies —
which is also §8's number to use. What the app owes the codec track (Q16) is the day that row
stops being a whole-asset average.

### 10.3 RAM

| | total | committed to audio | left for the app |
|---|---|---|---|
| IWRAM 32 KiB | 32 KiB | measured with all synthesis tables placed as the C reference places them, and re-measured for U6 on 2026-11-19: **`.iwram` = 26,496 B of 32,768** (code + `HotState` 8,192 B + `ClassTables` 560 B + `q3`/`clz6` 148 B) against a **stack high-water of 948 B (bench) / 1,988 B (gate)** ⇒ **6,272 B unclaimed, ≈4,284 B after the stack** — in an arm that carries **no UI** ([`PLAN-OF-RECORD.md`](crates/uli-lite/PLAN-OF-RECORD.md) → "Current state"; U6 met). *(corrected 2026-11-19: this row quoted the 2026-11-15 read — 25,548 B placed, a ≈2,960 B stack budget, 6,196 B of headroom — which had itself already superseded PR B's "~22.6 KiB port + 15.0 KiB stack")* | workable, still map-sized, and **tighter than the figure this file carried**: UI code and tile staging are checked against the linker map, and `.iwram` growth is a gate failure (AP-7). U6's own wording is the binding condition — "with the UI and feeder linked" — so the number to the left is the codec's arm, not this app's, until the shell lands (Phase 2) |
| EWRAM 256 KiB | 256 KiB | FLAC ring 40,320 B; ULI2 state ~96 KiB | workable; VRAM mirrors + ledgers + tables here |
| VRAM 64 KiB | | BG tile data + maps + OBJ tile data | §5.1, split to be recorded in AP-0 |
| OAM 1 KiB | | 128 objects × 8 B (max) | cursor/sprites |

`U6` (memory) is a codec gate the app inherits and must re-run *with the UI
linked*: "IWRAM/EWRAM/stack stay inside the product budget **with the UI and
feeder linked**" is ULI2's own wording. And the priority there is already ruled, in
these words: **"audio first"** — the IWRAM budget goes to code, hot state and stack,
and **"UI affordances are the slack and may be dropped to buy audio headroom"**
([`PLAN-OF-RECORD.md`](crates/uli-lite/PLAN-OF-RECORD.md) → U6, decided 2026-11-14).
That is the frame this application accepts: no UI feature in this plan is worth an
`.iwram` byte the decoder needs. And the memory story has already been rewritten once: with
the stack measured rather than inherited, the plan of record's own words are that **"the
binding constraint was never the stack, it was a stale measurement"** — `iwram-tables`,
previously recorded as crashing, runs with all twins and pins green. So what this application
carries from U6 is the *method*, not a number: size against the linker map, and re-read the
ledger before quoting any figure from it. The app therefore adds the linker-map diff to its
gate set, so a 200-byte surprise in `.iwram` is a red test rather than a silent playback
failure.

### 10.4 The 16-bit period counter: internal bookkeeping, and what widening it costs

`geometry.rs` pins it: the product ROM's audio-time instrument is a **16-bit**
hardware counter — timer2, cascaded from timer1, one tick per mixer period — so it
rolls at `65,536 × 560 = 36.7 M` samples, i.e. **37.4 minutes at 16,384 Hz and
~18.7 at 32,768 Hz**. An album-length run crosses that line, which raises exactly
the right question: *does a user ever notice?*

**No.** Concretely:

- Playback does not stop, stutter or drift at the roll. The audio path is a timer
  reload plus a FIFO DMA; neither reads this counter.
- The ROM's own rule is to **refuse to divide any share by a clock that has
  wrapped** (`geometry.rs`, restated in the product log). So what happens after the
  roll is that the ROM stops *reporting* its duty-cycle measurement — it does not
  misbehave.
- The screen does not use it either. The UI's clock and bar are driven by
  `committed_samples`, a 32-bit album-sample count (§4.4), which at 32,768 Hz covers
  2³²/32,768 ≈ 36 hours. Track boundaries and durations come from the manifest
  (§4.3), in the same units. Nothing a user sees is 16-bit.

So this is an **instrument limitation, not a product limitation**, and it is worth
listing only because it decides how the app measures itself: an album-length run
could not be *audited* — so no real-time claim about one could be written down —
until the counter is wider.

**What widening it actually costs.** Two ways, and the cheap one is software:

| way | cost | drawback |
|---|---|---|
| **A u32 counter incremented in the period IRQ** (chosen) | one load/add/store per period: at 29.3 periods/s that is well under 0.01% of a 16.78 MHz core, plus **4 bytes** | the 4 bytes belong in `.iwram`, because the handler is the latency-critical code and EWRAM reads carry wait states — and IWRAM is this project's scarce RAM (§10.3). An aligned 32-bit read is atomic on ARM7TDMI, so the main loop needs **no** critical section to read it. |
| A hardware cascade into timer2→timer3 | no handler instructions at all | spends the one free timer channel — `product_play_main.rs` currently disables timer3 explicitly ("nothing in this ROM uses timer3") and `uli_bench` uses that pair for cycle counting, so it is a shared resource with a claim on it. Worse, reading a cascaded pair is **non-atomic**: that is precisely why the shipped ROM's `audio_cycles()` is a median-of-three retry loop, and widening the period counter this way adds a second torn-read problem to solve for no user-visible gain. |

**Ruled 2026-11-14 (accepted): option A — one `u32` in `.iwram`, incremented in the
period IRQ**, landed in Phase 1. Not because the roll is urgent (it is not
user-visible) but because the app wants *one* 32-bit session clock that the UI,
the audit and the game's admission rule all read, rather than two clocks with
different widths and different failure modes. The accepted price is stated plainly
so it is not rediscovered later: **4 bytes of `.iwram` and roughly one
load/add/store per 34 ms period**, in exchange for keeping timer3 free and adding no
torn-read handling. Against the plan of record's measured numbers — **6,196 B of `.iwram`
headroom** with every synthesis table in place (§10.3) — 4 bytes is not a claim on the budget
anyone is fighting
over, and it sits in the UI's own allocation, which is exactly where "UI affordances
are the slack" (§10.3) says such bytes belong.

Everything else in this subsection stays closed as bookkeeping: a plan should say
that rather than promote an internal constant into a headline.

---

## 11. Phases, each demoable, each with an exit gate

| Phase — what it ships, and the artefacts it produces | Exit gate |
|---|---|
| **0 — Probes.** Three throwaway ROMs and no product code: (a) UI spike — 3 BGs + 1 OBJ + text + palette fade, VBlank-paced, *with the shipped lossless feeder playing underneath*; (b) art spike — a PNG on screen from a hand-written `ART1`; (c) skip spike — fade/drop/re-prime against the real ring, measuring the silence gap and proving the re-prime costs no underrun | `make album-probe-rom` + logs; **AP-0**: the frame cost breakdown closes with UI terms, the VRAM/OBJ split is recorded, `SKIP_PRIME_CHUNKS` gets its first number, and the *targets in §10 are replaced by measurements*. **Landed 2026-11-14** (`examples/album_probe/README.md`; `make album-probe-art/-test/-rom`, `make album-probe`): UI full arm **1.89%** of a period (≤2% target, and a lower bound), feeder 0.92%, video period **280,896 c** (§4.5's ~281,000 confirmed), one 4bpp tile write 726 c, art upload **0.60 period** live (+2.08% over audio-disarmed, n=1, emulator — the same upload has now read three ways and AP-4 re-prices it), `late_max` 4 samples in the skip sweep's total, inside the 16-sample grace — and `SKIP_PRIME_CHUNKS` came back **blocked on the truncate ruling (§4.4, Q13b)**, which is the finding Phase 0 existed to produce |
| **1 — It builds an album, and it plays one.** A1–A4, A6, A10, **A3a**. In order: **§2.2's lift of the feeder out of `examples/flac_spike/` into project source, with `truncate()` (§4.4) landing on the lifted type** — Phase 1 may not `#[path]` a spike. **Landed 2026-11-15 as PR-1**: `crates/gba-playback` holds `playback` + `checksum`, `RingFeed::truncate()` is on the lifted type with four host tests, `make playback-test` is part of `make flac-check`, and the skip probe's `truncate` arm measures it (3–17 periods = 102.5–581.1 ms). `SKIP_PRIME_CHUNKS` remains un-chosen; then **the pack step (§2.3, §9.0): `examples/album_player/album.toml` (tracked) → `scripts/album_bake.py`'s Phase 1 subset → N `GAFP` blobs + `ALBM1` + `src/assets.rs`, all generated and gitignored**, default material N entries over the tracked reference master plus one short clip (Q15) so a fresh clone builds with no untracked input; then the app crate — `PcmSource` + `backend/{flac,stub}` (§4.2b), the app's derived `geometry.rs`, `meta.rs` (the validating `ALBM1` reader over the per-track ROM-location map), the fade→**truncate**→re-seek→silence-cover→re-prime→fade-in feeder (§4.4), the 32-bit session clock (§10.4, ruled), the no-divide arithmetic (§6), play/pause/next/prev/restart, and title + elapsed/remaining on screen. **The `stub` and both backend arms compile from here on, and the suite runs on `stub` + `flac`; `uli2` runs when U4/U5 land (Phase 5, AP-9).** **Corrected 2026-11-19: they have landed — U4 with 13.73 points of margin at the worst frame, U5 on hardware with 0 underruns (§2, [`ULI2.md`](ULI2.md) §Gates). What still keeps the `uli2` arm from *running* here is the asset path (W2, R7/Q3) and this app's own `src/backend/uli2.rs`/geometry (W3, Phase 5's work); the encoder itself is byte-exact (S8). The three-arm compile result and the host-side parity PR-3a actually got are unchanged by any of that.** | **AP-1**, **AP-2**, **AP-9** — and the phase that hands the codec track its UI-cost addend (§10.2, R1b) and its first per-track real-music census (§10.2b, Q16). Landing it also updates `README.md` → "Build Process" and the `gba-build-and-test` skill with the new `album-*` targets |
| **2a — The surface exists, and the album plays on it.** **A5** (the layout *module*: a tracked `layout_spec.toml` → generated `src/layout.rs`, §5.0), **A7** (the progress widget — **moved here from Phase 3**, see the note under this table), the **authored control art** for A2/A4's transport in declared slots (§5.3), A6's `elapsed / total` wording on the real layout, one **`blank` scene** plus the scene slot's narrow contract (§5.2), and element-addressed redraw with per-element cost records (§5.1, §10.2), **and the text *mechanism* — `measure`/`fit` over the declared face plus the static glyph atlas (§5.5a is its build spec: `scripts/album_glyphs.py`, generated `src/glyphs.rs`, `--check` and the module's own floored suite under `make album-glyphs-test` — its own target, not a leg of the layout gate, because §5.5a's critical path runs through fontTools and a geometry-only refusal must not; 94 glyphs × 2 tile rows at the 16 px cut, 6,016 B, one `TileSet`, from the tracked `.ttf`).** **The text mechanism landed 2026-11-22, and what it landed with is worth naming because §5.5 and §9.0 both pointed at it:** the elision marker is three ASCII periods, not U+2026 (§5.5a's cost argument, plus the tracked title face has no U+2026 in its cmap at all, which the layout gate prints); the rule exists twice — `FaceMetrics.elide`/`title_render` in the bake, `elide`/`fit` in the generated module — and the generated module's test suite re-runs cases whose expected answers the *Python* path derived, because a rule duplicated and never compared is two rules (§9.0's `title-fit` row, D9's disease); and `title-fit` is a refusal again, but only for a box that cannot render any title at all — an over-long title on a real box is elided and printed as a note, exactly as §5.5 has always said. **What this PR did not take is the one character bound still in product code**, `text.rs`'s `COLUMNS = 30` on the verdict line: the mechanism for replacing it exists, but the verdict row is not a declared element in `layout_spec.toml`, so there is no box to measure it against until `surface.rs` declares one — reassigned to PR-(3) with that reason, not left as a TODO (§5.5).** The 2a ROM draws no list, and 2a still owns it, because the pool audit 2a owns (`image-tiles-over-budget`) is arithmetic *about a text path*: a bound that only holds once 2b has built a renderer is 2b's refusal wearing 2a's name, and it would put the critical path of a 2a gate through a 2b tool (§9.0's rule) | **AP-3.** Deliberately **no album art, no track list, no title card, no viz**: the point of 2a is that a person can play the album *on the real layout*, and that the layout can then be changed by editing a config file instead of rewriting a module. If 2a does not make the ROM more playable on its own, it should not have been split off. **Two things this row has to say and did not (review, 2026-11-20), both written where they belong rather than in prose elsewhere:** *(i)* **2a is the layout and its mechanisms, not the rendering** — every declared field (geometry, band table, control slots, fit boxes, `image_tiles`, `tile_budget`) exists, is generated and is checked at 2a even where the 2a ROM draws none of it, and `album-preview` composes the unrendered ones as host instruments (§9.1's row, the note under this table). The line the split draws is about **what the ROM may draw**: 2a's ROM draws the chrome and `blank`, nothing else. *(ii)* **2a's boot state is option (c) of §5.2's three** — chrome + title band + `blank`, audio auto-starting exactly as the gate ROM does. Decided 2026-11-20 rather than left to the PR, because "a row that says nothing ships the smallest option by default" is how defaults become decisions without anyone making them. **Not (a)**, which shows a person nothing of the layout this phase exists to build and leaves the phase unable to claim it made the player more playable. **Not (b)**, which puts a key press before the first sample: **AP-1**'s term is "audio begins within 1.5 s of boot", measured on a run where nothing is pressed (§13's AP-1 row, 578–580 ms witnessed), and a phase that wants a press has to restate that claim in its own PR, not inherit it. **(c) keeps the term byte-identical** — same run, same key log, same measurement — while putting a title band, a clock and a control column in front of a human with none of 2b's content. **The title card stays 2b's scene** (same renderer as the list, same fit boxes as A6, and §5.2's rule is that a scene earns residency by an answer it gives), and **whether a press should ever precede audio is Q20's to answer in 2b — and the PR that answers it restates AP-1 in the same PR.** What 2a owes that future decision is mechanism only: §5.2's scene-slot contract, and a chrome + title band drawn before any sample plays, which is exactly what its switch leg (L3) is priced at. **AP-3b (§13) belongs to 2a, not 2b**: its three legs are the numbers §5.1's switch ordering and §10.2's element rows are waiting on, they need only chrome and a background (no art, no list), and the instrument is already ~80 % present in `examples/album_probe` (§5.1), so the leg costs one new probe bin rather than a new harness |
| **2b — The region has content in it.** **A8** (album art: the `ART1` path, `album_art.py`, §7's derived boxes and the `bleed` option), **A11** (the track-list scene), **A12** (the title-card scene), the remaining icon slots (`RESTART`, the paused badge — §5.3), and the scene registry made complete (§5.2's table) | **AP-4** — real fixtures for art and control art, the 8bpp alternative judged on a screen, and the tile-pool question (R14) that only `bleed` raises (§5.1, §7). **A2b's art leg has an input this tree does not have: no image file outside `target/`, and the only art anywhere in it is the synthetic ART1 card `scripts/album_probe_art.py` generates (R15)** — so 2b's art work starts by supplying a cover (or ruling which album's art this product is allowed to carry), not by writing `album_art.py` (§7, §9, §13 AP-4) — **that input now exists (R15's note, 2026-11-25), so the art PR starts by pointing `album_art.py` at it; 2b's own PR sequence is scoped in the note under this table (2026-11-25), is tool-first, and its first two PRs change no pixel** |
| **3 — The bake becomes a verdict.** A3b, A13: the **same** `scripts/album_bake.py` grows §9's remaining refusal set (palette count, title fit, unused viz streams, the §10.4 audit ceiling — budget and the 32-track cap moved to **2b PR-(1)**, 2026-11-25), plus `album_viz.py` and the `[viz]`/`[game]` config sections (`[art]` and `album_art.py` land at **2b**, §9.0 — this row said otherwise until 2026-11-20), and two full runs → one byte-identical `.gba` (its audio-only half has been checked since Phase 1) | **AP-5** |
| **4 — It moves.** A9: `ALVZ1` + `bar` + `skyline`, `beat`-driven sprite; art+breathe | **AP-6**. **And A9's sprite spends from a pool this file has never budgeted** (§5.1, read out of agb 2026-11-20): OBJ tile data is its own run-time-allocated 32 KiB region at `0x0601_0000` — **1,024 4bpp tiles / 512 at 8bpp** — with agb itself selecting hardware mapping mode 1 (`object.rs:30`), so sprites never contend with the BG pools but have a ceiling of their own; **AP-6's row declares and audits that ceiling** |
| **5 — Hardware + the second backend.** Both GBA units, two runs each, `none` vs each viz; then the album-length build on the lossy backend — **and the precondition named in this row for two years is met: U4 (real-time decode) at 86.27% worst on the gate total with 13.73 points of margin and zero starved periods, U5 (playback) met on a GBA SP with 0 underruns across three player ROMs, PR D shipped as `crates/uli-playback`** ([`ULI2.md`](ULI2.md) §Gates; corrected here the same day — this row read "U4 is not met (105% worst / 89% mean, 2026-11-15), so this waits on real-time decode with margin, then U5 … and PR D"). Phase 5's lossy half is therefore **app-side work and nothing else**: `src/backend/uli2.rs`, the derived 32,768 Hz / 512-sample geometry (§4.2b — that geometry is now testable rather than sketched), a `uli2` asset path through §9's encode indirection (a fresh-clone build is blocked on W2/R7/Q3, not on the encoder, which is byte-exact at S8), and a fresh set of measurements changing no screen and no tool — §4.2b is why that is an expectation rather than a hope. Worth knowing that the shape itself is already in this tree: `make uli-player-cross` embeds a **12-track, 43.5-minute, 167,222-frame, 33,088,280 B** album in one 32 MiB ROM (33,407,264 B of cart, 147,168 B of headroom) and served **167,232/167,232 periods with 0 underruns in mGBA** — the codec's player, not this app's, and that arm was measured 2026-11-16, before the decode pass that won today's margin, with its hardware run still asked for. **The cross-track index is [`PLAN-OF-RECORD.md`](crates/uli-lite/PLAN-OF-RECORD.md) → "Remaining ULI2 work — the four threads"; W3 is this row.** | **AP-7**, **AP-9** |
| **6 — Reach.** A14 web config, A15 toy game | **AP-8** |

**Why Phase 2 is split in two (ruled 2026-11-20, and the reason is in the row itself).**
The old row shipped "layout module, title card, now-playing, track list, pause menu,
progress bar, cover art" as one unit, which is a phase that cannot be *reviewed* — it is
nine changes of three different kinds (geometry, scenes, art) bundled so that a layout
decision waits on an art tool. Two facts decide the seam:

1. **Layout is the thing that must be reviewable before it is expensive.** Once geometry is
   generated from a tracked spec (§5.0), changing the layout is editing `layout_spec.toml`
   and re-running `make album-preview`; once it is hard-coded across five screen modules,
   every taste change is a rewrite. So 2a's deliverable is that *mechanism* plus a playable
   ROM on it — the smallest product that proves the mechanism is real rather than
   architectural.
2. **Scenes are additive on top of it and cannot break it.** A scene gets one rectangle and
   may not touch a chrome cell (§5.1, §5.2), so 2b's work cannot invalidate 2a's cost
   records; that boundary is the whole point of the surface/scene split, and it is why 2b
   can be sequenced by taste afterwards (list first or art first — Q20).

**What that split does *not* exclude, said plainly because the rows above are otherwise
misreadable (review, 2026-11-20).** 2a is the **data layout and its mechanisms**, and it is not
a phase without decision instruments: every declared field of the spec — variant geometry, the
band table, control slots, the derived fit boxes (§5.5), `image_tiles` (§5.0) and the new
per-variant `tile_budget` (§5.1) — exists, is generated and is checked at 2a **even where the 2a
ROM renders none of it**, and `make album-preview` composes those unrendered fields (`LAYOUT=`,
`TITLE=`, an art box) as host instruments (§9.1's row). That is not scope creep into 2b: the
object under review in 2a *is* the geometry and its refusal set, and a tool that cannot express
the case it is checking has not checked it. **The line the split draws is about what the ROM may
draw** — 2a's ROM draws the chrome and `blank` and nothing else — and a host composite of region
content 2a never renders is **an instrument, not a witness**: AP-3's variant snapshots are the
cartridge witness for geometry, AP-4's for art. Reading the rows as "2a has no layout
information until 2b renders it" is the misreading this paragraph exists to prevent.

**And what 2a may *refuse*: the no-tooling-from-the-future rule (2026-11-20).** 2a's exit gate is
AP-3, so 2a has to be able to fail a build on its own instrument — which means every refusal inside
that gate must be a fact about input 2a's own tools can already read: `layout_spec.toml` and the
tracked face files. §9.0's register is built so that this is true; its nine 2a rows name no image,
no art tool, no visualiser tool and no ROM — **and one of those nine, `art-box-not-derived`, names
no image but still cannot fire at 2a, because its input is `album.toml`'s `[art]`, a section D2
keeps refused until 2b. That exception, its ruling and its consequences are §9.0's note under its
register; the short form for this row is that the box *list* is 2a's, the box *key* is 2b's, and
AP-3's bake-leg check reports eight firing plus one named-and-skipped.** Two consequences for the
phase rows above. **(i)** **2a
settles the list's row count even though the list scene is 2b's**: `row_px` and the face are 2a's
fields, so the window §5.2's list gets is derived and refused at 2a, and 2b builds a scene into a
rectangle whose size is already a settled number. **(ii)** The only *code* change 2a owned in order
for its refusals to be its own was making the bar grid variant-derived instead of a shared constant,
and **2a PR-1 landed it**: `BAR_CELLS` is gone from `crates/gba-playback/src/nodiv.rs` (it sat at
`:44` when this note was written) and `bar_recip` takes the count as an argument (`nodiv.rs:105`);
the boot-time `Refusal::BarCellsMismatch` is now `examples/album_player/src/meta.rs:595-596`, its
reciprocal check `:827-832` (called at `:721`), and the guard tests are
`examples/album_player/tests/album_pack_host.rs:487-534` and `:597-606` (§6). The consequence this
note was written to warn about — a variant declaring anything other than 32 cells making the ROM
refuse its own album, a refusal aimed at the hardware instead of at the spec — is closed: variant B
carries 80 cells and the shipped ROM plays it, and the refusal now fires from the bake and the
spec's own pre-flight instead. **(iii)** **`[art]` is not opened part-way (2026-11-20, scoping 2a's first PR).** 2a does not
accept `[art]` with one key made legal; it keeps refusing the section exactly as D2 does, and
`art-box-not-derived` is raised by 2b's parser against the derived list 2a generates into
`layout.rs`. See §9.0's note under the refusal register for the ruling and §5.0 for the pool-sum
consequence (the image term is taken at the variant's derived ceiling until an album can select
below it). The two-sentence version of the rule, so it is not re-argued per PR: **the box list is
2a's, the box key is 2b's, and the pool sum is audited at the variant's ceiling either way.**

**2a's PR sequence, scoped 2026-11-20 so this row is not re-derived every session.** Three PRs,
each independently reviewable, in this order. **(1) The layout spec and its generator:**
`layout_spec.toml` declaring A and B with §5.0's `product` flags; `album-layout-gen` inside the
existing `scripts/album_bake.py` (no new build target for it — `make album-bake` already runs it,
§5.0's step 5); generated, gitignored `src/layout.rs`, with the `.gitignore` and `make clean`
entries §9.0's duty (i) asks for; `make album-layout-test`; the eight fireable register refusals
asserted by `id` (nine now — `title-fit` returns with PR-(2), below); the derived box list and the `tile_budget` sum at the variant's ceiling; D9
discharged — the fit box and the bar grid became variant-derived across
four sites (`scripts/album_prep.py:48`, `examples/album_player/tests/album_pack_host.rs:241`,
`crates/gba-playback/src/nodiv.rs:44` and `examples/album_player/src/meta.rs:582-584,811-814`, as
those files stood when this row was written; each one's fate is in D9 below),
with the boot-time refusal **kept** as the backstop (§6, §9.0's `bar-cells-disagree` row); and the
`pyftsubset` ASCII cut of the declared face committed under `assets/fonts/` (§5.5a step 1 —
without it a product-eligible variant's face is not resolvable, and `face-not-resolvable` turns
every fit and pool leg into a *skipped* row instead of a number). **What it actually cost, as
against what this row expected (measured on the landed branch, 2026-11-21).** It draws nothing
new, and it does not leave boot behaviour byte-identical: the ROM prints one more line before
audio (`[album-rom] layout=B bar_cells=80 …`, `album-rom-run.log:4`) and the audited image moved
`.text` 96,604 → 96,764 B (`flac-docs/measurements.md`). Boot-to-audio on that same run reads
`parse=25,203 open=4,343,040 gate=86,781 prefill=5,022,982 total=9,483,307 c` = **565 ms** against
the 9,744,862 c = 580 ms §5.2's instrument paragraph records for the same bin — a difference that
sits almost wholly in `prefill`, the one leg that waits on the emulator's audio clock and so was
never a repeatable cycle count. So the honest term is the one AP-1 states: audio begins well
inside 1.5 s, on a run where nothing is pressed. And the re-baseline this row expected arrived
*differently*: `ALBM1` moved — its `bar_cells` word is 80 where it was 32, and every reciprocal
with it, including the copies of those reciprocals pinned in `src/assets.rs`'s `TrackPin` table (that
module's own `/// §6's progress reciprocal, floor(2^32 × 32 / n_samples)` comment said 32 beside
values computed on 80 until this row's review fixed it to the declared grid). No *content* pin is a
function of the grid — frames, blob sha256, the FNV folds are the audio's own facts. And the ROM
re-baseline was recorded as section
sizes for one image rather than as §2.2's table of bytes, sections and `sha256` per image — see
the note under that entry, which is where the gap is stated instead of being papered over. **(2) The
mechanism — landed 2026-11-22:** `scripts/album_glyphs.py` → generated `src/glyphs.rs`,
`measure`/`fit`/one-line elision, `album_glyphs.py --check`, and AP-3's fit legs (§5.5a) — all of
it, plus the gate §5.5a step 5 asks for (`make album-glyphs-test`, a separate target for the reason
that row gives), plus the §9.0 rows and the `title-fit` refusal that came with the elision. What
building it turned up and belongs to (3): `text.rs`'s `COLUMNS = 30` cannot be replaced by a
measured fit until the verdict row is a declared element with a box (§5.5) — and (3) then found
that even with a box it should not be, which is item 1 below. **(3) The surface —
landed 2026-11-23, except for the two items named below:**
`surface.rs`,
`chrome.rs`, the `blank` scene and §5.2's scene contract, boot option (c) as ruled above, and the
surface's own host legs inside `make album-layout-test` (the boot frame, the redraw rule, the
clock, the bar, the line cut, the tiles, the palettes, the two rectangles). `layout=<id>` on the
boot line was already printed from PR-(1), so this PR owed the surface's facts beside it rather
than the id again, and prints them. Nothing in that order
defers a gate: AP-3's legs are host-side and landed with (1) and (2), and the register rows that do
not fire at 2a are the two §9.0's note under its register names — each printed by the gate that
would have fired it, with its reason.

**Two items in that list did not land, and each is a ruling rather than an omission.**

1. **`text.rs`'s `COLUMNS = 30` stays, and this row's earlier promise to convert it was wrong about
   the cost.** PR-(2) reassigned it here on the grounds that `surface.rs` would declare the verdict
   row as an element with a box. Looking at it from the drawing side, both routes are worse than
   leaving it: the verdict takes the screen *after* the audio has stopped (§5.2's takeover rule) and
   draws in the **fusion8** instrument face, so declaring it as an element of the product variant
   would either make the variant declare **two faces** — which §5.5a refuses as
   `atlas-face-ambiguous`, because the atlas is one face per variant — or move the verdict onto the
   product face, which re-baselines **AP-7a's witnessed** legibility of `PASSED` on fusion8. It is
   a bound on a face this phase does not draw from, in a takeover this phase does not own, so it
   stays a measured cell count with its own line of explanation (`text.rs:65`, cited in
   `--layout-self-test`'s banned-name leg as a *different* bound). The honest version of the
   promise: **the mechanism now exists for every declared element; the verdict row is not one.**
2. **AP-3b's fourth probe bin did not land with the surface, and has since landed as 2b PR-(2)
   (2026-11-25).** Its legs need a ROM measurement harness (`examples/album_probe`'s fourth and
   fifth bins) and measure §10.2's element, region and switch costs — a *measurement* PR rather
   than the drawing PR this one is. The surface they were waiting for is on screen and playable,
   and PR-(2) plus AP-3c's arms have now taken all the numbers (§11's 2b note, §13's rows). Stated
   here because "landed with the surface" is what this row said, and a row that moves a dependency
   should say it moved.

**2b's PR sequence, scoped 2026-11-25 so this row is not re-derived every session.** Seven PRs,
each independently reviewable, in this order. It is a **tool-first** ladder: the first two change
no pixel, because the artifact this phase has to leave behind is the *spec → `layout.rs` → budget
report* pipeline, and 2b's three scenes are that pipeline's first three consumers. The order
**answers Q20 (i)**; Q20 (ii) and (iii) land in PR-(3), where the registry order and the
mid-album meaning of a card are decided. **Four things make a second album a config file rather
than a review, and none of them exists today**: per-scene residency (`resident_scenes` is a count,
`layout_spec.toml:99,347`, checked against the cap at `album_bake.py:2322`); a closed scene-kind
vocabulary (a kind is a Rust function in `surface.rs`, not a declared term); a resident-set pool
audit (the sum is one image-at-ceiling plus one text bound, `album_bake.py:2300-2335`, B's
`462/512`); and a per-album budget verdict (`over-budget` is Phase 3's today, §9.0; (1) is where it moves). Each is a fact
about the *spec*, which is why they are (1)'s and not a scene's. (After PR-(1) all four exist; the list is kept as the account of what the ladder had to build, not as a still-open checklist.)

**(1) The schema, the residency walk, and the report — no new pixels.** `layout_spec.toml` gains
a per-scene `kind` (`fill` / `text` / `art` / `composite`) and `resident`, and per-kind tile terms
the audit can sum; `album-layout-gen` walks the resident set instead of assuming one scene's
terms, and emits **one derivation, two renderings** — the existing prose plus a machine-readable
report (pool slots per scene, distinct and upper bounds, ROM addends for audio assets / art / code
reserve, and a cycles estimate whose every term is labelled `measured` or `arithmetic`).
`over-budget` and `over-track-count` are **re-homed here from Phase 3** (§9.0's register row is
amended in this PR: a generator that cannot say "this album does not fit" is not a budget tool),
while `unused-viz-streams` and `audit-ceiling` stay Phase 3's because they read `[viz]`. Gate: the
spec self-tests plus a leg that the report is byte-identical across two runs. Must not re-baseline:
`layout.rs`'s existing constants, AP-3's snapshots, the boot log, or any cycle figure — they stay
tagged arithmetic until (2).

**Landed 2026-11-25.** What the PR did, in the order it did it: `[[variant]]` lost its
`resident_scenes` count (a count cannot say *which* scene holds tiles), and every `[[scene]]` now
declares a required `kind` (`fill` / `text` / `art` / `composite`, the closed
vocabulary `SCENE_KINDS` in `scripts/album_bake.py`) and `resident`; the audit became a walk over
those terms — `resident + projection`, where a scene's *own* tiles are its kind's term and the
glyph atlas, the chrome art and the bar's pattern tiles stay the variant's shared terms, counted
once; the report is the sorted `key=value` rendering (`examples/album_player/layout_report.txt`,
gitignored build output) beside the existing prose; and `over-budget` / `over-track-count` fire by
their §9.0 ids (`over-track-count` is the Phase-1 `track-count` refusal renamed at last). **The
residency flags at 2a are `blank = true` and the other three `false`, deliberately**: 2a's ROM
holds no art or list scene, so claiming one resident would be a claim about a ROM that does not
exist. Their terms are audited as the projection, which keeps the pool sum at **B's 462/512 and
A's 425/512 — the generated `layout.rs` is byte-identical before and after this PR**, and that is
the no-re-baseline leg, not a slogan. When PR-(4) lands the art scene it moves from projection to
resident and changes no total; the same for PR-(6)'s list. Two things were not invented to fill
gaps: the scene switch is **named as excluded** from the cycle estimate (§5.1 calls "tens of
cycles" an assumption, and PR-(2)'s L3 is the leg that replaces it), and the ROM addends are
`not computed` in a standalone report because there is no album in that invocation — the bake is
where `over-budget` is asked and refused.

**(2) The render probe bin — landed 2026-11-25, and widened to carry AP-3c.** Two bins, not one,
because the two questions §5.7 separates are decided by the same harness and the user's constraint
was cost-first: `render_probe` (AP-3b's L1–L4) and `present_probe` (AP-3c's arms A/B/C). The
planned scope is below, and the landed note follows it.

`examples/album_probe`'s fourth bin: per-element redraw, a ~600-cell region change, a
`set_priority` switch with both backgrounds resident, and the pool leg that stands up art + chrome
+ list at the declared pitch and reads agb's live allocation — a repeated glyph's tile must carry
a refcount > 1, or the atlas silently fell back to per-cell allocation and a total-only check
would pass — then over-fills past `tile_budget` to show the bake refusal firing before R14's panic
exists. What this PR adds that the row did not ask for: the measured numbers are **checked into
the generator as coefficients with provenance**, so (1)'s report stops saying `arithmetic` for the
terms this bin measured. Gate: `make album-probe` prints `[album-probe] render …`; live-audio legs
inside §10.2's ≥20 % reserve; the unchanged-content leg near-free; the over-fill leg refuses at
the bake. Must not re-baseline: the product ROM (no product change) or AP-1's boot term.

**Landed 2026-11-25.** `examples/album_probe/src/bin/render_probe.rs` and
`present_probe.rs`, the shared `src/render_hw.rs`, two `[[bin]]` entries and the Makefile's
named-leg greps; `make album-probe` runs five images and checks the render and presentation logs
for their legs by name rather than only for a PASS line. **No product pixel changed**, `layout.rs`
is byte-identical, and AP-1's boot term was not touched. The measured answers, with the derivation
in `examples/album_probe/README.md`'s two new sections:

- **L1** (live audio): a changed bar cell **489 c** (min 480 / max 741); a changed clock digit
  **963 c** for two tile rows; an *unchanged* cell **407 c**. **The plan's "near-free" expectation
  is refuted**: an unchanged `set_tile` is **83%** of a changed one, because
  `set_tile_at_pos` adds the new tile's reference and removes the old one *before* it compares the
  map entries (`regular_background.rs:360-378`). The changed-cells rule still pays — two changed
  clock cells (963 c) against an 11-cell full redraw (≈5,291 c) — but its saving is the map store
  and the dirty mark, not the allocator.
- **L2**: a 600-cell region repaint is **550,437 c** mean (min 536,080 / max 556,701), **917 c per
  cell** — **0.96 FLAC periods, 2.10 lossy periods, 1.96 video frames**. §5.1's 1.7–2.8-lossy-period
  arithmetic lands inside that band; "no whole-region rewrite during playback" is now measured.
- **L3**: the `set_priority` store is **73 c** (two calls) — §5.1's "tens of cycles" was right about
  the write — and the *switch* is the commit that makes it land: **212,386 c** mean, **280,360 c**
  worst (one video period). The priority words were read back from `BGxCNT` on every flip
  (`landed=8` of 8), so the "lands at the next `show()`" claim is witnessed.
- **L4**: the art + atlas + chrome stand-up is **856 cell references to 460 live slots**, 189 slots
  shared, one glyph slot referenced **45 times** — the shared-atlas dedupe proved from the map, not
  from a total. A per-cell fallback would read `distinct == refs` (856) and overflow 512, which is
  exactly the failure a total-only check calls a pass. The hidden scene's 480 map entries and their
  tile words survive the switches (`resolves=yes`).
- **AP-3c**: arm A (three BGs, event commit) is **clean** — commit mean 12,222 c (2.1% of a period,
  agb's fast path, no halt), 87 periods, `late_max=0`, zero underflows. So the 1 Hz event commit is
  **not** a producer risk on this instrument, §5.7's "otherwise it waits for 2b" applies, and 2b
  PR-(5) is the implementation PR rather than an emergency. Arm B (OBJ chrome, no commit) is +1.1%
  of A's pass at this cadence (nothing to win at 1 Hz); arm C (60 Hz synthetic load) is **+24.8%**
  of A's pass, 12,686 c/frame in the main loop plus 2,026 c/frame in the VBlank handler (~5.2% of a
  video frame together), still zero underflows and zero lateness. Those are the numbers 2b PR-(5)
  and AP-6 build against.

**What it did not do, by ruling:** the over-fill leg is the bake's
(`image-tiles-over-budget` / `tile_budget` refusal, already fired by `--layout-self-test`), not a
panic hunt on the ROM — the ROM's job is the stand-up that fits; and the probe's numbers are
**mGBA** numbers, with the hardware leg still AP-7a's (§13 AP-3b's own wording).

**(3) The title card (A12), as the schema's first consumer — scoped 2026-11-25 with the budget
finding the PR must resolve first.** The card's declared art element makes the audit's
`projection` term sum *two* images (art 256 + card 256) and refuses B at 718/512, even though both
scenes draw the same tracked cover and therefore the same `(tileset, index)` keys. The resolution is
a scene field, **`shares_image = "<scene>"`**: the card declares it, its own image term is 0 (`audited_as=shared(art)`),
the bake validates that the named scene has an art element and that the card's box is no larger than
that scene's derived ceiling, and the projection sum stops double-counting one asset. That field and
its refusal are this PR's first commit; the alternative — making `projection` a max over resident
sets — was rejected because it would hide a genuinely different second image rather than state the
sharing. The text spans (album title / artist / year, `N tracks · M minutes`, and the prompt — or its
absence, below) plus that art element, as `layout_spec.toml` entries; the scene is `resident = false` because the
cap is list + art + `blank` (§5.1's cap of 3; the spec's own comment says so at
`layout_spec.toml:347`), so its setup is paid at an event, and at boot that event is before audio.

**Landed 2026-11-25.** What shipped, and what it changed:

- **`shares_image = "art"` on the card scene** — the budget finding above, resolved as a declared
  share rather than a looser sum. The card's `card_art` element declares a 10×10 box (a derived box
  in `art_boxes`), its own image term is **0**, and the report prints
  `scene_title_card_audited_as=shared(art)`. B's audit is **unchanged at 462/512**; the refusal for a
  share naming nothing (or a box larger than the named scene's ceiling) is `spec` /
  `art-box-not-derived`, and the self-test watches the former fire (schema legs 4 of 4).
- **The card's spans, in the tracked spec, with no new code path for text**: `card_title`,
  `card_artist`, `card_year`, `card_counts` (19 cells × two 16 px tile rows each) and `card_art`.
  The counts line is **ASCII** — `3 TRACKS - 10 MIN` — because §5.5a's cut has no middle dot; that is
  the same finding 2a PR-(2) made about U+2026, and it is recorded in the spec rather than
  rediscovered as a blank glyph. Minutes round **up** (9:37 is a ten-minute card).
- **The checked registry**: `layout.rs` now emits `SCENES: [SceneDecl; N]` in the spec's order, and
  `surface::SceneId::IMPLEMENTED` is a filtered view of it (the card and `blank`; `art` and `list`
  stay declared at 2b). A host leg proves every implemented id exists in the declaration and keeps
  its relative order, so `Select`'s cycle cannot drift from the spec; the ROM's boot line prints
  `scenes=2 of 4 declared boot=blank card=declared`.
- **`Select` is bound** (`Action::SelectScene`, the map's last row so a transport press on the same
  frame wins). The press **paints on its own pass** rather than waiting for the clock latch — a card
  up to a second late is not a card — and that pass's one commit suppresses the chrome refresh, so
  §5.7's at-most-one-commit invariant holds. `make album-card-test` is the runtime witness: the gate
  script plus one `Select` edge at frames 90/96, asserting the verdict, the `scene: title_card` line
  and `underflows=0` (it passed: 12 visits, 0 underflows).
- **The card's cost, priced from PR-(2)'s legs**: its box and spans are ~250 cells at L1/L2's
  measured prices, and its entry is a scene-map write plus one commit — an event, not a per-frame
  cost. The picture is a **placeholder fill** until `ART1` lands in PR-(4); the element is declared
  and its box painted so the layout and the audit are already the product's.
- **Gates**: `make album-layout-test` (the new schema leg + the report key), `make album-pack-check`,
  `make album-seam-test` (the Select row and its precedence), `make album-test` (AP-1 unchanged:
  verdict PASSED, boot 11.27 M c), `make album-card-test` (new), `make check`. The card's own host
  legs — rectangle-only, opaque over `blank`, every span fits, the registry order — are in
  `tests/album_surface_host.rs` (18 → 22 legs).
- **Q20 (ii)/(iii), answered** (§15's row now carries them): the `Select` order is the variant's
  declared registry order, filtered to what the ROM implements; and **the card stays in the runtime
  cycle with no "PRESS START"** — audio already plays under option (c), so a prompt would be a lie.
  The boot-flow flip (card first, press starts audio) remains a separate PR that restates AP-1.

**Boot flow stays 2a's option (c)**: the card is `Select`-reachable and
audio still starts by itself, so **AP-1's witness and its 1.5 s term are untouched**. Q20 (ii) and
(iii) are answered here: the `Select` order becomes a checked registry field, and a card that says
"PRESS START" over already-playing audio is named as the lie it is — the reachable card drops the
prompt and shows metadata, and **the boot-flow flip (card first, press starts audio) is a separate
PR that restates AP-1 in the same PR**, exactly as §5.2 requires. Gate: AP-3's snapshot and fit
legs extend over the card; a host leg proves the card paints its rectangle opaquely and touches no
chrome cell (§5.2's backing rule); the residency leg proves a declared non-resident scene is not
counted in the pool sum. Must not re-baseline: AP-1, the chrome bands, or the audio path.

**(4) Album art (A8), the card's picture, and the control art — 4a landed 2026-11-26, 4b remains.** The `[art]` parser (D2's section
refusal becomes a parsed section with a per-key allow-list), `scripts/album_art.py` generalised
from `scripts/album_probe_art.py` (its quantiser, 2×1 Bayer dither, `PALETTE_MAX` and 4bpp packing
are already the spec), `ART1`, the art scene, the card's picture, and determinism (two runs
byte-identical). **The remaining icon slots land here** — authored control art replacing 2a's
sentinels, plus `RESTART` and the `PAUSED` badge — because AP-4's control-art fixture asks that
every declared slot be authored and that Play/Pause differ by shape with colour removed. **AP-4
goes green here and not earlier.** R15's note records the cover that now exists; whether the source
image is tracked (as `assets/sound/`'s master is) or only its `ART1` is this PR's decision, and
the 17,473,190 B source — most of it a Photoshop APP1 preview chain — is a sizing fact for that
decision. Gate: `make album-art-check`, AP-4's reproduction, round-trip, allocation and
off-palette legs, and `bleed`'s occlusion report. Must not re-baseline: the derived box list or
`image_tiles` — the image term moves from *ceiling* to *selection*, which the report shows as a
change of term and not of budget.

**Landed in two commits, because the art pipeline and the screen are different reviews.** **4a (2026-11-26):** `crates/gba-scene` (the vocabulary: `Rect`/`Write`/`Writes`/`Source`, re-exported by `surface` so the 2a legs keep their names) and `crates/gba-art` (the `ART1` reader lifted from the probe, plus `paint`); `scripts/album_art.py` (ffmpeg decode, median cut to ≤15 colours, ordered dither, `ART1` v1, occlusion report, self-test, deterministic); the `[art]` parser and its allow-list; the **album-scoped budget split** — `layout.rs` keeps the spec (and `POOL_UPPER` at the ceiling), `assets.rs` carries the selected `POOL_IMAGE`/`POOL_DISTINCT`, the `ART1` blob and the album's asserts (Decision B; a 96×96 selection is witnessed in the layout self-test); and `make album-art-check` (tool self-test, two runs byte-identical, the generated blob re-read through `gba-art`, a 96×96 tool run). The fixture album's 128×128 is B's ceiling, so the selection difference is the self-test's witness, not the fixture's. **4b landed 2026-11-27:** the art scene (`SceneId::Art`, centred in the region from the selected box), the card's picture (a centre crop of the same `ART1`, since the card and the art scene share one image), the `Tilesets` ART1 tileset and palette-bank load (`art_bank` from the `[palette] role = "art"` allocation; a valid one-tile placeholder when `HAS_ART` is false), and the boot `art=<box px>` line. The card replay arm now takes **two** Select edges (blank → art → title_card) and still passes. **Scene backing fixed in the same commit:** both the art scene and the card paint the flat region fill before their content, so entering one from the other cannot show the previous scene through the cells the new one does not write; the card's background is the region fill, not the previous cover (§5.2's backing rule; two host legs compose art→card and card→art). **Still open in 4b:** the AP-4 control-art fixture and the state-dependent play/pause shape (the five slots are authored and distinct; the `PAUSED` badge remains the state word until the icon pair lands).

**(5) The presentation path (§5.7).** AP-3c's paired arms A/B/C are the instrument and can land
inside this PR — **and from 2026-11-25 the instrument list is A/B/C _plus_ arm D and the
dirty-screenblock-at-VBlank leg** (see §13 AP-3c's landed note): arms B/C present OAM only, so the
BG side of this PR's own claim is the one path the probe has not exercised. Landing this PR without
them would implement the replacement without measuring it; the implementation is the chrome as OBJ (~56 OAM, ~202–206 OBJ tiles of the
separate 32 KiB pool) with a double-buffered OAM shadow presented by DMA3 at VBlank, and the
scene/list map changes presented from VBlank as dirty screenblocks. If arm A shows the 1 Hz/event
commit is already a producer risk, the exception is retired and **§11's 2a row is amended in this
PR**, as §5.7 says. Must not re-baseline: the `Write` plan and cell geometry (the sprite is placed
at `x*8, y*8`), the host geometry legs, or the list's BG model (30×8 = 240 cells > 128 OAM).

**(6) The list (A11), as the schema's second consumer.** The window derived as
`floor(region_height / row_px)` — never a row count in prose — with scroll, cursor, now-playing
mark and album header, and `resident_scenes` set to the trio. This is the PR that proves the
schema generalises past one scene, and it gives the report its per-row text term on a real scene.
Gate: AP-3's snapshot table gains the list scene; the report's list term cites L1/L2's measured
per-element figures. Must not re-baseline: the title-card or art legs.

**(7) The productionization step this ladder exists to reach.** `albums/<name>/album.toml`, an
output-path parameter so two albums do not overwrite `examples/album_player/src/assets.rs`, a
`make album-rom ALBUM=…` shape, and the docs for adding an album. It is outside 2b's phase row and
changes no codec, no layout and no screen; it is named because (1)–(6) are what make it a config
file rather than a fork.

**Three rules this ladder is built on, stated so a later PR does not rediscover them.** *(i)* **The
kind vocabulary is closed and every kind is statically derivable** — §16's "no general GUI toolkit"
and §5.0's "a band table with a slot" are not taste, they are what makes the budget exact at bake
time; a scene kind whose geometry is decided at runtime cannot be audited and is not admitted.
*(ii)* **Albums select names, never numbers** — `[layout] variant` (and at most a named scene set)
stays an album's whole layout input, so AP-3's snapshot-per-variant remains the gate and growing
the library is a spec PR; per-album geometry would make every album a new gate and every budget an
unshared number. *(iii)* **Refusal before panic, and determinism** — R14's pool exhaustion is a
hardware panic, so no album may reach it; art gets the same "build twice, compare bytes" the audio
path has had since Phase 1.

**What the ladder costs, said rather than discovered:** the cover is on screen at (4) and not
before, which is the one thing an art-first order would buy; AP-4 is not green before (4); the
list is not reachable before (6). In exchange every scene after (1) is both product and
calibration, and album two is a config file.


**Two re-homings that come with the split, named rather than slipped:** **A7 (the progress
bar) moves from Phase 3 to 2a** — the reciprocal arithmetic has been generated and host-tested
since PR-2, and a bar is the one element of the chrome band that already exists in the plan's
furniture; a layout without it is not the layout. And **A8 (album art) moves from Phase 2 to
2b**, so 2a is art-free and *says so* — the honest alternative was an art path judged on a
diagram. Nothing else in the table moves, no gate is renumbered, and the old "pause menu" in
that row is gone because **no pause menu exists in 2a or 2b** (§5.3): the paused state is a
chrome state, settings remain a §16 non-goal, and the only runtime knob the app has ever
owned — the picture — is now a scene cycle on `Select` (§5.3), which the band re-rule makes
legal and **Q20** sequences.

**Phase 1's pack step landed 2026-11-15 as PR-2 (A3a, §9.0).** The tracked
`examples/album_player/album.toml` → `scripts/album_prep.py` (product-rate clip
extraction, tag read, §5.5 fold) → `scripts/album_bake.py` (the driver; per distinct
source it shells to `gafp_pack.py --master --emit-pin` and forwards the child's
refusal) → three `GAFP` blobs, `ALBM1`, and the generated `src/assets.rs`, all
gitignored. The gate is `make album-pack-check` (not `album-bake-check` — that name
is AP-5/Phase 3's budget-and-art gate, §9.1). It bakes twice and `cmp`s every byte,
runs the four Phase-1 refusals from `scripts/album_bake.py --self-test`, and runs
`examples/album_player/tests/album_pack_host.rs`, which parses the finished `ALBM1`
through the shipping `meta.rs` — refusals **(a)** declared rate ≠ product clock,
**(b)** frames outside the encode profile (inherited, never reimplemented), **(c)**
`ALBM1`'s `n_samples` vs the packed container's `total_samples`, **(d)** the writer's
output re-read through the reader that ships.

Real census from the fixture pack (asset-level mean, §10.2b's label — not a
worst-frame row): the 258.61 s master becomes **12,293,131 B / 380.3 kbit/s**, the
two 30 s clips **1,337,159 B / 356.6** and **1,474,602 B / 393.2 kbit/s**, and the
two full tracks alias one blob, so the asset region is **15,104,892 B of a
33,554,432 B cart (45.0%)** over **9.62 album minutes**; the duration-weighted mean
is **~380 kbit/s ⇒ ~11.8 min/cart**, against R2's measured ~12.3 min. The host
suite's count is a floor, not a target.

**`ALBM1`'s shape as shipped:** 64 B header + **40 B/track** (§4.3's field list, 32 B, plus
8 B reserved to the entry's end) + a 104 B string pool + the asset region = **15,105,220 B**
for the fixture. That is **8 B/track over §10.1's "~64 B + 32 B/track + strings"** — 256 B at
the 32-track cap, which the budget absorbs without noticing; recorded here rather than
§10.1 quietly re-budgeted from the other direction.

The app crate takes §6's no-divide arithmetic as a **project-source dependency**
(`gba_playback::nodiv`, path dependency on `crates/gba-playback`) and owns **no**
`#[path]` include — §2.2's rule applied to the second file the app wanted from
another directory. Recorded as **D4** below.

**Deviations D1–D5, recorded here and not yet applied to §9.1.** §9.1's table says
`album-rom` and `album-test` land in Phase 1. PR-2 lands the *pack* half only: there
is no ROM main, no `PcmSource`/backend pair and nothing that plays, so both targets
are PR-3's (**D3**). **Half of that is now discharged:** PR-3a landed `album-rom` and
§9.1 carries its row; `album-test` stays deferred to PR-3b, because there is still no
key handling, no pause and no skip — the ROM that exists plays one track and proves it.
The rest of this note stays as written for D1/D2/D4/D5.

The other two are Phase-1 readings the config and the reader needed and the spec
leaves implicit. **D1: a clip window is in the *source's* sample indices, not album
samples.** `[[track]]`'s `start_sample`/`end_sample` are applied to the master
before the resample to 16,384 Hz (Q15's quietest 50 ms windows are quoted at the
master's 96 kHz), because `album_prep.extract` is codec-blind by design and cuts
before it resamples; album positions come from `ALBM1` alone, which is the §4.3
rule-1 division D1 keeps intact. **D2: an unknown top-level config section is a
named refusal** (`config-section`, naming §9.0/Phase 3). Phase 1 reads
`[album]`/`[[track]]`/`[out]`; silently ignoring `[art]`/`[viz]`/`[ui]`/`[game]`
would make the tracked config a file that lies about what it controls, and refusing
is the cheapest honest answer until those sections mean something — which, since the
§11 re-ruling, is **2b for `[art]`** (the box and `bleed` are declared there, §7) and Phase 3–6 for
the others. **Confirmed rather than inferred, 2026-11-20, while scoping 2a's first PR: 2a does not
part-open that section for one key.** The box *validator* is 2a's — `album-layout-gen` derives the
legal box list and takes the pool term at that ceiling (§5.0) — and the refusal rides in with 2b's
`[art]` parser, checking against the list `layout.rs` already carries. §9.0's register row and
§11's note (iii) say the same thing.

**D4: the app depends on §6's arithmetic as project source, and owns no `#[path]`
include.** PR-2 first re-committed the error §2.2 names —
`examples/album_player/src/lib.rs` `#[path]`-included `examples/album_probe/src/nodiv.rs`
(and, through it, the FLAC track's `geometry.rs`). The fix is the lift §2.2 already
prescribes rather than a local patch: `nodiv.rs` **moved** into `crates/gba-playback`
(`gba_playback::nodiv`, re-exported by the probe and depended on by the app), and the
clock tie became the link site's instead of the module's — `nodiv` keeps §6's
`RATE_LOG2` and imports no geometry, while the probe asserts
`geometry::PLAYBACK_RATE_HZ == 1 << RATE_LOG2` and the app asserts its own
`meta::PRODUCT_RATE_HZ == 1 << RATE_LOG2`. PR-3 replaces that constant with the
§4.2b derived `geometry.rs`; inverting the dependency is what lets one file serve
three consumers across two directories meanwhile.

**D5: the fixture's titles are Q15's names and nothing more — Q15's "free extra" is
taken in the tool, not in the config.** PR-2 first shipped
`"Bioluminescent Clip 1 — Öffnung (Alternate Mix, Take 12)"`, a wholly parenthesised
`"(Bioluminescent Clip 2)"`, and `artist = "Reference Master"` in `album.toml`, to drive
§5.5's fold and §5.2's box report the way this file's Q15 detail suggests. That was the
wrong place. `album.toml` is the *one tracked artifact* of an album build (§2.3), and a
recording that does not exist, spelled as though it did, reads to every future reader as
album metadata — including in the PR body and the bake log, where it did. The names are now
exactly Q15's (`Bioluminescent 1`, `Bioluminescent Clip 1`, `Bioluminescent 2`, `Bioluminescent
Clip 2`), the album artist is `"unknown"` because the tracked master carries **no RIFF tags**
(verified by `ffprobe`, which prints nothing for it) and a track artist is therefore empty by
observation rather than by invention, and a host test
(`the_fixture_titles_are_q15s_names_verbatim`) now refuses any future decoration — it asserts the four
titles are Q15's names *verbatim* and every track artist empty, so a decorated or accented title can
only reach `ALBM1` through the fold that `scripts/album_prep.py --self-test` witnesses on inputs
labelled synthetic. A title's *fit* is not this test's business and, after 2a PR-1, was never a
character count: `album_prep.py`'s `TITLE_BOX_MAX = 28` is gone (it sat at `:48` when this row was
written), and the box belongs to the variant (§9.0's `title-fit` row, §5.5, §11's D9).
Nothing is lost as coverage: non-ASCII, the 74-character synthetic that used to be the >28-char case
(its own note says it is *folded, never truncated* — truncating there would be the tool making an album
decision), a whole wrapped title and a
title that folds to nothing are all `scripts/album_prep.py --self-test` cases on inputs
labelled synthetic (7 fold cases, and no box report — the box left this tool with `TITLE_BOX_MAX`, and
the fit measurement is `album_bake.py`'s, witnessed by `make album-layout-test`), which is where a rule gets tested
against *every* branch instead of against whichever word happened to be in a title. What
§5.5's one-line-elision rule still lacks is a **screen** to overflow — that is AP-3's, in 2b's
list window, and it will need such a string in a layout fixture rather than in an album's metadata
(§5.2's list row: an album longer than the window is the normal case, so a host-generated 12-track
album in an 8-row window overflows it by 50 % and is the fixture that proves the scroll, the elision
and the header total at once — and it stays out of the tracked album, per D5). `album-pack-check` 46 → **45** (two decoration-shaped tests replaced by the
guard); floors unmoved.

**Phase 1's PR-3a landed 2026-11-15: the seam, the derived geometry, and the first ROM
that plays a track off the manifest.** Four commits on `feat/album-player-phase1-rom`:
`crates/gba-audio-hw` (the arm-only sound bootstrap), `pcm.rs` + `geometry.rs` +
`backend/{mod,flac,stub}.rs` + `make album-seam-test`, the host leg proving the seam's
i16 round-trip, and `src/album_play_main.rs` + `make album-rom`. `album-player.gba` is
15,291,532 B (`.text` 56,384 / `.iwram` 2,036 / `.bss` 42,620 — **no `.iwram` growth
from app code**, §10.3), and headless it ends
`underflows=0`, `coalesced=0`, `late=0`, PCM and both FIFO proofs `MATCH`,
`verdict PASSED (ledger, audit, pcm, fifo)`. That is **mGBA**, one 30 s clip, no screen
and no keys: AP-1 is not closed, and no hardware claim is made (AP-7, R3).
Three things it settled that this file had left open, recorded as deviations:

**D6: `gba-audio-hw` is a fourth copy of the DMA/timer plumbing, deliberately.** §2.2 and
D4 forbid the app from `#[path]`-including the spike (`rate_play_main.rs`) or the probe
(`probe_hw.rs`), and the shipped copy lives in a 2,457-line bin. The lift's destination
crate now exists, parameterised by `{ticks_per_sample, chunk_samples}` rather than by
FLAC's geometry (§4.2b), and the migration of the three existing consumers onto it is its
own follow-up PR — witnessed by `flac-check`, `product-play-test` and `album-probe-test`
— because it re-baselines two accepted ROM images, which is exactly the reasoning §2.2
used to keep PR-1's lift separate. A copy with a named migration is better than a
`#[path]` the app is forbidden to own.

**D7: the int16 seam is FIFO-lossless, measured rather than assumed.** §4.2 hands the app
interleaved **int16** while `convert_frame` and every pin are defined over the decoder's
**i32**. `fifo_byte` reads only bits 15..8, so widening back with a sign-extend reproduces
the shipped bytes exactly: all of track 1 through the seam → `convert_frame` → `fold_bytes`
lands on `fnv_fifo_l 0xc26672ecd4d96261` / `fnv_fifo_r 0x00ea76cc78ca2608`, the bake's own
values, and that test is a leg of `album-seam-test` (20 tests, floor 18). Nothing in this
file said that was true before it was checked, and a scale mismatch would have been silent
in every UI test.

**D8: what the ROM's hashes witness.** The proofs are a **cold re-decode** after teardown,
not a fold of the bytes that reached the DMA (that fold is §10.2's refused row), so the
hashes are a *decode* witness and the ring ledger (`pushed == consumed == 491,520 ==
TRACKS[1].samples`) is the witness of the bytes carried. For Q15's fixture, where two
tracks alias one blob, the index→content claim therefore rests on `ALBM1`'s
`(offset, len)` + the per-track pins cross-checked at boot (§4.3 rule 1, R11) — which is
what PR-3a's boot gate does, and why a stale or swapped blob is a named `FAIL` and not a
plausible-sounding track.

**D9: geometry was stated three times in the tree. This file deleted the copy it owned on
2026-11-20; 2a PR-1 deleted the other two (landed).** This was a deviation the spec did not name,
found while re-ruling §5 — recorded here rather than fixed silently, because the fix was a work
item with a gate (§13 AP-3), not a doc edit. Three copies of the same number, with nothing between
them, and what became of each:

1. **this file's prose** — §5's old "28-character worst case", and §5.5's old 8-row/30-cell
   framing. **Removed here**, at the ruling: §5.5 names both numbers as history instead of stating
   them as rules, and §5.5's cost table and §10.2's element row are re-priced in tile cells rather
   than in characters (the "28 characters (the box's limit)" both of those rows carried is gone);
2. **a Rust host-test guard**, `assert!(title.chars().count() <= 28, …)` at
   `examples/album_player/tests/album_pack_host.rs:241` as this note found it — **removed by 2a
   PR-1**, which is the copy that had the most authority because it was the only one that failed a
   build. Its failure message pointed at *"§5.2's box"*, a section that stopped defining a
   28-character box at the re-rule: the drift, in one line. What stands in its place measures —
   `album_pack_host.rs:297` asserts every fixture title's advance sum is inside the box the variant
   declares, and `:330` is the negative control that shows the measurement can say *no*; the
   `chars().count() <= 28` string is on `album-layout-test`'s banned-name list so it cannot creep
   back;
3. **`TITLE_BOX_MAX = 28` in `scripts/album_prep.py:48`**, which decided what the *bake* refused
   and was the one that reached a user's album — **removed by 2a PR-1.** The bake now measures
   (`album_bake.py`'s `title_fit`, now at `:2352`, called from the title path at `:1072`) and, per
   §5.5, prints a title that does not fit rather than refusing it. **What that left unpaid is paid
   as of 2a PR-(2):** the elision that makes an over-long title renderable in one line exists on
   both sides — `FaceMetrics.elide` behind `title_render` in the bake, `elide`/`fit` in the
   generated atlas module — and a box that cannot render *any* title, elided or not, is `title-fit`
   again (§9.0's row, §5.5). So a long title prints the bake's note and draws ellipsised on screen;
   the case that fails the build is a variant whose title box is narrower than the marker.

A fourth copy existed in a different form: **`BAR_CELLS = 32`**, at `scripts/album_bake.py:76` as this
note found it (`:76` is `DEFAULT_ALBUM` today), which fed each track's `recip` in `ALBM1` — geometry that had already reached the audio arithmetic (§6).
**2a PR-1 removed the shared-crate constant too** (`nodiv::BAR_CELLS` is gone; `bar_recip` takes the
grid as an argument), and the number survives as `BAR_CELLS_REGISTERED` at `album_bake.py:104`,
passed **by name** at §6's worked example and at the Phase-1 fixture legs and the default of nothing
(:328, :352, :436, :581 all require it). And `src/layout.rs`, which §4.1's tree has always listed as
"every geometry constant in the tree", **now exists** — as generated, gitignored output, which is
what makes that listing true rather than aspirational. What it does *not* buy is a module-wide
backstop, and this is where the claim has to stop short: the boot check compares **one number**
(`meta.rs:595-596`, the manifest's `bar_cells` against `layout::BAR_CELLS`, with `:827-832` proving
the reciprocal against the same const). Two of the three artifacts are at least pinned to one
variant by name now — `assets::LAYOUT_VARIANT` against `layout::LAYOUT_ID`, in
`the_pinned_album_and_the_generated_module_name_one_layout_variant`, which `album-layout-test` runs
by filter because `assets.rs` promises that leg in its own comment. A stale `src/layout.rs` regenerated from an older revision
of the *same* variant — same grid, different advance tables, boxes or pool counts — boots without a
word, and so does a module for a *different* variant that declares the same grid and the same
geometry. `album-pack-check` and `album-layout-test` compare the module's bytes against a re-run of the
bake, which catches that drift on any machine that runs a gate; a `layout.rs` regenerated after the
last gate is not caught on hardware, and if 2b wants that, it wants a spec digest in the manifest
header. §6's row is where that would be ruled.

The shape that replaced the three copies is the one this note asked for: the tracked
`layout_spec.toml` owns the numbers, `src/layout.rs` is generated output, and the
bake tool and the host tests *read* both instead of holding their own. `album-layout-test`
then asserts the agreement across the three artifacts (§6's `bar_cells` ↔ reciprocals guard is
the first of them), which is cheaper than the alternative — a build that is green while the
tool, the ROM and the test each believe a different width. **What 2a is not doing: arguing about
28.** That number is not wrong for the fixture, and no part of this deviation is about its value;
it is about the fact that three artifacts hold it independently.

**(D9 is a 2026-11-20 record and is the last deviation note; the ones below are the phase's
own history and are unchanged.)**

**PR-3a fix 2 landed 2026-11-16, from a human playing the ROM (§10.5, R12).** It is recorded here
because it is the first finding in this application's line that no gate in §13 could have produced:
boot cost 52.56 s → **589 ms**, `ALBM1` **version 2** (64 B header unchanged: `fnv_structure` over
table‖pool at byte 44, `fnv_assets` at byte 60, 54..60 reserved — so §10.1's ~64 B + 32 B/track line
still holds), per-track `prefix_fnv`/`suffix_fnv` tripwires in the generated pin module, the full asset
fold moved to an explicit post-audio audit gate, `FlacSource::new` no longer touches track 0's 12.29 MB
container, a `CycleCounter` in `crates/gba-audio-hw`, and a backdrop verdict lamp. Gates moved with it:
`album-seam-test` 20 → 24, `album-pack-check` 45 → 50, and the new `boot_audit_bytes` guard is the
thing that keeps this from being rediscovered the expensive way. **Known and not yet paid:** the
post-audio proof + asset audit is ~75 s of CPU, which is fine for a test ROM and not fine for a
player — a `--features deep-audit` decision for `make album-test` and Phase 5.

**PR-3a fix 3 landed 2026-11-16, also from the caller's hardware session.** Two changes, both traceable
to a person using the product rather than a gate running it. The ~75 s post-audio verification is now
behind **`deep-audit`**, in `default` so `make album-rom` stays the artifact AP-1 will run, with
`make album-player-rom` building the fast one — which must *say* it skipped (`NOTE` at boot,
`SKIPPED` terms in the verdict), because an unaudited green is worse than an honest unknown. And the
verdict became perceivable: §5.6 is the new rule, born from the fact that fix 2's lamp blinked pure
blue (relative luminance 0.072) against black (0.000) and called that hue-free while the verifier is
colourblind; the lit phase is white (1.000), rate carries pass/fail (995 mHz / 4,978 mHz, computed from
the 280,896 c frame), and three 1 kHz beeps confirm it audibly. The same session produced this
application's **first hardware witness** — one unit, one 30 s track, clean audio with no stutter —
recorded against AP-7 and explicitly not a closure of it.

**PR-3a fix 4 landed 2026-11-16: the verdict is a word, and the annunciator is gone.** The caller
asked why this ROM didn't do what every past PR did — render PASS/FAIL as text — and there was no good
answer but my own scoping. `examples/album_player/src/text.rs` (§4.1's planned file, pulled forward at
minimum scope: the font include and one present, no measure/wrap/layout — that stays Phase 2's A5) now
gives white-while-running → BLUE `PASSED` / RED `FAILED`; fix 3's blink, tone, `arm_tone` and
`COLOR_LIT/OFF/PENDING` are deleted (−372/+117 in the ROM), which also voids the §5.6 open item about
refusals having no tone, because a refusal now needs no timer at all — it says `FAILED`. §5.6 is
rewritten to the accurate constraint, and §10.1 gets its first real linker-map read-out.
**What this gave up:** `arm_tone` was the application's only code to re-arm the audio chain after
teardown, which is the shape §4.4's skip path wants in PR-3b. It witnessed nothing but its own drain
count, so PR-3b derives that sequence properly, with tests, rather than inheriting a rehearsal.

**PR-3a's first two hardware legs were witnessed on 2026-11-16, by the caller, on a GBA** — see AP-7a.
The second one is the one that closed this loop: the same person who could not see fix 2's
violet→blue lamp confirmed "audio plays correctly, then the screen goes blue and reads `PASSED`". A
verdict scheme is not validated by its luminance arithmetic; it is validated by the verifier reading
it. The ROM hashes those runs used are recorded against AP-7a, and a later rebuild on a moved `main`
(ULI2 PR-E merged, PR #148 reverted) reproduced **both images byte-identical**, which is the strongest
reproducibility evidence this application has produced short of AP-5's full two-run gate.

**PR-3b landed 2026-11-17: the album plays through, and the buttons do something.** Keys (`keys.rs` —
the first code in this repository ever to read a keypad), the session clock (`position.rs`, §10.4's ruled
u32 in `.iwram`, +4 bytes, price paid as written), the transport (`transport.rs`: pause, skip, restart,
track-end auto-advance, `ALBUM_END`), the **hand-off gain** that makes a fade audible, and
`make album-test`. `SKIP_PRIME_CHUNKS = 4`, chosen from the probe's sweep rather than guessed (D2).
Measured and now in §4.4: the four-leg reaction chain (press → audible **306 ms**, ~580 ms onto a
silence-headed track), the inter-track boundary gap (**214 / 229 ms**, and *derived* — four of its
periods are the cover constant), and the pause contract, which had never been written down anywhere.
Three claims this round retracted rather than repaired silently, all of them worth the reader's
attention because each was asserted confidently first: *(1)* my own "≤ 3 frames ≈ 50 ms" from fix 4's
round was arithmetic about a floor with the loop's own latency left out of it; *(2)* "the content proofs
match through a pause, therefore a pause drops no sample" was **false** — those folds are a cold
re-decode and are invariant to the transport, so the transport witness moved to a host model that records
the post-gain bytes handed to `rearm_dma`; *(3)* the fade that §4.4 exists to require was being cut by
the transition that played it, for two rounds, by code quoting the row it violated. §13's AP-1 row is
amended for the two promises this harness physically cannot keep (no key injection, no audio capture) and
states plainly that **click-free-to-an-ear and physical presses remain unwitnessed until AP-7a**.

**PR-3c — landed on `main` as the merge `2a946f2` (PR #170, 2026-11-19; the branch it was developed
on, `feat/album-player-pr3c-resume-refill`, is deleted). A resume into an under-run ring
produces audio.** What it used to do was halt the album: `Chunk::Starved` made `prime_hand_off()` return
false (`src/album_play_main.rs:1777` at the scoping), the run stopped with reason `resume` on a bare red
`FAILED`, and **nothing in that block was feature-gated** — that is what a person playing
`make album-player-rom` met. The mechanism named above (`begin_reprime`, `State::Repriming`,
`fade_in_after(chunks)`) is **not** the path this PR took, and the reason is §4.4 rather than taste:
`Repriming` is reachable only through `begin_reprime`, which is called in the same critical section as
`truncate()`, and `truncate()` **steps the consumer over the stale span** — a seek, which for a skip is the
whole point (§4.4 step 5) and for a pause is the one thing its own row refuses (`resumed_committed` must
equal `frozen_committed`). Q18's ruling that the ring head must not move therefore selects the mechanism.
What the PR is instead, in one sentence: **the press is routed by the ring's own answer, and a ring that
cannot give the prime its first chunk is refilled with the chain still frozen.** `Transport::toggle` now
takes `primable` (`src/transport.rs:692`), the frame loop asks the ring first with `ring_primable()`
(`src/album_play_main.rs:1807` — `ready() >= CHUNK || (finished && ready() > 0)`, which is `take_chunk`'s
own two answers and not a guess about the next pass), and the `Paused` arm of `toggle` goes to a new
[`State::Buffering`] and returns `Step::Buffer` (`src/transport.rs:524,597,719-721`) instead of starting a
ramp nothing can be served on. `State::feeds()` grows to include it (`:551`) so the producer runs; a new
`State::pass_timeable()` (`:567`) keeps the two gates apart, because the two questions a state answers —
*may the producer fill this ring*, *may this pass be timed on the audio clock* — are the same set except
here, and timing a pass against a stopped counter books a false underflow on every pass of the wait.
`Transport::end_buffer()` (`:759`) then leaves `Buffering`, starts the ramp and returns the same
`Step::Prime` the direct resume returns: **the ramp starts on the arm's chunk, not at the press**, which is
`HandOffGain::fade_in_after`'s own lesson applied one state earlier — a ramp is spent on whatever the
feeder hands over next, and a frozen chain hands over nothing. The arm itself is the ordinary
`resume_arm` → `prime_hand_off` → `arm_audio_chain` sequence taken on the ring head the freeze stopped on.

What it carries, item by item against the list above:

1. **A leg with a bound, printed in the pause row — as `resume→arm=N frames (bound M)`, not
   `resume→audio=`.** `resume_to_audio_bound(worst_pass_frames, boundary_frames)` (`src/transport.rs:317`)
   is `PREFILL_FRAMES × worst_pass_frames + MAX_BOUNDARY_FRAMES`: the decode the run's own measured pass
   price charges for the prefill's depth, plus the alignment allowance every leg that ends at an audio
   period is given. It is a function of the run's measurement — on an `input-replay` arm the worst pass is
   this run's own `producer_max_cycles`; elsewhere the row prints `bound=not measured on this arm` rather
   than a number copied from an emulator (R13). The leg it bounds is press/take → **re-arm**, and the row
   names that: the primed chunk *is* the ramp's first chunk, so the rest of the way to the first audible
   sample is the ramp plus the play-out the `Resuming` state, the `latency:` lines and the `reaction=`
   terms already measure. **Ruled by the caller, 2026-11-19: that composition is left, with a named home.**
   The scoping asked for one row running end to end and this PR does not give it one. The reason is not
   taste: the only *machine-dependent* half of that leg is press → re-arm, and that is the half this bound
   bounds; the rest of the way to the first audible byte is fixed by the mechanism rather than by the
   machine: the in-ramp is confined to one chunk (`FADE_SAMPLES` = 256 < `CHUNK_SAMPLES`), which is exactly
   why `State::Resuming`'s own doc says that state lasts **one period** — the ramp completes inside the
   chunk the re-arm primed — and what follows is that chunk's play-out. Those are the same *kind* of
   mechanism constants `pause_to_freeze_bound` spends on the way out (`FADE_PERIODS` plus one alignment
   period), and the row already prints its measured out-half as `action→freeze=N periods (bound M)`. The
   missing figure is therefore derivable from what is printed, not unmeasured. Where it would go if it is
   ever closed: the `latency:`/`reaction` composition, which already totals a press→audible leg for the
   run's presses (`latency: press→detect=1f +
   detect→take=3f + take→hand-off=2p + hand-off→audible=5p (cover) = press→audible 18 frames`) — a resume
   press *is* a press, so that line, not two more terms in the pause row, is where its arm→audible half
   belongs. What leaving it costs, plainly: nothing refuses if a future change lengthens the ramp or moves
   where the arm lands; the claim stays split across two rows instead of closing in one.
2. **The counter the refusal stopped providing.** `Pauses` counts `refills` (`:2956,3026`) and the summary
   line prints `refills=N` **whatever its value** (`:3125`), with `primable=yes|no` on every row (`:3141`)
   so a branch that never happened and a branch never printed stay distinguishable; `album-test` reads the
   count against the rows and fails if they disagree. `dropped=` stays the pause's own term and is still 0
   on both routes — no cover is pushed, so the question "is a cover chunk track content?" does not arise on
   this path, and the no-seek witness is structural instead: `head_at_press == resumed_committed`, refused
   in `record_ok` (`:3089`) for every refill row.
3. **The visible indicator: deferred by the caller (2026-11-19), and its name in this scoping was wrong.**
   The wait is counted, printed and bounded, and it is still invisible to the person holding the cartridge —
   a paused-and-refilling product looks exactly like a paused one. What this item's own scoping called "the
   A11 affordance" is corrected at §5.3, where the re-homing belongs: A11 is the track-list screen (Phase 2,
   AP-3, not in this tree) and no affordance row covers a transport-state indicator at all. Deferring it is
   cheap rather than costly for two reasons worth recording: even **pause** has no live indicator today
   (`text.rs` paints a verdict banner, a `FAILED <TERM>`, and a `PAUSED` suffix at *teardown*, with a blank
   backdrop while running — `album_play_main.rs:43,91-96`), so PR-3c is not removing a signal, it is
   declining to half-invent a UI vocabulary that AP-3's `layout.rs` will replace; and everything a screen
   would need already exists — a named state, a count, a bounded leg. What is **not** deferred: the
   transport's refusal of a second press and of a skip during a refill. That stands until the layer §5.3
   describes exists to decide it.
4. **Host tests, and the gate**: `tests/album_seam_host.rs:3180` ("8c-bis") — the route taken and the same
   byte stream after it (a fixture that reaches a 160-sample ring through **full** periods, so its
   `underflows` is 0 and the stream comparison is unqualified); the negative half (a ring that can prime
   pays nothing); `resume_routes_by_what_the_ring_answers_not_by_what_the_state_hopes`, which compares
   `primable` with what `take_chunk` actually replies at every depth around the boundary; the frame count
   against the boot prefill's own; the wait against the bound at four pass lengths; and the state-gate
   table. The `album-seam-test` floor is 68, and `album-test`'s pause patterns now read the new terms.
5. **Both caller decisions, encoded rather than answered.** There is no cover count `N` to choose: the wait
   ends at the **boot prefill's own target** (`geometry::PREFILL_SAMPLES`), which is exactly
   `geometry::PREFILL_FRAMES` frames from any depth a refill can start from — pinned at
   `src/geometry.rs:284,288-291` and once as a runtime fact against the real ring. That geometry is
   what satisfies "no new number", and the 36-chunk ceiling the scoping cites is not approached
   from below: an entry depth `< CHUNK` plus `PREFILL_FRAMES` frames peaks at 10,799 of 20,160 samples.
   The **maximum wait** is likewise not a duration anyone chose: it is `resume_to_audio_bound`, i.e. the
   run's own pass price times the decode that is actually needed. The two things this item left to the
   caller are now **ruled, not open** (2026-11-19): the visible indicator is deferred to the shell's PR with
   its name corrected at §5.3, and the end-to-end leg is left with a named home in the latency composition.
   What is still owed here is measurement, not decision — the paragraph below.

What is unwitnessed, stated as such: **no ROM arm has ever met this branch.** On every hardware leg recorded
so far the ring still held the prefill's depth when resume was pressed, so the row prints `refills=0` and the
hardware evidence for PR-3c is that the branch is priced and does not fire — not that it works. That is a
fact about the fixtures and about the producer's margin on the arms run so far — and since
2026-11-19 the lossy backend's U4 margin is **not** part of that explanation either, because U4
is met with 13.73 points to spare (§2) — not a reason to leave the path untested, and the host half is
where it is exercised today; a cart run whose producer falls behind far enough to drain a paused ring is the
arm that would finally print `primable=no`. **Ruled 2026-11-19: that witness is asked of AP-7, which is the
only place its numbers exist.** What hardware owns is not the code path but the measurements: the bound's one
missing input is a *cart's* `worst_pass_frames` (this run prints `resume bound=43` — an emulator figure, and
by R13's own argument not a bound on a cart), the branch firing for a real reason rather than being
manufactured, and the listener-facing answer AP-7a exists to give: a hand press into a refilling product, and
whether its entry is click-free to an ear. What does **not** need hardware is the arm that manufactures the
condition — pause, a fixture-side `RingFeed::truncate()` (product code already, and a *fixture* action rather
than a transport one, so the record's no-seek check still holds: `head_at_press` is latched after it), then a
scripted resume press printing `primable=no`. It is deliberately not in this PR: on the emulator that arm
would only prove the path runs on the core and add a second emulator number, while the same arm on a cart
yields the number that matters at no extra cost. So the ask of the next AP-7 session is three items: run the
starve arm on hardware; record that cart's measured pass, so this bound stops being a hypothetical; and press
into the state by hand. Q18's row is still the spec; this paragraph is the record of
what was built against it. The old halt is now taken **only** for the case this item keeps out of scope — a
stream that has ended with an empty ring — and it is taken at the press, on a line that names it as Q13's
album-end case rather than as a failed prime; without that guard PR-3c would have made that case worse rather
than better, because an empty ring never reaches the target and the run would have sat in `Buffering` with a
frozen chain and never printed its end lines. The `Err` in `complete_resume` (`:1869`) stays as a named
refusal the routing makes unreachable: a frozen chain consumes nothing, so the ring's answer cannot go stale
between the ask and the arm, and the refill arms at the prefill's depth. A refusal that states its reason is
what R13's screen half asked for in place of a bare red; the shallow-ring condition no longer has a verdict,
it has a count. **Stated as a gap, not a pass:** the host model asserts that combination instead of modelling
it (`refill_to_target`'s `!finished` assertion), so the exhausted-stream branch is witnessed on the ROM's
routing rules and not by a host fixture.

Phase 0 exists because four things in this plan are genuinely new to this tree
and each has a way to be cheaply wrong: objects/sprites (nothing shipped draws
one), multiple simultaneously-shown backgrounds (`text.rs` shows one, one-shot),
VRAM writes and the audio DMA sharing the bus, and re-priming a full ring on
demand without starving it.
A half-day of throwaway ROMs retires four risks, and it is the pattern this repo
already uses (`examples/e5_fetch/` exists to arbitrate one question about ROM
fetching on hardware).

---

## 12. Reach items

### 12.1 A14 — the web UI

Scope it as **a form over the config file**, which makes it small, honest and
non-duplicating: a local page (`scripts/album_web.py` on `localhost`, stdlib
`http.server`, no build step, no framework, no network) that lets you point at a
folder, order tracks, edit titles/artists, set the cart budget, pick art/viz/game
options, and see live previews of the *actual* downsampled, quantised, dithered
cover — by calling the same `scripts/album_art.py` code the bake uses. Its output
is a reviewed `album.toml` (plus a `make album-bake` command line). Baking a ROM
from the browser is a checkbox that shells out to the *same* bake entry point, so
there is exactly one implementation of "make a ROM" and the web UI can never
produce a config the CLI refuses. Non-goals: hosting, accounts, uploads to a
server, audio in the browser, any dependency that has to be `npm install`ed.

**One thing that changes with the band model, and it is a restriction rather than a feature
(2026-11-20):** the form exposes `[layout] variant` as **a choice from the declared set** — a
dropdown of the names in `layout_spec.toml` (§5.0) — and never as geometry editing. The reason
is the same rule that keeps the web UI from being a second implementation: geometry a browser
can edit is geometry no host test snapshots, and §13 AP-3's promise is that every shipped
variant is one it has drawn and asserted. So the page may show a variant's composite (it
doubles as the 1×/4× preview surface, in this case `make album-preview`'s own code, §5.0 — one
implementation again, §12.1's own rule applied to the preview), and it may write a variant
name; it does not get to invent a band height.

Nice-to-have that earns its keep: the preview surface doubles as an art QA tool —
the dithering/palette refusal is visible and tunable interactively instead of
being discovered on hardware at 16 colours.

### 12.2 A15 — a toy game behind the album

The compute argument first: at **23.656% decode / 32.208% total work share on real music**
(§10.2b — not the fixture's 18.99%) there is real room, and a pong/snake
is ~12 objects, integer AABB collisions and a fixed-step sim — trivially inside
§10.2's 3% line. It is the *scheduling* that is dangerous, not the arithmetic.

**Admission rule (the whole design in one line):** a non-audio subsystem may run
only while `ring_ready >= GAME_ADMIT_CHUNKS`, and it yields immediately when that
falls. `geometry.rs` gives the currency: at the shipped prefill the ring holds
17 periods in hand and starves at the 18th, so "the ring is at least *k* chunks
deep" is a direct, measurable statement about how much time is safe to spend.
The same rule admits the viz and any art upload, and it degrades correctly: a
busy passage makes the game visibly slow down, never makes the audio glitch —
which is the right product behaviour for a music player: the picture is the thing
that is allowed to degrade, and the audio is the thing that is not.

Rules: the game reads **no audio state** and writes no VRAM the audio path uses;
it is deterministic integer-only (agb's `fixnum`, no divide); palette banks
11–15 and the OBJ pool are its allocation; it may be *observed* to be
frame-skipped (a counter in the log) without any audio consequence; and its gate
is that **the audio measurement is unchanged with the game on and off**: same
underflow count, same stage shares against the `none` control arm, and a captured
output that compares sample-for-sample against the no-game capture (mGBA audio
capture through `mgba-test-runner`, which this tree already drives; the capture
comparison is our own script). The game may be seen to drop frames; it may not be
*heard* to do anything at all.

Which game: **pong** first (one ball, two paddles, no state, ~30 lines of sim),
**snake** second (grid state, still nothing fancy), **breakout** third (tilemap
coupling). Chrome's T-Rex is the *feel* to hit — one scrolling obstacle, a jump
button, a distance counter — not the asset set.

---

## 13. Gates

`AP-*` numbering, deliberately not `U*` (taken by ULI2) or `C*` (FLAC). Each is a
`make` target in the tree's native/podman-twin shape, is verify-only, and follows
the rule that a gate touching a non-default configuration runs as its own target.

| Gate | Claim | Witness |
|---|---|---|
| **AP-0** | The probe ROMs run: objects + 3 BGs + text present simultaneously, the frame cost breakdown closes with UI terms, VRAM/OBJ split and refresh cost recorded, the skip gap measured | **Closed 2026-11-14** (`examples/album_probe/README.md`): measured — UI **1.89%** of a period, feeder 0.92%, one 4bpp tile write 726 c, art upload 0.60 period, video period 280,896 c, and the skip sweep's drain/drop ceiling (§4.4, Q13b). Not measured, still targets: viz (AP-6), game (AP-8), the ≥ 20% reserve — plus every screen and sound claim in the record's NOT VERIFIED section |
| **AP-1** | **the ROM under test was built by `make album-rom ALBUM=examples/album_player/album.toml` from a fresh clone, with no untracked input and nothing fetched (§2.3)**; ≥2 tracks play to the end of the album, pause/resume and next/prev work **from a scripted key log replayed at the keys layer**, `underflows = 0`, `old = 0` and `dropped > 0` on every pressed skip and `dropped = 0` on every track-end advance, a skip's cover is `SKIP_PRIME_CHUNKS + 1` periods, the boundary gap is printed with its decomposition (§4.4), and **audio begins within 1.5 s of boot** (564 ms and 580 ms on the 4-track fixture, in the two logs this tree keeps — `album-test.log:9-10` and `album-rom-run.log:9-10`; ~36 KiB per track of that, so a 32-track album needs the tripwire windows shrunk) | `make album-test` (**landed PR-3b**): the REPLAY arm (`--features "input-replay deep-audit"`) run headless, every assertion read off the **log** with a named refusal per failed term; exit 124 is the pass path, ~2m27s, deterministic (two runs' logs `cmp` byte-identical), and 21 doctored logs tried against it — 19 refused by name. **Two of this row's original promises are not deliverable by this harness, and are re-stated rather than quietly dropped:** *(i)* "scripted button presses" cannot mean the emulator's input layer — `mgba-test-runner` exposes no key-log, stdin, env or socket path (verified on the installed binary) — so the script is replayed through `keys.rs` via `update_with_state`, the ROM prints which source it used, and **a green here is never evidence that a physical press reaches a cartridge** (that leg is AP-7a); *(ii)* "a captured-audio comparison against a reference decode" has **no tool** — the runner records no audio — so the witness is on-ROM and in the model: per-track `pcm`/`fifo` folds against **each visit's own** pins, the whole-region `assets` fold, the ring ledger, and a host model that records the **post-gain bytes handed to `rearm_dma`** and asserts contiguity across a pause (§4.4). The folds are a *decoder* witness only — they were cited as a transport one, wrongly, and that claim is retracted here as well as in the code. **What AP-1 still cannot claim: that a transition is click-free to an ear, or that a real press works.** Until AP-7a listens, this gate is green on the emulator's terms only. **Also runs here, because the packer does: two consecutive packs → byte-identical blobs, and `ALBM1` re-read through the shipping `meta.rs` agrees with `album.toml` field by field.** **One term of this row is at risk from 2a, and it is named here rather than discovered inside the gate:** "audio begins within 1.5 s of boot" is measured today on a run where *nothing is pressed*, and §5.2's boot option (b) — surface up, transport waiting for Start — would put a human press inside that interval. If 2a chooses (b), this term splits into an auto-start arm and a Start-gated arm, and that belongs in this row's text at the moment the choice is made; choosing (a) leaves the term exactly as it is, which is a legitimate answer — but it should be an answer, not a silence. **The measuring half of "within 1.5 s" already exists; what does not exist is a gate** (2026-11-20): the ROM prints boot-to-audio, decomposed, on its own boot line (`album_play_main.rs:3866-3877` — `parse/open/gate/prefill/total` in cycles, then the total in ms; `album-test.log:9-10` records 564 ms, `album-rom-run.log:9-10` records 580 ms), and **nothing in the tree compares that number against anything**: no 1,500 ms constant, no `BOOT_BUDGET`, no `Refusal::BootTooSlow` — that grep is empty. (This row also carried two wrong log references: `album_play_main.rs:3880` for `:3877`, and `album-test.log:350` in a file that is 92 lines long.) So (b)'s 1.5 s is already being produced for a human and never asserted; if the ceiling is to be a gate, say whose job it is (AP-7a's report reads the printed number — AP-7's own row already says the 1.5 s ceiling is not machine-enforced) |
| **AP-1 status** | **PASSING — and it was closed on the instrument that opened it (2026-11-19; the row read MIXED from 2026-11-17).** `make album-test` is green and deterministic headless, and a property fuzz over 2,048 randomised interactive sessions finds no transport or ledger violation. What made it MIXED was the caller's cartridge run: heavy Left/Right/pause use ended `FAILED` while calm playthroughs passed, and the emulator never reproduced it. That run has now been repeated on a build carrying R13's fixes — `album-player-fast.gba` sha256 `76bfbba6…31755`, built by `make album-player-rom` from a throwaway merge of `main` (`e952a1c`) at this PR's head (`1676608`), the same target that found it — and the caller **cycled hard between tracks, played and paused repeatedly, then let the final track run to the end: no failure, the run passes** (AP-7a (4)). | **What that closes and what it does not.** It closes R13's observed instance, and it is why this row's discipline was worth keeping rather than softening a bound: all three mechanisms found were *verdict* bugs (ledgers summed over capped print arrays; `dropped >= 1` demanded of a press that legitimately drops nothing; a pass bound saturated exactly at mGBA's own 3-period measurement, which guaranteed a cart would fail what a cart got right), each fixed and proven in the model, and the durable part is that every bound now derives from the run's own measurement rather than from an instrument's frame count or a hand-picked constant. **It is not a claim of hardware correctness**: one unit, one session, no map-file memory report, and the emulator still does not reproduce the symptom — so this closure rests on the caller's report, which is exactly why the row stays. AP-7 (both units, two runs each, the `none`/`bar`/`skyline`/`game` arms, the memory report, a verdict legible without colour vision) remains open, as does AP-1's captured-audio promise, which still has no tool |
| **AP-2** | mm:ss elapsed + **total** match the durations **the same build's pack step printed** to ≤1 s (§6; the tool is Phase 1's as of §9.0, so this differential exists from the first album build rather than waiting for Phase 3); bar and time never disagree by more than one cell and never move backwards; no `__aeabi_*div*` in the playback/UI path | host twin on `position.rs`/`layout.rs` + objdump/symbol fact on the linked `.gba` + the bake's printed track table. **(Claim text amended 2026-11-20 from "elapsed + remaining" — §6's display re-rule. The witness is unchanged and does not need re-running, and the reason is narrower than the first one given here: text *over live audio* is not new in this tree and was never the questionable part — AP-0's probe drew text with audio running in every arm, and §14's R1b is precisely a measurement of a text redraw under live playback. So this row has no premise that a screen without words is the norm; what was wrong was the claim's pair. What has genuinely never happened anywhere in this application is the **elapsed / total pair** being displayed: the gate ROM prints verdict words only and its own header comment says it deliberately has no bar, no title text and no menu (`album_play_main.rs`). Two consequences this row should stop glossing over: the on-screen half of this witness is **owed, not passed** — it is A6's wording on the real layout, 2a's chrome (§11's 2a row), and a green host-side leg proves nothing about a screen; and there is no on-screen wording in this app for a relabelled claim to disagree with, which is why the host leg genuinely needs no re-run. `layout.rs` in this row's witness is 2a's, not today's — D9 records where the fit guard actually lives now.)** |
| **AP-3** | **Every product-eligible declared layout variant** composes to the expected host snapshot (**relaxed at the second review from "every declared variant": §5.0's `product` field and §9.0's `face-not-resolvable` refusal mean a variant can exist for another purpose — a development instrument, an art-only experiment — and be un-gateable by fit or by pool arithmetic because its declared face cannot be resolved from this tree. Those are reported as **skipped, by name with the reason printed**, so a green AP-3 always says what it did not look at. The instruments that *do* gate a non-product variant are its own: `album-preview`'s 1× composite for a face question, AP-4 for an art question.** from the same generated `src/layout.rs` — 2a: the chrome bands, the clock/bar/title elements and the `blank` scene; 2b adds the list, title-card and art scenes — **and every string fits the box that variant declares for it**, `Fit` measured from the face's real advance widths rather than a cell count (§5.5). Three records it owes, all in cycles — **and those three render legs now have their own row (AP-3b, below), because they need a ROM and this row's legs are host-side**; the geometry, fit and pool-audit legs stay here: **first, a bake-leg check that every row of §9.0's register owned by 2a fires on a spec written to be wrong about it** — **nine** of the register's rows fire on 2a's own layout instrument (`layout-variant-unknown`, `bands-not-tiling`, `pitch-not-tileable`, `face-not-resolvable`, `image-tiles-over-budget`, `scene-minimum`, `bar-cells-disagree`, `bar-cells-out-of-range`, and **`title-fit`, which arrived with 2a PR-(2)'s one-line elision** — its leg narrows a title element to one tile column against a marker that measures wider, and a leg beside it proves no title, however long, is refused at a box the variant really declares); the tenth layout row, `art-box-not-derived`, is a 2a validator waiting on a 2b key — 2a derives the box list and takes the pool term at that ceiling, but the value it refuses lives in `[art]`, which D2 keeps a named refusal through 2a (§9.0's note under its register, §11's note (iii)) — so this row reports it **named and skipped with its reason**, in the same idiom it uses for non-product variants, rather than claiming ten. **The atlas half of the register is a second instrument and a second gate: `make album-glyphs-test` fires four of the five `atlas-*` ids by name and prints the fifth (`atlas-ink-not-matching-outline`, the rasteriser's own bug path) as named-not-fired with its reason** (§5.5a step 5, §9.0's rows). Every one of those legs is checkable from `layout_spec.toml`, the tracked face files and the atlas generator's own fixture specs, with no image, no palette and no ROM in sight — which is what §11's rule "no phase's critical path may run through a tool it has not built" looks like as a test rather than as a sentence; **the per-element** redraw costs (§10.2's element rows), the **scene switch** with scenes resident against a partial map rewrite (§5.1) — **named by residency option** (§5.1's four, which differ by two orders of magnitude at that row) **and measured in the live-audio arm**, because every UI figure in §5.2, §5.5, §7 and §10.2 is arithmetic on AP-0's single-tile legs and none of them measures a renderer: this is the plan's first rough ROM leg of rendering bandwidth, and §10.2's caveat says what it is there for. and a **tile-pool audit per variant** — written against agb's pools rather than against character blocks, because that framing in §5.1 was wrong: the regular (BG) pool is **512 4bpp slots in total**, the affine pool is 510 more with a fallback path between them, maps are one whole 2,048 B screenblock per background with **eight of them in all**, and the sum being audited is the per-variant `tile_budget` the bake refuses on (§5.0, §5.1). **It also used to owe a question about sprites that is now closed by reading agb (2026-11-20):** OBJ tile data has **its own pool** — 32 KiB at `0x0601_0000` (`sprite_allocator/sprite.rs:18-24`, `TILE_SPRITE + 1024 * 8 * 4`) = **1,024 4bpp tiles / 512 at 8bpp** — agb itself selects hardware mapping mode 1 (`object.rs:30`, `set_obj_character_mapping(true)`), and its BG maps end at `0x0600_C000`, **not** at the OBJ base as this file used to claim. Three consequences carried into the plan: sprites never compete with the BG pools (so §7's refusal to buy back a full-bleed image with a sprite is right for the real reason); a sprite has a ceiling of its own that no BG-side audit sees, which is now AP-6's (§13) and Phase 4's rather than a question first met there; and **DISPCNT bit 6 must never be touched by this app**, because mode 0 would put OBJ tile data at `0x600_4000`, squarely on top of agb's regular BG pool (§5.1). The layout rows of **§9.0's refusal register** fire here, before a line of Rust compiles — this row no longer restates them, because the restatement was the drifted one (it counted an `[art] box` refusal as one of "five" that §9.0 also assigned to 2b, and it predated `pitch-not-tileable`/`face-not-resolvable` entirely). What is *this* row's, not the register's, is that the pool row must print **both bounds** of the text term (§5.1's measured table): the visible-cell bound and the distinct-glyph bound, with the declared face's line box and advances named, so a reviewer can see which bound a variant is being refused on. **The title leg is a test, not a sixth refusal:** the bake has refused an overflowing title since Phase 1, and 2a's version measures against the derived span with §5.5's one-line elision applied first — **landed with PR-(2)**: `FaceMetrics.elide`/`title_render` on the bake side, `glyphs::elide`/`fit` on the generated side, and the two copies compared through the case table the generator pins (§5.5, §5.5a) — so an album is still *fixed at bake time*, and what the host test proves is that the elision itself fits **and that it cuts at the last whole glyph that fits, not an earlier one** | `make album-layout-test` and `make album-glyphs-test` (**2a** — names this file proposes rather than ones §9.1 planned; §9.1 now carries both rows). Host snapshot + fit tests over **every** variant in `layout_spec.toml`, built on `examples/flac_integration/src/text.rs:65`'s `measure(text) -> Fit` precedent, plus the generated module's own floored suite over the atlas and the elision (§5.5a step 5), plus the surface's own floored host legs (`tests/album_surface_host.rs`: the boot frame as a 30×20 composite, §5.1's redraw rule as a write count, the clock, the bar, and the drawn line's box — PR-(3)), plus `make album-preview`'s **1×** composite — which is a *human* leg, not a machine one: a matching snapshot says the layout is what the spec says, and says nothing about whether anyone can read it (§5.6). The ROM's boot line prints `layout=<id>`, so every later hardware report is attributable to a geometry (AP-7a legs inherit that requirement) |
| **AP-3b** | **Three render legs measured in the harness this tree can actually run — mGBA, in the probe ROMs' arms — at the frame cadence of the app rather than of a benchmark; the hardware leg for each is AP-7's, and this row said "on hardware" until 2026-11-20, which asked a claim of the wrong instrument (R3)** — added 2026-11-20 because §5.1, §5.5, §7 and §10.2 are arithmetic on AP-0's single-tile legs and the plan keeps saying so; this is where those tags come off. **(L1) per-element redraw, changed cells only**, live audio running: one bar cell, one changed clock digit cell, and a re-show of an *unchanged* text run (which must be near-free — `set_tile_at_pos` early-outs when the map entry is unchanged, `regular_background.rs:375-378`, and agb's text path hands each drawn cell a `DynamicTile16` through `set_tile_dynamic16` (`font/tiled.rs:126-140`), so identical content dedupes; the early-out path has never been timed and the plan's per-cell figures are upper bounds taken from legs where the content *did* change). **(L2) a whole scene-region change**, ~600 cells, in the live-audio arm, which is what §7's full-bleed and §10.2's partial-map rows assume. **(L3) a scene switch by `set_priority` with both backgrounds resident**, timed across a commit boundary (§5.1: the priority value lands in the control word at the next `show()`), plus **one pool leg (L4), which is a text-pool leg specifically**: stand up the declared scene set — art or region image, the chrome art, and **the list's glyph cells at the declared `row_px` with real repeated text**, not a synthetic distinct-glyph fill — then read what agb actually allocated: (i) the number of live tile references, (ii) the reference counts on the shared glyph tiles, and (iii) whether the *hidden* scene's map still resolves. That single leg is not there to discover which of §5.1's two bounds is real — source settles that (§5.1's bullet: per-cell `DynamicTile16`s cannot share, so the *dynamic* path is the upper bound by construction) — it is there to prove that the **shared-glyph atlas actually dedupes in the real allocation**: the reference count on a repeated glyph's tile must be greater than 1, because a count of 1 on every glyph tile is what an atlas that silently fell back to per-cell allocation looks like, and it would read as a pass in a total-only check. It then prints the standing total (with the atlas: ≤188 glyph tiles + 256 for a 128×128 cover = 444 of 512, §5.1's interaction table) and whether the *hidden* scene's map still resolves. **The fallback ladder is corrected here, because the first version of this row offered "cut the window to six visible rows" as a pool fix and arithmetic says that does not work: six rows × 44 cells × 2 tile rows = 528 slots, still over 512 with no art at all, and five rows (440) only fits with no resident cover.** So the ranked ladder is (1) the atlas, which is the plan and needs nothing new measured to be affordable; (2) if it does not land, five rows *and* no resident art, which is a product cost paid in rows rather than cycles; (3) take residency option (c)/(d) and re-point on scene entry at the event price below. **At the declared 16 px pitch a whole-window repaint is 8 rows × 44 columns × 2 tile rows = 704 tile writes ≈ 874 k c at AP-0's measured 1,242 c/tile mean — 3.3 lossy periods (262,144 c), 1.5 FLAC periods (573,440 c), or 3.1 of mGBA's measured 280,896 c video frames.** That is an **event** price, not a frame price: it is paid at a boundary where the audio is re-primed anyway (a scene entry, a boot, a track change), and §10.2's upload row is where the figure lives — L4 measures it instead of taking the arithmetic. Then over-fill deliberately past the declared `tile_budget` and record that R14's panic is what fires and what the bake would have had to catch. **No new harness**: `art_probe.rs:94-135` already times per-cell `set_tile` with and without live audio, through the `Fold` accumulator (`probe_hw.rs:307-345`) and the timer2/3 `Counter` (`examples/e5_fetch/src/timer.rs`, `#[path]`-shared into `probe_hw`), with an 8-frame warmup and a discarded first pass (`art_probe.rs:57,196-202`); the every-frame producer shape is `ui_probe.rs:251-285`; a leg is a new `src/bin/*.rs` + a `[[bin]]` entry (`examples/album_probe/Cargo.toml:45-66`) + the two Makefile lines (`:1149-1154`, `:1164-1172`), and the verdict is the existing `[album-probe] PASS` shape. What has **no precedent anywhere in the tree**: a `set_priority` call in code (§5.1: no `.rs` in the tree calls it), a ~600-cell region change, and an intentional pool over-fill. **The row prints §10.2-style cycle columns and the `≥20 %` reserve (§10.2's reserve row) is what it is judged against**; the `tile_budget` sum (§5.0) takes its glyph term from L1/L2's face, from the declared face's own metrics read at bake (§5.5) | **`make album-probe`'s leg list gets a fourth probe bin** (the plumbing is Makefile `:1149-1154` and `:1164-1172`, the feature gate is `probe-roms` in `examples/album_probe/Cargo.toml:45-66`): legs print `[album-probe] render …` columns in §10.2's cycle form, and the row's own pass conditions are that the live-audio legs are inside the reserve §10.2 sets, that the unchanged-content leg comes back near-free (proving §5.1's "changed cells only" rule rather than assuming it), and that the pool leg shows the bake refusal firing *before* any panic exists to fire. Nothing here needs a new harness (§5.1 lists what is already built). **Landed 2026-11-25 (2b PR-(2))**: `render_probe` measured all four legs — L1/L2/L3 in the live-audio arm, L4 on the disarmed pool stand-up — L1 at 489 c for a bar cell, 963 c for a clock digit and 407 c for an unchanged cell; L2 at 550,437 c for 600 cells (917 c/cell, 2.10 lossy periods); L3 at 73 c for the priority store and 212,386 c for the switch; L4 at 856 refs / 460 slots / one slot at 45 refs, hidden scene resolves. **Two of this row's expectations were corrected by the measurement rather than confirmed by it:** the unchanged-content leg is 83% of a changed cell, not near-free (the early-out still pays `add_tile`+`remove_tile` before comparing, `regular_background.rs:360-378`), and the switch's price is the commit (mean 212,102 / max 280,360 c), not the "tens of cycles" write. The over-fill leg is the bake's, and already fired by `--layout-self-test`; the probe prints the stand-up that *fits* rather than hunting R14's panic. |
| **AP-3c** | **Presentation path (2a investigation, 2b implementation unless AP-3c forces it earlier): while audio is playing, the main loop does not block on a display `commit`.** §5.7 is the policy; this row is the witness. Paired L1 arms: **(A)** the current BG foreground + event-scoped `commit`; **(B)** the chrome as OBJ (title/clock/state word/icons/bar, ~56 OAM) with a double-buffered OAM shadow presented by DMA3 at VBlank; **(C)** B under a 60 Hz synthetic chrome load. Measure main-loop pass cycles and `paint.due` cadence, `late`/`underflows`, the VBlank/ISR/DMA cost, and the dirty-screenblock commit cost; the same feeder and `[album-probe] PASS` shape, headless on mGBA and on AP-7a's cart. The list stays BG (30×8 = 240 > 128 OAM); the chrome OBJ is the first candidate, the list is not. **2a's product ROM still draws no OBJ**; if AP-3c shows the 1 Hz/event commit is already a producer risk, the chrome OBJ + VBlank path is a 2a follow-up and the 2a row/§5.1 note are amended there; otherwise the implementation is 2b's first commit. | `make album-probe`'s paired arms (the feeder, the `[album-probe]` columns and the `PASS` shape exist; the probe bin is the new part). A green is the control/OBJ difference in the log, not a screenshot. The OBJ tile/OAM/VPAL total is printed and AP-6 audits the viz share. **Landed 2026-11-25 (2b PR-(2)'s widened `present_probe`)**: arm A is clean — commit mean 12,222 c (2.1% of a period; agb's fast path, no halt), 87 periods, `late_max=0`, zero underflows — so **this row does not force the OBJ chrome inside 2a** and §5.7's "otherwise it waits for 2b" applies. Arm B (OBJ, no commit) is +1.1% of A's pass at the 1 Hz cadence; arm C (60 Hz load) is +24.8%, 12,686 c/frame in the loop plus 2,026 c/frame in the VBlank handler (~5.2% of a video frame), with zero underflows and zero lateness. Those are the prices 2b PR-(5) and AP-6 build against; the hardware leg is AP-7a's, as R3 requires. **Open for PR-(5), added 2026-11-25 with PR-(3)'s scoping (the finding, not a new phase):** the probe prices the commit at both ends — 12,222 c on the fast path (≥2 vblanks between commits) and up to 280,360 c when one has not elapsed (`render_probe`'s L3 dense commits: mean 212,102 c) — but it has **no frame-paced BG arm**, and arms B/C present OAM only, with a static BG. So PR-(5) owes **arm D** (three BGs, N cells changed per frame, commit every frame, the same producer/`late`/underflow ledger) and the **dirty-screenblock-at-VBlank leg** the plan says replaces the commit. Until those land, a per-frame BG viz is priced by L2's arithmetic (480 cells ≈ 440k c ≈ 1.57 video frames), not by a measured arm. |
| **AP-4** | `ART1` is reproducible byte-for-byte from the source PNG, re-reads through the shipping parser, never exceeds its tile/palette allocation (linker map + VRAM audit — **naming the `bleed=true` arm, whose 600 tiles agb's regular pool cannot serve at all, so the audit is of pool state and of the variant's `tile_budget` — §5.1, R14**), and refuses an off-palette source image. From **2b** it carries two more fixtures: **control art** (every declared icon slot has authored pixel art in it, and Play/Pause are distinguishable by shape alone with colour removed, §5.3, §5.6) and the **occlusion report** for full-bleed (which rows sit under the bands, what fraction of the pixels are never seen, and a refusal if a declared safe area is cut, §7) | `make album-art-check` (**2b**). `make album-preview`'s **4×** image for tile/dither inspection, with its **1×** image as the legibility leg (§5.6); the 8bpp alternative judged side-by-side on a real screen, not a diagram (§7). **The machinery this row witnesses is already built and pinned in probe form** — `ART1.md`'s byte spec and 5-art golden corpus, `art1.rs`'s reader, `art_pin.rs`'s pins, `art_probe.rs`'s decode/paint legs, `scripts/album_probe_art.py`'s generator — so the byte-for-byte and round-trip legs are extensions of what `art_pin` already does; **what has no witness at all is a real cover** (no image file outside `target/`; `album.toml` has no cover field — R15), and the 600-tile leg is new work for the same reason |
| **AP-5** | `album.toml` + WAVs → ROM end to end; two runs → identical bytes; every refusal in §9 fires with its message — **the Phase 3 set; the four §9.0 refusals that land in Phase 1 are witnessed by AP-1** | `make album-bake-check` |
| **AP-6** | Each viz, alone and paired with art, holds `underflows = 0`, stays inside its §10.2 line, and leaves the audio cost breakdown unchanged vs the `none` control arm | `make album-viz-test` + on-target timing, `none` as control. **The pool a sprite spends from is not the pool this file budgets** (2026-11-20, §5.1): OBJ tile data has its own 32 KiB run-time-allocated pool at `0x0601_0000` — **1,024 4bpp tiles, 512 at 8bpp** — and agb selects hardware mapping mode 1 for it (`object.rs:30`). Nothing in the plan declares an OBJ-side budget, and no BG-side `tile_budget` refusal (§5.0, R14) would ever see a sprite overrun: **declaring that ceiling and auditing against it is this row's**, done before A9's badge is designed rather than after, and its cost is counted per sprite frame rather than per map cell. **The §5.7 chrome reservation is part of this audit:** ~56 OAM entries, ~202–206 OBJ tiles (~6.3–6.6 KiB), one VPAL bank, leaving ~72 entries and ~25 KiB for the viz; the per-line OBJ budget is the scarce number, not OBJ VRAM. **Its starting coefficients are now measured (2b PR-(2), 2026-11-25):** 226 c per changed OAM entry in the main loop, 236 c per entry on the sparse VBlank path, 2,026 c per frame for a full 1 KiB present (so a viz should batch to the full DMA above ~9 changed entries), and a full 56-entry chrome rewritten every frame costs 12,686 + 2,026 c/frame ≈ **5.2% of a period** before any viz sprite — 2.6× §10.2's 2% UI row, which is why the chrome's cadence stays event-scoped and the viz's budget is its own. On the BG side, L2's 917 c/cell puts a 480-cell region repaint at ~440k c ≈ 1.57 video frames, so full-region BG animation is a ≤10 Hz effect; **arm D (§13 AP-3c) is the leg that would turn that arithmetic into a frame-paced measurement**, and it belongs to this row as much as to PR-(5). |
| **AP-7** | Hardware: GBA 001 + GBA SP, two runs each, `none`/`bar`/`skyline`/`game`, with the map-file memory report attached and `.iwram` growth flagged; **and the run's verdict must be legible without colour vision (§5.6)** — a pass a verifier cannot perceive is not a witnessed pass | two units, a notebook, and a `flac-docs/history.md`-style record. **Partial witness already held (2026-11-16, PR-3a):** one unit, `album-player.gba`, track 1 end to end — audio played clean, no stutter and no non-encoder artifacts, reported by the caller from hardware. That is the first time this application's path has been heard; it is not AP-7, which needs both units, the viz/game arms and the memory report |
| **AP-7a** (informal, but name it) | **Hardware legs already witnessed by a human, one unit.** (1) PR-3a, `album-player.gba` sha256 `62271aa2…33e1c5` — 30 s of track 1, audio clean, no stutter, no non-encoder artifacts. (2) PR-3a fix 4, `album-player-fast.gba` sha256 `3fc1f184…310a25` — audio correct, **then a blue screen reading `PASSED`**: the §5.6 scheme witnessed on hardware by the colourblind verifier it was revised for. Both are one unit, one track, no keys, no UI — a listen and a legible verdict, not a gate closure | the caller's notebook, 2026-11-16. **(3) PR-3b, 2026-11-17, first hardware session with the keys live** — on `album-player-fast.gba`: a full unpressed playthrough of tracks 1→2→3 ending in `PASSED`; **Left/Right skip between tracks as expected**; **Start pauses without a click, at roughly the instant it was pressed**; **Down restarts the current track**; audio clean throughout with no glitches. That is the first witness this application has had of the two legs §13 AP-1 declared unwitnessable — *a physical press works* and *a fade is click-free to an ear* — and it is why the fade-deferral bug (§4.4's "a fade must be served, not merely handed over") was worth a red-proofed test rather than a comment. **The same session found R13**: heavy interactive use (repeated Left/Right, more than three Right skips, repeated pause) ends the run in `FAILED`, while the two calmer patterns pass. Still one unit, and the 214/229 ms boundary question is still the caller's to judge, not a gate's. Recorded here rather than only in a PR body, because "did a human hear it / read it" is the one claim this repository's emulator cannot make for itself (§10.5, R3). **(4) R13's fixes, 2026-11-19, third hardware session with the keys live** — on `album-player-fast.gba` sha256 `76bfbba6…31755`, built by `make album-player-rom` from a throwaway merge of `main` (`e952a1c`) with this PR's head (`1676608`): the caller **cycled aggressively between tracks and played/paused repeatedly, then let the final track play through to the end** — the pattern that had *consistently* ended `FAILED` in leg (3) — and reports that it **now passes**. That is the cart witness AP-1's status row was waiting for, and the reason its status changed on the same day it was asked for; the ROM was chosen to be the one that found the bug (same target, `make album-player-rom`), and the earlier build that reproduced it is preserved with its sha at `/home/sam/tmp/r13-hardware-roms-2026-11-17/` (`1617ed2d…0fcb`) so the before/after pair is auditable rather than asserted. Still one unit, still no memory report, and the emulator never reproduced the symptom: **AP-7 stays open, and so does the judgement call about what a human-perceptible pause→resume gap is** (R13's timing question, Q18's audible one) |
| **AP-8** | Reach: the web config produces an `album.toml` the CLI bakes unchanged; the game under the watermark rule never admits an underrun | `make album-web-check` (headless, config diff) + AP-6's arms with `game = on` |
| **AP-9** | **The seam holds:** the same screen/layout suite passes against `backend-flac`, `backend-uli2` and `stub`; no module outside `src/backend/` names a codec crate (a build-time fact, not a code-review hope). **Phase 1 read this as: all three arms compile, and the suite *runs* on `stub` + `flac`, with three-way runtime parity deferred because the lossy backend had no real-time decode (U4) and no in-tree encoder (R7). Half of that deferral lapsed on 2026-11-19: U4 and U5 are met (§2), so what still keeps the `uli2` arm from *running* is the asset path (W2, R7/Q3) and this app's own `src/backend/uli2.rs` (W3) — Phase 5's work, and neither of them a decoder's or encoder's fault now that S8 is byte-exact. The arms that cannot have assets still cannot be blamed for it; from here the excuse is the asset wiring and our own adapter, never the codec's real-time budget** | three `cargo` arms + a dependency check on the app crate's module graph |

Aggregate: **`make album-check`** = AP-0…AP-6 plus AP-9 on host+target, mirroring
`make flac-check`; the codec tracks' gates (`flac-check`, `uli-*`) stay the
authorities on decode, and an app change never re-decides them.

---

## 14. Risks

| # | Risk | Why it is real here | Mitigation |
|---|---|---|---|
| R1 | **The album-length build waits on the lossy backend's decode gate, and then on its playback gate** — **discharged as written, 2026-11-19** | It used to read: U4 not met (2026-11-15: 105% worst / 89% mean of the period), with **U5** (zero underruns, clean period audit, mGBA then hardware) and **PR D** (output stage, incl. the speaker fold-down this app exposes) behind it. All three are now met: **U4** at gate total **86.27% worst / 76.53% mean, 13.73 points of margin at the worst frame, `frames_over_period=0`, zero starved periods over 16,552** (mGBA, WAITCNT 4/2, **no UI in that arm**); **U5** on hardware (GBA SP, 2026-11-16, three player ROMs, two runs each, 0 underruns — one unit only, GBA 001 unavailable, and its 1 Hz-redraw defect is open); **PR D** as `crates/uli-playback` — the `PcmSource` seam, the int8/fold output stage, and a player over `gba_playback::RingFeed`. *(corrected 2026-11-19 against [`ULI2.md`](ULI2.md) §Gates; the 2026-11-15 corrections of this same row are recorded in §2's row, and this file has now been wrong in both directions on this point — which is why it cites the ledger instead of restating it)* | the row stays rather than being deleted, because the risk did not disappear, it **moved**: an album-length `uli2` build still cannot be baked from this checkout (the in-repo encoder is byte-exact, but the asset path is W2/R7, Q3), and the margin that gate existed to win is now the space this application's UI has to live in (R1b, §10.2). The app was built and gated on the lossless backend throughout, which is why the switch is a build flag rather than a rewrite; Phase 5 is app-side work and touches no screen. The seam is still the mitigation, and the risk is still only realised if someone writes `if flac` in the UI |
| R1b | **The app's UI cost is an input to the codec's own gate ceiling** — and as of 2026-11-19 it is the **only** term missing from that ceiling | U4's ceiling was "measured, not assigned", and now it *is* measured: the gate total (decode + convert + IRQ + DMA) is **86.27% worst / 76.53% mean with no UI in the arm**, leaving **13.73 points at the worst frame** (23.47 at the mean), and ULI2's own §Gates names the remainder: *"Whether that margin survives the UI is the open question."* The missing addend is this application's, and two instruments bracket it without being subtractable: mGBA / no-UI at 86.27%, and the lossy player on **hardware** at **94.52–95.89% worst with one 1 Hz text redraw producing 104 late re-arms** ([`ULI2.md`](ULI2.md) → U4, U5). One text redraw a second already reaches the DMA's grace there; an album shell with screens, art, a bar and a visualiser is a much larger addend than that. **Re-ruled on the design side 2026-11-20, precisely because of this row:** the shell is no longer five screens but **one surface with a persistent chrome** (§5), so the addend is itemised **per element and per cadence** (§10.2) instead of quoted as a screen average — a bar tick, a 1 Hz clock tick and a title rewrite on a track change are three different numbers, and R1b's 104-late-period arm is the smallest of them taken to failure. **And the target this row used to leave implicit (review, 2026-11-20): most scenes need no periodic redraw at all, and the one that does has a bounded worst case** (§5.2's cadence rule, §5.5's table) — a `mm:ss` clock's most-changed cell is the seconds ones digit, so its worst case is **one digit cell per second**, which is 6–10× less than the *row* that was redrawd late on hardware, and the bar moves at most once per declared cell, and the cell count is a variant field, not `BAR_CELLS = 32` (§6, §9.0's `bar-cells-*` rows), not once a frame. Interactive scenes are the exception and they own their own rows (viz: AP-6; the game: AP-8). So R1b's shape is: the smallest addend, measured to failure on hardware, **and named as the one addend 2a's element-addressed shell is designed not to contain** — AP-3's clock-cell figure, and AP-7a re-running that arm, are what would show it, and neither has been run. AP-3's records are per element and per switch for exactly that reason: a number attached to a screen cannot be reused by the next scene, and a number attached to an element can | AP-0 has handed across the basic-UI line (**1.89%** of a period, plus feeder 0.92%) and AP-6/AP-8 owe the viz and game lines, in the same units the codec ledger uses; §10.2's targets are explicitly targets until then. The mechanism to design around is already this file's: drawing driven off the **vblank latch**, never the main-loop pass (§4.4's producer-cadence finding) — ULI2's redraw is that same failure mode one layer up |
| R2 | **The lossless backend cannot hold a whole album** — the shipped clip carries 354.8 kbit/s (439,396 B for 9.91 s, `spike_l0.gfp`) ⇒ arithmetic puts ~12.6 min of that material on 32 MiB | that clip is a perf fixture, not a measure of real music, and no amount of FLAC tuning makes lossless reach album length at this depth | **now measured on real music, not inherited**: 364.0 kbit/s mid-album / 340.8 kbit/s for the head of `bioluminescent.wav` ⇒ **~12.3 min on a 32 MiB cart**, and the reference master is 4.3 min of that (Q15). **Phase 1's default album is real music**, so the first bitrate this app quotes for a real master comes out of the Phase 1 pack report, and the bake tool reports minutes-per-cart honestly. This risk is also the *reason* the lossy backend is not a nice-to-have: an album player that can only hold an EP is why §4.2b exists. Quote a measured number from here on, not 354.8 kbit/s |
| R3 | **Cartridge bus contention**: the app adds ROM *data* traffic (VIZ reads, tile uploads, art) around decoder *instruction* and audio-data fetches | mGBA does **not** model the ROM prefetch buffer, and this tree already had to arbitrate emulator-vs-hardware fetch once (`examples/e5_fetch/`); ULI2's method note says on-target numbers stay unclaimed until hardware | the ≥20% reserve; a hardware run per phase from Phase 5; the watermark rule keeps the risk off the audio deadline |
| R4 | **A skip that sounds broken**: too long a silence, a pop at the splice, or a re-prime that starves the ring | §4.4: 1.23 s is queued and must be dropped, then replaced while the DMA keeps consuming. **AP-0 found the stronger version of this risk: without a ring truncate the drop cannot be faster than one period per queued chunk, so at the ring's ceiling the skip is 1.33–1.81 s no matter what `SKIP_PRIME_CHUNKS` is (§4.4, Q13b)** | the fade/drop/re-prime sequence with `SKIP_PRIME_CHUNKS` chosen by the host model, not guessed; AP-1 measures the gap and holds `underflows = 0` across it; and Q13b ruled in the app's favour, so step 2 becomes a truncate rather than a drain |
| R5 | **Silent failure modes** (a bad blob, an album longer than the manifest's own arithmetic, an off-profile asset) | the bake tool is the only place with enough information to notice, and it is not currently the place that decides | refuse at bake time; show a named on-screen verdict ("DATA ERROR") at run time; a decode error skips to the next track and stops at the album end; the 32-bit session clock (§10.4) so a long run is still measurable |
| R6 | **IWRAM is the scarce RAM** on the album-length path, and it has been mis-sized twice in this repository's own documents | the plan of record now measures **`.iwram` = 26,496 B of 32,768 with a stack high-water of 948 B (bench) / 1,988 B (gate)** — U6 met for the decode/player arm (2026-11-19), i.e. **≈4,284 B of usable headroom before any UI is linked**, which is ~1,900 B tighter than the figure this row carried (2026-11-15: 25,548 B placed, ≈2,960 B stack, 6,196 B of headroom), itself a correction of the PR B-era "~22.6 KiB port + 15.0 KiB stack" reading | AP-7 flags `.iwram`/stack growth from the linker map, not from a remembered number; new non-hot code stays out of IWRAM by rule; the "a few hundred bytes of RAM growth breaks playback silently" failure mode is one this project has already recorded, so the map diff is a gate and not a courtesy. U6's own condition is the part this app cannot skip: the figure is met **"with the UI and feeder linked"** only once something with a UI in it is measured, and none of the codec's arms has one |
| R7 | **The lossy backend has an in-repo, byte-exact encoder but no asset path from this checkout**, so `BACKEND=uli2` cannot be baked here today — **and since R1's decode/playback half closed, this is the sole blocker on that backend** | `make gafp-pack-check` covers FLAC end to end; nothing in `scripts/` encodes the lossy format. Every `.uli` the tree's gates read comes from the C encoder run **off-tree**: `make uli-album-asset` / `uli-cross-asset` / `uli-two-track-asset` shell to `$(GBA_AUDIO)/encoder/prep.py` and `$(GBA_AUDIO)/build/uli_enc` and refuse without `GBA_AUDIO=` pointing outside this checkout | the bake tool calls encode behind one indirection with a declared interface (§9); the app is built and gated on the lossless backend in the meantime, so this blocks a *backend*, never the application (Q3). **Status change 2026-11-19:** the Rust encoder stopped being an unscheduled wish and became the codec track's **last planned milestone** — [`PLAN-OF-RECORD.md`](crates/uli-lite/PLAN-OF-RECORD.md) → "The encoder — the last missing piece" carries its requirements, four settled determinism decisions, its slices and gates, and an acceptance question answered by measurement. Until a slice lands, a `uli2` album ROM means either running that C encoder by hand with `GBA_AUDIO=` set or not shipping it — **amended 2026-11-27: the slices and S8 landed (S1–S8 in `crates/uli-enc`, byte-exact to the recorded oracle), so R7's remaining half is wiring the bake onto it (W2), not writing an encoder; the app-side adapter is W3.** §2.3's rule is unchanged either way: the assets stay build output |
| R8 | **Scope**: fifteen features, two backends, a UI toolkit, an art pipeline, an analysis pipeline and a game | the natural failure is a demo that does all of them badly | phases are ordered so each one is *usable*, and A9/A14/A15 are labelled reach; §16 lists what we will not build |
| R9 | Font/charset limits and licensing | 96-glyph ASCII subset; OFL fonts; ULI2 licensing (U7) unresolved | fold at bake time and print offenders (Q9); the app is not the vehicle that forces a publication decision |
| R10 | **The app's foundation includes a spike directory** — its feeder (`crates/gba-playback/src/playback.rs` since 2026-11-15; `examples/flac_spike/src/playback.rs` before that) and its cycle counter (`examples/e5_fetch/src/timer.rs`) live where the tree's own purpose statement says code goes to be thrown away | `flac_spike` is the perf-gate `ctrl` arm and `e5_fetch` has no Make target any more; a product feature implemented there has no owner, and Q13b's ownership looked genuinely ambiguous for exactly this reason. **The feeder half is closed** (Phase 1 PR-1) | §2.2: lift the feeder + checksum + counter into project source before Phase 1 links them, keep `flac-check`/`product-play-test` as their witnesses, and re-baseline the shipped ROM bytes the move touches. Feeder + checksum **landed** in `crates/gba-playback` (2026-11-15, `make playback-test`); the `e5_fetch` counter's *method* is landed as `gba_audio_hw::CycleCounter` (2026-11-16, PR-3a fix 2 — it is what measured R12, and it is borrowed-handle state-free rather than the spike's statics), so nothing new `#[path]`-includes the spike; `e5_fetch` and `uli_bench` keep their own copies until their owners retire them. Phase 0's probes lean on them on purpose and are throwaway |
| R11 | **A generated asset can go stale** — the policy that keeps album audio out of the repository (§2.3) means a build can silently link *yesterday's* blob | this is the price of the policy, not an accident of it: `master.gfp`, the rate clips and every future `album_player` asset are build output, and build output outlives its config more often than source does | `gafp_pack.py --emit-pin` already writes the sha256 pins the **ROM** checks its own embedded blob against at boot, so a mismatch is a named verdict (R5's "DATA ERROR") rather than a plausible-sounding track; `make album-rom` depends on the pack step so a build cannot skip it; `make clean` removes the generated paths; and AP-1's pack-twice check plus AP-5's byte-identical-ROM check make "generated" mean **regenerable**, not "whatever is lying around" |
| R12 | **A verification cost proportional to the cart, paid before the first sample** — the album's blobs are 38× the instrument's, and an audit written for a 394 KB clip does not scale to a 15 MB one | It happened, on the first album ROM: 52.56 s of CPU of boot-time asset folding, reported by a human's ears as "no audio for ~55 s" (§10.5). **No gate could have caught it** — headless mGBA models no ROM wait states (R3) and ran the whole image in ~4 s, so every green in the tree was compatible with the product being unusable | §10.5's rule; `meta::boot_audit_bytes` asserted `< 1 MiB` and linear in tracks, not in asset bytes, so the sweep cannot come back silently; the ROM prints its boot phases in cycles, which is the only timing unit that survives the emulator; and Phase 5's hardware runs are the witness R3 already names — an emulator green is not a usability green |
| R13 | **Interactive input ends a run in `FAILED` where a scripted run passes — and a cartridge `FAILED` names nothing the player can read.** Reported 2026-11-17 from hardware: an unpressed playthrough passes, and two skips then playing out passes, but repeated Left/Right, more than three Right skips, or repeated Start/pause ends the final track in `FAILED` (**root cause now known and removed: three verdict-side mechanisms, itemised in the last cell — the observation itself was never reproducible headless**) | Three suspects, and they are *not* equivalent, which is why this row exists rather than a loosened bound: (a) a real transport defect under rapid transitions; (b) a **counter that saturates** — `EDGE_LOG = 16` (`album_play_main.rs:688,833`) and the summary caps `skip: … limit=16` / `pause: … limit=4` are fixed sizes chosen around one scripted scenario, so a longer human session can overflow a *measurement* and fail a verdict with no audio sample wrong; (c) **bounds that encode the script rather than the product** — `dropped` is gated `1..=19040` on a pressed skip, but a second rapid skip lands while the ring is still shallow from the first and can legitimately drop nothing (both the implementer and the review named that line). §10.5/D7's discipline is that a green must not be softer than its witness; the mirror image is this row: **a red must not be louder than its witness either** | Reproduce headless with interactive-*shaped* scripts (rapid skips both directions, skips beyond the scripted count, repeated pause), then classify every tripped term as (a)/(b)/(c) **before** touching any bound — a number only one script satisfies is a scenario, not a property, and belongs in the test that owns that scenario. Separately: **print the failing term's name on the screen**, not just `FAILED` — a cart run cannot be diagnosed by the only person who can run it, which is §5.6's lesson (a signal the verifier cannot perceive is not a signal) reapplied to diagnosability rather than colour. **Status 2026-11-17 (fixes 1–2 of this row):** three false-FAIL mechanisms found and removed — ledgers summed over capped print arrays (a session longer than the print log failed with `underflows = 0`); `dropped >= 1` gated on a pressed skip when a press landing after a visit has drained legitimately drops nothing; and **bounds calibrated on mGBA**, where `MAX_PASS_FRAMES = 6` was *exactly saturated at 1×* — so every cartridge, whose ROM fetches cost cycles the emulator does not model (R3), was arranged to fail `reaction` with nothing wrong. Bounds are now in **cycles against the run's own measured producer frame**, capacity is a census line rather than a verdict, and the screen prints **which** term failed (`FAILED REACTION`) plus `PAUSED` when the run is frozen at the verdict. A property fuzz (2,048 randomised press sequences, 42,057 presses, both wire-wait and press-anytime regimes) found **no transport or ledger violation** — which rules out class (a) for everything the emulator can express, and proves nothing about a cart. **Status 2026-11-19: closed on the instrument that found it.** The caller's unit, on a ROM built from this PR (`album-player-fast.gba` sha256 `76bfbba6…31755`, i.e. the same target that reproduced the bug): the heavy pattern — cycle hard between tracks, play/pause repeatedly, then run the last track to the end — **passes**, where the 2026-11-17 session had it end `FAILED` consistently (AP-7a (3) → (4); the before-image is preserved with its sha at `/home/sam/tmp/r13-hardware-roms-2026-11-17/`). The emulator still does not reproduce it, so the closure rests on the caller's report and not on a gate — **which is precisely why the row stays rather than being deleted**: it is the record of a class of defect that only a person holding a D-pad could find, and the ledger's MIXED state was the honest way to hold it open until that person could close it. What makes the fix durable is not the report but the shape: every bound now comes from the run's own measurement (`note_pass_cycles`, `pass_bound_cycles`, `producer_max_cycles`, `take_to_handoff_bound`, `pause_to_freeze_bound`) instead of an instrument's frame count or a constant someone chose, so a cart cannot be failed for being a cart. R13's rule, kept: **a red must not be louder than its witness either** — a counter that saturates, a bound that quotes the instrument it was measured on, or a term that demands an event the product does not always produce will fail a correct cartridge forever |
| **R14** | **The tile pool runs out at run time, on the ROM, as a panic** — and the plan's own scene set is what pushes it there | Two numbers matter and both were mis-stated in this file until 2026-11-20: agb 0.25.0 serves a regular BG from **512 4bpp tile slots in total** (with 510 more reachable only through `alloc_for_regular`'s fallback into the affine pool, and a *panic* — `"Ran out of video RAM for tiles"` — when both are empty), and it allocates **one whole 2,048 B map per background with eight of them existing at all** (`display/tiled/vram_manager/tile_allocator.rs:25-46`, `display/tiled.rs:292-300`). A region canvas (480–510 tiles) is most of that pool by itself; full-bleed art (600) cannot be served from it at all; and the residency ruling (§5.1) asks for **every scene the variant allows to be resident at once**, glyph cache included | It is silent in a screenshot and loud on hardware, which is the worst arrangement. Mitigation, in order: **a per-variant `tile_budget` declared in `layout_spec.toml`, summed and refused at bake time** (the same shape as the art-box and title-fit refusals, §5.0/§5.5), so a variant that cannot hold its own scene set never compiles — and note the sum's terms are not the face's glyph inventory: agb's text path allocates tile data **per rendered glyph** (§5.1's refcount note), so a standing term is a **range, not a number**: upper bound = the glyph cells a frame can display at once (the declared boxes ÷ the declared face's advance widths, times the tile rows its line box spans), lower bound = the distinct glyph images actually on screen (bounded by the face's inventory × tile rows per line, because agb refcounts by *(tileset, index)* identity and repeated glyphs share one tile — **reachable only through one shared `TileSet`, i.e. the static glyph atlas of §5.5; on per-cell `DynamicTile16`s the lower bound collapses onto the upper, which is exactly why the atlas is a 2a deliverable and not an optimisation**). §5.1's measured table gives both for every face in this tree at every grid-aligned raster, and AP-3b's pool leg (L4) is the leg that says where in that range the real number is — plus whatever a hidden scene's map still references, which is the residency question (R14's own §5.1 options). Which is why "the face is a declared field" is a budget fact and not a taste note: **the bake has to read the face's metrics** (fontTools/`pyftsubset` are the tools the face convention already uses, `assets/fonts/README.md`) to compute that term at all; **AP-3's pool audit** records actual allocation state per variant (and, for Phase 4, the separate OBJ pool §5.1 now documents); and §5.1's four residency options are ranked there rather than improvised in `scene.rs`. Kept rule, from §5.1: **the plan's VRAM ceiling is on tile *data* and its time ceiling is on map *cells* — they are different currencies and this file used to spend one as if it were the other.** Its own failure mode is the opposite one: a variant refused at bake time because the arithmetic was guessed — a `~64`-tile prose figure, a `~287`-slot one, or a glyph term read off a font file's glyph *inventory* instead of computed from the declared face's advance widths and line box against the declared boxes and pitch (§5.1's measured table, §5.5) |
| **R15** | **There is no real cover in this tree, and the plan's art leg has no input.** (named 2026-11-20) Found by inspection: a `find` for `.png/.jpg/.jpeg/.bmp` outside `target/` returns nothing; `assets/` holds only the OFL licences and two TTFs; `examples/album_player/album.toml` has no cover field; and `examples/album_probe/README.md:253` says its own piece outright — *“there is no album cover in this tree”*, and `examples/album_probe/ART1.md:88-91` admits the other half — *“no image file in the repository and no Python imaging library in the toolchain”*. | Why it matters: every claim §7 makes about dither quality, palette count, legibility at 1× and the full-bleed ruling is a claim about art nobody has seen, and §5.6's own rule is that art is judged on a real screen, at 1×. The single witness that exists is a synthetic 120×120 gradient card. | What the plan does about it: 2b's art leg starts by supplying a cover (or ruling which album's art this product is allowed to carry) — §11's 2b row now says so; the tooling is otherwise already built in probe form (§9's `album_art.py` row lists what to generalise), and **AP-4's real-cover witness (§13) is the leg that closes this row**. **Updated 2026-11-25: the input now exists** — a 3000×3000 cover for the fixture album is at `assets/images/bioluminescent.jpeg` (17,473,190 B, most of it a Photoshop APP1 preview chain), supplied untracked. What remains is not sourcing but the art PR's decisions: whether the source image is tracked as `assets/sound/`'s master is or only its `ART1` output, how the preview chain is stripped before it is committed, and AP-4's witness on the rendered cover. The row is not closed until that leg exists |

---

## 15. Open questions

Numbered in this file's own space (the codec tracks keep theirs).

| # | Question | Provisional ruling |
|---|---|---|
| Q1 | Which backend is v1's default *artifact*? | **The application supports both and favours neither** (§4.2b, AP-9) — but the reason for favouring neither has changed, and so has the arithmetic behind the choice. **U4 and U5 are met** (2026-11-19 / 2026-11-16, [`ULI2.md`](ULI2.md) §Gates), so the lossy backend is no longer a *near* candidate for the album-length image: at **12,735 B/s ≈ 101.9 kbit/s ⇒ 43:31 of audio on a 32 MiB cart** ([`ULI2.md`](ULI2.md) → "Capacity: how much audio fits a cartridge") against R2's measured **~12.3 min** for the lossless one, it is the only backend that can hold an album at all. What keeps the lossless path the default today is specific and honest, not a codec judgement: this tree **cannot yet bake lossy album assets from a fresh clone** (W2/R7, Q3), the app has no `src/backend/uli2.rs` until Phase 5 (W3), and U5's hardware witness is **one unit** (GBA 001 unavailable) carrying an open redraw defect. So: the default stays lossless until the app's `uli2` arm exists and builds from a fresh clone; the lossless build stays forever as the A/B control. Either way this is a shipping decision about which artifact `make album-rom` builds by default, not a design assumption — nothing in the app may depend on it, and AP-9's three-arm parity is what keeps that honest |
| Q2 | App-side manifest, or extend a codec's container with album fields? | **App-side `ALBM1`, for both backends equally** — the decoupling argument is §4.3: an album feature must be available to both codecs or it is not a feature, it is a branch. Revisit only if a future format revision is happening for codec reasons anyway. |
| Q3 | An in-repo encoder for the lossy backend? | **Settled for v1 by the codec track** (2026-11-14): the encoder is used off-tree to produce assets, nothing is built from it and no C is vendored. This app therefore bakes from a *pre-encoded asset* on that backend. A Rust encoder was then a codec-track milestone; **it has since landed and is byte-exact** (`crates/uli-enc`, S1–S8, 2026-11-26) — what remains is W2 (wire the asset targets to it) and W3 (this app's asset path/adapter), not an encoder or exactness question. Nothing in Phases 0–4 blocks on it. |
| Q4 | Gapless playback? | ULI2 is gapless by construction (one stream, track boundaries are positions). The FLAC path is N blobs and will gap **audibly** unless the ring is bridged at a track boundary. **v1 rules: the lossless path has inter-track gaps and we say so — with the number now: 214 / 229 ms at a track-end boundary (§4.4), of which four periods are the `SKIP_PRIME_CHUNKS` cover and the rest is tail padding plus two starved periods.** A pressed skip is a larger gap by design (306 ms of chain, plus the destination's own lead-in). Bridging stays a Phase 5 item, and it is the first feature that must not assume a decoder can be pre-rolled into the old track's queue. |
| Q5 | Seek inside a track? | Deferred past v1. The UI affordance (a scrub screen) is the work; granularity is whatever the linked backend offers, which is why it arrives as a `PcmSource` **capability** (§4.2) and never as a constant in a screen. |
| Q6 | Does `bands` earn its ~243 KB in v1? | Probably not: `loud`+`beat` is 91 KB and drives two of the four visualisations. **v1 ships `loud`+`beat`;** `bands` is a switch, priced in the bake report, and the spectrum view is its own PR. |
| Q7 | Art box size and default mode? | **Re-ruled 2026-11-20, and the question changed shape instead of being answered.** The old answer — "120×120 `cover` for art-only; 160×96 when a viz is on" — is superseded twice over. *Size:* it is now a field of the selected layout variant, and the art tool validates against a **derived legal list** (§5.0, §7), so "how big may the art be" has no answer in the abstract — it has an answer per variant, and an album that asks for anything else is refused before a line of Rust compiles. **(Ruled 2026-11-20 while scoping 2a's first PR, so that last clause is not misread as "refused at 2a": the derived list, its validator (`album-layout-gen`) and the pool audit taken at that list's ceiling are 2a's, but the `[art]` key itself stays a `config-section` refusal through 2a (D2) — so the first build that *can* be refused for a bad box is 2b's, and the box it names will be selected from a table 2a already generated and audited, never added to one. §9.0's register row and its note carry the same split; `art-box-not-derived` is the one AP-3 bake-leg row that reports itself named-and-skipped at 2a for exactly that reason.)** *The 160×96 half died with the 96 px band:* a visualiser is now a scene that owns the region (§8), not a co-tenant of a picture. What is genuinely still open belongs to 2b, not to this document: **which legal box the shipped default album picks**, and the row's own discipline still governs that choice — *"to be confirmed on a real screen, not in a document"* — except the instrument that sentence was waiting for did not exist when it was written: it is `make album-preview`'s **1×** composite plus a hardware run (§5.6, AP-4). `mode` (`none` / `cover` / `cover+breathe`) is unchanged as a question, and full-bleed is now its own row (**Q21**) rather than a size in this one. |
| Q8 | Output profiles (headphone vs the built-in speaker)? | **In scope, and ruled into the output stage by the codec track** (2026-11-14): the decoder always produces full stereo, and the mono collapse happens *after* it, never by shortening synthesis. That is this application's layer, so the fold-down is the app's to expose (Select, §5.3) and the seam already returns the int16 it needs. **As of 2026-11-19 the mechanism is no longer only a ruling: `crates/uli-playback/src/fold.rs` (PR D) implements it.** `Profile::Speaker` decodes full stereo and folds *there*, `FoldHow::Average` = `(L+R)/2` is what ships, and `FoldHow::LeftOnly` is kept behind `--features fold-left` precisely as the non-fold-down comparison this row warned about (hard-panned content vanishes; on an M/S frame `L = M + S` is mid+side). The codec's speaker arm is measured on hardware at **95.89% worst / 82.61% mean, 0 underruns** ([`ULI2.md`](ULI2.md) → U5), **but the two folds have never been differenced against each other, and no fold has ever been priced inside *this* application's gate total.** So the ruling stands and the ask narrows: ship whichever the measurement supports, take it on the app's own path with its UI linked, and record its cost in the gate total (R1b) |
| Q9 | Font coverage / non-ASCII titles? | ASCII-fold at bake, print offenders. Extending the subset (Latin-1) is cheap; anything beyond that is a separate decision. **(This row stays about *coverage*; which *face and size* draws each element is a different question and is now **Q19** — the two used to be one row because there was only one face and one screen.)** |
| Q10 | Resume position in SRAM? | Optional, cheap (agb has `save`), and needs a save-type header decision. Phase 5. |
| Q11 | One album per cart, or a picker? | **One album per ROM.** A picker is a filesystem and an allocator, and the encoder's album-wide rate allocation makes a mixed cart a worse product. |
| Q12 | Mono vs stereo, and 8-bit vs int16 tap for viz? | **Stereo is the product target, ruled 2026-11-14** — fidelity is the project's purpose and is not traded for CPU headroom. The app receives int16 from `PcmSource` and owns the int8 conversion at the output stage, so a speaker fold-down or dither change never reopens the seam (§4.2). |
| Q13 | Where does the codec track's PR D stop and this application start — the output stage, the feeder, **and whether the UI reads the codec's tables or the app's manifest**? | Overlapping ground, named rather than papered over. The feeder is already shared in this repo's idiom (`RingFeed` through `crates/gba-playback` since 2026-11-15; `geometry.rs` through `#[path]` to this day), and the lossy track's PR D claims "reuse the ring/FIFO feeder … int8 direct … UI from the container's tables" — that last clause is the boundary §4.3 pushes back on. **Proposal:** the feeder / convert / fold-down lives in one shared module that both the product ROM and this app link (PR D's fold-down measurement lands there), the *player* (screens, manifest, keys, viz) lives here, and the adapter cross-checks the container's tables against the manifest so both statements hold at once. One piece of that boundary has since settled on its own terms: the ring/feeder is project source the *application* owns (§2.2) — `crates/gba-playback` since 2026-11-15 — so `truncate()` (Q13b) is not a cross-track ask at all. What still needs an owner's ruling is where PR D's output stage stops and the player begins, because that decides where U5's gate gets written. **Status 2026-11-19, after the fact: PR D has landed, so this is no longer a boundary in the abstract — there are now two things called `PcmSource` in this tree.** `crates/uli-playback` ships PR D's three pieces: the adapter (`source.rs`, `PcmSource` / `UliSource`), the output stage with the speaker fold-down (`fold.rs`, Q8), and a player over `gba_playback::RingFeed` (`player.rs`) — the crate U5 was witnessed with on hardware. Its `PcmSource` is **not** §4.2's: planar int8 frames, positions per **track** (`samples_into_track`), and `track_title()` read out of the `ULA2` container, which is ULI2 Phase 2 item 3's "drive the UI from the `ULA2` tables" written in code and pointing the other way from §4.3. Two traits, one name, is a review hazard rather than a decision already taken: the proposal above still stands, the app keeps `pcm.rs` as written and its `backend/uli2.rs` *wraps* `UliSource`, and §4.3 rule 1's cross-check is what would make "driven from the `ULA2` tables" and "driven from `ALBM1`" the same statement wherever they agree. Renaming one, or scoping them apart by module, is the owner's call — and this file's next PR must not add a third. |
| Q13b | **May the shared feeder *truncate* the ring at a skip?** (raised by AP-0, 2026-11-14) | **Landed 2026-11-15 (Phase 1 PR-1).** The 2026-11-14 ruling was *yes — this is the mechanism §4.4 step 2 means, and the application grows it*, and it is now the shipped method: `RingFeed::truncate()` in `crates/gba-playback/src/playback.rs` sets `consumed = pushed` under the same critical section `take_chunk` uses, keeping `consumed <= pushed` and "a starved period consumes nothing" intact. **Witnessed:** four host tests in `make playback-test` (it returns exactly the prior `ready()`; `ready() == 0` afterwards with the producer ledger untouched; the next push delivered whole, across a ring wrap too), and — measured, not derived — the skip probe's `truncate` arm reads 3/5/9/17 periods = 102.5/170.9/307.6/581.1 ms to new-track audio at prime 2/4/8/16 (`min == max` over 20 skips each), against the 39–53 periods (1.33–1.81 s) that drain and consume-and-discard still read at the 36-chunk ceiling. **Ownership:** ours, because §2.2's lift put the ring in project source; the old "the codec track's file" framing was wrong and is recorded as such in `examples/album_probe/README.md`. **Consequence:** `SKIP_PRIME_CHUNKS` is a Phase-1 product choice — still un-chosen by this PR — because `to_new = SKIP_PRIME_CHUNKS + 1` periods regardless of queue depth, so the ≤ ~0.6 s observable is met at every prime in the sweep and what remains to decide is the length of silence the listener hears between tracks. |
| Q14 | Does the app's reaction data need the lossy backend's int16 path, or is int8 enough? | Neither, as designed: the visualiser reads the **album timeline**, not the PCM (§8). This only reopens if the runtime-analysis experiment is taken, and then it is a measured cost against the gate total, not a free tap. |
| Q15 | What is Phase 1's album made of, given per-album assets are not committed (§2.3)? | **Ruled 2026-11-15: N entries over one properly-encoded master.** The fixture repeats the same encoded `assets/sound/bioluminescent.wav` as full-length tracks titled "Bioluminescent 1", "Bioluminescent 2", …, plus at least one short **"Bioluminescent Clip 1"** so the list carries *unequal* durations, alternating clips and full tracks to scale to whatever portion of the cart a test wants. Real music, zero new tracked bytes, and it exercises the source → ROM pipeline Phase 1 exists to build instead of a synthetic stand-in for it. The consequences are measured, and they are this ruling's to handle: |
| Q16 | Who measures the decode rows an album actually hits — FIXED-1/2/3 at 2048-sample frames — and does Phase 1 wait for them? | **Recorded, and not the app's to close** (§2.1: this file reopens no codec decision). `MEASURED_DECODE` holds one row at the product block size, `(2048, "fixed", 0)`, and real music contains no order-0 frames (§10.2b) — so every album-shaped build packs under `--master`'s blanket waiver instead of against a measurement. **The ask of the FLAC track:** three hardware rows for `(2048,"fixed",1/2/3)`, taken on frames whose `frames_sha256` the album bake can reproduce, so the guard can check real music against real numbers and retire the waiver on the product path. **Phase 1 does not wait:** it budgets against C2's measured real-audio aggregate (decode 23.656%, work/frame 32.208% on hardware) and labels that, in its own log, as an asset-level mean rather than a worst-frame row. The per-track census the bake already prints is the app's contribution, handed across with the UI-cost addend. |
| Q17 | Does `assets/sound/test.wav` still earn its 17 MB? | **Answered 2026-11-15: kept, and renamed to what it actually is — `assets/sound/bioluminescent-32768hz-8bit-stereo.wav`.** Measured before naming: `pcm_u8`, **32,768 Hz**, **2 channels**, **8 bits/sample**, block align 2, 16,948,526 B, 258.61 s, 524 kbit/s, encoder `Lavf63.1.101` — exactly the form `scripts/convert_stereo.sh` writes (`-ac 2 -ar 32768 -c:a pcm_u8`), of the same performance as the master (correlation **+1.000** at t = 10/60/200 s). So it is not a second recording and not a stray: it is the 32,768 Hz-era render at the 8-bit played depth, regenerable in-tree from a tracked input, which puts it squarely in §2.3's instrument class. What it was *not* earning was its name — `test.wav` said nothing about rate, depth or channels, and the script's default output path pointed back at it, so every fresh conversion re-created the same ambiguity. The rename is the fix; the script now names its output after what it contains. |
| Q18 | What should a listener hear when they press resume on a ring that has underrun? | **Recorded 2026-11-18 (PR-3b-ii review); the ruling below is the caller's, 2026-11-19.** **What this head does today:** a resume press whose ring has nothing to give ends the album. `Transport::toggle` takes `Paused → Resuming`, starts the ramp *immediately* and asks for a prime (`src/transport.rs:538-553` — `self.gain.fade_in()` at 551, `Step::Prime`); the frame loop then calls `prime_hand_off()`, whose `take_into_slot` hits `Chunk::Starved` and returns false (`src/album_play_main.rs:1765-1786`, the `return false` at 1777), so the log prints `FAIL the resume had nothing to prime — the ring was empty` and the run halts with reason `resume` (`:3871,3873`); the pause record's resume half is never written (`pauses.resumed` is reached only on the success path, `:3876`). **That is not a gate-only arm** — nothing in that block is feature-gated, so the `make album-player-rom` build a person actually plays does the same — and because a halt is not a verdict term, the screen carries a bare red `FAILED` with the reason only in the log (`src/album_play_main.rs:91,101`; `refuse()` at `:3499-3510` is the boot-path shape of the same gap). So today's answer is: **the album stops and the listener is not told why.** **When it can happen:** `Step::Freeze` stops the chain (`:3897-3901`), so a pause does not drain the ring — the empty ring has to predate the press. The sequence is a pause taken while the album is already underflowing and a resume before the producer refills. That is reachable rather than hypothetical, **though the reason changed on 2026-11-19 and this row is corrected for it: U4 is met with 13.73 points of margin at the worst frame (§2), so the lossy producer is no longer over budget** — this row originally cited "105% worst / 89% mean". A producer only has to fall behind once for a ring to drain, and the ways it can are measured, not assumed: a cart's ROM fetches cost cycles the emulator does not model, which is R13's own lesson, and the lossy track's own player on hardware reached the DMA's grace on 104 of 16,553 periods from nothing but a 1 Hz text redraw ([`ULI2.md`](ULI2.md) → U5) — which is what a UI does to a margin that size. A skip that left a 4-chunk ring ahead of the pause makes it likelier. The 2026-11-19 session that closed R13 (AP-7a (4)) worked the pause/resume pair hard and did not meet it, which bounds the frequency and not the defect. **Ruled, not open:** a product must not stop — **a listener should expect audio to resume.** The open part is narrowed to *how much cover, and how the wait is shown*. **Why the existing mechanism is not already the answer:** `State::Repriming` is entered only by `Transport::begin_reprime`, which returns false unless the state is already `Skipping` (`src/transport.rs:587-591`), and that is deliberate — PR-3b-iii's subject is the skip contract. Its ramp is *scheduled*: `HandOffGain::fade_in_after(chunks)` starts the fade on the chunk after the last cover one (`src/transport.rs:339-343`) and `State::feeds()` (`:433`) keeps the producer refilling behind the cover. There is no path from `Paused` into it, and this PR changes no behaviour, so the resume leg is untouched. **Corrected 2026-11-19 — this row's first version (34c6ee8) cited the pre-redesign tree.** `stats.starved_at_resume` and a `Starved at resume` row, `Stats::served_silent`, `HandOff::handoff_silent()`, `resume_at_mark`, `src/ring.rs`, and `transport.rs:292-299,332` are PR-3b-ii-B2b's shapes and none of them is in this head: the feed and `Chunk` now come from `crates/gba-playback/src/playback.rs`, the silence witnesses are the skip row's own fields (`:2450`: `underflow_delta=`, `head_silence=`, `silence=`) and `all_silent` (`:1607`), and `Transport::served_silent_tail()` (`src/transport.rs:530`) exists with no caller in the app. **The practical consequence: a resume that found nothing to prime has no counter at all** — the only witness is the FAIL line — which is R13's pattern again: a condition with no number attached is a condition that will be argued about instead of measured. **Scoped as PR-3c — §11 carries it as this phase's next work item (2026-11-19).** What that PR owes: (1) a **leg in §4.4 D2** for resume → first audible sample, bounded by this run's measured pass and its own producer rate rather than a constant (today's pause legs are press→freeze and the prime, and `pause_to_freeze_bound` bounds the *out* half only); (2) a **maximum on the wait** — with the chain un-armed nothing is consumed, so the ring's own capacity (**36 chunks = 1,230 ms**) ceilings any "don't arm until there is audio" design, which is exactly what makes a *counted cover* the shape for longer waits; the skip row's `dropped=`/`silence=` treatment is the model, since a cover chunk is pushed and consumed but is not track content (`:466`), and the shape was proved at `examples/album_probe/src/probe_hw.rs:859` (`truncate_stale`) and shipped for skips in `transition_hand_off` (`:1950-1975`), which pushes the cover's first whole chunk in the same critical section as the truncate; (3) an **A11 affordance** so the wait is visible to the human and not only to the audit, plus the **counter** the removal of the refusal makes mandatory — R13's lesson turned to face a halt: a red that stops a correct cartridge forever is a worse witness than a cover that is counted, printed and bounded. **Not the Q4/Q13b family**, which is about track boundaries: here the ring head must not move, and the pause's own claim is that the resume starts from the sample the freeze stopped on (`:2906` records it, `:2992` refuses if it moved). So "catch up from a later position" is **ruled out rather than deferred**, and "just stay paused" is not free either: the transport contract has no cancel, because reversing a half-served fade is a click of its own (§4.4). **Landed 2026-11-19** as the merge `2a946f2` (PR #170; the branch it was developed on, `feat/album-player-pr3c-resume-refill`, is deleted): §11's PR-3c record carries what was built (a ring-routed resume, `State::Buffering`, a frozen-chain refill to the prefill's own target, and `resume_to_audio_bound`), and the two knobs this row left open were resolved by borrowing that geometry rather than by choosing a number. **Ruled 2026-11-19, so this row holds nothing open as a question:** the visible indicator is deferred to the shell's PR — and its name here, "the A11 affordance", is corrected at §5.3, A11 being the track-list screen — the end-to-end `resume→audio` row leg is left with a named home in the `latency:`/`reaction` composition, and the hardware witness for the branch is asked of AP-7, where its numbers exist and nowhere else. What this row still owes is measurement, not decision. |
| Q19 | **Which face draws which element?** (raised 2026-11-20 by the band re-rule; §5.5) | **Open, and deliberately not closed by the PR that re-ruled the layout.** What *is* ruled: the face is a **declared per-element field** of the variant, and the shipped 8 px subset (`assets/fonts/fusion-pixel-8px-proportional-latin-ascii.ttf`, 96 glyphs, U+0020–U+007E — `assets/fonts/README.md`) is an **instrument and verdict face, not a product face decided by default**. **Stated as an expectation now rather than a hint: that cut was baked for development and is expected to be dropped once an authored cut exists, so its 96-glyph count is not a budget anywhere in this plan** — the glyph-cache figure every variant budgets against is the declared face's, measured at bake time (§5.1's BG2 row, R14), and what survives the face being replaced is the cost ordering below, which is about cadence rather than about typefaces. This file has always made that distinction about the verdict screen (AP-7a's legible `PASSED` is witnessed in it); the band model is what made it apply to the title band too. **What the cost arithmetic says, and it is why the answer is probably mixed:** the **clock** is the element whose redraw habit has already gone wrong on hardware (R1b: a 1 Hz text-**row** redraw, 104 late re-arms of 16,553 periods), and §5.2's/§5.5's cadence rule is what shrinks it — **not the face**: a changed digit at the product pitch is 1–2 characters × the tile rows the line box spans ≈ **2–4 tiles ≈ 1.5–3 k c disarmed / 2.5–5 k c live** (§10.2's clock row, priced at the 16 px pitch), and the redraw is one cell rather than a row; the earlier wording there ("the face stays small", 4–7 k c for 6–10 tiles) is **superseded twice over** — §5.5 forbids a small face on a product variant, and it was the changed-cells rule doing the work, not the typeface. The **title line** redraws on a track change, which is rare and off the 1 Hz path, and can afford the full declared face: **28 characters × 2 tile rows ≈ 56 tiles ≈ 41 k c disarmed / 69 k c live**, on an event that already skips and re-primes (§10.2's title row). The **title-card scene** is paid **once per run**, which is where the largest declared face is cheapest per occurrence and most likely to be worth having. **What the option costs, priced rather than waved at:** a second face is a tracked, *subsetted* font asset with its license rows (the `pyftsubset` pattern `assets/fonts/README.md` already documents), a second glyph cache competing with the region canvas for slots in agb's 512-slot regular tile pool — with a region-canvas image already taking 480 of them there is no room for a second face at all (§5.1, R14), and a second set of real metrics every fit verdict must use (§5.5) — plus the fact that a face change restyles a hardware-witnessed screen for no measurement of its own. **So the split is: the *mechanism* is 2a** (`measure`/`fit` take the face from the variant; snapshots cover every declared face, and `album-layout-test` is green with either), **and the *choice* is made on a 1× composite with a real title in it (§5.6), not here.** **Recommended default for 2a, re-ruled 2026-11-20 at the second review (the previous default in this cell — "the 8 px face everywhere it already draws" — is withdrawn):** the 8 px cut stays exactly where it is (tracked, used by the verdict/dev instrument, declared only by a `product = false` variant, and never linked into a ROM whose variant does not declare it — §5.5), and **the first product-eligible face is an ASCII subset cut of the `ark-pixel-10px` OFL face this repo already tracks**: it rasterises exactly at its design size, its line box is the 16 px pitch floor §5.0's rule makes minimum, its measured ink is 9.0 px against the instrument's 8.0 px, and the only work it needs is the `pyftsubset` cut `assets/fonts/README.md` already documents plus licence rows. **Two things this row keeps open on purpose: the final size (a 16 px line height buys eight list rows, a 20 px one buys six and most of the pool — §5.1's interaction table is the arithmetic, `album-preview`'s 1× composite is the decision), and the possibility of a better asset** (a face designed at ~12 px with a line box under 16 px would buy the rows back; it would be a new tracked subset, which is cheap in this plan's terms). What is not open: declaring a raster that is not a design size or a whole multiple of one (§5.5, `pitch-not-tileable`), and quoting a glyph count anywhere (§5.1's BG2 row). |
| Q20 | In 2b, which comes first — the title card, the track list, or the art? (raised 2026-11-20) | **Open, and it is a taste question now that it is allowed to be one.** 2a's contract makes the answer reversible — a scene is one rectangle and one function, and no scene may touch a chrome cell (§5.2) — so sequencing 2b stops being an architecture decision. **Three things now belong to this row, and §5.3 pointed at the wrong one until this review (that row cited "new **Q21**", which is the full-bleed art question): (i)** the order 2b's scenes are *built* in; **(ii)** the **runtime scene-cycle order** `Select` walks (§5.3) — a declared field of the variant's registry, which makes it bake-time checkable and refus-able like any other declared geometry rather than a runtime choice; and **(iii)** what a **`title_card` position means mid-album**, since a card whose only instruction is "PRESS START" is a boot affordance — either the card variant reachable from `Select` drops the prompt and shows album metadata, or the card is not in the runtime cycle at all. None of the three is settled here; all three are answerable by editing the spec, which is the point. **The pull, for the record:** the **title card first** (A12), because it is nearly free (reuses the chrome, needs no `ART1` path) and buys §5.2's boot flow plus a place for the album title `ALBM1` already carries; the **list next** (A11), because it is the scene that changes how an album-length cart *feels* to use and needs no art tooling; the **art last** (A8), because it is the only one of the three needing `album_art.py`, and the only one whose cost line has to be sliced across periods (§10.2) rather than paid at a boundary. That ordering is a recommendation, not a ruling: **if a real cover exists for the fixture album, the arithmetic flips and whoever has it should say so in the review.** **Answered for the build order, 2026-11-25:** the cover exists (R15's note), so the pull flips as this row said it would — 2b's ladder (§11's note) builds the schema and the probe first, then the title card, then **art before list**, then presentation, then the list, then productionization. That is (i); (ii) and (iii) land in the ladder's PR-(3), where the `Select` order and the mid-album card are decided, and a press-before-audio boot flow is still a separate PR that restates AP-1. **Answered 2026-11-25 (2b PR-(3)): (ii)** the cycle is the variant's declared registry order (`layout::SCENES`), filtered to the implemented scenes (`blank` ⇄ `title_card` today), with a host leg that keeps the filter's relative order; **(iii)** the card is in the cycle and **drops the prompt** — audio auto-starts under option (c), so a metadata card is the only honest mid-album reading, and a boot-flow flip stays a separate PR. What stays open here is a **product** question rather than a sequencing one: whether a card is in the runtime cycle at all, and whether boot ever waits for a press |
| Q21 | Is full-bleed 240×160 art a product feature or an experiment? (raised 2026-11-20; §7) | **Legal, not default — priced rather than assumed.** Already ruled as part of the band model: region-fit is the default, `bleed = true` is a declared variant field, and its four costs are named in §7 — legibility **free** (per-tile palette selection composes bands over art with no blending, §5.4, and that *is* §16's "palette trick"); ROM **~0.40 s** of lossless audio or **~1.5 s** of lossy; VRAM **600 tiles that agb's regular pool cannot serve at all** — 512 4bpp slots total, so a full-bleed image needs the affine-pool fallback and leaves nothing for the glyph cache and authored chrome to sit beside; and running out is a **panic on hardware**, not a bake refusal, unless §5.1's `tile_budget` catches it first (§5.1, R14); and **~0.75–0.9 M cycles**, affordable only sliced across ~26 periods — **of the lossy arm's audio time: the FLAC arm's slice budget and its lead are different numbers again (§7)**. **What stays open is exactly what no document can close:** whether agb's allocator places the second block without disturbing the text pool (AP-3/AP-4 record it), and whether a real cover composed for full-bleed survives four banks and a dither **at 1× on hardware** (§5.6). One thing must be decided *before* 2b rather than during it: the **safe-area rule** — a picture drawn under the bands needs a declared minimum pad, and retrofitting a pad after the tool has rendered a hundred covers means re-rendering a hundred covers. |

**Q15 in detail — measured 2026-11-15, on the master itself.**

- **One master is ~35% of a cart, so the repeats alias.** Mid-album material packs at
  1,364,938 B per 30 s = **364.0 kbit/s** (the head 9.9 s gives 340.8 kbit/s), so the whole
  258.61 s is **≈11.8 MB** against a 33,554,432 B cart; the cart would hold ~12.3 min of this
  material and the master is 4.3 min. That is fine precisely because `ALBM1`'s per-track
  `(offset, len)` is a *map* (§4.3): two tracks may address the same bytes, so a long list
  costs one encode — measured at **1.9 s of packing per 30 s of audio**, `flac -d` differential
  and self-audit included.
- **Aliasing serves the flow tests, not the size tests.** AP-1/AP-2/AP-3 (boundaries,
  position, bar, a 32-row list, skip, pause) are fully exercised by repeats. AP-5's
  budget refusals and R3's bus-contention hardware runs need **distinct bytes**, so those arms
  encode the whole master and split it into sequential tracks — and the bake report should say
  "11.8 MB of a 32 MiB cart" rather than implying a full cart was tested.
- **Identical audio makes a content-substitution bug inaudible** — playing track 1's bytes when
  track 2 was asked sounds exactly right. So the witness for this fixture is *track index →
  blob sha256*, asserted from the generated pin module and §4.3's manifest cross-check, not
  "audio came out". The pins §2.3 inherits are load-bearing here in a way they would not be
  for a real album.
- **The clip's endpoints should be chosen, not rounded.** `silencedetect` at −42 dB / 0.12 s
  finds **no interior silence** in this recording (only 0.000–0.341 s and 255.97–258.61 s), and
  an arbitrary cut begins at 9,205–12,761 of 32,767 — 28–39% full scale. The quietest 50 ms
  windows are **t ≈ 256.0 s** (peak 215), **t ≈ 1.0 s** (4,093) and **t ≈ 57.0 s** (4,297). A
  clip that starts and ends loud is still the better *transient* test for the fades; the point
  is that AP-1's no-click claim should be attributable to the fade code, not to wherever a
  round number happened to land.
- **Free extra, and worth taking:** let one fixture title exceed the fit box (driving §5.5's
  one-line elision — since 2026-11-20 that box is *derived* from the variant's declared span and
  the face's real metrics rather than being 28 characters of prose, §5.5) and one carry non-ASCII
  (driving §5.5's fold and its offender report, which is also Q9's evidence). Titles are config,
  so this costs no bytes and no encode.
  *(revised 2026-11-15, PR-2 **D5**: taken, but not in the fixture — the tracked `album.toml`
  carries Q15's plain names and these cases run as synthetic inputs in `scripts/album_prep.py
  --self-test`; inventing a recording to exercise a rule puts fake metadata in the one artifact
  a reviewer reads. §5.2's overflow needs a layout fixture at AP-3, not an album title. **(Re-ruled
  2026-11-20 and now actually schedulable: that fixture is `make album-layout-test`, which snapshots
  every product-eligible declared variant and asserts fit against the derived box — including a title
  long enough to fall outside that box, which the suite measures (the elision that would shorten it is
  PR-(2)'s, so what this proves today is the measurement and not the elide); it is a synthetic input in
  that suite and stays out of the tracked album, for exactly the
  reason D5 gave.))** *(updated 2026-11-22, 2a PR-(2): the elision exists now, on both sides of the
  boundary — `FaceMetrics.elide`/`title_render` in the bake, `elide`/`fit` in the generated atlas
  module — and the two copies are compared against each other through the case table the atlas
  generator pins, so these legs prove the elide as well as the measurement. The leg that makes "and
  it fits" mean anything is the maximality one: the next glyph in the input would **not** have
  fitted. A suite that only checked that the cut fits would pass a tool that cuts early.)*

---

## 16. Non-goals

No on-device file browsing or multi-album carts (Q11). **No album audio and no per-ROM
assets committed** — they belong to whichever album was built, so the build generates them
and git ignores them (§2.3); the repository keeps the instruments and the reference master
under `assets/sound/`, which are a different class of thing. No streaming, link-cable
or MIDI input here (goal 3 is its own ROM). No new codec work, no format
revisions, no encoder rewrites inside an app PR. No runtime FFT. No video, no
per-frame art streams, no alpha blending beyond palette tricks (and §5.4/§7's bands-over-art
composition *is* such a trick: per-tile palette selection, not blending). **No general GUI
toolkit: `layout.rs` + one surface + a `Scene` slot is the whole abstraction** — a scene is one
rectangle and one function, and there is no widget tree, no runtime layout engine, no focus
model, no stacking order anyone can change, and nothing a scene may do to a chrome cell (§5.1,
§5.2). **Re-ruled 2026-11-20: the band model is exactly the shape that invites someone to ask
for a GUI toolkit, so this line is written to be read, not implied — the abstraction is one
struct of compile-time constants and one trait-shaped seam, and it earns
extraction into a shared crate only when a **second** application needs it (which
is the plan for the toy-game ROM, if it survives).** **No part of this application
depends on anything outside this repository** — no external code, assets, byte
layouts or tools are cited as authority or linked in (§2.1). No claim of real-time,
hardware playback, battery life or "works on my cart" that has not gone through a
gate in §13.

---

## 17. Definition of done, v1

A ROM that a person can build from a folder of WAVs and a cover image with one
command, that boots through **one surface** to an album title-card **scene** (§5.2), plays the
whole record at the
codec's accepted fidelity with 0 underruns **on both GBA units**, shows a
dithered cover, a track title, elapsed and **total** time (the pair re-ruled 2026-11-20, §6) and
a progress bar that
agree with each other, **on a chrome that is on screen the whole time and never moves when a
scene changes (§5.1)** — with **the layout it is drawing printed on its own boot line
(`layout=<id>`, §13 AP-3)** and every product-eligible declared variant's host snapshots and fit
verdicts green (the rest named and skipped, §13 AP-3)
before any of it reaches hardware, lets Start pause and Resume, L/Left and R/Right skip with
no click and correct on-screen position, lists all tracks and plays any of them,
animates something in time with the music, and whose whole application frame
still closes in a ledger where the residual is named — on the lossless backend,
and behind the same surface, the same chrome bands and the same manifest on the lossy one — which is no longer gated on
that backend being ready (U4, U5 met, 2026-11-19) but on this app writing the adapter and the
asset path for it (Phase 5, R7) — with every target number in §10 replaced by a measurement.

---

## 18. Companion files

- [`FLAC.md`](FLAC.md), [`flac-docs/`](flac-docs/) — the lossless source this app plays first.
- [`ULI2.md`](ULI2.md), [`crates/uli-lite/PLAN-OF-RECORD.md`](crates/uli-lite/PLAN-OF-RECORD.md) —
  the album-length codec and its container. **The gates this file spent two years saying this app
  waits on are met: U4 (real-time decode) at gate total 86.27% worst / 76.53% mean with 13.73
  points of margin and zero starved periods, U5 (playback) on a GBA SP with 0 underruns across
  three player ROMs, PR D (output stage) shipped as `crates/uli-playback`, and U6 (memory) at
  26,496 B of 32,768 `.iwram`** — [`ULI2.md`](ULI2.md) §Gates is the authority and
  [`PLAN-OF-RECORD.md`](crates/uli-lite/PLAN-OF-RECORD.md) → "Current state" is the live plan;
  what is still open there is the codec's non-decode half of its own gate, the dense-frame entropy
  tail, **the app's asset path onto the in-repo encoder (R7, Q3)** — the encoder itself has
  landed through S8 and is byte-exact — and a
  licensing/publish decision. Read numbers there, not here: this file has carried a superseded
  figure forward on this point in **both** directions — first "U4 closed", then "U4 is not met —
  105% worst / 89% mean" — and every one of those corrections is marked inline (§2, §10.2, §10.3,
  §10.4, §10.5, §11, R1, R1b, R6, R7, Q1, Q8, Q18, AP-9).
- [`README.md`](README.md) → "Build Process" — the `master-pack` / `master-play-rom` flow this
  plan generalises to N tracks (§2.3), and the place the new `album-*` targets have to be
  documented when they land (Phase 1 owes that edit, along with the `gba-build-and-test` skill).
- [`.gitignore`](.gitignore) — where the split is actually enforced: ROMs, the generated
  master asset + pin and the rate clips are ignored as build output, while the instrument
  blobs (spike arms, corpora, fixtures, the probe's `ART1`) are tracked with their reasons in
  the comments. Phase 1 adds the `album_player` generated paths here, in the same idiom: the
  rule and its reason, not a bare filename.
- [`assets/sound/bioluminescent.wav`](assets/sound/) — the tracked reference master, and
  Phase 1's default album material (Q15). Beside it sits
  `bioluminescent-32768hz-8bit-stereo.wav`, the same performance rendered at `pcm_u8` /
  32,768 Hz / stereo, kept as an instrument and named for its measured format (Q17).
- [`README.md`](README.md) — "Project ideas (top-level ROMs)" carries this as a one-bullet
  summary with a pointer here, added 2026-11-15 (`6cd09f0`); the line below this bullet once said
  the pointer "should gain" itself when Phase 0 landed, and it had already. *(corrected 2026-11-19)*
  This file stays the only place the plan, the phases and the measured numbers live; the README
  bullet is a signpost and nothing more.
- [`examples/album_probe/README.md`](examples/album_probe/README.md) — **AP-0, the
  Phase 0 measurement record** (landed 2026-11-14): the probe ROMs' tables, §10's
  target→measured annotations, the deep-queue drain/drop sweep behind §4.4's ruling
  and Q13b, and the NOT VERIFIED list of everything an emulator run cannot claim.
