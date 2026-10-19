//! Prints what the particles make of a recorded drive's wheels: per sampled frame, each wheel's surface id under it,
//! its contact and its mass flow (`rbr_render::particles`, playback). Usage: `particles_probe STAGE RECORDING [EVERY]`.

use rbr_formats::data::Data;
use rbr_recording::{pose, Frames, WHEELS};
use rbr_render::particles::Particles;
use rbr_render::playback::Sample;
use rbr_render::stage::Selection;
use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args().skip(1);
    let stage = args.next().ok_or("stage name required")?;
    let recording = args.next().ok_or("recording path required")?;
    let every: usize = args.next().map_or(Ok(500), |s| s.parse())?;
    let data = Data::from_env().ok_or("RBR_DATA is not set")?;
    let particles = Particles::load(&data, &stage, Selection::default(), None, None)?.ok_or("no particle file")?;
    let bytes = std::fs::read(recording)?;
    let mut samples = Vec::new();
    for frame in Frames::new(&bytes) {
        let frame = frame?;
        let car = pose::car_pose(frame.car).ok_or("car block")?;
        let wheels: Vec<_> = (0..WHEELS).filter_map(|i| frame.wheel(i).and_then(pose::wheel_pose)).collect();
        let wheels = wheels.try_into().map_err(|_| "wheel blocks")?;
        samples.push(Sample {
            step: frame.post,
            car,
            wheels,
            control: frame.control_f32s(),
            brake_lamp: 0.0,
            track: 0.0,
            assist: None,
            shake: [glam::Vec3::ZERO; 5],
            hold: 0.0,
        });
    }
    let mut surfaces = std::collections::BTreeMap::new();
    for i in (1..samples.len()).step_by(every.max(1)) {
        let wheels = particles.wheels_from_poses(&samples[i], Some(&samples[i - 1]));
        for w in &wheels {
            *surfaces.entry(w.surface).or_insert(0) += 1;
        }
        let text: Vec<String> = wheels
            .iter()
            .map(|w| format!("s{} {} m{:.2} z{:.2}", w.surface, if w.contact { "on" } else { "off" }, w.mass, w.at.z))
            .collect();
        println!("step {} speed {:.1}: {}", samples[i].step, samples[i].car.speed(), text.join(" | "));
    }
    println!("surfaces sampled: {surfaces:?}");
    // A contact sheet of the decoded sprites, 8 to a row, over a grey ground (`PARTICLES_SHEET=out.png`).
    if let Some(out) = std::env::var_os("PARTICLES_SHEET") {
        let files = rbr_render::particles::sprite_files(&data, &particles.sprites)?;
        let sprites = rbr_render::particles::gpu::Sprites::decode(&files);
        let n = sprites.len();
        let size = rbr_render::particles::gpu::SPRITE_SIZE as usize;
        let (cols, rows) = (8, n.div_ceil(8));
        let mut sheet = vec![96u8; cols * size * rows * size * 3];
        for i in 0..n {
            let rgba = sprites.rgba(i).unwrap();
            for y in 0..size {
                for x in 0..size {
                    let p = &rgba[(y * size + x) * 4..][..4];
                    let at = (((i / cols) * size + y) * cols * size + (i % cols) * size + x) * 3;
                    for k in 0..3 {
                        let a = f32::from(p[3]) / 255.0;
                        sheet[at + k] = (f32::from(p[k]) * a + 96.0 * (1.0 - a)) as u8;
                    }
                }
            }
        }
        let mut encoder = png::Encoder::new(std::fs::File::create(out)?, (cols * size) as u32, (rows * size) as u32);
        encoder.set_color(png::ColorType::Rgb);
        encoder.write_header()?.write_image_data(&sheet)?;
    }
    Ok(())
}
