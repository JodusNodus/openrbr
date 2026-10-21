//! The joysticks the live drive polls (`crate::bindings::Device`), each giving the device state the game's data format
//! would (`rbr_gameplay::poll::wine::objects`), and the poll over our bindings ([`Input`]).
//!
//! On Linux the devices are read from evdev, as Proton's winebus reads them through SDL's Linux joystick for the game
//! (`rbr_gameplay::poll::wine`): a device's index counts the joystick nodes with its vendor and product ids
//! (ABS_X and ABS_Y, or a joystick button, as SDL and udev take a node for one), in `/dev/input/event*` order.
//! Elsewhere they come from gilrs's platform layer (`gilrs-core`): the axes and buttons in its order, scaled from
//! their range to dinput's 0..65535 (unverified on a wheel).

use rbr_gameplay::control::NGP_GEAR_PRESS;
use rbr_gameplay::poll::{keyboard, Device as PollDevice, Poll, DEFAULT_RANGE};
use rbr_recording::input::CELL_BUTTON;

use crate::bindings::{dik, Action, Bindings, Device, ACTIONS};
use crate::live::Controls;
use rbr_formats::saves::action_map::Object as Target;

/// The joysticks: each opened one, its state as last read, and whether its last read failed.
pub struct Joysticks {
    list: Vec<(Device, imp::Joystick)>,
    /// Per joystick: the read failed (unplugged); the state stays the last one read, as the game keeps a device's
    /// entries when `GetDeviceState` fails, until [`Joysticks::reopen`] finds it again.
    pub lost: Vec<bool>,
    context: imp::Context,
}

impl Joysticks {
    /// The joysticks `want` names, as the game sees them, and why each one is not open.
    pub fn open(want: &[Device]) -> (Joysticks, Vec<String>) {
        let mut context = imp::Context::new();
        let mut found = imp::Joystick::all(&mut context, false);
        let mut missing = Vec::new();
        let list: Vec<_> = want
            .iter()
            .filter_map(|d| match found.iter().position(|(f, _)| f == d) {
                Some(k) => Some(found.swap_remove(k)),
                None => {
                    missing.push(format!("{}: not connected (or not readable)", d.format()));
                    None
                }
            })
            .collect();
        let lost = vec![false; list.len()];
        (Joysticks { list, lost, context }, missing)
    }

    /// Every joystick connected, its axes at their values from the start (not the game's centre until they move):
    /// the launcher's capture and its bars.
    pub fn connected() -> Joysticks {
        let mut context = imp::Context::new();
        let list = imp::Joystick::all(&mut context, true);
        Joysticks { lost: vec![false; list.len()], list, context }
    }

