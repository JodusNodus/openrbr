//! The damping handlers: leaves and twigs thrown when the car goes through a bush or another soft object
//! (`E82E_Sys::ScanEmit` 0x5dd460 and `E82D_Fed::Emit` 0x5dbf90 on `CSolid::Emit`'s damping event). Physics frame, z up.
//!
//! Faithful: the event's material (below 255) through the file's material aliases picks the handlers; each throws
//! `(penetration + add) * mod` particles (truncated, none below one) at the event's contact point, launched along its
//! push vector times the velocity modifier. Ours: the particles die 1 m below the contact point.

use glam::Vec3;
use rbr_formats::pss::ParticleFile;

use super::sim::Simulation;

/// Ours: how far below the contact point the thrown particles die, metres.
const FLOOR_BELOW: f32 = 1.0;

#[derive(Debug, Clone)]
struct Handler {
    material: u32,
    pool: usize,
    penetration: [f32; 2],
    velocity: f32,
}

/// A file's damping handlers.
#[derive(Debug, Clone)]
pub struct Damping {
    handlers: Vec<Handler>,
    /// The file's material aliases, `(from, to)`.
    aliases: Vec<(u32, u32)>,
}

impl Damping {
    pub fn new(file: &ParticleFile, sim: &Simulation) -> Damping {
        let handlers = file
            .damping
            .iter()
            .filter_map(|d| {
                Some(Handler {
                    material: d.material,
                    pool: sim.find(&d.emitter)?,
                    penetration: d.penetration,
                    velocity: d.velocity_modifier,
                })
            })
            .collect();
        Damping { handlers, aliases: file.material_aliases.clone() }
    }

    /// One damping event.
    pub fn event(&self, sim: &mut Simulation, penetration: f32, material: u32, at: Vec3, push: Vec3) {
        if material >= 0xff {
            return;
        }
        let handled = self.aliases.iter().find(|&&(from, _)| from == material).map_or(material, |&(_, to)| to);
        for h in self.handlers.iter().filter(|h| h.material == handled) {
            let count = (penetration + h.penetration[0]) * h.penetration[1];
            if count < 1.0 {
                continue;
            }
            sim.emit(h.pool, at, at, push * h.velocity, count.trunc(), at.z - FLOOR_BELOW);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rbr_formats::pss;

    #[test]
    fn a_bush_throws_by_its_penetration() {
        let text = "version3 1 0 0 1 0 1 0 1 0 0 1 1 1 1 1 1 10 \
            1 pc 13 1 1 1 1 0 1 1 1 1 1 1 1 1 0.1 0.1 0.1 0.1 0.1 0.1 0 1 2 0.41 0 0 0 0 BLEND_BLEND 0 0 0.5 0 \
            1 ec 2 0.2 0.2 -3 0.3 0 0 0 0 0 1 pc \
            1 em ec 500 0 0 1 0 0 -9.81 0 0 0 \
            10 0 0 0 1 6 168 0 0 \
            1 168 em -0.1 32 1 $";
        let file = pss::parse(text).unwrap();
        let mut sim = Simulation::new(&file, 1);
        let damping = Damping::new(&file, &sim);
        damping.event(&mut sim, 0.12, 168, Vec3::ZERO, Vec3::X);
        assert_eq!(sim.count(), 0, "(0.12 - 0.1) * 32 is below one");
        damping.event(&mut sim, 0.3, 6, Vec3::ZERO, Vec3::X);
        assert_eq!(sim.count(), 6, "material 6 is handled as 168: (0.3 - 0.1) * 32");
        damping.event(&mut sim, 0.3, 300, Vec3::ZERO, Vec3::X);
        assert_eq!(sim.count(), 6);
    }
}
