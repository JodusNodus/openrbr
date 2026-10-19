# Weather presentation and wetness

How the 1.02 game chooses the weather of a stage and what each choice changes: the sky and fog, rain and snow
particles, the windscreen effects and wipers, and the wet road. Addresses are `RichardBurnsRally_SSE.exe` 1.02
unless marked HH (HedgeHog3D) or LM (LocoMotion). Labels: **loader** = read from the decompile or our
reconstruction of the named function; **file** = checked on retail data (counts say over which files); **inferred**
= plausible, not proven. Particle data and simulation: [particles.md](particles.md). Car lights:
[car-lights.md](car-lights.md). Stage textures and `.lbs` layout: [formats/lbs.md](formats/lbs.md). Clouds and
skyboxes: [formats/shaders.md](formats/shaders.md) (Clouds and sky). Collision and `.mat`:
[formats/col.md](formats/col.md), [formats/stage-files.md](formats/stage-files.md).

## Summary

- **State.** One object, `CWeatherDaemon` (global `DAT_007c36a8`, 0xbc bytes; [../src/rbr/RBR_E22.h](../src/rbr/RBR_E22.h)),
  holds the weather of the current stage: track letter, cloud part, weather preset (10 ids), wetness (0-2), wear
  (0-2) and the wind. Its virtual getters build the names used by everything else (**loader**).
- **Choice.** The state comes from one of four places, depending on the application state: the season/stage record
  (`E80Daemon`), values stored by the race start or a ghost, the tweak and challenge tables, or fixed values for
  two special maps (68, and 4 for wear). A quick race (`StartGame`) takes one row of the season table (section 3,
  "Quick race"); `rbr_gameplay::weather` derives it.
- **Presentation.** The stage's `Maps/TrackSettings.ini` section (name built from the state) gives the sun, ambient,
  fog and cloud face. HedgeHog3D reads it. Precipitation is 3D emitters set at track load from `misc/track_w.ini`
  and the stage's particle file. The windscreen drops and wipers are 2D effects.
- **Wetness, which is gameplay.** Wetness and wear pick one of nine `.mat` tables per stage. Its cell bytes are surface
  ids. Every stage's wet surfaces are separate physics records, and in all 19 full sets the wet record has lower static
  friction than the dry one. Puddle meshes (collision list A) load only when wetness is 2.
- **No thunder or lightning** appears in the exe, the extracted `misc`, `physics` and `Scripts` archives, or the
  plain `Maps` files.

## 1. Data sources

| File | Read by | What it selects |
|---|---|---|
| `Maps/TrackSettings.ini` (405 sections) | HH (`0x10067930` reads `maps\TrackSettings.ini`; the exe has no key strings) | Sun, ambient, fog, cloud face and skybox per stage and weather |
| `Maps/Tracks.ini` `Particles = ...` | exe (`CPC_TrackState`, `RBR_E09_TrackHead.cpp:436`, see [particles.md](particles.md)) | Which `PS_<rally>.ini` (no key: no particle system) |
| `Maps/PS_<Rally>.ini`, `_LOW` | exe `E83System::LoadFile` | Rain, snow and car-water emitters per rally (section 6) |
| `misc/track_w.ini` (27 sections `[TrackNN]`) | exe `E21_WeatherA` (`0x4a0910`), `CWeatherDaemon::M97e0` (`0x4a97e0`) | Wind and precipitation fall height and forward distance per stage |
| `misc/Part2D.ini` | exe 2D effects (`NParticleSystem2D`, E86-E87) | Windscreen rain, wiper, bug splash and camera dirt sprites |
| `misc/nice_w.ini`, `misc/bad_w.ini` | exe `E23SndParam::LoadWeather` (`0x4aa970`) | Per-stage index lists, nice and bad weather (sound side; section 4) |
| `Maps/Clouds.lbs`, `Maps/skyboxes.lbs`, `Clouds_textures.rbz` | HH | Cloud faces and skybox domes; the `CloudName` and `SkyboxName` keys name them |
| `Maps/track-NN_X.mat` (86 files) | exe `E09_LoadMaterials` (`0x46a290`) | Nine tables per stage: wetness x wear (section 5) |
| `physics/Physics/physics.lsp` (62 `*_Dry/_Damp/_Wet` records) | physics loader | Friction and water factor per surface |
| `physics/Physics/{pirelli,michelin}/tyres.lsp` | physics | `WetSlope_T0..T5`, `WetnessDesign`, `WetnessWearFtr` (the only two tyre files with these keys) |
| `Textures/particles.dat` (`WS_*.dds`) | exe E05 (`RBR_E05_Part.cpp:451`) | Windscreen sprites |

