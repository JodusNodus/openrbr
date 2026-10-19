//! What the profile stores after the season's lists (`SeasonSave::rest`): the season's stores and its closing tag
//! (`CAISeason` write 0x5d3e40 from 0x5d480d), then `E20WriteProfile`'s (0x49ec70) word 0xfffffffe, the setup slot
//! (`CCarSettingsManager::GetSlot`), the damage block (`E19_O::M567930`, read by `CGameflow::ReadDamageHandler`
//! 0x567b90) and the car setup (`CCarSettingsManager::ApplySetup` 0x5473d0 → `WriteToBinaryStream` 0x48e0e0), then the
//! profile's end tag.

use super::season::Season;
use super::{Error, Reader, Writer};

const STORES_END: [u8; 4] = *b"ASOE";
const MARK: u32 = 0xFFFF_FFFE;
const DAMAGE: [u8; 4] = *b"HGMD";
const DAMAGE_VERSION: u32 = 3;
const SETUP: [u8; 4] = *b"PTSC";
const END: [u8; 4] = *b"PFOE";
/// The words `CCarDamageHandler::WriteBinary` (0x4edfe0) writes, its own tags included.
pub const HANDLER_WORDS: usize = 490;

/// The season's stores (the write's three loops from 0x5d480d; the read 0x5d5330 loads them back in the same order).
#[derive(Clone, Debug, PartialEq)]
pub struct Stores {
    /// One per entry whose id is not negative, in entry order: the AI driver with the entry's id (`At30c`).
    pub drivers: Vec<DriverStore>,
    /// One per entry: the AI car instance of the entry's +0x904 (`At324`).
    pub cars: Vec<CarStore>,
    /// The seven teams of the season's +0x8 (`At318`).
    pub teams: [TeamStore; 7],
}

/// `E80Team::Store` (0x5cd510) / `Load` (0x5cd660): an AI driver's injuries and temperament.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DriverStore {
    /// +0xc4, +0xc8: the driver's injury (0-7) and its length.
    pub injury: u32,
    pub injury_len: u32,
    /// +0xcc, +0xd0: the co-driver's.
    pub co_injury: u32,
    pub co_injury_len: u32,
    /// +0xa0: the temperament (`ai.ini`'s five; `SetA0Rnd` 0x5cd4b0 changes it on a manager's order).
    pub temperament: u32,
}

/// `E80AIEntry::StoreAB` (0x5ccef0) / `LoadAB` (0x5ccf40): an AI car's eight part states (+0x50) and hit counts
/// (+0x70).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CarStore {
    pub parts: [u32; 8],
    pub hits: [u32; 8],
}

/// `E78Entry::Store` (0x5d6b40) / `Load` (0x5d6d40): a team.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TeamStore {
    /// +0x80: the budget (`aidrv.ini`).
    pub budget: u32,
    /// +0x84: the last service's repair cost (`Zero84` 0x5d6ad0 before each, `Add8488` 0x5d6a00).
    pub service_cost: u32,
    /// +0x88: the rally's repair cost (`Zero8488` at the rally's start).
    pub rally_cost: u32,
    /// +0x8c: the manager style (`aidrv.ini`).
    pub manager_style: u32,
    /// +0x98, +0x9c: the team's two driver ids (`SetupAI` 0x5d1080, `Set98`).
    pub drivers: [u32; 2],
    /// +0xa8, +0xac: their car instances (`SetA8`).
    pub cars: [u32; 2],
}

impl DriverStore {
    fn words(&self) -> [u32; 5] {
        [self.injury, self.injury_len, self.co_injury, self.co_injury_len, self.temperament]
    }

    fn read(r: &mut Reader) -> Result<Self, Error> {
        let [injury, injury_len, co_injury, co_injury_len, temperament] = r.words()?;
        Ok(Self { injury, injury_len, co_injury, co_injury_len, temperament })
    }
}

impl CarStore {
    fn read(r: &mut Reader) -> Result<Self, Error> {
        Ok(Self { parts: r.words()?, hits: r.words()? })
    }
}

impl TeamStore {
    fn words(&self) -> [u32; 8] {
        let [d0, d1] = self.drivers;
        let [c0, c1] = self.cars;
        [self.budget, self.service_cost, self.rally_cost, self.manager_style, d0, d1, c0, c1]
    }

    fn read(r: &mut Reader) -> Result<Self, Error> {
        let [budget, service_cost, rally_cost, manager_style, d0, d1, c0, c1] = r.words()?;
        Ok(Self { budget, service_cost, rally_cost, manager_style, drivers: [d0, d1], cars: [c0, c1] })
    }
}

/// The damage block (`M567930` with `a` 0, `b` 0).
#[derive(Clone, Debug, PartialEq)]
pub struct DamageBlock {
    /// `WriteBinary`'s words.
    pub handler: Vec<u32>,
    /// The gameflow's byte +0x17838 (as a word).
    pub b17838: u32,
    /// The player's tyre (gameflow +0x17834, `M5662a0`; 0-6).
    pub tyre: u32,
    /// The gameflow's word +0x1783c.
    pub w1783c: u32,
    /// `0x1660814`.
    pub w1660814: u32,
}

