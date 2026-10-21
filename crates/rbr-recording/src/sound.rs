//! The sound record (`SND1`) a live stepping process writes with each frame (`rbr_recording::live`), with what the
//! gameplay sound takes from one physics step (rbr-recomp `notes/sound.md` sections 3-5). Payload, little-endian:
//!
//! ```text
//! u32 engine[12]      the EngineNoise payload: the engine state's words from +0x2c (CCar +0x690 is the CEngine, its +8
//!                     the state); p[0] smoothed engine speed rad/s, p[1] throttle, p[2] turbo speed, p[3] boost,
//!                     p[4] misfire, p[5], p[6] turbo flow, p[7] wastegate, p[8..12] cylinder flags
//! u32 drivetrain[14]  the DrivetrainState payload as CDrive::M8480 (0x508480) builds it at CDrive +0x1a4 (CCar +0x4b0
//!                     is the CDrive): p[0] gearbox speed, p[1] gear teeth (int), p[2] diff speed, p[3] diff teeth
//!                     (int), p[4..7], p[7] |drive +0x37c|, p[8] gear-change flag, p[9], p[10..14] wheel speeds
//!                     while the wheel's driveshaft is broken, else 0
//! f32 speed           the car's speed, m/s (CCar +0x15fc)
//! u32 tyres[4][24]    each wheel's CSigTyreNoise block (the tyre scratch's +0x420, wheel +0xf00 + 0x420; CCar +0x470
//!                     holds the wheels): [0] wheel, [4] scaled grip maximum, [5] tyre factor, [8] |slip ratio|,
//!                     [9] speed, [0xc..] position, [0x10..] velocity, [0x14] surface id, [0x17] contact
//! u32 hit[20]         the damage handler's CSigCarDamage payload (handler +0x3e60; CCar +0x9f4 is the handler): [1]
//!                     state, [4] impact power, [8..] and [0xc..] the two vectors. It stays until the next impact
//! u32 speech_ring     the handler's speech ring index (+0x3e58), stepped by each CSigCarDamageSpeech of RingStore
//! f32 battery         CarSensorData +0x30: the E39 block's scale (CCar +0x620, +0x8c) times 12
//! u32 brakes          CarSensorData +0x2c as the count it is made from (the E39 block's +0x80)
//! f32 water           CarSensorData +0x14: CEngine::SimE970 (0x50e970), filled in by the writer
//! u32 dampers[4][5]   each wheel's damper signal words (the strut's CSpringDamper +0x1c, wheel +0xec4 + 0x1c, what
//!                     CDamperSignal::Emit 0x52de60 sends): [0] owner id, [1] peak, [2] bump, [3] droop velocity,
//!                     [4] valid
//! f32 strut_velocity[4] each wheel's strut velocity (wheel +0x5cc)
//! u32 fires           the step's CSigEngineFire emits, counted by the writer
//! ```
//!
//! The standalone reads these words from memory after the step. `M8480` runs from the frame loop (`CCar::Vf28x`), not
//! the physics step, so the drivetrain words are its reads of the drive, made again here without its writes. The tyre
//! blocks and the damper words are as the step leaves them: the wheels' frame finish (`CWheel::Vf28` 0x53f230, which
//! emits both and then resets them: the tyre block's word 1 and the damper's peak, bump and droop) runs from the frame
//! loop, which the standalone does not run. So the damper's peak is a maximum since the start, and its bump the last
//! bump-stop hit. The tyre block's word 1 is the frame's own: the writer folds each step's `FillOutputs` term
//! ([`TyreMax`]) from the finish's 0, as the frame the game emits it after has it.

use crate::damage::{CAR_DAMAGE_HANDLER, HANDLER_EVENT_CODE};

