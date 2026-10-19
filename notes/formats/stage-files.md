# Stage (track) files — formats and loaders

Per-stage data lives loose in `Maps/` (retail 1.0). The directory has 88
`.trk` files: 86 populated stage sets and the empty `Clouds`/`skyboxes` sets.
Rust readers live in `rust/rbr-formats`; unknown fields stay raw where the
loader does not establish their meaning.
All addresses below are for the official 1.02 `RichardBurnsRally_SSE.exe`;
decompiled C is in `out/decomp/RichardBurnsRally_SSE.exe.c`.

## Extension map (`Maps/Tracks.ini`)

`Tracks.ini` (292 lines) defines one `[MapNN]` section per stage
(`TrackName = "Maps\track-4"`, …) and the file roles in `[General]`:

| Key | Ext | Role (inferred) | Present |
|---|---|---|---|
| `TrackExt` | `.lbs` | track geometry/scenery (`Maps\track-4.lbs`) | 88 |
| `PhysicsExt` | `.trk` | physics surface data (`track-4.trk`, ~0.4–0.8 MB) | 88 |
| `DriveLineExt` | `.dls` | AI driveline / animation data | 86 |
| `CollisionExt` | `.col` | collision meshes | 86 |
| `MaterialExt` | `.mat` | surface-material grids | 86 |
| `FenceDataExt` | `.fnc` | fence data | 86 |
| `ScriptExt` | `.lua` | event scripts (inside `Root/Scripts.rbz`) | 749 total |
| `BrakeWallExt` | `.bws` | brake walls (?) | **0 — no `.bws` file ships** |

The two entries missing `.mat/.fnc/.dls/.col` are `Clouds` and `skyboxes`
(skybox sets: 0-byte `.trk`, plus `.lbs`/`.ini`). Every real stage
(`track-<NN>_<X>`) has the full set. Stage name suffixes seen: E/M/N/O
(plus texture-variant rbz names like `track-4_E_textures`,
`track-4_M_textures`).

## Headers (from `track-10_O.*`)

- **`.dls`** — magic `MINAATAD` (`4D 49 4E 41 41 54 41 44`), then
  `0x12 00 00 00` (version 18), `0x00…` (minor 0), then u32 tables.
  Loader `FUN_00479020` checks both words of the magic and errors
  `"Not an AnimData file!"` / `"File version does not match code version.
  ( %d != %d )"` (minor mismatch is a warning). So `.dls` = **AnimData**.
- **`.col`** — magic `OC7R` (`0x5237434f`, "R7CO" = Rally 7 Collision?),
  then u32 `0x10`, `0xdc`, offsets... Loader `FUN_004fe570` branches on the
  magic and walks offset tables (`param_1[1] + base`, count in `param_1[2]`).
  Full layout (tree, placeholders, triangles, materials): `notes/formats/col.md`.
- **`.mat`** — u32 count (9 on `track-10_O`), then per entry: NUL-terminated
  name (`"arctic standard dry new"`, …), u32 run count (26), then that many
  264-byte records. Each record is u32 `16`, u32 `16`, then 256 surface-class
  bytes (`g/f/e/u/X/...`, i.e. a 16×16 grid). Loader `FUN_0046a460` (below).
  It reads `count` entries; community files can carry a trailing `CMAP`
  block that it ignores (`notes/formats/rsf-stages.md`).
- **`.fnc`** — u32 magic `2`, u32 group count (13 on `track-10_O`), then per
  group: u32 post count, 4× u32, 6× f32, then that many posts of 10× f32 +
  1× u32. Loader `FUN_006038c0` (below).
- **`.trk`** — chunked container: repeating `[u32 header-len][header][u32
  size][size bytes payload]`, walked to EOF. `track-10_O.trk` = `(8, [3,
  20], 10596, …)` then `(8, [3, 22], 433456, …)`. So header-len 8, type 3,
  kind `0x14`/`0x16`. Loader `FUN_0046ed20` (below).
