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
  compile-time. `restart`'s disposition — the fifth drawing that has no place in a three-item ring —
  is U4, and it is the one product decision this ruling leaves.
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
| **R4 — a scene owns its ring** | A scene's navigation is its own: 1-D rows, 2-D grid, or nothing. Whether its edges wrap or leave is **declared per scene** (`edge = "wrap" | "exit_to_chrome"`), because the list's journey wants "one past the end → back to the chrome" while a grid would want "wrap". The default is `exit_to_chrome`; `wrap` is legal and must be declared. |
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
  track from the beginning" — which is the transport's own seek-to-track path, not a new one.

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
(G4). `restart` is **not** a ring item; U4 is its disposition (retire the drawing and the action, or
declare a fourth item — the schema takes N, the ring's length is not hard-coded).

**The current drawn order is wrong for this model and has to move.** Today the slots are declared in
button order — `select@21, start@22, right@23, left@24, down@25` — and the drawings are
`diamond, play/pause, '>' (next), '<' (previous), restart`. So "next" is drawn **left of** "previous":
`Right` from `play_pause` would move to "next" and then to "previous", which reads backwards. The
item model wants `previous | play_pause | next` left to right (the caller's journey assumes it). This
is a *layout* change (slot positions in `layout_spec.toml`), not a code change, and it is G1.

### 4.2 The indicator

The chrome band is two tile rows; the control drawings are one tile tall and live on the upper row, so
**the lower row is free under every item**. That is the cheapest place for an indicator and it is the
default proposal:

| option | how | cost per focus move | verdict |
|---|---|---|---|
| **A. underline caret** (default) | one authored 8×8 tile in the band's *lower* row, under the focused item's cell | 1 cell write (489 c) + one commit path | lands now; cheap; shape-bearing; no OBJ |
| **B. bracket/frame** | 2–3 authored tiles around the item | 2–3 writes | same class |
| **C. sprite** | 1 OAM entry over the item, presented by the existing OAM shadow | ~226 c/entry, no map copy, no foreground commit | the §5.7-aligned option; needs an **OBJ tile budget** declared (AP-6 owns that ceiling today, and no BG-side `tile_budget` can see it) |
| **D. palette inversion only** | recolour the item's cell(s) | 1 write | **refused by R7** (hue alone) |

Whichever is chosen, **the per-move cost is a commit, not the cells**: agb has no partial-map commit,
so a one-cell indicator change pays the foreground's whole 2,048 B map on the fast path
(12,222 c, §13 AP-3c arm A) and up to 280,360 c when a vblank has not elapsed. At repeat cadence
(134 ms ≈ 8 vblanks) the fast path is what a held direction sees. The spec's budget rule is therefore:
**at most one commit per pass, and a held direction must not raise the commit rate above one per
`REPEAT_INTERVAL_FRAMES`** — which the existing `paint.due()` latch already enforces structurally.

### 4.3 The freed cells, and the L/R affordances

Two layout facts follow from this model:

- **The `select` slot has nothing to draw.** `Select` is unbound (§2.1) and the scene cycle moved to
  `L`/`R`. The diamond at column 21 should either become an `L`/`R` affordance pair or be retired,
  and its tile returned to the pool (G2).
- **The debug word's nine cells are the natural hint strip.** `keylabels` is 9 cells at columns
  12–20 and is debug chrome behind `state-word` (it disappears when the flip happens). The moment it
  goes, the chrome has nine cells for `L`/`R`/`A`/`B` affordances or a focused-item name — which is
  what a focus model wants and what the word is currently occupying. That is a *reason to schedule*
  the flip, not a reason to rush it (§5.3's ruling says it is its own ruling).

### 4.4 What the chrome does on activation

`A` on `previous`/`next` is the transport's existing skip path; on `play_pause` the existing faded
toggle (§4.4); on `restart` the existing restart. **No new transport behaviour is introduced by this
spec** — the verbs already exist and are already measured (pause: action→freeze 2 periods, measured
frame 45→49 ≈ 67 ms; skip: ≤ ~0.6 s; both §4.4/R13). What changes is only *how they are reached*.

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
```

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

**And one meaning change that is not a new field:** `[[variant.control]] key` currently names a
**button** (`select`/`start`/`right`/`left`/`down`). Under this model the chrome's focusable things are
**actions**, so either `control` gains an `action` and keeps `key` as the button it *illustrates*, or
`item` replaces it (G3). The current five keys also cannot express `A`, `B`, `L`, `R`, which this map
needs as affordances.


### 6.1 The gap register — what the layout plan does not have yet

The list below is the answer to "where are the gaps in the layout effort" as a checklist, so a later
PR can tick a line instead of rediscovering it. **None of these exist today**; the first five are
blocking for UI-1, the rest for UI-2/UI-3.

| # | gap | where it bites | closed by |
|---|---|---|---|
| **G1** | **The drawn order is the button order, and it disagrees with the focus order.** Slots are `select@21, start@22, right@23, left@24, down@25`, and the drawings are `diamond, play/pause, '>' (next), '<' (previous), restart` — next is drawn **left of** previous, so a ring over them moves backwards | §4.1, the journey's `Right Right` | re-order the slots in `layout_spec.toml` + `item-order-not-slot-order` |
| **G2** | **The `select` slot has no item** (`Select` is unbound) and there are **no `L`/`R`/`A`/`B` affordance slots at all** — the five declared slots are the old button map | §4.3, the chrome row | retire the diamond; declare the L/R pair (the freed 9 cells are the option) |
| **G3** | **`[[variant.control]] key` names a button, not an action**, so the schema cannot express "this is the `next` item" apart from "this is where the Right button's icon sits"; `A`/`B`/`L`/`R` are not expressible | §1.2, §6 | `[[variant.item]]` (this section) |
| **G4** | **No declared focus order.** Nothing ties the item order to the slot geometry, so a ring can be built backwards and no gate can see it | §4.1, F1/F5 | declared order + `item-order-not-slot-order` |
| **G5** | **The scene schema is tiles-only**: `kind`/`resident`/`shares_image` say what a scene *draws*, and nothing says whether it can take the focus or how it enters/leaves | §1.6, §3, UI-2 | `focus`/`enter`/`edge` fields + three refusals |
| **G6** | **No indicator term in any budget.** Chrome tiles (10) and the OBJ ceiling (AP-6's) both have no focus-indicator line, so either option in §4.2 would be an *unaudited* tile | §4.2, R14's rule | `focus_art_tiles` (BG) or `obj_focus_*` (sprite) + `focus-indicator-over-budget` |
| **G7** | **Scenes are functions, not stateful objects.** `SceneId` is an enum and `surface.rs` paints each scene from `(album, scene)`; there is no place a cursor or a scroll offset could live | §1.7, §3 | the `scene/` structs + the `SceneState` enum (U8) |
| **G8** | **No item-count declaration, and there must not be one for `list`** — the count is `ALBM1`'s `n_tracks`. The schema has `min_visible_rows` (a floor), not an item count, and the audit must not start reading one | §1.2, §6 | stated here; `scene-items-not-derived` refuses the temptation |
| **G9** | **No prompt/hint element, and the art scene has no text element.** The `title_card` has four spans and an 80×80 picture; the caller's boot screen is cover + "PRESS START" | §7, U9 | U9's ruling; a declared span (card) or a backing-strip element (full-bleed) |
| **G10** | **No gate row for interaction.** AP-0…AP-9 name probes, phases and budgets; nothing names focus, wrap, ownership or state survival | §8.1 | AP-10 (proposed) |
| **G11** | **The existing replay scripts encode the old map** (`replay-script-a/b/c`, the `album-test` script, the card's two `Select` edges) | §8.2 | re-script through focus (UI-1/UI-2) |
| **G12** | **No §10.2 budget row for a navigation move or a list scroll step.** The element rows price a bar cell, a clock digit, a title line and a region repaint — the two costs a focus model actually spends are both missing | §5.3, F6 | one row each, measured by F6's arm |


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
- **The screen** is the declared boot scene: the cover and one prompt line. v1 can be the existing
  `title_card` with its prompt span restored (it already draws a picture and four spans); the
  full-bleed "art + prompt" variant needs the backing strip G9 describes. `U9b` picks between them.
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
| **`Start` → first audible sample** | a new ledger leg around the deferred stages (`open`+`gate`+`prefill`+prime under C) | **1.5 s**, kept, because every placement above is inside it with ≥ 0.9 s of headroom under C |
| **the old unpressed "boot → audio"** | unchanged code path, now reached only on an auto-start build | either retired or kept on a **second arm** (a feature flag that arms at boot, run as a control), which is exactly what AP-1's row called "an auto-start arm and a Start-gated arm" |

The measurement half already exists — the ROM has printed its boot decomposition since PR-3a — so
this is a restatement plus one new leg, not a new instrument. **What must not happen:** the gate
silently keeps asserting the old unpressed term on a ROM that now waits for a press (that is a green
that measures a run the product no longer performs).

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
| **F10** the unstarted state | before `Start`: no period is served, the clock does not move, no focus indicator exists, and **every** action except `Start` is discarded (including held-direction repeats); a `Start` edge at frame 0 starts the same run the gate had before | replay arm asserting `state=Idle`, `periods_served=0`, `focus=none` until the press |
| **F11** the split boot ledger | `boot → prompt` and `Start → first audible sample` are both printed and both inside their ceilings, on the arm that actually waits for the press | `album-test`-shaped, one log line each |

### 8.2 What the new map breaks, and how each is re-baselined

| gate/arm | what breaks | disposition |
|---|---|---|
| `album-seam-test` (68+ host legs) | asserts §5.3's control map, `Cursor`, the chord rules and the debounce/repeat behaviour | re-write the map legs against `ui.rs`'s table; the *machine* legs (edges, repeats, chord) stay verbatim |
| `album-test` (`input-replay deep-audit`) | its script presses `Left`/`Right`/`Down` and expects track changes / restart / pause | re-script through focus: `Right`→`play_pause`, `A`; a separate focus walk for the list. The audio assertions do not change |
| `album-card-test` | reaches the card with **two `Select` edges**; `Select` is unbound | use `R`/`L` (scene cycle); the assertion becomes "`R` reached the card, underflows=0" |
| `album-test`'s boot-flow terms | none today (audio still auto-starts) | §7's flip **splits AP-1's term into three** (prompt-visible, press→sound, and the unpressed arm as a control) in the same PR; F10/F11 are the new legs |
| every replay script | they boot a ROM that now waits at a prompt | a `Start` edge at frame 0; the card's `Select` edges become `Start`, then `R`/`L` |
| AP-3 snapshots / boot composite | the chrome row gains an indicator; the item order moves (`prev`/`play_pause`/`next`) | re-baseline with the layout change (G1) and record the new composite |
| AP-7a hardware legs | the old map was witnessed on a cart; the new one has not been | needs a hardware session (the count of presses to reach a track changes, and that is exactly what a human must judge) |
| AP-4/AP-6 | the indicator's tiles/OBJ entries are new pool terms | §6's own refusal + AP-6's OBJ ceiling |

---

## 9. Phasing

The work does not need a new phase; it lands inside the ladder that already exists, in this order:

| step | what | depends on | existing home |
|---|---|---|---|
| **UI-1 — the focus core** | `ui.rs` (pane, action table, chrome ring, dispatch), the chrome item declarations, L/R scene cycle, `Select` unbound, the indicator (option A), AP-10's F1–F5/F8 | §4.2's indicator term; §6's spec fields | 2b's PR-(5) window (the presentation PR) or its own PR before it |
| **UI-2 — the list** | `list` scene with cursor, scroll, now-playing mark, enter/leave; the journey's F7 arm | UI-1; the list's geometry (already declared) | 2b PR-(6), which already owns "scroll, cursor, now-playing mark" |
| **UI-3 — the boot flow** | `Boot::Unstarted` + `State::Idle`; `Start` arms audio from the prompt scene; the three AP-1 terms restated in the same PR | UI-1 (focus); **U9a ruled: C** (validate at boot, defer the decode) | its own PR (§5.2/§5.3 say so) |
| **UI-4 — the expensive indicator** | option C (sprite) if F6 says the commit rate is a problem | AP-3c/§5.7's chrome work | after PR-(5) |

**UI-1 is the smallest thing that can be reviewed**: it changes no tile, no audio and no scene
content — it changes *who reads a button* and adds one indicator cell. UI-2 is the first scene whose
behaviour the model has to generalise past one pane, and UI-3 is the only step that moves AP-1.

---

## 10. What this supersedes, and what it leaves alone

| # | passage | disposition |
|---|---|---|
| **C1** | §16: *"no widget tree, no runtime layout engine, **no focus model**, no stacking order anyone can change"* | **Amended, not deleted.** This spec's model is a declared, finite item table and one index per pane — no widget tree, no runtime layout, no reorderable stacking. The non-goal's *subject* (a general GUI toolkit) stands; the literal words "no focus model" are replaced with "no focus model beyond the declared item table (ALBUM-UI.md)". §16 is amended in the same commit that adds this file |
| **C2** | §5.3's control-map table (Start `\|` A, Right `\|` R, Left `\|` L, Down=restart, Up=none, Select=cycle, B reserved) | **Superseded for behaviour** by §2.1. The table is kept as the record of Phase 1's map, with a dated pointer here; the BIOS chord and the `WAITCNT` hold survive unchanged |
| **C3** | `keys.rs:156-175` (2026-11-18): "the D-pad navigates those screens … Right stops meaning next track … that layer belongs above this module" | **Confirmed and implemented.** This spec is that layer's contract; `keys.rs`'s `Action`/`Cursor` move up to `ui.rs` |
| **C4** | §5.2's scene table and §11/Q20: *"a scene is one rectangle and one function"* | **Amended**: one rectangle, one draw call, one handler, one state cell. The *rectangle* rule is untouched; "one function" was a statement about 2a's stateless scenes |
| **C5** | Q20 (ii): "the runtime scene-cycle order `Select` walks" | **Superseded on the button** (L/R), **kept on the order** (the declared registry order, filtered to the implemented set) |
| **C6** | Q20 (iii) / §5.3's 2026-11-25 answer: the card drops the prompt because audio auto-starts | **Reopened by §7, as a product question** (U9). Until U9 rules, the answer stands |
| **C7** | A11: "scroll + cursor + now-playing marker" | **Confirmed**, and its interaction half is now §3's contract: the cursor is the scene's, the pane handoff is the shell's |
| **C8** | §5.2's boot-flow diagram (`title_card → Start → default scene → Select → next scene`) | The **scene cycle's button** changes to L/R; the diagram is otherwise the target of §7 |
| **C9** | §5.3's "the two 'do the obvious thing' buttons are deliberately equivalent" | **Withdrawn** (no aliases, §2.4). `A` is contextual; `Start` is global |
| **C10** | §5.3's "Deliberately absent: anything requiring two buttons within a time window" | **Confirmed and extended** to navigation (§2.3) |
| **C11** | §5.1's OBJ line: "OBJ `\|` cursor, …" | **Confirmed** and given a budget question (§4.2, §6): a cursor sprite is legal, its OBJ entries must be declared |
| **C12** | AP-1: "audio begins within 1.5 s of boot", unpressed | **Approved in principle to change (2026-11-27):** §7's flip splits it into three terms — prompt-visible, `Start`→first sample, and the unpressed arm kept as a control — **in the same PR that lands the flip**. Until that PR the row stands exactly as written, and no gate may quietly re-point it at a ROM that waits for a press |
| **C13** | §5.3's `[[variant.control]]` key = button identity | **Needs a schema decision** (§6, G3): an item's identity is not a button |
| **C14** | The `state-word` debug feature and its nine cells | **Untouched** (its own flip ruling). §4.3 names the cells as the hint strip that frees when it flips |

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
| **U4** | `restart`'s disposition: the caller's ruled ring is exactly three items, but a fifth drawing (a return glyph) is already authored and its action exists | **Recommend retiring it for v1** — drop the slot, the drawing and the action, so the drawn set, the focusable set and the ring are the same three things (chrome term 10 → 9, B's pool sum 463 → 462). The alternatives are a **fourth ring item** (contradicting the stated order) or a drawn-but-unreachable icon (an item only `A` could reach, which no gate can explain). The action is recoverable later by declaring a fourth item — the schema does not hard-code three |
| **U5** | What does `Select` become? | **Unbound and reserved for v1** (recorded, not merely unimplemented), because an unbound button is a promise and a bound one is a decision. Candidates for later: a settings/menu entrance (currently a §16 non-goal) or "toggle the now-playing pane" |
| **U6** | Does `L`/`R` cycle scenes while a **scene** holds the focus, or only from the chrome? | **Always global** (§2.1). It is the one thing a user must be able to do from anywhere, and the pane's remembered cursor survives the cycle (R5) |
| **U7** | One indicator sprite for both panes, or one per pane? | **One per pane.** A pane may only draw inside its own rectangle (§5.2), so a shared sprite would be the one exception to the backing rule — and it would have to be presented by the chrome's OAM path anyway. R6 is about what is *visible*, not about who owns the pixels |
| **U8** | Trait objects for scenes, or an enum? | **Enum.** No allocation, a bounded call graph, and a cost a probe can attribute. A trait object is the first step towards the widget tree §16 refuses |
| **U9** | Does boot gate audio behind `Start`? | **Approved in principle 2026-11-27 (the caller): yes** — an unstarted state where the starting scene and the chrome are up and nothing else happens, "including not letting the user navigate the UI". §7 is its spec. Two sub-decisions remain: **U9a** where the line sits (recommended **C**: validate at boot, defer the prefill to the press, ≈ 305 ms from press to sound), and **U9b** the boot screen. Neither changes the model, only the payload |
| **U9a** | — | **Answered 2026-11-27 (the caller): C.** Boot validates the album (`open` + `gate`) and draws the prompt; the **prefill is deferred to the press** (≈ 305 ms from `Start` to the first sample), so no audio is decoded before the user asks and a `PRESS START` is never shown over an album that then refuses |
| **U9b** | The boot screen: the `title_card` with its prompt restored, or full-bleed art plus a prompt with a backing strip | **The card first** — it already draws a picture and four spans, so the flip lands without a new element; the full-bleed variant needs G9's backing rule and is a separate layout PR |
| **U10** | Does the list's row highlight invert the row's palette bank, or draw a marker? | **Marker + luminance**, because a bank inversion re-writes 88 cells per move and is hue-only (R7). The marker can be a sprite (cheap) or a cell in a dedicated column (the row numbering column is the natural home) |
| **U11** | When a scene's `handle` says `Ignored`, may the shell pass the action to the chrome? | **No** (R1). `Ignored` is for "understood, nothing to do"; a scene that cannot use `Left` says so by having no horizontal axis, and the shell does not fall through |
| **U12** | Is the debug word's strip (9 cells) the long-term hint line, i.e. does the `state-word` flip now have a *reason* to happen? | **Yes, but not in this PR.** §4.3 records the dependency: the hint strip is the thing the flip buys, and the flip is still its own ruling (§5.3) |
| **U13** | — | **Answered 2026-11-27 (the caller): the first menu item, for every direction.** Entering the chrome with no previous focus always lands on `previous`; the pressed direction is not consulted. `A`/`B` from the zero state are still ignored — no focused item to activate, no pane to leave |

---

## 12. Non-goals this model keeps

- No widget tree, no runtime layout engine, no arbitrary stacking, no z-order a scene can change.
- No nested menus, no modal dialogs, no pointer, no scrolling sub-windows inside a scene.
- No per-screen key maps; one button→action table, and panes interpret actions.
- No pointer-driven focus, no tab order, no focus traversal outside the two panes.
- No new transport verbs: every `activate` in the chrome is an existing, measured path.
- No allocation, no `Vec`, no recursion in dispatch.
