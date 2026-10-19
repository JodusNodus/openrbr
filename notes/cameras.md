# Cameras: player views, in-car, replay cuts, helicam, FOV

How Richard Burns Rally 1.02 chooses and moves the camera, and what the camera
emits. Addresses are `RichardBurnsRally_SSE.exe` 1.02 unless marked HH
(HedgeHog3D). Labels: **loader** = read from the decompile or our
reconstruction of the named function; **file** = checked on retail data (say
how many files); **string** = only the binary's RTTI/strings show it; **source**
= our C++ comment (may be wrong); **inferred** = plausible, not proven. Related
notes: [stage-characters.md](stage-characters.md) (driver/co-driver seats, which
use `CSigIncarShake`), [particles.md](particles.md) (camera-signal listeners in
the particle system), [car-lights.md](car-lights.md), [formats/stage-files.md](formats/stage-files.md)
(`.dls` tables), [formats/cars.md](formats/cars.md) (car `.ini` `Cam_*`
sections), [formats/sgs.md](formats/sgs.md) (`Camera ROTO` nodes),
[physics/jointed-bodies.md](physics/jointed-bodies.md) (seat springs).

## Summary

One camera object, the **camera host** (`E15CamHost`, our name; a block inside
the application at `app+0x70`, size at least 0x500). It holds the current
**mode** (an integer 0..13 at host `+0x00`) and the camera matrix (3x4 at
`+0xcc`), FOV in degrees (`+0x2c0`), near (`+0x290`) and far (`+0x294`). Each
frame the application calls `CApplication::M004825b0` (0x4825b0, from the frame
tick at 0x44a3d0), which runs the host for the current application state. The
host's dispatch `CCamera::Tic` (0x48a1f0) runs the builder for the mode, then
`M7320` (0x487320) emits `CSigCamera` (matrix, FOV in radians, near, far and two
flags). Mode changes emit `CSigCameraModeChange`.

Modes are chosen four ways: the CH_CAM key cycles a fixed list (input action 9,
0x487840); application states force a mode (0x4825b0); a `.dls` camera record
selects the spline path (mode 7, from the `.dls` table 3 look-at word, 0x489f70);
and a replay camera change may set any mode from its payload (0x487980,
inferred). The in-car views are not separate modes: the three car cameras
(`Cam_bonnet`, `Cam_bonnet2`, `Cam_internal`) are read from the car `.ini` and
sent once per car as `CSigCarCameras`. The handler (0x486790) puts them in the
builders of modes 3 (`Cam_bonnet`), 5 (`Cam_internal`) and 4 (`Cam_bonnet2`);
see section 4.

Head movement comes from TrackIR (polled in the frame tick, `CSigTrackIR`), and
body shake from the seat springs (`CSigIncarShake`). Both reach the host; their
exact effect on the view is not traced. Everything here is presentation. The camera
never writes to physics. Physics reaches it only through `CSigIncarShake` (seat
springs), whose camera-side effect is not traced; the camera otherwise makes
read-only driveline and car queries.

## 1. Camera host, signals and per-frame order

Per-frame order (**loader**, 0x44a3d0 calls, source `src/rbr/RBR_E04_Main.cpp`):

1. `TicFwd` (input), then if `UseTrackIR` is set: `E29TrackIRApi::Sample` and
   `CSigTrackIR` emit (see section 8).
2. `app->Tic`, then `CApplication::M004825b0(dt)` (0x4825b0): the state switch
   in section 2.
