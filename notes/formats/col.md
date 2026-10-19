# Stage collision files (`Maps/track-<NN>_<X>.col`)

Retail 1.0 ships 86 `.col` files, one per real stage (the `Clouds` and
`skyboxes` sets have none). Parser: `rust/rbr-formats/src/col/`; retail
test `all_retail_col_parse` (gated on `RBR_DATA`). All offsets are for the
official 1.02 `RichardBurnsRally_SSE.exe` (decompile:
`out/decomp/RichardBurnsRally_SSE.exe.c`).

Status: the whole file is tiled exactly by the header, table, root,
mesh lists, cells and triangle arrays. The one exception is the root's
brake-wall definition, which is left unparsed. Every triangle vertex
index is in range. The two material bytes are kept raw (see Unknowns).

## Evidence levels

- **Loader** = read from the 1.02 code (`FUN_004fe570` and the walkers
  it calls). Addresses below are exe VAs.
- **Retail** = checked over all 86 files with the Rust parser.
- **C++** = our reconstruction (`src/rbr/`), which may be approximate.
- **Exporter** = named by the Blender track exporter
  (`notes/references/README.md`); retail checks are stated where made.
- **Inferred** = not proven in the code; labelled where it appears.

## Loader (proven)

`NPhysics::E39_FlagC60` @ `0x4ffc60` → `FUN_004fe570` @ `0x4fe570`:

1. `u32[0] == 0x5237434f` ("OC7R") or return 0.
2. Root object at `base + u32[1]` → `FUN_004fdff0` (object fixup).
3. For `i < u32[2]` (cell count): table word `base + u32[3] + 4*i` gives
   an object offset → `base + off` → `FUN_004fdff0`.

`FUN_004fdff0` @ `0x4fdff0` fixes one object. Its byte `[1]` is the mode:

- **mode 1** (every cell in retail): if the object has vertices (`u32 +8`
  nonzero), `FUN_004fd630` @ `0x4fd630` assigns a vertex slot (0..0x3ff) and
  writes it to `obj+2`. The vertex base is `obj + u32 +0xc`. The tree root
  is `obj + u32 +0x10`. `FUN_004fd6b0` @ `0x4fd6b0` relocates the tree and
  grafts it onto the global tree (see Graft).
- **mode 2**: already relocated, nothing is done.
- **mode 3** (root, `[0] == 4` in retail): calls `SetWall(obj+u32 +0x14)`
  (`CBrakeWallManager::SetWall` @ `0x4e4400`), then up to two mesh lists
  (`FUN_004fb220` @ `0x4fb220`, see below). Then it relocates the root
  tree with mask `0xffe00000` and stores it as the collision system's
  tree root (`this+8`).

`CCollisionSystem` (E38) holds the 1024-entry slot table at `this+0xc` and
the tree root at `this+8`. The tree walk is `FUN_004fdd60` (E39 queries).

## Object layouts

All offsets are relative to the object's own start unless marked absolute.

### File header (16 bytes)

| Off | Size | Field | Notes |
|---|---|---|---|
| 0x00 | u32 | magic | `0x5237434f` ("OC7R") |
| 0x04 | u32 | root offset | `0x10` in all 86 files |
| 0x08 | u32 | cell count | 2..590 |
| 0x0c | u32 | table offset | absolute; table is `count` u32 offsets |

### Root object (type 4/3)

Header is 0x28 bytes. Data follows, all relative to the root start:

| Off | Field | Notes |
|---|---|---|
| 0x00 | u8 4, u8 3 | type; mode 3 selects the root path |
| 0x02 | u16 | slot (written by loader; 0 in file) |
| 0x04 | u32 | flags (0) |
| 0x08 | u32 | vertex count (0) |
| 0x0c | u32 | vertex rel: unused (the loader reads it only when `+0x08` is nonzero) |
| 0x10 | u32 | tree rel (`FUN_004fdff0` uses `+0x10`) |
| 0x14 | u32 | brake-wall definition rel (0 means none) |
| 0x18 | u32 | mesh list A count |
| 0x1c | u32 | mesh list A rel |
| 0x20 | u32 | mesh list B count |
| 0x24 | u32 | mesh list B rel |

