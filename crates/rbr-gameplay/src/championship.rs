//! The championship's rally: its stages from the season data, run stage by stage with the season's service stops, on
//! the stage ends of [`crate::flow`]. Pure. The season around it (the AI drivers, points, standings) is
//! [`crate::season`].
//!
//! Traced: the application's `CSigEndRace` receiver (`CApplication::M00480810`, 0x480810) in the championship's mode
//! (3). For a finished stage it gives the season (`E80Daemon` at `*0x7d6128`) the race time (`M005d14d0`) and the three
//! sectors (`M005d15b0` 0, 1, 2: split 1, split 2 - split 1, time - split 2); then, unless the car's test
//! (`CCarDamageHandler::Test3B20` 0x4ec0d0, non-zero while the car can drive: the engine word +0x3b20, which
//! `EngineGate` 0x4ef680 clears, and at least two of the four wheel words +0x3a30 + 0xc*k) holds or the stage was the
//! rally's last (the index +0x8e8c is 5), it moves the season to the next stage (`M005d0960`) and opens menu mode 0x18;
//! otherwise menu mode 0x13. Which screens those are is not traced; the rally goes on either way here (ours).
//! For a retire it records the retire (`M005d09b0`: `E80Entry::FillNeg` 0x5ce300 sets the time -1.0 and the retire flag
//! of the current stage and of every later stage not yet driven, so the rally is over for the player) and opens the
//! retire screen (0x17).
//!
//! The stages come from the season's forecast ([`crate::season::Block::rally`]); without one from `season.dat`
//! ([`Rally::from_season`]: the map and the track letter of `s<level>_st<n>.lsp`).
//! The service stops are the season's events ([`crate::service::service_before`]); the late penalties add to the
//! rally's penalty (`E80Daemon::Penalty` 0x5cf330, the entry's +0x170). The damage the car ends a stage with is carried
//! to the next ([`crate::car_damage`]). The rally's total (time plus penalties) is ours; the season's standings are
//! [`crate::season`]'s.

use rbr_formats::season_dat;

use crate::car_damage::CarriedDamage;
use crate::flow::{EndPage, EndRace, Mode, StageEnd};
use crate::service::{self, ServiceKind};
use crate::weather;

/// A stage of a rally: the map the stage loads and its track letter (`Maps/track-<map>_<letter>`), and its start time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RallyStage {
    pub map: u32,
    /// The weather row's track letter (M, N, E, O, S).
    pub letter: char,
    /// The season-table row the stage boots with (`rbr_gameplay::weather`, the random weather's row).
    pub row: usize,
    pub hour: u32,
    pub minute: u32,
}

impl RallyStage {
    /// The stage file's name, `track-<map>_<letter>`.
    pub fn track(&self) -> String {
        format!("track-{}_{}", self.map, self.letter)
    }
}

/// A rally: its stages in order. The season's rallies have six (the receiver's last index is 5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rally {
    pub stages: Vec<RallyStage>,
}

impl Rally {
    /// Rally `rally` (0 USA ... 5 France, [`season_dat::RALLIES`]) of the season's `level` (1-3), each stage on its
    /// weather row of the season table (`weather`).
    pub fn from_season(
        rallies: &[season_dat::Rally],
        weather: &weather::Season,
        rally: usize,
        level: usize,
    ) -> Option<Rally> {
        let stages = rallies.get(rally)?.stages(level)?;
        Some(Rally { stages: stages.iter().map(|s| stage_weather(s, weather)).collect::<Option<_>>()? })
    }
}

/// A stage on its weather row: the map's first season-table row with the stage's `TimeOfDay` letter, else the map's
/// first row; the stage loads that row's letter. Ours: a rally without the season's forecast; the forecast's rows
/// (`E80Blk28c::Init6` 0x5cea80) are [`crate::season::Block::rally`]'s.
pub fn stage_weather(info: &season_dat::StageInfo, weather: &weather::Season) -> Option<RallyStage> {
    let rows = || weather.rows.iter().enumerate().filter(|(_, r)| r.map == info.map);
    let (row, r) = rows().find(|(_, r)| r.letter == info.time_of_day).or_else(|| rows().next())?;
    let letter = *['M', 'N', 'E', 'O', 'S'].get(r.letter as usize)?;
    Some(RallyStage { map: info.map, letter, row, hour: info.hour, minute: info.minute })
}

