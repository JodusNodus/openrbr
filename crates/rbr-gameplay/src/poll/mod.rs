//! The device poll: raw DirectInput device states to the raw car control the input filter takes
//! (`CSigCarControlInput`, [`Raw`]), as `USInputManagerImpl::Tic` (0x4cbf50) and NGP's additions to it do.
//!
//! - Devices (`CInputDevice::CInputDevice` 0x4ca7e0, `E29EnumObjectsCb` 0x4ca440): the game sets its own data format, one
//!   entry per enumerated object in order; an axis (`dwType & 3`) takes a DWORD, anything else one byte. An axis's
//!   range is the device's (`GetProperty(DIPROP_RANGE)` by id; the game sets none). The keyboard keeps
//!   `c_dfDIKeyboard`, its entries the enumerated keys ([`keyboard`]).
//! - Each poll (`SInputDeviceImpl::Poll` 0x4c8500) updates every entry from `GetDeviceState` (`E29AxisEntry::Update`
//!   0x4cad30): an axis is `((raw - min) / (max - min)) * 2 - 1` in f32, a button 1 or 0. A failed poll leaves the
//!   entries.
//! - An axis reads 0 until it has moved twice by 0.001 or more since its device was made (`Update`'s counter +0x2c,
//!   `Bound` 0x4cad20).
//! - Actions (`E28Svc`, 21 slots; the profile's `.acm` binds them, `E28FileMap::FindRec` 0x4c4b50 and `ApplyMaps`
//!   0x4c4ee0): `GetActionValue` (0x4c2610) negates an axis for THROTTLE, ACCELERATE_AND_BRAKE, BRAKE, HANDBRAKE and
//!   CLUTCH (`InitImpl` 0x4c28c0), then maps an axis to `(v + 1) / 2` for LEFT, RIGHT, THROTTLE, BRAKE, HANDBRAKE and
//!   CLUTCH. `IsActionDown` (0x4c25d0) is a press: the value above 0 and changed since the previous poll (0.2 for a
//!   button; for an axis 1.25 times the poll's rate argument, 0.04 without one, `Changed` 0x4cacb0).
//! - `Tic` combines them ([`Poll::tic`]); `input.ini`'s `Invert*Axis` flip a value to `1 - v` ([`Invert`]).
//! - NGP: IGNITION is a level, the value above 0.99 (`0x4cc3e5` -> NGP's `GetActionValue(12) > 0.99`), and the
//!   emit (`0x4cc42e`) adds the direct gear actions with the automatic gearbox off ([`Poll::ngp_gears`],
//!   [`crate::control::ngp_direct_gears`]).
//!
//! The menu's "press a control" capture in `Tic` (+0x210) and the force-feedback send at its start are not ported:
//! neither is on the driving path. `session` replays a recorded RSF session's polls against its payloads.

pub mod session;
pub mod wine;

use crate::control::{ngp_direct_gears, NGP_GEAR_ACTIONS, NGP_IGNITION_PRESS};
use crate::filter::{Raw, CHANNELS};
use rbr_formats::saves::action_map::ActionMap;

/// The action slots (`rbr_formats::saves::action_map::ACTIONS` order).
pub const ACTIONS: usize = 21;
const STEERING: usize = 0;
const LEFT: usize = 1;
const RIGHT: usize = 2;
const THROTTLE: usize = 3;
const COMBINED: usize = 4;
const BRAKE: usize = 5;
const HANDBRAKE: usize = 6;
const GEAR_UP: usize = 7;
const GEAR_DOWN: usize = 8;
const CLUTCH: usize = 11;
const IGNITION: usize = 12;
/// REVERSE, the first of NGP's direct gear actions (R, N, first to sixth).
const REVERSE: usize = 13;