- **`.lbs`** — see `notes/formats/lbs.md` (geometry (1,0) and objects (1,1) parsed). Same chunked container with type 2: `(8, [2, 11], 88, …)`,
  `(8, [2, 16], 24, …)`, `(8, [2, 12], 3964, …)`, …. 26 MB render geometry;
  parsed by HedgeHog3D's `CHH_TrackLoader` (H21/H22; loader at world+0x1dc).
  Type-1 subtypes 0/1/3/6/7/8 and type-2 subtypes 10/12 carry the
  render/scenery payloads (4/11/14/15/16/18 are H23/H24): `(1,0)` geometry
  blocks (u32 count, u32 sky value, nine view payloads, chunks of 0x70 raw
  bytes + bbox corners); `(1,1)` object blocks (desc key, lanes of
  KObjectBlock items); `(1,3)` sky reflector / `(1,2)` clouds (named face
  + item set); `(1,6)` interactive, `(1,7)` reflection, `(1,8)` water
  faces (flag-selected texture/spec/vertex payloads); `(2,10)` visibility
  blocks (two vec3 + lane per entry); `(2,12)` driveline points (Vec3s equal
  to the `.trk` driveline with y/z swapped; see `lbs.md`).
- **`track-<NN>_<X>.ini`** — plain text: `[TextureInfo] NumTextures=42`,
  `Texture0=CHAIR 01.tga`, … (TGA names mapped to the `.dds` blobs in the
  per-stage `*_textures.rbz`).
