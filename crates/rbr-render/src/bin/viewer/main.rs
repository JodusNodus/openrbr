//! The viewer: a stage, a car on it or alone, and a recorded or a live drive, in a window or a screenshot.
//!
//! ```text
//! RBR_DATA=<install> viewer [STAGE] [--car <name>] [--screenshot <png>] [--weather W] [--wear W]
//! RBR_DATA=<install> viewer --car <name> [--screenshot <png>]
//! RBR_DATA=<install> viewer <stage> --car <name> --drive <recording> [--at <step>] [--screenshot <png>]
//! RBR_DATA=<install> viewer <stage> --car <name> [--bindings <ini>] [--no-sound] [--volume V] [--rain R] [--no-codriver] [--codriver-mod DIR] [--no-hud] [--mph] [--fps] --live <command> [<argument>...]
//! ```
//!
//! `--textures <dir>` replaces the stage textures a pack built by `texture-pack` holds (local upscaled
//! textures; not the game's).
//!
//! `--time S` (with `--screenshot`) draws the frame's moving parts (the water, the rain, the clouds) at S seconds.
//!
//! `--sequence N` (with `--screenshot` and a drive) writes N frames, `<stem>-000.png` on, each one drive frame after the
//! last, with one renderer, so that the exposure and the glass carry over (flicker and motion checks).
//!
//! `--frames N` (with `--screenshot`) draws the view N more times first and prints the mean time per frame, the
//! read-back included (for comparing `--enhance` costs, not an absolute figure; also lets the exposure settle), then
//! N more submitted back to back with nothing read back, as a window draws them (the GPU's frame time when it is the
//! limit), with the CPU's share: recording and encoding them, which includes waits for the GPU; then N more each
//! drawn with the GPU idle, timing the CPU's share alone and counting its instructions and cycles (macOS) and
//! allocations per frame (`rbr_counters`).
//! `--profile` then draws N more frames, waiting on the GPU between their stages, and prints each stage's median time.
//!
//! `--enhance LIST` turns on renderer additions beyond the game's look (`renderer::Enhance`): a comma-separated list of
//! `anisotropy`, `cabin` (the cockpit lit by the sky it sees and the sun through the glass), `clouds`, `contact` (the
//! soft contact shadow in place of the game's car shadow decal), `decals` (painted skid marks blended), `hdr`, `marks`
//! (our tyre marks), `particles`, `sky`, `sun`, `water` (flowing water) and `wet` (puddles), or `all`; the removed
//! `ao`, `lens`, `msaa`, `taa` and `wind` are accepted and ignored.
//! Night is a time of day: `--clock HH:MM` after dusk (the stage lit by the moon and the car's lamps).
//! `--anti-aliasing 4x|off` is the frame's anti-aliasing: 4x multisampling (the default) or one sample a pixel, the
//! cut-outs alpha-tested as in 1.02.
//! `--shadows high|medium|low` sets the taps of the car's sun and contact shadows' filters (high the default; fewer
//! over the same width, cheaper and coarser). `--reflections high|medium|low|off` the screen-space reflections' rays a
//! pixel and steps a ray (4 and 64, 2 and 48, 1 and 32), or none.
//! `--render-scale S` (0.25 to 1, with `hdr`) draws the frame at S of the window's width and height, scaled up by the
//! post pass: cheaper, softer frames. `auto`, from older settings, is full size.
//!
//! `STAGE` defaults to `track-10_O`. A community stage is its folder (RallySimFans `Maps/<id>-<name>`, loaded at its
//! default variant) or the path of its files without the extension (`notes/formats.md` "RSF stages"). `RBR_PATCH` (optional) is the 1.02 patch's `files/Main_group`: its files
//! replace the install's per file (the car shaders), so the car's shaders are the 1.02 ones. `RBR_DATA` is the extracted 1.0 install (its `Maps` folder holds
//! the stage files). The window starts at the car start location. Keys: W/S forward and back, A/D
//! strafe, Space up, C down, Shift fast. Hold the left mouse button and move the mouse to look.
//! `--screenshot` renders the start view to a PNG and exits without a window. `--camera ex,ey,ez,fx,fy,fz` replaces the
//! start view with an eye and a forward direction (stage coordinates; for comparing with a capture's view matrix), and
//! `--size WxH` sets the screenshot size (default 1280x720) and the window's size in pixels (default 1280x720 points).
//! `--fov DEG` is the horizontal field of view of the free-fly, orbit, cockpit, bonnet and bumper cameras, 30 to 120
//! degrees (default 50); the chase view keeps its own, 65 degrees. The vertical one follows from the window's aspect.
//!
//! `--car <name>` (the car's `Cars.ini` folder, for example `XSARA`) puts the car at the stage start.
//! `--rsf-car <pack|id>` puts an RSF car (`rbr_formats::rsf`: a pack path, or an id or pack name of the install
//! `RBR_RSF`) into slot 5 as RSF does, and makes its model folder the `--car`; give it before `--live`. A live
//! drive then plays the car's own FMOD bank (`AudioFMOD/<audio>.bank`, `rbr_sound::fmod`) when it has one.
//! Without a stage the car is shown alone under an orbit camera: drag with the left button to turn
//! around it, W/S to zoom.
//!
//! `--drive <recording>` (with `--car` and a stage) plays a recorded drive back: the body and the tyres follow the
//! recorded poses and a chase camera follows the car. Keys: P pauses, Left and Right step a frame, Up and Down halve
//! and double the speed, R restarts. `--at <step>` starts at the first frame whose step counter reaches it. With
//! `--drive` and `--live`, drag with the left button to look around the car, the mouse wheel zooms, V resets the view.
//!
//! With `--drive` and `--live`, C (`[controls] camera`) cycles the camera views (`rbr_render::views`): cockpit, bonnet, bumper and chase,
//! and `--view NAME` starts in one (also for `--screenshot`). Holding the look-back key (B by default, `[controls]
//! look back`; `--look-back` for a screenshot) turns the chase to look behind the car. The chase and the bonnet view
//! probe the stage's collision mesh (`rbr_render::stage_probe`). While the rescue helicopter runs its camera replaces
//! the view and C does nothing. C is the free-fly camera's down key only without a drive, where there are no views.
//! The bindings (`--bindings`) may move both keys or add a wheel button to them.
//! `--cameras <ini>` keeps the player's camera choices in the file's `[camera]` section (`rbr_render::camera_prefs`;
//! the launcher passes its `launcher-settings.ini`): the view C last chose (the start view unless `--view` is given)
//! and each car's nudges, which the pause menu's Camera page sets (`rbr_render::hud::camera_page`). A HUD (`rbr_render::hud`) shows the
//! race clock (live) or the drive time (playback), speed, gear, engine speed and the controls fed to the step.
//!
//! `--live <command>` (with `--car` and a stage; the rest of the line is the command) drives the car by keyboard
//! and gamepad, with the physics stepped by the command (`rbr-standalone --live`, see `rbr_render::live`). Default keys:
//! Up throttle, Down brake, Left and Right steer, Space handbrake, A gear up, Z gear down, R starts over from the
//! snapshot; no clutch key, no direct gear (an H-shifter's reverse, neutral and gears 1 to 6) and no gamepad inputs. `--bindings <ini>` (before `--live`) reads other keys and gamepad
//! inputs from the file's `[controls]` section (`rbr_render::bindings`; the launcher passes its
//! `launcher-settings.ini`), and the wheel's force feedback from its `[ffb]` section (`rbr_render::ffb`; on unless
//! `enabled = 0`, through the first SDL haptic device with a constant force). With `h-shifter = advanced` in that
//! section the direct gears need the clutch or matched revs (`rbr_render::shifter`): `--car-gearbox
//! synchro|dog[,dogleg]` is the car's gearbox its `auto` settings take (the launcher reads it from RBRControls'
//! `CarInfo.ini`), and `--grind-sound WAV` the grind it plays (RBRControls' `Shifter_Error.wav`; silent without).
//! With `--screenshot`, the first frame (the snapshot's step) is drawn, or with `--at N` the frame N payloads later (no keys
//! pressed). From the stage start (`rbr-standalone --live --boot`), the game's start orbit replaces the view until the
//! go (`rbr_render::views::start_orbit`). The live drive plays the car's sound
//! (`rbr_sound`, from the live process's SND1 records; `rbr-standalone` writes them) on the default audio output, with
//! the in-car sound in the bonnet and cockpit views and the car's voices placed from the camera; `--no-sound` turns
//! it off, `--volume V` (0 to 4, default 1) scales it, `--rain none|light|heavy` sets the rain (the season row's
//! weather; the launcher passes it) and `--no-codriver` turns the pace notes off. `--codriver-mod DIR` (a Pacenote
//! plugin folder, with `--codriver-voice`, `--codriver-style`, `--codriver-ranges` and `--pacenotes FILE`) calls them
//! with that co-driver instead (`rbr_sound::custom_codriver`), and the HUD shows its fired calls instead of the
//! stage's notes. `--no-hud` hides the HUD, and
//! `--mph` shows the speed in mph instead of km/h, and `--fps` shows the rendered frames per second
//! in the top-right corner. `--pace-stack N` (0 to 4, default 0: the game's PN_STACK slider) shows the next N pace
//! notes in a column at the right edge, and `--pace-3d` the next four as symbols over the road (the game's
//! AID_MARKER; off by default). `--hud-hide LIST` hides HUD parts by name (`rbr_render::hud::Part`). `--at-drive <recording>` (with `--live`, `--screenshot` and `--at`) feeds the
//! recording's controls instead of no keys: each `--at` payload is the control of the step it feeds, as the live boot
//! with the recorded controls equals the recording (`rbr-standalone`'s `a_paced_live_boot_with_the_recorded_controls`).
//! In live mode Escape (or the pause binding, `[controls] pause`, the gamepad's Start by default) opens the pause menu
//! (`rbr_render::hud::pause`, `rbr_gameplay::pause_menu`): the game's items for the mode (Resume, Call for help when
//! the game offers it, Restart outside a championship, Retire, Exit To Main Menu outside a championship; Continue
//! after a championship stage's end) and its confirm boxes, chosen with the arrows and Enter or the d-pad and South,
//! Escape or East going back; and ours after Resume, Camera: a panel at the bottom over the paused view, whose rows
//! (Up and Down) the arrows, A and D or the d-pad's Left and Right change, a held key or button repeating, each
//! change shown and written to `--cameras` at once. While it is open the drive is paused: the stepping process's clock stops (its game,
//! race clock and car stand still), the sound stops where it is and the wheel's force is 0; the drive goes on from
//! there with no catch-up. Exit closes the window (the game's exit leaves no
//! result); Retire ends the race retired. `--pause-menu MOVES` (with `--screenshot` and `--live`) draws the menu,
//! opened and moved through `MOVES` (`u`, `d`, `l`, `r`, `e`, `b`; `-` none). Escape closes the window outside live mode.
//! Once the live stage ends, result actions appear even with the HUD hidden: Restart stage and Quit to menu in a
//! quick rally, Continue in a championship. Click one, or choose with arrows/d-pad and Enter/South. Escape/East
//! returns to the launcher. Restart also resets `--quit-after end+S` to wait for the next result.
//! `--quit-after S` closes the window after S seconds as the menu's exit does (unattended runs, with
//! `rbr-standalone --pilot`;
//! `end+S`: S seconds after the race is over, its result shown), and the live process's end prints the drawn frames, the answers and the steps against the wall time.
//!
//! A drive (`--drive`, `--live`) throws the stage's particles (`rbr_render::particles`): tyre spray and dust by surface,
//! the exhaust, and rain or snow in the wet and damp weathers of the rain and snow presets. A screenshot replays the
//! frames before it so that they are in the air. `--no-particles` leaves them out. The stage's tape and net fences
//! (`rbr_render::fences`) are drawn at rest, and a drive pushes, tears and drops them as the car goes through;
//! `--no-fences` leaves them out. An RSF car with RBRHUD gauges in the RSF install (`rbr_render::dials`) has working
//! dials in the cockpit: its needles, lights and displays follow the drive, and its backlit faces glow by night;
//! `--no-dials` leaves its painted dials. The gauge mode key (End by default, `[controls] gauge mode`) turns the pages
//! of its digital dash, and `--gauge-mode N` starts on page N.
//!
//! The tyre spray dirties the car's paint (`rbr_render::dirt`) and spatters mud on the windscreen; the wipers, inside
//! and out, follow `--wipers off|intermittent|continuous|auto` (default auto: by the weather and the mud) and the wiper
//! key (I by default, `[controls] wipers`), which steps the switch. The headlight key (L, `[controls] headlights`)
//! steps the headlights from main beam to dipped to off (with the tail lights) and back; the game has them always on.
//!
//! `--damage ZONES` dents the car as the damage handler's zone values do (`rbr_render::damage`): `zone:value` pairs,
//! for example `--damage "1:1.0 3:0.43 8:0.53"`, the zones not named being 0 (`harness`-side dump: the handler's 64
//! floats at `*(car+0x9f4)+0x3808`). `--crack EVENTS` cracks the windows as `CSigCarDamage` events do: `code:index`
//! pairs, the record's event code and functor index, for example `--crack "3:0 4:4"` (front window, then the rear).
//! With `--live`, the live process's damage record after each frame (`rbr_recording::damage`) does both; a recorded
//! drive carries no damage.
//!
//! `--ghost FILE` (with `--drive` or `--live`) races a ghost car (`rbr_render::ghost`): a `.gho` (the game's
//! `Ghosts\rb_<map>.gho` in `Root/ghosts.rbz`, or one this viewer wrote) or a `.ghc` (the game's `GhostCars` save),
//! played at the drive's race time. `--save-ghost FILE` records the drive's ghost into a `.gho` (or `.ghc` by the
//! extension): a recorded drive's at the start, a live one's when the window closes. `--best-ghost FILE` (with
//! `--live`) races the best ghost kept in FILE (`./GhostCars/<name>_<map:02>.ghc`) when it is the stage's, and
//! replaces it when the drive finishes faster; `--ghost-weather` gives the weather words a written `.ghc` keeps.
//! `--ghost` also plays RSF's `.gch` on its RSF car's model, and `--rsf-ghosts DIR` (with `--live`) keeps a finished
//! drive in RSF's layout under DIR (`rbr_formats::rsf::ghosts`).
//!
//! The window opens first and shows a loading screen (`loading`): the stage's picture, `--title TEXT` (else the
//! stage's file name) and the load's progress.
//!
//! Missing textures and dropped triangles are reported on stderr when the stage loads.