/// `InitImpl`'s slot flag +0x10: the axis is negated.
const NEGATED: [usize; 5] = [THROTTLE, COMBINED, BRAKE, HANDBRAKE, CLUTCH];
/// `GetActionValue`'s switch (table 0x4c26c0): an axis is mapped from -1..1 to 0..1.
const HALF_RANGE: [usize; 6] = [LEFT, RIGHT, THROTTLE, BRAKE, HANDBRAKE, CLUTCH];
/// A change of at least this counts towards an axis's first two moves (0x795e94).
const MOVE: f32 = 0.001;
/// `Changed`: a button's change (0x72f428), an axis's change over the rate (0x7344d8) and the rate without one
/// (0x72e950).
const BUTTON_CHANGE: f32 = 0.2;
const AXIS_CHANGE: f32 = 1.25;
const AXIS_RATE: f32 = 0.04;
/// The filter cells `Tic` selects: the axis cell, the digital button cell.
const AXIS_CELL: u32 = 0;
const BUTTON_CELL: u32 = 3;
/// `DIDFT_ABSAXIS`.
const AXIS_KIND: u32 = 0x02;
/// DirectInput's default axis range, which Wine's joysticks report (`DIPROP_RANGE` 0..65535).
pub const DEFAULT_RANGE: (i32, i32) = (0, 65535);

/// `GUID_SysKeyboard`, as the `.acm` stores it.
pub const KEYBOARD: [u8; 16] =
    [0x61, 0x2b, 0x1d, 0x6f, 0xa0, 0xd5, 0xcf, 0x11, 0xbf, 0xc7, 0x44, 0x45, 0x53, 0x54, 0x00, 0x00];

/// One object of the data format the game set (`DIOBJECTDATAFORMAT`): its `dwType` and offset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Object {
    pub kind: u32,
    pub ofs: u32,
}

impl Object {
    pub fn is_axis(&self) -> bool {
        self.kind & 3 != 0
    }
}

/// `E29AxisEntry`: one device object's value.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    /// +0x00: the object's `dwType`, which a binding names.
    pub kind: u32,
    /// +0x28: the offset in the device state; +0x24: 4 for an axis, 1 otherwise.
    ofs: usize,
    axis: bool,
    /// +0x44, +0x48: the axis range.
    range: (i32, i32),
    /// +0x30, +0x38: the value and the previous poll's.
    value: f32,
    prev: f32,
    /// +0x2c: -1 until the first poll, then 1 or 2 until the second move, then 0.
    moves: i32,
    /// +0x40: the poll's argument, as `Changed` reads it.
    rate: f32,
}

impl Entry {
    /// `E29AxisEntry::E29AxisEntry` (0x4cabf0) and `SetRange` (0x4cac70).
    pub fn new(object: Object, range: (i32, i32)) -> Self {
        Self {
            kind: object.kind,
            ofs: object.ofs as usize,
            axis: object.is_axis(),
            range,
            value: 0.0,
            prev: 0.0,
            moves: -1,
            rate: 0.0,
        }
    }

    /// `E29AxisEntry::Update` (0x4cad30) from a device state.
    fn update(&mut self, state: &[u8], rate: f32) {
        self.prev = self.value;
        self.rate = rate;
        if self.axis {
            let raw = state.get(self.ofs..self.ofs + 4).map_or(0, |b| i32::from_le_bytes(b.try_into().unwrap()));
            let (lo, hi) = self.range;
            self.value = (raw.wrapping_sub(lo) as f32 / hi.wrapping_sub(lo) as f32) * 2.0 - 1.0;
        } else {
            self.value = if state.get(self.ofs).copied().unwrap_or(0) != 0 { 1.0 } else { 0.0 };
        }
        if self.moves < 0 {
            self.moves = 1;
        } else if self.moves != 0 && magnitude(self.value - self.prev) >= MOVE {
            self.moves += 1;
            if self.moves > 2 {
                self.moves = 0;
            }
        }
    }

    /// `E29AxisEntry::Changed` (0x4cacb0).
    fn changed(&self) -> bool {
        let change = magnitude(self.value - self.prev);
        if self.axis {
            let rate = if self.rate > 0.0 { self.rate } else { AXIS_RATE };
            change / rate >= AXIS_CHANGE
        } else {
            change >= BUTTON_CHANGE
        }
    }

    /// Whether the entry has not yet moved twice (`Bound`).
    fn settling(&self) -> bool {
        self.moves != 0
    }

    /// The value (`Raw` 0x4caca0).
    pub fn value(&self) -> f32 {
        self.value
    }
}

