// The settings window. Things that are set once and left alone live
// here, so the main screen stays about the lights.

import {
  AudioLines,
  FileDown,
  FileUp,
  FilePlus2,
  Info,
  Keyboard,
  Lightbulb,
  Save,
  SlidersHorizontal,
  X,
} from "lucide-react";
import { useEffect, useRef, useState, type ReactNode } from "react";
import changelog from "../../../CHANGELOG.md?raw";
import {
  advancedPref,
  engine,
  perFrame,
  settingsPage,
  useAdvanced,
  useHello,
  useSettingsPage,
  useShow,
  useStatus,
  type Action,
  type Launch,
  type SettingsPage,
  type Show,
} from "../engine";
import { AudioControls, Tempo } from "./AudioMeter";
import { DemoSwitch } from "./DevicePanel";
import { KeyAssign, Keycap } from "./Keys";
import { ConfirmButton, NameField, Slider, Switch } from "./ui";

const PAGES: { id: SettingsPage; name: string; icon: ReactNode }[] = [
  { id: "general", name: "General", icon: <SlidersHorizontal size={15} /> },
  { id: "sound", name: "Sound", icon: <AudioLines size={15} /> },
  { id: "lights", name: "Lights", icon: <Lightbulb size={15} /> },
  { id: "live", name: "Live and keys", icon: <Keyboard size={15} /> },
  { id: "show", name: "Show file", icon: <Save size={15} /> },
  { id: "about", name: "About", icon: <Info size={15} /> },
];

const setLimit = perFrame((limit: number) => engine.quiet({ type: "set_master", limit }));
const setDelay = perFrame((delay_ms: number) => engine.quiet({ type: "set_audio", delay_ms }));
const setFade = perFrame((fade: number) => engine.quiet({ type: "set_live", fade }));

/** A setting with its explanation underneath. */
function Row({ title, hint, children }: { title: string; hint?: string; children: ReactNode }) {
  return (
    <div className="setting">
      <div className="setting-text">
        <span className="setting-title">{title}</span>
        {hint && <span className="hint">{hint}</span>}
      </div>
      <div className="setting-control">{children}</div>
    </div>
  );
}

function General() {
  const advanced = useAdvanced();
  return (
    <>
      <Row
        title="Advanced settings"
        hint="Shows every setting on effects: extra sliders, custom frequency ranges, mixing, names and copies."
      >
        <Switch checked={advanced} label="Show advanced settings" onChange={(on) => advancedPref.set(on)} />
      </Row>
      <DemoSwitch />
    </>
  );
}

function Sound({ show }: { show: Show }) {
  return (
    <>
      <AudioControls />
      <Slider
        label="Light delay"
        min={0}
        max={500}
        step={5}
        value={show.audio.delay_ms}
        format={(v) => (v === 0 ? "None" : `${Math.round(v)} ms`)}
        onChange={setDelay}
      />
      <p className="hint">
        Makes the lights wait this long before following the sound. Use it when the lights are ahead of what you
        hear, which happens with Bluetooth speakers and some TVs. Most Bluetooth speakers need 100 to 250 ms.
      </p>
    </>
  );
}

/**
 * How many times a second a string of LEDs can be refreshed. The common
 * kinds (WS2812 and relatives) take about 30 millionths of a second per
 * LED, plus a pause between frames, whatever is driving them.
 */
function ceiling(leds: number): number {
  return Math.floor(1 / (leds * 30e-6 + 300e-6));
}

