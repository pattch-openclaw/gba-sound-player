# Open questions for a future session

Questions this project has already *earned*: each was raised, or left open by
name, in a landed record, and none has an answer in these docs. They are collected
here so a session does not have to reconstruct them from nine sections.

Rules for reading are restated at the top of the extracted section below. In
short: every Q names the record that opened it and the rung that owns it; a
question already answered in these docs is **not** listed; nothing here
re-declares a decision, a bar, or a reading rule; and a partial answer is written
down so it is not re-litigated. Answering one owes the standing landing record
(control build, section/`nm` table, gate counts, mutation witness), not a
paragraph.

Current tree state and the live plan are in [`../FLAC.md`](../FLAC.md) and
[`plan-of-record.md`](plan-of-record.md). Numbers and era rules:
[`measurements.md`](measurements.md).

---


**Not a measurement, and not a plan.** The plans of record (items 5 and 6) say what
will be built and what bar it is judged against. This section is the residue:
questions that this file has already *earned* — each one raised, or left open by
name, in a landed record — gathered where a session that arrives later can find
them in one place instead of reconstructing them from nine sections. Rules for
reading it: every Q names the record that opened it and the rung that owns it;
a question whose answer is already in these docs is **not** listed (so the list is
shorter than the file's number of "TODO"s would suggest); nothing here re-declares
a decision, a bar, or a reading rule; and where a Q has a partial answer, that
half is written down so it does not get re-litigated. Answering one of these owes
the standing landing record (control build, section/`nm` table, gate counts,
mutation witness), not a paragraph.

**Tree state when this section was written** (kept for the first Q's
answerability, updated 2026-11-08): item 6a is up to **6a-5**, which is on the
branch `feat/flac-item6a5-realization-reading` (the threaded `…_with` seam from
`frame` down to `residual`, the spike's feature-gated arm pass and its
`item6a5-test` witness, the two arm images and the 6a-5 landing record — not yet
merged). 6a-4 is merged (PR #101, `f1fb6da`); 6a-3 is merged (PR #98, `8711e2f`);
6a-2, 6a-1 and the toolchain pin are merged (#95–#97). The **product** image is
unchanged since item 5 PR 4 — `flac-integration` is byte-identical through both
6a-4 and 6a-5 — while the instrument's `.text` has moved with the harness (6a-5's
record attributes every byte). The cycle record still reads:
`.text` 46,008 B `f0f88c33d150…`, `rice_quotient` live, `decode_residual` 596 B,
`flac-gates` 1,169 passed / 6 ignored. The live `nm` anchor is
`LEADING_ONES @ 0x0300022c` — but only in an image with **no** second mechanism, which
is every image this record's default/control arms measure. Once a second `RiceKernel`
body is reachable the table *relocates* to `0x0300042c` with its bytes unchanged (see
the 6a-3 section table, arms T/H); so no reader should treat `0x0300022c` as a
scheme-wide constant, and no kernel-check may hard-code it.

**Checked again today (2026-11-11, `main`, `nm` on the pre-`gbafix` ELF of
`make native-flac-rom`).** The product image *does* now name a second mechanism, and
the whole point of that caveat has arrived: the IWRAM witness in the shipping image
reads `arm_rice_partition` **0x0300022c**, the core's `LEADING_ONES`
**0x030004d4**, and the overlay's `CTZ8`/`REV8` **0x030005f4**/**0x030006f4**, inside
a 2,036 B `.iwram`. The address that paragraph above used for `LEADING_ONES` is now
held by the ARM kernel body, and the table has moved again from the `0x0300042c` the
6a-3 arms recorded. Anyone re-witnessing an anchor should read these from their own
build, not from any file. The gate aggregate for the same tree is 1,498 passed /
3 ignored (the per-arm table, and the 1,476 → 1,498 attribution, are in
[`plan-of-record.md`](plan-of-record.md) §"Baselines to hold").

## The questions

### Q1 — Is the ARM kernel *right* through the seam on the device, and on whose tables?

**Opened by:** item 6a's row 6a-4; the boundaries item 5 handed forward; the corpus
blindnesses 6a-3 discovered rather than assumed. **Owned by:** 6a-4 — **answered
2026-11-08**, and the answer is in [`plan-of-record.md`](plan-of-record.md)
("6a-4 — landed"). The short form: (a) the ROM sweeps the same committed corpus and
the same synthesized case set the host does, and the asm, the model and the
incumbent agree on both; (b) the overlay owns its three tables' copies (544 B,
`.iwram` 812 → 1,356 B), witnessed by re-derivation and FNV digests on the host and
by runtime addresses on the device — never by a hard-coded address; (c) the
`unsafe` budget is written down and counted: **0 in a default build, 3 items with
`rice-arm`, 6 with `iwram-pin` on top**. And the half nobody could have reasoned
out: the first landed cut read its three stack arguments two words too high, every
host arm stayed green, and the ROM's first end-of-stream case convicted it.

Three halves, and none of them is about speed. (a) The kernel's boundary list is
long enough to be its own review, and the only oracle for ARM code here is the
emulator (this host is aarch64 — the stated reason `examples/arm-kernels` exists):
the spelled `q = 31` leg, the `w == 0` handoff signal rather than an answer value,
the `p = 0` mask leg, §9.2.7.3's two guards, the unterminated/short-remainder
**restore-nothing** contract, reads bounded by `region()`. The kernel-check ROM must
sweep the **same committed golden tables** — no second transcription of any
expectation. (b) Table ownership was deferred out of item 5 explicitly: does the
overlay carry the core's `CTZ8`/`REV8`/`BRUIJN_INDEX` behind a visibility bump, or
own copies? 6a-3 measured the cost of the second choice: a foreign kernel that
*used* those tables hits exactly four private names (`CTZ8`, `REV8`,
`BRUIJN_INDEX`, `rice_unmap`) and only the fourth was widened, deliberately. It
also put a price on reachability: **`.iwram` 812 → 1,324 B (+512)**, with
`LEADING_ONES` *relocated* to `0x0300042c` with its bytes unchanged. So the IWRAM
arithmetic in item 5's flip record (net +256 B) does **not** apply at the overlay
default, where the incumbent table stays live — the honest overlay figure is +512 B
pinnable. And no kernel-check may hard-code a table address: it moved with no source
change to the table at all. Whether the tables survive is a property of
reachability, not of the `link_section` attribute (arm P proved an unreachable
walk's tables are dropped by the link).
(c) The `unsafe` budget: this is the first real `unsafe` code in the repo. One
SAFETY block per FFI site, each naming the bounds it relies on, with a counted and
named exception list in the overlay's crate docs — the core's placement-attribute
discipline, copied rather than improvised. 6a-3's budget is still zero; 6a-4's is a
decision it must write down, not inherit.

### Q2 — Which image is each cycle number from, and can one image carry three arms?

**Opened by:** 6a-3's codegen/cycle table. **Owned by:** 6a-5 — **answered
2026-11-08, and the answer is "no, not in one image"**, with numbers. Three images
were measured: the shipped one (A), `ctrl` + the ARM kernel (C, the product shape)
and `ctrl` + both mechanisms (B). The asm arm's *absolute* cost is link-independent
(183,288,727 vs 183,231,371 cycles — 0.03% apart), but `ctrl` moves by −36,543
c/frame with one extra mechanism and −202,529 with two, exactly as 6a-3 found. So
the arms can share an image for a **paired, same-image** reading, and the number
that belongs to the carry must come from the product shape: asm − ctrl is
−419,236 c/frame in B and **−584,856 in C**. The Rust cross-check inherits the same
rule: its absolute cost reproduces item 5 PR 4 within 2.4%, while its paired delta
reads +164.2 c/sample against a control that itself moved. Full tables:
[`plan-of-record.md`](plan-of-record.md) "6a-5 — the realization reading".

The row plans "three arms, one image". 6a-3 measured why that is not free: putting
a second `RiceKernel` body in a link moves the **default** path by ≈168–199 k
cycles per frame — four orders of magnitude past the ~9–13 cyc/frame band the
pre-declared fork is written against — and moves the worst frame from 136 to 147.
A module *nothing calls*, whose own code the link throws away, is enough to do it
(−26.5 M / −26.7 M with the module present and unreachable; a further ≈4.4–4.8 M
when it is made reachable). So the open design question is not whether to run the
arms but how to number them: `ctrl` has to be measured in an image that contains no
second mechanism, arm-vs-arm in a second image, and every figure names its own
control in its own session. Two corollaries, already stated elsewhere and repeated
here because they are the ones a late-arriving session gets wrong: the
280,848,260 / 1,037,633,086 baseline belongs to an image **without** the arm, and a
worst-frame number without a named arm is quoting a different frame. The
cross-check 6a-3 carried forward — does the carried Rust mechanism reproduce item 5's
+42.6 c/sample through the shipped path — happens here, as a same-image reading, not
as a comparison against item 5's record.

### Q3 — Why did a code-size increase *buy* 26–31 M cycles?

**Opened by:** 6a-3's cycle table. **Owned by:** nobody yet; it is offered, not
assigned.

Arm P (module present, unreachable, +2,984 B of *other* code) is 26.5 M cycles
faster on FIXED and 26.7 M faster on LPC than the control; arm T/H add another ~4.4–
4.8 M. The decode-path symbols that changed are named (`decode_residual` +1,888 /
+1,216, `decode_subframe` +688, a second `read_bits` body appearing while the first
shrinks, `integrate_lpc` +20), but 6a-3's record refuses to guess *why* those shapes
are faster, and nothing since has. This is the same mechanism as the +2,984 B and the
+3,120 B findings (a second consumer re-decides how the inliner treats the code it
shares a codegen unit with) seen from the profitable side. It is worth pricing
because it is a *free* 9.4–11% on the default path that nobody chose, and because a
measurement plan that does not know it can accidentally buy it and read it as a
mechanism win. Cheapest way to close it: the sub-stage/instruction-level attribution
6a-8 describes, applied to the pair of images rather than to a kernel leg.

### Q4 — What is the exact width guard, is it per frame or per subframe, and what does a both-legs build cost?

**Opened by:** the width decision (6a-6's row) and E4's own numbers. **Owned by:**
6a-6.

**CLOSED 2026-11-09 (6a-6).** The guard is `Σ|c_j| · 2^(subframe_bits−1) ≤
i32::MAX`, armed per **subframe** (the grain the parsed fields support; the frame
reports the OR of its halves). Both legs are gated (`flac-test` / `flac-test-wide`)
and the wide escape is reachable — a boundary test and a same-build differential
witness it. The both-legs codegen cost is +440 B of `.text` on the default
instrument (record: `plan-of-record.md` 6a-6). The cycle marginal is closed by
6a-7: the same-image i64 leg costs **+44.1 c/sample** (FIXED) / **+1,759.1** (LPC).

The decision is taken (i32 default, feature-gated, armed per frame at parse time
from data the header already parsed, i64 as the escape leg rather than an error, and
which leg answered reported per frame). What is *not* taken is the formula and its
grain: E4's bound (`order × |coeff| × |sample|`, with the concrete
`4 × 2^(prec−1) × 2^16`) holds to precision 13 and **fails at 15**, and the mid/side
`>> 1` margin is sample-width-dependent, so E4's `3 × 2^16` figure does not
generalize on its own. Two more open halves ride along: (1) making i32 default means
`make flac-test` stops witnessing the i64 leg while every count stays numerically
identical — the companion wide-shaped gate and `tests/width_two_mechanism_differential.rs`
are not optional, they are the only thing that keeps a moved *meaning* visible; and
(2) a both-legs build carries both legs' arithmetic, which item 5's and 6a-3's
measureings say is not free — 6a-6's gate prices it in the configuration a ROM links.
And the size of the win must be re-taken, not inherited: E4's "LPC −46.90%" and E9's
"−58.53%" are era-denominated percentages of exactly the class 6a-2 showed moving by
three points; the absolute equivalents that survive the era boundary are the ones to
carry (E10 `tab − arm` = −715,300 cyc/frame; E9 `arm − i32` = −78,550 cyc/frame on
FIXED, and its LPC equivalent is *not* stable across denominators).

### Q5 — Does E9's additivity claim reproduce, and would a better-written portable kernel win?

**Opened by:** E9 (additive composition) and item 5 PR 4's own "what this record
does not settle". **Owned by:** 6a-7 (confirmation, no new code) and, if anyone
wants the second one, a new probe.

**Q5a CLOSED 2026-11-10 (6a-7).** Additivity reproduces: `asm_i32 + width` =
506,719 / 5,336,433 c/frame vs 6a-5d's `asm_i64` 506,413 / 5,336,152
(+0.06% / +0.005%). Q5b (a portable kernel written for Thumb-1) remains open and
is a new probe, not this item.

Q5a is a confirmation on the current tree of a pairing E9 measured as additive, and
it is the configuration the product ships; its second job is re-anchoring the E
numbers on the post-6a image, which 6b-1 inherits. Its first input now exists:
6a-5 measured the kernel leg *through the seam* (1,167,444 c/frame FIXED in the
product shape) and 6a-6 has moved the width leg, so the stack can be read as two
same-image marginals rather than against E9's vendored arms. Q5b is genuinely open: neither
E10 nor PR 4 tested a portable-Rust kernel *written for* Thumb-1 register pressure
and a 32-bit source window — PR 4 tested the twin as written, which is the
corpus-differential witness made executable. If someone wants that sign settled it
is a new probe, and an E8-style sub-stage probe would price the legs rather than
adding another whole-kernel arm (6a-8, conditional on 6a-5 landing near the bar).

### Q6 — What compiler produced the era's code, and can the pin be moved forward without losing the thread?

**Opened by:** 6a-2. **Owned by:** whoever next edits `rust-toolchain.toml`.

6a-2 narrowed the cross-record LPC gap (a constant **+66,403,116** cycles) to one
leg — the i64 LPC arithmetic: it appears on every i64 leg in both archives and on
nothing that uses the i32 leg, and on no FIXED clip. It cannot identify the
mechanism, because the era's binary is gone: E10's measurement commit (`5525336`) is
not an object in this repository, and its own clean-worktree rebuild already printed
a different ROM sha than the image it measured. What is settled is the discipline:
the compiler instance is now a string (`nightly-2026-09-04` = `a69a63265`), a
version string does not pin an instrument, and editing the pin is a codegen change
that carries the four-arm control plus a run log. The open arm is therefore narrow:
a future record that says "the pin moved" — and whether the era gap moves with it.
Two adjacent unknowns to keep in view while that is unanswered: the ~9–13 cyc/frame
layout-noise band is budgeted for *recompiled* layouts, and the only determinism
control that has been taken (PR 4's docs-only image) held the layout, so it bounds
re-runs of a fixed image at exactly zero and says nothing about a recompile; and
`agb-gbafix` is now pinned at 0.25.0 `--locked`, which is byte-neutral here but has
never been verified on another machine (it rewrites the ROM header every sha in this
file is computed over).

### Q7 — What does the frame walk cost once the facade turns it on, and what replaces the frame-number rule when there is no table?

**Opened by:** the `scan` feature's own note; the loop rung's end-of-stream boundary;
4b's "the format has exactly two answers". **Owned by:** 6b-1.

The charge is already measured and deferred with a name: **+3,120 B** of `.text`,
attributed symbol by symbol in 4b's link-line table (`decode_residual` 596 → 2,412,
`decode_subframe` 888 → 1,576, a newly outlined `peek_at`, one extra specialised
`read_bits`), deferred to the rung that turns the feature on — which is 6b-1, "not a
judgement call". No cycle of the walk has ever been measured on-target; the walk's
real-time cost is charged to the run that wires the facade onto it. The harder half
is a correctness question the loop rung explicitly left *bounded, not answered*:
the frame-number monotonicity rule that made the frame finder exact on real streams
is a property of table-backed sources, and a table-less scan has to replace it with
something defensible. The seam it can stand on is measured: `read_bits` reports
`EndOfStream` while `read_u` reports `FrameSync`, so "no bytes left" and "bytes
left, not a frame start" are different signals, and only one of them ends a stream —
and 4b's other two answers (a frame ends itself; the end of the bytes is the end of
the stream) are implemented and witnessed with no index in front of them. Two API
questions land in the same place: whether the crate's flat `Error` grows payloads or
these conditions become source-level rejections before decode (the facade's call,
recorded at the loop rung), and whether the two per-frame `[PredictorState; 2]`
shapes duplicated across `driver` and `scan` merge. Note the consequence for the
header-width follow-up: 4a replaced the width `debug_assert` with derived bounds and
deliberately did **not** add a width `Error`, because every value that could exceed
the ceiling is refused for its own reason — that reasoning presumes the parser is
handed bytes somebody chose. Table-less scanning hands it arbitrary bytes.

### Q8 — Is the pack-time profile guard load-bearing, and was the throwaway rate harness carrying anything unwritten?

**Opened by:** the split proposal's Phase 2 step 6; the rate-play path's generated
offset table. **Owned by:** 6b-2 and 6b-3.

6b-2 is where `scripts/pack_flac.sh`, `format::Manifest`, and
`EncodeProfile::validate` meet, and the failure it isolates is that the guard the
whole design leans on turns out to be decorative — it must convict at pack time,
not at decode time, and the `EncodeProfile` wording still has to obey Correction 3
(the enforceable constraint is max predictor order plus a fixed/LPC flag, never a
"reject LPC" assumption baked into the parser). 6b-3 retires the spike's generated
offset table in favour of a real manifest and isolates the opposite failure: that the
throwaway harness was load-bearing in ways nobody wrote down. Its witnesses are the
`rate-*` flow as it has actually been run before — assets staged, ROM built,
measured — through the manifest instead of a hand-computed index. The seek
witness is no longer 6b-3 work: `SEEKTABLE` + `Frames::anchored(n)` landed in 6b-2
as a stream-side seam, and the product path still has no random access. Ordering
items that never ran: the rate-sweep re-run on the
stacked path, and E5's hardware arbitration — which is also the only thing that can
turn the standing "no GBA real-time claim" line into a claim. Every verdict line in
every arm of every image in these docs still prints `OVER BUDGET`.

### Q9 — Which gates have never been run anywhere, and which ones assert nothing?

**Opened by:** 6a-1's honest caveat rows, the toolchain pin's container half, and the
project skill. **Owned by:** the next PR that touches each surface.

Stated plainly so nobody reads a defined target as a tested one: every `podman-*`
target in this Makefile — including `podman-overlay-test` and `podman-overlay-test-twin`
added in item 6, and `podman-rom`, which is the missing arm of the pin's container
half — has **never been run** in this harness (rootless podman cannot start here:
`newuidmap` absent, no podman machine, no nested virtualization). `make test-rom`
runs, exits 0, and prints `Running 0 tests`: the root ROM `#[test_case]` suite
collects nothing, which is a standing repo state, not this item's bug — and it is why
6a-5's row sentence "first PR where `make test-rom` runs" is already false as
written; what 6a-5 needs is a **non-empty** ROM suite, and 6a-4's kernel-check is
the first thing in the item to add one. `rate-test` is not runnable when the
generated assets are gitignored (several records say so). README's
gate aggregation is landed for the core — `make flac-gates` runs seven
configurations plus the combination arms, with `flac-test-wide` joining at 6a-6 —
but its `podman-flac-gates` sibling is still open. And `flac-portable-check` is a *compile*
gate: nothing in this tree measures a non-GBA target's performance, so no
host-side performance claim of any kind is available, in either direction.

### Q10 — Do the committed corpora keep witnessing what the new code needs witnessed?

**Opened by:** item 5 PR 3's corpus coverage lines, 6a-3's mutation arms M2/M6, the
hand-packed-vector rule. **Owned by:** 6a-4 (cheapest fix), then anyone extending the
corpus.

The corpus used by every Rice differential in this tree is **4 vectors, 11 partitions,
1,015 samples, parameters {0, 2, 3, 6, 8}**, and it is blind in three measured ways:
it contains **0 escaped partitions**, **no sample that folds to `u32::MAX`**, and
**no codeword that overflows the §9.2.7.3 guard** — so both rejection paths and the
escape path are witnessed only by in-crate spec-text comparisons, and mutation arms
M2/M6 are green *correctly*. Worse, and this is the one 6a-4 closes: as M3/M5 showed,
nothing in the corpus can tell one kernel from another, so a differential can be
keeping itself honest by comparing a kernel with a copy of itself; that is why both
source-level pins exist. The 5-bit-parameter remainder leg (`order` 17…31) is reached
only by a spec-text sweep, not by real streams; the cursor-restore difference between
the incumbent and a spec-order kernel on an unterminated final codeword is permitted
by the trait contract and is recorded as **untested**, not passed. The standing fix was
the one the plan named — a kernel-check ROM sweeping the **same** committed golden
tables (shared-consumer rule — never a second transcription of an expectation) —
and 6a-4 landed it: `examples/overlay-kernel-check/` parses the corpus through one
`#[path]`-shared source and runs three legs over it on the device, so the corpus can
now see a kernel swap. The census stays a **tripwire in both consumers** (host and
ROM): 11 partitions / 1,015 samples / parameters `{0, 2, 3, 6, 8}` / 0 escapes /
longest unary run 39. Widening the corpus now fails two gates and says so.

### Q11 — Is anything in these docs still a phantom?

**Opened by:** the record's own habit. **Owned by:** whoever reads next.

The known-likely list, offered as a standing invitation rather than a claim, in the
order the file's own corrections arrived: frame fixed fields are **32** bits, not 31
(Correction 1); channel assignment is **4** bits; the `side_bit` swap was a phantom
(Correction 2); `-l 4` does **not** mean "FIXED order 0–4" (Correction 3, and it is
load-bearing on the profile wording); `--force-utf8-legacy-noop` is not a real flag;
the frame header has **14**-bit sync (`0x3FFE`) so a frame starts `FFF8`/`FFF9`;
CRC-16's init is **0x0000**, not the scaffold's 0xFFFF, and the "reflected FLAC
variant" reading is excluded outright, not merely unfavoured; native FLAC has **no**
metadata-block CRC (the question needed no re-probe); libFLAC does **not** reject a
non-minimal coded-number form (the witness varied the frame's CRC, not the form) so
our stricter rule stands as a *deliberate divergence* justified by canonicity, not by
observed reference behaviour; a constant-16-bit footer walk does **not** silently
slip here (at all 23 such positions no header parses), and the byte-re-anchoring
variant lands *correctly* by arithmetic — the docs said both the opposite until they
were measured; the "no byte stride describes these frames" argument is refuted as
stated by `s3`, whose two starts *are* an arithmetic sequence, and the load-bearing
claim is the tiling instead; `resync`'s first version refused the frame it had found;
and the frame-count cross-check arm was implemented, documented and **untested** until
a mutation proved it. The general lesson filed with the method notes: a "measured"
claim has to state what the command *varied*, not only what it printed — and a
zero-returning throwaway script can report a fact that merely agrees with the prose
being checked.

### Q12 — Where do the seam's 48 c/sample go, and what puts 32,768 Hz FIXED back under budget?

**Opened by:** 6a-5's stack projection (2026-11-08) — the item's first measured
negative. **CLOSED 2026-11-09 (first half), and the answer is a memory address.**
The seam half closed by 6a-5b (the seam is priced at zero in a same-image pair),
the guard half by 6a-5c (the guards *are* the rest of it — 6 executed instructions
at **4.0000 cycles** each), and the question those two raised — why an instruction
costs 4 — by 6a-5d: because it was being fetched from cartridge ROM. The same
stream in IWRAM is **2.9×** cheaper and puts 32,768 Hz FIXED decode at **48.29%** of
budget. So “where do the 48 c/sample go” is answered: six instructions times the
difference between two addresses, paid by both realizations. The width half landed
in 6a-6 and 6a-7 measured it; neither carries the full-rate claim any more. **E5 answered this**: the fetch pricing is hardware (GBA 001 and GBA SP,
two runs each, reproduce mGBA within 2 cycles/call; ratio 3.94). The remaining
question is the kernel-level stack on hardware (**6a-7b**), not the model.

**Re-opened 2026-11-08: the premise did not survive a code-level check.** The
48 c/sample is a quantity taken across two records, and the FIXED clip's own
geometry bounds most of it away before any probe runs: the clip is `MidSide`, order-0
FIXED, `wasted == 0`, **partition order 0 on both subframes, 157/157 frames** — so
there are exactly **2 kernel calls per frame**, which bounds candidates (a) (the
window bridge) and (c) (the 12-byte frame and status word) together to ~0.15
c/sample. That leaves (b), the two §9.2.7.3 guards, at **5 instructions per sample
≈ 5–15 c/sample** — the only per-sample code delta the landing introduced, provable
by diffing the two asm bodies — and pushes the rest of the overhang into the
image-effect class this repo already measured at ~99 c/sample on `ctrl`'s unchanged
code. Read the plan's 6a-5b section before using any number below.

**Measured 2026-11-09 (6a-5b): the 48 c/sample is real, and the seam is not where
it lives.** The differential ran in one link, as one more timed leg in the existing pass, and reads
**`asm − vend` = +98,291 c/frame (FIXED) / +98,153 (LPC) = +24.11 cycles per Rice
codeword = +48.22 / +48.16 c/sample**, the vendored realization faster on 157/157
frames of both clips, and it reproduces the projection to **0.4%** — because the
*arm* absolutes are link-stable (landed 183,288,727 / 183,231,371 / 183,304,113
across three links; E10's 167,911,108 vs this image's 167,872,486) while only
`ctrl` moves with the link. So the re-opening above is withdrawn: the comparison was
between two realizations all along. Candidates (a) and (c) are priced at exactly
zero by that image: both legs make the same number of kernel calls per frame (2 on
FIXED, more on LPC, equal in the two legs either way) through a linker thunk, both
read pinned `.iwram` tables, both hand the same state ABI on an IWRAM stack, and the
landed kernel's bytes are identical across four images. (d) is bounded by the same
measurements: the arm's absolute does *not* move with the link, and while the vend
leg really does carry its own reader and subframe code — that is a path difference,
and a large one on other clips — path cost scales per frame or per call, and the
measured cost scales **per codeword** (+48.22 vs +48.16 on two clips whose per-frame
work differs 3.75×). (b), the guards, is the only candidate whose scaling matches,
and its instruction count is a fraction of its price: **6 executed instructions per
codeword** (G1's `ldr [sp,#8]+cmp+bhi`, G2's `adds+beq`, and one **dead duplicated
`and r12, r3, #0xFF`** — lines 256 and 258 of the shipped asm are the same line),
3 per partition, 3,840,942 per clip, against 15,431,627 cycles measured —
**4.02 cycles per inserted instruction** would be needed for the kernel body to be
the whole of the gap, 24.9% of it at one cycle each.

**So the first half of the question has an answer, and a narrower form.** The 48
c/sample is inside the codeword loop: in both bodies the codeword loop is
`str ip,[r8],#4 / cmp r8,r9 / blt .Lloop`, and a normalised word-for-word diff of
the two disassemblies shows the *same instruction stream* everywhere else. What is
open is **which unit the cost is paid in**. Six ALU/load/branch instructions, one of
them dead, should cost ≈7 c/codeword (an IWRAM load feeding a compare is the only
stall in sight); they are worth 24.11. That gap is Q3's species — a code-size or
layout effect this project has measured and never explained — and 6a-5c separates
the two channels by construction: delete the dead instruction *alone* and the
semantics are untouched while the loop shifts 4 bytes, so anything at or past
**3 cycles per codeword** from a provably dead instruction is a fetch/layout effect,
and the item must then stop describing the guards' *semantics* as the cost and start
asking about the loop's arrangement — 6a-5d being the same instruction stream
resident in IWRAM, with E6's contrary result (“instruction fetch is ruled out”,
measured on a Rust arm that also forced inlining and took veneers) stated rather
than buried.

**Measured 2026-11-09 (6a-5c): the first half is closed, and the answer is
arithmetic.** The 48 c/sample is **six instructions at 4.0000 cycles each**. One
provably dead `and` removed from the landed kernel moved the same-image pair by
`2,560,000 = 4 × 640,000` cycles; five guard instructions removed on top of it
(diagnostic leg) moved it by `12,800,000 = 5 × 4 × 640,000`; what is left is
**71,627 cycles of 15,431,627 — 0.46%**, per call rather than per codeword, the
size the static bound predicted for the bridge. So there is no unexplained residue
to hunt: 6 × 4 = 24 cycles per codeword, and the item's mistake was reading
6a-5b's "4.02 cycles per inserted instruction" as an anomaly instead of as the
price. That price is the ARM7TDMI fetching a 32-bit ARM instruction from cartridge
ROM: **4 cycles**, and the saving scales with codewords *executed* (exactly 16,384
on each 2048-sample frame, 4,096 on the 512-sample frame), so layout is excluded
rather than merely unproven. **E6's "instruction fetch is ruled out" is reversed
for this case** — it measured a Rust path where each instruction carried a dozen
cycles of work, which is exactly the case a 4x fetch price is invisible in.

**And then 6a-5d moved the memory, and Q12's first half closed with it.** The same
instruction stream in IWRAM takes the landed arm from **1,151,236 to 506,413
c/frame** — **109.77% → 48.29%** of the 32,768 Hz budget, worst frame 48.5%, `ctrl`
cycle-identical, `.text`/`.iwram` moving by exactly the body's 680 B. So the
4-cycle price was a *ROM* price: the same instructions from IWRAM cost ~1.4, the
landed 6-instruction guard delta falls 20.11 → 7.11 c/codeword, and E10's vendored
body pinned on its own falls −290.4 c/sample. With both bodies in IWRAM the
"realization" pair reads **+14.2 c/sample instead of +48.2**: three rungs' worth of
differential was six instructions multiplied by the difference between two
addresses, and the only part left is the guards at IWRAM prices. **Q12 is therefore
closed for the first half, with the answer "it was placement, and the fix is
2.9×".** What it hands to E5 is a specific, high-value hardware question: is the
ROM→IWRAM fetch ratio 4→1.4 on real cartridge hardware, or is that mGBA's model?

**What the guards are worth, at the price they are actually paid.** In ROM, each
guard instruction was worth 4 cycles/codeword = 16,384 c/frame = 1.56 budget points;
in IWRAM each is worth ~1.4 cycles/codeword = 5,734 c/frame = **0.55 points**. So the
whole 6-instruction delta that 6a-5b measured at 9.4 points is worth **2.1 points
once the body is where it belongs**, and L3 (re-spelling G1 to drop its `[sp,#8]`
reload) is worth about 0.9 of those. That is hygiene, not a lever, and it should not
buy a rung before 6a-6. The width half is untouched: it is still 6a-6's, its FIXED
site is still one named function, and E9's marginal is still not admissible input —
but with decode at 48.29% the width leg no longer carries the full-rate claim.

**The budget half, kept as the 6a-5b form of it and now re-read.** It was worth
**9.37 points** of the 32,768 Hz frame budget (98,291 / 1,048,750) against the
projected landed stack's **3.84** (1,167,542 − 78,541 = 1,089,001 c/frame =
103.84%). 6a-5c then priced the overhang at 1.56 points per instruction: **109.77%**
with the dead one gone, **102.00%** if all six vanished (an upper bound no shipped
kernel can reach), and the two instructions that matter are the `[sp,#8]` reload and
whatever a cheaper G1 spelling costs. The paragraph below is unchanged as the
projection it was. Recovering **41%** of the measured overhang fits
32 kHz FIXED with no other lever, and the vend leg plus E9's i32 marginal is
1,069,252 − 78,541 = 990,711 c/frame = **94.47% mean / 95.0% worst**: the
archive's one in-budget row, re-derived from a landed-tree measurement of the arm
instead of a probe's. There is therefore a kernel-side candidate worth 2.4× the
deficit, and it is unproven until a 6a-5c leg measures it. What has *not* moved:
6a-6's own measured i32 marginal is still the only admissible width input, its
FIXED half still has exactly one named site, 6a-8 still does not run, and neither
product rate is in question.

