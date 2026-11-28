# ALBUM-UI.md — the interaction spec: panes, focus, items, and who reads what

Status: **ruled 2026-11-27 from the caller's stated preference**, and written down because the plan of
record had no document for it. [`ALBUM-PLAYER.md`](ALBUM-PLAYER.md) stays the authority for
**geometry and budgets** (§5.0's layout spec, §5.1's pools, §5.5's faces, §9.0's refusals) and for the
**phases and gates** (§11, §13). This file is the authority for **behaviour**: which pane has the
focus, what each button means, what a scene owes the shell, and what has to be declared in
`layout_spec.toml` before any of it can be audited. Where the two disagree, the disagreement is
listed in §10 with the passage it supersedes — the plan is amended by reference, not silently.

It is **not** an implementation plan. §9 sequences the work onto the existing 2b ladder; nothing here
has landed. The code today is described in §1.4 so the delta is legible.

**Review pass, 2026-11-28 — feasibility against the code, not a re-litigation of the model.** R1–R9,
the map and the phase ladder stand; what follows are the places the first draft was wrong about the
*code* it describes. **Fixed:** G1's recipe ("re-order the slots") was written as a swap of `right`
and `left`, and **G1's own rule refuses a swap** — a transposition reverses two adjacent items, so the
drawn row would read `play | prev | next` against a ring that reads `prev → play → next`; the fix is a
re-layout of three items into columns 21/23/25, which leaves 22/24/26–29 free (§4.1). **Added:** the
`U4`+`G2` pool arithmetic now carries the indicator term the first draft omitted — net **9** chrome
tiles / **462** distinct against today's 10 / 463, so option A is **pool-neutral**, and G6 is a
declaration gap rather than a budget gap (§4.1). **Specified:** the **delivery** half of the input
layer, which the first draft omitted entirely — the ISR posts one mapped `Action` today and **no repeat
ever reaches the application**, so "hold to walk the list" has no path at all, and a direction held
across the boot boundary keeps repeating (§2.1, G13/G14). **Re-priced:** U14's paused half is a **new
transport path**, not a one-line conditional — and U16 makes it a general one (§4.5). **Corrected:**
U9b's "the card's prompt restored" — the prompt has **never** been drawn and the card's four spans are
title/artist/year/counts (§7.2, G15, U15). **Stated honestly:** §4.4 names *this exact rule* as the one
it replaced with a dedicated restart button; the reconciliation is about *which* ledger the rule needs,
not about press timing, and C15 now says so (§10). New gaps **G13–G17** are the residue.

**Ruled by the caller, 2026-11-28 — the open questions this review raised are closed:**
**U14 = defer** the paused target to the resume. **U15 = the boot screen is its own scene**, `boot`,
`cycle = false` — not a conditional fifth span on the card (§6, §7.2). **U16 = track switching works
while paused**: `next`, `previous` and `A` on a list row all re-target and leave the transport
`Paused` at the new track's `0:00`, deferring the audio to the resume — one mechanism with U14 (§4.5).
**U17 = three icons and nothing else**: no `A` marker, no `L`/`R`/`B` affordance, because a thing that
cannot hold the focus cannot be activated; salience comes from a **spaced** ring plus a shape-bearing
caret (§4.1, §4.3). **U18 = no throwaway scenes in a product ROM** — a `product = false` variant or a
one-off gate ROM may carry one, and the schema's `product` field already says so. **Ring order
confirmed** `previous → play_pause → next`, drawn left to right with a gap (§4.1).

---

## 0. The preference, in the caller's words

> "I think the Up/Down/Left/Right buttons should be used for UI *navigation* and not pre-programmed
> sequences. The start button maybe makes sense as it is for a reserved play/pause as that's natural
> for a GBA game, but the select button iterating through scenes doesn't make much sense to me.
> Instead, I think scenes could be navigated between using the L/R shoulder buttons. R goes to the
> next scene, L goes to the previous scene. The A button would be used to trigger whatever effect is
> currently selected in the UI. … Importantly, the Chrome and any other UI menu should wrap … If the
> current scene has no menu UI items, the 'focus' should stay in the Chrome, otherwise, the 'focus'
> should transfer to the scene. … Scenes are responsible for handling inputs, and the Chrome handles
> its inputs separately. Both Scenes and the Chrome are stateful. Transitioning in and out of scenes
> should not destroy state, at a minimum for the Chrome."

Everything below is that model made precise enough to build, audit and refuse. Where the caller's
journey and the general rule disagreed, §11 lists the question instead of picking silently.

---

## 1. The model

### 1.1 Two panes, one focus

The screen has exactly **two input panes**:

| pane | what it is | what it may draw |
|---|---|---|
| **Chrome** | the always-shown bands (§5.0 `BANDS`), i.e. the transport items, the clock and the title line | its own band cells, via `Surface`'s chrome/foreground layers |
| **Scene** | the region rectangle (§5.0 `REGION`), one scene at a time | only inside its own rectangle (§5.2's backing rule) |

Exactly **one pane holds the focus** at any instant, and within it exactly **one item**. "Focus" is a
pane+index pair, not a tree: there is no nesting, no stack of menus, and no modal dialog. That is the
line that keeps §16's "no GUI toolkit" intact (§10, C1).

### 1.2 What an item is

- **A chrome item** is one of the transport actions the chrome draws, declared in the layout spec with
  an **identity**, the **verb** its activation performs, and a **slot**. The ruled v1 set is exactly
  **`previous`, `play_pause`, `next`**, in that order (2026-11-27) — the caller's "in the end the order
  should be `Previous Track`, `Play Pause`, `Next Track`". The list is finite, declared, and
  compile-time. **`restart` is retired (U4, ruled 2026-11-27):** its slot, its drawing and its
  `Action` go, and restarting a track becomes the `previous` item's **position-dependent** behaviour
  (§4.5). Order is a declared field the ring reads, and §4.1's refusal is **linear** on purpose: a
  ring's drawn row must not be a transposition of its ring order, because "two items swapped" is
  exactly the backwards-feeling layout G1 found. A variant that legitimately wanted the ring to *start*
  at a different item would need a cyclic relaxation; no declared variant wants one, so the rule stays
  sharp rather than permissive.
- **A scene item** is whatever that scene decides its navigable things are. For `list` it is one
  **track row**, derived from `ALBM1`'s `n_tracks` and the window geometry — i.e. **runtime data, not
  a spec constant**. For a scene with no items (`blank`, the `art` scene, the `title_card`), the set
  is empty and the pane can never take the focus.

Item identity is an **action**, never a pixel and never a *button*. `previous` is an item; the `Left`
button is not. The current spec's `[[variant.control]] key = "left"` conflates the two and is one of
the schema gaps in §6 (G3).

### 1.3 The rules of movement

| rule | statement |
|---|---|
| **R1 — one owner per action** | An action is delivered to the focused pane only. There is no broadcast, no "both try it", and no fallback ("if the scene ignores it, the chrome takes it"). |
| **R2 — the chrome is a ring** | Within the chrome, `Left`/`Right` move to the previous/next item **and wrap**: `next` → `previous` at the end, `previous` → `next` at the start. One item per press, one press per item. |
| **R3 — the chrome has no vertical axis** | The chrome is one 8 px row. `Up`/`Down` are **not** a wrap axis in the chrome; they are the **pane-transfer axis** (§1.5). This is the resolution of the caller's "the same should go for up/down" against a journey whose list exits at its edges — the ambiguity is U2. |
| **R4 — a scene owns its ring** | A scene's navigation is its own: 1-D rows, 2-D grid, or nothing. Whether its edges wrap or leave is **declared per scene** (`edge = "wrap" \| "exit_to_chrome"`), because the list's journey wants "one past the end → back to the chrome" while a grid would want "wrap". The default is `exit_to_chrome`; `wrap` is legal and must be declared. |
| **R5 — the pane remembers** | The chrome remembers its last item; the scene remembers its last cursor and scroll. Leaving a pane does **not** reset either, and **a remembered cursor wins on re-entry** — `enter` is what a cold entry (or a declared `first`) uses (§1.6, U3). |
| **R6 — exactly one indicator** | At most one focus indicator is visible: the focused pane draws it, the other draws none. Two indicators on screen at once is a bug, not a style. |
| **R7 — shape, not hue** | The indicator is carried by **shape and luminance** (§5.6). A palette inversion alone is refused: it is the "differs only in colour" defect the plan already names. |
| **R8 — the zero state** | **At boot no pane has the focus and no indicator is drawn.** The first **D-pad** press — *any* direction — gives the chrome the focus **on its first item** (`previous`, §4.1). The direction is not used: entry is always the first item, which is the caller's answer to U1/U13 (2026-11-27). A scene can never be the first pane to take focus. |
| **R9 — unstarted is not paused** | Before `Start` the product is in a **boot state**, not a transport state: no audio chain, no period served, no clock, and **every action except `Start` (and the BIOS chord) is discarded**. It is one one-way flag (`Boot::Unstarted → Boot::Started`), not a modal stack; §7 is its spec. |

### 1.4 The pane transfer

The chrome band sits **below** the region, so the transfer axis follows the screen: **`Up` moves into
the scene, `Down` moves toward the chrome.** Precisely:

| action | no focus yet (R8) | focus is in the chrome | focus is in the scene |
|---|---|---|---|
| `Up` | **chrome takes the focus**, on the **first item** (`previous`) | if the scene has items, the scene takes the focus (its `enter` policy chooses the item); if not, **stays in the chrome** | scene-local (usually one item up) |
| `Down` | **chrome takes the focus**, on the **first item** (`previous`) | no-op in the chrome (there is nothing below it) | scene-local; at the last item/row it leaves per `edge` |
| `Left` | **chrome takes the focus**, on the **first item** (`previous`) | chrome ring (R2) | scene-local |
| `Right` | **chrome takes the focus**, on the **first item** (`previous`) — which is the caller's journey's first `Right` | chrome ring (R2) | scene-local |
| `L`/`R` (shoulders) | **scene cycle, global** — focus stays in the zero state | **scene cycle, global** — `R` next, `L` previous, wrapping the implemented-set (§1.5) | same, still global |
| `A` | **ignored** (there is no focused item) | **activate the focused chrome item** | **activate the focused scene item** |
| `B` | ignored | no-op (B is the game's button, and the chrome is the bottom pane) | **leave the scene**, focus returns to the chrome's remembered item |
| `Start` | **transport toggle, global** — and before the first press, the one key that leaves R9's boot state | **transport toggle, global** (never contextual) | same — a scene cannot claim `Start` |

One thing the zero state deliberately does **not** do: it does not choose an item from the direction
pressed. Every D-pad direction enters at the ring's **first** item (`previous`), because "the first
thing in the list" is the only entry point that needs no convention; the journey's `Right Right` then
reads `previous` → `play_pause`, which is the same two presses the caller described.

Two consequences stated so they are not discovered later:

- **`Up` from the chrome is the only way in**; `Down` from the chrome does nothing. That is spatial
  (the chrome is the bottom band) and it is what makes the caller's "focus transfers to the scene"
  one key rather than an ambiguous pair.
- **Leaving by the scene's edge and leaving by `B` are the same destination** (the chrome's
  remembered item) and may differ in whether the scene's cursor is kept — `edge = "exit_to_chrome"`
  keeps it, and `B` keeps it too (R5). A scene that wants "B resets" declares it.

### 1.5 Scene cycle (L/R)

`R` → the next **implemented** scene in the variant's declared registry order; `L` → the previous,
wrapping. The registry is `layout::SCENES` filtered to `SceneId::IMPLEMENTED` (§5.2, Q20 (ii)'s
mechanism — the *order* is reused; the *button* changes). The cycle is global: it works from the
chrome and from inside a scene alike, and it does not touch either pane's remembered item.

`L`/`R` are **not** transport verbs any more. The old aliases (`R` = next track, `L` = previous) are
removed (§10, C2). The one platform carve-out survives: **`L + R + Start` is the BIOS soft reset**
and is suppressed at the key layer exactly as today (`keys.rs:129`, `SOFT_RESET_CHORD`), and **hold
`L` at boot** still selects `WAITCNT = 0` (§5.3) because it is read before the application starts.

### 1.6 Enter/leave semantics are the scene's

- **Enter policy.** Each scene declares which item gains focus when the pane is handed to it. The
  rule is: **a remembered cursor wins on re-entry; the policy is what a cold entry (or a policy of
  `first`) uses.** The list's v1 policy is **`now_playing`** — the now-playing row, which is the
  first row when the album is at track 1, matching the caller's journey. `first`/`last`/`remembered`
  are the other legal policies and are declared, not coded per scene.
- **Leave.** A scene may observe that it lost the focus (to release a transient highlight, stop a
  blink timer, or commit a scroll). It is a notification, not a veto.
- **Activation.** `A` on a scene item calls that item's action. For the list that is "start this
  track from the beginning" — which is the transport's own **re-target to that track's start**
  (`start_track` + the fade/truncate/re-prime hand-off), not a seek: Q5 settled that the seam has no
  seek, and §4.5's wording is corrected for the same reason. It is also the *only* activation in the
  model that reaches the transport from a scene, and while the transport is `Paused` it is `Ignored`
  like every other skip — U16 owns that hole.

### 1.7 State, residency, and what survives

There are three different things and the plan has only ever named one of them:

| thing | survives a scene change? | who owns it | cost to restore |
|---|---|---|---|
| **model state** — cursor, scroll offset, remembered index | **yes**, always | the pane | nothing (a few words of RAM) |
| **tile/map state** — the scene's VRAM and map entries | only if the scene is `resident`; otherwise re-pointed on entry (§5.1's residency options) | the surface | the switch/upload price, 212,386 c mean (§13 AP-3b L3) or the sliced upload (§10.2) |
| **chrome state** — remembered item, drawn indicator | **yes**, and the chrome is never unmapped | the shell | nothing; the chrome's cells are rewritten only when they change |

