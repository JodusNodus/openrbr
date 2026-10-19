//! Reader for the championship's AI files in `Root/misc.rbz`: `Misc/ai/ai.ini` (the AI master tables) and
//! `Misc/ai/aidrv.ini` (the AI drivers, teams and cars). `CApplication` loads both at start-up
//! (`E79SeasonHost::LoadMaster` 0x5caa70, `LoadTeams` 0x5cb670).
//!
//! The loaders cut the text at the first `$`, skip `;` comments and read one value per token in file order; names are
//! whole lines. The other files in `Misc/ai/` (`acci*.ini`, `weat*.ini`, `w_*.ini`) are not named anywhere in the 1.02
//! exe and are not read by it.
//!
//! Layout of the master object (offsets the season code reads, see `rbr-gameplay::ai_season`):
//! - +0x0: the mistake table, 5 rows (the check value 1-5) x 18 columns (the skill sum 1-18), stored column-major
//!   (`grid[col * 5 + row]`);
//! - +0x168 skill modifiers (5), +0x17c par low (5), +0x190 par high (5), +0x1a4 driver injury bands (8), +0x1c4
//!   co-driver injury bands (8), +0x1e4 the chance to ignore damage to a tech speciality, +0x1e8 / +0x208 the repair
//!   values and times (8 each, interleaved in the file), +0x228 blow limits (3), +0x234 / +0x240 breakdown base and
//!   durability modifier (3 each, interleaved), +0x24c accident chance (5), +0x260 inspired chance (5), +0x274 mood
//!   resistance (5 x 5); then the accident tables (+0x2dc), the mistake types (+0x2f8), the car damage penalties (+0x2fc,
//!   `CAICarDmgPenaltyTable` 0x5ca940) and the tracks (+0x304, 0x5c bytes each).
//!
//! Times written `mm:ss:hh` (`E57Tokenizer::Parse` 0x55b860) are `mm * 60 + hh * 0.01 + ss`.

/// Columns of the mistake table (the skill sum 1-18).
pub const MISTAKE_COLUMNS: usize = 18;

#[derive(Debug, Clone, PartialEq)]
pub struct Master {
    /// `mistakes[row][col]`: the check value `row + 1` and the skill sum `col + 1`.
    pub mistakes: [[i32; MISTAKE_COLUMNS]; 5],
    pub skill_mods: [i32; 5],
    pub par_low: [f32; 5],
    pub par_high: [f32; 5],
    pub driver_injury: [u32; 8],
    pub codriver_injury: [u32; 8],
    /// Percent chance to ignore damage to a part the team has as a tech speciality (+0x1e4).
    pub ignore_speciality: u32,
    pub repair_value: [i32; 8],
    pub repair_time: [i32; 8],
    /// Blow limits for light, medium and severe damage (+0x228).
    pub blow_limits: [u32; 3],
    pub breakdown_base: [i32; 3],
    pub breakdown_durability: [i32; 3],
    pub temperaments: [Temperament; 5],
    pub accident_tables: Vec<AccidentTable>,
    pub mistake_types: Vec<MistakeType>,
    /// `CAICarDmgPenaltyTable`: (damage sum, time penalty).
    pub damage_penalties: Vec<(u32, f32)>,
    pub tracks: Vec<Track>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Temperament {
    pub accident: f32,
    pub inspired: f32,
    pub mood_resistance: [f32; 5],
}

/// An accident table (0xc bytes: count, the probabilities' total, the entries).
#[derive(Debug, Clone, PartialEq)]
pub struct AccidentTable {
    pub total: u32,
    pub accidents: Vec<Accident>,
}

/// One accident (0x34 bytes, loader 0x5cbec0).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Accident {
    /// The running total of the probabilities up to and including this one.
    pub cumulative: i32,
    /// `ACC_*` string index (+0x4).
    pub string: i32,
    /// Damaged parts in order of probability (+0x8).
    pub parts: [i32; 8],
    /// +0x28: only light damage.
    pub light_only: bool,
    /// +0x2c, +0x30: the mistake type range.
    pub low: i32,
    pub high: i32,
}

/// One mistake type (0x20 bytes, loader 0x5cc230).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MistakeType {
    /// The highest number receiving this type.
    pub highest: u32,
    pub string: i32,
    /// +0x8, +0xc: the time penalty range; negative is a withdrawal.
    pub low: f32,
    pub high: f32,
    pub light: u32,
    pub damage: u32,
    pub heavy: u32,
    pub injury: i32,
}

