//! The live drive's input through an RSF profile's action map, as RSF's game polls it (`rbr_gameplay::poll`): the
//! devices as Wine's DirectInput gives them to the game under Proton (`rbr_gameplay::poll::wine`), the keyboard from
//! the held keys, the `.acm`'s bindings, then `USInputManagerImpl::Tic` with NGP's ignition. The launcher imports the
//! map ([`RsfMap::import`]); the viewer reads the devices each frame ([`RsfInput::controls`]). NGP's direct gears
//! stay with `rbr-standalone`, which knows the gear in use.
//!
//! The devices are read from evdev (Linux only), as SDL's Linux joystick reads them for winebus: each frame takes the
//! values at that moment, where NGP polls at 1000 Hz.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use rbr_formats::saves::action_map::ActionMap;
use rbr_gameplay::control::NGP_GEAR_PRESS;
use rbr_gameplay::poll::wine::{self, Instance};
use rbr_gameplay::poll::{keyboard, Device, Poll};
use rbr_recording::input::CELL_BUTTON;
use winit::keyboard::KeyCode;

use crate::live::Controls;

/// An imported RSF action map: the profile's `.acm` and the devices its GUIDs name.
#[derive(Debug, Clone, PartialEq)]
pub struct RsfMap {
    pub acm: PathBuf,
    pub devices: Vec<Instance>,
}

impl RsfMap {
    /// The map at `acm` (`<RSF>/SavedGames/pf<name>.acm`) with its devices' ids from the Wine prefix's `user.reg`.
    pub fn import(acm: &Path, user_reg: &Path) -> Result<RsfMap, String> {
        let map = read_map(acm)?;
        let reg = std::fs::read_to_string(user_reg).map_err(|e| format!("{}: {e}", user_reg.display()))?;
        let named: HashSet<[u8; 16]> = map.records.iter().flat_map(|r| r.devices.iter().copied()).collect();
        let devices: Vec<Instance> = wine::instances(&reg).into_iter().filter(|i| named.contains(&i.guid)).collect();
        if devices.is_empty() {
            return Err(format!("{}: none of the profile's devices", user_reg.display()));
        }
        Ok(RsfMap { acm: acm.to_owned(), devices })
    }

    /// The ini's `rsf action map` and `rsf devices` values.
    pub fn parse(acm: &str, devices: &str) -> Option<RsfMap> {
        let devices = devices.split(',').map(str::trim).filter(|d| !d.is_empty()).map(parse_instance);
        Some(RsfMap { acm: PathBuf::from(acm), devices: devices.collect::<Option<_>>()? })
    }

    /// The `rsf devices` value: `<instance GUID> <vendor>:<product> <index>`, comma separated.
    pub fn devices_text(&self) -> String {
        let text = |i: &Instance| format!("{} {:04x}:{:04x} {}", guid_text(&i.guid), i.vendor, i.product, i.index);
        self.devices.iter().map(text).collect::<Vec<_>>().join(", ")
    }
}

fn read_map(acm: &Path) -> Result<ActionMap, String> {
    let bytes = std::fs::read(acm).map_err(|e| format!("{}: {e}", acm.display()))?;
    ActionMap::read(&bytes).map_err(|e| format!("{}: {e:?}", acm.display()))
}

fn parse_instance(text: &str) -> Option<Instance> {
    let mut parts = text.split_whitespace();
    let guid = parse_guid(parts.next()?)?;
    let (vendor, product) = parts.next()?.split_once(':')?;
    Some(Instance {
        guid,
        vendor: u16::from_str_radix(vendor, 16).ok()?,
        product: u16::from_str_radix(product, 16).ok()?,
        index: parts.next()?.parse().ok()?,
    })
}

/// A GUID as text, its first three fields little-endian as stored.
fn guid_text(g: &[u8; 16]) -> String {
    let hex = |b: &[u8]| b.iter().map(|b| format!("{b:02x}")).collect::<String>();
    let le = |b: &[u8]| hex(&b.iter().rev().copied().collect::<Vec<_>>());
    format!("{}-{}-{}-{}-{}", le(&g[0..4]), le(&g[4..6]), le(&g[6..8]), hex(&g[8..10]), hex(&g[10..16]))
}

fn parse_guid(text: &str) -> Option<[u8; 16]> {
    let fields: Vec<&str> = text.split('-').collect();
    let [a, b, c, d, e] = fields[..] else { return None };
    let bytes = |s: &str, reverse: bool| {
        let mut v: Vec<u8> =
            (0..s.len() / 2).map(|k| u8::from_str_radix(s.get(2 * k..2 * k + 2)?, 16).ok()).collect::<Option<_>>()?;
        if reverse {
            v.reverse();
        }
        Some(v)
    };
    let all = [bytes(a, true)?, bytes(b, true)?, bytes(c, true)?, bytes(d, false)?, bytes(e, false)?].concat();
    all.try_into().ok()
}

/// Where a poll device's state comes from.
enum Source {
    Keyboard,
    #[cfg(unix)]
    Joystick(evdev::Joystick),
}

