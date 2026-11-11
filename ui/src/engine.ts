// Connection to the engine, and the types of everything it sends.
//
// Slow-changing data (the show, status) goes through React via
// useSyncExternalStore. Fast data (preview colours 30 times a second,
// audio features) is kept out of React and read by canvases directly.

import { useSyncExternalStore } from "react";

export type ParamValue = boolean | number | string;

export interface ParamSpec {
  id: string;
  label: string;
  /** Hidden until the user switches advanced settings on. */
  advanced: boolean;
  kind: "float" | "color" | "bool" | "choice" | "palette";
  min?: number;
  max?: number;
  step?: number;
  options?: string[];
  default: ParamValue;
}

export interface EffectInfo {
  id: string;
  name: string;
  description: string;
  /** The group it is listed under in the picker. */
  category: string;
  audio: boolean;
  params: ParamSpec[];
}

export interface Palette {
  id: string;
  name: string;
  stops: string[];
  cyclic: boolean;
}

export interface Hello {
  app: string;
  version: string;
  /** CI build number, when this program was built there. */
  build: string | null;
  /** Edges of the 32 frequency bands in Hz (33 numbers). */
  band_hz: number[];
  /** First band and one past the last band of bass, mids and treble. */
  band_groups: [number, number][];
  /** Frame rates the engine can run at. */
  frame_rates: number[];
  effects: EffectInfo[];
  palettes: Palette[];
}

export interface Device {
  id: number;
  name: string;
  /** A virtual light exists only in the preview. */
  kind: "wled" | "virtual";
  /** One of the built-in demo lights. */
  demo: boolean;
  address: string;
  port: number;
  pixel_count: number;
  enabled: boolean;
  gamma: number;
  brightness: number;
  mac: string | null;
  version: string | null;
}

export type Shape =
  | { type: "strip"; count: number; reverse: boolean }
  | { type: "matrix"; cols: number; rows: number; serpentine: boolean };

export interface Fixture {
  id: number;
  name: string;
  device: number;
  start: number;
  shape: Shape;
  x: number;
  y: number;
  width: number;
  height: number;
  rotation: number;
  /** The group of lights this fixture is filed under, if any. */
  group: number | null;
}

/** A named set of lights that can be aimed at, dimmed or switched as one. */
export interface LightGroup {
  id: number;
  name: string;
  enabled: boolean;
  brightness: number;
}

/** A folder of effects. */
export interface LayerGroup {
  id: number;
  name: string;
  enabled: boolean;
  opacity: number;
}

/** What a key does. */
export type Action =
  | { type: "scene"; id: number }
  | { type: "layer"; id: number }
  | { type: "layer_group"; id: number }
  | { type: "light_group"; id: number }
  | { type: "blackout" }
  | { type: "tap_tempo" }
  | { type: "hit" };

/** Whether a key flips its thing on each press, or only while held down. */
export type KeyMode = "toggle" | "hold";

export interface Binding {
  id: number;
  /** The key's place on the keyboard, such as `KeyA`. */
  key: string;
  /** What is printed on the key. */
  label: string;
  action: Action;
  mode: KeyMode;
}

export type Launch = "now" | "beat" | "bar";

export type Blend = "normal" | "add" | "lighten" | "multiply";

/** The part of the spectrum a layer listens to. */
export type Band =
  | { type: "all" }
  | { type: "bass" }
  | { type: "mid" }
  | { type: "treble" }
  | { type: "range"; low: number; high: number };

export interface Sound {
  band: Band;
  /** Multiplier on what the layer hears, 0.25 to 4. */
  gain: number;
  /** How much the layer's brightness follows the sound, 0 to 1. */
  pulse: number;
}

export interface Layer {
  id: number;
  /** A label the user gave it; otherwise the effect's name is shown. */
  name: string | null;
  sound: Sound;
  effect: string;
  params: Record<string, ParamValue>;
  opacity: number;
  blend: Blend;
  /** Ids of fixtures and groups of lights; empty means every light. */
  targets: number[];
  enabled: boolean;
  /** The folder this layer is filed in, if any. */
  group: number | null;
}