    pub fn len(&self) -> usize {
        self.list.len()
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    pub fn device(&self, k: usize) -> Device {
        self.list[k].0
    }

    pub fn name(&self, k: usize) -> &str {
        self.list[k].1.name()
    }

    /// The axes and buttons the game's data format has for joystick `k`.
    pub fn objects(&self, k: usize) -> (u32, u32) {
        let j = &self.list[k].1;
        (j.axes(), j.buttons())
    }

    /// Reads every joystick; a failed read keeps its state and marks it [`Joysticks::lost`].
    pub fn read(&mut self) {
        self.context.update(&mut self.list);
        for ((_, j), lost) in self.list.iter_mut().zip(&mut self.lost) {
            *lost = !j.read(&self.context);
        }
    }

    /// Joystick `k`'s state as last read.
    pub fn state(&self, k: usize) -> &[u8] {
        self.list[k].1.state()
    }

    /// Opens the lost joysticks again if they are back (fresh, as a device plugged in again is).
    pub fn reopen(&mut self) {
        if !self.lost.contains(&true) {
            return;
        }
        let mut found = imp::Joystick::all(&mut self.context, false);
        for ((device, j), lost) in self.list.iter_mut().zip(&mut self.lost) {
            if let Some(k) = found.iter().position(|(f, _)| f == device).filter(|_| *lost) {
                *j = found.swap_remove(k).1;
                *lost = false;
            }
        }
    }
}

/// The live poll: the keyboard and the bound joysticks, bound as our bindings say, through `Tic`.
pub struct Input {
    poll: Poll,
    joysticks: Joysticks,
    /// Each action's device input: its poll device and entry.
    pads: Vec<(Action, usize, usize)>,
    /// The bound devices not open, why.
    pub missing: Vec<String>,
}

impl Input {
    /// The keyboard and the joysticks the bindings name that are connected.
    pub fn open(bindings: &Bindings) -> Input {
        let (joysticks, missing) = Joysticks::open(&bindings.devices());
        let mut devices = vec![keyboard()];
        for k in 0..joysticks.len() {
            let (axes, buttons) = joysticks.objects(k);
            let objects = rbr_gameplay::poll::wine::objects(axes, buttons);
            devices.push(PollDevice::new([0; 16], &objects, |_| DEFAULT_RANGE));
        }
        let index = |d: Option<Device>| match d {
            None => Some(0),
            Some(d) => (0..joysticks.len()).position(|k| joysticks.device(k) == d).map(|k| k + 1),
        };
        let slots: Vec<_> =
            bindings.slots().into_iter().filter_map(|(slot, d, object)| Some((slot, index(d)?, object))).collect();
        let mut poll = Poll::new(devices);
        poll.bind(&slots);
        poll.invert = bindings.invert;
        let pads = bindings
            .pads()
            .filter_map(|(a, p)| {
                let d = index(Some(p.device))?;
                Some((a, d, poll.devices[d].find(p.object)?))
            })
            .collect();
        Input { poll, joysticks, pads, missing }
    }

    /// The joysticks open, by name.
    pub fn names(&self) -> Vec<String> {
        (0..self.joysticks.len())
            .map(|k| format!("{} ({})", self.joysticks.name(k), self.joysticks.device(k).format()))
            .collect()
    }

    /// One poll, the keyboard's state (`c_dfDIKeyboard`, [`keyboard_state`]) and the joysticks as they are now: the
    /// controls `last` had with the poll's channels, cells, ignition and direct gear actions, and its gear up or down
    /// presses added to the flags; and in `pads` each action's device input now (0..1).
    pub fn controls(&mut self, keys: &[u8; 256], last: Controls, ngp: bool, pads: &mut [f32; ACTIONS]) -> Controls {
        self.joysticks.read();
        self.poll.devices[0].update(keys, 0.0);
        for k in 0..self.joysticks.len() {
            // A lost device keeps its entries: `Poll` skips the update on a failed `GetDeviceState`.
            if !self.joysticks.lost[k] {
                self.poll.devices[k + 1].update(self.joysticks.state(k), 0.0);
            }
        }
        *pads = [0.0; ACTIONS];
        for &(a, d, e) in &self.pads {
            let entry = &self.poll.devices[d].entries[e];
            let v = entry.value();
            pads[a as usize] = if entry.is_axis() { (v + 1.0) * 0.5 } else { v };
        }
        let raw = self.poll.tic(ngp);
        let gears = self.poll.gear_actions();
        let bits = |test: &dyn Fn(Option<f32>) -> bool| (0..8).filter(|&k| test(gears[k])).fold(0, |b, k| b | 1 << k);
        let [steering, throttle, brake, handbrake, clutch] = raw.channels;
        let cells = raw.select.map(|c| c as u8);
        Controls {
            steering,
            throttle,
            brake,
            handbrake,
            clutch,
            cells: Some(cells),
            keys_steer: cells[0] == CELL_BUTTON,
            ignition: raw.aux != 0,
            flags: last.flags | raw.flags,
            gears_held: if ngp { bits(&|v| v.is_some_and(|v| v > NGP_GEAR_PRESS)) } else { 0 },
            gears_bound: if ngp { bits(&|v| v.is_some()) } else { 0 },
            ..last
        }
    }

