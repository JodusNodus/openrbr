//! Writes the scripted events of a recorded drive as CSV for `rbr-sound --events`: per recording frame, the rescue
//! helicopter's body while it runs (`heli`), each bird strike (`bird`) and the helicopter call (`call`), in the
//! physics frame (z up). Usage: `scripted_events STAGE RECORDING OUT [--rescue-at STEP] [--birdstrike]`.

use std::error::Error;
use std::fmt::Write as _;

use glam::Vec3;
use rbr_formats::data::Data;
use rbr_render::characters::load::Options;
use rbr_render::characters::scripted::Event;
use rbr_render::characters::Characters;
use rbr_render::playback::Drive;
use rbr_render::stage;

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args().skip(1);
    let name = args.next().ok_or("stage name required")?;
    let recording = args.next().ok_or("recording path required")?;
    let out = args.next().ok_or("output path required")?;
    let (mut rescue_at, mut options) = (None, Options::default());
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--rescue-at" => rescue_at = Some(args.next().ok_or("--rescue-at needs a step")?.parse::<u32>()?),
            "--birdstrike" => options.birdstrike = true,
            other => return Err(format!("unknown argument {other}").into()),
        }
    }
    let data = Data::from_env().ok_or("RBR_DATA is not set")?;
    let mut stage = stage::load(&data.install().join("Maps"), &name, Default::default())?;
    let mut characters = Characters::load_with(&data, &name, &mut stage, options)?;
    let drive = Drive::parse(&std::fs::read(recording)?)?;
    let physics = |p: Vec3| [p.x, p.z, p.y];
    let mut csv = String::from("frame,kind,x,y,z\n");
    let mut line = |frame: usize, kind: &str, [x, y, z]: [f32; 3]| writeln!(csv, "{frame},{kind},{x},{y},{z}");
    for (frame, (end, sample)) in drive.frames().enumerate() {
        if Some(sample.step) == rescue_at {
            characters.call_rescue();
        }
        let car = characters.on_line(sample);
        characters.step(end, car);
        for event in characters.take_events() {
            match event {
                Event::RescueCalled(p) => line(frame, "call", physics(p))?,
                Event::Birdstrike(p) => line(frame, "bird", physics(p))?,
                _ => {}
            }
        }
        if let Some(p) = characters.rescue_position() {
            line(frame, "heli", physics(p))?;
        }
    }
    std::fs::write(out, csv)?;
    Ok(())
}
