# Entity and frame layer (E43 CFrame, CEntityFrame, CEntityOldFrame, CEntity): physics semantics

Pass 4 of the physics semantics brief, entity part. Notes only; no source edits.
"Current" names are the C++ reconstruction's. "Proposed" names are ours.
Confidence: `known` (exe string, key, or the equation is unambiguous), `inferred` (strong
argument from use or asm), `guess`. Asm: `/opt/homebrew/opt/llvm/bin/llvm-objdump` on
`RichardBurnsRally_SSE.exe`. Status comes from `notes/progress/gameplay-functions.csv`
(`-` = no row, i.e. not harness-verified) and the reccmp % from `base/RBR.txt`.

Scope: the frame hierarchy, the CEntity object (fields, ctor, child/sibling list, exclusion
set, walks). The ground (`CGround`) is in `ground.md`. The hinged and mounted bodies are in
`jointed-bodies.md`. Matrix helpers (`E43PairWalk`, `E33Mat34`) are not read here, except
where a reached function calls them.

## 1. Overview

The E43 "entity" is the scene-graph node that physics bodies sit on. Three classes:

- `CFrame` (0xc bytes, abstract, 7 pure slots, vtable 0x73ce4c): a parent pointer and a depth.
- `CEntityFrame : CFrame` (0x10 bytes, vtable 0x73cb64): adds `m_sub0C`. Its slots 1-6
  forward to the `m_sub0C` object when the depth is non-zero, and return their first argument
  either way.
- `CEntityOldFrame : CFrame` (0x10 bytes, vtable 0x73cb84): the same shape, but its slots
  gate on the owner's slot 32 (`S80`) and call slot 39/41 (or 40/42, 43/45 ...) on it.
- `CEntity : CEntityFrame` (0x64 bytes, vtable 0x73ccbc, 87 slots): the node. It embeds a
  `CEntityOldFrame` at `+0x1c`, a named `CState` (a `CBase`) at `+0x38` and a `CScratch`
  (vtable only) at `+0x5c`. `CBody`, `CBrakeWall`, `CGround` and the E34 wall/car classes
  derive from it.

Place in the step. The tree is walked by name-less walks that call a slot on each *gated*
child (`S80() != 0`) and recurse. The step's per-entity slots S20, S24, S48, S5C, S64, S6C,
S58 and S98 (`step.md` 1c) are the same walks on the bodies: on a `CSolid` they reach
`CEntity::E43Vf20`, `E43Vf58`, `E43Vf98` and so on. The walk order is pre-order: a node is
visited, then its subtree.

Frames carry no transform. Nothing in `CFrame`, `CEntityFrame` or `CEntityOldFrame` stores a
matrix or pose. The frame slots return their input (`a`) and, in `CEntity`, the hooks they
forward to (slots 39-50) also return their input. The parent-relative pose, if any, is
therefore not held in this layer; it lives in the body (`CSolid` orientation and position,
`rigid-body.md` 2b). Open 1.

Call tree of the reached entity functions (addresses; reccmp % and status):

```
CEntity::CEntity(def, x, y, flags)              0x51a640  38.2%  -      (ctor, 2a/2b)
  CEntityFrame::CEntityFrame(this, y)           0x5194b0  71.6%  -
    CFrame::CFrame(y->Vf30())                   0x51af70  92.3%  verified
    E43_Get08(y) (= y->+8)                      0x51a8d0  -       (identity read)
  CEntityOldFrame::CEntityOldFrame(this, y)     0x519650  71.6%  -
  CState init (CBase::CBase(name))              0x52f090  -       (CBase, not ours)
CEntity::~CEntity()                             0x51a810  -       (unlink, dtor)
CEntity::Vf00() (name)                          0x519950  87.5%  verified
CEntity::E43Vf20(p) (walk, slot 8)              0x51a3a0  100%   -
CEntity::E43Vf150(p) (child append, slot 84)    0x51a370  89.7%  -    5a resolved (no bug)
CEntity::E43_ExclF80(p) (exclusion test)        0x519f80  26.3%  -
CEntity::E43_ExclA000 / ExclA060 (set alloc/remove) 0x51a000 / 0x51a060  100% / -
CEntity::E43_Walk1F0(functor) (pre-order walk)  0x51a1f0  100%   -
CEntity::Vf40() (name walk, slot 16)            0x51a4b0  63.2%  -
CEntity::E43Vf98() (unlink, slot 38)            0x51a7c0  100%   -
CEntity::E43Vf158() (host notify, slot 86)      0x519aa0  100%   -
CEntity::E43_Cmp9A0(p) (order key)              0x5199a0  100%   -
CEntityOldFrame::Vf04..Vf18 (slot 1-6)          0x5196d0.. 32.1%  -
CEntityFrame::Vf04..Vf18 (slot 1-6)             0x519530.. 100%   verified
```

