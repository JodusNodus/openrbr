//! The car's damage as a championship carries it from one stage to the next, and the repair parts' view of it.
//!
//! **What is carried.** The damage handler (`CCarDamageHandler`, 0x3eb0 bytes) is one of the game flow host's four
//! slots (`*(CGameflow + 0xc98) + 0x10`, the player's at index 0). In a championship the slot outlives the stage:
//! `InitQuickRaceDamageHandler` (0x567640) resets it (`Vf0c`) in the quick race's modes (1, 2, 7, 10) but not in mode
//! 3, where it only stores the season's damage level (`Set3FB0` with `[0x1660c70]`, the season's `CARDAM` option) and
//! copies the slot to the restart backups (+0xfad0, +0x13980, `CopyFrom` 0x4ec470). The game's own copy of "the damage"
//! is `CopyFrom`: the 64 zones' first words (stride 0x58 from +0x2214), the 64 zone floats (+0x3808) and the 0xfa words
//! from +0x3a0c (the drivetrain, engine, light, wheel, body and windscreen state, the level and its counters). That is
//! [`CarriedDamage`]. The zones' other words (+4, +8) are transient: the service's refresh (0x4ec3b0) zeroes them.
//!
//! **The repair parts.** `NRepair::CCarRepairHandler` (ctor 0x4f22b0) holds one damage object per physics id;
//! `GetDamage` (0x4f2b40) calls its `Vf00` on the handler, 0 (sound) to 5, and `ResetDamage` (0x4f2b70) its `Vf04`, the
//! repair. [`severity`] and [`repair`] are those objects, checked against the asm of every class's `Vf00` and `Vf04`
//! (the ctor 0x4f22b0 gives each id its class and index; `PickDamageIndex` 0x500380). The radiator's odd second bound
//! (0.09) and the intercooler and hosing both reading +0x3b10 are the exe's. Under NGP the wheels and tyres are NGP's
//! own objects ([`level`], [`reset_damage`]).

use crate::service::Rules;

/// The zones (`ZONES` first words, `ZONE_STRIDE` apart from `ZONES_AT`), the zone floats and the tail, as `CopyFrom`
/// copies them.
pub const ZONES: usize = 64;
pub const ZONES_AT: u32 = 0x2214;
pub const ZONE_STRIDE: u32 = 0x58;
pub const FLOATS_AT: u32 = 0x3808;
pub const TAIL_AT: u32 = 0x3A0C;
pub const TAIL_WORDS: usize = 0xFA;
/// The handler's size (`CGameflow`'s slots are this far apart).
pub const HANDLER_SIZE: u32 = 0x3EB0;

/// The damage a championship carries between stages: the words `CCarDamageHandler::CopyFrom` copies, in its order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CarriedDamage {
    pub zones: [u32; ZONES],
    pub floats: [u32; ZONES],
    pub tail: [u32; TAIL_WORDS],
}

impl Default for CarriedDamage {
    fn default() -> Self {
        Self { zones: [0; ZONES], floats: [0; ZONES], tail: [0; TAIL_WORDS] }
    }
}

/// Where each carried word lives in the handler, in `CopyFrom`'s order (zone word, zone float, ..., then the tail).
pub fn offsets() -> impl Iterator<Item = u32> {
    (0..ZONES as u32)
        .flat_map(|z| [ZONES_AT + ZONE_STRIDE * z, FLOATS_AT + 4 * z])
        .chain((0..TAIL_WORDS as u32).map(|i| TAIL_AT + 4 * i))
}

impl CarriedDamage {
    /// The carried words read through `word(offset)` (the handler's word at that offset).
    pub fn read(mut word: impl FnMut(u32) -> u32) -> Self {
        let mut d = Self::default();
        for z in 0..ZONES {
            d.zones[z] = word(ZONES_AT + ZONE_STRIDE * z as u32);
            d.floats[z] = word(FLOATS_AT + 4 * z as u32);
        }
        for (i, w) in d.tail.iter_mut().enumerate() {
            *w = word(TAIL_AT + 4 * i as u32);
        }
        d
    }

