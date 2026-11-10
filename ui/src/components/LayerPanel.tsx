// The stack of effects. Each one is a single row until it is opened, so
// many fit on screen. Controls are built from what the engine says each
// effect exposes; a new effect needs no UI code.
//
// Beginners see the few settings that matter. The Advanced switch at the
// top reveals the rest.

import {
  AudioLines,
  ChevronDown,
  Copy,
  Eye,
  EyeOff,
  GripVertical,
  Plus,
  RotateCcw,
  SlidersHorizontal,
  Trash2,
} from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import {
  advancedPref,
  engine,
  perFrame,
  useAdvanced,
  useHello,
  useShow,
  type Band,
  type Blend,
  type EffectInfo,
  type Fixture,
  type Hello,
  type Layer,
  type Palette,
  type ParamSpec,
  type ParamValue,
} from "../engine";
import { ConfirmButton, IconButton, Slider, Switch, useDismiss } from "./ui";

const BLENDS: { id: Blend; name: string; hint: string }[] = [
  { id: "normal", name: "Replace", hint: "Covers the effects below" },
  { id: "add", name: "Add", hint: "Adds its light to the effects below" },
  { id: "lighten", name: "Lighten", hint: "Shows whichever is brighter" },
  { id: "multiply", name: "Mask", hint: "Darkens the effects below; black hides them" },
];

const BANDS: { type: Exclude<Band["type"], "range">; name: string }[] = [
  { type: "all", name: "Everything" },
  { type: "bass", name: "Bass" },
  { type: "mid", name: "Mids" },
  { type: "treble", name: "Highs" },
];

const senders = new Map<string, (value: number) => void>();
/** One rate-limited sender per slider, so dragging sends once a frame. */
function numberSender(key: string, make: (value: number) => Parameters<typeof engine.quiet>[0]) {
  let send = senders.get(key);
  if (!send) {
    send = perFrame((value: number) => engine.quiet(make(value)));
    senders.set(key, send);
  }
  return send;
}

function gradient(palette: Palette): string {
  const stops = palette.cyclic ? [...palette.stops, palette.stops[0] ?? "#000"] : palette.stops;
  return `linear-gradient(90deg, ${stops.join(", ")})`;
}

/** A small picture of a layer's colours, for its collapsed row. */
function swatch(layer: Layer, effect: EffectInfo | undefined, hello: Hello): string {
  const value = (spec: ParamSpec) => String(layer.params[spec.id] ?? spec.default);
  const palette = effect?.params.find((p) => p.kind === "palette");
  if (palette) {
    const found = hello.palettes.find((p) => p.id === value(palette));
    if (found) return gradient(found);
  }
  const colours = effect?.params.filter((p) => p.kind === "color").map(value) ?? [];
  if (colours.length === 1) return colours[0]!;
  if (colours.length > 1) return `linear-gradient(90deg, ${colours.join(", ")})`;
  return "var(--raised-2)";
}

function hz(value: number): string {
  return value >= 1000 ? `${(value / 1000).toFixed(value >= 10000 ? 0 : 1)} kHz` : `${Math.round(value)} Hz`;
}

/** The part of the spectrum a band choice covers, as positions 0..1. */
function bandSpan(band: Band, hello: Hello): [number, number] {
  const last = hello.band_hz.length - 2;
  const group = (index: number): [number, number] => {
    const [first, end] = hello.band_groups[index] ?? [0, last + 1];
    return [first / last, (end - 1) / last];
  };
  switch (band.type) {
    case "all":
      return [0, 1];
    case "bass":
      return group(0);
    case "mid":
      return group(1);
    case "treble":
      return group(2);
    case "range":
      return [band.low, band.high];
  }
}

function bandName(band: Band, hello: Hello): string {
  if (band.type !== "range") return BANDS.find((b) => b.type === band.type)!.name;
  const last = hello.band_hz.length - 2;
  const lowHz = hello.band_hz[Math.round(band.low * last)] ?? 0;
  const highHz = hello.band_hz[Math.round(band.high * last) + 1] ?? 0;
  return `${hz(lowHz)} to ${hz(highHz)}`;
}

