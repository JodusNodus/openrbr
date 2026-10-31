//! `--live` (ours, `rbr_recording::realtime`): the game on this process's clock, one frame per physics step
//! of wall time (1/720 s under NGP, 7 ms under 1.02), each with the newest input the viewer sent, as the original runs
//! a frame per step at a high frame rate. A frame that wakes late takes the steps it missed through the game's frame
//! clock (`Input::frames`), which cuts it to 0.1 s; the clock does not catch up past that. After every frame that
//! stepped a [`Step`] record gives the viewer the force and the car's control, and every 7 ms of game time an answer
//! gives it the frames' picture (`Live::answer_bytes`).
//!
//! A frame is [`Live::run`] of an input payload with the periods it stands for. `--frames-out` logs them, and
//! `--frames` ([`serve_frames`]) runs frames from a log or a tool instead of the clock, the same way.

use std::io::{ErrorKind, Read, Write};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use rbr_gameplay::ffb::racing_gate;
use rbr_hybrid::pure::{self, CAR_PTR};
use rbr_physics::car_body::steering_ffb;
use rbr_recording::input::{Input, Payload};
use rbr_recording::realtime::{
    self, Logged, Message, Step, INPUT, LOGGED_BYTES, LOG_MAGIC, MESSAGE_BYTES, PAUSE, RESTART, RESUME,
};
use rbr_recording::{CONTROL_BYTES, STEP_SECONDS};

use super::Live;
use crate::autopilot::Pilot;

/// Action flag 4: select the input's gear (`CDrive::M6840`); the gear goes with it.
const SELECT_GEAR: u8 = 4;

/// What the viewer sent so far.
#[derive(Default)]
struct Feed {
    /// The newest input and its number.
    input: Input,
    seq: u32,
    /// The action flags since a frame last took them, ORed (each is one press), and the gear the last select asked for.
    flags: u8,
    gear: u8,
    /// The first input came.
    first: bool,
    /// Between a RESUME and a PAUSE, and the RESUMEs so far.
    running: bool,
    resumes: u64,
    /// The viewer asked for a restart (`RESTART`), taken by the clock loop.
    restart: bool,
    /// The input ended (`Ok`) or broke.
    end: Option<Result<(), String>>,
}

type Shared = Arc<(Mutex<Feed>, Condvar)>;

/// Reads the viewer's messages into `shared` until the input ends.
fn read_feed(mut input: impl Read, shared: Shared) {
    let mut bytes = [0; MESSAGE_BYTES];
    loop {
        let message = match input.read_exact(&mut bytes) {
            Ok(()) => Message::read(&bytes),
            Err(e) if e.kind() == ErrorKind::UnexpectedEof => Err(String::new()),
            Err(e) => Err(e.to_string()),
        };
        let (lock, wake) = &*shared;
        let mut feed = lock.lock().unwrap();
        let input = message.and_then(|m| match (m.tag, Payload::read(&m.payload)) {
            (INPUT, Ok(Payload::Input(input))) => Ok(Some((m.seq, input))),
            (INPUT, _) => Err("a real-time input that is not an input payload".to_owned()),
            (PAUSE, _) => Ok(None).inspect(|_| feed.running = false),
            (RESUME, _) => Ok(None).inspect(|_| (feed.running, feed.resumes) = (true, feed.resumes + 1)),
            (RESTART, _) => Ok(None).inspect(|_| feed.restart = true),
            _ => unreachable!("Message::read takes the four tags only"),
        });
        match input {
            Ok(Some((seq, input))) => {
                feed.flags |= input.flags;
                if input.flags & SELECT_GEAR != 0 {
                    feed.gear = input.gear;
                }
                (feed.input, feed.seq, feed.first) = (input, seq, true);
            }
            Ok(None) => {}
            Err(e) => feed.end = Some(if e.is_empty() { Ok(()) } else { Err(e) }),
        }
        let ended = feed.end.is_some();
        drop(feed);
        wake.notify_all();
        if ended {
            return;
        }
    }
}

