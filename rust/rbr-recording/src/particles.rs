//! The tyre-particle record a live stepping process writes (`rbr-standalone --live`), which the viewer's particles
//! and skid marks read. It carries each wheel's `CSigTireParticles` payload of the step, as `CWheel`'s step emits it
//! (`E51MultiTire::Emit`, 0x53d0d0; `notes/particles.md`, Inputs). Little-endian:
//!
//! ```text
//! u32 tag      'PTC1'
//! u32 length   the payload bytes that follow: n * 0xb0
//! u8  payloads[n][0xb0]
//! ```
//!
//! Field meanings are from `E83System::EmitTire` (0x5e3050) and `E82F_BlkD0::Emit` (0x5ddba0), which read them; the
//! names of rubber, depth and dirt come from their pairing with the skid-mark coefficients (not verified).

use std::io;

/// The tag.
pub const PTC1: [u8; 4] = *b"PTC1";
/// The bytes of one payload carried.
pub const PAYLOAD_BYTES: usize = 0xb0;
/// Most payloads a record holds (a step emits one per wheel).
pub const MAX_PAYLOADS: usize = 16;

/// One wheel's `CSigTireParticles` payload.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct TyreParticles {
    /// +0x00: the wheel, 0 to 3.
    pub wheel: u32,
    /// +0x10 and +0x20: the contact patch's two edge points, world, z up. Skid marks are laid between them.
    pub edges: [[f32; 3]; 2],
    /// +0x50: the velocity `E82F_BlkD0::Emit` launches the particles with (times the handler's velocity modifier).
    pub velocity: [f32; 3],
    /// +0x80: rubber.
    pub rubber: f32,
    /// +0x84: the smoke flow of the signal handlers.
    pub smoke: f32,
    /// +0x88: the mass flow of the signal handlers (the particle amount HedgeHog3D's car dirt also reads).
    pub mass: f32,
    /// +0x90: depth.
    pub depth: f32,
    /// +0x94: dirt.
    pub dirt: f32,
    /// +0x98: what the system adds ×0.1 to its +0x838 (water, by `E82F_Emit::Run`'s use: the splash level).
    pub splash: f32,
    /// +0xa0: the two surface ids under the patch and the blend weight between them (`E36Triple`).
    pub surfaces: [u32; 2],
    pub blend: f32,
    /// +0xac: the wheel touches the ground.
    pub contact: bool,
}

impl TyreParticles {
    /// The surface the handlers go by: the first below a blend of 0.5, else the second (`E36Triple::MinPick`,
    /// 0x4f3ec0).
    pub fn surface(&self) -> u32 {
        if self.blend < 0.5 {
            self.surfaces[0]
        } else {
            self.surfaces[1]
        }
    }

    /// The patch's centre, world, z up.
    pub fn centre(&self) -> [f32; 3] {
        std::array::from_fn(|i| 0.5 * (self.edges[0][i] + self.edges[1][i]))
    }

    /// Reads a payload; `None` when `bytes` is shorter than [`PAYLOAD_BYTES`].
    pub fn from_payload(bytes: &[u8]) -> Option<Self> {
        let bytes = bytes.get(..PAYLOAD_BYTES)?;
        let word = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
        let float = |at: usize| f32::from_bits(word(at));
        let vec3 = |at: usize| [float(at), float(at + 4), float(at + 8)];
        Some(TyreParticles {
            wheel: word(0),
            edges: [vec3(0x10), vec3(0x20)],
            velocity: vec3(0x50),
            rubber: float(0x80),
            smoke: float(0x84),
            mass: float(0x88),
            depth: float(0x90),
            dirt: float(0x94),
            splash: float(0x98),
            surfaces: [word(0xa0), word(0xa4)],
            blend: float(0xa8),
            contact: word(0xac) != 0,
        })
    }
}

/// The record for `payloads` (raw, [`PAYLOAD_BYTES`] each; at most [`MAX_PAYLOADS`]).
pub fn record(payloads: &[[u8; PAYLOAD_BYTES]]) -> Vec<u8> {
    let payloads = &payloads[..payloads.len().min(MAX_PAYLOADS)];
    let mut out = Vec::with_capacity(8 + payloads.len() * PAYLOAD_BYTES);
    crate::live::write_record(&mut out, PTC1, payloads.as_flattened());
    out
}

