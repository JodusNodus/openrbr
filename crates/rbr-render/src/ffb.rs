//! The steering wheel's force feedback in live mode, through SDL3's haptic API (`notes/ngp.md` "Force feedback"): one
//! constant-force effect on the wheel's steering axis, its level the latest magnitude (`rbr_gameplay::ffb`). A thread
//! owns the device and, at `rate` Hz as NGP's FFB thread does (`ffbRate`, 1000 by default), updates and restarts the
//! effect with the latest level whether or not it changed, which is what RSF's `SetParameters(DIEP_START)` gives at
//! the wheel's evdev device under Proton (`harness/ffb_trace.sh`); under NGP it applies the dead zone there against the
//! latest steering. SDL's open sets the device's gain to 100 % and its autocentre off, as Proton's SDL bus does.
//!
//! A positive magnitude turns the wheel left (SDL's level on `SDL_HAPTIC_STEERING_AXIS`, measured on a Logitech
//! G923 under Linux): the force opposes the steering, so it centres the wheel. `inverse` flips it for a wheel that
//! turns the other way.

use std::ffi::CStr;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, Ordering};
use std::sync::{mpsc, Arc};
use std::thread::JoinHandle;
use std::time::Duration;

use rbr_gameplay::ffb::{self, Settings, MAX_MAGNITUDE};
use sdl3_sys::error::SDL_GetError;
use sdl3_sys::haptic::*;
use sdl3_sys::init::{SDL_InitSubSystem, SDL_QuitSubSystem, SDL_INIT_HAPTIC, SDL_INIT_JOYSTICK};

/// The surfaces of [`Options::sensitivity`], as its keys name them.
pub const SURFACES: [&str; 3] = ["gravel", "tarmac", "snow"];

/// How the wheel is driven.
#[derive(Debug, Clone, PartialEq)]
pub struct Options {
    pub settings: Settings,
    /// How often the thread resends the level, Hz (NGP's `ffbRate`, capped at 1000).
    pub rate: u32,
    /// A part of the device's name; `None` takes the first device with a constant force.
    pub device: Option<String>,
    /// NGP's force-feedback sensitivity for gravel, tarmac and snow, percent (`ForceFeedbackSensitivity<Surface>`;
    /// `rbr-standalone --ffb-sensitivity`): below 100 the force is stronger and its peaks flatter, above 100 lighter.
    pub sensitivity: [i32; 3],
    /// NGP's sensitivities for one RSF car, by its id, in place of [`Options::sensitivity`] (RSF's per-car ones).
    pub car_sensitivity: Vec<(u32, [i32; 3])>,
    /// The wheel's lock-to-lock rotation, degrees (`crate::wheel_rotation`); `None` leaves the wheel's own.
    pub rotation: Option<u16>,
    /// The rotation is the car's own lock when it has one (an RSF car's `cars_data.json`), else [`Options::rotation`].
    pub rotation_per_car: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            settings: Settings::default(),
            rate: 1000,
            device: None,
            sensitivity: [100; 3],
            car_sensitivity: Vec::new(),
            rotation: None,
            rotation_per_car: false,
        }
    }
}

