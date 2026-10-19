//! The weather a quick race starts with: the part of `CWeatherDaemon` (the object at `[0x7c36a8]`) the physics and
//! the stage load read. rbr-recomp `notes/weather.md` sections 2, 3 and 5 hold the evidence.
//!
//! `IRBRGame::StartGame` (0x626a70) stores its weather (0 good, 1 random, 2 bad) in `0x1660848`, sets the daemon's map
//! (`M9220`) and runs its `Update` (0x4aab40), which fills the daemon from one row of the season table (0x13f rows of
//! 0x18 bytes at 0x7d34c8, in the exe's `.data`):
//! - map 4: every field zero (letter `M`, dry, new);
//! - good (`InitA`, 0x4aa630): the first row of `misc\nice_w.ini`'s list that is the map's (`M9650`, 0x4a9650), new;
//! - bad (`InitB`, 0x4aa760): the same from `misc\bad_w.ini` (`M96f0`, 0x4a96f0); it leaves the wear as it was
//!   (`src/rbr/RBR_E23_Ambient.cpp`'s `InitB` draws one, which the exe does not);
//! - random (`E74_FindBest`, 0x5b53b0): the map's row number `rand() % n`, and the wear `rand() % 10 > 6`, so normal
//!   or worn.
//!
//! A map with no row in the list (68 and every map without rows) hits the `int3` in `InitA`/`InitB`. What the data
//! cannot give, [`quick_race`] takes as [`Draws`]: the random weather's two `rand()` draws (the CRT's state at the
//! race start) and the wear the bad weather keeps (the daemon's constructor leaves it unset; the first race of a
//! fresh game keeps 0, new, as the snapshots show).
//!
//! What the race reads back in the quick race's flow state (`Refresh`, 0x4a9250: the flow word 1 gives state 2, where
//! the getters return the fields): the track letter (`M9d50`, 0x4a9d50), so the stage file `track-<map>_<letter>`
//! (`StartGame` passes it to the stage load); the wetness (`Vf1c`, 0x4a9c40) and wear (`Vf18`, 0x4a9330), the two
//! words `E11_TyreCond` (0x46f6c0) gives `E09_LoadMaterials` (0x46a290) to pick the `.mat` table; and the wetness 2
//! test of the collision load's wet (puddle) meshes (0x4fdff0). Nothing else the daemon holds reaches the physics:
//! the tyres' wet grip is `CTyre::CScratch::BlendTable` (0x534e70) on the contact surfaces' `WaterFactor`, so the
//! weather reaches it only through the `.mat` table's surfaces.

use std::io::Read;

use rbr_formats::data::Data;
use rbr_formats::{pe, rbz};

pub use crate::crt::CrtRand;

/// The season table (`DAT_007d34c8`, E74's): 0x13f rows of six words.
pub const SEASON_TABLE: u32 = 0x007D_34C8;
pub const SEASON_ROWS: usize = 0x13F;
const ROW_BYTES: usize = 0x18;
/// The lists' end mark (`CountBad`, 0x4aa8c0).
const LIST_END: i32 = 123_456;
/// The track letters by index (`Vf04`, 0x4a9fc0).
const LETTERS: [char; 5] = ['M', 'N', 'E', 'O', 'S'];
/// The map whose race `Update` zeroes and `E11_TyreCond` keeps new and dry.
const FIXED_MAP: u32 = 4;

/// One season row: the map, then the words `Update` stores in the daemon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Row {
    pub map: u32,
    /// `+0x14`: the track letter (index into M, N, E, O, S).
    pub letter: u32,
    /// `+0x18`: the cloud (clear, partcloud, lightcloud, heavycloud).
    pub cloud: u32,
    /// `+0x40`: the cloud letter.
    pub cloud_letter: u32,
    /// `+0x44`: the weather (crisp ... heavyfog, `Vf0c`).
    pub preset: u32,
    /// `+0x1c`: the wetness before `Update`'s clamp to 2.
    pub wetness: u32,
}

