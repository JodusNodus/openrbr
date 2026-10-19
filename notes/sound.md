# Gameplay sound

How the 1.02 game makes its in-race sounds: the engine, transmission and turbo, the tyres, impacts and off-road
rattles, the co-driver's pace notes and damage calls, and the countdown. It also covers what drives each sound, whether
sound feeds back into gameplay, and a plan for a Rust version. Addresses are `RichardBurnsRally_SSE.exe` 1.02. Labels:
**loader** = read from the decompile or our reconstruction of the named function; **file** = checked on retail data
(counts say over which files); **inferred** = plausible, not proven. Data formats: [formats/audio.md](formats/audio.md)
(`audio.dat`, `audio.ini`, every audio ini) and [formats/eng.md](formats/eng.md) (engine banks). The ambient bed,
birds, siren and `.dls` sound spheres: [marks-sound-ghost.md](marks-sound-ghost.md) section 2 (not repeated here).
Pace-note markers: [formats/stage-markers.md](formats/stage-markers.md). Class map: [exe-subsystems.md](exe-subsystems.md)
(Sound).

## Summary

- **Sound engine.** DirectSound 8 (`DSOUND.dll`, imported only as ordinal 11, `DirectSoundCreate8`). Every voice is a
  DirectSound secondary buffer, 3D for positioned sounds, so DirectSound (hardware or its software mixer) does the
  final mix. The exe adds its own sound library, `SOUND::` with the `CDX*` PC backend. It decodes Ogg (libvorbis
  linked in), synthesises the engine, transmission, turbo and rain streams, and has a software Prologic mixer path.
  EAX goes through `CEAXMixer`/`CEAXSound` (**loader**).
- **Thread.** None in 1.02. A 30 ms sound thread exists, but it runs only if the mixer's slot `0x68` returns
  non-zero, and all three mixers return 0. Signal handlers run inside the physics step (they connect to each signal's
  "every state" list); the per-frame update runs on the main thread (**loader**).
- **Engine sound is granular.** One bank (`subaru.eng`, used by all 8 cars) holds a recording per rpm (1700-7000)
  and load (0, 100) with the sample position of every firing. Each firing period, the renderer picks the next firing
  grain (or a random one), resamples it to the target period (2 firings per revolution) and crossfades the two
  nearest rpm banks. The inputs are engine speed and throttle from the physics' `EngineNoise` signal, emitted once per
  physics step (**loader**, **file**).
- **Other car sounds** come from the same per-step signals: gear whine pitch from gearbox and differential speed
  times tooth counts, turbo whistle from turbo speed and boost, blow-off from turbo flow spikes, pops and bangs from
  the engine's misfire output, tyre loops per wheel from the tyre-noise block (surface, slip, contact), off-road
  rattles from speed and wheel flags, ranged impacts from collision power (**loader**).
- **Co-driver.** Pace notes are built into phrases from the stage marker tag, its flag word and the snapped distance
  ("100", "into", "and"), then queued on one speech voice. The countdown "three, two, one" is the co-driver's number
  samples; "go" is `Audio\Game\Go.wav`. Eight damage calls come from car sensor and fire signals (**loader**).
- **Determinism.** No sound code writes physics or gameplay state. Sound calls the CRT `rand()` on the main thread,
  so it moves the shared `rand` stream that some non-physics systems use. The physics uses its own seed (**loader**).

## 1. Data sources

| File | Read by | Controls |
|---|---|---|
| `Root/audio.dat` (CHELALIC, 3,512 entries) | every loader below | all samples and inis ([formats/audio.md](formats/audio.md)) |
| `audio.ini` (patch 1.02, next to the exe) | `E31Master::M4a70` `0x4d4a70` | 28 volume amps, channel count, optional engine EQ |
| `Audio/Ingame.ini` | `E31Master::M48e0` `0x4d48e0` | 15 `SOUNDDEF` volume and distance sets |
| `Audio/Cars/Cars.ini` | `CCarSound::OnX` `0x4b1810` | `.eng` name per car, in-car low-pass, low-shelf filter |
| `Audio/Cars/*.eng` | `CEngine::Vf04` `0x4bc370` | engine grains ([formats/eng.md](formats/eng.md)) |
| `Audio/Cars/Effects.ini` | `CCarSound::Vf04` `0x4b1bc0`, `CTireSnd` | effect banks (pops, bangs, squeaks, dog rings, driveshaft, gear change, wipers, off-road, tyre surfaces) |
| `Audio/Impact/Sound.ini` | `CCollisionSnd` | impact banks |
| `Audio/CoDriver.ini`, `Numbers.ini` (English); `CoOther.ini`, `Numother.ini` (other languages) | `CPaceNoteHandler::M7170` `0x4b7170` | pace-note words and numbers |
| `Audio/Frontend.ini` `[SFX::INGAME]` | `0x4ba690` | `GO` (the `c3`/`c2`/`c1` keys are unused) |
| `Audio/Speech/Broken/<lang>/*.ogg` | `CCarSound::OnSpeech` `0x4ab940` | damage calls |

All WAVs are 16-bit mono PCM. All Ogg files are mono at 8 or 11 kHz except five mono and two stereo files at 44.1 kHz
(**file**, all entries).

## 2. Sound library and backend

### 2.1 Objects

- **Master.** `CRBRMaster` (our `E31Master`, vtable `0x73574c`, 0x10800 bytes) is reached through the global
  `g_8d5338`. It owns the backend mixer (`g_8d5338[0x58]`), the voice manager (`*g_8d5338`, vtable `+0x50` creates a
  voice), the speech voice (`g[0x416c]`, master `+0x105b0`), the pace-note handler (`g[0x416d]`), the car sound
  (`g[0x4171]`), the `SOUNDDEF` table (master `+0x105dc`) and the amps (master `+0x10708`). `g[0x41bb]` is the
  in-car camera flag that switches inside and outside sound (**loader**).
- **Backend.** `E30DXMixer::Vf04` (`0x4cfcb0`) calls `DirectSoundCreate8`, sets the cooperative level to priority
  (2), reads the caps (logged by `DumpCaps` `0x4cf3f0`, errors to `sounderror.txt`), creates the primary buffer and
  queries a 3D listener from it. `E30DXMixer::SetListener` (`0x4cfb60`) moves the listener with the camera
  (`E30ConnCamera`, `CSigCamera`). `CDXPrologicMixer` (`0x4d09d0`-`0x4d1650`) and `CEAXMixer` (`0x4d3d90`-`0x4d4240`)
  are the alternatives. Which one runs (an option or the caps) is not traced (**loader**).
- **Voice.** `E30DXSound::Vf48` (`0x4d21e0`) creates one `IDirectSoundBuffer8` per voice: flags `0x500e0`
  (`GLOBALFOCUS | GETCURRENTPOSITION2 | CTRLVOLUME | CTRLPAN | CTRLFREQUENCY`), or `0x500b0` with `CTRL3D` (no pan)
  when the create flag `8` is set. The 3D voices also get an `IDirectSound3DBuffer` (**loader**).

### 2.2 Voice interface

Voice vtable slots used by the game code (**loader**; names ours, from the DirectSound call each reaches):

| Slot | Use |
|---|---|
| manager `+0x50` | create a voice from a wave, flags (`0x408` in the impact code), 0 |
| `+0x50` | play (argument 0) |
| `+0x54` | stop (**inferred**: the tyre reset calls it on every voice) |
| `+0x60` | set volume (linear 0-1) |
| `+0x64` | outside volume (`SOUNDDEF OutsideVolume`) |
| `+0x70` | pitch (1.0 = recorded rate) |
| `+0x74` / `+0x78` | max / min distance as called (stored at `+0x7c` / `+0x80`); in 1.02 both set the DirectSound min (2.4) |
| `+0x80` / `+0x84` | 3D position / velocity |
| `+0x88` / `+0x8c` | loop flags |

`E30DXSound::SetVol` (`0x4d2740`): volumes below 0.0001 become 0, values above 1 are clamped, values up to 5e-5 map
to -10000 mB, and the rest to `600 * log2(v)` mB (`E30_LogScale` `0x4d2060`: 6 dB per halving, so the game's volume
is a linear amplitude). `SetFreq` (`0x4d27c0`) passes Hz to `SetFrequency` (**loader**).

Most one-shot gains multiply `E31_4940()` (an attenuation term), the `SOUNDDEF` volume, the user's volume option
(`g_1660e10` effects, `g_1660e0c` for `GO`, `g_1660e18` tyres) and the `audio.ini` amp, for example
`CCollisionSnd::Vf64` (`0x4b2ed0`) (**loader**). Rust takes them from `rbr-sound --volumes` (`mixer::Volumes`, 1 by
default): the engine on the engine, reflections and whines, the co-driver on the speech and `GO`, the effects on the
turbo, blow-off, wind, rain, impacts, squeaks, pops and other one-shots, the tyres on the surface loops, rumble,
wetness and gravel hits. The harness sets the game's with `RBR_VOLUMES` (`harness/README.md`).

### 2.3 Update order

1. Physics step: the physics emits `EngineNoise`, `DrivetrainState`, `CSigTyreNoise`, damper, sensor and collision
   signals. The sound handlers (section 3) copy payload values into the car sound and fire one-shots (bang, blow-off,
   gear change, damage speech) at once (**loader**).
2. Frame: `CApplication` (`0x4825b0`) calls `E31Master::M48c0(g_8d5338, dt)` and then the master's `Vf08(dt)`. That
   updates the car sound (`CCarSound::Vf08` `0x4b0b50`), the tyres, ambient (`CAmbientSounds::Vf08` `0x4aaf50`), rain,
   and the streaming voices, which call the engine renderer for more samples (**loader**).
3. The 30 ms thread would call `Vf08(0.03)`, but it is off in 1.02 (section Summary). The `E32_EnterCrit`/`LeaveCrit`
   guards around sound calls are probably left over from it (**inferred**).

### 2.4 Voice distances (read in the disassembly, 2026-10-18)

- **Order.** `E30DXSound::Vf48` (`0x4d21e0`) and the stream create (`0x4d2410`) first reset the voice (`0x4dcbf0`,
  vtable `+0x04`: volume 1, outside volume 0, `+0x74` 10, `+0x78` 1), then create the buffer; the reset's two calls
  find no 3D buffer and only store the fields. So a voice that nobody sets afterwards keeps DirectSound's defaults
  (min 1 m, max 1e9 m) in 1.02 and under NGP.
- **Who sets what** (call order; 1.02 keeps the last value as the min, NGP the pair as called):

  | Voice | 3D | Calls | 1.02 | NGP |
  |---|---|---|---|---|
  | engine stream (`E30Engine::Vf04` `0x4cf220`) | yes (`0x498`) | `+0x78` `SOUNDDEF::EXHAUST` min, `+0x74` its max | min 100 | 40-100 |
  | gear and diff whine (`CDXTransmission` `0x4d2fe0`, `0x4d3260`) | no (`0x200`, `0x90`) | none | none | none |
  | turbo stream (`CDXTurbo` `0x4d3bf4`) | yes (`0x98`) | none | min 1 | min 1 |
  | blow-off (`E31Turbo::M3cb0` `0x4d3d2c`) | yes (`8`) | `+0x78` 2, `+0x74` 40 | min 40 | 2-40 |
  | tyre loops, hardware path (`M69a0` `0x4d69a0`) | yes (`0x498`) | `+0x78` 2, `+0x74` 10 | min 10 | 2-10 |
  | tyre wetness, hardware path | yes | `+0x78` 5, `+0x74` 20 | min 20 | 5-20 |
  | tyre loops, software path (`M6cb0` `0x4d6cb0`) | no (`0x200`) | `+0x78` 10, `+0x74` 40 (fields, read by `Mce250`) | 10-40 linear | 10-40 linear |
  | the wheel's mix stream, software path (`+0xb4`) | yes (`0x498`) | `+0x78` 100, `+0x74` 200 | min 200 | 100-200 |
  | rumble and wetness, software path | no (`0x200`) | wetness `+0x78` 10, `+0x74` 40 (unused) | none | none |

  The `SOUNDDEF` min and max of `TRANSMISSION`, `TIRE`, `TURBO` and `BLOWOFFVALVE` are loaded (`M48e0`) and never
  read: no code reads their offsets, and the indexed getter `0x4ac0d0` has no caller. `EXHAUST`'s are the engine's.