/// The payloads of a record's body (the bytes after the tag and length).
pub fn read_body(body: &[u8]) -> io::Result<Vec<TyreParticles>> {
    if !body.len().is_multiple_of(PAYLOAD_BYTES) || body.len() > MAX_PAYLOADS * PAYLOAD_BYTES {
        return Err(io::Error::new(io::ErrorKind::InvalidData, format!("a PTC1 body of {} bytes", body.len())));
    }
    Ok(body.chunks_exact(PAYLOAD_BYTES).filter_map(TyreParticles::from_payload).collect())
}

/// The particle-event record a live stepping process writes after PTC1: the step's other particle signals, as the
/// game's particle system receives them (`E83System::Init`'s connections, `notes/particles.md`). Little-endian:
///
/// ```text
/// u32 tag      'PEV1'
/// u32 length   n * 0x44
/// { u32 kind; u8 payload[0x40] } events[n]
/// ```
///
/// Kinds: 1 `CSigCarDamage` (the damage handler's 0x40-byte record, `E83System::EmitDebris` 0x5df5f0), 2 the damping
/// event of `CSolid::Emit` (`E82E_Sys::ScanEmit` 0x5dd460), 3 `CSigEngineFire` (a float, `E82E_Sys::Set844`), 4 the
/// step's `CSigEngineNoise` words (12, the engine state from +0x2c, `rbr_recording::sound`; `E82E_Sys::Scale838` reads
/// word 5), 5 the exhaust's inputs after the frame (the engine state's +0x200 and the engine's +0x188, which the car's
/// frame update `CCar::Vf28x` turns into `CSigExhaust` through `E43EngineSim::Sim8CF0` 0x518cf0 for
/// `E83System::EmitExhaust` 0x5dea10; the standalone does not run that frame update), 6 the car's lamps after the frame
/// (`CCarLightSystem` at the car's +0x838: its state's switch per kind 1 to 6, the light block's master word and each
/// kind's lamp words, which `CCarLightSystem::Update` 0x4f2150 sends as `CSigCarLight`). Readers skip kinds they do not
/// know.
pub const PEV1: [u8; 4] = *b"PEV1";
/// The payload bytes of one event, and of one event with its kind.
pub const EVENT_PAYLOAD_BYTES: usize = 0x40;
pub const EVENT_BYTES: usize = 4 + EVENT_PAYLOAD_BYTES;
/// Most events a record holds.
pub const MAX_EVENTS: usize = 64;
/// The kinds.
pub const CAR_DAMAGE: u32 = 1;
pub const DAMPING: u32 = 2;
pub const ENGINE_FIRE: u32 = 3;
pub const ENGINE_NOISE: u32 = 4;
pub const EXHAUST: u32 = 5;
pub const LIGHTS: u32 = 6;

/// One particle signal of a step.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Event {
    /// `CSigCarDamage`: its kind (+4; `EmitDebris` runs for 1 to 3), the impact's force (+0x10) and where it hit
    /// (+0x20, world, z up).
    CarDamage { kind: u32, force: f32, at: [f32; 3] },
    /// The damping event (a bush or other soft object the car goes through): the penetration (+0), the material id
    /// (+4), the contact point (+0x20) and the vector `E82D_Fed::Emit` launches along (+0x30).
    Damping { penetration: f32, material: u32, at: [f32; 3], push: [f32; 3] },
    /// `CSigEngineFire`: the fire's level (the damage handler sends 1).
    EngineFire(f32),
    /// The engine's `CSigEngineNoise` words 4 (misfire) and 5 (what `Scale838` turns into the engine steam).
    EngineNoise { misfire: f32, steam: f32 },
    /// The exhaust's inputs after the frame: the engine state's +0x200 (+0, which `Sim8CF0` smooths into
    /// `CSigExhaust`'s flow) and the engine's +0x188 (+4, `CSigExhaust`'s broken-exhaust flow).
    Exhaust { flow: f32, broken: f32 },
    /// The lamps: each kind's switch (`switches[kind - 1]`, words 0 to 5), the master word (6, the light block's +0xdc,
    /// which the engine's electrical damage clears), and per kind its lamps' works (bits 0 to 3) and glass (bits 8 to
    /// 11) flags (words 7 to 12).
    Lights { switches: [u32; 6], master: u32, lamps: [u32; 6] },
}