The caller's "transitioning in and out of scenes should not destroy state, at a minimum for the
chrome" is therefore **cheap for the chrome and free for model state**, and only the *pixels* of a
non-resident scene have to be rebuilt — which §5.1/§5.2 already price as an event (§13 AP-3b L3).

---

## 2. The input map

### 2.1 The map (this supersedes §5.3's table, see §10 C2)

| button | action (pane-independent) | scope |
|---|---|---|
| **Up** | `Nav(Up)` | focused pane; chrome → scene transfer (§1.4) |
| **Down** | `Nav(Down)` | focused pane; scene edge may leave |
| **Left** | `Nav(Left)` | focused pane; chrome wraps |
| **Right** | `Nav(Right)` | focused pane; chrome wraps |
| **A** | `Activate` | focused item |
| **B** | `Back` | scene → chrome; no-op in the chrome |
| **Start** | `TransportToggle` | **global** |
| **L** | `ScenePrev` | **global** |
| **R** | `SceneNext` | **global** |
| **Select** | *unbound* | reserved; §11 U5 |
| **L+R+Start** | BIOS soft reset | platform, never application state |
| **hold L at boot** | `WAITCNT = 0` | platform (read before the app starts) |

The mapping is a **const table** in the UI layer, not a branch per screen, and it is the same table in
every pane; *what an action does* is the pane's business, *which action a button produces* is not.
That is the separation `keys.rs`'s own header already asked for (2026-11-18; §10 C3).

**While `Boot::Unstarted` (§7, R9), the table is not consulted at all.** The only action dispatched
is `TransportToggle` from a `Start` edge; every other edge is **consumed and discarded**, including a
held D-pad's autorepeats — so a thumb resting on `Right` when `Start` is pressed cannot move the
focus on the frame the chain arms. The keys machine still samples and debounces (the vblank handler
is registered before boot work), it is the *dispatcher* that has one door.

**The delivery half does not exist yet, and it is the largest omission the first draft had (G13).**
`keys::Sampler` posts exactly one thing — `Detected { frame, action }`, a **mapped** `Action` — and
`KeyFrame.repeated` is produced by the machine and **consumed by nobody**: the ISR logs it into the
`Input` ledger and the posted slot cannot carry it. So "a held direction walks the list" (§2.3) is not
a plumbing detail, it is a change to the ISR's contract: the slot has to carry the frame (or a UI event
that is an *edge or a repeat*), because the repeat is the whole reason the machine exists. Three
consequences the first draft missed:

- **The single-slot policy stays** — newest-wins with a `dropped` count, because a queue is what an
  interrupt must not grow. But a *dropped repeat* is a silently skipped row, which is the artefact a
  134 ms cadence exists to avoid: F6's held-direction arm must assert `dropped = 0` and name what a
  drop would look like in the log, or the cadence is a claim about a path with holes in it.
- **`Keys` needs a repeat-phase reset (G14).** The burst is armed off the debounced held set and
  nothing clears it. So a thumb resting on `Right` at the prompt is discarded *by the dispatcher* —
  and then the burst, already running, fires again within `REPEAT_INTERVAL_FRAMES` (≤ 134 ms) of
  `Start` and moves the focus to `play_pause`. R9's "a thumb resting on `Right` when `Start` is pressed
  cannot move the focus" is therefore true for one frame and false for the next eight. The fix is
  three lines (`Keys::reset_repeat()`, called on the `Unstarted → Started` edge), but it is not in the
  model as written, and a test that presses `Right` and `Start` on the same frame would not catch it —
  the *second* repeat does.
- **The posted vocabulary moves, not just the map.** `Action`/`Cursor` are today's now-playing
  vocabulary and both are read inside `album_play_main.rs` (the replay script's log `match`, the
  `SelectScene` arm, the skip and toggle arms). §5.1's "they move up into `ui.rs`" is right; what it
  also means is that `Detected` stops being `Action`-shaped, and the ROM's `input edge: … action=…`
  log lines become the UI's, so those gate greps move with it (§8.2, F7).

### 2.2 Precedence, when a frame carries two edges

One deterministic order, evaluated top-down: **BIOS chord** (suppresses everything, as today) →
**`TransportToggle`** → **`ScenePrev`/`SceneNext`** → **pane-local** (`Nav`, `Activate`, `Back`). The
order is a `const fn`'s if-chain, exactly as `now_playing_action` is today, so a thumb rolling across
two buttons cannot produce an order the gate has never seen.

### 2.3 Repeat

- **Navigation repeats** (`Nav` only): first repeat after `REPEAT_DELAY_FRAMES` = 24 frames ≈ 402 ms,
  then every `REPEAT_INTERVAL_FRAMES` = 8 frames ≈ 134 ms (`keys.rs:138-147`). Holding `Down`
  walks the list; that is the whole reason the machine exists.
- **Nothing else repeats**: `Activate`, `Back`, `TransportToggle`, `SceneNext`/`ScenePrev` are
  edge-only. A held `R` does not walk the scene cycle; a held `A` does not retrigger.
- **Repeat is a repeat, not a sequence.** There is no time window, no double-tap, no chord that is
  not the BIOS's — the same rule §5.3 already states, now applied to navigation as well.

### 2.4 Deliberately absent

- **No pre-programmed sequences.** The old "Down = restart the track", "Left/Right = skip track",
  and the A≡Start equivalence are all removed: those are *items* now (`previous`, `next`,
  `restart`), reached by focus and triggered by `A`. A button that does something different
  depending on how many times it was pressed in a row is the pattern this rule bans.
- **No aliases.** One button, one action. (`A` no longer mirrors `Start`; `L`/`R` no longer mirror
  `Left`/`Right`.)
- **No per-scene key maps.** A scene says what `Nav`/`Activate`/`Back` *mean to it*; it never sees a
  raw button. Two scenes cannot disagree about which button is "up".

---

## 3. What a scene owes the shell (the contract)

The drawing half of the scene contract is §5.2's and is unchanged: one rectangle, writes only inside
it, every cell backed opaquely. The interaction half is new and is four methods:

| method | input | output | notes |
|---|---|---|---|
| `items()` | — | count (0 allowed) | 0 means the pane can never take focus (R4 with an empty set). Runtime-derived for `list`; a constant for the rest. |
| `enter(policy)` | the declared enter policy + the album | the item index to focus | "the scene itself is responsible for focus enter semantics" — this is where that sentence becomes code. |
| `leave()` | — | — | release transient state; no veto. |
| `handle(action)` | `Nav`/`Activate`/`Back` | `Handled` \| `Leave` \| `Ignored` | `Leave` is how the scene's own edge policy returns focus to the chrome. `Ignored` must be *rare*: an understood action with nothing to do (e.g. `Left` in a 1-D list) — not a way to fall through to the chrome, which R1 forbids. |
| `paint(...)` | its state | writes into its rectangle | unchanged from §5.2. |

**A sequencing fact this exposes, and §9 now obeys it:** `SceneId::IMPLEMENTED` is `art`, `title_card`,
`blank` today and **all three return `items() == 0`** — so until `list` lands, *no scene can take the
focus at all*, and F3's transfer legs exist only in the host suite against a synthetic scene. The pane
handoff can be built before that (it is a shell concern), but a ladder that put the ROM legs of F3 in
the same PR would be green on a path no thumb can reach.

Constraints that keep this inside the plan's budget:

- **No allocation, no trait objects.** Each scene is a struct with `const`-constructible state, held
  in an enum in the shell (U8). Dynamic dispatch would make the call graph unbounded and the
  per-frame cost un-auditable, which is §5.1's whole objection to a widget tree.
- **A scene may not draw outside its rectangle** — so it draws its *own* focus indicator when it has
  the focus, and the chrome draws its own. R6's "one indicator" is a *coordination* rule between the
  two, not a shared sprite (U7).
- **A scene change does not stop audio.** The cycle is a presentation change; the transport is
  untouched (already true today: `SelectScene` and `PauseToggle` are separate actions).