/// Serves the real-time protocol: the first input is answered with the first frame, and from the first RESUME the
/// clock runs the frames until the input ends. `pilot` drives instead of the viewer's input; `log` gets every frame
/// ([`Logged`]).
pub fn serve_realtime<'a>(
    mut live: Live<'a>,
    input: impl Read + Send + 'static,
    mut output: impl Write,
    mut pilot: Option<Pilot<'a>>,
    mut log: Option<Box<dyn Write>>,
) -> Result<Live<'a>, String> {
    let shared: Shared = Arc::default();
    let reader = Arc::clone(&shared);
    thread::spawn(move || read_feed(input, reader));
    let (lock, wake) = &*shared;
    let write = |out: &mut dyn Write, bytes: &[u8]| out.write_all(bytes).and_then(|()| out.flush());
    let mut feed = wake.wait_while(lock.lock().unwrap(), |f| !f.first && f.end.is_none()).unwrap();
    if let Some(end) = feed.end.take() {
        return end.map(|()| live);
    }
    let mut payload = feed.input.to_bytes();
    drop(feed);
    if let Some(own) = pilot.as_mut().and_then(|p| p.payload(&live, None)) {
        payload = own;
    }
    let mut last = live.answer(&payload)?;
    if let Some(log) = log.as_mut() {
        log.write_all(&LOG_MAGIC).map_err(|e| e.to_string())?;
    }
    logged(&mut log, &live, payload, 1, live.next_step() - live.start)?;
    write(&mut output, &last).map_err(|e| e.to_string())?;
    let step = f64::from(live.clock.step);
    let period = Duration::from_secs_f64(step);
    let mut stats = Stats::default();
    let mut answering = Answering::new(&live);
    loop {
        let feed = wake.wait_while(lock.lock().unwrap(), |f| !f.running && f.end.is_none()).unwrap();
        if feed.end.is_some() {
            break;
        }
        let resumes = feed.resumes;
        drop(feed);
        let (mut origin, mut ticks) = (Instant::now(), 0u64);
        loop {
            let due = origin + period.mul_f64((ticks + 1) as f64);
            thread::sleep(due.saturating_duration_since(Instant::now()));
            let woke = Instant::now();
            stats.late.push((woke - due).as_secs_f32() * 1000.0);
            let missed = (((woke - origin).as_secs_f64() / step) as u64).max(ticks + 1) - ticks;
            ticks += missed;
            let (input, seq) = {
                let mut feed = lock.lock().unwrap();
                if feed.end.is_some() || !feed.running || feed.resumes != resumes {
                    break;
                }
                let flags = std::mem::take(&mut feed.flags);
                (
                    Input {
                        flags,
                        gear: if flags & SELECT_GEAR != 0 { feed.gear } else { feed.input.gear },
                        ..feed.input
                    },
                    feed.seq,
                )
            };
            if std::mem::replace(&mut lock.lock().unwrap().restart, false) {
                // The stage starts over in process, answered with its first frame like the first RESUME's.
                live.restart();
                if let Some(log) = log.as_mut() {
                    log.write_all(&Logged::RESTART.to_bytes()).map_err(|e| e.to_string())?;
                }
                (origin, ticks) = (Instant::now(), 0);
                let mut payload = Input { frames: 0, ..input }.to_bytes();
                if let Some(own) = pilot.as_mut().and_then(|p| p.payload(&live, None)) {
                    payload = own;
                }
                last = live.answer(&payload)?;
                answering = Answering::new(&live);
                logged(&mut log, &live, payload, 1, live.next_step() - live.start)?;
                write(&mut output, &last).map_err(|e| e.to_string())?;
                stats.answers += 1;
                continue;
            }
            let periods = missed.min(u64::from(u8::MAX)) as u32;
            let mut payload = Input { frames: if periods == 1 { 0 } else { periods as u8 }, ..input }.to_bytes();
            if let Some(own) = pilot.as_mut().and_then(|p| p.payload(&live, Some(&last))) {
                payload = own;
            }
            let (_, substeps) = live.run(&payload, periods, false)?;
            logged(&mut log, &live, payload, periods, substeps)?;
            stats.frames += 1;
            stats.steps += u64::from(substeps);
            stats.long += u64::from(periods > 1);
            if substeps > 0 {
                write(&mut output, &step_record(&live, seq).to_bytes()).map_err(|e| e.to_string())?;
            }
            if let Some(answer) = answering.after(&mut live, periods, input) {
                last = answer;
                write(&mut output, &last).map_err(|e| e.to_string())?;
                stats.answers += 1;
            }
        }
    }
    eprintln!("rbr-standalone: {}", stats.summary(step));
    let end = lock.lock().unwrap().end.take();
    end.unwrap_or(Ok(())).map(|()| live)
}

