//! The Controls page: the wheel, pedals and shifter first, then the wheel's other buttons and its force feedback, and
//! the keyboard's keys last, collapsed. "Set up your wheel" walks through the inputs one at a time ([`Wizard`]): turn
//! the wheel, press each pedal, pull each paddle, put the H-shifter in each gear, each step skippable. The launcher
//! opens on it when a gamepad is connected and nothing drives from one yet (ours).
//!
//! A binding is set by moving the input: the first button pressed or axis moved from where it was when the capture
//! began (`PadInput::moved`; the steering needs a quarter of the axis, the rest half). The steering takes an axis
//! only: turning left binds steer left and its other side steer right. Each input keeps the device it came from.

use std::collections::HashMap;

use egui::{Color32, ComboBox, DragValue, ProgressBar, RichText, Ui};
use gilrs::{EventType, Gilrs};
use rbr_render::bindings::{self, Action, Bindings, Device, Element, PadInput};
use rbr_render::ffb;
use rbr_render::shifter::{Engagement, PerCar};
use winit::keyboard::KeyCode;

/// A binding being set: its action, and for a gamepad input each axis's value when it began (by gamepad and code).
enum Capture {
    Key(Action),
    Pad(Action, HashMap<(usize, u32), f32>),
}

/// The wheel and pedals rows, and what the set-up asks for each.
const WHEEL: [(Action, &str, &str); 5] = [
    (Action::SteerLeft, "Steering", "Turn the wheel to the left, about a quarter turn."),
    (Action::Throttle, "Throttle", "Press the throttle pedal all the way down."),
    (Action::Brake, "Brake", "Press the brake pedal all the way down."),
    (Action::Clutch, "Clutch", "Press the clutch pedal all the way down (Skip without one)."),
    (Action::Handbrake, "Handbrake", "Pull the handbrake, or press the wheel button you want for it."),
];

/// The shifter rows, and what the set-up asks for each (the H-shifter's from [`FIRST_GEAR`]).
const SHIFTER: [(Action, &str, &str); 9] = [
    (Action::GearUp, "Shift up", "Pull the shift-up paddle (the right one)."),
    (Action::GearDown, "Shift down", "Pull the shift-down paddle (the left one)."),
    (Action::Reverse, "Reverse", "H-shifter: put it in reverse."),
    (Action::Gear1, "1st", "H-shifter: put it in 1st gear."),
    (Action::Gear2, "2nd", "H-shifter: put it in 2nd gear."),
    (Action::Gear3, "3rd", "H-shifter: put it in 3rd gear."),
    (Action::Gear4, "4th", "H-shifter: put it in 4th gear."),
    (Action::Gear5, "5th", "H-shifter: put it in 5th gear."),
    (Action::Gear6, "6th", "H-shifter: put it in 6th gear."),
];

/// The set-up's step that asks for reverse, the first of the H-shifter's.
const FIRST_GEAR: usize = WHEEL.len() + 2;
/// The set-up's steps.
const STEPS: usize = WHEEL.len() + SHIFTER.len();

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
const BUTTONS: [(Action, &str); 12] = [
    (Action::Pause, "Pause"),
    (Action::Restart, "Restart"),
    (Action::CallForHelp, "Call for help"),
    (Action::LookBack, "Look back"),
    (Action::Camera, "Change camera"),
    (Action::CameraAdjust, "Adjust camera"),
    (Action::Wipers, "Wipers"),
    (Action::Headlights, "Headlights"),
    (Action::GaugeMode, "Gauge mode"),
    (Action::Ignition, "Ignition"),
    (Action::Accept, "Accept message"),
    (Action::Neutral, "Neutral"),
];

/// Step `n`'s action and question.
fn step(n: usize) -> (Action, &'static str) {
    let (action, _, ask) = if n < WHEEL.len() { WHEEL[n] } else { SHIFTER[n - WHEEL.len()] };
    (action, ask)
}