export interface Scene {
  id: number;
  name: string;
  layers: Layer[];
  groups: LayerGroup[];
}

export interface Show {
  version: number;
  name: string;
  devices: Device[];
  fixtures: Fixture[];
  light_groups: LightGroup[];
  layers: Layer[];
  layer_groups: LayerGroup[];
  scenes: Scene[];
  bindings: Binding[];
  master: { brightness: number; blackout: boolean; limit: number };
  audio: { source: string; sensitivity: number; delay_ms: number };
  live: { launch: Launch; fade: number };
  output: {
    frame_rate: number;
    /** Most flashes of the whole picture in a second; 0 means no limit. */
    flash_limit: number;
  };
}

/** What is happening live, beyond what is saved in the show. */
export interface Live {
  /** Ids of the keys being held down. */
  held: number[];
  /** The scene waiting for the beat. */
  pending_scene: number | null;
  /** The tempo tapped by hand, or 0 when the music's own is followed. */
  tapped_bpm: number;
}

export interface DeviceStats {
  frames_sent: number;
  frames_dropped: number;
  resolved: boolean;
}

export interface Discovered {
  name: string;
  address: string;
  leds: number;
  matrix: [number, number] | null;
  version: string;
  mac: string | null;
  added: boolean;
}

export interface Status {
  render: {
    fps: number;
    frame_ms: number;
    late_frames: number;
    /** The flash limit held the picture back in the last second. */
    flash_limited: boolean;
    devices: Record<string, DeviceStats>;
  };
  audio: {
    state: "starting" | "running" | "error";
    source: string;
    device: string | null;
    sample_rate: number;
    channels: number;
    message: string | null;
  };
  inputs: { id: string; name: string }[];
  /** Local networks this computer is on, best guess first. */
  networks: { address: string; prefix: string }[];
  scan: { running: boolean; prefix: string; done: number; total: number; found: number };
  discovered: Discovered[];
  notice: string | null;
  /** Where the show is saved on the computer running the engine. */
  show_file: string | null;
}

export interface AudioFeatures {
  level: number;
  bass: number;
  mid: number;
  treble: number;
  bands: number[];
  onset: number;
  onsets: number[];
  beat: number;
  beat_phase: number;
  bpm: number;
  beat_count: number;
  active: boolean;
}

export type Command =
  | { type: "add_device"; address: string }
  | { type: "scan_network"; prefix: string }
  | { type: "set_demo"; enabled: boolean }
  | {
      type: "add_virtual";
      name: string;
      shape: { type: "strip"; count: number } | { type: "matrix"; cols: number; rows: number };
    }
  | { type: "rename_layer"; id: number; name: string }
  | { type: "duplicate_layer"; id: number }
  | { type: "set_sound"; layer: number; band?: Band; gain?: number; pulse?: number }
  | { type: "remove_device"; id: number }
  | { type: "update_device"; id: number; name?: string; enabled?: boolean; gamma?: number; brightness?: number }
  | {
      type: "update_fixture";
      id: number;
      name?: string;
      x?: number;
      y?: number;
      width?: number;
      height?: number;
      rotation?: number;
      reverse?: boolean;
    }
  | { type: "add_layer"; effect: string; group?: number | null }
  | { type: "remove_layer"; id: number }
  | { type: "arrange_layers"; order: { id: number; group: number | null }[] }
  | { type: "add_layer_group"; name: string; layer?: number }
  | { type: "update_layer_group"; id: number; name?: string; enabled?: boolean; opacity?: number }
  | { type: "remove_layer_group"; id: number; keep_layers: boolean }
  | { type: "add_light_group"; name: string }
  | { type: "update_light_group"; id: number; name?: string; enabled?: boolean; brightness?: number }
  | { type: "remove_light_group"; id: number }
  | { type: "group_lights"; fixtures: number[]; group: number | null }
  | { type: "bind"; key: string; label: string; action: Action; mode: KeyMode }
  | { type: "update_binding"; id: number; mode: KeyMode }
  | { type: "unbind"; id: number }
  | { type: "trigger"; binding: number; down: boolean }
  | { type: "tap" }
  | { type: "hit" }
  | { type: "auto_tempo" }
  | { type: "set_live"; launch?: Launch; fade?: number }
  | { type: "set_output"; frame_rate?: number; flash_limit?: number }
  | { type: "rename_scene"; id: number; name: string }
  | { type: "new_show"; keep_lights: boolean }
  | { type: "export_show" }
  | {
      type: "update_layer";
      id: number;
      effect?: string;
      opacity?: number;
      blend?: Blend;
      enabled?: boolean;
      targets?: number[];
    }
  | { type: "set_param"; layer: number; param: string; value: ParamValue }
  | { type: "reset_params"; layer: number }
  | { type: "set_master"; brightness?: number; blackout?: boolean; limit?: number }
  | { type: "set_audio"; source?: string; sensitivity?: number; delay_ms?: number }
  | { type: "save_scene"; name: string }
  | { type: "recall_scene"; id: number }
  | { type: "delete_scene"; id: number }
  | { type: "rename_show"; name: string }
  | { type: "dismiss_notice" };

