//! The stage's precipitation and wind settings, `Misc/track_w.ini` in `Root/misc.rbz` (`E21_WeatherA` 0x4a0910,
//! `CWeatherDaemon::M97e0` 0x4a97e0; `notes/weather.md`, sections 4 and 6), and the wind's perturbation
//! (`CWeatherDaemon::M9a20`). Physics frame, z up.

use std::io::Read;

use glam::Vec3;
use rbr_formats::{data::Data, ini, rbz};

use super::sim::Rng;

/// One map's `[TrackNN]` values, with the game's defaults for missing keys (13 m, 0.5, add the car's velocity).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrackWeather {
    pub wind: Vec3,
    pub perturbation: Vec3,
    /// Seconds between wind perturbations.
    pub perturbation_time: f32,
    /// Rain's and snow's fall height and forward distance, and whether the car's velocity is added.
    pub rain: Fall,
    pub snow: Fall,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fall {
    pub height: f32,
    pub forward: f32,
    pub add_car_velocity: bool,
}

const DEFAULT_FALL: Fall = Fall { height: 13.0, forward: 0.5, add_car_velocity: true };

impl Default for TrackWeather {
    fn default() -> Self {
        TrackWeather {
            wind: Vec3::ZERO,
            perturbation: Vec3::ZERO,
            perturbation_time: 1.0,
            rain: DEFAULT_FALL,
            snow: DEFAULT_FALL,
        }
    }
}

impl TrackWeather {
    /// The values of `[Track<map, two digits>]` in `text`; the defaults when the map has no section.
    pub fn parse(text: &str, map: u32) -> TrackWeather {
        let Ok(sections) = ini::parse(text) else { return TrackWeather::default() };
        let Some(section) = sections.iter().find(|s| s.name.eq_ignore_ascii_case(&format!("Track{map:02}"))) else {
            return TrackWeather::default();
        };
        let float = |key: &str, default: f32| section.get(key).and_then(|v| v.trim().parse().ok()).unwrap_or(default);
        let flag = |key: &str| section.get(key).is_none_or(|v| v.trim().eq_ignore_ascii_case("true"));
        let vec = |prefix: &str| {
            Vec3::new(
                float(&format!("{prefix}X"), 0.0),
                float(&format!("{prefix}Y"), 0.0),
                float(&format!("{prefix}Z"), 0.0),
            )
        };
        let fall = |kind: &str| Fall {
            height: float(&format!("{kind}FallHeight"), DEFAULT_FALL.height),
            forward: float(&format!("{kind}ForwardDistance"), DEFAULT_FALL.forward),
            add_car_velocity: flag(&format!("{kind}AddCarVelocity")),
        };
        TrackWeather {
            wind: vec("Wind"),
            perturbation: vec("WindPerturbation"),
            perturbation_time: float("WindPerturbationTime", 1.0),
            rain: fall("Rain"),
            snow: fall("Snow"),
        }
    }

    /// The map's values from the install's `Root/misc.rbz`; the defaults when it cannot be read.
    pub fn load(data: &Data, map: u32) -> TrackWeather {
        let mut text = String::new();
        let read = data
            .resolve("Root/misc.rbz")
            .and_then(|path| rbz::open(&path).ok())
            .and_then(|mut zip| zip.by_name("Misc/track_w.ini").ok()?.read_to_string(&mut text).ok());
        if read.is_none() {
            return TrackWeather::default();
        }
        TrackWeather::parse(&text, map)
    }
}

/// The wind the particles follow: the base plus the perturbation times a random value in [-1, 1] per axis, drawn anew
/// every perturbation time (`M9a20`; the game's random draws are its own).
#[derive(Debug, Clone)]
pub struct Wind {
    settings: TrackWeather,
    since: f32,
    pub now: Vec3,
    rng: Rng,
}

impl Wind {
    pub fn new(settings: TrackWeather, seed: u32) -> Wind {
        Wind { settings, since: f32::INFINITY, now: settings.wind, rng: Rng::new(seed) }
    }

    /// Moves on by `dt` seconds and gives the wind.
    pub fn step(&mut self, dt: f32) -> Vec3 {
        self.since += dt;
        if self.since >= self.settings.perturbation_time {
            self.since = 0.0;
            let r = Vec3::new(self.rng.signed(), self.rng.signed(), self.rng.signed());
            self.now = self.settings.wind + self.settings.perturbation * r;
        }
        self.now
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sections_and_defaults() {
        let text = "[Track10]\nWindX = 10.0\nWindY = 4\nWindZ = -2\nWindPerturbationX = 2\nRainFallHeight = 7.0\n\
                    SnowForwardDistance = 0.8\nSnowAddCarVelocity = FALSE\n";
        let w = TrackWeather::parse(text, 10);
        assert_eq!(w.wind, Vec3::new(10.0, 4.0, -2.0));
        assert_eq!(w.rain, Fall { height: 7.0, forward: 0.5, add_car_velocity: true });
        assert_eq!(w.snow, Fall { height: 13.0, forward: 0.8, add_car_velocity: false });
        assert_eq!(TrackWeather::parse(text, 4), TrackWeather::default());
        let mut wind = Wind::new(w, 3);
        let first = wind.step(0.01);
        assert!((first.x - 10.0).abs() <= 2.0 && first.y == 4.0);
        assert_eq!(wind.step(0.5), first, "held until the perturbation time");
    }
}