Retail: `+0x14` is 0x28 in 76 files and 0 in 10. The brake-wall definition
(from `+0x28`) is not parsed. Its size is not proven, and the parser
treats everything up to the first following region as unparsed. Where
`+0x14 == 0` the 8 bytes at `+0x28` are zero and unused.

### Cell object (type 1)

| Off | Size | Field |
|---|---|---|
| 0x00 | u8 | sub (4 in all cells; not checked by the loader in mode 1) |
| 0x01 | u8 | mode 1 (required) |
| 0x02 | u16 | slot (loader) |
| 0x04 | u32 | graft flags: top byte = depth, low bits = path (see Graft) |
| 0x08 | u32 | vertex count (`nv`; all cells have `nv` > 0 in retail) |
| 0x0c | u32 | vertex rel (0x14 in all cells) |
| 0x10 | u32 | tree rel |
| 0x14 | nv×12 | vertices, 3×f32 each |
| tree | | 0x20-byte nodes, then triangle arrays |

Retail: 14,237 cells; 3,485,673 vertices in total; max 590 cells in one
file (min 2). Cells with vertices must number ≤ 0x400 (slot pool); the
parser enforces this.

### Nodes (0x20 bytes)

| Off | Field |
|---|---|
| 0x00..0x17 | 6×f32 bounds: min x, y, z then extent x, y, z (inferred; children lie inside the parent in every retail tree) |
| 0x18 | u32 flags: bits 0..20 triangle count; bit 21 (`0x200000`) graft placeholder; bits 22..31 slot (set by loader) |
| 0x1c | u32 pointer, relative to the tree root |

Node kinds, as `FUN_004fd6b0` and the query code read them:

- **Placeholder** (root only): any bit of `0xffe00000` set. The walk does
  not descend. The graft overwrites the node.
- **Bit 21 on cell trees is normal.** Nearly every cell tree root
  (14,106 of 14,237) and 131 cell leaves carry bit 21 in the file. The
  cell walk runs with mask 0, so the bit is preserved (leaf stamping keeps
  it: `flags & 0x3fffff`); only the 32-byte graft copy gets it cleared.
  The queries never test the bit, so it is inert there. Likely the
  exporter marks graft sources with it.
- **Inner** (count 0, pointer ≠ 0): a pair of consecutive nodes at
  `tree_root + pointer` (64 bytes). Children are at pair `+0` and `+0x20`.
- **Leaf** (count > 0): `count` triangles at `tree_root + pointer`.
  Each array is padded to a multiple of 4 bytes (the padding bytes are
  not constrained: some are zero, some are not).
- Count 0 and pointer 0 is a dead end. Not present in retail.

Retail: root has 14,237 placeholders (one per cell) and no leaves.
Cell trees have no placeholders.

### Triangle record (18 bytes, `E39_SegTest`/`Sweep3A0`/`FUN_004fe810`)

| Off | Field | Notes |
|---|---|---|
| 0x00 | u16 ×3 | vertex indices, in range of `nv` (checked) |
| 0x06 | u16 | attr A: three 5-bit values (bits 0-4, 5-9, 10-14, first vertex first); bit 15 flag. E39 reads the value as `[9]` (the tyre blend weight, see Triangle attributes) and the flag as `[13]` (unknown) |
| 0x08 | u16 | attr B: same layout. E39 reads the value as `[8]` (unknown) and the flag as `[14]` (unknown) |
| 0x0a | u8 | material index 0 (passed to `E44MatTable::Pick`, see Materials) |
| 0x0b | u8 | material index 1 |
| 0x0c..0x0e | u8 ×3 | material 0: per-vertex low nibble = u, high nibble = v (inferred) |
| 0x0f..0x11 | u8 ×3 | material 1: same layout |

