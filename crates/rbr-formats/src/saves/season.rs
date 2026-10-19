//! The championship season inside the profile (`CAISeason` write 0x5d3e40, read 0x5d5330; magic `RBR7`, version
//! 0x13). Typed up to the third count-prefixed list; what follows (the AI drivers', cars' and teams' stores, then the
//! profile's damage and setup blocks) is [`super::after_season`].

use super::{Error, Reader, Writer};

pub const MAGIC: [u8; 4] = *b"7RBR";
pub const VERSION: u32 = 0x13;
const EVENTS: [u8; 4] = *b"NEVE";
const PAIRS: [u8; 4] = *b"CCCA";

/// Drivers in the season (`E80Daemon` +0x24, 14 x 0x90c).
pub const DRIVERS: usize = 14;
/// Rallies per driver entry and stages per rally (`E80Entry` and `E80Sub`).
pub const RALLIES: usize = 6;
pub const STAGES: usize = 6;
/// The championship's skill level `0x1660c5c` (`SSeasonSettings` +0x1c, the `SSKILL` page) in [`Season::globals`] and
/// [`Season::snapshot`].
pub const SKILL_WORD: usize = 7;
/// The championship's damage level `0x1660c70` (`SSeasonSettings` +0x30, the season's `CARDAM` page) in
/// [`Season::snapshot`]. The globals block stops at 0x1660c6c and does not hold it.
pub const SNAPSHOT_DAMAGE: usize = 12;

/// The season's fixed part and its three lists, in file order.
#[derive(Clone, Debug, PartialEq)]
pub struct Season {
    /// The daemon's byte at +0x4, written as a word.
    pub b04: u32,
    /// The daemon's words at +0x8e84..+0x8e9c (7), +0xa0c0, +0xa0c4, +0x8ea4..+0x8eb4 (5). +0x8e88 is the current
    /// rally index and +0x8e8c the stage index (`M005d14d0`, `M005d15b0` index the entries with them).
    pub state: [u32; 14],
    /// The drivers' results (+0x24).
    pub drivers: Vec<Driver>,
    /// +0x8: the season's seven teams, ids in the AI team list (`E78HostCC::At318`; `SyncE78b` 0x5d0300 reads a
    /// slot, the write's third loop stores each).
    pub teams: [u32; 7],
    /// +0x7ecc: the current stage's start order, entry indices (`M3800` 0x5d3800).
    pub w7ecc: [u32; 14],
    /// +0x7f04 (0xf48 bytes): `E80Blk28c` blocks; the C++ reference leaves them untyped.
    pub blocks: Vec<u8>,
    /// +0x8e4c: the rally's first-stage start order, entry indices (`M4990` 0x5d4990).
    pub w8e4c: [u32; 14],
    /// Globals: 0x1660804 (map), 0x1660808 (car), 0x1660824, 0x1660c4c..0x1660c6c (9), 0x1660814.
    pub globals: [u32; 13],
    /// `E80Daemon::Ser14b` (0x5d2e70): the 14-word snapshot.
    pub snapshot: [u32; 14],
    /// After `EVEN`: the season's event queue (0x7d614c, `E81ListHost`): the events still to run, then the ones run
    /// (`Shift` 0x5d5f60 moves the head from the first list to the second).
    pub events: Vec<Event>,
    pub events_b: Vec<Event>,
    /// After `ACCC`: the vector behind +0x8ea0 (empty when the pointer is null).
    pub pairs: Vec<[u32; 2]>,
}

