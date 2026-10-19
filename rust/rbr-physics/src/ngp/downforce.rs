//! NGP's aerodynamic downforce (`0x10047040`, called each step from `0x10048d50`, which NGP puts in
//! `CDrive::Step1E0`'s call at `0x50a46e`). The car definition's `FrontDownforce_NGP` and `RearDownforce_NGP`
//! (`CCar::CDef +0x1f0`, `+0x200`, values at `+0xc`) are forces at 41.67 m/s (150 km/h) and sea-level density; they
//! scale with the square of the speed and with the density. It also sets the drag factor (`1017147c`) that NGP's
//! `CCar::DragAndSpinDamp` uses in place of 1.02's 0.5.

const HALF: f32 = f32::from_bits(0x3F00_0000); // 10147c58
const RHO0: f32 = f32::from_bits(0x3F9A_1F07); // 101487b8, 1.2040719
const V_REF: f32 = f32::from_bits(0x4226_AAAB); // 10147c6c, 41.666668
const REVERSE: f32 = f32::from_bits(0xBF00_0000); // 10147c74, -0.5

/// What `0x10047040` reads.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DownforceIn {
    /// The two aero parts' damage states (`*(car+0x9f4)` `+0x3db4`, `+0x3dbc`: 0 intact, 1 half, 2 gone), when the
    /// car has a damage object.
    pub damage: Option<[u32; 2]>,
    /// `FrontDownforce_NGP`, `RearDownforce_NGP`.
    pub front: f32,
    pub rear: f32,
    /// The air density (`10171ae0`, `atmosphere`).
    pub density: f32,
    /// The car's speed (`car+0x15f8`, signed).
    pub speed: f32,
    /// `+0x8b4` of the wheels at `car+0x470`, `+0x474`, `+0x478`, `+0x47c`.
    pub wheel_arms: [f32; 4],
    /// The force and moment sums it adds to (`car+0x378`, `car+0x310`).
    pub force_sum: f32,
    pub moment_sum: f32,
}

/// The way through `0x10047040` (its callers' `eax` differs by it).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Path {
    /// Neither (damage-scaled) downforce is nonzero.
    NoDownforce,
    /// No speed (0 or NaN): the speed's direction, 0, 1 or -1.
    NoSpeed(i32),
    Applied,
}

/// What it writes: `car+0x4a4`, `+0x4a8`, `+0x4ac`, the sums, and the drag factor.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DownforceOut {
    pub path: Path,
    pub speed_ratio_sq: f32,
    pub front_force: f32,
    pub rear_force: f32,
    pub force_sum: f32,
    pub moment_sum: f32,
    pub drag_factor: f32,
}

pub fn downforce(i: &DownforceIn) -> DownforceOut {
    let rho = i.density / RHO0;
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
        drag_factor: rho * HALF,
    };
    if !(front.abs() > 0.0 || rear.abs() > 0.0) {
        return out;
    }
    let v = i.speed;
    // 0 at a speed of exactly 0, else the sign (NaN takes its sign bit)
    let direction: i32 = if v == 0.0 {
        0
    } else if v.is_sign_negative() {
        -1
    } else {
        1
    };
    if !(v.abs() > 0.0) {
        out.path = Path::NoSpeed(direction);
        return out;
    }
    let q = (v / V_REF) * (v / V_REF);
    if direction < 0 {
        if front > 0.0 {
            front *= REVERSE;
        }
        if rear > 0.0 {
            rear *= REVERSE;
        }
    }
    let f = -q * front * rho;
    let r = -q * rear * rho;
    let [w470, w474, w478, w47c] = i.wheel_arms;
    out.path = Path::Applied;
    out.speed_ratio_sq = q;
    out.front_force = f;
    out.rear_force = r;
    out.force_sum = r + f + i.force_sum;
    out.moment_sum = ((w47c + w478) * r + (w474 + w470) * f) * HALF + i.moment_sum;
    out
}
