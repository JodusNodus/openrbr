//! Relighting a stage's baked vertex light for another sun (explore/time-of-day, ours). The opaque unlit draws'
//! vertex light holds the bake's sun: their luminance fits `a + b * facing` of the variant's `SunDir`
//! (`examples/baked_light.rs`). The shader takes `b * facing(bake sun)` out and puts `b * facing(drawn sun)` in.
//! Foliage (alpha-tested draws) does not follow the sun this way: its vertices take the normal of the ground they stand
//! on ([`Ground`]), so that grass and bushes shift with the ground around them.
//!
//! The bake's sun is found from the vertex light, not taken from `SunDir`: track-69's M, E and N carry the same
//! vertex light, baked with E's sun, under three different `SunDir`s.
//!
//! The shadow light maps hold the bake's cast shadows, which no relight moves: with the sun's shadows on
//! (`crate::sun`) they are not used.

use crate::atmosphere::Atmosphere;
use crate::stage::{Anchor, Layer, Mesh, Stage, Vertex};
use glam::Vec3;

/// The fit's correlation from which its sun is the bake's: below it (Loch Ard N, 51N, 63N, the unbaked RSF stages)
/// the section's `SunDir` stays (`relight_survey`).
pub const FIT_HOLDS: f32 = 0.35;

/// The stage's own sun, as `Atmosphere::sun_direction`: the bake's where the fit holds, so the drawn sun and its
/// shadows match the baked light (40 retail variants' `SunDir` lie over 20 degrees off it, 69M/N and the British M
/// clones over 100), else the section's `sun_direction`.
pub fn own_sun(bake: &Relight, sun_direction: Vec3) -> Vec3 {
    if bake.r >= FIT_HOLDS {
        -bake.bake_sun
    } else {
        sun_direction
    }
}

/// The bake's sun and how much of the vertex light it gives.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Relight {
    /// Toward the sun the stage was baked with: the direction whose facing best explains the vertex light.
    pub bake_sun: Vec3,
    /// `b`: vertex luminance per unit of facing the bake's sun.
    pub strength: f32,
    /// The fit's correlation, for the report.
    pub r: f32,
    /// The vertex luminance of open ground beside the bake's sun: [`OPEN_SHARE`] of the vertices get less once the
    /// sun's `strength * facing` is out (the light taken as unoccluded at night, `night_albedo` in `night.wgsl`).
    pub open: f32,
    /// The vertex luminance of sunlit open ground, the bake's sun in it: [`OPEN_SHARE`] of the faces turned up get
    /// less (1 on the unbaked stages' white). The stage's own level, which its textures were made for: our light
    /// (`own_light` in `shader.wgsl`) splits it between the sky and the sun. Needs no fit.
    pub level: f32,
    /// The drawn sun's colour times its strength, and the share of the bake's ambient that stays ([`Daylight`]; 1 by
    /// day), in the colour `tint`; how much the car's lamps light the stage (0 by day, 1 in the dark).
    pub sun: Vec3,
    pub ambient: f32,
    pub tint: Vec3,
    pub lamps: f32,
}

