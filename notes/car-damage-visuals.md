# Car damage visuals (body deformation, detached parts, glass, dirt, debris)

How the 1.02 game shows car damage on screen: the signals that carry it from the physics damage handler
to the renderer, what each visual reacts to, and the per-car data. The damage model itself (zones,
functors, levels) is in [`physics/damage.md`](physics/damage.md). Addresses are `RichardBurnsRally_SSE.exe`
1.02 unless marked HH (HedgeHog3D) or LM (LocoMotion). Labels: **loader** = read from the decompile or our
reconstruction of the named function; **file** = checked on retail data (counts say over which files);
**inferred** = plausible, not proven. Car files: [`formats/cars.md`](formats/cars.md), [`formats/sgc.md`](formats/sgc.md),
[`formats/shaders.md`](formats/shaders.md).

## Summary

Physics damage is a set of 64 zone values in `CCarDamageHandler` (0 to 1, at `handler+0x3808`). The handler
emits one `CSigCarDamage` per impact and per level reset. The signal points at the zone values (HH copies them),
and carries an event code. Four things react to it:

1. **Body deformation and scrape.** HH copies the zone values into one global array (`g_carDamage`,
   `0x101dd580`). Most damageable body panels (sgc subtype 3, section 6) have a list of `(vertex, weight)` pairs per zone.
   `CGeomObjectCar::UpdateDeformation` (`0x10003b80`) moves the panel's vertices by the summed weights and
   writes `weight * 0.01` into the colour stream. The paint shader uses that as the scrape amount.
2. **Windscreen and window cracks.** An event code 3 or 4 (set by a failure functor) makes HH project the
   impact onto the window panes' UVs, so `Textures\WS_Broken.dds` shows as an overlay.
3. **Detached parts.** Physics emits `CSigDetachFromParent` with one dword, the body's animation-object id (section 5). HH flags the node
   and draws it in a second pass. The physics side (not visual) is in the mounted and hinged body code.
4. **Debris and smoke.** `E83System::EmitDebris` (`0x5df5f0`) emits debris and smoke emitters for impacts.

Separately, tyre spray (`CSigTireParticles`) builds up dirt: a dirt level in the car shader constants and a
"Dirt" render target that the paint shaders blend in. Glass-light breakage uses the damage handler's light
block (see [`car-lights.md`](car-lights.md)). The LM in-car windscreen fit (`CLM_CharacterCar::OnWindscreen`)
handles the glass mesh in the cockpit view. Its emitter was not found.

Everything here is presentation. The one exception is the physics detach, which changes the physics body, not
the mesh (section 8).

## 1. Signals and payloads

| Signal | Emitted by (exe) | Payload as read by the consumers | Consumers |
|---|---|---|---|
| `CSigCarDamage` | `E36DamageOps::Slot1` `0x4f12a0` (impact), `Slot4` `0x4f1000` (level reset) via `CSignalManager::CMultiSignal1<CSigCarDamage>::Emit` `0x4ef1d0` (manager `+0x9f20`) | record of `0x40` bytes at `handler+0x3e60`, then `m_data` (`+0x40`) and `m_count` (`+0x44`) | HH `CHH_World::OnCarDamage` `0x10066e90`, HH `CHH_Car::OnCarDamage` `0x10016d50`, E83 `EmitDebris` `0x5df5f0` (via `E83System` `m_connDamage`, `RBR_E83_Sys.h:232`) |
| `CSigDetachFromParent` | `CMountedRigidBody::StepBig` `0x522100` and `FUN_0051fd00` (called from `CCar::Clear480` `0x4e58f0`); manager `+0xba58` | one dword, `*(this+0x30)`, the body's animation-object id (section 5) | HH `CHH_Car::OnDetachFromParent` `0x10016ed0`; LM `CLM_CharacterCar::OnDetach` `0x1001cdb0` |
| `CSigWindscreenData` | not found in exe, HH or LM decompiles (no `Emit` call site; manager `+0xe650` only in unwind code) | four points at float offsets 12, 16, 20, 24 (x, y, z, one unread float), read by LM | LM `CLM_CharacterCar::OnWindscreen` `0x1001c340` |
| `CSigTireParticles` | tyre code (`notes/particles.md`) | `m_unk88`, spray amount | HH `CCarShaderManager` `0x10001420` (dirt level), HH `CHH_TextureEffectDirt::...::OnTireParticles` `0x10043a70` (dirt target) |
| `CSigCarDamageSpeech` | `CCarSound::M260` `0x4ae260` and `Me3a0` `0x4ae3a0` (code 6) via `E23DamageSpeechSignal::Emit` `0x4ae0d0` | sound | out of scope (Other systems) |

