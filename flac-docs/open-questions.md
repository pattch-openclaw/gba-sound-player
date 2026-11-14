# Open questions for a future session

> **Scope: questions the FLAC track owes, and only those.** Each Q below is a FLAC-track
> question, owned by a rung in [`plan-of-record.md`](plan-of-record.md) and priced in
> [`measurements.md`](measurements.md). Nothing here is a repository-wide open question and
> nothing here decides another track's plan ([`../ULI2.md`](../ULI2.md),
> [`../ULC.md`](../ULC.md), [`../OPUS.md`](../OPUS.md) are their own entry points). "Product"
> and "product path" below mean the **FLAC** product path.

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

### Q12 — Where do the seam's 48 c/sample go, and what puts FIXED back under budget?

> **Re-framed by the rate ruling of 2026-11-13.** This question was written while 32,768 Hz was
> the commitment, so "back under budget" meant "under a 1,048,576-cycle frame". C2 now asks for
> the highest rate achievable (Q16 owns the bar), and the cycle costs this question is about are
> sample-denominated — they do not move with the clock, only the budget does. So Q12's two halves
> survive the ruling differently: **the attribution half (where the 48 c/sample go) is
> rate-independent and still owed**, and it is now the most valuable thing in the file, because
> 7-1's second reading put the product's attributed decode at **451,763 c/frame** against 6b-2's
> kernel-in-a-slot **396,066 c/frame** — a ~55,700-cycle difference, 12.4% of the decode bill,
> that lives somewhere between the kernel and a running producer and has never been attributed.
> The "is 32,768 Hz reachable" framing is closed: it was reached, at 59.946% of the frame, and the
> owner declined it.

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
(`plan-of-record.md` §"Ordering rules"). **Re-opened on 2026-11-13 by 7-1's first hardware
reading, on the product path and with a bigger gap than the one that parked this — the
numbers and the hypothesis are in Q15; the two questions are the same question at different
points on the map.**

---

### Q14 — Is the 2.4% pitch shift part of the product, or an accident of the rate contract?

*(RULED 2026-11-11. **IMPLEMENTED 2026-11-13** by plan item 7-0 — record at the end of item
7 in [`plan-of-record.md`](plan-of-record.md), numbers in the 7-0 row of
[`measurements.md`](measurements.md). The question stays answered as it was ruled; what
changed is that the answer is now in the product path rather than in front of it.)*

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
*further* rate reinterpretation.

**Implemented 2026-11-13, and each piece is named by what enforces it.** The product asset
is authored, encoded and declared at 32,768 Hz (160 frames, 326,144 samples, its last frame a
512-sample short tail by construction); `geometry.rs`'s `RATE_BRIDGE` is `None` and
`playback_rate_for_stream` is identity-or-refusal, with the exhaustive sweep over the
declarable set (`0..=65535` plus the table rates) run as a witness rather than described;
`PLAYBACK_RATE_FOR_STREAM` is gone from the packer, which refuses a foreign-declaring stream
*before* encoding and re-checks the rate at every frame, because 6b-3's per-frame rate
witness proved frames disagree with their own stream; `MEASURED_DECODE` is keyed by shape with
the frame budget named once in the arithmetic; and `crates/flac-lite`'s `EncodeProfile` now
carries `allowed_sample_rates_hz`, one element, enforced by the library facade and by the
product ROM against its own shipped blob. The two `TODO(Q14, BLOCKING)` markers described
above are gone from the code; the guard is now
`ruled_rate_policy_allows_no_reinterpretation_of_a_declared_rate`. Three details of what
landed differ from what was planned, each recorded in the 7-0 entry: the cost key is blocksize
+ predictor kind/order with the residual envelope carried as a stated caveat rather than as a
key component; the predicted `--allow-unmeasured` state did **not** happen — the census did
not change, so the asset packs cleanly against the inherited hardware row, and what the packer
gives instead is a provenance warning, which a gate reading only exit status cannot see; and
the planned committed negative leg — a tone authored at 32,000 and packed as a 32,768 asset —
cannot exist as a committed asset because the packer now refuses to make one, so that witness
exists as a re-synthesised 32,000-era design, a byte-level respell of the shipped blob (same
samples, refused by the profile), and a mutation left running through the whole generation
chain, where the pitch instrument answers 32,000 and the suite fails. **Nothing here reopens
the ruling.** What is still owed to it is 7-1's measured row on the frames 7-0 produced, which
is what clears the provenance warning: a row keyed by shape is a bridge too, and the rule
against borrowing one across eras is still in force.