The two nibble triples give the sample point in each 16×16 grid
(`FUN_00520010` takes (u, v) as floats in 0..1 and multiplies by the grid
size). The constant `DAT_007399bc` is presumably 1/15 (inferred). The
per-vertex order of the triples is inferred from the E39 weight code.

## Graft (proven)

`FUN_004fd6b0` grafts a mode-1 cell tree onto the root tree at the
placeholder reached by the cell's flags. Starting from the root node,
`depth = flags >> 24` steps are taken. Each step is `node = pair(node) +
(bit × 0x20)`, with `bit` taken from the flags, least significant first.
The final node reached is overwritten by the cell's root node (32 bytes),
and bit 21 is cleared. The loader reads the top byte as the depth
(`uVar3 >> 0x18`). Depths seen in retail: 1..14, most common 7..9.

Retail: the path of every cell reaches a distinct placeholder (14,237 of
14,237). Cells never reach the same placeholder. The root's placeholder
count equals the cell count in every file. A community stage can have two
cells reach one placeholder (RSF 129, cell 10); the grafts run in file
order, so the later cell wins (`notes/formats/rsf-stages.md`).

## Relocation (`col::relocate`)

`relocate(bytes, base)` applies the loader's in-place fixups to a file
image that will live at `base` (`FUN_004fe570` → `FUN_004fdff0`, decompile
above). Per object, in file order (root first, then the cell table):