/// Fits the bake's sun and `b` over the stage's opaque unlit world draws (the ones the shader relights), with face
/// normals as `sun::with_normals` makes them: the best correlation over directions 5 degrees apart above the horizon.
pub fn fit(stage: &Stage) -> Relight {
    let (mut faces, mut ys) = (Vec::new(), Vec::new());
    for mesh in stage.meshes.iter().filter(|m| m.layer == Layer::Solid && m.anchor == Anchor::World) {
        let mut normals = vec![Vec3::ZERO; mesh.vertices.len()];
        let mut used = vec![false; mesh.vertices.len()];
        for draw in mesh.draws.iter().filter(|d| d.state.shade.is_none() && d.state.alpha_ref.is_none()) {
            for t in mesh.indices[draw.indices.clone()].chunks_exact(3) {
                let p = |i: u32| Vec3::from(mesh.vertices[i as usize].position);
                let n = (p(t[1]) - p(t[0])).cross(p(t[2]) - p(t[0]));
                for &i in t {
                    normals[i as usize] += n;
                    used[i as usize] = true;
                }
            }
        }
        for ((v, n), _) in mesh.vertices.iter().zip(normals).zip(used).filter(|(_, u)| *u) {
            let Some(n) = n.try_normalize() else { continue };
            // D3DCOLOR order: b, g, r, a.
            let [b, g, r, _] = v.color.map(|c| c as f32 / 255.0);
            faces.push(n);
            ys.push(0.2126 * r + 0.7152 * g + 0.0722 * b);
        }
    }
    // Every vertex of the big stages (Semetin's 3.4 million) took seconds over the directions: an even share does.
    let step = faces.len().div_ceil(FIT_SAMPLES).max(1);
    let (faces, ys): (Vec<Vec3>, Vec<f32>) = faces.into_iter().zip(ys).step_by(step).unzip();
    let at = |az: f32, el: f32| {
        let sun = toward(az, el);
        let xs: Vec<f32> = faces.iter().map(|n| n.dot(sun).max(0.0)).collect();
        let (strength, r) = slope(&xs, &ys);
        (az, el, strength, r)
    };
    let best = |found: Option<(f32, f32, f32, f32)>, next: (f32, f32, f32, f32)| match found {
        Some(f) if f.3 >= next.3 => Some(f),
        _ => Some(next),
    };
    // 15 degrees apart over the sky, then 5 about the best (the 5-degree grid's answer on all but 2 retail variants).
    let coarse = (1..6).flat_map(|e| (0..24).map(move |a| (a as f32 * 15.0, e as f32 * 15.0)));
    let coarse = coarse.map(|(az, el)| at(az, el)).fold(None, best);
    let fine = coarse.into_iter().flat_map(|(az, el, ..)| {
        (-2..=2).flat_map(move |de| (-2..=2).map(move |da| (az + da as f32 * 5.0, el + de as f32 * 5.0)))
    });
    let fine = fine.filter(|&(_, el)| (5.0..=85.0).contains(&el)).map(|(az, el)| at(az, el)).fold(coarse, best);
    let level = level(&faces, &ys);
    fine.map(|(az, el, strength, r)| Relight {
        bake_sun: toward(az, el),
        strength: strength.max(0.0),
        r,
        open: 1.0,
        level,
        sun: Vec3::ONE,
        ambient: 1.0,
        tint: Vec3::ONE,
        lamps: 0.0,
    })
    .map(|bake| Relight { open: open(&faces, &ys, &bake), ..bake })
    .unwrap_or(Relight {
        bake_sun: Vec3::Y,
        strength: 0.0,
        r: 0.0,
        open: 1.0,
        level,
        sun: Vec3::ONE,
        ambient: 1.0,
        tint: Vec3::ONE,
        lamps: 0.0,
    })
}

/// At most this many vertices take part in [`fit`].
const FIT_SAMPLES: usize = 100_000;

/// The share of the vertices whose light, less the bake's sun, is below [`Relight::open`].
const OPEN_SHARE: f32 = 0.9;

/// [`Relight::level`] of the vertex luminances `ys` of faces turned `faces`: over those turned up (within about 45
/// degrees), 1 without any.
fn level(faces: &[Vec3], ys: &[f32]) -> f32 {
    let mut up: Vec<f32> = faces.iter().zip(ys).filter(|(n, _)| n.y > 0.7).map(|(_, &y)| y).collect();
    if up.is_empty() {
        return 1.0;
    }
    let index = ((up.len() - 1) as f32 * OPEN_SHARE) as usize;
    up.select_nth_unstable_by(index, f32::total_cmp).1.max(0.05)
}

/// [`Relight::open`] of the vertex luminances `ys` of faces turned `faces` under `bake`'s sun.
fn open(faces: &[Vec3], ys: &[f32], bake: &Relight) -> f32 {
    let mut rest: Vec<f32> =
        faces.iter().zip(ys).map(|(n, y)| y - bake.strength * n.dot(bake.bake_sun).max(0.0)).collect();
    if rest.is_empty() {
        return 1.0;
    }
    let index = ((rest.len() - 1) as f32 * OPEN_SHARE) as usize;
    rest.select_nth_unstable_by(index, f32::total_cmp).1.max(0.05)
}

/// The ground's normals on a grid over x and z: the opaque unlit world faces turned up, area-weighted per cell and
/// blurred over the cells around.
pub struct Ground {
    low: [f32; 2],
    size: [usize; 2],
    normals: Vec<Vec3>,
}

/// A ground cell's side, metres.
const CELL: f32 = 4.0;

