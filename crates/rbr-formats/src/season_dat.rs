//! Reader for `season.dat` (in the install's `Root/`): the championship's rallies, a `CHELALIC` archive ([`crate::dat`])
//! of `.lsp` files under `L:\Rally_7\TargetMedia_Develop\XBox\Debug\World\Misc\Season\<rally>\`, the root
//! `CSeasonStageManager::LoadData` (0x5d6290) opens.
//!
//! Per rally (the six names of the exe's table at 0x7d615c, in id order: USA, Finland, England, Canberra, Japan,
//! France):
//! - `setup.lsp` (`E81SeasonB::LoadSetup` 0x5d6450): one `"RallySetup"` block, `FogChance`, `RainChance`,
//!   `Temperature`, `MaxRain`, `MidRain`;
//! - `s<kind>_st<n>.lsp`, kind 1-3 (the season's level, `E80Blk28c::Init6`'s argument): one `"Weather"` block,
//!   `Stage` (the map), `TimeOfDay` (the track letter's index: M, N, E, O), `TintSet`, `Hour`, `Minute`. Files `st1` to
//!   `st6` are the rally's six stages (`LoadStage` 0x5d66e0 with index + 1); `st0` is the extra stage `LoadFinal`
//!   (0x5d6790) reads into the block's +0x1fc (the rally's start; its use is not traced).
//!
//! `Init6` (0x5cea80) stores the stage's `Stage`, `TimeOfDay`, `Hour` and `Minute` (`Get18`, `Get28`, `Get48`,
//! `Get58`); `TintSet` is not read there. The weather of each stage is then drawn by the season's forecast (the rest of
//! `Init6`, `rand()`), not from these files.

use crate::dat;
use crate::lsp::{self, Node};

/// The rallies' folder names by rally id (0x7d615c).
pub const RALLIES: [&str; 6] = ["USA", "Finland", "England", "Canberra", "Japan", "France"];
/// The season levels with files (`s1` to `s3`).
pub const LEVELS: usize = 3;
/// The archive's folder for the rallies.
const ROOT: &str = "L:/Rally_7/TargetMedia_Develop/XBox/Debug/World/Misc/Season";

/// One stage's `"Weather"` block. `Init6` stores `TimeOfDay` in the stage's letter word, which the season's forecast
/// then overwrites with its drawn row's letter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StageInfo {
    pub map: u32,
    pub time_of_day: u32,
    pub tint_set: u32,
    pub hour: u32,
    pub minute: u32,
}

impl StageInfo {
    /// The track letter (`CWeatherDaemon` `Vf04`'s table, the index `TimeOfDay`).
    pub fn letter(&self) -> char {
        ['M', 'N', 'E', 'O', 'S'].get(self.time_of_day as usize).copied().unwrap_or('O')
    }
}

/// A rally's `"RallySetup"` block.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RallySetup {
    pub fog_chance: f32,
    pub rain_chance: f32,
    pub temperature: f32,
    pub max_rain: f32,
    pub mid_rain: f32,
}

/// One rally: its setup and, per level, the files `st0` to `st6`.
#[derive(Debug, Clone, PartialEq)]
pub struct Rally {
    pub name: String,
    pub setup: RallySetup,
    pub levels: Vec<[StageInfo; 7]>,
}

impl Rally {
    /// The six stages of `level` (1 to 3), in order (`st1` to `st6`).
    pub fn stages(&self, level: usize) -> Option<&[StageInfo]> {
        self.levels.get(level.checked_sub(1)?).map(|files| &files[1..])
    }
}

/// The values of the block named `name`: the pairs after its quoted name, as `key value` atoms.
fn block<'a>(nodes: &'a [Node], name: &str) -> Option<&'a [Node]> {
    nodes.iter().find_map(|n| match n {
        Node::List(items) => match items.first() {
            Some(Node::Quoted(q)) if q == name => Some(&items[1..]),
            _ => block(items, name),
        },
        _ => None,
    })
}

fn value(items: &[Node], key: &str) -> Result<f32, String> {
    items
        .windows(2)
        .find_map(|pair| match pair {
            [Node::Atom(k), Node::Atom(v)] if k == key => v.parse::<f32>().ok(),
            _ => None,
        })
        .ok_or(format!("no {key}"))
}

/// Parses a `"Weather"` file.
pub fn parse_stage(text: &str) -> Result<StageInfo, String> {
    let nodes = lsp::parse(text).map_err(|e| e.to_string())?;
    let b = block(&nodes, "Weather").ok_or("no Weather block")?;
    let int = |key| value(b, key).map(|v| v as u32);
    Ok(StageInfo {
        map: int("Stage")?,
        time_of_day: int("TimeOfDay")?,
        tint_set: int("TintSet")?,
        hour: int("Hour")?,
        minute: int("Minute")?,
    })
}

