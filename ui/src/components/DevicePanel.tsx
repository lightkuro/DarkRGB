// The lights in the show, and every way to add one: found on the network,
// by address, by scanning, as a virtual light for planning, or the demo set.

import {
  BoxSelect,
  ChevronRight,
  Cpu,
  Folder,
  FolderOpen,
  FolderPlus,
  Ghost,
  Grid3x3,
  GripVertical,
  Merge,
  Minus,
  Network,
  Plug,
  Plus,
  Radar,
  Scissors,
  Search,
  Settings2,
  Trash2,
  Usb,
  SlidersHorizontal,
  MousePointer2,
  LayoutList,
  Power,
} from "lucide-react";
import { useEffect, useRef, useState } from "react";
import {
  addingLights,
  engine,
  perFrame,
  select,
  toggleFolded,
  universesOf,
  useFolded,
  useLive,
  useFull,
  useSelection,
  useShow,
  useStatus,
  type ColourOrder,
  type Corner,
  type Device,
  type DeviceStats,
  type Discovered,
  type Fixture,
  type LightGroup,
  type Show,
  stageModePref,
} from "../engine";
import { KeyAssign, Keycap } from "./Keys";
import { shaping } from "./ShapeEditor";
import { ConfirmButton, IconButton, NameField, Slider, Switch } from "./ui";
import { openMenu, SEPARATOR } from "./ContextMenu";

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
  if (device.kind === "sacn" || device.kind === "artnet") {
    return { tone: "ok", text: `Sending ${PROTOCOLS[device.kind]} to ${device.address}, ${universesText(device)}` };
  }
  if (device.kind === "openrgb") {
    return { tone: "ok", text: `Sending to OpenRGB at ${device.address}, its device ${(device.universe ?? 0) + 1}` };
  }
  if (device.kind === "serial") {
    return { tone: "ok", text: `Sending to ${device.address} at ${baudOf(device).toLocaleString()} baud` };
  }
  return { tone: "ok", text: `Sending to ${device.address}` };
}

const PROTOCOLS = { sacn: "sACN", artnet: "Art-Net" } as const;

const DEFAULT_BAUD = 115200;
const BAUDS = [115200, 230400, 460800, 500000, 921600, 1000000, 1500000, 2000000];
const baudOf = (device: Device) => (device.baud ? device.baud : DEFAULT_BAUD);

/** The most frames a second a serial port carries for this many LEDs: ten bits a byte. */
function serialRate(baud: number, leds: number): number {
  return Math.floor(baud / 10 / (6 + 3 * leds));
}

/** The serial ports this computer has, asked of the engine. */
function useSerialPorts(open: boolean): [string[], () => void, string[]] {
  const [ports, setPorts] = useState<string[]>([]);
  const [usb, setUsb] = useState<string[]>([]);
  const [asked, setAsked] = useState(0);
  useEffect(() => {
    if (!open) return;
    let live = true;
    fetch("/api/serial-ports")
      .then((r) => (r.ok ? r.json() : { ports: [] }))
      .then((body: { ports?: string[]; usb?: string[] }) => {
        if (!live) return;
        setPorts(body.ports ?? []);
        setUsb(body.usb ?? []);
      })
      .catch(() => live && setPorts([]));
    return () => {
      live = false;
    };
  }, [open, asked]);
  return [ports, () => setAsked((n) => n + 1), usb];
}

/** Which port a serial strip is on, and how fast it is sent to. */
function SerialPort({ device }: { device: Device }) {
  const [port, setPort] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [ports] = useSerialPorts(true);
  const change = async (update: { address?: string; baud?: number }) => {
    const result = await engine.request({ type: "update_device", id: device.id, ...update });
    setError(result.ok ? null : (result.error ?? "Could not change that."));
  };
  const commit = () => {
    if (port !== null && port.trim() !== "" && port.trim() !== device.address) void change({ address: port.trim() });
    setPort(null);
  };
  const baud = baudOf(device);
  return (
    <>
      <label className="field inline">
        <span>Port</span>
        <input
          type="text"
          className="address"
          aria-label="Serial port"
          list={`ports-${device.id}`}
          value={port ?? device.address}
          onChange={(e) => setPort(e.target.value)}
          onBlur={commit}
          onKeyDown={(e) => {
            e.stopPropagation();
            if (e.key === "Enter") e.currentTarget.blur();
          }}
        />
        <datalist id={`ports-${device.id}`}>
          {ports.map((p) => (
            <option key={p} value={p} />
          ))}
        </datalist>
      </label>
      <label className="field inline">
        <span>Speed</span>
        <select aria-label="Speed" value={baud} onChange={(e) => void change({ baud: Number(e.target.value) })}>
          {BAUDS.map((b) => (
            <option key={b} value={b}>
              {b.toLocaleString()} baud
            </option>
          ))}
        </select>
      </label>
      <p className="hint">
        At this speed {device.pixel_count} LEDs take at most {serialRate(baud, device.pixel_count)} frames a second.
        The board's sketch must listen at the same speed.
      </p>
      {error && <p className="callout error">{error}</p>}
    </>
  );
}

/** "universe 1" or "universes 1 to 3". */
function universesText(device: Device): string {
  const [first, last] = universesOf(device.universe ?? 0, device.pixel_count);
  return first === last ? `universe ${first}` : `universes ${first} to ${last}`;
}