/// `SELECTTYRES`, the page after the setup choice (`E56_AfterCarSwitch` 0x5581a0 opens page 0x56, 0x57 or 0x58 on the
/// next stage's surface, `E14_4a1330`): the tyres the player may pick, the dry one first (the page's tyre picture
/// starts on it, `OpCE0(.., 0x40000004, 0/3/6)`). Tarmac (0x5b5da0): dry, intermediate, wet (0-2); gravel (0x5b6100):
/// dry, normal, wet (3-5); snow (0x5b6460): snow (6). The pick goes to the gameflow's tyre byte (`M5662a0`) and
/// `0x1660838`. There is no tyre stock: the pick is a compound. The tyres' wear (the four `CU32Value8` at +0x3d04 of
/// the damage handler) is carried damage: it goes on into a stage without a service before it and each service's
/// refresh clears it ([`CarriedDamage::service_refresh`]).
pub fn tyre_choices(surface: u32) -> &'static [u32] {
    match surface {
        0 => &[3, 4, 5],
        1 => &[0, 1, 2],
        _ => &[6],
    }
}

/// Whether the player picks the setup and the tyres before stage `index`: only after a service stop. The service's
/// close (0x5c5530) and the rally's start (kind 7 in `M568cf0`) open page 0x1a (0x5ad390); its `CONTINUE_SEASON`
/// (0x5abc20) opens the setup choice (page 0x6c, `CSeasonChooseSetupUI`), then `SELECTTYRES` ([`tyre_choices`]), whose
/// pick runs the season on (`E74_TeamOffer` 0x5b5810 into `M568cf0`). A stage with no service before it is started
/// from the stage loop at once (kind 3, `M0047def0`), on the car's setup and tyres.
pub fn choice_before(index: usize) -> bool {
    service::service_before(index).is_some()
}

/// A finished stage's times as the receiver gives them to the season.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StageTimes {
    pub time: f32,
    pub sectors: [f32; 3],
}

impl StageTimes {
    /// The race time and the sectors from the `CSigEndRace` payload: `split 1`, `split 2 - split 1`, `time - split 2`
    /// (SSE f32 subtractions).
    pub fn from_end_race(end: &EndRace) -> Self {
        let [split1, split2] = end.splits;
        Self { time: end.time, sectors: [split1, split2 - split1, end.time - split2] }
    }
}

/// A service stop ([`crate::service::ServicePark`] runs it).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Service {
    /// The stage that follows the stop.
    pub next: usize,
    pub kind: ServiceKind,
}

/// Where the rally stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RallyPhase {
    /// The stage with this index is to be driven (or under way).
    Stage(usize),
    /// A service stop before a stage.
    Service(Service),
    /// The rally is over: every stage done, or retired (0x17).
    Over(RallyOutcome),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RallyOutcome {
    Finished,
    /// The stage was finished with a car that can't drive, before the last stage (page 0x18): its time counts, the
    /// later stages are retired.
    Wrecked {
        stage: usize,
    },
    /// The stage was retired (page 0x17).
    Retired {
        stage: usize,
    },
}

/// A championship rally under way.
#[derive(Debug, Clone, PartialEq)]
pub struct Championship {
    rally: Rally,
    phase: RallyPhase,
    times: Vec<StageTimes>,
    /// The late penalties so far, in seconds.
    penalty: f32,
    /// The damage the car carries into the next stage; none before the first stage (a fresh car).
    damage: Option<CarriedDamage>,
}

impl Championship {
    /// The rally at its start: the first service stop (or stage). An empty rally is over at once.
    pub fn start(rally: Rally) -> Self {
        let phase = if rally.stages.is_empty() { RallyPhase::Over(RallyOutcome::Finished) } else { Self::before(0) };
        Self { rally, phase, times: Vec::new(), penalty: 0.0, damage: None }
    }

    /// A rally resumed at `phase` with the stages done (a saved rally).
    pub fn resume(
        rally: Rally,
        phase: RallyPhase,
        times: Vec<StageTimes>,
        penalty: f32,
        damage: Option<CarriedDamage>,
    ) -> Self {
        Self { rally, phase, times, penalty, damage }
    }

    /// The phase before stage `index`: its service stop, if the season has one there, else the stage.
    fn before(index: usize) -> RallyPhase {
        match service::service_before(index) {
            Some(kind) => RallyPhase::Service(Service { next: index, kind }),
            None => RallyPhase::Stage(index),
        }
    }

    pub fn rally(&self) -> &Rally {
        &self.rally
    }

    pub fn phase(&self) -> RallyPhase {
        self.phase
    }

