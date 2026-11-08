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
answerability, updated 2026-11-08): item 6a is up to **6a-4**, which is on the
branch `feat/flac-lite-gba-arm-kernel` (the asm kernel, the FFI/ABI, the
`rice-arm` and `iwram-pin` features, `examples/overlay-kernel-check/` with its 8
device `#[test_case]`s, `overlay-check-arm`/`-test-arm`/`-objdump`/`-kernel-rom`/
`-test`, and the 6a-4 landing record — not yet merged). 6a-3 is merged (PR #98,
`8711e2f`); 6a-2, 6a-1 and the toolchain pin are merged (#95–#97). Nothing in any
shipped decode path has changed since item 5 PR 4:
`.text` 46,008 B `f0f88c33d150…`, `rice_quotient` live, `decode_residual` 596 B,
`flac-gates` 1,169 passed / 6 ignored. The live `nm` anchor is
`LEADING_ONES @ 0x0300022c` — but only in an image with **no** second mechanism, which
is every image this record's default/control arms measure. Once a second `RiceKernel`
body is reachable the table *relocates* to `0x0300042c` with its bytes unchanged (see
the 6a-3 section table, arms T/H); so no reader should treat `0x0300022c` as a
scheme-wide constant, and no kernel-check may hard-code it.

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

**Opened by:** 6a-3's codegen/cycle table. **Owned by:** 6a-5 (and 6a-7 after it).

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

Q5a is a confirmation on the current tree of a pairing E9 measured as additive, and
it is the configuration the product ships; its second job is re-anchoring the E
numbers on the post-6a image, which 6b-1 inherits. Q5b is genuinely open: neither
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
measured — through the manifest instead of a hand-computed index, plus the seek
witness via `SEEKTABLE` + `Frames::anchored(n)` (which 4b left as an explicit seam,
not an omission). Ordering items that never ran: the rate-sweep re-run on the
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
gate-aggregation TODO is half landed — `make flac-gates` exists and runs six
configurations plus the combination arms — but its invocation hook and a
`podman-flac-gates` sibling are still open. And `flac-portable-check` is a *compile*
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