/** Where a pixel controller is, to correct without adding it again. */
function PixelAddress({ device }: { device: Device }) {
  const [address, setAddress] = useState<string | null>(null);
  const [universe, setUniverse] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const commit = async () => {
    const change: { address?: string; universe?: number } = {};
    if (address !== null && address.trim() !== device.address) change.address = address.trim();
    if (universe !== null && /^\d{1,5}$/.test(universe.trim()) && Number(universe) !== (device.universe ?? 0)) {
      change.universe = Number(universe);
    }
    setAddress(null);
    setUniverse(null);
    if (Object.keys(change).length === 0) return;
    const result = await engine.request({ type: "update_device", id: device.id, ...change });
    setError(result.ok ? null : (result.error ?? "Could not change that."));
  };
  const keys = (e: React.KeyboardEvent<HTMLInputElement>) => {
    e.stopPropagation();
    if (e.key === "Enter") e.currentTarget.blur();
  };
  return (
    <>
      <label className="field inline">
        <span>Address</span>
        <input
          type="text"
          className="address"
          aria-label="Address"
          value={address ?? device.address}
          onChange={(e) => setAddress(e.target.value)}
          onBlur={() => void commit()}
          onKeyDown={keys}
        />
      </label>
      <label className="field inline">
        <span>First universe</span>
        <input
          className="number"
          type="text"
          inputMode="numeric"
          aria-label="First universe"
          value={universe ?? String(device.universe ?? 0)}
          onChange={(e) => setUniverse(e.target.value)}
          onBlur={() => void commit()}
          onKeyDown={keys}
        />
      </label>
      {error && <p className="callout error">{error}</p>}
    </>
  );
}

const updaters = new Map<string, (value: number) => void>();
/** One rate-limited sender per device and field, so sliders stay smooth. */
function deviceSetter(id: number, field: "brightness" | "gamma" | "delay_ms") {
  const key = `${id}:${field}`;
  let send = updaters.get(key);
  if (!send) {
    send = perFrame((value: number) => engine.quiet({ type: "update_device", id, [field]: value }));
    updaters.set(key, send);
  }
  return send;
}

const ORDERS: [ColourOrder, string][] = [
  ["rgb", "Red, green, blue"],
  ["grb", "Green, red, blue"],
  ["brg", "Blue, red, green"],
  ["rbg", "Red, blue, green"],
  ["gbr", "Green, blue, red"],
  ["bgr", "Blue, green, red"],
];

/** Amps, as people say them: 2.5 A, 800 mA. */
function amps(milliamps: number): string {
  return milliamps >= 1000 ? `${(milliamps / 1000).toFixed(milliamps >= 10000 ? 0 : 1)} A` : `${Math.round(milliamps)} mA`;
}

/**
 * The most current a device's lights may draw, typed in amps. Empty is
 * no limit. Under it: what the picture is drawing now, by estimate.
 */
function PowerLimit({ device, stats }: { device: Device; stats?: DeviceStats }) {
  const [draft, setDraft] = useState<string | null>(null);
  const limit = device.max_milliamps ?? 0;
  const commit = () => {
    if (draft !== null) {
      const text = draft.trim().replace(",", ".");
      const value = text === "" ? 0 : Number(text);
      if (Number.isFinite(value) && value >= 0) {
        engine.send({ type: "update_device", id: device.id, max_milliamps: Math.round(value * 1000) });
      }
    }
    setDraft(null);
  };
  // Every pixel at full white, by the same estimate the engine uses.
  const flatOut = device.pixel_count * 61;
  return (
    <>
      <label className="field inline">
        <span>Power limit (amps)</span>
        <input
          className="number"
          type="text"
          inputMode="decimal"
          aria-label="Power limit in amps"
          placeholder="None"
          value={draft ?? (limit > 0 ? String(limit / 1000) : "")}
          onChange={(e) => setDraft(e.target.value)}
          onBlur={commit}
          onKeyDown={(e) => {
            e.stopPropagation();
            if (e.key === "Enter") e.currentTarget.blur();
          }}
        />
      </label>
      <p className={`hint${stats?.limited ? " warn" : ""}`}>
        {stats?.milliamps !== undefined && device.enabled
          ? stats.limited
            ? `Dimmed to stay at ${amps(limit)}. `
            : `Drawing about ${amps(stats.milliamps)} now. `
          : ""}
        Flat out, {device.pixel_count} LEDs would draw about {amps(flatOut)}. An estimate for 5 volt strips: set the
        limit a little under what the power supply gives.
      </p>
    </>
  );
}

/**
 * What a light is connected to. A light planned on screen is given its
 * hardware here, and keeps its place, shape, group and settings.
 */