The `CSigCarDamage` record, as written by `Slot1` (**loader**, `Slot1` decompile lines 264645 to 264688, and
`RBR_E36_Damage.cpp:478-569`):

| offset | word | written by | meaning |
|---:|---|---|---|
| `+0x00` | event code `hit` | `ApplyFunctors` `0x4ec560` (`RBR_E35_DamageB.cpp:278-292`) | 0 = none; 3 = `DmgCFA0` `0x4ecfa0`; 4 = `DmgCFE0` `0x4ecfe0` (**loader**: both return values checked in the decompile) |
| `+0x04` | kind | `Slot1` sets 2; `Slot4` sets 0 | `EmitDebris` runs only for kind 1 to 3 (**loader**, `0x5df5f0`) |
| `+0x08` | functor index `i` | `ApplyFunctors` (`*ent`) | the table entry that fired; HH reads it as `data[2]` |
| `+0x0c` | zone index | `Slot1` (`a3`) | impact zone |
| `+0x10` | impact `dmg` | `Slot1` | used as the debris force |
| `+0x14` | `a5` | `Slot1` | not read by the visual code |
| `+0x20` | position `srcB` | `Slot1` (`this+0x3e80`) | debris origin |
| `+0x30` | position `srcA` | `Slot1` (`this+0x3e90`) | not read by the visual code |
| `+0x40` | `m_data` = `handler+0x3808` | `Slot1` and `Slot4` set `this+0x3ea0` (`RBR_E36_Damage.cpp:486, 566`) | 64 zone damage floats (`0..1`) |
| `+0x44` | `m_count` = 0x40 | ctor (`this+0x3ea4`, `RBR_E36_Damage.cpp:88`) | dwords in `m_data` |

`Slot4` emits the same record with `hit = 0` and `kind = 0`, so only the zone copy has an effect there.

## 2. Load path

Per car, the model (`<car>.sgc`) is loaded by the HH car loader (**loader**, `HH_H7LoadGeomCar` `0x1000acf0`,
`HH_3DSMaxLoaderGeom.cpp`):

1. Flags: subtype 3 (damageable panel) is loaded with flags `6`, so bit 2 sets `m_deformEnabled`. Subtype 1 is
   loaded with flags `4` (see [`formats/sgc.md`](formats/sgc.md) section 5).
2. Stream 0 (the vertex list, 24 bytes per vertex) is copied into `m_deformVerts` (base positions, `0x268`).
   Two new vertex buffers are created: stream 0 (stride `0x18`) and a second VB, stride 4, stream 7 (`HH_3DSMaxLoaderGeom.cpp:381-382`).
3. The morph block (subtype 3 and `auxFlags` bit 24) is read by `HH_H7LoadMorph` `0x1000bdd0` into `+0x160`.
   That object is the holder `UpdateDeformation` reads its offsets from (section 3).
4. The per-zone pair lists (the sgc `listCount` entries) are read into `m_morphs` (`0x278`). The update uses the first 64.

`InitDamage` (HH `0x10017f50`, `HH_CarInit.cpp:269`) loads `Textures\WS_Broken.dds` as `m_damageTex`. It builds
the damage render state (alpha blend, `SRCALPHA`/`INVSRCALPHA`), takes car shader `0x2e` (VS) and `0x11` (PS), and
sets two random bits in `m_flags15c` and `m_flags160`, one each (`rand() % 2`). `SetupDamagePart` `0x10018330`
matches each part name against `g_HH_DmgParts` (`HedgeHog3D.cpp:93`). That table maps the window meshes to
slots: `e_window_f` to 0, `e_window_rf` to 1, `e_window_rb` to 2, `e_window_lf` to 3, `e_window_lb` to 4,
`e_window_b` to 5. Each entry also has an interior name (`i_window_*`, paired with `k_windscreen_*` in the second column, used by `StoreLightData`). For each match it
averages the part's vertices into `g_HH_AvgTab` (the part centre) and allocates a `SHH_CarLightUV` array per
vertex.

