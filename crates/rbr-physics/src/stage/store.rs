//! The stage's store (`crate::store`): the collision-system selection the probes make every step. `Mem` implements it
//! in `layout/queries.rs`. The rest of the stage is the load's construction and stays on `&mut Mem`.

use super::layout::CollisionQuery;
use crate::store::Raw;

pub trait StageStore: Raw {
    /// The collision system `query`'s reader uses (NGP's first copy).
    fn collision_system(&self, query: CollisionQuery) -> u32;
}