impl Options {
    /// The `[ffb]` section of `text` (the launcher's settings file); the defaults for whatever it leaves out.
    pub fn parse(text: &str) -> Options {
        let mut out = Options::default();
        let sections = rbr_formats::ini::parse(text).unwrap_or_default();
        let Some(section) = sections.iter().find(|s| s.name == "ffb") else {
            return out;
        };
        let flag = |key: &str, default: bool| section.get(key).map_or(default, |v| v == "1" || v == "true");
        let number = |key: &str, default: f32| section.get(key).and_then(|v| v.parse().ok()).unwrap_or(default);
        let s = &mut out.settings;
        s.enabled = flag("enabled", s.enabled);
        s.strength = number("strength", s.strength).clamp(0.0, 1.0);
        s.inverse = flag("inverse", s.inverse);
        s.dead_zone = number("dead_zone", s.dead_zone).max(0.0);
        s.dead_zone_threshold = number("dead_zone_threshold", s.dead_zone_threshold).max(0.0);
        out.rate = section.get("rate").and_then(|v| v.parse().ok()).unwrap_or(out.rate).clamp(1, 1000);
        out.device = section.get("device").filter(|v| !v.is_empty()).map(str::to_owned);
        for (value, surface) in out.sensitivity.iter_mut().zip(SURFACES) {
            *value = section.get(&format!("sensitivity_{surface}")).and_then(|v| v.parse().ok()).unwrap_or(100);
        }
        out.rotation =
            section.get("rotation").and_then(|v| v.parse().ok()).filter(|v| crate::wheel_rotation::DEGREES.contains(v));
        out.rotation_per_car = flag("rotation_per_car", false);
        for (key, value) in &section.entries {
            let Some(car) = key.strip_prefix("sensitivity_car").and_then(|c| c.parse().ok()) else { continue };
            let values: Vec<i32> = value.split(',').filter_map(|v| v.trim().parse().ok()).collect();
            if let Ok(values) = values.try_into() {
                out.car_sensitivity.push((car, values));
            }
        }
        out
    }

    /// The sensitivities a drive of RSF car `car` (`None` for another car) takes.
    pub fn sensitivity_for(&self, car: Option<u32>) -> [i32; 3] {
        let own = self.car_sensitivity.iter().find(|(c, _)| Some(*c) == car);
        own.map_or(self.sensitivity, |(_, s)| *s)
    }

    /// The rotation a drive takes, `car` the car's own lock if it has one.
    pub fn rotation_for(&self, car: Option<u16>) -> Option<u16> {
        car.filter(|d| self.rotation_per_car && crate::wheel_rotation::DEGREES.contains(d)).or(self.rotation)
    }

    pub fn to_text(&self) -> String {
        let s = &self.settings;
        format!(
            "[ffb]\nenabled = {}\nstrength = {}\ninverse = {}\ndead_zone = {}\n\
             dead_zone_threshold = {}\nrate = {}\ndevice = {}\nsensitivity_gravel = {}\nsensitivity_tarmac = {}\n\
             sensitivity_snow = {}\nrotation = {}\nrotation_per_car = {}\n{}",
            u8::from(s.enabled),
            s.strength,
            u8::from(s.inverse),
            s.dead_zone,
            s.dead_zone_threshold,
            self.rate,
            self.device.as_deref().unwrap_or(""),
            self.sensitivity[0],
            self.sensitivity[1],
            self.sensitivity[2],
            self.rotation.map_or(String::new(), |d| d.to_string()),
            u8::from(self.rotation_per_car),
            self.car_sensitivity
                .iter()
                .map(|(c, [g, t, n])| format!("sensitivity_car{c} = {g},{t},{n}\n"))
                .collect::<String>(),
        )
    }
}

/// The wheel's force-feedback thread; dropping it stops the force.
pub struct Wheel {
    shared: Arc<Shared>,
    settings: Settings,
    thread: Option<JoinHandle<()>>,
    /// The device's name.
    pub name: String,
}

#[derive(Default)]
struct Shared {
    magnitude: AtomicI32,
    /// The steering input's bits, for NGP's dead zone.
    steering: AtomicU32,
    ngp: AtomicBool,
    stop: AtomicBool,
}

impl Wheel {
    /// Opens the device and starts its thread, or says why it cannot.
    pub fn open(options: Options) -> Result<Wheel, String> {
        let shared = Arc::new(Shared::default());
        let (tx, rx) = mpsc::channel();
        let thread = {
            let (shared, options) = (shared.clone(), options.clone());
            std::thread::Builder::new()
                .name("ffb".into())
                .spawn(move || run(&shared, &options, &tx))
                .map_err(|e| e.to_string())?
        };
        match rx.recv() {
            Ok(Ok(name)) => Ok(Wheel { shared, settings: options.settings, thread: Some(thread), name }),
            Ok(Err(e)) => {
                let _ = thread.join();
                Err(e)
            }
            Err(_) => Err("the force-feedback thread ended".into()),
        }
    }

