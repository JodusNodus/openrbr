//! The live mode's control bindings: a key and optionally a gamepad input for each driving action, read from a
//! `[controls]` section of `key = value` lines (`rbr_formats::ini`; the launcher keeps it in `launcher-settings.ini`
//! and passes that file to `viewer --bindings`), and the [`Controls`] one frame's inputs make.
//!
//! ```text
//! [controls]
//! throttle = ArrowUp
//! throttle pad = RightTrigger2
//! steer left = ArrowLeft
//! steer left pad = LeftStickX -
//! clutch =
//! clutch pad = #65538 full+
//! gear 1 pad = #65824 @044f:b660
//! ```
//!
//! A key is winit's `KeyCode` name. A gamepad input is gilrs's button or axis name, or `#` and the native code for an
//! element gilrs has no name for (a wheel's pedals), and for an axis the side that drives the action: `+` or `-` from
//! the centre (a stick), `full+` or `full-` over the whole range (a pedal resting at the other end), and optionally
//! `@` and the device's USB vendor and product ids, which limit it to that device (a shifter's buttons have the
//! same codes as the wheel's). An empty value
//! unbinds; a missing line, or one that does not parse, keeps the default. The defaults are the keys the viewer always had and no gamepad,
//! but for the pause: Escape and the gamepad's Start.
//!
//! A key is full while it is held, as the game's keyboard is: the steering keys give full lock to their side (the game's
//! input filter then moves the wheel at its digital button cell's rise and fall rates, `PC_InputFilter.ini`). A
//! gamepad input gives 0..1. Each pedal takes the larger of its key and its gamepad input; the steering takes the
//! keys' while one is held or no steering pad input is bound, else the pad's as it is (no deadzone: a wheel's centre is
//! its centre), through the filter's axis cell.
//!
//! The direct gear actions (an H-shifter, as NGP has them) select their gear when pressed. With a gear bound and
//! neutral not, releasing every gear selects neutral ([`Bindings::neutral_on_release`]). `h-shifter = advanced` makes
//! a gear need the clutch or matched revs instead (`crate::shifter`), with `engagement = auto|synchro|dog`,
//! `dogleg = auto|on|off` and `clutch threshold = 0..1`.

use gilrs::{Axis, Button, Gilrs};
use winit::keyboard::KeyCode;

use crate::live::Controls;
use crate::shifter::{self, Engagement, PerCar};

/// A gamepad value above this presses a gear, restart or call-for-help action, and a move this far sets a binding.
pub const PRESS: f32 = 0.5;
/// A move this far sets a steering binding: a wheel turned a quarter of its range, not half.
pub const STEER_MOVE: f32 = 0.2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Throttle,
    Brake,
    SteerLeft,
    SteerRight,
    Handbrake,
    Clutch,
    GearUp,
    GearDown,
    Restart,
    /// The call for help (the original's pause-menu item): the car is put back on the road with a time penalty.
    CallForHelp,
    /// Steps the wiper switch: off, intermittent, continuous, auto (ours, `crate::glass::model::Switch`).
    Wipers,
    /// Steps the headlights from main beam to dipped to off, with the tail lights, and back (ours: 1.02 has them always
    /// on).
    Headlights,
    /// Held: the ignition key turned to start, the control's aux bit (`CDrive+0x50`): NGP cranks the engine after a
    /// 0.5-2.5 s hold (`rbr_physics::ngp::starter`); 1.02 has no starter and ignores it.
    Ignition,
    Reverse,
    Neutral,
    Gear1,
    Gear2,
    Gear3,
    Gear4,
    Gear5,
    Gear6,
    /// Held: the chase cameras look behind the car (the game chase's look-back swing, which 1.02 binds to nothing;
    /// ours).
    LookBack,
    /// Cycles the camera views (`crate::views`; the game's CH_CAM).
    Camera,
    /// Opens and closes the pause menu (the game's PAUSE action, `rbr_gameplay::pause_menu`).
    Pause,
    /// Cycles the pages of the RBRHUD gauges' digital dashes (`crate::dials`; the plugin's End key).
    GaugeMode,
}

/// How many actions there are.
pub const ACTIONS: usize = 25;

