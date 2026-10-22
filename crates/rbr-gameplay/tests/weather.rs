//! The quick race's weather (`rbr_gameplay::weather`) on the retail data and against the original's snapshots.
//!
//! Runs only when `RBR_DATA` names the install and `RBR_PATCH` the 1.02 patch files (the exe); the snapshot check also
//! needs `RBR_RECORDINGS` (the drives' `.auto`) and the snapshots in `RBR_STEP_SNAPSHOT_DIR` or `RBR_SNAPS` (default
//! `/tmp/rbr-snaps`). Skipped otherwise.

use std::path::PathBuf;

use rbr_formats::data::Data;
use rbr_formats::mat;
use rbr_gameplay::weather::{self, Draws, Preset, Season, Weather};
use rbr_physics::snapshot::Snapshot;

/// `CWeatherDaemon` (its pointer) and the fields the race reads.
const DAEMON: u32 = 0x007C_36A8;
const LETTER: u32 = 0x14;
const CLOUD: u32 = 0x18;
const WETNESS: u32 = 0x1C;
const WEAR: u32 = 0x20;
const CLOUD_LETTER: u32 = 0x40;
const PRESET: u32 = 0x44;

fn data() -> Option<Data> {
    let data = Data::from_env()?;
    data.patch()?;
    Some(data)
}

/// The maps the install has stages of (`Maps/track-<map>_<letter>.mat`).
fn retail_maps(data: &Data) -> Vec<u32> {
    let mut maps: Vec<u32> = std::fs::read_dir(data.install().join("Maps"))
        .unwrap()
        .filter_map(|e| {
            let name = e.ok()?.file_name().into_string().ok()?;
            name.strip_prefix("track-")?.strip_suffix(".mat")?.split_once('_')?.0.parse().ok()
        })
        .collect();
    maps.sort_unstable();
    maps.dedup();
    maps
}

/// Every weather of `maps`: good, bad and each row of random, with each wear they can take.
fn every_weather(season: &Season, maps: &[u32]) -> Vec<(u32, Preset, Result<Weather, String>)> {
    let mut out = vec![];
    for &map in maps {
        for wear in 0..=2 {
            for preset in [Preset::Good, Preset::Bad] {
                out.push((map, preset, weather::quick_race(season, map, preset, Draws { row: None, wear })));
            }
            for row in season.rows_of(map).filter(|_| wear > 0) {
                let draws = Draws { row: Some(row), wear };
                out.push((map, Preset::Random, weather::quick_race(season, map, Preset::Random, draws)));
            }
        }
    }
    out
}

#[test]
fn every_retail_weather_selects_a_mat_table_of_its_stage() {
    let Some(data) = data() else {
        eprintln!("RBR_DATA or RBR_PATCH unset; skipped");
        return;
    };
    let season = Season::load(&data).expect("the season");
    assert_eq!(season.rows.len(), weather::SEASON_ROWS);
    let maps = retail_maps(&data);
    let (mut checked, mut stops, mut loaded) = (0, vec![], vec![]);
    for (map, preset, w) in every_weather(&season, &maps) {
        let w = match w {
            Ok(w) => w,
            Err(_) => {
                stops.push((map, preset));
                continue;
            }
        };
        let stage = w.stage();
        let path = data
            .resolve(&format!("Maps/{stage}.mat"))
            .unwrap_or_else(|| panic!("map {map} {preset:?}: {stage}.mat is missing"));
        let file = mat::parse(&std::fs::read(path).unwrap()).unwrap();
        let (wear, wetness) = w.mat_words();
        assert!(
            mat::select(&file.entries, wear, wetness).is_some(),
            "map {map} {preset:?}: {stage}.mat has no {wear} {wetness} entry"
        );
        checked += 1;
        loaded.push(stage);
    }
    stops.sort_unstable_by_key(|&(m, p)| (m, p.id()));
    stops.dedup();
    loaded.sort_unstable();
    loaded.dedup();
    eprintln!("{checked} weathers of {} maps checked; no row (int3): {stops:?}", maps.len());
    eprintln!("{} stages loaded by some weather: {loaded:?}", loaded.len());
    // Map 68 is the only retail map with a row and no listed one.
    assert_eq!(stops, [(68, Preset::Good), (68, Preset::Bad)]);
}

#[test]
fn every_quick_race_stage_has_a_good_and_a_bad_weather() {
    let Some(data) = data() else {
        eprintln!("RBR_DATA or RBR_PATCH unset; skipped");
        return;
    };
    let season = Season::load(&data).expect("the season");
    // The rally maps (Arctic, British, Australia, Mont Blanc, Japan, USA).
    for map in [10..=15, 20..=25, 31..=36, 41..=46, 51..=56, 61..=66].into_iter().flatten() {
        for preset in [Preset::Good, Preset::Bad] {
            let w = weather::quick_race(&season, map, preset, Draws::default()).unwrap();
            assert!(
                data.resolve(&format!("Maps/{}.lbs", w.stage())).is_some(),
                "map {map} {preset:?}: {} has no .lbs",
                w.stage()
            );
        }
    }
}

