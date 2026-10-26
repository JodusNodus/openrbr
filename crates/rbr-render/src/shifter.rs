//! The advanced H-shifter (opt-in; ours, after how a real gearbox shifts, `notes/ngp.md` "H-shifter"). A shift is two
//! steps. The engaged gear comes out only with the drivetrain unloaded: the clutch in or the throttle lifted. Then the
//! lever's gear goes in once the gearbox input shaft turns at that gear's speed:
//! - A synchro's cone clutch brings the free shaft to speed while the clutch is in, a delay that grows with the
//!   difference. With the clutch out the shaft is the engine, which a synchro cannot pull, so the driver matches the
//!   revs; short of that the blocker ring holds the gear out without a sound, and far off the gearbox grinds.
//! - A dog box has no synchro: the dogs go in at once with the clutch in, without it within a wide window of revs,
//!   and grind outside it.
//! - Reverse has no synchro in either: it needs the car and the shaft stopped (the clutch held in for a moment, or a
//!   forward gear first) and grinds until then.
//!
//! The normal mode is NGP's: the direct gear actions select their gear at once (`bindings`).
//!
//! The gear speeds come from the live process's gearbox record (`rbr_recording::gearbox`), the engaged gear from its
//! HUD record. The lever's gear is the first direct gear action held. A dogleg layout (the E30 M3's Getrag 265) puts
//! reverse in the gear 1 slot and first to fifth in the gear 2 to 6 slots. With the lever out of gear and neutral not
//! bound, the gear comes out when it can (`Bindings::neutral_on_release`).

use rbr_recording::gearbox::Gearbox;
use std::f32::consts::TAU;

/// The gear words (`Controls::gear`): 0 reverse, 1 neutral, 2 first.
pub const NEUTRAL: u8 = 1;
const REVERSE: u8 = 0;
/// The clutch pedal past this counts as pressed.
pub const DEFAULT_CLUTCH_THRESHOLD: f32 = 0.35;
/// With the clutch out, the engaged gear comes out at this throttle or less.
const LIFT_THROTTLE: f32 = 0.2;
/// A synchro gear goes in within this of its speed; a clutchless shift further off than `SYNCHRO_GRIND` grinds.
const SYNCHRO_MATCH: f32 = rpm(300.0);
const SYNCHRO_GRIND: f32 = rpm(1000.0);
/// How fast a synchro brings the free shaft to speed, rad/s²: 1000 rpm in 0.15 s.
const SYNCHRO_RATE: f32 = rpm(1000.0 / 0.15);
/// How fast the free shaft spins down in neutral, rad/s²: from idle to a stop in about 2 s.
const SPIN_DOWN: f32 = rpm(400.0);
/// A dog gear goes in without the clutch within this of its speed.
const DOG_MATCH: f32 = rpm(600.0);
/// Reverse goes in with the shaft and the car turning slower than this between them.
const REVERSE_MATCH: f32 = rpm(100.0);
/// The longest frame the shaft model takes, s (after a pause).
const MAX_FRAME: f32 = 0.1;
/// How long a selected gear counts as engaged before the HUD reports it, s; then it is selected again.
const SELECT_WAIT: f32 = 0.25;

const fn rpm(value: f32) -> f32 {
    value * TAU / 60.0
}

/// How the gearbox engages a gear.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Engagement {
    #[default]
    Synchro,
    /// A dog box: no synchros, so it needs the clutch less but grinds when the revs are off.
    Dog,
}

impl Engagement {
    pub fn name(self) -> &'static str {
        match self {
            Engagement::Synchro => "synchro",
            Engagement::Dog => "dog",
        }
    }

    pub fn parse(text: &str) -> Option<Engagement> {
        [Engagement::Synchro, Engagement::Dog].into_iter().find(|e| e.name() == text)
    }
}

/// What a car's gearbox is: its engagement, whether its first gear is on a dogleg, and whether it is sequential
/// (RBRControls' `CarInfo.ini`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CarBox {
    pub engagement: Engagement,
    pub dogleg: bool,
    /// Shifted up and down only, so the advanced H-shifter stays off.
    pub sequential: bool,
}

impl CarBox {
    /// The `viewer --car-gearbox` value: `synchro`, `dog`, `synchro,dogleg`, `sequential`.
    pub fn arg(self) -> String {
        if self.sequential {
            return "sequential".into();
        }
        format!("{}{}", self.engagement.name(), if self.dogleg { ",dogleg" } else { "" })
    }

