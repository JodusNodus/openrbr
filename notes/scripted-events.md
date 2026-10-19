# Scripted world events (car-flip marshals, rescue helicopter, bird strike, crash sites, start and finish)

How the 1.02 game triggers and runs the scripted set pieces that sit beside the
ordinary stage characters (`stage-characters.md`). Addresses are
`RichardBurnsRally_SSE.exe` 1.02 unless marked HH or LM. Labels: **loader** =
read from the decompile or our reconstruction; **file** = checked on retail
data or the Scripts archive; **inferred** = plausible, not proven. The
character descriptor and `.lbs` item layout are in `stage-characters.md`
sections 1-2 and `formats/lbs.md`.

## Summary

Five things, each with a different loader:

- **Car-flip spectators** are not stage items. The exe loads five
  `load_car_flipN.lua` characters itself at session start (`E03CarFlipInit`,
  `0x445380`). They play when the car raises the `CSigOutsideAssistance`
  signal and a 1.0 s HUD timer runs out. They are cosmetic: they do not move
  the car. The car's pose is changed separately by a teleport (section 2).
  The race-time penalty (10, 20 or 35 s) is gameplay.
- **Rescue helicopter** is also exe-loaded (`load_rescuechopper.lua`, string
  in the exe). It is started by a menu gate that needs race time >= 2.0 s and
  the flip window closed. It flies in, hovers for 24 s, then posts a menu
  message. Presentation, plus the menu gate.
- **Bird strike** is a stage item (`load_birdstrike` plus `run_birdstrike`)
  with a 1-in-200 spawn roll. The strike signal `CSigBirdstrike` comes from a
  state check in the particle manager. The bird's position comes from a global
  that the exe never writes (section 4). Presentation, with a signal that the
  sound and particle code consume.
- **Crash sites** have Lua scripts but no retail use: no stage item and no exe
  string refers to them, and their models are not in the install.
- **Start and finish** are stage items (`load_start_race01b/c`,
  `load_end_race01a/b`, 57 stage files each). The start countdown is a HUD
  timer (7.0 s) that fires one trigger at 1 s and sets the race clock running
  at GO. The race clock is gameplay.

## 1. Data sources

| Source | Content | Evidence |
|---|---|---|
| `Scripts.rbz` `Scripts/load_car_flip1..5.lua` (+ rally folders) | flip spectator model, flip clip | file (6 rally sets: root `ho_`, `arctic/ar_`, `canberra/au_`, `montblanc/mo_`, `pirelli/br_`, `ramada/us_`; `hokkaido/` = `ho_`) |
| `Scripts/run_car_flip.lua` | `Update` always `return 0, state`; `Trigger` no-op | file |
| `Scripts/load_rescuechopper.lua`, `run_rescuechopper.lua` | one heli `rescuechopper.sgs`, clips flyin (0), stretcher (1), idle (2) | file |
| `Scripts/load_birdstrike.lua` (`LuaRandom(1,200) > 1` returns 1, i.e. skip) | `bird_03.sgs`, clips start / animate_a / animate_b | file |
| `Scripts/run_birdstrike.lua` | state machine that calls `app:PlaceCharacterAtCar`, `app:Birdstrike` | file |
| `Scripts/load_crash*.lua` (12 root, 17 per rally folder with crash scripts) | `crash_xsara_0N.sgs`, `crashdriver_male_N`, `mechanic-chief_01` | file |
| `Scripts/load_start_race01a/b/c`, `load_end_race01a/b`, `run_startrace01c`, `run_end_race01a/b` | time card marshals, countdown marshal, finish marshals | file |
| Stage `.lbs` items | `material_a`/`material_b` names (uppercase, `.LUA`) | file (see Survey) |
| `SGS/`, `SGA/` in `retail-1.0/install` | flip, countdown, bird-strike, heli models and clips present; no `crash_*` or `crashdriver*` files | file |

## 2. Car-flip marshals

### 2.1 Load (session start)

`E03AnimImpl::Startup` (`0x445910`) runs after the Lua state opens and calls
`E03CarFlipInit` (`0x445380`) (loader). It does this:

1. Skips the whole init when the race host state (`g_e01RaceHost` vtable
   +0x4c) is in the list at `0x72ace0..0x72ad34` (loader; the list is not
   decoded here, see Open).
2. For `i = 1..5` loads `load_car_flip<i>.lua` with `run_car_flip.lua` through
   `E03AnimImpl::LoadAnim` (`0x445090`) and stores the descriptor in impl
   +0xcc + 4*(i-1) (loader). The loaded descriptor gets its LocoMotion
   character's `pelvis` node id (impl +0xe0 + 4*(i-1)) (loader; use not traced).
3. Sets the descriptor's active byte (desc +0x75) to 0 (loader).

The five descriptors are loaded by the exe, not from stage items. Zero
stage `.lbs` files name `load_car_flip` (file, 88 files).

### 2.2 Clips and models

- Model: `load_car_flipN.lua` calls `app:LoadCharacter` for one of four
  spectator models chosen by `LuaRandom(1,4)`, scale `.001` (file). Root
  scripts use `ho_spectator_00..03`; the rally folders use their own set
  (`ar_`, `au_`, `mo_`, `br_`, `us_`), so the spectator is rally-specific.
  Some root variants pick the same model twice (`load_car_flip2.lua` picks
  `ho_spectator_01` for values 1 and 2) (file).
