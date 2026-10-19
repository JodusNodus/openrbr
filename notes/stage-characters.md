# Stage and in-car characters (spectators, marshals, animals, crew)

How the 1.02 game spawns and animates the people and animals along a stage.
Addresses are `RichardBurnsRally_SSE.exe` 1.02 unless marked HH (HedgeHog3D)
or LM (LocoMotion). Labels: **loader** = read from the decompile or our
reconstruction of the named function; **file** = checked on retail data;
**inferred** = plausible, not proven. File layouts: `formats/lbs.md`
(items), `formats/sgs.md` (models, animations), `formats/stage-files.md`
(`.dls`).

## Summary

There is no AI and no pathing. Each character is a LocoMotion character at
a fixed spot. A Lua state machine picks which baked animation clip plays
next. Movement (a deer running off, a bird flying away, a marshal pacing)
is keyframed into helper nodes of the `.sga` clips. The car "triggers" a
character through `.dls` keys on the driveline.

## 1. Placement data: `.lbs` (2,14) items and (2,15) containers

The HH loader (`HH_H23_Block2e`, HH `0x10059230`) passes each item to the
exe's `CPC_AnimationHandler::OnAnimObjLoaded` (`0x446090`), which ends in
`E03AnimImpl::AnimLoad` (`0x445980`). That copies it into a 0x84-byte
descriptor (**loader**):

| desc word | from item | use |
|---|---|---|
| 0 | word 0 (object id) | trigger key, `"<name>_<id>"` character key |
| 1 | word 2 (group = container id) | random-group lookup |
| 2/5 | `material_a` | load-script id / text (`load_*.lua`) |
| 3/4 | `material_b` | run-script id / text (`run_*.lua`); 3 = -1 means none |
| 6 | 0 | Lua state number |
| 7 | (set by Load) | LocoMotion character handle |
| 9-11 | `vectors[0]` | position |
| 12-14 | `vectors[1]` | euler angles, degrees |
| 15-17 | `vectors[2]` | scale vector (not applied; see Load) |
| 0x12-0x15 | packed colour / 255 | not traced |
| 0x16-0x1b | `vectors[3]`, `vectors[4]` | group-shared point and extent; not traced |

The script path is resolved against the current rally's folder first
(`E01FrontendName` + `E01ResolveA`), which is how `Scripts/<rally>/` variants
override the generic scripts (**loader**; folder naming **inferred**).
`E01ResolveA` (`0x43cdd0`) opens each candidate with `CFile`; when neither the
rally's nor the generic load or run script exists, `AnimLoad` builds no
descriptor and the item is dropped without a log (**loader**). Ours drops it
too and counts it in the load report (RSF 129 places four scripts no install
has, `notes/formats/rsf-stages.md`).

Containers (2,15) are `{id, kind, count}` (**file**: `RandomGroup01 [34287,
1, 3]`). `PickRand` (`0x445c70`) stores `{id, kind, rand() % count}` for each
one. An item loads only when its group is missing, the group kind is not 1,
or `item word 1 == the picked index`. Kind 1 groups are therefore "spawn one
of N variants", and item word 1 is the variant index (**loader**; retail has
variants 0..2 per group, each holding one to several characters).
For kind 2 groups `ReloadAll` (`0x445eb0`) calls `E01RandSelect(group)`.
That gives `LuaRandom` an exclusion set shared across the group, so members
draw distinct values (for example distinct models) (**loader**).

## 2. Load (`SigPC_LoadCharacters` -> `E03AnimImpl::ReloadAll` -> `E02LuaLoader::Load` `0x441480`)

Per descriptor (**loader**):

1. `lua_dostring(load script)`. Read global `name` and build the key
   `"%s_%d" % (name, id)`.
2. Call `Load(key)`. Any result other than `0` means "skip" (`load_birdstrike`
   returns 1 unless `LuaRandom(1,200) == 1`). The script calls
   `app:LoadCharacter(sgs, ini, shaders_ini, key, tag)`,
   `app:LoadAnimation(sga, "<clip><key>")` and
   `app:AddAnimationToCharacter(key, "<clip><key>")`. The order of the
   `AddAnimation` calls defines clip indices 0, 1, 2...
3. Look up the LM character (`FindCharacter`, LM slot 0x40) and the HH
   character (`IHH_World::Unk6C`). Ask the HH visibility holder for the cell
   that contains the position (`IHH_World::Unk7C`, stored in desc +0x78).