/// One driver's season (`E80Entry`, 0x90c bytes).
#[derive(Clone, Debug, PartialEq)]
pub struct Driver {
    pub rallies: [DriverRally; RALLIES],
    /// +0x8d0 and its three padding bytes.
    pub b8d0: [u8; 4],
    /// +0x8d4: the driver id the time setters search for; the write stores a team for each entry with id > -1.
    pub id: i32,
    pub w8d8: u32,
    /// +0x8dc, NUL-terminated.
    pub name: [u8; 32],
    pub w8fc: u32,
    pub w900: u32,
    /// +0x904: the AI car instance (`E78HostCC::At324`; the write's second loop stores it). `SetupAI` (0x5d1080)
    /// gives a team's two drivers instances `2c` and `2c + 1` of the team's car `c`.
    pub car: u32,
    /// +0x908: the team. The stage loop (0x5d4c90) and the AI service take it as a team id (`At318`), the player's
    /// crew (0x5b0f47) as a slot of [`Season::teams`] (`SyncE78b`).
    pub team: u32,
}

/// One rally of a driver (`E80Sub`, 0x178 bytes).
#[derive(Clone, Debug, PartialEq)]
pub struct DriverRally {
    pub stages: [StageResult; STAGES],
    pub f168: f32,
    pub w16c: u32,
    /// +0x170: the penalties (`Penalty` 0x5cf330).
    pub f170: f32,
    /// +0x174: the false starts in the rally (`GetPlayerFalseStarts` 0x5d07a0 returns its address, `IncT174`
    /// 0x5ce2e0 counts the player's, `Ev094e0` the AI's).
    pub false_starts: u8,
    /// +0x175: padding.
    pub pad: [u8; 3],
}

/// One event of the season's queue (`E81Ev18`, 0x18 bytes; built by 0x5d5cc0).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Event {
    /// The driver's id (kinds 2, 3, 5, 6, 7, 11, 12), else 0 or the kind's own word. The stage loop reads the word as
    /// a float for kinds 5 and 6.
    pub id: u32,
    pub stage: u32,
    pub rally: u32,
    /// Kind 3: 1 for an AI driver (id >= 0).
    pub ai: u32,
    /// The clock: hour << 8 | minute (`E80Daemon` +0x8e98, +0x8e9c).
    pub time: u32,
    /// 3 a driver's stage, 4 the stage's end, 5 and 6 a service of 20 and 40 minutes, 8 the standings, 10 the
    /// overnight halt, 14 the rally's end (the stage loop's cases, 0x5d4c90).
    pub kind: u32,
}

impl Event {
    fn from_words(w: [u32; 6]) -> Self {
        Self { id: w[0], stage: w[1], rally: w[2], ai: w[3], time: w[4], kind: w[5] }
    }

    fn words(&self) -> [u32; 6] {
        [self.id, self.stage, self.rally, self.ai, self.time, self.kind]
    }
}

/// One stage result (`E80Rec`, 0x3c bytes).
#[derive(Clone, Debug, PartialEq)]
pub struct StageResult {
    /// +0x0..+0x8: the sectors (`M005d15b0`: split 1, split 2 - split 1, time - split 2).
    pub sectors: [f32; 3],
    /// +0xc: the stage time (`M005d14d0`).
    pub time: f32,
    /// +0x10: four 8-byte elements (`E79Elem8`).
    pub elems: [[u32; 2]; 4],
    /// +0x30: the element count, with padding.
    pub b30: [u8; 4],
    pub f34: f32,
    /// +0x38: 1 when the time was the retire sentinel (`M005d14d0`).
    pub retired: u32,
}

impl Season {
    pub(crate) fn read(r: &mut Reader) -> Result<Self, Error> {
        r.tag(MAGIC)?;
        let version = r.u32()?;
        if version != VERSION {
            return Err(Error::Version { found: version, want: VERSION });
        }
        let b04 = r.u32()?;
        let state = r.words()?;
        let drivers = (0..DRIVERS).map(|_| Driver::read(r)).collect::<Result<_, _>>()?;
        let teams = r.words()?;
        let w7ecc = r.words()?;
        let blocks = r.bytes(0xf48)?.to_vec();
        let w8e4c = r.words()?;
        let globals = r.words()?;
        let snapshot = r.words()?;
        r.tag(EVENTS)?;
        let events = list(r)?.into_iter().map(Event::from_words).collect();
        let events_b = list(r)?.into_iter().map(Event::from_words).collect();
        r.tag(PAIRS)?;
        let pairs = list(r)?;
        Ok(Self { b04, state, drivers, teams, w7ecc, blocks, w8e4c, globals, snapshot, events, events_b, pairs })
    }

