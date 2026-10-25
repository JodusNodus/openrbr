//! Collision-system selection for a boot, or for the original's captured memory.

use crate::mem::Mem;
use crate::stage::store::StageStore;

/// Readers of NGP's first collision-system copy. Each snapshot operand identifies its original reader.
#[derive(Clone, Copy)]
pub enum CollisionQuery {
    EntityCollision,
    GroundProbe,
    CarProbe,
    BodyProbe,
    WheelProbe,
}

impl CollisionQuery {
    pub fn system(self, store: &impl StageStore) -> u32 {
        store.collision_system(self)
    }
}

impl StageStore for Mem {
    fn collision_system(&self, query: CollisionQuery) -> u32 {
        if let Some(runtime) = self.ngp_runtime() {
            return runtime.collision_systems[0];
        }
        // Validation snapshots contain the patched instruction operands; vector records contain only data.
        let operand = match query {
            CollisionQuery::EntityCollision => 0x51bda4,
            CollisionQuery::GroundProbe => 0x51c726,
            CollisionQuery::CarProbe => 0x4e54e8,
            CollisionQuery::BodyProbe => 0x529d5a,
            CollisionQuery::WheelProbe => 0x53d5e0,
        };
        let mut word = [0; 4];
        let at =
            if self.try_read_into(operand, &mut word) { u32::from_le_bytes(word) } else { super::COLLISION_SYSTEM };
        self.u32(at)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ngp::layout::runtime::Runtime;

    #[test]
    fn boot_queries_use_the_same_copy_without_instruction_bytes() {
        let mut mem = Mem::new();
        mem.set_ngp_runtime(Runtime::new([0x1000, 0x2000, 0x3000], 1));
        for query in [
            CollisionQuery::EntityCollision,
            CollisionQuery::GroundProbe,
            CollisionQuery::CarProbe,
            CollisionQuery::BodyProbe,
            CollisionQuery::WheelProbe,
        ] {
            assert_eq!(query.system(&mem), 0x1000);
        }
        assert!(mem.try_read(0x53d5e0, 4).is_none());
    }
}