- Clip: `anim_flip_car_01..05.sga`, one per script, added as clip 0 (file).
- In the rally folders `canberra`, `arctic`, `montblanc`, `pirelli` and
  `ramada`, the script also loads `anim_ultra_idle_01.sga` as clip 1
  `end_idle` (file). `run_car_flip.lua` never returns clip 1, so that clip is
  not played by the stock run script (file + inferred from the script text).

### 2.3 Trigger chain

1. **Car raises `CSigOutsideAssistance`.** `CCar::Assist8DF0(a, b, c)`
   (`0x4e8df0`, `src/rbr/RBR_E34_CarC.cpp:789`) emits
   `CSigOutsideAssistance` at `g_signalManager + 0xd378` (`RBR_E34_CarC.cpp:811`) when `a != 0`
   (loader). The payload is one int, `code`:
   - `1` when `m_tPartialContact > 1.0` (impl +0x9f0 flag);
   - otherwise `3 - (Test1670() != 0)`, which is `2` when the override
     `m_tOverride` (`c`) is positive (loader).
   Callers with `a = 1`: a wall-contact branch (`RBR_E34_CarD.cpp:339`), a
   `CSigCarProblem` branch (`RBR_E34_CarE.cpp:257`) and the retire announcer
   `E34_RetireA` (`0x4e8fa0`, `RBR_E34_CarD.cpp:975`, with `b = 0.5 s`,
   `c = 4.0`). `E34_RetireA` with `a = 0` (`b = 30.0 s`) is the no-emit path
   (loader). The game conditions behind each caller are in section 3.1.
2. **HUD reset and flip timer.** The HUD (`E62GameMenu`, reconstruction
   `RBR_E62_Small.cpp:640`) connects a handler `M573900` (`0x573900`) to the
   bus at manager +0xd580 (loader, HUD ctor `0x5799f0`, handler slot +0x7c8).
   That bus is the same list the replay car uses for its OutsideAssist
   connection (`RBR_E93_Car.cpp:171-179`, comment "OutsideAssist"). The
   handler stores `*payload` at HUD +0x278 and zeros the timer at HUD +0x270
   (loader). Each emit restarts the timer. `+0xd580` is the `+0xd378`
   emitter's connection list (+0x208): the peeks show HUD +0x278 holding the
   emitted code on the emitting step (section 3.1).
3. **Flip gate.** `E62GameMenu::M576710` (`0x576710`) runs from the HUD tick
   `0x579df0` every frame and adds dt to +0x270 (loader):
   - timer crosses **1.0 s**: emit `CSigInitSpectatorFlipAnimation {code, 0}`
     (start) (`0x576710`, the `DAT_00795e14` branch);
   - if the race clock gate (`+0x144 == 0`) is set: add the penalty `t` to the
     race clock +0x140 and to `CGhostCarManager::Accumulate(t)` (loader);
   - timer crosses **4.5 s** (`DAT_0073b9b0`): emit `{code, 1}` (end);
   - between 3.5 s and 4.5 s the HUD fade value +0x274 rises to 1.0 (loader,
     presentation).
   The window gate `E62_M573920` (`0x573920`) is true for `0 <= timer < 8.5`
   (loader; constant `8.5` at `0x746890`, verified in the 1.02 image).

**Penalty values** (loader, `0x576710`): `t` is 10, 20 or 35 s.
- When the mode query `E59Host90::M00565470()` (`0x565470`) returns 3 or 5,
  `t` depends on `g_79host28 + 0x8e84` (0 -> 10, 1 -> 20, 2 -> 35).
- Otherwise `t` depends on the global `0x1660850` (0 or 1 -> 10, 2 -> 20,
  3 -> 35).
The penalty is applied only when +0x144 is 0, which is set at GO (section 5).

### 2.4 The flip handler (what the spectators do)

Connection vtable `0x72b128` (`CSigInitSpectatorFlipAnimation`) is set up in
`CPC_AnimationHandler::Impl` (`RBR_E03_Handler.cpp:347`). Its target is the
function our symbols name `E01Cam::ApplyState` (`0x43d550`,
`src/rbr/E01_Camera.cpp:59`). That name is wrong for this use: the body is the
flip handler, and the camera name is a leftover label (loader). The handler
(`mode = {code, flag}`):

- **flag == 1 (end):** for each of the five descriptors with the active byte
  set, release the LocoMotion character (vf `+0x58` with 0) and clear the
  byte (loader).
- **flag == 0 (start):**
  1. Reads the car's orientation and position from `NPhysics::CCar::Split17C`
     (`0x4e5530`; physics call, read only).
  2. Probes the ground under the car with the collision callback (`0x7c755c`).
     If there is a hit, the spectators go to that point; otherwise to the
     car's own point (loader).
  3. Angle `ang = acos(-fwd.x)`, negated when the car's up-side sign is
     positive (loader).
  4. Chooses the group: `code == 1` (or 0) uses descriptors 0..2 (`base = 0`);
     `code == 2` uses 2..4; `code == 3` uses 2..4 with `ang += pi` (loader).
  5. For each descriptor in the group: set the LM character's animation
     (`vf +0x4c`, 1), translate to the probe point (`+0x58`), rotate about Y
     by `ang` (`+0x5c`), reset the clock (`+0x44`, 0), show the character
     (`+0x58` on the holder) and set the active byte (loader).

So the handler **places and rotates the spectators** at the car. Nothing in
the handler writes the car's transform. The car is only moved by the
teleport in section 3.

