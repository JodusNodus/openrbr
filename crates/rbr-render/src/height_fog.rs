//! Height fog under the physically based sky (`Enhance::sky`, ours; `height_fogged` in `shader.wgsl`): a mist whose
//! density falls off exponentially with the height above the stage's low ground, integrated along the view ray, lit
//! by the sky's haze colour and the sun's in-scatter. The game has no height fog: its `TrackSettings.ini` sections give
//! only the linear fog's range and colour, so the mist's visibility follows the section's `FogEnd` under fog, and
//! otherwise our constants per weather word and time of day (dawn and dusk mist).

use crate::atmosphere::Atmosphere;
use crate::stage::{Anchor, Layer, Mesh};
use glam::{Vec2, Vec3};

/// The low ground's grid: each cell holds the lowest world-anchored solid vertex in it, metres a side.
const CELL: f32 = 32.0;
/// The mist's base is the mean low ground within this distance of the eye, metres (weighted to fade out at it, so it
/// changes smoothly as the eye moves), less [`BELOW`]: the valleys' floors below the mean lie in the mist, the ridges
/// above it out of it.
const AROUND: f32 = 400.0;
const BELOW: f32 = 15.0;

/// How far one sees through the mist at its base (cover 95%: optical depth 3), metres, and how fast it thins with
/// height (the scale height), metres, for each weather word of the section's name (ours).
const HEAVY_FOG: (f32, f32) = (0.5, 70.0);
const LIGHT_FOG: (f32, f32) = (1.0, 55.0);
/// Rain and snow under fog: a lighter ground fog, by the same share of `FogEnd`.
const PRECIPITATION: (f32, f32) = (2.0, 45.0);
/// Clear sections: dawn and dusk mist hugging the valleys, and a faint ground haze by day.
const MIST: (f32, f32) = (900.0, 22.0);
const DAY_HAZE: (f32, f32) = (9000.0, 60.0);
/// `hazy` sections see the day's haze this much nearer.
const HAZY: f32 = 0.5;
/// The sun's true elevation, degrees, at which the mist is thickest (from `MIST_LOW` up), and between which it burns
/// off in the morning and forms in the evening (or without a clock).
const MIST_LOW: f32 = -6.0;
const MORNING: [f32; 2] = [10.0, 30.0];
const EVENING: [f32; 2] = [3.0, 14.0];

/// The mist for the shader (`Frame::height_fog`) at the eye: `x` its density at the base (per metre; 0: none), `y` its
/// scale height, metres, `z` the base's height in stage space.
pub(crate) fn uniform(atmosphere: &Atmosphere, ground: Option<&Ground>, eye: Vec3) -> [f32; 4] {
    let Some(base) = ground.filter(|_| !atmosphere.night).and_then(|g| g.base(eye)) else { return [0.0; 4] };
    let (visibility, scale_height) = mist(atmosphere);
    [3.0 / visibility, scale_height, base, 0.0]
}

/// The mist's visibility at its base and its scale height, metres (the constants above).
fn mist(a: &Atmosphere) -> (f32, f32) {
    let section = a.section.as_deref().unwrap_or("");
    let fog_end = a.fog_end.max(100.0);
    if a.use_fog {
        let (share, height) = if section.contains("heavyfog") {
            HEAVY_FOG
        } else if section.contains("lightfog") {
            LIGHT_FOG
        } else {
            PRECIPITATION
        };
        return (share * fog_end, height);
    }
    let elevation = a.sun_elevation.unwrap_or_else(|| (-a.sun_direction.y).asin().to_degrees());
    let [low, high] = if a.solar_hour.is_some_and(|h| h < 12.0) { MORNING } else { EVENING };
    let dawn = smooth(MIST_LOW - 4.0, MIST_LOW, elevation) * (1.0 - smooth(low, high, elevation));
    let day = if section.contains("hazy") { DAY_HAZE.0 * HAZY } else { DAY_HAZE.0 };
    // Blended in density (the inverse of the visibility) and in scale height.
    let density = (1.0 - dawn) / day + dawn / MIST.0;
    (1.0 / density, DAY_HAZE.1 + (MIST.1 - DAY_HAZE.1) * dawn)
}

