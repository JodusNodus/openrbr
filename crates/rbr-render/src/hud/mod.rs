//! The viewer's HUD: what it shows, a [`Readout`] (with the live HUD's state from [`model`]), and a look that paints
//! it, any [`Hud`]. [`Overlay`] draws the HUD over the frame with egui, through `egui-wgpu`, on a screen
//! [`REFERENCE_HEIGHT`] points high so that it scales with the window's height. [`stock::Stock`] is ours; another look
//! implements [`Hud`] and goes to [`Overlay::with_hud`] in its place.
//!
//! The readout carries the drive time (playback), speed (km/h or mph), gear, engine speed and the controls fed to the
//! step; and in live mode on `rbr-standalone` the game HUD's information: the race clock and the countdown, the split
//! record, the pace notes, the warning lamps, turbo and water temperature, the car's damage, the messages, the stage
//! progress and odometer, "WRONG WAY", the rally's false-start warning, the pace stack and the 3D pace notes (both off
//! by default, as in 1.02). The information and its timing follow the 1.02 HUD (`model`); any look can use the
//! pace-note symbols (`pace`), the split comparison and result texts (`splits`), and the clock, the countdown's calls
//! and the warnings' texts here. A recorded drive has none of the live records, so those are empty.
//!
//! Over any HUD the viewer draws the race's own screens, which are not the HUD's: a retire's tints, message boxes and
//! FAILED page (`retire`, through [`paint_readout`]) and the pause menu (`pause`) with its Camera page
//! (`camera_page`).

use egui::{Pos2, Rect, Vec2};
use glam::Mat4;

use rbr_recording::splits::Splits;

use crate::live::RaceHud;

pub mod camera_page;
pub mod model;
pub mod pace;
pub mod pause;
pub mod results;
pub mod retire;
pub mod splits;
pub mod stock;

use model::HudView;

/// The screen's height in points; one point is `height / REFERENCE_HEIGHT` pixels.
pub const REFERENCE_HEIGHT: f32 = 720.0;
/// m/s to km/h and to mph (the `CSigCarSensorData` receiver's factors, 3.6 and 2.2369363).
const KMH: f32 = 3.6;
const MPH: f32 = 2.236_936_3;

/// A look for the HUD.
pub trait Hud {
    /// Paints `readout` on `ctx`, whose screen is [`REFERENCE_HEIGHT`] points high, keeping to the parts it shows
    /// ([`Readout::parts`]). The race's screens are drawn over it ([`paint_readout`]).
    fn paint(&mut self, ctx: &egui::Context, readout: &Readout);
}

/// A part of the HUD the player can hide (`viewer --hud-hide`). The race's calls and warnings (the countdown, FALSE
/// START, WRONG WAY, the rally's false-start warning) and the finish, result and retire pages always
/// show with the HUD.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Part {
    /// The race clock and its penalties.
    Clock,
    /// The progress bar, the odometer and the distance to go.
    Progress,
    /// The pace-note symbol of each call.
    Pace,
    /// A passed split's time and difference.
    Splits,
    /// The rev dial, gear, speed, engine speed, water temperature and turbo pressure.
    Dash,
    /// The pedals and the steering.
    Inputs,
    /// The lit warning lamps and the damaged parts.
    Car,
    /// The stand-still and car-problem messages.
    Messages,
    /// The camera view's name.
    View,
}

impl Part {
    pub const ALL: [Part; 9] = [
        Part::Clock,
        Part::Progress,
        Part::Pace,
        Part::Splits,
        Part::Dash,
        Part::Inputs,
        Part::Car,
        Part::Messages,
        Part::View,
    ];

    /// The part's name in `--hud-hide`.
    pub const fn name(self) -> &'static str {
        match self {
            Part::Clock => "clock",
            Part::Progress => "progress",
            Part::Pace => "pace",
            Part::Splits => "splits",
            Part::Dash => "dash",
            Part::Inputs => "inputs",
            Part::Car => "car",
            Part::Messages => "messages",
            Part::View => "view",
        }
    }
}

/// Whether each [`Part`] shows, by its index in [`Part::ALL`]; all by default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Parts(pub [bool; Part::ALL.len()]);

impl Default for Parts {
    fn default() -> Self {
        Parts([true; Part::ALL.len()])
    }
}

impl Parts {
    pub fn shows(&self, part: Part) -> bool {
        self.0[part as usize]
    }

    /// Every part but those `list` names, comma-separated (`--hud-hide`).
    pub fn hiding(list: &str) -> Result<Parts, String> {
        let mut parts = Parts::default();
        for name in list.split(',').filter(|n| !n.is_empty()) {
            let part = Part::ALL.into_iter().find(|p| p.name() == name).ok_or(format!("no HUD part {name}"))?;
            parts.0[part as usize] = false;
        }
        Ok(parts)
    }

