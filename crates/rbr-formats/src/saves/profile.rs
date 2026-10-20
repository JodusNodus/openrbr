//! The profile file `SavedGames/pf<name>.rbr` (`E20WriteProfile` 0x49ec70, `E20ReadProfile` 0x49ea80): 0x26000
//! bytes, encrypted, checksummed, version 5. It holds the menu settings, the keyboard controls, the driver profiles,
//! the input filter grid, the camera modes and, while a championship is under way, the season.
//!
//! Field names are the member offsets in the original objects where the meaning is not traced; see
//! `notes/formats.md` "Saves and times".

use super::season::Season;
use super::{decrypt, encrypt, seal, unseal, Error, Reader, Writer};

pub const SIZE: usize = 0x26000;
pub const VERSION: u32 = 5;
/// `SMenuSettings` stream version (`E66MenuSettings::Read` 0x586410 refuses others).
pub const MENU_VERSION: u32 = 3;
/// The profile's per-entry marker (`E69Profile::DeserializeA` 0x59a4d0 refuses others).
const PROFILE_MARKER: u32 = 100;
/// Profiles the manager holds (`E69ProfileManager`, 4 x 0x46c).
pub const MAX_PROFILES: usize = 4;
const END: [u8; 4] = *b"PFOE";

/// A whole profile file.
#[derive(Clone, Debug, PartialEq)]
pub struct ProfileFile {
    /// `M00565470` at save time: the game mode (3 is the championship).
    pub mode: u32,
    pub menu: MenuSettings,
    pub controls: Controls,
    pub profiles: Profiles,
    pub filter: InputFilter,
    /// `PMAC` (`M00567d40`): four words at gameflow +0xc98 -> +0x17840.
    pub camera: [u32; 4],
    /// Present when a season was under way (the word after the camera block is 1).
    pub season: Option<SeasonSave>,
}

/// The season and everything the profile stores after it.
#[derive(Clone, Debug, PartialEq)]
pub struct SeasonSave {
    pub season: Season,
    /// The bytes after [`Season`]'s typed part up to the end tag, kept as they are: the team, AI driver and rally
    /// stores (variable length), then the word 0xfffffffe, the car setup slot, the damage carried
    /// (`CGameflow::ReadDamageHandler`) and the setups (`CCarSettingsManager::LoadStream`), then `EOFP`.
    pub rest: Vec<u8>,
}

impl ProfileFile {
    /// Reads a file as the game wrote it (encrypted).
    pub fn read(encrypted: &[u8]) -> Result<Self, Error> {
        let mut plain = encrypted.to_vec();
        decrypt(&mut plain);
        Self::read_plain(&plain)
    }

    /// The file's bytes, encrypted, as the game writes them.
    pub fn write(&self) -> Result<Vec<u8>, Error> {
        let mut out = self.write_plain()?;
        encrypt(&mut out);
        Ok(out)
    }

    pub fn read_plain(plain: &[u8]) -> Result<Self, Error> {
        let mut r = unseal(plain, SIZE)?;
        let version = r.u32()?;
        if version != VERSION {
            return Err(Error::Version { found: version, want: VERSION });
        }
        let mode = r.u32()?;
        let menu = MenuSettings::read(&mut r)?;
        let controls = Controls::read(&mut r)?;
        let profiles = Profiles::read(&mut r)?;
        let filter = InputFilter::read(&mut r)?;
        r.tag(*b"PMAC")?;
        let camera = r.words()?;
        // The reader stops at a zero word without looking at the end tag.
        let season = match r.u32()? {
            0 => None,
            _ => {
                let season = Season::read(&mut r)?;
                let rest = r.rest();
                let used = rest.iter().rposition(|&b| b != 0).map_or(0, |k| k + 1);
                Some(SeasonSave { season, rest: rest[..used].to_vec() })
            }
        };
        Ok(Self { mode, menu, controls, profiles, filter, camera, season })
    }

    pub fn write_plain(&self) -> Result<Vec<u8>, Error> {
        let mut w = Writer::default();
        w.u32(0);
        w.u32(VERSION);
        w.u32(self.mode);
        self.menu.write(&mut w);
        self.controls.write(&mut w);
        self.profiles.write(&mut w);
        self.filter.write(&mut w);
        w.bytes(b"PMAC");
        w.words(&self.camera);
        match &self.season {
            None => {
                w.u32(0);
                w.bytes(&END);
            }
            Some(s) => {
                w.u32(1);
                s.season.write(&mut w);
                w.bytes(&s.rest);
            }
        }
        seal(w.out, SIZE)
    }

