# Gameplay sound

How 1.02 makes its in-race sound and how `crates/rbr-sound` reproduces it. Addresses are `RichardBurnsRally_SSE.exe`
1.02. File layouts (`audio.dat`, audio inis, `.eng`, FMOD banks): [formats.md](formats.md). The code's doc comments
carry formulas and constants. Detail: rbr-recomp `notes/sound.md`, `notes/marks-sound-ghost.md` section 2.

## Architecture

- **1.02.** DirectSound 8 under the exe's `SOUND::` library (`CDX*` backend, libvorbis). Placed voices are 3D
  buffers. The 30 ms sound thread never runs. Signal handlers run inside the physics step and fire one-shots at
  once; the frame update (`E31Master::Vf08`, from `CApplication` `0x4825b0`) renders streams and loops.
- **Gains.** `CRBRMaster` (`g_8d5338`) holds the 15 `SOUNDDEF` sets (`Ingame.ini`), the 28 amps (`audio.ini`) and
  the in-car flag `g[0x41bb]`. A gain is `SOUNDDEF` volume times amp times a player option (`0x01660e08 + 4k`:
  engine, co-driver, effects, tyres; Rust `mixer::Volumes`). Volumes are linear amplitude (`SetVol` `0x4d2740`).
- **Distances.** In 1.02 both `+0x74` (max) and `+0x78` (min) call `SetMinDistance`: the min is the last value set,
  the max DirectSound's default. NGP fixes `+0x74`. `SOUNDDEF` distances of `TRANSMISSION`, `TIRE`, `TURBO`,
  `BLOWOFFVALVE` are never read. Rust: `world::distance_gain` (`min / d` rolloff, inferred), `Command::Ngp`.
- **Sound limit.** 16 sounds (`E30DXMixer::Vf48` `0x4cf940`, limit `+0x440`), counted once a frame (`0x4d4980`).
  Over it a one-shot is refused; flag 0x400 (ranged impacts, `GO`) stops the newest one-shot instead. Without
  hardware mixing the original refuses about two in three one-shots. Rust: `mixer::SOUND_LIMIT` on the 60 Hz
  `Command::Frame`, 9 always-counted sounds (calibrated to harness `RBR_POPS` counts).
- **Level.** The original clips past full scale. Rust: master gain `mixer::HEADROOM` 0.5, about 4 dB under the
  original's level (0.8 matched its RMS within 0.5 dB but kept a limiter working most of the time), then a 2 ms
  look-ahead limiter at 0.8 (`mixer/limiter.rs`) and a tanh above 0.8 as a backstop. Tanner: four in five samples
  untouched, under 3% more than 3 dB down.
- **Determinism.** No sound code writes physics or gameplay state. Sound shares CRT `rand()` with non-physics
  systems only. Rust uses its own seeded RNG (`engine::Rng`).
- **Rust pipeline.** `rbr-standalone --live` writes per step SND1 (`rbr_recording::sound::Inputs`: `EngineNoise`,
  `DrivetrainState` as `CDrive::M8480` builds it, tyre noise blocks, damage, sensor and damper words, strut
  velocity, fire emits counted in `rbr-hybrid` `Pure::enter`), PCN1 (callouts), HUD2 (countdown, race clock), FMB1
  (NGP FMOD block), with PTC1 splash and DET1 doors. `Models` (`lib.rs`) turns a step and the camera's `Scene` into
  `voice::Command`s; `mixer` renders them; `device` plays them through cpal; `Player` serves the viewer.

## Faithful and ours

Every source below is faithful in triggers, banks, gains, pitches, distances and timers, except:

| Part | Ours |
| --- | --- |
| Random numbers | seeded xorshift, not CRT `rand()` |
| Frame clock | per-frame logic (tyres, gravel hits, damper, blow-off repeat, driveshaft, service park) on a 60 Hz frame of drive time |
| Resampling | 16-tap windowed sinc (`mixer::voices::interpolate`) for DirectSound's FIR |
| Mixing | stereo balance from the camera's right, no 3D panning; gains move over 10 ms |
| Clean-up | loops crossfade 20 ms over their seam; one-shots fade in 1 ms and out 4 ms, and over 6 ms when stopped; gravel hits, squeaks, rattles and impacts vary ±4% pitch and ±15% gain |
| In-car low-pass | `M4870`'s 1 kHz one-pole runs on gravel hits, squeaks, bottoming, driveshaft clunks and the rolling (not skid) tyre loops |
| In-car flag | the viewer's bonnet and cockpit views |
| Engine | grain stretched to exactly one period; replay/TV camera filter branch not modelled |
| Tyres | software 3D path only; no Doppler; no in-car z jitter of gravel hits |
| Gear click | on the flag's rising edge (`M8480`, which clears it, does not run) |
| Wipers | once per change of blade direction (1.02 restarts them each frame of the return stroke) |
| Impacts | a new impact is a changed payload; at most 3 ranged at once, 80 ms apart |
| Reflections | hard left/right, no crossfade on a delay change |
| Countdown | marks already passed when a drive is joined are skipped |
| Fallbacks | missing `SOUNDDEF` section: retail set; car slot missing from `Cars.ini`: car 0 |
| Birds, siren | own bird timer; siren at the sphere's centre, stopped at the next camera change |
| Extras | H-shifter grind (`Loop::Grind`); no music (user decision) |

## Engine

- **Input.** `EngineNoise` (`g_signalManager +0x6050`, each step, `CCarSound::M6a0` `0x4b06a0`): `p[0]` engine speed
  (rpm 500-9000, voice stops below 500), `p[1]` throttle, `p[2]`/`p[3]` turbo speed and boost, `p[4]` misfire,
  `p[5]` cooler, `p[6]` turbo flow, `p[8..11]` cylinder flags.
- **Bank.** All 8 cars use `subaru.eng` (`Cars.ini`); four of the other `.eng` files cannot load in 1.02.
- **Core** (`CEngine` vtable `0x732c64`, `engine.rs`): rpm slew of 500 per input skipping the table at `0x732cc8`;
  crossfade of the two nearest rpm banks, each mixing its load-0 and load-100 parts; one firing period per chunk,
  `44100 / (rpm / 30)` samples; next cycle row per chunk, random restart (`VoiceBb0` `0x4bbbb0`); cubic resampling.
  A misfiring cylinder plays at half rate and 0.25 gain. Ours: each chunk fades in over the last one's continuation
  (48 samples), `MixScale`'s gain ramps across the block and its 4:1 knee is soft (quadratic over 8192).
- **Output.** `EXHAUST` volume times `Engine` amp; `MixScale` (`0x4d99f0`, `engine::mix_scale`); then a one-pole
  (`E30Engine::Vf08` `0x4ced85`): in the car 400 Hz low-pass times 2 (`InsideCutFrequency` is not read), outside the
  dry block plus its 200 Hz low-pass times `(rpm - 2500) * 0.0008` above 2500 rpm. `LowShelfAmp` and the EQ are
  unused. Min distance 100 m (NGP 40-100 m).
- **Reflections** (`reflections.rs`, `CCarSound::Vf08` `0x4b11cd`): two head-relative voices at 1 m left and right
  replay the engine low-passed and delayed (1.6 m and 1.4 m in the car). A door off the car (Rust: DET1) adds the
  raw engine on that side.

## Transmission and turbo

- **Whine** (`DrivetrainState` `+0xa78`, `0x4ae510`, `car.rs`): pitch from tooth count times shaft speed, clamped
  0.125-4; volume from the load words (caps 600 and 6000, inferred); 2D, in the car only.
- **Gear click**: `p[8] > 0.001`, random `GearChange1-3`, in the car.
- **Turbo**: volume `clamp(speed * 7.6923e-5 * boost)`, a third outside; pitch `speed * 0.0004`.
- **Blow-off** (`Mcad0` `0x4acad0`): a flow rise arms it; it replays every 4000 samples while flow holds; min 40 m.

## Exhaust and car effects

- **Pops and bangs** (`exhaust.rs`, `Mf280`, `Mf3f0`, `Mf180`): `p[4]` non-zero plays a pop, above 0.9 a bang, above
  0.75 a pop run from four tables at `0x7c3808`, ended by a rising rpm target.
