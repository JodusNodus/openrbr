//! Plays a list of force-feedback magnitudes on the wheel through `rbr_render::ffb::Wheel`, at their times, for
//! device traces (`harness/ffb_trace.py`): `ffb_play FILE [--device PART] [--rate HZ]`. Each line of FILE is
//! `seconds magnitude` (seconds from the start, magnitude -10000 to 10000 as the game passes it to DirectInput);
//! `harness/ffb_trace.py magnitudes` writes it from an RSF session. The force ends at 0 after the last line.

use std::time::{Duration, Instant};

use rbr_render::ffb::{Options, Wheel};

fn main() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let file = args.next().ok_or("usage: ffb_play FILE [--device PART] [--rate HZ]")?;
    let mut options = Options::default();
    while let Some(arg) = args.next() {
        let value = args.next().ok_or(format!("{arg} needs a value"))?;
        match arg.as_str() {
            "--device" => options.device = Some(value),
            "--rate" => options.rate = value.parse().map_err(|e| format!("--rate {value}: {e}"))?,
            _ => return Err(format!("{arg}?")),
        }
    }
    let text = std::fs::read_to_string(&file).map_err(|e| format!("{file}: {e}"))?;
    let mut lines = Vec::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let mut words = line.split_whitespace().map(str::parse::<f64>);
        match (words.next(), words.next()) {
            (Some(Ok(t)), Some(Ok(m))) => lines.push((t, m as i32)),
            _ => return Err(format!("{file}: {line}?")),
        }
    }
    let wheel = Wheel::open(options)?;
    eprintln!("ffb_play: {} magnitudes on {}", lines.len(), wheel.name);
    let start = Instant::now();
    for (t, m) in lines {
        let at = start + Duration::from_secs_f64(t.max(0.0));
        std::thread::sleep(at.saturating_duration_since(Instant::now()));
        wheel.set_magnitude(m, 0.0);
    }
    wheel.set_magnitude(0, 0.0);
    std::thread::sleep(Duration::from_millis(50));
    Ok(())
}
