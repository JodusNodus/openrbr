//! The season's clock and event queue (`E80Daemon` +0x8e98, +0x8e9c and `E81ListHost` `*0x7d614c`), from the asm:
//! the rally's start (`M4990` 0x5d4990) and each stage's fill (`M3800` 0x5d3800) push the events, each at the clock's
//! time, and step the clock; the stage loop (`M4c90` 0x5d4c90) takes the head (`Shift` 0x5d5f60, which moves it to the
//! list of events run) and runs it: an AI driver's stage with its accident pairs, the stage's end, the AI services,
//! the points, the overnight halt, the rally's end. The menu flow (`E60Flow::M568cf0`) stops at the player's events
//! ([`Turn`]) and runs the rest. The queue is the save's (`EVEN`, `ACCC`), so a season continues where it stopped.
//!
//! Faithful (from the asm):
//! - the clock steps by minutes with one carry (`minute += n; if minute >= 60 { minute -= 60; hour += 1 }`); an event
//!   holds the time it was pushed at (`hour << 8 | minute`); peeking the head stores its time in +0xa0c0
//!   (`M005d2040`, the save's `state[7]`);
//! - a stage's events ([`fill_stage`]): the clock from the stage's slot, each starter's stage (kind 3, +3 minutes)
//!   in the start order, the stage's end (4, +15), then after the first and third stages a 20-minute service (5, +15,
//!   one per starter +1, +30), after the fourth a 40-minute one (6, +2 each) and the overnight halt (10, the clock to
//!   08:00), after the sixth the points (8, +60, +30), the results (9, +60), the between-rallies page for each entry
//!   (11) and the rally's end (14, +60);
//! - a rally's events ([`rally_start`]): the clock to 08:00, the start order, the rally's intro (1, +60), the welcome
//!   page for each entry (12), the clock from the start's slot, the player's two dead events (2 and 7, +15 each), then
//!   the first stage;
//! - the stage loop's cases ([`run_event`]): 3 the start time into the result (`Distribute` 0x5cfa10) and an AI
//!   driver's run (`Ev094e0`, [`ai_season::stage_time`]) with its false starts, time, sectors and accident pairs
//!   (the pairs vector +0x8ea0, `FillPairs` 0x5d2910 into the stage's result); 4 the stage marked retired for the
//!   drivers out after the last one, the uneventful bulletin when the stage had no pairs, the next stage's fill;
//!   5 and 6 the AI driver's service (`Ev09f80`) and its late penalty; 8 the points (`SortByT6` 0x5d2070); 10 the
//!   overnight 08:00; 14 the teams' budgets (`ScoreAll` 0x5d23d0, `E78Entry::Dmg` 0x5d6df0) and the next rally.
//!
//! Ours: the events' pointer table (+0x8eb8, +0xa0b8), +0xa0bc, the news queue's sorting and freeing and the
//! weather and scene calls of the flow are not kept; the bulletins go back to the caller with their event. Kind 2 and
//! kind 7 wait on `E60Flow` +0xd08, which 1.02 never sets, so they pass. A season this crate saved before it kept the
//! queue gets its stage's events again at Continue ([`ensure_queue`]).

use rbr_formats::ai_ini::{Master, Roster};
use rbr_formats::saves::season::{Event, Season, DRIVERS, RETIRED};

use crate::ai_season::{self, Bulletin, Run, RunLog, TeamCosts};
use crate::crt::CrtRand;
use crate::season::{self, AiField, Block};

/// The event kinds (the stage loop's cases 0x5d52fc and the flow's, `M568cf0`).
pub mod kind {
    /// The rally's intro: the flow sets the rally's scene.
    pub const RALLY_INTRO: u32 = 1;
    /// The player's recce day (`Push2`; `MSG_CRECCE`, "Forfeit Recce?"); waits on `E60Flow` +0xd08, never set in 1.02.
    pub const DRIVE_IN: u32 = 2;
    pub const STAGE: u32 = 3;
    pub const STAGE_END: u32 = 4;
    pub const SERVICE: u32 = 5;
    pub const LONG_SERVICE: u32 = 6;
    /// The player's rally-season page (`Push7`, page 0x1a); waits on +0xd08 like kind 2.
    pub const RALLY_SEASON: u32 = 7;
    pub const POINTS: u32 = 8;
    /// The rally's results page (`M567220`).
    pub const RESULTS: u32 = 9;
    pub const OVERNIGHT: u32 = 10;
    /// The between-rallies page (0x19: the trials, the car tune, the season data).
    pub const BETWEEN_RALLIES: u32 = 11;
    /// The season's welcome page (0x1b) before a rally.
    pub const WELCOME: u32 = 12;
    pub const RALLY_END: u32 = 14;
}

