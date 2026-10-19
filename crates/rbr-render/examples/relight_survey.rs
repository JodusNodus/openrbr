//! The relight fit (`rbr_render::relight`) over every retail stage (explore/time-of-day, ours): the fitted bake sun,
//! the angle to the variant's `SunDir`, the strength and the correlation.
//!
//! RBR_DATA=<install> cargo run --release -p rbr-render --example relight_survey

use rbr_formats::data::Data;
use rbr_render::stage::{self, Selection};
use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    let data = Data::from_env().ok_or("RBR_DATA is not set")?;
    let maps = data.install().join("Maps");
    let mut names: Vec<String> = std::fs::read_dir(&maps)?
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .filter_map(|f| f.strip_suffix(".lbs").filter(|n| n.starts_with("track-")).map(str::to_owned))
        .collect();
    names.sort_by_key(|n| (n[6..].split('_').next().and_then(|m| m.parse::<u32>().ok()), n.clone()));
    println!("{:<12} {:>7} {:>6} {:>6} {:>9}  section", "stage", "r", "b", "to Sun", "elev");
    for name in names {
        let stage = match stage::load(&maps, &name, Selection::default()) {
            Ok(stage) => stage,
            Err(e) => {
                println!("{name:<12} load failed: {e}");
                continue;
            }
        };
        let fit = rbr_render::relight::fit(&stage);
        let sun = -stage.atmosphere.sun_direction;
        let angle = fit.bake_sun.angle_between(sun).to_degrees();
        println!(
            "{name:<12} {:>7.3} {:>6.3} {:>5.0}° {:>8.0}°  {}  start {:?}",
            fit.r,
            fit.strength,
            angle,
            fit.bake_sun.y.asin().to_degrees(),
            stage.atmosphere.section.as_deref().unwrap_or("-"),
            stage.start.map(|s| (
                s.position.to_array().map(|v| v.round()),
                s.forward.to_array().map(|v| (v * 100.0).round() / 100.0)
            ))
        );
    }
    Ok(())
}