impl Ground {
    pub fn of(stage: &Stage) -> Ground {
        let faces: Vec<(Vec3, Vec3)> = stage
            .meshes
            .iter()
            .filter(|m| m.layer == Layer::Solid && m.anchor == Anchor::World)
            .flat_map(|mesh| {
                mesh.draws
                    .iter()
                    .filter(|d| d.state.shade.is_none() && d.state.alpha_ref.is_none())
                    .flat_map(|d| mesh.indices[d.indices.clone()].chunks_exact(3))
                    .map(|t| [t[0], t[1], t[2]].map(|i| Vec3::from(mesh.vertices[i as usize].position)))
                    .map(|[a, b, c]| ((a + b + c) / 3.0, (b - a).cross(c - a)))
            })
            .filter(|(_, n)| n.y > 0.3 * n.length())
            .collect();
        let (mut low, mut high) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        for (p, _) in &faces {
            (low, high) = (low.min(*p), high.max(*p));
        }
        if faces.is_empty() {
            return Ground { low: [0.0; 2], size: [1, 1], normals: vec![Vec3::Y] };
        }
        let size = [((high.x - low.x) / CELL) as usize + 1, ((high.z - low.z) / CELL) as usize + 1];
        let mut sums = vec![Vec3::ZERO; size[0] * size[1]];
        for (p, n) in &faces {
            let (x, z) = (((p.x - low.x) / CELL) as usize, ((p.z - low.z) / CELL) as usize);
            sums[z * size[0] + x] += *n;
        }
        let mut normals = vec![Vec3::Y; sums.len()];
        for z in 0..size[1] {
            for x in 0..size[0] {
                let mut sum = Vec3::ZERO;
                for nz in z.saturating_sub(1)..(z + 2).min(size[1]) {
                    for nx in x.saturating_sub(1)..(x + 2).min(size[0]) {
                        sum += sums[nz * size[0] + nx];
                    }
                }
                normals[z * size[0] + x] = sum.try_normalize().unwrap_or(Vec3::Y);
            }
        }
        Ground { low: [low.x, low.z], size, normals }
    }

    /// The ground's normal under `p` (up outside the ground).
    pub fn normal_at(&self, p: Vec3) -> Vec3 {
        let (x, z) = ((p.x - self.low[0]) / CELL, (p.z - self.low[1]) / CELL);
        if x < 0.0 || z < 0.0 || x as usize >= self.size[0] || z as usize >= self.size[1] {
            return Vec3::Y;
        }
        self.normals[z as usize * self.size[0] + x as usize]
    }

    /// `vertices` (of `mesh`) with the alpha-tested draws' normals replaced by the ground's under them.
    pub fn on_foliage(&self, mut vertices: Vec<Vertex>, mesh: &Mesh) -> Vec<Vertex> {
        for draw in mesh.draws.iter().filter(|d| d.state.shade.is_none() && d.state.alpha_ref.is_some()) {
            for &i in &mesh.indices[draw.indices.clone()] {
                let v = &mut vertices[i as usize];
                v.normal = self.normal_at(Vec3::from(v.position)).to_array();
            }
        }
        vertices
    }
}

/// A rally's latitude (degrees north) and day of the year, for the sun's path (ours: the real rallies' regions and
/// seasons, not from the game), by the map's number (`Maps/Tracks.ini` `Particles`).
pub fn rally_of(map: u32) -> (f32, f32) {
    match map {
        10..=19 | 93..=98 => (61.0, 40.0),            // arctic, February
        20..=29 | 71..=79 | 82..=89 => (52.0, 320.0), // british, November
        31..=39 => (-32.0, 260.0),                    // Australia, September
        41..=49 | 80 | 81 => (46.0, 280.0),           // Mont Blanc, October
        51..=59 => (43.0, 260.0),                     // Japan (Hokkaido), September
        61..=69 => (45.0, 300.0),                     // USA, late October
        _ => (50.0, 170.0),
    }
}

/// The sun's elevation and compass azimuth (degrees, clockwise from north) at local solar `hour`.
pub fn solar(latitude: f32, day: f32, hour: f32) -> (f32, f32) {
    let declination = (-23.44f32).to_radians() * (std::f32::consts::TAU / 365.0 * (day + 10.0)).cos();
    let (phi, h) = (latitude.to_radians(), (15.0 * (hour - 12.0)).to_radians());
    let elevation = (phi.sin() * declination.sin() + phi.cos() * declination.cos() * h.cos()).asin();
    let azimuth = (-h.sin()).atan2(declination.tan() * phi.cos() - phi.sin() * h.cos());
    (elevation.to_degrees(), azimuth.to_degrees().rem_euclid(360.0))
}

/// The light at a clock time ([`Clock::daylight`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Daylight {
    /// Toward the sun by day (at least 2 degrees up, for the sky and the shadows; `elevation` is the real one), toward
    /// the moon at night (opposite the sun, 10 to 60 degrees up).
    pub toward: Vec3,
    pub elevation: f32,
    /// The sun's colour times its strength: white high up, warm and fading toward the horizon, none below.
    pub sun: Vec3,
    /// The share of the day's ambient light (all down to a sun 4 degrees up, about a sixth at [`NIGHT`]; kept
    /// game-bright rather than measured), and its colour (bluer as the sun goes: the light then comes from the sky
    /// alone).
    pub ambient: f32,
    pub tint: Vec3,
    /// How much the car's lamps light the stage: none by day, all at [`NIGHT`].
    pub lamps: f32,
    /// The sun below [`NIGHT`]: drawn as night (`Atmosphere::night`).
    pub night: bool,
}

