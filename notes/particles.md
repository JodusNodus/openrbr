# Particle system: handoff

Goal: a Rust particle system for the new renderer. It should draw tyre spray, dust and smoke, gravel, grass,
water splashes, exhaust, debris and sparks, weather (rain and snow) and skidmarks, driven by the same data and
physics signals as 1.02. This note collects what is known as of 2026-10-16. No Rust code exists for it yet;
`rust/rbr-render/README.md` lists particles under "Not drawn".

## Scope and fidelity

- Particles are presentation. The physics does not read their state: `rbr-hybrid/src/pure.rs` `LISTENERS` skips
  every particle receiver in the pure step, and no car or wheel word changes over the checked drives. They use the
  CRT `rand()` (`E82_Rand1` 0x5dad60 and the `rand()` calls in `RBR_E82_C.cpp`), not the physics' own seed
  (`rbr-physics/src/car_body/damage.rs` `next_draw`). So a Rust port can use its own RNG and can never break the gate.
- **Decided (user, 2026-10-16): good-looking particles, not an exact match.** No matching, no bit-exact state, no
  harness comparison of particle values. Use the 1.02 data and signals as the starting point (which surface emits
  what, colours, sizes, lifetimes, sprites), and the reconstruction as a reference for how things roughly work. Free
  to change: the simulation maths, emission rates, sorting, lighting (sun and fog tinting, soft particles, depth
  fade), texture filtering, particle counts. Wherever the original looks worse than a simple modern approach, take the
  modern approach.
- In scope by that reading: the 3D system (`NParticleSystem::`), skidmarks, the 3D stone particles, weather emitters.
  The 1.02 2D windscreen effects (`NParticleSystem2D::`) are **not ported**: a modern glass-water system replaces
  them (see In-car glass effects). Out: `ParticleTweakerD.dll` and the dev menus (E84 glue).

## Data (all retail, all present in `RBR_DATA`)

| File | What |
|---|---|
| `Maps/Tracks.ini` `[MapNN] Particles = "Maps\ps_<rally>"` | Picks the rally's file: arctic (10 maps), Australia (7), british (24), japan (7), montblanc (9), usa (8). |
| `Maps/PS_<Rally>.ini`, `Maps/PS_<Rally>_LOW.ini` | The particle definitions ("version3" text, 885 to 3,631 lines, comments explain every field). |
| `Root/misc.rbz` `Misc/particletextures.ini` | 61 sprite texture names (`Textures\*.tga`). The particle class's `sprite id` indexes this list, 0-based. Example: id 13 is `br1_pebble` for the gravel spray. |
| `Textures/particles.dat` | `CHELALIC` archive (`rbr_formats::dat`) holding the sprites as `.dds` (the ini says `.tga`), plus `skidmarks.dds`, `splashcolor.dds` and the 2D `WS_*.dds` windscreen textures. |
| `Textures/splashalpha.tga` | 2D splash alpha (`CPC_Wave`). `Textures/splash.dat` holds the loading-screen splash images, not particles. |
| `Root/misc.rbz` `Misc/particletweak.ini` | Name / particle class / emitter class / emitter quads for the tweaker. Lists SNOW, SNOWHEADLIGHTS, Rally6Smoker, SnowDust. Probably dev-only; check whether 1.02 reads it outside the tweaker. |
| `Meshes\stone0.sml`, `Meshes\stone1.sml` | The 3D stone meshes (`CSMLModel` 0x5eb350). **Not found** in the retail install or any `.rbz`; a simple generated rock mesh is fine. |
| `VertexShaders/fog/Particle.vso`, `PixelShaders/fog/Particle.pso` | Particle shader: one texcoord (v7). Render state: alpha blend, alpha test > 10 or > 1, no z write (`notes/formats/shaders.md`, Other draws). |
| `RichardBurnsRally.ini` `[Settings] ParticleQuality`, `RenderParticles` | Quality and on/off (`RBR_E04_Init.cpp:420`). |

File choice (`CPC_TrackState`, `src/rbr/RBR_E09_TrackHead.cpp:436`): no `Particles` key means no particle system.
Maps below 0x48, and maps 0x4a, 0x4f and 0x50, load `<Particles>_low.ini` when `ParticleQuality` is `low` and
`<Particles>.ini` otherwise. Every other map always loads `_low`. Ours always loads the full `<Particles>.ini`
(`rbr_render::particles`; user decision: always the highest quality). The loader is `E83System::LoadFile` (0x5dffd0,
`src/rbr/RBR_E83_D.cpp`). It turns `$` into the end of the text, tokenizes on `;` comments, and checks `version3`.