/// A track's AI data (`E81Arr`, 0x5c bytes).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Track {
    /// The map number (+0x0).
    pub stage: u32,
    /// +0x4: 1 wet; the season sets it from the stage's weather before each run.
    pub wet: u32,
    pub sections: [Section; 3],
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Section {
    /// +0x8: the section's par time (the difference of the file's running par times).
    pub par: f32,
    /// +0x14: 0 gravel, 1 tarmac, 2 ice and snow.
    pub material: u32,
    /// +0x20, +0x38: two (type, check value) pairs; type 0 curves, 1 rough, 2 obstacles, 3 straights.
    pub checks: [(u32, u32); 2],
    /// +0x50: the accident table.
    pub accident_table: usize,
}

/// `aidrv.ini`: the drivers, teams and cars.
#[derive(Debug, Clone, PartialEq)]
pub struct Roster {
    pub drivers: Vec<Driver>,
    pub teams: Vec<Team>,
    /// The cars. `LoadTeams` doubles the file's count and loads each car into two instances (`2c`, `2c + 1`; its
    /// loop runs to the doubled count `>> 1`), one for each of a team's drivers.
    pub cars: Vec<Car>,
}

/// An AI driver (`E80Team`, 0xd4 bytes, parser 0x5cd890).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Driver {
    pub name: String,
    pub photo: i32,
    /// +0x9c.
    pub technique: i32,
    /// +0xa0: 0 aggressive, 1 defensive, 2 passive, 3 reckless, 4 perfectionist.
    pub temperament: i32,
    /// +0xa4: gravel, tarmac, ice and snow, wet.
    pub skills: [i32; 4],
    /// +0xb4.
    pub speed: i32,
    /// +0xb8.
    pub endurance: i32,
    /// +0xbc: 9 best, 1 worst.
    pub condition: i32,
    pub history: i32,
    pub nationality: i32,
    pub codriver: String,
}

/// A team (`E78Entry`, 0xe8 bytes, parser 0x5d6e70).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Team {
    pub name: String,
    pub budget: i32,
    pub manager_style: i32,
    pub mechanics: i32,
    pub engineering: i32,
    /// +0xbc, +0xc0: car parts the team is good at.
    pub specialities: Vec<i32>,
    pub difficulties: Vec<i32>,
    pub history: i32,
    pub nationality: i32,
    pub manager: String,
}

/// An AI car (`E80AIEntry`, 0x94 bytes, parser 0x5cd020).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Car {
    pub name: String,
    /// +0x40, +0x44, +0x48, +0x4c.
    pub control: i32,
    pub power: i32,
    pub handling: i32,
    pub durability: i32,
}

/// The text the loaders see: up to the first `$`, as lines with the `;` comments cut.
struct Cursor<'a> {
    lines: Vec<&'a str>,
    line: usize,
    word: usize,
}

impl<'a> Cursor<'a> {
    fn new(text: &'a str) -> Self {
        let text = text.split('$').next().unwrap_or("");
        let lines = text.lines().map(|l| l.split(';').next().unwrap_or("")).collect();
        Cursor { lines, line: 0, word: 0 }
    }

    fn token(&mut self) -> Result<&'a str, String> {
        while let Some(line) = self.lines.get(self.line) {
            if let Some(w) = line.split_whitespace().nth(self.word) {
                self.word += 1;
                return Ok(w);
            }
            self.line += 1;
            self.word = 0;
        }
        Err(format!("unexpected end at line {}", self.line + 1))
    }

    fn int(&mut self) -> Result<i32, String> {
        Ok(atoi(self.token()?))
    }

    fn time(&mut self) -> Result<f32, String> {
        Ok(parse_time(self.token()?))
    }

    /// A name: the next non-empty line, whole.
    fn name(&mut self) -> Result<String, String> {
        if self.word > 0 {
            self.line += 1;
            self.word = 0;
        }
        while let Some(line) = self.lines.get(self.line) {
            self.line += 1;
            let t = line.trim();
            if !t.is_empty() {
                return Ok(t.to_string());
            }
        }
        Err("unexpected end before a name".into())
    }
}

/// `E57Tokenizer::Parse` (0x55b860): `mm:ss:hh` as `mm * 60 + hh * 0.01 + ss` (x87, rounded once to f32), else `atof`.
pub fn parse_time(word: &str) -> f32 {
    let parts: Vec<&str> = word.split(':').collect();
    if let [m, s, h] = parts[..] {
        if !m.is_empty() && !s.is_empty() {
            return (atof(m) * 60.0 + atof(h) * f64::from(0.01f32) + atof(s)) as f32;
        }
    }
    atof(word) as f32
}

fn atof(word: &str) -> f64 {
    let end = word
        .char_indices()
        .find(|&(k, c)| !(c.is_ascii_digit() || c == '.' || (k == 0 && (c == '-' || c == '+'))))
        .map_or(word.len(), |(k, _)| k);
    word[..end].parse().unwrap_or(0.0)
}

