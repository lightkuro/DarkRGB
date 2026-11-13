// The stack of effects. Each one is a single row until it is opened, so
// many fit on screen. Controls are built from what the engine says each
// effect exposes; a new effect needs no UI code.
//
// Beginners see the few settings that matter. The Advanced switch at the
// top reveals the rest.

import {
  RotateCw,
  AudioLines,
  Bookmark,
  BoxSelect,
  ChevronDown,
  ChevronRight,
  Copy,
  Eye,
  EyeOff,
  Folder,
  FolderOpen,
  FolderPlus,
  GripVertical,
  Pin,
  Plus,
  RotateCcw,
  Settings2,
  SlidersHorizontal,
  Trash2,
  X,
} from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import {
  fullPref,
  engine,
  perFrame,
  readPalette,
  settingsPage,
  toggleFolded,
  useFull,
  useFolded,
  useHello,
  useLibrary,
  useLive,
  useSelection,
  useShow,
  writePalette,
  type Band,
  type Blend,
  type EffectInfo,
  type Fixture,
  type Hello,
  type Layer,
  type LayerGroup,
  type LightGroup,
  type Palette,
  type ParamSpec,
  type ParamValue,
  type SavedPattern,
  type Show,
} from "../engine";
import { KeyAssign, Keycap } from "./Keys";
import { ANGLE, HUE, LinkedSlider, STRENGTH } from "./Links";
import { ConfirmButton, IconButton, NameField, Slider, Switch, useDismiss } from "./ui";
import { PictureField } from "./PictureField";

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

/** A small picture of a layer's colours, for its collapsed row and its pad. */
export function swatch(layer: Layer, effect: EffectInfo | undefined, hello: Hello): string {
  const value = (spec: ParamSpec) => String(layer.params[spec.id] ?? spec.default);
  const palette = effect?.params.find((p) => p.kind === "palette");
  if (palette) {
    const found = readPalette(value(palette), hello.palettes);
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

/** Colours a new palette of one's own starts from. */
const STARTER = ["#ff3b30", "#ffffff"];

function PalettePicker({ value, onChange, label }: { value: string; onChange: (id: string) => void; label: string }) {
  const palettes = useHello()?.palettes ?? [];
  const library = useLibrary();
  const [open, setOpen] = useState(false);
  const [naming, setNaming] = useState<string | null>(null);
  const close = useCallback(() => setOpen(false), []);
  const ref = useDismiss(open, close);
  const current = readPalette(value, palettes) ?? palettes[0];
  const own = value.startsWith("#") ? current : undefined;
  const keptAs = library.palettes.find((p) => p.value === value);
  // Colour pickers fire continuously while dragging.
  const quiet = useRef(perFrame((next: string) => onChange(next))).current;
  const setOwn = (stops: string[], cyclic: boolean, dragging = false) =>
    (dragging ? quiet : onChange)(writePalette(stops, cyclic));
  return (
    <div className="field palette-field" ref={ref}>
      <span className="field-head">
        <span>{label}</span>
        <output>{keptAs?.name ?? current?.name}</output>
      </span>
      <button
        type="button"
        className="palette-current"
        aria-label={`${label}: ${keptAs?.name ?? current?.name ?? ""}. Change`}
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
          {library.palettes.map((p) => {
            const read = readPalette(p.value, palettes);
            return (
              <span key={`kept-${p.id}`} className="palette-kept">
                <button
                  type="button"
                  className={`palette-option ${p.value === value ? "selected" : ""}`}
                  onClick={() => {
                    onChange(p.value);
                    setOpen(false);
                  }}
                >
                  <span className="swatch" style={{ background: read ? gradient(read) : undefined }} />
                  {p.name}
                </button>
                <ConfirmButton
                  label={`Delete the palette ${p.name}`}
                  confirmLabel="Delete?"
                  onConfirm={() => engine.send({ type: "delete_palette", id: p.id })}
                >
                  <Trash2 size={13} />
                </ConfirmButton>
              </span>
            );
          })}
          <div className="palette-own">
            {own ? (
              <>
                <span className="palette-own-title">Your own colours</span>
                <div className="palette-stops">
                  {own.stops.map((stop, i) => (
                    <span key={i} className="palette-stop">
                      <input
                        type="color"
                        aria-label={`Colour ${i + 1}`}
                        value={stop}
                        onChange={(e) =>
                          setOwn(
                            own.stops.map((s, k) => (k === i ? e.target.value : s)),
                            own.cyclic,
                            true,
                          )
                        }
                      />
                      {own.stops.length > 2 && (
                        <button
                          type="button"
                          aria-label={`Remove colour ${i + 1}`}
                          title="Remove this colour"
                          onClick={() =>
                            setOwn(
                              own.stops.filter((_, k) => k !== i),
                              own.cyclic,
                            )
                          }
                        >
                          <X size={10} />
                        </button>
                      )}
                    </span>
                  ))}
                  {own.stops.length < 8 && (
                    <button
                      type="button"
                      className="palette-add"
                      aria-label="Add a colour"
                      title="Add a colour"
                      onClick={() => setOwn([...own.stops, own.stops[own.stops.length - 1] ?? "#ffffff"], own.cyclic)}
                    >
                      <Plus size={13} />
                    </button>
                  )}
                </div>
                <div className="field inline">
                  <span title="On: the last colour blends back into the first, for effects that go round. Off: it runs from the first colour to the last, for effects that go from quiet to loud.">
                    Goes round
                  </span>
                  <Switch
                    checked={own.cyclic}
                    label="The last colour blends back into the first"
                    onChange={(cyclic) => setOwn(own.stops, cyclic)}
                  />
                </div>
                {library.available &&
                  !keptAs &&
                  (naming === null ? (
                    <button type="button" className="text-button" onClick={() => setNaming("")}>
                      <Bookmark size={13} /> Keep these colours for any show
                    </button>
                  ) : (
                    <form
                      className="palette-name"
                      onSubmit={(e) => {
                        e.preventDefault();
                        if (!naming.trim()) return;
                        engine.send({ type: "save_palette", name: naming.trim(), value });
                        setNaming(null);
                      }}
                    >
                      <input
                        type="text"
                        autoFocus
                        maxLength={80}
                        placeholder="Name for these colours"
                        aria-label="Name for these colours"
                        value={naming}
                        onChange={(e) => setNaming(e.target.value)}
                      />
                      <button type="submit" className="button small">
                        Keep
                      </button>
                    </form>
                  ))}
              </>
            ) : (
              <button
                type="button"
                className="palette-option"
                onClick={() => setOwn(current && current.stops.length <= 8 ? current.stops : STARTER, current?.cyclic ?? true)}
              >
                <span className="swatch own" />
                Your own colours…
              </button>
            )}
          </div>
        </div>
      )}
    </div>
  );
}