// No console window on Windows: stderr goes to the log file (`rbr_render::console`).
#![windows_subsystem = "windows"]

mod args;
mod draw;
mod latency;
mod loading;
mod scene;
mod screenshot;
mod window;

use rbr_formats::data::Data;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// Counts the allocations for `--frames`' per-frame line.
#[global_allocator]
static ALLOCATOR: rbr_counters::Counting = rbr_counters::Counting;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("viewer: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let mut args = args::parse_args(std::env::args().skip(1))?;
    let mut data = Data::from_env().ok_or("RBR_DATA is not set")?;
    let mut fmod_bank = None;
    if let Some(key) = &args.rsf_car {
        let root = std::env::var_os("RBR_RSF").map(PathBuf::from);
        let selected = rbr_formats::rsf::select(data, key, rbr_formats::rsf::SLOT, root.as_deref())?;
        eprintln!("viewer: {}", selected.describe());
        args.car.get_or_insert_with(|| selected.folder().to_owned());
        fmod_bank = selected.audio_bank;
        args.rsf_install = Some(selected.root.clone());
        args.rsf_ghost_car = Some((selected.car.id, selected.car.group.clone()));
        data = selected.data;
    }
    args.ffb.rotation = args.ffb.rotation_for(args.car_lock);
    match &args.screenshot {
        Some(path) => {
            let mut scene = scene::load(&args, &data, &mut |_| {})?;
            screenshot::screenshot(&mut scene, &args, path).map_err(|e| e.to_string())
        }
        None => window::run_window(args, data, fmod_bank),
    }
}

