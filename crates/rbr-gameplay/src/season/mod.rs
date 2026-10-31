//! The championship season around the rally flow: its creation, the rallies'
//! forecast weather ([`init6`], [`Block`]), the AI field, the rally points
//! and the standings. The event queue and the clock that run the season are
//! [`crate::season_events`]. The state is the profile's `RBR7` season, so a
//! season is saved and continued in the game's own format. Layout evidence
//! lives in [`layout`].
//!
//! A new season draws six levels, then each rally's forecast in block order
//! (England, Japan, Finland, USA, France, Canberra), then the AI setup draws
//! on the same stream. The player takes the picked team's first seat. Each
//! rally starts in season-points order, the first by qualifying rank; each
//! stage runs slowest first, retired entries last. Points go 10 down to 1 to
//! the first eight finishers on time plus penalty. The AI field's drivers,
//! cars, budgets, manager styles and spending persist in the save's stores.
//!
//! The driver-class draws are ours: the game makes them at its start on a
//! time seed.

use rbr_formats::ai_ini::{Master, Roster};
use rbr_formats::saves::after_season::{CarStore, DriverStore, Stores, TeamStore};
use rbr_formats::saves::season::{Season, DRIVERS};

use crate::ai_season::{self, CarState, DriverState, TeamCosts};
use crate::crt::CrtRand;
use crate::race_options::{ChampionshipOptions, DamageLevel, Skill};

mod forecast;
pub mod layout;

#[cfg(test)]
pub(crate) use forecast::{BLOCK, SLOT};

pub use forecast::{blocks, init6, new_blocks, Block, Slot, RALLY_ORDER};
pub use layout::{
    add_rally_penalty, add_rally_points, advance_clock, advance_rally, advance_stage, clock, entry_car, entry_excluded,
    entry_id, entry_name, entry_rank, entry_team_slot, false_starts, fill_neg, first_stage_order, forecast_rally_id,
    forecast_slot_time, forecast_start_time, head_time, inc_false_starts, out_of_rally, player_entry, position,
    rally_penalty, rally_points, rally_total, rank_order_override, rewind_stage, season_points, set_clock,
    set_entry_rank, set_first_stage_order, set_head_time, set_start_order, skill_level, start_key, start_order,
    team_ids, wrapping_season_points, ENTRIES,
};
pub(crate) use layout::{bubble, push_pair, retired_in, set_stage_start_time, stage_retired};

/// The rally and stage in progress, with the stage run before it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RallyPosition {
    pub rally: u32,
    pub stage: u32,
    pub previous_stage: u32,
}

/// The season clock: hour and minute.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SeasonClock {
    pub hour: u32,
    pub minute: u32,
}

/// The points for places 1-8.
pub const POINTS: [u32; 8] = [10, 8, 6, 5, 4, 3, 2, 1];
/// The player's id (the first human gets id -2).
pub const PLAYER_ID: i32 = -2;

/// The season's seed over the local time.
pub fn seed(second: u32, minute: u32, millisecond: u32, hour: u32, weekday: u32, day: u32, year: u32) -> u32 {
    second + minute + millisecond + hour + weekday + day + year
}

/// The player of a new season: the profile's name, the picked team and the
/// rank, the profile's first locked car capped at 15, which orders the first
/// rally's start.
#[derive(Debug, Clone, Copy)]
pub struct Player<'a> {
    pub name: &'a str,
    pub team: u32,
    pub rank: u32,
}

/// The player's rank for a new season: the first locked car, capped at 15.
pub fn player_rank(profile: &rbr_formats::saves::profile::Profile) -> u32 {
    let first = profile.cars.iter().position(|&c| c == 0).unwrap_or(profile.cars.len()) as u32;
    first.min(15)
}

/// Each `aidrv.ini` driver's class from its skills: one `rand()` per driver
/// as the file loads, class 0, 1 or 2 by the skill sum. The setup reads it
/// for the drivers' qualifying ranks.
///
/// The game parses `aidrv.ini` at its start on a time seed, so the draws are
/// not reproducible; the caller picks the stream.
pub fn driver_classes(roster: &Roster, rng: &mut CrtRand) -> Vec<u32> {
    roster
        .drivers
        .iter()
        .map(|d| {
            let sum = [d.condition, d.endurance, d.technique, d.speed]
                .iter()
                .chain(&d.skills)
                .fold(0u32, |a, &v| a.wrapping_add(v as u32));
            let r = (rng.draw() % 3) as u32;
            if sum < 30 - r {
                0
            } else if sum < 32 - r {
                1
            } else {
                2
            }
        })
        .collect()
}