/// The sun's elevation, degrees, below which the stage is drawn as night: a little past the end of civil twilight,
/// where the day's dimmed light meets the moonlight.
pub const NIGHT: f32 = -7.0;

/// A stage's clock: its rally's sun path (`rally_of`, [`solar`]) anchored to the bake, and the atmosphere it lights.
/// Stage north is not known, so the bake's hour is where the path reaches the bake's elevation, in the morning for the
/// `M` variants, the afternoon for `E` and nearest noon otherwise (the letter is the section's: `54O_E` is `E`), and
/// the sun turns from the bake's direction as the compass azimuth turns from that hour. Started without a time, the
/// clock starts at the bake's hour with the sun lifted to the bake's own elevation (`lift`), so that it draws the
/// bake's sun where the rally's path never climbs as high (the British bakes). Ours throughout.
#[derive(Debug, Clone)]
pub struct Clock {
    latitude: f32,
    day: f32,
    /// The hour the bake's sun stands at, and the bake sun's azimuth (stage) and compass azimuth then.
    pub bake_hour: f32,
    bake_azimuth: f32,
    bake_compass: f32,
    /// The fitted bake ([`fit`]), relit unless `relight` is off (`viewer --no-relight`).
    bake: Relight,
    relight: bool,
    /// The stage's own atmosphere, which each time of day starts from.
    base: Atmosphere,
    /// The local solar hour at the start, and clock seconds per drive second.
    pub start: f32,
    pub rate: f32,
    /// Degrees added to the path's elevation: the bake's over the path's at the bake's hour when started without a
    /// time, else none.
    lift: f32,
}

impl Clock {
    /// The clock from `start` (local solar hour; `None` for the bake's sun).
    pub fn new(
        base: Atmosphere,
        bake: Relight,
        map: u32,
        letter: char,
        start: Option<f32>,
        rate: f32,
        relight: bool,
    ) -> Clock {
        let (latitude, day) = rally_of(map);
        let bake_elevation = bake.bake_sun.y.asin().to_degrees();
        let side = |h: &f32| match letter {
            'M' => *h <= 12.0,
            'E' => *h >= 12.0,
            _ => true,
        };
        let bake_hour = (0..=240)
            .map(|k| k as f32 * 0.1)
            .filter(side)
            .min_by(|a, b| {
                let off = |h: f32| (solar(latitude, day, h).0 - bake_elevation).abs() + 0.01 * (h - 12.0).abs();
                off(*a).total_cmp(&off(*b))
            })
            .unwrap_or(12.0);
        Clock {
            latitude,
            day,
            bake_hour,
            bake_azimuth: azimuth(bake.bake_sun),
            bake_compass: solar(latitude, day, bake_hour).1,
            bake,
            relight,
            base,
            start: start.unwrap_or(bake_hour),
            rate,
            lift: if start.is_none() { bake_elevation - solar(latitude, day, bake_hour).0 } else { 0.0 },
        }
    }

    /// The local solar hour `seconds` of drive time after the start.
    pub fn hour(&self, seconds: f64) -> f32 {
        (self.start + (seconds * f64::from(self.rate) / 3600.0) as f32).rem_euclid(24.0)
    }

    /// The light at local solar `hour`.
    pub fn daylight(&self, hour: f32) -> Daylight {
        let (elevation, compass) = solar(self.latitude, self.day, hour);
        let elevation = elevation + self.lift;
        // Compass azimuth runs clockwise seen from above; `azimuth` here from +x toward +z, the other way.
        let turned = self.bake_azimuth - (compass - self.bake_compass);
        let warm = Vec3::new(1.0, 0.55, 0.3).lerp(Vec3::ONE, smooth(0.0, 25.0, elevation));
        let night = elevation < NIGHT;
        let (ambient, tint) = ambient_at(elevation);
        Daylight {
            toward: if night {
                toward(turned + 180.0, (-elevation).clamp(10.0, 60.0))
            } else {
                toward(turned, elevation.max(2.0))
            },
            elevation,
            sun: warm * smooth(-1.0, 6.0, elevation),
            ambient,
            tint,
            lamps: 1.0 - smooth(NIGHT, 6.0, elevation),
            night,
        }
    }

