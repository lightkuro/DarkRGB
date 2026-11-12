// MIDI controllers: tying a pad on the controller to a pad of the show,
// and a knob or fader to something it moves. Both are taught by touching
// the control ("learn"). The engine does the listening.

import { Piano, X } from "lucide-react";
import { useEffect, useState } from "react";
import { engine, useShow, useStatus, type Binding, type KnobTarget, type MidiNote } from "../engine";

const NOTES = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];

/** What a note is called: C4 is note 60. */
export function noteName(note: MidiNote): string {
  return `${NOTES[note.note % 12]}${Math.floor(note.note / 12) - 1} · channel ${note.channel + 1}`;
}

const sameTarget = (a: KnobTarget, b: KnobTarget) =>
  a.type === b.type && (a.type !== "fader" || (b.type === "fader" && a.n === b.n));

/** Whether any controller is being listened to. */
export function useMidiPorts(): string[] {
  return useStatus()?.midi?.ports ?? [];
}

/** One row of a pad's settings: the pad on a controller that plays it. */
export function MidiPad({ binding }: { binding: Binding }) {
  const midi = useStatus()?.midi;
  const [waiting, setWaiting] = useState(false);
  // Taught: the pad now has its note, so the wait is over.
  useEffect(() => setWaiting(false), [binding.midi?.note, binding.midi?.channel]);
  // Leaving the row cancels the wait.
  useEffect(
    () => () => {
      engine.quiet({ type: "midi_learn", target: null });
    },
    [],
  );
  if (!midi?.available) return null;
  const learn = (on: boolean) => {
    setWaiting(on);
    engine.send({ type: "midi_learn", target: on ? { type: "pad", binding: binding.id } : null });
  };
  return (
    <div className="field key-assign">
      <div className="key-assign-row">
        <span className="key-assign-label">MIDI pad</span>
        {waiting ? (
          <button type="button" className="button small picking" onClick={() => learn(false)}>
            Press a pad on the controller…
          </button>
        ) : binding.midi ? (
          <>
            <button type="button" className="key-change" title="Press to pick a different pad" onClick={() => learn(true)}>
              <kbd className="keycap">{noteName(binding.midi)}</kbd>
            </button>
            <button
              type="button"
              className="icon-button"
              aria-label="Remove the MIDI pad"
              title="Remove the MIDI pad"
              onClick={() => engine.send({ type: "set_pad_midi", id: binding.id, midi: null })}
            >
              <X size={14} />
            </button>
          </>
        ) : (
          <button type="button" className="button small quiet" onClick={() => learn(true)}>
            <Piano size={14} /> Learn
          </button>
        )}
      </div>
      {midi.ports.length === 0 && <p className="hint">No MIDI controller is plugged in.</p>}
    </div>
  );
}

const KNOBS: { name: string; target: KnobTarget }[] = [
  { name: "Brightness", target: { type: "brightness" } },
  { name: "Speed", target: { type: "speed" } },
  { name: "Sound sensitivity", target: { type: "sensitivity" } },
  { name: "Fader 1", target: { type: "fader", n: 1 } },
  { name: "Fader 2", target: { type: "fader", n: 2 } },
  { name: "Fader 3", target: { type: "fader", n: 3 } },
  { name: "Fader 4", target: { type: "fader", n: 4 } },
];

function KnobRow({ name, target }: { name: string; target: KnobTarget }) {
  const show = useShow();
  const knob = show?.live.knobs?.find((k) => sameTarget(k.target, target));
  const [waiting, setWaiting] = useState(false);
  useEffect(() => setWaiting(false), [knob?.cc, knob?.channel]);
  const learn = (on: boolean) => {
    setWaiting(on);
    engine.send({ type: "midi_learn", target: on ? { type: "knob", target } : null });
  };
  return (
    <div className="field key-assign">
      <div className="key-assign-row">
        <span className="key-assign-label">{name}</span>
        {waiting ? (
          <button type="button" className="button small picking" onClick={() => learn(false)}>
            Turn a knob or move a fader…
          </button>
        ) : knob ? (
          <>
            <button type="button" className="key-change" title="Press to pick a different knob" onClick={() => learn(true)}>
              <kbd className="keycap">
                Knob {knob.cc} · channel {knob.channel + 1}
              </kbd>
            </button>
            <button
              type="button"
              className="icon-button"
              aria-label={`Remove the knob for ${name}`}
              title="Remove this knob"
              onClick={() => engine.send({ type: "remove_knob", target })}
            >
              <X size={14} />
            </button>
          </>
        ) : (
          <button type="button" className="button small quiet" onClick={() => learn(true)}>
            <Piano size={14} /> Learn
          </button>
        )}
      </div>
    </div>
  );
}

/** The MIDI part of the settings: what is heard, and the knobs. */
export function MidiSettings() {
  const midi = useStatus()?.midi;
  useEffect(
    () => () => {
      engine.quiet({ type: "midi_learn", target: null });
    },
    [],
  );
  if (!midi) return null;
  if (!midi.available) {
    return (
      <>
        <h3>MIDI controller</h3>
        <p className="hint">This computer has no MIDI. Pads and keys work without it.</p>
      </>
    );
  }
  return (
    <>
      <h3>MIDI controller</h3>
      <p className="hint">
        {midi.ports.length > 0
          ? `Listening to ${midi.ports.join(", ")}.`
          : "No controller is plugged in. One is picked up within a couple of seconds of being connected."}{" "}
        {midi.last ? `Last heard: ${midi.last}.` : ""}
      </p>
      <p className="hint">
        Pads on the controller are tied to the show's pads in the Live view, under Customize. Knobs and faders are
        tied here: choose Learn, then move the one you want. Speed and sensitivity are normal at the middle of a
        knob's travel.
      </p>
      {KNOBS.map((knob) => (
        <KnobRow key={knob.name} name={knob.name} target={knob.target} />
      ))}
      <p className="callout note">
        MIDI is new and has not yet been tried with a real controller. If yours works, or does not, a report would
        help a great deal.
      </p>
    </>
  );
}
