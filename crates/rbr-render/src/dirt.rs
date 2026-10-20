//! Car dirt from the tyre spray (rbr-recomp `notes/car-damage-visuals.md`, section 7).
//!
//! Each `CSigTireParticles` raises the car's dirt level by its spray amount (`m_unk88`, the payload's `mass`) times
//! `1e-5`, capped at 0.9 (`CCarShaderManager::OnTireParticles`, HH 0x10001420; `CHH_TextureEffectDirt` 0x10043a70 adds
//! the same). The level starts at 0 and never falls during a drive. `CarPaint` lerps its diffuse to the dirt colour by
//! the dirt map's alpha times the level (`shader.wgsl`). The signal is one per wheel per step, so the level after a
//! frame is the capped sum of every payload's mass up to it ([`Spray::level`]).
//!
//! The game's `CHH_TextureEffectDirt` and `CHH_TextureEffectTireDirt` (HH 0x10043a70, 0x10046820), which would redraw
//! `External` and `ExternalDynamic` towards the dirt colour by their own copy of the level, never run in 1.02:
//! `CTextureManager::LoadEffects` (HH 0x100499b0, from `CHH_World::Unk20` 0x10066610) makes them only from an
//! `[Effects]` section of the car's `_shaders.ini`, and no retail car has one (nor the `External` or `ExternalDynamic`
//! friendly names they draw into). The level is one per car: every wheel's payloads on every surface add to it, by the
//! payload's mass alone (rbr-recomp `notes/car-damage-visuals.md` section 7).
//!
//! Ours: the dirt colour (`shader.wgsl`), the mud on the windscreen ([`crate::glass`]), which takes the spray of each
//! step by what its surface throws ([`Spray::thrown`], [`thrown`]), and a mud film on the other exterior panes by the
//! level ([`grime`]).

use std::sync::LazyLock;

use rbr_formats::lsp_keys::{Class, KEYS};

/// The level per unit of spray amount (`OnTireParticles`).
pub const PER_MASS: f64 = 1e-5;
/// The most the level reaches.
pub const MOST: f32 = 0.9;

/// The exterior panes that take a mud film by the dirt level (ours: 1.02's `CarGlass` binds no dirt map), with their
/// share of it in tenths: the rear window most, the side windows less, the rear ones more than the front. The
/// windscreen is left to [`crate::glass`], whose wipers clear it.
const GRIME: [(&str, u8); 5] =
    [("e_window_b", 10), ("e_window_lb", 5), ("e_window_rb", 5), ("e_window_lf", 3), ("e_window_rf", 3)];

/// The mud film share of a car face of `part` drawn with `shader` (`stage::Shade::grime`): 0 for none.
pub fn grime(part: &str, shader: &str) -> u8 {
    if shader != "CarGlass" {
        return 0;
    }
    GRIME.iter().find(|(name, _)| *name == part).map_or(0, |&(_, share)| share)
}

/// What the spray of a surface throws at the windscreen, per unit of payload mass (ours): mud, snow and water.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Thrown {
    pub mud: f32,
    pub snow: f32,
    pub water: f32,
}

impl Thrown {
    pub(crate) fn scaled(self, k: f32) -> Thrown {
        Thrown { mud: self.mud * k, snow: self.snow * k, water: self.water * k }
    }

    fn plus(self, o: Thrown) -> Thrown {
        Thrown { mud: self.mud + o.mud, snow: self.snow + o.snow, water: self.water + o.water }
    }

    fn wide(self) -> [f64; 3] {
        [self.mud, self.snow, self.water].map(f64::from)
    }

    fn narrow([mud, snow, water]: [f64; 3]) -> Thrown {
        Thrown { mud: mud as f32, snow: snow as f32, water: water as f32 }
    }
}