/// The tag.
pub const SND1: [u8; 4] = *b"SND1";
/// Engine payload words.
pub const ENGINE_WORDS: usize = 12;
/// Drivetrain payload words.
pub const DRIVETRAIN_WORDS: usize = 14;
/// Tyre-noise block words, per wheel.
pub const TYRE_WORDS: usize = 24;
/// `CSigCarDamage` payload words.
pub const HIT_WORDS: usize = 20;
/// Damper payload words.
pub const DAMPER_WORDS: usize = 5;
/// The payload's size.
pub const SND1_BYTES: usize =
    4 * (ENGINE_WORDS + DRIVETRAIN_WORDS + 1 + 4 * TYRE_WORDS + HIT_WORDS + 4 + 4 * DAMPER_WORDS + 4 + 1);

/// `CCar` +0x690: the engine; its +8 the state, whose +0x2c is the EngineNoise payload.
pub const CAR_ENGINE: u32 = 0x690;
pub const ENGINE_STATE: u32 = 0x8;
pub const STATE_NOISE: u32 = 0x2c;
/// `CCar` +0x4b0: the drive; its +4 the definition (`lane`), its +8 the state (`dri`).
pub const CAR_DRIVE: u32 = 0x4b0;
pub const CAR_SPEED: u32 = 0x15fc;
/// `CCar` +0x470: the four wheels; a wheel's +0xf00 is its tyre scratch, whose +0x420 is the tyre-noise block.
pub const CAR_WHEELS: u32 = 0x470;
pub const WHEEL_TYRE_SCRATCH: u32 = 0xf00;
pub const WHEEL_TYRE_NOISE: u32 = WHEEL_TYRE_SCRATCH + 0x420;
/// The damage handler's ([`crate::damage::CAR_DAMAGE_HANDLER`]) speech ring index; its `CSigCarDamage` payload is at
/// [`crate::damage::HANDLER_EVENT_CODE`].
pub const HANDLER_SPEECH_RING: u32 = 0x3e58;
/// A wheel's +0xec4: the strut's `CSpringDamper`, whose +0x1c is the damper signal; +0x5cc: the strut velocity.
pub const WHEEL_DAMPER_SIGNAL: u32 = 0xec4 + 0x1c;
pub const WHEEL_STRUT_VELOCITY: u32 = 0x5cc;
/// `CCar` +0x620: the E39 block `Sample9800` reads, its count (+0x80) and scale (+0x8c).
pub const CAR_SENSOR_BLOCK: u32 = 0x620;
/// The battery sensor is the scale times 12 (0x7316bc).
pub const BATTERY_SCALE: f32 = 12.0;

/// Tyre block word 1 and the contact flag (word 0x17) that `FillOutputs` sets and the no-contact path clears.
pub const TYRE_MAX: usize = 1;
pub const TYRE_CONTACT: usize = 0x17;

/// Each wheel's tyre block word 1 over a game frame: `CScratch::FillOutputs` (0x532f10) keeps the largest
/// `(tread depth + carcass radius - c - state +0x48) / dt` of the frame's tyre steps (`v > max`, so a NaN is not kept),
/// a step without contact clears the block (0x5360c0's no-contact path), and the wheel's frame finish (`CWheel::Vf28`
/// 0x53f230, `CScratch::EmitNoise`) sends it and sets it to 0 for the next frame. The terms come from the pure host
/// (`rbr_hybrid::pure::Pure::take_tyre_terms`).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct TyreMax(pub [f32; 4]);

impl TyreMax {
    /// One tyre step's fold: `term` is its `FillOutputs` term, `None` without contact.
    pub fn fold(max: f32, term: Option<f32>) -> f32 {
        match term {
            Some(v) if v > max => v,
            Some(_) => max,
            None => 0.0,
        }
    }

    /// Folds in a tyre step of wheel `k`.
    pub fn add(&mut self, k: usize, term: Option<f32>) {
        if let Some(max) = self.0.get_mut(k) {
            *max = Self::fold(*max, term);
        }
    }

    /// `sound` with the frame's word 1 in each tyre block.
    pub fn apply(&self, sound: &mut Sound) {
        for (block, max) in sound.tyres.iter_mut().zip(self.0) {
            block[TYRE_MAX] = max.to_bits();
        }
    }
}