4. LM transforms: position (slot 0x58, `CLM_Character::Unk18710` translation),
   rotation `deg * 0.01745` (slot 0x5c, `Unk187c0`, X*Y*Z), uniform scale from
   the Lua global `scale` (default 1.0; 0.01 birds, 0.001 marshals) (slot
   0x60). Then the clock offset is set to 0 (slot 0x44, `+0xc0`) and the
   playback speed to 1.0 (slot 0x48, `+0xc4`).
5. `CreateCollisionMesh()` returning 1 (marshals) emits a `CSigStuff`
   collider at the spawn position (`E02FireStuff`, `0x440a20`; collision mesh
   `"animationobject"`, material 200 = `.col` "script character"). The
   collider stays at the spawn point and does not follow the animation
   (**inferred**: only the spawn position is passed). Session cleanup
   removes it (`E02AnimEmitStuff`, `0x443e40`).

Successful descriptors go into a live vector, plus an id -> index tree at
impl +0x34.

## 3. Per-frame update (`E02AnimUpdate`, `0x443a10`, called from the app tic)

For each live character (**loader**):

- If its visibility cell is not visible this frame (`IHH_World::Unk80(cell)`,
  holder flag +0xc8), hide it (`IHH_World::Unk58(node, false)`) and do nothing
  else. The Lua scripts only run for visible characters.
- Otherwise show it, and read the time left in the current clip (LM slot 0x54,
  `CLM_Character::Unk10`: `clip end - local time`). When that is `<= 0` and a
  run script exists, `dostring(run script)` and call `Update(state)`. It
  returns `(clip, new_state)`. The state is stored and `clip` starts through
  LM slot 0x4c (`Unk19f90`, which resets the clip clock).

So clips never loop by themselves. Every time a clip ends, `Update` picks the
next one.

## 4. Triggers (car proximity): `.dls` -> `CSigTrigger` -> `Trigger`

`E12_AnimSet::Load3b0` (`0x4783b0`) and `TrigLoop` (`0x477ff0`) emit
`CSigTrigger` (`g_signalManager + 0x53c0`) from the stage `.dls` key lists
(**loader**):

- `TrigLoop` uses rows `{record, flags, distance, lead}` and fires once when
  `distance - lead * v < x`. `x` is the car's driveline distance and `v` is
  `E12_Sampler::Dot150()` (speed along the driveline, **inferred**). The
  character therefore reacts about `lead` seconds before the car arrives,
  whatever the car's speed. The exporter calls these "animation channels
  {index, distance, lead time s}".
- `Load3b0` fires the sorted plain trigger keys forward as `x` passes them
  (and walks back when `x` decreases).

