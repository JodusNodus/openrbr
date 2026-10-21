//! The Championship page: a rally of the season (`season.dat`) driven stage by stage (`rbr_gameplay::championship`),
//! with the service park between stages (`rbr_gameplay::service`): the damaged parts to order, the time left, Repair!
//! and the late penalty, which goes into the season as the park closes. After each service and the rally's start
//! (the game's page 0x1a) the setup is picked (the surface's default, the current one or a save slot:
//! `CSeasonChooseSetupUI`, `rbr_gameplay::setup::SeasonSetup`) and the tyre from the stage surface's tyres
//! (`SELECTTYRES`, `championship::tyre_choices`); a stage with no service before it runs on the car's tyre and setup,
//! with the tyres' wear carried (`championship::choice_before`). The park reads the car's `repair.ini` from NGP's
//! `physics.rbz` (an RSF car's: slot 5's, where it runs). The road wear is the player's start place's (`season::road_wear`).
//!
//! The championship's options (`race_options::ChampionshipOptions`) are chosen before New season, as the game's
//! `SSKILL` and `CARDAM` pages ask them: the skill level (Rookie, Professional, Champion: the outside-assistance
//! penalty of 10, 20 or 35 s, and at Champion a stuck car is not helped) and the car damage. The season save keeps
//! them; each stage runs as the game's championship (`rbr-standalone --championship`, flow mode 3) on the season's.
//!
//! A stage runs as on the Drive page, on the stage's season-table row, with the damage the car carries
//! (`rbr-standalone --damage-in`); when the viewer closes, `rbr-standalone` leaves the stage's result and damage in
//! `rally/` in the player's folder (`--result-out`, `--damage-out`). The service's crew is the player's team's, its
//! rules NGP's (`service::Rules`); its texts are the game's English string table.
//!
//! The season (`rbr_gameplay::season`): New season draws the rallies' levels and forecast weather on the clock's
//! seed, with the player in the team picked from the game's team offers (`TEAM_OFFERS`, in its order; the team's car
//! is the season's, or when NGP doesn't drive it a car picked with it, ours, kept out of the save). The season's event
//! queue (`rbr_gameplay::season_events`) runs it: the AI drivers' stages and
//! services between the player's turns, as the game's menu flow stops at them: the rally's welcome, each stage (at
//! its start time on the season's clock), each service, the rally's results and the page between rallies with the
//! game's two trials (`S_YARD_TRIAL`, `S_STAGE_TRIAL`: practice drives that the season does not record). After each
//! turn the season (the player's and the AI's times, the points, the queue) is written to the profile's `RBR7` block
//! in `SavedGames/pf<profile>.rbr` in the player's folder; Continue reads it back. The page shows the rally's standings,
//! the season's points and the bulletins since the player's last turn. The save also holds the AI drivers', cars' and
//! teams' state, the player's carried damage and tyre and the setup (`rbr_formats::saves::after_season`), so Continue
//! picks them up. Ours: every team is offered (the game locks those whose car the profile has not unlocked; the
//! launcher has no unlocks), the trials run on their map's first weather row.

use std::path::{Path, PathBuf};

use egui::{Color32, ComboBox, Ui};
use rbr_formats::ai_ini::{Master, Roster};
use rbr_formats::data::Data;
use rbr_formats::saves::season::Season as SeasonSave;
use rbr_formats::saves::SaveDir;
use rbr_formats::season_dat;
use rbr_formats::strings::Strings;
use rbr_gameplay::car_damage::CarriedDamage;
use rbr_gameplay::championship::StageTimes;
use rbr_gameplay::championship::{choice_before, tyre_choices, Championship, RallyOutcome, RallyPhase, RallyStage};
use rbr_gameplay::crt::CrtRand;
use rbr_gameplay::flow::{EndCause, EndRace, StageEnd};
use rbr_gameplay::race_options::{ChampionshipOptions, DamageLevel, Skill};
use rbr_gameplay::season::{self as season_model, AiField, Block};
use rbr_gameplay::season_events::{self as events, Ai, Turn};
use rbr_gameplay::season_store::{self, ProfileSeason};
use rbr_gameplay::service::{self, ParkState, Rules, ServicePark};
use rbr_gameplay::setup::SeasonSetup;
use rbr_gameplay::weather::Season;

use crate::pages::WARN;

/// A stage to drive: the extra `rbr-standalone` arguments (the season's options and the rally's files), the tyre and
/// the road wear.
pub struct StageStart {
    pub stage: RallyStage,
    pub args: Vec<String>,
    pub tyre: u32,
    pub wear: u32,
    /// The setup row picked for the stage.
    pub setup: SeasonSetup,
}

/// What the page asks of the launcher.
pub enum Request {
    Launch(StageStart),
    /// Open the Setup page.
    Setup,
}

/// The season under way.
pub struct SeasonRun {
    pub save: SeasonSave,
    pub blocks: Vec<Block>,
    field: AiField,
    rng: CrtRand,
    /// Where the season's queue waits for the player.
    pub turn: Turn,
    /// The bulletins since the player's last turn, as text.
    pub news: Vec<String>,
    /// None: not saved (the screenshots).
    store: Option<ProfileSeason>,
}

pub struct RallyPage {
    rallies: Result<Vec<season_dat::Rally>, String>,
    ai: Result<(Master, Roster), String>,
    /// The options a new season takes (`SSKILL`, `CARDAM`).
    pub options: ChampionshipOptions,
    /// The tyre picked for the next stage (`SELECTTYRES`), and the one the car has.
    tyre: Option<u32>,
    last_tyre: u32,
    /// The setup row picked for the next stage (`CSeasonChooseSetupUI`).
    setup: SeasonSetup,
    /// The car setup's text, kept in the save (`PTSC`).
    pub setup_text: Vec<u8>,
    /// The setup a continued season's save holds, for the Setup page to take ([`RallyPage::take_saved_setup`]).
    saved_setup: Option<String>,
    /// The damage a continued season carries into its rally.
    loaded_damage: Option<CarriedDamage>,
    pub profile: String,
    pub run: Option<SeasonRun>,
    pub championship: Option<Championship>,
    pub park: Option<ServicePark>,
    /// `MSG_NOREP` is up: "Repair!" without orders.
    forfeit_asked: bool,
    /// Message 0x31 (`MSG_CRECCE`) is up: the rally's start with the recce day next ([`events::recce_next`]).
    recce_asked: bool,
    /// The English string table (`bstrings.eng`), for the park's texts.
    strings: Option<Strings>,
    rand: CrtRand,
    /// A stage is being driven.
    driving: bool,
    /// The stage being driven is a trial (not recorded).
    practice: bool,
    /// The team a new season takes (`TEAM_OFFERS`).
    pub team: u32,
    /// A new season was made: the launcher gives it its car ([`RallyPage::take_new_season`]).
    new_season: bool,
    pub status: String,
}

fn clock(seconds: f32) -> String {
    let s = seconds.max(0.0);
    format!("{}:{:05.2}", (s / 60.0) as u32, s % 60.0)
}

