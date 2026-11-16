# ALBUM-PLAYER.md — proposal: the Album Player application

Status: **proposal, opened 2026-11-14; Phase 0 (AP-0) landed the same day, and the plan has been
reviewed and re-ruled twice since** — the
three probe ROMs are built, run and measured, and their numbers live in
[`examples/album_probe/README.md`](examples/album_probe/README.md). Everything from
Phase 1 on is still unbuilt and unclaimed. It is a plan to be argued with, a scope
statement, and a budget the first probe ROM has to contradict usefully — and it did:
AP-0 refuted one row of §4.4's observable table, and the fix is recorded there rather
than quietly applied. Six things have already been settled
in review and are recorded inline rather than left open: **a skip is allowed a
short, bounded delay (§4.4)**; **a skip drops the queue by *truncating* the ring, and
the application grows that method on its own feeder once §2.2's lift lands (§4.4,
Q13b — ruled 2026-11-14 on AP-0's measurement)**; **the session clock is one `u32` in `.iwram`
(§10.4)**; **what this application waits on is still the lossy backend's real-time decode
(U4), and only then its playback (U5) and output stage (PR D) — §2, §10.2, §11, corrected
2026-11-15 against the plan of record, which never recorded U4 as met**; **a generated ROM's
assets are build output, never inventory — a tracked track list plus the already shipped
`master-pack` flow generate the blobs, the manifest and the pin module at build time, while the
development instruments and the reference master stay tracked (§2.3, ruled 2026-11-15)**; and
therefore **the bake tool is split across two phases:
packing a declared track list is Phase 1 work, its verdict set stays in Phase 3 (§3 A3a/A3b,
§9.0, §11)**. Every number below is either
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
> seam, not a rewrite.

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
| A second decoder in the tree | `crates/uli-lite`, `crates/uli-lite-gba` | **U4 (real-time decode) is NOT met.** This checkout does carry the ledger, and it says so: [`PLAN-OF-RECORD.md`](crates/uli-lite/PLAN-OF-RECORD.md) → "Critical path to real time" (2026-11-15) records the production frame at **274,198 worst / 232,090 mean = 105% / 89%** of the 262,144-cycle period after C-3's HotState-to-IWRAM — the mean fits, the worst frame is **12,054 cycles (4.6%) over**, and the codec's own working target is **≤ ~85%** so that the gate total (convert + IRQ + DMA) and this tree's UI fit beside it. Still **decode-only** even at parity: **U5** (playback — zero underruns, clean period audit, mGBA then hardware) and **PR D** (the output stage) are open behind it. Ladder for context: PR C 200% / 174% → C-3a/c 186% → C-3b 175% → C-1 164% → v4 162% → `iwram-tw` 157% → fused OLA 148/138% → **C-3 HotState 105%** *(corrected 2026-11-15; an earlier revision of this row read "U4 met", which no codec document says)* | the lossy backend is close, and is a candidate for the album-length build **once U4 lands with margin**. What this app waits on is **real-time decode, then playback and the output stage** — and everything it needs arrives **through the seam**, so the ordering is invisible above it |
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
| `crates/uli-lite/tests/fixtures/uli2_tiny.uli`, and its byte-identical twin in `examples/uli_spike/assets/` (sha `68dca43a…`, 39,308 B) | the 129-frame iteration corpus every ULI2 arm is measured against — 2 s of deliberately varied synthetic audio, encoded by the **off-tree** C reference, so untracking it would leave the corpus unregenerable from this checkout (§2.1) |
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
quietly drops it. As of 2026-11-15 that precondition is still **U4 (real-time decode —
105% worst / 89% mean today, `PLAN-OF-RECORD.md`)**, and behind it **U5 (playback) and PR D
(the output stage)**.

| # | Requirement | Phase | Gate | Notes / dependency |
|---|---|---|---|---|
| A1 | Multiple tracks on one cartridge | 1 | AP-1 | An application manifest indexes N tracks over whichever backend is linked (§4.3) — the same screen, the same code, both codecs. The index is `ALBM1`'s **internal per-track map of ROM locations and sample boundaries**, generated at build time (§2.3, §4.3), and it is not the A11 screen. `make album-rom BACKEND=flac\|uli2` (§4.2b). |
| A2 | Skip next / previous **on the cartridge itself** | 1 | AP-1 | `PcmSource::start_track` + a fade + a ring re-prime; a short, bounded delay is accepted product behaviour, not a defect (§4.4). Buttons, not the emulator. |
| A3a | Tool: a declared track list → a packed ROM (**the blobs are generated, never committed**) | **1** | AP-1 | `scripts/album_bake.py` at its Phase 1 scope: `album.toml`'s track list → one `GAFP` per track via `gafp_pack.py --master --emit-pin` → `ALBM1` + generated `src/assets.rs` → ROM (§2.3, §9.0). Refuses the two things Phase 1 can check: rate identity, and manifest-vs-container disagreement. |
| A4 | Play / pause | 1 | AP-1 | fade-in/fade-out over the DMA boundary, silent-ring state, no click (§4.4). |
| A5 | General UI layout support | 2 | AP-3 | screens, a layout table in one shared module, a host snapshot twin (§5). |
| A6 | Elapsed time / time left | 1 | AP-2 | shifts, not divides, because both clocks are powers of two (§6). |
| A7 | Progress bar | 2 | AP-2 | bake-time reciprocal per track; monotonic; bar ↔ time agreement (§6). |
| A8 | Album art, even low-res | 2 | AP-4 | 4bpp palettised tiles + ordered dither, generated by our own host tool (§7). |
| A9 | Sound-reactive visualisation (WMP-style) | 4 | AP-6 | precomputed analysis sidecar on the album timeline, not on-target FFT (§8). |
| A10 | Track titles | 1 | AP-1 | From the application manifest's string pool, never the codec container; ASCII-folded at bake time (§5.5). |
| A11 | Track list screen | 2 | AP-3 | ≤ 32 rows, scroll + cursor + now-playing marker. |
| A12 | Opening menu / title card with the album title | 2 | AP-3 | boot → title card → "press Start"; settings page inside it (§5.2). |
| A13 | Tool that *configures* the album and bakes the ROM | 3 (schema: **1**) | AP-5 | `album.toml` is the single input; the ROM is a function of it (§9). The file's `[album]`/`[[track]]`/`[out]` core is **Phase 1's**, because A3a needs it; `[art]`/`[viz]`/`[game]` and the budget verdict arrive in Phase 3. |
| A14 | **[reach]** Web UI for the configuration | 6 | AP-8 | a skin over the bake entry point, never a second implementation (§12.1). |
| A15 | **[reach]** Background toy game (T-Rex / pong / breakout / snake) | 6 | AP-8 | admitted by a ring-watermark rule under a hard cycle cap (§12.2). |

**Three things in this plan share the name "track list", and two of them are not a
screen.** *Track list* (A3a, §9) is the tracked config input — `[[track]]` entries naming
files, order, titles, artists. `ALBM1`'s per-track table (§4.3) is the **generated internal
map**: for each track, its `(offset, len)` of asset bytes in the ROM, its `start_sample` and
`n_samples` on the album timeline, its string-pool offsets and its §6 reciprocal. The *track
list screen* (A11, §5.2) is the user-visible browser, and it is Phase 2. When this file says
"the track list indexes the album", it means the second thing, which the first thing
generates.

