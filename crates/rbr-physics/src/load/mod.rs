//! The car's definitions and setups from its `.lsp` layers, as the load builds them (step 2 of the load path:
//! `notes/rust-plan.md` milestone L). Each file's root group is walked (`ntweak`) on its owner, as the game hands it to
//! the owner's `Parse`:
//! - the physics definition (`CPhysics::CDef`, [`new_physics_def`], made by `E46_CreateDef` with the first file) takes
//!   `misc/prime.lsp`, `physics.lsp`, the car's `common.lsp` and its surface file (`gravel.lsp`, ...)
//!   ([`physics_file`]): its own tweaks and bit-fields (`Flags`, `DbgFlags`, `LogFlags`), the collision solver's
//!   definition, the material manager's materials, and the definition manager's groups, which name definitions by
//!   factory and name in the [`Registry`]. `prime.lsp` first lists every name (as `Solid`s) and makes the car's
//!   definition (`Car` `CAR_ROOT`, whose group links the car's parts by registry index); the later files walk their
//!   groups on the definitions, and the surface file makes the four `Wheel` definitions, each with its spring damper and
//!   tyre definition inside;
//! - `CCompleteCarSetup` ([`construct_complete_setup`]) takes the setup file (`setups/d_gravel.lsp`, ...,
//!   [`setup_file`]): the car, drive, engine and control unit setups and each wheel's three setup objects;
//! - `CCompleteCarSetupOptions` ([`construct_setup_options`]) takes the options file (`r_gravel.lsp`, ...) and
//!   `tyres.lsp` ([`options_file`]): the car, drive, control unit, engine, spring damper, tyre and wheel options and the
//!   seven tyre types;
//! - the settings code stores the player's driver aids in the control unit's setup ([`set_driver_aids`]) and copies the
//!   gear rows the drive setup names from the drive options into it
//!   ([`select_drive_options`]: `CCarSettingsManager::NormalizeSetup`, 0x547300; NGP makes the brakes page's `vf48`,
//!   0x5487d0, return at once. The gears page's `vf48`, 0x5486c0, makes the same gear copies. Which path the reference run took is not known: no
//!   run has logged these calls), and each wheel's top mount its setup's slot names from the wheel options into it
//!   ([`select_top_mounts`]: the geometry pages' `vf48`, 0x548a50 and 0x548c60);
//! - the setup's emit (0x48e620) points each tyre setup at the race's tyre type, and the car copies the setups into
//!   the definitions (`CCar::UpdateA720`, 0x4ea720: the drive setup (`CDrive::M8770`), the control unit's maps and the
//!   car setup's words, [`copy_car_setups`], and `E51Wheel::CopyCF30` per wheel, [`copy_setups`]).
//!
//! The factories other than the ones these files use are not ported: their names are registered without a definition.

use rbr_formats::lsp::Node;

pub mod car_grids;
pub mod layout;

pub use layout::{
    construct_complete_setup, construct_setup_options, copy_car_setups, copy_setups, drive_options, emit_car_meshes,
    enter_tic_sample, fire_collision_mesh, new_body, new_car, new_physics_def, new_replay_stand_in, new_wheel,
    options_file, physics_file, select_drive_options, select_top_mounts, select_tyre_type, set_driver_aids, setup_file,
    setups, store_statics, tyre_type, BodyKind, Setups, WheelSetup,
};

/// The player's driver aids, from the game's options (`AutoGears` per player at 0x1660814, `GearGuard` 0x1660ea8,
/// `NeutralLock` 0x1660ea0, `ClutchHelp` 0x1660ea4; 0 or 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DriverAids {
    pub auto_gears: u32,
    pub gear_guard: u32,
    pub neutral_lock: u32,
    pub clutch_help: u32,
}

/// The wheels, in the order of the car's wheel slots (`CCar+0x470`) and of the setup's keys.
pub const POSITIONS: [&str; 4] = ["LF", "RF", "LB", "RB"];

/// The items of a retail file's one top-level group, `(("Name" ...))`: what the game's walk gets first.
pub fn root(file: &[Node]) -> Option<&[Node]> {
    match file {
        [Node::List(outer)] => match outer.as_slice() {
            [Node::List(items)] => Some(items),
            _ => None,
        },
        _ => None,
    }
}

/// One name of the definition registry: the factory that made its definition, and the definition (0 when the factory
/// is not ported).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub factory: String,
    pub def: u32,
}

/// The definition registry (`CDefFactory` at 0x7c8040) as the definition manager fills it (`CDefManager::Vf14`,
/// 0x501ef0): one entry per name, in the order the names first come (`prime.lsp` lists them all first). A factory
/// other than the entry's makes a new definition, which takes the entry's place and index: `prime.lsp`'s `Solid`
/// entries hold the names until the car's files make the wheels, bodies and car.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Registry {
    entries: Vec<Entry>,
}

impl Registry {
    /// The entries, in index order.
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// `CDictBase::Find` on the registry: the index of `name`.
    pub fn index(&self, name: &str) -> Option<u32> {
        self.entries.iter().position(|e| e.name == name).map(|i| i as u32)
    }

    /// The definition of `name` when `factory` made it.
    pub fn def(&self, factory: &str, name: &str) -> Option<u32> {
        self.entries.iter().find(|e| e.name == name && e.factory == factory).map(|e| e.def)
    }

    /// `factory`'s definition `def` for `name`, in the name's entry or a new one.
    pub fn set(&mut self, factory: &str, name: &str, def: u32) {
        let entry = Entry { name: name.to_string(), factory: factory.to_string(), def };
        match self.entries.iter_mut().find(|e| e.name == name) {
            Some(e) => *e = entry,
            None => self.entries.push(entry),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_factory_takes_the_names_place() {
        let mut registry = Registry::default();
        registry.set("Solid", "CAR_WHEEL_LF", 0);
        registry.set("Solid", "CAR_ROOT", 0);
        registry.set("Car", "CAR_ROOT", 0x1000);
        assert_eq!(registry.index("CAR_ROOT"), Some(1));
        assert_eq!(registry.def("Car", "CAR_ROOT"), Some(0x1000));
        assert_eq!(registry.def("Solid", "CAR_ROOT"), None);
        assert_eq!(registry.index("CAR_DOOR_L"), None);
    }
}
