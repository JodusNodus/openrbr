# Tyre marks, world sound triggers and the ghost car

How the 1.02 game draws tyre (skid) marks, which sounds come from world data
(`.dls` sphere and track tables, the ambient loop, the siren), and how the ghost
car is recorded, stored, played back and drawn. Addresses are
`RichardBurnsRally_SSE.exe` 1.02 unless marked HH (HedgeHog3D) or LM
(LocoMotion). Labels: **loader** = read from the decompile or our reconstruction
of the named function; **file** = checked on retail data; **inferred** = plausible,
not proven. Particle and spray context is in `particles.md`; stage markers and
animation triggers are in `formats/stage-files.md` and `stage-characters.md`.

## Summary

- **Skid marks** are not particles. Each physics step, `EmitTire` (`0x5e3050`)
  reads the wheel's contact patch from `CSigTireParticles`, and lays a
  segment of a ribbon in one of four rings (one per wheel). The surface's
  skid record in the rally's `PS_*.ini` decides colour, texture, size, and
  alpha. Only the full-quality particle file has records; the `_low` files and
  18 of 65 stage particle entries never produce marks.
- **Ambient sound** comes from `audio.dat` (`Audio\Ambient\europe.ini`): a looped
  `wind.wav` bed and seven bird clips. The bird branch looks dead in 1.02 (its
  timer is seeded with a float bit pattern; see 2.2, inferred).
- **`.dls` table 6** holds 627 sound spheres (57 stages). They do not drive birds.
  They gate one sound, `audio\ambient\siren.wav`: when the car is above 70 km/h
  and the active camera changes to a point inside a sphere (`0x4d5d00`).
- **`.dls` table 4** holds 82 track-emitter rows (6 stages). They bind particle
  emitters to stage dummies. They are presentation (particles).
- **The ghost car** is a LocoMotion character (`Mr. Ghost Car`) with the
  chosen car's model and `ghost_*` parts. It plays back a recorded pose stream
  from `Ghosts\rb_<map>.gho`. Recording reads the car's pose; nothing is written
  back to physics.

Relevance: all three parts are presentation. Marks and the ghost read physics
state (contact, pose) and never write to it. Sound triggers read car speed,
camera position and the replay bank state. None of this is in the physics step.

## 1. Tyre (skid) marks

### 1.1 Data (`Maps/PS_<Rally>.ini`, section "Skidmarks", **file**)

Section 8 of each particle file (see `particles.md`) is a count and then records
of 12 lines each (**file**, checked on all 12 full files):

| line | field | stored at (record, `E83System` loader) |
|---|---|---|
| 1 | material id (surface id) | `+0x00` |
| 2 | texture index into `skidmarks.dds` | `+0x04`; read as `e[1]` |
| 3 | RGB | `+0x10..0x18` |
| 4 | size modifier | `+0x20` (`e[8]`) |
| 5 | flush threshold | `+0x24` (`e[9]`) |
| 6 | general add | `+0x28` (`e[10]`) |
| 7-8 | rubber reduction, rubber multiplier | `+0x2c`, `+0x30` |
| 9-10 | depth reduction, depth multiplier | `+0x34`, `+0x38` |
| 11-12 | dirt reduction, dirt multiplier | `+0x3c`, `+0x40` |

Records are 0x50 bytes (`src/rbr/RBR_E83_D.cpp:449-470`, loader). The
skidmark aliases (`from to` pairs, after the material aliases) fill a
256-byte map at system `+0x518`. Material aliases fill `+0x418`
(**loader**, `RBR_E83_D.cpp:474-493`).

Counts per full file (**file**): Arctic 1, Australia 2, British 12, Japan 5,
MontBlanc 6, USA 7 records. Materials used: 17, 18, 19, 22, 37, 38, 39, 105,
106, 107, 177, 178, 179 and a few more; each rally uses its own subset. The
British file has 86 aliases, most of them mapping onto the handled ids.

The `_low` files (all six) have `0` skidmark records (**file**). So a stage
that loads `_low` has no tyre marks at all.

Which particle file a stage loads (`RBR_E09_TrackHead.cpp:436`, see
`particles.md`). Using the `[MapNN]` number as the map index (inferred):
- maps `< 0x48` (72), 74, 79 and 80: the full file when quality is not `low`,
  `_low` otherwise;
- all other maps: always `_low`. These are 18 of the 65 entries that have a
  `Particles` key (72, 73, 75-78, 81-89, 93, 94, 98) (**file** from
  `Maps/Tracks.ini` and `RBR_E09_TrackHead.cpp:436-455`, **loader**).

So full-quality skid marks exist on 47 stage entries, and only with quality not
set to `low`.