Descriptors 0..2 and 2..4 overlap at 2. The three group members play three
different flip clips at the same spot (inferred from the loop; the clips are
`anim_flip_car_01..05`).

## 3. Does the flip right the car? Teleport, not physics

Two separate mechanisms, both in the car:

- **Countdown worker** `CCar::Vf98x` (`0x4e6dd0`, `RBR_E34_CarB.cpp:493`;
  vtable slot 38). If the car's countdown +0x15e4 is 0, it samples the track
  under the car (`Sampler54A0`), calls `CCar::Vf184` (slot 97, `0x4e5270`)
  with the probed point and the forward vector, then calls the state update
  (`S48`). Otherwise it subtracts the app frame time from +0x15e4 and clamps at
  0 (loader).
- **Pose set** `CCar::Vf184` (`0x4e5270`, `RBR_E34_Quat.cpp:446`) builds a
  basis from a point and a direction and writes the car's position and
  quaternion directly. It is not an integrator step (loader). It is a teleport.

`Assist8DF0(1, 0.5 s, 4.0)` (section 2.3) arms the countdown at 0.5 s
(`0x3f000000`, `RBR_E34_CarC.cpp:795`, loader). So the car is re-placed on
the track about 0.5 s after the event. The spectators start at 1.0 s and
end at 4.5 s.

Answer to the question: the flip marshal animation does not right the car.
The car is put back by a direct pose write (teleport) from the countdown
worker, and that is the righting the player sees. The countdown is physics
time (`CPhysics::StepForward` calls slot 38 on each car every step and it takes
`MaxTimeStep` off), so 0.5 s is 72 steps; the spectators' 1.0 s is HUD time,
so they appear around the car already put back (their handler reads the car's
pose then). The HUD's time penalty (section 2.3) is the gameplay effect.

At 0 the worker takes the driveline point and direction (`Split17C`), negates
the direction, probes the ground +-10 m (`Sampler54A0`), and calls slot 97
(`0x4e5270`): the pose is the ground point + 0.4 m along the up axis; with the
flip flag (+0x9f0, set by `Assist8DF0` when the car had partial contact for
more than 1 s) it is turned upright and raised 0.4 m more, and the flag is
cleared. Then `CCar::Vf180` (`0x4e9660`, slot 96, `RBR_E34_CarD.cpp:153`)
writes the pose, clears the velocities and emits `CSigRespawn`
(`EmitFwd9600`, `0x4e9600`; its receiver is the ghost, `0x493c80`).

### 3.1 What puts the car back (triggers)

Checked 2026-10-16 with three recordings and HUD peeks (`drive-rcv-*`,
`rust/rbr-gameplay/README.md`, Race clock); `rbr-standalone --boot` runs each
equal to the original to step 3000. `Assist8DF0(1, 0.5, 4.0)` has three
callers. Nothing rights a flipped car on its own.

- **The player's call for help.** The pause menu item HELPCALL (row id
  `0x25`, callback `E61_M0056e6d0`, `0x56e6d0`) emits
  `CSigRespawnRequest(1)` (signals +0xd160) and resumes. Its predicate
  `0x56a620` offers it when the assistance timer is outside `[0, 8.5)`, the
  rescue helicopter is idle, `*0x166e378` +0x844 <= 0 and the race clock is at
  least 2.0 s. `E16Quant::NotifyRespawn` (`0x48d080`) sets quantized flag
  0x20, which the next substep's payload carries once as 0x10.
  `CCar::UpdateSeed` (`0x4e91f0`, outside replay state 2) then runs
  `CCar::MotionProbe` (`0x4e7730`): a car faster than 1.5 m/s, or one the probe's
  sampler gives a verdict for, gets `CSigCarProblem` (HUD message, `MSG_CARSTOP`
  when too fast); otherwise `Assist8DF0`. `drive-rcv-help-corolla-34`: the car
  on its side at step 1956, code 1, put back upright 72 steps later.
  `drive-tarmac-lancer` (a human drive) holds the same request at step 20343.
  The request 2 (`E65_EmitRespawn`, `0x5826c0`, payload 0x20, the reset branch)
  has no caller in the exe, and the input manager has no respawn action.
- **Stuck against a brake wall.** `CCar::Vf6cx` adds time to +0x9ec while the
  brake-wall penetration is above 0.5 with contact. `CCar::Update9AF0`, given
  the time keeper's still count (nonzero only on the 100-step samples with
  less than 4 m of progress), acts when the timer gap is at least 5 s and
  +0x9ec at least 1.0: outside the flow gate (`0x566640`) it retires (4);
  otherwise, unless `S8C`, `Assist8DF0` and +0x9ec = 0.
  `drive-rcv-stuck-206-64` step 2700, code 2.
- **A script character's box** (collision material 200): `CSolid` (call at
  `0x52c299`) calls `E34_RetireA`: `CSigRetireCar(3)`, then `Assist8DF0(1,
  0.5, 4.0)` when the flow gate `0x5665f0` holds, else `Assist8DF0(0, 30.0,
  4.0)` (no emit, no penalty). `drive-rcv-retire-impreza00-45` step 2144,
  code 2. The two flow gates are true unless the flow state is 3 and
  `*0x7d6128` +0x8e84 < 2.