- Mode-1 cells with vertices take the next vertex slot from 0 (written to
  `+2`, stamped into leaf bits 22..31), are marked `[1] = 2`, have their
  tree relocated with mask 0, and are grafted (32-byte copy, bit 21
  cleared). Cells without vertices are skipped with no slot and no graft
  (the loader's early return); none exist in retail. Mode 2 is refused
  (no retail coverage).
- The root (type 4/3) has its tree relocated with mask `0xffe00000` and
  becomes the graft target. Its slot is 0 (no retail root has vertices).
- Node pointers become `tree address + relative offset` (wrapping 32-bit
  add); leaf words keep bit 21.

Not reproduced (host-side): the vertex slot table stores
(`FUN_004fd630`), the brake-wall build (`SetWall`), and the mesh-list
objects (`FUN_004fb220` allocates `CCollisionMeshWater`s, gated on a
runtime check for list A). The retail test `all_retail_col_relocate`
relocates every file at two bases: only pointer words differ (by the
delta), every pointer lands in the image, every cell takes its own slot,
and every graft target holds the cell root with bit 21 clear.

## Mesh lists (proven, with a C++ constructor)

`FUN_004fb220(count, data)` builds `count` `CCollisionMeshWater` objects,
one per 0x40-byte record (`CtorWater` @ `0x4fb110`). Each record is four
16-byte corners (`CtorWater` takes min/max over corners at +0x00, +0x10,
+0x20, +0x30). The record layout is inferred from that constructor, which
is our reconstruction (`src/rbr/RBR_E38_Mesh.cpp`, 55%). List A (`+0x18`)
is called only when a runtime check returns 2. List B (`+0x20`) is always
called. Retail: 372 items in list A and 949 in list B, across 30 files.

Exporter: list A holds wet surfaces (puddles, only active in wet weather)
and list B holds water surfaces (deep water such as fords). Each record is
four Vec3 corners, each padded with an f32 0. The "only when wet" note
fits the runtime check that gates list A. At most 512 records in both
lists together (exporter limit).

## Materials and surfaces (link to `.mat` and `physics.lsp`)

Evidence: decompile (`out/decomp/RichardBurnsRally_SSE.exe.c`) and asm
(`scripts/fn.py RBR <addr> --asm`), with C++ transcriptions as a second
reading (45-55% confidence each).

1. **Stage table build.** `E09_LoadMaterials` @ `0x46a290` picks the first
   `.mat` entry whose name contains the wear word and the wetness word
   (`FUN_0046f6c0`). For each run j it writes four stack arrays: A[j] = low
   byte of the run width, B[j] = low byte of the run height (`+4`), C[j] = j
   (identity), D[j] = pointer to the 256 cell bytes (`+8`). It then emits
   `CSigMaterialMaps` with that payload. Count is at payload `+0x700`.
2. **E44 table.** The handler is `E44_TableBuild` @ `0x520190` (vtable slot
   `+0x81c`). It writes entry `C[i]` as `{b0 = A[i], b1 = b2 = b3 = 0,
   p04 = D[i]}` (8 bytes) and sets `this+0x800` = table base and
   `this+0x804` = count. **Note:** B is never copied into an entry, so b1
   (height) is 0 as decompiled. Verify in harness before relying on it.
3. **Pick.** `E44MatTable::Pick(tag, a, b)` @ `0x520240` (ECX = table):
   `tag < count` → sampler `FUN_00520010` @ `0x520010` on entry `tag`,
   else `return 0`. The sampler takes **one** float: its asm reads only
   `[esp+8]` (= `a`) and uses it for both axes, `x = clamp(floor(b0·a))`,
   `y = clamp(floor(b1·a))`, index `= b1·y + x`. So `b` is unused, and with
   b1 = 0 the index is `x` (row 0 of the run). This is as decompiled, not
   verified in game.
4. **Sample point.** `a` is the barycentric-weighted low nibble of
   `+0xc..+0xe` (material 0) or `+0xf..+0x11` (material 1), scaled by
   `0x007399bc` (= 1/15 by value). E39 and E38 transcriptions agree on
   this (inferred: the per-vertex order is the byte order).
5. **Surface id.** The sampled byte is the surface id: `hit[10]` / `hit[11]`
   in the ground-hit record (`0x4fe810`, `0x4febc0`, `0x4fd760`; decompile).
   The id goes to `CMaterialManager::Tag520940` @ `0x520940` (decompile):
   `tag < 0x100 && (tag == 0 || entry(tag)+0x158 bit 0)` → `entry(tag)`, else
   `entry(0)`. Bit 0 is `VALID` (see the resolved section below). Entries are 0x160 bytes at `manager+0xc`, 0x100 of them.

### Consumers of the tag (decompile-verified calls)

- `CWheel::Unk9900` @ `0x539900`: `s1 = Tag(x)`, `s2 = Tag(y)`, then
  `CTyre::CScratch::BlendTable(s1+0x148, s2+0x148, s1+0xe8, s2+0xe8, ...)`
  and `E50BlendOps::Blend(s1, s2, ..., z)`. So the tyre gets a blend of two
  surface entries. Its caller is the wheel ground probe `FUN_0053d520`
  (`E51Wheel::M3D520`, `src/rbr/RBR_E51_WheelD.cpp:26`), with
  `x = scratch+0x458`, `y = scratch+0x45c`, `z = scratch+0x454`.
  The probe record at `scratch+0x420` holds the `FUN_004fd760` hit words
  starting at `+0x10`, so `z` is `hit[9]` (attr-A interp, the blend weight),
  `x` is `hit[10]` (material-0 surface id) and `y` is `hit[11]`
  (material-1 surface id). The third probe ray writes `scratch+0x430`
  directly; the other two rays (`+0x4a0`, `+0x510`) merge into `+0x420`
  when it misses. An `E45` entity hit (`E45_M46A0` @ `0x5246a0`) overwrites
  the record with 16 entity words, and `M3D460` @ `0x53d460`
  (`src/rbr/RBR_E51_WheelB.cpp:192`) overwrites `+0x34` with 0.0 and
  `+0x38`/`+0x3c` with a global tag (`config+0x1614c`) when config bit
  `0x40000` (`config+0x160cc`) is set. `M3D230` @ `0x53d230`
  (`src/rbr/RBR_E51_WheelB.cpp:151`) reads the same three slots (`+0x34`
  as the float weight, `+0x38`/`+0x3c` as tags) to add a surface bump
  offset at record `+0x54`/`+0x58`.
- `E37Flag4AC0` @ `0x4f4ac0`: `Tag(o+0xac)` and `Tag(o+0x80)` set event
  flags `0x2000` / `0x4000` from entry `+0x158 & 8`.
- Other `Tag520940` callers: `E38_004FB720` @ `0x4fb720`, `E45PhysBody::M4230`
  @ `0x524230`.

### Triangle attributes (+0x06, +0x08)

Read by `Vf00` @ `0x4fe810` and the ray and sweep routines. Each is three 5-bit
per-vertex values, interpolated with the same barycentric weights as the
material point and divided by 31 (`0x3d... 0.03225806`, decompiled):

- `+0x08` → `hit[8]`, bit 15 → `hit[14]`.
- `+0x06` → `hit[9]`, bit 15 → `hit[13]`.
- Exporter names: `+0x06` is per-vertex "blending" (the mix between
  material 0 and material 1) with bit 15 "no auto spawn". `+0x08` is
  per-vertex "shading" with bit 15 "no auto spawn if flipped". Bits 0-4
  belong to the first vertex, which matches the E39 weights (bits 0-4 and
  `+0x0c` get the weight `1 - n0 - n1`). The exporter writes each UV byte
  as `u << 4 | v`, so the low nibble that `Pick` samples is the
  exporter's `v`. Not checked in game.
- Sweep (`0x4ff3a0`, `src/rbr/RBR_E39_Collide.cpp:260`) stores them raw,
  `<<3` per field, in the 0x40 record at `+0x32`, `+0x35`, `+0x38`
  (from `+0x06`) and, when the sweep flags word has bit 2 set, the flags
  at `+0x39` (else `+0x39` is 0): bit 0 is attr-A bit 15 and bit 1 is
  attr-B bit 15 (both flags). `(w8 >> 14) & 2` keeps bit 1 of the shifted
  value, i.e. bit 15 of the word; decomp `(puVar14[4] >> 0xe) & 2 |
  (puVar14[3] >> 0xf)` and C++ `((w8 >> 14) & 2) | ((w6 >> 15) & 1)`
  agree (`w6`/`w8` are the `+0x06`/`+0x08` words). Who reads sweep `+0x39`
  was not traced (readers are the fence
  and ground queries `CFenceHandler::QueryGround` @ `0x601a60` and
  `CPhysics::Vf10` @ `0x524f80` via `E39_Query7F0`/`E39_Query8D0`).

Who reads the float-record fields:

- `hit[9]` (attr-A interp): resolved. The wheel probe record carries it at
  `+0x34` and `E51Wheel::M3D520` @ `0x53d520` passes it as `z` to
  `CWheel::Unk9900` @ `0x539900`, which blends the two surface entries
  with it (`E50BlendOps::Blend`). `M3D230` @ `0x53d230` reads the same
  slot as the weight for its bump offset.
- `hit[8]` (attr-B interp): partly traced. `E33Mat34::CollideInit` @
  `0x51bd30` (`src/rbr/RBR_E43_GroundC.cpp:12`) copies `hit[8]`..`hit[11]`
  into event-build locals alongside the normal; no downstream consumer of
  the `hit[8]` copy was found (`CCollisionEvent::InitFull` @ `0x4f50e0`
  takes no blend fields). No other reader found.
- `hit[13]` / `hit[14]` (bit 15 of `+0x06` / `+0x08`): no reader of the
  float-record slots found in game code. The same two bits reach the
  sweep record `+0x39` (above). The exporter "no auto spawn" names stay
  exporter-only.

### Sentinel and out-of-range bytes

- Retail `.mat` run counts are at most 38. Bytes 58, 60, 61, 62, 63 and 124
  are out of range for every stage (`materials[1]` = 63 in 1,730,284
  triangles, 62 in 449,920, 61 in 5,703, 60 in 227, 58 in 2, and
  `materials[0]` = 124 in 58,637).
- `Pick` returns 0 for out-of-range `tag`. So these sample surface id 0
  (`Undefined`), as decompiled. That is the best reading of 61..63 and
  124: "no material". Unproven in game.
- Zero values in `materials[0]` (6,639) occur only in stages whose `.mat`
  has no runs (`track-69_*`, `track-68_M`).

### Surface id → physics surface (resolved)

- **The id is the cell byte.** The exe holds a 134-record table at VA
  `0x7caf38` (file offset `0x3caf38`), stride 16: `{u32 name pointer, 0,
  u32 value, 0}`. For every record, `value = 88·id + 3` with `id` the
  surface id. The name is the lsp key. Read `id = (value - 3) / 88`.
  Found by searching the exe for the surface-name strings. The table is in
  the binary, not the `.lsp`. `Undefined` is 0.
- **The lsp `; id N` comments are not the key.** `StripComments`
  (`0x52eba0`) blanks every `;` comment before parsing, so the parser never
  sees them. They agree with the exe table on 84 of 86 labelled entries. They
  differ on `BR2_Gravel_Coarse_Deeper_Damp` (comment 194, table 198) and
  `BR2_Gravel_Coarse_Deeper_Wet` (comment 195, table 199). The data uses 198
  and 199, so the table is right and the comments are stale.
- **Entry index and enable bit.** The parser maps a child key to its slot
  through the class dictionary (`CBase::Parse` @ `0x52f0c0`, `Find`
  @ `0x52ebf0`). The slot is `m_materials[id]`. The enable bit is entry+0x158
  bit 0. The bit-name table at `g_7caf18` gives bit 0 = `VALID`, 1 = `SOFT`,
  2 = `HARD`, 3 = `BENDABLE`. Every retail surface entry in the lsp carries
  `+VALID` (133 of 134 table ids; `Metal_Barrier`, id 64, is not in the lsp).
  Every in-range cell tag in retail is enabled. So an id that is not `VALID`
  falls back to `Undefined`. (Earlier notes wrote +0x164; `Tag520940` reads
  `manager+0x164+tag·0x160`, which is entry+0x158.)
- **Check on track-10_O (arctic).** Its cell tags map to SnowOnGravel_Deep
  (103, 17,965 cells), SnowOnGravel_Medium (102), SnowOnGravel_Shallow (101),
  SnowOnIce_Shallow (117), Snowwall_Bottom (88), Rough_Rough_Dry (129),
  SnowOnIce_Deep (119). That fits a snow road. The three road tags are
  identical in the dry, damp and wet `.mat` entries, which matches
  snow surfaces, which have no wet variants.
- **Harness confirmation on Tanner (track-54).** Run `h1-surface` probed the x/y
  tags passed to `CWheel::Unk9900` for all four wheel slots over 12,084 physics
  steps. Every wheel saw ids 6, 14, 22, 106 and 114; id 0 (`Undefined`) also
  appeared in each wheel's y tag. The exe table resolves these to
  `Gravel_Medium_Deeper_Damp` (6), `Gravel_Medium_Deep_Damp` (14),
  `Gravel_Medium_Shallow_Damp` (22), `Grass_Medium_Damp` (106), and
  `Grass_Soft_Damp` (114). The observed gravel and grass types match the stage's
  damp gravel route with grass shoulders. Probe config and full result:
  `harness/probes/surface-tags.peek` and `~/rbr-harness-runs/h1-surface.peek.bin`
  on cachy; details are in `lead/reports/h1.md`.
- **Other stages look plausible.** British (track-22): Grass_Soft_Dry,
  Rough_Rough_Dry, BR2 gravel. USA 2002 (track-62): Dirt_Medium_Dry,
  Gravel_Coarse_Shallow_Dry. Every in-range tag in the retail data maps to a
  table name.
- **Scenery ids** (bushes, trees, rocks, poles, spectator, ids 24–32,
  56–63, 255) never appear as ground tags. They are object surfaces.

### Consumers of the tag (decompile-verified calls)

Same callers as above (`Unk9900` with the resolved `z`/`x`/`y` mapping,
`E37Flag4AC0`, `E38_004FB720`, `E45PhysBody::M4230`), plus the E37 mesh-
triangle path: `E37GridOps::M9B80` @ `0x4f9b80`, `M9C50` @ `0x4f9c50` and
`Build9CD0` read a per-triangle tag byte (`t[3]` of E37's own 7-byte mesh
records, not `.col` triangles) and test entry `+0x158` bits 1 (`SOFT`) and
2 (`HARD`) for edge insertion (`src/rbr/RBR_E37_Solver.cpp:180,267,292`).

## Brake-wall definition (`root+0x14`)

Evidence: `SetWall` @ `0x4e4400` and `E34WallVf::E34WallVf` @ `0x4e4220`
(decompile), plus `CBrakeWall::AddCount` @ `0x4e3810` and `QueryPoint`
@ `0x4e3dc0` (C++ transcriptions for the tree walk).

- `def = root + u32(root+0x14)`. `SetWall` builds a wall only if `def+4 != 0`.
- `def+0x00` u32: not read by the ctor. Size of the whole definition: it
  equals the end of the node tree's last leaf, minus `def` (retail 76/76;
  field name from the exporter).