/// The live poll: the game's devices for the imported map and the record it bound.
pub struct RsfInput {
    poll: Poll,
    sources: Vec<Source>,
    pub record: usize,
    /// The map's devices not connected (or not readable), why.
    pub missing: Vec<String>,
}

impl RsfInput {
    /// The keyboard and each of the map's devices that is connected, bound as the game binds them; an error when no
    /// record of the map fits them.
    pub fn open(map: &RsfMap) -> Result<RsfInput, String> {
        let acm = wine::without_flags(&read_map(&map.acm)?);
        let mut devices = vec![keyboard()];
        let mut sources = vec![Source::Keyboard];
        let mut missing = Vec::new();
        for i in &map.devices {
            match open_joystick(i) {
                Ok((device, source)) => {
                    devices.push(device);
                    sources.push(source);
                }
                Err(e) => missing.push(e),
            }
        }
        let mut poll = Poll::new(devices);
        let Some(record) = poll.bind(&acm) else {
            return Err(format!("{}: no record fits the devices here ({})", map.acm.display(), missing.join("; ")));
        };
        Ok(RsfInput { poll, sources, record, missing })
    }

    /// One poll, the devices as they are now and `held` the keys held: the controls `last` had with the poll's
    /// channels, cells, ignition and direct gear actions, and its gear up or down presses added to the flags.
    pub fn controls(&mut self, held: &HashSet<KeyCode>, last: Controls) -> Controls {
        for (device, source) in self.poll.devices.iter_mut().zip(&mut self.sources) {
            let state = match source {
                Source::Keyboard => keyboard_state(held),
                #[cfg(unix)]
                Source::Joystick(j) => j.state(),
            };
            device.update(&state, 0.0);
        }
        let raw = self.poll.tic(true);
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
            gears_held: bits(&|v| v.is_some_and(|v| v > NGP_GEAR_PRESS)),
            gears_bound: bits(&|v| v.is_some()),
            ..last
        }
    }
}

/// The keyboard's state (`c_dfDIKeyboard`): 0x80 at each held key's DIK code.
fn keyboard_state(held: &HashSet<KeyCode>) -> Vec<u8> {
    let mut state = vec![0; 256];
    for dik in held.iter().filter_map(|&k| dik(k)) {
        state[usize::from(dik)] = 0x80;
    }
    state
}

#[cfg(unix)]
fn open_joystick(i: &Instance) -> Result<(Device, Source), String> {
    if !cfg!(target_os = "linux") {
        return Err(format!("{:04x}:{:04x}: devices are read from evdev, on Linux only", i.vendor, i.product));
    }
    let j = evdev::Joystick::open(i.vendor, i.product, i.index)?;
    let device = Device::new(i.guid, &wine::objects(j.axes(), j.buttons()), |_| rbr_gameplay::poll::DEFAULT_RANGE);
    Ok((device, Source::Joystick(j)))
}

#[cfg(not(unix))]
fn open_joystick(i: &Instance) -> Result<(Device, Source), String> {
    Err(format!("{:04x}:{:04x}: devices are read from evdev, on Linux only", i.vendor, i.product))
}

