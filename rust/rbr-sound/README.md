# rbr-sound

The gameplay sound of Richard Burns Rally 1.02, from the traced behaviour in `notes/sound.md` (sections 4 to 10 and 13) and
the retail data (`notes/formats/audio.md`, `notes/formats/eng.md`). Two layers:

- **Models** (pure, unit-tested): `engine` (the granular `CEngine`), `car` (the per-step gains and pitches), `tyre`
  (the surface loops, rumble, wetness, gravel hits and the off-road rattles), `suspension` (squeaks, knocks and
  bottoming), `driveshaft` (the broken driveshaft's clunks), `exhaust` (pops, bangs, pop runs and the engine fire), `effects` (the ignition, dog rings, broken cooler
  and wipers), `reflections` (the engine's two delayed reflections), `impact` (the ranged impacts and the scream), `codriver` (the
  pace-note phrases, the countdown and the damage calls), `world` (the wind, birds, siren and rain, and where the
  car is from the camera), `scripted` (the rescue helicopter, the bird hit and the rally incident, from the
  characters' events) and `menu` (the music and the service park, for the launcher, with its own mixer). `Models` turns one step's inputs (`rbr_recording::sound::Inputs`: SND1, PCN1 and the
  HUD's countdown and race clock) and the camera's `Scene` into voice commands (`voice`).
- **Backend**: `mixer` plays the commands into stereo frames (no device; offline renders use it directly), `device`
  sends them to the default output through cpal. `Player` joins them for the viewer's live drive; the viewer gives
  it the camera and the car each frame (`rbr_render::sound::scene`).

`fmod` plays an RSF car's own FMOD Studio bank. The 7 Vorbis setup headers it needs are vendored from the `fsbex`
crate (MIT, `data/fsb-vorbis/`, with its license and attribution).

## Inputs

`rbr-standalone --live` writes one SND1 record per step (`rbr_recording::live`, between DMG1 and SCN1): the
`EngineNoise` payload (engine state +0x2c, 12 words), the `DrivetrainState` payload as `CDrive::M8480` builds it
(14 words, read from the drive without its writes; `M8480` runs from the frame loop, not the physics step), and the
car speed; then each wheel's `CSigTyreNoise` block (the tyre scratch's +0x420, 24 words), the damage handler's
`CSigCarDamage` payload (+0x3e60, 20 words) and speech ring index (+0x3e58), and the battery, brake and water
values of `CarSensorData`, each wheel's damper signal words (+0xec4+0x1c, 5 words) and strut velocity (+0x5cc), and
the step's count of `CSigEngineFire` emits (counted by `rbr-hybrid`'s `Pure::enter`, read-only). The record is read
from memory after the step and changes nothing in it. Each wheel's splash comes from the step's PTC1 records
(`CSigTireParticles` +0x98), and whether each door is off the car from DET1 (latched: DET1 lists only awake parts).

PCN1 (`rbr_recording::pacenote`) carries the step's `rbr_gameplay::markers::PaceCallout`s with what the co-driver's
handler looks up in the key list besides: the payload's distance (`E12_SeekD` at the callout's x, since `Game` leaves
it at 0), the gap to the next spoken key (0x476b10) and the next key's tag (0x475de0). The countdown is HUD2's
`countdown` (HUD +0x244), the timer the original's countdown codes come from.

## Faithful and ours

