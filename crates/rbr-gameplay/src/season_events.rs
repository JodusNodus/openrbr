//! The season's clock and event queue: the rally's start and each stage's
//! fill push the events, each at the clock's time, and step the clock; the
//! stage loop takes the head and runs it: an AI driver's stage with its
//! accident pairs, the stage's end, the AI services, the points, the
//! overnight halt, the rally's end. The menu flow stops at the player's
//! events ([`Turn`]) and runs the rest. The queue lives in the save, so a
//! season continues where it stopped. Layout evidence lives in
//! [`crate::season::model`].
//!
//! The clock steps by minutes with one carry; an event holds the time it was
//! pushed at. A stage queues each starter's stage in start order, the stage's
//! end, then the services: a 20-minute stop after the first and third stages,
//! a 40-minute one with the overnight halt after the fourth, and after the
//! sixth the points, the results, the between-rallies page per entry and the
//! rally's end. A rally queues the intro, a welcome page per entry, the
//! player's two dead events, then its first stage. The stage loop records the
//! start time of day, runs AI stages with false starts and pairs, carries
//! retirements forward, runs AI services with late penalties, awards points,
//! resets the clock overnight and scores team budgets before the next rally.
//!
//! The news queue's sorting and freeing and the flow's weather and scene calls
//! are not kept; bulletins go back to the caller with their event. Kind 2 and
//! kind 7 waits never trigger in 1.02, so they pass. A season saved before the
//! queue was kept gets its stage's events again at Continue.

use rbr_formats::ai_ini::{Master, Roster};
use rbr_formats::saves::season::{Event, Season, DRIVERS};

use crate::ai_season::{self, Bulletin, Run, RunLog, TeamCosts};
use crate::crt::CrtRand;
use crate::season::{
    self, advance_clock, advance_rally, advance_stage, clock as model_clock, entry_car, entry_excluded, entry_id,
    entry_team_slot, first_stage_order, forecast_rally_id, forecast_slot_time, forecast_start_time, model, position,
    push_pair, rally_total, rewind_stage, set_clock as model_set_clock, set_first_stage_order, set_head_time,
    set_stage_start_time, set_start_order, AiField, Block,
};

/// The event kinds the stage loop and the menu flow run.
pub mod kind {
    /// The rally's intro: the flow sets the rally's scene.
    pub const RALLY_INTRO: u32 = 1;
    /// The player's recce day; waits on a flag 1.02 never sets, so it passes.
    pub const DRIVE_IN: u32 = 2;
    pub const STAGE: u32 = 3;
    pub const STAGE_END: u32 = 4;
    pub const SERVICE: u32 = 5;
    pub const LONG_SERVICE: u32 = 6;
    /// The player's rally-season page; waits like kind 2.
    pub const RALLY_SEASON: u32 = 7;
    pub const POINTS: u32 = 8;
    /// The rally's results page.
    pub const RESULTS: u32 = 9;
    pub const OVERNIGHT: u32 = 10;
    /// The between-rallies page: the trials, the car tune and the season data.
    pub const BETWEEN_RALLIES: u32 = 11;
    /// The season's welcome page before a rally.
    pub const WELCOME: u32 = 12;
    pub const RALLY_END: u32 = 14;
}

/// The season's clock: hour and minute.
pub fn clock(s: &Season) -> (u32, u32) {
    let c = model_clock(s);
    (c.hour, c.minute)
}

/// An event's time as hour and minute.
pub fn time_of(ev: &Event) -> (u32, u32) {
    ((ev.time >> 8) & 0xff, ev.time & 0xff)
}

/// Whether the recce question comes up: the head is the recce day, or the
/// first event of another kind than the head's is. At a rally's start the
/// head is the player's welcome (kept
/// at the head while its pages are up), so the question always comes; after a service it does not. An empty queue is
/// ours (the game reads the missing head's kind).
pub fn recce_next(events: &[Event]) -> bool {
    let Some(head) = events.first() else { return false };
    head.kind == kind::DRIVE_IN
        || events.iter().find(|ev| ev.kind != head.kind).is_some_and(|ev| ev.kind == kind::DRIVE_IN)
}

