//! The live mode's control bindings: a key and a device input for each action, read from a `[controls]` section of
//! `key = value` lines (`rbr_formats::ini`; the launcher keeps it in `launcher-settings.ini` and passes that file to
//! `viewer --bindings`), and the device poll's bindings they make ([`Bindings::slots`]).
//!
//! ```text
//! [controls]
//! throttle = ArrowUp
//! throttle pad = 046d:c266/0 axis 2
//! steer left pad = 046d:c266/0 axis 0
//! steer right pad = 046d:c266/0 axis 0
//! gear 1 pad = 044f:b660/0 button 3
//! clutch =
//! invert axes = throttle, brake
//! input rate = 1000
//! ```
//!
//! A key is winit's `KeyCode` name. A device input names the joystick by its USB vendor and product ids and its index
//! among the joysticks with them (`crate::devices`), then an axis or button by its place in the order the game sees
//! the device's objects (`rbr_gameplay::poll::wine::objects`: the DirectInput instance an RSF profile's `.acm` names).
//! An empty value unbinds; a missing line, or one that does not parse, keeps the default. A `pad` line in the older
//! gamepad format (gilrs's names) is dropped ([`Bindings::dropped`]). The defaults are the keys the viewer always had
//! and no device input.
//!
//! The driving actions take the game's rules (`rbr_gameplay::poll`): each of its action slots reads one object, the
//! device input when there is one, else the key; steer left and right on the same axis are the steering axis, throttle
//! and brake on the same axis the combined one; an axis reads as the game reads it, and `invert axes` flips a channel
//! (`input.ini`'s `Invert*Axis`). The other actions (pause, camera, restart and the rest of ours) take both their key
//! and their device input. `input rate` is the poll's rate, Hz (NGP's `inputRate`).
//!
//! The direct gear actions (an H-shifter, as NGP has them) select their gear when pressed. With a gear bound and
//! neutral not, releasing every gear selects neutral ([`Bindings::neutral_on_release`]). `h-shifter = advanced` makes
//! a gear need the clutch or matched revs instead (`crate::shifter`), with `engagement = auto|synchro|dog`,
//! `dogleg = auto|on|off` and `clutch threshold = 0..1`.

use winit::keyboard::KeyCode;

use crate::shifter::{self, Engagement, PerCar};
use rbr_formats::saves::action_map::{ActionMap, Object as Target};
use rbr_gameplay::poll::{self, Invert};

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

/// A joystick: its USB vendor and product ids, and its index among the joysticks with them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Device {
    pub vendor: u16,
    pub product: u16,
    pub index: u32,
}

impl Device {
    /// `046d:c266/0`.
    pub fn format(self) -> String {
        format!("{:04x}:{:04x}/{}", self.vendor, self.product, self.index)
    }

    pub fn parse(text: &str) -> Option<Device> {
        let (ids, index) = text.split_once('/')?;
        let (vendor, product) = ids.split_once(':')?;
        let id = |hex: &str| u16::from_str_radix(hex, 16).ok().filter(|_| hex.len() == 4);
        Some(Device { vendor: id(vendor)?, product: id(product)?, index: index.parse().ok()? })
    }
}

/// A device input: the joystick and its axis or button ([`Target::Axis`] or [`Target::Button`], the instance).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PadInput {
    pub device: Device,
    pub object: Target,
}

impl PadInput {
    /// The ini value, also the launcher's label: `046d:c266/0 axis 2`, `044f:b660/0 button 3`.
    pub fn format(self) -> String {
        match self.object {
            Target::Axis(n) => format!("{} axis {n}", self.device.format()),
            Target::Button(n) => format!("{} button {n}", self.device.format()),
            other => format!("{} {other:?}", self.device.format()),
        }
    }

    pub fn parse(text: &str) -> Option<PadInput> {
        let mut parts = text.split_whitespace();
        let device = Device::parse(parts.next()?)?;
        let n = |p: Option<&str>| p?.parse().ok();
        let object = match parts.next()? {
            "axis" => Target::Axis(n(parts.next())?),
            "button" => Target::Button(n(parts.next())?),
            _ => return None,
        };
        parts.next().is_none().then_some(PadInput { device, object })
    }

    pub fn is_axis(self) -> bool {
        matches!(self.object, Target::Axis(_))
    }
}