Inputs and outputs. Inputs: the owner's `CDef` (the `def` argument, `+0x34`), the parent
entity (`+0x44`), and the child list (`+0x48`). Outputs: the walks deliver slot calls to
children. The exclusion set (`+0x2c`) is used by the contact detectors (`contacts.md`,
`E43_ExclF80` gates `E46L2Detector::Vf04`).

## 2. Data

### 2a. CFrame and CEntityFrame (`this`, 0x10 bytes; CEntity uses the same offsets)

| offset | current C++ name | proposed name | type | unit | meaning | confidence | evidence |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 0x00 | vtable | vtable | ptr | - | 0x73ce4c (CFrame), 0x73cb64 (CEntityFrame), 0x73ccbc (CEntity) | known | ctors |
| 0x04 | `m_unk04` | frame_parent | ptr | - | parent frame: the parent entity (`y->Vf30()`, which returns `this`), or the identity object `g_7cbb00` for a root | known (code: 0x51af70, 0x5194b0) | `RBR_E43_Frame.cpp:17`; asm 0x51af70 |
| 0x08 | `m_unk08` | frame_depth | int | count | parent's depth + 1 (`p[8]+1`); 1 for a root under the identity object; 0 for `CFrame(null)` before `CEntityFrame` sets it | inferred (depth) / known (arithmetic) | asm 0x51af83-0x51af87; `RBR_E43_Frame.cpp:20-24` |
| 0x0c | `m_sub0C` | frame_owner | ptr | - | for `CEntity`, the entity itself (the `x` argument, `this`); for the old frame, the owning `CEntity` | known (asm 0x5194b0 stores arg1 = this) | `RBR_E43_Frame.cpp:31` |

`+8 != 0` is the gate for every frame slot: 0 means "no frame", nonzero means "attached".

### 2b. CEntity (`this`, 0x64 bytes)

Offsets are the C++ names' offsets, checked against the ctor asm (0x51a640-0x51a6f3).

