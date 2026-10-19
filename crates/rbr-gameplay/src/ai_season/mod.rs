//! The AI drivers' stage times and accidents ([`stage_time`]) and services ([`service()`]) (`CAISeason`, E79/E80), pure,
//! on the CRT's `rand()` stream.
//!
//! Traced from the asm of `E78HostCC::Ev094e0` (0x5c94e0, one AI driver's stage: three sections, two checks each) and
//! its helpers: `E80Team::Decay3` 0x5cd230, `ResetCD` 0x5cd1e0, `Count9c` 0x5cd370, `CountB8` 0x5cd410, `StepC4`
//! 0x5cd6c0, `StepCC` 0x5cd7b0; `E80AIEntry::ClampA/B/C` 0x5cce30/60/90, `StepA` 0x5ccf80, `SumA`, `AnyAGt3`; the
//! accident pick 0x5cba10, the mistake type 0x5cbb10, the accident time 0x5cba70, the par draw 0x5c90f0, the section
//! par 0x5d7480 (rally level table at 0x7d60d0 / 0x7d60d4, wet x1.09) and `CAICarDmgPenaltyTable`. The `rand()` calls
//! come in the order of the asm; each `rand() % n` is the CRT's signed remainder unless noted. x87 products are taken
//! in f64 and rounded to f32 where the original stores them.
//!
//! Ours: the x87 extended intermediates are f64 here (a last-bit difference is possible but not seen); the read of
//! the master object past the mistake table for skill sums outside 1-18 follows the object's layout
//! ([`master_word`]), but a read before the object gives 0.

use rbr_formats::ai_ini::{Car, Driver, Master};

use crate::crt::CrtRand;

mod service;
mod stage;

pub use service::{repair_cost, service, service_penalty, set_temperament, TeamCosts};
pub use stage::{false_start_penalty, section_par, stage_time, track_par, Run, RunLog, FALSE_START_PENALTY};

/// The CRT's `rand() % n` (signed `idiv`; `rand()` is never negative).
fn rem(rng: &mut CrtRand, n: i32) -> u32 {
    (rng.draw() % n) as u32
}

/// `_ftol2`: truncation, the integer indefinite (0x80000000) outside the `i32` range; read unsigned.
fn ftol(v: f64) -> u32 {
    if v > -2_147_483_649.0 && v < 2_147_483_648.0 {
        v as i32 as u32
    } else {
        0x8000_0000
    }
}

/// `1 / 32767` (0x795e68).
const INV_RAND_MAX: f32 = 3.051_851e-5;
/// A withdrawal or retire time (0x795e20).
pub const RETIRED: f32 = -1.0;

/// An AI driver's state over the season (`E80Team`).
#[derive(Debug, Clone, PartialEq)]
pub struct DriverState {
    pub driver: Driver,
    /// +0xc4, +0xc8: the driver's injury level (0-7) and its length.
    pub injury: u32,
    pub injury_len: u32,
    /// +0xcc, +0xd0: the co-driver's.
    pub co_injury: u32,
    pub co_injury_len: u32,
    /// +0x0: the last stage time less the par.
    pub vs_par: f32,
    /// +0x4: the last stage's section times.
    pub sections: [f32; 3],
}

impl DriverState {
    pub fn new(driver: Driver) -> Self {
        Self { driver, injury: 0, injury_len: 0, co_injury: 0, co_injury_len: 0, vs_par: 0.0, sections: [0.0; 3] }
    }

    /// `Decay3` (0x5cd230).
    fn decay3(&mut self) {
        if self.injury != 0 && self.injury_len < 3 {
            self.injury_len = 0;
            self.injury = 0;
        }
        self.vs_par = 0.0;
        if self.co_injury != 0 && self.co_injury_len < 3 {
            self.co_injury_len = 0;
            self.co_injury = 0;
        }
    }

    /// `ResetCD` (0x5cd1e0).
    fn reset_cd(&mut self) {
        if self.injury != 0 {
            if self.injury_len == 0 {
                self.injury = 0;
            }
            if self.injury_len < 2 {
                self.injury_len = 0;
            }
        }
        if self.co_injury != 0 {
            if self.co_injury_len == 0 {
                self.co_injury = 0;
            }
            if self.co_injury_len < 2 {
                self.co_injury_len = 0;
            }
        }
    }

    /// `Count9c` (0x5cd370): the technique less the injury.
    fn technique(&self) -> u32 {
        let t = self.driver.technique as u32;
        let less = match self.injury {
            1..=5 => 1,
            6 => 2,
            _ => return t,
        };
        let v = t.wrapping_sub(less);
        if v < 2 {
            1
        } else {
            v
        }
    }