function ConnectHardware({ device }: { device: Device }) {
  const virtual = device.kind === "virtual";
  const [open, setOpen] = useState(false);
  const [kind, setKind] = useState<"wled" | "sacn" | "artnet">("wled");
  const [address, setAddress] = useState("");
  const [universe, setUniverse] = useState("1");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const found = useFound();
  const connect = async (to: Extract<Parameters<typeof engine.request>[0], { type: "connect" }>["to"]) => {
    setBusy(true);
    const result = await engine.request({ type: "connect", device: device.id, to });
    setBusy(false);
    if (!result.ok) return setError(result.error ?? "Could not connect that.");
    setError(null);
    setOpen(false);
    setAddress("");
    if (result.note) engine.toast("info", result.note);
  };
  const choose = (next: "wled" | "sacn" | "artnet") => {
    if (next !== "wled" && universe === (kind === "artnet" ? "0" : "1")) setUniverse(next === "sacn" ? "1" : "0");
    setKind(next);
    setError(null);
  };
  const valid = address.trim() !== "" && (kind === "wled" || /^\d{1,5}$/.test(universe));
  return (
    <div className={`connect${open ? " open" : ""}`}>
      <button type="button" className="text-button connect-toggle" aria-expanded={open} onClick={() => setOpen(!open)}>
        <Plug size={13} /> {virtual ? "Connect to real hardware" : "Change what it is connected to"}
        <ChevronRight size={13} className="chevron" aria-hidden />
      </button>
      {open && (
        <form
          className="connect-form"
          onSubmit={(e) => {
            e.preventDefault();
            if (!valid || busy) return;
            const where = address.trim();
            void connect(
              kind === "wled"
                ? { type: "wled", address: where }
                : { type: "pixels", protocol: kind, address: where, universe: Number(universe) },
            );
          }}
        >
          <p className="hint">
            {virtual
              ? "This light keeps its place, its shape, its group and everything that plays on it. Only where its colours are sent changes."
              : "Point this light at other hardware. Everything else about it stays."}
          </p>
          <div className="segmented" role="radiogroup" aria-label="Kind of hardware">
            {(
              [
                ["wled", "WLED"],
                ["sacn", "sACN"],
                ["artnet", "Art-Net"],
              ] as const
            ).map(([id, name]) => (
              <button key={id} type="button" role="radio" aria-checked={kind === id} className={kind === id ? "selected" : ""} onClick={() => choose(id)}>
                {name}
              </button>
            ))}
          </div>
          {kind === "wled" && found.length > 0 && (
            <ul className="found compact">
              {found.map((d) => (
                <li key={d.address}>
                  <span>
                    {d.name} <span className="hint">{d.leds} LEDs</span>
                  </span>
                  <button type="button" className="button small" disabled={busy} onClick={() => void connect({ type: "wled", address: d.address })}>
                    Use
                  </button>
                </li>
              ))}
            </ul>
          )}
          <input
            type="text"
            className="address"
            aria-label="Address of the hardware"
            placeholder={kind === "wled" ? "IP address, e.g. 192.168.1.50" : "IP address of the controller"}
            value={address}
            onChange={(e) => setAddress(e.target.value)}
            onKeyDown={(e) => e.stopPropagation()}
          />
          {kind !== "wled" && (
            <label className="field inline">
              <span>First universe</span>
              <input
                className="number"
                type="text"
                inputMode="numeric"
                aria-label="First universe"
                value={universe}
                onChange={(e) => setUniverse(e.target.value)}
                onKeyDown={(e) => e.stopPropagation()}
              />
            </label>
          )}
          {error && <p className="callout error">{error}</p>}
          <div className="form-actions">
            {!virtual && (
              <button type="button" className="text-button" disabled={busy} onClick={() => void connect({ type: "nothing" })}>
                Make it preview only
              </button>
            )}
            <button type="submit" className="button small" disabled={!valid || busy}>
              {busy ? "Asking…" : "Connect"}
            </button>
          </div>
        </form>
      )}
    </div>
  );
}

/** A few numbers that add up to `total`, as equal as whole numbers allow. */
function equalParts(total: number, parts: number): number[] {
  const base = Math.floor(total / parts);
  return Array.from({ length: parts }, (_, i) => base + (i < total - base * parts ? 1 : 0));
}

/**
 * Shares of a strip given in percent, as numbers of LEDs: each zone ends
 * at the LED nearest its share, so the counts always add up to the strip.
 * Null when the shares are not sensible or a zone would get no LED.
 */
export function percentParts(total: number, percents: number[]): number[] | null {
  const sum = percents.reduce((a, b) => a + b, 0);
  if (percents.length < 2 || percents.some((p) => !(p > 0)) || Math.abs(sum - 100) > 0.5) return null;
  let done = 0;
  let upTo = 0;
  const parts = percents.map((p, i) => {
    upTo += p;
    const end = i === percents.length - 1 ? total : Math.round((total * upTo) / sum);
    const count = end - done;
    done = end;
    return count;
  });
  return parts.every((n) => n >= 1) ? parts : null;
}

/**
 * Cutting a strip into zones: parts of it that are lights of their own,
 * so the ends can do one thing and the middle another. And, once cut,
 * the zones with their names, and the way back to one strip.
 */