/// The guided set-up: the step it is at and the input each step took.
#[derive(Debug, Default)]
struct Wizard {
    step: usize,
    taken: [Option<(Element, Option<Device>)>; STEPS],
}

impl Wizard {
    /// The action the step binds, none when done.
    fn action(&self) -> Option<Action> {
        (self.step < STEPS).then(|| step(self.step).0)
    }

    /// Binds the step's action to `input` and goes on; whether it did. An input an earlier step took is passed over:
    /// the pedal just pressed moves back when it is let go.
    fn take(&mut self, bindings: &mut Bindings, input: PadInput) -> bool {
        let Some(action) = self.action() else { return false };
        if self.taken.contains(&Some((input.element, input.device))) {
            return false;
        }
        bind(bindings, action, input);
        self.taken[self.step] = Some((input.element, input.device));
        self.step += 1;
        true
    }

    fn skip(&mut self) {
        self.step = (self.step + 1).min(STEPS);
    }

    fn back(&mut self) {
        self.step = self.step.saturating_sub(1);
        self.taken[self.step] = None;
    }
}

/// Sets `action`'s gamepad input; turning left also sets the steering's right, the axis's other side.
fn bind(bindings: &mut Bindings, action: Action, input: PadInput) {
    bindings.actions[action as usize].pad = Some(input);
    if action == Action::SteerLeft {
        bindings.actions[Action::SteerRight as usize].pad = Some(input.opposite());
    }
}

/// Whether nothing drives from a gamepad yet: no steering or pedal input bound.
pub fn unbound(bindings: &Bindings) -> bool {
    [Action::SteerLeft, Action::SteerRight, Action::Throttle, Action::Brake]
        .iter()
        .all(|&a| bindings.get(a).pad.is_none())
}

pub struct ControlsPage {
    capture: Option<Capture>,
    wizard: Option<Wizard>,
    /// The first start's offer was made (the page shown) or turned down.
    offered: bool,
    dismissed: bool,
    gilrs: Option<Gilrs>,
}

impl ControlsPage {
    pub fn new() -> ControlsPage {
        ControlsPage {
            capture: None,
            wizard: None,
            offered: false,
            dismissed: false,
            gilrs: Gilrs::new().map_err(|e| eprintln!("launcher: no gamepads: {e}")).ok(),
        }
    }

    /// Whether a binding is being set: the keys go to it.
    pub fn capturing(&self) -> bool {
        self.capture.is_some()
    }

    /// Whether a gamepad is connected.
    pub fn has_pads(&self) -> bool {
        self.gilrs.as_ref().is_some_and(|g| g.gamepads().next().is_some())
    }

    /// Once: whether to show the page, as a gamepad is connected and nothing drives from one yet.
    pub fn offer(&mut self, bindings: &Bindings) -> bool {
        let offer = !self.offered && self.has_pads() && unbound(bindings);
        self.offered |= offer;
        offer
    }

    /// The set-up started (`--screenshot`).
    pub fn start_setup(&mut self) {
        self.wizard = Some(Wizard::default());
        self.arm();
    }

