# Cameras, HUD and launcher

Presentation around the race: none of it writes physics. The game's behaviour is from the 1.02 exe; the look is
ours unless stated. Gameplay timing: [gameplay.md](gameplay.md). Detail dropped here: rbr-recomp `notes/cameras.md`.

## Cameras

One camera host (`E15CamHost`, app +0x70): mode +0x00, matrix +0xcc, FOV in degrees +0x2c0, near +0x290, far +0x294
(300.0). `CCamera::Tic` (0x48a1f0) runs the mode's builder, then `M7320` (0x487320) emits `CSigCamera` (matrix,
velocity, FOV in radians, near, far, two flags). `M487680` (0x487680) changes mode and emits `CSigCameraModeChange`.

| Mode | Builder | Use |
| --- | --- | --- |
| 0 | 0x4849c0 | States 1 (with `m_unk10`) and 7 |
| 1 | `M6530` 0x486530 | Chase orbit; CH_CAM |
| 2 | as 1 (`2r`, `0.6a`) | Replay payload only |
| 3 | `M5760` 0x485760 | `Cam_bonnet`; CH_CAM |
| 4 | `M5120` 0x485120 | `Cam_bonnet2` plus seat shake and TrackIR; CH_CAM; forced in states 1 and 3 (the race view) |
| 5 | `M5c50` 0x485c50 | `Cam_internal` plus shake and TrackIR; CH_CAM |
| 6, 9 | 0x486290, 0x485090 | Replay payload only |
| 7 | 0x489f70 | `.dls` track cameras (table 3); playback |
| 8 | `M5010` 0x485010 | Rescue helicopter, set by `Activate`; camera 30 m back, 1.8 m up |
| 10 | as 1 | Start orbit in the countdown |
| 11 | `M8690` 0x488690 | Fly-in; no code sets it, so 1.02 has no fly-in |
| 13 | inline | Frozen car-flip camera, FOV 75 |

- CH_CAM (action 9, `M487840` 0x487840) cycles 1, 3, 4, 5 in a race (7 too in playback). Blocked while the helicopter
  flies and in mode 13.
- Start orbit (`M8840` 0x488840): entering state 10 sets the swing angle to pi; each frame `angle -= facc * 1.2`;
  below 0 mode 1 and the go (`M0047ca40`). `pi / 1.2` s, 375 frames of 7 ms. Rust: `rbr_gameplay::camera_go`,
  `rbr_render::views::start_orbit`.
- Chase settings (globals 0x1660d94..0x1660dbc, read at run time): `CAM_RADIUS` 4.4 (slider 3-7), `CAM_ANGLE` 13
  (10-16), `VW_ANGLE` 11 (9-12), FOV 75, minimum distance 3.0, lag 0.8. The chase eye's lag ring, height window and
  collision probes are ported in full (`rbr_render::game_camera`, probes on the stage `.col` in
  `rbr_render::stage_probe`). The look-back swing is dead code.
- Car cameras (`E09_LoadCarCameras` 0x469210) read only `Cam_bonnet`, `Cam_internal`, `Cam_bonnet2` from the car
  `.ini` (`Pos`, `Target`, `Near`, `FOV` in radians). `Up` is ignored: no roll.
- Seat shake: `CSigIncarShake` from the driver's seat (`CCar+0x14c0` in FRM1 car blocks) is added to modes 4 and 5.
- Track cameras: `.dls` table 3 (look-at, FOV, flag, mode word, znear); look-at -15/-16 give mode 4, -14 mode 1.
- Car-flip camera (`FUN_00487980`, on `CSigInitSpectatorFlipAnimation`): a track-side eye per flip code with
  probes, then mode 13. Ported in `rbr_render::characters::scripted::flip_camera`; not checked against a capture.

### Ours: the viewer's views

`rbr_render::views`. C cycles cockpit, bonnet, bumper and chase (`[controls]` `camera`). The pause menu's Camera
item (ours, after Resume) opens the Camera page (`rbr_render::hud::camera_page`): a bottom panel over the live paused
view that sets the view and its sideways, height, forward and tilt (the chase: height and distance), saved at once.
The chosen view and each car's nudges live in `launcher-settings.ini` `[camera]` (`rbr_render::camera_prefs`;
`CAR view = right up forward [tilt]`, the tilt in degrees written only when not 0, added after `COCKPIT_PITCH` and
`hub_in_view`). The X-key adjust mode is gone; an old `camera adjust` line is ignored.

