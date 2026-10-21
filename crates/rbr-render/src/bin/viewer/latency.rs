//! The live drive's input latency (`RBR_LATENCY`, ours): when a change of the steering is read, sent in a payload,
//! answered, taken by a pump, drawn, presented and done on the GPU, and each drawn frame's own times, printed as a
//! summary when the window closes.
//!
//! `RBR_LATENCY=script` (or `script:MS`) replaces the steering with a square wave of ±[`SCRIPT_STEER`] that flips
//! every 500 ms (or MS): each flip is due at a known time, as a device event would arrive, so the first stage holds
//! the wait for the frame's input read. Any other value times the frames only. On Wayland the compositor's
//! presentation feedback gives when each frame went on screen ([`shown`]).

#[cfg(all(unix, not(target_vendor = "apple")))]
mod shown;
/// No feedback where the window is not a Wayland one.
#[cfg(not(all(unix, not(target_vendor = "apple"))))]
mod shown {
    #[derive(Debug, Clone, Copy, PartialEq)]
    #[allow(dead_code)]
    pub enum Feedback {
        Shown { at: u64, flags: u32 },
        Discarded,
    }
    pub const VSYNC: u32 = 1;
    pub const ZERO_COPY: u32 = 8;
    pub struct Shown;
    impl Shown {
        pub fn new(_: &winit::window::Window) -> Result<Shown, String> {
            Err("not a Wayland window".into())
        }
        pub fn ask(&mut self, _: u64) {}
        pub fn take(&mut self) -> Vec<(u64, Feedback)> {
            Vec::new()
        }
    }
    pub fn now_ns() -> u64 {
        0
    }
}

use rbr_render::live::Live;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use winit::window::Window;

/// The scripted steering's size.
const SCRIPT_STEER: f32 = 0.5;
/// Flips kept, at most (a summary over a long run keeps the first).
const MOST_PROBES: usize = 10_000;
const MOST_FRAMES: usize = 100_000;

/// One flip of the scripted steering and when each stage first had it.
#[derive(Debug, Clone, Copy)]
struct Probe {
    due: Instant,
    steer: f32,
    read: Instant,
    /// The first payload that carried it.
    payload: Option<u64>,
    sent: Option<Instant>,
    received: Option<Instant>,
    taken: Option<Instant>,
    /// The frame that drew it: its number, its commands submitted, presented, and its GPU work done.
    frame: Option<usize>,
    submitted: Option<Instant>,
    presented: Option<Instant>,
    gpu_done: Option<Instant>,
}

/// A drawn frame's times: the surface's texture asked for and had, the input read, the commands submitted and the
/// frame presented.
#[derive(Debug, Clone, Copy)]
struct Frame {
    acquire: (Instant, Instant),
    read: Instant,
    submitted: Instant,
    presented: Instant,
    /// `presented` on the feedback's clock (`CLOCK_MONOTONIC`), ns.
    presented_ns: u64,
}

/// The measurement of one drive.
pub struct Latency {
    /// The scripted flip period.
    script: Option<Duration>,
    started: Instant,
    probes: Vec<Probe>,
    frames: Vec<Frame>,
    /// When the GPU finished a probe's frame ([`Latency::watch_gpu`]).
    gpu: (Sender<Instant>, Receiver<Instant>),
    /// The compositor's feedback ([`Latency::watch_screen`]) and what it said per frame number.
    shown: Option<shown::Shown>,
    feedback: HashMap<u64, shown::Feedback>,
    /// Each answer taken: when, its physics steps and its steering force; the answers counted so far.
    answers: Vec<(Instant, u32, Option<f32>)>,
    counted: u64,
    /// Each drawn frame's force for the wheel (`Wheel::set`).
    wheel: Vec<Option<f32>>,
    /// This frame's times so far.
    acquire: (Instant, Instant),
    read: Instant,
}