    /// `CountB8` (0x5cd410): the endurance less the injury.
    fn endurance(&self) -> u32 {
        let e = self.driver.endurance as u32;
        let less = match self.injury {
            5 | 6 => 1,
            7 => 2,
            _ => return e,
        };
        let v = e.wrapping_sub(less);
        if v < 2 {
            1
        } else {
            v
        }
    }

    /// `StepC4` (0x5cd6c0, co-driver `false`) and `StepCC` (0x5cd7b0, `true`): an injury roll; 1 when it retires.
    fn injure(&mut self, m: &Master, add: i32, co: bool, rng: &mut CrtRand) -> bool {
        let x = (rem(rng, 20) as i32 + 1 + add - self.endurance() as i32).clamp(1, 20) as u32;
        let bands = if co { &m.codriver_injury } else { &m.driver_injury };
        let level = bands.iter().position(|&b| x <= b).unwrap_or(7) as u32;
        let (cur, len) =
            if co { (self.co_injury, &mut self.co_injury_len) } else { (self.injury, &mut self.injury_len) };
        if level < cur {
            return false;
        }
        match (co, level) {
            (_, 1) => *len = 1,
            (false, 2) => *len = 2,
            (false, 3) => *len = 3,
            (true, 2 | 3) => *len = 3,
            (_, 4..=7) => *len = 4,
            _ => {}
        }
        if co {
            self.co_injury = level;
        } else {
            self.injury = level;
        }
        level >= 5 && rem(rng, 100) < (8u32.wrapping_sub(self.endurance())).wrapping_mul(10)
    }
}

/// An AI car's state (`E80AIEntry`): eight parts (tyres, suspension, brakes, turbo, engine, transmission, steering,
/// differential), each 0 sound, 1-3 light to severe, 4 broken, and the hits at the current level.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CarState {
    pub car: Car,
    pub parts: [u32; 8],
    pub hits: [u32; 8],
}

impl CarState {
    pub fn new(car: Car) -> Self {
        Self { car, parts: [0; 8], hits: [0; 8] }
    }

    fn clamped(&self, base: i32, parts: &[usize]) -> i32 {
        if parts.iter().any(|&p| self.parts[p] == 2) {
            return 1;
        }
        let v = parts.iter().fold(base, |v, &p| v.wrapping_sub(self.parts[p] as i32));
        v.max(1)
    }

    /// `ClampA` (0x5cce30): control less suspension and steering.
    fn control(&self) -> i32 {
        self.clamped(self.car.control, &[6, 1])
    }

    /// `ClampB` (0x5cce60): power less turbo, engine and transmission.
    fn power(&self) -> i32 {
        self.clamped(self.car.power, &[3, 4, 5])
    }

    /// `ClampC` (0x5cce90): handling less brakes, tyres and differential.
    fn handling(&self) -> i32 {
        self.clamped(self.car.handling, &[2, 0, 7])
    }

    /// `StepA` (0x5ccf80): damage `part` at `level` (1 light, 2 medium, 3 heavy); 1 when the part breaks.
    fn damage(&mut self, m: &Master, part: usize, level: u32, rng: &mut CrtRand) -> bool {
        let Some(&state) = self.parts.get(part) else { return false };
        let l = level as usize - 1;
        if state == 3 {
            let chance = m.breakdown_base[l].wrapping_sub(self.car.durability.wrapping_mul(m.breakdown_durability[l]));
            if rng.draw() % 100 < chance {
                self.parts[part] = 4;
                return true;
            }
        } else if state <= level {
            let next = if state == level {
                self.hits[part] += 1;
                if self.hits[part] <= m.blow_limits[l] {
                    return false;
                }
                state + 1
            } else {
                level
            };
            self.parts[part] = next;
            self.hits[part] = 1;
        }
        false
    }
}