    /// Opens the lost joysticks again if they are back (the keepalive's).
    pub fn reopen(&mut self) {
        let lost = self.joysticks.lost.clone();
        self.joysticks.reopen();
        // A device opened again is a fresh one to the game: its axes settle again.
        for (k, was) in lost.iter().enumerate() {
            if *was && !self.joysticks.lost[k] {
                let (axes, buttons) = self.joysticks.objects(k);
                let fresh =
                    PollDevice::new([0; 16], &rbr_gameplay::poll::wine::objects(axes, buttons), |_| DEFAULT_RANGE);
                if fresh.entries.len() == self.poll.devices[k + 1].entries.len() {
                    self.poll.devices[k + 1] = fresh;
                }
            }
        }
    }
}

/// The keyboard's state (`c_dfDIKeyboard`): 0x80 at each held key's DIK code.
pub fn keyboard_state<'a>(held: impl IntoIterator<Item = &'a winit::keyboard::KeyCode>, state: &mut [u8; 256]) {
    state.fill(0);
    for dik in held.into_iter().filter_map(|&k| dik(k)) {
        state[usize::from(dik)] = 0x80;
    }
}

/// A move of a joystick's object from where it rested: an axis's value (-1..1) or a button pressed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Moved {
    pub device: Device,
    pub object: Target,
    pub rest: f32,
    pub value: f32,
}

/// The objects' values of a joystick's state, -1..1 per axis (dinput's 0..65535), then 0 or 1 per button.
pub fn values(state: &[u8], axes: u32, buttons: u32) -> impl Iterator<Item = (Target, f32)> + '_ {
    let axis = |k: u32| {
        let at = 4 * k as usize;
        let raw = state.get(at..at + 4).map_or(32768, |b| i32::from_le_bytes(b.try_into().unwrap()));
        (raw as f32 / 65535.0) * 2.0 - 1.0
    };
    let button = move |k: u32| if state.get((4 * axes + k) as usize).is_some_and(|&b| b != 0) { 1.0 } else { 0.0 };
    (0..axes).map(move |k| (Target::Axis(k), axis(k))).chain((0..buttons).map(move |k| (Target::Button(k), button(k))))
}

#[cfg(target_os = "linux")]
mod imp {
    pub use super::evdev::Joystick;

    /// evdev needs no context: each joystick is its file.
    pub struct Context;

    impl Context {
        pub fn new() -> Context {
            Context
        }

        pub fn update(&mut self, _: &mut [(crate::bindings::Device, Joystick)]) {}
    }
}

/// SDL2's Linux joystick (`SDL_sysjoystick.c`, `ConfigJoystick` and `AxisCorrect`) over an evdev device, and
/// winebus's report of it: an axis reads winebus's 0 (dinput 32768) until it first changes, and the whole device
/// dinput's centre until winebus has a report (the T500 RS's untouched axes: 32767, then 32768).
#[cfg(target_os = "linux")]
mod evdev {
    use std::fs::File;
    use std::os::fd::AsRawFd;

    use crate::bindings::Device;
    use rbr_gameplay::poll::wine;

    const ABS_MAX: u32 = 0x3f;
    const KEY_MAX: u32 = 0x2ff;
    const BTN_JOYSTICK: u32 = 0x120;
    const BTN_DIGI: u32 = 0x140;
    const BTN_TRIGGER_HAPPY: u32 = 0x2c0;
    const ABS_X: u32 = 0x00;
    const ABS_Y: u32 = 0x01;
    const ABS_HAT0X: u32 = 0x10;
    const ABS_HAT3Y: u32 = 0x17;
    const EV_KEY: u32 = 1;
    const EV_ABS: u32 = 3;
    const KEY_BYTES: usize = (KEY_MAX as usize + 1) / 8;

    /// `struct input_absinfo`.
    #[repr(C)]
    #[derive(Default, Clone, Copy)]
    struct AbsInfo {
        value: i32,
        minimum: i32,
        maximum: i32,
        fuzz: i32,
        flat: i32,
        resolution: i32,
    }