### 1.2 Emission (`CSigTireParticles` -> `E83System::EmitTire`, `0x5e3050`)

`E51MultiTire::Emit` (`0x53d0d0`, physics) sends one `CSigTireParticles` per
wheel per physics step. The layout is in `particles.md`. The skid path reads
(**loader**, `src/rbr/RBR_E83_B.cpp:237-318`):

1. Wheel index `s[0]` (0-3). A per-wheel stamp at `+0x84` (step counter at
   `+0x80`) gives `fresh`, which only the spray path uses. The skid path does
   not depend on it.
2. Material: `E36Triple::MinPick` at payload `+0xa0` gives the surface id. The
   material alias (`+0x418`) feeds the particle handlers. The skidmark alias
   (`+0x518`) selects the skid record: the record with that id in the table at
   `+0x414` (count at `+0x410`, 0x50-byte records). No record means no mark.
3. Contact: payload `+0xac` must be non-zero.
4. Intensity `k`, the mark's alpha (**loader**, decompile `0x5e3050`):

   `k = general add + (rubber + rubber red) * rubber mult + (depth + depth red) * depth mult + (dirt + dirt red) * dirt mult`

   with rubber = payload `+0x80`, depth = `+0x90`, dirt = `+0x94`. The mark
   needs `k > 0`. It is clamped to 1.0 and stored as the colour alpha.
5. Accumulator. Per wheel, `acc += E83System +0x94` (the last `Update` frame
   time, `RBR_E83_B.cpp:66`), so `acc` adds frame time per call, not distance.
   - If `acc <= flush threshold` and the wheel's newest mark is still open,
     the mark is extended to the new edge pair (`M5e9010`).
   - Otherwise `acc -= threshold` and a new mark starts (`M5e9120`) with the
     record's RGB and the texture index.

   So the cadence is in frame time: each `CSigTireParticles` payload adds the
   frame's dt. In 1.02 a rendered frame sends one payload per wheel, so the
   sum runs in real time. `rbr-render` adds `GAME_FRAME` (1/60 s, ours) per
   payload; a live answer is one frame under 1.02 and under NGP, where the
   standalone sends each wheel's last payload of the frame (fx-1).
6. Segment geometry: two edge points from the contact patch (payload
   `+0x10` and `+0x20`, the midpoint, scaled about the midpoint by the size
   modifier), lifted 0.03 along z (**loader**, the `0.03f` lift in
   `RBR_E83_B.cpp`; the decompile uses constant `0x739958`). The mark is laid between them.

`Update` (`RBR_E83_B.cpp:~220`) closes a wheel's open mark (`M5e8fe0`, sets
`+0x30 = 1`) on any frame where the wheel did not reach step 5 (no contact,
`k <= 0`, or no record). That is the break in a skid (**loader**).

### 1.3 Storage: four rings (`0x7d6d48`, `E84_Ring.cpp`)

- Four rings, one per wheel index, each `0x4010` bytes: 256 marks of `0x40`
  bytes, then head and tail pointers at `+0x4000` / `+0x4004`
  (**loader**, `E83TrackRing::M5e9340` `0x5e9340`).
- Mark layout (`0x40` bytes; **loader**, `M5e9010`/`M5e9120`):
  - `+0x00` edge point A (vec4), `+0x10` edge point B (vec4);
  - `+0x20` colour RGBA (from the record RGB and the `k` alpha);
  - `+0x2c` / `+0x30` open-or-closed flag (`0` open, `1` closed);
  - `+0x34` texture index; `+0x38` age ramp (see 1.4); `+0x3c` texture v
    coordinate, set from the segment length over the previous one.
- When the ring is full the oldest mark is overwritten and the tail advances
  (`M5e9120`, `RBR_E84_Ring.cpp:126-148`).

### 1.4 Fade and draw

- **Fade is by mark count, not time.** After each new mark, `M5e9120` writes
  `(i+1)/32` into `+0x38` of the 32 marks starting at the tail, oldest first
  (`0x5e9120`, decompile loop: stride `0x10` floats = one mark per step;
  **loader**). So the oldest marks are faintest. Marks outside that window keep
  the last value (`1.0` on creation). The reconstruction's loop steps 4 floats
  instead, which is wrong (see Open).
- **Draw**: `CPC_ParticleRenderer::Vf08` (`0x5ed100`, E85 ribbon streamer)
  walks the four rings from tail to write pointer, emits a ribbon per run of
  the same texture index, and uses `Textures\skidmarks.dds` (**loader**,
  `RBR_E85_Draw.cpp:654-720`). Vertex alpha is `ramp * colour alpha`.
