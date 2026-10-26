//! The Controls page: the wheel, pedals and shifter first, then the wheel's other buttons and its force feedback, and
//! the keyboard's keys last, collapsed. The launcher opens on it when a wheel is connected and nothing drives from one
//! yet (ours).
//!
//! A binding is set by moving the input on a joystick (`rbr_render::devices`, evdev on Linux): the first button
//! pressed or axis moved from where it was when the capture began (the steering needs a tenth of the axis' range,
//! the rest a quarter). The steering takes an axis only: turning left binds both steering actions to it. Each input
//! keeps the device it came from. A pedal or the steering moving the other way from the game's (a pedal resting at
//! the other end) sets its channel's invert (`input.ini`'s, `Bindings::invert`).
//!
//! "Import from RSF" (`crate::rsf_import`) writes an RSF install's controls and force feedback into these settings.

use std::time::{Duration, Instant};

use egui::{Color32, ComboBox, DragValue, ProgressBar, RichText, Ui};
use rbr_formats::saves::action_map::Object;
use rbr_gameplay::poll::Invert;
use rbr_gameplay::race_options::DriverAids;
use rbr_render::bindings::{self, Action, Bindings, PadInput};
use rbr_render::devices::{values, Joysticks};
use rbr_render::ffb;
use rbr_render::shifter::{Engagement, PerCar};
use rbr_render::wheel_rotation::{self, Wheel};
use winit::keyboard::KeyCode;

/// A binding being set: its action, and for a device input every joystick's values when it began.
enum Capture {
    Key(Action),
    Pad(Action, Vec<Vec<(Object, f32)>>),
}

/// The wheel and pedals rows.
const WHEEL: [(Action, &str); 5] = [
    (Action::SteerLeft, "Steering"),
    (Action::Throttle, "Throttle"),
    (Action::Brake, "Brake"),
    (Action::Clutch, "Clutch"),
    (Action::Handbrake, "Handbrake"),
];

/// The shifter rows.
const SHIFTER: [(Action, &str); 9] = [
    (Action::GearUp, "Shift up"),
    (Action::GearDown, "Shift down"),
    (Action::Reverse, "Reverse"),
    (Action::Gear1, "1st"),
    (Action::Gear2, "2nd"),
    (Action::Gear3, "3rd"),
    (Action::Gear4, "4th"),
    (Action::Gear5, "5th"),
    (Action::Gear6, "6th"),
];

/// How often the joysticks are looked for again while nothing is being set (a wheel plugged in).
const RESCAN: Duration = Duration::from_secs(2);

/// The force feedback: on or off, the strength, the direction and NGP's sensitivity per surface.
fn ffb_ui(ui: &mut Ui, ffb: &mut ffb::Options) {
    ui.horizontal(|ui| {
        ui.checkbox(&mut ffb.settings.enabled, "on");
        ui.add_enabled_ui(ffb.settings.enabled, |ui| {
            ui.add(DragValue::new(&mut ffb.settings.strength).range(0.0..=1.0).speed(0.01).prefix("strength "));
            ui.checkbox(&mut ffb.settings.inverse, "inverse");
        });
    });
    ui.add_enabled_ui(ffb.settings.enabled, |ui| {
        ui.horizontal(|ui| {
            ui.label("sensitivity %").on_hover_text(SENSITIVITY_HELP);
            for (value, surface) in ffb.sensitivity.iter_mut().zip(ffb::SURFACES) {
                ui.add(DragValue::new(value).range(10..=300).prefix(format!("{surface} ")))
                    .on_hover_text(SENSITIVITY_HELP);
            }
        });
        if !ffb.car_sensitivity.is_empty() {
            let n = ffb.car_sensitivity.len();
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("{n} RSF cars have their own, from RSF.")).color(Color32::GRAY));
                if ui.small_button("Use these for every car").clicked() {
                    ffb.car_sensitivity.clear();
                }
            });
        }
    });
}

/// NGP's force-feedback sensitivity per surface (`ffb::Options::sensitivity`), in its readme's words.
const SENSITIVITY_HELP: &str = "NGP's force feedback sensitivity per surface. Below 100 the force is stronger and \
     less precise, and big hits are flattened; above 100 it is lighter, with more detail and sharper peaks.";
/// The rows' label column and the live bars' width.
const ROW_LABEL: f32 = 110.0;
const BAR: f32 = 110.0;
/// The width of each of the two columns.
const COLUMN: f32 = 500.0;