/// The season table and the good and bad weather lists (row numbers).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Season {
    pub rows: Vec<Row>,
    pub nice: Vec<u32>,
    pub bad: Vec<u32>,
}

/// `StartGame`'s weather (`IRBRGame::ERBRWeatherType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preset {
    Good,
    Random,
    Bad,
}

impl Preset {
    pub fn from_id(id: u32) -> Option<Preset> {
        Some(match id {
            0 => Preset::Good,
            1 => Preset::Random,
            2 => Preset::Bad,
            _ => return None,
        })
    }

    pub fn id(self) -> u32 {
        match self {
            Preset::Good => 0,
            Preset::Random => 1,
            Preset::Bad => 2,
        }
    }
}

/// What the data cannot give: the random weather's row (an index into the season table, one of the map's; its
/// `rand()` draw, `None` takes the map's first) and the wear (0 new, 1 normal, 2 worn) the random weather draws (1 or
/// 2) or the bad weather keeps from before the race (0 in a fresh game). The good weather sets new.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Draws {
    pub row: Option<usize>,
    pub wear: u32,
}

/// The daemon's state after `Update`, as the race reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Weather {
    pub map: u32,
    /// The season row it came from (`None` for map 4).
    pub row: Option<usize>,
    pub letter: u32,
    /// 0 dry, 1 damp, 2 wet.
    pub wetness: u32,
    /// 0 new, 1 normal, 2 worn.
    pub wear: u32,
}

impl Season {
    /// The table's bytes (`SEASON_ROWS` rows) and the two lists' texts.
    pub fn parse(table: &[u8], nice: &str, bad: &str) -> Season {
        let word = |at: usize| u32::from_le_bytes(table[at..at + 4].try_into().unwrap());
        let rows = table
            .chunks_exact(ROW_BYTES)
            .take(SEASON_ROWS)
            .enumerate()
            .map(|(k, _)| {
                let w = |i: usize| word(k * ROW_BYTES + 4 * i);
                Row { map: w(0), letter: w(1), cloud: w(2), cloud_letter: w(3), preset: w(4), wetness: w(5) }
            })
            .collect();
        Season { rows, nice: list(nice), bad: list(bad) }
    }

    /// The exe's table (`RichardBurnsRally_SSE.exe`, from the 1.02 patch) and `Root/misc.rbz`'s lists.
    pub fn load(data: &Data) -> Result<Season, String> {
        let exe_path =
            data.resolve("RichardBurnsRally_SSE.exe").ok_or("RichardBurnsRally_SSE.exe: not found (RBR_PATCH)")?;
        let exe = std::fs::read(&exe_path).map_err(|e| format!("{}: {e}", exe_path.display()))?;
        let exe = pe::parse(&exe).map_err(|e| format!("the exe: {e}"))?;
        let table = exe.read(SEASON_TABLE, SEASON_ROWS * ROW_BYTES).ok_or("the exe has no season table")?;
        let misc = data.resolve("Root/misc.rbz").ok_or("Root/misc.rbz: not found")?;
        let mut zip = rbz::open(&misc).map_err(|e| format!("misc.rbz: {e}"))?;
        let mut text = |name: &str| -> Result<String, String> {
            let mut bytes = Vec::new();
            zip.by_name(name)
                .map_err(|e| format!("misc.rbz/{name}: {e}"))?
                .read_to_end(&mut bytes)
                .map_err(|e| format!("misc.rbz/{name}: {e}"))?;
            Ok(String::from_utf8_lossy(&bytes).into_owned())
        };
        let (nice, bad) = (text("Misc/nice_w.ini")?, text("Misc/bad_w.ini")?);
        Ok(Season::parse(table, &nice, &bad))
    }