/// `|d|` as `Changed` and `Update` take it: `d` when above 0, else `0 - d`.
fn magnitude(d: f32) -> f32 {
    if d > 0.0 {
        d
    } else {
        0.0 - d
    }
}

/// One device the game polls: its DirectInput instance GUID (as the `.acm` stores it) and its entries.
#[derive(Clone, Debug, PartialEq)]
pub struct Device {
    pub guid: [u8; 16],
    pub entries: Vec<Entry>,
}

impl Device {
    /// The device as the game made it from its data format; `range` gives an axis's range by its `dwType`.
    pub fn new(guid: [u8; 16], objects: &[Object], range: impl Fn(u32) -> (i32, i32)) -> Self {
        let entries =
            objects.iter().map(|&o| Entry::new(o, if o.is_axis() { range(o.kind) } else { (0, 0) })).collect();
        Self { guid, entries }
    }

    /// `SInputDeviceImpl::Poll`'s update after a successful `GetDeviceState`; `rate` is `Tic`'s argument.
    pub fn update(&mut self, state: &[u8], rate: f32) {
        for e in &mut self.entries {
            e.update(state, rate);
        }
    }
}

/// The keyboard as the game makes it (`E29EnumObjectsCb`'s keyboard branch, 0x4ca486): an entry per key, its
/// `dwType` the enumerated one (`DIDFT_PSHBUTTON`, the DIK code as the instance) at the key's `c_dfDIKeyboard`
/// offset, the DIK code.
pub fn keyboard() -> Device {
    let objects: Vec<Object> = (0..256).map(|dik| Object { kind: dik << 8 | 0x04, ofs: dik }).collect();
    Device::new(KEYBOARD, &objects, |_| (0, 0))
}

/// `input.ini` `[Settings]`: `InvertSteeringAxis`, `InvertThrottleAxis`, `InvertBrakeAxis`, `InvertCombinedAxis`,
/// `InvertHandbrakeAxis`, `InvertClutchAxis` (`USInputManagerImpl` +0x254..+0x268).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Invert {
    pub steering: bool,
    pub throttle: bool,
    pub brake: bool,
    pub combined: bool,
    pub handbrake: bool,
    pub clutch: bool,
}

/// The input manager's devices and action slots.
#[derive(Clone, Debug, PartialEq)]
pub struct Poll {
    pub devices: Vec<Device>,
    /// Per action: the bound entry (device, entry).
    slots: [Option<(usize, usize)>; ACTIONS],
    pub invert: Invert,
}

impl Poll {
    pub fn new(devices: Vec<Device>) -> Self {
        Self { devices, slots: [None; ACTIONS], invert: Invert::default() }
    }

    /// `E28FileMap::FindRec` and `ApplyMaps`: the first record that names as many devices as there are and only
    /// devices there are binds its actions (each binding: the record's device, the entry with that `dwType`).
    /// Returns the record's index; `None` leaves every action unbound.
    pub fn bind(&mut self, map: &ActionMap) -> Option<usize> {
        self.slots = [None; ACTIONS];
        let find = |guid: &[u8; 16]| self.devices.iter().position(|d| d.guid == *guid);
        let index = map
            .records
            .iter()
            .position(|r| r.devices.len() == self.devices.len() && r.devices.iter().all(|g| find(g).is_some()))?;
        let record = &map.records[index];
        for b in &record.bindings {
            let Some(device) = record.devices.get(b.device as usize).and_then(find) else { continue };
            let entry = self.devices[device].entries.iter().position(|e| e.kind == b.object);
            if let (Some(entry), Some(slot)) = (entry, self.slots.get_mut(b.action as usize)) {
                *slot = Some((device, entry));
            }
        }
        Some(index)
    }

    fn entry(&self, action: usize) -> Option<&Entry> {
        self.slots[action].map(|(d, e)| &self.devices[d].entries[e])
    }

    /// `IsActionBound` (0x4c2710).
    pub fn bound(&self, action: usize) -> bool {
        self.slots[action].is_some()
    }

