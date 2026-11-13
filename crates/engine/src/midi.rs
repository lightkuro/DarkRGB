//! MIDI controllers: pads and keys play the show's pads, knobs and faders
//! move its sliders.
//!
//! The engine listens itself, so this works in the app window on every
//! system, including Linux, where the web view has no MIDI of its own.
//! What a message means is worked out here as plain logic; the listening
//! thread only passes bytes along.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use darkrgb_core::show::{KnobTarget, MidiNote};
use midir::{Ignore, MidiInput, MidiInputConnection};
use serde::{Deserialize, Serialize};

use crate::app::Shared;

/// What a controller said, as far as this program cares.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MidiEvent {
    /// A pad or key went down (true) or up.
    Note(MidiNote, bool),
    /// A knob or fader moved; where it stands, 0..1.
    Control { channel: u8, cc: u8, value: f32 },
    /// One tick of a clock another machine is sending: 24 to the beat.
    Clock,
    /// The sender of the clock started from the top.
    ClockStart,
}

/// Reads one MIDI message. Anything other than notes, controllers and
/// clock (pitch bend, other system messages) is ignored.
pub fn parse(bytes: &[u8]) -> Option<MidiEvent> {
    let (&status, data) = bytes.split_first()?;
    // Clock messages are one byte long. "Continue" picks up where the
    // sender stopped, which the tick count already does by itself.
    match status {
        0xF8 => return Some(MidiEvent::Clock),
        0xFA => return Some(MidiEvent::ClockStart),
        _ => {}
    }
    let (kind, channel) = (status & 0xF0, status & 0x0F);
    let (&a, &b) = (data.first()?, data.get(1)?);
    if a > 127 || b > 127 {
        return None;
    }
    match kind {
        // A note-on with no velocity is how many controllers say "off".
        0x90 => Some(MidiEvent::Note(MidiNote { channel, note: a }, b > 0)),
        0x80 => Some(MidiEvent::Note(MidiNote { channel, note: a }, false)),
        0xB0 => Some(MidiEvent::Control {
            channel,
            cc: a,
            value: b as f32 / 127.0,
        }),
        _ => None,
    }
}

/// What the next message from a controller should be tied to.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LearnTarget {
    /// The next pad pressed plays this pad of the show.
    Pad { binding: u32 },
    /// The next knob or fader moved moves this.
    Knob { target: KnobTarget },
}

/// What the UI is told about MIDI.
#[derive(Clone, Debug, Default, Serialize)]
pub struct MidiStatus {
    /// False when this computer has no MIDI at all.
    pub available: bool,
    /// The controllers being listened to.
    pub ports: Vec<String>,
    /// Waiting for a pad or a knob to be touched.
    pub learning: Option<LearnTarget>,
    /// The last thing a controller did, in words, so the user can see
    /// that it is heard.
    pub last: Option<String>,
}

/// What a note is called: C4 is note 60.
pub fn note_name(note: u8) -> String {
    const NAMES: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
    format!("{}{}", NAMES[(note % 12) as usize], note as i32 / 12 - 1)
}

/// Where a knob's position puts a setting that runs from a quarter to
/// four times normal: half way is normal, and each quarter turn doubles.
pub fn doubling(value: f32) -> f32 {
    2f32.powf((value.clamp(0.0, 1.0) - 0.5) * 4.0)
}

/// Opens the system's MIDI. On Linux the sound library complains on the
/// terminal every time it is asked for MIDI that is not there, so the
/// question is only put when the system has MIDI at all.
pub fn open_input() -> Result<MidiInput, String> {
    #[cfg(target_os = "linux")]
    if !std::path::Path::new("/dev/snd/seq").exists() {
        return Err("this system has no MIDI sequencer".into());
    }
    MidiInput::new("darkRGB").map_err(|e| e.to_string())
}

