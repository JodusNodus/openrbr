//! NGP's damper helpers, which replace or wrap 1.02's `DamperStep1`/`DamperStep2`/`DamperSolve`
//! (`notes/ngp-replacements.md`). `DamperSolve`'s wrapper (`0x10060230`) only passes the air pressure as the limit,
//! so it is [`crate::spring_damper::solve`] with [`super::atmosphere::Atmosphere::pressure`].

/// 293.14 (`10148ae8`): 1.02's temperature bias, which NGP swaps for the air temperature.
const T_BIAS: f32 = f32::from_bits(0x4392_91EC);
const TWO_PI: f32 = f32::from_bits(0x40C9_0FDB); // 10148ae4
const EIGHTH: f32 = f32::from_bits(0x3E00_0000); // 10148adc

/// `0x1004d2f0`, NGP's `DamperStep1` (`0x1004d2c0`, SSE): `(a - b') * (c * d)` with `b' = b - 293.14 + temperature`
/// (the atmosphere's), returned in st0.
pub fn step1(a: f32, b: f32, c: f32, d: f32, temperature: f32) -> f32 {
    let b = b - T_BIAS + temperature;
    (a - b) * (c * d)
}

/// `0x1004d340`, the scratch's damper factor from the words at `*(scratch+0x10)` `+0x7c`, `+0x8c`, `+0xac`.
pub fn scratch_factor(w7c: f32, w8c: f32, wac: f32) -> f32 {
    (w8c + w7c) * TWO_PI * wac * EIGHTH
}

/// `0x1004d380`, NGP's `DamperStep2` (its third argument unused): `-a * b * factor`, x87 (at NGP's 53-bit precision:
/// both products round to double), returned in st0.
pub fn step2(a: f32, b: f32, factor: f32) -> f64 {
    f64::from(-a) * f64::from(b) * f64::from(factor)
}

/// The default convection factor (`10148ae0`) and the speed's exponent (`10147c60`, the double of `0.78f`).
const CONVECTION_DEFAULT: f32 = f32::from_bits(0x40C0_0000); // 6
const SPEED_EXPONENT: f64 = 0.779_999_971_389_770_5;
const HALF: f32 = f32::from_bits(0x3F00_0000); // 10147c58

/// What NGP's convection term (`0x1004d410`) reads besides its arguments.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Convection {
    /// The tyre's words at `*(scratch+0x10)` `+0x7c`, `+0x8c` and `+0xec` (its outer radius and the two that give the
    /// inner one), and its kind (`+0x1c`).
    pub w7c: f32,
    pub w8c: f32,
    pub wec: f32,
    pub kind: u32,
    /// The wheel's spin (`*(scratch+0x410)` `+0xea0`) and the car's speed (the wheel's car, `+0x7f4`, `+0x15f8`).
    pub spin: f32,
    pub car_speed: f32,
    /// `scratch+0x1c8` when `scratch+0x480` is set.
    pub boost: Option<f32>,
    /// NGP's per-car factors for kinds 0 and 1 (`10171470`) and 2 and 3 (`10171474`): the car's `+0xe58` and `+0xe68`,
    /// or 6, set by `0x10045230`.
    pub factors: [f32; 2],
}

/// `0x1004d410` (cdecl `(a, b, c, _)`, the scratch in `esi`), in `CScratch::RelaxStep`'s call at `0x534638`, where
/// 1.02 calls `DamperStep1` (T, 298.15, 20, 0.54), the tyre's heat flux to the air: [`step1`] with `c` raised by a
/// convection term, `max(|car speed|, |spin| * mean radius)^0.78 * factor`, and the tread's area `2 pi (r_o^2 - r_i^2)`
/// as `d` in place of 0.54.
pub fn convection(a: f32, b: f32, c: f32, x: &Convection, temperature: f32) -> f32 {
    let inner = x.w7c + x.w8c - x.wec;
    let area = (x.w7c * x.w7c - inner * inner) * TWO_PI;
    let mean = (inner + x.w7c) * HALF;
    let rolling = x.spin.abs() * mean;
    let car = x.car_speed.abs();
    // maxss: the second operand when they are unordered
    let speed = if car > rolling { car } else { rolling };
    let mut factor = match x.kind {
        0 | 1 => x.factors[0],
        2 | 3 => x.factors[1],
        _ => CONVECTION_DEFAULT,
    };
    if let Some(boost) = x.boost {
        factor *= boost + 1.0;
    }
    let term = f64::from(speed).powf(SPEED_EXPONENT) as f32;
    step1(a, b, term * factor + c, area, temperature)
}

/// `0x1004d2c0`, NGP's `DamperStep1` without the air temperature: `(a - b) * (c * d)`.
pub fn step1_plain(a: f32, b: f32, c: f32, d: f32) -> f32 {
    (a - b) * (c * d)
}

/// `0x1004d550` (cdecl `(a, b, c, _)`, the scratch in `esi`), in `CScratch::RelaxStep`'s call at `0x534612` (1.02:
/// `DamperStep1(T, T_carcass, 30, 0.54)`): [`step1_plain`] with the tyre's inner area as `d`, `2 pi (r^2 - r_i^2) +
/// 2 pi r wac` for `r = w7c - 2 w9c` and `r_i = w7c + w8c - wec` (the words at `*(scratch+0x10)`).
pub fn inner_flux(a: f32, b: f32, c: f32, w: [f32; 5]) -> f32 {
    let [w7c, w8c, w9c, wac, wec] = w;
    let r = w7c - (w9c + w9c);
    let inner = w7c + w8c - wec;
    let rim = r * TWO_PI * wac;
    let area = (r * r - inner * inner) * TWO_PI + rim;
    step1_plain(a, b, c, area)
}

/// What the exe's tyre thermal steps (`CScratch::HeatStep`, `TableStep`, `RelaxStep`) read under NGP.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Thermal {
    /// The air temperature (`10171adc`).
    pub temperature: f32,
    /// `RelaxStep`'s rate (`10171478`, 1.02's 100): the car's `+0xe78`, else 800 (`0x10045230`).
    pub relax_rate: f32,
    /// The tyre's `+0x9c` and `+0xac` (the others are in `convection`).
    pub w9c: f32,
    pub wac: f32,
    pub convection: Convection,
}

impl Thermal {
    /// The tyre's words `+0x7c`, `+0x8c`, `+0x9c`, `+0xac`, `+0xec`.
    pub fn shape(&self) -> [f32; 5] {
        let c = &self.convection;
        [c.w7c, c.w8c, self.w9c, self.wac, c.wec]
    }

    /// The scratch's factor ([`scratch_factor`]).
    pub fn factor(&self) -> f32 {
        scratch_factor(self.convection.w7c, self.convection.w8c, self.wac)
    }
}