/// A new season before the first rally: one player, ids 0-13, the player in
/// entry `2 * team` with id -2, the rally, stage and previous stage unset.
/// Each team's two entries drive its car ([`TEAM_CARS`]; the player in the
/// team's first), and each AI entry in order gets its `aidrv.ini` driver's
/// name and a rank of `rand() % 6` plus 2, 5 or 8 by driver class. `rng` goes
/// on from [`new_blocks`]. The options set the skill and damage level.
pub fn create(
    roster: &Roster,
    blocks: &[Block],
    options: ChampionshipOptions,
    player: Player,
    classes: &[u32],
    rng: &mut CrtRand,
) -> Season {
    let mut season = layout::new_season(blocks, options, player);
    for k in 0..season.drivers.len() {
        let id = entry_id(&season, k).unwrap_or(-1);
        if id < 0 {
            continue;
        }
        let base = classes.get(id as usize).and_then(|&c| [2, 5, 8].get(c as usize));
        if let Some(base) = base {
            set_entry_rank(&mut season, k, (rng.draw() % 6) as u32 + base);
        }
        if let Some(d) = roster.drivers.get(id as usize) {
            layout::set_entry_name(&mut season, k, &d.name);
        }
    }
    season
}

/// The team offers' order: the rows' team ids.
pub const TEAM_OFFERS: [u32; 7] = [4, 2, 5, 1, 3, 6, 0];

/// The cars a new 1.02 profile has open. The launcher has no unlocks, so
/// every team is offered.
pub const OPEN_CARS: [u32; 3] = [3, 5, 6];

/// The season's championship options: the skill and the damage level the
/// car takes in championship mode.
pub fn options(season: &Season) -> ChampionshipOptions {
    ChampionshipOptions {
        skill: Skill::from_word(season.rally_level()),
        damage: DamageLevel::from_word(season.damage_level()),
    }
}

/// The team of an entry: through its team slot into the season's team list.
pub fn team_of<'a>(season: &Season, roster: &'a Roster, entry: usize) -> Option<&'a rbr_formats::ai_ini::Team> {
    let slot = entry_team_slot(season, entry)? as usize;
    roster.teams.get(*team_ids(season).get(slot)? as usize)
}

/// The player's crew in the service: the team's engineering skill and
/// number of mechanics.
pub fn player_crew(season: &Season, roster: &Roster) -> Option<crate::service::Crew> {
    let team = team_of(season, roster, player_entry(season)?)?;
    Some(crate::service::Crew { a: team.engineering, b: team.mechanics })
}

/// The road's wear for a starter: the first three starters drive a new
/// road (0), places 4-11 a normal one (1), the rest a worn one (2); 1 when
/// the player is not in the start list.
pub fn road_wear(start_place: Option<usize>) -> u32 {
    match start_place {
        Some(p) if p <= 2 => 0,
        Some(p) => 1 + u32::from(p > 10),
        None => 1,
    }
}

/// The player's place in the stage's start list ([`stage_start_order`]).
pub fn player_start_place(season: &Season) -> Option<usize> {
    start_order(season).iter().position(|&k| entry_id(season, k as usize).is_some_and(|id| id == PLAYER_ID))
}

/// `ai.ini` and `aidrv.ini` from `Root/misc.rbz`.
pub fn load_ai(data: &rbr_formats::data::Data) -> Result<(Master, Roster), String> {
    use std::io::Read;
    let misc = data.resolve("Root/misc.rbz").ok_or("Root/misc.rbz: not found")?;
    let mut zip = rbr_formats::rbz::open(&misc).map_err(|e| format!("misc.rbz: {e}"))?;
    let mut text = |name: &str| -> Result<String, String> {
        let mut b = Vec::new();
        zip.by_name(name).and_then(|mut f| Ok(f.read_to_end(&mut b)?)).map_err(|e| format!("misc.rbz/{name}: {e}"))?;
        Ok(String::from_utf8_lossy(&b).into_owned())
    };
    let master = rbr_formats::ai_ini::parse_master(&text("Misc/ai/ai.ini")?).map_err(|e| format!("ai.ini: {e}"))?;
    let roster =
        rbr_formats::ai_ini::parse_roster(&text("Misc/ai/aidrv.ini")?).map_err(|e| format!("aidrv.ini: {e}"))?;
    Ok((master, roster))
}

