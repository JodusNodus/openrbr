//! The water wave a wheel leaves in water: two sheets per wheel, one thrown out of the car's side and one in, drawn
//! with `Textures/SplashColor.dds` and `Textures/splashalpha.tga` (`E82F_Emit::Run` 0x5de140, `E84Wave` 0x5ea3c0 to
//! 0x5ea9a0, `E83RendererStep::M5ea720`, `CPC_Wave::Vf04` 0x5f05e0). Physics frame, z up.
//!
//! Faithful: the launch speeds and the alpha from the wheel's water level (tables keyed at 0x7d64c0; values 0x74d6c0
//! to 0x74d724), which edge each sheet leaves from, a ring of 64 slots that moves on every 0.05 s, the latest emit
//! filling the current slot with a random share of the alpha, a slot living 0.6 s, a break where the slot before had
//! died, each slot's profile (`E84Wave::Splash`: six points along its arc between `age - 0.3` and `age`, the rise
//! fading to nothing at 0.6 s, gravity 20.81, at most 0.1 m over the start), and the drawing (`CPC_Wave::Vf04`): a
//! strip of triangles between neighbouring slots, each vertex's alpha full until 0.25 s and gone at 0.55 s, clear at
//! the top point and on a sheet's first slot, the colour's texture coordinate along the profile and the alpha's
//! along it flipped, both along the sheet with its length (0.2 a metre from the first slot). Ours: the texture's
//! scroll (`CPC_Wave` adds two offsets we have not traced) and the vertex alpha times the share as the colour's alpha.

use bytemuck::{Pod, Zeroable};
use glam::Vec3;

use super::sim::{to_stage, Rng};
use super::tyres::table_lerp;

/// The water levels the wave tables are keyed by (0x7d64c0).
const KEYS: [f32; 6] = [0.0, 0.025, 0.06, 0.135, 0.2, 0.4];
/// The outward sheet's sideways and upward speeds, the inward one's, and the alpha.
const OUT_SIDE: [f32; 6] = [0.0, 0.0, 0.5, 1.0, 3.0, 7.0];
const OUT_UP: [f32; 6] = [0.0, 0.0, 1.0, 2.5, 7.0, 20.0];
const IN_SIDE: [f32; 6] = [0.0, 0.0, 0.5, 1.0, 3.0, 7.0];
const IN_UP: [f32; 6] = [0.0, 0.0, 1.0, 2.5, 5.0, 5.0];
const ALPHA: [f32; 6] = [0.0, 0.0, 1.0, 1.0, 1.0, 1.0];
const SLOTS: usize = 64;
const SLOT_TIME: f32 = 0.05;
const LIFE: f32 = 0.6;
/// The age `E84Wave::Emit` gives a new slot before it measures the sheet's length.
const FIRST_AGE: f32 = 0.01;
/// The texture coordinate per metre of sheet (`E84Wave::Emit`'s 0.2).
const ALONG_PER_METRE: f32 = 0.2;
/// The vertex alpha falls `FADE` a second, from 0.05 s before the life ends (`CPC_Wave::Vf04`:
/// `(0.55 - age) * 10 / 3`).
const FADE: f32 = 10.0 / 3.0;
const PROFILE: usize = 6;
const GRAVITY: f32 = 20.81;
/// The texture sprite id of the wave's layer ([`super::gpu::Sprites::add`]).
pub const SPRITE: u32 = 1000;

/// One vertex of the sheets: the stage position, the colour's and the alpha's texture coordinates, the colour (its
/// alpha is the vertex's) and the texture layer. Drawn as a triangle list (`gpu::Pass`).
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
#[repr(C)]
pub struct WaveVertex {
    pub position: [f32; 3],
    pub colour_uv: [f32; 2],
    pub alpha_uv: [f32; 2],
    pub colour: [f32; 4],
    pub layer: f32,
}

#[derive(Debug, Clone, Copy, Default)]
struct Slot {
    origin: Vec3,
    velocity: Vec3,
    /// Seconds since the emit; at [`LIFE`] the slot is dead.
    age: f32,
    alpha: f32,
    /// The slot before it was dead when it was filled: the sheet starts here.
    starts: bool,
    /// The sheet's length up to here, times [`ALONG_PER_METRE`]: the texture's coordinate along the sheet.
    along: f32,
}

