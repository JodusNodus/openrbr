//! When the compositor put each presented frame on screen (Wayland's `wp_presentation` feedback on the window's
//! surface), for `RBR_LATENCY`. The feedback's clock is checked to be `CLOCK_MONOTONIC`, which [`now_ns`] reads too.

use std::os::raw::c_void;
use wayland_client::backend::{Backend, ObjectId};
use wayland_client::protocol::wl_registry::{self, WlRegistry};
use wayland_client::protocol::wl_surface::WlSurface;
use wayland_client::{Connection, Dispatch, EventQueue, Proxy, QueueHandle};
use wayland_protocols::wp::presentation_time::client::wp_presentation::{self, WpPresentation};
use wayland_protocols::wp::presentation_time::client::wp_presentation_feedback::{self, WpPresentationFeedback};
use winit::raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle};
use winit::window::Window;

/// One frame's feedback: on screen at `at` (ns, `CLOCK_MONOTONIC`) with the feedback's flags, or discarded.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Feedback {
    Shown { at: u64, flags: u32 },
    Discarded,
}

/// The feedback flags (`wp_presentation_feedback.kind`).
pub const VSYNC: u32 = 1;
pub const ZERO_COPY: u32 = 8;

/// The window's surface on a second handle to winit's Wayland connection, with the presentation global.
pub struct Shown {
    connection: Connection,
    queue: EventQueue<State>,
    state: State,
    surface: WlSurface,
}

#[derive(Default)]
struct State {
    presentation: Option<WpPresentation>,
    clock: Option<u32>,
    /// Frame number and feedback, in the order they came.
    feedback: Vec<(u64, Feedback)>,
}

impl Shown {
    /// The feedback for `window`, if it is a Wayland window and the compositor has `wp_presentation` on
    /// `CLOCK_MONOTONIC`.
    pub fn new(window: &Window) -> Result<Shown, String> {
        let (RawDisplayHandle::Wayland(display), RawWindowHandle::Wayland(surface)) = (
            window.display_handle().map_err(|e| e.to_string())?.as_raw(),
            window.window_handle().map_err(|e| e.to_string())?.as_raw(),
        ) else {
            return Err("not a Wayland window".into());
        };
        // Safety: winit's display and surface outlive the window, which outlives this.
        let backend = unsafe { Backend::from_foreign_display(display.display.as_ptr().cast()) };
        let connection = Connection::from_backend(backend);
        let id =
            unsafe { ObjectId::from_ptr(WlSurface::interface(), surface.surface.as_ptr().cast::<c_void>().cast()) }
                .map_err(|e| e.to_string())?;
        let surface = WlSurface::from_id(&connection, id).map_err(|e| e.to_string())?;
        let mut queue = connection.new_event_queue();
        connection.display().get_registry(&queue.handle(), ());
        let mut state = State::default();
        // The globals, then the presentation's clock.
        for _ in 0..2 {
            queue.roundtrip(&mut state).map_err(|e| e.to_string())?;
        }
        match state.clock {
            _ if state.presentation.is_none() => Err("no wp_presentation".into()),
            Some(CLOCK_MONOTONIC) => Ok(Shown { connection, queue, state, surface }),
            clock => Err(format!("wp_presentation on clock {clock:?}, not CLOCK_MONOTONIC")),
        }
    }

    /// Asks for the feedback of the surface's next commit (the present that follows) as frame `frame`.
    pub fn ask(&mut self, frame: u64) {
        if let Some(presentation) = &self.state.presentation {
            presentation.feedback(&self.surface, &self.queue.handle(), frame);
        }
        let _ = self.connection.flush();
    }

    /// The feedback that came since the last call.
    pub fn take(&mut self) -> Vec<(u64, Feedback)> {
        let _ = self.connection.flush();
        if let Some(guard) = self.connection.prepare_read() {
            // Reads what is waiting without blocking (winit's loop reads the socket too).
            let _ = guard.read_without_dispatch();
        }
        let _ = self.queue.dispatch_pending(&mut self.state);
        std::mem::take(&mut self.state.feedback)
    }
}

const CLOCK_MONOTONIC: u32 = 1;

/// `CLOCK_MONOTONIC` now, ns.
pub fn now_ns() -> u64 {
    let mut time = libc::timespec { tv_sec: 0, tv_nsec: 0 };
    // Safety: a valid timespec out pointer.
    unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut time) };
    time.tv_sec as u64 * 1_000_000_000 + time.tv_nsec as u64
}

impl Dispatch<WlRegistry, ()> for State {
    fn event(
        state: &mut State,
        registry: &WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<State>,
    ) {
        if let wl_registry::Event::Global { name, interface, .. } = event {
            if interface == WpPresentation::interface().name {
                state.presentation = Some(registry.bind(name, 1, qh, ()));
            }
        }
    }
}

impl Dispatch<WpPresentation, ()> for State {
    fn event(
        state: &mut State,
        _: &WpPresentation,
        event: wp_presentation::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<State>,
    ) {
        if let wp_presentation::Event::ClockId { clk_id } = event {
            state.clock = Some(clk_id);
        }
    }
}

impl Dispatch<WpPresentationFeedback, u64> for State {
    fn event(
        state: &mut State,
        _: &WpPresentationFeedback,
        event: wp_presentation_feedback::Event,
        frame: &u64,
        _: &Connection,
        _: &QueueHandle<State>,
    ) {
        let feedback = match event {
            wp_presentation_feedback::Event::Presented { tv_sec_hi, tv_sec_lo, tv_nsec, flags, .. } => {
                let seconds = u64::from(tv_sec_hi) << 32 | u64::from(tv_sec_lo);
                let flags = match flags {
                    wayland_client::WEnum::Value(kind) => kind.bits(),
                    wayland_client::WEnum::Unknown(bits) => bits,
                };
                Feedback::Shown { at: seconds * 1_000_000_000 + u64::from(tv_nsec), flags }
            }
            wp_presentation_feedback::Event::Discarded => Feedback::Discarded,
            _ => return,
        };
        state.feedback.push((*frame, feedback));
    }
}