### `PS_*.ini` sections, in file order

Field names come from the file's own comments. Offsets come from `LoadFile`, matched by read order. Check them
against `RBR_E83_D.cpp` before relying on them.

1. **General settings**: 4 leaf colours (RGBA, from system +0x780), air friction (+0x7c0).
2. **Particle classes** (count at +0x760, array at +0x764, 0x130 bytes each). Fields: name (+0; a name containing
   `headlights` sets +0x104), sprite id (+0x40), start colour, intermediate colour set-in, intermediate colour, end
   colour (13 floats from +0x50), start/intermediate/end size (w h each) plus size set-in (7 floats from +0x90),
   size variance +0xac, life +0xb0, weight +0xb4, trail length +0xb8, trail variance +0xbc, camera attenuation
   start/end +0xc0/+0xc4, blend mode token (`BLEND_BLEND`, ...; `E83_ReadBlend` 0x5dfa70) +0xf0, ground z push
   (int) +0xf4, offset width +0xf8, offset height +0xfc, rendered-in-car-space flag.
3. **Emitter classes**: name, emitter type, then fields that depend on the type, then frustum-only flag,
   back-plane kill distance, force variance, and a list of particle class names. Types across all 12 files:
   - 0: no extra fields, 8 uses (sparks);
   - 1: speed, diversion, 144 uses (smoke);
   - 2: base width/height, force/deviation, shear, 183 uses (tyre spray, gravel, grass);
   - 3: an identifier token (for example `CAR_ENGINE_STRIP`, a car dummy) plus speed/deviation, 42 uses (fire,
     steam).
4. **Emitters**: name, emitter class, max particles, emit rate, camera-speed modulation, then up to four forces.
   Force 1 is an acceleration vector (gravity `0 0 -9.81`) and force 2 is friction. Forces 3 and 4 are unused in
   the samples. A force with value 0 has no parameter line.
5. **More general settings**: max live 3D particles.
6. **3D particles** (the stones): lifetime, weight, restitution, friction, collision radius, inertia, gravity scale,
   mesh id (`stone0`/`stone1`), colour, force flags, cull flags. They collide with the ground, so the Rust version
   needs stage collision queries (rbr-physics/rbr-hybrid already have the stage world).
7. **Signal handlers** (tyre emission per surface): material id, emitter name; mass-flow, smoke-flow and velocity
   reduction/modifier/perturbation; equalization, flags, car-speed factor, speed reduction/modifier, limit.
   Several handlers can share a material, and each one feeds its own emitter.
8. **Skidmarks**: material id, texture index (into `skidmarks.dds`, see `E82D_Atlas::Row` 0x5dbe20), RGB, size
   modifier, flush threshold, general add, rubber/depth/dirt reduction and multiplier.
9. **Material aliases** (`from to` pairs), then **skidmark aliases**: these map surface material ids onto the
   handled ids (`self+0x418` and `self+0x518` in `EmitTire`).
10. **Fake smoke signal handlers**: material, dirt/smoke/speed add and mod, speed factor, emitter.
11. **Damping signal handlers** (bushes): material, emitter, penetration add/mod, velocity mod. PS_British's four
    handlers name `ho_DAMP_grass1_emitter` and `ho_DAMP_grass2_emitter`, which the file does not define (the only
    dangling names in the retail data; `rbr-formats/tests/particles.rs`).

Every weather emitter (`g_rain*`, `g_snow*`) has max particles 1: the game's precipitation does not run through the
emitter's pool size. Map 54 (Tanner, the reference drive) names `ps_japan`.