## 3. Per-frame: deformation (body panels)

`CGeomObjectCar::Update` `0x10003b60` calls `UpdateDeformation` `0x10003b80` when `m_deformEnabled` is set
(**loader**, `HH_GeomCar.cpp:63-78, 168-290`).

1. **Change test.** Compare the object's cached copy `m_damage[64]` (`+0x164`) with `g_carDamage`. If equal, return.
   Otherwise copy the 256 bytes.
2. **Zone weights.** Clear `m_weights` (`+0x288`, `m_unk118` entries). For zone `j` (`j < min(listCount, 64)`), each
   pair `(idx, w)` in list `j` adds `w * damage_j * 100` to `m_weights[idx]`, clamped to 100. Pairs are processed
   4 at a time, then a tail.
3. **Vertices.** The holder (`+0x160`, the `CHH_ModifierMorph` the morph block loads) is the offset source:
   `holder+8` points at its targets, and target 0's `+4` is the array of per-vertex deltas (vec3), the sgc morph
   target (**asm**, `0x10003b80`). For each vertex `i`: `pos = m_deformVerts.pos + delta_i * w_i`, the normal and
   tail words are copied unchanged from `m_deformVerts`, and the colour stream gets `w_i * 0.01`. The VBs are locked
   with `0x2800` each frame.
4. **Colour use.** `CarPaint` reads that colour as the damage weight (`oD0.w`, scrape amount). See
   [`formats/shaders.md`](formats/shaders.md), "Car vertex attributes".

Checked at runtime (snapshot `vcu-tanner-damage.snap-11827`, the Corolla after its impacts on Tanner): the zones at
`*(car+0x9f4)+0x3808` equal `g_carDamage`, and `m_weights` of all 30 deforming meshes equal the weights
`rbr-render` computes from the sgc lists, bit for bit (`rust/rbr-render/tests/damage_data.rs`). The constant 100 is
`0x101b42a8`. Normals are not rotated, so a dented panel keeps its undented shading.

Only the body panels with subtype 3 deform. Subtype 1 meshes with lists (for example `i_body`) carry the same
kind of list, but `m_deformEnabled` is clear for them, so they never deform (**loader** + **file**, section 6).

## 4. Per-frame: window cracks

### 4.1 Event code to window

`ApplyFunctors` `0x4ec560` calls each functor in the impact's table. A non-zero code is kept when its threshold `e[7]` is at least the current `best` (`RBR_E35_DamageB.cpp:289`). The
two codes that matter here (**loader**, decompile of `DmgCFA0` and `DmgCFE0`):

- `DmgCFA0` `0x4ecfa0` returns 3. It needs level > 1, `b < a`, and its per-index flag `handler+0x3dd8+4*i` set. It
  clears the flag. Our source calls it "head-gasket failure" (`RBR_E35_DamageB.cpp:694`).
- `DmgCFE0` `0x4ecfe0` returns 4. It needs `b < a` and flag `handler+0x3dc4+4*i` set. It clears the flag. Our source
  calls it "windshield failure" (`RBR_E35_DamageB.cpp:708`).

The names are our comments, not verified against RTTI. The window they reach is what HH does with the code
(section 4.2), and that mapping is the same as the names only if the names are right (Open 2).

### 4.2 HH side

`CHH_Car::OnCarDamage` `0x10016d50` (**loader**, `HH_Car.cpp:358-395`), reading the record as ints:

- Code 3: `Unk16f00(0, data[2])`. Slot 0 is the front window, with `data[2]` (the functor index) choosing bit 2 or
  bit 4 of `m_flags15c`.
- Code 4 with index != 4: `Unk16f00(index + 1, 0)`. Slots 1 to 4 are the side windows.
- Code 4 with index 4: the rear window (slot 5), both halves: `Unk16f00` is called with 0 and then 1 (**asm**).

The functor index's parity picks the front half: even gives base 0.375 with `u` at most 0.5 and flag bit 4, odd
gives 0.625 with `u` at least 0.5 and flag bit 2 (**asm**). The slot limits are 2, 1, 1, 1, 1, 2 (front, rf, rb, lf,
lb, rear).