A flipped or stuck car outside these is not put back: `Update9AF0`'s other
branch (partial contact over 8 s, or the still count above 5 with the damage,
drive or engine tests failing) only emits `CSigCarStandStill` (+0xcb18), whose
HUD handler (`0x5735b0`) shows a message for counts above 5 (`+0x138`: 1
`MSG_DAMAGE_PC`, 2 `MSG_WRECKED_PC`, 3 `MSG_STUCK_PC`, 4 `MSG_ASSIST_PC`, 5
`MSG_CARSTOP`, 6 `MSG_CARSTUCK`).

## 4. Rescue helicopter

### 4.1 Load and clips

`CPC_RescueHelicopter::Vf18` (`0x452290`, `RBR_E05_Heli.cpp:108`) calls
`CPC_AnimationHandler::LoadRescue` with `load_rescuechopper.lua` and
`run_rescuechopper.lua` (loader). The character is loaded with
`LoadCharacterNoCull` and `scale = 1.0`; `CreateCollisionMesh` returns 0 (file).
Clips: 0 `flyin`, 1 `stretcher` (takeoff), 2 `idle`. Assets are present
(`rescuechopper.sgs`, `rescuechopper_fly_in.sga`, `rescuechopper_takeoff.sga`,
`rescuechopper.sga`) (file). The run script plays clip 0 once, then clip 1,
then clip 2 forever (file, from `run_rescuechopper.lua`, via `Update` with
state 0 -> 1 -> 2).

After loading, the exe sets two emitter tracks named `default` and `Stretcher`
on the character, and a random heading (`rand() * 0.000191753`) (loader).

### 4.2 Trigger

- **Gate (menu).** Menu row id `0x25` is registered with the predicate
  `E60_56a620` (`0x56a620`, `RBR_E60_MenuA.cpp:81`). It is true when the flip
  window is closed (`E62_M573920 == 0`), the heli is not active
  (`IsActive`), the value at `g_e14_166e378 + 0x844` is <= 0, and the race clock +0x140 is
  >= 2.0 s (loader). Constant `2.0` at `0x72ad94` (verified in the 1.02 image).
  **Discrepancy:** our `E60_MenuA.cpp` writes the last comparison as
  `2.0f > x || 2.0f == x` (x <= 2.0). The decompile has
  `DAT_0072ad94 < x || DAT_0072ad94 == x` (x >= 2.0), which matches the
  intent. The reconstruction is reversed.
- **Who calls `Vf18`** (the activation) is **not traced**. The menu row's action
  is not identified.

### 4.3 Runtime

- `Activate` (`0x49bc90`, `RBR_E20_Heli.cpp:202`): places the heli on the
  driveline at the anchor `Thunk4F0(+0x48)` with a lateral side (`+0x60`, +-1,
  set from the car's driveline distance and a `rand()` roll) (loader). Sets the
  flag bits
  (`+0x10 |= 3`), selects the smoke emitter by the race host's slot
  (`ar_`, `br2_`, `au_`, `ho_DAMP_`, `us1_` names), and sets the emitter rate
  (loader).
- `Tic` (`0x49c170`): if the active bit is set, adds dt to +0x14. At >= 24 s,
  it calls the deactivate vfunc and `E60_56a120` (`0x56a120`), which posts the
  menu message `MSG_RETIRE_BYSTAND` (loader). Then `Burst(dt)` runs the smoke.
- `LevelPair` (`0x49bb50`) and `M0049c1c0` (`0x49c1c0`, E13 host) set the
  render volume from the level (loader; presentation).

## 5. Start countdown and finish

### 5.1 Countdown (HUD timer)

- Reset: HUD `+0x244` is set to **7.0 s** (`DAT_00738398`, verified) in the
  HUD reset `0x5772e0` (loader).
- Tick: `E62GameMenu::M573d90` (`0x573d90`, `RBR_E62_Small.cpp:512`) decrements
  +0x244 by dt (loader). The first time it drops below **6.0 s** and
  `+8 == 0`, it calls `E12_AnimSet::FireByIdx("start_animation2")`
  (`0x47a7d0`) (loader). That fires the stage trigger named
  `start_animation2` in the stage's `.dls` (verified, setpieces): `FindByName`
  (`0x4765c0`) walks the set's named records (count `+0x34`, records `+0x38`,
  strings `+0x3c`; `.dls` table 9 `{object, name offset}` stored swapped by
  `Load020` `0x479020`, names in table 7), `_stricmp`, first match, 0 when none;
  `FireTrig` (`0x478ac0`) emits `CSigTrigger {id, BSearch50(id), 0, +0xc = 0}`;
  `E02TrigLoader::Fire` (`0x443d00`) looks `+0` up as the `(2, 14)` object and
  calls its `Trigger(state, ~+0xc & 1 = 1)`. On the 57 stages that name it, the
  object is the countdown marshal (`load_start_race01c`; track-54_E `0xc9cd`).
  The trigger is armed by HUD `+0x8` (reset 0); a jump start sets it.
- GO: when the value crosses zero (from positive), it plays a menu cue and
  message (`M5403e0(0xd)`, `E95_MenuFind`, message from `DAT_01660804`), then
  sets **+0x144 = 0** (loader).
- The race clock +0x140 (`RBR_E62_Small.cpp:578`) adds dt every frame while
  +0x144 == 0 (loader), the go's frame included. The HUD ticks only in
  application state 1, after the frame's steps, with the frame's `facc`.
