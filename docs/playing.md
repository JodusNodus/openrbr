# Playing

How to drive ReRBR: the first start, the pages, the keys, wheels and your files.

## Before you start

You need ReRBR, your own Richard Burns Rally 1.02 with NGP 7, and NGP cars. The [README](../README.md#install) says
how to get each. RallySimFans (RSF) is the easiest way: it has the game, NGP, the cars and the community stages.
ReRBR only reads your game folder; it never writes to it.

## First start

The first time, ReRBR asks for your game folder. It looks in the usual places and offers what it finds: press **Use
this folder**, type a path, or press **Browse…**. With only the disc, press **Set up from the disc…**
([install-from-disc.md](install-from-disc.md)). A folder that won't load says why, for example a missing 1.02 patch.

The launcher then opens on **Settings** while something you need is missing, else on **Drive**. Every choice is
saved at once.

## The launcher's pages

The tabs at the top are the pages. The status line at the bottom says what is missing (NGP, or a car NGP drives),
with a button to fix it, or else what happened last.

### Drive

The quick race. Pick a car, a stage, the conditions and the race options, then press **Start** (or Enter); **Stop**
closes the driving window. If something prevents a start, the reason shows under the button, often with a fix.

- **Car** and **Stage** open searchable lists with filters. Recent picks come first; entries that cannot start come
  last, with the reason.
- **The cars** are the NGP cars: RallySimFans' cars with their group, the cars you added, and NGP cars installed into
  the game's car slots. The game's 8 original cars have no NGP physics and are not listed.
- **The stages** are the 36 retail rally stages and the RallySimFans ones. The rally school and test maps are not
  listed.
- **Weather**: Good, Random or Bad, the game's presets; the line over **Start** shows what Random draws. Also **Time
  of day**, **Tyre wear** and **Tyre** (Auto picks the game's choice).
- **Wipers**: the switch at the start: off, intermittent, continuous, or auto (by the rain and the mud on the glass).
- **Ghost**: race a ghost of an earlier drive on the stage ([Where your files go](#where-your-files-go)).
- **Car damage**: Off, Safe, Reduced or Realistic, the game's levels, each with its outside-assistance penalty.
- **Driver aids**: Automatic gearbox, Gear Protection, Neutral Lock, Clutch Help. "Game defaults" restores them and
  the damage level.

### Championship

The game's season. Pick the profile name (default `player`), the team, the skill level (Rookie, Professional,
Champion), the car damage and the season's car (by default your last season's), then start a new season or continue
the saved one. Before each stage you choose the tyres and the setup; between stages, the service park lets you repair
parts against the clock. The page shows the standings, the AI drivers' times, the bulletins and the two trials
between rallies. The teams, AI drivers, stages and rules are the game's. The Drive page's driver aids apply here too.

### Car setup

The car's setup for the stage's surface, by area, within the car's ranges. Named setups are saved in the game's
`.lsp` format. A changed setup is used at the next Start.

### Graphics & sound

- **Display:** **Fullscreen**, **Resolution**, **Render scale**, **Anti-aliasing** (4x multisampling, the default,
  or Off: one sample a pixel with the cut-outs alpha-tested as in 1.02, faster with jagged edges), **Frame
  presentation**, **Mirrors** and the **FPS counter**.
- **Camera:** **Field of view** of the in-car views (30-120°, default 50). **Reset this car's cameras** drops the
  changes you made on the pause menu's Camera panel for the chosen car.
- **HUD and pace notes:** **HUD** on or off; **Working dials** (an RSF car's needles and displays, from RBRHUD's
  gauges); **Speed unit** km/h or mph; **Co-driver** (untick it for no pace notes);
  **Shown notes**: the next 0-4 notes stacked at the right, 3D symbols over the road, or both; **Call-outs**: the
  call-out distance and the marker threshold.
- **Volume:** 0-4, default 1.
- **Our renderer additions** ([what is ours](#what-is-the-original-and-what-is-ours)), each on or off, or all at
  once with "All on", "All off" and "Defaults".

### Settings

- **Ready to drive** ticks the game folder, NGP, the cars and the wheel, or shows the next step for each.
- **Game folder**: a field, **Browse…** and **Set up from the disc…**.
- **Cars**: **Add cars…** adds gvrc.de's NGP car packs from a folder. Added cars are listed with **Remove**; adding a
  car again replaces it.
- **Advanced**: the NGP physics.rbz and RSF folder fields. Leave them empty to use the game folder's.
- **Your files** and **About**: your files' folder and the version.

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
| Wipers: off → intermittent → continuous → auto | I |
| Headlights on or off | L |
| Ignition (hold, with the clutch in): restarts a stalled engine in 0.5-2.5 s | E |
| Look back (hold, chase views) | B |
| Clutch, reverse, neutral, gears 1-6 (H-shifter) | unbound |
| Change view | C |
| Next page of the digital dash (RSF cars) | End |
| Look around / zoom / reset the look | left mouse drag / mouse wheel / V |
| Pause menu | Escape, gamepad Start |
| Close a retire's message box | Escape, Enter, a click, gamepad Start or South (A) |

### Views

The views are the cockpit, the bonnet, the bumper and the chase; the next drive starts in the one you last picked.
Before the go, the game's start orbit swings round the car.

To move a view for this car, pause and choose **Camera**. A panel opens at the bottom and the view stays live above
it. Up and Down pick a row, Left and Right change it (the arrows, W/S and A/D, or the d-pad; hold to repeat):
- **View**: cockpit, bonnet, bumper or chase. The next drive starts in it.
- **Sideways**, **Height**, **Forward** and **Tilt** for the cockpit, bonnet and bumper, in 2 cm and 0.5° steps, up
  to 1 m and 15° each way.
- **Height** and **Distance** for the chase, in 0.1 m steps.
- **Reset** puts this view back to the car's own; **Done**, Backspace, Escape or East (B) goes back to the menu.

Each change is saved for this car and view at once.

### The pause menu

Escape (or the gamepad's Start) pauses the stage. The menu has the game's choices:
- **Resume** (or Escape);
- **Camera**, ours: the camera panel ([Views](#views));
- **Call for help**, once the race clock has run 2 s, and not within 8.5 s of an earlier call;
- in a quick race, **Restart**, and **Exit To Main Menu**, which leaves with no result;
- **Retire**, which ends the stage as a retire;
- after a championship stage's end, **Continue** back to the launcher.

Restart, Retire and Exit ask first. Move with the arrows or the d-pad, choose with Enter or South (A), go back with
Escape or East (B). In a championship, Retire is how you leave a stage, and the rally counts you as retired, as in the
game; closing the window does the same.

## Wheels and force feedback

- **Set up your wheel** asks for one input at a time: the steering, the throttle, brake and clutch, the handbrake,
  the paddles, then the H-shifter's reverse and each gear. Each step has **Skip**; **No H-shifter** ends it. The
  launcher opens on this page when a wheel or gamepad is connected and nothing is bound to one yet.
- Each input has a bar that follows it. To set one again, click it and move the input. The steering has no
  deadzone.
- **Wheel buttons** binds the other actions: pause, restart, call for help, look back, change the camera,
  wipers, headlights, ignition and neutral.
- **Keyboard** holds every action's key. **clear** unbinds one; **Reset to defaults** restores the keys
  above.

**Force feedback** has on/off, **strength**, **inverse**, and NGP's **sensitivity** per surface (gravel, tarmac, snow).
Strength scales the whole force; sensitivity changes its shape. At 100 % it is the car's own; below, the force is
stronger and big hits are flattened; above, it is lighter and more detailed. It uses the first force feedback device it
finds; to pick another, set `device =` in the `[ffb]` section of `launcher-settings.ini`.

## Advanced H-shifter

Off by default. With **Advanced H-shifter** ticked on the Controls page, a gear on the shifter no longer engages at
once:
- With the clutch pressed past **Clutch threshold**, it engages.
- Without the clutch, the engine speed must match the gear's, and an upshift needs the throttle at half or less. A
  synchro box needs the match to hold for a moment; a dog box accepts a wider range and waits briefly for the revs.
- Otherwise the gearbox grinds. Keep holding the lever and match the revs or press the clutch to finish the shift.
- With the lever out of gear and neutral unbound, the car goes to neutral.

**Gearbox** and **Dogleg first gear** on auto take an RSF car's from RBRControls' settings in the RSF folder; other
cars get a synchro box and no dogleg. The grind sound is RBRControls' too, so without it the grind is silent. It turns
Clutch Help off.

## Where your files go

Everything ReRBR writes goes to one folder, never to the game folder:
- macOS: `~/Library/Application Support/ReRBR/`;
- Linux: `$XDG_DATA_HOME/rerbr/`, by default `~/.local/share/rerbr/`;
- Windows: `%APPDATA%\ReRBR\` (paste it in Explorer's address bar);
- or the folder in the `RERBR_HOME` environment variable, if set (for a portable copy).

The Settings page shows it under **Your files**. The saves are in the game's formats:

| What | Where |
|---|---|
| Stage records | `NGP/SavedGames/pfresult.rst` |
| Profile (personal bests) | `NGP/SavedGames/pf<profile>.rbr` |
| RSF stage records | `NGP/SavedGames/community-records.ini` |
| Best ghost per stage | `NGP/GhostCars/Best_<map>.ghc` |
| Championship season | `SavedGames/pf<profile>.rbr`; its car in `launcher-settings.ini` |
| Setups | `Physics/<car>/Setups/*.lsp`; the game's setup slots `SavedGames/<car>slot<n>_<name>.lsp` |
| Settings, bindings, view adjustments | `launcher-settings.ini` |
| Added cars | `cars/`, in RallySimFans' layout |
| The log, on Windows | `rerbr.log`, rewritten at each start |

The quick race keeps your best ghost per stage. The **Ghost** field can also race any `GhostCars/<name>_<map>.ghc`
for the stage, in the ghost's own weather.

## What is the original and what is ours

The physics is NGP 7.4.778's. The gameplay (weather draws, race flow, damage, service, season, records) is the game's.

Ours, not in 1.02:
- **The launcher**, in place of the game's menus.
- **The season's car.** In the game, the season drives your team's car.
- **The HUD and end screens**, with a modern look.
- **The wiper switch, the look-back key, the "Best" ghost and the `NGP/` records folder.**
- **The renderer additions** on the Graphics & sound page: sharp distant textures (16x anisotropic filtering), cabin
  light, moving clouds, a soft car shadow (in place of the game's shadow decal), HDR and bloom, weather and particle
  effects, aerial perspective, sun shadows, flowing water and puddles on wet roads. All are on by default except
  Moving clouds.
- **Night.** The Drive page's time of day can be after dusk: the stage is lit by the moon and the car's lamps.
- **Working cockpit mirrors.** They show the stage behind, the car's flank, the cabin, ghost cars, dust and rain.
  **Mirrors** sets how often they are redrawn: every frame (the default), every 2nd or 3rd frame, or Off (the glass
  as in 1.02).
- **The stage always draws at the game's highest quality**; there are no quality settings.

Left out on purpose: replays, music, co-driver languages other than English, the rally school, Richard Burns
Challenge, hot seat and custom rallies, unlocks (everything is open), options in the pause menu (the
launcher has them), automatic recovery (H calls for help), TrackIR, EAX and Prologic.

## Known limitations

- **Car sound.** An RSF car with an FMOD sound bank plays its own engine, turbo, gearbox, backfire and wheel sounds.
  Other cars, and cars added from gvrc.de, use the retail Impreza's sound.
- **Keyboard steering on a RallySimFans install.** As in RSF's own game, keyboard steering moves at a tenth of the
  retail speed.
- **RSF car models.** A car whose model isn't installed is drawn as the retail car RSF names as its stand-in.
- **RSF car options.** Car-parts options, extra lights, lights on/off and other modder options are not applied; the
  selected skin is. A car installed with its lights off still lights the stage at night, without lamp flares.
- **RSF cars share one slot.** RSF cars and added cars share the same ghosts, named setups and three setup slots, and
  a ghost is drawn on the current car's model. The service park repairs them by NGP's rules for that slot, not the
  car's own.
- **Encrypted RSF stages.** RallySimFans encrypts some stages, and we do not decrypt them. They stay in the list,
  marked unavailable, as do RSF cars RallySimFans hasn't downloaded.
- **Missing stage details.** A few RSF stages miss textures (for example stage 384). Many show few spectators: their
  scripts are in no install, and the game drops them too.
- **Not compared with the original yet:** RSF stages and cars (NGP physics is checked on retail cars), damage levels
  Off and Reduced, changed driver aids and changed setups. A championship in a car you picked has no original to
  compare with.
- **Sound gaps.** Engine reflections are missing, and the rain sound is a stand-in.
- **Frame rate on long stages.** RSF 129, a long stage, ran at 103-108 fps at 1280x720 on an M3 Pro with the default
  graphics. If your frame rate drops, turn off additions on the Graphics & sound page.