## 2. Weather state (`CWeatherDaemon`)

Fields that matter here (**loader**, from setter and getter addresses; names ours):

| Offset | Meaning | Setter (`0x...`) | Getter (virtual or direct) |
|---|---|---|---|
| `+0x10` | map (stage id, = track number) | `M9220` `0x4a9220` | `Vf20` `0x4a8ee0` |
| `+0x14` | track letter, state 2/3 | `M92d0` `0x4a92d0` | `M9d50` `0x4a9d50` |
| `+0x18` | cloud id, state 2/3 | `M9300` `0x4a9300` | `M9c90` `0x4a9c90` |
| `+0x1c` | wetness, clamped to 2 | `M93c0` `0x4a93c0` | `Vf1c` `0x4a9c40` |
| `+0x20` | wear, state 2/3 | `M93e0` `0x4a93e0` | `Vf18` `0x4a9330` |
| `+0x3c` | float | `M9230` `0x4a9230` | (tweak and challenge set it; saved with ghosts) |
| `+0x40` | cloud letter, state 2/3 | (direct store) | `M9cf0` `0x4a9cf0` |
| `+0x44` | weather id, state 2/3 | (direct store) | `M9360` `0x4a9360` |
| `+0x5c` | flag: stage row exists (state 4) | `M9240` `0x4a9240` | (read by `Refresh`) |
| `+0x60`, `+0x70`, `+0x80`, `+0x90` | wind vectors: base, perturbation, previous, current | `M97e0`, `M9a20` | `M8fb0` `0x4a8fb0` |
| `+0xa0` | wind perturbation time | `M97e0` | (read by `M9a20`) |

**Application state to weather state** (`Refresh`, `0x4a9250`). `Refresh` reads the app state word through
`FUN_00565470` (`0x565470`) and maps it: 2 gives state 3, 3 or 5 gives state 1, anything else gives state 2. State 4 is
map 4 with `+0x5c` set. **loader.** The meaning of the app-state values is not traced.

**Getters** (**loader**):

| Getter | Returns | Table |
|---|---|---|
| `Vf04` `0x4a9fc0` (uses `M9d50` `0x4a9d50`) | track letter, `0`-`4` | `M`, `N`, `E`, `O`, `S` (the default, state 0 or 5+, is 3 = `O`) |
| `M9cf0` / `Vf08` `0x4aa050` | cloud letter, `0`-`2` (else empty) | `M`, `N`, `E` |
| `M9360` / `Vf0c` `0x4a9430` | weather id, `0`-`9` | see the weather table below |
| `M9c90` / `Vf10` `0x4aa0c0` | cloud id, `0`-`3` | `clear`, `partcloud`, `lightcloud`, `heavycloud` |
| `Vf1c` `0x4a9c40` | wetness, `0`-`2` | `dry`, `damp`, `wet` |
| `Vf18` `0x4a9330` | wear, `0`-`2` (else 1) | `new`, `normal`, `worn` |
| `Vf14` `0x4aa1b0` | track name for TrackSettings (below) | |

State 1 reads the same values from `E80Daemon` (`GetSubB10` `0x5d0710` weather, `GetSubB11` `0x5d0730` cloud letter,
`GetSubB12` `0x5d06b0` cloud, `GetSubB13` `0x5d06e0` wetness, `GetA0c8` `0x5d0750` wear, `E80Sub54::Get4` for the letter).

**Weather ids** (`Vf0c` strings and the `TrackSettings.ini` section names agree; **loader** and **file**):

| id | Name | Precipitation amount (`E23`) | Rain or snow (`M9e60`) |
|---:|---|---|---|
| 0 | `crisp` | none | |
| 1 | `hazy` | none | |
| 2 | `norain` | none | |
| 3 | `lightrain` | light (amount 50) | rain |
| 4 | `heavyrain` | heavy (amount 100) | rain |
| 5 | `nosnow` | none | |
| 6 | `lightsnow` | light (amount 50) | snow |
| 7 | `heavysnow` | heavy (amount 100) | snow |
| 8 | `lightfog` | none (fog only) | |
| 9 | `heavyfog` | none (fog only) | |