    /// The carried words with their handler offsets, in `CopyFrom`'s order.
    pub fn words(&self) -> impl Iterator<Item = (u32, u32)> + '_ {
        offsets().zip(self.zones.iter().zip(&self.floats).flat_map(|(&z, &f)| [z, f]).chain(self.tail.iter().copied()))
    }

    /// The word at a handler offset, if it is carried.
    pub fn get(&self, at: u32) -> Option<u32> {
        self.slot(at).map(|s| match s {
            Slot::Zone(z) => self.zones[z],
            Slot::Float(z) => self.floats[z],
            Slot::Tail(i) => self.tail[i],
        })
    }

    /// Sets a carried word; offsets outside the carried words are ignored (the game's copy drops them too).
    pub fn set(&mut self, at: u32, value: u32) {
        match self.slot(at) {
            Some(Slot::Zone(z)) => self.zones[z] = value,
            Some(Slot::Float(z)) => self.floats[z] = value,
            Some(Slot::Tail(i)) => self.tail[i] = value,
            None => {}
        }
    }

    fn slot(&self, at: u32) -> Option<Slot> {
        if (TAIL_AT..TAIL_AT + 4 * TAIL_WORDS as u32).contains(&at) && (at - TAIL_AT).is_multiple_of(4) {
            return Some(Slot::Tail(((at - TAIL_AT) / 4) as usize));
        }
        if (FLOATS_AT..FLOATS_AT + 4 * ZONES as u32).contains(&at) && (at - FLOATS_AT).is_multiple_of(4) {
            return Some(Slot::Float(((at - FLOATS_AT) / 4) as usize));
        }
        if (ZONES_AT..ZONES_AT + ZONE_STRIDE * ZONES as u32).contains(&at)
            && (at - ZONES_AT).is_multiple_of(ZONE_STRIDE)
        {
            return Some(Slot::Zone(((at - ZONES_AT) / ZONE_STRIDE) as usize));
        }
        None
    }

    fn f32(&self, at: u32) -> f32 {
        f32::from_bits(self.get(at).unwrap_or(0))
    }

    fn u32(&self, at: u32) -> u32 {
        self.get(at).unwrap_or(0)
    }

    fn set_f32(&mut self, at: u32, v: f32) {
        self.set(at, v.to_bits());
    }

    /// The carried words as bytes (little-endian words in `CopyFrom`'s order): the file `rbr-standalone` reads and
    /// writes between stages (ours).
    pub fn to_bytes(&self) -> Vec<u8> {
        self.words().flat_map(|(_, w)| w.to_le_bytes()).collect()
    }

    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() != 4 * (2 * ZONES + TAIL_WORDS) {
            return None;
        }
        let mut words = bytes.chunks_exact(4).map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]));
        let mut d = Self::default();
        for z in 0..ZONES {
            d.zones[z] = words.next()?;
            d.floats[z] = words.next()?;
        }
        for w in d.tail.iter_mut() {
            *w = words.next()?;
        }
        Some(d)
    }

    /// The service park's refresh (`CCarDamageHandler` 0x4ec3b0, run on entering and leaving the service) on the carried
    /// words: the four `CU32Value8` at +0x3d04 cleared (eight floats each), +0x3af8, +0x3afc and +0x3b04 to 1.0, and the
    /// zone words zeroed when the level (+0x3df0) is 0. The zones' +4/+8 words it also zeroes are not carried.
    pub fn service_refresh(&mut self) {
        for k in 0..4 {
            for i in 0..8 {
                self.set_f32(0x3D04 + 0x20 * k + 4 * i, 0.0);
            }
        }
        for at in [0x3AFC, 0x3AF8, 0x3B04] {
            self.set_f32(at, 1.0);
        }
        if self.u32(0x3DF0) == 0 {
            self.zones = [0; ZONES];
        }
    }
}