/// DirectInput's key code (DIK, the set 1 scan code; 0x80 and up the extended keys) for a key.
fn dik(key: KeyCode) -> Option<u8> {
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

/// SDL2's Linux joystick (`SDL_sysjoystick.c`, `ConfigJoystick` and `AxisCorrect`) over an evdev device, and
/// winebus's report of it: an axis reads winebus's 0 (dinput 32768) until it first changes, and the whole device
/// dinput's centre until winebus has a report (the T500 RS's untouched axes: 32767, then 32768).
#[cfg(unix)]
mod evdev {
    use std::fs::File;
    use std::os::fd::AsRawFd;

    use rbr_gameplay::poll::wine;

    const ABS_MAX: u32 = 0x3f;
    const KEY_MAX: u32 = 0x2ff;
    const BTN_JOYSTICK: u32 = 0x120;
    const ABS_HAT0X: u32 = 0x10;
    const ABS_HAT3Y: u32 = 0x17;
    const EV_KEY: u32 = 1;
    const EV_ABS: u32 = 3;

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
        /// Per axis: its code, range, value when opened and whether it changed since.
        axes: Vec<(u32, i32, i32, i32, bool)>,
        keys: Vec<u32>,
        reported: bool,
    }

    impl Joystick {
        /// The `index`-th evdev device (in `/dev/input/event*` order) with the vendor and product ids, opened read-only.
        pub fn open(vendor: u16, product: u16, index: u32) -> Result<Joystick, String> {
            let mut paths: Vec<_> = std::fs::read_dir("/dev/input")
                .map_err(|e| format!("/dev/input: {e}"))?
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("event")))
                .collect();
            paths.sort_by_key(|p| p.to_str().and_then(|n| n.rsplit("event").next()?.parse::<u32>().ok()));
            let ids = |file: &File| {
                let mut id = [0u16; 4];
                ioctl(file, 0x02, &mut id).then_some((id[1], id[2]))
            };
            let file = paths
                .iter()
                .filter_map(|p| File::open(p).ok())
                .filter(|f| ids(f) == Some((vendor, product)))
                .nth(index as usize)
                .ok_or_else(|| format!("{vendor:04x}:{product:04x}: not connected (or /dev/input not readable)"))?;
            Ok(Joystick::new(file))
        }

        fn new(file: File) -> Joystick {
            let (mut absbits, mut keybits) = ([0u8; (ABS_MAX as usize + 1) / 8], [0u8; (KEY_MAX as usize + 1) / 8]);
            ioctl(&file, 0x20 + EV_ABS, &mut absbits);
            ioctl(&file, 0x20 + EV_KEY, &mut keybits);
            let keys = (BTN_JOYSTICK..KEY_MAX).chain(0..BTN_JOYSTICK).filter(|&k| bit(&keybits, k)).collect();
            // A pair of hat codes SDL takes for a digital hat (`GuessIfAxesAreDigitalHat`) is not an axis.
            let hat = |pair: u32| {
                let info = |k: u32| bit(&absbits, k).then(|| abs(&file, k)).flatten();
                let (x, y) = (info(pair), info(pair + 1));
                let digital = |i: Option<AbsInfo>| i.is_none_or(|i| i.minimum == -1 && i.maximum == 1);
                let quiet = |i: Option<AbsInfo>| i.is_none_or(|i| i.fuzz == 0 && i.flat == 0 && i.resolution == 0);
                (x.is_some() || y.is_some()) && (digital(x) && digital(y) || quiet(x) && quiet(y))
            };
            let axes = (0..ABS_MAX)
                .filter(|&k| bit(&absbits, k))
                .filter(|&k| !((ABS_HAT0X..=ABS_HAT3Y).contains(&k) && hat(ABS_HAT0X + (k - ABS_HAT0X) / 2 * 2)))
                .filter_map(|k| abs(&file, k).map(|i| (k, i.minimum, i.maximum, i.value, false)))
                .collect();
            let mut joystick = Joystick { file, axes, keys, reported: false };
            // SDL sends the buttons held when it opens the device; the axes not.
            joystick.reported = joystick.buttons_down().contains(&true);
            joystick
        }

        pub fn axes(&self) -> u32 {
            self.axes.len() as u32
        }

        pub fn buttons(&self) -> u32 {
            self.keys.len() as u32
        }

        fn buttons_down(&self) -> Vec<bool> {
            let mut keybits = [0u8; (KEY_MAX as usize + 1) / 8];
            ioctl(&self.file, 0x18, &mut keybits);
            self.keys.iter().map(|&k| bit(&keybits, k)).collect()
        }

        /// The state the game's data format gives now ([`wine::objects`]).
        pub fn state(&mut self) -> Vec<u8> {
            let mut values = Vec::with_capacity(self.axes.len());
            for (code, min, max, opened, moved) in &mut self.axes {
                let value = abs(&self.file, *code).map_or(*opened, |i| i.value);
                *moved |= value != *opened;
                self.reported |= *moved;
                values.push(if *moved { wine::axis(wine::sdl_axis(value, *min, *max)) } else { wine::axis(0) });
            }
            let buttons = self.buttons_down();
            self.reported |= buttons.contains(&true);
            if !self.reported {
                values.iter_mut().for_each(|v| *v = CENTRE);
            }
            wine::state(&values, &buttons)
        }
    }

    /// dinput's axis value before the device's first report: the range's centre.
    const CENTRE: i32 = 32767;
}

#[cfg(test)]
mod tests {
    use super::*;
    use rbr_gameplay::poll::KEYBOARD;

    #[test]
    fn devices_round_trip() {
        let guid = [0x90, 0x6c, 0x30, 0x9b, 0x19, 0xc4, 0xf1, 0x11, 0x80, 0x03, 0, 0, 0x44, 0x45, 0x53, 0x54];
        assert_eq!(guid_text(&guid), "9b306c90-c419-11f1-8003-000044455354");
        let map = RsfMap {
            acm: PathBuf::from("/rsf/SavedGames/pfX.acm"),
            devices: vec![Instance { guid, vendor: 0x046d, product: 0xc266, index: 0 }],
        };
        assert_eq!(map.devices_text(), "9b306c90-c419-11f1-8003-000044455354 046d:c266 0");
        assert_eq!(RsfMap::parse("/rsf/SavedGames/pfX.acm", &map.devices_text()), Some(map));
        assert_eq!(guid_text(&KEYBOARD), "6f1d2b61-d5a0-11cf-bfc7-444553540000");
    }

    #[test]
    fn the_keyboard_s_arrows() {
        let state = keyboard_state(&HashSet::from([KeyCode::ArrowLeft, KeyCode::KeyW]));
        assert_eq!((state[0xcb], state[0x11], state[0xcd]), (0x80, 0x80, 0));
    }
}
