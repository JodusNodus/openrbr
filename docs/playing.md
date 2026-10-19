# Playing

How to drive ReRBR, this reimplementation of Richard Burns Rally 1.02 with NGP 7's physics: what to install, how to
build it, and what the launcher and the driving window do. This page is for players. The rest of the repository is about how the game is
reconstructed.

## What you need

An installed Richard Burns Rally with the 1.02 patch and NGP 7 (NGP's `physics.rbz`), and NGP cars. RallySimFans
(RSF) is the easiest way to get all of it, and it adds the community stages. Retail RBR with the 1.02 patch and the
NGP package works too, but the NGP package brings no cars: the game's 8 original cars have no NGP physics, so ReRBR
does not offer them. The cars come from NGP car packs:
- gvrc.de's NGP car packs, added in the launcher (no RallySimFans needed): download a car's
  `<Car> - NGP Physics.zip` and its `<Model> - Model.7z` from https://gvrc.de/NGP.html into a folder, then press
  **Add cars…** (on the Drive page under Car, or on Settings) and choose that folder, or run
  `rbr-launcher --add-cars FOLDER`. Every physics pack in the folder is added, with the model whose name starts the
  car's (some cars share one: the Lancer Evo IX N4 and R4 both take `Mitsubishi Lancer Evo IX - Model.7z`). A pack
  without its model is still added and drawn as the retail car 5 (the Impreza 2003), as RSF draws a car whose model
  is missing. Adding a pack again replaces it; **Remove** on Settings deletes it. The launcher never downloads;
- RSF's, installed with it;
- or an NGP car installed into one of the game's car slots (with RBRCIT or similar).

Original game files are never shipped here: bring your own. You set that one folder in the launcher (below). It is
only read, never written.

From the install folder the launcher takes, at fixed places only:

| What | Where in the folder | Needed for |
|---|---|---|
| The game data and the 1.02 exe (`RichardBurnsRally_SSE.exe`, the patched shaders and scripts) | the folder itself (`Maps/`, `Cars/`, `misc.rbz`, ...) | everything: the stages, the game's rules, sounds and shaders |
| NGP's `physics.rbz` | `physics.rbz` (NGP and RSF put NGP's in the retail one's place) | the physics, required, and the NGP cars installed into the car slots |
| RallySimFans content (optional) | `rsfdata/`, `Maps/<id>-<name>/`, `Cars/` | the community cars and stages |

You also need a Rust toolchain and SDL3 (force feedback goes through SDL3's haptic API; on macOS `brew install sdl3`).

Advanced overrides, for development or split setups:

| Override | Replaces |
|---|---|
| `RBR_DATA`, or `launcher --data DIR` | the game data. The extracted retail 1.0 install works too (its `Root/` folder holds what an installed game has at the top). |
| `RBR_PATCH` | the 1.02 files: the official patch's extracted `files/Main_group` folder |
| The Settings page's NGP physics.rbz field (`ngp_physics`; unsaved: `RBR_NGP_PHYSICS`) | NGP's archive |
| The Settings page's RSF folder field (`rsf_root`; unsaved: `RBR_RSF`) | the RallySimFans folder |

`scripts/launcher.sh` sets all four from the main checkout's `original/` folder.

**Mac.** RBR, NGP and the RSF installer are Windows programs. Install them on a Windows PC (or in Wine), then copy the
installed folder to the Mac and choose that copy in the launcher.

**RallySimFans** is only read, never written. From it the launcher reads:
- `Maps/<id>-<name>/`: the community stages (`notes/formats.md`, "RSF stages");
- `rsfdata/cache/*.json`: the car list, groups and models;
- `rsfdata/cars/<pack>/`: the NGP physics packs;
- `Cars/<model>/`: the car models and the selected skin (`notes/formats.md`, "RSF cars").

Some RSF stages are encrypted by RallySimFans: their `.lbs`, `.col` and texture archives start with `01 a0 00 00`
and the rest is scrambled. We do not decrypt them. They stay in the map list, marked unavailable, for example
"track-294_N.lbs is encrypted by RallySimFans". On a full install that is 162 of 443 stage folders. RSF cars whose
physics pack is not in `rsfdata/cars` are listed unavailable too ("no physics pack in rsfdata/cars").

## Download

The releases page has a download for macOS (`ReRBR-<version>-macos.dmg`, Apple silicon, macOS 15 or later) and Linux
(`ReRBR-<version>-x86_64.AppImage`). Nothing else needs installing: SDL3 is inside.

**macOS.** Open the `.dmg` and drag **ReRBR** to Applications. ReRBR is not signed with an Apple Developer ID, so the
first time macOS refuses to open it:
1. Open ReRBR. macOS says it cannot verify it; press **Done**.
2. Open **System Settings**, **Privacy & Security**. Below Security it says "ReRBR was blocked…": press **Open Anyway**
   and confirm with your password.

After that it opens normally. In Terminal, `xattr -dr com.apple.quarantine /Applications/ReRBR.app` does the same.

**Linux.** Make the AppImage executable and start it:

```sh
chmod +x ReRBR-*.AppImage
./ReRBR-*.AppImage
```

It needs FUSE 2 or 3 (most desktops have it); without it, `./ReRBR-*.AppImage --appimage-extract-and-run`. The
launcher's options work on the AppImage the same way, for example `./ReRBR-*.AppImage --list`.

## Build once

From `rust/`:

```sh
cargo build --release -p rbr-render -p rbr-standalone -p rbr-launcher
```

This makes three programs in `rust/target/release/`:
- `rbr-launcher`: the window you use (not plain `launcher`: CachyOS's ananicy-cpp runs any process named `launcher` at
  nice 16 under SCHED_IDLE, and the viewer and physics it starts inherit that, so the drive stutters);
- `viewer`: the driving window;
- `rbr-standalone`: the physics.

The launcher looks for the other two next to itself (`--viewer PATH` and `--standalone PATH` override that).

## Start

```sh
rust/target/release/rbr-launcher
```

At the first start the window asks for your RBR install folder: type it or press **Browse…**. It fills in the first
install with NGP (a folder whose `physics.rbz` is NGP's) it finds in the usual places, for **Use this folder**:
- Windows: `C:\Richard Burns Rally`, `C:\Games\Richard Burns Rally`;
- Linux: `~/Games/Richard Burns Rally`, `~/.wine/drive_c/Richard Burns Rally`, and `drive_c/Richard Burns Rally` in
  the Lutris and Bottles prefixes (`~/Games/*/`, `~/.local/share/bottles/bottles/*/`);
- macOS: `~/Games/Richard Burns Rally`.

The folder is saved in `launcher-settings.ini` as `install_folder`, and the **Settings** page changes it later.

In a checkout with `original/` filled in, `scripts/launcher.sh` builds and starts it in one go, with the overrides
above set from `original/`.

Launcher options (`--help` lists them; the viewer and `rbr-standalone` have `--help` too):

| Option | What it does |
|---|---|
| `--data DIR` | the game data, instead of the install folder's (or `RBR_DATA`) |
| `--viewer PATH`, `--standalone PATH` | the two programs, if they are not next to the launcher |
| `--list` | prints how many maps can start, every map and RSF car that cannot, with the reason, the added cars, and what the install lacks (for example no NGP physics.rbz); no window |
| `--command` | prints the viewer command Start would run with the saved selection and exits: `RBR_DATA=` and `RBR_PATCH=` lines (the viewer reads the game from them), the program, then one argument a line |
| `--screenshot DIR` | draws the pages offscreen to PNGs in DIR and exits |
| `--add-cars DIR` | adds gvrc.de's NGP car packs downloaded into DIR to `cars/` in your files' folder, and exits |

All choices are saved in `launcher-settings.ini` in your files' folder ("Where your files go"), on every change. `--list`, `--command` and
`--screenshot` only read it.

## The launcher

The tabs at the top are the pages.

- **Drive**: the quick race. Pick a car, a stage, the conditions and the race options, then press **Start** (or
  Enter). The driving window opens and the launcher waits; **Stop** closes it. If something prevents a start, the
  reason shows in orange under the button, with a fix button when there is one (for example "Open Settings").
- **Championship**: the season. Pick the profile name (default `player`), the team, the skill level (Rookie,
  Professional, Champion) and the car damage, and the season's car (the same searchable list as Drive's, by default
  the car of your last season, else the Drive page's), then start a new season, or continue the saved one. The teams,
  the AI drivers and their times, the stages and the rules stay the game's; the AI drive the game's cars. The page
  shows your car with the season's options. A season with no car picked (one saved by an earlier version) asks for
  one on Continue. Each rally
  runs stage by stage. Between stages there is the service park, where you repair parts against the clock, and before
  each stage you choose the tyres and the setup. The page shows the standings, the AI drivers' times, the bulletins
  and the two trials between rallies. The Drive page's driver aids apply to the season too.
- **Car setup**: the car's setup for the stage's surface, by area, within the car's ranges and steps (an RSF car's
  from its pack, a slot-installed car's from NGP's `physics.rbz`). Named setups are saved in the game's `.lsp` format. A changed setup is used at the next Start.
- **Controls**: the wheel, pedals and shifter, the wheel's other buttons, force feedback and the keyboard (below).
- **Graphics & sound**: the window (**Resolution**, **Render scale**, **FPS counter**), the camera (**Field of view**
  of the in-car views, 30-120°, default 50; **Reset this car's cameras** drops the adjustments made for the chosen
  car), the HUD and pace notes (**HUD** shown or hidden, **Speed unit** km/h or mph; **Co-driver**: untick it to turn
  the pace notes off; **Shown notes**: the next 0-4 notes stacked at the right, and/or 3D symbols over the road;
  **Call-outs**: the call-out distance and the marker threshold), the **Volume** (0-4, default 1) and our renderer
  additions, each on or off (see "Ours" below). "All on", "All off" and "Defaults" set the additions.
- **Settings**: the RBR install folder (a field and **Browse…**), and what the launcher found in it, each with a ✔ or
  ✖: the game data, the 1.02 files, NGP's archive with the number of cars it drives, and the RallySimFans folder
  (optional: it adds the community cars and stages). **Cars** adds gvrc.de's NGP car packs (**Add cars…**, with its
  progress, Cancel and a result line) and lists the added cars, each with **Remove**.
  **Advanced** holds the NGP physics.rbz and RSF folder fields; empty, they take the install folder's. Changing the
  folder loads the lists again. **About** shows the version, the viewer and physics programs and the settings file.

The status line at the bottom of every page says what the install lacks (a missing program, no NGP archive, no car
NGP drives), with a button that opens Settings; otherwise the last event ("viewer running", "viewer exited").
Without NGP it says: "This install has no NGP physics.rbz. Install NGP (or RallySimFans, which includes it), or set
NGP physics.rbz under Advanced."

### The Drive page's fields

Car and stage:

- **Car** and **Stage** open searchable lists (name, number, group, country, author). They are grouped and can be
  filtered by surface and stage length, and by whether they can start. Recent picks come first. Entries that cannot
  start come last, with the reason. The previews on the right follow the highlighted entry.
- **Cars**: the NGP cars. These are the car slots NGP's `physics.rbz` has an NGP car installed in (their
  `common.lsp` sets NGP's keys; the NGP package's own archive has none), and with a RallySimFans folder its cars, with
  their group, for example "Mitsubishi Lancer Evo IX N4 (Group N4, RSF 24)", raced in slot 5 as RSF does. With RSF
  cars, slot 5 is left out: in RSF's archive it is the RSF car last picked. The cars added from gvrc.de's packs are
  listed as "Added" and raced in slot 5 the same way; a car that the RSF folder also has is listed once, RSF's. The
  game's 8 original cars have no NGP physics and are not listed. With no car at all the status line says: "No cars
  to drive. Add cars from gvrc.de's NGP car packs (NGP Physics .zip + Model .7z), or use a RallySimFans install."