/// The live drive's sound (`--no-sound` turns it off): the car's FMOD bank when the RSF car has one, and the
/// `--grind-sound`; the rain or snow the scene's (`Scene::precipitation`).
fn start_sound(
    args: &args::Args,
    data: &Data,
    fmod_bank: Option<&Path>,
    precipitation: Option<rbr_render::particles::Precipitation>,
) -> Option<rbr_render::sound::Player> {
    match (&args.live, &args.car) {
        (Some(_), Some(car)) if args.sound => {
            // The weather daemon's amounts (50 light, 100 heavy); snow plays no rain.
            let amount = precipitation.map_or(0.0, |p| p.intensity * 100.0);
            let rain = if precipitation.is_some_and(|p| p.snow) { 0.0 } else { amount };
            let sky = rbr_render::sound::Sky { rain, precipitation: amount, ..Default::default() };
            let mut player = rbr_render::sound::start(data, car, args.volume, sky, args.codriver);
            if let (Some(player), Some(m), Some(stage), Some(live)) =
                (&mut player, &args.codriver_mod, &args.stage, &args.live)
            {
                if let Some(choice) = m.choice() {
                    // The distance the standalone's markers call at (the launcher's setting).
                    rbr_render::sound::use_codriver(player, data, stage, &choice, args::callout_time(live));
                }
            }
            if let (Some(player), Some(path)) = (&mut player, fmod_bank) {
                rbr_render::sound::use_fmod(player, path);
            }
            if let (Some(player), Some(path)) = (&mut player, &args.grind_sound) {
                match std::fs::read(path).map_err(|e| e.to_string()).and_then(|b| rbr_render::sound::Wave::decode(&b)) {
                    Ok(wave) => player.set_grind(wave),
                    Err(e) => eprintln!("viewer: {}: {e}", path.display()),
                }
            }
            player
        }
        _ => None,
    }
}