---

## 4. The chrome

### 4.1 Items and order

**Ruled 2026-11-27: the ring is exactly three items, in this order — `previous`, `play_pause`,
`next`.** The order is a **declared field**, not the order the slots happen to sit in, and the two
must agree: the generator refuses an item order that is not the left-to-right order of its slots
(G4). `restart` is **retired** (U4, ruled 2026-11-27): the ring is the three items above, the
return-glyph slot is dropped, and the chrome term moves 10 → 9 with B's distinct pool sum 463 → 462.
Restarting a track does not disappear — it moves into `previous` (§4.5).

**The current drawn order is wrong for this model and has to move.** Today the slots are declared in
button order — `select@21, start@22, right@23, left@24, down@25` — and the drawings are
`diamond, play/pause, '>' (next), '<' (previous), restart`. So "next" is drawn **left of** "previous":
`Right` from `play_pause` would move to "next" and then to "previous", which reads backwards. The
item model wants `previous | play_pause | next` left to right (the caller's journey assumes it). This
is a *layout* change (slot positions in `layout_spec.toml`), not a code change, and it is G1.

**The mechanical fix, and the trap in the obvious reading of it.** `select@21` and `down@25` retire
(U4, G2), which leaves three items to place in ring order. **Ruled 2026-11-28: with a gap between
them — `previous@21, play_pause@23, next@25`** (all three fit inside the declared `keyart` box,
columns 21–29, both tile rows; columns 22/24/26–29 stay backdrop). The gap is not decoration: the
focus indicator is **salience**, and a caret under an item with a blank cell beside it is legible at a
glance in a way that three touching 8×8 drawings with a caret under one of them is not. Consequences of
the spacing:

- **A focus move is now two cell writes, not one** — clear the old caret's cell, set the new one's —
  so §4.2's option A costs 2 × 489 c ≈ 1 k c plus the commit, against 489 c for the touching layout.
  That is 4.5% of the 12,222 c commit it rides on, so the gap is nearly free; F6's "1–3 cells" already
  bounds it.
- **The ring order is unchanged by the gap**: 21 < 23 < 25 is still `previous → play_pause → next`
  left to right, so G4's rule is satisfied by the layout rather than beside it.

What the fix is *not* is "swap `right` and `left`": a swap moves two adjacent items and reverses them,
so the drawn row would read `play | previous | next` while the ring reads
`previous → play → next`. **G4's rule refuses that, correctly** — the rule is doing its job, which is
why G1's recipe is corrected here rather than G4's rule. Two more consequences worth having written
down:

- **Nothing else in the chrome is drawn.** Ruled 2026-11-28 (U17): the three item icons are the whole
  chrome's interactive content, and **no `L`/`R` affordance, no `A` marker and no `B` hint are
  declared** — `L`/`R` cycle scenes and `B` leaves a scene, neither is focusable, and a thing that
  cannot hold the focus naturally cannot be activated, so it needs no drawing to say so. This closes G2
  as *retire both slots and declare nothing in their place*; the columns 24–29 that the draft
  considered spending on affordances stay backdrop, and the `keyart` box can shrink from 9 columns to
  5 (21–25) whenever someone wants the cells for something else.
- The ring's order is the *item* order, and the item order is derived from the slots; nothing in the
  generator knows what a ring is. G4's refusal is the only thing standing between a re-layout and a
  backwards chrome, which is why it must land **with** the layout change (UI-0) rather than after it
  (UI-1).

### 4.2 The indicator

The chrome band is two tile rows; the control drawings are one tile tall and live on the upper row, so
**the lower row is free under every item**. That is the cheapest place for an indicator and it is the
default proposal:

| option | how | cost per focus move | verdict |
|---|---|---|---|
| **A. underline caret** (default) | one authored 8×8 tile in the band's *lower* row, under the focused item's cell | 1–2 cell writes (489 c each; two when the items are spaced, as ruled) + one commit path | lands now; cheap; shape-bearing; no OBJ |
| **B. bracket/frame** | 2–3 authored tiles around the item | 2–3 writes | same class |
| **C. sprite** | 1 OAM entry over the item, presented by the existing OAM shadow | ~226 c/entry, no map copy, no foreground commit | the §5.7-aligned option; needs an **OBJ tile budget** declared (AP-6 owns that ceiling today, and no BG-side `tile_budget` can see it) |
| **D. palette inversion only** | recolour the item's cell(s) | 1 write | **refused by R7** (hue alone) |

Whichever is chosen, **the per-move cost is a commit, not the cells**: agb has no partial-map commit,
so a one-cell indicator change pays the foreground's whole 2,048 B map on the fast path
(12,222 c, §13 AP-3c arm A) and up to 280,360 c when a vblank has not elapsed. At repeat cadence
(134 ms ≈ 8 vblanks) the fast path is what a held direction sees. The spec's budget rule is therefore:
**at most one commit per pass, and a held direction must not raise the commit rate above one per
`REPEAT_INTERVAL_FRAMES`**.

**That rule is a new global invariant, not something the existing latch already gives (corrected
2026-11-28).** `paint.due()` bounds the *clock/bar* reader to one sample per latched video frame; it
does not bound the number of *commits* in a pass. The loop already has two committers —
`refresh_surface` under `paint.due()`, and a scene change's own commit, which suppresses the former
through `scene_committed` — and the indicator would be a third claimant on the same pass (a focus move
and a clock tick in one frame). So the rule needs an owner: one commit site at the end of the pass
body, with all three writers registering into it, and a counter in the log (`commits=`) that F6 reads.
Stated the other way round: **if the indicator is allowed its own `commit()` call, F6 cannot be true**,
because two writers would each believe they were the only one.

**The pool arithmetic, which the first draft left out.** Retiring `restart` takes the chrome term
10 → 9 (B's distinct sum 463 → 462); retiring the `select` diamond too takes it to 8; **option A adds
one authored tile back, so UI-1 lands on 9 tiles and 462 distinct — one tile *under* today's 463, not
over it.** Option C (sprite) leaves 8 and spends an OAM entry instead, from AP-6's ceiling (§4.2's own
"needs a declared OBJ term"). Either way the indicator is paid for by the two retirements rather than
found in headroom nobody had counted, so **G6 is a declaration gap rather than a budget gap**: the
measured sum does not move, but no gate would notice if it did.

### 4.3 The freed cells, and the L/R affordances

Two layout facts follow from this model:

- **The `select` slot has nothing to draw.** `Select` is unbound (§2.1) and the scene cycle moved to
  `L`/`R`. **Ruled 2026-11-28: retire it and declare no affordance in its place** (U17) — the diamond
  and the `restart` drawing both leave, the chrome is three icons, and its tile goes back to the pool
  (G2, U4).
- **The debug word's strip is not needed for affordances any more, and it is staying debug chrome**
  (ruled 2026-11-28, U12). `keylabels` is **9 characters** wide (9 columns × 2 tile rows = 18 cells at
  `ark10`'s 8 px maximum advance — "nine cells" in the first draft read as a count of characters and a
  count of cells at once), at columns 12–20, and it remains debug chrome behind `state-word`: no
  `L`/`R`/`A`/`B` legend (U17) and **no focused-item name either**. The caller's reason is the one that
  survives the review: the chrome does not need the extra information, and the caret carries the state.
  Resurrecting the strip for a name later costs nothing a model change would—it is a debug feature whose
  removal is a feature-flag flip, not a schema question—so the flip has no pending decision at all.

### 4.4 What the chrome does on activation

`A` on `next` and on `play_pause` is the transport's existing skip / faded-toggle path. `A` on
`previous` is those same paths with a **conditional target** (§4.5). **No new transport transition is
introduced by this spec** — the verbs already exist and are already measured (pause: action→freeze 2
periods, measured frame 45→49 ≈ 67 ms; skip: ≤ ~0.6 s; both §4.4/R13). What changes is *how they are
reached*, and, for `previous`, *which* of two already-existing targets it picks.

### 4.5 The `previous` item is position-dependent (ruled 2026-11-27)

**The rule.** Activating `previous` restarts the current track when the current visit is past a dwell
threshold, and otherwise goes to the previous track (wrapping at the album's start):

```
visit >= PREVIOUS_RESTART_SAMPLES   ->  target = the current track, re-targeted to its start
visit <  PREVIOUS_RESTART_SAMPLES   ->  target = the previous track (wrap 0 -> last)
```

**The threshold is 2 s**, expressed once at the product rate (`2 × RATE_HZ` = 32,768 samples at
16,384 Hz), owned by the transport where the position lives, and **not** an album or variant field for
v1 — a different threshold is taste, not a budget. The comparison uses the **same visit position the
clock and the bar already display** (`paint.visit`), so the rule agrees with the screen: if the clock
has not reached `0:02`, `previous` goes back a track.

**Why this is not the pattern the plan bans — stated honestly, because the first draft's version of this
paragraph was a rationalisation (corrected 2026-11-28).** The plan objects to this rule in **two**
places, and they are different objections:

- §5.3 refuses anything that needs *two buttons within a time window* — the "press `Left` twice
  quickly" chord. This rule does not do that: it counts nothing, and one press is one deterministic
  decision. The first draft answered this objection and stopped there.
- **§4.4 refuses the rule itself, in words that describe this rule exactly:** *"the control map uses no
  time-dependent chord — 'previous track if within 3 s of the start' is exactly the rule that would
  need the ledger, so it is replaced by a dedicated restart button."* The objection there is not press
  timing at all; it is that the comparison would need the **audible-position ledger** — the block map
  from DMA buffer to sample range — that §4.4 declines to build, and AP-2 is the gate that would have
  to hold if one ever were.

**So the reconciliation has to be about the ledger, not about the chord.** The rule does **not** need
that ledger: `paint.visit` is `committed_samples − visit base`, a whole-track-sample count the
position module already produces for the clock and the bar. What changed since §4.4 was written is
*that the product now displays this number* — the objection was to an invisible ledger kept for a
policy, and the thing being read here is the number on the screen, at the same instant, with no second
source that could disagree. That is the strongest form of the argument, and it is still weaker than the
position the plan took: **C15 records this reversal as a deliberate product ruling by the caller (U14),
not as a re-reading of §4.4.** §4.4's own paragraph is annotated in place (2026-11-28) rather than left
standing, so a reader cannot find a live contradiction.

**The restart target exists; the pause's version of it does not (corrected 2026-11-28).** A restart is
the existing target — the same track, **re-targeted to its own start** through the same fade → truncate
→ re-prime machinery as a skip (measured ≤ 0.6 s, never a click, §4.4). There is no seek in the seam
(Q5), so "seek to 0" is the wrong phrase; the code calls `start_track` again. Either way the merge
costs one conditional in the target choice, not a transition — **while the transport is playing**. The
paused case is a different matter; see below.

**Three edges the spec fixes rather than leaving to the implementation:**

| case | behaviour |
|---|---|
| track 1, visit < 2 s | `previous` **wraps to the last track** — the album's ends wrap (§5.3), and the rule does not special-case track 0 |
| a transition in flight (`Skipping`/`Repriming`/`Pausing`/`Resuming`/`Buffering`) | **ignored and counted**, exactly as `Transport::skip` is today (it only acts from `Playing`); the contract has no cancel |
| the transport is `Paused` | **Ruled 2026-11-28 (U14 = defer): the press changes the *selection* and the transport stays `Paused` on it**, with the target applied at the resume. This is a new transport *path*, not a one-line change — `Transport::skip` acts only from `Playing`; `Paused`'s own contract refuses a re-target (the ring's head is the sample the freeze stopped on, and the resume ledger refuses a re-arm whose head moved); and the `Buffering` route that *would* refill ends by arming the chain (`end_buffer → Step::Prime`), which is not "stay paused". The options and their real costs are below |

**Why "change the track and stay paused" is not the cheap option it reads as.** Everything needed
exists — just not in the shape the sentence implies. A frozen chain cannot serve a fade, so retargeting
in place would have to truncate the ring with no cover (booking `dropped`), call `start_track(track)`
again, decode to `PREFILL_SAMPLES` with the chain still frozen, and then *not* arm. That is a
**frozen-feed state beside `Buffering`**, plus an amendment to §4.4's no-seek invariant — which today is
exactly what makes a paused resume provably play the sample the freeze stopped on — splitting "resumed
the same visit" from "re-targeted while frozen" — plus the pause ledger's head-equality term learning
the difference. It is a transport PR with its own gate legs.

| option | what it costs | verdict |
|---|---|---|
| **defer the target — RULED 2026-11-28 (U14)** | the press records a pending target and moves the *painted* track to it at `0:00`; the **resume** applies it, which is the one moment the ring is legitimately replaced (a `Buffering`-shaped prefill that ends in the prime it already ends in). No new state, no invariant amended — one field beside the existing `pending`, one branch in the resume arm, one field on the `View` | **ruled**: the screen tells the truth immediately, the audio goes where the rule says, and the wait is the resume's own already-bounded one |
| retarget while frozen | the new frozen-feed state + §4.4's no-seek invariant amended + the pause ledger's head term split in two | legal, most work, and it weakens the paused-resume claim for every listener who never presses a transport item |
| ignore while paused | one line — and a transport item that does nothing at exactly the moment the user is looking at its position | **refused 2026-11-28**: the caller's answer to U16 is that track switching while paused is a deficiency, not a behaviour |
| change the track and resume | one line (the existing skip path) — and a pause the user did not lift | refused on product grounds: `Start` is the key that starts audio |

**Three properties worth noting, because they are what make the deferred option faithful rather than a
compromise.** *(i)* The pending target is **not** a queue or a mode: one `Option`, cleared by the press
that supersedes it. *(ii)* With the painted visit already at `0:00`, a **second** `previous` while
paused reads `visit < 2 s` and goes back one more track — so the CD-player behaviour the caller
described ("two presses go back two tracks") holds while paused too, without a second rule. *(iii)* The
"painted track" is more than the clock: **the title line, the bar, and the list's now-playing mark all
read the pending target**, otherwise the chrome shows track 2's title over track 3's `0:00`. That is one
field carried through the `View`, not three, but it is a display commitment the first draft did not
name.

**U16, ruled 2026-11-28 — the rule is general, not `previous`'s** (caller: *"not being able to switch
tracks while paused is a deficiency … going to the next track or the previous track should work
similarly to while music is playing"*). So while `Paused`: `next` selects the next track, `previous`
applies §4.5's 2 s rule against the painted visit, and `A` on a list row selects that row — all of them
deferring to the resume and leaving the transport `Paused` at `0:00` of the new selection. **The same
hole exists today because `Transport::skip` is `Ignored` outside `Playing`; a focus model makes it far
more likely to be hit, because pausing and then navigating is the natural thing to do.** One
consequence the implementation owes: the pending target and the deferred resume are now **one**
mechanism (`{kind, index}` plus "apply at resume"), not a `previous` flag and a separate `next` path —
which is why U14 and U16 were answered together rather than months apart.

---

## 5. Architecture

### 5.1 Shape

```
album_play_main.rs          owns the frame loop, the transport, the feeder          (unchanged)
├── keys.rs                 buttons → edges / repeats / debounce; NO screen knowledge
├── ui.rs            (new)  buttons → Actions; owns Pane + chrome cursor; dispatches
├── chrome.rs               authored tiles; transport_icon (already state-dependent)
├── surface.rs              draws writes; no notion of focus
└── scene/           (new)  one struct per scene: items/enter/leave/handle/paint + state
```

- **`keys.rs` keeps its ignorance** — it must not learn what a screen is (its own header's rule, and
  the reason the sample lives in the vblank handler). Its `Action` enum and `Cursor` plumbing move
  *up* into `ui.rs`, where the map becomes a const table and the context lives.
- **`ui.rs` owns exactly two things**: which pane has the focus, and the chrome's cursor index. The
  scene owns its own cursor/scroll (§1.6). That split is what makes "the scene is responsible for
  its enter semantics" literal rather than a comment.
- **`surface.rs` stays a drawer.** It already takes a `View` of plain values; the focus indicator
  becomes two more fields on it (or a small `FocusView`), and it must not gain a dispatcher. This
  keeps AP-3's host composite testable with no input at all.
- **Dispatch is an enum `match`, not a trait object** (U8): `Pane::Chrome` → the item table;
  `Pane::Scene` → the scene enum's `handle`. Bounded call graph, audit-able cost, no allocator.
- **`album_play_main.rs` is the one line above that is visibly *not* unchanged**, and the first draft's
  diagram said otherwise. Three of its jobs move: the ISR's posted slot becomes frame-shaped (§2.1,
  G13), the four hand-written `if let Some(detected) && detected.action == …` blocks (`:4380-4430`)
  collapse into one `ui.dispatch(…)` call, and the pass gains **one** commit site (§4.2's invariant).
  What stays exactly where it is: the transport, the feeder, the paint reader and the gate ledgers.

### 5.2 What each layer is allowed to know

| layer | knows | must not know |
|---|---|---|
| `keys.rs` | button bits, edges, repeat timing | panes, items, scenes, transport |
| `ui.rs` | the button→action table, the pane, the chrome cursor, the scene registry | tile indices, cells, palettes |
| scene modules | their own items, cursor, scroll, and what `A` means to them | the chrome, other scenes, the transport's internals |
| `surface.rs` | rectangles, cells, tiles, glyphs | why an item is focused |

### 5.3 The per-frame budget this adds

Nothing here is a new audio-path term. The costs are **events**, and they are the already-measured
ones: a focus move is 1–3 cells plus one commit (12,222 c fast path); a list scroll step is one row
(88 writes ≈ 81 k c at 917 c/cell) plus the indicator; a scene cycle is the existing switch
(212,386 c mean / 280,360 c worst) or the sliced upload; a row activation is the existing skip.
**The one new figure to add to §10.2 is "navigation move" — this spec proposes it be recorded, not
invented**: `1–3 cells + one commit`, and the *measured* figure arrives with the first replay that
holds a direction (AP-10's F6).

**And one latency distinction the dispatch must keep, because the existing loop does not have it.**
Every transport action today is issued **at a period boundary** — the frame slice exists so the DMA is
never changed mid-buffer (§4.4's splice rule) — and the `reaction` term is gated against that wait
(`MAX_BOUNDARY_FRAMES`). A `Nav`, an `Activate` that is not a transport verb, and the indicator's cell
write have no such constraint: they are state changes plus cells, and they land on the next **pass**. A
UI that routes a focus move through the period-boundary door pays up to a period (34 ms) plus a pass
for a keystroke the transport never asked for, and it would make a held direction's cadence depend on
the audio clock rather than on `REPEAT_INTERVAL_FRAMES`. F6 should measure the pass, and the dispatch
should say which of its arms are period-bound (exactly two: `next`/`previous`'s transport half and
`play_pause`'s).

---

## 6. What `layout_spec.toml` has to declare (the schema gaps)

The generator derives everything it audits from the spec, so a model that exists only in Rust is a
model no gate can refuse. These declarations are the *minimum* for the focus model to be auditable.
Each is a gap today (G1–G12, listed at the end of this section).

```toml
# The chrome's focusable items, in focus order. Order is declared, and must equal the
# left-to-right order of the slots it names (refused otherwise).
[[variant.item]]
id       = "previous"        # the focus identity, not a button
band     = "chrome"
slot     = "keyart"
x0 = 21                      # the cell the item occupies, inside the declared art element
y0 = 0
x1 = 21
y1 = 0
activate = "track_prev"      # what A does; a closed vocabulary of transport verbs

[[variant.item]]
id       = "play_pause"
...

```

and per scene:

```toml
[[variant.scene]]
name    = "list"
kind    = "text"
resident = true
focus    = "items"           # "items" | "none"  — can this scene ever take the focus?
enter    = "now_playing"     # "now_playing" | "first" | "last" | "remembered"
edge     = "exit_to_chrome"  # "exit_to_chrome" | "wrap"
cycle    = true              # is this scene in the `L`/`R` cycle? (false: boot only)
```

The **boot screen is a scene, and it is the `cycle = false` one** (ruled 2026-11-28, U15):

```toml
[[variant.scene]]
name    = "boot"
kind    = "composite"
resident = false             # like the card: entered and left once per run
shares_image = "art"         # the same cover — one image, two boxes (§9.0a)
focus    = "none"            # R8: no indicator at boot; R9: nothing navigates
cycle    = false             # never reached by L/R, and the registry filter must say so
```

Its spans are the card's own five boxes declared again under `scene = "boot"` (elements are keyed by
scene, so a shared box is a *declaration*, not an alias): title, artist, year, counts, and the prompt.
**The renderer is not duplicated** (ruled 2026-11-28): `boot` paints through the card's own function,
which takes the prompt as an **optional span** (`Option<(element_index, &str)>`, the same shape as the
already-optional `card_year`). A second paint path would re-implement §5.5's elision, the backing fill
and the per-span `Cells` diffing, and **the drift would be invisible** — AP-3 snapshots each scene
separately, so a card that elides at 103 px and a boot span that does not is two greens, not a red.
What *is* duplicated is the element boxes, and that duplication is **still the caller's to review**;
the alternative, a `shares_elements = "title_card"` alias, is more schema for less text and is not
taken unless the review asks for it.

The text pool does not move — `text_distinct` is the face's **atlas** tile count, not a count of used
characters — and the image term is shared by `shares_image`, so the boot scene costs **0 tiles**; what
it costs is spec surface (six element blocks), one registry field and one scene entry at the press
(§7.3).

plus one indicator term, whichever §4.2 option is chosen: `focus_art_tiles = 1` added to
`chrome_art_tiles`'s enumeration (BG indicator), or an `obj_focus_entries`/`obj_focus_tiles` term in
the OBJ budget AP-6 owns (sprite indicator).

**Refusals that must exist before the model is real** (proposed §9.0 ids; the plan's register is the
place they belong, and none is implemented):

| id | fires when | why it is a *build* refusal |
|---|---|---|
| `item-order-not-slot-order` | the declared item order is not the left-to-right order of the slots | a ring that moves backwards is a taste claim the spec cannot see, so the geometry must agree with the order |
| `item-outside-band` / `item-overlaps-item` | an item's box leaves the chrome band or shares a cell with another | two items on one cell is two indicators R6 cannot draw |
| `item-activate-unknown` | `activate` names a verb outside the closed transport vocabulary | the vocabulary is the transport's; a typo would be a dead item |
| `scene-focus-none-but-items` | a scene declares items but `focus = "none"` | an unreachable item is a hole |
| `scene-focus-items-but-fill` | `kind = "fill"` with `focus = "items"` | a flat fill has no items; the declaration is a lie |
| `scene-enter-not-a-policy` | `enter` outside `now_playing`/`first`/`last`/`remembered` | the policy is what the scene's `enter()` receives |
| `focus-indicator-over-budget` | the indicator's tiles/entries push their pool over | same arithmetic as `image-tiles-over-budget` (R14's panic is not a refusal) |
| `scene-items-not-derived` | a scene declares a *count* of items where the count is runtime data (`list`) | the album's track count is `ALBM1`'s, not the spec's — a declared number would be wrong per album |
| `scene-cycle-not-a-bool` | `cycle` is anything but a bool, or the cycle is empty | the flag is the only thing keeping a boot screen out of `L`/`R`; a typo would put a dead-end scene in the ring |
| `scene-boot-and-cycled` | the boot scene declares `cycle = true`, or a `cycle = false` scene is the *only* implemented one | a `PRESS START` reachable by `L`/`R` over a playing album is exactly the dishonesty Q20(iii) refused; and a cycle with nothing in it is an unreachable screen |

**And one meaning change that is not a new field:** `[[variant.control]] key` currently names a
**button** (`select`/`start`/`right`/`left`/`down`). Under this model the chrome's focusable things are
**actions**, so either `control` gains an `action` and keeps `key` as the button it *illustrates*, or
`item` replaces it (G3). The current five keys also cannot express `A`, `B`, `L`, `R`, which this map
needs as affordances.


### 6.1 The gap register — what the layout plan does not have yet

The list below is the answer to "where are the gaps in the layout effort" as a checklist, so a later
PR can tick a line instead of rediscovering it. **None of these exist today**; G1–G6 and G8 are
blocking for **UI-0** (they are the schema), G7/G9/G10 for UI-1–UI-3, and G11–G17 are delivery or
re-baselining work that lands with the step that owns them (§9).

| # | gap | where it bites | closed by |
|---|---|---|---|
| **G1** | **The drawn order is the button order, and it disagrees with the focus order.** Slots are `select@21, start@22, right@23, left@24, down@25`, and the drawings are `diamond, play/pause, '>' (next), '<' (previous), restart` — next is drawn **left of** previous, so a ring over them moves backwards | §4.1, the journey's `Right Right` | re-layout the three items into columns 21–23 in ring order + `item-order-not-slot-order`. **Not a swap** (§4.1): swapping `right`/`left` is refused by that very rule, correctly |
| **G2** | **The `select` slot has no item** (`Select` is unbound) and there are **no `L`/`R`/`A`/`B` affordance slots at all** — the five declared slots are the old button map | §4.3, the chrome row | **Ruled 2026-11-28 (U17): retire `select` and `down` and declare no affordance in their place.** `L`/`R` cycle and `B` leaves, neither is focusable, so neither is drawn; the chrome is three item icons with a gap between them (21/23/25) and nothing else. The columns 24–29 and the `keylabels` strip stay free |
| **G3** | **`[[variant.control]] key` names a button, not an action**, so the schema cannot express "this is the `next` item" apart from "this is where the Right button's icon sits"; `A`/`B`/`L`/`R` are not expressible | §1.2, §6 | `[[variant.item]]` (this section) |
| **G4** | **No declared focus order.** Nothing ties the item order to the slot geometry, so a ring can be built backwards and no gate can see it | §4.1, F1/F5 | declared order + `item-order-not-slot-order` |
| **G5** | **The scene schema is tiles-only**: `kind`/`resident`/`shares_image` say what a scene *draws*, and nothing says whether it can take the focus or how it enters/leaves | §1.6, §3, UI-2 | `focus`/`enter`/`edge` fields + three refusals |
| **G6** | **No indicator term in any budget.** Chrome tiles (10 today, 9 after U4's retirement) and the OBJ ceiling (AP-6's) both have no focus-indicator line, so either option in §4.2 would be an *unaudited* tile | §4.2, R14's rule | `focus_art_tiles` (BG) or `obj_focus_*` (sprite) + `focus-indicator-over-budget` |
| **G7** | **Scenes are functions, not stateful objects.** `SceneId` is an enum and `surface.rs` paints each scene from `(album, scene)`; there is no place a cursor or a scroll offset could live | §1.7, §3 | the `scene/` structs + the `SceneState` enum (U8) |
| **G8** | **No item-count declaration, and there must not be one for `list`** — the count is `ALBM1`'s `n_tracks`. The schema has `min_visible_rows` (a floor), not an item count, and the audit must not start reading one | §1.2, §6 | stated here; `scene-items-not-derived` refuses the temptation |
| **G9** | **No prompt/hint element, and the art scene has no text element.** The `title_card` has four spans and an 80×80 picture; the caller's boot screen is cover + "PRESS START" | §7, U9 | U9's ruling; a declared span (card) or a backing-strip element (full-bleed) |
| **G10** | **No gate row for interaction.** AP-0…AP-9 name probes, phases and budgets; nothing names focus, wrap, ownership or state survival | §8.1 | AP-10 (proposed) |
| **G11** | **The existing replay scripts encode the old map** (`replay-script-a/b/c`, the `album-test` script, the card's two `Select` edges) | §8.2 | re-script through focus (UI-1/UI-2) |
| **G12** | **No §10.2 budget row for a navigation move or a list scroll step.** The element rows price a bar cell, a clock digit, a title line and a region repaint — the two costs a focus model actually spends are both missing | §5.3, F6 | one row each, measured by F6's arm |
| **G13** | **The ISR's posted slot cannot carry a repeat.** `Sampler` posts one mapped `Action` (`Detected`); `KeyFrame.repeated` never leaves the ISR (it is only *logged*, by `Input`). So "hold to walk the list" has no delivery path, and a `dropped` repeat becomes a silently skipped row | §2.3, §5.1, F6/F13 | the frame-shaped post (§2.1) + `dropped = 0` on the held-direction arm |
| **G14** | **No repeat-phase reset.** The autorepeat burst is armed off the debounced held set and cleared only by a direction *change*, so a direction held across the `Unstarted → Started` edge fires again ≤ 134 ms after `Start` — R9's "cannot move the focus" is true for one frame and false for the next eight | §2.1, §7.2, R9 | `Keys::reset_repeat()` on the boot edge + F13 |
| **G15** | **The boot prompt has no element and never had one.** `title_card`'s four spans are title/artist/year/counts (`card_title`/`card_artist`/`card_year`/`card_counts`), deliberately — the prompt was dropped in 2b PR-(3) because audio auto-starts. So U9b's "the card with its prompt restored" is a **new** `[[variant.element]]` + a `paint_card` span + a fit verdict, and on the *cycle* path it says "PRESS START" over a playing album unless the prompt is conditioned on `Boot::Unstarted` | §7.2, U9b, U15 | one declared element, painted only while unstarted (U15) |
| **G16** | **The unstarted state meets the wedge leash.** `WEDGE_PASSES` = `(36+2)×4096` passes, incremented every pass with a full ring and no frame due; at the video cadence that is ≈ 43 minutes, after which a prompt left on screen prints `FAIL the ring wedged` and halts. The unstarted arm must either skip the leash or count "waiting for `Start`" as progress | §7.2, F10 | one condition in the leash, printed so the arm is not silently weaker |
| **G17** | **`next`/`previous` (and a list row's `A`) are dead while `Paused`.** `Transport::skip` acts only from `Playing`, and nothing in this spec binds a paused transport item — U14 covers `previous` only, and a focus model makes all of them far likelier to be pressed, because pausing and then navigating is the natural thing to do | §4.5, U16 | the deferred target generalised to `{kind, index}` |


---

## 7. The boot flow: the unstarted state (U9, approved in principle 2026-11-27)

The caller's journey opens on **album art with "PRESS START"**, and nothing happens until `Start` —
**including no navigation**. That is a new boot state, and this section is its spec: what is on screen,
what has been computed, what has not, and what the measurement becomes. The *flip is approved in
principle*, **and the placement is ruled (U9a = C, 2026-11-27)**; the boot screen (U9b) is what
remains open. The boot path is not one step, so what follows itemises it and says exactly where the
gate sits under C.

### 7.1 What the boot path does today, and what the gate could sit in front of

Measured on the 4-track fixture (`album-test.log:9-10`, 9,477,295 c = 564 ms, and the same
decomposition in `album-rom-run.log`):

| stage | cycles | ≈ time | what it does | before `Start`? |
|---|---|---|---|---|
| vblank handler + input source | (before the clock) | — | the keypad sampler exists; it reads, it does not act | **yes** |
| `surface` / `parse` / `ui` | ≈ 25 k + ε | ≈ 2 ms | `Album::parse` (the `ALBM1` reader), then the first UI frame and its commit | **yes** — the screen must be up |
| `open` | 4,343,040 | ≈ 259 ms | `open_source`: every pin cross-checked against the blob (R11's staleness check, R5's `DATA ERROR` class) | *the placement question* |
| `gate` | 86,781 | ≈ 5 ms | the track's container/profile gate | *the placement question* |
| `prefill` | 5,022,982 | ≈ 299 ms | 16 chunks / 8,960 samples decoded into the ring — **the only stage that decodes audio** | *the placement question* |
| `prime` | — | — | the first chunk handed to the DMA | **no** |
| `enable_sound` + `arm_audio_chain` | — | — | `SOUNDCNT` master enable, timer1 handler, timers armed | **no** |
| the loop | — | — | `paint.due()` → clock/bar/scene refresh; produce/serve one period | **no** |

So "nothing happens before `Start`" has a precise meaning, and three honest placements:

| placement | validated at boot? | decoded before the press? | `Start` → first audible sample |
|---|---|---|---|
| **A — before `open`** | no | no | ≈ **563 ms** (open + gate + prefill + prime) |
| **B — after `prefill`** | yes | **yes** (≈ 560 ms of CPU spent before the user asks) | **≈ 0** |
| **C — after `gate`, before `prefill`** | **yes** | **no** | ≈ **305 ms** (prefill + prime) |

**Ruled 2026-11-27: C.** It splits the boot path where the two concerns actually separate: *validating the
album* is boot work whose failure must be shown **before** the prompt (a `PRESS START` that then
refuses is the worst possible moment for R5's error screen), while *decoding the first half-second of
audio* is playback work that has no business running before the user asked for playback. A is the
strictest reading and it is **rejected for the reason above** — it moves the pin check and the
container gate to the press. B is legal and would make the press feel instantaneous; it is the option
to take **if** the press latency is judged a product problem, and the cost is that the device has
already spent 560 ms of CPU on audio nobody asked for.

Under all three the **UI is up in ≈ 2 ms** (parse + one frame), which is the number a user perceives.

### 7.2 What the unstarted state is

- **One flag, one way.** `Boot::Unstarted | Boot::Started`; `Start` (and only `Start`) sets it. It is
  not a pause, not a mode stack, and not a scene: it does not change the pane model, the item model,
  or the scene registry.
- **The screen is its own scene — `boot` — not the `title_card`** (ruled 2026-11-28, U15). This
  corrects the draft twice over: the card's four spans are title/artist/year/counts and **no prompt has
  ever been drawn** (it was removed by decision in 2b PR-(3), because audio auto-started), and a prompt
  *added* to the card would have to be conditioned on `Boot::Unstarted` or it says `PRESS START` over a
  playing album when the card is reached with `L`/`R`. A separate scene removes the condition instead of
  adding one: **`boot` is a declared scene with `cycle = false`**, `focus = "none"`, `shares_image =
  "art"`, and its own five spans (title, artist, year, counts, prompt) laid out over the card's boxes.
  Its spec surface is six element blocks and one registry field; its **pool cost is 0 tiles**
  (`text_distinct` is the face's atlas count, not a count of used characters, and the image is shared);
  and it keeps `paint_card` a pure function with no notion of transport or boot state. The alternative
  — a fifth span on the card, painted only while unstarted — is one element cheaper in the spec and
  costs a two-state composite in every snapshot plus an `if` in the one paint function that currently
  has no state at all. **What it does *not* buy** is the full-bleed boot screen: that still needs G9's
  backing strip, and it is now a change to `boot` alone rather than to the card as well.
- **The full-bleed "art + prompt" variant** additionally needs the backing strip G9 describes. `U9b`
  picks between the card-shaped boot scene now and that variant later.
- **The leash must know about this state (G16).** The loop's `idle` counter increments on every pass
  with nothing due, and `WEDGE_PASSES` fires after ≈ 43 minutes at the video cadence — so a prompt
  left on screen long enough prints `FAIL the ring wedged` and halts. "Waiting for `Start`" has to be
  progress: the leash is skipped while `Unstarted`, and the boot line says so, because an arm whose
  wedge bound was silently disabled is the same green-that-means-something-else this section refuses.
- **The chrome is up and inert.** The clock reads `0:00/total`, the bar is at cell 0, the play/pause
  icon shows the **play triangle** (`Paused`-equivalent: a press starts audio), and **no focus
  indicator is drawn** (R8).
- **The transport is not `Paused`.** `transport::State` gains an `Idle` state — never armed, no
  committed sample, nothing consumed — because `Paused` already means "a frozen chain whose ring head
  is the sample the freeze stopped on" (§4.4), and boot has no such head. `Start` from `Idle` runs the
  prefill (if C/A deferred it) + the existing prime path and enters `Playing`; the *resume* path is
  untouched.
- **No period is served, no clock ticks, no bar advances, no scene cursor moves.** The loop still
  waits on vblank (so the screen is stable and the sampler runs) and dispatches nothing but `Start`.
- **A discarded edge is discarded.** `A`/`B`/`Select`/D-pad/`L`/`R` edges are dropped and held
  directions are not allowed to autorepeat into the started state (§2.1). The BIOS chord still works.

### 7.3 The measurement this replaces, exactly

AP-1's term today is **"audio begins within 1.5 s of boot"**, and AP-1's own row already says what a
press-gated boot does to it: *"if 2a chooses (b), this term splits into an auto-start arm and a
Start-gated arm, and that belongs in this row's text at the moment the choice is made."* The choice is
now, in principle, made, so the flip's PR restates **three** terms, not one:

| term | instrument | ceiling |
|---|---|---|
| **boot → the prompt is on screen** (parse + one frame) | the existing boot ledger's `surface`/`parse`/`ui` legs, printed | ≈ 2 ms measured; a ceiling is the gate's to set, and it must not be a number nobody compares (the row's own lesson) |
| **`Start` → first audible sample** | a new ledger leg around the deferred stages (`open`+`gate`+`prefill`+prime under C), **plus the boot scene's exit** (below) | **1.5 s**, kept, because every placement above is inside it with ≥ 0.9 s of headroom under C |
| **the old unpressed "boot → audio"** | unchanged code path, now reached only on an auto-start build | either retired or kept on a **second arm** (a feature flag that arms at boot, run as a control), which is exactly what AP-1's row called "an auto-start arm and a Start-gated arm" |

The measurement half already exists — the ROM has printed its boot decomposition since PR-3a — so
this is a restatement plus one new leg, not a new instrument. **What must not happen:** the gate
silently keeps asserting the old unpressed term on a ROM that now waits for a press (that is a green
that measures a run the product no longer performs).

**One term the draft was missing, and it is the boot scene's fault (added 2026-11-28).** `Start` does
not only arm the chain: it **leaves the `boot` scene for the default scene**, and every scene entry
paints the whole region through the backing rule — `paint_blank` (480 cells) then the scene's own
content, which is L2's measured leg (**550,437 c mean, 917 c/cell**, ≈ 33 ms) plus the switch/commit
(212,386 c mean, 13 ms). The boot scene raises the same bill the card would have (the card is a scene
too), so this is not an argument between U15's two options — it is a term that belongs in the press
budget either way: **≈ 305 ms press→sound becomes ≈ 350 ms**, still far inside the 1.5 s ceiling but
not the "≈ 305 ms" §7.1 prints. It is paid once, at the press, and it is the one place the flip's cost
is *visible* to a person (the screen changes), which is also why the audio decode and the region paint
should be ordered so the paint does not sit between the press and the first decode pass.

### 7.4 One consequence for the existing gates

`album-test`, `album-card-test` and every replay script boot into a prompt now. They gain a `Start`
edge at frame 0 (`Replay::new(&[(0, BTN_START), …])` — expressible today), and
`album-test`'s verdict terms that depend on the chain being armed (`underflows`, `late`, the period
audit) now measure from the press. The card test's two `Select` edges become `Start`, then `R`/`L`.

**Until that PR lands, the journey's step 1 is "the ROM is already playing, `Start` pauses it".**
That is a one-line change to the expectation, not to the model: the model is identical whether audio
is armed at boot or at a press.

---

## 8. Gates

### 8.1 A new gate row: AP-10, "the focus model"

Proposed for §13 (none of its legs exist):

| leg | what it asserts | instrument |
|---|---|---|
| **F1** ring wrap | `Right` past the last chrome item is the first; `Left` past the first is the last; `n` presses return to the start | host test over `ui.rs` |
| **F2** one owner | every `Nav`/`Activate`/`Back` changes exactly one pane's state, and the chrome cannot consume a scene's action or vice versa | host test |
| **F3** transfer | `Up` from the chrome enters the scene only when `items() > 0`; `B` and the scene's edge both return to the **remembered** chrome item; `Down` in the chrome is a no-op | host test |
| **F4** state survives | focus in one pane, a scene cycle and back, cursor and remembered index unchanged; no allocation path exists | host test + a `no_std` link check |
| **F5** indicator rule | at most one indicator per frame; it is shape-bearing (the tile mask differs from its neighbours' by pixels, not palette); palette-inversion-only is refused | host test on the write plan, like AP-3's snapshot |
| **F6** cost | a focus move is 1–3 cells + at most one commit; a held direction's commit rate ≤ one per `REPEAT_INTERVAL_FRAMES`; the figures are recorded in `layout_report.txt` | probe/replay arm |
| **F7** the journey | the caller's journey is one replay script, and the ROM logs `focus: pane=… item=…` on every change; the gate greps the log, so the *sequence* is the witness and no screenshot is needed | `album-test`-shaped replay |
| **F8** refusals | every §6 refusal id fires by id on a spec written to be wrong about it | `album-layout-test`'s self-test |
| **F9** no regressions from the remap | audio is bit-identical (the input path is not the audio path), and no scene write leaves its rectangle | the existing `album-test` + AP-3 legs |
| **F10** the unstarted state | before `Start`: the `boot` scene is up (`scene=boot` in the log), no period is served, the clock does not move, no focus indicator exists, and **every** action except `Start` is discarded (including held-direction repeats); a `Start` edge at frame 0 starts the same run the gate had before | replay arm asserting `state=Idle`, `scene=boot`, `periods_served=0`, `focus=none` until the press |
| **F11** the split boot ledger | `boot → prompt` and `Start → first audible sample` are both printed and both inside their ceilings, on the arm that actually waits for the press, **including the boot scene's exit** (a full region entry, ≈ 45 ms) | `album-test`-shaped, one log line each |
| **F12** the `previous` rule | at `visit ≥ 2 s` the target is the **current** track at 0; at `visit < 2 s` it is the previous track, wrapping `0 → last`; a second activation after a restart therefore goes back a track; a press during a transition is ignored and counted | host test over the target choice + a replay leg that reads `track=`/`visit=` from the log |

### 8.2 What the new map breaks, and how each is re-baselined

| gate/arm | what breaks | disposition |
|---|---|---|
| `album-seam-test` (68+ host legs) | asserts §5.3's control map, `Cursor`, the chord rules and the debounce/repeat behaviour | re-write the map legs against `ui.rs`'s table; the *machine* legs (edges, repeats, chord) stay verbatim |
| `album-test` (`input-replay deep-audit`) | its script presses `Left`/`Right`/`Down` and expects track changes / restart / pause | re-script through focus: **two** `Right`s (`previous` → `play_pause`, because the first D-pad press only *enters*) then `A`; a focus walk for `next`/`previous`; a separate walk for the list. The audio assertions do not change |
| `album-card-test` | reaches the card with **two `Select` edges**; `Select` is unbound | use `R`/`L` (scene cycle); the assertion becomes "`R` reached the card, underflows=0" |
| `album-test`'s boot-flow terms | none today (audio still auto-starts) | §7's flip **splits AP-1's term into three** (prompt-visible, press→sound, and the unpressed arm as a control) in the same PR; F10/F11 are the new legs |
| every replay script | they boot a ROM that now waits at a prompt | a `Start` edge at frame 0; the card's `Select` edges become `Start`, then `R`/`L`. **And every script's first press must be a D-pad edge (2026-11-28):** the gate script opens with `A` at frame 8, and under R8 `A` has no focused item and is **ignored** — a re-scripted suite that kept it would be asserting a pause that never happened. The zero state is silent about it, so `F5`'s "the journey is the witness" needs the rule written down: *a script that presses a non-D-pad action before any D-pad edge is testing the discard path* |
| AP-3 snapshots / boot composite | the chrome row gains an indicator; the item order moves (`prev`/`play_pause`/`next`) | re-baseline with the layout change (G1) and record the new composite |
| AP-7a hardware legs | the old map was witnessed on a cart (**and its own legs name it**: "Left/Right skip … Down restarts the current track"); the new one has not been | needs a hardware session — and **not only to count presses**: the two things only a human can judge are whether the zero state reads as "nothing is focused yet" rather than as a broken chrome, and whether an `A` that activates a drawn `>` icon teaches the map or fights it (U17) |
| AP-4/AP-6 | the indicator's tiles/OBJ entries are new pool terms | §6's own refusal + AP-6's OBJ ceiling. **The BG side is pool-neutral** (§4.2: 9 tiles / 462 distinct after U4+G2+option A), so this leg is a declaration check, not a headroom check |
| `album_surface_host.rs`'s boot-frame composite | pins `writes.len() == 5` (one icon per slot) and the chrome row's exact cells `"SB><R"` — the **drawn order G1 is about**, in one string — plus a second-row assertion that the icon columns are band fill, which is exactly the cell the indicator caret wants | re-baseline with UI-0's re-layout: the string becomes the new three-item row and the second row gains one non-fill cell. `src/preview.rs`'s `ICON_*`→character map needs a caret arm or `the_composite_is_printable_for_a_human` breaks |
| `chrome.rs`'s const asserts | `ICON_TILES == CONTROL_SLOTS.len() + 1`, `FILL_TILES + ICON_TILES == POOL_CHROME_DECLARED`, `every_slot_has_icon()` over the declared slots, and `a_pause_swaps_the_start_slot…`, which *finds* `start` by scanning `CONTROL_SLOTS` and is therefore order-sensitive | both asserts follow the generator once `focus_art_tiles` exists; the slot-scanning test follows the re-layout |
| `album-layout-test`'s pinned counts | the Makefile greps `register rows fired: 9 of 9`, `budget rows fired: 2 of 2`, `scene schema legs fired: 4 of 4`, and `grep -c '  ok   <-' ≥ 120`; the Python side hardcodes `schema_fired != 4` | each new *layout* refusal row moves `9 of 9`; the three scene rules move `4 of 4` in **both** the Python and the Makefile; `FIXTURE_BASE` grows by the item table and the three scene fields, and every existing leg mutates that base, so its lines must stay stable |
| the boot scene (`boot`, `cycle = false`) | the declared scene count goes 4 → 5; `SceneId::IMPLEMENTED` must stay the *cycle* set (`art`/`title_card`/`blank`) so the host leg `the_scene_registry_is_the_spec_order…` gains a second assertion ("the boot scene is not in the cycle"); the ROM's boot line prints `scenes=N of M declared`, and `M` moves; `scene=` in every log gains a fourth value | one new `SceneId` arm in `paint_scene`, the `cycle` field + its refusal, five duplicated element blocks, and the boot composite as its own AP‑3 snapshot (the card's four-span composite is unchanged, which is the point of the split) |

---

## 9. Phasing

The work does not need a new phase; it lands inside the ladder that already exists, in this order.
**Re-cut at the 2026-11-28 review**, because the first draft's UI-1 bundled four unrelated kinds of
work (a spec schema, a generator change, an ISR contract change and a runtime dispatcher) and claimed
to be "the smallest thing that can be reviewed", and because no scene can take the focus until the list
lands (§3).

| step | what | depends on | existing home |
|---|---|---|---|
| **UI-0 — the schema** | `[[variant.item]]` (identity, verb, slot), the scene's `focus`/`enter`/`edge`/**`cycle`**, the indicator term, **G1's re-layout** (three items at 21/23/25, `select`/`restart` retired), and **the eleven refusals by id** in `album-layout-test`'s register (F8). No Rust, no tile, no audio | G1–G6, G8 | its own PR **before** PR-(5). It is genuinely the smallest reviewable unit — the diff is a TOML file, a register and its fixtures — and every step below needs it |
| **UI-1 — the focus core** | `ui.rs` (pane, action table, chrome ring, dispatch), **the ISR's delivery contract (§2.1, G13/G14)**, `L`/`R` scene cycle, `Select` unbound, the indicator (option A), **one commit site per pass**, AP-10's F1–F2/F5–F6/F13 | UI-0 | 2b's PR-(5) window (the presentation PR), or its own PR before it |
| **UI-2 — the scene pane and the list** | the `SceneState` enum and `items`/`enter`/`leave`/`handle`, `list` with cursor, scroll, now-playing mark, enter/leave; **F3, F7, F14** | UI-1; the list's geometry is already declared; PR-(6) already owns the list's content | 2b PR-(6), which already owns "scroll, cursor, now-playing mark" |
| **UI-3 — the boot flow** | `Boot::Unstarted` + `State::Idle`; the **`boot` scene** (five spans + prompt, `cycle = false`) is what is up; `Start` arms audio *and* enters the default scene; the leash condition (G16); the three AP-1 terms restated in the same PR, including the scene entry | UI-1 (focus); **U9a ruled: C** (validate at boot, defer the decode); **U15 ruled: its own scene** | its own PR (§5.2/§5.3 say so) |
| **UI-4 — the expensive indicator** | option C (sprite) if F6 says the commit rate is a problem | AP-3c/§5.7's chrome work | **inside** PR-(5), not after it — see below |

**The pane handoff and the scene pane are one step, and it is UI-2**, because `SceneId::IMPLEMENTED`
is `art`, `title_card`, `blank` today and **all three have no items** — a scene pane built before the
list has nothing to hand the focus to, so its ROM legs would be unreachable and its host legs would be
the only witness. F3 and F7 are therefore UI-2's, not UI-1's, and UI-1's ROM leg is the chrome ring
alone (which is fully testable: wrap, one-owner, zero state, the indicator, the commit count).

**UI-4's home moved (2026-11-28).** §5.7's chrome-as-OBJ work is where the OAM path, the VPAL bank and
AP-6's ceiling all get built; a focus sprite added *after* it is a second change to the same machinery.
So if option C is ever chosen, PR-(5) is the PR that should carry it — and because PR-(5) is also the
window the first draft gave UI-1, the two must not both claim it: **UI-1 goes before PR-(5), UI-4 is
PR-(5)'s, and the ladder is serial** (UI-0 → UI-1 → PR-(5) → UI-2/UI-4).

**UI-1 is still the smallest *runtime* step**: it changes no tile art, no audio and no scene content —
it changes *who reads a button*, adds one indicator cell and one commit site. UI-2 is the first step
whose behaviour the model has to generalise past one pane, and UI-3 is the only step that moves AP-1.

---

## 10. What this supersedes, and what it leaves alone

| # | passage | disposition |
|---|---|---|
| **C1** | §16: *"no widget tree, no runtime layout engine, **no focus model**, no stacking order anyone can change"* | **Amended, not deleted.** This spec's model is a declared, finite item table and one index per pane — no widget tree, no runtime layout, no reorderable stacking. The non-goal's *subject* (a general GUI toolkit) stands; the literal words "no focus model" are replaced with "no focus model beyond the declared item table (ALBUM-UI.md)". §16 is amended in the same commit that adds this file |
| **C2** | §5.3's control-map table (Start `\|` A, Right `\|` R, Left `\|` L, Down=restart, Up=none, Select=cycle, B reserved) | **Superseded for behaviour** by §2.1. The table is kept as the record of Phase 1's map, with a dated pointer here; the BIOS chord and the `WAITCNT` hold survive unchanged |
| **C3** | `keys.rs:156-175` (2026-11-18): "the D-pad navigates those screens … Right stops meaning next track … that layer belongs above this module" | **Confirmed and implemented.** This spec is that layer's contract; `keys.rs`'s `Action`/`Cursor` move up to `ui.rs` |
| **C4** | §5.2's scene table and §11/Q20: *"a scene is one rectangle and one function"* | **Amended**: one rectangle, one draw call, one handler, one state cell. The *rectangle* rule is untouched; "one function" was a statement about 2a's stateless scenes |
| **C5** | Q20 (ii): "the runtime scene-cycle order `Select` walks" | **Superseded on the button** (L/R), **kept on the order** (the declared registry order, filtered to the implemented set) |
| **C6** | Q20 (iii) / §5.3's 2026-11-25 answer: the card drops the prompt because audio auto-starts | **Reopened by §7, and U9 has since ruled** (approved in principle, placement C). What is *not* settled is the mid-album half: see **C16** and **U15** — the prompt is a boot-state element or it is a lie on the cycle path |
| **C7** | A11: "scroll + cursor + now-playing marker" | **Confirmed**, and its interaction half is now §3's contract: the cursor is the scene's, the pane handoff is the shell's |
| **C8** | §5.2's boot-flow diagram (`title_card → Start → default scene → Select → next scene`) | The **scene cycle's button** changes to L/R; the diagram is otherwise the target of §7 |
| **C9** | §5.3's "the two 'do the obvious thing' buttons are deliberately equivalent" | **Withdrawn** (no aliases, §2.4). `A` is contextual; `Start` is global |
| **C10** | §5.3's "Deliberately absent: anything requiring two buttons within a time window" | **Confirmed and extended** to navigation (§2.3) |
| **C11** | §5.1's OBJ line: "OBJ `\|` cursor, …" | **Confirmed** and given a budget question (§4.2, §6): a cursor sprite is legal, its OBJ entries must be declared |
| **C12** | AP-1: "audio begins within 1.5 s of boot", unpressed | **Approved in principle to change (2026-11-27):** §7's flip splits it into three terms — prompt-visible, `Start`→first sample, and the unpressed arm kept as a control — **in the same PR that lands the flip**. Until that PR the row stands exactly as written, and no gate may quietly re-point it at a ROM that waits for a press |
| **C13** | §5.3's `[[variant.control]]` key = button identity | **Decided (UI-0):** `[[variant.item]]` carries the identity, the verb and the slot; `control` is retired with `select`/`down` (`U17`). G3 records what the old key could not express |
| **C14** | The `state-word` debug feature and its nine cells | **Untouched, and it stays that way (ruled 2026-11-28, U12).** The strip is debug chrome for v1: no affordance legend and no focused-item name, so it buys nothing the caret does not already say. Nothing in this spec reads it |
| **C15** | §5.3's "**Down** = restart the current track … replaces the usual 'press Left twice quickly' chord on purpose (§4.4)", `keys.rs`'s "nothing is time-dependent", **and §4.4's own paragraph**: *"the control map uses no time-dependent chord — 'previous track if within 3 s of the start' is exactly the rule that would need the ledger, so it is replaced by a dedicated restart button"* | **Superseded in part (2026-11-27), with the reconciliation corrected 2026-11-28.** Restart is folded into `previous` as a **position-dependent** rule (§4.5). Two objections were in play and the first draft answered only the second: §5.3 refuses a *press-timing window* (this rule has none — one press, one decision), but **§4.4 refuses this exact rule** on the ground that it needs the **audible-position ledger** §4.4 declines to build. The rule does not need that ledger — it reads `paint.visit`, the same track-local position the clock and bar already display — but that is a narrower argument than "not the banned pattern", and **the reversal is a product ruling by the caller (U14), not a re-reading of §4.4**. §4.4's paragraph is annotated in place (2026-11-28) so no live contradiction remains on disk |
| **C16** | Q20(iii)'s "the card is in the cycle and **drops the prompt**", and §7.2's boot screen | **RESOLVED 2026-11-28 (U15): the card keeps no prompt, and the boot screen is its own scene.** Q20(iii)'s answer is untouched rather than reopened, and §7 gets a prompt by putting it where no cycle can reach it: `boot` is declared with `cycle = false`, so `L`/`R` cannot land on a `PRESS START` over a playing album. The fifth-span alternative — a prompt on the card, painted only while unstarted — is refused because it buys the same screen by *adding a condition* to the one paint function that has no state at all, and by giving every card snapshot two states |
| **C17** | §4.4's no-seek invariant for `Paused` ("the ring's head is the sample the freeze stopped on"), and the pause ledger's head-equality refusal | **Untouched for the ordinary resume; the one place it could move is U14's paused half.** The deferred-target option (recommended) keeps it exactly as written — the ring is replaced at the *resume*, which is the one moment a re-prime is sanctioned. The retarget-while-frozen option would amend it, and that is the price that makes it the second choice |

**Left alone deliberately:** every geometry constant, every pool term, every face, the audio path, the
transport's own contracts (fade/truncate/re-prime), the BIOS chord, the `WAITCNT` hold, and AP-1's
measurement until §7's own PR.

---

## 11. Open questions (numbered for the plan's register)

| # | question | recommendation, and why |
|---|---|---|
| **U1** | — | **Answered 2026-11-27 (the caller).** *No indicator at first*: the **zero state** (R8) — nothing owns the focus until the first D-pad press, which gives it to the **chrome on its first item** (`previous`). A scene can never be the first pane to take focus |
| **U2** | — | **Answered 2026-11-27 (the caller).** "Wrapping is a principle, but individual scenes are responsible for their own behaviour": the chrome is a ring (R2), its vertical axis is the pane transfer (R3), and a scene declares its own `edge` (R4) — **the list uses `exit_to_chrome`**, which is the journey, and `wrap` stays legal for a scene that wants it |
| **U3** | — | **Answered 2026-11-27 (the caller): remembered cursor wins.** R5/§1.6: a re-entry restores the scene's cursor; `enter` is what a **cold** entry (or a declared `first`) uses. The list declares `now_playing` for the cold case, which is the journey's "first element" when the album is at track 1 |
| **U4** | — | **Answered 2026-11-27 (the caller): retire the slot, the drawing and the action**, and roll restarting into `previous` (§4.5). Drawn set = focusable set = ring = three items; chrome term 10 → 9, B's pool sum 463 → 462. This is the CD-player model: the player assumes the last "track" was a mis-press |
| **U14** | The `previous` rule's two parameters: the dwell threshold, and what a transport item does while **paused** | **Answered 2026-11-28 (the caller): threshold 2 s, paused behaviour = defer.** 2 s (32,768 samples) is the upper end of "one or two seconds", so a deliberate press just after the start does not fight the rule. While paused the press moves the **painted** track to the target at `0:00` and the **resume** applies the audio — no frozen re-target, no amendment to §4.4's no-seek invariant, one `Option` and one branch (§4.5). It folds into U16, which is the same mechanism for every transport item |
| **U5** | What does `Select` become? | **Unbound and reserved for v1** (recorded, not merely unimplemented), because an unbound button is a promise and a bound one is a decision. Candidates for later: a settings/menu entrance (currently a §16 non-goal) or "toggle the now-playing pane" |
| **U6** | Does `L`/`R` cycle scenes while a **scene** holds the focus, or only from the chrome? | **Always global** (§2.1). It is the one thing a user must be able to do from anywhere, and the pane's remembered cursor survives the cycle (R5) |
| **U7** | One indicator sprite for both panes, or one per pane? | **One per pane.** A pane may only draw inside its own rectangle (§5.2), so a shared sprite would be the one exception to the backing rule — and it would have to be presented by the chrome's OAM path anyway. R6 is about what is *visible*, not about who owns the pixels |
| **U8** | Trait objects for scenes, or an enum? | **Enum.** No allocation, a bounded call graph, and a cost a probe can attribute. A trait object is the first step towards the widget tree §16 refuses |
| **U9** | Does boot gate audio behind `Start`? | **Approved in principle 2026-11-27 (the caller): yes** — an unstarted state where the starting scene and the chrome are up and nothing else happens, "including not letting the user navigate the UI". §7 is its spec. Two sub-decisions remain: **U9a** where the line sits (recommended **C**: validate at boot, defer the prefill to the press, ≈ 305 ms from press to sound), and **U9b** the boot screen. Neither changes the model, only the payload |
| **U9a** | — | **Answered 2026-11-27 (the caller): C.** Boot validates the album (`open` + `gate`) and draws the prompt; the **prefill is deferred to the press** (≈ 305 ms from `Start` to the first sample), so no audio is decoded before the user asks and a `PRESS START` is never shown over an album that then refuses |
| **U9b** | The boot screen: the `title_card` with a prompt, a **separate boot scene**, or full-bleed art plus a prompt with a backing strip | **Answered 2026-11-28 (the caller): its own scene.** `boot`, `cycle = false`, `focus = "none"`, `shares_image = "art"`, five spans (title/artist/year/counts/prompt) over the card's boxes. It costs **0 pool tiles** (`text_distinct` is the face's atlas count; the image is shared) and buys the thing the fifth-span route cannot: Q20(iii)'s "no prompt over a playing album" is enforced by the *registry* instead of by an `if`, and `paint_card` stays a pure function. Full-bleed remains a later change to `boot` alone (G9's backing strip) |
| **U10** | Does the list's row highlight invert the row's palette bank, or draw a marker? | **Marker + luminance**, because a bank inversion re-writes 88 cells per move and is hue-only (R7). The marker can be a sprite (cheap) or a cell in a dedicated column (the row numbering column is the natural home) |
| **U11** | When a scene's `handle` says `Ignored`, may the shell pass the action to the chrome? | **No** (R1). `Ignored` is for "understood, nothing to do"; a scene that cannot use `Left` says so by having no horizontal axis, and the shell does not fall through |
| **U12** | Is the debug word's strip (9 cells) the long-term hint line, i.e. does the `state-word` flip now have a *reason* to happen? | **Answered 2026-11-28 (the caller): keep it debug-only.** No affordance legend (U17) and no focused-item name — "I don't think we need that extra UI information, but can always resurrect it later". So the strip stays behind `state-word` for v1, the caret is the whole of the chrome's state signal, and resurrecting a name later is a feature-flag flip rather than a schema question |
| **U13** | — | **Answered 2026-11-27 (the caller): the first menu item, for every direction.** Entering the chrome with no previous focus always lands on `previous`; the pressed direction is not consulted. `A`/`B` from the zero state are still ignored — no focused item to activate, no pane to leave |
| **U15** | **The boot prompt: a fifth span on the card, or a scene of its own?** (raised 2026-11-28; C16) | **Answered 2026-11-28 (the caller): a scene of its own — `boot`, `cycle = false`.** What the fifth span would have bought is *one element instead of six*; what the scene buys is the rule enforced by the registry rather than by an `if`, a `paint_card` that stays stateless, and a boot screen that can later go full-bleed without touching the card. **`cycle = false` is a per-scene declared field, not a boot-specific flag** (confirmed 2026-11-28: the scene says it should not be in the `L`/`R` toggle), and the boot scene **reuses the card's renderer** with the prompt as an optional span so there is one copy of the drawing rules; only the element boxes are duplicated, and that is still under the caller's review. The scene's real costs are named rather than waved at: a `cycle` field + two refusals (UI-0), five duplicated element blocks, `SceneId::Boot` kept *out* of `IMPLEMENTED`, one registry-count assertion, and one scene entry at the press (§7.3). Pool cost: **0 tiles** |
| **U16** | **What a transport-reaching item does while the transport is `Paused`** (G17) | **Answered 2026-11-28 (the caller): it works like it does while playing.** *"Not being able to switch tracks while paused is a deficiency … going to the next track or the previous track should work similarly to while music is playing."* So `next`, `previous` and `A` on a list row all re-target, and the transport **stays `Paused`** on the new track at `0:00`; the audio is deferred to the resume (U14's mechanism, generalised to `{kind, index}`). The display half is named in §4.5: the title line, the clock, the bar and the list's now-playing mark all read the pending target, or the chrome shows the old title over the new `0:00`. **This is a UI-2 prerequisite** because it decides what `A` on a row means |
| **U17** | **Does the chrome still read as a button legend?** The drawn row was five *button* icons; after the remap they are three *items* activated by `A`, and `L`/`R` cycle scenes | **Answered 2026-11-28 (the caller): three icons, no affordance text, salience from the focus indicator.** *"Nothing on the screen needs to tell the user to press A … so long as focus indicators are salient. The other UI affordances can't be focused and so naturally won't be interactable."* So: `previous`, `play_pause`, `next` and nothing else; the ring is **spaced** (21/23/25) so a caret under one item is unambiguous; and the dismissal of the `L`/`R`/`B` drawings follows from the focus model itself — an unfocusable thing needs no drawing to say it cannot be activated. AP-7a's human leg still judges whether the caret is salient *enough* |
| **U18** | **Does the scene pane need a throwaway one-item scene so F3/F7 can be witnessed on the ROM before the list exists?** | **Answered 2026-11-28 (the caller): no, and not in a product ROM at all.** *"We can always build a one-off ROM with strange scene implementations / variants for testing purposes. Anything esoteric like that should not make its way into final ROMs, which should be configurable to not include them."* That mechanism already exists twice: a `product = false` variant (§5.0) and a gate-only build. So §9's answer stands — the pane handoff ships with the list in UI-2, F3 is a host leg over `SceneState` until then — and a synthetic scene, if someone wants one, is a **non-product variant's** scene and never a registry entry in the shipped variant |

---

## 12. Non-goals this model keeps

- No widget tree, no runtime layout engine, no arbitrary stacking, no z-order a scene can change.
- No nested menus, no modal dialogs, no pointer, no scrolling sub-windows inside a scene.
- No per-screen key maps; one button→action table, and panes interpret actions.
- No pointer-driven focus, no tab order, no focus traversal outside the two panes.
- No new transport verbs: every `activate` in the chrome is an existing, measured path.
- No allocation, no `Vec`, no recursion in dispatch.
