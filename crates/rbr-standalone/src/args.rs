//! The command line (the crate's documentation describes the flags) and `--help`'s text.

use std::path::PathBuf;

use rbr_formats::{rsf, stage_files};
use rbr_gameplay::markers::MarkerSettings;
use rbr_gameplay::race_options::{ChampionshipOptions, DamageLevel, DriverAids, Skill};

use crate::{autopilot, live};

/// The command line. `--live` takes no value; the other flags take one.
#[derive(Default)]
pub struct Args {
    pub recording: Option<PathBuf>,
    pub live: bool,
    pub autopilot: Option<PathBuf>,
    /// `--pilot autopilot|RECORDING` (with `--live`): what drives instead of the viewer's payloads
    /// (`autopilot::Pilot`).
    pub pilot: Option<String>,
    /// `--frames-out FILE` (with `--live`): the clock's frames logged (`live::serve_realtime`); `--frames FILE|-`:
    /// frames from a log or stdin instead of the clock, each answered and a log's compared (`live::serve_frames`).
    pub frames_out: Option<PathBuf>,
    pub frames: Option<String>,
    /// `--replay-out FILE` (with `--live`): the clock's frames appended to a replay the viewer started
    /// (`rbr_recording::replay`); `--replay FILE`: a replay's frames instead of the clock, on the clock or back to back
    /// (`--replay-pace clock|free`, `live::serve_replay`), writing no files.
    pub replay_out: Option<PathBuf>,
    pub replay: Option<PathBuf>,
    pub replay_pace: live::Pace,
    pub top_speed: Option<f32>,
    pub aim: Option<autopilot::Aim>,
    pub boot: bool,
    pub snapshots: Vec<String>,
    /// The stage's name or path (`stage_files`); for a community stage chosen by folder or id, filled in `main` once
    /// the weather picks its variant.
    pub stage: String,
    /// The community stage of `--stage-dir`, `--rsf-stage` or a `--stage` path in a stage folder.
    pub rsf: Option<rsf::Stage>,
    pub data: PathBuf,
    pub car: Option<u32>,
    pub steps: usize,
    pub read_set: Option<PathBuf>,
    pub marker_settings: MarkerSettings,
    pub dump_image: Option<PathBuf>,
    /// `--twin KIND` (headless runs): a second playback of the image, compared after every step (`twin.rs`).
    pub twin: Option<crate::twin::Kind>,
    pub weather: Option<u32>,
    pub tyre: Option<u32>,
    pub wear: Option<u32>,
    /// `--wetness dry|damp|wet`: the road's wetness over the weather row's (ours: the game takes the row's).
    pub wetness: Option<u32>,
    pub season_row: Option<usize>,
    pub rand_seed: Option<u32>,
    pub damage: Option<DamageLevel>,
    /// `--championship`: a championship stage (flow mode 3) on these options.
    pub championship: Option<ChampionshipOptions>,
    /// `--championship-false-starts`: the player's false starts so far in the rally.
    pub false_starts: u8,
    /// `--start-time`: the start board's clock (a championship stage's, or a quick race's time of day), s after midnight.
    pub start_time: Option<f32>,
    /// `--dispatch direct` (with `--live`, the default): the pure host runs the leaf calls of the entity walk, wheel,
    /// collision and rigid body in place (B1's Mem-direct dispatch, `notes/physics.md` "Store seam"); `pure`, the fact-check,
    /// makes every call through the host.
    pub direct: bool,
    /// `--rally-next`: what follows the championship stage.
    pub rally_next: Option<live::RallyNext>,
    pub auto_gears: Option<bool>,
    pub aids: DriverAids,
    pub setup: Option<PathBuf>,
    /// NGP's `physics.rbz` (`boot::Race::ngp_physics`).
    pub ngp_physics: Option<PathBuf>,
    /// `--rsf-car`: an RSF car (a pack path, or an id or pack name in `RBR_RSF`), loaded into `--car`'s slot (default
    /// RSF's slot 5) under `--ngp-physics` (`rbr_formats::rsf::locate`).
    pub rsf_car: Option<String>,
    /// `--ffb-sensitivity G,T,S`: NGP's force-feedback sensitivity percentages (`boot::Race::ffb_sensitivity`).
    pub ffb_sensitivity: Option<[i32; 3]>,
    /// `--head-gasket-durability N`: NGP's `headGasketDurability` (`boot::Race::head_gasket_durability`).
    pub head_gasket_durability: Option<i32>,
    /// The rally flow's files (`rally.rs`): the damage carried in and out, and the stage's result.
    pub damage_in: Option<PathBuf>,
    pub damage_out: Option<PathBuf>,
    pub result_out: Option<PathBuf>,
    /// The save files' directory and the profile in it (`saves.rs`): the stage record and the personal best.
    pub saves: Option<PathBuf>,
    pub profile: Option<String>,
    /// `--events-out`: the recording's particle and lamp records through the live boot (`events_out.rs`).
    pub events_out: Option<PathBuf>,
    /// `--recorded-payload` and `--trace-out` (headless runs, `headless/trace.rs`).
    pub recorded_payload: bool,
    pub extra_payloads: Option<PathBuf>,
    /// `--wall-clock FILE` (with `--recorded-payload`): the recording's wall clock per step for NGP's starter.
    pub wall_clock: Option<PathBuf>,
    pub trace_out: Option<PathBuf>,
    /// `--ffb-settings ENABLED,STRENGTH,INVERSE,DEAD_ZONE,THRESHOLD` (with `--trace-out`): the player's settings the
    /// trace's magnitude takes (`rbr_gameplay::ffb::Settings`; default on, 1, normal, no dead zone).
    pub ffb_settings: rbr_gameplay::ffb::Settings,
}