The precipitation classes are `FUN_005b5560` (`0x5b5560`: ids 3, 4, 6, 7) and `FUN_005b5590` (`0x5b5590`: 4, 7 are
heavy) (**loader**). Amount: `E23SndParam::Update` (`0x4aab40`) sets the target to 50 (light) or 100 (heavy) with
constants `0x7304a0` and `0x72bf10` (**loader**). The amount reaches `E25AppObj::M9b80` (`0x4a9b80`), which returns
`+0x34 * +0x30 + +0x2c` (the `+0x2c` target) and 0 for map 68.

**Track name** (`Vf14`, `0x4aa1b0`, **loader**). The string is `"%u%s_%s%s%s_%s"` with map, track letter, `_`, cloud
letter (only O and S tracks), `_` (only O and S), weather preset, `_`, cloud. Examples from the file: `5O_N_norain_lightcloud`,
`54O_E_lightrain_heavycloud`, `04E_crisp_clear`, `68M_crisp_clear`. Map 68 has one section and gets the fixed
`crisp` `clear` values (section 3).

## 3. Where the state comes from

Each path below is **loader** unless noted.

- **Race start (stored state).** The race-start path sets the map (`CApplication::M00481a50`, `0x481a50`, from
  `g_7eac64`). The weather choice enters as `DAT_01660848` through `NFilter::CRBRGameStub::StartGame` (`0x626a70`:
  `eWeather` is stored, then `M9220(iMap)` and the daemon's `Update`, `0x4aab40`). See "Quick race" below.
- **Quick race** (`StartGame`, **loader** from the decompile, **file** for the table, checked on 7 snapshots).
  `Update` (`0x4aab40`, the class the C++ calls `E23SndParam`; `this` is the daemon) fills the daemon from one row of
  the season table (`0x7d34c8`, rows `map, letter, cloud, cloud letter, weather id, wetness`):
  - map 4: letter, cloud, cloud letter, weather, wetness and wear all 0;
  - `eWeather` 0 (good, `InitA` `0x4aa630`): the first row of the `misc\nice_w.ini` list (row numbers, loaded by
    `LoadWeather`) whose map matches (`M9650`); wear 0 (new);
  - 2 (bad, `InitB` `0x4aa760`): the same from `misc\bad_w.ini` (`M96f0`). It does **not** write the wear: the wear
    stays what it was (the constructor `0x4a9980` leaves it unset; the first race of a harness game has 0, new, on
    the wet and damp snapshots). `src/rbr/RBR_E23_Ambient.cpp`'s `InitB` draws a wear with `rand()`, which the exe
    does not (C++ observation, not fixed: the C++ is frozen);
  - 1 (random, `E74_FindBest` `0x5b53b0`): the map's row `rand() % n`, then wear `(rand() % 10 > 6) + 1` (normal or
    worn);
  - a map with no listed row hits `int3` in `InitA`/`InitB`: among the retail stages only map 68 (good and bad).
  The wetness is clamped to 2. The row's letter is the stage variant: `StartGame` passes `M9d50` (the letter) to the
  stage load, which loads `Maps\track-<map>` (`Tracks.ini` `TrackName`) `_<letter>`. Good weather loads a non-`_O`
  variant on most maps; bad loads `_O` except Australia (same letter as good) and the single-variant maps.
  No weather loads `track-4_E`, `track-4_N`, `track-4_O` (map 4 is always `M`) or `track-54_M` (no row of map 54 has
  letter `M`). Good weather is damp on all of Japan (51-56); on Japan's stages the dry and damp `.mat` tables are the
  same bytes (S17). Bad weather is wet (2) on British, Mont Blanc, Japan and USA except 64 (row 206: `lightfog`, damp);
  Arctic and Australia stay dry in every weather.
- **Ghost and replay records.** `CGhostCarManager::Save` (`0x492eb0`) writes the state fields (track letter, cloud,
  weather, cloud letter, wear, wetness, `E25AppObj::M9b80` and `+0x3c`) into the ghost block. The apply code
  (`E18_ConnGhostMemory::Dispatch`, `0x4930d0`) calls `M9220`, `M92d0`, `M9300`, `M93c0`, `M93e0` with the saved values.
- **Tweak.** `E19ApplyTweak` (`0x499ac0`) copies a tweak record into the same setters.
- **RB challenge.** `FUN_005a7850` (`0x5a7850`) takes the weather of a challenge from a 0x28-byte table at `0x7d289c`
  and loads the ghost `Ghosts\rb_<n>.gho`.