/// One step's sound inputs.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Sound {
    pub engine: [u32; ENGINE_WORDS],
    pub drivetrain: [u32; DRIVETRAIN_WORDS],
    pub speed: f32,
    pub tyres: [[u32; TYRE_WORDS]; 4],
    pub hit: [u32; HIT_WORDS],
    pub speech_ring: u32,
    pub battery: f32,
    pub brakes: u32,
    pub water: f32,
    pub dampers: [[u32; DAMPER_WORDS]; 4],
    pub strut_velocity: [f32; 4],
    pub fires: u32,
}

impl Sound {
    /// Builds the record from a memory reader `u32(address)`, with `car` the CCar address.
    pub fn read_from(car: u32, u32_at: impl Fn(u32) -> u32) -> Sound {
        let state = u32_at(car + CAR_ENGINE + ENGINE_STATE);
        let engine = std::array::from_fn(|i| u32_at(state + STATE_NOISE + 4 * i as u32));
        let drive = car + CAR_DRIVE;
        let lane = u32_at(drive + 4);
        let dri = u32_at(drive + 8);
        let gear = u32_at(dri + 0x54c);
        let mut drivetrain = [0; DRIVETRAIN_WORDS];
        for (i, word) in drivetrain.iter_mut().enumerate() {
            *word = u32_at(drive + 0x1a4 + 4 * i as u32);
        }
        drivetrain[1] = u32_at(lane.wrapping_add(gear.wrapping_mul(0x50)).wrapping_add(0x6a4));
        drivetrain[2] = u32_at(dri + 0x384);
        drivetrain[3] = u32_at(lane + 0x9d4);
        drivetrain[7] = u32_at(dri + 0x37c) & 0x7fff_ffff;
        // M8480 copies a wheel's speed only while its driveshaft is broken: its word in the drive's link block (+0x170,
        // the damage handler's +0x3a18, linked by `E39_Link3AB0` 0x503ab0) is 0. Else the word in the drive stays as
        // it was: the constructor's 0, as the standalone never runs M8480 (the game's last speed after a repair).
        let link = u32_at(drive + 0x170);
        for (k, at) in [0x94, 0x15c, 0x224, 0x2ec].into_iter().enumerate() {
            if u32_at(link.wrapping_add(0x18 + 0xc * k as u32)) == 0 {
                drivetrain[10 + k] = u32_at(dri + at);
            }
        }
        let wheels: [u32; 4] = std::array::from_fn(|k| u32_at(car + CAR_WHEELS + 4 * k as u32));
        fn words<const N: usize>(u32_at: &impl Fn(u32) -> u32, wheel: u32, at: u32) -> [u32; N] {
            if wheel == 0 {
                return [0; N];
            }
            std::array::from_fn(|i| u32_at(wheel + at + 4 * i as u32))
        }
        let tyres = wheels.map(|wheel| words(&u32_at, wheel, WHEEL_TYRE_NOISE));
        let dampers = wheels.map(|wheel| words(&u32_at, wheel, WHEEL_DAMPER_SIGNAL));
        let strut_velocity = wheels.map(|wheel| f32::from_bits(words::<1>(&u32_at, wheel, WHEEL_STRUT_VELOCITY)[0]));
        let handler = u32_at(car + CAR_DAMAGE_HANDLER);
        let block = u32_at(car + CAR_SENSOR_BLOCK);
        Sound {
            engine,
            drivetrain,
            speed: f32::from_bits(u32_at(car + CAR_SPEED)),
            tyres,
            hit: std::array::from_fn(|i| u32_at(handler + HANDLER_EVENT_CODE + 4 * i as u32)),
            speech_ring: u32_at(handler + HANDLER_SPEECH_RING),
            battery: f32::from_bits(u32_at(block + 0x8c)) * BATTERY_SCALE,
            brakes: u32_at(block + 0x80),
            water: 0.0,
            dampers,
            strut_velocity,
            fires: 0,
        }
    }

