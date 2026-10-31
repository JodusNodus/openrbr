//! The standalone program's first, headless form (milestone S0): the gameplay loop
//! (`rbr_gameplay::game::Game`) drives the pure Rust physics step (`rbr_hybrid::pure`), with no emulator and no
//! original code, from a snapshot's memory image, and the car and the wheels are compared with the recording after
//! every step.
//!
//! Usage: rbr-standalone --recording X --stage NAME --snapshot Y [--snapshot Z...] [--steps N] [--data DIR] [--read-set DIR]
//!        rbr-standalone --recording X --stage NAME --boot [--steps N] [--data DIR] [--dump-image FILE] [--events-out FILE] [--twin mem|mem-direct] [--recorded-payload [--extra-payloads FILE] [--wall-clock FILE]] [--trace-out FILE] [WEATHER]
//!        rbr-standalone --live --stage NAME (--snapshot Y | --boot [WEATHER]) [--car N] [--data DIR]
//!        rbr-standalone --autopilot OUT (--recording X --snapshot Y | --boot [--car N] [WEATHER]) --stage NAME [--steps N] [--top-speed M/S] [--aim X,Y[,COAST]] [--data DIR]
//!
//! `--stage` names the recording's stage (`track-54_E`); its `.dls` markers and `.trk` driveline are read from
//! `DIR/Maps` (default `$RBR_DATA`). The car number is `--car`, else the recording's `.auto` second word, else 0.
//!
//! A community stage in a folder of its own (RallySimFans, `notes/formats.md` "RSF stages") is `--stage-dir FOLDER`, or
//! `--rsf-stage ID` with `--rsf-root DIR` (else `$RBR_RSF`), or `--stage` with the path of its files without the
//! extension (`.../101-Autiovaara II/track-101_O`, which fixes the variant). The weather picks the variant from the
//! stage's `TrackSettings<id>.ini` (`Season::of_rsf`, ours), and its surface is RSF's (`rsf::Surface`).
//!
//! `--boot` starts from the image `boot/` builds from the data (`$RBR_PATCH` gives the 1.02 exe) instead of a
//! snapshot, at the stage start: the race is a quick race of the
//! stage's map and the car, on the recording's `.auto` tyre (its fourth word), else the surface's.
//!
//! `--championship rookie|professional|champion [--championship-damage off|safe|reduced|realistic]` (with `--boot`)
//! runs the stage as a championship's (flow mode 3, `rbr_gameplay::race_options::ChampionshipOptions`): the season's
//! skill sets the outside-assistance penalty (10, 20, 35 s) and shuts the assistance gate at champion, and the
//! championship's damage level (default realistic, the game's) goes to the car instead of `--damage`'s. The season
//! daemon the gate reads is a stand-in holding the skill only (`boot::season_stand_in`, ours).
//! `--championship-false-starts N` gives the player's false starts so far in the rally: a jump start costs 10, 15, 30
//! or 600 s by it (`E80Daemon::IdxFloat`), and the result says whether the stage had one (`false_start`).
//! `--start-time H:MM` gives the start board's clock: a championship stage's is the season queue's head (the launcher's
//! `season_events::start_clock`, `FUN_005733c0`), a quick race's the time of day the launcher draws it at (ours);
//! without it the board takes the cloud letter's, as the game's quick race.
//! `--rally-next stage|service|end` says what follows the championship stage: `end` makes it the rally's last (a
//! finish in a car that can't drive opens page 0x13, not 0x18), `service` picks page 0x18's text (`live::RallyNext`).
//!
//! `--damage-in FILE` (with `--boot`) starts a championship stage with the damage the stage before left
//! (`rbr_gameplay::car_damage`); `--damage-out FILE` and `--result-out FILE` (with `--autopilot`, or `--live --boot`)
//! write the damage and the stage's result after the drive, or when the viewer closes the live run (`rally.rs`).
//!
//! `WEATHER` is `[--weather N] [--tyre N] [--wear W] [--season-row N]`: `StartGame`'s
//! weather (0 good, 1 random, 2 bad; default the `.auto`'s third word, else 0) and tyre (default as above); the wear
//! (`new`, `normal`, `worn`) the random weather draws (default `normal`) or the bad weather keeps from before the race
//! (default `new`, a fresh game's); and the random weather's season row (default the map's first). An explicit
//! `--wear` sets the boot's wear for any preset. The weather must load `--stage` (`rbr_gameplay::weather`).
//! `--rand-seed N` draws the random weather's row and wear as the game does, from the CRT's `rand()` after `srand(N)`
//! (`weather::random_draws`; the game seeds with `time(NULL)`); `--season-row` and `--wear` still override it.
//!
//! `OPTIONS` is `[--damage off|safe|reduced|realistic] [--auto-gears on|off] [--gear-guard on|off]
//! [--neutral-lock on|off] [--clutch-help on|off]`, the menu's race options (`rbr_gameplay::race_options`,
//! `notes/gameplay.md` "Race options"); the defaults are the game's and the reference drives' (safe, off, on, off, on). The
//! damage level also sets the outside-assistance penalty (10, 10, 20, 35 s). Without `--damage` and `--auto-gears` the
//! recording's `.auto` fifth and sixth words give them (the harness writes them to 0x1660850 and 0x1660814).
//!
//! `--live` is the viewer's stepping process (`live/`, `rbr_recording::realtime`): the game on this process's clock, a
//! frame per physics step with the viewer's newest input from stdin, a step record after each and an answer (frame,
//! HUD and the other records) per 7 ms of game time on stdout, from the snapshot's step on. It takes no recording. Play
//! it with the viewer, from the repository root (the viewer's `--live` takes the rest of the line as the command):
//!
//! `RBR_DATA=<install> target/release/viewer track-54_E --car COROLLA --live target/release/rbr-standalone --live --snapshot /tmp/rbr-snaps/vcu-cov.snap-2000.bin --stage track-54_E`
//!
//! `--pilot autopilot|RECORDING` (with `--live`) drives the live run itself, for unattended live runs that the viewer
//! draws as a player's: the autopilot below, once a frame, or the recording's control of each step it feeds (the
//! viewer's input is ignored; `autopilot::Pilot`). At the end of its input `--live` prints the clock's figures.
//!
//! `--frames-out FILE` logs the clock's frames, and `--live ... --frames FILE|-` runs frames from such a log (compared
//! frame by frame: steps and car) or from stdin instead of the clock, each answered on stdout: the lockstep of the
//! tools that step a drive themselves (`live::serve_frames`, `rbr_recording::realtime::Logged`; `--pilot` drives those
//! frames too, for the viewer's `--screenshot` of a drive).
//!
//! `--replay-out FILE` appends the clock's frames, restarts marked, to a replay the viewer started, and `--replay FILE`
//! runs the replay's frames of its last start in place of the viewer's input, on the clock (`--replay-pace clock`, the
//! viewer's PAUSE, RESUME and RESTART as in a drive) or back to back (`free`, for a video), compared frame by frame.
//! A replay writes no files: neither the records (`--saves` is read for the profile's input filter) nor the rally's.
//!
//! `--events-out FILE` (with `--boot --recording X`) runs the recording's controls through the live boot and writes the
//! PTC1 and PEV1 records of each step it answers (`events_out.rs`): the viewer's playback takes the lamps from them.
//!
//! `--recorded-payload` (headless) feeds each step the recorded payload as it is instead of the replay entry rounded
//! from it (an NGP live drive's is unquantized); `--trace-out FILE` writes our car and wheels after each step as FRM1
//! frames, with the force-feedback force, engine speed, gear, tyre slip, application state and the wheel's magnitude
//! (with `--ffb-settings`) in `FILE.ext` (`headless/trace.rs`);
//! `--extra-payloads FILE` (with `--recorded-payload`) emits more payloads before a step's own: records of `u32 pre`
//! and the 0x20-byte payload, for a step whose control input took more than one (an RSF session's CTL1).
//! `--wall-clock FILE` (with `--recorded-payload`) times NGP's starter by the recording's clock: records of `u32 pre` and
//! `u64 us` (an RSF session's STP1).
//!
//! `--autopilot OUT` drives from the snapshot with a driveline-following controller (`autopilot/`) for at most `--steps` steps (`--top-speed`: the controller's top speed, m/s) and
//! writes a playback for `harness/play.sh` to `OUT`: the recording's frames before the snapshot's step, then the live
//! steps with the controller's controls. With `--boot` it drives from the stage start (`live::Live::boot`, the race as
//! `--live --boot` takes it), and the playback holds the live steps only. `--aim` drives into a stage point
//! (`autopilot::Aim`).
//!
//! The loop runs from the recording's first frame: up to the snapshot's step, `Game::frame` takes the recorded car as
//! its physics sample; from the snapshot's step on, each frame's physics substep is a pure
//! `StepForward` on the snapshot's memory, with the control `Game` emits from the recorded payload. A snapshot at
//! step 0 (S1) runs everything pure: the load settle's `StepForward(40)` frames, the
//! camera's go and the countdown, then driving. `Game` gives each application state change to the pure memory
//! (`PhysicsStep::app_state`, `SetAppState`'s store), so a control emitted while driving reaches the car (S2). The compare is `pure_step`'s (`rbr_hybrid::pure::Compare`; at step 0
//! `Compare::at_start`). `Game`'s time card and clock are compared with the bound `CTimeKeeper`'s in `Mem` at the boot
//! and after every step. A call the pure host cannot run stops the snapshot's run, with its step and address; there
//! is no fallback.
//!
//! `--read-set DIR` writes `DIR/<snapshot>.reads` per snapshot: the image's bytes the pure steps read before anything
//! wrote them, with their first reader (`pure::ReadTrace`).
//!
//! The physics step is the image's loaded `MaxTimeStep` (`pure::max_time_step`), and the camera's go comes at the
//! first frame after the settle frames (the frame headers' substeps).

