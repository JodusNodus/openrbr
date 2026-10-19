# Ground (CGround, CGround::CDef, the ground probes): physics semantics

Pass 4 of the physics semantics brief, ground part. Notes only; no source edits.
Confidence: `known` (exe string, LSP key, or the equation is unambiguous), `inferred`,
`guess`. Asm: `/opt/homebrew/opt/llvm/bin/llvm-objdump`. `-` in the status column = no row in
`notes/progress/gameplay-functions.csv` (not harness-verified). reccmp % from `base/RBR.txt`.

Related: `entities.md` (the CEntity base, its ctor and child list), `collision.md` and
`contacts.md` (the solver and detector that consume the probe results).

## 1. Overview

The ground is a **single static node** in the entity tree. It is a `CGround : CEntity`
(vtable 0x73cf1c, 0x64 bytes, 87 slots plus 10 overrides). It carries no triangle mesh of its
own. Its geometry is the LSP `Ground/GROUND` block (`notes/formats/physics-lsp.md:246`:
`Height int[32768]`, `Left int[-16384]`, `Top int[-16384]`, `Width int[32768]`). The actual
surface queries go to a **runtime host object** whose pointer is in the global `g_7c755c`
(`RBR_Globals.h:118`, "runtime-set"; its owner is E01 per `RBR_E20.h:117`). Its slot 0 is the
box/segment query `S00(a, b, out)` that `CGround::VfE8` calls. The host's other entries
(`E39_SegTest70` 0x4ff070, `E38Ops::M_EBC0` 0x4febc0, `E39_OpenC90`/0x4ffc90) are the
collision-grid queries the contact code uses (`col.md:327`, `col.md:194`). Who installs the
host, and what object it is, is **open** (Open 1).

Creation. `CGround::CDef::Vf50(a, b)` (slot 20, 0x51c810) deletes the previous ground (the
singleton at `0x01659268`), then `new CGround(def = this, x = a, y = b)`. The ground's flags
are `0x80` (CGround ctor, `RBR_E43_GroundA.cpp:121`: `CEntity((unsigned)a, b, c, 0x80)`).
The ground def is created from the LSP block by the registry factory `CGround::CDef::New9`
(0x51baa0, LSP name `Ground/GROUND`, `physics-lsp.md:294`).

Place in the step. The ground is not in `list450`/`list480` (those are CSolid and wall lists,
`step.md` 1c); it is queried on demand, by the wheel probe and by the CPhysics detectors
(`col.md:204`, `FUN_0053d520`, and `E46` `Vf10`). Its only per-step member that the physics
step touches is `Vf88`/`Vf110`/`Vf114`/`VfCC` (return 1, "ground present"), which are
exact.

Call tree (reached functions, addresses):

```
CGround::CDef::New9(p)                          0x51baa0  56.6%  -    factory (LSP "Ground")
  CGround::CDef::CDef(p)                        0x51b990  72.0%  -    E43EntityDef(p) + members
CGround::CDef::Vf50(a, b)                       0x51c810  54.2%  -    replaces singleton, new CGround
CGround::VfE8(a, b, c, d, e)                    0x51c6f0  43.3%  -    box probe (slot 58) -> host S00
  E33Mat34::FetchX(t, g_7cbb00) x2              0x51b6c0  100%   verified
  host S00(t60, t70, out)                        (g_7c755c vtable slot 0; owner open)
  E01Vec4::CopyFrom(d)                          0x43c5b0
  E33Mat34::ComposeA? (not called here)
E33Mat34::CollideInit(a, b, c)                  0x51bd30  17.8%  verified   ground contact grid
  E38/E39 host queries 0x4ff070, 0x4febc0, 0x473c90, E36AllocEvent
CGround::Vf88 / Vf110 / Vf114 / VfCC            0x51bb60.. 100%  -    return 1
CGround::CDef::Vf4c                             0x51ba20  50.0%  verified   name "DefGround" (0x73cf0c)
CGround::CDef::Unk20 / Unk1C                    0x51bb00 / 0x51bb10  50.0% / -  return 0x165926c
CGround::BoxBA0 / Vf11C (min/max box)           0x51bba0 / 0x51bb90  -      box from the host or [this+8]
```

Inputs: the LSP `Ground` block (the four integers), the host query object (`g_7c755c`), the
two transforms of the query (`a`, `b`: two `E33Mat34` objects, `FetchX` extracts their
transforms). Outputs: a contact record (13 floats, `d`) and the host hit flag.

## 2. Data

