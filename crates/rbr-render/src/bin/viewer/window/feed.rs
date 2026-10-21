//! The live drive's input (`rbr_render::live::Sender`): a thread polls the devices at the bindings' input rate (1000
//! Hz by default), as NGP's input thread does (every 1.1 ms in RSF), and sends the input when it changes, and at least
//! every [`KEEPALIVE`], so the stepping process steps with the newest input whatever the frame rate. The poll is the
//! game's (`rbr_render::devices::Input`: the keyboard and the bound joysticks through `Tic`). The frame loop posts the
//! held keys and its controls ([`Feed::post`]): their flags go once, their gear and the shifter's choice stand, and the
//! thread puts the poll's axes, pedals, flags and gear actions over them. It reads back each action's device input
//! and the last polled controls ([`Feed::polled`]) for its presses and the H-shifter.

use crate::latency::Script;
use rbr_render::bindings::{Bindings, ACTIONS};
use rbr_render::devices::{keyboard_state, Input};
use rbr_render::live::{Controls, Sender};
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use winit::keyboard::KeyCode;

/// The longest time between two sends of the same input, and how often a lost device is looked for.
const KEEPALIVE: Duration = Duration::from_millis(50);

/// The input thread.
pub struct Feed {
    shared: Arc<Mutex<State>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

/// What the frame loop and the thread give each other.
#[derive(Default)]
struct State {
    sender: Option<Sender>,
    /// The frame's controls, and their flags not sent yet.
    base: Controls,
    flags: u32,
    /// The held keys as the keyboard's device state.
    keys: Option<Box<[u8; 256]>>,
    /// The physics is NGP's: ignition and the direct gears are the poll's.
    ngp: bool,
    /// The advanced H-shifter selects: no direct gear actions.
    shifter: bool,
    /// The last poll: each action's device input and the controls.
    pads: [f32; ACTIONS],
    polled: Controls,
    /// Each send (when, the input's number, its steering), for `RBR_LATENCY`.
    sends: Option<Vec<(Instant, u32, f32)>>,
}

impl Feed {
    /// Starts the thread with the bindings, the scripted steering over the devices' (`RBR_LATENCY`), and with
    /// `keep_sends` each send kept for [`Feed::take_sends`].
    pub fn start(bindings: Bindings, script: Option<Script>, keep_sends: bool) -> Feed {
        let state = State { sends: keep_sends.then(Vec::new), keys: Some(Box::new([0; 256])), ..State::default() };
        let shared = Arc::new(Mutex::new(state));
        let stop = Arc::new(AtomicBool::new(false));
        let thread = {
            let (shared, stop) = (Arc::clone(&shared), Arc::clone(&stop));
            thread::spawn(move || run(&shared, &stop, &bindings, script))
        };
        Feed { shared, stop, thread: Some(thread) }
    }

    /// Sends to `sender` from now on (a live process started, or started over).
    pub fn attach(&self, sender: Sender) {
        self.shared.lock().unwrap().sender = Some(sender);
    }

    /// The frame's controls: their flags are sent once (and cleared here), the rest stand until the next post.
    pub fn post(&self, controls: &mut Controls, held: &HashSet<KeyCode>, ngp: bool, shifter: bool) {
        let mut state = self.shared.lock().unwrap();
        state.flags |= std::mem::take(&mut controls.flags);
        (state.base, state.ngp, state.shifter) = (*controls, ngp, shifter);
        if let Some(keys) = state.keys.as_mut() {
            keyboard_state(held, keys);
        }
    }

    /// The last poll's device input per action (0..1) and its controls.
    pub fn polled(&self) -> ([f32; ACTIONS], Controls) {
        let state = self.shared.lock().unwrap();
        (state.pads, state.polled)
    }

    /// The sends since the last call.
    pub fn take_sends(&self) -> Vec<(Instant, u32, f32)> {
        self.shared.lock().unwrap().sends.as_mut().map(std::mem::take).unwrap_or_default()
    }
}

impl Drop for Feed {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// The thread: a poll every period, sent when it differs from the last send, has flags or the keepalive is due.
fn run(shared: &Mutex<State>, stop: &AtomicBool, bindings: &Bindings, script: Option<Script>) {
    // Opened here: off Linux the devices' context stays on its thread.
    let mut input = Input::open(bindings);
    input.names().iter().for_each(|n| eprintln!("viewer: controls from {n}"));
    input.missing.iter().for_each(|e| eprintln!("viewer: controls: {e}"));
    let period = Duration::from_secs(1) / bindings.input_rate.max(1);
    let (mut seq, mut last, mut sent, mut looked) = (0, None, Instant::now(), Instant::now());
    let mut keys = [0u8; 256];
    let mut pads = [0.0; ACTIONS];
    while !stop.load(Ordering::Relaxed) {
        let start = Instant::now();
        let (sender, base, ngp, shifter) = {
            let mut state = shared.lock().unwrap();
            let flags = std::mem::take(&mut state.flags);
            keys.copy_from_slice(state.keys.as_deref().unwrap_or(&[0; 256]));
            (state.sender.clone(), Controls { flags, ..state.base }, state.ngp, state.shifter)
        };
        let mut controls = input.controls(&keys, base, ngp, &mut pads);
        if shifter {
            (controls.gears_held, controls.gears_bound) = (0, 0);
        }
        let now = Instant::now();
        if let Some(script) = script {
            (controls.steering, controls.keys_steer) = (script.steer(now), false);
        }
        let quiet = Controls { flags: 0, ..controls };
        let mut send = None;
        if let Some(sender) = &sender {
            if controls.flags != 0 || last != Some(quiet) || now - sent >= KEEPALIVE {
                seq += 1;
                // A closed process sends nothing: the frame loop attaches the next.
                send = sender.input(seq, &controls).is_ok().then_some((now, seq, controls.steering));
                (last, sent) = (Some(quiet), now);
            }
        }
        {
            let mut state = shared.lock().unwrap();
            (state.pads, state.polled) = (pads, controls);
            if let (Some(sends), Some(send)) = (state.sends.as_mut(), send) {
                sends.push(send);
            }
        }
        if now - looked >= KEEPALIVE {
            input.reopen();
            looked = now;
        }
        thread::sleep(period.saturating_sub(start.elapsed()));
    }
}