After the file, `LoadFile` looks up named emitters that the code uses directly. Not every file defines all of them
(`rbr_formats::pss::NAMED_EMITTERS`): every file has `g_debris_{b,c,smoke}`, `g_engine_steam`, `g_exhaust_broken`,
`g_fire`, `g_firesmoke` and `g_pop`; the arctic files have `g_exhaust_arctic_emitter` instead of `g_exhaust_emitter`;
PS_Australia has no rain or snow emitter. `g_car_sparks`' class has life 0. The names:
`g_exhaust_emitter`, `g_exhaust_broken_emitter`, `g_pop_emitter`, `g_fire_emitter`, `g_firesmoke_emitter`,
`g_engine_steam_emitter`, `g_debris_{a,b,c,smoke}_emitter`, `g_car_sparks_emitter`, `g_carwater_emitter`,
`g_splash_emitter`, `g_splash_smoke_emitter`, `g_rain_emitter`, `g_rain2_emitter`, plus snow and headlight
variants (strings in the exe; no retail `PS_*.ini` defines a headlight emitter, see `weather.md`). Material ids are the stage surface ids, the same as `.mat` and
`rbr_formats::mat`. Check this mapping.

## Code map (1.02 exe, reconstruction in `src/rbr/`)

All in S12 (0x5d7ec0-0x5f7560, `notes/exe-plan.md`). The reconstruction compiles and pairs, but most functions
are partial matches. Read it as structure and as data flow; for exact arithmetic, read the Ghidra output in
`out/decomp/`.

| Batch | Range | Content | Files |
|---|---|---|---|
| E82 | 0x5d7ec0-0x5dea10 | `CEmitter` (Step 0x5d9640, GenVel, FindSlot, Activate), `CEmitParam`, `CEmitterRain`, `CParticleSystem::Step` 0x5daea0, spawn loops, name lookups (`FindEmitter` 0x5dcfe0), table lerp, `E82F_Emit::Run` 0x5de140 (signal-handler emission), `E82F_Post::Run` 0x5de950 | `RBR_E82.h`, `RBR_E82_A-F.cpp` |
| E83 | 0x5dea10-0x5e4a00 | `E83System` (the 0xa90-byte `CParticleSystem`, global at 0x166e378): `EmitExhaust` 0x5dea10, `EmitDebris` 0x5df5f0, `EmitDebrisRing`, `GenTyrePos`, `Coverage`, `LoadFile`, `Update` 0x5e2850, `EmitTire` 0x5e3050, `Init` 0x5e34d0 (13 signal connections), `Reload` | `RBR_E83.h`, `RBR_E83_Sys.h`, `RBR_E83_A-D.cpp` |
| E84 | 0x5e4a00-0x5eaa40 | tweaker glue (out of scope), tyre-track rings (skidmarks: 4 rings × 0x100 marks × 0x40 bytes at 0x7d6d48; `M5e9010` extends a mark, `M5e9120` starts one), `E83PartHost` (renderer manager at 0x167e7e0), `E84TrackMgr` (`CTrackEmittersManager`, stage-placed emitters bound to animation dummies) | `RBR_E84.h`, `RBR_E84_Ring/Host/Emit.cpp` |
| E85 | 0x5eaa40-0x5f0230 | `CPC_ParticleRenderer` (vtable 0x74eb64; billboards, sorting `RBR_E85_Sort.cpp`), `CSMLModel` (stones, skidmark meshes) | `RBR_E85.h`, `RBR_E85_*.cpp` |
| E86-E87 | 0x5f0230-0x5f7560 | `CPC_Wave` (2D splash), `NParticleSystem2D` effects (rain, wipers, bug splash, camera dirt), `CWiperData` | `RBR_E86*.`, `RBR_E87*.` |

Per frame: `CApplication` calls `UpdateParticleSystem` (0x5e34c0), which calls `E83System::Update(dt)` (frame
time, not the 7 ms physics step), and then the renderers draw. Layout notes for each batch are in
`notes/exe-plan.md` under "### E83/E84/E85/E86 result".

## Inputs: signals

`E83System::Init` connects 13 signals: CarGeneralState, Camera, Exhaust, TireParticles, CarParticles, Collision,
CameraModeChange, ReplayCameraChange, CarDamage, DampingCollision, EngineFire, ParticleSystemSunParams,
EngineNoise.

The pure step reaches these receivers (`pure.rs` `LISTENERS`):

| Receiver | Signal | Emitted from |
|---|---|---|
| 0x5e3050 `EmitTire` | `CSigTireParticles` | each wheel, every physics step (`E51MultiTire::Emit` 0x53d0d0; `rbr-hybrid/src/wheel.rs`) |
| 0x5e3e50 → `EmitDebris` | `CSigCarDamage` | the damage handler |
| 0x5dd460 `ScanEmit` | damping event | `CSolid::Emit` (bushes) |
| 0x5dd400 | `CSigEngineNoise` | drivetrain |

