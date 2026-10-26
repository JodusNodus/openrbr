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
- Chase settings (globals 0x1660d94..0x1660dbc, read at run time; `rbr_render::game_camera` ports the builder, lag
  ring, height window and collision probes in full): `CAM_RADIUS` 4.4 (slider 3-7), `CAM_ANGLE` 13 (10-16),
  `VW_ANGLE` 11 (9-12), FOV 75, minimum distance 3.0, lag 0.8. The look-back swing is dead code.
- Car cameras (`rbr_render::views`; `E09_LoadCarCameras` 0x469210): only `Cam_bonnet`, `Cam_internal`, `Cam_bonnet2`
  from the car `.ini` (`Pos`, `Target`, `Near`, `FOV` in radians). `Up` is ignored: no roll.
- Seat shake: `CSigIncarShake` from the driver's seat (`CCar+0x14c0` in FRM1 car blocks) is added to modes 4 and 5.
- Track cameras: `.dls` table 3 (look-at, FOV, flag, mode word, znear); look-at -15/-16 give mode 4, -14 mode 1.
- Car-flip camera (`FUN_00487980`, on `CSigInitSpectatorFlipAnimation`): a track-side eye per flip code with
  probes, then mode 13. Ported in `rbr_render::characters::scripted::flip_camera`; not checked against a capture.

### Ours: the viewer's views

`rbr_render::views` (module doc has the mounts, nudges and FOV rules). C cycles cockpit, bonnet, bumper and chase
(`[controls]` `camera`). The pause menu's Camera item (ours, after Resume) opens the Camera page (`hud::camera_page`)
over the live paused view: the view and its nudges, saved at once into `launcher-settings.ini` `[camera]`
(`rbr_render::camera_prefs`; `CAR view = right up forward [tilt]`, tilt in degrees written only when not 0). The X-key
adjust mode is gone; an old `camera adjust` line is ignored.

- RSF `<car>_personal.ini` `[Cam_*]` sections override the `.ini`'s (`car::camera_sections`); whether RSF does this
  is not traced.
