//! The stage's state machine: the application state the frame loop switches on (`CApplication` +0x728), from the quick
//! race's set-up through the load settle and the countdown to driving and the end of the race, and the car requests the
//! payload's flag bits make. Pure: the host gives the events (the race host is ready, the camera's go, the finish and
//! end keys, the substeps of each settle call); the functions return the next value. The arithmetic is the assembly's;
//! `README.md` ("Stage (G7)") says what is not ported and what is ours.
//!
//! Original: `CApplication::StartQuickRace` (0x481660) sets state 5; `CApplication::M004801d0` (0x4801d0) waits in 5
//! for the race host, starts the load countdown (`m_unk72c` = 0x96) and state 0xd, runs the settle calls in 0xd and
//! moves to 10; the camera's `E15CamHost::M8840` calls `M0047ca40` (0x47ca40) to give the go; `CApplication::Tic`
//! (0x4804f0) counts the countdown down in state 10 and starts driving (1).

/// The states of `CApplication` +0x728 the stage passes through (the original's numbers).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// 1: driving; `Tic` runs `TicPhysicsRecording` each frame.
    Driving,
    /// 2: the pause menu (`M0047fc90` from 1; `M0047f9c0` resumes to 1). The HUD's timers wait in it.
    Menu,
    /// 5: set up by `StartQuickRace`; waits for the race host to be ready.
    Setup,
    /// 0xd: the load; each frame runs its settle steps until the load countdown is spent.
    Load,
    /// 10: the countdown; the go starts the driving.
    Countdown,
    /// 9: the race is over and its end page shows (`CApplication::M0047f3a0`, 0x47f3a0, from the `CSigEndRace`
    /// receiver `M00480810`). `Tic` runs no physics in it: the car stops being stepped.
    Ended,
}

impl State {
    /// The original's value of +0x728.
    pub fn value(self) -> u32 {
        match self {
            State::Driving => 1,
            State::Menu => 2,
            State::Setup => 5,
            State::Load => 0xd,
            State::Countdown => 10,
            State::Ended => 9,
        }
    }
}

/// The load countdown (`m_unk72c`, in substeps) when the load starts: 0x96 (`M004801d0` case 5).
pub const LOAD_SUBSTEPS: u32 = 0x96;
/// NGP 7.4.778's load countdown: its patch of that immediate (0x482bc0) to 0x5dc.
pub const NGP_LOAD_SUBSTEPS: u32 = 0x5dc;
/// The countdown (`m_unk714`) at the go (`M0047ca40`): the next `Tic` takes the frame's `dt` off it.
const GO_VALUE: f32 = -1.0;
/// Payload flag bit 0x10: the call for help (`CCar::UpdateSeed`'s motion probe).
const CALL_FOR_HELP_FLAG: u32 = 0x10;
/// Payload flag bit 0x20: a reset (`CCar::UpdateSeed`'s reset branch, which takes the bit over 0x10). Only
/// `CSigRespawnRequest(2)` sets it, and nothing in the exe emits that.
const RESET_FLAG: u32 = 0x20;

/// The stage's state: the quick race's set-up and the state the frame loop is in.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stage {
    state: State,
    track: u32,
    car: u32,
    load_left: u32,
    countdown: f32,
    armed: bool,
}

/// What the payload's flag bits ask of the car.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CarRequest {
    None,
    /// Flag 0x10, the pause menu's call for help. The original's motion probe (`CCar::MotionProbe`, 0x4e7730)
    /// decides: a car faster than 1.5 m/s, or one its surface sampler gives a verdict for, gets `CSigCarProblem` (a
    /// HUD message; MSG_CARSTOP when too fast), any other the assist (`CCar::Assist8DF0`, 0x4e8df0), which puts it
    /// back on the road 0.5 s later.
    CallForHelp,
    /// Flag 0x20.
    Reset,
}

impl Stage {
    /// `StartQuickRace` (0x481660) for `track` (`m_unk730`, the map's id) and `car` (`m_unk734`): state 5, with the
    /// countdown cleared.
    pub fn quick_race(track: u32, car: u32) -> Self {
        Self { state: State::Setup, track, car, load_left: 0, countdown: 0.0, armed: false }
    }