/// `value` for `flag` as a number: the flag names a bad value.
fn parsed<T: std::str::FromStr>(flag: &str, value: &str) -> Result<T, String> {
    value.parse().map_err(|_| format!("{flag} {value}"))
}

/// An aid's `on` or `off`.
fn on_off(flag: &str, value: &str) -> Result<bool, String> {
    match value {
        "on" => Ok(true),
        "off" => Ok(false),
        _ => Err(format!("{flag} {value}: on or off")),
    }
}

/// `--help`'s text.
pub const USAGE: &str = "usage: rbr-standalone --recording X --stage NAME --snapshot Y [--snapshot Z...] [--steps N] [--data DIR] [--read-set DIR]
       rbr-standalone --recording X --stage NAME --boot [--steps N] [--data DIR] [--dump-image FILE] [--events-out FILE] [--twin mem|mem-direct] [--recorded-payload [--extra-payloads FILE] [--wall-clock FILE]] [--trace-out FILE [--ffb-settings E,S,I,DZ,T]] [WEATHER]
       rbr-standalone --live --stage NAME (--snapshot Y | --boot [WEATHER]) [--car N] [--data DIR] [--callout-distance F] [--marker-threshold F]
       WEATHER: [--weather 0|1|2] [--tyre N] [--wear new|normal|worn] [--wetness dry|damp|wet] [--season-row N] [--rand-seed N] [--setup FILE]
       NGP (required with --boot): --ngp-physics FILE [--rsf-car PACK|ID] [--ffb-sensitivity G,T,S] [--head-gasket-durability N]
       OPTIONS (with --boot): [--damage off|safe|reduced|realistic] [--auto-gears on|off] [--gear-guard on|off] [--neutral-lock on|off] [--clutch-help on|off] [--championship rookie|professional|champion [--championship-damage off|safe|reduced|realistic] [--championship-false-starts N]]
       rbr-standalone --autopilot OUT (--recording X --snapshot Y | --boot [--car N] [WEATHER]) --stage NAME [--steps N] [--top-speed M/S] [--aim X,Y[,COAST]] [--data DIR]
       --stage NAME can be --stage-dir FOLDER or --rsf-stage ID [--rsf-root DIR] (a RallySimFans stage)
       with --live: [--frames-out FILE | --frames FILE|- | --replay-out FILE | --replay FILE [--replay-pace clock|free]] [--pilot autopilot|RECORDING] [--saves DIR] [--profile NAME] [--dispatch direct|pure] [--start-time H:MM]
       a championship stage (with --boot): [--rally-next stage|service|end] [--damage-in FILE] [--damage-out FILE] [--result-out FILE]
       -V, --version prints the version
       -h, --help prints this text";

pub fn args() -> Result<Args, String> {
    parse(std::env::args().skip(1))
}