    pub(crate) fn write(&self, w: &mut Writer) {
        w.bytes(&MAGIC);
        w.u32(VERSION);
        w.u32(self.b04);
        w.words(&self.state);
        self.drivers.iter().for_each(|d| d.write(w));
        w.words(&self.teams);
        w.words(&self.w7ecc);
        w.bytes(&self.blocks);
        w.words(&self.w8e4c);
        w.words(&self.globals);
        w.words(&self.snapshot);
        w.bytes(&EVENTS);
        write_list(w, &self.events.iter().map(Event::words).collect::<Vec<_>>());
        write_list(w, &self.events_b.iter().map(Event::words).collect::<Vec<_>>());
        w.bytes(&PAIRS);
        write_list(w, &self.pairs);
    }

    /// The current rally and stage indices (+0x8e88, and +0x8e8c's low byte).
    pub fn position(&self) -> (u32, u32) {
        (self.state[1], self.state[2] & 0xff)
    }

    /// The season's skill level (+0x8e84): `DaemonInit` (0x5d34b0) stores its second argument, which
    /// `E13MenuInitA` (0x47c7f0) takes from `0x1660c5c`. 0 Rookie, 1 Professional, 2 Champion. The outside-assistance
    /// penalty (`M576710`), the assistance gate (`E60Flow::M566640`, shut at 2 or more) and the AI's par (0x5d7480)
    /// read it.
    pub fn rally_level(&self) -> u32 {
        self.state[0]
    }

    /// The season's damage level (`0x1660c70`, snapshot word 12): `E80Snap14` 0x5d0cc0 puts it back in the global
    /// before each stage, and `InitQuickRaceDamageHandler` (0x567640) gives it to the car in mode 3.
    pub fn damage_level(&self) -> u32 {
        self.snapshot[SNAPSHOT_DAMAGE]
    }

    /// The options as a season the game started holds them: the skill in +0x8e84 and in `0x1660c5c`'s globals and
    /// snapshot words, the damage level in `0x1660c70`'s snapshot word (in mode 3 `Ser14b` copies the menu globals
    /// into the snapshot before it writes it).
    pub fn set_options(&mut self, skill: u32, damage: u32) {
        self.state[0] = skill;
        self.globals[SKILL_WORD] = skill;
        self.snapshot[SKILL_WORD] = skill;
        self.snapshot[SNAPSHOT_DAMAGE] = damage;
    }

    /// The entry and the current stage's record of the driver with this id, as the setters find them (`None` where
    /// the original breaks: no such id, or an index past 5).
    fn current(&mut self, id: i32) -> Option<(&mut DriverRally, usize)> {
        let (rally, stage) = self.position();
        let driver = self.drivers.iter_mut().find(|d| d.id == id)?;
        let rally = driver.rallies.get_mut(rally as usize)?;
        (stage < STAGES as u32).then_some((rally, stage as usize))
    }

    /// `E80Daemon::M005d14d0` (0x5d14d0): stores a stage time in the current stage and adds it to the rally's total
    /// (+0x168); [`RETIRED`] marks the stage retired and sets the total to it. Returns whether it stored.
    pub fn set_stage_time(&mut self, id: i32, time: f32) -> bool {
        let Some((rally, stage)) = self.current(id) else { return false };
        rally.stages[stage].time = time;
        if time == RETIRED {
            rally.stages[stage].retired = 1;
            rally.f168 = RETIRED;
        } else {
            rally.f168 += time;
        }
        true
    }