- **`TextureFilenameMap.dat`** — u32 count (6,970) then NUL-separated
  `archive-path ↔ short-name` pairs
  (`track-4_m_textures\dry\cone01.dds\0cone01.dds\0…`), with `\` separators.
  Presumably maps material texture names to archived DDS files.

## Loader anchors in the exe

- `FUN_0046d040` — bootstrap that reads `maps\tracks.ini` and the
  `[General]`/… extension keys (`TrackExt`, …).
- `FUN_00479020` — `.dls` AnimData parser (magic + version gate above).
- `FUN_004fe570` — `.col` parser (magic `0x5237434f` gate above).
- `.trk` / `.lbs` / `.mat` / `.fnc` loaders not yet located (no obvious
  magic strings; search by callers of the archive-read helpers
  `FUN_00401260`/`FUN_00401460` next).

## Loaders (`.trk`, `.mat`, `.fnc` located; `.lbs` traced to renderer)

How found: `FUN_0046d040` stores each `[General]` extension in a
`std::string` member (`+0x30` TrackExt, `+0x4c` PhysicsExt, `+0x68`
CollisionExt, `+0x84` DriveLineExt, `+0xd8` ScriptExt, `+0xa0` MaterialExt,
`+0xbc` BrakeWallExt, `+0xf4` FenceDataExt); paths are built as
`TrackName + ext` by `FUN_004665f0` @ `0x4665f0`. String→code xrefs were
resolved by scanning `.text` for the extension-string VAs. The async load
state machine is `CPC_TrackLoaderDaemon::vf1c` @ `0x46f330` (state 2 =
`FUN_00466f10` frontend/textures + `FUN_00466d80` driveline, state 3 =
`FUN_0046ed20` physics set, state 6 = `FUN_00467440` fences).

- **`.trk` — `FUN_0046ed20` @ `0x46ed20`.** Builds TrackName+PhysicsExt /
  +CollisionExt / +MaterialExt / +BrakeWallExt paths. `.col` goes to the
  known wrapper `FUN_004ffc60` (`0x4ffc60` → `FUN_004fe570`); `.trk` is
  opened with the chunk reader (`FUN_00419690` init / `FUN_004196c0` open,
  cursor readers `FUN_00419930` u32 / `FUN_004199f0` bytes / `FUN_00419a20`
  EOF / `FUN_00419a40` seek / `FUN_00419a50` tell) and walked (`0x46f190`):
  `hlen = u32; header = hlen bytes; size = u32`; if `header[0] == 3`,
  `header[1] == 0x14` → `FUN_0046a530` @ `0x46a530`, `header[1] == 0x16` →
  `FUN_0046e300` @ `0x46e300`; then seek to `pos + size`. `0x46a530` reads
  a u32 count and records containing two Vec3 values, one f32 and two u16
  values (32 source bytes; runtime allocation is `count × 0x30`).
  `0x46e300` reads collision meshes when the
  `Settings/SendShapeCollisionMeshes` setting is enabled. `.mat` goes to
  `FUN_0046a460` (below); the `.bws` path is built but **never consumed**
  (only destroyed) — brake-wall loading is dead code in 1.02.
- **`.mat` — `FUN_0046a460` @ `0x46a460` → `FUN_0046a290` @ `0x46a290`.**
  Opens the path with `CreateFileA`/`ReadFile` (whole file) and parses:
  u32 count; per entry NUL name + u32 run count + that many 264-byte
  records, of which the loader keeps byte 0, byte 4 and a pointer to the
  remaining 256 class bytes. Entry selection: `FUN_0046f6c0` @ `0x46f6c0`
  provides two `strstr` match words — wear (`"new"`/`"normal"`/`"worn"`)
  and wetness (`"dry"`/`"damp"`/`"wet"`) — matching names like
  `"arctic standard dry new"`; the winner is applied by `FUN_00468970` @
  `0x468970` (unexamined). The first-match rule is `mat::select` (all nine
  wear/wetness pairs resolve in every retail file). A zero run count is
  accepted by `FUN_0046a290`:
  it skips the run loop and leaves the named entry selectable. Six retail
  files have nine empty entries each, all named `USA 2002 standard ...`,
  and are exactly 289 bytes.
- **`.fnc` — `FUN_00467440` @ `0x467440` → `FUN_006038c0` @ `0x6038c0`**
  (in the `NStickSystem` range, cf. `CFenceRenderer::Init` @ `0x605100`).
  Opens TrackName+FenceDataExt with `fopen(..., "rb")` (loose file, not
  via archive) after `FUN_006036e0` @ `0x6036e0` initialises the fence
  handler. Header: u32 magic must equal the global at `0x74f858` (value
  `2`, verified in the binary), then u32 group count (zero is accepted).
  Per group: u32 post count `n` (via `FUN_00603760`), 4× u32, 3× f32, 3×
  f32, then `n` posts of 3× f32 + 3× f32 + 3× f32 + 1× u32 (stride `0xb4`,
  prev/next linked); posts are finished via `FUN_00600bb0` and
  `CFenceRenderer::vf14` @ `0x603ba0`. For nonzero groups,
  `FUN_006058c0` @ `0x6058c0` reads a u32 texture count; if nonzero, a u32
  fixed-name width followed by `count × width` bytes. Retail names are
  32-byte NUL-padded values used as `Textures\\<name>.dds`. The caller skips
  this helper for zero groups; 24 retail files then leave an 8-byte trailer
  (`u32 0`, `u32 32`) unread.
- **`.lbs` — path only, in exe.** `FUN_00466f10` @ `0x466f10` assembles
  `TrackName + time-of-day + TrackExt` (`sprintf "%s%s%s.%s"`) and hands
  it to the graphics engine via callback (`(DAT_007ea990 + 0x40) + 0x18`).
  The only chunk-reader loop in the exe is the `.trk` one above
  (verified: sole callers of `FUN_00419930`/`FUN_00419a40` outside the
  reader family itself), and `HedgeHog3D.dll` has no `TrackName`/`.lbs`
  references — so the type-2 chunk walk lives outside the located code
  (unknown). Container framing (type 2) verified from file bytes.
- **`.dls` wrapper** `FUN_0047afe0` @ `0x47afe0` (`"Animation file not
  found: %s"` → `FUN_00479020`) is called from `FUN_00466d80` @ `0x466d80`
  with TrackName+DriveLineExt (`+0x84`; warns `"No spline file found,
  animations will not work correctly."`).
- **`.col` — see [col.md](col.md).** `FUN_004fe570` @ `0x4fe570` dispatches
  to the object and tree walkers at `0x4fdff0` and `0x4fd6b0`. The Rust reader
  parses the root, cell trees, grafts, vertices, triangles, and water-mesh
  records. It verifies exact tiling and vertex indices across all 86 retail
  files. The brake-wall points and root box are parsed; its node leaf-item
  encoding and some triangle attributes remain unknown.
