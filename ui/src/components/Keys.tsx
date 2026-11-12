// Tying a keyboard key to something in the show, and showing which key
// a thing has. Everything that has a key is also a pad in the Live view.

import { Keyboard, X } from "lucide-react";
import { useEffect, useState } from "react";
import { engine, sameAction, useShow, type Action, type Binding, type KeyMode } from "../engine";
import { cancelPick, keyLabel, pickKey } from "../keys";

/** Actions with something to flip. */
export function isSwitch(action: Action): boolean {
  return ["layer", "layer_group", "light_group", "blackout", "freeze", "colours"].includes(action.type);
}

/** What a press can do for an action, with the words for each choice. */
export function behaviours(action: Action): { mode: KeyMode; name: string; hint: string }[] {
  if (action.type === "scene") {
    return [
      { mode: "toggle", name: "Switch to it", hint: "A press changes to this scene" },
      { mode: "hold", name: "While held", hint: "Shows this scene only while held down, then goes back" },
    ];
  }
  if (!isSwitch(action)) return [];
  const list: { mode: KeyMode; name: string; hint: string }[] = [
    { mode: "toggle", name: "Each press", hint: "Each press switches it on or off" },
    { mode: "hold", name: "While held", hint: "Switched only while held down" },
  ];
  if (action.type === "layer" || action.type === "layer_group") {
    list.push({ mode: "fire", name: "Fire it", hint: "Each press sets off its reaction, as if a beat had hit" });
  }
  return list;
}

/** A few words for what a pad's behaviour is, when it is not the plain one. */
export function modeNote(binding: Binding | undefined): string {
  if (binding?.mode === "hold") return "while held";
  if (binding?.mode === "fire") return "fires it";
  return "";
}

export function useBinding(action: Action): Binding | undefined {
  return useShow()?.bindings.find((b) => sameAction(b.action, action));
}

/** A small picture of a key. Nothing for a pad without one. */
export function Keycap({ binding, held }: { binding: Pick<Binding, "key" | "label">; held?: boolean }) {
  if (!binding.key) return null;
  return <kbd className={`keycap ${held ? "held" : ""}`}>{keyLabel(binding)}</kbd>;
}

/** The choice of what a press does, for actions that have a choice. */
export function BehaviourChoice({ action, binding, label }: { action: Action; binding: Binding; label: string }) {
  const choices = behaviours(action);
  if (choices.length === 0) return null;
  return (
    <div className="segmented" role="radiogroup" aria-label={`What a press does for ${label}`}>
      {choices.map((choice) => (
        <button
          key={choice.mode}
          type="button"
          role="radio"
          aria-checked={binding.mode === choice.mode}
          className={binding.mode === choice.mode ? "selected" : ""}
          title={choice.hint}
          onClick={() => engine.send({ type: "update_binding", id: binding.id, mode: choice.mode })}
        >
          {choice.name}
        </button>
      ))}
    </div>
  );
}

/**
 * One row: which key a thing has, with a button to pick one by pressing
 * it, and what a press does.
 *
 * `keepPad` is for the Live view, where taking the key away leaves the
 * pad for mouse and touch. Elsewhere removing the key removes the pad.
 */
export function KeyAssign({
  action,
  label = "Keyboard key",
  keepPad,
}: {
  action: Action;
  label?: string;
  keepPad?: boolean;
}) {
  const binding = useBinding(action);
  const [picking, setPicking] = useState(false);
  const [refused, setRefused] = useState(false);
  // Stop waiting for a key if this row goes away.
  useEffect(() => () => cancelPick(), []);

  const pick = async () => {
    setPicking(true);
    setRefused(false);
    const picked = await pickKey(() => setRefused(true));
    setPicking(false);
    setRefused(false);
    if (!picked) return;
    const own = engine.show.get()?.bindings.find((b) => sameAction(b.action, action));
    const taken = engine
      .show.get()
      ?.bindings.find(
        (b) =>
          b.key === picked.key &&
          !sameAction(b.action, action) &&
          // A key on another page is not in the way.
          (!b.page || !own?.page || b.page === own.page),
      );
    const result = await engine.request({
      type: "bind",
      key: picked.key,
      label: picked.label,
      action,
      mode: binding?.mode ?? "toggle",
    });
    if (!result.ok) engine.toast("error", result.error ?? "Could not use that key.");
    else if (taken) engine.toast("info", `${keyLabel(picked)} was in use and now does this instead.`);
  };

  const hasKey = Boolean(binding?.key);
  return (
    <div className="field key-assign">
      <div className="key-assign-row">
        <span className="key-assign-label">{label}</span>
        {picking ? (
          <button type="button" className="button small picking" onClick={() => cancelPick()}>
            {refused ? "Not that one. Another key…" : "Press a key…"}
          </button>
        ) : binding && hasKey ? (
          <>
            <button type="button" className="key-change" title="Press to pick a different key" onClick={() => void pick()}>
              <Keycap binding={binding} />
            </button>
            <button
              type="button"
              className="icon-button"
              aria-label={`Remove the key for ${label}`}
              title="Remove this key"
              onClick={() => engine.send(keepPad ? { type: "clear_key", id: binding.id } : { type: "unbind", id: binding.id })}
            >
              <X size={14} />
            </button>
          </>
        ) : (
          <button type="button" className="button small quiet" onClick={() => void pick()}>
            <Keyboard size={14} /> Assign
          </button>
        )}
      </div>
      {picking && <p className="hint">A letter, a number, space or punctuation. Escape cancels.</p>}
      {binding && !picking && <BehaviourChoice action={action} binding={binding} label={label} />}
    </div>
  );
}
