//! The audio device: the system's default output through cpal, fed by a [`Render`]er (the drive's [`Mixer`], the
//! menus' `MenuMixer`) at the device's rate. The stereo
//! mix goes to the first two channels and their average to any others; a mono device takes the average.

use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::mixer::Mixer;

/// What the output plays: stereo frames, -1 to 1, into `out` (overwritten).
pub trait Render: Send + 'static {
    fn render(&mut self, out: &mut [[f32; 2]]);
}

impl Render for Mixer {
    fn render(&mut self, out: &mut [[f32; 2]]) {
        Mixer::render(self, out);
    }
}

/// A playing output stream. Dropping it stops the sound.
pub struct Output {
    stream: cpal::Stream,
}

impl Output {
    /// Opens the default output and plays the mixer `make` builds for its rate.
    pub fn start<M: Render>(make: impl FnOnce(u32) -> M) -> Result<(Output, Arc<Mutex<M>>), String> {
        let host = cpal::default_host();
        let device = host.default_output_device().ok_or("no audio output device")?;
        let supported = device.default_output_config().map_err(|e| e.to_string())?;
        let format = supported.sample_format();
        let config: cpal::StreamConfig = supported.into();
        let channels = usize::from(config.channels.max(1));
        let mixer = Arc::new(Mutex::new(make(config.sample_rate.0)));
        let shared = mixer.clone();
        let mut stereo = Vec::new();
        let mut fill = move |frames: usize| -> Vec<[f32; 2]> {
            stereo.resize(frames, [0.0; 2]);
            if let Ok(mut mixer) = shared.lock() {
                mixer.render(&mut stereo);
            } else {
                stereo.fill([0.0; 2]);
            }
            stereo.clone()
        };
        let spread = |frame: &mut [f32], [l, r]: [f32; 2]| match frame {
            [only] => *only = (l + r) * 0.5,
            [left, right, rest @ ..] => {
                *left = l;
                *right = r;
                rest.fill((l + r) * 0.5);
            }
            [] => {}
        };
        let error = |e| eprintln!("audio: {e}");
        let stream = match format {
            cpal::SampleFormat::F32 => device.build_output_stream(
                &config,
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    let frames = fill(data.len() / channels);
                    for (frame, s) in data.chunks_mut(channels).zip(frames) {
                        spread(frame, s);
                    }
                },
                error,
                None,
            ),
            cpal::SampleFormat::I16 => device.build_output_stream(
                &config,
                move |data: &mut [i16], _: &cpal::OutputCallbackInfo| {
                    let frames = fill(data.len() / channels);
                    let mut floats = [0.0f32; 8];
                    for (frame, s) in data.chunks_mut(channels).zip(frames) {
                        let floats = &mut floats[..frame.len().min(8)];
                        spread(floats, s);
                        for (out, f) in frame.iter_mut().zip(floats.iter()) {
                            *out = (f * 32767.0) as i16;
                        }
                    }
                },
                error,
                None,
            ),
            other => return Err(format!("audio output format {other} is not supported")),
        }
        .map_err(|e| e.to_string())?;
        stream.play().map_err(|e| e.to_string())?;
        Ok((Output { stream }, mixer))
    }

    /// Stops or restarts the stream: the renderer is not called while paused, so it goes on where it stopped.
    pub fn set_paused(&self, paused: bool) {
        let result = match paused {
            true => self.stream.pause().map_err(|e| e.to_string()),
            false => self.stream.play().map_err(|e| e.to_string()),
        };
        if let Err(e) = result {
            eprintln!("audio: {e}");
        }
    }
}