/// The season's seed from a Unix time in milliseconds (ours: UTC; the original reads the local time).
pub fn seed_from_unix_ms(ms: u64) -> u32 {
    let secs = ms / 1000;
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    // Days to the civil date (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = (yoe + era * 400 + i64::from(month <= 2)) as u32;
    let weekday = ((days + 4).rem_euclid(7)) as u32;
    seed((rem % 60) as u32, (rem / 60 % 60) as u32, (ms % 1000) as u32, (rem / 3600) as u32, weekday, day, year)
}

/// Team `t`'s car in `aidrv.ini` (car 2 has no team). Its two drivers, ids
/// `2t` and `2t + 1`, drive the car's instances `2c` and `2c + 1`.
pub const TEAM_CARS: [u32; 7] = [0, 1, 3, 4, 5, 6, 7];

/// The AI side of the season: the drivers by id, two car instances per
/// `aidrv.ini` car, and the teams' repair spending. The season's stores keep
/// it in the save.
#[derive(Debug, Clone, PartialEq)]
pub struct AiField {
    pub drivers: Vec<DriverState>,
    pub cars: Vec<CarState>,
    pub teams: Vec<TeamCosts>,
}

impl AiField {
    pub fn new(roster: &Roster) -> AiField {
        AiField {
            drivers: roster.drivers.iter().cloned().map(DriverState::new).collect(),
            cars: roster.cars.iter().flat_map(|c| [CarState::new(c.clone()), CarState::new(c.clone())]).collect(),
            teams: roster.teams.iter().map(TeamCosts::new).collect(),
        }
    }

    /// The season's stores of this field: each AI entry's driver, each entry's car, the
    /// season's seven teams with their budget, manager style and spending, and their drivers (the player's id in the
    /// player's seat) and cars as the new-season setup gives them.
    pub fn stores(&self, season: &Season, roster: &Roster) -> Stores {
        let drivers = season
            .drivers
            .iter()
            .filter(|e| e.id >= 0)
            .map(|e| {
                self.drivers.get(e.id as usize).map_or(DriverStore::default(), |d| DriverStore {
                    injury: d.injury,
                    injury_len: d.injury_len,
                    co_injury: d.co_injury,
                    co_injury_len: d.co_injury_len,
                    temperament: d.driver.temperament as u32,
                })
            })
            .collect();
        let cars = season
            .drivers
            .iter()
            .map(|e| {
                self.cars.get(e.car as usize).map_or(CarStore::default(), |c| CarStore { parts: c.parts, hits: c.hits })
            })
            .collect();
        let teams = team_ids(season).map(|t| {
            let costs = self.teams.get(t as usize).copied().unwrap_or_default();
            let car = TEAM_CARS.get(t as usize).map_or(0, |c| c * 2);
            let id = |k: u32| season.drivers.get(k as usize).map_or(k, |e| e.id as u32);
            roster.teams.get(t as usize).map_or(TeamStore::default(), |_| TeamStore {
                budget: costs.budget,
                service_cost: costs.service,
                rally_cost: costs.rally,
                manager_style: costs.style,
                drivers: [id(t * 2), id(t * 2 + 1)],
                cars: [car, car + 1],
            })
        });
        Stores { drivers, cars, teams }
    }

    /// The field back from the season's stores.
    pub fn restore(&mut self, season: &Season, stores: &Stores) {
        for (e, w) in season.drivers.iter().filter(|e| e.id >= 0).zip(&stores.drivers) {
            if let Some(d) = self.drivers.get_mut(e.id as usize) {
                (d.injury, d.injury_len, d.co_injury, d.co_injury_len) =
                    (w.injury, w.injury_len, w.co_injury, w.co_injury_len);
                d.driver.temperament = w.temperament as i32;
            }
        }
        for (e, w) in season.drivers.iter().zip(&stores.cars) {
            if let Some(c) = self.cars.get_mut(e.car as usize) {
                (c.parts, c.hits) = (w.parts, w.hits);
            }
        }
        for (&t, w) in team_ids(season).iter().zip(&stores.teams) {
            if let Some(c) = self.teams.get_mut(t as usize) {
                *c = TeamCosts {
                    budget: w.budget,
                    service: w.service_cost,
                    rally: w.rally_cost,
                    style: w.manager_style,
                };
            }
        }
    }

