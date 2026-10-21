//! Named gameplay view over the profile season save.
//!
//! The save type (`rbr_formats::saves::season::Season`) keeps the game's
//! layout byte for byte. Gameplay code should use the names here: the rally
//! and stage in progress, the season clock, each entry's rank and results,
//! and the start orders. Only this module reads the layout words directly.
//!
//! Evidence lives here so the gameplay modules do not need it. Daemon words
//! at `0x8e84` on: word 0 the skill level, 1 the rally index, 2 the stage
//! index (low byte), 3 the previous stage, 5 and 6 the clock hour and minute,
//! 7 the head event's time (`M005d2040`), 8 the rank-order flag (`+0xa0c4`),
//! 13 the set-up flag written as 1. Entry words at `+0x8d0` (player flag),
//! `+0x8d8` (always 0 in the saves seen), `+0x8fc` (excluded from points),
//! `+0x900` (qualifying rank), `+0x904` (car instance), `+0x908` (team slot).
//! Rally words at `+0x168` (time total), `+0x16c` (points), `+0x170`
//! (penalties), `+0x174` (false starts). Stage result words hold the three
//! sectors, the time, four accident pairs with their count (`+0x30`), the
//! start time of day (`+0x34`) and the retired flag. `+0x7ecc` is the current
//! stage's start order, `+0x8e4c` the rally's first, `+0x7f04` the forecast
//! blocks (stage start times at slot `+0xc`, the rally start at block
//! `+0x208`, the block id first). Unknown words pass through untouched.

use rbr_formats::saves::season::{Season, DRIVERS, RETIRED};

use super::forecast::{BLOCK, SLOT};

/// Entries in season order, 0-13.
pub const ENTRIES: [u32; DRIVERS] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13];

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

impl SeasonClock {
    /// The clock as the queue stores it: hour << 8 | minute.
    pub fn word(self) -> u32 {
        (self.hour << 8).wrapping_add(self.minute)
    }
}

/// Where the season stands.
pub fn position(save: &Season) -> RallyPosition {
    let (rally, stage) = save.position();
    RallyPosition { rally, stage, previous_stage: save.state[3] }
}

/// The clock.
pub fn clock(save: &Season) -> SeasonClock {
    SeasonClock { hour: save.state[5], minute: save.state[6] }
}

/// Set the clock.
pub fn set_clock(save: &mut Season, hour: u32, minute: u32) {
    save.state[5] = hour;
    save.state[6] = minute;
}

/// Step the clock by minutes, with the game's single carry.
pub fn advance_clock(save: &mut Season, minutes: u32) {
    let m = save.state[6].wrapping_add(minutes);
    if m >= 60 {
        save.state[6] = m - 60;
        save.state[5] = save.state[5].wrapping_add(1);
    } else {
        save.state[6] = m;
    }
}

/// The queue head's stored time (`+0xa0c0`).
pub fn head_time(save: &Season) -> u32 {
    save.state[7]
}

/// Remember the queue head's time.
pub fn set_head_time(save: &mut Season, time: u32) {
    save.state[7] = time;
}

/// The skill level (word 0).
pub fn skill_level(save: &Season) -> u32 {
    save.state[0]
}

/// Whether the start orders use ranks even past the first rally (`+0xa0c4`).
/// While set the player also stays in the rally.
pub fn rank_order_override(save: &Season) -> bool {
    save.state[8] != 0
}

/// Move to the next rally: index up by one to a cap of 5, stage unset.
pub fn advance_rally(save: &mut Season) {
    save.state[1] = (save.state[1] as i32).wrapping_add(1).min(5) as u32;
    save.state[2] = u32::MAX;
}

/// Move to the next stage for its fill: the stage index up by one to a cap
/// of 5, the previous stage one behind.
pub fn advance_stage(save: &mut Season) {
    let stage = (save.state[2] as i32).wrapping_add(1);
    save.state[3] = stage.wrapping_sub(1) as u32;
    save.state[2] = stage.min(5) as u32;
}

