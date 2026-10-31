//! The tweak store (`crate::store`): the words the NTweak walk and `SetDirty` read and write. `Mem` implements it in
//! `layout/mod.rs`. The tweak value bodies are [`crate::store::Real`] and its siblings.

use crate::store::Raw;

pub trait TweakStore: Raw {
    /// The word at `at`: a member's value, as the walk sees it before its own stores.
    fn word(&self, at: u32) -> u32;
    /// The dispatch-table word of the tweak or object at `at`.
    fn vtable(&self, at: u32) -> u32;
    /// The parse entry of `table`, by which the walk tells the members apart.
    fn reader(&self, vtable: u32) -> u32;
    /// The object's inner-group entry, which takes the walk's inner groups.
    fn unk14(&self, object: u32) -> u32;
    /// The names of a bit-field class's bits (over its table).
    fn bit_names(&self, table: u32, count: u32) -> Vec<String>;
    /// The dirty flag of the tweak header at `at`.
    fn store_dirty(&mut self, at: u32, value: u32);
}