    /// The rows of `map`, by row number.
    pub fn rows_of(&self, map: u32) -> impl Iterator<Item = usize> + '_ {
        (0..self.rows.len()).filter(move |&k| self.rows[k].map == map)
    }

    /// `M9650`/`M96f0`: the first row of `list` that is `map`'s.
    fn listed(&self, list: &[u32], map: u32) -> Option<usize> {
        list.iter().map(|&k| k as usize).find(|&k| self.rows.get(k).is_some_and(|r| r.map == map))
    }
}

impl Season {
    /// A community stage's weathers as a season of its own (ours; RallySimFans's choice is not traced): one row per
    /// `TrackSettings<id>.ini` section whose variant is installed (`rsf::Stage::installed_weathers`), or only those
    /// of variant `letter` when one is given. The good list holds the rows without rain, snow or fog, the bad list the
    /// others, each the other's when it is empty. The wetness is 2 in heavy rain, 1 in light rain, else the stage's
    /// `DefaultWetness` (else dry). A stage with no such section gets one crisp, clear row of its default variant.
    pub fn of_rsf(stage: &rbr_formats::rsf::Stage, letter: Option<char>) -> Season {
        let wetness = |preset: u32| match preset {
            4 => 2,
            3 => 1,
            _ => stage.default_wetness.unwrap_or(0).min(2),
        };
        let mut rows: Vec<(Row, bool)> = stage
            .installed_weathers()
            .filter(|w| letter.is_none_or(|l| w.letter_char() == l))
            .map(|w| {
                let row = Row {
                    map: stage.id,
                    letter: w.letter,
                    cloud: w.cloud,
                    cloud_letter: w.cloud_letter,
                    preset: w.preset,
                    wetness: wetness(w.preset),
                };
                (row, w.bad())
            })
            .collect();
        let fallback = letter.or(stage.default_variant()).and_then(|l| LETTERS.iter().position(|&c| c == l));
        if let (true, Some(index)) = (rows.is_empty(), fallback) {
            let row =
                Row { map: stage.id, letter: index as u32, cloud: 0, cloud_letter: 0, preset: 0, wetness: wetness(0) };
            rows.push((row, false));
        }
        let list = |bad: bool| -> Vec<u32> { (0..rows.len() as u32).filter(|&k| rows[k as usize].1 == bad).collect() };
        let (mut nice, mut bad) = (list(false), list(true));
        if nice.is_empty() {
            nice = bad.clone();
        } else if bad.is_empty() {
            bad = nice.clone();
        }
        Season { rows: rows.into_iter().map(|(row, _)| row).collect(), nice, bad }
    }
}

/// A weather list (`LoadWeather`, 0x4aa970): the text up to `$`, one number per line before its `;` comment, up to
/// the end mark.
fn list(text: &str) -> Vec<u32> {
    text.split('$')
        .next()
        .unwrap_or("")
        .lines()
        .filter_map(|line| line.split(';').next()?.split_whitespace().next())
        .map(atoi)
        .take_while(|&v| v != LIST_END)
        .map(|v| v as u32)
        .collect()
}

/// The CRT's `atoi`: an optional sign and the leading digits.
fn atoi(word: &str) -> i32 {
    let (sign, digits) = match word.strip_prefix('-') {
        Some(rest) => (-1, rest),
        None => (1, word.strip_prefix('+').unwrap_or(word)),
    };
    let value = digits
        .bytes()
        .take_while(u8::is_ascii_digit)
        .fold(0i32, |v, d| v.wrapping_mul(10).wrapping_add(i32::from(d - b'0')));
    sign * value
}

