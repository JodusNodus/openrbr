//! The race options a quick race starts with: the damage level, the four driver aids and the weather, as the game's
//! menu settings hold them (`notes/gameplay.md` "Race options"). Pure: the boot writes [`RaceOptions::globals`] and the words the
//! loads derive from them; the race flow takes [`RaceOptions::assist_settings`].
//!
//! The game keeps them in `E66MenuSettings` (0x16607f8): its quick-race block (+0x0) holds the auto gears per player
//! (+0x1c, `0x1660814`), the weather (+0x50, `0x1660848`), the damage selection (+0x54, `0x166084c`, always 1: only the
//! block's constructor and reset write it) and the damage level (+0x58, `0x1660850`, the `CARDAM` slider); its gears
//! block (+0x698) the neutral lock (`0x1660ea0`), the clutch help (`0x1660ea4`) and the gear guard (`0x1660ea8`). The
//! defaults are the blocks' constructors' (`InitQR` 0x586f30, `InitGEAR` 0x588f00), which are the reference drives'.
//! There is no assistance-penalty option outside a rally: the penalty follows the damage level (`M576710`).
//!
//! A championship has its own two options ([`ChampionshipOptions`], `SSeasonSettings` +0x448 of the menu settings):
//! the skill level (`SSKILL` page 0x24, `0x1660c5c`) and the damage level (the season's `CARDAM` page 0x25,
//! `0x1660c70`). New season stores the skill in the season daemon's +0x8e84 (`E13MenuInitA` 0x47c7f0 →
//! `E60Flow::M566990` → `DaemonInit` 0x5d34b0), which sets the penalty and the assistance gate in mode 3.

use crate::flow::Mode;
use crate::race_clock::AssistSettings;
use crate::weather::Preset;

/// The `CARDAM` slider's four values (labels `OFF`, `DIFF_SAFE`, `DIFF_REDUCE`, `DIFF_REAL`, the list at 0x165fa5c).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DamageLevel {
    Off,
    #[default]
    Safe,
    Reduced,
    Realistic,
}

impl DamageLevel {
    pub const ALL: [DamageLevel; 4] =
        [DamageLevel::Off, DamageLevel::Safe, DamageLevel::Reduced, DamageLevel::Realistic];

    /// The word in `0x1660850`, which `InitQuickRaceDamageHandler` (0x567640) gives `CCarDamageHandler::Set3FB0`.
    pub fn word(self) -> u32 {
        self as u32
    }

    /// The profile's word (`SQuickRaceSettings::Read` clamps it to 3).
    pub fn from_word(word: u32) -> DamageLevel {
        DamageLevel::ALL[word.min(3) as usize]
    }

    pub fn name(self) -> &'static str {
        ["off", "safe", "reduced", "realistic"][self as usize]
    }

    pub fn from_name(name: &str) -> Option<DamageLevel> {
        DamageLevel::ALL.into_iter().find(|level| level.name() == name)
    }
}

/// The player's driver aids, each on or off (the menus' `OFF`/`ON` toggles).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DriverAids {
    /// `0x1660814` (per player; the quick race's is player 0).
    pub auto_gears: bool,
    /// `GEAR_PROT`, `0x1660ea8`.
    pub gear_guard: bool,
    /// `CS_NEUTRALLOCK`, `0x1660ea0`.
    pub neutral_lock: bool,
    /// `CLUTCH_HLP`, `0x1660ea4`.
    pub clutch_help: bool,
}

impl Default for DriverAids {
    /// `InitQR`'s and `InitGEAR`'s: 0, 1, 0, 1.
    fn default() -> Self {
        DriverAids { auto_gears: false, gear_guard: true, neutral_lock: false, clutch_help: true }
    }
}

impl DriverAids {
    /// The words `CCarSettingsManager::LoadGlobal` (0x547080) and `InitializeCarSetting` store in the control unit's
    /// setup: `AutoGears`, `GearGuard`, `NeutralLock`, `ClutchHelp`.
    pub fn words(self) -> [u32; 4] {
        [self.auto_gears, self.gear_guard, self.neutral_lock, self.clutch_help].map(u32::from)
    }
}

/// The game's menu globals the options live in.
pub const AUTO_GEARS: u32 = 0x0166_0814;
pub const WEATHER: u32 = 0x0166_0848;
pub const DAMAGE_SELECT: u32 = 0x0166_084C;
pub const DAMAGE_LEVEL: u32 = 0x0166_0850;
pub const NEUTRAL_LOCK: u32 = 0x0166_0EA0;
pub const CLUTCH_HELP: u32 = 0x0166_0EA4;
pub const GEAR_GUARD: u32 = 0x0166_0EA8;

/// The quick-race block's damage selection: the constructor's 1, which no menu changes.
pub const DAMAGE_SELECT_WORD: u32 = 1;

