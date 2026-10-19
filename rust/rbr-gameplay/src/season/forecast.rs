//! A season's forecast weather: each rally's block of stage slots (`E80Blk28c::Init6` 0x5cea80), drawn as the season
//! starts, and the blocks in the season's bytes (+0x7f04).
//!
//! Faithful (from the asm): the rally's weather trend (`E80Rand4` 0x5ce8c0: 20/10/35/35 %), the start's row drawn
//! over the map's rows that are not fog (presets 8 and 9; `0x5b5610`), one row later when the rain chance is 50 or
//! more; each stage walks the index by the trend's step (`RandPick` 0x5ce990), then the rain amount and chance, the
//! fog rolls and the temperature, every `rand()` in the asm's order; each slot keeps its level's start time.

use rbr_formats::saves::season::Season;
use rbr_formats::season_dat;

use crate::championship::{Rally, RallyStage};
use crate::crt::CrtRand;
use crate::weather;

/// The blocks' rally ids in season order (`DaemonInit`).
pub const RALLY_ORDER: [u32; 6] = [2, 4, 1, 0, 5, 3];
pub(crate) const BLOCK: usize = 0x28c;
pub(crate) const SLOT: usize = 0x54;
const LETTERS: [char; 5] = ['M', 'N', 'E', 'O', 'S'];
/// `InitSlot`'s temperature (0x72f614).
const SLOT_TEMPERATURE: f32 = 20.0;

/// A stage's weather slot (`E80Sub54`, 0x54 bytes).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Slot {
    pub map: u32,
    pub letter: u32,
    pub preset: u8,
    pub cloud_letter: u8,
    pub cloud: u8,
    pub wetness: u8,
    /// +0x10: the rain amount.
    pub rain: f32,
    /// +0x14: the rain chance.
    pub rain_chance: f32,
    /// +0x18.
    pub temperature: f32,
    /// +0xc, +0xd: the start's hour and minute (`Init6`'s first loop 0x5ceab0 from `season.dat`'s level; the forecast
    /// leaves them; `M3800` sets the clock from them and `CSeasonScheduleUI` shows them, `GetC`/`GetD` 0x5ce790).
    pub hour: u8,
    pub minute: u8,
    /// The season-table row the slot came from (ours: the original keeps the row's words, not its number).
    pub row: usize,
}

impl Slot {
    /// `InitSlot` (0x5ce6e0): the slot from a season-table row.
    fn from_row(table: &weather::Season, row: usize) -> Slot {
        let r = table.rows[row];
        Slot {
            map: r.map,
            letter: r.letter,
            preset: r.preset as u8,
            cloud_letter: r.cloud_letter as u8,
            cloud: r.cloud as u8,
            wetness: r.wetness as u8,
            rain: 0.0,
            rain_chance: 0.0,
            temperature: SLOT_TEMPERATURE,
            hour: 0,
            minute: 0,
            row,
        }
    }

    pub fn track(&self) -> String {
        format!("track-{}_{}", self.map, self.letter_char())
    }

    fn letter_char(&self) -> char {
        LETTERS.get(self.letter as usize).copied().unwrap_or('M')
    }
}

/// A rally's block (`E80Blk28c`, 0x28c bytes): its id, weather trend, start and six stages.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Block {
    pub rally: u32,
    pub trend: u32,
    pub start: Slot,
    pub stages: [Slot; 6],
}

/// `E80Rand4` (0x5ce8c0): the rally's weather trend.
fn trend(rng: &mut CrtRand) -> u32 {
    let r = (rng.draw() % 100) as u32;
    let mut sum = 0;
    for (k, w) in [20u32, 10, 35, 35].into_iter().enumerate() {
        sum += w;
        if r < sum {
            return k as u32;
        }
    }
    3
}

