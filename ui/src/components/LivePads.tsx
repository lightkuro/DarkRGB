// The live view's pads: everything that can be set off while performing,
// as buttons big enough to hit without looking, each showing its key.
// They work by mouse and touch too, so a phone can be the remote.
//
// "Customize" turns the pads into things to set up instead of press:
// give one a key, choose what a press does, reorder, add and remove.

import {
  Activity,
  ArrowLeft,
  ArrowRight,
  Clapperboard,
  Folder,
  Lightbulb,
  Palette,
  Pencil,
  Plus,
  Power,
  SkipBack,
  SkipForward,
  Snowflake,
  Sparkles,
  Trash2,
  Zap,
} from "lucide-react";
import { useCallback, useState, type ReactNode } from "react";
import {
  engine,
  onPage,
  perFrame,
  readPalette,
  sameAction,
  settingsPage,
  useCurrentScene,
  useHello,
  useLibrary,
  useLive,
  useShow,
  type Action,
  type Hello,
  type Layer,
  type Show,
  inUse as sceneInUse,
} from "../engine";
import { KeyAssign, Keycap, behaviours, modeNote } from "./Keys";
import { swatch } from "./LayerPanel";
import { MidiPad } from "./Midi";
import { Slider, Switch, useDismiss } from "./ui";
import { Transport } from "./Timeline";
import { BeatBar } from "./Beat";

const setSpeed = perFrame((speed: number) => engine.quiet({ type: "set_live", speed }));
const setSensitivity = perFrame((sensitivity: number) => engine.quiet({ type: "set_audio", sensitivity }));
const setFader = [1, 2, 3, 4].map((n) => perFrame((value: number) => engine.quiet({ type: "set_fader", n, value })));

/**
 * The faders that something follows, with what each one moves. A fader
 * nothing follows would do nothing, so it is not shown.
 */
function fadersInUse(show: Show, hello: Hello): { n: number; moves: string[] }[] {
  const moves = new Map<number, string[]>();
  for (const layer of [...show.layers, ...show.scenes.flatMap((s) => s.layers)]) {
    const effect = hello.effects.find((e) => e.id === layer.effect);
    for (const link of layer.links ?? []) {
      if (link.source.type !== "fader") continue;
      const own: Record<string, string> = { opacity: "strength", angle: "angle", hue: "colour shift" };
      const what = own[link.param] ?? effect?.params.find((p) => p.id === link.param)?.label ?? link.param;
      const line = `${layer.name ?? effect?.name ?? layer.effect}: ${what.toLowerCase()}`;
      const list = moves.get(link.source.n) ?? [];
      if (!list.includes(line)) list.push(line);
      moves.set(link.source.n, list);
    }
  }
  return [...moves.entries()].sort((a, b) => a[0] - b[0]).map(([n, list]) => ({ n, moves: list }));
}

/** The kinds of pad that have an order and a page of their own. */
const PAD_KINDS = ["layer", "layer_group", "light_group", "colours"];

/** A square to drag in: across moves fader 1, up moves fader 2. */
function XYPad({ x, y }: { x: number; y: number }) {
  const move = (e: React.PointerEvent<HTMLDivElement>) => {
    const box = e.currentTarget.getBoundingClientRect();
    const clamp = (v: number) => Math.min(1, Math.max(0, v));
    setFader[0]!(clamp((e.clientX - box.left) / box.width));
    setFader[1]!(clamp(1 - (e.clientY - box.top) / box.height));
  };
  return (
    <div
      className="xy-pad"
      role="group"
      aria-label="Faders 1 and 2 together: across is fader 1, up is fader 2"
      title="Drag: across moves fader 1, up moves fader 2"
      onPointerDown={(e) => {
        e.currentTarget.setPointerCapture(e.pointerId);
        move(e);
      }}
      onPointerMove={(e) => {
        if (e.buttons > 0) move(e);
      }}
    >
      <span className="xy-dot" style={{ left: `${x * 100}%`, top: `${(1 - y) * 100}%` }} />
    </div>
  );
}

/** A layer by id, in what is playing or in any scene. */
function findLayer(show: Show, id: number): Layer | undefined {
  return show.layers.find((l) => l.id === id) ?? show.scenes.flatMap((s) => s.layers).find((l) => l.id === id);
}