    /// `E80Daemon::M005d15b0` (0x5d15b0): stores a sector time (sector 3 and above go to sector 2).
    pub fn set_sector(&mut self, id: i32, sector: u8, value: f32) -> bool {
        let Some((rally, stage)) = self.current(id) else { return false };
        rally.stages[stage].sectors[sector.min(2) as usize] = value;
        true
    }
}

/// The retire time `M005d14d0` compares with (the f32 at 0x795e20).
pub const RETIRED: f32 = -1.0;

fn list<const N: usize>(r: &mut Reader) -> Result<Vec<[u32; N]>, Error> {
    let n = r.u32()? as usize;
    // Each item is at least four bytes: a count past the data is corrupt, not a huge allocation.
    if n > r.bytes.len() / 4 {
        return Err(Error::Eof { at: r.at() });
    }
    (0..n).map(|_| r.words()).collect()
}

fn write_list<const N: usize>(w: &mut Writer, items: &[[u32; N]]) {
    w.u32(items.len() as u32);
    items.iter().for_each(|i| w.words(i));
}

impl Driver {
    fn read(r: &mut Reader) -> Result<Self, Error> {
        let mut rallies = Vec::with_capacity(RALLIES);
        for _ in 0..RALLIES {
            rallies.push(DriverRally::read(r)?);
        }
        Ok(Self {
            rallies: rallies.try_into().unwrap(),
            b8d0: r.array()?,
            id: r.u32()? as i32,
            w8d8: r.u32()?,
            name: r.array()?,
            w8fc: r.u32()?,
            w900: r.u32()?,
            car: r.u32()?,
            team: r.u32()?,
        })
    }

    fn write(&self, w: &mut Writer) {
        self.rallies.iter().for_each(|s| s.write(w));
        w.bytes(&self.b8d0);
        w.u32(self.id as u32);
        w.u32(self.w8d8);
        w.bytes(&self.name);
        w.words(&[self.w8fc, self.w900, self.car, self.team]);
    }

    /// The name up to its NUL.
    pub fn name(&self) -> String {
        c_str(&self.name)
    }
}

impl DriverRally {
    fn read(r: &mut Reader) -> Result<Self, Error> {
        let mut stages = Vec::with_capacity(STAGES);
        for _ in 0..STAGES {
            stages.push(StageResult::read(r)?);
        }
        Ok(Self {
            stages: stages.try_into().unwrap(),
            f168: r.f32()?,
            w16c: r.u32()?,
            f170: r.f32()?,
            false_starts: r.array::<1>()?[0],
            pad: r.array()?,
        })
    }

    fn write(&self, w: &mut Writer) {
        self.stages.iter().for_each(|s| s.write(w));
        w.f32(self.f168);
        w.u32(self.w16c);
        w.f32(self.f170);
        w.bytes(&[self.false_starts]);
        w.bytes(&self.pad);
    }
}

impl StageResult {
    fn read(r: &mut Reader) -> Result<Self, Error> {
        Ok(Self {
            sectors: [r.f32()?, r.f32()?, r.f32()?],
            time: r.f32()?,
            elems: [r.words()?, r.words()?, r.words()?, r.words()?],
            b30: r.array()?,
            f34: r.f32()?,
            retired: r.u32()?,
        })
    }

    fn write(&self, w: &mut Writer) {
        self.sectors.iter().for_each(|&s| w.f32(s));
        w.f32(self.time);
        self.elems.iter().for_each(|e| w.words(e));
        w.bytes(&self.b30);
        w.f32(self.f34);
        w.u32(self.retired);
    }
}

pub(crate) fn c_str(b: &[u8]) -> String {
    let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    String::from_utf8_lossy(&b[..end]).into_owned()
}