- **Stages**: the retail stages and the RallySimFans folder's.

Conditions:

- **Weather**: Good, Random or Bad, the game's presets. Random draws the season row and the tyre wear as the game
  does. The line over **Start** shows what the draw gives.
- **Time of day**, **Tyre wear**, **Tyre** (Auto picks the game's default for the surface and weather).
- **Wipers**: the switch at the start: off, intermittent, continuous, or auto (by the rain and the mud on the glass).

Race:

- **Ghost** (below).
- **Car damage**: Off, Safe, Reduced, Realistic, the game's levels, which also set the outside-assistance penalty
  (shown next to it).
- **Driver aids**: Automatic gearbox, Gear Protection, Neutral Lock, Clutch Help. "Game defaults" restores them and
  the damage level.

## Driving

### Keys

| Action | Default key |
|---|---|
| Throttle, brake | Up, Down |
| Steer | Left, Right |
| Handbrake | Space |
| Gear up, gear down | A, Z |
| Restart the stage (not in a championship) | R |
| Call for help: back on the road, with a time penalty | H |
| Wipers: step off → intermittent → continuous → auto | I |
| Ignition (held, with the clutch in): restarts a stalled engine after 0.5-2.5 s | E |
| Look back (held, chase views) | B |
| Accept a message (a retire box) | Enter |
| Clutch, reverse, neutral, gears 1-6 (H-shifter) | unbound |
| Change view (kept for the next drive) | C (`camera`) |
| Adjust the view for this car | X (`camera adjust`, below) |
| Look around the car / zoom / reset the look | drag with the left mouse button / mouse wheel / V |
| Pause menu | Escape, gamepad Start |

The views are in the car (cockpit), on the bonnet, on the bumper, and the chase. The view you pick is the one the
next drive starts in. Before the go, the start orbit replaces the view, and the rescue helicopter's camera replaces
it while the helicopter flies.

X adjusts the current view for the car you drive; the top of the screen says so while it is on, also with the HUD
off. Left and Right move it
sideways, Up and Down forward and back, Page Up and Page Down up and down; on the chase, Up and Down change the
distance and Page Up and Page Down the height. Backspace puts the view back where it was. X (or the key or wheel
button you bound) or Escape finishes, and
the adjustment is kept for that car (in `launcher-settings.ini`, `[camera]`). The field of view is the one setting for
all cars.

Escape (or the gamepad's Start) pauses the stage and opens the pause menu. It offers what the game's does:
- **Resume** (Escape again does the same);
- **Call for help**, once the race clock has run 2 s and outside an assistance's 8.5 s;
- in a quick race **Restart** and **Exit To Main Menu**, which leaves the stage with no result;
- **Retire**, which ends the stage as a retire. In a championship this is how you leave a stage: the rally then counts
  you as retired, as the game does;
- after a championship stage's end, **Continue** back to the launcher.

Restart, Retire and Exit ask first. Move with the arrows or the d-pad, choose with Enter or the gamepad's South (A),
go back with Escape or East (B). Closing the window during a championship stage still counts as a retire.

### Bindings, gamepads and wheels

The **Controls** page is laid out for a wheel and pedals:
- **Set up your wheel** asks for one input at a time: turn the wheel left, press the throttle, brake and clutch,
  pull the handbrake, the shift-up and shift-down paddles, then put the H-shifter in reverse and each gear. Each
  step has **Skip**, and **No H-shifter** ends it. The launcher opens on this page when a wheel or gamepad is
  connected and nothing is bound to one yet.
- The rows below show each input with a bar that follows it. To set one again, click it and move the input; the
  steering takes an axis, and turning left sets both sides. Escape cancels.
- **Wheel buttons** binds the other actions (pause, restart, call for help, look back, change camera, adjust
  camera, wipers, headlights, ignition, accept message, neutral) to buttons. **Force feedback**: on or off, strength, inverse, and NGP's sensitivity per surface (gravel, tarmac, snow; 100 % is the car's own). Below 100 the
  force is stronger and big hits are flattened; above 100 it is lighter, with more detail and sharper peaks. Strength
  scales the whole force; sensitivity changes its shape.
- **Keyboard** (collapsed) holds every action's key. **clear** unbinds, and **Reset to defaults** restores the keys
  above, with only the gamepad's Start (pause) bound.
- The steering has no deadzone: the wheel's centre is its centre.

Gamepads and wheels are read through gilrs. Wheel pedals and shifters that gilrs has no name for are stored as native
codes, each with its device's USB ids. The bindings are saved in the `[controls]` section of
`launcher-settings.ini`.

### Advanced H-shifter

Off by default. With **Advanced H-shifter** ticked in Controls, gears 1-6 and reverse (bound to the shifter) no longer
engage at once:
- With the clutch pressed past **Clutch threshold**, the gear engages.
- Without it, the engine speed must match the gear's, and an upshift needs the throttle at half or less. A synchro
  box needs the match to hold for a moment. A dog box accepts a wider range and waits briefly for the revs.
- Otherwise the gearbox grinds and the gear stays out. Holding the lever and then matching the revs or pressing the
  clutch completes the shift.
- With the lever out of gear and neutral unbound, the car goes to neutral.

**Gearbox** and **Dogleg first gear** on auto take an RSF car's from RBRControls' `CarInfo.ini` in the RSF folder.
For other cars they fall back to synchro and no dogleg. A dogleg moves first gear to the gear 2 slot, and the gear 1
slot is unused. The grind is RBRControls' `Shifter_Error.wav` (or its `.wav.sample`) from the same folder, and it is
silent without one. The mode turns Clutch Help off, because clutch help ignores the pedal. The rules follow
RBRControls' HShifter advanced mode (`notes/ngp.md`, "H-shifter"), but the target engine speed comes
from the physics' own gear ratios.

Force feedback drives the first SDL haptic device that has a constant force, or the one named by `device =` in the
file's `[ffb]` section. The Force feedback box turns it off.

## Where your files go

Everything is written to one folder, never to the game folder or next to the programs:
- macOS: `~/Library/Application Support/ReRBR/`;
- Linux: `$XDG_DATA_HOME/rerbr/`, by default `~/.local/share/rerbr/`;
- or the folder in `RERBR_HOME`, if set (for a portable copy, or tests).

The Settings page shows it under **Your files**. The stage records and ghosts are kept in its `NGP/`, apart from the
1.02 ones beside it.

Earlier versions wrote these files next to the launcher. If that folder has a `launcher-settings.ini` and the folder
above doesn't exist yet, the launcher copies them over once at its start and says so on its status line. The old
files stay where they were.

| What | Where |
|---|---|
| Stage records table | `NGP/SavedGames/pfresult.rst` |
| Profile (personal bests) | `NGP/SavedGames/pf<profile>.rbr` |
| RSF stage records | `NGP/SavedGames/community-records.ini` |
| Best ghost per map | `NGP/GhostCars/Best_<map>.ghc` |
| Championship season | `SavedGames/pf<profile>.rbr` (the `RBR7` block); its car in `launcher-settings.ini` (`[season]`) |
| Setups | `Physics/<car>/Setups/*.lsp`; slots `SavedGames/<car>slot<n>_<name>.lsp` |
| Settings, bindings | `launcher-settings.ini` |
| Added cars | `cars/`: `rsfdata/cars/<pack>/` (the physics packs), `Cars/<model>/` (the models), `rsfdata/cache/cars.json` and `carmodels.json` (the list), RSF's layout |

The save files are in the game's own formats. The quick race keeps the best ghost and replaces it when you finish
faster. The Ghost field can also race any `GhostCars/<name>_<map>.ghc` for the map, and a saved ghost brings its own
weather.

## What is the original and what is ours

The physics is NGP 7.4.778's. The gameplay (weather draws, race flow, damage, service, season, records) is the
game's.

Ours, not in 1.02:
- **The launcher (ReRBR).** It replaces the game's menus.
- **The season's car.** The game's season drives its team's car.
- **The HUD and end screens.** They have a modern look.
- **The wiper switch, the look-back key, the "Best" ghost store and the `NGP/` records folder.**
- **The Graphics & sound page's additions.** The defaults are all of them except Ambient occlusion and Night:
  - sharp distant textures (16x anisotropic filtering);
  - ambient occlusion;
  - a soft car shadow, in place of the game's shadow decal;
  - HDR and bloom;
  - Night: the stage at night, lit by the moon and the car's lamps;
  - weather and particle effects;
  - aerial perspective;
  - sun shadows;
  - temporal anti-aliasing (needs HDR);
  - wind in the foliage.
- **The stage always draws at the game's highest quality.** The game's quality settings are not offered.

Left out on purpose (user decisions):
- no replays and no replay cameras;
- no music;
- the co-driver speaks English only;
- the modes are the quick race and the championship: no rally school, Richard Burns Challenge, hot seat or custom
  rallies;
- no unlocks: everything is offered from the start;
- no options in the pause menu (the launcher has them), and no automatic recovery (H calls for help);
- no TrackIR, EAX or Prologic.

## Known limitations

- **Car sound.** An RSF car with an FMOD sound bank (`AudioFMOD/<bank>.bank`, named in RSF's car list) plays its own
  engine, turbo, transmission, gear, backfire and wheel sounds from it. Cars without one, and cars added from gvrc.de
  packs, use the sound of the retail car in slot 5 (the Impreza 2003). The game's other sounds come from the retail
  `audio.dat`, which an RSF install keeps as `audio.dat.disabled`.
- **Keyboard steering on an RSF install folder.** RSF renames `PC_InputFilter.ini` to `PC_InputFilter.ini.bak`, so the
  game's built-in input filter applies, as in RSF's own game: the digital steering moves at a tenth of the retail
  file's rates.
- **RSF car models.** A car without its model folder in `Cars/` is drawn as the retail car RSF names as its
  fallback. `--list` shows which, for example "model: Cars/Escort_mk_2 not installed, retail car 5".
- **RSF car options.** Car-parts options, extra lights, lights on/off and other modder option sets are not applied.
  The selected skin is.
- **RSF ghosts.** A ghost stores the slot, so every RSF car shares slot 5's ghosts, and a ghost is drawn on the
  current car's model.
- **RSF car setups and repairs.** Named setups and the game's three setup slots are slot 5's, shared by every RSF car.
  The service park repairs the RSF car on the repair tree of slot 5 in NGP's `physics.rbz`, not the car pack's own.
- **Encrypted RSF stages** cannot be played (above).
- **Missing stage details.** A few RSF stages miss textures, for example stage 384, which has JPEG files named
  `.dds`. Many RSF stages show few spectators, because their scripts exist in no install; the game drops those too.
- **Not compared with the original.** No RSF stage or RSF car has been compared with the original game yet. NGP
  physics is checked against the original running NGP on retail cars. A championship with a picked car has no
  original to compare with (the game's season drives its team's car).
- **Damage levels and driver aids.** Damage levels Off and Reduced, the changed driver aids and changed setups are
  implemented from the game's code, but not yet compared with recorded drives of the original.
- **Start and finish line.** The start and finish set pieces (the countdown marshal's animation) do not play yet.
- **Sound gaps.** Engine reflections are missing, and the rain sound is a stand-in.
- **Frame rate on long stages.** Long RSF stages are the heaviest: RSF 129 ran at 103-108 fps at 1280x720 on an M3
  Pro with the default graphics. Turn off additions on the Graphics & sound page if your frame rate drops.

Development status and open items: `LOG.md` and `notes/status.md`.