/// The other actions a wheel button can drive.
const BUTTONS: [(Action, &str); 10] = [
    (Action::Pause, "Pause"),
    (Action::Restart, "Restart"),
    (Action::CallForHelp, "Call for help"),
    (Action::LookBack, "Look back"),
    (Action::Camera, "Change camera"),
    (Action::Wipers, "Wipers"),
    (Action::Headlights, "Headlights"),
    (Action::GaugeMode, "Gauge mode"),
    (Action::Ignition, "Ignition"),
    (Action::Neutral, "Neutral"),
];

/// An input that moved: which, and whether its value went up.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Moved {
    input: PadInput,
    up: bool,
}

/// The channel an action's axis drives, as `invert axes` names it.
fn invert_flag(invert: &mut Invert, action: Action) -> Option<&mut bool> {
    Some(match action {
        Action::SteerLeft => &mut invert.steering,
        Action::Throttle => &mut invert.throttle,
        Action::Brake => &mut invert.brake,
        Action::Handbrake => &mut invert.handbrake,
        Action::Clutch => &mut invert.clutch,
        _ => return None,
    })
}

/// Sets `action`'s device input; turning left sets both steering actions to the axis. An axis that went up as
/// the input was pressed or turned left is inverted: the game's steering is negative to the left, and its pedals
/// read `(1 - v) / 2`.
fn bind(bindings: &mut Bindings, action: Action, moved: Moved) {
    bindings.actions[action as usize].pad = Some(moved.input);
    if action == Action::SteerLeft {
        bindings.actions[Action::SteerRight as usize].pad = Some(moved.input);
    }
    if let Some(flag) = invert_flag(&mut bindings.invert, action).filter(|_| moved.input.is_axis()) {
        *flag = moved.up;
    }
}

/// The action's live value from the joysticks' values, as the drive reads it: the steering -1..1, the rest 0..1.
fn live_value(bindings: &Bindings, action: Action, value: f32) -> f32 {
    if !bindings.get(action).pad.is_some_and(PadInput::is_axis) {
        return value;
    }
    let mut invert = bindings.invert;
    let inverted = invert_flag(&mut invert, action).is_some_and(|f| *f);
    match (action, inverted) {
        (Action::SteerLeft, false) => value,
        (Action::SteerLeft, true) => -value,
        (_, true) => (1.0 + value) / 2.0,
        (_, false) => (1.0 - value) / 2.0,
    }
}

/// Whether nothing drives from a wheel yet: no steering or pedal input bound.
pub fn unbound(bindings: &Bindings) -> bool {
    [Action::SteerLeft, Action::SteerRight, Action::Throttle, Action::Brake]
        .iter()
        .all(|&a| bindings.get(a).pad.is_none())
}

pub struct ControlsPage {
    capture: Option<Capture>,
    /// The page was shown at the first start, for a wheel nothing drives from yet.
    offered: bool,
    /// The joysticks connected, and when they were last looked for.
    joysticks: Joysticks,
    scanned: Instant,
    /// The wheels whose rotation can be set, as last read, or why the last set failed.
    wheels: Result<Vec<Wheel>, String>,
    /// The Wine prefix whose `user.reg` names an RSF profile's devices, and the last import's lines or error.
    pub rsf_prefix: String,
    pub import_status: Option<Result<Vec<String>, String>>,
    /// The RSF folder was just set: the import is offered.
    pub import_offer: bool,
}

impl ControlsPage {
    pub fn new() -> ControlsPage {
        ControlsPage {
            capture: None,
            offered: false,
            joysticks: Joysticks::connected(),
            scanned: Instant::now(),
            wheels: Ok(wheel_rotation::wheels()),
            rsf_prefix: crate::rsf_import::default_prefix(),
            import_status: None,
            import_offer: false,
        }
    }

    /// Whether a binding is being set: the keys go to it.
    pub fn capturing(&self) -> bool {
        self.capture.is_some()
    }

    /// Whether a joystick is connected.
    pub fn has_pads(&self) -> bool {
        !self.joysticks.is_empty()
    }

    /// The joysticks connected, by id.
    pub fn connected(&self) -> Vec<bindings::Device> {
        (0..self.joysticks.len()).map(|k| self.joysticks.device(k)).collect()
    }

