// The lights in the show, and every way to add one: found on the network,
// by address, by scanning, as a virtual light for planning, or the demo set.

import {
  BoxSelect,
  ChevronRight,
  Folder,
  FolderOpen,
  FolderPlus,
  Ghost,
  Grid3x3,
  GripVertical,
  Minus,
  Plus,
  Radar,
  Search,
  Settings2,
  Trash2,
} from "lucide-react";
import { useEffect, useRef, useState } from "react";
import {
  addingLights,
  engine,
  perFrame,
  select,
  toggleFolded,
  useFolded,
  useLive,
  useFull,
  useSelection,
  useShow,
  useStatus,
  type Device,
  type DeviceStats,
  type Discovered,
  type Fixture,
  type LightGroup,
  type Show,
} from "../engine";
import { KeyAssign, Keycap } from "./Keys";
import { ConfirmButton, IconButton, NameField, Slider, Switch } from "./ui";

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

/** The angle of one light, to read off or type. */
export function AngleField({ fixture }: { fixture: Fixture }) {
  const [draft, setDraft] = useState<string | null>(null);
  const commit = () => {
    const value = Number(draft);
    if (draft !== null && draft.trim() !== "" && Number.isFinite(value)) {
      engine.send({ type: "update_fixture", id: fixture.id, rotation: ((value % 360) + 360) % 360 });
    }
    setDraft(null);
  };
  return (
    <label className="angle" title="The angle of this light, in degrees">
      <input
        className="number"
        type="text"
        inputMode="numeric"
        aria-label="Angle in degrees"
        value={draft ?? String(Math.round(fixture.rotation))}
        onChange={(e) => setDraft(e.target.value)}
        onBlur={commit}
        onKeyDown={(e) => {
          // Arrow keys belong to the field here, not to nudging the light.
          e.stopPropagation();
          if (e.key === "Enter") e.currentTarget.blur();
          if (e.key === "Escape") setDraft(null);
        }}
      />
      °
    </label>
  );
}

/** The group a device's lights are filed under. */
function groupOf(show: Show, device: Device): number | null {
  return show.fixtures.find((f) => f.device === device.id)?.group ?? null;
}

/** Files every fixture of a device in a group, or in none. */
function fileDevice(show: Show, device: number, group: number | null): void {
  const fixtures = show.fixtures.filter((f) => f.device === device).map((f) => f.id);
  if (fixtures.length > 0) engine.send({ type: "group_lights", fixtures, group });
}

