import { ChevronDown, Pencil, Play, Power, Redo2, Settings as Gear, Sun, Undo2 } from "lucide-react";
import { engine, libraryOpen, perFrame, settingsPage, useHistory, useShow, useView, viewPref } from "../engine";
import { AudioMeter } from "./AudioMeter";
import { Keycap, useBinding } from "./Keys";
import { Slider } from "./ui";

const setBrightness = perFrame((brightness: number) => engine.quiet({ type: "set_master", brightness }));

/** The show's name. A click opens the list of shows, where it can be renamed. */
function ShowName({ name }: { name: string }) {
  return (
    <button
      type="button"
      className="show-name"
      title="Your shows: open another, start a new one, rename, copy"
      aria-haspopup="dialog"
      onClick={() => libraryOpen.set(true)}
    >
      <span>{name}</span>
      <ChevronDown size={13} aria-hidden />
    </button>
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
  const history = useHistory();
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
        <span className="history">
          <button
            type="button"
            className="icon-button"
            aria-label="Undo"
            title="Undo the last change (Ctrl+Z)"
            disabled={!history.undo}
            onClick={() => engine.send({ type: "undo" })}
          >
            <Undo2 size={15} />
          </button>
          <button
            type="button"
            className="icon-button"
            aria-label="Redo"
            title="Redo (Ctrl+Shift+Z)"
            disabled={!history.redo}
            onClick={() => engine.send({ type: "redo" })}
          >
            <Redo2 size={15} />
          </button>
        </span>
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