    /// The stage to drive now, if the rally is at one.
    pub fn stage(&self) -> Option<(usize, RallyStage)> {
        match self.phase {
            RallyPhase::Stage(index) => Some((index, self.rally.stages[index])),
            _ => None,
        }
    }

    /// The finished stages' times, in order.
    pub fn times(&self) -> &[StageTimes] {
        &self.times
    }

    /// The late penalties so far.
    pub fn penalty(&self) -> f32 {
        self.penalty
    }

    /// The damage carried into the next stage (none for a fresh car).
    pub fn damage(&self) -> Option<&CarriedDamage> {
        self.damage.as_ref()
    }

    /// The rally's time so far: the finished stages' race times and the penalties added up (ours).
    pub fn total_time(&self) -> f32 {
        self.times.iter().map(|t| t.time).sum::<f32>() + self.penalty
    }

    /// The stage's end, as the receiver takes it in mode 3 (`flow::EndPage::of`), with the damage the car ends it with
    /// and whether it can still drive (`Test3B20`). Ignored outside a stage.
    pub fn stage_end(self, end: &StageEnd, damage: CarriedDamage, drivable: bool) -> Self {
        let RallyPhase::Stage(index) = self.phase else { return self };
        let damage = Some(damage);
        let last = index + 1 >= self.rally.stages.len();
        let phase = match EndPage::of(Mode::CHAMPIONSHIP, end, drivable, last) {
            EndPage::RallyRetired => {
                return Self { phase: RallyPhase::Over(RallyOutcome::Retired { stage: index }), damage, ..self };
            }
            EndPage::RallyWrecked => RallyPhase::Over(RallyOutcome::Wrecked { stage: index }),
            _ if last => RallyPhase::Over(RallyOutcome::Finished),
            _ => Self::before(index + 1),
        };
        let mut times = self.times;
        times.push(StageTimes::from_end_race(&end.end_race));
        Self { phase, times, damage, ..self }
    }