- `def+0x04` u32: point count `n`. Ctor reads it into the wall's count field.
- `def+0x08` u32: point block offset, relative to `def`. 0x10 in all 76 files.
- `def+0x0c` u32: root node offset, relative to `def`.
- Point block: `n` × 8 bytes, two f32 `(x, y)`. Exporter: `n/2` pairs of
  (inner, outer) points; it repeats the first pair at the end to close the
  loop (retail: last pair equals first in 67 of 76). It ends exactly where the root
  node starts (`def+8 + 8n == def+0xc`, 76/76). The ctor names it
  `m_mesh218` (segment vertex table, inferred).
- Root node: 0x18 bytes. 4 f32 box, u32 items (nonzero = leaf), u32 child
  (relocated by `AddCount` with the root node's block as base). The box is
  (centre x, centre y, half x, half y): `QueryPoint` tests `|p.x - cx| < hx`
  and `|p.y - cy| < hy`. The box edges equal the point extremes to about 1e-4
  in all 76 files. Children are two 0x18 nodes at the child pointer (from
  `AddCount`).
- Leaf items (exporter, checked on retail): `items` u16 values at
  `root node + child`, padded to 4 bytes. Bits 0-13 are the index of a
  pair's inner point (always even and `< n - 1` in retail), bit 14 is
  "rally school" (969 items) and bit 15 is "auto respawn" (9,342 of 40,616).
  Both flags are read by `E34WallVf::Raycast` @ `0x4e4570`
  (`src/rbr/RBR_E34_Wall.cpp:275`): bit 15 lands in out-record `+0x18`
  bit 0, and `CCar::Vf6cx` @ `0x4ea300` (`src/rbr/RBR_E34_CarD.cpp:583`)
  accumulates its wall-contact (respawn) timer only when that bit is set
  and the penetration depth exceeds 0.5. Bit 14 makes the segment
  non-colliding: `Raycast` skips the impulse (returns 0) and instead emits
  `CSigCarBrakeWallPenetration` with out `+0x18` bit 2 set, so bit-14
  segments are signal-only sensors. Children of inner nodes are
  also relative to the root node. With this encoding the walk ends exactly
  at `def + def[0]` in all 76 files. The Rust parser still declares the
  whole definition as one unparsed region.
- **Correction to the task brief:** `QueryPoint` and `QueryBox` walk this
  brake-wall tree (`this+0x64`), not the cells. Callers: `QueryPoint` ←
  `E34WallVf::Raycast` @ `0x4e4570`; `QueryBox` ← `CBrakeWall::Vf15C` @
  `0x4e4070` and `E34WallVf::VfCC` @ `0x4e42f0`. Cells are walked by the E39
  and E43 queries (`0x4febc0`, `0x4fd760`, `0x4ff3a0`, `0x4ffc90`, callers
  E39 query callers: `0x4febc0` ← `E33Mat34::CollideInit` (`0x51bd30`);
  `0x4fd760` ← `CollideInit` and `E51Wheel::M3D520` (`0x53d520`); `0x4ff3a0`
  ← `E39_Query7F0` (← `CFenceHandler::QueryGround` `0x601a60`) and
  `E39_Query8D0` (← `CPhysics::Vf10` `0x524f80`); `0x4ffc90` ← `CGround::Vf100`
  (`0x51c660`).)
- Parsed by `col::parse` into `Root::brake_wall` (points and root box).
  Retail test checks 76 walls and the box edges.

## Unknowns

- Surface id → physics surface: resolved (above). The table holds the names and ids, and the lsp supplies the properties.
- Who writes the entry+0x158 `VALID` bit at load time: resolved. Retail
  `Physics/physics.lsp` (inside `Root/physics.rbz`) gives 133 surface
  entries a `Flags +HARD +VALID` line. `CBase::Parse` @ `0x52f0c0`
  dispatches the `Flags` child to `CBitField::Parse` @ `0x52f930`
  (`src/rbr/RBR_E48_NTweak.cpp:568`), which sets mask bits from `+`/`-`
  tokens matched against `CMaterial::CFlags::GetBitName` @ `0x520840` →
  `g_7caf18` (`src/rbr/RBR_Globals.cpp:377`, index 0 = `"VALID"`). The mask
  sits at entry+0x158, which `Tag520940` @ `0x520940`
  (`src/rbr/RBR_E45_Material.cpp:99`) gates on.
- Wheel probe source of `z`, `x`, `y` (`scratch+0x454/0x458/0x45c`):
  resolved. They are the `FUN_004fd760` hit words at record `+0x34`/`+0x38`/
  `+0x3c`: `z` = `hit[9]` (attr-A blend weight), `x` = `hit[10]`
  (material-0 tag), `y` = `hit[11]` (material-1 tag); `E51Wheel::M3D520`
  passes them to `CWheel::Unk9900`, with `E45`/config overrides as above.
- `hit[9]` reader: resolved (tyre blend weight, above). `hit[8]`:
  still open (copied by `CollideInit` @ `0x51bd30`, no consumer found).
  `hit[13]`/`hit[14]`: still open for the float record (no reader found);
  the sweep path forwards both bit-15 flags to its `+0x39`, whose readers
  (`0x601a60`, `0x524f80`) were not traced.
- The `b1 = 0` entry and row-0-only sampling (as decompiled). Harness check
  needed (no cachy access from here). `Pick` @ `0x520240` also serves the
  ray (`0x4febc0`) and open (`0x4ffc90`) queries, not just `0x4fd760`.
- Brake-wall leaf flags (bits 14/15): resolved (above). Bit 15 feeds the
  respawn timer via `CCar::Vf6cx`; bit 14 makes sensor-only segments that
  emit `CSigCarBrakeWallPenetration`. Leaf items and extent are resolved above.
- Triangle attribute meanings (blending, shading, auto spawn) are
  exporter names, except the interpolated attr-A value, which is proven
  to be the tyre surface-blend weight. The bit-to-vertex order follows
  from the E39 weights. `col` exposes the decode as
  `Triangle::attr_parts`/`attr_a`/`attr_b`.
- Node bounds: meaning inferred from geometry (children inside parents).
- Mesh record layout (0x40 bytes, four corners): inferred from our
  constructor and matches the exporter; not checked against data.
- Triangle array padding bytes are not constrained (some zero, some not).
- The loader's mode 2 path is not supported (only mode 1; no retail file has
  mode 2).
