//! The viewer's HUD in playback and live mode, drawn over the frame with egui: the drive time (playback), speed (km/h
//! or mph), gear, engine speed with the shift light and the controls fed to the step; and in live mode on
//! `rbr-standalone` the game HUD's information: the race clock (the start countdown with 3, 2, 1, GO, the race time
//! with its penalties, the false start), the split comparison and the finish (`splits`), the pace-note symbol of each
//! call (`pace`, `model`), the warning lamps, the turbo and water temperature, the stand-still and car-problem messages,
//! the stage progress bar, the odometer and distance to go, the start board's clock, "WRONG WAY", the rally's
//! false-start warning, the pace stack and the 3D pace notes (both off by default, as in 1.02), a retire's tints,
//! message box and FAILED page (`retire`), and the pause menu (`pause`). A recorded drive has none of the live
//! records, so those parts are left out.
//!
//! The information and its timing follow the 1.02 HUD (`model`, `lead/reports/hud-1.md`); the look is ours, not the
//! minidash or digidash art.
//!
//! egui (already in the workspace for `rbr-launcher`) draws the text with its own font and the bars and panels as
//! shapes, through `egui-wgpu` into the frame the renderer drew. The layout is in points on a screen
//! [`REFERENCE_HEIGHT`] points high, so it scales with the window's height.

use egui::{Align2, Color32, FontId, Pos2, Rect, Vec2};
use glam::{Mat4, Vec3};

use rbr_recording::splits::Splits;

use crate::live::RaceHud;

pub mod model;
pub mod pace;
pub mod pause;
pub mod retire;
pub mod splits;

use model::{HudView, Lamp, StackNote, LAMPS};

/// The screen's height in points; one point is `height / REFERENCE_HEIGHT` pixels.
pub const REFERENCE_HEIGHT: f32 = 720.0;
/// The engine speed at the end of the rpm bar. Ours.
pub const RPM_SCALE: f32 = 10_000.0;
/// The digidash's shift light comes on at this engine speed (`E57DashData::UpdateFull`, 5500 rpm for every car).
pub const SHIFT_RPM: f32 = 5500.0;
/// m/s to km/h and to mph (the `CSigCarSensorData` receiver's factors, 3.6 and 2.2369363).
const KMH: f32 = 3.6;
const MPH: f32 = 2.236_936_3;
const LAMP_ON: Color32 = Color32::from_rgb(235, 60, 50);
const ALERT: Color32 = Color32::from_rgb(240, 160, 60);

/// Margin, padding and bar sizes, points.
const MARGIN: f32 = 16.0;
const PAD: f32 = 10.0;
const BAR_WIDTH: f32 = 180.0;
const BAR_HEIGHT: f32 = 12.0;
const PANEL: Color32 = Color32::from_rgba_premultiplied(0, 0, 0, 150);
const TEXT: Color32 = Color32::from_gray(235);
const DIM: Color32 = Color32::from_gray(160);
const TRACK: Color32 = Color32::from_gray(60);

/// What the HUD shows.
#[derive(Debug, Clone, PartialEq)]
pub struct Readout {
    /// The camera view's name.
    pub view: &'static str,
    /// The bindings that change and adjust it (`C, X adjusts`), or empty.
    pub view_keys: String,
    /// The camera adjust mode's line while it runs (`crate::camera_prefs::Session::hud_line`).
    pub adjust: Option<String>,
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
}

impl Readout {
    /// The speed in the chosen unit, and the unit's label.
    pub fn speed_in_unit(&self) -> (f32, &'static str) {
        if self.mph {
            (self.speed * MPH, "mph")
        } else {
            (self.speed * KMH, "km/h")
        }
    }
}