    /// The force for the latest step (`live::Hud::force`) and the steering input now (-1 to 1).
    pub fn set(&self, force: f32, ngp: bool, steering: f32) {
        self.set_magnitude(ffb::magnitude(force, &self.settings, ngp), ngp, steering);
    }

    /// A magnitude (-10000 to 10000) as the game passes it to DirectInput, and the steering input now (public for
    /// `examples/ffb_play.rs`, which plays recorded magnitudes).
    pub fn set_magnitude(&self, magnitude: i32, ngp: bool, steering: f32) {
        self.shared.magnitude.store(magnitude, Ordering::Relaxed);
        self.shared.ngp.store(ngp, Ordering::Relaxed);
        self.shared.steering.store(steering.to_bits(), Ordering::Relaxed);
    }
}

impl Drop for Wheel {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn sdl_error() -> String {
    // SAFETY: SDL_GetError returns a valid C string owned by SDL.
    unsafe { CStr::from_ptr(SDL_GetError()).to_string_lossy().into_owned() }
}

/// The evdev level Proton gives a DirectInput magnitude (-10000 to 10000): dinput's `scale_to_logical_value`
/// (`dlls/dinput/joystick_hid.c`) onto winebus's PID magnitude, logical -32768 to 32767 for physical -10000 to 10000
/// (`dlls/winebus.sys/hid.c`), passed on at the effect's 100 % gain (`bus_sdl.c`). 0 gives -1.
fn level(magnitude: i32) -> i16 {
    let m = magnitude.clamp(-MAX_MAGNITUDE, MAX_MAGNITUDE);
    (-32768 + (m + MAX_MAGNITUDE) * 65535 / (2 * MAX_MAGNITUDE)) as i16
}

/// The thread: opens the device, reports its name or the error on `ready`, then updates the effect until stopped.
fn run(shared: &Shared, options: &Options, ready: &mpsc::Sender<Result<String, String>>) {
    // SAFETY: the SDL calls below run on this thread only, on the haptic handle it opened and closes.
    unsafe {
        if !SDL_InitSubSystem(SDL_INIT_HAPTIC | SDL_INIT_JOYSTICK) {
            let _ = ready.send(Err(format!("SDL haptic: {}", sdl_error())));
            return;
        }
        let device = open_device(options.device.as_deref());
        let (haptic, name) = match device {
            Ok(found) => found,
            Err(e) => {
                let _ = ready.send(Err(e));
                SDL_QuitSubSystem(SDL_INIT_HAPTIC | SDL_INIT_JOYSTICK);
                return;
            }
        };
        let mut effect: SDL_HapticEffect = std::mem::zeroed();
        effect.constant.r#type = SDL_HAPTIC_CONSTANT;
        effect.constant.direction.r#type = SDL_HAPTIC_STEERING_AXIS;
        effect.constant.length = SDL_HAPTIC_INFINITY;
        let id = SDL_CreateHapticEffect(haptic, &effect);
        if id.0 < 0 || !SDL_RunHapticEffect(haptic, id, 1) {
            let _ = ready.send(Err(format!("{name}: the constant force: {}", sdl_error())));
            SDL_CloseHaptic(haptic);
            SDL_QuitSubSystem(SDL_INIT_HAPTIC | SDL_INIT_JOYSTICK);
            return;
        }
        let _ = ready.send(Ok(name));
        let period = Duration::from_secs_f64(1.0 / f64::from(options.rate.clamp(1, 1000)));
        while !shared.stop.load(Ordering::Relaxed) {
            let mut m = shared.magnitude.load(Ordering::Relaxed);
            if shared.ngp.load(Ordering::Relaxed) {
                let steering = f32::from_bits(shared.steering.load(Ordering::Relaxed));
                m = ffb::dead_zone(m, steering, &options.settings);
            }
            effect.constant.level = level(m);
            SDL_UpdateHapticEffect(haptic, id, &effect);
            SDL_RunHapticEffect(haptic, id, 1);
            std::thread::sleep(period);
        }
        SDL_StopHapticEffect(haptic, id);
        SDL_DestroyHapticEffect(haptic, id);
        SDL_CloseHaptic(haptic);
        SDL_QuitSubSystem(SDL_INIT_HAPTIC | SDL_INIT_JOYSTICK);
    }
}

/// The first haptic device with a constant force whose name contains `part` (any, without one).
unsafe fn open_device(part: Option<&str>) -> Result<(*mut SDL_Haptic, String), String> {
    let mut count = 0;
    let ids = SDL_GetHaptics(&mut count);
    if ids.is_null() {
        return Err(format!("SDL haptic: {}", sdl_error()));
    }
    let mut seen = Vec::new();
    for k in 0..usize::try_from(count).unwrap_or(0) {
        let id = *ids.add(k);
        let raw = SDL_GetHapticNameForID(id);
        let name = if raw.is_null() { String::new() } else { CStr::from_ptr(raw).to_string_lossy().into_owned() };
        if part.is_some_and(|p| !name.to_lowercase().contains(&p.to_lowercase())) {
            seen.push(name);
            continue;
        }
        let haptic = SDL_OpenHaptic(id);
        if haptic.is_null() {
            seen.push(format!("{name} (cannot open: {})", sdl_error()));
            continue;
        }
        if SDL_GetHapticFeatures(haptic) & u32::from(SDL_HAPTIC_CONSTANT.0) != 0 {
            sdl3_sys::stdinc::SDL_free(ids.cast());
            return Ok((haptic, name));
        }
        SDL_CloseHaptic(haptic);
        seen.push(format!("{name} (no constant force)"));
    }
    sdl3_sys::stdinc::SDL_free(ids.cast());
    Err(if seen.is_empty() {
        "no force-feedback device".to_owned()
    } else {
        format!("no force-feedback device fits: {}", seen.join(", "))
    })
}

#[cfg(test)]
mod tests {
    use super::{level, Options};