/// One word of `CCarDamageHandler::WriteBinary` (0x4edfe0): a constant (a count or a tag) or the handler's word at an
/// offset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryWord {
    Const(u32),
    At(u32),
}

/// `WriteBinary`'s words in order (from the asm and its sub-writers): two counts of 64, the 64 zones' four words
/// (+0x2210..+0x221c, 0x58 apart), `GMDC`, the engine block (0x50c410 on +0x3aac), the four 8-float blocks at +0x3d04
/// (0x531820), the four wheels' 13 words at +0x3c04 (0x53b0a0), the four +0x3ce8 words (0x52d970), `GMDC`, the hinged
/// parts' triples at +0x3d84 (0x51cd70), the mounted parts' words at +0x3db4 (0x521650), the nine electrics words
/// +0x3dc4..+0x3de4, `GMDC`, the drivetrain at +0x3a18 (0x502d20), the 49 words of 0x4f1a00 on +0x3b24, +0x3dec,
/// +0x3de8, +0x3df0 and `SCOE`.
pub fn binary_layout() -> Vec<BinaryWord> {
    use BinaryWord::{At, Const};
    const GMDC: u32 = 0x4344_4D47;
    let rel = |base: u32, offs: &[u32]| offs.iter().map(move |&o| At(base + o)).collect::<Vec<_>>();
    let mut v = vec![Const(0x40), Const(0x40)];
    for z in 0..ZONES as u32 {
        v.extend(rel(0x2210 + ZONE_STRIDE * z, &[0, 4, 8, 0xC]));
    }
    v.push(Const(GMDC));
    v.extend(rel(
        0x3AAC,
        &[
            0x14, 0xC, 0x10, 0x8, 0x24, 0x1C, 0x20, 0x18, 0x34, 0x2C, 0x30, 0x28, 0x44, 0x3C, 0x40, 0x38, 0x58, 0x5C,
            0x60, 0x50, 0x54, 0x4C, 0x48, 0x64, 0x68, 0x70, 0x6C, 0x74,
        ],
    ));
    for k in 0..4 {
        v.extend(rel(0x3D04 + 0x20 * k, &[0, 4, 8, 0xC, 0x10, 0x14, 0x18, 0x1C]));
    }
    for k in 0..4 {
        v.extend(rel(0x3C04 + 0x38 * k, &[0, 4, 8, 0xC, 0x10, 0x14, 0x24, 0x18, 0x28, 0x1C, 0x2C, 0x20, 0x30]));
    }
    v.extend((0..4).map(|k| At(0x3CE8 + 8 * k)));
    v.push(Const(GMDC));
    for k in 0..4 {
        v.extend(rel(0x3D84 + 0xC * k, &[4, 0, 8]));
    }
    v.extend((0..4).map(|k| At(0x3DB4 + 4 * k)));
    v.extend((0..9).map(|k| At(0x3DC4 + 4 * k)));
    v.push(Const(GMDC));
    v.extend(rel(
        0x3A18,
        &[
            0x48, 0x4C, 0x50, 0x54, 0x58, 0x5C, 0x60, 0x64, 0x68, 0x6C, 0x20, 0x1C, 0x18, 0x2C, 0x28, 0x24, 0x38, 0x34,
            0x30, 0x44, 0x40, 0x3C, 0x4, 0x0, 0xC, 0x8, 0x14, 0x10, 0x78, 0x7C, 0x80, 0x84, 0x88, 0x8C, 0x90,
        ],
    ));
    for k in 0..4 {
        v.extend(rel(0x3B40 + 8 * k, &[0, 4, 0x20, 0x24, 0x40, 0x44, 0x60, 0x64, 0x80, 0x84, 0xA0, 0xA4]));
    }
    v.push(At(0x3C00));
    v.extend([At(0x3DEC), At(0x3DE8), At(0x3DF0), Const(0x454F_4353)]);
    v
}