    /// `CfgReady` (0x4c25a0): bound to an axis.
    fn axis(&self, action: usize) -> bool {
        self.entry(action).is_some_and(|e| e.axis)
    }

    /// `GetActionValue` (0x4c2610).
    pub fn value(&self, action: usize) -> f32 {
        let Some(e) = self.entry(action) else { return 0.0 };
        if e.axis && e.settling() {
            return 0.0;
        }
        let mut v = e.value;
        if e.axis && NEGATED.contains(&action) {
            v *= -1.0;
        }
        if e.axis && HALF_RANGE.contains(&action) {
            v = (v + 1.0) * 0.5;
        }
        v
    }

    /// `IsActionDown` (0x4c25d0): pressed at this poll.
    pub fn down(&self, action: usize) -> bool {
        self.entry(action).is_some_and(|e| e.value > 0.0 && e.changed())
    }

    /// `USInputManagerImpl::Tic` after the device polls: the raw control it emits. `ngp` reads IGNITION as NGP's
    /// level. The asm's order and its f32 stores are kept: the combined axis splits into its negative part (+0x1c)
    /// and positive part (+0x18); BRAKE and THROTTLE replace them when larger, and then choose the cell by their own
    /// binding; the steering axis, else RIGHT - LEFT.
    pub fn tic(&self, ngp: bool) -> Raw {
        let cell = |axis: bool| if axis { AXIS_CELL } else { BUTTON_CELL };
        let one_minus = |v: f32| 1.0 - v;
        let (mut negative, mut positive) = (0.0f32, 0.0f32);
        if self.bound(COMBINED) {
            let mut v = self.value(COMBINED);
            if self.invert.combined {
                v = 0.0 - v;
            }
            if 0.0 > v {
                negative = 0.0 - v;
            } else {
                positive = v;
            }
        }
        // +0x1c, the poll's brake.
        let mut brake_axis = true;
        let brake = self.value(BRAKE);
        if brake > negative {
            negative = brake;
            brake_axis = self.axis(BRAKE);
        }
        if brake_axis && self.invert.brake {
            negative = one_minus(negative);
        }
        // +0x18, the poll's throttle.
        let mut throttle_axis = true;
        let throttle = self.value(THROTTLE);
        if throttle > positive {
            positive = throttle;
            throttle_axis = self.axis(THROTTLE);
        }
        if throttle_axis && self.invert.throttle {
            positive = one_minus(positive);
        }
        let mut handbrake = self.value(HANDBRAKE);
        let handbrake_axis = self.axis(HANDBRAKE);
        if handbrake_axis && self.invert.handbrake {
            handbrake = one_minus(handbrake);
        }
        let mut clutch = self.value(CLUTCH);
        let clutch_axis = self.axis(CLUTCH);
        if clutch_axis && self.invert.clutch {
            clutch = one_minus(clutch);
        }
        let steering_axis = self.bound(STEERING);
        let steering = if steering_axis {
            let v = self.value(STEERING);
            if self.invert.steering {
                0.0 - v
            } else {
                v
            }
        } else {
            self.value(RIGHT) + -self.value(LEFT)
        };
        let ignition = if ngp { self.value(IGNITION) > NGP_IGNITION_PRESS } else { self.down(IGNITION) };
        let flags = u32::from(self.down(GEAR_UP)) | u32::from(self.down(GEAR_DOWN)) << 1;
        let select: [u32; CHANNELS] =
            [cell(steering_axis), cell(throttle_axis), cell(brake_axis), cell(handbrake_axis), cell(clutch_axis)];
        Raw {
            flags,
            aux: u32::from(ignition),
            byte2: 0,
            channels: [steering, positive, negative, handbrake, clutch],
            gear: 0,
            select,
        }
    }