/// A bulletin the season posts (`E53NewsQueue`, the 11 `CAIBulletinEvent*` kinds), with the AI driver's id.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Bulletin {
    /// `PostAccident` 0x544080: an accident cost `time` seconds.
    Accident {
        driver: i32,
        accident: i32,
        time: f32,
    },
    /// `PostShaken` 0x5440e0: shaken by an accident in a section, with the worse of the two injuries.
    Shaken {
        driver: i32,
        accident: i32,
        stage: u32,
        section: u32,
        injury: u32,
    },
    /// `PostDamageRetire` 0x544140: a broken car.
    DamageRetire {
        driver: i32,
    },
    /// `PostDamageHurt` 0x544190: hurt in an accident.
    Hurt {
        driver: i32,
        accident: i32,
        stage: u32,
    },
    /// `PostDead` 0x5441e0: the worst injury (7).
    Dead {
        driver: i32,
        stage: u32,
    },
    /// `PostManagerOrder` 0x544230 (from the AI service, `Ev09f80`): the manager asks the driver for a temperament
    /// (`SetA0Rnd`; the driver may resist).
    ManagerOrder {
        driver: i32,
        order: i32,
    },
    /// `PostFalseStart`, `PostFalseStartRetire` (from the stage loop 0x5d4c90).
    FalseStart {
        driver: i32,
        stage: u32,
    },
    FalseStartRetire {
        driver: i32,
        stage: u32,
    },
    /// `PostServicePenalty` 0x544320 (`E80Daemon::Penalty` 0x5cf330, above 20 s): late out of the service.
    ServicePenalty {
        driver: i32,
        penalty: f32,
    },
    CarUnlock {
        driver: i32,
        car: i32,
    },
    Uneventful {
        stage: u32,
    },
}

impl Bulletin {
    /// Simple English text (ours; the game's strings are in `strings.rbz`).
    pub fn text(&self, name: &dyn Fn(i32) -> String) -> String {
        match *self {
            Bulletin::Accident { driver, time, .. } => format!("{} lost {:.1} s in an accident", name(driver), time),
            Bulletin::Shaken { driver, section, .. } => {
                format!("{} was shaken by an accident in section {}", name(driver), section + 1)
            }
            Bulletin::DamageRetire { driver } => format!("{} retired with a broken car", name(driver)),
            Bulletin::Hurt { driver, .. } => format!("{} was hurt in an accident", name(driver)),
            Bulletin::Dead { driver, .. } => format!("{} was badly injured and is out", name(driver)),
            Bulletin::ManagerOrder { driver, .. } => format!("{} received team orders", name(driver)),
            Bulletin::FalseStart { driver, .. } => format!("{} jumped the start", name(driver)),
            Bulletin::FalseStartRetire { driver, .. } => {
                format!("{} was excluded for repeated false starts", name(driver))
            }
            Bulletin::ServicePenalty { driver, penalty } => {
                format!("{} was penalised {penalty:.0} s for a late service", name(driver))
            }
            Bulletin::CarUnlock { .. } => "A new car is available".to_string(),
            Bulletin::Uneventful { .. } => "An uneventful stage".to_string(),
        }
    }
}

/// An accident record (`E79PairTmp` pushed to the daemon's pair list): driver, section, accident string, retired,
/// time.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AccidentRecord {
    pub driver: i32,
    pub section: u32,
    pub accident: u8,
    pub retired: bool,
    pub time: f32,
}