    /// The service stop is over, with its late penalty and the car as the mechanics leave it: the next stage.
    pub fn service_done(self, penalty: f32, damage: Option<CarriedDamage>) -> Self {
        match self.phase {
            RallyPhase::Service(service) => Self {
                phase: RallyPhase::Stage(service.next),
                penalty: self.penalty + penalty,
                damage: damage.or(self.damage),
                ..self
            },
            _ => self,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flow::EndCause;

    #[test]
    fn the_setup_and_tyres_are_picked_after_a_service_only() {
        assert_eq!((0..6).map(choice_before).collect::<Vec<_>>(), [true, true, false, true, true, false]);
    }

    #[test]
    fn each_surface_offers_its_own_tyres() {
        assert_eq!(tyre_choices(0), [3, 4, 5]);
        assert_eq!(tyre_choices(1), [0, 1, 2]);
        assert_eq!(tyre_choices(2), [6]);
    }

    fn stage(map: u32) -> RallyStage {
        RallyStage { map, letter: 'M', row: 0, hour: 9, minute: 0 }
    }

    fn rally(n: u32) -> Rally {
        Rally { stages: (0..n).map(|k| stage(10 + k)).collect() }
    }

    fn worn() -> CarriedDamage {
        let mut d = CarriedDamage::default();
        d.tail[0] = 7;
        d
    }

    fn end(time: f32, retired: bool) -> StageEnd {
        StageEnd {
            cause: if retired { EndCause::Bystander } else { EndCause::Finish },
            end_race: EndRace { time, splits: [time * 0.25, time * 0.75], retired },
        }
    }

    #[test]
    fn a_rally_runs_its_stages_with_the_season_services() {
        use ServiceKind::*;
        let mut c = Championship::start(rally(6));
        let mut seen = Vec::new();
        for index in 0..6 {
            if let RallyPhase::Service(s) = c.phase() {
                assert_eq!(s.next, index);
                seen.push((index, s.kind));
                c = c.service_done(30.0, None);
            }
            assert_eq!(c.stage(), Some((index, stage(10 + index as u32))));
            c = c.stage_end(&end(100.0, false), worn(), true);
        }
        assert_eq!(seen, [(0, Start), (1, Short), (3, Short), (4, Long)]);
        assert_eq!(c.phase(), RallyPhase::Over(RallyOutcome::Finished));
        assert_eq!(c.times().len(), 6);
        assert_eq!(c.penalty(), 120.0);
        assert_eq!(c.total_time(), 720.0);
        assert_eq!(c.damage(), Some(&worn()));
    }

    #[test]
    fn damage_is_carried_and_the_service_replaces_it() {
        let c = Championship::start(rally(3));
        assert_eq!(c.damage(), None);
        let c = c.service_done(0.0, None).stage_end(&end(100.0, false), worn(), true);
        assert_eq!(c.damage(), Some(&worn()));
        let c = c.service_done(0.0, Some(CarriedDamage::default()));
        assert_eq!(c.phase(), RallyPhase::Stage(1));
        assert_eq!(c.damage(), Some(&CarriedDamage::default()));
    }

    #[test]
    fn a_finish_in_a_car_that_cannot_drive_ends_the_rally_but_counts_unless_last() {
        let c = Championship::start(rally(2)).service_done(0.0, None).stage_end(&end(100.0, false), worn(), false);
        assert_eq!(c.phase(), RallyPhase::Over(RallyOutcome::Wrecked { stage: 0 }));
        assert_eq!(c.times().len(), 1);
        let c = Championship::start(rally(1)).service_done(0.0, None).stage_end(&end(100.0, false), worn(), false);
        assert_eq!(c.phase(), RallyPhase::Over(RallyOutcome::Finished));
    }

    #[test]
    fn a_retire_ends_the_rally() {
        let c = Championship::start(rally(6)).service_done(0.0, None).stage_end(&end(100.0, false), worn(), true);
        let c = c.service_done(0.0, None).stage_end(&end(40.0, true), worn(), true);
        assert_eq!(c.phase(), RallyPhase::Over(RallyOutcome::Retired { stage: 1 }));
        assert_eq!(c.times().len(), 1);
    }

    #[test]
    fn the_season_gives_the_stages_on_their_rows() {
        let info = |map, tod| season_dat::StageInfo { map, time_of_day: tod, tint_set: 0, hour: 8, minute: 15 };
        let row = |map, letter| weather::Row { map, letter, cloud: 0, cloud_letter: 0, preset: 0, wetness: 0 };
        let table = weather::Season { rows: vec![row(63, 3), row(63, 1), row(34, 2)], nice: vec![], bad: vec![] };
        let mut files = [info(63, 1); 7];
        files[2] = info(34, 1);
        let rallies = [season_dat::Rally {
            name: "USA".into(),
            setup: season_dat::RallySetup {
                fog_chance: 0.0,
                rain_chance: 0.0,
                temperature: 0.0,
                max_rain: 0.0,
                mid_rain: 0.0,
            },
            levels: vec![files],
        }];
        let r = Rally::from_season(&rallies, &table, 0, 1).unwrap();
        assert_eq!(r.stages.len(), 6);
        assert_eq!((r.stages[0].track(), r.stages[0].row), ("track-63_N".to_string(), 1));
        // No row with the letter: the map's first.
        assert_eq!((r.stages[1].track(), r.stages[1].row), ("track-34_E".to_string(), 2));
        assert_eq!(Rally::from_season(&rallies, &table, 0, 2), None);
    }

    /// Every retail rally's stages boot on an installed stage (`RBR_DATA`, `RBR_PATCH`).
    #[test]
    fn retail_rallies_load_installed_stages() {
        let Some(data) = rbr_formats::data::Data::from_env() else { return };
        if data.patch().is_none() {
            return;
        }
        let table = weather::Season::load(&data).unwrap();
        let rallies = season_dat::parse(&std::fs::read(data.resolve("Root/season.dat").unwrap()).unwrap()).unwrap();
        for rally in 0..6 {
            for level in 1..=3 {
                let r = Rally::from_season(&rallies, &table, rally, level).unwrap();
                for s in &r.stages {
                    let track = format!("Maps/{}.trk", s.track());
                    assert!(data.resolve(&track).is_some(), "rally {rally} level {level}: no {track}");
                }
            }
        }
    }

    #[test]
    fn sectors_are_the_split_differences() {
        let times = StageTimes::from_end_race(&EndRace { time: 300.0, splits: [100.0, 250.0], retired: false });
        assert_eq!(times.sectors, [100.0, 150.0, 50.0]);
    }

    #[test]
    fn events_outside_their_phase_change_nothing() {
        let c = Championship::start(rally(2));
        assert_eq!(c.clone().stage_end(&end(1.0, false), worn(), true), c);
        let over = Championship::start(rally(1)).service_done(0.0, None).stage_end(&end(1.0, false), worn(), true);
        assert_eq!(over.clone().stage_end(&end(1.0, false), worn(), true), over);
        assert_eq!(over.clone().service_done(10.0, None), over);
        assert_eq!(Championship::start(rally(0)).phase(), RallyPhase::Over(RallyOutcome::Finished));
    }
}
