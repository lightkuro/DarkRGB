// The settings window. Things that are set once and left alone live
// here, so the main screen stays about the lights.

import {
  AudioLines,
  FolderOpen,
  Info,
  Keyboard,
  Lightbulb,
  Save,
  SlidersHorizontal,
  X,
} from "lucide-react";
import { useEffect, useRef, type ReactNode } from "react";
import changelog from "../../../CHANGELOG.md?raw";
import {
  fullPref,
  engine,
  libraryOpen,
  perFrame,
  settingsPage,
  useFull,
  useHello,
  useLibrary,
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
import { KeyAssign, Keycap, modeNote } from "./Keys";
import { MidiSettings } from "./Midi";
import { NameField, Slider, Switch } from "./ui";

const PAGES: { id: SettingsPage; name: string; icon: ReactNode }[] = [
  { id: "general", name: "General", icon: <SlidersHorizontal size={15} /> },
  { id: "sound", name: "Sound", icon: <AudioLines size={15} /> },
  { id: "lights", name: "Lights", icon: <Lightbulb size={15} /> },
  { id: "live", name: "Live and keys", icon: <Keyboard size={15} /> },
  { id: "show", name: "Show", icon: <Save size={15} /> },
  { id: "about", name: "About", icon: <Info size={15} /> },
];

const setLimit = perFrame((limit: number) => engine.quiet({ type: "set_master", limit }));
const setDelay = perFrame((delay_ms: number) => engine.quiet({ type: "set_audio", delay_ms }));
const setFade = perFrame((fade: number) => engine.quiet({ type: "set_live", fade }));
const setDynamics = perFrame((dynamics: number) => engine.quiet({ type: "set_audio", dynamics }));

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
  const full = useFull();
  return (
    <>
      <div className="field">
        <span className="field-head">
          <span>Layout</span>
        </span>
        <div className="segmented" role="radiogroup" aria-label="Layout">
          <button
            type="button"
            role="radio"
            aria-checked={!full}
            className={full ? "" : "selected"}
            onClick={() => fullPref.set(false)}
          >
            Simple
          </button>
          <button
            type="button"
            role="radio"
            aria-checked={full}
            className={full ? "selected" : ""}
            onClick={() => fullPref.set(true)}
          >
            Full
          </button>
        </div>
      </div>
      <p className="hint">
        {full
          ? "Full shows every control: folders and groups, which part of the sound an effect hears, how effects mix, keys on everything, and every setting."
          : "Simple shows what most shows need and looks after the rest. Full adds folders and groups, which part of the sound an effect hears, how effects mix, keys on everything, and every setting."}{" "}
        Nothing in a show is lost by switching. This choice is kept on this device.
      </p>
      <DemoSwitch />
    </>
  );
}

