//! Detached car parts (rbr-recomp `notes/car-damage-visuals.md`, section 8). The physics detaches a part's body from the car
//! (`rbr_recording::detach`, the `DET1` record). The body's motion after that is the physics', so the viewer draws
//! the part's mesh with the body's pose.
//!
//! The mesh stays in the car's model space. At the first record that lists the part, its placement on the car is the
//! placement the car draws it at, `attached`, and its body is at `body`. From then on the mesh's placement is the body's
//! pose times the same offset: `body_now * (body^-1 * attached)`, a rigid move with the body. A part whose body has
//! fallen asleep (left out of later records) keeps its last placement.
//!
//! This is ours (not traced): the viewer shows the body's pose at each step, and the mesh's offset at the detach is the
//! car's placement at the first record. The one-step gap between the detach and that record is not drawn.

use glam::Mat4;

/// A detached part's placement from its body's pose.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Detach {
    /// The mesh's placement relative to its body: `body^-1 * attached` at the first record. Constant after it.
    relative: Mat4,
    /// The mesh's placement at the latest record.
    placed: Mat4,
}

impl Detach {
    /// The first record of a part: its body's stage pose `body` and the car's placement of the mesh `attached`, both at
    /// the same step. The mesh is placed at `attached` until the body moves.
    pub fn start(body: Mat4, attached: Mat4) -> Detach {
        Detach { relative: body.inverse() * attached, placed: attached }
    }

    /// The body's stage pose at a later record: the mesh moves with it.
    pub fn follow(&mut self, body: Mat4) {
        self.placed = body * self.relative;
    }

    /// The mesh's placement (model space to stage space).
    pub fn placement(&self) -> Mat4 {
        self.placed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::{Quat, Vec3};

    fn close(a: Mat4, b: Mat4) -> bool {
        a.to_cols_array().iter().zip(b.to_cols_array()).all(|(x, y)| (x - y).abs() < 1e-5)
    }

    #[test]
    fn a_detached_part_starts_where_the_car_draws_it_and_moves_with_its_body() {
        let attached = Mat4::from_translation(Vec3::new(1.0, 0.0, 2.0));
        let body = Mat4::from_translation(Vec3::new(1.0, 0.1, 2.0));
        let mut detach = Detach::start(body, attached);
        assert!(close(detach.placement(), attached));
        // The body turns and drops: the mesh turns and drops with it, about the same point.
        let moved = Mat4::from_rotation_translation(Quat::from_rotation_z(0.5), Vec3::new(3.0, -1.0, 0.5));
        let body_now = moved * body;
        detach.follow(body_now);
        let want = body_now * body.inverse() * attached;
        assert!(close(detach.placement(), want));
        // The same body pose again places the mesh the same way.
        detach.follow(body_now);
        assert!(close(detach.placement(), want));
    }
}