No drive reaches the engine-fire receiver (0x5e3f10). **Open:** where Exhaust, CarParticles, Collision, Camera and
SunParams are emitted. They are not in the physics step's listener list, so the emitter is probably game-side or
per frame. Use Ghidra xrefs to the signal's slot in the global object.

`CSigTireParticles` payload (uint indices `s[i]` as in `EmitTire`; byte offsets in brackets). The meanings of
rubber, depth and dirt come from their pairing with the skidmark coefficients; verify them:
- `s[0]` [+0x00]: wheel index 0-3;
- [+0x10], [+0x20]: two contact-patch edge points (vec3 each); the skidmark is laid between them, scaled by the size
  modifier and lifted 0.03;
- [+0x80]: rubber; [+0x88]: particle amount (HedgeHog3D's car dirt reads this too); [+0x90]: depth;
  [+0x94]: dirt; [+0x98]: added ×0.1 to system +0x838;
- [+0xa0]: material triple (`E36Triple::MinPick` gives the surface id);
- [+0xac]: in-contact flag.

The rest is read by `E82F_Emit::Run` (0x5de140) and `E82F_Post::Run` (0x5de950). Read both to finish the layout.

## Proposed implementation

1. **Parser** (`rust/rbr-formats/src/pss.rs`): `PS_*.ini` into typed sections, plus `particletextures.ini`. Add an
   `RBR_DATA` test over all 12 files that checks every name reference (class → particle class, emitter → class,
   handler → emitter, `g_*` lookups). Write the format spec as `notes/formats/particles.md`, referring here.
2. **Event capture in the standalone.** Record each particle-relevant signal as `(kind, payload bytes)` while the
   physics step runs. Recommendation: hook the emits themselves (the `emit` call in `rbr-physics` wheel/damage
   code, or `signal::emit` keyed by signal address), not the listener stand-ins. `rbr-standalone --boot` builds
   no particle connections (`notes/rust-plan.md` decisions), so its emits have no slots and never reach a
   listener. This touches the pure step, so it needs the user's or lead's approval. It must not change any
   compared word: `scripts/rust-check.sh` and a long run must stay identical.
3. **Transport to the viewer.** Live mode (`rbr-standalone/src/live/` ↔ `rbr-render/src/live/`) sends one FRM1
   frame plus a HUD record per payload. Add a third record type for the step's particle events, or run the particle
   simulation in the standalone and send the particles; the first keeps the simulation next to the renderer.
   Playback of recordings (`playback.rs`) has no signal payloads. There, particles would need payloads rebuilt from
   recorded wheel state, or stay live-only.
4. **Simulation** (`rust/rbr-render/src/particles.rs`, or its own crate if it grows): emitters, classes, forces,
   colour and size lerp over life, per-frame `dt`, pools capped by max particles and max live 3D particles. Write it
   directly from the data's meaning. Read `CEmitter::Step`/`GenVel`/`E82F_Emit::Run` and `EmitTire` only for how
   the signal values turn into emission (mass flow, smoke flow, speed terms); a close approximation is enough.
5. **Rendering**: camera-facing quads, sorted back to front, alpha blend, no depth write. Sprites come from
   `particles.dat`. Lit by the stage's sun and ambient (`atmosphere.rs`) and hazed like the stage, with a soft
   depth fade where quads meet geometry. Skidmarks are ground strips (decals) along the wheel path. Use `coords.rs`
   for axis conversion: the game's particle space is z-up (gravity `0 0 -9.81`), the renderer's is y-up.
6. Then exhaust, damage debris and sparks, weather (`CEmitterRain`, `E21` weather setters), damping bushes, stones,
   then the in-car glass effects below.

## Status (particles-1)

Built: steps 1, 3 to 5 for the tyres, exhaust and precipitation (`rbr-formats/src/pss.rs`, `rbr-render/src/particles/`,
README "Particles" with faithful and ours per file). Step 2 for the tyres only: `rbr-hybrid`'s `Pure` copies each
`CSigTireParticles` payload at the call to 0x53d0d0 with `ecx` = `[0x893600]+0x4518` when `keep_tyre_particles` is set
(live mode only; no Mem write), and the live protocol sends them as a `PTC1` record (`rbr_recording::particles`) after
`DMG1`. The game does not signal every wheel every step. Payload: wheel +0, contact edges +0x10/+0x20, velocity +0x50,
rubber +0x80, smoke +0x84, mass +0x88, depth +0x90, dirt +0x94, splash +0x98, surfaces +0xa0/+0xa4, blend +0xa8 (the
handled surface is +0xa0 below 0.5), contact +0xac. Playback rebuilds the values from poses and the `.col`/`.mat`
ground. Not built: the 3D stones, camera-speed modulation. Soft (depth-faded) edges are drawn under `--enhance particles` only (`fs_soft`, ours; the game has none).

## Status (particles-3)

Built: debris, dust and (ours) sparks from `CSigCarDamage`, engine fire, steam and exhaust pops, the damping handlers
(bushes), the water splash tables and the `E84Wave` sheets, and the game's precipitation rule with `track_w.ini`.
`Pure` also copies the payloads at 0x4ef1d0 (`CSigCarDamage`, `[0x893600]+0x9f20`), 0x52be70 (damping, `+0xadc8`) and
0x4ef290 (`CSigEngineFire`, `+0xb840`, 4 bytes) when `keep_tyre_particles` is set; the live protocol sends them, with
the engine noise words (`car+0x690` -> `+8` -> `+0x2c`: word 4 misfire, word 5 steam), as a `PEV1` record after
`PTC1`. Findings: 1.02 throws no sparks (`g_spark_emitter` is in no file, `g_car_sparks` is empty) and the collision
handler `E82_BinLoad` is effectively empty; the damping handlers are on material 168 in `PS_Japan` and `PS_British`
only; the Tanner damage drive hits only material 28, and none of the drives tried (tanner-damage, v2 gravel 21 and 24)
has water or material 168, so the splash and bush captures use its events with the splash level set to 0.3 and the
damping material relabelled 168. Playback calibration on tanner-damage: contact agrees with the live payloads 100%,
surface 99.6%, live smoke is 0 on gravel; mass follows slip through a table (`SLIP_MASS`). Not verified against the
original game's screen: the wave's drawing, the pop and steam rates in a real drive (no fire or steam in the drives).

### Status (particles-4): the wave's drawing and the splash smoke

- `CPC_Wave::Vf04` (0x5f05e0) builds the sheets as a strip of triangles: 6 vertices per slot, 5 quads between two
  neighbouring slots, FVF 0x2c2 (XYZ, diffuse, specular, two texture sets). Texture set 0 is the colour: `u` the sheet's
  length (`E84Wave::Emit`'s `+0x44`, 0.2 a metre from the first slot), `v` the profile point times 0.2. Set 1 is the
  alpha (`splashalpha.tga`): the same `u`, `v` flipped. The vertex's alpha is `(0.55 - age) * 10/3` clamped, zero at
  the top point and on a sheet's first slot. The earlier band-per-quad drawing mapped the whole texture into every
  quad, which made the grid.
- The M68 (`E86SplashV`) outputs for the vertex alpha are untraced; so the vertex alpha is the random share times the
  level alpha times the ramp (ours). The sheets read soft and faint at 0.3.
- The splash smoke at the 0.3 forced level covered the road: its pool was four times the file's 50 and its alpha was
  doubled by the light-class density. The water emitters now keep the file's pool and skip the density (ours).
- No drive has water: the wet drive on track-54_O (`drive-wb-wet-corolla-54`, the race's wet stage) produces no
  splash payloads (`splash` is 0 at every step), and playback from poses carries no splash.


### Status (fx-1)

- Rain and snow are the game's `CEmitterRain` (`Init` 0x5daa90, `Step` 0x5da450, the camera handler 0x5da2f0), made
  by `LoadFile` 0x5dffd0, not the files' `g_rain_emitter`/`g_snow_emitter` (max 1, dropped): `particles/rain.rs`
  with the instance table in its doc. `--enhance particles` keeps the user's rule (`Precipitation::by_wetness`).
- Tyre rate: `E82F_BlkD0::Emit` 0x5ddba0 emits `rate x [E83System+0x94]` (the frame dt `Update` 0x5e2850 stores) per
  payload, so the x4 became `GAME_FRAME / STEP` (1/60 s is ours). Playback spray takes the anchors' velocity (the
  live payload's `+0x50` is the patch's, within a few cm/s on tanner-damage), so the kick is gone.
- Exhaust: `EmitExhaust` 0x5dea10 adds `p0 x 2` (0x7d648c) to `g_exhaust_emitter` (+0xd8, the arctic one where the
  file has it) and `p1` to `g_exhaust_broken_emitter` (+0xe0), pops from `g_pop_emitter` (+0xdc) when `p4 > 0.8`, at
  most 2; launch 0.95 of the car's velocity. `CSigExhaust` comes from `Sim8CF0` 0x518cf0 in the per-frame
  `CCar::Vf28x` 0x4ea020, which the standalone does not run, so live.rs sends the engine's words as PEV1 kind 5:
  the flow `state+0x200` (smoothed 0.7/0.3 as the game does) and `engine+0x188`. The engine is embedded at
  `car+0x690`; the old engine-noise read dereferenced it twice and sent nothing. Live flow on tanner-damage: 0 to 0.113,
  mean 0.049. Ours: the exhaust spin (omega x r) is left out; the pops use the engine-noise misfire word.
- 3D stones: `0x5ebce0` loads `Meshes\stone0/1.sml` and `leaf0/1.sml` through `CSMLModel` 0x5eb350 (no NULL check
  on `fopen`), but nothing calls, jumps to or points at `0x5ebce0`, so 1.02 never loads or draws them. None ported.
- Damping: `ScanEmit` 0x5dd460 and `E82D_Fed::Emit` 0x5dbf90 agree with `damping.rs`.

### Status (leftovers-1)

- A live answer of several payload periods (a late viewer's, `Input::frames`; sound-7) is one game frame of that
  many 7 ms periods, as `E83System::Update` takes the frame's whole time: the simulation, wind, track emitters, rain,
  car effects and the exhaust step by it, and the tyre handlers emit for all its steps from each wheel's last payload
  (the standalone keeps one per wheel; the count is `rate x GAME_FRAME` per step, so x periods). The skid marks'
  accumulator gets the periods' `GAME_FRAME`s. `Playback::periods` reads the count from the echoed payload; a
  recording's frames are one period each.
- Playback of a recording takes the game's PTC1 and PEV1 from its replay (`particles/sidecar.rs`, made once by the
  viewer with `rbr-standalone --events-out`), so the poses' payloads (ours) are only the fallback.

## In-car glass effects (replaces `NParticleSystem2D`)

Decided (user, 2026-10-16): an in-car camera is planned, and the windscreen gets a modern water-on-glass system instead
of 1.02's screen-fixed sprites (rain, wipers, bug splash, camera dirt, "Effect Break"; E86-E87, `WS_*.dds`). Build it
after the world particles, once the in-car camera and the glass mesh are drawn: it depends on them.

- **On the glass, not the screen.** State lives in the windscreen's texture space (one render target per glass
  panel), drawn on the car's glass mesh, so drops have correct parallax with head and camera movement.
- **Refraction.** Each drop is a small lens: the glass shader samples the rendered scene (opaque pass copied to a
  texture) with offsets from the drop's normal and a slight blur, so drops show the inverted road behind them. Wet
  film between drops adds a faint blur and smear. This is the main realism win.
- **GPU water simulation** (a compute or fragment pass over the water texture): drops spawn with the rain or snow
  intensity, merge, and run down under gravity; car speed adds airflow that pushes them up and outward; moving drops
  leave thin trails that evaporate. Snow sticks and melts slower.
- **Wipers** write a mask from the blade angle each frame that clears water and leaves a thin film at the edge. The
  angle comes from `CSigWiperPositions`' source or our own wiper animation.
- **Dirt and damage in the same space.** A dirt layer builds up from spray (the tyre-particle amount, splashes,
  other cars) and is smeared, not fully cleared, by the wipers. The cracked windscreen is a decal with a normal map on
  the glass, lit by the scene (replaces `WS_Broken`).
- **Outside cameras.** Lens drops or dirt on chase cameras are a screen-space texture that the sun and bloom light,
  replacing "Camera Dirt".
- **Inputs only from 1.02:** weather intensity, car speed and velocity, wiper position, tyre spray amount, damage
  (windscreen state). No 1.02 sprite code or `WS_*` textures.
- **Check:** screenshots and short captures from the in-car camera in rain, snow and on a muddy stage, wipers on and
  off, at low and high speed, reviewed by the user; frame rate holds.

Built (particles-2, `rbr-render/src/glass/`, README "In-car glass"): drops, flakes and bugs in the windscreen's chart
(`i_window_f`, a mean plane with a height grid) with pinning, airflow, merging and melting; the `i_wiper_*` meshes
turned about their pivots (the low end of each parked mesh) on our automatic schedule; water drops drawn as lenses
over the resolved frame. Simpler than the design above: the drops are CPU instances, not a GPU water texture. Added in
visuals-3 (all ours): trail droplets behind running drops, a wet film on a 24x12 grid of the chart (drawn as blurring
blobs; the wipers thin it and leave a wetter edge), snow carried on the blades, and the drops drawn on the exterior
windscreen in the outside views; fx-2 added crack normals, the `TireDirt` layer and lens dirt (ours; removed
2026-10-18). Data used:
`misc/Part2D.ini` (bug colours, life and speeds). The continuous schedule is 1.02's (`E88Wiper`: 0.4 s each way in any
rain or snow, [weather.md](weather.md) section 7); the sweep angle and the intermittent schedule are ours.