Deferred-but-named, so they are not forgotten: seek-to-position within a track
(A2's natural next step, and the first feature where the two backends visibly
differ in granularity — the app's answer is a `PcmSource` capability query, §4.2,
not an `if`), resume-from-last-track via SRAM (agb ships
`sram`/`flash`/`eeprom`), speaker output profile, **[waiting]** album-length
builds on the lossy backend's **U4 (real-time decode — not met, §2)**, then U5 (playback)
and PR D (output stage).

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
  src/main.rs            boot, screen stack, the frame scheduler (§4.5)
  src/pcm.rs             PcmSource — the codec seam (§4.2)
  src/geometry.rs        period/ring/prefill arithmetic DERIVED per backend (§4.2b) ← host-twin'd
  src/backend/{mod,flac,uli2,stub}.rs   the only place a codec crate is named (§4.2b)
  src/playback.rs        ring states: play / fade-out / silence / re-prime / fade-in / end
  src/position.rs        the 32-bit session clock the UI reads (§4.4)   ← host-twin'd
  src/keys.rs            edge detect, repeat, chord, debounce    ← host-twin'd
  src/layout.rs          every geometry constant in the tree     ← host-twin'd
  src/text.rs            the shipped renderer, generalised from examples/flac_integration/src/text.rs
  src/art.rs             tile/map/palette upload, palette banks, fade
  src/screens/{title,menu,nowplaying,tracklist,pause,error}.rs
  src/viz.rs             ALVZ1 reader (§8)
  src/game/{core,pong,snake}.rs                                   (§12.2, phase 6)
  src/meta.rs            ALBM1 album-metadata reader (§4.3)
  album.toml             TRACKED — the track list: which files, in what order, what titles/artists (§9, §2.3)
  assets/                GENERATED, gitignored — per-track GAFP + ALBM1 + ART1 + ALVZ1 (§2.3)
  src/assets.rs          GENERATED, gitignored — include_bytes! + sha256 pins over the above (master_pin.rs's shape, N wide)
  tests/                 host twins + snapshot checks
```

### 4.2 The seam is `PcmSource`, and it was already specified

[ULI2.md](ULI2.md) → "Phase 2" item 4 asks for exactly this: *"Make the playback
layer codec-agnostic with a small `PcmSource` trait, so FLAC remains a
compile-time alternative and an A/B control rather than dead code."* The app is
where that gets built. The trait is deliberately narrow — 9 methods, no lifetime
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
| Screen changes to the new track (title, index, bar reset) | within one video frame of the button press |
| Audio of the new track begins | ≤ ~0.6 s, and that number is `SKIP_PRIME_CHUNKS`, a policy knob (§4.4 below). **The precondition is met: step 2 truncates the ring (Q13b, landed 2026-11-15). Measured at the ring's ceiling depth, the truncate reads `SKIP_PRIME_CHUNKS + 1` periods — 3/5/9/17 periods = 102.5/170.9/307.6/581.1 ms for prime 2/4/8/16 — inside this row at every setting; drain and consume-and-discard still read 39–53 periods = 1.33–1.81 s.** |
| Clicks / pops at the transition | none: fades, never a mid-waveform splice |
| Underruns attributed to a skip | 0, including the silence fed during the re-prime |

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
  4. feed *silence* chunks so the DMA keeps getting a whole chunk every period
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

One shipped gotcha to carry into the UI design, because it bit 6a-7b's first
hardware run: agb's `TiledGraphicsFrame::commit` derives DISPCNT's
background-enable mask from the backgrounds *shown into that frame* — so a commit
from a fresh frame with nothing shown writes `enabled_backgrounds = 0` and repaints
the backdrop alone (`text.rs`'s "One-shot present" note). The app is by
construction *not* one-shot: it owns its backgrounds for the ROM's lifetime and
shows every live one every frame, and AP-0 exists partly to prove that on screen.

---

## 5. Screens and layout (A5, A11, A12; the frame A8/A9 sit in)

240×160 px, mode 0 (tiled, 4 BGs). Everything is 8×8 tiles: 30 columns × 20 rows.
The 8 px font (`assets/fonts/fusion-pixel-8px-proportional-latin-ascii.ttf`, a
96-glyph U+0020–U+007E subset, OFL) is proportional, so text is laid out by the
shipped renderer and *measured*, never assumed to fit.

### 5.1 Layer budget

| layer | contents | VRAM cost (4bpp, 32 B/tile) | notes |
|---|---|---|---|
| BG0 | flat backdrop + progress-bar track | ~4 tiles | the verdict-colour trick already in `text.rs` |
| BG1 | album art (§7) | 120×120 px → 15×15 = 225 tiles ≈ 7.2 KB; full-screen 600 tiles ≈ 19.2 KB | one of the art modes, picked per album at bake time |
| BG2 | text + progress bar fill + list rows | ~64 tiles shared/glyph-ish | agb's text renderer + dynamic tiles |
| BG3 | reserved (viz bars / menu chrome) | ≤ 256 tiles | |
| OBJ | cursor, sprite animation frames, "PAUSED" badge | second VRAM half | **new for this tree**: nothing shipped has drawn a sprite. AP-0 proves it. |
| palettes | 16 BG palettes × 15 usable colours, 16 OBJ palettes | — | allocation in §5.4 |

The precise VRAM split between BG and OBJ tile pools (mapping mode, character
base blocks, and what agb 0.25's `Graphics` chooses) is **a fact to record in
AP-0's log, not a table to guess at here.**

### 5.2 The screens

```
boot ──► TITLE CARD (A12)                     ► NOW PLAYING (default)
         album art, album title / artist /     art or viz, title, artist,
         year, N tracks · M minutes,           mm:ss elapsed · mm:ss left,
         "PRESS START"                         progress bar, track n/N, state
                                                  │ Select ──► TRACK LIST (A11)
                                                  │            rows: "01 Title … 3:24"
                                                  │            cursor + now-playing mark
                                                  │ Start (hold) ─► PAUSE/MENU
                                                  │                   resume · track list ·
                                                  │                   viz on/off · art mode ·
                                                  │                   game on/off · output profile
                                                  └── any screen ─► ERROR (→ §14 R5): "DATA ERROR",
                                                                    auto-skip to next, stop at end
```

Now-playing layout, 240×160. The split — visual in the top 96 px, text in the
bottom 64 px — is this plan's own choice, made for arithmetic reasons rather than
taste: 96 = 12 tile rows leaves the art boxes (§7) on a tile boundary, and 64 px
= 8 text rows of 8 px is exactly the title/artist/time block below:

```
┌────────────────────────────────────────┐ 0
│                                        │
│        VISUALISATION / ART             │  96 px tall: art when the album ships art,
│            (96 px)                     │  viz strip when --animation, both if the
│                                        │  art is ≤ 96 px tall
├────────────────────────────────────────┤ 96
│ 03 · Shellbound                        │  title, ≤ 28 chars, wrap w/ ellipsis
│ sila                                 ⏵ │  artist + state glyph
│ ──────────────────────────────────────  │
│  1:47 ▓▓▓▓▓▓▓▓▓▓░░░░░░░░░░░░░░ 3:12    │  elapsed · bar · remaining
└────────────────────────────────────────┘ 160
```

`layout.rs` holds every one of these numbers as a `pub const`, and the host twin
re-renders a screen's map from the *same* module, so a layout change that pushes
text off-screen fails a host test rather than turning up as a purple band on
hardware. `text::measure`'s `Fit { lines, right, bottom }` generalises from "the
verdict text fits" to "this screen fits", and the ROM still prints a fit verdict
rather than silently clipping.

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
| **Select** | cycle the picture: art → viz → nothing | cycle output profile (Q8, later) | one button for "give the cart's CPU back" |
| **B** | — (reserved: it is the game's action button) | back / cancel | |
| **L + R + Start** | soft reset (the BIOS chord) | — | always available, never application state |
| **hold L at boot** | program `WAITCNT = 0x0000` (4/2, no prefetch) | — | the conservative-cartridge arm this project's records already measure against: a slow reproduction cart gets a documented fallback instead of a mystery |

Deliberately absent: anything requiring two buttons *within a time window*, and
anything that reads differently in an emulator than on hardware.

### 5.4 Palette allocation (the constraint that shapes the art)

| bank | use |
|---|---|
| 0 | text: 0 backdrop, 1 glyph, 2 dim, 3 accent (extends `text.rs`'s shipped entry-0/1 convention) |
| 1–2 | progress bar + chrome (lit/dim/peak) |
| 3–6 | album art, up to 4×15 colours (or 1 palette + a 256-colour mode-2 tile bank if we want 8-bit art — §7) |
| 7–10 | visualisation |
| 11–15 | reserved for the game (§12.2) |

Fade-in/out and the dimming behind a modal are done by rewriting palette entries
(`set_background_palette_colour`), not by touching tiles — cheap, and it gives the
menu transitions a 60 Hz cost of tens of cycles.

### 5.5 Text rules

Titles/artists come from tags and are **ASCII-folded at bake time** (Unicode NFKD,
drop what is not ASCII, collapse whitespace, strip a whole title wrapped in
parentheses), because the shipped font is a 96-glyph subset and `include_font!`
rasterises every glyph in the file (that is
why the subset exists — `assets/fonts/README.md`). Extending the subset is a
cheap, explicit decision (**Q9**), not something a title with an em-dash should
force silently. Long titles wrap or elide by rule; the bake tool prints any title
that will not fit its box, so an album can be *fixed at bake time*.

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
| elapsed / remaining in a track | `pos - start`, `(start + n) - pos` | 2 subs |

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

Either way the bar is a fixed grid of **32 or 64 equal cells** (one 8-px tile
column each at 256 px is over-wide, so 32 cells × 7 px, or a 64-cell bar drawn two
cells per tile), which makes "does the bar's cell match the printed mm:ss" an
arithmetic property a host test can settle. **AP-2 is the gate that says these
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

- **Source → art pipeline (host, Python + PIL, in `scripts/`):** downsample the
  cover to a target box (candidates **120×120** centred, **160×96** for the
  animation layout, or **240×160** full-bleed for a no-viz build), quantise to
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
- **Cost, and who pays it.** 120×120 at 4bpp is 7.2 KB of ROM and 7.2 KB of VRAM;
  full-screen is 19.2 KB each. Art is carved out of the **audio budget**, exactly
  as this plan rules (§10.1): **art and reaction data are carved out of the audio
  budget, never the other way round,** so `album.toml`'s art choice visibly changes
  the kbit/s the encoder targets, and the bake tool reports both.
- **Two art modes to start:** `cover` (still image, one bank) and
  `cover+breathe` (the same tiles with a 1-palette-per-frame hue/brightness cycle,
  ~15 cycles/frame, which reads as a "living" cover for free). Runtime tile
  changes are available (`set_tile_dynamic16`, `replace_tile`) but any per-frame
  tile rewrite is VRAM DMA over the same bus the audio DMA uses, so it is a
  measured decision, not a free flourish.
- **The 256-colour alternative, priced.** Mode 0 can also address **8bpp** tiles,
  which buys one 256-colour BG palette bank at double the bytes per tile (64 B per
  8×8): 120×120 art becomes 14,400 B of ROM *and* 14,400 B of a 16 KiB BG character
  block, crowding out anything else drawing from it — and 8bpp halves the tile
  addressing room for a full-screen image. The better-looking option for a title
  card, at a real VRAM price. **Default is 4bpp across up to 4 banks (§5.4);** the
  8bpp variant is an AP-4 side-by-side judged on a real screen, not on a diagram.
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
album-length path **very little, still**: U4 is **not** met (2026-11-15: 105% worst /
89% mean of the period, [`PLAN-OF-RECORD.md`](crates/uli-lite/PLAN-OF-RECORD.md)), and the
codec has explicitly retired "match C" as its target — the working bar is **≤ ~85% of the
period**, written down precisely so the gate total (convert + IRQ + DMA) and this tree's UI
fit beside the decoder (§10.2). So the CPU that a real-time decoder frees up is not automatically the
visualiser's; it is the gate total's, and §10.2 is where the claim gets written.

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

---

## 9. The bake tools (A3a, A3b, A13)

Two entry points, one code path, and the ROM is a pure function of the config.

```
album.toml (TRACKED) ──► scripts/album_bake.py ──► per-track GAFP via gafp_pack.py --master --emit-pin
                                                  ├──► ALBM1 + ART1 + ALVZ1        (GENERATED, §2.3)
                                                  └──► src/assets.rs (include_bytes! + pins)
                                                  └──► cargo build --release --target thumbv4t-none-eabi
                                                  └──► agb-gbafix  ──► album-player.gba  (never tracked)
```

### 9.0 Two phases, one tool: the packer lands in Phase 1, the verdict set in Phase 3

This is one tool, landed early at its minimum scope, and what moves between the phases is its
**refusal set**, not its existence: packing a declared track list is Phase 1's because nothing
downstream can decode N tracks without the `ALBM1` only a packer writes (§12.1's
one-implementation rule), while the size-and-art verdicts need the `[art]`/`[viz]` config
sections that Phase 3 introduces.

| piece | **Phase 1** (AP-1) | **Phase 3** (AP-5) |
|---|---|---|
| `scripts/album_bake.py` | exists, and carries the whole audio path: read the config's `[album]`/`[[track]]`/`[out]` subset → per track, `gafp_pack.py --master --emit-pin` → write `ALBM1` (the §4.3 internal map: `(offset, len)`, `start_sample`, `n_samples`, string offsets, the §6 reciprocal) → write `src/assets.rs` → link | *the same file*, grown: budget arithmetic, minutes-per-cart, `[art]`/`[viz]`/`[game]`, every remaining §9 refusal |
| `scripts/album_prep.py` | its rate half — 16,384 Hz stereo (much of which `gafp_pack.py` already does from the WAV header) and the tag read + ASCII fold (§5.5) | its album half — join the tracks into one continuous timeline, so `start_sample` is an album position rather than a file offset |
| `scripts/album_art.py`, `scripts/album_viz.py` | not built | built (§7, §8) |
| determinism | **pack twice, compare bytes** — nearly free once the packer exists, and it is what makes "generated" mean "regenerable" rather than "whatever is lying around" (§2.3) | two full `make album-rom` runs → byte-identical `.gba` |
| refusals that fire | declared rate ≠ product clock (Q14); frames outside the encode profile (inherited from `gafp_pack.py`); **`ALBM1`'s `n_samples` disagreeing with the packed container's `total_samples`** (§4.3 rule 1); the writer's own output re-read through the shipping `meta.rs` parser (§4.3's `audit_blob` rule) | plus over budget, >32 tracks, palette count, title fit, unused viz streams, the §10.4 audit ceiling |

Two duties the split makes explicit, both Phase 1's: **(i)** land the `.gitignore` entries for
every generated album path, in the `master-pack` idiom (rule in the comment, not a bare
filename — see the header comment on the `*.gba` rule), and teach `make clean` the same paths;
**(ii)** keep `album.toml` — never the assets — as the tracked artifact a reviewer reads, so an
album is reviewable and rebuildable by someone who does not have this machine's `~/Music`.

| Tool | Job |
|---|---|
| `scripts/album_bake.py` | the driver: read config, resolve tracks, check budget, call the rest, write the generated Rust pin module, link the ROM. **Refuses before it writes** (see below). **Its Phase 1 subset is A3a (§9.0): pack a declared track list, nothing else — same file, grown in Phase 3, never rewritten.** |
| `scripts/album_prep.py` | ffmpeg → the product rate, stereo; joins the tracks into one continuous album timeline; reads tags → titles/artists and ASCII-folds them (§5.5). **This is where `ALBM1`'s `start_sample` / `n_samples` come from, and it is codec-blind: it produces PCM, not a bitstream** |
| `scripts/album_art.py` | image → `ART1` (median-cut + dither + tiles/map/palettes + palette-count refusal) |
| `scripts/album_viz.py` | album PCM → `ALVZ1` (§8). Also codec-blind: it analyses the same prepared PCM whichever backend is going to be linked |
| `scripts/gafp_pack.py` *(shipped)* | per-track GAFP for the FLAC source — **the bake calls it once per track**, and it already performs the ffmpeg normalisation, the 16,384 Hz / 16-bit / 2048-frame rate-and-profile refusal (Q14), the `flac -d` differential and `--emit-pin` |
| *lossy encode step* | **not this track's to solve, and settled for v1 by the codec track:** the encoder of record is used **off-tree** to produce assets, nothing in this repository builds or vendors it ([`ULI2.md`](ULI2.md) → risks 5–6). So the bake tool's contract is *"an already-encoded asset plus a tracklist"* for that backend: `album-bake` takes the asset bytes, decodes nothing, and does the same manifest/art/viz/cost work as the lossless path. Phases 0–4 are demoable on the lossless backend alone; **Q3** tracks an in-repo encoder, which is a codec-milestone question and not an app task. |

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
box         = "120x120"        # 120x120 | 160x96 | 240x160
palette_max = 15
dither      = "bayer4"

[viz]
streams = ["loud", "beat"]     # → ALVZ1
default = "bar"

[ui]
show = ["title", "artist", "times", "bar"]

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
`geometry.rs` names the ceiling, and §10.4 owns widening it). Refusing is correct
behaviour until then: the app must not silently wrap the clock it measures itself
against. It
re-reads everything it wrote through the shipping parsers, and a second run on the
same inputs must produce a **byte-identical** `.gba` (a real gate, and a strict
improvement on today's reproducibility, since ROM header/entry/nintendo-logo
bytes are fixed by `agb-gbafix` at a pinned version). Because nothing here is committed
(§2.3), that byte-identity claim — not a stored blob — *is* the evidence that the
generated assets mean what the config says they mean; its audio-only half is checked from
Phase 1.

### 9.1 The Make surface this implies

Same naming discipline as the rest of the tree (`native-*` / `podman-*` twins, all
listed by `make help`, all gated through `toolchain-check`, all running `format`
as a pre-step and never gating on tests):

| Target | Does | Lands |
|---|---|---|
| `make album-bake ALBUM=album.toml` | runs the bake: assets + generated pin module, prints the size/budget/duration table, refuses what it cannot fit | **1** (track list → blobs → manifest); 3 (budget/art/viz verdicts) |
| `make album-rom ALBUM=album.toml` | `album-bake` → cargo → `agb-gbafix` → `album-player.gba` (this is §1's one command) | **1 — landed 2026-11-15 (PR-3a)**; `album-bake` is a prerequisite so a build cannot skip the pack step (R11) |
| `make album-seam-test` | AP-2/AP-9's host leg: the derived geometry against the shipped FLAC figures, every track's `ALBM1`-vs-pin agreement, the eight `AUDITED_MUTATIONS` refusals, `PcmSource` on both arms, and the shipped track folded to its **FIFO pins on the host** — plus a stub-only build that must link no decoder | **1 (landed, PR-3a)** — a name §9.1 did not plan; it is the seam's gate, and `album-test` is the ROM's |
| `make album-probe-rom` | the Phase 0 throwaway probes | 0 (landed) |
| `make album-test` | AP-1/AP-2: scripted-key playback ROM in headless mGBA + audio-capture diff | **1 — not landed by PR-3a** (no keys yet; deferred to PR-3b). Its audio-capture leg also has no tool today: the installed `mgba-test-runner` (agb v0.25.0) takes a ROM and nothing else, so AP-1's content witness is on-ROM — the ring ledger plus the pins PR-3a folds — and the sample-for-sample capture diff is owed where it is genuinely needed (AP-6/AP-8, §12.2) |
| `make album-layout-test` | AP-3 host snapshots + fit verdicts | 2 |
| `make album-art-check` / `album-bake-check` / `album-viz-test` | AP-4 / AP-5 / AP-6 | 2 / 3 / 4 |
| `make album-web` | AP-8's local config server (never a build dependency) | 6 |
| **`make album-check`** | the app's aggregate, the way `flac-check` is the FLAC track's | grows with the phases |

`make clean` removes every generated album path (blobs, `ALBM1`, `ART1`, `ALVZ1`,
`src/assets.rs`, `album-player.gba`) and keeps the tracked fixture config — the same division
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
| code + tables + headers + metadata | **256 KiB** | a reserve, chosen as ~0.8% of the cart; the real number is read out of the linker map at AP-5 and will be far smaller. What matters is that it is *reserved before* the encoder sizes the audio, so a code growth never silently eats samples |
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
| **content fold on the push path** — FNV-1a-64 over the bytes as they are committed, 12,288 B/frame | **cannot live here: 553 underflows in 1,430 periods** | **measured 2026-11-15 (PR-3a, `album-play`, mGBA, one run — the outcome is the observation, not a per-byte cycle count).** Per-byte the fold is ~65 c on a 2,097,152-cycle frame, which is the order that starves a ring this deep, so the fold belongs off the audio deadline: `album-play` proves its pins in a post-audio re-decode, the same shape the product ROM already uses (`product_play_main.rs:1690-1712`). This is the application's **first non-audio cost line handed across to §10.2's ledger** (R1b) — and it is a *refusal*, not a budget |
| UI: key read, position, bar + 2 text lines, palette fade | **≤ 2% of audio time** target | **AP-0 measures it. This is a target, not a fact.** |
| viz tick (1–2 ROM reads + table + tiles/sprites) | **≤ 1% target, unmeasured** | AP-6 |
| art upload (rare, on screen change) | ≤ 1 period, off the audio path | AP-4 |
| game tick (pong/snake, ≤ 12 objects) | ≤ 3%, admitted by watermark only | AP-8 |
| **reserve** | **≥ 20%** | headroom for flash-cart fetch behaviour (§14 R3) and IRQ jitter |

The claim that must survive is `underflows = 0` **with the UI and a viz enabled**,
on hardware, not `underflows = 0` with the screen off.

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

And the app's lines above are **not** free against `100% − decode%`. The lossy
track's own reading is that the C record's 69.4% decode wall sits inside a 73.63%
gate total, i.e. **~4 points for everything that is not the decoder** — and its U4
says the ceiling itself is *"measured, not assigned"*, written down at the first
near-real-time arm "from the measured gate total plus this tree's non-audio cost".
(That 69.4% is a *diagnostic* now, not a target: the codec retired "match C" as an
acceptance criterion on 2026-11-15 and works to **≤ ~85% of the period**, which is the same
arithmetic with the app's cost as one of its terms.)
**U4 is not met (2026-11-15: 105% worst / 89% mean), but that ceiling is writable now and the
missing addend in it is this application's own cost.** So the cross-track debt runs one way:
the app owes the codec track a measured UI + viz + game cost (AP-0, AP-6), in the same
units the ledger uses, before U4's ceiling and U5's gate total can be struck. **AP-0 has
delivered the basic-UI line, and AP-6/AP-8 owe the viz and game lines.** Until then every
percentage in the table above is a target with no witness, which is why the basis
column says so.

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
| IWRAM 32 KiB | 32 KiB | measured with all synthesis tables placed as the C reference places them: `.iwram` = **25,548 B, 6,964 B left**, and the stack budget is **≈2,960 B** (2,192 B above `run()` + 768 B of descent) ⇒ **6,196 B headroom** — [`PLAN-OF-RECORD.md`](crates/uli-lite/PLAN-OF-RECORD.md) → "The stack figure in this file was stale". *(corrected 2026-11-15: this row and R6 carried PR B's superseded "~22.6 KiB port + 15.0 KiB stack" numbers)* | workable, still map-sized: UI code/tiles staging is checked against the linker map, and `.iwram` growth is a gate failure (AP-7) |
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

| Phase | Ships | Exit gate |
|---|---|---|
| **0 — Probes.** Three throwaway ROMs and no product code: (a) UI spike — 3 BGs + 1 OBJ + text + palette fade, VBlank-paced, *with the shipped lossless feeder playing underneath*; (b) art spike — a PNG on screen from a hand-written `ART1`; (c) skip spike — fade/drop/re-prime against the real ring, measuring the silence gap and proving the re-prime costs no underrun | `make album-probe-rom` + logs; **AP-0**: the frame cost breakdown closes with UI terms, the VRAM/OBJ split is recorded, `SKIP_PRIME_CHUNKS` gets its first number, and the *targets in §10 are replaced by measurements*. **Landed 2026-11-14** (`examples/album_probe/README.md`; `make album-probe-art/-test/-rom`, `make album-probe`): UI full arm **1.89%** of a period (≤2% target, and a lower bound), feeder 0.92%, video period **280,896 c** (§4.5's ~281,000 confirmed), one 4bpp tile write 726 c, art upload **0.60 period** live (+2.08% over audio-disarmed, n=1, emulator — the same upload has now read three ways and AP-4 re-prices it), `late_max` 4 samples in the skip sweep's total, inside the 16-sample grace — and `SKIP_PRIME_CHUNKS` came back **blocked on the truncate ruling (§4.4, Q13b)**, which is the finding Phase 0 existed to produce |
| **1 — It builds an album, and it plays one.** A1–A4, A6, A10, **A3a**. In order: **§2.2's lift of the feeder out of `examples/flac_spike/` into project source, with `truncate()` (§4.4) landing on the lifted type** — Phase 1 may not `#[path]` a spike. **Landed 2026-11-15 as PR-1**: `crates/gba-playback` holds `playback` + `checksum`, `RingFeed::truncate()` is on the lifted type with four host tests, `make playback-test` is part of `make flac-check`, and the skip probe's `truncate` arm measures it (3–17 periods = 102.5–581.1 ms). `SKIP_PRIME_CHUNKS` remains un-chosen; then **the pack step (§2.3, §9.0): `examples/album_player/album.toml` (tracked) → `scripts/album_bake.py`'s Phase 1 subset → N `GAFP` blobs + `ALBM1` + `src/assets.rs`, all generated and gitignored**, default material N entries over the tracked reference master plus one short clip (Q15) so a fresh clone builds with no untracked input; then the app crate — `PcmSource` + `backend/{flac,stub}` (§4.2b), the app's derived `geometry.rs`, `meta.rs` (the validating `ALBM1` reader over the per-track ROM-location map), the fade→**truncate**→re-seek→silence-cover→re-prime→fade-in feeder (§4.4), the 32-bit session clock (§10.4, ruled), the no-divide arithmetic (§6), play/pause/next/prev/restart, and title + elapsed/remaining on screen. **The `stub` and both backend arms compile from here on, and the suite runs on `stub` + `flac`; `uli2` runs when U4/U5 land (Phase 5, AP-9).** | **AP-1**, **AP-2**, **AP-9** — and the phase that hands the codec track its UI-cost addend (§10.2, R1b) and its first per-track real-music census (§10.2b, Q16). Landing it also updates `README.md` → "Build Process" and the `gba-build-and-test` skill with the new `album-*` targets |
| **2 — The shell.** A5, A7, A8, A11, A12: layout module, title card, now-playing, track list, pause menu, progress bar, cover art | **AP-3**, **AP-4** |
| **3 — The bake becomes a verdict.** A3b, A13: the **same** `scripts/album_bake.py` grows §9's remaining refusal set (budget, 32-track cap, palette count, title fit, unused viz streams, the §10.4 audit ceiling), plus `album_art.py` / `album_viz.py`, the `[art]`/`[viz]`/`[game]` config sections, and two full runs → one byte-identical `.gba` (its audio-only half has been checked since Phase 1) | **AP-5** |
| **4 — It moves.** A9: `ALVZ1` + `bar` + `skyline`, `beat`-driven sprite; art+breathe | **AP-6** |
| **5 — Hardware + the second backend.** Both GBA units, two runs each, `none` vs each viz; then the album-length build on the lossy backend — **U4 is not met (105% worst / 89% mean, 2026-11-15), so this waits on real-time decode with margin, then U5 (playback) and PR D (the output stage)**, and the app-side work is a `BACKEND=uli2` rebuild plus a fresh set of measurements, changing no screen and no tool (§4.2b is why that is the expectation rather than a hope) | **AP-7**, **AP-9** |
| **6 — Reach.** A14 web config, A15 toy game | **AP-8** |

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
is the cheapest honest answer until those sections mean something.

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
(`the_fixture_titles_are_q15s_names_verbatim`) now refuses any future decoration — a title over
§5.2's 28 cells or a non-empty track artist is a red, so this cannot silently come back.
Nothing is lost as coverage: non-ASCII, a >28-char title, a whole wrapped title and a
title that folds to nothing are all `scripts/album_prep.py --self-test` cases on inputs
labelled synthetic (7 fold cases + the box report), which is where a rule gets tested
against *every* branch instead of against whichever word happened to be in a title. What
§5.2's wrap/ellipsis rule still lacks is a **screen** to overflow — that is AP-3's, in
Phase 2, and it will need such a string in a layout fixture rather than in an album's
metadata. `album-pack-check` 46 → **45** (two decoration-shaped tests replaced by the
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
| **AP-1** | **the ROM under test was built by `make album-rom ALBUM=examples/album_player/album.toml` from a fresh clone, with no untracked input and nothing fetched (§2.3)**; ≥2 tracks play to the end of the album, pause/resume and next/prev work **from scripted button presses**, `underflows = 0`, fades produce no click, and a skip's silence is bounded by `SKIP_PRIME_CHUNKS` | `make album-test`: `mgba-test-runner` with a scripted key log, plus a captured-audio comparison against a reference decode of the same asset. **Also runs here, because the packer does: two consecutive packs → byte-identical blobs, and `ALBM1` re-read through the shipping `meta.rs` agrees with `album.toml` field by field** |
| **AP-2** | mm:ss elapsed + remaining match the durations **the same build's pack step printed** to ≤1 s (§6; the tool is Phase 1's as of §9.0, so this differential exists from the first album build rather than waiting for Phase 3); bar and time never disagree by more than one cell and never move backwards; no `__aeabi_*div*` in the playback/UI path | host twin on `position.rs`/`layout.rs` + objdump/symbol fact on the linked `.gba` + the bake's printed track table |
| **AP-3** | Every screen's map renders to the expected host snapshot from the same `layout.rs`, and every string on every screen *fits* 240×160 | host snapshot test + on-screen fit verdict (the `text::measure` precedent) |
| **AP-4** | `ART1` is reproducible byte-for-byte from the source PNG, re-reads through the shipping parser, never exceeds its tile/palette allocation (linker map + VRAM audit), and refuses an off-palette source image | `make album-art-check` |
| **AP-5** | `album.toml` + WAVs → ROM end to end; two runs → identical bytes; every refusal in §9 fires with its message — **the Phase 3 set; the four §9.0 refusals that land in Phase 1 are witnessed by AP-1** | `make album-bake-check` |
| **AP-6** | Each viz, alone and paired with art, holds `underflows = 0`, stays inside its §10.2 line, and leaves the audio cost breakdown unchanged vs the `none` control arm | `make album-viz-test` + on-target timing, `none` as control |
| **AP-7** | Hardware: GBA 001 + GBA SP, two runs each, `none`/`bar`/`skyline`/`game`, with the map-file memory report attached and `.iwram` growth flagged | two units, a notebook, and a `flac-docs/history.md`-style record |
| **AP-8** | Reach: the web config produces an `album.toml` the CLI bakes unchanged; the game under the watermark rule never admits an underrun | `make album-web-check` (headless, config diff) + AP-6's arms with `game = on` |
| **AP-9** | **The seam holds:** the same screen/layout suite passes against `backend-flac`, `backend-uli2` and `stub`; no module outside `src/backend/` names a codec crate (a build-time fact, not a code-review hope). **Phase 1 reads this as: all three arms compile, and the suite *runs* on `stub` + `flac`; three-way runtime parity is witnessed in Phase 5, because the lossy backend has no real-time decode (U4) and no in-tree encoder (R7) yet — the arms that cannot have assets cannot be blamed for it** | three `cargo` arms + a dependency check on the app crate's module graph |

Aggregate: **`make album-check`** = AP-0…AP-6 plus AP-9 on host+target, mirroring
`make flac-check`; the codec tracks' gates (`flac-check`, `uli-*`) stay the
authorities on decode, and an app change never re-decides them.

---

## 14. Risks

| # | Risk | Why it is real here | Mitigation |
|---|---|---|---|
| R1 | **The album-length build waits on the lossy backend's decode gate, and then on its playback gate** | U4 is **not met** — 2026-11-15: 105% worst / 89% mean of the period, and the codec's working bar is ≤ ~85% so the gate total and this tree's UI fit beside it ([`PLAN-OF-RECORD.md`](crates/uli-lite/PLAN-OF-RECORD.md) → "Critical path to real time"). Behind it, **U5** (zero underruns, clean period audit, mGBA then hardware) and **PR D** (output stage, incl. the speaker fold-down this app exposes) are still open. *(corrected 2026-11-15: this row and four others in earlier revisions of this file read "U4 closed 2026-11-14", which no codec document says — see §2's row for the ladder)* | the app is built and gated on the lossless backend meanwhile; the backend switch is Phase 5 and touches no screen. The seam is the mitigation — the risk is only realised if someone writes `if flac` in the UI |
| R1b | **The app's UI cost is an input to the codec's own gate ceiling**, so an unmeasured UI blocks U4's ceiling and U5's gate total on the lossy backend | U4's ceiling is "measured, not assigned": the gate total **plus this tree's non-audio cost**, and the critical path's ≤ ~85% target is derived from exactly that sum — so the addend missing from it is this application's | AP-0 has handed across the basic-UI line (**1.89%** of a period, plus feeder 0.92%) and AP-6/AP-8 owe the viz and game lines, in the same units the codec ledger uses; §10.2's targets are explicitly targets until then |
| R2 | **The lossless backend cannot hold a whole album** — the shipped clip carries 354.8 kbit/s (439,396 B for 9.91 s, `spike_l0.gfp`) ⇒ arithmetic puts ~12.6 min of that material on 32 MiB | that clip is a perf fixture, not a measure of real music, and no amount of FLAC tuning makes lossless reach album length at this depth | **now measured on real music, not inherited**: 364.0 kbit/s mid-album / 340.8 kbit/s for the head of `bioluminescent.wav` ⇒ **~12.3 min on a 32 MiB cart**, and the reference master is 4.3 min of that (Q15). **Phase 1's default album is real music**, so the first bitrate this app quotes for a real master comes out of the Phase 1 pack report, and the bake tool reports minutes-per-cart honestly. This risk is also the *reason* the lossy backend is not a nice-to-have: an album player that can only hold an EP is why §4.2b exists. Quote a measured number from here on, not 354.8 kbit/s |
| R3 | **Cartridge bus contention**: the app adds ROM *data* traffic (VIZ reads, tile uploads, art) around decoder *instruction* and audio-data fetches | mGBA does **not** model the ROM prefetch buffer, and this tree already had to arbitrate emulator-vs-hardware fetch once (`examples/e5_fetch/`); ULI2's method note says on-target numbers stay unclaimed until hardware | the ≥20% reserve; a hardware run per phase from Phase 5; the watermark rule keeps the risk off the audio deadline |
| R4 | **A skip that sounds broken**: too long a silence, a pop at the splice, or a re-prime that starves the ring | §4.4: 1.23 s is queued and must be dropped, then replaced while the DMA keeps consuming. **AP-0 found the stronger version of this risk: without a ring truncate the drop cannot be faster than one period per queued chunk, so at the ring's ceiling the skip is 1.33–1.81 s no matter what `SKIP_PRIME_CHUNKS` is (§4.4, Q13b)** | the fade/drop/re-prime sequence with `SKIP_PRIME_CHUNKS` chosen by the host model, not guessed; AP-1 measures the gap and holds `underflows = 0` across it; and Q13b ruled in the app's favour, so step 2 becomes a truncate rather than a drain |
| R5 | **Silent failure modes** (a bad blob, an album longer than the manifest's own arithmetic, an off-profile asset) | the bake tool is the only place with enough information to notice, and it is not currently the place that decides | refuse at bake time; show a named on-screen verdict ("DATA ERROR") at run time; a decode error skips to the next track and stops at the album end; the 32-bit session clock (§10.4) so a long run is still measurable |
| R6 | **IWRAM is the scarce RAM** on the album-length path, and it has already been mis-sized once in this repository's own documents | the plan of record now measures `.iwram` = 25,548 B with every synthesis table in place, **6,196 B of headroom after a ≈2,960 B stack** — which supersedes the PR B-era "~22.6 KiB port + 15.0 KiB stack" reading this file's earlier revision carried | AP-7 flags `.iwram`/stack growth from the linker map, not from a remembered number; new non-hot code stays out of IWRAM by rule; the "a few hundred bytes of RAM growth breaks playback silently" failure mode is one this project has already recorded, so the map diff is a gate and not a courtesy |
| R7 | **The lossy backend has no encoder in this repository**, so `BACKEND=uli2` cannot be baked here today | `make gafp-pack-check` covers FLAC end to end; nothing in `scripts/` encodes the lossy format | the bake tool calls encode behind one indirection with a declared interface (§9); the app is built and gated on the lossless backend in the meantime, so this blocks a *backend*, never the application (Q3) |
| R8 | **Scope**: fifteen features, two backends, a UI toolkit, an art pipeline, an analysis pipeline and a game | the natural failure is a demo that does all of them badly | phases are ordered so each one is *usable*, and A9/A14/A15 are labelled reach; §16 lists what we will not build |
| R9 | Font/charset limits and licensing | 96-glyph ASCII subset; OFL fonts; ULI2 licensing (U7) unresolved | fold at bake time and print offenders (Q9); the app is not the vehicle that forces a publication decision |
| R10 | **The app's foundation includes a spike directory** — its feeder (`crates/gba-playback/src/playback.rs` since 2026-11-15; `examples/flac_spike/src/playback.rs` before that) and its cycle counter (`examples/e5_fetch/src/timer.rs`) live where the tree's own purpose statement says code goes to be thrown away | `flac_spike` is the perf-gate `ctrl` arm and `e5_fetch` has no Make target any more; a product feature implemented there has no owner, and Q13b's ownership looked genuinely ambiguous for exactly this reason. **The feeder half is closed** (Phase 1 PR-1) | §2.2: lift the feeder + checksum + counter into project source before Phase 1 links them, keep `flac-check`/`product-play-test` as their witnesses, and re-baseline the shipped ROM bytes the move touches. Feeder + checksum **landed** in `crates/gba-playback` (2026-11-15, `make playback-test`); the `e5_fetch` counter is still owed. Phase 0's probes lean on them on purpose and are throwaway |
| R11 | **A generated asset can go stale** — the policy that keeps album audio out of the repository (§2.3) means a build can silently link *yesterday's* blob | this is the price of the policy, not an accident of it: `master.gfp`, the rate clips and every future `album_player` asset are build output, and build output outlives its config more often than source does | `gafp_pack.py --emit-pin` already writes the sha256 pins the **ROM** checks its own embedded blob against at boot, so a mismatch is a named verdict (R5's "DATA ERROR") rather than a plausible-sounding track; `make album-rom` depends on the pack step so a build cannot skip it; `make clean` removes the generated paths; and AP-1's pack-twice check plus AP-5's byte-identical-ROM check make "generated" mean **regenerable**, not "whatever is lying around" |

