//! The advanced H-shifter (opt-in; ours, after RBRControls 5.3.7.0's HShifter advanced mode,
//! `notes/references/rbrcontrols-hshifter.md`): a gear the lever selects engages only with the clutch pressed past a
//! threshold or with the engine speed matched to the gear's; otherwise the gearbox grinds and the gear stays out. The
//! normal mode is NGP's: the direct gear actions select their gear at once (`bindings`).
//!
//! The engine speed a gear would give comes from the live process's gearbox record (`rbr_recording::gearbox`), so a
//! shift is judged on the drive's own ratios and speed, where RBRControls estimates them from the car's files and the
//! car speed. The timings are RBRControls' tick counts at an assumed 60 ticks a second.
//!
//! The lever's gear is the first direct gear action held. A dogleg layout moves first gear to the gear 2 slot (gear 2
//! to 6 select first to fifth; the gear 1 slot selects nothing). With the lever out of gear and neutral not bound,
//! neutral is selected at once (`Bindings::neutral_on_release`).

use rbr_recording::gearbox::Gearbox;

/// The gear words (`Controls::gear`): 0 reverse, 1 neutral, 2 first.
pub const NEUTRAL: u8 = 1;
/// The clutch pedal past this counts as pressed (RBRControls' `ClutchThreshold`).
pub const DEFAULT_CLUTCH_THRESHOLD: f32 = 0.35;
/// An upshift fails above this throttle, matched or not.
const UPSHIFT_THROTTLE: f32 = 0.5;
/// How long a synchro gear must stay matched before it engages, and how long a dog gear waits for matched revs before
/// it grinds, seconds (5 and 10 ticks; RBRControls engages after more than 4 and grinds after more than 10).
const SYNCHRO_SECONDS: f32 = 5.0 / 60.0;
const DOG_GRACE_SECONDS: f32 = 11.0 / 60.0;
const RAD_PER_SECOND_TO_RPM: f32 = 60.0 / std::f32::consts::TAU;

/// How the gearbox engages a gear.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Engagement {
    #[default]
    Synchro,
    /// A dog box: wider windows and a short wait for the revs, so it needs the clutch less.
    Dog,
}

impl Engagement {
    /// The engine speed window, as factors of the gear's, for an upshift or a downshift.
    fn window(self, up: bool) -> (f32, f32) {
        match (self, up) {
            (Engagement::Synchro, true) => (0.90, 1.05),
            (Engagement::Synchro, false) => (0.98, 1.25),
            (Engagement::Dog, true) => (0.60, 1.60),
            (Engagement::Dog, false) => (0.80, 1.80),
        }
    }

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

/// What a car's gearbox is: its engagement and whether its first gear is on a dogleg (RBRControls' `CarInfo.ini`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CarBox {
    pub engagement: Engagement,
    pub dogleg: bool,
}

impl CarBox {
    /// The `viewer --car-gearbox` value: `synchro`, `dog`, `synchro,dogleg`.
    pub fn arg(self) -> String {
        format!("{}{}", self.engagement.name(), if self.dogleg { ",dogleg" } else { "" })
    }

    pub fn parse_arg(text: &str) -> Option<CarBox> {
        let (engagement, dogleg) = match text.split_once(',') {
            Some((e, "dogleg")) => (e, true),
            Some(_) => return None,
            None => (text, false),
        };
        Some(CarBox { engagement: Engagement::parse(engagement)?, dogleg })
    }