- Jump start (`E62GameMenu::M579430`, `0x579430`), once per race (+0x248): the
  clock takes the countdown's rest plus the penalty (`E60Flow::M565ea0`: 10 s,
  a rally's from the season), the countdown is set to 0 and the go follows on
  the next tick. Callers: the start line's marker key 0x15 in `Load3b0` while
  the countdown runs (`M572fc0`), and `M579590` (the HUD's `CSigCamera`
  receiver) when the car moves faster than 1 m/s with the countdown in
  `(0, 4)`. A rally counts false starts and ends the race after three.
  Playback runs show it on every drive (their HUD runs slower than the steps),
  peeked on the step after the crossing. This clock is the time used for the rescue gate
  (section 4.2) and the flip penalty (section 2.3). So GO starts the timed
  race as far as these systems are concerned (loader).

### 5.2 Start marshals and the start-line set piece

Stage items reference (file, count of stage `.lbs` files):

| Script | Stage files | Clips (in order) | Run script |
|---|---|---|---|
| `load_start_race01b` (time card writer) | 57 | `start`, `act`, `idle` | `run_threestate` family (per `stage-characters.md` section 5; not re-checked) |
| `load_start_race01c` (countdown marshal) | 57 | `start` (`anim_countdown_start`), `act` (`anim_countdown`), `idle` (`anim_countdown_idle`) | `run_startrace01c` |
| `load_start_race01a` (time card walker) | 0 | `start`, `walk`, `idle` | not used by any retail item |

`start_animation1` names the time card writer (`load_start_race01b`; 54_E
`0xc9cc`). Nothing in a race fires it: its only firer is the replay timeline
event `0xf0000001` (`CAnimatedReplayManager`, `0x5fe890`, below), so the writer
holds its first clip (setpieces).

`run_startrace01c.lua`: state 0 holds clip 0; the trigger moves 0 to 1;
state 1 plays clip 1 (`act`, the countdown) and goes to 2; state 2 holds clip 2
(idle). So the countdown marshal plays its count-down clip once, on the
trigger that the HUD fires at 6.0 s (section 5.1) (file + loader).
Models: `ho_marshal_00..02` (file, `load_start_race01a/b`). Scale is `0.001`
for 01b and 01c and `0.1` for 01a (file).

No start lights, gantry or start-line object appears in `Scripts.rbz`, the
`SGS/` or the `SGA/` names. The start-line visuals are these marshals, the
HUD, and effect timelines (`start_animationN`, section 6) (file).

### 5.3 Finish

`load_end_race01a` and `load_end_race01b` (57 stage files each) load walker and
writer clip sets: `anim_end_walker_01_a` / `_b` with `_start` and `_idle`
variants, and `anim_end_writer_01_a` with `_start` (file).
`run_end_race01a/b` (5 states): clip 0 idle; first trigger 0 -> 1 plays
`move1` and goes to 2 (idle1); second trigger 2 -> 3 plays `move2` and goes to
4 (idle2) forever (file, from the script text). The exe emits `CSigEndRace`
(`CSignalManager` `+0x7970`, `E14EndRaceSig::EmitEndRace`, called from
`RBR_E60_Sig.cpp:50`); it does not reach the walkers (setpieces). The walkers
are triggered by the `Driveline` plain keys (`Load3b0`): on track-54_E
`0xc7c1`/`0xc7bf` (`load_end_race01a/b`, the `.dls` names `end_animation0/1`)
have forward keys at 4207.15 m (the finish line is at 4168.16, the end key at
4269.93) and flag-1 keys at 4307.14, which trigger only when the car backs
over them, so the walkers make their first move as the car passes and their
second only if it reverses (track-10_O: 6233.6 / 6306.3, and 6433.6 / 6506.3).
The names `end_animationN` and `start_animationN` are fired otherwise only by
`CAnimatedReplayManager` (`0x5fe890`, ids `0xf0000000..9` and
`0xf000000a..13`, from its timeline at `+0x84`/`+0x88` in `Tick` `0x5fea00`);
its recorder `Snapshot` (`0x5feac0`) has no caller in the exe (byte scan), so
in 1.02 the timeline only plays what a loaded replay holds. The RSF samples
101, 138, 140 and 294 name the same four (129 and 450 have no named records).

## 6. Bird strike

### 6.1 Load

`load_birdstrike.lua` (file): skips the item unless `LuaRandom(1,200) == 1`
(so about 1 in 200 bird items get the strike bird). Model `bird_03.sgs`,
scale `0.01`, clips 0 `start` (`anim_bird_03`), 1 `animate_a`
(`anim_bird_strike1a`), 2 `animate_b` (`anim_bird_strike1b`). Both randomised
branches load the same files (file). The `end` animation is loaded but never
added as a clip (file).

36 stage files name `load_birdstrike`, 30 name `run_birdstrike` (file).

### 6.2 Run script

`run_birdstrike.lua` (file):

- state 0: hold clip 0.
- `Trigger` (car proximity, `CSigTrigger` via `TrigLoop`, see
  `stage-characters.md` section 4): 0 -> 1.
- state 1 (at the next clip end): `app:PlaceCharacterAtCar(false, false, true)`,
  state 2, play clip 1 (`animate_a`).
- state 2 (at the next clip end): `app:Birdstrike()`, state 3, play clip 2
  (`animate_b`).
- state 3 (at the next clip end): `app:PlaceCharacterAtCar(false, false, false)`,
  play clip 0 (`start`) forever.
- After state 3, `Trigger` does nothing (state is not 0).

The strike is one-shot per bird.

### 6.3 Exe side