/// The race clock's lines: the countdown before the go, else the race time; then the penalties and the jump start.
pub fn race_lines(race: &RaceHud) -> (String, String, Vec<String>) {
    let (title, value) = if !race.running && race.countdown > 0.0 {
        ("start in", format!("{:.1}", race.countdown))
    } else {
        ("race time", time_label(f64::from(race.time)))
    };
    let mut notes = Vec::new();
    if race.penalties > 0.0 {
        notes.push(format!("penalties +{:.0} s", race.penalties));
    }
    if race.jump_start {
        notes.push("jump start +10 s".to_owned());
    }
    (title.to_owned(), value, notes)
}

/// The clock the HUD shows: the race clock's lines ([`race_lines`]) in live play, and in playback, which has no race
/// clock, the drive time (the recording's steps of 7 ms from its first frame).
fn time_lines(r: &Readout) -> (String, String, Vec<String>) {
    match &r.race {
        Some(race) => race_lines(race),
        None => ("drive time".to_owned(), time_label(r.time), Vec::new()),
    }
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

/// A distance in km (or miles with `mph`), two decimals: `12.34 km`. The unit follows the speed's (ours).
pub fn distance_label(metres: f32, mph: bool) -> String {
    if mph {
        format!("{:.2} mi", metres / 1609.344)
    } else {
        format!("{:.2} km", metres / 1000.0)
    }
}

/// The start board's `hh:mm:ss` (the digits `start_clock_digits.dds` shows: hours, minutes, seconds, each truncated).
pub fn clock_label(seconds: f32) -> String {
    let s = seconds.max(0.0) as u32;
    format!("{:02}:{:02}:{:02}", s / 3600 % 24, s / 60 % 60, s % 60)
}

/// egui and its wgpu renderer for one target format.
pub struct Overlay {
    context: egui::Context,
    renderer: egui_wgpu::Renderer,
}

impl Overlay {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Overlay {
        Overlay {
            context: egui::Context::default(),
            renderer: egui_wgpu::Renderer::new(device, format, Default::default()),
        }
    }

    /// Draws `readout` over `target` (`size` pixels), keeping what is there.
    pub fn draw(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        size: (u32, u32),
        readout: &Readout,
    ) {
        self.draw_with(device, queue, encoder, target, size, |ctx| paint_readout(ctx, readout));
    }

    /// Draws what `ui` paints over `target` (`size` pixels), keeping what is there.
    pub fn draw_with(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        size: (u32, u32),
        ui: impl FnMut(&egui::Context),
    ) {
        let scale = size.1.max(1) as f32 / REFERENCE_HEIGHT;
        let mut input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(size.0 as f32, size.1 as f32) / scale)),
            ..Default::default()
        };
        input.viewports.entry(egui::ViewportId::ROOT).or_default().native_pixels_per_point = Some(scale);
        let output = self.context.run(input, ui);
        let primitives = self.context.tessellate(output.shapes, output.pixels_per_point);
        let screen =
            egui_wgpu::ScreenDescriptor { size_in_pixels: [size.0, size.1], pixels_per_point: output.pixels_per_point };
        for (id, delta) in &output.textures_delta.set {
            self.renderer.update_texture(device, queue, *id, delta);
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
        for id in &output.textures_delta.free {
            self.renderer.free_texture(id);
        }
    }
}

/// The HUD and the retire's screens over it.
pub fn paint_readout(ctx: &egui::Context, readout: &Readout) {
    paint(ctx, readout);
    retire::paint(ctx, &readout.state, readout.splits.as_ref());
}

/// The camera adjust mode's line at the top middle; also drawn with the HUD hidden.
pub fn paint_adjust(ctx: &egui::Context, line: &str) {
    let painter = ctx.layer_painter(egui::LayerId::background());
    let at = Pos2::new(ctx.content_rect().center().x, ctx.content_rect().min.y + MARGIN);
    let galley = painter.layout_no_wrap(line.to_owned(), FontId::proportional(15.0), TEXT);
    let rect = Rect::from_center_size(
        at + Vec2::new(0.0, galley.size().y * 0.5 + PAD),
        galley.size() + Vec2::splat(2.0 * PAD),
    );
    painter.rect_filled(rect, 6.0, PANEL);
    painter.galley(rect.min + Vec2::splat(PAD), galley, TEXT);
}