- RSF `<car>_personal.ini` `[Cam_*]` sections override the `.ini`'s (`rbr_render::car::camera_sections`); whether
  RSF does this is not traced.
- Cockpit: `Cam_internal` pitched down at least `COCKPIT_PITCH` (2.8 deg) and moved back `COCKPIT_BACK` (0.5 m), less
  in 0.05 m steps until nothing visible lies within 0.6 m (`cockpit_back`). An eye ahead of the wheel moves to at least
  `WHEEL_BEHIND` (0.65 m) behind it; a `FOV` under 1.2 rad (RSF) pitches down until the wheel hub shows
  (`hub_in_view`). Retail cockpits are unchanged (`car_data::the_cockpit_eye_stays_out_of_the_seat`).
- Bonnet: the eye walks back along the body's top from `Cam_bonnet` to the windscreen, 0.08 m above it, tilted so the
  bonnet edge is 10 deg below centre (`Mounts::from_sections`).
- Chase: the game's mode 1 at our framing (8.5 m, 5.6 deg, 1.5 deg view angle) with the horizontal distance held
  (`game_camera::Settings::steady`).
- The FOV is global (launcher, default 50 deg horizontal); the cars' `FOV` is not used.

## HUD

`crates/rbr-render/src/hud/`, egui over the frame. In live mode it shows the game HUD's information from the
`rbr-standalone` records; a recorded drive gets only speed, gear, rpm and controls.

| File | Content |
| --- | --- |
| `model.rs` | Pure HUD state: pace-note queue (`M576580`, 8 notes, 0.5 s fade in), warning lamps (table 0x7d1900, 1 Hz flash), stand-still and car-problem messages, FALSE START, rally false-start warning, WRONG WAY (0x577590), odometer and distance to go, pace stack (`PN_STACK` 0-4) and 3D notes (`AID_MARKER`), both off by default |
| `pace.rs` | Pace-note symbol per tag (`M5721c0` 0x5721c0), drawn as shapes |
| `splits.rs` | Split times against `misc/times.ini` or, with `--saves`, the stage record; the finish panel as the result page (`CQRResultUI::Vf40` 0x5c3b10) |
| `retire.rs` | Red and black tints, retire message boxes, the championship FAILED page |
| `pause.rs` | Pause menu over `rbr_gameplay::pause_menu`, with the game's confirm boxes and our Camera item |
| `camera_page.rs` | The pause menu's Camera page (ours) |
| `mod.rs` | Layout and paint |

- The race clock shows 3, 2, 1, GO from the countdown; the time includes penalties.
- The HUD hides while the rescue helicopter flies (`HUD::Update`).
- Texts are 1.02's `bstrings.eng`. The minidash and digidash art is not drawn.

### RBRHUD gauges (RSF cockpit dials)

RSF cars ship dial faces without needles; RallySimFans' RBRHUD plugin draws needles and displays onto the interior
textures. Its data is read by `rbr_formats::rsf::gauges`, drawn by `rbr_render::dials` (toggle: `dials`).

- `Plugins/RBRHUD/gauge_selections.ini` picks the variant per car folder; `gauges/<car>/<variant>/config<N>.ini` is
  one per car texture drawn on (99 cars, 156 configs); `gauges/<car>/textures/` holds the remapped textures,
  emissive masks and atlas. `texture_migrations.ini` copies skin rects into the remapped texture.
- `config<N>.ini` `[main]`: `texture` (replaced), `remapped`, `atlas`, `emissive_mask`. Element `type`s:
  `DirtyRegion`, `Rotating` (needle; value to degrees through `rotation<N>value`/`deg` pairs, clockwise in texel
  space, angle 0 points left), `Static`, `Simple` (shown at `equal` or above `threshold`), `Number` (atlas font,
  formats `Whole`..`TimeHoursMinSec`), `Expanding` (bar), `Effect` (blink, hide, show, colour change, pulse), `Colour`,
  `Pie`, `Damage`. Positions are remapped-texture texels.
- `mode` 0 shows on every page; 1-3 are digital-dash pages cycled by the gauge mode action (default End).
- Emissive masks: RGB backlight plus alpha glow; ours lights the dials by `mask.rgb + mask.a` at night and dusk.
- Telemetry names most used: `RPM`, `Gear` (0 R, 1 N, 2 first), `UpShift` (ours: the VCU `Gear<N>Upshift`),
  `DistanceDriven`, `CoolantTemperatureC`, `SpeedKPH`, `RaceTime`, `OilPressureBar`, `DistanceToFinish`.