/// A rally stage's start clock, s after midnight, from the queue head while
/// 5) takes the hour and minute of the queue's head (`M005d2040`) when it is a driver's stage (kind 3), the seconds 0.
/// Otherwise it leaves the clock as it was, which is `None` here.
pub fn start_clock(s: &Season) -> Option<f32> {
    let head = s.events.first().filter(|ev| ev.kind == kind::STAGE)?;
    let (hour, minute) = time_of(head);
    Some((hour * 3600 + minute * 60) as f32)
}

fn set_clock(s: &mut Season, hour: u32, minute: u32) {
    model_set_clock(s, hour, minute);
}

/// The pushes' clock step: one carry.
fn tick(s: &mut Season, minutes: u32) {
    advance_clock(s, minutes);
}

fn now(s: &Season) -> u32 {
    model_clock(s).word()
}

/// An event with every field.
fn full(s: &Season, kind: u32, id: i32, stage: u32, ai: bool) -> Event {
    let pos = position(s);
    Event { id: id as u32, stage, rally: pos.rally, ai: u32::from(ai), time: now(s), kind }
}

/// A driver's event (stage and rally -1).
fn driver(s: &Season, kind: u32, id: i32, ai: bool) -> Event {
    Event { id: id as u32, stage: u32::MAX, rally: u32::MAX, ai: u32::from(ai), time: now(s), kind }
}

/// The rally's event (player id, stage -1).
fn rally_event(s: &Season, kind: u32) -> Event {
    let pos = position(s);
    Event { id: season::PLAYER_ID as u32, stage: u32::MAX, rally: pos.rally, ai: 0, time: now(s), kind }
}

/// Push the event at the clock's time, then step the clock.
fn push(s: &mut Season, ev: Event, minutes: u32) {
    s.events.push(ev);
    tick(s, minutes);
}

fn id_of(s: &Season, entry: u32) -> i32 {
    entry_id(s, entry as usize).unwrap_or(-1)
}

/// The next stage and its events.
pub fn fill_stage(s: &mut Season) {
    advance_stage(s);
    s.pairs.clear();
    let order = season::stage_start_order(s);
    set_start_order(s, order);
    let pos = position(s);
    let (rally, stage) = (pos.rally, pos.stage);
    let (hour, minute) = forecast_slot_time(s, rally as usize, stage as usize);
    set_clock(s, u32::from(hour), u32::from(minute));
    let starters: Vec<u32> = order.into_iter().filter(|&k| !season::out_of_rally(s, k as usize)).collect();
    for &k in &starters {
        let id = id_of(s, k);
        push(s, full(s, kind::STAGE, id, stage, id >= 0), 3);
    }
    push(s, full(s, kind::STAGE_END, 0, stage, false), 15);
    let service = |s: &mut Season, long: bool| {
        tick(s, 15);
        for &k in &starters {
            let id = id_of(s, k);
            let kind = if long { kind::LONG_SERVICE } else { kind::SERVICE };
            push(s, driver(s, kind, id, id >= -1), if long { 2 } else { 1 });
        }
        tick(s, 30);
    };
    match stage {
        0 | 2 => service(s, false),
        3 => {
            service(s, true);
            s.events.push(rally_event(s, kind::OVERNIGHT));
        }
        _ => {}
    }
    if stage == 5 {
        push(s, rally_event(s, kind::POINTS), 60);
        tick(s, 30);
        push(s, rally_event(s, kind::RESULTS), 60);
        for k in 0..DRIVERS as u32 {
            let id = id_of(s, k);
            s.events.push(driver(s, kind::BETWEEN_RALLIES, id, id >= 0));
        }
        push(s, rally_event(s, kind::RALLY_END), 60);
    }
}

/// The next rally (the first from a new season): the teams' spending and the
/// cars reset, the clock to 08:00, the start order with the queue cleared,
/// then the rally's events and its first stage's.
pub fn rally_start(s: &mut Season, field: &mut AiField) {
    advance_rally(s);
    field.rally_start(s);
    set_clock(s, 8, 0);
    let order = season::rally_start_order(s);
    set_first_stage_order(s, order);
    s.events.clear();
    s.events_b.clear();
    push(s, rally_event(s, kind::RALLY_INTRO), 60);
    for k in first_stage_order(s) {
        let id = id_of(s, k);
        s.events.push(driver(s, kind::WELCOME, id, id >= 0));
    }
    let pos = position(s);
    let (hour, minute) = forecast_start_time(s, pos.rally as usize);
    set_clock(s, u32::from(hour), u32::from(minute));
    for k in first_stage_order(s) {
        let id = entry_id(s, k as usize).unwrap_or(-1);
        if id < 0 && !entry_excluded(s, k as usize) {
            push(s, full(s, kind::DRIVE_IN, id, 0, false), 15);
            push(s, driver(s, kind::RALLY_SEASON, id, false), 15);
        }
    }
    fill_stage(s);
}

