//! The scripted events' sounds (`notes/scripted-events.md`, `notes/sound.md` section 8): the rescue helicopter's
//! rotor loop (`CSoundEffects`, `E31_5a30` 0x4d5a30), the bird strike's hit (`CCollisionSnd::OnBirdstrike` 0x4b2460
//! and `Vf68` 0x4b2c50) and the rally incident on the helicopter call (`CRallyIncidentSnd::OnSignal` 0x4b7d10). Pure:
//! commands per step, from the events the characters give (`rbr_render::characters`).
//!
//! - **Helicopter**: while `CRescueHelicopter::IsActive`, one looping `Audio\Ambient\heli_g.wav` voice at the
//!   helicopter's body (`Vf20`), gain `Helicopter amp` times the effects volume. Its min distance is set twice, 50 and
//!   then 100 m (`+0x78` both times), so it is full within 100 m and falls as `100 / d` past it (max distance left at
//!   DirectSound's default); the velocity is set to zero each frame, so no Doppler. It stops when the helicopter is
//!   retired. `heli_r.wav` is the replay's (`E31_5800`), out of scope.
//! - **Bird hit**: `OnBirdstrike` copies the strike's point and plays `IMPACT::VARIOUS` sample 0 (`birdhit`) there,
//!   min 5 m (min 3 m, then max 5 m as called; 1.02 keeps the last as the min, NGP 3 to 5 m), gain `Impacts amp` times the effects volume (the `SOUNDDEF` volume is not applied).
//! - **Rally incident**: the call's `CSigRallyIncident` (code 0) clears and stops the co-driver's speech. 1.02 then
//!   builds `audio\speech\RallyIncident\<lang>\snd_chopper.wav` and never plays it (the handler ends at its
//!   `sprintf`, checked in the disassembly), so nothing is said. That the call emits it (`0x577550`) is inferred.

use crate::voice::{Command, Loop, Shot};
use crate::world::{distance_gain, pan, Scene, DEFAULT_MAX_DISTANCE};

/// The helicopter's min distance (the second `+0x78` call, 0x42c80000); its max is DirectSound's default.
const HELICOPTER_DISTANCES: (f32, f32) = (100.0, f32::INFINITY);
/// Under NGP, whose patch makes the second call `+0x74` (0x4d5ad4), the max distance: 50 to 100 m.
const NGP_HELICOPTER_DISTANCES: (f32, f32) = (50.0, 100.0);
/// The bird hit's distances (`CCollisionSnd::Vf68`: `+0x78` 3.0 at 0x4b2ca9, then `+0x74` 5.0 at 0x4b2cb5): 1.02 keeps
/// the last as the min, NGP 3 to 5 m.
const BIRD_HIT_DISTANCES: (f32, f32) = (5.0, DEFAULT_MAX_DISTANCE);
const NGP_BIRD_HIT_DISTANCES: (f32, f32) = (3.0, 5.0);

/// What the scripted events did since the last step, physics frame (z up).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Events {
    /// The rescue helicopter's body while it runs.
    pub helicopter: Option<[f32; 3]>,
    /// The bird strikes' points.
    pub birdstrikes: Vec<[f32; 3]>,
    /// The rescue helicopter was called (`CSigRallyIncident` 0).
    pub rescue_called: bool,
}

/// The `audio.ini` amps the scripted sounds take.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScriptedGains {
    pub helicopter: f32,
    /// The `Impacts` amp (the bird hit).
    pub bird_hit: f32,
}

impl Default for ScriptedGains {
    /// The retail values.
    fn default() -> ScriptedGains {
        ScriptedGains { helicopter: 0.3, bird_hit: 1.0 }
    }
}

/// The scripted sounds' state between steps.
#[derive(Debug, Clone, Default)]
pub struct Scripted {
    pub gains: ScriptedGains,
    /// The physics is NGP's (the helicopter's distances).
    pub ngp: bool,
    /// The helicopter voice runs.
    helicopter: bool,
}

impl Scripted {
    pub fn new(gains: ScriptedGains) -> Scripted {
        Scripted { gains, ngp: false, helicopter: false }
    }

    /// The commands for one step's events, heard from `scene`'s camera.
    pub fn step(&mut self, scene: &Scene, events: &Events) -> Vec<Command> {
        let mut out = Vec::new();
        if events.rescue_called {
            out.push(Command::StopSpeech);
        }
        let from = scene.listener;
        let distance = |at: [f32; 3]| (0..3).map(|i| (at[i] - from[i]).powi(2)).sum::<f32>().sqrt();
        match events.helicopter {
            Some(at) => {
                let range = if self.ngp { NGP_HELICOPTER_DISTANCES } else { HELICOPTER_DISTANCES };
                let gain = self.gains.helicopter * distance_gain(distance(at), range);
                out.push(Command::Loop { voice: Loop::Helicopter, gain, pitch: 1.0 });
                out.push(Command::Pan { voice: Loop::Helicopter, pan: pan(from, scene.right, at) });
                self.helicopter = true;
            }
            None if self.helicopter => {
                out.push(Command::Loop { voice: Loop::Helicopter, gain: 0.0, pitch: 1.0 });
                self.helicopter = false;
            }
            None => {}
        }
        for &at in &events.birdstrikes {
            let range = if self.ngp { NGP_BIRD_HIT_DISTANCES } else { BIRD_HIT_DISTANCES };
            let gain = self.gains.bird_hit * distance_gain(distance(at), range);
            out.push(Command::ShotAt { shot: Shot::BirdHit, gain, pan: pan(from, scene.right, at) });
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn heli(out: &[Command]) -> Option<f32> {
        out.iter().find_map(|c| match *c {
            Command::Loop { voice: Loop::Helicopter, gain, .. } => Some(gain),
            _ => None,
        })
    }

    #[test]
    fn the_helicopter_is_full_within_100_m_and_stops_when_retired() {
        let mut scripted = Scripted::new(ScriptedGains::default());
        let scene = Scene::default();
        let near = Events { helicopter: Some([0.0, 60.0, 30.0]), ..Events::default() };
        assert_eq!(heli(&scripted.step(&scene, &near)), Some(0.3));
        let far = Events { helicopter: Some([0.0, 400.0, 0.0]), ..Events::default() };
        assert!((heli(&scripted.step(&scene, &far)).unwrap() - 0.075).abs() < 1e-6);
        assert_eq!(heli(&scripted.step(&scene, &Events::default())), Some(0.0));
        assert!(scripted.step(&scene, &Events::default()).is_empty());
    }

    #[test]
    fn a_bird_strike_hits_at_its_point_and_the_call_stops_the_speech() {
        let mut scripted = Scripted::new(ScriptedGains::default());
        let events = Events { birdstrikes: vec![[6.0, 0.0, 0.0]], rescue_called: true, ..Events::default() };
        let out = scripted.step(&Scene::default(), &events);
        assert_eq!(out[0], Command::StopSpeech);
        // 1.02: min 5 m; NGP: 3 to 5 m.
        assert_eq!(out[1], Command::ShotAt { shot: Shot::BirdHit, gain: 5.0 / 6.0, pan: 1.0 });
        scripted.ngp = true;
        let out = scripted.step(&Scene::default(), &events);
        assert_eq!(out[1], Command::ShotAt { shot: Shot::BirdHit, gain: 0.6, pan: 1.0 });
    }
}