- Every sprite and font the configs name is in its atlas (`rbr-formats/tests/rsf_data.rs`).

## Launcher

`crates/rbr-launcher/src/` (egui window, `main.rs` doc has the command line). Start runs the viewer with
`rbr-standalone --live --boot` as child processes (`command.rs`); `--command` prints that command.

| Page | File | Content |
| --- | --- | --- |
| Drive | `drive_page.rs`, `picker.rs`, `preview.rs`, `ghosts.rs` | Car, stage, weather preset, wear, tyre, race options, ghost, previews, Start |
| Championship | `rally_page.rs` | Season, rally stages, service park, setup and tyre choice, skill and damage |
| Setup | `setup_page.rs` | Setup values by the game's steps; named setups and the game's save slots |
| Controls | `controls_page.rs` | Bindings, wheel wizard, force feedback |
| Graphics & sound | `graphics_page.rs` | Window, FOV, HUD, pace notes, volume, renderer additions |
| Settings | `settings_page.rs`, `install.rs`, `disc_install.rs` | Install folder (1.02 with NGP, read only), NGP and RSF paths, setup from the disc |

- Random weather draws like `E74_FindBest` from `rand()` seeded once at launcher start; the Drive page shows the next
  draw, each Start takes it. Maps without a good or bad row cannot start (`catalog.rs`).
- The game's quality settings and music are not offered (user decisions).
- Install folder (`install.rs`): the first start offers the first folder with NGP's `physics.rbz` from the usual places
  (Windows `C:\Richard Burns Rally`, `C:\Games\...`; Linux `~/Games/`, `~/.wine/drive_c/`, Lutris and Bottles
  prefixes; macOS `~/Games/`). A refused folder's details go to stderr. Saved as `install_folder`.
- Cars: the slots NGP's `physics.rbz` has an NGP car in (its `common.lsp` sets NGP keys; the NGP package's own archive
  has none), and the RSF folder's cars, raced in slot 5 as RSF does; with RSF cars slot 5 itself is left out (in RSF's
  archive it is the car last picked). Added gvrc.de packs (`car_packs.rs`) race in slot 5 too; a pack without its model
  is drawn as retail car 5; a car RSF also has is listed once, RSF's. `--add-cars DIR` adds packs with no window.
- Stages: the rally school and test maps ([HOMEBASE], [RSSTAGE], [RALLYYARD]) are not listed; a saved map naming one
  starts on the first startable stage. The viewer and `rbr-standalone` still start them.
- A season with no car (saved before the season's car existed) asks for one on Continue.
- A missing viewer or `rbr-standalone` shows "ReRBR is incomplete: … Reinstall ReRBR."; stderr names the program.

### `launcher-settings.ini`

In the player's files folder (`home.rs`: macOS `~/Library/Application Support/ReRBR`, Linux `$XDG_DATA_HOME/rerbr`, or
`$RERBR_HOME`), with the saves, records, ghosts, setups and added cars. Earlier versions wrote them next to the
launcher; the first start without the folder copies them over once (`migrate`). Sections (`settings.rs`):

| Section | Keys |
| --- | --- |
| `[launcher]` | `car`, `map`, `recent_cars`, `recent_maps`, `weather` (good/random/bad), `wear`, `tyre`, `size`, `fov` (30-120), `volume` (0-4), `codriver`, `hud`, `units`, `fps`, `pace_stack`, `pace_3d`, `wipers`, `dials`, `callout_distance`, `marker_threshold`, `setup`, `damage`, `auto_gears`, `gear_guard`, `neutral_lock`, `clutch_help`, `ghost`, `championship_skill`, `championship_damage`, `championship_team`, `install_folder`, `ngp_physics`, `rsf_root`, `render_scale`, `present`, `fullscreen`, `start_time`, one key per `Enhance::NAMES` |
| `[controls]` | Viewer bindings, H-shifter (`rbr_render::bindings`) |
| `[ffb]` | Force feedback (`rbr_render::ffb`) |
| `[camera]` | View and per-car nudges, written by the viewer |
| `[season]` | Championship cars NGP does not drive (ours, kept out of the game's save) |

Invalid values read as unset. `--list`, `--command` and `--screenshot` never write the file.