/// `args()` of `argv`: the command line as `Args`.
fn parse(mut it: impl Iterator<Item = String>) -> Result<Args, String> {
    // The command line so far; the stage, the data and the championship options are computed below.
    let mut args = Args { steps: 30, direct: true, ..Args::default() };
    let mut stage: Option<String> = None;
    let (mut stage_dir, mut rsf_stage): (Option<PathBuf>, Option<u32>) = (None, None);
    let mut rsf_root = std::env::var_os("RBR_RSF").map(PathBuf::from);
    let mut data = std::env::var_os("RBR_DATA").map(PathBuf::from);
    let (mut skill, mut championship_damage): (Option<Skill>, Option<DamageLevel>) = (None, None);
    let mut false_starts: Option<u8> = None;
    let mut ffb_settings: Option<rbr_gameplay::ffb::Settings> = None;
    while let Some(flag) = it.next() {
        if flag == "--help" || flag == "-h" {
            println!("{USAGE}");
            std::process::exit(0);
        }
        if flag == "--version" || flag == "-V" {
            println!("rbr-standalone {}", env!("CARGO_PKG_VERSION"));
            std::process::exit(0);
        }
        if flag == "--live" || flag == "--boot" {
            args.live |= flag == "--live";
            args.boot |= flag == "--boot";
            continue;
        }
        if flag == "--recorded-payload" {
            args.recorded_payload = true;
            continue;
        }
        let value = it.next().ok_or(format!("{flag}: no value"))?;
        match flag.as_str() {
            "--recording" => args.recording = Some(PathBuf::from(value)),
            "--autopilot" => args.autopilot = Some(PathBuf::from(value)),
            "--pilot" => args.pilot = Some(value),
            "--frames-out" => args.frames_out = Some(PathBuf::from(value)),
            "--frames" => args.frames = Some(value),
            "--replay-out" => args.replay_out = Some(PathBuf::from(value)),
            "--replay" => args.replay = Some(PathBuf::from(value)),
            "--replay-pace" => {
                args.replay_pace = match value.as_str() {
                    "clock" => live::Pace::Clock,
                    "free" => live::Pace::Free,
                    _ => return Err(format!("--replay-pace {value}: clock or free")),
                }
            }
            "--dispatch" => {
                args.direct = match value.as_str() {
                    "pure" => false,
                    "direct" => true,
                    _ => return Err(format!("--dispatch {value}: pure or direct")),
                }
            }
            "--top-speed" => args.top_speed = Some(parsed("--top-speed", &value)?),
            "--aim" => args.aim = Some(value.parse()?),
            "--snapshot" => args.snapshots.push(value),
            "--stage" => stage = Some(value),
            "--stage-dir" => stage_dir = Some(PathBuf::from(value)),
            "--rsf-stage" => rsf_stage = Some(parsed("--rsf-stage", &value)?),
            "--rsf-root" => rsf_root = Some(PathBuf::from(value)),
            "--data" => data = Some(PathBuf::from(value)),
            "--car" => args.car = Some(parsed("--car", &value)?),
            "--read-set" => args.read_set = Some(PathBuf::from(value)),
            "--dump-image" => args.dump_image = Some(PathBuf::from(value)),
            "--twin" => args.twin = Some(crate::twin::Kind::parse(&value)?),
            "--steps" => args.steps = parsed("--steps", &value)?,
            "--weather" => args.weather = Some(parsed("--weather", &value)?),
            "--tyre" => args.tyre = Some(parsed("--tyre", &value)?),
            "--wear" => {
                args.wear = Some(
                    ["new", "normal", "worn"]
                        .iter()
                        .position(|w| *w == value)
                        .ok_or(format!("--wear {value}: new, normal or worn"))? as u32,
                )
            }
            "--wetness" => {
                args.wetness = Some(
                    ["dry", "damp", "wet"]
                        .iter()
                        .position(|w| *w == value)
                        .ok_or(format!("--wetness {value}: dry, damp or wet"))? as u32,
                )
            }
            "--setup" => args.setup = Some(PathBuf::from(value)),
            "--ngp-physics" => args.ngp_physics = Some(PathBuf::from(value)),
            "--rsf-car" => args.rsf_car = Some(value),
            "--ffb-sensitivity" => {
                args.ffb_sensitivity =
                    Some(percentages(&value).ok_or(format!("--ffb-sensitivity {value}: three percentages, G,T,S"))?)
            }
            "--head-gasket-durability" => {
                args.head_gasket_durability =
                    Some(value.parse().map_err(|_| format!("--head-gasket-durability {value}: an integer"))?)
            }
            "--damage-in" => args.damage_in = Some(PathBuf::from(value)),
            "--damage-out" => args.damage_out = Some(PathBuf::from(value)),
            "--result-out" => args.result_out = Some(PathBuf::from(value)),
            "--saves" => args.saves = Some(PathBuf::from(value)),
            "--profile" => args.profile = Some(value.to_owned()),
            "--events-out" => args.events_out = Some(PathBuf::from(value)),
            "--trace-out" => args.trace_out = Some(PathBuf::from(value)),
            "--ffb-settings" => {
                ffb_settings = Some(ffb_settings_of(&value).ok_or(format!(
                    "--ffb-settings {value}: ENABLED,STRENGTH,INVERSE,DEAD_ZONE,THRESHOLD (0 or 1, then numbers)"
                ))?)
            }
            "--extra-payloads" => args.extra_payloads = Some(PathBuf::from(value)),
            "--wall-clock" => args.wall_clock = Some(PathBuf::from(value)),
            "--season-row" => args.season_row = Some(parsed("--season-row", &value)?),
            "--rand-seed" => args.rand_seed = Some(parsed("--rand-seed", &value)?),
            "--damage" => {
                args.damage = Some(
                    DamageLevel::from_name(&value)
                        .ok_or(format!("--damage {value}: off, safe, reduced or realistic"))?,
                )
            }
            "--championship" => {
                skill = Some(
                    Skill::from_name(&value)
                        .ok_or(format!("--championship {value}: rookie, professional or champion"))?,
                )
            }
            "--championship-damage" => {
                championship_damage = Some(
                    DamageLevel::from_name(&value)
                        .ok_or(format!("--championship-damage {value}: off, safe, reduced or realistic"))?,
                )
            }
            "--championship-false-starts" => {
                false_starts =
                    Some(value.parse::<u8>().map_err(|e| format!("--championship-false-starts {value}: {e}"))?)
            }
            "--start-time" => args.start_time = Some(clock_time(&value).ok_or(format!("--start-time {value}: H:MM"))?),
            "--rally-next" => {
                args.rally_next =
                    Some(live::RallyNext::parse(&value).ok_or(format!("--rally-next {value}: stage, service or end"))?)
            }
            "--auto-gears" => args.auto_gears = Some(on_off(&flag, &value)?),
            "--gear-guard" => args.aids.gear_guard = on_off(&flag, &value)?,
            "--neutral-lock" => args.aids.neutral_lock = on_off(&flag, &value)?,
            "--clutch-help" => args.aids.clutch_help = on_off(&flag, &value)?,
            "--callout-distance" => args.marker_settings.callout_distance = parsed("--callout-distance", &value)?,
            "--marker-threshold" => args.marker_settings.marker_threshold = parsed("--marker-threshold", &value)?,
            _ => return Err(format!("unknown argument {flag}")),
        }
    }
    match (args.live, args.recording.is_some(), args.snapshots.len()) {
        _ if args.boot && !args.snapshots.is_empty() => return Err("--boot takes no --snapshot".into()),
        (true, true, _) => return Err("--live takes no --recording".into()),
        (true, false, 1) => {}
        (true, false, 0) if args.boot => {}
        (true, false, _) => return Err("--live takes one --snapshot".into()),
        (false, false, _) if !(args.boot && args.autopilot.is_some()) => return Err("--recording is required".into()),
        (false, false, _) => {}
        (false, true, _) => {}
    }
    if args.autopilot.is_some() && (args.live || args.snapshots.len() != usize::from(!args.boot)) {
        return Err("--autopilot takes one --snapshot or --boot, and no --live".into());
    }
    // Flags that only make sense with `--boot`, in the order their errors used to read.
    for (flag, present) in [("--setup", args.setup.is_some()), ("--ngp-physics", args.ngp_physics.is_some())] {
        if present && !args.boot {
            return Err(format!("{flag} needs --boot"));
        }
    }
    if args.boot && args.ngp_physics.is_none() {
        return Err("--boot needs --ngp-physics FILE (NGP's physics.rbz): ReRBR steps NGP physics only".into());
    }
    for (flag, present) in [
        ("--rsf-car", args.rsf_car.is_some()),
        ("--ffb-sensitivity", args.ffb_sensitivity.is_some()),
        ("--head-gasket-durability", args.head_gasket_durability.is_some()),
        ("--damage-in", args.damage_in.is_some()),
        ("--championship", skill.is_some()),
    ] {
        if present && !args.boot {
            return Err(format!("{flag} needs --boot"));
        }
    }
    if (args.damage_out.is_some() || args.result_out.is_some()) && args.autopilot.is_none() && !(args.live && args.boot)
    {
        return Err("--damage-out and --result-out need --autopilot or --live --boot".into());
    }
    if (championship_damage.is_some() || false_starts.is_some() || args.rally_next.is_some()) && skill.is_none() {
        return Err("--championship-damage, --championship-false-starts and --rally-next need --championship".into());
    }
    let championship = skill.map(|skill| ChampionshipOptions {
        skill,
        damage: championship_damage.unwrap_or(ChampionshipOptions::default().damage),
    });
    if args.events_out.is_some() && (args.live || !args.boot || args.recording.is_none() || args.autopilot.is_some()) {
        return Err("--events-out needs --boot and --recording, and no --live or --autopilot".into());
    }
    if (args.recorded_payload || args.trace_out.is_some())
        && (args.live || args.recording.is_none() || args.autopilot.is_some())
    {
        return Err("--recorded-payload and --trace-out need --recording, and no --live or --autopilot".into());
    }
    if ffb_settings.is_some() && args.trace_out.is_none() {
        return Err("--ffb-settings needs --trace-out".into());
    }
    if (args.extra_payloads.is_some() || args.wall_clock.is_some()) && !args.recorded_payload {
        return Err("--extra-payloads and --wall-clock need --recorded-payload".into());
    }
    if args.pilot.is_some() && !args.live {
        return Err("--pilot needs --live".into());
    }
    if (args.frames_out.is_some() || args.frames.is_some()) && !args.live {
        return Err("--frames-out and --frames need --live".into());
    }
    if args.frames.is_some() && args.frames_out.is_some() {
        return Err("--frames takes no --frames-out (the frames are given)".into());
    }
    if (args.replay_out.is_some() || args.replay.is_some()) && !args.live {
        return Err("--replay-out and --replay need --live".into());
    }
    let logs = [args.frames_out.is_some(), args.frames.is_some(), args.replay_out.is_some(), args.replay.is_some()];
    if logs.into_iter().filter(|&on| on).count() > 1 {
        return Err("one of --frames-out, --frames, --replay-out and --replay".into());
    }
    if args.replay.is_some() && args.pilot.is_some() {
        return Err("--replay takes no --pilot (the replay drives)".into());
    }
    if args.aim.is_some() && args.autopilot.is_none() {
        return Err("--aim needs --autopilot".into());
    }
    let rsf = match (&stage, stage_dir, rsf_stage) {
        (None, Some(dir), None) => Some(rsf::open(&dir)?),
        (None, None, Some(id)) => Some(rsf::find(&rsf_root.ok_or("--rsf-stage needs --rsf-root or RBR_RSF")?, id)?),
        (Some(name), None, None) => match rsf::folder_id(name) {
            Some(_) => Some(rsf::open(stage_files::folder(name).expect("a folder stage"))?),
            None => None,
        },
        (None, None, None) => return Err("--stage, --stage-dir or --rsf-stage is required".into()),
        _ => return Err("give one of --stage, --stage-dir and --rsf-stage".into()),
    };
    if let Some(problem) = rsf.as_ref().and_then(|s| s.problem.as_ref()) {
        return Err(format!("RSF stage {}: {problem}", rsf.as_ref().map_or(0, |s| s.id)));
    }
    if rsf.is_some() && stage.is_none() && !args.boot {
        return Err("--stage-dir and --rsf-stage need --boot (the weather picks the variant)".into());
    }
    Ok(Args {
        stage: stage.unwrap_or_default(),
        rsf,
        data: data.ok_or("--data or RBR_DATA is required")?,
        championship,
        false_starts: false_starts.unwrap_or(0),
        ffb_settings: ffb_settings.unwrap_or_default(),
        ..args
    })
}