    /// The profile the manager has selected, if any.
    pub fn current(&self) -> Option<&Profile> {
        self.profiles.list.get(self.profiles.current as usize)
    }
}

/// `E66MenuSettings::Write` (0x5862f0): version 3, then the eleven blocks the game writes, in its order. The
/// career, memory unit, multiplayer and replay settings write nothing; season, physics debug and developer settings
/// are not called. The readers clamp some values (`SQuickRaceSettings::Read` limits `w58` to 3); this reads them raw.
#[derive(Clone, Debug, PartialEq)]
pub struct MenuSettings {
    pub quick_race: QuickRace,
    pub options: Options,
    pub dual_shock: u32,
    pub controller: [u32; 6],
    pub pacenotes: Pacenotes,
    pub display: Display,
    pub camera: Camera,
    pub sound: Sound,
    pub gear: Gear,
    pub force_feedback: ForceFeedback,
    pub pc_controller: [u32; 2],
}

/// `SQuickRaceSettings` (0x586fc0): +0x58, +0x60, then the tag `QSET` last.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct QuickRace {
    pub w58: u32,
    pub w60: u32,
}

/// `SOptionsPageSettings` (0x586cc0): `OPTI`, +0x10, +0x14, +0xc.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Options {
    pub w10: u32,
    pub w14: u32,
    pub w0c: u32,
}

/// `SPacenoteSettings` (0x588420): `PACE`, +0xc..+0x30, +0x38, +0x34 (float), +0x3c, +0x40 (float). The names are the
/// options page's keys for the word each item edits (0x5962c4-0x596877; English texts from `bstrings.eng`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pacenotes {
    /// +0xc, `PN_STACK` ("Pacenote Stack").
    pub stack: u32,
    /// +0x10, `CHKP_TIMES` ("Checkpoint Times"): the split panel.
    pub checkpoint_times: u32,
    /// +0x14, `IG_CLOCK` ("In-Game Clock").
    pub clock: u32,
    pub w18: u32,
    /// +0x1c, `AID_MARKER` ("3D Pacenotes").
    pub markers_3d: u32,
    /// +0x20, `PACENOTES` ("Pacenotes").
    pub pacenotes: u32,
    pub w24: u32,
    /// +0x28, `DISTANCE` ("Distance").
    pub distance: u32,
    /// +0x2c, `DIST_CNTDWN` ("Distance Countdown").
    pub distance_countdown: u32,
    pub w30: u32,
    pub w38: u32,
    /// +0x34, `CALLOUTD` ("Callout Distance"): the marker ticker's lookahead scale (0x1660b74, `Load3b0`).
    pub callout_distance: f32,
    pub w3c: u32,
    /// +0x40: the speed below which the marker ticker calls no pace note (0x1660b80, `Load3b0`); no page edits it.
    pub callout_speed: f32,
}

/// `SDisplaySettings` (0x587480): `DISP`, +0xc..+0x38, +0x3c (float), +0x40, +0x44, +0x50, +0x48, +0x54, +0x58
/// (float).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Display {
    pub w0c: [u32; 12],
    pub f3c: f32,
    /// +0x40, `DIGIDASH` ("Digidash").
    pub digidash: u32,
    /// +0x44, `DIGIDASH_SZ` ("Digidash Size").
    pub digidash_size: u32,
    pub w50: u32,
    pub w48: u32,
    pub w54: u32,
    pub f58: f32,
}

/// `SCameraSettings` (0x587b40): `CAME`, floats +0xc, +0x10, +0x14, +0x18, +0x20, +0x28..+0x34, then +0x1c, +0x24.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    /// +0xc, `CAM_RADIUS` ("Camera Radius").
    pub radius: f32,
    /// +0x10, `CAM_ANGLE` ("Camera Angle").
    pub angle: f32,
    /// +0x14, `VW_ANGLE` ("View Angle").
    pub view_angle: f32,
    pub f18: f32,
    pub f20: f32,
    pub f28: [f32; 4],
    pub w1c: u32,
    pub w24: u32,
}

