//! Captures audio and keeps the latest analysis available to the renderer.
//!
//! The default source, `system`, is whatever the computer is playing, with
//! no setup by the user:
//!
//! - Windows: a loopback stream on the default output device (WASAPI).
//! - Linux: the monitor of the default output, through the PulseAudio
//!   protocol. PipeWire systems answer that protocol too.
//! - macOS: a loopback tap on the default output (macOS 14.6 or newer).
//!
//! Each platform has a backend (`backend_pulse`, `backend_cpal`) that opens
//! a stream; everything else here is shared.
//!
//! A worker thread owns the stream. It reopens it when the default output
//! changes or the device goes away, and reports what it is doing through
//! [`AudioStatus`] so the UI can explain problems instead of going quiet.

use std::any::Any;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use darkrgb_core::AudioFeatures;
use rtrb::Consumer;
use serde::Serialize;

use crate::analysis::Analyzer;
#[cfg(not(target_os = "linux"))]
use crate::backend_cpal as backend;
#[cfg(target_os = "linux")]
use crate::backend_pulse as backend;

/// The source id meaning "whatever the computer is playing".
pub const SYSTEM_SOURCE: &str = "system";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioState {
    Starting,
    Running,
    Error,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct AudioStatus {
    pub state: AudioState,
    /// The source that was asked for: `system` or a device id.
    pub source: String,
    /// Name of the device being listened to.
    pub device: Option<String>,
    pub sample_rate: u32,
    pub channels: u16,
    /// What went wrong, in words a user can act on.
    pub message: Option<String>,
}

/// An input the user can pick instead of system audio.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct InputDevice {
    pub id: String,
    pub name: String,
}

enum Command {
    Source(String),
    Sensitivity(f32),
    Stop,
}

struct Shared {
    features: Mutex<AudioFeatures>,
    status: Mutex<AudioStatus>,
    samples_received: AtomicU64,
}

/// Handle to the capture thread. Dropping it stops capture.
pub struct AudioCapture {
    shared: Arc<Shared>,
    commands: Sender<Command>,
    worker: Option<JoinHandle<()>>,
}

impl AudioCapture {
    pub fn start(source: &str, sensitivity: f32) -> Self {
        let shared = Arc::new(Shared {
            features: Mutex::new(AudioFeatures::default()),
            status: Mutex::new(AudioStatus {
                state: AudioState::Starting,
                source: source.to_string(),
                device: None,
                sample_rate: 0,
                channels: 0,
                message: None,
            }),
            samples_received: AtomicU64::new(0),
        });
        let (commands, rx) = mpsc::channel();
        let worker = {
            let shared = shared.clone();
            let source = source.to_string();
            thread::Builder::new()
                .name("audio".into())
                .spawn(move || run(shared, rx, source, sensitivity))
                .expect("could not start the audio thread")
        };
        Self {
            shared,
            commands,
            worker: Some(worker),
        }
    }

    /// The most recent analysis. Cheap; call it every frame.
    pub fn features(&self) -> AudioFeatures {
        *self.shared.features.lock().unwrap()
    }

    /// Total audio frames received since start. Diagnostics compare this
    /// with the sample rate to check that nothing is being dropped.
    pub fn samples_received(&self) -> u64 {
        self.shared.samples_received.load(Ordering::Relaxed)
    }

    pub fn status(&self) -> AudioStatus {
        self.shared.status.lock().unwrap().clone()
    }

    pub fn set_source(&self, source: &str) {
        let _ = self.commands.send(Command::Source(source.to_string()));
    }

    pub fn set_sensitivity(&self, sensitivity: f32) {
        let _ = self.commands.send(Command::Sensitivity(sensitivity));
    }
}