| offset | current C++ name | proposed name | type | unit | meaning | confidence | evidence |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 0x10 | `E34Link10` (m_next) | sib_next | ptr | - | next sibling in the parent's child list (`E43_Walk1F0` follows it) | known (asm 0x51a37a-0x51a392; UnlinkA 0x4e41a0) | `RBR_E43_EntityB.cpp:383-390` |
| 0x14 | `E34Link10` (m_prev) | sib_prev | ptr | - | previous sibling (set on append) | known (asm, UnlinkA) | `RBR_E43_EntityB.cpp:383` |
| 0x18 | `E34Link10` (m_list) | sib_list | ptr | - | the parent's child-list header (`parent+0x48`), or 0 when unlinked | known (asm) | `RBR_E43_EntityB.cpp:389` |
| 0x1c | `CEntityOldFrame` | frame_old | struct (0x10) | - | embedded old frame (vtable 0x73cb84); `Vf34` returns this address | known | `RBR_E43_Frame.h:56` |
| 0x2c | `m_arr2c` | excl_set | ptr | - | `[0]` = count n, `[1..n]` = CEntity pointers; null until `ExclA000` allocates it | known (ExclF80/ExclA060 read it) | `RBR_E43_EntityC.cpp:224-300` |
| 0x30 | `x` (ctor arg 2) | sort_key | f32/int | unknown | ordering key of `E43_Cmp9A0` (compared before the address). Meaning not known; set once in the ctor | guess | ctor store at 0x51a6dd; `Cmp9A0` (EntityB:417) |
| 0x34 | `flags` (ctor arg 1) | def | ptr | - | the entity's definition (a `CBase`); `Vf98` calls slot 21 on it; the ctor reads its name (`GetNameOrNull`) | known (asm: name from arg1) | ctor 0x51a640 |
| 0x38 | `CState` (CBase) | state | struct | - | the named state sub-object (vtable 0x73cc6c): `+0x04` name, `+0x0c` (aliases CEntity +0x44), `+0x20` owner (this) | known | ctor 0x51a6b4-0x51a6d0 |
| 0x3c | (CBase name, `m_name`) | name | ptr | - | `CBase` name; `Vf00` returns it or `"(nomen luni)"` | known | `RBR_E43_EntityA.cpp:11` |
| 0x44 | `m_parent` | parent | ptr | - | the parent entity (ctor arg 3, `y`), or 0 at top level. Walks and `ExclF80` recurse through it; `E43Vf90/94/158` notify the host when it is 0 | known (asm 0x51a6a8-0x51a6c6) | ctor; `E43Vf90` |
| 0x48 | `m_list48` head | child_head | ptr | - | first child (walks start here) | known (asm 0x51a37a-0x51a38d, and walks) | `E43Vf150`, `E43_Walk1F0` |
| 0x4c | `m_list48` tail | child_tail | ptr | - | last child (used to append) | known (asm) | `E43Vf150` |
| 0x50 | `m_list48` count | child_count | int | count | number of children (`++` on append, `--` in the dtor and `E43Vf98`) | known (asm 0x51a395; dtor) | `E43Vf150`, `E43Vf98` |
| 0x54 | - | - | int | - | zeroed by the ctor, not read in this pass | unknown | ctor 0x51a6bd-0x51a6c3 |
| 0x58 | `m_self58` | owner_self | ptr | - | `this`, stored for the `CState` owner | known (asm 0x51a6cd) | `RBR_E43_EntityC.cpp:41` |
| 0x5c | `CScratch` | scratch | struct (vtable 0x73cbb4) | - | scratch sub-object; `E43Vf54` tail-jumps to its slot 1 (`Vf04`, no-op) | known (asm 0x51a6d3, 0x519ca0) | `RBR_E33_Body.h` |
| 0x60 | `m_flags60` | flags | u32 | bits | the caller's `def` argument (arg 4) with bit 0 set. For CBody this is `(flags | 2) | 1`, so bit 1 comes from CBody and bit 0 is always 1 | known (asm 0x51a6da-0x51a6e1) | `RBR_E43_EntityC.cpp:44` |

### 2c. CEntity::CState (0x24 bytes, `this+0x38`)

| offset (CState) | proposed name | meaning | confidence |
| --- | --- | --- | --- |
| 0x00 | vtable 0x73cc6c | CBase | known |
| 0x04 | name | the CBase name (`GetNameOrNull` of the def) | known |
| 0x0c | parent | the same word as CEntity `+0x44`. The header's name `m_def0C` is misleading | known (asm 0x51a6c6) |
| 0x10-0x1c | zeros | not read here | unknown |
| 0x20 | owner | CEntity `this` | known |

### 2d. Exclusion set (`+0x2c`)

`arr[0]` is the count, `arr[1..n]` the CEntity pointers. `ExclA000(n)` allocates `n` words
and zeroes the count. `ExclA060(v)` removes `v` by shifting and decrements the count.
`ExclF80(other)` returns 0 when the parent's test returns 0, or when `other == this`, or when
the smaller of the two sets (by count) holds the other side: if `|self| < |other|` it looks
for `other` in `self`'s set, else for `self` in `other`'s set. It returns 1 otherwise. A
null set on either side gives 1 (the `if (a != 0 && b != 0)` guard). Either set may hold
the pair, so this is a membership check in whichever set is smaller (asm 0x519fa7-0x519fbd).

## 3. Functions

