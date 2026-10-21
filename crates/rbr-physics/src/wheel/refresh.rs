//! The suspension geometry of one wheel for a strut travel. The wishbone swings about the joint axis, so the top mount, the
//! strut and the wishbone close into a triangle that gives the wishbone's angle; the steering rod closes a second one
//! about the strut axis. From them come the knuckle's frame, the hub position, the wheel's rotation and the steering
//! lever factors. The sequence (the reads, host calls and stores) is `layout::refresh`; this module holds the pure stages.
//! Operation order is the original's (its x87 products are f32 products at 24-bit precision, so each one is an f32
//! operation here). Under NGP the x87 runs at 53 bits: its three chains (the strut length and the two `sqrt(1 - c*c)`)
//! round once, at their store.
//!
//! The stages split at the steering target, the only call whose result the later arithmetic reads. The fourth lane of a
//! vector the original never writes is 0 here (a project decision: the original reads stack leftovers there).

use crate::fpu::{precision, Precision};
use crate::math::vec::{Vec3, Vec4};
use crate::math::Mat3;

/// How much of the damage record's steering word shortens the steering rod (named by use).
const STEERING_DAMAGE_SCALE: f32 = 0.015;
/// The size below which a lever term counts as zero.
const MIN_LEVER: f32 = 0.001;

/// The wheel definition's suspension geometry. Positions and axes are in the world, as the car's transforms leave them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SuspensionDef {
    /// `SteeringRodLength`, m.
    pub steering_rod_length: f32,
    /// The two header words of the `WishboneLength` real (its label and dirty words), which the wheel keeps a copy of.
    pub wishbone_header: [u32; 2],
    /// `WishboneLength`, m.
    pub wishbone_length: f32,
    /// `vecJointMount`: a point of the wishbone's joint axis.
    pub joint_mount: Vec3,
    /// `vecJointAxis`: the axis the wishbone swings about.
    pub joint_axis: Vec4,
    /// `WishboneSign`: the sign of the wishbone's sine.
    pub wishbone_sign: f32,
    /// `StrutDimX`: the strut's x extent.
    pub strut_dim_x: f32,
    /// The real after `StrutDimX` in the definition, which the strut's z extent takes (named by use: the unit strut
    /// direction is `(strut_dim_x, 0, travel + strut_dim_z)` over the strut length; the key table lists `StrutDimZ` two
    /// reals further on).
    pub strut_dim_z: f32,
    /// `SteeringSign`: the sign of the steering rod's sine.
    pub steering_sign: f32,
    /// `vecStrutRodMount`: where the steering rod meets the strut, in the strut's frame.
    pub strut_rod_mount: Vec3,
    /// `vecStrutHubPoint`: the hub's point in the strut's frame.
    pub strut_hub_point: Vec3,
    /// `IsSteered`: whether the car's steering target moves the steering mount.
    pub is_steered: bool,
    /// `DriveAxleMountOffset`: how far the drive axle's mount is along the wheel axle from the hub.
    pub drive_axle_mount_offset: f32,
    /// `vecSteeringMount`: the steering rack end that the steering rod reaches.
    pub steering_mount: Vec4,
    /// `vecSteeringAxis`: the axis the steering mount moves along.
    pub steering_axis: Vec4,
    /// `vecDriveAxleJointPos`: the drive axle's inner joint.
    pub drive_axle_joint: Vec3,
}

/// What the refresh takes from the wheel and its damage record, read at the entry (none of it is written by the refresh
/// before it is used, except the top mount and the axle, which the caller stores first and passes here).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PoseIn {
    /// The strut's travel, m.
    pub travel: f32,
    pub def: SuspensionDef,
    /// The damage record's steering word (named by use: `STEERING_DAMAGE_SCALE` of it shortens the steering rod).
    pub steering_damage: f32,
    /// The top mount, deformed by damage (`axes::deformed_top_mount`).
    pub top_mount: Vec4,
    /// The wheel axle, wobbled by damage (`axes::wobbled_axle`; three lanes are used).
    pub axle: Vec4,
}

