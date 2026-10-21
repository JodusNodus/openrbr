//! The live drive's input (`rbr_render::live::Sender`): a thread polls the devices every millisecond, as NGP's input
//! thread does (every 1.1 ms in RSF), and sends the input when it changes, and at least every [`KEEPALIVE`], so the
//! stepping process steps with the newest input whatever the frame rate. The frame loop still reads the keys, the
//! presses and the gear logic and posts its controls ([`Feed::post`]): their flags go once, their gear and the
//! shifter's choice stand, and the thread puts the devices' axes, pedals and buttons over them (the RSF poll under
//! NGP, else the bindings over the held keys and a gamepad state of its own).

use crate::latency::Script;
use gilrs::Gilrs;
use rbr_render::bindings::{Action, Bindings, ACTIONS};
use rbr_render::live::{Controls, Sender};
use rbr_render::rsf_input::RsfInput;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use winit::keyboard::KeyCode;

/// The poll's period.
const POLL: Duration = Duration::from_millis(1);
/// The longest time between two sends of the same input.
const KEEPALIVE: Duration = Duration::from_millis(50);

/// The input thread.
pub struct Feed {
    shared: Arc<Mutex<State>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

/// What the frame loop gives the thread.
#[derive(Default)]
struct State {
    sender: Option<Sender>,
    /// The frame's controls, and their flags not sent yet.
    base: Controls,
    flags: u32,
    held: HashSet<KeyCode>,
    /// The physics is NGP's: the RSF poll drives.
    ngp: bool,
    /// The advanced H-shifter selects: no direct gear actions.
    shifter: bool,
    /// Each send (when, the input's number, its steering), for `RBR_LATENCY`.
    sends: Option<Vec<(Instant, u32, f32)>>,
}

impl Feed {
    /// Starts the thread with the bindings and the RSF poll, the scripted steering over the devices' (`RBR_LATENCY`),
    /// and with `keep_sends` each send kept for [`Feed::take_sends`].
    pub fn start(bindings: Bindings, rsf: Option<RsfInput>, script: Option<Script>, keep_sends: bool) -> Feed {
        let shared = Arc::new(Mutex::new(State { sends: keep_sends.then(Vec::new), ..State::default() }));
        let stop = Arc::new(AtomicBool::new(false));
        let thread = {
            let (shared, stop) = (Arc::clone(&shared), Arc::clone(&stop));
            thread::spawn(move || run(&shared, &stop, &bindings, rsf, script))
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
        state.held.clone_from(held);
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

/// The thread: a poll every [`POLL`], sent when it differs from the last send, has flags or the keepalive is due.
fn run(
    shared: &Mutex<State>,
    stop: &AtomicBool,
    bindings: &Bindings,
    mut rsf: Option<RsfInput>,
    script: Option<Script>,
) {
    let mut gilrs = Gilrs::new().ok();
    let (mut seq, mut last, mut sent) = (0, None, Instant::now());
    while !stop.load(Ordering::Relaxed) {
        let start = Instant::now();
        let (sender, base, held, ngp, shifter) = {
            let mut state = shared.lock().unwrap();
            let flags = std::mem::take(&mut state.flags);
            (state.sender.clone(), Controls { flags, ..state.base }, state.held.clone(), state.ngp, state.shifter)
        };
        if let Some(sender) = sender {
            let mut controls = match rsf.as_mut().filter(|_| ngp) {
                Some(rsf) => rsf.controls(&held, base),
                None => {
                    let pads = gilrs.as_mut().map_or([0.0; ACTIONS], |gilrs| {
                        while gilrs.next_event().is_some() {}
                        bindings.read_pads(gilrs)
                    });
                    let keys = Action::ALL.map(|a| bindings.get(a).key.is_some_and(|k| held.contains(&k)));
                    let mut controls = bindings.controls(keys, pads, base);
                    if shifter {
                        (controls.gears_held, controls.gears_bound) = (0, 0);
                    }
                    controls
                }
            };
            let now = Instant::now();
            if let Some(script) = script {
                (controls.steering, controls.keys_steer) = (script.steer(now), false);
            }
            let quiet = Controls { flags: 0, ..controls };
            if controls.flags != 0 || last != Some(quiet) || now - sent >= KEEPALIVE {
                seq += 1;
                // A closed process sends nothing: the frame loop attaches the next.
                if sender.input(seq, &controls).is_ok() {
                    if let Some(sends) = shared.lock().unwrap().sends.as_mut() {
                        sends.push((now, seq, controls.steering));
                    }
                }
                (last, sent) = (Some(quiet), now);
            }
        }
        thread::sleep(POLL.saturating_sub(start.elapsed()));
    }
}