    /// `_IOC(_IOC_READ, 'E', nr, size)`.
    fn read_request(nr: u32, size: usize) -> libc::c_ulong {
        (2 << 30 | (size as libc::c_ulong) << 16 | (b'E' as libc::c_ulong) << 8 | nr as libc::c_ulong) as _
    }

    fn ioctl<T>(file: &File, nr: u32, out: &mut T) -> bool {
        // SAFETY: the request's size is `T`'s, and `out` is a valid `T` the kernel writes at most that much into.
        unsafe { libc::ioctl(file.as_raw_fd(), read_request(nr, size_of::<T>()) as _, out as *mut T) >= 0 }
    }

    fn bit(bits: &[u8], k: u32) -> bool {
        bits.get((k / 8) as usize).is_some_and(|b| b >> (k % 8) & 1 != 0)
    }

    fn abs(file: &File, code: u32) -> Option<AbsInfo> {
        let mut info = AbsInfo::default();
        ioctl(file, 0x40 + code, &mut info).then_some(info)
    }

    pub struct Joystick {
        file: File,
        name: String,
        /// Per axis: its code, range, value when opened and whether it changed since.
        axes: Vec<(u32, i32, i32, i32, bool)>,
        keys: Vec<u32>,
        reported: bool,
        /// The state as last read ([`wine::objects`]'s layout).
        state: Vec<u8>,
    }