    fn words(&self) -> impl Iterator<Item = u32> + '_ {
        self.engine
            .iter()
            .chain(&self.drivetrain)
            .copied()
            .chain([self.speed.to_bits()])
            .chain(self.tyres.iter().flatten().copied())
            .chain(self.hit.iter().copied())
            .chain([self.speech_ring, self.battery.to_bits(), self.brakes, self.water.to_bits()])
            .chain(self.dampers.iter().flatten().copied())
            .chain(self.strut_velocity.map(f32::to_bits))
            .chain([self.fires])
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        self.words().flat_map(u32::to_le_bytes).collect()
    }

    /// The record from its payload, or `None` when the payload has another size.
    pub fn from_bytes(bytes: &[u8]) -> Option<Sound> {
        if bytes.len() != SND1_BYTES {
            return None;
        }
        let word = |i: usize| u32::from_le_bytes(bytes[4 * i..4 * i + 4].try_into().unwrap());
        let tyres = ENGINE_WORDS + DRIVETRAIN_WORDS + 1;
        let hit = tyres + 4 * TYRE_WORDS;
        let tail = hit + HIT_WORDS;
        let dampers = tail + 4;
        let end = dampers + 4 * DAMPER_WORDS;
        Some(Sound {
            engine: std::array::from_fn(word),
            drivetrain: std::array::from_fn(|i| word(ENGINE_WORDS + i)),
            speed: f32::from_bits(word(ENGINE_WORDS + DRIVETRAIN_WORDS)),
            tyres: std::array::from_fn(|k| std::array::from_fn(|i| word(tyres + TYRE_WORDS * k + i))),
            hit: std::array::from_fn(|i| word(hit + i)),
            speech_ring: word(tail),
            battery: f32::from_bits(word(tail + 1)),
            brakes: word(tail + 2),
            water: f32::from_bits(word(tail + 3)),
            dampers: std::array::from_fn(|k| std::array::from_fn(|i| word(dampers + DAMPER_WORDS * k + i))),
            strut_velocity: std::array::from_fn(|k| f32::from_bits(word(end + k))),
            fires: word(end + 4),
        })
    }

    pub fn engine_f32(&self, i: usize) -> f32 {
        f32::from_bits(self.engine[i])
    }

    pub fn drivetrain_f32(&self, i: usize) -> f32 {
        f32::from_bits(self.drivetrain[i])
    }
}

/// The NGP sound record's tag (`NSD1`): sent under NGP 7.4.778 only, after FFB1.
pub const NSD1: [u8; 4] = *b"NSD1";
/// NGP.dll's words (file addresses) that NGP's patches of the 1.02 sound read instead of the exe's constants, which its
/// engine init (`0x1004ab40`, `rbr_physics::ngp::layout::car_init`) sets per car: the gear and diff whine caps of
/// `CCarSound::Me510` (0x4ae510; 0x72f00c, 0x731028) and the engine voice's low rpm of `CCarSound::M6a0` (0x4b06a0;
/// 0x731cd4).
pub const NGP_GEAR_LOAD_CAP: u32 = 0x1017_1acc;
pub const NGP_DIFF_LOAD_CAP: u32 = 0x1017_1ad0;
pub const NGP_MIN_RPM: u32 = 0x1017_1ad4;

/// The game's `CCompleteCarSetupOptions` pointer (`0x7ecd04`, NGP's getter `0x10013cd0`) and NGP's `IdleRPM_NGP` in
/// it (an integer, rpm), which `NGP.dll` sends its FMOD daemon as the start orbit's `rpms` (0x100b3310-0x100b3353).
pub const SETUP_OPTIONS: u32 = 0x7e_cd04;
pub const OPTIONS_IDLE_RPM: u32 = 0x13c0;

/// The NGP sound record (NSD1): `f32 gear_load_cap, diff_load_cap, min_rpm, idle_rpm`, NGP.dll's words
/// [`NGP_GEAR_LOAD_CAP`], [`NGP_DIFF_LOAD_CAP`] and [`NGP_MIN_RPM`] after the step and the car's `IdleRPM_NGP`
/// ([`OPTIONS_IDLE_RPM`]). `Default` is the exe's caps and low rpm (tests, examples).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NgpSound {
    pub gear_load_cap: f32,
    pub diff_load_cap: f32,
    pub min_rpm: f32,
    pub idle_rpm: f32,
}