/** The name, icon and colour of the thing an action is about. */
export function describe(action: Action, show: Show, hello: Hello): { name: string; icon: ReactNode; colour?: string } {
  switch (action.type) {
    case "scene":
      return { name: show.scenes.find((s) => s.id === action.id)?.name ?? "?", icon: <Clapperboard size={15} /> };
    case "layer": {
      const layer = findLayer(show, action.id);
      const effect = hello.effects.find((e) => e.id === layer?.effect);
      return {
        name: layer ? (layer.name ?? effect?.name ?? layer.effect) : "?",
        icon: <Sparkles size={15} />,
        colour: layer ? swatch(layer, effect, hello) : undefined,
      };
    }
    case "layer_group": {
      const folder =
        show.layer_groups.find((g) => g.id === action.id) ??
        show.scenes.flatMap((s) => s.groups).find((g) => g.id === action.id);
      return { name: folder?.name ?? "?", icon: <Folder size={15} /> };
    }
    case "light_group":
      return { name: show.light_groups.find((g) => g.id === action.id)?.name ?? "?", icon: <Lightbulb size={15} /> };
    case "blackout":
      return { name: show.master.blackout ? "Lights off" : "Blackout", icon: <Power size={15} /> };
    case "tap_tempo":
      return { name: "Tap the tempo", icon: <Activity size={15} /> };
    case "hit":
      return { name: "Fire a beat", icon: <Zap size={15} /> };
    case "next_scene":
      return { name: "Next scene", icon: <SkipForward size={15} /> };
    case "prev_scene":
      return { name: "Previous scene", icon: <SkipBack size={15} /> };
    case "freeze":
      return { name: "Freeze", icon: <Snowflake size={15} /> };
    case "downbeat":
      return { name: "Beat one", icon: <Activity size={15} /> };
    case "colours": {
      const found = show.live.swatches?.find((s) => s.id === action.id);
      const palette = found ? readPalette(found.value, hello.palettes) : undefined;
      const stops = palette ? (palette.cyclic ? [...palette.stops, palette.stops[0]] : palette.stops) : [];
      return {
        name: found?.name ?? "?",
        icon: <Palette size={15} />,
        colour: stops.length ? `linear-gradient(90deg, ${stops.join(", ")})` : undefined,
      };
    }
  }
}

/**
 * Whether the thing an action switches is on, counting a key held right
 * now. Undefined when the thing is not part of what is playing.
 */
function isOn(action: Action, show: Show, frozen: boolean): boolean | undefined {
  switch (action.type) {
    case "layer": {
      const layer = show.layers.find((l) => l.id === action.id);
      const folder = show.layer_groups.find((g) => g.id === layer?.group);
      return layer ? layer.enabled && (folder?.enabled ?? true) : undefined;
    }
    case "layer_group":
      return show.layer_groups.find((g) => g.id === action.id)?.enabled;
    case "light_group":
      return show.light_groups.find((g) => g.id === action.id)?.enabled;
    case "colours":
      return show.live.swatches?.some((s) => s.id === action.id) ? show.live.colours === action.id : undefined;
    case "blackout":
      return show.master.blackout;
    case "freeze":
      return frozen;
    default:
      return false;
  }
}

interface PadProps {
  action: Action;
  show: Show;
  hello: Hello;
  note?: string;
  /** Lit regardless of the action's own state, for scenes. */
  lit?: boolean;
  waiting?: boolean;
  tone?: "danger";
  /** Customizing: a click selects the pad instead of pressing it. */
  editing?: boolean;
  selected?: boolean;
  onSelect?: () => void;
}