- **Fixed maps.** `FUN_005abc60` (`0x5abc60`, map 0x48 = 72) and `FUN_005abd30` (`0x5abd30`, map 0x47 = 71) set
  zeros.
- **Season (state 1).** The `E80Daemon` stage block (`GetSubB10..13`, `GetA0c8`) is read while the app state is 3 or 5.
  How the block is filled from the season data (`season.dat`, `CSeasonStageManager::LoadData` `0x5d62e0`, per
  [formats/cars.md](formats/cars.md)) is not traced.
- **Season table (`DAT_007d34c8`).** 319 rows of 0x18 bytes (E74 owns, [../src/rbr/RBR_E74_UI.h](../src/rbr/RBR_E74_UI.h)).
  `FUN_005b53b0` (`0x5b53b0`) picks one row for a key at random (`rand() % count`). The caller is `E23SndParam::Update`.
- **Special maps.** `M9360`, `M9c90`, `M9cf0`, `M9d50` and `M9b80` all test the map against the one-entry table at
  `0x731560`, which holds **68**. For map 68 the letter is `M`, cloud `clear`, weather `crisp`,
  amount 0. Separately `E11_TyreCond` (`0x46f6c0`) forces the wear word to `new` for map 4 (`DAT_01660804 == 4`), and
  the wetness word stays `dry` (section 5).

## 4. Load path (stage load)

1. Track-loader state 4 (`CPC_TrackLoaderDaemon::Vf1C`, `0x46f330`): the car is loaded, then `E09_LoadWeather`
   (`0x46b160`) emits `CSigStuff` with the stage's cloud or weather kind (`+0x34c`, `RBR_E09_Loaders.h`). The payload
   is not decoded.
2. State 5: `CPC_TrackState::LoadWeather(map)` (`0x4675e0`) calls `E21_WeatherA(map)` and `E21_WeatherB` (empty).
3. `E21_WeatherA` (`0x4a0910`, reconstructed in [../src/rbr/RBR_E21_Track.cpp](../src/rbr/RBR_E21_Track.cpp) lines 280-330):
   - `M9220(map)`; reads `misc/track_w.ini` through `M97e0` (`Track%02d` section, wind and perturbation keys, time).
   - If `M9b80 > 0` (precipitation) and `M9e60 != 0` (rain): sets the E82 flag, `Sel684(add-car-velocity, 0)`,
     `SetWheelEmitter("g_rain_emitter", RainFallHeight, RainForwardDistance)`, the same for `g_rain2_emitter` with
     the `...D` keys, `SetEmitterC4("g_carwater_emitter")`, `SetEmitterEC("g_rain_headlights_emitter")`.
   - Else (snow): the `Snow...` keys and `g_snow_emitter`, `g_snow2_emitter`, `g_snow_headlights_emitter`.
   - Missing keys use 13.0 (fall height), 0.5 (forward), 1 (add velocity). The `...D` keys are never in
     `track_w.ini`, so the second emitter always uses the defaults (**file**).
4. Collision and material load: the collision file loader `CPC_TrackLoaderDaemon::LoadCollisionFile` (`0x46a460`)
   calls `E09_LoadMaterials` (`0x46a290`) for the stage's material block (section 5). Its list-A (puddle) branch (`0x4fdff0`,
   `FUN_004fb220`) runs only when `Vf1c() == 2` (wet) (**loader**; resolves the gate noted in
   [formats/col.md](formats/col.md)).

Missing emitter names are not an error: `E83System::SetWheelEmitter` looks the name up in the stage's particle file, and
retail files lack some of the names (section 6), so those calls do nothing (**inferred**).

## 5. Wetness and wear: the `.mat` tables and the surface ids

- `E11_TyreCond` (`0x46f6c0`, **loader**): wear word from `Vf18` (0 `new`, 1 `normal`, 2 `worn`); wetness word from
  `Vf1c` (0 `dry`, 1 `damp`, 2 `wet`). Map 4 gives `new` and `dry`, whatever the weather.
- `E09_LoadMaterials` (`0x46a290`, **loader**) takes the first entry whose name contains both words (`strstr`).
  `rbr_formats::mat::select` does the same (`rust/rbr-formats/src/mat.rs`).
- **file.** Every one of the 86 `.mat` files has 9 entries: `<rally> standard <wetness> <wear>` (for example
  `arctic standard dry new`). Six files are empty (9 entries of 289 bytes, see [formats/stage-files.md](formats/stage-files.md)).