/// The HUD's shapes: the time and view at the top left, the controls at the bottom left, and the speed, gear and
/// engine speed at the bottom right.
fn paint(ctx: &egui::Context, r: &Readout) {
    let painter = ctx.layer_painter(egui::LayerId::background());
    let screen = ctx.content_rect();
    let font = |size| FontId::proportional(size);
    let mono = |size| FontId::monospace(size);

    // Top left: the race clock when the source has one (live), else the drive time; then the view.
    let (title, value, notes) = time_lines(r);
    let panel = Rect::from_min_size(
        screen.min + Vec2::splat(MARGIN),
        Vec2::new(2.0 * PAD + 170.0, 2.0 * PAD + 64.0 + 16.0 * notes.len() as f32),
    );
    painter.rect_filled(panel, 6.0, PANEL);
    let mut at = panel.min + Vec2::splat(PAD);
    painter.text(at, Align2::LEFT_TOP, title, font(13.0), DIM);
    at.y += 16.0;
    painter.text(at, Align2::LEFT_TOP, value, mono(26.0), TEXT);
    at.y += 32.0;
    for note in notes {
        painter.text(at, Align2::LEFT_TOP, note, font(13.0), Color32::from_rgb(240, 160, 60));
        at.y += 16.0;
    }
    let keys = if r.view_keys.is_empty() { String::new() } else { format!(" ({})", r.view_keys) };
    painter.text(at, Align2::LEFT_TOP, format!("view: {}{keys}", r.view), font(13.0), DIM);
    if let Some(adjust) = &r.adjust {
        paint_adjust(ctx, adjust);
    }

    // Top right: the rendered frames per second, when the viewer measures them. Ours.
    if let Some(fps) = r.fps {
        painter.text(
            Pos2::new(screen.max.x - MARGIN, screen.min.y + MARGIN),
            Align2::RIGHT_TOP,
            format!("{fps:.0} fps"),
            font(13.0),
            DIM,
        );
    }

    if let Some(record) = &r.splits {
        splits::paint(&painter, screen, record, r.state.end_page);
    }
    // `HUD::Update` draws nothing while the rescue helicopter flies; the caption is ours.
    if r.state.helicopter && r.state.end_page.is_none() {
        let at = Pos2::new(screen.center().x, screen.min.y + MARGIN);
        painter.text(at, Align2::CENTER_TOP, "rescue helicopter called", FontId::proportional(15.0), DIM);
        return;
    }
    paint_live(&painter, screen, r);

    // Bottom left: the controls.
    let rows = [
        ("throttle", r.throttle, Color32::from_rgb(80, 200, 90)),
        ("brake", r.brake, Color32::from_rgb(220, 70, 60)),
        ("handbrake", r.handbrake, Color32::from_rgb(230, 160, 50)),
        ("clutch", r.clutch, Color32::from_rgb(90, 150, 230)),
    ];
    let row = BAR_HEIGHT + 8.0;
    let label = 72.0;
    let panel = Rect::from_min_size(
        Pos2::new(screen.min.x + MARGIN, screen.max.y - MARGIN - 2.0 * PAD - row * 5.0),
        Vec2::new(2.0 * PAD + label + BAR_WIDTH, 2.0 * PAD + row * 5.0),
    );
    painter.rect_filled(panel, 6.0, PANEL);
    let bar_rect = |i: usize| {
        Rect::from_min_size(
            panel.min + Vec2::new(PAD + label, PAD + row * i as f32 + 4.0),
            Vec2::new(BAR_WIDTH, BAR_HEIGHT),
        )
    };
    for (i, (name, value, color)) in rows.into_iter().enumerate() {
        let bar = bar_rect(i);
        painter.text(Pos2::new(panel.min.x + PAD, bar.center().y), Align2::LEFT_CENTER, name, font(13.0), DIM);
        painter.rect_filled(bar, 2.0, TRACK);
        let filled = Rect::from_min_size(bar.min, Vec2::new(bar.width() * value.clamp(0.0, 1.0), bar.height()));
        painter.rect_filled(filled, 2.0, color);
    }
    let bar = bar_rect(4);
    painter.text(Pos2::new(panel.min.x + PAD, bar.center().y), Align2::LEFT_CENTER, "steer", font(13.0), DIM);
    painter.rect_filled(bar, 2.0, TRACK);
    let centre = bar.center().x;
    let end = centre + bar.width() * 0.5 * r.steering.clamp(-1.0, 1.0);
    painter.rect_filled(Rect::from_x_y_ranges(centre.min(end)..=centre.max(end), bar.y_range()), 2.0, TEXT);
    painter.line_segment([Pos2::new(centre, bar.min.y - 2.0), Pos2::new(centre, bar.max.y + 2.0)], (1.0, DIM));

    // Bottom right: gear, speed and engine speed.
    let size = Vec2::new(2.0 * PAD + 250.0, 2.0 * PAD + 96.0);
    let panel = Rect::from_min_size(screen.max - Vec2::splat(MARGIN) - size, size);
    painter.rect_filled(panel, 6.0, PANEL);
    let inner = panel.shrink(PAD);
    painter.text(inner.left_top(), Align2::LEFT_TOP, r.gear.as_deref().unwrap_or("-"), font(60.0), TEXT);
    painter.text(
        Pos2::new(inner.max.x - 52.0, inner.min.y),
        Align2::RIGHT_TOP,
        format!("{:.0}", r.speed_in_unit().0),
        mono(52.0),
        TEXT,
    );
    painter.text(Pos2::new(inner.max.x, inner.min.y + 30.0), Align2::RIGHT_TOP, r.speed_in_unit().1, font(16.0), DIM);
    if r.rpm.is_some_and(|rpm| rpm >= SHIFT_RPM) {
        painter.circle_filled(Pos2::new(inner.max.x - 8.0, inner.min.y + 8.0), 6.0, LAMP_ON);
    }
    let bar = Rect::from_min_size(
        Pos2::new(inner.min.x, inner.max.y - BAR_HEIGHT),
        Vec2::new(inner.width() - 90.0, BAR_HEIGHT),
    );
    painter.rect_filled(bar, 2.0, TRACK);
    if let Some(rpm) = r.rpm {
        let filled =
            Rect::from_min_size(bar.min, Vec2::new(bar.width() * (rpm / RPM_SCALE).clamp(0.0, 1.0), bar.height()));
        painter.rect_filled(filled, 2.0, Color32::from_rgb(240, 200, 60));
    }
    painter.text(
        Pos2::new(inner.max.x, bar.center().y),
        Align2::RIGHT_CENTER,
        match r.rpm {
            Some(rpm) => format!("{rpm:.0} rpm"),
            None => "- rpm".to_owned(),
        },
        font(14.0),
        DIM,
    );
}

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