    /// The hidden parts' names, comma-separated, or `None` when all show.
    pub fn hidden(&self) -> Option<String> {
        let names: Vec<&str> = Part::ALL.into_iter().filter(|p| !self.shows(*p)).map(Part::name).collect();
        (!names.is_empty()).then(|| names.join(","))
    }
}

/// What the HUD shows.
#[derive(Debug, Clone, PartialEq)]
pub struct Readout {
    /// The camera view's name.
    pub view: &'static str,
    /// The binding that changes it (`C`), or empty.
    pub view_keys: String,
    /// Drive time at the shown frame, seconds: shown only without a race clock (playback).
    pub time: f64,
    /// The car's speed, m/s.
    pub speed: f32,
    /// Show the speed in mph instead of km/h (the game's speed unit option, global 0x1660d00).
    pub mph: bool,
    /// Rendered frames per second, smoothed over about half a second by the viewer. Ours: the original
    /// has no FPS counter. `None` hides it.
    pub fps: Option<f32>,
    /// The dashboard gear (R, N, 1, ...), when the source has it.
    pub gear: Option<String>,
    pub rpm: Option<f32>,
    /// The controls fed to the step: pedals 0 to 1, steering -1 (left) to 1 (right).
    pub throttle: f32,
    pub brake: f32,
    pub handbrake: f32,
    pub clutch: f32,
    pub steering: f32,
    /// The live process's race clock (HUD2 records only).
    pub race: Option<RaceHud>,
    /// The live process's split record, when it sends one (`splits`).
    pub splits: Option<Splits>,
    /// The live HUD's state (`model`); empty in playback.
    pub state: HudView,
    /// The 3D pace notes (`AID_MARKER`, default off): the frame's stage-space view-projection to place them with.
    pub pace_3d: Option<Mat4>,
    /// The parts shown.
    pub parts: Parts,
}

impl Readout {
    /// The clock: the race clock ([`Clock::race`]) in live play, and in playback, which has no race clock, the drive
    /// time (the recording's steps of 7 ms from its first frame).
    pub fn clock(&self) -> Clock {
        match &self.race {
            Some(race) => Clock::race(race),
            None => Clock { title: "drive time", time: clock_label(self.time), notes: Vec::new() },
        }
    }

    /// The speed in the chosen unit, and the unit's label.
    pub fn speed_in_unit(&self) -> (f32, &'static str) {
        if self.mph {
            (self.speed * MPH, "mph")
        } else {
            (self.speed * KMH, "km/h")
        }
    }
}

/// The clock's lines: its title, the time (`mm:ss.mmm`) and its notes, each a name and an amount.
#[derive(Debug, Clone, PartialEq)]
pub struct Clock {
    pub title: &'static str,
    pub time: String,
    pub notes: Vec<(&'static str, String)>,
}

impl Clock {
    /// The race clock: the race time, held at 0 until the go (the countdown has its own calls, [`countdown_call`]),
    /// then the penalties and the jump start.
    pub fn race(race: &RaceHud) -> Clock {
        let time = if !race.running && race.countdown > 0.0 { 0.0 } else { race.time };
        let mut notes = Vec::new();
        if race.penalties > 0.0 {
            notes.push(("penalties", format!("+{:.0} s", race.penalties)));
        }
        if race.jump_start {
            notes.push(("jump start", "+10 s".to_owned()));
        }
        Clock { title: "race time", time: clock_label(f64::from(time)), notes }
    }
}

/// `mm:ss.mmm`.
pub fn clock_label(seconds: f64) -> String {
    let ms = (seconds.max(0.0) * 1000.0).round() as u64;
    format!("{:02}:{:02}.{:03}", ms / 60_000, ms / 1000 % 60, ms % 1000)
}

/// `m:ss.cc`.
pub fn time_label(seconds: f64) -> String {
    let hundredths = (seconds.max(0.0) * 100.0).round() as u64;
    format!("{}:{:02}.{:02}", hundredths / 6000, hundredths / 100 % 60, hundredths % 100)
}

/// The 1.02 English texts (`bstrings.eng`) of the warnings.
pub const WRONG_WAY: &str = "WRONG WAY";
/// `FSWAR_1` to `FSWAR_3`, by the false starts so far.
pub const FALSE_START_WARNINGS: [&str; 3] = [
    "Three more false starts and you will be excluded from the rally.",
    "This is your second false start, two more will lead to exclusion.",
    "One more false start and you are excluded from the rally!",
];

/// The countdown's big call: "3", "2", "1" in the last three seconds and "GO" for two seconds after the go (the
/// countdown sounds' windows in `HUD::Update`); "Get ready!" before them (ours: the game's message names the profile).
pub fn countdown_call(race: &RaceHud) -> Option<String> {
    if !race.running && race.countdown > 3.0 {
        Some("Get ready!".to_owned())
    } else if !race.running && race.countdown > 0.0 {
        Some(format!("{}", race.countdown.ceil() as u32))
    } else if race.running && race.countdown > -2.0 && !race.jump_start {
        Some("GO".to_owned())
    } else {
        None
    }
}

/// egui and its wgpu renderer for one target format, with the HUD it draws.
pub struct Overlay {
    context: egui::Context,
    renderer: egui_wgpu::Renderer,
    hud: Box<dyn Hud>,
}

impl Overlay {
    /// The overlay with our HUD ([`stock::Stock`]).
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Overlay {
        Overlay::with_hud(device, format, Box::new(stock::Stock))
    }