/// `SSoundSettings` (0x587ff0): `SOUN`, floats +0xc..+0x24, then +0x28, +0x2c, +0x38. `OSND_INCAR` ("Force In-Car
/// Sound", +0x30) is not saved.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sound {
    pub f0c: f32,
    /// +0x10, `ENG_VOL` ("Engine Sound Volume").
    pub engine: f32,
    /// +0x14, `CODRV_VOL` ("Co-Driver Sound Volume").
    pub co_driver: f32,
    /// +0x18, `EFF_VOL` ("Effects Sound Volume").
    pub effects: f32,
    /// +0x1c, `MUS_VOL` ("Music Volume").
    pub music: f32,
    /// +0x20, `TIRE_VOL` ("Tyre Sound Volume").
    pub tyres: f32,
    /// +0x24, `RPLMUS_VOL` ("Replay Music Volume").
    pub replay_music: f32,
    pub w28: u32,
    pub w2c: u32,
    pub w38: u32,
}

/// `SGearSettings` (0x588f50): `GEAR`, +0xc..+0x18.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Gear {
    /// +0xc, `DEF_GEARS` ("Default Gears").
    pub default_gears: u32,
    /// +0x10, `CS_NEUTRALLOCK` ("Neutral Lock").
    pub neutral_lock: u32,
    /// +0x14, `CLUTCH_HLP` ("Clutch Help").
    pub clutch_help: u32,
    /// +0x18, `GEAR_PROT` ("Gear Protection").
    pub gear_protection: u32,
}

/// `SForceFeedBackSettings` (0x5891d0): tagged `GEAR` like the gear block; +0xc, +0x10 (float), +0x14, +0x18.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ForceFeedback {
    /// +0xc, `FRCFEEDBK` ("Force Feedback").
    pub enabled: u32,
    /// +0x10, `STRENGTH` ("Strength").
    pub strength: f32,
    /// +0x14, `DIRECTION` ("Direction").
    pub direction: u32,
    /// +0x18, `FF_AUTOCENTER` ("Autocenter Wheel").
    pub autocenter: u32,
}

fn floats<const N: usize>(r: &mut Reader) -> Result<[f32; N], Error> {
    Ok(r.words::<N>()?.map(f32::from_bits))
}

fn write_floats(w: &mut Writer, f: &[f32]) {
    f.iter().for_each(|&v| w.f32(v));
}

impl MenuSettings {
    fn read(r: &mut Reader) -> Result<Self, Error> {
        let version = r.u32()?;
        if version != MENU_VERSION {
            return Err(Error::Version { found: version, want: MENU_VERSION });
        }
        let quick_race = QuickRace { w58: r.u32()?, w60: r.u32()? };
        r.tag(*b"TESQ")?;
        r.tag(*b"ITPO")?;
        let options = Options { w10: r.u32()?, w14: r.u32()?, w0c: r.u32()? };
        r.tag(*b"LAUD")?;
        let dual_shock = r.u32()?;
        r.tag(*b"TNOC")?;
        let controller = r.words()?;
        r.tag(*b"ECAP")?;
        let [stack, checkpoint_times, clock, w18, markers_3d, pacenotes, w24, distance, distance_countdown, w30] =
            r.words()?;
        let pacenotes = Pacenotes {
            stack,
            checkpoint_times,
            clock,
            w18,
            markers_3d,
            pacenotes,
            w24,
            distance,
            distance_countdown,
            w30,
            w38: r.u32()?,
            callout_distance: r.f32()?,
            w3c: r.u32()?,
            callout_speed: r.f32()?,
        };
        r.tag(*b"PSID")?;
        let display = Display {
            w0c: r.words()?,
            f3c: r.f32()?,
            digidash: r.u32()?,
            digidash_size: r.u32()?,
            w50: r.u32()?,
            w48: r.u32()?,
            w54: r.u32()?,
            f58: r.f32()?,
        };
        r.tag(*b"EMAC")?;
        let [radius, angle, view_angle, f18, f20, f28 @ ..] = floats::<9>(r)?;
        let camera = Camera { radius, angle, view_angle, f18, f20, f28, w1c: r.u32()?, w24: r.u32()? };
        r.tag(*b"NUOS")?;
        let [f0c, engine, co_driver, effects, music, tyres, replay_music] = floats(r)?;
        let sound = Sound {
            f0c,
            engine,
            co_driver,
            effects,
            music,
            tyres,
            replay_music,
            w28: r.u32()?,
            w2c: r.u32()?,
            w38: r.u32()?,
        };
        r.tag(*b"RAEG")?;
        let [default_gears, neutral_lock, clutch_help, gear_protection] = r.words()?;
        let gear = Gear { default_gears, neutral_lock, clutch_help, gear_protection };
        r.tag(*b"RAEG")?;
        let force_feedback =
            ForceFeedback { enabled: r.u32()?, strength: r.f32()?, direction: r.u32()?, autocenter: r.u32()? };
        r.tag(*b"NOCP")?;
        let pc_controller = r.words()?;
        Ok(Self {
            quick_race,
            options,
            dual_shock,
            controller,
            pacenotes,
            display,
            camera,
            sound,
            gear,
            force_feedback,
            pc_controller,
        })
    }