/// Parses a `"RallySetup"` file.
pub fn parse_setup(text: &str) -> Result<RallySetup, String> {
    let nodes = lsp::parse(text).map_err(|e| e.to_string())?;
    let b = block(&nodes, "RallySetup").ok_or("no RallySetup block")?;
    Ok(RallySetup {
        fog_chance: value(b, "FogChance")?,
        rain_chance: value(b, "RainChance")?,
        temperature: value(b, "Temperature")?,
        max_rain: value(b, "MaxRain")?,
        mid_rain: value(b, "MidRain")?,
    })
}

/// Reads the six rallies from the archive's bytes.
pub fn parse(bytes: &[u8]) -> Result<Vec<Rally>, String> {
    let entries = dat::parse(bytes).map_err(|e| format!("season.dat: {e}"))?;
    let text = |rally: &str, file: &str| -> Result<String, String> {
        let path = format!("{ROOT}/{rally}/{file}");
        let entry = entries
            .iter()
            .find(|e| e.path.eq_ignore_ascii_case(&path))
            .ok_or(format!("season.dat: no {rally}/{file}"))?;
        let data = dat::file_data(bytes, entry).ok_or(format!("season.dat: {rally}/{file} out of range"))?;
        Ok(String::from_utf8_lossy(data).into_owned())
    };
    RALLIES
        .iter()
        .map(|&name| {
            let setup = parse_setup(&text(name, "setup.lsp")?).map_err(|e| format!("{name}/setup.lsp: {e}"))?;
            let levels = (1..=LEVELS)
                .map(|kind| {
                    let mut files = [StageInfo { map: 0, time_of_day: 0, tint_set: 0, hour: 0, minute: 0 }; 7];
                    for (n, slot) in files.iter_mut().enumerate() {
                        let file = format!("s{kind}_st{n}.lsp");
                        *slot = parse_stage(&text(name, &file)?).map_err(|e| format!("{name}/{file}: {e}"))?;
                    }
                    Ok(files)
                })
                .collect::<Result<Vec<_>, String>>()?;
            Ok(Rally { name: name.to_owned(), setup, levels })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_blocks() {
        let stage =
            "((\"Weather\"\n Stage           63\n TimeOfDay       1\n TintSet         1\n Hour            12\n \
                     Minute          30\n ))";
        let s = parse_stage(stage).unwrap();
        assert_eq!(s, StageInfo { map: 63, time_of_day: 1, tint_set: 1, hour: 12, minute: 30 });
        assert_eq!(s.letter(), 'N');
        let setup = "((\"RallySetup\"\n FogChance 0.000000\n RainChance 30.000000\n Temperature 20.000000\n \
                     MaxRain 80.000000\n MidRain 40.000000\n ))";
        assert_eq!(parse_setup(setup).unwrap().rain_chance, 30.0);
        assert!(parse_stage("((\"Weather\" Stage 1 ))").is_err());
    }

    /// Every retail rally, level and stage file (`RBR_DATA`).
    #[test]
    fn retail_season_dat() {
        let Some(data) = crate::data::Data::from_env() else { return };
        let path = data.resolve("Root/season.dat").expect("Root/season.dat");
        let rallies = parse(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(rallies.len(), 6);
        let usa = &rallies[0];
        assert_eq!(usa.setup.rain_chance, 30.0);
        let stages = usa.stages(1).unwrap();
        assert_eq!(stages.len(), 6);
        assert_eq!((stages[0].map, stages[0].letter(), stages[0].hour, stages[0].minute), (63, 'N', 12, 30));
        // Every stage of every rally is one of the rally's own six maps (USA 61-66, Finland 10-15, ...).
        for r in &rallies {
            for level in 1..=LEVELS {
                let maps: Vec<u32> = r.stages(level).unwrap().iter().map(|s| s.map).collect();
                let lo = maps.iter().min().unwrap();
                assert!(maps.iter().all(|m| m - lo < 7), "{} s{level}: {maps:?}", r.name);
                // Every stage's map is installed (its letter is the forecast's row's: `TimeOfDay` alone names no
                // installed file for some, e.g. Canberra's 34 at N).
                for s in r.stages(level).unwrap() {
                    let any = "MNEOS".chars().any(|l| data.resolve(&format!("Maps/track-{}_{l}.trk", s.map)).is_some());
                    assert!(any, "{} s{level}: no track-{}", r.name, s.map);
                }
            }
        }
    }
}
