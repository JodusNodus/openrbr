# E45 physics core (mounted-body update, pair-walk call, shake payload): physics semantics

Short companion to `jointed-bodies.md` and `contacts.md`. Notes only; no source edits.
Confidence: `known` (asm-checked, or an unambiguous equation), `inferred`, `guess`.
Asm: `/opt/homebrew/opt/llvm/bin/llvm-objdump`. `-` = no row in
`notes/progress/gameplay-functions.csv` (not harness-verified). reccmp % from `base/RBR.txt`.

Scope: the E45 functions the reference drive reaches that are not already covered by the other
notes. The materials (`CMaterial`), the L1/L2 detectors and `E45_M40C0`/`E45_P33F0` are in
`contacts.md` (verified there). The passenger and hinged bodies are in `jointed-bodies.md`.

## 1. What is covered here

The E45 functions that are on the reached path and not yet verified, and the one that is
harness-`bad`:

| address | current name | proposed name | status | this note |
| --- | --- | --- | --- | --- |
| 0x521980 | `E45_M1980` | mounted_update_pose | `-`, 32.0% | 2a, 3 |
| 0x524250 | `E45PhysBody::M4250` | pair_walk_sd4 | bad, 55.2% | 4 (call order checked) |
| 0x523210 | `E45_P3210` | incar_shake_emit | `-`, 42.3% | in `jointed-bodies.md` 3 |
| 0x5229e0 / 0x522bc0 / 0x523360 | passenger smalls | - | `-` / `-` / `-` | `jointed-bodies.md` |
| 0x5215a0 / 0x521480 / 0x521c30 / 0x521d20 | mounted ctors and Vf50 | - | `-` | not read in depth |

## 2. Data

### 2a. Mounted body (`CMountedRigidBody`, `this`, 0x590 bytes; see `jointed-bodies.md` 2c)

| offset | proposed name | type | unit | meaning | confidence | evidence |
| --- | --- | --- | --- | --- | --- | --- |
| 0x34 | def | ptr | - | `CMountedRigidBody::CDef` (not the owner) | known | `E45_M1980` `movl 0x34(%esi)` |
| 0x3d0 | owner | ptr | - | the car (`CCar*`); `Xform6A30` is called on it | known (asm 0x5219d2) | `E45_M1980` |
| 0x140 | position | vec (E01Vec4) | m | world position of the mount point (copy of `def+0x220`) | known (asm 0x5219dd) | `E45_M1980` |
| 0x330 | pose_end_pos | vec | m | copy of `0x140` (end-of-window position, same slot as CSolid 0x330) | inferred | `E45_M1980` (asm 0x5219ec) |
| 0xf0 / 0x100 | orientation | vec4 | - | the quaternion object (data at 0x100, as CSolid) | inferred | `E45_M1980` (`src = this+0x100`) |
| 0x1d0 (CBase), 0x1dc | active_flag | int | bool | `0x1dc = (**(this+0x580) != 0)`; `SetDirty` on the base at 0x1d0 first | known (asm 0x521986-0x5219af) | `E45_M1980` |
| 0x580 | flag_ptr | ptr to int | bool | the pointer whose target is the enable flag | inferred | `E45_M1980` |

CDef fields read by `E45_M1980` (`def` = `this+0x34`):

| offset | proposed name | meaning | confidence |
| --- | --- | --- | --- |
| 0x200 (CVector object, data 0x210) | base_point_prf | the mount point in the car's PRF frame (LSP `vecBasePoint_PRF`, `physics-lsp.md:210`) | known (LSP key) + inferred (location) |
| 0x220 (raw, 16 bytes) | base_point_world | output of the car transform: `car.Xform6A30(def+0x210, def+0x220)` | inferred (asm 0x5219c2-0x5219d8) |

### 2b. Shake payload (`E45_P3210`)