export interface CommandResult {
  ok: boolean;
  note?: string | null;
  error?: string;
}

export interface Toast {
  id: number;
  kind: "error" | "info";
  text: string;
}

const MSG_PREVIEW = 1;
const MSG_LAYOUT = 2;

const SILENCE: AudioFeatures = {
  level: 0,
  bass: 0,
  mid: 0,
  treble: 0,
  bands: new Array<number>(32).fill(0),
  onset: 0,
  onsets: [0, 0, 0],
  beat: 0,
  beat_phase: 0,
  bpm: 0,
  beat_count: 0,
  active: false,
};

class Store<T> {
  private listeners = new Set<() => void>();
  constructor(private value: T) {}
  get = (): T => this.value;
  set(value: T): void {
    this.value = value;
    for (const listener of this.listeners) listener();
  }
  subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  };
}

class Engine {
  readonly connected = new Store(false);
  readonly hello = new Store<Hello | null>(null);
  readonly show = new Store<Show | null>(null);
  readonly status = new Store<Status | null>(null);
  readonly live = new Store<Live>({ held: [], pending_scene: null, tapped_bpm: 0 });
  /** The scene that is playing exactly as it was saved, if any. */
  readonly currentScene = new Store<number | null>(null);
  readonly toasts = new Store<Toast[]>([]);

  /** Latest audio features. Read from animation frames, not from React. */
  audio: AudioFeatures = SILENCE;
  /** Canvas x,y of every pixel, in the order preview colours arrive. */
  positions = new Float32Array(0);
  /** RGB for every pixel; only valid when `colorsSerial === layoutSerial`. */
  colors = new Uint8Array(0);
  layoutSerial = 0;
  colorsSerial = -1;
  /** Bumped whenever positions change, so the stage can rebuild buffers. */
  layoutVersion = 0;

  private socket: WebSocket | null = null;
  private nextId = 1;
  private pending = new Map<number, (result: CommandResult) => void>();
  private nextToast = 1;

  connect(): void {
    // One connection only, however often this is called.
    if (this.socket && this.socket.readyState !== WebSocket.CLOSED) return;
    const scheme = location.protocol === "https:" ? "wss" : "ws";
    const socket = new WebSocket(`${scheme}://${location.host}/ws`);
    socket.binaryType = "arraybuffer";
    this.socket = socket;
    socket.onopen = () => this.connected.set(true);
    socket.onmessage = (event) => {
      if (typeof event.data === "string") this.onText(event.data);
      else this.onBinary(event.data as ArrayBuffer);
    };
    socket.onclose = () => {
      this.connected.set(false);
      this.audio = SILENCE;
      // Commands in flight will never be answered now.
      for (const resolve of this.pending.values()) resolve({ ok: false, error: "Lost the connection to the engine." });
      this.pending.clear();
      setTimeout(() => this.connect(), 1000);
    };
  }

