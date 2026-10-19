//! The service park between two stages of a rally: the repair part tree from the car's `repair.ini`, the repair
//! orders, the time budget and the late penalty. Pure; the damage it reads and repairs is [`CarriedDamage`].
//!
//! Traced (the asm, this task):
//! - **Budget** (0x5c70c0, the service page's set-up): 20 minutes for the season events of kind 5 and 7, 40 for kind 6,
//!   none (-1) otherwise; outside a season 20. In seconds (`* 60.0`) into the park (`*0x7d2af8`, `M5b0050`): +4 the time
//!   left, +0 the clock's rate 1.0, +8 set when the budget is above 20.1 s. With no damaged part the park closes at
//!   once, penalty 0 (0x5c7279).
//! - **Schedule** (`E80Daemon` 0x5d3800, after the stage results): after the rally's first stage (index 0) and its third
//!   (index 2) a kind-5 event (20 min); after index 3 a kind-6 one (40 min, `OSERVICE`, before the night: `Push10`
//!   0x5cf6f0 then sets the season's clock to 08:00); none after 1, 4 and 5. The rally's start (0x5d4c90) gives the
//!   player a kind-7 event (20 min) before the first stage. See [`service_before`].
//! - **Parts** (`M5b16f0` 0x5b16f0 and `M0dc0` 0x5b0dc0): each node keeps the file's unmount and mount times (+0x38,
//!   +0x3c) and draws its own from them with one CRT `rand()` each (+0x40, +0x44): `r = rand() / 32767 * 2`, then
//!   `time - r * min` for `r <= 1`, else `time + (r - 1) * max`. All four are then scaled by the crew ([`Crew::scales`]).
//! - **Severity** (`E73_M0320` 0x5b0320): `[0, 1, 3, 6, 10, 10][level] * importance`; the row's label is
//!   `D_FINE`..`D_LOST` by the level, at most 4 (0x5c60c8).
//! - **Estimate** (`SP_ESTIM`, `M5b14f0` over `Ma50` 0x5b0a50): until a part's step starts it counts the file's time,
//!   then moves to the drawn time with the step's progress ([`ServicePark::estimate`]).
//! - **Clock** (`CServiceParkTimerUI::Vf08` 0x5c5600, `M5b1640`): until the started orders are all done, the time left
//!   falls by the rate times the frame's seconds, and once the repairs have started the scaled time also advances them
//!   (`M5b12b0`): each order in the list's order is unmounted children first (`M6f0` 0x5b06f0), then mounted back with
//!   its children (`M820` 0x5b0820, `Mfbd0` 0x5afbd0), the time left over of one step going to the next. "Repair!"
//!   (`CSelectPartsUI::Vf04` 0x5c5920) sets the rate to 30.0 when there are orders; without, `MSG_NOREP` asks whether
//!   to leave ([`ServicePark::forfeit`]). Selecting during the repairs finishes every order at once and takes their
//!   time still to spend off the clock (`M5b11c0`, `M620`; [`ServicePark::finish_now`]).
//! - **End** (0x5c5600): the orders done, one second later the park closes (`M5b1220`: the repairs, `ResetAll`
//!   0x4f2bb0 with the refresh 0x4ec3b0) and `E80Daemon::Penalty` (0x5cf330) gives 10 s for every started minute past
//!   the budget, added to the rally's penalty (entry +0x170), with `MSG_REPAIRS` or `MSG_REPAIR_PEN`. With the budget
//!   gone before "Repair!", `MSG_SERVTIMEOUT`, then the park closes unrepaired with that penalty (0x5c5530).
//! - **Orders** (`CSelectPartsUI::Vf04` on the tree's list, 0x5b0520-0x5b1ae0): the rows are every damaged part at any
//!   depth; ordering a part covers its subtree (its ordered descendants leave the list, a covered part cannot be
//!   ordered again), taking an order off orders the damaged parts under it instead; the manager's advice is
//!   [`ServicePark::manager_advice`].
//! - **NGP** ([`Rules::Ngp`]): its patches to this code are the mechanics' index (0x5b1012, [`Crew::scales`]), the
//!   advice's compare (0x5b0c94) and its own wheel and tyre damage objects ([`car_damage::level`]). The part tree is
//!   the car's `repair.ini` in NGP's `physics.rbz` ([`repair_parts_in`]): the same parts as retail (NGP's car files
//!   declare 76 where retail's say 74; the loader reads 76 from both). NGP's physics keeps its damage in the same
//!   handler words, so the carried damage and the repairs need nothing more: its downforce reads the aero parts'
//!   +0x3db4 and +0x3dbc (ids 50 and 52), its idle torque the engine block's +0x3ab4..+0x3ae4 (id 23's floats), and its
//!   over-rev health is the engine block's +0x58 (handler +0x3b04, `*(engine+0x48)+0x58` with +0x48 set by `UpdateMrr`
//!   to handler +0x3aac): carried into a stage without a service, no part row shows it, and every service's refresh
//!   sets it back to 1.0.
//!
//! Ours: with the budget gone before "Repair!" the park closes as the time runs out (the game closes when the message
//! is confirmed; whether its clock runs on meanwhile is not known). `MSG_UNMOUNT` is never posted by 1.02's order code
//! and is not used.