impl Slot {
    fn alive(&self) -> bool {
        self.age < LIFE
    }

    /// `E84Wave::Splash`: profile point `i` (0 to 5).
    fn point(&self, i: usize) -> Vec3 {
        let age = self.age;
        let back = (age - 0.3).max(0.0);
        let f = (back - age) * i as f32 * 0.2 + age;
        let mut p = self.velocity * f;
        p.z *= (LIFE - age) * (1.0 / LIFE);
        p += self.origin;
        p.z -= GRAVITY * f * f * 0.5;
        p.z = p.z.min(self.origin.z + 0.1);
        p
    }

    /// The alpha of profile point `i` (`CPC_Wave::Vf04`): the top point and a sheet's first slot are clear.
    fn vertex_alpha(&self, i: usize) -> f32 {
        if self.starts || i + 1 == PROFILE {
            return 0.0;
        }
        ((LIFE - 0.05 - self.age) * FADE).clamp(0.0, 1.0)
    }

    /// Profile point `i` of this slot, for the texture layer `layer`.
    fn vertex(&self, i: usize, layer: f32) -> WaveVertex {
        let fu = i as f32 * 0.2;
        WaveVertex {
            position: to_stage(self.point(i)).to_array(),
            colour_uv: [self.along, fu],
            alpha_uv: [self.along, 1.0 - fu],
            colour: [1.0, 1.0, 1.0, self.alpha * self.vertex_alpha(i)],
            layer,
        }
    }
}

/// One sheet: its ring of slots.
#[derive(Debug, Clone)]
struct Sheet {
    slots: [Slot; SLOTS],
    current: usize,
    timer: f32,
}

impl Sheet {
    fn new() -> Sheet {
        Sheet { slots: [Slot { age: LIFE, ..Slot::default() }; SLOTS], current: 0, timer: 0.0 }
    }

    /// `E83RendererStep::M5ea720`: every 0.05 s the ring moves on to a dead slot; the live ones age.
    fn step(&mut self, dt: f32) {
        self.timer += dt;
        if self.timer >= SLOT_TIME {
            self.current = (self.current + 1) % SLOTS;
            self.slots[self.current] = Slot { age: LIFE, ..Slot::default() };
            self.timer = 0.0;
        }
        for slot in self.slots.iter_mut().filter(|s| s.alive()) {
            slot.age += dt;
        }
    }

    /// `E84Wave::Emit`: fills the current slot. Its texture coordinate runs on from the slot before, when that lives.
    fn emit(&mut self, origin: Vec3, velocity: Vec3, alpha: f32) {
        let previous = self.slots[(self.current + SLOTS - 1) % SLOTS];
        let mut slot = Slot { origin, velocity, age: FIRST_AGE, alpha, starts: !previous.alive(), along: 0.0 };
        if !slot.starts {
            slot.along = previous.along + ALONG_PER_METRE * (origin - slot.point(0)).length();
        }
        self.slots[self.current] = slot;
    }

    /// The strip's triangles between each two neighbouring live slots, oldest first.
    fn vertices(&self, out: &mut Vec<WaveVertex>, layer: f32) {
        for k in 1..SLOTS {
            let (a, b) = (&self.slots[(self.current + k) % SLOTS], &self.slots[(self.current + k + 1) % SLOTS]);
            if !a.alive() || !b.alive() {
                continue;
            }
            for j in 0..PROFILE - 1 {
                let [a0, a1, b0, b1] =
                    [a.vertex(j, layer), a.vertex(j + 1, layer), b.vertex(j, layer), b.vertex(j + 1, layer)];
                out.extend([a0, a1, b0, a1, b1, b0]);
            }
        }
    }
}

/// The four wheels' two sheets each.
#[derive(Debug, Clone)]
pub struct Waves {
    sheets: Vec<[Sheet; 2]>,
    rng: Rng,
}

impl Waves {
    pub fn new(seed: u32) -> Waves {
        Waves { sheets: (0..4).map(|_| [Sheet::new(), Sheet::new()]).collect(), rng: Rng::new(seed) }
    }

