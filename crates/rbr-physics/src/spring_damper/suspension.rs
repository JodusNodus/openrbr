//! Owned suspension state and the complete per-strut update: the definition, the controller, the fade and the
//! effective damping. The caller supplies travel and current settings; nothing here accesses a memory image or a call
//! stack. The memory edge (`layout`) loads a `Suspension` from the strut's objects, runs one method, and stores it
//! back, so every dispatch path (bindings, host calls, pure leaves) runs this code.
//!
//! The step mirrors NGP's `CSpringDamper::Step` wrapper (`0x10059f70`): the bump rubber softens the definition for
//! this calculation alone, the high-speed latch overrides the effective damping and clears its break speed, the
//! ported `Step` runs on the adjusted definition, the definition is restored, and `UpdateForces` recomputes the
//! effective damping from the (possibly faded) fade state. The strut's scratch words (`+0x30/0x34/0x38`) and the
//! car's copies (`+0xe80/0xe84/0xe88/0xe8c`) capture the pre-step latch state; `Output` carries them as typed values
//! so the memory edge can store them without reimplementing the logic.

use super::store::SpringStore;
use super::{Controller, DamperSignal, Definition, FadeState, NgpStep, SetupValues, State, StepInput, StepOutput};
use crate::ngp::spring::{bump_stop, latch, rebound_scale, BumpRubber, Latch};

/// The three effective damping values the strut's state carries.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Damping {
    pub bump: f32,
    pub rebound: f32,
    pub high_speed_bump: f32,
}

impl Damping {
    pub fn from_state(state: State) -> Self {
        Self { bump: state.damp_bump_eff, rebound: state.damp_rebound_eff, high_speed_bump: state.hs_damp_bump_eff }
    }

    pub fn state(self) -> State {
        State {
            damp_bump_eff: self.bump,
            damp_rebound_eff: self.rebound,
            hs_damp_bump_eff: self.high_speed_bump,
            ..State::default()
        }
    }
}

/// Wheel settings that affect the bump rubber during this step: the progressive flag (NGP settings bit `0x20000`
/// with the game object's bit), the wheel definition's gap (`+0x41c`) and rebound ratio (`+0x1fc`).
#[derive(Clone, Copy, Debug)]
pub struct Environment {
    pub progressive: bool,
    pub gap_offset: f32,
    pub rebound_ratio: f32,
}