/// A quick race's options.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RaceOptions {
    pub damage: DamageLevel,
    pub aids: DriverAids,
    pub weather: Preset,
}

impl Default for RaceOptions {
    fn default() -> Self {
        RaceOptions { damage: DamageLevel::default(), aids: DriverAids::default(), weather: Preset::Good }
    }
}

impl RaceOptions {
    /// The menu globals and their words, as the game holds them when the race loads.
    pub fn globals(&self) -> [(u32, u32); 7] {
        let [auto_gears, gear_guard, neutral_lock, clutch_help] = self.aids.words();
        [
            (AUTO_GEARS, auto_gears),
            (WEATHER, self.weather.id()),
            (DAMAGE_SELECT, DAMAGE_SELECT_WORD),
            (DAMAGE_LEVEL, self.damage.word()),
            (NEUTRAL_LOCK, neutral_lock),
            (CLUTCH_HELP, clutch_help),
            (GEAR_GUARD, gear_guard),
        ]
    }

    /// The quick race's penalty settings: flow mode 1 and the damage level as the global `M576710` switches on.
    pub fn assist_settings(&self) -> AssistSettings {
        AssistSettings { level: self.damage.word(), ..AssistSettings::QUICK_RACE }
    }
}

/// The `SSKILL` page's three values (`SROOKIE`, `SPROFFS`, `SCHAMPION`; page 0x24, builder 0x5ab840): `0x1660c5c`,
/// then the season's +0x8e84. Rookie and Professional give 10 and 20 s per outside assistance; Champion gives 35 s and
/// shuts the assistance gate (`E60Flow::M566640`), so the car is not put back on the road.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Skill {
    #[default]
    Rookie,
    Professional,
    Champion,
}

impl Skill {
    pub const ALL: [Skill; 3] = [Skill::Rookie, Skill::Professional, Skill::Champion];

    /// The word in `0x1660c5c` and the season's +0x8e84.
    pub fn word(self) -> u32 {
        self as u32
    }

    /// A season's word. The menu only writes 0-2; a larger word (no penalty in `M576710`, the gate shut) reads as
    /// Champion (ours).
    pub fn from_word(word: u32) -> Skill {
        Skill::ALL[word.min(2) as usize]
    }

    pub fn name(self) -> &'static str {
        ["rookie", "professional", "champion"][self as usize]
    }

    pub fn from_name(name: &str) -> Option<Skill> {
        Skill::ALL.into_iter().find(|skill| skill.name() == name)
    }

    /// The English label (`bstrings.eng`).
    pub fn label(self) -> &'static str {
        ["Rookie", "Professional", "Champion"][self as usize]
    }
}

/// The championship's menu globals.
pub const SKILL: u32 = 0x0166_0C5C;
pub const CHAMPIONSHIP_DAMAGE: u32 = 0x0166_0C70;

/// A championship's options, chosen before New season (`SSKILL`, then `CARDAM`) and kept in the season save
/// (`rbr_formats::saves::season::Season::set_options`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChampionshipOptions {
    pub skill: Skill,
    pub damage: DamageLevel,
}

impl Default for ChampionshipOptions {
    /// `InitSEA` (0x588990): +0x1c 0 (Rookie), +0x30 3 (Realistic).
    fn default() -> Self {
        ChampionshipOptions { skill: Skill::Rookie, damage: DamageLevel::Realistic }
    }
}

impl ChampionshipOptions {
    /// The menu globals the season's stage loads with (`E80Snap14` 0x5d0cc0 restores them before each stage).
    pub fn globals(&self) -> [(u32, u32); 2] {
        [(SKILL, self.skill.word()), (CHAMPIONSHIP_DAMAGE, self.damage.word())]
    }

    /// A championship stage's penalty settings: flow mode 3 and the season's skill (+0x8e84). `quick` is the quick
    /// race's menu, whose `0x1660850` keeps its value but is not read in mode 3.
    pub fn assist_settings(&self, quick: &RaceOptions) -> AssistSettings {
        AssistSettings {
            mode: Mode::CHAMPIONSHIP.value(),
            rally_level: self.skill.word(),
            level: quick.damage.word(),
            false_starts: 0,
        }
    }
}