function Zones({ device, own }: { device: Device; own: Fixture[] }) {
  const [open, setOpen] = useState(false);
  const [text, setText] = useState("");
  const [percent, setPercent] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const first = own[0];
  if (!first || own.some((f) => f.shape.type !== "strip")) return null;
  const total = own.reduce((sum, f) => sum + (f.shape.type === "strip" ? f.shape.count : 0), 0);

  if (own.length > 1) {
    return (
      <div className="zones">
        <span className="field-head">
          <span>Zones</span>
        </span>
        <ul className="zone-list">
          {own.map((zone) => (
            <li key={zone.id}>
              <NameField
                value={zone.name}
                label={`Name of the zone ${zone.name}`}
                onCommit={(name) => engine.send({ type: "update_fixture", id: zone.id, name })}
              />
              <span className="hint">
                LEDs {zone.start + 1} to {zone.start + (zone.shape.type === "strip" ? zone.shape.count : 0)}
              </span>
            </li>
          ))}
        </ul>
        <p className="hint">
          Each zone is a light of its own: move it on the stage, put it in a group, and choose it under an effect's
          "Plays on".
        </p>
        <button type="button" className="text-button" onClick={() => engine.send({ type: "join_strip", device: device.id })}>
          <Merge size={13} /> Make it one strip again
        </button>
      </div>
    );
  }
  if (total < 2) return null;
  const typed = text
    .split(/[\s,%]+/)
    .filter(Boolean)
    .map(Number);
  // Typed as shares of the strip, they become numbers of LEDs here.
  const parts = percent ? (percentParts(total, typed) ?? []) : typed;
  const sum = typed.reduce((a, b) => a + b, 0);
  const valid = percent
    ? parts.length >= 2
    : parts.length >= 2 && parts.every((n) => Number.isInteger(n) && n > 0) && sum === total;
  const equal = (n: number) => {
    if (!percent) return equalParts(total, n).join(", ");
    // To one decimal place, the last share taking up what rounding left.
    const share = Math.floor(1000 / n) / 10;
    const last = Math.round((100 - share * (n - 1)) * 10) / 10;
    return [...Array.from({ length: n - 1 }, () => share), last].join(", ");
  };
  const split = async () => {
    const result = await engine.request({ type: "split_strip", fixture: first.id, parts });
    if (!result.ok) return setError(result.error ?? "Could not cut it like that.");
    setError(null);
    setOpen(false);
  };
  return (
    <div className={`connect${open ? " open" : ""}`}>
      <button
        type="button"
        className="text-button connect-toggle"
        aria-expanded={open}
        onClick={() => {
          if (!open) setText(equalParts(total, 3).join(", "));
          setOpen(!open);
        }}
      >
        <Scissors size={13} /> Cut into zones
        <ChevronRight size={13} className="chevron" aria-hidden />
      </button>
      {open && (
        <form
          className="connect-form"
          onSubmit={(e) => {
            e.preventDefault();
            if (valid) void split();
          }}
        >
          <p className="hint">
            Zones are parts of the strip that act as lights of their own, so the ends can play one effect and the
            middle another. Give the size of each, from the strip's first LED.
          </p>
          <div className="segmented" role="radiogroup" aria-label="Sizes given in">
            <button
              type="button"
              role="radio"
              aria-checked={!percent}
              className={!percent ? "selected" : ""}
              onClick={() => {
                // What was typed is kept, turned into the other measure.
                if (percent && parts.length >= 2) setText(parts.join(", "));
                setPercent(false);
              }}
            >
              Numbers of LEDs
            </button>
            <button
              type="button"
              role="radio"
              aria-checked={percent}
              className={percent ? "selected" : ""}
              onClick={() => {
                if (!percent && valid) setText(parts.map((n) => Math.round((n / total) * 1000) / 10).join(", "));
                setPercent(true);
              }}
            >
              Percent
            </button>
          </div>
          <div className="chips">
            {[2, 3, 4, 5].filter((n) => n <= total).map((n) => (
              <button key={n} type="button" className="chip" onClick={() => setText(equal(n))}>
                {n} equal
              </button>
            ))}
            {total >= 5 && (
              <button
                type="button"
                className="chip"
                title="A fifth at each end and the rest in the middle"
                onClick={() => setText(percent ? "20, 60, 20" : (percentParts(total, [20, 60, 20]) ?? []).join(", "))}
              >
                Ends and middle
              </button>
            )}
          </div>
          <input
            type="text"
            aria-label={percent ? "Percent of the strip in each zone" : "LEDs in each zone"}
            placeholder={percent ? "e.g. 20, 60, 20" : "e.g. 30, 90, 30"}
            value={text}
            onChange={(e) => setText(e.target.value)}
            onKeyDown={(e) => e.stopPropagation()}
          />
          {percent ? (
            <p className={`hint${typed.length >= 2 && !valid ? " warn" : ""}`}>
              {typed.length < 2
                ? "Two or more shares that add up to 100."
                : valid
                  ? `${parts.length} zones of ${parts.join(", ")} LEDs.`
                  : Math.abs(sum - 100) > 0.5
                    ? `These add up to ${Math.round(sum * 10) / 10}%, not 100%.`
                    : "A zone that small would have no LED."}
            </p>
          ) : (
            <p className={`hint${parts.length >= 2 && sum !== total ? " warn" : ""}`}>
              {parts.length < 2
                ? `Two or more numbers that add up to ${total}.`
                : sum === total
                  ? `${parts.length} zones, ${total} LEDs in all.`
                  : `These add up to ${sum}; the strip has ${total}.`}
            </p>
          )}
          {error && <p className="callout error">{error}</p>}
          <div className="form-actions">
            <button type="submit" className="button small" disabled={!valid}>
              Cut
            </button>
          </div>
        </form>
      )}
    </div>
  );
}

const CORNERS: [Corner, string][] = [
  ["top_left", "Top left"],
  ["top_right", "Top right"],
  ["bottom_left", "Bottom left"],
  ["bottom_right", "Bottom right"],
];

/** A small drawing of the order a matrix's LEDs are wired in. */
function WiringDiagram({ fixture }: { fixture: Fixture }) {
  if (fixture.shape.type !== "matrix") return null;
  // A few lines are enough to show the way it goes.
  const cols = Math.min(fixture.shape.cols, 6);
  const rows = Math.min(fixture.shape.rows, 5);
  const { serpentine } = fixture.shape;
  const start = fixture.wiring?.start ?? "top_left";
  const vertical = fixture.wiring?.vertical ?? false;
  const [lines, perLine] = vertical ? [cols, rows] : [rows, cols];
  const points: [number, number][] = [];
  for (let line = 0; line < lines; line++) {
    for (let step = 0; step < perLine; step++) {
      const s = serpentine && line % 2 === 1 ? perLine - 1 - step : step;
      let [col, row] = vertical ? [line, s] : [s, line];
      if (start.endsWith("right")) col = cols - 1 - col;
      if (start.startsWith("bottom")) row = rows - 1 - row;
      points.push([14 + col * 22, 14 + row * 22]);
    }
  }
  const [fx, fy] = points[0]!;
  return (
    <svg
      className="wiring-diagram"
      width={28 + (cols - 1) * 22}
      height={28 + (rows - 1) * 22}
      role="img"
      aria-label="The order the LEDs are wired in, from the ringed one"
    >
      <polyline points={points.map((p) => p.join(",")).join(" ")} />
      {points.map(([x, y], i) => (
        <circle key={i} cx={x} cy={y} r={3} />
      ))}
      <circle className="first" cx={fx} cy={fy} r={7} />
    </svg>
  );
}