/// Big centred text over a soft shadow, readable on a bright sky.
fn shadowed(painter: &egui::Painter, at: Pos2, text: String, font: FontId, colour: Color32) {
    let shadow = Color32::from_black_alpha(colour.a() / 2);
    painter.text(at + Vec2::splat(2.0), Align2::CENTER_CENTER, &text, font.clone(), shadow);
    painter.text(at, Align2::CENTER_CENTER, text, font, colour);
}

/// The live HUD's parts: the progress bar, the countdown, the false start, the pace note, the message and the lamps.
fn paint_live(painter: &egui::Painter, screen: Rect, r: &Readout) {
    let s = &r.state;
    let font = FontId::proportional;
    if let Some((car, splits)) = &s.progress {
        let bar =
            Rect::from_min_size(Pos2::new(screen.min.x + MARGIN, 4.0), Vec2::new(screen.width() - 2.0 * MARGIN, 4.0));
        painter.rect_filled(bar, 2.0, Color32::from_rgba_unmultiplied(60, 60, 60, 160));
        painter.rect_filled(Rect::from_min_size(bar.min, Vec2::new(bar.width() * car, bar.height())), 2.0, TEXT);
        for split in splits {
            let x = bar.min.x + bar.width() * split;
            painter.line_segment([Pos2::new(x, bar.min.y - 2.0), Pos2::new(x, bar.max.y + 2.0)], (2.0, ALERT));
        }
    }
    let centre = screen.center().x;
    if let Some(call) = r.race.as_ref().and_then(countdown_call) {
        let size = if call.len() > 2 { 44.0 } else { 96.0 };
        let colour = if call == "GO" { Color32::from_rgb(90, 220, 100) } else { TEXT };
        shadowed(painter, Pos2::new(centre, screen.height() * 0.3), call, font(size), colour);
    }
    if let Some(alpha) = s.false_start {
        let colour = Color32::from_rgba_unmultiplied(235, 60, 50, (alpha * 255.0) as u8);
        shadowed(painter, Pos2::new(centre, screen.height() * 0.42), "FALSE START".into(), font(48.0), colour);
    }
    if let Some((note, alpha)) = &s.note {
        let panel = Rect::from_min_size(Pos2::new(centre - 60.0, screen.min.y + MARGIN), Vec2::new(120.0, 124.0));
        painter.rect_filled(panel, 8.0, Color32::from_rgba_unmultiplied(0, 0, 0, (150.0 * alpha) as u8));
        let symbol = Rect::from_center_size(Pos2::new(centre, panel.min.y + 8.0 + pace::SIZE.y / 2.0), pace::SIZE);
        pace::paint(painter, symbol, &note.symbol, *alpha);
        let ink = |c: Color32| c.gamma_multiply(*alpha);
        let mut line = pace::label(&note.symbol).to_owned();
        if let Some(word) = pace::modifier(note.flags) {
            line = format!("{line}, {word}");
        }
        painter.text(Pos2::new(centre, symbol.max.y + 4.0), Align2::CENTER_TOP, line, font(13.0), ink(TEXT));
        // The called distance (the game's DISTANCE option) and the countdown to the note (DIST_CNTDWN), both snapped
        // as the game does.
        if note.called > 0.0 {
            let called = format!("{:.0}", rbr_sound::codriver::snap(note.called));
            painter.text(panel.right_top() + Vec2::new(-6.0, 4.0), Align2::RIGHT_TOP, called, font(13.0), ink(DIM));
        }
        if let Some(d) = s.note_distance.filter(|d| *d > 0.0) {
            let left = format!("{:.0} m", rbr_sound::codriver::snap(d));
            painter.text(panel.left_top() + Vec2::new(6.0, 4.0), Align2::LEFT_TOP, left, font(13.0), ink(DIM));
        }
    }
    if let Some((message, alpha)) = s.message {
        let lines = message.text().lines().count() as f32;
        let panel = Rect::from_center_size(
            Pos2::new(centre, screen.height() * 0.62),
            Vec2::new(380.0, 2.0 * PAD + 20.0 * lines),
        );
        painter.rect_filled(panel, 6.0, Color32::from_rgba_unmultiplied(0, 0, 0, (170.0 * alpha) as u8));
        painter.text(panel.center(), Align2::CENTER_CENTER, message.text(), font(17.0), ALERT.gamma_multiply(alpha));
    }
    // A race clock that stopped with time on it is the finish's, not the start's.
    if let Some(seconds) = s.start_clock.filter(|_| r.race.is_none_or(|race| !race.running && race.time == 0.0)) {
        // The start board's clock until the go, also drawn on the board itself (`crate::start_board`); the panel under
        // the countdown is ours.
        let panel = Rect::from_center_size(Pos2::new(centre, screen.height() * 0.3 + 70.0), Vec2::new(150.0, 46.0));
        painter.rect_filled(panel, 6.0, PANEL);
        painter.text(panel.center_top() + Vec2::new(0.0, 4.0), Align2::CENTER_TOP, "start clock", font(11.0), DIM);
        let at = panel.center_bottom() - Vec2::new(0.0, 4.0);
        painter.text(at, Align2::CENTER_BOTTOM, clock_label(seconds), FontId::monospace(22.0), ALERT);
    }
    if let Some(alpha) = s.wrong_way {
        let colour = Color32::from_rgba_unmultiplied(235, 60, 50, (alpha * 255.0) as u8);
        let panel = Rect::from_center_size(Pos2::new(centre, screen.height() * 0.5), Vec2::new(300.0, 64.0));
        painter.rect_filled(panel, 8.0, Color32::from_black_alpha((170.0 * alpha) as u8));
        painter.rect_stroke(panel, 8.0, (2.0, colour), egui::StrokeKind::Inside);
        painter.text(panel.center(), Align2::CENTER_CENTER, WRONG_WAY, font(40.0), colour);
    }
    if let Some((count, alpha)) = s.warning {
        let text = FALSE_START_WARNINGS[usize::from(count.clamp(1, 3)) - 1];
        let panel = Rect::from_center_size(Pos2::new(centre, screen.height() * 0.78), Vec2::new(560.0, 52.0));
        painter.rect_filled(panel, 6.0, Color32::from_black_alpha((255.0 * alpha) as u8));
        let ink = TEXT.gamma_multiply((alpha * 2.0).min(1.0));
        painter.text(panel.center(), Align2::CENTER_CENTER, text, font(17.0), ink);
    }
    if !s.stack.is_empty() {
        paint_stack(painter, screen, &s.stack);
    }
    if let Some(view_projection) = r.pace_3d {
        paint_3d(painter, screen, view_projection, &s.stack);
    }
    if let Some(lamps) = s.lamps {
        // A row of lamps above the speed panel, with the water temperature and the turbo pressure, and the odometer.
        let size = Vec2::new(2.0 * PAD + 250.0, 2.0 * PAD + 58.0);
        let bottom = screen.max.y - MARGIN - (2.0 * PAD + 96.0) - 8.0;
        let panel = Rect::from_min_size(Pos2::new(screen.max.x - MARGIN - size.x, bottom - size.y), size);
        painter.rect_filled(panel, 6.0, PANEL);
        let width = 250.0 / LAMPS.len() as f32;
        for (i, (spec, lamp)) in LAMPS.iter().zip(lamps).enumerate() {
            let lit = lamp == Lamp::On || (lamp == Lamp::Flashing && s.flash_on);
            let chip = Rect::from_min_size(
                panel.min + Vec2::new(PAD + width * i as f32 + 2.0, PAD),
                Vec2::new(width - 4.0, 18.0),
            );
            painter.rect_filled(chip, 3.0, if lit { LAMP_ON } else { TRACK });
            painter.text(chip.center(), Align2::CENTER_CENTER, spec.name, font(10.0), if lit { TEXT } else { DIM });
        }
        let at = panel.min + Vec2::new(PAD, PAD + 24.0);
        let water = s.water.map_or(String::new(), |w| format!("water {w:.0} °C"));
        painter.text(at, Align2::LEFT_TOP, water, font(13.0), DIM);
        if let Some(body) = s.body {
            let colour = if body > 0.25 {
                LAMP_ON
            } else if body > 0.05 {
                ALERT
            } else {
                DIM
            };
            let at = Pos2::new(panel.center().x, at.y);
            painter.text(at, Align2::CENTER_TOP, format!("body {:.0}%", body * 100.0), font(13.0), colour);
        }
        if let Some(boost) = s.boost.filter(|b| *b > 0.0) {
            let at = Pos2::new(panel.max.x - PAD, at.y);
            painter.text(at, Align2::RIGHT_TOP, format!("turbo {:.2} bar", boost / 100_000.0), font(13.0), DIM);
        }
        if let Some((driven, to_go)) = s.odometer {
            let at = at + Vec2::new(0.0, 18.0);
            painter.text(at, Align2::LEFT_TOP, format!("odo {}", distance_label(driven, r.mph)), font(13.0), DIM);
            if let Some(to_go) = to_go {
                let at = Pos2::new(panel.max.x - PAD, at.y);
                let line = format!("to go {}", distance_label(to_go.max(0.0), r.mph));
                painter.text(at, Align2::RIGHT_TOP, line, font(13.0), DIM);
            }
        }
    }
}