/// Step the stage index back by one, for rebuilding the current stage's queue.
pub fn rewind_stage(save: &mut Season) {
    save.state[2] = save.state[2].wrapping_sub(1);
}

/// The player's entry (the one with id -2).
pub fn player_entry(save: &Season) -> Option<usize> {
    save.drivers.iter().position(|d| d.id == super::PLAYER_ID)
}

/// An entry's driver id.
pub fn entry_id(save: &Season, entry: usize) -> Option<i32> {
    save.drivers.get(entry).map(|d| d.id)
}

/// Whether the entry is the player.
pub fn entry_is_player(save: &Season, entry: usize) -> bool {
    save.drivers.get(entry).is_some_and(|d| d.id == super::PLAYER_ID)
}

/// An entry's qualifying rank.
pub fn entry_rank(save: &Season, entry: usize) -> u32 {
    save.drivers.get(entry).map_or(0, |d| d.w900)
}

/// Set an entry's qualifying rank.
pub fn set_entry_rank(save: &mut Season, entry: usize, rank: u32) {
    if let Some(d) = save.drivers.get_mut(entry) {
        d.w900 = rank;
    }
}

/// Whether the entry is excluded from points.
pub fn entry_excluded(save: &Season, entry: usize) -> bool {
    save.drivers.get(entry).is_some_and(|d| d.w8fc != 0)
}

/// An entry's car instance.
pub fn entry_car(save: &Season, entry: usize) -> Option<u32> {
    save.drivers.get(entry).map(|d| d.car)
}

/// An entry's team slot.
pub fn entry_team_slot(save: &Season, entry: usize) -> Option<u32> {
    save.drivers.get(entry).map(|d| d.team)
}

/// An entry's name.
pub fn entry_name(save: &Season, entry: usize) -> String {
    save.drivers.get(entry).map_or_else(String::new, |d| d.name())
}

/// Whether the entry is out of the current rally: a retired stage or a
/// negative time, unless the override keeps the player in.
pub fn out_of_rally(save: &Season, entry: usize) -> bool {
    let pos = position(save);
    let Some(e) = save.drivers.get(entry) else { return false };
    if rank_order_override(save) && e.id < 0 {
        return false;
    }
    e.rallies.get(pos.rally as usize).is_some_and(retired_in)
}

fn retired_in(r: &rbr_formats::saves::season::DriverRally) -> bool {
    r.stages.iter().any(|s| s.retired != 0 || s.time < 0.0)
}

/// The rally's time total with penalties, -1e8 when out of the rally.
pub fn start_key(save: &Season, entry: usize) -> f32 {
    let pos = position(save);
    if out_of_rally(save, entry) {
        return -100_000_000.0;
    }
    let r = &save.drivers[entry].rallies[pos.rally as usize];
    r.f170 + r.f168
}

/// An entry's rally time total.
pub fn rally_total(save: &Season, entry: usize, rally: usize) -> f32 {
    save.drivers.get(entry).and_then(|d| d.rallies.get(rally)).map_or(0.0, |r| r.f168)
}

/// An entry's rally penalties.
pub fn rally_penalty(save: &Season, entry: usize, rally: usize) -> f32 {
    save.drivers.get(entry).and_then(|d| d.rallies.get(rally)).map_or(0.0, |r| r.f170)
}

/// Add to an entry's rally penalties.
pub fn add_rally_penalty(save: &mut Season, entry: usize, rally: usize, penalty: f32) {
    if let Some(r) = save.drivers.get_mut(entry).and_then(|d| d.rallies.get_mut(rally)) {
        r.f170 += penalty;
    }
}

/// An entry's rally points.
pub fn rally_points(save: &Season, entry: usize, rally: usize) -> u32 {
    save.drivers.get(entry).and_then(|d| d.rallies.get(rally)).map_or(0, |r| r.w16c)
}