/// A word of the master object at word index `k` (layout in `rbr_formats::ai_ini`), as the original reads it past a
/// table's end; 0 before the object or past the temperaments (ours).
pub fn master_word(m: &Master, k: i64) -> i32 {
    let f = |v: f32| v.to_bits() as i32;
    let k = match usize::try_from(k) {
        Ok(k) => k,
        Err(_) => return 0,
    };
    match k {
        0..=89 => m.mistakes[k % 5][k / 5],
        90..=94 => m.skill_mods[k - 90],
        95..=99 => f(m.par_low[k - 95]),
        100..=104 => f(m.par_high[k - 100]),
        105..=112 => m.driver_injury[k - 105] as i32,
        113..=120 => m.codriver_injury[k - 113] as i32,
        121 => m.ignore_speciality as i32,
        122..=129 => m.repair_value[k - 122],
        130..=137 => m.repair_time[k - 130],
        138..=140 => m.blow_limits[k - 138] as i32,
        141..=143 => m.breakdown_base[k - 141],
        144..=146 => m.breakdown_durability[k - 144],
        147..=151 => f(m.temperaments[k - 147].accident),
        152..=156 => f(m.temperaments[k - 152].inspired),
        _ => 0,
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use rbr_formats::ai_ini::{Accident, AccidentTable, MistakeType, Section, Team, Temperament, Track};

    pub(crate) fn master() -> Master {
        let mut mistakes = [[0; 18]; 5];
        mistakes[2] = [30, 30, 29, 26, 23, 20, 17, 13, 10, 9, 7, 5, 3, 2, 1, 1, 1, 1];
        let acc = |k: i32| Accident {
            cumulative: k + 1,
            string: k,
            parts: [0, 1, 2, 3, 4, 5, 6, 7],
            light_only: k % 2 == 0,
            low: 1,
            high: 13,
        };
        let mt = |h: u32, lo: f32, hi: f32| MistakeType {
            highest: h,
            string: 0,
            low: lo,
            high: hi,
            light: 50,
            damage: 10,
            heavy: 5,
            injury: -5,
        };
        Master {
            mistakes,
            skill_mods: [-3, -1, 0, 1, 3],
            par_low: [-0.05, -0.11, -0.15, -0.22, -0.30],
            par_high: [4.225, 2.425, 0.975, 0.5, 0.2],
            driver_injury: [0, 8, 11, 14, 16, 18, 19, 20],
            codriver_injury: [0, 8, 11, 14, 16, 18, 19, 20],
            ignore_speciality: 50,
            repair_value: [5; 8],
            repair_time: [5; 8],
            blow_limits: [2, 2, 1],
            breakdown_base: [20, 70, 100],
            breakdown_durability: [1, 10, 10],
            temperaments: [Temperament { accident: 1.6, inspired: 3.5, mood_resistance: [0.0; 5] }; 5],
            accident_tables: vec![AccidentTable { total: 4, accidents: (0..4).map(acc).collect() }],
            mistake_types: vec![mt(10, 0.5, 4.5), mt(24, 6.0, 75.0)],
            damage_penalties: vec![(0, 0.0), (1, 0.5), (2, 1.5)],
            tracks: vec![],
        }
    }

    pub(crate) fn track() -> Track {
        let s = |par| Section { par, material: 0, checks: [(0, 3), (3, 2)], accident_table: 0 };
        Track { stage: 10, wet: 0, sections: [s(97.5), s(97.67), s(62.87)] }
    }

    pub(crate) fn driver() -> DriverState {
        DriverState::new(Driver {
            name: "Pablo Ramirez".into(),
            photo: 0,
            technique: 3,
            temperament: 0,
            skills: [3, 5, 3, 2],
            speed: 4,
            endurance: 3,
            condition: 9,
            history: 0,
            nationality: 16,
            codriver: "Rodrigo Perez".into(),
        })
    }

    pub(crate) fn car() -> CarState {
        CarState::new(Car { name: "Citroen Saxo".into(), control: 5, power: 4, handling: 5, durability: 3 })
    }

    pub(crate) fn team() -> Team {
        Team {
            name: "CoreDrive".into(),
            budget: 4,
            manager_style: 5,
            mechanics: 6,
            engineering: 3,
            specialities: vec![4],
            difficulties: vec![5],
            history: 4,
            nationality: 8,
            manager: "Estelle Gasparino".into(),
        }
    }

    #[test]
    fn the_master_words_follow_the_object() {
        let m = master();
        // Skill 1 reads the word before the modifiers: the mistake table's last word.
        assert_eq!(master_word(&m, 0x58 + 1), m.mistakes[4][17]);
        assert_eq!(master_word(&m, 0x58 + 2), -3);
        assert_eq!(master_word(&m, 0x58 + 6), 3);
        // Skill sum 3, check 3: row 2, column 2.
        assert_eq!(master_word(&m, 5 * 3 + 3 - 6), 29);
        assert_eq!(master_word(&m, -1), 0);
    }

    #[test]
    fn injuries_follow_the_bands() {
        let m = master();
        let mut d = driver();
        // rand() % 20 = 19: 20 + add 0 - endurance 3 = 17 -> band 5 (<= 18).
        let mut rng = CrtRand(0);
        let seed = (0..u32::MAX).find(|&s| CrtRand(s).draw() % 20 == 19).unwrap();
        rng.0 = seed;
        let mut probe = rng;
        probe.draw();
        let retire_draw = probe.draw() as u32 % 100;
        let out = d.injure(&m, 0, false, &mut rng);
        assert_eq!((d.injury, d.injury_len), (5, 4));
        // Level 5: the retire roll against (8 - endurance after the injury, 3 - 1) * 10.
        assert_eq!(out, retire_draw < 60);
    }

    #[test]
    fn car_damage_climbs_and_breaks() {
        let m = master();
        let mut c = car();
        let mut rng = CrtRand(1);
        assert!(!c.damage(&m, 2, 1, &mut rng));
        assert_eq!((c.parts[2], c.hits[2]), (1, 1));
        c.damage(&m, 2, 1, &mut rng);
        c.damage(&m, 2, 1, &mut rng);
        assert_eq!((c.parts[2], c.hits[2]), (2, 1));
        c.damage(&m, 2, 3, &mut rng);
        assert_eq!(c.parts[2], 3);
        assert_eq!(c.handling(), 2);
        // At 3 every hit rolls the breakdown: base 100 - durability 3 * 10 = 70 percent.
        let before = rng;
        let broke = c.damage(&m, 2, 3, &mut rng);
        assert_eq!(broke, CrtRand(before.0).draw() % 100 < 70);
    }
}