/// Points of the 480-line screen the game lays the HUD out on.
const GAME_POINT: f32 = REFERENCE_HEIGHT / 480.0;

/// The pace stack (`HUD::Update`, `PN_STACK`): a column of small symbols at the right (x 605 of 640), the next note at
/// y 190 + 46 times the way covered to it, the passed one 44 below it, the later ones above, 3 apart. The positions
/// are the game's; the symbols are ours.
fn paint_stack(painter: &egui::Painter, screen: Rect, stack: &[StackNote]) {
    let size = pace::SIZE * 0.6;
    let x = screen.max.x - MARGIN - size.x / 2.0;
    let fraction = stack.first().map_or(1.0, |n| n.fraction);
    let mut y = screen.min.y + (190.0 + 46.0 * fraction) * GAME_POINT;
    for note in stack {
        let at = if note.passed { y + 44.0 * GAME_POINT } else { y };
        let rect = Rect::from_center_size(Pos2::new(x, at), size);
        painter.rect_filled(rect.expand(4.0), 6.0, Color32::from_black_alpha((150.0 * note.alpha) as u8));
        pace::paint(painter, rect, &note.symbol, (note.alpha * 2.0).min(1.0));
        if !note.passed {
            y -= size.y + 3.0 * GAME_POINT + 8.0;
        }
    }
}