/// `RandPick` (0x5ce990): the stage's step through the rows for the trend.
fn step(trend: u32, rng: &mut CrtRand) -> i32 {
    let roll = |rng: &mut CrtRand| (rng.draw() % 100) as u32;
    match trend {
        0 => {
            if roll(rng) < 30 {
                return if roll(rng) > 49 { 1 } else { -1 };
            }
            0
        }
        1 => {
            let size = if roll(rng) < 50 { 2 } else { 1 };
            (if roll(rng) > 49 { 1 } else { -1 }) * size
        }
        2 => -i32::from(roll(rng) < 70),
        3 => i32::from(roll(rng) < 70),
        _ => 0,
    }
}

/// `0x5b5610`: the map's rows that are not fog (presets 8 and 9), as season-table row numbers.
fn clear_rows(table: &weather::Season, map: u32) -> Vec<usize> {
    table.rows_of(map).filter(|&k| !matches!(table.rows[k].preset, 8 | 9)).collect()
}

/// `0x5b55d0`: the map's first fog row (preset 2 or 5).
fn fog_row(table: &weather::Season, map: u32) -> Option<usize> {
    table.rows_of(map).find(|&k| matches!(table.rows[k].preset, 2 | 5))
}

/// `E80Blk28c::Init6` (0x5cea80): rally `rally`'s block for `kind` (its `season.dat` level, 1-3).
pub fn init6(
    rally: u32,
    kind: usize,
    rallies: &[season_dat::Rally],
    table: &weather::Season,
    rng: &mut CrtRand,
) -> Result<Block, String> {
    let r = rallies.get(rally as usize).ok_or(format!("no rally {rally}"))?;
    let files = r.levels.get(kind.wrapping_sub(1)).ok_or(format!("rally {rally}: no level {kind}"))?;
    let setup = r.setup;
    let trend = trend(rng);
    let list = clear_rows(table, files[0].map);
    if list.is_empty() {
        return Err(format!("map {}: no season rows (the original stops at an int3)", files[0].map));
    }
    let mut index = (rng.draw() as u32 % list.len() as u32) as i32;
    if setup.rain_chance >= 50.0 {
        index += 1;
    }
    index = index.clamp(0, list.len() as i32 - 1);
    let start = Slot::from_row(table, list[index as usize]);
    let mut stages = [start; 6];
    for (u, info) in files[1..].iter().enumerate() {
        let list = clear_rows(table, info.map);
        if list.is_empty() {
            return Err(format!("map {}: no season rows", info.map));
        }
        index += step(trend, rng);
        index = index.max(0);
        if index >= list.len() as i32 {
            index = list.len() as i32 - 1;
        }
        let row = list[index as usize];
        // The row whose wetness the stage keeps (`[esp+0x1c]`): the drawn row, or the fog row that replaces it.
        let mut wet_row = row;
        let mut slot = Slot::from_row(table, row);
        slot.rain_chance = setup.rain_chance;
        let preset = table.rows[row].preset;
        if matches!(preset, 3 | 4 | 6 | 7) {
            let r = (rng.draw() % 100) as f32;
            let mut rain = if setup.rain_chance > r { setup.rain_chance } else { 0.0 };
            if rain > setup.max_rain {
                rain = setup.max_rain;
            }
            slot.rain = rain;
            let threshold;
            if matches!(preset, 4 | 7) {
                let v = setup.rain_chance * 2.0;
                threshold = if v > 100.0 { 100.0 } else { v };
                slot.rain_chance = threshold;
                slot.rain = 100.0;
            } else {
                slot.rain = 50.0;
                threshold = if setup.rain_chance > 100.0 { 100.0 } else { setup.rain_chance };
            }
            let r = ((rng.draw() % 1000) as f32 * 0.1).min(100.0);
            if r > threshold {
                if let Some(fog) = fog_row(table, slot.map) {
                    slot = Slot::from_row(table, fog);
                    wet_row = fog;
                }
            }
        } else {
            slot.rain_chance = 0.0;
            slot.rain = 0.0;
        }
        let t = ((rng.draw() % 100 - 50) as f64 * f64::from(0.1f32)) as f32;
        slot.temperature = (f64::from(setup.temperature) + f64::from(t)) as f32;
        if u > 0 {
            let mut fog = setup.fog_chance;
            if slot.letter == 0 || slot.letter == 2 {
                if matches!(stages[u - 1].preset, 8 | 9) {
                    fog *= 0.5;
                }
                let r = (rng.draw() % 100) as f32;
                if fog > r {
                    let list = clear_rows(table, slot.map);
                    if !list.is_empty() {
                        let k = usize::from(fog * 0.5 > r).min(list.len() - 1);
                        slot = Slot::from_row(table, list[k]);
                    }
                }
            }
            slot.wetness = table.rows[wet_row].wetness as u8;
        }
        stages[u] = slot;
    }
    for k in 0..5 {
        if stages[k + 1].rain_chance > 0.0 {
            let v = (f64::from(setup.rain_chance) * 0.5 + f64::from(stages[k].temperature)) as f32;
            stages[k].temperature = if v > 90.0 { 90.0 } else { v };
        }
        if stages[k].temperature == 0.0 && rally != 3 {
            stages[k].temperature = (rng.draw() % 10) as f32 + 1.0;
        }
    }
    let mut block = Block { rally, trend, start, stages };
    let time = |info: &season_dat::StageInfo| (info.hour as u8, info.minute as u8);
    (block.start.hour, block.start.minute) = time(&files[0]);
    for (slot, info) in block.stages.iter_mut().zip(&files[1..]) {
        (slot.hour, slot.minute) = time(info);
    }
    Ok(block)
}

