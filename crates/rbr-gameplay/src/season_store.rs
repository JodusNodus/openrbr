//! The championship saved in the game's own profile file (`pf<name>.rbr`):
//! the season is the profile's `RBR7` block, written after a new season and
//! after each of the player's turns. A finished stage stores its time and
//! sectors, then marks later stages when the car cannot continue; a retire
//! marks this stage on. The service penalty is added as the park closes.
//! What follows the season in the file is the AI field's stores, the setup
//! slot, the player's carried damage and the car setup, as the game writes.

use rbr_formats::saves::after_season::AfterSeason;
use rbr_formats::saves::profile::{ProfileFile, SeasonSave};
use rbr_formats::saves::season::Season;
use rbr_formats::saves::SaveDir;

use crate::championship::StageTimes;
use crate::season::{self, add_rally_penalty, false_starts, inc_false_starts, position, rally_penalty, PLAYER_ID};

/// The rally's last stage index, after which a car that cannot drive does
/// not retire the rally.
const LAST_STAGE: u32 = 5;

/// The player's finished (`Some`) or retired (`None`) stage at the season's
/// position. A finish gives the time and the three sectors; then, when the
/// car cannot drive before the rally's last stage, the stages after it are
/// marked not driven. A retire marks this stage on, leaving the rally total.
pub fn record_stage(s: &mut Season, times: Option<StageTimes>, drivable: bool) {
    let pos = position(s);
    let entry = season::player_entry(s);
    match times {
        Some(t) => {
            // The commit is explicit: the time and sectors go in first, then
            // the later stages are marked when the car cannot continue.
            s.set_stage_time(PLAYER_ID, t.time);
            for (k, &v) in t.sectors.iter().enumerate() {
                s.set_sector(PLAYER_ID, k as u8, v);
            }
            if let Some(entry) = entry.filter(|_| !drivable && pos.stage != LAST_STAGE) {
                season::fill_neg(s, entry, pos.stage as usize + 1);
            }
        }
        None => {
            if let Some(entry) = entry {
                season::fill_neg(s, entry, pos.stage as usize);
            }
        }
    }
}

/// The service's late penalty as the park closes: added to the player's
/// rally at once, so the save after the service holds it.
pub fn record_penalty(s: &mut Season, penalty: f32) {
    let pos = position(s);
    if let Some(entry) = season::player_entry(s) {
        add_rally_penalty(s, entry, pos.rally as usize, penalty);
    }
}

/// The player's late penalties so far in the season's rally.
pub fn player_penalty(s: &Season) -> f32 {
    let pos = position(s);
    season::player_entry(s).map_or(0.0, |e| rally_penalty(s, e, pos.rally as usize))
}

/// The player's false starts so far in the season's rally, which the stage's
/// jump-start penalty takes.
pub fn player_false_starts(s: &Season) -> u8 {
    let pos = position(s);
    season::player_entry(s).map_or(0, |e| false_starts(s, e, pos.rally as usize))
}

/// The player jumped the start: count one more false start in the rally.
pub fn record_false_start(s: &mut Season) {
    let pos = position(s);
    if let Some(entry) = season::player_entry(s) {
        inc_false_starts(s, entry, pos.rally as usize);
    }
}

/// What the profile stores behind the season: the field's stores, the
/// player's carried damage and tyre, and the car setup's text. `tyre` is
/// the compound the last stage ran on. There is no tyre stock. The setup
/// slot stays 0 and the damage block carries the automatic-gears flag.
pub fn after_season(
    season: &Season,
    field: &season::AiField,
    roster: &rbr_formats::ai_ini::Roster,
    damage: &crate::car_damage::CarriedDamage,
    tyre: u32,
    auto_gears: bool,
    setup: &[u8],
) -> AfterSeason {
    use rbr_formats::saves::after_season::DamageBlock;
    AfterSeason {
        stores: field.stores(season, roster),
        setup_slot: 0,
        damage: DamageBlock {
            handler: damage.to_binary(),
            b17838: tyre,
            tyre,
            w1783c: 0,
            w1660814: u32::from(auto_gears),
        },
        // `E16_BuildSetup` wraps the tree's text, `("CarSetup" ...)`, in parentheses: a setup file's whole text.
        setup: rbr_formats::saves::setup_slots::to_bytes(&String::from_utf8_lossy(setup)),
    }
}