    pub fn parse_arg(text: &str) -> Option<CarBox> {
        if text == "sequential" {
            return Some(CarBox { sequential: true, ..CarBox::default() });
        }
        let (engagement, dogleg) = match text.split_once(',') {
            Some((e, "dogleg")) => (e, true),
            Some(_) => return None,
            None => (text, false),
        };
        Some(CarBox { engagement: Engagement::parse(engagement)?, dogleg, sequential: false })
    }

    /// RSF car `id`'s entry of RBRControls' `CarInfo.ini` (`[Car<id>]`: `engagement` 0 synchro, 1 dog, a dog box when
    /// it is missing; `dogleg` 0 or 1; `h_pattern` 0 sequential), or `None` when it has none.
    pub fn from_car_info(text: &str, id: u32) -> Option<CarBox> {
        let sections = rbr_formats::ini::parse(text).ok()?;
        let section = sections.iter().find(|s| s.name.eq_ignore_ascii_case(&format!("Car{id}")))?;
        let flag = |key: &str| section.get(key).map(|v| v.trim() == "1");
        Some(CarBox {
            engagement: if flag("engagement").unwrap_or(true) { Engagement::Dog } else { Engagement::Synchro },
            dogleg: flag("dogleg").unwrap_or(false),
            sequential: flag("h_pattern") == Some(false),
        })
    }
}

/// A setting that the car's gearbox can decide.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PerCar<T> {
    /// The car's (`CarBox`), else the default.
    #[default]
    Auto,
    Fixed(T),
}

/// The player's H-shifter settings (the `[controls]` section's `h-shifter`, `engagement`, `dogleg` and
/// `clutch threshold`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Settings {
    /// The advanced mode; off, the direct gears select at once.
    pub advanced: bool,
    pub engagement: PerCar<Engagement>,
    pub dogleg: PerCar<bool>,
    pub clutch_threshold: f32,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            advanced: false,
            engagement: PerCar::Auto,
            dogleg: PerCar::Fixed(false),
            clutch_threshold: DEFAULT_CLUTCH_THRESHOLD,
        }
    }
}

impl Settings {
    /// The gearbox for a car whose own is `car` (`None`: not known, a synchro box without a dogleg).
    pub fn gearbox(&self, car: Option<CarBox>) -> CarBox {
        let car = car.unwrap_or_default();
        CarBox {
            engagement: match self.engagement {
                PerCar::Auto => car.engagement,
                PerCar::Fixed(e) => e,
            },
            dogleg: match self.dogleg {
                PerCar::Auto => car.dogleg,
                PerCar::Fixed(d) => d,
            },
            ..car
        }
    }
}

/// One frame's inputs.
#[derive(Debug, Clone, Copy)]
pub struct Inputs {
    /// The gear the lever is in (after the dogleg), or `None` out of gear.
    pub lever: Option<u8>,
    /// The gear the car is in (the HUD's).
    pub engaged: u8,
    pub clutch: f32,
    pub throttle: f32,
    /// rad/s.
    pub engine_speed: f32,
    /// `None` before the live process's first frame.
    pub gearbox: Option<Gearbox>,
}

/// The shifter's state between frames.
#[derive(Debug, Clone, Default)]
pub struct Shifter {
    pub gearbox: CarBox,
    pub clutch_threshold: f32,
    pub auto_neutral: bool,
    /// The gearbox input shaft's speed, rad/s: the engine's with the clutch out, the engaged gear's with it in, else
    /// spinning down or brought to speed by a synchro.
    shaft: f32,
    /// The last frame's time.
    last: Option<f32>,
    /// The gear last selected and when, until the HUD reports it.
    selected: Option<(u8, f32)>,
    /// The grind's pitch while the gearbox grinds.
    grind: Option<f32>,
}

impl Shifter {
    pub fn new(gearbox: CarBox, clutch_threshold: f32, auto_neutral: bool) -> Shifter {
        Shifter { gearbox, clutch_threshold, auto_neutral, ..Shifter::default() }
    }

    /// The pitch of the grind (1 = the sample's rate), while the gearbox grinds.
    pub fn grind(&self) -> Option<f32> {
        self.grind
    }