function Pad({ action, show, hello, note, lit, waiting, tone, editing, selected, onSelect }: PadProps) {
  const live = useLive();
  const binding = show.bindings.find((b) => sameAction(b.action, action));
  const held = binding ? live.held.includes(binding.id) : false;
  const { name, icon, colour } = describe(action, show, hello);
  const stored = isOn(action, show, live.frozen);
  // An effect that only works flashing fast does nothing while the show's
  // flash limit is on. The pad says so, and where to change it.
  const blocked =
    action.type === "layer" &&
    show.flash.limit > 0 &&
    hello.effects.find((e) => e.id === findLayer(show, action.id)?.effect)?.flash === "needed";
  const missing = stored === undefined;
  // A held key shows the opposite of what is saved; a fired pad has no
  // "on", it only goes off.
  const on = lit ?? (binding?.mode === "fire" ? false : binding?.mode === "hold" && held ? !stored : Boolean(stored));

  const down = () => {
    if (binding) engine.quiet({ type: "trigger", binding: binding.id, down: true });
    else engine.quiet({ type: "perform", action });
  };
  const up = () => {
    if (binding) engine.quiet({ type: "trigger", binding: binding.id, down: false });
  };
  const classes = `pad ${on ? "on" : ""} ${held ? "held" : ""} ${waiting ? "waiting" : ""} ${
    missing || blocked ? "missing" : ""
  } ${tone ?? ""} ${editing ? "editing" : ""} ${selected ? "selected" : ""}`;
  const body = (
    <>
      <span className="pad-top">
        <span className="pad-icon">{icon}</span>
        {binding && <Keycap binding={binding} held={held} />}
      </span>
      <span className="pad-name">{name}</span>
      <span className="pad-note">
        {blocked ? "flash limit is on" : waiting ? "on the beat…" : (note ?? modeNote(binding))}
      </span>
      {colour && <span className="pad-colour" style={{ background: colour }} />}
    </>
  );
  if (editing) {
    return (
      <button type="button" className={classes} aria-pressed={selected} onClick={onSelect}>
        {body}
      </button>
    );
  }
  return (
    <button
      type="button"
      className={classes}
      disabled={missing}
      aria-pressed={on}
      title={
        missing
          ? "Not part of the scene that is playing"
          : blocked
            ? "This effect only works flashing fast. Switch the flash limit off to use it."
            : undefined
      }
      onPointerDown={(e) => {
        // Keep getting the pointer, so letting go outside still lets go.
        e.currentTarget.setPointerCapture(e.pointerId);
        down();
      }}
      onPointerUp={up}
      onPointerCancel={up}
      onKeyDown={(e) => {
        // A key that belongs to the show has already been dealt with.
        if ((e.key === "Enter" || e.key === " ") && !e.repeat && !e.defaultPrevented) down();
      }}
      onKeyUp={(e) => {
        if ((e.key === "Enter" || e.key === " ") && !e.defaultPrevented) up();
      }}
    >
      {body}
    </button>
  );
}

/** Setting up one pad: its key, what a press does, its place, removing it. */
function PadEditor({ action, show, hello, movable }: { action: Action; show: Show; hello: Hello; movable: boolean }) {
  const binding = show.bindings.find((b) => sameAction(b.action, action));
  const { name } = describe(action, show, hello);
  const choices = behaviours(action);
  // Only pads for effects, folders and groups have an order of their own.
  const ordered = show.bindings.filter((b) => PAD_KINDS.includes(b.action.type));
  const at = binding ? ordered.findIndex((b) => b.id === binding.id) : -1;
  const move = (by: number) => {
    const next = [...ordered];
    const [pad] = next.splice(at, 1);
    next.splice(at + by, 0, pad!);
    const rest = show.bindings.filter((b) => !ordered.includes(b));
    engine.send({ type: "arrange_pads", order: [...next, ...rest].map((b) => b.id) });
  };
  return (
    <div className="pad-editor" role="group" aria-label={`Set up ${name}`}>
      <strong className="pad-editor-name">{name}</strong>
      <KeyAssign action={action} label="Key" keepPad />
      {binding && <MidiPad binding={binding} />}
      {binding && movable && (
        <div className="field">
          <span className="field-head">
            <span>Page</span>
          </span>
          <div className="segmented" role="radiogroup" aria-label={`The page ${name} is on`}>
            {[0, 1, 2, 3, 4].map((page) => (
              <button
                key={page}
                type="button"
                role="radio"
                aria-checked={(binding.page ?? 0) === page}
                className={(binding.page ?? 0) === page ? "selected" : ""}
                title={
                  page === 0
                    ? "On every page"
                    : `Only on page ${page}. Its key can then do something else on the other pages.`
                }
                onClick={() => engine.send({ type: "set_pad_page", id: binding.id, page })}
              >
                {page === 0 ? "Every" : page}
              </button>
            ))}
          </div>
        </div>
      )}
      {/* A pad with no key yet can still be told what a press does. */}
      {!binding && choices.length > 0 && (
        <div className="segmented" role="radiogroup" aria-label={`What a press does for ${name}`}>
          {choices.map((choice) => (
            <button
              key={choice.mode}
              type="button"
              role="radio"
              aria-checked={choice.mode === "toggle"}
              className={choice.mode === "toggle" ? "selected" : ""}
              title={choice.hint}
              onClick={() => engine.send({ type: "bind", key: null, label: "", action, mode: choice.mode })}
            >
              {choice.name}
            </button>
          ))}
        </div>
      )}
      {movable && binding && (
        <div className="pad-editor-actions">
          <button type="button" className="text-button" disabled={at <= 0} onClick={() => move(-1)}>
            <ArrowLeft size={13} /> Earlier
          </button>
          <button type="button" className="text-button" disabled={at >= ordered.length - 1} onClick={() => move(1)}>
            Later <ArrowRight size={13} />
          </button>
          <span className="spacer" />
          <button
            type="button"
            className="text-button danger"
            title="Removes the pad and its key. The effect itself stays."
            onClick={() => engine.send({ type: "unbind", id: binding.id })}
          >
            <Trash2 size={13} /> Remove pad
          </button>
        </div>
      )}
    </div>
  );
}