- `skidmarks.dds` is 256 x 64 (DXT3) with eight 32-texel vertical strips;
  the record's texture index (0 to 7) picks a strip, and the strip runs along
  the mark through `+0x3c`: `prev + 2 * distance / width` (`M5e9120`,
  `M5e9010`). `0x5dbe20` is only the ring index (four rings of `0x4010`), not
  an atlas row (fx-1).

### 1.5 Surface dependence

Yes, through the material id. A surface without a skid record (or whose alias
maps to one) leaves no mark. The record sets colour, size, flush threshold and
how strongly rubber, depth and dirt raise `k`. Wet and damp tarmac records mostly
set `k` from the constant "general add" (0 to 0.2 across retail) plus the
dirt term. Dirt reduction is -0.5 on the tarmac records named DAMP and WET in the file
comments (**file**). Surface
classes of the stage (`.col` surface ids) are in `col.md`.

### 1.6 Gameplay vs presentation

Presentation. The marks read the physics contact state. Nothing in
`EmitTire` writes to the physics step, and the Rust pure step has no listener for
`CSigTireParticles` for skid marks (`particles.md`).

## 2. World sound triggers

### 2.1 Data

- `Root/audio.dat` (CHELALIC archive, **file**): `Audio/Ambient/europe.ini`,
  `Audio/Ambient/wind.wav`, `Europe/bird01..bird06.wav`, `Europe/crow.wav`,
  `Ambient/siren.wav`, `Ambient/crowd.wav`, `Ambient/heli*.wav`,
  `Ambient/Prize/0[1-4].wav` and the audio INIs.
- `europe.ini` `[AMBIENTFX]` (**file**): `wind = Audio\Ambient\wind`;
  `bird1..bird7` = `Europe\bird01..06` and `crow`. There is one such ini; the
  load path always uses `europe.ini` (`CAmbientSounds::Vf04`, `0x4ab5d0`), so
  birds do not vary by stage.
- `.dls` table 6 (sound emitters, `count, Vec4 {x, y, z, radius}` rows): 57 of
  86 stages have spheres; 627 spheres in total; max 21 per stage; radii 100 m
  (614), 120 m (11), 125 m (2) (**file**, `Maps/*.dls`).
- `.dls` table 4 (track emitters, `{name key, trigger id, activation distance^2}`
  rows): 6 stages (`track-10_O`, `track-14_O`, `track-41_M`, `track-41_O`,
  `track-46_E`, `track-46_O`), 82 rows. Squared distances 10000 (100 m, 10 rows)
  and 90000 (300 m, 72 rows) (**file**).

### 2.2 Ambient bed and birds (`CAmbientSounds`, `0x4ab530` / `0x4aaf50`)

Start and stop (**loader**). The app's `case 5` block (`0x4825b0`, the race
start in `RBR_E14_App.cpp:1248-1261`) emits `CSigAmbientSnd` twice. The first
payload is float `0.0` (owner 0: `Vf40` stops the wind, `LoadBirds` `0x4aae10`
reads `AMBIENTFX`, `wind` and `bird1..bird7` and loads them; the timer and
the enable flag are cleared). The second is bit pattern `1` (owner 1: timer
= `0x40a00000`, enable = 1). No other emit site exists in the exe. Owner 2
(disable) is therefore never sent (**loader**; the only emits are at
`0x4825b0`).

Per frame, `CAmbientSounds::Vf08` (`0x4aaf50`; **loader**, decompile only, the
reconstruction does not contain this function) runs when:
- the frame time is `<= 1.0` (`0x795e14`);
- app state is 8 (a replay; the race is state 1, see Wind below);
- enable flag is set;
- `FUN_00565470() != 7` (not named; `0x565470` returns a host field).

It then does:
- *Distance*: `d` = listener to car distance. The listener is the audio
  backend (`g_8d5338[0x58]`, vtable `+0x5c`); the car is `g_8d5338[0x4171]`
  (`+0x105c4`) at `+0x41880`. The ambient volume is `clamp(d * 0.01, 0, 1)` for
  `d < 100`, and `1` otherwise, capped at `g_8d5338[0x41bc]`.
- *Wind* (read in the decompile, 2026-10-17): `Vf48(a, b)` drives the looping wind sound (`m_wind`, loop flag
  via vtable `+0x8c`, placed at `(0, -1, 0)` with create flags `0x418`): volume `g[0x41ce]` (the `Wind` amp) `*
  g_1660e10 * b`, pitch `a * 0.25` (`0x730888`); `a == 0` stops it. Vf08 calls it only when `M9c90` (the cloud id,
  0-3, never -1) is not -1:
  - camera in the car, no replay: `a = |car velocity +0x41890| * 0.1`, `b = clamp(a, 0.1, 1)`;
  - camera outside, replay: `a = cloud + 1`, `b` = the ambient volume above;
  - camera outside, no replay: `Vf48(0, 0)`, the wind stops;
  - camera in the car, replay: no call.
  But Vf08 returns unless the app state is 8 (a replay), and the race stays in app state 1 (harness state log,
  2026-10-19; the captured original has no wind in the car). So the wind plays only in replays, never while
  driving; the in-car branch is for state 8 without `E19IsState2`.
