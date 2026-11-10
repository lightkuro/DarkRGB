//! Capture on Linux through the PulseAudio protocol.
//!
//! PipeWire answers the same protocol, so this one path covers both. The
//! client is pure Rust; nothing needs installing.
//!
//! System audio is the "monitor" of the default output: a source the sound
//! server provides that carries whatever is being played.

use std::ffi::CString;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use futures::executor::block_on;
use pulseaudio::Client;
use pulseaudio::protocol;
use rtrb::RingBuffer;

use crate::capture::{Health, InputDevice, Open, SYSTEM_SOURCE};

/// Prefix on input ids, so ids say which sound system they belong to.
const ID_PREFIX: &str = "pulse:";

const NO_SERVER: &str = "No PulseAudio or PipeWire sound server was found, so audio cannot be captured. \
                         Check that sound works in other apps.";

fn connect() -> Result<Client, String> {
    Client::from_env(c"OpenLightShow").map_err(|_| NO_SERVER.to_string())
}

fn text(value: &Option<CString>, fallback: &CString) -> String {
    value.as_ref().unwrap_or(fallback).to_string_lossy().into_owned()
}

pub(crate) fn list_inputs() -> Vec<InputDevice> {
    let Ok(client) = connect() else {
        return Vec::new();
    };
    let Ok(sources) = block_on(client.list_sources()) else {
        return Vec::new();
    };
    sources
        .iter()
        // Monitors of outputs are what `system` already covers.
        .filter(|s| s.monitor_of_sink_index.is_none())
        .map(|s| InputDevice {
            id: format!("{ID_PREFIX}{}", s.name.to_string_lossy()),
            name: text(&s.description, &s.name),
        })
        .collect()
}

/// Name of the default output, as the server knows it.
fn default_sink(client: &Client) -> Result<CString, String> {
    block_on(client.server_info())
        .map_err(|e| format!("The sound server did not answer: {e}"))?
        .default_sink_name
        .ok_or_else(|| "The sound server has no default output to listen to.".to_string())
}

pub(crate) fn open(source: &str) -> Result<Open, String> {
    let client = connect()?;

    let (source_info, device_name, following) = if source == SYSTEM_SOURCE {
        let sink_name = default_sink(&client)?;
        let sink = block_on(client.sink_info_by_name(sink_name.clone()))
            .map_err(|e| format!("Could not look up the default output: {e}"))?;
        let sink_label = text(&sink.description, &sink.name);
        let monitor_name = sink
            .monitor_source_name
            .clone()
            .ok_or_else(|| format!("The default output ({sink_label}) has no monitor to listen to."))?;
        let monitor = block_on(client.source_info_by_name(monitor_name))
            .map_err(|e| format!("Could not open the monitor of {sink_label}: {e}"))?;
        (monitor, sink_label, Some(sink_name))
    } else {
        let name = source
            .strip_prefix(ID_PREFIX)
            .and_then(|n| CString::new(n).ok())
            .ok_or("The chosen audio input is not valid. Pick another one.")?;
        let info = block_on(client.source_info_by_name(name))
            .map_err(|_| "The chosen audio input is not connected. Plug it in or pick another one.".to_string())?;
        let label = text(&info.description, &info.name);
        (info, label, None)
    };

    let sample_rate = source_info.sample_spec.sample_rate;
    let channels = source_info.channel_map.num_channels() as u16;
    if sample_rate == 0 {
        return Err(format!("{device_name} reported no sample rate."));
    }

    // Ask the server for mono 32-bit float at the device's own rate. It
    // mixes the channels down and converts the format for us.
    let frame_bytes = 4u32;
    let mut props = protocol::Props::new();
    props.set(protocol::Prop::MediaName, c"Audio for lights");
    let params = protocol::RecordStreamParams {
        source_index: Some(source_info.index),
        sample_spec: protocol::SampleSpec {
            format: protocol::SampleFormat::Float32Le,
            channels: 1,
            sample_rate,
        },
        channel_map: protocol::ChannelMap::mono(),
        buffer_attr: protocol::stream::BufferAttr {
            // Deliver about 10 ms at a time. Left alone, the server hands
            // recordings over in chunks of around two seconds. The buffer
            // limit stays at its default so nothing is thrown away.
            fragment_size: (sample_rate / 100).max(64) * frame_bytes,
            ..Default::default()
        },
        flags: protocol::stream::StreamFlags {
            // Lets the server shorten the device's own buffering to match.
            adjust_latency: true,
            ..Default::default()
        },
        props,
        ..Default::default()
    };

    // Room for two seconds, far more than the worker ever leaves unread.
    let (mut producer, samples) = RingBuffer::<f32>::new(sample_rate as usize * 2);
    let stream = block_on(client.create_record_stream(params, move |data: &[u8]| {
        // Runs on the client's I/O thread. If the worker falls behind,
        // samples are dropped rather than waited on.
        for bytes in data.chunks_exact(4) {
            let _ = producer.push(f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]));
        }
    }))
    .map_err(|e| format!("Could not listen to {device_name}: {e}"))?;

    // One round trip tells us both whether the server is still there and
    // whether the default output is still the one we opened.
    let health_client = client.clone();
    let health: Box<dyn FnMut() -> Health> = Box::new(move || match block_on(health_client.server_info()) {
        Err(_) => Health::Lost,
        Ok(info) => match (&following, &info.default_sink_name) {
            (Some(opened_on), Some(current)) if opened_on != current => Health::DefaultChanged,
            _ => Health::Ok,
        },
    });

    Ok(Open {
        samples,
        failed: Arc::new(AtomicBool::new(false)),
        device_name,
        sample_rate,
        channels,
        health,
        _keep: Box::new((client, stream)),
    })
}
