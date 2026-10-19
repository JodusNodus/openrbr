# Audio data (`Root/audio.dat`, `audio.ini` and the audio inis)

Where the 1.02 game's sounds come from and what each sound ini controls. Addresses are `RichardBurnsRally_SSE.exe`
1.02. Labels: **loader** = read from the decompile or our reconstruction of the named function; **file** = checked
on retail data (counts say over which files); **inferred** = not proven. The engine banks have their own note:
[eng.md](eng.md). How the sounds are triggered: [../sound.md](../sound.md). The ambient bed, birds and siren:
[../marks-sound-ghost.md](../marks-sound-ghost.md) section 2.

## `audio.dat`

`Root/audio.dat` (retail 1.0 install, 88,305,193 bytes) is a `CHELALIC` archive with the layout `rbr_formats::dat`
reads (`rust/rbr-formats/src/dat.rs`; checked here with a throwaway Python reader of the same layout, **file**). It
holds 3,512 entries under one root, `C:/Projects/Rally_7/TargetMedia_Develop/Pc/Debug/World/Audio/`. The game opens
files as `Audio\...`; the part up to `Debug/World/` is dropped. The 1.02 patch ships no audio files.

`audio.ini` says (comment at its end) that an extracted `Audio` folder next to the exe replaces `audio.dat` once the
archive is renamed (**file**; the loader side is not traced).

Entry counts by extension (**file**):

| Extension | Count | Notes |
|---|---|---|
| `.ogg` | 3,154 | speech and music |
| `.wav` | 235 | effects, surfaces, impacts, countdown, ambient |
| `.scc` | 69 | `vssver.scc`, SourceSafe leftovers |
| `.vag` | 21 | `Impact/Grus/*.VAG`, PS2 leftovers |
| `.ini` | 13 | listed below |
| `.sfk` | 10 | Sound Forge peak files |
| `.eng` | 8 | engine banks ([eng.md](eng.md)) |
| `.bak` | 2 | `Cars/Effects.ini.bak`, `Impact/Sound.ini.bak` |

Directory tree (gameplay parts; `Frontend` and `Season` are menu sounds):

| Directory | Contents |
|---|---|
| `Cars/` | 8 `.eng`, `Cars.ini`, `Effects.ini` |
| `Cars/Effects/` | `Bang/01-06`, `BlowOffValve`, `BlowOffValve2`, `cooler`, `Dogring/1-8`, `DriveShaft/1-13`, `ex_grav`, `ex_tarm`, `Fire`, `gearchng`, `gearchn2`, `gearchn3`, `ignition`, `pop1-11`, `PopsNew/1-11`, `rumble`, `Suspen/01-02`, `SuspSqee/01-18`, `trans`, `turbo`, `wipereng`, `wiperdwn`, `wiperup` |
| `Surfaces/Normal/`, `Surfaces/Skid/` | `generic`, `gravel`, `snow`, `water` (normal); `generic`, `gravel`, `snow`, `tarm` (skid) |
| `Impact/` | `Addon` (`bend`, `skrynkel`), `Branch/0-2`, `Glass/0-1` and `birdhit`, `Grus/0-20`, `Heavy/0-16`, `Medium/0-9`, `Offroad/01-07`, `Ranged/01-20`, `Shatter/0-1`, `Soft/0`, `SoftMet/0-4`, `Speech` (`oh_no`, `ugh`, `ughhard`, `watch`), `Sound.ini` |
| `Game/` | `Three`, `Two`, `One`, `Go` |
| `Ambient/` | `crowd`, `heli`, `heli_g`, `heli_r`, `siren`, `wind`, `Europe/bird01-06` and `crow`, `Prize/01-04`, `europe.ini` |
| `Speech/PaceNote/` (101), `Speech/Number/` (110) | English co-driver: pace-note words and numbers, several takes |
| `Speech/<Ger,Fre,Spa,Pol,Ita,Czh>/` (about 45 each), `Speech/Num<lang>/` (61 each) | the other co-drivers, one take |
| `Speech/Broken/<lang>/` | 8 damage calls: `batterys_flat`, `gearbox_broke`, `problem_engine`, `steering_broke`, `sus_broke`, `water_temp`, `weve_got_fire`, `wv_lost_brakes` |
| `Speech/Incident/`, `Speech/RallyIncident/<lang>/` | `sendheli`, `w_way1`, `w_way2`; `snd_chopper.wav` |
| `RS/<7 languages>/` (about 325 each) | rally school speech, `Audio\RS\%s\%d.ogg` |
| `Service/` | service park: `bg`, `pneu`, `badaboom`, `medium`, `soft`, `flerp` |
| `Music/` | `ba`, `Creep`, `cy`, `go`, `gt`, `sp` |