- `CPC_Application::PlaceAtCar` (`0x4481e0`) -> `CPC_AnimationHandler::FwdDispatch`
  (`0x446370`) -> `E03AnimImpl::Dispatch(a,b,c)` (`0x4462c0`) (loader).
  - all-zero args: remove the current descriptor's pair from the vector (+0xc0).
  - otherwise: push `{id, a, b, c}` onto the vector at impl +0xbc, clear desc
    +0x74, and if `c != 0` call the effect host `RandPair` (loader).
- Each frame, `E02AnimUpdate` (`0x443a10`, `RBR_E02_Game.cpp:1422`) loops the
  pair vector (loader):
  - record byte +4 (`a`) `!= 0`: `Vf58(id, obj+0xa4)` (translation from the
    impl's 6-float buffer);
  - byte +5 (`b`) `!= 0`: `Vf5c(id, obj+0xb0, obj+0xb4, obj+0xb8)` (rotation);
  - byte +6 (`c`) `!= 0`: `Vf58(id, g_e02167ea40)` (translation from the global
    triple, see 6.4).

  The bird strike passes `c = 1`, so the bird's translation comes from
  `g_e02167ea40` (the code comment at `RBR_E02_Game.cpp:1491` calls it a "BSS
  direction triple").
- `RandPair` (E87/E88 effect host, `0x5f71e0`, reconstruction alias
  `E88Handler::M71e0`) writes a random point at E88 +0x180..+0x18c (loader).
- `CPC_Application::Birdstrike` (`0x4481f0`) -> `E88ParticleSystem::BirdstrikeCheck`
  (`0x5f7670`, `RBR_E88_Particle.cpp:92`): if the gameflow host state
  `host + 0x3df0 > 1`, it emits `CSigBirdstrike` at the point stored at E88
  +0x190 (with y and z taken from +0x198 and +0x194) (loader). The strike sound
  is the `CCollisionSnd::OnBirdstrike` connection (`RBR_E24_Collide.cpp:22`,
  file). The strike effect is the particle system (presentation).

### 6.4 The global `g_e02167ea40`

`g_e02167ea40` (`0x167ea40`) is read in `E02AnimUpdate` (section 6.3) and has
no write in the decompile. The PE section table puts that address in
the zero-filled tail of `.data` (section `.data` is `0x7c0000 + 0xf442f4`
virtual, `0x2b000` raw), so the value is 0 in the image (file). If the
translation path is ever used, the bird is at the origin. In practice the
strike path uses `c` only, so this may not matter (**inferred**).

**Superseded (scripted-2, asm):** it is E88 `+0x190`, written each frame by `SetupDx` (0x5f6820); see
"Findings from scripted-2" below.

## 7. Crash sites (not used in 1.02 retail)

- Scripts: `load_crash_car`, `load_crash_codriver`, `load_crash_driver`,
  `load_crashcar_site0..2`, `load_crashcodriver_site0..2`,
  `load_crashdriver_site0..2` at the root, plus `test_crash*.lua`. Rally
  folders `arctic`, `canberra`, `montblanc`, `pirelli`, `ramada` each have 17
  crash scripts (file). `hokkaido/` has none.
- They call `GetCrashedCarID(0)` (`Lua_GetCrashedCarID`, `0x43b0f0`), which
  returns -1 unless the race mode is 3 or 5, else the crashed car id for team
  0 (`E01RaceHost::CrashedQuery`, `0x4a0820`, `RBR_E21_Race.cpp:521`) (loader).
- Models: `crash_xsara_00..04.sgs/.sga`, `crashdriver_male_0..3`,
  `mechanic-chief_01` (the last is a stand-in with `anim2_tirefiddle_01`).
  The `crash_*` and `crashdriver*` files are **not** in `SGS/` or `SGA/`
  (file).
- No `.lbs` item names a crash script (file, 88 stage files). No exe string
  `load_crash` (file).

So the crash-site scripts are present in the archive but not reachable from
retail stage data or the exe. They look like dead assets from an earlier build
(**inferred**).

## 8. Other Lua bindings that drive world events

| Binding | Body | Used by | Notes |
|---|---|---|---|
| `app:PlaceCharacterAtCar(a,b,c)` | section 6.3 | `run_birdstrike.lua` only | |
| `app:Birdstrike()` | `0x4481f0` -> E88 `BirdstrikeCheck` | `run_birdstrike.lua` only | |
| `app:DestroyCharacter()` | `0x448200` -> `E01CamHolder::ReleaseB` (`RBR_E03_App.cpp:948`) | no script | not traced beyond the call |
| `app:SetActiveAnimation(name, b)` | `0x448100` -> `E03AnimHost34::Set3` | no script | |
| `GetCrashedCarID(team)` | `0x43b0f0` | crash scripts only | section 7 |
| `app:LoadCharacterNoCull` | used by `load_rescuechopper.lua` | the heli | |

`DestroyCharacter` and `SetActiveAnimation` have no caller in `Scripts.rbz`
(file, all root and rally folders).

## 9. Survey (retail stage items, file counts)

Counts are the number of the 88 `Maps/*.lbs` files containing the name
(`grep -a -l -i`). Counts of matches per string are not used: the first tally
was cut short by a timeout.

| Name | Stage files |
|---|---|
| `load_car_flip*` | 0 |
| `load_rescuechopper` / `run_rescuechopper` | 0 |
| `load_crash*` | 0 |
| `load_birdstrike` | 36 |
| `run_birdstrike` | 30 |
| `load_start_race01a` | 0 |
| `load_start_race01b`, `load_start_race01c`, `run_startrace01c` | 57 each |
| `load_end_race01a/b`, `run_end_race01a/b` | 57 each |

## Relevance

Gameplay: the flip's race-time penalty (10, 20 or 35 s), the race clock that
starts at GO, the rescue menu gate and the car's teleport back to the track
change race time or car state. Presentation: the spectators, the heli's flight
and smoke, the bird animation and effects, the start and finish marshals, and
the HUD fade.

## Open

- The `0x72ace0..0x72ad34` denied-state list in `E03CarFlipInit` (decode the
  table).
- The `pelvis` bone id stored at impl +0xe0 (is it used?).
- The rally-folder flip clip 1 (`end_idle`) is unused by the stock run script.
  Check whether any rally folder has its own `run_car_flip.lua` (none found).
- The caller of `CPC_RescueHelicopter::Vf18` (row `0x25` is HELPCALL, section 3.1). Gameflow-1 traced it: the
  `CSigRetireCar` receiver `M5662c0` calls the helicopter on codes 1 and 2 (and code 3 after the rally incident timer).
- `M5403e0(0xd)` (the GO cue) and the message at `DAT_01660804`.
- The source of the `CSigBirdstrike` point (E88 +0x190, +0x194, +0x198), and
  the bird placement mapping (section 6.3, 6.4).
- Reconstruction issues found (not fixed here): `E01_Camera.cpp:59` is the flip
  handler, not a camera; `E60_MenuA.cpp` reverses the `0x140 >= 2.0` gate;
  `Lua` `load_car_flip2` picks the same model twice.

## Findings from the Rust port (scripted-1, `rust/rbr-render/src/characters/scripted.rs`)

- **Helicopter landing and camera points** (decompile of `Activate` 0x49bc90, constants read from the 1.02 image):
  the helicopter's model is placed at the driveline point of the anchor (`FillMatrix`), the anchor being
  `E12_SampleX` of the latched +0x48 (the car's driveline position). The point at +0x50 (+0x4c in the decompile's
  base) is `Sample(anchor - side * 30.0)` (`0x72f42c`) lifted by 1.8 (`0x73088c`): that is where the mode-8 camera
  stands. `RBR_E20_Heli.cpp` drops the `- side * 30`. Side test constants: 200.0 (`0x730890`), 0.0 (`0x795e50`).
  `UpdateHeli`'s constants are 1/21 (`0x7308a0`), 100.0 (`0x72bf10`), 40.0 (`0x73089c`) and 10.0 (`0x72b154`); the
  vertical lane is +8 of the physics vector (z up).
- **Bird strike triggers**: no `Driveline` animation channel (`arrays[2]`) names a bird-strike item. They are
  triggered by the plain keys, `arrays[1]` `{trigger data index, distance f32, flags}`, which `Load3b0` walks as the
  car's distance passes them; forward, a key with flag bit 0 clear emits `CSigTrigger` that `Fire` accepts. All 72
  retail `run_birdstrike` items have such a key (12,610 plain keys over the 86 stages).
- **Run-script booleans**: `run_birdstrike.lua` passes `true`/`false` (globals from `startup.lua`) to
  `PlaceCharacterAtCar`; a run-script state needs them too.
- **Emit in the recordings**: the frames carry the car words `Assist8DF0` writes: the countdown +0x15e4 armed with
  0.5 s (falling 0.007 per step after), the flip flag +0x9f0 and the override +0x1670 = 4.0. The three `drive-rcv-*`
  recordings show it at posts 1957 (flag set, code 1), 2700 and 2144 (code 2). With `c = 4.0` at every caller, code 3
  does not occur in 1.02.
- **Rescue model**: `rescuechopper.sgs` has no skinned mesh; its static meshes (`default`, `Stretcher`, `Cable`,
  `rotor`, `rotor_b`) follow their nodes, and the model `.ini` has one section per mesh (the stage characters' have
  one).

## Findings from scripted-2 (`characters/random.rs`, `scripted.rs`; asm-checked unless marked)

- **`g_e02167ea40` is written.** The 2D effect system (`NParticleSystem2D`, `E88ParticleSystem`) is the static at
  0x167e8b0 (`E14_App` constructs it in place), so 0x167ea40 is its `+0x190`. `E87EffectHost::SetupDx` (0x5f6820)
  puts the chart point `+0x180` through the car's matrix into `+0x190` every frame. `RandPair` (0x5f71e0) draws that
  chart point with the system's own LCG (`+0x44`, the exe's `* 0x2000081 + 0xe623f41d`): `x = r * w * 0.8 + 0.1`,
  `y = r * w * 0.4 + 0.3`, `r = seed % 0x7fff / 32768`, `w` the chart's `+0x30` (`SetupBig` 0x5f6930: bottom edge over
  side edge of a four-corner set not traced to the car). `Dispatch` with `c` draws it, so the bird (moved there each
  frame, its turn kept) flies at a random windscreen point; `BirdstrikeCheck` emits `CSigBirdstrike` at it (y and z
  swapped) and draws a new one. Section 6.4's "never written" is wrong.
