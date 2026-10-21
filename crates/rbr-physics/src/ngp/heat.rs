//! NGP's heat transfer: Newton's law of cooling against the actual air temperature, the tyre's convection to the air
//! (still air plus a forced term that grows with speed^0.78), its conduction through the carcass to the air inside,
//! and conduction through the tread. 1.02's model assumes air at 20 °C; NGP shifts its ambient temperatures by the
//! difference. Where NGP's functions and data live: `notes/ngp.md`, "Replacements".

use std::f32::consts::TAU;

/// The air temperature 1.02's ambient temperatures assume, K (293.14).
const REFERENCE_AIR: f32 = 293.14;
/// The exponent of the speed in the forced convection coefficient, as the f32 0.78 widened to double.
const SPEED_EXPONENT: f64 = 0.78_f32 as f64;
/// The forced convection factor of a tyre kind without a per-car factor.
const DEFAULT_FORCED_FACTOR: f32 = 6.0;
/// The tread's segments: each takes an equal share of the tread surface.
const TREAD_SEGMENTS: f32 = 8.0;

/// Heat flow from a body at `hot` to one at `cold`, K, through `area` with heat transfer coefficient `h`.
pub fn heat_flow(hot: f32, cold: f32, h: f32, area: f32) -> f32 {
    (hot - cold) * (h * area)
}

/// [`heat_flow`] to surroundings at `ambient`, a 1.02 temperature that assumes [`REFERENCE_AIR`], moved with the air
/// at `air_temperature`.
pub fn heat_flow_to_air(hot: f32, ambient: f32, h: f32, area: f32, air_temperature: f32) -> f32 {
    heat_flow(hot, ambient - REFERENCE_AIR + air_temperature, h, area)
}

/// Heat conducted through `area` with conductivity `k` along the temperature `gradient`, at double precision.
pub fn conduction(gradient: f32, k: f32, area: f32) -> f64 {
    f64::from(-gradient) * f64::from(k) * f64::from(area)
}

/// The tyre's dimensions that set its heat-exchanging areas, m.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TyreShape {
    pub carcass_radius: f32,
    pub tread_depth: f32,
    pub carcass_thickness: f32,
    pub tread_width: f32,
    /// From the tread's surface down to the rim.
    pub sidewall_height: f32,
}

impl TyreShape {
    pub fn rim_radius(&self) -> f32 {
        self.carcass_radius + self.tread_depth - self.sidewall_height
    }

    /// Both sidewalls' outer faces: the annuli between the rim and the carcass radius.
    pub fn sidewall_area(&self) -> f32 {
        let rim = self.rim_radius();
        (self.carcass_radius * self.carcass_radius - rim * rim) * TAU
    }

    /// Halfway up the sidewall: where its surface moves at the sidewall's mean rolling speed.
    pub fn mean_sidewall_radius(&self) -> f32 {
        (self.rim_radius() + self.carcass_radius) * 0.5
    }

    /// The carcass's inner surface, which faces the air inside the tyre: both inner sidewall faces and the band
    /// under the tread.
    pub fn inner_area(&self) -> f32 {
        let inner = self.carcass_radius - (self.carcass_thickness + self.carcass_thickness);
        let rim = self.rim_radius();
        let band = inner * TAU * self.tread_width;
        (inner * inner - rim * rim) * TAU + band
    }

    /// One tread segment's share of the tread surface.
    pub fn tread_segment_area(&self) -> f32 {
        (self.carcass_radius + self.tread_depth) * TAU * self.tread_width * (1.0 / TREAD_SEGMENTS)
    }
}

/// What sets the forced convection of a tyre's sidewalls: how fast air moves past them, and the car's factors.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Airflow {
    /// The tyre's kind: kinds 0 and 1 take the first per-car factor, 2 and 3 the second, others the default 6.
    pub kind: u32,
    /// The wheel's spin, rad/s, and the car's speed, m/s.
    pub spin: f32,
    pub car_speed: f32,
    /// An extra share of the factor, when the tyre has one.
    pub boost: Option<f32>,
    /// The car's forced convection factors for kinds 0-1 and 2-3.
    pub factors: [f32; 2],
}

impl Airflow {
    /// The forced convection coefficient: the faster of the car and the sidewall's rolling surface, to the 0.78,
    /// times the kind's factor.
    fn coefficient(&self, shape: &TyreShape) -> f32 {
        let rolling = self.spin.abs() * shape.mean_sidewall_radius();
        let car = self.car_speed.abs();
        // maxss: the second operand when they are unordered
        let speed = if car > rolling { car } else { rolling };
        let mut factor = match self.kind {
            0 | 1 => self.factors[0],
            2 | 3 => self.factors[1],
            _ => DEFAULT_FORCED_FACTOR,
        };
        if let Some(boost) = self.boost {
            factor *= boost + 1.0;
        }
        let term = f64::from(speed).powf(SPEED_EXPONENT) as f32;
        term * factor
    }
}

/// Heat flow from the sidewalls at `hot` to the air (1.02's `ambient`, moved with `air_temperature`), with the still
/// air coefficient `still_h` raised by the forced convection of `airflow`.
pub fn sidewall_convection(
    hot: f32,
    ambient: f32,
    still_h: f32,
    shape: &TyreShape,
    airflow: &Airflow,
    air_temperature: f32,
) -> f32 {
    let area = shape.sidewall_area();
    let h = airflow.coefficient(shape) + still_h;
    heat_flow_to_air(hot, ambient, h, area, air_temperature)
}

/// Heat flow from the carcass at `carcass` to the air inside the tyre at `inner_air`, through the inner surface.
pub fn carcass_to_inner_air(carcass: f32, inner_air: f32, h: f32, shape: &TyreShape) -> f32 {
    heat_flow(carcass, inner_air, h, shape.inner_area())
}

/// What the tyre's thermal model needs under NGP.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TyreThermal {
    pub air_temperature: f32,
    /// The scale of the carcass's temperature difference in its relaxation (1.02: 100).
    pub relax_rate: f32,
    pub shape: TyreShape,
    pub airflow: Airflow,
}
