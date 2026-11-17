// The beat, where a performer can see it and put it right.
//
// Four boxes show the bar, lit as the beats go by. Beside them: the
// tempo, where it comes from, and the corrections a performer makes when
// the music's own beat is right in tempo but wrong in something else.

import { ChevronDown, ChevronLeft, ChevronRight, Lock, LockOpen, Music } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { useDismiss } from "./ui";
import { engine, useLive } from "../engine";

/** A sixteenth of a beat: small enough to place a beat by ear. */
const NUDGE = 1 / 16;

function sourceNote(source: string, coasting: boolean, bpm: number): string {
  if (source === "midi") return "from the MIDI clock";
  if (source === "tapped") return "tapped";
  if (source === "locked") return "locked";
  if (bpm <= 0) return "no beat found yet";
  return coasting ? "kept from memory" : "with the music";
}

/** The bar as four boxes and the tempo in figures, drawn every frame. */
function Bar() {
  const boxes = useRef<HTMLSpanElement>(null);
  const tempo = useRef<HTMLSpanElement>(null);
  const note = useRef<HTMLSpanElement>(null);
  useEffect(() => {
    let frame = 0;
    const tick = () => {
      const a = engine.audio;
      const source = engine.live.get().clock?.source ?? "music";
      const has = a.bpm > 0;
      if (boxes.current) {
        const at = has ? (a.bar_beat ?? 0) : -1;
        // The flash of the beat itself fades through the beat.
        const glow = Math.max(0.35, 1 - a.beat_phase * 1.4);
        [...boxes.current.children].forEach((box, i) => {
          const el = box as HTMLElement;
          el.classList.toggle("on", i === at);
          el.style.opacity = i === at ? String(glow) : "";
        });
      }
      if (tempo.current) tempo.current.textContent = has ? `${Math.round(a.bpm)} BPM` : "No tempo";
      if (note.current) {
        const coasting = a.coasting ?? false;
        note.current.textContent = sourceNote(source, coasting, a.bpm);
        note.current.classList.toggle("warn", coasting && source === "music");
      }
      frame = requestAnimationFrame(tick);
    };
    frame = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(frame);
  }, []);
  return (
    <>
      <span className="beat-boxes" ref={boxes} aria-hidden>
        <span className="first" />
        <span />
        <span />
        <span />
      </span>
      <span className="beat-text">
        <span className="beat-tempo" ref={tempo}>
          No tempo
        </span>
        <span className="beat-note" ref={note} />
      </span>
    </>
  );
}

/** Holds the beat at one tempo: the one heard now, or one typed. */
function LockTempo({ locked }: { locked: boolean }) {
  const [open, setOpen] = useState(false);
  const [typed, setTyped] = useState("");
  const ref = useDismiss(open, () => setOpen(false));
  const lock = (bpm: number | null) => {
    engine.send({ type: "lock_tempo", bpm });
    setOpen(false);
  };
  return (
    <span className="lock-tempo" ref={ref}>
      <button
        type="button"
        className={`tool ${locked ? "active" : ""}`}
        aria-pressed={locked}
        title={locked ? "The beat is held at one tempo. Click to follow the music again." : "Hold the beat at the tempo it has now"}
        onClick={() => (locked ? engine.send({ type: "auto_tempo" }) : lock(null))}
      >
        {locked ? <Lock size={13} /> : <LockOpen size={13} />} {locked ? "Locked" : "Lock"}
      </button>
      <button type="button" className="tool" aria-label="Lock to a tempo you type" aria-expanded={open} onClick={() => setOpen(!open)}>
        <ChevronDown size={13} />
      </button>
      {open && (
        <form
          className="popover lock-form"
          onSubmit={(e) => {
            e.preventDefault();
            const bpm = Number(typed);
            if (bpm >= 40 && bpm <= 240) lock(bpm);
          }}
        >
          <label className="field inline">
            <span>Lock at</span>
            <input
              type="text"
              inputMode="decimal"
              className="number"
              aria-label="Beats a minute to lock at"
              placeholder="128"
              value={typed}
              autoFocus
              onChange={(e) => setTyped(e.target.value)}
              onKeyDown={(e) => e.stopPropagation()}
            />
            <span className="hint">a minute</span>
          </label>
          <button type="submit" className="button small" disabled={!(Number(typed) >= 40 && Number(typed) <= 240)}>
            Lock
          </button>
        </form>
      )}
    </span>
  );
}

/**
 * The beat and everything that corrects it. `compact` leaves out the
 * words, for the row above the pads.
 */
export function BeatBar({ compact }: { compact?: boolean }) {
  const live = useLive();
  const clock = live.clock ?? { source: "music", scale: 1, nudge: 0, midi_bpm: null };
  const fromMusic = clock.source === "music";
  const corrected = clock.source === "tapped" || clock.source === "locked" || clock.scale !== 1 || Math.abs(clock.nudge) > 1e-6;
  const scale = (to: number) => engine.send({ type: "tempo_scale", scale: clock.scale === to ? 1 : to });
  return (
    <div className={`beat-bar ${compact ? "compact" : ""}`} role="group" aria-label="The beat">
      <Bar />
      <div className="tool-group">
        <button
          type="button"
          className={`tool ${clock.scale === 0.5 ? "active" : ""}`}
          aria-pressed={clock.scale === 0.5}
          disabled={!fromMusic}
          title="Count the beat at half the speed it is heard at"
          onClick={() => scale(0.5)}
        >
          ÷2
        </button>
        <button
          type="button"
          className={`tool ${clock.scale === 2 ? "active" : ""}`}
          aria-pressed={clock.scale === 2}
          disabled={!fromMusic}
          title="Count the beat at double the speed it is heard at"
          onClick={() => scale(2)}
        >
          ×2
        </button>
      </div>
      <div className="tool-group">
        <button type="button" className="tool" disabled={!fromMusic} aria-label="Beat a little earlier" title="Move the beat a little earlier" onClick={() => engine.send({ type: "nudge", beats: -NUDGE })}>
          <ChevronLeft size={14} />
        </button>
        <button type="button" className="tool" disabled={!fromMusic} aria-label="Beat a little later" title="Move the beat a little later" onClick={() => engine.send({ type: "nudge", beats: NUDGE })}>
          <ChevronRight size={14} />
        </button>
      </div>
      <div className="tool-group">
        <button type="button" className="tool" title="This is beat one: start the bar on the beat nearest now" onClick={() => engine.send({ type: "downbeat" })}>
          Beat 1
        </button>
        <button type="button" className="tool" title="Tap along. With the music's own beat, taps correct it; at another tempo, they replace it." onClick={() => engine.quiet({ type: "tap" })}>
          Tap
        </button>
        {(corrected || !compact) && (
          <button type="button" className="tool" disabled={!corrected} title="Go back to the beat as it is heard in the music" onClick={() => engine.send({ type: "auto_tempo" })}>
            Follow the music
          </button>
        )}
      </div>
      <div className="tool-group">
        <LockTempo locked={clock.source === "locked"} />
        <button
          type="button"
          className="tool"
          title="A new song has started: forget the last one's tempo and levels and listen afresh"
          onClick={() => engine.send({ type: "new_song" })}
        >
          <Music size={13} /> New song
        </button>
      </div>
      {!compact && Math.abs(clock.nudge) > 1e-6 && (
        <span className="hint">
          Moved {Math.abs(Math.round(clock.nudge * 16))} sixteenth{Math.abs(Math.round(clock.nudge * 16)) === 1 ? "" : "s"} {clock.nudge > 0 ? "later" : "earlier"}.
        </span>
      )}
    </div>
  );
}