## Sample formats (**file**, all entries)

- **WAV** (235): all PCM, 16-bit, mono, `fmt ` as the first chunk. Rates: 8000 Hz x4, 11025 x7, 22050 x65, 27000 x2,
  44100 x157. The RIFF reader (`E32Voice::WaveParse` `0x4dd740`) requires `fmt ` right after `WAVE`, skips other
  chunks up to `data`, and assumes 16-bit samples later (`WaveFloat` `0x4dd8e0` converts `int16`) (**loader**).
- **Ogg Vorbis** (3,154): mono 8000 Hz x469, mono 11025 x2673, mono 44100 x5, stereo 44100 x2 (music). libvorbis and
  libogg are linked into the exe (`COgg`, `CDXOgg`). Five music files (`ba`, `Creep`, `cy`, `go`, `gt`) start with a
  132-byte `INFO` block holding the title before `OggS`; `sp.ogg` starts with `OggS`. A plain Ogg reader must skip
  to the first `OggS`.
- Ini file names in the archive omit the extension; the code adds `.wav` or `.ogg` (**loader**, e.g. `Audio\RS\%s\%d.ogg`,
  `audio\speech\broken\%s\%s.ogg`).

## `audio.ini` (patch 1.02, next to the exe)

Read by `E31Master::M4a70` (`0x4d4a70`) after `audio\ingame.ini` (**loader**). A missing key reads as 0, so a
missing `audio.ini` would mute every amp.

- `[AMP::SOUNDS]`: 28 volume multipliers, stored as floats from master `+0x10708` (`g_8d5338[0x41c2]` onwards) in
  this order: `Engine` 1.0, `EngineTransmission` 0.35, `TransmissionDiff` 0.35, `Turbo` 0.2, `BlowOffValve` 1.0,
  `DriveShaftBroken` 1.0, `GravelParticles` 1.0, `Squeeks` 1.0, `Suspension` 1.0, `Horn` 0.5, `Helicopter` 0.3, `Bird`
  0.25, `Wind` 0.5, `EnviormentalReflections` 1.0, `Offtrack` 1.0, `Pop` 0.5, `ExhaustPop` 0.5, `Bang` 0.5, `Ignition`
  1.0, `Enginefire` 1.0, `BrokenCooler` 1.0, `WiperEngine` 0.25, `Wiper` 0.25, `Impacts` 1.0, `ImpactScreams` 0.5,
  `Servicepark_BG` 1.0, `Servicepark_FX` 1.0, `Rain` 1.0 (**file**). The tyre voices use the `GravelParticles` amp
  (`g_8d5338[0x41c8]` in `E31Tire::M7330` `0x4d7330`, **loader**).
- `[OPTIONS::SOUNDS]`: `NumberChannels=0` (all hardware channels; passed to the backend, vtable `+0x78`),
  `EnableEQOnCars=false`.
- `EQ_NOLOAD::CARn` and `EQ_FULLLOAD::CARn`, n = 1..8, ten bands 31 Hz to 16 kHz, read only when `EnableEQOnCars` is
  set. The gain is interpolated by engine load. Only `CAR6` ("Subaru '03") has non-zero values. The EQ numbering
  starts at 1 (`CAR1` is the Citroen), unlike `Cars.ini` (`Car0`-`Car7`).

## `Audio/Ingame.ini` (`SOUNDDEF::*`)