function Lights({ show }: { show: Show }) {
  const hello = useHello();
  const status = useStatus();
  const pixels = show.fixtures.reduce(
    (sum, f) => sum + (f.shape.type === "strip" ? f.shape.count : f.shape.cols * f.shape.rows),
    0,
  );
  const limited = show.output.flash_limit > 0;
  // Real lights that cannot refresh as often as the chosen frame rate.
  const slow = show.devices.filter((d) => d.kind === "wled" && ceiling(d.pixel_count) < show.output.frame_rate);
  return (
    <>
      <Slider
        label="Maximum brightness"
        min={0.05}
        max={1}
        value={show.master.limit}
        format={(v) => `${Math.round(v * 100)}%`}
        onChange={setLimit}
      />
      <p className="hint">
        What the brightness slider's 100% means. Lower it to protect a power supply that cannot run every LED at
        full white, or simply to make the lights easier on the eyes. Each light also has its own limit.
      </p>
      <div className="field">
        <span className="field-head">
          <span>Frames per second</span>
        </span>
        <div className="segmented" role="radiogroup" aria-label="Frames per second">
          {(hello?.frame_rates ?? [60]).map((rate) => (
            <button
              key={rate}
              type="button"
              role="radio"
              aria-checked={show.output.frame_rate === rate}
              className={show.output.frame_rate === rate ? "selected" : ""}
              onClick={() => engine.send({ type: "set_output", frame_rate: rate })}
            >
              {rate}
            </button>
          ))}
        </div>
      </div>
      <p className="hint">
        60 suits most setups. If lights on WiFi stutter or lag behind, try 40 or 30: fewer frames are easier for a
        busy network and for the devices. 120 only helps short strips on a fast connection, because the LEDs
        themselves take time to refresh.
      </p>
      {slow.length > 0 && (
        <p className="callout note">
          At {show.output.frame_rate} a second these cannot keep up and will skip frames:{" "}
          {slow.map((d) => `${d.name} (${d.pixel_count} LEDs, about ${ceiling(d.pixel_count)} a second)`).join(", ")}.
        </p>
      )}

      <h3>Flashing</h3>
      <Row
        title="Flash limit"
        hint={
          limited
            ? `No more than ${show.output.flash_limit} flashes of the whole picture in a second. Faster flashing is softened, on the lights and on this screen.`
            : "Off. Strobes and fast flashes run at full speed."
        }
      >
        <Switch
          checked={limited}
          label="Limit flashing"
          onChange={(on) => engine.send({ type: "set_output", flash_limit: on ? 3 : 0 })}
        />
      </Row>
      {limited && (
        <div className="segmented" role="radiogroup" aria-label="Flashes allowed in a second">
          {[3, 4].map((n) => (
            <button
              key={n}
              type="button"
              role="radio"
              aria-checked={show.output.flash_limit === n}
              className={show.output.flash_limit === n ? "selected" : ""}
              onClick={() => engine.send({ type: "set_output", flash_limit: n })}
            >
              {n} a second{n === 3 ? " (recommended)" : ""}
            </button>
          ))}
        </div>
      )}
      <p className="hint">
        Flashing faster than about three times a second can trigger seizures in people with photosensitive
        epilepsy. Three is the limit used for web pages and four for broadcast. Leave this on when anyone you do
        not know will see the lights.
      </p>
      {status && (
        <p className="hint">
          Right now: {pixels} LEDs, {status.render.fps.toFixed(0)} frames a second,{" "}
          {status.render.frame_ms.toFixed(1)} ms of work for each.
        </p>
      )}
    </>
  );
}

const LAUNCHES: { id: Launch; name: string }[] = [
  { id: "now", name: "At once" },
  { id: "beat", name: "On the beat" },
  { id: "bar", name: "Every 4th beat" },
];

/** What a key is tied to, in words. */
export function actionName(action: Action, show: Show, effectName: (id: string) => string): string {
  switch (action.type) {
    case "scene":
      return `Scene: ${show.scenes.find((s) => s.id === action.id)?.name ?? "?"}`;
    case "layer": {
      const layer =
        show.layers.find((l) => l.id === action.id) ??
        show.scenes.flatMap((s) => s.layers).find((l) => l.id === action.id);
      return `Effect: ${layer ? (layer.name ?? effectName(layer.effect)) : "?"}`;
    }
    case "layer_group": {
      const folder =
        show.layer_groups.find((g) => g.id === action.id) ??
        show.scenes.flatMap((s) => s.groups).find((g) => g.id === action.id);
      return `Folder: ${folder?.name ?? "?"}`;
    }
    case "light_group":
      return `Lights: ${show.light_groups.find((g) => g.id === action.id)?.name ?? "?"}`;
    case "blackout":
      return "Blackout";
    case "tap_tempo":
      return "Tap the tempo";
    case "hit":
      return "Fire a beat";
  }
}