/// A new season's blocks on `rng` (after `srand(seed)`, [`CrtRand`]`(seed)`): the six levels, then each block's
/// forecast. The season's draws go on on the same stream ([`create`](super::create)).
pub fn new_blocks(
    rng: &mut CrtRand,
    rallies: &[season_dat::Rally],
    table: &weather::Season,
) -> Result<([usize; 6], Vec<Block>), String> {
    let kinds: [usize; 6] = std::array::from_fn(|_| (rng.draw() % 3 + 1) as usize);
    let blocks = RALLY_ORDER
        .iter()
        .zip(kinds)
        .map(|(&rally, kind)| init6(rally, kind, rallies, table, rng))
        .collect::<Result<_, _>>()?;
    Ok((kinds, blocks))
}

impl Block {
    /// The block in the season's bytes (+0x7f04, block `k`).
    pub fn write(&self, bytes: &mut [u8]) {
        let put = |b: &mut [u8], at: usize, v: u32| b[at..at + 4].copy_from_slice(&v.to_le_bytes());
        put(bytes, 0, self.rally);
        for (s, slot) in self.stages.iter().enumerate() {
            let at = 4 + s * SLOT;
            put(bytes, at, slot.map);
            put(bytes, at + 4, slot.letter);
            bytes[at + 8..at + 14].copy_from_slice(&[
                slot.preset,
                slot.cloud_letter,
                slot.cloud,
                slot.wetness,
                slot.hour,
                slot.minute,
            ]);
            put(bytes, at + 0x10, slot.rain.to_bits());
            put(bytes, at + 0x14, slot.rain_chance.to_bits());
            put(bytes, at + 0x18, slot.temperature.to_bits());
        }
        put(bytes, 0x1fc, self.start.map);
        put(bytes, 0x200, self.start.letter);
        let s = &self.start;
        bytes[0x204..0x20a].copy_from_slice(&[s.preset, s.cloud_letter, s.cloud, s.wetness, s.hour, s.minute]);
        put(bytes, 0x214, SLOT_TEMPERATURE.to_bits());
        put(bytes, 0x288, self.trend);
    }