/// Stage 1: the strut's geometry, up to the steering target.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StrutGeometry {
    /// The steering rod's length after damage, m.
    pub steering_rod_length: f32,
    /// The unit vector from the joint axis to the top mount, across the axis (lane 3 is 0).
    pub radial_dir: Vec4,
    /// `joint_axis x radial_dir`.
    pub tangent_dir: Vec3,
    /// `(0, 0, 1) x axle`, the second row of the axle's frame.
    pub axle_side: Vec3,
    /// `axle x axle_side`, the third row of the axle's frame.
    pub axle_normal: Vec3,
    /// The strut's length: `sqrt(strut_dim_x^2 + (travel + strut_dim_z)^2)`.
    pub strut_length: f32,
    /// The cosine of the wishbone's angle, clamped to [-1, 1].
    pub wishbone_cos: f32,
    /// The sine of the wishbone's angle, times the wishbone sign.
    pub wishbone_sin: f32,
}

/// Stage 1, pure: the geometry of the strut from the top mount, the axle and the joint axis.
pub fn strut_geometry(input: &PoseIn) -> StrutGeometry {
    let def = &input.def;
    let p = input.top_mount;
    let steering_rod_length = def.steering_rod_length - input.steering_damage * STEERING_DAMAGE_SCALE;
    // `d` is the offset of the top mount from the joint mount, projected on the axis (summed z, y, x).
    let d = Vec3 { x: def.joint_mount.x - p.x, y: def.joint_mount.y - p.y, z: def.joint_mount.z - p.z };
    let along_axis = (def.joint_axis.z * d.z + def.joint_axis.y * d.y) + def.joint_axis.x * d.x;
    // The axis times the projection, plus the top mount: the top mount's offset across the axis, from the joint mount.
    let v = Vec3 {
        x: def.joint_axis.x * along_axis + p.x,
        y: def.joint_axis.y * along_axis + p.y,
        z: def.joint_axis.z * along_axis + p.z,
    };
    let radial = Vec3 { x: v.x - def.joint_mount.x, y: v.y - def.joint_mount.y, z: v.z - def.joint_mount.z };
    let radius = radial.length3();
    // The normalise is a scale by the f32 reciprocal (the x87 division at 24 bits, then the scale's product).
    let inv = 1.0 / radius;
    let radial_dir = Vec4 { x: radial.x * inv, y: radial.y * inv, z: radial.z * inv, w: 0.0 };
    let axle = Vec3 { x: input.axle.x, y: input.axle.y, z: input.axle.z };
    let axle_side = Vec3 { x: 0.0, y: 0.0, z: 1.0 }.cross3(axle);
    let axle_normal = axle.cross3(axle_side);
    let joint_axis = Vec3 { x: def.joint_axis.x, y: def.joint_axis.y, z: def.joint_axis.z };
    let tangent_dir = joint_axis.cross3(Vec3 { x: radial_dir.x, y: radial_dir.y, z: radial_dir.z });
    // The strut's length: the squares, their sum, the root.
    let dim_x = def.strut_dim_x;
    let strut_length = match precision() {
        Precision::Pc24 => {
            let dim_z = input.travel + def.strut_dim_z;
            (dim_x * dim_x + dim_z * dim_z).sqrt()
        }
        Precision::Pc53 => {
            let dim_z = f64::from(input.travel) + f64::from(def.strut_dim_z);
            (f64::from(dim_x) * f64::from(dim_x) + dim_z * dim_z).sqrt() as f32
        }
    };
    // The wishbone's angle from the three lengths (the law of cosines; f32 products and sums).
    let wishbone = def.wishbone_length;
    let num = (wishbone * wishbone + radius * radius) - (strut_length * strut_length - along_axis * along_axis);
    let den = (wishbone * radius) * 2.0;
    let (wishbone_cos, sin) = clamp_cos(num / den);
    StrutGeometry {
        steering_rod_length,
        radial_dir,
        tangent_dir,
        axle_side,
        axle_normal,
        strut_length,
        wishbone_cos,
        wishbone_sin: sin * def.wishbone_sign,
    }
}

/// `a * s + b` on the four lanes (the product rounds, then the sum).
fn madd(a: Vec4, s: f32, b: Vec4) -> Vec4 {
    a.scale(s) + b
}

/// Clamps a cosine to [-1, 1] and gives the sine of its angle: a value at or above 1 (or at or below -1) is the clamp with
/// sine 0; a NaN takes the square root path (`sqrt(1 - c*c)`, NaN).
fn clamp_cos(cos: f32) -> (f32, f32) {
    if cos >= 1.0 {
        (1.0, 0.0)
    } else if -1.0 >= cos {
        (-1.0, 0.0)
    } else {
        let sin = match precision() {
            Precision::Pc24 => (1.0 - cos * cos).sqrt(),
            Precision::Pc53 => (1.0 - f64::from(cos) * f64::from(cos)).sqrt() as f32,
        };
        (cos, sin)
    }
}