    /// Once: whether to show the page, as a joystick is connected and nothing drives from one yet.
    pub fn offer(&mut self, bindings: &Bindings) -> bool {
        let offer = !self.offered && self.has_pads() && unbound(bindings);
        self.offered |= offer;
        offer
    }

    /// A key pressed while a binding is being set: Escape cancels it, any other bindable key sets a
    /// key binding. Whether the bindings changed.
    pub fn key(&mut self, bindings: &mut Bindings, key: KeyCode) -> bool {
        match self.capture {
            _ if key == KeyCode::Escape => {
                self.capture = None;
                false
            }
            Some(Capture::Key(action)) if bindings::bindable(key) => {
                bindings.actions[action as usize].key = Some(key);
                self.capture = None;
                true
            }
            _ => false,
        }
    }

    /// Reads the joysticks; while a device binding is being set, the first button pressed or axis moved sets it.
    pub fn poll(&mut self, bindings: &mut Bindings) {
        if self.capture.is_none() && self.scanned.elapsed() >= RESCAN {
            self.joysticks = Joysticks::connected();
            self.scanned = Instant::now();
        }
        self.joysticks.read();
        let Some(Capture::Pad(action, rest)) = &self.capture else { return };
        let action = *action;
        let steering = action == Action::SteerLeft;
        let threshold = if steering { bindings::STEER_MOVE } else { bindings::PRESS };
        let mut moved = None;
        for (k, rest) in rest.iter().enumerate().take(self.joysticks.len()) {
            let (axes, buttons) = self.joysticks.objects(k);
            let now = values(self.joysticks.state(k), axes, buttons);
            let device = self.joysticks.device(k);
            // The steering takes an axis only.
            let found = now.zip(rest).find(|((object, v), (_, r))| {
                (!steering || matches!(object, Object::Axis(_))) && (v - r).abs() > threshold
            });
            if let Some(((object, v), (_, r))) = found {
                moved = Some(Moved { input: PadInput { device, object }, up: v > *r });
                break;
            }
        }
        if let Some(moved) = moved {
            bind(bindings, action, moved);
            self.capture = None;
        }
    }

    /// Each joystick's values now.
    fn pad_rest(&self) -> Vec<Vec<(Object, f32)>> {
        (0..self.joysticks.len())
            .map(|k| {
                let (axes, buttons) = self.joysticks.objects(k);
                values(self.joysticks.state(k), axes, buttons).collect()
            })
            .collect()
    }

    /// The value of a device input now, -1..1 for an axis, 0 or 1 for a button.
    fn read(&self, pad: PadInput) -> Option<f32> {
        let k = (0..self.joysticks.len()).find(|&k| self.joysticks.device(k) == pad.device)?;
        let (axes, buttons) = self.joysticks.objects(k);
        values(self.joysticks.state(k), axes, buttons).find(|(o, _)| *o == pad.object).map(|(_, v)| v)
    }

    /// The joysticks connected: name and id.
    fn pads(&self) -> Vec<String> {
        (0..self.joysticks.len())
            .map(|k| format!("{} ({})", self.joysticks.name(k), self.joysticks.device(k).format()))
            .collect()
    }