/// `InitQuickRaceDamageHandler` (0x567640): the damage level the car gets in flow `mode`. Modes 1, 2, 4 and 5 take the
/// quick race's `0x1660850`, the championship (3) its `0x1660c70`, the rally school (7) 2, and any other mode 1.
pub fn damage_level(mode: Mode, quick: DamageLevel, championship: DamageLevel) -> DamageLevel {
    match mode.value() {
        1 | 2 | 4 | 5 => quick,
        3 => championship,
        7 => DamageLevel::Reduced,
        _ => DamageLevel::Safe,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_the_menu_constructors_and_the_reference_drives() {
        let options = RaceOptions::default();
        assert_eq!(options.aids.words(), [0, 1, 0, 1]);
        assert_eq!(options.damage.word(), 1);
        assert_eq!(options.assist_settings(), AssistSettings::QUICK_RACE);
        assert_eq!(
            options.globals(),
            [
                (0x1660814, 0),
                (0x1660848, 0),
                (0x166084c, 1),
                (0x1660850, 1),
                (0x1660ea0, 0),
                (0x1660ea4, 1),
                (0x1660ea8, 1)
            ]
        );
    }

    #[test]
    fn each_option_moves_only_its_word() {
        let base = RaceOptions::default().globals();
        let changed = |options: RaceOptions| -> Vec<(u32, u32)> {
            options.globals().into_iter().zip(base).filter(|(a, b)| a != b).map(|(a, _)| a).collect()
        };
        let aids = DriverAids::default();
        let with = |aids| RaceOptions { aids, ..RaceOptions::default() };
        assert_eq!(changed(with(DriverAids { auto_gears: true, ..aids })), [(AUTO_GEARS, 1)]);
        assert_eq!(changed(with(DriverAids { gear_guard: false, ..aids })), [(GEAR_GUARD, 0)]);
        assert_eq!(changed(with(DriverAids { neutral_lock: true, ..aids })), [(NEUTRAL_LOCK, 1)]);
        assert_eq!(changed(with(DriverAids { clutch_help: false, ..aids })), [(CLUTCH_HELP, 0)]);
        let damage = |damage| RaceOptions { damage, ..RaceOptions::default() };
        assert_eq!(changed(damage(DamageLevel::Realistic)), [(DAMAGE_LEVEL, 3)]);
        assert_eq!(changed(RaceOptions { weather: Preset::Random, ..RaceOptions::default() }), [(WEATHER, 1)]);
    }

    #[test]
    fn the_damage_level_sets_the_assistance_penalty() {
        // M576710 outside a rally: 0 and 1 give 10 s, 2 gives 20 s, 3 gives 35 s.
        let penalty = |damage| RaceOptions { damage, ..RaceOptions::default() }.assist_settings().penalty();
        let penalties: Vec<f32> = DamageLevel::ALL.into_iter().map(penalty).collect();
        assert_eq!(penalties, [10.0, 10.0, 20.0, 35.0]);
    }

    #[test]
    fn the_championship_defaults_are_init_sea() {
        let options = ChampionshipOptions::default();
        assert_eq!((options.skill, options.damage), (Skill::Rookie, DamageLevel::Realistic));
        assert_eq!(options.globals(), [(0x1660c5c, 0), (0x1660c70, 3)]);
    }

    #[test]
    fn the_skill_sets_the_championship_penalty_and_gate() {
        // M576710 in mode 3: +0x8e84 0, 1, 2 give 10, 20, 35 s; M566640 shuts the gate from 2.
        let quick = RaceOptions { damage: DamageLevel::Realistic, ..RaceOptions::default() };
        let settings = |skill| ChampionshipOptions { skill, ..ChampionshipOptions::default() }.assist_settings(&quick);
        let penalties: Vec<f32> = Skill::ALL.into_iter().map(|s| settings(s).penalty()).collect();
        assert_eq!(penalties, [10.0, 20.0, 35.0]);
        let gates: Vec<bool> =
            Skill::ALL.into_iter().map(|s| crate::flow::assistance_gate(settings(s))).collect();
        assert_eq!(gates, [true, true, false]);
    }

    #[test]
    fn the_damage_level_follows_the_mode() {
        use DamageLevel::*;
        let level = |mode| damage_level(Mode(mode), Off, Realistic);
        assert_eq!([0, 1, 2, 3, 4, 5, 7, 8].map(level), [Safe, Off, Off, Realistic, Off, Off, Reduced, Safe]);
    }

    #[test]
    fn skills_round_trip_and_clamp() {
        for skill in Skill::ALL {
            assert_eq!(Skill::from_word(skill.word()), skill);
            assert_eq!(Skill::from_name(skill.name()), Some(skill));
        }
        assert_eq!(Skill::from_word(5), Skill::Champion);
        assert_eq!(Skill::from_name("legend"), None);
    }

    #[test]
    fn damage_levels_round_trip_and_clamp_as_the_profile_read() {
        for level in DamageLevel::ALL {
            assert_eq!(DamageLevel::from_word(level.word()), level);
            assert_eq!(DamageLevel::from_name(level.name()), Some(level));
        }
        assert_eq!(DamageLevel::from_word(7), DamageLevel::Realistic);
        assert_eq!(DamageLevel::from_name("hard"), None);
    }
}
