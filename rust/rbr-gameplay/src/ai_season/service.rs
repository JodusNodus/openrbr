//! An AI driver's service (`E78HostCC::Ev09f80` 0x5c9f80, [`service`]): the manager's order through the temperament
//! roll (`SetA0Rnd`), the repairs at the team's cost (`Ev9ec0`) within the minutes and the budget, and the late
//! penalty.

use rbr_formats::ai_ini::{Master, Team};

use super::{ftol, Bulletin, CarState, DriverState, RunLog, INV_RAND_MAX};
use crate::crt::CrtRand;

/// A team's money (`E78Entry`): +0x80 the budget and +0x8c the manager style, from `aidrv.ini` and changed by the
/// rally's result (`Dmg` 0x5d6df0); the repair spending, +0x84 the current service's and +0x88 the rally's
/// (`Add8488` 0x5d6a00 adds to both).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TeamCosts {
    pub budget: u32,
    pub service: u32,
    pub rally: u32,
    pub style: u32,
}

impl TeamCosts {
    /// The team as `aidrv.ini` loads it.
    pub fn new(team: &Team) -> Self {
        TeamCosts { budget: team.budget as u32, service: 0, rally: 0, style: team.manager_style as u32 }
    }

    /// `Add8488`: returns the service's cost so far.
    fn add(&mut self, cost: u32) -> u32 {
        self.rally = self.rally.wrapping_add(cost);
        self.service = self.service.wrapping_add(cost);
        self.service
    }
}

/// `SetA0Rnd` (0x5cd4b0): the driver takes temperament `v` unless the current one resists (`ai.ini`'s resistance of
/// the current temperament to `v`, `Get274` 0x5c9190, against `rand() / 32767`). One `rand()`.
pub fn set_temperament(m: &Master, d: &mut DriverState, v: u32, rng: &mut CrtRand) {
    // Ours: a temperament or order outside `ai.ini`'s 5 x 5 table resists with 0 (the original reads past it).
    let resist = m
        .temperaments
        .get(d.driver.temperament as usize)
        .and_then(|t| t.mood_resistance.get(v as usize))
        .copied()
        .unwrap_or(0.0);
    let r = f64::from(rng.draw()) * f64::from(INV_RAND_MAX);
    // `fcomi`'s `ja`: the change when not above, a NaN included.
    if f64::from(resist).partial_cmp(&r) != Some(std::cmp::Ordering::Greater) {
        d.driver.temperament = v as i32;
    }
}

/// `Ev9ec0` (0x5c9ec0): a part's repair cost and minutes for the team: `ai.ini`'s repair value less the engineering
/// percent, its repair time less engineering x mechanics percent (`(100 - x) * 0.01 * v`, truncated).
pub fn repair_cost(m: &Master, team: &Team, part: usize) -> (u32, u32) {
    let pct = |x: u32| f64::from(100u32.wrapping_sub(x)) * f64::from(0.01f32);
    let eng = team.engineering as u32;
    let cost = ftol(pct(eng) * f64::from(m.repair_value[part] as u32));
    let time = ftol(pct(eng.wrapping_mul(team.mechanics as u32)) * f64::from(m.repair_time[part] as u32));
    (cost, time)
}

/// `rand() % n` as the float `x * 0.01 + base` (SSE single precision).
fn roll(rng: &mut CrtRand, n: i32, base: f32) -> f32 {
    (rng.draw() % n) as f32 * 0.01 + base
}