- Cockpit: `Cam_internal` pitched down at least `COCKPIT_PITCH` (2.8 deg) and moved back `COCKPIT_BACK` (0.5 m),
  less in 0.05 m steps until nothing visible lies within 0.6 m ahead (nor 0.35 m across the default frame, also from
  3 cm lower, the seat shake's dip); ahead of the wheel: at least `WHEEL_BEHIND` (0.65 m) behind it; `FOV` under
  1.2 rad pitches down until the hub shows. Retail cockpits are unchanged
  (`car_data::the_cockpit_eye_stays_out_of_the_seat`).
- Bonnet: the eye walks back along the body's top from `Cam_bonnet` to the windscreen, 0.08 m above it, tilted so the
  bonnet edge is 10 deg below centre (`Mounts::from_sections`).
- Chase: the game's mode 1 at our framing (8.5 m, 5.6 deg, 1.5 deg view angle) with the horizontal distance held
  (`game_camera::Settings::steady`).
- The FOV is global (launcher, default 50 deg horizontal); the cars' `FOV` is not used.

## HUD

`crates/rbr-render/src/hud/`, egui over the frame. In live mode it shows the game HUD's information from the
`rbr-standalone` records; a recorded drive gets only speed, gear, rpm and controls.

The look is pluggable: `hud::Readout` is what the HUD shows (with the model's `HudView`), and a look implements
`hud::Hud` (`paint(&mut self, &egui::Context, &Readout)`, on a 720-point-high screen). `Overlay::new` draws ours,
`hud::stock::Stock`; `Overlay::with_hud` takes another. The race's screens (`retire`, `pause`, `camera_page`) are
drawn over any look; the per-file content is `hud/mod.rs`. Model (pure HUD state: `hud::model` has the traced rules):

- The race clock holds at 0:00.00 until the go; the time includes penalties. The countdown's 3, 2, 1, GO is on an LED plaque
  (`stock/countdown.rs`, ours after RSF's board, shapes only; 1.02's are the "3 2 1 GO!" sprites of `Generic/PC_*_02.dds`).
- Stock layout (ours, 2026-10-21, after EA WRC): the clock at the top left (title chip, `mm:ss.mmm` in tabular digits,
  penalty rows, and since 2026-10-25 a passed split's panel under them); the progress line across the top (driven and
  stage length, start, splits, the car's diamond); fps and view at the top right; pace note, finish and result panels
  under the line; the dial at the bottom right (rev arc to the SHF1 rev limit plus 10% rounded up to 1000, red from
  the limit; shift dot at the gear's SHF1 upshift; brake and clutch arcs left, throttle and handbrake right; speed,
  gear, rpm, water, turbo, steering). The default font has no bold: the digits are drawn four times a fraction of a
  point apart.
- Car status (ours, `stock/car_status.rs`), at the bottom left only when something is lit or damaged: red chips for the lit
  lamps (a flashing one outlined between flashes), over a card with each damaged group at its worst repair part's
  level (`car_damage::level`, the service park's, NGP rules; Fine to Lost, `bstrings.eng`) and a top view of
  the car, body and wheels coloured by level. Groups: engine, turbo, gearbox, electrics, brakes, bodywork from the
  `repair.ini` part ids, then suspension and tyres per wheel. `rbr-standalone --live` sends CDM1 each step:
  `CarriedDamage::to_bytes`, read from the handler without writing.
- Each part hides on its own (`hud::Part`, `viewer --hud-hide clock,progress,pace,splits,dash,inputs,car,messages,view`,
  the launcher's HUD parts); the dial draws what shows. The countdown, FALSE START, WRONG WAY,
  the rally's false-start warning and the end pages always show with the HUD.
- The HUD hides while the rescue helicopter flies (`HUD::Update`).
- Result actions appear only at `MNU1.over`, including DNF and failed season stages; Restart uses RTS1 and clears
  the result actions and `--quit-after end+S` timer; Continue closes the viewer so the launcher's existing
  `RallyPage::stage_closed` records and saves the turn. Layout captures: `rbr-render` example `hud_results`.
- Texts are 1.02's `bstrings.eng`. The minidash and digidash art is not drawn.

### RBRHUD gauges (RSF cockpit dials)

RSF cars ship dial faces without needles; RallySimFans' RBRHUD plugin draws needles and displays onto the interior
textures. Data read by `rbr_formats::rsf::gauges`, drawn by `rbr_render::dials` (toggle: `dials`); both module docs
carry the layout (`gauges.rs`) and the drawing rules (`dials`).

- 99 cars, 156 configs (`rbr-formats/tests/rsf_data.rs`: every sprite and font the configs name is in its atlas).
  `gauge_selections.ini` picks the variant per car folder; `texture_migrations.ini` copies skin rects into the
  remapped texture.
- Ours where the plugin's behaviour is not in its data: needle angles clockwise in texel space; `UpShift` the VCU
  `Gear<N>Upshift`; dials lit by `mask.rgb + mask.a` at night and dusk. The gauge mode action (default End) cycles
  the digital-dash pages.
- Telemetry names most used: `RPM`, `Gear` (0 R, 1 N, 2 first), `UpShift`, `DistanceDriven`,
  `CoolantTemperatureC`, `SpeedKPH`, `RaceTime`, `OilPressureBar`, `DistanceToFinish`.

## Launcher

`crates/rbr-launcher/src/` (egui window, `main.rs` doc has the command line). Start runs the viewer with
`rbr-standalone --live --boot` as child processes (`command.rs`); `--command` prints that command.

| Page | File | Content |
| --- | --- | --- |
| Drive | `drive_page.rs`, `browser.rs`, `stage_browser.rs`, `car_browser.rs`, `preview.rs`, `ghosts.rs` | Car, stage (Browse… opens the car or stage browser in place of the body; favourite chips under each field), environment (weather preset or season row, road wetness, road wear, time of day, fog, clouds, rain / snow), tyre, race options, ghost, previews, Start |
| Championship | `rally_page.rs` | Season, rally stages, service park, setup and tyre choice, skill and damage |
| Setup | `setup_page.rs` | Setup values by the game's steps; named setups and the game's save slots |
| Controls | `controls_page.rs` | Bindings, force feedback |
| Graphics & sound | `graphics_page.rs` | Window, FOV, HUD and its parts, pace notes, volume, renderer additions, the performance log |
| Settings | `settings_page.rs`, `install.rs`, `disc_install.rs`, `gvrc.rs` | Ready to drive: a row each for the game folder (setup from the disc), NGP (download), cars (download, add, added) and the wheel; Advanced: NGP DLL, NGP and RSF paths, programs |

- Environment (`main.rs` doc has the weather, wear, fog and time-of-day rules; `drive_page.rs`, `command.rs`,
  `settings.rs`): Weather is either Stage weather or Custom (user decision; Stage weather the default). A specific row
  launches like Random with that row (`rbr-standalone` gets `--weather 1 --season-row` with the explicit `--wear`).
  Custom is the good weather on a dry road, the bad on a damp or wet one, with the chosen wear and our look over it
  (Look chip Sunny … Foggy, Adjust…, time of day default the bake's own sun). Wetness overrides the base's (viewer's
  `--weather` word, auto tyre and `rbr-standalone --wetness` follow); the road's grip stays with wetness. On a snow
  stage (surface 2: maps 10-18) the Rain row and the Drizzle and Rain looks read Snow, Flurries and Snow;
  Australia's particle file has neither emitter. Tyre and wipers are free in both. A saved ghost brings its weather
  (wetness and wear follow it). Random weather draws like `E74_FindBest` from `rand()` seeded once at launcher
  start; the Drive page shows the next draw, each Start takes it. Maps without a good or bad row cannot start
  (`catalog.rs`).
- The game's quality settings and music are not offered (user decisions).
- The co-driver is one "Co-driver" dropdown (Off, `1.02`, then each installed voice by source) and one "Stage notes"
  dropdown (Original, the RSF install, each local pack); "Call style" and "Distance calls" sit under a collapsed
  "Advanced", shown for a custom voice only (`main.rs` doc). An unset voice plays its source's default, and `1.02`
  keeps the original voice. Saved as `codriver`, `codriver_voice`, `codriver_pack`, `codriver_style`,
  `codriver_ranges`, `my_pacenotes` and `notes_pack` in `[launcher]`; old settings files load unchanged.
- Locally owned co-driver packs and stage-note packs (`codriver_packs.rs`): `codrivers/<pack>/` (a plugin folder:
  `PaceNote.ini`, `sounds/`, `config/pacenotes|ranges`, relative paths kept, one pack per folder) and
  `notes/<pack>/` (note INIs, looked up recursively by stage-folder name). `Import co-driver…` and
  `launcher --import-codriver SRC` copy an extracted folder, a `.zip`/`.7z` archive or a
  whole install (only read) into them; the plugin is found at the top, beside the notes, or under a single
  wrapper (note paths are kept whole, so stage folders keep their names); only settings, definitions, voice audio and parseable note INIs are copied,
  a taken name gets a `-2` suffix, and a failure leaves no pack behind. A successful import selects what it
  brought. The Co-driver and Stage notes rows choose a local pack or the RSF install (the default, unchanged when
  unset); the choices persist as `codriver_pack` and `notes_pack`, and the Stage notes row shows the resolved file
  or why the original notes are used. The lookup tries RSF's names then the launcher's own catalogue names, so it
  works without RSF. An audio-only pack with no styles or range sets is listed with its missing dependency and never
  appears ready.
- Install folder (`install.rs`): the first start offers the first folder with NGP's `physics.rbz` from the usual places
  (Windows `C:\Richard Burns Rally`, `C:\Games\...`, `%ProgramFiles%\...`, `%ProgramFiles(x86)%\...`; Linux `~/Games/`, `~/.wine/drive_c/`, Lutris and Bottles
  prefixes; macOS `~/Games/`). A refused folder's details go to stderr. Saved as `install_folder`.
- Cars: the slots NGP's `physics.rbz` has an NGP car in (its `common.lsp` sets NGP keys; the NGP package's own archive
  has none), and the RSF folder's cars, raced in slot 5 as RSF does; with RSF cars slot 5 itself is left out (in RSF's
  archive it is the car last picked). Added gvrc.de packs (`car_packs.rs`) race in slot 5 too; a pack without its model
  is drawn as retail car 5; a car RSF also has is listed once, the added one (removing it brings RSF's back). `--add-cars DIR` adds packs with no window.
- Downloads from gvrc.de (`gvrc.rs`, `ureq` with rustls): the list is gvrc.de's `carList.ini` (RBRCIT's), read live each
  time the Settings page's "Download cars…" opens; only `https://gvrc.de/` links are followed, the original eight cars
  and the sound banks are left out. A car's physics zip and model `.7z` go to a temp folder in `cars/` and are added
  by `car_packs::add_downloads`. "Download NGP…" (shown when the game folder has no NGP or no NGP plugin, and is not
  RSF's) installs the list's `plugin_physics_url` package into the game folder (`disc_install::add_ngp`): the only
  write the launcher makes there; it refuses a package whose `NGP.dll` is not 7.4.778's, writes `physics.rbz` last
  and keeps the retail one as `physics.rbz.bak`. `--download-ngp GAME`, `--download-cars [NAME...]`.
- Stages: the rally school and test maps ([HOMEBASE], [RSSTAGE], [RALLYYARD]) are not listed; a saved map naming one
  starts on the first startable stage. The viewer and `rbr-standalone` still start them.
- A season with no car (saved before the season's car existed) asks for one on Continue.
- Loading screen (ours, `viewer/loading.rs`): the viewer's window opens first (fullscreen when set) and shows the
  stage's picture (`rbr_render::stage_picture`, the Drive page's preview), `--title` (the launcher's stage and car)
  and a bar while a thread loads the scene, builds the renderer, draws the start view once offscreen and starts the
  sound, then drives in the same window. Escape or closing the window during the load exits. The bar's step shares
  are a Semetin load's with the Fiesta (M3 Pro: scene 1.2 s, renderer 3.3 s of which the stage meshes 3.2 s, frame
  0.17 s, sound 0.85 s); the meshes report their own progress. The offscreen frame makes the targets, the reflection
  cube and the dials' atlases, so the first window frame is a steady one (4K: 169 ms before, 30 ms after); the
  loading screen's own frames load the overlay's fonts (the HUD's first frame 54 ms before, 4 ms after).
- A missing viewer or `rbr-standalone` shows "ReRBR is incomplete: … Reinstall ReRBR."; stderr names the program.
- Windows: the programs are `viewer.exe` and `rbr-standalone.exe` beside `rerbr.exe` (`EXE_SUFFIX`). The launcher and
  the viewer are GUI programs; without a stderr the launcher's goes to `rerbr.log` in the files folder, inherited by
  the viewer and `rbr-standalone`, which the viewer and PowerShell's dialogs start with `CREATE_NO_WINDOW`
  (`rbr_render::console`). SDL3 makes its DirectInput helper window without its video part (`SDL_InitSubSystem`).

### `launcher-settings.ini`

In the player's files folder (`home.rs`: macOS `~/Library/Application Support/ReRBR`, Windows `%APPDATA%\ReRBR`, Linux
`$XDG_DATA_HOME/rerbr`, or `$RERBR_HOME`), with the saves, records, ghosts, setups and added cars. Earlier versions wrote them next to the
launcher; the first start without the folder copies them over once (`migrate`). Sections (`settings.rs`):

| Section | Keys |
| --- | --- |
| `[launcher]` | `car`, `map`, `recent_cars`, `recent_maps`, `favourite_maps`, `weather` (good/random/bad or a season row's section), `wetness` (dry/damp/wet; absent: from weather), `fog` (off/light/heavy; absent: from weather), `clouds` (clear/partly/light/heavy), `precipitation` (none/light/heavy; both absent: from weather), `wear`, `tyre`, `size`, `fov` (30-120), `volume` (0-4), `codriver`, `hud`, `units`, `fps`, `pace_stack`, `pace_3d`, `wipers`, `dials`, `hud_hide` (the hidden HUD parts, comma-separated), `callout_distance`, `marker_threshold`, `setup`, `damage`, `auto_gears`, `gear_guard`, `neutral_lock`, `clutch_help`, `ghost`, `championship_skill`, `championship_damage`, `championship_team`, `install_folder`, `ngp_physics`, `rsf_root`, `render_scale`, `anti_aliasing` (taa/off; `4x` from older files reads as taa), `reflections` (high/medium/low/off), `shadows` (high/medium/low), `present`, `fullscreen`, `start_time`, `perf_log` (on/off, default off: `viewer --perf-log <files folder>/logs`, `home::LOGS`; the Graphics page shows the folder), one key per `Enhance::NAMES` (`hdr` from older files is ignored: HDR is always on) |
| `[controls]` | Viewer bindings, H-shifter (`rbr_render::bindings`) |
| `[ffb]` | Force feedback (`rbr_render::ffb`) |
| `[camera]` | View and per-car nudges, written by the viewer |
| `[season]` | Championship cars NGP does not drive (ours, kept out of the game's save) |

Invalid values read as unset. `--list`, `--command` and `--screenshot` never write the file.