fn atoi(word: &str) -> i32 {
    let (sign, digits) = match word.strip_prefix('-') {
        Some(rest) => (-1, rest),
        None => (1, word.strip_prefix('+').unwrap_or(word)),
    };
    let v = digits.bytes().take_while(u8::is_ascii_digit).fold(0i32, |v, d| v.wrapping_mul(10) + i32::from(d - b'0'));
    sign * v
}

/// `ai.ini` (`LoadMaster` 0x5caa70).
pub fn parse_master(text: &str) -> Result<Master, String> {
    let mut c = Cursor::new(text);
    let mut mistakes = [[0; MISTAKE_COLUMNS]; 5];
    for row in mistakes.iter_mut() {
        for v in row.iter_mut() {
            *v = c.int()?;
        }
    }
    let ints = |c: &mut Cursor, out: &mut [i32]| -> Result<(), String> {
        out.iter_mut().try_for_each(|v| c.int().map(|x| *v = x))
    };
    let mut skill_mods = [0; 5];
    ints(&mut c, &mut skill_mods)?;
    let mut par_low = [0.0; 5];
    let mut par_high = [0.0; 5];
    for v in par_low.iter_mut().chain(par_high.iter_mut()) {
        *v = c.time()?;
    }
    let mut driver_injury = [0; 8];
    let mut codriver_injury = [0; 8];
    for v in driver_injury.iter_mut().chain(codriver_injury.iter_mut()) {
        *v = c.int()? as u32;
    }
    let ignore_speciality = c.int()? as u32;
    let (mut repair_value, mut repair_time) = ([0; 8], [0; 8]);
    for k in 0..8 {
        repair_value[k] = c.int()?;
        repair_time[k] = c.int()?;
    }
    let mut blow_limits = [0; 3];
    for v in blow_limits.iter_mut() {
        *v = c.int()? as u32;
    }
    let (mut breakdown_base, mut breakdown_durability) = ([0; 3], [0; 3]);
    for k in 0..3 {
        breakdown_base[k] = c.int()?;
        breakdown_durability[k] = c.int()?;
    }
    let mut temperaments = [Temperament { accident: 0.0, inspired: 0.0, mood_resistance: [0.0; 5] }; 5];
    for t in temperaments.iter_mut() {
        t.accident = c.time()?;
        t.inspired = c.time()?;
        for v in t.mood_resistance.iter_mut() {
            *v = c.time()?;
        }
    }
    let tables = c.int()?.max(0);
    let mut accident_tables = Vec::new();
    for _ in 0..tables {
        let n = c.int()?.max(0);
        let mut total = 0u32;
        let mut accidents = Vec::new();
        for _ in 0..n {
            let cumulative = c.int()?.wrapping_add(total as i32);
            total = cumulative as u32;
            let string = c.int()?;
            let mut parts = [0; 8];
            ints(&mut c, &mut parts)?;
            let light_only = c.int()? != 0;
            let (low, high) = (c.int()?, c.int()?);
            accidents.push(Accident { cumulative, string, parts, light_only, low, high });
        }
        accident_tables.push(AccidentTable { total, accidents });
    }
    let n = c.int()?.max(0);
    let mut mistake_types = Vec::new();
    for _ in 0..n {
        mistake_types.push(MistakeType {
            highest: c.int()? as u32,
            string: c.int()?,
            low: c.time()?,
            high: c.time()?,
            light: c.int()? as u32,
            damage: c.int()? as u32,
            heavy: c.int()? as u32,
            injury: c.int()?,
        });
    }
    let n = c.int()?.max(0);
    let mut damage_penalties = Vec::new();
    for _ in 0..n {
        damage_penalties.push((c.int()? as u32, c.time()?));
    }
    let n = c.int()?.max(0);
    let mut tracks = Vec::new();
    for _ in 0..n {
        let stage = c.int()? as u32;
        let wet = c.int()? as u32;
        let mut sections = [Section { par: 0.0, material: 0, checks: [(0, 0); 2], accident_table: 0 }; 3];
        let mut before = 0.0f32;
        for s in sections.iter_mut() {
            let par = c.time()?;
            s.material = c.int()? as u32;
            s.par = par - before;
            before = par;
            for k in 0..2 {
                s.checks[k] = (c.int()? as u32, c.int()? as u32);
            }
            s.accident_table = c.int()? as usize;
        }
        tracks.push(Track { stage, wet, sections });
    }
    Ok(Master {
        mistakes,
        skill_mods,
        par_low,
        par_high,
        driver_injury,
        codriver_injury,
        ignore_speciality,
        repair_value,
        repair_time,
        blow_limits,
        breakdown_base,
        breakdown_durability,
        temperaments,
        accident_tables,
        mistake_types,
        damage_penalties,
        tracks,
    })
}