/// The fraction of a 7 ms answer period that counts as whole: the steps' f64 sum misses 7 ms by an ulp.
const ANSWER_SLACK: f64 = 1e-6;

/// The answers of a clocked run: one per 7 ms of game time, standing for the frames since the last.
struct Answering {
    /// Game time not answered yet, s.
    unanswered: f64,
    /// The step the next answer starts at.
    from: u32,
}

impl Answering {
    /// From the frame `live` answered last.
    fn new(live: &Live) -> Answering {
        Answering { unanswered: 0.0, from: live.next_step() }
    }

    /// After a frame of `periods` steps' time that took `input`: the answer, once the time makes up 7 ms (the input
    /// echoed with the answer's periods, more than one after a stall).
    fn after(&mut self, live: &mut Live, periods: u32, input: Input) -> Option<Vec<u8>> {
        self.unanswered += f64::from(periods) * f64::from(live.clock.step);
        let answers = (self.unanswered / f64::from(STEP_SECONDS) + ANSWER_SLACK) as u32;
        if answers == 0 {
            return None;
        }
        self.unanswered = (self.unanswered - f64::from(answers) * f64::from(STEP_SECONDS)).max(0.0);
        live.picture();
        let echo = Input { flags: 0, frames: if answers == 1 { 0 } else { answers.min(255) as u8 }, ..input };
        let answer = live.answer_bytes(&echo.to_bytes(), self.from, live.next_step() - self.from);
        self.from = live.next_step();
        Some(answer)
    }
}

/// The step record after a frame: the force the wheel gets (through the racing gate) and the car's control now, and
/// the input `seq` the frame took.
fn step_record(live: &Live, seq: u32) -> Step {
    let mem = &live.playback.pure.mem;
    let force = steering_ffb::force(mem, mem.u32(CAR_PTR));
    Step { step: live.next_step(), seq, force: racing_gate(force, live.game.stage().state()), control: live.control }
}

/// Logs a frame to `log`, with the digest of the car and wheels after it.
fn logged(
    log: &mut Option<Box<dyn Write>>,
    live: &Live,
    payload: [u8; CONTROL_BYTES],
    periods: u32,
    substeps: u32,
) -> Result<(), String> {
    let Some(log) = log.as_mut() else { return Ok(()) };
    let digest = realtime::digest(&pure::frame_bytes(&live.playback.pure.mem));
    log.write_all(&Logged { payload, periods, substeps, digest }.to_bytes()).map_err(|e| e.to_string())
}