### 2a. CGround::CDef (0x15c bytes; vtable 0x73ceac; base `E43EntityDef`, 0x20 bytes)

| offset | current C++ name | proposed name | type | unit | meaning | confidence | evidence |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 0x00 | vtable | vtable | ptr | - | `CGround::CDef` (24 slots) | known | `RBR_E43.h` (CDef vtable 0x73ceac) |
| 0x04 | name (CBase) | name | ptr | - | the def name (`DefGround`, from `Vf4c`) | known | `Vf4c` 0x73cf0c |
| 0x11c | `CInteger` | ground_left? | i32 | model units | first integer of the pair (order 0x11c, 0x12c). LSP key `Left` or `Top`: order not confirmed | guess | `CDef` ctor `RBR_E43_GroundA.cpp:12-16`; `physics-lsp.md:246` |
| 0x12c | `CInteger` | ground_top? | i32 | model units | second integer | guess | same |
| 0x13c | `CUnsigned` | ground_width? | u32 | model units | first unsigned (`Width` or `Height`, range 0..32768 in LSP) | guess | same; LSP `Width` `int[32768]` |
| 0x14c | `CUnsigned` | ground_height? | u32 | model units | second unsigned | guess | same |

Size 0x15c (`New9`, `operator new(0x15c)`). The four members are the four `GROUND` keys. The
ctor passes no names (`new (…) NTweak::CInteger()`), so the key names are bound elsewhere
(not read). Two things are not settled: which key is at which offset, and the unit (the LSP
gives integer ranges, not metres). The LSP ranges themselves (Left/Top ±16384, Width/Height
up to 32768) are the only evidence for signs (Left/Top can be negative, so they are the
`CInteger`s). Open 2.

`DtorBody` (0x51ba50) demotes the four members with the `0x72e444` vtable and zeroes the word
after each vtable (`+0x120`, `+0x130`, `+0x140`, `+0x150`). Those word offsets are the CBase
member layout for these members, not the NTweak value offset used by `CSolid` (`rigid-body.md`
2a, value at +0xc). Not checked further (Open 3).

### 2b. CGround (`this`, CEntity 0x64 bytes)

Inherits the CEntity layout (`entities.md` 2b). Additional fields: none read. The `this+8`
box read by `BoxBA0` is the CEntityFrame `frame_depth` (`entities.md` 2a), which the box code
reads as a pointer to `float[6]` (centre xyz and extent xyz). That is a type-confusion in the
C++ read, see 5b.

### 2c. The contact record (VfE8 output, 13 floats, `d`)

Built from the host's `out` (13 floats). Layout in the asm (`out` base = `d` source; `o` = the
record):

| record | source | meaning | confidence |
| --- | --- | --- | --- |
| `o[0..3]` | `out[0..3]` (`CopyFrom`) | a 4-float block (point or plane) | inferred |
| `o[4..7]` | `out[4..7]` | a second 4-float block | inferred |
| `o[8]`, `o[9]`, `o[10]` | `out[10]`, `out[11]`, `out[9]` | a 3-vector (order as the asm writes it) | inferred (5a) |
| `o[11]`, `o[12]` | 0, 0 | cleared | known (asm) |
| `o[13]` | `out[12]` | a scalar (distance or weight) | guess |
| `o[14]` | `this` (CGround) | owner pointer | known (asm) |

The record is 15 words, not 14: `o[14]` is the owner.

## 3. Functions