- **Engine fire** (`0x4ae3a0`): `fire.wav` and damage call 6.
- **Effects** (`effects.rs`): ignition, dog rings, broken cooler, wipers (in the car in rain or snow). Only the
  wipers fire in the drives tried.

## Tyres and surfaces

- **Input.** `CSigTyreNoise` (`+0x6268`, tyre scratch `+0x420`, 24 words per wheel, `CTireSnd` `0x4d5fd0`): surface,
  contact, slip, grip maximum, speed, position.
- **Surfaces.** Table at `0x735c80` (two materials, pitch, share; `tables.rs`). The 80 road surfaces at `0x7c5870`
  gate rumble, gravel hits and rattles.
- **Loops** (`tyre.rs`): two per wheel, skid loop above a grip maximum of 1.25. Wetness from the
  `CSigTireParticles` splash (0 in every drive tried). Rumble in the car off tarmac, using `sig[1]` folded per frame
  (`rbr_recording::sound::TyreMax`).
- **Software path.** Below 0x21 hardware channels (every system without DirectSound hardware mixing, inferred)
  `M7ea0` mixes each wheel's loops into one 3D stream at 0.7 times a linear 10-40 m gain (`Mce250` `0x4ce250`), both
  at the first voice's volume. Rust models only this path.
- **Gravel hits** (`M7330`): car within 20 m, per wheel every `trunc(50 / (force + 0.01))` frames off road,
  `IMPACT::GRUS`.
- **Off-road rattles** (`Mce10` `0x4ace10`): all wheels off road from 5 km/h, every `10 / kmh` s.
- **In-car low-pass** (`M4870`) only touches software-mixed voices, so 1.02 filters nothing. Rust runs it (ours):
  1 kHz on gravel hits, squeaks, bottoming, driveshaft clunks and rolling tyre loops in the car.

## Suspension and driveshaft

- **Damper** (`+0xc2b8`, `0x4ae260`, `suspension.rs`): squeaks, knocks (ranged impacts), bottoming, damage call 7.
  The standalone does not run `CWheel::Vf28` (`0x53f230`), so Rust rebuilds the frame's words from the strut
  velocity and the bump word.
- **Driveshaft** (`M7590` `0x4d7590`, `driveshaft.rs`): `p[10..13]` carry a wheel's speed only while its shaft is
  broken; then a clunk once a turn above 10 rad/s.

## Impacts

`CCollisionSnd` (`impact.rs`). `OnDamage` (`0x4b27c0`): a ranged impact at power + 500; above 8000 in the car a
scream that stops the speech. Retail `SHATTER` and `GLASS` volumes are 0: only `RANGED` is heard.

## Co-driver

- **Banks.** English: `CoDriver.ini`/`Numbers.ini`, random take per word; other languages one take. Word ids:
  `rbr_gameplay::markers`.
- **Phrase** (`0x4b65b0`, `codriver.rs`): optional `AND`, tag word, flag word, distance snapped to {10, 15, 30, 50,
  70, 100, 150, 200, 300, 400 ... 1000}, or for a short gap `AND` or `INTO`/`ONTO` before the next call. PCN1
  carries the callout, the `E12_SeekD` distance, the gap to the next spoken key and its tag. The co-driver switch
  (`CoDriver::muted`) mutes notes only.
- **Queue** (`CSpeech::SpInsert` `0x4dd4d0`): 10 entries, full drops; damage calls first, notes after the last note.
  A 0.3715 s pause after each word until the race clock runs (`E31Speech::Vf08` `0x4d2e30`).
- **Damage calls** (`OnSpeech` `0x4ab940`, once each, not in replays): battery, brakes, water, gearbox, fire (6),
  damper (7). 1.02 swaps 6 and 7 (fire says `sus_broke`); NGP patches them back, modelled under NSD1. Calls 2 and 3
  have no sender.

## Countdown

The HUD's 7.0 s timer (HUD `+0x244`, frame time) emits `CSigSoundFX` at 3, 2, 1, 0 s; `CSoundFX` (`0x4ba810`) says
the numbers on the speech voice and plays `Go.wav`. Rust: `RaceClock::countdown` via HUD2. No finish sound.

## World and ambient

`world.rs`, `mixer/rain.rs`.