- **No feathers, no windscreen splat.** `CSigBirdstrike`'s only typed connection is `CCollisionSnd`; the E88 call after
  the emit is `RandPair`. The strike is the bird clip and the sound.
- **`LuaRandom` is not `rand()`.** `Lua_LuaRandom` (0x43be90) calls `E01_RandomRange` (0x43bdb0): `g_e01Seed =
  g_e01Seed * 0x2000081 + 0xe623f41d`, `lo + seed % (hi - lo + 1)`. With `g_e01RandFlag` set (`E01RandSelect`
  0x43bce0, called by the stage loader 0x445eb0 before an item of a kind-2 group; any other item calls
  `E01RandClear` 0x43bd30), a value already drawn in the group is bumped (seed + 1, + 2, ... with the scan restarting
  at index 1) until new, while the set is smaller than the range.
- **Seeding.** `E59Host90::M00567640` (the race start) calls `E01SetSeed(time(NULL))` and `srand` with the same value
  and keeps it at `+0x1783c`; the replay path seeds both from that word. The app init (0x44c890) seeds both from
  `time` too, and `E80Daemon::DaemonInit` (0x5d34b0) reseeds `rand()` from the local time's fields. `PickRand`,
  `Activate`'s side roll, `Vf18`'s heading and `Burst` use `rand()` (msvcr71, `/MD`).