/// The 3D pace notes (`AID_MARKER`, 0x576680: the stack's next four keys, 0x574440 at each): a symbol over each key's
/// driveline point, 1.5 m up, sized by its distance. Ours: the game's 3D sprite is not ported, so the symbol is
/// drawn on the HUD at the projected point.
fn paint_3d(painter: &egui::Painter, screen: Rect, view_projection: Mat4, stack: &[StackNote]) {
    for note in stack.iter().filter(|n| !n.passed).take(4) {
        let Some([x, y, z]) = note.position else { continue };
        let clip = view_projection * Vec3::new(x, z + 1.5, y).extend(1.0);
        if clip.w <= 0.5 {
            continue;
        }
        let ndc = clip.truncate() / clip.w;
        if ndc.x.abs() > 1.1 || ndc.y.abs() > 1.1 {
            continue;
        }
        let at = Pos2::new(
            screen.min.x + (ndc.x + 1.0) * 0.5 * screen.width(),
            screen.min.y + (1.0 - ndc.y) * 0.5 * screen.height(),
        );
        let size = pace::SIZE * (30.0 / clip.w).clamp(0.25, 1.0);
        pace::paint(painter, Rect::from_center_size(at, size), &note.symbol, 0.85);
    }
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
    fn the_race_clock_shows_the_countdown_then_the_time_and_its_penalties() {
        let start = RaceHud { countdown: 3.24, ..Default::default() };
        assert_eq!(race_lines(&start), ("start in".to_owned(), "3.2".to_owned(), vec![]));
        let assisted = RaceHud { time: 75.5, countdown: -60.0, penalties: 10.0, running: true, jump_start: true };
        assert_eq!(
            race_lines(&assisted),
            (
                "race time".to_owned(),
                "1:15.50".to_owned(),
                vec!["penalties +10 s".to_owned(), "jump start +10 s".to_owned()]
            )
        );
    }
}