/// Add to an entry's rally points, as the points award does.
pub fn add_rally_points(save: &mut Season, entry: usize, rally: usize, points: u32) {
    if let Some(r) = save.drivers.get_mut(entry).and_then(|d| d.rallies.get_mut(rally)) {
        r.w16c += points;
    }
}

/// An entry's season points.
pub fn season_points(save: &Season, entry: usize) -> u32 {
    save.drivers.get(entry).map_or(0, |d| d.rallies.iter().map(|r| r.w16c).sum())
}

/// An entry's season points with the start order's wrapping add, kept
/// distinct from [`season_points`]: the first-rally order folds with
/// wrapping while the standings sum plainly, as before.
pub fn wrapping_season_points(save: &Season, entry: usize) -> u32 {
    save.drivers.get(entry).map_or(0, |d| d.rallies.iter().fold(0u32, |a, r| a.wrapping_add(r.w16c)))
}

/// An entry's false starts in a rally.
pub fn false_starts(save: &Season, entry: usize, rally: usize) -> u8 {
    save.drivers.get(entry).and_then(|d| d.rallies.get(rally)).map_or(0, |r| r.false_starts)
}

/// Count one more false start.
pub fn inc_false_starts(save: &mut Season, entry: usize, rally: usize) {
    if let Some(r) = save.drivers.get_mut(entry).and_then(|d| d.rallies.get_mut(rally)) {
        r.false_starts = r.false_starts.wrapping_add(1);
    }
}

/// The current stage's start order.
pub fn start_order(save: &Season) -> [u32; DRIVERS] {
    save.w7ecc
}

/// Store the stage's start order, in the save and in the stage's slot.
pub fn set_start_order(save: &mut Season, order: [u32; DRIVERS]) {
    let pos = position(save);
    save.w7ecc = order;
    let at = pos.rally as usize * BLOCK + 4 + pos.stage as usize * SLOT + 0x1c;
    if let Some(b) = save.blocks.get_mut(at..at + 4 * DRIVERS) {
        for (k, w) in order.iter().enumerate() {
            b[4 * k..4 * k + 4].copy_from_slice(&w.to_le_bytes());
        }
    }
}

/// The rally's first-stage start order.
pub fn first_stage_order(save: &Season) -> [u32; DRIVERS] {
    save.w8e4c
}

/// Store the rally's first-stage start order.
pub fn set_first_stage_order(save: &mut Season, order: [u32; DRIVERS]) {
    save.w8e4c = order;
}

/// The team's entry ids in the season's team list.
pub fn team_ids(save: &Season) -> [u32; 7] {
    save.teams
}

/// A stage slot's start time from the forecast blocks.
pub fn forecast_slot_time(save: &Season, rally: usize, stage: usize) -> (u8, u8) {
    let slot = rally * BLOCK + 4 + stage * SLOT + 0xc;
    save.blocks.get(slot..slot + 2).map_or((0, 0), |b| (b[0], b[1]))
}

/// A rally's start time from the forecast blocks.
pub fn forecast_start_time(save: &Season, rally: usize) -> (u8, u8) {
    let at = rally * BLOCK + 0x208;
    save.blocks.get(at..at + 2).map_or((8, 0), |b| (b[0], b[1]))
}

/// A forecast block's rally id, if present.
pub fn forecast_rally_id(save: &Season, rally: usize) -> Option<u32> {
    save.blocks.get(rally * BLOCK..rally * BLOCK + 4).map(|b| u32::from_le_bytes(b.try_into().unwrap()))
}

/// Mark stages from `from` on as not driven when below 0.1: time -1, retired.
pub fn fill_neg(save: &mut Season, entry: usize, from: usize) {
    let pos = position(save);
    if let Some(r) = save.drivers.get_mut(entry).and_then(|d| d.rallies.get_mut(pos.rally as usize)) {
        for s in r.stages.iter_mut().skip(from).filter(|s| s.time < 0.1) {
            s.time = RETIRED;
            s.retired = 1;
        }
    }
}