| address | current name | proposed name | what it computes | confidence | status and evidence |
| --- | --- | --- | --- | --- | --- |
| 0x51c6f0 | `CGround::VfE8` | ground_box_probe | `t60 = FetchX(a)`, `t70 = FetchX(b)`; `hit = host.S00(t60, t70, out)`; if hit: `rec = out` (see 2c), `rec.owner = this` | inferred (structure) | `-`, 43.3%. **Output index mismatch: suspect 5a** |
| 0x51b6c0 | `E33Mat34::FetchX` | - | copies a transform's X column (not read here) | known | verified, 100% |
| 0x51bd30 | `E33Mat34::CollideInit` | ground_grid_collide | builds an `E37GridOps` grid from the two transforms (`M6610`), runs `M_D760X` per grid edge and `0x4febc0` as fallback, allocates `CCollisionEvent`s (`E36AllocEvent`, `InitFull` kinds 4/5), samples the grid (`SampleA/SampleB`), clears (`E38RayClear`) | inferred | drive-verified; 21.3%; miss-path call shapes fixed from asm; contact-hit path remains unverified |
| 0x51baa0 | `CGround::CDef::New9(p)` | ground_def_new | `operator new(0x15c)`, construct `CDef(p)` | known | `-`, 56.6%; C++ read |
| 0x51b990 | `CGround::CDef::CDef(p)` | ground_def_init | `E43EntityDef(p)`; vtable 0x73ceac; two `CInteger`, two `CUnsigned` (2a) | known (code) | `-`, 72.0%; C++ read |
| 0x51c810 | `CGround::CDef::Vf50(a, b)` | ground_make | if `g_1659268` set: `old.Vf00(1)`, clear it; `g = new CGround(this, a, b)`; store `g` at `0x1659268`; return `g` | known (asm-shape from C++; not asm-checked) | `-`, 54.2%; the p2ra "cached-ground" note applies (`p2ra-final.md:26`) |
| 0x51bb00 / 0x51bb10 | `Unk20` / `Unk1C` | - | return `0x165926c` (twin pointer, not physics) | known | verified (Unk20 50%) |
| 0x51ba20 | `CGround::CDef::Vf4c()` | def_name | returns `"DefGround"` (0x73cf0c) | known | verified, 50% |
| 0x51bc30 | `CGround::CDef::Vf54(t)` | def_clear_global | if `t`: call `t.S80()`; clear `g_1659268` | inferred | `-`, C++ read |
| 0x51bb60 / 0x51bb70 / 0x51bb80 / 0x51bc70 | `Vf88`, `Vf110`, `Vf114`, `VfCC` | ground_present | return 1 | known | 100%, `-` |
| 0x51bb90 | `CGround::Vf11C(a, b)` | box_from_host | `mn = c - e; mx = c + e` from `g_7c755c+8` (the host's centre/extent pairs) | inferred | `-`; not reached |
| 0x51bba0 | `CGround::BoxBA0(a, b)` | box_from_this | same box from `this+8` | inferred | `-`; not reached |
| 0x51bb30 | `CGround::VfFC` | forward | calls `CEntity::VfFC` | known | `-`, 100% |

## 4. Open questions

1. **The host object at `g_7c755c`.** Every probe goes through it. Its class, its slot 0
   (`S00`) implementation, and who sets it are not identified here (E01 owns the global per
   `RBR_E20.h:117`). Settles it: the runtime value of `g_7c755c` and its vtable (a peek), and the
   E01 setter.
2. **Which `GROUND` key is which offset.** The four `CInteger`/`CUnsigned` members at
   `0x11c`/`0x12c`/`0x13c`/`0x14c`. The LSP lists `Height`, `Left`, `Top`, `Width` (alphabetical,
   not the storage order). Settles it: the key name strings the registry binds to each member
   (the `NTweak` key table in the exe, or a `physics.lsp` load with a known Ground).
3. **Member value offsets.** `DtorBody` writes zeros at member+4 and the vtable at member+0.
   `rigid-body.md` 2a puts the NTweak value at member+0xc. Which word is the integer value of
   each `GROUND` member? Settles it: one `NTweak::CInteger` value read in the asm.
4. **Units.** Are `Left`/`Top`/`Width`/`Height` in metres, centimetres or grid cells? The
   LSP gives only integer ranges. Settles it: the consumer of `g_7c755c+8` (box) and the value
   of a loaded `GROUND` block.
5. **`E33Mat34::CollideInit` shape.** The miss-path call shapes are checked against the asm
   and match the drive after the fixes in 5d. The contact-hit path remains open.

## 5. Suspect code

### 5a. `CGround::VfE8`: the output buffer is offset by three floats (asm-confirmed)

**Fixed in c5910713** (the `result + 3` shift is gone; GH1 checked the C++ against 0x51c741-0x51c79c line by line, 2026-10-14). Verified over the drive (54,340 calls), but the hit path (S00 nonzero) never runs on tanner-corolla, so that branch is asm-checked only. The text below describes the old bug.

- C++: `src/rbr/RBR_E43_GroundB.cpp:96-99` (`result[0..2] = first[1..3]`; `S00(t60, t70,
  result + 3)`), and the scatter at lines 100-112 (`o[8] = result[9]`, `o[9] = result[10]`,
  `o[10] = result[11]`, `o[13] = result[12]`).
- Why inconsistent: the host writes its 13 floats through the pointer passed as the third
  argument. The C++ passes `result + 3`, so the host's floats land at `result[3..15]`. The C++
  then reads `result[0..3]` for `CopyFrom`, which holds `first[1..3]` (the input transform's
  components, not the host's output) plus one host float. Those are not the host's output.
- What the asm does: `0x51c724`-`0x51c73b` push `out = esp+0x30` as the third argument; the
  `CopyFrom` at `0x51c741`-`0x51c74b` reads `out[0..3]` from the same `esp+0x30`; the
  scatter at `0x51c76d`-`0x51c797` writes `o[8] = out[10]`, `o[9] = out[11]`, `o[10] = out[9]`,
  `o[13] = out[12]`, `o[11] = o[12] = 0`, `o[14] = this`. The asm never writes `first[1..3]`
  into the output. So:
  - `CopyFrom` source should be `out[0..3]` (not `result[0..3]` with the shift);
  - the host output pointer should be `result` (not `result + 3`);
  - the scatter indices differ from the C++ (`o[8]` from `out[10]`, not `out[9]`).
- Effect: the record the wheel probe reads is wrong for every hit (slot 58 is called by the
  CEntity walks). The function is not harness-verified (`-`). Route to the E43 owner: fix the
  out pointer and the three scatter indices after reading the `out` mapping in the asm
  (`0x51c76d`-`0x51c785`). Confidence: known (the asm offsets are read directly; the mapping of
  `esp` offsets was checked by tracing the pushes).

### 5b. `CGround::BoxBA0` / `Vf11C` read the frame depth as a box pointer (inferred)

- C++: `RBR_E43_GroundB.cpp` (`BoxBA0`): `float* src = *(float**)((char*)this + 8);`.
- Why inconsistent: `this+8` is `frame_depth`, an integer (`entities.md` 2a), not a pointer to
  a box. Dereferencing a small integer crashes unless the function is never reached.
- What the asm does: not read. `BoxBA0` and `Vf11C` are not in `called.csv` (not reached), so
  this is a latent bug, not a live one. Mention only.

### 5c. `CGround::VfF0` and `Vf100` read uninitialised locals (inferred)

- C++: `RBR_E43_GroundB.cpp` (`VfF0`, `Vf100`): `int w[7]` and `t50` are never written before
  they are copied to the output (`w[0..6]`).
- Why inconsistent: the outputs are copies of uninitialised stack. Not reached (not in
  `called.csv`), so no live effect. Keep as a note; the owner should not rely on these
  bodies until they are rewritten from asm.

### 5d. `E33Mat34::CollideInit` uses the filled boxes and body flag at +0x60 (asm-confirmed)

Fixed in `RBR_E43_GroundC.cpp` from the assembly at `0x51bd73-0x51bda8`,
`0x51bdb5-0x51bdd6`, and `0x51bf5e-0x51c0dc`:

- Slot 72 fills the two stack boxes that the segment gate then reads. The old C++ passed different local boxes to the gate.
- Slot 12 receives `g_7cbb00`; slot 70 receives slot 12's result. The original leaves each stack argument in place for the next call and `M6610`; the C++ now passes the same values and balances the two `retl` calls explicitly.
- The second grid pass is selected by bit 3 of the byte at `bodyA + 0x60`. The old C++ tested `bodyA + 0x18`, so it skipped the pass.

The reference drive takes the second pass: the recorded calls at steps 40 and 1295 include 22 gate passes, 11 with bit 3 set. Tag `f2-final` stayed identical over all 12,084 steps (21,420 `CollideInit` calls). The changed copy also replayed identically over step 40 (70 calls). The C++ first-pass-hit path replayed identically on fuzz seed 7. Seed 4 reaches a first-pass hit but differs in the event's fourth hit-vector lane: `M_D760X` leaves that lane untouched, and the original and C++ stack layouts contain different prior values. The copy then faults after the event path; seed 4 is not verified. Second-pass contact arithmetic remains open. The 21.29% reccmp score reflects the approximate frame and local layout.

## Notes for the lead

- **5a** is the one to route: the ground box probe (slot 58) hands the wrong floats to every
  caller, and the function is not harness-verified.
- **5d**'s gate boxes, slots, and second-pass test now follow the asm. The drive miss paths and
  fuzz seed 7 replay pass; the seed 4 first-pass hit still exposes the uninitialized output-lane
  difference described above. Full-drive verification is recorded in the worker report.
- The ground is queried, not simulated. Physics checks against it go through the host
  (`g_7c755c`); the representation of the ground's geometry is therefore the host's, not the
  `CGround` node's (Open 1).
- 5b and 5c are unreached; flag them so nobody wires them in unread.