/// `rbr-standalone --result-out`'s file: the race's end, and whether the car can still drive.
fn read_result(path: &Path) -> Result<(StageEnd, bool, bool), String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let value = |key: &str| text.lines().find_map(|l| l.strip_prefix(key)?.strip_prefix('='));
    let get = |key: &str| -> f32 { value(key).and_then(|v| v.parse().ok()).unwrap_or(0.0) };
    let cause = match value("cause") {
        Some("Finish") => EndCause::Finish,
        Some("Marker") => EndCause::Marker,
        Some("TimeLimit") => EndCause::TimeLimit,
        Some("Bystander") => EndCause::Bystander,
        Some("Offstage") => EndCause::Offstage,
        Some("Excluded") => EndCause::Excluded,
        Some("Retired") => EndCause::Retired,
        Some("FinishZone") => EndCause::FinishZone,
        _ => EndCause::Left,
    };
    let end_race =
        EndRace { time: get("time"), splits: [get("split1"), get("split2")], retired: get("finished") == 0.0 };
    Ok((StageEnd { cause, end_race }, get("drivable") != 0.0, get("false_start") != 0.0))
}

impl RallyPage {
    pub fn new(data: &Data) -> RallyPage {
        let rallies = data
            .resolve("Root/season.dat")
            .ok_or("Root/season.dat: not found".to_owned())
            .and_then(|p| std::fs::read(&p).map_err(|e| format!("{}: {e}", p.display())))
            .and_then(|bytes| season_dat::parse(&bytes));
        RallyPage {
            rallies,
            ai: season_model::load_ai(data),
            options: ChampionshipOptions::default(),
            tyre: None,
            last_tyre: 4,
            setup: SeasonSetup::Current,
            setup_text: Vec::new(),
            saved_setup: None,
            loaded_damage: None,
            profile: "player".into(),
            run: None,
            championship: None,
            park: None,
            forfeit_asked: false,
            recce_asked: false,
            strings: rbr_formats::strings::english(data).ok(),
            rand: CrtRand(1),
            driving: false,
            practice: false,
            team: season_model::TEAM_OFFERS[0],
            new_season: false,
            status: String::new(),
        }
    }

    /// The team of the season under way.
    pub fn season_team(&self) -> Option<u32> {
        let run = self.run.as_ref()?;
        Some(run.save.drivers[season_model::player_entry(&run.save)?].team)
    }

    /// The car of the season under way (`0x1660808`, the team's: `SetupAI`'s table), as the game's car index.
    #[cfg(test)]
    pub fn season_car(&self) -> Option<u32> {
        season_model::TEAM_CARS.get(self.season_team()? as usize).copied()
    }

    /// The rally is at a service stop (the service park's sound plays).
    pub fn in_service(&self) -> bool {
        self.championship.as_ref().is_some_and(|c| matches!(c.phase(), RallyPhase::Service(_)))
    }

    pub(crate) fn store(&self, here: &Path) -> ProfileSeason {
        ProfileSeason { saves: SaveDir::new(here), profile: self.profile.clone() }
    }

    /// A new season on the clock's seed (`store`: where it is saved), with the player's rank from the profile, to the
    /// player's first turn.
    pub fn new_season(&mut self, table: &Season, store: Option<ProfileSeason>, seed: u32) {
        let (Ok(rallies), Ok((master, roster))) = (&self.rallies, &self.ai) else { return };
        let mut rng = CrtRand(seed);
        match season_model::new_blocks(&mut rng, rallies, table) {
            Ok((_, blocks)) => {
                let rank = store
                    .as_ref()
                    .and_then(|s| s.saves.load_profile(&self.profile).ok())
                    .and_then(|f| f.profiles.list.first().map(season_model::player_rank))
                    .unwrap_or(0);
                // Ours: the driver classes' draws on their own stream (the game draws them at its start).
                let classes = season_model::driver_classes(roster, &mut CrtRand(seed.rotate_left(16)));
                let player = season_model::Player { name: &self.profile, team: self.team, rank };
                let mut save = season_model::create(roster, &blocks, self.options, player, &classes, &mut rng);
                let mut field = AiField::new(roster);
                events::rally_start(&mut save, &mut field);
                let mut news = Vec::new();
                let mut ai = Ai { master, roster, field: &mut field, blocks: &blocks, rng: &mut rng };
                let turn = events::run_to_turn(&mut save, &mut ai, &mut news);
                self.run = Some(SeasonRun { save, blocks, field, rng, turn, news: Vec::new(), store });
                self.new_season = true;
                self.save();
                self.open_rally();
            }
            Err(e) => self.status = format!("new season: {e}"),
        }
    }

    /// Whether a new season was made since the last call.
    pub fn take_new_season(&mut self) -> bool {
        std::mem::take(&mut self.new_season)
    }

    /// The setup of a season just continued (`PTSC`), once.
    pub fn take_saved_setup(&mut self) -> Option<String> {
        self.saved_setup.take()
    }

    /// The profile's season, if it has one.
    fn continue_season(&mut self, here: &Path, table: &Season) {
        let Ok((master, roster)) = &self.ai else { return };
        let store = self.store(here);
        match store.load() {
            Ok(Some((mut save, after))) => {
                let blocks = season_model::blocks(&save, table);
                let mut field = AiField::new(roster);
                if let Some(after) = &after {
                    field.restore(&save, &after.stores);
                    self.loaded_damage = CarriedDamage::from_binary(&after.damage.handler);
                    self.last_tyre = after.damage.tyre;
                    let text = rbr_formats::saves::setup_slots::text(&after.setup);
                    self.saved_setup = Some(text).filter(|t| !t.trim().is_empty());
                }
                // Ours: the save does not hold the `rand()` stream; it goes on from the clock's seed.
                let mut rng = CrtRand(season_model::seed_from_unix_ms(now_ms()));
                events::ensure_queue(&mut save);
                let mut news = Vec::new();
                let mut ai = Ai { master, roster, field: &mut field, blocks: &blocks, rng: &mut rng };
                let turn = events::run_to_turn(&mut save, &mut ai, &mut news);
                self.run = Some(SeasonRun { save, blocks, field, rng, turn, news: Vec::new(), store: Some(store) });
                self.open_rally();
            }
            Ok(None) => self.status = format!("{}: no season under way", self.profile),
            Err(e) => self.status = e,
        }
    }

    /// The championship of the season's current rally, at the player's turn.
    fn open_rally(&mut self) {
        let (Ok(rallies), Some(run)) = (&self.rallies, self.run.as_mut()) else { return };
        let (rally, stage) = run.save.position();
        let Some(block) = run.blocks.get(rally as usize) else { return };
        let r = block.rally(rallies);
        let p = player_rally(&run.save, rally as usize);
        let times = |n: usize| {
            p.map_or(Vec::new(), |p| {
                p.stages[..n.min(6)].iter().map(|s| StageTimes { time: s.time, sectors: s.sectors }).collect()
            })
        };
        let stage = stage as usize;
        let damage = self.loaded_damage.take();
        let penalty = season_store::player_penalty(&run.save);
        self.championship = Some(match run.turn {
            Turn::Welcome => Championship::start(r),
            Turn::Stage(s) => {
                Championship::resume(r, RallyPhase::Stage(s as usize), times(s as usize), penalty, damage)
            }
            Turn::Service { .. } => {
                let kind = service::service_before(stage).unwrap_or(service::ServiceKind::Short);
                let phase = RallyPhase::Service(rbr_gameplay::championship::Service { next: stage, kind });
                Championship::resume(r, phase, times(stage), penalty, damage)
            }
            Turn::Results | Turn::BetweenRallies | Turn::Over => {
                let retired = p.and_then(|p| p.stages.iter().position(|s| s.retired != 0));
                let outcome = retired.map_or(RallyOutcome::Finished, |stage| RallyOutcome::Retired { stage });
                let done = retired.unwrap_or(6);
                Championship::resume(r, RallyPhase::Over(outcome), times(done), penalty, damage)
            }
        });
        self.park = None;
        self.status.clear();
    }