The landed realization is +48.2 c/sample (+9.18%) slower than E10's vendored arm
(183,288,727 vs 167,877,039 on a 157-frame FIXED clip), and stacking E9's i32
marginal on it projects 103.83% mean / 104.36% worst of the 32,768 Hz budget — over
where the probe projected 94.47%. To fit, the stack needs **−40,152 c/frame =
−19.6 c/sample** more than the projection gives, and the seam's own overhang is
+48.2 c/sample, so *part* of it is enough. Candidate shares, all named in the
landing's own text and none measured: (a) the seam's window hand-out/take-back per
partition (`kernel_window` / `set_kernel_window`, plus the wrapper's class mapping
and the `order > MAX_ORDER` branch); (b) the two §9.2.7.3 guards the landing added
to E10's kernel (`mvn+lsr+str` per partition, `ldr+cmp+bhi` and `adds+beq` per
sample); (c) the 12-byte frame and the third status word; (d) monomorphization
codegen — 6a-3 measured that a second kernel body in a link moves the *shared* path
by ~168–199 K cycles/frame, so a per-partition call boundary is not free even
before its instructions.

Two constraints on any answer. The conditional **6a-8 does not run** — the plan
wrote it for a near-band fork result, and this result is far outside the band — and
the shape 6a-5b uses is deliberately *not* 6a-8's: it compares two realizations in
one link, pairing per frame, rather than pricing one kernel's sub-stages. And
**6a-6's measured i32 marginal, not E9's**, is the only admissible width input:
the i32 sites are outside the kernel and the seam, so transfer is expected, but
"expected" is how E9's caveat (1) got written in the first place. Honest answers
already established: both product rates fit FIXED with the kernel alone (61.7% at
18,157 Hz, 35.7% at 10,512 Hz), so this Q is about the *full-rate* claim, not about
whether the kernel was worth landing (it is: −286.9 c/sample, fork = carry).

