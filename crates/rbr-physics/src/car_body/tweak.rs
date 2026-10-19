//! NTweak's float triple (exe 0x4f3d20). The load reads the four words of a binary block into the triple.

use crate::car_body::store::CarStore;

/// `NTweak::CFloat3::LoadBinary(data)` (0x4f3d20, `ret 4`): reads the 16 bytes at `data` (all of them, before any store,
/// as the asm's `movups` pair does) into `this`, and returns `this`.
pub fn load_binary<S: CarStore>(mem: &mut S, this: u32, data: u32) -> u32 {
    let words: [u32; 4] = std::array::from_fn(|i| mem.u32(data + 4 * i as u32));
    for (i, word) in words.into_iter().enumerate() {
        mem.set_u32(this + 4 * i as u32, word);
    }
    this
}