---

## 15. Open questions

Numbered in this file's own space (the codec tracks keep theirs).

| # | Question | Provisional ruling |
|---|---|---|
| Q1 | Which backend is v1's default *artifact*? | **The application supports both and favours neither** (§4.2b, AP-9). U4 is not met yet (105% worst / 89% mean, §2), so the lossy backend is a *near* candidate for the album-length image; it becomes a real one when U4 lands with margin and the default once **U5** gives it zero-underrun playback on hardware — the lossless backend stays the default until then because it is the one with hardware witnesses today, and it stays a build forever, as the A/B control. Either way this is a shipping decision about which artifact `make album-rom` builds by default, not a design assumption: nothing in the app may depend on it. |
| Q2 | App-side manifest, or extend a codec's container with album fields? | **App-side `ALBM1`, for both backends equally** — the decoupling argument is §4.3: an album feature must be available to both codecs or it is not a feature, it is a branch. Revisit only if a future format revision is happening for codec reasons anyway. |
| Q3 | An in-repo encoder for the lossy backend? | **Settled for v1 by the codec track** (2026-11-14): the encoder of record is used off-tree to produce assets, nothing is built from it and no C is vendored. This app therefore bakes from a *pre-encoded asset* on that backend. A Rust encoder remains a codec-track milestone; nothing in Phases 0–4 blocks on it. |
| Q4 | Gapless playback? | ULI2 is gapless by construction (one stream, track boundaries are positions). The FLAC path is N blobs and will gap **audibly** unless the ring is bridged at a track boundary. **v1 rules: lossless path has inter-track gaps, and we say so**; bridging is a Phase 5 item. |
| Q5 | Seek inside a track? | Deferred past v1. The UI affordance (a scrub screen) is the work; granularity is whatever the linked backend offers, which is why it arrives as a `PcmSource` **capability** (§4.2) and never as a constant in a screen. |
| Q6 | Does `bands` earn its ~243 KB in v1? | Probably not: `loud`+`beat` is 91 KB and drives two of the four visualisations. **v1 ships `loud`+`beat`;** `bands` is a switch, priced in the bake report, and the spectrum view is its own PR. |
| Q7 | Art box size and default mode? | 120×120 `cover` for art-only; 160×96 when a viz is on. To be confirmed on a real screen in Phase 2, not in a document. |
| Q8 | Output profiles (headphone vs the built-in speaker)? | **In scope, and ruled into the output stage by the codec track** (2026-11-14): the decoder always produces full stereo, and the mono collapse happens *after* it, never by shortening synthesis. That is this application's layer, so the fold-down is the app's to expose (Select, §5.3) and the seam already returns the int16 it needs. The **method is PR D's to measure** — `(L+R)/2` versus `L` only, which are *not* equivalent (hard-panned content vanishes; on an M/S frame `L = M + S` is mid+side, not mid). Ship whichever the measurement supports, and record its cost in the gate total. |
| Q9 | Font coverage / non-ASCII titles? | ASCII-fold at bake, print offenders. Extending the subset (Latin-1) is cheap; anything beyond that is a separate decision. |
| Q10 | Resume position in SRAM? | Optional, cheap (agb has `save`), and needs a save-type header decision. Phase 5. |
| Q11 | One album per cart, or a picker? | **One album per ROM.** A picker is a filesystem and an allocator, and the encoder's album-wide rate allocation makes a mixed cart a worse product. |
| Q12 | Mono vs stereo, and 8-bit vs int16 tap for viz? | **Stereo is the product target, ruled 2026-11-14** — fidelity is the project's purpose and is not traded for CPU headroom. The app receives int16 from `PcmSource` and owns the int8 conversion at the output stage, so a speaker fold-down or dither change never reopens the seam (§4.2). |
| Q13 | Where does the codec track's PR D stop and this application start — the output stage, the feeder, **and whether the UI reads the codec's tables or the app's manifest**? | Overlapping ground, named rather than papered over. The feeder is already shared in this repo's idiom (`RingFeed` through `crates/gba-playback` since 2026-11-15; `geometry.rs` through `#[path]` to this day), and the lossy track's PR D claims "reuse the ring/FIFO feeder … int8 direct … UI from the container's tables" — that last clause is the boundary §4.3 pushes back on. **Proposal:** the feeder / convert / fold-down lives in one shared module that both the product ROM and this app link (PR D's fold-down measurement lands there), the *player* (screens, manifest, keys, viz) lives here, and the adapter cross-checks the container's tables against the manifest so both statements hold at once. One piece of that boundary has since settled on its own terms: the ring/feeder is project source the *application* owns (§2.2) — `crates/gba-playback` since 2026-11-15 — so `truncate()` (Q13b) is not a cross-track ask at all. What still needs an owner's ruling is where PR D's output stage stops and the player begins, because that decides where U5's gate gets written. |
| Q13b | **May the shared feeder *truncate* the ring at a skip?** (raised by AP-0, 2026-11-14) | **Landed 2026-11-15 (Phase 1 PR-1).** The 2026-11-14 ruling was *yes — this is the mechanism §4.4 step 2 means, and the application grows it*, and it is now the shipped method: `RingFeed::truncate()` in `crates/gba-playback/src/playback.rs` sets `consumed = pushed` under the same critical section `take_chunk` uses, keeping `consumed <= pushed` and "a starved period consumes nothing" intact. **Witnessed:** four host tests in `make playback-test` (it returns exactly the prior `ready()`; `ready() == 0` afterwards with the producer ledger untouched; the next push delivered whole, across a ring wrap too), and — measured, not derived — the skip probe's `truncate` arm reads 3/5/9/17 periods = 102.5/170.9/307.6/581.1 ms to new-track audio at prime 2/4/8/16 (`min == max` over 20 skips each), against the 39–53 periods (1.33–1.81 s) that drain and consume-and-discard still read at the 36-chunk ceiling. **Ownership:** ours, because §2.2's lift put the ring in project source; the old "the codec track's file" framing was wrong and is recorded as such in `examples/album_probe/README.md`. **Consequence:** `SKIP_PRIME_CHUNKS` is a Phase-1 product choice — still un-chosen by this PR — because `to_new = SKIP_PRIME_CHUNKS + 1` periods regardless of queue depth, so the ≤ ~0.6 s observable is met at every prime in the sweep and what remains to decide is the length of silence the listener hears between tracks. |
| Q14 | Does the app's reaction data need the lossy backend's int16 path, or is int8 enough? | Neither, as designed: the visualiser reads the **album timeline**, not the PCM (§8). This only reopens if the runtime-analysis experiment is taken, and then it is a measured cost against the gate total, not a free tap. |
| Q15 | What is Phase 1's album made of, given per-album assets are not committed (§2.3)? | **Ruled 2026-11-15: N entries over one properly-encoded master.** The fixture repeats the same encoded `assets/sound/bioluminescent.wav` as full-length tracks titled "Bioluminescent 1", "Bioluminescent 2", …, plus at least one short **"Bioluminescent Clip 1"** so the list carries *unequal* durations, alternating clips and full tracks to scale to whatever portion of the cart a test wants. Real music, zero new tracked bytes, and it exercises the source → ROM pipeline Phase 1 exists to build instead of a synthetic stand-in for it. The consequences are measured, and they are this ruling's to handle: |
| Q16 | Who measures the decode rows an album actually hits — FIXED-1/2/3 at 2048-sample frames — and does Phase 1 wait for them? | **Recorded, and not the app's to close** (§2.1: this file reopens no codec decision). `MEASURED_DECODE` holds one row at the product block size, `(2048, "fixed", 0)`, and real music contains no order-0 frames (§10.2b) — so every album-shaped build packs under `--master`'s blanket waiver instead of against a measurement. **The ask of the FLAC track:** three hardware rows for `(2048,"fixed",1/2/3)`, taken on frames whose `frames_sha256` the album bake can reproduce, so the guard can check real music against real numbers and retire the waiver on the product path. **Phase 1 does not wait:** it budgets against C2's measured real-audio aggregate (decode 23.656%, work/frame 32.208% on hardware) and labels that, in its own log, as an asset-level mean rather than a worst-frame row. The per-track census the bake already prints is the app's contribution, handed across with the UI-cost addend. |
| Q17 | Does `assets/sound/test.wav` still earn its 17 MB? | **Answered 2026-11-15: kept, and renamed to what it actually is — `assets/sound/bioluminescent-32768hz-8bit-stereo.wav`.** Measured before naming: `pcm_u8`, **32,768 Hz**, **2 channels**, **8 bits/sample**, block align 2, 16,948,526 B, 258.61 s, 524 kbit/s, encoder `Lavf63.1.101` — exactly the form `scripts/convert_stereo.sh` writes (`-ac 2 -ar 32768 -c:a pcm_u8`), of the same performance as the master (correlation **+1.000** at t = 10/60/200 s). So it is not a second recording and not a stray: it is the 32,768 Hz-era render at the 8-bit played depth, regenerable in-tree from a tracked input, which puts it squarely in §2.3's instrument class. What it was *not* earning was its name — `test.wav` said nothing about rate, depth or channels, and the script's default output path pointed back at it, so every fresh conversion re-created the same ambiguity. The rename is the fix; the script now names its output after what it contains. |

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
- **Free extra, and worth taking:** let one fixture title exceed 28 characters (driving §5.2's
  wrap/ellipsis rule) and one carry non-ASCII (driving §5.5's fold and its offender report,
  which is also Q9's evidence). Titles are config, so this costs no bytes and no encode.
  *(revised 2026-11-15, PR-2 **D5**: taken, but not in the fixture — the tracked `album.toml`
  carries Q15's plain names and these cases run as synthetic inputs in `scripts/album_prep.py
  --self-test`; inventing a recording to exercise a rule puts fake metadata in the one artifact
  a reviewer reads. §5.2's overflow needs a layout fixture at AP-3, not an album title.)*

---

## 16. Non-goals

No on-device file browsing or multi-album carts (Q11). **No album audio and no per-ROM
assets committed** — they belong to whichever album was built, so the build generates them
and git ignores them (§2.3); the repository keeps the instruments and the reference master
under `assets/sound/`, which are a different class of thing. No streaming, link-cable
or MIDI input here (goal 3 is its own ROM). No new codec work, no format
revisions, no encoder rewrites inside an app PR. No runtime FFT. No video, no
per-frame art streams, no alpha blending beyond palette tricks. No general GUI
toolkit: `layout.rs` + a `Screen` trait is the whole abstraction, and it earns
extraction into a shared crate only when a **second** application needs it (which
is the plan for the toy-game ROM, if it survives). **No part of this application
depends on anything outside this repository** — no external code, assets, byte
layouts or tools are cited as authority or linked in (§2.1). No claim of real-time,
hardware playback, battery life or "works on my cart" that has not gone through a
gate in §13.

---

## 17. Definition of done, v1

A ROM that a person can build from a folder of WAVs and a cover image with one
command, that boots to an album title card, plays the whole record at the
codec's accepted fidelity with 0 underruns **on both GBA units**, shows a
dithered cover, a track title, elapsed and remaining time and a progress bar that
agree with each other, lets Start pause and Resume, L/Left and R/Right skip with
no click and correct on-screen position, lists all tracks and plays any of them,
animates something in time with the music, and whose whole application frame
still closes in a ledger where the residual is named — on the lossless backend,
and behind the same screens and the same manifest on the lossy one whenever that
backend is ready, with every target number in §10 replaced by a measurement.

---

## 18. Companion files

- [`FLAC.md`](FLAC.md), [`flac-docs/`](flac-docs/) — the lossless source this app plays first.
- [`ULI2.md`](ULI2.md), [`crates/uli-lite/PLAN-OF-RECORD.md`](crates/uli-lite/PLAN-OF-RECORD.md) —
  the album-length codec, its container, and the gates this app waits on: **U4 (real-time
  decode) is not met — 105% worst / 89% mean as of 2026-11-15, with U5 (playback), PR D
  (output stage) and U6 (memory) behind it**, and the plan of record's "Critical path to real
  time" section is the authority on all four. Read numbers there, not here: this file has
  already carried two of its superseded figures forward ("U4 closed", "15.0 KiB stack"), and
  both corrections are marked inline (§2, §10.3, §10.4, R1, R6).
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
- [`README.md`](README.md) — "Project ideas (top-level ROMs)" lists this as an
  idea; this file is where that idea stops being one. The README should gain a
  pointer to this document when Phase 0 lands.
- [`examples/album_probe/README.md`](examples/album_probe/README.md) — **AP-0, the
  Phase 0 measurement record** (landed 2026-11-14): the probe ROMs' tables, §10's
  target→measured annotations, the deep-queue drain/drop sweep behind §4.4's ruling
  and Q13b, and the NOT VERIFIED list of everything an emulator run cannot claim.
