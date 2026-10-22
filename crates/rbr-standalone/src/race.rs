//! The race `--boot` starts (`boot::Race`) from the command line: the weather, the car (an RSF car under NGP), the
//! tyre, the setup, the race options and a championship's, with the recording's `.auto` words as the defaults.

use std::path::Path;

use rbr_formats::data::Data;
use rbr_formats::lsp::{self, Node};
use rbr_formats::{rsf, stage_files};
use rbr_gameplay::race_options::{DamageLevel, DriverAids, RaceOptions};
use rbr_gameplay::setup::{self, Setup};
use rbr_gameplay::weather::{self, CrtRand, Draws, Preset, Season, Weather};

use crate::args::Args;
use crate::stage::StageData;
use crate::{boot, rally};

/// Word `k` of a recording's `.auto` (`map car weather tyre ...`, `StartGame`'s numbers), if there is one.
pub fn recorded_word(recording: &Path, k: usize) -> Option<u32> {
    std::fs::read_to_string(recording.with_extension("auto")).ok()?.split_whitespace().nth(k)?.parse().ok()
}

/// The race's weather (`rbr_gameplay::weather`): the `--weather`, else the recording's `.auto` third word, else good;
/// the draws from `--rand-seed`, then `--season-row` and `--wear`.
pub fn race_weather(data: &Data, args: &Args, map: u32) -> Result<(Preset, Weather), String> {
    let id = args.weather.or_else(|| args.recording.as_deref().and_then(|r| recorded_word(r, 2))).unwrap_or(0);
    let preset = Preset::from_id(id).ok_or(format!("weather {id}: 0 good, 1 random or 2 bad"))?;
    let season = match &args.rsf {
        // A variant named by the `--stage` path keeps its weathers, unless a `--season-row` counts in all of them (the
        // launcher's).
        Some(stage) => {
            let letter = (!args.stage.is_empty() && args.season_row.is_none())
                .then(|| stage_files::parts(&args.stage))
                .flatten()
                .map(|(_, l)| l);
            Season::of_rsf(stage, letter)
        }
        None => Season::load(data)?,
    };
    let drawn = match args.rand_seed {
        Some(seed) if preset == Preset::Random => Some(
            weather::random_draws(&season, map, &mut CrtRand::srand(seed))
                .ok_or(format!("map {map} has no season row (the original stops at its int3)"))?,
        ),
        _ => None,
    };
    // `--wear` is set after the weather: a championship stage's road wear can be new under the random preset (its
    // row is the season's, `season::road_wear`), which the random draw never gives.
    let draws = Draws {
        row: args.season_row.or(drawn.and_then(|d| d.row)),
        wear: drawn.map_or(u32::from(preset == Preset::Random), |d| d.wear),
    };
    let mut selected = weather::quick_race(&season, map, preset, draws)?;
    if let Some(wear) = args.wear {
        if wear > 2 {
            return Err(format!("wear {wear}: not new, normal or worn"));
        }
        // The launcher makes the previous-race wear explicit, including for good weather.
        selected.wear = wear;
    }
    // Ours: the launcher's road wetness over the row's; the `.mat` entry and the wet meshes follow.
    if let Some(wetness) = args.wetness {
        selected.wetness = wetness.min(2);
    }
    Ok((preset, selected))
}

/// `--rsf-car KEY` in slot `slot`: the data with the car's model in the slot, and its pack.
fn rsf_car(data: Data, key: &str, slot: u32) -> Result<(Data, rsf::Pack), String> {
    let selected = rsf::select(data, key, slot, std::env::var_os("RBR_RSF").as_deref().map(Path::new))?;
    eprintln!("{}", selected.describe());
    Ok((selected.data, selected.pack))
}

/// The root group of a setup file (`--setup`), checked as a setup (`rbr_gameplay::setup`).
fn setup_root(path: &Path) -> Result<Vec<Node>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let file = lsp::parse(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    let root = setup::root(&file).ok_or(format!("{}: no root group", path.display()))?;
    Setup::default().layer(root).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(root.to_vec())
}

/// The data the run loads (with `--rsf-car`'s model in `car`'s slot) and, with `--boot`, the race on `stage`.
pub fn race(args: &Args, data: Data, stage: &StageData, car: u32) -> Result<(Data, Option<boot::Race>), String> {
    let recording = args.recording.as_deref();
    let surface = match args.rsf.as_ref() {
        Some(rsf) => rsf.surface.map_or(boot::surface(stage.number), rsf::Surface::name),
        None => boot::surface(stage.number),
    };
    let tyre = args.tyre.or_else(|| recording.and_then(|r| recorded_word(r, 3))).unwrap_or(boot::default_tyre(surface));
    let (data, car_pack) = match args.rsf_car.as_deref() {
        Some(key) => {
            let (data, pack) = rsf_car(data, key, car)?;
            (data, Some(pack))
        }
        None => (data, None),
    };
    if !args.boot {
        return Ok((data, None));
    }
    let (preset, weather) = race_weather(&data, args, stage.number)?;
    let (wear, wetness) = weather.mat_words();
    eprintln!(
        "weather: season row {:?}, {}, .mat {wear} {wetness}, wet meshes {}",
        weather.row,
        weather.stage(),
        weather.wet_meshes()
    );
    let setup = args.setup.as_deref().map(setup_root).transpose()?;
    let carried = args.damage_in.as_deref().map(rally::read_damage).transpose()?;
    // The harness's `.auto` fifth and sixth words set 0x1660850 and 0x1660814 before `StartGame`.
    let damage = args.damage.or_else(|| recording.and_then(|r| recorded_word(r, 4)).map(DamageLevel::from_word));
    let auto_gears = args.auto_gears.or_else(|| recording.and_then(|r| recorded_word(r, 5)).map(|w| w != 0));
    let aids = DriverAids { auto_gears: auto_gears.unwrap_or(args.aids.auto_gears), ..args.aids };
    let options = RaceOptions { damage: damage.unwrap_or_default(), aids, weather: preset };
    let ngp_physics = args.ngp_physics.clone().ok_or("--boot needs --ngp-physics (ReRBR steps NGP physics only)")?;
    let race = boot::Race {
        setup,
        ffb_sensitivity: args.ffb_sensitivity.unwrap_or([100; 3]),
        head_gasket_durability: args.head_gasket_durability.unwrap_or(1),
        car_pack,
        options,
        damage: carried,
        surface,
        ..boot::Race::quick_race(stage.number, car, tyre, weather, ngp_physics)
    };
    let race = match args.championship {
        Some(championship) => boot::Race { false_starts: args.false_starts, ..race.in_championship(championship) },
        None => race,
    };
    Ok((data, Some(race)))
}