- The cell bytes are surface classes. `value = 88 * id + 3` in the exe's table (`0x7caf38`) gives the physics surface
  id (see [formats/col.md](formats/col.md), "Surface id"). On track-10_O (arctic, snow) the road tags are the same in
  the dry, damp and wet entries, which fits snow surfaces having no wet variants (col.md). On most other
  stages the `damp` and `wet` tables differ in bytes from `dry new` (rough byte comparison, segment boundaries
  approximate, not decoded per cell; `dry normal` and `dry worn` differ on about half the stages).
- **Physics.** The wet surfaces are separate physics records (**file**, `physics.lsp`). Example, tarmac fine clean:

  | Surface | `myStatic` | `myKinetic` | `WaterFactor` |
  |---|---:|---:|---:|
  | `Tarmac_Fine_Clean_Dry` | 1.54 | 1.50 | 0.0 |
  | `Tarmac_Fine_Clean_Damp` | 1.40 | 1.32 | 0.3 |
  | `Tarmac_Fine_Clean_Wet` | 1.166 | 1.0433 | 1.0 |

  Over the 19 base surfaces that have all three records: wet/dry `myStatic` is 0.71-0.88 and wet/dry `myKinetic`
  is 0.65-0.86. Damp/dry `myStatic` is 0.77-1.01, so damp is not always lower (**file**, computed from the lsp; not a
  runtime check).
- **Tyres.** `WetSlope_T0..T5`, `WetnessDesign` and `WetnessWearFtr` are all in `pirelli/tyres.lsp` and
  `michelin/tyres.lsp`, and in no other tyre file (**file**). They are `CTyreTypes` reals (`+0x18c`, `+0x19c..+0x1ec`,
  `+0x69c`); `CTyre::CScratch::CopyDefFrom` (`0x534760`) copies them into the tyre definition (`+0x2dc`, `+0x2e0..`,
  `+0x310`; `rust/rbr-physics/src/tyre/layout.rs` `DEF_FROM_SETUP`). `CWheel::Unk9900` (`0x539900`) calls
  `CTyre::CScratch::BlendTable` (`0x534e70`) with the two contact materials' `Type` (`+0x148`, the slope index) and
  `WaterFactor` (`+0xe8`); each grip is `base + WetSlope * |WetnessDesign - WaterFactor|`, scaled by the wear factor
  (**loader**). So the tyre reads no weather state: the weather reaches the tyre only through the `.mat` table's
  surfaces (dry, damp and wet records with their own `WaterFactor`). Nothing else in the physics reads the daemon
  (the E33-E51 sources and the daemon's getters; `+0x3c`, 20 or -10 by weather, is read only by the ghost save).
- Australia (tracks 31-36, 28 sections) has no rain or snow preset in `TrackSettings` (**file**).

## 6. Precipitation particles (3D)

Data per rally (**file**, `Maps/PS_*.ini`, full files; `_LOW` has the same names):

| Particle file | Rally stages (`Tracks.ini`) | Weather emitters present (emit rate, raw) |
|---|---|---|
| `PS_Arctic.ini` | 10-15, 18, 93, 94, 98 | `g_snow_emitter` (1000), `g_snow2_emitter` (600), `g_snow3_emitter` (600); no rain |
| `PS_British.ini` | 20-25, 28, 71-79, 82-89 | `g_rain_emitter` (600), `g_rain2_emitter` (600), `g_carwater_emitter` (300, max 100) |
| `PS_MontBlanc.ini` | 41-46, 49, 80, 81 | rain, rain2, carwater (as British) |
| `PS_Japan.ini` | 51-56, 59 | rain, rain2, carwater (as British) |
| `PS_USA.ini` | 61-66, 68, 69 | rain (600), rain2 (600), `g_snow_emitter` (1000), `g_snow2_emitter` (600), `g_snow3_emitter` (600) |
| `PS_Australia.ini` | 31-36, 39 | none (`g_splash_emitter` only) |

Every emitter in this table has max particles 1, except the car-water emitter (100). The rate and max values are
the raw file values; their units are not checked.

- **No headlight emitters.** None of the 12 files defines `g_rain_headlights_emitter` or `g_snow_headlights_emitter`
  (0 matches for "headlight"). `car-lights.md` and `particles.md` say they are in the files; that is wrong for the
  retail data (**file**). The calls in `E21_WeatherA` therefore find no emitter (**inferred**).
- Rain emitter class (`PS_British.ini`, `g_rain_emitterclass`): type 2 (base 6 x 6, force 9.81 (gravity), deviation
  0.02). Particle class `g_rain_particleclass`: sprite id 26, grey-white alpha 0.2, life 0.6 s, weight 4, trail 0.035.
  Snow (`PS_USA.ini`, `g_snow_emitterclass`): type 2, base 15 x 15, force 0.5, deviation 0.2, variance 0.2.
  Simulation and sprites are in [particles.md](particles.md).
- Wind. `CWeatherDaemon::M9a20(dt)` runs every frame from `E83System::Update` (`0x5e2850`). It advances the
  accumulators: every `WindPerturbationTime` (1.0 s) it sets the current wind (`+0x90`) to the base (`+0x60`) plus
  `WindPerturbation` (`+0x70`) times a random value in [-1, 1]. `M8fb0` (`0x4a8fb0`) gives the scaled wind to the
  particle motion: it writes the current wind (`+0x90`) scaled by `accA4 / pertA0` through `E11_VecLerp`, and the E82
  emitter kind-4 step (`RBR_E82_B.cpp:41`) lerps each particle's velocity toward that vector (**loader**; the exact
  lerp argument order is not checked). The rain init (`FUN_005daa90`, `RBR_E82_C.cpp:608`) draws per-drop random values and the wind (**loader**).