/// One action's key and device input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Binding {
    pub key: Option<KeyCode>,
    pub pad: Option<PadInput>,
}

impl Binding {
    /// How the HUD names it: the key without winit's `Key` or `Digit` (`X`, `F1`, `ArrowUp`), else the device
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

/// The game's action slots ours map to one to one (`rbr_gameplay::poll`; steering, combined and the pedals' halves
/// are [`Bindings::slots`]'s).
const SLOTS: [(usize, Action); 17] = [
    (poll::LEFT, Action::SteerLeft),
    (poll::RIGHT, Action::SteerRight),
    (poll::THROTTLE, Action::Throttle),
    (poll::BRAKE, Action::Brake),
    (poll::HANDBRAKE, Action::Handbrake),
    (poll::GEAR_UP, Action::GearUp),
    (poll::GEAR_DOWN, Action::GearDown),
    (poll::CLUTCH, Action::Clutch),
    (poll::IGNITION, Action::Ignition),
    (poll::REVERSE, Action::Reverse),
    (poll::REVERSE + 1, Action::Neutral),
    (poll::REVERSE + 2, Action::Gear1),
    (poll::REVERSE + 3, Action::Gear2),
    (poll::REVERSE + 4, Action::Gear3),
    (poll::REVERSE + 5, Action::Gear4),
    (poll::REVERSE + 6, Action::Gear5),
    (poll::REVERSE + 7, Action::Gear6),
];

/// The game's slots `Tic` leaves to the menus: an import sets them, the poll does not read them.
const OTHER_SLOTS: [(usize, Action); 2] = [(poll::CAMERA, Action::Camera), (poll::PAUSE, Action::Pause)];

/// The channels `invert axes` names, in [`Invert`] order.
const INVERT: [&str; 6] = ["steering", "throttle", "brake", "combined", "handbrake", "clutch"];

/// The poll's rate by default, Hz (NGP's `inputRate`).
pub const INPUT_RATE: u32 = 1000;

#[derive(Debug, Clone, PartialEq)]
pub struct Bindings {
    /// In [`Action::ALL`] order.
    pub actions: [Binding; ACTIONS],
    pub shifter: shifter::Settings,
    /// The axes `input.ini` would invert.
    pub invert: Invert,
    /// The poll's rate, Hz.
    pub input_rate: u32,
    /// `pad` lines of the older gamepad format were dropped.
    pub dropped: bool,
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
                key(KeyCode::Escape),
                key(KeyCode::End),
            ],
            shifter: shifter::Settings::default(),
            invert: Invert::default(),
            input_rate: INPUT_RATE,
            dropped: false,
        }
    }
}