impl Drop for AudioCapture {
    fn drop(&mut self) {
        let _ = self.commands.send(Command::Stop);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

/// Inputs (microphones, line-in) that can be chosen instead of system audio.
pub fn list_inputs() -> Vec<InputDevice> {
    backend::list_inputs()
}

/// What a periodic check of an open stream found.
pub(crate) enum Health {
    Ok,
    /// The default output is now a different device.
    DefaultChanged,
    /// The connection to the sound system is gone.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    Lost,
}

/// A stream that is delivering samples, and what it is connected to.
pub(crate) struct Open {
    /// Mono samples from the stream, written by the audio thread.
    pub samples: Consumer<f32>,
    /// Set by the backend when the stream stops by itself.
    pub failed: Arc<AtomicBool>,
    pub device_name: String,
    pub sample_rate: u32,
    pub channels: u16,
    /// Called every couple of seconds to notice a changed default output
    /// or a lost connection.
    pub health: Box<dyn FnMut() -> Health>,
    /// Whatever must stay alive for the stream to keep running.
    pub _keep: Box<dyn Any>,
}

fn run(shared: Arc<Shared>, commands: Receiver<Command>, mut source: String, mut sensitivity: f32) {
    let set_status = |status: AudioStatus| *shared.status.lock().unwrap() = status;
    let publish = |features: AudioFeatures| *shared.features.lock().unwrap() = features;

    loop {
        set_status(AudioStatus {
            state: AudioState::Starting,
            source: source.clone(),
            device: None,
            sample_rate: 0,
            channels: 0,
            message: None,
        });

        let mut open = match backend::open(&source) {
            Ok(open) => open,
            Err(message) => {
                log::warn!("audio: {message}");
                publish(AudioFeatures::default());
                set_status(AudioStatus {
                    state: AudioState::Error,
                    source: source.clone(),
                    device: None,
                    sample_rate: 0,
                    channels: 0,
                    message: Some(message),
                });
                // Try again shortly: the sound server may still be starting,
                // or a device may be plugged in.
                match commands.recv_timeout(Duration::from_secs(2)) {
                    Ok(Command::Source(next)) => source = next,
                    Ok(Command::Sensitivity(value)) => sensitivity = value,
                    Ok(Command::Stop) | Err(RecvTimeoutError::Disconnected) => return,
                    Err(RecvTimeoutError::Timeout) => {}
                }
                continue;
            }
        };

        log::info!(
            "audio: listening to {} ({} Hz, {} ch)",
            open.device_name,
            open.sample_rate,
            open.channels
        );
        set_status(AudioStatus {
            state: AudioState::Running,
            source: source.clone(),
            device: Some(open.device_name.clone()),
            sample_rate: open.sample_rate,
            channels: open.channels,
            message: None,
        });

        let mut analyzer = Analyzer::new(open.sample_rate);
        analyzer.set_sensitivity(sensitivity);
        let mut buffer: Vec<f32> = Vec::with_capacity(8192);
        let mut last_samples = Instant::now();
        let mut last_idle = Instant::now();
        let mut last_health_check = Instant::now();

        loop {
            match commands.recv_timeout(Duration::from_millis(4)) {
                Ok(Command::Source(next)) => {
                    if next != source {
                        source = next;
                        break;
                    }
                }
                Ok(Command::Sensitivity(value)) => {
                    sensitivity = value;
                    analyzer.set_sensitivity(value);
                }
                Ok(Command::Stop) | Err(RecvTimeoutError::Disconnected) => return,
                Err(RecvTimeoutError::Timeout) => {}
            }

            let available = open.samples.slots();
            if available > 0 {
                if let Ok(chunk) = open.samples.read_chunk(available) {
                    let (a, b) = chunk.as_slices();
                    buffer.clear();
                    buffer.extend_from_slice(a);
                    buffer.extend_from_slice(b);
                    chunk.commit_all();
                    shared
                        .samples_received
                        .fetch_add(buffer.len() as u64, Ordering::Relaxed);
                    analyzer.push(&buffer);
                    publish(analyzer.features());
                }
                last_samples = Instant::now();
                last_idle = last_samples;
            } else if last_samples.elapsed() > Duration::from_millis(100) {
                // Windows loopback sends nothing while nothing plays.
                analyzer.idle(last_idle.elapsed().as_secs_f32());
                last_idle = Instant::now();
                publish(analyzer.features());
            }

            if open.failed.swap(false, Ordering::Relaxed) {
                log::warn!("audio: the stream stopped; reopening");
                break;
            }
            if last_health_check.elapsed() > Duration::from_secs(2) {
                last_health_check = Instant::now();
                match (open.health)() {
                    Health::Ok => {}
                    Health::DefaultChanged => {
                        log::info!("audio: the default output changed; following it");
                        break;
                    }
                    Health::Lost => {
                        log::warn!("audio: lost the sound system; reconnecting");
                        break;
                    }
                }
            }
        }
        // Give a device that just vanished a moment before reopening.
        drop(open);
        thread::sleep(Duration::from_millis(150));
    }
}
