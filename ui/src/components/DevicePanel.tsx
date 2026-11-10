// The list of devices in the show, devices found on the network, and the
// way to add one by address.

import { ChevronRight, Grid3x3, Minus, Plus, Radar, Trash2 } from "lucide-react";
import { useState } from "react";
import {
  engine,
  perFrame,
  useShow,
  useStatus,
  type Device,
  type DeviceStats,
  type Discovered,
  type Fixture,
} from "../engine";
import { ConfirmButton, Slider, Switch } from "./ui";

function describe(fixtures: Fixture[], device: Device): string {
  const own = fixtures.filter((f) => f.device === device.id);
  const matrix = own.find((f) => f.shape.type === "matrix");
  if (own.length === 1 && matrix && matrix.shape.type === "matrix") {
    return `${matrix.shape.cols}×${matrix.shape.rows} matrix`;
  }
  return `${device.pixel_count} LEDs`;
}

function statusOf(device: Device, stats: DeviceStats | undefined): { tone: string; text: string } {
  if (!device.enabled) return { tone: "off", text: "Paused" };
  if (!stats) return { tone: "wait", text: "Starting…" };
  if (!stats.resolved) return { tone: "bad", text: `Cannot find ${device.address} on the network` };
  if (stats.frames_dropped > 0 && stats.frames_dropped > stats.frames_sent / 100) {
    return { tone: "bad", text: "The network is refusing frames" };
  }
  return { tone: "ok", text: `Sending to ${device.address}` };
}

const updaters = new Map<string, (value: number) => void>();
/** One rate-limited sender per device and field, so sliders stay smooth. */
function deviceSetter(id: number, field: "brightness" | "gamma") {
  const key = `${id}:${field}`;
  let send = updaters.get(key);
  if (!send) {
    send = perFrame((value: number) => engine.quiet({ type: "update_device", id, [field]: value }));
    updaters.set(key, send);
  }
  return send;
}

/** A name that is saved when the field loses focus or Enter is pressed. */
function NameField({ device }: { device: Device }) {
  const [draft, setDraft] = useState<string | null>(null);
  const commit = () => {
    const next = draft?.trim();
    if (next && next !== device.name) engine.send({ type: "update_device", id: device.id, name: next });
    setDraft(null);
  };
  return (
    <label className="field">
      <span className="field-head">
        <span>Name</span>
      </span>
      <input
        type="text"
        maxLength={80}
        value={draft ?? device.name}
        onChange={(e) => setDraft(e.target.value)}
        onBlur={commit}
        onKeyDown={(e) => {
          if (e.key === "Enter") e.currentTarget.blur();
          if (e.key === "Escape") setDraft(null);
        }}
      />
    </label>
  );
}

function DeviceRow({ device, fixtures, stats }: { device: Device; fixtures: Fixture[]; stats?: DeviceStats }) {
  const [open, setOpen] = useState(false);
  const status = statusOf(device, stats);
  return (
    <li className={`device ${open ? "open" : ""}`}>
      <div className="device-row">
        <button
          type="button"
          className="device-main"
          onClick={() => setOpen(!open)}
          aria-expanded={open}
          title="Device settings"
        >
          <ChevronRight size={14} className="chevron" />
          {fixtures.some((f) => f.device === device.id && f.shape.type === "matrix") ? (
            <Grid3x3 size={15} aria-hidden />
          ) : (
            <Minus size={15} aria-hidden />
          )}
          <span className="device-text">
            <span className="device-name">{device.name}</span>
            <span className="device-sub">
              <span className={`dot ${status.tone}`} />
              {describe(fixtures, device)}
            </span>
          </span>
        </button>
        <Switch
          checked={device.enabled}
          label={device.enabled ? "Stop sending to this device" : "Send to this device"}
          onChange={(enabled) => engine.send({ type: "update_device", id: device.id, enabled })}
        />
      </div>
      {open && (
        <div className="device-settings">
          <p className={`device-status ${status.tone}`}>{status.text}</p>
          <NameField device={device} />
          <Slider
            label="Brightness limit"
            min={0}
            max={1}
            value={device.brightness}
            format={(v) => `${Math.round(v * 100)}%`}
            onChange={deviceSetter(device.id, "brightness")}
          />
          <Slider
            label="Gamma"
            min={1}
            max={3.5}
            step={0.05}
            value={device.gamma}
            onChange={deviceSetter(device.id, "gamma")}
          />
          <p className="hint">
            Gamma makes dim colours look right on LEDs. If colours look washed out, raise it; if dark shades vanish,
            lower it.
          </p>
          <div className="device-footer">
            <span className="hint">{device.version ? `WLED ${device.version}` : ""}</span>
            <ConfirmButton
              label="Remove this device"
              confirmLabel="Remove?"
              onConfirm={() => engine.send({ type: "remove_device", id: device.id })}
            >
              <Trash2 size={15} />
            </ConfirmButton>
          </div>
        </div>
      )}
    </li>
  );
}