/// Record an accident pair in every matching entry's stage result, four at most per entry.
pub fn push_pair(save: &mut Season, driver_id: i32, pair: [u32; 2]) -> bool {
    let pos = position(save);
    let mut wrote = false;
    for e in save.drivers.iter_mut().filter(|e| e.id == driver_id) {
        if let Some(r) = e.rallies.get_mut(pos.rally as usize).and_then(|r| r.stages.get_mut(pos.stage as usize)) {
            if r.b30[0] < 4 {
                r.elems[r.b30[0] as usize] = pair;
                r.b30[0] += 1;
                wrote = true;
            }
        }
    }
    wrote
}

/// Set the start time of day in a stage result.
pub fn set_stage_start_time(save: &mut Season, driver_id: i32, seconds: f32) {
    let pos = position(save);
    for e in save.drivers.iter_mut().filter(|e| e.id == driver_id) {
        if let Some(r) = e.rallies.get_mut(pos.rally as usize).and_then(|r| r.stages.get_mut(pos.stage as usize)) {
            r.f34 = seconds;
        }
    }
}

/// Mark the current stage retired for drivers out after the last one.
pub fn carry_retire_forward(save: &mut Season) {
    let pos = position(save);
    if pos.stage == 0 {
        return;
    }
    for e in save.drivers.iter_mut() {
        let Some(r) = e.rallies.get_mut(pos.rally as usize) else { continue };
        let Some(prev) = pos.stage.checked_sub(1) else { continue };
        let retired = r.stages.get(prev as usize).is_some_and(|s| s.time < 0.0 || s.retired != 0);
        if retired {
            if let Some(cur) = r.stages.get_mut(pos.stage as usize) {
                cur.retired = 1;
                cur.time = RETIRED;
            }
        }
    }
}