/// Stage 2: the wishbone's tip, the strut's direction and the strut's local frame, up to the steering target.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Linkage {
    /// The wishbone's tip: the joint mount plus the wishbone (three lanes).
    pub tip: Vec3,
    /// The unit vector from the tip to the top mount (lane 3 is 0).
    pub strut_dir: Vec4,
    /// The x component of the strut's direction in the strut's local frame, `strut_dim_x / strut_length`.
    pub local_x: f32,
    /// The strut direction in the local frame dotted with the rod mount.
    pub rod_along_strut: f32,
    /// The length of the rod mount's part across the strut direction.
    pub rod_radius: f32,
    /// The length of the local x axis' part across the strut direction.
    pub x_across_len: f32,
    /// The unit across part of the local x axis dotted with the unit across part of the rod mount.
    pub dot_a: f32,
    /// The same unit across part of the local x axis dotted with `local_dir x rod_across_dir`.
    pub dot_b: f32,
}

/// Stage 2: the geometry after the wishbone's angle.
pub fn linkage(input: &PoseIn, g: &StrutGeometry) -> Linkage {
    let def = &input.def;
    let p = input.top_mount;
    // The wishbone: `radial_dir * cos`, then `+= tangent_dir * sin`, then times its length.
    let tangent4 = Vec4 { x: g.tangent_dir.x, y: g.tangent_dir.y, z: g.tangent_dir.z, w: 0.0 };
    let wishbone = madd(tangent4, g.wishbone_sin, g.radial_dir.scale(g.wishbone_cos));
    let wishbone = wishbone.scale(def.wishbone_length);
    let tip = Vec3 {
        x: def.joint_mount.x + wishbone.x,
        y: def.joint_mount.y + wishbone.y,
        z: def.joint_mount.z + wishbone.z,
    };
    let to_top = Vec3 { x: p.x - tip.x, y: p.y - tip.y, z: p.z - tip.z };
    // `1 / strut_length` scales the offset and the local direction (the x87 division, then the scale's product).
    let inv_length = 1.0 / g.strut_length;
    let strut_dir = Vec4 { x: to_top.x * inv_length, y: to_top.y * inv_length, z: to_top.z * inv_length, w: 0.0 };
    let local_dir = Vec4 {
        x: def.strut_dim_x * inv_length,
        y: 0.0 * inv_length,
        z: (def.strut_dim_z + input.travel) * inv_length,
        w: 0.0,
    };
    let rod = def.strut_rod_mount;
    let rod_along_strut = (rod.z * local_dir.z + rod.y * local_dir.y) + rod.x * local_dir.x;
    // The rod mount's part across the strut direction, and its unit vector.
    let scaled = local_dir.scale(0.0 - rod_along_strut);
    let rod_across = Vec4 { x: scaled.x + rod.x, y: scaled.y + rod.y, z: scaled.z + rod.z, w: 0.0 };
    let rod_radius = Vec3 { x: rod_across.x, y: rod_across.y, z: rod_across.z }.length3();
    let inv_radius = 1.0 / rod_radius;
    let rod_across_dir = rod_across.scale(inv_radius);
    let local_dir3 = Vec3 { x: local_dir.x, y: local_dir.y, z: local_dir.z };
    let rod_tangent = local_dir3.cross3(Vec3 { x: rod_across_dir.x, y: rod_across_dir.y, z: rod_across_dir.z });
    // The local x axis' part across the strut direction, `(1 - u_x*u_x, 0, 0 - u_z*u_x)`, and its unit vector.
    let q = local_dir.scale(local_dir.x);
    let x_across = Vec3 { x: 1.0 - q.x, y: 0.0, z: 0.0 - q.z };
    let x_across_len = x_across.length3();
    let x_across_dir = x_across.scale(1.0 / x_across_len);
    let dot_a =
        (x_across_dir.z * rod_across_dir.z + x_across_dir.y * rod_across_dir.y) + x_across_dir.x * rod_across_dir.x;
    let dot_b = (rod_tangent.z * x_across_dir.z + rod_tangent.y * x_across_dir.y) + rod_tangent.x * x_across_dir.x;
    Linkage { tip, strut_dir, local_x: local_dir.x, rod_along_strut, rod_radius, x_across_len, dot_a, dot_b }
}