use rbr_formats::repair::Part as RepairPart;

use crate::car_damage::{self, CarriedDamage};
use crate::crt::{crt_qsort, CrtRand};

mod tree;

use tree::{build, finish, mount_step, shown, subtree, unmount_step};
pub use tree::{draw_time, Node, Progress};

/// The service rules: 1.02's, or 1.02's as NGP patches them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Rules {
    #[default]
    V102,
    Ngp,
}

/// The kinds of season events with a service, and their budgets in minutes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceKind {
    /// Kind 7: before the rally's first stage, 20 min.
    Start,
    /// Kind 5: 20 min.
    Short,
    /// Kind 6: 40 min, the overnight service (`OSERVICE`).
    Long,
}

impl ServiceKind {
    /// The budget in seconds (0x5c70c0: minutes * 60).
    pub fn budget(self) -> f32 {
        match self {
            ServiceKind::Start | ServiceKind::Short => 20.0 * 60.0,
            ServiceKind::Long => 40.0 * 60.0,
        }
    }

    /// The page's title key: `NSERVICE` "Car Service", or `OSERVICE` "Overnight car service" for the long one.
    pub fn title_key(self) -> &'static str {
        match self {
            ServiceKind::Long => "OSERVICE",
            _ => "NSERVICE",
        }
    }
}

/// The service before stage `index` of a six-stage rally (0x5d3800 and 0x5d4c90).
pub fn service_before(index: usize) -> Option<ServiceKind> {
    match index {
        0 => Some(ServiceKind::Start),
        1 | 3 => Some(ServiceKind::Short),
        4 => Some(ServiceKind::Long),
        _ => None,
    }
}

/// The repair tree of car `car` (`[CarNN]`), its `repair.ini` in the retail `Root/physics.rbz`.
pub fn repair_parts(data: &rbr_formats::data::Data, car: usize) -> Result<Vec<RepairPart>, String> {
    let archive = data.resolve("Root/physics.rbz").ok_or("Root/physics.rbz: not found")?;
    repair_parts_in(&archive, car)
}

/// The repair tree of car `car` from the `physics.rbz` at `archive`: under NGP its own archive, whose slot folders hold
/// the installed NGP cars' `repair.ini` (the loader reads `%srepair.ini` in the car's physics folder, whatever archive
/// the game mounted).
pub fn repair_parts_in(archive: &std::path::Path, car: usize) -> Result<Vec<RepairPart>, String> {
    let folder = crate::setup::CAR_FOLDERS.get(car).ok_or(format!("car {car}: not a retail car"))?;
    let mut zip = rbr_formats::rbz::open(archive).map_err(|e| format!("{}: {e}", archive.display()))?;
    let name = format!("{folder}repair.ini");
    let mut text = String::new();
    std::io::Read::read_to_string(
        &mut rbr_formats::rbz::by_name(&mut zip, &name).map_err(|e| format!("{name}: {e}"))?,
        &mut text,
    )
    .map_err(|e| format!("{name}: {e}"))?;
    rbr_formats::repair::parse(&text).map_err(|e| format!("{name}: {e}"))
}

/// The late penalty (`E80Daemon::Penalty` 0x5cf330): 10 s for every started 60 s past the budget, as the asm's loop.
pub fn late_penalty(time_left: f32) -> f32 {
    let mut over = -time_left;
    let mut penalty = 0.0f32;
    while over > 0.0 {
        penalty += 10.0;
        over -= 60.0;
    }
    penalty
}

/// The row's damage label (`D_FINE`, `D_LIGHT`, `D_MED`, `D_SEVERE`, `D_LOST`): the level, at most 4 (0x5c60cd).
pub fn damage_key(level: u8) -> &'static str {
    ["D_FINE", "D_LIGHT", "D_MED", "D_SEVERE", "D_LOST"][usize::from(level.min(4))]
}