function PalettePicker({ value, onChange, label }: { value: string; onChange: (id: string) => void; label: string }) {
  const palettes = useHello()?.palettes ?? [];
  const [open, setOpen] = useState(false);
  const close = useCallback(() => setOpen(false), []);
  const ref = useDismiss(open, close);
  const current = palettes.find((p) => p.id === value) ?? palettes[0];
  return (
    <div className="field palette-field" ref={ref}>
      <span className="field-head">
        <span>{label}</span>
        <output>{current?.name}</output>
      </span>
      <button
        type="button"
        className="palette-current"
        aria-label={`${label}: ${current?.name ?? ""}. Change`}
        aria-expanded={open}
        style={{ background: current ? gradient(current) : undefined }}
        onClick={() => setOpen(!open)}
      />
      {open && (
        <div className="popover palette-list">
          {palettes.map((p) => (
            <button
              key={p.id}
              type="button"
              className={`palette-option ${p.id === value ? "selected" : ""}`}
              onClick={() => {
                onChange(p.id);
                setOpen(false);
              }}
            >
              <span className="swatch" style={{ background: gradient(p) }} />
              {p.name}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

function ParamControl({ layer, spec }: { layer: Layer; spec: ParamSpec }) {
  const value: ParamValue = layer.params[spec.id] ?? spec.default;
  const set = (next: ParamValue) => engine.send({ type: "set_param", layer: layer.id, param: spec.id, value: next });

  switch (spec.kind) {
    case "float":
      return (
        <Slider
          label={spec.label}
          min={spec.min ?? 0}
          max={spec.max ?? 1}
          step={spec.step ?? 0.01}
          value={typeof value === "number" ? value : Number(spec.default)}
          onChange={numberSender(`${layer.id}:${spec.id}`, (v) => ({
            type: "set_param",
            layer: layer.id,
            param: spec.id,
            value: v,
          }))}
        />
      );
    case "bool":
      return (
        <div className="field inline">
          <span>{spec.label}</span>
          <Switch checked={value === true} label={spec.label} onChange={set} />
        </div>
      );
    case "color":
      return (
        <label className="field inline">
          <span>{spec.label}</span>
          <span className="color-input">
            <input
              type="color"
              value={typeof value === "string" ? value : String(spec.default)}
              // Colour pickers fire continuously while dragging.
              onChange={(e) => engine.quiet({ type: "set_param", layer: layer.id, param: spec.id, value: e.target.value })}
            />
            <code>{String(value)}</code>
          </span>
        </label>
      );
    case "choice":
      return (
        <div className="field">
          <span className="field-head">
            <span>{spec.label}</span>
          </span>
          <div className="segmented" role="radiogroup" aria-label={spec.label}>
            {(spec.options ?? []).map((option) => (
              <button
                key={option}
                type="button"
                role="radio"
                aria-checked={value === option}
                className={value === option ? "selected" : ""}
                onClick={() => set(option)}
              >
                {option}
              </button>
            ))}
          </div>
        </div>
      );
    case "palette":
      return <PalettePicker label={spec.label} value={String(value)} onChange={set} />;
  }
}

/** Effects grouped the way the engine groups them. */
export function EffectPicker({ onPick, current }: { onPick: (id: string) => void; current?: string }) {
  const effects = useHello()?.effects ?? [];
  const categories = [...new Set(effects.map((e) => e.category))];
  return (
    <div className="effect-picker">
      {categories.map((category) => (
        <div key={category} className="effect-group">
          <h3>{category}</h3>
          <div className="effect-grid">
            {effects
              .filter((e) => e.category === category)
              .map((effect) => (
                <button
                  key={effect.id}
                  type="button"
                  className={`effect-card ${effect.id === current ? "selected" : ""}`}
                  onClick={() => onPick(effect.id)}
                >
                  <span className="effect-name">{effect.name}</span>
                  <span className="effect-description">{effect.description}</span>
                </button>
              ))}
          </div>
        </div>
      ))}
    </div>
  );
}

function Targets({ layer, fixtures }: { layer: Layer; fixtures: Fixture[] }) {
  if (fixtures.length < 2) return null;
  const all = layer.targets.length === 0;
  const toggle = (id: number) => {
    const next = layer.targets.includes(id) ? layer.targets.filter((t) => t !== id) : [...layer.targets, id];
    // Selecting every fixture is the same as "all lights".
    engine.send({ type: "update_layer", id: layer.id, targets: next.length === fixtures.length ? [] : next });
  };
  return (
    <div className="field">
      <span className="field-head">
        <span>Plays on</span>
      </span>
      <div className="chips">
        <button
          type="button"
          className={`chip ${all ? "selected" : ""}`}
          aria-pressed={all}
          onClick={() => engine.send({ type: "update_layer", id: layer.id, targets: [] })}
        >
          All lights
        </button>
        {fixtures.map((f) => (
          <button
            key={f.id}
            type="button"
            className={`chip ${layer.targets.includes(f.id) ? "selected" : ""}`}
            aria-pressed={layer.targets.includes(f.id)}
            onClick={() => toggle(f.id)}
          >
            {f.name}
          </button>
        ))}
      </div>
    </div>
  );
}

/**
 * The live spectrum with the part this layer hears lit up. In advanced
 * mode the two edges can be dragged to pick any range.
 */
function BandRange({ layer, hello, editable }: { layer: Layer; hello: Hello; editable: boolean }) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const wrapRef = useRef<HTMLDivElement>(null);
  const [low, high] = bandSpan(layer.sound.band, hello);
  // What is under the pointer while an edge is being dragged.
  const [live, setLive] = useState<[number, number] | null>(null);
  const span = live ?? [low, high];
  const spanRef = useRef(span);
  spanRef.current = span;
  useEffect(() => {
    if (live && Math.abs(live[0] - low) < 0.02 && Math.abs(live[1] - high) < 0.02) setLive(null);
  }, [live, low, high]);

  useEffect(() => {
    const canvas = canvasRef.current;
    const ctx = canvas?.getContext("2d");
    if (!canvas || !ctx) return;
    let frame = 0;
    const draw = () => {
      const dpr = window.devicePixelRatio || 1;
      const [w, h] = [canvas.clientWidth, canvas.clientHeight];
      if (canvas.width !== Math.round(w * dpr) || canvas.height !== Math.round(h * dpr)) {
        canvas.width = Math.round(w * dpr);
        canvas.height = Math.round(h * dpr);
      }
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      ctx.clearRect(0, 0, w, h);
      const bands = engine.audio.bands;
      const n = bands.length;
      const [lo, hi] = spanRef.current;
      for (let i = 0; i < n; i++) {
        const t = i / (n - 1);
        const inside = t >= lo - 0.001 && t <= hi + 0.001;
        const v = bands[i] ?? 0;
        const height = Math.max(2, v * (h - 2));
        ctx.fillStyle = inside ? `hsl(${38 + t * 135} 90% 55%)` : "#2a2f3a";
        ctx.fillRect((i * w) / n, h - height, Math.max(1, w / n - 1), height);
      }
      frame = requestAnimationFrame(draw);
    };
    frame = requestAnimationFrame(draw);
    return () => cancelAnimationFrame(frame);
  }, []);

  const sendRange = numberSenderPair(layer.id);
  const drag = (edge: 0 | 1) => (e: React.PointerEvent) => {
    e.preventDefault();
    const wrap = wrapRef.current!;
    const steps = hello.band_hz.length - 2;
    const move = (ev: PointerEvent) => {
      const rect = wrap.getBoundingClientRect();
      // Snap to whole bands.
      const t = Math.round(Math.min(Math.max((ev.clientX - rect.left) / rect.width, 0), 1) * steps) / steps;
      const current = spanRef.current;
      const next: [number, number] = edge === 0 ? [Math.min(t, current[1]), current[1]] : [current[0], Math.max(t, current[0])];
      setLive(next);
      sendRange(next);
    };
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  };

  const label = live ? bandName({ type: "range", low: live[0], high: live[1] }, hello) : bandName(layer.sound.band, hello);
  return (
    <div className="band-range">
      <div className="band-canvas" ref={wrapRef}>
        <canvas ref={canvasRef} aria-hidden />
        {editable && (
          <>
            <button
              type="button"
              className="band-handle"
              style={{ left: `${span[0] * 100}%` }}
              aria-label="Lowest frequency this effect hears"
              title="Drag to set the lowest frequency"
              onPointerDown={drag(0)}
            />
            <button
              type="button"
              className="band-handle"
              style={{ left: `${span[1] * 100}%` }}
              aria-label="Highest frequency this effect hears"
              title="Drag to set the highest frequency"
              onPointerDown={drag(1)}
            />
          </>
        )}
      </div>
      {(editable || layer.sound.band.type === "range") && <p className="hint">Hears {label}</p>}
    </div>
  );
}

const rangeSenders = new Map<number, (range: [number, number]) => void>();
function numberSenderPair(layer: number) {
  let send = rangeSenders.get(layer);
  if (!send) {
    send = perFrame(([low, high]: [number, number]) =>
      engine.quiet({ type: "set_sound", layer, band: { type: "range", low, high } }),
    );
    rangeSenders.set(layer, send);
  }
  return send;
}

function SoundControls({ layer, hello, advanced }: { layer: Layer; hello: Hello; advanced: boolean }) {
  const band = layer.sound.band;
  return (
    <div className="field">
      <span className="field-head">
        <span>Reacts to</span>
        {band.type === "range" && !advanced && <output>{bandName(band, hello)}</output>}
      </span>
      <div className="segmented" role="radiogroup" aria-label="Reacts to">
        {BANDS.map((b) => (
          <button
            key={b.type}
            type="button"
            role="radio"
            aria-checked={band.type === b.type}
            className={band.type === b.type ? "selected" : ""}
            onClick={() => engine.send({ type: "set_sound", layer: layer.id, band: { type: b.type } })}
          >
            {b.name}
          </button>
        ))}
      </div>
      <BandRange layer={layer} hello={hello} editable={advanced} />
    </div>
  );
}

function LayerName({ layer }: { layer: Layer }) {
  const [draft, setDraft] = useState<string | null>(null);
  const commit = () => {
    if (draft !== null && draft.trim() !== (layer.name ?? "")) {
      engine.send({ type: "rename_layer", id: layer.id, name: draft.trim() });
    }
    setDraft(null);
  };
  return (
    <label className="field">
      <span className="field-head">
        <span>Name</span>
      </span>
      <input
        type="text"
        maxLength={80}
        placeholder="Uses the effect's name"
        value={draft ?? layer.name ?? ""}
        onChange={(e) => setDraft(e.target.value)}
        onBlur={commit}
        onKeyDown={(e) => {
          if (e.key === "Enter") e.currentTarget.blur();
          if (e.key === "Escape") setDraft(null);
        }}
      />
    </label>
  );
}

function LayerBody({
  layer,
  effect,
  fixtures,
  hello,
  count,
  advanced,
}: {
  layer: Layer;
  effect: EffectInfo | undefined;
  fixtures: Fixture[];
  hello: Hello;
  count: number;
  advanced: boolean;
}) {
  const [picking, setPicking] = useState(false);
  const params = effect?.params ?? [];
  const basic = params.filter((p) => !p.advanced);
  const extra = params.filter((p) => p.advanced);
  const changed = Object.keys(layer.params).length > 0;
  // Sound choices only matter to effects that use sound, or once
  // "brightness follows sound" is in play.
  const usesSound = effect?.audio || layer.sound.pulse > 0;

  if (picking) {
    return (
      <div className="layer-body">
        <EffectPicker
          current={layer.effect}
          onPick={(id) => {
            setPicking(false);
            if (id !== layer.effect) engine.send({ type: "update_layer", id: layer.id, effect: id });
          }}
        />
        <button type="button" className="text-button" onClick={() => setPicking(false)}>
          Cancel
        </button>
      </div>
    );
  }

  return (
    <div className="layer-body">
      <button type="button" className="effect-choice" onClick={() => setPicking(true)} title="Choose a different effect">
        <span>
          <span className="effect-choice-label">Effect</span>
          {effect?.name ?? `Unknown (${layer.effect})`}
        </span>
        <span className="effect-choice-change">Change</span>
      </button>
      {!effect && <p className="callout error">This effect is not available in this version.</p>}

      {basic.map((spec) => (
        <ParamControl key={spec.id} layer={layer} spec={spec} />
      ))}
      {usesSound && <SoundControls layer={layer} hello={hello} advanced={advanced} />}
      <Targets layer={layer} fixtures={fixtures} />
      <Slider
        label="Strength"
        min={0}
        max={1}
        value={layer.opacity}
        format={(v) => `${Math.round(v * 100)}%`}
        onChange={numberSender(`${layer.id}:opacity`, (opacity) => ({ type: "update_layer", id: layer.id, opacity }))}
      />

      {advanced && (
        <div className="advanced">
          <h3>
            <SlidersHorizontal size={12} aria-hidden /> Advanced
          </h3>
          {extra.map((spec) => (
            <ParamControl key={spec.id} layer={layer} spec={spec} />
          ))}
          <Slider
            label="Brightness follows sound"
            min={0}
            max={1}
            value={layer.sound.pulse}
            format={(v) => (v === 0 ? "Off" : `${Math.round(v * 100)}%`)}
            onChange={numberSender(`${layer.id}:pulse`, (pulse) => ({ type: "set_sound", layer: layer.id, pulse }))}
          />
          {usesSound && (
            <Slider
              label="Sound sensitivity"
              min={0.25}
              max={4}
              step={0.05}
              value={layer.sound.gain}
              format={(v) => `${v.toFixed(2)}×`}
              onChange={numberSender(`${layer.id}:gain`, (gain) => ({ type: "set_sound", layer: layer.id, gain }))}
            />
          )}
          {count > 1 && (
            <label className="field">
              <span className="field-head">
                <span>Mixing with the effects below</span>
              </span>
              <select
                value={layer.blend}
                onChange={(e) => engine.send({ type: "update_layer", id: layer.id, blend: e.target.value as Blend })}
              >
                {BLENDS.map((b) => (
                  <option key={b.id} value={b.id}>
                    {b.name}: {b.hint.toLowerCase()}
                  </option>
                ))}
              </select>
            </label>
          )}
          <LayerName layer={layer} />
        </div>
      )}

      <div className="layer-footer">
        {advanced && (
          <button
            type="button"
            className="text-button"
            onClick={() => engine.send({ type: "duplicate_layer", id: layer.id })}
          >
            <Copy size={13} /> Duplicate
          </button>
        )}
        {changed && (
          <button
            type="button"
            className="text-button"
            onClick={() => engine.send({ type: "reset_params", layer: layer.id })}
          >
            <RotateCcw size={13} /> Reset
          </button>
        )}
        <span className="spacer" />
        <ConfirmButton
          label="Remove this effect"
          confirmLabel="Remove?"
          onConfirm={() => engine.send({ type: "remove_layer", id: layer.id })}
        >
          <Trash2 size={15} />
        </ConfirmButton>
      </div>
    </div>
  );
}

/** What a collapsed row says about its layer, beyond the name. */
function summary(layer: Layer, fixtures: Fixture[], hello: Hello): string[] {
  const parts: string[] = [];
  if (layer.sound.band.type !== "all") parts.push(bandName(layer.sound.band, hello));
  if (layer.targets.length === 1) {
    parts.push(fixtures.find((f) => f.id === layer.targets[0])?.name ?? "1 light");
  } else if (layer.targets.length > 1) {
    parts.push(`${layer.targets.length} lights`);
  }
  if (layer.opacity < 0.995) parts.push(`${Math.round(layer.opacity * 100)}%`);
  return parts;
}

export function LayerPanel() {
  const show = useShow();
  const hello = useHello();
  const advanced = useAdvanced();
  const [adding, setAdding] = useState(false);
  // One effect is open at a time, so the list stays short.
  const [open, setOpen] = useState<number | null>(null);
  const [drag, setDrag] = useState<{ id: number; over: number } | null>(null);
  const rows = useRef(new Map<number, HTMLLIElement>());
  const known = useRef<number[] | null>(null);

  // Open a layer the moment it is added; open the only one at start.
  const ids = show?.layers.map((l) => l.id) ?? [];
  const idKey = ids.join(",");
  useEffect(() => {
    if (!show) return;
    const before = known.current;
    known.current = ids;
    if (before === null) {
      if (ids.length === 1) setOpen(ids[0]!);
      return;
    }
    const added = ids.filter((id) => !before.includes(id));
    if (added.length === 1) setOpen(added[0]!);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [idKey, show === null]);

  if (!show || !hello) return null;
  // Listed top-down, like layers in a paint program: the last one drawn is first.
  const listed = show.layers.map((layer, index) => ({ layer, index })).reverse();

  const startDrag = (id: number) => (e: React.PointerEvent) => {
    e.preventDefault();
    setOpen(null);
    const from = listed.findIndex((l) => l.layer.id === id);
    setDrag({ id, over: from });
    const move = (ev: PointerEvent) => {
      // The slot is the first row whose middle is below the pointer.
      let over = listed.length - 1;
      for (let i = 0; i < listed.length; i++) {
        const rect = rows.current.get(listed[i]!.layer.id)?.getBoundingClientRect();
        if (rect && ev.clientY < rect.top + rect.height / 2) {
          over = i;
          break;
        }
      }
      setDrag({ id, over });
    };
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      setDrag((current) => {
        if (current && current.over !== from) {
          // The list is shown top-down; the engine counts from the bottom.
          engine.send({ type: "move_layer", id, to: listed.length - 1 - current.over });
        }
        return null;
      });
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  };

  return (
    <aside className="panel layers">
      <div className="panel-head">
        <h2>Effects</h2>
        <label className="advanced-toggle" title="Show every setting">
          Advanced
          <Switch checked={advanced} label="Show advanced settings" onChange={(on) => advancedPref.set(on)} />
        </label>
      </div>

      {adding ? (
        <div className="add-effect">
          <EffectPicker
            onPick={(effect) => {
              setAdding(false);
              engine.send({ type: "add_layer", effect });
            }}
          />
          <button type="button" className="text-button" onClick={() => setAdding(false)}>
            Cancel
          </button>
        </div>
      ) : (
        <button type="button" className="button wide" onClick={() => setAdding(true)}>
          <Plus size={15} /> Add an effect
        </button>
      )}
      {show.layers.length === 0 && !adding && (
        <p className="hint">No effects, so the lights are dark. Add one to get started.</p>
      )}

      <ul className={`layer-list ${drag ? "dragging" : ""}`}>
        {listed.map(({ layer }, position) => {
          const effect = hello.effects.find((e) => e.id === layer.effect);
          const isOpen = open === layer.id && !drag;
          const title = layer.name ?? effect?.name ?? layer.effect;
          const notes = summary(layer, show.fixtures, hello);
          const dropClass =
            drag && drag.id !== layer.id && drag.over === position
              ? listed.findIndex((l) => l.layer.id === drag.id) > position
                ? "drop-before"
                : "drop-after"
              : "";
          return (
            <li
              key={layer.id}
              ref={(el) => {
                if (el) rows.current.set(layer.id, el);
                else rows.current.delete(layer.id);
              }}
              className={`layer ${layer.enabled ? "" : "disabled"} ${isOpen ? "open" : ""} ${
                drag?.id === layer.id ? "lifted" : ""
              } ${dropClass}`}
            >
              <div className="layer-row">
                {show.layers.length > 1 && (
                  <span
                    className="grip"
                    title="Drag to reorder. Effects higher up are drawn on top."
                    onPointerDown={startDrag(layer.id)}
                  >
                    <GripVertical size={15} />
                  </span>
                )}
                <button
                  type="button"
                  className="layer-main"
                  aria-expanded={isOpen}
                  onClick={() => setOpen(isOpen ? null : layer.id)}
                >
                  <span className="layer-swatch" style={{ background: swatch(layer, effect, hello) }} />
                  <span className="layer-text">
                    <span className="layer-title">
                      {title}
                      {effect?.audio && <AudioLines size={12} aria-label="Reacts to sound" />}
                    </span>
                    {(notes.length > 0 || (layer.name && effect)) && (
                      <span className="layer-notes">{[layer.name && effect ? effect.name : null, ...notes].filter(Boolean).join(" · ")}</span>
                    )}
                  </span>
                  <ChevronDown size={15} className="chevron" />
                </button>
                <IconButton
                  label={layer.enabled ? "Hide this effect" : "Show this effect"}
                  onClick={() => engine.send({ type: "update_layer", id: layer.id, enabled: !layer.enabled })}
                >
                  {layer.enabled ? <Eye size={15} /> : <EyeOff size={15} />}
                </IconButton>
              </div>
              {isOpen && (
                <LayerBody
                  layer={layer}
                  effect={effect}
                  fixtures={show.fixtures}
                  hello={hello}
                  count={show.layers.length}
                  advanced={advanced}
                />
              )}
            </li>
          );
        })}
      </ul>
      {show.layers.length > 1 && <p className="hint">Effects higher in the list are drawn on top of those below.</p>}
    </aside>
  );
}