- **Wind** (`CAmbientSounds::Vf08` `0x4aaf50`): replay only (the race stays in app state 1), outside the car.
- **Birds**: replay only, car beyond 50 m. 1.02's int timer never fires (inferred).
- **Siren** (`E31_5d00` `0x4d5d00`): on a change to a non-following camera inside a `.dls` table-6 sphere, above
  70 km/h.
- **Rain** (`E30Rain::Vf08` `0x4d17e0`): three synthesised drop streams through 300 Hz one-poles; light and heavy
  rain sound the same.
- The viewer has no replay cameras: birds, wind and siren play only in `rbr-sound --camera trackside`.

## Scripted and service park

- **Helicopter** (`E31_5a30`, `scripted.rs`): `heli_g.wav` looping at it while active. **Bird hit**
  (`0x4b2460`): `birdhit` at the point. **Rally incident** (`0x4b7d10`): clears the speech; `snd_chopper.wav` is
  never played. Events come from `rbr_render::sound::events` or `rbr-sound --events`.
- **Service park** (`CServiceParkSnd` `0x4b98c0`, `menu.rs`, launcher at a service stop): background loop and
  drawn work hits; with 15-bit `rand()` only wrench, hard and medium hits, within 8 m.
- **Music** (`CMusic`): not played.

## FMOD cars (ours)

An RSF car with an FMOD bank plays its recordings (`fmod.rs`, `mixer/fmod_mix.rs`, `mixer/dsp.rs`) instead of the
1.02 engine, reflections and turbo. NGP's daemon is not run. The bank is `AudioFMOD/<bank>.bank`, named by RSF's car
list (`cars.json` `audio`). Cars without one, and the launcher's added gvrc.de cars, use slot 5's retail sound (the
Impreza 2003). The other sounds come from the retail `audio.dat`, which an RSF install keeps as `audio.dat.disabled`.

- **From NGP** (`NGP.dll` 7.4.778, `NgpFmodDaemon.exe` 7.4): the command-5 block (`FUN_100b3470`; FMB1), the
  parameters (`FUN_004092f0`, incl. `gear`/`state` and `fluctuation`), the pauses (`FUN_00408ef0`), the 0.2 s
  timeout that silences the start orbit, and 1.02 voices turned down to 0.003 where the bank has the event.
- **Mix.** Each bank's own tree (tracks, groups, sends, faders, automations); no VCA or snapshot. NGP adds no
  limiter, so Rust keeps one master gain. NGP's FMOD output sits 6.8 dB (car) and 8.8 dB (outside) above ours.
- **Bank levels (ours).** The 57 RSF banks' engines measure -36 to -1 dB in the car and -31 to +1 dB outside, so
  at load each bank's engine is played through its tree over rpm, throttle and camera angle
  (`mixer::fmod_loudness`, three passes for its compressors) and trimmed, at most 30 dB, to where the retail engine
  sits on the same drive (`fmod::LOUDNESS`). The in-car trim takes the `_int` events, the outside one the `_ext`,
  the shared events their mean. Tanner under NGP, chase camera: engines at -21 to -15 dB against -34.6 to -10.7
  before, tyres at -23 dB. The 208 R2 stays 3 dB hot at the cap.
- **Ours.** `Distance`, `Event Cone Angle`, `Speed`; filter, reverb and compressor shapes. Not modelled:
  convolution reverb wet, echo, distortion, panner, modulators, `skid`, `gear_grind`.

Under NSD1 the 1.02 models also take NGP's patches: engine low rpm, whine caps, the tyres' second surface, road list
and skid threshold, max distances, damage-call swap.

## Open

- Which mixer (DX, Prologic, EAX) runs; which resampler mode 2 is.
- Whether 1.02 on current Windows takes the tyres' software path.
- The sound limit's count: 9 against the live list's 11.
- HUD countdown against the gameplay start; why a `--boot` drive starts below 0.
- What writes the splash (`+0x98`); ignition, dog rings, cooler and `EngineNoise p[5]`/`p[7]` never seen live.
- `E31Speech +0x95` as the idle flag (inferred).
- Wine's constant-power 3D panning against 2D speech.
- FMOD: region end rule, async and loop flags, parameter defaults, 3D attributes, an A/B against NGP.