    #[test]
    fn options_round_trip_and_default() {
        let mut o = Options {
            rate: 500,
            device: Some("G923".into()),
            sensitivity: [80, 100, 120],
            rotation: Some(540),
            rotation_per_car: true,
            car_sensitivity: vec![(88, [300, 250, 280])],
            ..Options::default()
        };
        o.settings.strength = 0.6;
        o.settings.inverse = true;
        o.settings.dead_zone = 0.04;
        assert_eq!(Options::parse(&o.to_text()), o);
        assert_eq!(Options::parse("[controls]\nbrake = KeyB\n"), Options::default());
        assert_eq!(Options::parse("[ffb]\nstrength = 3\nrate = 0\n").settings.strength, 1.0);
        assert_eq!(
            (o.sensitivity_for(Some(88)), o.sensitivity_for(Some(87)), o.sensitivity_for(None)),
            ([300, 250, 280], [80, 100, 120], [80, 100, 120])
        );
        assert_eq!((o.rotation_for(Some(565)), o.rotation_for(None)), (Some(565), Some(540)));
        o.rotation_per_car = false;
        assert_eq!(o.rotation_for(Some(565)), Some(540));
    }

    #[test]
    fn levels_are_protons() {
        // Measured at the G923's evdev device for these magnitudes (harness/ffb_trace.sh probe).
        assert_eq!(level(-263), -863);
        assert_eq!(level(0), -1);
        assert_eq!(level(10_000), 32767);
        assert_eq!(level(-10_000), -32768);
        assert_eq!(level(5_000), 16383);
        assert_eq!(level(20_000), 32767);
    }
}
