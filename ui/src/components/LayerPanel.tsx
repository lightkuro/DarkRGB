// The stack of effects, and the controls for each one. Controls are built
// from what the engine says each effect exposes, so a new effect needs no
// UI code.

import { ArrowDown, ArrowUp, AudioLines, Eye, EyeOff, Plus, RotateCcw, Trash2 } from "lucide-react";
import { useCallback, useState } from "react";
import {
  engine,
  perFrame,
  useHello,
  useShow,
  type Blend,
  type EffectInfo,
  type Fixture,
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

export function EffectPicker({ onPick, current }: { onPick: (id: string) => void; current?: string }) {
  const effects = useHello()?.effects ?? [];
  return (
    <div className="effect-grid">
      {effects.map((effect) => (
        <button
          key={effect.id}
          type="button"
          className={`effect-card ${effect.id === current ? "selected" : ""}`}
          onClick={() => onPick(effect.id)}
        >
          <span className="effect-name">
            {effect.name}
            {effect.audio && (
              <span className="badge" title="Reacts to sound">
                <AudioLines size={12} />
              </span>
            )}
          </span>
          <span className="effect-description">{effect.description}</span>
        </button>
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

function LayerCard({
  layer,
  effect,
  fixtures,
  index,
  count,
}: {
  layer: Layer;
  effect: EffectInfo | undefined;
  fixtures: Fixture[];
  index: number;
  count: number;
}) {
  const [picking, setPicking] = useState(false);
  const changed = Object.keys(layer.params).length > 0;
  return (
    <li className={`layer ${layer.enabled ? "" : "disabled"}`}>
      <div className="layer-head">
        <button
          type="button"
          className="layer-effect"
          aria-expanded={picking}
          title="Choose a different effect"
          onClick={() => setPicking(!picking)}
        >
          {effect?.name ?? `Unknown effect (${layer.effect})`}
          {effect?.audio && <AudioLines size={13} aria-label="Reacts to sound" />}
        </button>
        <div className="layer-actions">
          {count > 1 && (
            <>
              <IconButton
                label="Move up (drawn later, on top)"
                onClick={() => engine.send({ type: "move_layer", id: layer.id, to: Math.min(count - 1, index + 1) })}
              >
                <ArrowUp size={14} />
              </IconButton>
              <IconButton
                label="Move down (drawn earlier, underneath)"
                onClick={() => engine.send({ type: "move_layer", id: layer.id, to: Math.max(0, index - 1) })}
              >
                <ArrowDown size={14} />
              </IconButton>
            </>
          )}
          <IconButton
            label={layer.enabled ? "Hide this effect" : "Show this effect"}
            onClick={() => engine.send({ type: "update_layer", id: layer.id, enabled: !layer.enabled })}
          >
            {layer.enabled ? <Eye size={15} /> : <EyeOff size={15} />}
          </IconButton>
          <ConfirmButton
            label="Remove this effect"
            confirmLabel="Remove?"
            onConfirm={() => engine.send({ type: "remove_layer", id: layer.id })}
          >
            <Trash2 size={15} />
          </ConfirmButton>
        </div>
      </div>

      {picking ? (
        <EffectPicker
          current={layer.effect}
          onPick={(id) => {
            setPicking(false);
            if (id !== layer.effect) engine.send({ type: "update_layer", id: layer.id, effect: id });
          }}
        />
      ) : (
        <div className="layer-body">
          {!effect && <p className="callout error">This effect is not available in this version.</p>}
          {effect?.params.map((spec) => <ParamControl key={spec.id} layer={layer} spec={spec} />)}
          <Targets layer={layer} fixtures={fixtures} />
          <div className="layer-mix">
            <Slider
              label="Strength"
              min={0}
              max={1}
              value={layer.opacity}
              format={(v) => `${Math.round(v * 100)}%`}
              onChange={numberSender(`${layer.id}:opacity`, (opacity) => ({
                type: "update_layer",
                id: layer.id,
                opacity,
              }))}
            />
            {count > 1 && (
              <label className="field">
                <span className="field-head">
                  <span>Mixing</span>
                </span>
                <select
                  value={layer.blend}
                  title={BLENDS.find((b) => b.id === layer.blend)?.hint}
                  onChange={(e) => engine.send({ type: "update_layer", id: layer.id, blend: e.target.value as Blend })}
                >
                  {BLENDS.map((b) => (
                    <option key={b.id} value={b.id}>
                      {b.name}
                    </option>
                  ))}
                </select>
              </label>
            )}
          </div>
          {changed && (
            <button
              type="button"
              className="text-button"
              onClick={() => engine.send({ type: "reset_params", layer: layer.id })}
            >
              <RotateCcw size={13} /> Reset settings
            </button>
          )}
        </div>
      )}
    </li>
  );
}

export function LayerPanel() {
  const show = useShow();
  const hello = useHello();
  const [adding, setAdding] = useState(false);
  if (!show || !hello) return null;
  // Listed top-down, like layers in a paint program: the last one drawn is first.
  const layers = show.layers.map((layer, index) => ({ layer, index })).reverse();
  return (
    <aside className="panel layers">
      <h2>Effects</h2>
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
          <Plus size={15} /> Add an effect on top
        </button>
      )}
      {show.layers.length === 0 && !adding && (
        <p className="hint">No effects, so the lights are dark. Add one to get started.</p>
      )}
      <ul className="layer-list">
        {layers.map(({ layer, index }) => (
          <LayerCard
            key={layer.id}
            layer={layer}
            index={index}
            count={show.layers.length}
            effect={hello.effects.find((e) => e.id === layer.effect)}
            fixtures={show.fixtures}
          />
        ))}
      </ul>
    </aside>
  );
}