/// The `row`s weighted per lane as `((a·w0 + b·w1) + c·w2)`. The fourth lane is the original's 16-byte load of the next
/// words: the second lane of `b` times `w0`, the first of `c` times `w1`, and `axle_x` (the axle's first word) times `w2`.
fn weighted_rows(rows: [Vec4; 3], w: Vec3, axle_x: f32) -> Vec4 {
    let [a, b, c] = rows;
    Vec4 {
        x: (a.x * w.x + b.x * w.y) + c.x * w.z,
        y: (a.y * w.x + b.y * w.y) + c.y * w.z,
        z: (a.z * w.x + b.z * w.y) + c.z * w.z,
        w: (b.x * w.x + c.x * w.y) + axle_x * w.z,
    }
}

/// `n · m` as the original sums it: `out[r][k] = (m[2][k]·n[r][2] + m[1][k]·n[r][1]) + m[0][k]·n[r][0]`.
fn matrix_product(n: Mat3, m: Mat3) -> Mat3 {
    let mut out = [[0.0; 3]; 3];
    for (r, row) in out.iter_mut().enumerate() {
        for (k, cell) in row.iter_mut().enumerate() {
            *cell = (m[2][k] * n[r][2] + m[1][k] * n[r][1]) + m[0][k] * n[r][0];
        }
    }
    out
}

/// The pose after the steering target: the knuckle's frame, the wheel's rotation, the hub and the steering levers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HubFrame {
    /// The knuckle's axes in the world (lane 3 is 0).
    pub knuckle_axes: [Vec4; 3],
    /// The wheel's rotation, `axle_frame · knuckle_axes`, where the axle's frame has the rows `axle`, `axle_side` and
    /// `axle_normal`. Its first row is the wheel axle.
    pub rotation: Mat3,
    /// The hub position: the strut hub point through the knuckle's axes, plus the tip.
    pub hub_position: Vec4,
    /// The unit vector from the tip to the top mount.
    pub strut_dir: Vec4,
    pub tip: Vec4,
    /// The third knuckle axis, negated (lane 3 is 0).
    pub neg_knuckle_z: Vec4,
    /// The rod mount in the world: the strut rod mount through the knuckle's axes, plus the tip (lane 3 is 0).
    pub rod_mount: Vec4,
    /// The steering rod: from the rod mount to the steering mount, relative to the tip (lane 3 is 0).
    pub steering_rod: Vec4,
    /// `1 / arm` and `1 / rack`: `arm` is the steering rod's moment arm about the strut direction, `rack` the rod's
    /// direction along the steering axis. Both are 0 when either is within `MIN_LEVER` of zero.
    pub inv_arm: f32,
    pub inv_rack: f32,
}

