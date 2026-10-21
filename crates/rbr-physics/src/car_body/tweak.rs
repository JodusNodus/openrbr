//! A tweak key's float triple, loaded from a binary block of four words.

use crate::car_body::store::CarStore;

/// Reads the four words at `data` (all of them, before any store, as the original's paired 16-byte moves do) into the
/// triple at `this`, and returns `this`.
pub fn load_binary<S: CarStore>(mem: &mut S, this: u32, data: u32) -> u32 {
    let words: [u32; 4] = std::array::from_fn(|i| mem.u32(data + 4 * i as u32));
    for (i, word) in words.into_iter().enumerate() {
        mem.set_u32(this + 4 * i as u32, word);
    }
    this
}