/// `--ffb-settings`' five values.
fn ffb_settings_of(value: &str) -> Option<rbr_gameplay::ffb::Settings> {
    let v: Vec<f32> = value.split(',').map(|v| v.trim().parse().ok()).collect::<Option<_>>()?;
    let flag = |x: f32| (x == 0.0 || x == 1.0).then_some(x == 1.0);
    match v[..] {
        [enabled, strength, inverse, dead_zone, dead_zone_threshold] => Some(rbr_gameplay::ffb::Settings {
            enabled: flag(enabled)?,
            strength,
            inverse: flag(inverse)?,
            dead_zone,
            dead_zone_threshold,
        }),
        _ => None,
    }
}

/// `G,T,S` as three whole percentages.
fn percentages(value: &str) -> Option<[i32; 3]> {
    let values: Vec<i32> = value.split(',').map(|v| v.trim().parse().ok()).collect::<Option<_>>()?;
    values.try_into().ok()
}

/// `H:MM` as s after midnight.
fn clock_time(value: &str) -> Option<f32> {
    let (hour, minute) = value.split_once(':')?;
    let (hour, minute) = (hour.parse::<u32>().ok()?, minute.parse::<u32>().ok()?);
    (hour < 24 && minute < 60).then_some((hour * 3600 + minute * 60) as f32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;
    use std::sync::Mutex;

    #[test]
    fn a_start_time_reads_hours_and_minutes() {
        assert_eq!(clock_time("9:36"), Some(34560.0));
        assert_eq!([clock_time("24:00"), clock_time("9:60"), clock_time("936")], [None; 3]);
    }

    /// `RBR_DATA`/`RBR_RSF` held at `data`/`rsf` for the call, restored after.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    struct EnvGuard {
        data: Option<OsString>,
        rsf: Option<OsString>,
    }

    impl EnvGuard {
        fn hold(data: Option<&str>, rsf: Option<&str>) -> Self {
            let guard = EnvGuard { data: std::env::var_os("RBR_DATA"), rsf: std::env::var_os("RBR_RSF") };
            match data {
                Some(v) => std::env::set_var("RBR_DATA", v),
                None => std::env::remove_var("RBR_DATA"),
            }
            match rsf {
                Some(v) => std::env::set_var("RBR_RSF", v),
                None => std::env::remove_var("RBR_RSF"),
            }
            guard
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            match self.data.take() {
                Some(v) => std::env::set_var("RBR_DATA", v),
                None => std::env::remove_var("RBR_DATA"),
            }
            match self.rsf.take() {
                Some(v) => std::env::set_var("RBR_RSF", v),
                None => std::env::remove_var("RBR_RSF"),
            }
        }
    }

    fn run(argv: &[&str], data: Option<&str>, rsf: Option<&str>) -> Result<Args, String> {
        let _lock = ENV_LOCK.lock().unwrap();
        let _env = EnvGuard::hold(data, rsf);
        parse(argv.iter().map(|s| s.to_string()))
    }

    /// `run` of a whitespace-separated command line (no value holds a space).
    fn run_str(argv: &str, data: Option<&str>) -> Result<Args, String> {
        run(&argv.split_whitespace().collect::<Vec<_>>(), data, None)
    }

    /// The modes' bases: the recording's, the boot's and the live run's.
    const REC: &str = "--recording R --stage track-54_E --snapshot S --data D";
    /// The boot reading the recording (twin runs, the events run).
    const BOOT_REC: &str = "--boot --recording R --stage track-54_E --data D --ngp-physics N";
    const BOOT: &str = "--boot --stage track-54_E --data D --ngp-physics N --autopilot OUT";
    const LIVE: &str = "--live --stage track-54_E --snapshot S --data D";

    fn recording(a: &Args) {
        assert_eq!(a.recording, Some(PathBuf::from("R")));
        assert_eq!(a.stage, "track-54_E");
        assert_eq!(a.snapshots, ["S"]);
        assert_eq!(a.data, PathBuf::from("D"));
        assert!(!a.live && !a.boot && !a.recorded_payload && a.direct);
        assert_eq!(a.steps, 30);
        assert_eq!(a.replay_pace, live::Pace::Clock);
        assert_eq!(a.false_starts, 0);
        assert_eq!(a.damage, None);
        assert_eq!(a.championship, None);
    }

    fn boot(a: &Args) {
        assert!(a.boot && !a.live);
        assert_eq!(a.stage, "track-54_E");
        assert_eq!(a.ngp_physics, Some(PathBuf::from("N")));
        assert_eq!(a.car, None);
        assert_eq!(a.data, PathBuf::from("D"));
    }

    fn live(a: &Args) {
        assert!(a.live && !a.boot);
        assert_eq!(a.snapshots, ["S"]);
    }

    /// A good command line and its checks on the `Args`.
    type Good = (String, fn(&Args));

    /// Every flag's good values, off the mode's base.
    #[test]
    fn good_flags_parse_as_before() {
        let good: Vec<Good> = vec![
            (REC.into(), recording),
            (BOOT.into(), boot),
            (LIVE.into(), live),
            (format!("{REC} --autopilot OUT"), |a| assert_eq!(a.autopilot, Some(PathBuf::from("OUT")))),
            (format!("{LIVE} --pilot autopilot"), |a| assert_eq!(a.pilot.as_deref(), Some("autopilot"))),
            (format!("{REC} --top-speed 12.5"), |a| assert_eq!(a.top_speed, Some(12.5))),
            (format!("{REC} --autopilot OUT --aim 1,2"), |a| {
                assert_eq!(a.aim, Some(autopilot::Aim { at: [1.0, 2.0], coast: 0 }))
            }),
            (format!("{REC} --autopilot OUT --aim 1,2,7"), |a| {
                assert_eq!(a.aim, Some(autopilot::Aim { at: [1.0, 2.0], coast: 7 }))
            }),
            (format!("{LIVE} --replay F --replay-pace free"), |a| {
                assert_eq!(a.replay_pace, live::Pace::Free)
            }),
            (format!("{BOOT} --dispatch pure"), |a| assert!(!a.direct)),
            (format!("{BOOT} --car 5"), |a| assert_eq!(a.car, Some(5))),
            (format!("{REC} --steps 0"), |a| assert_eq!(a.steps, 0)),
            (
                format!(
                    "{BOOT} --weather 2 --tyre 7 --wear worn --wetness wet --season-row 3 --rand-seed 9 --setup SET"
                ),
                |a| {
                    assert_eq!(a.weather, Some(2));
                    assert_eq!(a.tyre, Some(7));
                    assert_eq!(a.wear, Some(2));
                    assert_eq!(a.wetness, Some(2));
                    assert_eq!(a.season_row, Some(3));
                    assert_eq!(a.rand_seed, Some(9));
                    assert_eq!(a.setup, Some(PathBuf::from("SET")));
                },
            ),
            (format!("{BOOT} --damage realistic"), |a| assert_eq!(a.damage, Some(DamageLevel::Realistic))),
            (format!("{BOOT} --championship champion"), |a| {
                assert!(matches!(
                    a.championship,
                    Some(c) if c.skill == Skill::Champion && c.damage == DamageLevel::Realistic
                ))
            }),
            (
                format!(
                    "{BOOT} --championship rookie --championship-damage safe --championship-false-starts 2 --rally-next service"
                ),
                |a| {
                    assert!(matches!(
                        a.championship,
                        Some(c) if c.skill == Skill::Rookie && c.damage == DamageLevel::Safe
                    ));
                    assert_eq!(a.false_starts, 2);
                    assert_eq!(a.rally_next, Some(live::RallyNext::Service));
                },
            ),
            (
                format!("{BOOT} --auto-gears on --gear-guard off --neutral-lock on --clutch-help off"),
                |a| {
                    assert_eq!(a.auto_gears, Some(true));
                    assert!(!a.aids.gear_guard && a.aids.neutral_lock && !a.aids.clutch_help);
                },
            ),
            (format!("{LIVE} --callout-distance 150 --marker-threshold 0.5"), |a| {
                assert_eq!(a.marker_settings.callout_distance, 150.0);
                assert_eq!(a.marker_settings.marker_threshold, 0.5);
            }),
            (format!("{BOOT_REC} --twin mem"), |a| assert_eq!(a.twin, Some(crate::twin::Kind::Mem))),
            (format!("{BOOT_REC} --twin mem-direct"), |a| {
                assert_eq!(a.twin, Some(crate::twin::Kind::MemDirect))
            }),
            (format!("{BOOT} --damage-in IN --damage-out OUT2 --result-out RES"), |a| {
                assert_eq!(a.damage_in, Some(PathBuf::from("IN")));
                assert_eq!(a.damage_out, Some(PathBuf::from("OUT2")));
                assert_eq!(a.result_out, Some(PathBuf::from("RES")));
            }),
            (
                format!(
                    "{REC} --recorded-payload --extra-payloads EP --wall-clock WC --trace-out T --ffb-settings 1,0.5,0,0.1,0.2"
                ),
                |a| {
                    assert!(a.recorded_payload);
                    assert_eq!(a.extra_payloads, Some(PathBuf::from("EP")));
                    assert_eq!(a.wall_clock, Some(PathBuf::from("WC")));
                    assert_eq!(a.trace_out, Some(PathBuf::from("T")));
                    assert!(a.ffb_settings.enabled && !a.ffb_settings.inverse);
                    assert_eq!(a.ffb_settings.strength, 0.5);
                },
            ),
            (format!("{BOOT_REC} --events-out E"), |a| assert_eq!(a.events_out, Some(PathBuf::from("E")))),
            (format!("{LIVE} --frames F"), |a| assert_eq!(a.frames.as_deref(), Some("F"))),
            (format!("{LIVE} --frames-out F --start-time 9:36"), |a| {
                assert_eq!(a.frames_out, Some(PathBuf::from("F")));
                assert_eq!(a.start_time, Some(34560.0));
            }),
            (
                format!(
                    "{BOOT} --rsf-car PACK --ffb-sensitivity 90,80,70 --head-gasket-durability 3 --saves SV --profile P --read-set RS --dump-image IMG"
                ),
                |a| {
                    assert_eq!(a.rsf_car.as_deref(), Some("PACK"));
                    assert_eq!(a.ffb_sensitivity, Some([90, 80, 70]));
                    assert_eq!(a.head_gasket_durability, Some(3));
                    assert_eq!(a.saves, Some(PathBuf::from("SV")));
                    assert_eq!(a.profile.as_deref(), Some("P"));
                    assert_eq!(a.read_set, Some(PathBuf::from("RS")));
                    assert_eq!(a.dump_image, Some(PathBuf::from("IMG")));
                },
            ),
        ];
        for (argv, check) in &good {
            match run_str(argv, None) {
                Ok(args) => check(&args),
                Err(e) => panic!("{argv}: error {e}, want Args"),
            }
        }
        // `--data` from the environment.
        match run_str("--recording R --stage track-54_E --snapshot S", Some("ENV")) {
            Ok(args) => assert_eq!(args.data, PathBuf::from("ENV")),
            Err(e) => panic!("RBR_DATA fallback: error {e}"),
        }
    }

    /// Every flag's bad values and broken combinations: the command line and its exact error.
    #[test]
    fn bad_flags_fail_as_before() {
        let bad: Vec<(String, &str)> = vec![
            ("--car".into(), "--car: no value"),
            ("--warp x".into(), "unknown argument --warp"),
            ("--recording R --rsf-stage x --snapshot S --data D".into(), "--rsf-stage x"),
            ("--recording R --rsf-stage 101 --snapshot S --data D".into(), "--rsf-stage needs --rsf-root or RBR_RSF"),
            (format!("{REC} --top-speed fast"), "--top-speed fast"),
            (format!("{REC} --steps x"), "--steps x"),
            (format!("{REC} --car x"), "--car x"),
            (format!("{BOOT} --weather x"), "--weather x"),
            (format!("{BOOT} --tyre x"), "--tyre x"),
            (format!("{BOOT} --season-row x"), "--season-row x"),
            (format!("{BOOT} --rand-seed x"), "--rand-seed x"),
            (format!("{REC} --autopilot OUT --aim 1"), "--aim 1: want X,Y[,COAST]"),
            (format!("{REC} --autopilot OUT --aim 1,2,x"), "--aim 1,2,x"),
            (format!("{LIVE} --callout-distance x"), "--callout-distance x"),
            (format!("{LIVE} --marker-threshold x"), "--marker-threshold x"),
            (format!("{BOOT_REC} --twin both"), "--twin both: mem or mem-direct"),
            (format!("{LIVE} --replay F --replay-pace fast"), "--replay-pace fast: clock or free"),
            (format!("{BOOT} --dispatch warp"), "--dispatch warp: pure or direct"),
            (format!("{BOOT} --wear muddy"), "--wear muddy: new, normal or worn"),
            (format!("{BOOT} --wetness humid"), "--wetness humid: dry, damp or wet"),
            (format!("{BOOT} --damage flat"), "--damage flat: off, safe, reduced or realistic"),
            (format!("{BOOT} --championship easy"), "--championship easy: rookie, professional or champion"),
            (
                format!("{BOOT} --championship rookie --championship-damage flat"),
                "--championship-damage flat: off, safe, reduced or realistic",
            ),
            (
                format!("{BOOT} --championship-false-starts x"),
                "--championship-false-starts x: invalid digit found in string",
            ),
            (format!("{BOOT} --championship rookie --rally-next soon"), "--rally-next soon: stage, service or end"),
            (format!("{BOOT} --auto-gears maybe"), "--auto-gears maybe: on or off"),
            (format!("{BOOT} --ffb-sensitivity 90,80"), "--ffb-sensitivity 90,80: three percentages, G,T,S"),
            (format!("{BOOT} --head-gasket-durability x"), "--head-gasket-durability x: an integer"),
            (
                format!("{REC} --recorded-payload --trace-out T --ffb-settings 1,2"),
                "--ffb-settings 1,2: ENABLED,STRENGTH,INVERSE,DEAD_ZONE,THRESHOLD (0 or 1, then numbers)",
            ),
            (
                format!("{REC} --recorded-payload --trace-out T --ffb-settings 2,1,0,0,0"),
                "--ffb-settings 2,1,0,0,0: ENABLED,STRENGTH,INVERSE,DEAD_ZONE,THRESHOLD (0 or 1, then numbers)",
            ),
            (format!("{LIVE} --start-time 9"), "--start-time 9: H:MM"),
            (format!("{BOOT} --snapshot S"), "--boot takes no --snapshot"),
            (format!("{LIVE} --recording R"), "--live takes no --recording"),
            ("--live --stage track-54_E --data D".into(), "--live takes one --snapshot"),
            (format!("{LIVE} --snapshot T"), "--live takes one --snapshot"),
            ("--stage track-54_E --snapshot S --data D".into(), "--recording is required"),
            ("--recording R --stage track-54_E --snapshot S".into(), "--data or RBR_DATA is required"),
            (format!("{LIVE} --autopilot OUT"), "--autopilot takes one --snapshot or --boot, and no --live"),
            (
                format!("{REC} --autopilot OUT --snapshot T"),
                "--autopilot takes one --snapshot or --boot, and no --live",
            ),
            (format!("{REC} --setup SET"), "--setup needs --boot"),
            (format!("{REC} --ngp-physics N"), "--ngp-physics needs --boot"),
            (
                "--boot --stage track-54_E --data D --autopilot OUT".into(),
                "--boot needs --ngp-physics FILE (NGP's physics.rbz): ReRBR steps NGP physics only",
            ),
            (format!("{REC} --rsf-car PACK"), "--rsf-car needs --boot"),
            (format!("{REC} --ffb-sensitivity 90,80,70"), "--ffb-sensitivity needs --boot"),
            (format!("{REC} --head-gasket-durability 3"), "--head-gasket-durability needs --boot"),
            (format!("{REC} --damage-in IN"), "--damage-in needs --boot"),
            (format!("{REC} --damage-out OUT"), "--damage-out and --result-out need --autopilot or --live --boot"),
            (
                format!("{BOOT} --rally-next end"),
                "--championship-damage, --championship-false-starts and --rally-next need --championship",
            ),
            (format!("{REC} --championship rookie"), "--championship needs --boot"),
            (
                format!("{REC} --events-out E"),
                "--events-out needs --boot and --recording, and no --live or --autopilot",
            ),
            (
                format!("{LIVE} --recorded-payload"),
                "--recorded-payload and --trace-out need --recording, and no --live or --autopilot",
            ),
            (format!("{REC} --recorded-payload --ffb-settings 1,1,0,0,0"), "--ffb-settings needs --trace-out"),
            (format!("{REC} --extra-payloads EP"), "--extra-payloads and --wall-clock need --recorded-payload"),
            (format!("{REC} --pilot autopilot"), "--pilot needs --live"),
            (format!("{REC} --frames-out F"), "--frames-out and --frames need --live"),
            (format!("{LIVE} --frames F --frames-out G"), "--frames takes no --frames-out (the frames are given)"),
            (format!("{REC} --replay-out F"), "--replay-out and --replay need --live"),
            (format!("{LIVE} --frames F --replay G"), "one of --frames-out, --frames, --replay-out and --replay"),
            (format!("{LIVE} --replay F --pilot autopilot"), "--replay takes no --pilot (the replay drives)"),
            (format!("{REC} --aim 1,2"), "--aim needs --autopilot"),
            ("--recording R --snapshot S --data D".into(), "--stage, --stage-dir or --rsf-stage is required"),
            (format!("{REC} --stage-dir DIR"), "give one of --stage, --stage-dir and --rsf-stage"),
        ];
        for (argv, want) in &bad {
            match run_str(argv, None) {
                Err(e) => assert_eq!(&e, want, "{argv}"),
                Ok(_) => panic!("{argv}: parsed, want error {want}"),
            }
        }
    }

    /// A stage folder on disk: `--stage-dir` reads it, and an empty one and a missing one fail as before.
    #[test]
    fn stage_dir_reads_a_folder() {
        let dir = std::env::temp_dir().join("rerbr-args-test-stage");
        let empty = std::env::temp_dir().join("rerbr-args-test-empty");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::create_dir_all(&empty).unwrap();
        std::fs::write(dir.join("Tracks101.ini"), "[Map101]\nTrackName=track-101\n").unwrap();
        for ext in ["lbs", "col", "trk", "dls", "mat", "ini"] {
            std::fs::write(dir.join(format!("track-101_O.{ext}")), [0u8; 4]).unwrap();
        }
        let dir = dir.to_string_lossy().into_owned();
        let boot = ["--boot", "--stage-dir", &dir, "--data", "D", "--ngp-physics", "N", "--autopilot", "OUT"];
        let args: Vec<&str> = boot.iter().map(|s| s.as_ref()).collect();
        let parsed = run(&args, None, None).expect("a stage folder with --boot parses");
        assert!(parsed.rsf.is_some() && parsed.stage.is_empty());
        let no_boot = ["--recording", "R", "--snapshot", "S", "--stage-dir", &dir, "--data", "D"];
        let args: Vec<&str> = no_boot.iter().map(|s| s.as_ref()).collect();
        match run(&args, None, None) {
            Err(e) => assert_eq!(e, "--stage-dir and --rsf-stage need --boot (the weather picks the variant)"),
            Ok(_) => panic!("a stage folder without --boot parses"),
        }
        let empty = empty.to_string_lossy().into_owned();
        let missing_ini = ["--recording", "R", "--snapshot", "S", "--stage-dir", &empty, "--data", "D"];
        let args: Vec<&str> = missing_ini.iter().map(|s| s.as_ref()).collect();
        match run(&args, None, None) {
            Err(e) => assert_eq!(e, format!("{empty}: no Tracks<id>.ini")),
            Ok(_) => panic!("an empty folder parses"),
        }
        std::fs::remove_dir_all(dir).ok();
    }
}