/// The daemon's state after `StartGame(map, ..., preset)` with the draws `draws`, or why the original stops.
pub fn quick_race(season: &Season, map: u32, preset: Preset, draws: Draws) -> Result<Weather, String> {
    if map == FIXED_MAP {
        return Ok(Weather { map, row: None, letter: 0, wetness: 0, wear: 0 });
    }
    let row = match preset {
        Preset::Good => season.listed(&season.nice, map),
        Preset::Bad => season.listed(&season.bad, map),
        Preset::Random => match draws.row {
            Some(k) if season.rows.get(k).is_some_and(|r| r.map == map) => Some(k),
            Some(k) => return Err(format!("season row {k} is not map {map}'s")),
            None => season.rows_of(map).next(),
        },
    }
    .ok_or(format!("map {map} has no {} weather row (the original stops at its int3)", name(preset)))?;
    let r = season.rows[row];
    Ok(Weather {
        map,
        row: Some(row),
        letter: r.letter,
        wetness: r.wetness.min(2),
        wear: match preset {
            Preset::Good => 0,
            Preset::Random if !(1..=2).contains(&draws.wear) => {
                return Err(format!("the random weather draws the wear normal or worn, not {}", draws.wear))
            }
            _ if draws.wear > 2 => return Err(format!("wear {}: not new, normal or worn", draws.wear)),
            _ => draws.wear,
        },
    })
}

/// The random weather's draws (`Update` with weather 1): `E74_FindBest` (0x5b53b0) counts the map's rows of the season
/// table and takes the `rand() % n`-th, then `Update` draws the wear `(rand() % 10 > 6) + 1` (normal or worn). `None`
/// when the map has no row (`FindBest` returns 0 and `Update` reaches its `int3`); then no wear is drawn. Map 4 takes
/// `Update`'s fixed branch before the weather: no draw.
pub fn random_draws(season: &Season, map: u32, rand: &mut CrtRand) -> Option<Draws> {
    if map == FIXED_MAP {
        return Some(Draws::default());
    }
    let rows: Vec<usize> = season.rows_of(map).collect();
    if rows.is_empty() {
        return None;
    }
    let row = rows[(rand.draw() as u32 % rows.len() as u32) as usize];
    let wear = u32::from(rand.draw() % 10 > 6) + 1;
    Some(Draws { row: Some(row), wear })
}

pub fn name(preset: Preset) -> &'static str {
    match preset {
        Preset::Good => "good",
        Preset::Random => "random",
        Preset::Bad => "bad",
    }
}

/// `E74_WeatherKind` (0x5b5ab0): the tyre selected from `E14_4a1330`'s surface and the weather daemon's wetness.
/// Surface 0 is gravel, 1 tarmac and 2 snow. Gravel and tarmac have three wetness variants; snow always uses tyre 6.
pub fn tyre_kind(surface: u32, wetness: u32) -> Option<u32> {
    match surface {
        0 if wetness <= 2 => Some(3 + wetness),
        1 if wetness <= 2 => Some(wetness),
        2 => Some(6),
        _ => None,
    }
}

impl Weather {
    /// The track letter (`Vf04`); an index past the table gives `?`.
    pub fn letter_char(&self) -> char {
        LETTERS.get(self.letter as usize).copied().unwrap_or('?')
    }

    /// The stage the race loads: `Maps/Tracks.ini`'s `TrackName` (`Maps\track-<map>`) and the letter.
    pub fn stage(&self) -> String {
        format!("track-{}_{}", self.map, self.letter_char())
    }

    /// `E11_TyreCond`'s words, the `.mat` entry's wear and wetness: map 4 gives `new` and `dry`.
    pub fn mat_words(&self) -> (&'static str, &'static str) {
        if self.map == FIXED_MAP {
            return ("new", "dry");
        }
        (["new", "normal", "worn"][self.wear.min(2) as usize], ["dry", "damp", "wet"][self.wetness.min(2) as usize])
    }

    /// The collision load's wet meshes (collision list A) load: `Vf1c() == 2`.
    pub fn wet_meshes(&self) -> bool {
        self.wetness == 2
    }