/** How a matrix is wired, and the way back to a plain strip. */
function MatrixWiring({ fixture, full, wled }: { fixture: Fixture; full: boolean; wled: boolean }) {
  if (fixture.shape.type !== "matrix") return null;
  const start = fixture.wiring?.start ?? "top_left";
  const vertical = fixture.wiring?.vertical ?? false;
  const byDevice = fixture.wiring?.by_device ?? false;
  const set = (change: { serpentine?: boolean; start?: Corner; vertical?: boolean; by_device?: boolean }) =>
    engine.send({ type: "set_wiring", fixture: fixture.id, ...change });
  const plain = (
    <button
      type="button"
      className="text-button"
      title="Treat these LEDs as one long strip again"
      onClick={() => engine.send({ type: "reshape", fixture: fixture.id, shape: { type: "strip", count: fixture.shape.type === "matrix" ? fixture.shape.cols * fixture.shape.rows : 0 } })}
    >
      <Minus size={13} /> Make it a plain strip
    </button>
  );
  // Where the wiring is described: in WLED's own 2D settings, or here
  // with WLED left as one long strip. Either works; both at once does not.
  const who = (wled || byDevice) && (
    <div className="field">
      <span className="field-head">
        <span>The wiring is set up</span>
      </span>
      <div className="segmented" role="radiogroup" aria-label="Where the wiring is set up">
        <button type="button" role="radio" aria-checked={byDevice} className={byDevice ? "selected" : ""} onClick={() => set({ by_device: true })}>
          In WLED
        </button>
        <button type="button" role="radio" aria-checked={!byDevice} className={!byDevice ? "selected" : ""} onClick={() => set({ by_device: false })}>
          Here, in darkRGB
        </button>
      </div>
    </div>
  );
  if (byDevice) {
    return (
      <div className="wiring">
        {who}
        <p className="hint">
          WLED's own 2D settings say how this panel is wired, and darkRGB sends it plain rows from the top left. If
          pictures come out scrambled, either fix the 2D settings in WLED, or choose "Here" and set WLED back to one
          plain strip.
        </p>
        {full && plain}
      </div>
    );
  }
  return (
    <div className="wiring">
      {who}
      {!who && (
        <span className="field-head">
          <span>Wiring</span>
        </span>
      )}
      <div className="wiring-row">
        <WiringDiagram fixture={fixture} />
        <p className="hint">
          Say how the LEDs are wired, so pictures and words come out the right way round. The ring is the first LED.
          {wled ? " WLED itself should then be left as one plain strip, with no 2D set-up of its own." : ""}
        </p>
      </div>
      <div className="field inline">
        <span>
          Snaked
          <span className="hint">One strip folded back and forth: every other line runs the other way.</span>
        </span>
        <Switch checked={fixture.shape.serpentine} label="Snaked: every other line runs the other way" onChange={(serpentine) => set({ serpentine })} />
      </div>
      <label className="field inline">
        <span>First LED</span>
        <select aria-label="Corner of the first LED" value={start} onChange={(e) => set({ start: e.target.value as Corner })}>
          {CORNERS.map(([id, name]) => (
            <option key={id} value={id}>
              {name}
            </option>
          ))}
        </select>
      </label>
      <div className="segmented" role="radiogroup" aria-label="Which way the LEDs run">
        <button type="button" role="radio" aria-checked={!vertical} className={!vertical ? "selected" : ""} onClick={() => set({ vertical: false })}>
          Along the rows
        </button>
        <button type="button" role="radio" aria-checked={vertical} className={vertical ? "selected" : ""} onClick={() => set({ vertical: true })}>
          Up and down the columns
        </button>
      </div>
      {full && plain}
    </div>
  );
}