    /// RSF car `id`'s entry of RBRControls' `CarInfo.ini` (`[Car<id>]`: `engagement` 0 synchro, 1 dog, a dog box when
    /// it is missing; `dogleg` 0 or 1), or `None` when it has none.
    pub fn from_car_info(text: &str, id: u32) -> Option<CarBox> {
        let sections = rbr_formats::ini::parse(text).ok()?;
        let section = sections.iter().find(|s| s.name.eq_ignore_ascii_case(&format!("Car{id}")))?;
        let flag = |key: &str| section.get(key).map(|v| v.trim() == "1");
        Some(CarBox {
            engagement: if flag("engagement").unwrap_or(true) { Engagement::Dog } else { Engagement::Synchro },
            dogleg: flag("dogleg").unwrap_or(false),
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
        }
    }
}

/// One frame's inputs.
#[derive(Debug, Clone, Copy, Default)]
pub struct Inputs {
    /// The gear the lever is in (after the dogleg), or `None` out of gear.
    pub lever: Option<u8>,
    pub clutch: f32,
    pub throttle: f32,
    /// rad/s.
    pub engine_speed: f32,
    pub gearbox: Option<Gearbox>,
}

/// The shifter's state between frames.
#[derive(Debug, Clone, Default)]
pub struct Shifter {
    pub gearbox: CarBox,
    pub clutch_threshold: f32,
    pub auto_neutral: bool,
    /// The gear last selected, and the last one other than neutral (which tells an upshift from a downshift).
    selected: Option<u8>,
    last_gear: Option<u8>,
    /// When the pending gear started synchronising (synchro) or waiting for the revs (dog), seconds.
    since: Option<f32>,
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
        let Some(lever) = inputs.lever else {
            (self.since, self.grind) = (None, None);
            return (self.auto_neutral && self.selected != Some(NEUTRAL)).then(|| self.select(NEUTRAL));
        };
        if self.selected == Some(lever) {
            return None;
        }
        if lever == NEUTRAL || inputs.clutch > self.clutch_threshold {
            return Some(self.select(lever));
        }
        // A gear the car does not have stays out, as `CDrive::M6840` would turn it into neutral.
        let target = inputs.gearbox?.engine_speed(lever)?;
        let up = self.last_gear.is_some_and(|last| lever > last);
        let (low, high) = self.gearbox.engagement.window(up);
        let matched = (low * target..=high * target).contains(&inputs.engine_speed)
            && !(up && inputs.throttle > UPSHIFT_THROTTLE);
        let error = (inputs.engine_speed - target).abs() * RAD_PER_SECOND_TO_RPM;
        match (self.gearbox.engagement, matched) {
            (Engagement::Synchro, true) => {
                let since = *self.since.get_or_insert(now);
                self.grind = None;
                if now - since >= SYNCHRO_SECONDS {
                    return Some(self.select(lever));
                }
            }
            (Engagement::Synchro, false) => (self.since, self.grind) = (None, Some(grind_pitch(error))),
            (Engagement::Dog, true) => return Some(self.select(lever)),
            (Engagement::Dog, false) => {
                let since = *self.since.get_or_insert(now);
                if now - since > DOG_GRACE_SECONDS {
                    self.grind = Some(grind_pitch(error));
                }
            }
        }
        None
    }

    fn select(&mut self, gear: u8) -> u8 {
        (self.selected, self.since, self.grind) = (Some(gear), None, None);
        if gear != NEUTRAL {
            self.last_gear = Some(gear);
        }
        gear
    }
}

/// The grind's pitch for an engine speed `error` rpm off the gear's: RBRControls' `0.65 + 1.2 tanh((e - 200) / 1500)`
/// above 200 rpm (0x1002f4a3; the 0.65 below, where it passes no pitch).
fn grind_pitch(error: f32) -> f32 {
    0.65 + 1.2 * ((error - 200.0).max(0.0) * (1.0 / 1500.0)).tanh()
}

