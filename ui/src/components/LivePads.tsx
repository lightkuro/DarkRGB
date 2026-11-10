// The live view's pads: everything that can be set off while performing,
// as buttons big enough to hit without looking, each showing its key.
// They work by mouse and touch too, so a phone can be the remote.

import { Activity, Clapperboard, Folder, Lightbulb, Power, Sparkles, Zap } from "lucide-react";
import type { ReactNode } from "react";
import {
  engine,
  sameAction,
  settingsPage,
  useCurrentScene,
  useHello,
  useLive,
  useShow,
  type Action,
  type Binding,
  type Command,
  type Show,
} from "../engine";
import { Keycap } from "./Keys";

interface PadProps {
  name: string;
  note?: string;
  icon: ReactNode;
  binding?: Binding;
  /** Lit: the thing is on right now. */
  on?: boolean;
  /** Waiting for the beat. */
  waiting?: boolean;
  /** The thing is not part of what is playing, so the pad does nothing. */
  missing?: boolean;
  tone?: "danger";
  /** What to send when there is no key to trigger. */
  press?: Command;
}

function Pad({ name, note, icon, binding, on, waiting, missing, tone, press }: PadProps) {
  const live = useLive();
  const held = binding ? live.held.includes(binding.id) : false;
  const down = () => {
    if (binding) engine.quiet({ type: "trigger", binding: binding.id, down: true });
    else if (press) engine.quiet(press);
  };
  const up = () => {
    if (binding) engine.quiet({ type: "trigger", binding: binding.id, down: false });
  };
  return (
    <button
      type="button"
      className={`pad ${on ? "on" : ""} ${held ? "held" : ""} ${waiting ? "waiting" : ""} ${missing ? "missing" : ""} ${
        tone ?? ""
      }`}
      disabled={missing}
      aria-pressed={on}
      title={missing ? "Not part of the scene that is playing" : undefined}
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
      <span className="pad-top">
        <span className="pad-icon">{icon}</span>
        {binding && <Keycap binding={binding} held={held} />}
      </span>
      <span className="pad-name">{name}</span>
      <span className="pad-note">{waiting ? "on the beat…" : (note ?? "")}</span>
    </button>
  );
}

/** Whether the thing a key switches is on, counting a key held right now. */
function isOn(action: Action, show: Show, heldNow: boolean, mode: Binding["mode"]): boolean | undefined {
  let stored: boolean | undefined;
  switch (action.type) {
    case "layer": {
      const layer = show.layers.find((l) => l.id === action.id);
      const folder = show.layer_groups.find((g) => g.id === layer?.group);
      stored = layer ? layer.enabled && (folder?.enabled ?? true) : undefined;
      break;
    }
    case "layer_group":
      stored = show.layer_groups.find((g) => g.id === action.id)?.enabled;
      break;
    case "light_group":
      stored = show.light_groups.find((g) => g.id === action.id)?.enabled;
      break;
    case "blackout":
      stored = show.master.blackout;
      break;
    default:
      return undefined;
  }
  if (stored === undefined) return undefined;
  return heldNow && mode === "hold" ? !stored : stored;
}

export function LivePads() {
  const show = useShow();
  const hello = useHello();
  const live = useLive();
  const current = useCurrentScene();
  if (!show || !hello) return null;
  const keyFor = (action: Action) => show.bindings.find((b) => sameAction(b.action, action));
  const modeNote = (binding: Binding | undefined) => (binding?.mode === "hold" ? "while held" : "");

  const switches = show.bindings.filter((b) => ["layer", "layer_group", "light_group"].includes(b.action.type));
  const blackout = keyFor({ type: "blackout" });
  const tap = keyFor({ type: "tap_tempo" });
  const hit = keyFor({ type: "hit" });

  return (
    <section className="pads" aria-label="Live pads">
      <div className="pad-group">
        <h2>Scenes</h2>
        {show.scenes.length === 0 ? (
          <p className="hint">
            No scenes yet. In the Edit view, set up a look you like and use "Save as scene" at the bottom. The first
            ten scenes get the number keys.
          </p>
        ) : (
          <div className="pad-grid">
            {show.scenes.map((scene) => (
              <Pad
                key={scene.id}
                name={scene.name}
                icon={<Clapperboard size={15} />}
                binding={keyFor({ type: "scene", id: scene.id })}
                on={current === scene.id}
                waiting={live.pending_scene === scene.id}
                press={{ type: "recall_scene", id: scene.id }}
              />
            ))}
          </div>
        )}
      </div>

      <div className="pad-group">
        <h2>Effects and lights</h2>
        {switches.length === 0 ? (
          <p className="hint">
            Give an effect, a folder or a group of lights a key (open it in the Edit view and choose "Assign") and it
            appears here, to flash or switch while you play.
          </p>
        ) : (
          <div className="pad-grid">
            {switches.map((binding) => {
              const action = binding.action;
              const on = isOn(action, show, live.held.includes(binding.id), binding.mode);
              let name = "?";
              let icon: ReactNode = <Sparkles size={15} />;
              if (action.type === "layer") {
                const layer =
                  show.layers.find((l) => l.id === action.id) ??
                  show.scenes.flatMap((s) => s.layers).find((l) => l.id === action.id);
                name = layer ? (layer.name ?? hello.effects.find((e) => e.id === layer.effect)?.name ?? layer.effect) : "?";
              } else if (action.type === "layer_group") {
                name =
                  (
                    show.layer_groups.find((g) => g.id === action.id) ??
                    show.scenes.flatMap((s) => s.groups).find((g) => g.id === action.id)
                  )?.name ?? "?";
                icon = <Folder size={15} />;
              } else if (action.type === "light_group") {
                name = show.light_groups.find((g) => g.id === action.id)?.name ?? "?";
                icon = <Lightbulb size={15} />;
              }
              return (
                <Pad
                  key={binding.id}
                  name={name}
                  note={modeNote(binding)}
                  icon={icon}
                  binding={binding}
                  on={on ?? false}
                  missing={on === undefined}
                />
              );
            })}
          </div>
        )}
      </div>

      <div className="pad-group">
        <h2>
          Show
          <button type="button" className="text-button" onClick={() => settingsPage.set("live")}>
            Keys and tempo…
          </button>
        </h2>
        <div className="pad-grid">
          <Pad
            name={show.master.blackout ? "Lights off" : "Blackout"}
            note={modeNote(blackout)}
            icon={<Power size={15} />}
            binding={blackout}
            on={isOn({ type: "blackout" }, show, blackout ? live.held.includes(blackout.id) : false, blackout?.mode ?? "toggle")}
            tone="danger"
            press={{ type: "set_master", blackout: !show.master.blackout }}
          />
          <Pad
            name="Tap the tempo"
            note={live.tapped_bpm > 0 ? `${Math.round(live.tapped_bpm)} BPM` : "following the music"}
            icon={<Activity size={15} />}
            binding={tap}
            on={live.tapped_bpm > 0}
            press={{ type: "tap" }}
          />
          <Pad name="Fire a beat" icon={<Zap size={15} />} binding={hit} press={{ type: "hit" }} />
        </div>
      </div>
    </section>
  );
}