export function AddByAddress({ autoFocus }: { autoFocus?: boolean }) {
  const [address, setAddress] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const add = async () => {
    const value = address.trim();
    if (!value || busy) return;
    setBusy(true);
    setError(null);
    const result = await engine.request({ type: "add_device", address: value });
    setBusy(false);
    if (result.ok) {
      setAddress("");
      if (result.note) engine.toast("info", result.note);
    } else {
      setError(result.error ?? "Could not add that device.");
    }
  };
  return (
    <form
      className="add-address"
      onSubmit={(e) => {
        e.preventDefault();
        void add();
      }}
    >
      <div className="add-row">
        <input
          type="text"
          inputMode="url"
          autoFocus={autoFocus}
          placeholder="IP address, e.g. 192.168.1.50"
          aria-label="Device IP address"
          value={address}
          onChange={(e) => {
            setAddress(e.target.value);
            setError(null);
          }}
        />
        <button type="submit" className="button" disabled={busy || !address.trim()}>
          {busy ? "Looking…" : "Add"}
        </button>
      </div>
      {error && <p className="callout error">{error}</p>}
    </form>
  );
}

/**
 * Devices seen on the network that are not in the show. The engine's list
 * refreshes once a second, so a device added a moment ago is also checked
 * against the show itself; otherwise it could be clicked twice.
 */
function useFound(): Discovered[] {
  const status = useStatus();
  const show = useShow();
  const known = new Set(show?.devices.map((d) => d.mac?.toLowerCase()).filter(Boolean));
  return (status?.discovered ?? []).filter((d) => !d.added && !(d.mac && known.has(d.mac.toLowerCase())));
}

export function FoundDevices() {
  const [adding, setAdding] = useState<string | null>(null);
  const found = useFound();
  if (found.length === 0) return null;
  const add = async (address: string) => {
    setAdding(address);
    const result = await engine.request({ type: "add_device", address });
    setAdding(null);
    if (!result.ok) engine.toast("error", result.error ?? "Could not add that device.");
    else if (result.note) engine.toast("info", result.note);
  };
  return (
    <ul className="found">
      {found.map((d) => (
        <li key={d.address}>
          <span className="device-text">
            <span className="device-name">{d.name}</span>
            <span className="device-sub">
              {d.matrix ? `${d.matrix[0]}×${d.matrix[1]} matrix` : `${d.leds} LEDs`} · {d.address}
            </span>
          </span>
          <button type="button" className="button small" disabled={adding !== null} onClick={() => void add(d.address)}>
            <Plus size={14} />
            {adding === d.address ? "Adding…" : "Add"}
          </button>
        </li>
      ))}
    </ul>
  );
}

export function DevicePanel() {
  const show = useShow();
  const status = useStatus();
  const found = useFound();
  if (!show) return null;
  const waiting = found.length;
  return (
    <aside className="panel devices">
      <h2>Devices</h2>
      {show.devices.length === 0 ? (
        <p className="hint">Your lights will be listed here once you add them.</p>
      ) : (
        <ul className="device-list">
          {show.devices.map((device) => (
            <DeviceRow
              key={device.id}
              device={device}
              fixtures={show.fixtures}
              stats={status?.render.devices[String(device.id)]}
            />
          ))}
        </ul>
      )}

      {/* With no devices yet, the stage itself offers these. */}
      {show.devices.length > 0 && (
        <>
          <h2>
            <Radar size={13} aria-hidden /> Found on your network
          </h2>
          {waiting === 0 ? <p className="hint">No other WLED devices found.</p> : <FoundDevices />}
          <h2>Add by address</h2>
          <AddByAddress />
        </>
      )}
    </aside>
  );
}