function ParamControl({ layer, spec, full }: { layer: Layer; spec: ParamSpec; full: boolean }) {
  const value: ParamValue = layer.params[spec.id] ?? spec.default;
  const set = (next: ParamValue) => engine.send({ type: "set_param", layer: layer.id, param: spec.id, value: next });

  switch (spec.kind) {
    case "float":
      return (
        <LinkedSlider
          layer={layer}
          param={spec.id}
          offer={full}
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
    case "picture":
      return <PictureField spec={spec} value={String(value)} layer={layer.id} />;
    case "words":
      return (
        <label className="field">
          <span className="field-head">
            <span>{spec.label}</span>
          </span>
          <input
            type="text"
            maxLength={spec.max ?? 60}
            value={String(value)}
            // Each letter shows on the lights as it is typed.
            onChange={(e) => engine.quiet({ type: "set_param", layer: layer.id, param: spec.id, value: e.target.value })}
          />
        </label>
      );
  }
}

/** Effects grouped the way the engine groups them. */
export function EffectPicker({
  onPick,
  onUse,
  current,
}: {
  onPick: (id: string) => void;
  /** Given when something kept can be added here: called with its id. */
  onUse?: (pattern: number) => void;
  current?: string;
}) {
  const hello = useHello();
  const library = useLibrary();
  const effects = hello?.effects ?? [];
  const categories = [...new Set(effects.map((e) => e.category))];
  const kept = onUse ? library.patterns : [];
  const describe = (pattern: SavedPattern) =>
    pattern.look
      ? `A look of ${pattern.layers.length} effects. Added as a scene.`
      : (effects.find((e) => e.id === pattern.layers[0]?.effect)?.name ?? "An effect") + ", with your settings.";
  return (
    <div className="effect-picker">
      {kept.length > 0 && onUse && (
        <div className="effect-group">
          <h3>Kept by you</h3>
          <div className="effect-grid">
            {kept.map((pattern) => (
              <button key={pattern.id} type="button" className="effect-card kept" onClick={() => onUse(pattern.id)}>
                <span className="effect-name">
                  <Bookmark size={12} aria-hidden /> {pattern.name}
                </span>
                <span className="effect-description">{describe(pattern)}</span>
              </button>
            ))}
          </div>
        </div>
      )}
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

/**
 * What can be done to any pattern, whatever the effect: turn it, set it
 * spinning, mirror it, repeat it, wrap it round the middle, spread it
 * from light to light, and shift its colours. A single strip has only
 * one direction, so turning and wrapping change nothing on it.
 */
function Turning({ layer, full, matrices }: { layer: Layer; full: boolean; matrices: boolean }) {
  const angle = layer.angle ?? 0;
  const spin = layer.spin ?? 0;
  const mirror = layer.mirror ?? "off";
  const repeat = layer.repeat ?? 1;
  const kaleido = layer.kaleido ?? 0;
  const spread = layer.spread ?? 0;
  const hue = layer.hue ?? 0;
  const linked = (param: string) => layer.links?.some((l) => l.param === param) ?? false;
  const update = (change: Partial<Extract<Parameters<typeof engine.send>[0], { type: "update_layer" }>>) =>
    engine.send({ type: "update_layer", id: layer.id, ...change });
  const slide = (key: string, make: (value: number) => Parameters<typeof engine.quiet>[0]) =>
    numberSender(`${layer.id}:${key}`, make);
  // The simple layout shows what is set, and offers the angle once there
  // is a matrix for it to show on.
  const showAngle = full || matrices || angle !== 0 || linked(ANGLE);
  // Turned by an eighth of a turn at a time: upright, cornerwise, sideways.
  const step = (by: number) => update({ angle: (((Math.round(angle / 45) + by) * 45) % 360 + 360) % 360 });
  return (
    <>
      {showAngle && (
        <div className="angle-row">
          <LinkedSlider
            layer={layer}
            param={ANGLE}
            offer={full}
            label="Angle"
            min={0}
            max={360}
            step={full ? 1 : 45}
            value={angle}
            format={(v) => `${Math.round(v) % 360}°`}
            onChange={slide("angle", (value) => ({ type: "update_layer", id: layer.id, angle: value }))}
          />
          <div className="tool-group angle-steps">
            <button type="button" className="tool" aria-label="Turn the pattern 45 degrees anticlockwise" title="Turn 45° anticlockwise" onClick={() => step(-1)}>
              <RotateCcw size={14} /> 45°
            </button>
            <button type="button" className="tool" aria-label="Turn the pattern 45 degrees clockwise" title="Turn 45° clockwise" onClick={() => step(1)}>
              <RotateCw size={14} /> 45°
            </button>
          </div>
        </div>
      )}
      {(full || spin !== 0) && (
        <Slider
          label="Spin"
          min={-180}
          max={180}
          step={1}
          value={spin}
          format={(v) => (v === 0 ? "Still" : `${Math.abs(Math.round(v))}° a second${v < 0 ? ", backwards" : ""}`)}
          onChange={slide("spin", (value) => ({ type: "update_layer", id: layer.id, spin: value }))}
        />
      )}
      {layer.each && (full || layer.turns) && (
        <div className="field inline each">
          <span>
            Turn with each light
            <span className="hint"> Off, the pattern stays the right way up however a matrix is hung.</span>
          </span>
          <Switch checked={layer.turns ?? false} label="Turn the pattern with each light" onChange={(turns) => update({ turns })} />
        </div>
      )}
      {(full || mirror !== "off") && (
        <div className="field">
          <span className="field-head">
            <span>Mirror</span>
          </span>
          <div className="segmented" role="radiogroup" aria-label="Mirror">
            {(
              [
                ["off", "Off"],
                ["across", "Left and right"],
                ["down", "Up and down"],
                ["both", "Both"],
              ] as const
            ).map(([id, name]) => (
              <button key={id} type="button" role="radio" aria-checked={mirror === id} className={mirror === id ? "selected" : ""} onClick={() => update({ mirror: id })}>
                {name}
              </button>
            ))}
          </div>
        </div>
      )}
      {(full || repeat > 1) && (
        <Slider
          label="Repeat"
          min={1}
          max={8}
          step={1}
          value={repeat}
          format={(v) => (v <= 1 ? "Once" : `${Math.round(v)} times`)}
          onChange={slide("repeat", (value) => ({ type: "update_layer", id: layer.id, repeat: Math.round(value) }))}
        />
      )}
      {(full || kaleido > 0) && (
        <Slider
          label="Kaleidoscope"
          min={1}
          max={12}
          step={1}
          value={kaleido === 0 ? 1 : kaleido}
          format={(v) => (v <= 1 ? "Off" : `${Math.round(v)} wedges`)}
          onChange={slide("kaleido", (value) => ({ type: "update_layer", id: layer.id, kaleido: value <= 1 ? 0 : Math.round(value) }))}
        />
      )}
      {layer.each && (full || spread > 0) && (
        <Slider
          label="Spread from light to light"
          min={0}
          max={1}
          step={0.01}
          value={spread}
          format={(v) => (v === 0 ? "None" : `${Math.round(v * 100)}%`)}
          onChange={slide("spread", (value) => ({ type: "update_layer", id: layer.id, spread: value }))}
        />
      )}
      {(full || hue !== 0 || linked(HUE)) && (
        <LinkedSlider
          layer={layer}
          param={HUE}
          offer={full}
          label="Colour shift"
          min={0}
          max={360}
          step={1}
          value={hue}
          format={(v) => (Math.round(v) % 360 === 0 ? "None" : `${Math.round(v)}°`)}
          onChange={slide("hue", (value) => ({ type: "update_layer", id: layer.id, hue: value }))}
        />
      )}
    </>
  );
}

function Targets({
  layer,
  fixtures,
  groups,
  full,
}: {
  layer: Layer;
  fixtures: Fixture[];
  groups: LightGroup[];
  full: boolean;
}) {
  const picked = useSelection();
  const strips = fixtures.filter((f) => f.shape.type === "strip").length;
  // Offered when the show has both kinds of light, or the effect is kept to one already.
  const kinds = (strips > 0 && strips < fixtures.length) || layer.only !== "any";
  if (fixtures.length < 2 && groups.length === 0 && !kinds) return null;
  const all = layer.targets.length === 0 && layer.only === "any";
  // What is picked on the stage, when it is not already what this plays on.
  const pickedIsNew =
    picked.length > 0 &&
    picked.length < fixtures.length &&
    !(picked.length === layer.targets.length && picked.every((id) => layer.targets.includes(id)));
  const toggle = (id: number) => {
    const next = layer.targets.includes(id) ? layer.targets.filter((t) => t !== id) : [...layer.targets, id];
    // Ticking every light one by one is the same as "all lights".
    const everyLight = fixtures.every((f) => next.includes(f.id));
    engine.send({ type: "update_layer", id: layer.id, targets: everyLight ? [] : next });
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
          onClick={() => engine.send({ type: "update_layer", id: layer.id, targets: [], only: "any" })}
        >
          All lights
        </button>
        {kinds &&
          (["strips", "matrices"] as const).map((only) => (
            <button
              key={only}
              type="button"
              className={`chip ${layer.only === only ? "selected" : ""}`}
              aria-pressed={layer.only === only}
              title={`Only on ${only}, including ones added later`}
              onClick={() =>
                engine.send({ type: "update_layer", id: layer.id, only: layer.only === only ? "any" : only })
              }
            >
              {only === "strips" ? "Strips" : "Matrices"}
            </button>
          ))}
        {pickedIsNew && (
          <button
            type="button"
            className="chip add"
            title="Play on the lights picked on the stage"
            onClick={() => engine.send({ type: "update_layer", id: layer.id, targets: picked })}
          >
            <BoxSelect size={12} aria-hidden /> The {picked.length} picked
          </button>
        )}
        {groups.map((g) => (
          <button
            key={g.id}
            type="button"
            className={`chip ${layer.targets.includes(g.id) ? "selected" : ""}`}
            aria-pressed={layer.targets.includes(g.id)}
            title={`Every light in ${g.name}, including ones added to it later`}
            onClick={() => toggle(g.id)}
          >
            <Folder size={12} aria-hidden /> {g.name}
          </button>
        ))}
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
      {
        // Only a choice when it plays on more than one light.
        full && (all || layer.targets.length > 1 || layer.targets.some((id) => groups.some((g) => g.id === id))) && (
          <div className="field inline each">
            <span title="Off: the effect is drawn once across all of these lights, wherever they are. On: every light gets the whole effect to itself.">
              Each light on its own
            </span>
            <Switch
              checked={layer.each}
              label="Draw the effect on each light by itself"
              onChange={(each) => engine.send({ type: "update_layer", id: layer.id, each })}
            />
          </div>
        )
      }
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

/** For effects played by hand: deaf to the music, lit only when fired. */
function ManualSwitch({ layer }: { layer: Layer }) {
  return (
    <div className="field inline">
      <span>
        Only when fired
        <span className="hint block">Ignores the music. Give it a key set to "Fire it".</span>
      </span>
      <Switch
        checked={layer.sound.manual}
        label="React only when fired"
        onChange={(manual) => engine.send({ type: "set_sound", layer: layer.id, manual })}
      />
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

/** The layers top first, as they are listed, with the folder each is in. */
function placements(show: Show): { id: number; group: number | null }[] {
  return show.layers.map((l) => ({ id: l.id, group: l.group })).reverse();
}

/** Sends a new arrangement; the engine wants it bottom first. */
function arrange(order: { id: number; group: number | null }[]): void {
  engine.send({ type: "arrange_layers", order: [...order].reverse() });
}

function LayerBody({
  layer,
  effect,
  show,
  hello,
  advanced,
}: {
  layer: Layer;
  effect: EffectInfo | undefined;
  show: Show;
  hello: Hello;
  advanced: boolean;
}) {
  const fixtures = show.fixtures;
  const count = show.layers.length;
  const library = useLibrary();
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
      {effect && effect.flash !== "none" && show.flash.limit > 0 && (
        <p className="callout note flash-note">
          {effect.flash === "needed"
            ? "This effect only works flashing fast, so it is off while this show's flash limit is on."
            : `This show's flash limit holds it to ${show.flash.limit} flashes a second.`}{" "}
          <button type="button" className="text-button" onClick={() => settingsPage.set("lights")}>
            Flash limit…
          </button>
        </p>
      )}

      {basic.map((spec) => (
        <ParamControl key={spec.id} layer={layer} spec={spec} full={advanced} />
      ))}
      {usesSound && !layer.sound.manual && <SoundControls layer={layer} hello={hello} advanced={advanced} />}
      {layer.sound.manual && !advanced && <ManualSwitch layer={layer} />}
      <Targets layer={layer} fixtures={fixtures} groups={show.light_groups} full={advanced} />
      <LinkedSlider
        layer={layer}
        param={STRENGTH}
        offer={advanced}
        label="Strength"
        min={0}
        max={1}
        value={layer.opacity}
        format={(v) => `${Math.round(v * 100)}%`}
        onChange={numberSender(`${layer.id}:opacity`, (opacity) => ({ type: "update_layer", id: layer.id, opacity }))}
      />
      <Turning layer={layer} full={advanced} matrices={fixtures.some((f) => f.shape.type === "matrix")} />
      {/* In the simple layout, keys are given in the Live view. One that is there already still shows. */}
      {(advanced || show.bindings.some((b) => b.action.type === "layer" && b.action.id === layer.id && b.key)) && (
        <KeyAssign action={{ type: "layer", id: layer.id }} />
      )}
      {advanced && show.layer_groups.length > 0 && (
        <label className="field inline">
          <span>Folder</span>
          <select
            value={layer.group ?? ""}
            onChange={(e) => {
              const group = e.target.value === "" ? null : Number(e.target.value);
              arrange(placements(show).map((p) => (p.id === layer.id ? { ...p, group } : p)));
            }}
          >
            <option value="">None</option>
            {show.layer_groups.map((g) => (
              <option key={g.id} value={g.id}>
                {g.name}
              </option>
            ))}
          </select>
        </label>
      )}

      {advanced && (
        <div className="advanced">
          <h3>
            <SlidersHorizontal size={12} aria-hidden /> Advanced
          </h3>
          {extra.map((spec) => (
            <ParamControl key={spec.id} layer={layer} spec={spec} full={advanced} />
          ))}
          {effect?.audio && <ManualSwitch layer={layer} />}
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
        {library.available && (changed || advanced) && (
          <button
            type="button"
            className="text-button"
            title="Keep this effect, with its settings, to use in any show. It is listed when you add an effect."
            onClick={() => engine.send({ type: "save_pattern", layer: layer.id })}
          >
            <Bookmark size={13} /> Keep
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
function summary(layer: Layer, show: Show, hello: Hello): string[] {
  const parts: string[] = [];
  const flash = hello.effects.find((e) => e.id === layer.effect)?.flash;
  if (flash === "needed" && show.flash.limit > 0) parts.push("off: flash limit");
  if (layer.sound.manual) parts.push("when fired");
  else if (layer.sound.band.type !== "all") parts.push(bandName(layer.sound.band, hello));
  if (layer.only !== "any") parts.push(layer.only);
  if (layer.targets.length === 1) {
    const id = layer.targets[0];
    const name = show.fixtures.find((f) => f.id === id)?.name ?? show.light_groups.find((g) => g.id === id)?.name;
    parts.push(name ?? "1 light");
  } else if (layer.targets.length > 1) {
    parts.push(`${layer.targets.length} places`);
  }
  if (layer.opacity < 0.995) parts.push(`${Math.round(layer.opacity * 100)}%`);
  const links = layer.links?.length ?? 0;
  if (links > 0) parts.push(links === 1 ? "1 slider follows" : `${links} sliders follow`);
  return parts;
}

/** What is being dragged, and where it would land if let go now. */
interface Drag {
  kind: "layer" | "folder";
  id: number;
  drop: { key: string; where: "before" | "after" | "into" } | null;
}

/** A top-level item of the list: a folder with its layers, or a lone layer. */
type Item = { folder: LayerGroup; layers: Layer[] } | { folder: null; layer: Layer };

/** The stack as it is listed: top first, folders gathered, empty folders on top. */
function listItems(show: Show): Item[] {
  const items: Item[] = [];
  const top = [...show.layers].reverse();
  for (const folder of show.layer_groups) {
    if (!top.some((l) => l.group === folder.id)) items.push({ folder, layers: [] });
  }
  for (const layer of top) {
    const folder = show.layer_groups.find((g) => g.id === layer.group) ?? null;
    const last = items[items.length - 1];
    if (folder && last && last.folder && last.folder.id === folder.id) last.layers.push(layer);
    else if (folder) items.push({ folder, layers: [layer] });
    else items.push({ folder: null, layer });
  }
  return items;
}

function FolderSettings({ folder, onAdd }: { folder: LayerGroup; onAdd: () => void }) {
  return (
    <div className="folder-settings">
      <NameField
        value={folder.name}
        autoSelect
        onCommit={(name) => engine.send({ type: "update_layer_group", id: folder.id, name })}
      />
      <Slider
        label="Strength of everything inside"
        min={0}
        max={1}
        value={folder.opacity}
        format={(v) => `${Math.round(v * 100)}%`}
        onChange={numberSender(`folder:${folder.id}`, (opacity) => ({
          type: "update_layer_group",
          id: folder.id,
          opacity,
        }))}
      />
      <KeyAssign action={{ type: "layer_group", id: folder.id }} />
      <div className="field inline">
        <span>
          Keep through scene changes
          <span className="hint block">
            Stays on top and is not part of any scene. For things you play by hand, like a flash.
          </span>
        </span>
        <Switch
          checked={folder.pinned}
          label="Keep this folder through scene changes"
          onChange={(pinned) => engine.send({ type: "update_layer_group", id: folder.id, pinned })}
        />
      </div>
      <div className="layer-footer">
        <button type="button" className="text-button" onClick={onAdd}>
          <Plus size={13} /> Add an effect here
        </button>
        <span className="spacer" />
        <button
          type="button"
          className="text-button"
          title="Remove the folder and keep its effects"
          onClick={() => engine.send({ type: "remove_layer_group", id: folder.id, keep_layers: true })}
        >
          Ungroup
        </button>
        <ConfirmButton
          label="Remove this folder and the effects in it"
          confirmLabel="Remove all?"
          onConfirm={() => engine.send({ type: "remove_layer_group", id: folder.id, keep_layers: false })}
        >
          <Trash2 size={15} />
        </ConfirmButton>
      </div>
    </div>
  );
}

export function LayerPanel() {
  const show = useShow();
  const hello = useHello();
  const advanced = useFull();
  const folded = useFolded();
  const live = useLive();
  // The effect picker, and the folder a new effect should go into.
  const [adding, setAdding] = useState<{ group: number | null } | null>(null);
  // One effect is open at a time, so the list stays short.
  const [open, setOpen] = useState<number | null>(null);
  const [openFolder, setOpenFolder] = useState<number | null>(null);
  const [drag, setDrag] = useState<Drag | null>(null);
  const rows = useRef(new Map<string, HTMLElement>());
  const known = useRef<{ layers: number[]; folders: number[] } | null>(null);

  // Open a layer the moment it is added; open the only one at start. A
  // new folder opens too, with its name ready to type over.
  const ids = show?.layers.map((l) => l.id) ?? [];
  const folderIds = show?.layer_groups.map((g) => g.id) ?? [];
  const idKey = `${ids.join(",")}|${folderIds.join(",")}`;
  useEffect(() => {
    if (!show) return;
    const before = known.current;
    known.current = { layers: ids, folders: folderIds };
    if (before === null) {
      if (ids.length === 1) setOpen(ids[0]!);
      return;
    }
    const added = ids.filter((id) => !before.layers.includes(id));
    if (added.length === 1) setOpen(added[0]!);
    const addedFolders = folderIds.filter((id) => !before.folders.includes(id));
    if (addedFolders.length === 1) setOpenFolder(addedFolders[0]!);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [idKey, show === null]);

  if (!show || !hello) return null;
  const items = listItems(show);
  const isFolded = (id: number) => folded.includes(id);
  const register = (key: string) => (el: HTMLElement | null) => {
    if (el) rows.current.set(key, el);
    else rows.current.delete(key);
  };

  /** Where the pointer is, in terms of the rows on screen. */
  const dropAt = (kind: Drag["kind"], id: number, y: number): Drag["drop"] => {
    // Rows that can be dropped on, top to bottom. A folder being dragged
    // only goes between top-level items.
    const keys: string[] = [];
    for (const item of items) {
      if (item.folder) {
        if (kind === "folder") keys.push(`b:${item.folder.id}`);
        else {
          keys.push(`f:${item.folder.id}`);
          if (!isFolded(item.folder.id)) for (const layer of item.layers) keys.push(`l:${layer.id}`);
        }
      } else keys.push(`l:${item.layer.id}`);
    }
    let last: DOMRect | undefined;
    for (const key of keys) {
      const rect = rows.current.get(key)?.getBoundingClientRect();
      if (!rect) continue;
      last = rect;
      if (y > rect.bottom) continue;
      if (key === `${kind === "folder" ? "b" : "l"}:${id}`) return null;
      if (key.startsWith("f:")) return { key, where: y < rect.top + rect.height * 0.3 ? "before" : "into" };
      return { key, where: y < rect.top + rect.height / 2 ? "before" : "after" };
    }
    // Below everything: the bottom of the stack, in no folder.
    return last && y > last.bottom + 4 ? { key: "end", where: "after" } : null;
  };

  /** The arrangement after letting go at `drop`. */
  const dropped = (moving: Drag, drop: NonNullable<Drag["drop"]>) => {
    const order = placements(show);
    const targetId = Number(drop.key.slice(2));
    if (moving.kind === "layer") {
      const rest = order.filter((p) => p.id !== moving.id);
      const put = (at: number, group: number | null) => rest.splice(at < 0 ? 0 : at, 0, { id: moving.id, group });
      if (drop.key === "end") put(rest.length, null);
      else if (drop.key.startsWith("l:")) {
        const at = rest.findIndex((p) => p.id === targetId);
        put(drop.where === "before" ? at : at + 1, rest[at]?.group ?? null);
      } else {
        // On a folder: into it at the top, or just above it.
        const at = rest.findIndex((p) => p.group === targetId);
        put(at, drop.where === "into" ? targetId : null);
      }
      return rest;
    }
    const block = order.filter((p) => p.group === moving.id);
    const rest = order.filter((p) => p.group !== moving.id);
    let at = rest.length;
    if (drop.key.startsWith("l:")) {
      const index = rest.findIndex((p) => p.id === targetId);
      at = drop.where === "before" ? index : index + 1;
    } else if (drop.key.startsWith("b:")) {
      const first = rest.findIndex((p) => p.group === targetId);
      const lastIndex = rest.map((p) => p.group).lastIndexOf(targetId);
      at = first < 0 ? 0 : drop.where === "before" ? first : lastIndex + 1;
    }
    rest.splice(Math.max(at, 0), 0, ...block);
    return rest;
  };

  const startDrag = (kind: Drag["kind"], id: number) => (e: React.PointerEvent) => {
    e.preventDefault();
    setOpen(null);
    setOpenFolder(null);
    setDrag({ kind, id, drop: null });
    const move = (ev: PointerEvent) => setDrag({ kind, id, drop: dropAt(kind, id, ev.clientY) });
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      setDrag((current) => {
        if (current?.drop) {
          const next = dropped(current, current.drop);
          const before = placements(show);
          const same = next.every((p, i) => p.id === before[i]?.id && p.group === before[i]?.group);
          if (!same) arrange(next);
        }
        return null;
      });
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  };

  const dropClass = (key: string) => (drag?.drop?.key === key ? `drop-${drag.drop.where}` : "");

  const newFolder = () => {
    const taken = show.layer_groups.map((g) => g.name);
    let n = show.layer_groups.length + 1;
    while (taken.includes(`Folder ${n}`)) n += 1;
    engine.send({ type: "add_layer_group", name: `Folder ${n}` });
  };

  const layerRow = (layer: Layer, folder: LayerGroup | null) => {
    const effect = hello.effects.find((e) => e.id === layer.effect);
    const isOpen = open === layer.id && !drag;
    const title = layer.name ?? effect?.name ?? layer.effect;
    const notes = summary(layer, show, hello);
    const binding = show.bindings.find((b) => b.action.type === "layer" && b.action.id === layer.id);
    const hidden = !layer.enabled || (folder !== null && !folder.enabled);
    const canDrag = show.layers.length > 1 || show.layer_groups.length > 0;
    return (
      <li
        key={layer.id}
        ref={register(`l:${layer.id}`)}
        className={`layer ${hidden ? "disabled" : ""} ${isOpen ? "open" : ""} ${
          drag?.kind === "layer" && drag.id === layer.id ? "lifted" : ""
        } ${dropClass(`l:${layer.id}`)}`}
      >
        <div className="layer-row">
          {canDrag && (
            <span
              className="grip"
              title="Drag to reorder or into a folder. Effects higher up are drawn on top."
              onPointerDown={startDrag("layer", layer.id)}
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
                <span className="layer-notes">
                  {[layer.name && effect ? effect.name : null, ...notes].filter(Boolean).join(" · ")}
                </span>
              )}
            </span>
            {binding && <Keycap binding={binding} held={live.held.includes(binding.id)} />}
            <ChevronDown size={15} className="chevron" />
          </button>
          <IconButton
            label={layer.enabled ? "Hide this effect" : "Show this effect"}
            onClick={() => engine.send({ type: "update_layer", id: layer.id, enabled: !layer.enabled })}
          >
            {layer.enabled ? <Eye size={15} /> : <EyeOff size={15} />}
          </IconButton>
        </div>
        {isOpen && <LayerBody layer={layer} effect={effect} show={show} hello={hello} advanced={advanced} />}
      </li>
    );
  };

  const folderBlock = (folder: LayerGroup, layers: Layer[]) => {
    const shut = isFolded(folder.id);
    const settings = openFolder === folder.id && !drag;
    const binding = show.bindings.find((b) => b.action.type === "layer_group" && b.action.id === folder.id);
    return (
      <li
        key={`folder-${folder.id}`}
        ref={register(`b:${folder.id}`)}
        className={`folder ${folder.enabled ? "" : "disabled"} ${
          drag?.kind === "folder" && drag.id === folder.id ? "lifted" : ""
        } ${dropClass(`b:${folder.id}`)} ${dropClass(`f:${folder.id}`)}`}
      >
        <div className="folder-row" ref={register(`f:${folder.id}`)}>
          {layers.length > 0 && (
            <span
              className="grip"
              title="Drag to move the folder and everything in it"
              onPointerDown={startDrag("folder", folder.id)}
            >
              <GripVertical size={15} />
            </span>
          )}
          <button
            type="button"
            className="folder-main"
            aria-expanded={!shut}
            title={shut ? "Show the effects in this folder" : "Fold this folder shut"}
            onClick={() => toggleFolded(folder.id)}
          >
            <ChevronRight size={14} className={`chevron ${shut ? "" : "down"}`} />
            {shut ? <Folder size={15} aria-hidden /> : <FolderOpen size={15} aria-hidden />}
            <span className="folder-name">{folder.name}</span>
            <span className="folder-count">{layers.length}</span>
            {folder.opacity < 0.995 && <span className="folder-count">{Math.round(folder.opacity * 100)}%</span>}
            {folder.pinned && (
              <span className="folder-pin" title="Kept on top and through scene changes">
                <Pin size={12} aria-label="Pinned" />
              </span>
            )}
          </button>
          {binding && <Keycap binding={binding} held={live.held.includes(binding.id)} />}
          <IconButton
            label="Folder settings"
            active={settings}
            onClick={() => setOpenFolder(settings ? null : folder.id)}
          >
            <Settings2 size={15} />
          </IconButton>
          <IconButton
            label={folder.enabled ? "Hide everything in this folder" : "Show this folder's effects"}
            onClick={() => engine.send({ type: "update_layer_group", id: folder.id, enabled: !folder.enabled })}
          >
            {folder.enabled ? <Eye size={15} /> : <EyeOff size={15} />}
          </IconButton>
        </div>
        {settings && <FolderSettings folder={folder} onAdd={() => setAdding({ group: folder.id })} />}
        {!shut &&
          (layers.length > 0 ? (
            <ul className="layer-list nested">{layers.map((layer) => layerRow(layer, folder))}</ul>
          ) : (
            <p className="hint folder-empty">Empty. Drag effects onto this folder.</p>
          ))}
      </li>
    );
  };

  const addingTo = adding?.group != null ? show.layer_groups.find((g) => g.id === adding.group) : undefined;
  return (
    <aside className="panel layers">
      <div className="panel-head">
        <h2>Effects</h2>
        <label
          className="advanced-toggle"
          title="The simple layout shows what most shows need. The full layout shows every control."
        >
          Full layout
          <Switch checked={advanced} label="Full layout" onChange={(on) => fullPref.set(on)} />
        </label>
      </div>

      {adding ? (
        <div className="add-effect">
          {addingTo && <p className="hint">Adding to the folder {addingTo.name}.</p>}
          <EffectPicker
            onPick={(effect) => {
              setAdding(null);
              engine.send({ type: "add_layer", effect, group: addingTo?.id ?? null });
            }}
            onUse={(id) => {
              setAdding(null);
              engine.send({ type: "use_pattern", id, group: addingTo?.id ?? null });
            }}
          />
          <button type="button" className="text-button" onClick={() => setAdding(null)}>
            Cancel
          </button>
        </div>
      ) : (
        <div className="add-row">
          <button type="button" className="button wide" onClick={() => setAdding({ group: null })}>
            <Plus size={15} /> Add an effect
          </button>
          {advanced && (
            <button
              type="button"
              className="button square"
              aria-label="New folder"
              title="New folder, to keep effects that belong together in one place"
              onClick={newFolder}
            >
              <FolderPlus size={16} />
            </button>
          )}
        </div>
      )}
      {show.layers.length === 0 && !adding && (
        <p className="hint">No effects, so the lights are dark. Add one to get started.</p>
      )}

      <ul className={`layer-list ${drag ? "dragging" : ""} ${dropClass("end")}`}>
        {items.map((item) => (item.folder ? folderBlock(item.folder, item.layers) : layerRow(item.layer, null)))}
      </ul>
      {show.layers.length > 1 && <p className="hint">Effects higher in the list are drawn on top of those below.</p>}
    </aside>
  );
}
