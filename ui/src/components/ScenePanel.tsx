// Scenes as things to work on: the heading of the effects panel, which
// says which scene is in it and saves it, and the window where scenes
// are found, added, filed in sets and opened to be changed.

import { Clapperboard, EyeOff, FolderPlus, Pencil, Play, Plus, Search, Trash2, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import {
  Store,
  engine,
  inUse,
  useBaseScene,
  useCurrentScene,
  useHello,
  useLibrary,
  useLive,
  useShow,
  type Scene,
  type Show,
} from "../engine";
import { useSyncExternalStore } from "react";
import { EffectThumb } from "./EffectTiles";
import { ConfirmButton } from "./ui";

/** Whether the window for managing scenes is open. */
export const managingScenes = new Store<boolean>(false);
const useManaging = () => useSyncExternalStore(managingScenes.subscribe, managingScenes.get);

/** A name that no scene has yet, counting up from "Scene 1". */
function freeName(show: Show, stem = "Scene"): string {
  let n = show.scenes.length + 1;
  while (show.scenes.some((s) => s.name === `${stem} ${n}`)) n += 1;
  return `${stem} ${n}`;
}

/** A button that turns into a name to type, in the same place. */
export function NameButton({
  label,
  placeholder,
  className = "text-button",
  start = "",
  onName,
  children,
}: {
  label: string;
  placeholder: string;
  className?: string;
  /** What the field starts with, selected. */
  start?: string;
  onName: (name: string) => void;
  children?: React.ReactNode;
}) {
  const [name, setName] = useState<string | null>(null);
  const input = useRef<HTMLInputElement>(null);
  useEffect(() => {
    if (name !== null) input.current?.select();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [name === null]);
  if (name === null) {
    return (
      <button type="button" className={className} onClick={() => setName(start)}>
        {children}
        {label}
      </button>
    );
  }
  const done = () => {
    if (name.trim()) onName(name.trim());
    setName(null);
  };
  return (
    <form
      className="name-form"
      onSubmit={(e) => {
        e.preventDefault();
        done();
      }}
    >
      <input
        ref={input}
        value={name}
        maxLength={80}
        placeholder={placeholder}
        aria-label={placeholder}
        onChange={(e) => setName(e.target.value)}
        onKeyDown={(e) => {
          e.stopPropagation();
          if (e.key === "Escape") setName(null);
        }}
        onBlur={() => !name.trim() && setName(null)}
      />
      <button type="submit" className="button small" disabled={!name.trim()}>
        Save
      </button>
      <button type="button" className="icon-button" aria-label="Cancel" onClick={() => setName(null)}>
        <X size={14} />
      </button>
    </form>
  );
}

/**
 * The top of the effects panel: which scene the effects below belong to,
 * whether the lights are playing it, and how to save it.
 */
export function SceneHead() {
  const show = useShow();
  const live = useLive();
  const current = useCurrentScene();
  const base = useBaseScene();
  if (!show) return null;
  const find = (id: number | null | undefined) => (id == null ? undefined : show.scenes.find((s) => s.id === id));

  if (live.blind) {
    const editing = find(live.editing);
    // The scene the lights are on, if they are on one as it was saved.
    const onAir = find(live.on_air_scene) ?? find(base);
    return (
      <div className="scene-head blind">
        <div className="scene-title">
          <EyeOff size={15} aria-hidden />
          <strong>{editing?.name ?? "A scene"}</strong>
          <span className="scene-state">not on the lights</span>
        </div>
        <p className="hint">
          You are changing this scene out of sight. The lights go on playing {onAir ? onAir.name : "what they were"};
          the stage shows what you are changing.
        </p>
        <div className="scene-actions">
          <button type="button" className="button small" onClick={() => engine.send({ type: "finish_edit", play: false })}>
            Done
          </button>
          <button type="button" className="text-button" onClick={() => engine.send({ type: "finish_edit", play: true })}>
            <Play size={13} /> Put it on the lights
          </button>
        </div>
      </div>
    );
  }

  const saved = find(current);
  // Not what any scene holds: it came from this one and has been changed since.
  const from = saved ? undefined : find(base);
  return (
    <div className="scene-head">
      <div className="scene-title">
        <Clapperboard size={15} aria-hidden />
        <strong>{saved?.name ?? from?.name ?? "Not a scene yet"}</strong>
        <span className="scene-state">{saved ? "playing" : from ? "changed since it was saved" : "playing"}</span>
      </div>
      <p className="hint">
        {saved
          ? "The effects below are this scene. Change them and the lights change; save to keep the change."
          : from
            ? "The effects below are what is playing. Save the changes, or keep them as a scene of their own."
            : "The effects below are what is playing. Save them as a scene to come back to with one press."}
      </p>
      <div className="scene-actions">
        {from && (
          <button type="button" className="button small" onClick={() => engine.send({ type: "update_scene", id: from.id })}>
            Save changes to {from.name}
          </button>
        )}
        <NameButton
          label={saved || from ? "Save as a new scene" : "Save as scene"}
          placeholder="Scene name"
          className={saved || from ? "text-button" : "button small"}
          start={freeName(show)}
          onName={(name) => engine.send({ type: "save_scene", name })}
        >
          <Plus size={13} />
        </NameButton>
        <button type="button" className="text-button" onClick={() => managingScenes.set(true)}>
          All scenes…
        </button>
      </div>
    </div>
  );
}

/**
 * The effect whose picture stands for a set of effects: the lowest one
 * that shows strongly, which is the body of the look. What lies over it
 * is usually decoration.
 */
function faceOf(layers: { effect: string; opacity?: number }[]): string {
  return (layers.find((l) => (l.opacity ?? 1) >= 0.5) ?? layers[layers.length - 1])?.effect ?? "";
}

function SceneRow({ show, scene, playing, editing }: { show: Show; scene: Scene; playing: boolean; editing: boolean }) {
  const sets = show.scene_sets ?? [];
  return (
    <li className={`manage-row${playing ? " playing" : ""}`}>
      <EffectThumb effect={faceOf(scene.layers)} playing={false} className="small" />
      <span className="manage-name">
        <strong>{scene.name}</strong>
        <span className="hint">
          {scene.layers.length} {scene.layers.length === 1 ? "effect" : "effects"}
          {playing ? " · playing" : ""}
          {editing ? " · being changed" : ""}
        </span>
      </span>
      {sets.length > 0 && (
        <select
          aria-label={`The set ${scene.name} is in`}
          value={scene.set ?? ""}
          onChange={(e) =>
            engine.send({ type: "file_scene", scene: scene.id, set: e.target.value === "" ? null : Number(e.target.value) })
          }
        >
          <option value="">Every set</option>
          {sets.map((set) => (
            <option key={set.id} value={set.id}>
              {set.name}
            </option>
          ))}
        </select>
      )}
      <button
        type="button"
        className="text-button"
        title={`Put ${scene.name} on the lights`}
        onClick={() => engine.send({ type: "recall_scene", id: scene.id })}
      >
        <Play size={13} /> Play
      </button>
      <button
        type="button"
        className="text-button"
        title="Open it in the effects panel to change it, while the lights go on playing what they are playing"
        onClick={() => {
          engine.send({ type: "edit_scene", id: scene.id });
          managingScenes.set(false);
        }}
      >
        <Pencil size={13} /> Change
      </button>
      <ConfirmButton
        label={`Delete ${scene.name}`}
        confirmLabel="Delete?"
        onConfirm={() => engine.send({ type: "delete_scene", id: scene.id })}
      >
        <Trash2 size={15} />
      </ConfirmButton>
    </li>
  );
}

/** Finding, adding, filing and opening scenes. */
export function SceneManager() {
  const open = useManaging();
  const show = useShow();
  const hello = useHello();
  const live = useLive();
  const library = useLibrary();
  const current = useCurrentScene();
  const [tab, setTab] = useState<"have" | "add">("have");
  const [search, setSearch] = useState("");
  useEffect(() => {
    if (!open) return;
    const close = (e: KeyboardEvent) => e.key === "Escape" && managingScenes.set(false);
    window.addEventListener("keydown", close);
    return () => window.removeEventListener("keydown", close);
  }, [open]);
  if (!open || !show || !hello) return null;

  const sets = show.scene_sets ?? [];
  const using = show.live.scene_set ?? null;
  const words = search.trim().toLowerCase();
  const matches = (text: string) => words === "" || text.toLowerCase().includes(words);
  const mine = show.scenes.filter((s) => inUse(show, s) && matches(s.name));
  const hidden = show.scenes.length - show.scenes.filter((s) => inUse(show, s)).length;
  const presets = hello.looks.filter((l) => matches(`${l.name} ${l.description}`));
  const kept = library.patterns.filter((p) => p.look && matches(p.name));
  const has = (name: string) => show.scenes.some((s) => s.name === name);
  const add = async (command: Parameters<typeof engine.request>[0], name: string) => {
    const result = await engine.request(command);
    if (result.ok) engine.toast("info", `${name} is now one of this show's scenes.`);
    else engine.toast("error", result.error ?? "Could not add that scene.");
  };

  return (
    <div
      className="dialog-backdrop"
      onPointerDown={(e) => {
        if (e.target === e.currentTarget) managingScenes.set(false);
      }}
    >
      <div className="dialog library scene-manager" role="dialog" aria-modal="true" aria-label="Scenes">
        <div className="settings-head">
          <h2>
            <Clapperboard size={16} aria-hidden /> Scenes
          </h2>
          <button type="button" className="icon-button" aria-label="Close" onClick={() => managingScenes.set(false)}>
            <X size={16} />
          </button>
        </div>
        <div className="library-body">
          <div className="manage-top">
            <div className="segmented" role="tablist" aria-label="Which scenes">
              <button type="button" role="tab" aria-selected={tab === "have"} className={tab === "have" ? "selected" : ""} onClick={() => setTab("have")}>
                In this show ({show.scenes.length})
              </button>
              <button type="button" role="tab" aria-selected={tab === "add"} className={tab === "add" ? "selected" : ""} onClick={() => setTab("add")}>
                Add a scene
              </button>
            </div>
            <label className="picker-search">
              <Search size={13} aria-hidden />
              <input
                type="search"
                placeholder="Find a scene"
                aria-label="Find a scene"
                value={search}
                onChange={(e) => setSearch(e.target.value)}
                onKeyDown={(e) => e.key !== "Escape" && e.stopPropagation()}
              />
            </label>
          </div>

          {tab === "have" && (
            <>
              <div className="manage-sets">
                <span className="hint">Set in use</span>
                <div className="chips">
                  <button
                    type="button"
                    className={`chip ${using === null ? "selected" : ""}`}
                    aria-pressed={using === null}
                    onClick={() => engine.send({ type: "use_scene_set", id: null })}
                  >
                    All scenes
                  </button>
                  {sets.map((set) => (
                    <span key={set.id} className="set-chip">
                      <button
                        type="button"
                        className={`chip ${using === set.id ? "selected" : ""}`}
                        aria-pressed={using === set.id}
                        onClick={() => engine.send({ type: "use_scene_set", id: set.id })}
                      >
                        {set.name}
                      </button>
                      {using === set.id && (
                        <>
                          <NameButton
                            label=""
                            placeholder="Name of the set"
                            className="icon-button"
                            start={set.name}
                            onName={(name) => engine.send({ type: "rename_scene_set", id: set.id, name })}
                          >
                            <Pencil size={13} aria-label={`Rename ${set.name}`} />
                          </NameButton>
                          <ConfirmButton
                            label={`Remove the set ${set.name}. Its scenes stay.`}
                            confirmLabel="Remove set?"
                            onConfirm={() => engine.send({ type: "remove_scene_set", id: set.id })}
                          >
                            <Trash2 size={14} />
                          </ConfirmButton>
                        </>
                      )}
                    </span>
                  ))}
                  <NameButton
                    label="New set"
                    placeholder="Name of the set"
                    className="chip add"
                    onName={(name) => engine.send({ type: "add_scene_set", name })}
                  >
                    <FolderPlus size={13} />
                  </NameButton>
                </div>
                <p className="hint">
                  A set is a group of scenes for one occasion, such as dinner or a party. The scene buttons, the
                  next and previous keys and autopilot keep to the set in use. A scene filed in "Every set" is
                  always offered.
                </p>
              </div>
              {show.scenes.length === 0 ? (
                <p className="hint">No scenes yet. Add some from the other tab, or save what is playing.</p>
              ) : (
                <ul className="manage-list">
                  {mine.map((scene) => (
                    <SceneRow
                      key={scene.id}
                      show={show}
                      scene={scene}
                      playing={!live.blind ? current === scene.id : live.on_air_scene === scene.id}
                      editing={live.blind === true && live.editing === scene.id}
                    />
                  ))}
                </ul>
              )}
              {hidden > 0 && (
                <p className="hint">
                  {hidden} more {hidden === 1 ? "scene is" : "scenes are"} in other sets. Choose "All scenes" to see{" "}
                  {hidden === 1 ? "it" : "them"}.
                </p>
              )}
            </>
          )}

          {tab === "add" && (
            <>
              <div className="manage-new">
                <NameButton
                  label="New empty scene"
                  placeholder="Scene name"
                  className="button small"
                  start={freeName(show)}
                  onName={(name) => void add({ type: "add_scene", name }, name)}
                >
                  <Plus size={13} />
                </NameButton>
                <span className="hint">Starts with one effect. Open it with "Change" to build it without touching the lights.</span>
              </div>
              {kept.length > 0 && (
                <>
                  <h3>Kept by you</h3>
                  <ul className="preset-grid">
                    {kept.map((pattern) => (
                      <li key={pattern.id} className="preset">
                        <EffectThumb effect={faceOf(pattern.layers)} playing={false} />
                        <span className="preset-words">
                          <strong>{pattern.name}</strong>
                          <span className="hint">A look of {pattern.layers.length} effects you kept.</span>
                        </span>
                        <button type="button" className="button small" onClick={() => void add({ type: "use_pattern", id: pattern.id }, pattern.name)}>
                          Add
                        </button>
                      </li>
                    ))}
                  </ul>
                </>
              )}
              <h3>Ready-made</h3>
              <ul className="preset-grid">
                {presets.map((look) => (
                  <li key={look.name} className={`preset${has(look.name) ? " have" : ""}`}>
                    <EffectThumb effect={look.effect ?? ""} playing={false} />
                    <span className="preset-words">
                      <strong>{look.name}</strong>
                      <span className="hint">{look.description}</span>
                    </span>
                    <button
                      type="button"
                      className="button small"
                      disabled={has(look.name)}
                      onClick={() => void add({ type: "add_look", name: look.name }, look.name)}
                    >
                      {has(look.name) ? "Added" : "Add"}
                    </button>
                  </li>
                ))}
              </ul>
              {presets.length === 0 && kept.length === 0 && <p className="hint">Nothing called that.</p>}
            </>
          )}
        </div>
      </div>
    </div>
  );
}