/// What the AI's events need: `ai.ini`, `aidrv.ini`, the AI field, the season's blocks and the `rand()` stream.
pub struct Ai<'a> {
    pub master: &'a Master,
    pub roster: &'a Roster,
    pub field: &'a mut AiField,
    pub blocks: &'a [Block],
    pub rng: &'a mut CrtRand,
}

/// A bulletin and the event that posted it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct News {
    pub event: Event,
    pub bulletin: Bulletin,
}

/// An accident pair: the id, section, accident and retired bytes,
/// then the time.
fn pair(a: &ai_season::AccidentRecord) -> [u32; 2] {
    let bytes = [a.driver as u8, a.section as u8, a.accident, u8::from(a.retired)];
    [u32::from_le_bytes(bytes), a.time.to_bits()]
}

/// The stage's pairs of driver `id` into its result, four at most.
fn fill_pairs(s: &mut Season, id: i32) {
    let pairs: Vec<[u32; 2]> = s.pairs.iter().filter(|p| p[0] & 0xff == id as u32).copied().collect();
    for p in pairs {
        if !push_pair(s, id, p) {
            break;
        }
    }
}

/// Case 3 for an AI driver: its stage on the track and weather, with false
/// starts, time, sectors and accident pairs.
fn ai_stage(s: &mut Season, ai: &mut Ai, ev: &Event, news: &mut Vec<News>) {
    let id = ev.id as i32;
    let Some(k) = s.drivers.iter().position(|d| d.id == id) else { return };
    let pos = position(s);
    let Some(slot) = ai.blocks.get(pos.rally as usize).and_then(|b| b.stages.get(ev.stage as usize)) else {
        return;
    };
    // A map `ai.ini` has no track for is not run.
    let Some(mut track) = ai.master.tracks.iter().find(|t| t.stage == slot.map).copied() else { return };
    track.wet = u32::from(slot.wetness == 2);
    let run = Run { track: &track, level: s.rally_level(), stage: ev.stage, extra: 0 };
    let (car_entry, team_slot) = (entry_car(s, k).unwrap_or(0), entry_team_slot(s, k).unwrap_or(0) as usize);
    let (Some(d), Some(car), Some(team)) = (
        ai.field.drivers.get_mut(id as usize),
        ai.field.cars.get_mut(car_entry as usize),
        ai.roster.teams.get(team_slot),
    ) else {
        return;
    };
    let mut log = RunLog::default();
    let starts = &mut s.drivers[k].rallies[pos.rally as usize].false_starts;
    let before = *starts;
    let time = ai_season::stage_time(ai.master, &run, id, d, car, team, starts, ai.rng, &mut log);
    let after = *starts;
    if before < after {
        log.bulletins.push(if after < 4 {
            Bulletin::FalseStart { driver: id, stage: ev.stage }
        } else {
            Bulletin::FalseStartRetire { driver: id, stage: ev.stage }
        });
    }
    s.set_stage_time(id, time);
    for (n, &v) in d.sections.iter().enumerate() {
        s.set_sector(id, n as u8, v);
    }
    s.pairs.extend(log.accidents.iter().map(pair));
    fill_pairs(s, id);
    news.extend(log.bulletins.into_iter().map(|bulletin| News { event: *ev, bulletin }));
}

