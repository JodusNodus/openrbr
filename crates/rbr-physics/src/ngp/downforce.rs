//! NGP's aerodynamic downforce, applied each step. The car's `FrontDownforce_NGP` and `RearDownforce_NGP` are the
//! forces at 41.67 m/s (150 km/h) and sea-level density; they scale with the square of the speed and with the air
//! density, and with the damage state of the aero parts. It also sets the drag factor NGP's drag uses in place of
//! 1.02's 0.5.

const HALF: f32 = 0.5;
/// The sea-level air density the forces are given at, kg/m^3.
const REFERENCE_DENSITY: f32 = 1.2040719;
/// The speed the forces are given at, m/s.
const REFERENCE_SPEED: f32 = 41.666668;
/// The factor on a positive force when reversing.
const REVERSE_FACTOR: f32 = -0.5;

/// What the downforce reads.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DownforceIn {
    /// The damage states of the front and rear aero parts (0 intact, 1 half, 2 gone), when the car has a damage
    /// object.
    pub damage: Option<[u32; 2]>,
    /// `FrontDownforce_NGP`, `RearDownforce_NGP`.
    pub front: f32,
    pub rear: f32,
    /// The air density, kg/m^3.
    pub density: f32,
    /// The car's signed speed along its axis, m/s.
    pub speed: f32,
    /// The arms of wheels 0 to 3 (the front pair, then the rear pair) that carry the moment of the forces.
    pub wheel_arms: [f32; 4],
    /// The sums of force and moment it adds to.
    pub force_sum: f32,
    pub moment_sum: f32,
}

/// The way through the computation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Path {
    /// Neither (damage-scaled) downforce is nonzero.
    NoDownforce,
    /// No speed (0 or NaN): the speed's direction, 0, 1 or -1.
    NoSpeed(i32),
    Applied,
}

/// What it gives.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DownforceOut {
    pub path: Path,
    /// The square of the speed over the reference speed.
    pub speed_ratio_sq: f32,
    pub front_force: f32,
    pub rear_force: f32,
    pub force_sum: f32,
    pub moment_sum: f32,
    pub drag_factor: f32,
}

pub fn downforce(i: &DownforceIn) -> DownforceOut {
    let density_ratio = i.density / REFERENCE_DENSITY;
    let mut scale = [1.0f32; 2];
    if let Some(states) = i.damage {
        for (s, state) in scale.iter_mut().zip(states) {
            match state {
                0 => *s = 1.0,
                1 => *s = 0.5,
                2 => *s = 0.0,
                _ => {}
            }
        }
    }
    let (mut front, mut rear) = (scale[0] * i.front, scale[1] * i.rear);
    let mut out = DownforceOut {
        path: Path::NoDownforce,
        speed_ratio_sq: 0.0,
        front_force: 0.0,
        rear_force: 0.0,
        force_sum: i.force_sum,
        moment_sum: i.moment_sum,
        drag_factor: density_ratio * HALF,
    };
    if !(front.abs() > 0.0 || rear.abs() > 0.0) {
        return out;
    }
    let v = i.speed;
    let direction = super::sign(v);
    if !(v.abs() > 0.0) {
        out.path = Path::NoSpeed(direction);
        return out;
    }
    let q = (v / REFERENCE_SPEED) * (v / REFERENCE_SPEED);
    if direction < 0 {
        if front > 0.0 {
            front *= REVERSE_FACTOR;
        }
        if rear > 0.0 {
            rear *= REVERSE_FACTOR;
        }
    }
    let f = -q * front * density_ratio;
    let r = -q * rear * density_ratio;
    let [front_0, front_1, rear_2, rear_3] = i.wheel_arms;
    out.path = Path::Applied;
    out.speed_ratio_sq = q;
    out.front_force = f;
    out.rear_force = r;
    out.force_sum = r + f + i.force_sum;
    out.moment_sum = ((rear_3 + rear_2) * r + (front_1 + front_0) * f) * HALF + i.moment_sum;
    out
}