    /// The atmosphere `seconds` of drive time after the start, and its light.
    pub fn atmosphere(&self, seconds: f64) -> (Atmosphere, Daylight) {
        let hour = self.hour(seconds);
        let day = self.daylight(hour);
        let mut a = self.base.clone();
        a.solar_hour = Some(hour);
        a.sun_direction = -day.toward;
        a.sun_elevation = Some(day.elevation);
        a.diffuse *= day.sun;
        a.ambient *= day.ambient * day.tint;
        a.fog_color *= day.ambient * day.sun.lerp(Vec3::ONE, 0.6).max(Vec3::splat(0.3));
        if day.sun.max_element() < 0.05 {
            a.car_shadow = 0.0;
        }
        let mut bake = Relight { sun: day.sun, ambient: day.ambient, tint: day.tint, lamps: day.lamps, ..self.bake };
        if day.night {
            // At night (`night.wgsl`) the stage's colour is the albedo the moon and the lamps light.
            a = a.night();
            (bake.ambient, bake.tint, bake.lamps) = (1.0, Vec3::ONE, 0.0);
        }
        a.relight = self.relight.then_some(bake);
        (a, day)
    }
}

/// [`Daylight::ambient`] and [`Daylight::tint`] for a sun `elevation` degrees up.
pub fn ambient_at(elevation: f32) -> (f32, Vec3) {
    let ambient = 0.15 + 0.85 * smooth(NIGHT, 4.0, elevation);
    (ambient, Vec3::new(0.72, 0.84, 1.12).lerp(Vec3::ONE, smooth(-4.0, 15.0, elevation)))
}

fn smooth(low: f32, high: f32, x: f32) -> f32 {
    let t = ((x - low) / (high - low)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Toward a sun at azimuth `az` (degrees from +x toward +z) and elevation `el` (degrees above the horizon).
fn toward(az: f32, el: f32) -> Vec3 {
    let (az, el) = (az.to_radians(), el.to_radians());
    Vec3::new(el.cos() * az.cos(), el.sin(), el.cos() * az.sin())
}

/// The azimuth of `toward`'s direction, degrees.
fn azimuth(dir: Vec3) -> f32 {
    dir.z.atan2(dir.x).to_degrees()
}

/// Least squares slope of `y` on `x`, and the correlation.
fn slope(xs: &[f32], ys: &[f32]) -> (f32, f32) {
    let n = xs.len().max(1) as f64;
    let mx = xs.iter().map(|&x| x as f64).sum::<f64>() / n;
    let my = ys.iter().map(|&y| y as f64).sum::<f64>() / n;
    let (mut sxx, mut syy, mut sxy) = (0.0, 0.0, 0.0);
    for (&x, &y) in xs.iter().zip(ys) {
        let (dx, dy) = (x as f64 - mx, y as f64 - my);
        sxx += dx * dx;
        syy += dy * dy;
        sxy += dx * dy;
    }
    if sxx == 0.0 || syy == 0.0 {
        return (0.0, 0.0);
    }
    ((sxy / sxx) as f32, (sxy / (sxx * syy).sqrt()) as f32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_ground_is_the_light_most_vertices_keep_without_the_sun() {
        let bake = Relight {
            bake_sun: Vec3::Y,
            strength: 0.4,
            r: 1.0,
            open: 1.0,
            level: 1.0,
            sun: Vec3::ONE,
            ambient: 1.0,
            tint: Vec3::ONE,
            lamps: 0.0,
        };
        // Ground (facing 1) and walls (facing 0): 0.3 ambient and the sun's 0.4 on the ground; one vertex in deep shade.
        let faces = [Vec3::Y, Vec3::Y, Vec3::X, Vec3::Z, Vec3::Y];
        let ys = [0.7, 0.7, 0.3, 0.3, 0.1];
        assert!((open(&faces, &ys, &bake) - 0.3).abs() < 1e-6);
        assert_eq!(open(&[], &[], &bake), 1.0);
    }

    #[test]
    fn the_sun_is_overhead_at_noon_on_the_equator_at_the_equinox() {
        let (elevation, _) = solar(0.0, 80.0, 12.0);
        assert!(elevation > 88.0, "{elevation}");
    }

    #[test]
    fn the_northern_sun_rises_in_the_east_and_sets_in_the_west() {
        let (morning, east) = solar(50.0, 172.0, 7.0);
        let (evening, west) = solar(50.0, 172.0, 17.0);
        assert!(morning > 0.0 && (morning - evening).abs() < 0.5);
        assert!((60.0..120.0).contains(&east) && (240.0..300.0).contains(&west), "{east} {west}");
    }
}