impl Event {
    /// The event of a kind and payload; `None` for an unknown kind.
    pub fn from_payload(kind: u32, payload: &[u8; EVENT_PAYLOAD_BYTES]) -> Option<Event> {
        let word = |at: usize| u32::from_le_bytes(payload[at..at + 4].try_into().unwrap());
        let float = |at: usize| f32::from_bits(word(at));
        let vec3 = |at: usize| [float(at), float(at + 4), float(at + 8)];
        match kind {
            CAR_DAMAGE => Some(Event::CarDamage { kind: word(4), force: float(0x10), at: vec3(0x20) }),
            DAMPING => {
                Some(Event::Damping { penetration: float(0), material: word(4), at: vec3(0x20), push: vec3(0x30) })
            }
            ENGINE_FIRE => Some(Event::EngineFire(float(0))),
            ENGINE_NOISE => Some(Event::EngineNoise { misfire: float(0x10), steam: float(0x14) }),
            EXHAUST => Some(Event::Exhaust { flow: float(0), broken: float(4) }),
            LIGHTS => Some(Event::Lights {
                switches: std::array::from_fn(|k| word(4 * k)),
                master: word(24),
                lamps: std::array::from_fn(|k| word(28 + 4 * k)),
            }),
            _ => None,
        }
    }
}

/// The PEV1 record for `events` (kind and payload; at most [`MAX_EVENTS`]).
pub fn events_record(events: &[(u32, [u8; EVENT_PAYLOAD_BYTES])]) -> Vec<u8> {
    let mut body = Vec::with_capacity(events.len().min(MAX_EVENTS) * EVENT_BYTES);
    for (kind, payload) in events.iter().take(MAX_EVENTS) {
        body.extend_from_slice(&kind.to_le_bytes());
        body.extend_from_slice(payload);
    }
    let mut out = Vec::with_capacity(8 + body.len());
    crate::live::write_record(&mut out, PEV1, &body);
    out
}

/// The events of a PEV1 body; unknown kinds are skipped.
pub fn read_events(body: &[u8]) -> io::Result<Vec<Event>> {
    if !body.len().is_multiple_of(EVENT_BYTES) || body.len() > MAX_EVENTS * EVENT_BYTES {
        return Err(io::Error::new(io::ErrorKind::InvalidData, format!("a PEV1 body of {} bytes", body.len())));
    }
    Ok(body
        .chunks_exact(EVENT_BYTES)
        .filter_map(|e| Event::from_payload(u32::from_le_bytes(e[..4].try_into().unwrap()), e[4..].try_into().unwrap()))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payloads_round_trip() {
        let mut payload = [0u8; PAYLOAD_BYTES];
        payload[0] = 2;
        payload[0x10..0x14].copy_from_slice(&1.0f32.to_le_bytes());
        payload[0x20..0x24].copy_from_slice(&3.0f32.to_le_bytes());
        payload[0x88..0x8c].copy_from_slice(&0.5f32.to_le_bytes());
        payload[0xa0] = 17;
        payload[0xa4] = 18;
        payload[0xa8..0xac].copy_from_slice(&0.75f32.to_le_bytes());
        payload[0xac] = 1;
        let bytes = record(&[payload]);
        assert_eq!(&bytes[..4], b"PTC1");
        let [(tag, body)] = &crate::live::read_records(&mut &[&bytes[..], b"SCN1"].concat()[..]).unwrap()[..] else {
            panic!("one record");
        };
        assert_eq!(*tag, PTC1);
        let wheels = read_body(body).unwrap();
        let w = wheels[0];
        assert_eq!((w.wheel, w.mass, w.surface(), w.contact), (2, 0.5, 18, true));
        assert_eq!(w.centre(), [2.0, 0.0, 0.0]);
        assert!(read_body(&body[1..]).is_err());
    }

    #[test]
    fn events_round_trip() {
        let mut damage = [0u8; EVENT_PAYLOAD_BYTES];
        damage[4] = 2;
        damage[0x10..0x14].copy_from_slice(&20000.0f32.to_le_bytes());
        damage[0x24..0x28].copy_from_slice(&5.0f32.to_le_bytes());
        let mut noise = [0u8; EVENT_PAYLOAD_BYTES];
        noise[0x14..0x18].copy_from_slice(&0.5f32.to_le_bytes());
        let bytes = events_record(&[(CAR_DAMAGE, damage), (99, damage), (ENGINE_NOISE, noise)]);
        assert_eq!(&bytes[..4], b"PEV1");
        let events = read_events(&bytes[8..]).unwrap();
        assert_eq!(
            events,
            [
                Event::CarDamage { kind: 2, force: 20000.0, at: [0.0, 5.0, 0.0] },
                Event::EngineNoise { misfire: 0.0, steam: 0.5 }
            ]
        );
        assert!(read_events(&bytes[9..]).is_err());
    }
}