- Wind values (**file**, `track_w.ini`): `WindX 10`, `WindY 4`, `WindZ -2`, perturbation `2`, `5`, `1`, time `1.0`, in
  all 27 sections. Rain fall height is 7 (21 sections) or 13 (6); snow fall height 12 (26) or 7 (1); forward distance
  0.5 (rain) and 0.8 (snow, 25 sections; 2 and 1 in one each). Add-car-velocity is TRUE in all 27.
- Intensity: `M9a20` adds `+0x30 * +0x38` to `+0x2c` once `+0x34` passes `+0x38` (300 s), then clears `+0x30`
  (**loader**). `E23SndParam::Update` sets `+0x30` to 0, so the amount stays at its target (50 or 100) (**inferred** from
  the constants). The emitters and the 3D update read `M9b80` (`E83System::Update`); the 2D rain draw reads `M9e60`
  (section 7).

## 7. Windscreen effects and wipers (2D, presentation)

- **Rain and snow layers on the glass.** The 2D renderer `NParticleSystem2D::CPC_ParticleRenderer::Tick`
  (`0x452020`) calls `M9e60` and draws the rain or snow effect set (**loader**). `CParticleEffectRain::vf08`
  (`0x5f25f0`) and `RainMax::vf0c` (`0x5f3200`) also call `M9e60` (**loader**). The effect parameters are in
  `misc/Part2D.ini`: `[Effect Rain]` buffer 150, alpha 60, life 2.0 s, size 0.004; `[Effect Rain Move]` buffer 10,
  emit delay 0.5 s; `[Effect Wiper Top]` buffer 100, life 0.5 s; `[Effect Wiper Trail]` buffer 50.
- **Wiper motion.** `E88Wiper::Tick` (`0x5f8720`, [../src/rbr/RBR_E88_Wiper.cpp](../src/rbr/RBR_E88_Wiper.cpp))
  advances two angle pairs (`+0x78`, `+0x84`) with rates (`+0x90`, `+0x98`) times `dt`, folds at the stops, and
  emits `CSigWiperRotation` on `g_signalManager + 0x2180`. It runs only when the global `DAT_0167e8e8` is non-zero
  (**loader**). That global is the 2D host's (`0x167e8b0`) `+0x38`, which `FUN_005f6790` (`0x5f6790`) sets each
  frame to `M9b80(0) * 0.01`: the wipers run in any rain or snow (**disassembly**). `InitTable` (`0x5f8a50`): blade 0
  turns at 3.4558 rad/s from 0.0942 to 1.4765 rad, blade 1 at 3.2987 from 0.0942 to 1.4137, 0.4 s each way; at blade
  0's stop both fold back by their range and `+0x30` (1 at the start) flips, and while it is 0 the angles shown are
  mirrored (`min + max - angle`): a linear sweep out and back every 0.8 s, with no intermittent mode. The sound of
  the strokes is [sound.md](sound.md) 5.4. `FUN_005f6790` calls `Tick` when the effect's `+0x174` is set
  (**loader**).