#[cfg(test)]
pub(crate) fn sample() -> Season {
    let stage = |k: u32| StageResult {
        sectors: [k as f32, k as f32 + 0.5, 2.25],
        time: 100.0 + k as f32,
        elems: [[k, 1], [2, 3], [4, 5], [6, 7]],
        b30: [2, 0, 0, 0],
        f34: 0.5,
        retired: k & 1,
    };
    let rally = |k: u32| DriverRally {
        stages: std::array::from_fn(|s| stage(k * 10 + s as u32)),
        f168: 600.0,
        w16c: 10,
        f170: 1.5,
        false_starts: 3,
        pad: [0; 3],
    };
    let driver = |k: u32| {
        let mut name = [0; 32];
        name[..6].copy_from_slice(b"Driver");
        Driver {
            rallies: std::array::from_fn(|r| rally(k * 100 + r as u32)),
            b8d0: [1, 0, 0, 0],
            id: k as i32 - 1,
            w8d8: 0,
            name,
            w8fc: 7,
            w900: 8,
            car: k,
            team: 9,
        }
    };
    Season {
        b04: 2,
        state: std::array::from_fn(|k| k as u32),
        drivers: (0..DRIVERS as u32).map(driver).collect(),
        teams: [0, 1, 2, 3, 4, 5, 6],
        w7ecc: [5; 14],
        blocks: (0..0xf48u32).map(|k| k as u8).collect(),
        w8e4c: [6; 14],
        globals: [54, 3, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1],
        snapshot: [9; 14],
        events: vec![Event { id: 1, stage: 2, rally: 3, ai: 4, time: 5, kind: 6 }],
        events_b: vec![],
        pairs: vec![[7, 8], [9, 10]],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn season_round_trips() {
        let season = sample();
        let mut w = Writer::default();
        season.write(&mut w);
        // Magic, version, byte, 14 words, the drivers and the fixed tables, globals, snapshot, three lists.
        let fixed = 4 * 17 + 14 * 0x90c + 0x1c + 0x38 + 0xf48 + 0x38 + 4 * 13 + 4 * 14;
        assert_eq!(w.out.len(), fixed + 4 + (4 + 24) + 4 + 4 + (4 + 16));
        let back = Season::read(&mut Reader::new(&w.out)).unwrap();
        assert_eq!(back, season);
        assert_eq!(back.position(), (1, 2));
        assert_eq!(back.drivers[0].name(), "Driver");
    }

    #[test]
    fn the_options_live_in_the_level_word_and_the_menu_words() {
        let mut s = sample();
        s.set_options(2, 1);
        assert_eq!((s.rally_level(), s.damage_level()), (2, 1));
        assert_eq!((s.state[0], s.globals[7], s.snapshot[7], s.snapshot[12]), (2, 2, 2, 1));
        let mut w = Writer::default();
        s.write(&mut w);
        let back = Season::read(&mut Reader::new(&w.out)).unwrap();
        assert_eq!((back.rally_level(), back.damage_level()), (2, 1));
    }

    #[test]
    fn stage_times_follow_the_setters() {
        let mut s = sample();
        let total = s.drivers[3].rallies[1].f168;
        assert!(s.set_stage_time(2, 250.5));
        assert!(s.set_sector(2, 7, 80.25));
        let rally = &s.drivers[3].rallies[1];
        assert_eq!((rally.stages[2].time, rally.stages[2].sectors[2], rally.f168), (250.5, 80.25, total + 250.5));
        assert!(s.set_stage_time(2, RETIRED));
        assert_eq!((s.drivers[3].rallies[1].stages[2].retired, s.drivers[3].rallies[1].f168), (1, RETIRED));
        assert!(!s.set_stage_time(99, 1.0));
    }

    #[test]
    fn wrong_version_is_an_error() {
        let mut w = Writer::default();
        sample().write(&mut w);
        w.out[4] = 0x12;
        assert_eq!(Season::read(&mut Reader::new(&w.out)), Err(Error::Version { found: 0x12, want: VERSION }));
    }
}