/// Cases 5 and 6: the AI driver's service and its late penalty.
fn ai_service(s: &mut Season, ai: &mut Ai, ev: &Event, long: bool, news: &mut Vec<News>) {
    let id = ev.id as i32;
    if id < 0 {
        return;
    }
    let pos = position(s);
    for k in 0..DRIVERS {
        if entry_id(s, k).unwrap_or(-1) != id || season::out_of_rally(s, k) {
            continue;
        }
        let (car_entry, team_slot) = (entry_car(s, k).unwrap_or(0), entry_team_slot(s, k).unwrap_or(0) as usize);
        let (Some(d), Some(car), Some(team), Some(costs)) = (
            ai.field.drivers.get_mut(id as usize),
            ai.field.cars.get_mut(car_entry as usize),
            ai.roster.teams.get(team_slot),
            ai.field.teams.get_mut(team_slot),
        ) else {
            continue;
        };
        let mut log = RunLog::default();
        let minutes = ai_season::service(ai.master, id, d, car, team, costs, long, ai.rng, &mut log);
        let penalty = ai_season::service_penalty(id, minutes, long, &mut log);
        model::add_rally_penalty(s, k, pos.rally as usize, penalty);
        news.extend(log.bulletins.into_iter().map(|bulletin| News { event: *ev, bulletin }));
    }
}

/// The team's budget after the rally's score, less half the rally's repairs: one
/// `rand() % 100` against it for 100 more, or against its loss for 100 less; below 100 the budget is 100 and the
/// manager's style 3. It returns 0, so `ScoreAll`'s branch on it never runs.
fn budget_after(costs: &mut TeamCosts, score: i32, rng: &mut CrtRand) {
    let v = score.wrapping_sub((costs.rally >> 1) as i32);
    if v >= 0 {
        if rng.draw() % 100 < v {
            costs.budget = costs.budget.wrapping_add(100);
        }
    } else if rng.draw() % 100 < v.wrapping_neg() {
        let b = (costs.budget as i32).wrapping_sub(100);
        if b < 100 {
            (costs.style, costs.budget) = (3, 100);
        } else {
            costs.budget = b as u32;
        }
    }
}

/// Each team's score from its drivers' places in the rally (by time total,
/// those without a retired stage: 200, 100, 75, 30 for the first four, -50,
/// -20, -10 for the last three; -40 for a retired driver), then its budget,
/// teams 0-6 in order.
pub fn score_teams(s: &Season, field: &mut AiField, rng: &mut CrtRand) {
    let pos = position(s);
    let finished = |k: usize| s.drivers[k].rallies[pos.rally as usize].stages.iter().all(|st| st.retired == 0);
    let count = (0..DRIVERS).filter(|&k| finished(k)).count() as i32;
    let order = season::bubble(season::ENTRIES, |n, c| {
        rally_total(s, c, pos.rally as usize) > rally_total(s, n, pos.rally as usize)
    });
    for t in 0..7u32 {
        let mut score = 0i32;
        let mut place = 0i32;
        for k in order.iter().map(|&k| k as usize) {
            let mine = entry_team_slot(s, k).unwrap_or(99) == t;
            if finished(k) {
                if mine {
                    score += match place {
                        0 => 200,
                        1 => 100,
                        2 => 75,
                        3 => 30,
                        p if p == count - 1 => -50,
                        p if p == count - 2 => -20,
                        p if p == count - 3 => -10,
                        _ => 0,
                    };
                }
                place += 1;
            } else if mine {
                score -= 40;
            }
        }
        if let Some(costs) = field.teams.get_mut(t as usize) {
            budget_after(costs, score, rng);
        }
    }
}

/// The head event, moved to the events run, and its case. Returns the event.
pub fn run_event(s: &mut Season, ai: &mut Ai, news: &mut Vec<News>) -> Option<Event> {
    if s.events.is_empty() {
        return None;
    }
    let ev = s.events.remove(0);
    s.events_b.push(ev);
    let pos = position(s);
    let (rally, stage) = (pos.rally, pos.stage);
    match ev.kind {
        kind::STAGE => {
            // The start's time of day into the stage's result.
            let (hour, minute) = time_of(&ev);
            set_stage_start_time(s, ev.id as i32, hour as f32 * 3600.0 + minute as f32 * 60.0);
            if (ev.id as i32) >= 0 {
                ai_stage(s, ai, &ev, news);
            }
        }
        kind::STAGE_END => {
            if stage > 0 {
                model::carry_retire_forward(s);
            }
            if s.pairs.is_empty() {
                news.push(News { event: ev, bulletin: Bulletin::Uneventful { stage } });
            }
            if stage < 5 {
                fill_stage(s);
            }
        }
        kind::SERVICE | kind::LONG_SERVICE => ai_service(s, ai, &ev, ev.kind == kind::LONG_SERVICE, news),
        kind::POINTS => season::award_points(s, rally as usize),
        kind::OVERNIGHT => set_clock(s, 8, 0),
        kind::RALLY_END if rally < 5 => {
            score_teams(s, ai.field, ai.rng);
            rally_start(s, ai.field);
        }
        _ => {}
    }
    Some(ev)
}