- **Car nodes.** `CLM_CharacterCar::OnWiper` (LM, [../src/locomotion/CLM_CharacterCar.cpp](../src/locomotion/CLM_CharacterCar.cpp)
  line 256) receives the rotation on three connections and sets the matrices of `i_wiper_r`, `i_wiper_l`, `e_wiper_r`,
  `e_wiper_l` (LM `Unk2d0..Unk2dc`, looked up at car setup). Each car `.ini` gives these four nodes a material, shader,
  `NotUsed` predicate and switch box, nothing else (**file**, all eight cars).
- **Unused windscreen signals.** No `Emit` for `CSigWiperPositions` or `CSigWindscreenData` exists in the 1.02 exe
  (**loader**: only the connection, list and signal constructors appear). The `CWiperData` and windscreen
  connections (`0x5f8920`, `0x5f89b0`, `0x5f7830`, `0x5f7b10`) are never driven (**inferred**).
- The modern windscreen replacement is planned in [particles.md](particles.md) ("In-car glass effects").

## 8. Sky, clouds, fog and sun (HedgeHog3D, presentation)

Keys read by HH from `Maps/TrackSettings.ini` (**file**: 45 distinct keys; 387 of the 405 sections carry the full
set): `SunDir`, `Sun_Intensity` (value 98 in 228 sections), `Ambient*`, `Diffuse*`, `Car_*`, `Character_Lighting`,
`Particle_Lighting`, `Terrain_Reflectance`, `Specular_*`, `Inscattering`, `Extinction`, `Rayleigh_Multiplier`,
`Mie_Multiplier`, `Turbidity`, `UseFog`, `FogStart`, `FogEnd`, `FogRed/Green/Blue`, `SuperbowlFogStart/End`,
`CloudName`, `Cloud_Scale`, `SkyboxName`, `Skybox_Scale`, `MipMapBias`. The key `MipMap_Bias` appears in 192 sections
and the misspelling `MipMip_Bias` in 68.

- **Cloud face.** `CloudName` is one of `Us_clear`, `Us_clear02`, `Us_clouded`, `Us_clouded02`, `Us_partly_clouded`,
  `Us_partly_clouded02`, `Us_overcast`, `Us_overcast02`, `Us_precipitation`, `Us_precipitation02_pc` (file counts:
  `Us_overcast02` 107, `Us_precipitation02_pc` 91, `Us_clouded02` 61, `Us_clear02` 44, `Us_partly_clouded02` 25,
  `Us_clear` 18, empty 18). These names match the face names in `Clouds.lbs` (shaders.md).
- **Skybox.** `SkyboxName` is `Evening_clear` (239 sections), `Morning_clear` (148) or empty (18), matching the
  `skyboxes.lbs` chunks `EVENING_CLEAR`, `MORNING_CLEAR` (shaders.md).
- **Cloud fog.** `SkyboxSaturation` is the clouds' fog visibility under `UseFog` (`CloudsSingleTexture.vso`
  `oFog = c49.w`; shaders.md, Clouds and sky items): 0.12 in 27 of 38 `heavyrain` sections, 0.03 or 0 in `heavysnow`,
  0.33-0.51 in the unfogged ones. `Superbowl_Scale` and `Cloud_Scale` are overwritten by the `CloudName` copy in
  `LoadSkySettings`.