/// `aidrv.ini` (`LoadTeams` 0x5cb670).
pub fn parse_roster(text: &str) -> Result<Roster, String> {
    let mut c = Cursor::new(text);
    let n = c.int()?.max(0);
    let mut drivers = Vec::new();
    for _ in 0..n {
        let name = c.name()?;
        let photo = c.int()?;
        let technique = c.int()?;
        let temperament = c.int()?;
        let skills = [c.int()?, c.int()?, c.int()?, c.int()?];
        drivers.push(Driver {
            name,
            photo,
            technique,
            temperament,
            skills,
            speed: c.int()?,
            endurance: c.int()?,
            condition: c.int()?,
            history: c.int()?,
            nationality: c.int()?,
            codriver: c.name()?,
        });
    }
    let n = c.int()?.max(0);
    let mut teams = Vec::new();
    for _ in 0..n {
        let name = c.name()?;
        let budget = c.int()?;
        let manager_style = c.int()?;
        let mechanics = c.int()?;
        let engineering = c.int()?;
        let k = c.int()?.max(0);
        let specialities = (0..k).map(|_| c.int()).collect::<Result<_, _>>()?;
        let k = c.int()?.max(0);
        let difficulties = (0..k).map(|_| c.int()).collect::<Result<_, _>>()?;
        teams.push(Team {
            name,
            budget,
            manager_style,
            mechanics,
            engineering,
            specialities,
            difficulties,
            history: c.int()?,
            nationality: c.int()?,
            manager: c.name()?,
        });
    }
    let n = c.int()?.max(0);
    let mut cars = Vec::new();
    for _ in 0..n {
        let name = c.name()?;
        cars.push(Car { name, control: c.int()?, power: c.int()?, handling: c.int()?, durability: c.int()? });
    }
    Ok(Roster { drivers, teams, cars })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times_are_minutes_seconds_hundredths() {
        assert_eq!(parse_time("01:37:50"), 97.5);
        assert_eq!(parse_time("00:01:50"), 1.5);
        assert_eq!(parse_time("-0.05"), -0.05);
        assert_eq!(parse_time("4.225"), 4.225);
    }

    fn read(data: &crate::data::Data, name: &str) -> String {
        use std::io::Read;
        let mut zip = crate::rbz::open(&data.resolve("Root/misc.rbz").unwrap()).unwrap();
        let mut bytes = Vec::new();
        zip.by_name(name).unwrap().read_to_end(&mut bytes).unwrap();
        String::from_utf8_lossy(&bytes).into_owned()
    }

    /// The retail files (`RBR_DATA`).
    #[test]
    fn retail_ai_files_parse() {
        let Some(data) = crate::data::Data::from_env() else { return };
        let m = parse_master(&read(&data, "Misc/ai/ai.ini")).unwrap();
        assert_eq!(m.mistakes[0][..4], [20, 15, 10, 9]);
        assert_eq!(m.mistakes[4][17], 1);
        assert_eq!(m.skill_mods, [-3, -1, 0, 1, 3]);
        assert_eq!(m.par_high[0], 4.225);
        assert_eq!(m.driver_injury, [0, 8, 11, 14, 16, 18, 19, 20]);
        assert_eq!(m.ignore_speciality, 50);
        assert_eq!((m.repair_value[7], m.repair_time[7]), (10, 15));
        assert_eq!((m.breakdown_base, m.breakdown_durability), ([20, 70, 100], [1, 10, 10]));
        assert_eq!(m.temperaments[3].accident, 2.0);
        assert_eq!(m.accident_tables.len(), 1);
        assert_eq!((m.accident_tables[0].total, m.accident_tables[0].accidents.len()), (30, 30));
        assert_eq!(m.mistake_types.len(), 11);
        assert_eq!(m.mistake_types[10].low, -1.0);
        assert_eq!(m.damage_penalties.len(), 11);
        assert_eq!(m.damage_penalties[10], (10, 11.5));
        assert_eq!(m.tracks.len(), 36);
        assert_eq!(m.tracks[0].stage, 10);
        assert_eq!(m.tracks[0].sections[0].par, 97.5);
        assert_eq!(m.tracks[0].sections[1].par, 195.17 - 97.5);
        assert_eq!(m.tracks.last().unwrap().stage, 66);
        let r = parse_roster(&read(&data, "Misc/ai/aidrv.ini")).unwrap();
        assert_eq!(r.drivers.len(), 14);
        assert_eq!((r.drivers[0].name.as_str(), r.drivers[0].codriver.as_str()), ("Pablo Ramirez", "Rodrigo Perez"));
        assert_eq!(r.drivers[13].name, "Dimitri Ergma");
        assert_eq!(r.teams.len(), 7);
        assert_eq!(r.teams[4].specialities, [4, 1]);
        assert_eq!(r.teams[3].difficulties, [5, 3]);
        assert_eq!(r.teams[5].manager, "Patricia Goodman");
        assert_eq!(r.cars.len(), 8);
        assert_eq!(r.cars[3].durability, 5);
    }
}