    /// The page; whether "Import from RSF" was pressed.
    pub fn ui(
        &mut self,
        ui: &mut Ui,
        bindings: &mut Bindings,
        ffb: &mut ffb::Options,
        aids: &mut DriverAids,
        head_gasket: &mut i32,
        rsf_root: &str,
    ) -> bool {
        let pads = self.pads();
        if pads.is_empty() {
            ui.label(
                "No wheel or gamepad found: connect it and it shows here. Until then the keyboard drives: its keys are \
                 under Keyboard below.",
            );
        } else {
            ui.label(format!("Connected: {}", pads.join(", ")));
        }
        if bindings.dropped {
            ui.colored_label(Color32::LIGHT_RED, DROPPED);
        }
        ui.add_space(6.0);
        let import = self.rsf_ui(ui, rsf_root);
        ui.add_space(10.0);
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(COLUMN);
                    heading(ui, "Wheel and pedals");
                    self.pad_rows(ui, bindings, "wheel", WHEEL);
                    invert_ui(ui, bindings);
                    self.rotation_ui(ui, ffb);
                    heading(ui, "Wheel buttons");
                    self.pad_rows(ui, bindings, "buttons", BUTTONS);
                });
                ui.add_space(24.0);
                ui.vertical(|ui| {
                    ui.set_width(COLUMN);
                    heading(ui, "Shifter");
                    self.pad_rows(ui, bindings, "shifter-rows", SHIFTER);
                    ui.label(
                        RichText::new("With gears bound and Neutral not, letting go of the H-shifter selects neutral.")
                            .color(Color32::GRAY),
                    );
                    shifter_ui(ui, bindings);
                    heading(ui, "Driver aids");
                    aids_ui(ui, aids, head_gasket, bindings.shifter.advanced);
                    heading(ui, "Force feedback");
                    ffb_ui(ui, ffb);
                });
            });
            ui.add_space(8.0);
            egui::CollapsingHeader::new(RichText::new("Keyboard").size(16.5).strong())
                .default_open(false)
                .show(ui, |ui| self.keys_ui(ui, bindings));
            ui.add_space(8.0);
            if ui.button("Reset all controls to defaults").clicked() {
                *bindings = Bindings::default();
                self.capture = None;
            }
        });
        import
    }

    /// "Import from RSF": the Wine prefix whose `user.reg` names the profile's devices, the button and the last
    /// import's summary; whether it was pressed.
    fn rsf_ui(&mut self, ui: &mut Ui, rsf_root: &str) -> bool {
        let mut import = false;
        egui::CollapsingHeader::new(RichText::new("Import from RSF").size(16.5).strong())
            .default_open(self.import_status.is_some())
            .show(ui, |ui| {
                ui.label(RichText::new(IMPORT_HELP).color(Color32::GRAY));
                ui.horizontal(|ui| {
                    ui.label("Wine prefix");
                    ui.add(egui::TextEdit::singleline(&mut self.rsf_prefix).desired_width(COLUMN));
                    import = ui.add_enabled(!rsf_root.trim().is_empty(), egui::Button::new("Import")).clicked();
                });
                self.import_status_ui(ui);
            });
        import
    }

    fn import_status_ui(&self, ui: &mut Ui) {
        match &self.import_status {
            Some(Ok(lines)) => {
                ui.label("Imported:");
                lines.iter().for_each(|l| _ = ui.label(format!("• {l}")));
            }
            Some(Err(e)) => _ = ui.colored_label(Color32::LIGHT_RED, e),
            None => {}
        }
    }

    /// The offer to import, once an RSF folder is first set; whether "Import" was pressed.
    pub fn offer_ui(&mut self, ctx: &egui::Context) -> bool {
        let mut import = false;
        if !self.import_offer {
            return false;
        }
        egui::Window::new("Import your RSF settings?").collapsible(false).resizable(false).show(ctx, |ui| {
            ui.label(IMPORT_HELP);
            ui.horizontal(|ui| {
                ui.label("Wine prefix");
                ui.add(egui::TextEdit::singleline(&mut self.rsf_prefix).desired_width(360.0));
            });
            ui.horizontal(|ui| {
                import = ui.button("Import").clicked();
                if ui.button("Not now").clicked() {
                    self.import_offer = false;
                }
            });
            ui.label(RichText::new("It stays on the Controls page.").color(Color32::GRAY));
        });
        self.import_offer &= !import;
        import
    }

    /// A section's device rows.
    fn pad_rows<const N: usize>(&mut self, ui: &mut Ui, bindings: &mut Bindings, id: &str, rows: [(Action, &str); N]) {
        egui::Grid::new(id).num_columns(4).min_col_width(ROW_LABEL).spacing([12.0, 4.0]).show(ui, |ui| {
            for (action, label) in rows {
                self.pad_row(ui, bindings, action, label);
            }
        });
    }

    /// An action's device row: its input (click to set it), its live value and a clear.
    fn pad_row(&mut self, ui: &mut Ui, bindings: &mut Bindings, action: Action, label: &str) {
        ui.label(label);
        let binding = bindings.get(action);
        let text = match &self.capture {
            Some(Capture::Pad(a, _)) if *a == action => {
                if action == Action::SteerLeft { "turn the wheel left…" } else { "move it…" }.to_owned()
            }
            _ if action == Action::SteerLeft => steering_label(bindings),
            _ => binding.pad.map_or("-".to_owned(), PadInput::format),
        };
        if ui.add(egui::Button::new(text).min_size(egui::vec2(190.0, 0.0))).clicked() {
            self.capture = Some(Capture::Pad(action, self.pad_rest()));
        }
        match binding.pad.and_then(|p| self.read(p)) {
            Some(value) => {
                let value = live_value(bindings, action, value);
                let bar = match action {
                    Action::SteerLeft => ProgressBar::new((value + 1.0) / 2.0).text(format!("{value:+.2}")),
                    _ => ProgressBar::new(value),
                };
                ui.add(bar.desired_width(BAR));
            }
            None => {
                ui.add_sized([BAR, 0.0], egui::Label::new(""));
            }
        }
        if ui.small_button("clear").clicked() {
            bindings.actions[action as usize].pad = None;
            if action == Action::SteerLeft {
                bindings.actions[Action::SteerRight as usize].pad = None;
            }
        }
        ui.end_row();
    }

    /// Each action's key, and a clear.
    fn keys_ui(&mut self, ui: &mut Ui, bindings: &mut Bindings) {
        egui::Grid::new("keys").num_columns(3).spacing([12.0, 4.0]).show(ui, |ui| {
            for action in Action::ALL {
                ui.label(action.name());
                let text = match self.capture {
                    Some(Capture::Key(a)) if a == action => "press a key (Esc cancels)",
                    _ => bindings.get(action).key.map_or("-", bindings::key_name),
                };
                if ui.button(text).clicked() {
                    self.capture = Some(Capture::Key(action));
                }
                if ui.small_button("clear").clicked() {
                    bindings.actions[action as usize].key = None;
                }
                ui.end_row();
            }
        });
    }

    /// The wheel's lock-to-lock rotation: off, fixed, or per car (each car's own up to the degrees). Set on the wheel
    /// when changed, and by the viewer when a drive starts.
    fn rotation_ui(&mut self, ui: &mut Ui, ffb: &mut ffb::Options) {
        ui.horizontal(|ui| {
            ui.add_sized([ROW_LABEL, 18.0], egui::Label::new("Rotation")).on_hover_text(ROTATION_HELP);
            const MODES: [(Option<bool>, &str); 3] = [(None, "off"), (Some(false), "fixed"), (Some(true), "per car")];
            let mut mode = ffb.rotation.map(|_| ffb.rotation_per_car);
            let name = MODES.iter().find(|(m, _)| *m == mode).map_or("", |(_, n)| n);
            let mut changed = false;
            ComboBox::from_id_salt("rotation").selected_text(name).show_ui(ui, |ui| {
                for (m, n) in MODES {
                    changed |= ui.selectable_value(&mut mode, m, n).changed();
                }
            });
            let mut degrees = ffb.rotation.unwrap_or(900);
            if let Some(per_car) = mode {
                ui.label(if per_car { "up to" } else { "lock to lock" });
                let r = ui.add(DragValue::new(&mut degrees).range(wheel_rotation::DEGREES).speed(5).suffix("°"));
                changed |= (r.changed() && !r.dragged()) || r.drag_stopped();
                ffb.rotation_per_car = per_car;
            }
            ffb.rotation = mode.map(|_| degrees);
            if let Some(degrees) = ffb.rotation.filter(|_| changed) {
                self.wheels = wheel_rotation::set(degrees);
            }
        });
        let status = match &self.wheels {
            Ok(wheels) if wheels.is_empty() => "No wheel whose rotation can be set.".to_owned(),
            Ok(wheels) => wheels.iter().map(|w| format!("{}: {}°", w.name, w.degrees)).collect::<Vec<_>>().join(", "),
            Err(e) => e.clone(),
        };
        ui.label(RichText::new(status).color(Color32::GRAY));
    }
}