    pub fn with_hud(device: &wgpu::Device, format: wgpu::TextureFormat, hud: Box<dyn Hud>) -> Overlay {
        Overlay {
            context: egui::Context::default(),
            renderer: egui_wgpu::Renderer::new(device, format, Default::default()),
            hud,
        }
    }

    /// Draws the HUD for `readout`, with the race's screens over it, over `target` (`size` pixels), keeping what is
    /// there.
    pub fn draw(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        size: (u32, u32),
        readout: &Readout,
    ) {
        self.draw_with(device, queue, encoder, target, size, |ctx, hud| paint_readout(ctx, hud, readout));
    }

    /// Draws what `ui` paints, given the HUD, over `target` (`size` pixels), keeping what is there.
    pub fn draw_with(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        size: (u32, u32),
        mut ui: impl FnMut(&egui::Context, &mut dyn Hud),
    ) {
        let scale = size.1.max(1) as f32 / REFERENCE_HEIGHT;
        let mut input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(size.0 as f32, size.1 as f32) / scale)),
            ..Default::default()
        };
        input.viewports.entry(egui::ViewportId::ROOT).or_default().native_pixels_per_point = Some(scale);
        let hud = &mut *self.hud;
        let mut output = self.context.run_pass(input, |ctx| ui(ctx, hud));
        let primitives = self.context.tessellate(output.shapes, output.pixels_per_point);
        let screen =
            egui_wgpu::ScreenDescriptor { size_in_pixels: [size.0, size.1], pixels_per_point: output.pixels_per_point };
        for (id, deltas) in output.textures_delta.set.drain() {
            for delta in deltas {
                self.renderer.update_texture(device, queue, id, &delta);
            }
        }
        // Only paint callbacks return command buffers, and the HUD has none.
        self.renderer.update_buffers(device, queue, encoder, &primitives, &screen);
        let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("hud"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
            })],
            ..Default::default()
        });
        self.renderer.render(&mut pass.forget_lifetime(), &primitives, &screen);
        // Applied (egui checks that no delta is dropped unapplied).
        for id in output.textures_delta.free.drain() {
            self.renderer.free_texture(&id);
        }
    }
}

/// `hud`'s HUD for `readout` and the retire's screens over it.
pub fn paint_readout(ctx: &egui::Context, hud: &mut dyn Hud, readout: &Readout) {
    hud.paint(ctx, readout);
    retire::paint(ctx, &readout.state, readout.splits.as_ref());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times_read_minutes_seconds_and_hundredths() {
        assert_eq!(time_label(0.0), "0:00.00");
        assert_eq!(time_label(12.345), "0:12.35");
        assert_eq!(time_label(61.0), "1:01.00");
        assert_eq!(time_label(3599.999), "60:00.00");
        assert_eq!(time_label(-1.0), "0:00.00");
    }

    #[test]
    fn hidden_parts_round_trip_through_their_list() {
        assert_eq!(Parts::hiding(""), Ok(Parts::default()));
        assert_eq!(Parts::default().hidden(), None);
        let parts = Parts::hiding("inputs,clock").unwrap();
        assert!(!parts.shows(Part::Clock) && !parts.shows(Part::Inputs) && parts.shows(Part::Dash));
        assert_eq!(parts.hidden().as_deref(), Some("clock,inputs"));
        assert!(Parts::hiding("speedo").is_err());
    }

    #[test]
    fn the_race_clock_holds_at_zero_until_the_go_then_shows_the_time_and_its_penalties() {
        let start = RaceHud { countdown: 3.24, ..Default::default() };
        assert_eq!(Clock::race(&start), Clock { title: "race time", time: "00:00.000".to_owned(), notes: vec![] });
        let assisted = RaceHud { time: 75.5, countdown: -60.0, penalties: 10.0, running: true, jump_start: true };
        assert_eq!(
            Clock::race(&assisted),
            Clock {
                title: "race time",
                time: "01:15.500".to_owned(),
                notes: vec![("penalties", "+10 s".to_owned()), ("jump start", "+10 s".to_owned())]
            }
        );
    }
}