/// `Ev09f80` (0x5c9f80): an AI driver's service. The manager's order first (by the team's manager style, the
/// driver's last stage against the par and the car's damage; each order goes through [`set_temperament`] and is
/// posted), then the repairs: broken parts (4) first, then the tyres, then the cheapest of the rest (state plus the
/// technique, or the speed for parts 3-5; a light part (1) counts 20 more) until the service's minutes (20, or 40 when
/// `long`) or, for style 3, the budget's share run out. Returns the minutes used. Every `rand()` in the asm's order.
#[allow(clippy::too_many_arguments)]
pub fn service(
    m: &Master,
    id: i32,
    d: &mut DriverState,
    car: &mut CarState,
    team: &Team,
    costs: &mut TeamCosts,
    long: bool,
    rng: &mut CrtRand,
    log: &mut RunLog,
) -> u32 {
    let vs = d.vs_par;
    let mut take = false;
    let mut limit = 1_000_000u32;
    costs.service = 0;
    let damage = car.parts.iter().filter(|&&p| p != 0).fold(0u32, |a, &p| a.wrapping_add(p.wrapping_mul(10)));
    let mut order = |v: u32, d: &mut DriverState, rng: &mut CrtRand| {
        set_temperament(m, d, v, rng);
        log.bulletins.push(Bulletin::ManagerOrder { driver: id, order: v as i32 });
    };
    match costs.style {
        0 => {
            let mut r3 = 0;
            if vs >= roll(rng, 150, 0.5) {
                order(1, d, rng);
            } else if -roll(rng, 1500, 5.0) >= vs {
                r3 = rng.draw() % 100;
                order(if r3 < 50 { 0 } else { 3 }, d, rng);
            }
            if damage >= 100 {
                order(1, d, rng);
            } else if damage == 0 {
                order(if r3 < 50 { 0 } else { 3 }, d, rng);
            }
        }
        1 => {
            let v = if vs >= roll(rng, 100, 2.0) {
                0
            } else if -roll(rng, 500, 5.0) >= vs {
                take = true;
                1
            } else {
                2
            };
            order(if damage < 100 && v != 1 { 0 } else { 1 }, d, rng);
        }
        2 => {
            if vs.partial_cmp(&roll(rng, 900, 1.0)).is_none_or(|o| o.is_lt()) {
                rng.draw();
            }
            take = true;
            if rng.draw() % 100 < 50 {
                let v = (rng.draw() % 5) as u32;
                order(v, d, rng);
            }
        }
        3 => {
            let v = if vs >= roll(rng, 300, 2.0) {
                0
            } else if -roll(rng, 500, 5.0) >= vs {
                1
            } else {
                2
            };
            limit = costs.budget.wrapping_mul(if long { 40 } else { 30 }) / 100;
            if damage >= 100 || v == 1 {
                order(1, d, rng);
            }
        }
        4 | 5 => {
            let first = if costs.style == 4 { roll(rng, 300, 2.0) } else { roll(rng, 200, 1.0) };
            if vs >= first {
                order(0, d, rng);
            } else {
                let second = if costs.style == 4 { roll(rng, 500, 5.0) } else { roll(rng, 400, 1.0) };
                if -second >= vs {
                    order(1, d, rng);
                }
            }
            if damage >= 30 {
                order(1, d, rng);
            } else if damage == 0 {
                order(0, d, rng);
            }
        }
        _ => {}
    }
    let minutes = if long { 40 } else { 20 };
    let mut used = 0u32;
    let mut spent = 0u32;
    // One repair if it fits; true when the service is over.
    let mut fix = |part: usize, car: &mut CarState, used: &mut u32, spent: &mut u32| {
        let (cost, time) = repair_cost(m, team, part);
        let t = time.wrapping_add(*used);
        if (t <= minutes || !take) && cost.wrapping_add(*spent) <= limit {
            *spent = costs.add(cost);
            *used = t;
            car.parts[part] = 0;
            car.hits[part] = 0;
        }
        (*used >= minutes && take) || *spent >= limit
    };
    for part in 0..8 {
        if car.parts[part] >= 4 && fix(part, car, &mut used, &mut spent) {
            return used;
        }
    }
    if car.parts[0] != 0 && fix(0, car, &mut used, &mut spent) {
        return used;
    }
    let mut score: [u32; 8] = std::array::from_fn(|p| match car.parts[p] {
        0 => 100,
        a if (3..=5).contains(&p) => a.wrapping_add(d.driver.speed as u32),
        a => a.wrapping_add(d.technique()),
    });
    loop {
        let mut best = 8;
        let mut least = 100;
        for (p, &v) in score.iter().enumerate() {
            if v < least {
                least = v;
                best = p;
                if car.parts[p] == 1 {
                    least += 20;
                }
            }
        }
        if best == 8 || fix(best, car, &mut used, &mut spent) {
            return used;
        }
        score[best] = 100;
    }
}