    /// The block back from the season's bytes; the rows are found again in the table by their words (ours).
    pub fn read(bytes: &[u8], table: &weather::Season) -> Block {
        let word = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
        let slot = |at: usize, bytes4: usize| {
            let (map, letter) = (word(at), word(at + 4));
            let b = &bytes[bytes4..bytes4 + 4];
            let row = table
                .rows
                .iter()
                .position(|r| {
                    r.map == map
                        && r.letter == letter
                        && r.preset as u8 == b[0]
                        && r.cloud_letter as u8 == b[1]
                        && r.cloud as u8 == b[2]
                })
                .or_else(|| table.rows_of(map).find(|&k| table.rows[k].letter == letter))
                .unwrap_or(0);
            Slot {
                map,
                letter,
                preset: b[0],
                cloud_letter: b[1],
                cloud: b[2],
                wetness: b[3],
                rain: 0.0,
                rain_chance: 0.0,
                temperature: SLOT_TEMPERATURE,
                hour: bytes[bytes4 + 4],
                minute: bytes[bytes4 + 5],
                row,
            }
        };
        let stages = std::array::from_fn(|s| {
            let at = 4 + s * SLOT;
            Slot {
                rain: f32::from_bits(word(at + 0x10)),
                rain_chance: f32::from_bits(word(at + 0x14)),
                temperature: f32::from_bits(word(at + 0x18)),
                ..slot(at, at + 8)
            }
        });
        Block { rally: word(0), trend: word(0x288), start: slot(0x1fc, 0x204), stages }
    }

    /// The rally the flow runs: the forecast's tracks and rows, with the slots' start times. A season this crate
    /// saved before it kept the times has 00:00 in every slot: those take `season.dat`'s (the map's stage in any
    /// level; ours).
    pub fn rally(&self, rallies: &[season_dat::Rally]) -> Rally {
        let info = rallies.get(self.rally as usize);
        let legacy = self.stages.iter().all(|s| (s.hour, s.minute) == (0, 0));
        let time = |s: usize, slot: &Slot| {
            if !legacy {
                return (u32::from(slot.hour), u32::from(slot.minute));
            }
            info.and_then(|r| {
                r.levels
                    .iter()
                    .find_map(|files| files.get(s + 1).filter(|f| f.map == slot.map).map(|f| (f.hour, f.minute)))
            })
            .unwrap_or((9, 0))
        };
        let stages = self
            .stages
            .iter()
            .enumerate()
            .map(|(s, slot)| {
                let (hour, minute) = time(s, slot);
                RallyStage { map: slot.map, letter: slot.letter_char(), row: slot.row, hour, minute }
            })
            .collect();
        Rally { stages }
    }
}

/// The season's blocks.
pub fn blocks(season: &Season, table: &weather::Season) -> Vec<Block> {
    season.blocks.chunks_exact(BLOCK).map(|b| Block::read(b, table)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::season::tests::{rallies, table};

    #[test]
    fn the_trend_and_steps_follow_their_tables() {
        let seed = (0..1000).find(|&s| CrtRand(s).draw() % 100 == 25).unwrap();
        assert_eq!(trend(&mut CrtRand(seed)), 1);
        // Trend 2: one roll, -1 below 70.
        let mut r = CrtRand(7);
        let expect = -i32::from(CrtRand(7).draw() % 100 < 70);
        assert_eq!(step(2, &mut r), expect);
    }

    /// The forecast's first draws: the trend, then the start row over the non-fog rows (one later at a 60 % rain
    /// chance).
    #[test]
    fn the_forecast_draws_in_order() {
        let (t, r) = (table(), rallies());
        let seed = 12345;
        let mut probe = CrtRand(seed);
        probe.draw();
        let start = (probe.draw() as u32 % 4) as usize + 1;
        let b = init6(2, 1, &r, &t, &mut CrtRand(seed)).unwrap();
        // The slots keep the level's start times.
        assert_eq!((b.start.hour, b.stages[5].minute), (10, 30));
        assert_eq!(b.start.row, t.rows_of(10).filter(|&k| t.rows[k].preset != 8).nth(start.min(3)).unwrap());
        for s in &b.stages {
            assert_ne!(s.preset, 8);
            assert!(s.temperature > 0.0 && s.temperature <= 90.0);
        }
        // Deterministic on the seed.
        assert_eq!(b, init6(2, 1, &r, &t, &mut CrtRand(seed)).unwrap());
    }
}
