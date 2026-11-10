// Scenes along the bottom, and the version with its patch notes.

import { Plus, X } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import changelog from "../../../CHANGELOG.md?raw";
import { engine, useHello, useShow } from "../engine";
import { useDismiss } from "./ui";

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

/** Renders the few Markdown features the changelog uses. */
function Notes({ text }: { text: string }) {
  const blocks: React.ReactNode[] = [];
  let items: string[] = [];
  const flush = () => {
    if (items.length) blocks.push(<ul key={blocks.length}>{items.map((i, k) => <li key={k}>{i}</li>)}</ul>);
    items = [];
  };
  for (const line of text.split("\n")) {
    if (line.startsWith("- ")) {
      items.push(line.slice(2));
      continue;
    }
    flush();
    if (line.startsWith("## ")) blocks.push(<h3 key={blocks.length}>{line.slice(3)}</h3>);
    else if (line.trim() && !line.startsWith("# ")) blocks.push(<p key={blocks.length}>{line}</p>);
  }
  flush();
  return <>{blocks}</>;
}

function Version() {
  const hello = useHello();
  const [open, setOpen] = useState(false);
  const close = useCallback(() => setOpen(false), []);
  const ref = useDismiss(open, close);
  if (!hello) return null;
  return (
    <div className="version" ref={ref}>
      {open && (
        <div className="popover notes" role="dialog" aria-label="What's new">
          <div className="notes-body">
            <Notes text={changelog} />
          </div>
          <p className="hint">Free software under the GNU GPL, version 3 or later.</p>
        </div>
      )}
      <button type="button" className="text-button" aria-expanded={open} onClick={() => setOpen(!open)}>
        {open ? "Close" : `Version ${hello.version}`}
      </button>
    </div>
  );
}

export function ScenesBar() {
  const show = useShow();
  const [armed, setArmed] = useState<number | null>(null);
  useEffect(() => {
    if (armed === null) return;
    const timer = setTimeout(() => setArmed(null), 3000);
    return () => clearTimeout(timer);
  }, [armed]);
  if (!show) return <footer className="scenes" />;
  return (
    <footer className="scenes">
      <span className="scenes-label">Scenes</span>
      <div className="scene-list">
        {show.scenes.map((scene) => (
          <span key={scene.id} className="scene">
            <button
              type="button"
              className="chip"
              title={`Switch to ${scene.name}`}
              onClick={() => engine.send({ type: "recall_scene", id: scene.id })}
            >
              {scene.name}
            </button>
            <button
              type="button"
              className={`scene-delete ${armed === scene.id ? "armed" : ""}`}
              aria-label={armed === scene.id ? `Really delete ${scene.name}` : `Delete ${scene.name}`}
              title={armed === scene.id ? "Click again to delete" : "Delete this scene"}
              onClick={() => {
                if (armed === scene.id) engine.send({ type: "delete_scene", id: scene.id });
                setArmed(armed === scene.id ? null : scene.id);
              }}
            >
              {armed === scene.id ? "Delete?" : <X size={12} />}
            </button>
          </span>
        ))}
        <SaveScene />
      </div>
      <Version />
    </footer>
  );
}