  private onText(text: string): void {
    const message = JSON.parse(text) as { type: string } & Record<string, unknown>;
    switch (message.type) {
      case "audio":
        this.audio = message as unknown as AudioFeatures;
        break;
      case "state":
        this.currentScene.set((message.current_scene as number | null) ?? null);
        this.show.set(message.show as Show);
        break;
      case "live":
        this.live.set(message as unknown as Live);
        break;
      case "status":
        this.status.set(message as unknown as Status);
        break;
      case "hello":
        this.hello.set(message as unknown as Hello);
        break;
      case "result": {
        const result = message as unknown as CommandResult & { id: number | null };
        const resolve = result.id == null ? undefined : this.pending.get(result.id);
        if (resolve && result.id != null) {
          this.pending.delete(result.id);
          resolve(result);
        } else if (!result.ok && result.error) {
          this.toast("error", result.error);
        }
        break;
      }
    }
  }

  private onBinary(buffer: ArrayBuffer): void {
    if (buffer.byteLength < 8) return;
    const header = new DataView(buffer);
    const kind = header.getUint8(0);
    const serial = header.getUint32(4, true);
    if (kind === MSG_LAYOUT) {
      this.positions = new Float32Array(buffer.slice(8));
      this.layoutSerial = serial;
      this.layoutVersion += 1;
    } else if (kind === MSG_PREVIEW) {
      this.colors = new Uint8Array(buffer, 8);
      this.colorsSerial = serial;
    }
  }

  /** Sends a command and resolves with the engine's answer. */
  request(command: Command): Promise<CommandResult> {
    const socket = this.socket;
    if (!socket || socket.readyState !== WebSocket.OPEN) {
      return Promise.resolve({ ok: false, error: "Not connected to the engine." });
    }
    const id = this.nextId++;
    return new Promise((resolve) => {
      this.pending.set(id, resolve);
      socket.send(JSON.stringify({ id, ...command }));
    });
  }

  /**
   * Sends a command and shows any error or note as a toast. For controls
   * where there is nothing more useful to do with the answer.
   */
  send(command: Command): void {
    void this.request(command).then((result) => {
      if (!result.ok) this.toast("error", result.error ?? "That did not work.");
      else if (result.note) this.toast("info", result.note);
    });
  }

  /**
   * Sends a command without asking for an answer. For streams of updates
   * while dragging; the engine still reports a failure, shown as a toast.
   */
  quiet(command: Command): void {
    const socket = this.socket;
    if (socket && socket.readyState === WebSocket.OPEN) socket.send(JSON.stringify(command));
  }

  /**
   * Replaces the show with one from a file. A whole show does not fit in
   * a WebSocket message, so this goes over HTTP.
   */
  async importShow(json: string): Promise<CommandResult> {
    try {
      const response = await fetch("/api/import", { method: "POST", body: json });
      if (response.status === 413) return { ok: false, error: "That file is far too large to be a show." };
      return (await response.json()) as CommandResult;
    } catch {
      return { ok: false, error: "Could not reach the engine." };
    }
  }

  toast(kind: Toast["kind"], text: string): void {
    // The engine words its errors as part of a sentence.
    text = text.charAt(0).toUpperCase() + text.slice(1);
    const id = this.nextToast++;
    this.toasts.set([...this.toasts.get().slice(-3), { id, kind, text }]);
    // Long messages stay long enough to be read.
    const reading = Math.min(20000, 3500 + text.length * 55);
    setTimeout(() => this.dismissToast(id), kind === "error" ? Math.max(8000, reading) : reading);
  }

  dismissToast(id: number): void {
    this.toasts.set(this.toasts.get().filter((t) => t.id !== id));
  }
}