`Unk16f00` `0x10016f00` skips the slot unless it has a registered part and the count is under the limit
(`g_HH_CntTab` against `0x101de12c`). For slots 0 and 5 it also checks the car's `m_flags` (bit 2 or 4) so that a
slot is projected once per flag. It then calls `ProjectUVs` `0x100165d0` on the part's vertices, once per part
registered for that slot (`g_HH_FloatTab` plus `0x101ddffc`, the `A` and `B` parts). The per-car bits seeded by
`rand()` in `InitDamage` only pick which of the two flag bits (2 or 4) an entry sets. The crack half comes from
`ProjectUVs` (below).

`ProjectUVs` `0x100165d0` (**asm**) creates a new vertex buffer per projection (`count * 8` bytes, FVF `TEX1`). For
each vertex `(x, y, z)` of the part, relative to the part centre `(cx, cy, cz)`, both transformed by the part's
`m_world` (`+0x40`) in `SetupDamagePart` (`D3DXVec3TransformCoord` `0x100a4ff0`; taken as model space):

- slots 0 and 5: `u = (x - cx) * 0.6 + base`, clamped to the half, then `u = 1 - u` when bit 0 of
  `car[0x15c]` (slot 0) or `car[0x160]` (slot 5) is set (the `rand()` bits of `InitDamage`).
- side slots: `u = (z - cz) * 0.8 + (bit ? 0.625 : 0.375)`.
- all: `v = 0.5 - (y - cy) * 2.0`.

Constants: `0x101b4320` 0.6, `0x101b4324` 0.8, `0x101b4328` 0.625, `0x101b432c` 0.375, `0x101b41d8` 0.5,
`0x101b41e4` 2.0, `0x101b4184` 1.0. `WS_Broken.dds` is 512 x 512 DXT3, white, with the cracks in alpha: one star in
the upper left of the left half, one in the right half.

The overlay draw is `FUN_100161e0` (**asm**): for each slot with a projection, part A (the exterior, when
`car+0x114 == 1`) or B (the interior), with `m_damageRes` (blend `SRCALPHA`/`INVSRCALPHA`, z write on, cull CCW,
clamped sampler, mip bias -2) and car shaders VS `0x2e` and PS `0x11` (`CarTextureNoDirt`: `t0 * v0 + v1`, alpha
`t0.a`). It draws the part's first face block once per projection, with that projection's UV buffer.

Projections happen on the impact, not per frame. Each one takes one slot entry, and the count is capped
by the slot's limit (`g_HH_CntTab` against `0x101de12c`).

## 5. Per-frame: detached parts

The payload is an int id (settled at runtime, snapshot `vcu-tanner-damage.snap-11827`): `body+0x30` of each car sub-body
equals the `m_animObject` (`+0xcc`) of the HH node of the same name. Hinged bodies (vtable `0x73d23c`): `e_door_l`
`0x116`, `e_door_r` `0x10c`, `e_bonnet` `0x12e`, `e_exhaust` `0xfc`. Mounted bodies (`0x73e62c`): `e_wing` `0x100`,
`e_bumper_b` `0x129`, `e_bumper_f` `0x126`. Wheels (`0x74288c`): `e_wheel_gravel_lf` `0xd7`, `rf` `0x146`, `lb` `0xd9`,
`rb` `0xdb`. LM's `CLM_CharacterCar::OnDetach` passes the dword to `LM_FindNodeByName` `0x1001cab0`, whose decompile
calls a node virtual (`vtable+0x10`) and compares no string, so its `char*` type is ours and probably wrong: LM
likely matches the same id (**inferred**). The original reaches LM's handler at step 11827 of tanner-damage (the pure
host stops there, `notes/rust-plan.md`, gate B) and does not crash. By `snap-12000` the left door (hinged body `0x116`)
is off the car and HH's `e_door_l` node has `m_unkD2 = 1`, the other nodes 0: HH found the node by that id.

`CSigDetachFromParent` reaches HH in `CHH_Car::OnDetachFromParent` `0x10016ed0` (**loader**, `HH_Car.cpp:405-410`):
`FindByAnimObject(id)` on the car root, then `SetUnkD2Recursive(true)` on the found node and its children
(`HH_Node.cpp:164`, `0x10034310`).

