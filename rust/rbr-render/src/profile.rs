//! Time per stage of a frame (`viewer --frames --profile`), the median over the frames. A measuring tool.
//!
//! Where the device has timestamps inside encoders ([`wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS`], requested by
//! [`crate::renderer::request_gpu`]), each mark writes the GPU's clock, so the frame runs as usual and each span is the
//! GPU time between its marks. Otherwise, on macOS, or with `RBR_PROFILE=wait`, at each mark the frame's commands so
//! far are submitted and waited for, so the wall-clock time between two marks is the GPU time of the stage between
//! them, plus a submission's overhead: that slows the frame and lets the GPU clock down. (Metal samples its timestamps
//! without waiting for the passes before, which the tile GPUs run late and overlapped, so they say nothing there;
//! `RBR_PROFILE=stamps` uses them anyway.)

use std::time::{Duration, Instant};

/// Timestamps a frame can hold.
const STAMPS: u32 = 64;

pub(crate) struct Profiler {
    device: wgpu::Device,
    queue: wgpu::Queue,
    stamps: Option<Stamps>,
    /// When each mark of this frame finished (waiting marks).
    marks: Vec<Instant>,
    /// This frame's spans by their marks (timestamp marks), measured at [`Profiler::end_frame`].
    spans: Vec<(&'static str, usize, usize)>,
    /// Per label, in the order first seen: the milliseconds of each frame.
    times: Vec<(&'static str, Vec<f64>)>,
}

/// The query set the marks write and the buffers that read it back.
struct Stamps {
    set: wgpu::QuerySet,
    resolved: wgpu::Buffer,
    read: wgpu::Buffer,
    used: u32,
}

impl Profiler {
    pub(crate) fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Profiler {
        let timestamps = wgpu::Features::TIMESTAMP_QUERY | wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS;
        let wait = match std::env::var("RBR_PROFILE").as_deref() {
            Ok("wait") => true,
            Ok("stamps") => false,
            _ => cfg!(target_os = "macos"),
        };
        let stamps = (device.features().contains(timestamps) && !wait).then(|| {
            let size = u64::from(STAMPS) * 8;
            let buffer = |usage| {
                device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("profile"),
                    size,
                    usage,
                    mapped_at_creation: false,
                })
            };
            Stamps {
                set: device.create_query_set(&wgpu::QuerySetDescriptor {
                    label: Some("profile"),
                    ty: wgpu::QueryType::Timestamp,
                    count: STAMPS,
                }),
                resolved: buffer(wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC),
                read: buffer(wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ),
                used: 0,
            }
        });
        eprintln!("profile: {}", if stamps.is_some() { "GPU timestamps" } else { "waiting at each mark" });
        Profiler {
            device: device.clone(),
            queue: queue.clone(),
            stamps,
            marks: Vec::new(),
            spans: Vec::new(),
            times: Vec::new(),
        }
    }

    /// Marks the point `encoder` has reached and returns the mark's index, for [`Profiler::span`].
    pub(crate) fn mark(&mut self, encoder: &mut wgpu::CommandEncoder) -> usize {
        if let Some(stamps) = &mut self.stamps {
            let index = stamps.used.min(STAMPS - 1);
            encoder.write_timestamp(&stamps.set, index);
            stamps.used += 1;
            return index as usize;
        }
        let done = std::mem::replace(encoder, self.device.create_command_encoder(&Default::default()));
        self.queue.submit([done.finish()]);
        let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
        self.marks.push(Instant::now());
        self.marks.len() - 1
    }

    /// Ends the span `label` that started at mark `start`.
    pub(crate) fn span(&mut self, encoder: &mut wgpu::CommandEncoder, label: &'static str, start: usize) {
        let end = self.mark(encoder);
        match self.stamps {
            Some(_) => self.spans.push((label, start, end)),
            None => self.add(label, self.marks[end] - self.marks[start]),
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

    /// Ends the frame's marks: with timestamps, reads them back (the frame's commands are submitted by now).
    pub(crate) fn end_frame(&mut self) {
        self.marks.clear();
        let Some(stamps) = &mut self.stamps else { return };
        let used = std::mem::take(&mut stamps.used).min(STAMPS);
        if used == 0 {
            return;
        }
        let mut encoder = self.device.create_command_encoder(&Default::default());
        encoder.resolve_query_set(&stamps.set, 0..used, &stamps.resolved, 0);
        encoder.copy_buffer_to_buffer(&stamps.resolved, 0, &stamps.read, 0, u64::from(used) * 8);
        self.queue.submit([encoder.finish()]);
        let slice = stamps.read.slice(..u64::from(used) * 8);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
        let ticks: Vec<u64> = bytemuck::pod_collect_to_vec(&slice.get_mapped_range());
        stamps.read.unmap();
        let period = f64::from(self.queue.get_timestamp_period());
        for (label, start, end) in std::mem::take(&mut self.spans) {
            let nanos = ticks[end].saturating_sub(ticks[start]) as f64 * period;
            self.add(label, Duration::from_nanos(nanos as u64));
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
