//! Plays the engine on the default audio output: an rpm sweep from idle to 8000 and back at full throttle, one
//! input per 1/100 s, as a live drive feeds it. A check that the device opens and the player runs.
//!
//! Usage: RBR_DATA=<install> RBR_PATCH=<patch> cargo run --release -p rbr-sound --example sweep [SECONDS]

use std::time::{Duration, Instant};

use rbr_formats::data::Data;
use rbr_recording::sound::{Inputs, Sound};
use rbr_sound::Player;

fn main() -> Result<(), String> {
    let seconds: f32 = std::env::args().nth(1).map_or(Ok(6.0), |s| s.parse()).map_err(|e| format!("{e}"))?;
    let data = Data::from_env().ok_or("RBR_DATA is not set")?;
    let mut player = Player::start(&data, 6, 1.0, rbr_sound::Sky::default(), true)?;
    let start = Instant::now();
    let steps = (seconds * 100.0) as u32;
    for step in 0..steps {
        let phase = step as f32 / steps as f32;
        let rpm = 1000.0 + 7000.0 * (1.0 - (2.0 * phase - 1.0).abs());
        let mut sound = Sound { battery: 12.0, brakes: 1, ..Sound::default() };
        sound.engine[0] = (rpm / 9.549_296).to_bits();
        sound.engine[1] = 1.0f32.to_bits();
        for flag in &mut sound.engine[8..12] {
            *flag = 1.0f32.to_bits();
        }
        player.step(&Inputs { sound, ..Inputs::default() });
        let due = Duration::from_millis(10 * u64::from(step + 1));
        std::thread::sleep(due.saturating_sub(start.elapsed()));
    }
    eprintln!("sweep: {steps} steps in {:.2} s", start.elapsed().as_secs_f32());
    Ok(())
}