/// Where the menu flow stops for the player.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Turn {
    /// The season's welcome page before a rally (kind 12).
    Welcome,
    /// The player's stage (kind 3), unless the player has retired from the rally (`AnyRec5c`).
    Stage(u32),
    /// The player's service after a stage (kinds 5 and 6, not retired): `long` the 40-minute one.
    Service { long: bool },
    /// The rally's results page (kind 9).
    Results,
    /// The between-rallies page (kind 11): the trials, the car tune and the season data.
    BetweenRallies,
    /// The season is over: the queue is empty, or a kind 11 is at the head in the last rally (block id 3).
    Over,
}

/// Whether the entry with `id` has a retired stage in the rally.
fn retired(s: &Season, id: i32) -> bool {
    let (rally, _) = s.position();
    s.drivers
        .iter()
        .find(|d| d.id == id)
        .and_then(|d| d.rallies.get(rally as usize))
        .is_some_and(|r| r.stages.iter().any(|st| st.retired != 0))
}

/// The turn at the head of the queue, if the flow stops there; peeking
/// stores the head's time.
pub fn turn(s: &mut Season) -> Option<Turn> {
    let Some(&head) = s.events.first() else { return Some(Turn::Over) };
    set_head_time(s, head.time);
    let pos = position(s);
    let rally_id = forecast_rally_id(s, pos.rally as usize);
    if head.kind == kind::BETWEEN_RALLIES && rally_id == Some(3) {
        return Some(Turn::Over);
    }
    let player = (head.id as i32) < 0;
    match head.kind {
        kind::STAGE if player && !retired(s, head.id as i32) => Some(Turn::Stage(head.stage)),
        kind::SERVICE | kind::LONG_SERVICE if player && !retired(s, head.id as i32) => {
            Some(Turn::Service { long: head.kind == kind::LONG_SERVICE })
        }
        kind::RESULTS => Some(Turn::Results),
        kind::BETWEEN_RALLIES if player => Some(Turn::BetweenRallies),
        kind::WELCOME if player => Some(Turn::Welcome),
        _ => None,
    }
}

/// Runs the queue to the player's next turn.
pub fn run_to_turn(s: &mut Season, ai: &mut Ai, news: &mut Vec<News>) -> Turn {
    loop {
        if let Some(t) = turn(s) {
            return t;
        }
        run_event(s, ai, news);
    }
}

/// The player's turn is done then the queue runs to the
/// next one.
pub fn turn_done(s: &mut Season, ai: &mut Ai, news: &mut Vec<News>) -> Turn {
    if turn(s) != Some(Turn::Over) {
        run_event(s, ai, news);
    }
    run_to_turn(s, ai, news)
}