    /// The race under way (state 1), for a boot from a snapshot taken mid-drive: the load and the countdown are over
    /// before it, so none runs.
    pub fn driving(track: u32, car: u32) -> Self {
        Self { state: State::Driving, track, car, load_left: 0, countdown: 0.0, armed: false }
    }

    pub fn state(self) -> State {
        self.state
    }

    pub fn track(self) -> u32 {
        self.track
    }

    pub fn car(self) -> u32 {
        self.car
    }

    /// The load countdown left, in substeps (`m_unk72c`).
    pub fn load_left(self) -> u32 {
        self.load_left
    }

    /// The countdown (`m_unk714`).
    pub fn countdown(self) -> f32 {
        self.countdown
    }

    /// Whether the countdown is armed (`m_unk718`).
    pub fn armed(self) -> bool {
        self.armed
    }

    /// The race host reports ready in state 5: the load starts with [`LOAD_SUBSTEPS`] to spend.
    pub fn load_ready(self) -> Self {
        self.load_ready_with(LOAD_SUBSTEPS)
    }

    /// [`Self::load_ready`] with `substeps` to spend ([`NGP_LOAD_SUBSTEPS`] under NGP).
    pub fn load_ready_with(self, substeps: u32) -> Self {
        match self.state {
            State::Setup => Self { state: State::Load, load_left: substeps, ..self },
            _ => self,
        }
    }

    /// One frame of the load, after its `substeps` settle steps ran (`M004801d0` case 0xd). The countdown spends them
    /// (keeping 0 once spent); at 0 the countdown starts (state 10). The original also waits for the sound host, which
    /// is not modelled.
    pub fn load_tick(self, substeps: u32) -> Self {
        match self.state {
            State::Load => {
                let load_left = self.load_left.saturating_sub(substeps);
                let state = if load_left == 0 { State::Countdown } else { State::Load };
                Self { state, load_left, ..self }
            }
            _ => self,
        }
    }

    /// The camera's go (`M0047ca40`): arms the countdown in state 10.
    pub fn go(self) -> Self {
        match self.state {
            State::Countdown => Self { countdown: GO_VALUE, armed: true, ..self },
            _ => self,
        }
    }

    /// `Tic` in state 10: once armed, takes the frame's `dt` off the countdown; below 0 the countdown disarms and the
    /// driving starts (1).
    pub fn countdown_tick(self, dt: f32) -> Self {
        if self.state != State::Countdown || !self.armed {
            return self;
        }
        let countdown = self.countdown - dt;
        if countdown < 0.0 {
            Self { state: State::Driving, countdown, armed: false, ..self }
        } else {
            Self { countdown, ..self }
        }
    }

    /// RESTART (the pause menu's and the retire page's, callbacks 0x56a600/0x59b6b0): after its confirm box (0x58dc60,
    /// message 0x3b) `CApplication::SetUnk724_3` leaves action 3, which `M004825b0` runs after the fade:
    /// `InitQuickRace` (0x481d00) calls `StartQuickRace` again with the menu's track and car, so the stage is set up
    /// from state 5 as at the first start. Nothing of the driven race is kept (`README.md`, Restart).
    pub fn restart(self) -> Self {
        Self::quick_race(self.track, self.car)
    }

    /// A `CSigEndRace` while driving, or from a message box's callback in the pause: its receiver
    /// (`CApplication::M00480810`) moves to the end page (state 9, `M0047f3a0`) in the same call, so the frame that
    /// emitted it is the last one the car is stepped in.
    pub fn race_over(self) -> Self {
        match self.state {
            State::Driving | State::Menu => Self { state: State::Ended, ..self },
            _ => self,
        }
    }

    /// `CSigMenuActivate` while driving (`CApplication::M0047f9c0`, 0x47f9c0): the pause menu's page opens and the
    /// state is 2, where `Tic` runs no physics.
    pub fn pause(self) -> Self {
        match self.state {
            State::Driving => Self { state: State::Menu, ..self },
            _ => self,
        }
    }
}

