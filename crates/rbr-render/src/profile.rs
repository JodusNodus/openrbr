//! Time per stage of a frame: `viewer --frames --profile`, the median over the frames (a measuring tool), and the
//! performance log's GPU passes and CPU stages ([`crate::perf_log`], `Profiler::new` with `streaming`).
//!
//! Where the device has timestamps inside encoders ([`wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS`], requested by
//! [`crate::renderer::request_gpu`]), each mark writes the GPU's clock, so the frame runs as usual and each span is the
//! GPU time between its marks. Otherwise, on macOS, or with `RBR_PROFILE=wait`, at each mark the frame's commands so
//! far are submitted and waited for, so the wall-clock time between two marks is the GPU time of the stage between
//! them, plus a submission's overhead: that slows the frame and lets the GPU clock down. (Metal samples its timestamps
//! without waiting for the passes before, which the tile GPUs run late and overlapped, so they say nothing there;
//! `RBR_PROFILE=stamps` uses them anyway.)
//!
//! Streaming (the log) never waits: the timestamps of a frame are read back [`SLOTS`] - 1 frames at most behind it,
//! through a ring of query sets and buffers (a frame finding its slot still read has no GPU times). Where the
//! timestamps are not used (macOS, no feature) only the CPU's stages are timed, each from its mark to its span.

use crate::perf_log;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Timestamps a frame can hold.
const STAMPS: u32 = 64;
/// Frames in flight in the ring.
const SLOTS: usize = 8;
/// The shadow thread's span, which no mark takes: the label of its two timestamps ([`Profiler::shadow_stamps`]).
const SHADOW: &str = "sun cascades and lamp shadow";
/// The offset of the marks' results in a slot's buffers, in 8-byte ticks (a resolve's offset is a multiple of 256
/// bytes); the shadow thread's two come first.
const MARKS_AT: usize = 32;