/// Everything after the season's lists.
#[derive(Clone, Debug, PartialEq)]
pub struct AfterSeason {
    pub stores: Stores,
    pub setup_slot: u32,
    pub damage: DamageBlock,
    /// The setup's text as the stream holds it (`E16_BuildSetup` 0x48d850: `(`, the setup, `)`, zero-padded to words).
    pub setup: Vec<u8>,
}

impl AfterSeason {
    /// Reads the bytes after `season`'s lists (`SeasonSave::rest`).
    pub fn parse(season: &Season, rest: &[u8]) -> Result<Self, Error> {
        let mut r = Reader::new(rest);
        let drivers =
            season.drivers.iter().filter(|d| d.id >= 0).map(|_| DriverStore::read(&mut r)).collect::<Result<_, _>>()?;
        let cars = season.drivers.iter().map(|_| CarStore::read(&mut r)).collect::<Result<_, _>>()?;
        let mut teams = [TeamStore::default(); 7];
        for t in &mut teams {
            *t = TeamStore::read(&mut r)?;
        }
        r.tag(STORES_END)?;
        if r.u32()? != MARK {
            return Err(Error::Tag { at: r.at() - 4, found: [0; 4], want: MARK.to_le_bytes() });
        }
        let setup_slot = r.u32()?;
        r.tag(DAMAGE)?;
        let version = r.u32()?;
        if version != DAMAGE_VERSION {
            return Err(Error::Version { found: version, want: DAMAGE_VERSION });
        }
        let handler = (0..HANDLER_WORDS).map(|_| r.u32()).collect::<Result<_, _>>()?;
        let damage = DamageBlock { handler, b17838: r.u32()?, tyre: r.u32()?, w1783c: r.u32()?, w1660814: r.u32()? };
        r.tag(DAMAGE)?;
        r.tag(SETUP)?;
        let n = r.u32()? as usize;
        let setup = r.bytes(n)?.to_vec();
        r.tag(END)?;
        Ok(Self { stores: Stores { drivers, cars, teams }, setup_slot, damage, setup })
    }

    /// The bytes [`Self::parse`] reads, ending with the profile's end tag.
    pub fn write(&self) -> Vec<u8> {
        let mut w = Writer::default();
        self.stores.drivers.iter().for_each(|d| w.words(&d.words()));
        for c in &self.stores.cars {
            w.words(&c.parts);
            w.words(&c.hits);
        }
        self.stores.teams.iter().for_each(|t| w.words(&t.words()));
        w.bytes(&STORES_END);
        w.u32(MARK);
        w.u32(self.setup_slot);
        w.bytes(&DAMAGE);
        w.u32(DAMAGE_VERSION);
        w.words(&self.damage.handler);
        let d = &self.damage;
        w.words(&[d.b17838, d.tyre, d.w1783c, d.w1660814]);
        w.bytes(&DAMAGE);
        w.bytes(&SETUP);
        w.u32(self.setup.len() as u32);
        w.bytes(&self.setup);
        w.bytes(&END);
        w.out
    }
}

/// The setup's stream form (`E16_BuildSetup`): the text in parentheses, zero-padded to a multiple of four.
pub fn setup_stream(text: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len() + 6);
    out.push(b'(');
    out.extend_from_slice(text);
    out.push(b')');
    while out.len() % 4 != 0 {
        out.push(0);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn after_season_round_trips() {
        let season = super::super::season::sample();
        let after = AfterSeason {
            stores: Stores {
                drivers: season
                    .drivers
                    .iter()
                    .filter(|d| d.id >= 0)
                    .map(|_| DriverStore { injury: 1, injury_len: 2, co_injury: 3, co_injury_len: 4, temperament: 0 })
                    .collect(),
                cars: season.drivers.iter().map(|_| CarStore { parts: [1; 8], hits: [2; 8] }).collect(),
                teams: [TeamStore {
                    budget: 5,
                    service_cost: 6,
                    rally_cost: 7,
                    manager_style: 4,
                    drivers: [0, 1],
                    cars: [0, 1],
                }; 7],
            },
            setup_slot: 0,
            damage: DamageBlock { handler: vec![7; HANDLER_WORDS], b17838: 0, tyre: 4, w1783c: 0, w1660814: 1 },
            setup: setup_stream(b"x"),
        };
        let bytes = after.write();
        assert_eq!(&bytes[bytes.len() - 4..], b"PFOE");
        assert_eq!(after.setup, b"(x)\0");
        assert_eq!(AfterSeason::parse(&season, &bytes), Ok(after));
        assert!(AfterSeason::parse(&season, b"PFOE").is_err());
    }
}