The flag `m_unkD2` (`0xd2`, `HH_Node.h:93`) decides the pass. `CShaderBase::Render(pass)` `0x1007b0b0`
(`ShaderBase.cpp:52`) draws only the objects whose `m_unkD2` equals `pass`, so a flagged object is drawn only in `Render(true)`.
`CHH_Car::SetModeAndRender` `0x100154e0` draws the `Render(true)` lists. `CHH_Car::Render(false)` skips the
flagged objects. So a detached part is drawn once, from a separate list, and keeps being drawn (**loader**, the
pass logic; **inferred**, that nothing else hides it). Its transform keeps following its animation object, which
the physics drives after the detach (**inferred**; not traced).

The physics that raises the signal is in section 8.

### 5.1 Rust renderer (`rbr-render`)

- **Deformation and scrape.** `damage::vertex_weights` is `UpdateDeformation`'s sum (first 64 lists, clamp after each
  add); each damageable mesh moves by `delta * w` (`car::Car::dent`, `playback::Playback::set_damage`, with the same
  change test). The colour alpha is `w * 0.01`. The paint shaders' scrape (`CarPaint1.pso`, `CarPaintInternal.pso`:
  `lrp r0, sat(v0.w * t3.a), t3 * v0, paint`) is drawn as a mix towards the lit scrape texture (friendly names
  `Scrape` and `InternalScrape`). The renderer's lighting is applied to the scrape texture, not the shader's exact
  terms.
- **Cracks.** Each window mesh gets overlay meshes: for the front and rear one per half, for the sides one each.
  They are shown as `CSigCarDamage` events take slots (`damage::Cracks`, the limits and halves above), with UVs from
  `ProjectUVs`'s mapping. The `rand()` bits are taken as clear (no mirror, side base 0.375). The overlays are blended
  without z writes (the game writes z with `LESSEQUAL`; our depth test is `Less`, which would hide the second half on
  the same pane).
- **Detached parts.** Not drawn differently. In the game a detached node stays drawn and follows its animation
  object, and the viewer has no sub-body poses (the recordings hold the car block and the wheels, not the bonnet,
  doors or bumpers), so it keeps them at rest.
- **Crack texture (dmgvis-1, ours).** The overlays now sample a generated spider-web decal (`crack_decal.rs`) with
  its two impacts at `WS_Broken.dds`'s star positions (`(0.06, 0.17)` and `(0.77, 0.62)` in uv), in place of the file.
- **Dirt (dmgvis-1).** `rbr-render/src/dirt.rs` and `shader.wgsl`: the level of section 7 from the tyre payloads'
  mass, the `Dirt` map on `CarPaint`; the dirt colour is ours (the unused preset). Interior panels detach and dent
  with their exterior twin (ours, `car::detach_part`, `car::follow_dents`).
- **Inputs.** `viewer --damage` and `--crack` set zones and events by hand. In live mode the live process writes a
  `DMG1` record after each HUD record (`rbr_recording::damage`: the 64 zones, and the handler's last `CSigCarDamage`
  code and functor index). The record stays in the handler until the next impact, so the viewer sees the last event
  again each frame; a repeated event takes no new slot. Recorded drives carry no damage.

## 6. Retail survey: deformation data

Survey (throwaway scripts in `/tmp/car-damage-visuals-scripts/`, not committed) over the `.sgc` of all eight retail
cars, with the record framing and mesh body parse from [`formats/sgc.md`](formats/sgc.md) section 5. All 8
files parse, and every record ends at its length.

Per car: mesh counts by subtype, panels with per-zone lists, and total pairs.

| Car | subtype 3 (damageable) | subtype 3 with lists | subtype 1 with lists | total pairs |
|---|---:|---:|---:|---:|
| ACCENT | 36 | 30 | 1 (`i_body`) | 49,340 |
| COROLLA | 37 | 30 | 1 (`i_body`) | 50,700 |
| IMPREZA00 | 30 | 24 | 7 | 62,728 |
| IMPREZA03 | 32 | 24 | 6 | 48,120 |
| LANCER_EVO_VII | 41 | 29 | 1 (`i_body`) | 53,344 |
| PEUGEOT_206 | 41 | 31 | 1 (`i_body`) | 43,060 |
| XSARA | 35 | 29 | 3 (`e_window_f`, `i_window_f`, `i_body`) | 37,460 |
| ZR | 35 | 27 | 4 | 42,884 |

Facts that hold for all eight cars (**file**):