impl Default for NgpSound {
    fn default() -> NgpSound {
        NgpSound { gear_load_cap: 600.0, diff_load_cap: 6000.0, min_rpm: 500.0, idle_rpm: 0.0 }
    }
}

impl NgpSound {
    pub fn to_bytes(&self) -> Vec<u8> {
        [self.gear_load_cap, self.diff_load_cap, self.min_rpm, self.idle_rpm]
            .into_iter()
            .flat_map(f32::to_le_bytes)
            .collect()
    }

    /// The record from its payload, or `None` when the payload has another size.
    pub fn from_bytes(bytes: &[u8]) -> Option<NgpSound> {
        if bytes.len() != 16 {
            return None;
        }
        let word = |i: usize| f32::from_le_bytes(bytes[4 * i..4 * i + 4].try_into().unwrap());
        Some(NgpSound { gear_load_cap: word(0), diff_load_cap: word(1), min_rpm: word(2), idle_rpm: word(3) })
    }
}

/// The FMOD block record's tag (`FMB1`).
pub const FMB1: [u8; 4] = *b"FMB1";
/// `CCar` +0x860: what `NGP.dll` sends its FMOD daemon as `brake` (block +0x10).
pub const CAR_BRAKE: u32 = 0x860;
/// `CCar` +0x34: the car definition, whose +0x24c scales the drive state's +0x384 into the transmission's speed.
pub const CAR_DEF: u32 = 0x34;
/// A wheel's (`CCar +0x470`) +0xaa0 and spin rate +0xa50, whose product is its speed.
pub const WHEEL_SPEED_SCALE: u32 = 0xaa0;
pub const WHEEL_SPIN: u32 = 0xa50;

/// What `NGP.dll`'s `FUN_100b3470` copies from the car into its FMOD daemon's block (rbr-recomp `notes/sound.md` 14) that SND1
/// does not have, read after the step (FMB1, sent under 1.02 and NGP alike; payload `i32 gear`, `f32 brake,
/// transmission_speed, wheel_speeds[4]`):
/// - `gear` the drive state's gear (`+0x54c`, block +0x14);
/// - `brake` the car's [`CAR_BRAKE`] (block +0x10);
/// - `transmission_speed` the drive state's +0x384 times the definition's +0x24c (m/s; block +0x30 takes its
///   magnitude in km/h);
/// - `wheel_speeds` each wheel's [`WHEEL_SPEED_SCALE`] times its [`WHEEL_SPIN`] (m/s; block +0x38..+0x44 the same).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct FmodBlock {
    pub gear: i32,
    pub brake: f32,
    pub transmission_speed: f32,
    pub wheel_speeds: [f32; 4],
}