impl CarriedDamage {
    /// The damage block's handler words (`WriteBinary`): the carried words where the block has them, 0 for the zones'
    /// other words (transient: the refresh clears +4 and +8; +0 is not carried).
    pub fn to_binary(&self) -> Vec<u32> {
        binary_layout()
            .into_iter()
            .map(|w| match w {
                BinaryWord::Const(c) => c,
                BinaryWord::At(at) => self.get(at).unwrap_or(0),
            })
            .collect()
    }

    /// The carried words from a damage block (`ReadBinary`). The zone floats (+0x3808) are not in the block: a season
    /// read back from the file has them 0, as the game's own load does.
    pub fn from_binary(words: &[u32]) -> Option<Self> {
        let layout = binary_layout();
        if words.len() != layout.len() {
            return None;
        }
        let mut d = Self::default();
        for (w, &v) in layout.iter().zip(words) {
            match *w {
                BinaryWord::Const(c) if c != v => return None,
                BinaryWord::At(at) => d.set(at, v),
                _ => {}
            }
        }
        Some(d)
    }
}

#[derive(Debug, Clone, Copy)]
enum Slot {
    Zone(usize),
    Float(usize),
    Tail(usize),
}

/// `PickDamageIndex` (0x500380): the first bound `v` does not exceed gives its id, kept if above `cur`.
fn pick(v: f32, bounds: &[f32], ids: &[i32], cur: i32) -> i32 {
    match bounds.iter().position(|&b| v <= b) {
        Some(i) if ids[i] > cur => ids[i],
        _ => cur,
    }
}

/// A flag word: set means sound (0), clear means `level`.
fn flag(d: &CarriedDamage, at: u32, level: i32) -> i32 {
    if d.u32(at) != 0 {
        0
    } else {
        level
    }
}

const STEPS4: [i32; 4] = [4, 3, 2, 1];
const STEPS3: [i32; 3] = [3, 2, 1];

/// The wheel (0 LF, 1 RF, 2 LB, 3 RB) and its part (id - base) of the per-wheel ids 101-111, 121-131, 141-151,
/// 161-171 (`CCarRepairHandler`'s ctor, 0x14 apart).
fn wheel_part(id: u32) -> Option<(u32, u32)> {
    if !(101..=171).contains(&id) {
        return None;
    }
    let (wheel, part) = ((id - 101) / 20, (id - 101) % 20);
    (part <= 10).then_some((wheel, part))
}