/// `--frames`: the frames of `input` (a [`LOG_MAGIC`] stream) instead of the clock, each run as the clock runs it
/// ([`Live::run`], with its picture) and answered on `output`; a frame from `--frames-out` is compared with the drive's
/// substeps and digest. Returns the run and the comparison's line; an error at the first frame that differs.
pub fn serve_frames<'a>(
    mut live: Live<'a>,
    mut input: impl Read,
    mut output: impl Write,
    mut pilot: Option<Pilot<'a>>,
) -> Result<(Live<'a>, String), String> {
    let mut last: Option<Vec<u8>> = None;
    let mut magic = [0; 4];
    match input.read_exact(&mut magic) {
        Err(e) if e.kind() == ErrorKind::UnexpectedEof => return Ok((live, "no frames".to_owned())),
        r => r.map_err(|e| e.to_string())?,
    }
    if magic != LOG_MAGIC {
        return Err("--frames: not a frame stream (RTF1)".to_owned());
    }
    let (mut frames, mut checked, mut steps) = (0, 0, 0);
    // The frame that starts the stage: the first, and the first after a restart.
    let mut first = true;
    let mut bytes = [0; LOGGED_BYTES];
    loop {
        match input.read_exact(&mut bytes) {
            Err(e) if e.kind() == ErrorKind::UnexpectedEof => break,
            r => r.map_err(|e| e.to_string())?,
        }
        let mut frame = Logged::from_bytes(&bytes);
        if frame.is_restart() {
            live.restart();
            first = true;
            continue;
        }
        // `--pilot` replaces each frame's payload with its own (screenshots of a drive nobody plays).
        if let Some(own) = pilot.as_mut().and_then(|p| p.payload(&live, last.as_deref())) {
            frame.payload = own;
        }
        let (pre, substeps) = live.run(&frame.payload, frame.periods, true)?;
        // The first frame is the snapshot's own call, whose steps count from the image's.
        let substeps = if std::mem::take(&mut first) { live.next_step() - live.start } else { substeps };
        if frame.checked() {
            let digest = realtime::digest(&pure::frame_bytes(&live.playback.pure.mem));
            if (substeps, digest) != (frame.substeps, frame.digest) {
                return Err(format!(
                    "frame {frames} (step {pre}): {substeps} substeps, digest {digest:#018x}; the drive's {} \
                     substeps, {:#018x}",
                    frame.substeps, frame.digest
                ));
            }
            checked += 1;
        }
        let answer = live.answer_bytes(&frame.payload, pre, live.next_step() - pre);
        output.write_all(&answer).and_then(|()| output.flush()).map_err(|e| e.to_string())?;
        last = Some(answer);
        (frames, steps) = (frames + 1, steps + substeps);
    }
    Ok((live, format!("{frames} frames ({steps} steps), {checked} compared, all equal")))
}

/// How `--replay` runs the drive's frames.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Pace {
    /// On the clock, as the drive ran them: the viewer watches.
    #[default]
    Clock,
    /// Back to back, as fast as the answers are read (a video's offline render); the output ends with the frames.
    Free,
}