impl FmodBlock {
    /// The record from a memory reader `u32(address)`, with `car` the CCar address.
    pub fn read_from(car: u32, u32_at: impl Fn(u32) -> u32) -> FmodBlock {
        let f = |at: u32| f32::from_bits(u32_at(at));
        let dri = u32_at(car + CAR_DRIVE + 8);
        let def = u32_at(car + CAR_DEF);
        let wheel_speeds = std::array::from_fn(|k| {
            let wheel = u32_at(car + CAR_WHEELS + 4 * k as u32);
            if wheel == 0 {
                return 0.0;
            }
            f(wheel + WHEEL_SPEED_SCALE) * f(wheel + WHEEL_SPIN)
        });
        FmodBlock {
            gear: u32_at(dri + 0x54c) as i32,
            brake: f(car + CAR_BRAKE),
            transmission_speed: f(dri + 0x384) * f(def + 0x24c),
            wheel_speeds,
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let words = [self.gear as u32, self.brake.to_bits(), self.transmission_speed.to_bits()];
        words.into_iter().chain(self.wheel_speeds.map(f32::to_bits)).flat_map(u32::to_le_bytes).collect()
    }

    /// The record from its payload, or `None` when the payload has another size.
    pub fn from_bytes(bytes: &[u8]) -> Option<FmodBlock> {
        if bytes.len() != 28 {
            return None;
        }
        let word = |i: usize| u32::from_le_bytes(bytes[4 * i..4 * i + 4].try_into().unwrap());
        Some(FmodBlock {
            gear: word(0) as i32,
            brake: f32::from_bits(word(1)),
            transmission_speed: f32::from_bits(word(2)),
            wheel_speeds: std::array::from_fn(|k| f32::from_bits(word(3 + k))),
        })
    }
}

/// What the sound takes from one live answer: the SND1 record, the PCN1 callouts, HUD2's countdown, the PTC1
/// splash, the DET1 doors, the FMB1 record and, under NGP, the NSD1 record.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Inputs {
    pub sound: Sound,
    pub callouts: Vec<crate::pacenote::Callout>,
    /// The HUD's start countdown (HUD +0x244, `RaceHud::countdown`), when the record has the race clock.
    pub countdown: Option<f32>,
    /// The race clock (HUD +0x140, `RaceHud::time`), when the record has it.
    pub race_time: Option<f32>,
    /// Each wheel's last `CSigTireParticles` splash level of the step (+0x98, what `CTireSnd::M6560` 0x4d6560
    /// keeps), when the step emitted one.
    pub splash: [Option<f32>; 4],
    /// The left and right door are in the DET1 record: detached and awake. `CHingedRigidBody::SimFB40` (0x51fb40)
    /// emits each step whether its body is still on the car (`+0x650`, cleared on the detach), which the sound
    /// keeps (`CCarSound::SetPair` 0x4ac220); DET1 leaves out a part once it sleeps, so the sound latches it.
    pub doors_off: [bool; 2],
    /// NGP's sound words (NSD1).
    pub ngp: NgpSound,
    /// What NGP's FMOD block takes from the car beyond SND1 (FMB1), when the answer has it.
    pub block: Option<FmodBlock>,
}