/// The gear the lever selects from the direct gear actions held (`held(word)`: whether the action selecting that gear
/// word is held), the first in RBRControls' order: first gear up to sixth, then reverse. A dogleg moves each forward
/// gear down one slot: the gear 2 slot selects first, and the gear 1 slot nothing.
pub fn lever(held: impl Fn(u8) -> bool, dogleg: bool) -> Option<u8> {
    const SLOTS: [u8; 7] = [2, 3, 4, 5, 6, 7, 0];
    let first = SLOTS[0];
    SLOTS.into_iter().filter(|&w| !(dogleg && w == first)).find(|&w| held(w)).map(|w| {
        if dogleg && w != 0 {
            w - 1
        } else {
            w
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A five-speed: reverse, neutral, first to fifth, at a drive speed of 100 (first gear 300 rad/s).
    fn gearbox() -> Gearbox {
        Gearbox { drive_speed: 100.0, count: 7, ratios: [-3.0, 0.0, 3.0, 2.0, 1.5, 1.2, 1.0, 0.0] }
    }

    fn inputs(lever: Option<u8>, engine_speed: f32) -> Inputs {
        Inputs { lever, engine_speed, gearbox: Some(gearbox()), ..Inputs::default() }
    }

    fn synchro() -> Shifter {
        let mut s = Shifter::new(CarBox::default(), DEFAULT_CLUTCH_THRESHOLD, true);
        s.selected = Some(3);
        s.last_gear = Some(3);
        s
    }

    #[test]
    fn the_clutch_engages_at_once() {
        let mut s = synchro();
        let clutch = Inputs { clutch: 0.5, ..inputs(Some(6), 900.0) };
        assert_eq!(s.update(0.0, clutch), Some(6));
        assert_eq!(s.update(0.1, clutch), None, "selected already");
        // Below the threshold it does not count: 2nd at 900 rad/s is no match.
        let mut s = synchro();
        assert_eq!(s.update(0.0, Inputs { clutch: 0.3, ..inputs(Some(2), 900.0) }), None);
        assert!(s.grind().is_some());
    }

    #[test]
    fn a_synchro_upshift_needs_matched_revs_for_a_while() {
        let mut s = synchro();
        // 3rd (word 4) wants 150 rad/s: 160 is above 0.90..1.05 x 150.
        assert_eq!(s.update(0.0, inputs(Some(4), 160.0)), None);
        let pitch = s.grind().unwrap();
        assert!((pitch - 0.65).abs() < 1e-6, "10 rad/s is under 200 rpm: {pitch}");
        assert_eq!(s.update(0.01, inputs(Some(4), 150.0)), None, "matched, synchronising");
        assert_eq!(s.grind(), None);
        assert_eq!(s.update(0.05, inputs(Some(4), 150.0)), None);
        assert_eq!(s.update(0.01 + SYNCHRO_SECONDS, inputs(Some(4), 150.0)), Some(4));
        // An upshift with the throttle down fails however well matched.
        let mut s = synchro();
        assert_eq!(s.update(0.0, Inputs { throttle: 0.8, ..inputs(Some(4), 150.0) }), None);
        assert!(s.grind().is_some());
        // A downshift takes the higher window and ignores the throttle: 1st wants 300, 0.98..1.25.
        let mut s = synchro();
        assert_eq!(s.update(0.0, Inputs { throttle: 0.8, ..inputs(Some(2), 350.0) }), None);
        assert_eq!(s.grind(), None);
        assert_eq!(s.update(1.0, Inputs { throttle: 0.8, ..inputs(Some(2), 350.0) }), Some(2));
    }

    #[test]
    fn a_dog_box_waits_before_it_grinds() {
        let dog = CarBox { engagement: Engagement::Dog, dogleg: false };
        let mut s = Shifter { selected: Some(3), last_gear: Some(3), ..Shifter::new(dog, 0.35, true) };
        assert_eq!(s.update(0.0, inputs(Some(4), 300.0)), None);
        assert_eq!(s.grind(), None, "in the grace time");
        assert_eq!(s.update(0.1, inputs(Some(4), 300.0)), None);
        assert_eq!(s.grind(), None);
        assert_eq!(s.update(0.2, inputs(Some(4), 300.0)), None);
        // 150 rad/s off is 1432 rpm.
        let expected = 0.65 + 1.2 * ((150.0 * RAD_PER_SECOND_TO_RPM - 200.0) / 1500.0).tanh();
        assert!((s.grind().unwrap() - expected).abs() < 1e-5);
        // The revs come down into 0.6..1.6 x 150: it engages at once.
        assert_eq!(s.update(0.3, inputs(Some(4), 230.0)), Some(4));
        assert_eq!(s.grind(), None);
    }

    #[test]
    fn out_of_gear_is_neutral() {
        let mut s = synchro();
        assert_eq!(s.update(0.0, inputs(Some(2), 900.0)), None);
        assert!(s.grind().is_some());
        assert_eq!(s.update(0.1, inputs(None, 900.0)), Some(NEUTRAL));
        assert_eq!(s.grind(), None);
        assert_eq!(s.update(0.2, inputs(None, 900.0)), None);
        // Without the auto neutral, out of gear does nothing.
        let mut s = Shifter { auto_neutral: false, ..synchro() };
        assert_eq!(s.update(0.0, inputs(None, 900.0)), None);
        // Neutral itself (a bound neutral) needs nothing; a stopped car needs the clutch for first.
        assert_eq!(s.update(0.0, inputs(Some(NEUTRAL), 900.0)), Some(NEUTRAL));
        let stopped = Inputs { gearbox: Some(Gearbox { drive_speed: 0.0, ..gearbox() }), ..inputs(Some(2), 90.0) };
        assert_eq!(s.update(0.1, stopped), None);
        assert_eq!(s.update(0.2, Inputs { clutch: 1.0, ..stopped }), Some(2));
        // A gear the car does not have (sixth on a five-speed) stays out.
        assert_eq!(s.update(0.3, Inputs { clutch: 0.0, ..inputs(Some(7), 100.0) }), None);
        assert_eq!(s.grind(), None);
    }

    #[test]
    fn the_lever_and_the_dogleg() {
        let held = |words: &'static [u8]| move |w: u8| words.contains(&w);
        assert_eq!(lever(held(&[]), false), None);
        assert_eq!(lever(held(&[2]), false), Some(2));
        assert_eq!(lever(held(&[0]), false), Some(0));
        assert_eq!(lever(held(&[4, 0]), false), Some(4));
        // Dogleg: the gear 2 slot is first, the gear 6 slot fifth, the gear 1 slot nothing, reverse stays.
        assert_eq!(lever(held(&[3]), true), Some(2));
        assert_eq!(lever(held(&[7]), true), Some(6));
        assert_eq!(lever(held(&[2]), true), None);
        assert_eq!(lever(held(&[0]), true), Some(0));
    }

    #[test]
    fn car_info_and_settings() {
        let text = "; comment\n[Car17]\ncar_name = \"Fiat 126 Grp2\"\nh_pattern = 1\nengagement = 0\ndogleg = 0\n\n\
                    [Car19]\nh_pattern = 1\nengagement = 0\ndogleg = 1\n[Car8]\nh_pattern = 0\n";
        assert_eq!(CarBox::from_car_info(text, 17), Some(CarBox { engagement: Engagement::Synchro, dogleg: false }));
        assert_eq!(CarBox::from_car_info(text, 19), Some(CarBox { engagement: Engagement::Synchro, dogleg: true }));
        assert_eq!(CarBox::from_car_info(text, 8), Some(CarBox { engagement: Engagement::Dog, dogleg: false }));
        assert_eq!(CarBox::from_car_info(text, 99), None);
        for car in [CarBox::default(), CarBox { engagement: Engagement::Dog, dogleg: true }] {
            assert_eq!(CarBox::parse_arg(&car.arg()), Some(car));
        }
        assert_eq!(CarBox::parse_arg("dog,x"), None);
        let dog = CarBox { engagement: Engagement::Dog, dogleg: true };
        let settings = Settings::default();
        assert_eq!(settings.gearbox(None), CarBox::default());
        assert_eq!(settings.gearbox(Some(dog)), CarBox { engagement: Engagement::Dog, dogleg: false });
        let fixed = Settings { engagement: PerCar::Fixed(Engagement::Synchro), dogleg: PerCar::Auto, ..settings };
        assert_eq!(fixed.gearbox(Some(dog)), CarBox { engagement: Engagement::Synchro, dogleg: true });
    }
}
