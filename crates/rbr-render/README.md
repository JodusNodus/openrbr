# rbr-render

wgpu renderer and the driving window. Notes: [renderer](../../notes/renderer.md),
[cameras, HUD, launcher](../../notes/cameras-hud-launcher.md).

```sh
RBR_DATA=... RBR_PATCH=... target/release/viewer track-54_E --car COROLLA \
  --drive $RBR_RECORDINGS/drive-tanner-corolla.bin --enhance all   # play a recording
target/release/viewer --help                                       # every option
scripts/launcher.sh   # live drives: the launcher runs viewer --live rbr-standalone --live --boot ...
```

## src

- `coords`: axis, depth and colour conventions
- `stage/`: `.lbs` meshes, render states, textures
- `visibility`: visibility cells
- `renderer/`: wgpu passes, pipelines, `Enhance`
- `atmosphere`: `TrackSettings.ini` section
- `relight`: time of day
- `skylight`, `height_fog`, `light_shafts`: sky and lighting (`lighting`)
- `sun`, `foliage`: sun cascades, plants
- `post`: HDR post pass, the composite's sharpening under TAA
- `taa`: temporal anti-aliasing (the default; upscales below 100 % render scale); `renderer/motion`: its motion pass
- `effects`, `puddles`: GPU rain, wet roads
- `car/`: car loading, parts, cabin bake
- `car_shadow`: shadow decal
- `lights`: lamps
- `damage`, `damage_parts`, `crack_decal`, `dirt`: damage visuals
- `particles/`: particles
- `glass/`: windscreen and wipers
- `skidmarks`, `skidmarks_pass`: tyre marks
- `ghost`: ghost car
- `characters/`: stage characters, scripted events, crew
- `fences/`: fence cloth
- `dials/`: RSF working dials
- `start_board`: start clock digits and lamps
- `playback/`: recorded drives on the car
- `live/`: live mode
- `camera`, `views`, `game_camera`, `camera_prefs`: cameras
- `stage_probe`: camera `.col` probes
- `hud/`: egui HUD
- `bindings`, `devices`, `shifter`, `ffb`: controls
- `sound`: live sound
- `present`, `profile`: presentation, timing
- `shader_check`: WGSL to SPIR-V test
- `bin/viewer/`, `bin/texture-pack.rs`

Tests in `tests/` need `RBR_DATA` (and `RBR_RECORDINGS`).