- *Birds*: `Vf44` plays the next bird (sequential over `m_birds[0..6]`,
  `m_next` wraps at 7). It sets min distance 5, max 30 (`0x40a00000`,
  `0x41f00000`), volume `g[0x41cd] * g_1660e10 * v`, where `v` is the distance
  ramp, and only if `d > 50` (`DAT_007304a0` = 50.0). The timer runs and the
  bird plays only in a replay (`E19IsState2`). `rand()` is called three
  times and the result is discarded (**loader**). The bird is placed at the
  listener's position: the mixer's `+0x64` fills a stack vector that `+0x80`
  then takes (**inferred** from the decompile's stack offsets).

**Bird timer is not reached in 1.02 (inferred from arithmetic).** The timer
`m_unk70` is an `int` (decompile `0x4ab530`, `0x4ab5d0`). It is seeded with
`DAT_0072ae44`, which holds the bit pattern of `5.0f` read as an int:
`0x40a00000` = 1,084,227,584. Each frame Vf08 computes
`(float)timer - dt`, stores it back as an int, and fires only when the float is
`< 0`. At `2^30`, floats step by 128, so subtracting `dt <= 1` leaves the value
unchanged and the test never passes. Only `Vf40` (timer = 0) resets it, and
that runs before owner 1 in the same block. So under this reading no bird
plays during a race. Once the timer does fire (if it ever did), the next timer
is `int(rand() & 3) + 2`, that is 2-5 units, subject to the int truncation
(**loader** for the expression). This needs a runtime check (see Open); the
stage-files note says birds play above 70 km/h inside a sphere, which the code
does not support (see 2.3).

### 2.3 Siren (table 6 spheres, `0x4d5d00`)

`E31_5d00` is the owner of the table-6 spheres. It is called every frame
(`src/rbr/RBR_E31_Effects.cpp:166-190`, decompile `0x4d5d00`). Logic:

1. App state is 8 (driving). The active camera id is `app +1000`
   (`CApplication::GetUnk70`; **inferred**). The code runs only when this id
   changes from the last one stored at self `+0x50`.
2. If the played flag at self `+0x54` is set, or the car speed is `<= 70`, the
   flag is cleared and the function returns. The speed is `car +0x4192c`
   (km/h; **loader**, `RBR_E23_BigCtor.cpp:789`, `|v| * 3.6`).
3. The camera class test (`E12_Classify`, `0x474ee0`) must return 0, 1 or 2.
   The function only returns 0, -14 or -16 (tags of chase and driver views,
   `stage-files.md` table 3), so this means "not a car-tracking camera"
   (**loader**; **inferred** meaning).
4. The camera position (`E12_SplineCtrl::CopyTo`, `0x474f00`) must lie inside a
   sphere of the table-6 set (`E12_Spheres::Hit`, `0x477310`: squared distance
   below the squared radius). The set is `g_7eabb0`, filled from `.dls` table 6
   (`RBR_E12_Load020.cpp:838-912`, **loader**). Table 6 has no other reader in
   the exe (**loader**; only caller of `0x477310`).
5. On a hit: `E31_5bf0` (`0x4d5bf0`) creates a sound object for
   `audio\ambient\siren.wav` (vtable `+0x50`), sets loop (`+0x8c`), min
   distance 1.0, max 30.0, volume `g[0x41cb] * g_1660e10`, and stores it at
   self `+0x3c`. Then `S50(0)` plays it, and the played flag is set.

So table 6 spheres produce the siren, not birds. The siren plays on a camera
change into a sphere, at speed. Repeated camera changes inside the sphere do
not replay it immediately (the flag is cleared on the next change).

### 2.4 Track emitters (table 4, `E83PartHost::M5e96f0`, `0x5e96f0`)

Table 4 rows are read by `E12` and passed to `E83PartHost::M5e96f0`
(`src/rbr/RBR_E84_Host.cpp:220`, **loader**). Each row clones the named
emitter from the particle file, binds it to the animation dummy with the same
name (`BSearch50`), and stores the trigger id and the squared distance
(`+0x200`). Per frame (`M5e9490` 0x5e9490, fx-2) a row emits at its file rate while
its switch (`+0x204`) is on and the camera is nearer than the distance, else at
rate 0. The switch is never on in 1.02 (`notes/particles.md`, Track emitters).
Particle emitter; presentation (see `particles.md`).

### 2.5 Not found in 1.02

- **Crowd cheering near spectators:** no exe string, Lua call, or ini key
  references `crowd.wav` (`Ambient/crowd.wav` is in `audio.dat` only; string,
  decompile and `Scripts.rbz` searches, **file**). The Lua scripts make no
  sound calls.
- **Water or rain ambience:** no key. `water_temp.ogg` and "caution water" are
  car warnings (`Cars/Effects.ini`, `CoDriver.ini`).
- **Table 6 spheres as bird emitters:** not in the code (2.3).

### 2.6 Gameplay vs presentation

Presentation. All of this is audio. The triggers read car speed, camera
position and the replay state, and write nothing back to physics.

## 3. The ghost car

### 3.1 Car model (`E09_LoadGhostCar`, `0x466910`)

Called from the track loader (`RBR_E10_Daemon.cpp:307`, after the player's car
loads; **loader**). Steps (decompile, **loader**):

1. `Car%.2d` section in `Cars\Cars.ini`: keys `FileName`, `IniFile`,
   `ShaderFile`, `TexturePath`, `ShaderSettings`.
2. The shader settings name gets a weather suffix (`_clear` for weather 0-1,
   `_overcast` for 2-3) and `_low`.
3. The ghost character is registered on the LM side: `AddCharacter` (name
   `Car`), `AddAnimation` (`Mr. Car%.2d_Anim`), `Activate`. The loadout strings
   are `fileName`, `iniFile`, `shaderFile`, the `_low` shader settings,
   `texPath`, `fileName`, `"gravel"`, and `AppLoader()->LoadGhost(2)` runs.
4. `g_gfxDaemon->OrFlags(2)`.

The reconstruction (`RBR_E09_TrackHead.cpp:273`) has the three success flags
fixed to 0 and so always returns 0 without loading. The decompile does the
ini checks through `FUN_0041b850` (not traced). Treat this function as
partial.

Car data: `Cars/GhostCarSettings.ini` (`Root/...`, **file**): `[Settings]`
`Ambient = 0.5 0.5 0.7`, `FadeDistance = 100`, `MaxOpacity = 0.7`. It is read
by HH `CShaderGhostCar::Load` (`src/hedgehog3d/ShaderCar.cpp:1688`, HH
`0x1007ecb0`).

### 3.2 Recording (`CGhostCarManager`, `0x4926b0`-`0x493240`)

- Input: `E18GhostState`, the car pose (position `+0x10`, quaternion `+0x20`),
  delivered by `CSigCarGeneralState` through `E18_ConnGhostGeneral`
  (`RBR_E18_Graph.h`; **loader**).
- `CGhostCarManager::Update` (`0x493240`) writes one sample when the frame
  counter `m_cntA` (`+0x96084`, incremented by `Accumulate` 0x4939f0 once per
  frame in state 1, from `CApplication::M004825b0`) is odd and differs from the
  last sampled count `m_cntB` (decompile; the C++ in `RBR_E18_Ghost.cpp` has the
  test inverted, `m_cntB != cnt`). So a sample every second frame. A key (type
  2) when `m_playback` is set (`Start`, and the `CSigRespawn` handler
  `SetPlayback` 0x492e90), otherwise a delta (type 1). Before a delta that moved
  more than about 1.95 m (`1 - (int)(max|d| * -0.5128205)` parts) it writes
  in-between deltas lerped and slerped from the last sample's pose, stamped
  `f * m_accum + (t - m_accum)`. The keys' eighth byte is stack garbage
  (`Record1` copies 8 bytes from a 7-byte record). Clock: `m_wtime`, the race clock (`GetWTime` `0x492720`),
  stored as `t * 1024` in 24 bits (1/1024 s units).
- Packets (**loader**, `src/rbr/RBR_E18_GhostGS.cpp:31-104`):
  - Type 2 keyframe (20 bytes): position as three floats; 3 bytes time; four
    signed bytes quaternion x, y, z, w (`* 127.5`).
  - Type 1 delta (10 bytes): 3 bytes time; three signed bytes position delta in
    1/64 m (range about +/-2 m); 4 bytes quaternion.
- Buffers: two `0x4b000`-byte buffers in the manager (`+0x5c`). Writing stops
  at the buffer end (`0x4afe8`).

### 3.3 File (`Ghosts\rb_%d.gho`, `E17_LoadGhostFile` `0x491d90`)

- Shipped in `Root/ghosts.rbz` (**file**, 32 entries): `rb_13`, `rb_22`,
  `rb_33`, `rb_41`, `rb_53`, `rb_62`, `rb_71` (seven stage ghosts), 13
  `rse*.gho` rally-school ghosts (0, 1, 2, 3, 5, 6, 7, 8, 9, 11, 12, 13, 15), `rse*.arp` (arp, not traced), `demo1.rpl`.
- Header (**file** and **loader**, `E17_ParseGhost`, `0x491cc0`): magic bytes
  `SOHGRACT` (two dwords `0x47484f53`, `0x54434152`, i.e. "GHOSTCAR" with each
  dword byte-reversed), then word 2, the map (`rb_N` holds `N`), then word 3
  (version in the high 16 bits = 2; the car's `Cars.ini` index in the low 16:
  5, the Impreza 2003, in every file but `rse0`, 4, the 206; `WriteBuf` stores
  both into the slot `Start` fills from `m_unk730`/`m_unk734`, the track and
  car), then the sample byte count, 12 bytes of `0xff` (skipped) and the
  samples (`CGhostCarManager::WriteBuf`, `0x492dc0`). `rb_41` and `rb_71` carry
  45 and 28 more deltas past the count, which the loader does not copy;
  `rse0` and `rse13` end in 20 bytes the playback reads as header words of no
  kind (the debug heap's `fd`/`ab`/`ee` fill, then a zero word), so it stops there.
  Rust: `rbr_formats::ghost` (every retail ghost written back byte for byte).
- Name: `DAT_01660804` is the map id set when the race starts
  (`E95_GameStub.cpp:138`; **inferred** that `rb_%d` takes this id).
- User-saved ghosts go to `./GhostCars/<name>_<map:02>.ghc` (save-file kind 5,
  `CPC_SaveFileManager` `0x452870`/`0x452a80`; **loader**), not encrypted.
  `CGhostCarManager::Save` (0x492eb0) writes 14 words: the checksum
  (`Crc32::Run`, the game's table, see `notes/formats/saves.md`) over the rest
  from word 2, the length less 8, `TSHG`, map, car, the weather getters
  `M9d50`, `M9c90`, `M9360`, `M9cf0`, `Vf18`, `Vf1c`, the float `M9b80(0)`,
  `CWeatherDaemon +0x3c`, the stream's byte count; then the stream, which ends
  in the zero header word `BeginWrite` wrote. `+0x3c` is a float (`M9230` stores `(float)v`); only the replay's
  weather (`E19ApplyTweak`) and the RB challenge set it, so a menu quick race saves the constructor's 0.0; every
  harness snapshot (`StartGame` races, maps 54 and 13) has 15.0.
- **Golden `.ghc`** (ghosts-2): `Save` itself builds the file in memory (the file manager writes it), so it was run in
  the emulator (`harness/replay.py`'s `Machine`, `m_idx` flipped to read the live recording) on
  `vcu-cov.snap-11000.bin`: `recordings/golden-tanner-corolla-11000.ghc`, 58,154 bytes. `SavedGhost` reads it (the
  checksum holds) and writes it back byte for byte; words: map 54, car 6, letter 2, cloud 0, weather 0, cloud letter 2,
  wear 0, wetness 1, amount 0.0, `+0x3c` 15.0 (`rbr-render/tests/playback_data.rs`). Its clock ran 9.5 s for 76 s of
  driving (the harness runs frames faster than their `dt`), so only the points are compared.
- **The recorded point is the model's root.** `CCar::Vf28x` (0x4ea020) sends `CSigCarGeneralState` with
  `Xform8130(0)`: the world point of the model origin, which is the body position plus the rotated
  `-vecCenterOfGravity` (`Physics/t_coroll/common.lsp` 0 -1.37 0.22; the emulator gives (0, 1.3700, -0.2200) in the
  body frame at steps 2000 and 11000). The golden file's points lie within 5 cm of that point of the drive for 98% of
  its 5,805 samples (the rest are in-between samples on a chord, up to 0.56 m).

### 3.4 Start, select and playback

- At race load, `CGhostCarManager::BeginWrite` (`RBR_E14_Init.cpp:295`) starts a
  recording. `Start` (`0x492b60`) runs when the stage has no ghost to race
  against; `Select(0)` (`0x492c20`) and `SetFlag(1)` (`0x492ae0`) run when the
  stored ghost for this stage is current (`IsCurrent`) (**loader**,
  `RBR_E14_Init.cpp:420-436`; the meaning of `Select`/`Start` as "record" vs
  "play" is **inferred**).
- `CGhostCarManager::Playback(dt, t)` (`0x493650`) interpolates the stored pose
  at race time `t`: position by `E11_VecLerp`, quaternion by `E18_QuatSlerp`,
  then emits `CSigGhostCarState` on the bus at `g_signalManager + 0x7da0`
  (`RBR_E18_GhostGS.cpp:240-246`; **loader**).
- `CSigGhostCarState` payload: object index, position, rotation matrix.
  LocoMotion's `CLM_ControllerManager::OnGhostCarState` (LM
  `0x100377b0`, `src/locomotion/CLM_ControllerManager.cpp:66`) sets the local
  transform of that object and its state (**loader**, LM). Connections are at
  slots 0 and 7 of `m_sigGhostCarState` (LM `0x7db0`).

- **Which drive becomes the ghost** (`BeginWrite` 0x492ca0, asm): at the race's end the recording ends with the zero
  word and the write slot gets the race time. The caller, `CApplication::M00480810` (the `CSigEndRace` receiver, the
  call at 0x480e20; decompile), passes the HUD race clock (`+0x140`, with the penalties) by the mode query
  `0x565470`: in mode 1 (the quick race) `clear` is 1 when the race ended retired (the end payload's `+0x10`); in
  modes 4, 5 and 8 `clear` is 0 and a retired race passes 1800.0 (`0x72e384`); in any other mode `clear` is 1 (checked
  in the asm: 0x480e3c indexes the byte table 0x480f54 by mode - 1, modes 0 and above 8 take the default). Mode 4 is
  the hot seat stage (the `HOT_STAGE` item's callback `0x58f6e0` sets it), 8 the hot seat rally (`0x590160`, with the
  season daemon reset); 5 has no direct setter (hot seat, out of scope). So the championship (3) never keeps its
  recording, and neither does a retired quick race.
  Without `clear` the slots swap, so the new recording
  becomes the one to race, when its map differs from the stored one's or its time is lower (strictly). The car is not
  compared; `InitQuickRace` races it when `IsCurrent(map)` and the ghost option (`0x1660854`) is on, drawn as the
  player's current car (`0x1660808`). `RBR_E18_Ghost.cpp` has the time test inverted (`wt->t > rt->t`). The game keeps
  this in memory; `./GhostCars/` holds only the ghosts the player saves. Rust: `rbr_gameplay::ghost::replaces`.
- **The race time is never written.** The slot's time (`m_slots[i].t`, which `m_rtime`/`m_wtime` point at) is set
  only by `BeginWrite`. `Save` (0x492eb0) writes the 14 header words of 3.3 and no time; loading a saved ghost
  (`E18_ConnGhostMemory::Dispatch` 0x4930d0, file kind 5) checks the checksum, `TSHG` and the map, then `WriteBuf`
  (0x492dc0) sets the read slot's map and car only, so its time stays whatever the slot held. Our best ghost
  (`--best-ghost`, `Best_<map>.ghc`, ours) persists across runs in that same format, so it has no race time to compare;
  it compares the ghost clock's end (`rbr_gameplay::ghost::end_time`, race time plus the countdown's ~7 s) instead.
- **Pause** (`SetFlag` 0x492ae0): bit 31 of the mode. `InitQuickRace` sets it only in app mode 7 (rally school), with
  the ghost selected; the start-line pace event (0x15, in `FUN_004783b0`) clears it in mode 7. No quick race sets it,
  so the Rust viewer never pauses its ghost. `Accumulate` and `Playback` run only with the mode not negative,
  so the clock, the frame count (and so the sampling) and the playback stand still.
- **`ghost_parent`** is a root node of every retail car's `.sgc` (`InitGhostParent` 0x4926b0 looks the name up in the
  LocoMotion object map). Its world matrix equals the car's `parent` root's, and `ghost_external` sits under it with
  `e_body`'s translation (the eight retail cars). `CLM_ControllerManager::OnGhostCarState` (LM 0x100377b0) sets the
  node's matrix to the signal's rotation and position (y and z swapped), with no offset, so the ghost's model origin is
  the recorded point.

### 3.5 Drawing

- The ghost is a LocoMotion character, so it is posed by LM like any other
  object (3.4). Its meshes are the car's `ghost_*` parts, which the renderer
  skips for the normal car (`part_in_set` in `rust/rbr-render/src/car/parts.rs`; **file**).
- Shader `GhostCar` (HH `CShaderGhostCar`, `src/hedgehog3d/ShaderCar.cpp`):
  transparent (`IsTransparent` returns true), ambient colour from the ini, and
  an alpha term `min(|d|^2 / FadeDistance, MaxOpacity)`, where `d` is the
  offset from the camera (inferred from `m_unk268`) set in
  the vertex constant `c93` (HH `0x1007f000`; **loader** for the expression, its
  use as alpha is **inferred**). Note that `d^2 / 100` reaches 0.7 at about 8.4 m,
  so the ghost fades in from the camera up to that distance.

### 3.6 Rust

`rbr_formats::ghost` (files), `rbr_gameplay::ghost` (`Recorder`, `Player`, pure),
`rbr_render::ghost` (the mesh and look, `viewer --ghost`, `--save-ghost`). The
viewer's race time is the drive's step counter less the 160 settle steps: the
stage peek has state 1 from the next step. Played on map 33 against
`drive-scn-gravel-impreza03-33`, `rb_33` starts on the same line and leaves it
at 7.32 s, after the race clock's 7 s countdown.
The recorder takes the drive's body pose moved to the model root (the player car's model offset, the wheel fit,
within 3 cm of `-vecCenterOfGravity` on the Corolla), and the ghost is drawn at its point with no offset, as LM does.
Ghosts written before this (`--save-ghost`, `Best_<map>.ghc`) hold body points and show 1.4 m ahead of the line.
The launcher's saved ghosts bring their weather into the race (`Dispatch`; `Season::ghost_weather`) and list it.

### 3.7 Physics

The recorder reads the car's pose and does not feed back. Playback feeds only
LocoMotion. The Rust side has no listener for the ghost signal (`rust/` grep
finds only the render skip for `ghost_*` parts). So the ghost does not touch
the physics step (**loader** and **file**).

## Other systems seen

- Rescue helicopter sounds: `Audio\Ambient\heli_g.wav` (`E31_5a30`), `heli_r.wav`
  (replay, `E31_5800`); `snd_chopper.wav` is the rally incident's, never played
  ([sound.md](sound.md) section 13).
- Birdstrike: `Lua_Birdstrike` `0x43aa80`; `birdhit.wav` (`audio.dat`).
- Weather sound: `CWeatherDaemon` `0x4aa050`-`0x4aa1b0`,
  `E23SndParam::LoadWeather` `0x4aa970`.
- Tyre noise sound: `CSigTyreNoise` consumers in `CCarSound` and the FFB
  controller (`0x491520`).
- Replay state bank: `E19State20` `0x499860` sets state 2, used as the race
  gate by the bird and wind branches (`0x499900`).
- `CRainSnd`, `CPrizeCermonySnd` (exe-subsystems).

## Open

- **Bird timer (2.2):** confirm at runtime. Read `CAmbientSounds +0x70` and
  `+0x74` in a race in the vanilla install (`harness/`), or check whether any
  path other than `Vf40` writes that int. If birds never play, the stage
  files and the stage-characters notes must not describe them as a trigger.
- **Siren camera gate (2.3):** the camera-class meaning of `E12_Classify`
  and whether `app +1000` is the camera id.
- **Reconstruction bugs (no edits made here):**
  - `src/rbr/RBR_E31_Effects.cpp` (`E31_5d00`) reads speed at
    `g_8d5338 + 0x1065c`; the decompile reads `car + 0x4192c`, the car pointer
    at `g_8d5338 + 0x105c4`.
  - `src/rbr/RBR_E84_Ring.cpp` (`M5e9120`): the age-ramp loop steps `stamp += 4`
    floats; the decompile steps `0x10` floats (one mark per step).
  - `src/rbr/RBR_E09_TrackHead.cpp:273` (`E09_LoadGhostCar`): success flags are
    fixed to 0.
  - `CAmbientSounds::Vf08` (the per-frame update) is not in
    `src/rbr/RBR_E23_Ambient.cpp`; this note uses the decompile only.
- **`stage-files.md` line 306** says birds play above 70 km/h with the camera
  in a sphere. The code shows the 70 km/h and sphere gate on the siren
  (`E31_5d00`). The birds have no such gate.
- **Skid dash cadence (1.2 step 5):** whether `acc` gets frame time per physics
  step (so cadence depends on frame rate), and what `E83System +0x80` counts.
- **Ring fade for a non-full ring:** the 32-mark ramp then starts at the first
  mark (the tail stays at the ring start until the ring wraps). Confirm in play.
- **Table 4 distance use** (`+0x200` in `M5e9120`-family code) and the
  emitter-to-dummy binding timing.
- **Ghost:** the meaning of `rse*.arp`, the order of the car's `CSigCarGeneralState`
  and `Accumulate` within a frame, what `"gravel"` in the loadout is,
  and whether the alpha in `c93` is alpha or fade.
- **`g_1660e10`** (sound volume scale) is a BSS float; no writer found in the
  reconstruction. Likely 0.01 (**inferred**).
- `FUN_00565470` (returns host field `+0`; state 7 gates birds and ghost
  recording) is not named.

Relevance: presentation only. Tyre marks, ambient sound, the siren and the
ghost car draw or play from physics and car state. They do not write any
physics state (checked for `CSigTireParticles`, `CSigAmbientSnd`,
`CSigGhostCarState`).