/// The player's team's crew: its engineering skill (`team+0x94`) and number of mechanics (`team+0x90`). The default
/// (no season team) is ours.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Crew {
    pub a: i32,
    pub b: i32,
}

impl Default for Crew {
    fn default() -> Self {
        Crew { a: 3, b: 6 }
    }
}

impl Crew {
    /// The two scales every repair time takes (0x5b0f70-0x5b103e): `[1.8, 1.65, 1.5, 1.35, 1.2][a - 1]` and `[0.525,
    /// 0.375, 0.3, 0.225, 0.15, 0.075][b - 4]` (NGP: `b - 1`, its patch at 0x5b1012), each index capped above (4 and 5)
    /// but not below. The tables are neighbours on the stack, so a negative mechanics index reads the first table's end
    /// (`B[-k] = A[5 - k]`); further down, and a skill index below 0, are other locals: clamped here (ours).
    pub fn scales(self, rules: Rules) -> (f32, f32) {
        const T: [f32; 11] = [1.8, 1.65, 1.5, 1.35, 1.2, 0.525, 0.375, 0.3, 0.225, 0.15, 0.075];
        let a = (self.a - 1).clamp(0, 4);
        let b = match rules {
            Rules::V102 => self.b - 4,
            Rules::Ngp => self.b - 1,
        }
        .min(5);
        (T[a as usize], T[(5 + b).max(0) as usize])
    }
}

/// How the park closed, for its message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum End {
    /// No damaged part: closed on opening, no message.
    NothingToRepair,
    /// The orders done (`MSG_REPAIRS`, or `MSG_REPAIR_PEN` with a penalty).
    Finished,
    /// The budget gone before "Repair!" (`MSG_SERVTIMEOUT`).
    TimedOut,
    /// Left without orders (after `MSG_NOREP`).
    Forfeited,
}

/// The park's state.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ParkState {
    /// The player picks the orders; the clock runs at 1.0.
    Choosing,
    /// "Repair!": the orders run at 30x.
    Repairing,
    /// Closed, with the late penalty.
    Closed { penalty: f32, end: End },
}

/// The message on closing, a string-table key, and the penalty to put into it (`%1.f`) for `MSG_REPAIR_PEN`.
pub fn end_message(end: End, penalty: f32) -> Option<&'static str> {
    match end {
        End::NothingToRepair | End::Forfeited => None,
        End::Finished if penalty > 0.0 => Some("MSG_REPAIR_PEN"),
        End::Finished => Some("MSG_REPAIRS"),
        End::TimedOut => Some("MSG_SERVTIMEOUT"),
    }
}

/// A service park visit.
#[derive(Debug, Clone, PartialEq)]
pub struct ServicePark {
    pub kind: ServiceKind,
    pub rules: Rules,
    /// Every part, in the file's order.
    pub tree: Vec<Node>,
    /// Indices into `tree` of the ordered parts, in order (the list at +0x8).
    pub orders: Vec<usize>,
    pub time_left: f32,
    rate: f32,
    pub state: ParkState,
    /// The one-second hold after the orders are done (`CServiceParkTimerUI` +4).
    hold: f32,
    pub damage: CarriedDamage,
}

/// The clock's rate during the repairs (`0x72f42c`).
pub const REPAIR_RATE: f32 = 30.0;
/// `M5b19c0`'s bound: the manager drops only orders scoring below this.
const ADVICE_KEEP: u32 = 0x50;

impl ServicePark {
    /// Opens the park on the car's repair tree with the damage carried in: the drawn times, the refresh (0x4ec3b0) and
    /// the rows' count (`M5b0520`, whose `GetDamage` calls clear 1.02's wheels' floats, [`car_damage::get_damage`]).
    pub fn open(
        kind: ServiceKind,
        parts: &[RepairPart],
        crew: Crew,
        rules: Rules,
        rand: &mut CrtRand,
        damage: CarriedDamage,
    ) -> Self {
        let mut tree = Vec::new();
        build(parts, None, crew.scales(rules), rand, &mut tree);
        let mut damage = damage;
        damage.service_refresh();
        let mut rows = 0;
        for n in &tree {
            if car_damage::get_damage_in(&mut damage, n.id, rules) > 0 {
                rows += 1;
            }
        }
        let mut park = Self {
            kind,
            rules,
            tree,
            orders: Vec::new(),
            time_left: kind.budget(),
            rate: 1.0,
            state: ParkState::Choosing,
            hold: 1.0,
            damage,
        };
        if rows == 0 {
            park.close(End::NothingToRepair, 0.0);
        }
        park
    }