/// What a surface of `name` throws (ours), by the exe's `MaterialManager` name (`physics.lsp`'s surfaces, whose id the
/// tyre payloads carry): gravel, dirt and rough ground mud, grass half as much; tarmac nothing dry, mud by the gravel on
/// it (sprinkled a quarter, covered a half), and water damp (half) or wet; snow snow; the water surface water. Trees,
/// bushes, rocks, walls and barriers throw nothing.
fn thrown_by_name(name: &str) -> Thrown {
    let name = name.trim_start_matches("US2_").trim_start_matches("BR2_");
    let wet = if name.contains("_Wet") {
        1.0
    } else if name.contains("_Damp") {
        0.5
    } else {
        0.0
    };
    let mud = |mud| Thrown { mud, ..Thrown::default() };
    if name.starts_with("Tarmac_") {
        let gravel = if name.contains("_Covered") {
            0.5
        } else if name.contains("_Sprinkled") {
            0.25
        } else {
            0.0
        };
        Thrown { mud: gravel, water: wet, ..Thrown::default() }
    } else if ["Gravel_", "Dirt_", "Rough_"].iter().any(|p| name.starts_with(p)) {
        mud(1.0)
    } else if name.starts_with("Grass_") {
        mud(0.5)
    } else if name.starts_with("SnowOn") {
        Thrown { snow: 1.0, ..Thrown::default() }
    } else if name == "Water_Surface" {
        Thrown { water: 1.0, ..Thrown::default() }
    } else {
        Thrown::default()
    }
}

/// [`thrown_by_name`] of each surface id in the exe's `MaterialManager` table; nothing for an id it lacks.
static THROWN: LazyLock<[Thrown; 256]> = LazyLock::new(|| {
    let mut by_id = [Thrown::default(); 256];
    for key in KEYS.iter().filter(|k| k.class == Class::MaterialManager) {
        if let Some(t) = by_id.get_mut(key.offset as usize) {
            *t = thrown_by_name(key.key);
        }
    }
    by_id
});

/// What the spray of surface id `surface` throws at the windscreen per unit of mass ([`thrown_by_name`]).
pub fn thrown(surface: u32) -> Thrown {
    THROWN.get(surface as usize).copied().unwrap_or_default()
}

/// A frame's tyre payloads, as `(surface id, mass)`: their summed mass, and what they throw at the windscreen.
pub fn frame_spray(payloads: impl IntoIterator<Item = (u32, f32)>) -> (f32, Thrown) {
    payloads.into_iter().fold((0.0, Thrown::default()), |(mass, glass), (surface, m)| {
        (mass + m, glass.plus(thrown(surface).scaled(m.max(0.0))))
    })
}

/// The tyre spray of a drive's frames: per frame index, the sum of its payloads' mass and the running total, and the
/// same of what they throw at the windscreen.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Spray {
    /// Per frame index, the total mass up to and including it, and the frame's physics step.
    totals: Vec<f64>,
    glass: Vec<[f64; 3]>,
    steps: Vec<u32>,
}

impl Spray {
    /// Frames whose spray is known (indices `0..len`).
    pub fn len(&self) -> usize {
        self.totals.len()
    }

    pub fn is_empty(&self) -> bool {
        self.totals.is_empty()
    }

    /// Whether frame `index` is known with physics step `step`.
    pub fn knows(&self, index: usize, step: u32) -> bool {
        self.steps.get(index) == Some(&step)
    }

    /// Records frame `index` (physics step `step`) with its summed payload mass and what it throws at the windscreen
    /// ([`frame_spray`]). A frame already known with the same step keeps its spray; one known with another step (a new
    /// drive) drops it and the frames after it. A gap counts as no spray.
    pub fn record(&mut self, index: usize, step: u32, (mass, glass): (f32, Thrown)) {
        if self.knows(index, step) {
            return;
        }
        self.totals.truncate(index);
        self.glass.truncate(index);
        self.steps.truncate(index);
        let before = self.totals.last().copied().unwrap_or(0.0);
        let glass_before = self.glass.last().copied().unwrap_or_default();
        self.totals.resize(index, before);
        self.glass.resize(index, glass_before);
        self.steps.resize(index, u32::MAX);
        self.totals.push(before + f64::from(mass.max(0.0)));
        self.glass.push(std::array::from_fn(|k| glass_before[k] + glass.wide()[k]));
        self.steps.push(step);
    }

    /// The dirt level after frame `index` (the last known frame's beyond them).
    pub fn level(&self, index: usize) -> f32 {
        let total = self.totals.get(index).or(self.totals.last()).copied().unwrap_or(0.0);
        ((total * PER_MASS) as f32).min(MOST)
    }