Packs 20 words from the host (`+0x70..0x7c`, `+0x80..0x8c`, `+0x90..0x9c`, `+0xa0..0xbc`) into a
24-word `CSigIncarShake` after clearing it. Word 0 is zero. The layout is the signal's
(`CSigIncarShake`, not read). Meaning of the host blocks: not determined (Open 2).

## 3. Functions

| address | current name | proposed name | what it computes | confidence | status and evidence |
| --- | --- | --- | --- | --- | --- |
| 0x521980 | `E45_M1980(self)` | mounted_update_pose | (1) `active = (**(self+0x580) != 0)` (dirty first). (2) `base_world = car.Xform6A30(def.base_point_prf, def+0x220)` with `car = this+0x3d0`. (3) `self.pos = base_world`; `self.pos_end = self.pos`. (4) orientation: the C++ copies `self+0x100` to `self+0x2a0` (C++ lines 222-224, asm not checked) | known (asm 0x521980-0x5219ff) | `-`, 32.0%. Structure checked against asm; the orientation and the rest of the function (after 0x5219ff) are not read |
| 0x524250 | `E45PhysBody::M4250(a, b, c, d)` | pair_walk_sd4 | `pair1 = MD50(d, b.S30())`, `pair2 = MD50(c, b.S34())`, then `a.SD4(pair2[0], pair2[1], pair1[0], pair1[1], b, this+0x20, this+0x24)` (slot 0xd4 on `a`) | known (asm 0x524250-0x5242b5) | bad, 55.2%. See 4 |
| 0x51ad50 | `E43PairWalk::MD50(a, b)` | pair_set | writes `this[0] = a`, `this[1] = b`; returns `this` in eax (asm 0x51ad50-0x51ad5f) | known (asm) | the C++ header says `void`; the asm returns `this` (used by M4250) |

## 4. Open questions

1. **Why M4250 is bad.** The call sequence and argument order match the asm exactly (section 3).
   The `bad` status must come from `E43PairWalk::MD50` (checked: writes two words, returns this),
   the callee `SD4` (slot 0xd4 of `a`, owner not identified here), or the inputs
   (`b.S30`, `b.S34`, `this+0x20`, `this+0x24`). Settles it: a harness peek of the `SD4` arguments
   against the original, with the callee's vtable.
2. **Host blocks in the shake payload.** `E45_P3210` copies `+0x70..0xbc` of the host (the
   passenger state, `jointed-bodies.md` 2d). Which quantities these are (position, velocity,
   accumulators) is open. Settles it: the `CSigIncarShake` field names (the signal's TSV FUNCSIG).
3. **`E45_M1980` tail.** Only the first part (pose from the car) is read. The orientation and the
   rest are not. Settles it: read 0x521a00-0x521a40 and the `Xform6A30` callee.

## 5. Suspect code

### 5a. `E45_M1980`'s "car" local is the CDef (naming only)

- C++: `src/rbr/RBR_E45_MountedA.cpp:213`: `int car = *(int*)((char*)self + 0x34);`
- The local is the mounted body's **def** (`this+0x34`), not the car. The car is `this+0x3d0`
  (`RBR_E45_MountedA.cpp:217`). Behaviour is right (the asm uses `0x34(%esi)` for the def and
  `0x3d0(%esi)` for the car); the name misleads. Rename to `def`. Naming only.

### 5b. `E43PairWalk::MD50` is declared `void`; the asm returns `this`

- C++: `src/rbr/RBR_E43_Types.h` and `RBR_E43_MatrixD.cpp:136` (naked).
- The header's `void` return is wrong for the asm (`movl %ecx, %eax`, `retl $8`). The naked
  body is correct. Any caller that uses the return (`M4250` does, at the asm level) needs the
  `this` return. Naming/declaration only.

## Notes for the lead

- Section 4 item 1: the M4250 "bad" is not in its call order. Route the `SD4` callee and the
  inputs to the E45/E46 owner for a peek, not the call site.
- 5a and 5b are declaration/naming fixes; no behaviour change.
- E45 materials and detectors are not re-checked here; `contacts.md` has them.