    /// A part's damage level (`GetDamage`).
    pub fn level(&self, i: usize) -> u8 {
        car_damage::level(&self.damage, self.tree[i].id, self.rules)
    }

    /// `E73_M0320` (0x5b0320): `[0, 1, 3, 6, 10, 10][level] * importance`.
    pub fn score(&self, i: usize) -> i32 {
        const TABLE: [i32; 6] = [0, 1, 3, 6, 10, 10];
        TABLE[self.level(i) as usize] * self.tree[i].importance
    }

    /// The list's rows (`M5b0520`, `M5b05b0`): every part whose own level is not 0, at any depth, in the file's order.
    pub fn rows(&self) -> Vec<usize> {
        (0..self.tree.len()).filter(|&i| self.level(i) > 0).collect()
    }

    /// The order a part is in (`M5b09e0`): the part itself or an ordered ancestor, nearest first.
    pub fn covering(&self, i: usize) -> Option<usize> {
        let mut p = Some(i);
        while let Some(k) = p {
            if self.orders.contains(&k) {
                return Some(k);
            }
            p = self.tree[k].parent;
        }
        None
    }

    fn is_ancestor(&self, a: usize, mut i: usize) -> bool {
        while let Some(p) = self.tree[i].parent {
            if p == a {
                return true;
            }
            i = p;
        }
        false
    }

    /// `M5b13d0` (0x5b13d0): orders a part not yet covered; its ordered descendants leave the list (the part covers
    /// them).
    fn add(&mut self, i: usize) {
        if self.state != ParkState::Choosing || self.covering(i).is_some() {
            return;
        }
        self.orders.push(i);
        let orders = std::mem::take(&mut self.orders);
        self.orders = orders.into_iter().filter(|&o| o == i || !self.is_ancestor(i, o)).collect();
    }

    /// `M5b1460` (0x5b1460): orders a damaged part, else looks for damaged parts below it.
    fn add_damaged(&mut self, i: usize) {
        if self.level(i) != 0 {
            self.add(i);
            return;
        }
        for c in self.tree[i].children.clone() {
            self.add_damaged(c);
        }
    }

    /// `M5b1900` (0x5b1900): takes the order covering the part off the list, then orders the damaged parts under it
    /// (each child if damaged, else its damaged descendants).
    fn remove(&mut self, i: usize) {
        if self.state != ParkState::Choosing {
            return;
        }
        let Some(order) = self.covering(i) else { return };
        self.orders.retain(|&o| o != order);
        for c in self.tree[order].children.clone() {
            self.add_damaged(c);
        }
    }

    /// Selecting a row (`CSelectPartsUI::Vf04` 0x5c5920): a covered part is taken off, another ordered, both by its
    /// physics id (`M5b1ae0`, `M5b18c0`: the first node with that id).
    pub fn toggle(&mut self, part: usize) {
        if self.state != ParkState::Choosing || part >= self.tree.len() {
            return;
        }
        let id = self.tree[part].id;
        let first = self.tree.iter().position(|n| n.id == id).unwrap_or(part);
        if self.covering(part).is_some() {
            self.remove(first);
        } else {
            self.add(first);
        }
    }

    /// The manager's advice (`SP_MANADV`, `M5b19c0` 0x5b19c0): the list cleared, the rows sorted (`qsort` with
    /// `0x5b0c90`) and ordered in that order, then, while the estimate is above the time left, the rows still in the
    /// list by their id that score below 80 are taken off, in the sorted order.
    ///
    /// In 1.02 the compare hands `0x5b0320` the array's slots for nodes, so it scores the next slot's pointer as an id
    /// (no part: 0) and every compare is 0: the sort only permutes (`shortsort`'s swaps) up to 8 rows and keeps more.
    /// The last slot's neighbour is taken as 0 too (ours: a stack word). NGP's patch (0x5b0c94) sorts by the score,
    /// lowest first.
    pub fn manager_advice(&mut self) {
        if self.state != ParkState::Choosing {
            return;
        }
        self.orders.clear();
        let mut rows = self.rows();
        match self.rules {
            Rules::V102 => crt_qsort(&mut rows, |_, _| 0),
            Rules::Ngp => crt_qsort(&mut rows, |a, b| {
                let (a, b) = (self.score(a) as u32, self.score(b) as u32);
                if b > a {
                    -1
                } else {
                    i32::from(b < a)
                }
            }),
        }
        for &i in &rows {
            self.add(i);
        }
        loop {
            let mut dropped = false;
            for &i in &rows {
                if self.time_left >= self.estimate() {
                    return;
                }
                let id = self.tree[i].id;
                if self.covering(i).is_some()
                    && (self.score(i) as u32) < ADVICE_KEEP
                    && self.orders.iter().any(|&o| self.tree[o].id == id)
                {
                    self.remove(i);
                    dropped = true;
                }
            }
            if !dropped {
                return;
            }
        }
    }