/// The pedals' and the steering's inverts (`input.ini`'s `Invert*Axis`), which the set-up sets.
fn invert_ui(ui: &mut Ui, bindings: &mut Bindings) {
    ui.horizontal(|ui| {
        ui.add_sized([ROW_LABEL, 18.0], egui::Label::new("Invert")).on_hover_text(INVERT_HELP);
        for (action, label) in [
            (Action::SteerLeft, "steering"),
            (Action::Throttle, "throttle"),
            (Action::Brake, "brake"),
            (Action::Clutch, "clutch"),
            (Action::Handbrake, "handbrake"),
        ] {
            if let Some(flag) = invert_flag(&mut bindings.invert, action) {
                ui.checkbox(flag, label).on_hover_text(INVERT_HELP);
            }
        }
    });
}

/// What the page says about the dropped bindings, and the launcher's log once.
pub const DROPPED: &str = "Pad bindings from an older version were dropped: bind your wheel again, or Import from RSF.";

/// What the import does.
const IMPORT_HELP: &str = "Writes your RSF settings into ReRBR's, once: the profile's controls (its devices named by \
     the Wine prefix's user.reg), the force feedback, NGP's sensitivities, dead zone and rates, the driver aids and the \
     head gasket. Drives then use only ReRBR's settings.";