- **Which tyre path.** `CTireSnd`'s init (`0x4d72e6`) and frame (`Vf0c` `0x4d8290`) take the hardware path with
  0x21 or more channels from the mixer's `+0x44` (`0x4cf820`: `+0x440`, the caps' `dwFreeHwMixingAllBuffers`, 16
  when 0). Without DirectSound hardware mixing (every Windows since Vista, and Wine; **inferred**, not measured) that is
  under 0x21: the software path. There each wheel's two loops are 2D voices that `M7ea0` renders into the wheel's 3D mix stream at
  `Mce250`'s gain times 0.7 (`0x72f674`). `E30MixerBase::Mce250` (`0x4ce250`) is a linear law on the voice's own
  fields: its volume up to the min (`+0x80`), falling linearly to its outside volume (`+0x74`, 0 in retail) at the
  max (`+0x7c`), the outside volume past it. Rust models the software path.
- **Rust** (`mixer::Place`): the engine, turbo and blow-off as above, the tyre loops `0.7 * linear(10, 40)`, the
  whine and the wetness without distance; the others as in 6.1, 6.2, 7 and 13.

### 2.5 Sound limit (read in the disassembly and measured with the harness, 2026-10-19)

- **The limit.** `E32Factory::Make3` (`0x4db570`, the master's vtable `+0x50`) asks the mixer for a sound:
  `E30DXMixer::Vf48` (`0x4cf940`) and `Vf4c` (`0x4cf840`) compare the master's count (`[+0x280]+0x106f8`) with the
  limit `+0x440` (`Get440` `0x4cf820`). `Vf04` sets it to 16 at `0x4cfdca` when `Set440` (`0x4cf360`, from the
  caps' free hardware buffers, **inferred**) left 0. At the limit, a request without flag 0x400 gets the silent
  stand-in `+0x40`, which `Make3` deletes (the sound is refused); with 0x400 (the ranged impacts 0x408 at
  `0x4b2f5f`, `GO` 0x400 at `0x4ba5ea`; the tyre and other persistent voices 0x418) `Vf48` stops the first sound
  in the master's list (the newest) without flag 0x80 or 0x10 and makes the new one.
- **The count** is retaken once a frame by `E31Master::Vf08` (`0x4d4980`): the sounds playing (flag 2) without flag
  0x200. Every request in a frame sees the same count, so a frame can go past 16 (the harness saw 17).
- **Measured** (harness `RBR_POPS` with `RBR_FPS=60`, the retail rate with `WaitRetrace`; tanner in the car,
  12000 emits, 5228 frames): in the race the count is 16 or 17 throughout; the live list has 11 sounds that always
  play (base object, mono and stereo Ogg, turbo, four tyres, transmission, two reflections, exhaust, less one) and
  5 one-shots. Made / refused by caller: gravel hits (`0x4d745f`) 866 / 1598, blow-off (`0x4d3cd6`) 187 / 322,
  `Mf280` pops (`0x4af315`) 205 / 186, `Mf180` run pops (`0x4af1d1`) 51 / 115, `Mf3f0` bangs (`0x4af449`) 16 / 15,
  squeaks (`0x4d772f`) 7 / 26, ranged impacts (`0x4b2f68`) 4 / 0, `E31_34a0` 3 / 1, `GO` 1 / 0. So on current
  systems the original refuses about two in three one-shots; on XP with hardware voices the limit was the hardware
  count and everything played. Uncapped (about 2000 frames a second under Wine's windowed mode) the per-frame
  requesters take nearly every place: gravel hits 818 / 80979, blow-off 370 / 37420, `Mf280` pops 47 / 305.
- **Rust** (`mixer::SOUND_LIMIT`): every one-shot asks through the cap. `Command::Frame` (the models' 60 Hz frame,
  the tyres' clock) takes the count: 9 sounds plus the one-shots playing; the requests until the next frame see it.
  A sound made takes its place even when silent; the ranged impacts and `GO` stop the newest one-shot; the blow-off's
  `Mcad0` runs on the frame. FMOD voices are not capped (the banks carry no channel limit). Ours on the same drive in
  the car: gravel hits 879 / 1538, blow-off 185 / 397, `Mf280` pops 199 / 183, run pops 54 / 121, bangs 14 / 16,
  squeaks 11 / 15, ranged 1 / 0 (ours' drive has fewer impacts). With the 11 of the live list ours makes about a
  quarter fewer of each (gravel hits 667, pops 156); 9 matches every caller. Why the original's count leaves room
  for two more is open (its one-shots would have to end sooner than the samples' length).

### 2.6 Output level (ours, measured 2026-10-19)

The original's mix goes past full scale: its captured stream (Wine, options at 0.9, tanner, 12-85 s) has an RMS of
-8.1 dBFS in the car and -7.4 outside, with 2.1% and 2.9% of samples past 1 (a device clips them). Rust's master gain
(`mixer::HEADROOM`) is 0.8, which matches that RMS within 0.5 dB on both cameras; before its soft limiter (linear to
0.8, then a tanh toward 1) 1.4% and 2.7% of samples are past 1, 4.2% and 6.8% past the knee, peaks 2.6 and 2.4.

## 3. Signals into sound

`CCarSound` (vtable `0x731c74`, 0x51b60 bytes, ctor `0x4afcc0`) connects in `Vf04` (`0x4b1bc0`) to these signals'
"every state" lists (signal object `g_signalManager`, list at signal `+0x208`), so each handler runs inside the emit.
Decompile note: in `CCarSound` code, `m_data + X` is `this + 0x1d4 + X` (checked against the disassembly), unlike
`E31Master`, where `m_data` is `this + 4`.

| Signal (`g_signalManager +`) | Emitted by, rate | Handler | What sound takes |
|---|---|---|---|
| `EngineNoise` `+0x6050` | `E43EngineSim::Sim8E20` `0x518e20` via `0x50ef50`, each physics step | `CCarSound::M6a0` `0x4b06a0` | rpm, throttle, turbo, misfire, blow-off, cylinders (section 4) |
| `DrivetrainState` `+0xa78` | `CDrive::M8480` `0x508480` via `0x506e60`, each step | `0x4ae510` | gearbox and diff speeds, teeth, broken gearbox, gear change, wheel speeds (section 5) |
| `CSigTyreNoise` `+0x6268` | tyre `0x535890` (payload: tyre scratch `+0x420`, 24 words), each step per wheel | `CTireSnd` `0x4d5fd0` | surface, slip, contact (section 6) |
| `CarSensorData` `+0x12d8` | `0x4e8270` from the car body, each step | `0x4ae3e0` | speed, battery, brakes, water temperature |
| damper `+0xc2b8` | suspension, each step | `0x4ae260` | per-wheel damper travel |
| `EngineFire` `+0xb840` | | `0x4ae3a0` | fire flag |
| `CarObject` `+0xbc70` | car load | `0x4ac320` | `Point_engine`, `Point_gearbox`, `Point_exhaust1/2` positions from the car ini |
| `HingedBody` `+0xc4d0` | | `0x4ac220` | `p[0]` 0 or 1 into flags |
| float `+0x51a8` | `CCar` `0x4e95f0` (`car +0x17c`) | `0x4ac4a0` | one float stored at `this +0x1d4` |
| `CarGeneralState` `+0x2e10`, `AppState` `+0x6ce0`, `CarSound` `+0x7540`, `CarDamageHandlerInit` `+0xddf0` | | `0x4ac4c0`, `0x4ac7d0`, `0x4ac260` | car index, app state |
| `CarControl` `+0xc90`, `TireParticles` `+0x4518` | | `0x4ac790`, `0x4ac490` | nothing (empty handlers) |

Other listeners: `CCollisionSnd` (vtable `0x731e54`) on collision and damage signals; `CPaceNoteHandler` on
`CSigPaceNoteCallout` (`+0x6ae0`); `CSoundFX` (`0x4ba810`) on `CSigSoundFX` (`+0x6ef8`, also used by every menu);
`CAmbientSounds` on `CSigAmbientSnd`. The force-feedback controller (`0x491520`) also reads `CSigTyreNoise`.

## 4. Engine

### 4.1 Inputs (`EngineNoise` payload, `CCarSound::M6a0`)

The payload is the engine state from `+0x2c` (**loader**):

| Word | Source | Use |
|---|---|---|
| `p[0]` | smoothed engine speed, rad/s (state `+0x188`, filter 0.3 new / 0.7 old) | `rpm = p0 * 9.5493`, clamped to 500-9000; below 500 the engine voice stops |
| `p[1]` | throttle (engine `+0x20`) | `load = int(p1 * 100)` into `CEngine +0x1018` |
| `p[2]` | turbo shaft speed (state `+0x5c`) | turbo whistle (section 5) |
| `p[3]` | boost, atm (state `+0x68 * 9.8716682e-6`; ambient 101300 Pa) | turbo whistle volume |
| `p[4]` | misfire output (state `+0x3c`, `SimE520` `0x50e520` on the physics seed) | `Mf280(p4)` pops; `p4 > 0.9` calls `Mf3f0`; `p4 > 0.75` plays a bang, variant `rand() & 3` |
| `p[5]` | | `min(p5, 1) * 0.25` (a gain) |
| `p[6]` | turbo flow (state `+0x44`) | blow-off when `p6 > 2 * previous`: volume `min(p6, 0.6) * 1.6667` (`Mcad0` `0x4acad0`, `E31Turbo::M3cb0` `0x4d3cb0`) |
| `p[7]` | wastegate fraction (state `+0x48`) | |
| `p[8..11]` | per-cylinder flags | `CEngine +0x32cf0..+0x32cfc` (0 = that cylinder misfires) |

Turbo inputs come from the turbo model `Sim7D30` (`0x517d30`).

### 4.2 `CEngine` (vtable `0x732c64`, granular core)

- **Defaults** (ctor `0x4bc180`): rpm 1000 (`+0x101c`), output rate 44100 (`+0x1030`), sample clamp +-32767,
  resampler mode 2 (`+0x1004`), four cylinder flags set (**loader**).
- **rpm slew** (`M740` `0x4bc740`): each call moves the rpm by at most 500 toward the target and skips values listed
  in a table at `0x732cc8` (**loader**).
- **Bank choice** (`M580`/`M5d0`): the banks just below and above the rpm (**loader**).
- **Crossfade** (`M7b0` `0x4bc7b0`): `gain = 1 - (rpm - rpm_hi) / (rpm_lo - rpm_hi)` for one bank, the rest for the
  other; times 0.25 when the misfire flag `+0x1028` is set. Each bank mixes its load-0 and load-100 parts by `load`
  (`MixBd10`/`MixBdc0`) (**loader**).
- **Renderer** (`M9c0` `0x4bc9c0`): output comes in chunks of one firing period, `44100 / (rpm / 30)` samples
  (constant `0.033333335`). A cylinder counter rotates 0-3; if that cylinder's flag is 0, the chunk plays at half rate
  with the 0.25 gain (**loader**).
- **Grain choice** (`VoiceBb0` `0x4bbbb0`): the next cycle row of the part. At the end, it restarts at
  `(rand() % (count - 11) & ~3) + 5`. The resample ratio is the grain length over the target period (**loader**).
- **Resamplers:** cubic (`0x4bb3e0`, `0x4bb5c0`), linear (`0x4bb7a0`, `0x4bb8b0`), nearest (`0x4bb9d0`, `0x4bbac0`).
  Mode 2 is the default; which mode maps to which resampler is not checked (**loader**).

### 4.3 Engine output (`CCarSound::Vf08`, `CalcVol` `0x4ac8c0`, `E30Engine::Vf08` `0x4cebc0`)

- Gain: `SOUNDDEF::EXHAUST` volume times the `Engine` amp (1.0) (**loader**, **file**).
- The filter after `MixScale` (`0x4ced85`-`0x4cef5b`, **disassembly**, 2026-10-18), on the engine's `E32Flt`
  (`+0xf0`, the one-pole bilinear filter of 5.4: `SetCoef` `0x4dde60`, `Run` `0x4ddeb0` replaces the block with the
  filtered block times its argument, `Mix` `0x4ddf30` adds it):
  - in the car (master `+0x106ec`, `g[0x41bb]`): `SetCoef(400, 44100)`, `Run(2.0)`. A literal 400 Hz:
    `InsideCutFrequency` (750) is stored elsewhere (`*(CCarSound+0x1d4+0x1c) + 0x50`) and not read here;
  - in a replay (app `+0x728 == 8`) or on camera type 7 (`0x565470`): `d` = the dot product of the voice's
    `+0x40..+0x48` with the listener object's `+0xc..+0x14` (untraced); `d > 0`: `SetCoef(22050 - d * 21500)`,
    `Run(2.0)`; else `SetCoef(450)`, `Mix(2.0)`;
  - else (outside, live): `SetCoef(CCarSound +0x41934)` (`Car%d_LowShelfFrequency`, 200 in retail) and, with the rpm
    above 2500, `Mix((rpm - 2500) * 0.0004 * 2)`; at or below 2500 the block is untouched.
  - There is no distance term, and `Car%d_LowShelfAmp` (loaded into `CCarSound +0x41930`) is never read. The old
    notes' 750 Hz in-car and `320 / dist * 50` outside low-pass were wrong; the C++ reconstruction has 225 for the
    replay's 450.
- Optional 10-band EQ from `audio.ini`, off in retail (`EnableEQOnCars=false`) (**file**).
- `E32BufferHandler::MixScale` (`0x4d99f0`), run by `E30Engine::Vf08` on each engine block (the stream's `+0xdf8`
  and the reflections' `+0x20df8`): a gain of `(256 + trunc(min(rpm, 8000) * 0.000125 * 256 + (load << 7) * 0.01))
  / 256` (1 to 2.5), then a 4:1 knee above 0x4000 of the 16-bit range. This is `audio.ini`'s "compressor -6 dB 4:1"
  (**disassembly**). Rust: `rbr_sound::engine::mix_scale`.
- Two "Reflection" streaming voices carry a delayed echo of the engine (`EnviormentalReflections` amp), 5.4.

## 5. Transmission, turbo, blow-off and exhaust

### 5.1 Gear whine (`DrivetrainState` handler `0x4ae510`, `CDXTransmission` `0x4d2fe0`-`0x4d37f0`)

Payload (`CDrive +0x1a4`, **loader**):

| Word | Source | Use |
|---|---|---|
| `p[0]` | gearbox shaft speed | gear whine pitch `= teeth * speed / (2 pi) * 0.00067476`, clamped 0.125-4 |
| `p[1]` | tooth count of the gear (int; gear table `+0x6a4 + 0x50 * gear`) | |
| `p[2]` | differential speed (`+0x384`) | diff whine pitch, factor `0.0013495` |
| `p[3]` | diff tooth count (int, `+0x9d4`) | |
| `p[4]` | broken-gearbox flag | non-zero: damage call 8 (`gearbox_broke`); its rate `* 1/(2 pi) * 0.5` sets periodic driveshaft clunks `E31_3510` (`0x4d3510`), in the car only |
| `p[5]` | | clunk argument |
| `p[6]`, `p[7]` | loads, floored at 600 and 6000 | normalised; whine volume `= p6n * amp` (`EngineTransmission` 0.35, `TransmissionDiff` 0.35), 0 below 0.001 |
| `p[8]` | gear-change flag | `> 0.001`: gear-change click (`E31_34a0` `0x4d34a0`, random from `GearChange1-3`, in the car only, volume x0.9); cleared after the emit |
| `p[9]` | | `vf54` (`0x4ad090`) |
| `p[10..13]` | wheel speeds (`+0x94`, `+0x15c`, `+0x224`, `+0x2ec`), updated only while the wheel's driveshaft is broken (6.4) | `CTireSnd::M7590` per wheel |

### 5.2 Turbo (`CCarSound::Vf08`, `CDXTurbo` `0x4d3800`-`0x4d3cb0`)

- Volume: `clamp(turbo_speed * 7.6923e-5 * boost)`, 0 below 0.01, a third outside the car, times the `Turbo` amp
  (0.2) (**loader**, **file**).
- Pitch: `turbo_speed * 0.0004` (**loader**).
- Blow-off valve: section 4.1 (`p[6]`), sample `CAR::OTHER Valve`, amp `BlowOffValve` 1.0. The flow rise only arms
  it (`+0x41690`); `CCarSound::Mcad0` (`0x4acad0`) plays it each frame while armed when `E31_39f0` reads the last
  blow-off's cursor past 4000 samples or the voice gone, at `min(flow * 1.6667, 1)`, and disarms when the flow is
  0.01 or less or that volume 0.1 or less. So while the flow holds it repeats every 4000 samples, and a refused
  one asks again the next frame (2.5). Rust: once a 60 Hz frame (`car.rs`, `Repeat::After`).

### 5.3 Pops and bangs

`Mf280` (`0x4af280`), `Mf3f0` (`0x4af3f0`) and `Mf180` (`0x4af180`) play the `CARn::EXHAUSTNEW` pops and
`CAR::BANGS` with `SOUNDDEF::EXHAUSTPOP` and the `Pop`, `ExhaustPop`, `Bang` amps (0.5 each). Read in the
disassembly of `M6a0` (2026-10-17, **loader**):

- `p[4] != 0`: `Mf280(p[4])` plays a `CAR::EXHAUST` pop, index `rand() % (count - 4) + 4` above 0.8, else
  `rand() % count`, pitch `p[4] * 0.5 + 0.75`, `ExhaustPop amp * SOUNDDEF::EXHAUSTPOP volume`; it stops the last pop.
- `p[4] > 0.9`: `Mf3f0`, a `CAR::BANGS` sample (`bang1-6` + `.wav`), `Bang amp`.
- `p[4] > 0.75` with no run: a pop run starts with table `rand() & 3` (four tables at `0x7c3808`, each entry a
  `CARn::EXHAUSTNEW` index and the gap to the next in 44.1 kHz samples, a gap of 0 ends it).
- Then the rpm target `clamp(p[0] * 9.5493, 500, 9000)` above the last (stored as an int) ends the run, which also
  ends it on a steady engine (the fraction above the truncated int).
- The frame update (`0x4b0940`) adds `trunc(-dt * 44100)` to the wait; at 0 or below it plays `Mf180(kind)`
  (`CARn::EXHAUSTNEW Pop[kind % count]`, `Pop amp`) and waits the gap.

Rust: `rust/rbr-sound/src/exhaust.rs`.

### 5.4 Other car effects

- Fire (`EngineFire` handler `0x4ae3a0`): each emit sends damage call 6 (section 10.3); `Md1c0` (`0x4ad1c0`) plays
  `Audio\Cars\Effects\fire.wav` when the last one is not playing, `Enginefire amp`, min distance 20 (**loader**).
  Rust counts the `CSigEngineFire` emits (`0x4ef290`) in `Pure::enter`.
- Ignition, dog rings, broken cooler and wipers (**disassembly**, Rust `rbr_sound::effects`):
  - Ignition: the `DrivetrainState` handler (`0x4ae510`, `0x4ae73b`) keeps the starter word `p[9]` (`CDrive +0x1c8`)
    in `+0x41858` while the engine is stopped (`+0x41924`, set by `M6a0` below the low rpm, cleared above it); when it
    rises from 0, `Vf54` (`0x4ad090`) plays `CAR::IGNITION Ignition1` at the car if the last one has ended (`Ignition`
    amp, min distance 10 m). In the standalone's boots the engine already runs when `p[9]` is non-zero (0.85
    falling), so it never plays; whether 1.02 cranks at the stage start is a game-run question.
  - Dog rings: the handler keeps `f = p[4] / 2pi * 0.5` (`+0x41ac4`; `p[4]` is `|CDrive::M5A10|` while
    `dri +0x55c` is 0 in `CDrive::M8570` `0x508570`, else 0) and `p[5]` (`dri +0x560`). Each frame with `f > 0`
    (`Vf08` `0x4b1085`) a clock `+0x41b14` gains the frame time and, at `f / 2pi`, loses `1 / f` and plays a random
    `CAR::DOGRINGS DR1-8` in the car, 2D, at `p[5] * 0.7` (`E31Transmission::M3510` `0x4d3510`). `p[4]` stays 0 in
    every drive tried.
  - Broken cooler: `M6a0` keeps `min(EngineNoise p[5], 1) * 0.25` (`+0x41abc`); `Md2b0` (`0x4ad2b0`) loops
    `Audio\Cars\Effects\cooler.wav` at the car (`BrokenCooler` amp times it, min 5 m) while it is at least 0.001.
    `p[5]` is 0 in every drive tried; its source is not traced.
  - Wipers: `Md490` (`0x4ad490`), in the car, with the glass on (`0x167e834`, camera modes 4-6) and precipitation,
    plays `CAR::WIPER WiperEngine` (amp `WiperEngine` 0.25) and `WiperUp` (`+0x41928 != 0`) or `WiperDown` (amp
    `Wiper` 0.25), 2D, when `E88Wiper +0x30` (`0x167e830`) differs from `CCarSound +0x41928`. Nothing writes
    `+0x41928` after the ctor's 1 (the getter `0x4abeb0` has no callers), so 1.02 starts both again every frame of
    each return stroke. Rust plays both once per change of direction (ours). `E88Wiper` (`InitTable` `0x5f8a50`,
    `Tick` `0x5f8720`) turns blade 0 at 3.4558 rad/s over 0.0942-1.4765 rad and blade 1 at 3.2987 over 0.0942-1.4137,
    0.4 s each way, flipping `+0x30` at blade 0's stop; its enable (`0x167e8e8`) is `M9b80(0) * 0.01`, set each frame
    by `E87EffectHost::Tick` (`0x5f6790`): any rain or snow. No intermittent mode.
  - The rumble is 6.1.
- Engine reflections (**disassembly**, Rust `rbr_sound::reflections`): `Reflection L/R` streaming voices
  (`CCarSound +0x1090`/`+0x1094`), 3D head-relative at 1 m left and right (`Vf04`; the DX mixer's `+0x70` is 0),
  volume `EnviormentalReflections amp * CCarSound+0x1d8` (`CalcVol`'s engine volume). `Vf08` (`0x4b11cd`-`0x4b14fe`)
  feeds them from the engine's block after `MixScale` (`CDXEngine +0x20df8`, 4.3): `E25Engine` (`+0x111c`)
  low-passes it (`Filter80` `0x4b8780`, one-pole bilinear: `p = 2pi f`, `y = (88200 - p)/(88200 + p) y + p/(88200 +
  p) (x + x')`) into a 32k ring (`Mix60`), and `Blend90` (`0x4b8e90`) reads it back per side a delay behind (plus the
  ring's 128-sample lag, `InitD0`) at `((trunc(gain * 128) * vol) >> 8) / 256` into `+0x1138` (L) and `+0x11138`
  (R); `StereoD0` crossfades 256 samples on a delay change. `vol` is 256 from 4500 rpm or in a replay, else `128 +
  trunc(rpm * 0.028444)`. The delays and gains are the master's `+0x106cc..+0x106d8` (`Mcb80` `0x4acb80`:
  `trunc(m / 340 * 44100)`, cap 0x60c0): in the car `Vf08` sets 0.4 m / 1.4 m, gains 1, and `Mcb80` then sets the
  left to 1.6 m (207 and 181 samples). Outside nothing sets them (they keep the car's once set; 0 before, the heap
  block, **inferred**); `E12_AnimSet::CbD80` (`0x477d80`) tags 0x15-0x18 on `CSigAnimReal` could, but the five retail
  `.dls` files with a `Real` channel have only tag 11. The cutoff is 750 Hz in the car, else
  `min(max((L + R) * 31.25, 100), min(16000 / distance, 2000))`. A side's feeder gets the delayed block alone,
  or, in the car with that side's flag set, the delayed block plus `clamp(raw * trunc(g_1660e08 * 512) / 256,
  ±24576)` (`M86a0`). The flags (`+0x41a9c` left, `+0x41aa0` right) are `SetPair` (`0x4ac220`) of the `HingedBody`
  signal that `CHingedRigidBody::SimFB40` (`0x51fb40`) emits each step: `p[0]` the side (`def +0x4fc`), `p[1] =
  (*(+0x650) == 0)`, the body off the car (`SimEFE0` clears `+0x650` on the detach). `Zero4` (`0x4ac260`) sets a flag
  when the car has no hinged body on that side, reset clears both (`0x4acd5d`). Rust latches the doors from DET1
  (`Inputs::doors_off`), which lists only awake parts. Ours: the output rate, a hard left/right split, the delay
  change at once. No retail drive tried detaches a door.

## 6. Tyres, surfaces, off-road and suspension

### 6.1 Tyre loops (`CTireSnd`, our `E31Tire`, vtable `0x7375c0`, inside `CCarSound` at `+0x380`)

Four wheel slots of 0x2b0 bytes from `+0x40` (**loader**, [../src/rbr/RBR_E31_Tire.cpp](../src/rbr/RBR_E31_Tire.cpp)).
The `CSigTyreNoise` handler (`0x4d5fd0`) reads the tyre's 24-word block:

- `sig[0]` wheel; `sig[0xc..]` position; `sig[0x10..]` velocity.
- `sig[0x14]` surface id. A table at `0x735c80` (256 rows of 0x18 bytes) gives word 0 (0.6, 0.4 or 0.25, stored at
  slot `+0x2a0`: the gravel hit's pitch base, not a gain), the first and second material (0 gravel, 1 tarmac, 2 snow), a 16.16 pitch, the second
  material's 16.16 share and a variant (0, 0x400, 0x800) that the handler loads and discards. `E31_5fa0`
  (`0x4d5fa0`) returns 0 for the 80 ids listed at `0x7c5870` (the road surfaces) and 1 for the rest.
- `sig[0x17]` contact flag: 0 silences the wheel.
- The level (read in the disassembly, 2026-10-17): `a = sig[5] > 5000 ? 1 : sig[5] * 0.0002` (tyre factor),
  `b = sig[4] >= 2 ? 1 : sig[4] >= 1.25 ? (sig[4] - 1.25) * 4/3 : (tarmac first ? 0.5 : 0)` (grip maximum),
  `c = sig[8] > 40 ? 1 : sig[8] * 0.025` (slip ratio), `d = min(sig[9] * 0.05 + sig[8] * 0.1, 1)` (speed);
  `level = min(trunc((a + b + c) * 2000 * d), 4000)`. Voice 1 plays the first material at
  `(1 - share) * level / 4000 * g_1660e18`, voice 2 the second at `share * level / 4000 * g_1660e18`, both at the
  table's pitch. `sig[4] > 1.25` switches both to the skid loop (`M7ba0` picks the wave by material and skid; the
  loaders put `NormalGravel`, `SkidGravel`, `NormalTarmac`, `SkidTarmac`, `NormalSnow`, `SkidSnow` at `+0xafc` to
  `+0xb10`). A changed wave restarts at `wheel * 0.25` of its length.
- Wetness (voice `+0x84`, `SkidWater`): `g_1660e18 * min(splash, 1)` when the splash (slot `+0x70`, written by
  `M6560` `0x4d6560` from `CSigTireParticles` +0x98) is above 0, else 0; `M7ba0`/`M7ea0` decay it by 0.95 a frame, to 0
  below 0.001. In the drives tried the splash is 0 (the wet `drive-wb-wet-corolla-54` too).
- Rumble (voice `+0x80`, `M6750` `0x4d6750`, `CAR::RUMBLE`): in the car, with contact and the first material not
  tarmac, `x = min(speed * 0.1, 1)`, `level = min((sig[1] + 1 + factor * 1.5625e-5) * x, 1)`, the volume moving a
  quarter of the way to the level each frame and set at half of it times `g_1660e18`, pitch `x * 0.4 + 0.8`;
  otherwise stopped (its volume kept).

Per frame, `Vf0c` (`0x4d8290`) calls `M7ea0` when the backend has fewer than 0x21 hardware channels and `M7ba0`
otherwise. `M7ea0` is the software 3D path (`E30MixerBase::Mce250`), not a mix of the wheels: each wheel plays at its
own level, times 0.7 and the path's linear distance gain (2.4). Without hardware mixing the software path runs.
`M7ea0` mixes both surface voices (`E32Voice::Cfg` 0x4d80e8, `Cfg2` 0x4d81b1, nearest-sample `MixA`/`MixB`) at the
first voice's `Mce250` volume and its pitch times `Mce250`'s Doppler factor (0.5-2), so the second voice's own
volume (the share) is unused: on a surface of one material (share 0, both materials the same) the second voice
plays the same loop from the same place, doubling the first (+6 dB; read in the disassembly 2026-10-19). Rust
plays both at the first's volume (`tyre.rs`); the Doppler factor and the nearest-sample mixing are not modelled.

Gravel hits (`M7330` `0x4d7330`, per frame, **loader**): only with the car within 20 m of the listener. A wheel's
period `n` (slot `+0x280`) is `trunc(50 / (sig[2] force sum + 0.01))` off tarmac-first surfaces and 0 on them; with
`1 <= n < 1000` and contact a counter runs, and at `n` it plays `IMPACT::GRUS[rand() % count]` (`E31Wave::LoadWaves`
reads `Snd1` up, so 20 samples; `Snd0` is unused) at pitch `row[0] + (rand() % 10) * 0.02 - 0.1`, volume
`GravelParticles amp * SOUNDDEF::TIRE volume * g_1660e18`, distances 1-10 m as called (`+0x74` 10 at 0x4d7530, then
`+0x78` 1: in 1.02 min 1 m and the default max, see Open), at the wheel (in the car z drops by up to
0.2 m). Rust places each hit at its wheel's block position (`+0xc`). Samples: `TIRE::SURFACES` (Normal and Skid for Gravel, Tarmac, Snow,
Water) (**loader**, **file**). Rust: `rust/rbr-sound/src/tyre.rs`.

In-car low-pass (`E31Mgr::M4870` `0x4d4870`, decompile 2026-10-19): with the in-car flag (`g[0x41bb]`) set and its
`z` argument 0 it calls the voice's `CSound::SetRate` (`0x4dd110`, vtable `+0x90`) with 1000, else with 1.0. That
only sets an `E32Flt` (the one-pole of 4.3) and its flag (`+0x110`). The filter runs only where a voice is mixed in
software: `CSound::Vf10` (`0x4dccd0`, after `Cfg2`) and `E30Prologic::Vf08`. The callers' voices are
`CDXSound` buffers (vtable `0x7353bc`: `+0x10` is not `CSound::Vf10`, `+0x60` is `SetVol`), which DirectSound plays as
loaded, and `M7ea0` mixes the surface loops through `E32Voice::Cfg`/`Cfg2` itself without the filter. So on the
DirectSound path (`UseSoftwareAudio = false`, the default and the test install's; inferred to pick it) nothing is
low-passed. The A/B on cachy agrees: the original's tyres are as bright in the car as outside
(`lead/reports/sound-a.md` Phase 2). The callers: the gravel hits (`M7330`), the squeaks (`M7700`), the bottoming (`M77c0`) and the driveshaft clunk (`M7590`), all with `z = 0`,
and the surface loops in `M7ba0`/`M7ea0` with `z` the wheel's skid flag (`m_data + 0x2c0` per wheel, which `M5fd0`
sets above a grip maximum of 1.25 and which picks the skid wave). Rust has no filter on these.

### 6.2 Off-road rattles (`CCarSound::Mce10` `0x4ace10`)

When the speed (`CarSensorData`, `|p+8| * 3.6` km/h) is at least 5 km/h and all four wheel slots have the
`E31_5fa0` flag (the surface is not a road surface; no contact clears it), a timer of `10 / speed` runs; at zero a
random `Offroad1-7` plays (min distance 10, max 20 or 11 m as called: `+0x78` 10, then `+0x74`, so in 1.02 a min of 20
or 11 m) times the `Offtrack` amp. Without the four flags the
timer is 0 (**loader**).

### 6.3 Suspension (damper handler `0x4ae260`)

Payload `[owner, peak, bump, droop, valid]`; nothing when `CCarSound+0x1060` is set. Read in the disassembly
(2026-10-17, **loader**):

- `Vf5c(|peak - last peak|, i)`: in the car, at 1.0 or more and `rand() % 10 == 0`, a squeak (`M7700`,
  `CAR::SQUEEKS`, `Squeeks amp`, pitch 0.5, at the wheel).
- In app state 1, `valid == 0` sends damage call 7.
- `bump > 0.01` in the car: `CCollisionSnd::Vf64((bump + 1) * 200)`, a ranged impact at the wheel; above 0.8 also
  `M77c0`, the bottoming (`CAR::SUSPENSION`, `Suspension amp`).

The emit (`0x52e010`) comes from the wheel's frame finish `CWheel::Vf28` (`0x53f230`), which resets the words; the
standalone does not run it, so Rust takes the strut velocity (`+0x5cc`) for the frame's peak and a changed bump word
for a hit, kept over a 60 Hz frame as the damper step keeps them between resets: the frame's peak is the step
velocity of the largest size, its bump the last hit, 0 without one (ours: the frame rate). Rust:
`rust/rbr-sound/src/suspension.rs`.

### 6.4 Broken driveshaft (`CTireSnd::M7590` `0x4d7590`, read in the decompilation 2026-10-19)

- **Input.** `CDrive::M8480` (`0x508480`, from the frame loop) copies wheel `k`'s speed into `p[10 + k]` only while
  the word at `(CDrive +0x170) + 0x18 + 0xc k` is 0, else keeps the last value (the constructor's 0). `+0x170` is
  the damage handler's block at `+0x3a18` (`E39_Link3AB0` `0x503ab0`, also `CCar +0x620`), so the word is
  `+0x3a30 + 0xc k`: `CDriveShaftDamage::Vf00` (`0x501610`) reports damage 5 when it is 0 and `Vf04` (`0x501640`)
  repairs it to 1, and `DmgCF20` (`0x4ecf20`) clears it in an impact. With the shafts whole, `p[10..13]` stay 0.
  Measured in the standalone: the words are 1 on all four wheels through tanner and three other retail drives.
- **Clunk.** `CCarSound::Vf08` calls `M7590(k, p[10 + k], dt)` each frame. The period is
  `|1 / (f1 * 0.15915494)|` (`0x731b1c`; 0 for `f1 == 0`), one turn of the wheel. With a period and `|f1| > 10`
  (`0x72b154`) the wheel's timer (`+0xb90`) adds `dt`; once it is above the period it goes back to 0 and a random
  `CAR::DRIVESHAFT` wave (`DS1-13`, bank `+0xb38`) plays: the master's `+0x50` with flags 8, 1 (so under the sound
  limit, 2.5), volume `E31_4940() * g_1660e10 * DriveShaftBroken` (the effects volume, amp 1.0), outside volume 0,
  at the wheel (the tyre slot's `+0x7c`, velocity `+0x8c`), pitch unchanged; in every camera. `E31Mgr::M4870`
  (the voice's low-pass) is not modelled.
- **Rust.** SND1 takes `M8480`'s test (`rbr_recording::sound`), so its `p[10..13]` are 0 while the shafts are whole
  and the drive's wheel speeds once one breaks (ours: after a repair the game keeps the last speed, SND1 the
  constructor's 0; no repair happens on a stage). `rust/rbr-sound/src/driveshaft.rs` runs `M7590` on the 60 Hz frame.
  Tanner renders are unchanged (byte-identical); with wheel 0's word forced to 0 the shaft clunks once a turn (169
  requests in 30 s, about half refused at the limit).

## 7. Impacts (`CCollisionSnd`, vtable `0x731e54`)

- **Collision** (`OnCollision` `0x4b2490`): power `sig[0xb]`. Below 20 nothing; 20-90 calls `vf4c`, 90-400 `vf48`,
  400 and above `vf44`. All three are empty in 1.02 (**loader**).
- **Damage** (`OnDamage` `0x4b27c0`, 20-word payload, power `sig[4]`) (**loader**):
  - power 100 or more merges the impact points;
  - above 5000: `vf58` (shatter);
  - above 8000 in the car: stops the speech voice and plays `vf6c` (`IMPACT::SPEECH` "ugh", amp `ImpactScreams` 0.5);
  - `sig[1]` in 1-3: `vf64(power + 500)`, the ranged impact. Only for `v >= 200`: index
    `rand() % 3 + trunc((v - 200) * 0.005)`, capped at `count - 1 - rand() % 3`; volume `E31_4940() *
    SOUNDDEF::RANGED volume * g_1660e10 * Impacts amp`;
  - `sig[0]` 1 or 4: `vf58`.
- **Glass** (`vf54` `0x4b2d10`): random from `IMPACT::GLASS`.
- **Retail data mutes most of it.** `SOUNDDEF::SHATTER` and `GLASS` have `Volume = 0.0`, so `vf58` and `vf54` are
  silent; only `RANGED` (1.0) is heard. The `GRAVEL`, `SOFTMETAL`, `BRANCH` and hit sets are never read (**loader**
  plus **file**, [formats/audio.md](formats/audio.md)).
- **Gravel hits** (`IMPACT::GRUS`): played by the tyres, 6.1. `SOUNDDEF::GRAVEL` is never read.

## 8. Wind, rain and the world

- Wind and birds: `CAmbientSounds`, [marks-sound-ghost.md](marks-sound-ghost.md) 2.2.
- Siren and the `.dls` spheres: [marks-sound-ghost.md](marks-sound-ghost.md) 2.3-2.4.
- Rain: `CDXRain` (`E30Rain::Vf08` `0x4d17e0`, **loader**) runs in app states 1, 8 and 10. With no precipitation
  (`E25AppObj::M9b80(0) == 0`) it stops its three voices; with rain (`M9e60`, not snow) it sets their volume to the
  `Rain` amp times the effects volume and renders three synthesised streams (`E25Rain::Render90` `0x4b7b90`: drops of
  random length `[n, 2n)` and random gain, `KernelA0` shapes each) through `E32Flt` filters at 300 Hz, with a gain of
  2.0 outside the car and 1.0 inside (the third stream's cutoff is 0 outside). Read in the disassembly: `n =
  ftol((1 - min(amount * 0.01 + 0.5, 1)) * 50 + 4)`, 4 for both light (50) and heavy (100) rain, so the two sound
  the same (the reconstruction's C++ has this wrong). Per drop `q = rand() & 0xffff`, a step of `±(q & 0xfff)` (the
  sign from the sample count's parity), `span = q % n + n`, gains `q * 0.125` (streams 1, 2) and `(rand() % 100) *
  0.01` (stream 3); `KernelA0` (`k = 0.25` from `Set50(0.25, 0.8)`) is a two-pole smoothing of the step, which
  decays by 0.9 a sample. `E32Flt` is the same one-pole bilinear filter as `Filter80` (5.4). Rust:
  `rbr_sound::mixer` `Rain`, about 2170 RMS (16-bit) outside.
- Rescue helicopter (`heli_g`; `heli_r` in a replay), the rally incident (`snd_chopper`, never played) and the bird
  strike (`birdhit`): section 13.

## 9. Countdown and finish

- The HUD keeps its own timer at HUD `+0x244`, set to 7.0 in `0x5772e0` and lowered by the frame time in `0x573d90`
  (it fires `start_animation2` below 6). The HUD draw `FUN_00578c30` emits `CSigSoundFX` (`+0x6ef8`) once each: 8 at
  `t <= 3`, 7 at `t <= 2`, 6 at `t <= 1`, 9 at `t <= 0` (while `t > -2`), with done bits in HUD `+0x250`. It also emits
  `CSigCountDownTimer` (`+0xa138`) (**loader**).
- `CSoundFX` (`0x4ba810`, checked in the disassembly): 6, 7 and 8 call the pace-note handler's `Vf48` (`0x4b64d0`)
  with 1, 2 and 3. That plays sample `idx - 1` of `NUMBER::NUMBERS` (`p1_`, `p2_`, `p3_`: the co-driver says the
  numbers) on the speech voice. 9 goes to `0x4ba690`, which plays `SFX::INGAME GO` (`Audio\Game\Go.wav`; `POLISHGO`
  when the language is 6, but the ini has no such key) at volume `g_1660e0c` (**loader**, **file**).
- The countdown is a frame-rate HUD timer, not the gameplay start in `rbr_gameplay::stage` (`m_unk714`). Rust has it
  as `rbr_gameplay::race_clock::RaceClock::countdown` (HUD `+0x244`, ticked in the driving state); HUD2 carries it.
  In `rbr-standalone --live --boot` it is already below 0 at the first answer (the boot's countdown phase runs no
  steps), so a boot drive says no countdown; `drive-arctic-snow` says "three" and then "two, one, go" at once (the
  clock's jump start).
- Finish: no finish sound was found. The co-driver's "finish" is the pace-note word `FINISH` (tag word `0x1b`) from
  the stage's markers.

## 10. Co-driver

### 10.1 Loading (`CPaceNoteHandler`, our `E25Handler`)

`M7170` (`0x4b7170`): language 0 (English) reads `Numbers.ini` and `CoDriver.ini` with paths `Speech\Number\%s` and
`Speech\PaceNote\%s`, several takes per word. Other languages read `Numother.ini` and `CoOther.ini` with
`Audio\Speech\Num%s\%s` and `Audio\Speech\%s\%s`, one take. `M6e40` loads the 14 `RANGE` banks and 43 `PACENOTE`
banks. `FindBank` (`0x4b63a0`) returns a random take unless the caller asks for one (**loader**, **file**).

Word ids (`Word5880` `0x4b5880`, **loader**):

| Id | Bank | Id | Bank | Id | Bank |
|---|---|---|---|---|---|
| 0-6 | RANGE 400-1000 | 0x16 | DONTCUT | 0x26 | MEDIUMRIGHT |
| 7, 8, 9 | RANGE 100, 150, 200 | 0x17, 0x18 | EASYLEFT, EASYRIGHT | 0x27 | NARROWS |
| 0xa, 0xb | RANGE 30, 300 | 0x19, 0x1a | FASTLEFT, FASTRIGHT | 0x28 | OVERCREST |
| 0xc, 0xd | RANGE 50, 70 | 0x1b | FINISH | 0x29, 0x2a | TIGHTENSBAD, TIGHTENS |
| 0xe, 0xf | 90_LEFT, 90_RIGHT | 0x1c | FORD | 0x2b | (none) |
| 0x10 | AND | 0x1d, 0x1e | HAIRPINLEFT, HAIRPINRIGHT | 0x2c, 0x2d | TWISTY, WIDEOUT |
| 0x11, 0x12 | BRIDGE, BUMP | 0x1f | INTO | 0x2e, 0x2f | PLUS, MINUS |
| 0x13, 0x14 | CAUTION, CAUTION_WATER | 0x20 | JUMP | 0x30 | ONTO |
| 0x15 | CUT | 0x21, 0x22 | KLEFT, KRIGHT | 0x31, 0x32 | FLATRIGHT, FLATLEFT |
| | | 0x23, 0x24 | LONG, MAYBE | 0x33-0x36 | GOSTRAIGHT, KEEPRIGHT, KEEPLEFT, KEEPMIDDLE |
| | | 0x25 | MEDIUMLEFT | | |

The ini's `CARE` bank has no word id; the tags that might mean care map to `CAUTION`. The tag-to-word and flag-to-word
maps (`E25_SlotId` `0x4b55e0`, `E25_BitId` `0x4b5540`) are already in `rbr_gameplay::markers` (`slot_word_id`,
`flag_word_id`).

### 10.2 Phrase building (handler `0x4b65b0` on `CSigPaceNoteCallout`, read in the disassembly)

1. Tag `0x17` (split): nothing. In game flow 7, tag `0x16`: nothing. Also gated on the replay state, `0x5fe390`, and
   option flag `+0x3e8 & 4` (the co-driver switch, **inferred**).
2. Tag `0xd` (distance) clears the pending flags `+0x50` and `+0x54`.
3. If `+0x54` is pending, queue `AND` first.
4. Queue the tag's word (`E25_SlotId`), then the flag word (`E25_BitId(word1)`), if any.
5. Distance: the payload's (`E12_SeekD`), truncated, with `M5760(d, 1)`; when the payload's is exactly 0,
   `ftol(0x476b10(key))` with `M5760(d, 0)`, which never queues a word. `0x476b10` is the distance from the key to
   the next key with a spoken tag (the switch at `0x476bbc`, the same set `E12_SeekD` scans for), past start, finish
   and split keys, or -1. `FUN_00572e60` snaps to the nearer end of the segment the value is in, of
   {10, 15, 30, 50, 70, 100, 150, 200, 300, 400, ... 1000} (below 10 to 10, above 1000 to 1000). `M5760`: below 11
   sets `+0x50` and clears `+0x54`, below 30 sets `+0x54` and clears `+0x50` (an "and" before the next call),
   otherwise (when asked) queues the RANGE word and leaves both flags.
6. With the next key (`0x475de0(key, 1)`: the key after it, past start, finish and split keys): if there is one and
   `+0x50` is set, the next tag is not one of {0x16, 0x13, 0x1c, 0x10, 0x14, 0x1e, 0x1d, 0x1f} and the current tag is
   not 0x16, queue `INTO` (`0x1f`), or `ONTO` (`0x30`) when the current tag is 0x1b; then clear `+0x50`. Without a
   next key `+0x50` stays.

Rust: `rust/rbr-sound/src/codriver.rs` (the switch: `CoDriver::muted`; the launcher's Co-driver box);
`rbr-standalone --live` sends each callout with the recomputed `E12_SeekD`
(`Game` passes 0), the gap and the next tag in the PCN1 record.

The callout moment itself is the gameplay marker logic: `rbr_gameplay::markers` `PaceCallout`, with the callout
distance (`0x1660b74`, default 4.0) and speed threshold (`0x1660b80`).

### 10.3 Speech queue and damage calls

- Words go to the speech voice with vtable `+0xc (txt, sample, priority)`. The queue (`SOUND::CSpeech::SpInsert`
  `0x4dd4d0`) holds 10 entries (a full queue drops the new one); `E32_SpPop` (`0x4dd5a0`) takes the head (**loader**).
- **Priorities** (the call sites, **loader**): pace-note words (`E25Handler::M6510` `0x4b6510`) and the countdown
  numbers (`Vf48` `0x4b64d0`) queue with 0, damage calls (`CCarSound::OnSpeech`) with 2, rally-school speech with 0
  or 1. `SpInsert` stores `n ^ pri << 30` and places the new entry after the last entry with
  `pri <= entry >> 30`, the shift being signed: priority 2 reads back as -2. So a damage call goes before every
  queued entry (the newest damage call first), and a pace note goes after the last queued note, or first when only
  damage calls are queued. Nothing interrupts the word playing. (When the place is 8 and the queue holds 9, the
  shift is skipped and entry 8 is overwritten; an edge case.)
- **Gap between words** (`E31Speech::Vf08` `0x4d2e30`, **loader**): the next entry is popped only when the voice is
  idle (`+0x95`, **inferred**), the global fade `g[0x41bc]` is 1.0 and a timer, lowered by the frame time only while
  idle, is below 0. After each pop the timer is set to 0.3715 s (`0x7354e8`) only when the HUD's race clock
  (`*0x165fc68 + 0x140`) is at most 0. So before the start the co-driver pauses 0.37 s after each word; once the
  clock runs, the words follow each other at the next frame.
- Damage calls (`CCarSound::OnSpeech` `0x4ab940`): each code plays once (a bitmask latch; when it is reset is not traced) and never in replay
  (`E19IsState2`). Path `audio\speech\broken\%s\%s.ogg`, queued on the same speech voice. Table at `0x7c37e0`: 1
  `batterys_flat`, 2 `steering_broke`, 3 `problem_engine`, 4 `wv_lost_brakes`, 5 `water_temp`, 6 `sus_broke`, 7
  `weve_got_fire`, 8 `gearbox_broke` (**loader**).
- Senders: `CarSensorData` battery `p+0x30 <= 6.0` sends 1, brakes `p+0x2c == 0` sends 4, water temperature
  `p+0x14 > 120` sends 5; broken gearbox sends 8 (5.1); `EngineFire` sends 6 (the immediate is 6 in the
  disassembly), so a fire plays "sus_broke", and a broken damper sends 7 (6.3), so it plays "weve_got_fire": the two
  are swapped in 1.02. Rust keeps it. NGP patches the two pushes back (0x4ae2eb 7 to 6 in the damper handler,
  0x4ae3c0 6 to 7 in the fire handler, read in the `ngp-c6-bis` snapshots), so under NGP a broken damper says
  `sus_broke` and a fire `weve_got_fire`; `rbr-sound` does the same under NSD1. Codes 2 (`steering_broke`) and 3 (`problem_engine`) have no sender: the callers
  of `0x4ae0d0` are only these, so they are never said (**loader**).

## 11. Determinism

- **No writes back.** The sound handlers read signal payloads and write only sound objects. They run inside the
  physics step, but no sound code writes physics, car or gameplay state (**loader**, every handler in section 3).
- **`rand()`.** Sound calls msvcr71 `rand()` (IAT `0x729540`) from `CCarSound` (`M6a0`, `Vf5c`, `Mce10`, `Mf280`,
  `Mf3f0`), `CCollisionSnd`, `CTireSnd`, `CEngine` (`VoiceBb0`), the pace-note handler, rain, ambient and music.
  The CRT state is per thread and everything is on the main thread, so these calls move the same stream as other
  `rand()` users: weather pick `0x5b53b0`, the E80 season/AI code, the rescue helicopter, `E20SplineField`. The
  physics randomness (misfire, `E35_Rand`) is its own seed, not CRT `rand`. Sound can therefore change which random
  numbers those non-physics systems get, but never the physics (**loader**; the list is from a scan of `rand` call
  sites).
- **Not sound:** the "sound gate" `0x544e60` named in [gameplay-loop.md](gameplay-loop.md) is the attract daemon
  (`E32Host40 + 8 == 0`), not sound.
- **3D calls from the car.** `CCar::Vf28x` reaches the DirectSound 3D position and velocity setters; the car-body
  harness stubs it (`replay.py`). It is presentation only.

## 12. Rust plan sketch

### 12.1 Modules

- `rbr-formats`: `eng` (the [formats/eng.md](formats/eng.md) layout, refusing two-wave parts), a small ini reader for
  the audio inis (or reuse the existing ini code), and `dat` for `audio.dat` (already there). Ogg and WAV decoding
  come from crates (`lewton` or `symphonia`; `hound`).
- New crate `rbr-sound`, in two layers:
  - **Pure models** (no audio device, unit-testable): engine (rpm slew, bank choice, crossfade, grain choice, the
    renderer producing samples), transmission and diff whine, turbo and blow-off, pops and bangs, tyre voices,
    off-road timer, impacts, the pace-note phrase builder (word ids, distance snap, AND/INTO/ONTO), the damage-call
    latch and the countdown triggers. Inputs: per-step signal payloads and per-frame camera and app state. Outputs: a
    list of voice commands (`create`, `play`, `stop`, `volume`, `pitch`, `position`, `queue speech`).
  - **Backend**: plays those commands. It mixes the voices, applies distance attenuation, does the in-car low-pass,
    and streams the engine.
- The standalone and the viewer connect `rbr-sound` where the physics already calls host sinks.

### 12.2 Signals: what Rust has and what is missing

| Needed | In Rust now |
|---|---|
| Engine noise payload (engine state `+0x2c` words 0-11) | The physics computes it; `EngineStepHost::emit` (`engine_driveline/engine.rs`) is the sink where a listener plugs in. |
| Drivetrain payload (`CDrive +0x1a4`) | Computed; the emit goes through `physics_core::signal::emit` on `+0xa78`. |
| Tyre noise block (scratch `+0x420`) | `tyre::layout::emit_noise` (`+0x6268`) |
| Car sensor data (speed, battery, brakes, water temperature) | `car_body/sample9800.rs` emits `+0x12d8` |
| Pace callouts | `rbr_gameplay::markers::PaceCallout` (tag, word1, distance, word ids) |
| Countdown | `rbr_gameplay::stage` has the gameplay countdown; the HUD's 7.0 s sound timer is not modelled |
| Damper, collision and damage payloads, `EngineFire`, `CarObject` points | Not checked in the Rust physics; needed for suspension, impacts, fire and voice positions |
| Camera position and in-car flag, app state, replay state | Viewer and game state; not wired to anything yet |

The standalone has no sound connections (rust-plan, "Decisions in force"). Adding listeners must not change the
physics: they only read payloads.

### 12.3 Audio crate

Use **cpal** for output with our own mixer. The engine is a custom granular synth that writes samples per firing
period, and the game needs only simple one-shots, loops, linear gain, pitch by resampling, and distance attenuation.
That is little code on top of cpal, and it keeps the mixing deterministic and testable offline (render to a buffer
without a device). cpal covers Windows (WASAPI), macOS (CoreAudio) and Linux (ALSA, PulseAudio, PipeWire). **kira**
is the alternative if we want its tweening and spatial tracks; the engine would then be a custom `Sound`. rodio's
source-chain model fits a per-firing synth less well.

Resampling (sound-a, 2026-10-18): DirectSound plays each buffer at its `SetFrequency` rate and resamples it into the
mix; Wine's dsound and Windows since Vista do that with a FIR filter, so a voice neither images nor aliases. The
retail samples run from 8 kHz (`Go`) and 11 kHz (the co-driver) to 48 kHz, and pitches reach 4 (the whine) and 0.3
(the gravel hits), so a nearest or linear read is audibly harsh. Rust reads every sample voice through a 16-tap
windowed sinc from precomputed polyphase tables, with the cutoff band picked by the pitch
(`rbr_sound::mixer::voices::interpolate`).

### 12.4 What "exact" can mean for sound

Sample-exact output is not a goal: DirectSound's mixer, hardware 3D and EAX differ by machine. Behaviour-exact means:

- the same triggers on the same physics step or frame (which sound, which bank, which sample index when the original
  picks it without `rand`);
- the same parameters per step or frame: rpm after the slew, load, crossfade weights, whine pitch and volume, turbo
  pitch and volume, tyre gain and pitch, impact index before the random part, pace-note phrase word lists and
  distances, countdown codes;
- the same engine renderer output for a fixed grain sequence. With a seeded grain choice, the granular synth itself
  can be compared sample by sample against a C++ or logged reference.

Random picks use an RNG separate from the physics. Matching the original's CRT `rand` stream is not possible, because
other systems share it.

### 12.5 Tests

- Unit tests for the pure models from the formulas above (rpm slew, crossfade, distance snap, phrase builder with
  `rbr_gameplay::markers` keys from retail stages).
- Retail tests (`RBR_DATA`): parse `audio.dat`, every `.eng` (`subaru.eng` fully; the four two-wave files rejected),
  every audio ini, and every WAV/Ogg header.
- Original reference (needs a harness plugin change, not done here): on cachy in the vanilla install, hook the voice
  vtable calls (create, play, volume, pitch, position) and the engine inputs (`CEngine +0x101c` rpm, `+0x1018` load,
  the cylinder flags), the speech queue inserts (`0x4dd4d0`), and `CSoundFX` codes. Log them per frame and physics
  step with the recording's step number. Then run a recorded drive back and compare the Rust command list against the
  log. Read-only watches of `CEngine +0x101c`/`+0x1018` per frame would be a cheaper first check.

## 13. Scripted events, music and the service park

All **loader** unless marked; Rust: `rbr_sound::scripted` and `rbr_sound::menu`.

- **Rescue helicopter** (`CSoundEffects::Vf08` `0x4d5e70` → `E31_5a30` `0x4d5a30`): while
  `CRescueHelicopter::IsActive`, one voice of `Audio\Ambient\heli_g.wav` (flags `0x418`), looping (`+0x50(1)`), at
  the helicopter (`g_mgr38` slot `0x20`, its body) each frame, velocity zero, gain `Helicopter` amp (0.3) times the
  effects volume. The min distance is set twice (`+0x78` with 50, then 100), so the voice is full within 100 m; the
  max is not set. Freed when the helicopter is no longer active. `E31_5800` (`0x4d5800`) plays `heli_r.wav` at the
  origin instead in a replay (app state 8) with a camera flag `& 4`; `heli_g` in its state-1 branch is unreachable.
- **Bird hit**: `CCollisionSnd::OnBirdstrike` (`0x4b2460`) copies the `CSigBirdstrike` point and calls `Vf68`
  (`0x4b2c50`): `IMPACT::VARIOUS` sample 0 (`Impact/Glass/birdhit.wav`) at the point, min 3 m, max 5 m as called (`+0x78` then `+0x74`: in 1.02 a min of 5 m), gain `Impacts`
  amp times the effects volume (`E31_4940` is 1.0), outside volume 0, pitch 1.
- **Rally incident** (`CRallyIncidentSnd::OnSignal` `0x4b7d10`, disassembly): `CSigRallyIncident` clears the speech
  queue (`E32_SpClearAll`) and stops the speech voice (`+0x10`), then `sprintf`s
  `audio\speech\RallyIncident\<lang>\<table 0x7c4790[code]>` (code 0: `snd_chopper.wav`; the other entries are 0
  or junk) and returns: the file is never played. The emit is `0x577550` (HUD code, code 0 when `0x5665f0` returns
  0); that it is the helicopter call is **inferred**.
- **Music** (`CMusic`, `Audio\Sound.ini`; `CSigMusic` at signal `+0x7328`, handler `OnAppState`): 0 front end
  (`MUSICPC::FRONTEND`, `ba.ogg`), 1 replay (`MUSICPC::REPLAY` `Music%d`, random when `Random`), 2 service park
  (`MUSICPC::SERVICEPARK`, `sp.ogg`), 3 no section (the default `Audio\Music\ba.ogg` under `audio\music\`), 4
  stops. `Vf40` (`0x4b4760`) opens the stream; the fade `+0xf4` restarts at 0 when the stream is not ready and `Vf08`
  (`0x4b4c70`) raises it by the frame time to 1; the gain is `g[0x41bc]` times the music volume (`g_1660e14`, the
  replay's `g_1660e1c`) times the fade. At the stream's end it is recreated and reopened, so the music loops.
  `MUSICPC::PRIZECERMONY` is not read.
- **Prize ceremony** (`CPrizeCermonySnd::Vf08` `0x4b7570`): in app state 0xf, once the flow state is 3, it sends
  `CSigMusic` 0 (the front-end music) and clears its flag. Nothing reads `Ambient/Prize/0[1-4].wav` (no exe string).
- **Service park** (`CServiceParkSnd`, `Audio\service.ini`, our `RBR_E26_Park.cpp`; `CSigServicePark` 1 starts,
  2 stops):
  - start: a looping `SFX::BACKGROUND` sample (`rand` pick), min 0.5 m, max 17 m, gain `Servicepark_BG` amp times
    the effects volume; counter 1, state 0, timer 0;
  - each frame (`Vf08` `0x4b98c0`) with the timer at or below 0: the counter drops; at 0 a new state
    `rand / 65536 * 5`. `M96b0` then: with the counter above 0, the state's hit plays (and the timer runs on below 0,
    so the next frame decides again); at 0 it draws a place (`(rand / 65536 + 0.5) * 32 - 24` for x and y, z -1), a
    counter `rand / 65536 * a + 1` (`a` 4, tools 1) and a 0.7 s timer, and plays nothing;
  - hits: 0 `SFX::PNEUMATIC` (min 1, gain `Servicepark_FX` times 0.8), 1 `HARDHIT` (min 5), 2 `MEDIUMHIT` (min 3,
    outside 0.3), 3 `SOFTHIT` (min 3, outside 0.3), 4 `TOOLS` (min 5, outside 0.3), all max 17 m and flag `+0x8c(1)`;
  - with the CRT's 15-bit `rand()` every `rand / 65536` is below 0.5: states 0-2 only (no soft hits, no tools), the
    counter 1 or 2 (so a draw plays its hit once or not at all) and the places within 8 m (**inferred** from the
    arithmetic). The tools could never play anyway: their counter is always 1.
- **Not in a rally**: the school's incident and speech classes (`CIncidentSnd`, `CRallySchoolSnd`).

Rust: the viewer passes the characters' events (`rbr_render::sound::events`) to `Player::events`; `rbr-sound
--events` plays a CSV that the `rbr-render` example `scripted_events` writes. The launcher plays the service park's
ambience (`rbr_sound::menu`, at the sound volume) while a championship is at a service stop. The music (`CMusic`:
front end, replay, service park) is out (user decision): ours plays none, and an older `launcher-settings.ini`'s
`music` and `music_volume` keys are ignored.

## 14. RSF cars' FMOD banks (ours)

An RSF car with an FMOD bank (`notes/formats/fmod-bank.md`) plays its own recordings (`rbr_sound::fmod`, user
decision 2026-10-18) instead of the 1.02 engine (and its reflections) and turbo voices, turns the 1.02 blow-off,
gearbox/diff whines, pops, bangs, ignition and wind down to 0.003 and moves the gear-change click out of reach as NGP
does (below); the other voices stay 1.02's. Cars without a bank keep the 1.02 sounds. NGP's FMOD path is not run.

- From the bank: the events `engine_int`/`engine_ext`, `turbo`, `transmission`, `gear_int`/`ext`, `blowoff_int`/`ext`,
  `backfire_int`/`ext`, `ignition`, `wind` and `wheel` (four instances); their `fluctuation_length`;
  each instrument's sheet regions, volume, pitch, auto-pitch and fade curves; and the bank's whole mixing tree
  (`rbr_sound::mixer::fmod_mix`): each event's tracks, its master track, its input bus, the mixer's groups
  (`bus:/internal`, `bus:/external`, `bus:/common` and one per event), the returns and sends, down to `bus:/`, each with
  its fader, its dB automations over the parameters below, and its effects before and after the fader. Automations
  on one property add in dB (gains multiply); curves are straight lines. The car banks carry their own mixer: there
  is no master bank, and none of the 57 has a VCA or a snapshot, so nothing changes between the car and outside
  but which events play.
- Ours: some parameters (`boost` the 1.02 turbo level, `Speed` and the other events' `wheel_speed` the car's speed
  in km/h, `Distance` camera to car, `Event Cone Angle` the angle between the car's back (against its velocity) and the
  camera, inferred from the banks' `rear` recordings being loud near 0; `valve` the 1.02 blow-off volume; the rest
  are the daemon's, below); a blow-off (1.02's flow rise) plays a random recording of its event; the outside events
  take the 1.02 engine voice's distance gain and the car's side; the mix level (`MIX`, 1).
- The buses' effects (`rbr_sound::mixer::dsp`), with the bank's values and their automations (`Distance`, `rpms`,
  `throttle`, ...): `LOWPASS_SIMPLE` (two one-poles), `HIGHPASS_SIMPLE` (one), `LOWPASS`/`HIGHPASS` (RBJ, Q the
  resonance), `PARAMEQ` (RBJ peaking), `THREE_EQ` (Butterworth crossovers of order 2/4/8 by the slope, bands as
  differences), `FADER`, sends, `COMPRESSOR` (peak envelope, gain worked out every 8 samples), `SFXREVERB` (dry level,
  and unless the wet is -80 dB a small reverb: two early taps and a four-line feedback delay network, the decay time,
  delays, early/late mix, high cut, low shelf and wet level), and `CONVOLUTIONREVERB`'s dry level. The filter, reverb
  and compressor shapes are ours (FMOD's are not published); a low-pass at 22 kHz is open. The buses are mono, at
  FMOD's scale (a full-scale sample is 1, which the compressors' thresholds need).
- Fiesta_Rally2 (RSF 109): the `turbo` track's `THREE_EQ` (mid -24.5 dB and high -19.5 dB, 140-9000 Hz) was the
  in-car brightness; the mixer's levels: `bus:/internal` +8.5 dB into a compressor (-10 dB, 2.5:1, 20/200 ms),
  `bus:/external` a compressor (-10 dB, 2:1), `bus:/common/turbo` -23 dB, `transmission` -12 dB, `wheel` -20 dB,
  `bus:/internal/gear_int` -16 dB, `bus:/external/engine_ext` -2 dB under the event's master at -5 dB. Lancer_Evo_IX_X
  has the same groups, with sends into returns that hold a convolution reverb (dry -80 dB) or an echo at -80 dB.
- Not modelled: the convolution reverb's wet signal (an impulse response in 77 effects of the banks, on 25 banks'
  `engine_ext` masters and on returns), echo (on returns at -80 dB), distortion (1 bank), the 3D panner and channel
  mix, modulators, curve shapes, timelines, the effect order around the fader (the `BUS`'s two effect lists are taken
  as before and after it), a paused event's playing one-shots (they play on), and the skid and `gear_grind` events
  (no instruments in the 57 banks; the daemon loads neither).
- Level: on the tanner drive (chase) the loaded segments match the 1.02 engine within a few dB (306 Maxi +8 dB);
  idle is 10-20 dB under 1.02's, as the banks' throttle automations make it.
- Against NGP 7.4 (harness, 2026-10-19: the NGP test install with RSF 109's physics and the Fiesta_Rally2 bank,
  FMOD on, the tanner drive retimed): while driving, NGP's FMOD sits 6.8 dB (in the car) and 8.8 dB (outside) above
  ours, the same as the 1.02 engine's gap before the master gain (2.6), so the bank needs no gain of its own. Ours
  is about 3-4 dB heavier below 400 Hz and above 4 kHz (indicative: the car crashes at about 21 s). During the
  countdown NGP's FMOD engine is silent; ours idles at about -30 dBFS (open, below).
- The end of NGP's FMOD chain (leftovers-5, `NgpFmodDaemon.exe` 7.4 x86): no limiter or compressor after the
  mixer groups. `bus:/` has no effects and a 0 dB fader in every bank (no master bank, no VCA, no snapshot). The
  daemon sets the core master channel group's volume to `max(AudioFMOD.ini masterVolume, 0)` times its current
  volume (0x409d4d; `masterVolume` is 1 for every slot on RSF's install) and adds no DSP. It initialises Studio with
  256 channels and core flags 0x20000 (`FMOD_INIT_VOL0_BECOMES_VIRTUAL`, `vol0virtualvol` 0.001 in the advanced
  settings), so without `FMOD_INIT_CLIP_OUTPUT`: its float output goes to the device unclipped as a second stream next
  to 1.02's DirectSound one, and the two are summed and clipped by the system mixer. So the original clips the FMOD
  cars as it clips 1.02 (2.6); Rust keeps the one master gain and soft limiter over the whole mix. On the tanner
  drive (30 s from 10 s) 109 has 2.2% (in the car) and 1.7% (chase) of samples past 1 before the limiter, 95 6.1% and
  4.9%, and the retail COROLLA 3.8% and 5.6%.
- The backfire, ignition and wind events (Sound-B, read 2026-10-19 in `NgpFmodDaemon.exe` 7.4 x86 and `NGP.dll`
  7.4.778; Rust `rbr_sound::fmod`):
  - **The block.** `NGP.dll`'s hook of the `EngineNoise` handler (0x100ee010) copies the 12-word payload and calls
    `FUN_100b3470`, which in app states 1 and 8 fills the shared block (`NGP.dll` object `+0x3c`, block offset =
    object offset - 0x3c) and sends it as command 5: `+0x8` the in-car flag (master `+0x106ec`, `g[0x41bb]`), `+0xc`
    throttle `p[1]`, `+0x10` brake (car `+0x860`), `+0x14` gear (`dri +0x54c`), `+0x18` engine stopped
    (`CCarSound +0x41924`), `+0x1c` rpm `min(p[0] / 0.10472, car def +0x3d8 * 1.02)` (0 under 1), `+0x20` valve
    (`CCarSound +0x41868` while `+0x41864`, 0 under 0.1), `+0x24` boost `min(p[3] / 3, 1)`, `+0x28` misfire `p[4]`,
    `+0x2c` the starter word (`CCarSound +0x418f0`, which is drivetrain `p[9]`: the `DrivetrainState` handler
    copies the payload to `+0x418cc` at 0x4ae616), `+0x30` the transmission's `wheel_speed` `|dri +0x384 * def
    +0x24c * 3.6|`, `+0x34` `car_speed` `|car +0x1c0| * 3.6`, `+0x38..+0x44` the four wheels' speeds, `+0x48` the
    pause word (0 here; command 4 sets it in app states 2 and 9, the pause menus). Speeds under 1 km/h are 0.
  - **The daemon's parameters** (`FUN_004092f0`): `exhaustpop` = `+0x28` from 0.9 up (0x4098d6), else 0, with
    `throttle`, to `backfire_int`/`_ext`; `startup` 1 with `battery` = `+0x2c` while `+0x18` is set and `+0x2c > 0`,
    else `startup` 0 (`battery` keeps its last value), to `ignition`; `car_speed` = `+0x34` to `wind`; `valve`,
    `rpms`, `throttle`, `boost`, `wheel_speed`, `brake` to the others. `gear` is the gear - 1 and `state` a random
    0.6-1.0 on a shift up or 0.1-0.5 down, to `gear_int`/`_ext`; each shift also sets `fluctuation` to 1 on the engine
    events and `transmission`, which falls to 0 over the event's `fluctuation_length` user property (seconds, read
    by `FUN_00409ab0`); a shift waits while a fluctuation still runs. Ported in Sound-C (below).
  - **The pauses** (`FUN_00408ef0`, each update): the `_int` events outside the car and the `_ext` ones in it; the
    wind outside the car; `engine_*`, `turbo`, `blowoff_*` and `backfire_*` while `+0x18` (engine stopped) or the
    pause word is set; the rest only on the pause word. Ported: a paused loop is silent and a paused one-shot event
    neither starts nor tracks its parameters.
  - **The one-shots.** A backfire or ignition instrument starts as its parameters enter its regions (FMOD's
    parameter-sheet triggering) and plays to its end (ours: the async flag is not read; its gain and pitch are taken
    at the start), one recording of its playlist at random. 306_maxi_B's backfire sits on `throttle` 0-0.86 and
    `exhaustpop` 0-0.985, so it pops on each lift below 0.86 (gain 0 at a closed throttle); the Audi Quattro banks'
    on `exhaustpop` 0.9-1. Fiesta_Rally2 has the backfire and blow-off events with no instruments.
  - **What the events stand in for.** The daemon's load flags (`FUN_00409ab0`: the event exists and its sample data
    loads) come back to `NGP.dll` (`+0x2cc` engine_int ... `+0x2f8` wind, in the daemon's log order). On app state
    10 `NGP.dll` (0x100b32cc-0x100b32fd) sets these 1.02 amps to 0.003: `Pop` and `ExhaustPop` with `backfire_int`,
    `Bang` with `backfire_ext`, `Ignition`, `BlowOffValve` with `blowoff_int`, `EngineTransmission` and
    `TransmissionDiff` with `transmission`, `Wind` with `wind`, and its own `0x10173214` with `gear_int`. Rust
    (`FmodCar::mute`) turns those voices down to 0.003 the same way, whether or not the event has instruments (they
    still count under the sound limit): Fiesta_Rally2's empty `blowoff_int` turns the 1.02 valve down (it was -41.2
    dBFS of the in-car mix on the tanner drive, -50.9 in chase); its `transmission` has instruments, so its whines,
    removed before, now stay as voices at 0.003 (-77.8 dBFS in the car). The FMOD layer is unchanged.
  - Levels (tanner drive under NGP, 30 s from 10 s, `lead/reports/sound-b.md`): RSF 109 (Fiesta_Rally2) gains the
    wind at -72 dBFS in the car (the bank's wind is -80 dB at 10 km/h and -8 dB at 120 km/h, the drive peaks near
    80 km/h) and loses the 1.02 pops (-37.5 dBFS of the mix); RSF 95 (306 Maxi, 306_maxi_B) gains its backfire, -28.7
    dBFS in the car and -22.5 dBFS outside. The 1.02 retail renders are byte-identical.
- **Gear, fluctuation, wheels, followed one-shots, the orbit's start (Sound-C, read 2026-10-19 in
  `NgpFmodDaemon.exe` 7.4 x86 and `NGP.dll` 7.4.778; Rust `rbr_sound::fmod`, the FMB1 record
  `rbr_recording::sound::FmodBlock`).**
  - **The daemon's event slots** (11 words each from `+0x28`, `FUN_004092f0`): instance, then `fluctuation` (`+4`),
    `fluctuation_length` (`+8`) and the paused flag (`+0xc`): `engine_int` +0x28, `engine_ext` +0x54, `backfire_int`
    +0x80, `backfire_ext` +0xac, `gear_int` +0xd8, `ignition` +0x104, `blowoff_int` +0x130, `gear_ext` +0x15c,
    `blowoff_ext` +0x188, `transmission` +0x1b4, `turbo` +0x1e0, `wind` +0x20c, the four `wheel` instances +0x238,
    +0x264, +0x290, +0x2bc; the last gear at +0x2cc, the last update's time (QueryPerformanceCounter, µs) at +0x2d0.
    Which parameters go where: the engines `rpms` and `throttle`; the turbo `throttle` and `boost`; the transmission
    `throttle`, `wheel_speed` (block +0x30) and `fluctuation`; each wheel `wheel_speed` (block +0x38 + 4k) and `brake`
    (+0x10); the wind `car_speed`; the backfires `throttle` and `exhaustpop`; the ignition `startup`/`battery`; the
    blow-offs `valve`; the gears `gear` and `state`.
  - **`fluctuation_length`** (`FUN_00409ab0`): `getUserProperty("fluctuation_length")`, the value's first word kept as
    a float (0 when missing). Every RSF bank has it on `engine_int`, `engine_ext` and `transmission` (0-1.5 s; in the
    `EVTB` body, `notes/formats/fmod-bank.md`).
  - **A shift** (0x40950d-0x409864): with the block's gear (`dri +0x54c`) above the last, base 0.6, below it 0.1. If a
    `fluctuation` of the three is above 0, the shift waits: the three are halved, `state` is 0 and the old gear is
    sent. Else the last gear takes the new one and `state` = `rand() % 100 / 100 * 0.4 + base`; each `fluctuation`
    whose length is above 0 goes to 1. Without a shift each falls by `dt / length` (dt the time since the last update),
    to 0 (0 at once without a length). `gear` = the sent gear - 1 and `state` go to `gear_int`/`_ext`; `state` is 0 on
    every update but a shift's. Command 2 sets the last gear to 1 (neutral) and `gear`, `state` to 0. So a gear
    instrument plays as `state` enters its region, once per shift. Fiesta_Rally2's `gear_int` has one instrument per
    gear and direction (`gear -1..0` up to `6..7`, `state 0.5..1` up and `0.085..0.485` down; "Neutral to first" at
    `1..2` up); a down `state` of 0.489-0.496 (`rand() % 100` of 97-99) lands in neither. The regions follow on from
    each other, so a `gear` region is taken as holding its start, not its end (ours; FMOD's rule is not traced).
    Rust: `FmodCar::shift` with the step's 7 ms, on each step NGP sends its block.
  - **NGP's own gear-change words.** With `gear_int` loaded (`+0x2dc`), app states 1, 8 and 10 set `0x10173210` to 1
    and put `0x10173214` among the muted words (0.003); without it they are 0.001 and 0.9 (0x100b32a0-0x100b32c5).
    `NGP.dll`'s patch table writes their addresses into the exe at 0x4ae62d (the `DrivetrainState` handler's
    `comiss` with 0x795e94, the gear-change flag's threshold) and 0x4d34e7 (`CDXTransmission`'s click volume 0.9,
    in 0x4d34a0) (0x100ffc5b, 0x100ffcd8). So with `gear_int` the 1.02 click needs a flag above 1 and plays at 0.003.
    Rust: `FmodCar::gear_click` sets `car::Car::gear_click`; the click is no longer removed outright.
  - **The wheels.** Command 2 starts the `wheel` event four times; the block's wheel speeds are `|wheel +0xaa0 * wheel
    +0xa50 * 3.6|` (0 under 1 km/h) for the wheels at car `+0x470..+0x47c`, `brake` the car's `+0x860` (0-1 in steps
    of 1/255 on the tanner drive: the brake input), the transmission's speed `|dri +0x384 * (car +0x34) +0x24c * 3.6|`.
    These are the FMB1 record (`rbr-standalone --live`, before SCN1: `i32 gear, f32 brake, transmission speed, four
    wheel speeds`, m/s). The wheel events pause only on the pause word. Rust: four slots per instrument, each wheel's
    tracks (the event's unnamed buses: tracks, master, input) copied so their automations take its own speed.
    Fiesta_Rally2's `wheel`: `Fiestabrake` on `brake 0..1` (track -80 dB at 0 to 0 dB at 1, and over `wheel_speed`
    -80 dB at 0 to 0 dB at 10 km/h) and `tyre_rolling` on `wheel_speed 0..300` (pitch -3 to +6 semitones over
    50-300 km/h, track -80 dB at 15 to 0 dB at 275 km/h), into `bus:/common/wheel` at -20 dB.
  - **One-shots follow their automations** (ours, standing for FMOD updating a playing instrument): each step every
    one-shot slot not paused sends its gain and pitch from its automations (regions ignored) when they change
    (`Command::FmodTrack`), and the mixer sets them on that slot's playing shots. A paused event's shots play on.
  - **The orbit's first 0.2 s.** Command 2 starts every event unpaused (the paused flags of the engines and gears
    cleared) with `rpms` = `IdleRPM_NGP` (`*0x7ecd04 + 0x13c0`, an integer; NSD1's fourth word) and nothing else set.
    Rust: while NGP does not send its block and the watchdog has not fired, every event plays, with `rpms` the idle
    (the live rpm without NSD1), the other parameters 0, nothing placed (no 3D attributes).
  - Levels (tanner drive under NGP, 20 s from 10 s, `lead/reports/sound-c.md`): RSF 109 (Fiesta_Rally2) FMOD layer
    -21.9 dBFS in the car (unchanged), -28.0 to -28.1 in chase; what each part adds (RMS of the difference with it
    off): the gear shots -36.9 in the car (two shifts), -36.4 in chase (`gear_ext`); `fluctuation` -42.4 / -36.7; the
    transmission's own speed -44.2 in the car; the wheels -56.7. RSF 95 (306_maxi_B): the gear shots -25.9 / -19.4, the
    wheels -31.8, `fluctuation` -30.9 / -24.4, the followed backfires -48.9 / -36.5. The orbit's first 0.2 s: -18.7 to
    -17.8 dBFS in the car, -29.7 to -17.8 in chase. The 1.02 retail renders are byte-identical.
- How NGP silences the engine (`NgpFmodDaemon.exe` x86 and `NGP.dll`, read 2026-10-19): `NGP.dll` `FUN_100b3470`
  copies sound state into a 0x2d0-byte shared block each frame in app states 1 and 8; the daemon's update
  (`FUN_004092f0`) sets the parameters from it and `FUN_00408ef0` pauses events. The engine events (`engine_int`
  at daemon `+0x28`, `engine_ext` at `+0x54`, by the event load at 0x407a74) are paused while block `+0x18` is
  non-zero or the block's pause word `+0x48` is set (also: the `_int` one outside the car, the `_ext` one in it).
  Block `+0x18` is 1.02's `CCarSound` engine-stopped flag (m_data `+0x41750`): `CCarSound::OnX` (0x4b1810) sets it,
  and `M6a0` sets it while the `EngineNoise` rpm (`p[0] * 9.5493`) is under 500 and clears it at 500 or more. With it
  set and block `+0x2c` (m_data `+0x4171c`) above 0 the daemon sets `startup` 1, which plays `event:/ignition`
  (region `startup` 1, `battery` 0.009-1). So NGP's engine is silent while the game holds the engine stopped. That is
  not the countdown's silence: the harness (`RBR_POPS`, NGP 7.4 idle on Tanner with RSF 109 and the Fiesta bank,
  2026-10-19) saw the flag 1 only for three loading frames (app state 5 to 13) and 0 through the countdown and the
  race, with `EngineNoise` emitted all along. Rust now pauses the engine, turbo, blow-off and backfire events while
  the engine is stopped, as the daemon does.
- **The countdown's silence, settled from the code (Sound-B).** The idle Sound-A heard in NGP's countdown is the
  start orbit, app state 10 (`notes/gameplay-loop.md`, `notes/cameras.md` 7a: the HUD's 7 s countdown starts in
  state 1, after the camera's go; ours shows the same, the HUD countdown holds 7.0 for the 376 orbit steps and the
  engine idles at 1006 rpm). On entering state 10 `NGP.dll` sends command 2 (0x100b3310-0x100b3353), and the daemon
  starts every event (0x408927-0x4089fb) with `rpms` the car's `IdleRPM_NGP` (`+0x13c0` of NGP's car object), `gear`
  and `state` 0. Then nothing: `FUN_100b3470` sends the block only in app states 1 and 8. The daemon's loop waits
  for a command with a 200 ms timeout (`WaitForMultipleObjects` at 0x40888d and 0x408e8b); on a timeout after a
  command it calls `FUN_00408ef0` with the pause set (0x408e46-0x408e6a), which pauses every event. So NGP's engine
  plays the first 0.2 s of the orbit and is silent for the rest, until state 1's command 4 (pause word 0) and the
  first command 5. Rust (`FmodCar::step`'s `sent`): the block counts as sent unless the HUD countdown is still at
  its 7.0 (the orbit), and 0.2 s without it pauses every event. RSF 109 on the tanner drive: the orbit (0.3-2.5 s)
  goes from -21.7 dBFS (in the car) and -32.0 (chase) to silence; the first 0.2 s and the countdown in state 1 are
  unchanged (`lead/reports/sound-b/109-countdown-*.wav`). The same timeout silences the FMOD sound whenever the game
  stops sending (pause menus already pause it, states 2 and 9).

## Open

- **Mixer selection.** Which of DX, Prologic or EAX runs, and from which option or caps value. Also the mapping of
  resampler mode 2 to the cubic, linear or nearest code.
- **Countdown alignment.** The HUD's 7.0 s timer (frame time) against the gameplay start in `rbr_gameplay::stage`:
  which one seeds which, and whether "go" can drift from the real start at low frame rates. The Rust race clock is
  the HUD timer; why the boot's starts below 0 is open (section 9).
- **Frame finish.** The standalone does not run `CWheel::Vf28` (`0x53f230`), so the damper words and the tyre's
  `sig[1]` accumulate from boot; Rust rebuilds the damper's per-frame words on a 60 Hz frame (6.3) and folds
  `sig[1]` per frame (6.1, below).
- **Splash.** No drive tried has a non-zero `CSigTireParticles` +0x98; the wetness voice is unheard. Rain does not
  change that (Sound-C: tanner, `drive-tarmac-lancer` and `drive-v1-gravel-impreza03` with weather preset 2, wipers
  running, 60 s each: wetness 0 on every step), so the splash comes from the ground (water areas), not the weather;
  what writes +0x98 is not traced.
- **FMOD daemon, left.** FMOD's region rule (ends held or not), the instruments' async and loop flags, a paused
  event's playing one-shots (FMOD pauses them; ours play on), the parameters' default values (the orbit's 0.2 s
  takes 0), the 3D attributes, and an A/B of the gear, wheel and fluctuation sounds against NGP's FMOD output.
- **Effects that never fire here.** The ignition, the dog rings and the cooler are built (5.4) but none fires in the
  drives tried; a game run should show whether 1.02 cranks at the start, what `EngineNoise p[5]` is, and when
  `CDrive +0x1b4` is non-zero.
- **Camera dirt.** `NParticleSystem2D::CParticleEffectCameraDirt` (vtable `0x74f06c`, spawn `vf08` `0x5f23f0`) gets
  its rate from `E87EffectHost::SetupAmt` (`0x5f7040`) on `CSigTireParticles`: with precipitation (`host +0x38`),
  the host's mode 1 (`SetupMode` value 7, inferred the `.dls` TV camera) and mass `+0x88 >= 1`, `f = -dot(camera
  dir, unit velocity)` above 0 and the payload within 5 m of the camera, `rate = lerp(+0x34, +0x38, 1 - (mass - 1)
  * (5 - d) * f / 245)`, else 0. The viewer has no such camera; not ported.
- **Tyre `sig[1]`.** Ported (leftovers-1). `FillOutputs` (0x532f10) keeps the running max (`v > max`) of `(tread
  depth +0x8c + carcass radius +0x7c - c +0x3a8 - state +0x48) / dt` per `CScratch::Step`, the no-contact path
  clears the whole block, and `CWheel::Vf28` sends it (`EmitNoise` 0x535890) and zeroes it after each frame. The
  state's +0x48 changes again after `Step` within the physics step (`SetState48` 0x533490), so the term is read
  right after each `Step` (and NGP's wrapper 0x1005b660) by the pure host (`Pure::take_tyre_terms`) and folded per
  frame from 0 (`rbr_recording::sound::TyreMax`) into SND1's tyre word 1. Checked on tanner to the finish: wherever
  the term or the memory's word rises above the memory's last value they are bit-equal (477 times), and ours is
  never above the memory's. It adds up to `sig[1]` to the rumble level below 10 m/s.
- **Speech voice idle flag.** `E31Speech +0x95` is read as the voice being idle (10.3); not checked.
- **`.eng` trailing tables.** Meaning of the `a`/`b` tables (only `Saxo.eng`, unused).
- **`EngineNoise` `p[5]` and `p[7]`.** Their sources and how sound uses them beyond the gain noted in 4.1.
- **`Incident.ini`.** No exe string names it, and its calls (`sendheli`, `w_way1`, `w_way2`) belong to the rally
  school's `CIncidentSnd` (`CSigSchoolIncident`, emitted by `CReplayManager::SchoolIncident` 0x498e10), out of scope.
- **`CServiceParkSnd` `+0x8c` flag and `Vf44`.** What `S8c(1)` sets (head-relative placement is inferred) and what
  `CSigServicePark` 0 (`Vf44`) does.
- **Min and max distance.** Settled (2.4): `E30DXSound`'s `+0x74` (0x4d2940) and `+0x78` (0x4d2970) both call
  `SetMinDistance` in 1.02, and every voice's distances come from its own calls after creation; the `SOUNDDEF`
  min and max of the transmission, tyre, turbo and blow-off sets are never read. Open: whether a real 1.02 install on
  current Windows takes the tyres' software path (the caps value is inferred from DirectSound's behaviour without
  hardware mixing), by ear or with the harness.
- **Live runs.** The harness (`harness/README.md`: `RBR_CAMERA`, `RBR_VOLUMES`, `RBR_POPS`) captured the original
  on the tanner drive on cachy (Wine); the A/B per volume group is in `lead/reports/sound-a.md`. Wine's DirectSound
  pans a 3D voice by constant power (a centred voice -3 dB, `dlls/dsound/sound3d.c`), a 2D voice (the speech) at
  unity: part of the co-driver's level against the 3D car voices there. Rust does not model it (a plain balance).
- **Sound limit's room.** Rust's count takes 9 always-counted sounds where the live list has 11 (2.5): 9 matches the
  original's made counts at 60 fps for every caller; what frees the two places is open.

## Relevance

Presentation only. Sound reads per-step physics payloads, frame state and stage markers and writes none of them. The
one shared resource is the CRT `rand()` stream, which the physics does not use. A Rust port needs the engine and
drivetrain payloads that the physics already computes, the tyre noise block, the pace callouts that
`rbr_gameplay::markers` already produces, and later the damper, collision and camera inputs.