/// `GetDamage(id)`: the part's damage level, 0 (sound) to 5. Ids with no damage object give 0, as the handler's empty
/// slot does.
pub fn severity(d: &CarriedDamage, id: u32) -> u8 {
    let s = match id {
        10 => pick(1.0 - d.f32(0x3B00), &[0.8, 0.09, 0.97, 0.995], &STEPS4, 0),
        11 => flag(d, 0x3AF4, 4),
        12 => i32::from(d.f32(0x3B08) == 0.0) * 4,
        13 => i32::from(d.f32(0x3B0C) > 0.0) * 4,
        20 => {
            // 0x500b90: the shaft (+0x3b18) over four bounds, then the blades (+0x3b1c) over five, ids rising.
            let cur = pick(d.f32(0x3B18), &[0.1, 0.2, 0.4, 0.8], &[0, 1, 2, 3], 0);
            pick(d.f32(0x3B1C), &[0.1, 0.2, 0.4, 0.6, 0.8], &[0, 1, 2, 3, 4], cur)
        }
        21 => pick(d.f32(0x3B10), &[0.0, 0.05, 0.02], &[0, 2, 3], 0),
        22 => pick(1.0 - d.f32(0x3B10), &[0.6, 0.9, 0.95, 0.99], &STEPS4, 0),
        23 => (0..4).fold(0, |cur, k| pick(1.0 - d.f32(0x3AB4 + 0x10 * k), &[0.3, 0.8, 0.95, 0.97], &STEPS4, cur)),
        28 => {
            // 0x500a70: the words are working flags; a clear one is damage.
            if (0..4).any(|k| d.u32(0x3DD8 + 4 * k) == 0) {
                2
            } else if (0..5).any(|k| d.u32(0x3DC4 + 4 * k) == 0) {
                1
            } else {
                0
            }
        }
        29 => {
            let total: f32 =
                (0..ZONES as u32).map(|z| ((d.f32(ZONES_AT + ZONE_STRIDE * z) - 0.2) * 1.25).clamp(0.0, 1.0)).sum();
            pick(1.0 - total * 0.015625, &[0.6, 0.85, 0.99], &STEPS3, 0)
        }
        30 => {
            let level = d.f32(0x3AA0);
            let wear = (0..10).fold(0.0f32, |w, k| w.max(1.0 - d.f32(0x3A60 + 4 * k) * level));
            pick(1.0 - wear, &[0.2, 0.5, 0.8, 0.9], &STEPS4, 0)
        }
        31 => flag(d, 0x3A90, 4),
        32 => pick(d.f32(0x3A9C), &[0.4, 0.6, 0.8, 0.95], &STEPS4, 0),
        33 => flag(d, 0x3A94, 4),
        34 => pick(d.f32(0x3AA4), &[0.5, 0.65, 0.83, 0.92], &STEPS4, 0),
        35 => flag(d, 0x3AA8, 4),
        36 => flag(d, 0x3C00, 3),
        37 => flag(d, 0x3A98, 5),
        40 | 42 | 44 | 46 => flag(d, 0x3D88 + 0xC * ((id - 40) / 2), 5),
        41 | 43 | 45 => flag(d, 0x3D84 + 0xC * ((id - 41) / 2), 4),
        47 => flag(d, 0x3DB0, 4),
        50..=52 => match d.u32(0x3DB4 + 4 * (id - 50)) {
            1 => 2,
            2 => 5,
            _ => 0,
        },
        90..=92 => {
            let k = id - 90;
            let base = if d.u32(0x3A1C + 8 * k) == 0 { 4 } else { 0 };
            pick(d.f32(0x3A18 + 8 * k), &[0.3, 0.5, 0.7, 0.85], &STEPS4, base)
        }
        _ => match wheel_part(id) {
            Some((w, 0)) => pick(1.0 - d.f32(0x3CE8 + 8 * w), &[0.3, 0.6, 0.9, 0.98], &STEPS4, 0),
            Some((w, 1)) => flag(d, 0x3A30 + 0xC * w, 5),
            Some((w, 2)) => pick(1.0 - d.f32(0x3C10 + 0x38 * w).abs(), &[0.7, 0.9, 0.98], &STEPS3, 0),
            Some((w, 3)) => pick(1.0 - d.f32(0x3C0C + 0x38 * w).abs(), &[0.7, 0.9, 0.98], &STEPS3, 0),
            Some((w, 7)) => flag(d, 0x3A34 + 0xC * w, 4),
            Some((w, 9)) => {
                let bounds = [0.8, 0.95, 0.99];
                let cur = pick(1.0 - d.f32(0x3C04 + 0x38 * w).abs(), &bounds, &STEPS3, 0);
                pick(1.0 - d.f32(0x3C08 + 0x38 * w).abs(), &bounds, &STEPS3, cur)
            }
            Some((w, 10)) => pick(1.0 - d.f32(0x3C14 + 0x38 * w).abs(), &[0.9, 0.95, 0.99], &STEPS3, 0),
            // The wheel (part 4: its `Vf00` zeroes its fields and returns 0, see [`get_damage`]), tyre, disc and pad
            // (5, 6, 8: return 0, repair nothing): always sound.
            _ => 0,
        },
    };
    s.clamp(0, 5) as u8
}