- **`.dls` — `FUN_00479020` @ `0x479020`.** The 56-byte header is `MINAATAD`,
  version 18, minor version 0, and ten file-relative offsets. Table 0 has a
  count and two-word entries; the first word points to a 17-word record, the
  second is retained as unknown. That record contains a byte-block reference,
  seven counts (words 1–7), seven offsets (words 9–15; six fixed arrays and
  one nested table), and the replay stream offset at word 16. The nested
  table uses count/offset words 4/12. The decoded
  array widths are respectively 2, 3, 3, nested variable records, 4, 3 and 5
  words; fields remain raw because their meanings are not established. Table
  1 is a count followed by three-word group headers and each header's third
  word count of 11-word children. Tables 2/3/4/5 are count-prefixed arrays of
  14/6/3/7 words. Table 4 is passed to `FUN_005e96f0` @ `0x5e96f0`. Table 6
  is count-prefixed Vec4 data used to initialise spheres; `FUN_004772a0`
  squares the fourth component. Table 7 is a byte-counted block copied by the
  loader. Table 8 is optional four-word world/default data (`0xffffffff`
  means absent); the loader uses xyz and squares w. Table 9 is a count and
  two-word records. `CReplayManager::ReadStream` @ `0x498840` consumes a
  seven-word header; only version 6 proceeds to its 10-, 6- and 2-word arrays.
- **`.bws`** — extension key `BrakeWallExt` is read but its path is unused
  (see `.trk`); no samples ship. Format unknown, loader absent.

## Rust reader coverage and retail observations

- `.mat` (`FUN_0046a290`) and `.fnc` (`FUN_006038c0`, plus texture helper
  `FUN_006058c0`) are fully parsed through the bytes those loaders consume.
  `.fnc` fields are loader-named (`FncGroup`/`FncPost` in `fnc.rs`); the
  parser preserves the texture bytes and any bytes the game leaves unread
  rather than assigning unsupported meanings.
