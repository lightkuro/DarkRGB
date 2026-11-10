// The lights in the show, and every way to add one: found on the network,
// by address, by scanning, as a virtual light for planning, or the demo set.

import { ChevronRight, Ghost, Grid3x3, Minus, Plus, Radar, Search, Trash2 } from "lucide-react";
import { useEffect, useState } from "react";
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
  if (device.kind === "virtual") return { tone: "virtual", text: "Shown in the preview only. Nothing is sent anywhere." };
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
  const virtual = device.kind === "virtual";
  const matrix = fixtures.some((f) => f.device === device.id && f.shape.type === "matrix");
  return (
    <li className={`device ${open ? "open" : ""} ${virtual ? "virtual" : ""}`}>
      <div className="device-row">
        <button
          type="button"
          className="device-main"
          onClick={() => setOpen(!open)}
          aria-expanded={open}
          title="Settings for this light"
        >
          <ChevronRight size={14} className="chevron" />
          {matrix ? <Grid3x3 size={15} aria-hidden /> : <Minus size={15} aria-hidden />}
          <span className="device-text">
            <span className="device-name">{device.name}</span>
            <span className="device-sub">
              {virtual ? <Ghost size={11} aria-hidden /> : <span className={`dot ${status.tone}`} />}
              {describe(fixtures, device)}
              {virtual && " · preview only"}
            </span>
          </span>
        </button>
        {!virtual && (
          <Switch
            checked={device.enabled}
            label={device.enabled ? "Stop sending to this device" : "Send to this device"}
            onChange={(enabled) => engine.send({ type: "update_device", id: device.id, enabled })}
          />
        )}
      </div>
      {open && (
        <div className="device-settings">
          <p className={`device-status ${status.tone}`}>{status.text}</p>
          <NameField device={device} />
          {!virtual && (
            <>
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
                Gamma makes dim colours look right on LEDs. If colours look washed out, raise it; if dark shades
                vanish, lower it.
              </p>
            </>
          )}
          <div className="device-footer">
            <span className="hint">{device.version ? `WLED ${device.version}` : ""}</span>
            <ConfirmButton
              label="Remove this light"
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

/**
 * Add a device by its address. The field starts with the beginning of this
 * computer's own network address, since devices are almost always on the
 * same network: usually only the last number is left to type.
 */
export function AddByAddress({ autoFocus }: { autoFocus?: boolean }) {
  const status = useStatus();
  const networks = status?.networks ?? [];
  const scan = status?.scan;
  const [address, setAddress] = useState("");
  // Only fill the field in for the user until they have typed in it.
  const [touched, setTouched] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [scanned, setScanned] = useState<string | null>(null);

  const suggestion = networks[0]?.prefix ?? "";
  useEffect(() => {
    if (!touched && suggestion) setAddress(suggestion);
  }, [suggestion, touched]);

  // The network to scan: what is typed if it names one, else the suggestion.
  const typedPrefix = /^\s*(\d{1,3}\.\d{1,3}\.\d{1,3})(\.|$)/.exec(address)?.[1];
  const prefix = typedPrefix ? `${typedPrefix}.` : suggestion;
  const needsLastNumber = networks.some((n) => n.prefix === address.trim()) || /^\d{1,3}\.\d{1,3}\.\d{1,3}\.$/.test(address.trim());

  const add = async () => {
    const value = address.trim();
    if (!value || busy || needsLastNumber) return;
    setBusy(true);
    setError(null);
    const result = await engine.request({ type: "add_device", address: value });
    setBusy(false);
    if (result.ok) {
      setTouched(false);
      setAddress(suggestion);
      if (result.note) engine.toast("info", result.note);
    } else {
      setError(result.error ?? "Could not add that device.");
    }
  };

  const startScan = async () => {
    setError(null);
    const result = await engine.request({ type: "scan_network", prefix });
    if (result.ok) setScanned(prefix);
    else setError(result.error ?? "Could not scan.");
  };

  const scanning = scan?.running ?? false;
  const finished = scanned !== null && scan && !scan.running && scan.prefix === scanned && scan.done === scan.total;
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
          inputMode="decimal"
          autoFocus={autoFocus}
          placeholder="IP address, e.g. 192.168.1.50"
          aria-label="Device IP address"
          value={address}
          onChange={(e) => {
            setAddress(e.target.value);
            setTouched(true);
            setError(null);
          }}
          onFocus={(e) => {
            // Put the caret after the suggested start, ready for the last number.
            const end = e.target.value.length;
            requestAnimationFrame(() => e.target.setSelectionRange(end, end));
          }}
        />
        <button type="submit" className="button" disabled={busy || !address.trim() || needsLastNumber}>
          {busy ? "Looking…" : "Add"}
        </button>
      </div>
      {networks.length > 1 && (
        <div className="chips">
          {networks.map((n) => (
            <button
              key={n.prefix}
              type="button"
              className={`chip ${address.startsWith(n.prefix) ? "selected" : ""}`}
              title={`This computer is ${n.address} on this network`}
              onClick={() => {
                setAddress(n.prefix);
                setTouched(true);
              }}
            >
              {n.prefix}
            </button>
          ))}
        </div>
      )}
      {needsLastNumber && !error && <p className="hint">Type the last number of the device's address.</p>}
      {error && <p className="callout error">{error}</p>}
      {prefix && (
        <div className="scan">
          {scanning ? (
            <>
              <div className="progress" role="progressbar" aria-valuemin={0} aria-valuemax={scan!.total} aria-valuenow={scan!.done}>
                <span style={{ width: `${(scan!.done / Math.max(1, scan!.total)) * 100}%` }} />
              </div>
              <p className="hint">
                Asking every address on {scan!.prefix}x… {scan!.done} of {scan!.total}
                {scan!.found > 0 ? `, ${scan!.found} found` : ""}
              </p>
            </>
          ) : (
            <>
              <button type="button" className="text-button" onClick={() => void startScan()}>
                <Search size={13} /> Scan {prefix}x for WLED devices
              </button>
              {finished && (
                <p className="hint">
                  {scan!.found === 0
                    ? `Nothing on ${scan!.prefix}x answered as WLED.`
                    : `Found ${scan!.found} on ${scan!.prefix}x. ${scan!.found === 1 ? "It is" : "They are"} listed above unless already added.`}
                </p>
              )}
            </>
          )}
        </div>
      )}
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

/** A light that exists only in the preview, for planning a layout. */
function AddVirtual() {
  const [open, setOpen] = useState(false);
  const [name, setName] = useState("");
  const [kind, setKind] = useState<"strip" | "matrix">("strip");
  const [count, setCount] = useState("60");
  const [cols, setCols] = useState("16");
  const [rows, setRows] = useState("16");
  const [error, setError] = useState<string | null>(null);

  if (!open) {
    return (
      <button type="button" className="text-button" onClick={() => setOpen(true)}>
        <Ghost size={13} /> Add a virtual light
      </button>
    );
  }
  const add = async () => {
    const shape =
      kind === "strip"
        ? ({ type: "strip", count: Number(count) } as const)
        : ({ type: "matrix", cols: Number(cols), rows: Number(rows) } as const);
    const fallback = kind === "strip" ? "Virtual strip" : "Virtual matrix";
    const result = await engine.request({ type: "add_virtual", name: name.trim() || fallback, shape });
    if (result.ok) {
      setOpen(false);
      setName("");
      setError(null);
    } else {
      setError(result.error ?? "Could not add that light.");
    }
  };
  const whole = (value: string) => /^\d{1,6}$/.test(value);
  const valid = kind === "strip" ? whole(count) : whole(cols) && whole(rows);
  return (
    <form
      className="virtual-form"
      onSubmit={(e) => {
        e.preventDefault();
        void add();
      }}
    >
      <p className="hint">
        A virtual light appears on the stage like a real one, so you can plan a layout before buying or wiring
        anything. Nothing is sent for it.
      </p>
      <div className="segmented" role="radiogroup" aria-label="Shape">
        <button type="button" role="radio" aria-checked={kind === "strip"} className={kind === "strip" ? "selected" : ""} onClick={() => setKind("strip")}>
          Strip
        </button>
        <button type="button" role="radio" aria-checked={kind === "matrix"} className={kind === "matrix" ? "selected" : ""} onClick={() => setKind("matrix")}>
          Matrix
        </button>
      </div>
      <input type="text" maxLength={80} placeholder="Name (optional)" aria-label="Name" value={name} onChange={(e) => setName(e.target.value)} />
      {kind === "strip" ? (
        <label className="field inline">
          <span>Number of LEDs</span>
          <input className="number" type="text" inputMode="numeric" aria-label="Number of LEDs" value={count} onChange={(e) => setCount(e.target.value)} />
        </label>
      ) : (
        <label className="field inline">
          <span>Columns × rows</span>
          <span className="pair">
            <input className="number" type="text" inputMode="numeric" aria-label="Columns" value={cols} onChange={(e) => setCols(e.target.value)} />
            ×
            <input className="number" type="text" inputMode="numeric" aria-label="Rows" value={rows} onChange={(e) => setRows(e.target.value)} />
          </span>
        </label>
      )}
      {error && <p className="callout error">{error}</p>}
      <div className="form-actions">
        <button type="button" className="text-button" onClick={() => setOpen(false)}>
          Cancel
        </button>
        <button type="submit" className="button small" disabled={!valid}>
          Add
        </button>
      </div>
    </form>
  );
}

/** Switches the built-in demo lights on and off as a set. */
export function DemoSwitch() {
  const show = useShow();
  const on = show?.devices.some((d) => d.demo) ?? false;
  return (
    <div className="field inline demo-switch">
      <span>
        Demo lights
        <span className="hint"> Two strips and a matrix in the preview, to try effects without hardware.</span>
      </span>
      <Switch checked={on} label="Demo lights" onChange={(enabled) => engine.send({ type: "set_demo", enabled })} />
    </div>
  );
}

export function DevicePanel() {
  const show = useShow();
  const status = useStatus();
  const found = useFound();
  if (!show) return null;
  return (
    <aside className="panel devices">
      <h2>Lights</h2>
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

      {/* With no lights yet, the stage itself offers the ways to add one. */}
      {show.devices.length > 0 && (
        <>
          <h2>
            <Radar size={13} aria-hidden /> Add a WLED device
          </h2>
          {found.length > 0 ? <FoundDevices /> : <p className="hint">None found by themselves yet.</p>}
          <AddByAddress />
          <h2>Without hardware</h2>
          <DemoSwitch />
          <AddVirtual />
        </>
      )}
    </aside>
  );
}