    /// The weather words `CGhostCarManager::Save` (0x492eb0) writes into a `.ghc`: the row's letter, cloud, weather
    /// and cloud letter, the wear and wetness. Map 4 gives zeros, as `Update` leaves them. Ours: the amount
    /// (`E25AppObj::M9b80`, `+0x34 * +0x30 + +0x2c`) is the precipitation target `Update` sets, 50 for light and 100
    /// for heavy rain or snow, not the ramped value at the save; `+0x3c` (set by the tweak and the RB challenge) is 0.
    pub fn ghost_words(&self, season: &Season) -> rbr_formats::ghost::Weather {
        let Some(row) = self.row.and_then(|k| season.rows.get(k)) else {
            return rbr_formats::ghost::Weather::default();
        };
        rbr_formats::ghost::Weather {
            track_letter: self.letter,
            cloud: row.cloud,
            weather: row.preset,
            cloud_letter: row.cloud_letter,
            wear: self.wear,
            wetness: self.wetness,
            amount: match row.preset {
                3 | 6 => 50.0,
                4 | 7 => 100.0,
                _ => 0.0,
            },
            unk3c: 0.0,
        }
    }
}

impl Season {
    /// The weather a `.ghc` of `map` brings (`E18_ConnGhostMemory::Dispatch` 0x4930d0 sets the daemon's letter,
    /// cloud, wetness and wear from the file): the map's first season row with the file's letter, cloud and wetness
    /// (ours: the row is how the launcher starts a race; the weather and cloud letter pick between equal rows), and the
    /// file's wear. `None` when no row matches (a file of another map, or one without weather words).
    pub fn ghost_weather(&self, map: u32, words: &rbr_formats::ghost::Weather) -> Option<Weather> {
        let fits = |k: &usize| {
            let r = &self.rows[*k];
            r.letter == words.track_letter && r.cloud == words.cloud && r.wetness.min(2) == words.wetness
        };
        let exact =
            |k: &usize| self.rows[*k].preset == words.weather && self.rows[*k].cloud_letter == words.cloud_letter;
        let row = self.rows_of(map).filter(fits).find(exact).or_else(|| self.rows_of(map).find(fits))?;
        let wetness = self.rows[row].wetness.min(2);
        Some(Weather { map, row: Some(row), letter: self.rows[row].letter, wetness, wear: words.wear.min(2) })
    }
}