`E02TrigLoader::Fire` (`0x443d00`, the handler's `CSigTrigger` connection)
ignores signals with bit 0 of `+0xc` set. Otherwise it looks up the object
by id (`sig[0]`), runs its run script's `Trigger(state, isStart)` and stores
the returned state (**loader**).

## 5. The run scripts (state machines, `Root/Scripts.rbz`; **file**)

Retail pairs (all 86 stages, 7,041 items):

| Load script (items) | Run script | Behaviour |
|---|---|---|
| `load_bird` (3,500), `load_birdstrike` (16) | `run_threestate` | clip 0 (sitting) loops; on trigger clip 1 (fly off along `D_path`) plays once, then clip 2 forever |
| `load_media_*` slope/flat/hill/fence/rasbrant (~1,330), `load_deer_flat` (21), `load_start_race01b` | `run_threestate` / `new/run_threestate` | same: idle, then a triggered clip (run back, take a photo, deer bolts), then end idle |
| `load_media_stand` (1,198), `load_marshal_stand` (344), drivers, mechanics, prize | `run_singlestate_noidle` | clip 0 forever; `Trigger` does nothing |
| `load_marshal_walk` (231), frontend, chief mechanic | `run_cycletwostate_noidle` | alternates clip 0 (idle) and clip 1 (walk) forever, no trigger needed |
| `load_birdstrike` (72) | `run_birdstrike` | on trigger: `app:PlaceCharacterAtCar(...)`, flight clip, `app:Birdstrike()`, then back to clip 0 |
| `load_end_race01a/b`, `load_start_race01c`, Red Bull cans | own `run_*` | scripted start/finish crowds |

`load_*` scripts randomise the model (`LuaRandom(1,3)` over
`<rally>_marshal_0[012].sgs` and similar) and the clip variant. Kangaroo,
cow, sheep and moose scripts ship, but no retail 1.02 stage item uses them.

## 6. Motion lives in the clips (**file**)

Sampled-PRS translation, first to last sample (model units, before `scale`):
`anim_bird_01` `D_path` moves about 13,000 units (130 m at scale 0.01).
`anim_deer_run_flat` `D_mother` moves 565 and `anim_kangaroo_run_flat`
`D_origo` 488. `anim_walk`'s pelvis paces about 1,250 units out and returns
to its start, so the walk cycle loops cleanly.

## Open

- Exact meaning of `.dls` trigger fields and the plain-key flag bit; which
  `.dls` section feeds `TrigLoop` versus `Load3b0` (offsets only, see
  `src/rbr/RBR_E12_Load.cpp`, `RBR_E12_Mgr.cpp`).
- `Load3b0` in our source drops the isStart flag (`(void)flag`), so the
  signal's +0xc byte is not reproduced there. Check against the binary.
- Uses of desc 0x12-0x1b (colour, group point and extent) and of the
  scale vector `vectors[2]`.
- `PlaceCharacterAtCar` / `Birdstrike` bodies and the rescue-chopper and
  car-flip spectators (`E03CarFlipInit`, `SigInitSpectatorFlipAnimation`).

## Findings from the Rust port (characters-1, `rust/rbr-render/src/characters/`)

- `TrigLoop` rows are the Driveline set's `arrays[2]` `{trigger-data index, distance f32, lead f32}`; the index
  selects an `arrays[0]` `{object id, kind}` row (`rbr_formats::markers::animation_triggers`). Every stage's rows
  name a placed object except 14 of 31 in `track-54_*` (the birds there never fire). `x` is `CCar+0x17c`.
- Scripts are Lua 4.0 (tolua): no booleans, `true`/`false` are globals set in `startup.lua`. Every retail script
  parses except `load_crash_{car,codriver,driver}.lua`, `load_cow_walk_03.lua`, `*_site0_test.lua` and the
  `test_redbull` ones, which are broken in the archive.
- Script paths: `CArchivePath` keeps only the file name, so an item's script is `Scripts/<rally folder>/<file>`,
  else `Scripts/<file>`. The folder comes from the exe table at `0x7C03DC`, indexed by map number / 10.
- Skin records in the `sgs` weight stream are 32 bytes: 4 f32 weights, then 4 i32 byte offsets (`bone * 0x40`,
  `-0x40` = unused). A few vertices of `richard.sgs` and `robert.sgs` have `-0x40` in slot 0 (identity scratch).
  Bone matrix = placement * node world * inverse bind (inferred, checked visually).
- `PRSSampled::Evaluate` is an element-wise lerp of the 12 floats, ends held, `f = 0` when two sample times are within
  1e-4. The `_43 = _42` line in our C++ is a reconstruction bug.
- Visibility cells: every (2,14) item of all 86 retail stages and of the RSF samples lies in a `(2,10)` cell's
  `x`/`z` box (**file**). Ours shows a stage character while its cell's box meets the camera's frustum, the same
  test as `VisSelect` (`characters::cells`), from the frame before (the game uses the last render's flags).
- The 42 dangling `TrigLoop` rows (14 per `track-54` variant) name no placed object: `Fire` finds no descriptor and
  does nothing (**loader**), and ours ignores unknown ids the same way.
- Checked on the tanner drive (`track-54_E`): deer 51378 triggers at step ~2127 and bolts at ~2540, before the car's
  closest approach (~2766); media 52066 triggers at ~22830, with the car closest at ~23459.

## 7. In-car crew (driver and co-driver)

These are not stage items or Lua state machines. They are two LocoMotion
classes (`notes/locomotion.md`, Characters) driven by car signals every frame.
Labels as above; LM addresses from `src/locomotion/`.

**Loading.** `CPC_TrackState::LoadCar` (`0x46d980`) calls
`E09_RunCharScripts` (`0x466710`), which runs `scripts/richard.lua` and
`scripts/robert.lua`. Each one calls `app:LoadHumanCharacter(sgs, ini,
shaders_ini, "sgs\\driver.ini", name, "Driver" | "CoDriver")` and adds one
clip (`richard.sga` / `robert.sga`). `CLM_CharacterLoader::Load` (LM
`0x10015550`) picks `CLM_CharacterDriver` (0xc20) or `CLM_CharacterCoDriver`
(0x5e0) from the type string (**loader**). `sgs\driver.ini` is not in the
retail install (**file**). Its consumer on the HH side is not traced.
`robert.lua` passes `driver.ini` too; only `robert_lowpoly.lua` uses
`codriver.ini`. The `driver_N_M` / `codriver_N_M` models are used only by the
podium and test scripts.

**Seat.** On `CSigCarGeneralState`, `CLM_CharacterIncarPeople::OnCarGeneralState`
(LM `0x10022120`) sets the body rotation from the car quaternion. The seat
offset is rotated and added to the car position. The offsets come from the
car `.sgc` nodes `D_driver_use` / `D_codriver_use`
(`CLM_CharacterCar::Unk1d630`, LM `0x1001d630`). The same function reads
`i_steeringwheel`, `D_hand R/L`, `D_hand R_gear`, `D_hand R_gear p`,
`D_hand R_handbrake` and `D_hand R_handbrake p` as driver-relative targets.
A car without the two gear nodes is flagged gearstick-less (`+0x2e4 = 1`)
(**loader**).

**Body shake (the driver only).** `E45_P3210` (`0x523210`) emits
`CSigIncarShake` from one `CPassenger`, the one at `CCar+0x8b0` (state block
`host = CCar+0x1450`), always with seat 0 (**asm**). The signal is the seat
int at `+0`, then six vec4 (cleared by `0x49fea0`): `+0x10` = `host+0x90`,
`+0x20` = `host+0x80`, `+0x30` = `host+0x70` (the vector the cameras shake
by), `+0x40` = `host+0xa0`, `+0x50` = `host+0xb0`, `+0x60` = zero, never
written (**asm**). The co-driver's stored signal is never written, so his
shake stays zero. On the tanner drive only `+0x70` and `+0x90` move (up to
about 0.05); the others stay 0 (**recording**, the FRM1 car block).
`Unk23000` (LM `0x10023000`, **asm**):

- the seat translation (from `+0x60`) is therefore always zero;
- for the 5 nodes `B_spine01`, `B_spine02`, `B_head`, `B_upper arm L`,
  `B_upper arm R` it takes the vectors `+0x10`..`+0x50` in order:
  `x = normalize(x + clamp((s.x, s.z, s.y), ±0.2))` (tables at LM
  `0x1006c4b0`, from `0x1005a440/444`), `z = normalize(x × y_old)`,
  `y = z × x`, translation kept, children updated;
- then restores both hands (`B_hand L/R`) to their matrices from before the
  shake, so the arm IK absorbs the lean.

**Arms.** `Unk237a0` caches 13 bones (`B_spine01/02`, `B_head`, upper/lower
arm, hand rotation and hand, L/R, thighs) and builds two 3-link IK chains
(upper arm, lower arm, hand). `Unk22ce0` solves both chains towards the
hand targets every update (**loader**). `CLM_IKChain::Solve` (LM
`0x10027190`, **asm**): `X = norm(target)`, `Y` = the unit rejection of the
current elbow from `X`, `a = clamp(((L0² - L1²)/d + d)/2, 0, L0)`,
`h = sqrt(L0² - a²)`, elbow `= aX + hY`; it succeeds only for `0 < a < L0`,
otherwise the arm is left as it was. `Unk22380` then rebuilds the frames:
the upper arm's `x` along the elbow (its `y` from its old `y`), the lower
arm's `x` towards the target (`y` from the upper arm's new `y`, placed at the
elbow), and `B_hand rotation` (the forearm twist) takes the lower arm's `x`
with its own `y`, at the elbow. The hand and the fingers are not touched.

**Co-driver** (`CLM_CharacterCoDriver::Update`, LM `0x1001e360`). Plays
`robert.sga` looping (the clock wraps every 14.133333 s) at scale 0.001,
applies the seat-1 shake (always zero, see above), then runs the arm IK. Nothing reacts to pace notes
or input (**loader**).

**Driver** (`CLM_CharacterDriver`; config `Misc/LM_Driver.ini` in
`Root/misc.rbz`, read by `Unk14`, LM `0x1001ea00`):

- *Steering* (`Unk18`, LM `0x10020cf0`). The car's `i_steeringwheel` node is
  rotated by `steer * MaxSteeringwheelDegrees` (60). The driver clip is not
  played: it is scrubbed. Root time is `Normal * (steer + 1) / 30` with
  Normal = 5, so frames 0..10 hold full left .. full right. The rest of the
  clip is a pose library: gear 15/20/25, handbrake 30/35/40, gear to
  handbrake 45/50. Then the seat-0 shake is applied, and the wheel hand keys
  follow the wheel (**loader**; frame meaning from the ini key names).
- *Hand moves* (`OnCarControl`, LM `0x10020ac0`, from `CSigCarControl`:
  `+0x10` steering, `+0x08` an analogue control the code treats as the
  handbrake, `+0x14` bits 0/1 gear change; the field meanings are
  **inferred** from use). The handler pushes `{move, direction, hold}`
  entries into a 20-slot ring (`+0x3b8`). Moves:
  1 wheel→gear, 2 gear→gear pushed, 3 wheel→handbrake, 4 handbrake→pulled,
  5 gear↔handbrake, 6 "steeringwheel gear" (at the wheel, used when the car
  has no gearstick).
  - A gear change with a stick queues 1, 2, 2 back and 1 back (`Unk22a20`).
    If the hand is on the handbrake it first goes 4 back, 5, and so on.
  - Handbrake on: 3 then 4 with hold set, so the hand stays on the pulled
    handbrake. Handbrake off (value 0): 4 back, then return.
- *Playback* (`Unk22b90`, `Unk24`, `Unk28`). Each move runs for its
  `Animation Lengths` time (0.1-0.3 s). The hand and elbow targets follow a
  Hermite curve between the keys (`Animation Tangents`). The hand-rotation
  matrices blend between the poses sampled at the move's start and end
  frames, the finger nodes are posed from the clip at the matching frame,
  and the IK places the elbows (**loader**).
- Input is ignored while the global object's `+0x50` check (slot 0x10)
  returns 0. `+0x50` is `view[0x14]` of the `PC_GlobalObject` mapping, the
  fence renderer (`0x16fc4f4`); its `Vf10` (`0x603e80`) returns
  `car && CCar+0x1670 <= 0` (`Test1670`, the start hold `m_tOverride`), so
  the hands stay put until the start (**asm**; tanner: the first 415 frames,
  3.72 s).

**Drawn.** The crew's HH shaders are in `world+0x200`. The High renderer
(`RenderQuality=high`, the default; HH `0x100763f0`) draws them only when the
view is not in-car (render option `0xF`, set for camera modes 4-6) and the
camera mode is not 2 (mode 3, bonnet). The Custom renderer draws them in-car
too, after a z-clear (HH `0x10066d20`) (**asm**). So by default the crew is
seen in the chase views only; the retail capture `apitrace-check` (in-car)
shows no arms (**capture**).

Open: the `0.002` frame-to-time factor in `Unk28` (our source marks it
uncertain); what HH does with the missing `driver.ini`.

**Findings from the Rust port** (characters-2, `rust/rbr-render/src/characters/{crew,incar}.rs`):

- `richard.sgs` has the 13 bones plus `D_mother` (root), `D_steeringwheel` (a second root) with `D_wrist L/R`,
  `D_hand L/R` and `IK Chain01-04` under it, `D_elbow L/R` and `Driver` (**file**). `robert.sgs` has the same tree.
- `richard.sga`'s range is -0.6..3.5 s; `Unk18` passes the scrub time as it is, so frame `f` is `f / 30` s, not
  from the range start (**file** plus the **loader**'s `m_root->Update(t)`).
- Over frames 0-10 the clip's hands turn about `i_steeringwheel`'s `z` by about ±45° for steer ±1 (wheel ±60°), with
  `+steer` a positive turn about that axis (**file**, measured on COROLLA).
- `m_unk628` on COROLLA is (-52.5, -41.6, -68.0) mm: the clip's `D_steeringwheel` lands on the car's wheel (**file**).
- All eight retail cars have both gear nodes, so no retail car uses move 6 (**file**).
- The recordings' control word 5 carries the gear flags (17 in `drive-tanner-corolla`), so the moves play back from
  them (**file**).
- `Misc/LM_Driver.ini` is in `Root/misc.rbz`; its lengths (0.3/0.2/0.3/0.142/0.3 s) differ from `Unk14`'s defaults.


**Findings from the Rust port** (crew-1):

- The FRM1 car block starts at frame byte 52 (20-byte header, 0x20 control); `playback::Sample::shake` reads the five
  signal vectors from `CCar+0x1450+{0x90,0x80,0x70,0xa0,0xb0}` and `Sample::hold` reads `CCar+0x1670` (**file**).
- RSF cars (slot 5, their own model) seat the crew from their own `D_driver_use`/`D_codriver_use` (`--rsf-car 24`,
  `evo_IX.sgc`); the Lua scripts and `richard`/`robert` are the retail ones (**file**).