/// `GetDamage(id)` (0x4f2b40) with its side effect: the wheel's `Vf00` (0x501030, part 4 of each wheel) clears the
/// wheel's eight floats (its repair's words) before it returns 0. [`severity`] is the rest, without it.
pub fn get_damage(d: &mut CarriedDamage, id: u32) -> u8 {
    if let Some((_, 4)) = wheel_part(id) {
        repair(d, id);
    }
    severity(d, id)
}

/// NGP's damage objects for the wheels and tyres (its hook on `E75_Svc::M5afe40`, NGP.dll 0x10062a30, run as the park
/// builds the repair tree). The tyres (106, 126, 146, 166; 0x10063850) read the mean of their `CU32Value8` at +0x3d04
/// (0x1005c6c0: `((f0 + 0.0) + f1 + ... + f7) * 0.125`, SSE) as the count of `[0.2, 0.4, 0.6, 0.8, 1.0]` below it, at
/// most 4 (0x10062440), and their repair zeroes the eight floats. The wheels (105, 125, 145, 165; 0x10063b60) read
/// their eight floats at +0x3c18 the same way (0x1005fb90) and keep 1.02's repair, without 1.02's clearing read.
fn ngp_level(d: &CarriedDamage, at: u32) -> u8 {
    let mean = (1..8).fold(d.f32(at) + 0.0, |s, k| s + d.f32(at + 4 * k)) * 0.125;
    [0.2f32, 0.4, 0.6, 0.8, 1.0].iter().filter(|&&b| b < mean).count().min(4) as u8
}

const NGP_TYRE_AT: u32 = 0x3D04;
const NGP_WHEEL_AT: u32 = 0x3C18;

/// `GetDamage(id)` under `rules`, without its side effect: [`severity`], or NGP's own wheels and tyres.
pub fn level(d: &CarriedDamage, id: u32, rules: Rules) -> u8 {
    match (rules, wheel_part(id)) {
        (Rules::Ngp, Some((w, 4))) => ngp_level(d, NGP_WHEEL_AT + 0x38 * w),
        (Rules::Ngp, Some((w, 5))) => ngp_level(d, NGP_TYRE_AT + 0x20 * w),
        _ => severity(d, id),
    }
}

/// `GetDamage(id)` under `rules`, with 1.02's side effect ([`get_damage`]; NGP's wheels have none).
pub fn get_damage_in(d: &mut CarriedDamage, id: u32, rules: Rules) -> u8 {
    match rules {
        Rules::V102 => get_damage(d, id),
        Rules::Ngp => level(d, id, rules),
    }
}

/// `ResetDamage(id)` under `rules`: [`repair`], or NGP's tyre repair.
pub fn reset_damage(d: &mut CarriedDamage, id: u32, rules: Rules) {
    match (rules, wheel_part(id)) {
        (Rules::Ngp, Some((w, 5))) => (0..8).for_each(|k| d.set(NGP_TYRE_AT + 0x20 * w + 4 * k, 0)),
        _ => repair(d, id),
    }
}