function LivePage({ show }: { show: Show }) {
  const hello = useHello();
  const effectName = (id: string) => hello?.effects.find((e) => e.id === id)?.name ?? id;
  const sorted = [...show.bindings].sort((a, b) => a.key.localeCompare(b.key));
  return (
    <>
      <div className="field">
        <span className="field-head">
          <span>Scene changes happen</span>
        </span>
        <div className="segmented" role="radiogroup" aria-label="Scene changes happen">
          {LAUNCHES.map((l) => (
            <button
              key={l.id}
              type="button"
              role="radio"
              aria-checked={show.live.launch === l.id}
              className={show.live.launch === l.id ? "selected" : ""}
              onClick={() => engine.send({ type: "set_live", launch: l.id })}
            >
              {l.name}
            </button>
          ))}
        </div>
      </div>
      <p className="hint">
        Waiting for the beat makes a change land in time with the music even when you press a little early. With
        no steady beat to wait for, changes happen at once.
      </p>
      <Slider
        label="Scene changes fade over"
        min={0}
        max={5}
        step={0.1}
        value={show.live.fade}
        format={(v) => (v === 0 ? "No fade" : `${v.toFixed(1)} s`)}
        onChange={setFade}
      />
      <Tempo />
      <p className="hint">
        Tap in time with the music to set the tempo by hand when it is not being found, or is found wrong. The
        first tap counts as the first beat of four.
      </p>

      <h3>Keys</h3>
      <p className="hint">
        Scenes, effects, folders and groups of lights get their keys where they are listed. These three belong to
        the whole show.
      </p>
      <KeyAssign action={{ type: "blackout" }} label="Blackout" />
      <KeyAssign action={{ type: "tap_tempo" }} label="Tap the tempo" />
      <KeyAssign action={{ type: "hit" }} label="Fire a beat" />
      <p className="hint">
        Firing a beat sets off every effect that waits for one, such as Beat pulse and Ripple, with or without
        music.
      </p>

      {sorted.length > 0 && (
        <>
          <h3>All keys</h3>
          <ul className="key-list">
            {sorted.map((binding) => (
              <li key={binding.id}>
                <Keycap binding={binding} />
                <span className="key-what">{actionName(binding.action, show, effectName)}</span>
                <span className="hint">{binding.mode === "hold" ? "while held" : ""}</span>
                <button
                  type="button"
                  className="icon-button"
                  aria-label={`Remove the key ${binding.label || binding.key}`}
                  title="Remove this key"
                  onClick={() => engine.send({ type: "unbind", id: binding.id })}
                >
                  <X size={14} />
                </button>
              </li>
            ))}
          </ul>
        </>
      )}
    </>
  );
}

function ShowFile({ show }: { show: Show }) {
  const status = useStatus();
  const fileRef = useRef<HTMLInputElement>(null);
  const [keepLights, setKeepLights] = useState(true);
  const [picked, setPicked] = useState<{ name: string; text: string } | null>(null);
  const [busy, setBusy] = useState(false);

  const open = async () => {
    if (!picked) return;
    setBusy(true);
    const result = await engine.importShow(picked.text);
    setBusy(false);
    if (result.ok) {
      engine.toast("info", `Opened ${picked.name}. ${result.note ?? ""}`.trim());
      setPicked(null);
    } else {
      engine.toast("error", result.error ?? "Could not open that file.");
    }
  };

  return (
    <>
      <NameField
        label="Name of this show"
        value={show.name}
        onCommit={(name) => engine.send({ type: "rename_show", name })}
      />
      <p className="hint">
        {status?.show_file
          ? `Every change is saved by itself to ${status.show_file}`
          : "This show is not being saved to a file."}
      </p>

      <h3>Copies</h3>
      <div className="setting-actions">
        <button type="button" className="button" onClick={() => engine.send({ type: "export_show" })}>
          <FileDown size={15} /> Save a copy
        </button>
        <button type="button" className="button" onClick={() => fileRef.current?.click()}>
          <FileUp size={15} /> Open a show file…
        </button>
        <input
          ref={fileRef}
          type="file"
          accept=".json,application/json"
          hidden
          onChange={async (e) => {
            const file = e.target.files?.[0];
            e.target.value = "";
            if (file) setPicked({ name: file.name, text: await file.text() });
          }}
        />
      </div>
      <p className="hint">
        A copy goes into a darkRGB folder in your Documents, on the computer the engine runs on. Use copies to keep
        versions, or to move a show to another computer.
      </p>
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
            <button type="button" className="button small" disabled={busy} onClick={() => void open()}>
              {busy ? "Opening…" : "Open it"}
            </button>
          </span>
        </div>
      )}

      <h3>Start again</h3>
      <div className="setting">
        <div className="setting-text">
          <span className="setting-title">New show</span>
          <span className="hint">
            Clears the effects, scenes and keys. The show as it is now is kept in a file beside the show file, in
            case you change your mind.
          </span>
          <label className="check">
            <input type="checkbox" checked={keepLights} onChange={(e) => setKeepLights(e.target.checked)} />
            Keep my lights, their places and their groups
          </label>
        </div>
        <div className="setting-control">
          <ConfirmButton
            label="Start a new show"
            confirmLabel="Start again?"
            onConfirm={() => engine.send({ type: "new_show", keep_lights: keepLights })}
          >
            <FilePlus2 size={16} />
          </ConfirmButton>
        </div>
      </div>
    </>
  );
}