pub(crate) struct Profiler {
    device: wgpu::Device,
    queue: wgpu::Queue,
    /// The performance log's: reads back without waiting, and records the CPU's stages too.
    streaming: bool,
    /// The ring of query sets, or none (waiting marks, or streaming without timestamps).
    slots: Vec<Slot>,
    /// The slot this frame's marks write.
    current: usize,
    /// When each mark of this frame finished (waiting marks), or was made (streaming).
    marks: Vec<Instant>,
    /// This frame's spans by their marks (timestamp marks), measured at [`Profiler::end_frame`].
    spans: Vec<(&'static str, usize, usize)>,
    /// Per label, in the order first seen: the milliseconds of each frame.
    times: Vec<(&'static str, Vec<f64>)>,
}

/// A query set the marks write and the buffers that read it back.
struct Slot {
    set: wgpu::QuerySet,
    /// The shadow thread's two timestamps, and whether it wrote them this frame.
    shadow_set: wgpu::QuerySet,
    shadow: bool,
    resolved: wgpu::Buffer,
    read: wgpu::Buffer,
    /// Set (`Some`) while `read` is being mapped, with the frame's spans, number and end on the CPU's clock.
    reading: Option<Reading>,
}

struct Reading {
    done: Arc<AtomicBool>,
    shadow: bool,
    spans: Vec<(&'static str, usize, usize)>,
    frame: u64,
    end: Instant,
    /// The bytes mapped.
    size: u64,
}

impl Profiler {
    pub(crate) fn new(device: &wgpu::Device, queue: &wgpu::Queue, streaming: bool) -> Profiler {
        let timestamps = wgpu::Features::TIMESTAMP_QUERY | wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS;
        let wait = match std::env::var("RBR_PROFILE").as_deref() {
            Ok("wait") => true,
            Ok("stamps") => false,
            _ => cfg!(target_os = "macos"),
        };
        let stamped = device.features().contains(timestamps) && !wait;
        let slots = (0..if !stamped {
            0
        } else if streaming {
            SLOTS
        } else {
            1
        })
            .map(|_| Slot::new(device))
            .collect();
        if !streaming {
            eprintln!("profile: {}", if stamped { "GPU timestamps" } else { "waiting at each mark" });
        }
        Profiler {
            device: device.clone(),
            queue: queue.clone(),
            streaming,
            slots,
            current: 0,
            marks: Vec::new(),
            spans: Vec::new(),
            times: Vec::new(),
        }
    }

    /// Whether the GPU's passes are timed (the log's header says so).
    pub(crate) fn gpu_timed(&self) -> bool {
        !self.slots.is_empty()
    }

    /// Whether the profiler stops the frame to measure it (`--profile`): the shadow maps then encode in line.
    pub(crate) fn blocking(&self) -> bool {
        !self.streaming
    }

    /// The query set for the shadow thread's two timestamps (before and after its commands), if they are timed.
    pub(crate) fn shadow_stamps(&mut self) -> Option<wgpu::QuerySet> {
        let slot = self.slots.get_mut(self.current)?;
        slot.shadow = true;
        Some(slot.shadow_set.clone())
    }

    /// Marks the point `encoder` has reached and returns the mark's index, for [`Profiler::span`].
    pub(crate) fn mark(&mut self, encoder: &mut wgpu::CommandEncoder) -> usize {
        if let Some(slot) = self.slots.get(self.current) {
            encoder.write_timestamp(&slot.set, (self.marks.len() as u32).min(STAMPS - 1));
        } else if !self.streaming {
            let done = std::mem::replace(encoder, self.device.create_command_encoder(&Default::default()));
            self.queue.submit([done.finish()]);
            let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
        }
        self.marks.push(Instant::now());
        self.marks.len() - 1
    }

    /// Ends the span `label` that started at mark `start`.
    pub(crate) fn span(&mut self, encoder: &mut wgpu::CommandEncoder, label: &'static str, start: usize) {
        let end = self.mark(encoder);
        if self.streaming {
            // What a mark costs is no stage.
            if label != "(nothing)" {
                perf_log::record(label, self.marks[start], self.marks[end]);
            }
        }
        if !self.slots.is_empty() {
            let stamp = |mark: usize| mark.min(STAMPS as usize - 1);
            self.spans.push((label, stamp(start), stamp(end)));
        } else if !self.streaming {
            self.add(label, self.marks[end] - self.marks[start]);
        }
    }

    /// Adds `time` to `label`.
    pub(crate) fn add(&mut self, label: &'static str, time: Duration) {
        let ms = time.as_secs_f64() * 1000.0;
        match self.times.iter_mut().find(|(l, _)| *l == label) {
            Some((_, times)) => times.push(ms),
            None => self.times.push((label, vec![ms])),
        }
    }

    /// Ends the frame's marks: with timestamps, reads them back (the frame's commands are submitted by now), at once
    /// when measuring and else, once the GPU has them, in a later call. The frame's end is the log's calibration.
    pub(crate) fn end_frame(&mut self) {
        let used = std::mem::take(&mut self.marks).len() as u32;
        let spans = std::mem::take(&mut self.spans);
        let current = self.current;
        let next = (current + 1) % self.slots.len().max(1);
        let Some(slot) = self.slots.get_mut(current) else { return };
        self.current = next;
        let shadow = std::mem::take(&mut slot.shadow);
        if (used == 0 && !shadow) || slot.reading.is_some() {
            return self.collect();
        }
        let used = used.min(STAMPS);
        let mut encoder = self.device.create_command_encoder(&Default::default());
        if shadow {
            encoder.resolve_query_set(&slot.shadow_set, 0..2, &slot.resolved, 0);
        }
        if used > 0 {
            encoder.resolve_query_set(&slot.set, 0..used, &slot.resolved, MARKS_AT as u64 * 8);
        }
        let size = (MARKS_AT as u64 + u64::from(used)) * 8;
        encoder.copy_buffer_to_buffer(&slot.resolved, 0, &slot.read, 0, size);
        self.queue.submit([encoder.finish()]);
        let done = Arc::new(AtomicBool::new(false));
        let signal = Arc::clone(&done);
        slot.read.slice(..size).map_async(wgpu::MapMode::Read, move |_| signal.store(true, Ordering::Release));
        let end = Instant::now();
        slot.reading = Some(Reading { done, shadow, spans, frame: perf_log::frame_index(), end, size });
        let poll = if self.streaming { wgpu::PollType::Poll } else { wgpu::PollType::wait_indefinitely() };
        let _ = self.device.poll(poll);
        self.collect();
    }

    /// Takes in the frames whose timestamps are mapped.
    fn collect(&mut self) {
        let period = f64::from(self.queue.get_timestamp_period());
        for i in 0..self.slots.len() {
            let slot = &mut self.slots[i];
            if !slot.reading.as_ref().is_some_and(|r| r.done.load(Ordering::Acquire)) {
                continue;
            }
            let Some(Reading { shadow, spans, frame, end, size, .. }) = slot.reading.take() else { continue };
            let ticks: Vec<u64> =
                bytemuck::pod_collect_to_vec(&slot.read.slice(..size).get_mapped_range().expect("mapped"));
            slot.read.unmap();
            let tick = |mark: usize| ticks[MARKS_AT + mark];
            let mut spans: Vec<_> = spans.into_iter().map(|(label, a, b)| (label, tick(a), tick(b))).collect();
            if shadow {
                spans.push((SHADOW, ticks[0], ticks[1]));
            }
            let nanos = |from: u64, to: u64| (to.saturating_sub(from) as f64 * period) as u64;
            if !self.streaming {
                for (label, from, to) in spans {
                    self.add(label, Duration::from_nanos(nanos(from, to)));
                }
                continue;
            }
            let first = spans.iter().map(|s| s.1).min().unwrap_or(0);
            let last = spans.iter().map(|s| s.2).max().unwrap_or(0);
            let passes: Vec<_> = spans
                .iter()
                .filter(|s| s.0 != "(nothing)")
                .map(|&(label, from, to)| (label, nanos(first, from), nanos(first, to)))
                .collect();
            perf_log::gpu(frame, end, nanos(first, last), &passes);
        }
    }

    /// Each label's median milliseconds over the frames it was in, in the order first seen.
    pub(crate) fn medians(&self) -> Vec<(&'static str, f64)> {
        let median = |times: &[f64]| {
            let mut sorted = times.to_vec();
            sorted.sort_by(f64::total_cmp);
            sorted[sorted.len() / 2]
        };
        self.times.iter().map(|(label, times)| (*label, median(times))).collect()
    }
}

impl Slot {
    fn new(device: &wgpu::Device) -> Slot {
        let buffer = |usage| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("profile"),
                size: (MARKS_AT as u64 + u64::from(STAMPS)) * 8,
                usage,
                mapped_at_creation: false,
            })
        };
        let set = |count| {
            device.create_query_set(&wgpu::QuerySetDescriptor {
                label: Some("profile"),
                ty: wgpu::QueryType::Timestamp,
                count,
            })
        };
        Slot {
            set: set(STAMPS),
            shadow_set: set(2),
            shadow: false,
            resolved: buffer(wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC),
            read: buffer(wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ),
            reading: None,
        }
    }
}