    /// One frame at `now` seconds: the gear to select, if any.
    pub fn update(&mut self, now: f32, inputs: Inputs) -> Option<u8> {
        let dt = self.last.map_or(0.0, |last| (now - last).clamp(0.0, MAX_FRAME));
        self.last = Some(now);
        self.grind = None;
        let gearbox = inputs.gearbox?;
        let engaged = match self.selected {
            Some((gear, at)) if gear != inputs.engaged && now - at < SELECT_WAIT => gear,
            _ => {
                self.selected = None;
                inputs.engaged
            }
        };
        let clutch = inputs.clutch > self.clutch_threshold;
        let lever = inputs.lever.or(self.auto_neutral.then_some(NEUTRAL));
        let synchronising = lever.filter(|&g| g > NEUTRAL && self.gearbox.engagement == Engagement::Synchro);
        self.shaft = match (clutch, engaged, synchronising.and_then(|g| gearbox.engine_speed(g))) {
            (false, _, _) => inputs.engine_speed,
            (true, NEUTRAL, Some(target)) => approach(self.shaft, target, SYNCHRO_RATE * dt),
            (true, NEUTRAL, None) => (self.shaft - SPIN_DOWN * dt).max(0.0),
            (true, gear, _) => gearbox.engine_speed(gear).unwrap_or(0.0),
        };
        // Out of gear with neutral bound, the gear stays in.
        let lever = lever?;
        if lever == engaged {
            return None;
        }
        if engaged != NEUTRAL {
            return (clutch || inputs.throttle <= LIFT_THROTTLE).then(|| self.select(NEUTRAL, now));
        }
        // A gear the car does not have stays out, as `CDrive::M6840` would turn it into neutral.
        let target = gearbox.engine_speed(lever)?;
        let (error, matched, grinds) = if lever == REVERSE {
            // The shaft turns the other way to reverse while the car rolls forward.
            let error = self.shaft + target;
            (error, error <= REVERSE_MATCH, true)
        } else {
            let error = (self.shaft - target).abs();
            match self.gearbox.engagement {
                Engagement::Synchro => (error, error <= SYNCHRO_MATCH, !clutch && error > SYNCHRO_GRIND),
                Engagement::Dog => (error, clutch || error <= DOG_MATCH, true),
            }
        };
        if matched {
            return Some(self.select(lever, now));
        }
        if grinds {
            self.grind = Some(grind_pitch(error * 60.0 / TAU));
        }
        None
    }

    fn select(&mut self, gear: u8, now: f32) -> u8 {
        self.selected = Some((gear, now));
        gear
    }
}

/// `from` moved toward `to` by at most `step`.
fn approach(from: f32, to: f32, step: f32) -> f32 {
    from + (to - from).clamp(-step, step)
}

/// The grind's pitch for a shaft speed `error` rpm off the gear's: 0.65 up to 200 rpm, rising to 1.85 far off.
fn grind_pitch(error: f32) -> f32 {
    0.65 + 1.2 * ((error - 200.0).max(0.0) * (1.0 / 1500.0)).tanh()
}