fn invert_flags(invert: &mut Invert) -> [&mut bool; 6] {
    [
        &mut invert.steering,
        &mut invert.throttle,
        &mut invert.brake,
        &mut invert.combined,
        &mut invert.handbrake,
        &mut invert.clutch,
    ]
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
            if let Some(value) = section.get(&format!("{} pad", action.name())).filter(|v| !v.is_empty()) {
                binding.pad = PadInput::parse(value);
                out.dropped |= binding.pad.is_none();
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
        if let Some(value) = section.get("invert axes") {
            let names: Vec<&str> = value.split(',').map(str::trim).collect();
            for (flag, name) in invert_flags(&mut out.invert).into_iter().zip(INVERT) {
                *flag = names.contains(&name);
            }
        }
        if let Some(rate) = section.get("input rate").and_then(|v| v.parse::<u32>().ok()) {
            out.input_rate = rate.clamp(1, 1000);
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
        let mut invert = self.invert;
        let inverted: Vec<&str> =
            invert_flags(&mut invert).into_iter().zip(INVERT).filter(|(f, _)| **f).map(|(_, n)| n).collect();
        let mut out = format!(
            "[controls]\nh-shifter = {}\nengagement = {engagement}\ndogleg = {dogleg}\nclutch threshold = {}\n\
             invert axes = {}\ninput rate = {}\n",
            if shifter.advanced { "advanced" } else { "normal" },
            shifter.clutch_threshold,
            inverted.join(", "),
            self.input_rate,
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

    /// The axis both actions' device inputs are, if they are the same one.
    fn shared_axis(&self, a: Action, b: Action) -> Option<PadInput> {
        self.get(a).pad.filter(|p| p.is_axis() && self.get(b).pad == Some(*p))
    }

    /// The device poll's bindings (`rbr_gameplay::poll::Poll::bind`): per game action slot, its device (`None` the
    /// keyboard, whose objects are DIK codes) and object. One object per slot: the device input, else the key.
    pub fn slots(&self) -> Vec<(usize, Option<Device>, Target)> {
        let steering = self.shared_axis(Action::SteerLeft, Action::SteerRight);
        let combined = self.shared_axis(Action::Throttle, Action::Brake);
        let shared = [(poll::STEERING, steering), (poll::COMBINED, combined)];
        let mut out: Vec<_> = shared.iter().filter_map(|&(s, p)| Some((s, Some(p?.device), p?.object))).collect();
        for &(slot, action) in &SLOTS {
            let b = self.get(action);
            let pad = b.pad.filter(|_| match action {
                Action::SteerLeft | Action::SteerRight => steering.is_none(),
                Action::Throttle | Action::Brake => combined.is_none(),
                _ => true,
            });
            match (pad, b.key.and_then(dik)) {
                (Some(p), _) => out.push((slot, Some(p.device), p.object)),
                (None, Some(code)) => out.push((slot, None, Target::Button(u32::from(code)))),
                (None, None) => {}
            }
        }
        out
    }

    /// The actions' device inputs (for the presses and the shifter), in [`Action::ALL`] order.
    pub fn pads(&self) -> impl Iterator<Item = (Action, PadInput)> + '_ {
        Action::ALL.into_iter().filter_map(|a| Some((a, self.get(a).pad?)))
    }

    /// The devices the bindings name, each once.
    pub fn devices(&self) -> Vec<Device> {
        let mut out: Vec<Device> = Vec::new();
        for (_, p) in self.pads() {
            if !out.contains(&p.device) {
                out.push(p.device);
            }
        }
        out
    }

    /// An RSF profile's action map in place of the game's actions' bindings: the first record whose devices `device`
    /// resolves (`Some(None)` the keyboard, `Some(Some(_))` a joystick), as `FindRec` takes the first that fits. Each
    /// slot the record binds sets its action's key (a keyboard key) or device input; the driving actions it leaves
    /// out are unbound, ours keep theirs. Returns the record's index.
    pub fn import(&mut self, map: &ActionMap, device: impl Fn(&[u8; 16]) -> Option<Option<Device>>) -> Option<usize> {
        let (index, record, devices) = map.records.iter().enumerate().find_map(|(k, r)| {
            let devices: Option<Vec<_>> = r.devices.iter().map(&device).collect();
            Some((k, r, devices?))
        })?;
        // Per slot the record's last binding, as `ApplyMaps` leaves it.
        let mut slots: [Option<(Option<Device>, Target)>; poll::ACTIONS] = [None; poll::ACTIONS];
        for b in &record.bindings {
            let (Some(&source), Some(slot)) = (devices.get(b.device as usize), slots.get_mut(b.action as usize)) else {
                continue;
            };
            *slot = Some((source, b.object()));
        }
        for &(_, action) in &SLOTS {
            self.actions[action as usize] = Binding::default();
        }
        for &(slot, action) in SLOTS.iter().chain(&OTHER_SLOTS) {
            let binding = &mut self.actions[action as usize];
            match slots[slot] {
                Some((None, Target::Button(code))) => binding.key = key_of(code),
                Some((Some(device), object)) => binding.pad = Some(PadInput { device, object }),
                _ => {}
            }
        }
        for (slot, a, b) in
            [(poll::STEERING, Action::SteerLeft, Action::SteerRight), (poll::COMBINED, Action::Throttle, Action::Brake)]
        {
            if let Some((Some(device), object @ Target::Axis(_))) = slots[slot] {
                self.actions[a as usize].pad = Some(PadInput { device, object });
                self.actions[b as usize].pad = Some(PadInput { device, object });
            }
        }
        Some(index)
    }

    /// Bit `k` set for the direct gear action of gear word `k` (reverse, neutral, first to sixth) when `test` holds.
    fn gear_bits(&self, test: impl Fn(Action) -> bool) -> u8 {
        Action::ALL.into_iter().filter_map(|a| a.gear().filter(|_| test(a))).fold(0, |bits, g| bits | 1 << g)
    }

    /// The direct gear actions bound, as [`crate::live::Controls::gears_bound`] has them.
    pub fn gears_bound(&self) -> u8 {
        self.gear_bits(|a| self.get(a) != Binding::default())
    }

    /// Whether releasing every gear selects neutral: a direct gear is bound and neutral is not (NGP's H-shifter).
    pub fn neutral_on_release(&self) -> bool {
        let bound = |a: Action| self.get(a) != Binding::default();
        !bound(Action::Neutral) && Action::ALL.into_iter().any(|a| a.gear().is_some() && bound(a))
    }
}

/// Whether a direct gear action is held, by its key or its device input.
pub fn gear_held(held: [bool; ACTIONS], pads: [f32; ACTIONS]) -> bool {
    Action::ALL.into_iter().any(|a| a.gear().is_some() && (held[a as usize] || pads[a as usize] > PRESS))
}

/// The actions whose device input went above the press threshold since `before`.
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

pub fn dik(key: KeyCode) -> Option<u8> {
    use KeyCode::*;
    Some(match key {
        Escape => 0x01,
        Digit1 => 0x02,
        Digit2 => 0x03,
        Digit3 => 0x04,
        Digit4 => 0x05,
        Digit5 => 0x06,
        Digit6 => 0x07,
        Digit7 => 0x08,
        Digit8 => 0x09,
        Digit9 => 0x0a,
        Digit0 => 0x0b,
        Minus => 0x0c,
        Equal => 0x0d,
        Backspace => 0x0e,
        Tab => 0x0f,
        KeyQ => 0x10,
        KeyW => 0x11,
        KeyE => 0x12,
        KeyR => 0x13,
        KeyT => 0x14,
        KeyY => 0x15,
        KeyU => 0x16,
        KeyI => 0x17,
        KeyO => 0x18,
        KeyP => 0x19,
        BracketLeft => 0x1a,
        BracketRight => 0x1b,
        Enter => 0x1c,
        ControlLeft => 0x1d,
        KeyA => 0x1e,
        KeyS => 0x1f,
        KeyD => 0x20,
        KeyF => 0x21,
        KeyG => 0x22,
        KeyH => 0x23,
        KeyJ => 0x24,
        KeyK => 0x25,
        KeyL => 0x26,
        Semicolon => 0x27,
        Quote => 0x28,
        Backquote => 0x29,
        ShiftLeft => 0x2a,
        Backslash => 0x2b,
        KeyZ => 0x2c,
        KeyX => 0x2d,
        KeyC => 0x2e,
        KeyV => 0x2f,
        KeyB => 0x30,
        KeyN => 0x31,
        KeyM => 0x32,
        Comma => 0x33,
        Period => 0x34,
        Slash => 0x35,
        ShiftRight => 0x36,
        NumpadMultiply => 0x37,
        AltLeft => 0x38,
        Space => 0x39,
        CapsLock => 0x3a,
        F1 => 0x3b,
        F2 => 0x3c,
        F3 => 0x3d,
        F4 => 0x3e,
        F5 => 0x3f,
        F6 => 0x40,
        F7 => 0x41,
        F8 => 0x42,
        F9 => 0x43,
        F10 => 0x44,
        NumLock => 0x45,
        ScrollLock => 0x46,
        Numpad7 => 0x47,
        Numpad8 => 0x48,
        Numpad9 => 0x49,
        NumpadSubtract => 0x4a,
        Numpad4 => 0x4b,
        Numpad5 => 0x4c,
        Numpad6 => 0x4d,
        NumpadAdd => 0x4e,
        Numpad1 => 0x4f,
        Numpad2 => 0x50,
        Numpad3 => 0x51,
        Numpad0 => 0x52,
        NumpadDecimal => 0x53,
        F11 => 0x57,
        F12 => 0x58,
        NumpadEnter => 0x9c,
        ControlRight => 0x9d,
        NumpadDivide => 0xb5,
        AltRight => 0xb8,
        Home => 0xc7,
        ArrowUp => 0xc8,
        PageUp => 0xc9,
        ArrowLeft => 0xcb,
        ArrowRight => 0xcd,
        End => 0xcf,
        ArrowDown => 0xd0,
        PageDown => 0xd1,
        Insert => 0xd2,
        Delete => 0xd3,
        SuperLeft => 0xdb,
        SuperRight => 0xdc,
        _ => return None,
    })
}

/// The bindable key with DIK code `code`.
fn key_of(code: u32) -> Option<KeyCode> {
    KEYS.iter().map(|&(k, _)| k).find(|&k| dik(k).map(u32::from) == Some(code))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rbr_formats::saves::action_map::{Binding as AcmBinding, Record};
    use rbr_gameplay::poll::{keyboard, wine, Device as PollDevice, Poll, DEFAULT_RANGE, KEYBOARD};

    const G923: Device = Device { vendor: 0x046d, product: 0xc266, index: 0 };
    const SHIFTER: Device = Device { vendor: 0x044f, product: 0xb660, index: 0 };

    fn pad(device: Device, object: Target) -> Option<PadInput> {
        Some(PadInput { device, object })
    }

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
        assert_eq!(Binding::default().label(), "unbound");
        assert!(b.actions.iter().all(|a| a.pad.is_none()));
        assert_eq!((b.invert, b.input_rate, b.dropped), (Invert::default(), 1000, false));
        assert_eq!(Bindings::parse(""), b);
        assert_eq!(Bindings::parse("[launcher]\ncar = XSARA\n"), b);
    }

    #[test]
    fn ini_round_trip() {
        let shifter = shifter::Settings {
            advanced: true,
            engagement: PerCar::Fixed(Engagement::Dog),
            dogleg: PerCar::Auto,
            clutch_threshold: 0.5,
        };
        let mut b = Bindings { shifter, input_rate: 500, ..Default::default() };
        (b.invert.throttle, b.invert.clutch) = (true, true);
        b.actions[Action::Throttle as usize].pad = pad(G923, Target::Axis(2));
        b.actions[Action::SteerLeft as usize].pad = pad(G923, Target::Axis(0));
        b.actions[Action::Clutch as usize].key = Some(KeyCode::KeyC);
        b.actions[Action::Restart as usize].key = None;
        b.actions[Action::Gear1 as usize].pad = pad(SHIFTER, Target::Button(3));
        let text = b.to_text();
        assert!(text.contains("gear 1 pad = 044f:b660/0 button 3\n"), "{text}");
        assert!(text.contains("throttle pad = 046d:c266/0 axis 2\n"), "{text}");
        assert!(text.contains("restart = \n"), "{text}");
        assert!(text.contains("invert axes = throttle, clutch\ninput rate = 500\n"), "{text}");
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
        assert_eq!(Binding { key: None, pad: pad(G923, Target::Axis(0)) }.label(), "046d:c266/0 axis 0");
    }

    #[test]
    fn older_gamepad_lines_are_dropped() {
        let b = Bindings::parse(
            "[controls]\nthrottle pad = RightTrigger2\ngear 1 pad = #65824 + @044f:b660\nbrake pad =\n",
        );
        assert!(b.dropped);
        assert_eq!((b.get(Action::Throttle).pad, b.get(Action::Throttle).key), (None, Some(KeyCode::ArrowUp)));
        assert!(!Bindings::parse(&b.to_text()).dropped);
        assert!(!Bindings::parse("[controls]\nbrake pad =\n").dropped);
    }

    #[test]
    fn the_slots_take_the_device_input_over_the_key() {
        let mut b = Bindings::default();
        assert!(b.slots().contains(&(poll::LEFT, None, Target::Button(0xcb))));
        b.actions[Action::SteerLeft as usize].pad = pad(G923, Target::Axis(0));
        b.actions[Action::SteerRight as usize].pad = pad(G923, Target::Axis(0));
        b.actions[Action::Throttle as usize].pad = pad(G923, Target::Axis(2));
        b.actions[Action::GearUp as usize].pad = pad(G923, Target::Button(4));
        let slots = b.slots();
        assert!(slots.contains(&(poll::STEERING, Some(G923), Target::Axis(0))));
        // The steering axis is shared: the arrows keep LEFT and RIGHT.
        assert!(slots.contains(&(poll::LEFT, None, Target::Button(0xcb))));
        assert!(slots.contains(&(poll::THROTTLE, Some(G923), Target::Axis(2))));
        assert!(slots.contains(&(poll::GEAR_UP, Some(G923), Target::Button(4))));
        assert!(!slots.iter().any(|s| s.0 == poll::COMBINED || s.0 == poll::PAUSE));
        assert_eq!(b.devices(), [G923]);
        assert_eq!(key_of(0xcb), Some(KeyCode::ArrowLeft));
    }

    /// An `.acm` imported, then bound through our slots and polled: the game's `Tic` over our bindings.
    #[test]
    fn an_imported_map_polls_through_our_bindings() {
        let wheel = [7; 16];
        let (axis, button) = (|n: u32| n << 8 | 0x02, |n: u32| n << 8 | 0x04);
        let b = |device, object, action: usize| AcmBinding { device, object, action: action as u32 };
        let map = ActionMap {
            records: vec![
                Record { devices: vec![[9; 16]], bindings: vec![b(0, axis(0), poll::STEERING)] },
                Record {
                    devices: vec![KEYBOARD, wheel],
                    bindings: vec![
                        b(1, axis(0) | 0x0100_0000, poll::STEERING),
                        b(1, axis(2), poll::THROTTLE),
                        b(1, button(4), poll::GEAR_UP),
                        b(0, button(0xcb), poll::LEFT),
                        b(0, button(0x01), poll::PAUSE),
                    ],
                },
            ],
        };
        let mut ours = Bindings::default();
        ours.actions[Action::Clutch as usize].key = Some(KeyCode::KeyC);
        let resolve = |g: &[u8; 16]| match *g {
            KEYBOARD => Some(None),
            g if g == wheel => Some(Some(G923)),
            _ => None,
        };
        assert_eq!(ours.import(&map, resolve), Some(1));
        assert_eq!(ours.get(Action::SteerRight).pad, pad(G923, Target::Axis(0)));
        assert_eq!(ours.get(Action::SteerLeft).key, Some(KeyCode::ArrowLeft));
        // RIGHT and the clutch were not in the record: unbound. Ours and the pause stay keys.
        assert_eq!((ours.get(Action::SteerRight).key, ours.get(Action::Clutch)), (None, Binding::default()));
        assert_eq!(
            (ours.get(Action::Pause).key, ours.get(Action::Restart).key),
            (Some(KeyCode::Escape), Some(KeyCode::KeyR))
        );

        let objects = wine::objects(3, 8);
        let mut p = Poll::new(vec![keyboard(), PollDevice::new(wheel, &objects, |_| DEFAULT_RANGE)]);
        let slots: Vec<_> = ours.slots().into_iter().map(|(s, d, o)| (s, usize::from(d.is_some()), o)).collect();
        p.bind(&slots);
        let wheel_state = |x: i32, z: i32, b4: bool| {
            let mut buttons = [false; 8];
            buttons[4] = b4;
            wine::state(&[x, 32768, z], &buttons)
        };
        let keys = [0u8; 256];
        for (x, z) in [(32768, 65535), (20000, 30000), (16384, 0)] {
            p.devices[0].update(&keys, 0.0);
            p.devices[1].update(&wheel_state(x, z, false), 0.0);
        }
        let raw = p.tic(true);
        assert_eq!((raw.channels[0], raw.channels[1]), ((16384.0f32 / 65535.0) * 2.0 - 1.0, 1.0));
        p.devices[1].update(&wheel_state(16384, 0, true), 0.0);
        assert_eq!(p.tic(true).flags, 1, "gear up");
        // The throttle pedal inverted (one resting at the other end): `input.ini`'s 1 - v.
        p.invert = Invert { throttle: true, ..Invert::default() };
        assert_eq!(p.tic(true).channels[1], 0.0);
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
        assert_eq!(b.gears_bound(), 1 << 4);
        b.actions[Action::Neutral as usize].key = Some(KeyCode::Digit0);
        assert!(!b.neutral_on_release());
        let mut pads = [0.0; ACTIONS];
        assert!(!gear_held([false; ACTIONS], pads));
        pads[Action::Gear3 as usize] = 1.0;
        assert!(gear_held([false; ACTIONS], pads));
        pads[Action::GearUp as usize] = 1.0;
        pads[Action::Gear3 as usize] = 0.0;
        assert!(!gear_held([false; ACTIONS], pads));
        let mut before = [0.0; ACTIONS];
        before[Action::GearUp as usize] = 0.9;
        pads[Action::GearDown as usize] = 1.0;
        assert_eq!(pad_presses(before, pads).collect::<Vec<_>>(), [Action::GearDown]);
    }
}