/// A profile's season in `SavedGames`.
pub struct ProfileSeason {
    pub saves: SaveDir,
    pub profile: String,
}

/// The end tag that closes a season this crate wrote (`PFOE`).
const END: [u8; 4] = *b"PFOE";

impl ProfileSeason {
    /// The profile's file, or a fresh one when there is none.
    fn file(&self) -> Result<ProfileFile, String> {
        match self.saves.load_profile(&self.profile) {
            Ok(f) => Ok(f),
            Err(rbr_formats::saves::Error::Io { kind: std::io::ErrorKind::NotFound, .. }) => {
                Ok(ProfileFile::fresh(&self.profile))
            }
            Err(e) => Err(format!("{}: {e}", self.profile)),
        }
    }

    /// The season under way, if any, with what follows it when it parses.
    pub fn load(&self) -> Result<Option<(Season, Option<AfterSeason>)>, String> {
        Ok(self.file()?.season.map(|s| {
            let after = AfterSeason::parse(&s.season, &s.rest).ok();
            (s.season, after)
        }))
    }

    /// Writes `season` into the profile with `after` behind it (without, what followed it stays).
    pub fn save(&self, season: &Season, after: Option<&AfterSeason>) -> Result<(), String> {
        let mut file = self.file()?;
        let old = file.season.take().map_or_else(|| END.to_vec(), |s| s.rest);
        let rest = after.map_or(old, AfterSeason::write);
        file.mode = 3;
        file.season = Some(SeasonSave { season: season.clone(), rest });
        self.saves.save_profile(&self.profile, &file).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::race_options::{ChampionshipOptions, DamageLevel, Skill};
    use rbr_formats::ai_ini::Roster;
    use rbr_formats::saves::season::RETIRED;

    fn new_season(roster: &Roster, options: ChampionshipOptions) -> Season {
        let player = season::Player { name: "Tester", team: 6, rank: 0 };
        let mut s = season::create(roster, &[], options, player, &[], &mut crate::crt::CrtRand(1));
        crate::season_events::rally_start(&mut s, &mut season::AiField::new(roster));
        s
    }

    #[test]
    fn a_season_is_written_and_read_back() {
        let dir = std::env::temp_dir().join(format!("rbr-season-store-{}", std::process::id()));
        let store = ProfileSeason { saves: SaveDir { path: dir.clone() }, profile: "tester".into() };
        assert_eq!(store.load().unwrap(), None);
        let roster = Roster { drivers: vec![], teams: vec![], cars: vec![] };
        let mut s = new_season(&roster, ChampionshipOptions { skill: Skill::Champion, damage: DamageLevel::Realistic });
        assert!(!s.events.is_empty());
        store.save(&s, None).unwrap();
        assert_eq!(store.load().unwrap(), Some((s.clone(), None)));
        // With the stores, the damage and the setup behind it, the season reads back whole.
        let mut damage = crate::car_damage::CarriedDamage::default();
        damage.set(0x3A90, 1);
        let field = season::AiField::new(&roster);
        let after = after_season(&s, &field, &roster, &damage, 4, false, b"((setup))");
        store.save(&s, Some(&after)).unwrap();
        let (_, back) = store.load().unwrap().unwrap();
        let back = back.unwrap();
        assert_eq!(back, after);
        assert_eq!(crate::car_damage::CarriedDamage::from_binary(&back.damage.handler), Some(damage));
        record_stage(&mut s, Some(StageTimes { time: 300.5, sectors: [100.0, 150.0, 50.5] }), true);
        record_penalty(&mut s, 10.0);
        assert_eq!(player_penalty(&s), 10.0);
        record_false_start(&mut s);
        assert_eq!(player_false_starts(&s), 1);
        store.save(&s, None).unwrap();
        let (back, _) = store.load().unwrap().unwrap();
        let r = &back.drivers[12].rallies[0];
        assert_eq!((r.stages[0].time, r.stages[0].sectors, r.f168, r.f170), (300.5, [100.0, 150.0, 50.5], 300.5, 10.0));
        s.state[2] = 1;
        record_stage(&mut s, None, true);
        store.save(&s, None).unwrap();
        let (back, _) = store.load().unwrap().unwrap();
        let r = &back.drivers[12].rallies[0];
        // `FillNeg` leaves the driven stage and the rally's time.
        assert_eq!((r.stages[0].retired, r.stages[0].time, r.f168), (0, 300.5, 300.5));
        assert_eq!((r.stages[1].retired, r.stages[1].time, r.stages[5].retired), (1, RETIRED, 1));
        let _ = std::fs::remove_dir_all(dir);
    }

    /// A finish in a car that can't drive retires the later stages (`M005d0960`) but keeps the stage's time; on the
    /// rally's last stage it retires nothing.
    #[test]
    fn a_wrecked_finish_retires_the_stages_after_it() {
        let roster = Roster { drivers: vec![], teams: vec![], cars: vec![] };
        let mut s = new_season(&roster, ChampionshipOptions::default());
        let times = StageTimes { time: 200.0, sectors: [50.0, 100.0, 50.0] };
        s.state[2] = 2;
        record_stage(&mut s, Some(times), false);
        let r = &s.drivers[season::player_entry(&s).unwrap()].rallies[0];
        assert_eq!((r.stages[2].time, r.stages[2].retired, r.f168), (200.0, 0, 200.0));
        assert!(r.stages[3..].iter().all(|st| st.retired == 1 && st.time == RETIRED));
        assert!(r.stages[..2].iter().all(|st| st.retired == 0));
        let mut s = new_season(&roster, ChampionshipOptions::default());
        s.state[2] = 5;
        record_stage(&mut s, Some(times), false);
        let r = &s.drivers[season::player_entry(&s).unwrap()].rallies[0];
        assert_eq!((r.stages[5].time, r.stages[5].retired), (200.0, 0));
        assert!(r.stages.iter().all(|st| st.retired == 0));
    }

    /// A profile of `RBR_SAVES` the original 1.02 wrote that holds no season, copied into a new folder `dir`, as a
    /// store; `None` (and a skip message) when there is none.
    fn original_profile(dir: &str) -> Option<ProfileSeason> {
        let Some(saves) = std::env::var_os("RBR_SAVES").map(|path| SaveDir { path: path.into() }) else {
            eprintln!("skipping: RBR_SAVES not set");
            return None;
        };
        let names = saves.profiles().unwrap();
        let Some(name) = names.into_iter().find(|n| saves.load_profile(n).is_ok_and(|f| f.season.is_none())) else {
            eprintln!("skipping: no profile without a season in RBR_SAVES");
            return None;
        };
        let dir = std::env::temp_dir().join(format!("{dir}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = rbr_formats::saves::Kind::Profile.file_name(&name);
        std::fs::copy(saves.path.join(&file), dir.join(&file)).unwrap();
        Some(ProfileSeason { saves: SaveDir { path: dir }, profile: name })
    }

    /// The championship's options written into an original 1.02 profile (`RBR_SAVES`, one that holds no season) and
    /// read back: every option, and the rest of the profile unchanged.
    #[test]
    fn the_options_round_trip_in_the_original_profile() {
        let Some(store) = original_profile("rbr-season-options") else {
            return;
        };
        let dir = store.saves.path.clone();
        let original = store.saves.load_profile(&store.profile).unwrap();
        assert_eq!(store.load().unwrap(), None);
        let roster = Roster { drivers: vec![], teams: vec![], cars: vec![] };
        for skill in Skill::ALL {
            for damage in DamageLevel::ALL {
                let options = ChampionshipOptions { skill, damage };
                let s = new_season(&roster, options);
                store.save(&s, None).unwrap();
                let (back, _) = store.load().unwrap().unwrap();
                assert_eq!(season::options(&back), options);
                assert_eq!(
                    (back.state[0], back.globals[7], back.snapshot[7]),
                    (skill.word(), skill.word(), skill.word())
                );
                assert_eq!(back.snapshot[12], damage.word());
                let file = store.saves.load_profile(&store.profile).unwrap();
                assert_eq!(ProfileFile { mode: original.mode, season: None, ..file }, original);
            }
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    /// A season played on the retail data (a rally through the AI's stage loop, the player's carried damage, a tyre
    /// and a setup) saved into the original 1.02 profile (`RBR_SAVES`): the file reads back to the same season, AI
    /// stores and damage, and read, written and read again it is byte for byte the same file.
    #[test]
    fn a_played_season_round_trips_byte_for_byte_in_the_original_profile() {
        let Some(data) = rbr_formats::data::Data::from_env() else {
            eprintln!("skipping: RBR_DATA not set");
            return;
        };
        if data.patch().is_none() {
            return;
        }
        use crate::season_events::{self as ev, Ai, Turn};
        let (master, roster, rallies, table) = crate::season::tests::retail(&data);
        let mut rng = crate::crt::CrtRand(77);
        let (_, blocks) = season::new_blocks(&mut rng, &rallies, &table).unwrap();
        let options = ChampionshipOptions { skill: Skill::Professional, damage: DamageLevel::Reduced };
        let (mut s, mut field) = crate::season::tests::start(&roster, &blocks, options, 4, &mut rng);
        let mut ai = Ai { master: &master, roster: &roster, field: &mut field, blocks: &blocks, rng: &mut rng };
        let mut news = Vec::new();
        // Into the rally, to the player's fourth stage: the queue holds the rest of the rally.
        let mut turn = ev::run_to_turn(&mut s, &mut ai, &mut news);
        while turn != Turn::Stage(3) {
            if let Turn::Stage(_) = turn {
                record_stage(&mut s, Some(StageTimes { time: 250.5, sectors: [80.0, 90.0, 80.5] }), true);
                record_penalty(&mut s, 10.0);
            }
            turn = ev::turn_done(&mut s, &mut ai, &mut news);
        }
        assert!(!s.events.is_empty() && !s.events_b.is_empty());
        let mut damage = crate::car_damage::CarriedDamage::default();
        damage.set(0x3A90, 2);
        let after = after_season(&s, &field, &roster, &damage, 4, true, b"(\"CarSetup\")");

        let Some(store) = original_profile("rbr-season-played") else {
            return;
        };
        let dir = store.saves.path.clone();
        store.save(&s, Some(&after)).unwrap();
        let (back, back_after) = store.load().unwrap().unwrap();
        assert_eq!(back, s);
        let back_after = back_after.unwrap();
        assert_eq!(back_after, after);
        assert_eq!(crate::car_damage::CarriedDamage::from_binary(&back_after.damage.handler), Some(damage));
        let mut again = season::AiField::new(&roster);
        again.restore(&back, &back_after.stores);
        assert_eq!(again.stores(&back, &roster), after.stores);

        let path = dir.join(rbr_formats::saves::Kind::Profile.file_name(&store.profile));
        let bytes = std::fs::read(&path).unwrap();
        let file = ProfileFile::read(&bytes).unwrap();
        assert_eq!(file.write().unwrap(), bytes);
        let _ = std::fs::remove_dir_all(dir);
    }
}
