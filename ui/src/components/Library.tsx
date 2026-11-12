// The shows on this computer: open one, start a new one, copy, rename,
// delete. Opened from the show's name in the top bar.

import { Bookmark, Copy, FileDown, FilePlus2, FileUp, Pencil, Trash2, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import {
  engine,
  libraryOpen,
  libraryTab,
  readPalette,
  useHello,
  useLibrary,
  useLibraryOpen,
  useShow,
  useStatus,
  type Kind,
  type SavedPattern,
  type ShowEntry,
} from "../engine";
import { KindCards } from "./KindCards";
import { ConfirmButton, IconButton } from "./ui";

const count = (n: number, one: string) => `${n} ${one}${n === 1 ? "" : "s"}`;

/** "saved 5 minutes ago", from a time in seconds since 1970. */
function saved(modified: number): string {
  if (!modified) return "";
  const seconds = Math.max(0, Date.now() / 1000 - modified);
  if (seconds < 90) return "saved just now";
  if (seconds < 3600) return `saved ${Math.round(seconds / 60)} minutes ago`;
  if (seconds < 86400) return `saved ${count(Math.round(seconds / 3600), "hour")} ago`;
  if (seconds < 86400 * 7) return `saved ${count(Math.round(seconds / 86400), "day")} ago`;
  const date = new Date(modified * 1000);
  return `saved ${date.toLocaleDateString(undefined, { day: "numeric", month: "short", year: "numeric" })}`;
}

/** A name being typed over. Enter or leaving the field keeps it, Escape does not. */
function NameEdit({ value, onDone }: { value: string; onDone: (name: string | null) => void }) {
  const [draft, setDraft] = useState(value);
  const ref = useRef<HTMLInputElement>(null);
  // Escape must not also keep the name when the field then loses focus.
  const settled = useRef(false);
  useEffect(() => ref.current?.select(), []);
  const finish = (name: string | null) => {
    if (settled.current) return;
    settled.current = true;
    const next = name?.trim();
    onDone(next && next !== value ? next : null);
  };
  return (
    <input
      ref={ref}
      className="show-row-edit"
      aria-label="Name of the show"
      value={draft}
      maxLength={80}
      onChange={(e) => setDraft(e.target.value)}
      onBlur={() => finish(draft)}
      onKeyDown={(e) => {
        if (e.key === "Enter") finish(draft);
        if (e.key === "Escape") {
          // Only the name, not the whole window.
          e.stopPropagation();
          finish(null);
        }
      }}
    />
  );
}

function Row({ entry, open }: { entry: ShowEntry; open: boolean }) {
  const [renaming, setRenaming] = useState(false);
  const details = entry.readable
    ? [count(entry.lights, "light"), count(entry.effects, "effect"), count(entry.scenes, "scene"), saved(entry.modified)]
        .filter(Boolean)
        .join(" · ")
    : "This file could not be read as a show.";

  const openIt = async () => {
    const result = await engine.request({ type: "open_show", file: entry.file });
    if (!result.ok) engine.toast("error", result.error ?? "Could not open that show.");
    else libraryOpen.set(false);
  };

  return (
    <li className={`show-row ${open ? "open" : ""} ${entry.readable ? "" : "broken"}`}>
      <div className="show-row-text">
        <div className="show-row-title">
          {renaming ? (
            <NameEdit
              value={entry.name}
              onDone={(name) => {
                setRenaming(false);
                if (name) engine.send({ type: "rename_show_file", file: entry.file, name });
              }}
            />
          ) : (
            <span className="show-row-name">{entry.name}</span>
          )}
          {open && <span className="show-row-flag">Open now</span>}
        </div>
        <span className="hint">{details}</span>
      </div>
      <div className="show-row-actions">
        {!open && entry.readable && (
          <button type="button" className="button small" onClick={() => void openIt()}>
            Open
          </button>
        )}
        {entry.readable && (
          <>
            <IconButton label={`Rename ${entry.name}`} onClick={() => setRenaming(true)}>
              <Pencil size={14} />
            </IconButton>
            <IconButton
              label={`Make a copy of ${entry.name}`}
              onClick={() => engine.send({ type: "duplicate_show", file: entry.file })}
            >
              <Copy size={14} />
            </IconButton>
          </>
        )}
        {open ? (
          // Keeps the buttons of every row in line.
          <span className="show-row-gap" title="The open show cannot be deleted. Open another one first." />
        ) : (
          <ConfirmButton
            label={`Delete ${entry.name}`}
            confirmLabel="Delete?"
            onConfirm={() => engine.send({ type: "delete_show", file: entry.file })}
          >
            <Trash2 size={14} />
          </ConfirmButton>
        )}
      </div>
    </li>
  );
}

/** The form for a new show: a name, and whether the lights come along. */
function NewShow({ replaces, onDone }: { replaces: boolean; onDone: () => void }) {
  const show = useShow();
  const hasLights = (show?.devices.length ?? 0) > 0;
  const [name, setName] = useState("");
  const [kind, setKind] = useState<Kind>(show && show.kind !== "custom" ? show.kind : "room");
  const [keepLights, setKeepLights] = useState(true);
  const [sure, setSure] = useState(false);
  const ref = useRef<HTMLInputElement>(null);
  useEffect(() => ref.current?.focus(), []);

  const create = async () => {
    // Without a library the new show takes the place of this one: ask once more.
    if (replaces && !sure) return setSure(true);
    const result = await engine.request({
      type: "new_show",
      name: name.trim() || undefined,
      keep_lights: hasLights && keepLights,
      kind,
    });
    if (!result.ok) return engine.toast("error", result.error ?? "Could not start a new show.");
    if (result.note) engine.toast("info", result.note);
    // The new show is open: straight to it.
    libraryOpen.set(false);
  };

  return (
    <form
      className="new-show"
      onSubmit={(e) => {
        e.preventDefault();
        void create();
      }}
    >
      <KindCards value={kind} onChange={setKind} />
      <label className="field">
        <span className="field-head">
          <span>Name of the new show</span>
        </span>
        <input
          ref={ref}
          type="text"
          maxLength={80}
          placeholder="New show"
          value={name}
          onChange={(e) => setName(e.target.value)}
        />
      </label>
      {hasLights && (
        <label className="check">
          <input type="checkbox" checked={keepLights} onChange={(e) => setKeepLights(e.target.checked)} />
          Keep my lights, their places and their groups
        </label>
      )}
      {replaces && (
        <p className="hint">
          It takes the place of the show you have now, which is kept in a file beside the show file.
        </p>
      )}
      <div className="new-show-actions">
        <button type="button" className="text-button" onClick={onDone}>
          Cancel
        </button>
        <button type="submit" className="button small">
          {sure ? "Replace this show?" : "Create"}
        </button>
      </div>
    </form>
  );
}

function PatternRow({ pattern }: { pattern: SavedPattern }) {
  const hello = useHello();
  const [renaming, setRenaming] = useState(false);
  const names = pattern.layers.map((l) => l.name ?? hello?.effects.find((e) => e.id === l.effect)?.name ?? l.effect);
  return (
    <li className="show-row">
      <div className="show-row-text">
        <div className="show-row-title">
          {renaming ? (
            <NameEdit
              value={pattern.name}
              onDone={(name) => {
                setRenaming(false);
                if (name) engine.send({ type: "rename_pattern", id: pattern.id, name });
              }}
            />
          ) : (
            <span className="show-row-name">{pattern.name}</span>
          )}
        </div>
        <span className="hint">{pattern.look ? names.join(", ") : `${names[0]}, with your settings`}</span>
      </div>
      <div className="show-row-actions">
        <button
          type="button"
          className="button small"
          title={pattern.look ? "Add it to this show as a scene, and switch to it" : "Add it to what is playing"}
          onClick={async () => {
            const result = await engine.request({ type: "use_pattern", id: pattern.id });
            if (!result.ok) return engine.toast("error", result.error ?? "Could not use that.");
            engine.toast("info", pattern.look ? `${pattern.name} is now a scene in this show.` : `${pattern.name} was added.`);
          }}
        >
          Use
        </button>
        <IconButton label={`Rename ${pattern.name}`} onClick={() => setRenaming(true)}>
          <Pencil size={14} />
        </IconButton>
        <ConfirmButton
          label={`Delete ${pattern.name}`}
          confirmLabel="Delete?"
          onConfirm={() => engine.send({ type: "delete_pattern", id: pattern.id })}
        >
          <Trash2 size={14} />
        </ConfirmButton>
      </div>
    </li>
  );
}

/** What has been kept to use in any show: looks, effects and palettes. */
function Kept() {
  const library = useLibrary();
  const hello = useHello();
  const [name, setName] = useState("");
  if (!library.available) {
    return (
      <div className="library-body">
        <p className="hint">
          darkRGB was started with a show file of its own, so nothing can be kept between shows this time.
        </p>
      </div>
    );
  }
  const looks = library.patterns.filter((p) => p.look);
  const effects = library.patterns.filter((p) => !p.look);
  return (
    <div className="library-body">
      <p className="hint">
        Effects with your settings, whole looks and palettes, kept to use in any show. They are stored beside your
        shows, on the computer darkRGB runs on.
      </p>
      <form
        className="keep-look"
        onSubmit={(e) => {
          e.preventDefault();
          if (!name.trim()) return;
          engine.send({ type: "save_look", name: name.trim() });
          setName("");
        }}
      >
        <input
          type="text"
          maxLength={80}
          placeholder="A name for what is playing now"
          aria-label="A name for what is playing now"
          value={name}
          onChange={(e) => setName(e.target.value)}
        />
        <button type="submit" className="button small" disabled={!name.trim()}>
          <Bookmark size={13} /> Keep it as a look
        </button>
      </form>

      <h3>Looks</h3>
      {looks.length > 0 ? (
        <ul className="show-list" aria-label="Kept looks">
          {looks.map((p) => (
            <PatternRow key={p.id} pattern={p} />
          ))}
        </ul>
      ) : (
        <p className="hint">
          None yet. A look is everything that is playing, kept as one. Used in a show, it becomes a scene.
        </p>
      )}

      <h3>Effects</h3>
      {effects.length > 0 ? (
        <ul className="show-list" aria-label="Kept effects">
          {effects.map((p) => (
            <PatternRow key={p.id} pattern={p} />
          ))}
        </ul>
      ) : (
        <p className="hint">None yet. Open an effect you have changed and choose Keep.</p>
      )}

      <h3>Palettes</h3>
      {library.palettes.length > 0 ? (
        <ul className="show-list" aria-label="Kept palettes">
          {library.palettes.map((p) => {
            const read = readPalette(p.value, hello?.palettes ?? []);
            const stops = read ? (read.cyclic ? [...read.stops, read.stops[0]] : read.stops) : [];
            return (
              <li key={p.id} className="show-row">
                <span className="kept-swatch" style={{ background: `linear-gradient(90deg, ${stops.join(", ")})` }} />
                <div className="show-row-text">
                  <span className="show-row-name">{p.name}</span>
                </div>
                <div className="show-row-actions">
                  <ConfirmButton
                    label={`Delete ${p.name}`}
                    confirmLabel="Delete?"
                    onConfirm={() => engine.send({ type: "delete_palette", id: p.id })}
                  >
                    <Trash2 size={14} />
                  </ConfirmButton>
                </div>
              </li>
            );
          })}
        </ul>
      ) : (
        <p className="hint">
          None yet. In an effect's colours, choose "Your own colours", then keep them. Kept palettes are offered
          wherever colours are chosen.
        </p>
      )}
    </div>
  );
}

export function Library() {
  const isOpen = useLibraryOpen();
  const library = useLibrary();
  const show = useShow();
  const status = useStatus();
  const panelRef = useRef<HTMLDivElement>(null);
  const fileRef = useRef<HTMLInputElement>(null);
  const [adding, setAdding] = useState(false);
  const [renaming, setRenaming] = useState(false);
  const [picked, setPicked] = useState<{ name: string; text: string } | null>(null);
  const [busy, setBusy] = useState(false);
  const [tab, setTab] = useState<"shows" | "kept">("shows");

  useEffect(() => {
    if (!isOpen) return;
    setAdding(false);
    setRenaming(false);
    setPicked(null);
    setTab(libraryTab.get());
    libraryTab.set("shows");
    // Files may have been added or removed outside the program.
    engine.quiet({ type: "refresh_library" });
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") libraryOpen.set(false);
    };
    document.addEventListener("keydown", onKey);
    panelRef.current?.focus();
    return () => document.removeEventListener("keydown", onKey);
  }, [isOpen]);

  if (!isOpen || !show) return null;

  const bringIn = async (file: { name: string; text: string }) => {
    setBusy(true);
    const result = await engine.importShow(file.text);
    setBusy(false);
    setPicked(null);
    if (!result.ok) return engine.toast("error", result.error ?? "Could not open that file.");
    engine.toast(
      "info",
      library.available
        ? `${file.name} was added to your shows and is open now.`
        : `Opened ${file.name}. ${result.note ?? ""}`.trim(),
    );
    libraryOpen.set(false);
  };

  // The open show first, as it is this moment, then the most recently saved.
  const shows = library.shows
    .map((entry) =>
      entry.file === library.current
        ? {
            ...entry,
            name: show.name,
            lights: show.devices.length,
            effects: show.layers.length,
            scenes: show.scenes.length,
          }
        : entry,
    )
    .sort((a, b) => Number(b.file === library.current) - Number(a.file === library.current));

  return (
    <div
      className="dialog-backdrop"
      onPointerDown={(e) => {
        if (e.target === e.currentTarget) libraryOpen.set(false);
      }}
    >
      <div className="dialog library" role="dialog" aria-modal="true" aria-label="Shows" tabIndex={-1} ref={panelRef}>
        <div className="settings-head">
          <div className="segmented library-tabs" role="tablist" aria-label="What to list">
            <button
              type="button"
              role="tab"
              aria-selected={tab === "shows"}
              className={tab === "shows" ? "selected" : ""}
              onClick={() => setTab("shows")}
            >
              Shows
            </button>
            <button
              type="button"
              role="tab"
              aria-selected={tab === "kept"}
              className={tab === "kept" ? "selected" : ""}
              onClick={() => setTab("kept")}
            >
              Kept effects and colours
            </button>
          </div>
          <button type="button" className="icon-button" aria-label="Close" onClick={() => libraryOpen.set(false)}>
            <X size={16} />
          </button>
        </div>

        {tab === "kept" && <Kept />}
        <div className="library-body" hidden={tab !== "shows"}>
          <div className="setting-actions">
            <button type="button" className="button" onClick={() => setAdding(true)} disabled={adding}>
              <FilePlus2 size={15} /> New show
            </button>
            <button type="button" className="button" onClick={() => fileRef.current?.click()} disabled={busy}>
              <FileUp size={15} /> Open a file…
            </button>
            <button type="button" className="button" onClick={() => engine.send({ type: "export_show" })}>
              <FileDown size={15} /> Save a copy
            </button>
            <input
              ref={fileRef}
              type="file"
              accept=".json,application/json"
              hidden
              onChange={async (e) => {
                const file = e.target.files?.[0];
                e.target.value = "";
                if (!file) return;
                const chosen = { name: file.name, text: await file.text() };
                // In a library nothing is replaced, so there is nothing to ask.
                if (library.available) void bringIn(chosen);
                else setPicked(chosen);
              }}
            />
          </div>

          {adding && <NewShow replaces={!library.available} onDone={() => setAdding(false)} />}
          {picked && (
            <div className="callout confirm">
              <span>
                Open <strong>{picked.name}</strong> in place of the show you have now? The current one is kept in a
                file beside it.
              </span>
              <span className="confirm-actions">
                <button type="button" className="text-button" onClick={() => setPicked(null)}>
                  Cancel
                </button>
                <button type="button" className="button small" disabled={busy} onClick={() => void bringIn(picked)}>
                  {busy ? "Opening…" : "Open it"}
                </button>
              </span>
            </div>
          )}

          {library.available ? (
            <>
              <ul className="show-list" aria-label="Your shows">
                {shows.map((entry) => (
                  <Row key={entry.file} entry={entry} open={entry.file === library.current} />
                ))}
              </ul>
              <p className="hint">
                Every change is saved as you make it. Shows are kept in {library.folder}, on the computer darkRGB
                runs on. A deleted show is moved to the trash folder beside it, not erased. "Save a copy" puts the
                open show in a darkRGB folder in your Documents, to keep a version or move it to another computer.
              </p>
            </>
          ) : (
            <>
              <ul className="show-list" aria-label="The open show">
                <li className="show-row open">
                  <div className="show-row-text">
                    <div className="show-row-title">
                      {renaming ? (
                        <NameEdit
                          value={show.name}
                          onDone={(name) => {
                            setRenaming(false);
                            if (name) engine.send({ type: "rename_show", name });
                          }}
                        />
                      ) : (
                        <span className="show-row-name">{show.name}</span>
                      )}
                      <span className="show-row-flag">Open now</span>
                    </div>
                    <span className="hint">
                      {status?.show_file
                        ? `Every change is saved to ${status.show_file}`
                        : "This show is not being saved to a file."}
                    </span>
                  </div>
                  <div className="show-row-actions">
                    <IconButton label={`Rename ${show.name}`} onClick={() => setRenaming(true)}>
                      <Pencil size={14} />
                    </IconButton>
                  </div>
                </li>
              </ul>
              <p className="hint">
                darkRGB was started with a show file of its own, so the list of shows on this computer is not in
                use. "Save a copy" puts this show in a darkRGB folder in your Documents.
              </p>
            </>
          )}
        </div>
      </div>
    </div>
  );
}