    pub fn clear(&mut self) {
        for sheet in self.sheets.iter_mut().flatten() {
            *sheet = Sheet::new();
        }
    }

    /// One wheel's emit at water level `level` (over 1e-4, on the ground): the outward sheet from `edges[0]`, the
    /// inward from `edges[1]`, `out` the car's side pointing away from the wheel.
    pub fn emit(&mut self, wheel: usize, level: f32, edges: [Vec3; 2], out: Vec3) {
        let Some(sheets) = self.sheets.get_mut(wheel) else { return };
        let alpha = table_lerp(&KEYS, &ALPHA, level);
        let (out_side, out_up) = (table_lerp(&KEYS, &OUT_SIDE, level), table_lerp(&KEYS, &OUT_UP, level));
        let (in_side, in_up) = (table_lerp(&KEYS, &IN_SIDE, level), table_lerp(&KEYS, &IN_UP, level));
        if out_side != 0.0 && out_up != 0.0 {
            let share = self.rng.unit();
            sheets[0].emit(edges[0], out * out_side + Vec3::Z * out_up, alpha * share);
        }
        if in_side != 0.0 && in_up != 0.0 {
            let share = self.rng.unit();
            sheets[1].emit(edges[1], -out * in_side + Vec3::Z * in_up, alpha * share);
        }
    }

    pub fn step(&mut self, dt: f32) {
        for sheet in self.sheets.iter_mut().flatten() {
            sheet.step(dt);
        }
    }

    /// The sheets' triangles (stage space), with texture layer `layer`.
    pub fn vertices(&self, layer: u32) -> Vec<WaveVertex> {
        let mut out = Vec::new();
        for sheet in self.sheets.iter().flatten() {
            sheet.vertices(&mut out, layer as f32);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sheet_spreads_and_dies() {
        let mut waves = Waves::new(1);
        for _ in 0..30 {
            waves.emit(0, 0.3, [Vec3::ZERO, Vec3::X], Vec3::Y);
            waves.step(0.01);
        }
        let vertices = waves.vertices(0);
        assert!(!vertices.is_empty());
        assert!(vertices.iter().all(|v| v.position[1] <= 0.1 + 1e-4), "at most 0.1 m over the water (stage y is up)");
        for _ in 0..70 {
            waves.step(0.01);
        }
        assert!(waves.vertices(0).is_empty(), "every slot is past 0.6 s");
        let mut dry = Waves::new(1);
        dry.emit(0, 0.01, [Vec3::ZERO, Vec3::X], Vec3::Y);
        dry.step(0.01);
        assert!(dry.vertices(0).is_empty(), "below 0.025 the tables give no speed");
    }

    #[test]
    fn the_texture_runs_along_the_sheet_and_the_alpha_fades() {
        let mut waves = Waves::new(1);
        for _ in 0..12 {
            waves.emit(0, 0.3, [Vec3::ZERO, Vec3::X], Vec3::Y);
            waves.step(0.05);
        }
        assert!(waves.vertices(0).iter().any(|v| v.colour_uv[0] > 0.0), "the sheet's length moves the texture on");
        // The colour runs across the profile, the alpha the other way.
        let slot = Slot { age: 0.1, alpha: 1.0, ..Slot::default() };
        assert_eq!(slot.vertex(1, 0.0).colour_uv, [0.0, 0.2]);
        assert_eq!(slot.vertex(1, 0.0).alpha_uv, [0.0, 0.8]);
        // The top point is clear, a sheet's first slot is clear, and the alpha is gone at 0.55 s.
        assert_eq!(slot.vertex(5, 0.0).colour[3], 0.0);
        assert_eq!(Slot { starts: true, ..slot }.vertex(0, 0.0).colour[3], 0.0);
        let old = Slot { age: 0.4, alpha: 1.0, ..Slot::default() };
        assert!((old.vertex(0, 0.0).colour[3] - (0.55 - 0.4) * FADE).abs() < 1e-5);
        assert_eq!(Slot { age: 0.56, alpha: 1.0, ..Slot::default() }.vertex(0, 0.0).colour[3], 0.0);
    }
}