/** Renders the few Markdown features the changelog uses. */
function Notes({ text }: { text: string }) {
  const blocks: ReactNode[] = [];
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
    // A wrapped line continues the item before it.
    if (line.startsWith("  ") && items.length) {
      items[items.length - 1] += ` ${line.trim()}`;
      continue;
    }
    flush();
    if (line.startsWith("## ")) blocks.push(<h4 key={blocks.length}>{line.slice(3)}</h4>);
    else if (line.trim() && !line.startsWith("# ")) blocks.push(<p key={blocks.length}>{line}</p>);
  }
  flush();
  return <>{blocks}</>;
}

function About() {
  const hello = useHello();
  return (
    <>
      <p className="about-version">
        <strong>darkRGB {hello?.version}</strong>
        {hello?.build ? ` (build ${hello.build})` : ""}
      </p>
      <p className="hint">
        A light show controller for WLED devices. Free software under the GNU General Public License, version 3 or
        later: you may use, study, share and change it.
      </p>
      <h3>What's new</h3>
      <div className="notes-body">
        <Notes text={changelog} />
      </div>
    </>
  );
}

export function Settings() {
  const page = useSettingsPage();
  const show = useShow();
  const panelRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!page) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") settingsPage.set(null);
    };
    document.addEventListener("keydown", onKey);
    panelRef.current?.focus();
    return () => document.removeEventListener("keydown", onKey);
    // Focus the window when it opens, not on every page change.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [page === null]);

  if (!page || !show) return null;
  return (
    <div
      className="dialog-backdrop"
      onPointerDown={(e) => {
        if (e.target === e.currentTarget) settingsPage.set(null);
      }}
    >
      <div className="dialog settings" role="dialog" aria-modal="true" aria-label="Settings" tabIndex={-1} ref={panelRef}>
        <nav className="settings-nav" aria-label="Settings pages">
          <h2>Settings</h2>
          {PAGES.map((p) => (
            <button
              key={p.id}
              type="button"
              className={page === p.id ? "selected" : ""}
              aria-current={page === p.id ? "page" : undefined}
              onClick={() => settingsPage.set(p.id)}
            >
              {p.icon}
              {p.name}
            </button>
          ))}
        </nav>
        <div className="settings-page">
          <div className="settings-head">
            <h2>{PAGES.find((p) => p.id === page)?.name}</h2>
            <button type="button" className="icon-button" aria-label="Close settings" onClick={() => settingsPage.set(null)}>
              <X size={16} />
            </button>
          </div>
          <div className="settings-body">
            {page === "general" && <General />}
            {page === "sound" && <Sound show={show} />}
            {page === "lights" && <Lights show={show} />}
            {page === "live" && <LivePage show={show} />}
            {page === "show" && <ShowFile show={show} />}
            {page === "about" && <About />}
          </div>
        </div>
      </div>
    </div>
  );
}
