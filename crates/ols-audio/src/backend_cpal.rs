//! Capture through cpal, used on Windows and macOS.
//!
//! Opening the default *output* device as an input turns on loopback
//! capture: WASAPI loopback on Windows, a CoreAudio tap on macOS 14.6+.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SampleFormat, SizedSample};
use rtrb::{Producer, RingBuffer};

use crate::capture::{Health, InputDevice, Open, SYSTEM_SOURCE};

pub(crate) fn list_inputs() -> Vec<InputDevice> {
    let Ok(devices) = cpal::default_host().input_devices() else {
        return Vec::new();
    };
    devices
        .filter_map(|device| {
            Some(InputDevice {
                id: device.id().ok()?.to_string(),
                name: device.description().ok()?.name().to_string(),
            })
        })
        .collect()
}

pub(crate) fn open(source: &str) -> Result<Open, String> {
    let (device, config, device_name, following) = if source == SYSTEM_SOURCE {
        system_source()?
    } else {
        chosen_input(source)?
    };

    let sample_rate = config.sample_rate();
    let channels = config.channels();
    // Room for two seconds, far more than the worker ever leaves unread.
    let (producer, samples) = RingBuffer::<f32>::new(sample_rate as usize * 2);
    let failed = Arc::new(AtomicBool::new(false));

    let stream_config = config.config();
    let stream = match config.sample_format() {
        SampleFormat::F32 => build::<f32>(&device, stream_config, producer, failed.clone()),
        SampleFormat::I16 => build::<i16>(&device, stream_config, producer, failed.clone()),
        SampleFormat::I32 => build::<i32>(&device, stream_config, producer, failed.clone()),
        SampleFormat::I24 => build::<cpal::I24>(&device, stream_config, producer, failed.clone()),
        SampleFormat::U16 => build::<u16>(&device, stream_config, producer, failed.clone()),
        SampleFormat::U8 => build::<u8>(&device, stream_config, producer, failed.clone()),
        SampleFormat::I8 => build::<i8>(&device, stream_config, producer, failed.clone()),
        SampleFormat::F64 => build::<f64>(&device, stream_config, producer, failed.clone()),
        other => {
            return Err(format!(
                "{device_name} uses a sample format this app cannot read ({other})."
            ));
        }
    }
    .map_err(|e| format!("Could not listen to {device_name}: {e}"))?;
    stream
        .play()
        .map_err(|e| format!("Could not start listening to {device_name}: {e}"))?;

    let health: Box<dyn FnMut() -> Health> = match following {
        Some(opened_on) => Box::new(move || match default_output_id() {
            Some(current) if current != opened_on => Health::DefaultChanged,
            _ => Health::Ok,
        }),
        None => Box::new(|| Health::Ok),
    };

    Ok(Open {
        samples,
        failed,
        device_name,
        sample_rate,
        channels,
        health,
        _keep: Box::new(stream),
    })
}

fn build<T>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    mut producer: Producer<f32>,
    failed: Arc<AtomicBool>,
) -> Result<cpal::Stream, cpal::Error>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    let channels = config.channels.max(1) as usize;
    device.build_input_stream(
        config,
        move |data: &[T], _: &cpal::InputCallbackInfo| {
            // Runs on the audio thread: no locks, no allocation. If the
            // worker falls behind, samples are dropped rather than waited on.
            for frame in data.chunks_exact(channels) {
                let sum: f32 = frame.iter().map(|s| f32::from_sample(*s)).sum();
                let _ = producer.push(sum / channels as f32);
            }
        },
        move |error: cpal::Error| match error.kind() {
            // These do not stop the stream.
            cpal::ErrorKind::Xrun | cpal::ErrorKind::RealtimeDenied | cpal::ErrorKind::DeviceChanged => {
                log::debug!("audio: {error}");
            }
            _ => {
                log::warn!("audio: {error}");
                failed.store(true, Ordering::Relaxed);
            }
        },
        Some(Duration::from_secs(5)),
    )
}

type Source = (cpal::Device, cpal::SupportedStreamConfig, String, Option<String>);

fn system_source() -> Result<Source, String> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or("This computer has no audio output to listen to.")?;
    let name = device_name(&device);
    let config = device
        .default_output_config()
        .map_err(|e| format!("Could not read the settings of {name}: {e}"))?;
    let following = device.id().ok().map(|id| id.to_string());
    Ok((device, config, name, following))
}

fn chosen_input(id: &str) -> Result<Source, String> {
    let device_id: cpal::DeviceId = id
        .parse()
        .map_err(|_| "The chosen audio input is not valid. Pick another one.".to_string())?;
    let host =
        cpal::host_from_id(device_id.host()).map_err(|e| format!("The chosen audio input is not available: {e}"))?;
    let device = host
        .device_by_id(&device_id)
        .ok_or("The chosen audio input is not connected. Plug it in or pick another one.")?;
    let name = device_name(&device);
    let config = device
        .default_input_config()
        .map_err(|e| format!("Could not read the settings of {name}: {e}"))?;
    Ok((device, config, name, None))
}

fn device_name(device: &cpal::Device) -> String {
    device
        .description()
        .map(|d| d.name().to_string())
        .unwrap_or_else(|_| "audio device".to_string())
}

/// Id of the current default output, to notice when the user switches
/// speakers or headphones.
fn default_output_id() -> Option<String> {
    cpal::default_host()
        .default_output_device()?
        .id()
        .ok()
        .map(|id| id.to_string())
}