    /// A key pressed while a binding is being set: Escape cancels it (and the set-up), any other bindable key sets a
    /// key binding. Whether the bindings changed.
    pub fn key(&mut self, bindings: &mut Bindings, key: KeyCode) -> bool {
        match self.capture {
            _ if key == KeyCode::Escape => {
                self.capture = None;
                self.wizard = None;
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

    /// Takes the gamepad events; while a gamepad binding is being set, the first button pressed or axis moved sets it.
    pub fn poll(&mut self, bindings: &mut Bindings) {
        let Some(gilrs) = &mut self.gilrs else {
            return;
        };
        let mut next = false;
        while let Some(event) = gilrs.next_event() {
            let Some(Capture::Pad(action, rest)) = &mut self.capture else {
                continue;
            };
            let steering = *action == Action::SteerLeft;
            let threshold = if steering { bindings::STEER_MOVE } else { bindings::PRESS };
            let input = match event.event {
                // The steering takes an axis only.
                EventType::ButtonChanged(button, value, code) if !steering => {
                    PadInput::moved(Element::button(button, code.into_u32()), 0.0, value, threshold)
                }
                // An axis gilrs had no value for when the capture began (not moved since the start) takes its first
                // value as its rest: a pedal resting at an end is not taken for one pressed from the centre.
                EventType::AxisChanged(axis, value, code) => {
                    match rest.get(&(usize::from(event.id), code.into_u32())) {
                        Some(&rest) => PadInput::moved(Element::axis(axis, code.into_u32()), rest, value, threshold),
                        None => {
                            rest.insert((usize::from(event.id), code.into_u32()), value);
                            None
                        }
                    }
                }
                _ => None,
            };
            if let Some(mut input) = input {
                input.device = gilrs.connected_gamepad(event.id).and_then(|pad| Device::of(&pad));
                match &mut self.wizard {
                    Some(wizard) => {
                        if wizard.take(bindings, input) {
                            next = true;
                            break;
                        }
                    }
                    None => {
                        bind(bindings, *action, input);
                        self.capture = None;
                    }
                }
            }
        }
        if next {
            self.arm();
        }
    }

    /// The capture for the set-up's step, from the axes as they are now.
    fn arm(&mut self) {
        self.capture =
            self.wizard.as_ref().and_then(Wizard::action).map(|action| Capture::Pad(action, self.pad_rest()));
    }

    /// Each connected gamepad's axis values, by gamepad and code.
    fn pad_rest(&self) -> HashMap<(usize, u32), f32> {
        let Some(gilrs) = &self.gilrs else {
            return HashMap::new();
        };
        gilrs
            .gamepads()
            .flat_map(|(id, pad)| {
                pad.state()
                    .axes()
                    .map(|(code, data)| ((usize::from(id), code.into_u32()), data.value()))
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    /// The connected gamepads: name and ids.
    fn pads(&self) -> Vec<String> {
        let Some(gilrs) = &self.gilrs else { return Vec::new() };
        gilrs
            .gamepads()
            .map(|(_, pad)| match Device::of(&pad) {
                Some(d) => format!("{} ({:04x}:{:04x})", pad.name(), d.vendor, d.product),
                None => pad.name().to_owned(),
            })
            .collect()
    }

    pub fn ui(&mut self, ui: &mut Ui, bindings: &mut Bindings, ffb: &mut ffb::Options) {
        let pads = self.pads();
        if pads.is_empty() {
            ui.label(
                "No wheel or gamepad found: connect it and it shows here. Until then the keyboard drives: its keys are \
                 under Keyboard below.",
            );
        } else {
            ui.label(format!("Connected: {}", pads.join(", ")));
        }
        ui.add_space(6.0);
        self.setup_ui(ui, bindings);
        ui.add_space(10.0);
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(COLUMN);
                    heading(ui, "Wheel and pedals");
                    self.pad_rows(ui, bindings, "wheel", WHEEL.map(|(action, label, _)| (action, label)));
                    heading(ui, "Wheel buttons");
                    self.pad_rows(ui, bindings, "buttons", BUTTONS);
                });
                ui.add_space(24.0);
                ui.vertical(|ui| {
                    ui.set_width(COLUMN);
                    heading(ui, "Shifter");
                    self.pad_rows(ui, bindings, "shifter-rows", SHIFTER.map(|(action, label, _)| (action, label)));
                    ui.label(
                        RichText::new("With gears bound and Neutral not, letting go of the H-shifter selects neutral.")
                            .color(Color32::GRAY),
                    );
                    shifter_ui(ui, bindings);
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
                self.wizard = None;
            }
        });
    }

    /// A section's gamepad rows.
    fn pad_rows<const N: usize>(&mut self, ui: &mut Ui, bindings: &mut Bindings, id: &str, rows: [(Action, &str); N]) {
        egui::Grid::new(id).num_columns(4).min_col_width(ROW_LABEL).spacing([12.0, 4.0]).show(ui, |ui| {
            for (action, label) in rows {
                self.pad_row(ui, bindings, action, label);
            }
        });
    }

    /// The set-up's button, its current step, or the first start's offer.
    fn setup_ui(&mut self, ui: &mut Ui, bindings: &Bindings) {
        let Some(wizard) = &mut self.wizard else {
            let offer = !self.dismissed && self.has_pads() && unbound(bindings);
            ui.horizontal(|ui| {
                if ui.button(RichText::new("Set up your wheel").strong()).clicked() {
                    self.start_setup();
                }
                if offer {
                    ui.label("Nothing is bound to your wheel yet: set it up in a few steps.");
                    if ui.small_button("Not now").clicked() {
                        self.dismissed = true;
                    }
                } else {
                    ui.label(
                        RichText::new("Turn the wheel, press each pedal and paddle, one at a time.")
                            .color(Color32::GRAY),
                    );
                }
            });
            return;
        };
        let (mut moved, mut done) = (false, false);
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_width(ui.available_width());
            if wizard.action().is_none() {
                ui.label(RichText::new("Your wheel is set up.").heading());
                ui.label("Check each input below: its bar follows the wheel. Click one to set it again.");
                done = ui.button("Done").clicked();
                return;
            }
            ui.label(format!("Wheel set-up, step {} of {STEPS}", wizard.step + 1));
            ui.label(RichText::new(step(wizard.step).1).heading());
            ui.label(RichText::new("Escape stops the set-up.").color(Color32::GRAY));
            ui.horizontal(|ui| {
                if ui.add_enabled(wizard.step > 0, egui::Button::new("Back")).clicked() {
                    wizard.back();
                    moved = true;
                }
                if ui.button("Skip").clicked() {
                    wizard.skip();
                    moved = true;
                }
                if wizard.step >= FIRST_GEAR && ui.button("No H-shifter").clicked() {
                    wizard.step = STEPS;
                    moved = true;
                }
                done = ui.button("Stop").clicked();
            });
        });
        if done {
            self.wizard = None;
            self.capture = None;
        } else if moved {
            self.arm();
        }
    }

    /// An action's gamepad row: its input (click to set it), its live value and a clear.
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
            self.wizard = None;
            self.capture = Some(Capture::Pad(action, self.pad_rest()));
        }
        match &self.gilrs {
            Some(gilrs) if binding.pad.is_some() => {
                let read = |a: Action| bindings.get(a).pad.map_or(0.0, |p| p.read(gilrs));
                let bar = match action {
                    Action::SteerLeft => {
                        let steer = read(Action::SteerRight) - read(Action::SteerLeft);
                        ProgressBar::new((steer + 1.0) / 2.0).text(format!("{steer:+.2}"))
                    }
                    _ => ProgressBar::new(read(action)),
                };
                ui.add(bar.desired_width(BAR));
            }
            _ => {
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
                    self.wizard = None;
                    self.capture = Some(Capture::Key(action));
                }
                if ui.small_button("clear").clicked() {
                    bindings.actions[action as usize].key = None;
                }
                ui.end_row();
            }
        });
    }
}