/// `ResetDamage(id)`: the part's repair on the handler's words.
pub fn repair(d: &mut CarriedDamage, id: u32) {
    let set = |d: &mut CarriedDamage, at: u32, v: u32| d.set(at, v);
    let one = 1.0f32.to_bits();
    match id {
        10 => set(d, 0x3B00, 0),
        11 => set(d, 0x3AF4, 1),
        12 => set(d, 0x3B08, one),
        13 => set(d, 0x3B0C, 0),
        20 => {
            set(d, 0x3B1C, 0);
            set(d, 0x3B18, 0);
        }
        21 | 22 => set(d, 0x3B10, 0),
        23 => {
            for k in 0..4 {
                set(d, 0x3AB4 + 0x10 * k, 0);
                set(d, 0x3AB8 + 0x10 * k, 0);
            }
        }
        28 => {
            for at in [0x3DD8, 0x3DDC, 0x3DE0, 0x3DE4, 0x3DC4, 0x3DC8, 0x3DCC, 0x3DD0, 0x3DD4] {
                set(d, at, 1);
            }
        }
        29 => {
            for z in 0..ZONES as u32 {
                let at = ZONES_AT + ZONE_STRIDE * z;
                if d.f32(at) > 0.15 {
                    d.set_f32(at, 0.15);
                }
            }
        }
        30 => {
            set(d, 0x3A90, 1);
            for k in 0..10 {
                set(d, 0x3A60 + 4 * k, one);
            }
        }
        31 => set(d, 0x3A90, 1),
        32 => set(d, 0x3A9C, one),
        33 => set(d, 0x3A94, 1),
        34 => set(d, 0x3AA4, one),
        35 => set(d, 0x3AA8, 1),
        36 => set(d, 0x3C00, 1),
        37 => set(d, 0x3A98, 1),
        40 | 42 | 44 | 46 => set(d, 0x3D88 + 0xC * ((id - 40) / 2), 1),
        41 | 43 | 45 => set(d, 0x3D84 + 0xC * ((id - 41) / 2), 1),
        47 => set(d, 0x3DB0, 1),
        50..=52 => set(d, 0x3DB4 + 4 * (id - 50), 0),
        90..=92 => {
            set(d, 0x3A1C + 8 * (id - 90), 1);
            set(d, 0x3A18 + 8 * (id - 90), one);
        }
        _ => match wheel_part(id) {
            Some((w, 0)) => set(d, 0x3CE8 + 8 * w, 0),
            Some((w, 1)) => set(d, 0x3A30 + 0xC * w, 1),
            Some((w, 2)) => set(d, 0x3C10 + 0x38 * w, 0),
            Some((w, 3)) => set(d, 0x3C0C + 0x38 * w, 0),
            Some((w, 4)) => {
                for at in [0x3C18, 0x3C28, 0x3C1C, 0x3C2C, 0x3C20, 0x3C30, 0x3C24, 0x3C34] {
                    set(d, at + 0x38 * w, 0);
                }
            }
            Some((w, 7)) => set(d, 0x3A34 + 0xC * w, 1),
            Some((w, 9)) => {
                set(d, 0x3C08 + 0x38 * w, 0);
                set(d, 0x3C04 + 0x38 * w, 0);
            }
            Some((w, 10)) => set(d, 0x3C14 + 0x38 * w, 0),
            _ => {}
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_round_trip_in_copy_from_order() {
        let mut d = CarriedDamage::default();
        d.set(ZONES_AT + ZONE_STRIDE, 7);
        d.set(FLOATS_AT + 4, 8);
        d.set(TAIL_AT, 9);
        let words: Vec<_> = d.words().take(4).collect();
        assert_eq!(words, [(ZONES_AT, 0), (FLOATS_AT, 0), (ZONES_AT + ZONE_STRIDE, 7), (FLOATS_AT + 4, 8)]);
        assert_eq!(d.words().nth(2 * ZONES), Some((TAIL_AT, 9)));
        assert_eq!(CarriedDamage::from_bytes(&d.to_bytes()), Some(d.clone()));
        let read = CarriedDamage::read(|at| d.get(at).unwrap());
        assert_eq!(read, d);
        // Words outside the copy are not carried.
        d.set(ZONES_AT + 4, 1);
        assert_eq!(d.get(ZONES_AT + 4), None);
    }

    #[test]
    fn flags_and_tables_give_levels_and_repairs_clear_them() {
        let mut d = CarriedDamage::default();
        // A clear flag is damage: the gearbox actuator (31) at 4, the brake circuit (37) at 5.
        assert_eq!(severity(&d, 31), 4);
        assert_eq!(severity(&d, 37), 5);
        repair(&mut d, 31);
        assert_eq!(severity(&d, 31), 0);
        // Battery charge 0.6: the second bound (0.65) gives 3.
        d.set(0x3AA4, 0.6f32.to_bits());
        assert_eq!(severity(&d, 34), 3);
        repair(&mut d, 34);
        assert_eq!(severity(&d, 34), 0);
        // A bent left-rear control arm (143): |bend| 0.25 -> 1 - 0.25 = 0.75 <= 0.9 gives 2.
        d.set(0x3C10 + 0x38 * 2, (-0.25f32).to_bits());
        assert_eq!(severity(&d, 143), 2);
        repair(&mut d, 143);
        assert_eq!(severity(&d, 143), 0);
        // The wheel, tyre, disc and pad objects never report damage; unknown ids neither.
        assert_eq!([105, 106, 107, 109, 200].map(|id| severity(&d, id)), [0; 5]);
    }

    #[test]
    fn turbo_tables_follow_the_asm() {
        let mut d = CarriedDamage::default();
        // Shaft 0.3 (<= 0.4: id 2), blades 0.7 (<= 0.8: id 4): the worse wins.
        d.set(0x3B18, 0.3f32.to_bits());
        assert_eq!(severity(&d, 20), 2);
        d.set(0x3B1C, 0.7f32.to_bits());
        assert_eq!(severity(&d, 20), 4);
        repair(&mut d, 20);
        assert_eq!(severity(&d, 20), 0);
    }

    #[test]
    fn electrics_repair_sets_all_nine_words() {
        let mut d = CarriedDamage::default();
        repair(&mut d, 28);
        assert!((0..9).all(|k| d.get(0x3DC4 + 4 * k) == Some(1)));
        assert_eq!(severity(&d, 28), 0);
    }

    #[test]
    fn the_wheels_get_damage_clears_its_floats() {
        let mut d = CarriedDamage::default();
        d.set(0x3C18 + 0x38, 0.5f32.to_bits());
        assert_eq!(severity(&d, 125), 0);
        assert_eq!(d.get(0x3C18 + 0x38), Some(0.5f32.to_bits()));
        assert_eq!(get_damage(&mut d, 125), 0);
        assert_eq!(d.get(0x3C18 + 0x38), Some(0));
    }

    #[test]
    fn the_damage_block_has_the_games_length_and_round_trips() {
        assert_eq!(binary_layout().len(), rbr_formats::saves::after_season::HANDLER_WORDS);
        let mut d = CarriedDamage::default();
        d.set(ZONES_AT, 0.5f32.to_bits());
        d.set(0x3A90, 1);
        d.set(0x3C00, 1);
        d.set(0x3B40, 7);
        let words = d.to_binary();
        assert_eq!(&words[..2], &[0x40, 0x40]);
        assert_eq!(*words.last().unwrap(), u32::from_le_bytes(*b"SCOE"));
        assert_eq!(CarriedDamage::from_binary(&words), Some(d));
    }

    #[test]
    fn bodywork_sums_the_zone_dents() {
        let mut d = CarriedDamage::default();
        assert_eq!(severity(&d, 29), 0);
        // 32 zones fully dented: total 32, 1 - 32/64 = 0.5 <= 0.6 gives 3.
        for z in 0..32 {
            d.set(ZONES_AT + ZONE_STRIDE * z, 1.0f32.to_bits());
        }
        assert_eq!(severity(&d, 29), 3);
        repair(&mut d, 29);
        assert_eq!(d.get(ZONES_AT), Some(0.15f32.to_bits()));
        assert_eq!(severity(&d, 29), 0);
    }

    #[test]
    fn the_service_refresh_keeps_dents_unless_the_level_is_zero() {
        let mut d = CarriedDamage::default();
        d.set(ZONES_AT, 0.5f32.to_bits());
        d.set(0x3D04, 2.0f32.to_bits());
        d.set(0x3DF0, 1);
        d.service_refresh();
        assert_eq!(d.get(ZONES_AT), Some(0.5f32.to_bits()));
        assert_eq!(d.get(0x3D04), Some(0));
        assert_eq!(d.get(0x3AFC), Some(1.0f32.to_bits()));
        d.set(0x3DF0, 0);
        d.service_refresh();
        assert_eq!(d.get(ZONES_AT), Some(0));
    }
}