impl Action {
    pub const ALL: [Action; ACTIONS] = [
        Action::Throttle,
        Action::Brake,
        Action::SteerLeft,
        Action::SteerRight,
        Action::Handbrake,
        Action::Clutch,
        Action::GearUp,
        Action::GearDown,
        Action::Restart,
        Action::CallForHelp,
        Action::Wipers,
        Action::Headlights,
        Action::Ignition,
        Action::Reverse,
        Action::Neutral,
        Action::Gear1,
        Action::Gear2,
        Action::Gear3,
        Action::Gear4,
        Action::Gear5,
        Action::Gear6,
        Action::LookBack,
        Action::Camera,
        Action::Pause,
        Action::GaugeMode,
    ];

    /// The ini key and the launcher's label.
    pub fn name(self) -> &'static str {
        match self {
            Action::Throttle => "throttle",
            Action::Brake => "brake",
            Action::SteerLeft => "steer left",
            Action::SteerRight => "steer right",
            Action::Handbrake => "handbrake",
            Action::Clutch => "clutch",
            Action::GearUp => "gear up",
            Action::GearDown => "gear down",
            Action::Restart => "restart",
            Action::CallForHelp => "call for help",
            Action::Wipers => "wipers",
            Action::Headlights => "headlights",
            Action::Ignition => "ignition",
            Action::Reverse => "reverse",
            Action::Neutral => "neutral",
            Action::Gear1 => "gear 1",
            Action::Gear2 => "gear 2",
            Action::Gear3 => "gear 3",
            Action::Gear4 => "gear 4",
            Action::Gear5 => "gear 5",
            Action::Gear6 => "gear 6",
            Action::LookBack => "look back",
            Action::Camera => "camera",
            Action::Pause => "pause",
            Action::GaugeMode => "gauge mode",
        }
    }

    /// The gear a direct gear action selects, as the control's word 6 has it (`CDrive::M6840`, 0x506840): 0
    /// reverse, 1 neutral, 2 first.
    pub fn gear(self) -> Option<u8> {
        match self {
            Action::Reverse => Some(0),
            Action::Neutral => Some(1),
            Action::Gear1 => Some(2),
            Action::Gear2 => Some(3),
            Action::Gear3 => Some(4),
            Action::Gear4 => Some(5),
            Action::Gear5 => Some(6),
            Action::Gear6 => Some(7),
            _ => None,
        }
    }
}

/// A gamepad element: a button or an axis by gilrs's name, or by its native code when gilrs has none for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Element {
    Button(Button),
    Axis(Axis),
    Code(u32),
}

impl Element {
    /// A gilrs event's button: by name, or by code when gilrs has no name for it.
    pub fn button(button: Button, code: u32) -> Element {
        match button {
            Button::Unknown => Element::Code(code),
            b => Element::Button(b),
        }
    }

    /// A gilrs event's axis: by name, or by code when gilrs has no name for it.
    pub fn axis(axis: Axis, code: u32) -> Element {
        match axis {
            Axis::Unknown => Element::Code(code),
            a => Element::Axis(a),
        }
    }
}

/// The part of an element's range that drives an action: from the centre to one end (a stick, or a button's 0..1
/// for `Plus`), or the whole range from the other end (a pedal axis that rests at -1 or 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Range {
    Plus,
    Minus,
    FullPlus,
    FullMinus,
}

/// A device by its USB vendor and product ids.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Device {
    pub vendor: u16,
    pub product: u16,
}