| address | current name | proposed name | what it computes | confidence | status and evidence |
| --- | --- | --- | --- | --- | --- |
| 0x51af70 | `CFrame::CFrame(p)` | frame_init | `frame_parent = p`; `frame_depth = p ? p.depth + 1 : 0` | known | verified; 92.3%; asm 0x51af70-0x51af92 |
| 0x5194b0 | `CEntityFrame::CEntityFrame(x, ent)` | frame_entity_init | `CFrame(ent->Vf30())`; `frame_owner = x`; `frame_parent = ent->Vf30()`, `depth = ent.depth + 1`; with no ent: `depth = 1`, `frame_parent = g_7cbb00` | known | 71.6%; asm 0x5194b0-0x5194f9 matches (two `Vf30` calls, `E43_Get08` on the parent) |
| 0x519650 | `CEntityOldFrame::CEntityOldFrame(x, ent)` | old_frame_init | as above with `ent->Vf34()` (the old frame of the parent) | known | 71.6%; asm 0x519650-0x519698 |
| 0x519530 | `CEntityFrame::Vf04` | frame_hook_1 | if `depth != 0`: `frame_owner->E43Vf9C(a, b)`; returns `a` | known | verified; 100% |
| 0x5196d0 | `CEntityOldFrame::Vf04` | old_hook_1 | if `depth == 0` return `a`; else `if owner.S80() == 0: owner.E43Vf9C(a,b) else owner.E43VfA4(a,b)`; returns `a` | known (asm 0x5196d0-0x51970d) | 32.1%, `-`; asm-checked above |
| 0x519ca0 | `CEntity::E43Vf54` | scratch_tail | tail-jump to `scratch->Vf04` (no-op) | known | 100%, `-` |
| 0x51a370 | `CEntity::E43Vf150(p)` | child_append | append `p` to the child list: `p.parent_link = tail; p.next = 0; if tail: tail.next = p else head = p; tail = p; p.list = &head; count++`. The head is written only when the list was empty (asm and `RBR_E43_EntityB.cpp:385-388` agree; 5a resolved) | known (asm) | 89.7%, `-` |
| 0x51a1f0 | `CEntity::E43_Walk1F0(functor)` | walk_children_pre | for each child `c` in order: if `c.S80() != 0`: `functor(c)`, then `c.E43_Walk1F0(functor)` | known (asm) | 100%, `-` |
| 0x51a3a0 | `CEntity::E43Vf20(p)` | walk_s20 | for each child with `S80() != 0`: `child.E43Vf20(p)` | known (asm 0x51a3a0-0x51a3cf) | 100%, verified path in step.md |
| 0x51a250 / 0x51a290 / 0x51a2c0 / 0x51a2f0 / 0x51a330 / 0x51a440 | `E43Vf58`, `E43Vf5C`, `E43Vf64`, `E43Vf68`, `E43Vf6C`, `E43Vf48` | walk_sNN | same gated walk, calling the same slot on each child (`E43Vf6C` first calls `Vf40` on self) | known (C++ shape, same as Vf20) | 100%; `E43Vf58` is the walk in step.md |
| 0x51a4b0 | `CEntity::Vf40()` | name_walk | `state.Unk44()` (CBase slot 17 on the state), then `E43_Walk1F0` with functor `E43_Tail40` (calls `child.Vf40()`) | known (asm 0x51a4b0-0x51a4f3) | 63.2%, `-`; asm-checked |
| 0x519f80 | `CEntity::E43_ExclF80(p)` | excl_test | `0` if the parent's test returns 0; else `0` if `this == other`, or if the smaller of the two sets contains the other side (search the other entity in this set when `|this.excl| < |other.excl|`, else this entity in the other set); else `1` | known (asm 0x519f80-0x519fe8 read) | 26.3%, `-`; semantics match, shape differs |
| 0x51a000 | `E43_ExclA000(n)` | excl_alloc | allocate `n` words, count 0, if not yet allocated | known | 100%, `-` |
| 0x51a060 | `E43_ExclA060(v)` | excl_remove | remove `v` from the set | known | `-`, C++ read |
| 0x5199a0 | `E43_Cmp9A0(p)` | order_less | `a < b` ordering: gated nodes first; then by `sort_key` (+0x30) if both non-zero; else by address | inferred | 100%, `-` |
| 0x51a7c0 | `CEntity::E43Vf98()` | unlink_slot38 | `Vf00()`; if `sib_list`: `--count` and `UnlinkA`; then `def.E43Vf54(this)` (slot 21 on the def) | known (asm 0x51a7c0-0x51a7e3) | 100%; the def call is a vptr call |
| 0x51a810 | `CEntity::~CEntity()` | entity_dtor | demote to `CEntity`, free the excl set, scratch demote, `state.DtorBody()`, then if `sib_list`: `--count` and `UnlinkA` | known (C++ read, asm 0x51a810-0x51a84b not read) | 46.8%, not in `called.csv`; `-` |
| 0x51a530 | `E43EntityDef::E43EntityDef(name)` | def_init | `CBase(name)`; zero `+0x0c..+0x18`; vtable 0x73cc04 | known (asm) | 92.9%, `-` |
| 0x51a560 | `E43EntityDef::~E43EntityDef` | def_dtor | release `+0x14` (`--refcount`, unlink at `+0x0c`); demote to CBase | known (asm 0x51a560-0x51a589) | 45%, `-`; asm-checked above |
| 0x52f090 / 0x52f0b0 | `CBase::CBase(name)` / `GetNameOrNull` | - | name holder (NTweak, not ours) | known | GetNameOrNull: `p ? p.name : 0x739520` (asm 0x52f0b0) |