/// The service's late penalty in the stage loop (0x5d4c90 cases 5 and 6, `E80Daemon::Penalty` 0x5cf330): the
/// minutes used past the service's 20 (or 40), 10 s for each started minute. A penalty above 20 s is posted.
pub fn service_penalty(id: i32, minutes: u32, long: bool, log: &mut RunLog) -> f32 {
    let over = minutes as f32 * 60.0 - if long { 2400.0 } else { 1200.0 };
    let penalty = crate::service::late_penalty(-over);
    if id >= 0 && penalty > 20.0 {
        log.bulletins.push(Bulletin::ServicePenalty { driver: id, penalty });
    }
    penalty
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_season::tests::*;

    /// `Ev9ec0`: value 5 at engineering 3 costs `97 * 0.01 * 5` (4.85 -> 4), time 5 at 3 x 6 = 18 % off
    /// (`82 * 0.01 * 5` = 4.1 -> 4).
    #[test]
    fn repair_cost_takes_the_crew_off() {
        assert_eq!(repair_cost(&master(), &team(), 2), (4, 4));
    }

    /// The temperament changes when `rand() / 32767` reaches the resistance.
    #[test]
    fn a_temperament_resists_by_its_table() {
        let mut m = master();
        m.temperaments[0].mood_resistance[2] = 0.5;
        let seed =
            |pass: bool| (0..u32::MAX).find(|&s| (CrtRand(s).draw() as f32 * INV_RAND_MAX >= 0.5) == pass).unwrap();
        let mut d = driver();
        set_temperament(&m, &mut d, 2, &mut CrtRand(seed(false)));
        assert_eq!(d.driver.temperament, 0);
        set_temperament(&m, &mut d, 2, &mut CrtRand(seed(true)));
        assert_eq!(d.driver.temperament, 2);
    }

    /// Style 3 limits the repairs to the budget's share: 4 x 30 / 100 = 1, below every repair's cost of 4, so only
    /// the order is drawn and nothing is repaired; with style 5 the broken part goes first, then the tyres, then the
    /// cheapest of the rest until the 20 minutes are used (no `take`, so repairs go on past them).
    #[test]
    fn service_picks_broken_parts_first_within_the_limits() {
        let m = master();
        let (mut d, mut c) = (driver(), car());
        c.parts = [1, 2, 0, 0, 4, 0, 3, 0];
        let t = Team { manager_style: 3, ..team() };
        let mut costs = TeamCosts::new(&t);
        let mut log = RunLog::default();
        assert_eq!(service(&m, 3, &mut d, &mut c, &t, &mut costs, false, &mut CrtRand(9), &mut log), 0);
        assert_eq!(c.parts, [1, 2, 0, 0, 4, 0, 3, 0]);
        costs.style = 5;
        let used = service(&m, 3, &mut d, &mut c, &t, &mut costs, false, &mut CrtRand(9), &mut log);
        assert_eq!(c.parts, [0; 8]);
        assert_eq!((used, costs.service, costs.rally), (16, 16, 16));
        assert_eq!(service_penalty(3, used, false, &mut log), 0.0);
        // 21 minutes: 10 s; 23 minutes: 30 s, posted (above 20 s).
        assert_eq!(service_penalty(3, 21, false, &mut log), 10.0);
        assert_eq!(service_penalty(3, 23, false, &mut log), 30.0);
        assert_eq!(service_penalty(3, 43, true, &mut log), 30.0);
        assert_eq!(log.bulletins.iter().filter(|b| matches!(b, Bulletin::ServicePenalty { .. })).count(), 2);
    }
}