use std::panic;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use rbr_formats::data::Data;
use rbr_formats::rsf;
use rbr_gameplay::control::QuantizedControl;
use rbr_gameplay::weather::{self, Season};
use rbr_physics::snapshot::Snapshot;
use rbr_recording::replay::Replay;
use rbr_recording::Frame;

use args::{Args, USAGE};
use physics::{payload, run_guarded};
use stage::{stage_data, StageData};

/// Counts the allocations for `--boot`'s timing line.
#[global_allocator]
static ALLOCATOR: rbr_counters::Counting = rbr_counters::Counting;

mod args;
mod autopilot;
mod boot;
mod events_out;
mod handles;
mod headless;
mod live;
mod physics;
mod race;
mod rally;
mod saves;
mod stage;
mod twin;

fn main() -> ExitCode {
    let mut args = match args::args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}\n{USAGE}");
            return ExitCode::FAILURE;
        }
    };
    let patch = std::env::var_os("RBR_PATCH").map(PathBuf::from);
    let data = Data::new(args.data.clone(), patch);
    if let (Some(rsf), true) = (&args.rsf, args.stage.is_empty()) {
        // A community stage by folder or id: its variant is the weather's letter, as the season row's for retail.
        match race::race_weather(&data, &args, rsf.id) {
            Ok((_, weather)) => args.stage = rsf.stage(weather.letter_char()),
            Err(e) => {
                eprintln!("{e}");
                return ExitCode::FAILURE;
            }
        }
    }
    let args = args;
    let recording = args.recording.as_deref();
    let car = args
        .car
        .or_else(|| recording.and_then(|r| race::recorded_word(r, 1)))
        .unwrap_or(if args.rsf_car.is_some() { rsf::SLOT } else { 0 });
    let stage = match stage_data(&data, &args.stage, car) {
        Ok(stage) => stage,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    };
    // Stops and port failures are reported per snapshot (headless) or per answer (live), not by the panic hook.
    panic::set_hook(Box::new(|_| {}));
    let (data, race) = match race::race(&args, data, &stage, car) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    };
    if args.live {
        return live(&args, &data, race.as_ref(), &stage);
    }
    if let Some(out) = &args.autopilot {
        return autopilot(&args, out, &data, race.as_ref(), &stage);
    }
    if let (Some(out), Some(race), Some(recording)) = (&args.events_out, &race, recording) {
        return events(&args, out, &data, race, &stage, recording);
    }
    let Some(recording) = recording else {
        unreachable!("args() requires a recording unless --live or --autopilot --boot");
    };
    headless::run_all(&args, &data, &stage, race.as_ref(), recording)
}

