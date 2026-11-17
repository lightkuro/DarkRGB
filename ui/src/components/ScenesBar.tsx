// Scenes along the bottom: saved looks, one click (or one key) each.

import {
  Bookmark,
  ChevronDown,
  ChevronUp,
  LayoutGrid,
  Pencil,
  Plus,
  RefreshCw,
  Shuffle,
  SkipBack,
  SkipForward,
  Sparkles,
  Trash2,
  Play,
} from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import {
  MOOD_NAMES,
  engine,
  partOfSong,
  scenesInUse,
  settingsPage,
  useCurrentScene,
  useLibrary,
  useHello,
  useLive,
  useShow,
  type Mood,
  type Scene,
} from "../engine";
import { KeyAssign, Keycap } from "./Keys";
import { NEXT_WAYS } from "./Settings";
import { NameButton, managingScenes } from "./ScenePanel";
import { ConfirmButton, NameField, Switch, useDismiss } from "./ui";
import { openMenu, SEPARATOR } from "./ContextMenu";

function SaveScene() {
  return (
    <NameButton label="Save as scene" placeholder="Scene name" className="chip add" onName={(name) => engine.send({ type: "save_scene", name })}>
      <Plus size={14} />
    </NameButton>
  );
}

/** Rename, key, update and delete for one scene. */
const MOODS: { id: Mood; hint: string }[] = [
  { id: "any", hint: "It can be chosen at any time" },
  { id: "calm", hint: "For breakdowns and slow stretches; passed over while the song is at its biggest" },
  { id: "steady", hint: "For verses and the parts that are neither quiet nor big" },
  { id: "big", hint: "For choruses and drops; passed over in quiet stretches" },
];

/**
 * Next and Previous, with the way Next chooses beside them: in order,
 * the same kind of scene again, or whatever suits the song right now.
 */