    /// The estimate (`SP_ESTIM`, `M5b14f0`): the orders' shown times, in the list's order.
    pub fn estimate(&self) -> f32 {
        self.orders.iter().fold(0.0, |t, &i| t + shown(&self.tree, i))
    }

    /// A part's subtree shown time.
    pub fn time_of(&self, i: usize) -> f32 {
        shown(&self.tree, i)
    }

    /// "Repair!" with orders: the clock runs at 30x and the repairs start. Without orders nothing happens; the game asks
    /// with `MSG_NOREP` whether to leave ([`Self::forfeit`]).
    pub fn start(&mut self) -> bool {
        if self.state != ParkState::Choosing || self.orders.is_empty() {
            return false;
        }
        self.state = ParkState::Repairing;
        self.rate = REPAIR_RATE;
        true
    }

    /// `MSG_NOREP` confirmed (0x5c5530): the park closes unrepaired, with the time over budget's penalty.
    pub fn forfeit(&mut self) {
        if self.state == ParkState::Choosing {
            self.close(End::Forfeited, self.time_left);
        }
    }

    /// Selecting during the repairs (0x5c5920): every order finished at once, its drawn time still to spend taken off
    /// the clock (`M5b11c0`).
    pub fn finish_now(&mut self) {
        if self.state != ParkState::Repairing {
            return;
        }
        let left = self.orders.clone().into_iter().fold(0.0f32, |t, o| t + finish(&mut self.tree, o));
        self.time_left -= left;
    }

    /// `M5b1220` and the penalty: the finished parts repaired, children first (0x5b03a0: `ResetDamage` in state 3), the
    /// refresh, and `Penalty(-time_left)`.
    fn close(&mut self, end: End, time_left: f32) {
        fn apply(park: &mut ServicePark, i: usize) {
            for c in park.tree[i].children.clone() {
                apply(park, c);
            }
            if park.tree[i].progress == Progress::Done {
                car_damage::reset_damage(&mut park.damage, park.tree[i].id, park.rules);
            }
        }
        for o in self.orders.clone() {
            apply(self, o);
        }
        self.damage.service_refresh();
        self.state = ParkState::Closed { penalty: late_penalty(time_left), end };
    }

    /// `M5b1360`: started and every order's subtree done.
    fn orders_done(&self) -> bool {
        self.state == ParkState::Repairing
            && self
                .orders
                .iter()
                .all(|&i| subtree(&self.tree, i).iter().all(|&k| self.tree[k].progress == Progress::Done))
    }