    /// A rally starts: the teams' spending zeroed and each entry's car repaired.
    pub fn rally_start(&mut self, season: &Season) {
        self.teams.iter_mut().for_each(|t| (t.service, t.rally) = (0, 0));
        for e in &season.drivers {
            if let Some(c) = self.cars.get_mut(e.car as usize) {
                c.parts = [0; 8];
                c.hits = [0; 8];
            }
        }
    }
}

/// A rally's first start order: the season's points so far, most first; the
/// first rally, or while the rank-order flag is set, by the entries' ranks.
pub fn rally_start_order(season: &Season) -> [u32; DRIVERS] {
    let pos = position(season);
    if pos.rally == 0 || rank_order_override(season) {
        bubble(ENTRIES, |n, c| entry_rank(season, c) < entry_rank(season, n))
    } else {
        bubble(ENTRIES, |n, c| wrapping_season_points(season, c) < wrapping_season_points(season, n))
    }
}

/// A stage's start order: the rally's first order for the first stage, then
/// the rally's time plus penalties, highest first, retired entries last.
pub fn stage_start_order(season: &Season) -> [u32; DRIVERS] {
    let pos = position(season);
    if pos.stage == 0 {
        return first_stage_order(season);
    }
    bubble(ENTRIES, |n, c| start_key(season, n) > start_key(season, c))
}

/// One line of a rally's standings.
#[derive(Debug, Clone, PartialEq)]
pub struct RallyLine {
    pub entry: usize,
    pub name: String,
    /// The rally's time with its penalties, `None` when retired.
    pub total: Option<f32>,
    pub stages: Vec<Option<f32>>,
    pub points: u32,
}

/// The points sort's key: time plus penalty, -1 for a retired driver.
fn key(season: &Season, entry: usize, rally: usize) -> f32 {
    let retired = season.drivers.get(entry).and_then(|d| d.rallies.get(rally)).is_some_and(retired_in);
    if retired {
        ai_season::RETIRED
    } else {
        rally_penalty(season, entry, rally) + rally_total(season, entry, rally)
    }
}

/// Rally `rally`'s standings after `stages` stages: by time plus penalty,
/// retired drivers last.
pub fn rally_standings(season: &Season, rally: usize, stages: usize) -> Vec<RallyLine> {
    let mut lines: Vec<RallyLine> = season
        .drivers
        .iter()
        .enumerate()
        .map(|(k, _)| {
            let r = &season.drivers[k].rallies[rally];
            let done = &r.stages[..stages.min(6)];
            let retired = done.iter().any(stage_retired);
            RallyLine {
                entry: k,
                name: entry_name(season, k),
                total: (!retired).then(|| done.iter().map(|s| s.time).sum::<f32>() + rally_penalty(season, k, rally)),
                stages: done.iter().map(|s| (s.retired == 0 && s.time >= 0.0).then_some(s.time)).collect(),
                points: rally_points(season, k, rally),
            }
        })
        .collect();
    lines.sort_by(|a, b| match (a.total, b.total) {
        (Some(x), Some(y)) => x.total_cmp(&y),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    });
    lines
}

/// The rally's points into the drivers.
pub fn award_points(season: &mut Season, rally: usize) {
    let mut order: Vec<usize> = (0..season.drivers.len()).collect();
    loop {
        let mut sorted = true;
        for i in 0..order.len() - 1 {
            if key(season, order[i], rally) > key(season, order[i + 1], rally) {
                order.swap(i, i + 1);
                sorted = false;
            }
        }
        if sorted {
            break;
        }
    }
    let mut place = 0;
    for e in order {
        let retired = retired_in(&season.drivers[e].rallies[rally]);
        let no_time = rally_total(season, e, rally) == ai_season::RETIRED;
        if retired || no_time || entry_excluded(season, e) || place >= POINTS.len() {
            continue;
        }
        let pts = POINTS[place];
        add_rally_points(season, e, rally, pts);
        place += 1;
    }
}