impl Inputs {
    /// The inputs from an answer's tagged records (`crate::live::read_records`) and HUD record; `None` without SND1 or NSD1.
    pub fn from_records(records: &[([u8; 4], Vec<u8>)], hud: &crate::hud::Hud) -> Option<Inputs> {
        let payload = |tag: [u8; 4]| records.iter().find(|(t, _)| *t == tag).map(|(_, p)| p.as_slice());
        let mut splash = [None; 4];
        for tyre in
            payload(crate::particles::PTC1).and_then(|p| crate::particles::read_body(p).ok()).unwrap_or_default()
        {
            if let Some(s) = splash.get_mut(tyre.wheel as usize) {
                *s = Some(tyre.splash);
            }
        }
        let mut doors_off = [false; 2];
        for part in payload(crate::detach::DET1).and_then(|p| crate::detach::parse(p).ok()).unwrap_or_default() {
            if let Some(door) = doors_off.get_mut(part.part as usize) {
                *door = true;
            }
        }
        Some(Inputs {
            sound: Sound::from_bytes(payload(SND1)?)?,
            callouts: payload(crate::pacenote::PCN1).and_then(crate::pacenote::from_bytes).unwrap_or_default(),
            countdown: hud.race.map(|race| race.countdown),
            race_time: hud.race.map(|race| race.time),
            splash,
            doors_off,
            ngp: payload(NSD1).and_then(NgpSound::from_bytes)?,
            block: payload(FMB1).and_then(FmodBlock::from_bytes),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tyre_max_keeps_the_frame_s_largest_term_and_a_lift_clears_it() {
        let fold = |terms: &[Option<f32>]| terms.iter().fold(0.0, |m, &t| TyreMax::fold(m, t));
        assert_eq!(fold(&[Some(0.5), Some(2.0), Some(1.0)]), 2.0);
        assert_eq!(fold(&[Some(-1.0)]), 0.0, "the finish's 0 stays above a negative term");
        assert_eq!(fold(&[Some(3.0), None, Some(1.0)]), 1.0, "a step off the ground clears the block");
        assert_eq!(fold(&[Some(1.0), Some(f32::NAN)]), 1.0);
        let mut sound = Sound::default();
        TyreMax([1.0, 0.0, 2.0, 0.0]).apply(&mut sound);
        assert_eq!((sound.tyres[0][1], sound.tyres[2][1]), (1f32.to_bits(), 2f32.to_bits()));
    }

    #[test]
    fn records_round_trip() {
        let mut sound = Sound { speed: 12.5, water: 90.0, ..Sound::default() };
        sound.engine[0] = 300f32.to_bits();
        sound.drivetrain[13] = 7;
        sound.tyres[3][TYRE_CONTACT] = 1;
        sound.hit[4] = 900f32.to_bits();
        sound.dampers[2][2] = 0.5f32.to_bits();
        sound.strut_velocity[1] = -0.25;
        sound.fires = 1;
        let bytes = sound.to_bytes();
        assert_eq!(bytes.len(), SND1_BYTES);
        assert_eq!(Sound::from_bytes(&bytes), Some(sound));
        assert_eq!(Sound::from_bytes(&bytes[4..]), None);
        let ngp = NgpSound { gear_load_cap: 520.0, diff_load_cap: 5200.0, min_rpm: 750.0, idle_rpm: 1000.0 };
        assert_eq!(NgpSound::from_bytes(&ngp.to_bytes()), Some(ngp));
        assert_eq!(NgpSound::from_bytes(&ngp.to_bytes()[4..]), None);
        let block = FmodBlock { gear: -1, brake: 0.5, transmission_speed: -3.0, wheel_speeds: [1.0, 2.0, 3.0, 4.0] };
        assert_eq!(FmodBlock::from_bytes(&block.to_bytes()), Some(block));
        assert_eq!(FmodBlock::from_bytes(&block.to_bytes()[4..]), None);
    }

    #[test]
    fn reads_the_payloads_from_memory() {
        // A car at 0x1000: the engine state at 0x8000, the drive's definition at 0x9000 and state at 0xa000.
        let car = 0x1000;
        let mem = |at: u32| match at {
            0x1698 => 0x8000,
            0x8030 => 1.0f32.to_bits(),
            0x14b4 => 0x9000,
            0x14b8 => 0xa000,
            0xa54c => 3,
            0x9794 => 47,
            0xa37c => (-2.0f32).to_bits(),
            0x25fc => 20.0f32.to_bits(),
            0x1478 => 0xb000,
            0xc37c => 1,
            0x19f4 => 0xc000,
            0xfe70 => 900f32.to_bits(),
            0x1620 => 0xd000,
            0xd08c => 0.5f32.to_bits(),
            // The drive's link block is the same 0xd000: wheel 1's shaft is whole (its word 1), so its speed stays
            // the drive's.
            0xa094 => 30.0f32.to_bits(),
            0xa15c => 40.0f32.to_bits(),
            0xd024 => 1,
            0x1680 => 5.0f32.to_bits(),
            // FMB1: the brake, the definition at 0xe000 and its scale, the drive's speed, wheel 2's speed.
            0x1860 => 0.75f32.to_bits(),
            0x1034 => 0xe000,
            0xe24c => 0.25f32.to_bits(),
            0xa384 => 10.0f32.to_bits(),
            0xbaa0 => 0.5f32.to_bits(),
            0xba50 => (-20.0f32).to_bits(),
            _ => 0,
        };
        let block = FmodBlock::read_from(car, mem);
        assert_eq!(
            block,
            FmodBlock { gear: 3, brake: 0.75, transmission_speed: 2.5, wheel_speeds: [0.0, 0.0, -10.0, 0.0] }
        );
        let sound = Sound::read_from(car, mem);
        assert_eq!(sound.engine_f32(1), 1.0);
        assert_eq!(sound.drivetrain[1], 47);
        assert_eq!(sound.drivetrain_f32(7), 2.0);
        assert_eq!((sound.drivetrain_f32(10), sound.drivetrain_f32(11)), (30.0, 5.0));
        assert_eq!(sound.speed, 20.0);
        assert_eq!((sound.tyres[2][TYRE_CONTACT], sound.tyres[0][TYRE_CONTACT]), (1, 0));
        assert_eq!(f32::from_bits(sound.hit[4]), 900.0);
        assert_eq!(sound.battery, 6.0);
    }
}