/// The drives' snapshots: the daemon's fields after the race start equal the derivation's for the drive's `.auto`
/// (map and weather); for the random and bad weathers, the draws (the row, the wear) are read from the snapshot, and
/// the derivation must give its fields from them.
#[test]
fn the_original_snapshots_hold_the_derived_weather() {
    let (Some(data), Some(recordings)) = (data(), std::env::var_os("RBR_RECORDINGS").map(PathBuf::from)) else {
        eprintln!("RBR_DATA, RBR_PATCH or RBR_RECORDINGS unset; skipped");
        return;
    };
    let snaps = ["RBR_STEP_SNAPSHOT_DIR", "RBR_SNAPS"]
        .into_iter()
        .find_map(std::env::var_os)
        .map_or_else(|| PathBuf::from("/tmp/rbr-snaps"), PathBuf::from);
    let season = Season::load(&data).expect("the season");
    let mut checked = 0;
    for (snapshot, drive) in [
        ("vcu-cov.snap-1000.bin", "tanner-corolla"),
        ("vcu-v4-gravel-corolla-55.snap-1000.bin", "v4-gravel-corolla-55"),
        ("vcu-v2-snow-lancer-13.snap-1000.bin", "v2-snow-lancer-13"),
        ("vcu-v3-gravel-lancer-23.snap-1000.bin", "v3-gravel-lancer-23"),
        ("vcu-wb-wet-corolla-54.snap-1000.bin", "wb-wet-corolla-54"),
        ("vcu-wb-damp-lancer-64.snap-1000.bin", "wb-damp-lancer-64"),
        ("vcu-wb-snow-xsara-10.snap-1000.bin", "wb-snow-xsara-10"),
    ] {
        let (path, auto) = (snaps.join(snapshot), recordings.join(format!("drive-{drive}.auto")));
        let (Ok(snap), Ok(auto)) = (Snapshot::open(&path), std::fs::read_to_string(&auto)) else {
            eprintln!("{snapshot}: not found; skipped");
            continue;
        };
        let words: Vec<u32> = auto.split_whitespace().map(|w| w.parse().unwrap()).collect();
        let (map, preset) = (words[0], Preset::from_id(words[2]).unwrap());
        let mem = snap.mem();
        let daemon = mem.u32(DAEMON);
        let field = |at| mem.u32(daemon + at);
        let row = |k: usize| season.rows[k];
        let fits = |k: usize| {
            let r = row(k);
            [r.letter, r.cloud, r.cloud_letter, r.preset, r.wetness.min(2)]
                == [field(LETTER), field(CLOUD), field(CLOUD_LETTER), field(PRESET), field(WETNESS)]
        };
        let draws = Draws { row: season.rows_of(map).find(|&k| fits(k)), wear: field(WEAR) };
        let w = weather::quick_race(&season, map, preset, draws).unwrap();
        assert!(fits(w.row.unwrap()), "{snapshot}: the daemon's row is not the derived one");
        assert_eq!((field(WETNESS), field(WEAR)), (w.wetness, w.wear), "{snapshot}");
        eprintln!("{snapshot}: {} wetness {} wear {} ({:?})", w.stage(), w.wetness, w.wear, w.mat_words());
        checked += 1;
    }
    if checked == 0 {
        eprintln!("no snapshots in {}; skipped", snaps.display());
        return;
    }
    assert!(checked >= 2, "fewer than two snapshots found");
}

/// Every season row of a stage the install has names a section of `Maps/TrackSettings.ini` (`Weather::section`), so the
/// renderer draws the row's own sky, light and fog. Map 4 has no row; the nine clear and part-cloud rows of `69S` name
/// sections the file lacks (the game's lookup for them is not traced; the renderer falls back to its own pick).
#[test]
fn every_retail_row_names_a_track_settings_section() {
    let Some(data) = data() else {
        eprintln!("skipping: RBR_DATA and RBR_PATCH not set");
        return;
    };
    let season = Season::load(&data).unwrap();
    let text = std::fs::read(data.install().join("Maps/TrackSettings.ini")).unwrap();
    let sections = rbr_formats::ini::parse(&String::from_utf8_lossy(&text)).unwrap();
    let mut missing = vec![];
    let mut named = 0;
    for map in retail_maps(&data).into_iter().filter(|&m| m != 4) {
        for row in season.rows_of(map) {
            let race = weather::quick_race(&season, map, Preset::Random, Draws { row: Some(row), wear: 1 }).unwrap();
            if !data.install().join(format!("Maps/{}.lbs", race.stage())).exists() {
                continue;
            }
            let section = race.section().unwrap();
            named += 1;
            if !sections.iter().any(|s| s.name == section) {
                missing.push(section);
            }
        }
    }
    assert!(named > 100, "only {named} rows of installed stages");
    let lacking: Vec<String> = ["M", "N", "E"]
        .iter()
        .flat_map(|l| ["crisp_clear", "hazy_clear", "crisp_partcloud"].map(|w| format!("69S_{l}_{w}")))
        .collect();
    assert_eq!(missing, lacking);
}
