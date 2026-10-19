//! One AI driver's stage (`E78HostCC::Ev094e0` 0x5c94e0, [`stage_time`]): the false-start roll, three sections of two
//! checks each with their par draws, mistakes, accidents and the inspired roll, and the section pars by the rally level.

use rbr_formats::ai_ini::{Accident, Master, MistakeType, Section, Team, Track};

use super::{ftol, master_word, rem, AccidentRecord, Bulletin, CarState, DriverState, INV_RAND_MAX, RETIRED};
use crate::crt::CrtRand;

/// The run's context: the track (with `wet` set from the stage's weather), the rally level (`*0x7d6128 + 0x8e84`),
/// the stage's index for the bulletins (`Ev094e0`'s last argument, the event's stage) and the extra check value (its
/// sixth argument; the stage loop passes 0).
#[derive(Debug, Clone, Copy)]
pub struct Run<'a> {
    pub track: &'a Track,
    pub level: u32,
    pub stage: u32,
    pub extra: u32,
}

/// What a run gives besides the time.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RunLog {
    pub bulletins: Vec<Bulletin>,
    pub accidents: Vec<AccidentRecord>,
}

/// False-start penalties (`Ev094e0`'s table at the start: 10, 60, 180, 600 s; the fourth retires) and chances (15,
/// 1, 0.25, 0.1, 0.01 percent by the starts jumped so far).
pub const FALSE_START_PENALTY: [f32; 4] = [10.0, 60.0, 180.0, 600.0];
const FALSE_START_CHANCE: [f32; 5] = [15.0, 1.0, 0.25, 0.1, 0.01];

/// The time penalty for a driver's `n`th false start in the rally (1-based), or `None` when it retires (the fourth).
pub fn false_start_penalty(n: u8) -> Option<f32> {
    (1..=3).contains(&n).then(|| FALSE_START_PENALTY[n as usize - 1])
}

/// The rally levels' par factors (0x7d60d0, 0x7d60d4; two (map, percent) pairs each, -1 any map): level 0 +30 %
/// (map 20: -1 %), level 1 +11 %, level 2 the par.
const LEVEL_PAR: [[(i8, i8); 2]; 2] = [[(20, -1), (-1, 30)], [(20, 11), (-1, 11)]];

/// `0x5d7480`: section `s`'s par for the run.
pub fn section_par(run: &Run, s: usize) -> f32 {
    let mut par = run.track.sections[s].par;
    if let Some(pairs) = LEVEL_PAR.get(run.level as usize) {
        if let Some(&(_, pct)) = pairs.iter().find(|&&(map, _)| map as i32 == run.track.stage as i32 || map == -1) {
            par = ((f64::from(pct) * f64::from(0.01f32) + 1.0) * f64::from(par)) as f32;
        }
    }
    if run.track.wet != 0 {
        par = (f64::from(par) * f64::from(1.09f32)) as f32;
    }
    par
}

/// `0x5d7530`: the three sections' pars added up.
pub fn track_par(run: &Run) -> f64 {
    (0..3).map(|s| f64::from(section_par(run, s))).sum()
}

/// `0x5c90f0`: a par adjustment drawn between par low and par high of `index` (1-5; 0 and 6 read the neighbouring
/// words as the original does).
fn par_draw(m: &Master, index: i64, rng: &mut CrtRand) -> f32 {
    let word = |k: i64| f32::from_bits(master_word(m, k) as u32);
    let (low, high) = (word(94 + index), word(99 + index));
    let r = rng.draw();
    ((f64::from(high) - f64::from(low)) * f64::from(r) * f64::from(INV_RAND_MAX) + f64::from(low)) as f32
}

/// `0x5cba10`: the accident drawn from `table` for the skill sum and check value.
fn pick_accident(table: &rbr_formats::ai_ini::AccidentTable, skill: i32, check: u32, rng: &mut CrtRand) -> Accident {
    let r = rng.draw() as u32;
    let roll = (r % table.total.max(1)).wrapping_sub(skill as u32).wrapping_add(check) as i32;
    table.accidents.iter().find(|a| roll <= a.cumulative).or(table.accidents.last()).copied().unwrap_or(Accident {
        cumulative: 0,
        string: 0,
        parts: [0; 8],
        light_only: true,
        low: 0,
        high: 1,
    })
}