/// The pre-step values the original persists outside the definition and state: the strut's scratch words and the
/// car's copies. Both hold the latched state before the force calculation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Diagnostics {
    /// Strut `+0x30`: the bump engagement ratio (0 when the rubber is not engaged).
    pub bump_ratio: f32,
    /// Strut `+0x34`: the rubber stiffness for the step (the definition's when disengaged).
    pub bump_stiffness: f32,
    /// Strut `+0x38`: the recorded rubber damping (the definition's when disengaged).
    pub bump_damping: f32,
    /// Car `+0xe80/0xe84`: the latched effective bump and rebound damping.
    pub effective_damping: [f32; 2],
    /// Car `+0xe88/0xe8c`: the break speeds for the step (cleared when latched).
    pub break_speeds: [f32; 2],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Output {
    pub forces: StepOutput,
    pub diagnostics: Diagnostics,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Suspension {
    pub definition: Definition,
    pub controller: Controller,
    pub fade: FadeState,
    pub damping: Damping,
}

impl Suspension {
    /// Loads the strut's owned state from its controller, definition, state and fade objects.
    pub fn load<S: SpringStore>(mem: &S, controller_at: u32) -> Self {
        Self {
            definition: mem.definition(mem.definition_at(controller_at)),
            controller: mem.controller(controller_at),
            fade: mem.fade_state(mem.fade_at(controller_at)),
            damping: Damping::from_state(mem.state(mem.state_at(controller_at))),
        }
    }

    /// Stores the controller, fade and effective damping. The definition is untouched by the step; `adopt_setup`
    /// persists through [`store_definition`](Self::store_definition).
    pub fn store_state<S: SpringStore>(&self, mem: &mut S, controller_at: u32) {
        mem.store_step_state(controller_at, self.controller);
        mem.store_fade_state(mem.fade_at(controller_at), self.fade);
        let mut stored = mem.state(mem.state_at(controller_at));
        stored.damp_bump_eff = self.damping.bump;
        stored.damp_rebound_eff = self.damping.rebound;
        stored.hs_damp_bump_eff = self.damping.high_speed_bump;
        mem.store_effective_damping(mem.state_at(controller_at), stored);
    }

    /// Persists the definition fields `adopt_setup` mutates (the nine setup-adopted bodies).
    pub fn store_definition<S: SpringStore>(&self, mem: &mut S, controller_at: u32) {
        mem.store_setup_values(mem.definition_at(controller_at), &self.definition);
    }

    /// Adopts the nine setup values into the definition (`CopyStateToDef`, 0x52dbd0). Effective damping is left
    /// alone; `UpdateForces` recomputes it separately, as the original sequences them.
    pub fn adopt_setup(&mut self, setup: SetupValues) {
        super::apply_setup(&mut self.definition, setup);
    }

    pub fn update_damping(&mut self) {
        let mut state = self.damping.state();
        super::update_forces(self.fade, &self.definition, &mut state);
        self.damping = Damping::from_state(state);
    }

    /// `UpdateForces` then the reset without the signal-manager emit (`UpdateAndReset`, 0x52e060).
    pub fn update_and_reset(&mut self, owner_id: u32) -> DamperSignal {
        self.update_damping();
        self.reset_signal(owner_id)
    }

    pub fn reset_signal(&mut self, owner_id: u32) -> DamperSignal {
        super::reset_outputs(&mut self.controller, owner_id)
    }

    pub fn step(&mut self, input: StepInput, environment: Environment, tuning: NgpStep) -> Output {
        // Changes to the bump rubber and high-speed breakpoints apply to this calculation alone.
        let mut active = self.definition;
        let rubber = BumpRubber {
            pos: self.definition.bump_stop_pos.value,
            stiffness: self.definition.bump_stop_rate.value,
            damping: self.definition.stop_damp.value,
        };
        let bump = bump_stop(
            environment.progressive,
            rubber,
            input.coordinate,
            input.velocity,
            environment.gap_offset,
            environment.rebound_ratio,
        );
        let rebound = rebound_scale(self.definition.hs_damp_rebound.value, self.fade.amount);
        let latch = latch(
            self.definition.hs_break_bump.value,
            self.definition.hs_break_rebound.value,
            input.velocity,
            self.damping.high_speed_bump,
            rebound,
        );
        let mut diagnostics = Diagnostics {
            bump_ratio: 0.0,
            bump_stiffness: rubber.stiffness,
            bump_damping: rubber.damping,
            effective_damping: [self.damping.bump, self.damping.rebound],
            break_speeds: [self.definition.hs_break_bump.value, self.definition.hs_break_rebound.value],
        };
        if let Some(bump) = bump {
            active.bump_stop_rate.value = bump.stiffness;
            active.stop_damp.value = bump.damping;
            diagnostics.bump_ratio = bump.ratio;
            diagnostics.bump_stiffness = bump.stiffness;
            diagnostics.bump_damping = bump.damping_out;
        }
        if self.definition.hs_damp_rebound.value > 0.0 {
            active.hs_damp_rebound.value = rebound;
        }
        let mut state = self.damping.state();
        match latch {
            Latch::Bump(damping) => {
                state.damp_bump_eff = damping;
                active.hs_break_bump.value = 0.0;
                diagnostics.effective_damping[0] = damping;
                diagnostics.break_speeds[0] = 0.0;
            }
            Latch::Rebound(damping) => {
                state.damp_rebound_eff = damping;
                active.hs_break_rebound.value = 0.0;
                diagnostics.effective_damping[1] = damping;
                diagnostics.break_speeds[1] = 0.0;
            }
            Latch::None => {}
        }
        let forces = super::step(&mut self.controller, &mut self.fade, &active, &mut state, input, tuning);
        self.update_damping();
        Output { forces, diagnostics }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn definition() -> Definition {
        let mut def = Definition::default();
        def.bump_stop_pos.value = 0.1;
        def.bump_stop_rate.value = 2.0;
        def.stop_damp.value = 3.0;
        def.hs_break_bump.value = 1.0;
        def.hs_break_rebound.value = 1.0;
        def.hs_damp_rebound.value = 4.0;
        def.damp_bump.value = 1000.0;
        def.damp_rebound.value = 2000.0;
        def.hs_damp_bump.value = 3000.0;
        def
    }

    fn suspension() -> Suspension {
        Suspension {
            definition: definition(),
            controller: Controller::default(),
            fade: FadeState { test_multiplier: 1.0, amount: 0.0 },
            damping: Damping { bump: 1000.0, rebound: 2000.0, high_speed_bump: 5000.0 },
        }
    }

    fn input(coordinate: f32, velocity: f32) -> StepInput {
        StepInput { travel: 0.0, coordinate, velocity }
    }

    fn env() -> Environment {
        Environment { progressive: false, gap_offset: 0.0, rebound_ratio: 0.0 }
    }

    #[test]
    fn disengaged_rubber_keeps_definition_stiffness() {
        let mut s = suspension();
        let out = s.step(input(0.2, 0.0), env(), NgpStep { fade_threshold: 27_000.0, fade_scale: 1.0 });
        assert_eq!(out.diagnostics.bump_ratio, 0.0);
        assert_eq!(out.diagnostics.bump_stiffness, 2.0);
        assert_eq!(out.diagnostics.bump_damping, 3.0);
    }

    #[test]
    fn engaged_rubber_softens_for_the_step_only() {
        let mut s = suspension();
        let out = s.step(input(0.08, 1.0), env(), NgpStep { fade_threshold: 27_000.0, fade_scale: 1.0 });
        assert_eq!(
            (out.diagnostics.bump_ratio, out.diagnostics.bump_stiffness, out.diagnostics.bump_damping),
            (1.0, 2.0, 3.0)
        );
        // The owned definition is untouched: a disengaged step sees the original stiffness.
        let out = s.step(input(0.2, 0.0), env(), NgpStep { fade_threshold: 27_000.0, fade_scale: 1.0 });
        assert_eq!(out.diagnostics.bump_stiffness, 2.0);
        assert_eq!(s.definition.bump_stop_rate.value, 2.0);
    }

    #[test]
    fn latch_overrides_damping_and_clears_break_speed() {
        // Fast droop takes the scaled rebound damping and clears its break speed.
        let mut s = suspension();
        let out = s.step(input(0.2, 2.0), env(), NgpStep { fade_threshold: 27_000.0, fade_scale: 1.0 });
        assert_eq!(out.diagnostics.effective_damping[1], 4.0);
        assert_eq!(out.diagnostics.break_speeds[1], 0.0);
        // Fast bump takes the high-speed bump damping instead.
        let mut s = suspension();
        let out = s.step(input(0.2, -2.0), env(), NgpStep { fade_threshold: 27_000.0, fade_scale: 1.0 });
        assert_eq!(out.diagnostics.effective_damping[0], 5000.0);
        assert_eq!(out.diagnostics.break_speeds[0], 0.0);
    }

    #[test]
    fn setup_adoption_copies_nine_values_without_touching_damping() {
        let mut s = suspension();
        let mut setup = SetupValues::default();
        setup.spring_rate.value = 42.0;
        setup.damp_bump.value = 43.0;
        s.adopt_setup(setup);
        assert_eq!(s.definition.spring_rate.value, 42.0);
        assert_eq!(s.definition.damp_bump.value, 43.0);
        assert_eq!((s.damping.bump, s.damping.rebound, s.damping.high_speed_bump), (1000.0, 2000.0, 5000.0));
    }

    #[test]
    fn update_and_reset_recomputes_damping_and_returns_signal() {
        let mut s = suspension();
        s.fade.amount = 0.5;
        s.controller.signal.peak_velocity = 9.0;
        let emitted = s.update_and_reset(7);
        assert_eq!(s.damping.bump, (1.0 - 0.5 * 0.7) * 1000.0);
        assert_eq!(s.damping.rebound, (1.0 - 0.5 * 0.3) * 2000.0);
        assert_eq!((emitted.owner_id, emitted.peak_velocity), (0, 9.0));
        assert_eq!((s.controller.signal.owner_id, s.controller.signal.peak_velocity), (7, 0.0));
    }
}