- **Seed order, read from the snapshots** (ghosts-2). The race seed is at `*(*0x165fa90 + 0xc98) + 0x1783c`;
  `g_e01Seed` is 0x7ea1f4; msvcr71's `_holdrand` is `+0x14` of its per-thread data, which `_getptd` (0x7c349636)
  keeps in FLS slot 1 (`FlsGetValue`, the TEB's FLS chunk, not the TLS slots). Counting each generator's steps from the
  seed, on track-54_E (Tanner, five runs with different seeds) and track-13 (arctic snow):
  - `LuaRandom`: 9 draws between the race seed and the first physics step, none afterwards (steps 0 to 12071). Our
    load draws 39 from the seed on track-54_E (34 for the stage items, 5 for the flip spectators), so most stage items
    draw before the race seed (from the app's earlier `time` seed and the menus' draws). Which 9 follow it is not
    pinned: the five `load_car_flipN` draws fit, plus four others.
  - `rand()`: 882 calls between the race seed and the first step in every Tanner run, none during the settle (steps
    40 to 80), then 4 to 45 a step, different in every run (9,320 to 12,456 by step 572): other systems (particles
    among them) draw from the same generator per frame. So the race-time draws (`PickRand` for a flip's load,
    `Activate`, `Vf18`, `Burst`) cannot be reproduced from the seed, and our own seed (`--seed`) stays.
- **`Burst` (0x49bf90)** on flag bit 1: the time `+0x64` grows by dt; `n = trunc((t - last) * 60)`; below 2 nothing,
  else `last = t`, the body point (`vf20`; `CPC_RescueHelicopter::Vf20` reads the model's node) is probed 100 m down;
  `f = clamp(1 - (z - (hit + 0.1)) / 30, 0, 1)`, `trunc(n * f)` particles at the hit + 0.1, each with velocity
  `(dx, dy, z_prev) * 15 / |..|`, then z = 0.5 (`dx`, `dy` = `rand() / 32767 - 0.5`; the first particle's z lane is the
  stack's old value). `Activate` picks the emitter by `map / 10` (1 `ar_smoke_b`, 2 `br2_fakesmoke_a`, 3
  `au_fakesmoke_a`, 4 `br2_fakesmoke_a`, 5 `ho_DAMP_fakesmoke`, 6 `us1_fakesmoke_a`), writes `0.7, 0.4, 0.25, 0.075,
  0.2, 0.2` (by rally, one value) into its particle class's `+0x6c`, the intermediate colour's alpha (`E83_Read13`
  0x5dfb50 reads start RGBA to `+0x50`, the set-in to `+0x80`, the intermediate RGBA to `+0x60` and the end to
  `+0x70`; setpieces) and scales the class's `+0xb0` (the life, as
  `CEmitterRain::Init` and the slot-age compares use it) by 0.2.
- **The flip camera** (`FUN_00487980`, `notes/cameras.md` 5d): `Split17C`'s first output is the driveline point
  (`BlendB`), the second the normalized direction (`LerpAt`); the reconstruction normalizes the wrong one. Offsets at
  0x72f580..0x72f59c are 16-byte rows (the reconstruction indexes them by 4 bytes). `CH_CAM` (0x487840) returns in mode
  13 and while the helicopter is active.

## Other systems seen

- `CCar::Assist8DF0` and the car outside-assist path, `0x4e8df0`
  (`RBR_E34_CarC.cpp:789`). Car physics; the emitter of the flip trigger.
- `CCar::Vf98x` (countdown worker), `0x4e6dd0` (`RBR_E34_CarB.cpp:493`).
- `CCar::Vf184` (pose set, teleport), `0x4e5270` (`RBR_E34_Quat.cpp:446`).
- `CCar::Vf180` (respawn seed and `CSigRespawn` emit), `0x4e9660`
  (`RBR_E34_CarD.cpp:153`).
- `E34_RetireA` / `E34_RetireB`, `0x4e8fa0` / `0x4e9000` (retire emit, car).
- `E62/E63` HUD tick `0x579df0` (`RBR_E63_HudCore.cpp`), also the "retire
  offstage" message at HUD +0x280 (`MSG_RETIRE_OFFSTAGE`).
- `E89` effect timeline events `start_animationN` / `end_animationN`,
  `0x5fe890` (`RBR_E89_Mgr.cpp:487`).
- `E12_AnimSet::FireByIdx` / `FireTrig`, `0x47a7d0` (`RBR_E12_Tail.cpp:41`).
- `E59Host90::M00565470`, the race mode query, `0x565470` (used by the flip,
  penalty and crash checks).