/// Listens to every controller plugged in, until told to stop. New ones
/// are picked up within a couple of seconds, and ones that go away are
/// let go of.
pub fn listen(shared: Arc<Shared>, stop: Arc<AtomicBool>) {
    let mut open: HashMap<String, MidiInputConnection<()>> = HashMap::new();
    let mut warned = false;
    while !stop.load(Ordering::Relaxed) {
        match open_input() {
            Ok(probe) => {
                let ports = probe.ports();
                let names: Vec<String> = ports
                    .iter()
                    .filter_map(|p| probe.port_name(p).ok())
                    // The system's own loop-back port is not a controller.
                    .filter(|name| !name.contains("Midi Through") && !name.contains("darkRGB"))
                    .collect();
                open.retain(|name, _| names.contains(name));
                for port in &ports {
                    let Ok(name) = probe.port_name(port) else { continue };
                    if open.contains_key(&name) || !names.contains(&name) {
                        continue;
                    }
                    // Each connection uses up one input object.
                    let Ok(mut input) = open_input() else {
                        continue;
                    };
                    input.ignore(Ignore::All);
                    let listener = shared.clone();
                    match input.connect(port, "darkRGB in", move |_, bytes, _| listener.midi_message(bytes), ()) {
                        Ok(connection) => {
                            log::info!("listening to the MIDI controller {name}");
                            open.insert(name, connection);
                        }
                        Err(e) => log::warn!("could not listen to the MIDI controller {name}: {e}"),
                    }
                }
                let mut status = shared.midi.lock().unwrap();
                status.available = true;
                let mut listening: Vec<String> = open.keys().cloned().collect();
                listening.sort();
                status.ports = listening;
            }
            Err(e) => {
                if !warned {
                    log::info!("MIDI is not available on this computer: {e}");
                    warned = true;
                }
                let mut status = shared.midi.lock().unwrap();
                status.available = false;
                status.ports.clear();
            }
        }
        // Look again soon, in small steps so that stopping is prompt.
        for _ in 0..20 {
            if stop.load(Ordering::Relaxed) {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notes_and_controllers_are_read_and_the_rest_is_ignored() {
        let note = |channel, note| MidiNote { channel, note };
        assert_eq!(parse(&[0x90, 60, 100]), Some(MidiEvent::Note(note(0, 60), true)));
        assert_eq!(parse(&[0x93, 36, 1]), Some(MidiEvent::Note(note(3, 36), true)));
        assert_eq!(
            parse(&[0x90, 60, 0]),
            Some(MidiEvent::Note(note(0, 60), false)),
            "no velocity is off"
        );
        assert_eq!(parse(&[0x8F, 60, 64]), Some(MidiEvent::Note(note(15, 60), false)));
        assert_eq!(
            parse(&[0xB1, 7, 127]),
            Some(MidiEvent::Control {
                channel: 1,
                cc: 7,
                value: 1.0
            })
        );
        assert_eq!(
            parse(&[0xB0, 1, 0]),
            Some(MidiEvent::Control {
                channel: 0,
                cc: 1,
                value: 0.0
            })
        );
        // A clock another machine sends, and its start.
        assert_eq!(parse(&[0xF8]), Some(MidiEvent::Clock));
        assert_eq!(parse(&[0xFA]), Some(MidiEvent::ClockStart));
        // Stop and continue, pitch bend, short and malformed messages.
        for bytes in [
            &[0xFC][..],
            &[0xFB],
            &[0xE0, 0, 64],
            &[0x90, 60],
            &[],
            &[0x90, 200, 10],
            &[0x90, 60, 200],
        ] {
            assert_eq!(parse(bytes), None, "{bytes:?}");
        }
    }

    #[test]
    fn notes_have_names_and_knobs_double_by_the_quarter_turn() {
        assert_eq!(note_name(60), "C4");
        assert_eq!(note_name(36), "C2");
        assert_eq!(note_name(61), "C#4");
        assert_eq!(note_name(0), "C-1");
        assert_eq!(note_name(127), "G9");
        assert!((doubling(0.5) - 1.0).abs() < 1e-6);
        assert!((doubling(0.0) - 0.25).abs() < 1e-6);
        assert!((doubling(1.0) - 4.0).abs() < 1e-6);
        assert!((doubling(0.75) - 2.0).abs() < 1e-6);
        assert!((doubling(7.0) - 4.0).abs() < 1e-6);
    }
}