    fn write(&self, w: &mut Writer) {
        w.u32(MENU_VERSION);
        w.words(&[self.quick_race.w58, self.quick_race.w60]);
        w.bytes(b"TESQ");
        w.bytes(b"ITPO");
        w.words(&[self.options.w10, self.options.w14, self.options.w0c]);
        w.bytes(b"LAUD");
        w.u32(self.dual_shock);
        w.bytes(b"TNOC");
        w.words(&self.controller);
        let p = &self.pacenotes;
        w.bytes(b"ECAP");
        w.words(&[
            p.stack,
            p.checkpoint_times,
            p.clock,
            p.w18,
            p.markers_3d,
            p.pacenotes,
            p.w24,
            p.distance,
            p.distance_countdown,
            p.w30,
            p.w38,
        ]);
        w.f32(p.callout_distance);
        w.u32(p.w3c);
        w.f32(p.callout_speed);
        let d = &self.display;
        w.bytes(b"PSID");
        w.words(&d.w0c);
        w.f32(d.f3c);
        w.words(&[d.digidash, d.digidash_size, d.w50, d.w48, d.w54]);
        w.f32(d.f58);
        w.bytes(b"EMAC");
        let c = &self.camera;
        write_floats(w, &[c.radius, c.angle, c.view_angle, c.f18, c.f20]);
        write_floats(w, &c.f28);
        w.words(&[c.w1c, c.w24]);
        w.bytes(b"NUOS");
        let s = &self.sound;
        write_floats(w, &[s.f0c, s.engine, s.co_driver, s.effects, s.music, s.tyres, s.replay_music]);
        w.words(&[s.w28, s.w2c, s.w38]);
        w.bytes(b"RAEG");
        let g = &self.gear;
        w.words(&[g.default_gears, g.neutral_lock, g.clutch_help, g.gear_protection]);
        let f = &self.force_feedback;
        w.bytes(b"RAEG");
        w.u32(f.enabled);
        w.f32(f.strength);
        w.words(&[f.direction, f.autocenter]);
        w.bytes(b"NOCP");
        w.words(&self.pc_controller);
    }
}

/// `CControlConfig` (write 0x55a640, read 0x55a7c0): `CONF`, five words, then the eight keyboard keys (DirectInput
/// key codes, +0xc..+0x28).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Controls {
    pub w04: u32,
    pub w58: u32,
    pub w5c: u32,
    pub w4c: u32,
    pub w50: u32,
    pub keys: [u32; 8],
}

impl Controls {
    fn read(r: &mut Reader) -> Result<Self, Error> {
        r.tag(*b"FNOC")?;
        Ok(Self { w04: r.u32()?, w58: r.u32()?, w5c: r.u32()?, w4c: r.u32()?, w50: r.u32()?, keys: r.words()? })
    }

    fn write(&self, w: &mut Writer) {
        w.bytes(b"FNOC");
        w.words(&[self.w04, self.w58, self.w5c, self.w4c, self.w50]);
        w.words(&self.keys);
    }
}

/// `E69ProfileManager::SerializeB` (0x59a770): `PROF`, the count, the selected index, the profiles.
#[derive(Clone, Debug, PartialEq)]
pub struct Profiles {
    /// +0x11b8. The reader skips it and selects profile 0.
    pub current: u32,
    pub list: Vec<Profile>,
}