/// What the inverts do.
const INVERT_HELP: &str = "Flips an axis, as RBR's input.ini does: for a pedal that rests at the other end, or a \
     wheel that turns the other way. Binding an axis sets it.";

/// A section's heading, as the other pages have them.
fn heading(ui: &mut Ui, title: &str) {
    ui.add_space(10.0);
    ui.label(RichText::new(title).size(16.5).strong());
    ui.add_space(2.0);
}

/// The steering's inputs: the axis once when both are on it, else both.
fn steering_label(bindings: &Bindings) -> String {
    let (left, right) = (bindings.get(Action::SteerLeft).pad, bindings.get(Action::SteerRight).pad);
    match (left, right) {
        (None, None) => "-".to_owned(),
        (Some(l), Some(r)) if r == l => l.format(),
        (l, r) => {
            let f = |p: Option<PadInput>| p.map_or("-".to_owned(), PadInput::format);
            format!("{} / {}", f(l), f(r))
        }
    }
}

/// What the rotation setting does.
const ROTATION_HELP: &str = "How far the wheel turns from lock to lock. The wheel stops there and full lock is \
     reached there. Fixed: the degrees for every car. Per car: each car's steering lock (its setup's MaxSteeringLock \
     x 720), up to the degrees. Off leaves the wheel as it is. Linux wheel drivers only (the wheel tools' udev rule \
     must allow it).";

fn shifter_ui(ui: &mut Ui, bindings: &mut Bindings) {
    let shifter = &mut bindings.shifter;
    ui.checkbox(&mut shifter.advanced, "Advanced H-shifter: a gear needs the clutch or matched revs").on_hover_text(
        "With gears 1-6 and reverse bound to the shifter. Off: the gears select at once, as NGP has them. \
             Turns Clutch Help and the automatic gearbox off. Sequential cars keep both and shift as before. The grind plays RBRControls' Shifter_Error.wav from the RSF folder.",
    );
    if !shifter.advanced {
        return;
    }
    egui::Grid::new("shifter").num_columns(2).spacing([12.0, 4.0]).show(ui, |ui| {
        ui.label("Gearbox");
        let engagement = |e: PerCar<Engagement>| match e {
            PerCar::Auto => "auto (the car's)",
            PerCar::Fixed(e) => e.name(),
        };
        ComboBox::from_id_salt("engagement")
            .selected_text(engagement(shifter.engagement))
            .show_ui(ui, |ui| {
                for e in [PerCar::Auto, PerCar::Fixed(Engagement::Synchro), PerCar::Fixed(Engagement::Dog)] {
                    ui.selectable_value(&mut shifter.engagement, e, engagement(e));
                }
            })
            .response
            .on_hover_text("Auto: the RSF car's from RBRControls' CarInfo.ini, else synchro");
        ui.end_row();
        ui.label("Dogleg first gear");
        let dogleg = |d: PerCar<bool>| match d {
            PerCar::Auto => "auto (the car's)",
            PerCar::Fixed(true) => "on",
            PerCar::Fixed(false) => "off",
        };
        ComboBox::from_id_salt("dogleg")
            .selected_text(dogleg(shifter.dogleg))
            .show_ui(ui, |ui| {
                for d in [PerCar::Auto, PerCar::Fixed(true), PerCar::Fixed(false)] {
                    ui.selectable_value(&mut shifter.dogleg, d, dogleg(d));
                }
            })
            .response
            .on_hover_text("On: the gear 1 slot is reverse, gear 2 first, gear 3 second and so on");
        ui.end_row();
        ui.label("Clutch threshold");
        ui.add(egui::Slider::new(&mut shifter.clutch_threshold, 0.0..=0.95))
            .on_hover_text("How far the clutch pedal must be pressed to count as in");
        ui.end_row();
    });
}

/// NGP's `headGasketDurability` with RSF's "Engine head gasket durability fix" on, as its install has it.
const HEAD_GASKET_FIX: i32 = 10;