## Verification

- The parser test covers all 12 files.
- After step 2, the pure step's car and wheel words are unchanged (rust-check plus a long run on tanner-corolla).
- Visual review: viewer screenshots (`viewer --screenshot`) of gravel spray, tarmac smoke, snow, a water splash and a
  crash, shown to the user. Original-game screenshots are a reference for intent, not a target to match.
- Performance: the viewer keeps its frame rate with the busiest stage files (`PS_British`, `PS_USA`).

## Open questions

- Where the in-car glass mesh and the windscreen damage state come from (car `.sgc` glass parts, damage signals).
- Emitters of the Exhaust, CarParticles, Collision, Camera and SunParams signals; the full `CSigTireParticles`
  layout.
- Whether `misc/particletweak.ini` is read in a normal 1.02 run.
- Camera dirt (`[Effect Camera Dirt]`, `CParticleEffectCameraDirt` vf08 0x5f23f0): what triggers it. 0x5f6790 updates
  the twelve 2D effects and 0x5f8720 is the wiper; the trigger is not traced.

## Track emitters (fx-2)

`CTrackEmittersManager` (0x450 bytes): `+0` count (at most 0x40, "Too many emitters"), `+4+4i` dummy id, `+0x104+4i`
cloned emitter, `+0x204+4i` switch, `+0x304+4i` squared distance, `+0x410..0x418` camera position.