/// One driver profile (`E69Profile`, 0x46c bytes; `SerializeA` 0x59a380).
#[derive(Clone, Debug, PartialEq)]
pub struct Profile {
    /// +0x4, NUL-terminated.
    pub name: [u8; 32],
    /// +0x424: the cars' unlock levels.
    pub cars: [u32; 17],
    pub w468: u32,
    /// +0x88: fifteen counters (16-bit, written as words).
    pub counters: [u16; 15],
    /// +0xa8: the number of used [`Profile::records`].
    pub record_count: u32,
    /// +0xac: 37 records of 0x18 bytes, five words written (`InsertTime` 0x599580 fills id, three values, a time).
    pub records: [ProfileRecord; 37],
    /// +0x24: 100 byte flags (`TestSet24`, `Query64`).
    pub flags: [u8; 100],
}

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct ProfileRecord {
    pub id: u32,
    pub w04: u32,
    pub f08: [f32; 3],
}

impl Profiles {
    fn read(r: &mut Reader) -> Result<Self, Error> {
        r.tag(*b"FORP")?;
        let count = r.u32()? as usize;
        if count > MAX_PROFILES {
            return Err(Error::TooLarge { len: count, size: MAX_PROFILES });
        }
        let current = r.u32()?;
        let list = (0..count).map(|_| Profile::read(r)).collect::<Result<_, _>>()?;
        Ok(Self { current, list })
    }

    fn write(&self, w: &mut Writer) {
        w.bytes(b"FORP");
        w.words(&[self.list.len() as u32, self.current]);
        self.list.iter().for_each(|p| p.write(w));
    }
}

impl Profile {
    /// A new profile as `AddProfile` leaves it before play: the name, everything else zero.
    pub fn new(name: &str) -> Self {
        let mut n = [0; 32];
        let len = name.len().min(31);
        n[..len].copy_from_slice(&name.as_bytes()[..len]);
        Self {
            name: n,
            cars: [0; 17],
            w468: 0,
            counters: [0; 15],
            record_count: 0,
            records: [ProfileRecord::default(); 37],
            flags: [0; 100],
        }
    }

    pub fn name(&self) -> String {
        super::season::c_str(&self.name)
    }

    fn read(r: &mut Reader) -> Result<Self, Error> {
        let name = r.array()?;
        let cars = r.words()?;
        let w468 = r.u32()?;
        let counters = r.words::<15>()?.map(|v| v as u16);
        let record_count = r.u32()?;
        let mut records = [ProfileRecord::default(); 37];
        for rec in &mut records {
            *rec = ProfileRecord { id: r.u32()?, w04: r.u32()?, f08: floats(r)? };
        }
        let marker = r.u32()?;
        if marker != PROFILE_MARKER {
            return Err(Error::Version { found: marker, want: PROFILE_MARKER });
        }
        let flags = r.array()?;
        r.tag(*b"FORP")?;
        Ok(Self { name, cars, w468, counters, record_count, records, flags })
    }

    fn write(&self, w: &mut Writer) {
        w.bytes(&self.name);
        w.words(&self.cars);
        w.u32(self.w468);
        self.counters.iter().for_each(|&c| w.u32(c as u32));
        w.u32(self.record_count);
        for rec in &self.records {
            w.words(&[rec.id, rec.w04]);
            write_floats(w, &rec.f08);
        }
        w.u32(PROFILE_MARKER);
        w.bytes(&self.flags);
        w.bytes(b"FORP");
    }
}