impl Latency {
    /// The measurement `RBR_LATENCY` asks for, if any.
    pub fn from_env() -> Option<Latency> {
        let value = std::env::var("RBR_LATENCY").ok()?;
        let script = value
            .strip_prefix("script")
            .map(|rest| Duration::from_millis(rest.strip_prefix(':').and_then(|ms| ms.parse().ok()).unwrap_or(500)));
        let now = Instant::now();
        eprintln!("viewer: timing the live input (RBR_LATENCY={value})");
        Some(Latency {
            script,
            started: now,
            probes: Vec::new(),
            frames: Vec::new(),
            answers: Vec::new(),
            counted: 0,
            wheel: Vec::new(),
            gpu: mpsc::channel(),
            shown: None,
            feedback: HashMap::new(),
            acquire: (now, now),
            read: now,
        })
    }

    /// The force this frame gives the wheel.
    pub fn wheel(&mut self, force: Option<f32>) {
        if self.wheel.len() < MOST_FRAMES {
            self.wheel.push(force);
        }
    }

    /// A frame began by asking for the surface's texture at `start` and had it at `had`.
    pub fn acquired(&mut self, start: Instant, had: Instant) {
        self.acquire = (start, had);
    }

    /// The input was read; returns the scripted steering, if scripted, and opens a probe at each flip.
    pub fn read(&mut self) -> Option<f32> {
        let now = Instant::now();
        self.read = now;
        let period = self.script?;
        let flips = ((now - self.started).as_nanos() / period.as_nanos()) as u32;
        let steer = if flips.is_multiple_of(2) { -SCRIPT_STEER } else { SCRIPT_STEER };
        if flips > 0 && self.probes.last().is_none_or(|p| p.steer != steer) && self.probes.len() < MOST_PROBES {
            self.probes.push(Probe {
                due: self.started + period * flips,
                steer,
                read: now,
                payload: None,
                frame: None,
                sent: None,
                received: None,
                taken: None,
                submitted: None,
                presented: None,
                gpu_done: None,
            });
        }
        Some(steer)
    }

    /// A pump wrote payloads from number `before` on: the open probe's first payload, and its answer's times.
    pub fn pumped(&mut self, live: &Live, before: u64) {
        for n in self.counted..live.answered() {
            if let Some(s) = live.stamps(n).filter(|_| self.answers.len() < MOST_FRAMES) {
                self.answers.push((s.taken.unwrap_or(s.sent), s.substeps, s.force));
            }
        }
        self.counted = live.answered();
        let Some(probe) = self.probes.last_mut() else { return };
        if probe.payload.is_none() && live.posted() > before {
            probe.payload = Some(before);
        }
        if let Some(stamps) = probe.payload.and_then(|n| live.stamps(n)) {
            probe.sent = Some(stamps.sent);
            probe.received = probe.received.or(stamps.received);
            probe.taken = probe.taken.or(stamps.taken);
        }
    }

    /// Asks the compositor when `window`'s frames go on screen (Wayland only).
    pub fn watch_screen(&mut self, window: &Window) {
        match shown::Shown::new(window) {
            Ok(shown) => self.shown = Some(shown),
            Err(e) => eprintln!("viewer: no presentation feedback: {e}"),
        }
    }

    /// The frame is about to be presented: asks for its feedback.
    pub fn presenting(&mut self) {
        if let Some(shown) = &mut self.shown {
            shown.ask(self.frames.len() as u64);
        }
    }

    /// The frame was submitted and presented; whether it is the first to draw the open probe's answer (live play
    /// shows the newest answer taken), when the caller calls [`Latency::watch_gpu`].
    pub fn drawn(&mut self, submitted: Instant) -> bool {
        let presented = Instant::now();
        let presented_ns = shown::now_ns();
        if let Some(shown) = &mut self.shown {
            self.feedback.extend(shown.take());
        }
        if let (Ok(done), Some(probe)) =
            (self.gpu.1.try_recv(), self.probes.iter_mut().rev().find(|p| p.submitted.is_some()))
        {
            probe.gpu_done = Some(done);
        }
        if self.frames.len() < MOST_FRAMES {
            self.frames.push(Frame { acquire: self.acquire, read: self.read, submitted, presented, presented_ns });
        }
        match self.probes.last_mut() {
            Some(probe) if probe.submitted.is_none() && probe.taken.is_some() => {
                (probe.submitted, probe.presented) = (Some(submitted), Some(presented));
                probe.frame = Some(self.frames.len() - 1);
                true
            }
            _ => false,
        }
    }