/// `--replay`: serves the real-time protocol with a replay's `frames` (`rbr_recording::replay`, the frames of its last
/// start) in place of the viewer's input, each run as the clock ran it and compared with the drive's substeps and
/// digest. The viewer's PAUSE and RESUME stop and go on, RESTART starts the replay over; at its end the clock waits
/// for a RESTART or the input's end, or under [`Pace::Free`] the run ends. Returns the run and the comparison's line;
/// an error at the first frame that differs.
pub fn serve_replay<'a>(
    mut live: Live<'a>,
    frames: &[Logged],
    input: impl Read + Send + 'static,
    mut output: impl Write,
    pace: Pace,
) -> Result<(Live<'a>, String), String> {
    let (first, rest) = frames.split_first().ok_or("the replay has no frames")?;
    let shared: Shared = Arc::default();
    let reader = Arc::clone(&shared);
    thread::spawn(move || read_feed(input, reader));
    let (lock, wake) = &*shared;
    let write = |out: &mut dyn Write, bytes: &[u8]| out.write_all(bytes).and_then(|()| out.flush());
    let compare = |live: &Live, at: usize, frame: &Logged, substeps: u32| {
        let digest = realtime::digest(&pure::frame_bytes(&live.playback.pure.mem));
        if !frame.checked() || (substeps, digest) == (frame.substeps, frame.digest) {
            return Ok(());
        }
        Err(format!(
            "the replay differs at frame {at}: {substeps} substeps, digest {digest:#018x}; the drive's {} substeps, \
             {:#018x} (another build, install, setup or profile than the drive's)",
            frame.substeps, frame.digest
        ))
    };
    // The stage's start: the first frame, answered before the clock runs.
    let start = |live: &mut Live<'a>, output: &mut dyn Write| -> Result<Answering, String> {
        let answer = live.answer(&first.payload)?;
        compare(live, 0, first, live.next_step() - live.start)?;
        write(output, &answer).map_err(|e| e.to_string())?;
        Ok(Answering::new(live))
    };
    let feed = wake.wait_while(lock.lock().unwrap(), |f| !f.first && f.end.is_none()).unwrap();
    if feed.end.is_some() {
        drop(feed);
        return Ok((live, "not watched".to_owned()));
    }
    drop(feed);
    let mut answering = start(&mut live, &mut output)?;
    let period = Duration::from_secs_f64(f64::from(live.clock.step));
    let (mut next, mut steps) = (0, 0u64);
    'run: loop {
        let feed = wake.wait_while(lock.lock().unwrap(), |f| !f.running && f.end.is_none()).unwrap();
        if feed.end.is_some() {
            break;
        }
        let resumes = feed.resumes;
        drop(feed);
        let (mut origin, mut ticks) = (Instant::now(), 0u64);
        loop {
            if std::mem::take(&mut lock.lock().unwrap().restart) {
                live.restart();
                answering = start(&mut live, &mut output)?;
                (next, steps, origin, ticks) = (0, 0, Instant::now(), 0);
            }
            let Some(frame) = rest.get(next) else {
                if pace == Pace::Free {
                    break 'run;
                }
                // The end: the last answer stays until a restart, a pause or the input's end.
                let feed = wake
                    .wait_while(lock.lock().unwrap(), |f| {
                        f.end.is_none() && f.running && !f.restart && f.resumes == resumes
                    })
                    .unwrap();
                if feed.end.is_some() || !feed.running || feed.resumes != resumes {
                    continue 'run;
                }
                drop(feed);
                (origin, ticks) = (Instant::now(), 0);
                continue;
            };
            ticks += u64::from(frame.periods);
            if pace == Pace::Clock {
                thread::sleep((origin + period.mul_f64(ticks as f64)).saturating_duration_since(Instant::now()));
            }
            {
                let mut feed = lock.lock().unwrap();
                if feed.end.is_some() || !feed.running || feed.resumes != resumes {
                    continue 'run;
                }
                if std::mem::take(&mut feed.restart) {
                    drop(feed);
                    live.restart();
                    answering = start(&mut live, &mut output)?;
                    (next, steps, origin, ticks) = (0, 0, Instant::now(), 0);
                    continue;
                }
            }
            let (_, substeps) = live.run(&frame.payload, frame.periods, false)?;
            compare(&live, next + 1, frame, substeps)?;
            (next, steps) = (next + 1, steps + u64::from(substeps));
            if substeps > 0 {
                write(&mut output, &step_record(&live, 0).to_bytes()).map_err(|e| e.to_string())?;
            }
            let input = match Payload::read(&frame.payload) {
                Ok(Payload::Input(input)) => input,
                _ => Input::default(),
            };
            if let Some(answer) = answering.after(&mut live, frame.periods, input) {
                write(&mut output, &answer).map_err(|e| e.to_string())?;
            }
        }
    }
    Ok((live, format!("{} of {} frames ({steps} steps) run, all equal", next + 1, frames.len())))
}

/// The clock's figures, printed at the end.
#[derive(Default)]
struct Stats {
    frames: u64,
    steps: u64,
    /// Frames that stood for more than one step's time (a late wake).
    long: u64,
    answers: u64,
    /// Each wake's lateness, ms.
    late: Vec<f32>,
}

impl Stats {
    fn summary(&mut self, step: f64) -> String {
        self.late.sort_by(f32::total_cmp);
        let at = |q: f64| self.late.get(((self.late.len().max(1) - 1) as f64 * q) as usize).copied().unwrap_or(0.0);
        format!(
            "real time: {} frames of {:.3} ms, {} steps, {} frames of more than one step's time, {} answers; wake \
             late p50 {:.3} ms, p99 {:.3}, max {:.3}",
            self.frames,
            step * 1000.0,
            self.steps,
            self.long,
            self.answers,
            at(0.5),
            at(0.99),
            at(1.0)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    /// A RESTART message flags the restart without touching the clock or the input: the clock loop takes it.
    #[test]
    fn restart_message_flags_the_restart() {
        let input = Input::default().to_bytes();
        let mut bytes = Vec::new();
        for tag in [INPUT, PAUSE, RESUME, RESTART] {
            bytes.extend_from_slice(&Message { tag, seq: 7, payload: input }.to_bytes());
        }
        let shared: Shared = Arc::default();
        read_feed(Cursor::new(bytes), Arc::clone(&shared));
        let feed = shared.0.lock().unwrap();
        assert!(feed.first);
        assert!(feed.running);
        assert_eq!(feed.resumes, 1);
        assert!(feed.restart);
    }
}
