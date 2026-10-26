//! The in-game performance log (`viewer --perf-log DIR`; the launcher's "Performance log"): where a player's frame time
//! goes, in two files named by the session's start (UTC), `perf-<YYYYMMDD-HHMMSS>.log` and `.trace.json`.
//!
//! The `.log` is a header (the machine, the GPU, the settings), a line a second and a summary at exit; the trace holds
//! the last [`RING`] frames for ui.perfetto.dev or chrome://tracing, written at exit (a killed viewer leaves the
//! `.log` only). Timings come from [`span`]s on the CPU, one track a thread (name a thread with [`name_thread`]), and
//! from the GPU's timestamps (`profile::Profiler`, a few frames late, never waited for), where the device has them.
//!
//! GPU time is exact; its place on the CPU's timeline is not: the GPU's clock is not the CPU's and wgpu offers no way
//! to correlate them, so each frame's passes end where its last submission was made (`Profiler::end_frame`). A GPU
//! running behind the CPU shows its passes earlier than they ran; a pass is never shorter or longer than it was.
//!
//! Off, a [`span`] is a load of [`ON`] and a branch, and no query set exists. On, a span takes two clock reads and a
//! lock held for a push. Overhead, 45O with `--enhance all`, 2560x1440, 600 frames, 1 second a line: see
//! `notes/renderer.md` ("Performance log").

use std::cell::Cell;
use std::collections::VecDeque;
use std::fmt::Write as _;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::sync::{Mutex, MutexGuard};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

/// Frames the trace keeps.
const RING: usize = 600;
/// Events kept for one frame's trace (the statistics count all of them).
const MAX_EVENTS: usize = 512;
/// A hitch is a frame over twice the second's median and over this long, milliseconds.
const HITCH_MS: f64 = 25.0;

static ON: AtomicBool = AtomicBool::new(false);
static RECORDER: Mutex<Option<Recorder>> = Mutex::new(None);

thread_local! {
    /// The track (index into [`Recorder::tracks`]) of the spans this thread records; 0 is the main thread's.
    static TRACK: Cell<usize> = const { Cell::new(0) };
}

/// Whether the log is on ([`start`]).
pub fn enabled() -> bool {
    ON.load(Relaxed)
}

/// Stops recording spans (`true`) or goes on: to measure what the log costs.
pub fn pause(paused: bool) {
    ON.store(!paused && lock().is_some(), Relaxed);
}

/// Starts a session in `dir` (made if missing). Spans recorded before the first [`frame`] are the startup's.
pub fn start(dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let stem = format!("perf-{}", utc(secs).stamp());
    let log = std::fs::File::create(dir.join(format!("{stem}.log")))?;
    let recorder =
        Recorder::new(Instant::now(), Box::new(std::io::BufWriter::new(log)), dir.join(format!("{stem}.trace.json")));
    *lock() = Some(recorder);
    ON.store(true, Relaxed);
    Ok(())
}

/// Writes the session's header: `lines` are the settings as `(name, value)` pairs.
pub fn header(lines: &[(&str, String)]) {
    if let Some(recorder) = lock().as_mut() {
        recorder.write(&format_header(lines));
    }
}

/// Ends the session: the last second, the summary, the trace. Does nothing without a session.
pub fn finish() {
    ON.store(false, Relaxed);
    let Some(mut recorder) = lock().take() else { return };
    recorder.finish();
}

/// Starts frame `n` and ends the one before; a window calls it once a loop.
pub fn frame() {
    if enabled() {
        if let Some(recorder) = lock().as_mut() {
            recorder.next_frame(Instant::now());
        }
    }
}

/// The open frame's number (what [`gpu`] takes).
pub fn frame_index() -> u64 {
    lock().as_ref().map_or(0, |r| r.frames.saturating_sub(1))
}

/// Names the calling thread's track in the trace.
pub fn name_thread(name: &'static str) {
    if enabled() {
        if let Some(recorder) = lock().as_mut() {
            TRACK.set(recorder.track(name));
        }
    }
}