/** Things that could have a pad and do not, and the ready-made pads. */
function AddPad({ show, hello, onAdded }: { show: Show; hello: Hello; onAdded: (action: Action) => void }) {
  // Where the menu goes, when open. It is placed on the window, not in
  // the pads area, which scrolls and would cut it off.
  const [open, setOpen] = useState<{ left: number; bottom: number; maxHeight: number } | null>(null);
  const close = useCallback(() => setOpen(null), []);
  const ref = useDismiss(open !== null, close);
  const has = (action: Action) => show.bindings.some((b) => sameAction(b.action, action));
  const candidates: { title: string; actions: Action[] }[] = [
    { title: "Effects", actions: show.layers.map((l) => ({ type: "layer", id: l.id }) as Action).reverse() },
    { title: "Folders", actions: show.layer_groups.map((g) => ({ type: "layer_group", id: g.id }) as Action) },
    { title: "Groups of lights", actions: show.light_groups.map((g) => ({ type: "light_group", id: g.id }) as Action) },
  ].map((group) => ({ ...group, actions: group.actions.filter((a) => !has(a)) }));

  // Palettes that have no colour pad yet: the built-in ones and the kept ones.
  const [colours, setColours] = useState(false);
  const library = useLibrary();
  const colourChoices = [
    ...hello.palettes.map((p) => ({ name: p.name, value: p.id })),
    ...library.palettes.map((p) => ({ name: p.name, value: p.value })),
  ]
    .filter((c) => !show.live.swatches?.some((s) => s.value === c.value))
    .map((c) => {
      const palette = readPalette(c.value, hello.palettes);
      const stops = palette ? (palette.cyclic ? [...palette.stops, palette.stops[0]] : palette.stops) : [];
      return { ...c, css: `linear-gradient(90deg, ${stops.join(", ")})` };
    });

  const add = async (action: Action) => {
    setOpen(null);
    const result = await engine.request({ type: "bind", key: null, label: "", action, mode: "toggle" });
    if (result.ok) onAdded(action);
    else engine.toast("error", result.error ?? "Could not add that pad.");
  };
  const quick = (kind: "flash" | "strobe" | "pulse" | "lightning") => {
    setOpen(null);
    engine.send({ type: "add_quick_pad", kind });
  };
  return (
    <div className="add-pad" ref={ref}>
      <button
        type="button"
        className="pad add"
        aria-expanded={open !== null}
        onClick={(e) => {
          if (open) return setOpen(null);
          const rect = e.currentTarget.getBoundingClientRect();
          setOpen({
            left: Math.max(8, Math.min(rect.left, window.innerWidth - 276)),
            bottom: window.innerHeight - rect.top + 6,
            maxHeight: Math.max(160, rect.top - 24),
          });
        }}
      >
        <Plus size={18} />
        <span className="pad-name">Add a pad</span>
      </button>
      {open && (
        <div className="popover add-pad-menu" role="menu" style={open}>
          <h3>Ready-made</h3>
          <button type="button" role="menuitem" onClick={() => quick("flash")}>
            White flash <span className="hint">everything white while held</span>
          </button>
          <button type="button" role="menuitem" onClick={() => quick("strobe")}>
            Strobe <span className="hint">while held</span>
          </button>
          <button type="button" role="menuitem" onClick={() => quick("pulse")}>
            Colour pulse <span className="hint">a burst on each press</span>
          </button>
          <button type="button" role="menuitem" onClick={() => setColours(!colours)}>
            Colours… <span className="hint">a pad that recolours everything that is playing</span>
          </button>
          {colours &&
            colourChoices.map((choice) => (
              <button
                key={choice.value}
                type="button"
                role="menuitem"
                className="colour-choice"
                onClick={() => {
                  setOpen(null);
                  setColours(false);
                  engine.send({ type: "add_colour_pad", name: choice.name, value: choice.value });
                }}
              >
                <span className="swatch" style={{ background: choice.css }} />
                {choice.name}
              </button>
            ))}
          <button type="button" role="menuitem" onClick={() => quick("lightning")}>
            Lightning{" "}
            <span className="hint">
              {show.flash.limit > 0 ? "very fast flashes; needs the flash limit off" : "very fast flashes on each press"}
            </span>
          </button>
          {candidates
            .filter((group) => group.actions.length > 0)
            .map((group) => (
              <div key={group.title}>
                <h3>{group.title}</h3>
                {group.actions.map((action) => (
                  <button
                    key={`${action.type}-${"id" in action ? action.id : ""}`}
                    type="button"
                    role="menuitem"
                    onClick={() => void add(action)}
                  >
                    {describe(action, show, hello).name}
                  </button>
                ))}
              </div>
            ))}
        </div>
      )}
    </div>
  );
}