function NextScene() {
  const show = useShow();
  const [open, setOpen] = useState(false);
  const ref = useDismiss(open, () => setOpen(false));
  // The part of the song, looked at twice a second while the menu is open.
  const [heard, setHeard] = useState({ part: "steady" as Mood, active: false });
  useEffect(() => {
    if (!open) return;
    const look = () => setHeard({ part: partOfSong(engine.audio), active: engine.audio.active });
    look();
    const timer = setInterval(look, 500);
    return () => clearInterval(timer);
  }, [open]);
  if (!show) return null;
  const way = show.live.next ?? "order";
  const { part } = heard;
  const audio = heard;
  const chosen = NEXT_WAYS.find((w) => w.id === way);
  return (
    <div className="next-scene" ref={ref}>
      <button
        type="button"
        className="chip icon"
        aria-label="Previous scene"
        title="Previous scene"
        onClick={() => engine.send({ type: "perform", action: { type: "prev_scene" } })}
      >
        <SkipBack size={13} />
      </button>
      <button
        type="button"
        className="chip"
        title={`Next scene: ${chosen?.name.toLowerCase()}`}
        onClick={() => engine.send({ type: "perform", action: { type: "next_scene" } })}
      >
        <SkipForward size={13} /> Next
        {way !== "order" && <span className="next-way">{chosen?.short}</span>}
      </button>
      <button
        type="button"
        className="chip icon"
        aria-label="Which scene Next goes to"
        aria-expanded={open}
        title="Which scene Next goes to"
        onClick={() => setOpen(!open)}
      >
        <ChevronDown size={13} />
      </button>
      {open && (
        <div className="popover next-menu" role="dialog" aria-label="Which scene Next goes to">
          <div role="radiogroup" aria-label="Next goes to" className="next-ways">
            {NEXT_WAYS.map((w) => (
              <button
                key={w.id}
                type="button"
                role="radio"
                aria-checked={way === w.id}
                className={way === w.id ? "selected" : ""}
                onClick={() => engine.send({ type: "set_live", next: w.id })}
              >
                <strong>{w.name}</strong>
                <span className="hint">{w.hint}</span>
              </button>
            ))}
          </div>
          <p className="hint">
            Right now the song is in {part === "steady" ? "an in-between part" : part === "big" ? "a big part" : "a quiet part"}
            {audio.active ? "" : " (no sound is playing)"}. Say what a scene suits in its own menu, the small arrow
            beside it.
          </p>
          <div className="next-jumps">
            <span>Jump to the next scene for</span>
            {(show.scene_kinds ?? []).map((k) => (
              <button
                key={`k${k.id}`}
                type="button"
                className="chip"
                disabled={!scenesInUse(show).some((s) => s.kind === k.id)}
                title={`The next scene of the kind ${k.name}`}
                onClick={() => engine.send({ type: "perform", action: { type: "next_of_kind", id: k.id } })}
              >
                {k.name}
              </button>
            ))}
            {(["calm", "steady", "big"] as Mood[]).map((mood) => (
              <button
                key={mood}
                type="button"
                className="chip"
                disabled={!scenesInUse(show).some((s) => s.mood === mood)}
                title={
                  scenesInUse(show).some((s) => s.mood === mood)
                    ? `The next scene made for ${MOOD_NAMES[mood].toLowerCase()}`
                    : "No scene is marked for this yet"
                }
                onClick={() => engine.send({ type: "perform", action: { type: "next_of", mood } })}
              >
                {MOOD_NAMES[mood]}
              </button>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

/**
 * A kind of the user's own for a scene: "Chorus", "Build-up". Next can
 * keep to one kind. A new kind is made right here.
 */
function SceneKindField({ scene }: { scene: Scene }) {
  const show = useShow();
  const [making, setMaking] = useState(false);
  const [name, setName] = useState("");
  const kinds = show?.scene_kinds ?? [];
  const make = async () => {
    const trimmed = name.trim();
    if (!trimmed) return;
    const result = await engine.request({ type: "add_scene_kind", name: trimmed });
    if (!result.ok) return engine.toast("error", result.error ?? "Could not make that kind.");
    // The new kind is the last; give it to this scene once it is known.
    const made = engine.show.get()?.scene_kinds?.find((k) => k.name === trimmed);
    if (made) engine.send({ type: "set_scene_kind", scene: scene.id, kind: made.id });
    setName("");
    setMaking(false);
  };
  return (
    <div className="field">
      <span className="field-head">
        <span title="Kinds you make yourself. Next can keep to scenes of one kind.">Kind of your own</span>
      </span>
      {making ? (
        <form
          className="scene-form"
          onSubmit={(e) => {
            e.preventDefault();
            void make();
          }}
        >
          <input
            type="text"
            value={name}
            placeholder="e.g. Chorus, Build-up"
            aria-label="Name of the new kind"
            autoFocus
            maxLength={40}
            onChange={(e) => setName(e.target.value)}
            onKeyDown={(e) => e.stopPropagation()}
          />
          <button type="submit" className="button small" disabled={!name.trim()}>
            Make
          </button>
        </form>
      ) : (
        <select
          aria-label={`The kind of your own ${scene.name} is`}
          value={scene.kind ?? ""}
          onChange={(e) => {
            if (e.target.value === "new") return setMaking(true);
            engine.send({ type: "set_scene_kind", scene: scene.id, kind: e.target.value === "" ? null : Number(e.target.value) });
          }}
        >
          <option value="">None</option>
          {kinds.map((k) => (
            <option key={k.id} value={k.id}>
              {k.name}
            </option>
          ))}
          <option value="new">A new kind…</option>
        </select>
      )}
    </div>
  );
}

function SceneMenu({
  scene,
  left,
  close,
  playing,
}: {
  scene: Scene;
  left: number;
  close: () => void;
  /** This scene is what is playing, unchanged. */
  playing: boolean;
}) {
  const ref = useDismiss(true, close);
  const library = useLibrary();
  return (
    <div className="popover scene-menu" role="dialog" aria-label={`Options for ${scene.name}`} ref={ref} style={{ left }}>
      <NameField
        value={scene.name}
        onCommit={(name) => engine.send({ type: "rename_scene", id: scene.id, name })}
      />
      <KeyAssign action={{ type: "scene", id: scene.id }} />
      <div className="field inline">
        <span>Autopilot may pick it</span>
        <Switch
          checked={scene.auto}
          label={`Autopilot may pick ${scene.name}`}
          onChange={(auto) => engine.send({ type: "set_scene_auto", id: scene.id, auto })}
        />
      </div>
      <div className="field">
        <span className="field-head">
          <span title="Autopilot chooses by this, and so does Next when it is set to go by kind or by the song">
            It suits
          </span>
        </span>
        <div className="segmented" role="radiogroup" aria-label={`The parts of a song ${scene.name} suits`}>
          {MOODS.map((mood) => (
            <button
              key={mood.id}
              type="button"
              role="radio"
              aria-checked={scene.mood === mood.id}
              className={scene.mood === mood.id ? "selected" : ""}
              title={mood.hint}
              onClick={() => engine.send({ type: "set_scene_mood", id: scene.id, mood: mood.id })}
            >
              {MOOD_NAMES[mood.id]}
            </button>
          ))}
        </div>
      </div>
      <SceneKindField scene={scene} />
      <button
        type="button"
        className="text-button"
        title="Open it in the effects panel to change it, while the lights go on playing what they are playing"
        onClick={() => {
          engine.send({ type: "edit_scene", id: scene.id });
          close();
        }}
      >
        <Pencil size={13} /> Change it without playing it
      </button>
      <div className="layer-footer">
        <button
          type="button"
          className="text-button"
          title="Replace this scene with the effects that are playing now"
          onClick={() => {
            engine.send({ type: "save_scene", name: scene.name });
            close();
          }}
        >
          <RefreshCw size={13} /> Update with what is playing
        </button>
        {library.available && playing && (
          <button
            type="button"
            className="text-button"
            title="Keep this look to use in any show. It is listed when you add an effect, and under the show's name."
            onClick={() => {
              engine.send({ type: "save_look", name: scene.name });
              close();
            }}
          >
            <Bookmark size={13} /> Keep for any show
          </button>
        )}
        <span className="spacer" />
        <ConfirmButton
          label={`Delete ${scene.name}`}
          confirmLabel="Delete?"
          onConfirm={() => {
            engine.send({ type: "delete_scene", id: scene.id });
            close();
          }}
        >
          <Trash2 size={15} />
        </ConfirmButton>
      </div>
    </div>
  );
}

export function ScenesBar() {
  const show = useShow();
  const hello = useHello();
  const live = useLive();
  const current = useCurrentScene();
  const [menu, setMenu] = useState<{ id: number; left: number } | null>(null);
  const closeMenu = useCallback(() => setMenu(null), []);
  if (!show) return <footer className="scenes" />;
  const menuScene = menu ? show.scenes.find((s) => s.id === menu.id) : undefined;
  const auto = show.live.auto;
  const ownLooks = hello?.kinds.find((k) => k.id === show.kind)?.looks ?? [];
  const missingLooks = ownLooks.filter((look) => !show.scenes.some((s) => s.name === look));
  const sets = show.scene_sets ?? [];
  // While a scene is changed out of sight, the one the lights are on is marked.
  const playing = live.blind ? (live.on_air_scene ?? null) : current;
  return (
    <footer className="scenes">
      <span className="scenes-label">Scenes</span>
      {sets.length > 0 && (
        <select
          className="set-select"
          aria-label="Set of scenes in use"
          title="The set of scenes in use. The buttons here, the next and previous keys and autopilot keep to it."
          value={show.live.scene_set ?? ""}
          onChange={(e) => engine.send({ type: "use_scene_set", id: e.target.value === "" ? null : Number(e.target.value) })}
        >
          <option value="">All scenes</option>
          {sets.map((set) => (
            <option key={set.id} value={set.id}>
              {set.name}
            </option>
          ))}
        </select>
      )}
      <button
        type="button"
        className="chip manage"
        title="Find and add ready-made scenes, file scenes in sets, and open a scene to change it without playing it"
        onClick={() => managingScenes.set(true)}
      >
        <LayoutGrid size={14} /> Manage scenes
      </button>
      {show.scenes.length > 1 && (
        <button
          type="button"
          className={`chip autopilot ${auto.enabled ? "selected" : ""}`}
          aria-pressed={auto.enabled}
          title={
            auto.enabled
              ? `Autopilot is on: a new scene every ${auto.every_bars} bars${auto.on_drops ? ", and when the song changes" : ""}. Click to switch it off.`
              : "Autopilot: change scene by itself, in time with the music"
          }
          onClick={() => engine.send({ type: "set_live", auto: !auto.enabled })}
        >
          <Shuffle size={13} /> Autopilot
        </button>
      )}
      {show.scenes.length > 1 && <NextScene />}
      <div className="scene-list">
        {scenesInUse(show).map((scene, place) => {
          // With number keys going by place, a scene shows the number
          // of its place; its own key shows only if it is not a number.
          const byPlace = (show.live.number_keys ?? "own") === "set";
          const own = show.bindings.find((b) => b.action.type === "scene" && b.action.id === scene.id);
          const binding = byPlace && own?.key && /^(Digit|Numpad)/.test(own.key) ? undefined : own;
          const waiting = live.pending_scene === scene.id;
          return (
            <span
              key={scene.id}
              className={`scene ${playing === scene.id ? "current" : ""} ${waiting ? "waiting" : ""} ${
                scene.auto ? "" : "skipped"
              } ${live.blind && live.editing === scene.id ? "editing" : ""}`}
            >
              <button
                type="button"
                className="chip"
                title={waiting ? `${scene.name} starts on the beat` : `Switch to ${scene.name}`}
                aria-pressed={playing === scene.id}
                onClick={() => engine.send({ type: "recall_scene", id: scene.id })}
                onContextMenu={(e) => {
                  const rect = e.currentTarget.getBoundingClientRect();
                  openMenu(
                    e,
                    [
                      { label: "Play it", icon: <Play size={13} />, action: () => engine.send({ type: "recall_scene", id: scene.id }) },
                      { label: "Change it without playing it", icon: <Pencil size={13} />, action: () => engine.send({ type: "edit_scene", id: scene.id }) },
                      { label: "Update it with what is playing", icon: <RefreshCw size={13} />, action: () => engine.send({ type: "save_scene", name: scene.name }) },
                      { label: "Name, key and kind…", icon: <ChevronUp size={13} />, action: () => setMenu({ id: scene.id, left: Math.max(8, Math.min(rect.left - 120, window.innerWidth - 300)) }) },
                      SEPARATOR,
                      { label: "Delete it", icon: <Trash2 size={13} />, danger: true, action: () => engine.send({ type: "delete_scene", id: scene.id }) },
                    ],
                    scene.name,
                  );
                }}
              >
                {byPlace && place < 9 ? (
                  <kbd className="keycap" title={`Number key ${place + 1} plays it while this set is in use`}>
                    {place + 1}
                  </kbd>
                ) : (
                  binding && <Keycap binding={binding} />
                )}
                {scene.name}
              </button>
              <button
                type="button"
                className="scene-more"
                aria-label={`Options for ${scene.name}`}
                aria-expanded={menu?.id === scene.id}
                title="Rename, key, update or delete"
                // Stop the click that opens the menu from also closing it.
                onPointerDown={(e) => e.stopPropagation()}
                onClick={(e) => {
                  const rect = e.currentTarget.getBoundingClientRect();
                  const left = Math.max(8, Math.min(rect.left - 120, window.innerWidth - 300));
                  setMenu(menu?.id === scene.id ? null : { id: scene.id, left });
                }}
              >
                <ChevronUp size={12} />
              </button>
            </span>
          );
        })}
        <SaveScene />
        {show.scenes.length === 0 && missingLooks.length > 0 && show.fixtures.length > 0 && (
          <button
            type="button"
            className="chip add"
            title="Ready-made scenes to start from. They are yours to change or delete."
            onClick={() => engine.send({ type: "add_looks" })}
          >
            <Sparkles size={14} /> Add starter scenes
          </button>
        )}
      </div>
      {menu && menuScene && (
        <SceneMenu
          key={menu.id}
          scene={menuScene}
          left={menu.left}
          close={closeMenu}
          playing={current === menuScene.id}
        />
      )}
      {hello && (
        <button
          type="button"
          className="text-button version"
          title="What's new, and about darkRGB"
          onClick={() => settingsPage.set("about")}
        >
          Version {hello.version}
          {hello.build ? ` (build ${hello.build})` : ""}
        </button>
      )}
    </footer>
  );
}