/// A time span, recorded when dropped; `None` inside when the log is off.
#[must_use = "the span ends when it is dropped"]
pub struct Span(Option<(&'static str, Instant)>);

/// Starts the span `name` on this thread.
pub fn span(name: &'static str) -> Span {
    Span(enabled().then(|| (name, Instant::now())))
}

impl Span {
    /// Ends the span and starts `name` at once: the phases of a function in a row.
    pub fn next(&mut self, name: &'static str) {
        if let Some((last, start)) = self.0 {
            let now = Instant::now();
            record(last, start, now);
            self.0 = Some((name, now));
        }
    }
}

impl Drop for Span {
    fn drop(&mut self) {
        if let Some((name, start)) = self.0.take() {
            record(name, start, Instant::now());
        }
    }
}

/// Records the CPU span `name` of this thread, from `start` to `end`.
pub fn record(name: &'static str, start: Instant, end: Instant) {
    if let Some(recorder) = lock().as_mut() {
        recorder.cpu_span(TRACK.get(), name, start, end);
    }
}

/// Records the GPU's passes of frame `frame` (`profile::Profiler`): each pass's `(label, start, end)` in nanoseconds
/// from the frame's first timestamp, the frame's `total` from it to its last, which was `end` on the CPU's clock.
pub fn gpu(frame: u64, end: Instant, total: u64, passes: &[(&'static str, u64, u64)]) {
    if let Some(recorder) = lock().as_mut() {
        recorder.gpu_frame(frame, end, total, passes);
    }
}

fn lock() -> MutexGuard<'static, Option<Recorder>> {
    RECORDER.lock().unwrap_or_else(|e| e.into_inner())
}

/// One timed span in the trace: times in nanoseconds from the session's start.
struct Event {
    track: usize,
    name: &'static str,
    start: u64,
    dur: u64,
}

/// What a frame holds: when it began and its events.
struct Frame {
    start: u64,
    events: Vec<Event>,
}

struct Recorder {
    start: Instant,
    log: Box<dyn Write + Send>,
    trace: PathBuf,
    tracks: Vec<&'static str>,
    /// What was recorded before the first frame: the loading.
    startup: Vec<Event>,
    /// The last [`RING`] frames, the open one last; empty before the first.
    ring: VecDeque<Frame>,
    /// Frames begun.
    frames: u64,
    /// The open frame's CPU time per name, all threads.
    totals: Vec<(&'static str, u64)>,
    /// The second being counted: when it began and its frames' times in milliseconds.
    second: (u64, Vec<f64>),
    cpu: Table,
    gpu: Table,
    frame_times: Hist,
}

impl Recorder {
    fn new(start: Instant, log: Box<dyn Write + Send>, trace: PathBuf) -> Recorder {
        Recorder {
            start,
            log,
            trace,
            tracks: vec!["main"],
            startup: Vec::new(),
            ring: VecDeque::new(),
            frames: 0,
            totals: Vec::new(),
            second: (0, Vec::new()),
            cpu: Table::default(),
            gpu: Table::default(),
            frame_times: Hist::default(),
        }
    }

    fn write(&mut self, text: &str) {
        // A full disk must not stop the drive.
        let _ = self.log.write_all(text.as_bytes());
        let _ = self.log.flush();
    }

    fn nanos(&self, at: Instant) -> u64 {
        at.saturating_duration_since(self.start).as_nanos() as u64
    }

    fn track(&mut self, name: &'static str) -> usize {
        self.tracks.iter().position(|t| *t == name).unwrap_or_else(|| {
            self.tracks.push(name);
            self.tracks.len() - 1
        })
    }

    fn cpu_span(&mut self, track: usize, name: &'static str, start: Instant, end: Instant) {
        let (start, dur) = (self.nanos(start), end.saturating_duration_since(start).as_nanos() as u64);
        let events = match self.ring.back_mut() {
            Some(frame) => {
                match self.totals.iter_mut().find(|(n, _)| *n == name) {
                    Some((_, total)) => *total += dur,
                    None => self.totals.push((name, dur)),
                }
                &mut frame.events
            }
            None => &mut self.startup,
        };
        if events.len() < MAX_EVENTS {
            events.push(Event { track, name, start, dur });
        }
    }

    fn gpu_frame(&mut self, frame: u64, end: Instant, total: u64, passes: &[(&'static str, u64, u64)]) {
        let first = self.nanos(end).saturating_sub(total);
        let mut sums: Vec<(&'static str, u64)> = vec![("GPU frame", total)];
        let track = self.track("GPU");
        let oldest = self.frames - self.ring.len() as u64;
        let events = frame.checked_sub(oldest).and_then(|i| self.ring.get_mut(i as usize));
        let mut events = events.map(|f| &mut f.events);
        for &(name, from, to) in passes {
            match sums.iter_mut().find(|(n, _)| *n == name) {
                Some((_, sum)) => *sum += to - from,
                None => sums.push((name, to - from)),
            }
            if let Some(events) = events.as_mut().filter(|e| e.len() < MAX_EVENTS) {
                events.push(Event { track, name, start: first + from, dur: to - from });
            }
        }
        for (name, ns) in sums {
            self.gpu.add(name, ns);
        }
    }

    /// Ends the open frame and begins the next, `now`.
    fn next_frame(&mut self, now: Instant) {
        let now = self.nanos(now);
        let track = self.track("frames");
        match self.ring.back_mut() {
            Some(open) => {
                let dur = now - open.start;
                open.events.push(Event { track, name: "frame", start: open.start, dur });
                self.second.1.push(dur as f64 / 1e6);
                self.frame_times.add(dur);
                for (name, ns) in std::mem::take(&mut self.totals) {
                    self.cpu.add(name, ns);
                }
            }
            None => {
                let startup = std::mem::take(&mut self.startup);
                let line = startup_line(&startup);
                self.startup = startup;
                self.write(&line);
                self.second.0 = now;
            }
        }
        self.ring.push_back(Frame { start: now, events: Vec::new() });
        self.frames += 1;
        if self.ring.len() > RING {
            self.ring.pop_front();
        }
        if now - self.second.0 >= 1_000_000_000 {
            self.flush_second(now);
        }
    }

    /// Writes the line of the second that ended at `now` (nanoseconds).
    fn flush_second(&mut self, now: u64) {
        let line = second_line((now / 1_000_000_000).max(1), &self.second.1, &self.cpu.second(), &self.gpu.second());
        self.write(&line);
        self.second = (now, Vec::new());
        self.cpu.reset_second();
        self.gpu.reset_second();
    }

    fn finish(&mut self) {
        let now = self.nanos(Instant::now());
        if !self.second.1.is_empty() {
            self.flush_second(now);
        }
        let mut summary = format!("\nSession: {} frames in {:.1} s\n", self.frames, now as f64 / 1e9);
        summary += &summary_table("frame", &[("frame time", &self.frame_times)]);
        summary += &summary_table("cpu", &self.cpu.hists());
        summary += &summary_table("gpu", &self.gpu.hists());
        self.write(&summary);
        let trace = self.trace_json();
        match std::fs::write(&self.trace, trace) {
            Ok(()) => eprintln!("viewer: performance log: {}", self.trace.display()),
            Err(e) => eprintln!("viewer: {}: {e}", self.trace.display()),
        }
    }

    /// The trace: Chrome's Trace Event format, a complete event ("X", microseconds) per span, a thread a track.
    fn trace_json(&self) -> String {
        let mut json = String::from("{\"displayTimeUnit\":\"ms\",\"traceEvents\":[\n");
        for (tid, name) in self.tracks.iter().enumerate() {
            let _ = writeln!(
                json,
                "{{\"ph\":\"M\",\"pid\":1,\"tid\":{tid},\"name\":\"thread_name\",\"args\":{{\"name\":\"{name}\"}}}},"
            );
        }
        let events = self.startup.iter().chain(self.ring.iter().flat_map(|f| &f.events));
        for (i, e) in events.enumerate() {
            let comma = if i == 0 { "" } else { ",\n" };
            let (name, ts, dur) = (e.name.replace(['"', '\\'], "'"), e.start as f64 / 1e3, e.dur as f64 / 1e3);
            let _ = write!(
                json,
                "{comma}{{\"ph\":\"X\",\"pid\":1,\"tid\":{},\"name\":\"{name}\",\"ts\":{ts:.3},\"dur\":{dur:.3}}}",
                e.track
            );
        }
        json + "\n]}\n"
    }
}

/// Spans' statistics by name, in the order first seen.
#[derive(Default)]
struct Table(Vec<(&'static str, Acc, Hist)>);

/// The count, sum and largest of a second's times, nanoseconds.
#[derive(Default, Clone, Copy)]
struct Acc {
    n: u64,
    sum: u64,
    max: u64,
}

impl Table {
    fn add(&mut self, name: &'static str, ns: u64) {
        let i = self.0.iter().position(|(n, ..)| *n == name).unwrap_or_else(|| {
            self.0.push((name, Acc::default(), Hist::default()));
            self.0.len() - 1
        });
        let (_, acc, hist) = &mut self.0[i];
        (acc.n, acc.sum, acc.max) = (acc.n + 1, acc.sum + ns, acc.max.max(ns));
        hist.add(ns);
    }

    /// Each name's mean and largest milliseconds this second, `None` when it did not run.
    fn second(&self) -> Vec<(&'static str, Option<(f64, f64)>)> {
        let ms = |ns: u64| ns as f64 / 1e6;
        let stat = |a: Acc| (a.n > 0).then(|| (ms(a.sum) / a.n as f64, ms(a.max)));
        self.0.iter().map(|&(name, acc, _)| (name, stat(acc))).collect()
    }

    fn reset_second(&mut self) {
        self.0.iter_mut().for_each(|(_, acc, _)| *acc = Acc::default());
    }

    fn hists(&self) -> Vec<(&'static str, &Hist)> {
        self.0.iter().map(|(name, _, hist)| (*name, hist)).collect()
    }
}

/// Times for percentiles in constant memory: buckets of a 16th of an octave (4.4 %) from 1 microsecond up.
struct Hist {
    counts: Vec<u32>,
    n: u64,
    sum: u64,
    max: u64,
}

const BUCKETS: usize = 24 * 16;

impl Default for Hist {
    fn default() -> Hist {
        Hist { counts: vec![0; BUCKETS], n: 0, sum: 0, max: 0 }
    }
}

impl Hist {
    fn add(&mut self, ns: u64) {
        let bucket = ((ns.max(1000) as f64 / 1000.0).log2() * 16.0) as usize;
        self.counts[bucket.min(BUCKETS - 1)] += 1;
        (self.n, self.sum, self.max) = (self.n + 1, self.sum + ns, self.max.max(ns));
    }

    fn mean(&self) -> f64 {
        self.sum as f64 / self.n.max(1) as f64
    }

    /// The `q` quantile (0 to 1), nanoseconds: the middle of its bucket, within the largest time.
    fn quantile(&self, q: f64) -> f64 {
        let want = (q * self.n as f64).ceil().max(1.0) as u64;
        let mut seen = 0;
        for (bucket, &count) in self.counts.iter().enumerate() {
            seen += u64::from(count);
            if seen >= want {
                return (1000.0 * ((bucket as f64 + 0.5) / 16.0).exp2()).min(self.max as f64);
            }
        }
        self.max as f64
    }
}

/// A moment of UTC.
struct Utc([u64; 6]);

/// `secs` since the epoch as a date and time.
fn utc(secs: u64) -> Utc {
    let (days, day) = (secs / 86400, secs % 86400);
    // Days to a civil date (Hinnant's algorithm), from 1 March of year 0.
    let (era, doe) = ((days + 719_468) / 146_097, (days + 719_468) % 146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + u64::from(month <= 2);
    Utc([year, month, doy - (153 * mp + 2) / 5 + 1, day / 3600, day % 3600 / 60, day % 60])
}

impl Utc {
    /// `YYYYMMDD-HHMMSS`.
    fn stamp(&self) -> String {
        let [y, mo, d, h, mi, s] = self.0;
        format!("{y:04}{mo:02}{d:02}-{h:02}{mi:02}{s:02}")
    }
}

/// The log's start: the session's time, the settings `lines` and how to read the rest.
fn format_header(lines: &[(&str, String)]) -> String {
    let width = lines.iter().map(|(k, _)| k.len()).max().unwrap_or(0);
    let mut text = String::from("ReRBR performance log\n");
    for (key, value) in lines {
        let _ = writeln!(text, "{key:width$}  {value}");
    }
    text + "\nA line a second: frames, the frame time in ms (mean, median, 99th percentile, largest) and hitches (frames\n\
            over twice the second's median and over 25 ms), then the CPU spans and the GPU's passes as \"name mean/max\"\n\
            in ms over the frames each ran in (a CPU span nests in the one that holds it; a name seen later joins the end).\n\n"
}

/// The line of the startup: each span the loading recorded, with its count and total milliseconds.
fn startup_line(events: &[Event]) -> String {
    let mut spans: Vec<(&str, u32, u64)> = Vec::new();
    for e in events {
        match spans.iter_mut().find(|s| s.0 == e.name) {
            Some(s) => (s.1, s.2) = (s.1 + 1, s.2 + e.dur),
            None => spans.push((e.name, 1, e.dur)),
        }
    }
    let spans: Vec<String> = spans.iter().map(|(name, n, ns)| format!("{name} {:.0} x{n}", *ns as f64 / 1e6)).collect();
    format!("startup (ms): {}\n", spans.join("; "))
}

/// The line of second `t`: `frames` are its frames' times in ms, `cpu` and `gpu` the spans' (mean, largest) ms.
fn second_line(
    t: u64,
    frames: &[f64],
    cpu: &[(&str, Option<(f64, f64)>)],
    gpu: &[(&str, Option<(f64, f64)>)],
) -> String {
    let mut sorted = frames.to_vec();
    sorted.sort_by(f64::total_cmp);
    let at = |q: f64| sorted[((q * sorted.len() as f64).ceil() as usize).clamp(1, sorted.len()) - 1];
    let (mean, median) = (frames.iter().sum::<f64>() / frames.len() as f64, at(0.5));
    let hitches = frames.iter().filter(|&&ms| ms > 2.0 * median && ms > HITCH_MS).count();
    let spans = |stats: &[(&str, Option<(f64, f64)>)]| {
        let one = |(name, stat): &(&str, Option<(f64, f64)>)| match stat {
            Some((mean, max)) => format!("{name} {mean:.2}/{max:.2}"),
            None => format!("{name} -"),
        };
        stats.iter().map(one).collect::<Vec<_>>().join("; ")
    };
    format!(
        "t={t}s frames {} frame {mean:.2} {median:.2} {:.2} {:.2} hitches {hitches} | cpu {} | gpu {}\n",
        frames.len(),
        at(0.99),
        sorted[sorted.len() - 1],
        spans(cpu),
        spans(gpu),
    )
}

/// A summary's rows: each name's frames and its mean, median, 99th percentile and largest in ms.
fn summary_table(title: &str, rows: &[(&str, &Hist)]) -> String {
    let width = rows.iter().map(|(name, _)| name.len()).max().unwrap_or(0);
    let mut text = format!("{title:width$}  {:>8} {:>9} {:>9} {:>9} {:>9}\n", "n", "mean", "p50", "p99", "max");
    for (name, hist) in rows {
        let ms = |ns: f64| ns / 1e6;
        let _ = writeln!(
            text,
            "{name:width$}  {:>8} {:>9.3} {:>9.3} {:>9.3} {:>9.3}",
            hist.n,
            ms(hist.mean()),
            ms(hist.quantile(0.5)),
            ms(hist.quantile(0.99)),
            ms(hist.max as f64),
        );
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn recorder() -> Recorder {
        Recorder::new(Instant::now(), Box::new(std::io::sink()), PathBuf::new())
    }

    #[test]
    fn the_stamp_is_utc() {
        assert_eq!(utc(1_700_000_000).stamp(), "20231114-221320");
        assert_eq!(utc(951_782_400 + 86_399).stamp(), "20000229-235959");
    }

    #[test]
    fn a_second_reads_in_a_stable_order() {
        let frames = [16.0, 17.0, 16.5, 60.0];
        let cpu = [("acquire", Some((8.0, 9.5))), ("mirrors", None)];
        let line = second_line(12, &frames, &cpu, &[("GPU frame", Some((5.0, 6.0)))]);
        assert_eq!(
            line,
            "t=12s frames 4 frame 27.38 16.50 60.00 60.00 hitches 1 | cpu acquire 8.00/9.50; mirrors - | gpu GPU frame 5.00/6.00\n"
        );
    }

    #[test]
    fn a_slow_second_is_no_hitch() {
        let line = second_line(1, &[40.0, 40.0, 90.0], &[], &[]);
        assert!(line.contains("hitches 1"), "{line}");
        let line = second_line(1, &[20.0, 20.0, 24.0, 5.0], &[], &[]);
        assert!(line.contains("hitches 0"), "{line}");
    }

    #[test]
    fn the_header_aligns_its_values() {
        let text = format_header(&[("adapter", "GPU".into()), ("os", "linux".into())]);
        assert!(text.starts_with("ReRBR performance log\nadapter  GPU\nos       linux\n"), "{text}");
    }

    #[test]
    fn percentiles_are_within_a_bucket() {
        let mut hist = Hist::default();
        for ms in 1..=100u64 {
            hist.add(ms * 1_000_000);
        }
        let within = |got: f64, want: f64| (got / want - 1.0).abs() < 0.05;
        assert!(within(hist.quantile(0.5), 50e6) && within(hist.quantile(0.99), 99e6));
        assert_eq!((hist.mean(), hist.max), (50.5e6, 100_000_000));
    }

    #[test]
    fn the_trace_holds_the_last_frames_with_their_gpu_passes() {
        let mut r = recorder();
        let start = r.start;
        let at = |ms: u64| start + Duration::from_millis(ms);
        r.cpu_span(0, "loading", at(0), at(1));
        for i in 0..700 {
            r.next_frame(at(10 + i * 10));
            r.cpu_span(0, "render", at(10 + i * 10), at(14 + i * 10));
        }
        // The GPU's frame 650 ended 3 ms into the next one.
        r.gpu_frame(650, at(10 + 650 * 10 + 3), 2_000_000, &[("scene", 0, 1_500_000)]);
        r.gpu_frame(2, at(0), 1, &[("scene", 0, 1)]);
        assert_eq!(r.ring.len(), RING);
        let json = r.trace_json();
        assert_eq!(json.matches("\"name\":\"render\"").count(), RING);
        assert_eq!(json.matches("\"name\":\"loading\"").count(), 1);
        assert_eq!(json.matches("\"name\":\"scene\"").count(), 1);
        // The GPU work began 2 ms before the 6513 ms it ended.
        assert!(json.contains("\"tid\":2,\"name\":\"scene\",\"ts\":6511000.000,\"dur\":1500.000"), "{json}");
        assert!(json.contains("\"args\":{\"name\":\"GPU\"}"));
    }
}