export const engine = new Engine();

/**
 * A setting of the interface itself, kept in the browser. It belongs to
 * whoever is looking (a phone used as a remote keeps its own), not to
 * the show.
 */
class Pref<T> {
  private store: Store<T>;
  constructor(
    private key: string,
    fallback: T,
  ) {
    let value = fallback;
    try {
      const saved = localStorage.getItem(key);
      if (saved !== null) value = JSON.parse(saved) as T;
    } catch {
      // Storage can be unavailable or hold something unreadable.
    }
    this.store = new Store(value);
  }
  get = (): T => this.store.get();
  subscribe = (listener: () => void) => this.store.subscribe(listener);
  set(value: T): void {
    this.store.set(value);
    try {
      localStorage.setItem(this.key, JSON.stringify(value));
    } catch {
      // Not being able to remember it is not worth an error.
    }
  }
}

/** Whether advanced settings are shown. Off for a new user. */
export const advancedPref = new Pref<boolean>("darkrgb.advanced", false);
export const useAdvanced = () => useSyncExternalStore(advancedPref.subscribe, advancedPref.get);

/** Building the show, or performing it with the panels out of the way. */
export type View = "edit" | "live";
export const viewPref = new Pref<View>("darkrgb.view", "edit");
export const useView = () => useSyncExternalStore(viewPref.subscribe, viewPref.get);

/** Folders and groups the user has folded shut, by id. */
export const foldedPref = new Pref<number[]>("darkrgb.folded", []);
export const useFolded = () => useSyncExternalStore(foldedPref.subscribe, foldedPref.get);
export function toggleFolded(id: number): void {
  const folded = foldedPref.get();
  foldedPref.set(folded.includes(id) ? folded.filter((f) => f !== id) : [...folded, id]);
}

/** Whether the note about flashing lights has been shown on this device. */
export const flashNoticePref = new Pref<boolean>("darkrgb.flashNotice", false);

/**
 * Says once, the first time something that flashes is added, what the
 * flash limit is and where its switch lives.
 */
export function flashNotice(): void {
  if (flashNoticePref.get()) return;
  flashNoticePref.set(true);
  engine.toast(
    "info",
    "Flashing lights can trigger seizures in people with photosensitive epilepsy. Anything faster than " +
      "3 flashes a second is softened. The switch for this is in Settings, under Lights.",
  );
}

/** Which page of the settings window is open, if it is open at all. */
export type SettingsPage = "general" | "sound" | "lights" | "live" | "show" | "about";
export const settingsPage = new Store<SettingsPage | null>(null);
export const useSettingsPage = () => useSyncExternalStore(settingsPage.subscribe, settingsPage.get);

export const useLive = () => useSyncExternalStore(engine.live.subscribe, engine.live.get);
export const useCurrentScene = () => useSyncExternalStore(engine.currentScene.subscribe, engine.currentScene.get);

export function sameAction(a: Action, b: Action): boolean {
  return a.type === b.type && ("id" in a ? a.id : 0) === ("id" in b ? b.id : 0);
}

export const useConnected = () => useSyncExternalStore(engine.connected.subscribe, engine.connected.get);
export const useHello = () => useSyncExternalStore(engine.hello.subscribe, engine.hello.get);
export const useShow = () => useSyncExternalStore(engine.show.subscribe, engine.show.get);
export const useStatus = () => useSyncExternalStore(engine.status.subscribe, engine.status.get);
export const useToasts = () => useSyncExternalStore(engine.toasts.subscribe, engine.toasts.get);

/** Coalesces rapid calls (a dragged slider) into one per animation frame. */
export function perFrame<T>(run: (value: T) => void): (value: T) => void {
  let latest: T | undefined;
  let scheduled = false;
  return (value: T) => {
    latest = value;
    if (scheduled) return;
    scheduled = true;
    requestAnimationFrame(() => {
      scheduled = false;
      run(latest as T);
    });
  };
}