/// `0x5cbb10`: the first mistake type whose highest number is at least `n`, else the last.
fn mistake_type(m: &Master, n: u32) -> MistakeType {
    *m.mistake_types.iter().find(|t| n <= t.highest).or(m.mistake_types.last()).expect("ai.ini has mistake types")
}

/// `Ev094e0` (0x5c94e0): one AI driver's stage. `false_starts` is the driver's count in the rally
/// (`CAISeason::GetPlayerFalseStarts`). Returns the stage time (sections plus penalties) or [`RETIRED`]; the driver's
/// [`DriverState::sections`] hold the section times.
#[allow(clippy::too_many_arguments)]
pub fn stage_time(
    m: &Master,
    run: &Run,
    id: i32,
    d: &mut DriverState,
    car: &mut CarState,
    team: &Team,
    false_starts: &mut u8,
    rng: &mut CrtRand,
    log: &mut RunLog,
) -> f32 {
    d.decay3();
    if d.driver.condition as u32 <= 2 || car.parts.iter().any(|&p| p > 3) {
        return RETIRED;
    }
    let mut total = 0.0f32;
    let r = rem(rng, 1000) as f32 * 0.1;
    let jumped = *false_starts;
    if jumped >= 5 {
        return RETIRED;
    }
    if FALSE_START_CHANCE[jumped as usize] > r {
        total = *FALSE_START_PENALTY.get(jumped as usize).unwrap_or(&0.0);
        *false_starts = jumped + 1;
        if *false_starts > 3 {
            return RETIRED;
        }
        log.accidents.push(AccidentRecord {
            driver: id,
            section: 0,
            accident: 0xff,
            retired: false,
            time: FALSE_START_PENALTY[*false_starts as usize - 1],
        });
    }
    let temperament = m.temperaments.get(d.driver.temperament as usize).copied();
    let (accident_chance, inspired_chance) = temperament.map_or((0.0, 0.0), |t| (t.accident, t.inspired));
    for s in 0..3 {
        let section: &Section = &run.track.sections[s];
        let mut mistakes = 0;
        let mut skill_sum = 0i32;
        let mut check_sum = 0u32;
        d.reset_cd();
        let mut time = section_par(run, s);
        for j in 0..2 {
            let skill = d.driver.skills.get(section.material as usize).copied().unwrap_or(0);
            let mut t5 = master_word(m, 0x58 + i64::from(skill));
            let (kind, check) = section.checks[j];
            let tech = d.technique() as i32;
            let average = match kind {
                0 => {
                    t5 = t5.wrapping_add(tech + car.handling() + car.control());
                    Some(f64::from(tech + car.handling() + car.control()) * f64::from(0.333_333_34f32))
                }
                1 => {
                    t5 = t5.wrapping_add(tech + car.handling());
                    Some(f64::from(tech + car.handling()) * 0.5)
                }
                2 => {
                    t5 = t5.wrapping_add(tech + car.control());
                    Some(f64::from(tech + car.control()) * 0.5)
                }
                3 => {
                    t5 = t5.wrapping_add(d.driver.speed + tech + car.power());
                    Some(f64::from(d.driver.speed + car.power()) * 0.5)
                }
                _ => None,
            };
            if let Some(a) = average {
                let index = (a + 0.5).floor() as i64;
                time = (f64::from(par_draw(m, index, rng)) + f64::from(time)) as f32;
            }
            skill_sum = skill_sum.wrapping_add(t5);
            let mut sw = check;
            let mut add = match d.co_injury {
                1..=3 => 1,
                4..=7 => 2,
                _ => 0,
            };
            add += run.extra;
            if add != 0 {
                sw = (sw + add).min(5);
            }
            check_sum += sw;
            let mistake = master_word(m, 5 * i64::from(t5) + i64::from(sw) - 6);
            let chance = ftol(f64::from(mistake as u32) * f64::from(accident_chance));
            if rem(rng, 100) > chance {
                continue;
            }
            mistakes += 1;
            let table = m.accident_tables.get(section.accident_table).or(m.accident_tables.last());
            let Some(table) = table else { continue };
            let acc = pick_accident(table, t5, sw, rng);
            let span = acc.high.wrapping_sub(acc.low) as u32;
            let n = (rng.draw() as u32 % span.max(1)).wrapping_add(acc.low as u32);
            let mt = mistake_type(m, n);
            let mut part = 0usize;
            let mut retired = false;
            let mut broken = false;
            let ignored = |car_part: i32, rng: &mut CrtRand| {
                team.specialities.contains(&car_part) && (rng.draw() as u32 % 100) < m.ignore_speciality
            };
            if !acc.light_only {
                if rem(rng, 100) < mt.heavy && !ignored(acc.parts[0], rng) {
                    if car.damage(m, acc.parts[0] as usize, 3, rng) {
                        log.bulletins.push(Bulletin::DamageRetire { driver: id });
                        broken = true;
                        retired = true;
                    }
                    part = 1;
                }
                let mut hurt = false;
                if rem(rng, 100) < mt.heavy {
                    hurt = d.injure(m, mt.injury, false, rng);
                }
                if rem(rng, 100) < mt.heavy {
                    hurt |= d.injure(m, mt.injury, true, rng);
                }
                let worst = d.injury.max(d.co_injury);
                if !hurt {
                    log.bulletins.push(Bulletin::Shaken {
                        driver: id,
                        accident: acc.string,
                        stage: run.stage,
                        section: s as u32,
                        injury: worst & 0xff,
                    });
                } else {
                    retired = true;
                    if worst as u8 == 7 {
                        log.bulletins.push(Bulletin::Dead { driver: id, stage: run.stage });
                    } else {
                        log.bulletins.push(Bulletin::Hurt { driver: id, accident: acc.string, stage: run.stage });
                    }
                }
                if rem(rng, 100) < mt.damage && !ignored(acc.parts[part], rng) {
                    if car.damage(m, acc.parts[part] as usize, 2, rng) {
                        retired = true;
                        if !broken {
                            log.bulletins.push(Bulletin::DamageRetire { driver: id });
                            broken = true;
                        }
                    }
                    part += 1;
                }
            }
            if rem(rng, 100) < mt.light
                && !ignored(acc.parts[part.min(7)], rng)
                && car.damage(m, acc.parts[part.min(7)] as usize, 1, rng)
            {
                retired = true;
                if !broken {
                    log.bulletins.push(Bulletin::DamageRetire { driver: id });
                }
            }
            let r = rng.draw();
            let lost = ((f64::from(mt.high) - f64::from(mt.low)) * f64::from(r) * f64::from(INV_RAND_MAX)
                + f64::from(mt.low)) as f32;
            if lost == RETIRED {
                retired = true;
            } else {
                time += lost;
                log.bulletins.push(Bulletin::Accident { driver: id, accident: acc.string, time: lost });
            }
            log.accidents.push(AccidentRecord {
                driver: id,
                section: s as u32,
                accident: acc.string as u8,
                retired,
                time: lost,
            });
            if retired {
                d.vs_par = (f64::from(RETIRED) - track_par(run)) as f32;
                return RETIRED;
            }
        }
        if mistakes == 0 {
            let x = (skill_sum / 2).wrapping_sub(check_sum as i32 / 2) as u32;
            let x = if (1..=99).contains(&x) { x } else { 1 };
            let r = rem(rng, 100);
            let v = ftol(f64::from(x) * f64::from(inspired_chance));
            if r <= v {
                let r2 = rng.draw();
                let gain = (r2 as f32 * INV_RAND_MAX) * (skill_sum as f32 * 0.05);
                let gain = gain.clamp(0.1, 1.0);
                let cut = f64::from(gain) * f64::from(0.01f32) * f64::from(section_par(run, s));
                time = (f64::from(time) - cut) as f32;
            }
        }
        let worn: u32 = car.parts.iter().sum();
        let over = worn.wrapping_sub(d.technique()) as i32;
        if over > 0 {
            let penalty = m
                .damage_penalties
                .iter()
                .find(|&&(key, _)| key >= over as u32)
                .or(m.damage_penalties.last())
                .map_or(0.0, |&(_, v)| v);
            time += penalty;
        }
        d.sections[s] = time;
        total += time;
    }
    d.vs_par = (f64::from(total) - track_par(run)) as f32;
    total
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_season::tests::*;

    #[test]
    fn par_follows_the_level_and_the_wet() {
        let t = track();
        let run = |level, wet| {
            let mut t = t;
            t.wet = wet;
            section_par(&Run { track: &t, level, stage: 0, extra: 0 }, 0)
        };
        assert_eq!(run(2, 0), 97.5);
        assert_eq!(run(1, 0), ((11.0 * f64::from(0.01f32) + 1.0) * 97.5) as f32);
        let level0 = ((30.0 * f64::from(0.01f32) + 1.0) * 97.5) as f32;
        assert_eq!(run(0, 1), (f64::from(level0) * f64::from(1.09f32)) as f32);
    }

    /// The run draws on one stream: two runs from the same seed agree, and a clean stage is close to the par.
    #[test]
    fn a_stage_is_deterministic_on_its_seed() {
        let m = master();
        let t = track();
        let run = Run { track: &t, level: 2, stage: 10, extra: 0 };
        let go = |seed| {
            let (mut d, mut c, mut fs, mut rng, mut log) = (driver(), car(), 0u8, CrtRand(seed), RunLog::default());
            let time = stage_time(&m, &run, 3, &mut d, &mut c, &team(), &mut fs, &mut rng, &mut log);
            (time, d.sections, fs, rng, log)
        };
        for seed in 0..200 {
            let a = go(seed);
            assert_eq!(a, go(seed));
            let (time, sections, ..) = a;
            if time != RETIRED {
                let par = track_par(&run) as f32;
                assert!(time > par - 5.0 && time < par + 700.0, "seed {seed}: {time} vs par {par}");
                assert_eq!(time, sections.iter().sum::<f32>() + if a.2 > 0 { FALSE_START_PENALTY[0] } else { 0.0 });
            }
        }
    }

    /// The rand order of a clean section: the false-start roll; check 1 (curves: skill sum -1 + 3 + 5 + 5 = 12,
    /// par index (3 + 5 + 5) / 3 + 0.5 -> 4) par draw and mistake roll (check 3: table 5 x 1.6 -> 8); check 2
    /// (straights: index (4 + 4) / 2 + 0.5 -> 4) par draw and mistake roll (row 2: 0); the inspired roll ((22 / 2 -
    /// 5 / 2) x 3.5 -> 31).
    #[test]
    fn the_rand_order_of_a_clean_section() {
        let m = master();
        let t = track();
        let run = Run { track: &t, level: 2, stage: 10, extra: 0 };
        let seed = (0..100_000u32)
            .find(|&s| {
                let mut r = CrtRand(s);
                let d: Vec<i32> = (0..6).map(|_| r.draw()).collect();
                d[0] % 1000 >= 150 && d[2] % 100 > 8 && d[4] % 100 > 0 && d[5] % 100 > 31
            })
            .unwrap();
        let mut r = CrtRand(seed);
        let d: Vec<i32> = (0..6).map(|_| r.draw()).collect();
        let draw = |x: i32| {
            let (low, high) = (f64::from(m.par_low[3]), f64::from(m.par_high[3]));
            ((high - low) * f64::from(x) * f64::from(INV_RAND_MAX) + low) as f32
        };
        let par = section_par(&run, 0);
        let expect = (f64::from(draw(d[3])) + f64::from((f64::from(draw(d[1])) + f64::from(par)) as f32)) as f32;
        let (mut dr, mut c, mut fs, mut rng, mut log) = (driver(), car(), 0u8, CrtRand(seed), RunLog::default());
        stage_time(&m, &run, 3, &mut dr, &mut c, &team(), &mut fs, &mut rng, &mut log);
        assert_eq!(fs, 0);
        assert_eq!(dr.sections[0].to_bits(), expect.to_bits());
    }

    #[test]
    fn false_starts_cost_then_retire() {
        assert_eq!(false_start_penalty(1), Some(10.0));
        assert_eq!(false_start_penalty(3), Some(180.0));
        assert_eq!(false_start_penalty(4), None);
    }
}