/// `--live`: serves the viewer's protocol from the snapshot or the boot (`live/`), then writes the rally's files.
fn live(args: &Args, data: &Data, race: Option<&boot::Race>, stage: &StageData) -> ExitCode {
    let r = run_guarded("the live run", || {
        let mut live = if let Some(race) = race {
            live::Live::boot(data, &args.stage, race, stage, args.marker_settings, true)?
        } else {
            let path = &args.snapshots[0];
            let snap = Snapshot::open(path).map_err(|e| format!("{path}: {e}"))?;
            live::Live::new(&snap, stage, args.marker_settings).map_err(|e| format!("{path}: {e}"))?
        };
        live.set_direct(args.direct);
        if let Some(dir) = &args.saves {
            let saves = stage.saves(dir, args.profile.as_deref())?;
            live.set_saves(if args.replay.is_some() { saves.dry() } else { saves });
        }
        // The start board's clock: `--start-time` (a championship stage's from the season, a quick race's time of
        // day), else the time of day by the race's cloud letter (`weather::start_clock`).
        let cloud_letter = race.and_then(|race| {
            let season = Season::load(data).ok()?;
            Some(season.rows.get(race.weather.row?)?.cloud_letter)
        });
        if let Some(seconds) = args.start_time.or_else(|| cloud_letter.and_then(weather::start_clock)) {
            live.set_start_clock(seconds);
        }
        let pilot = match &args.pilot {
            Some(what) => Some(autopilot::Pilot::new(what, stage, &live)?),
            None => None,
        };
        if let Some(next) = args.rally_next {
            live.set_rally_next(next);
        }
        let live = if let Some(path) = &args.replay {
            let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
            let replay = Replay::read(&bytes).map_err(|e| format!("{}: {e}", path.display()))?;
            let output = std::io::BufWriter::new(std::io::stdout());
            let (_, line) = live::serve_replay(live, &replay.frames, std::io::stdin(), output, args.replay_pace)?;
            eprintln!("rbr-standalone: --replay: {line}");
            return Ok(());
        } else if let Some(path) = &args.frames {
            let input: Box<dyn std::io::Read> = match path.as_str() {
                "-" => Box::new(std::io::stdin()),
                path => {
                    Box::new(std::io::BufReader::new(std::fs::File::open(path).map_err(|e| format!("{path}: {e}"))?))
                }
            };
            let (live, line) = live::serve_frames(live, input, std::io::BufWriter::new(std::io::stdout()), pilot)?;
            eprintln!("rbr-standalone: --frames: {line}");
            live
        } else {
            let file = match (&args.frames_out, &args.replay_out) {
                (Some(path), _) => Some((path, std::fs::File::create(path))),
                (None, Some(path)) => Some((path, std::fs::OpenOptions::new().append(true).open(path))),
                (None, None) => None,
            };
            let log = match file {
                Some((path, file)) => {
                    Some(Box::new(std::io::BufWriter::new(file.map_err(|e| format!("{}: {e}", path.display()))?))
                        as Box<dyn std::io::Write>)
                }
                None => None,
            };
            live::serve_realtime(live, std::io::stdin(), std::io::stdout(), pilot, log)?
        };
        // The stage's end when the viewer closes: the rally's files (ours).
        rally::write(
            &rally::stage_out(live.mem(), live.game().end_now(), live.game().race_clock().jumped),
            args.damage_out.as_deref(),
            args.result_out.as_deref(),
        )
    });
    match r {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("rbr-standalone: {e}");
            ExitCode::FAILURE
        }
    }
}

