//! `CBody::VfD0` (0x4e1eb0) and `CBody::VfDC` (0x4e2850): the point, edge and pair contacts against another entity,
//! are `collision::body_contact`, generic over `CollisionStore`.

pub use crate::collision::body_contact::{body_vf_d0, body_vf_dc};