    /// NGP's emit (`0x100ed540`, 7.5.779 the same): with the automatic gearbox off, the direct gear actions'
    /// selection ([`ngp_direct_gears`]); `gear_in_use` is the last sensor data's gear (input manager +0x250).
    pub fn ngp_gears(&self, raw: Raw, gear_in_use: u32) -> Raw {
        let actions: [Option<f32>; NGP_GEAR_ACTIONS] =
            std::array::from_fn(|k| self.bound(REVERSE + k).then(|| self.value(REVERSE + k)));
        let (flags, gear) = ngp_direct_gears(raw.flags, raw.gear, actions, gear_in_use);
        Raw { flags, gear, ..raw }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rbr_formats::saves::action_map::{Binding, Record};

    const WHEEL: [u8; 16] = [1; 16];
    const X: u32 = 0x0100_0002;
    const Z: u32 = 0x0202;
    const BUTTON0: u32 = 0x0200_0004;

    fn poll(bindings: &[(u32, u32)]) -> Poll {
        let objects = [Object { kind: X, ofs: 0 }, Object { kind: Z, ofs: 4 }, Object { kind: BUTTON0, ofs: 8 }];
        let mut p = Poll::new(vec![Device::new(WHEEL, &objects, |_| DEFAULT_RANGE)]);
        let bindings = bindings.iter().map(|&(object, action)| Binding { device: 0, object, action }).collect();
        let map = ActionMap {
            records: vec![
                Record { devices: vec![[2; 16]], bindings: vec![] },
                Record { devices: vec![WHEEL], bindings },
            ],
        };
        assert_eq!(p.bind(&map), Some(1));
        p
    }

    fn state(x: i32, z: i32, button: u8) -> [u8; 12] {
        let mut s = [0; 12];
        s[0..4].copy_from_slice(&x.to_le_bytes());
        s[4..8].copy_from_slice(&z.to_le_bytes());
        s[8] = button;
        s
    }

    #[test]
    fn an_axis_reads_zero_until_it_has_moved_twice() {
        let mut p = poll(&[(X, STEERING as u32)]);
        p.devices[0].update(&state(0, 0, 0), 0.0);
        assert_eq!(p.value(STEERING), 0.0, "first poll");
        p.devices[0].update(&state(10, 0, 0), 0.0);
        assert_eq!(p.value(STEERING), 0.0, "a move below 0.001 does not count");
        p.devices[0].update(&state(1000, 0, 0), 0.0);
        assert_eq!(p.value(STEERING), 0.0, "one move");
        p.devices[0].update(&state(2000, 0, 0), 0.0);
        assert_eq!(p.value(STEERING), (2000.0f32 / 65535.0) * 2.0 - 1.0);
    }

    #[test]
    fn pedals_are_negated_and_halved() {
        let mut p = poll(&[(Z, THROTTLE as u32), (X, STEERING as u32)]);
        for z in [65535, 30000, 0] {
            p.devices[0].update(&state(32767, z, 0), 0.0);
        }
        // Pressed fully (0): 1.0; the steering axis takes its own value.
        assert_eq!(p.tic(true).channels[1], 1.0);
        assert_eq!(p.tic(true).select, [AXIS_CELL, AXIS_CELL, AXIS_CELL, BUTTON_CELL, BUTTON_CELL]);
        p.devices[0].update(&state(32767, 65535, 0), 0.0);
        assert_eq!(p.tic(true).channels[1], 0.0);
    }

    #[test]
    fn a_button_action_is_a_press_and_ignition_a_level_under_ngp() {
        let mut p = poll(&[(BUTTON0, GEAR_UP as u32)]);
        p.devices[0].update(&state(0, 0, 0x80), 0.0);
        assert_eq!(p.tic(false).flags, 1);
        p.devices[0].update(&state(0, 0, 0x80), 0.0);
        assert_eq!(p.tic(false).flags, 0, "held: no new press");
        let mut p = poll(&[(BUTTON0, IGNITION as u32)]);
        p.devices[0].update(&state(0, 0, 0x80), 0.0);
        p.devices[0].update(&state(0, 0, 0x80), 0.0);
        assert_eq!((p.tic(false).aux, p.tic(true).aux), (0, 1));
    }

    #[test]
    fn digital_steering_is_right_minus_left_on_the_button_cell() {
        let mut p = poll(&[(BUTTON0, LEFT as u32)]);
        p.devices[0].update(&state(0, 0, 0x80), 0.0);
        let raw = p.tic(false);
        assert_eq!((raw.channels[0], raw.select[0]), (-1.0, BUTTON_CELL));
    }
}