/// A season saved before the queue was kept (no events, the season not
/// over): the current stage's events again, from the stage's start.
pub fn ensure_queue(s: &mut Season) {
    if !s.events.is_empty() || !s.events_b.is_empty() {
        return;
    }
    rewind_stage(s);
    fill_stage(s);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::race_options::ChampionshipOptions;
    use rbr_formats::ai_ini::Roster;

    fn empty() -> (Roster, AiField) {
        let roster = Roster { drivers: vec![], teams: vec![], cars: vec![] };
        let field = AiField::new(&roster);
        (roster, field)
    }

    fn new_season(roster: &Roster, field: &mut AiField, team: u32) -> Season {
        let player = season::Player { name: "P", team, rank: 0 };
        let mut s = season::create(roster, &[], ChampionshipOptions::default(), player, &[], &mut CrtRand(1));
        rally_start(&mut s, field);
        s
    }

    #[test]
    fn the_clock_carries_once() {
        let (roster, mut field) = empty();
        let mut s = new_season(&roster, &mut field, 6);
        set_clock(&mut s, 9, 50);
        tick(&mut s, 15);
        assert_eq!(clock(&s), (10, 5));
        // One carry only: +60 at 9:50 is 10:50.
        set_clock(&mut s, 9, 50);
        tick(&mut s, 60);
        assert_eq!(clock(&s), (10, 50));
    }

    /// A rally's first events in the asm's order and times: the intro at 08:00, the welcome pages, the player's two
    /// dead events at the start slot's time, then from the stage slot's time (00:00 both here: no blocks) the stage's
    /// starters 3 minutes apart, its end, and the first service.
    #[test]
    fn a_rally_starts_with_its_events() {
        let (roster, mut field) = empty();
        let s = new_season(&roster, &mut field, 6);
        assert_eq!(s.position(), (0, 0));
        assert_eq!(season::player_entry(&s), Some(12));
        let kinds: Vec<u32> = s.events.iter().map(|e| e.kind).collect();
        let mut want = vec![1];
        want.extend([12; 14]);
        want.extend([2, 7]);
        want.extend([3; 14]);
        want.push(4);
        want.extend([5; 14]);
        assert_eq!(kinds, want);
        // The welcome pages' turn: the recce day is the next other kind.
        assert!(!recce_next(&s.events));
        assert!(recce_next(&s.events[1..]));
        assert!(recce_next(&s.events[15..]));
        assert!(!recce_next(&s.events[17..]));
        assert!(!recce_next(&[]));
        assert_eq!(s.events[0].time, 8 << 8);
        assert_eq!(time_of(&s.events[15]), (0, 0));
        assert_eq!(time_of(&s.events[16]), (0, 15));
        let stages: Vec<(u32, u32)> = s.events[17..31].iter().map(time_of).collect();
        assert_eq!(stages[0], (0, 0));
        assert_eq!(stages[13], (0, 39));
        // The end 3 minutes after the last start, the first service 30 after the end, a minute apart, then 30 more.
        assert_eq!(time_of(&s.events[31]), (0, 42));
        assert_eq!(time_of(&s.events[32]), (1, 12));
        assert_eq!(time_of(&s.events[45]), (1, 25));
        assert_eq!(clock(&s), (1, 56));
        // Every entry's +0x900 is 0: the start order is the entries' (a stable sort), the player 13th.
        assert_eq!(s.w8e4c, season::ENTRIES);
        assert_eq!(s.events[17 + 12].id as i32, season::PLAYER_ID);
    }

    /// The start clock is the head's time while a driver's stage is at the head, else none.
    #[test]
    fn the_start_clock_is_the_heads_stage_time() {
        let (roster, mut field) = empty();
        let mut s = new_season(&roster, &mut field, 6);
        assert_eq!(start_clock(&s), None, "the rally's intro at the head");
        s.events.drain(..17 + 12);
        s.events[0].time = 9 << 8 | 36;
        assert_eq!(start_clock(&s), Some((9 * 3600 + 36 * 60) as f32));
    }

    /// Stage 3's fill ends with the 40-minute services and the overnight halt; its case sets the clock to 08:00.
    #[test]
    fn the_overnight_halt_sets_eight_oclock() {
        let (roster, mut field) = empty();
        let mut s = new_season(&roster, &mut field, 6);
        s.events.clear();
        s.state[2] = 2;
        fill_stage(&mut s);
        let kinds: Vec<u32> = s.events.iter().map(|e| e.kind).collect();
        assert_eq!(kinds.iter().filter(|&&k| k == kind::LONG_SERVICE).count(), 14);
        assert_eq!(*kinds.last().unwrap(), kind::OVERNIGHT);
        let master = crate::ai_season::tests::master();
        let mut rng = CrtRand(1);
        let mut ai = Ai { master: &master, roster: &roster, field: &mut field, blocks: &[], rng: &mut rng };
        let mut news = Vec::new();
        while s.events.first().is_some_and(|e| e.kind != kind::OVERNIGHT) {
            s.events.remove(0);
        }
        set_clock(&mut s, 17, 40);
        assert_eq!(run_event(&mut s, &mut ai, &mut news).map(|e| e.kind), Some(kind::OVERNIGHT));
        assert_eq!(clock(&s), (8, 0));
        assert_eq!(s.events_b.last().map(|e| e.kind), Some(kind::OVERNIGHT));
    }

    /// The player's turns through a rally with no AI data: the welcome page, each stage and the three services, the
    /// results, the between-rallies page, then the next rally's welcome.
    #[test]
    fn the_player_turns_follow_the_queue() {
        let (roster, mut field) = empty();
        let mut s = new_season(&roster, &mut field, 2);
        let master = crate::ai_season::tests::master();
        let mut rng = CrtRand(1);
        let mut ai = Ai { master: &master, roster: &roster, field: &mut field, blocks: &[], rng: &mut rng };
        let mut news = Vec::new();
        let mut turns = vec![run_to_turn(&mut s, &mut ai, &mut news)];
        while turns.len() < 13 {
            if let Some(Turn::Stage(_)) = turns.last() {
                s.set_stage_time(season::PLAYER_ID, 100.0);
            }
            turns.push(turn_done(&mut s, &mut ai, &mut news));
        }
        use Turn::*;
        let short = Service { long: false };
        assert_eq!(
            turns,
            [
                Welcome,
                Stage(0),
                short,
                Stage(1),
                Stage(2),
                short,
                Stage(3),
                Service { long: true },
                Stage(4),
                Stage(5),
                Results,
                BetweenRallies,
                Welcome
            ]
        );
        assert_eq!(s.position(), (1, 0));
        assert!(news.iter().any(|n| matches!(n.bulletin, Bulletin::Uneventful { .. })));
    }

    /// The last rally ends the season at its between-rallies event (block id 3).
    #[test]
    fn the_season_ends_after_the_last_rally() {
        let (roster, mut field) = empty();
        let mut s = new_season(&roster, &mut field, 0);
        s.state[1] = 5;
        s.blocks[5 * season::BLOCK..5 * season::BLOCK + 4].copy_from_slice(&3u32.to_le_bytes());
        s.events = vec![driver(&s, kind::BETWEEN_RALLIES, 3, true)];
        assert_eq!(turn(&mut s), Some(Turn::Over));
    }

    /// A retired player has no more stage or service turns in the rally.
    #[test]
    fn a_retired_player_skips_to_the_results() {
        let (roster, mut field) = empty();
        let mut s = new_season(&roster, &mut field, 3);
        let master = crate::ai_season::tests::master();
        let mut rng = CrtRand(1);
        let mut ai = Ai { master: &master, roster: &roster, field: &mut field, blocks: &[], rng: &mut rng };
        let mut news = Vec::new();
        assert_eq!(run_to_turn(&mut s, &mut ai, &mut news), Turn::Welcome);
        assert_eq!(turn_done(&mut s, &mut ai, &mut news), Turn::Stage(0));
        let entry = season::player_entry(&s).unwrap();
        let stage = s.position().1 as usize;
        season::fill_neg(&mut s, entry, stage);
        assert_eq!(turn_done(&mut s, &mut ai, &mut news), Turn::Results);
    }

    /// `Dmg`: 100 more on a roll below the score less half the repairs, 100 less on one below the loss, and below 100
    /// the budget is 100 with style 3.
    #[test]
    fn the_budget_follows_the_score() {
        let seed = |pass: bool, v: i32| (0..u32::MAX).find(|&s| (CrtRand(s).draw() % 100 < v) == pass).unwrap();
        let mut c = TeamCosts { budget: 150, service: 0, rally: 40, style: 1 };
        budget_after(&mut c, 70, &mut CrtRand(seed(true, 50)));
        assert_eq!(c.budget, 250);
        budget_after(&mut c, 70, &mut CrtRand(seed(false, 50)));
        assert_eq!(c.budget, 250);
        budget_after(&mut c, -40, &mut CrtRand(seed(true, 60)));
        assert_eq!((c.budget, c.style), (150, 1));
        budget_after(&mut c, -40, &mut CrtRand(seed(true, 60)));
        assert_eq!((c.budget, c.style), (100, 3));
    }

    /// The pairs of a driver go into the stage's result in order, four at most.
    #[test]
    fn the_pairs_fill_the_result() {
        let (roster, mut field) = empty();
        let mut s = new_season(&roster, &mut field, 6);
        let rec =
            |driver, section| ai_season::AccidentRecord { driver, section, accident: 2, retired: false, time: 1.5 };
        s.pairs = (0..5).map(|k| pair(&rec(3, k))).chain([pair(&rec(4, 0))]).collect();
        fill_pairs(&mut s, 3);
        let r = &s.drivers[3].rallies[0].stages[0];
        assert_eq!(r.b30[0], 4);
        assert_eq!(r.elems[1], [u32::from_le_bytes([3, 1, 2, 0]), 1.5f32.to_bits()]);
        assert_eq!(s.drivers[4].rallies[0].stages[0].b30[0], 0);
    }
}