/// `state` words: the clock (+0x8e98, +0x8e9c) and the head's time (+0xa0c0).
const HOUR: usize = 5;
const MINUTE: usize = 6;
const HEAD_TIME: usize = 7;

/// The season's clock: hour and minute.
pub fn clock(s: &Season) -> (u32, u32) {
    (s.state[HOUR], s.state[MINUTE])
}

/// An event's time as hour and minute.
pub fn time_of(ev: &Event) -> (u32, u32) {
    ((ev.time >> 8) & 0xff, ev.time & 0xff)
}

/// `E80Daemon::HasType2` (0x5d2e00): the head is the recce day (kind 2), or the first event of another kind than the
/// head's is. `CONTINUE_SEASON` (0x5abc20) then asks message 0x31 (`MSG_CRECCE`, `INF_FORFEITRECCE`; 0x58c4f0) before
/// the setup page 0x6c, which "Yes" opens (0x5abc00). At a rally's start the head is the player's welcome (kind 12, kept
/// at the head while its pages are up), so the question always comes; after a service it does not. An empty queue is
/// ours (the game reads the missing head's kind).
pub fn recce_next(events: &[Event]) -> bool {
    let Some(head) = events.first() else { return false };
    head.kind == kind::DRIVE_IN
        || events.iter().find(|ev| ev.kind != head.kind).is_some_and(|ev| ev.kind == kind::DRIVE_IN)
}

/// A rally stage's start clock, s after midnight: `FUN_005733c0` (0x5733c0, from `CWeatherDaemon::Vf2c` in modes 3 and
/// 5) takes the hour and minute of the queue's head (`M005d2040`) when it is a driver's stage (kind 3), the seconds 0.
/// Otherwise it leaves the clock as it was, which is `None` here.
pub fn start_clock(s: &Season) -> Option<f32> {
    let head = s.events.first().filter(|ev| ev.kind == kind::STAGE)?;
    let (hour, minute) = time_of(head);
    Some((hour * 3600 + minute * 60) as f32)
}

fn set_clock(s: &mut Season, hour: u32, minute: u32) {
    (s.state[HOUR], s.state[MINUTE]) = (hour, minute);
}

/// The pushes' clock step: one carry.
fn tick(s: &mut Season, minutes: u32) {
    let m = s.state[MINUTE].wrapping_add(minutes);
    if m >= 60 {
        s.state[MINUTE] = m - 60;
        s.state[HOUR] = s.state[HOUR].wrapping_add(1);
    } else {
        s.state[MINUTE] = m;
    }
}

fn now(s: &Season) -> u32 {
    (s.state[HOUR] << 8).wrapping_add(s.state[MINUTE])
}

/// 0x5d5cc0: an event with every field.
fn full(s: &Season, kind: u32, id: i32, stage: u32, ai: bool) -> Event {
    Event { id: id as u32, stage, rally: s.state[1], ai: u32::from(ai), time: now(s), kind }
}

/// 0x5d5c80: a driver's event (stage and rally -1).
fn driver(s: &Season, kind: u32, id: i32, ai: bool) -> Event {
    Event { id: id as u32, stage: u32::MAX, rally: u32::MAX, ai: u32::from(ai), time: now(s), kind }
}

/// 0x5d5c40: the rally's event (id -2, stage -1).
fn rally_event(s: &Season, kind: u32) -> Event {
    Event { id: season::PLAYER_ID as u32, stage: u32::MAX, rally: s.state[1], ai: 0, time: now(s), kind }
}

/// The pushes (0x5cf6f0-0x5d1f50): the event at the clock's time, then the clock's step.
fn push(s: &mut Season, ev: Event, minutes: u32) {
    s.events.push(ev);
    tick(s, minutes);
}

fn id_of(s: &Season, entry: u32) -> i32 {
    s.drivers.get(entry as usize).map_or(-1, |d| d.id)
}