/** Saying that a strip is really snaked back and forth into a matrix. */
function AsMatrix({ fixture }: { fixture: Fixture }) {
  const [open, setOpen] = useState(false);
  const [cols, setCols] = useState("");
  const [error, setError] = useState<string | null>(null);
  if (fixture.shape.type !== "strip" || fixture.shape.count < 4) return null;
  const count = fixture.shape.count;
  const across = Number(cols);
  const rows = Number.isInteger(across) && across >= 2 ? count / across : NaN;
  const valid = Number.isInteger(rows) && rows >= 2;
  // Widths that divide the strip evenly, nearest to square first.
  const fits = Array.from({ length: count }, (_, i) => i + 2)
    .filter((n) => count % n === 0 && count / n >= 2)
    .sort((a, b) => Math.abs(a - count / a) - Math.abs(b - count / b))
    .slice(0, 4);
  const make = async () => {
    const result = await engine.request({ type: "reshape", fixture: fixture.id, shape: { type: "matrix", cols: across, rows } });
    if (!result.ok) return setError(result.error ?? "Could not arrange it like that.");
    setError(null);
    setOpen(false);
  };
  return (
    <div className={`connect${open ? " open" : ""}`}>
      <button type="button" className="text-button connect-toggle" aria-expanded={open} onClick={() => setOpen(!open)}>
        <Grid3x3 size={13} /> It is snaked into a matrix
        <ChevronRight size={13} className="chevron" aria-hidden />
      </button>
      {open && (
        <form
          className="connect-form"
          onSubmit={(e) => {
            e.preventDefault();
            if (valid) void make();
          }}
        >
          <p className="hint">
            For a strip folded back and forth into rows. Give how many LEDs are in one row; the {count} LEDs must
            fill whole rows. The wiring can be set afterwards.
          </p>
          {fits.length > 0 && (
            <div className="chips">
              {fits.map((n) => (
                <button key={n} type="button" className="chip" onClick={() => setCols(String(n))}>
                  {n} × {count / n}
                </button>
              ))}
            </div>
          )}
          <label className="field inline">
            <span>LEDs in a row</span>
            <input className="number" type="text" inputMode="numeric" aria-label="LEDs in a row" value={cols} onChange={(e) => setCols(e.target.value)} onKeyDown={(e) => e.stopPropagation()} />
          </label>
          <p className={`hint${cols !== "" && !valid ? " warn" : ""}`}>
            {cols === "" ? `${count} LEDs.` : valid ? `${across} across and ${rows} down.` : `${count} LEDs do not make whole rows of ${cols}.`}
          </p>
          {error && <p className="callout error">{error}</p>}
          <div className="form-actions">
            <button type="submit" className="button small" disabled={!valid}>
              Arrange as a matrix
            </button>
          </div>
        </form>
      )}
    </div>
  );
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
          title="Settings for this light. Shift+click to pick several. Right-click for more."
          onContextMenu={(e) =>
            openMenu(
              e,
              [
                { label: open ? "Close its settings" : "Open its settings", icon: <SlidersHorizontal size={13} />, action: () => setOpen(!open) },
                { label: "Pick it on the stage", icon: <MousePointer2 size={13} />, action: () => select(own.map((f) => f.id)) },
                { label: "See it light by light", icon: <LayoutList size={13} />, action: () => stageModePref.set("lights") },
                ...(device.kind !== "virtual"
                  ? [{ label: device.enabled ? "Pause sending to it" : "Send to it again", icon: <Power size={13} />, action: () => engine.send({ type: "update_device", id: device.id, enabled: !device.enabled }) }]
                  : []),
                SEPARATOR,
                { label: "Remove the light", icon: <Trash2 size={13} />, danger: true, action: () => engine.send({ type: "remove_device", id: device.id }) },
              ],
              device.name,
            )
          }
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
          {own.length === 1 && own[0]!.shape.type === "strip" && (
            <div className="field inline">
              <span>Shape</span>
              <button type="button" className="text-button" onClick={() => shaping.set(own[0]!.id)}>
                {(own[0]!.path?.length ?? 0) >= 2 ? `${own[0]!.path!.length} corners` : "Straight"} · Change…
              </button>
            </div>
          )}
          {own.length === 1 && own[0]!.shape.type === "matrix" && (
            <MatrixWiring fixture={own[0]!} full={full} wled={device.kind === "wled"} />
          )}
          {own.length === 1 && own[0]!.shape.type === "strip" && (own[0]!.path?.length ?? 0) < 2 && <AsMatrix fixture={own[0]!} />}
          {(own.length > 1 || (own[0]?.path?.length ?? 0) < 2) && <Zones device={device} own={own} />}
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
          {(device.kind === "sacn" || device.kind === "artnet") && <PixelAddress device={device} />}
          {device.kind === "serial" && <SerialPort device={device} />}
          {(virtual || full) && <ConnectHardware device={device} />}
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
                  {(device.kind === "sacn" || device.kind === "artnet" || device.kind === "serial") && (
                    <label className="field inline">
                      <span>Colour order</span>
                      <select
                        aria-label="Colour order"
                        value={device.order ?? "rgb"}
                        onChange={(e) =>
                          engine.send({ type: "update_device", id: device.id, order: e.target.value as ColourOrder })
                        }
                      >
                        {ORDERS.map(([id, name]) => (
                          <option key={id} value={id}>
                            {name}
                          </option>
                        ))}
                      </select>
                    </label>
                  )}
                  <PowerLimit device={device} stats={stats} />
                  <Slider
                    label="Delay"
                    min={0}
                    max={500}
                    step={5}
                    value={device.delay_ms ?? 0}
                    format={(v) => (v === 0 ? "None" : `${Math.round(v)} ms`)}
                    onChange={deviceSetter(device.id, "delay_ms")}
                  />
                  <p className="hint">Holds this light back to line it up with slower ones.</p>
                  <div className="field inline">
                    <span>
                      Smoother dim levels
                      <span className="hint"> Finer steps in slow fades near black. Switch off if it shimmers.</span>
                    </span>
                    <Switch
                      checked={device.dither ?? false}
                      label="Smoother dim levels"
                      onChange={(dither) => engine.send({ type: "update_device", id: device.id, dither })}
                    />
                  </div>
                </>
              )}
            </>
          )}
          <div className="device-footer">
            <span className="hint">
              {device.kind === "sacn" || device.kind === "artnet"
                ? `${PROTOCOLS[device.kind]} · ${universesText(device)}`
                : device.kind === "openrgb"
                  ? `OpenRGB · device ${(device.universe ?? 0) + 1}`
                  : device.kind === "serial"
                  ? `Serial · Adalight · ${baudOf(device).toLocaleString()} baud`
                  : device.version
                  ? `WLED ${device.version}`
                  : ""}
            </span>
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