- **Fog.** `UseFog = true` in 204 of 387 sections. Every heavy-fog section has `FogEnd` 300; the crisp sections range
  over 200-7,100 (**file**; the renderer, not the game's physics, uses it).
- **Terrain reflectance.** `Terrain_Reflectance` is nearly the same for every preset (mean 0.09-0.11, crisp 0.09,
  heavy rain 0.096; **file**). It does not mark a wet road.
- **No lightning or thunder.** See the summary.

## 9. Retail data survey (weather presets)

- `TrackSettings.ini`: 405 sections. Preset counts (suffix after the track letter): `crisp_partcloud` 42,
  `crisp_clear` 41, `norain_lightcloud` 14 + 14 (M, N) and 11 (E), `heavyrain_heavycloud` 38 (over all letters),
  `hazy_partcloud` 23, `hazy_clear` 36 (**file**). Sections for track 68 (one) and track 5 (one, `5O_N_norain_lightcloud`)
  match the `%u` name rule. Track 4 has 43 sections named `04...` (section 2, Open).
- `Tracks.ini`: 68 `[Map##]` sections; 65 name one of six particle files; maps 4, 5 and 92 have none (**file**).
- `misc/track_w.ini`: 27 sections for maps 4, 10-14, 20-23, 31-34, 41-44, 51-54, 61-64 and 94. Other maps use the
  defaults in `E21_WeatherA` (**file**).
- `.mat`: 86 files, 9 tables each; the six empty files are the USA 2002 set (stage-files.md).

## Open

- **Track 4 names.** The game builds `4E_...` (`%u` of map 4). The file's 43 sections for track 4 start with `04`.
  Track 5 is `5O_N_...`, so the rule is `%u` and track 4 probably falls back to defaults. Settle at runtime: which
  TrackSettings section HH loads for map 4.
- **Season block.** Which code fills the `E80Daemon` stage block (weather, cloud, wetness, wear, letter) from the
  season data, and what app-state values 2, 3 and 5 are. Also whether the table row pick (`FUN_005b53b0`) feeds the
  season block or only sound.
- **`DAT_0167e8e8`.** Resolved: the 2D host's `+0x38`, set each frame by `0x5f6790` (section 7).
- **Wiper constants.** Resolved: `InitTable` (`0x5f8a50`, read from the binary's pool at `0x74f260..0x74f288`): `+0x90`
  3.4558 and `+0x98` 3.2987 rad/s, stops `+0x80`/`+0x8c` 0.0942 and `+0x7c` 1.4765, `+0x88` 1.4137. Blade 0 turns
  `i_wiper_l`/`e_wiper_l` and blade 1 `i_wiper_r`/`e_wiper_r` (LM `0x1001cb20`), each by `RotationY` of its angle from
  0.0942. The binary puts 0.25 (`0x730888`) at `+0x1c` and 1.4765 (`0x74f270`) at `+0x7c`; our C++ had them swapped (fixed).
- **`nice_w` / `bad_w` consumers.** Resolved: `InitA`/`InitB` (section 3, Quick race).
- **`Noon_*` sections.** Three sections (`Noon_Clear`, `Noon_Light`, `Noon_Heavy`) have no track number; no name builder
  produces them. Possibly a frontend or superbowl use.
- **Headlight emitters.** The calls in `E21_WeatherA` name emitters that no retail file defines. Confirm at runtime that
  nothing is emitted.
- **Wet `.mat` cells.** How the wet and damp tables differ from dry per cell on a stage with wet variants (only a
  rough byte comparison was done). `rbr_formats::mat` would settle it.
- **Tyre wetness.** Resolved (section 5, Tyres).
- **`DAT_01660848` values 0 and 2.** Resolved (section 3, Quick race).
- **Bad weather's wear after an earlier race.** `InitB` keeps the wear, so a bad-weather race after a random one
  inherits its wear. The standalone takes it as `--wear` (default new, a fresh game's).
- **`E74_WeatherKind` (`0x5b5ab0`).** Its surface/wetness branches are ported as the pure
  `rbr_gameplay::weather::tyre_kind`: gravel 3-5, tarmac 0-2 by wetness, snow 6. The launcher uses it for its default
  tyre; `weather::tests::e74_weather_kind_maps_every_surface_and_wetness_to_its_tyre` checks all nine cases against
  the asm.

## Relevance

Gameplay: wetness and wear pick the `.mat` table, whose surface ids select physics surfaces with lower friction when
wet, and the puddle collision meshes load only when wet. Presentation: sky, cloud and skybox faces, fog, sun,
precipitation particles, windscreen effects and wipers; nothing read here writes physics state directly.

## Other systems seen

- Sound: `E23SndParam::Update` (`0x4aab40`), `E23SndParam::LoadWeather` (`0x4aa970`), `E30Rain::Vf08` (`0x4d17e0`, rain sound
  gated on `M9b80` and `M9e60`), `CCarSound::Md490` (`0x4ad490`).
- Season tables: `FUN_005b53b0` (`0x5b53b0`) and the 0x13f-row table at `0x7d34c8` (E74).
- Stage weather frontend: `E67_BuildWeather` (`0x590440`), `E67_BuildWeatherSel` (`0x58f840`).
- Ghost save: `CGhostCarManager::Save` (`0x492eb0`).
- Collision and material load: `CPC_TrackLoaderDaemon::LoadCollisionFile` (`0x46a460`), `FUN_004fdff0` (`0x4fdff0`),
  `FUN_004fb220` (`0x4fb220`, puddle mesh list).
- Stage-start weather events: `CPC_TrackState::LoadWeather` (`0x4675e0`), `E09_LoadWeather` (`0x46b160`).
- Wind particle motion: `FUN_005d8b00` (`0x5d8b00`, emitter kind steps), `FUN_005daa90` (`0x5daa90`, rain init).