/// What the payload's flags ask of the car (`CCar::UpdateSeed`, 0x4e91f0). Nothing is asked while the replay is in its
/// state 2 (`replay_state2`, `E19IsState2`), and the reset takes precedence over the car problem.
pub fn car_request(flags: u32, replay_state2: bool) -> CarRequest {
    if replay_state2 {
        CarRequest::None
    } else if flags & RESET_FLAG != 0 {
        CarRequest::Reset
    } else if flags & CALL_FOR_HELP_FLAG != 0 {
        CarRequest::CallForHelp
    } else {
        CarRequest::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The load's settle calls: four of 40 substeps, as the peek shows (the countdown 150, 110, 70, 30 before each).
    #[test]
    fn load_spends_the_countdown_in_four_settle_calls() {
        let mut stage = Stage::quick_race(54, 6).load_ready();
        assert_eq!(stage.state(), State::Load);
        assert_eq!(stage.load_left(), 150);
        for before in [150, 110, 70, 30] {
            assert_eq!(stage.load_left(), before);
            stage = stage.load_tick(40);
        }
        assert_eq!(stage.state(), State::Countdown);
        assert_eq!(stage.load_left(), 0);
    }

    #[test]
    fn load_ready_only_in_set_up() {
        let stage = Stage::quick_race(54, 6).load_ready();
        assert_eq!(stage.load_ready(), stage);
    }

    /// The go is needed, and the next tick below 0 starts the driving; a go outside the countdown does nothing.
    #[test]
    fn countdown_needs_the_go_then_a_tick_below_zero() {
        let counting = Stage::quick_race(54, 6).load_ready().load_tick(150);
        assert_eq!(counting.state(), State::Countdown);
        assert_eq!(counting.countdown_tick(1.0), counting);
        let armed = counting.go();
        assert!(armed.armed());
        assert_eq!(armed.countdown(), -1.0);
        let driving = armed.countdown_tick(0.0);
        assert_eq!(driving.state(), State::Driving);
        assert!(!driving.armed());
        assert_eq!(driving.countdown(), -1.0);
        assert!(!Stage::quick_race(54, 6).go().armed());
    }

    #[test]
    fn the_frame_time_is_taken_off_the_go_value() {
        let driving = Stage::quick_race(54, 6).load_ready().load_tick(150).go().countdown_tick(0.25);
        assert_eq!(driving.state(), State::Driving);
        assert_eq!(driving.countdown(), -1.25);
    }

    #[test]
    fn a_restart_sets_the_same_race_up_again() {
        let ended = Stage::driving(54, 6).race_over();
        assert_eq!(ended.restart(), Stage::quick_race(54, 6));
        assert_eq!(Stage::quick_race(54, 6).load_ready().restart(), Stage::quick_race(54, 6));
    }

    #[test]
    fn the_race_end_moves_to_the_end_page_only_while_driving_or_paused() {
        let driving = Stage::driving(54, 6);
        assert_eq!(driving.race_over().state(), State::Ended);
        assert_eq!(State::Ended.value(), 9);
        assert_eq!(Stage::quick_race(54, 6).race_over(), Stage::quick_race(54, 6));
        let paused = driving.pause();
        assert_eq!((paused.state(), paused.state().value()), (State::Menu, 2));
        assert_eq!(paused.race_over().state(), State::Ended);
        assert_eq!(Stage::quick_race(54, 6).pause(), Stage::quick_race(54, 6));
    }

    #[test]
    fn reset_takes_precedence_and_the_replay_gate_clears_requests() {
        assert_eq!(car_request(0x10, false), CarRequest::CallForHelp);
        assert_eq!(car_request(0x20, false), CarRequest::Reset);
        assert_eq!(car_request(0x30, false), CarRequest::Reset);
        assert_eq!(car_request(0x07, false), CarRequest::None);
        assert_eq!(car_request(0x30, true), CarRequest::None);
    }
}
