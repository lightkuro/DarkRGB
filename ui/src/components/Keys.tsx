// Tying a keyboard key to something in the show, and showing which key
// a thing has.

import { Keyboard, X } from "lucide-react";
import { useEffect, useState } from "react";
import { engine, sameAction, useShow, type Action, type Binding, type KeyMode } from "../engine";
import { cancelPick, keyLabel, pickKey } from "../keys";

/** Actions with something to flip, which a key can therefore hold. */
export function isSwitch(action: Action): boolean {
  return ["layer", "layer_group", "light_group", "blackout"].includes(action.type);
}

export function useBinding(action: Action): Binding | undefined {
  return useShow()?.bindings.find((b) => sameAction(b.action, action));
}

/** A small picture of a key. */
export function Keycap({ binding, held }: { binding: Pick<Binding, "key" | "label">; held?: boolean }) {
  return <kbd className={`keycap ${held ? "held" : ""}`}>{keyLabel(binding)}</kbd>;
}

/**
 * One row: which key a thing has, with a button to pick one by pressing
 * it. Things that can be switched also get the choice between flipping on
 * each press and only while the key is down.
 */
export function KeyAssign({ action, label = "Keyboard key" }: { action: Action; label?: string }) {
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
    const taken = engine.show.get()?.bindings.find((b) => b.key === picked.key && !sameAction(b.action, action));
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

  const setMode = (mode: KeyMode) => binding && engine.send({ type: "update_binding", id: binding.id, mode });

  return (
    <div className="field key-assign">
      <div className="key-assign-row">
        <span className="key-assign-label">{label}</span>
        {picking ? (
          <button type="button" className="button small picking" onClick={() => cancelPick()}>
            {refused ? "Not that one. Another key…" : "Press a key…"}
          </button>
        ) : binding ? (
          <>
            <button type="button" className="key-change" title="Press to pick a different key" onClick={() => void pick()}>
              <Keycap binding={binding} />
            </button>
            <button
              type="button"
              className="icon-button"
              aria-label={`Remove the key for ${label}`}
              title="Remove this key"
              onClick={() => engine.send({ type: "unbind", id: binding.id })}
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
      {binding && !picking && isSwitch(action) && (
        <div className="segmented" role="radiogroup" aria-label={`What the key for ${label} does`}>
          <button
            type="button"
            role="radio"
            aria-checked={binding.mode === "toggle"}
            className={binding.mode === "toggle" ? "selected" : ""}
            title="Each press switches it on or off"
            onClick={() => setMode("toggle")}
          >
            Each press
          </button>
          <button
            type="button"
            role="radio"
            aria-checked={binding.mode === "hold"}
            className={binding.mode === "hold" ? "selected" : ""}
            title="Switched only while the key is held down"
            onClick={() => setMode("hold")}
          >
            While held
          </button>
        </div>
      )}
    </div>
  );
}