/// The gear the lever selects from the direct gear actions held (`held(word)`: whether the action selecting that gear
/// word is held), the first of first gear up to sixth, then reverse. A dogleg moves each forward gear down one slot and
/// reverse to the gear 1 slot.
pub fn lever(held: impl Fn(u8) -> bool, dogleg: bool) -> Option<u8> {
    const SLOTS: [u8; 7] = [2, 3, 4, 5, 6, 7, REVERSE];
    SLOTS.into_iter().find(|&w| held(w)).map(|w| match w {
        2 if dogleg => REVERSE,
        REVERSE => REVERSE,
        w if dogleg => w - 1,
        w => w,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A five-speed: reverse, neutral, first to fifth, at a drive speed of 100 (first gear 300 rad/s).
    fn gearbox() -> Gearbox {
        Gearbox { drive_speed: 100.0, count: 7, ratios: [-3.0, 0.0, 3.0, 2.0, 1.5, 1.2, 1.0, 0.0] }
    }

    fn inputs(lever: Option<u8>, engaged: u8, engine_speed: f32) -> Inputs {
        Inputs { lever, engaged, clutch: 0.0, throttle: 0.0, engine_speed, gearbox: Some(gearbox()) }
    }

    fn shifter(engagement: Engagement) -> Shifter {
        Shifter::new(CarBox { engagement, dogleg: false, sequential: false }, DEFAULT_CLUTCH_THRESHOLD, true)
    }

    #[test]
    fn a_gear_comes_out_unloaded() {
        let mut s = shifter(Engagement::Synchro);
        // In 2nd (word 3) at full throttle, the lever in neutral: it stays in.
        assert_eq!(s.update(0.0, Inputs { throttle: 0.9, ..inputs(None, 3, 200.0) }), None);
        assert_eq!(s.update(0.1, Inputs { throttle: 0.1, ..inputs(None, 3, 200.0) }), Some(NEUTRAL));
        // Not selected again while the HUD catches up, nor once it has.
        assert_eq!(s.update(0.11, inputs(None, 3, 200.0)), None);
        assert_eq!(s.update(0.12, inputs(None, NEUTRAL, 200.0)), None);
        // The clutch frees it at full throttle.
        let mut s = shifter(Engagement::Dog);
        assert_eq!(s.update(0.0, Inputs { throttle: 1.0, clutch: 1.0, ..inputs(Some(4), 3, 200.0) }), Some(NEUTRAL));
        // A selection the game does not take is made again.
        assert_eq!(s.update(0.1, inputs(None, 3, 200.0)), None);
        assert_eq!(s.update(0.1 + SELECT_WAIT, inputs(None, 3, 200.0)), Some(NEUTRAL));
    }

    #[test]
    fn a_synchro_needs_matched_revs_without_the_clutch() {
        let mut s = shifter(Engagement::Synchro);
        // 3rd (word 4) wants 150 rad/s: 190 is off (380 rpm) but blocked without a sound.
        assert_eq!(s.update(0.0, inputs(Some(4), NEUTRAL, 190.0)), None);
        assert_eq!(s.grind(), None);
        assert_eq!(s.update(0.1, inputs(Some(4), NEUTRAL, 170.0)), Some(4));
        // Far off it grinds: 1st wants 300, 150 is 1432 rpm short.
        let mut s = shifter(Engagement::Synchro);
        assert_eq!(s.update(0.0, inputs(Some(2), NEUTRAL, 150.0)), None);
        let expected = 0.65 + 1.2 * ((150.0 * 60.0 / TAU - 200.0) / 1500.0).tanh();
        assert!((s.grind().unwrap() - expected).abs() < 1e-5);
    }

    #[test]
    fn a_synchro_brings_the_shaft_to_speed_with_the_clutch_in() {
        let mut s = shifter(Engagement::Synchro);
        // The shaft at 400 rad/s as the clutch goes in; 1st wants 300, a 955 rpm gap: about 0.14 s.
        assert_eq!(s.update(0.0, inputs(None, NEUTRAL, 400.0)), None);
        let clutch = |t, s: &mut Shifter| s.update(t, Inputs { clutch: 1.0, ..inputs(Some(2), NEUTRAL, 80.0) });
        assert_eq!(clutch(0.05, &mut s), None);
        assert_eq!(s.grind(), None);
        assert_eq!(clutch(0.1, &mut s), Some(2));
    }

    #[test]
    fn a_dog_box_goes_in_with_the_clutch_or_close_revs() {
        let mut s = shifter(Engagement::Dog);
        assert_eq!(s.update(0.0, Inputs { clutch: 1.0, ..inputs(Some(2), NEUTRAL, 80.0) }), Some(2));
        let mut s = shifter(Engagement::Dog);
        // 3rd wants 150: 200 is 477 rpm off, in the window.
        assert_eq!(s.update(0.0, inputs(Some(4), NEUTRAL, 200.0)), Some(4));
        let mut s = shifter(Engagement::Dog);
        assert_eq!(s.update(0.0, inputs(Some(4), NEUTRAL, 250.0)), None);
        assert!(s.grind().is_some());
    }

    #[test]
    fn reverse_needs_the_car_and_the_shaft_stopped() {
        let stopped = Gearbox { drive_speed: 0.0, ..gearbox() };
        let reverse = |clutch| Inputs { clutch, gearbox: Some(stopped), ..inputs(Some(REVERSE), NEUTRAL, 84.0) };
        let mut s = shifter(Engagement::Synchro);
        // The clutch out: the shaft is the engine at idle.
        assert_eq!(s.update(0.0, reverse(0.0)), None);
        assert!(s.grind().is_some());
        // The clutch in: it grinds until the shaft spins down below 100 rpm, about 1.75 s.
        let mut t = 0.0;
        while s.update(t, reverse(1.0)).is_none() {
            assert!(s.grind().is_some());
            t += 0.05;
        }
        assert!((1.6..2.0).contains(&t), "{t}");
        // Rolling, it grinds however long the clutch is in.
        let mut s = shifter(Engagement::Synchro);
        let rolling = Inputs { gearbox: Some(Gearbox { drive_speed: 5.0, ..gearbox() }), ..reverse(1.0) };
        assert!((0..100).all(|i| s.update(i as f32 * 0.05, rolling).is_none()));
        // A forward gear first stops the shaft: then reverse goes in at once.
        let mut s = shifter(Engagement::Synchro);
        let first = Inputs { lever: Some(2), ..reverse(1.0) };
        assert_eq!(s.update(0.0, reverse(0.0)), None);
        assert_eq!(s.update(0.05, first), None);
        assert_eq!(s.update(0.1, first), Some(2));
        assert_eq!(s.update(0.2, Inputs { engaged: 2, ..reverse(1.0) }), Some(NEUTRAL));
        assert_eq!(s.update(0.3, reverse(1.0)), Some(REVERSE));
    }

    #[test]
    fn out_of_gear() {
        // Without the auto neutral the gear stays in.
        let mut s = Shifter { auto_neutral: false, ..shifter(Engagement::Synchro) };
        assert_eq!(s.update(0.0, inputs(None, 3, 200.0)), None);
        // Before the first frame nothing happens; a gear the car does not have stays out.
        let mut s = shifter(Engagement::Synchro);
        assert_eq!(s.update(0.0, Inputs { gearbox: None, ..inputs(None, 3, 200.0) }), None);
        assert_eq!(s.update(0.1, Inputs { clutch: 1.0, ..inputs(Some(7), NEUTRAL, 100.0) }), None);
        assert_eq!(s.grind(), None);
    }

    #[test]
    fn the_lever_and_the_dogleg() {
        let held = |words: &'static [u8]| move |w: u8| words.contains(&w);
        assert_eq!(lever(held(&[]), false), None);
        assert_eq!(lever(held(&[2]), false), Some(2));
        assert_eq!(lever(held(&[0]), false), Some(0));
        assert_eq!(lever(held(&[4, 0]), false), Some(4));
        // Dogleg: the gear 1 slot is reverse, the gear 2 slot first, the gear 6 slot fifth; reverse stays.
        assert_eq!(lever(held(&[2]), true), Some(0));
        assert_eq!(lever(held(&[3]), true), Some(2));
        assert_eq!(lever(held(&[7]), true), Some(6));
        assert_eq!(lever(held(&[0]), true), Some(0));
    }

    #[test]
    fn car_info_and_settings() {
        let text = "; comment\n[Car17]\ncar_name = \"Fiat 126 Grp2\"\nh_pattern = 1\nengagement = 0\ndogleg = 0\n\n\
                    [Car19]\nh_pattern = 1\nengagement = 0\ndogleg = 1\n[Car8]\nh_pattern = 0\n";
        assert_eq!(
            CarBox::from_car_info(text, 17),
            Some(CarBox { engagement: Engagement::Synchro, dogleg: false, sequential: false })
        );
        assert_eq!(
            CarBox::from_car_info(text, 19),
            Some(CarBox { engagement: Engagement::Synchro, dogleg: true, sequential: false })
        );
        let sequential = CarBox { engagement: Engagement::Dog, dogleg: false, sequential: true };
        assert_eq!(CarBox::from_car_info(text, 8), Some(sequential));
        assert_eq!(CarBox::from_car_info(text, 99), None);
        let dog = CarBox { engagement: Engagement::Dog, dogleg: true, sequential: false };
        for car in [CarBox::default(), dog, CarBox { sequential: true, ..CarBox::default() }] {
            assert_eq!(CarBox::parse_arg(&car.arg()), Some(car));
        }
        assert_eq!(CarBox::parse_arg("dog,x"), None);
        let settings = Settings::default();
        assert_eq!(settings.gearbox(None), CarBox::default());
        assert_eq!(
            settings.gearbox(Some(dog)),
            CarBox { engagement: Engagement::Dog, dogleg: false, sequential: false }
        );
        let fixed = Settings { engagement: PerCar::Fixed(Engagement::Synchro), dogleg: PerCar::Auto, ..settings };
        assert_eq!(
            fixed.gearbox(Some(dog)),
            CarBox { engagement: Engagement::Synchro, dogleg: true, sequential: false }
        );
    }
}