/// A stable bubble sort of the entries, swapping while `before(next, current)`.
pub fn bubble(mut order: [u32; DRIVERS], before: impl Fn(usize, usize) -> bool) -> [u32; DRIVERS] {
    loop {
        let mut sorted = true;
        for i in 0..DRIVERS - 1 {
            if before(order[i + 1] as usize, order[i] as usize) {
                order.swap(i, i + 1);
                sorted = false;
            }
        }
        if sorted {
            return order;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crt::CrtRand;
    use crate::race_options::ChampionshipOptions;

    fn nonzero() -> Season {
        let roster = rbr_formats::ai_ini::Roster { drivers: vec![], teams: vec![], cars: vec![] };
        let mut rng = CrtRand(1);
        let mut s = super::super::create(
            &roster,
            &[],
            ChampionshipOptions::default(),
            super::super::Player { name: "T", team: 0, rank: 0 },
            &[],
            &mut rng,
        );
        s.b04 = 0xdead_beef;
        s.state = [11, 0, 0, 9, 0xaaaa_aaaa, 8, 15, 0x1234_5678, 0, 0xbbbb_bbbb, 0xcccc_cccc, 0xdddd_dddd, 7, 1];
        for d in &mut s.drivers {
            d.w8d8 = 0xeeee_eeee;
            d.b8d0 = [d.b8d0[0], 0x11, 0x22, 0x33];
            for r in &mut d.rallies {
                r.pad = [0x44, 0x55, 0x66];
                for st in &mut r.stages {
                    st.b30 = [st.b30[0], 0x77, 0x88, 0x99];
                }
            }
        }
        s.globals = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13];
        s.snapshot = [21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34];
        s.teams = [6, 5, 4, 3, 2, 1, 0];
        s
    }

    #[test]
    fn gameplay_edits_leave_unknown_words_alone() {
        let mut s = nonzero();
        let before = s.clone();
        set_clock(&mut s, 9, 50);
        advance_clock(&mut s, 15);
        assert_eq!(clock(&s), SeasonClock { hour: 10, minute: 5 });
        set_head_time(&mut s, 0x0936);
        assert_eq!(head_time(&s), 0x0936);
        set_entry_rank(&mut s, 3, 42);
        assert_eq!(entry_rank(&s, 3), 42);
        add_rally_penalty(&mut s, 3, 0, 10.0);
        add_rally_points(&mut s, 3, 0, 6);
        inc_false_starts(&mut s, 3, 0);
        set_start_order(&mut s, ENTRIES);
        set_first_stage_order(&mut s, ENTRIES);
        let id3 = entry_id(&s, 3).unwrap_or(0);
        push_pair(&mut s, id3, [1, 2]);
        set_stage_start_time(&mut s, id3, 3600.0);
        advance_stage(&mut s);
        advance_rally(&mut s);
        rewind_stage(&mut s);
        fill_neg(&mut s, 3, 1);
        // Unknown words are untouched.
        assert_eq!(s.b04, before.b04);
        assert_eq!(
            [s.state[4], s.state[9], s.state[10], s.state[11], s.state[12]],
            [0xaaaa_aaaa, 0xbbbb_bbbb, 0xcccc_cccc, 0xdddd_dddd, 7]
        );
        assert!(s.drivers.iter().all(|d| d.w8d8 == 0xeeee_eeee));
        assert!(s.drivers.iter().all(|d| d.b8d0[1..] == [0x11, 0x22, 0x33]));
        assert!(s.drivers.iter().all(|d| d.rallies.iter().all(|r| r.pad == [0x44, 0x55, 0x66])));
        assert!(s
            .drivers
            .iter()
            .all(|d| d.rallies.iter().all(|r| r.stages.iter().all(|st| st.b30[1..] == [0x77, 0x88, 0x99]))));
        assert_eq!(s.globals, before.globals);
        assert_eq!(s.snapshot, before.snapshot);
        assert_eq!(s.teams, before.teams);
        // The named views read back what was written.
        assert_eq!(start_order(&s), ENTRIES);
        assert_eq!(first_stage_order(&s), ENTRIES);
        assert_eq!(entry_rank(&s, 3), 42);
    }

    #[test]
    fn duplicate_ids_each_take_the_pair() {
        // Two entries sharing one driver id stand in for a save with a
        // duplicated id. Every matching entry takes the pair, as the old
        // per-pair per-entry write did.
        let mut s = nonzero();
        s.drivers[2].id = 77;
        s.drivers[5].id = 77;
        assert!(push_pair(&mut s, 77, [9, 10]));
        for k in [2, 5] {
            let r = &s.drivers[k].rallies[0].stages[0];
            assert_eq!(r.b30[0], 1);
            assert_eq!(r.elems[0], [9, 10]);
        }
        // A full first entry does not block the second.
        s.drivers[2].rallies[0].stages[0].b30[0] = 4;
        assert!(push_pair(&mut s, 77, [11, 12]));
        assert_eq!(s.drivers[2].rallies[0].stages[0].b30[0], 4);
        let r = &s.drivers[5].rallies[0].stages[0];
        assert_eq!(r.b30[0], 2);
        assert_eq!(r.elems[1], [11, 12]);
    }

    #[test]
    fn start_order_points_wrap() {
        // Six rallies at u32::MAX fold with wrapping to u32::MAX - 5, still
        // ahead of a small total; a plain sum would trap on overflow here.
        let mut s = nonzero();
        advance_rally(&mut s);
        for r in 0..6 {
            add_rally_points(&mut s, 0, r, u32::MAX);
        }
        add_rally_points(&mut s, 1, 0, 5);
        let order = super::super::rally_start_order(&s);
        assert_eq!(order[0], 0);
        assert_eq!(order[1], 1);
    }

    #[test]
    fn clock_carries_once_like_the_game() {
        let mut s = nonzero();
        set_clock(&mut s, 9, 50);
        advance_clock(&mut s, 60);
        assert_eq!(clock(&s), SeasonClock { hour: 10, minute: 50 });
    }
}