    fn save(&mut self) {
        let (Some(run), Ok((_, roster))) = (&self.run, &self.ai) else { return };
        if let Some(store) = &run.store {
            let damage = self.championship.as_ref().and_then(Championship::damage).cloned().unwrap_or_default();
            let after = season_store::after_season(
                &run.save,
                &run.field,
                roster,
                &damage,
                self.last_tyre,
                false,
                &self.setup_text,
            );
            if let Err(e) = store.save(&run.save, Some(&after)) {
                self.status = format!("save: {e}");
            }
        }
    }

    /// The player's turn is done: the queue runs to the next one (the AI's stages and services on the way, a new
    /// rally at the end of one), and the season is saved. While the player's rally is over (retired, or a car that
    /// can't go on), the player's stages are retired and the services passed.
    fn turn_done(&mut self) {
        let (Ok((master, roster)), Some(run)) = (&self.ai, self.run.as_mut()) else { return };
        let rally = run.save.position().0;
        let mut news = Vec::new();
        let mut ai = Ai { master, roster, field: &mut run.field, blocks: &run.blocks, rng: &mut run.rng };
        run.turn = events::turn_done(&mut run.save, &mut ai, &mut news);
        let over = self.championship.as_ref().is_some_and(|c| matches!(c.phase(), RallyPhase::Over(_)));
        while over && matches!(run.turn, Turn::Stage(_) | Turn::Service { .. }) {
            if let Turn::Stage(_) = run.turn {
                season_store::record_stage(&mut run.save, None, true);
            }
            run.turn = events::turn_done(&mut run.save, &mut ai, &mut news);
        }
        let name = |id: i32| roster.drivers.get(id as usize).map_or("?".to_owned(), |d| d.name.clone());
        run.news = news
            .iter()
            .map(|n| match n.event.kind {
                events::kind::STAGE | events::kind::STAGE_END => {
                    format!("SS{}: {}", n.event.stage + 1, n.bulletin.text(&name))
                }
                _ => format!("Service: {}", n.bulletin.text(&name)),
            })
            .collect();
        let next_rally = run.save.position().0 != rally;
        self.save();
        if next_rally {
            self.open_rally();
        }
    }

    /// The player's stage is over (`times` None: retired; `drivable`: the car can go on): the season takes it and runs
    /// on.
    pub fn season_stage_done(&mut self, times: Option<StageTimes>, drivable: bool) {
        let Some(run) = self.run.as_mut().filter(|r| matches!(r.turn, Turn::Stage(_))) else { return };
        season_store::record_stage(&mut run.save, times, drivable);
        self.turn_done();
    }

    /// The park closed and its message confirmed: the late penalty goes to the season at once (`Penalty` 0x5cf330 in
    /// the close), the car leaves as the mechanics left it, and the season runs on to the next stage.
    pub(crate) fn service_done(&mut self, penalty: f32, damage: CarriedDamage) {
        let Some(c) = self.championship.take() else { return };
        self.championship = Some(c.service_done(penalty, Some(damage)));
        self.park = None;
        if let Some(run) = self.run.as_mut().filter(|r| matches!(r.turn, Turn::Service { .. })) {
            season_store::record_penalty(&mut run.save, penalty);
            self.turn_done();
        }
    }

    /// `--screenshot`: a season on a fixed seed, its first stage driven in 5:00 (ending with `damage`) and the AI run
    /// on it; without `damage`, past the service to SS2.
    pub fn screenshot_season(&mut self, table: &Season, damage: Option<CarriedDamage>) {
        let to_service = damage.is_some();
        self.new_season(table, None, 2940);
        self.start_rally();
        let end_race = EndRace { time: 300.0, splits: [100.0, 200.0], retired: false };
        if let Some(c) = self.championship.take() {
            self.championship =
                Some(c.stage_end(&StageEnd { cause: EndCause::Finish, end_race }, damage.unwrap_or_default(), true));
        }
        self.season_stage_done(Some(StageTimes::from_end_race(&end_race)), true);
        if !to_service {
            // Past the service: SS2 to drive.
            if let Some(c) = self.championship.take() {
                self.championship = Some(c.service_done(0.0, None));
            }
            self.turn_done();
        }
    }

    /// The welcome page's Continue: the rally's start (its first service stop, the car new) and the queue to the
    /// player's first stage.
    pub(crate) fn start_rally(&mut self) {
        if let Some(c) = self.championship.take() {
            self.championship = Some(c.service_done(0.0, None));
        }
        if self.run.as_ref().is_some_and(|r| r.turn == Turn::Welcome) {
            self.turn_done();
        }
    }

    /// A trial between rallies (`E72_Mbc60` 0x5abc60 the yard, map 72; `E72_Mbd30` 0x5abd30 the stage, map 71): a
    /// practice drive on tyre 0, not recorded.
    fn trial(&self, table: &Season, map: u32) -> Result<StageStart, String> {
        let row = table.rows_of(map).find(|&k| table.rows[k].letter == 0).or_else(|| table.rows_of(map).next());
        let row = row.ok_or(format!("map {map} has no weather row"))?;
        let letter = ['M', 'N', 'E', 'O', 'S'].get(table.rows[row].letter as usize).copied().unwrap_or('M');
        let stage = RallyStage { map, letter, row, hour: 12, minute: 0 };
        Ok(StageStart { stage, args: Vec::new(), tyre: 0, wear: 1, setup: SeasonSetup::Current })
    }

    /// The tyre stage `index` runs on: the pick (the surface's dry one by default) after a service, else the car's.
    fn next_tyre(&self, index: usize) -> u32 {
        let stage = self.championship.as_ref().and_then(|c| c.rally().stages.get(index));
        let Some(stage) = stage.filter(|_| choice_before(index)) else { return self.last_tyre };
        let choices = tyre_choices(crate::catalog::surface_kind(stage.map));
        self.tyre.filter(|t| choices.contains(t)).unwrap_or(choices[0])
    }

