# rbr-render

wgpu renderer for Richard Burns Rally stages (`notes/rust-plan.md`, Rendering). Milestone 2: a stage's geometry,
object lanes, sky faces and global clouds in a free-fly viewer, for a chosen weather and wear. Milestone 3: a car,
alone or at a stage start. Reads data only through `rbr-formats`.

## Data

`RBR_DATA` is the extracted retail 1.0 install. `RBR_PATCH` (optional) is the 1.02 patch's
`original/patch-1.02/files/Main_group`. The patch replaces some files (the car shaders in `Fog/`, the executables), so
where the patch has a file it is used, per file, through `rbr_formats::data::Data`. The target is 1.02: with `RBR_PATCH`
set, the car's Fog shaders are the patch's (`notes/formats/shaders.md`, Which car shader files load). The viewer prints
each car's shader files on load.
| `src/coords.rs` | The one place for conventions: left-handed y-up maths, reversed depth (1 near, 0 far: signs no longer fight their backings at range; the stage vertex entries' position is `@invariant`, so the depth prepass's equal test holds on Vulkan too), front faces, D3DCOLOR order, the rotation-only sky view |
| `src/camera.rs` | Free-fly camera, the orbit camera of a car shown alone, and the frustum for mesh culling |
| `src/car/` | Loads a car: its model meshes placed in stage space (`meshes.rs`), the parts drawn (`parts.rs`), face looks (`shading.rs`), textures and files (`files.rs`), the interior's sky share for the cabin light (`cabin.rs`, ours), and the load report (milestone 3) |
| `src/playback/` | Plays a recorded drive on the car: the drive file (`drive.rs`), body and tyre meshes from the recorded poses (`frames.rs`), chase camera (milestone 4) |
| `src/lights.rs` | The car's lights: lamp kinds and switching (brake, reverse, front and tail; live play takes the game's switches and broken lamps and glass from PEV1, playback the brake lamp from the drive's brake `CCar+0x4ec` past 0.1, after the VCU swap, and the game's lamps from the replay's kind-6 events, `particles/sidecar.rs`), the lit and unlit lamp parts, and the flares (additive sprites from `<car>_settings.ini`) |
| `src/live/` | Live mode: runs the stepping process, sends the keyboard's controls, reads its frames back (`read.rs`), paces them to real time (`pace.rs`) |
| `src/views.rs` | The camera views of a drive: cockpit, bonnet, bumper and chase, placed by the car's `Cam_*` sections and model, with the player's nudges, and the cameras the game forces over them (start orbit, helicam) |
| `src/game_camera.rs` | The game's chase camera (mode 1, and the start orbit's mode 10): its settings, lag, height window, distance clamp and collision probes; the bonnet view's ground probe |
| `src/stage_probe.rs` | The stage `.col` as the cameras probe it: the collision system's segment query (`CCollisionSystem::Vf00`) and sphere push (`0x4fe5e0`) |
| `src/hud/` | The drive HUD drawn over the frame with egui: time, speed, gear, rpm, controls, race clock, countdown, splits (`splits.rs`), pace-note symbols (`pace.rs`), lamps and messages (`model.rs`), the retire's screens (`retire.rs`), the pause menu (`pause.rs`) |
| `src/bindings.rs` | Live mode's keys and gamepad inputs (gilrs): the `[controls]` ini section and the controls they make |
| `src/stage/` | Loads `.lbs` meshes and render states (`mesh.rs`), the `.ini` texture names, the filename map and `*_textures.rbz` DDS files (`textures.rs`) |
| `src/renderer/` | wgpu renderer: `mod.rs` (`Renderer`, upload), `pipelines.rs` (pipelines per render state), `textures.rs` (BC texture upload, bind groups per texture binding), `uniforms.rs`, `meshes.rs`, `shadows.rs`, `reflection.rs`, `frame.rs` (the passes of a frame), `offscreen.rs` (readback) |
| `src/atmosphere.rs` | The stage's `Maps/TrackSettings.ini` section: sun, ambient and diffuse colours, car light scales, fog |
| `src/height_fog.rs` | The height fog under `--enhance sky` (ours): the stage's low ground at load, and the mist's density and scale height by the section and the time of day, for `height_fogged` in `sky_lut.wgsl` |
| `src/light_shafts.rs` | Light shafts under `--enhance sky` with `hdr` (ours, `light_shafts.wgsl`): the sun's place in the frame, and the mask, radial blur and apply passes over the HDR scene |
| `src/skylight.rs` | The physically based sky (`--enhance sky`, ours): Hillaire's tables (`skylight.wgsl`) for the stage's sun and air, read by the stage shader (`sky_lut.wgsl`) for the sky, the haze, the sunlight's colour and the sky's light on the car and in the cabin |
| `src/shader.wgsl` | Texture 0 times vertex colour, second texture blended by vertex alpha, mip LOD bias, alpha test; haze, sky gradient, car lighting |
| `src/skidmarks.rs` | Tyre marks as a pure model: emission per wheel from the tyre payloads (PTC1 live, or made from the poses in playback), a ring of 256 marks per wheel, fade, the strips to draw |
| `src/skidmarks_pass.rs` | Draws the mark strips on the ground (and in the car's reflection cube, ours) with `Textures\skidmarks.dds` (eight strips, one per texture index), depth-tested with a bias and no depth write, alpha blended |
| `src/skidmarks.wgsl` | Mark strip shader: UVs inside the texture index's strip, times the mark's colour and alpha |
| `src/particles/` | Particles (particles-1): `PS_*.ini` emitters fed by the tyres, exhaust and rain or snow, simulated per physics step and drawn as sorted billboards; see [Particles](#particles) |
| `src/ghost.rs` | The ghost car: the car's `ghost_*` mesh with the `GhostCar` look (`Cars/GhostCarSettings.ini`), played at the drive's race time; recording a drive's ghost; the best ghost per map (`--best-ghost`, ours: kept in a file, replaced when a finished live drive is faster) and the `.ghc` weather words; RSF's `.gch` ghosts on their RSF car's model, and RSF's layout for kept drives (`--rsf-ghosts`); see `ghost.rs` for faithful vs ours |
| `src/glass/` | In-car glass (particles-2): rain, snow and bugs on the windscreen and the wipers, in the in-car views; see [In-car glass](#in-car-glass) |
| `src/fences/` | Fences (fences-1): the `.fnc` tape and net lines as a cloth (`CFenceHandler`) the car pushes, tears and drops; see [Fences](#fences) |
| `src/dials/` | Working dials (dials-1, ours): an RSF car's cockpit needles, lights and displays drawn from RBRHUD's gauges in the RSF install; see [Working dials](#working-dials) |
| `src/camera_prefs.rs` | Ours: the remembered view and the per-car nudges (`[camera]` in `launcher-settings.ini`, `--cameras`) and the X adjust mode |
| `src/bin/viewer/` | Window, input, `--weather`, `--wear`, `--car`, `--drive`, `--live`, `--view`, `--bindings`, `--screenshot`, `--camera`, `--size`, `--fov`, `--cameras`, `--no-particles`, `--no-sound`, `--volume`, `--no-fences`, `--no-dials`, `--ghost`, `--save-ghost`, `--best-ghost`, `--ghost-weather`, `--rsf-ghosts`, `--seed`, `--look-back`, `--pause-menu` |
| `src/shader_check.rs` | Test: every WGSL module the renderer builds (both depth sample counts, every override combination the pipelines set), validated and written to SPIR-V with naga as wgpu's Vulkan backend does, so a shader that only translates for Metal fails on the Mac |
| `tests/car_data.rs` | `RBR_DATA` (and `RBR_PATCH`): the eight retail cars load with their textures, indices and shader files; XSARA at the track-10_O start |
| `tests/stage_data.rs` | `RBR_DATA`: track-10_O counts, cloud faces per weather, all 86 stages for every weather and wear with every texture present, the launcher's weathers read their own folders, the fallbacks reach only dry maps, levels of detail and mip bias over all 86 stages |
| `tests/interactive_data.rs` | `RBR_DATA`: per-stage `(1,6)` draw records and visibility instances drawn or withheld |
| `tests/stage_probe_data.rs` | `RBR_DATA`: on every retail `.col`, a segment down through sampled upward faces hits them (the probes' leaf boxes and grid) |
| `tests/playback_data.rs` | `RBR_DATA` and `RBR_RECORDINGS`: the Tanner drive's first frame is at the track-54_E start |

## What is drawn

- Stage geometry (`(1,0)` slots 0, 5, 6, 7, 8): vertex colour, texture 0, and texture 1 blended in by the vertex alpha
  (`notes/formats/shaders.md`). Back faces culled, depth on, samplers wrap.
- Levels of detail: each geometry chunk's draw list (render chunk 0x6c) gives its `Lod`. Near chunks (list 1) draw
  while the camera is less than 600 m from the block's box centre, far chunks (list 3) beyond 550 m, and list 2
  always (`notes/formats/lbs.md`, Levels of detail; `stage::LOD_DISTANCE`, the high preset's 600 m, ours: the
  reference installs run low's 200 m).
- Visibility cells (`(2,10)`, `src/visibility.rs`): one box per geometry block, holding the block and governing its
  object lane and the interactive props whose position lies in it across. In the main view a block, its lane's objects
  and those props are drawn only while the cell's box begins within `CELL_REACH`, 315 m along the view: the game's
  camera far plane (300 m times the high renderer's far factor 1.05) in the planes it tests the cells against. A cell
  that crosses it draws whole, as the game's 10 km projection does. The side planes are the view's own (ours, a wider
  image than the game's 4:3). A stage without cells draws as before. The game pops cells in at that distance too.
  The car's reflection cube draws the blocks and lane objects of the cells whose box comes within `CUBE_REACH`, 10 m,
  of its centre, and no interactive props, as the game's cube update does (`HH_H24_VisFill5`, `notes/formats/lbs.md`
  Visibility cells); meshes without a cell (fences, characters, water) keep its 500 m reach (ours). The surroundings
  cube (below, Car reflection) takes everything within the main view's 315 m.
- Mip LOD bias -3 on textures 0 and 1 of near chunks and on the car's diffuse texture, as the capture's samplers
  (`notes/formats/shaders.md`, Mip LOD bias and mip levels).
- Shadow light map on slot 7 and slot 8 chunks with the shadow flag: multiplied into the colour, from the slot's shadow uv
  (TEXCOORD2 for slot 7, TEXCOORD4 for slot 8; `notes/formats/shaders.md`).
- Object lanes (`(1,1)`): alpha test `alpha > 60/255`, double-sided, depth on. The U and V address modes come from the
  kind byte (clamp, mirror or wrap).
- Interactive scenery (`(1,6)`): supported 24-byte `0x15` draw records use their `[TextureInfo]` texture key, alpha
  test `alpha > 60/255`, double-sided rendering, depth test/write and wrap addressing. Each visibility record is drawn
  at its `.lbs` placement, while its visibility cell shows (`CHH_KInteractiveObject::UpdateVisibility`; see Visibility
  cells). The live viewer updates supported props from the
  awake-solid list's handle, position and quaternion; it retains a prop's last pose after sleep. Fixed GENERIC walls
  have no interactive handle and remain at placement. Unsupported class counts are printed at load time and included
  in `LoadReport`. Retail accounting: 76 of 86 stages contain interactive records; all 1,992 draw records and all
  13,703 visibility instances are currently supported, with zero withheld classes.
- Stage sky faces (`(1,3)`): texture 0 and texture 1 with the vertex colour, no culling, depth test, no depth write.
  The tree-line strips' textures have alpha cut-outs (`OpacityMap=true`); the faces' render state is not decoded,
  so the object lanes' alpha test (`alpha > 60/255`, ours) cuts them.
- Global clouds (`Clouds.lbs`): the one face the stage's `TrackSettings.ini` section names (`CloudName`, for example
  `Us_precipitation02_pc` for `US_PRECIPITATION02_PC`; the names match, the game's lookup is not traced). Without a
  section, the faces named for the weather (`US_CLEAR` dry; `US_CLOUDED`, `US_PARTLY_CLOUDED` damp; `US_OVERCAST`,
  `US_PRECIPITATION` wet). Blended, clamped, no depth test or write, camera-anchored. They draw in the sky layer
  before the stage, as in the capture, so the ground covers them.
- Water (`(1,8)`, 90 items in 33 stages): flat RGBA `WATER_STAND_IN` while the retail texture keys are unresolved.
  It is a stand-in, not the game's water texture. Uses the proven state: alpha blend, alpha test `> 0`, back-face
  culling, depth test and depth write.
- Texture coordinates are drawn as stored. No vertex shader in the stage path moves them (`notes/formats/shaders.md`).

## Stage characters (characters-1, `src/characters/`)

Spectators, marshals, media and animals placed by the stage's `.lbs` and driven by its Lua scripts
(`notes/stage-characters.md`). Faithful unless marked ours:

- Placement: `(2,15)` groups `{id, kind, count}`; a kind 1 group loads only the items whose word 1 is the pick. Each
  item's load script is resolved as `Scripts/<rally folder>/<file>`, else `Scripts/<file>` (file name only), the
  folder from the exe's table at 0x7C03DC by map number / 10. Pose: `T(pos) * Rz * Ry * Rx * S(scale)`, `scale` from
  the script (0.001 for people, 0.01 for birds and deer).
- Scripts: a Lua 4.0 subset (`rbr_formats::lua`, every retail script parses except the known-broken ones) and a small
  interpreter (`script.rs`): nil is the only false value, comparisons give 1 or nil. Load scripts call
  `LoadCharacter`, `LoadAnimation`, `AddAnimationToCharacter`; run scripts get `Update(state)` when the clip ends and
  `Trigger(state, 1)` when the car crosses the Driveline trigger.
- Triggers: the Driveline set's `arrays[2]` `{trigger data, distance, lead}`, object id from `arrays[0]`. Fire once when
  `distance - lead * v < x`, `x` the car's driveline value (`CCar+0x17c`), `v` its speed along the line. 14 triggers in
  `track-54_*` name no object; they never fire (the game's `Fire` finds no descriptor either). Then the plain keys, `arrays[1]` `{trigger data, distance, flags}`
  (`Load3b0`, 0x4783b0): passed forward by `x` they trigger their object unless flag bit 0 is set (backward, the
  reverse). Every retail bird strike has only a plain key, as have the finish walkers (`load_end_race01a/b`, keys
  about 40 m past the finish line).
- Named triggers (setpieces): the `.dls` table 9 `{object, name}` (`rbr_formats::markers::named_triggers`);
  `Characters::fire_named` is `FireByIdx` (0x47a7d0). The HUD's countdown fires `start_animation2` once below 6 s
  (`M573d90`; none after a jump start), which starts the countdown marshal (`load_start_race01c`):
  `Characters::countdown`, per frame in `follow`, from live play's HUD2, else (ours) the drive's race time from 7 s.
  `start_animation1` (the time card writer) and `end_animationN` are fired only by the replay's timeline
  (`CAnimatedReplayManager`, 0x5fe890), so the writer holds its first clip.
- Animation: clips sample `<clip>::<node>::PRS` tracks by element-wise lerp, ends held (PRSSampled::Evaluate; the
  `_43=_42` in our C++ is a reconstruction bug). Nodes without a track keep identity. Clips do not loop: `Update` runs
  when the time left reaches 0.
- Skinning: 4 weights and 4 bone offsets per vertex (`sgs` stream); bone matrix = placement * node world * inverse
  bind. The inverse bind convention is inferred and checked visually only.
  A vertex whose valid bones carry no weight (retail: four per crew model, slot 0 with no bone and weight 0) follows
  its mesh's node, placement * node world, as an unskinned vertex would (ours: the upload would sum it to the origin).
- `LuaRandom` and the group picks are the game's generators (`characters/random.rs`: the exe's LCG
  `E01_RandomRange` with the kind-2 groups' no-repeat sets, and msvcr71's `rand()` for `PickRand`), seeded by
  `--seed` (the game's race start seeds both with `time(NULL)`; ours: 0 by default). Ours: the stream position (the
  game's other `rand()` calls between the seed and these draws), the shading (`ka 1, kd 0.8`, alpha test `> 60/255` on opacity-mapped faces),
  CPU skinning each frame.
- Visibility (`characters/cells.rs`): a character takes the first `.lbs` `(2,10)` cell whose `x`/`z` box holds its
  spawn point, and is shown, and its scripts run, while that box meets the camera's six clip planes (`VisSelect`,
  `E02AnimUpdate`). Ours: the planes are last frame's (`Characters::apply`), the far plane ours.
- Test: `tests/characters_data.rs` drives the tanner recording on `track-54_E`; the deer and a media person trigger
  and switch clip before the car passes; the countdown marshal starts at race time 1 s and the finish walkers past the
  finish line; the RSF samples 101, 138 and 140 start their countdown marshal.

## Scripted events (scripted-1, `src/characters/scripted.rs`)

The car-flip spectators, the rescue helicopter and the bird strike (`notes/scripted-events.md`), on the same actors as
the stage characters. Events for sound and particles come out of `Characters::take_events` (the viewer prints them
and passes them with `rescue_position` to the sound, `sound::events`: the helicopter's loop, the bird hit and the
rally incident's speech stop; the example `scripted_events` writes them as CSV for `rbr-sound --events`).

- Flip spectators: the exe's five `load_car_flip<n>.lua` (rally folder first, none on the maps in the exe's list at
  0x72ace0). The recordings carry no signal, so the `CSigOutsideAssistance` emit is read from the frame's car block:
  `Assist8DF0` arms the countdown +0x15e4 with 0.5 s; the code is 1 with the flip flag +0x9f0, else 2 (override
  +0x1670 positive), else 3. The HUD timer is `rbr_gameplay::race_clock` (start at 1.0 s, end at 4.5 s) on the drive's
  time. The start puts spectators 0-2 (code 1) or 2-4 (codes 2, 3; code 3 turned by pi) at the car's driveline point,
  turned to the driveline's heading (`acos(-x)`, negated for positive `y`), clip 0 from its start; the end hides them.
- Rescue helicopter: `Characters::call_rescue` (the viewer: key K, or `--rescue-at <step>`), standing in for the game
  flow's retire codes 1 and 2. `Activate`'s side (+1 before 200 m, -1 within 200 m of the end, else `rand() > 0x3ffe`),
  landing at the car's driveline distance with a random heading, clips fly-in, takeoff, idle. 24 s later
  `RetireBystand`. Its model is rigid: static meshes follow their node (ours). Its dust (`Burst` 0x49bf90): the rally's
  emitter (`ar_smoke_b`, `br2_fakesmoke_a`, `au_fakesmoke_a`, `ho_DAMP_fakesmoke`, `us1_fakesmoke_a`) with its life
  cut to 0.2 and its intermediate colour's alpha (class `+0x6c`) set by rally (0.7, 0.4, 0.25, 0.075, 0.2, 0.2), 60 particles a second under 30 m over the ground, from the ground under the body at 15 m/s outward and
  0.5 up (`Characters::take_smoke` to `Particles::rescue_smoke`; ours: the ground is the driveline's height). Mode-8
  camera: on the driveline 30 m
  short of the landing point (away from the side it comes from), 1.8 m up, looking at `UpdateHeli`'s point (100 m
  along and 50 m up, falling to 10 m over the landing point in 21 s); the viewer uses it while the helicopter runs.
- Car-flip camera (mode 13, `FUN_00487980`, the camera's handler of the flip start): the look point 1.5 m along the
  driveline from the car's point, the eye at the code's offset in the flat driveline frame (code 1 `(-6.63, 2.668,
  0.967)`, 2 `(3.709, 3.709, 1.041)`, 3 `(-3.919, -4.49, 0.86)`, across, along, up), then 1 m up; 75 degrees; C is
  blocked; not while the helicopter runs; back to the player's view at the flip end. Ours: no scenery probes pull the
  eye in.
- Bird strike: `run_birdstrike.lua` loads (1 in 200 by `LuaRandom`; `--birdstrike` forces it) and runs with
  `PlaceCharacterAtCar` and `Birdstrike` as host calls. `g_e02167ea40` is the 2D effect system's `+0x190` (the system
  is the static at 0x167e8b0): the windscreen point in the world, a random point of the windscreen chart (`RandPair`
  0x5f71e0) through the car's matrix each frame. The bird is moved there and keeps its own turn; `Birdstrike` is the
  event at that point, and a new point is drawn. 1.02 throws no feathers and draws nothing on the glass for it (the
  signal's only listener is `CCollisionSnd`). Ours: the chart is the glass's (`glass::Windscreen`), read as fractions.
- Ours: the ground probes are the driveline's points, `CDriveLine::Sample` by distance interpolates the records, the
  `BirdstrikeCheck` game-flow gate is not modelled.
- Tests: `scripted.rs` (emit detection, the HUD timer's frames, groups and heading, helicopter side, flight, camera and
  24 s), `tests/scripted_data.rs` (the flip events and the helicopter on `drive-rcv-help-corolla-34`, the six bird
  strikes of `drive-v6-gravel-206-64`), `rbr-formats` `stage_plain_triggers_reach_the_bird_strikes`.

## In-car crew (characters-2, `src/characters/{crew,incar}.rs`)

The driver and co-driver in the car with `--drive` and `--live` (`notes/stage-characters.md`, section 7). `crew.rs`
is the pure model (unit-tested), `incar.rs` loads and poses them. Faithful unless marked ours:

- Load: `scripts/richard.lua` and `scripts/robert.lua` through `app:LoadHumanCharacter`, one clip each. Settings from
  `Misc/LM_Driver.ini` in `Root/misc.rbz`.
- Seat: the car `.sgc`'s `D_driver_use` / `D_codriver_use`, the car's axes, scale 0.001. The driver is moved by the
  car's `i_steeringwheel` less the model's `D_steeringwheel - D_mother`, so his hands meet the car's wheel.
- Steering: the driver clip is scrubbed to `Normal * (steer + 1) / 30` s (frames 0-10); `i_steeringwheel` turns by
  `steer * 60°` about its node's `z`, the way the clip's hands turn. Playback finds the wheel mesh by its centre
  (within 12 cm of the node; ours).
- Gear and handbrake: the 20-entry move ring of `OnCarControl`, fed by the frame's control word (word 5 bits 0/1 a
  gear change, word 2 the handbrake). Each move runs its `Animation Lengths` time; hand and elbow follow the Hermite
  keys at the car's `D_hand R_gear`, `_gear p`, `_handbrake`, `_handbrake p` with the ini tangents; the hand rotation
  blends between the clip's pose frames (gear 20/25, handbrake 35/40). The tanner recording has 17 gear flags and no
  handbrake. `--live` frames carry the same word, so the crew follows the keys there too (HUD2's gear is not needed).
- Arms: two chains (upper arm, lower arm, hand) solved to the hand targets every frame (`CLM_IKChain::Solve`: the
  elbow in the plane of the current elbow; out of reach the arm stays). The frames are rebuilt as `Unk22380` does:
  upper and lower arm with `x` along the bone, `B_hand rotation` (the forearm twist) with the lower arm's `x` at the
  elbow; hand and fingers untouched. Co-driver: `robert.sga` looping every 14.133333 s.
- Shake (`Unk23000`): the driver's five `CSigIncarShake` vectors from the frame's car block (`Sample::shake`, the
  physics' own) tilt the `x` axis of `B_spine01`, `B_spine02`, `B_head` and both upper arms by the ±0.2-clamped
  `(x, z, y)`; then the hands go back to where they were. The co-driver's signal is never sent, so he does not lean.
- Input gate: no new hand moves while the start hold (`CCar+0x1670`, `Sample::hold`) is above 0.
- Drawn in the chase views only (`CarView::shows_crew`): the game's High renderer skips the crew in-car and in the
  bonnet view.
- Ours: the replaced playing entry turns its progress round (`1 - p`); elbow keys are the clip's elbow at the pose
  frame; fingers during a move use `frame / 30`. Lighting as the stage characters.
- Tests: `crew.rs` (steering mapping, Hermite, ring, IK frames, lean), `tests/crew_data.rs` (all eight cars seat the crew
  and have one wheel rim; the tanner gear changes go to the stick and back).

## Renderer choices beyond the original (render-m12, render-m13)

These change how the data looks, not what it means; each is ours, not the game's.

- 4x multisampling (`renderer::SAMPLE_COUNT`), resolved into the target. Opaque alpha-tested draws (object lanes:
  foliage, fences, spectators) use alpha to coverage: the alpha test still drops the same texels, and the alpha of the
  ones it keeps becomes a coverage ramp about a pixel wide at the cut-out edge.
- Atmosphere (`atmosphere.rs`): one `Maps/TrackSettings.ini` section per stage and weather. The game builds the
  section name from undecoded weather state, so the viewer picks it: the sections of the stage's track number and
  letter (`54E_` for `track-54_E`, `54O_` for `track-54_O`), the first holding the weather's preset (dry
  `norain_lightcloud`, `nosnow_lightcloud`, `crisp_clear`; damp `lightrain`, `lightsnow`, `lightfog`,
  `crisp_partcloud`; wet `heavyrain`, `heavysnow`, `hazy_partcloud`, `hazy_clear`), else the first. The viewer prints
  the section. Defaults (no section, the car alone) are the common `54E_crisp_clear` values with a fixed sun.
- Haze: linear fog by the distance to the eye, `FogStart` to `FogEnd` toward the fog colour, on world-anchored draws;
  the stage sky faces (`(1,3)`, the superbowl backdrop) use `SuperbowlFogStart` and `SuperbowlFogEnd`. With `UseFog =
  false` the game hazes through its scattering shaders (not reproduced); the renderer then still applies the section's
  fog values, capped at 60 % (`CLEAR_HAZE`), so far hills fade but keep their shape. Under `UseFog` the game's
  superbowl range covers the backdrop almost entirely (in `41O_M_heavyrain_heavycloud`, -6880 to 2200 m: 85 % at
  800 m), which drew it as a flat pale slab. Ours: the backdrop's foot is all fog, as the fogged stage in front of it,
  and from 300 m above the eye (`MIST_HEIGHT`) the fog covers at most 80 % (`SUPERBOWL_HAZE`), so the mountains rise
  out of the haze with their shape and some texture. `Superbowl_Scale` and `Cloud_Scale` are not used: the game's
  `LoadSkySettings` copies the `CloudName` string over them.
- Sky (render-m13, `sky` in `shader.wgsl`): drawn first, in place of the undrawn `SkyDome`. Without `UseFog` the hue
  runs from a pale horizon (the fog colour halfway to white) to the blue zenith with the elevation, and the brightness
  follows the Preetham, Shirley and Smits (1999) daylight distribution for the sun at `SunDir` (turbidity 2.5, relative
  to the zenith, capped at 3): brighter toward the sun and the horizon, darkest about 90 degrees from the sun. Near a low
  sun the light warms toward the sunlight's colour after the air (Rayleigh depths over the Kasten and Young air mass).
  The sun is a white-hot disc about a degree across (twice the real size, so it reads on screen) with a tight glow.
  Under `UseFog` the sky is the fog colour, a little brighter overhead and toward the sun. The game's `SkyDome` would
  compute it from the section's scattering values (`Rayleigh_Multiplier`, `Mie_Multiplier`, `Greenstein_Value`,
  `Sun_Intensity`), whose shader inputs are unproven, so they are not used.
- Clouds (render-m13): the cloud domes (about 70 m across, from 3 m at the rim to 33 m up, camera-anchored) fade out
  between about 2 and 12 degrees of elevation and take on the sky's colour toward the horizon, so they have no
  visible rim. Their game transform is still undecoded. Their texture times vertex colour is doubled (`CLOUD_LIGHT`,
  ours): the vertex colours peak at 128 and the game's extinction and in-scatter terms that scale and add to them are
  not reproduced, so undoubled the clouds drew as dark smoke. Under `UseFog` they fade into the sky by
  `1 - SkyboxSaturation`, as the game fogs them (`CloudsSingleTexture.vso` `oFog = c49.w`, which
  `CHH_KClouds::Render` loads with `SkyboxSaturation`; mostly 0 to 0.16 in the rain and snow sections). That leaves
  an almost plain sky, so ours caps it at 70 % (`CLOUD_HAZE`): a grey overcast with soft structure.
- Moving clouds (visuals-4, ours, `--enhance clouds`, the launcher's "Moving clouds"; `clouds.wgsl`,
  `notes/visual-enhancements.md`): in place of the cloud domes, a procedural layer drifting with the wind, as cloudy as
  the section's cloud word, lit by the sun and the time of day, high cirrus in clear skies, and the clouds' shadows on
  the stage and the car under clear sections.
- Physically based sky (visuals-7, ours, `--enhance sky`, the launcher's sky option; `skylight.rs`, `skylight.wgsl`,
  `sky_lut.wgsl`, `notes/visual-enhancements.md`): by day under clear sections, the sky, the haze between the eye and
  the stage and the sunlight's colour come from Hillaire's sky model for the stage's sun (its true elevation under a
  clock, so the twilight follows the set sun) and a ground haze as dense as the section's fog; by the cells' reach the
  stage is hazed as the backdrop's foot is. The car's ambient takes the sky's irradiance by direction, and the cabin's
  sky share too; reflections before the car's cube is drawn take a blurred sky cube. Overcast (`UseFog`) sections keep
  the fog's sky; their irradiance follows it. Night keeps the night sky. Since visuals-9 also a height fog (mist over
  the low ground at dawn and dusk and in the fog sections, `height_fog.rs`) and, with `hdr`, light shafts from a sun in
  or near the view (`light_shafts.rs`). The game has neither: the mist's density and height are our constants per
  weather word, scaled by the section's `FogEnd` under fog.
- Cabin light (visuals-interior, ours, `--enhance cabin`, the launcher's "Cabin light"; `cabin.wgsl`, `car/cabin.rs`,
  `notes/visual-enhancements.md`): in the cockpit the interior set's ambient follows the sky each vertex sees past the
  car's opaque faces (measured at load) and the light outside the way it sees out (the window it looks through), the
  sun reaches it only through the glass (the car's shadow map, the sun's cascades, the clouds) and bounces back off the
  sunlit faces, the plastics take a faint sheen of what they reflect out of the windows, the mirrors mirror the
  surroundings, and at night the headlit road glows back in. Off, the interior is lit as the game lights it.
- Car lighting: see Car below.
- Tone curve (render-m13, `graded` in `shader.wgsl`): every drawn colour goes through `(1 - exp(-1.5 c)) / (1 - exp(-1.5))`
  per channel, then a power of 1.1 for the dark end, then 1.08 saturation. Kept mild for a natural look: it lifts the
  dim baked stage light (the forest of track-54_E from a mean luminance of 28 to 41 of 255, track-10_O from 109 to 141)
  and keeps white white. The game writes its shader output as it is; its in-scatter, which adds light to the stage, is not reproduced.
- Car sun shadow (render-m13, `renderer::SHADOW_SIZE`): a 2048-texel depth map of the car's opaque lit draws, seen
  along `SunDir` over the car's bounds, filtered 4x4 for an edge about 15 cm wide. The stage in it darkens by
  `Car_Shadow_Alpha` (TrackSettings, read by `LoadSkySettings`) times 1.75, at most 0.45; the overcast sections have 0,
  so no sun shadow. It fades out where the baked vertex light is already dark (luminance 0.5 down to 0.2: ground in
  the shade of trees or banks), so the car casts no second shadow there. The car's own faces in it lose their sun term. The game's car shadow uses the `Plane_body` and
  `Plane_deepshadow` meshes, whose drawing is not traced.
- Car shadow decal (faithful, `car_shadow.rs`, visuals-3): the game's `RenderShadows` (0x605670): `car_dpshad2.tga`
  laid on the `.col` triangles under a 2.0 x 4.2 m rectangle of the car's frame, alpha `Car_Deep_Shadow_Alpha` times
  the tilt and the height fades, drawn by the skid marks' pass (`SkidPass::decal`). On by default; the contact shadow
  below replaces it under `--enhance contact`. Ours: the car's frame is the recorded pose (the game's car-body offset is
  not traced); stages whose `.col` does not read (RSF-encrypted) have no decal.
- Car contact shadow (render-m13, ours, `--enhance contact`, the launcher's "Soft car shadow", `renderer::GROUND_REACH`):
  a second layer of the same map, the car seen from straight above. Ground with the car less than 0.6 m above it darkens by `Car_Deep_Shadow_Alpha` (1, the game's
  default, where the section has none) times 0.4, half that from 0.6 to 1.4 m, filtered 6x6 at 8 cm steps into a soft
  blob about the car's footprint. It replaces the game's decal (above) and grounds the car in overcast sections. The car model's
  `Plane_deepshadow` quad (1.8 by 3.9 m, its texture not named in the car files) is still skipped, its drawing untraced.
- Ground detail (render-m13, `ground_detail` in `shader.wgsl`): the near ground (the unlit draws with the mip bias)
  is brightened or darkened by up to 8 % by two octaves of 3D value noise in stage space (9 and 33 cm), fading out from
  4 to 30 m, so the low-resolution ground textures do not look smeared close to the camera. Two more octaves (8 and
  30 m, stretched to their full range) brighten or darken it by up to 22 % at every distance, so the repeats of a
  ground texture break into patches. The noise lattice repeats every 289 points (no seam in stage space).
- Car reflection (render-m13, `Reflection` in `renderer/reflection.rs`): the lit draws reflect the stage, drawn without the car
  into a 128-texel cube map at the car's centre (all six faces on the first frame, then one face per frame, so a face is
  at most five frames old), instead of the sky alone. Like the game's, the cube holds the sky and only the stage cells
  within 10 m of the car (above, Visibility cells), so the car mostly reflects the sky and the road it is on. A
  second cube of the same size (ours, `View::Surroundings`) holds the stage to the main view's 315 m, props included,
  but without meshes under 2 texels across and with the far ground of every block but the one under the car. It draws
  one face every third frame (all six on the first frame and after the car jumps over 50 m: a seek, a teleport or a
  cut to another car); the car's light from its surroundings (below), the wet roads' reflection and the water's
  missed rays read it, so those keep the look they were made with. The faces are drawn graded; the shader takes them back through
  the inverse tone curve before the Fresnel weighting.
- Car light from its surroundings (render-m13, `surroundings` in `shader.wgsl`): the reflection cube gets mip levels
  (`blit.wgsl`). The cube toward the sun (16 texels a face) against the sky there says whether trees or hills block
  the sun (an open sky measures about 0.4 through the blur, a forest about 0.1); blocked, the car keeps 35 % of the
  sun. The cube around each normal (4 texels a face) against the open sky scales the ambient, half by its colour, so a
  car in a forest is as shaded as the road beside it and takes the ground's tint. Unchanged in the open.
- Water (render-m13, `water_surface` in `shader.wgsl`): the water items are drawn as a surface instead of the flat
  stand-in fill: a dark body that shows the bed when looked at from above, two octaves of ripples (fading with
  distance), water's Fresnel (F0 0.02) over the sky, a dull bank colour reflected near the horizon (banks and trees are
  not drawn into reflections) and a sun glint in clear weather. The ripples do not move (no frame time).
- Road shine (render-m13, `road_shine` in `shader.wgsl`): the chunks with a specular flag in slots 6 and 8 (11,643
  over the retail stages; their maps cover the road surfaces) add `4 * sat(t2.rgb * S * strength)` before the shadow
  light map, as the game's pixel shaders do (`notes/formats/shaders.md`, Stage slots 6-8). The map (`SpecularTexture<N>`,
  wear folder then weather folder), the per-vertex strength (TEXCOORD3: 1.0 or 0.3 per stage) and the stored normal
  are the data's; `S = scale * 2^(sharpness * (1 - 1/(n.h)^2)) / (n.h)^4` is the game's form with our sharpness 8 and
  scale 0.5 (`c90` untraced). Ours too: the highlight is scaled by 1.125 dry and 1.5 damp and wet (`WEATHER_SHINE`);
  the game's specular maps are the same in every weather. It shows toward the sun only, so chase views facing away
  barely change.
- Wet roads (render-m13, `wet_film` in `shader.wgsl`), stylised after photos of rain rather than physical water: in
  wet weather the specular draws are darkened (x0.7) and saturated (x1.2), and get a film of water reflecting the stage
  around the car (its reflection cube) and a small sun glint, 6 to 35 % at a steep view rising to 75 % at a grazing one
  (`WET_REFLECT`, `WET_MOST`). Where water stands is set by stage position (a thin film everywhere and smooth patches
  0.7 to 3 m across), scaled by the game's specular map, so the film fades out where the map does toward the road's
  borders. The ground around is soaked at 60 % of the road (`GROUND_SOAK`) so the road blends into it. The dry-weather
  road glitter is damped to 0.3 when wet.
- Screen-space reflections (render-m13, `ScreenReflection` in `renderer/reflection.rs`, `fs_reflect` in `shader.wgsl`): when a
  stage has water or is wet, the frame is resolved first and the water and wet-road draws are drawn again, stepping
  each reflected ray through the frame's depth (64 steps out to 120 m) and blending the frame's colour where it hits,
  by the surface's reflectance. Rays that leave the screen or hit nothing keep what the surface was drawn with (on
  water, the car's reflection cube within 20 to 60 m of the eye). Glossy, not a mirror (no frame-to-frame smoothing):
  each pixel averages 4 rays spread over a narrow cone, stretched 4x toward and away from the eye on roads (wet roads
  smear reflections down the screen), reads the frame from a blurrier mip level the further the ray travels on screen,
  trusts a hit less the further behind the frame's surface it ends, and keeps stepping past things it only passed
  behind.
- Flowing water and puddles (visuals-5, ours, `--enhance water` / `wet`, the launcher's "Flowing water" and "Puddles
  on wet roads"; `water.wgsl`, `puddles.rs`, `notes/visual-enhancements.md`): the water items fade into their banks by
  their vertex alpha, take their vertex shade, show the bed through the shallows and ripple along their texture
  scroll; on the roads with the wet film, water stands in the ruts and dips (each road vertex's height against the road
  around it), and damp weather gets a thinner film with no screen-space reflections.
- Tried and dropped: 16x anisotropic filtering. With the -3 mip bias it adds little sharpness, and its wider footprint
  reads the neighbouring tiles of the plate atlases, which drew dark dashes along the road (render-m12 report).

Not drawn: the global sky dome (`skyboxes.lbs`; its colour comes from the `SkyDome` scattering shader, not
reproduced), the particles the tyres do not throw (see [Particles](#particles)), and
these `.lbs` chunks (counts over the 86 retail stages): reflection `(1,7)`, 51 items in 10 stages, is parsed and
counted but withheld because its runtime texture lookup and render-state values remain unresolved. Interactive `(1,6)`
in 76 stages is parsed, with draw records withheld until runtime visibility is known. Animation objects `(2,14)` and
container objects `(2,15)` have names only; their models are not traced. Water and reflection account for 141 items
together. The loader report lists parsed, drawn and withheld counts per stage.

## Conventions

Positions are the stage file's values, left-handed with y up (`notes/formats/shaders.md`). The view and projection
are left-handed (`coords::view_matrix`, `coords::projection`), which matches wgpu's clip space, so nothing flips an
axis. Front faces are clockwise on screen (`FRONT_FACE = Cw`, back faces culled), the D3D `D3DCULL_CCW` default.
Vertex colour is stored as D3DCOLOR (B, G, R, A in memory). Textures are not converted to sRGB, and the surface is
a non-sRGB 8-bit format (`present::surface_format`), as the D3D9 output was. Camera-anchored meshes (the clouds) use the view's rotation only, so they
stay at infinity.

## Stage data (retail install, `tests/stage_data.rs`)

- Every stage (86) loads for every weather and wear with no bad triangles, skipped draws or unnamed textures.
- `dry` textures are complete for every stage and wear.
- 51 stages have every `damp` and `wet` texture. 35 are missing 15 to 37 `damp` and `wet` textures each, because
  the install has no such files for them and the filename map has no entry (`notes/formats/lbs.md`). The wear folder
  never changes the count.
- Wear is resolved by the filename map: `normal` and `worn` ground requests fall back to `new` when the wear folder
  lacks the file.
- Weather falls back to a drier folder (ours, `Weather::fallbacks`): a `wet` texture the install lacks is read from
  `damp`, then `dry`; a `damp` one from `dry`. Every stage then loads for every weather and wear with no missing texture.
  The 35 stages above fall back, all on maps the season table keeps dry in every row (4, 10-15, 31-36, 71-89;
  `fallbacks_reach_only_maps_the_season_table_keeps_dry`), so the launcher's weathers never use a fallback
  (`launcher_weathers_read_their_own_texture_folders`). A snow stage, with only its `dry` set, uses that set in every
  weather. `LoadReport::fallback_textures` lists the requests read from a drier folder.

## Playback (milestone 4)

`viewer track-54_E --car COROLLA --drive <recording> [--at <step>] [--screenshot <png>]` plays a drive from
`rbr-recording` back on the car. The body and the four tarmac tyres follow the recorded poses, and the camera is a
chase view 6 m behind and 2.5 m above the body (ours, not the game's). Keys: P pauses, Left and Right step a frame,
Up and Down halve and double the speed, R restarts, K calls the rescue helicopter (ours). `--at` starts at the first
frame whose step counter reaches it; `--rescue-at` calls the helicopter at a step and `--birdstrike` loads every bird
strike (scripted events).

- Frame: the physics is z-up, the stage y-up, `stage = (x, z, y)` of the physics. The car model's front is `-z`, and
  `model_to_body` maps it to the physics front `-Y` and keeps `+X`: a mirror, so that model to stage is a proper
  rotation like the static placement. A proper `model_to_body` turned the car inside out (render-m10): the outer faces
  were culled and the black interior showed. The model's left (`+X`, the `l` tyres) is body `+X`, where wheels 0 and 2
  are mounted. The body and tyre checks are in `rbr-recording/README.md`.
- Tyres: found from the `e_wheel_tarmac_*` nodes of the `.sgc` and matched to their drawn meshes by position. Each
  tyre's hub is placed at its recorded anchor, and it rolls about the lateral axis by the recorded roll. The body's
  offset to the model is the mean over the tyres of `anchor - model(hub)`; the residual is under 0.03 m per tyre on
  the Tanner drive.
- Not shown: the recorded roll grows about 2.2 rad per metre of forward travel, so the tyre radius is not what the
  roll implies (0.45 m); the roll sign is set by that data, not by a drawing. Steering is not in the recording, so the
  front tyres do not turn. The recorded `WheelPose::orientation` (the right wheels about 180° about the body's up) is
  not used; it may carry steering (not checked).
- Ride height (render-m11): the hub, the recorded anchor, sits 0.301 to 0.313 m above the drawn road under it
  (COROLLA on track-54_E and LANCER_EVO_VII on track-41_M, at rest and at 14 to 25 m/s). The physics wheel `Radius` is
  0.315, so that is a few mm of tyre deflection, and no offset is applied. The drawn tyre mesh is a little larger
  (COROLLA half height 0.322), so its bottom dips about 2 cm into the road. Where the game draws the wheel mesh is not
  traced.
- The per-frame update rewrites each car mesh's vertices (`Renderer::set_vertices`); the stage's own meshes are not
  touched.

## Particles

`src/particles/` throws the game's particles from the stage's `PS_*.ini` (`Maps/Tracks.ini`'s `Particles`, the
`_LOW` file for the maps the game loads it for), parsed by `rbr_formats::pss`. `--no-particles` turns them off.

| file | role | faithful | ours |
| --- | --- | --- | --- |
| `track.rs` | Track emitters (`CTrackEmittersManager`, `.dls` table 4) | the rows (name, dummy, squared distance), the 0x40 limit, the rule (`M5e9490` 0x5e9490: emit at the file rate while switched on and the camera is nearer), the switch never set in 1.02 (only `CSigAnimationDummyChange`, 0x5e9550, sets it and no animation channel names the dummies), so the retail chimneys never emit | chimney smoke under `--enhance particles` (`chimney_smoke`, a clone of `g_firesmoke_emitter`); a 9 s warm-up after a seek |
| `sim.rs` | Pools per emitter, emission, motion, billboards | emitter classes (box, cone, point) and their launch, particle class colours and sizes over life, accel, wind and friction forces, air friction | the step (one per physics step, Euler), `POOL_SCALE` 4 times the data's maximums (the stones and the water's splash keep the file's), `MIN_SIZE` 7 cm, smoke denser by 2 (not the water's), stones (the pebble classes: heavy, under 5 cm, no trail) at least 3.5 cm, shaded to 0.4 of the file's colour and keeping the file's pool, the floor kill, sorting back to front |
| `tyres.rs` | Signal handlers per wheel, the water splash | `E82F_BlkD0::Emit`: rate from mass, smoke and car speed, perturbations, the limit, equalization smoothing, launch from the wheel velocity and car speed, `rate * frame time` particles a payload (`[E83System+0x94]`, a game frame); the surface's handlers then its alias's; `E82F_Emit::Run`'s splash: the water level following the payload by 0.3, the splash and splash-smoke counts and side and up speeds from the level tables, out of the car's side from the wheel's outer edge | the game frame as 1/60 s (`GAME_FRAME`), stones at one particle per unit of rate, the fake-smoke rate |
| `wave.rs` | The water wave sheets (`E84Wave`) | the level tables (speeds, alpha), the outer and inner sheets, the 64-slot ring moving on every 0.05 s, 0.6 s life, the six-point `Splash` profile with gravity 20.81 and at most 0.1 m rise, the strip of triangles between slots and its vertex alpha (`CPC_Wave::Vf04`), `splashcolor.dds` with `splashalpha.tga` at the strip's texture coordinates (the colour along the profile, the alpha flipped, both along the sheet's length) | the vertex alpha times the random share as the colour's alpha, the texture's scroll left out (`CPC_Wave` adds two offsets we have not traced), the colour and the alpha in one layer wrapped with `fract`, drawn flat under `--enhance particles` |
| `car.rs` | Damage debris and sparks, engine fire, steam and pops | `EmitDebris`: the force keys and count tables for debris A, B, C and smoke, the tilt boost past 45 degrees, 0.025 of the car's velocity, debris landing and resting; `CSigEngineFire`: rate times the level, held; steam: the level from the splash (0.1 a payload) and the engine noise word 5 (times 20), one particle a step, falling 2.5 a second, 0.5 of the car's velocity; `EmitExhaust`: the flow (live `state+0x200`, smoothed 0.7 new) times 2 for the exhaust and the broken word times 1 for the broken exhaust, 0.95 of the car's velocity; pops past 0.8 (up to two) | the sparks (1.02 throws none: `g_spark_emitter` is in no file and `g_car_sparks` is empty), the pops fed from the engine noise misfire word, the exhaust's spin (omega x r) left out, the sums over a 60 fps frame, the engine strip (`Point_engine` plus or minus 0.3 m sideways), playback impacts from the poses (a speed drop over 1 m/s, 1400 per m/s, 2 m ahead) |
| `damping.rs` | Bushes (`E82E_Sys::ScanEmit`, `E82D_Fed::Emit`) | material below 255 through the aliases, `(penetration + add) * mod` particles truncated, at the contact point along the push vector times the velocity modifier | the particles die 1 m below the contact |
| `rain.rs` | Rain and snow (`CEmitterRain`, `Init` 0x5daa90, `Step` 0x5da450) | the instances `LoadFile` makes (rain 1000 and 20, snow 200, 100 and 10 above amount 75), each particle's velocity (wind times the wind factor, jitter, fall), the 16-bit positions wrapping in a box half a box ahead of the camera, raised 0.15 and 0.8 tall, the age from the height, the trail velocity, rain showing `amount * n / 100`, snow all | the trail's camera motion over a 60 fps frame, the random draws |
| `weather.rs` | `Misc/track_w.ini` | `[TrackNN]` wind and its perturbation every perturbation time (the fall fields are parsed, unused by the rain) | the random draws |
| `ground.rs` | Surface under a wheel in playback | `.col` cell triangles and the `.mat` runs, barycentric material pick and blend weight | the 4 m grid, the highest face within 0.5 m |
| `mod.rs` | Following the drive, exhaust, precipitation | the exhaust emitter at the car's `[Point_exhaust1]`; the weather's amount (`CWeatherDaemon`): the preset's name picks light or heavy rain or snow at amount 50 or 100 | playback's tyre values from poses (contact from ride height, mass from slip through a table calibrated on the live payloads of the Tanner damage drive, velocity from the anchors), exhaust rate by throttle when no exhaust event comes, `--enhance particles`' wetness rule (the user's), a 400-step warm-up after a seek |
| `gpu.rs`, `particles.wgsl` | Billboard and wave pass | sprites from `Misc/particletextures.ini` in `Textures/particles.dat`, blend and additive classes | the water waves as one triangle list drawn first, then one sorted instanced draw of the billboards, both with premultiplied blending, 64x64 sprite array, ambient plus a fifth of the diffuse light, the stage haze and tone curve; soft (depth-faded) edges only under `--enhance particles` (`fs_soft`; the game has none) |

In live mode the viewer feeds the game's own tyre values: `rbr-standalone --live` reads each `CSigTireParticles`
payload (0xB0 bytes) the step emits and writes it in a `PTC1` record (`rbr_recording::particles`). In playback the
values come from the poses and the ground (`ground.rs`), so their scale is ours. The damage, damping, engine fire and
engine noise signals come in a `PEV1` record after `PTC1` (the payloads `Pure` copies at 0x4ef1d0, 0x52be70 and
0x4ef290, plus the engine noise words). Playback of a recording takes such records from its replay
(`particles/sidecar.rs`): `rbr-standalone --boot --recording X --events-out FILE` runs the recording's controls
through the live boot (bit-exact; it reports the first step whose car pose differs from the recording's) and writes
per step a u32 step, a u32 length, then the `PTC1` and `PEV1` records. The viewer runs it once per recording and
stage and keeps the file in `RBR_EVENTS_CACHE` (default `drive-events/` next to the viewer), never next to the
recording; it needs the recording's `.auto` and `rbr-standalone` next to the viewer (or `RBR_STANDALONE`). Tanner
takes about 5 s. `RBR_PARTICLE_EVENTS=<file>` gives such a file instead, and an empty value turns them off (the
poses' payloads, no recorded lamps). `RBR_PARTICLES_TIMING=1` prints the
alive count and the time spent per draw: 860 us for a 400-step warm-up of the Tanner spray (464 alive), 4.8 ms for
arctic snow (6425 alive); a single frame's step is about 12 us, sorting and building 6400 billboards 0.36 ms.

## In-car glass

`src/glass/` replaces 1.02's screen-fixed `NParticleSystem2D` sprites with water on the car's windscreen
(`notes/particles.md`, "In-car glass effects"). It is drawn on the interior windscreen in the cockpit view and, ours
(visuals-3), on the exterior one (`car.exterior`, at the points over the interior glass's) in the outside views; a car
without an exterior windscreen shows none there. It follows the drive one physics step per frame, with an 8 s warm-up
after a seek.

| file | role | from the data | ours |
| --- | --- | --- | --- |
| `mod.rs` | The windscreen and wipers, following the drive | the glass is the interior `i_window_f` mesh, the wipers `i_wiper_r` and `i_wiper_l`, placed as the interior set; the weather's rain or snow (`particles::Precipitation`); each blade's sweep from its park, `E88Wiper::InitTable` 0x5f8a50: 1.3823 rad for `*_wiper_l` (blade 0), 1.3195 for `*_wiper_r` (blade 1), both 0.4 s each way | the glass's chart (mean plane plus a 64x32 height grid), each wiper's pivot (the lower end of its parked mesh) and blade (beyond a third of its reach) |
| `model.rs` | Drops, flakes and bugs; the wipers' schedule and sweep | `misc/Part2D.ini` `[Effect Bug Splash]`: four colours, 10 s life, 2 s fade; `[Particle System]` speeds 25 and 75 km/h for the bugs | drop rates by intensity and speed, sizes, pinning against gravity and airflow, running drops merging, snow sticking and melting in 14 s, running drops leaving trail droplets (4 s), the wet film (a 48x24 grid wetted by rain, drying in 50 s, cleared where a wiper sweeps, thinned by the airflow to a third at 24 m/s), the snow a blade sweeps carried on it (at most 60 flakes, melting), the intermittent schedule (a 2 s pause at the park; 1.02 has none), the blades parking when the schedule ends (1.02's stop where they are), the wipers clearing water and snow in their swept area and leaving bugs |
| `gpu.rs`, `glass.wgsl` | Drop pass over the frame | | water drops are lenses sampling the resolved frame mirrored about their centre, darker at the rim with a sky highlight; flakes and bugs ragged opaque splats; the film a patch per grid cell blurring the frame (a 12-tap sunflower disc, 0.003 screen heights), its cover a quarter of the film blended between the cells' corners and fading out at the glass's edge, drawn under the drops; depth tested against the interior; the stage's ambient plus half its diffuse light |

The pass needs the resolved frame: it reads the screen-space reflections' one when the stage has them, else the
renderer resolves the frame into the glass pass's own texture and draws the particles and the glass over the kept
multisampled frame (`Renderer::draw_over`). The wiper meshes turn through `Playback::set_motion`.

Damage visuals (dmgvis-1):

| piece | from the game | ours |
| --- | --- | --- |
| Car dirt (`src/dirt.rs`, `shader.wgsl`) | the level: each tyre payload's spray amount (`mass`) times 1e-5, capped at 0.9, never falling (`OnTireParticles`, HH 0x10001420); `CarPaint` lerps its diffuse to the dirt colour by the `Dirt` map's alpha (`<car>_external_mud.dds`) times the level | the dirt colour (the manager's unused preset 0.27, 0.22, 0.14: `c3.xyz` has no writer); the reflection reduced by the dirt; the spray before a seek's warm-up from the poses or the recorded payloads; not drawn with `--no-particles` (the spray comes from the particles) |
| Mud on the windscreen (`glass/model.rs`) | the spray amount; each payload's surface id named by the exe's `MaterialManager` table (`lsp_keys`) | what each surface throws (`dirt::thrown`): gravel, dirt and rough mud, grass half, dry tarmac nothing (a quarter mud sprinkled, a half covered), damp and wet tarmac water drops (half, all), snow white-grey specks (lighter seen from inside, 0.75), the water surface water; specks per mass (0.01 at 20 m/s and over, fewer slower), sizes and colours, a wet blade clears them and a dry one leaves 35%; seen from inside, the shaded side (0.3 of the colour); after a seek, the specks the auto wipers would have left of the earlier spray (all below 300, then 105 plus what came after the last wipe at 300) |
| Exterior wipers (`glass/mod.rs`) | the `e_wiper_l`, `e_wiper_r` and `e_window_f` meshes | pivots and sweep by the in-car rule on the exterior windscreen; the in-car wipers' phase |
| Wiper switch (`glass::model::Switch`) | | off, intermittent, continuous, auto (by the weather, and intermittent in the dry once 300 specks of mud are on the glass); changes take effect at the park; `viewer --wipers`, the wiper key (I, `[controls] wipers`), the launcher's Wipers box (`wipers` in `launcher-settings.ini`) |
| Cracked glass (`src/crack_decal.rs`) | the projection, slots, halves and limits (`damage.rs`, `ProjectUVs`) | the texture: a generated 1024-texel spider-web crack (crushed core, forking rays, ring cracks) at `WS_Broken.dds`'s two star positions, in place of `WS_Broken.dds`; lit as before (`CarTextureNoDirt`'s class) plus (fx-2) a normal from the crack's alpha slope (4 taps, `CRACK_BUMP` 1.5, `mode.lit` 4) giving the cracks a sun glint and a mirrored sky (`CRACK_MIRROR` 0.6); the game's `WS_Broken` has no normal map |
| Mud film on the side and rear windows (`dirt::grime`, `shader.wgsl`) | the level | all of it (1.02's `CarGlass` binds no dirt map): `e_window_b` takes the level's full share, `e_window_lb`/`rb` half, `e_window_lf`/`rf` 0.3; the film covers `share x level x (0.5 + 0.9 noise)` (at most 0.8) of the pane, mottled by a value noise on its uv, hiding the glass and its reflection; the windscreen is left to the glass model's mud, which the wipers clear |
| Tyre and texture dirt (`car::dirt_texture`) | `CarTire` lerps to the dirt colour by `TireDirt`'s alpha (`<car>_external_dynamic_mud.dds`), `CarTexture` by `Dirt` | the level times the mask (the game's `TireDirt` texture effect, HH 0x100462d0, wants a friendly name `ExternalDynamic` no retail ini gives, so in 1.02 its `Init` fails and the tyres stay clean; it is not even made: `CTextureManager::LoadEffects` HH 0x100499b0 makes it only beside an effect of a `[Effects]` section in the car's `_shaders.ini`, and no retail car has one); non-square alpha-only masks (ACCENT, IMPREZA03 512x256) stretched to the larger side |
| Interior panels (`car::detach_part`, `car::follow_dents`) | | an `i_` panel detaches with its `e_` twin; a detachable interior panel without its own morph target (COROLLA's `i_door_l`, `i_door_r`) dents as its twin's nearest vertex |

Not done: a normal map for the cracks, camera dirt.
The game's lens drops on the outside cameras ("Camera Dirt", a 2D effect) are not ported (our `--enhance lens` was
removed, `notes/visual-enhancements.md`).

## Fences

`src/fences/` draws the stage's `.fnc` lines (parsed by `rbr_formats::fnc`), one stage mesh per line, at rest without a
drive. With `--drive` or `--live` the cloth steps once per frame with the car's pose (a jump back replays the drive
from its first frame), and the lines it moved are written back into their meshes.

| File | Role | From the game | Ours |
| --- | --- | --- | --- |
| `cloth/` | The cloth, pure (building in `build.rs`, the step kernels in `solve.rs`) | `g_fenceTypes`; `E90Group::Build`, `BuildSpan`, `BuildPost` (particles, springs, bends, pins); the kernels of `CFenceHandler::Tic` (forces once loose, Verlet with 0.96 damping, one spring pass that breaks a post past 0.5 m and tears a tape past 0.2 m, ground plane with friction, sleep and wake); a touched span from the post boxes ; the car's collision body (`c_e_body`, grid 1) with `EdgeEdge`'s first front-facing hit in face order and `E91AttachPairs`' rules (a post through it moves out along the hits' mean normal, a foot inside drags the top, a pin inside goes out from the centre); the stage's wind (`track_w.ini`, as the particles step it); a line resets only when out of the frame's view and 100 m from the car | the inside test is a ray parity (the game walks the grid's plane tree); the body sits on the car by the drawn model's offset (the physics' placing of the fence grid is not traced); the wind's `M8fb0` scale and the game's wind draws; the view is the frame's camera for every step it follows; the model's box stands in when the car has no collision body |
| `mod.rs` | Mesh, textures, follow | span quads and post rods with the type's uvs, textures from the `.fnc` names in `Textures/fence.dat`, post colours | the moving spans keep the post colours (the game's are white); the end quads to the posts are drawn; the stage objects' alpha test; the car's contact shadow (ours, `--enhance contact`) falls on the spans and posts under the car as on the ground. The game's `RenderShadows` (0x605670, `car_shadow.rs`, the default) lays `car_dpshad2.tga` on the `.col` faces under the car only, not on the fences (see `notes/interactive-scenery.md`) |

## Working dials

`src/dials/` gives an RSF car (`--rsf-car`) working dials when the RSF install has RBRHUD gauges for it
(`Plugins/RBRHUD/gauges/<car folder>/<variant>/config<N>.ini`, read by `rbr_formats::rsf::gauges`,
`notes/formats/rbrhud-gauges.md`). The RSF models ship their dial faces without needles; RBRHUD draws them onto the
interior textures each frame, and so does this module, on the GPU: each config's car texture becomes an `RGBA8` render
target (`Image::Dial`) beside its `remapped` copy, and when a value moves, `Renderer::draw_dials` queues a small pass in
the frame's scene encoder that restores the changed part of the dirty regions from the copy and draws the sprites
(needles, lights, digits) over it, at every mip level (`src/dials/dials.wgsl`). The values
come from the live HUD records: engine speed, speed, gear, race clock, the digidash's water temperature and turbo
pressure, the odometer and distance to go, the warning lamps and the controls. `--no-dials` (launcher: Working dials)
keeps the bare faces. Retail cars have no gauges.

| Faithful (the plugin's data) | Ours |
| --- | --- |
| Elements, sprites, fonts, value maps, conditions (`equal`, `threshold`, `limit`), effects (`Show`, `Hide`, `Blink`, `ColourChange`), the variant per car (`gauge_selections.ini`), the dirty regions | The plugin is not traced: positions as texels of the remapped texture (a needle's at its pivot, a sprite's at its top left), angles clockwise in texel space, the needle spring (`springconstant * rotationspeed`, `dampingratio`), the number formats and justification, modes 0 and 1 shown (no End key cycling), the whole texture redrawn for a config without dirty regions; fixed oil temperature 95 °C, oil pressure 4 bar and battery 13.8 V, no shift lights; no emissive masks, `Colour`, `Damage`, `Pie` or `Pulse` |

## Live mode

A development tool: a Python process steps the physics and the viewer draws it. The final app runs the Rust step
in-process. `viewer track-54_E --car COROLLA --live <command...>` drives the car from the keyboard. Everything after
`--live` is the command that steps the physics; `harness/live.py` runs the original's step in an emulator from a
snapshot of the Tanner drive, with every `rbr-hybrid` binding in Rust:

```sh
RBR_DATA=... RBR_PATCH=... rust/target/release/viewer track-54_E --car COROLLA \
    --live .venv/bin/python -I -B harness/live.py /tmp/rbr-snaps/vcu-cov.snap-2000.bin
```

The viewer writes one 32-byte control payload per physics step to the command's stdin and draws the FRM1 frames it
writes back through the playback path (`src/live/`). The payload is the recordings' layout, built in one place
(`Controls::payload`). Payloads go out at real time (below, "Camera views and HUD") and at most `IN_FLIGHT` ahead of
the frames received; `harness/live.py`'s hybrid step runs about 65 per second, so there the car moves at about half
of real time. Default keys: Up throttle, Down brake, Left and Right steer (ramped, 2 per second), Space
handbrake, A and Z gear up and down, R restarts (the stepping process again from its command: from the snapshot, or
with `--boot` the game's restart, a fresh `StartQuickRace`; `rbr-gameplay` README, Restart); no clutch key and no
gamepad inputs. When `rbr-standalone` reports the race's end (SPL1's over and retired flags) the split panel says
the stage is over, or shows a retire (DNF); the car is no longer stepped, as in the game's state 9. A helicopter
call in FLW1 (the race flow's retire codes 1 and 2) starts the rescue helicopter. A retire's message box
(`MSG_RETIRE_BYSTAND`, `MSG_RETIRE_OFFSTAGE`) holds the game paused until its accept (Enter, `accept message`),
which ends the race. Escape (or the `pause` binding, the gamepad's Start by default) opens the pause menu
(`src/hud/pause.rs`, the items `rbr_gameplay::pause_menu` traces per mode, from the live MNU1 record): Resume, Call for
help when the game offers it, Restart and Exit To Main Menu outside a championship, Retire (its confirm then the
`RETIRE` input flag ends the race retired), Continue after a championship stage's end; arrows and Enter or the d-pad
and South choose, Escape or East go back. While it is open no payload is sent (the game, its clocks and the car stand
still), the sound is paused (`rbr_sound::Player::set_paused`), the wheel's force is 0 and the live pacing
(`live::Pace`) leaves the pause out, so the drive goes on at 142.9 payloads a second with no catch-up; the
characters' and the wind's clocks hold too. The menu's look is ours, its texts the game's. `--bindings
<ini>` (before `--live`) takes other keys and gamepad inputs from the file's `[controls]` section (`src/bindings.rs`):
a key or gamepad button or axis for throttle, brake, steer left and right, handbrake, clutch, gear up and down, and
restart, and the steering deadzone. Gamepad inputs are analog: each pedal takes the larger of its key and its
gamepad input, and the steering takes the gamepad's when it is outside the deadzone. The camera keys, V and Escape
(the pause menu in live mode) stay fixed. `--screenshot` draws the snapshot's frame. `live.py` reports on
stderr how many steps used x87 results not from the CPU (it does not fetch them).

Live on `rbr-standalone`, the viewer plays the car's sound (`rbr-sound`, `src/sound.rs`) from the SND1 and PCN1 records
and the HUD's countdown after each frame (`rbr_recording::live`, `rbr_recording::sound`) on the default audio output:
the engine, the gear and diff whine, the turbo, the tyres, off-road rattles and impacts, and the co-driver's pace
notes, countdown and damage calls, with the in-car sound in the bonnet and cockpit views. `--no-sound` turns it off, `--volume V`
(0 to 4, default 1) scales it.

`rust/target/release/rbr-launcher` is a window that picks a car, map, the game's good or bad weather,
tyre wear, a tyre, window size (`--size`, in pixels), field of view (`--fov`) and sound volume (`--volume`). The game weather selects the stage variant and the viewer's
dry, damp or wet textures. The launcher always starts `rbr-standalone --live --boot`; it keeps the selections and
bindings in `launcher-settings.ini`. `--list` prints the startable map count and unavailable-map reasons without a
window. `scripts/launcher.sh` builds the three binaries and starts the launcher. Its Drive page holds the game's race
options (`rbr_gameplay::race_options`, `notes/race-options.md`): the damage level, which sets the outside-assistance
penalty, and the driver aids (automatic gearbox, gear protection, neutral lock, clutch help). It also
offers random weather. The launcher draws it from its own `rand()` stream, seeded with the time when it starts (ours),
and passes the draw as `--season-row` and `--wear`.

## Camera views and HUD

With `--drive` or `--live`, C (`[controls] camera`) cycles the views and `--view cockpit|bonnet|bumper|chase` starts in one (with
`--screenshot` too, so each view can be captured headless). The view stays until C is pressed again (R keeps it).
Ours (the user, 2026-10-18): with `--cameras INI` the view chosen with C is saved there and is the next drive's
first view (unless `--view` is given), and X (`[controls] camera adjust`; both bindings may also be a wheel button) adjusts the view for this car (`camera_prefs`: Left/Right sideways,
Up/Down forward and back, PgUp/PgDn up and down, the chase's Up/Down its distance and PgUp/PgDn its height;
Backspace resets, X or Escape finishes and saves), shown at the top of the HUD (also with `--no-hud`); the nudges are kept per car (retail
folder, or `rsf-<id>`) within 1 m (the chase within its ranges). The launcher passes its `launcher-settings.ini`. C is
the free-fly camera's down key only without a drive, where there are no views. Every `<car>.ini` has `Cam_*` sections
(`Pos`, `Target`, `Up` in the model frame, `FOV` in radians, `Near` in metres; `notes/formats/cars.md`), and
`E09_LoadCarCameras` (0x469210) reads `Cam_bonnet`, `Cam_internal` and `Cam_bonnet2`. For an RSF car the viewer takes
the `[Cam_*]` sections of `<car>_personal.ini` over the `.ini`'s (ours; whether RSF does is not traced). The viewer takes their place,
target, up and near plane but not their `FOV`. The bonnet, bumper and cockpit views, and the free-fly and orbit
cameras, have the one horizontal field of view of `--fov DEG` (30 to 120, default 50; the launcher passes its own, kept
in `launcher-settings.ini`; its label says "in-car views"). The chase keeps a fixed 65 degrees
(`CHASE_FOV_DEGREES`, ours), so the setting does not narrow the view of the car behind it. The vertical one follows from
the window's aspect, `2 atan(tan(fov / 2) / aspect)`, so a wider window shows more to the sides and the same height.

| View | Source |
| --- | --- |
| cockpit | `Cam_internal`, looking at least 2.8 degrees down (`COCKPIT_PITCH`, ours), moved 0.5 m back along the car (`COCKPIT_BACK`, ours), or further to 0.65 m behind the steering wheel's node where `Pos` lies over the dash (`WHEEL_BEHIND`, ours: the RSF Fiesta Rally3's is 0.17 m in front of its wheel), or less where the seat or the cage would then fill the view (`cockpit_back`). A car whose own `FOV` is narrower than retail's (the RSF models' 0.98 rad) looks down, by at most 5 degrees, until the wheel's hub is at most 3 degrees under the image (`hub_in_view`, ours: their displays sit on the wheel or inside its rim): COROLLA's eye is 0.33 m to the left, 1.02 m up, 0.73 m ahead of the origin, near 0.193. The steering wheel and the dash are in frame at 50 degrees, 16:9, and the eye is inside the cabin. The seat shake moves it (the game's mode 5) |
| bonnet | Ours: from `Cam_bonnet` (or, where an RSF model has copied `Cam_internal` there or emptied it, its `Backup_*` keys or a place 1.4 m ahead of the driver) the eye walks back along the top of the body to where the windscreen rises, 0.08 m over it, and tilts down until the bonnet's front edge is 10 degrees below the middle of the image (at most 8 degrees more than `Cam_bonnet`). Every retail and sample RSF model shows its bonnet at 50 degrees (`tests/car_data.rs`). Draws the exterior body (see below). Not done: the game's ground probe (mode 3, `M5760`) |
| bumper | Ours: centred, 0.05 m ahead of the model's front and 0.45 m above its lowest point, looking straight ahead, near 0.05 |
| chase | The game's mode 1 (`game_camera`, `M8840` 0x488840 and `M6530` 0x486530): the eye at a radius from a look point 1.54 m ahead of the car and 0.75 m up, at a height angle above it, looking a view angle down from that line; the 0.8 s lag over a 64-frame mean, the downhill lift, the height window and the distance clamp. The collision probes on the stage's `.col` (`stage_probe`): the wanted eye is pulled in front of what lies between it and the look point and pushed off nearby faces, the eye moves straight at it while pushed, and after the distance clamp the eye is put in front of what lies between it and the look point. Ours: the framing, 8.5 m, 5.6 degrees, 1.5 degrees up (`Settings::default`: the eye 6.9 m behind the car's centre and 1.6 m above it, the car in the lower third and the horizon about 40% from the top at 16:9; the game's camera page defaults are 4.4 m, 13 and 11 degrees, `Settings::GAME`, which crop the car at 65 degrees), and the distance held at the snapped one at every speed instead of the game's 3 m to radius + 1 m clamp (`Settings::steady`); 65 degrees horizontal (`CHASE_FOV_DEGREES`; the game's is 75); drag, wheel and V turn it about the look point; the look-back key (B, `[controls] look back`, held) drives the game's look-back swing, which 1.02 binds to nothing; on an RSF-encrypted `.col` there are no probes. `Cam_external` is not used: no traced code reads it |
| start orbit | Live play from the stage start only (`rbr-standalone --live --boot`, its CAM1 record): until the go, the game chase turned about the look point by the game's mode-10 swing angle (`views::start_orbit`), from in front of the car (pi) round to behind it (0), whichever view is chosen, with the eye at its place every frame (no lag). The angle, its timing and the geometry are the game's (`rbr_gameplay::camera_go`, `game_camera`). Ours: while the line of sight, or one 0.6 m to either side, crosses the stage (a start gantry's pillar on RSF 138), the eye comes in front of it, no nearer than 2 m (`game_camera::orbit_reach`) |
| helicam | While the rescue helicopter runs: its camera (mode 8, `characters::Characters::helicam`) at the game's 75 degrees (mode 8 keeps the last mode's), whichever view is chosen; C does nothing then, as the game's CH_CAM is blocked |

The seat shake (cockpit) is the game's: `CSigIncarShake` +0x30, the driver's seat spring at `CCar+0x14c0`
(`CPassenger` +0x8b0's host +0x70), read from each frame's car block and added to the camera's place in the car
(`views::Mount::shaken`); a few millimetres sideways and lengthways, about 12 mm down at rest. The game's C order is
chase (1), `Cam_bonnet` (3), `Cam_bonnet2` (4), `Cam_internal` (5); ours is cockpit, bonnet, bumper and chase
(the user dropped the rest, 2026-10-18).

The fixed views go through the same model-to-stage matrix as the body meshes (`Playback::mounted`), so they follow the
frame's pose with no lag and roll with the car. The cockpit view draws the interior set (`i_*`, `car::load_interior`)
and hides the exterior, as `CHH_Car::Render` does in the in-car views (`notes/formats/sgc.md`, section 9). Every retail
car has 19 to 26 `i_` sections. With the data's near planes, the COROLLA and LANCER_EVO_VII captures show no car geometry
cut open in the cockpit view.

Deviation (ours, the user's decision): the bonnet view draws the exterior body, not the interior set. `CHH_Car::Render`
draws only the interior set in the in-car views, and the bonnet view was empty with it: the bonnet is out of frame from
`Cam_bonnet`'s eye. The eye is placed over the bonnet's rear (the table above) so the bonnet shows at the bottom of
the image.

The HUD (ours, not the game's) is drawn with egui over the frame and scales with the window height (one point is
`height / 720` pixels). Its top-left clock is the race clock live on `rbr-standalone`, whose records are `HUD2`: the
start countdown before the go, then the race time with the penalties added so far and the jump start
(`rbr_gameplay::race_clock`). Without a race clock (playback, and `harness/live.py`, which sends `HUD1`) it shows the
drive time instead: the frames' substeps since the first frame, 7 ms each (`rbr_recording::STEP_SECONDS`). It also
shows the speed, the gear and the engine speed (from the live HUD record, `rbr_recording::hud`; the recordings carry
the speed only, so a drive shows `-`), and the throttle, brake, handbrake, clutch and steering of the frame's control
payload. The rpm bar ends at 10,000 (ours).

Every clock runs on the game's 7 ms physics step (`PhysicsEngine.MaxTimeStep`). The game's frame splits wall time into
whole steps and ticks the countdown, the camera's start orbit and the HUD's race clock by those steps' time
(`rbr_gameplay::tick`). Live mode sends one payload per step of wall time (`live::STEPS_PER_SECOND`, about 142.9 a
second), so each answer is one game frame of one step: the car, the countdown, the go and the race clock run at real
speed and agree. The payloads never lag real time by more than 0.1 s (`live::MAX_BEHIND`): `CPC_Application::Vf0c`
(0x44a3d0) cuts a frame's wall time to 0.1 s (0x795e30), so after a stall (the load before the first drawn frame, a
dragged window) the game goes on from where it was, as the original does, instead of running fast to catch up. Closing
the window closes the stepping process's input, so `rbr-standalone` ends on its own (it writes the rally's files then)
and prints its answers' times; the viewer prints the pacing (drawn frames, the payload periods sent against real time, the time not
played, the long frames). `--quit-after S` closes the window after S seconds as Escape does, for unattended runs with
`rbr-standalone --pilot` (`lead/reports/goal-e2e.md`); `--quit-after end+S` S seconds after the race is over. Playback, the particles, the glass, the fences, the in-car crew, the ghost's times and the sound
models step 7 ms per substep too.

Under NGP (`rbr-standalone --ngp-physics`) the step is 1/720 s. The payloads still go out one per 7 ms, and each is a
game frame of 5 or 6 NGP steps (the frame header's substeps; the frame clock carries the rest). So the game's clocks
run at real speed as under 1.02, and every presentation model still takes one answer as 7 ms of wall time. The answer's
step clock record (CLK1, `rbr_recording::clock`: the step and the load settle's steps, 1520 under NGP) gives what a
step is to the drive time (`Drive::parse_with`), the ghost's race time and the wipers' clock. A frame of several steps
sends each wheel's last tyre payload, all the frame's particle events, and the sound, dash and force after its last
step; its first step takes the payload's gear flags (`FrameInput::replay_tail`).

The split record (`SPL1`, `rbr_recording::splits`) from `rbr-standalone` sets the split panel (`splits`, ours, not the
game's layout): after a split is passed its time and its difference from the `times.ini` reference show for 5 s of
stage time (green ahead, red behind); after the finish a panel shows the stage result: the race time with the
penalties, the finish against its reference and each split. With `--saves` the standalone adds the standing: the
splits and the finish are compared with the stage record's row (`pfresult.rst`, equal to `times.ini` until a record is
set), the split panel names the record, and the finish panel shows the stage record with its holder, the personal
best (the profile's record) and the new-best message the game's result page shows (`MSG_QRBEST`, `BEAT_PERSONAL`,
the English strings; "this stage" in `BEAT_PERSONAL` is ours). After the finish the result page (`CQRResultUI`)
shows the time, the penalties, that message and the Stage Record, Personal Best and Player rows by split; the
retire pages (0x12, 0x17, 0x18) name why the stage ended, and while the rescue helicopter flies the HUD is hidden
(`HUD::Update` 0x578c30) under our caption. `hud::retire` draws the red and black tints of the rally incident and the
off-stage timer (FLW1, `HUD::Update`'s 0x573630 and 0x5736b0), a retire's message box over the paused game, and page
0x18's FAILED text (`MSG_UNFINISH_SERVICE` or `_STAGE` by what follows the stage); the texts are the game's, the
panels ours. The launcher's quick race passes `--saves` (the saves next to it) and the Championship page's
profile name. `harness/live.py` sends no split record, so no panel.

The rest of the 1.02 HUD's information comes from `rbr-standalone`'s dash record (`DSH1`, `rbr_recording::dash`) and
the pace-note record (`PCN1`), fed to `hud::model` once per live answer (one 7 ms frame): the countdown calls ("3", "2", "1",
"GO" in the countdown sounds' windows) and FALSE START for 3 s after a jump start; the pace-note symbol at the top
(the game's tag map 0x5721c0 and colours, an 8-note queue of 1.5 s each with the game's fades; the drawing is ours,
`hud::pace`); the five warning lamps of the sensor receiver 0x573cc0 with the lamp table 0x7d1900's thresholds,
blinking at 1 Hz, with the water temperature and turbo pressure; the stand-still and car-problem messages for 4 s; the
progress bar (the game has it but hides it); and a shift light at 5500 rpm. The body damage percentage is ours (the
1.02 HUD shows none). The dash record also carries the wrong-way test (0x579590; the banner after 2 s, fading in
over 3 s), the rally's false-start count (FSWAR_1-3 for 10 s, 0x573100), the odometer and distance to go (0x5715b0;
shown in the speed unit's km or miles, ours), the start clock's time of day (drawn on the start board's face,
`start_board`: HedgeHog3D 0x10070b20's `hh:mm:ss` cells from `start_clock_digits.dds`, written into the
`START_CLOCK_DIGITS_PLACEHOLDER` face texture rather than drawn as quads over it, ours; and as a panel until the go,
ours) and the keys of the pace stack (`--pace-stack N`, the game's 0x578c30 slots and
alphas; off by default as in the menu) and of the 3D notes (`--pace-3d`, AID_MARKER; our symbols over the
driveline). `--mph` shows the speed in mph and `--no-hud` hides the HUD; the launcher keeps both in
`launcher-settings.ini` (`units`, `hud`). In playback the HUD shows what the recording carries.
`examples/hud_capture.rs` captures it over a recorded drive and `examples/hud_symbols.rs` draws the symbols and the
countdown (a live boot has none); `examples/hud_states.rs` captures the wrong way, the false-start warnings, the
result and retire pages and the helicopter; `examples/retire_states.rs` drives a championship stage at the champion
level and captures the tints, the message boxes and the end pages 0x17 and 0x18 (typed over its last frame where the
drive does not reach them).

## Checks

- Texture mapping against a capture (render-m10): `viewer track-54_E --weather damp --camera ex,ey,ez,fx,fy,fz --size
  800x600 --screenshot <png>` at the eye and forward axis of an `apitrace-check` snapshot, compared with the
  `d3dretrace` snapshot of that frame (`notes/formats/shaders.md`, Tools).
- Road and start: the start view shows the road under the camera running away ahead, tyre tracks visible, so the
  mirror and culling conventions give a visible front face.
- Object lanes: trees, spectators and barriers in track-10_O render in place.
- Screenshots at the start, one per country: `track-10_O` dry (Arctic, Finland), `track-20_O` damp (British, Wales),
  `track-31_E` dry (Australia), `track-51_N` wet (Hokkaido, Japan).
- `cargo test -p rbr-formats -p rbr-render` with `RBR_DATA`, and with `RBR_PATCH` as well, `cargo clippy --all-targets -- -D warnings`,
  and `cargo fmt --check`.

## Open questions for milestone 3

- The global clouds: the render transform (`CHH_KClouds::Render`, `src/hedgehog3d/HH_KClouds.cpp:114`) is marked
  approximate. The faces sit at their stored positions relative to the camera, so they look like a ceiling. The
  face is the section's `CloudName`; the section itself is the renderer's pick (`atmosphere.rs`).
- The global sky dome: `SkyDome.vso` computes its colour from the scattering constants (`c41`-`c66`). The stage
  sky faces may use the same shader; their vertex colours are drawn unchanged for now. Inputs and gaps: see Clouds
  and sky items (`notes/formats/shaders.md`).
- Specular (types 6 and 8): drawn with the game's formula and data (below, Road shine), but `c90` (the highlight's
  sharpness and scale) has no proven stage source, so those two numbers are the renderer's. Slots 7 and 8 shadows are
  drawn.
- Level-of-detail distance: fixed at the high preset's 600 m (`stage::LOD_DISTANCE`); the reference installs and the
  capture ran `RenderQuality = low`'s 200 m. Other presets use 300 or 500.
- Mip levels: the game creates textures with the `.ini` `MipLevels` (up to 6) and generates the levels the DDS files
  lack. The renderer uploads the 4 stored levels.
- Stage colour: the capture is darker and browner. The vertex shaders' extinction and in-scatter are not reproduced.
- Clamp samplers per geometry chunk: no retail source found for stage geometry; every geometry draw wraps.
- Object decals and blend modes: the kind byte's blend bits are not decoded.
- Missing damp and wet textures: the reconstructed `LoadTextures` (`src/hedgehog3d/HH_TrackLoaderTextures.cpp`, about
  lines 426-582) has no fallback: a missing ground texture throws `error loading texture`. Its weather key is
  hard-coded to 0, but the decompile (`HedgeHog3D.dll.c` ~110876) reads the `Vf1c` key, and the folders are
  `dry`, `damp` and `wet`. The season table keeps the 35 stages' maps dry in every row, so the game never asks for the
  missing folders (not traced in the loader itself). The viewer falls back to a drier folder (above), ours; it reports
  only the requests that find no file at all.
- Visibility `(2,10)` and visible objects `(2,18)`: the loaders are known (`notes/formats/lbs.md`), but the consumer
  that gives them meaning is not traced, so they are not used for culling. Culling uses the mesh bounds only.

## Car (milestone 3)

`viewer --car <name>` shows a car alone under an orbit camera (drag to turn, W/S to zoom). With a stage it puts the
car at the stage start (`(2,11)`), its front along the start heading, and the start view is a chase view behind the
car. The chase offset (6 m back, 2.5 m up, aimed at the car's middle) is ours, not the game's camera, which is not
traced. `Car` in `src/car/` is the loader; it reads `Cars/Cars.ini`, the model (`sgc`), `<car>.ini`
(`car_parts`), `<car>_shaders.ini` and the DDS textures.

Conventions: a mesh's vertices are at `F * P * v`. `F` is the node's chained world (`sgc::chained_worlds`: its stored
matrix, composed with its parents'; the car root is the origin). `P` is its 14-word transform block as
`HH_H7ReadTransform` (0x1000c7a0) reads it and `NLB_ComposeTransformEH` (0x1008d420) composes it:
`T * R1 * R2 * S * R2^-1`, with the translation in words 0 to 2, the rotation `R1` (angle, axis) in words 3 to 6, the
scale in words 7 to 9 and `R2` in words 10 to 13 (`coords::part_to_car`, the same rule as the collision grids in
`rbr-physics`). The wheels carry a half turn about `-y` that turns their rims outward, and the side doors one that
undoes their node's rotation. The car's front is its `-z`, its `+x` is its left, and it is placed on the stage with
`coords::car_to_stage` (a proper rotation). A mirrored `F` reverses its triangles. The format facts and their
evidence are in `notes/formats/sgc.md` (section 5.2) and `notes/formats/cars.md` (the part, material and texture
chain).

Drawn: diffuse texture times white vertex colour, lit (below), with the face's material. The state follows
`SShaderMaterial::SetupRenderState` (0x1003f4b0), which every car class runs: a material with an opacity channel is
alpha blended (source alpha), alpha tested `> 0`, with no depth write and back faces culled, whatever its class. The
capture agrees (frame 175: the `CarTexture` tyre and the `CarGlass` panes). `CarPlastic`'s `Load` then turns depth
write on and culling off, and `CarIntGlass` is drawn two-sided. The faces are drawn in the game's order
(`FUN_1001e430`): the wheel list (`CarRim`, `CarTire`), then the opaque list, then the transparent list
(`IsTransparent`: `CarIntGlass`, and `CarGlass`, `CarPlastic`, `CarLight` with an opacity channel), each by shader in
the order the loader created them. So the blended `CarTextureNoDirt` window panes (`pc_e_window_*`) are drawn in the
opaque list, before the glass, as in the game. Screenshots are written as RGB: the back buffer is `X8R8G8B8`, and
texels such as COROLLA's number plates have alpha 0. Not drawn: ghost parts, the dirt and scrape overlays
(`texture_*`), shadow planes (`Plane_*`), switched parts not in their start state, gravel and snow wheels and spare
wheels, the interior set `i_*`, and the `k_` and `d_` parts. The switched parts (`Switch = true`) start hidden, and
`CHH_World` (vf14, 0x100690a0) sets the light, brake and reverse switch boxes to `off`, so the parts with
`Predicate = off` (the unlit lamps and brake discs) are drawn. The loader never builds `k_` and `d_` parts as
geometry, and `CHH_Car::Render` draws the interior set only in the in-car views (the viewer's cockpit view; see
Camera views and HUD). Drawing it from outside put the interior door panels through the doors (render-m10).
Each skipped mesh is listed in the load report with its reason.

Checks: `tests/car_data.rs` (`RBR_DATA`) loads the eight cars with no missing texture, bad triangle or unknown shader
class, with 106 blended draws and 23 `CarPlastic` draws, and places XSARA at the track-10_O start.
`all_retail_cars_draw_only_their_exterior_set` checks that no `i_`, `k_` or `d_` part and no gravel or snow wheel or spare
wheel is drawn. Its shader tests check the
class-to-file mapping and, with `RBR_PATCH`, that the Fog files are the patch's bytes. `rbr-formats` `tests/real_data.rs` checks that
the rear doors' interior and exterior panels coincide under the chain in all eight cars.
`all_retail_cars_turn_their_rims_outwards` checks that each car's rim faces point away from the body, and the hinged
panel test now covers COROLLA's right door. `coords`, `car` and
`rbr-formats::sgc` have unit tests.

Open (milestone 4 and 5 unless noted):

- Milestone 5 (`notes/formats/shaders.md`, Material roles): the car classes are drawn with the base texture only, which
  is their start state. Dirt and scrape are zero at the stage start (dirt level and damage weights). Open: the reflection
  cube and the tyre dirt colour (`c3.xyz`).
- Lighting (render-m12, the renderer's model): the game's car lighting needs constants with no proven source (`c41`,
  `c67`, `c68`, `c90`, the dirt ratio; `notes/formats/shaders.md`). The renderer lights each face with its class's
  section of `<car>_shader_settings_clear.ini` (`Ka`, `Kd`, `CookTorranceKs`, `CookTorranceM`, `FresnelMin`,
  `FresnelMax`, `Kf`; the overcast file is not used) and the stage's atmosphere: `0.5 * (texture * (Ka * ambient *
  hemisphere + Kd * sun * max(n.l, 0)) + 4 * Ks * highlight * sun)`, ambient and sun being `Ambient*` and `Diffuse*`
  times `Car_Ambient_Lighting` and `Car_Diffuse_Lighting`, the highlight `exp((1 - 1/(n.h)^2) / M^2)`. A reflection of
  the stage around the car (below) stands in for the reflection cube, weighted by the game's Fresnel:
  Schlick with `F0 = 0.125` (`c91`), clamped to `[FresnelMin, FresnelMax]`, times `Kf`, and by the diffuse alpha
  (`CarPaint`'s `t0.a`). Normals are the `sgc` normals through the inverse transpose of the part's placement (and the
  pose in playback); two-sided faces flip them on their back side. Every drawn retail class has a section (a class
  without one would stay unlit).
- Glass: the opacity map is not read (the diffuse texture's alpha is used). `CarGlass` adds `0.5 * F * Kf` to its alpha,
  as its 1.02 pixel shader does, with `F` from the drawn normal and eye.
- Materials with no diffuse bitmap are drawn white. Their `Diffuse` colour and the `Ambient` and `Specular` terms are
  not used.
- Vertex colour: the model has none (its aux stream is always white, `0xffffffff`), so white is drawn.
- Surface and state: which wheel set the car shows is not traced; the tarmac wheel is drawn. The caller of `CHH_World`
  vf14 (the switch start state) is not traced.
- Mirrored parts: their triangles are reversed so the outside is the front. The game's own culling is not checked.
- Doors: the hinge's default angle (0.01 rad) turns the door panels only, not the windows on them. Whether the game
  turns those with the door body is not traced.
- Start placement: the car's origin (the rear axle) is the start position, its front the start heading (row 2 of the
  `(2,11)` matrix). The height of the origin above the road is taken as given. Neither is checked against the game's
  camera.