impl Device {
    /// The gamepad's ids, when gilrs knows them.
    pub fn of(pad: &gilrs::Gamepad) -> Option<Device> {
        Some(Device { vendor: pad.vendor_id()?, product: pad.product_id()? })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PadInput {
    pub element: Element,
    pub range: Range,
    /// The device it is read from; `None` reads every gamepad.
    pub device: Option<Device>,
}

impl PadInput {
    /// The action's 0..1 for the element's value.
    pub fn shape(self, value: f32) -> f32 {
        match self.range {
            Range::Plus => value,
            Range::Minus => -value,
            Range::FullPlus => (value + 1.0) / 2.0,
            Range::FullMinus => (1.0 - value) / 2.0,
        }
        .clamp(0.0, 1.0)
    }

    /// The input for an element that moved from `rest` to `value`, when it moved further than `threshold`: an axis
    /// resting at an end is a pedal.
    pub fn moved(element: Element, rest: f32, value: f32, threshold: f32) -> Option<PadInput> {
        if (value - rest).abs() <= threshold {
            return None;
        }
        let range = match (rest.abs() > PRESS, value > rest) {
            (true, true) => Range::FullPlus,
            (true, false) => Range::FullMinus,
            (false, _) if value >= 0.0 => Range::Plus,
            (false, _) => Range::Minus,
        };
        Some(PadInput { element, range, device: None })
    }

    /// The other side of the same element: steering right for steering left.
    pub fn opposite(self) -> PadInput {
        let range = match self.range {
            Range::Plus => Range::Minus,
            Range::Minus => Range::Plus,
            Range::FullPlus => Range::FullMinus,
            Range::FullMinus => Range::FullPlus,
        };
        PadInput { range, ..self }
    }

    pub fn parse(text: &str) -> Option<PadInput> {
        let (text, device) = match text.rsplit_once(" @") {
            Some((rest, ids)) => {
                let (vendor, product) = ids.split_once(':')?;
                let id = |hex| u16::from_str_radix(hex, 16).ok();
                (rest, Some(Device { vendor: id(vendor)?, product: id(product)? }))
            }
            None => (text, None),
        };
        let (name, range) = text.split_once(' ').unwrap_or((text, "+"));
        let element = match name.strip_prefix('#') {
            Some(code) => Element::Code(code.parse().ok()?),
            None => lookup(BUTTONS, name).map(Element::Button).or_else(|| lookup(AXES, name).map(Element::Axis))?,
        };
        let range = match range.trim() {
            "+" => Range::Plus,
            "-" => Range::Minus,
            "full+" => Range::FullPlus,
            "full-" => Range::FullMinus,
            _ => return None,
        };
        Some(PadInput { element, range, device })
    }

    /// The ini value, also the launcher's label: `South`, `LeftStickX -`, `#65538 full+`, `#65824 @044f:b660`.
    pub fn format(self) -> String {
        let range = match self.range {
            Range::Plus => "+",
            Range::Minus => "-",
            Range::FullPlus => "full+",
            Range::FullMinus => "full-",
        };
        let mut out = match self.element {
            Element::Button(b) => name_of(BUTTONS, b).unwrap_or("?").to_owned(),
            Element::Axis(a) => format!("{} {range}", name_of(AXES, a).unwrap_or("?")),
            Element::Code(c) => format!("#{c} {range}"),
        };
        if let Some(d) = self.device {
            out.push_str(&format!(" @{:04x}:{:04x}", d.vendor, d.product));
        }
        out
    }

    /// The action's 0..1 for the element's value, `None` while gilrs has none (the element has not moved since the
    /// start: gilrs learns a pedal's position from its events): at rest, 0. A full-range pedal's 0.0 would be half
    /// pressed.
    pub fn shape_read(self, value: Option<f32>) -> f32 {
        value.map_or(0.0, |v| self.shape(v))
    }

    /// The input's 0..1 on the connected gamepads, the largest if several have the element.
    pub fn read(self, gilrs: &Gilrs) -> f32 {
        gilrs
            .gamepads()
            .filter(|(_, pad)| self.device.is_none() || self.device == Device::of(pad))
            .map(|(_, pad)| {
                let value = match self.element {
                    Element::Button(b) => pad.button_data(b).map(|d| d.value()),
                    Element::Axis(a) => pad.axis_data(a).map(|d| d.value()),
                    Element::Code(c) => {
                        let state = pad.state();
                        let axis = state.axes().find(|(code, _)| code.into_u32() == c);
                        let button = state.buttons().find(|(code, _)| code.into_u32() == c);
                        axis.map(|(_, d)| d.value()).or(button.map(|(_, d)| d.value()))
                    }
                };
                self.shape_read(value)
            })
            .fold(0.0, f32::max)
    }
}

/// One action's key and gamepad input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Binding {
    pub key: Option<KeyCode>,
    pub pad: Option<PadInput>,
}

impl Binding {
    /// How the HUD names it: the key without winit's `Key` or `Digit` (`X`, `F1`, `ArrowUp`), else the gamepad
    /// input, else `unbound`.
    pub fn label(self) -> String {
        match (self.key, self.pad) {
            (Some(key), _) => {
                let name = key_name(key);
                name.strip_prefix("Key").or_else(|| name.strip_prefix("Digit")).unwrap_or(name).to_owned()
            }
            (None, Some(pad)) => pad.format(),
            (None, None) => "unbound".to_owned(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Bindings {
    /// In [`Action::ALL`] order.
    pub actions: [Binding; ACTIONS],
    pub shifter: shifter::Settings,
}

impl Default for Bindings {
    fn default() -> Bindings {
        let key = |key| Binding { key: Some(key), pad: None };
        Bindings {
            actions: [
                key(KeyCode::ArrowUp),
                key(KeyCode::ArrowDown),
                key(KeyCode::ArrowLeft),
                key(KeyCode::ArrowRight),
                key(KeyCode::Space),
                Binding::default(),
                key(KeyCode::KeyA),
                key(KeyCode::KeyZ),
                key(KeyCode::KeyR),
                key(KeyCode::KeyH),
                key(KeyCode::KeyI),
                key(KeyCode::KeyL),
                key(KeyCode::KeyE),
                Binding::default(),
                Binding::default(),
                Binding::default(),
                Binding::default(),
                Binding::default(),
                Binding::default(),
                Binding::default(),
                Binding::default(),
                key(KeyCode::KeyB),
                key(KeyCode::KeyC),
                Binding {
                    key: Some(KeyCode::Escape),
                    pad: Some(PadInput { element: Element::Button(Button::Start), range: Range::Plus, device: None }),
                },
                key(KeyCode::End),
            ],
            shifter: shifter::Settings::default(),
        }
    }
}

impl Bindings {
    /// The `[controls]` section of `text`; the defaults for whatever it leaves out.
    pub fn parse(text: &str) -> Bindings {
        let mut out = Bindings::default();
        let sections = rbr_formats::ini::parse(text).unwrap_or_default();
        let Some(section) = sections.iter().find(|s| s.name == "controls") else {
            return out;
        };
        for (binding, action) in out.actions.iter_mut().zip(Action::ALL) {
            // An empty value unbinds; one that does not parse keeps the default.
            if let Some(value) = section.get(action.name()) {
                binding.key = match value {
                    "" => None,
                    _ => lookup(KEYS, value).or(binding.key),
                };
            }
            if let Some(value) = section.get(&format!("{} pad", action.name())) {
                binding.pad = match value {
                    "" => None,
                    _ => PadInput::parse(value).or(binding.pad),
                };
            }
        }
        let shifter = &mut out.shifter;
        match section.get("h-shifter") {
            Some("advanced") => shifter.advanced = true,
            Some("normal") => shifter.advanced = false,
            _ => {}
        }
        match section.get("engagement") {
            Some("auto") => shifter.engagement = PerCar::Auto,
            Some(value) => shifter.engagement = Engagement::parse(value).map_or(shifter.engagement, PerCar::Fixed),
            None => {}
        }
        match section.get("dogleg") {
            Some("auto") => shifter.dogleg = PerCar::Auto,
            Some("on") => shifter.dogleg = PerCar::Fixed(true),
            Some("off") => shifter.dogleg = PerCar::Fixed(false),
            _ => {}
        }
        if let Some(threshold) = section.get("clutch threshold").and_then(|v| v.parse().ok()) {
            shifter.clutch_threshold = f32::clamp(threshold, 0.0, 0.99);
        }
        out
    }

    pub fn to_text(&self) -> String {
        let shifter = self.shifter;
        let engagement = match shifter.engagement {
            PerCar::Auto => "auto",
            PerCar::Fixed(e) => e.name(),
        };
        let dogleg = match shifter.dogleg {
            PerCar::Auto => "auto",
            PerCar::Fixed(true) => "on",
            PerCar::Fixed(false) => "off",
        };
        let mut out = format!(
            "[controls]\nh-shifter = {}\nengagement = {engagement}\ndogleg = {dogleg}\nclutch threshold = {}\n",
            if shifter.advanced { "advanced" } else { "normal" },
            shifter.clutch_threshold,
        );
        for (binding, action) in self.actions.iter().zip(Action::ALL) {
            let key = binding.key.map_or("", key_name);
            let pad = binding.pad.map(PadInput::format).unwrap_or_default();
            out.push_str(&format!("{0} = {key}\n{0} pad = {pad}\n", action.name()));
        }
        out
    }

    pub fn get(&self, action: Action) -> Binding {
        self.actions[action as usize]
    }

    /// The action `key` is bound to, if any.
    pub fn action_for_key(&self, key: KeyCode) -> Option<Action> {
        Action::ALL.into_iter().find(|&a| self.get(a).key == Some(key))
    }

    /// Each action's gamepad value, in [`Action::ALL`] order.
    pub fn read_pads(&self, gilrs: &Gilrs) -> [f32; ACTIONS] {
        self.actions.map(|b| b.pad.map_or(0.0, |input| input.read(gilrs)))
    }

    /// The controls for one frame: `held` says which actions have their key held, `pads` is each action's gamepad
    /// value. The gear flags stay as `last` has them.
    pub fn controls(&self, held: [bool; ACTIONS], pads: [f32; ACTIONS], last: Controls) -> Controls {
        let pedal = |a: Action| f32::max(held[a as usize] as u8 as f32, pads[a as usize]);
        let analog = pads[Action::SteerRight as usize] - pads[Action::SteerLeft as usize];
        let keys = held[Action::SteerRight as usize] as u8 as f32 - held[Action::SteerLeft as usize] as u8 as f32;
        let pad_bound = self.get(Action::SteerLeft).pad.is_some() || self.get(Action::SteerRight).pad.is_some();
        let keys_steer = held[Action::SteerLeft as usize] || held[Action::SteerRight as usize] || !pad_bound;
        Controls {
            brake: pedal(Action::Brake),
            throttle: pedal(Action::Throttle),
            handbrake: pedal(Action::Handbrake),
            clutch: pedal(Action::Clutch),
            steering: if keys_steer { keys } else { analog },
            ignition: held[Action::Ignition as usize] || pads[Action::Ignition as usize] > PRESS,
            flags: last.flags,
            gear: last.gear,
            keys_steer,
        }
    }

    /// Whether releasing every gear selects neutral: a direct gear is bound and neutral is not (NGP's H-shifter).
    pub fn neutral_on_release(&self) -> bool {
        let bound = |a: Action| self.get(a) != Binding::default();
        !bound(Action::Neutral) && Action::ALL.into_iter().any(|a| a.gear().is_some() && bound(a))
    }
}

/// Whether a direct gear action is held, by its key or its gamepad input.
pub fn gear_held(held: [bool; ACTIONS], pads: [f32; ACTIONS]) -> bool {
    Action::ALL.into_iter().any(|a| a.gear().is_some() && (held[a as usize] || pads[a as usize] > PRESS))
}

/// The actions whose gamepad value went above the press threshold since `before`.
pub fn pad_presses(before: [f32; ACTIONS], now: [f32; ACTIONS]) -> impl Iterator<Item = Action> {
    Action::ALL.into_iter().filter(move |&a| now[a as usize] > PRESS && before[a as usize] <= PRESS)
}

/// winit's name for a bindable key (`?` for one that is not).
pub fn key_name(key: KeyCode) -> &'static str {
    name_of(KEYS, key).unwrap_or("?")
}

/// Whether `key` can be bound (it has a name in the ini). Escape has one (the pause's default), but the launcher's
/// capture takes it as the cancel.
pub fn bindable(key: KeyCode) -> bool {
    name_of(KEYS, key).is_some()
}

fn lookup<T: Copy>(table: &[(T, &str)], name: &str) -> Option<T> {
    table.iter().find(|(_, n)| *n == name).map(|&(v, _)| v)
}

fn name_of<T: PartialEq>(table: &[(T, &'static str)], value: T) -> Option<&'static str> {
    table.iter().find(|(v, _)| *v == value).map(|&(_, n)| n)
}

/// `[(Type::Name, "Name"), ...]`.
macro_rules! names {
    ($ty:ident: $($name:ident),* $(,)?) => {
        &[$(($ty::$name, stringify!($name))),*]
    };
}

const BUTTONS: &[(Button, &str)] = names!(Button:
    South, East, North, West, C, Z, LeftTrigger, LeftTrigger2, RightTrigger, RightTrigger2, Select, Start, Mode,
    LeftThumb, RightThumb, DPadUp, DPadDown, DPadLeft, DPadRight,
);

const AXES: &[(Axis, &str)] =
    names!(Axis: LeftStickX, LeftStickY, LeftZ, RightStickX, RightStickY, RightZ, DPadX, DPadY);

const KEYS: &[(KeyCode, &str)] = names!(KeyCode:
    KeyA, KeyB, KeyC, KeyD, KeyE, KeyF, KeyG, KeyH, KeyI, KeyJ, KeyK, KeyL, KeyM, KeyN, KeyO, KeyP, KeyQ, KeyR, KeyS,
    KeyT, KeyU, KeyV, KeyW, KeyX, KeyY, KeyZ,
    Digit0, Digit1, Digit2, Digit3, Digit4, Digit5, Digit6, Digit7, Digit8, Digit9,
    ArrowUp, ArrowDown, ArrowLeft, ArrowRight, Space, Enter, Escape, Tab, Backspace, Insert, Delete, Home, End, PageUp,
    PageDown, ShiftLeft, ShiftRight, ControlLeft, ControlRight, AltLeft, AltRight, SuperLeft, SuperRight,
    Backquote, Minus, Equal, BracketLeft, BracketRight, Backslash, Semicolon, Quote, Comma, Period, Slash,
    Numpad0, Numpad1, Numpad2, Numpad3, Numpad4, Numpad5, Numpad6, Numpad7, Numpad8, Numpad9, NumpadAdd,
    NumpadSubtract, NumpadMultiply, NumpadDivide, NumpadDecimal, NumpadEnter,
    F1, F2, F3, F4, F5, F6, F7, F8, F9, F10, F11, F12,
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_the_old_keys() {
        let b = Bindings::default();
        assert_eq!(b.get(Action::Throttle).key, Some(KeyCode::ArrowUp));
        assert_eq!(b.get(Action::Clutch), Binding::default());
        assert_eq!(b.action_for_key(KeyCode::KeyZ), Some(Action::GearDown));
        assert_eq!(b.action_for_key(KeyCode::KeyW), None);
        assert_eq!(b.action_for_key(KeyCode::Escape), Some(Action::Pause));
        assert_eq!((b.action_for_key(KeyCode::KeyC), b.action_for_key(KeyCode::KeyX)), (Some(Action::Camera), None));
        // The camera adjust key went with the Camera page (`crate::hud::camera_page`): an old line is ignored.
        assert_eq!(Bindings::parse("[controls]\ncamera adjust = KeyX\n"), b);
        let pad = PadInput { element: Element::Code(65824), range: Range::Plus, device: None };
        assert_eq!(Binding { key: None, pad: Some(pad) }.label(), "#65824 +");
        assert_eq!(Binding::default().label(), "unbound");
        assert_eq!(b.get(Action::Pause).pad.map(PadInput::format).as_deref(), Some("Start"));
        assert!(b.actions.iter().filter(|a| a.pad.is_some()).count() == 1);
        assert_eq!(Bindings::parse(""), b);
        assert_eq!(Bindings::parse("[launcher]\ncar = XSARA\n"), b);
    }

    #[test]
    fn the_held_ignition_sets_the_payload_s_aux_bit() {
        let b = Bindings::default();
        assert_eq!(b.get(Action::Ignition), Binding { key: Some(KeyCode::KeyE), pad: None });
        assert_eq!(b.action_for_key(KeyCode::KeyE), Some(Action::Ignition));
        let aux = |c: Controls| u32::from_le_bytes(c.payload()[20..24].try_into().unwrap()) & 1 << 24 != 0;
        let mut held = [false; ACTIONS];
        let off = b.controls(held, [0.0; ACTIONS], Controls::default());
        assert!(!off.ignition && !aux(off));
        held[Action::Ignition as usize] = true;
        assert!(aux(b.controls(held, [0.0; ACTIONS], Controls::default())));
        let mut pads = [0.0; ACTIONS];
        pads[Action::Ignition as usize] = 1.0;
        assert!(b.controls([false; ACTIONS], pads, Controls::default()).ignition);
    }

    #[test]
    fn ini_round_trip() {
        let shifter = shifter::Settings {
            advanced: true,
            engagement: PerCar::Fixed(Engagement::Dog),
            dogleg: PerCar::Auto,
            clutch_threshold: 0.5,
        };
        let mut b = Bindings { shifter, ..Default::default() };
        b.actions[Action::Throttle as usize].pad =
            Some(PadInput { element: Element::Button(Button::RightTrigger2), range: Range::Plus, device: None });
        b.actions[Action::SteerLeft as usize].pad =
            Some(PadInput { element: Element::Axis(Axis::LeftStickX), range: Range::Minus, device: None });
        b.actions[Action::Clutch as usize].pad =
            Some(PadInput { element: Element::Code(65538), range: Range::FullMinus, device: None });
        b.actions[Action::Clutch as usize].key = Some(KeyCode::KeyC);
        b.actions[Action::Restart as usize].key = None;
        let shifter = Some(Device { vendor: 0x044f, product: 0xb660 });
        b.actions[Action::Gear1 as usize].pad =
            Some(PadInput { element: Element::Code(65824), range: Range::Plus, device: shifter });
        b.actions[Action::Reverse as usize].pad =
            Some(PadInput { element: Element::Button(Button::South), range: Range::Plus, device: shifter });
        let text = b.to_text();
        assert!(text.contains("gear 1 pad = #65824 + @044f:b660\n"), "{text}");
        assert!(text.contains("reverse pad = South @044f:b660\n"), "{text}");
        assert!(text.contains("throttle pad = RightTrigger2\n"), "{text}");
        assert!(text.contains("steer left pad = LeftStickX -\n"), "{text}");
        assert!(text.contains("clutch pad = #65538 full-\n"), "{text}");
        assert!(text.contains("restart = \n"), "{text}");
        assert!(
            text.contains("h-shifter = advanced\nengagement = dog\ndogleg = auto\nclutch threshold = 0.5\n"),
            "{text}"
        );
        assert_eq!(Bindings::parse(&text), b);
        // Next to the launcher's own section, in either order.
        assert_eq!(Bindings::parse(&format!("[launcher]\nsize = 800x600\n{text}")), b);
        // A line left out keeps its default.
        let partial = Bindings::parse("[controls]\nbrake = KeyB\nthrottle = NoSuchKey\n");
        assert_eq!(partial.get(Action::Brake).key, Some(KeyCode::KeyB));
        assert_eq!(partial.get(Action::Throttle).key, Some(KeyCode::ArrowUp));
        assert_eq!(partial.get(Action::Restart).key, Some(KeyCode::KeyR));
    }

    #[test]
    fn pad_ranges() {
        let input = |range| PadInput { element: Element::Axis(Axis::LeftZ), range, device: None };
        assert_eq!(input(Range::Plus).shape(0.5), 0.5);
        assert_eq!(input(Range::Plus).shape(-0.5), 0.0);
        assert_eq!(input(Range::Minus).shape(-0.5), 0.5);
        assert_eq!(input(Range::FullPlus).shape(-1.0), 0.0);
        assert_eq!(input(Range::FullPlus).shape(0.0), 0.5);
        assert_eq!(input(Range::FullMinus).shape(-1.0), 1.0);
        // A pedal gilrs has no value for yet reads at rest, not half pressed.
        assert_eq!(input(Range::FullMinus).shape(0.0), 0.5);
        for range in [Range::Plus, Range::Minus, Range::FullPlus, Range::FullMinus] {
            assert_eq!(input(range).shape_read(None), 0.0);
        }
        assert_eq!(input(Range::FullMinus).shape_read(Some(-1.0)), 1.0);
        let moved =
            |rest, value| PadInput::moved(Element::Axis(Axis::LeftZ), rest, value, PRESS).map(|input| input.range);
        assert_eq!(moved(0.0, 0.8), Some(Range::Plus));
        assert_eq!(moved(0.0, -0.8), Some(Range::Minus));
        assert_eq!(moved(-1.0, 0.0), Some(Range::FullPlus));
        assert_eq!(moved(1.0, 0.0), Some(Range::FullMinus));
        assert_eq!(moved(0.0, 0.3), None);
        let steer = PadInput::moved(Element::Axis(Axis::LeftStickX), 0.0, -0.3, STEER_MOVE).expect("a quarter turn");
        assert_eq!((steer.range, steer.opposite().range), (Range::Minus, Range::Plus));
        assert_eq!(Element::axis(Axis::Unknown, 7), Element::Code(7));
        assert_eq!(Element::button(Button::South, 7), Element::Button(Button::South));
    }

    #[test]
    fn keys_give_full_values() {
        let b = Bindings::default();
        let mut held = [false; ACTIONS];
        held[Action::Throttle as usize] = true;
        held[Action::SteerRight as usize] = true;
        let last = Controls { flags: crate::live::GEAR_UP, ..Default::default() };
        let c = b.controls(held, [0.0; ACTIONS], last);
        assert_eq!((c.throttle, c.brake, c.clutch), (1.0, 0.0, 0.0));
        assert_eq!((c.steering, c.keys_steer), (1.0, true));
        assert_eq!(c.flags, crate::live::GEAR_UP);
        // Released: centred (the game's filter moves the wheel back).
        let c = b.controls([false; ACTIONS], [0.0; ACTIONS], c);
        assert_eq!(c.steering, 0.0);
    }

    #[test]
    fn pads_mix_with_keys() {
        let mut b = Bindings::default();
        b.actions[Action::SteerRight as usize].pad =
            Some(PadInput { element: Element::Axis(Axis::LeftStickX), range: Range::Plus, device: None });
        let mut held = [false; ACTIONS];
        held[Action::SteerLeft as usize] = true;
        held[Action::Brake as usize] = true;
        let mut pads = [0.0; ACTIONS];
        pads[Action::Throttle as usize] = 0.4;
        pads[Action::Brake as usize] = 0.3;
        // A held key steers.
        pads[Action::SteerRight as usize] = 0.1;
        let c = b.controls(held, pads, Controls::default());
        assert_eq!((c.throttle, c.brake), (0.4, 1.0));
        assert_eq!((c.steering, c.keys_steer), (-1.0, true));
        // Else the pad, as it is: no deadzone, also at the centre.
        held[Action::SteerLeft as usize] = false;
        let c = b.controls(held, pads, c);
        assert!((c.steering - 0.1).abs() < 1e-6 && !c.keys_steer);
        pads[Action::SteerRight as usize] = 0.0;
        let c = b.controls(held, pads, c);
        assert_eq!((c.steering, c.keys_steer), (0.0, false));
        pads[Action::SteerRight as usize] = 0.6;
        let mut before = [0.0; ACTIONS];
        before[Action::GearUp as usize] = 0.9;
        pads[Action::GearUp as usize] = 1.0;
        pads[Action::GearDown as usize] = 1.0;
        assert_eq!(pad_presses(before, pads).collect::<Vec<_>>(), [Action::SteerRight, Action::GearDown]);
    }

    #[test]
    fn direct_gears() {
        assert_eq!(Action::Reverse.gear(), Some(0));
        assert_eq!(Action::Gear1.gear(), Some(2));
        assert_eq!(Action::Gear6.gear(), Some(7));
        assert_eq!(Action::GearUp.gear(), None);
        let mut b = Bindings::default();
        assert!(!b.neutral_on_release());
        b.actions[Action::Gear3 as usize].key = Some(KeyCode::Digit3);
        assert!(b.neutral_on_release());
        b.actions[Action::Neutral as usize].key = Some(KeyCode::Digit0);
        assert!(!b.neutral_on_release());
        let mut pads = [0.0; ACTIONS];
        assert!(!gear_held([false; ACTIONS], pads));
        pads[Action::Gear3 as usize] = 1.0;
        assert!(gear_held([false; ACTIONS], pads));
        pads[Action::GearUp as usize] = 1.0;
        pads[Action::Gear3 as usize] = 0.0;
        assert!(!gear_held([false; ACTIONS], pads));
    }
}