    /// Drive stage `index`: its tyre and setup row (picked after a service; else the car's tyre and the current
    /// setup), the season's options, the start clock and the damage files.
    pub(crate) fn stage_start(&mut self, here: &Path, index: usize) -> Result<StageStart, String> {
        let stage = *self.championship.as_ref().and_then(|c| c.rally().stages.get(index)).ok_or("no such stage")?;
        let stage_args = self.stage_args(here)?;
        let tyre = self.next_tyre(index);
        let setup = if choice_before(index) { self.setup } else { SeasonSetup::Current };
        let (wear, options, false_starts, start) = self.run.as_ref().map_or((1, self.options, 0, None), |r| {
            (
                season_model::road_wear(season_model::player_start_place(&r.save)),
                season_model::options(&r.save),
                season_store::player_false_starts(&r.save),
                events::start_clock(&r.save),
            )
        });
        let mut args = championship_args(options, false_starts, start);
        args.extend(["--rally-next".to_owned(), self.next_event(index).name().to_owned()]);
        args.extend(stage_args);
        self.driving = true;
        self.last_tyre = tyre;
        Ok(StageStart { stage, args, tyre, wear, setup })
    }

    fn files(here: &Path) -> (PathBuf, PathBuf, PathBuf) {
        let dir = here.join("rally");
        (dir.join("carried.dmg"), dir.join("stage.dmg"), dir.join("stage.txt"))
    }

    /// The `rbr-standalone` arguments for the stage: the damage files (the launcher passes the row).
    fn stage_args(&self, here: &Path) -> Result<Vec<String>, String> {
        let (carried, out, result) = Self::files(here);
        let dir = carried.parent().unwrap_or(here);
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        for path in [&out, &result] {
            let _ = std::fs::remove_file(path);
        }
        let mut args = Vec::new();
        if let Some(damage) = self.championship.as_ref().and_then(Championship::damage) {
            std::fs::write(&carried, damage.to_bytes()).map_err(|e| format!("{}: {e}", carried.display()))?;
            args.extend(["--damage-in".to_owned(), carried.display().to_string()]);
        }
        let path = |p: &Path| p.display().to_string();
        args.extend(["--damage-out".to_owned(), path(&out), "--result-out".to_owned(), path(&result)]);
        Ok(args)
    }

    /// The viewer closed: the stage's end from `rbr-standalone`'s files. A stage left before the finish, or a run
    /// without the files, is a retire (ours: the game retires from its pause menu).
    pub fn stage_closed(&mut self, here: &Path) {
        if !std::mem::take(&mut self.driving) || std::mem::take(&mut self.practice) {
            return;
        }
        let Some(c) = self.championship.take() else { return };
        let (_, out, result) = Self::files(here);
        let left = StageEnd { cause: EndCause::Left, end_race: EndRace { time: 0.0, splits: [0.0; 2], retired: true } };
        let (end, drivable, false_start) = read_result(&result).unwrap_or((left, true, false));
        if let Some(run) = self.run.as_mut().filter(|_| false_start) {
            season_store::record_false_start(&mut run.save);
        }
        let damage = std::fs::read(&out).ok().and_then(|b| CarriedDamage::from_bytes(&b));
        let damage = damage.or_else(|| c.damage().cloned()).unwrap_or_default();
        self.championship = Some(c.stage_end(&end, damage, drivable));
        let times = (!end.end_race.retired).then(|| StageTimes::from_end_race(&end.end_race));
        self.season_stage_done(times, drivable);
    }

    /// Opens the park for the current service stop, on the car's repair tree: under NGP (`ngp`: its `physics.rbz`) the
    /// tree of the car NGP has in the slot, with NGP's rules (the launcher's: an RSF car in slot 5 gets the archive's
    /// slot 5 tree, as a quick race's RSF car runs there); without, the retail one with 1.02's (the tests').
    pub(crate) fn open_park(&mut self, data: &Data, car: u32, ngp: Option<&Path>) {
        let Some(c) = &self.championship else { return };
        let RallyPhase::Service(stop) = c.phase() else { return };
        let (parts, rules) = match ngp {
            Some(archive) => (service::repair_parts_in(archive, car as usize), Rules::Ngp),
            None => (service::repair_parts(data, car as usize), Rules::V102),
        };
        match parts {
            Ok(parts) => {
                let damage = c.damage().cloned().unwrap_or_default();
                let crew = match (&self.run, &self.ai) {
                    (Some(run), Ok((_, roster))) => season_model::player_crew(&run.save, roster),
                    _ => None,
                };
                let crew = crew.unwrap_or_default();
                self.park = Some(ServicePark::open(stop.kind, &parts, crew, rules, &mut self.rand, damage));
                self.forfeit_asked = false;
            }
            Err(e) => self.status = format!("repairs: {e}"),
        }
    }