/// `M3800` (0x5d3800): the next stage and its events.
pub fn fill_stage(s: &mut Season) {
    let stage = (s.state[2] as i32).wrapping_add(1);
    s.state[3] = stage.wrapping_sub(1) as u32;
    s.state[2] = stage.min(5) as u32;
    s.pairs.clear();
    let order = season::stage_start_order(s);
    season::store_start_order(s, order);
    let (rally, stage) = s.position();
    let slot = rally as usize * season::BLOCK + 4 + stage as usize * season::SLOT + 0xc;
    let (hour, minute) = s.blocks.get(slot..slot + 2).map_or((0, 0), |b| (u32::from(b[0]), u32::from(b[1])));
    set_clock(s, hour, minute);
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

/// `M4990` (0x5d4990): the next rally (the first from a new season, [`season::create`]): the teams' spending and the
/// cars ([`AiField::rally_start`]), the clock to 08:00, the start order with the queue cleared, the rally's events
/// and its first stage's.
pub fn rally_start(s: &mut Season, field: &mut AiField) {
    s.state[1] = (s.state[1] as i32).wrapping_add(1).min(5) as u32;
    s.state[2] = u32::MAX;
    field.rally_start(s);
    set_clock(s, 8, 0);
    s.w8e4c = season::rally_start_order(s);
    s.events.clear();
    s.events_b.clear();
    push(s, rally_event(s, kind::RALLY_INTRO), 60);
    for k in s.w8e4c {
        let id = id_of(s, k);
        s.events.push(driver(s, kind::WELCOME, id, id >= 0));
    }
    let at = s.state[1] as usize * season::BLOCK + 0x208;
    let (hour, minute) = s.blocks.get(at..at + 2).map_or((8, 0), |b| (u32::from(b[0]), u32::from(b[1])));
    set_clock(s, hour, minute);
    for k in s.w8e4c {
        let e = &s.drivers[k as usize];
        if e.id < 0 && e.w8fc == 0 {
            let id = e.id;
            push(s, full(s, kind::DRIVE_IN, id, 0, false), 15);
            // `Push7` (0x5d1f50) pushes for ids below -1 only; for another it pushes the last event again.
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

/// An accident pair (`E79PairTmp::Init` 0x5cbc50): the id, section, accident (0xff a false start) and retired bytes,
/// then the time.
fn pair(a: &ai_season::AccidentRecord) -> [u32; 2] {
    let bytes = [a.driver as u8, a.section as u8, a.accident, u8::from(a.retired)];
    [u32::from_le_bytes(bytes), a.time.to_bits()]
}

/// `FillPairs` (0x5d2910): the stage's pairs of driver `id` into its result, four at most.
fn fill_pairs(s: &mut Season, id: i32) {
    let (rally, stage) = s.position();
    let pairs: Vec<[u32; 2]> = s.pairs.iter().filter(|p| p[0] & 0xff == id as u32).copied().collect();
    for p in pairs {
        for e in s.drivers.iter_mut().filter(|e| e.id == id) {
            let r = &mut e.rallies[rally as usize].stages[stage as usize];
            if r.b30[0] < 4 {
                r.elems[r.b30[0] as usize] = p;
                r.b30[0] += 1;
            }
        }
    }
}

/// Case 3 for an AI driver (0x5d4ccc): `Ev094e0` on the stage's track and weather, its false starts, the time and
/// sectors (`M005d1360`, `M005d1440`) and the pairs.
fn ai_stage(s: &mut Season, ai: &mut Ai, ev: &Event, news: &mut Vec<News>) {
    let id = ev.id as i32;
    let Some(k) = s.drivers.iter().position(|d| d.id == id) else { return };
    let (rally, _) = s.position();
    let Some(slot) = ai.blocks.get(rally as usize).and_then(|b| b.stages.get(ev.stage as usize)) else { return };
    // Ours: a map `ai.ini` has no track for is not run (the original reads past its list).
    let Some(mut track) = ai.master.tracks.iter().find(|t| t.stage == slot.map).copied() else { return };
    track.wet = u32::from(slot.wetness == 2);
    let run = Run { track: &track, level: s.rally_level(), stage: ev.stage, extra: 0 };
    let e = &s.drivers[k];
    let (Some(d), Some(car), Some(team)) = (
        ai.field.drivers.get_mut(id as usize),
        ai.field.cars.get_mut(e.car as usize),
        ai.roster.teams.get(e.team as usize),
    ) else {
        return;
    };
    let mut log = RunLog::default();
    let starts = &mut s.drivers[k].rallies[rally as usize].false_starts;
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

/// Cases 5 and 6 (0x5d4e2a, 0x5d4ec0): the AI driver's service and its late penalty (`Penalty` 0x5cf330).
fn ai_service(s: &mut Season, ai: &mut Ai, ev: &Event, long: bool, news: &mut Vec<News>) {
    let id = ev.id as i32;
    if id < 0 {
        return;
    }
    let (rally, _) = s.position();
    for k in 0..DRIVERS {
        let e = &s.drivers[k];
        if e.id != id || season::out_of_rally(s, k) {
            continue;
        }
        let (Some(d), Some(car), Some(team), Some(costs)) = (
            ai.field.drivers.get_mut(id as usize),
            ai.field.cars.get_mut(e.car as usize),
            ai.roster.teams.get(e.team as usize),
            ai.field.teams.get_mut(e.team as usize),
        ) else {
            continue;
        };
        let mut log = RunLog::default();
        let minutes = ai_season::service(ai.master, id, d, car, team, costs, long, ai.rng, &mut log);
        let penalty = ai_season::service_penalty(id, minutes, long, &mut log);
        if let Some(r) = s.drivers[k].rallies.get_mut(rally as usize) {
            r.f170 += penalty;
        }
        news.extend(log.bulletins.into_iter().map(|bulletin| News { event: *ev, bulletin }));
    }
}

/// `E78Entry::Dmg` (0x5d6df0): the team's budget after the rally's score, less half the rally's repairs: one
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

/// `ScoreAll` (0x5d23d0): each team's score from its drivers' places in the rally (by +0x168, those without a retired
/// stage: 200, 100, 75, 30 for the first four, -50, -20, -10 for the last three; -40 for a retired driver), then its
/// budget ([`budget_after`]), teams 0-6 in order.
pub fn score_teams(s: &Season, field: &mut AiField, rng: &mut CrtRand) {
    let (rally, _) = s.position();
    let r = |k: usize| &s.drivers[k].rallies[rally as usize];
    let finished = |k: usize| r(k).stages.iter().all(|st| st.retired == 0);
    let count = (0..DRIVERS).filter(|&k| finished(k)).count() as i32;
    let order = season::bubble(season::ENTRIES, |n, c| r(c).f168 > r(n).f168);
    for t in 0..7u32 {
        let mut score = 0i32;
        let mut place = 0i32;
        for k in order.iter().map(|&k| k as usize) {
            let mine = s.drivers[k].team == t;
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

/// `M4c90` (0x5d4c90): the head event, moved to the events run, and its case. Returns the event.
pub fn run_event(s: &mut Season, ai: &mut Ai, news: &mut Vec<News>) -> Option<Event> {
    if s.events.is_empty() {
        return None;
    }
    let ev = s.events.remove(0);
    s.events_b.push(ev);
    let (rally, stage) = s.position();
    match ev.kind {
        kind::STAGE => {
            // `Distribute`: the start's time of day into the stage's result.
            let (hour, minute) = time_of(&ev);
            for e in s.drivers.iter_mut().filter(|e| e.id == ev.id as i32) {
                if let Some(r) = e.rallies.get_mut(rally as usize).and_then(|r| r.stages.get_mut(stage as usize)) {
                    r.f34 = hour as f32 * 3600.0 + minute as f32 * 60.0;
                }
            }
            if (ev.id as i32) >= 0 {
                ai_stage(s, ai, &ev, news);
            }
        }
        kind::STAGE_END => {
            if stage > 0 {
                for e in s.drivers.iter_mut() {
                    let r = &mut e.rallies[rally as usize];
                    let last = &r.stages[stage as usize - 1];
                    if last.time < 0.0 || last.retired != 0 {
                        r.stages[stage as usize].retired = 1;
                        r.stages[stage as usize].time = RETIRED;
                    }
                }
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

/// Where the menu flow stops for the player (`E60Flow::M568cf0`).
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

/// `AnyRec5c` (0x5cfe90): the entry with `id` has a retired stage in the rally.
fn retired(s: &Season, id: i32) -> bool {
    let (rally, _) = s.position();
    s.drivers
        .iter()
        .find(|d| d.id == id)
        .and_then(|d| d.rallies.get(rally as usize))
        .is_some_and(|r| r.stages.iter().any(|st| st.retired != 0))
}

/// The turn at the head of the queue, if the flow stops there; peeking stores the head's time (+0xa0c0).
pub fn turn(s: &mut Season) -> Option<Turn> {
    let Some(&head) = s.events.first() else { return Some(Turn::Over) };
    s.state[HEAD_TIME] = head.time;
    let (rally, _) = s.position();
    let rally_id = s.blocks.get(rally as usize * season::BLOCK..).and_then(|b| b.get(..4));
    if head.kind == kind::BETWEEN_RALLIES && rally_id == Some(&3u32.to_le_bytes()[..]) {
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

/// Runs the queue to the player's next turn (`M568cf0`'s loop).
pub fn run_to_turn(s: &mut Season, ai: &mut Ai, news: &mut Vec<News>) -> Turn {
    loop {
        if let Some(t) = turn(s) {
            return t;
        }
        run_event(s, ai, news);
    }
}

/// The player's turn is done (`M568cf0` with its argument set: the head's case first), then the queue runs to the
/// next one.
pub fn turn_done(s: &mut Season, ai: &mut Ai, news: &mut Vec<News>) -> Turn {
    if turn(s) != Some(Turn::Over) {
        run_event(s, ai, news);
    }
    run_to_turn(s, ai, news)
}

/// A season this crate saved before it kept the queue (no events, the season not over): the current stage's events
/// again (ours), from the stage's start.
pub fn ensure_queue(s: &mut Season) {
    if !s.events.is_empty() || !s.events_b.is_empty() {
        return;
    }
    s.state[2] = s.state[2].wrapping_sub(1);
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