- Load: `E83PartHost::M5e96f0` 0x5e96f0 clones the particle file's emitter of each `.dls` table-4 row's name (none
  found: slot 0 and the log "was not found"), places it at the table-2 dummy and zeroes the switches.
- Frame: `M5e9490` 0x5e9490, from 0x5e2fec in `E83System::Update`: `FilterB` 0x5d93f0 (rate scale 0) unless the switch
  is on and the camera's squared distance is below the row's, then `FilterA` 0x5d9360 (scale 1); then `Step` 0x5d9640.
- Camera: handler 0x5e9470 copies `CSigCamera` `+0x24..0x2c` into `+0x410`.
- Switch: handler 0x5e9550 (`CSigAnimationDummyChange`, connected in 0x5e9ac0) sets it to the dummy's `+0x40 & 2`.
  `E12_AnimSet::Tic570` 0x47a570 sends that signal only for dummies with bit 0 set, which only animation channels
  set (0x477a50, called at 0x477e99, and 0x475e40, registered at 0x47b0eb). No channel in `.dls` table 0 names an
  emitter's dummy, so the switch stays 0.
- Retail rows: 82, all chimneys: 10_O and 14_O five `Chimney` rows each (distance² 10000), 41_M, 41_O, 46_E and 46_O
  eighteen `Chimney_Smoke` rows each (90000). No retail `PS_*.ini` (nor the RSF samples seen) has those emitters.
  RSF 294 (N/O) has 18 rows too.

So the track emitters never emit in 1.02. `particles/track.rs` keeps that rule; chimney smoke is ours under
`--enhance particles` (`notes/visual-enhancements.md`).