/// The start clock's time of day at the stage's start, seconds after midnight: the stage start board (`START_CLOCK`,
/// HedgeHog3D 0x10070b20, `start_clock_digits.dds`) shows `CWeatherDaemon::Vf24` (0x4a9400), the daemon's hours,
/// minutes and seconds (+0xac, +0xb0, +0xb4). `Vf2c` (0x4a9eb0) sets them by the cloud letter (`M9cf0`): `M` 8:15,
/// `N` 12:22, `E` 17:23, then each frame adds its time (0x4a9a20, 60 s a minute, 24 h). Another letter leaves them as
/// they were (`None`). In a rally whose stage says so (`FUN_005733c0`) the season's start hour and minute are taken
/// instead (not ported).
pub fn start_clock(cloud_letter: u32) -> Option<f32> {
    let (hours, minutes) = match cloud_letter {
        0 => (8.0, 15.0),
        1 => (12.0, 22.0),
        2 => (17.0, 23.0),
        _ => return None,
    };
    Some(hours * 3600.0 + minutes * 60.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A community stage of variants M and O: good weather takes the first dry section, bad the first wet one, and
    /// a stage with no sections its default variant, crisp and dry.
    #[test]
    fn a_community_stage_is_its_own_season() {
        use rbr_formats::rsf::{Stage, WeatherRow};
        let rows =
            ["129M_crisp_clear", "129O_M_lightrain_heavycloud", "129O_M_heavyrain_heavycloud", "129S_crisp_clear"];
        let stage = Stage {
            id: 129,
            name: "Peyregrosse".into(),
            folder: "/rsf/Maps/129-Peyregrosse".into(),
            track: "track-129".into(),
            variants: vec!['M', 'O'],
            particles: None,
            surface: None,
            length: None,
            author: None,
            country: None,
            reference_time: None,
            default_wetness: None,
            animation_path: None,
            weathers: rows.iter().filter_map(|r| WeatherRow::parse(129, r)).collect(),
            problem: None,
        };
        let season = Season::of_rsf(&stage, None);
        assert_eq!(season.rows.len(), 3, "S is not installed");
        let good = quick_race(&season, 129, Preset::Good, Draws::default()).unwrap();
        assert_eq!((good.letter_char(), good.wetness), ('M', 0));
        let bad = quick_race(&season, 129, Preset::Bad, Draws::default()).unwrap();
        assert_eq!((bad.stage(), bad.wetness), ("track-129_O".to_owned(), 1));
        let only_o = Season::of_rsf(&stage, Some('O'));
        assert_eq!(quick_race(&only_o, 129, Preset::Good, Draws::default()).unwrap().stage(), "track-129_O");
        let bare = Season::of_rsf(&Stage { weathers: vec![], ..stage }, None);
        let w = quick_race(&bare, 129, Preset::Bad, Draws::default()).unwrap();
        assert_eq!((w.stage(), w.wetness), ("track-129_O".to_owned(), 0));
    }

    #[test]
    fn the_start_clock_starts_by_the_cloud_letter() {
        assert_eq!(start_clock(0), Some(8.0 * 3600.0 + 15.0 * 60.0));
        assert_eq!(start_clock(2), Some(17.0 * 3600.0 + 23.0 * 60.0));
        assert_eq!(start_clock(3), None);
    }

    fn table(rows: &[[u32; 6]]) -> Vec<u8> {
        let mut bytes: Vec<u8> = rows.iter().flatten().flat_map(|w| w.to_le_bytes()).collect();
        bytes.resize(SEASON_ROWS * ROW_BYTES, 0);
        bytes
    }

    fn season() -> Season {
        Season::parse(
            &table(&[
                [20, 1, 0, 1, 0, 0],
                [20, 3, 3, 1, 4, 2],
                [20, 3, 3, 1, 8, 1],
                [52, 0, 0, 0, 1, 1],
                [52, 3, 3, 0, 4, 5],
            ]),
            "; Nice\n;\n3\t; Track 52\n\n0 ; Track 20\n123456\n$ 1",
            "; Bad\n1 ; Track 20\n4 ; Track 52\n123456\n$",
        )
    }

    #[test]
    fn a_ghost_keeps_the_weather_and_brings_it_back() {
        let season = season();
        let weather = quick_race(&season, 20, Preset::Random, Draws { row: Some(1), wear: 2 }).unwrap();
        let words = weather.ghost_words(&season);
        assert_eq!((words.track_letter, words.cloud, words.weather, words.cloud_letter), (3, 3, 4, 1));
        assert_eq!((words.wear, words.wetness, words.amount), (2, 2, 100.0));
        assert_eq!(season.ghost_weather(20, &words), Some(weather));
        assert_eq!(season.ghost_weather(20, &rbr_formats::ghost::Weather { cloud: 1, ..words }), None);
    }

    #[test]
    fn lists_stop_at_the_end_mark_and_the_dollar() {
        assert_eq!(list("1\n; c\n2 ;x\n123456\n3\n"), [1, 2]);
        assert_eq!(list("7\n$\n8\n"), [7]);
        assert_eq!(season().nice, [3, 0]);
    }

    #[test]
    fn good_and_bad_take_the_first_listed_row_of_the_map() {
        let s = season();
        let good = quick_race(&s, 20, Preset::Good, Draws::default()).unwrap();
        assert_eq!((good.row, good.stage(), good.mat_words()), (Some(0), "track-20_N".into(), ("new", "dry")));
        let bad = quick_race(&s, 20, Preset::Bad, Draws { row: None, wear: 2 }).unwrap();
        assert_eq!((bad.stage(), bad.mat_words(), bad.wet_meshes()), ("track-20_O".into(), ("worn", "wet"), true));
        let japan = quick_race(&s, 52, Preset::Bad, Draws::default()).unwrap();
        assert_eq!((japan.wetness, japan.wear), (2, 0), "the wetness is clamped to 2; the wear is kept");
    }

    #[test]
    fn random_takes_the_drawn_row_of_the_map() {
        let s = season();
        let w = quick_race(&s, 20, Preset::Random, Draws { row: Some(2), wear: 1 }).unwrap();
        assert_eq!((w.letter_char(), w.mat_words()), ('O', ("normal", "damp")));
        assert!(quick_race(&s, 20, Preset::Random, Draws { row: Some(3), wear: 1 }).is_err());
        assert!(quick_race(&s, 20, Preset::Random, Draws { row: Some(2), wear: 0 }).is_err());
        let draws = Draws { row: None, wear: 2 };
        assert_eq!(quick_race(&s, 52, Preset::Random, draws).unwrap().row, Some(3));
    }

    #[test]
    fn crt_rand_is_msvcr71s_stream() {
        // srand(1) is the CRT's initial state: 41, 18467, 6334, 26500, 19169 (msvcr71's documented first values).
        let mut rand = CrtRand::srand(1);
        assert_eq!([0; 5].map(|_| rand.draw()), [41, 18467, 6334, 26500, 19169]);
        let mut rand = CrtRand::srand(0x6530_0000);
        assert!((0..1000).all(|_| rand.draw() <= 0x7FFF));
    }

    #[test]
    fn random_draws_take_the_map_row_then_the_wear() {
        let s = season();
        // srand(1): 41 % 3 = 2, the map's third row (2); 18467 % 10 = 7 > 6, worn.
        let mut rand = CrtRand::srand(1);
        assert_eq!(random_draws(&s, 20, &mut rand), Some(Draws { row: Some(2), wear: 2 }));
        // Then 6334 % 2 = 0, map 52's first row (3); 26500 % 10 = 0, normal.
        assert_eq!(random_draws(&s, 52, &mut rand), Some(Draws { row: Some(3), wear: 1 }));
        // A map with no row draws nothing, and map 4 does not draw.
        let before = rand;
        assert_eq!(random_draws(&s, 68, &mut rand), None);
        assert_eq!(random_draws(&s, 4, &mut rand), Some(Draws::default()));
        assert_eq!(rand, before);
        let w = quick_race(&s, 20, Preset::Random, random_draws(&s, 20, &mut CrtRand::srand(1)).unwrap()).unwrap();
        assert_eq!((w.stage(), w.mat_words()), ("track-20_O".into(), ("worn", "damp")));
    }

    #[test]
    fn map_4_is_fixed_and_unlisted_maps_stop() {
        let s = season();
        let w = quick_race(&s, 4, Preset::Bad, Draws { row: None, wear: 2 }).unwrap();
        assert_eq!((w.stage(), w.mat_words(), w.wet_meshes()), ("track-4_M".into(), ("new", "dry"), false));
        assert!(quick_race(&s, 68, Preset::Good, Draws::default()).is_err());
    }

    #[test]
    fn e74_weather_kind_maps_every_surface_and_wetness_to_its_tyre() {
        // E74_WeatherKind's branches: gravel 3/4/5, tarmac 0/1/2, snow 6 for wetness 0/1/2.
        let asm = [[Some(3), Some(4), Some(5)], [Some(0), Some(1), Some(2)], [Some(6), Some(6), Some(6)]];
        for (surface, expected) in asm.iter().enumerate() {
            for (wetness, tyre) in expected.iter().enumerate() {
                assert_eq!(tyre_kind(surface as u32, wetness as u32), *tyre);
            }
        }
        assert_eq!(tyre_kind(0, 3), None);
        assert_eq!(tyre_kind(1, 3), None);
        assert_eq!(tyre_kind(3, 0), None);
    }
}