Notes on the child list and the ctor: the walks above need the list to be complete. The ctor is correct
in the store set (asm 0x51a640-0x51a6f3 matches every store in the C++), only its
parameter names are swapped (2b, 0x34/0x60).

## 4. Open questions

1. **Is there a transform on the entity at all?** The frame and hook slots return their
   input. If a transform exists, it is in a subclass (E33 `CBody`, `CSolid`, E45 `CNullFrame`)
   or in the `E43Mat*` helpers. `CNullFrame` (E45, vtable 0x73f5dc, `0x5236a0`) reuses the
   `0x51af70` init (per `RBR_E43_Frame.h`), and its slots are not read here. Settles it: read
   the `CNullFrame` slots and every class that overrides slot 0-6 or 39-50.
2. **The `sort_key` at +0x30.** `ctor` stores the second argument (`x`, from CBody's `c`).
   `E43_Cmp9A0` orders by it. What is `c` in `CBody`? Settles it: the `CBody` call site.
3. **Which classes make `S80` return nonzero?** `CEntity::E43Vf80` returns 0. Only the gate
   overrides decide whether a child is walked. Settles it: grep the `S80`/slot-32 overrides in
   E33/E34/E38/E45 (not read here).
4. **Who appends children besides the car?** `E43Vf150` has one caller in `src/rbr`
   (`RBR_E34_CarA.cpp:199`, the wheel add). Settles it: the other `E43Vf150`/slot-84 callers
   in the original (the tree may have more than wheels).
5. **Meaning of `+0x54` and `+0x58`.** Zeroed (`+0x54`) and self-stored (`+0x58`) by the ctor,
   not read in this pass. The `+0x58` self pointer is the `CState` owner, so it is probably a
   back-link for the state. Settles it: readers of `+0x54` in the reached code.
6. **Identity object `g_7cbb00`.** The root frame's parent. Its `+8` depth and contents are not
   read. The depth of a root (1) relies on it. Settles it: dump `0x7cbb00` (read-only peek).

## 5. Suspect code

### 5a. `CEntity::E43Vf150` head write: resolved, no bug

- The asm (`0x51a37a`-`0x51a38d`) writes the head (`this+0x48`) only when the old tail is null; otherwise it writes
  `tail.next` and leaves the head. `RBR_E43_EntityB.cpp:385-388` has the same `else`, so the C++ matches the asm.
- The B3 vectors agree: four load-time appends to one parent, two of them onto a non-empty list, keep the head
  (`src/rbr` is not changed). An earlier note said the C++ wrote the head on every append; that was wrong.

### 5b. CEntity ctor parameter names are swapped in the header (naming only)

- C++: `src/rbr/RBR_E33_Body.h:136` (`CEntity(unsigned flags, void* x, void* y, NTweak::CBase* def)`)
  and `RBR_E43_EntityC.cpp:17`.
- Why it reads wrong: the asm's arg 1 (the C++ `flags`) is the def (name source and `+0x34`),
  and arg 4 (the C++ `def`) is the flags word (`+0x60 = arg4 | 1`). The C++ body uses the
  right roles (`GetNameOrNull((CBase*)flags)`, `+0x34 = flags`, `+0x60 = def | 1`), so the
  behaviour is right. The header comment already says so (`RBR_E43_EntityC.cpp:12-14`).
- Fix: rename the parameters to `(def, x, y, flags)`. No behaviour change.

### 5c. Frame hooks are identity (no bug)

- C++: `RBR_E43_FrameSlots.cpp:23-70` and `CEntity::E43Vf9C..E43VfC8` (`EntityA.cpp:303-380`).
- The hooks call slot 39-50 on the owner and return the first argument. Every target returns its
  first argument (`E43Vf9C` to `E43VfC8`), so the net effect is "return `a`". Not a bug (the asm
  matches), but a reader should not expect a transform here (Open 1).

## Notes for the lead

- 5b is naming only; it can wait for a batch rename.
- Still open for the next pass: `CGround` (`ground.md`), `CHingedRigidBody` and `CMountedRigidBody`
  (`jointed-bodies.md`), and the E43 `EngineSim` family (not a frame concern; see `jointed-bodies.md`
  or the lead's choice).