**Two rate sets, not one — and the sweep is over the declarable one.** `agb` 0.25's
mixer can play **10,512 / 18,157 / 32,768 Hz**, and the product geometry is 32,768; a FLAC
*stream* can declare any of the format's 4-bit table rates or a 16-bit Hz value. The Q14
rule is about the declared stream rate, so the exhaustive guard is a sweep over the
declarable set (`0..=65535` plus the table values, 65,536 cheap host calls), not over agb's
three clocks. 6b-3 keeps its sampled refusal list as the interim guard; the behaviour
change — identity-or-refusal in the packer and the player — is 7-0's, so the exhaustive
loop is described here rather than added. Consequence, stated where it costs something: the
asset this repository currently ships, `examples/flac_integration/assets/spike_l0.gfp`,
declares 32,000 Hz and is therefore **not a product asset**. 6b-3's record, `FLAC.md`
and C2 all say so now. *(As of 7-0 it declares 32,768 Hz and is a product asset; those
sentences stand as the state until it did.)*

**And the root cause is named too, because a ruling without it would leave the shape that
produced the bridge intact.** The mistake was never the *value* 32,000 — it is one of FLAC's
table rates, and 6a-7b's rows were honestly measured on an encode at it. The mistake was
making that rate the fixture and measurement target while the plan's only clock is 32,768,
and letting `gafp_pack.py` key `MEASURED_DECODE` by the **stream** rate while
`frame_budget_cycles()` takes the frame budget from the **playback** rate: keying cost on a
variable the cost does not depend on (the rate probe measured it at −20 c/frame, the
instrument's own floor) is exactly what forced the fixture to 32,000, the rows to
32,000-keys, and the bridge into being load-bearing. So 7-0's scope is structural, and says
so now: cost re-keyed onto blocksize, predictor kind/order and residual envelope *(the landed
key is the first two; the envelope travels as a stated caveat on the row — see the 7-0 entry)*;
the sample
rate demoted from a lookup dimension to a *validity check*; the missing rate clause added to
`crates/flac-lite`'s `EncodeProfile`, which guards blocksize, FIXED order, LPC and bit depth
and has no rate field at all *(it had none: 7-0 added `allowed_sample_rates_hz`)*; identity-or-refusal in both the packer and the player; and a pitch witness, because a digest cannot see this defect — the samples are the same bytes. The
full inventory of where 32,000 lives and which uses are legitimate, plus the two retarget
traps (sample-period fixtures preserving the pitch error; 128/125 being exact but ffmpeg's
`-ar` being float), are in `plan-of-record.md`'s "Q14 follow-up" section, summarised in
`FLAC.md`. What all of it protects is the failure mode this project must never ship: audio
arriving pitch-shifted away from what was encoded.

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
59.4% of it in both arms of that probe (the pre-median clock's reading; the current
image prints 53.964%), and a run against a stream that *declares* 32,768 has an
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
  point where the producer's measured work is 53.390% of a frame (53.062% on the pre-guard instrument) — payable in
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

---

### Q15 — Is the emulator-versus-hardware gap a property of how these images reach a unit, and would the answer change the product's rate?

**Opened by:** 7-1's first hardware reading (2026-11-13) — four runs of one image
(`8113f04c…84cd`), GBA 001 ×2 and GBA SP ×2, against an mGBA run of the **same image**.
Every leg came out high on silicon, and one leg far more than the others:
**convert +35.1%**, decode window +6.85%, push +1.09%, feeder +2.35%, **work +12.08%**
(+67,727 cycles a frame). The legs sum to the total on both sides, so it is not a clock
term. Q13's trigger is met — this *is* a product path, and the gap is now 12.08% of the
whole rather than 3.7–6.9% of a leg.

**The hypothesis, as supplied by the person running the units, recorded as stated.** These
images reach the units from a **flash cart backed by an SD card**, not from a mask ROM, and
cartridges are memory-bandwidth-hungry, so the cart probably under-performs a real ROM. If
that is the mechanism, one way to design around it is **to keep more of the pipeline in
IWRAM — "the way the FIFO does, but bigger"** — and the simplest way to finish the project
is a lower sample rate instead. The observed shape is at least consistent with it: the one
leg with no company in the comparison is convert, and convert is the leg whose writes go to
the 40,320 B ring in **EWRAM** — a placement that is *default*, not chosen, because there is
no custom linker script in this repository.

**What this tree already constrains, all of it in the same direction:**
- **E5** priced instruction fetch (SROM/BIOS/ROM/SDRAM and the NE72/NE73/NI72 states) on real
  units, and mGBA reproduces every per-call figure to **0.012%** with `WAITCNT` identical at
  `0x4317`. So "the CPU/bus model is wrong about *fetch*" is out.
- **6b-1** is the sharpest fact available for the hypothesis: in one binary on two units, the
  **IWRAM-resident** ARM kernel came back **+0.055%** over mGBA and the ROM-resident Thumb
  kernel **+4.03%**. *Residence changes the gap* — which is exactly the lever an IWRAM buffer
  design pulls.
- **E3** measured the **3.482 c/B** penalty for writing decoded samples to EWRAM **on real
  GBA hardware**, and it was worth ~43 points of work/frame. So an IWRAM-resident ring is a
  measured prize, not a modelling hope — and it is the reason the 6b-1 record says the
  *placement of the ring*, not the decoder, is the standing target on this path.
- **The geometry says what fits.** The shipped ring is 20,160 samples per channel × 2 =
  **40,320 B** against **32,768 B** of IWRAM: at this rate an IWRAM ring is not expensive,
  it is arithmetically excluded. At half the rate the same 0.615 s of lead is 20,160 B.
  That is the arithmetic that makes the two halves of the owner's judgment fit together —
  **the lower rate is what makes the IWRAM design affordable.**

**And one thing the hypothesis does not explain, which is why it is still a question.**
push (+1.09%) and feeder (+2.35%) are essentially the same in both environments, and those
legs touch the same ring; meanwhile the decode-window delta sits *inside* the band 6a-7b
already measured for legs whose time is fetching instructions from cartridge ROM, whereas the
same kernel shape measured +0.055% when IWRAM-resident in 6b-1. Two explanations are still
open and they are not distinguishable with the numbers in hand: a real memory penalty, or
**window attribution** — the screen's `dec` is a window mean taken inside a running player
with timer IRQs and a live DMA on the bus, and 6b-3 already refused to reconcile exactly
this pair of numbers once (436,756 measured-in-a-player versus 422,248 attributed for the
same asset). The instrument step that separates them on a unit is 7-1's next step anyway:
put the image's own `attribution:` line on the screen.

**What would answer it, cheapest first.**
1. **Built, controlled, and answered on the units 2026-11-13** (image `86f6a1dd7e525dfc…`,
   +87 c/frame in mGBA / +262 c/frame on silicon, fit gate green). **The answer:
   `late in push 1/1`, four runs, both units** — period 22 belongs to the ring push, and this
   question's bus leg is not the explanation for it. What follows is the record of how the field
   was built, kept because the register witness caught an emulator-side artefact that would
   otherwise have been read as hardware evidence. Three witnesses went onto the screen, and one of them corrected a guess made
   here: the `attribution:` figure now prints as `attr`; the enable-mask/IME/**DISPSTAT** set
   is printed at arm time and again at report time; and late services are attributed to the leg
   they landed in — by the period counter the ring-push section publishes as its first act,
   which is exact rather than windowed, since a ~29-sample section on a 560-sample grid crosses
   at most one boundary. **What was guessed wrong:** `DISPSTAT` bit 0 was said to be what
   decides whether a VBlank IRQ exists, but agb's `Interrupt::VBlank::enable()` writes **bit 3**
   (`DISPLAY_STATUS.set_bits(1, 1, 3)`), and the mask at 0x0400_0200 is **agb's own** with
   agb's bit numbering (`VBlank = 0`, `Timer1 = 4`), not GBA's IE map. Hence the raw registers
   are printed and not interpreted, and the behavioural witness — the mask as the audio handler
   itself sees it, with the co-entry count and its complement — is what carries any claim. What
   is owed now is the **reading, not the instrument**: the same image twice per unit with
   `late in push a/b` transcribed. That field either names the push section as the cause of the
   period-22 miss or kills it for that event — and if it kills it, item 2 below becomes the
   next experiment.
2. **A single-variable ring-placement control that does fit at this rate.** The host model's
   depth cliff is **20 chunks**, and 20×560×2 = **22,400 B**, which does fit in IWRAM
   alongside the ~2 KB the image already carries there. So: one image with the ring in `.bss`
   (EWRAM) at 20 chunks, one with the ring in `.iwram` at 20 chunks, everything else held —
   and the convert leg either collapses toward the mGBA figure or it does not. This test has
   never been run here, and it is the one that would put a number on the flashcart/EWRAM
   hypothesis instead of a name on it.
3. **The transport question, which has never been answered in this repository.** Which cart,
   whether its wait states are configurable, and whether a different cart (or a real mask
   ROM) is obtainable. Q13's own words still stand: absent that, the flashcart term has to
   be *carried as a stated limit of every hardware number here* — and now of the product
   path's stage shares too.

**What this question does not decide.** Not C2's rate — that was the project owner's to rule and
**was ruled twice on 2026-11-13**: C2 now asks for the highest rate achievable rather than a
committed 32 kHz, and the rung chosen is **16,384 Hz**. The bar that makes "achievable" checkable
is **Q16**, not this question.

> **And one thing this question's ruling explicitly did NOT relax, recorded because it nearly
> was.** The owner's first instruction on retargeting was to **drop rate exactness** and take
> agb's `Frequency::Hz18157` (~18 kHz), on the understanding that it was "the already supported
> sample rate". Checking it turned up two facts that voided the premise, and the instruction was
> withdrawn the same day in favour of 16,384 Hz:
>
> 1. **Nothing on this path uses agb's mixer.** `product_play_main.rs:1795` states it as a safety
>    precondition — *"no `agb::sound::mixer::Mixer` is instantiated in this ROM, so timer0-3 are
>    unowned"* — and the only `gba.mixer.mixer(…)` call sites in the tree are the baseline
>    `examples/pcm_playback.rs` demo and the *failed* `symphonia_flac_probe` compile probe. The
>    product ROM programs timer0, the timer1 cascade, the sound-DMA enable and every per-period
>    re-arm itself; agb's `enable_dma_for_sound` does not even write a transfer count. So agb's
>    three-rate menu is a **self-imposed domain** here (via Sam's 2026-10-18 route choice, and via
>    `CHUNK_SAMPLES = 560` being agb's `Hz32768` buffer size), not a dependency.
> 2. **~18 kHz is worse, not merely inexact.** A 924-tick reload plays **18,157.16 Hz** (+8.82 ppm,
>    0.015 cents), and because the re-arm grace is **sample**-denominated while the confirmed
>    ring-push mask is **cycle**-denominated, the faster clock eats more of the grace: the mask is
>    **15.89 samples on the mean and 17.13 on the worst window ever recorded, against a 16-sample
>    grace — still late** — where 16,384 Hz gives 14.34 / 15.46. So it would have traded Q14's
>    machine-checkable equality *and* 3 points of headroom for a defect that is still refusing.
>
> **Q14's identity rule therefore stands exactly as ruled: the stream declares the rate it is
> played at, and at 16,384 Hz those two numbers are equal integers, so `playback_rate_for_stream`
> stays an equality and no tolerance is introduced anywhere.** If a future session is tempted to
> re-derive "we must relax exactness to use agb's rates", the answer is the first bullet. The one
> place where exactness *is* genuinely re-tested is 7-2, if agb's mixer is ever adopted as the
> shipping path — at which point its menu (10,512 / 18,157 / 32,768) and a 16,384 Hz product clock
> are in direct tension and that tension is 7-2's to resolve out loud. **And it no longer explains period 22 either** — that question was
answered on the units on 2026-11-13, and the answer is not this: 7-1's second reading came back
**`late in push 1/1`** on four runs across both units, so the one late service was held by the
producer's own interrupt-masked ring push, which is a code property rather than a bus property.
Two earlier findings pointed the same way before a cart was filled: a *second source in the same
interrupt entry* would have to cost ≥ 11,776 c, about five times this ROM's entire measured
audio handler (2,286 c mean / 2,999 c worst), in a build that registers exactly one handler; and
the count of period boundaries falling inside the masked push windows is **0–2, not the 8.1 a
random-phase producer would give**, because `wait_for_room` phase-locks the producer to the
period grid — which is also why that count moved between two builds differing only in log
strings, and why a number like it has to be read from a unit rather than modelled.

**So what this question owns now is narrower, and one notch more urgent.** It owns the
**convert leg's +35.1%**, which no attribution field explains, and which the ruling on the rate
does not make disappear: at 16,384 Hz that unexplained gap is ~7% of the frame instead of ~14%,
but it is still an unpriced leg in the budget of whatever rate ships. And the second reading
handed it a new datum in the *emulator's* direction: `2nd src 0/582` with the display's VBlank
request bit never set on a unit, where mGBA reported that bit **set** and one shared entry. The
emulator generates an interrupt the real hardware does not, so interrupt-topology claims now need
silicon in both directions — an mGBA control run cannot settle them by presence *or* absence. The
placement control (item 2 above) is still the probe that would put a number on the
flashcart/EWRAM hypothesis instead of a name on it, and it is the cheapest remaining piece of
this question.

---

### Q16 — What makes a rate "the highest achievable"? C2's new wording needs a bar before it can be judged

**Opened by:** the owner's ruling of 2026-11-13, which amended C2 from "in real time, at
32 kHz" to "at the highest rate the hardware achieves" (full text and arithmetic in
[`plan-of-record.md`](plan-of-record.md) under "The rate ruling").

**Why it is a question and not a formality.** "Highest achievable" is a *result*, so it has to
be produced by a measurement with a stopping rule. Without one, the clause is unfalsifiable in
both directions: any rate that happens to pass can be called the highest achievable, and any
rate that fails can be called unachievable. C2 used to be checkable precisely because it named
a number; the amendment trades that for a search, and the search needs the bar that makes its
answer the answer.

**The shape the bar has to have.** The ladder is fixed by the clock, not by taste: the rates
this architecture expresses *exactly* are 65,536 / 32,768 / **16,384** / 8,192 Hz, and Q14's
identity rule survives the change, so a rate below the current one is only a candidate if the
stream *declares* it (explicit-rate codes — FLAC's four-bit table has 16,000 and 32,000, and
`16,384/16,000 = 1.024` re-imports Q14's 41.06-cent defect exactly). That leaves at most four
rungs to test, and two of them are already answered from rows in this ledger: **65,536 Hz is
out on arithmetic** (twice the current 59.946%, no instrument needed), and **32,768 Hz is the
rung just refused** — not by the late service, which is now explained and fixable, but by
59.946% of the frame going to a FIXED-0 stream that carries no prediction at all.

**The three things the bar has to say, and what each is for.**

1. **A headroom floor, in cycles, on the worst frame** — not the mean. The product-path numbers
   are close to their own means (`work 59.946%` against `maxwk 60.755%`), so a mean-only bar
   would be nearly free; the reason to price the worst frame is that the shipped clip is 160
   frames of one asset, and a real library will not be.
2. **What the rate must be able to carry, not just what it did carry.** This is the substantive
   choice, and it is the owner's: a bar set against the shipped FIXED-0 clip is met at
   16,384 Hz with the work figure around **30%** (from the second reading's 59.946%, halved,
   cycles unchanged) — comfortable, and arguably not worth calling a result. The same bar set
   against *predicted* frames is met nowhere above **8,192 Hz**, because LPC decodes at 173.23%
   mean / 174.14% worst of a 32,768 Hz frame on hardware (6a-7b), which is 86.6% / 87.1% at
   16,384 Hz and **43.3% / 43.5% at 8,192 Hz**. So "highest achievable" means 16,384 Hz if the
   product is a FIXED-0 player and 8,192 Hz if it is a music player. 7-0's own census said real
   material censuses FIXED 1-4 and LPC, and this question is where that observation gets
   either paid for or officially dropped.
3. **What the headroom is *for*.** One candidate use is already in the ledger and already
   overdue: the audit's 16-sample grace is currently oversubscribed by design — the producer's
   masked push window is 28.7 samples at 32,768 Hz, confirmed as the cause of period 22 by
   `late in push 1/1`. Halving the rate brings that window to 14.3 samples against the same
   16-sample grace, which is a 1.7-sample mean margin and 0.5 on the worst window ever recorded
   for that section. **A headroom bar that ignores this buys a rate change and still leaves the
   product passing its own latency audit by less than one sample of audio.**

**Status update, same day: the target is named, so the bar's job has narrowed.** The owner chose
**16,384 Hz** as the rate of record ("The rung is chosen" in `plan-of-record.md`), which removes
the "which rung" half of this question and leaves the "is it the ceiling" half. What Q16 still
owns: whether 16,384 Hz is accepted as achieved (work ~30% of the frame projected, worst frame
~30.4%) or whether the bar's real purpose is the **LPC** question it exposes — at 16,384 Hz LPC
still projects to **86.6% / 87.1%** of the frame, so a product that must carry predicted frames
lands on **8,192 Hz** (43.3% / 43.5%), and no FIXED-0-only figure can decide that for it. Until
that is ruled, 16,384 Hz is a **candidate of record**, and C2's rate claim is not met at any rate
because C2 is met at 7-3.

**What would answer it, in the owner's hands.** One of:

- a stated rule — e.g. "the highest exactly-expressible rate at which the full gate set passes
  on both units with the worst frame at or below X% of the budget, where X is chosen to leave
  room for the shapes we intend to ship"; or
- a narrowing of C2's wording to the product the tree actually means (FIXED-0 only, or music),
  which settles the ladder by itself; or
- a decision to keep the search but bind it to a named fixture — e.g. "the highest rate at which
  the shipped clip passes with a 2x worst-frame margin, re-measured per asset."

Any of those makes the amended C2 checkable. What must not happen is the amended C2 being
treated as satisfied by whatever rate is convenient at the moment someone wants a claim: the
16-sample clause, the zero-underrun clause, Q14's identity rule and the listening witness all
carried across the amendment unchanged, and they are the parts that make the rate mean anything.

**What this question does not decide.** Not 7-2's push-mask fix, which is owed at whatever rate
ships and is now a decided thing rather than an open one. Not Q15 either: an unpriced +35.1% on
the convert leg is a leg missing from any rate's budget, and at 16,384 Hz that leg is roughly
7% of the frame — so the same unexplained gap that was survivable-at-40%-headroom at one rung is
a bigger share of the margin at the next one, but it is not the thing setting the bar.

**Status 2026-11-13: closed for C2.** The owner chose 16,384 Hz and accepted the real-audio
master path after the four-run hardware reading (audible, BLUE, zero underruns, zero late
services). The acceptance model is now: validate the code path once end to end; judge subsequent
encodes with build-time tests and the mGBA control. A derated-margin bar and a stress corpus are
post-C2 hardening, not an open acceptance criterion.
