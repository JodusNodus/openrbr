//! E33's inherited tweak dictionary helpers.

use crate::mem::Mem;

/// The parent-dictionary virtual calls used by `Unk0C` and `Unk10`.
pub trait DictHost {
    fn count(&mut self, mem: &Mem, parent: u32) -> u32;
    fn entry(&mut self, mem: &Mem, parent: u32, index: i32) -> u32;
    fn lookup_start(&mut self, mem: &Mem, dictionary: u32) -> u32;
    fn find(&mut self, mem: &Mem, dictionary: u32, name: u32, index_out: u32) -> u32;
}

/// E33Dict::Unk18 (0x4e0b70), which delegates lookup through two virtual calls.
pub fn lookup(mem: &Mem, this: u32, name: u32, index_out: u32, host: &mut impl DictHost) -> u32 {
    if host.lookup_start(mem, this) == 0 {
        return 0;
    }
    let dictionary = host.lookup_start(mem, this);
    host.find(mem, dictionary, name, index_out)
}

/// E33Dict::Unk0C (0x4e0bd0).
pub fn count(mem: &Mem, this: u32, host: &mut impl DictHost) -> u32 {
    let parent = mem.u32(this + 0x10);
    let base = mem.u32(this + 8);
    if parent == 0 {
        base
    } else {
        base.wrapping_add(host.count(mem, parent))
    }
}

/// E33Dict::Unk10 (0x4e0c00).
pub fn entry(mem: &Mem, this: u32, index: i32, host: &mut impl DictHost) -> u32 {
    let parent = mem.u32(this + 0x10);
    let base = mem.u32(this + 0x0c);
    if parent == 0 {
        return base.wrapping_add(index.wrapping_mul(0x10) as u32);
    }

    let parent_count = host.count(mem, parent);
    if (index as u32) < parent_count {
        host.entry(mem, parent, index)
    } else {
        // The reference makes a second virtual count call on this path.
        let parent_count = host.count(mem, parent) as i32;
        base.wrapping_add(index.wrapping_sub(parent_count).wrapping_mul(0x10) as u32)
    }
}
