import { Pencil, Play, Power, Settings as Gear, Sun } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { engine, perFrame, settingsPage, useShow, useView, viewPref } from "../engine";
import { AudioMeter } from "./AudioMeter";
import { Keycap, useBinding } from "./Keys";
import { Slider } from "./ui";

const setBrightness = perFrame((brightness: number) => engine.quiet({ type: "set_master", brightness }));

function ShowName({ name }: { name: string }) {
  const [draft, setDraft] = useState<string | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  useEffect(() => {
    if (draft !== null) inputRef.current?.select();
    // Select only when editing starts, not on each keystroke.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [draft === null]);

  if (draft === null) {
    return (
      <button type="button" className="show-name" title="Rename this show" onClick={() => setDraft(name)}>
        {name}
      </button>
    );
  }
  const commit = () => {
    const next = draft.trim();
    if (next && next !== name) engine.send({ type: "rename_show", name: next });
    setDraft(null);
  };
  return (
    <input
      ref={inputRef}
      className="show-name editing"
      aria-label="Show name"
      value={draft}
      maxLength={80}
      onChange={(e) => setDraft(e.target.value)}
      onBlur={commit}
      onKeyDown={(e) => {
        if (e.key === "Enter") commit();
        if (e.key === "Escape") setDraft(null);
      }}
    />
  );
}

/** Switches between building the show and performing it. */
function ViewSwitch() {
  const view = useView();
  return (
    <div className="segmented view-switch" role="radiogroup" aria-label="View">
      <button
        type="button"
        role="radio"
        aria-checked={view === "edit"}
        className={view === "edit" ? "selected" : ""}
        title="Build the show: lights, effects and their settings"
        onClick={() => viewPref.set("edit")}
      >
        <Pencil size={13} /> Edit
      </button>
      <button
        type="button"
        role="radio"
        aria-checked={view === "live"}
        className={view === "live" ? "selected" : ""}
        title="Perform: a big stage and pads for everything that has a key"
        onClick={() => viewPref.set("live")}
      >
        <Play size={13} /> Live
      </button>
    </div>
  );
}

export function TopBar() {
  const show = useShow();
  const blackoutKey = useBinding({ type: "blackout" });
  if (!show) return <header className="topbar" />;
  const { brightness, blackout } = show.master;
  return (
    <header className="topbar">
      <div className="brand">
        <span className="logo" aria-hidden>
          <i />
          <i />
          <i />
        </span>
        <span className="brand-name">darkRGB</span>
        <ShowName name={show.name} />
      </div>

      <ViewSwitch />
      <AudioMeter />

      <div className="master">
        <Sun size={16} aria-hidden />
        <div className="master-slider">
          <Slider
            bare
            label="Master brightness"
            min={0}
            max={1}
            step={0.01}
            value={brightness}
            onChange={setBrightness}
          />
        </div>
        <span className="master-value">{Math.round(brightness * 100)}%</span>
        <button
          type="button"
          className={`blackout ${blackout ? "on" : ""}`}
          aria-pressed={blackout}
          title={blackout ? "Lights are off. Click to turn them back on." : "Turn all lights off"}
          onClick={() => engine.send({ type: "set_master", blackout: !blackout })}
        >
          <Power size={15} />
          {blackout ? "Lights off" : "Blackout"}
          {blackoutKey && <Keycap binding={blackoutKey} />}
        </button>
        <button
          type="button"
          className="icon-button settings-button"
          aria-label="Settings"
          title="Settings"
          onClick={() => settingsPage.set("general")}
        >
          <Gear size={17} />
        </button>
      </div>
    </header>
  );
}
