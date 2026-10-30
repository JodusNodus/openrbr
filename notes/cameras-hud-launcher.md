# Cameras, HUD and launcher

Presentation around the race: none of it writes physics. The game's behaviour is from the 1.02 exe; the look is ours
unless stated. Gameplay timing: [gameplay.md](gameplay.md). Detail dropped here: rbr-recomp `notes/cameras.md`.

## Cameras

Chase builder, settings and start orbit in `rbr_render::game_camera`; car views in `rbr_render::views` (modes 1, 3,
4, 5, 8, 10, 13, CH_CAM, seat shake); helicopter and flip cameras in `rbr_render::characters::scripted`; the go in
[gameplay.md](gameplay.md). Mode numbers are the game's.

- One camera host (`E15CamHost`, app +0x70): mode +0x00, matrix +0xcc, FOV in degrees +0x2c0, near +0x290, far +0x294 (300.0).
- `CCamera::Tic` runs the mode's builder, then `M7320` (0x487320) emits `CSigCamera` (matrix, velocity, FOV in radians, near, far, two flags); a mode change emits `CSigCameraModeChange`.
- Unported builders: mode 0 (0x4849c0), mode 4 (`M5120` 0x485120), modes 6 and 9 (0x486290, 0x485090).
- `CH_CAM` (action 9, `M487840` 0x487840) cycles 1, 3, 4, 5 in a race, 7 too in playback; blocked while the helicopter flies and in mode 13.

- Track cameras (mode 7, `.dls` table 3, builder 0x489f70; formats parsing in `rbr_formats::markers`): look-at,
  FOV, flag, mode word, znear; look-at -15/-16 give mode 4, -14 mode 1 (not in render code).
- Mode 11 (`M8690` 0x488690) is the fly-in; no code sets it, so 1.02 has no fly-in. Modes 2, 6, 9 are replay payload
  only. Mode 0 covers states 1 (with `m_unk10`) and 7. The mode-13 flip camera uses FOV 75 (not in render code).
- The car-flip camera port is not checked against a capture.

### Ours: the viewer's views

Ours views, nudges and FOV rules in `rbr_render::views`; choices in `rbr_render::camera_prefs` (`[camera]`,
`viewer --cameras`).

- Whether RSF itself applies `<car>_personal.ini` `[Cam_*]` overrides is not traced (the loader is
  `rbr_render::car::camera_sections`).
- The X-key adjust mode is gone; an old `camera adjust` line is ignored (not in code).

## HUD

Model and rules in `rbr_render::hud::model`, sprites in `hud::pace`, layout in `hud::stock`, parts in `hud::mod`
(`viewer --hud-hide`); split references in `hud::splits`. A recorded drive shows only speed, gear, rpm and controls.

- The game's 3D notes (`AID_MARKER`) are not drawn (dropped 2026-10-27).
- The minidash and digidash art is not drawn (not in code).

### RBRHUD gauges (RSF cockpit dials)

Data in `rbr_formats::rsf::gauges`, drawing in `rbr_render::dials` (toggle: `dials`); check
`rbr-formats/tests/rsf_data.rs` (every sprite and font the configs name is in its atlas).

## Launcher

Pages and policy in `crates/rbr-launcher/src/` (`main.rs` doc: command line, weather rules, volumes, co-driver
sources). Start runs the viewer with `rbr-standalone --live --boot` as child processes (`command.rs`).

- The game's quality settings and music are not offered (user decisions).
- The co-driver is set up on the Settings page, not per drive (user decision 2026-10-29).
- Install findings are dated data, not code: RSF install (2026-10-29, 525 stage folders) resolves LuppisV3 on 511
  stages (477 as the newest-file rule did, 33 where a newer `_default` won, 15 `_default`-only keeping their own
  notes, one stray file); the note-set rule itself is `codriver_packs::set_files`.
- Replays (ours): every live start records one (`--save-replay <files>/replays/<nanoseconds>-<stage>.rrp`,
  full history kept; `viewer --replay FILE [ARGS...]` watches it again). The file holds the viewer arguments that
  drove and the stepping process's frame log (`rbr_recording::replay`); watching runs the drive's frames again
  from its last start, so a replay needs the same install, setup and profile, else it stops with a digest error.
  While watching there is no device input, pause menu or ghost saving: P or Space pauses, R starts over, the
  camera key cycles the views, V resets the mouse look, Esc closes.
- Replays tab, after Car setup (`pages::Page::TABS`, `replay_page.rs`): cached, newest-first headers from
  `<files>/replays`, searched by title or filename or conditions;
  malformed and unfinished files stay visible with their error. Rows show the stage time, recorded time in UTC and
  saved road/cloud/precipitation/fog overrides; details include the stage start time and included penalties. A live
  window saves its last SPL1 beside the replay as `.rrp.splits`, distinguishing a finish, DNF and an interrupted drive;
  older replays without it show Time unavailable. Watch uses the saved presentation with the selected
  camera; Export offers 720p–2160p, 30/60 fps, an MP4 filename, audio and progress from the viewer's log (ffmpeg on
  PATH). Replay exit never advances the championship. Files are kept until removed by the player.

### `launcher-settings.ini`

Form in `rbr-launcher/src/settings.rs`, folder and once-only migrate in `rbr-launcher/src/home.rs`.

- Quality keys (`graphics_page::Graphics`): `lighting`, `particles`, `surfaces`, `cabin`, `contact`, `grass`, `windscreen`,
  `marks`, `wet_roads`, `detail`, `mirrors`; unset takes the High preset. The preset box is not stored: it shows the preset the
  keys match, else Custom; presets leave `marks`, the render scale and anti-aliasing alone.
- `perf_log` defaults off; on, the viewer logs with `viewer --perf-log <files folder>/logs` (`home::LOGS`, shown on the Graphics page).