    /// The page for the season's car (its slot `car` and name `car_name`) under NGP (its `physics.rbz`, `ngp`);
    /// `car_row` draws the new season's car field for a team, and says whether that car can start.
    #[allow(clippy::too_many_arguments)]
    pub fn ui(
        &mut self,
        ui: &mut Ui,
        data: &Data,
        season: &Season,
        here: &Path,
        car: u32,
        car_name: &str,
        ngp: &Path,
        car_row: &mut dyn FnMut(&mut Ui, u32) -> bool,
    ) -> Option<Request> {
        if let Err(e) = &self.rallies {
            ui.colored_label(WARN, e);
            return None;
        }
        let mut request = None;
        if let Err(e) = &self.ai {
            ui.colored_label(WARN, e);
            return None;
        }
        let startable = self.options_ui(ui, car_row);
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.label("Profile");
            ui.add_enabled(!self.driving, egui::TextEdit::singleline(&mut self.profile).desired_width(100.0));
            if ui.add_enabled(!self.driving && startable, egui::Button::new("New season")).clicked() {
                let store = self.store(here);
                self.new_season(season, Some(store), season_model::seed_from_unix_ms(now_ms()));
            }
            if ui.add_enabled(!self.driving, egui::Button::new("Continue")).clicked() {
                self.continue_season(here, season);
            }
        });
        let Some(c) = self.championship.clone() else {
            if !self.status.is_empty() {
                ui.label(&self.status);
            }
            return None;
        };
        if let Some(run) = &self.run {
            let options = season_model::options(&run.save);
            let team = self.team_label(season_model::player_entry(&run.save).map_or(0, |e| run.save.drivers[e].team));
            let (hour, minute) = events::clock(&run.save);
            ui.label(
                egui::RichText::new(format!(
                    "This season: {car_name}, {team}, {}, car damage {}   Clock {hour:02}:{minute:02}",
                    options.skill.label(),
                    crate::drive_page::damage_label(options.damage)
                ))
                .color(Color32::GRAY),
            );
        }
        ui.add_space(8.0);
        egui::Grid::new("stages").num_columns(4).spacing([16.0, 4.0]).show(ui, |ui| {
            for (k, stage) in c.rally().stages.iter().enumerate() {
                ui.label(format!("SS{}", k + 1));
                ui.label(stage.track());
                ui.label(format!("{:02}:{:02}", stage.hour, stage.minute));
                ui.label(c.times().get(k).map_or("–".to_owned(), |t| clock(t.time)));
                ui.end_row();
            }
        });
        ui.label(format!("Penalties {}   Total {}", clock(c.penalty()), clock(c.total_time())));
        ui.separator();
        match c.phase() {
            RallyPhase::Stage(index) => {
                if let Some(ev) = self.run.as_ref().filter(|r| r.turn == Turn::Stage(index as u32)) {
                    if let Some(head) = ev.save.events.first() {
                        let (hour, minute) = events::time_of(head);
                        let place = season_model::player_start_place(&ev.save).map_or(0, |p| p + 1);
                        ui.label(format!("SS{}: start place {place}, at {hour:02}:{minute:02}", index + 1));
                    }
                }
                let stage = c.rally().stages[index];
                let choosing = choice_before(index);
                ui.horizontal(|ui| {
                    if choosing {
                        let choices = tyre_choices(crate::catalog::surface_kind(stage.map));
                        let tyre = self.next_tyre(index);
                        ui.label("Tyres");
                        ComboBox::from_id_salt("stage-tyre").selected_text(crate::catalog::tyre_label(tyre)).show_ui(
                            ui,
                            |ui| {
                                for &t in choices {
                                    if ui.selectable_label(t == tyre, crate::catalog::tyre_label(t)).clicked() {
                                        self.tyre = Some(t);
                                    }
                                }
                            },
                        );
                        let slots = SaveDir::new(here).setup_slots(car).unwrap_or_default();
                        let label = |row: SeasonSetup| match row {
                            SeasonSetup::Slot(n) => {
                                let name = slots.get(n as usize).cloned().flatten();
                                format!("{}: {}", row.label(), name.unwrap_or_else(|| "empty".into()))
                            }
                            _ => row.label(),
                        };
                        ui.label("Setup");
                        ComboBox::from_id_salt("stage-setup").selected_text(label(self.setup)).show_ui(ui, |ui| {
                            for row in SeasonSetup::rows(slots.len() as u32) {
                                let empty = matches!(row, SeasonSetup::Slot(n) if slots[n as usize].is_none());
                                let button = egui::Button::selectable(row == self.setup, label(row));
                                if ui.add_enabled(!empty, button).clicked() {
                                    self.setup = row;
                                }
                            }
                        });
                    } else {
                        // No service before the stage: the stage loop starts it on the car as it is.
                        ui.label(format!("Tyres {}, setup as driven", crate::catalog::tyre_label(self.last_tyre)));
                    }
                    let go = ui.add_enabled(!self.driving, egui::Button::new(format!("Drive SS{}", index + 1)));
                    if go.clicked() {
                        match self.stage_start(here, index) {
                            Ok(start) => request = Some(Request::Launch(start)),
                            Err(e) => self.status = e,
                        }
                    }
                    if choosing && ui.button("Setup").clicked() {
                        request = Some(Request::Setup);
                    }
                });
            }
            RallyPhase::Service(stop) if c.damage().is_none() => {
                // The rally's start (the season's welcome page): a fresh car, nothing to repair.
                ui.label(format!(
                    "Welcome to {}. Service before SS{}: the car is new.",
                    self.rally_name(),
                    stop.next + 1
                ));
                if self.recce_asked {
                    // `CONTINUE_SEASON` (0x5abc20) asks before the setup choice; "Yes" goes on to it (0x5abc00).
                    let text = |key| text_of(self.strings.as_ref(), key);
                    let (title, info, yes, no) =
                        (text("MSG_CRECCE"), text("INF_FORFEITRECCE"), text("YES"), text("NO"));
                    ui.label(title);
                    ui.label(info);
                    ui.horizontal(|ui| {
                        if ui.button(yes).clicked() {
                            self.recce_asked = false;
                            self.start_rally();
                        }
                        if ui.button(no).clicked() {
                            self.recce_asked = false;
                        }
                    });
                } else if ui.button("Start the rally").clicked() {
                    let welcome = self.run.as_ref().filter(|r| r.turn == Turn::Welcome);
                    if welcome.is_some_and(|r| events::recce_next(&r.save.events)) {
                        self.recce_asked = true;
                    } else {
                        self.start_rally();
                    }
                }
            }
            RallyPhase::Service(stop) => {
                if self.park.is_none() {
                    self.open_park(data, car, Some(ngp));
                }
                if let Some((penalty, damage)) = self.park_ui(ui, stop.next, &mut request) {
                    self.service_done(penalty, damage);
                }
            }
            RallyPhase::Over(outcome) => {
                ui.label(match outcome {
                    RallyOutcome::Finished => "Rally finished.".to_owned(),
                    RallyOutcome::Wrecked { stage } => {
                        // Page 0x18 (FAILED, `CFailedSeasonFinishedWithBadCarUI`): the text by the season's queue.
                        let key = match self.next_event(stage) {
                            NextEvent::Service => "MSG_UNFINISH_SERVICE",
                            _ => "MSG_UNFINISH_STAGE",
                        };
                        let text = text_of(self.strings.as_ref(), key);
                        format!(
                            "SS{} finished, failed: {}",
                            stage + 1,
                            text.split_whitespace().collect::<Vec<_>>().join(" ")
                        )
                    }
                    RallyOutcome::Retired { stage } => format!("Retired on SS{}: rally over.", stage + 1),
                });
                match self.run.as_ref().map(|r| r.turn) {
                    Some(Turn::Results) => {
                        if ui.button("Continue").clicked() {
                            self.turn_done();
                        }
                    }
                    Some(Turn::BetweenRallies) => {
                        // Page 0x19 (0x5acf30): Continue, the two trials (the car tune and the season data are the
                        // Setup page and the standings below).
                        ui.horizontal(|ui| {
                            if ui.add_enabled(!self.driving, egui::Button::new("Next rally")).clicked() {
                                self.turn_done();
                            }
                            for (label, map) in [("Yard trial", 72), ("Stage trial", 71)] {
                                if ui.add_enabled(!self.driving, egui::Button::new(label)).clicked() {
                                    match self.trial(season, map) {
                                        Ok(start) => {
                                            (self.driving, self.practice) = (true, true);
                                            request = Some(Request::Launch(start));
                                        }
                                        Err(e) => self.status = e,
                                    }
                                }
                            }
                        });
                    }
                    Some(Turn::Over) => {
                        ui.strong("The season is over.");
                    }
                    _ => {}
                }
            }
        }
        self.season_ui(ui);
        if !self.status.is_empty() {
            ui.label(&self.status);
        }
        request
    }

    /// What follows the rally's stage `index` for the player: the end of the rally after its last stage; otherwise a
    /// service when the season's queue holds one (kinds 5 and 6), which is what page 0x18's text checks
    /// (`CFailedSeasonFinishedWithBadCarUI::vf0c` 0x5b6900 walks the queue from its head). Ours: the queue is read as
    /// the stage starts; the AI's turns do not run while the player drives.
    fn next_event(&self, index: usize) -> NextEvent {
        let last = self.championship.as_ref().is_none_or(|c| index + 1 >= c.rally().stages.len());
        let service = self.run.as_ref().is_some_and(|r| {
            r.save.events.iter().any(|e| matches!(e.kind, events::kind::SERVICE | events::kind::LONG_SERVICE))
        });
        match (last, service) {
            (true, _) => NextEvent::End,
            (false, true) => NextEvent::Service,
            (false, false) => NextEvent::Stage,
        }
    }

    /// A team's row on the team offers: its name and car.
    fn team_label(&self, team: u32) -> String {
        let Ok((_, roster)) = &self.ai else { return format!("team {team}") };
        let car = season_model::TEAM_CARS.get(team as usize).and_then(|&c| roster.cars.get(c as usize));
        let name = roster.teams.get(team as usize).map_or("?", |t| t.name.as_str());
        format!("{name} ({})", car.map_or("?", |c| c.name.as_str()))
    }

    /// The options a new season takes, one field each: the game's `TEAM_OFFERS`, `SSKILL` and `CARDAM` pages, and the
    /// car (`car_row`; ours). Whether the car can start.
    fn options_ui(&mut self, ui: &mut Ui, car_row: &mut dyn FnMut(&mut Ui, u32) -> bool) -> bool {
        ui.strong("New season");
        egui::Grid::new("championship-options")
            .num_columns(2)
            .spacing([12.0, 8.0])
            .show(ui, |ui| {
                ui.label("Team");
                let label = |page: &Self, t: u32| {
                    let car = season_model::TEAM_CARS.get(t as usize);
                    let open = car.is_some_and(|c| season_model::OPEN_CARS.contains(c));
                    let lock = if open { "" } else { "  (locked in a new 1.02 profile)" };
                    format!("{}{lock}", page.team_label(t))
                };
                let selected = label(self, self.team);
                let mut team = self.team;
                ComboBox::from_id_salt("season-team").width(200.0).selected_text(selected).show_ui(ui, |ui| {
                    for t in season_model::TEAM_OFFERS {
                        ui.selectable_value(&mut team, t, label(self, t));
                    }
                });
                self.team = team;
                ui.end_row();
                ui.label("Car");
                let startable = car_row(ui, self.team);
                ui.end_row();
                ui.label("Skill Level");
                let skill = &mut self.options.skill;
                ComboBox::from_id_salt("season-skill").width(200.0).selected_text(skill.label()).show_ui(ui, |ui| {
                    for s in Skill::ALL {
                        ui.selectable_value(skill, s, s.label());
                    }
                });
                ui.end_row();
                ui.label("Car Damage");
                let damage = &mut self.options.damage;
                ComboBox::from_id_salt("season-damage")
                    .width(200.0)
                    .selected_text(crate::drive_page::damage_label(*damage))
                    .show_ui(ui, |ui| {
                        for d in DamageLevel::ALL {
                            ui.selectable_value(damage, d, crate::drive_page::damage_label(d));
                        }
                    });
                ui.end_row();
                ui.label("Outside assistance");
                ui.label(egui::RichText::new(assistance_note(self.options)).color(Color32::GRAY));
                ui.end_row();
                startable
            })
            .inner
    }

    /// The name of the season's current rally.
    fn rally_name(&self) -> String {
        let (Some(run), Ok(rallies)) = (&self.run, &self.rallies) else { return String::new() };
        let block = run.blocks.get(run.save.position().0 as usize);
        block.and_then(|b| rallies.get(b.rally as usize)).map_or("?".to_owned(), |r| r.name.clone())
    }

    /// The rally's standings after the stages done, the season's points and the last stage's bulletins.
    fn season_ui(&self, ui: &mut Ui) {
        let Some(run) = &self.run else { return };
        let done = self.championship.as_ref().map_or(0, |c| c.times().len());
        let (rally, stage) = run.save.position();
        // Stages the AI has run in this rally.
        let ran = match run.turn {
            Turn::Welcome => 0,
            Turn::Stage(s) => s as usize,
            Turn::Service { .. } => stage as usize,
            Turn::Results | Turn::BetweenRallies | Turn::Over => 6,
        };
        let ran = ran.max(done);
        ui.separator();
        ui.columns(2, |cols| {
            let ui = &mut cols[0];
            ui.strong(format!("{} — after SS{}", self.rally_name(), ran));
            egui::Grid::new("rally-standings").striped(true).spacing([12.0, 2.0]).show(ui, |ui| {
                ui.label("");
                ui.label("Driver");
                for s in 0..ran {
                    ui.label(format!("SS{}", s + 1));
                }
                ui.label("Total");
                ui.end_row();
                for (place, line) in season_model::rally_standings(&run.save, rally as usize, ran).iter().enumerate() {
                    ui.label(if line.total.is_some() { format!("{}", place + 1) } else { "–".into() });
                    let label = egui::RichText::new(&line.name);
                    let player = run.save.drivers[line.entry].id == season_model::PLAYER_ID;
                    ui.label(if player { label.strong() } else { label });
                    for t in &line.stages {
                        ui.label(t.map_or("ret".to_owned(), clock));
                    }
                    ui.label(line.total.map_or("retired".to_owned(), clock));
                    ui.end_row();
                }
            });
            let ui = &mut cols[1];
            ui.strong("Season");
            egui::Grid::new("season-standings").striped(true).spacing([12.0, 2.0]).show(ui, |ui| {
                for (place, (_, name, points)) in season_model::season_standings(&run.save).iter().enumerate() {
                    ui.label(format!("{}", place + 1));
                    ui.label(name);
                    ui.label(format!("{points}"));
                    ui.end_row();
                }
            });
            ui.add_space(8.0);
            ui.strong("Bulletins since your last turn");
            for line in &run.news {
                ui.label(line);
            }
        });
    }

    /// The park, as `CSelectPartsUI` and the timer show it: the allowed and estimated times, the damaged parts to
    /// order, the manager's advice, Repair! (with no orders, `MSG_NOREP` asks to leave), selecting during the repairs
    /// to finish them now, and the closing message. Returns the penalty and the car when closed and confirmed.
    fn park_ui(&mut self, ui: &mut Ui, next: usize, request: &mut Option<Request>) -> Option<(f32, CarriedDamage)> {
        let park = self.park.as_mut()?;
        let strings = self.strings.as_ref();
        let text = |key: &str| text_of(strings, key);
        let dt = ui.input(|i| i.stable_dt).min(0.1);
        if !self.forfeit_asked {
            park.tick(dt);
        }
        if !matches!(park.state, ParkState::Closed { .. }) {
            ui.ctx().request_repaint();
        }
        ui.strong(format!("{} before SS{}", text(park.kind.title_key()), next + 1));
        ui.label(format!(
            "{}{}   {}{}   time left {}",
            text("SP_ALLOW"),
            clock(park.kind.budget()),
            text("SP_ESTIM"),
            clock(park.estimate()),
            clock(park.time_left)
        ));
        let choosing = park.state == ParkState::Choosing && !self.forfeit_asked;
        for i in park.rows() {
            let mut on = park.covering(i).is_some();
            let line = format!(
                "{} — {}, {}",
                text_of(strings, &park.tree[i].name),
                text(service::damage_key(park.level(i))),
                clock(park.time_of(i))
            );
            if ui.add_enabled(choosing, egui::Checkbox::new(&mut on, line)).changed() {
                park.toggle(i);
            }
        }
        let mut done = None;
        if self.forfeit_asked {
            ui.label(text("MSG_NOREP"));
            ui.horizontal(|ui| {
                if ui.button(text("YES")).clicked() {
                    park.forfeit();
                    self.forfeit_asked = false;
                }
                if ui.button(text("NO")).clicked() {
                    self.forfeit_asked = false;
                }
            });
            return None;
        }
        ui.horizontal(|ui| match park.state {
            ParkState::Choosing => {
                if ui.button(text("SP_MANADV")).clicked() {
                    park.manager_advice();
                }
                if ui.button(text("SP_REPAIR")).clicked() && !park.start() {
                    self.forfeit_asked = true;
                }
                if ui.button("Setup").clicked() {
                    *request = Some(Request::Setup);
                }
            }
            ParkState::Repairing => {
                ui.label(format!("{}…", text("SP_STATUS")));
                if ui.button("Finish now").clicked() {
                    park.finish_now();
                }
            }
            ParkState::Closed { penalty, end } => {
                if let Some(key) = service::end_message(end, penalty) {
                    ui.label(text(key).replace("%1.f", &format!("{penalty:.0}")));
                }
                ui.label(format!("{}{penalty:.0} s", text("SP_PEN")));
                if ui.button("Continue").clicked() {
                    done = Some((penalty, park.damage.clone()));
                }
            }
        });
        done
    }
}