/**
 * The lights in and around a computer, by way of the OpenRGB program:
 * it is asked what it has, and each of its devices becomes a light.
 */
function AddOpenRgb() {
  const [open, setOpen] = useState(false);
  const [address, setAddress] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const add = async () => {
    setBusy(true);
    const result = await engine.request({ type: "add_open_rgb", address: address.trim() });
    setBusy(false);
    if (!result.ok) return setError(result.error ?? "OpenRGB could not be reached.");
    setError(null);
    setOpen(false);
    if (result.note) engine.toast("info", result.note);
  };
  return (
    <div className={`connect${open ? " open" : ""}`}>
      <button type="button" className="text-button connect-toggle" aria-expanded={open} onClick={() => setOpen(!open)}>
        <Cpu size={13} /> Add a computer's own lights (OpenRGB)
        <ChevronRight size={13} className="chevron" aria-hidden />
      </button>
      {open && (
        <form
          className="connect-form"
          onSubmit={(e) => {
            e.preventDefault();
            if (!busy) void add();
          }}
        >
          <p className="hint">
            Keyboards, memory, fans and the like, through the free OpenRGB program. Start OpenRGB on the computer
            that has them and switch on its SDK server; every device it lists is added here as a light.
          </p>
          <label className="field">
            <span className="field-head">
              <span>Where OpenRGB runs</span>
            </span>
            <input
              type="text"
              aria-label="Address of the computer OpenRGB runs on"
              placeholder="This computer (127.0.0.1)"
              value={address}
              onChange={(e) => setAddress(e.target.value)}
              onKeyDown={(e) => e.stopPropagation()}
            />
          </label>
          {error && <p className="callout error">{error}</p>}
          <div className="form-actions">
            <button type="submit" className="button small" disabled={busy}>
              {busy ? "Asking OpenRGB…" : "Add its lights"}
            </button>
          </div>
        </form>
      )}
    </div>
  );
}

/**
 * A strip on a serial port: an Arduino, an ESP or a WLED board on a USB
 * cable, running a sketch that takes the Adalight protocol.
 */