/// The season's standings: entry, name, points; highest first.
pub fn season_standings(season: &Season) -> Vec<(usize, String, u32)> {
    let mut lines: Vec<(usize, String, u32)> =
        (0..season.drivers.len()).map(|k| (k, entry_name(season, k), season_points(season, k))).collect();
    lines.sort_by_key(|l| std::cmp::Reverse(l.2));
    lines
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::weather;
    use rbr_formats::saves::season::RETIRED;
    use rbr_formats::season_dat;

    pub(crate) fn table() -> weather::Season {
        let row =
            |map, letter, preset, wetness| weather::Row { map, letter, cloud: 1, cloud_letter: 2, preset, wetness };
        let mut rows = Vec::new();
        for map in [10, 11, 12, 13, 14, 15, 16] {
            rows.extend([
                row(map, 0, 0, 0),
                row(map, 1, 3, 1),
                row(map, 2, 4, 2),
                row(map, 0, 5, 0),
                row(map, 3, 8, 0),
            ]);
        }
        weather::Season { rows, nice: vec![], bad: vec![] }
    }

    pub(crate) fn rallies() -> Vec<season_dat::Rally> {
        let info = |map| season_dat::StageInfo { map, time_of_day: 0, tint_set: 0, hour: 10, minute: 30 };
        let files: [season_dat::StageInfo; 7] = std::array::from_fn(|k| info(10 + k as u32));
        let setup = season_dat::RallySetup {
            fog_chance: 20.0,
            rain_chance: 60.0,
            temperature: 15.0,
            max_rain: 40.0,
            mid_rain: 20.0,
        };
        (0..6).map(|k| season_dat::Rally { name: format!("R{k}"), setup, levels: vec![files; 3] }).collect()
    }

    #[test]
    fn the_road_wears_with_the_start_place() {
        assert_eq!([0, 2, 3, 10, 11, 13].map(|p| road_wear(Some(p))), [0, 0, 1, 1, 2, 2]);
        assert_eq!(road_wear(None), 1);
    }

    /// A new season with its first rally started, the player in `team`'s seat.
    pub(crate) fn start(
        roster: &Roster,
        blocks: &[Block],
        options: ChampionshipOptions,
        team: u32,
        rng: &mut CrtRand,
    ) -> (Season, AiField) {
        let classes = driver_classes(roster, &mut CrtRand(7));
        let mut s = create(roster, blocks, options, Player { name: "Player", team, rank: 0 }, &classes, rng);
        let mut field = AiField::new(roster);
        crate::season_events::rally_start(&mut s, &mut field);
        (s, field)
    }

    #[test]
    fn the_season_holds_the_crew_the_damage_level_and_the_start_order() {
        let team = |name: &str, mechanics, engineering| rbr_formats::ai_ini::Team {
            name: name.into(),
            budget: 3,
            manager_style: 1,
            mechanics,
            engineering,
            specialities: vec![],
            difficulties: vec![],
            history: 0,
            nationality: 0,
            manager: String::new(),
        };
        let teams = (0..7).map(|k| team("T", 4 + k, 1 + k % 5)).collect();
        let roster = Roster { drivers: vec![], teams, cars: vec![] };
        let picked = ChampionshipOptions { skill: Skill::Rookie, damage: DamageLevel::Reduced };
        let (mut s, _) = start(&roster, &[], picked, 6, &mut CrtRand(1));
        assert_eq!(options(&s).damage, DamageLevel::Reduced);
        // The player takes the seventh team's first seat, entry 12 (10 mechanics, engineering 2).
        assert_eq!(player_entry(&s), Some(12));
        assert_eq!(player_crew(&s, &roster), Some(crate::service::Crew { a: 2, b: 10 }));
        assert_eq!(s.drivers[12].car, TEAM_CARS[6] * 2);
        // The first rally's first stage starts in entry order (every +0x900 is 0): the player 13th, on a worn road.
        assert_eq!(s.w8e4c, ENTRIES);
        assert_eq!(player_start_place(&s), Some(12));
        // After a stage the rally's times decide, the slowest first (`M3800`): the player, 100 s behind, first.
        for (k, d) in s.drivers.iter_mut().enumerate() {
            d.rallies[0].f168 = if k == 12 { 300.0 } else { 200.0 + k as f32 };
        }
        // Entry 3 retired: last.
        s.drivers[3].rallies[0].stages[0].retired = 1;
        crate::season_events::fill_stage(&mut s);
        assert_eq!(s.position(), (0, 1));
        assert_eq!(player_start_place(&s), Some(0));
        assert_eq!(road_wear(player_start_place(&s)), 0);
        assert_eq!(s.w7ecc, [12, 13, 11, 10, 9, 8, 7, 6, 5, 4, 2, 1, 0, 3]);
        // The order is in the stage's slot too (+0x1c).
        let at = 4 + SLOT + 0x1c;
        assert_eq!(s.blocks[at..at + 4], 12u32.to_le_bytes());
    }

    /// `SetupAI`'s +0x900 by the driver class, in entry order, and the class from the skills' sum.
    #[test]
    fn the_ai_ranks_follow_the_driver_classes() {
        let driver = |v: i32| rbr_formats::ai_ini::Driver {
            name: format!("D{v}"),
            photo: 0,
            technique: v,
            temperament: 0,
            skills: [v; 4],
            speed: v,
            endurance: v,
            condition: v,
            history: 0,
            nationality: 0,
            codriver: String::new(),
        };
        // A sum of 32: class 2 whatever the roll; 0: class 0.
        let roster = Roster { drivers: vec![driver(4), driver(0)], teams: vec![], cars: vec![] };
        assert_eq!(driver_classes(&roster, &mut CrtRand(5)), [2, 0]);
        let mut rng = CrtRand(9);
        let s = create(
            &roster,
            &[],
            ChampionshipOptions::default(),
            Player { name: "P", team: 3, rank: 4 },
            &[2, 0],
            &mut rng,
        );
        let mut probe = CrtRand(9);
        assert_eq!(s.drivers[0].w900, (probe.draw() % 6) as u32 + 8);
        assert_eq!(s.drivers[1].w900, (probe.draw() % 6) as u32 + 2);
        assert_eq!(s.drivers[6].w900, 4);
        assert_eq!(rng, probe);
        assert_eq!((s.drivers[6].id, s.drivers[6].b8d0[0], s.drivers[6].name()), (PLAYER_ID, 1, "P".into()));
        assert_eq!(s.position(), (u32::MAX, 255));
    }

    #[test]
    fn the_seed_adds_the_time_fields() {
        // 2026-10-17 12:34:56.789 UTC, a Saturday (6).
        assert_eq!(seed_from_unix_ms(1_792_240_496_789), 56 + 34 + 789 + 12 + 6 + 17 + 2026);
    }

    #[test]
    fn a_new_season_round_trips_through_the_save() {
        let (t, r) = (table(), rallies());
        let mut rng = CrtRand(99);
        let (kinds, blocks) = new_blocks(&mut rng, &r, &t).unwrap();
        assert!(kinds.iter().all(|&k| (1..=3).contains(&k)));
        assert_eq!(blocks.iter().map(|b| b.rally).collect::<Vec<_>>(), RALLY_ORDER);
        let roster = Roster { drivers: vec![], teams: vec![], cars: vec![] };
        let options = ChampionshipOptions { skill: Skill::Professional, damage: DamageLevel::Realistic };
        let (s, _) = start(&roster, &blocks, options, 0, &mut rng);
        let back = super::blocks(&s, &t);
        assert_eq!(back.len(), 6);
        for (a, b) in back.iter().zip(&blocks) {
            assert_eq!((a.rally, a.trend), (b.rally, b.trend));
            for (x, y) in a.stages.iter().zip(&b.stages) {
                assert_eq!(
                    (x.map, x.letter, x.preset, x.wetness, x.temperature, x.hour, x.minute),
                    (y.map, y.letter, y.preset, y.wetness, y.temperature, y.hour, y.minute)
                );
            }
            assert_eq!(a.rally(&r).stages.len(), 6);
        }
        assert_eq!((s.drivers[0].id, s.drivers[0].name()), (PLAYER_ID, "Player".into()));
        assert_eq!(s.rally_level(), 1);
        // The clock and the stage's starts from the slots (10:30 in the test's `season.dat`).
        assert_eq!(s.events.iter().find(|e| e.kind == 3).map(|e| e.time), Some(10 << 8 | 30));
        assert_eq!(back[0].rally(&r).stages[0].hour, 10);
    }

    #[test]
    fn points_go_to_the_first_eight_finishers() {
        let roster = Roster { drivers: vec![], teams: vec![], cars: vec![] };
        let (mut s, _) = start(&roster, &[], ChampionshipOptions::default(), 6, &mut CrtRand(1));
        for (k, d) in s.drivers.iter_mut().enumerate() {
            d.rallies[0].f168 = 100.0 + k as f32;
        }
        s.drivers[0].rallies[0].stages[2].retired = 1;
        s.drivers[0].rallies[0].stages[2].time = RETIRED;
        s.drivers[3].rallies[0].f170 = 50.0;
        award_points(&mut s, 0);
        let pts: Vec<u32> = s.drivers.iter().map(|d| d.rallies[0].w16c).collect();
        assert_eq!(pts, [0, 10, 8, 0, 6, 5, 4, 3, 2, 1, 0, 0, 0, 0]);
        let table = season_standings(&s);
        assert_eq!(table[0], (1, String::new(), 10));
        let r = rally_standings(&s, 0, 6);
        assert_eq!(r.last().unwrap().entry, 0);
    }

    /// A whole retail rally through the queue (`RBR_DATA`, `RBR_PATCH`): the player drives each stage in 250 s;
    /// every AI entry gets a time or a retire per stage, the services post the managers' orders, the teams spend, and
    /// the next rally starts with the budgets scored; the field's stores give the same field back.
    #[test]
    fn retail_rally_through_the_queue() {
        use crate::season_events::{self as ev, Ai, Turn};
        let Some(data) = rbr_formats::data::Data::from_env() else { return };
        if data.patch().is_none() {
            return;
        }
        let (master, roster, rallies, table) = crate::season::tests::retail(&data);
        let mut rng = CrtRand(2024);
        let (_, blocks) = new_blocks(&mut rng, &rallies, &table).unwrap();
        let options = ChampionshipOptions { skill: Skill::Champion, damage: DamageLevel::Realistic };
        let (mut s, mut field) = start(&roster, &blocks, options, 1, &mut rng);
        let budgets: Vec<u32> = field.teams.iter().map(|t| t.budget).collect();
        let mut news = Vec::new();
        let mut ai = Ai { master: &master, roster: &roster, field: &mut field, blocks: &blocks, rng: &mut rng };
        let mut turn = ev::run_to_turn(&mut s, &mut ai, &mut news);
        let mut stores = None;
        while turn != Turn::Welcome || s.position().0 == 0 {
            match turn {
                Turn::Stage(stage) => {
                    assert_eq!(s.position(), (0, stage));
                    s.set_stage_time(PLAYER_ID, 250.0);
                }
                Turn::Results => {
                    for d in s.drivers.iter().filter(|d| d.id >= 0) {
                        let r = &d.rallies[0];
                        assert!(r.stages.iter().all(|st| st.time > 0.0 || st.retired == 1), "{}", d.name());
                        assert!(r.stages.iter().all(|st| st.f34 > 0.0 || st.retired == 1), "{}", d.name());
                    }
                    stores = Some(ai.field.stores(&s, &roster));
                }
                _ => {}
            }
            turn = ev::turn_done(&mut s, &mut ai, &mut news);
        }
        assert_eq!(s.position(), (1, 0));
        use crate::ai_season::Bulletin;
        assert!(news.iter().any(|n| matches!(n.bulletin, Bulletin::ManagerOrder { .. })));
        assert!(news.iter().any(|n| matches!(n.bulletin, Bulletin::Accident { .. })));
        // Accidents leave pairs in the results.
        assert!(s.drivers.iter().any(|d| d.rallies[0].stages.iter().any(|st| st.b30[0] > 0)));
        assert_eq!(s.drivers[2].id, PLAYER_ID);
        assert_eq!(s.drivers[2].rallies[0].f168, 1500.0);
        // The rally's spending was zeroed for the next, the budgets scored.
        assert!(ai.field.teams.iter().all(|t| t.rally == 0));
        assert_ne!(ai.field.teams.iter().map(|t| t.budget).collect::<Vec<_>>(), budgets);
        let stores = stores.unwrap();
        let mut back = AiField::new(&roster);
        back.restore(&s, &stores);
        assert_eq!(back.stores(&s, &roster).drivers, stores.drivers);
        assert_eq!(stores.teams[1].drivers, [PLAYER_ID as u32, 3]);
    }

    pub(crate) fn retail(data: &rbr_formats::data::Data) -> (Master, Roster, Vec<season_dat::Rally>, weather::Season) {
        let (master, roster) = load_ai(data).unwrap();
        let rallies = season_dat::parse(&std::fs::read(data.resolve("Root/season.dat").unwrap()).unwrap()).unwrap();
        (master, roster, rallies, weather::Season::load(data).unwrap())
    }
}
