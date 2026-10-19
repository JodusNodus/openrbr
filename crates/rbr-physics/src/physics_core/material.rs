//! `NPhysics::CMaterial` (`src/rbr/RBR_E45_Material.cpp`, 0x160 bytes): one per material, 256 in the material manager.
//! The constructor stores its `CBase` header (no name), 19 `CReal` tweaks from +0x0c (16 bytes each, value zero), the
//! `CUnsigned` at +0x13c and the flags bitfield at +0x14c. Every word of the object is one of these.

use super::construct::{
    Storage, TweakHeader, TweakReal, TweakUnsigned, FLAGS_NAME, REAL_NAME, REAL_VTABLE, UNSIGNED_NAME, UNSIGNED_VTABLE,
};
use super::layout;
use crate::mem::Mem;

/// `CMaterial`'s size (0x160).
pub const SIZE: u32 = 0x160;
/// The `CMaterial` vtable (its `CBase` header, 0x73d81c) and the flags bitfield's (`CMaterial::CFlags`, 0x73d7bc).
pub const VTABLE: u32 = 0x0073_d81c;
pub const FLAGS_VTABLE: u32 = 0x0073_d7bc;
/// The `CReal` tweaks of a material.
const REALS: usize = 19;

/// The words `CMaterial::CMaterial` writes.
pub struct MaterialImage {
    pub base: TweakHeader,
    pub reals: [TweakReal; REALS],
    pub unsigned: TweakUnsigned,
    pub flags: Storage,
}

/// The image a material's constructor makes: every value zero, the names the constructor stores.
pub fn image() -> MaterialImage {
    let real = TweakReal { header: TweakHeader { vtable: REAL_VTABLE, name: REAL_NAME, dirty: 0 }, value: 0.0 };
    MaterialImage {
        base: TweakHeader { vtable: VTABLE, name: 0, dirty: 0 },
        reals: [real; REALS],
        unsigned: TweakUnsigned {
            header: TweakHeader { vtable: UNSIGNED_VTABLE, name: UNSIGNED_NAME, dirty: 0 },
            value: 0,
        },
        flags: Storage { vtable: FLAGS_VTABLE, name: FLAGS_NAME },
    }
}

/// The materials a `CMaterialManager` holds (`TAGS`), the first at +0xc of the manager.
pub const TAGS: u32 = 0x100;
const FIRST_AT: u32 = 0x0c;
/// The flag byte of a material, at +0x164 of the manager's entry for it (the material's own +0x158, its flags word).
const FLAG_BYTE_AT: u32 = 0x164;

/// `CMaterialManager::Tag520940` (0x520940): the entry of material `tag`, which is the first entry for tag 0, for a tag
/// out of range, and for a material whose flag byte has bit 0 clear. The manager's `Tag520940` is its only user of
/// the flag byte.
pub fn tag_entry(mem: &Mem, manager: u32, tag: u32) -> u32 {
    if tag >= TAGS || (tag != 0 && layout::byte(mem, manager + tag * SIZE + FLAG_BYTE_AT) & 1 == 0) {
        return entry(manager, 0);
    }
    entry(manager, tag)
}

/// The manager's entry for material `tag`, flag or not: where the walk finds the material a key of the manager names
/// (`CBase::Unk34`, the key's word `88 * tag + 3` times four).
pub fn entry(manager: u32, tag: u32) -> u32 {
    manager + FIRST_AT + tag * SIZE
}

/// `CMaterialManager`'s vtable (0x73d86c) and name (`"MaterialManager"`, 0x73d8b8).
const MANAGER_VTABLE: u32 = 0x0073_d86c;
const MANAGER_NAME: u32 = 0x0073_d8b8;

/// The header `CMaterialManager::CMaterialManager` (0x520b30) writes before its materials.
pub fn manager_image() -> TweakHeader {
    TweakHeader { vtable: MANAGER_VTABLE, name: MANAGER_NAME, dirty: 0 }
}

/// `CMaterial::CMaterial` (0x520980) on the object at `at`.
pub fn construct(mem: &mut Mem, at: u32) {
    layout::store_material(mem, at, &image());
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANAGER: u32 = 0x2000;

    /// A manager (0x1600c bytes) whose material `tag`'s flag byte is `flag`.
    fn manager(tag: u32, flag: u8) -> Mem {
        let mut mem = Mem::new();
        mem.map(MANAGER, &vec![0; 0x1600c]);
        mem.map(MANAGER + tag * SIZE + FLAG_BYTE_AT, &[flag]);
        mem
    }

    #[test]
    fn tag_zero_is_the_first_entry() {
        assert_eq!(tag_entry(&manager(0, 0), MANAGER, 0), MANAGER + FIRST_AT);
    }

    #[test]
    fn enabled_tag_is_its_entry_and_disabled_is_the_first() {
        let on = manager(3, 1);
        assert_eq!(tag_entry(&on, MANAGER, 3), MANAGER + FIRST_AT + 3 * SIZE);
        let off = manager(3, 0);
        assert_eq!(tag_entry(&off, MANAGER, 3), MANAGER + FIRST_AT);
    }

    #[test]
    fn out_of_range_tag_is_the_first_entry() {
        let mem = manager(0, 0);
        assert_eq!(tag_entry(&mem, MANAGER, TAGS), MANAGER + FIRST_AT);
    }
}