/// The input filter grid (`E16Filter::WriteBin` 0x48c0a0, `ReadBin` 0x48bd60): `FILT`, five channels of four cells,
/// `FILE`. The file puts the channel's selector before each of its cells; the game writes the same value four
/// times and keeps the last on reading. Word names follow `rbr-gameplay`'s `filter::Cell`.
#[derive(Clone, Debug, PartialEq)]
pub struct InputFilter {
    pub channels: [FilterChannel; 5],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FilterChannel {
    pub select: u32,
    pub cells: [FilterCell; 4],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FilterCell {
    pub active: u32,
    pub nonlinear: u32,
    pub lin: f32,
    pub c2: f32,
    pub c3: f32,
    pub rise: f32,
    pub fall: f32,
    pub fade: f32,
    pub fade_speed: f32,
}

impl InputFilter {
    fn read(r: &mut Reader) -> Result<Self, Error> {
        r.tag(*b"TLIF")?;
        let mut channel = || -> Result<FilterChannel, Error> {
            let mut select = 0;
            let mut cell = || -> Result<FilterCell, Error> {
                select = r.u32()?;
                let (active, nonlinear) = (r.u32()?, r.u32()?);
                let [lin, c2, c3, rise, fall, fade, fade_speed] = floats(r)?;
                Ok(FilterCell { active, nonlinear, lin, c2, c3, rise, fall, fade, fade_speed })
            };
            let cells = [cell()?, cell()?, cell()?, cell()?];
            Ok(FilterChannel { select, cells })
        };
        let channels = [channel()?, channel()?, channel()?, channel()?, channel()?];
        r.tag(*b"ELIF")?;
        Ok(Self { channels })
    }

    fn write(&self, w: &mut Writer) {
        w.bytes(b"TLIF");
        for ch in &self.channels {
            for c in &ch.cells {
                w.words(&[ch.select, c.active, c.nonlinear]);
                write_floats(w, &[c.lin, c.c2, c.c3, c.rise, c.fall, c.fade, c.fade_speed]);
            }
        }
        w.bytes(b"ELIF");
    }
}

impl ProfileFile {
    /// A new profile file with one driver profile (ours: the menu defaults are the vanilla profile's values where
    /// known, not traced from the menus' constructors). The launcher creates one when `SavedGames` has none.
    pub fn fresh(name: &str) -> ProfileFile {
        // The input filter as a new game profile has it (`E66MenuSettings::ResetAll` 0x586150: `E16_InitGrid`, then
        // `E16_LoadINI`'s built-in values for steering cells 0, 1 and 3, as without `PC_InputFilter.ini`).
        let cell = |active, nonlinear, rise, fall, fade, fade_speed| FilterCell {
            active,
            nonlinear,
            lin: 1.0,
            c2: 0.0,
            c3: 0.0,
            rise,
            fall,
            fade,
            fade_speed,
        };
        let zero = cell(0, 0, 0.0, 0.0, 0.0, 0.0);
        let steering =
            [cell(1, 1, 0.0, 0.0, 0.3, 35.0), cell(1, 1, 0.0, 0.0, 0.5, 27.0), zero, cell(1, 0, 0.3, 0.5, 0.5, 27.0)];
        ProfileFile {
            mode: 3,
            menu: MenuSettings {
                quick_race: QuickRace { w58: 1, w60: 0 },
                options: Options { w10: 0, w14: 1, w0c: 0 },
                dual_shock: 0,
                controller: [0, 1, 1, 1, 1, 1],
                pacenotes: Pacenotes {
                    stack: 1,
                    checkpoint_times: 1,
                    clock: 1,
                    w18: 1,
                    markers_3d: 1,
                    pacenotes: 1,
                    w24: 1,
                    distance: 1,
                    distance_countdown: 1,
                    w30: 1,
                    w38: 1,
                    callout_distance: 4.0,
                    w3c: 3,
                    callout_speed: 25.0,
                },
                display: Display {
                    w0c: [1; 12],
                    f3c: 200.0,
                    digidash: 1,
                    digidash_size: 0,
                    w50: 1,
                    w48: 1,
                    w54: 1,
                    f58: 0.5,
                },
                camera: Camera {
                    radius: 4.4,
                    angle: 13.0,
                    view_angle: 11.0,
                    f18: 0.0,
                    f20: 1.0,
                    f28: [0.4, 75.0, 3.0, 0.8],
                    w1c: 0,
                    w24: 0,
                },
                sound: Sound {
                    f0c: 0.54,
                    engine: 0.9,
                    co_driver: 0.9,
                    effects: 0.9,
                    music: 0.9,
                    tyres: 0.9,
                    replay_music: 0.9,
                    w28: 1,
                    w2c: 2,
                    w38: 0,
                },
                gear: Gear { default_gears: 0, neutral_lock: 0, clutch_help: 1, gear_protection: 1 },
                force_feedback: ForceFeedback { enabled: 1, strength: 0.7, direction: 1, autocenter: 1 },
                pc_controller: [0, 0],
            },
            controls: Controls {
                w04: 0,
                w58: 1,
                w5c: 0,
                w4c: 0,
                w50: 0,
                keys: [0x1e, 0x2c, 0x1f, 0x2d, 0x2f, 0x39, 0xcb, 0xcd],
            },
            profiles: Profiles { current: 0, list: vec![Profile::new(name)] },
            filter: InputFilter {
                channels: std::array::from_fn(|k| FilterChannel {
                    select: 0,
                    cells: if k == 0 { steering } else { [zero; 4] },
                }),
            },
            camera: [4; 4],
            season: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> ProfileFile {
        let mut file = ProfileFile::fresh("tester");
        let profile = &mut file.profiles.list[0];
        profile.cars[3] = 2;
        profile.counters[0] = 7;
        profile.record_count = 1;
        profile.records[0] = ProfileRecord { id: 54, w04: 1, f08: [1.0, 2.0, 123.5] };
        profile.flags = [1; 100];
        file.profiles.list.push(Profile::new("second"));
        for (k, ch) in file.filter.channels.iter_mut().enumerate() {
            ch.select = k as u32 % 2;
            for (c, cell) in ch.cells.iter_mut().enumerate() {
                cell.c2 = c as f32;
            }
        }
        file
    }

    #[test]
    fn profile_round_trips() {
        let file = sample();
        let bytes = file.write().unwrap();
        assert_eq!(bytes.len(), SIZE);
        assert_eq!(ProfileFile::read(&bytes).unwrap(), file);
        assert_eq!(file.current().unwrap().name(), "tester");
    }

    #[test]
    fn season_profile_round_trips() {
        let mut file = sample();
        let mut rest = vec![0xfe, 0xff, 0xff, 0xff, 2, 0, 0, 0, 0, 0, 1, 0];
        rest.extend_from_slice(&END);
        file.season = Some(SeasonSave { season: crate::saves::season::sample(), rest });
        let bytes = file.write().unwrap();
        assert_eq!(ProfileFile::read(&bytes).unwrap(), file);
    }

    #[test]
    fn corruption_is_caught_by_the_checksum() {
        let mut plain = sample().write_plain().unwrap();
        plain[0x40] ^= 1;
        assert!(matches!(ProfileFile::read_plain(&plain), Err(Error::Checksum { .. })));
    }

    /// The profiles the original 1.02 wrote (`pf<name>.rbr`), when `RBR_SAVES` names their directory: each reads and
    /// writes back byte for byte. The maintainers' vanilla profile (`pfthomas.rbr`, copied from the harness install)
    /// also has its settings checked.
    #[test]
    fn original_profile_round_trips() {
        let Some(dir) = std::env::var_os("RBR_SAVES") else {
            eprintln!("skipping: RBR_SAVES not set");
            return;
        };
        let saves = crate::saves::SaveDir { path: dir.into() };
        let names = saves.profiles().unwrap();
        assert!(!names.is_empty(), "no pf<name>.rbr in {}", saves.path.display());
        for name in names {
            let bytes = std::fs::read(saves.path.join(crate::saves::Kind::Profile.file_name(&name))).unwrap();
            let file = ProfileFile::read(&bytes).unwrap();
            assert_eq!(file.write().unwrap(), bytes, "{name}");
            if name == "thomas" {
                check_vanilla_profile(&file);
            }
        }
    }

    fn check_vanilla_profile(file: &ProfileFile) {
        assert_eq!(file.current().unwrap().name(), "thomas");
        assert_eq!(file.controls.keys, [0x1e, 0x2c, 0x1f, 0x2d, 0x2f, 0x39, 0xcb, 0xcd]);
        assert_eq!(file.filter.channels[0].cells[0].fade_speed, 35.0);
        assert_eq!(file.camera, [4; 4]);
        assert!(file.season.is_none());
        // The marker ticker's lookahead scale and speed gate, as the drives' snapshots hold them at 0x1660b74/0x1660b80.
        let m = &file.menu;
        assert_eq!((m.pacenotes.callout_distance, m.pacenotes.callout_speed), (4.0, 25.0));
        assert_eq!((m.pacenotes.checkpoint_times, m.pacenotes.markers_3d), (1, 0));
        assert_eq!((m.camera.radius, m.sound.engine, m.gear.clutch_help), (4.4, 0.9, 1));
        assert_eq!((m.force_feedback.enabled, m.force_feedback.autocenter), (1, 1));
    }
}