| Part | Source | Status |
| --- | --- | --- |
| RSF car with an FMOD bank: its recordings for the engine, turbo, blow-off, gear change and transmission, and its backfire, ignition and wind events (`fmod`) | the bank's sheets, fades, auto-pitch, and its mixing tree: tracks, groups, returns and sends with their faders, effects (filters, EQs, compressors, the SFX reverb) and automations (`notes/formats/fmod-bank.md`, `mixer::fmod_mix`, `mixer::dsp`) | ours: the engine-to-gear-change parameter values, the effects' implementation; the convolution reverb's wet signal, distortion, panner and modulators are left out. The backfire, ignition and wind parameters, the events' pauses and the 1.02 voices turned down to 0.003 (pops, bangs, ignition, blow-off, whines, wind) follow NGP's FMOD daemon (`NgpFmodDaemon.exe` `FUN_004092f0`, `FUN_00408ef0`; `notes/sound.md` 14) |
| rpm target, load, cylinder flags from `EngineNoise` | `CCarSound::M6a0` 0x4b06a0 | faithful (constants from the exe) |
| rpm slew: 500 per input, the 80 skipped values at 0x732cc8 | `CEngine::M740` 0x4bc740 | faithful |
| bank pair and crossfade | `M580`/`M5d0`/`M7b0` (decompile) | faithful; which gain goes to which bank is inferred from the formulas |
| one firing period per chunk, `rate / (rpm * 0.033333335)` | `M9c0` 0x4bc9c0 | faithful |
| grain choice: next row, row 0 and a random restart at `(r % (n - 11) & !3) + 5` | `VoiceBb0` 0x4bbbb0 | faithful; the random numbers are ours (seeded xorshift, not CRT `rand()`) |
| load mix of the load-0 and load-100 parts, misfire half load and 0.25 gain | `MixBd10`, `M9c0` | faithful |
| cubic resampling of a grain to the period | `Resample3e0` | formula faithful; the grain is stretched to exactly one chunk (ours: the original's sub-sample carry between chunks is not modelled) |
| gear and diff whine pitch, load volumes, in-car only | `CCarSound::Me510` 0x4ae510, `CalcVol` | faithful; the load normalisers `+0x41738`/`+0x41744` are taken as the caps 600 and 6000 (inferred) |
| gear-change click | `Me510` (`p[8] > 0.001`) | ours: on the flag's rising edge (the standalone does not run `M8480`, which clears it) |
| turbo whistle volume and pitch, a third outside | `CCarSound::Vf08`, `CalcVol` | faithful |
| blow-off valve on a turbo flow rise | `M6a0` | faithful |
| 10-step fade-in | `CalcVol` | faithful |
| gains: `audio.ini` amps times `Ingame.ini` volumes | `E31Master` | faithful; without an `audio.ini` the retail amps (ours) |
| engine filter after `MixScale`: in the car a one-pole low-pass at 400 Hz times 2 (`InsideCutFrequency` is not read here); outside the dry engine plus itself low-passed at `Car%d_LowShelfFrequency` (200 Hz) times `(rpm - 2500) * 0.0008` above 2500 rpm, nothing added below | `E30Engine::Vf08` 0x4ced85-0x4cef5b (disassembly), `E32Flt::SetCoef`/`Run`/`Mix` 0x4dde60-0x4ddf30 | faithful; `Car%d_LowShelfAmp` is loaded and never read; the replay and TV cameras' branch (a cutoff of `22050 - d * 21500` from a direction product `d`, else 450 Hz mixed in at 2) is not modelled: they play as outside |
| in-car flag | `g_8d5338[0x41bb]` | ours: the viewer's bonnet and cockpit views |
| tyre surface loops: two per wheel, the surface table at 0x735c80 (materials, pitch, share), the level from slip, tyre factor and speed, the skid loop above a grip maximum of 1.25 | `CTireSnd::M5fd0` 0x4d5fd0 (disassembly) | faithful; the table is copied from the exe (`tables.rs`) |
| each wheel's voices at the full tyre gain, times 0.7 and the software 3D path's distance gain (full within 10 m, linear to 0 at 40 m) | `M7ea0` 0x4d7ea0 (fewer than 0x21 free hardware channels: every system without DirectSound hardware mixing), `E30MixerBase::Mce250` 0x4ce250, `M6cb0` 0x4d6cb0 | faithful to the software path; the hardware path (`M69a0` 0x4d69a0, `M7ba0`, 2-10 m through DirectSound 3D) is not modelled |
| a wheel loop starts a quarter of the sample in per wheel | `M7ba0` `SetScaled` | faithful |
| tyre rumble: in the car, off tarmac-first surfaces, `x = min(speed * 0.1, 1)`, level `min((word 1 + 1 + factor * 1.5625e-5) * x, 1)` a quarter of the way per frame, half of it times the tyre gain, pitch `x * 0.4 + 0.8`; else stopped | `CTireSnd::M6750` 0x4d6750 (disassembly) | faithful; word 1 is the frame's largest `FillOutputs` term from the finish's 0 (`rbr_recording::sound::TyreMax`, the pure host's per-step terms; checked against the memory over tanner) |
| tyre wetness (`SkidWater`): the tyre gain times `min(splash, 1)` from `CSigTireParticles` +0x98, decaying by 0.95 a frame to 0 below 0.001 | `M5fd0`, `M6560` 0x4d6560, `M7ba0` | faithful; the splash is 0 in every PTC1 record of the drives tried, so the voice has not been heard |
| gravel hits (`IMPACT::GRUS Snd1-20`): the car within 20 m, per wheel in contact every `trunc(50 / (force sum + 0.01))` frames (under 1000, none on tarmac-first surfaces), pitch `row[0] + (rand % 10) * 0.02 - 0.1`, `GravelParticles amp * TIRE vol`, 1-10 m, at the wheel (the block's `+0xc`) | `CTireSnd::M7330` 0x4d7330 | faithful; the original counts render frames (retail `WaitRetrace = true` holds them to the display); ours counts 60 Hz frames of the drive's time, whatever the viewer's frame rate (ours); the in-car z jitter (up to 0.2 m) is not modelled |
| suspension squeaks (`CAR::SQUEEKS`, in the car, a peak change of 1 or more, one in ten, pitch 0.5), knocks (a ranged impact of power `(bump + 1) * 200` above 0.01 in the car), bottoming (`CAR::SUSPENSION` above 0.8), the broken-damper call, once a frame | damper handler 0x4ae260, `Vf5c` 0x4ac2c0, `M7700`, `M77c0` | formulas faithful; the frame's words as the damper step keeps them between the frame finish's resets (`CWheel::Vf28` 0x53f230, which the standalone does not run): the peak is the step velocity of the largest size, the bump the last bump-stop hit (a changed bump word). Ours: a 60 Hz frame, the strut velocity (+0x5cc) for the damper's; the `+0x1060` gate is taken as clear; the squeaks and the bottoming play at their wheel (`M7700`, `M77c0`: the tyre slot's `+0x7c`), the knocks at the car |
| broken driveshaft clunks (`CAR::DRIVESHAFT DS1-13`): per wheel once a frame, while its speed (SND1 `p[10 + k]`, kept at 0 while the shaft is whole, as `CDrive::M8480` 0x508480 does) is above 10 rad/s, once a turn (`|1 / (speed / 2 pi)|` s), `DriveShaftBroken` amp, at the wheel, in every camera | `CTireSnd::M7590` 0x4d7590 | faithful; ours: a 60 Hz frame, the `M4870` low-pass not modelled |
| exhaust pops (`CAR::EXHAUST`, from the fifth sample above 0.8, pitch `p4 * 0.5 + 0.75`), bangs above 0.9 (`CAR::BANGS`), pop runs above 0.75 (four tables at 0x7c3808, `CARn::EXHAUSTNEW`, gaps in 44.1 kHz samples, ended by a rising rpm target) | `M6a0`, `Mf280` 0x4af280, `Mf3f0` 0x4af3f0, `Mf180`, frame update 0x4b0940 | faithful; one voice per kind, a new one stops the last |
| engine fire: `Audio\Cars\Effects\fire.wav` when the last is not playing, `Enginefire amp`, min distance 20, and call 6 | `CCarSound::OnEngineFire` 0x4ae3a0, `Md1c0` 0x4ad1c0 | faithful; the emits of `CSigEngineFire` (0x4ef290) are counted by `rbr-hybrid`'s `Pure::enter` |
| off-road rattles: all four wheels off the 80 road surfaces (0x7c5870), from 5 km/h, every `10 / kmh` s, a random `Offroad1-7` | `CCarSound::Mce10` 0x4ace10 | faithful; the speed is the car's (CarSensorData's is inferred equal) |
| ranged impact: power + 500, index `rand() % 3 + (v - 200) * 0.005`, capped `count - 1 - rand() % 3` | `CCollisionSnd::OnDamage` 0x4b27c0, `Vf64` 0x4b2ed0 | faithful; a new impact is a changed payload (ours) |
| scream above 8000 in the car (stops the speech) | `OnDamage`, `Vf6c` | faithful |
| pace-note phrase: AND, the tag and flag words, the snapped distance or the gap's flags, INTO/ONTO | handler 0x4b65b0 (disassembly), `M5760`, `FUN_00572e60` | faithful; the co-driver switch and replay gates are taken as on and off |
| English takes: a random take per word | `FindBank` 0x4b63a0 | faithful; missing takes (`plus`, `minus`) are silent |
| countdown: numbers 3, 2, 1 and `Go.wav` at 3, 2, 1 and 0 s, once each | `FUN_00578c30`, `CSoundFX` 0x4ba810 | faithful; marks already passed when the drive is joined are skipped (ours) |
| damage calls once each: battery, brakes, water, gearbox and the damage handler's speech ring | `CCarSound::OnSpeech` 0x4ab940 and its senders | faithful; the fire sends 6 (`sus_broke` in 1.02's table) and a broken damper 7 (`weve_got_fire`), swapped as in 1.02 (NGP patches the two pushes, 0x4ae2eb and 0x4ae3c0, back the right way round: modelled under NSD1); the steering (2) and engine (3) calls have no sender in 1.02 and are never said |
| co-driver switch: no pace notes when off (countdown and damage calls stay) | handler 0x4b65b0, option `+0x3e8 & 4` | faithful gate; the option word is inferred. Launcher: Co-driver box, `viewer --no-codriver` |
| speech queue: 10 entries, a full queue drops; damage calls (priority 2) first, notes (0) after the last note | `SOUND::CSpeech::SpInsert` 0x4dd4d0, the callers' priorities | faithful (the signed read of priority 2 included) |
| a 0.3715 s pause after each word before the race clock runs, none after | `E31Speech::Vf08` 0x4d2e30 (HUD +0x140) | faithful; the pause runs per block (one 7 ms step), the voice's idle flag `+0x95` is inferred |
| car voices' distance gain from the camera, from the distances the code sets on each voice (1.02's `+0x74` and `+0x78` both set the min; NGP's as called): engine `SOUNDDEF::EXHAUST` (1.02 min 100 m, NGP 40-100 m), blow-off (40 m; NGP 2-40 m), turbo none (DirectSound's 1 m), ranged, pops, gravel hits, off-road and fire as called; the whine and the software path's wetness are 2D (no distance). A missing `SOUNDDEF` section takes the retail set, a car slot the audio `Cars.ini` lacks car 0's sound | `E30Engine::Vf04` 0x4cf220, `E31Turbo::M3cb0` 0x4d3d2c, `CDXTransmission` 0x4d2fe0, `Ingame.ini`, DirectSound 3D | distances faithful; DirectSound's default rolloff (`min / d`, held past max) is inferred; the two fallbacks are ours (the game would play silent) |
| sides (stereo balance) from the camera's right | DirectSound 3D | ours (a plain balance, no HRTF) |
| wind: only in a replay (app state 8; the race stays in state 1), outside the car, by the cloud and the ambient distance ramp | `CAmbientSounds::Vf08` 0x4aaf50, `Vf48` 0x4ab310 | faithful; the in-car wind by speed (state 8 without `E19IsState2`) has no case in Rust |
| birds: in a replay with the car more than 50 m away, in turn `bird1-7`, `Bird amp * clamp(d / 100)`, at the listener | `Vf08`, `Vf44` 0x4ab1b0 | the timer is ours: 1.02's int timer never fires (notes/marks-sound-ghost.md 2.2); here 5 s, then 2-5 s |
| siren: on a change to a camera that does not follow the car, inside a `.dls` table-6 sphere, above 70 km/h; every other change clears it; `Horn amp`, 1-30 m | `E31_5d00` 0x4d5d00, `E31_5bf0` | trigger faithful; placed at the sphere's centre and stopped at the next camera change (ours: the original sets no position and never stops it) |
| rain: light (50) and heavy (100) rain weathers, `Rain amp`, gain 2 outside and 1 inside; drops of `q % 4 + 4` samples (light and heavy alike), a two-pole smoothed decaying step of `±(q & 0xfff)` times `q / 8`, through a 300 Hz one-pole; the third stream (gain `rand % 100 / 100`) in the car only | `E30Rain::Vf08` 0x4d17e0, `E25Rain::Render90` 0x4b7b90, `KernelA0` (disassembly) | faithful; ours: the output rate, a drop across blocks, the streams mixed to both channels |
| rescue helicopter: `heli_g.wav` looping at the helicopter's body while it runs, `Helicopter amp`, full within 100 m (min distance set twice, 50 then 100), no Doppler | `E31_5a30` 0x4d5a30 | faithful; the body is `Characters::rescue_position` |
| bird hit: `IMPACT::VARIOUS` 0 (`birdhit`) at the strike's point, `Impacts amp`, 3-5 m | `CCollisionSnd::OnBirdstrike` 0x4b2460, `Vf68` 0x4b2c50 | faithful |
| rally incident (the helicopter call): the speech queue cleared and the voice stopped; `snd_chopper.wav` is built as a path and never played | `CRallyIncidentSnd::OnSignal` 0x4b7d10 (disassembly) | faithful; that the call sends it is inferred |
| music: front end `ba.ogg`, service park `sp.ogg` (`Sound.ini`), 1 s fade-in when opened, looped by reopening, music volume | `CMusic::Vf40` 0x4b4760, `Vf08` 0x4b4c70 | faithful; where it plays (the launcher's pages) is ours |
| service park: `SFX::BACKGROUND` loop (`Servicepark_BG`), hits drawn per frame: kind, place within 8 m, count 1-2, 0.7 s timer; only the wrench (0.8), hard and medium hits with 15-bit `rand` | `CServiceParkSnd` 0x4b98c0, `M96b0` 0x4b96b0 | faithful; ours: the random numbers, a 60 Hz frame, the place read as x right, the background at the listener |
| engine `MixScale`: gain `1 + (trunc(min(rpm, 8000) * 0.032 + load * 1.28)) / 256` and a 4:1 knee above half scale (`audio.ini`'s "compressor -6 dB 4:1") | `E32BufferHandler::MixScale` 0x4d99f0, `E30Engine::Vf08` | faithful (in floats) |
| engine reflections: the engine block low-passed (750 Hz in the car, else 100 Hz up to `16000 / d`), read back 1.6 m left and 1.4 m right later (+128 samples) at `vol / 512`, `vol` 128-256 by rpm; in the car a missing door adds the engine block `* 2`, clamped; head-relative at 1 m; `EnviormentalReflections amp` times the engine volume | `CCarSound::Vf08` 0x4b11cd, `E25Engine` `Filter80` 0x4b8780 / `Blend90` 0x4b8e90, `Mcb80` 0x4acb80, `SetPair` 0x4ac220, `CHingedRigidBody::SimFB40` 0x51fb40 | faithful; outside the car they keep the car's delays once set (before: 0, **inferred**). Ours: the output rate, a hard left and right, no 256-sample crossfade |
| ignition: `Ignition1` when the starter word rises from 0 while the engine is stopped, at the car, min 10 m | `DrivetrainState` handler 0x4ae510, `Vf54` 0x4ad090 | faithful; never fires in the drives tried |
| dog rings: `DR1-8` at `CDrive +0x1b4 / 4pi` Hz (first after `f / 2pi` s of engagement), in the car, 2D, `p[5] * 0.7` | `Vf08` 0x4b1085, `E31Transmission::M3510` 0x4d3510 | faithful; the clock runs on the step (ours); `p[4]` is 0 in the drives tried |
| broken cooler: `cooler.wav` looped at `min(EngineNoise p[5], 1) * 0.25` from 0.001, at the car, min 5 m | `M6a0`, `Md2b0` 0x4ad2b0 | faithful; `p[5]` is 0 in the drives tried |
| wipers: `WiperEngine` and `WiperUp`/`WiperDown` (amps 0.25) in the car in rain or snow at each change of the blades' direction, every 0.4 s | `Md490` 0x4ad490, `E88Wiper::Tick` 0x5f8720 | ours: once per change (1.02 never updates `+0x41928` and starts both every frame of each return stroke) |
| at most 16 sounds (`SOUND_LIMIT`): a one-shot is refused while 16 play by the count taken once a 60 Hz frame; the ranged impacts and `GO` stop the newest one-shot instead | `E30DXMixer::Vf48` 0x4cf940, `Vf4c` 0x4cf840, `Vf04` 0x4cfdca, `E31Master::Vf08` 0x4d4980 | faithful to current systems at 60 fps (XP with hardware voices played everything); 9 sounds always count, calibrated to the original's made counts per caller (the live list has 11) |
| blow-off: armed by a flow rise, then once a frame again each time the last has played 4000 samples until the flow falls to 0.01 or the volume to 0.1 | `CCarSound::Mcad0` 0x4acad0, `E31_39f0` | faithful |
| ranged impacts: at most 3 at once, 80 ms apart | | ours (scraping) |
| sample voices (loops, one-shots, speech, FMOD recordings) read at their pitch through a 16-tap windowed sinc from precomputed polyphase tables (256 phases, five cutoff bands: the source's Nyquist, or below the output's above pitch 1): no images of the 8-22 kHz samples and little aliasing of raised pitches | DirectSound's resampler (a FIR on Wine and on Windows since Vista) | ours |
| player volume options (engine, co-driver, effects, tyres), `--volumes` | `0x01660e08` + 4 k, their gain call sites | faithful |
| gain moves over 10 ms, master gain 0.8 (the original's level on the tanner drive; the menus 0.5), soft limiter above 0.8 | | ours |
| mixing | DirectSound | ours (stereo; the speech, countdown, birds and wind in the middle) |

Not done: the replay music and `heli_r`, the rally school's speech, the `.dls` table-4 track emitters (particle
emitters, not sound: notes/marks-sound-ghost.md 2.4) (see `lead/reports/sound-3.md`). The viewer has no replay or
trackside cameras, so the birds, the far wind and the siren play only in the offline render's trackside camera.

## Tools

- `rbr-sound --recording X [--stage NAME] --out WAV [--seconds S] [--from S] [--camera inside|chase|trackside]
  [--no-codriver] [--volume V] [--log CSV] [--solo LAYER]`: renders a recorded drive through `rbr-standalone --live --boot` to a
  stereo WAV (`src/bin/rbr-sound.rs`). The stage, rain and cloud come from the `.auto`'s weather preset. The
  trackside camera (ours) stands for the replay's TV cameras: it waits 8 m beside the road 50 m ahead of the car.
  `--events CSV` adds the scripted events (`frame,kind,x,y,z`, kinds `heli`, `bird`, `call`), as the `rbr-render`
  example `scripted_events STAGE RECORDING OUT [--rescue-at STEP] [--birdstrike]` writes them. `--solo` plays one
  of the mixer's layers alone (`mixer::LAYERS`: engine, reflections, loops, tyres, wheel, fmod, rain, shots, speech)
  for level checks. The last line gives the mixer's time per second of audio.
- `cargo run --release -p rbr-sound --example menu -- frontend|service OUT [SECONDS] [MUSIC_VOLUME] [VOLUME]`: the
  menus' sound (`menu`: `MenuMixer`, `MenuPlayer` for the launcher) to a WAV.
- `cargo run --release -p rbr-sound --example sweep`: an rpm sweep on the audio device.
