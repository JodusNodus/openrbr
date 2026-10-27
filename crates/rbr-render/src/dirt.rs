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
//! level ([`grime`]). With `Enhance::particles`, the dirt by what the surfaces threw ([`CarDirt::look`]): mud tinted by
//! the stage's dust, darker and glossier in the wet, packed snow on snow, little on tarmac.

use std::sync::LazyLock;

use rbr_formats::lsp_keys::{Class, KEYS};

/// The level per unit of spray amount (`OnTireParticles`).
pub const PER_MASS: f64 = 1e-5;
/// The most the level reaches.
pub const MOST: f32 = 0.9;
/// The dirt's colour in the game's look: `c3.xyz` comes from `m_unkC0`, which nothing writes (`notes/formats.md`
/// "Shaders and vertex layouts"); this is the manager's unused preset `m_dirtColor`.
pub const COLOUR: [f32; 3] = [0.27, 0.22, 0.14];
/// Ours, with the surfaces: the share of the spray's mass that dirties the car when it throws water, and when it
/// throws nothing (dry tarmac); the dry dust's colour as a share of the stage's dust colour; the packed snow's
/// colour; and the share of the paint's reflection the wet mud keeps.
const WATER_GRIME: f32 = 0.3;
const DRY_GRIME: f32 = 0.1;
const DRY_DUST: f32 = 0.85;
const SNOW: [f32; 3] = [0.8, 0.82, 0.85];
const WET_SHEEN: f32 = 0.35;

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

/// Whether a car part takes a dirt spatter with the surfaces' dirt (ours, `Shade::spatter`): the wheels and what hangs
/// by them (`e_wheel_*`, `e_wheelsusp_*`, ...) and the mud flaps, by name, as the cars' classes and dirt maps for them
/// vary (the 208 Rally4's wheels are `CarPlastic` and `CarRim`, its `TireDirt` map is empty, as a third of the RSF
/// cars' are).
pub fn spatter(part: &str) -> bool {
    part.starts_with("e_wheel") || part.starts_with("e_mudflap")
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

/// What the car's dirt has taken up to a frame ([`Spray::car`]): the game's level, the spray's mass, and what it threw.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CarDirt {
    pub level: f32,
    pub mass: f32,
    pub thrown: Thrown,
    /// The stage's dust colour (`particles::wheel_spray::dust_colour`), which tints its mud.
    pub dust: [f32; 3],
}

/// The car's dirt as drawn: the level its masks take, its colour and the share of the paint's reflection it keeps.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Look {
    pub level: f32,
    pub colour: [f32; 3],
    pub sheen: f32,
}

impl CarDirt {
    /// The game's look: the level, [`COLOUR`], no reflection. With `surfaces` (ours, `Enhance::particles`) the level
    /// takes the mud and snow thrown, `WATER_GRIME` of the water and `DRY_GRIME` of the rest; the colour goes from
    /// the stage's dust (`DRY_DUST` of it) when dry to the game's mud tinted by that dust against the neutral one
    /// when wet, by `wetness` (0 dry, 0.5 damp, 1 wet), and to `SNOW` by the snow's share; the wet mud keeps
    /// `WET_SHEEN` of the reflection.
    pub fn look(self, surfaces: bool, wetness: f32) -> Look {
        if !surfaces {
            return Look { level: self.level, colour: COLOUR, sheen: 0.0 };
        }
        let Thrown { mud, snow, water } = self.thrown;
        let rest = (self.mass - mud - snow - water).max(0.0);
        let grime = mud + snow + WATER_GRIME * water + DRY_GRIME * rest;
        let snowy = if grime > 0.0 { snow / grime } else { 0.0 };
        let neutral = crate::particles::wheel_spray::NEUTRAL_DUST;
        let colour = std::array::from_fn(|k| {
            let (dry, wet) = (DRY_DUST * self.dust[k], COLOUR[k] * self.dust[k] / neutral[k]);
            let mud = dry + (wet - dry) * wetness;
            mud + (SNOW[k] - mud) * snowy
        });
        Look {
            level: ((f64::from(grime) * PER_MASS) as f32).min(MOST),
            colour,
            sheen: WET_SHEEN * wetness * (1.0 - snowy),
        }
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

    /// The car's dirt after frame `index` (the last known frame's beyond them), with the stage's `dust` colour.
    pub fn car(&self, index: usize, dust: [f32; 3]) -> CarDirt {
        let mass = self.totals.get(index).or(self.totals.last()).copied().unwrap_or(0.0);
        CarDirt { level: self.level(index), mass: mass as f32, thrown: self.before(index + 1), dust }
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
    fn the_surfaces_give_the_dirt_its_level_and_colour() {
        let dust = crate::particles::wheel_spray::NEUTRAL_DUST;
        let car = |mass, thrown| CarDirt { level: 0.5, mass, thrown, dust };
        let near = |a: [f32; 3], b: [f32; 3]| a.iter().zip(b).all(|(a, b)| (a - b).abs() < 1e-6);
        let gravel = car(1e4, Thrown { mud: 1e4, ..Thrown::default() });
        assert_eq!(gravel.look(false, 1.0), Look { level: 0.5, colour: COLOUR, sheen: 0.0 }, "the game's");
        let dry = gravel.look(true, 0.0);
        assert!((dry.level - 0.1).abs() < 1e-6 && dry.sheen == 0.0);
        assert_eq!(dry.colour, dust.map(|c| DRY_DUST * c), "dry: the stage's dust");
        let wet = gravel.look(true, 1.0);
        assert!(near(wet.colour, COLOUR) && wet.sheen == WET_SHEEN, "wet: the game's mud, glossy");
        let snow = car(1e4, Thrown { snow: 1e4, ..Thrown::default() }).look(true, 1.0);
        assert!(near(snow.colour, SNOW) && snow.sheen == 0.0, "snow: packed snow, matte");
        let tarmac = car(1e4, Thrown::default()).look(true, 0.0);
        assert!((tarmac.level - 0.1 * DRY_GRIME).abs() < 1e-6, "dry tarmac: little");
        assert_eq!(car(0.0, Thrown::default()).look(true, 0.0).level, 0.0);
    }

    #[test]
    fn the_wheels_and_mud_flaps_take_a_spatter() {
        assert!(spatter("e_wheel_tarmac_lf") && spatter("e_wheelsusp_rb") && spatter("e_mudflap_b"));
        assert!(!spatter("e_steeringwheel") && !spatter("e_sparewheel_tarmac") && !spatter("e_body"));
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
