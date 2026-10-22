//! A championship stage's ends for the rally flow (`rbr_gameplay::championship`): the
//! damage the car carries out of the stage, whether it can still drive and the stage's time, read from the physics memory after
//! the run, the race flow's end (`Game::end_now`), and the files the launcher passes between stages (ours:
//! `--damage-in`, `--damage-out`, `--result-out`).

use std::path::Path;

use rbr_gameplay::car_damage::CarriedDamage;
use rbr_gameplay::flow::StageEnd;
use rbr_hybrid::pure::CAR_PTR;
use rbr_physics::car_body::{damage, layout::DAMAGE_HANDLER};
use rbr_physics::mem::Mem;

/// The stage's end as the rally takes it.
#[derive(Debug, Clone, PartialEq)]
pub struct StageOut {
    pub damage: CarriedDamage,
    /// `CCarDamageHandler::Test3B20` on the car at the end: the engine runs and two wheels are on.
    pub drivable: bool,
    /// The race's end: the `CSigEndRace` payload (the HUD's race time with the penalties and its two splits) the
    /// season takes in mode 3.
    pub end: StageEnd,
    /// The race had a jump start: the season counts it (`IncT174` 0x5ce2e0, the rally's +0x174).
    pub false_start: bool,
}

/// The car's damage handler (`car + 0x9f4`).
fn handler(mem: &Mem) -> u32 {
    mem.u32(mem.u32(CAR_PTR) + DAMAGE_HANDLER)
}

/// `CCarDamageHandler::Test3B20` on the car in `mem`: the engine runs and two wheels are on.
pub fn drivable(mem: &Mem) -> bool {
    damage::drivable(mem, handler(mem))
}

/// The end of the stage: the car in `mem` and the race's `end`.
pub fn stage_out(mem: &Mem, end: StageEnd, false_start: bool) -> StageOut {
    let h = handler(mem);
    StageOut { damage: CarriedDamage::read(|at| mem.u32(h + at)), drivable: damage::drivable(mem, h), end, false_start }
}

/// Reads a carried-damage file.
pub fn read_damage(path: &Path) -> Result<CarriedDamage, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    CarriedDamage::from_bytes(&bytes).ok_or(format!("{}: not a carried-damage file", path.display()))
}

/// Writes the damage file and the result (`key=value` lines: `finished`, `time`, `split1`, `split2`, `drivable`,
/// `cause`, `false_start`).
pub fn write(out: &StageOut, damage_path: Option<&Path>, result_path: Option<&Path>) -> Result<(), String> {
    if let Some(path) = damage_path {
        std::fs::write(path, out.damage.to_bytes()).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    if let Some(path) = result_path {
        let race = out.end.end_race;
        let [s1, s2] = race.splits;
        let text = format!(
            "finished={}\ntime={}\nsplit1={s1}\nsplit2={s2}\ndrivable={}\ncause={:?}\nfalse_start={}\n",
            u8::from(out.end.finished()),
            race.time,
            u8::from(out.drivable),
            out.end.cause,
            u8::from(out.false_start)
        );
        std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    Ok(())
}