const BUILT_IN: Action[] = [
  { type: "blackout" },
  { type: "tap_tempo" },
  { type: "hit" },
  { type: "prev_scene" },
  { type: "next_scene" },
  { type: "freeze" },
  { type: "downbeat" },
];

export function LivePads() {
  const show = useShow();
  const hello = useHello();
  const live = useLive();
  const current = useCurrentScene();
  const [customizing, setCustomizing] = useState(false);
  const [selected, setSelected] = useState<Action | null>(null);
  if (!show || !hello) return null;

  const page = show.live.page ?? 1;
  const allPads = show.bindings.filter((b) => PAD_KINDS.includes(b.action.type));
  const pads = allPads.filter((b) => onPage(b, page));
  const paged = allPads.some((b) => b.page);
  const inUse = fadersInUse(show, hello);
  const xy = inUse.some((f) => f.n === 1) && inUse.some((f) => f.n === 2);
  const isSelected = (action: Action) => customizing && selected !== null && sameAction(selected, action);
  const edit = (action: Action) => ({
    editing: customizing,
    selected: isSelected(action),
    onSelect: () => setSelected(isSelected(action) ? null : action),
  });
  // The pad being set up may have been removed meanwhile.
  const selectedExists =
    selected !== null &&
    (BUILT_IN.some((a) => sameAction(a, selected)) ||
      (selected.type === "scene" && show.scenes.some((s) => s.id === selected.id)) ||
      allPads.some((b) => sameAction(b.action, selected)));
  const limited = show.flash.limit > 0;

  return (
    <section className={`pads ${customizing ? "customizing" : ""}`} aria-label="Live pads">
      <div className="pads-bar">
        <button
          type="button"
          className={`chip ${customizing ? "selected" : ""}`}
          aria-pressed={customizing}
          title="Set up the pads: keys, what a press does, their order"
          onClick={() => {
            setCustomizing(!customizing);
            setSelected(null);
          }}
        >
          <Pencil size={13} /> {customizing ? "Done" : "Customize"}
        </button>
        {!customizing && <BeatBar compact />}
        {/* A show with a timeline is played, and recorded, from here too. */}
        {!customizing && show.timeline && <Transport compact />}
        {customizing && (
          <span className="hint">
            {selectedExists ? "Press a key to assign it, or pick another pad." : "Pick a pad to set it up."}
          </span>
        )}
        {(paged || customizing) && (
          <div className="segmented pages" role="radiogroup" aria-label="Page of pads">
            {[1, 2, 3, 4].map((n) => (
              <button
                key={n}
                type="button"
                role="radio"
                aria-checked={page === n}
                className={page === n ? "selected" : ""}
                title={`Page ${n} of the pads`}
                onClick={() => engine.send({ type: "set_page", page: n })}
              >
                {n}
              </button>
            ))}
          </div>
        )}
        <span className="spacer" />
        <div className="live-faders">
          {xy && <XYPad x={show.live.faders[0] ?? 0} y={show.live.faders[1] ?? 0} />}
          {inUse.map(({ n, moves }) => (
            <div key={n} className="live-fader" title={`Moves ${moves.join("; ")}`}>
              <Slider
                label={`Fader ${n}`}
                min={0}
                max={1}
                value={show.live.faders[n - 1] ?? 0}
                format={(v) => `${Math.round(v * 100)}%`}
                onChange={setFader[n - 1]!}
              />
            </div>
          ))}
          <Slider
            label="Speed"
            min={-2}
            max={2}
            step={0.05}
            // The slider works in doublings, so half and double are the same distance from normal.
            value={Math.log2(show.live.speed)}
            format={(v) => `${(2 ** v).toFixed(2)}×`}
            onChange={(v) => setSpeed(2 ** v)}
          />
          <Slider
            label="Sound sensitivity"
            min={0.25}
            max={4}
            step={0.05}
            value={show.audio.sensitivity}
            format={(v) => `${v.toFixed(2)}×`}
            onChange={setSensitivity}
          />
          {show.scenes.length > 1 && (
            <div className="field inline">
              <span>Autopilot</span>
              <Switch
                checked={show.live.auto.enabled}
                label="Autopilot"
                onChange={(auto) => engine.send({ type: "set_live", auto })}
              />
            </div>
          )}
          <div className="field inline">
            <span>Flash limit</span>
            <Switch
              checked={limited}
              label="Limit flashing"
              onChange={(on) => engine.send({ type: "set_flash", limit: on ? 3 : 0 })}
            />
          </div>
          {show.live.speed !== 1 && (
            <button type="button" className="text-button" onClick={() => engine.send({ type: "set_live", speed: 1 })}>
              Back to normal speed
            </button>
          )}
        </div>
      </div>
      {customizing && selected && selectedExists && (
        <PadEditor
          key={`${selected.type}-${"id" in selected ? selected.id : ""}`}
          action={selected}
          show={show}
          hello={hello}
          movable={PAD_KINDS.includes(selected.type)}
        />
      )}

      <div className="pad-group">
        <h2>Scenes</h2>
        {show.scenes.length === 0 ? (
          <p className="hint">
            No scenes yet. In the Edit view, set up a look you like and use "Save as scene" at the bottom. The first
            ten scenes get the number keys.
          </p>
        ) : (
          <div className="pad-grid">
            {show.scenes.filter((scene) => sceneInUse(show, scene)).map((scene) => {
              const action: Action = { type: "scene", id: scene.id };
              return (
                <Pad
                  key={scene.id}
                  action={action}
                  show={show}
                  hello={hello}
                  lit={current === scene.id}
                  waiting={live.pending_scene === scene.id}
                  {...edit(action)}
                />
              );
            })}
          </div>
        )}
      </div>

      <div className="pad-group">
        <h2>Effects and lights</h2>
        <div className="pad-grid">
          {pads.map((binding) => (
            <Pad key={binding.id} action={binding.action} show={show} hello={hello} {...edit(binding.action)} />
          ))}
          {customizing && <AddPad show={show} hello={hello} onAdded={setSelected} />}
        </div>
        {pads.length === 0 && !customizing && (
          <p className="hint">
            Nothing here yet. Choose Customize, then "Add a pad", for a white flash, a strobe, or any effect, folder
            or group of lights you want under your fingers.
          </p>
        )}
      </div>

      <div className="pad-group">
        <h2>
          Show
          <button type="button" className="text-button" onClick={() => settingsPage.set("live")}>
            More settings…
          </button>
        </h2>
        <div className="pad-grid">
          {BUILT_IN.map((action) => (
            <Pad
              key={action.type}
              action={action}
              show={show}
              hello={hello}
              tone={action.type === "blackout" ? "danger" : undefined}
              note={
                action.type === "tap_tempo"
                  ? live.tapped_bpm > 0
                    ? `${Math.round(live.tapped_bpm)} BPM`
                    : "following the music"
                  : undefined
              }
              lit={action.type === "tap_tempo" ? live.tapped_bpm > 0 : undefined}
              {...edit(action)}
            />
          ))}
        </div>
      </div>
    </section>
  );
}