/// `--events-out OUT`: the recording's controls through the live boot, with the particle and lamp records of each step
/// (`events_out.rs`).
fn events(args: &Args, out: &Path, data: &Data, race: &boot::Race, stage: &StageData, recording: &Path) -> ExitCode {
    let r = run_guarded("the events run", || {
        let live = live::Live::boot(data, &args.stage, race, stage, args.marker_settings, true)?;
        events_out::write(live, stage, recording, out)
    });
    match r {
        Ok(written) => {
            eprintln!("rbr-standalone: {} steps of events in {}", written.steps, out.display());
            if let Some(step) = written.diverged {
                eprintln!("rbr-standalone: the replay's car differs from the recording's from step {step}");
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("rbr-standalone: {e}");
            ExitCode::FAILURE
        }
    }
}

/// `--autopilot OUT`: drives from the snapshot or the boot and writes the playback (`autopilot/`).
fn autopilot(args: &Args, out: &Path, data: &Data, race: Option<&boot::Race>, stage: &StageData) -> ExitCode {
    let bytes = match &args.recording {
        Some(path) if !args.boot => match std::fs::read(path) {
            Ok(bytes) => bytes,
            Err(e) => {
                eprintln!("{}: {e}", path.display());
                return ExitCode::FAILURE;
            }
        },
        _ => vec![],
    };
    let frames: Vec<Frame> = match rbr_recording::read_all(&bytes) {
        Ok(frames) => frames,
        Err(e) => {
            match &args.recording {
                Some(path) => eprintln!("{}: {e}", path.display()),
                None => eprintln!("{e}"),
            }
            return ExitCode::FAILURE;
        }
    };
    let r = run_guarded("the autopilot", || {
        let (mut live, head, control) = if let Some(race) = race {
            let live = live::Live::boot(data, &args.stage, race, stage, args.marker_settings, false)?;
            let neutral = payload(&QuantizedControl::default().car_control(), 0);
            (live, &bytes[..], neutral)
        } else {
            autopilot::from_snapshot(&args.snapshots[0], &bytes, &frames, stage, args.marker_settings)?
        };
        // With `--saves` the drive runs on to the race's end, which submits the stage record and the personal best as
        // the live drive does.
        if let Some(dir) = &args.saves {
            live.set_saves(stage.saves(dir, args.profile.as_deref())?);
        }
        autopilot::drive(live, head, control, stage, args.steps as u32, args.top_speed, args.aim, out)
    });
    match r {
        Ok(d) => {
            let finish = d.finish.map_or("not reached".into(), |s| format!("step {s}"));
            println!(
                "{}: from step {}, {} steps driven, finish {finish}, ended by {}",
                out.display(),
                d.start,
                d.steps,
                d.end
            );
            if let Some(o) = &d.out {
                println!("rally: {:?}, drivable {}", o.end, o.drivable);
                if let Err(e) = rally::write(o, args.damage_out.as_deref(), args.result_out.as_deref()) {
                    eprintln!("rbr-standalone: {e}");
                    return ExitCode::FAILURE;
                }
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("rbr-standalone: {e}");
            ExitCode::FAILURE
        }
    }
}