- Every panel with lists has `listCount = 64`. Panels without lists have `listCount = 0`. No other count appears.
- Zones used are 1 to 31 except 19. Zone 0, zone 19 and zones 32 to 63 are empty in every car, so no damage zone
  above 31 moves any vertex, and zone 19 never does.
- Every pair index is below the panel's vertex count. Weights are in `0.25` to `1.0` (no zero or negative weight).
- The `e_body` panel has 30 non-empty zones in every car. ACCENT's `e_body` has 7,524 pairs; COROLLA's has 10,036.
  Its non-empty zone lists run from 38 to 629 pairs (ACCENT) and 60 to 1,132 (COROLLA).
- Panels with lists and a morph target: the target's `perTarget` equals the vertex count in every such panel.
- 8 subtype-3 panels have lists but no morph target (all in COROLLA: `i_window_f`, `i_window_b`, `i_door_l`,
  `i_door_r`, `i_window_lf`, `i_window_rf`, `i_window_lb`, `i_window_rb`). This matches the 8 subtype-3 meshes
  without a morph block in [`formats/sgc.md`](formats/sgc.md) section 5.1.

Panels with lists, by name (the set is the same in each car, when present): `e_body`, `e_bonnet`, `e_bumper_f`,
`e_bumper_b`, `e_bumpervent(s)`, `e_door_l/r/b`, `e_window_f/lf/lb/rf/rb/b`, `e_light_lf/rf`, `e_wing`, plus the
interior twins `i_bonnet`, `i_bumper_f/b`, `i_door_*`, `i_window_*`, `i_wing`, and `i_body` (subtype 1, not deformed).

Subtype 3 meshes without lists: the `pc_e_window_*` panels (five or six per car, in every car). They
have a morph target but no pair lists and are not in `g_HH_DmgParts`. Their `<car>.ini` entry uses the material
`external transparent::tire`, shader `CarTextureNoDirt`, and `Predicate = NotUsed` (XSARA). Their role is not
traced (Open 5).

The lists are the sgc `listCount` entries (`formats/sgc.md` section 5 step 16). Section 12 of that note lists their
meaning as unknown. This note's finding answers that: the lists are the per-zone deform pairs read by `UpdateDeformation`.
`formats/sgc.md` was not edited.

## 7. Other visuals: dirt, debris, smoke, fire

- **Dirt on paint (file + loader).** `CSigTireParticles` raises the car's dirt level in `CCarShaderManager`
  (`0x10001420`, `m_unkBC`, capped at 0.9, [`formats/shaders.md`](formats/shaders.md)). The same signal feeds
  `CHH_TextureEffectDirt::...::OnTireParticles` `0x10043a70` (`HH_TE_A.cpp:253`). It adds the amount times `1e-5`
  to `m_dirtA` and `m_dirtB` (capped at 0.9). When `m_dirtB > 0.01` it sets the texture manager's `m_effectsDirty`,
  so the "Dirt" render target is redrawn. The car paint shaders read "Dirt" as their dirt stage (`ShaderCar.cpp:1463,
  1521, 1566`).