/// A section's heading, as the other pages have them.
fn heading(ui: &mut Ui, title: &str) {
    ui.add_space(10.0);
    ui.label(RichText::new(title).size(16.5).strong());
    ui.add_space(2.0);
}

/// The steering's inputs: the axis once when right is left's other side, else both.
fn steering_label(bindings: &Bindings) -> String {
    let (left, right) = (bindings.get(Action::SteerLeft).pad, bindings.get(Action::SteerRight).pad);
    match (left, right) {
        (None, None) => "-".to_owned(),
        (Some(l), Some(r)) if r == l.opposite() => format!("{} (left)", l.format()),
        (l, r) => {
            let f = |p: Option<PadInput>| p.map_or("-".to_owned(), PadInput::format);
            format!("{} / {}", f(l), f(r))
        }
    }
}

/// The advanced H-shifter and its options.
fn shifter_ui(ui: &mut Ui, bindings: &mut Bindings) {
    let shifter = &mut bindings.shifter;
    ui.checkbox(&mut shifter.advanced, "Advanced H-shifter: a gear needs the clutch or matched revs").on_hover_text(
        "With gears 1-6 and reverse bound to the shifter. Off: the gears select at once, as NGP has them. \
             Turns Clutch Help off. The grind plays RBRControls' Shifter_Error.wav from the RSF folder.",
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
            .on_hover_text("On: the gear 2 slot is first, gear 3 second and so on; the gear 1 slot is unused");
        ui.end_row();
        ui.label("Clutch threshold");
        ui.add(egui::Slider::new(&mut shifter.clutch_threshold, 0.0..=0.95))
            .on_hover_text("How far the clutch pedal must be pressed to change gear without matching the revs");
        ui.end_row();
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use gilrs::Axis;
    use rbr_render::bindings::Range;

    fn axis(axis: Axis, range: Range) -> PadInput {
        PadInput { element: Element::Axis(axis), range, device: Some(Device { vendor: 0x046d, product: 0xc266 }) }
    }

    /// The G923 on Linux: the wheel on LeftStickX, the pedals resting at +1 on LeftZ and RightZ, the handbrake and a
    /// paddle by native code.
    #[test]
    fn the_setup_binds_a_wheel_step_by_step() {
        let mut bindings = Bindings::default();
        let mut wizard = Wizard::default();
        assert!(unbound(&bindings));
        assert!(wizard.take(&mut bindings, axis(Axis::LeftStickX, Range::Minus)));
        assert_eq!(bindings.get(Action::SteerRight).pad, Some(axis(Axis::LeftStickX, Range::Plus)));
        assert!(wizard.take(&mut bindings, axis(Axis::LeftZ, Range::FullMinus)));
        // The wheel let go and the throttle let go are passed over.
        assert!(!wizard.take(&mut bindings, axis(Axis::LeftStickX, Range::Plus)));
        assert!(!wizard.take(&mut bindings, axis(Axis::LeftZ, Range::FullPlus)));
        assert!(wizard.take(&mut bindings, axis(Axis::RightZ, Range::FullMinus)));
        wizard.skip();
        let code = |c| PadInput { element: Element::Code(c), range: Range::Plus, device: None };
        assert!(wizard.take(&mut bindings, code(288)));
        assert_eq!(wizard.action(), Some(Action::GearUp));
        assert_eq!(bindings.get(Action::Throttle).pad, Some(axis(Axis::LeftZ, Range::FullMinus)));
        assert_eq!(bindings.get(Action::Brake).pad, Some(axis(Axis::RightZ, Range::FullMinus)));
        assert_eq!(bindings.get(Action::Clutch).pad, None);
        assert_eq!(bindings.get(Action::Handbrake).pad, Some(code(288)));
        // Back frees the handbrake's input for its step again.
        wizard.back();
        assert_eq!(wizard.action(), Some(Action::Handbrake));
        assert!(wizard.take(&mut bindings, code(288)));
        assert!(!unbound(&bindings));
        // The keys stay.
        assert_eq!(bindings.get(Action::Throttle).key, Some(KeyCode::ArrowUp));
        while wizard.action().is_some() {
            wizard.skip();
        }
        assert!(!wizard.take(&mut bindings, code(300)));
        assert_eq!(step(FIRST_GEAR).0, Action::Reverse);
    }

    #[test]
    fn the_steering_shows_once_for_one_axis() {
        let mut bindings = Bindings::default();
        assert_eq!(steering_label(&bindings), "-");
        bind(&mut bindings, Action::SteerLeft, axis(Axis::LeftStickX, Range::Minus));
        assert_eq!(steering_label(&bindings), "LeftStickX - @046d:c266 (left)");
        bindings.actions[Action::SteerRight as usize].pad = None;
        assert_eq!(steering_label(&bindings), "LeftStickX - @046d:c266 / -");
    }
}