/// The pose after the steering target `steer_target` (0 when the wheel is not steered) with the steering rod's length
/// `rod_length`.
pub fn hub_frame(input: &PoseIn, g: &StrutGeometry, l: &Linkage, steer_target: f32, rod_length: f32) -> HubFrame {
    let def = &input.def;
    let o = l.strut_dir;
    let c = l.tip;
    // The steering mount moved along the steering axis by the target.
    let steering_point = madd(def.steering_axis, steer_target, def.steering_mount);
    // The rod mount's axis point: along the strut direction from the tip by `rod_along_strut`; and its offset from the
    // steering point.
    let pc =
        Vec3 { x: o.x * l.rod_along_strut + c.x, y: o.y * l.rod_along_strut + c.y, z: o.z * l.rod_along_strut + c.z };
    let d = Vec3 { x: pc.x - steering_point.x, y: pc.y - steering_point.y, z: pc.z - steering_point.z };
    let axial = (o.z * d.z + o.y * d.y) + o.x * d.x;
    // The steering point moved along the strut direction level with the axis point, and its radial offset from it.
    let q = Vec3 {
        x: o.x * axial + steering_point.x,
        y: o.y * axial + steering_point.y,
        z: o.z * axial + steering_point.z,
    };
    let r = Vec3 { x: q.x - pc.x, y: q.y - pc.y, z: q.z - pc.z };
    let radius = r.length3();
    // The steering rod's angle from the three lengths (the law of cosines).
    let rod_radius = l.rod_radius;
    let num = (radius * radius + rod_radius * rod_radius) - (rod_length * rod_length - axial * axial);
    let den = (radius * rod_radius) * 2.0;
    let (cos, sin0) = clamp_cos(num / den);
    let sin = sin0 * def.steering_sign;
    let radial_dir = Vec4 { x: r.x, y: r.y, z: r.z, w: 0.0 }.scale(1.0 / radius);
    let tangent_dir = o.cross3(radial_dir);
    let t1 = cos * l.dot_a - sin * l.dot_b;
    let t2 = cos * l.dot_b + sin * l.dot_a;
    // The knuckle's x axis `(tangent * t2 + radial * t1) * x_across_len + strut_dir * local_x`, its y axis
    // `unit(strut_dir x x_axis)` and its z axis `x_axis x y_axis`.
    let a1 = Vec4 {
        x: tangent_dir.x * t2 + radial_dir.x * t1,
        y: tangent_dir.y * t2 + radial_dir.y * t1,
        z: tangent_dir.z * t2 + radial_dir.z * t1,
        w: 0.0,
    };
    let a2 = a1.scale(l.x_across_len);
    let x_axis = Vec4 { w: 0.0, ..madd(o, l.local_x, a2) };
    let y_axis = Vec4 { w: 0.0, ..o.cross3(x_axis).normalize() };
    let z_axis = Vec4 { w: 0.0, ..x_axis.cross3(y_axis) };
    let knuckle_axes = [x_axis, y_axis, z_axis];
    let m: Mat3 = [[x_axis.x, x_axis.y, x_axis.z], [y_axis.x, y_axis.y, y_axis.z], [z_axis.x, z_axis.y, z_axis.z]];
    let n: Mat3 = [
        [input.axle.x, input.axle.y, input.axle.z],
        [g.axle_side.x, g.axle_side.y, g.axle_side.z],
        [g.axle_normal.x, g.axle_normal.y, g.axle_normal.z],
    ];
    let rotation = matrix_product(n, m);
    let axle_x = input.axle.x;
    let tip = Vec4 { x: c.x, y: c.y, z: c.z, w: 0.0 };
    let hub = weighted_rows(knuckle_axes, def.strut_hub_point, axle_x);
    let hub_position = Vec4 { x: hub.x + c.x, y: hub.y + c.y, z: hub.z + c.z, w: hub.w };
    let rod = weighted_rows(knuckle_axes, def.strut_rod_mount, axle_x);
    let rod_mount = Vec4 { x: rod.x + c.x, y: rod.y + c.y, z: rod.z + c.z, w: 0.0 };
    let steering_rod = Vec4 {
        x: (steering_point.x - c.x) - rod.x,
        y: (steering_point.y - c.y) - rod.y,
        z: (steering_point.z - c.z) - rod.z,
        w: 0.0,
    };
    // The unit steering rod and the rod mount's moment about it give the terms of the two inverses.
    let rod_dir = Vec4 { w: 0.0, ..steering_rod.scale(1.0 / rod_length) };
    let moment = rod.cross3(rod_dir);
    let arm = (moment.x * o.x + moment.z * o.z) + moment.y * o.y;
    let axis = def.steering_axis;
    let rack = (axis.y * rod_dir.y + axis.z * rod_dir.z) + rod_dir.x * axis.x;
    let (inv_arm, inv_rack) =
        if arm.abs() > MIN_LEVER && rack.abs() > MIN_LEVER { (1.0 / arm, 1.0 / rack) } else { (0.0, 0.0) };
    HubFrame {
        knuckle_axes,
        rotation,
        hub_position,
        strut_dir: o,
        tip,
        neg_knuckle_z: Vec4 { w: 0.0, ..z_axis.scale(-1.0) },
        rod_mount,
        steering_rod,
        inv_arm,
        inv_rack,
    }
}

/// The hub after the tyre's `HubOffset` is known: the hub position moves along the wheel `axle` by `-hub_offset`, and the
/// drive axle's direction is the axle mount's position (`drive_axle_mount_offset` along the axle from the moved hub) less
/// the drive axle's joint, normalised. Returns the moved hub position and the drive axle's direction.
pub fn place_hub(
    axle: Vec4,
    hub_offset: f32,
    hub_position: Vec4,
    drive_axle_mount_offset: f32,
    drive_axle_joint: Vec3,
) -> (Vec4, Vec4) {
    let hub = madd(axle, 0.0 - hub_offset, hub_position);
    let mount = madd(axle, drive_axle_mount_offset, hub);
    let along = Vec4 {
        x: mount.x - drive_axle_joint.x,
        y: mount.y - drive_axle_joint.y,
        z: mount.z - drive_axle_joint.z,
        w: 0.0,
    };
    (hub, along.normalize())
}
