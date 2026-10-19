# HH_3DSMAXLoaderUTILS on-disk format (H7, 0x1000a330-0x1000e59f)

Read cursor: `{ base, pos, end }` (+0x00/+0x04/+0x08). All integers are
little-endian; all strings are `int length + bytes` (no terminator).
LTCG threads the cursor in registers (esi/ebp normally, ecx for the
matrix readers); donors are H6 0x10007620 and H11 0x1001e430.

## Primitives (0x1000c680-0x1000c920)

- int/byte: direct loads, pos += 4/1.
- blob (`ReadBlob`): fails (returns 1) when `pos >= end`, else copies
  `end - pos` bytes and returns 0.
- 3x4 row-major matrix (`ReadMatrix`): 12 floats, stored transposed with a
  1.0 diagonal (i.e. the file holds rows, the loader builds columns).
- transform block (`ReadTransform`, 0x38 bytes): scale vec3, axis vec3 +
  angle (degrees, x87 fsin/fcos), translation vec3, second axis + angle.
  Produces scale + two quaternions + translation (`NLB_TransformParams`),
  composed by `NLB_ComposeTransformEH` into the node's local matrix; the
  world matrix follows via `ReadMatrix`.
- node header: name string, 2 ints (+0xc0/+0xc4), transform block, local
  (+0x80), world (+0x40).

## Records

- AABB (0xa330): min vec3, 0x10 skip, max vec3 into a `CHH_AABB`.
- Collision (0xa3f0, 0x130 `CHH_CollisionData`): name, 2 ints, transform,
  world matrix, vec3 vertices (pushed 12B), face count + 8-byte face records
  into a resized 8B-element vector, sub-record loop (24B pushes), optional
  flag-1 tail (two `count*count`-byte blocks).
- Shadow (0xa7c0, 0x1a4): name in, 2 ints, transform, world, vec3 vertices,
  face records (H17 0x10042130), 16-byte elements (inlined `H8_Insert10A`
  pushes), H17 finisher (0x10041810); `s_wing/s_mirror_*` sets +0x14c.
- Occlusion (0xaab0, 0x158): 2 ints, transform, world, 0x18 skip, vec3
  vertices at 24B stride (12B pushed), flag-gated skips (bit2: `n*8`,
  bit4: `n*4`), 3xu16 face records (H26 0x10074c90), name strings + 0x10.
- GeomCar (0xacf0, 0x2a0): deform flag (bit1 -> +0x294), node header, AABB
  record (int + min/max + lastPlane 0), vertex count + stream flags. Bit0:
  deform-holder vertices (0x1000d530) or fresh VB (H6 0x1000a2c0) + two
  `PushStream`s + lock/zero/unlock; bit2: texcoord fill; bit4: `n*4` skip.
  Index buffer (count x 6B) via N-B 0x10088880 (throws
  `[HH_3DSMAXLoaderUTILS::LoadGeomObject] error creating index buffer`).
  FaceBlock list (name, 4 ints, +0x54=4, +0x64=0; car ptr pushed per block).
  Morph (auxFlags bit24) stored at +0x160; morph-count vector (0x1000d8b0)
  + per-morph 8B vectors (0x1000dc10).
- Geom factory (0xb490): Deformable (flags bit0|1, flags byte kept at +0x180)
  else plain `CGeomObject`. Same node header/AABB; bit0/1/2 streams, morph
  (+0x160), skin (auxFlags bit16 -> +0x164 + `m_boneCount` -> +0x16c +
  `(count+1)` identity scratch matrices at +0x168).
- Skin (0xbb50, 0x90): head matrix (+0xc) + inverse, count, per-bone (cursor
  string [accumulated, bone names all get the one pre-loop string], matrix,
  inverse, 0x100a57fc combine into 0xa0 `SHH_SkinBone` slots; bone id is an
  uninitialized-stack read), trailing `new[]` 0x20-records at +0x8c.
- Morph (0xbdd0, 0x14): count, per-target (1 skipped byte, `Add`, file vec3s
  pushed into the target's index vector - its elements are 0xc bytes, so
  H2's `vector<int>` guess for `SHH_MorphTarget::m_indices` needs revisiting).
- Camera/Light/Helper/Bone/Node (0xbed0-0xc550): node header + kind ints;
  Bone writes parent id +0xc8; finders (0xc990-0xcb10) are child-list scans
  (the "BFS" one is iterative with an explicit deque).
- Material (0xcc30): INI section reads (N-B 0x1008a0a0) into `D3DMATERIAL9`.
- FaceShader (0xd0b0): `FaceBlock%1%` format + faceSlot, `JoinPath`
  `*_Shader/*_Material`, INI reads, `SShaderDesc::CreateShader`.
- Shader-desc twin ctor/dtor (0xd400/0xd480): default-construct/destroy the
  five strings (this on the stack, EH states).

## Findings for other batches

- H2: `SHH_MorphTarget::m_indices` elements are 0xc bytes, not int.
- H5: Car/Deformable +0x160 receives the morph modifier (holder slot
  repurposed when a morph is present); `SHH_MorphEntry` (0x10) is the
  0x1000dc10 target (base + begin/end/cap triple).
- H8: 0x1000eb50/0x100106b0 are the 16B vector insert/fill; 0x1000a57fc is a
  3-matrix bone combine; 0x1000a57ef is listed by Ghidra as a bb50 callee but
  never called.
- H11: loader entry order - (flags, cursor, name, auxFlags) for the factory;
  (obj, cursor, name, flags, auxFlags) for the Car overload; morph flag is
  auxFlags bit24, skin bit16.
- N-B: 0x100887e0/0x10088880 are `__fastcall` 6-arg VB/IB creators;
  0x10008900/0x10008700 are insert-style (count, vec, where, val) helpers.
- H6: 0x1000a2c0 is `(stride, bytes, out, cursor)` `__fastcall` VB filler.
- TU flags: /GX + /GL throughout; only 0xbb50/0xd0b0 carry /GS cookies.
- 0x1000d790 is a `jmp 0x1000dcd0` alias; 0x1000dcb0 is `ret 4`;
  0x1000dd50 returns the global at 0x101de608 (null-tested by d0b0).
