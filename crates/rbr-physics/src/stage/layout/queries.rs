//! Collision-system selection for a boot, or for the original's captured memory.

use crate::mem::Mem;

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
    pub fn system(self, mem: &Mem) -> u32 {
        if let Some(runtime) = mem.ngp_runtime() {
            return runtime.collision_systems[0];
        }
        // Validation snapshots contain the patched instruction operands; vector records contain only data.
        let operand = match self {
            Self::EntityCollision => 0x51bda4,
            Self::GroundProbe => 0x51c726,
            Self::CarProbe => 0x4e54e8,
            Self::BodyProbe => 0x529d5a,
            Self::WheelProbe => 0x53d5e0,
        };
        let mut word = [0; 4];
        let at = if mem.try_read_into(operand, &mut word) { u32::from_le_bytes(word) } else { super::COLLISION_SYSTEM };
        mem.u32(at)
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