/// A string-table text by key, the key itself when the table or the key is missing.
fn text_of(strings: Option<&Strings>, key: &str) -> String {
    strings.and_then(|s| s.get(key)).unwrap_or(key).to_owned()
}

/// What follows a championship stage (`rbr-standalone --rally-next`): the rally's next stage, a service, or the end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NextEvent {
    Stage,
    Service,
    End,
}

impl NextEvent {
    fn name(self) -> &'static str {
        match self {
            NextEvent::Stage => "stage",
            NextEvent::Service => "service",
            NextEvent::End => "end",
        }
    }
}

/// `rbr-standalone`'s championship arguments: a stage in flow mode 3 on the season's options, with the player's false
/// starts so far in the rally and the start board's clock (the queue head's time).
fn championship_args(options: ChampionshipOptions, false_starts: u8, start: Option<f32>) -> Vec<String> {
    let start = start
        .map(|s| s as u32)
        .into_iter()
        .flat_map(|s| ["--start-time".into(), format!("{}:{:02}", s / 3600, s / 60 % 60)]);
    vec![
        "--championship".into(),
        options.skill.name().into(),
        "--championship-damage".into(),
        options.damage.name().into(),
        "--championship-false-starts".into(),
        false_starts.to_string(),
    ]
    .into_iter()
    .chain(start)
    .collect()
}