    impl Joystick {
        /// Every joystick node (in `/dev/input/event*` order) that can be opened, with its device id; with `actual`
        /// its axes read their values at once.
        pub fn all(_: &mut super::imp::Context, actual: bool) -> Vec<(Device, Joystick)> {
            let Ok(dir) = std::fs::read_dir("/dev/input") else { return Vec::new() };
            let mut paths: Vec<_> = dir
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("event")))
                .collect();
            paths.sort_by_key(|p| p.to_str().and_then(|n| n.rsplit("event").next()?.parse::<u32>().ok()));
            let mut out: Vec<(Device, Joystick)> = Vec::new();
            for file in paths.iter().filter_map(|p| File::open(p).ok()) {
                let mut id = [0u16; 4];
                if !ioctl(&file, 0x02, &mut id) {
                    continue;
                }
                let (mut absbits, mut keybits) = ([0u8; (ABS_MAX as usize + 1) / 8], [0u8; KEY_BYTES]);
                ioctl(&file, 0x20 + EV_ABS, &mut absbits);
                ioctl(&file, 0x20 + EV_KEY, &mut keybits);
                let buttons = (BTN_JOYSTICK..BTN_DIGI).chain(BTN_TRIGGER_HAPPY..BTN_TRIGGER_HAPPY + 40);
                let joystick =
                    bit(&absbits, ABS_X) && bit(&absbits, ABS_Y) || buttons.into_iter().any(|k| bit(&keybits, k));
                if !joystick {
                    continue;
                }
                let index = out.iter().filter(|(d, _)| (d.vendor, d.product) == (id[1], id[2])).count() as u32;
                let joystick = Joystick::new(file, &absbits, &keybits, actual);
                out.push((Device { vendor: id[1], product: id[2], index }, joystick));
            }
            out
        }

        fn new(file: File, absbits: &[u8], keybits: &[u8], actual: bool) -> Joystick {
            let mut name = [0u8; 256];
            ioctl(&file, 0x06, &mut name);
            let name = String::from_utf8_lossy(name.split(|&b| b == 0).next().unwrap_or(&[])).into_owned();
            let keys: Vec<u32> = (BTN_JOYSTICK..KEY_MAX).chain(0..BTN_JOYSTICK).filter(|&k| bit(keybits, k)).collect();
            // A pair of hat codes SDL takes for a digital hat (`GuessIfAxesAreDigitalHat`) is not an axis.
            let hat = |pair: u32| {
                let info = |k: u32| bit(absbits, k).then(|| abs(&file, k)).flatten();
                let (x, y) = (info(pair), info(pair + 1));
                let digital = |i: Option<AbsInfo>| i.is_none_or(|i| i.minimum == -1 && i.maximum == 1);
                let quiet = |i: Option<AbsInfo>| i.is_none_or(|i| i.fuzz == 0 && i.flat == 0 && i.resolution == 0);
                (x.is_some() || y.is_some()) && (digital(x) && digital(y) || quiet(x) && quiet(y))
            };
            let axes: Vec<_> = (0..ABS_MAX)
                .filter(|&k| bit(absbits, k))
                .filter(|&k| !((ABS_HAT0X..=ABS_HAT3Y).contains(&k) && hat(ABS_HAT0X + (k - ABS_HAT0X) / 2 * 2)))
                .filter_map(|k| abs(&file, k).map(|i| (k, i.minimum, i.maximum, i.value, actual)))
                .collect();
            let state = wine::state(&vec![CENTRE; axes.len()], &vec![false; keys.len()]);
            let mut joystick = Joystick { file, name, axes, keys, reported: false, state };
            // SDL sends the buttons held when it opens the device; the axes not.
            let mut keybits = [0u8; KEY_BYTES];
            joystick.reported =
                actual || ioctl(&joystick.file, 0x18, &mut keybits) && joystick.keys.iter().any(|&k| bit(&keybits, k));
            joystick
        }

        pub fn name(&self) -> &str {
            &self.name
        }

        pub fn axes(&self) -> u32 {
            self.axes.len() as u32
        }

        pub fn buttons(&self) -> u32 {
            self.keys.len() as u32
        }

        pub fn state(&self) -> &[u8] {
            &self.state
        }

        /// Reads the state the game's data format gives now ([`wine::objects`]); `false`, the state kept, when the
        /// device does not answer (unplugged).
        pub fn read(&mut self, _: &super::imp::Context) -> bool {
            let mut keybits = [0u8; KEY_BYTES];
            if !ioctl(&self.file, 0x18, &mut keybits) {
                return false;
            }
            let mut values = [0i32; ABS_MAX as usize];
            for (k, (code, ..)) in self.axes.iter().enumerate() {
                match abs(&self.file, *code) {
                    Some(info) => values[k] = info.value,
                    None => return false,
                }
            }
            let axes = self.axes.len();
            for (k, (_, min, max, opened, moved)) in self.axes.iter_mut().enumerate() {
                *moved |= values[k] != *opened;
                self.reported |= *moved;
                let v = if *moved { wine::axis(wine::sdl_axis(values[k], *min, *max)) } else { wine::axis(0) };
                self.state[4 * k..4 * k + 4].copy_from_slice(&v.to_le_bytes());
            }
            for (k, &code) in self.keys.iter().enumerate() {
                let down = bit(&keybits, code);
                self.reported |= down;
                self.state[4 * axes + k] = if down { 0x80 } else { 0 };
            }
            if !self.reported {
                for k in 0..axes {
                    self.state[4 * k..4 * k + 4].copy_from_slice(&CENTRE.to_le_bytes());
                }
            }
            true
        }
    }

    /// dinput's axis value before the device's first report: the range's centre.
    const CENTRE: i32 = 32767;
}

#[cfg(not(target_os = "linux"))]
mod imp {
    use crate::bindings::Device;
    use gilrs_core::{EventType, Gilrs};

    /// gilrs's platform layer, which every pad reads its events from.
    pub struct Context(Option<Gilrs>);

    impl Context {
        pub fn new() -> Context {
            Context(Gilrs::new().map_err(|e| eprintln!("devices: no gamepads: {e}")).ok())
        }

