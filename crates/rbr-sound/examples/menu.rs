//! Renders the service park's ambience (`rbr_sound::menu`) to a stereo WAV, offline: `menu OUT [SECONDS] [VOLUME]`
//! (defaults 30 s and 1). `RBR_DATA` (and `RBR_PATCH` for `audio.ini`).

use rbr_formats::data::Data;
use rbr_sound::menu::{MenuBank, MenuMixer};

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [out, rest @ ..] = &args[..] else { return Err("usage: menu OUT [SECONDS] [VOLUME]".into()) };
    let number =
        |i: usize, default: f32| rest.get(i).map_or(Ok(default), |s| s.parse::<f32>().map_err(|e| e.to_string()));
    let (seconds, volume) = (number(0, 30.0)?, number(1, 1.0)?);
    let data = Data::from_env().ok_or("RBR_DATA is not set")?;
    let rate = 44100;
    let mut mixer = MenuMixer::new(MenuBank::load(&data)?, rate, volume);
    mixer.set_park(true);
    let spec =
        hound::WavSpec { channels: 2, sample_rate: rate, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
    let mut wav = hound::WavWriter::create(out, spec).map_err(|e| e.to_string())?;
    let mut block = vec![[0.0f32; 2]; 441];
    let mut peak = 0.0f32;
    for _ in 0..(seconds * 100.0) as usize {
        mixer.render(&mut block);
        for &s in block.iter().flatten() {
            peak = peak.max(s.abs());
            wav.write_sample((s * 32767.0) as i16).map_err(|e| e.to_string())?;
        }
    }
    wav.finalize().map_err(|e| e.to_string())?;
    eprintln!("menu: {seconds} s, peak {peak:.3}");
    Ok(())
}