    /// One frame of `dt` real seconds (`CServiceParkTimerUI::Vf08` 0x5c5600): the clock (`M5b1640`) and the repairs
    /// (`M5b12b0`), then the time-out before "Repair!", or the close one second after the orders are done.
    pub fn tick(&mut self, dt: f32) {
        if matches!(self.state, ParkState::Closed { .. }) {
            return;
        }
        let started = self.state == ParkState::Repairing;
        if !self.orders_done() {
            let mut scaled = self.rate * dt;
            self.time_left -= scaled;
            if started {
                for o in self.orders.clone() {
                    if !unmount_step(&mut self.tree, o, &mut scaled) || !mount_step(&mut self.tree, o, &mut scaled) {
                        break;
                    }
                }
            }
        }
        if self.time_left < 0.0 && !started {
            self.close(End::TimedOut, self.time_left);
        } else if self.orders_done() {
            self.hold -= dt;
            if self.hold <= 0.0 {
                self.close(End::Finished, self.time_left);
            }
        } else {
            self.hold = 1.0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rbr_formats::repair::Timing;

    fn part(id: i32, unmount: f32, mount: f32, children: Vec<RepairPart>) -> RepairPart {
        let t = |time| Timing { time, min_variance: 0.0, max_variance: 0.0 };
        RepairPart {
            physics_id: id,
            importance: 10,
            name: format!("p{id}"),
            unmount: t(unmount),
            mount: t(mount),
            children,
        }
    }

    fn open(parts: &[RepairPart], crew: Crew, rules: Rules) -> ServicePark {
        ServicePark::open(ServiceKind::Short, parts, crew, rules, &mut CrtRand(1), CarriedDamage::default())
    }

    #[test]
    fn penalty_is_ten_seconds_per_started_minute_late() {
        assert_eq!(late_penalty(5.0), 0.0);
        assert_eq!(late_penalty(0.0), 0.0);
        assert_eq!(late_penalty(-1.0), 10.0);
        assert_eq!(late_penalty(-60.0), 10.0);
        assert_eq!(late_penalty(-60.5), 20.0);
        assert_eq!(late_penalty(-300.0), 50.0);
    }

    #[test]
    fn schedule_and_budgets() {
        let services: Vec<_> = (0..6).map(service_before).collect();
        use ServiceKind::*;
        assert_eq!(services, [Some(Start), Some(Short), None, Some(Short), Some(Long), None]);
        assert_eq!(Long.budget(), 2400.0);
        assert_eq!(Short.budget(), 1200.0);
        assert_eq!((Long.title_key(), Short.title_key()), ("OSERVICE", "NSERVICE"));
        assert_eq!([0, 1, 4, 5].map(damage_key), ["D_FINE", "D_LIGHT", "D_LOST", "D_LOST"]);
    }

    #[test]
    fn crew_scales_and_draws() {
        assert_eq!(Crew { a: 5, b: 9 }.scales(Rules::V102), (1.2, 0.075));
        assert_eq!(Crew { a: 1, b: 4 }.scales(Rules::V102), (1.8, 0.525));
        assert_eq!(Crew { a: 9, b: 20 }.scales(Rules::V102), (1.2, 0.075));
        // Three mechanics: index -1 reads the skill table's last entry.
        assert_eq!(Crew { a: 3, b: 3 }.scales(Rules::V102), (1.5, 1.2));
        // NGP counts the mechanics from 1.
        assert_eq!(Crew { a: 3, b: 1 }.scales(Rules::Ngp), (1.5, 0.525));
        assert_eq!(Crew { a: 3, b: 6 }.scales(Rules::Ngp), (1.5, 0.075));
        assert_eq!(draw_time(100.0, 10.0, 20.0, 0), 100.0);
        // rand 32767: r = 2 (to f32 precision), the time plus one max.
        assert!((draw_time(100.0, 10.0, 20.0, 32767) - 120.0).abs() < 1e-3);
        let mut r = CrtRand(1);
        // MSVC's first rand() with seed 1 is 41.
        assert_eq!(r.draw(), 41);
    }

    #[test]
    fn times_scale_in_the_asms_order_and_the_estimate_shows_the_files_until_done() {
        let mut p = part(31, 73.3, 41.7, vec![]);
        p.unmount.min_variance = 30.0;
        p.mount.max_variance = 30.0;
        let crew = Crew { a: 2, b: 7 };
        let (a, b) = crew.scales(Rules::V102);
        let mut park = open(&[p], crew, Rules::V102);
        let n = park.tree[0].clone();
        assert_eq!((n.base_unmount, n.base_mount), (a * 73.3 * b, a * b * 41.7));
        assert_ne!(n.unmount, n.base_unmount);
        park.toggle(0);
        assert_eq!(park.estimate(), n.base_unmount + n.base_mount);
        park.start();
        while park.state == ParkState::Repairing {
            park.tick(0.1);
        }
        assert_eq!(park.estimate(), n.unmount + n.mount);
    }

    #[test]
    fn a_repair_runs_at_thirty_times_and_repairs_when_mounted() {
        // Gearbox actuator (31) flag clear: level 4. Part times 30 + 60 s.
        let parts = [part(31, 30.0, 60.0, vec![])];
        let crew = Crew { a: 5, b: 9 };
        let (a, b) = crew.scales(Rules::V102);
        let mut park = open(&parts, crew, Rules::V102);
        assert_eq!(park.rows(), [0]);
        assert!(!park.start());
        park.toggle(0);
        assert_eq!(park.estimate(), a * 30.0 * b + a * b * 60.0);
        assert!(park.start());
        // One real second spends 30 s of budget.
        park.tick(1.0);
        assert!((park.time_left - 1170.0).abs() < 1e-3);
        while park.state == ParkState::Repairing {
            park.tick(0.1);
        }
        assert_eq!(park.state, ParkState::Closed { penalty: 0.0, end: End::Finished });
        assert_eq!(end_message(End::Finished, 0.0), Some("MSG_REPAIRS"));
        assert_eq!(car_damage::severity(&park.damage, 31), 0);
    }

    #[test]
    fn running_late_costs_a_penalty_and_finish_now_takes_the_rest_off_the_clock() {
        let parts = [part(31, 600.0, 600.0, vec![part(37, 600.0, 600.0, vec![])])];
        let mut park = open(&parts, Crew { a: 1, b: 4 }, Rules::V102);
        park.toggle(0);
        park.start();
        while park.state == ParkState::Repairing {
            park.tick(1.0);
        }
        let ParkState::Closed { penalty, end } = park.state else { panic!() };
        // 2400 s * 0.945 = 2268 s of work on a 1200 s budget: 1068 s over, 18 started minutes.
        assert_eq!((penalty, end), (180.0, End::Finished));
        assert_eq!(end_message(end, penalty), Some("MSG_REPAIR_PEN"));
        assert_eq!(car_damage::severity(&park.damage, 31), 0);
        assert_eq!(car_damage::severity(&park.damage, 37), 0);

        let mut park = open(&parts, Crew { a: 1, b: 4 }, Rules::V102);
        park.toggle(0);
        park.start();
        park.tick(1.0);
        let work = 4.0 * 600.0 * 1.8 * 0.525;
        park.finish_now();
        assert!((park.time_left - (1200.0 - work)).abs() < 1e-2);
        park.tick(0.5);
        assert_eq!(park.state, ParkState::Repairing);
        park.tick(0.5);
        assert_eq!(park.state, ParkState::Closed { penalty: 180.0, end: End::Finished });
    }

    #[test]
    fn idling_past_the_budget_times_out_and_leaving_forfeits() {
        let parts = [part(31, 10.0, 10.0, vec![])];
        let mut park = open(&parts, Crew::default(), Rules::V102);
        park.toggle(0);
        park.tick(1200.5);
        assert_eq!(park.state, ParkState::Closed { penalty: 10.0, end: End::TimedOut });
        assert_eq!(car_damage::severity(&park.damage, 31), 4);
        let mut park = open(&parts, Crew::default(), Rules::V102);
        park.forfeit();
        assert_eq!(park.state, ParkState::Closed { penalty: 0.0, end: End::Forfeited });
        // A sound car: closed on opening.
        let mut d = CarriedDamage::default();
        car_damage::repair(&mut d, 31);
        let park = ServicePark::open(ServiceKind::Short, &parts, Crew::default(), Rules::V102, &mut CrtRand(1), d);
        assert_eq!(park.state, ParkState::Closed { penalty: 0.0, end: End::NothingToRepair });
    }

    #[test]
    fn rows_are_damaged_parts_at_any_depth_and_orders_cover_subtrees() {
        let parts = [part(30, 10.0, 10.0, vec![part(31, 10.0, 10.0, vec![]), part(37, 10.0, 10.0, vec![])])];
        let mut d = CarriedDamage::default();
        // The gearbox (30) is sound: its flag set and its wear 1.0 at level 1.0.
        car_damage::repair(&mut d, 30);
        d.set(0x3AA0, 1.0f32.to_bits());
        d.set(0x3A90, 0);
        let mut park = ServicePark::open(ServiceKind::Short, &parts, Crew::default(), Rules::V102, &mut CrtRand(1), d);
        assert_eq!(park.tree.iter().map(|n| n.id).collect::<Vec<_>>(), [30, 31, 37]);
        assert_eq!(park.rows(), [1, 2]);
        park.toggle(1);
        park.toggle(2);
        assert_eq!(park.orders, [1, 2]);
        // Ordering the parent covers both children: they leave the list and count as ordered.
        park.add(0);
        assert_eq!(park.orders, [0]);
        assert_eq!(park.covering(2), Some(0));
        // A covered child is not ordered twice; selecting it takes the parent's order off and orders the damaged
        // children instead.
        park.add(2);
        assert_eq!(park.orders, [0]);
        park.toggle(2);
        assert_eq!(park.orders, [1, 2]);
        park.toggle(2);
        assert_eq!(park.orders, [1]);
    }

    #[test]
    fn manager_advice_orders_then_drops_cheap_parts_that_do_not_fit() {
        let mut parts = [part(31, 3000.0, 3000.0, vec![]), part(37, 10.0, 10.0, vec![])];
        parts[0].importance = 1; // score 10 < 80: may be dropped
        parts[1].importance = 10; // score 100: kept
        let mut park = open(&parts, Crew { a: 1, b: 4 }, Rules::V102);
        park.manager_advice();
        assert_eq!(park.orders, [1]);
    }

    #[test]
    fn manager_advice_sorts_nothing_in_102_and_by_score_under_ngp() {
        let ids = [31, 33, 35, 36, 37, 11, 40, 41, 47];
        let mut parts: Vec<_> = ids.iter().map(|&id| part(id, 1.0, 1.0, vec![])).collect();
        for (k, p) in parts.iter_mut().enumerate().take(3) {
            p.importance = [1, 3, 2][k];
        }
        // Three rows: 1.02's zero compares leave shortsort's swaps.
        let mut park = open(&parts[..3], Crew::default(), Rules::V102);
        park.manager_advice();
        assert_eq!(park.orders, [1, 2, 0]);
        // Nine rows: the partition keeps the order.
        let mut park = open(&parts, Crew::default(), Rules::V102);
        park.manager_advice();
        assert_eq!(park.orders, (0..9).collect::<Vec<_>>());
        // NGP: the lowest score first (10, 20, 30).
        let mut park = open(&parts[..3], Crew::default(), Rules::Ngp);
        park.manager_advice();
        assert_eq!(park.orders, [0, 2, 1]);
    }

    #[test]
    fn ngp_reads_its_own_wheels_and_the_refresh_clears_its_tyres() {
        let mut d = CarriedDamage::default();
        for k in 0..8 {
            d.set(0x3C18 + 4 * k, 0.9f32.to_bits());
            d.set(0x3D04 + 4 * k, 0.5f32.to_bits());
        }
        assert_eq!(car_damage::level(&d, 105, Rules::Ngp), 4);
        assert_eq!(car_damage::level(&d, 106, Rules::Ngp), 2);
        assert_eq!(car_damage::level(&d, 106, Rules::V102), 0);
        let mut e = d.clone();
        car_damage::reset_damage(&mut e, 106, Rules::Ngp);
        assert_eq!(car_damage::level(&e, 106, Rules::Ngp), 0);
        let parts = [part(105, 0.0, 0.0, vec![]), part(106, 0.0, 0.0, vec![])];
        let park =
            ServicePark::open(ServiceKind::Short, &parts, Crew::default(), Rules::Ngp, &mut CrtRand(1), d.clone());
        // The wheel's floats stay (NGP's read clears nothing), the tyre's are gone with the refresh.
        assert_eq!(park.rows(), [0]);
        let park = ServicePark::open(ServiceKind::Short, &parts, Crew::default(), Rules::V102, &mut CrtRand(1), d);
        assert!(matches!(park.state, ParkState::Closed { end: End::NothingToRepair, .. }));
    }

    /// Under NGP the tree is read from NGP's `physics.rbz` (`RBR_NGP_PHYSICS`): for the season's cars NGP has physics
    /// for (cars 4, 5, 6) the same part ids as retail. NGP's car files declare 76 parts where retail's say 74, and the
    /// loader reads 76 from both (the last top-level block whole).
    #[test]
    fn ngp_trees_have_the_retail_parts() {
        let (Some(data), Some(ngp)) = (rbr_formats::data::Data::from_env(), std::env::var_os("RBR_NGP_PHYSICS")) else {
            return;
        };
        fn ids(parts: &[RepairPart], out: &mut Vec<i32>) {
            for p in parts {
                out.push(p.physics_id);
                ids(&p.children, out);
            }
        }
        for car in [4, 5, 6] {
            let (mut a, mut b) = (Vec::new(), Vec::new());
            ids(&repair_parts(&data, car).unwrap(), &mut a);
            ids(&repair_parts_in(std::path::Path::new(&ngp), car).unwrap(), &mut b);
            assert_eq!(a, b, "car {car}");
        }
    }

    /// The Corolla's retail tree opens a park with a drawn time on every part but the wheels (`RBR_DATA`).
    #[test]
    fn retail_repair_tree() {
        let Some(data) = rbr_formats::data::Data::from_env() else { return };
        let parts = repair_parts(&data, 6).unwrap();
        assert_eq!(rbr_formats::repair::node_count(&parts), 76);
        let park = open(&parts, Crew::default(), Rules::V102);
        // The wheels and tyres take no time (the file's times are 0).
        assert_eq!(park.tree.len(), 76);
        let free: Vec<_> = (0..park.tree.len()).filter(|&i| park.time_of(i) <= 0.0).collect();
        assert!(free
            .iter()
            .all(|&i| park.tree[i].name.starts_with("wheel_") || park.tree[i].name.starts_with("tyre_")));
        assert_eq!(free.len(), 8);
    }
}