/// What the skill level does to outside assistance (`M576710`'s penalty, the gate `E60Flow::M566640`).
fn assistance_note(options: ChampionshipOptions) -> String {
    let quick = rbr_gameplay::race_options::RaceOptions::default();
    let settings = options.assist_settings(&quick);
    let penalty = settings.penalty();
    if rbr_gameplay::flow::assistance_gate(settings, None) {
        format!("{penalty} s penalty")
    } else {
        format!("{penalty} s penalty; a stuck car is not helped back and retires")
    }
}

/// The player's rally in the season.
pub(crate) fn player_rally(save: &SeasonSave, rally: usize) -> Option<&rbr_formats::saves::season::DriverRally> {
    save.drivers.iter().find(|d| d.id == season_model::PLAYER_ID)?.rallies.get(rally)
}

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_start_time_goes_to_the_stage_as_hours_and_minutes() {
        let args = championship_args(ChampionshipOptions::default(), 1, Some((9 * 3600 + 6 * 60) as f32));
        assert_eq!(args[args.len() - 2..], ["--start-time", "9:06"]);
        assert!(!championship_args(ChampionshipOptions::default(), 1, None).contains(&"--start-time".into()));
    }

    /// A retail season through the page (`RBR_DATA`, `RBR_PATCH`): the welcome, six stages with their services, the
    /// results, the page between rallies, then the next rally's welcome on its first stage.
    #[test]
    fn a_rally_runs_through_the_page() {
        let Some(data) = Data::from_env() else { return };
        let table = Season::load(&data).unwrap();
        let mut page = RallyPage::new(&data);
        page.team = 5;
        page.new_season(&table, None, 1234);
        assert_eq!(page.run.as_ref().map(|r| r.turn), Some(Turn::Welcome));
        assert_eq!(page.season_car(), Some(6));
        // Message 0x31 comes at the rally's start (the recce day is next), not after a service.
        let recce = |page: &RallyPage| events::recce_next(&page.run.as_ref().unwrap().save.events);
        assert!(recce(&page));
        page.start_rally();
        let end_race = EndRace { time: 300.0, splits: [100.0, 200.0], retired: false };
        let end = StageEnd { cause: EndCause::Finish, end_race };
        for stage in 0..6 {
            assert_eq!(page.run.as_ref().map(|r| r.turn), Some(Turn::Stage(stage)));
            let c = page.championship.take().unwrap();
            page.championship = Some(c.stage_end(&end, CarriedDamage::default(), true));
            page.season_stage_done(Some(StageTimes::from_end_race(&end_race)), true);
            if let Some(Turn::Service { .. }) = page.run.as_ref().map(|r| r.turn) {
                let c = page.championship.take().unwrap();
                assert!(matches!(c.phase(), RallyPhase::Service(_)));
                assert!(!recce(&page));
                page.championship = Some(c.service_done(0.0, None));
                page.turn_done();
            }
        }
        assert_eq!(page.run.as_ref().map(|r| r.turn), Some(Turn::Results));
        assert!(matches!(
            page.championship.as_ref().map(|c| c.phase()),
            Some(RallyPhase::Over(RallyOutcome::Finished))
        ));
        page.turn_done();
        assert_eq!(page.run.as_ref().map(|r| r.turn), Some(Turn::BetweenRallies));
        assert!(page.trial(&table, 72).is_ok() && page.trial(&table, 71).is_ok());
        page.turn_done();
        let run = page.run.as_ref().unwrap();
        assert_eq!((run.turn, run.save.position()), (Turn::Welcome, (1, 0)));
        assert!(matches!(page.championship.as_ref().map(|c| c.phase()), Some(RallyPhase::Service(_))));
        assert!(recce(&page));
    }

    /// What `rbr-standalone` leaves after a stage: the car's damage and a finish in `time`.
    fn drive(here: &Path, damage: &CarriedDamage, time: f32) {
        let (_, out, result) = RallyPage::files(here);
        std::fs::write(out, damage.to_bytes()).unwrap();
        let text =
            format!("finished=1\ntime={time}\nsplit1=100\nsplit2=200\ndrivable=1\ncause=Finish\nfalse_start=0\n");
        std::fs::write(result, text).unwrap();
    }

    /// Two stages of a retail season through the player's turns, read back from the `RBR7` save after each: SS1 on a
    /// picked tyre and setup row ends with a broken part and worn tyres; the service repairs it past its budget (the
    /// late penalty in the save at once, the tyres' wear cleared); SS2 on another tyre ends worn again; SS3 has no
    /// service before it, so it takes the car as SS2 left it, tyres and wear included, on the current setup.
    #[test]
    fn a_two_stage_championship_carries_damage_repairs_tyres_and_the_setup_in_the_save() {
        let Some(data) = Data::from_env() else { return };
        let table = Season::load(&data).unwrap();
        let here = std::env::temp_dir().join(format!("rbr-rally-two-stages-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&here);
        std::fs::create_dir_all(&here).unwrap();
        let mut page = RallyPage::new(&data);
        page.team = 5;
        let store = page.store(&here);
        page.new_season(&table, Some(store), 1234);
        page.start_rally();
        let saved = |page: &RallyPage| {
            let (save, after) = page.store(&here).load().unwrap().unwrap();
            let after = after.unwrap();
            let damage = CarriedDamage::from_binary(&after.damage.handler).unwrap();
            let setup = rbr_formats::saves::setup_slots::text(&after.setup);
            (save, after.damage.tyre, damage, setup)
        };
        let car = page.season_car().unwrap();
        let parts = service::repair_parts(&data, car as usize).unwrap();
        let mut sound = CarriedDamage::default();
        let mut ids = Vec::new();
        fn walk(parts: &[rbr_formats::repair::Part], ids: &mut Vec<i32>) {
            for p in parts {
                ids.push(p.physics_id);
                walk(&p.children, ids);
            }
        }
        walk(&parts, &mut ids);
        for &id in &ids {
            rbr_gameplay::car_damage::repair(&mut sound, id as u32);
        }
        sound.set(0x3AA0, 1.0f32.to_bits()); // the gearbox's wear at level 1.0
        let worn = |d: &CarriedDamage, w: f32| {
            let mut d = d.clone();
            (0..32).for_each(|k| d.set(0x3D04 + 4 * k, w.to_bits()));
            d
        };
        let map = page.championship.as_ref().unwrap().rally().stages[0].map;
        let choices = tyre_choices(crate::catalog::surface_kind(map));
        assert!(choices.len() > 1, "a tarmac or gravel rally");

        // SS1, after the rally's start: the tyre and setup row are picked.
        assert_eq!(page.run.as_ref().unwrap().turn, Turn::Stage(0));
        assert!(choice_before(0));
        (page.tyre, page.setup) = (Some(choices[1]), SeasonSetup::Default);
        let start = page.stage_start(&here, 0).unwrap();
        assert_eq!((start.tyre, start.setup), (choices[1], SeasonSetup::Default));
        assert!(!start.args.contains(&"--damage-in".to_owned()), "a new car");
        page.setup_text = b"((\"CarSetup\" Drive (\":-D\" DropGearId 12 ) ))".to_vec();
        let mut hit = worn(&sound, 0.5);
        hit.set(0x3A90, 0); // part 31 lost
        drive(&here, &hit, 300.0);
        page.stage_closed(&here);
        assert!(matches!(page.run.as_ref().unwrap().turn, Turn::Service { .. }));
        let (_, tyre, damage, setup) = saved(&page);
        assert_eq!((tyre, &damage), (choices[1], &hit));
        assert!(setup.contains("DropGearId 12"));

        // The service: part 31 ordered with 10 s of the budget left, so it runs late.
        page.open_park(&data, car, None);
        let park = page.park.as_mut().unwrap();
        let row = park.tree.iter().position(|n| n.id == 31).unwrap();
        let ids: Vec<_> = park.rows().iter().map(|&i| (park.tree[i].id, park.level(i))).collect();
        assert_eq!(park.rows(), [row], "{ids:?}");
        park.toggle(row);
        park.tick(1190.0);
        assert!(park.start());
        while park.state == ParkState::Repairing {
            park.tick(0.1);
        }
        let ParkState::Closed { penalty, end } = park.state else { panic!("{:?}", park.state) };
        assert_eq!(end, service::End::Finished);
        assert!(penalty >= 10.0);
        let repaired = park.damage.clone();
        assert_eq!(rbr_gameplay::car_damage::severity(&repaired, 31), 0);
        let mut want = sound.clone();
        want.service_refresh();
        assert_eq!(repaired, want, "the part repaired, the refresh run and the tyres' wear cleared");
        page.service_done(penalty, repaired.clone());
        assert_eq!(page.run.as_ref().unwrap().turn, Turn::Stage(1));
        let (save, _, damage, _) = saved(&page);
        assert_eq!((season_store::player_penalty(&save), &damage), (penalty, &repaired));

        // SS2 on another tyre, with the repaired car.
        assert!(choice_before(1));
        page.tyre = Some(choices[2 % choices.len()]);
        let start = page.stage_start(&here, 1).unwrap();
        assert_eq!(start.tyre, choices[2 % choices.len()]);
        assert!(start.args.contains(&"--damage-in".to_owned()));
        let (carried, _, _) = RallyPage::files(&here);
        assert_eq!(std::fs::read(&carried).unwrap(), repaired.to_bytes());
        let worn2 = worn(&repaired, 0.3);
        drive(&here, &worn2, 310.0);
        page.stage_closed(&here);

        // SS3: no service before it; the pick is not offered and the car goes on as it is.
        assert_eq!(page.run.as_ref().unwrap().turn, Turn::Stage(2));
        assert!(!choice_before(2));
        page.tyre = Some(choices[0]);
        let (save, tyre, damage, _) = saved(&page);
        assert_eq!((tyre, &damage), (choices[2 % choices.len()], &worn2));
        let r = player_rally(&save, 0).unwrap();
        assert_eq!((r.stages[0].time, r.stages[1].time, r.f170), (300.0, 310.0, penalty));
        let start = page.stage_start(&here, 2).unwrap();
        assert_eq!((start.tyre, start.setup), (choices[2 % choices.len()], SeasonSetup::Current));
        assert_eq!(std::fs::read(&carried).unwrap(), worn2.to_bytes());
        let _ = std::fs::remove_dir_all(&here);
    }

    /// A finish in a car that can't drive (page 0x18) counts the stage and ends the player's rally: the season retires
    /// the later stages and the queue goes on to the results.
    #[test]
    fn a_wrecked_finish_ends_the_rally_through_the_page() {
        let Some(data) = Data::from_env() else { return };
        let table = Season::load(&data).unwrap();
        let mut page = RallyPage::new(&data);
        page.new_season(&table, None, 1234);
        page.start_rally();
        assert_eq!(page.next_event(0), NextEvent::Service);
        assert_eq!(page.next_event(5), NextEvent::End);
        let end_race = EndRace { time: 300.0, splits: [100.0, 200.0], retired: false };
        let c = page.championship.take().unwrap();
        page.championship =
            Some(c.stage_end(&StageEnd { cause: EndCause::Finish, end_race }, CarriedDamage::default(), false));
        page.season_stage_done(Some(StageTimes::from_end_race(&end_race)), false);
        let run = page.run.as_ref().unwrap();
        assert_eq!(run.turn, Turn::Results);
        let r = player_rally(&run.save, 0).unwrap();
        assert_eq!((r.stages[0].time, r.stages[0].retired), (300.0, 0));
        assert!(r.stages[1..].iter().all(|s| s.retired == 1));
        assert!(matches!(
            page.championship.as_ref().map(|c| c.phase()),
            Some(RallyPhase::Over(RallyOutcome::Wrecked { stage: 0 }))
        ));
    }
}