**And one half of the width question is now identified by mechanism, not averaged.**
On this clip E9's FIXED `i32 − arm` marginal (−78,541 c/frame = −38.4 c/sample) has
exactly one active site — `stereo::decorrelate`'s mid/side arm, which is `i64` with
two `>> 1` per sample pair — because `integrate_fixed` is a no-op at order 0 and
`pad_block` a no-op at `wasted == 0`. So 6a-6's FIXED half is testable at a named
function rather than against a clip average; see the plan's 6a-5b section.

### Q13 — What is the 3.7–6.9% mGBA does not charge, and is it the decoder's problem at all?

**Opened by:** 6a-7b (2026-11-10) — the first time a decode number in this
repository was taken on silicon rather than in mGBA. Four runs, two units, all
identical to the cycle, and every leg of the 6a-7 image lands above the mGBA
reading: FIXED `ctrl` +6.64%, `wide` +5.38%, `asm` +6.89%; LPC +4.91%, +6.80%,
+3.75%. The stack (the kernel's win) is +6.56% / +6.54%, and the width marginal
moves in *opposite* directions per clip (−18.09% FIXED, +8.38% LPC). So 39.74%
became 42.48% mean / 42.72% worst, and E5's fetch arbitration — which matched
hardware to 0.05% — does not cover this.

**Why it is a question rather than a defect report.** Two of the three candidate
mechanisms are properties of the *model*, not of the code: mGBA's pricing of
cartridge-ROM **data** reads (the bit reader streams bytes from ROM, and E5 only
ever arbitrated instruction fetch), the **prefetch buffer's interaction** with
those reads (E5's stream had no data reads to disturb the prefetched line), and
the **flashcart** — every hardware number here runs ROM from PSRAM, and a
production mask ROM need not price a byte read the same way. The shape argues
against the two cheap explanations: the deltas are not a constant per-frame offset
(so not calibration: they run +28,737 to +446,980) and not a constant percentage
(so not a clock or timer error: 3.75%–6.89%).

**What would answer it.** E5's shape one layer down: a synthetic `ldr` stream read
from cartridge ROM and from IWRAM, timed in one image on both units, with the
prefetch buffer on and off, and in mGBA. That separates the data-read model from
the prefetch interaction in one run; the flashcart term needs a mask-ROM cart, or
has to be carried as a stated limit of every hardware number here.

**Why it is not blocking.** Decode still has 2.34× headroom at full rate (42.72%
worst), which is why the rate decision was pre-declared robust to a 20% penalty.
It becomes blocking only if a product path ever needs the last 6% — or if a
*scaling* claim is made from cycle counts, because the shortfall is per-leg and
would not cancel.

**Narrowed by 6b-1's hardware run (2026-11-10) — without a probe.** The product ROM
was measured on a GBA 001 and a GBA SP with both kernels: `rice-arm` came back
**+0.055%** over mGBA (396,492 → 396,709) and `TableCountKernel` **+4.03%**
(1,559,312 → 1,622,116). Same binary, same walk, same reader, same timer. So the
shortfall is not a clock, timer or calibration term and not "the model is 6% off
everywhere": it attaches to the leg whose time is dominated by **fetching
instructions from cartridge ROM** (the default kernel's Thumb codeword loop), while
the IWRAM-resident kernel is priced essentially exactly. What is left is the split
between mGBA's ROM-fetch model and the prefetch/data interaction, and whether a
mask-ROM cart differs from this flashcart — neither changes a decision today.

**Parked by decision, 2026-11-10.** The project owner set two acceptance criteria
(`plan-of-record.md` §"Acceptance criteria"): C1, `flac-lite` stands alone as a
`no_std` crate ready for its own repository; C2, `flac-lite-gba` decodes *and plays
back* in real time at 32 kHz on a GBA. This question cannot decide either — C2's
decode figure is already a hardware measurement and the acceptance run is on
hardware — so the probe is out of the queue and the critical path runs
6b-1 → 6b-2 → 6b-3 → item 7 instead. **The trigger to re-open is unchanged and now
written here rather than implied:** a product path that needs the last 6%, or any
claim derived by scaling an mGBA cycle count. Until then the working substitute is
the **+7% derating** rule on mGBA figures for memory-heavy paths
(`plan-of-record.md` §"Ordering rules").

---

### Q14 — Is the 2.4% pitch shift part of the product, or an accident of the rate contract?

**RULED 2026-11-11 by the project owner, and the ruling is BLOCKING for the product:**
*a stream is encoded at the rate it will be played at, and the player does not
reinterpret a declared rate to fit the clock.* "Is 41 cents acceptable" was the
question; the answer is no — in itself, not "not yet" and not "not unless the run is
clean". So the product path carries a true 32,768 Hz stream, and the
`32000 → 32768` mapping this question was opened about
(`scripts/gafp_pack.py::PLAYBACK_RATE_FOR_STREAM`, mirrored by
`geometry.rs::playback_rate_for_stream`) is **not a shipping path**: it is 6b-3's
measurement bridge, kept only because the play path had to be built and measured
before an asset at the right rate existed. It carries a bold `TODO(Q14, BLOCKING)` in
`examples/flac_integration/src/geometry.rs` and in the ROM's own module docs —
greppable as `TODO(Q14` — and its mechanical guard is
`ruled_rate_policy_allows_one_bridge_and_no_more` (`make product-play-test`), which
computes the ratio instead of quoting it (**1.024× = 41.06 cents**) and refuses any
*further* rate reinterpretation. Consequence, stated where it costs something: the
asset this repository currently ships, `examples/flac_integration/assets/spike_l0.gfp`,
declares 32,000 Hz and is therefore **not a product asset**. 6b-3's record, `FLAC.md`
and C2 all say so now.

**Opened by:** 6b-3 (2026-11-11), and by the packer before it.
`scripts/gafp_pack.py` declares `PLAYBACK_RATE_FOR_STREAM = {32000: 32768}` and
`examples/flac_integration/src/geometry.rs::playback_rate_for_stream` implements the
same mapping, so the shipped asset — a **32,000 Hz** FLAC stream — is consumed by a
**32,768 Hz** clock. The first product-play run says it out loud on its own serial
line: `rate: stream 32000 Hz -> playback 32768 Hz … the stream's own nominal period
would be 1073741`. Nothing in these docs contains a decision that this is *allowed*.

**What it costs, both halves measured rather than estimated.** Playback runs
**1.024× fast** — about **41 cents** sharp, comfortably audible to anyone comparing
the GBA against the file they encoded, and enough to fail a tuning-fork check. And the
producer's real-time budget is **2.4% tighter** than the stream asks for: the frame's
period is 1,048,576 cycles at the clock the player actually runs, where the stream's
own period would be 1,073,741. So the mismatch is not free in either direction; it
buys the only rate the hardware speaks and it charges pitch plus a tighter deadline.
**Measured since, and the second half is smaller than it looked:** the tighter deadline
prices nothing at current cost levels. The frame deadline is 1,048,576 cycles either
way — no instrument here budgets from the declared rate — the worst frame's work is
59.4% of it either way, and a run against a stream that *declares* 32,768 has an
`audio_span` digit-for-digit identical to the shipped one. It would bite only if
work/frame approached the period. The pitch half stands unchanged.

**Why it is a question rather than a defect.** Some resolution is forced: `agb`'s
`Frequency` enum offers 32,768 Hz (and the other divisor rates), not 32,000, and the
GBA's FIFO rate is a timer divider. The options, with what is already known about
each:

- **Encode at 32,768 Hz so the stream *is* the playback rate.** This is the bullet
  that used to read "the encoder will simply do it". It is now measured (2026-11-11,
  `measurements.md`, the 6b-3 rate row), and it survives with two corrections and one
  reversal. What is free, measured: 32,768 is representable — the 4-bit rate table has
  no entry for it, so the stream spells the rate as `0b0000` plus a stream default,
  which changes **no frame length** (the shipped blob re-spelled that way is the same
  865,996 bytes), or as `0b1101` plus a 16-bit Hz field, which is what `flac` itself
  emits when simply asked and which costs 2 B/frame. `flac-lite` plays such a stream to
  a **bit-identical PCM**, with the same `571/571` period audit, the same
  `min_lead 9120`, zero underflows, and cycle counts that do not separate from the
  shipped asset's at the instrument's floor. The corrections: "zero runtime cost" is
  honest written as ±0.2% system / 0.005% decode-only, and "the packer's table changes"
  understates what it takes — `gafp_pack` refuses the re-spelled asset for want of a row
  keyed `(32768, 2048, fixed, 0)`, and every row it carries is a **hardware** figure, so
  filling that key is a 7-1 hardware read rather than a transcription. The reversal: this
  option fixes the pitch no better and no worse than relabelling does — measured, a
  re-spelled stream changes no sample at all. The 41 cents lives in the resample step,
  so the pipeline work is resample-at-32,768 → encode at the product profile → measure
  the new key; and for a given playing time such a stream carries 2.4% more samples,
  hence ≈2.4% more blob bytes (arithmetic, not measurement). What it forfeits is still
  nothing *measured* — which is why it stays first, and why the honest status now is
  that the runtime half of this question is closed and the product half is not.
  **Ruled, 2026-11-11: this option is the product path.** What remains of it is work
  (item 7-0), not decision.
- **A nearest-programmable clock at 32,000.** Arithmetic from `agb`'s own formula
  (`hw.rs`: `set_overflow_amount((16777216 / frequency) as u16)`, where the overflow
  amount *is* the period in 2^24-scaled ticks): 2^24 / 32,000 = **524.288**, so the
  nearest is 524 → **32,017.6 Hz**, 0.055% sharp ≈ **0.95 cents** — a thousand times
  closer than the present 41. The cost is that the geometry's exact arithmetic dies:
  512 cycles/sample and the compile-time pins in `geometry.rs` were chosen because the
  number was an integer. Not run, not measured; it is arithmetic, and it belongs to
  7-2's "is this the shipping path" decision rather than to this row. The measurement
  above changes this option's ranking, not its arithmetic: an asset encoded at the
  playback rate buys 0 cents with the integer arithmetic intact, so this one is now
  cheaper only in the narrow sense that it re-uses the asset that exists.
  **Ruled out 2026-11-11 by the same ruling:** it is a smaller reinterpretation, not
  an end to one — 0.95 cents is still a stream played at a rate it does not declare —
  and it spends the exact integer arithmetic the geometry's pins are built on.
- **Resample in the player.** A new per-sample cost on the hot path, unmeasured, at a
  point where the producer's measured work is 53.062% of a frame — payable in
  principle, and the price is unknown. Not forbidden by the ruling in the way the
  mapping above is (rate conversion changes the rate rather than reinterpreting it),
  but not chosen either: the plan takes the encode-side route, because the place to
  pay for a rate change is where it is free — at a desk, offline — and because this
  option would put unmeasured work into the one path this project has measured as
  tight. It stays unmeasured, and it is not the plan.

**What is left is not a decision, it is an item.** The decision half closed on
2026-11-11, so the remaining work is recorded as **7-0** in `plan-of-record.md`:
resample the source to 32,768 Hz, encode it at the product profile (FIXED-0, 2048-sample
frames, the residual shapes the guard admits), and land a **pitch witness** in the same
PR — a tone fixture, because nothing in the existing gate set can see a pitch shift: the
pinned PCM digest is byte-identical for the correct-rate asset and the reinterpreted
one, which is precisely how a 41-cent error walked through every gate this asset has
been through. 7-1 owns the hardware row at `(32768, 2048, fixed, 0)` that
`gafp_pack`'s guard keys its product path on; that row is a silicon read, and the
mGBA pair in this question's rows cannot fill it. The ordering is forced by the guard:
the packer will build a 32,768 blob only under `--allow-unmeasured` with its
`this is NOT a product blob` note attached, and the way that note comes off is 7-1's
measurement, not anything obtainable at a desk.

**Who owned it, and who owns the work now.** Until 2026-11-11 this was 7-2's call to
make alongside the shipping-path decision. The ruling removes the call; what is left
belongs to 7-0 (the 32,768 encode and the pitch witness) and 7-1 (the hardware row at
the new key), and 7-2 inherits a real asset rather than an open question. Half the
witness was already in before the ruling: representability, both rate spellings and
their byte cost, the decoder's behaviour, bit-exactness against the pinned PCM and the
cost (`measurements.md`, the 6b-3 rate row) — which is what makes the ruling affordable
to accept: it costs pipeline work, not a redesign.