3. `CPC_GraphicsDaemon::SetViewMatrix(app+0x13c)` (source; the renderer gets the
   application's view matrix, not `CSigCamera`).

Camera signals (**loader**: emit sites; payload layouts from the decompile and
`src/hedgehog3d/HH_Signals.h`):

| Signal | Emit (address) | Payload | Listeners (**string**/**loader**) |
|---|---|---|---|
| `CSigCamera` | `E15CamHost::M7320` 0x487320, emitter `CMultiSignal1<CSigCamera>::Emit` 0x487060, slot `g_signalManager+0x3028` | 0x54-byte block, section 3 | HH `CHH_World::OnCamera` 0x10066dc0 (string `CConnection<CHH_World,CSigCamera>`), `CPC_GraphicsDaemon::OnCamera` 0x4c0970, `C3DMixerBase`, particle systems (`NParticleSystem::*`, string) |
| `CSigCameraModeChange` | `E14Sub70::M487680` 0x487680 when the mode changes and `flag != 0`; slot `+0x5e38`, `Emit` 0x4871a0 | `{old, new}` as two ints | HH `OnCameraModeChange` 0x10066d20, particle systems (string) |
| `CSigCarCameras` | `E09_LoadCarCameras` 0x469210, `Emit` 0x468a30, signal manager `+0x5c20` | 3 cameras (section 4) | the camera host (`CConnection<CCamera,CSigCarCameras>`, string) |
| `CSigReplayCameraChange` | E12 replay firers: `E12_AnimSet::FireReplayByIdx` 0x47a7f0, `E12_FireReplay` 0x478b20, `E12_EmitFwd::FwdReplayCam` 0x477f20, E12_Tail.cpp:160-175 (address not traced); `Emit` 0x4777a0, slot `+0x6698` | a pointer to an E12 record; first word = camera index, second = mode (**inferred**, section 5d) | camera host (string), `CCarSound` (string), particle systems (string) |
| `CSigIncarShake` | `NPhysics::E45_P3210` 0x523210, slot `+0xc6e8` | 6 `vec4` (96 bytes) of seat-spring state (section 8) | camera host (string); LocoMotion seat (`notes/stage-characters.md`) |
| `CSigTrackIR` | `E04TrackIRSignal::Emit` 0x4497e0, slot `+0xe008`, sent from 0x44a3d0 when `UseTrackIR` | stack block built from `Sample`'s result (section 8a) | camera host (string) |
| `CSigInput` | (not traced) | | camera host (string, `CConnection<CCamera,CSigInput>`) |

Camera-host connections (**string**, RTTI names `CConnection<CCamera, ...>`):
`CarGeneralState`, `Input`, `CarCameras`, `ReplayCameraChange`, `ExitGame`,
`InitSpectatorFlipAnimation`, `IncarShake`, `TrackIR`, and an `ABM` one. Their
init is `E16_CamInit` (0x489c20); teardown `E16_CamTeardown` (0x489e00). The
thunks at 0x489a60 (IncarShake) and 0x489ac0 (TrackIR) load a `this` at
connection `+0x10` and a function at `+0x14` and jump to it (**loader**). The
camera init (0x4881c0) stores the targets: `CarGeneralState` 0x486620 (`M6620`:
the body axes, the look point `+0x260`, the velocity and the position `+0x3c0`),
`Input` 0x486880, `CarCameras` 0x486790, `ReplayCameraChange` 0x486760,
`ExitGame` 0x487010 (the reset 0x4868a0), `InitSpectatorFlipAnimation` 0x487980,
`ABM` 0x484990, `IncarShake` 0x484970, `TrackIR` 0x486d00 (**loader**).

## 2. Mode selection

### 2a. Mode numbers and builders (`CCamera::Tic` 0x48a1f0, **loader**)

| Mode | Builder | Selectable by CH_CAM | Notes |
|---|---|---|---|
| 0 | `E15CamHost::Tic` 0x4849c0 | no | source: damps host `+0x4f4..+0x4fc`, rebuilds basis rows `+0xcc..+0xec`. Set in application state 1 (`m_unk10 != 0`) and 7. |
| 1 | `M6530` 0x486530 | yes | Orbit around the car (below). Set by the camera init (0x4881c0) and by the -14 path (section 5b). Used for `.dls` look-at -14. |
| 2 | as 1 | no | Same builder. Reached only through a replay payload (inferred). |
| 3 | `M5760` 0x485760 | yes | `Cam_bonnet` (host `+0x318`, section 4). The eye is lifted to `near + 0.1` above the ground when a probe from 1 m above it to 30 m below hits within `near + 0.2` (`PTR_DAT_007c755c`). No shake. **visuals-2 (asm):** the probe runs from 0.5 m above (0x795e18), not 1 m, to 30 m below (0x72f42c); the eye goes to `hit + n (near + 0.1)` when `|hit - eye| < near + 0.2` (0x72f428). |
| 4 | `M5120` 0x485120 | yes | `Cam_bonnet2` (host `+0x350`), plus the seat shake `+0x4e0` (section 8b) and the TrackIR yaw and pitch `+0x518/+0x51c`. Default in state 1 (`m_unk70 == 0`) and state 3, so the game's race view. Used for `.dls` look-at -15/-16. |
| 5 | `M5c50` 0x485c50 | yes | `Cam_internal` (host `+0x388`), plus the shake and TrackIR as mode 4. |
| 6 | `M6290` 0x486290 | no | source: negated-basis snap. Reached only through a replay payload (inferred). |
| 7 | `E16CamHolder::Update` 0x489f70 | yes in state 8; elsewhere only if the key's `v` is nonzero, and no caller passes that | `.dls` track camera (section 5). |
| 8 | `M5010` 0x485010 | no | Rescue helicopter camera (section 6). Holds its last matrix while the helicopter is inactive. |
| 9 | `M5090` 0x485090 | no | source: position follower, rate `CAM_RADIUS*10` on `+0x258`. Reached only through a replay payload (inferred). |
| 10 | as 1 (`M6530`) | no | Start orbit in the countdown (state 10): `M487680(10,1)` sets the angle to pi; `M8840` swings it and gives the go (section 7a). |
| 11 | `M8690` 0x488690 | no | Fly-in along the driveline, then switches to mode 1 (section 7). No direct caller sets it. |
| 13 | inline, `CCamera::Tic` case 0xd | no | Frozen: sets FOV to 75.0 (`0x72f430`, **loader**) and runs `M7320` only. The matrix is whatever the last cut set (section 5). |

Mode 12 is not handled: `CCamera::Tic` logs "Unknown enumeration value in
CCamera::Tic( R32 )" (string).

### 2b. Key cycling (CH_CAM = action 9)

`USInputManagerImpl::Tic` (0x4cbf50) calls `E14Sub70::M487840` (0x487840) with
`v = 0` when `IsActionDown(9)` and the edge test pass (**loader**). Action 9 is
`CH_CAM` (`src/rbr/RBR_E28_Action.cpp:250`, the action-name table). `InitA`
(0x47ca10) calls it once with `v = 0` at start-up (**loader**). The cycle
(**loader**, 0x487840):

- Blocked when the rescue helicopter is active (`CRescueHelicopter::IsActive`,
  0x49bea0), when application state 8 has `E32Host40::M00544f50`, when the
  current mode is 13, or when the gameflow host reports state 7 with sub-state
  6, 2, 7 or 10 and `DAT_0165fa90+0xca4 != 5`.
- Otherwise it steps `mode = (mode + 1) % 12` and skips modes that do not fit.
  Outside state 8 the allowed modes are 1, 3, 4, 5, plus 7 when `v != 0`. In
  state 8 they are 1, 3, 4, 5, 7.
- A chosen mode goes through `M487680(mode, 1)`, which emits the mode change and
  runs the per-mode setup. Then, unless state 8, it calls the gameflow host `M663e0` (0x5663e0), and it clears the host flag `+0x08`.

So the player cycles 1 -> 3 -> 4 -> 5 -> 1 in races (**loader**): chase, `Cam_bonnet`, `Cam_bonnet2`,
`Cam_internal`. HedgeHog3D's mode-change handler (0x10066d20) groups 4, 5 and 6 (its in-car mode 1), 3 alone (2)
and 1 alone (4). Mode 7 is
never reached by the key outside playback (state 8), since `v == 0` at both
call sites.

### 2c. Application state driver (`CApplication::M004825b0`, 0x4825b0, **loader**)

Application states are the `m_unk728` values (`SetAppState`, 0x47f370). The
state values are not named in the source. The `APPSTATE_*` strings in the exe
give the names; the numbers below are read from the switch (**loader**), and the
names are **inferred** from the code around each case:

| State | Camera action |
|---|---|
| 1 | If `m_unk10 != 0`: mode 0, then dispatch. Otherwise dispatch, first forcing mode 4 when the 4-byte word at `+0x70` is 0 (then `E19IsState1` and 0x566340). |
| 2 | Dispatch only. Goes to state 1 when gameflow `+0x1e == 1`. |
| 3 | Mode 4, then dispatch. |
| 5 | Init: `SetAppState(0xd)` (view init). |
| 7 | Mode 0, then dispatch. |
| 8 | Playback (inferred from `gameplay-loop.md`, which names state 8 as playback). Dispatch. State 10 sets mode 7 when it enters this state (`M487680(7,1)`). |
| 10 | Countdown. Sets mode 10 (`M487680(10,1)` on some paths); at the end goes to state 1 or 8. |
| 0xc, 0xe | Set mode 7. |
| 0xd | Dispatch with the host's own mode (13 is set by the cut code, section 5). |
| 0xf | Dispatch. |

Exact guards are in the decompile at `0x4825b0` lines 38-336.

### 2d. Mode-change side effects (`M487680` 0x487680, **loader**)

- Emits `CSigCameraModeChange` `{old, new}` when `flag != 0` and the mode changes.
- Sets the snap flag `+0x08` (the chase eye jumps to its place each frame while it is set, section 9a), except for a
  change into 1 or 2 from 1, 2 or 10. `M487840` clears it after a CH_CAM change; `0x4868a0` (reset) sets it.
- Leaving mode 8: stores the view offset `+0x260..+0x26c` from the matrix rows
  and the `+0x3c0` offsets.
- Entering mode 8 sets `+0x3f0 = 0`, clamps `+0x134` into `+0x400` through the
  driveline (`CDriveLine::ClampMix` 0x50b0f0), and adds 2.5 to `+0x408`.
- Mode 10 in state 10 sets `g_1660da0 = pi` and returns.
- Mode 11 zeroes `+0x410` (the driveline distance).

## 3. Per-frame emission: `CSigCamera` (`M7320`, 0x487320, **loader**)

`M7320(dt)` does four things:

1. Calls `M6af0` (0x486af0; arguments not traced). When `dt > 0`, it sets the velocity `+0x90` to the change in `+0xf0..+0xf8` divided by `dt`.
2. If the debug flag `DAT_01660dac` is set, prints `N: near F: far FOV: fov` and
   `Cam: pos Dir: dir` through the debug view (strings, **loader**).
3. Sets two flags from the current spline record `+0x3e8` (section 5):
   bit 0 when the record's word 3 (float) is > 0; and a second flag
   when mode is 7 and the record's word 4 has bit 2 set.
4. Emits `CSigCamera` built from locals: the 3x4 matrix (copied from `+0xcc`)
   and the fields at `+0x40..+0x50` below.

`CSigCamera` as the exe fills it (**loader**, offsets from the locals layout;
HH's view is `src/hedgehog3d/HH_Signals.h`, which names the same offsets):

| Offset | Field | Source in the host |
|---|---|---|
| `+0x00..+0x2f` | 3x4 view matrix | `+0xcc` |
| `+0x30..+0x3f` | camera velocity `vec4` | (position change in `+0xf0..+0xf8`) / `dt`, also stored to `+0x90` |
| `+0x40` | FOV in radians | `+0x2c0 * 0.01745329` |
| `+0x44` | near | `+0x290` (int bits read as float) |
| `+0x48` | far | `+0x294` |
| `+0x4c` | flag word, bit 0 | record word 3 (float) `> 0` |
| `+0x50` | flag, 1 when mode 7 and record word 4 bit 2 | record `+0x10` |

HH uses it (**source**, `HH_World.cpp:356`): `fov*0.75`, near `max(0.1, +0x44)`,
far `renderer m_unk24 * +0x48`, and `+0x50` picks the far factor (1.0 or 0.9,
`HH_Renderer.cpp:685`). This is rendering only.

## 4. Car cameras (`Cam_*` in `<car>.ini`, `E09_LoadCarCameras` 0x469210)

**file**: every retail car `.ini` has `[Cam_backseat]`, `[Cam_bonnet]`,
`[Cam_bonnet2]`, `[Cam_external]`, `[Cam_internal]` (8 of 8 cars);
PEUGEOT_206 also has `[Cam_front_l]` and `[Cam_front_r]`. Each section has
`FOV` (radians), `Near`, `Pos`, `Target`, `Up` (car space).

**loader** (0x469210): reads only `Cam_bonnet`, `Cam_internal` and
`Cam_bonnet2` (string literals in the function). For each it reads `Pos`,
`Target`, `Up` (vec3, defaults `(0,0,0)`, `(0,1,0)`, `(0,0,1)`), `Near`
(default 1.0) and `FOV` (default 1.3 rad; multiplied by 57.29578 to degrees).
`E09_BuildCamMatrix` (0x4663f0) makes a 3x4 matrix from `Pos` and `Target` (yaw
from `atan2(dx, dz)`, pitch from `atan2(dy, horizontal)`; it never reads its `up`
argument, so `Up` has no effect and the camera has no roll: the BMW M3 E30 and
Alpine A110 RSF models' `Up = 0 0.049 0.999` changes nothing in the game). Each contributes a 12-float matrix with its FOV (degrees) and near floats beside it. The three go out together as one `CSigCarCameras` (message layout not fully established). The loader also walks
`g_e09ExhaustNames` for exhaust positions; that part is not camera (see Other systems).

Retail values (**file**, XSARA `xsara.ini`): `Cam_internal` FOV 1.308997 rad
(75.0 deg), Near 0.0991, `Pos` (0.284, 0.951, -1.303); `Cam_bonnet` FOV 75 deg,
Near 0.1, `Pos` (0, 0.811, -2.916). All eight cars have `Cam_internal` FOV 1.308997.

`CSigCarCameras` listeners: only the camera host (**string**). The handler is
`0x486790` (bound by the camera init 0x4881c0, connection `+0x50`, **loader**):
`Cam_bonnet` (payload `+0x00`, matrix, FOV, near) to host `+0x318` (mode 3),
`Cam_internal` (`+0x38`) to `+0x388` (mode 5), a vec4 at `+0x70` to the chase
look point `+0x420` unless it is zero, and `Cam_bonnet2` (`+0x80`) to `+0x350`
(mode 4) unless its FOV and near are zero. The loader clears the vec4 and never
sets it, so the look point keeps the init's `(0, -1.544, 0.75)` (body frame,
1.54 m ahead of the body and 0.75 m up).
`E09CamState::Clear` (0x466300) resets a vec4 and two floats; its users are not traced.

**Which file** (**file**, not traced in RSF): the config reader reads the car's `IniFile` from `Cars.ini`, the
`<car>.ini`; RSF's `Cars.ini` entries name that file too. RSF cars also ship `<car>_personal.ini` with the same
`[Cam_*]` sections (42 of the samples' models). Mostly it equals the `.ini`; the Alpine A110's `Cam_internal`
differs (`.ini`: `Pos` z -0.963, FOV 1.309; `_personal.ini`: z -1.323, y +0.07, FOV 0.98). Whether RSF's plugin copies
`_personal.ini` over the `.ini` at load is not traced (its code is not in the samples). Ours takes its `[Cam_*]`
sections over the `.ini`'s, as the player's RSF cameras (`rbr_render::car::camera_sections`, Cameras-2 of
2026-10-18; an open question). Some RSF models copy `Cam_internal` into `Cam_bonnet` (no-shake in-car) and keep
the model's own as `Backup_Pos`/`Backup_Target`/`Backup_Near` (Fiat 131 Abarth); NGPCarMenu empties it (Xsara Kit
Car); the Fiesta WRC 14's sits over the windscreen. The viewer's bonnet view handles all three
(`rbr_render::views::Mounts::from_sections`).

**Ours, the four views (Cameras-2, user 2026-10-18):** C cycles cockpit, bonnet, bumper and chase. The bonnet eye
walks back along the body's top from `Cam_bonnet` to where the windscreen rises (the largest growth of the rise over
0.25 m), 0.08 m above it, tilted down so the bonnet's front edge is 10 degrees below the image's middle (at most 8
degrees below `Cam_bonnet`): every retail and sample RSF model shows its bonnet at 50 degrees across (checked by
`car_data::the_cockpit_eye_stays_out_of_the_seat` and captures). The chase is the game's mode 1 at our framing. The
view chosen and each car's nudges are kept in `launcher-settings.ini` `[camera]`.

**Ours, Cameras-3 (2026-10-18):** the chase framing is 8.5 m, 5.6 degrees and a 1.5 degree view angle (the eye 6.9 m
behind the car's centre, 1.6 m above it), and its horizontal distance from the look point is held at the snapped
`r cos a` (`game_camera::Settings::steady`) instead of step 8's 3 m to `r + 1` window, so it is the same at the start
and at speed; the lag then only swings the eye about the car. In the in-car view the interior is lit without the
car's own sun shadow and without the surroundings' ambient (the game's car constants have neither; both are renderer
additions), with the ambient times 2.5 (ours, against a 1.02 capture's dash). C and X are bindings (`[controls]
camera`, `camera adjust`).

**Ours (Cameras-4, 2026-10-18):** the viewer's cockpit is `Cam_internal` (with `_personal.ini` over it) turned to
look at least `COCKPIT_PITCH` (2.8 degrees) down and moved back `COCKPIT_BACK` (0.5 m), or less in 0.05 m steps
until nothing the view shows lies within 0.6 m along a 5 by 3 grid of lines (about 27 by 14 degrees) around its
middle (`rbr_render::views::cockpit_back`). Here "shown" means not alpha-blended, and facing the eye if the face is
culled at its back. Every retail car's `Cam_internal` looks 2.8 to 5.3 degrees down and moves the whole 0.5 m, so the
retail cockpits are unchanged (`car_data::the_cockpit_eye_stays_out_of_the_seat`). The earlier rule kept the eye 0.05 m
in front of any surface behind it. It stopped the RSF GR Yaris at 0.01 m: a thin part of its headrest is 0.07 m
behind the eye, and the eye passes it unseen (back faces). Now it moves 0.10 m and stops before its seat shell.
Level or upward RSF cameras (Alpine A110 +0.2 degrees, Fiesta Rally3 Evo +2.4, Focus WRC 06 +0.9, Xsara Kit Car,
Celica ST185, 306 Maxi) showed only the dash top and the roof at 50 degrees across. The Alpine's "black interior" was
this: its textures load and draw (the wheel and the dials show when it looks down), and its
`Alpine_A110_2020_internal.dds` is almost black (mean 10/255).

Measured over the samples (eye at the cockpit, `i_steeringwheel` node): the retail cars' wheel hub is 0.30 to 0.40 m
below the eye and 0.66 to 0.93 m ahead. The RSF Yaris's is 0.29 m below and 0.70 m ahead. Its eye is no higher
than a retail one. What fills its view is the roof lining and the windscreen banner, at our 50 degrees across.
`Cam_internal`'s `FOV` (the game shows `0.75 * FOV` vertically at 4:3; 56 degrees for retail, 42 to 43 for most RSF
models) is not used: the FOV is global (user).

**Ours (Cockpit-Cam, 2026-10-19):** some RSF models' `Cam_internal` `Pos` lies over the dash, at or in front of the
steering wheel: the Fiesta Rally3's 0.17 m in front of its `i_steeringwheel` node (`Pos` z -1.70, the node -1.53;
its `D_driver_use` seat node is at -1.10, where every retail `Pos` lies within 0.3 m of its own), the Fiesta Rally2's
0.25 m, the Focus WRC 06's 0.05 m. Rendered at its own eye and its own FOV (0.98 rad, 42 degrees vertically) the
Fiesta Rally3 shows the dash top and no wheel, so it hides the dials in the game too. The samples hold no other RSF
camera data (`<car>_personal.ini` equals the `.ini` for it; no plugin camera file). Two rules (ours,
`rbr_render::views`): the eye moves back at least to 0.65 m behind the wheel node (`WHEEL_BEHIND`; retail eyes sit
0.66 to 0.93 m behind it after `COCKPIT_BACK`, so they do not move), at most 1 m from `Pos`, still stopped by the seat
probe; and a car whose `FOV` is narrower than 1.2 rad (every RSF sample; retail's are 1.309) looks down until the
wheel's hub is at most 3 degrees under the bottom of the 50-degree, 16:9 image, at most 5 degrees more, stopping before
a surface comes within 0.3 m (`hub_in_view`). The second is for the modern cars' displays on the wheel or the column
inside its rim (208 Rally4, 208 R2, Clio R3, Twingo R1), which a retail-like eye height leaves just under the
image. The gate keeps the retail cockpits byte-identical: without it, five retail cars would tilt 1 to 6 degrees
(their dials sit above the wheel). Player nudges apply on top as before.

## 5. Track cameras (`.dls` table 3) and replay cuts

### 5a. Table 3 record (`.dls`, 6 words; **file**: 86 of 86 files, 3,205 records)

Per the survey, with the layout the loader uses (**loader**: `FUN_00479020`
via `rust/rbr-formats/src/dls.rs` `DlsCamera`; the field meanings are from the
values, **file**):

| Word | Meaning | Retail values (**file**) |
|---|---|---|
| 0 | id (resolved through the trigger-id search) | |
| 1 | look-at | -1 car 2,092; 0 fixed 994; a trigger id 74; -16 25; -15 15; -14 5 |
| 2 | FOV, degrees (float) | 80, 50, 60, 40, 20, 64 ... |
| 3 | float; the consumer tests it against zero | 0.0 in most rows; 0.1 to 0.4 in others |
| 4 | mode word, values 0/1/2 | 0: 2,926; 1: 157; 2: 122 |
| 5 | znear (float) | 1.0 in 2,918; 0.1, 0.5, 0.05 and others |

Word 4 bit 0 and bit 1 become flag bits in the record (Rust `DlsCamera` doc,
**loader**). The `.dls` names "shake" for word 3 and "helicam" for mode 2 (from
the Blender exporter, `formats/stage-files.md`) are not confirmed by the code
that reads them. The code uses word 3 only as zero or nonzero.

### 5b. How the record drives the camera (`E16CamHolder::Update` 0x489f70, **loader**)

Current record: the pointer at host `+0x3e8` (an E12 camera point) gives the look-at class through
`E12_Classify` (0x474ee0): it returns `-16` for raw `-16` or `-15`, `-14` for
raw `-14`, and 0 otherwise.

- Class `-16` (look-at -15 or -16): mode 4 (`M5120`), then `M8840` and `M7320`.
  So the "driver" label in `formats/stage-files.md` is not what the code does:
  both -15 and -16 give mode 4.
- Class `-14`: mode 1 (`M6530`), then `M8840`, `M7320`. After either -16 or -14 the code sets the host mode back to 7 silently (`M487680(7,0)`, no emit). The next frame's dispatch therefore sees 7, and this path sets 4 or 1 again with an emit each frame (**loader**: code order; the run-time emit is not traced).
- Class 0 (fixed, car, and trigger-id looks): mode 7. Then:
  - look-at == -1 (car) and word 3 == 0: `E12_SplineCtrl::CopyTo` and
    `E11_MatCompose` into the host matrix `+0xcc`.
  - look-at == -1 and word 3 != 0: `E20` spline shell `Tic` and `Blend` (0x49f6e0,
    0x49fbc0). The float is used as a flag here, not as a shake amount.
  - Otherwise: `E12_Cam::Tic` (0x474f20) over the record's E12 camera point,
    which has three branches (follow, driven and fixed; section 5c).
- The record's FOV (word 2) goes to `+0x2c0` (`w[0xb0] = rec+8`), and near (word
  5) to `+0x290` (`w[0xa4] = rec+0x14`).

The state switch in `M004825b0` (state 1 with `m_unk70 == 0` forcing mode 4)
does not depend on the record.

### 5c. E12 camera point (`E12_Cam::Tic` 0x474f20, **loader**)

Three branches on the object's flags: (a) a parent link `self[1]` (compose with
the parent's matrix); (b) flag `& 2` (driven: sets `app+0x4dc` from the time field `self[6]` cubed, takes a position from `E20SplineField::Eval2` (0x49fad0), and corrects it along a normal when it lies behind the plane); (c) otherwise a fixed matrix from `E12_BuildMatrixA` (0x474420).
Init: `E12_Cam::InitX` (0x475150), which sets flag 2 and rebases on
`Eval1` (0x49fab0). The branch details are **source** (`RBR_E12_Tic.cpp`, "50%");
the three-way structure is **loader**.

### 5d. Replay camera cut and the car-flip camera

**Correction (cameras-2, loader):** the camera init (0x4881c0) binds `0x486760` to the `ReplayCameraChange`
connection (host `+0x54`, function at `+0x68`): it stores the payload's record (through 0x476e90) at `+0x3e8`, the
`.dls` camera record of section 5b, and calls 0x5f72a0. `FUN_00487980` is bound to the
`InitSpectatorFlipAnimation` connection (host `+0x84`, function at `+0x98`), so it is the car-flip spectators'
camera, not the replay cut. Its first word 1..3 is then the flip code (`notes/scripted-events.md`; **inferred** from
the three offsets). Its code:

- If the second word is 0: it places a camera from the car position (`NPhysics::CCar::Split17C`
  on the car object at `0x8ef660`), a driveline frame (`FUN_006015f0`), and three fixed side
  offsets chosen by the index (constants at `0x72f580..0x72f59c`, index 1..3).
  The offset is pulled toward the car while a segment test against the track
  fails (`PTR_DAT_007c755c`, not identified). It then composes the matrix into
  `+0xcc` and sets mode 13 (`M487680(0xd,1)`), remembering the old mode in
  `+0x04`. This is the frozen track-side camera of the car-flip scene.
- Otherwise: `M487680(second word, 1)`.
- **Scripted-2 (asm):** the driveline point comes 1.5 m along the driveline (0x72dcdc); the frame is `side = dir x
  up`, `ahead = up x side` with `up = (0, 0, 1)`; the eye offsets per code are `(-6.63, 2.668, 0.967)`, `(3.709,
  3.709, 1.041)`, `(-3.919, -4.49, 0.86)` (across, along, up). The probe loop (3 times): if the segment from the look
  point to the eye hits, and the reverse hits with `1 - t1 > t2`, the eye goes to the two hits' middle; then a probe
  from 10 m along the view-perpendicular up down to the eye puts the eye on the ray to that hit at its old distance,
  0.1 m up. Last, a vertical probe +-10 m: `eye.z = max(eye.z, hit.z) + 1.0`. The matrix is `E11_MatCompose(+0xcc,
  eye, look, +0x2d0)`. Tic case 0xd sets the FOV to 75. Ported in `rbr-render` `characters::scripted::flip_camera`
  with the probes on the stage `.col` (`stage_probe::StageProbe::segment`, ghosts-2). Not checked against a game
  capture: the reading of the probe loop is scripted-2's.

The car-side listener of the same signal is `CCarSound` (**string**, sound).

Trigger kind 13 (replay camera change, `formats/stage-files.md`) is not traced
from the table-2 trigger data to the emit (Open).

### 5e. Table 3 mode 2 and the far factor

Because `CSigCamera +0x50` is set for mode 7 records with word 4 bit 2 (which is
mode 2 under the bit mapping above), mode-2 records change the renderer's far
factor to 1.0 (HH `m_unk650`). That path is **inferred**: the bit mapping comes
from the Rust doc and the flag is read by HH only.

## 6. Helicam: `.dls` table 5 and the rescue helicopter (mode 8)

### 6a. `.dls` table 5 (**file**)

Present in 4 of 86 files, 10 records: `track-56_M`, `track-56_O` (1 record each,
trigger 51172) and `track-62_M`, `track-62_O` (4 records each, triggers 57534-57537).
Each record is `{trigger id, name offset, two raw words, three floats}`. The
loader resolves the trigger id (a miss fails the load, `FUN_00479020`,
**loader**); whether the game uses the floats is not traced. The exporter says
table 5 is unused by the game. The loader does read the trigger ids, and no
traced code reads the floats (**inferred**).

### 6b. Rescue helicopter camera (mode 8, **loader**)

`CRescueHelicopter::Activate` (0x49bc90) sets mode 8 (`M487680(8,1)`), places the helicopter on the driveline at the car's distance less a fixed offset (`0x72f42c`, 30.0), and runs `E20TestVolume`. Its
`FillMatrix` (0x49bed0) samples the driveline at `(m_base+0x44)`.
`M5010` (0x485010), mode 8, runs only while `CRescueHelicopter::IsActive` is
true: it reads the helicopter matrix through the helicopter's virtual slot
`+0x24` and composes it into the host's `+0xcc`. `CPC_RescueHelicopter::Vf18` (0x452290) calls `Activate`; the signal behind it is not traced. The helicopter is presentation: its only gameplay effect is the cycle
block in 2b.

## 7. Fly-in camera (mode 11, `M8690` 0x488690, **loader**)

No code in the exe sets mode 11 directly: of the 41 `call 0x487680` sites (scanned in the 1.02 `.text`), none
pushes 11, and the register-argument callers pass 1, 4, 8, 10 or a menu/replay value (`0x566340`, `0x566390`, the
replay handler `0x487980`, **loader**). So the 1.02 race start has no fly-in; mode 11 can only come from a replay
payload (**inferred**). The start camera is mode 10 (section 7a).

Advances `+0x410` by `dt * 20.0` (`0x72f614`), samples the driveline there (`CDriveLine::Sample`,
`DAT_01659184`) into `+0x250`, applies the orbit swing (`+0x268`, section 2b
formula) and composes the matrix into `+0xcc`. Its FOV is `DAT_01660db4` (see
Open) and near is 0.8 (`0x72f610`). When the distance from the point at
`+0x3c0` is at most `CAM_RADIUS`, it switches to mode 1. Callers of mode 11 are
not traced (the race start is **inferred**).

## 7a. Start orbit and the go (mode 10, `M8840` 0x488840, **loader**, asm checked)

When the load settle ends, `M004801d0` sets application state 10 and, while the game flow's `0x565e80` is true
(flow states 1-5 and 8), `M487680(10,1)`, which in state 10 sets the swing angle `g_1660da0` to pi (`0x72f414`).
Each frame `M004825b0` state 10 calls `CCamera::Tic(facc)`; its first call `M8840` does, in mode 10:

- state 10 and (flow state 1 or 2, or `E19IsState1`): `angle -= facc * 1.2` (`0x72f680`, SSE f32). Below 0:
  `M487680(1,1)` (mode 1, the chase orbit) and `M0047ca40`, the go (countdown `m_unk714 = -1`, armed). The next
  frame's `Tic` takes `facc` off it and starts the driving (state 1).
- otherwise: `angle -= facc * 0.3` (`0x72afcc`): a slow orbit with no go.
- then the angle wraps into `[0, 2 pi]`. In every other mode `M8840` sets it to 0.

The angle turns the chase offset (`+0x108`, scaled by `cos(CAM_ANGLE) * CAM_RADIUS`) about the physics up axis
through the 3x3 `[cos, sin, 0; -sin, cos, 0; 0, 0, 1]` (`0x483c90`); at pi the camera is in front of the car, at 0
behind it. The go comes after `pi / 1.2 = 2.618` s of frame time: 375 frames of 7 ms in f32.

Rust: `rbr_gameplay::camera_go` (the timer), `Game::with_camera_go` (live play from the stage start), CAM1 record and
`rbr_render::views::start_orbit` (the viewer). No recording shows the game's timing: FRM1 records only `StepForward`
calls, and state 10 runs none, so the recorded runs keep the host's go at the recording's step.

## 8. Head and body movement (TrackIR, `CSigIncarShake`)

### 8a. TrackIR (**loader**, E29)

- `UseTrackIR` is read from `[Settings]` in `CPC_Application::Vf04` (0x44c890).
  If set, `E29AllocTrackIR` (0x4cd610) builds the binding (`E29TrackIRData::Init`
  0x4cd5c0: scale `0.78539819` (pi/4) at `+0x38`, `0.17453294` (10 deg) at
  `+0x3c`, enabled).
- The frame tick (0x44a3d0) calls `E29TrackIRApi::Sample` (0x4cd750) and emits
  `CSigTrackIR` with the result (`E04TrackIRSignal`, slot `+0xe008`). `Sample`
  reads the NPClient data through the window object (`+0x18`), skips the update
  when the frame stamp (`+0x30`) is unchanged, scales the two axes by
  `DAT_007349ec` (8.1387e-5) and `DAT_007349e8` (1.2207e-4), clamps both to `[-1, 0]` as the decompile
  reads them (one-sided; possibly a decompile artefact, not checked), and
  multiplies by the window object's `+0x38`/`+0x3c` scales. It returns 1 on a new frame, 0 otherwise (and zeroes the output).
- The camera's use of `CSigTrackIR` (vf04 0x489ac0) is not identified (Open).

### 8b. `CSigIncarShake` (**loader**, E45)

`NPhysics::E45_P3210` (0x523210) packs six `vec4` from the seat-spring state of
the passenger (`CPassenger`, `physics/jointed-bodies.md`): `v[3]` from `+0x70..`,
`v[2]` from `+0x80..`, `v[1]` from `+0x90..`, `v[4]` from `+0xa0..`, `v[5]` from
`+0xb0..`, and `v[0].x = 0`. It emits it once per seat. LocoMotion uses the same
signal for the driver's body (`stage-characters.md` section 7). The camera's handler is
`0x484970` (bound by 0x4881c0, **loader**): when payload word 0 is 0 (always, `E45_P3210` writes 0) it copies
payload `+0x30` (`v[3]`, the seat's host `+0x70`: the seat spring's displacement, `CPassenger::spring` lanes 28-30)
to host `+0x4e0`, which modes 4 and 5 add to the camera's place in the car. Only the driver's seat emits it (the car's
`Vf28x` calls `0x523210` on `CCar+0x8b0`, whose host is `CCar+0x1450`), so the shake is `CCar+0x14c0`, in every FRM1
car block. On `drive-tanner-corolla` it is a few mm sideways and lengthways and about 12 mm down at rest.

## 9. Camera settings (`CAM_RADIUS`, `CAM_ANGLE`, `VW_ANGLE`)

Three runtime floats, all in BSS, set at run time (not in the image):

| Global | Setting | Slider (`CGameMenuDaemon::M56da50` 0x56da50, **loader**) | Used by |
|---|---|---|---|
| `0x1660d94` | `CAM_RADIUS` | 3.0 to 7.0, step 0.1 | modes 1/10 (`M6530`), 11 (`M8690`), 9 (`M5090`, rate x10, source) |
| `0x1660d98` | `CAM_ANGLE` | 10 to 16, step 1 | modes 1/10/11 swing |
| `0x1660d9c` | `VW_ANGLE` | 9 to 12, step 0.2 | modes 1/10/11 swing |

Orbit swing (`M6530` 0x486530, **loader**): with `a = CAM_ANGLE`, `b = VW_ANGLE`
(both in radians, times 0.01745), the swing `d = CAM_RADIUS * sin(b) /
sin(3*pi/2 - b + a)` (`0x72f434` = 4.712389) is subtracted from `+0x268`, the
matrix is composed, and `+0x268` is restored. Mode 11 uses the same `d`.

`SCameraSettings` (menu settings struct, `InitCAM` 0x587a90, **loader**) fills
words 3 to 13 with `4.4, 13.0, 11.0, 0, 0, 1.0, 0, 0.4, 75.0, 3.0, 0.8` (size
word 0x38, next pointer `0x1660dc0`) and reads tag `0x43414d45` ("EMAC")
(`Read` 0x587e70). `CGameflow::ReadCameraSettings` (0x567e90) checks tag `0x43414d50` ("PMAC") and copies four words (one at `+0x17840`, three after) into the gameflow host.

**Confirmed at run time** (ghosts-2: `vcu-cov.snap-1000.bin` holds 4.4, 13, 11, 0, 0, 1.0, 0, 0.4, 75, 3.0, 0.8 at
`0x1660d94..0x1660dbc`): the eleven words are the globals `0x1660d94..0x1660dbc`
(`CAM_RADIUS` 4.4, `CAM_ANGLE` 13, `VW_ANGLE` 11, `0x1660da0` the swing 0, `0x1660da4` a flag 0, `0x1660da8` the pad
1.0, `0x1660dac` the debug flag 0, `0x1660db0` 0.4, `0x1660db4` the FOV 75, `0x1660db8` the minimum distance 3.0,
`0x1660dbc` the lag 0.8). The player sets the first three on the camera page; Rust: `game_camera::Settings::GAME`. The viewer's chase takes
our framing (`Settings::default`: 5.5 m, 16 degrees, -2 degrees tilt) with the player's per-car nudges
(`rbr_render::camera_prefs`, user decision 2026-10-18: no camera-page field in the launcher). The settings object lives in the
menu settings (`+0x590`, built at 0x5893e0), and its pointer `0x1660dc0` is the change-detect copy that vf08
(0x5866a0) writes and vf10 (0x5866e0) compares. No code in the image writes the globals except the menu sliders
(0x56db53, 0x56dc02, 0x56dcb1), so their race-time values need a run-time read.

### 9a. The chase eye (`M8840` 0x488840, modes 1, 2, 10; asm read)

In the z-up physics frame, with `T` the look point (`M6620`: the body position plus the body axes times `+0x420`),
`Y` the body's rear axis (host `+0x108`), `r = CAM_RADIUS`, `a = CAM_ANGLE` (mode 2: `2r`, `0.6a`):

1. near = 0.1. The swing angle (section 7a).
2. The downhill lift `DAT_007eacf8` eases at `dt` per second to `5 Y.z` (0 when `Y.z <= 0`), within 0 to 2.5.
3. The wanted eye `D = T + rot_z(swing + look-back) (Y r cos a) + (0, 0, r sin a)`. The look-back `+0x2e8` eases
   at `2 dt` to pi while `+0x2e0` is set, else on to `2 pi` and to 0 within 0.0209.
4. Collision probes push `D` (visuals-2, asm 0x488c49..0x489190; `seg(a, b)` is `CCollisionSystem::Vf00` 0x4fe810
   through `PTR_DAT_007c755c` = `0x16578a8`, the stage collision system; its record is the point, the unit normal
   `e2 x e1` at +0x10 and `d` at +0x1c; `|n.p + d|` is a point's plane distance). With `R = min(r, 3.25)` (0x72f678)
   and `onto(p) = T + normalize(p + n |n.p + d| - T) R`: if `seg(E, D)` hits, `P = onto(D)`; else if `seg(T, D)` hits,
   `P = D`; else `+0x270 *= 0.7` (0x72f674) and skip to the end. Up to 5 times while `seg(T, P)` hits: `P =
   onto(P)`. If the fifth still hit: up to 6 times while `seg(T, P)` hits, `P += n (|n.P + d| + near + 0.2)`; if the
   sixth still hit, `P = hit + n (near + 0.1)` when `|hit - T| < R`, else `T + (hit - T) R / |hit - T|`. Then
   `+0x270 = push(P, near + 0.3) - D`, where `push` is 0x4fe5e0: every face of the tree leaves whose boxes meet the cube
   about the point moves it out to the radius along its normal when it is in front of the plane and nearer
   (`0 < s < radius`), whether or not it lies over the face. End: `D += +0x270`.
5. While `0x1660da4` or the snap flag `+0x08` is set: the eye `E` (`+0x2a0`) = `D`, done.
6. The eye before this step is kept (`prev`). The lag: a 64-frame ring (`+0x138`, from 0.02 each) of `dt / 0.8`; `k` = its mean, at most 1.
7. The height window about `h = r sin a + T.z + lift`: `E.z` is held within `h + floor` and `h + ceiling`; the floor
   (`+0x2f0`, from -0.2) eases at `dt` to -0.3 while holding, else -0.2; the ceiling (`+0x2ec`, from 0.3) to 0.4 while
   holding, else 0.3. Then `E += (D - E) k`. Instead of this step, while `|+0x270|^2 > 0.01` (0x72f440): `Q = E +
   (D - E) 2 dt`, up to 6 times while `seg(Q, E)` hits `Q = hit + n near`; `E = Q`, the ceiling 1.2 (0x72f680) and
   the floor -0.6 (0x72f66c).
8. The horizontal distance from `T` is held between 3.0 (`0x1660db8`) and `r + 1.0` (`0x1660da8`).
9. More probes: if `seg(T, E)` or else `seg(prev, E)` hits, `+0x280 = n (|n.E + d| + near + 0.1)`; else if
   `push(E, near)` moved it, `+0x280` is the move; else `+0x280 *= 0.8` (0x72f610). `E += +0x280`.

`M6530` then looks from `E` at `T` raised by `-r sin b / sin(3 pi / 2 - b + a)` (0.84 m) with up `(0, 0, 1)` and FOV
`0x1660db4`. Rust: `rbr_render::game_camera` (all steps; the probes on the stage `.col` in `rbr_render::stage_probe`).

The look-back flag `+0x2e0` is written only by the camera's constructor (0x489d3b, zero): no action in the 1.02
action map (`CH_CAM`, `PAUSE`, the driving actions) sets it, so the look-back swing is dead code in the game.

## 10. Field of view and clip planes

- FOV is kept in degrees in the host (`+0x2c0`), set by the car camera (`FOV`
  rad x 57.3), by table 3 word 2 (degrees), by the orbit and fly-in modes
  (`DAT_01660db4`, runtime, not traced), and by mode 13 (75.0). `CSigCamera`
  gets radians.
- Near is `+0x290`: table 3 word 5 for look-at -1/0/trigger modes, `0.8` in
  mode 11, and car `Near` only as a stored value (`CSigCarCameras`).
- Far is `+0x294`, set to 300.0 (`0x72f5e0`) by the camera init (0x4881c0). Other writers are `E11_Set294` (0x473df0) and 0x477d80, not traced to the host. HH uses `far = m_unk24 * CSigCamera+0x48`.
- HH applies FOV x0.75 and near clamped to at least 0.1 (**source**,
  `HH_World.cpp:356`). That is rendering only.

## 11. Where the camera position goes

| Consumer | How | Gameplay? |
|---|---|---|
| HedgeHog3D render camera | `CSigCamera` (`CHH_World::OnCamera` 0x10066dc0), `CSigCameraModeChange` (0x10066d20) | no (render) |
| `CPC_GraphicsDaemon::OnCamera` | `CSigCamera` (0x4c0970) | no (render) |
| Particle systems (rain, dirt, emitters) | `CSigCamera`, `CSigCameraModeChange`, `CSigReplayCameraChange` (string) | no |
| `C3DMixerBase` | `CSigCamera` (string) | no (sound listener, inferred) |
| Bird and ambient spheres | `E31_5d00` (0x4d5d00): runs only in application state 8; takes a point from `0x474f00` (the E15 host's `M74f00` on the `+0x3e8` object; inferred to be the camera's track point) and calls `E12_Spheres::Hit` (0x477310); then plays a sound through `E31`. The exporter's 70 km/h gate is not identified; the code compares a float against the constant at `0x735c2c`. | no (sound) |
| Physics | none. Camera code reads the driveline (`DAT_01659184`, `CDriveLine::Sample` 0x50b180), the car (`CCar::Split17C`), and the physics definition `g_1659b70`, all read-only. | no |

## 12. "Camera ROTO" clips (`.sga` / `.sgs`)

**file**: 23 retail clips contain a `Camera ROTO` node (`SGA/anim2_REDBULL_*`,
`anim2_prize*`, `anim2_loser_*`, `anim2_Ps2_REDBULL_*`; the list is from a binary
grep of the retail `SGA/` and `SGS/` folders). They are scene clips for the Red Bull can
challenge, podium and loser scenes (names). Their camera nodes are bound through
LocoMotion (`formats/sgs.md`: (8,31) position and (10,0)/(11,24) roll tracks).
Nothing in the 749 retail `Scripts.rbz` `.lua` files mentions a camera (**file**),
and no traced exe path connects them to the host (not fully traced). Presentation only (Open).

## Open

- What the `TrackIR` (0x486d00) and `Input` (0x486880, nine words to `+0x2f4`) handlers feed.
- Mode names: the exe has no mode-name table. `Spectator`, `Helicopter` and `Change camera` appear as strings but are not tied to mode numbers. Names here are ours.
- `0x1660d94..0x1660dbc`: the defaults are inferred from the layout (section 9); no run-time read.
- Whether the snap flag `+0x08` is cleared after the start orbit's go (10 -> 1 keeps it set).
- The car-flip camera (`FUN_00487980`, section 5d): the payload's second word, and when mode 13 ends.
- What `E20TestVolume` does. (`PTR_DAT_007c755c` is the stage collision system, section 9a.)
- Trigger kind 13 (`.dls` table 2) to the replay change emit.
- The 70 km/h bird gate (`0x735c2c`).
- What the other `CSigIncarShake` fields mean (the camera uses `+0x30`, section 8b).
- `Cam_backseat`, `Cam_external`, `Cam_front_*` (car `.ini`, and `.sgc` kind 2/11 nodes): not loaded by `E09_LoadCarCameras`. Their consumer is not traced.
- Table 5 floats: not consumed by any traced code.
- The meaning of table 3 word 3 beyond zero/nonzero, and whether word 4 bit 2 (mode 2) matters beyond the HH far factor.

## Relevance

Presentation. The camera host, its modes and car cameras only feed rendering,
sound and particles. The camera makes no write to physics. The camera-side
use of TrackIR and `CSigIncarShake` is not traced. The replay cut's mode
choice is a camera state only.

## Other systems seen

- `CRescueHelicopter` (crash rescue helicopter, presentation): `Activate` 0x49bc90, `IsActive` 0x49bea0, `FillMatrix` 0x49bed0, `CPC_RescueHelicopter::Vf18` 0x452290.
- Car flip spectators (`CSigInitSpectatorFlipAnimation`): `E03CarFlipInit`, connection at 0x4899a0 (camera side).
- Bird and ambient sphere sound: `E31_5d00` 0x4d5d00 (section 11).
- Driveline sampling `NPhysics::CDriveLine::Sample` 0x50b180 (also physics).
- Exhaust point loop in `E09_LoadCarCameras` (`g_e09ExhaustNames`, 0x469210).
- E20 spline shell `Tic` / `Blend`: 0x49f6e0 / 0x49fbc0.
- Menu camera page `CGameMenuDaemon::M56da50` 0x56da50 and `SCameraSettings` 0x587a90 (menus, out of scope).
- Application state machine `CApplication::M004825b0` 0x4825b0 (general state, not only camera).