fn smooth(low: f32, high: f32, x: f32) -> f32 {
    let t = ((x - low) / (high - low)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The stage's low ground: the lowest world-anchored solid vertex in each [`CELL`] square.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Ground {
    /// The first cell's low x and z corner.
    origin: Vec2,
    columns: usize,
    /// Row by row (z), `NaN` where nothing lies.
    lowest: Vec<f32>,
}

impl Ground {
    /// `None` without world-anchored solid meshes.
    pub(crate) fn of(meshes: &[Mesh]) -> Option<Ground> {
        let points = || {
            meshes
                .iter()
                .filter(|m| m.anchor == Anchor::World && m.layer == Layer::Solid)
                .flat_map(|m| m.vertices.iter().map(|v| Vec3::from(v.position)))
                .filter(|p| p.is_finite())
        };
        let (low, high) = points().fold(None, |bounds: Option<(Vec3, Vec3)>, p| {
            Some(bounds.map_or((p, p), |(low, high)| (low.min(p), high.max(p))))
        })?;
        let origin = Vec2::new(low.x, low.z);
        let cell = |p: Vec3| ((Vec2::new(p.x, p.z) - origin) / CELL).as_uvec2();
        let last = cell(high);
        let columns = last.x as usize + 1;
        let mut lowest = vec![f32::NAN; columns * (last.y as usize + 1)];
        for p in points() {
            let c = cell(p);
            let slot = &mut lowest[c.y as usize * columns + c.x as usize];
            *slot = slot.min(p.y);
        }
        Some(Ground { origin, columns, lowest })
    }

    /// The mist's base about `eye` (`None`: no ground within [`AROUND`]).
    pub(crate) fn base(&self, eye: Vec3) -> Option<f32> {
        let rows = self.lowest.len() / self.columns;
        let at = (Vec2::new(eye.x, eye.z) - self.origin) / CELL;
        let reach = AROUND / CELL;
        let span = |centre: f32, count: usize| {
            let low = (centre - reach).floor().max(0.0) as usize;
            let high = ((centre + reach).ceil().max(0.0) as usize).min(count);
            low..high
        };
        let (mut sum, mut weight) = (0.0, 0.0);
        for row in span(at.y, rows) {
            for column in span(at.x, self.columns) {
                let lowest = self.lowest[row * self.columns + column];
                let d = (Vec2::new(column as f32, row as f32) + 0.5 - at).length() / reach;
                if lowest.is_nan() || d >= 1.0 {
                    continue;
                }
                let w = (1.0 - d * d).powi(2);
                sum += w * lowest;
                weight += w;
            }
        }
        (weight > 0.0).then(|| sum / weight - BELOW)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;

    fn clear(elevation: f32) -> Atmosphere {
        Atmosphere { section: Some("54E_crisp_clear".into()), sun_elevation: Some(elevation), ..Default::default() }
    }

    #[test]
    fn mist_lies_at_dawn_and_burns_off_by_day() {
        let ground = Ground { origin: Vec2::ZERO, columns: 1, lowest: vec![110.0] };
        let at = |a: &Atmosphere| uniform(a, Some(&ground), Vec3::new(16.0, 0.0, 16.0));
        let [dawn, noon] = [clear(1.0), clear(40.0)].map(|a| at(&a));
        assert!((dawn[0] - 3.0 / MIST.0).abs() < 1e-6 && dawn[1] == MIST.1 && dawn[2] == 110.0 - BELOW);
        assert!((noon[0] - 3.0 / DAY_HAZE.0).abs() < 1e-6 && noon[1] == DAY_HAZE.1);
        let midway = at(&clear(8.0));
        assert!(midway[0] < dawn[0] && midway[0] > noon[0]);
        // The morning's mist lasts longer.
        let morning = at(&Atmosphere { solar_hour: Some(7.0), ..clear(8.0) });
        assert!(morning[0] > midway[0] && morning[0] <= dawn[0]);
        // None at night, nor without ground.
        assert_eq!(at(&Atmosphere { night: true, ..clear(1.0) }), [0.0; 4]);
        assert_eq!(uniform(&clear(1.0), None, Vec3::ZERO), [0.0; 4]);
        // Nor out of the ground's reach.
        assert_eq!(uniform(&clear(1.0), Some(&ground), Vec3::new(1000.0, 0.0, 0.0)), [0.0; 4]);
    }

    #[test]
    fn fog_sections_take_their_fog_end() {
        let fog = |name: &str| Atmosphere {
            section: Some(name.into()),
            use_fog: true,
            fog_end: 600.0,
            sun_direction: Vec3::new(0.0, -0.5, 0.866),
            ..Default::default()
        };
        let ground = Ground { origin: Vec2::ZERO, columns: 1, lowest: vec![0.0] };
        let at = |name: &str| uniform(&fog(name), Some(&ground), Vec3::ZERO);
        let heavy = at("21O_M_heavyfog_heavycloud");
        let light = at("21O_M_lightfog_heavycloud");
        let rain = at("21O_M_lightrain_heavycloud");
        assert!((heavy[0] - 3.0 / 300.0).abs() < 1e-6 && heavy[1] == HEAVY_FOG.1);
        assert!(heavy[0] > light[0] && light[0] > rain[0]);
    }

    #[test]
    fn the_base_is_the_mean_low_ground_about_the_eye() {
        let mesh = |anchor, layer, points: &[[f32; 3]]| Mesh {
            layer,
            anchor,
            bounds: None,
            cell: None,
            vertices: points
                .iter()
                .map(|&position| crate::stage::Vertex { position, ..bytemuck::Zeroable::zeroed() })
                .collect(),
            indices: Vec::new(),
            draws: Vec::new(),
        };
        // A valley floor at 0 m and a ridge at 100 m (its tree tops at 120 m) 64 m east; a sky face and camera-anchored
        // clouds far below, which do not count.
        let meshes = [
            mesh(Anchor::World, Layer::Solid, &[[0.0, 0.0, 0.0], [64.0, 100.0, 0.0], [64.0, 120.0, 0.0]]),
            mesh(Anchor::World, Layer::Sky, &[[0.0, -500.0, 0.0]]),
            mesh(Anchor::Camera, Layer::Solid, &[[64.0, -500.0, 0.0]]),
        ];
        let ground = Ground::of(&meshes).unwrap();
        assert_eq!(ground.columns, 3);
        // Seen from the middle the two weigh about the same; nearer the valley, the valley more.
        let middle = ground.base(Vec3::new(32.0, 50.0, 16.0)).unwrap();
        let valley = ground.base(Vec3::new(0.0, 50.0, 16.0)).unwrap();
        assert!((middle - (50.0 - BELOW)).abs() < 5.0, "{middle}");
        assert!(valley < middle);
        assert_eq!(ground.base(Vec3::new(5000.0, 0.0, 0.0)), None);
        assert_eq!(Ground::of(&[]), None);
    }
}