- **The texture effects never run in 1.02 (file + loader, presentation5).** `CHH_TextureEffectDirt` (redraws `External`
  from `External[Org]` and `Dirt`) and `CHH_TextureEffectTireDirt` (`HH_TE_C3.cpp`: Init `0x100462d0`, handler
  `0x10046820`, render `0x100468c0`; redraws `ExternalDynamic` as `lerp(ExternalDynamic[Org], (0.271, 0.224, 0.137),
  TireDirt.a * level)`, its own copy of the level) are made only by `CTextureManager::LoadEffects` (`0x100499b0`, from
  `CHH_World::Unk20` `0x10066610` with the car's `_shaders.ini`): one per enabled key of an `[Effects]` section, each
  followed by a `TireDirt` effect. No retail or 1.02 car `_shaders.ini` has an `[Effects]` section, and none names the
  `External`, `ExternalDynamic` or `Gloss` textures they draw into (the friendly names in use are `Dirt`, `TireDirt`,
  `Scrape` and `InternalScrape`). What the game shows is the shader side alone: `CarPaint` by the level, `CarTexture`
  and `CarTire` by their masks (`formats/shaders.md`).
- **Where the dirt comes from (loader).** One level per car (`CCarShaderManager`, `m_unkBC`): every `CSigTireParticles`
  payload, from each of the four wheels and on every surface, adds its mass (`+0x88`) times `1e-5`; nothing keys it by
  wheel or surface beyond the mass the tyre code gives (snow spray dirties the car in the same colour). `CarGlass` binds
  `Gloss` and `Scrape` and samples neither, so 1.02 has no dirt on the glass; `rbr-render` adds a film of its own
  (`dirt::grime`).
- **Dirt textures per car (file).** Each car folder has `<car>_external_mud.dds`, `<car>_external_dynamic_mud.dds`,
  `<car>_external_scrape.dds` and `<car>_internal_scrape.dds`. Eight cars, 4 files each (32 files). Which slot binds
  the dynamic file is not traced (Open 6).
- **Debris and smoke (loader).** `EmitDebris` `0x5df5f0` runs for kinds 1 to 3. The force is the impact `dmg`
  (`+0x10`), and it grows by up to 5 times when the car's tilt (`acos` of the up vector) is over pi/4. Four emitters
  get counts from tables `g_e83DebrisA/B/C/Smoke`: `g_debris_a_emitter`, `_b_`, `_c_` and `g_debris_smoke_emitter`
  (`RBR_E83_A.cpp:346-410`). The position is the record's `srcB` (`+0x20`). The per-particle scale is `0.025` for
  A, B and C and `0` for the smoke emitter.
- **Engine fire and steam (loader).** `E36DamageOps::EngineGate` `0x4ef680`, at level > 2 and `b < a` while `+0x3b20` is set, clears `+0x3b20`, `+0x3a94`
  and `+0x3c00`, sets the light block's refresh word `+0x3b24` to 1, and
  with a random chance below `DAT_0072dd78` emits `CSigEngineFire` (`0x4ef290`, manager `+0xb840`). Fire and steam
  emitters are in [`particles.md`](particles.md).
- **Lights (loader).** The glass and works flags of the light block are in [`car-lights.md`](car-lights.md) (lamp
  glass switches `lightglass_*`, `Predicate = on`). Which event clears them was not found (Open 7).

## 8. Gameplay side: physical detach

The physics side of detach is not visual, but the visual depends on it. Two functions emit `CSigDetachFromParent`:

- `CMountedRigidBody::StepBig` `0x522100` (`RBR_E45_MountedC.cpp:82-147`). It runs when `this+0x44` and `this+0x18`
  are non-zero. It unlinks the body from the car (`this+0x18`, `UnlinkA`), clears `this+0x44` and `this+0x3d0`
  (the owner), re-registers the body as a wall (`RegWall`), and emits the detach with id `*(this+0x30)`. The mounted
  bodies in `physics.lsp` are the bumpers and the wing (`MountedBody/CAR_BUMPER_F`, `CAR_BUMPER_B`, `CAR_WING`,
  [`formats/physics-lsp.md`](formats/physics-lsp.md)).
- `CCar::Clear480` `0x4e58f0` (`RBR_E34_State.cpp:222`), called from the hinged-body step (`RBR_E44_SimB.cpp:690-695`)
  when the hinge reaction projection exceeds `+-2000` (`bad`) and `def+0x50c == 0`. It clears the matching `+0x480`
  slot and calls `FUN_0051fd00`, which emits the detach (the decompile shows the call; our reconstruction gates the
  clear on `SimFD00`). The hinged bodies are the doors and bonnet (**inferred** from the physics names; see
  [`formats/sgc.md`](formats/sgc.md) section 12).

Both paths change the physics body, so they are gameplay (contacts and collision). The visual result (HH's
node flag, section 5) only follows. The trigger conditions (sub-body mode words, `unreached.csv` entries
`0x51fd00`, `0x522100`) are in [`physics/coverage-gaps.md`](physics/coverage-gaps.md).

## 9. Windscreen fit (LM, in-car view)

`CLM_CharacterCar::OnWindscreen` `0x1001c340` (**loader**, `CLM_CharacterCar.cpp:1003-1075`) reads four points at
floats 12, 16, 20 and 24 (x, y, z, and one unread float each) and computes min and max boxes to fit the glass to them.
It is the driver's windscreen, for the in-car view only. No emitter was found (section 1), so whether it fires in
1.02 is open (Open 3). The exterior windscreen is the HH `e_window_f` path (section 4).

## 10. Wheel and suspension node names (not damage)

LM `CLM_CharacterCar::Unk1c770` `0x1001c770` (`CLM_CharacterCar.cpp:750-794`) registers, for each wheel corner, the
nodes `e_wheel<kind>_<corner>` and `Point_e_wheel<kind>_<corner>` through `Unk1b880` `0x1001b880`, with kinds
`hang`, `prop`, `stag` and `susp`. The retail models have these nodes (`e_wheelhang_lf`, `e_wheelprop_rb`,
`e_wheelstag_rf`, `e_wheelsusp_lb`, ...; 20 to 40 vertices each, lists 0, no morph). Nothing in the damage path
references them. Their use is wheel kinematics, not damage (**file**, **loader** for the names). So the `hang`, `prop`,
`stag` and `susp` names are wheel-corner nodes, not damage parts.

## 11. Relevance

Presentation: body deformation, scrape, cracks, dirt, debris and smoke, and the glass fit are rendering, particle
and sound work. The one part that changes the simulation is the detach (section 8), and it is in the physics
code, not in these visuals.

## Open

1. **Detach payload type.** Settled: an animation-object id, as HH reads it (section 5). LM's `char*` parameter type
   is ours and probably wrong.
2. **Functor to window map.** The names `DmgCFA0` (head gasket) and `DmgCFE0` (windshield) are our comments. HH
   treats code 3 as the front window and code 4 as the side or rear window (section 4.2). Which functor index sits in
   which table entry comes from the 13 `AddFunctor` calls in `E36DamageOps::InitTables` (`0x4ef910`, see
   [`physics/damage.md`](physics/damage.md)). Settle by reading that table.
3. **Windscreen signal emitter.** No `Emit` for `CSigWindscreenData` in the exe, HH or LM decompiles. Settle by
   searching the exe for the manager offset `+0xe650` written through a different template, or by a runtime
   breakpoint on the emit.
4. **Morph block use.** Settled: the holder is the morph block (`+0x160`), and its target 0 holds the deltas
   (section 3).
5. **`pc_e_window_*` panels.** They have a morph target, no lists, `Predicate = NotUsed`, and the tyre material. Their
   role (a spare glass overlay, a pre-crack variant, or unused) is not traced.
6. **Dynamic mud file.** `<car>_external_dynamic_mud.dds` appears in `<car>_shaders.ini` (`Texture3` / `Filename`).
   Which material and slot bind it, and whether it is the "Dirt" render target's source, is not traced.
7. **Light-block clearing.** The lamp works and glass flags (`E36LightBlock`, `+0x3b24`) have `Reset`, `Read` and
   `Write` only. No setter that clears a lamp or glass flag was found. `EngineGate` sets the refresh word only.
8. **Units and gate of `dmg`.** Shared with the physics note (`physics/damage.md` open 2): the debris force and the
   `500` gate both use the raw impact `dmg`, and its unit is not established.
9. **Global zone array.** `g_carDamage` is one array, written by whichever car's handler emits last. HH does not
   filter by car (**loader**). With several damaged cars on screen, the render may show the last one's zones
   (**inferred**; needs a runtime check).
10. **Detached node motion.** Whether a detached node keeps following its physics body, or freezes at its last
    transform, was not traced (section 5).

## Other systems seen

- `E83System::EmitDebris` `0x5df5f0` and its emitters: particle system (`particles.md`).
- `E36DamageOps::EngineGate` `0x4ef680` emits `CSigEngineFire` `0x4ef290`: fire and steam (`particles.md`).
- `CCarSound::M260` `0x4ae260`, `Me3a0` `0x4ae3a0`, `E23DamageSpeechSignal::Emit` `0x4ae0d0`: damage speech (sound).
- `CHingedRigidBody` step, `RBR_E44_SimB.cpp:690-695`: door and bonnet detach trigger (physics).
- `CCarDamageHandler::ApplyFunctors` `0x4ec560` and the functor table (`E36DamageOps::InitTables` `0x4ef910`): physics
  damage model (`physics/damage.md`).
- `CSigTireParticles` emitters (`EmitTire` `0x5e3050`): tyre spray (`particles.md`).
- Car light setters and `CCarLightSystem` (`0x4f1f00` to `0x4f1ed0`): lights (`car-lights.md`).