/// What the head gasket fix does.
const HEAD_GASKET_HELP: &str = "RSF's fix: the head gasket wears 10 times slower, so realistic damage no longer kills \
     the engine unexpectedly. Off is NGP's default. Import from RSF takes RSF's.";

/// The game's gearbox aids (its English labels, `bstrings.eng`) and "Game defaults", then the head gasket fix (NGP's
/// `headGasketDurability`, 1 off). The advanced H-shifter turns the automatic gearbox and clutch help off (but for a
/// sequential car), so they show off.
fn aids_ui(ui: &mut Ui, aids: &mut DriverAids, head_gasket: &mut i32, advanced: bool) {
    egui::Grid::new("aids").num_columns(2).spacing([16.0, 4.0]).show(ui, |ui| {
        let rows = [
            ("Automatic gearbox", &mut aids.auto_gears, !advanced, "Shifts up and down for you"),
            ("Gear Protection", &mut aids.gear_guard, true, "Stops a shift into a gear that would over-rev"),
            ("Neutral Lock", &mut aids.neutral_lock, true, "Keeps the gearbox from slipping into neutral"),
            ("Clutch Help", &mut aids.clutch_help, !advanced, "Works the clutch for you when you shift"),
        ];
        for (n, (label, value, enabled, tip)) in rows.into_iter().enumerate() {
            let mut off = false;
            let value = if enabled { value } else { &mut off };
            ui.add_enabled(enabled, egui::Checkbox::new(value, label))
                .on_hover_text(tip)
                .on_disabled_hover_text("Off with the advanced H-shifter");
            if n % 2 == 1 {
                ui.end_row();
            }
        }
    });
    if ui.small_button("Game defaults").on_hover_text("The game's driver aids").clicked() {
        *aids = DriverAids::default();
    }
    let mut fix = *head_gasket > 1;
    if ui.checkbox(&mut fix, "Head gasket durability fix").on_hover_text(HEAD_GASKET_HELP).changed() {
        *head_gasket = if fix { HEAD_GASKET_FIX } else { 1 };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rbr_render::bindings::Device;

    const G923: Device = Device { vendor: 0x046d, product: 0xc266, index: 0 };

    fn moved(object: Object, up: bool) -> Moved {
        Moved { input: PadInput { device: G923, object }, up }
    }

    /// The G923 on Linux: the wheel axis 0, the pedals axes 1 and 2 (going down as pressed), the handbrake a button.
    #[test]
    fn binding_sets_the_inverts_and_the_bars_read_as_the_drive() {
        let mut bindings = Bindings::default();
        assert!(unbound(&bindings));
        bind(&mut bindings, Action::SteerLeft, moved(Object::Axis(0), false));
        assert_eq!(bindings.get(Action::SteerRight).pad, Some(moved(Object::Axis(0), false).input));
        bind(&mut bindings, Action::Throttle, moved(Object::Axis(2), false));
        // A brake pedal that rises as pressed is inverted; a button sets no invert.
        bind(&mut bindings, Action::Brake, moved(Object::Axis(1), true));
        bind(&mut bindings, Action::Handbrake, moved(Object::Button(7), true));
        let i = bindings.invert;
        assert_eq!((i.steering, i.throttle, i.brake, i.handbrake), (false, false, true, false));
        assert!(!unbound(&bindings));
        // The keys stay.
        assert_eq!(bindings.get(Action::Throttle).key, Some(KeyCode::ArrowUp));
        // The bars read as the drive does: the throttle at rest (axis +1) is 0, pressed (-1) is 1.
        assert_eq!(
            (live_value(&bindings, Action::Throttle, 1.0), live_value(&bindings, Action::Throttle, -1.0)),
            (0.0, 1.0)
        );
        assert_eq!(live_value(&bindings, Action::Brake, -1.0), 0.0);
    }

    #[test]
    fn the_steering_shows_once_for_one_axis() {
        let mut bindings = Bindings::default();
        assert_eq!(steering_label(&bindings), "-");
        bind(&mut bindings, Action::SteerLeft, moved(Object::Axis(0), true));
        assert_eq!(steering_label(&bindings), "046d:c266/0 axis 0");
        assert!(bindings.invert.steering);
        bindings.actions[Action::SteerRight as usize].pad = None;
        assert_eq!(steering_label(&bindings), "046d:c266/0 axis 0 / -");
    }
}