    /// Notes when the GPU is done with the work submitted so far, without holding the frames back: a thread polls
    /// the device every 0.2 ms until the queue's callback has run.
    pub fn watch_gpu(&self, device: &wgpu::Device, queue: &wgpu::Queue) {
        let (send, done) = (self.gpu.0.clone(), Arc::new(AtomicBool::new(false)));
        let flag = done.clone();
        queue.on_submitted_work_done(move || {
            let _ = send.send(Instant::now());
            flag.store(true, Ordering::Release);
        });
        let device = device.clone();
        thread::spawn(move || {
            while !done.load(Ordering::Acquire) {
                let _ = device.poll(wgpu::PollType::Poll);
                thread::sleep(Duration::from_micros(200));
            }
        });
    }

    /// The summary: per stage of the flips, ms from the flip being due (median, p95), and the frames' own times.
    pub fn summary(&self) -> String {
        let ms = |a: Instant, b: Instant| (b.saturating_duration_since(a)).as_secs_f64() * 1000.0;
        let line = |name: &str, values: Vec<f64>| {
            let (p50, p95, max) = quantiles(values.clone());
            format!("\n  {name:<34} n {:>5}  p50 {p50:6.2}  p95 {p95:6.2}  max {max:6.2}", values.len())
        };
        let done: Vec<&Probe> = self.probes.iter().filter(|p| p.gpu_done.is_some()).collect();
        let from_due = |stage: fn(&Probe) -> Option<Instant>| {
            done.iter().filter_map(|p| Some(ms(p.due, stage(p)?))).collect::<Vec<_>>()
        };
        let between = |a: fn(&Probe) -> Option<Instant>, b: fn(&Probe) -> Option<Instant>| {
            done.iter().filter_map(|p| Some(ms(a(p)?, b(p)?))).collect::<Vec<_>>()
        };
        let mut out =
            format!("live latency: {} flips, {} drawn; ms from the flip being due:", self.probes.len(), done.len());
        if !done.is_empty() {
            out += &line("input read", from_due(|p| Some(p.read)));
            out += &line("payload sent", from_due(|p| p.sent));
            out += &line("answer received (reader thread)", from_due(|p| p.received));
            out += &line("answer taken (pump)", from_due(|p| p.taken));
            out += &line("frame submitted", from_due(|p| p.submitted));
            out += &line("frame presented", from_due(|p| p.presented));
            out += &line("frame done on the GPU", from_due(|p| p.gpu_done));
            out += &line("frame on screen", done.iter().filter_map(|p| self.on_screen(p.frame?, p.due)).collect());
            out += "\n stage by stage:";
            out += &line("read -> sent", between(|p| Some(p.read), |p| p.sent));
            out += &line("sent -> received (step, pipes)", between(|p| p.sent, |p| p.received));
            out += &line("received -> taken", between(|p| p.received, |p| p.taken));
            out += &line("taken -> submitted", between(|p| p.taken, |p| p.submitted));
            out += &line("submitted -> presented", between(|p| p.submitted, |p| p.presented));
            out += &line("submitted -> GPU done", between(|p| p.submitted, |p| p.gpu_done));
        }
        let frames = &self.frames;
        out += &format!("\n frames ({}), ms:", frames.len());
        out += &line("period", frames.windows(2).map(|w| ms(w[0].acquire.0, w[1].acquire.0)).collect());
        out += &line("acquire wait", frames.iter().map(|f| ms(f.acquire.0, f.acquire.1)).collect());
        out += &line("acquired -> input read", frames.iter().map(|f| ms(f.acquire.1, f.read)).collect());
        out += &line("input read -> submitted", frames.iter().map(|f| ms(f.read, f.submitted)).collect());
        out += &line("submitted -> presented", frames.iter().map(|f| ms(f.submitted, f.presented)).collect());
        out += &line("input read -> presented", frames.iter().map(|f| ms(f.read, f.presented)).collect());
        out += &line(
            "presented -> on screen",
            (0..frames.len()).filter_map(|i| self.on_screen(i, frames[i].presented)).collect(),
        );
        out += &self.screen_counts();
        out += &self.force_counts();
        out
    }
}