- `.fnc` names from the Blender track exporter (`rbr_track_formats/fnc.py`),
  matching the loader's field widths: the group header is `{tile type,
  pole type, tile texture index, pole texture index}` plus a bounding box
  (6× f32). A post is a Vec3 position, a bounding box covering this post to
  the next, and a BGRA colour u32. Fence types 0-14: tape, tape long, net,
  net long, tape/net pole grey, black, blue, red, barbed wire, barbed wire
  long, barbed wire pole. The exporter says the game needs the file even
  when empty: without it the car shadow ignores fog and particles may fail.
- `.trk` framing and both observed payloads are parsed. Across all 88 retail
  files there are 86 nonempty files, each with exactly one `(3,0x14)` and one
  `(3,0x16)` chunk; the two skybox `.trk` files are empty. Kind `0x14` records
  are 32 bytes. Kind `0x16` records contain a NUL name, a u32 kind, two bytes
  (the first has unknown meaning; the second selects static when zero), a
  shape code, u16-truncated vertex/face counts and variable arrays. Shape 0
  has no shape data, shape 1 has two Vec3 bounds, shape 2 has a Vec3 and f32
  radius; other shape codes consume no shape bytes in `FUN_0046e300`. Each
  face consumes one byte and three u32 values, of which only low bytes are
  stored. Each placement is an id, two Vec3 values and four f32 values.
- `.trk` open fields after the loader work: each of 27,458 kind
  `0x14` records has 4 bytes as two u16 values; both are `0x0000` in every
  observed record, they are stored swapped (first at element `+0xe`,
  second at `+0xc`), and no consumer is traced. Each of 6,295 kind `0x16`
  meshes has a material byte after its u32 kind with no traced consumer
  (histogram below; values sit in the `.col` surface-id space but that use
  is unproven). The static/dynamic byte is resolved (0 = static, 1 =
  dynamic; 1 only on kind-0/material-0 meshes) and kind 15 is resolved
  (loader default branch, 12 static `HO_BUSH01` bushes). Observed material-byte histogram: `00:5311`,
  `19:20`, `1b:18`, `1c:12`, `1d:37`, `1e:45`, `1f:174`, `20:191`,
  `38:66`, `39:47`, `3a:50`, `3b:11`, `3c:14`, `3d:25`, `3e:6`, `3f:4`,
  `78:28`, `79:179`, `c8:57` (hex values). Each of 68,886 faces also has an
  unknown 1-byte tag: `00` in 49,514 and `ff` in 19,372. The loader stores
  that tag as-is and only the low byte of each u32 index; those index words
  have zero high bits in all retail faces. The parser preserves these values
  without naming them.
- `.trk` field names from the Blender track exporter (`rbr_track_formats/trk`,
  see `notes/references/README.md`), checked against all retail files where
  noted:
  - Kind `0x14` is the driveline, a cubic Hermite spline: Vec3 position,
    Vec3 tangent, f32 distance along the stage, then two u16 written as 0.
    Retail: the distance increases along every file (86/86) and its step
    matches the chord length (median ratio 1.0004). Points whose tangents
    give `(t0 + t1) · (p1 - p0) <= 0` make the game interpolate wrongly or
    backwards (exporter's check, not verified).
  - Kind `0x16` mesh header: u32 object kind, u8 physics material id (the
    unknown byte above), u8 flag, u32 soft-volume type (the shape code:
    0 none, 1 box, 2 sphere). The material id is a `.col` surface id
    (`col.md`). Retail values are exactly the exporter's object materials:
    `0x19` spret bush medium, `0x1b`-`0x1d` lov bush, `0x1e`-`0x20` tree,
    `0x38`-`0x3a` rock, `0x3b`-`0x3d` trunk, `0x3e` snowwall, `0x3f` metal
    pole, `0x78` tree stump, `0x79` bendable tree, `0xc8` script character.
    Boxes appear only on snowwall and script character, spheres only on
    bushes, trees and bendable trees (retail). The exporter calls a soft
    volume a soft collision: a force on the car instead of a hard contact.
  - Object kinds (exporter, with its masses): 1 traffic cone (5 kg),
    2 concrete block (3003), 3 street sign (10), 6 banner fence (100),
    7 log (33), 8 log pole (1000), 9 wooden fence bar (333), 10 wooden fence
    pole (2000). Kind 0 is static. Retail also has 12 kind-15 meshes with a
    material; the exporter treats that as bad data. It also says the game
    rewrites the kind of meshes named `CONE` or `BANNER_STAND`; not checked.
  - The flag byte: the exporter calls it `use_local_rotation` and writes 0
    for dynamic kinds. In the loader, nonzero makes each placement point at
    its own mesh instead of the shared static id. Retail: 1 only on 4,708
    kind-0 meshes with material 0. The name is unproven.
  - Face tag: material id, `0x00` undefined or `0xff` spectator. The
    exporter says the spectator tag only suppresses the red-screen
    penalty; not checked.
  - Placement: u32 key (matches an interactive object in `.lbs` for dynamic
    kinds), Vec3 position, Vec3 scale, quaternion `x, y, z, w`.
  - Exporter limits: 30 vertices, 84 edges and 56 faces per mesh, at most
    `0x2000` meshes (the last matches the loader's log check).
- `.col` parses full collision trees and cell geometry; see [col.md](col.md)
  for loader evidence, the exact layout, and retail observations. All 86
  retail files pass exact tiling, graft, and vertex-index checks.
- `.dls` parses its header, all ten directory entries, fixed-width tables,
  animation records and referenced arrays, nested variable groups, replay
  streams, sphere table, byte block and optional world vector according to
  `FUN_00479020` and `CReplayManager::ReadStream` (`0x498840`). Tables 2,
  3 and 5 are decoded as the loader reads them (`rust/rbr-formats/src/dls.rs`:
  `DlsTrigger`, `DlsCamera`, `DlsTable5Record`): table-2 rows are trigger
  records `{ids, spline group/id (matched against table-1 groups; `(0, 0)`
  is the no-match default), position, rotation degrees (stored as radians),
  raw word 10, word-11 float (nonzero sets flag bit 1), raw words 12/13}`;
  table-3 rows are camera records `{id, look-at}` (both resolved through
  the trigger-id search except the raw sentinels `-16`/`-15`/`-14`/`-1`)
  plus two raw words, a mode word (bits 0/1 only) and a raw word;
  table-5 rows are `{trigger id (must resolve), name offset into the
  string block, two raw words, three floats}`. All 86 retail files pass;
  14 replay streams begin with version 6. Table 9 stays two raw words in
  file order (the loader stores them swapped; which is the id is
  unproven).
- `.dls` names from the Blender track exporter (`rbr_track_formats/dls`). Its
  record widths match the loader layout above. These names are not checked
  against game code:
  - Directory entries 0-9: animation sets, splines, trigger data, animation
    cameras, track emitters, helicams, sound emitters, name block, finish
    registration zone, animation ids.
  - Table 0 entry: `{set descriptor offset, set size}`. The 17-word record is
    a name offset, eight counts and eight offsets. The sections are sig
    trigger data `{id, trigger kind}`, section channels `{index, distance,
    is_exit}` (replay camera cuts), animation channels `{index, distance,
    lead time s}` (Lua animations such as marshals and deer), bool channels
    (nested), real channels `{id, kind, count, offset}` with 7-word control
    points, pace notes `{id, flags, distance}` (`stage-markers.md`),
    animation data `{name, start, end, speed, flags}`, and rally school (the
    replay stream at word 16).
  - Real-channel control point: u32 interpolation (0 cubic Hermite, `0x200`
    linear, `0x400` constant; other bits ignored), Vec2 `(distance, value)`
    and two Vec2 tangents. Trigger kinds: 1-3 position x/y/z, 4 position from
    spline, 5-10 two rotation triples, 11 activation, 12 camera FOV, 13
    replay camera change, 14/15 fog end/start, 16 zfar, 17-20 unused
    helicam, 21-24 sound left/right delay and reverb.
  - Table 1 (splines): `{group, id, count}` followed by 11-word points
    `{interpolation, position, end tangent, start tangent, value}`.
    Table 2 (trigger data, 14 words): `{id, id, spline group, spline id,
    position, pitch/roll/yaw degrees, three unused angles, active}`.
    Table 3 (cameras, 6 words): `{id, look-at (0 fixed, -1 car, -14 chase,
    -15/-16 driver, else a trigger id), fov, shake, mode (0 steady,
    1 shake, 2 helicam), znear}`. Table 4 (track emitters): `{name offset,
    trigger id, squared activation distance}`. Table 5 (helicams): unused
    by the game, present only in stages 56 and 62. Table 6 (sound emitters):
    Vec3 + radius. The exporter calls these bird sounds; the game uses them for
    `audio\ambient\siren.wav` (`E31_5d00`, `0x4d5d00`: on a camera change in
    app state 8, car above the speed threshold, new camera inside a sphere;
    see `../marks-sound-ghost.md`). Table 8: finish
    registration zone, Vec3 + radius (`0xffffffff` when absent). Table 9:
    `{animation id, name offset}`.
  - On load errors the game writes `dlserrors.log`.

## Unknowns

- `.trk` driveline and mesh fields are named above from the exporter; the
  mesh flag byte and the kind-15 meshes are resolved from the loader
  (`FUN_0046e300`, `src/rbr/RBR_E10_Loaders.cpp`), while the two driveline
  words and the mesh material byte stay open. The flag byte is the
  static/dynamic byte: 0 is static (the mesh shares one collision id
  across all its placements), nonzero is dynamic (each placement points
  at the mesh with `0x30` ORed into its flags). Retail uses only 0/1, and
  1 only on kind-0 meshes with material byte 0. Kind 15 falls in the
  loader's default branch (runtime type 1, like kind 0/5); retail has 12
  kind-15 meshes, all the same static sphere bush (`HO_BUSH01.KSH`,
  Hokkaido stages). The driveline words are stored swapped (first word
  at element `+0xe`, second at `+0xc`); they are zero in all 27,458
  retail records and no consumer is traced. The mesh material byte (mesh
  `+0x30`) has no traced consumer either; retail values sit in the `.col`
  surface-id space (boxes only with 62/`0x3e` snowwall and 200/`0xc8`
  script character, spheres only with bush/tree/bendable ids), matching
  the exporter's material-id claim, but that use is unproven. Start, split, finish and pace-note markers are keys of the `.dls`
  `Driveline` set; see `stage-markers.md`. `FUN_0046ed20`
  dispatches only `(3,0x14)` to `FUN_0046a530` and `(3,0x16)` to
  `FUN_0046e300`; neither handler names split/checkpoint/start/finish fields.
  `.dls` contains spheres (`FUN_004772a0`) and opaque table fields, but no
  loader evidence here ties them specifically to checkpoints or stage
  boundaries. Do not infer those meanings from position alone.
- `.col` brake-wall leaf-item encoding is resolved (above) except the
  bits 14/15 flag meanings (exporter names only); surface id to physics
  name is resolved (above). Some triangle attribute meanings stay
  exporter-named; see [col.md](col.md).
  `.mat` class-byte-to-surface mapping is resolved: each class byte is the
  surface id into the exe table at `0x7caf38` (see [col.md](col.md)).
  Downstream of `FUN_0046a290`, `E44_TableBuild` (`0x520190`) keeps the
  width byte and the cell pointer per run (the height byte is never read)
  and the sampler (`0x520010`) reads `cells[clamp(floor(width * a))]` for
  the single float `a` the collision queries pass (the second float
  argument is never read), i.e. the first 16 cell bytes only. The `a` the
  queries pass is the barycentric-weighted low nibble (`/15`,
  `DAT_007399bc` = 1/15); the high nibble is computed but discarded.
  `.fnc` group/post fields are resolved from `CFenceHandler::Read`
  (`0x6038c0`) and the renderer (`src/rbr/RBR_E90_Handler.cpp`,
  `RBR_E90_Renderer.cpp`): group `{fence type, post type, material, rail
  material}` + bounds, post `{position, bounds center/half to the next
  post, packed BGRA color}` (the color lands in the vertex color slot in
  `EmitPosts`/`EmitRails`).
- `.lbs` type-2 chunks are resolved from the HedgeHog3D handlers
  dispatched by `CHH_TrackLoader::Load` (`0x10052220`; framing is u32
  header-len (= 8: type, subtype), header, u32 payload-len, payload):
  `(2,10)` visibility (`HH21_Block2a`, `0x10056d90`): count + 24-byte
  entries of two vectors, lane = entry index; `(2,11)` start location
  (`HH_H23_Block2b`, `0x10059990`): 0x40 skipped bytes (4x4 matrix) + two
  vectors; `(2,12)` driveline points (`HH21_Block2c`, `0x10056b20`): count
  + vectors (equal to the `.trk` driveline with y/z swapped, 86/86);
  `(2,14)` animation objects (`HH_H23_Block2e`, `0x10059230`): count +
  items of name, 3 words, 2 material (script path) names, 5 vectors,
  packed color; `(2,15)` container objects (`HH_H23_Block2f`,
  `0x10059690`): count + items of name + 3 words; `(2,16)` track vectors
  (`HH_H23_Block210`, `0x100597c0`): two vectors (second is a unit vector
  in retail); `(2,18)` visible-object vectors (`HH_H23_Block212`,
  `0x10059850`): 60-byte records of 4 vectors + 3 words tiling the
  payload (no count word; the loader iterates its visibility count, which
  equals the `(2,10)` entry count in 86/86 files). Parsed by
  `rust/rbr-formats/src/lbs/`; see [lbs.md](lbs.md).
- `.bws` has no retail files and is not consumed by the 1.02 loader.
- What consumes `TextureFilenameMap.dat` and `maps.m2b`/`maps.rmb`
  (in `misc.rbz`). H21 answers the first half: HedgeHog3D's
  `CHH_TrackLoader::LoadFilenameMap` (0x10054400) reads the u32 count +
  NUL-separated `archive-path \0 short-name \0` pairs into a
  `map<string,string>`, and `ResolveFilename` (0x10054380) maps every
  track texture name through it (identity when absent).
- Time-of-day variants: `Tracks.ini` says "time of day settings are added to
  [TrackName], as well as the extensions" — `FUN_00466f10` implements this
  with the `"%s%s%s.%s"` sprintf above; exact variant strings unexamined.
