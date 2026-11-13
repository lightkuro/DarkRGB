// A slider that can follow something: the music, a slow wave, or a fader
// in the Live view. The slider stays where it is put; the link moves the
// setting from there, by a share of its range, as the source rises.

import { Activity, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { engine, perFrame, type AudioFeatures, type Layer, type Link, type Source } from "../engine";
import { Slider } from "./ui";

/** The name the engine uses for an effect's strength in a link. */
export const STRENGTH = "opacity";
/** The name a link uses for the angle a layer's pattern is turned to. */
export const ANGLE = "angle";
/** The name a link uses for the hue a layer's colours are shifted by. */
export const HUE = "hue";

const SOURCES: { id: string; name: string; source: Source }[] = [
  { id: "level", name: "Loudness", source: { type: "level" } },
  { id: "bass", name: "Bass", source: { type: "bass" } },
  { id: "mid", name: "Mids", source: { type: "mid" } },
  { id: "treble", name: "Highs", source: { type: "treble" } },
  { id: "beat", name: "The beat", source: { type: "beat" } },
  { id: "hit", name: "Hits", source: { type: "hit" } },
  { id: "energy", name: "The song's energy", source: { type: "energy" } },
  { id: "wave", name: "A slow wave", source: { type: "wave", beats: 8 } },
  { id: "ramp", name: "A climb, then back to the start", source: { type: "ramp", beats: 4 } },
  { id: "square", name: "On, then off", source: { type: "square", beats: 2 } },
  { id: "random", name: "A new level by chance", source: { type: "random", beats: 1 } },
  { id: "fader1", name: "Fader 1", source: { type: "fader", n: 1 } },
  { id: "fader2", name: "Fader 2", source: { type: "fader", n: 2 } },
  { id: "fader3", name: "Fader 3", source: { type: "fader", n: 3 } },
  { id: "fader4", name: "Fader 4", source: { type: "fader", n: 4 } },
];

const idOf = (source: Source): string => (source.type === "fader" ? `fader${source.n}` : source.type);

/** A few words for what a link follows. */
export function sourceName(source: Source): string {
  return SOURCES.find((s) => s.id === idOf(source))?.name ?? source.type;
}

/** Where a source is right now, 0 to 1, as near as the UI can tell. */
function sourceValue(source: Source, audio: AudioFeatures, faders: number[]): number {
  switch (source.type) {
    case "level":
      return audio.level;
    case "bass":
      return audio.bass;
    case "mid":
      return audio.mid;
    case "treble":
      return audio.treble;
    case "beat":
      return audio.beat;
    case "hit":
      return audio.onset;
    case "energy":
      return audio.active ? audio.intensity : 0;
    case "wave":
    case "ramp":
    case "square":
    case "random": {
      const turns =
        audio.bpm > 0 ? (audio.beat_count + audio.beat_phase) / source.beats : (performance.now() / 500) / source.beats;
      const within = turns - Math.floor(turns);
      if (source.type === "ramp") return within;
      if (source.type === "square") return within < 0.5 ? 1 : 0;
      // The engine draws its own lots; the mark only shows that it jumps.
      if (source.type === "random") return (Math.sin(Math.floor(turns) * 12.9898) * 43758.5453) % 1 < 0 ? 0.3 : 0.8;
      return 0.5 - 0.5 * Math.cos(turns * Math.PI * 2);
    }
    case "fader":
      return faders[source.n - 1] ?? 0;
  }
}

const senders = new Map<string, (amount: number) => void>();

/** A mark under the slider that shows where the setting is at this moment. */
function LiveMark({ link, at }: { link: Link; at: number }) {
  const mark = useRef<HTMLSpanElement>(null);
  useEffect(() => {
    let frame = 0;
    const tick = () => {
      const faders = engine.show.get()?.live.faders ?? [];
      const moved = at + sourceValue(link.source, engine.audio, faders) * link.amount;
      if (mark.current) mark.current.style.left = `${Math.min(1, Math.max(0, moved)) * 100}%`;
      frame = requestAnimationFrame(tick);
    };
    frame = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(frame);
  }, [link, at]);
  return (
    <span className="link-track" aria-hidden>
      <span className="link-mark" ref={mark} />
    </span>
  );
}

function Editor({ layer, param, label, link }: { layer: Layer; param: string; label: string; link: Link | undefined }) {
  const set = (source: Source, amount: number) =>
    engine.send({ type: "set_link", layer: layer.id, param, source, amount });
  let sendAmount = senders.get(`${layer.id}:${param}`);
  if (!sendAmount) {
    sendAmount = perFrame((amount: number) => {
      const now = engine.show.get()?.layers.find((l) => l.id === layer.id)?.links?.find((l) => l.param === param);
      if (now) engine.quiet({ type: "set_link", layer: layer.id, param, source: now.source, amount });
    });
    senders.set(`${layer.id}:${param}`, sendAmount);
  }
  // Smoothed, a setting rises quickly with its source and settles back slowly.
  let sendSmooth = senders.get(`${layer.id}:${param}:smooth`);
  if (!sendSmooth) {
    sendSmooth = perFrame((smooth: number) => {
      const now = engine.show.get()?.layers.find((l) => l.id === layer.id)?.links?.find((l) => l.param === param);
      if (now) engine.quiet({ type: "set_link", layer: layer.id, param, source: now.source, amount: now.amount, smooth });
    });
    senders.set(`${layer.id}:${param}:smooth`, sendSmooth);
  }
  return (
    <div className="link-editor" role="group" aria-label={`What ${label} follows`}>
      <label className="field inline">
        <span>Follows</span>
        <select
          value={link ? idOf(link.source) : ""}
          onChange={(e) => {
            const picked = SOURCES.find((s) => s.id === e.target.value);
            if (!picked) return engine.send({ type: "remove_link", layer: layer.id, param });
            // Something timed keeps its length when another timed thing is picked.
            const source =
              "beats" in picked.source && link && "beats" in link.source
                ? { ...picked.source, beats: link.source.beats }
                : picked.source;
            set(source, link?.amount ?? 0.5);
          }}
        >
          <option value="">Nothing</option>
          {SOURCES.map((s) => (
            <option key={s.id} value={s.id}>
              {s.name}
            </option>
          ))}
        </select>
      </label>
      {link && "beats" in link.source && (
        <div className="segmented" role="radiogroup" aria-label="How long one round takes">
          {(link.source.type === "wave" ? [2, 4, 8, 16, 32] : [0.5, 1, 2, 4, 8]).map((beats) => (
            <button
              key={beats}
              type="button"
              role="radio"
              aria-checked={"beats" in link.source && link.source.beats === beats}
              className={"beats" in link.source && link.source.beats === beats ? "selected" : ""}
              onClick={() => "beats" in link.source && set({ ...link.source, beats }, link.amount)}
            >
              {beats === 0.5 ? "½ beat" : beats === 1 ? "1 beat" : `${beats} beats`}
            </button>
          ))}
        </div>
      )}
      {link && (
        <>
          <Slider
            label="By how much"
            min={-1}
            max={1}
            step={0.01}
            value={link.amount}
            format={(v) => `${v > 0 ? "+" : ""}${Math.round(v * 100)}% of its range`}
            onChange={sendAmount}
          />
          <Slider
            label="Smoothing"
            min={0}
            max={1}
            step={0.01}
            value={link.smooth ?? 0}
            format={(v) => (v === 0 ? "None" : v < 0.35 ? "A little" : v < 0.7 ? "Smooth" : "Very smooth")}
            onChange={(smooth) => sendSmooth!(smooth)}
          />
          <p className="hint">
            {label} moves from where its slider is set, {link.amount < 0 ? "down" : "up"} as{" "}
            {sourceName(link.source).toLowerCase()} rises.
            {link.source.type === "fader" ? " The fader is in the Live view." : ""}
          </p>
        </>
      )}
    </div>
  );
}

/**
 * A slider for one setting of an effect, with a button to make it follow
 * something. `offer` is false in the simple layout, where the button only
 * shows on a slider that follows something already.
 */
export function LinkedSlider({
  layer,
  param,
  label,
  min,
  max,
  step,
  value,
  format,
  onChange,
  offer,
}: {
  layer: Layer;
  param: string;
  label: string;
  min: number;
  max: number;
  step?: number;
  value: number;
  format?: (value: number) => string;
  onChange: (value: number) => void;
  offer: boolean;
}) {
  const link = layer.links?.find((l) => l.param === param);
  const [open, setOpen] = useState(false);
  if (!offer && !link) {
    return <Slider label={label} min={min} max={max} step={step} value={value} format={format} onChange={onChange} />;
  }
  const button = (
    <button
      type="button"
      className={`link-button ${link ? "linked" : ""}`}
      aria-expanded={open}
      aria-label={link ? `${label} follows ${sourceName(link.source)}` : `Make ${label} follow something`}
      title={
        link
          ? `Follows ${sourceName(link.source).toLowerCase()}. Click to change.`
          : "Make this follow the music, a slow wave or a fader"
      }
      onClick={() => setOpen(!open)}
    >
      {open ? <X size={12} /> : <Activity size={12} />}
      {link && !open && <span>{sourceName(link.source)}</span>}
    </button>
  );
  return (
    <div className={`linked-slider ${link ? "has-link" : ""}`}>
      <Slider
        label={label}
        min={min}
        max={max}
        step={step}
        value={value}
        format={format}
        onChange={onChange}
        extra={button}
      />
      {link && <LiveMark link={link} at={(value - min) / (max - min)} />}
      {open && <Editor layer={layer} param={param} label={label} link={link} />}
    </div>
  );
}
