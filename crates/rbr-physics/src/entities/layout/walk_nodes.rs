//! The walked nodes (`E43PairWalk`): the node and pair words, the copies the walks make, the name list and the
//! virtual-slot targets.

use super::construct::OLD_FRAME;
use crate::entities::walk::{Node, Walker};
use crate::mem::Mem;
pub use crate::physics_core::layout::vtable_target;

/// Offsets of the child link and the depth of a walked node (`E43PairWalk::W9D0` and the others).
const NODE_CHILD: u32 = 0x04;
const NODE_DEPTH: u32 = 0x08;

/// Copies the 16 bytes at `from` to `to` (the `E43Mat16` assignments).
pub fn copy_mat16(mem: &mut Mem, to: u32, from: u32) {
    mem.copy_words(to, from, 4);
}

/// The words of a contact grid (`CColGrid`, 0x6b0 bytes).
const GRID_WORDS: u32 = 0x1ac;

/// `E37GridOps::Copy6770` (0x4f6770) on `to`: copies the grid at `from` word by word (`rep movsd`); nothing when `to`
/// is 0.
pub fn copy_grid(mem: &mut Mem, to: u32, from: u32) {
    if to == 0 {
        return;
    }
    mem.copy_words(to, from, GRID_WORDS);
}

/// The factory-name list of a pair walker (`E43PairWalk::MF8B0`): its count at +0x18, its indices from +0x1c.
const NAME_COUNT: u32 = 0x18;
const NAME_LIST: u32 = 0x1c;

/// Appends `index` to the factory-name list of the walker at `owner`.
pub fn append_name_index(mem: &mut Mem, owner: u32, index: u32) {
    let count = mem.u32(owner + NAME_COUNT);
    mem.set_u32(owner + NAME_LIST + 4 * count, index);
    mem.set_u32(owner + NAME_COUNT, count + 1);
}

/// The node at `at`: its child link and depth.
pub fn load_node(mem: &Mem, at: u32) -> Node {
    Node { child: mem.u32(at + NODE_CHILD), depth: mem.u32(at + NODE_DEPTH) }
}

/// Offsets of the two words a pair node holds (`E43PairWalk::MD50`, `MD70`).
const PAIR_FIRST: u32 = 0x00;
const PAIR_SECOND: u32 = 0x04;

/// `E43PairWalk::MD50` and `MD70` (0x51ad50, 0x51ad70; the same code): stores `first` and `second` in the
/// node and returns the node.
pub fn store_pair(mem: &mut Mem, node: u32, first: u32, second: u32) -> u32 {
    mem.set_u32(node + PAIR_FIRST, first);
    mem.set_u32(node + PAIR_SECOND, second);
    node
}

/// A hook the original leaves empty (`CEntity` 0x519b90: `ret`). Its eax is whatever the caller left.
pub fn empty_hook() {}

/// `E43PairWalk::MFc00` (0x519c00): the node itself (`return this`).
pub fn node_of(this: u32) -> u32 {
    this
}

/// `E43PairWalk::MFc10` (0x519c10): the node's old frame.
pub fn old_frame_of(this: u32) -> u32 {
    this + OLD_FRAME
}

/// The slot `CEntity::Vf120` (0x519db0) jumps to: the entity's slot 0x11c (`mov eax, [ecx]; jmp [eax + 0x11c]`).
pub fn vf120_target(mem: &Mem, entity: u32) -> u32 {
    mem.u32(mem.u32(entity) + 0x11c)
}

/// The contact grids' stride (`CColGrid`, 0x6b0 bytes) and the two box tables the grid hooks get: the first pass's (+0x40)
/// and the second's (+0x220).
pub const GRID_STRIDE: u32 = 0x6B0;
pub const GRID_FIRST_BOXES: u32 = 0x40;
pub const GRID_SECOND_BOXES: u32 = 0x220;

/// The original's functions the walkers stand for (`E43PairWalk`).
pub fn walker_address(walker: Walker) -> u32 {
    match walker {
        Walker::PairOneTwo => 0x51A9D0,
        Walker::PairThreeFour => 0x51AA20,
        Walker::PairFiveSix => 0x51AA70,
        Walker::PairOneTwoDeeper => 0x51AFE0,
        Walker::PairOneTwoShallower => 0x51B050,
        Walker::PairThreeFourDeeper => 0x51B0C0,
        Walker::PairThreeFourShallower => 0x51B130,
        Walker::PairFiveSixDeeper => 0x51B1A0,
        Walker::PairFiveSixShallower => 0x51B210,
        Walker::RoutePairOneTwo => 0x51B320,
        Walker::RoutePairThreeFour => 0x51B370,
        Walker::RoutePairFiveSix => 0x51B3C0,
        Walker::PairGrids => 0x51AAC0,
        Walker::PairGridsDeeper => 0x51B280,
        Walker::PairGridsShallower => 0x51B2D0,
        Walker::RoutePairGrids => 0x51B410,
    }
}