        /// The events since the last update, into the pads' values.
        pub fn update(&mut self, list: &mut [(Device, Joystick)]) {
            let Some(gilrs) = &mut self.0 else { return };
            while let Some(event) = gilrs.next_event() {
                let Some((_, pad)) = list.iter_mut().find(|(_, j)| j.id == event.id) else { continue };
                match event.event {
                    EventType::ButtonPressed(code) | EventType::ButtonReleased(code) => {
                        if let Some(k) = pad.buttons.iter().position(|&c| c == code) {
                            pad.state[4 * pad.ranges.len() + k] =
                                if matches!(event.event, EventType::ButtonPressed(_)) { 0x80 } else { 0 };
                        }
                    }
                    EventType::AxisValueChanged(value, code) => {
                        if let Some(k) = pad.axes.iter().position(|&c| c == code) {
                            let (min, max) = pad.ranges[k];
                            let v = if max > min {
                                ((i64::from(value.clamp(min, max)) - i64::from(min)) * 65535 / i64::from(max - min))
                                    as i32
                            } else {
                                32768
                            };
                            pad.state[4 * k..4 * k + 4].copy_from_slice(&v.to_le_bytes());
                        }
                    }
                    EventType::Disconnected => pad.connected = false,
                    _ => {}
                }
            }
        }
    }

    pub struct Joystick {
        id: usize,
        name: String,
        axes: Vec<gilrs_core::EvCode>,
        ranges: Vec<(i32, i32)>,
        buttons: Vec<gilrs_core::EvCode>,
        connected: bool,
        state: Vec<u8>,
    }

    impl Joystick {
        /// The connected gamepads with their ids (an index per vendor and product, in gilrs's order). An axis reads
        /// the centre until its first event, `actual` or not.
        pub fn all(context: &mut Context, _actual: bool) -> Vec<(Device, Joystick)> {
            let Some(gilrs) = &context.0 else { return Vec::new() };
            let mut out: Vec<(Device, Joystick)> = Vec::new();
            for id in 0..gilrs.last_gamepad_hint() {
                let Some(pad) = gilrs.gamepad(id).filter(|p| p.is_connected()) else { continue };
                let (vendor, product) = (pad.vendor_id().unwrap_or(0), pad.product_id().unwrap_or(0));
                let index = out.iter().filter(|(d, _)| (d.vendor, d.product) == (vendor, product)).count() as u32;
                let axes = pad.axes().to_vec();
                let ranges = axes.iter().map(|&c| pad.axis_info(c).map_or((0, 0), |i| (i.min, i.max))).collect();
                let buttons = pad.buttons().to_vec();
                let mut state = vec![0u8; 4 * axes.len() + buttons.len()];
                for k in 0..axes.len() {
                    state[4 * k..4 * k + 4].copy_from_slice(&32768i32.to_le_bytes());
                }
                let joystick =
                    Joystick { id, name: pad.name().to_owned(), axes, ranges, buttons, connected: true, state };
                out.push((Device { vendor, product, index }, joystick));
            }
            out
        }

        pub fn name(&self) -> &str {
            &self.name
        }

        pub fn axes(&self) -> u32 {
            self.axes.len() as u32
        }

        pub fn buttons(&self) -> u32 {
            self.buttons.len() as u32
        }

        pub fn state(&self) -> &[u8] {
            &self.state
        }

        pub fn read(&mut self, _: &Context) -> bool {
            self.connected
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_keyboard_s_arrows() {
        let mut state = [0xff; 256];
        keyboard_state(&[winit::keyboard::KeyCode::ArrowLeft, winit::keyboard::KeyCode::KeyW], &mut state);
        assert_eq!((state[0xcb], state[0x11], state[0xcd]), (0x80, 0x80, 0));
    }

    #[test]
    fn a_state_s_values() {
        let state = rbr_gameplay::poll::wine::state(&[0, 65535], &[false, true]);
        let v: Vec<_> = values(&state, 2, 2).collect();
        assert_eq!(
            v,
            [(Target::Axis(0), -1.0), (Target::Axis(1), 1.0), (Target::Button(0), 0.0), (Target::Button(1), 1.0)]
        );
    }
}