function DeviceRow({
  device,
  show,
  stats,
  onGrip,
  lifted,
}: {
  device: Device;
  show: Show;
  stats?: DeviceStats;
  /** Starts dragging this light to another group. */
  onGrip?: (e: React.PointerEvent) => void;
  lifted?: boolean;
}) {
  const [open, setOpen] = useState(false);
  const picked = useSelection();
  const fixtures = show.fixtures;
  const own = fixtures.filter((f) => f.device === device.id);
  const isPicked = own.some((f) => picked.includes(f.id));
  const status = statusOf(device, stats);
  const full = useFull();
  const virtual = device.kind === "virtual";
  const matrix = own.some((f) => f.shape.type === "matrix");
  return (
    <li
      className={`device ${open ? "open" : ""} ${virtual ? "virtual" : ""} ${lifted ? "lifted" : ""} ${
        isPicked ? "selected" : ""
      }`}
    >
      <div className="device-row">
        {onGrip && (
          <span className="grip" title="Drag into a group" onPointerDown={onGrip}>
            <GripVertical size={14} />
          </span>
        )}
        <button
          type="button"
          className="device-main"
          // Picks the light on the stage as well. With Shift it only adds
          // to (or takes from) what is picked.
          onClick={(e) => {
            select(
              own.map((f) => f.id),
              e.shiftKey,
            );
            if (!e.shiftKey) setOpen(!open);
          }}
          aria-expanded={open}
          title="Settings for this light. Shift+click to pick several."
        >
          {!onGrip && <ChevronRight size={14} className="chevron" />}
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
          <NameField value={device.name} onCommit={(name) => engine.send({ type: "update_device", id: device.id, name })} />
          {own.length === 1 && (
            <div className="field inline">
              <span>Angle</span>
              <AngleField fixture={own[0]!} />
            </div>
          )}
          {show.light_groups.length > 0 && (
            <label className="field inline">
              <span>Group</span>
              <select
                value={groupOf(show, device) ?? ""}
                onChange={(e) => fileDevice(show, device.id, e.target.value === "" ? null : Number(e.target.value))}
              >
                <option value="">None</option>
                {show.light_groups.map((g) => (
                  <option key={g.id} value={g.id}>
                    {g.name}
                  </option>
                ))}
              </select>
            </label>
          )}
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
              {full && (
                <>
                  <Slider
                    label="Gamma"
                    min={1}
                    max={3.5}
                    step={0.05}
                    value={device.gamma}
                    onChange={deviceSetter(device.id, "gamma")}
                  />
                  <p className="hint">
                    Gamma makes dim colours look right on LEDs. If colours look washed out, raise it; if dark
                    shades vanish, lower it.
                  </p>
                </>
              )}
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

const groupDimmers = new Map<number, (value: number) => void>();
function groupDimmer(id: number) {
  let send = groupDimmers.get(id);
  if (!send) {
    send = perFrame((brightness: number) => engine.quiet({ type: "update_light_group", id, brightness }));
    groupDimmers.set(id, send);
  }
  return send;
}

function GroupSettings({ group }: { group: LightGroup }) {
  return (
    <div className="folder-settings">
      <NameField
        value={group.name}
        autoSelect
        onCommit={(name) => engine.send({ type: "update_light_group", id: group.id, name })}
      />
      <Slider
        label="Brightness of the group"
        min={0}
        max={1}
        value={group.brightness}
        format={(v) => `${Math.round(v * 100)}%`}
        onChange={groupDimmer(group.id)}
      />
      <KeyAssign action={{ type: "light_group", id: group.id }} label="Key to switch it" />
      <div className="layer-footer">
        <span className="hint">Effects can play on a whole group.</span>
        <span className="spacer" />
        <button
          type="button"
          className="text-button"
          title="Remove the group and keep its lights"
          onClick={() => engine.send({ type: "remove_light_group", id: group.id })}
        >
          Ungroup
        </button>
      </div>
    </div>
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
    addingLights();
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
    addingLights();
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
    addingLights();
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
      <Switch
        checked={on}
        label="Demo lights"
        onChange={(enabled) => {
          addingLights();
          engine.send({ type: "set_demo", enabled });
        }}
      />
    </div>
  );
}

export function DevicePanel() {
  const full = useFull();
  const show = useShow();
  const status = useStatus();
  const found = useFound();
  const folded = useFolded();
  const live = useLive();
  const [openGroup, setOpenGroup] = useState<number | null>(null);
  // A light being dragged, and the group it is over ("none" for no group).
  const [drag, setDrag] = useState<{ device: number; over: number | "none" | null } | null>(null);
  const blocks = useRef(new Map<number | "none", HTMLElement>());
  const knownGroups = useRef<number[] | null>(null);

  // A new group opens with its name ready to type over.
  const groupKey = show?.light_groups.map((g) => g.id).join(",") ?? "";
  useEffect(() => {
    if (!show) return;
    const ids = show.light_groups.map((g) => g.id);
    const before = knownGroups.current;
    knownGroups.current = ids;
    const added = before ? ids.filter((id) => !before.includes(id)) : [];
    if (added.length === 1) setOpenGroup(added[0]!);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [groupKey, show === null]);

  if (!show) return null;
  const groups = show.light_groups;
  const loose = show.devices.filter((d) => groupOf(show, d) === null);

  const startDrag = (device: Device) => (e: React.PointerEvent) => {
    e.preventDefault();
    const from = groupOf(show, device);
    setDrag({ device: device.id, over: null });
    const move = (ev: PointerEvent) => {
      let over: number | "none" | null = null;
      for (const [key, el] of blocks.current) {
        const rect = el.getBoundingClientRect();
        if (ev.clientY >= rect.top && ev.clientY <= rect.bottom) over = key;
      }
      setDrag({ device: device.id, over: over === (from ?? "none") ? null : over });
    };
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      setDrag((current) => {
        if (current && current.over !== null) fileDevice(show, device.id, current.over === "none" ? null : current.over);
        return null;
      });
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  };

  const row = (device: Device) => (
    <DeviceRow
      key={device.id}
      device={device}
      show={show}
      stats={status?.render.devices[String(device.id)]}
      onGrip={groups.length > 0 ? startDrag(device) : undefined}
      lifted={drag?.device === device.id}
    />
  );

  const newGroup = () => {
    const taken = groups.map((g) => g.name);
    let n = groups.length + 1;
    while (taken.includes(`Group ${n}`)) n += 1;
    engine.send({ type: "add_light_group", name: `Group ${n}` });
  };

  return (
    <aside className="panel devices">
      <div className="panel-head">
        <h2>Lights</h2>
        {full && show.devices.length > 0 && (
          <IconButton label="New group of lights" onClick={newGroup}>
            <FolderPlus size={15} />
          </IconButton>
        )}
      </div>
      {show.devices.length === 0 ? (
        <p className="hint">Your lights will be listed here once you add them.</p>
      ) : (
        <ul className={`device-list ${drag ? "dragging" : ""}`}>
          {groups.map((group) => {
            const members = show.devices.filter((d) => groupOf(show, d) === group.id);
            const shut = folded.includes(group.id);
            const settings = openGroup === group.id && !drag;
            const binding = show.bindings.find((b) => b.action.type === "light_group" && b.action.id === group.id);
            return (
              <li
                key={`group-${group.id}`}
                ref={(el) => {
                  if (el) blocks.current.set(group.id, el);
                  else blocks.current.delete(group.id);
                }}
                className={`folder ${group.enabled ? "" : "disabled"} ${drag?.over === group.id ? "drop-into" : ""}`}
              >
                <div className="folder-row">
                  <button
                    type="button"
                    className="folder-main"
                    aria-expanded={!shut}
                    title={shut ? "Show the lights in this group" : "Fold this group shut"}
                    onClick={() => toggleFolded(group.id)}
                  >
                    <ChevronRight size={14} className={`chevron ${shut ? "" : "down"}`} />
                    {shut ? <Folder size={15} aria-hidden /> : <FolderOpen size={15} aria-hidden />}
                    <span className="folder-name">{group.name}</span>
                    <span className="folder-count">{members.length}</span>
                    {group.brightness < 0.995 && (
                      <span className="folder-count">{Math.round(group.brightness * 100)}%</span>
                    )}
                  </button>
                  {binding && <Keycap binding={binding} held={live.held.includes(binding.id)} />}
                  {members.length > 0 && (
                    <IconButton
                      label={`Pick the lights in ${group.name} on the stage`}
                      onClick={() =>
                        select(show.fixtures.filter((f) => f.group === group.id).map((f) => f.id))
                      }
                    >
                      <BoxSelect size={15} />
                    </IconButton>
                  )}
                  <IconButton
                    label="Group settings"
                    active={settings}
                    onClick={() => setOpenGroup(settings ? null : group.id)}
                  >
                    <Settings2 size={15} />
                  </IconButton>
                  <Switch
                    checked={group.enabled}
                    label={group.enabled ? "Turn this group's lights off" : "Turn this group's lights on"}
                    onChange={(enabled) => engine.send({ type: "update_light_group", id: group.id, enabled })}
                  />
                </div>
                {settings && <GroupSettings group={group} />}
                {!shut &&
                  (members.length > 0 ? (
                    <ul className="device-list nested">{members.map(row)}</ul>
                  ) : (
                    <p className="hint folder-empty">Empty. Drag lights onto this group.</p>
                  ))}
              </li>
            );
          })}
          {(loose.length > 0 || drag) && groups.length > 0 ? (
            <li
              className={`loose ${drag?.over === "none" ? "drop-into" : ""}`}
              ref={(el) => {
                if (el) blocks.current.set("none", el);
                else blocks.current.delete("none");
              }}
            >
              {loose.length > 0 ? (
                <ul className="device-list">{loose.map(row)}</ul>
              ) : (
                <p className="hint folder-empty">Drop here to take it out of its group.</p>
              )}
            </li>
          ) : (
            loose.map(row)
          )}
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
          {full && <AddVirtual />}
        </>
      )}
    </aside>
  );
}