export function AddSerial() {
  const [open, setOpen] = useState(false);
  const [name, setName] = useState("");
  const [port, setPort] = useState("");
  const [baud, setBaud] = useState(DEFAULT_BAUD);
  const [count, setCount] = useState("60");
  const [error, setError] = useState<string | null>(null);
  const [ports, again, usb] = useSerialPorts(open);
  // A board on USB is filled in; a port built into the computer rarely
  // has a strip on it, so it is offered but not chosen.
  useEffect(() => {
    if (port === "" && usb.length > 0) setPort(usb[0] ?? "");
  }, [usb, port]);
  const opener = (
    <button type="button" className="text-button connect-toggle" aria-expanded={open} onClick={() => setOpen(!open)}>
      <Usb size={13} /> Add a strip on a USB cable (serial)
      <ChevronRight size={13} className="chevron" aria-hidden />
    </button>
  );
  if (!open) return opener;
  const valid = port.trim() !== "" && /^\d{1,6}$/.test(count) && Number(count) > 0;
  const add = async () => {
    addingLights();
    const result = await engine.request({
      type: "add_serial",
      name: name.trim() || "Serial strip",
      port: port.trim(),
      baud,
      shape: { type: "strip", count: Number(count) },
    });
    if (result.ok) {
      setOpen(false);
      setName("");
      setError(null);
    } else {
      setError(result.error ?? "Could not add that strip.");
    }
  };
  return (
    <div className="connect open">
      {opener}
      <form
        className="virtual-form"
        onSubmit={(e) => {
          e.preventDefault();
          if (valid) void add();
        }}
      >
        <p className="hint">
          For a strip driven by an Arduino, an ESP or a WLED board plugged in by USB, running a sketch that takes the
          Adalight protocol (WLED does, on its USB port). Not yet tried on a real strip: tell us how it goes.
        </p>
        <input type="text" maxLength={80} placeholder="Name (optional)" aria-label="Name" value={name} onChange={(e) => setName(e.target.value)} />
        <label className="field inline">
          <span>Port</span>
          <span className="pair">
            <input
              type="text"
              maxLength={128}
              list="serial-ports"
              placeholder="/dev/ttyUSB0 or COM3"
              aria-label="Serial port"
              value={port}
              onChange={(e) => setPort(e.target.value)}
              onKeyDown={(e) => e.stopPropagation()}
            />
            <button type="button" className="text-button" onClick={again} title="Look for ports again">
              Look again
            </button>
          </span>
          <datalist id="serial-ports">
            {ports.map((p) => (
              <option key={p} value={p} />
            ))}
          </datalist>
        </label>
        {usb.length === 0 && (
          <p className="hint">
            No board found on USB. Plug it in, then choose Look again, or type the port's name
            {ports.length > 0 ? ` (this computer has ${ports.join(", ")})` : ""}.
          </p>
        )}
        <label className="field inline">
          <span>Number of LEDs</span>
          <input className="number" type="text" inputMode="numeric" aria-label="Number of LEDs" value={count} onChange={(e) => setCount(e.target.value)} />
        </label>
        <label className="field inline">
          <span>Speed</span>
          <select aria-label="Speed" value={baud} onChange={(e) => setBaud(Number(e.target.value))}>
            {BAUDS.map((b) => (
              <option key={b} value={b}>
                {b.toLocaleString()} baud
              </option>
            ))}
          </select>
        </label>
        {valid && (
          <p className="hint">
            At most {serialRate(baud, Number(count))} frames a second at this speed. Use the speed the sketch is set to.
          </p>
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
    </div>
  );
}

/**
 * A pixel controller that takes sACN (E1.31) or Art-Net, as the
 * controllers made for outdoor light shows do. Nothing can be asked of
 * such a controller, so its LEDs and universe are typed in.
 */
export function AddPixels() {
  const [open, setOpen] = useState(false);
  const [protocol, setProtocol] = useState<"sacn" | "artnet">("sacn");
  const [name, setName] = useState("");
  const [address, setAddress] = useState("");
  const [universe, setUniverse] = useState("1");
  const [kind, setKind] = useState<"strip" | "matrix">("strip");
  const [count, setCount] = useState("170");
  const [cols, setCols] = useState("16");
  const [rows, setRows] = useState("16");
  const [error, setError] = useState<string | null>(null);

  // The button that opens the form stays where it is and closes it again.
  const opener = (
    <button type="button" className="text-button connect-toggle" aria-expanded={open} onClick={() => setOpen(!open)}>
      <Network size={13} /> Add an sACN or Art-Net controller
      <ChevronRight size={13} className="chevron" aria-hidden />
    </button>
  );
  if (!open) return opener;
  const whole = (value: string) => /^\d{1,6}$/.test(value);
  const pixels = kind === "strip" ? Number(count) : Number(cols) * Number(rows);
  const valid =
    address.trim() !== "" && /^\d{1,5}$/.test(universe) && (kind === "strip" ? whole(count) : whole(cols) && whole(rows));
  const [first, last] = universesOf(Number(universe), valid ? pixels : 1);
  const choose = (next: "sacn" | "artnet") => {
    // sACN counts universes from 1, Art-Net from 0. Move the usual start
    // along unless another has been typed.
    if (universe === (protocol === "sacn" ? "1" : "0")) setUniverse(next === "sacn" ? "1" : "0");
    setProtocol(next);
  };
  const add = async () => {
    const shape =
      kind === "strip"
        ? ({ type: "strip", count: Number(count) } as const)
        : ({ type: "matrix", cols: Number(cols), rows: Number(rows) } as const);
    const fallback = `${PROTOCOLS[protocol]} ${kind}`;
    addingLights();
    const result = await engine.request({
      type: "add_pixels",
      name: name.trim() || fallback,
      protocol,
      address: address.trim(),
      universe: Number(universe),
      shape,
    });
    if (result.ok) {
      setOpen(false);
      setName("");
      setError(null);
    } else {
      setError(result.error ?? "Could not add that controller.");
    }
  };
  return (
    <div className="connect open">
      {opener}
    <form
      className="virtual-form"
      onSubmit={(e) => {
        e.preventDefault();
        void add();
      }}
    >
      <p className="hint">
        For pixel controllers that are not WLED, such as the ones made for outdoor light shows. Set the controller
        to receive {PROTOCOLS[protocol]} sent straight to its address (unicast), with its LEDs in RGB order.
      </p>
      <div className="segmented" role="radiogroup" aria-label="Protocol">
        <button type="button" role="radio" aria-checked={protocol === "sacn"} className={protocol === "sacn" ? "selected" : ""} onClick={() => choose("sacn")}>
          sACN (E1.31)
        </button>
        <button type="button" role="radio" aria-checked={protocol === "artnet"} className={protocol === "artnet" ? "selected" : ""} onClick={() => choose("artnet")}>
          Art-Net
        </button>
      </div>
      <input type="text" maxLength={80} placeholder="Name (optional)" aria-label="Name" value={name} onChange={(e) => setName(e.target.value)} />
      <input type="text" maxLength={253} placeholder="Address, such as 192.168.1.50" aria-label="Address" value={address} onChange={(e) => setAddress(e.target.value)} />
      <div className="segmented" role="radiogroup" aria-label="Shape">
        <button type="button" role="radio" aria-checked={kind === "strip"} className={kind === "strip" ? "selected" : ""} onClick={() => setKind("strip")}>
          Strip
        </button>
        <button type="button" role="radio" aria-checked={kind === "matrix"} className={kind === "matrix" ? "selected" : ""} onClick={() => setKind("matrix")}>
          Matrix
        </button>
      </div>
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
      <label className="field inline">
        <span>First universe</span>
        <input className="number" type="text" inputMode="numeric" aria-label="First universe" value={universe} onChange={(e) => setUniverse(e.target.value)} />
      </label>
      {valid && (
        <p className="hint">
          170 LEDs fit in a universe, so {pixels} LEDs use {first === last ? `universe ${first}` : `universes ${first} to ${last}`}.
        </p>
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
    </div>
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

  // The button that opens the form stays where it is and closes it again.
  const opener = (
    <button type="button" className="text-button connect-toggle" aria-expanded={open} onClick={() => setOpen(!open)}>
      <Ghost size={13} /> Add a virtual light
      <ChevronRight size={13} className="chevron" aria-hidden />
    </button>
  );
  if (!open) return opener;
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
    <div className="connect open">
      {opener}
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
    </div>
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
        {show.devices.length > 1 && (
          <button
            type="button"
            className="text-button"
            title="A named group of lights, such as Roof or Stage left. Drag lights into it, or pick them on the stage and choose Group. Effects can then play on the group."
            onClick={newGroup}
          >
            <FolderPlus size={14} /> New group
          </button>
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
          <h2>
            <Network size={13} aria-hidden /> Other pixel controllers
          </h2>
          <AddPixels />
          <AddOpenRgb />
          <AddSerial />
          <h2>Without hardware</h2>
          <DemoSwitch />
          <AddVirtual />
        </>
      )}
    </aside>
  );
}
