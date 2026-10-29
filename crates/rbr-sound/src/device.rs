//! The audio device: the system's default output through cpal, fed by a [`Render`]er (the drive's [`Mixer`], the
//! menus' `MenuMixer`) at the device's rate. The stereo
//! mix goes to the first two channels and their average to any others; a mono device takes the average.

use std::sync::{mpsc, Arc, Mutex};

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

/// A playing output stream, held by its own thread: a cpal stream stays on the thread that made it, and the output can
/// then be made on one thread (the viewer's loading) and played from another. Dropping it stops the sound.
pub struct Output {
    /// Pauses (`true`) or restarts the stream; dropped, the thread drops the stream.
    paused: mpsc::Sender<bool>,
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
        let (paused, pauses) = mpsc::channel();
        let (started, start) = mpsc::channel();
        let open = move || {
            // Each stream owns a stereo buffer reused between callbacks: the mix renders into it and spreads
            // to the channels, with no allocation or copy per callback.
            match format {
                cpal::SampleFormat::F32 => {
                    let shared = shared.clone();
                    let mut stereo = Vec::new();
                    device.build_output_stream(
                        &config,
                        move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                            stereo.resize(data.len() / channels, [0.0; 2]);
                            if let Ok(mut mixer) = shared.lock() {
                                mixer.render(&mut stereo);
                            } else {
                                stereo.fill([0.0; 2]);
                            }
                            for (frame, s) in data.chunks_mut(channels).zip(stereo.iter()) {
                                spread(frame, *s);
                            }
                        },
                        error,
                        None,
                    )
                }
                cpal::SampleFormat::I16 => {
                    let shared = shared.clone();
                    let mut stereo = Vec::new();
                    device.build_output_stream(
                        &config,
                        move |data: &mut [i16], _: &cpal::OutputCallbackInfo| {
                            stereo.resize(data.len() / channels, [0.0; 2]);
                            if let Ok(mut mixer) = shared.lock() {
                                mixer.render(&mut stereo);
                            } else {
                                stereo.fill([0.0; 2]);
                            }
                            let mut floats = [0.0f32; 8];
                            for (frame, s) in data.chunks_mut(channels).zip(stereo.iter()) {
                                let floats = &mut floats[..frame.len().min(8)];
                                spread(floats, *s);
                                for (out, f) in frame.iter_mut().zip(floats.iter()) {
                                    *out = (f * 32767.0) as i16;
                                }
                            }
                        },
                        error,
                        None,
                    )
                }
                other => return Err(format!("audio output format {other} is not supported")),
            }
            .map_err(|e| e.to_string())
            .and_then(|stream| stream.play().map(|()| stream).map_err(|e| e.to_string()))
        };
        let play = move || match open() {
            Ok(stream) => {
                let _ = started.send(Ok(()));
                for paused in pauses {
                    let result = match paused {
                        true => stream.pause().map_err(|e| e.to_string()),
                        false => stream.play().map_err(|e| e.to_string()),
                    };
                    if let Err(e) = result {
                        eprintln!("audio: {e}");
                    }
                }
            }
            Err(e) => {
                let _ = started.send(Err(e));
            }
        };
        std::thread::Builder::new().name("audio".into()).spawn(play).map_err(|e| e.to_string())?;
        start.recv().map_err(|_| "the audio thread stopped".to_owned())??;
        Ok((Output { paused }, mixer))
    }

    /// Stops or restarts the stream: the renderer is not called while paused, so it goes on where it stopped.
    pub fn set_paused(&self, paused: bool) {
        let _ = self.paused.send(paused);
    }
}
