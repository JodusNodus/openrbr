//! The command line (the crate's documentation describes the flags) and `--help`'s text.

use std::path::PathBuf;

use rbr_formats::{rsf, stage_files};
use rbr_gameplay::markers::MarkerSettings;
use rbr_gameplay::race_options::{ChampionshipOptions, DamageLevel, DriverAids, Skill};

use crate::{autopilot, live};

/// The command line. `--live` takes no value; the other flags take one.
pub struct Args {
    pub recording: Option<PathBuf>,
    pub live: bool,
    pub autopilot: Option<PathBuf>,
    /// `--pilot autopilot|RECORDING` (with `--live`): what drives instead of the viewer's payloads
    /// (`autopilot::Pilot`).
    pub pilot: Option<String>,
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
    /// The rally flow's files (`rally.rs`): the damage carried in and out, and the stage's result.
    pub damage_in: Option<PathBuf>,
    pub damage_out: Option<PathBuf>,
    pub result_out: Option<PathBuf>,
    /// The save files' directory and the profile in it (`saves.rs`): the stage record and the personal best.
    pub saves: Option<PathBuf>,
    pub profile: Option<String>,
    /// `--events-out`: the recording's particle and lamp records through the live boot (`events_out.rs`).
    pub events_out: Option<PathBuf>,
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
       rbr-standalone --recording X --stage NAME --boot [--steps N] [--data DIR] [--dump-image FILE] [--events-out FILE] [--twin mem|mem-direct] [WEATHER]
       rbr-standalone --live --stage NAME (--snapshot Y | --boot [WEATHER]) [--car N] [--data DIR] [--callout-distance F] [--marker-threshold F]
       WEATHER: [--weather 0|1|2] [--tyre N] [--wear new|normal|worn] [--season-row N] [--rand-seed N] [--setup FILE] [--ngp-physics FILE [--rsf-car PACK|ID] [--ffb-sensitivity G,T,S]]
       OPTIONS (with --boot): [--damage off|safe|reduced|realistic] [--auto-gears on|off] [--gear-guard on|off] [--neutral-lock on|off] [--clutch-help on|off] [--championship rookie|professional|champion [--championship-damage off|safe|reduced|realistic] [--championship-false-starts N]]
       rbr-standalone --autopilot OUT (--recording X --snapshot Y | --boot [--car N] [WEATHER]) --stage NAME [--steps N] [--top-speed M/S] [--aim X,Y[,COAST]] [--data DIR]
       --stage NAME can be --stage-dir FOLDER or --rsf-stage ID [--rsf-root DIR] (a RallySimFans stage)
       with --live: [--pilot autopilot|RECORDING] [--saves DIR] [--profile NAME] [--dispatch direct|pure] [--start-time H:MM]
       a championship stage (with --boot): [--rally-next stage|service|end] [--damage-in FILE] [--damage-out FILE] [--result-out FILE]
       -h, --help prints this text";

pub fn args() -> Result<Args, String> {
    let mut recording = None;
    let mut live = false;
    let mut autopilot = None;
    let mut pilot = None;
    let mut top_speed = None;
    let mut aim = None;
    let mut boot = false;
    let mut snapshots = vec![];
    let mut stage = None;
    let (mut stage_dir, mut rsf_stage) = (None, None);
    let mut rsf_root = std::env::var_os("RBR_RSF").map(PathBuf::from);
    let mut data = std::env::var_os("RBR_DATA").map(PathBuf::from);
    let mut car = None;
    let mut steps = 30;
    let mut read_set = None;
    let mut marker_settings = MarkerSettings::default();
    let mut dump_image = None;
    let mut twin = None;
    let (mut weather, mut tyre, mut wear, mut season_row, mut setup) = (None, None, None, None, None);
    let (mut ngp_physics, mut rsf_car, mut ffb_sensitivity) = (None, None, None);
    let (mut damage_in, mut damage_out, mut result_out) = (None, None, None);
    let (mut saves, mut profile, mut events_out) = (None, None, None);
    let (mut rand_seed, mut damage, mut auto_gears, mut aids) = (None, None, None, DriverAids::default());
    let (mut skill, mut championship_damage, mut false_starts, mut start_time) = (None, None, None, None);
    let mut rally_next = None;
    let mut direct = true;
    let mut it = std::env::args().skip(1);
    while let Some(flag) = it.next() {
        if flag == "--help" || flag == "-h" {
            println!("{USAGE}");
            std::process::exit(0);
        }
        if flag == "--live" || flag == "--boot" {
            live |= flag == "--live";
            boot |= flag == "--boot";
            continue;
        }
        let value = it.next().ok_or(format!("{flag}: no value"))?;
        match flag.as_str() {
            "--recording" => recording = Some(PathBuf::from(value)),
            "--autopilot" => autopilot = Some(PathBuf::from(value)),
            "--pilot" => pilot = Some(value),
            "--dispatch" => {
                direct = match value.as_str() {
                    "pure" => false,
                    "direct" => true,
                    _ => return Err(format!("--dispatch {value}: pure or direct")),
                }
            }
            "--top-speed" => top_speed = Some(value.parse().map_err(|_| format!("--top-speed {value}"))?),
            "--aim" => aim = Some(value.parse()?),
            "--snapshot" => snapshots.push(value),
            "--stage" => stage = Some(value),
            "--stage-dir" => stage_dir = Some(PathBuf::from(value)),
            "--rsf-stage" => rsf_stage = Some(value.parse::<u32>().map_err(|_| format!("--rsf-stage {value}"))?),
            "--rsf-root" => rsf_root = Some(PathBuf::from(value)),
            "--data" => data = Some(PathBuf::from(value)),
            "--car" => car = Some(value.parse().map_err(|_| format!("--car {value}"))?),
            "--read-set" => read_set = Some(PathBuf::from(value)),
            "--dump-image" => dump_image = Some(PathBuf::from(value)),
            "--twin" => twin = Some(crate::twin::Kind::parse(&value)?),
            "--steps" => steps = value.parse().map_err(|_| format!("--steps {value}"))?,
            "--weather" => weather = Some(value.parse().map_err(|_| format!("--weather {value}"))?),
            "--tyre" => tyre = Some(value.parse().map_err(|_| format!("--tyre {value}"))?),
            "--wear" => {
                wear = Some(
                    ["new", "normal", "worn"]
                        .iter()
                        .position(|w| *w == value)
                        .ok_or(format!("--wear {value}: new, normal or worn"))? as u32,
                )
            }
            "--setup" => setup = Some(PathBuf::from(value)),
            "--ngp-physics" => ngp_physics = Some(PathBuf::from(value)),
            "--rsf-car" => rsf_car = Some(value),
            "--ffb-sensitivity" => {
                ffb_sensitivity =
                    Some(percentages(&value).ok_or(format!("--ffb-sensitivity {value}: three percentages, G,T,S"))?)
            }
            "--damage-in" => damage_in = Some(PathBuf::from(value)),
            "--damage-out" => damage_out = Some(PathBuf::from(value)),
            "--result-out" => result_out = Some(PathBuf::from(value)),
            "--saves" => saves = Some(PathBuf::from(value)),
            "--profile" => profile = Some(value.to_owned()),
            "--events-out" => events_out = Some(PathBuf::from(value)),
            "--season-row" => season_row = Some(value.parse().map_err(|_| format!("--season-row {value}"))?),
            "--rand-seed" => rand_seed = Some(value.parse().map_err(|_| format!("--rand-seed {value}"))?),
            "--damage" => {
                damage = Some(
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
            "--start-time" => start_time = Some(clock_time(&value).ok_or(format!("--start-time {value}: H:MM"))?),
            "--rally-next" => {
                rally_next =
                    Some(live::RallyNext::parse(&value).ok_or(format!("--rally-next {value}: stage, service or end"))?)
            }
            "--auto-gears" => auto_gears = Some(on_off(&flag, &value)?),
            "--gear-guard" => aids.gear_guard = on_off(&flag, &value)?,
            "--neutral-lock" => aids.neutral_lock = on_off(&flag, &value)?,
            "--clutch-help" => aids.clutch_help = on_off(&flag, &value)?,
            "--callout-distance" => {
                marker_settings.callout_distance = value.parse().map_err(|_| format!("--callout-distance {value}"))?
            }
            "--marker-threshold" => {
                marker_settings.marker_threshold = value.parse().map_err(|_| format!("--marker-threshold {value}"))?
            }
            _ => return Err(format!("unknown argument {flag}")),
        }
    }
    match (live, recording.is_some(), snapshots.len()) {
        _ if boot && !snapshots.is_empty() => return Err("--boot takes no --snapshot".into()),
        (true, true, _) => return Err("--live takes no --recording".into()),
        (true, false, 1) => {}
        (true, false, 0) if boot => {}
        (true, false, _) => return Err("--live takes one --snapshot".into()),
        (false, false, _) if !(boot && autopilot.is_some()) => return Err("--recording is required".into()),
        (false, false, _) => {}
        (false, true, _) => {}
    }
    if autopilot.is_some() && (live || snapshots.len() != usize::from(!boot)) {
        return Err("--autopilot takes one --snapshot or --boot, and no --live".into());
    }
    if setup.is_some() && !boot {
        return Err("--setup needs --boot".into());
    }
    if ngp_physics.is_some() && !boot {
        return Err("--ngp-physics needs --boot".into());
    }
    if rsf_car.is_some() && ngp_physics.is_none() {
        return Err("--rsf-car needs --ngp-physics".into());
    }
    if ffb_sensitivity.is_some() && ngp_physics.is_none() {
        return Err("--ffb-sensitivity needs --ngp-physics".into());
    }
    if damage_in.is_some() && !boot {
        return Err("--damage-in needs --boot".into());
    }
    if (damage_out.is_some() || result_out.is_some()) && autopilot.is_none() && !(live && boot) {
        return Err("--damage-out and --result-out need --autopilot or --live --boot".into());
    }
    if (championship_damage.is_some() || false_starts.is_some() || rally_next.is_some()) && skill.is_none() {
        return Err("--championship-damage, --championship-false-starts and --rally-next need --championship".into());
    }
    if skill.is_some() && !boot {
        return Err("--championship needs --boot".into());
    }
    let championship = skill.map(|skill| ChampionshipOptions {
        skill,
        damage: championship_damage.unwrap_or(ChampionshipOptions::default().damage),
    });
    if events_out.is_some() && (live || !boot || recording.is_none() || autopilot.is_some()) {
        return Err("--events-out needs --boot and --recording, and no --live or --autopilot".into());
    }
    if pilot.is_some() && !live {
        return Err("--pilot needs --live".into());
    }
    if aim.is_some() && autopilot.is_none() {
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
    if rsf.is_some() && stage.is_none() && !boot {
        return Err("--stage-dir and --rsf-stage need --boot (the weather picks the variant)".into());
    }
    Ok(Args {
        recording,
        live,
        autopilot,
        pilot,
        top_speed,
        aim,
        boot,
        stage: stage.unwrap_or_default(),
        rsf,
        data: data.ok_or("--data or RBR_DATA is required")?,
        car,
        snapshots,
        steps,
        read_set,
        marker_settings,
        dump_image,
        twin,
        weather,
        tyre,
        wear,
        season_row,
        rand_seed,
        damage,
        championship,
        false_starts: false_starts.unwrap_or(0),
        start_time,
        rally_next,
        direct,
        auto_gears,
        aids,
        setup,
        ngp_physics,
        rsf_car,
        ffb_sensitivity,
        damage_in,
        damage_out,
        result_out,
        saves,
        profile,
        events_out,
    })
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

    #[test]
    fn a_start_time_reads_hours_and_minutes() {
        assert_eq!(clock_time("9:36"), Some(34560.0));
        assert_eq!([clock_time("24:00"), clock_time("9:60"), clock_time("936")], [None; 3]);
    }
}
