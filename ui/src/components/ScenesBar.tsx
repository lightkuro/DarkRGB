// Scenes along the bottom: saved looks, one click (or one key) each.

import { Bookmark, ChevronUp, Plus, RefreshCw, Shuffle, Sparkles, Trash2 } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import {
  engine,
  settingsPage,
  useCurrentScene,
  useLibrary,
  useHello,
  useLive,
  useShow,
  type Scene,
} from "../engine";
import { KeyAssign, Keycap } from "./Keys";
import { ConfirmButton, NameField, Switch, useDismiss } from "./ui";

function SaveScene() {
  const [name, setName] = useState<string | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  useEffect(() => {
    if (name !== null) inputRef.current?.focus();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [name === null]);
  if (name === null) {
    return (
      <button type="button" className="chip add" onClick={() => setName("")}>
        <Plus size={14} /> Save as scene
      </button>
    );
  }
  const save = () => {
    if (name.trim()) engine.send({ type: "save_scene", name: name.trim() });
    setName(null);
  };
  return (
    <form
      className="scene-form"
      onSubmit={(e) => {
        e.preventDefault();
        save();
      }}
    >
      <input
        ref={inputRef}
        value={name}
        maxLength={80}
        placeholder="Scene name"
        aria-label="Scene name"
        onChange={(e) => setName(e.target.value)}
        onKeyDown={(e) => e.key === "Escape" && setName(null)}
        onBlur={() => !name.trim() && setName(null)}
      />
      <button type="submit" className="button small" disabled={!name.trim()}>
        Save
      </button>
    </form>
  );
}

/** Rename, key, update and delete for one scene. */
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
  return (
    <footer className="scenes">
      <span className="scenes-label">Scenes</span>
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
      <div className="scene-list">
        {show.scenes.map((scene) => {
          const binding = show.bindings.find((b) => b.action.type === "scene" && b.action.id === scene.id);
          const waiting = live.pending_scene === scene.id;
          return (
            <span
              key={scene.id}
              className={`scene ${current === scene.id ? "current" : ""} ${waiting ? "waiting" : ""} ${
                scene.auto ? "" : "skipped"
              }`}
            >
              <button
                type="button"
                className="chip"
                title={waiting ? `${scene.name} starts on the beat` : `Switch to ${scene.name}`}
                aria-pressed={current === scene.id}
                onClick={() => engine.send({ type: "recall_scene", id: scene.id })}
              >
                {binding && <Keycap binding={binding} />}
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