    /// What the known frames before `index` threw at the windscreen.
    pub fn before(&self, index: usize) -> Thrown {
        Thrown::narrow(
            index.checked_sub(1).and_then(|i| self.glass.get(i).or(self.glass.last())).copied().unwrap_or_default(),
        )
    }

    /// What frame `index` threw at the windscreen (nothing when unknown).
    pub fn thrown(&self, index: usize) -> Thrown {
        let at = |i: usize| self.glass.get(i).copied();
        match (at(index), index.checked_sub(1).map_or(Some([0.0; 3]), at)) {
            (Some(now), Some(before)) => Thrown::narrow(std::array::from_fn(|k| now[k] - before[k])),
            _ => Thrown::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_level_sums_the_spray_and_stops_at_its_cap() {
        let gravel = |m: f32| (m, Thrown { mud: m, ..Thrown::default() });
        let mut spray = Spray::default();
        spray.record(0, 10, gravel(2000.0));
        spray.record(1, 11, gravel(3000.0));
        // A gap: frame 2 had no spray.
        spray.record(3, 13, gravel(-1.0));
        assert_eq!(spray.len(), 4);
        assert!((spray.level(1) - 0.05).abs() < 1e-6);
        assert_eq!(spray.level(2), spray.level(1));
        assert_eq!(spray.thrown(1).mud, 3000.0);
        assert_eq!(spray.thrown(2).mud, 0.0);
        assert_eq!(spray.thrown(9).mud, 0.0);
        assert_eq!((spray.before(0).mud, spray.before(2).mud, spray.before(9).mud), (0.0, 5000.0, 4999.0));
        spray.record(4, 14, gravel(1e6));
        assert_eq!(spray.level(4), MOST);
        assert_eq!(spray.level(99), MOST, "past the known frames, the last level");
        // The same frame again changes nothing; another drive's frame replaces it and the later ones.
        spray.record(1, 11, gravel(0.0));
        assert_eq!(spray.len(), 5);
        spray.record(1, 1, gravel(0.0));
        assert_eq!(spray.len(), 2);
        assert!((spray.level(1) - 0.02).abs() < 1e-6);
    }

    #[test]
    fn each_surface_throws_its_own_dirt_at_the_glass() {
        // The exe's ids (`MaterialManager`): every surface's spray dirties the paint, only some reach the glass so.
        let (gravel, tarmac, wet_tarmac, covered, snow, tree) = (17, 37, 39, 53, 103, 30);
        assert_eq!(thrown(gravel), Thrown { mud: 1.0, ..Thrown::default() });
        assert_eq!(thrown(181), thrown(gravel), "BR2_Gravel_Coarse_Shallow_Dry");
        assert_eq!(thrown(tarmac), Thrown::default());
        assert_eq!(thrown(38), Thrown { water: 0.5, ..Thrown::default() });
        assert_eq!(thrown(wet_tarmac), Thrown { water: 1.0, ..Thrown::default() });
        assert_eq!(thrown(covered), Thrown { mud: 0.5, ..Thrown::default() });
        assert_eq!(thrown(snow), Thrown { snow: 1.0, ..Thrown::default() });
        assert_eq!(thrown(tree), Thrown::default());
        assert_eq!(thrown(4), Thrown::default(), "an id the table lacks");
        assert_eq!(thrown(1000), Thrown::default());
        let (mass, glass) = frame_spray([(gravel, 10.0), (tarmac, 20.0), (wet_tarmac, 4.0), (snow, -1.0)]);
        assert_eq!(mass, 33.0, "the paint's dirt takes all of it");
        assert_eq!(glass, Thrown { mud: 10.0, snow: 0.0, water: 4.0 });
    }

    #[test]
    fn only_the_exterior_panes_but_the_windscreen_take_a_film() {
        assert_eq!(grime("e_window_b", "CarGlass"), 10);
        assert_eq!(grime("e_window_lf", "CarGlass"), 3);
        assert_eq!(grime("e_window_f", "CarGlass"), 0);
        assert_eq!(grime("i_window_b", "CarGlass"), 0);
        assert_eq!(grime("e_window_b", "CarTextureNoDirt"), 0);
    }
}