Fifteen sections of four floats, `Volume`, `OutsideVolume`, `MinDistance`, `MaxDistance` (read by `E31Master::M48e0`
`0x4d48e0`; stored 16 bytes apart from master `+0x105dc`, **loader**). The voices take `Volume` into the gain,
`OutsideVolume` through voice vtable `+0x64`, `MinDistance` through `+0x78` and `MaxDistance` through `+0x74`
(`CCollisionSnd::Vf64` `0x4b2ed0`, **loader**).

| Index | Section | Volume | Min / max distance | Used by (**loader**) |
|---|---|---|---|---|
| 0 | `EXHAUST` | 1.0 | 40 / 100 | engine (`CCarSound::CalcVol` `0x4ac8c0`) |
| 1 | `EXHAUSTPOP` | 1.0 | 20 / 50 | pops and bangs (`Mf180`, `Mf280`, `Mf3f0`) |
| 2 | `TRANSMISSION` | 1.0 | 5 / 20 | gear whine (`CalcVol`) |
| 3 | `TIRE` | 1.0 | 10 / 40 | tyres (`CalcVol`, `E31Tire::M7330`) |
| 4 | `TURBO` | 1.0 | 10 / 50 | none found |
| 5 | `BLOWOFFVALVE` | 1.0 | 10 / 50 | turbo and blow-off (`CalcVol`, `E31Turbo::M3cb0`) |
| 6 | `GRAVEL` | 1.0 | 3 / 10 | none found |
| 7 | `SOFTMETAL` | 0.0 | 10 / 30 | none found |
| 8 | `SHATTER` | 0.0 | 10 / 50 | `CCollisionSnd::Vf58` (`0x4b2df0`) |
| 9 | `GLASS` | 0.0 | 10 / 30 | `CCollisionSnd::Vf54` (`0x4b2d10`) |
| 10 | `BRANCH` | 0.0 | 10 / 30 | none found |
| 11-13 | `SOFTHIT`, `MEDIUMHIT`, `HARDHIT` | 0.0 | 1/2, 10/30, 3/50 | none found (their callers, `CCollisionSnd` vf44-vf4c, are empty) |
| 14 | `RANGED` | 1.0 | 3 / 50 | `CCollisionSnd::Vf64` (`0x4b2ed0`) |

`OutsideVolume` is 0.0 in every section (**file**). With `Volume` 0, the shatter and glass sounds are silent in
retail 1.02: of the impacts, only the ranged set is heard (**loader** plus **file**).

## `Audio/Cars/Cars.ini`

Read in `CCarSound::OnX` (`0x4b1810`) when the car is set up (**loader**):

- `[CARS] InsideCutFrequency=750.0` (code default 300): the low-pass cutoff on the engine when the camera is in the
  car.
- `CarN=subaru` for N = 0..7: the `.eng` bank name. All eight are `subaru` (**file**).
- `CarN_LowShelfAmp` (2.0; `Car1` 1.0) and `CarN_LowShelfFrequency` (200; code default 300): a low-shelf filter on
  the engine, stored at `CCarSound +0x41930`/`+0x41934` and passed to `E32Flt::SetCoef` from `E30Engine::Vf08`
  (`0x4cebc0`).

## `Audio/Cars/Effects.ini`

Bank lists, `Key = path` without extension (**file**). Sections and what they feed:

| Section | Keys | Sound |
|---|---|---|
| `CAR::EXHAUST` | `Pop1`-`Pop11` | old pops (`pop1-11`) |
| `CAR0::EXHAUSTNEW` .. `CAR7::EXHAUSTNEW` | `Pop1`-`Pop11` | per-car pops (`PopsNew/1-11` for all cars; `LoadPops` `0x4b16c0`) |
| `CAR::SUSPENSION` | `Sus1`-`Sus4` | suspension bottoming (`Suspen/01-02`, repeated) |
| `CAR::BANGS` | `bang1`-`bang6` | misfire bangs |
| `CAR::SQUEEKS` | `Sqe1`-`Sqe18` | suspension squeaks |
| `CAR::DOGRINGS` | `DR1`-`DR8` | dog-ring clunks |
| `CAR::DRIVESHAFT` | `DS1`-`DS13` | broken driveshaft clunks |
| `CAR::TRANSMISSION` | `Transmission`, `GearChange1`-`3` | gear whine loop and gear-change clicks |
| `CAR::IGNITION` | `Ignition1`-`3` | engine start (same file three times) |
| `CAR::WIPER` | `WiperEngine`, `WiperDown`, `WiperUp` | wipers |
| `CAR::RUMBLE` | `Rumble` | |
| `CAR::OTHER` | `Valve`, `Turbo`, `Offroad1`-`7` | blow-off valve, turbo whistle, off-road rattles |
| `CAR::EFFECTS` | `Effect1`-`4` | `Fire`, `cooler` (broken radiator), `ex_tarm`, `ex_grav` |
| `TIRE::SURFACES` | `Normal`/`Skid` x `Gravel`, `Tarmac`, `Snow`, `Water` | tyre loops (`SkidWater` points at `Normal/Water`) |

## `Audio/Impact/Sound.ini`

Sections `IMPACT::ADDON` (1), `BRANCH` (3), `GLASS` (2), `VARIOUS` (1, `birdhit`), `GRUS` (21), `HEAVY` (17),
`MEDIUM` (10), `SHATTER` (2), `RANGED` (20), `SOFT` (1), `SOFTMET` (5), `SPEECH` (2 live: `ugh`, `ughhard`; `oh_no`
and `watch` are commented out). Keys are `Snd0..SndN-1` (**file**). `RANGED` is ordered by strength: the code picks
the index from the impact power (see [../sound.md](../sound.md) section 7).

## Speech inis

All four use `[GROUP::NAME]` sections with `Sounds=N` and `Snd0..SndN-1` file names (**file**):

| File | Used for (**loader**, `CPaceNoteHandler::M7170` `0x4b7170`) | Sections |
|---|---|---|
| `CoDriver.ini` | English pace notes, `Speech\PaceNote\%s` | 41 `PACENOTE::*`, 1-3 takes each |
| `Numbers.ini` | English numbers, `Speech\Number\%s` | `PLACE::1-16`, `PLACE::LOSTPLACES`, `NUMBER::NUMBERS` (15: `p1_`..), `RANGE::30` .. `RANGE::1000` (14) |
| `CoOther.ini` | other languages, `Audio\Speech\%s\%s` | 41 `PACENOTE::*`, 1 take each |
| `Numother.ini` | other languages, `Audio\Speech\Num%s\%s` | as `Numbers.ini` |

`Incident.ini` lists the incident calls (`CarDamage`: `sendheli`; `OffTrack`: `w_way1`, `w_way2`) and four speed
groups that name rally school clips by number (`VeryLowSpeed` 227, `LowSpeed` 224, `HighSpeed` 225 and 219,
`VeryHighSpeed` 311). No exe string names it; its calls belong to the rally school ([../sound.md](../sound.md) Open).

## Other inis

- `Frontend.ini`: `[SFX::MENU]` (menu, out of scope) and `[SFX::INGAME]` with `c3`, `c2`, `c1` (`Game/Three`, `Two`,
  `One`) and `GO`. Only `GO` is read in 1.02; the countdown numbers are spoken by the co-driver instead
  ([../sound.md](../sound.md) section 9).
- `Sound.ini`: `MUSICPC::FRONTEND` (`ba.ogg`), `SERVICEPARK` (`sp.ogg`), `PRIZECERMONY` (not read by 1.02), `REPLAY`
  (`Music00`-`Music80`, random) ([../sound.md](../sound.md) section 13).
- `Service.ini`: the service park's `SFX::BACKGROUND`, `PNEUMATIC`, `HARDHIT`, `MEDIUMHIT`, `SOFTHIT`, `TOOLS`, one
  sample each (`Sounds`, `Random`, `snd0`) ([../sound.md](../sound.md) section 13).
- `Ambient/europe.ini`: `[AMBIENTFX]` `wind` and `bird1`-`bird7` ([../marks-sound-ghost.md](../marks-sound-ghost.md)).