impl Latency {
    /// Ms from `from` until frame `frame` went on screen, if the compositor said.
    fn on_screen(&self, frame: usize, from: Instant) -> Option<f64> {
        let Some(&shown::Feedback::Shown { at, .. }) = self.feedback.get(&(frame as u64)) else { return None };
        let f = &self.frames[frame];
        let since = f.presented.saturating_duration_since(from).as_secs_f64() * 1000.0;
        Some(since + (at as f64 - f.presented_ns as f64) / 1e6)
    }

    /// The physics steps per answer and per drawn frame, and how often the steering force changed a second: in the
    /// answers, and in what the frames give the wheel.
    fn force_counts(&self) -> String {
        let (Some(first), Some(last)) = (self.answers.first(), self.answers.last()) else { return String::new() };
        let seconds = (last.0 - first.0).as_secs_f64().max(1e-9);
        let changes = |forces: &mut dyn Iterator<Item = Option<f32>>| {
            let forces: Vec<_> = forces.collect();
            forces.windows(2).filter(|w| w[0] != w[1]).count() as f64 / seconds
        };
        let steps: u64 = self.answers.iter().map(|a| u64::from(a.1)).sum();
        let per_answer = quantiles(self.answers.iter().map(|a| f64::from(a.1)).collect());
        format!(
            "\n  steps: {:.0}/s, per answer p50 {:.0} max {:.0}, per drawn frame {:.1}; force changes {:.0}/s in the \
             answers ({:.0} answers/s), {:.0}/s given to the wheel ({} frames)",
            steps as f64 / seconds,
            per_answer.0,
            per_answer.2,
            steps as f64 / self.frames.len().max(1) as f64,
            changes(&mut self.answers.iter().map(|a| a.2)),
            self.answers.len() as f64 / seconds,
            changes(&mut self.wheel.iter().copied()),
            self.wheel.len(),
        )
    }

    /// The feedback's counts: shown with vsync, without (torn), zero-copy (direct scan-out), and discarded.
    fn screen_counts(&self) -> String {
        if self.shown.is_some() {
            let count = |test: &dyn Fn(&shown::Feedback) -> bool| self.feedback.values().filter(|f| test(f)).count();
            let flag =
                |bit| move |f: &shown::Feedback| matches!(f, shown::Feedback::Shown { flags, .. } if flags & bit != 0);
            return format!(
                "\n  feedback: {} shown ({} vsync, {} not vsync, {} zero-copy), {} discarded",
                count(&|f| matches!(f, shown::Feedback::Shown { .. })),
                count(&flag(shown::VSYNC)),
                count(&|f| matches!(f, shown::Feedback::Shown { flags, .. } if flags & shown::VSYNC == 0)),
                count(&flag(shown::ZERO_COPY)),
                count(&|f| *f == shown::Feedback::Discarded),
            );
        }
        String::new()
    }
}

/// The median, 95th percentile and largest of `values` (0 for none).
fn quantiles(mut values: Vec<f64>) -> (f64, f64, f64) {
    values.sort_by(f64::total_cmp);
    let at =
        |q: f64| values.get(((values.len().saturating_sub(1)) as f64 * q).round() as usize).copied().unwrap_or(0.0);
    (at(0.5), at(0.95), at(1.0))
}
