# rbr-sound

The gameplay sound of RBR 1.02, and RSF cars' FMOD banks. Behaviour, faithful and ours:
[notes/sound.md](../../notes/sound.md).

## Modules (`src/`)

| Module | Contents |
| --- | --- |
| `lib.rs` | `Models` (one step's `Inputs` and `Scene` to commands), `Player` (live drive) |
| `voice.rs` | voice commands |
| `data.rs`, `tables.rs` | banks from `audio.dat` and the inis; tables copied from the exe |
| `engine.rs` | granular `CEngine`, `mix_scale`, `Rng` |
| `car.rs` | whine, gear click, turbo, blow-off |
| `exhaust.rs` | pops, bangs, pop runs, engine fire |
| `effects.rs` | ignition, dog rings, cooler, wipers |
| `reflections.rs` | the engine's two reflections |
| `tyre.rs` | surface loops, rumble, wetness, gravel hits, off-road rattles |
| `suspension.rs`, `driveshaft.rs` | squeaks, knocks, bottoming; driveshaft clunks |
| `impact.rs` | ranged impacts, scream |
| `codriver.rs` | pace-note phrases, countdown, damage calls, speech queue |
| `world.rs` | wind, birds, siren, rain triggers, distance gain |
| `scripted.rs` | helicopter, bird hit, rally incident |
| `menu.rs` | service park ambience for the launcher |
| `fmod.rs` | RSF car FMOD banks, NGP daemon parameters |
| `mixer/` | renderer: voices, rain, FMOD tree, DSP effects, sound limit |
| `device.rs` | cpal output |
| `bin/rbr-sound.rs` | offline render of a recorded drive |

`data/fsb-vorbis/` holds the 7 Vorbis setup headers FMOD banks need (from `fsbex`, MIT).

## Tools

- `rbr-sound --recording X --out WAV [--stage NAME] [--seconds S] [--from S] [--camera inside|chase|trackside]
  [--no-codriver] [--volume V] [--volumes E,C,F,T] [--log CSV] [--events CSV] [--solo LAYER] [--car N]
  [--ngp-physics FILE [--rsf-car PACK|ID [--no-fmod]]]`: renders a drive through `rbr-standalone --live --boot`
  to a stereo WAV and prints each one-shot's made and refused counts. Options are in the binary's doc comment.
- `cargo run --release -p rbr-sound --example menu -- OUT [SECONDS] [VOLUME]`: service park ambience to a WAV.
- `cargo run --release -p rbr-sound --example sweep [SECONDS]`: an rpm sweep on the audio device.

They read `RBR_DATA`, `RBR_PATCH` (for `audio.ini`) and, for RSF cars, `RBR_RSF`.