function Sound({ show }: { show: Show }) {
  const full = useFull();
  return (
    <>
      <AudioControls />
      <Slider
        label="Follow the song's energy"
        min={0}
        max={1}
        value={show.audio.dynamics}
        format={(v) => (v === 0 ? "Off" : `${Math.round(v * 100)}%`)}
        onChange={setDynamics}
      />
      <p className="hint">
        Makes the quiet parts of a song dimmer and calmer, so the big parts land harder. Off treats every part
        of a song alike. Effects you play by hand are never held back.
      </p>
      {(full || show.audio.delay_ms > 0) && (
        <>
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
      )}
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
  const full = useFull();
  const hello = useHello();
  const status = useStatus();
  const pixels = show.fixtures.reduce(
    (sum, f) => sum + (f.shape.type === "strip" ? f.shape.count : f.shape.cols * f.shape.rows),
    0,
  );
  const limited = show.flash.limit > 0;
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
      {(full || show.output.frame_rate !== 60) && (
        <>
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
        </>
      )}

      <h3>Flashing</h3>
      <Row
        title="Flash limit"
        hint={
          !limited
            ? "Off. Strobes and fast flashes run at full speed."
            : show.flash.strict
              ? "Strict: any change of a tenth of full brightness counts, at most three a second."
              : "At most three hard flashes a second. Faster ones are softened, on the lights and on this screen."
        }
      >
        <Switch
          checked={limited}
          label="Limit flashing"
          onChange={(on) => engine.send({ type: "set_flash", limit: on ? 3 : 0 })}
        />
      </Row>
      {limited && (full || show.flash.strict) && (
        <div className="segmented" role="radiogroup" aria-label="What counts as a flash">
          <button
            type="button"
            role="radio"
            aria-checked={!show.flash.strict}
            className={show.flash.strict ? "" : "selected"}
            onClick={() => engine.send({ type: "set_flash", strict: false })}
          >
            Hard flashes
          </button>
          <button
            type="button"
            role="radio"
            aria-checked={show.flash.strict}
            className={show.flash.strict ? "selected" : ""}
            onClick={() => engine.send({ type: "set_flash", strict: true })}
          >
            Strict
          </button>
        </div>
      )}
      <p className="hint">
        Flashing faster than about three times a second can trigger seizures in people with photosensitive
        epilepsy. With the limit on, strobes slow down to three a second and anything else that flashes the whole
        picture faster is softened. This choice is kept with the show, so a club night and a display on the lawn
        can each have their own. Leave it on when anyone you do not know will see the lights.
      </p>
      {limited && (full || show.flash.strict) && (
        <p className="hint">
          "Hard flashes" counts a change of a quarter of full brightness or more. "Strict" is the limit used for
          screens, a tenth; it also flattens most effects that follow fast music.
        </p>
      )}
      {full && status && (
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
    case "next_scene":
      return "Next scene";
    case "prev_scene":
      return "Previous scene";
    case "freeze":
      return "Freeze the picture";
  }
}

const setIdleAfter = perFrame((after: number) => {
  const scene = engine.show.get()?.live.idle.scene ?? null;
  engine.quiet({ type: "set_idle", scene, after });
});

function LivePage({ show }: { show: Show }) {
  const full = useFull();
  const hello = useHello();
  const effectName = (id: string) => hello?.effects.find((e) => e.id === id)?.name ?? id;
  // Pads with keys, by key. Pads without one are set up in the Live view.
  const sorted = show.bindings
    .filter((b) => b.key)
    .sort((a, b) => (a.key ?? "").localeCompare(b.key ?? ""));
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
      {(full || show.live.fade > 0) && (
        <>
          <Slider
            label="Scene changes fade over"
            min={0}
            max={5}
            step={0.1}
            value={show.live.fade}
            format={(v) => (v === 0 ? "No fade" : `${v.toFixed(1)} s`)}
            onChange={setFade}
          />
        </>
      )}
      <Tempo />
      <p className="hint">
        Tap in time with the music to set the tempo by hand when it is not being found, or is found wrong. The
        first tap counts as the first beat of four.
      </p>

      {show.scenes.length > 0 && (
        <>
          <h3>When the music stops</h3>
          <label className="field inline">
            <span>
              Change to
              <span className="hint block">And come back to what was playing when the music does.</span>
            </span>
            <select
              aria-label="The scene to change to when the music stops"
              value={show.live.idle.scene ?? ""}
              onChange={(e) =>
                engine.send({ type: "set_idle", scene: e.target.value === "" ? null : Number(e.target.value) })
              }
            >
              <option value="">Stay on what is playing</option>
              {show.scenes.map((scene) => (
                <option key={scene.id} value={scene.id}>
                  {scene.name}
                </option>
              ))}
            </select>
          </label>
          {show.live.idle.scene !== null && (full || show.live.idle.after !== 20) && (
            <Slider
              label="After this much silence"
              min={3}
              max={120}
              step={1}
              value={show.live.idle.after}
              format={(v) => `${Math.round(v)} s`}
              onChange={setIdleAfter}
            />
          )}
        </>
      )}

      <h3>Autopilot</h3>
      <Row
        title="Change scene by itself"
        hint="Moves on to another scene in time with the music. A scene you choose yourself plays its full turn first."
      >
        <Switch
          checked={show.live.auto.enabled}
          label="Autopilot"
          onChange={(auto) => engine.send({ type: "set_live", auto })}
        />
      </Row>
      {full && (
        <>
          <div className="field">
            <span className="field-head">
              <span>A new scene every</span>
            </span>
            <div className="segmented" role="radiogroup" aria-label="Bars per scene">
              {[2, 4, 8, 16].map((bars) => (
                <button
                  key={bars}
                  type="button"
                  role="radio"
                  aria-checked={show.live.auto.every_bars === bars}
                  className={show.live.auto.every_bars === bars ? "selected" : ""}
                  onClick={() => engine.send({ type: "set_live", every_bars: bars })}
                >
                  {bars} bars
                </button>
              ))}
            </div>
          </div>
          <div className="field inline">
            <span>
              Also when the song changes
              <span className="hint block">A drop or a breakdown brings the next scene at once.</span>
            </span>
            <Switch
              checked={show.live.auto.on_drops}
              label="Change scene when the song changes"
              onChange={(on_drops) => engine.send({ type: "set_live", on_drops })}
            />
          </div>
          <div className="field inline">
            <span>Shuffle the order</span>
            <Switch
              checked={show.live.auto.shuffle}
              label="Shuffle the order of scenes"
              onChange={(shuffle) => engine.send({ type: "set_live", shuffle })}
            />
          </div>
          <p className="hint">
            A bar is four beats. With no steady beat to count, a bar is taken as two seconds. A scene can be kept out
            of autopilot from its own menu in the scenes bar.
          </p>
        </>
      )}

      <MidiSettings />

      {full ? (
        <>
          <h3>Keys</h3>
          <p className="hint">
            Scenes, effects, folders and groups of lights get their keys where they are listed, or in the Live view
            under Customize. These belong to the whole show.
          </p>
          <KeyAssign action={{ type: "blackout" }} label="Blackout" />
          <KeyAssign action={{ type: "tap_tempo" }} label="Tap the tempo" />
          <KeyAssign action={{ type: "hit" }} label="Fire a beat" />
          <KeyAssign action={{ type: "next_scene" }} label="Next scene" />
          <KeyAssign action={{ type: "prev_scene" }} label="Previous scene" />
          <KeyAssign action={{ type: "freeze" }} label="Freeze the picture" />
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
                    <span className="hint">{modeNote(binding)}</span>
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
      ) : (
        <p className="hint">
          Keys for scenes and effects are set in the Live view, under Customize. The full layout has more here:
          how often autopilot changes scene, fades, and keys for blackout, tempo and the rest.
        </p>
      )}
    </>
  );
}

function ShowFile({ show }: { show: Show }) {
  const status = useStatus();
  const library = useLibrary();
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

      <h3>Other shows</h3>
      <div className="setting">
        <div className="setting-text">
          <span className="setting-title">Your shows</span>
          <span className="hint">
            {library.available
              ? `${library.shows.length === 1 ? "This is the only show" : `${library.shows.length} shows`} on this computer. `
              : ""}
            Open another show, start a new one, make copies, or bring one in from a file. The show's name in the
            top bar opens the same window.
          </span>
        </div>
        <div className="setting-control">
          <button
            type="button"
            className="button"
            onClick={() => {
              settingsPage.set(null);
              libraryOpen.set(true);
            }}
          >
            <FolderOpen size={15} /> Shows…
          </button>
        </div>
      </div>
      <p className="hint">
        The flash limit, frames per second, sound source, speaker delay and brightness ceiling belong to this
        computer, not to a show: they stay as they are when you open another show.
      </p>
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
